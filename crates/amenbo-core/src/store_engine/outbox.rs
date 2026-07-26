//! The **plugin observation outbox** — the transactional event log a plugin's dispatcher drains.
//!
//! amenbo fires *semantic* lifecycle events at the edge of a plugin — `task.created`,
//! `task.status_changed`, `comment.added` and their kin (see [`crate::plugin_payload`]). Those events
//! are carried on this outbox, a table separate in every way from the [`change_feed`](super::read):
//!
//! - **Why not the feed.** The change feed is SQLite's `update_hook` reported as `(dataset, row_id,
//!   op)` — no actor, no old value, no new value (`AMB-D-348`). That is enough for the GUI to invalidate
//!   a stale query, but not to fire a plugin event: an `update` splits into six different events (a
//!   status change vs. a completion vs. a reassignment vs. a move) that only the *new state* tells apart,
//!   and every event needs the **actor** the feed structurally cannot hold. The information the split
//!   turns on exists only at the ops write moment (`AMB-D-367`).
//! - **So the write points compose the event.** Unlike the feed — drained from the `update_hook` at
//!   commit — this log is appended *explicitly*: an ops write point builds the semantic event from what
//!   it already holds (its operation kind, the actor, the record's new state) and calls
//!   [`super::write::WriteTx::emit_event`], which appends the row **inside the same transaction**. So the
//!   event lands with the write that caused it, or not at all — generation is leak-free, even though
//!   *delivery* (the dispatcher firing hooks) is best-effort and after the fact (`AMB-D-352`).
//! - **The store interprets none of it.** The columns are opaque strings and ids: this module writes
//!   and reads them, and never classifies. Which event name a change is, what its new state means, and
//!   which project the record was in (`AMB-D-405`) are all the caller's — the mapping that fills those in
//!   sits above this seam. The project is stamped here rather than read back at delivery because it is a
//!   fact about the moment, not a classification: the record may have moved since, or be gone.
//!
//! The read half is [`events_since`] — a **pure query** with a cursor, the outbox's counterpart to
//! [`super::read::changes_since`]. It is the one thing shared with the feed's shape and for the same
//! reason: a reader that has been away drains in pages, and a cursor that has fallen behind retention is
//! told [`OutboxSlice::Gap`] rather than handed a silent empty page. Each consumer keeps its **own**
//! cursor and reads independently; the query holds no state of its own.
//!
//! **This table is the record of what happened, and nothing else** (`AMB-D-399`). What each plugin still
//! *owes* is its own [queue](super::queue)'s business, and the only reader here is the fan-out that copies
//! rows onto those queues: it drains this log, writes the copies, and trims what it copied, all on one
//! transaction. So reclaiming this table is bounded by the fan-out alone, never by how fast a plugin runs.

use rusqlite::{Connection, OptionalExtension};

use super::engine::{Result, StoreEngineError};
use super::schema::col;
use super::sql::{Delete, Expr, Pred, Select, Sort, Sql};

/// The `store_meta` key recording how far outbox retention has trimmed — the watermark
/// [`events_since`] compares a cursor against to answer [`OutboxSlice::Gap`]. It is the outbox's own
/// key, distinct from the change feed's (`AMB-D-367`: retention here is a *separate policy* — an event
/// survives until it has been fanned out onto the queues of everyone who observes it, not merely until a
/// window slides past it). A store that has never trimmed carries no row, which reads back as `0` — the
/// same answer, said by the absence.
pub(crate) const META_OUTBOX_TRUNCATED_THROUGH: &str = "plugin_outbox_truncated_through";

/// One semantic event to append to the outbox — the fields a fired event carries, minus the payload
/// version constant. The caller (an ops write point) has already classified the change into its event
/// name and read whatever new state the event carries; this is the value it hands the store to append
/// in-transaction. Borrowed throughout: nothing is stored beyond the INSERT, so the row can be built
/// from the caller's own strings without owning them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventRow<'a> {
    /// The event's namespace name, e.g. `task.status_changed` — opaque to the store, dispatched on by a
    /// plugin.
    pub event: &'a str,
    /// The affected record's id — the conversational number a reader knows it by (a task, a decision, a
    /// comment).
    pub record_id: i64,
    /// Who drove the write — the process's actor facet, which the feed cannot hold.
    pub actor: &'a str,
    /// When the write committed, as `2026-07-22T09:00:00Z`.
    pub at: &'a str,
    /// The record's new state, for the events an `update` disambiguates (a status, an assignee facet, a
    /// destination slug); `None` for the events whose name is the whole state.
    pub new_state: Option<&'a str>,
    /// The project the record was in when the event fired (`AMB-D-405`) — the material the fan-out routes
    /// a project-scoped subscription on, resolved once by the caller at the emit door. `None` is a real
    /// answer: a record in no project has none.
    pub project: Option<i64>,
    /// The vanished record's own shape as JSON, for an event whose record is gone by the time anyone
    /// reads it (`AMB-D-407`) — captured by the caller at the emit door, the last instant it exists.
    /// `None` on every other event: a record still there is read back by name, not carried.
    pub record: Option<&'a str>,
}

/// Append one semantic event to the outbox. Runs on the caller's transaction (a `&Transaction` deref-es
/// to `&Connection`), so it lands with the operation's other writes — see
/// [`super::write::WriteTx::emit_event`], the only caller.
pub(super) fn append(conn: &Connection, ev: &EventRow<'_>) -> Result<()> {
    let out = col::plugin_outbox::ALL;
    super::sql::Insert::into(out.table)
        .set(out.event, ev.event)
        .set(out.record_id, ev.record_id)
        .set(out.actor, ev.actor)
        .set(out.at, ev.at)
        .set_opt(out.new_state, ev.new_state)
        .set_opt(out.project, ev.project)
        .set_opt(out.record, ev.record)
        .sql()
        .execute(conn)
        .map(|_| ())
        .map_err(StoreEngineError::from)
}

/// One row of the outbox, as a dispatcher reads it: the cursor value, and the fired event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRow {
    /// The outbox's monotonic id — what a reader passes back as "everything after this".
    pub id: i64,
    /// The event's namespace name (`task.created`, …).
    pub event: String,
    /// The affected record's id.
    pub record_id: i64,
    /// Who drove the write.
    pub actor: String,
    /// When it committed (RFC3339).
    pub at: String,
    /// The new state the event carries, or `None`.
    pub new_state: Option<String>,
    /// The project the record was in when the event fired, as it was stamped (`AMB-D-405`); `None` for a
    /// record in no project, and for a row an older store appended before the column existed.
    pub project: Option<i64>,
    /// The vanished record's shape as JSON (`AMB-D-407`), as it was captured; `None` on an event whose
    /// record is still there to read, and on a deletion from before the column existed.
    pub record: Option<String>,
}

/// What a reader gets back when it asks the outbox for everything after its cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboxSlice {
    /// The events since the cursor, oldest first. `more` says the `limit` cut the page short, so the
    /// reader should come back with the last id it saw.
    Events { rows: Vec<OutboxRow>, more: bool },
    /// **The cursor is gone.** Retention has removed events the reader had not read, so the outbox can no
    /// longer replay them — the fan-out must resync its cursor to the head and accept the gap rather than
    /// read an empty page as "nothing fired". Saying it out loud is the whole reason a trim records a
    /// watermark ([`META_OUTBOX_TRUNCATED_THROUGH`]).
    Gap,
}

/// The outbox after a cursor. `limit` bounds one read, so a dispatcher that has been away drains in
/// pages instead of materialising the whole log; it returns [`OutboxSlice::Gap`] when retention has
/// passed the cursor. A reader with no cursor yet (`after_id = 0`) is in that position by definition
/// once anything has been trimmed, and is told so. Pure: it holds no cursor of its own — the caller does.
pub fn events_since(conn: &Connection, after_id: i64, limit: i64) -> Result<OutboxSlice> {
    let meta = col::store_meta::ALL;
    // The store's scalars are text; the watermark is read back as the integer it was written as. A store
    // that has never trimmed carries no row — the same answer as `0`, said by the absence.
    let mut sel = Select::new();
    let mark = sel.expr::<i64>(format!("CAST({} AS INTEGER)", meta.value.to_sql()));
    let mut sql = Sql::from(&sel, meta.table);
    sql.push_where(Some(&Pred::eq(meta.key, META_OUTBOX_TRUNCATED_THROUGH)));
    let truncated_through: i64 = conn
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| mark.get(r))
        .optional()
        .map_err(StoreEngineError::from)?
        .unwrap_or(0);
    if after_id < truncated_through {
        return Ok(OutboxSlice::Gap);
    }
    let out = col::plugin_outbox::ALL;
    // One row past the page, so "is there more?" costs no second query.
    let mut sel = Select::new();
    let (id, event, record_id, actor, at, new_state, project, record) = (
        sel.col(out.id),
        sel.col(out.event),
        sel.col(out.record_id),
        sel.col(out.actor),
        sel.col(out.at),
        sel.col(out.new_state),
        sel.col(out.project),
        sel.col(out.record),
    );
    let mut page = Sql::from(&sel, out.table);
    page.push_where(Some(&Pred::cmp(out.id, ">", after_id)))
        .order_by([Sort::by(out.id)])
        .limit(limit.saturating_add(1));
    let mut stmt = conn.prepare(page.text()).map_err(StoreEngineError::from)?;
    let mut rows = stmt
        .query_map(rusqlite::params_from_iter(page.params()), |r| {
            Ok(OutboxRow {
                id: id.get(r)?,
                event: event.get(r)?,
                record_id: record_id.get(r)?,
                actor: actor.get(r)?,
                at: at.get(r)?,
                new_state: new_state.get(r)?,
                project: project.get(r)?,
                record: record.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(StoreEngineError::from)?;
    let more = rows.len() as i64 > limit;
    rows.truncate(limit.max(0) as usize);
    Ok(OutboxSlice::Events { rows, more })
}

/// The outbox's newest id — the cursor a dispatcher starts from when it only wants what fires *next*
/// (a fresh install, or a resync after a [`OutboxSlice::Gap`]). `0` on an empty outbox.
pub fn outbox_head(conn: &Connection) -> Result<i64> {
    let out = col::plugin_outbox::ALL;
    let mut sel = Select::new();
    // An aggregate over no rows is `NULL`, and an empty outbox's head is `0`.
    let head = sel.expr::<i64>(format!("COALESCE(MAX({}), 0)", out.id.to_sql()));
    let sql = Sql::from(&sel, out.table);
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| head.get(r))
        .map_err(StoreEngineError::from)
}

/// Reclaim what has been fanned out — the outbox's retention (`AMB-D-367`, `AMB-D-399`). Everything at or
/// below `through` (the fan-out's high-water mark, the persisted [`crate::plugin_drive::CURSOR_META`]) is
/// removed, and the watermark [`events_since`] reads to answer [`OutboxSlice::Gap`] is advanced to
/// `through`. Returns how many rows were removed.
///
/// This is deliberately **not** the change feed's window trim: the feed drops its oldest rows once a
/// count-based window slides past them, consumed or not, because a stale GUI cache re-reads on a gap and
/// loses nothing that matters. An observation event dropped before anyone was offered it is a hook that
/// never fired, so retention here is gated on the *fan-out*, never on age or count — an event survives
/// until it is on the queue of every plugin that observes it, which is precisely what the caller's
/// transaction makes true before it calls this. What happens after that is each queue's business
/// (`AMB-D-399`): a plugin that has not run yet still holds its rows, and this table no longer waits on it.
/// `through <= 0` (a fan-out that copied nothing) trims nothing.
///
/// Runs on the caller's transaction so the delete and the watermark land together or not at all: a
/// watermark ahead of the rows it claims are gone would turn a live cursor into a false gap; rows gone
/// with no watermark to name them would read back as "nothing fired" and freeze the fan-out. The
/// watermark is written only when a row was actually removed, so a repeat call on an already-trimmed
/// outbox is a pure read.
pub fn trim_fanned_out(conn: &Connection, through: i64) -> Result<usize> {
    if through <= 0 {
        return Ok(0);
    }
    let out = col::plugin_outbox::ALL;
    let removed = Delete::from(out.table)
        .filter(Pred::cmp(out.id, "<=", through))
        .sql()
        .execute(conn)
        .map_err(StoreEngineError::from)?;
    if removed == 0 {
        return Ok(0);
    }
    super::engine::upsert_meta(conn, META_OUTBOX_TRUNCATED_THROUGH, Some(&through.to_string()))
        .map_err(StoreEngineError::from)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::StoreEngine;

    /// Append through a committed transaction, the way an ops write point does.
    fn emit(e: &StoreEngine, ev: &EventRow<'_>) {
        let tx = e.write().unwrap();
        tx.emit_event(ev).unwrap();
        tx.commit().unwrap();
    }

    fn ev<'a>(event: &'a str, id: i64, new_state: Option<&'a str>) -> EventRow<'a> {
        EventRow { event, record_id: id, actor: "ai", at: "2026-07-22T09:00:00Z", new_state, project: None, record: None }
    }

    /// The same event with a project stamped on it — what the emit door composes for a record that lives
    /// in one.
    fn ev_in<'a>(event: &'a str, id: i64, project: i64) -> EventRow<'a> {
        EventRow { project: Some(project), ..ev(event, id, None) }
    }

    fn drain_all(e: &StoreEngine) -> Vec<OutboxRow> {
        match events_since(e.conn(), 0, 1000).unwrap() {
            OutboxSlice::Events { rows, more } => {
                assert!(!more, "1000 is past the whole test log");
                rows
            }
            OutboxSlice::Gap => panic!("no trim happened, so no gap"),
        }
    }

    /// An emitted event round-trips: every field the caller composed comes back, and the cursor id is the
    /// outbox's own, not the record's.
    #[test]
    fn emit_round_trips_through_the_read() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, &ev("task.status_changed", 42, Some("in_progress")));

        let rows = drain_all(&e);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.event, "task.status_changed");
        assert_eq!(r.record_id, 42);
        assert_eq!(r.actor, "ai");
        assert_eq!(r.at, "2026-07-22T09:00:00Z");
        assert_eq!(r.new_state.as_deref(), Some("in_progress"));
        assert_eq!(r.id, 1, "the cursor id is the outbox's own, monotonic from 1");
    }

    /// The project the caller stamped round-trips, and an event from no project reads back as `NULL` —
    /// the column the fan-out routes on (`AMB-D-405`) is carried, not derived.
    #[test]
    fn the_stamped_project_round_trips() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, &ev_in("task.created", 7, 3));
        emit(&e, &ev("task.created", 8, None));

        let rows = drain_all(&e);
        assert_eq!(rows[0].project, Some(3));
        assert_eq!(rows[1].project, None, "a record in no project stamps none");
    }

    /// An event whose name is the whole state carries no `new_state`, and it round-trips as `NULL`.
    #[test]
    fn an_event_without_new_state_stores_null() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, &ev("task.created", 7, None));
        assert_eq!(drain_all(&e)[0].new_state, None);
    }

    /// The emit rides the caller's transaction: a rolled-back operation leaves no event, so a plugin
    /// never sees a change that did not commit.
    #[test]
    fn a_rolled_back_operation_emits_no_event() {
        let e = StoreEngine::open_in_memory().unwrap();
        {
            let tx = e.write().unwrap();
            tx.emit_event(&ev("task.created", 1, None)).unwrap();
            // drop without commit — the whole batch, event included, rolls back
        }
        assert_eq!(outbox_head(e.conn()).unwrap(), 0, "no event survives an uncommitted operation");
    }

    /// The outbox is a sibling of the change feed, not fed by it: appending an event does not itself
    /// enter the change feed (a plain table is outside the `update_hook` whitelist), so the two logs
    /// never feed on each other.
    #[test]
    fn emitting_an_event_does_not_touch_the_change_feed() {
        use crate::store_engine::read::change_feed_head;
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, &ev("task.created", 1, None));
        assert_eq!(change_feed_head(e.conn()).unwrap(), 0, "the outbox write is not a change-feed row");
    }

    /// Paging: `limit` bounds one read and `more` says the page was cut short; the cursor walks the log.
    #[test]
    fn reads_page_and_report_more() {
        let e = StoreEngine::open_in_memory().unwrap();
        for i in 1..=5 {
            emit(&e, &ev("comment.added", i, None));
        }

        let OutboxSlice::Events { rows, more } = events_since(e.conn(), 0, 2).unwrap() else {
            panic!("no gap");
        };
        assert_eq!((rows.len(), more), (2, true), "a full page short of the tail says there is more");
        let cursor = rows.last().unwrap().id;

        let OutboxSlice::Events { rows, more } = events_since(e.conn(), cursor, 2).unwrap() else {
            panic!("no gap");
        };
        assert_eq!((rows.len(), more), (2, true));

        let OutboxSlice::Events { rows, more } = events_since(e.conn(), rows.last().unwrap().id, 2).unwrap()
        else {
            panic!("no gap");
        };
        assert_eq!((rows.len(), more), (1, false), "the last partial page says there is no more");
    }

    /// A cursor older than the retention watermark is a gap — the honest answer when trim has removed
    /// events the reader never delivered. (Trim itself is a later policy; the read contract is complete
    /// now, so the watermark is set by hand here to prove it.)
    #[test]
    fn a_cursor_behind_the_watermark_is_a_gap() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, &ev("task.created", 1, None));
        emit(&e, &ev("task.created", 2, None));
        // Pretend retention trimmed through id 1.
        let tx = e.write().unwrap();
        tx.set_meta(META_OUTBOX_TRUNCATED_THROUGH, Some("1")).unwrap();
        tx.commit().unwrap();

        assert_eq!(events_since(e.conn(), 0, 10).unwrap(), OutboxSlice::Gap, "a bare cursor is behind the trim");
        // A cursor at or past the watermark still reads normally.
        let OutboxSlice::Events { rows, .. } = events_since(e.conn(), 1, 10).unwrap() else {
            panic!("a cursor at the watermark is not a gap");
        };
        assert_eq!(rows.len(), 1, "only the event after the watermark remains to read");
    }

    /// Head is the newest id, and `0` on an empty outbox — a fresh dispatcher starts from there and sees
    /// only what fires next.
    #[test]
    fn head_is_the_newest_id_or_zero() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert_eq!(outbox_head(e.conn()).unwrap(), 0, "an empty outbox has head 0");
        emit(&e, &ev("task.created", 1, None));
        emit(&e, &ev("task.created", 2, None));
        assert_eq!(outbox_head(e.conn()).unwrap(), 2);
    }

    /// Trim removes every event through the delivered cursor and records the watermark, so what it cut
    /// reads back as a gap and what it left reads normally — the two halves the retention contract needs
    /// (`AMB-T-2021`).
    #[test]
    fn trim_cuts_through_the_cursor_and_records_the_watermark() {
        let e = StoreEngine::open_in_memory().unwrap();
        for i in 1..=5 {
            emit(&e, &ev("comment.added", i, None));
        }
        // The fan-out copied through id 3.
        assert_eq!(trim_fanned_out(e.conn(), 3).unwrap(), 3, "ids 1..=3 are gone");

        // A cursor behind the cut is a gap; a cursor at the cut reads only what remains.
        assert_eq!(events_since(e.conn(), 0, 10).unwrap(), OutboxSlice::Gap, "the trimmed span is gone");
        let OutboxSlice::Events { rows, more } = events_since(e.conn(), 3, 10).unwrap() else {
            panic!("a cursor at the watermark is not a gap");
        };
        assert_eq!((rows.iter().map(|r| r.id).collect::<Vec<_>>(), more), (vec![4, 5], false));
    }

    /// A dispatcher that never delivered (`through = 0`) trims nothing and records no watermark — the
    /// outbox is untouched and every event still reads.
    #[test]
    fn a_trim_of_a_cursor_that_copied_nothing_is_a_noop() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, &ev("task.created", 1, None));
        emit(&e, &ev("task.created", 2, None));
        assert_eq!(trim_fanned_out(e.conn(), 0).unwrap(), 0, "nothing delivered, nothing trimmed");
        assert_eq!(drain_all(&e).len(), 2, "no watermark, so a bare cursor still reads the whole log");
    }

    /// Re-trimming an already-trimmed outbox removes nothing — the second call is a pure read, so a drive
    /// that advances the cursor by nothing does not churn the watermark.
    #[test]
    fn trimming_twice_is_idempotent() {
        let e = StoreEngine::open_in_memory().unwrap();
        for i in 1..=3 {
            emit(&e, &ev("task.created", i, None));
        }
        assert_eq!(trim_fanned_out(e.conn(), 3).unwrap(), 3);
        assert_eq!(trim_fanned_out(e.conn(), 3).unwrap(), 0, "the rows are already gone");
    }
}
