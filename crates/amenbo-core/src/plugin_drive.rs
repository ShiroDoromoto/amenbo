//! Mounting the single observation-hook dispatcher at the write seam, and owning its cursor
//! (`AMB-D-367`, `AMB-D-380`, `AMB-D-399`).
//!
//! [`fan_out`] is **pure over its cursor**: it drains the outbox past a cursor onto the subscribed plugins'
//! queues and returns the one to store next, holding none of its own. This module is the caller
//! `AMB-D-367` hands that cursor to — the mount. The dispatcher is single, and so is the cursor: both faces
//! read and write the same persisted one ([`drive_persisted`]), and trim is measured against that one
//! (`AMB-D-380`). Where they still differ is only in *what becomes of the fires*:
//!
//! - A **short-lived CLI** **joins** the launched hooks before it exits (the caller's, from
//!   [`Delivered::hooks`]), so a fire it started is not cut short by the process ending.
//! - A **long-lived GUI** **drops** them — true fire-and-forget (`AMB-D-352`). Its process outlives the
//!   fires, so nothing is cut short.
//!
//! One cursor is what makes an event reach a plugin exactly once across the two: a session cursor held in
//! the GUI's memory left the events it delivered still standing in the outbox, so the next CLI run
//! delivered them a second time — and an observation hook's effects leave the machine, which makes a double
//! fire a bug the user sees. There is no per-face start position either: the persisted cursor is the single
//! answer to "how far has this store been fanned out", and whatever sits past it is unqueued, whichever
//! face launched when.
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
use crate::plugin_dispatch::{fan_out, run_queues, Delivered, FannedOut, Subscribers};
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
/// write lock — and needs nothing from it: each plugin's queue says for itself what it still owes.
///
/// `face` selects which subscriptions resolve (`AMB-D-383`) and is stamped beside the cursor for diagnosis
/// ([`CURSOR_FACE_META`]). The returned [`Delivered`] carries the launched hooks — the CLI **joins** them
/// before it exits, the GUI drops them — the replies to surface, and whether a retention gap was hit. `log`
/// is the execution log every run and every gap is recorded in (`AMB-D-361`).
pub fn drive_persisted(
    engine: &StoreEngine,
    face: Face,
    subs: &dyn Subscribers,
    log: Option<&std::path::Path>,
) -> Result<Delivered> {
    let cursor = persisted_cursor(engine)?;
    let fanned = fan_out_persisted(engine, cursor, face, subs, log)?;
    // The transaction is closed by now, so the replying hooks can run: they are synchronous, and holding
    // the write lock across a subprocess is exactly what the queue exists to avoid.
    let replies = crate::plugin_dispatch::run_replies(fanned.replies, log);
    let hooks = run_queues(engine.conn(), subs, log)?;
    Ok(Delivered { cursor: fanned.cursor, hooks, replies, gapped: fanned.gapped })
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

    fn emit(e: &StoreEngine, event: &str, id: i64) {
        let tx = e.write().unwrap();
        tx.emit_event(&EventRow { event, record_id: id, actor: "ai", at: "2026-07-23T09:00:00Z", new_state: None })
            .unwrap();
        tx.commit().unwrap();
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

        // First run: both events fire, and the cursor is persisted at the head.
        let first = drive_persisted(&e, Face::Cli, &subs, None).unwrap();
        assert_eq!(first.hooks.len(), 2, "both committed events fire on the first run");
        for h in first.hooks {
            h.join().unwrap();
        }
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor is stored at the head");

        // A fresh event, then a second short-lived run: only the new event fires — the first two are behind
        // the persisted cursor.
        emit(&e, "task.created", 3);
        let second = drive_persisted(&e, Face::Cli, &subs, None).unwrap();
        assert_eq!(second.hooks.len(), 1, "only what committed since the stored cursor fires");
        for h in second.hooks {
            h.join().unwrap();
        }
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
        let _ = drive_persisted(&e, Face::Gui, &NoSubscribers, None).unwrap();
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

        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None).unwrap();
        assert!(d.hooks.is_empty(), "nobody is installed, so nothing fires");
        assert!(!d.gapped);
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor still walks to the head and is stored");
    }

    /// A drive with nothing new to deliver leaves the stored cursor untouched — no needless `store_meta`
    /// write on a read command that happens to drive.
    #[test]
    fn a_no_op_drive_does_not_rewrite_the_cursor() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        let _ = drive_persisted(&e, Face::Cli, &NoSubscribers, None).unwrap();
        assert_eq!(persisted_cursor(&e).unwrap(), 1);

        // Nothing committed since: the cursor holds, and the drive is a pure read.
        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None).unwrap();
        assert!(d.hooks.is_empty());
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

        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None).unwrap();
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

        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None).unwrap();
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

        // The long-lived face delivers it and drops the hooks, as it does in the GUI.
        let gui = drive_persisted(&e, Face::Gui, &subs, None).unwrap();
        assert_eq!(gui.hooks.len(), 1, "the GUI's drive fires the event");
        assert_eq!(e.get_meta(CURSOR_FACE_META).unwrap().as_deref(), Some("gui"));

        // A short-lived run after it starts from the same stored cursor, so the event is already past.
        let cli = drive_persisted(&e, Face::Cli, &subs, None).unwrap();
        assert!(cli.hooks.is_empty(), "what the other face delivered does not fire a second time");
        assert_eq!(
            e.get_meta(CURSOR_FACE_META).unwrap().as_deref(),
            Some("gui"),
            "a drive that moved nothing does not restamp the face"
        );

        // The next event fires once, on whichever face gets there first.
        emit(&e, "task.created", 2);
        let cli = drive_persisted(&e, Face::Cli, &subs, None).unwrap();
        assert_eq!(cli.hooks.len(), 1);
        for h in cli.hooks {
            h.join().unwrap();
        }
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
        let _ = drive_persisted(&e, Face::Gui, &NoSubscribers, None).unwrap();
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

        // A resolver that subscribes but whose program does not exist: the rows are queued, and the runs
        // fail — which changes nothing about the reclaim, and that is the point.
        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };
        let d = drive_persisted(&e, Face::Cli, &subs, None).unwrap();
        for h in d.hooks {
            h.join().unwrap();
        }

        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor is stored at the fan-out's position");
        assert_eq!(
            crate::store_engine::events_since(e.conn(), 0, 10).unwrap(),
            OutboxSlice::Gap,
            "the outbox is reclaimed through the fan-out, whatever became of the runs"
        );
        assert!(
            queued_for(e.conn(), "fixed", 10).unwrap().is_empty(),
            "and the queue is empty again, the rows having been handed to the plugin"
        );
    }
}
