//! The engine's own row writes, pinned from inside the crate. [`StoreEngine::put_record`],
//! [`StoreEngine::set_field`], [`StoreEngine::delete_record`] and their [`super::write::WriteTx`] twins write a
//! row without writing the activity that says so, so they are `pub(crate)`: outside the crate a row is
//! written through [`crate::store::Store`], which writes both. What these tests pin is the layer beneath.

mod writes {
    //! The store engine over a single local store: field writes UPSERT straight into the read-model,
    //! deletes take the row out, and startup migrates an older store's columns to the registry.

    use super::super::{StoreEngine, StoreEngineError};
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
}

mod constraints {
    //! The column types are not decoration — a `CHECK` refuses a value the column does not admit,
    //! and a `REFERENCES` refuses a reference the graph cannot keep whole. These pin that the constraints
    //! actually bite (and, just as important, that they still admit the `''` not-yet-written default a
    //! field-by-field create passes through). The vocabulary itself lives in `store_engine::schema`.

    use super::super::StoreEngine;
    use rusqlite::types::Value;

    fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }

    /// A `CHECK` admits its column's own `''` default: a record is created field-by-field (`INSERT id`,
    /// then one `UPDATE` per field), so a task written with only a title must succeed with `status`,
    /// `subtype`, `due_on` … left at their unwritten sentinels. If a `CHECK` rejected `''`, no record
    /// could ever be created.
    #[test]
    fn a_check_admits_the_unwritten_default() {
        let e = StoreEngine::open_in_memory().unwrap();
        e.put_record("task", 1, &[("title", text("only a title"))]).unwrap();
        let status: String =
            e.conn().query_row("SELECT status FROM task WHERE id=1", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "", "the enum column sits at its unwritten default, admitted by the CHECK");
    }

    /// An enum column refuses a value outside its closed set.
    #[test]
    fn a_check_refuses_a_value_off_the_enum() {
        let e = StoreEngine::open_in_memory().unwrap();
        let bad = e.put_record("task", 1, &[("status", text("nonsense"))]);
        assert!(bad.is_err(), "'nonsense' is not one of todo/in_progress/done/blocked");
        // A real value lands.
        e.put_record("task", 2, &[("status", text("in_progress"))]).unwrap();
    }

    /// A `DATE` column refuses anything that is not a `%Y-%m-%d` day — a timestamp, a slashed date.
    #[test]
    fn a_check_refuses_a_malformed_date() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(e.put_record("task", 1, &[("due_on", text("2026/07/11"))]).is_err(), "slashes");
        assert!(
            e.put_record("task", 2, &[("due_on", text("2026-07-11T00:00:00Z"))]).is_err(),
            "a day column is not an instant"
        );
        e.put_record("task", 3, &[("due_on", text("2026-07-11"))]).unwrap();
    }

    /// A `TS` column refuses anything that is not the fixed-width RFC3339Z instant form.
    #[test]
    fn a_check_refuses_a_malformed_instant() {
        let e = StoreEngine::open_in_memory().unwrap();
        // A bare day is not an instant (`completed_at` is a timestamp, not a date).
        assert!(e.put_record("task", 1, &[("completed_at", text("2026-07-11"))]).is_err());
        // Missing the trailing Z.
        assert!(e.put_record("task", 2, &[("completed_at", text("2026-07-11T00:00:00"))]).is_err());
        e.put_record("task", 3, &[("completed_at", text("2026-07-11T09:30:00Z"))]).unwrap();
    }

    /// A `HASH` column refuses anything that is not 64 lower-case hex digits (the blake3 blob address).
    #[test]
    fn a_check_refuses_a_non_hex_or_wrong_length_hash() {
        let e = StoreEngine::open_in_memory().unwrap();
        // Blob-mode attachments carry the hash; target_id is polymorphic so any id is fine here.
        assert!(e.put_record("attachment", 1, &[("blob_hash", text("deadbeef"))]).is_err(), "too short");
        let with_g = "g".repeat(64);
        assert!(e.put_record("attachment", 2, &[("blob_hash", text(&with_g))]).is_err(), "not hex");
        let ok = "deadbeef".repeat(8); // 64 hex digits
        e.put_record("attachment", 3, &[("blob_hash", text(&ok))]).unwrap();
    }

    /// A `BOOL` column refuses an integer that is not 0 or 1.
    #[test]
    fn a_check_refuses_a_non_boolean_flag() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(e.put_record("project", 1, &[("archived", Value::Integer(2))]).is_err());
        e.put_record("project", 2, &[("archived", Value::Integer(1))]).unwrap();
    }

    /// A `REFERENCES` refuses a reference that names no row — checked at `COMMIT`, so a whole logical
    /// operation that would leave the graph dangling rolls back and nothing lands.
    #[test]
    fn a_foreign_key_refuses_a_dangling_reference_at_commit() {
        let e = StoreEngine::open_in_memory().unwrap();
        let dangling: Result<(), _> = (|| {
            let tx = e.write()?;
            tx.put_record("task", 1, &[("title", text("real"))])?;
            // The blocker names a task that does not exist.
            tx.put_record("task_dependency", 1, &[("task_id", text("1")), ("blocked_by_id", text("999"))])?;
            tx.commit()
        })();
        assert!(dangling.is_err(), "the edge's blocked_by_id names no task — the commit is refused");
        let edges: i64 =
            e.conn().query_row("SELECT count(*) FROM task_dependency", [], |r| r.get(0)).unwrap();
        assert_eq!(edges, 0, "the refused operation left no edge behind");
    }

    /// The `REFERENCES` is `DEFERRABLE INITIALLY DEFERRED`, so within one transaction a child may be
    /// written before its parent — the check waits for `COMMIT`, by when both exist.
    #[test]
    fn a_deferred_foreign_key_allows_child_before_parent() {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.write().unwrap();
        // Edge first, its blocker task second — order that an immediate FK would reject.
        tx.put_record("task_dependency", 1, &[("task_id", text("1")), ("blocked_by_id", text("2"))]).unwrap();
        tx.put_record("task", 1, &[("title", text("a"))]).unwrap();
        tx.put_record("task", 2, &[("title", text("b"))]).unwrap();
        tx.commit().unwrap();
        let edges: i64 =
            e.conn().query_row("SELECT count(*) FROM task_dependency", [], |r| r.get(0)).unwrap();
        assert_eq!(edges, 1, "both tasks exist by commit, so the edge is kept");
    }

    /// **A concept row is not swept away with its parent** (`AMB-D-403`). Leaving a child behind stops the
    /// parent's `DELETE` where it stands rather than taking it: what a delete op does not take, the database
    /// refuses to lose. And it stops *there* — `RESTRICT` is not deferred, so the statement fails rather than
    /// the commit, even though the reference is declared `DEFERRABLE INITIALLY DEFERRED` (that deferral is the
    /// dangling-reference check's, which is a different one).
    #[test]
    fn a_parent_with_a_child_left_behind_cannot_be_deleted() {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.write().unwrap();
        tx.put_record("task", 1, &[("title", text("has a comment"))]).unwrap();
        tx.put_record("task_comment", 1, &[("task_id", text("1")), ("text", text("said something"))])
            .unwrap();
        tx.commit().unwrap();

        let refused = e.conn().execute("DELETE FROM task WHERE id = 1", []);
        assert!(refused.is_err(), "the comment is still there, so the task cannot go: {refused:?}");
        let left: i64 =
            e.conn().query_row("SELECT count(*) FROM task_comment", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 1, "and nothing was taken on the way out");

        // Taking the child first is what a delete op does, and then the parent goes.
        e.conn().execute("DELETE FROM task_comment WHERE id = 1", []).unwrap();
        e.conn().execute("DELETE FROM task WHERE id = 1", []).expect("with no child left, the task goes");
    }

    /// The exclusion the same decision names: Amenbo's own per-project settings are not concepts anyone points
    /// at, so they still ride the project's cascade. Pinned here because a blanket rewrite of the schema would
    /// take them along silently, and the cost of that is a `project delete` op that has to sweep rows nobody
    /// outside Amenbo ever sees.
    #[test]
    fn amenbos_own_settings_still_go_with_the_project() {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.write().unwrap();
        tx.put_record("project", 1, &[("name", text("going away"))]).unwrap();
        tx.commit().unwrap();
        let tx = e.write().unwrap();
        tx.put_record("project_notify", 1, &[("project_id", text("1")), ("enabled", text("1"))]).unwrap();
        tx.commit().unwrap();

        e.conn().execute("DELETE FROM project WHERE id = 1", []).expect("no concept row holds it back");
        let left: i64 =
            e.conn().query_row("SELECT count(*) FROM project_notify", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "the setting went with the project it was about");
    }
}
