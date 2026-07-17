//! The truth source is plain SQLite, with no application-level at-rest encryption: protecting the
//! bytes on the device is the job of the OS's full-disk encryption. Two things follow, and this file
//! pins both: (1) an ordinary open reads a plaintext store, the live engine stays plaintext, and
//! backup/restore ([`archive`]) round-trips without a key; (2) a legacy SQLCipher store can no longer
//! be migrated back to plaintext, so the write-path open fails with an explicit error rather than
//! guessing. `encrypt_engine_in_place` exists only to fabricate such a legacy store — it is **test
//! scaffolding**, with no counterpart in the library.

use amenbo_core::archive;
use amenbo_core::config::Paths;
use amenbo_core::ops::task::NewTask;
use amenbo_core::store::Store;

/// Progress sink for tests: observes nothing and never cancels.
fn no_progress(_: &amenbo_core::progress::Progress) -> std::ops::ControlFlow<()> {
    std::ops::ControlFlow::Continue(())
}

/// A scratch directory for one test, wiped clean before use.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("amenbo-atrest-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Does the file start with the plaintext SQLite header (`SQLite format 3\0`)? An encrypted database
/// opens with a random salt instead, so this reads false for one.
fn plaintext_header(path: &std::path::Path) -> bool {
    std::fs::read(path).map(|b| b.starts_with(b"SQLite format 3\0")).unwrap_or(false)
}

const KEY: [u8; 32] = [7u8; 32];

fn new_task(title: &str) -> NewTask {
    NewTask {
        title: title.into(),
        project_id: None,
        due_on: None,
        start_on: None,
        priority: None,
        notes: String::new(),
        created_by_kind: None,
    }
}

/// Fabricate the legacy state: SQLCipher-encrypt the **plaintext** engine at `path` under `key` and
/// leave the result in its place. Test scaffolding with no counterpart in the library — it exists to
/// produce the legacy store the open paths must refuse.
fn encrypt_engine_in_place(path: &std::path::Path, key: &[u8; 32]) {
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    let key_literal = format!("x'{hex}'");
    let tmp = path.with_extension("enc-tmp");
    let _ = std::fs::remove_file(&tmp);
    {
        let src = rusqlite::Connection::open(path).unwrap(); // plaintext source: no PRAGMA key
        src.execute(
            "ATTACH DATABASE ?1 AS enc KEY ?2",
            rusqlite::params![tmp.to_str().unwrap(), key_literal],
        )
        .unwrap();
        src.query_row("SELECT sqlcipher_export('enc')", [], |_| Ok::<_, rusqlite::Error>(())).unwrap();
        src.execute_batch("DETACH DATABASE enc").unwrap();
    }
    // Drop the plaintext original's WAL/SHM sidecars before swapping the encrypted copy in.
    for ext in ["-wal", "-shm"] {
        let mut side = path.as_os_str().to_os_string();
        side.push(ext);
        let _ = std::fs::remove_file(std::path::PathBuf::from(side));
    }
    std::fs::rename(&tmp, path).unwrap();
}

/// An ordinary open reads a plaintext store and the live engine stays plaintext. A backup
/// ([`archive`]) and the restore back out of it round-trip in plaintext too, with no key anywhere.
#[test]
fn backup_and_restore_round_trip_a_plaintext_store() {
    let dir = scratch("backup");
    let paths = Paths::at(dir.clone());
    let live = paths.base_dir.join("store.sqlite");
    {
        let mut s = Store::open_at(paths.clone()).unwrap();
        for name in ["alice", "bob"] {
            s.add_task(new_task(name)).unwrap();
        }
    }
    assert!(plaintext_header(&live), "the live truth source is plaintext at rest");

    // Backup: a VACUUM INTO off the plaintext live connection lands in the archive.
    let arc = dir.join(format!("snap.{}", archive::ARCHIVE_EXT));
    let source = archive::StoreSource { db_path: live.clone(), bindings: vec![] };
    archive::backup_from(&source, &arc, &mut no_progress).unwrap();

    // Restore: put the archive back and the truth source completes the round trip. Verify the swap by
    // reopening the engine from disk. Still no key.
    archive::restore_into(&arc, "20260714T000000Z", &live, &mut no_progress).unwrap();
    assert!(plaintext_header(&live), "the restored truth source is plaintext");

    let reopened = Store::open_at(Paths::at(dir.clone())).unwrap();
    let db = amenbo_core::store_engine::hydrate_database(reopened.read_model().conn()).unwrap();
    let titles: std::collections::BTreeSet<&str> =
        db.tasks.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains("alice") && titles.contains("bob"), "restored store serves its tasks");
}

/// With no at-rest key derivation left, a legacy SQLCipher store can no longer be migrated back to
/// plaintext. Opening one on the write path fails with an explicit error and leaves the bytes on disk
/// exactly as they were.
#[test]
fn legacy_encrypted_truth_source_errors_on_open() {
    let dir = scratch("encrypted-errors");
    let paths = Paths::at(dir.clone());
    let engine = paths.base_dir.join("store.sqlite");

    // 1. Create a plaintext store the ordinary way and save one task into it.
    {
        let mut s = Store::open_at(paths.clone()).unwrap();
        s.add_task(new_task("legacy")).unwrap();
        assert!(plaintext_header(&engine), "a new store is plaintext");
    }

    // 2. Fabricate the legacy state: encrypt the plaintext engine in place under a key.
    encrypt_engine_in_place(&engine, &KEY);
    assert!(!plaintext_header(&engine), "the fabricated legacy store is encrypted");

    // 3. Reopen: with no decryption migration, this must error out and leave the ciphertext alone.
    let err = match Store::open_at(paths.clone()) {
        Ok(_) => panic!("an at-rest-encrypted store must not open"),
        Err(e) => e,
    };
    assert_eq!(err.code(), "invalid_value", "an at-rest-encrypted store errors on open");
    assert!(!plaintext_header(&engine), "the encrypted bytes are left untouched");
}
