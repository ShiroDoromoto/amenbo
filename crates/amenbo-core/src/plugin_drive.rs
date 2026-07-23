//! Mounting the single observation-hook dispatcher at the write seam, and owning its cursor
//! (`AMB-D-367`, `AMB-D-380`).
//!
//! [`deliver`] is **pure over its cursor**: it drains the
//! outbox past a cursor and returns the one to store next, holding none of its own. This module is the
//! caller `AMB-D-367` hands that cursor to — the mount. The dispatcher is single, and so is the cursor:
//! both faces read and write the same persisted one ([`drive_persisted`]), and trim is measured against
//! that one (`AMB-D-380`). Where they still differ is only in *what becomes of the fires*:
//!
//! - A **short-lived CLI** **joins** the launched hooks before it exits (the caller's, from
//!   [`Delivered::hooks`]), so a fire it started is not cut short by the process ending.
//! - A **long-lived GUI** **drops** them — true fire-and-forget (`AMB-D-352`). Its process outlives the
//!   fires, so nothing is cut short.
//!
//! One cursor is what makes an event fire exactly once across the two: a session cursor held in the GUI's
//! memory left the events it delivered still standing in the outbox, so the next CLI run delivered them a
//! second time — and an observation hook's effects leave the machine, which makes a double fire a bug the
//! user sees. There is no per-face start position either: the persisted cursor is the single answer to
//! "how far has this store been delivered", and whatever sits past it is undelivered, whichever face
//! launched when.
//!
//! Because both faces drain, the advance is **contention-tolerant**: the cursor is re-read under the write
//! lock and only ever moves forward, so a face that loses the race leaves the winner's position standing
//! and picks up from it on its next drive. The whole advance — cursor, face, and the trim it authorises —
//! rides one transaction of its own, separate from the mutation that triggered the drive: the drive never
//! enlarges that operation, and the persist is skipped when the cursor did not move, so a read command that
//! drives finds nothing and writes nothing.

use crate::error::Result;
use crate::plugin_dispatch::{deliver, Delivered, Subscribers};
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

/// Drive the dispatcher once from the persisted cursor and persist where it advanced to — the mount both
/// faces use (`AMB-D-380`). Reads the stored cursor, fires the subscribers of everything committed since,
/// and writes the advanced cursor back so the next drive, in either face, continues past it (the persist is
/// skipped when the cursor did not move). `face` is stamped beside the cursor for diagnosis
/// ([`CURSOR_FACE_META`]) and changes nothing about what is delivered. The returned [`Delivered`] carries
/// the launched hooks — the CLI **joins** them before it exits, the GUI drops them — and whether a retention
/// gap was hit. `log` is the execution log every run and every gap is recorded in (`AMB-D-361`), passed
/// straight down to [`deliver`]. The cursor is already stored on return.
pub fn drive_persisted(
    engine: &StoreEngine,
    face: Face,
    subs: &dyn Subscribers,
    log: Option<&std::path::Path>,
) -> Result<Delivered> {
    let cursor = persisted_cursor(engine)?;
    let delivered = deliver(engine.conn(), cursor, subs, log)?;
    if delivered.cursor != cursor {
        // Everything through the new cursor is now delivered (or, on a gap resync, jumped past and
        // accepted as lost — either way the dispatcher will never read it again), so the same call that
        // stores the cursor reclaims the outbox through it (`AMB-T-2021`). One cursor is what trim waits
        // on, and it has, by definition, passed `delivered.cursor`.
        advance_cursor(engine, face, delivered.cursor)?;
    }
    Ok(delivered)
}

/// Move the persisted cursor forward to `to`, stamp `face` beside it, and reclaim the outbox through it —
/// all on one transaction. Returns whether the cursor actually moved.
///
/// The cursor is re-read **inside** the transaction, whose `BEGIN IMMEDIATE` holds the write lock from the
/// start: the other face may have drained the same span while this one was firing, so a `to` that is not
/// ahead of what is already stored writes nothing and trims nothing. The loser of that race simply reads
/// the winner's position on its next drive (`AMB-D-380`) — the cursor never goes backwards, which is what
/// keeps a delivered event from being replayed.
fn advance_cursor(engine: &StoreEngine, face: Face, to: i64) -> Result<bool> {
    let tx = engine.write()?;
    let moved = to > persisted_cursor(engine)?;
    if moved {
        tx.set_meta(CURSOR_META, Some(&to.to_string()))?;
        tx.set_meta(CURSOR_FACE_META, Some(face.as_str()))?;
        crate::store_engine::outbox::trim_delivered(tx.conn(), to)?;
    }
    tx.commit()?;
    Ok(moved)
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
        fn resolve(&self, event: &str, _project: Option<i64>) -> Vec<Subscriber> {
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

    /// The advance only ever moves forward: a face that lost the race — the other one having drained the
    /// same span while it was firing — stores nothing and leaves the winner's position standing
    /// (`AMB-D-380`). Replaying a delivered event is exactly what this refuses.
    #[test]
    fn the_cursor_never_moves_backwards() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);
        emit(&e, "task.created", 3);

        assert!(advance_cursor(&e, Face::Gui, 3).unwrap(), "the first advance moves the cursor");
        assert_eq!(persisted_cursor(&e).unwrap(), 3);

        // A slower face finishing behind the winner: no move, and no restamp of the face either.
        assert!(!advance_cursor(&e, Face::Cli, 2).unwrap(), "a cursor behind what is stored does not land");
        assert_eq!(persisted_cursor(&e).unwrap(), 3);
        assert!(!advance_cursor(&e, Face::Cli, 3).unwrap(), "nor does one that merely ties");
        assert_eq!(e.get_meta(CURSOR_FACE_META).unwrap().as_deref(), Some("gui"));
    }
}
