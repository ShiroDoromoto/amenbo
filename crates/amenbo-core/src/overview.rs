//! The **overview tables** of the unified database.
//!
//! Folder bindings, read receipts, the inbox archive, the lint-hook consent, the AI-harness consent, the
//! nudge log and the usage tallies are device-local overview state: per-machine, never synced, and —
//! unlike a task or a decision — not a record of any one project. They are ordinary tables of the one
//! database, declared by [`crate::store_engine::schema::schema_sql`].
//!
//! This module is the read/write path onto them, written against a bare [`StoreEngine`] rather than a
//! particular store type.
//!
//! Nothing here carries the engine's LWW record shape: every table is keyed by the id it names (the
//! project's or the task's `INTEGER` key, or — for the nudge log — the id a declaration in the binary
//! goes by) and every write is a direct UPSERT/DELETE, wrapped in a transaction only where a caller
//! needs several rows to land together.

use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::binding::Registry;
use crate::error::Result;
use crate::harness::Consent;
use crate::read_receipts::ReadReceipts;
use crate::store_engine::schema::col;
use crate::store_engine::sql::{Col, Delete, Insert, Int, Pred, Select, Sort, Sql, Table};
use crate::store_engine::{StoreEngine, StoreEngineError};

/// The plain tables this module owns, named through the generated column identifiers — so a column
/// that is not in the store is a name that does not compile.
const BPD: col::binding_project_dir::Cols = col::binding_project_dir::ALL;
const RR: col::read_receipt::Cols = col::read_receipt::ALL;
const IA: col::inbox_archive::Cols = col::inbox_archive::ALL;
const MN: col::mailbox_notified::Cols = col::mailbox_notified::ALL;
const HO: col::hook_optout::Cols = col::hook_optout::ALL;
const HC: col::harness_consent::Cols = col::harness_consent::ALL;
const NF: col::nudge_fired::Cols = col::nudge_fired::ALL;

/// `store_meta` key for the mailbox's single last-seen instant. A scalar, so it lives in the KV
/// singleton table rather than the per-task `read_receipt` table.
pub const MAILBOX_LAST_SEEN_META: &str = "read_receipt.mailbox_last_seen";

/// `store_meta` key for how many times the app has been launched on this device.
pub const LAUNCH_COUNT_META: &str = "usage.launch_count";

/// `store_meta` key for the day the app was first launched on this device (`%Y-%m-%d`).
pub const FIRST_LAUNCH_DAY_META: &str = "usage.first_launch_day";

// ───────────────────────── folder bindings ─────────────────────────

/// Read the folder bindings — [`Registry::project_dirs`] (project → every dir pointing at it), keyed by
/// the project's `INTEGER` id, the same key the `project` record carries.
pub fn load_bindings(conn: &Connection) -> Registry {
    let mut reg = Registry::default();
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

/// Bring `binding_project_dir` to `reg`'s project-keyed index, inside the caller's transaction: the pairs
/// the registry no longer holds are deleted, the pairs it has gained are inserted, and **a pair that was
/// already there is left untouched — row and id both**.
///
/// The difference is what a row's `id` is for (`AMB-D-648`): a task points at a bound folder by that id,
/// so a save that deleted every row and wrote it back would renumber the whole index and leave every such
/// task pointing at nothing. [`Registry`] is a value type that holds no ids ([`crate::binding::Registry`]),
/// which is what keeps its whole test surface intact — so the ids are held here, by matching on the pair
/// the row is identified by. One transaction still means a contended save cannot tear.
pub fn write_bindings(tx: &Transaction<'_>, reg: &Registry) -> Result<()> {
    let held = binding_rows(tx)?;
    for (id, pair) in &held {
        if !reg.project_dirs.get(&pair.0).is_some_and(|dirs| dirs.contains(&pair.1)) {
            Delete::from(BPD.table)
                .filter(Pred::eq(BPD.id, *id))
                .sql()
                .execute(tx)
                .map_err(StoreEngineError::from)?;
        }
    }
    for (project_id, dirs) in &reg.project_dirs {
        for dir in dirs {
            if held.iter().any(|(_, pair)| pair.0 == *project_id && pair.1 == *dir) {
                continue;
            }
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

/// One bound folder as something else can point at it: the binding row's `id` and the path it records
/// (`AMB-D-648`). [`Registry`] is the shape for asking *which folders* a project has and stays free of
/// ids; this is the shape for naming **one** of them — what a task carries, and what `bind --rebind`
/// re-points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundFolder {
    /// The binding row's id — stable across a move or a rename of the folder, and retired when the
    /// folder is unbound (the id is never handed to the next folder bound).
    pub id: i64,
    /// The path recorded for it, as it was canonicalised at bind time.
    pub dir: String,
}

/// The folders bound to `project_id`, each with the id that names it, in path order (the set's own —
/// there is no main folder to lead with, `AMB-D-531`). Empty when the project has none.
pub fn bound_folders(conn: &Connection, project_id: i64) -> Vec<BoundFolder> {
    let mut sel = Select::new();
    let (id, dir) = (sel.col(BPD.id), sel.col(BPD.dir));
    let mut sql = Sql::from(&sel, BPD.table);
    sql.push_where(Some(&Pred::eq(BPD.project_id, project_id)));
    sql.order_by([Sort::by(BPD.dir)]);
    let Ok(mut stmt) = conn.prepare(sql.text()) else { return Vec::new() };
    let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(sql.params()), |r| {
        Ok(BoundFolder { id: id.get(r)?, dir: dir.get(r)? })
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

/// Every binding row as `(id, (project_id, dir))` — what [`write_bindings`] compares the registry against.
/// The pair is the row's identity (`UNIQUE (project_id, dir)`), so it is what says whether a row the
/// registry holds is a row the store already has.
fn binding_rows(tx: &Transaction<'_>) -> Result<Vec<(i64, (i64, String))>> {
    let mut sel = Select::new();
    let (id, project_id, dir) = (sel.col(BPD.id), sel.col(BPD.project_id), sel.col(BPD.dir));
    let mut stmt =
        tx.prepare(Sql::from(&sel, BPD.table).text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map([], |r| Ok((id.get(r)?, (project_id.get(r)?, dir.get(r)?))))
        .map_err(StoreEngineError::from)?;
    let mut held = Vec::new();
    for row in rows {
        held.push(row.map_err(StoreEngineError::from)?);
    }
    Ok(held)
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

// ───────────────────────── mailbox notified set ─────────────────────────

/// The task ids this device has already raised an OS notification for, ascending (the table's PK
/// order). The empty default means nothing has been announced yet — the state a fresh store is in.
pub fn mailbox_notified_ids(engine: &StoreEngine) -> Result<Vec<i64>> {
    let conn = engine.conn();
    let mut sel = Select::new();
    let task_id = sel.col(MN.task_id);
    let mut sql = Sql::from(&sel, MN.table);
    sql.order_by([Sort::by(MN.task_id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt.query_map([], |r| task_id.get(r)).map_err(StoreEngineError::from)?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row.map_err(StoreEngineError::from)?);
    }
    Ok(ids)
}

/// Record that these tasks have now been notified: INSERT one row each, in one transaction.
/// Idempotent — a task already in the set conflicts on its key and is left as it was, so a re-announce
/// is a no-op rather than an error.
pub fn mailbox_notified_add(engine: &StoreEngine, task_ids: &[i64]) -> Result<()> {
    if task_ids.is_empty() {
        return Ok(());
    }
    let tx = engine.transaction()?;
    for &task_id in task_ids {
        Insert::into(MN.table)
            .set(MN.task_id, task_id)
            .on_conflict_do_nothing(MN.task_id)
            .sql()
            .execute(&tx)
            .map_err(StoreEngineError::from)?;
    }
    tx.commit().map_err(StoreEngineError::from)?;
    Ok(())
}

/// GC: drop notified rows whose `task_id` is not `keep` (deleted/absent tasks), returning whether
/// anything was pruned. Without it the set would keep the ids of tasks long gone.
pub fn retain_live_mailbox_notified(engine: &StoreEngine, keep: impl Fn(i64) -> bool) -> Result<bool> {
    retain_live(engine, MN.table, MN.task_id, keep)
}

// ───────────────────────── nudge log ─────────────────────────

/// The nudges already put to the person on this device, each with the instant it went out. Ascending by
/// id (the table's PK order); the empty default means none has been put yet — the state a fresh store is
/// in, and the state every store is in until a nudge is declared ([`crate::nudge`]).
pub fn nudges_fired(engine: &StoreEngine) -> Result<Vec<(String, String)>> {
    let conn = engine.conn();
    let mut sel = Select::new();
    let (id, at) = (sel.col(NF.nudge_id), sel.col(NF.at));
    let mut sql = Sql::from(&sel, NF.table);
    sql.order_by([Sort::by(NF.nudge_id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows =
        stmt.query_map([], |r| Ok((id.get(r)?, at.get(r)?))).map_err(StoreEngineError::from)?;
    let mut fired = Vec::new();
    for row in rows {
        fired.push(row.map_err(StoreEngineError::from)?);
    }
    Ok(fired)
}

/// Record that `nudge_id` has now been put, at `at` (RFC3339 UTC `z`). The latest write wins: a
/// once-only nudge writes this row once and never asks again, and a repeating one leaves the instant of
/// the most recent time it went out.
pub fn mark_nudge_fired(engine: &StoreEngine, nudge_id: &str, at: &str) -> Result<()> {
    Insert::into(NF.table)
        .set(NF.nudge_id, nudge_id)
        .set(NF.at, at)
        .on_conflict_update(NF.nudge_id)
        .sql()
        .execute(engine.conn())
        .map_err(StoreEngineError::from)?;
    Ok(())
}

// ───────────────────────── usage tallies ─────────────────────────

/// What this device has counted about being used: how many launches, and the day of the first one
/// (`None` until a launch has been recorded). Scalars, so they live in `store_meta` beside the mailbox's
/// last-seen instant rather than in a table of their own — and they are the only tallies kept at all,
/// because everything else a nudge asks about is countable from the store itself (`AMB-D-543`).
pub fn usage_tallies(engine: &StoreEngine) -> Result<(i64, Option<String>)> {
    let launches =
        engine.get_meta(LAUNCH_COUNT_META)?.and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    Ok((launches, engine.get_meta(FIRST_LAUNCH_DAY_META)?))
}

/// Count one launch on `day` (`%Y-%m-%d`): raise the tally by one, and record the day if this is the
/// first launch this device has seen. The first day is written once and never moved — it is *when this
/// device started*, and a later launch says nothing about that.
///
/// A read-modify-write, and deliberately not held under a lock: two launches racing lose a tick between
/// them, and a nudge is judged on an order of magnitude of use, not on an exact count. Serialising the
/// app's startup on this would be paying far more than the answer is worth.
pub fn record_launch(engine: &StoreEngine, day: &str) -> Result<()> {
    let (launches, first_day) = usage_tallies(engine)?;
    engine.set_meta(LAUNCH_COUNT_META, Some(&launches.saturating_add(1).to_string()))?;
    if first_day.is_none() {
        engine.set_meta(FIRST_LAUNCH_DAY_META, Some(day))?;
    }
    Ok(())
}

// ───────────────────────── lint-hook opt-out ─────────────────────────

/// Has this project been opted out of the lint hooks — did `hooks uninstall` run in it? Presence of the
/// row is the whole answer, so this is a plain existence check ([`crate::hooks`]).
pub fn hook_opted_out(engine: &StoreEngine, project_id: i64) -> Result<bool> {
    let mut sel = Select::new();
    let pid = sel.col(HO.project_id);
    let mut sql = Sql::from(&sel, HO.table);
    sql.push_where(Some(&Pred::eq(HO.project_id, project_id)));
    let found: Option<i64> = engine
        .conn()
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| pid.get(r))
        .optional()
        .map_err(StoreEngineError::from)?;
    Ok(found.is_some())
}

/// Opt this project out, or take the opt-out back. `hooks uninstall` sets it and `hooks install` clears
/// it: both are explicit acts on one repository, and each undoes what the other said. Setting it twice is
/// setting it once (the UPSERT is a no-op), and clearing one that was never there is the asked-for state
/// rather than an error.
pub fn set_hook_optout(engine: &StoreEngine, project_id: i64, opted_out: bool) -> Result<()> {
    if opted_out {
        // The row is its own whole content — presence is the veto — so a repeat is a no-op, not an upsert
        // onto other columns (there are none). This mirrors `inbox_archive`, the other set-shaped table.
        Insert::into(HO.table)
            .set(HO.project_id, project_id)
            .on_conflict_do_nothing(HO.project_id)
            .sql()
            .execute(engine.conn())
            .map_err(StoreEngineError::from)?;
    } else {
        Delete::from(HO.table)
            .filter(Pred::eq(HO.project_id, project_id))
            .sql()
            .execute(engine.conn())
            .map_err(StoreEngineError::from)?;
    }
    Ok(())
}

// ───────────────────────── AI-harness consent ─────────────────────────

/// This project's answer on being asked to start its AI on `amenbo agent`, or `None` when it has never
/// been asked (`AMB-D-440`). The absence of a row is the unanswered state, so there is nothing here to
/// tell apart from a `no` that was actually given.
///
/// It is **not** a mirror of the settings on disk: what a folder is wired with is read every time
/// ([`crate::harness::probe`]), and the two meet in [`crate::harness::reconcile`] alone.
pub fn harness_consent(engine: &StoreEngine, project_id: i64) -> Result<Option<Consent>> {
    let mut sel = Select::new();
    let (allowed, asked_again) = (sel.col(HC.allowed), sel.col(HC.asked_again));
    let mut sql = Sql::from(&sel, HC.table);
    sql.push_where(Some(&Pred::eq(HC.project_id, project_id)));
    let found: Option<(i64, i64)> = engine
        .conn()
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
            Ok((allowed.get(r)?, asked_again.get(r)?))
        })
        .optional()
        .map_err(StoreEngineError::from)?;
    Ok(found.map(|(allowed, asked_again)| Consent {
        allowed: allowed != 0,
        asked_again: asked_again != 0,
    }))
}

/// Record this project's answer, replacing whatever it said before. The answer to the question as first
/// put is [`Consent::answered`]; the answer to the one re-ask is [`Consent::answered_again`], which is
/// what spends it.
pub fn set_harness_consent(engine: &StoreEngine, project_id: i64, consent: Consent) -> Result<()> {
    Insert::into(HC.table)
        .set(HC.project_id, project_id)
        .set(HC.allowed, i64::from(consent.allowed))
        .set(HC.asked_again, i64::from(consent.asked_again))
        .on_conflict_update(HC.project_id)
        .sql()
        .execute(engine.conn())
        .map_err(StoreEngineError::from)?;
    Ok(())
}

/// Take the answer off the record, putting this project back to never having been asked (`AMB-D-459`).
/// Deleting the row is the only way back: the unanswered state *is* the absence of one, so there is no
/// value to write. It takes the re-ask along with it — what is spent is spent against an answer, and
/// there is no longer one.
///
/// Clearing what was never answered is the asked-for state rather than an error, as with the opt-out.
pub fn clear_harness_consent(engine: &StoreEngine, project_id: i64) -> Result<()> {
    Delete::from(HC.table)
        .filter(Pred::eq(HC.project_id, project_id))
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

        // Bindings: the project-keyed set of folders round-trips, keyed by the project's INTEGER id.
        let mut reg = Registry::default();
        reg.record_project_ref(7, "/work/a");
        reg.record_project_ref(7, "/work/b");
        {
            let tx = engine.transaction().unwrap();
            write_bindings(&tx, &reg).unwrap();
            tx.commit().unwrap();
        }
        let back = load_bindings(engine.conn());
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

        // Mailbox notified set: a batched, idempotent set of announced tasks, prunable the same way.
        assert!(mailbox_notified_ids(&engine).unwrap().is_empty(), "fresh store has announced nothing");
        mailbox_notified_add(&engine, &[]).unwrap(); // an empty batch is a no-op, not an error
        assert!(mailbox_notified_ids(&engine).unwrap().is_empty());
        mailbox_notified_add(&engine, &[12, 11]).unwrap();
        mailbox_notified_add(&engine, &[11]).unwrap(); // re-announcing an id already in the set is a no-op
        assert_eq!(mailbox_notified_ids(&engine).unwrap(), [11, 12]);
        assert!(retain_live_mailbox_notified(&engine, |id| id == 11).unwrap());
        assert_eq!(mailbox_notified_ids(&engine).unwrap(), [11]);
        assert!(!retain_live_mailbox_notified(&engine, |id| id == 11).unwrap(), "nothing left to prune");

        // Nudge log: keyed by the nudge's declared id, and the instant is replaced rather than appended
        // beside — a nudge put twice is still one row, saying when it last went out.
        assert!(nudges_fired(&engine).unwrap().is_empty(), "a fresh store has put nothing");
        mark_nudge_fired(&engine, "autostart", "2026-08-04T00:00:00Z").unwrap();
        mark_nudge_fired(&engine, "autostart", "2026-08-11T00:00:00Z").unwrap();
        assert_eq!(
            nudges_fired(&engine).unwrap(),
            [("autostart".to_owned(), "2026-08-11T00:00:00Z".to_owned())]
        );

        // Usage tallies: nothing until a launch is counted, then the count climbs and the first day
        // stays where the first launch put it.
        assert_eq!(usage_tallies(&engine).unwrap(), (0, None));
        record_launch(&engine, "2026-08-04").unwrap();
        record_launch(&engine, "2026-08-09").unwrap();
        assert_eq!(usage_tallies(&engine).unwrap(), (2, Some("2026-08-04".to_owned())));
    }

    /// A save writes the whole index, and **a folder that is still in it keeps the row it had** — id and
    /// all (`AMB-D-648`). This is what lets something else point at a bound folder: renumbering the index
    /// on every bind would leave every such pointer naming a folder nobody chose.
    #[test]
    fn a_binding_that_survives_a_save_keeps_its_id() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let ids = || -> Vec<(i64, i64, String)> {
            let conn = engine.conn();
            let mut sel = Select::new();
            let (id, project, dir) = (sel.col(BPD.id), sel.col(BPD.project_id), sel.col(BPD.dir));
            let mut sql = Sql::from(&sel, BPD.table);
            sql.order_by([Sort::by(BPD.id)]);
            let mut stmt = conn.prepare(sql.text()).unwrap();
            let rows = stmt
                .query_map([], |r| Ok((id.get(r)?, project.get(r)?, dir.get(r)?)))
                .unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        let save = |reg: &Registry| {
            let tx = engine.transaction().unwrap();
            write_bindings(&tx, reg).unwrap();
            tx.commit().unwrap();
        };

        let mut reg = Registry::default();
        reg.record_project_ref(7, "/work/a");
        reg.record_project_ref(7, "/work/b");
        save(&reg);
        let first = ids();
        assert_eq!(first.len(), 2, "each folder is one row");

        // Saving the same index again changes nothing at all — not even the ids.
        save(&reg);
        assert_eq!(ids(), first, "an unchanged index is left exactly where it was");

        // One folder goes, another arrives: the row that stays keeps its id, and the new one takes a
        // number of its own rather than the one just freed.
        reg.forget_project_ref(7, "/work/b");
        reg.record_project_ref(7, "/work/c");
        save(&reg);
        let after = ids();
        assert_eq!(after[0], first[0], "the folder still bound keeps its row");
        assert_eq!(after.len(), 2);
        assert!(
            after[1].0 > first[1].0 && after[1].2 == "/work/c",
            "the folder that arrived is numbered past the one that left: {after:?}"
        );
    }
}
