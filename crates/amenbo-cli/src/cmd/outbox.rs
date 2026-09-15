//! Carrying a write out around the command. The CLI is a short-lived process, so the observations a
//! command appends to the outbox are carried out at the write seam it makes here.

use amenbo_core::outbox_drive::Face;
use amenbo_core::{activity_log, Store};

use crate::output::{CliError, Flags};

/// How this face re-runs itself as a **notification sender** (`AMB-D-885`): the hidden `notify-sender`
/// command, which core follows with the store to post through. The CLI's own spelling of the entry point,
/// named where it is dispatched.
const NOTIFY_ARGV: &[&str] = &["notify-sender"];

/// The same, for a **Viewer carrier** (`AMB-D-884`): the hidden `viewer-carrier` command, which core
/// follows with the store to carry. The second entry point this face owns the spelling of, named beside the
/// other.
const CARRIER_ARGV: &[&str] = &["viewer-carrier"];

/// Run a mutating command group, then carry the outbox out once at the short-lived CLI's write seam
/// (`AMB-T-2033`). After the command committed, walk the outbox from the persisted cursor, word what the
/// projects report out of it, and hand the messages to a sender process — waiting for none of it, because a
/// sender is not this process's to cut short (`AMB-D-367` / `AMB-D-885`). Only on success: if the command
/// errored its mutation rolled back, so there is nothing new to carry.
///
/// A failure here is a warning, never the command's exit: the mutation is already committed.
pub(crate) fn with_dispatch(
    store: &mut Store,
    op: impl FnOnce(&mut Store) -> Result<i32, CliError>,
) -> Result<i32, CliError> {
    let code = op(store)?;
    // The Viewer is set off beside the drive and not through it: what a carrier carries is the backlog, so
    // which record moved and who moved it decide nothing here — a write happened, and the phone is now
    // behind (`AMB-D-884`). It is a process, so this waits for none of it.
    store.set_the_viewer_off(CARRIER_ARGV);
    if let Err(e) = store.drive_delivery(Face::Cli, NOTIFY_ARGV) {
        eprintln!("warning: could not carry this write out: {e}");
    }
    Ok(code)
}

/// Pick up what a previous run left half carried, before this command does anything of its own
/// (`AMB-D-399`). The CLI's whole life *is* a startup, so this is where a walk that was cut short is
/// noticed — and it is noticed on a read as much as on a write, which is the point: the write that would
/// otherwise carry those rows out may be days away.
///
/// It costs a command with nothing pending one read and no write lock — the guard is core's
/// ([`Store::resume_delivery`]), so both faces make the same judgement.
pub(crate) fn resume_dispatch(store: &Store) {
    if let Err(e) = store.resume_delivery(Face::Cli, NOTIFY_ARGV) {
        eprintln!("warning: could not carry out what a previous run left standing: {e}");
    }
}

/// Carry what a carrier read out and never placed (`AMB-D-884`) — the Viewer's half of the same startup.
///
/// A carrier dies where any process dies: a machine asleep, a session killed, a restart. What it leaves is
/// a queue nobody is coming back for, because the only thing that starts a carrier is a write — so on a
/// device where nobody writes again, the phone goes on showing what it had. This face's whole life is a
/// startup, so this is where those rows are noticed, on a read as much as on a write.
///
/// It costs a command with an empty queue one count and nothing else; what it asks before starting
/// anything, and why it asks in that order, is core's ([`Store::carry_what_was_left_behind`]).
pub(crate) fn resume_the_viewer(store: &Store) {
    store.carry_what_was_left_behind(CARRIER_ARGV);
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
