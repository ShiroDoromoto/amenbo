//! Mounting the single observation-hook dispatcher at the write seam, and owning its cursor
//! (`AMB-D-367`, `AMB-D-380`, `AMB-D-399`).
//!
//! [`fan_out`] is **pure over its cursor**: it drains the outbox past a cursor onto the subscribed plugins'
//! queues and returns the one to store next, holding none of its own. This module is the caller
//! `AMB-D-367` hands that cursor to — the mount. The dispatcher is single, and so is the cursor: both faces
//! read and write the same persisted one ([`drive_persisted`]), and trim is measured against that one
//! (`AMB-D-380`).
//!
//! **Neither face waits for what it set going, and neither has to** (`AMB-T-2175`). A runner is a process of
//! its own ([`crate::plugin_runner`]), so a short-lived CLI command exiting no longer cuts one short, and a
//! long-lived GUI is not holding one open. What each face supplies is only how to launch itself as one
//! ([`RunnerLauncher`]) — the drive is the same on both.
//!
//! One cursor is what makes an event reach a plugin exactly once across the two: a session cursor held in
//! the GUI's memory left the events it delivered still standing in the outbox, so the next CLI run
//! delivered them a second time — and an observation hook's effects leave the machine, which makes a double
//! fire a bug the user sees. There is no per-face start position either: the persisted cursor is the single
//! answer to "how far has this store been fanned out", and whatever sits past it is unqueued, whichever
//! face launched when.
//!
//! **Two mounts, one drive.** [`drive_persisted`] is the write seam's — what just committed goes out behind
//! it. [`resume_persisted`] is the one every face makes as it starts, for what a *previous* run left half
//! delivered (`AMB-D-399`): the write seam cannot answer for that, since the write it would ride may never
//! come.
//!
//! Because both faces drain, the advance is **contention-tolerant**: the cursor is re-read under the write
//! lock and only ever moves forward, so a face that loses the race leaves the winner's position standing
//! and picks up from it on its next drive. The fan-out, the outbox reclaim it authorises and the cursor
//! that records it ride **one transaction of their own**, separate from the mutation that triggered the
//! drive: the drive never enlarges that operation, and the cursor is not rewritten when it did not move, so
//! a read command that drives finds nothing and writes nothing. Running the queues sits outside that
//! transaction — a subprocess must not be launched under the write lock, and by then each queue holds
//! everything the run needs.

use crate::error::Result;
use crate::plugin_dispatch::{fan_out, Delivered, FannedOut, Subscribers};
use crate::plugin_runner::RunnerLauncher;
use crate::store_engine::StoreEngine;

/// The `store_meta` key the dispatch cursor is persisted under — the id of the last outbox event a drive
/// delivered, shared by both faces (`AMB-D-380`). Distinct from the outbox's retention watermark
/// ([`META_OUTBOX_TRUNCATED_THROUGH`](crate::store_engine::outbox) — that is the producer's low-water mark;
/// this is the consumer's high-water mark) and from the change feed's own cursor (a different consumer,
/// `AMB-D-367`). A store that has never persisted one carries no row, which reads back as `0`.
pub const CURSOR_META: &str = "plugin_dispatch_cursor";

/// The `store_meta` key holding which [`Face`] last advanced [`CURSOR_META`] — written beside the cursor,
/// on the same transaction, so the two never disagree. It is **diagnostic only**: the cursor's meaning does
/// not depend on it, and nothing branches on it. When a double fire or a miss is being chased, it is what
/// says which face delivered a span, to line the store up against this machine's execution log
/// (`AMB-D-361`).
pub const CURSOR_FACE_META: &str = "plugin_dispatch_cursor_face";

/// Which face drove the dispatcher — recorded beside the cursor it advanced (`AMB-D-380`).
///
/// The same [`Face`] a subscription declares it fires on (`AMB-D-383`): one
/// vocabulary, defined with the manifest shape and re-exported here. Recorded beside [`CURSOR_FACE_META`],
/// both faces share one cursor, so as a stamp this is not a selector — it names the driver for a later
/// reader, and nothing more.
pub use crate::plugin_manifest::Face;

/// Read the persisted dispatch cursor — the id of the last event a previous run delivered, or `0` when none
/// was ever stored (a fresh dispatcher, starting from the bottom of the outbox). A value that does not parse
/// is treated the same as absent: the honest floor is `0`, and a resync on the first drive costs at most a
/// gap the outbox already reports.
pub fn persisted_cursor(engine: &StoreEngine) -> Result<i64> {
    Ok(engine.get_meta(CURSOR_META)?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Read the face that last advanced the cursor ([`CURSOR_FACE_META`]), or `None` when nothing has advanced
/// it — a store that has never delivered, or one last driven by a build that did not stamp the face. This is
/// the diagnostic half of the pair: it says who moved the cursor to where it now stands, never whose turn is
/// next (`AMB-D-380` — both faces drive, and nothing branches on this). An unreadable token reads as `None`,
/// the same honest floor an unparsable cursor gets.
pub fn persisted_cursor_face(engine: &StoreEngine) -> Result<Option<Face>> {
    Ok(engine.get_meta(CURSOR_FACE_META)?.as_deref().and_then(Face::parse))
}

/// Drive the dispatcher once from the persisted cursor: fan the outbox out onto the plugins' queues,
/// persist where it advanced to, and run the queues — the mount both faces use (`AMB-D-380`, `AMB-D-399`).
///
/// The first half rides **one transaction**: the fan-out copies each event onto the queue of every plugin
/// that observes it, deletes the outbox rows it copied, and the cursor that says how far it got is stored
/// beside them. Either all three land or none does, so the outbox can never be reclaimed past what was
/// queued, nor queued twice. The second half runs outside it — a subprocess must not be launched under the
/// write lock — and needs nothing from it: each plugin's queue says for itself what it still owes, and the
/// runner reading it holds that plugin's lease, so there is only ever one (`AMB-D-399`).
///
/// `face` selects which subscriptions resolve (`AMB-D-383`) and is stamped beside the cursor for diagnosis
/// ([`CURSOR_FACE_META`]). `launcher` is how this face starts a runner process ([`RunnerLauncher`]); `None`
/// launches none at all — the fan-out still happens and its rows wait on the queues, which is what a caller
/// with no executable to re-run behind it (a test of the walk itself) wants. The returned [`Delivered`]
/// names the runners it launched, carries the replies to surface, and says whether a retention gap was hit;
/// there is nothing in it to wait for (`AMB-T-2175`). `log` is the execution log every run and every gap is
/// recorded in (`AMB-D-361`) — a runner finds it for itself, from the store it is pointed at.
pub fn drive_persisted(
    engine: &StoreEngine,
    face: Face,
    subs: &dyn Subscribers,
    launcher: Option<&dyn RunnerLauncher>,
    log: Option<&std::path::Path>,
) -> Result<Delivered> {
    let cursor = persisted_cursor(engine)?;
    let fanned = fan_out_persisted(engine, cursor, face, subs, log)?;
    // The transaction is closed by now, so the replying hooks can run: they are synchronous, and holding
    // the write lock across a subprocess is exactly what the queue exists to avoid.
    let replies = crate::plugin_dispatch::run_replies(fanned.replies, log);
    let runners = match launcher {
        Some(launcher) => crate::plugin_runner::start(engine, launcher)?,
        None => Vec::new(),
    };
    Ok(Delivered { cursor: fanned.cursor, runners, replies, gapped: fanned.gapped })
}

/// Whether a previous run left delivery half-finished — **read-only, and asked of both layers**
/// (`AMB-D-399`).
///
/// There are two places a run can be cut short, so there are two places to look. A fan-out that died leaves
/// rows in the **outbox** with every queue empty; a runner that died leaves rows on a **queue** with the
/// outbox already reclaimed. Asking only about the queues misses the first kind entirely — nothing is
/// waiting on any queue, so nothing starts, and the events sit there until somebody happens to write.
///
/// An outbox row *is* leftover, without consulting the cursor: the fan-out reclaims what it delivered on the
/// same transaction that queues it, so anything still standing there was never handed on.
pub fn unfinished(engine: &StoreEngine) -> Result<bool> {
    let conn = engine.conn();
    Ok(crate::store_engine::outbox_head(conn)? > 0
        || !crate::store_engine::queued_plugins(conn)?.is_empty())
}

/// Pick up what a previous run left behind, and only then — the **startup kick** both faces make
/// (`AMB-D-399`).
///
/// Startup is the one moment amenbo can catch a delivery nobody is going to trigger again: a run cut short
/// leaves its rows standing, and what would move them next is the next *write*, which may be days away or
/// never — a day spent reading only would leave them there. Driving from here covers both layers at once,
/// since a drive fans the outbox out and then starts a runner for every queue that has work
/// ([`drive_persisted`]).
///
/// It is guarded rather than unconditional because every face reaches this on every start, including the
/// commands that only read: a drive opens a write transaction, and taking the store's write lock to find
/// nothing to do would put every read behind whatever holds it. [`unfinished`] answers from two reads, and
/// `None` is that answer — nothing was pending, so nothing was driven.
pub fn resume_persisted(
    engine: &StoreEngine,
    face: Face,
    subs: &dyn Subscribers,
    launcher: Option<&dyn RunnerLauncher>,
    log: Option<&std::path::Path>,
) -> Result<Option<Delivered>> {
    if !unfinished(engine)? {
        return Ok(None);
    }
    drive_persisted(engine, face, subs, launcher, log).map(Some)
}

/// The transactional half of a drive: fan the outbox out and store where the fan-out reached, on one
/// transaction. Returns what it moved, the replying hooks included — those are the caller's to run once
/// this has committed.
///
/// The cursor is re-read **inside** the transaction, whose `BEGIN IMMEDIATE` holds the write lock from the
/// start: the other face may have fanned the same span out while this one was assembling, so a cursor that
/// is not ahead of what is already stored is not written (the fan-out itself found nothing to copy in that
/// case — it read past the same trimmed rows). The cursor never goes backwards, which is what keeps a
/// queued event from being queued twice (`AMB-D-380`).
fn fan_out_persisted(
    engine: &StoreEngine,
    cursor: i64,
    face: Face,
    subs: &dyn Subscribers,
    log: Option<&std::path::Path>,
) -> Result<FannedOut> {
    let tx = engine.write()?;
    let fanned = fan_out(&tx, cursor, subs, face, log)?;
    if fanned.cursor > persisted_cursor(engine)? {
        tx.set_meta(CURSOR_META, Some(&fanned.cursor.to_string()))?;
        tx.set_meta(CURSOR_FACE_META, Some(face.as_str()))?;
    }
    tx.commit()?;
    Ok(fanned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_dispatch::{NoSubscribers, Subscriber};
    use crate::plugin_exec::PluginInvocation;
    use crate::store_engine::{outbox::EventRow, StoreEngine};

    /// A resolver that fires one fixed invocation for each of the named events.
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
    fn bogus() -> PluginInvocation {
        PluginInvocation::new("/nonexistent/amenbo-drive-test-plugin")
    }

    /// What is standing on a plugin's queue, taken off as it is read — these tests start no runner, so the
    /// draining a runner would do is done here, and each assertion sees only what its own drive queued.
    fn drained(e: &StoreEngine, plugin: &str) -> Vec<crate::store_engine::QueueRow> {
        let rows = crate::store_engine::queued_for(e.conn(), plugin, 100).unwrap();
        for row in &rows {
            crate::store_engine::dequeue(e.conn(), row.id).unwrap();
        }
        rows
    }

    fn emit(e: &StoreEngine, event: &str, id: i64) {
        let tx = e.write().unwrap();
        tx.emit_event(&EventRow {
            event,
            record_id: id,
            actor: "ai",
            at: "2026-07-23T09:00:00Z",
            new_state: None,
            project: None,
            record: None,
        })
            .unwrap();
        tx.commit().unwrap();
    }

    /// A launcher that makes no process and remembers who it was asked for — a start is what these tests
    /// count, and a runner is not theirs to actually run.
    #[derive(Default)]
    struct Launched(std::sync::Mutex<Vec<String>>);
    impl RunnerLauncher for Launched {
        fn launch(&self, plugin: &str, _owner: &str) -> std::io::Result<()> {
            self.0.lock().unwrap().push(plugin.to_string());
            Ok(())
        }
    }

    /// Put one row on a plugin's queue directly — a fan-out's leavings, from before the run that was cut
    /// short.
    fn queue(e: &StoreEngine, plugin: &str, record_id: i64) {
        let tx = e.write().unwrap();
        tx.queue_event(&crate::store_engine::QueuedEvent {
            plugin,
            face: "cli",
            event: "task.created",
            record_id,
            actor: "ai",
            at: "2026-07-23T09:00:00Z",
            new_state: None,
            project: None,
            record: None,
        })
        .unwrap();
        tx.commit().unwrap();
    }

    /// A store with nothing standing on either layer is not driven at all: the startup kick every face makes
    /// answers from reads and takes no write lock, so a read command pays for it and nothing else.
    #[test]
    fn a_start_with_nothing_pending_drives_nothing() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(!unfinished(&e).unwrap(), "a fresh store left nothing behind");

        let launcher = Launched::default();
        let resumed = resume_persisted(&e, Face::Cli, &NoSubscribers, Some(&launcher), None).unwrap();
        assert!(resumed.is_none(), "nothing was pending, so nothing was driven");
        assert_eq!(persisted_cursor(&e).unwrap(), 0, "and no cursor was written");
    }

    /// A fan-out cut short leaves the outbox standing with every queue empty. Looking only at the queues
    /// would find nothing to do and leave those events until somebody happens to write — so the start looks
    /// at the outbox too, and delivers them (`AMB-D-399`).
    #[test]
    fn a_start_resumes_an_outbox_a_previous_fan_out_left_standing() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.deleted", 2);
        assert!(unfinished(&e).unwrap(), "the outbox is holding what was never handed on");

        let subs = Fixed { events: vec!["task.created", "task.deleted"], invocation: bogus() };
        let launcher = Launched::default();
        let resumed = resume_persisted(&e, Face::Cli, &subs, Some(&launcher), None).unwrap();

        assert!(resumed.is_some(), "the start drove");
        assert_eq!(drained(&e, "fixed").len(), 2, "both leftover events reached the subscriber's queue");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "and the cursor is stored at the head");
    }

    /// A runner cut short leaves its queue standing with the outbox already reclaimed — the other half of
    /// `AMB-D-399`'s two layers. The start finds it by the queue alone and launches a runner for it.
    #[test]
    fn a_start_resumes_a_queue_a_previous_runner_left_standing() {
        let e = StoreEngine::open_in_memory().unwrap();
        // Deliver everything first, so the outbox is empty and the queue is the only thing left standing.
        emit(&e, "task.created", 1);
        let _ = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None).unwrap();
        queue(&e, "stalled", 1);
        assert!(unfinished(&e).unwrap(), "a queue with rows is unfinished delivery");

        let launcher = Launched::default();
        let resumed = resume_persisted(&e, Face::Cli, &NoSubscribers, Some(&launcher), None).unwrap();

        assert!(resumed.is_some(), "the start drove");
        assert_eq!(
            launcher.0.lock().unwrap().as_slice(),
            ["stalled"],
            "a runner was launched for the queue nobody was working"
        );
    }

    /// A store that never drove carries no cursor, read back as `0`.
    #[test]
    fn an_unpersisted_cursor_reads_as_zero() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert_eq!(persisted_cursor(&e).unwrap(), 0);
    }

    /// The persisted cursor advances across simulated short-lived runs, so a second run does not re-fire the
    /// first run's events — the whole reason the CLI persists it (`AMB-D-367`).
    #[test]
    fn the_cursor_persists_across_runs_so_events_fire_once() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);
        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };

        // First run: both events are queued for the subscriber, and the cursor is persisted at the head.
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None).unwrap();
        assert_eq!(drained(&e, "fixed").len(), 2, "both committed events are queued on the first run");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor is stored at the head");

        // A fresh event, then a second short-lived run: only the new event is queued — the first two are
        // behind the persisted cursor.
        emit(&e, "task.created", 3);
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None).unwrap();
        assert_eq!(drained(&e, "fixed").len(), 1, "only what committed since the stored cursor is queued");
        assert_eq!(persisted_cursor(&e).unwrap(), 3);
    }

    /// The face read back beside the cursor (`AMB-D-380`): absent until something has been delivered, the
    /// driver's afterwards, and `None` again for a token this build does not know — a stamp it cannot read
    /// is no answer rather than a wrong one. It says who moved the cursor; nothing anywhere chooses on it.
    #[test]
    fn the_cursor_face_reads_back_who_advanced_it_and_nothing_more() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert_eq!(persisted_cursor_face(&e).unwrap(), None, "nothing has delivered from this store");

        emit(&e, "task.created", 1);
        let _ = drive_persisted(&e, Face::Gui, &NoSubscribers, None, None).unwrap();
        assert_eq!(persisted_cursor_face(&e).unwrap(), Some(Face::Gui));

        // A face from a build that knows one this does not: unreadable, so unanswered.
        let tx = e.write().unwrap();
        tx.set_meta(CURSOR_FACE_META, Some("daemon")).unwrap();
        tx.commit().unwrap();
        assert_eq!(persisted_cursor_face(&e).unwrap(), None);
    }

    /// With nothing installed ([`NoSubscribers`]) the drive fires nothing but still walks and persists the
    /// cursor to the head, so a plugin enabled later starts from what fires next, not the whole backlog.
    #[test]
    fn no_subscriber_advances_and_persists_the_cursor() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.status_changed", 2);

        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None).unwrap();
        assert!(d.runners.is_empty(), "nobody is installed, so nothing is queued and nobody runs");
        assert!(!d.gapped);
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor still walks to the head and is stored");
    }

    /// A drive with nothing new to deliver leaves the stored cursor untouched — no needless `store_meta`
    /// write on a read command that happens to drive.
    #[test]
    fn a_no_op_drive_does_not_rewrite_the_cursor() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        let _ = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None).unwrap();
        assert_eq!(persisted_cursor(&e).unwrap(), 1);

        // Nothing committed since: the cursor holds, and the drive is a pure read.
        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None).unwrap();
        assert!(d.runners.is_empty());
        assert_eq!(persisted_cursor(&e).unwrap(), 1, "an empty drive does not move the cursor");
    }

    /// A persisted drive reclaims what it delivered: once the cursor advances, the events through it are
    /// trimmed, so the same run that fired them also frees the space (`AMB-T-2021`). What is delivered is
    /// gone from the outbox, and a cursor behind the new head reads it as the honest gap.
    #[test]
    fn a_persisted_drive_trims_what_it_delivered() {
        use crate::store_engine::outbox::{events_since, OutboxSlice};
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);

        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None).unwrap();
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor walked to the head");
        assert!(!d.gapped);

        // The delivered span is trimmed: a bare cursor is now a gap, and the head cursor reads an empty
        // tail rather than replaying what already fired.
        assert_eq!(events_since(e.conn(), 0, 10).unwrap(), OutboxSlice::Gap, "everything delivered is gone");
        let OutboxSlice::Events { rows, more } = events_since(e.conn(), 2, 10).unwrap() else {
            panic!("a cursor at the head is not a gap");
        };
        assert!(rows.is_empty() && !more, "nothing remains past the delivered head");
    }

    /// A retention gap resyncs the persisted cursor to the head and reports `gapped`, so the lost span is
    /// not replayed and the next run starts clean (`AMB-D-352` / `AMB-D-361`).
    #[test]
    fn a_gap_resyncs_and_persists_the_head() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);
        // Pretend retention trimmed through id 1, and the stored cursor is behind it.
        let tx = e.write().unwrap();
        tx.set_meta(crate::store_engine::outbox::META_OUTBOX_TRUNCATED_THROUGH, Some("1")).unwrap();
        tx.commit().unwrap();

        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None).unwrap();
        assert!(d.gapped, "a cursor behind the watermark is a gap");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor resyncs to the head and is persisted");
    }

    /// The two faces share the one cursor, so what the GUI delivered the CLI does not deliver again — the
    /// double fire `AMB-D-380` closes. The face beside the cursor names whoever moved it last.
    #[test]
    fn one_cursor_spans_both_faces_so_an_event_fires_once() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };

        // The long-lived face delivers it, as it does in the GUI.
        let _ = drive_persisted(&e, Face::Gui, &subs, None, None).unwrap();
        assert_eq!(drained(&e, "fixed").len(), 1, "the GUI's drive queues the event");
        assert_eq!(e.get_meta(CURSOR_FACE_META).unwrap().as_deref(), Some("gui"));

        // A short-lived run after it starts from the same stored cursor, so the event is already past.
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None).unwrap();
        assert!(drained(&e, "fixed").is_empty(), "what the other face delivered is not queued a second time");
        assert_eq!(
            e.get_meta(CURSOR_FACE_META).unwrap().as_deref(),
            Some("gui"),
            "a drive that moved nothing does not restamp the face"
        );

        // The next event is queued once, by whichever face gets there first.
        emit(&e, "task.created", 2);
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None).unwrap();
        assert_eq!(drained(&e, "fixed").len(), 1);
        assert_eq!(e.get_meta(CURSOR_FACE_META).unwrap().as_deref(), Some("cli"), "the face follows the mover");
    }

    /// The stored cursor only ever moves forward: a face that comes back with an older one — the other
    /// having fanned the same span out while this was running — stores nothing and leaves the winner's
    /// position standing (`AMB-D-380`). Queueing an event twice is exactly what this refuses.
    #[test]
    fn the_stored_cursor_never_moves_backwards() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);
        emit(&e, "task.created", 3);

        // The winner fans the whole span out and stores its position.
        let _ = drive_persisted(&e, Face::Gui, &NoSubscribers, None, None).unwrap();
        assert_eq!(persisted_cursor(&e).unwrap(), 3);

        // The loser comes back from the cursor it read before the race: the outbox is empty behind the
        // winner's trim, so it finds nothing to queue and does not write its own position back.
        let _ = fan_out_persisted(&e, 0, Face::Cli, &NoSubscribers, None).unwrap();
        assert_eq!(persisted_cursor(&e).unwrap(), 3, "the winner's position stands");
        assert_eq!(
            e.get_meta(CURSOR_FACE_META).unwrap().as_deref(),
            Some("gui"),
            "and the face is not restamped by a pass that stored nothing"
        );
    }

    /// The two halves are one atom: what the fan-out queued, what it reclaimed from the outbox, and the
    /// cursor that says how far it got all commit together (`AMB-D-399`). A plugin that has not run yet is
    /// what makes the three visible at once — its queue holds the events, and the outbox is already free of
    /// them.
    #[test]
    fn one_transaction_carries_the_queues_the_reclaim_and_the_cursor() {
        use crate::store_engine::{queued_for, OutboxSlice};
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);

        // A resolver that subscribes, with nobody started to run what it queues: the reclaim does not wait
        // on a run, and that is the point.
        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None).unwrap();

        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor is stored at the fan-out's position");
        assert_eq!(
            crate::store_engine::events_since(e.conn(), 0, 10).unwrap(),
            OutboxSlice::Gap,
            "the outbox is reclaimed through the fan-out, whatever became of the runs"
        );
        assert_eq!(
            queued_for(e.conn(), "fixed", 10).unwrap().len(),
            2,
            "and what it was reclaimed against is standing on the plugin's queue"
        );
    }
}
