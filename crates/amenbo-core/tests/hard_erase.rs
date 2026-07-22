//! Hard-erase must make specific content *physically gone* from the truth source — not tombstoned. A
//! comment is removed in full (with the files attached to it); an accepted decision's body is
//! redacted while the decision itself survives. In both cases the old content must vanish from the
//! read model (not merely be tombstoned) and stay gone across a reopen.


use amenbo_core::config::Paths;
use amenbo_core::model::{ActorKind, AttachmentTarget, View};
use amenbo_core::ops;
use amenbo_core::store::{HardEraseReport, HardEraseTarget};
use amenbo_core::Store;

const SECRET_COMMENT: &str = "OUT_OF_DESIGN_COMMENT_MARKER";
const SECRET_SECTION: &str = "OUT_OF_DESIGN_SECTION_MARKER";

fn temp_paths() -> Paths {
    let base = amenbo_scratch::scratch("harderase");
    Paths::at(base)
}

// Physical-presence probes over the read-model tables (test-only accessors on Store).
fn content_containing(store: &Store, needle: &str) -> i64 {
    store.debug_content_containing(needle)
}
fn rows_for(store: &Store, dataset: &str, row: i64) -> i64 {
    store.debug_rows_for(dataset, row)
}

/// A store with one task carrying a secret comment, and one accepted decision whose body
/// carries a secret section. Returns the store plus the task, comment and decision ids.
fn build() -> (Store, i64, i64, i64) {
    let paths = temp_paths();
    let mut store = Store::open_at(paths.clone()).unwrap();
    // Author/actor attribution is a facet plus a placeholder author_id string (audit-trail trace
    // only, not stored on the comment/decision).
    let me = "self".to_string();

    let project = store
        .project_add(ops::project::NewProject {
            name: "Backlog".to_string(),
            view: View::List,
            notes: String::new(),
            color: None,
        })
        .unwrap()
        .id;
    let task = store.add_task(ops::task::NewTask {
        title: "a task".to_string(),
        project_id: Some(project),
        due_on: None,
        start_on: None,
        priority: None,
        notes: String::new(),
        created_by_kind: Some(ActorKind::Ai),
    })
    .unwrap()
    .id;
    let comment = store
        .add_task_comment(
            task,
            ActorKind::Ai,
            &format!("keep this — but {SECRET_COMMENT} — is out of design"),
        )
        .unwrap()
        .id;

    let decision = store
        .add_decision(ops::decision::NewDecision {
            title: "a real decision".to_string(),
            body: format!("Sound conclusion.\n\nAside: {SECRET_SECTION} should not be here.\n\nMore rationale."),
            project_id: project,
        })
        .unwrap()
        .id;
    store.accept_decision(decision, Some(me.clone())).unwrap();

    // Reopen so the caller gets a store it can lock and erase through. (Drop first to release the
    // exclusive lock before reopening the same store.)
    drop(store);
    let store = Store::open_at(paths).unwrap();

    (store, task, comment, decision)
}

#[test]
fn hard_erase_comment_removes_it_physically() {
    let (mut store, _task, comment, _decision) = build();
    assert!(content_containing(&store, SECRET_COMMENT) > 0, "precondition: comment text is in the read model");
    assert!(rows_for(&store, "task_comment", comment) > 0);

    let report: HardEraseReport = store
        .hard_erase(&[HardEraseTarget::Comment { id: comment }])
        .unwrap();
    assert_eq!(report.comments_erased, vec![comment]);
    assert!(report.rows_removed > 0);

    // Read-model row gone and the secret text nowhere in the read model. (The store's in-memory `db` is
    // a snapshot taken at open, not a mirror, so the truth source is what these assertions read.)
    assert_eq!(rows_for(&store, "task_comment", comment), 0);
    assert_eq!(content_containing(&store, SECRET_COMMENT), 0, "comment text must be physically gone");

    // Survives a reopen (the erase was persisted, not just in-memory). Drop the store first to
    // release its exclusive lock.
    let paths = store.paths.clone();
    drop(store);
    let reopened = Store::open_at(paths).unwrap();
    assert_eq!(content_containing(&reopened, SECRET_COMMENT), 0);
    assert_eq!(rows_for(&reopened, "task_comment", comment), 0);
}

#[test]
fn hard_erase_redacts_accepted_decision_body_keeping_the_decision() {
    let (mut store, _task, _comment, decision) = build();
    assert!(content_containing(&store, SECRET_SECTION) > 0, "precondition: the section is in the read model");

    let redacted = "Sound conclusion.\n\nMore rationale.";
    let report = store
        .hard_erase(&[HardEraseTarget::DecisionBody {
            id: decision,
            new_body: redacted.to_string(),
        }])
        .unwrap();
    assert_eq!(report.decisions_redacted, vec![decision]);

    // The decision survives with the redacted body; the section is physically gone from the read model.
    assert_eq!(rows_for(&store, "decision", decision), 1, "decision still exists");
    assert!(content_containing(&store, "Sound conclusion.") > 0, "the redacted body is what remains");
    assert_eq!(content_containing(&store, SECRET_SECTION), 0, "the section must be physically gone");

    // Survives a reopen — the redacted body is what the read-model rebuilds `db` from.
    let paths = store.paths.clone();
    drop(store);
    let reopened = Store::open_at(paths).unwrap();
    assert_eq!(content_containing(&reopened, SECRET_SECTION), 0);
    let db = amenbo_core::store_engine::hydrate_database(reopened.read_model().conn()).unwrap();
    let d2 = db.decisions.iter().find(|d| d.id == decision).expect("decision persists");
    assert_eq!(d2.body, redacted);
}

/// Erasing a comment takes the files attached to it — the attachment rows *and* the bytes.
/// `attachment` is polymorphic, so nothing cascades it; and the bytes are out-of-band, so nothing
/// removes them either unless the erase reclaims them. Content-addressed bytes another live attachment
/// still points at survive: dedup means one stored copy, and the erase may not take someone else's file.
#[test]
fn hard_erase_comment_takes_its_attachments_and_their_bytes() {
    let (mut store, task, comment, _decision) = build();

    let only_the_comments = store.blobs().ingest_bytes(b"the file that should never have been posted").unwrap().hash;
    let shared = store.blobs().ingest_bytes(b"also attached to the task itself").unwrap().hash;
    for hash in [&only_the_comments, &shared] {
        store
            .attach_blob(AttachmentTarget::TaskComment, comment, hash, "f.bin", None, 0, ActorKind::Ai)
            .unwrap();
    }
    store
        .attach_blob(AttachmentTarget::Task, task, &shared, "f.bin", None, 0, ActorKind::Ai)
        .unwrap();
    store
        .attach_url(AttachmentTarget::TaskComment, comment, "https://example.com/", None, ActorKind::Ai)
        .unwrap();
    assert_eq!(store.attachments_for_target(AttachmentTarget::TaskComment, comment).unwrap().len(), 3);

    let report = store.hard_erase(&[HardEraseTarget::Comment { id: comment }]).unwrap();

    // The rows the comment carried are gone (no orphans left pointing at a comment that no longer exists,
    // ready to be re-parented onto whichever row is minted that id next).
    assert!(
        store.attachments_for_target(AttachmentTarget::TaskComment, comment).unwrap().is_empty(),
        "the comment's attachment rows go with it"
    );
    // …and so are the bytes it was the last reference to. No aging: an erase takes effect now, where the
    // ordinary reclaim spares blobs younger than GC_MIN_AGE.
    assert!(!store.blobs().has(&only_the_comments), "the erased comment's file must be gone from disk");
    assert_eq!(report.blobs_reclaimed, 1);
    assert!(report.bytes_reclaimed > 0);

    // The shared bytes stay — the task's own attachment still points at them.
    assert!(store.blobs().has(&shared), "bytes another live attachment references must survive");
    assert_eq!(store.attachments_for_target(AttachmentTarget::Task, task).unwrap().len(), 1);
}

#[test]
fn hard_erase_is_all_or_nothing_on_unknown_target() {
    let (mut store, _task, comment, _decision) = build();
    // A batch with one good and one unknown target must erase nothing.
    let err = store.hard_erase(&[
        HardEraseTarget::Comment { id: comment },
        HardEraseTarget::Comment { id: 999_999 },
    ]);
    assert!(err.is_err(), "an unknown target fails the whole call");
    assert!(content_containing(&store, SECRET_COMMENT) > 0, "the good target must be untouched");
    assert!(rows_for(&store, "task_comment", comment) > 0);
}
