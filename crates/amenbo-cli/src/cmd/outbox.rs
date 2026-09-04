//! Driving the plugin dispatcher around a write. The CLI is a short-lived process, so the
//! observations a command appends to the outbox are delivered at the write seam it makes here.

use amenbo_core::plugin_drive::Face;
use amenbo_core::plugin_installed;
use amenbo_core::plugin_subscribe::EnabledSubscribers;
use amenbo_core::{activity_log, Store};

use crate::output::{CliError, Flags};

/// How this face re-runs itself as a plugin runner (`AMB-T-2175`): the hidden `plugin-runner` command, which
/// core follows with the plugin, the lease's owner and the store to work. The CLI's own spelling of the
/// entry point, named where it is dispatched.
const RUNNER_ARGV: &[&str] = &["plugin-runner"];

/// Run a mutating command group, then drive the plugin observation dispatcher once at the short-lived
/// CLI's write seam (`AMB-T-2033`). After the command committed, drain the outbox from the persisted
/// cursor onto the subscribed plugins' queues, persist where it advanced, and launch a runner process for
/// each queue nobody is already working — waiting for none of them, because a runner is not this process's
/// to cut short (`AMB-D-367` / `AMB-D-399` / `AMB-T-2175`). Only on success: if the command errored its
/// mutation rolled back, so there is nothing new to dispatch.
///
/// Who fires is [`EnabledSubscribers`]'s answer, over the plugins installed on this machine
/// ([`plugin_installed::installed`]) read once per drive: the resolver is a pure function of the state it
/// is handed, and this mount is what hands it (`AMB-T-2032`). With nothing installed it resolves nobody,
/// and the cursor still walks and persists, so a plugin installed later starts from what fires *next*, not
/// the whole backlog. A dispatch failure is a warning, never the command's exit: the mutation is already
/// committed.
pub(crate) fn with_dispatch(
    store: &mut Store,
    op: impl FnOnce(&mut Store) -> Result<i32, CliError>,
) -> Result<i32, CliError> {
    let code = op(store)?;
    dispatch(store, |store, subs| {
        store.drive_plugins_persisted(Face::Cli, subs, RUNNER_ARGV).map(Some)
    });
    Ok(code)
}

/// Pick up what a previous run left half-delivered, before this command does anything of its own
/// (`AMB-D-399`). The CLI's whole life *is* a startup, so this is where a fan-out or a runner that was cut
/// short is noticed — and it is noticed on a read as much as on a write, which is the point: the write that
/// would otherwise carry those rows out may be days away.
///
/// It costs a command with nothing pending two reads and no write lock — the guard is core's
/// ([`Store::resume_plugin_delivery`]), so both faces make the same judgement.
pub(crate) fn resume_dispatch(store: &Store) {
    dispatch(store, |store, subs| store.resume_plugin_delivery(Face::Cli, subs, RUNNER_ARGV));
}

/// The half both dispatch mounts share: resolve who is installed, hand the resolver to `drive`, and relay
/// whatever came back. Never fails a command — a mutation behind it is already committed, and a startup
/// kick has no command's outcome to speak for.
pub(crate) fn dispatch(
    store: &Store,
    drive: impl FnOnce(
        &Store,
        &dyn amenbo_core::plugin_dispatch::Subscribers,
    ) -> amenbo_core::Result<Option<amenbo_core::plugin_dispatch::Delivered>>,
) {
    // A directory that will not read is not "nothing is installed": drive nothing rather than walk the
    // cursor past events no subscriber was ever offered. The events stay in the outbox, and the next run
    // reads the directory again and delivers them.
    let installed = match plugin_installed::installed(&store.paths) {
        Ok(installed) => installed,
        Err(e) => {
            eprintln!("warning: could not read the installed plugins, so none was dispatched: {e}");
            return;
        }
    };
    let subscribers = EnabledSubscribers::new(&installed, store);
    match drive(store, &subscribers) {
        // A `reply:true` hook (worktree advice, `AMB-D-383`) ran synchronously; relay its stderr to the
        // caller — the AI reads it off this command's stderr and decides, named by the plugin that gave
        // it. The queues are a runner's, and this command waits for none of it (`AMB-T-2175`): a runner
        // is a process, so it is not cut short by this one returning.
        Ok(Some(delivered)) => {
            for reply in &delivered.replies {
                eprintln!("[{}] {}", reply.plugin, reply.stderr.trim_end());
            }
        }
        // Nothing was pending, so nothing was driven.
        Ok(None) => {}
        Err(e) => eprintln!("warning: could not dispatch plugin observation hooks: {e}"),
    }
}

/// Emit a system event into the ledger, under our own facet. Call it after the mutation wrapper has
/// committed. Activity is not the system of record, so a failed write must not fail the command: warn and
/// carry on, erring towards a missing line.
pub(crate) fn emit_event(store: &mut Store, flags: &Flags, target_id: i64, event: serde_json::Value) {
    // Every caller sits behind a mutation, and a mutation declared its facet — so there is always one to
    // record the line under. With none there is no author to name, which this treats the way it treats a
    // failed write: warn, and err towards the missing line.
    let Ok(actor) = flags.facet() else {
        eprintln!("warning: could not record the activity event: no facet was declared");
        return;
    };
    if let Err(e) = store.add_system_event(actor, target_id, event) {
        eprintln!("warning: could not record the activity event: {e}");
    }
}

/// The same, for a line whose subject is a decision rather than a task ([`emit_event`]).
pub(crate) fn emit_decision_event(
    store: &mut Store,
    flags: &Flags,
    decision_id: i64,
    event: serde_json::Value,
) {
    let Ok(actor) = flags.facet() else {
        eprintln!("warning: could not record the activity event: no facet was declared");
        return;
    };
    if let Err(e) = store.add_decision_system_event(actor, decision_id, event) {
        eprintln!("warning: could not record the activity event: {e}");
    }
}

/// The live tasks that just became ready because `blocker_id` stopped blocking them; empty if the read
/// fails. All this read feeds is the `task.unblocked` activity line — readiness itself is derived from the
/// dependency edges on every query, so a dependent becomes ready whether or not the signal is emitted. So it
/// takes the same stance as [`emit_event`]: never fail the command, warn, and err towards the missing line
/// (activity is not the system of record). What may be dropped is the line, not the fact of the failure.
pub(crate) fn newly_ready_or_warn(store: &Store, blocker_id: i64) -> Vec<i64> {
    store.newly_ready_by(blocker_id).unwrap_or_else(|e| {
        eprintln!("warning: could not tell which tasks this unblocked: {e}");
        Vec::new()
    })
}

/// After blocker `blocker_id` goes done, send `task.unblocked` to every dependent that just became ready.
pub(crate) fn emit_unblocks(store: &mut Store, flags: &Flags, blocker_id: i64) {
    let blocker = blocker_id.to_string();
    for tid in newly_ready_or_warn(store, blocker_id) {
        emit_event(store, flags, tid, activity_log::event::task_unblocked(&blocker));
    }
}
