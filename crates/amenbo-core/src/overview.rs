//! The **overview tables** of the unified database.
//!
//! Folder bindings, read receipts, the inbox archive and the lint-hook consent are device-local
//! overview state: per-machine, never synced, and — unlike a task or a decision — not a record of any
//! one project. They are ordinary tables of the one database, declared by
//! [`crate::store_engine::schema::schema_sql`].
//!
//! This module is the read/write path onto them, written against a bare [`StoreEngine`] rather than a
//! particular store type.
//!
//! Nothing here carries the engine's LWW record shape: every table is keyed by the id it names (the
//! project's or the task's `INTEGER` key) and every write is a direct UPSERT/DELETE, wrapped in a
//! transaction only where a caller needs several rows to land together.

use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::binding::Registry;
use crate::error::Result;
use crate::hooks::HookConsent;
use crate::read_receipts::ReadReceipts;
use crate::store_engine::schema::col;
use crate::store_engine::sql::{Col, Delete, Insert, Int, Pred, Select, Sort, Sql, Table};
use crate::store_engine::{StoreEngine, StoreEngineError};

/// The plain tables this module owns, named through the generated column identifiers — so a column
/// that is not in the store is a name that does not compile.
const BP: col::binding_path::Cols = col::binding_path::ALL;
const BPD: col::binding_project_dir::Cols = col::binding_project_dir::ALL;
const RR: col::read_receipt::Cols = col::read_receipt::ALL;
const IA: col::inbox_archive::Cols = col::inbox_archive::ALL;
const HC: col::hook_consent::Cols = col::hook_consent::ALL;

/// `store_meta` key for the mailbox's single last-seen instant. A scalar, so it lives in the KV
/// singleton table rather than the per-task `read_receipt` table.
pub const MAILBOX_LAST_SEEN_META: &str = "read_receipt.mailbox_last_seen";

// ───────────────────────── folder bindings ─────────────────────────

/// Read the folder bindings — [`Registry::paths`] (project → its main dir) and
/// [`Registry::project_dirs`] (project → every dir pointing at it). Both are keyed by the project's
/// `INTEGER` id, the same key the `project` record carries.
pub fn load_bindings(conn: &Connection) -> Registry {
    let mut reg = Registry::default();
    let mut paths = Select::new();
    let (bp_project, bp_dir) = (paths.col(BP.project_id), paths.col(BP.dir));
    if let Ok(mut stmt) = conn.prepare(Sql::from(&paths, BP.table).text()) {
        if let Ok(rows) = stmt.query_map([], |r| Ok((bp_project.get(r)?, bp_dir.get(r)?))) {
            for (project_id, dir) in rows.flatten() {
                reg.paths.insert(project_id, dir);
            }
        }
    }
    let mut dirs = Select::new();
    let (bpd_project, bpd_dir) = (dirs.col(BPD.project_id), dirs.col(BPD.dir));
    if let Ok(mut stmt) = conn.prepare(Sql::from(&dirs, BPD.table).text()) {
        if let Ok(rows) = stmt.query_map([], |r| Ok((bpd_project.get(r)?, bpd_dir.get(r)?))) {
            for (project_id, dir) in rows.flatten() {
                reg.project_dirs.entry(project_id).or_default().insert(dir);
            }
        }
    }
    reg
}

/// Replace `binding_path` / `binding_project_dir` with `reg`'s two project-keyed indexes, inside the
/// caller's transaction. A full rewrite: the data is a handful of folders, so rewriting every row per
/// save keeps the value-type API (and its whole test surface) intact, and one transaction means a
/// contended save cannot tear.
pub fn write_bindings(tx: &Transaction<'_>, reg: &Registry) -> Result<()> {
    for table in [BP.table, BPD.table] {
        Delete::from(table).sql().execute(tx).map_err(StoreEngineError::from)?;
    }
    for (project_id, dir) in &reg.paths {
        Insert::into(BP.table)
            .set(BP.project_id, *project_id)
            .set(BP.dir, dir.as_str())
            .sql()
            .execute(tx)
            .map_err(StoreEngineError::from)?;
    }
    for (project_id, dirs) in &reg.project_dirs {
        for dir in dirs {
            Insert::into(BPD.table)
                .set(BPD.project_id, *project_id)
                .set(BPD.dir, dir.as_str())
                .sql()
                .execute(tx)
                .map_err(StoreEngineError::from)?;
        }
    }
    Ok(())
}

// ───────────────────────── read receipts ─────────────────────────

/// Load this device's read receipts: every per-task last-seen row plus the mailbox last-seen scalar.
/// Returns the empty default when nothing has been recorded yet.
pub fn read_receipts(engine: &StoreEngine) -> Result<ReadReceipts> {
    let conn = engine.conn();
    let mut rr = ReadReceipts::default();
    {
        let mut sel = Select::new();
        let (task_id, last_seen) = (sel.col(RR.task_id), sel.col(RR.last_seen));
        let mut stmt =
            conn.prepare(Sql::from(&sel, RR.table).text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map([], |r| Ok((task_id.get(r)?, last_seen.get(r)?)))
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let (id, seen) = row.map_err(StoreEngineError::from)?;
            rr.tasks.insert(id, seen);
        }
    }
    rr.mailbox_last_seen = engine.get_meta(MAILBOX_LAST_SEEN_META)?;
    Ok(rr)
}

/// Mark a task seen at `at` (RFC3339 UTC `z`): UPSERT one row. Idempotent per task — the latest write
/// to `last_seen` simply wins (no LWW bookkeeping; device-local single writer semantics).
pub fn mark_task_seen(engine: &StoreEngine, task_id: i64, at: &str) -> Result<()> {
    Insert::into(RR.table)
        .set(RR.task_id, task_id)
        .set(RR.last_seen, at)
        .on_conflict_update(RR.task_id)
        .sql()
        .execute(engine.conn())
        .map_err(StoreEngineError::from)?;
    Ok(())
}

/// Mark the whole mailbox seen at `at` (advances the badge-freshness scalar in `store_meta`).
pub fn mark_mailbox_seen(engine: &StoreEngine, at: &str) -> Result<()> {
    engine.set_meta(MAILBOX_LAST_SEEN_META, Some(at))?;
    Ok(())
}

/// GC: drop per-task receipt rows whose `task_id` is not `keep` (deleted/absent tasks), returning
/// whether anything was pruned. The mailbox scalar is a separate axis and is left untouched.
pub fn retain_live_read_receipts(engine: &StoreEngine, keep: impl Fn(i64) -> bool) -> Result<bool> {
    retain_live(engine, RR.table, RR.task_id, keep)
}

// ───────────────────────── inbox archive ─────────────────────────

/// The task ids this device has dismissed from the inbox, ascending (the table's PK order). The inbox
/// view excludes these; the empty default means nothing dismissed yet.
pub fn inbox_archive_ids(engine: &StoreEngine) -> Result<Vec<i64>> {
    let conn = engine.conn();
    let mut sel = Select::new();
    let task_id = sel.col(IA.task_id);
    let mut sql = Sql::from(&sel, IA.table);
    sql.order_by([Sort::by(IA.task_id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt.query_map([], |r| task_id.get(r)).map_err(StoreEngineError::from)?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row.map_err(StoreEngineError::from)?);
    }
    Ok(ids)
}

/// Dismiss a task from the inbox: INSERT one row. Idempotent — re-dismissing an already-archived task
/// is a no-op (the insert conflicts on the task's key and does nothing).
pub fn inbox_archive_add(engine: &StoreEngine, task_id: i64) -> Result<()> {
    Insert::into(IA.table)
        .set(IA.task_id, task_id)
        .on_conflict_do_nothing(IA.task_id)
        .sql()
        .execute(engine.conn())
        .map_err(StoreEngineError::from)?;
    Ok(())
}

/// Un-dismiss a task (return it to the inbox): DELETE its row. Idempotent — removing an absent row is
/// a no-op.
pub fn inbox_archive_remove(engine: &StoreEngine, task_id: i64) -> Result<()> {
    Delete::from(IA.table)
        .filter(Pred::eq(IA.task_id, task_id))
        .sql()
        .execute(engine.conn())
        .map_err(StoreEngineError::from)?;
    Ok(())
}

/// GC: drop archive rows whose `task_id` is not `keep` (deleted/absent tasks), returning whether
/// anything was pruned.
pub fn retain_live_inbox_archive(engine: &StoreEngine, keep: impl Fn(i64) -> bool) -> Result<bool> {
    retain_live(engine, IA.table, IA.task_id, keep)
}

// ───────────────────────── lint-hook consent ─────────────────────────

/// What a project answered when asked whether amenbo may install the lint hook ([`HookConsent`]), or
/// `None` if it has never been asked — the absence of a row *is* the unanswered state, so a project
/// that refused reads back differently from one that was never asked.
///
/// A row the `CHECK` should have refused reads back as `None` too: an unreadable answer is one nobody
/// gave, and asking again is the safe way to be wrong.
pub fn hook_consent(engine: &StoreEngine, project_id: i64) -> Result<Option<HookConsent>> {
    let mut sel = Select::new();
    let answer = sel.col(HC.answer);
    let mut sql = Sql::from(&sel, HC.table);
    sql.push_where(Some(&Pred::eq(HC.project_id, project_id)));
    let stored: Option<String> = engine
        .conn()
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| answer.get(r))
        .optional()
        .map_err(StoreEngineError::from)?;
    Ok(stored.as_deref().and_then(HookConsent::parse))
}

/// Record a project's answer: UPSERT one row. The latest answer simply wins — asked again after a
/// state change (a hook deleted by hand), a project answers again, and the new answer is the one that
/// counts.
pub fn set_hook_consent(engine: &StoreEngine, project_id: i64, answer: HookConsent) -> Result<()> {
    Insert::into(HC.table)
        .set(HC.project_id, project_id)
        .set(HC.answer, answer.as_str())
        .on_conflict_update(HC.project_id)
        .sql()
        .execute(engine.conn())
        .map_err(StoreEngineError::from)?;
    Ok(())
}

/// Prune the rows of a `task_id`-keyed overview table down to the ids `keep` accepts, in one
/// transaction. Shared by the receipts and the archive: both are device-local sets of task ids whose
/// tasks may have been deleted since.
///
/// Callers MUST pass a `keep` reflecting the **complete** set of live task ids — a partial set would
/// evict legitimate rows, which is why the caller, not this function, decides what "live" means.
fn retain_live(
    engine: &StoreEngine,
    table: Table,
    task_id: Col<Int>,
    keep: impl Fn(i64) -> bool,
) -> Result<bool> {
    let conn = engine.conn();
    let dead: Vec<i64> = {
        let mut sel = Select::new();
        let id_slot = sel.col(task_id);
        let mut stmt =
            conn.prepare(Sql::from(&sel, table).text()).map_err(StoreEngineError::from)?;
        let ids = stmt.query_map([], |r| id_slot.get(r)).map_err(StoreEngineError::from)?;
        let mut dead = Vec::new();
        for id in ids {
            let id = id.map_err(StoreEngineError::from)?;
            if !keep(id) {
                dead.push(id);
            }
        }
        dead
    };
    if dead.is_empty() {
        return Ok(false);
    }
    let tx = engine.transaction()?;
    for chunk in dead.chunks(PRUNE_CHUNK) {
        Delete::from(table)
            .filter(Pred::is_in(task_id, chunk.iter().copied()))
            .sql()
            .execute(&tx)
            .map_err(StoreEngineError::from)?;
    }
    tx.commit().map_err(StoreEngineError::from)?;
    Ok(true)
}

/// How many ids one prune statement names. An `IN (…)` binds one value per id, and SQLite caps a
/// statement's variables — so the dead set is cut into statements rather than assumed small.
const PRUNE_CHUNK: usize = 500;

#[cfg(test)]
mod tests {
    use super::*;

    /// The unified database carries the overview tables, and this module reads and writes them
    /// through a plain store engine.
    #[test]
    fn overview_tables_round_trip_through_the_unified_engine() {
        let engine = StoreEngine::open_in_memory().unwrap();

        // Bindings: the two project-keyed indexes round-trip, keyed by the project's INTEGER id.
        let mut reg = Registry::default();
        reg.set(7, "/work/a");
        reg.record_project_ref(7, "/work/a");
        reg.record_project_ref(7, "/work/b");
        {
            let tx = engine.transaction().unwrap();
            write_bindings(&tx, &reg).unwrap();
            tx.commit().unwrap();
        }
        let back = load_bindings(engine.conn());
        assert_eq!(back.get(7), Some("/work/a"));
        assert_eq!(back.dirs_for_project(7), vec!["/work/a", "/work/b"]);

        // Read receipts: per-task rows plus the mailbox scalar, and the GC drops dead tasks only.
        // The key is the task's INTEGER id — the same key the records carry.
        mark_task_seen(&engine, 11, "2026-07-10T00:00:00Z").unwrap();
        mark_task_seen(&engine, 12, "2026-07-10T00:00:00Z").unwrap();
        mark_task_seen(&engine, 11, "2026-07-11T00:00:00Z").unwrap(); // last write wins
        mark_mailbox_seen(&engine, "2026-07-11T00:00:00Z").unwrap();
        let rr = read_receipts(&engine).unwrap();
        assert_eq!(rr.tasks.get(&11).map(String::as_str), Some("2026-07-11T00:00:00Z"));
        assert_eq!(rr.mailbox_last_seen.as_deref(), Some("2026-07-11T00:00:00Z"));
        assert!(retain_live_read_receipts(&engine, |id| id == 11).unwrap());
        assert_eq!(read_receipts(&engine).unwrap().tasks.keys().collect::<Vec<_>>(), [&11]);
        assert!(!retain_live_read_receipts(&engine, |id| id == 11).unwrap(), "nothing left to prune");

        // Inbox archive: an idempotent set of dismissed tasks, prunable the same way.
        inbox_archive_add(&engine, 12).unwrap();
        inbox_archive_add(&engine, 11).unwrap();
        inbox_archive_add(&engine, 11).unwrap();
        assert_eq!(inbox_archive_ids(&engine).unwrap(), [11, 12]);
        inbox_archive_remove(&engine, 11).unwrap();
        inbox_archive_remove(&engine, 11).unwrap();
        assert_eq!(inbox_archive_ids(&engine).unwrap(), [12]);
        assert!(retain_live_inbox_archive(&engine, |_| false).unwrap());
        assert!(inbox_archive_ids(&engine).unwrap().is_empty());
    }
}
