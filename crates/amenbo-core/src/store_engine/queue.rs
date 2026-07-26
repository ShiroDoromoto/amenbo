//! The **per-plugin work queues** — the second layer of observation-hook delivery (`AMB-D-399`).
//!
//! *What happened* and *what is still to do* are two lists with two lifetimes, so they are two tables. The
//! [`outbox`](super::outbox) is the first: an ops write point appends the semantic event inside the
//! operation's own transaction, which is what makes generation leak-free (`AMB-D-367`). These queues are
//! the second. The **fan-out** joins them — it reads the outbox once, copies each event onto the queue of
//! every plugin subscribed to it, and deletes what it copied, on one transaction, so nothing is copied
//! twice and nothing is dropped uncopied. What each plugin still owes then lives on its own queue:
//!
//! - **The outbox is reclaimed independently of any plugin.** A stalled plugin backs up its own queue and
//!   nothing else.
//! - **Who subscribes is decided once**, at the fan-out. A runner reads its own queue from the head and
//!   never asks whose event this is.
//! - **A row can carry a per-item state.** A cursor is a number, and a number has nowhere to record *this
//!   one failed*. Nothing writes such a state today (a failed event is dropped, by contract), but the shape
//!   is what a retry would need.
//!
//! This module is the table's read/write half and nothing more: it stores the wire fields as the opaque
//! strings the outbox stores, and classifies none of them. Who is subscribed
//! ([`plugin_dispatch::fan_out`](crate::plugin_dispatch::fan_out)) and who runs a queue are above this
//! seam.

use rusqlite::Connection;

use super::engine::{Result, StoreEngineError};
use super::schema::col;
use super::sql::{Delete, Expr, Pred, Select, Sort, Sql};

/// One event to place on a plugin's queue — an outbox row, addressed. The event's own fields come from the
/// outbox row unchanged ([`EventRow`](super::outbox::EventRow)): its wire fields, and the project it was
/// stamped with. What the fan-out adds is the two the split needs: whose queue the row goes on, and the
/// face the subscription resolved on. Borrowed throughout: nothing is kept beyond the INSERT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedEvent<'a> {
    /// The plugin whose queue this row joins, as the installed registry knows it.
    pub plugin: &'a str,
    /// The face the fan-out resolved this subscription on (`AMB-D-383`), stored so the runner can rebuild
    /// the plugin's invocation for this row on the face it was subscribed for — not on whichever face
    /// happens to be running the queue.
    pub face: &'a str,
    /// The event's namespace name, e.g. `task.status_changed` — opaque to the store.
    pub event: &'a str,
    /// The affected record's id.
    pub record_id: i64,
    /// Who drove the write.
    pub actor: &'a str,
    /// When the write committed, as `2026-07-22T09:00:00Z`.
    pub at: &'a str,
    /// The record's new state, for the events an `update` disambiguates; `None` otherwise.
    pub new_state: Option<&'a str>,
    /// The project the event was stamped with when it was appended (`AMB-D-405`), copied off the outbox row
    /// as it stands. The runner resolves the subscription again and needs it to answer a project-scoped
    /// plugin's gate; `None` is a real answer — a record in no project, or a row from before the column.
    pub project: Option<i64>,
    /// The vanished record's shape as JSON, copied off the outbox row (`AMB-D-407`). The runner builds
    /// the payload from this row alone, and for a deletion there is nothing left to read it off.
    pub record: Option<&'a str>,
    /// The id of the record the vanished one hung on, copied off the outbox row (`AMB-D-407`).
    pub parent: Option<i64>,
}

/// One queued row, as a runner reads it: the row's own id (what it passes to [`dequeue`] once the plugin
/// has replied for the event) and the event to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRow {
    /// The queue row's id — the order within a plugin's queue, and the handle [`dequeue`] takes.
    pub id: i64,
    /// The plugin this row is queued for.
    pub plugin: String,
    /// The face the subscription was resolved on.
    pub face: String,
    /// The event's namespace name.
    pub event: String,
    /// The affected record's id.
    pub record_id: i64,
    /// Who drove the write.
    pub actor: String,
    /// When it committed (RFC3339).
    pub at: String,
    /// The new state the event carries, or `None`.
    pub new_state: Option<String>,
    /// The project the event was stamped with (`AMB-D-405`), or `None` when it carries none.
    pub project: Option<i64>,
    /// The vanished record's shape as JSON (`AMB-D-407`), or `None` on an event that carries none.
    pub record: Option<String>,
    /// The id of the record the vanished one hung on (`AMB-D-407`), or `None` when it had none.
    pub parent: Option<i64>,
}

/// Place one event on a plugin's queue. Runs on the caller's transaction — the fan-out's, the same one
/// that deletes the outbox row it copied, so a copy and its reclaim are one atom
/// ([`super::write::WriteTx::queue_event`] is the only caller).
pub(super) fn enqueue(conn: &Connection, ev: &QueuedEvent<'_>) -> Result<()> {
    let q = col::plugin_queue::ALL;
    super::sql::Insert::into(q.table)
        .set(q.plugin, ev.plugin)
        .set(q.face, ev.face)
        .set(q.event, ev.event)
        .set(q.record_id, ev.record_id)
        .set(q.actor, ev.actor)
        .set(q.at, ev.at)
        .set_opt(q.new_state, ev.new_state)
        .set_opt(q.project, ev.project)
        .set_opt(q.record, ev.record)
        .set_opt(q.parent, ev.parent)
        .sql()
        .execute(conn)
        .map(|_| ())
        .map_err(StoreEngineError::from)
}

/// The head of one plugin's queue: its oldest `limit` rows, in the order they were fanned out. A runner
/// reads this, runs each row, and [`dequeue`]s it — so the next read starts at whatever it did not finish.
pub fn queued_for(conn: &Connection, plugin: &str, limit: i64) -> Result<Vec<QueueRow>> {
    let q = col::plugin_queue::ALL;
    let mut sel = Select::new();
    let (id, face, event, record_id, actor, at, new_state, project, record, parent) = (
        sel.col(q.id),
        sel.col(q.face),
        sel.col(q.event),
        sel.col(q.record_id),
        sel.col(q.actor),
        sel.col(q.at),
        sel.col(q.new_state),
        sel.col(q.project),
        sel.col(q.record),
        sel.col(q.parent),
    );
    let mut sql = Sql::from(&sel, q.table);
    sql.push_where(Some(&Pred::eq(q.plugin, plugin))).order_by([Sort::by(q.id)]).limit(limit);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(QueueRow {
                id: id.get(r)?,
                plugin: plugin.to_string(),
                face: face.get(r)?,
                event: event.get(r)?,
                record_id: record_id.get(r)?,
                actor: actor.get(r)?,
                at: at.get(r)?,
                new_state: new_state.get(r)?,
                project: project.get(r)?,
                record: record.get(r)?,
                parent: parent.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// One plugin's queue, counted rather than read: how much it still owes, and since when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDepth {
    /// The plugin the rows are queued for.
    pub plugin: String,
    /// How many events are waiting on it.
    pub waiting: i64,
    /// When the oldest waiting event committed (RFC3339) — how long this queue has been stuck.
    pub oldest: String,
}

/// What every plugin still owes, the one that has waited longest first — the queue layer's whole state,
/// without reading a single event.
///
/// Two callers ask the same question for different reasons. A drive asks *who needs a runner*, and a queue
/// exists because a fan-out put something on it, so the answer is the store's rather than the plugins
/// directory's — a plugin uninstalled since still has its rows named here (whoever runs them resolves it,
/// or drops what no longer resolves). A diagnosis asks *what is piling up and since when*: with a row now
/// leaving only once its plugin has replied (`AMB-D-399`), a backlog is the only place a stopped plugin
/// shows, and the ordering that serves the drive is the one a reader wants too.
pub fn backlog(conn: &Connection) -> Result<Vec<QueueDepth>> {
    let q = col::plugin_queue::ALL;
    let mut sel = Select::new();
    let plugin = sel.col(q.plugin);
    let waiting = sel.count_all();
    // The oldest instant, not the instant of the oldest id: rows are queued in commit order, so the two
    // agree — and `at` is fixed-width UTC, which orders as text exactly as it does as time.
    let oldest = sel.expr::<String>(format!("MIN({})", q.at.to_sql()));
    let mut sql = Sql::from(&sel, q.table);
    // Grouped by plugin and ordered by each group's oldest row: whoever has been waiting longest is run
    // first. The clause is written out because it is an aggregate over the grouping, not a column of the
    // projection.
    sql.push(format!(" GROUP BY {} ORDER BY MIN({}) ASC", q.plugin.to_sql(), q.id.to_sql()));
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(QueueDepth {
                plugin: plugin.get(r)?,
                waiting: waiting.get(r)?,
                oldest: oldest.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// Which plugins have work waiting, oldest queue first — [`backlog`] with the counts dropped, for the drive,
/// which needs the names and nothing else.
pub fn queued_plugins(conn: &Connection) -> Result<Vec<String>> {
    Ok(backlog(conn)?.into_iter().map(|d| d.plugin).collect())
}

/// Throw away what `plugin` has waiting, and say how many rows went — what a plugin being **stopped** costs
/// its queue (`AMB-D-399`). `project` narrows it to the rows stamped with that project; `None` takes every
/// row the plugin holds, whichever project each came from.
///
/// The narrowing is what a per-project switch needs: a project-scoped plugin can be on in one project and
/// off in another (`AMB-D-379`), and turning it off in one is no statement about the other's events. `None`
/// is the whole-plugin stop — a machine-wide switch closing, or an uninstall.
///
/// Dropping rather than keeping is the decision, not an optimisation: a disabled plugin's queue has no
/// condition under which it would ever be worked, so held rows would grow for as long as the plugin is off
/// and then arrive as a storm at a moment when the world they describe has moved on.
pub fn drop_queued(conn: &Connection, plugin: &str, project: Option<i64>) -> Result<usize> {
    let q = col::plugin_queue::ALL;
    let mut filter = Pred::eq(q.plugin, plugin);
    if let Some(project) = project {
        filter = filter.and(Pred::eq(q.project, project));
    }
    Delete::from(q.table).filter(filter).sql().execute(conn).map_err(StoreEngineError::from)
}

/// Remove one queued row — what a runner does once the plugin it was handed to has replied (`AMB-D-399`).
/// Returns whether a row was there to remove, so a double dequeue is visible rather than silent.
pub fn dequeue(conn: &Connection, id: i64) -> Result<bool> {
    let q = col::plugin_queue::ALL;
    Delete::from(q.table)
        .filter(Pred::eq(q.id, id))
        .sql()
        .execute(conn)
        .map(|removed| removed > 0)
        .map_err(StoreEngineError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::StoreEngine;

    /// Queue through a committed transaction, the way the fan-out does.
    fn queue(e: &StoreEngine, plugin: &str, event: &str, id: i64) {
        let tx = e.write().unwrap();
        tx.queue_event(&QueuedEvent {
            plugin,
            face: "cli",
            event,
            record_id: id,
            actor: "ai",
            at: "2026-07-25T09:00:00Z",
            new_state: None,
            project: None,
            record: None,
            parent: None,
        })
        .unwrap();
        tx.commit().unwrap();
    }

    /// A queued event round-trips: every field the fan-out addressed comes back, and the row's id is the
    /// queue's own — the handle a runner dequeues by.
    #[test]
    fn a_queued_event_round_trips() {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.write().unwrap();
        tx.queue_event(&QueuedEvent {
            plugin: "slack",
            face: "gui",
            event: "task.status_changed",
            record_id: 42,
            actor: "ai",
            at: "2026-07-25T09:00:00Z",
            new_state: Some("in_progress"),
            project: Some(3),
            record: None,
            parent: None,
        })
        .unwrap();
        tx.commit().unwrap();

        let rows = queued_for(e.conn(), "slack", 10).unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.id, 1, "the id is the queue's own, monotonic from 1");
        assert_eq!(r.plugin, "slack");
        assert_eq!(r.face, "gui");
        assert_eq!(r.event, "task.status_changed");
        assert_eq!(r.record_id, 42);
        assert_eq!(r.actor, "ai");
        assert_eq!(r.at, "2026-07-25T09:00:00Z");
        assert_eq!(r.new_state.as_deref(), Some("in_progress"));
        assert_eq!(r.project, Some(3), "the project the event was stamped with rides the copy");
    }

    /// A plugin being stopped loses what it had waiting, and nobody else's rows go with it.
    #[test]
    fn dropping_a_queue_takes_that_plugins_rows_and_no_others() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", "task.created", 1);
        queue(&e, "slack", "task.deleted", 2);
        queue(&e, "mail", "task.created", 3);

        let tx = e.write().unwrap();
        assert_eq!(tx.drop_queued("slack", None).unwrap(), 2);
        tx.commit().unwrap();

        assert!(queued_for(e.conn(), "slack", 10).unwrap().is_empty());
        assert_eq!(queued_for(e.conn(), "mail", 10).unwrap().len(), 1, "another plugin's queue stands");
    }

    /// Turning a plugin off in one project takes that project's rows only: the same plugin may still be on
    /// in another (`AMB-D-379`), and its events there were never part of the answer that changed.
    #[test]
    fn dropping_one_projects_share_leaves_the_other_projects_rows() {
        let e = StoreEngine::open_in_memory().unwrap();
        let queue_in = |project: Option<i64>, id: i64| {
            let tx = e.write().unwrap();
            tx.queue_event(&QueuedEvent {
                plugin: "slack",
                face: "cli",
                event: "task.created",
                record_id: id,
                actor: "ai",
                at: "2026-07-25T09:00:00Z",
                new_state: None,
                project,
                record: None,
                parent: None,
            })
            .unwrap();
            tx.commit().unwrap();
        };
        queue_in(Some(1), 1);
        queue_in(Some(2), 2);
        queue_in(None, 3);

        let tx = e.write().unwrap();
        assert_eq!(tx.drop_queued("slack", Some(1)).unwrap(), 1);
        tx.commit().unwrap();

        let left: Vec<_> =
            queued_for(e.conn(), "slack", 10).unwrap().into_iter().map(|r| r.project).collect();
        assert_eq!(left, [Some(2), None], "only the switched-off project's row went");
    }

    /// A row fanned out for an event that carries no project reads back as `NULL` — the answer a
    /// project-scoped subscription fires nothing for, said rather than guessed (`AMB-D-405`).
    #[test]
    fn a_queued_event_from_no_project_round_trips_as_none() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", "task.deleted", 7);
        assert_eq!(queued_for(e.conn(), "slack", 10).unwrap()[0].project, None);
    }

    /// A queue is one plugin's: a read sees its own rows, in the order they were fanned out, and nobody
    /// else's.
    #[test]
    fn a_queue_holds_only_its_own_plugins_rows_oldest_first() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", "task.created", 1);
        queue(&e, "email", "task.created", 2);
        queue(&e, "slack", "task.done", 3);

        let slack = queued_for(e.conn(), "slack", 10).unwrap();
        assert_eq!(
            slack.iter().map(|r| r.record_id).collect::<Vec<_>>(),
            vec![1, 3],
            "slack's own rows, oldest first"
        );
        let email = queued_for(e.conn(), "email", 10).unwrap();
        assert_eq!(email.iter().map(|r| r.record_id).collect::<Vec<_>>(), vec![2]);
        assert!(queued_for(e.conn(), "nobody", 10).unwrap().is_empty(), "an empty queue reads empty");
    }

    /// `limit` bounds one read to the head of the queue, so a runner works in pages instead of
    /// materialising a backlog.
    #[test]
    fn a_read_is_bounded_to_the_head_of_the_queue() {
        let e = StoreEngine::open_in_memory().unwrap();
        for i in 1..=5 {
            queue(&e, "slack", "task.created", i);
        }
        let head = queued_for(e.conn(), "slack", 2).unwrap();
        assert_eq!(head.iter().map(|r| r.record_id).collect::<Vec<_>>(), vec![1, 2]);
    }

    /// Dequeuing removes exactly that row and leaves the rest of the queue standing; removing it twice says
    /// so rather than reporting a second success.
    #[test]
    fn dequeue_removes_one_row_and_reports_whether_it_was_there() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", "task.created", 1);
        queue(&e, "slack", "task.created", 2);

        let head = queued_for(e.conn(), "slack", 10).unwrap()[0].id;
        assert!(dequeue(e.conn(), head).unwrap(), "the row was there");
        assert!(!dequeue(e.conn(), head).unwrap(), "and it is gone the second time");
        assert_eq!(
            queued_for(e.conn(), "slack", 10).unwrap().iter().map(|r| r.record_id).collect::<Vec<_>>(),
            vec![2],
            "the rest of the queue is untouched"
        );
    }

    /// The plugins with work, oldest queue first — the list a drive starts runners from. A plugin whose
    /// last row was dequeued drops off it.
    #[test]
    fn the_plugins_with_work_are_listed_oldest_queue_first() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(queued_plugins(e.conn()).unwrap().is_empty(), "nothing queued, nobody to run");

        queue(&e, "email", "task.created", 1);
        queue(&e, "slack", "task.created", 2);
        queue(&e, "email", "task.done", 3);
        assert_eq!(queued_plugins(e.conn()).unwrap(), vec!["email", "slack"]);

        for row in queued_for(e.conn(), "email", 10).unwrap() {
            dequeue(e.conn(), row.id).unwrap();
        }
        assert_eq!(queued_plugins(e.conn()).unwrap(), vec!["slack"], "an emptied queue is nobody's work");
    }

    /// The backlog counts each queue and dates it by its oldest row — the two facts a diagnosis reads.
    /// The oldest is the earliest instant still waiting, so working the head of a queue moves it forward.
    #[test]
    fn the_backlog_counts_each_queue_and_dates_it_by_its_oldest_row() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(backlog(e.conn()).unwrap().is_empty(), "an empty store owes nothing");

        queue_at(&e, "email", 1, "2026-07-25T09:00:00Z");
        queue_at(&e, "slack", 2, "2026-07-25T09:05:00Z");
        queue_at(&e, "email", 3, "2026-07-25T09:10:00Z");

        let depths = backlog(e.conn()).unwrap();
        assert_eq!(depths.len(), 2);
        assert_eq!(depths[0], QueueDepth { plugin: "email".into(), waiting: 2, oldest: "2026-07-25T09:00:00Z".into() });
        assert_eq!(depths[1], QueueDepth { plugin: "slack".into(), waiting: 1, oldest: "2026-07-25T09:05:00Z".into() });

        let head = queued_for(e.conn(), "email", 1).unwrap();
        dequeue(e.conn(), head[0].id).unwrap();
        let depths = backlog(e.conn()).unwrap();
        assert_eq!(depths[0].plugin, "slack", "email waited less long than slack once its oldest row went");
        let email = depths.iter().find(|d| d.plugin == "email").unwrap();
        assert_eq!((email.waiting, email.oldest.as_str()), (1, "2026-07-25T09:10:00Z"));
    }

    /// Queue one event stamped with `at`, so a test can space a queue out in time.
    fn queue_at(e: &StoreEngine, plugin: &str, id: i64, at: &str) {
        let tx = e.write().unwrap();
        tx.queue_event(&QueuedEvent {
            plugin,
            face: "cli",
            event: "task.created",
            record_id: id,
            actor: "ai",
            at,
            new_state: None,
            project: None,
            record: None,
            parent: None,
        })
        .unwrap();
        tx.commit().unwrap();
    }
}
