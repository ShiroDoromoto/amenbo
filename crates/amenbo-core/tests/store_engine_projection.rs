//! Projecting a `Database` into an engine read-model must be *faithful*: every record lands, in every
//! dataset, and the friendly `#number` (the row's key) survives. The vessel is production's own write
//! mapping (`store_engine::record` → `WriteTx`), which every `Database` fixture rides into an engine —
//! there is no second, hand-copied projection to drift from it. A record type the mapping silently
//! drops would rot every fixture, and a column it drops would be a real write losing a field. This
//! pins that they land.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use amenbo_core::config::Paths;
use amenbo_core::model::{ActorKind, Database, Priority, TaskStatus};
use amenbo_core::ops::{self};
use amenbo_core::store_engine::{self, StoreEngine};
use amenbo_core::Store;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_paths() -> Paths {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let base: PathBuf = std::env::temp_dir().join(format!("amenbo-projection-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    Paths::at(base)
}

/// Build a small varied backlog with the real ops (a couple projects, statuses, a dependency, a
/// comment, and a deletion) and return `(me, a numbered task id)`. The subject is a facet
/// (human/ai); `me` is only a placeholder author/created_by token (the trace-only `author_id` arg /
/// `Attachment.created_by` string).
fn build_backlog(paths: &Paths) -> (String, i64) {
    let me = "self".to_string();
    let numbered;
    {
        // Writes go through the `Store` wrappers (one `BEGIN IMMEDIATE` tx each, committed on the
        // spot), so the numbered id is captured
        // from the wrapper's return value and the hydrated read happens via `reopen` at the call site.
        let mut store = Store::open_at(paths.clone()).unwrap();

        let p1 = store
            .project_add(ops::project::NewProject {
                name: "Backlog".to_string(),
                view: amenbo_core::model::View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;

        let add = |store: &mut Store, title: &str, pri: Option<Priority>| {
            store
                .add_task(ops::task::NewTask {
                    title: title.to_string(),
                    project_id: Some(p1),
                    due_on: None,
                    start_on: None,
                    priority: pri,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Human),
                })
                .unwrap()
                .id
        };

        let t1 = add(&mut store, "alpha", Some(Priority::High));
        let t2 = add(&mut store, "beta", Some(Priority::Medium));
        let t3 = add(&mut store, "gamma blocker", Some(Priority::Low));
        let t4 = add(&mut store, "delta gone", None);

        store.set_task_status(t2, TaskStatus::InProgress).unwrap();
        store.set_task_assignee(t2, Some(ActorKind::Ai)).unwrap();
        store.depend_task(t1, t3, Some(ActorKind::Human)).unwrap();

        // A comment so the comment dataset is exercised.
        store.add_task_comment(t1, ActorKind::Human, "looks good").unwrap();

        // A commit SHA so the task_commit dataset is exercised (non-empty parity, not vacuous).
        store
            .add_task_commit(t1, "0123456789abcdef0123456789abcdef01234567", Some(ActorKind::Ai))
            .unwrap();

        // Placement is task-held: a task has exactly one home, so there is no multi-membership
        // scenario to seed here. Delete t4 to exercise a deleted task in the projection.
        store.delete_task(t4, amenbo_core::model::ActorKind::Human).unwrap();
        numbered = t1;
    }
    (me, numbered)
}

/// Re-open the store (the write wrappers commit per operation, so a reopen sees every row).
fn reopen(paths: &Paths) -> Store {
    Store::open_at(paths.clone()).unwrap()
}

/// The hydrated, id-sorted `Database` a production read sees, raised from the truth source. The
/// `Store` keeps no in-memory copy of it, so the projection's source is raised on demand.
fn hydrated(store: &Store) -> Database {
    store_engine::hydrate_database(store.read_model().conn()).unwrap()
}

/// Project into a fresh in-memory engine and count, per dataset, the source records against the rows
/// that landed. The dataset→table pairing comes from the schema registry, so it cannot drift from the
/// tables the projection actually writes.
fn count_parity(db: &Database, e: &StoreEngine) -> Vec<(&'static str, usize, usize)> {
    let sources: Vec<(&'static str, usize)> = vec![
        ("project", db.projects.len()),
        ("task", db.tasks.len()),
        ("dependency", db.task_dependencies.len()),
        ("task_commit", db.task_commits.len()),
        ("decision", db.decisions.len()),
        ("decision_edge", db.decision_edges.len()),
        ("decision_task_link", db.decision_task_links.len()),
        ("dimension", db.dimensions.len()),
        ("dimension_value", db.dimension_values.len()),
        ("task_dimension_value", db.task_dimension_values.len()),
        ("task_comment", db.task_comments.len()),
        ("decision_comment", db.decision_comments.len()),
        ("attachment", db.attachments.len()),
    ];
    sources
        .into_iter()
        .map(|(dataset, source_total)| {
            let table = store_engine::schema::dataset(dataset).expect("dataset is registered").table;
            let landed: i64 = e
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            (dataset, source_total, landed as usize)
        })
        .collect()
}

/// Project `db` into a fresh in-memory engine.
fn projected(db: &Database) -> StoreEngine {
    let engine = StoreEngine::open_in_memory().unwrap();
    let tx = engine.write().unwrap();
    store_engine::record::put_database(&tx, db).unwrap();
    tx.commit().unwrap();
    engine
}

#[test]
fn every_record_lands_in_its_dataset() {
    let paths = temp_paths();
    let (_me, _numbered) = build_backlog(&paths);
    let store = reopen(&paths);

    let db = hydrated(&store);
    let counts = count_parity(&db, &projected(&db));

    for (dataset, source, landed) in &counts {
        assert_eq!(source, landed, "dataset {dataset}: {source} source records vs {landed} projected");
    }

    // A deleted task leaves no row (the backlog creates 4 and deletes 1).
    let (_, tasks, _) = counts.iter().find(|(d, ..)| *d == "task").unwrap();
    assert_eq!(*tasks, 3, "4 created, 1 deleted — a delete leaves no tombstone");

    // A comment was made → the task_comment dataset carried it (comments live in their own table),
    // so the parity above is asserting over a non-empty dataset, not a vacuous one.
    let (_, comments, _) = counts.iter().find(|(d, ..)| *d == "task_comment").unwrap();
    assert!(*comments >= 1, "task_comment projected");

    // A commit SHA was recorded → the task_commit dataset carried it, so its parity is over a
    // non-empty dataset too.
    let (_, commits, _) = counts.iter().find(|(d, ..)| *d == "task_commit").unwrap();
    assert!(*commits >= 1, "task_commit projected");
}

/// The friendly number *is* the key, so "does the number survive the projection" is the same question
/// as "does the row keep its id". There is no separate `number` column: the projection carries the id
/// and the model derives the number back off it.
#[test]
fn friendly_number_survives_projection() {
    let paths = temp_paths();
    let (_me, numbered) = build_backlog(&paths);
    let store = reopen(&paths);

    // The source task carries a friendly number, and it is the row's key.
    let db = hydrated(&store);
    let src = db.tasks.iter().find(|t| t.id == numbered).unwrap();
    assert_eq!(src.id, numbered, "the number is the key (the id is the conversational number itself)");

    // Project into a fresh engine and read the key straight out of the read model.
    let engine = projected(&db);
    let got: Option<i64> = engine
        .conn()
        .query_row("SELECT id FROM task WHERE id = ?1", [numbered], |r| r.get(0))
        .unwrap();
    assert_eq!(got, Some(numbered), "friendly number must survive migration");
}

#[test]
fn attachments_project_faithfully() {
    use amenbo_core::model::{ActorKind, AttachmentTarget};

    let paths = temp_paths();
    let (_me, numbered) = build_backlog(&paths);

    // Attach a live blob to a task and a since-removed url to a decision (the two attachment modes).
    {
        let mut store = reopen(&paths);
        store
            .attach_blob(
                AttachmentTarget::Task,
                numbered,
                "abc12300abc12300abc12300abc12300abc12300abc12300abc12300abc12300",
                "design.pdf",
                Some("application/pdf"),
                4096,
                ActorKind::Ai,
            )
            .unwrap();
        let dead = store
            .attach_url(
                AttachmentTarget::Decision,
                7001,
                "https://example.com/spec",
                Some("spec"),
                ActorKind::Ai,
            )
            .unwrap();
        store.remove_attachment(dead.id).unwrap();
    }
    let store = reopen(&paths);

    // Count parity: the removed attachment leaves no row behind, so 1 row.
    let db = hydrated(&store);
    let engine = projected(&db);
    for (dataset, source, landed) in count_parity(&db, &engine) {
        assert_eq!(source, landed, "dataset {dataset} projected faithfully with attachments present");
    }
    assert_eq!(db.attachments.len(), 1, "two created, one removed — the removal is physical");

    // Field-level fidelity: the live blob's columns survive the projection.
    let (tt, kind, hash, filename, mime, size): (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
    ) = engine
        .conn()
        .query_row(
            "SELECT target_type, kind, blob_hash, filename, mime, size_bytes FROM attachment",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!((tt.as_str(), kind.as_str()), ("task", "blob"));
    assert_eq!(hash.as_deref(), Some("abc12300abc12300abc12300abc12300abc12300abc12300abc12300abc12300"));
    assert_eq!(filename.as_deref(), Some("design.pdf"));
    assert_eq!(mime.as_deref(), Some("application/pdf"));
    assert_eq!(size, Some(4096));

    // The removed url-mode row is gone from the projection too — the removal was physical.
    let rows: i64 = engine
        .conn()
        .query_row("SELECT COUNT(*) FROM attachment", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "only the live blob attachment is projected");
}

/// Attachments on a task_comment / decision_comment survive the projection and hydrate back to their
/// `AttachmentTarget` variants (the `target_type` column is a plain string, so the variants are purely
/// additive).
#[test]
fn comment_target_attachments_project_and_hydrate_faithfully() {
    use amenbo_core::model::{ActorKind, AttachmentTarget};

    let paths = temp_paths();
    let (_me, _numbered) = build_backlog(&paths);
    {
        let mut store = reopen(&paths);
        for (tt, id) in [
            (AttachmentTarget::TaskComment, 7101_i64),
            (AttachmentTarget::DecisionComment, 7102_i64),
        ] {
            store
                .attach_url(
                    tt,
                    id,
                    &format!("https://example.com/{id}"),
                    Some("note"),
                    ActorKind::Ai,
                )
                .unwrap();
        }
    }

    // Projection keeps the exact target_type strings.
    let store = reopen(&paths);
    let engine = projected(&hydrated(&store));
    let mut kinds: Vec<String> = engine
        .conn()
        .prepare("SELECT target_type FROM attachment ORDER BY target_type")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    kinds.sort();
    assert_eq!(kinds, vec!["decision_comment".to_string(), "task_comment".to_string()]);

    // Hydrate parses the strings back into the enum variants (not a load error).
    let db = hydrated(&store);
    let mut got: Vec<AttachmentTarget> = db.attachments.iter().map(|a| a.target_type).collect();
    got.sort_by_key(|t| t.as_str());
    assert_eq!(got, vec![AttachmentTarget::DecisionComment, AttachmentTarget::TaskComment]);
}
