//! Data-preservation tests: nothing is lost, nothing is corrupted. Keeping the user's local data
//! intact is amenbo's one non-negotiable duty, so this file pins it as invariants of the truth
//! source `store.sqlite` (WAL plus explicit backup/restore). What is pinned here:
//! - **Concurrent writers.** SQLite's writer exclusion serialises concurrent writes to one store:
//!   no lost update under contention, and a final reopen reads every record back intact.
//! - **Crash injection.** Freeze the on-disk bytes (engine plus WAL/SHM sidecars) mid-flight and
//!   reopen that snapshot: SQLite replays the WAL, every committed record is there, and it passes
//!   `integrity_check` — i.e. a half-written truth source is never served.
//! - **backup → restore round trip.** Restoring an archive over another store reproduces every
//!   record, and it survives a reopen from disk.
//! - **Rollback from the set-aside engine.** `restore` moves the pre-swap engine aside with a
//!   timestamp; even if the process is killed mid-restore, that file is a *complete* recovery point.
//! - **The reach of corruption detection at open.** A corrupt `store.sqlite` is caught at open and
//!   errors out rather than silently becoming an empty store — while a *missing* truth source is
//!   genesis, not corruption. That is the boundary of what open can detect.

use std::path::{Path, PathBuf};

use amenbo_core::config::Paths;
use amenbo_core::store_engine::{hydrate_database, read_format_version, StoreEngine};
use amenbo_core::{archive, ops, Store};

fn temp_base(tag: &str) -> PathBuf {
    let p = amenbo_scratch::scratch(&format!("preserve-{tag}"));
    p
}

fn add_task(store: &mut Store, title: &str) {
    store.add_task(ops::task::NewTask {
        title: title.to_string(),
        project_id: None,
        due_on: None,
        start_on: None,
        priority: None,
        notes: String::new(),
        created_by_kind: None,
    })
    .unwrap();
}

fn live_titles(store: &Store) -> Vec<String> {
    hydrate_database(store.read_model().conn())
        .unwrap()
        .tasks
        .iter()
        
        .map(|t| t.title.clone())
        .collect()
}

fn task_exists(store: &Store, title: &str) -> bool {
    live_titles(store).iter().any(|t| t == title)
}

/// Path to the truth-source engine (`Paths::at` puts every file in one directory, so it is
/// `data_dir/store.sqlite`).
fn db_path(paths: &Paths) -> PathBuf {
    paths.base_dir.join("store.sqlite")
}

/// Progress sink for tests: observes nothing and never cancels.
fn no_progress(_: &amenbo_core::progress::Progress) -> std::ops::ControlFlow<()> {
    std::ops::ControlFlow::Continue(())
}

/// Back the store at `paths` up into a `.amenbo-backup` archive at `dest` (truth source plus
/// attachments), through the exact path a user's `amenbo backup` takes ([`amenbo_core::archive`]).
fn backup_archive(paths: &Paths, dest: &Path) {
    let source = archive::StoreSource { db_path: db_path(paths), bindings: vec![] };
    archive::backup_from(&source, dest, &mut no_progress).expect("the archive can be written");
}

/// Copy a directory tree verbatim — used to snapshot the on-disk bytes as they stand at crash time.
fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&from, &to);
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}

/// Check that a snapshot / engine file is a complete truth source by walking the same path the app
/// takes on startup (open → `integrity_check` → hydrate), and return its live task count. No key is
/// needed: the truth source is plaintext SQLite.
fn hydrated_task_count(file: &Path) -> usize {
    let engine = StoreEngine::open(file).expect("snapshot opens");
    let db = hydrate_database(engine.conn()).expect("snapshot hydrates");
    db.tasks.len()
}

/// SQLite's writer exclusion (one transaction at a time, plus `busy_timeout`) serialises concurrent
/// writes to the same store: many threads opening and adding at once lose no update, and the final
/// reopen reads every record back from an uncorrupted store.
#[test]
fn concurrent_writers_are_serialized_without_loss_or_corruption() {
    let base = temp_base("concurrent");
    // Get genesis out of the way first — it only happens once.
    {
        let mut s = Store::open_at(Paths::at(base.clone())).unwrap();
        add_task(&mut s, "seed");
    }

    const N: usize = 6;
    let handles: Vec<_> = (0..N)
        .map(|i| {
            let base = base.clone();
            std::thread::spawn(move || {
                // Every thread opens the same store, so they queue on the lock and serialise.
                let mut s = Store::open_at(Paths::at(base)).unwrap();
                add_task(&mut s, &format!("concurrent-{i}"));
            })
        })
        .collect();
    for h in handles {
        h.join().expect("writer thread");
    }

    // Reopen: every record is there (no lost update) and the store is intact (hydrate succeeds).
    let reopened = Store::open_at(Paths::at(base)).unwrap();
    assert!(task_exists(&reopened, "seed"));
    for i in 0..N {
        assert!(task_exists(&reopened, &format!("concurrent-{i}")), "concurrent-{i} was lost (lost update)");
    }
}

/// Crash injection: with the live connection still open (so no clean close, no checkpoint), snapshot
/// the on-disk bytes — engine plus WAL/SHM sidecars — and reopen that "killed mid-flight" state.
/// SQLite replays the WAL, every committed record comes back, and it passes `integrity_check` (the
/// startup hydrate): a half-written truth source is never served.
#[test]
fn a_crash_before_checkpoint_reopens_intact_via_wal() {
    let live_base = temp_base("wal-live");
    let live_paths = Paths::at(live_base.clone());

    // Hold the live connection **open** so no clean close (and hence no WAL checkpoint) happens.
    let mut live = Store::open_at(live_paths.clone()).unwrap();
    for i in 0..4 {
        add_task(&mut live, &format!("t{i}"));
    }

    // Copy the disk exactly as a kill would leave it (store.sqlite plus -wal/-shm).
    let crash_base = temp_base("wal-crash");
    copy_tree(&live_base, &crash_base);
    drop(live); // Only now close the live side; the copy holds the pre-close bytes.

    // Reopen the captured crash state: the WAL replays, every commit is back, nothing is corrupt.
    let crash_paths = Paths::at(crash_base);
    let reopened = Store::open_at(crash_paths.clone()).unwrap();
    for i in 0..4 {
        assert!(task_exists(&reopened, &format!("t{i}")), "committed t{i} was not restored by WAL replay");
    }
    drop(reopened);
    // It is also complete along the startup-hydrate path (it passes the integrity check).
    assert_eq!(hydrated_task_count(&db_path(&crash_paths)), 4);
}

/// A backup restored over another store's truth source reproduces every record, and the swap
/// survives a reopen from disk. [`amenbo_core::archive`] is the only road that replaces a truth
/// source: there is no shortcut restore that skips the manifest, the version gate, the migration
/// chain, or the stage-and-swap.
#[test]
fn backup_then_restore_preserves_every_record_across_a_reopen() {
    // Source store (3 tasks) → archive.
    let src_base = temp_base("rt-src");
    let src_paths = Paths::at(src_base.clone());
    let arc = src_base.join(format!("snapshot.{}", archive::ARCHIVE_EXT));
    {
        let mut s = Store::open_at(src_paths.clone()).unwrap();
        for name in ["Alice task", "Bob task", "Carol task"] {
            add_task(&mut s, name);
        }
    }
    backup_archive(&src_paths, &arc);

    // Restore into a different store (1 task), replacing its truth source.
    let dst_base = temp_base("rt-dst");
    let dst_paths = Paths::at(dst_base.clone());
    {
        let mut dst = Store::open_at(dst_paths.clone()).unwrap();
        add_task(&mut dst, "古いタスク");
    }
    archive::restore_into(&arc, "20260714T000000Z", &db_path(&dst_paths), &mut no_progress).unwrap();

    // Reopening from disk serves the swapped-in store: the old task is gone, the 3 are there.
    let reopened = Store::open_at(dst_paths).unwrap();
    let titles = live_titles(&reopened);
    assert_eq!(titles.len(), 3, "restore leaves no old source of truth");
    assert!(["Alice task", "Bob task", "Carol task"].iter().all(|n| titles.contains(&n.to_string())));
    assert!(!task_exists(&reopened, "古いタスク"));
}

/// Two erases within one second each get their own rewind point. The stamp is to the second, and erasing
/// a comment and then a decision is ordinary maintenance typed at ordinary speed — so the second archive is
/// named around the first rather than refused (`AMB-T-2249`), and the first goes only once the new one is
/// on disk, leaving exactly one rewind point behind.
#[test]
fn two_erases_in_one_second_each_get_their_own_rewind_point() {
    let base = temp_base("pre-erase-same-second");
    let paths = Paths::at(base.clone());
    {
        let mut store = Store::open_at(paths.clone()).unwrap();
        add_task(&mut store, "消される前のタスク");
    }
    let source = archive::StoreSource { db_path: db_path(&paths), bindings: Vec::new() };
    let stamp = "20260726T090000Z";

    let first = archive::pre_erase_backup(&source, &base, stamp, &mut no_progress).unwrap();
    assert!(first.superseded.is_empty(), "nothing preceded it");
    assert!(Path::new(&first.backup.path).is_file());

    let second = archive::pre_erase_backup(&source, &base, stamp, &mut no_progress)
        .expect("a second erase in the same second still gets its rewind point");
    assert_ne!(second.backup.path, first.backup.path, "it is named around the one already there");
    assert!(Path::new(&second.backup.path).is_file());
    assert_eq!(
        second.superseded,
        vec![first.backup.path.clone()],
        "and the first is swept, because a rewind point older than this one leads nowhere",
    );
    assert!(!Path::new(&first.backup.path).exists(), "one rewind point per kind, the newest");
}

/// `restore` moves the pre-swap engine aside under a timestamped name. Even if the process is killed
/// before the in-process rollback can run, that file is a **complete recovery point**: it passes the
/// startup hydrate and holds the old data, so moving it back restores the store. Dying mid-restore
/// therefore cannot cost the user their data.
#[test]
fn the_restore_aside_is_a_complete_recovery_point() {
    // Build an archive holding a single task, from an unrelated store.
    let other_base = temp_base("aside-other");
    let other_paths = Paths::at(other_base.clone());
    let arc = other_base.join(format!("other.{}", archive::ARCHIVE_EXT));
    {
        let mut s = Store::open_at(other_paths.clone()).unwrap();
        add_task(&mut s, "差し替え後タスク");
    }
    backup_archive(&other_paths, &arc);

    // Swap that archive over the store we care about (which holds 2 tasks).
    let dst_base = temp_base("aside-dst");
    let dst_paths = Paths::at(dst_base.clone());
    {
        let mut dst = Store::open_at(dst_paths.clone()).unwrap();
        add_task(&mut dst, "守りたい-1");
        add_task(&mut dst, "守りたい-2");
    }

    let report =
        archive::restore_into(&arc, "20260714T000000Z", &db_path(&dst_paths), &mut no_progress).unwrap();
    let aside = PathBuf::from(report.previous_saved_to.expect("the pre-swap engine is set aside"));

    // The set-aside file is a complete recovery point: it hydrates, and holds exactly the 2 pre-swap tasks.
    assert!(aside.exists(), "the set-aside file exists");
    assert_eq!(hydrated_task_count(&aside), 2, "the set-aside holds all pre-swap data");

    // Replay a kill right after the swap: move the set-aside file back over the truth source by hand,
    // exactly as a human (or doctor) would recover.
    let live = db_path(&dst_paths);
    std::fs::remove_file(&live).unwrap();
    std::fs::rename(&aside, &live).unwrap();
    let recovered = Store::open_at(dst_paths).unwrap();
    assert!(task_exists(&recovered, "守りたい-1") && task_exists(&recovered, "守りたい-2"));
    assert!(!task_exists(&recovered, "差し替え後タスク"), "restoring from the set-aside undoes the swap");
}

/// A corrupt `store.sqlite` is caught at open and errors out — it never becomes a silently empty
/// store, because that would hide the fact that data is unrecoverable. The boundary: a *missing*
/// truth source is genesis, not corruption. Open can only detect a file that exists but cannot be
/// read.
#[test]
fn a_corrupt_engine_is_detected_at_open_not_silently_accepted() {
    let base = temp_base("corrupt");
    let paths = Paths::at(base.clone());
    {
        let mut s = Store::open_at(paths.clone()).unwrap();
        add_task(&mut s, "keep-me");
    }

    let engine = db_path(&paths);
    // Overwrite the truth source with bytes SQLite cannot read, physically wrecking the checkpointed
    // main file. Drop the sidecars first so a leftover WAL cannot paper over the damage.
    for ext in ["-wal", "-shm"] {
        let mut side = engine.clone().into_os_string();
        side.push(ext);
        let _ = std::fs::remove_file(PathBuf::from(side));
    }
    std::fs::write(&engine, b"this is not a sqlite database").unwrap();

    // Open detects the corruption and fails, instead of quietly bootstrapping an empty store.
    assert!(Store::open_at(paths.clone()).is_err(), "a corrupted source of truth is detected on open");

    // The boundary: a file that is gone entirely is genesis, not corruption — outside what open detects.
    std::fs::remove_file(&engine).unwrap();
    let fresh = Store::open_at(paths).unwrap();
    assert!(!task_exists(&fresh, "keep-me"), "on loss it is genesis (not subject to corruption detection)");
}

/// The write-path open stamps `FORMAT_VERSION` **only at genesis**. A brand-new store is born in the
/// current shape and needs no step of the chain, so stamping its version records a fact. An existing
/// store's version, by contrast, open must never touch: stamping it would claim a migration ran that
/// did not, letting an unmigrated store pass itself off as current. Only the migration chain
/// (`store_engine::migrate::run`) may advance an existing store's version, and only
/// `migrate::at_startup` runs it.
#[test]
fn open_at_stamps_the_format_version_at_genesis_only() {
    let base = temp_base("format-version");
    let paths = Paths::at(base.clone());

    // Genesis: the first open stamps the version.
    {
        let mut s = Store::open_at(paths.clone()).unwrap();
        add_task(&mut s, "seed");
    }
    let stamped = {
        let engine = StoreEngine::open(&db_path(&paths)).unwrap();
        read_format_version(engine.conn()).unwrap()
    };
    assert_eq!(stamped, amenbo_core::model::FORMAT_VERSION, "opening a genesis stamps the version");
    assert!(stamped >= 1, "HEAD is guard v1 or higher");

    // Existing store: delete the version key so it reads as absent — v0, a store predating the gate.
    // Reopening must not re-stamp it.
    {
        let engine = StoreEngine::open(&db_path(&paths)).unwrap();
        engine.conn().execute("DELETE FROM store_meta WHERE key = 'format_version'", []).unwrap();
        assert_eq!(read_format_version(engine.conn()).unwrap(), 0, "absence is v0 (the compatibility baseline)");
    }
    let _ = Store::open_at(paths.clone()).unwrap();
    let after_reopen = {
        let engine = StoreEngine::open(&db_path(&paths)).unwrap();
        read_format_version(engine.conn()).unwrap()
    };
    assert_eq!(after_reopen, 0, "open does not touch an existing store's version (only migration stamps it)");
}

/// `version_status` reads `format_version` from the truth source, alongside the highest format this
/// build supports. Only the upstream `latest.json` can raise the update flag, and reaching it takes
/// network, so `version_status()` on its own never raises it.
#[test]
fn version_status_reads_format_state_from_the_truth_source() {
    let base = temp_base("version-status");
    let paths = Paths::at(base.clone());

    // Create the store.
    {
        let mut s = Store::open_at(paths.clone()).unwrap();
        add_task(&mut s, "seed");
    }

    let s = Store::open_at(paths.clone()).unwrap();
    let vs = s.version_status();
    assert_eq!(vs.app_version, env!("CARGO_PKG_VERSION"), "the running binary's version");
    assert_eq!(vs.format_version, amenbo_core::model::FORMAT_VERSION, "the store's format version");
    assert_eq!(vs.max_supported_format, amenbo_core::model::FORMAT_VERSION, "this build's max supported");
    assert!(!vs.update_available, "the zero-network standalone check does not flag an update");
    assert_eq!(vs.latest_version, None, "unknown because no upstream has been merged");
}

/// The forward-migration version gate. Simulate an **old binary meeting a newer store** — one
/// migrated past this build's `FORMAT_VERSION` — and check that both the write and the read open path
/// fail immediately with a bilingual hard error, stopping the old code before it reaches for a column
/// or table that no longer exists. The error carries its own code, `format_ahead`: a resident GUI can
/// be overtaken at any moment by an `amenbo migrate` in another process, and it cannot route to a
/// dedicated screen if this is indistinguishable from any other `invalid_value`. The message leads
/// with *restart*, not *update*: everything ships in one installer, so a user who hits this gate
/// already has the newer build on disk.
#[test]
fn open_rejects_a_store_from_a_newer_binary_on_both_paths() {
    let base = temp_base("format-version-gate");
    let paths = Paths::at(base.clone());

    // Create the store (stamped with this build's version).
    {
        let mut s = Store::open_at(paths.clone()).unwrap();
        add_task(&mut s, "seed");
    }

    // Simulate a store a newer amenbo has migrated forward: set its version one above our ceiling.
    let future = amenbo_core::model::FORMAT_VERSION + 1;
    {
        let engine = StoreEngine::open(&db_path(&paths)).unwrap();
        engine
            .conn()
            .execute(
                "INSERT INTO store_meta(key, value) VALUES('format_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [future.to_string()],
            )
            .unwrap();
        assert_eq!(read_format_version(engine.conn()).unwrap(), future, "set the version above the max");
    }

    // Write path: a hard error, with no destructive migration and no re-stamp.
    // (`Store` is not Debug, hence the match rather than `unwrap_err`.)
    let write_err = match Store::open_at(paths.clone()) {
        Ok(_) => panic!("a newer store should be refused on a write open"),
        Err(e) => e,
    };
    assert_eq!(write_err.code(), "format_ahead", "a stable code the GUI can branch to a dedicated screen on");
    assert!(write_err.message_en().contains("newer amenbo"), "the sentence: {}", write_err.message_en());
    assert!(write_err.message_en().contains(&format!("v{future}")), "names the store's version");
    // One installer ships everything, so a user at this gate already has the newer build: tell them
    // to restart, not to update.
    assert!(write_err.message_en().contains("restart amenbo"), "it says restart: {}", write_err.message_en());

    // Read path: refused too — a read reaches for the newer binary's schema just as a write does.
    let read_err = match Store::open_read_at(paths.clone()) {
        Ok(_) => panic!("a newer store should be refused on a read open"),
        Err(e) => e,
    };
    assert_eq!(read_err.code(), "format_ahead", "the read path returns the same code");
    assert!(read_err.message_en().contains("newer amenbo"), "the sentence: {}", read_err.message_en());

    // The gate neither damages nor waves through: the store's version is still `future` — a refused
    // open does not stamp it back down to ours.
    let after = {
        let engine = StoreEngine::open(&db_path(&paths)).unwrap();
        read_format_version(engine.conn()).unwrap()
    };
    assert_eq!(after, future, "a refused open does not rewrite the store's version");
}

/// Snapshot the store's physical schema (the SQL definition of every `sqlite_master` object) over a
/// **raw** connection, without opening the engine. Peering through the engine would run migration
/// DDL and change the very thing we are trying to observe.
fn schema_fingerprint(path: &Path) -> std::collections::BTreeMap<String, String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn.prepare("SELECT type, name, sql FROM sqlite_master").unwrap();
    let rows = stmt
        .query_map([], |r| {
            let (t, name, sql): (String, String, Option<String>) = (r.get(0)?, r.get(1)?, r.get(2)?);
            Ok((format!("{t} {name}"), sql.unwrap_or_default()))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    rows
}

/// Render the disagreement between two schema fingerprints — objects dropped, added, or redefined —
/// as human-readable lines. Showing only the delta (rather than diffing two full dumps) makes a
/// failure say at a glance which DDL ran.
fn schema_diff(
    before: &std::collections::BTreeMap<String, String>,
    after: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (obj, sql) in before {
        match after.get(obj) {
            None => out.push(format!("removed: {obj}")),
            Some(now) if now != sql => out.push(format!("definition changed: {obj}")),
            Some(_) => {}
        }
    }
    out.extend(after.keys().filter(|o| !before.contains_key(*o)).map(|o| format!("appeared: {o}")));
    out
}

/// The version gate is evaluated **before** `StoreEngine::open`. Reading the version *through* the
/// engine and only then failing would be too late: `StoreEngine::open` runs DDL (`ALTER TABLE ADD
/// COLUMN`, `DROP TABLE`) before it hands back a connection, so the old binary's migration would
/// already be applied by the time the hard error fires. The `format_version` scalar is untouched by
/// that, so the test above would still pass — hence this one observes the **physical schema**. To
/// stand in for an old binary, plant a structure only an old binary knows about (the retired `device`
/// table) in a store stamped above our ceiling, and check that every open path refuses it without
/// touching a single byte.
#[test]
fn a_rejected_open_does_not_touch_the_physical_schema() {
    let base = temp_base("format-gate-schema");
    let paths = Paths::at(base.clone());
    let db = db_path(&paths);

    // Create the store.
    {
        let mut s = Store::open_at(paths.clone()).unwrap();
        add_task(&mut s, "seed");
    }

    // Make it look like a store a newer amenbo migrated forward, and plant the structure an old
    // binary's migration would want to touch.
    let future = amenbo_core::model::FORMAT_VERSION + 1;
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE device (id TEXT PRIMARY KEY, label TEXT)").unwrap();
        conn.execute(
            "INSERT INTO store_meta(key, value) VALUES('format_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [future.to_string()],
        )
        .unwrap();
    }
    let before = schema_fingerprint(&db);
    assert!(before.contains_key("table device"), "premise: planted the old structure (a device table)");

    // Write open, read open, init, and the phantom-store check behind `doctor --fix`: all four paths
    // that reach for the engine refuse.
    assert!(Store::open_at(paths.clone()).is_err(), "a write open is refused");
    assert!(Store::open_read_at(paths.clone()).is_err(), "a read open is refused");
    assert!(Store::init(paths.clone(), Some("Alice")).is_err(), "init is refused too");
    assert!(
        amenbo_core::store::store_file_is_content_empty(&db).is_err(),
        "doctor --fix's phantom check is refused too (cannot inspect contents = err on the safe side and skip deletion)"
    );

    // The physical schema is byte-for-byte unchanged: not one line of the old binary's DDL ran.
    let changed = schema_diff(&before, &schema_fingerprint(&db));
    assert!(changed.is_empty(), "a refused open changed the physical schema: {changed:#?}");
}

/// **Neither open runs DDL.** The registry's `CREATE TABLE IF NOT EXISTS` statements are either
/// genesis or a no-op; they never touch an older store's physical schema. Forward migration belongs
/// solely to the chain of numbered, version-bound steps. This matters most on the read path, which
/// takes no lock against other processes: if it migrated, the GUI taking a snapshot would be
/// rewriting the physical schema of a store another process is writing to, with nothing serialising
/// them. So build a store as an older binary would have left it (drop one registry column) and pin
/// that **neither open quietly grows the column back**.
#[test]
fn neither_open_touches_the_physical_schema_of_an_old_store() {
    let base = temp_base("open-no-ddl");
    let paths = Paths::at(base.clone());
    let db = db_path(&paths);

    {
        let mut s = Store::open_at(paths.clone()).unwrap();
        add_task(&mut s, "seed");
    }

    // Drop one column, so the store looks like it was written by a binary that never knew about it.
    // The column carries no index, so dropping it drags no other DDL along.
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("ALTER TABLE dimension_value DROP COLUMN end_on;").unwrap();
    }
    let before = schema_fingerprint(&db);
    assert!(!before["table dimension_value"].contains("end_on"), "premise: dropped a column");

    // Neither open grows the column back — that is, neither issues a single ALTER.
    let _ = Store::open_read_at(paths.clone());
    let _ = Store::open_at(paths.clone());

    let changed = schema_diff(&before, &schema_fingerprint(&db));
    assert!(changed.is_empty(), "open changed the physical schema: {changed:#?}");
}

/// A `query_only = ON` read open can read a store whose writer has **not yet checkpointed its WAL** —
/// the everyday case of a GUI reading (taking no lock) while the CLI writes. `query_only` does not
/// get in the way: replaying the WAL and creating the `-shm` file are the pager layer's business, and
/// only writes issued from a statement are rejected with `SQLITE_READONLY`.
#[test]
fn read_open_reads_through_an_uncheckpointed_wal_held_by_a_writer() {
    let base = temp_base("read-open-wal");
    let paths = Paths::at(base.clone());

    // Keep the writer open, so its committed frames are still sitting in the WAL.
    let mut writer = Store::open_at(paths.clone()).unwrap();
    add_task(&mut writer, "書き手のタスク");

    // The reader opens without taking a lock and reads the writer's commit through the WAL, via a
    // direct read-model SQL query.
    let reader = Store::open_read_at(paths.clone()).unwrap();
    let listed = reader
        .list_tasks(amenbo_core::query::ListParams {
            project_id: None,
            filter_expr: None,
            sort: "created".to_string(),
            limit: None,
            offset: None,
        })
        .unwrap();
    assert!(
        listed.tasks.iter().any(|t| t.title == "書き手のタスク"),
        "the read open cannot see committed writes inside the WAL"
    );
    drop(writer);
}
