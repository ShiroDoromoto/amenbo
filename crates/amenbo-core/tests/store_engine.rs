//! The engine engine over a single local store: field writes UPSERT straight into the read-model,
//! deletes take the row out, and startup migrates an older store's columns to the registry.

use amenbo_core::store_engine::{StoreEngine, StoreEngineError};
use rusqlite::types::Value;

/// A fresh temp file path for a path-backed engine (so we can reopen the same DB file).
fn temp_db_path() -> std::path::PathBuf {
    amenbo_scratch::scratch("engine").join("store.db")
}

fn txt(s: &str) -> Value {
    Value::Text(s.into())
}

/// Read a single read-model column as text (NULL → None).
fn field(e: &StoreEngine, table: &str, id: &str, col: &str) -> Option<String> {
    e.conn()
        .query_row(&format!("SELECT \"{col}\" FROM {table} WHERE id=?1"), [id], |r| {
            r.get::<_, Option<String>>(0)
        })
        .unwrap()
}

/// A delete is physical. The row goes — there is no `deleted_at` left to write, so nothing can
/// hold a deleted record's text where a read might still find it.
#[test]
fn delete_removes_the_row() {
    let a = StoreEngine::open_in_memory().unwrap();
    a.put_record("task", 1, &[("title", txt("doomed")), ("notes", txt("convergence harness"))])
        .unwrap();
    a.put_record("task", 2, &[("title", txt("kept"))]).unwrap();

    a.delete_record("task", 1).unwrap();

    let rows: i64 = a
        .conn()
        .query_row("SELECT COUNT(*) FROM task WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0, "the row is gone, not tombstoned");
    assert_eq!(field(&a, "task", "2", "title").as_deref(), Some("kept"), "the sibling is untouched");
}

/// A store this build writes carries no fts5 index over tasks. A store an older build left behind still
/// does, and that leftover is **inert**: no read touches it, so the open leaves it alone rather than
/// dropping it (the open runs no migrations of any kind).
#[test]
fn the_task_fts_index_is_never_created() {
    let path = temp_db_path();
    let a = StoreEngine::open(&path).unwrap();
    let made: i64 = a
        .conn()
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'task_fts%'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(made, 0, "this build creates no fts5 index over tasks");
}

#[test]
fn repeated_writes_keep_only_the_latest_value() {
    let e = StoreEngine::open_in_memory().unwrap();

    // Rewriting one field several times leaves the read-model at the latest value (each write UPSERTs
    // in place — no append-only log accumulates), and distinct live fields are each preserved.
    e.put_record("task", 1, &[("title", txt("v0"))]).unwrap();
    for i in 1..=4 {
        e.set_field("task", 1, "title", txt(&format!("v{i}"))).unwrap();
    }
    e.set_field("task", 1, "status", txt("todo")).unwrap();
    e.put_record("project", 1, &[("name", txt("Backlog"))]).unwrap();

    assert_eq!(field(&e, "task", "1", "title").as_deref(), Some("v4"), "newest write wins");
    assert_eq!(field(&e, "task", "1", "status").as_deref(), Some("todo"));
    assert_eq!(field(&e, "project", "1", "name").as_deref(), Some("Backlog"));

    // Exactly one read-model row per record, however many times a field was rewritten.
    let task_rows: i64 =
        e.conn().query_row("SELECT count(*) FROM task", [], |r| r.get(0)).unwrap();
    assert_eq!(task_rows, 1, "one row per record, not a growing write log");
}

#[test]
fn unknown_dataset_or_column_is_rejected() {
    let a = StoreEngine::open_in_memory().unwrap();
    assert!(matches!(a.set_field("nope", 1, "title", txt("v")), Err(StoreEngineError::UnknownDataset(_))));
    assert!(matches!(
        a.set_field("task", 1, "not_a_column", txt("v")),
        Err(StoreEngineError::UnknownColumn { .. })
    ));
}
