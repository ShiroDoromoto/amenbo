//! The single observation-hook **dispatcher** — the wiring that turns committed lifecycle events into
//! fired plugin hooks (`AMB-D-367`), in the two layers `AMB-D-399` splits it into.
//!
//! The pieces this joins already exist: ops write points append semantic events to the transactional
//! [`outbox`](crate::store_engine::outbox) (`AMB-D-367`); [`Payload`] is the wire shape a plugin receives
//! (`AMB-D-348`); [`plugin_hooks::fire`](crate::plugin_hooks::fire) launches each hook fire-and-forget,
//! under a timeout, warning on anything but a clean exit (`AMB-D-352`). What sits between them is two
//! steps, not one:
//!
//! 1. **[`fan_out`] — from what happened to what is to do.** It drains the outbox past a cursor, asks a
//!    [`Subscribers`] who observes each event, copies the event onto the
//!    [queue](crate::store_engine::queue::QueueRow) of every plugin that does, and deletes the outbox rows it copied
//!    — all on the caller's transaction, so nothing is copied twice and nothing is reclaimed uncopied.
//! 2. **The runners — from what is to do to what ran.** One per plugin ([`crate::plugin_runner`]) reads its
//!    own queue from the head and takes each row off it; [`hook_for`] is the half that lives here, turning
//!    one row back into that plugin's invocation.
//!
//! The split is what keeps the outbox clear of the slowest plugin: reclaiming it waits on the fan-out,
//! which runs at the store's speed, rather than on whether some plugin's subprocess has run yet. A stalled
//! plugin backs up its own queue and nothing else (`AMB-D-399`).
//!
//! **Stateless, cursor owned by the caller** (`AMB-D-367`). Like [`events_since`], this holds no cursor of
//! its own: [`fan_out`] takes the caller's cursor and returns the one to store next — the cursor being
//! *how far the outbox has been fanned out*, not how far any plugin has got (each queue says that for
//! itself). The single dispatcher keeps *one* cursor, persisted in the store and shared by both faces
//! (`AMB-D-380`); that mounting, and the transaction the fan-out rides, is the caller's
//! ([`crate::plugin_drive`]).
//!
//! **Who subscribes is a seam, asked once.** [`fan_out`] asks a [`Subscribers`] which plugins observe an
//! event; it does not itself know what is installed or enabled. The real resolver is
//! [`EnabledSubscribers`](crate::plugin_subscribe::EnabledSubscribers), which the install≠enable lifecycle
//! supplies (`AMB-T-2032`) and each face mounts over the installed set: only an *enabled*, subscribed
//! plugin is queued (`AMB-D-351`). [`NoSubscribers`] is the empty stand-in for a face that mounts no
//! resolver at all — the fan-out then queues nothing and still advances the cursor. A runner asks the same
//! resolver for one named plugin ([`Subscribers::resolve_one`]), because a queued row carries the plugin's
//! *name*, never the invocation to run it: what a plugin's config resolves to is read when it runs.
//!
//! **Most hooks are fired and forgotten; a reply is not** (`AMB-D-383`). Both halves carry the driving
//! [`Face`], and a subscription fires only on a face it declares — which is why a queued row records the
//! face it was resolved on. A `reply:true` hook — the worktree advice, only ever resolved on the CLI face —
//! is the one exception to fire-and-forget, and the one thing that never joins a queue: its stderr is the
//! answer a caller is waiting on *now*, so [`fan_out`] hands it back on [`FannedOut::replies`] for the
//! caller to run synchronously ([`run_replies`]) once the transaction has committed. Every other hook stays
//! fire-and-forget, its output landing only in the execution log.
//!
//! **Delivery is best-effort** (`AMB-D-352`). Generation is leak-free (the event landed in the same
//! transaction as its cause), but firing is after the fact: a hook that will not spawn, exits non-zero, or
//! overruns its timeout is a warning and nothing more. And if retention (`AMB-T-2021`) has trimmed past the
//! caller's cursor, the lost span cannot be replayed — [`fan_out`] resyncs the cursor to the head and
//! reports [`FannedOut::gapped`] rather than pretend nothing fired.

use std::thread::JoinHandle;

use rusqlite::Connection;

use crate::error::Result;
use crate::plugin_exec::PluginInvocation;
use crate::plugin_hooks::{Hook, REPLY_TIMEOUT};
use crate::plugin_manifest::Face;
use crate::plugin_payload::Payload;
use crate::store_engine::{events_since, outbox_head, queue, OutboxSlice, WriteTx};

/// How many events one [`fan_out`] call drains per page. A dispatcher fired after each write sees one
/// event at a time; this only bounds a catch-up drain after downtime, so it is generous — the page cost is
/// one query, not one process.
const DELIVER_PAGE: i64 = 256;

/// One resolved subscriber: the plugin to fire and its non-secret config to hand it alongside the event.
///
/// The resolver builds the `invocation` with its program and the plugin's **secret** config as environment
/// variables (`AMB-T-2016`, off argv and off logs), and carries the **text** (non-secret) config as
/// `config` — a JSON object the runner places under the payload's `config` key on stdin. The split is the
/// author's `secret` flag's (`AMB-D-356`), resolved by [`plugin_inject`](crate::plugin_inject). The
/// resolver does **not** set the payload event fields; this module composes the whole stdin document, so
/// the payload channel stays the dispatcher's.
///
/// A subscriber is resolved twice over an event's life, and for different questions: the fan-out resolves
/// the *set* to learn whose queues the event joins, and the runner resolves *one by name*
/// ([`Subscribers::resolve_one`]) to learn how to run the row it is holding. Only the name is stored in
/// between — a program path and an injected config are what the store must not keep.
pub struct Subscriber {
    /// The plugin's name, as the installed registry knows it. Carried alongside the invocation because a
    /// program path is not a name: it is what goes on the queue row, and what a warning or an execution log
    /// says ran.
    pub plugin: String,
    /// The plugin to run, with its program and secret env already set. Its stdin is left for the runner.
    pub invocation: PluginInvocation,
    /// The plugin's non-secret config, placed under the payload's `config` key on stdin. Empty when the
    /// plugin has no text settings — the runner then adds no `config` key at all.
    pub config: serde_json::Map<String, serde_json::Value>,
    /// Whether this subscription's output is relayed to the caller (`AMB-D-383`). `true` is the worktree
    /// advice case: [`fan_out`] hands it back rather than queueing it, and [`run_replies`] runs it
    /// **synchronously** under a short bound and carries its stderr back on [`Delivered::replies`]. Only
    /// ever `true` on the CLI face — the resolver already filtered on the driving face, and the validator
    /// pins `reply:true` to `faces:[cli]`, so a GUI drive never resolves a replying subscriber.
    pub reply: bool,
}

impl Subscriber {
    /// A named subscriber with no text config and no reply — an invocation whose stdin is entirely the
    /// event's payload, fired and forgotten.
    pub fn new(plugin: impl Into<String>, invocation: PluginInvocation) -> Self {
        Self { plugin: plugin.into(), invocation, config: serde_json::Map::new(), reply: false }
    }
}

/// A hook's output relayed back to the caller (`AMB-D-383`). A `reply:true` subscription resolved on the
/// CLI face is run synchronously by [`run_replies`]; whatever it wrote to stderr — its advice — is carried
/// here for the caller to surface, beside its provenance (which plugin, which event). This is separate from
/// the execution log the run is also recorded in (`AMB-D-361`): the log is for later diagnosis, this is the
/// reply the caller is waiting on right now.
pub struct Reply {
    /// The plugin whose hook produced the reply, as the installed registry knows it.
    pub plugin: String,
    /// The event that fired the hook.
    pub event: &'static str,
    /// What the hook wrote to stderr — the advice to relay. Never empty (an empty reply is not carried).
    pub stderr: String,
}

/// Resolves which plugins observe an event — the seam the enable lifecycle fills (`AMB-T-1975`).
///
/// Given an event name (one of [`crate::plugin_payload::V1_EVENTS`]) and the project it happened in,
/// return one [`Subscriber`] per enabled, subscribed plugin: its program, whatever the resolver injects
/// alongside (secret config as env vars, `AMB-T-2016`), and its non-secret config for the payload's
/// `config` key. The resolver does **not** set the payload event fields — this module composes the stdin
/// document, so the payload channel stays the dispatcher's. Return an empty vector for an event nobody
/// observes.
///
/// `project` is what makes a project-scoped plugin's switch answerable here (`AMB-D-379`): the dispatcher
/// resolves it from the row it is holding ([`project_of_event`]). `None` means the event's record no longer
/// says which project it belonged to — a deleted task is the ordinary case — and a resolver that needs a
/// project must then fire nothing rather than guess one.
///
/// `face` is the face the subscription is resolved on (`AMB-D-383`): a subscription fires only when its
/// declared `faces` include it, so a `faces:[cli]` hook stays silent on a GUI drive and vice versa. It is
/// also what keeps a reply off the wrong face — `reply:true` is valid only with `faces:[cli]`, so a GUI
/// drive never resolves a replying subscriber.
pub trait Subscribers {
    /// The subscribers to queue for `event`, in `project`, on `face` — the fan-out's question: *who
    /// observes this?*
    fn resolve(&self, event: &str, project: Option<i64>, face: Face) -> Vec<Subscriber>;

    /// The one subscriber named by `plugin`, or `None` when it is no longer one — the runner's question:
    /// *how do I run this row?* A queued row names its plugin and nothing else, so a runner comes back here
    /// for the program and the config to hand it (both of which can have changed since the fan-out, which is
    /// the point of reading them now rather than storing them).
    ///
    /// `None` is an ordinary answer, not a failure: between the fan-out and the run the plugin may have been
    /// disabled, uninstalled, or updated into something that no longer subscribes. The row is then dropped —
    /// delivery is best-effort (`AMB-D-352`), and a plugin that is off must not fire.
    ///
    /// The default answer is the resolved set, narrowed by name; a resolver with a cheaper way to answer for
    /// one plugin may override it.
    fn resolve_one(
        &self,
        plugin: &str,
        event: &str,
        project: Option<i64>,
        face: Face,
    ) -> Option<Subscriber> {
        self.resolve(event, project, face).into_iter().find(|s| s.plugin == plugin)
    }
}

/// The empty resolver: no event has a subscriber. It is what a face with no mount drives, and what the
/// dispatcher's own tests use to exercise the walk alone. A fan-out under it queues nothing but still walks
/// the cursor, so a plugin enabled later observes what fires *next*, not the whole backlog.
pub struct NoSubscribers;

impl Subscribers for NoSubscribers {
    fn resolve(&self, _event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
        Vec::new()
    }
}

/// The result of one delivery pass, as a caller sees it: how far the cursor advanced, the hooks it
/// launched, the replies it gathered, and whether it hit a retention gap. Assembled by the mount
/// ([`crate::plugin_drive`]) out of the two halves below.
///
/// The `hooks` are the launched threads — **join them before a short-lived process exits** (so the hooks
/// it started are not cut short), or **drop them to forget** (the true fire-and-forget a long-lived GUI
/// wants). That choice is the caller's, exactly as [`plugin_hooks::fire`](crate::plugin_hooks::fire)
/// hands it over.
#[must_use = "wait for or drop `runners`, and surface `replies`"]
pub struct Delivered {
    /// The cursor the fan-out reached — the id of the last outbox event copied onto the queues, or the
    /// outbox head when a gap forced a resync. Never moves backwards.
    pub cursor: i64,
    /// The runner threads this drive started — one per plugin whose lease it took (`AMB-D-399`), each
    /// working that plugin's whole queue. A long-lived face drops them; a short-lived one waits for them
    /// with [`wait_for_runners`](Delivered::wait_for_runners) rather than joining, so a plugin that never
    /// returns cannot hold the process open. A `reply:true` hook is in neither — it ran synchronously and
    /// its output is in [`replies`](Delivered::replies).
    pub runners: Vec<JoinHandle<()>>,
    /// Signalled once by each runner as it ends — the bounded half of the wait above. It is a channel and
    /// not a join because a runner is not bounded by anything: waiting on one is a choice the caller can
    /// stop making, which joining does not allow.
    pub finished: std::sync::mpsc::Receiver<()>,
    /// The replies gathered from `reply:true` hooks, in fan-out order (`AMB-D-383`). Each ran synchronously
    /// under [`REPLY_TIMEOUT`], and carries the stderr the caller should surface. Empty on the GUI face,
    /// which never resolves a replying subscriber, and empty whenever no fired hook asked to reply.
    pub replies: Vec<Reply>,
    /// Retention had trimmed past the caller's cursor: the span between it and the head is lost and was
    /// never queued. The cursor is resynced to the head. A caller may log this (`AMB-D-361`); delivery being
    /// best-effort, it is not an error (`AMB-D-352`).
    pub gapped: bool,
}

impl Delivered {
    /// Wait up to `budget` for the runners this drive started, then walk away — what a **short-lived** face
    /// does before it exits (`AMB-D-399`).
    ///
    /// A process about to end has to wait for something, or the runner it just started dies with it and the
    /// events it queued sit until the store is next driven. But it must not wait for *ever*: a runner is
    /// deliberately unbounded — a plugin is no longer killed for being slow — so joining would hand every
    /// command's exit to the slowest plugin installed. The budget is the line between the two. What is still
    /// running when it runs out is left running; the process ends, that runner dies with it, and its rows
    /// stay queued behind a lease that expires — the next drive picks the queue up where this one left it.
    pub fn wait_for_runners(self, budget: std::time::Duration) {
        let deadline = std::time::Instant::now() + budget;
        for _ in 0..self.runners.len() {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if self.finished.recv_timeout(left).is_err() {
                break; // the budget ran out, or every runner is already gone
            }
        }
    }
}

/// What one [`fan_out`] pass moved: how far it read, how much it queued, the replying hooks it could not
/// queue, and whether it hit a retention gap.
#[must_use = "store the cursor, run the replies, and then run the queues"]
pub struct FannedOut {
    /// The cursor to store for the next pass — the id of the last outbox event copied, or the outbox head
    /// when a gap forced a resync. Equal to the cursor passed in when nothing was there to read.
    pub cursor: i64,
    /// How many queue rows were written — the events copied, times the plugins that observe each. `0` with
    /// a moved cursor is the ordinary case of an event nobody subscribes to.
    pub queued: usize,
    /// The `reply:true` hooks, payload already on stdin, for the caller to run **after the transaction
    /// commits** ([`run_replies`]). They are not queued: a reply is the answer a caller is waiting on now,
    /// and a queue is for work that outlives this run (`AMB-D-383`).
    pub replies: Vec<Hook>,
    /// Retention had trimmed past the cursor: nothing was queued for the lost span, and the cursor is
    /// resynced to the head.
    pub gapped: bool,
}

/// **Fan out** the outbox onto the subscribed plugins' queues — the first layer of delivery (`AMB-D-399`).
///
/// Drains every event past `cursor`, asks `subs` who observes it, and writes one queue row per subscriber,
/// then deletes the outbox rows it copied (through [`trim_fanned_out`](crate::store_engine::outbox)). All of
/// it runs on `tx`, the caller's transaction: a copy and the reclaim of what it copied commit together, so
/// no event is written to a queue twice and none is reclaimed before it was copied. The caller stores
/// [`FannedOut::cursor`] on that same transaction, which is what makes the whole step one atom.
///
/// A row this amenbo does not recognise (an event outside the v1 catalog, an unparseable actor/time) is
/// warned about and skipped, and the cursor still walks past it. On a retention gap the cursor is resynced
/// to the head and nothing is queued for the lost span (see [`FannedOut::gapped`]).
///
/// `face` is the face driving this pass (`AMB-D-383`): it is handed to [`Subscribers::resolve`] so only the
/// subscriptions declaring this face are queued, and it is **recorded on each queue row**, so the runner
/// resolves the plugin on the face the subscription was for rather than on whichever face gets to the row.
/// A `reply:true` subscriber is the one thing not queued — it comes back on [`FannedOut::replies`] for the
/// caller to run once this transaction has committed.
///
/// `log` is the execution log (`AMB-D-361`), used here for the one thing this step alone knows: a gap. A gap
/// queues nothing and therefore leaves no run to look at, so the line it writes is the only trace a reader
/// can find that events went undelivered. `None` records nothing, which is what a test of the fan-out itself
/// wants.
pub fn fan_out(
    tx: &WriteTx<'_>,
    cursor: i64,
    subs: &dyn Subscribers,
    face: Face,
    log: Option<&std::path::Path>,
) -> Result<FannedOut> {
    let conn = tx.conn();
    let mut cursor = cursor;
    let mut queued = 0usize;
    let mut replies: Vec<Hook> = Vec::new();
    loop {
        match events_since(conn, cursor, DELIVER_PAGE)? {
            OutboxSlice::Gap => {
                // Retention passed the cursor; the lost events cannot be replayed. Resync to the head and
                // queue nothing for the gap — delivery is best-effort (`AMB-D-352`). A gap can only surface
                // on the first page (the cursor only ever moves forward), so nothing has been written yet.
                // It is recorded here rather than left to the caller: this is where the fact is known, and
                // a silently dropped span is precisely what the log exists to make visible (`AMB-D-361`).
                if let Some(path) = log {
                    crate::plugin_log::record_gap(path);
                }
                return Ok(FannedOut {
                    cursor: outbox_head(conn)?,
                    queued: 0,
                    replies: Vec::new(),
                    gapped: true,
                });
            }
            OutboxSlice::Events { rows, more } => {
                for row in &rows {
                    cursor = row.id;
                    let Some(payload) = Payload::from_outbox_row(row) else {
                        tracing::warn!(
                            event = %row.event,
                            id = row.record_id,
                            "unrecognised plugin outbox event; skipped"
                        );
                        continue;
                    };
                    // Which project the event happened in, read from the record it names — the outbox row
                    // does not carry it (`AMB-D-379` needs it for a project-scoped plugin's gate). A read
                    // that fails is treated as "unknown", the same as a record that has gone: this walk is
                    // best-effort (`AMB-D-352`) and a resolver that needs a project fires nothing without
                    // one.
                    let project = project_of_event(conn, payload.event, payload.id).unwrap_or_else(|e| {
                        tracing::warn!(
                            event = %payload.event,
                            id = payload.id,
                            error = %e,
                            "could not read the event's project"
                        );
                        None
                    });
                    for sub in subs.resolve(payload.event, project, face) {
                        if sub.reply {
                            // A replying hook (CLI-only, `AMB-D-383`) never joins a queue: its stderr is the
                            // advice the caller is waiting on, and a queue is for work that outlives this
                            // run. Its stdin is composed here, where the payload and the resolved config are
                            // both in hand, and the caller runs it once this transaction has committed —
                            // holding the write lock across a subprocess is what queueing exists to avoid.
                            let json = with_config(&payload, sub.config)?;
                            replies.push(Hook::new(
                                sub.plugin,
                                payload.event,
                                sub.invocation.stdin_json(json),
                            ));
                            continue;
                        }
                        // The row is copied as it stands — the store classifies none of these strings — with
                        // the two things the queue adds: whose work it is, and the face it was resolved on.
                        tx.queue_event(&queue::QueuedEvent {
                            plugin: &sub.plugin,
                            face: face.as_str(),
                            event: &row.event,
                            record_id: row.record_id,
                            actor: &row.actor,
                            at: &row.at,
                            new_state: row.new_state.as_deref(),
                        })?;
                        queued += 1;
                    }
                }
                if !more {
                    break;
                }
            }
        }
    }
    // Everything through `cursor` is now on the queue of everyone who observes it, so the outbox is free of
    // it — on this same transaction, so the copy and the reclaim are one atom. This is the whole of what
    // `AMB-D-399` moves off the plugins' critical path: the outbox is reclaimed at the fan-out's speed, not
    // at the slowest plugin's.
    crate::store_engine::outbox::trim_fanned_out(conn, cursor)?;
    Ok(FannedOut { cursor, queued, replies, gapped: false })
}

/// Run the `reply:true` hooks a fan-out could not queue, and collect what they said (`AMB-D-383`).
///
/// Each runs **synchronously**, under [`REPLY_TIMEOUT`], because its stderr is the advice the caller is
/// waiting on. A hook that overruns, will not launch, or says nothing yields no reply and never stalls the
/// pass — the run is still recorded in the execution log (`AMB-D-361`). Call it **after** the fan-out's
/// transaction has committed: nothing here needs the store, and a subprocess under the write lock is what
/// the queue exists to avoid.
pub fn run_replies(hooks: Vec<Hook>, log: Option<&std::path::Path>) -> Vec<Reply> {
    let mut replies = Vec::new();
    for hook in hooks {
        // The event is `&'static str`, so it outlives the borrow of `hook` the run takes.
        let event = hook.event;
        if let Some(stderr) = crate::plugin_hooks::run_reply(&hook, REPLY_TIMEOUT, log) {
            replies.push(Reply { plugin: hook.plugin, event, stderr });
        }
    }
    replies
}

/// The hook one **queued row** runs, or `None` when the row names nothing runnable any more.
///
/// This is the second layer's per-row half (`AMB-D-399`): the row is turned back into the plugin's
/// invocation on the face the fan-out resolved it for, with the event payload and the plugin's own text
/// config on stdin. Taking the row *off* the queue is the runner's ([`crate::plugin_runner`]) — it does that
/// on a transaction of its own, whether or not there is a hook to run.
///
/// A row whose plugin no longer resolves — disabled, uninstalled, or updated out of the subscription — is
/// `None` rather than a hook: what is on a queue is a claim about the past, and the gate is read now. Same
/// for a row this build cannot rebuild (an unknown event or face): it is warned about and dropped, so a row
/// nobody can run never blocks the ones behind it.
pub fn hook_for(conn: &Connection, subs: &dyn Subscribers, row: &queue::QueueRow) -> Result<Option<Hook>> {
    let (Some(payload), Some(face)) = (Payload::from_queue_row(row), Face::parse(&row.face)) else {
        tracing::warn!(
            plugin = %row.plugin,
            event = %row.event,
            face = %row.face,
            "unrecognised queued plugin event; dropped"
        );
        return Ok(None);
    };
    let project = project_of_event(conn, payload.event, payload.id).unwrap_or_else(|e| {
        tracing::warn!(
            event = %payload.event,
            id = payload.id,
            error = %e,
            "could not read the event's project"
        );
        None
    });
    // Who this plugin is *now*: its program, its config, and whether it still subscribes at all. A
    // subscriber that no longer resolves has been turned off since the fan-out, and a plugin that is off
    // must not fire.
    let Some(sub) = subs.resolve_one(&row.plugin, payload.event, project, face) else {
        tracing::debug!(
            plugin = %row.plugin,
            event = %payload.event,
            "queued for a plugin that no longer subscribes; dropped"
        );
        return Ok(None);
    };
    // Compose this subscriber's stdin: the event payload with the plugin's own text config folded under
    // `config` (`AMB-D-356`). Serialized straight from the typed payload so its declared field order is the
    // wire order — `v` leads (`AMB-D-349`) — rather than round-tripping through a `serde_json::Value`,
    // whose map sorts the keys.
    let json = with_config(&payload, sub.config)?;
    // A `reply:true` subscription resolved here is one the fan-out did not see (the manifest changed in
    // between): there is no caller waiting on this row, so it is run and forgotten like the rest, and what
    // it says lands in the execution log.
    Ok(Some(Hook::new(sub.plugin, payload.event, sub.invocation.stdin_json(json))))
}

/// The project one drained event happened in, or `None` when nothing says so any more.
///
/// The outbox row carries the event's name and the record's id, never a project (`AMB-D-367` — it is a
/// change feed, not a routing table), so the project is read back from the record the event names: a
/// task's own, a decision's own, and for a comment the project of the task it hangs on. This is what lets
/// the resolver answer a **project-scoped** plugin's switch (`AMB-D-379`).
///
/// `None` is a real answer, not a failure: a task that has been deleted takes its project with it, and a
/// task that belongs to no project never had one. A caller that needs a project must fire nothing in that
/// case — guessing a project would open a gate the user never opened there.
pub fn project_of_event(conn: &Connection, event: &str, record_id: i64) -> Result<Option<i64>> {
    use crate::plugin_payload::name as ev;
    use crate::store_engine::read;
    match event {
        ev::TASK_CREATED
        | ev::TASK_STATUS_CHANGED
        | ev::TASK_DONE
        | ev::TASK_ASSIGNED
        | ev::TASK_MOVED
        | ev::TASK_DELETED => Ok(read::task(conn, record_id)?.and_then(|t| t.project_id)),
        ev::DECISION_ACCEPTED | ev::DECISION_REJECTED => {
            Ok(read::decision(conn, record_id)?.map(|d| d.project_id))
        }
        // A comment's project is its task's — the comment table holds no project of its own.
        ev::COMMENT_ADDED => match read::task_comment(conn, record_id)? {
            Some(comment) => Ok(read::task(conn, comment.task_id)?.and_then(|t| t.project_id)),
            None => Ok(None),
        },
        _ => Ok(None),
    }
}

/// Fold a subscriber's non-secret config into the event payload under `config` (`AMB-D-356`), producing the
/// wire JSON a plugin reads on stdin. An empty config adds no key, so a plugin with no text settings
/// receives the bare event payload — the absent and the empty forms stay one wire document, as elsewhere in
/// the plugin contract.
///
/// The payload is serialized straight from its typed form, so its declared field order survives onto the
/// wire — `v` leads (`AMB-D-349`), with `config` appended last. Routing through a `serde_json::Value` would
/// lose that: its map sorts the keys (`v` is not first alphabetically), which is the bug `AMB-T-2084` fixes.
fn with_config(
    payload: &Payload,
    config: serde_json::Map<String, serde_json::Value>,
) -> Result<String> {
    /// The wire document: the event payload's fields inlined (`flatten` streams them in declaration order,
    /// so `v` still leads), then `config` last — dropped entirely when empty.
    #[derive(serde::Serialize)]
    struct Wire<'a> {
        #[serde(flatten)]
        payload: &'a Payload,
        #[serde(skip_serializing_if = "serde_json::Map::is_empty")]
        config: serde_json::Map<String, serde_json::Value>,
    }
    Ok(serde_json::to_string(&Wire { payload, config })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::{outbox::EventRow, StoreEngine};

    /// What one whole pass over a store came to — the test-side counterpart of [`Delivered`], where the
    /// runs are the hooks themselves rather than the threads that would have carried them.
    struct Pass {
        cursor: i64,
        ran: Vec<Hook>,
        replies: Vec<Reply>,
        gapped: bool,
    }

    /// One whole pass over a store: fan out on a transaction, run the replies it handed back, then drain
    /// every queue the way a runner does — what [`crate::plugin_drive::drive_persisted`] does, minus the
    /// persisted cursor and minus the threads. Both halves have to line up for anything to run, which is
    /// what most of these tests are about; the tests that are about one half call [`fan_out`] or
    /// [`hook_for`] directly. Each row is run where it is read, on this thread, so a test that looks at what
    /// a plugin actually received has it by the time it looks. The draining is deliberately not a runner
    /// ([`crate::plugin_runner`]): a runner's own loop, its lease and its leaving are tested where they
    /// live, and what is under test here is which hook a queued row comes back as.
    fn deliver(
        e: &StoreEngine,
        cursor: i64,
        subs: &dyn Subscribers,
        face: Face,
        log: Option<&std::path::Path>,
    ) -> Result<Pass> {
        let tx = e.write()?;
        let fanned = fan_out(&tx, cursor, subs, face, log)?;
        tx.commit()?;
        let replies = run_replies(fanned.replies, log);
        let mut ran = Vec::new();
        for plugin in queue::queued_plugins(e.conn())? {
            for row in queue::queued_for(e.conn(), &plugin, 256)? {
                queue::dequeue(e.conn(), row.id)?;
                if let Some(hook) = hook_for(e.conn(), subs, &row)? {
                    crate::plugin_hooks::run_queued(&hook, log);
                    ran.push(hook);
                }
            }
        }
        Ok(Pass { cursor: fanned.cursor, ran, replies, gapped: fanned.gapped })
    }

    /// Fan out on its own transaction — for the tests that look at what landed on the queues, rather than at
    /// what ran.
    fn fan(e: &StoreEngine, cursor: i64, subs: &dyn Subscribers, face: Face) -> FannedOut {
        let tx = e.write().unwrap();
        let fanned = fan_out(&tx, cursor, subs, face, None).unwrap();
        tx.commit().unwrap();
        fanned
    }

    /// A resolver that fires one fixed invocation for each of the named events, and nothing for the rest.
    struct Fixed {
        events: Vec<&'static str>,
        invocation: PluginInvocation,
    }

    impl Subscribers for Fixed {
        fn resolve(&self, event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
            if self.events.contains(&event) {
                vec![Subscriber::new("fixed", self.invocation.clone())]
            } else {
                Vec::new()
            }
        }
    }

    /// An invocation whose program does not exist: `fire` spawns a thread, the spawn fails and warns, and
    /// the thread ends at once — enough to count fires without depending on a real subprocess.
    fn bogus() -> PluginInvocation {
        PluginInvocation::new("/nonexistent/amenbo-dispatch-test-plugin")
    }

    fn emit(e: &StoreEngine, event: &str, id: i64, new: Option<&str>) {
        let tx = e.write().unwrap();
        tx.emit_event(&EventRow { event, record_id: id, actor: "ai", at: "2026-07-22T09:00:00Z", new_state: new })
            .unwrap();
        tx.commit().unwrap();
    }

    /// With no subscriber, delivery fires nothing but still walks the cursor to the head — so a plugin
    /// enabled later starts from what fires next, not the whole backlog.
    #[test]
    fn no_subscriber_advances_the_cursor_and_fires_nothing() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.status_changed", 2, Some("in_progress"));

        let d = deliver(&e, 0, &NoSubscribers, Face::Cli, None).unwrap();
        assert_eq!(d.cursor, 2, "the cursor walks to the head even with nobody listening");
        assert!(d.ran.is_empty(), "no subscriber, no fire");
        assert!(!d.gapped);
    }

    /// One hook fires per subscribed event; an event nobody subscribed to fires nothing.
    #[test]
    fn fires_one_hook_per_subscribed_event() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.status_changed", 2, Some("in_progress"));
        emit(&e, "task.status_changed", 3, Some("blocked"));

        let subs = Fixed { events: vec!["task.status_changed"], invocation: bogus() };
        let d = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert_eq!(d.cursor, 3);
        assert_eq!(d.ran.len(), 2, "two status_changed events fire, the creation does not");
    }

    /// Delivering again from the returned cursor fires nothing — a committed event is delivered once.
    #[test]
    fn does_not_refire_from_the_returned_cursor() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 1, Some("in_progress"));
        let subs = Fixed { events: vec!["task.status_changed"], invocation: bogus() };

        let first = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert_eq!(first.ran.len(), 1);

        let second = deliver(&e, first.cursor, &subs, Face::Cli, None).unwrap();
        assert_eq!(second.cursor, first.cursor, "nothing new, so the cursor holds");
        assert!(second.ran.is_empty(), "the event already fired once");
    }

    /// A row whose event is outside the v1 catalog is warned about and skipped, but the cursor still walks
    /// past it so it is never revisited.
    #[test]
    fn an_unrecognised_event_is_skipped_and_the_cursor_advances() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.exploded", 2, None); // not a v1 event

        let subs = Fixed { events: vec!["task.created", "task.exploded"], invocation: bogus() };
        let d = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert_eq!(d.cursor, 2, "the cursor walks past the unrecognised row");
        assert_eq!(d.ran.len(), 1, "only the recognised event resolved a subscriber");
    }

    /// A cursor behind the retention watermark is a gap: nothing is fired for the lost span and the cursor
    /// is resynced to the head.
    #[test]
    fn a_retention_gap_resyncs_the_cursor_and_fires_nothing() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.created", 2, None);
        // Pretend retention trimmed through id 1.
        let tx = e.write().unwrap();
        tx.set_meta(crate::store_engine::outbox::META_OUTBOX_TRUNCATED_THROUGH, Some("1")).unwrap();
        tx.commit().unwrap();

        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };
        let d = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert!(d.gapped, "a cursor behind the watermark is a gap");
        assert_eq!(d.cursor, 2, "the cursor resyncs to the head");
        assert!(d.ran.is_empty(), "the lost span is not replayed");
    }

    /// A gap fires nothing, so it leaves no run to look at: the one line it does leave in the execution log
    /// is the only trace a reader can find that events went undelivered (`AMB-D-361`).
    #[test]
    fn a_retention_gap_is_recorded_in_the_execution_log() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        let tx = e.write().unwrap();
        tx.set_meta(crate::store_engine::outbox::META_OUTBOX_TRUNCATED_THROUGH, Some("1")).unwrap();
        tx.commit().unwrap();

        let dir = amenbo_scratch::scratch("dispatch-gap-log");
        let log = dir.join(crate::plugin_log::FILE_NAME);
        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };
        let d = deliver(&e, 0, &subs, Face::Cli, Some(&log)).unwrap();
        assert!(d.gapped);

        let lines = crate::plugin_log::read(&log);
        assert_eq!(lines.len(), 1, "the gap is one line");
        assert_eq!(lines[0].outcome, crate::plugin_log::Outcome::Gap);
        assert_eq!(lines[0].plugin, "", "no plugin ran, so none is named");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `with_config` folds a plugin's text config under `config`, adds no key when the config is empty (so a
    /// plugin with no text settings still receives the bare event payload), and keeps `v` at the head of the
    /// wire in both cases (`AMB-D-349`) — the regression `AMB-T-2084` fixes.
    #[test]
    fn with_config_merges_under_the_config_key_only_when_present() {
        use crate::model::ActorKind;
        use crate::time::Timestamp;
        let at = Timestamp::parse_rfc3339("2026-07-22T09:00:00Z").unwrap();
        let payload = Payload::task_created(7, ActorKind::Ai, at);

        // Empty config: the bare event payload, `v` leading, no `config` key.
        let bare = with_config(&payload, serde_json::Map::new()).unwrap();
        assert!(bare.starts_with(r#"{"v":1,"#), "v leads the wire: {bare}");
        assert!(!bare.contains(r#""config""#), "an empty config adds no key");
        let bare_val: serde_json::Value = serde_json::from_str(&bare).unwrap();
        assert_eq!(bare_val["event"], "task.created");

        // Non-empty config: it lands under `config`, `v` still leads, the event fields untouched.
        let mut cfg = serde_json::Map::new();
        cfg.insert("channel".to_string(), serde_json::Value::String("#ops".to_string()));
        let merged = with_config(&payload, cfg).unwrap();
        assert!(merged.starts_with(r#"{"v":1,"#), "v leads the wire even with config: {merged}");
        let merged_val: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(merged_val["config"]["channel"], "#ops");
        assert_eq!(merged_val["event"], "task.created", "the event fields are left alone");
    }

    /// The whole chain end to end: a committed event's payload reaches the subscribed plugin on stdin, in
    /// the v1 wire shape, with the plugin's text config folded under `config`. A real subprocess needs a
    /// shell, so this is unix-only (the same gate the exec and command round-trip tests use).
    #[cfg(unix)]
    #[test]
    fn the_event_payload_and_config_reach_the_plugin_on_stdin() {
        /// A resolver that fires one invocation with a fixed text config for the named event.
        struct WithConfig {
            event: &'static str,
            invocation: PluginInvocation,
            config: serde_json::Map<String, serde_json::Value>,
        }
        impl Subscribers for WithConfig {
            fn resolve(&self, event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
                if event == self.event {
                    vec![Subscriber { plugin: "configured".into(), invocation: self.invocation.clone(), config: self.config.clone(), reply: false }]
                } else {
                    Vec::new()
                }
            }
        }

        let dir = amenbo_scratch::scratch("plugin-dispatch-config-stdin");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("payload.json");
        let _ = std::fs::remove_file(&out);

        let invocation = PluginInvocation::new("/bin/sh")
            .arg("-c")
            .arg(format!("cat > {}", out.display()));
        let mut config = serde_json::Map::new();
        config.insert("channel".to_string(), serde_json::Value::String("#ops".to_string()));
        let subs = WithConfig { event: "task.status_changed", invocation, config };

        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 42, Some("in_progress"));

        let d = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert_eq!(d.ran.len(), 1);

        let got: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(got["event"], "task.status_changed");
        assert_eq!(got["new"], "in_progress");
        // The plugin's text config rides the same stdin document, under `config`.
        assert_eq!(got["config"]["channel"], "#ops");
    }

    /// The whole chain end to end: a committed event's payload reaches the subscribed plugin on stdin, in
    /// the v1 wire shape. A real subprocess needs a shell, so this is unix-only (the same gate the exec and
    /// command round-trip tests use).
    #[cfg(unix)]
    #[test]
    fn the_event_payload_reaches_the_plugin_on_stdin() {
        let dir = amenbo_scratch::scratch("plugin-dispatch-stdin");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("payload.json");
        let _ = std::fs::remove_file(&out);

        let invocation = PluginInvocation::new("/bin/sh")
            .arg("-c")
            .arg(format!("cat > {}", out.display()));
        let subs = Fixed { events: vec!["task.status_changed"], invocation };

        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 42, Some("in_progress"));

        let d = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert_eq!(d.ran.len(), 1);

        let got: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(got["v"], 1);
        assert_eq!(got["event"], "task.status_changed");
        assert_eq!(got["id"], 42);
        assert_eq!(got["actor"], "ai");
        assert_eq!(got["at"], "2026-07-22T09:00:00Z");
        assert_eq!(got["new"], "in_progress");
    }

    // ───────────────────── the two layers (`AMB-D-399`) ────────────────────────────────────────────

    /// A resolver that subscribes several named plugins to every event — for looking at what a fan-out
    /// writes when more than one plugin observes the same thing.
    struct Many {
        plugins: Vec<&'static str>,
        invocation: PluginInvocation,
    }
    impl Subscribers for Many {
        fn resolve(&self, _event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
            self.plugins.iter().map(|p| Subscriber::new(*p, self.invocation.clone())).collect()
        }
    }

    /// A resolver that subscribes on one face only — the seam for showing that a queued row is resolved
    /// again on the face it was fanned out for.
    struct OnFace {
        face: Face,
        invocation: PluginInvocation,
    }
    impl Subscribers for OnFace {
        fn resolve(&self, _event: &str, _project: Option<i64>, face: Face) -> Vec<Subscriber> {
            if face == self.face {
                vec![Subscriber::new("faceful", self.invocation.clone())]
            } else {
                Vec::new()
            }
        }
    }

    /// The fan-out writes one queue row per subscriber per event, and empties the outbox of what it copied —
    /// the split `AMB-D-399` is for. Nothing has run at this point: the reclaim does not wait on it.
    #[test]
    fn the_fan_out_queues_every_subscriber_and_reclaims_the_outbox() {
        use crate::store_engine::{events_since, queued_for, OutboxSlice};
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.status_changed", 2, Some("in_progress"));

        let subs = Many { plugins: vec!["slack", "email"], invocation: bogus() };
        let fanned = fan(&e, 0, &subs, Face::Cli);
        assert_eq!(fanned.cursor, 2, "the cursor walks to the head of the outbox");
        assert_eq!(fanned.queued, 4, "two events × two subscribers");

        for plugin in ["slack", "email"] {
            let rows = queued_for(e.conn(), plugin, 10).unwrap();
            assert_eq!(
                rows.iter().map(|r| r.event.as_str()).collect::<Vec<_>>(),
                vec!["task.created", "task.status_changed"],
                "{plugin} holds both events, in the order they were fanned out"
            );
            assert_eq!(rows[1].new_state.as_deref(), Some("in_progress"), "the wire fields are copied as they stand");
        }
        assert_eq!(
            events_since(e.conn(), 0, 10).unwrap(),
            OutboxSlice::Gap,
            "the outbox is free of what was copied, though nothing has run yet"
        );
    }

    /// A queued row records the face it was resolved on, and the runner resolves it again on that face —
    /// not on whichever face happens to be draining the queue (`AMB-D-383`). Without it, a CLI-only
    /// subscription fanned out by the CLI could never be run by a GUI that got there first.
    #[test]
    fn a_queued_row_is_run_on_the_face_it_was_fanned_out_for() {
        use crate::store_engine::queued_for;
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);

        let subs = OnFace { face: Face::Cli, invocation: bogus() };
        let _ = fan(&e, 0, &subs, Face::Cli);
        let rows = queued_for(e.conn(), "faceful", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].face, "cli", "the row remembers the face it was resolved on");

        // The same resolver, driven by nobody in particular: the row still resolves, because the face comes
        // from the row.
        assert!(
            hook_for(e.conn(), &subs, &rows[0]).unwrap().is_some(),
            "the CLI-only subscription runs from its queue"
        );
    }

    /// A row whose plugin no longer subscribes — disabled, uninstalled, updated out of it — is dropped
    /// rather than fired: what is queued is a claim about the past, and the gate is read now
    /// (`AMB-D-351`). It leaves the queue either way, so it cannot block the rows behind it.
    #[test]
    fn a_row_whose_plugin_no_longer_subscribes_is_dropped() {
        use crate::store_engine::queued_for;
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        let _ = fan(&e, 0, &Fixed { events: vec!["task.created"], invocation: bogus() }, Face::Cli);
        assert_eq!(queued_for(e.conn(), "fixed", 10).unwrap().len(), 1);

        let row = queued_for(e.conn(), "fixed", 10).unwrap().remove(0);
        assert!(hook_for(e.conn(), &NoSubscribers, &row).unwrap().is_none(), "a plugin that is off does not fire");
        // The runner takes the row off whether or not it came back as a hook, so it cannot block the rows
        // behind it — the drop is what this test is about, the taking is `plugin_runner`'s.
        assert!(queue::dequeue(e.conn(), row.id).unwrap());
        assert!(queued_for(e.conn(), "fixed", 10).unwrap().is_empty(), "and its row does not linger");
    }

    // ───────────────────── where an event happened (`AMB-D-379`) ──────────────────────────────────

    /// Every v1 event's project, read back from the record it names: a task's own, a comment's task's,
    /// a decision's own. This is the only thing that can answer a project-scoped plugin's switch, since
    /// the outbox row itself carries no project.
    #[test]
    fn an_events_project_is_read_from_the_record_it_names() {
        use crate::plugin_payload::name as ev;

        let dir = amenbo_scratch::scratch("dispatch-event-project");
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = crate::Store::open_at(crate::config::Paths::at(dir)).unwrap();
        let project = store
            .project_add(crate::ops::project::NewProject {
                name: "p".into(),
                view: crate::model::View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        let task = store
            .add_task(crate::ops::task::NewTask {
                title: "t".into(),
                project_id: Some(project),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(crate::model::ActorKind::Ai),
            })
            .unwrap()
            .id;
        let comment =
            store.add_task_comment(task, crate::model::ActorKind::Ai, "c").unwrap().id;
        let decision = store
            .add_decision(crate::ops::decision::NewDecision {
                title: "d".into(),
                body: "b".into(),
                project_id: project,
            })
            .unwrap()
            .id;

        let conn = store.engine.conn();
        assert_eq!(project_of_event(conn, ev::TASK_CREATED, task).unwrap(), Some(project));
        assert_eq!(project_of_event(conn, ev::TASK_DONE, task).unwrap(), Some(project));
        assert_eq!(project_of_event(conn, ev::COMMENT_ADDED, comment).unwrap(), Some(project));
        assert_eq!(project_of_event(conn, ev::DECISION_ACCEPTED, decision).unwrap(), Some(project));

        // A record that is gone says nothing — the `task.deleted` case, and the reason `None` is an
        // answer rather than an error.
        assert_eq!(project_of_event(conn, ev::TASK_DELETED, 999_999).unwrap(), None);
        assert_eq!(project_of_event(conn, ev::COMMENT_ADDED, 999_999).unwrap(), None);
        // An event outside the v1 catalog names no record kind at all.
        assert_eq!(project_of_event(conn, "something.else", task).unwrap(), None);
    }

    // ───────────────────── the reply path (`AMB-D-383`) ────────────────────────────────────────────

    /// A resolver that fires one subscriber running `invocation` for the named event, with `reply` set as
    /// given — the seam for exercising the synchronous reply path against a real subprocess.
    struct Replying {
        event: &'static str,
        invocation: PluginInvocation,
        reply: bool,
    }
    impl Subscribers for Replying {
        fn resolve(&self, event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
            if event == self.event {
                vec![Subscriber {
                    plugin: "advisor".into(),
                    invocation: self.invocation.clone(),
                    config: serde_json::Map::new(),
                    reply: self.reply,
                }]
            } else {
                Vec::new()
            }
        }
    }

    /// A `reply:true` hook runs **synchronously** and its stderr comes back on [`Delivered::replies`], named
    /// by the plugin and the event — not launched fire-and-forget, so it is absent from `hooks`. A real
    /// subprocess needs a shell, so this is unix-only (the same gate the stdin round-trip tests use).
    #[cfg(unix)]
    #[test]
    fn a_reply_hook_runs_synchronously_and_its_stderr_is_relayed() {
        let invocation =
            PluginInvocation::new("/bin/sh").arg("-c").arg("echo 'run the worktree' >&2");
        let subs = Replying { event: "task.status_changed", invocation, reply: true };

        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 7, Some("in_progress"));

        let d = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert!(d.ran.is_empty(), "a replying hook ran inline, and never joined a queue");
        assert_eq!(d.replies.len(), 1, "its reply is carried back");
        assert_eq!(d.replies[0].plugin, "advisor");
        assert_eq!(d.replies[0].event, "task.status_changed");
        assert_eq!(d.replies[0].stderr.trim(), "run the worktree");
    }

    /// A `reply:true` hook that writes nothing to stderr yields no reply — an empty reply is not carried, so
    /// the caller has nothing to surface. It still ran (and was logged); it simply had nothing to say.
    #[cfg(unix)]
    #[test]
    fn a_reply_hook_that_says_nothing_carries_no_reply() {
        let invocation = PluginInvocation::new("/bin/sh").arg("-c").arg("true");
        let subs = Replying { event: "task.status_changed", invocation, reply: true };

        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 7, Some("in_progress"));

        let d = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert!(d.replies.is_empty(), "a silent reply hook carries nothing back");
        assert!(d.ran.is_empty(), "and it was not queued as a fire-and-forget one either");
    }

    /// The same hook without `reply` is a fire-and-forget one: it lands in `hooks`, and nothing is relayed —
    /// its stderr goes only to the execution log, as every notification hook's does.
    #[cfg(unix)]
    #[test]
    fn a_non_reply_hook_is_fired_and_forgotten_not_relayed() {
        let invocation =
            PluginInvocation::new("/bin/sh").arg("-c").arg("echo 'not advice' >&2");
        let subs = Replying { event: "task.status_changed", invocation, reply: false };

        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 7, Some("in_progress"));

        let d = deliver(&e, 0, &subs, Face::Cli, None).unwrap();
        assert!(d.replies.is_empty(), "a hook that did not ask to reply relays nothing");
        assert_eq!(d.ran.len(), 1, "it was queued and run as a fire-and-forget one");
    }
}
