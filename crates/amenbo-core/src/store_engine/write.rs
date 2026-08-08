//! The write seam: **one logical operation = one transaction**.
//!
//! [`crate::ops`] mutates the truth source by opening a [`WriteTx`], doing its reads and its UPSERTs
//! through it, and committing. A transaction covers the whole operation, so SQLite's own writer
//! exclusion is enough — the store needs no file lock to serialise writers.
//!
//! What the transaction guards, and what it deliberately does not:
//!
//! - **It guards the body UPSERTs, and only those.** One logical operation touches several rows (the
//!   `task` row plus its placement, its dependency edges, its dimension values …). Partial
//!   application — the task row written, the edges lost — leaves the store inconsistent, so the batch
//!   is all-or-nothing.
//! - **It guards read-then-write.** `add_task` reads the next id then INSERTs; `task move` scans
//!   sibling `order_key`s then UPDATEs. [`WriteTx::conn`] hands out the *same* connection the writes
//!   run on, so the read happens inside the transaction, and [`StoreEngine::transaction`] opens
//!   `BEGIN IMMEDIATE` so the write lock is held from `BEGIN` — no second writer can
//!   read the same high-water mark and take the same number.
//! - **It writes the change feed, in the same transaction.** Every record row the batch touched —
//!   including the ones only SQLite knows about, such as a child a constraint took — is collected by an
//!   `update_hook` and appended to `change_feed` inside the commit. That
//!   in-transaction ordering is the whole point: a committed change always has its feed rows, so the GUI
//!   reading the feed with a cursor can invalidate exactly what went stale instead of re-reading the
//!   store. It is **not an audit log**: the feed says *which rows moved*, for a machine, and
//!   carries no actor, no reason, no values.
//! - **It does not carry the activity append.** activity is a bounded viewing stream, not a system of
//!   record, and its ledger is a file, which cannot join a SQLite transaction. The ordering is settled:
//!   append **after the commit succeeds** — a crash before commit means the row
//!   never appears, a crash after commit means the row is lost. Never duplicated; it falls to the
//!   losing side. This seam offers no hook for it precisely so an event cannot ride inside the
//!   transaction; the caller appends once [`WriteTx::commit`] has returned `Ok`.
//!
//! Failure needs no ceremony: return early with `?` and the guard drops before `commit()`, rolling the
//! whole batch back. That is what keeps a contended `SQLITE_BUSY` mid-batch from leaving a torn row
//! whose unwritten timestamp columns sit at their schema default `''` and break every read store-wide.

use rusqlite::types::Value;
use rusqlite::Connection;

use super::engine::{Result, StoreEngine, StoreEngineError};
use super::schema::col;
use super::sql::Insert;

/// One logical operation's transaction on the truth source. Obtain it with [`StoreEngine::write`],
/// write through it, then [`commit`](Self::commit). Dropping it without committing rolls back. It
/// holds the engine alongside the transaction guard because the write methods run on the engine's
/// connection — the very connection the guard scopes — so a write issued through `WriteTx` is inside
/// the transaction by construction. There is no way to write through this type *outside* it.
pub struct WriteTx<'a> {
    engine: &'a StoreEngine,
    tx: rusqlite::Transaction<'a>,
    /// The projects this operation declared it touches — what [`commit`](Self::commit) stamps its
    /// version onto. A set: the same project named twice is one project, and the order is nobody's.
    /// `RefCell` because the write methods take `&self`, not because two threads reach it.
    projects: std::cell::RefCell<std::collections::BTreeSet<i64>>,
}

impl<'a> WriteTx<'a> {
    /// Open the transaction (`BEGIN IMMEDIATE`). Called by [`StoreEngine::write`].
    fn begin(engine: &'a StoreEngine) -> Result<WriteTx<'a>> {
        let tx = engine.transaction()?;
        // Start with an empty change ledger, so what `commit` writes to the feed is this transaction's
        // rows and nothing else — a rolled-back batch leaves its collected rows behind, and they must not
        // be attributed to the next one.
        engine.take_changes();
        Ok(WriteTx { engine, tx, projects: Default::default() })
    }

    /// Declare that this operation touches `project`, so [`commit`](Self::commit) moves that project's
    /// sync version (`AMB-D-582`). Say it **before** the mutation: a row the batch is about to delete can
    /// still name its project, and one about to be re-homed can still name the project it is leaving.
    ///
    /// The store interprets nothing here — as with the outbox's own `project` (`AMB-D-405`), the caller
    /// is the one that knows. The declaration the write door already makes for the reach guard
    /// (`store::write_reach::WriteTarget`) is where these come from, so there is no second thing for a new
    /// write path to remember.
    pub fn touches_project(&self, project: i64) {
        self.projects.borrow_mut().insert(project);
    }

    /// The connection this transaction runs on — the **read half of a read-then-write**. Every
    /// `store_engine::read` helper takes `&Connection`, so `read::max_task_number(tx.conn())`
    /// reads inside the transaction, under the write lock taken at `BEGIN`.
    pub fn conn(&self) -> &Connection {
        self.engine.conn()
    }

    /// Write one field into its read-model column (registry-validated). See
    /// [`StoreEngine::set_field`].
    pub fn set_field(&self, dataset: &str, row: i64, col: &str, val: Value) -> Result<()> {
        self.engine.set_field(dataset, row, col, val)
    }

    /// Create or update a record as a batch of field writes. See [`StoreEngine::put_record`].
    pub fn put_record(&self, dataset: &str, id: i64, fields: &[(&str, Value)]) -> Result<()> {
        self.engine.put_record(dataset, id, fields)
    }

    /// Physically delete one record row. See [`StoreEngine::delete_record`].
    pub fn delete_record(&self, dataset: &str, id: i64) -> Result<()> {
        self.engine.delete_record(dataset, id)
    }

    /// Upsert a store-level singleton scalar **inside this transaction**. See [`StoreEngine::set_meta`].
    /// The activity sequence's file-only high-water mark rides here, so the id a deletion hands to
    /// the ledger is spent exactly when the deletion commits.
    pub fn set_meta(&self, key: &str, value: Option<&str>) -> Result<()> {
        self.engine.set_meta(key, value)
    }

    /// Physically delete the `attachment` rows of `(target_type, target_id)`. See
    /// [`StoreEngine::delete_records_for_target`].
    pub fn delete_records_for_target(&self, target_type: &str, target_id: i64) -> Result<usize> {
        self.engine.delete_records_for_target(target_type, target_id)
    }

    /// Append one plugin observation event to the outbox **inside this transaction** — the leak-free half
    /// of `AMB-D-367`. The event lands with the write that caused it or, on an earlier `?`/drop, not at
    /// all, so a plugin never sees a change that did not commit. Unlike the change feed (drained from
    /// SQLite's `update_hook` at [`commit`](Self::commit)), the caller *composes* the event: it alone
    /// knows the actor, and — for an `update` — which of the six events the new state names. The store
    /// appends the row it is given and interprets none of its strings. See [`super::outbox`].
    pub fn emit_event(&self, event: &super::outbox::EventRow<'_>) -> Result<()> {
        super::outbox::append(&self.tx, event)
    }

    /// Place one event on a plugin's queue **inside this transaction** — the fan-out's write
    /// (`AMB-D-399`). It rides the same transaction that deletes the outbox rows it copied, so an event is
    /// on every subscriber's queue and off the outbox together, or neither: no copy is made twice, and none
    /// is reclaimed uncopied. As with [`emit_event`](Self::emit_event) the caller composes the row and the
    /// store interprets none of its strings. See [`super::queue`].
    pub fn queue_event(&self, event: &super::queue::QueuedEvent<'_>) -> Result<()> {
        super::queue::enqueue(&self.tx, event)
    }

    /// Take one row off a plugin's queue **inside this transaction** — what a runner does as it hands the
    /// event on (`AMB-D-399`). It rides the same transaction that pushes the runner's lease out, so a runner
    /// that has lost its lease takes nothing. See [`super::queue`].
    pub fn dequeue_event(&self, row: i64) -> Result<bool> {
        super::queue::dequeue(&self.tx, row)
    }

    /// Throw away what is queued for `plugin` — every row, or only those stamped with `project` — **inside
    /// this transaction**, and say how many went (`AMB-D-399`). A stopped plugin's queue and the lease of
    /// whoever is working it go together ([`drop_runner`](Self::drop_runner)), so a stop is one atom: no
    /// runner is left holding a queue that is no longer there. See [`super::queue`].
    pub fn drop_queued(&self, plugin: &str, project: Option<i64>) -> Result<usize> {
        super::queue::drop_queued(&self.tx, plugin, project)
    }

    /// Take `plugin`'s runner lease away whoever holds it, **inside this transaction** — the stop side of
    /// [`release_runner`](Self::release_runner), issued when the plugin itself is being stopped
    /// (`AMB-D-399`). See [`super::runner`].
    pub fn drop_runner(&self, plugin: &str) -> Result<bool> {
        super::runner::drop_lease(&self.tx, plugin)
    }

    /// Take `plugin`'s runner lease for `owner` until `expires_at`, judged against `now` — `true` when it
    /// was taken, `false` when a live lease is already standing (`AMB-D-399`). Claiming **inside this
    /// transaction** is what makes "at most one runner per plugin" hold: the read that finds the lease
    /// absent and the write that takes it are one atom under the write lock, so two drives cannot both find
    /// it free. See [`super::runner`].
    pub fn claim_runner(&self, plugin: &str, owner: &str, expires_at: &str, now: &str) -> Result<bool> {
        super::runner::claim(&self.tx, plugin, owner, expires_at, now)
    }

    /// Push `owner`'s lease on `plugin` out to `expires_at`; `false` when the lease is no longer its own —
    /// it was taken over past its horizon. See [`super::runner`].
    pub fn extend_runner(&self, plugin: &str, owner: &str, expires_at: &str) -> Result<bool> {
        super::runner::extend(&self.tx, plugin, owner, expires_at)
    }

    /// Give up `owner`'s lease on `plugin`; `false` when it was already taken over. Issue it **on the
    /// transaction that read the queue empty** — the pairing is what leaves no gap between "nothing left to
    /// run" and "nobody is running" (`AMB-D-399`). See [`super::runner`].
    pub fn release_runner(&self, plugin: &str, owner: &str) -> Result<bool> {
        super::runner::release(&self.tx, plugin, owner)
    }

    /// Commit the batch. Everything written through this guard lands together; on any earlier `?` the
    /// guard drops and none of it does. Consumes the guard, so a committed transaction cannot be
    /// written to again — and the caller's activity append can only follow this returning `Ok`. **The
    /// change feed is written here, inside this transaction**: the rows SQLite reported to
    /// the `update_hook` while the batch ran are appended to `change_feed` just before the commit. A
    /// committed change therefore always has its feed rows — the feed cannot say less than the truth
    /// source does, which is what a reader keeping a screen current depends on (an append *after* the
    /// commit, which is what the activity ledger does deliberately, can lose the row and leave the
    /// screen wrong). The sync version of every project this operation declared it touches rides the same
    /// drain ([`stamp_project_versions`](Self::stamp_project_versions)), for the same reason.
    pub fn commit(self) -> Result<()> {
        self.write_change_feed()?;
        self.tx.commit().map_err(StoreEngineError::from)
    }

    /// Drain the transaction's collected row changes into `change_feed`. The feed's own INSERTs fire the
    /// hook too; they are filtered out at the source (`change_feed` is not a registry table), so this
    /// cannot feed on itself. **The same `(dataset, row_id, op)` is written once per transaction**: a
    /// record is written column by column ([`StoreEngine::set_field`]), so one `task update` reports the
    /// same row to the hook once per field it touched — five UPDATEs where the reader has one thing to
    /// do, re-read that one task. Collapsing repeats keeps the feed small without losing anything a
    /// reader could act on. Distinct ops are *not* collapsed (an insert followed by a delete of the same
    /// id stays two rows), and the order SQLite reported them in is kept, so "everything after id N"
    /// replays as it happened.
    fn write_change_feed(&self) -> Result<()> {
        let changes = self.engine.take_changes();
        if changes.is_empty() {
            return Ok(());
        }
        let feed = col::change_feed::ALL;
        let mut written = std::collections::HashSet::new();
        for c in &changes {
            if !written.insert((c.dataset, c.row_id, c.op)) {
                continue;
            }
            Insert::into(feed.table)
                .set(feed.dataset, c.dataset)
                .set(feed.row_id, c.row_id)
                .set(feed.op, c.op)
                .sql()
                .execute(&self.tx)
                .map_err(StoreEngineError::from)?;
        }
        self.stamp_project_versions()?;
        self.engine.trim_change_feed_if_due(&self.tx, written.len() as u64)
    }

    /// Move the sync version of every project this operation declared it touches
    /// ([`touches_project`](Self::touches_project)) to the feed id this transaction just reached — the
    /// one number a reader outside the store asks to decide whether to re-send its window (`AMB-D-582`).
    ///
    /// Called from the drain, and so **only when the batch actually wrote a record row**: an operation
    /// that changed nothing leaves the version where it was, which is the whole of "no write, no move".
    /// Taking the feed's own head rather than counting is what keeps the number from ever rewinding, and
    /// keeps a project's version below the store's.
    ///
    /// A project the batch has just deleted is skipped rather than stamped: the row would have no project
    /// to reference, and a version is an answer about a project that is still there. Whoever was watching
    /// it learns it is gone from the store's own version, which moved with this commit.
    fn stamp_project_versions(&self) -> Result<()> {
        let projects = self.projects.borrow();
        if projects.is_empty() {
            return Ok(());
        }
        let version = super::read::change_feed_head(self.conn())?;
        let pv = col::project_version::ALL;
        for &project in projects.iter() {
            if !super::read::record_exists(self.conn(), "project", project)? {
                continue;
            }
            Insert::into(pv.table)
                .set(pv.project_id, project)
                .set(pv.version, version)
                .on_conflict_update(pv.project_id)
                .sql()
                .execute(&self.tx)
                .map_err(StoreEngineError::from)?;
        }
        Ok(())
    }
}

impl StoreEngine {
    /// Open a write transaction: **one logical operation = one transaction**. See [`WriteTx`].
    pub fn write(&self) -> Result<WriteTx<'_>> {
        WriteTx::begin(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }

    fn temp_store(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = amenbo_scratch::scratch(tag);
        let path = dir.join("store.sqlite");
        (dir, path)
    }

    /// A committed batch lands whole.
    #[test]
    fn write_commits_the_whole_batch() {
        let e = StoreEngine::open_in_memory().unwrap();

        let tx = e.write().unwrap();
        tx.put_record("task", 1, &[("title", text("hello")), ("status", text("todo"))]).unwrap();
        // The edge names both tasks, so both exist — the foreign keys on `task_id` /
        // `blocked_by_id` are deferred to commit, so the edge may be written before task 2 within the tx.
        tx.put_record("task", 2, &[("title", text("blocker"))]).unwrap();
        tx.put_record("dependency", 1, &[("task_id", text("1")), ("blocked_by_id", text("2"))])
            .unwrap();
        tx.commit().unwrap();

        let title: String =
            e.conn().query_row("SELECT title FROM task WHERE id=1", [], |r| r.get(0)).unwrap();
        let edges: i64 = e
            .conn()
            .query_row("SELECT count(*) FROM task_dependency WHERE task_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!((title.as_str(), edges), ("hello", 1), "the task row and its dependency edge commit together");
    }

    /// The transaction guards the body UPSERTs. A failure partway — here the `UnknownColumn` a
    /// registry-violating write raises, but equally a contended `SQLITE_BUSY` or a disk error — returns
    /// through `?`, dropping the guard before `commit()`. Nothing partial survives: neither the task row
    /// nor the dependency edge written before the failure.
    #[test]
    fn write_rolls_back_a_partially_applied_operation() {
        let e = StoreEngine::open_in_memory().unwrap();

        let torn: Result<()> = (|| {
            let tx = e.write()?;
            tx.set_field("task", 1, "title", text("hello"))?;
            tx.set_field("dependency", 1, "task_id", text("1"))?;
            tx.set_field("task", 1, "does_not_exist", Value::Null)?; // fails here
            tx.commit()
        })();
        assert!(torn.is_err(), "the operation fails on the unknown column");

        let tasks: i64 =
            e.conn().query_row("SELECT count(*) FROM task WHERE id=1", [], |r| r.get(0)).unwrap();
        let edges: i64 = e
            .conn()
            .query_row("SELECT count(*) FROM task_dependency WHERE task_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!((tasks, edges), (0, 0), "no half-applied operation is left behind");
    }

    /// Dropping the guard without committing rolls back — the plain-`?` early return above relies on
    /// exactly this, so it is worth pinning on its own.
    #[test]
    fn write_rolls_back_when_dropped_without_commit() {
        let e = StoreEngine::open_in_memory().unwrap();

        {
            let tx = e.write().unwrap();
            tx.set_field("task", 1, "title", text("hello")).unwrap();
        }

        let n: i64 =
            e.conn().query_row("SELECT count(*) FROM task WHERE id=1", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "an uncommitted transaction leaves nothing");
    }

    /// `BEGIN IMMEDIATE`. Structural proof, no timing — SQLite reports `Write` at `BEGIN`
    /// only under IMMEDIATE/EXCLUSIVE; a DEFERRED transaction sits at `None` until its first statement.
    /// Without it a read-then-write (`next_id` → INSERT) would take `SQLITE_BUSY` on the read→write
    /// upgrade, where SQLite deliberately refuses to run the busy handler.
    #[test]
    fn write_takes_the_write_lock_at_begin() {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.write().unwrap();
        assert_eq!(
            e.conn().transaction_state(None::<&str>).unwrap(),
            rusqlite::TransactionState::Write,
            "the write lock is held at BEGIN, not deferred to the first write"
        );
        drop(tx);
    }

    /// The reason the operation and its read must share one `BEGIN IMMEDIATE` transaction: two writers
    /// each computing `next_id` and INSERTing must not take the same id — which *is* the
    /// conversational number. SQLite's writer exclusion is per-transaction, so the read has to be *in*
    /// the transaction. Two connections to one file stand in for the CLI + GUI pair.
    #[test]
    fn concurrent_writers_never_take_the_same_number() {
        const WRITERS: i64 = 4;
        const EACH: i64 = 5;
        let (dir, path) = temp_store("write-number");
        // Open once up front: opening runs migrations, which would themselves contend for the lock.
        StoreEngine::open(&path).unwrap();

        std::thread::scope(|s| {
            for w in 0..WRITERS {
                let path = path.clone();
                s.spawn(move || {
                    let e = StoreEngine::open(&path).unwrap();
                    for i in 0..EACH {
                        let tx = e.write().unwrap();
                        let next = super::super::read::next_id(tx.conn(), "task").unwrap();
                        tx.put_record(
                            "task",
                            next,
                            &[("title", text(&format!("w{w}-{i}")))],
                        )
                        .unwrap();
                        tx.commit().unwrap();
                    }
                });
            }
        });

        let e = StoreEngine::open(&path).unwrap();
        let mut stmt = e.conn().prepare("SELECT id FROM task ORDER BY id").unwrap();
        let numbers: Vec<i64> =
            stmt.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        drop(stmt);
        assert_eq!(
            numbers,
            (1..=WRITERS * EACH).collect::<Vec<_>>(),
            "every writer read the next id under the write lock, so the numbers are dense and unique"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two logical operations cannot nest on one connection: SQLite has no nested transactions, and a
    /// silent nesting would let the inner `commit()` publish the outer operation's half-written rows.
    /// The failure is loud.
    #[test]
    fn a_second_write_on_the_same_engine_is_refused() {
        let e = StoreEngine::open_in_memory().unwrap();
        let outer = e.write().unwrap();
        assert!(e.write().is_err(), "a write transaction cannot be opened inside another");
        drop(outer);
    }
}
