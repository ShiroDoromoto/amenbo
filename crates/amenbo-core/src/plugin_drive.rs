//! Mounting the single observation-hook dispatcher at the write seam, and owning its cursor
//! (`AMB-D-367`, `AMB-T-2033`).
//!
//! [`deliver`] is **pure over its cursor**: it drains the
//! outbox past a cursor and returns the one to store next, holding none of its own. This module is the
//! caller `AMB-D-367` hands that cursor to — the mount. The dispatcher is single, but two faces drive the
//! same pure function differently, and the difference is entirely in *where the cursor lives* and *what
//! becomes of the fires*:
//!
//! - A **short-lived CLI** persists the cursor in the store between runs ([`drive_persisted`]): read the
//!   stored cursor, deliver, write the advanced cursor back. Without persistence a fresh process restarts
//!   at `0` every run and, once a plugin is enabled, re-fires the whole backlog each time. It then **joins**
//!   the launched hooks before it exits (the caller's, from [`Delivered::hooks`]), so a fire it started is
//!   not cut short by the process ending.
//! - A **long-lived GUI** keeps the cursor in memory and calls
//!   [`deliver`] straight (via [`Store::deliver_plugins`](crate::Store::deliver_plugins)), **dropping**
//!   the hooks — true fire-and-forget. Its process outlives the fires, so nothing is cut short and nothing
//!   needs storing between drives. (It starts a session at
//!   [`outbox_head`](crate::store_engine::outbox_head) so it observes what fires *next*, not the backlog
//!   from before it launched.)
//!
//! Persisting the cursor is a write of its own — a bare `store_meta` upsert, outside any mutation's
//! transaction — so it never emits an outbox event and never enlarges the operation that triggered the
//! drive. The persist is skipped when the cursor did not move, so a read command that drives finds nothing
//! and writes nothing.

use crate::error::Result;
use crate::plugin_dispatch::{deliver, Delivered, Subscribers};
use crate::store_engine::StoreEngine;

/// The `store_meta` key the short-lived face persists its dispatch cursor under — the id of the last outbox
/// event a previous run delivered. Distinct from the outbox's retention watermark
/// ([`META_OUTBOX_TRUNCATED_THROUGH`](crate::store_engine::outbox) — that is the producer's low-water mark;
/// this is one consumer's high-water mark) and from the change feed's own cursor (a different consumer,
/// `AMB-D-367`). A store that has never persisted one carries no row, which reads back as `0`.
pub const CURSOR_META: &str = "plugin_dispatch_cursor";

/// Read the persisted dispatch cursor — the id of the last event a previous run delivered, or `0` when none
/// was ever stored (a fresh dispatcher, starting from the bottom of the outbox). A value that does not parse
/// is treated the same as absent: the honest floor is `0`, and a resync on the first drive costs at most a
/// gap the outbox already reports.
pub fn persisted_cursor(engine: &StoreEngine) -> Result<i64> {
    Ok(engine.get_meta(CURSOR_META)?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Drive the dispatcher once from the persisted cursor and persist where it advanced to — the **short-lived
/// (CLI) face** (`AMB-D-367`). Reads the stored cursor, fires the subscribers of everything committed since,
/// and writes the advanced cursor back so the next process continues past it (the persist is skipped when
/// the cursor did not move). The returned [`Delivered`] carries the launched hooks for the caller to **join**
/// before it exits, and whether a retention gap was hit (`AMB-D-361` log). The cursor is already stored on
/// return.
pub fn drive_persisted(engine: &StoreEngine, subs: &dyn Subscribers) -> Result<Delivered> {
    let cursor = persisted_cursor(engine)?;
    let delivered = deliver(engine.conn(), cursor, subs)?;
    if delivered.cursor != cursor {
        engine.set_meta(CURSOR_META, Some(&delivered.cursor.to_string()))?;
        // Everything through the new cursor is now delivered (or, on a gap resync, jumped past and
        // accepted as lost — either way the dispatcher will never read it again), so reclaim it
        // (`AMB-T-2021`). This is the persisted cursor's own retention: the long-lived (GUI) face keeps
        // its cursor in memory and never persists, so the durable dispatcher is the one consumer trim
        // waits on — and it has, by definition, passed `delivered.cursor`. The delete and the watermark
        // that records it must land together (`AMB-D-367`), so they ride one transaction, separate from
        // the bare cursor persist above; trimming touches only the outbox and its watermark, neither
        // drained into the change feed.
        let tx = engine.write()?;
        crate::store_engine::outbox::trim_delivered(tx.conn(), delivered.cursor)?;
        tx.commit()?;
    }
    Ok(delivered)
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
        fn resolve(&self, event: &str) -> Vec<Subscriber> {
            if self.events.contains(&event) {
                vec![Subscriber::new(self.invocation.clone())]
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
        let first = drive_persisted(&e, &subs).unwrap();
        assert_eq!(first.hooks.len(), 2, "both committed events fire on the first run");
        for h in first.hooks {
            h.join().unwrap();
        }
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor is stored at the head");

        // A fresh event, then a second short-lived run: only the new event fires — the first two are behind
        // the persisted cursor.
        emit(&e, "task.created", 3);
        let second = drive_persisted(&e, &subs).unwrap();
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

        let d = drive_persisted(&e, &NoSubscribers).unwrap();
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
        let _ = drive_persisted(&e, &NoSubscribers).unwrap();
        assert_eq!(persisted_cursor(&e).unwrap(), 1);

        // Nothing committed since: the cursor holds, and the drive is a pure read.
        let d = drive_persisted(&e, &NoSubscribers).unwrap();
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

        let d = drive_persisted(&e, &NoSubscribers).unwrap();
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

        let d = drive_persisted(&e, &NoSubscribers).unwrap();
        assert!(d.gapped, "a cursor behind the watermark is a gap");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor resyncs to the head and is persisted");
    }
}
