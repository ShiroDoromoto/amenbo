//! End-to-end wiring of the content-addressed blob store to the attachment metadata. The blob bytes
//! live out-of-band under `<store>/blobs`, but their **refcount** and the
//! **GC root set** are derived from the live `blob` attachments in the engine read-model — so a blob
//! survives exactly while some live attachment points at it, and `gc_blobs()` collects the rest.

use std::time::{Duration, SystemTime};

use amenbo_core::config::Paths;
use amenbo_core::model::{ActorKind, AttachmentTarget};
use amenbo_core::store_engine::read;
use amenbo_core::Store;

fn temp_paths() -> Paths {
    let base = amenbo_scratch::scratch("blobstore");
    Paths::at(base)
}

/// Append a `blob`-mode attachment whose `blob_hash` is `hash`, removing it again when `then_remove`.
fn attach_blob(store: &mut Store, target: i64, hash: &str, then_remove: bool) {
    let a = store
        .attach_blob(
            AttachmentTarget::Task,
            target,
            hash,
            "f.bin",
            Some("application/octet-stream"),
            0,
            ActorKind::Ai,
        )
        .unwrap();
    if then_remove {
        store.remove_attachment(a.id).unwrap();
    }
}

/// Stand-in task ids for the attachment targets in these fixtures (no task rows needed — the
/// attachment read-model keys on the target id alone).
const TASK_A: i64 = 1;
const TASK_B: i64 = 2;
/// A target id nothing was ever issued under — an attachment on it is an orphan by construction.
const GONE_TASK: i64 = 9_999;

#[test]
fn refcount_and_gc_track_live_attachments() {
    let paths = temp_paths();

    // Ingest four blobs; wire attachments to three of them (one shared by two live attachments, one
    // unique, one referenced only by an attachment that is then removed), and leave the fourth an orphan.
    let (shared, unique, dead, orphan) = {
        let mut store = Store::open_at(paths.clone()).unwrap();
        let shared = store.blobs().ingest_bytes(b"shared payload").unwrap().hash;
        let unique = store.blobs().ingest_bytes(b"unique payload").unwrap().hash;
        let dead = store.blobs().ingest_bytes(b"dead payload").unwrap().hash;
        let orphan = store.blobs().ingest_bytes(b"orphan payload").unwrap().hash;

        // shared is referenced by two live attachments (refcount 2 — dedup means one stored copy).
        attach_blob(&mut store, TASK_A, &shared, false);
        attach_blob(&mut store, TASK_B, &shared, false);
        attach_blob(&mut store, TASK_A, &unique, false);
        // dead's only reference is removed → refcount 0.
        attach_blob(&mut store, TASK_A, &dead, true);
        (shared, unique, dead, orphan)
    };

    let store = Store::open_at(paths.clone()).unwrap();
    let rm = store.read_model();
    let conn = rm.conn();

    // Refcount counts only live attachments; the shared hash is referenced twice.
    assert_eq!(read::blob_refcount(conn, &shared).unwrap(), 2);
    assert_eq!(read::blob_refcount(conn, &unique).unwrap(), 1);
    assert_eq!(read::blob_refcount(conn, &dead).unwrap(), 0);
    assert_eq!(read::blob_refcount(conn, &orphan).unwrap(), 0);

    // The GC root set is exactly the live-referenced hashes.
    let referenced = read::referenced_blob_hashes(conn).unwrap();
    assert!(referenced.contains(&shared));
    assert!(referenced.contains(&unique));
    assert!(!referenced.contains(&dead));
    assert!(!referenced.contains(&orphan));

    // gc_blobs collects the unreferenced bytes (dead + orphan) and keeps the referenced ones.
    let report = store.gc_blobs(Duration::ZERO).unwrap();
    assert_eq!(report.removed, 2);
    assert!(store.blobs().has(&shared));
    assert!(store.blobs().has(&unique));
    assert!(!store.blobs().has(&dead));
    assert!(!store.blobs().has(&orphan));
}

/// Deleting a project takes its attachments' bytes with it: the delete reclaims the blobs its rows
/// were the last reference to, while a blob another live project still points at survives. The
/// teardown that follows sweeps the whole blob store as the catch-all — by then there is normally
/// nothing left, which is what it means for the delete to have cleaned up after itself.
#[test]
fn deleting_a_project_reclaims_the_blobs_it_was_the_last_reference_to() {
    let paths = temp_paths();
    let mut store = Store::open_at(paths.clone()).unwrap();

    let doomed = store.project_add(new_project("消えるPJ")).unwrap().id;
    let survivor = store.project_add(new_project("残るPJ")).unwrap().id;
    let doomed_task = store.add_task(new_task("添付つき", doomed)).unwrap().id;
    let survivor_task = store.add_task(new_task("生き残り", survivor)).unwrap().id;

    let dropped = store.blobs().ingest_bytes(b"only the doomed project's").unwrap().hash;
    let shared = store.blobs().ingest_bytes(b"also on a live project's task").unwrap().hash;
    attach_blob(&mut store, doomed_task, &dropped, false);
    attach_blob(&mut store, doomed_task, &shared, false);
    attach_blob(&mut store, survivor_task, &shared, false);

    // GC spares blobs younger than GC_MIN_AGE (an attach in another process may be mid-flight), so
    // age both files past it — the sweep, not the clock, is what this test is about.
    age_blobs(&paths, &[&dropped, &shared]);

    store.project_delete(doomed, amenbo_core::model::ActorKind::Human).unwrap();
    assert!(!store.blobs().has(&dropped), "the deleted project's own attachment bytes are gone");
    assert!(store.blobs().has(&shared), "bytes a live attachment still points at survive");

    let report = amenbo_core::project_teardown::teardown_deleted_project(&store, doomed).unwrap();
    assert_eq!(report.blobs_reclaimed, 0, "the delete already collected what it orphaned");
    assert!(store.blobs().has(&shared), "and the sweep leaves a referenced blob alone");
}

/// `attach rm` reclaims the bytes it just orphaned, without a sweep. Bytes another live attachment
/// still points at are untouched: blobs are content-addressed, so "the attachment that held it is
/// gone" is not "the bytes are garbage".
#[test]
fn removing_an_attachment_reclaims_the_bytes_it_orphaned() {
    let paths = temp_paths();
    let mut store = Store::open_at(paths.clone()).unwrap();

    let dropped = store.blobs().ingest_bytes(b"nobody else's").unwrap().hash;
    let shared = store.blobs().ingest_bytes(b"a second attachment holds this").unwrap().hash;
    let a = attach(&mut store, TASK_A, &dropped);
    let b = attach(&mut store, TASK_A, &shared);
    attach(&mut store, TASK_B, &shared);
    age_blobs(&paths, &[&dropped, &shared]);

    store.remove_attachment(a).unwrap();
    assert!(!store.blobs().has(&dropped), "the bytes nothing points at any more are gone");

    store.remove_attachment(b).unwrap();
    assert!(store.blobs().has(&shared), "the other attachment still points at these bytes");
}

/// Deleting a task takes the bytes of its own attachments — and of its comments' attachments — with
/// it. The rows go physically, so nothing is left to tell a later sweep what they held.
#[test]
fn deleting_a_task_reclaims_its_attachments_bytes() {
    let paths = temp_paths();
    let mut store = Store::open_at(paths.clone()).unwrap();

    let project = store.project_add(new_project("PJ")).unwrap().id;
    let doomed = store.add_task(new_task("消えるタスク", project)).unwrap().id;
    let survivor = store.add_task(new_task("残るタスク", project)).unwrap().id;

    let on_task = store.blobs().ingest_bytes(b"attached to the task").unwrap().hash;
    let on_comment = store.blobs().ingest_bytes(b"attached to its comment").unwrap().hash;
    let elsewhere = store.blobs().ingest_bytes(b"attached to another task").unwrap().hash;
    attach(&mut store, doomed, &on_task);
    attach(&mut store, survivor, &elsewhere);
    let comment = store.add_task_comment(doomed, ActorKind::Ai, "コメント").unwrap();
    store
        .attach_blob(
            AttachmentTarget::TaskComment, comment.id, &on_comment, "c.bin",
            Some("application/octet-stream"), 0, ActorKind::Ai,
        )
        .unwrap();
    age_blobs(&paths, &[&on_task, &on_comment, &elsewhere]);

    store.delete_task(doomed, amenbo_core::model::ActorKind::Human).unwrap();

    assert!(!store.blobs().has(&on_task), "the deleted task's attachment bytes are gone");
    assert!(!store.blobs().has(&on_comment), "and its comments' attachment bytes with them");
    assert!(store.blobs().has(&elsewhere), "another task's attachment is untouched");
}

/// The targeted reclaim keeps a blob younger than [`amenbo_core::blob::GC_MIN_AGE`] even when its last
/// reference is gone — those bytes may be an attach in flight in another process, about to reference
/// them. Nothing is lost: they are unreferenced garbage the sweep (`doctor --fix`, project teardown)
/// collects once they are old enough.
#[test]
fn a_blob_too_young_to_judge_survives_the_delete_and_falls_to_the_sweep() {
    let paths = temp_paths();
    let mut store = Store::open_at(paths.clone()).unwrap();

    let fresh = store.blobs().ingest_bytes(b"ingested a moment ago").unwrap().hash;
    let a = attach(&mut store, TASK_A, &fresh);

    store.remove_attachment(a).unwrap();
    assert!(store.blobs().has(&fresh), "too young to sweep — an attach elsewhere may be mid-flight");

    let report = store.gc_blobs(Duration::ZERO).unwrap(); // the sweep, once the age guard is satisfied
    assert_eq!(report.removed, 1);
    assert!(!store.blobs().has(&fresh));
}

/// An attachment whose target is gone is unreachable from every surface, and the one thing it still does is
/// keep its hash in the GC root set — so the bytes are not collectible until the row is. `doctor` names it
/// and `sweep_orphan_attachments` (what `doctor --fix` calls, ahead of the blob sweep) is what lets go of
/// both.
#[test]
fn sweeping_an_orphaned_attachment_releases_the_bytes_it_was_holding() {
    let paths = temp_paths();
    let mut store = Store::open_at(paths.clone()).unwrap();

    let project = store.project_add(new_project("PJ")).unwrap().id;
    let live = store.add_task(new_task("生きているタスク", project)).unwrap().id;

    let kept = store.blobs().ingest_bytes(b"on a live task").unwrap().hash;
    let stranded = store.blobs().ingest_bytes(b"on a task that is not there").unwrap().hash;
    attach(&mut store, live, &kept);
    // A target number nothing was ever issued under — the row a forgotten `sweep_polymorphic` leaves.
    attach(&mut store, GONE_TASK, &stranded);
    age_blobs(&paths, &[&kept, &stranded]);

    // The blob sweep alone cannot reach these bytes: the orphan row still holds the hash in the root set.
    assert_eq!(store.gc_blobs(Duration::ZERO).unwrap().removed, 0);
    assert!(store.blobs().has(&stranded));

    // doctor names the row — and only that row.
    let named: Vec<String> = amenbo_core::doctor::report(&store)
        .unwrap()
        .issues
        .iter()
        .filter(|i| i.kind == amenbo_core::validate::DoctorIssueKind::OrphanAttachment)
        .map(|i| i.target.clone())
        .collect();
    assert_eq!(named.len(), 1, "{named:?}");

    assert_eq!(store.sweep_orphan_attachments().unwrap(), 1);
    assert!(!store.blobs().has(&stranded), "with the row gone the bytes are reclaimed");
    assert!(store.blobs().has(&kept), "the live attachment's bytes are untouched");
    assert_eq!(store.sweep_orphan_attachments().unwrap(), 0, "idempotent — nothing is left to sweep");
    assert!(
        amenbo_core::doctor::report(&store)
            .unwrap()
            .issues
            .iter()
            .all(|i| i.kind != amenbo_core::validate::DoctorIssueKind::OrphanAttachment),
        "and the report the repair was raised from comes back clean",
    );
}

/// Attach `hash` to a task and return the attachment id.
fn attach(store: &mut Store, target: i64, hash: &str) -> i64 {
    store
        .attach_blob(
            AttachmentTarget::Task, target, hash, "f.bin", Some("application/octet-stream"), 0, ActorKind::Ai,
        )
        .unwrap()
        .id
}

fn new_project(name: &str) -> amenbo_core::ops::project::NewProject {
    amenbo_core::ops::project::NewProject {
        name: name.to_string(),
        view: amenbo_core::model::View::Board,
        notes: String::new(),
        color: None,
    }
}

fn new_task(title: &str, project_id: i64) -> amenbo_core::ops::task::NewTask {
    amenbo_core::ops::task::NewTask {
        title: title.to_string(),
        project_id: Some(project_id),
        due_on: None,
        start_on: None,
        priority: None,
        notes: String::new(),
        created_by_kind: Some(ActorKind::Ai),
        at_binding_id: None,
    }
}

/// Backdate the blob files past [`amenbo_core::blob::GC_MIN_AGE`] so a sweep judges them on their
/// refcount alone.
fn age_blobs(paths: &Paths, hashes: &[&str]) {
    let old = SystemTime::now() - (amenbo_core::blob::GC_MIN_AGE + Duration::from_secs(60));
    let blobs = paths.base_dir.join(amenbo_core::blob::BLOBS_SUBDIR);
    for hash in hashes {
        let f = std::fs::File::options().write(true).open(blobs.join(hash)).unwrap();
        f.set_modified(old).unwrap();
    }
}

/// The GUI viewer's read-model query returns one target's **live** attachments in attach order, and
/// leaves out the removed ones — mirroring the CLI `attach ls` set off the read-model.
#[test]
fn attachments_for_target_lists_live_in_order() {
    let paths = temp_paths();
    {
        let mut store = Store::open_at(paths.clone()).unwrap();
        let h1 = store.blobs().ingest_bytes(b"one").unwrap().hash;
        let h2 = store.blobs().ingest_bytes(b"two").unwrap().hash;
        // Two live blobs on task-a (h1 before h2 by order_key), one removed again, plus a url on task-a
        // and an unrelated blob on task-b that must not leak in.
        store.attach_blob(
            AttachmentTarget::Task, TASK_A, &h1, "a.txt", Some("text/plain"), 3, ActorKind::Human,
        ).unwrap();
        store.attach_blob(
            AttachmentTarget::Task, TASK_A, &h2, "b.txt", Some("text/plain"), 3, ActorKind::Human,
        ).unwrap();
        let dead = store.attach_blob(
            AttachmentTarget::Task, TASK_A, &h1, "dead.txt", Some("text/plain"), 3, ActorKind::Human,
        ).unwrap();
        store.remove_attachment(dead.id).unwrap();
        store.attach_url(
            AttachmentTarget::Task, TASK_A, "https://example.com", Some("link"), ActorKind::Human,
        ).unwrap();
        attach_blob(&mut store, TASK_B, &h2, false);
    }

    let store = Store::open_at(paths).unwrap();
    let rm = store.read_model();
    let rows = read::attachments_for_target(rm.conn(), "task", TASK_A).unwrap();

    // Three live (h1 blob, h2 blob, url) in attach order; the removed one is gone; task-b absent.
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].kind, "blob");
    assert_eq!(rows[0].filename.as_deref(), Some("a.txt"));
    assert_eq!(rows[1].filename.as_deref(), Some("b.txt"));
    assert_eq!(rows[2].kind, "url");
    assert_eq!(rows[2].url.as_deref(), Some("https://example.com"));
    assert!(rows.iter().all(|r| r.filename.as_deref() != Some("dead.txt")));
}
