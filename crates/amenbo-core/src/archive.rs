//! Whole-device backup: bundle **everything on this device** into a single self-contained
//! `.amenbo-backup` archive.
//!
//! The device holds **one database** — [`enumerate_store`] returns one source or none — and that is the
//! only shape this module can even spell: the manifest carries **a store**, not a list of them, and the
//! archive's own paths mirror the live tree. An archive written for the older layout of N stores (v1–v4) is
//! a thing to **refuse** ([`read_manifest`]), not a thing to read. A device whose store still carries that
//! layout is caught at `open` ([`has_legacy_layout`]) and told which build can fold it, rather than being
//! shown a device with no store.
//!
//! The archive is an uncompressed tar with, in order: `manifest.json` ([`ArchiveManifest`]), written
//! **first** so a restore can read the store's generation and run its pre-flight `format_version` gate
//! **without extracting** the (potentially large) snapshot; the store's `VACUUM INTO` snapshot at
//! `store.sqlite`; and the attachment bytes beside it, `blobs/<hash>`.
//!
//! The engine carries only attachment *metadata*; the bytes live out-of-band in the store's
//! content-addressed [`crate::blob`] store. An archive without them would restore rows pointing at
//! files that do not exist on the destination machine, so every blob is bundled. `blobs/tmp/` (ingest
//! staging) is never bundled — [`crate::blob::BlobStore::list`] only ever reports hash-named files.
//! Bytes are **not** re-hashed on the way in or out: the file's
//! name *is* its content-address, and verifying it would cost O(total bytes) — the same trade-off the
//! bounded snapshot verify accepts.
//!
//! The [`StoreEntry`] records the store's frozen `schema_version` and its monotonic
//! `format_version` (the axis the restore gate keys on), read straight from the source store's
//! `store_meta` — plus the producing binary's app + format version at the archive level.
//!
//! The snapshot is produced with `VACUUM INTO` (a checkpointed physical copy) and streamed into the tar
//! file-by-file, so archive memory stays bounded regardless of store size. It is verified
//! **bounded** — `integrity_check` + a table-existence + a `COUNT` probe — never a full hydrate (which
//! would be O(rows) memory). Progress is reported through the shared [`crate::progress`]
//! callback; cancellation is observed at the phase boundaries.
//!
//! What a snapshot must contain depends on who is asking, so the basis is split by function:
//! [`verify_snapshot_mirrors_source`] is for a **copy of a live store** (a backup, including the
//! pre-migration one), and its basis is the source's own tables — the copy may be of any generation, since
//! a faithful copy of an old store is what a rewind point *is*. [`verify_snapshot_current_schema`] is for a
//! snapshot the version chain has **carried to this build** (the staged migrated database, a restored
//! archive's snapshot), and its basis is this build's datasets.
//!
//! Machine-local, non-portable state (`identity.json` — the display name and `bound_hw`) is
//! **not** bundled: a restore must not overwrite the destination machine's identity.
//!
//! [`restore_into`] is the read side: a **destructive replace of the database the archive carries**
//! (a point-in-time rewind). An archive from before the consolidation is refused outright at
//! [`read_manifest`] — its manifest speaks of N stores this build has nowhere to put. What the envelope
//! guarantees is that a forward migration (a write) can never leave a **partial** application:
//! 1. **Pre-flight generation gate** (before touching the archive or the live tree): read the manifest
//!    only, and if the store's `format_version` exceeds this build's [`crate::model::FORMAT_VERSION`],
//!    refuse the archive with the "update" error — nothing is changed.
//! 2. **Stage + migrate + verify**: extract the snapshot to a staging dir and, *in staging*, re-gate on the
//!    real stamped generation (the authoritative check; the manifest is advisory), run the **version
//!    chain** forward from the version the snapshot carries — which is what lets an archive taken
//!    by an older build still be restored by this one — and bounded-verify the result
//!    (`integrity_check` + COUNT, never a full hydrate). The live tree is untouched, so any failure here
//!    aborts with nothing to roll back; the chain needs no pre-migration backup of its own, because the
//!    thing it migrates is a copy in a staging dir and the archive itself is the "before".
//! 3. **Swap** (only once the snapshot is green): place the archive's blobs (additive — a hash already
//!    present is left alone), then clear the handles that would block the replace (this process lets go
//!    of what it keeps open, another program's connection is refused with `store_busy` — `AMB-D-704`,
//!    the demand Windows makes and Unix does not), then [`checkpoint`] the live `store.sqlite` and copy
//!    it aside to a timestamped `store.pre-restore-<stamp>.sqlite` (kept as the safety backup on
//!    success; the checkpoint is what makes that single file carry the WAL's commits) and rename the
//!    staged, migrated snapshot over it ([`replace_truth_source`] — the live path is never absent). An
//!    I/O failure puts the aside back, so a failed restore never leaves a half-replaced store. Blobs
//!    are *not* rolled back: they are content-addressed and additive, so a stray one is unreferenced
//!    garbage that [`crate::store::Store::gc_blobs`] reclaims — whereas removing one could destroy
//!    bytes a live attachment still points at.
//!
//! The primitives the envelope is built from — `snapshot_into`, the two verifies, `move_file`,
//! `remove_sidecars`, `checkpoint`, `DirGuard` — are `pub(crate)`: a restore is where they were proven,
//! and a migration is meant to reach for them rather than grow its own.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{sqlite_at, Error, ErrorCode, Msg, Result};
use crate::progress::{Phase, Progress};
use crate::store_engine::migrate::{self as chain, Step};
use crate::swap_lock;

/// File extension of a whole-device archive (`<name>.amenbo-backup`).
pub const ARCHIVE_EXT: &str = "amenbo-backup";

/// Filename prefix of the aside a restore sets the store it replaced to (`store.pre-restore-<stamp>.sqlite`)
/// — the store's rewind point, and what [`sweep_superseded`] recognises an older one by.
const PRE_RESTORE_PREFIX: &str = "store.pre-restore-";

/// Filename prefix of the rewind point `hard-erase` takes before it destroys content
/// (`pre-erase-<stamp>.amenbo-backup`) — what [`sweep_superseded`] recognises an older one by.
const PRE_ERASE_PREFIX: &str = "pre-erase-";

/// Manifest entry name inside the archive. Written first (see the module docs).
pub const MANIFEST_ENTRY: &str = "manifest.json";

/// Layout version of the archive itself (bumped only if the *container* layout changes — distinct from
/// the store's `format_version`, which tracks store schema generations). **This build reads v5 and nothing
/// else**: v1–v4 are the multi-store container, whose manifest carries a `stores` **list** naming a store
/// id and the `stores/<id>/…` path each snapshot took inside the tar — a shape a build with one store has
/// no type for, so [`read_manifest`] refuses them by version and names the build that can read them rather
/// than dying in `serde` on a missing field. In v5 the manifest carries **one** store ([`StoreEntry`], no
/// id and no path), the snapshot is [`SNAPSHOT_ENTRY`] and its attachment bytes are `blobs/<hash>` — the
/// archive names things the way the live tree does.
const ARCHIVE_LAYOUT_VERSION: u32 = 5;

/// The first layout this build can read — see [`ARCHIVE_LAYOUT_VERSION`].
const MIN_ARCHIVE_LAYOUT_VERSION: u32 = 5;

/// The store's snapshot inside the tar. A constant, not a manifest field: the archive holds one store, so
/// there is nothing for a path to select between.
const SNAPSHOT_ENTRY: &str = "store.sqlite";

/// The archive prefix under which the store's attachment bytes live — `blobs/<hash>`, mirroring the live
/// blob dir beside the truth source.
fn blobs_prefix() -> String {
    format!("{}/", crate::blob::BLOBS_SUBDIR)
}

/// The store as recorded in the archive manifest: its generation and what it carries. The generation
/// fields (`schema_version` / `format_version`) are the part the restore gate reads; `bindings` is
/// overview metadata carried for restore UX.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreEntry {
    /// Folder paths bound to this store, when known — informational for restore.
    pub bindings: Vec<String>,
    /// Frozen schema version (`"1"`); recorded for completeness/forward-proofing.
    pub schema_version: String,
    /// Monotonic store format version — the axis the restore pre-flight gate compares against
    /// the restoring binary's `FORMAT_VERSION`. Missing in the source reads as `0` (v0 baseline).
    pub format_version: i64,
    /// How many attachment blobs the store contributed, and their total byte size — recorded
    /// for restore UX ("why is this archive 3 GB?"). **Advisory**: counted before the bytes are
    /// streamed, so a concurrent ingest can shift it. The restore drives off the entries it actually
    /// extracts, never off these.
    pub blob_count: u64,
    /// See [`Self::blob_count`].
    pub blob_bytes: u64,
}

/// The archive manifest (`manifest.json`). Written first so a restore can gate on the generation before
/// extracting the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveManifest {
    /// Container layout version (see [`ARCHIVE_LAYOUT_VERSION`]).
    pub archive_layout_version: u32,
    /// When the archive was produced (RFC3339).
    pub created_at: String,
    /// The producing binary's human-readable version ([`crate::agent::VERSION`]).
    pub producer_app_version: String,
    /// The producing binary's [`crate::model::FORMAT_VERSION`] — the max generation it could stamp.
    pub producer_format_version: i64,
    /// The store this archive carries. **One**: the device holds a single database, so the manifest says
    /// so in its type rather than in a runtime check on a list of length one.
    pub store: StoreEntry,
}

/// Just enough of the manifest to gate on before the real one is parsed — the layout version, which decides
/// whether the rest of the JSON is a shape this build has a type for at all ([`read_manifest`]).
#[derive(Deserialize)]
struct ManifestEnvelope {
    archive_layout_version: u32,
}

/// The store to bundle: where its truth-source file lives plus the overview metadata to record. Kept
/// separate from the [`crate::config::Paths`] enumeration so the backup is unit-testable against
/// hand-built stores and the OS-layout glue ([`enumerate_store`]) stays thin.
#[derive(Debug, Clone)]
pub struct StoreSource {
    /// Absolute path to the store's `store.sqlite` truth source.
    pub db_path: PathBuf,
    /// Folder bindings to record, if any.
    pub bindings: Vec<String>,
}

/// Outcome of a whole-device backup: where the archive landed, its on-disk size, and how many attachment
/// blobs it bundled.
#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    /// Absolute-or-given path the archive was written to.
    pub path: String,
    /// Archive size in bytes.
    pub bytes: u64,
    /// Number of attachment blobs bundled.
    pub blobs: u64,
}

/// Deletes a directory tree on drop — cleans up the `VACUUM INTO` staging dir even on the early-return
/// / error paths.
pub(crate) struct DirGuard(pub(crate) PathBuf);
impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A store's live blob root: `blobs/` beside its truth source (mirroring [`crate::store::Store::blobs`],
/// which roots at the store's data dir). Used symmetrically by backup (source) and restore
/// (destination), so neither side needs its own path convention.
pub(crate) fn blobs_dir_of(db_path: &Path) -> Option<PathBuf> {
    db_path.parent().map(|dir| dir.join(crate::blob::BLOBS_SUBDIR))
}

/// Every blob physically present in a store, or an empty list when the store has no blob dir (never
/// attached anything). Skips `blobs/tmp/` and any stray non-hash file.
pub(crate) fn list_blobs(db_path: &Path) -> Result<Vec<crate::blob::BlobRef>> {
    match blobs_dir_of(db_path) {
        Some(dir) => crate::blob::BlobStore::at(dir).list(),
        None => Ok(Vec::new()),
    }
}

/// Read a source store's `(schema_version, format_version)` from its `store_meta` — a read-only open,
/// no migration/stamp (backup must never mutate the source). A missing `format_version` reads as `0`
/// (the v0 baseline); a missing `schema_version` falls back to the frozen constant.
fn read_generation(db_path: &Path) -> Result<(String, i64)> {
    let conn = Connection::open(db_path).map_err(sqlite_at(db_path))?;
    let format = crate::store_engine::read_format_version(&conn).map_err(sqlite_at(db_path))?;
    let schema = crate::store_engine::read_meta(&conn, crate::store_engine::META_SCHEMA_VERSION)
        .map_err(sqlite_at(db_path))?
        .unwrap_or_else(|| crate::model::SCHEMA_VERSION.to_string());
    Ok((schema, format))
}

/// `VACUUM INTO` a checkpointed physical snapshot of the store at `src` to `dest` (which must not
/// exist). Reads through a fresh connection — no migration, no stamp — so the source is untouched.
pub(crate) fn snapshot_into(src: &Path, dest: &Path) -> Result<()> {
    let conn = Connection::open(src).map_err(sqlite_at(src))?;
    let dest_str = dest.to_str().ok_or_else(|| {
        Error::invalid(format!("snapshot path is not valid UTF-8: {}", dest.display()))
    })?;
    conn.execute("VACUUM INTO ?1", [dest_str]).map_err(sqlite_at(src))?;
    Ok(())
}

/// `integrity_check` — structural health of the database file, whatever generation it is of. `at` is the
/// file `conn` was opened on: a connection cannot say where it came from, and a caller needs the failure to
/// name the file.
fn integrity_check(conn: &Connection, at: &Path) -> Result<()> {
    let problems: Vec<String> = conn
        .prepare("PRAGMA integrity_check")
        .map_err(sqlite_at(at))?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sqlite_at(at))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_at(at))?
        .into_iter()
        .filter(|s| s != "ok")
        .collect();
    if problems.is_empty() {
        return Ok(());
    }
    let (file, problems) = (at.display(), problems.join("; "));
    Err(Error::invalid(format!("{file} failed integrity_check: {problems}")))
}

// ── the physical-schema probes ──
//
// The four helpers below are **raw SQL by necessity**. They exist to read a store file *as it actually is*
// — whichever generation it is of, before any migration has run — so they read SQLite's own catalogue
// (`sqlite_master`, `PRAGMA table_info`) and take the table as a **name** the caller walked in with. The
// registry describes the schema this binary migrates *to*, which is the one thing a snapshot's
// verification must not assume: naming a column through `col::` would make an older store fail
// verification for holding exactly what it is supposed to hold.

/// Whether the database at `at` — open as `conn` — has a table by this name.
fn has_table(conn: &Connection, at: &Path, table: &str) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )
    .map_err(sqlite_at(at))
}

/// Bounded `COUNT` — prove the table answers through a real query path. O(1) memory (no `Vec`).
fn count_probe(conn: &Connection, at: &Path, table: &str) -> Result<()> {
    let _n: i64 = conn
        .query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| r.get(0))
        .map_err(sqlite_at(at))?;
    Ok(())
}

/// The column names the table actually holds. Bounded by the table's width (a few dozen names), which is
/// the schema's, not the data's — so this stays O(1) in the store's size.
fn columns_of(conn: &Connection, at: &Path, table: &str) -> Result<Vec<String>> {
    conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .map_err(sqlite_at(at))?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(sqlite_at(at))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_at(at))
}

/// Every table the store at `path` actually holds — its own read model, whichever generation that is.
/// SQLite's internal tables (`sqlite_*`) are excluded by name; every other table is kept, including the
/// ones an older generation left behind, so a snapshot carries the store as it is.
fn tables_of(path: &Path) -> Result<Vec<String>> {
    let conn = Connection::open(path).map_err(sqlite_at(path))?;
    let names = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(sqlite_at(path))?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sqlite_at(path))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sqlite_at(path))?;
    Ok(names)
}

/// Prove a snapshot is usable **without** a full hydrate, against **this build's** read model:
/// `integrity_check` for structural health, then — for the store kind's datasets — every expected table
/// exists, holds every column the registry declares, and answers a `COUNT` (a real query path, O(1)
/// memory). The columns are the part that bites: a missing *table* never reaches here (`StoreEngine::open`
/// recreates it with `CREATE TABLE IF NOT EXISTS`), but a missing *column* survives — a `COUNT` answers
/// happily on a table of the wrong generation — so without this check the last gate before a destructive
/// swap would pass a snapshot the version chain forgot to carry, and the store would fail at the first read
/// with `no such column` instead of here, with the archive still intact. This basis is right for a snapshot
/// the version chain has carried to this build ([`stage_snapshot`]) and **wrong** for a copy of a store
/// this build has never opened (a backup of an older store): that store predates every table a later build
/// added, and demanding them would refuse a perfectly faithful copy — use
/// [`verify_snapshot_mirrors_source`] there.
pub(crate) fn verify_snapshot_current_schema(path: &Path) -> Result<()> {
    let conn = Connection::open(path).map_err(sqlite_at(path))?;
    integrity_check(&conn, path)?;

    for d in crate::store_engine::schema::DATASETS {
        if !has_table(&conn, path, d.name)? {
            let (file, table) = (path.display(), d.name);
            return Err(Error::invalid(format!("snapshot {file} is missing read-model table `{table}`")));
        }
        let held = columns_of(&conn, path, d.name)?;
        if let Some(missing) = d.all_columns().map(|c| c.name).find(|c| !held.iter().any(|h| h == c)) {
            let (file, table) = (path.display(), d.name);
            return Err(Error::invalid(
                format!("snapshot {file} table `{table}` is missing column `{missing}` — it is of an older generation than this build's read model"),
            ));
        }
        count_probe(&conn, path, d.name)?;
    }
    Ok(())
}

/// Prove a snapshot faithfully mirrors the store it was copied from — bounded the same way, but with the
/// **source's own** table list as the basis: `integrity_check`, then every table the source holds exists in
/// the snapshot and answers a `COUNT`. This is the verify a **backup** wants, because a backup copies
/// whatever generation the store is of — most sharply the pre-migration backup, whose whole purpose is to
/// capture a store *older* than this build. Verifying that against this build's datasets would ask a v2
/// store for a table v3 invented, failing the copy rather than the copy failing: every additively-added
/// table would break the backup path, and therefore the migration that must back up first.
pub(crate) fn verify_snapshot_mirrors_source(snapshot: &Path, source: &Path) -> Result<()> {
    let conn = Connection::open(snapshot).map_err(sqlite_at(snapshot))?;
    integrity_check(&conn, snapshot)?;

    for table in tables_of(source)? {
        if !has_table(&conn, snapshot, &table)? {
            let (snap, src) = (snapshot.display(), source.display());
            return Err(Error::invalid(
                format!("snapshot {snap} is missing table `{table}`, which the store it copied ({src}) holds"),
            ));
        }
        count_probe(&conn, snapshot, &table)?;
    }
    Ok(())
}

/// Create (and return) a fresh staging directory next to `dest` for the `VACUUM INTO` outputs.
fn staging_dir(dest: &Path) -> Result<PathBuf> {
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let dir = parent.join(format!(".amenbo-backup-stage-{}", crate::tmpdir::suffix()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Error returned when a progress callback asks to cancel a backup.
fn backup_cancelled() -> Error {
    Error::invalid("backup cancelled")
}

/// Error returned when a progress callback asks to cancel a restore. Its own wording: a cancelled restore
/// that says "backup cancelled" names an operation the user did not start.
fn restore_cancelled() -> Error {
    Error::invalid("restore cancelled")
}

/// Back up this device's store into a single `.amenbo-backup` archive at `dest`.
///
/// Refuses to overwrite an existing `dest` (the caller owns rotation). Writes `manifest.json` first,
/// then streams the store's verified `VACUUM INTO` snapshot into the tar. On **any** failure — a
/// store that won't open/verify, an I/O error, or a progress cancellation — the partial archive is
/// removed so a failed backup never leaves a file that looks usable. The staging snapshot is cleaned up
/// unconditionally.
pub fn backup_from(
    source: &StoreSource,
    dest: &Path,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<BackupReport> {
    // Named before the overwrite guard, which would otherwise call a directory an "existing archive"
    // and send the user looking for a backup that is not there. Nothing in `backup` says it wants a
    // file, and `export` — right next to it — takes a directory, so a directory here is a plausible
    // mistake, not a careless one.
    if dest.is_dir() {
        return Err(Error::Invalid(
            Msg::new(format!(
                "destination is a directory — backup writes one archive file: {}",
                dest.display()
            ))
            .coded(ErrorCode::InvalidBackupDestIsDir)
            .with("path", dest.display()),
        ));
    }
    if dest.exists() {
        return Err(Error::Invalid(
            Msg::new(format!("refusing to overwrite existing archive: {}", dest.display()))
                .coded(ErrorCode::InvalidBackupDestExists)
                .with("path", dest.display()),
        ));
    }

    let stage = staging_dir(dest)?;
    let _guard = DirGuard(stage.clone());

    // Pass 1: read the store's generation (read-only) and lay out the manifest. Doing this before the
    // snapshot means the manifest is complete and can be written first.
    let (schema_version, format_version) = read_generation(&source.db_path)?;
    // Counted here and dropped: peak memory stays at one blob listing, and the (advisory) totals reach
    // the manifest, which is written before the snapshot.
    let blobs = list_blobs(&source.db_path)?;
    let manifest = ArchiveManifest {
        archive_layout_version: ARCHIVE_LAYOUT_VERSION,
        created_at: crate::time::Timestamp::now().0.to_rfc3339(),
        producer_app_version: crate::agent::VERSION.to_string(),
        producer_format_version: crate::model::FORMAT_VERSION,
        store: StoreEntry {
            bindings: source.bindings.clone(),
            schema_version,
            format_version,
            blob_count: blobs.len() as u64,
            blob_bytes: blobs.iter().map(|b| b.size_bytes).sum(),
        },
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;

    // Pass 2: build the archive — manifest first, then the verified snapshot plus the blobs.
    let mut blobs_bundled = 0u64;
    let build = (|| -> Result<u64> {
        let file = File::create(dest)?;
        let mut builder = tar::Builder::new(BufWriter::new(file));
        append_bytes(&mut builder, MANIFEST_ENTRY, &manifest_json)?;

        if progress(&Progress { phase: Phase::Snapshotting, done: 0, total: Some(1) }).is_break() {
            return Err(backup_cancelled());
        }
        let snap = stage.join(SNAPSHOT_ENTRY);
        snapshot_into(&source.db_path, &snap)?;

        let _ = progress(&Progress { phase: Phase::Verifying, done: 0, total: Some(1) });
        verify_snapshot_mirrors_source(&snap, &source.db_path)?;

        append_file(&mut builder, SNAPSHOT_ENTRY, &snap)?;
        // Free the staging snapshot as we go so peak staging disk is one copy of the store, not two.
        let _ = std::fs::remove_file(&snap);

        // The attachment bytes, streamed straight from the live blob store (no staging copy — they
        // are immutable, content-addressed files).
        blobs_bundled += append_blobs(&mut builder, source, progress)?;

        builder.into_inner()?.flush()?;
        Ok(std::fs::metadata(dest)?.len())
    })();

    match build {
        Ok(bytes) => Ok(BackupReport {
            path: dest.display().to_string(),
            bytes,
            blobs: blobs_bundled,
        }),
        Err(e) => {
            let _ = std::fs::remove_file(dest);
            Err(e)
        }
    }
}

/// Append raw bytes to the tar under `name` (used for `manifest.json`).
fn append_bytes<W: Write>(builder: &mut tar::Builder<W>, name: &str, bytes: &[u8]) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, name, bytes).map_err(Error::from)
}

/// Append a file's contents to the tar under `name`, streaming from disk (bounded memory).
fn append_file<W: Write>(builder: &mut tar::Builder<W>, name: &str, path: &Path) -> Result<()> {
    let mut f = File::open(path)?;
    builder.append_file(name, &mut f).map_err(Error::from)
}

/// Stream the store's attachment bytes into the tar at `blobs/<hash>`, returning how
/// many were appended. Ticks [`Phase::Blobs`] per
/// blob (`done`/`total` count blobs) and honours cancellation there; the bytes are what a large archive
/// spends its time on. A no-op for a store that has never attached anything.
fn append_blobs<W: Write>(
    builder: &mut tar::Builder<W>,
    src: &StoreSource,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<u64> {
    let Some(dir) = blobs_dir_of(&src.db_path) else {
        return Ok(0);
    };
    let prefix = blobs_prefix();
    let store = crate::blob::BlobStore::at(dir.clone());
    let blobs = store.list()?;
    let total = blobs.len() as u64;
    for (j, b) in blobs.iter().enumerate() {
        if progress(&Progress { phase: Phase::Blobs, done: j as u64, total: Some(total) }).is_break() {
            return Err(backup_cancelled());
        }
        // Resolve the real on-disk path and name the entry flat — the archive is always written in the
        // current layout.
        let Some(path) = store.path(&b.hash) else {
            continue; // listed then vanished
        };
        append_file(builder, &format!("{prefix}{}", b.hash), &path)?;
    }
    Ok(total)
}

/// What the rewind point `hard-erase` stands on cost: the archive it wrote, and the older ones that
/// archive superseded and which it therefore deleted ([`sweep_superseded`]).
#[derive(Debug, Clone, Serialize)]
pub struct PreEraseReport {
    /// The rewind point this erase can be undone from (`amenbo restore`).
    pub backup: BackupReport,
    /// The pre-erase archives from earlier erases this one superseded. Empty when there were none.
    pub superseded: Vec<String>,
}

/// Take the rewind point a `hard-erase` stands on: a verified archive of the whole store at
/// `<dir>/pre-erase-<stamp>.amenbo-backup`, written **before** the destructive step so a botched erase
/// has the one way back there is (`amenbo restore`).
///
/// The archive is a rewind point like the pre-migration and pre-restore ones, so it is swept like them:
/// the moment this one exists, the pre-erase archives from earlier erases are not rewind points any more —
/// going back to one would roll back everything done since — and each is a whole copy of the store
/// *carrying the very content an erase was asked to destroy*. So the maker sweeps them here, right after
/// its own is on disk, and reports what went.
pub fn pre_erase_backup(
    source: &StoreSource,
    dir: &Path,
    stamp: &str,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<PreEraseReport> {
    let dest = free_archive_path(dir, PRE_ERASE_PREFIX, stamp);
    let backup = backup_from(source, &dest, progress)?;
    let superseded = sweep_superseded(dir, PRE_ERASE_PREFIX, &format!(".{ARCHIVE_EXT}"), &dest);
    Ok(PreEraseReport { backup, superseded })
}

/// Where the next rewind point of one kind goes: `<dir>/<prefix><stamp>.amenbo-backup`, or the next free
/// name beside it (`…-2`, `…-3`) when a file of that name is already there.
///
/// The stamp is to the second, and a second archive of the same kind inside one second is ordinary use,
/// not an edge: a comment erased and then a decision, typed as fast as anyone types (`pre-erase-`), or a
/// migration that fails and is retried at once, which a small store finishes well inside the second
/// (`pre-migrate-`). Without this the second one dies on [`backup_from`]'s refuse-to-overwrite, having
/// done nothing it was asked to do, and the user is left reading an error about an archive they never
/// asked for. Naming around the one already there rather than deleting it first is what keeps the window
/// closed: it is still the only way back to what came before, and it goes only once the new rewind point
/// is on disk ([`sweep_superseded`], which matches these names too, so "the newest one only" holds).
///
/// The bound is a formality — it takes 99 archives in one second to reach it — and past it the plain name
/// comes back, so the caller meets the overwrite guard's own honest error rather than a loop.
pub(crate) fn free_archive_path(dir: &Path, prefix: &str, stamp: &str) -> PathBuf {
    let at = |suffix: String| dir.join(format!("{prefix}{stamp}{suffix}.{ARCHIVE_EXT}"));
    let first = at(String::new());
    if !first.exists() {
        return first;
    }
    (2..100).map(|n| at(format!("-{n}"))).find(|p| !p.exists()).unwrap_or(first)
}

/// This device's store, read from the on-disk layout: **one database** (`<app-data>/store.sqlite`, or
/// `<AMENBO_HOME>/store.sqlite`). `None` when the store file does not exist. Folder bindings are
/// informational in the manifest and are read best-effort from the store's own binding tables (a
/// corrupt/locked store simply records none).
pub fn enumerate_store() -> Option<StoreSource> {
    let db_path = crate::config::resolve_store_file(&crate::config::Paths::user_base());
    if !db_path.is_file() {
        return None;
    }
    let bindings = Connection::open(&db_path)
        .ok()
        .map(|c| crate::overview::load_bindings(&c).all_dirs())
        .unwrap_or_default();
    Some(StoreSource { db_path, bindings })
}

/// The legacy subdirectory that held the N project stores (`<app-data>/stores/<store_id>/`).
const LEGACY_STORES_SUBDIR: &str = "stores";
/// The legacy subdirectory that held the root overview store (`<app-data>/root/`).
const LEGACY_ROOT_SUBDIR: &str = "root";

/// Does `base` still hold the legacy layout — N project stores under `stores/<id>/`, the root overview
/// store under `root/`? This is what separates an unfolded layout from a fresh install: this build cannot
/// migrate such a device, so the one thing it must not do is mistake it for a device with no store and show
/// the user zero tasks. The guard is `crate::store::open::ensure_truth_source_in_place`, which names the
/// build that *can* fold it.
pub fn has_legacy_layout(base: &Path) -> bool {
    let projects = std::fs::read_dir(base.join(LEGACY_STORES_SUBDIR))
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| crate::config::resolve_store_file(&e.path()).is_file());
    projects || crate::config::resolve_store_file(&base.join(LEGACY_ROOT_SUBDIR)).is_file()
}

/// This device's truth source under `base`: the unified `store.sqlite`, or — only where `base` *is* a store
/// directory — the legacy name beside it ([`crate::config::resolve_store_file`]). The narrowing is the
/// point: on a device that still holds the legacy layout, the `oplog.sqlite` lying at the app-data root is
/// not the truth source but a fossil that nothing can open. Resolve it and it impersonates the truth
/// source, so every open dies on it before the migration that would retire the whole layout can even be
/// offered.
pub fn resolve_store_file(base: &Path) -> PathBuf {
    let unified = base.join(crate::config::STORE_FILE_NAME);
    if unified.exists() || has_legacy_layout(base) {
        return unified;
    }
    crate::config::resolve_store_file(base)
}

// ───────────────────────────────── Restore ─────────────────────────────────

/// Outcome of a whole-device restore: where the replaced store's previous truth source was set aside
/// (the `store.pre-restore-<stamp>.sqlite` safety backup, kept on success), how many attachment blobs
/// landed, and what the version chain did to the archive's store on the way in.
#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    /// Absolute path the replaced store's previous truth source was set aside to. `None` when there was
    /// no live predecessor and the store was created fresh.
    pub previous_saved_to: Option<String>,
    /// Attachment blobs written into the live blob stores. Excludes those already present:
    /// blobs are content-addressed, so a hash the destination already holds is left alone.
    pub blobs: u64,
    /// What the version chain ran over the staged snapshot: an archive taken by an older build
    /// is carried forward before it is swapped in, and `migration.migrated()` says whether it was. The
    /// caller tells the human — a restore that quietly moved the data to a new shape is exactly the thing
    /// not to keep to oneself.
    pub migration: chain::Run,
    /// The older pre-restore asides this restore's own aside superseded, and which it therefore deleted
    /// ([`sweep_superseded`]). Empty when there were none.
    pub superseded: Vec<String>,
}

/// Read the [`ArchiveManifest`] from a `.amenbo-backup` archive **without extracting the snapshot** — it is
/// written first (see the module docs), so the restore pre-flight gate reads it cheaply regardless of how
/// large the snapshot is. The layout version is read **before** the manifest is parsed
/// ([`ManifestEnvelope`]), because it decides whether the rest of the JSON is a shape this build has a type
/// for: a v1–v4 archive carries a `stores` list, which [`ArchiveManifest`] does not describe. Gating first
/// turns "this build cannot read that archive" into a sentence that names the fix, instead of a `serde`
/// error about a missing field.
pub fn read_manifest(archive: &Path) -> Result<ArchiveManifest> {
    // The mirror of the guard in `backup_from`: opening a directory succeeds and only reading it
    // fails, so without this the user gets the OS's `Is a directory` and has to work out which of
    // the two paths it means.
    if archive.is_dir() {
        return Err(Error::Invalid(
            Msg::new(format!(
                "that is a directory — restore takes one archive file: {}",
                archive.display()
            ))
            .coded(ErrorCode::InvalidRestoreSourceIsDir)
            .with("path", archive.display()),
        ));
    }
    let mut ar = tar::Archive::new(File::open(archive)?);
    for entry in ar.entries()? {
        let mut entry = entry?;
        let is_manifest =
            entry.path().ok().and_then(|p| p.to_str().map(str::to_string)).as_deref() == Some(MANIFEST_ENTRY);
        if is_manifest {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf)?;
            let envelope: ManifestEnvelope = serde_json::from_slice(&buf)?;
            ensure_layout_readable(envelope.archive_layout_version)?;
            return Ok(serde_json::from_slice(&buf)?);
        }
    }
    Err(Error::Invalid(
        Msg::new(format!("archive has no `{MANIFEST_ENTRY}` — not a valid .amenbo-backup archive"))
            .coded(ErrorCode::InvalidRestoreNotAnArchive)
            .with("path", archive.display()),
    ))
}

/// Refuse an archive whose container layout this build does not read — in **both** directions, before the
/// live tree (or the manifest's own shape) is touched at all. Older: written for the N-store layout, for
/// which this build has neither the type nor anywhere to put a second store; the rewind path is the Amenbo
/// that wrote it. Newer: it may hold entries this build does not know to place, and for a
/// disaster-recovery tool, dropping them silently is worse than refusing (no partial application).
fn ensure_layout_readable(layout: u32) -> Result<()> {
    if layout < MIN_ARCHIVE_LAYOUT_VERSION {
        return Err(Error::Invalid(
            Msg::new(format!(
                "this archive uses layout v{layout} — it was written before the consolidation, and this build reads v{MIN_ARCHIVE_LAYOUT_VERSION} and later. Restore it with the Amenbo that wrote it (nothing here was changed)"
            ))
            .coded(ErrorCode::InvalidRestoreLayoutTooOld)
            .with("layout", layout)
            .with("min", MIN_ARCHIVE_LAYOUT_VERSION),
        ));
    }
    if layout > ARCHIVE_LAYOUT_VERSION {
        return Err(Error::Invalid(
            Msg::new(format!(
                "this archive uses layout v{layout} — this build reads up to v{ARCHIVE_LAYOUT_VERSION}. update to the latest Amenbo — nothing was changed"
            ))
            .coded(ErrorCode::InvalidRestoreLayoutTooNew)
            .with("layout", layout)
            .with("max", ARCHIVE_LAYOUT_VERSION),
        ));
    }
    Ok(())
}

/// The live destination `store.sqlite` on this device — the OS-layout glue for restore, symmetric to
/// [`enumerate_store`] for backup. There is **one** destination (`<app-data>/store.sqlite`), always the
/// canonical [`crate::config::STORE_FILE_NAME`], so a restore normalises a legacy `oplog.sqlite` name.
pub fn restore_dest() -> PathBuf {
    crate::config::Paths::user_base().join(crate::config::STORE_FILE_NAME)
}

/// **One rewind point per kind, and it is the newest one**: delete every file in `dir` whose name is
/// `<prefix>…<suffix>`, except `keep` — the one just written.
///
/// Amenbo takes a copy of the whole store before it moves it or destroys part of it: the pre-migration
/// archive ([`crate::migrate`]), the pre-restore aside a restore sets the replaced store to, and the
/// pre-erase archive a `hard-erase` stands on ([`pre_erase_backup`]). Each is a whole copy of the store
/// (truth source *and* every attachment blob), so **a new rewind point supersedes the old one the moment it
/// exists**, and the thing that makes it is the thing that sweeps — no background job, no `doctor --fix`,
/// no cleanup the user has to know to run. Only the newest is worth keeping, because it is the only one you
/// can go back to: there is no downgrade, the way back from a version is its own pre-migration backup, and
/// the build that wrote the one before that is not on this machine any more.
///
/// Best-effort: a file that will not go simply stays. A leftover copy is a nuisance; failing a migration
/// or a restore over one is not. Returns the paths it removed, so the caller can tell the human.
pub(crate) fn sweep_superseded(dir: &Path, prefix: &str, suffix: &str, keep: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep || !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            removed.push(path.display().to_string());
        }
    }
    removed.sort();
    removed
}

/// The fast, advisory pre-flight gate: if the manifest declares a store at a generation newer than this
/// build supports, refuse the archive before touching the live tree (no partial application). The manifest
/// can be tampered with, so this is a UX-level fast fail; the authoritative generation gate is
/// [`stage_snapshot`], which re-checks the snapshot's real stamped generation. (The container layout is
/// gated earlier still, in [`read_manifest`] — it decides whether the manifest can be parsed at all.)
fn preflight_generation_gate(manifest: &ArchiveManifest) -> Result<()> {
    let max = crate::model::FORMAT_VERSION;
    let found = manifest.store.format_version;
    if found > max {
        // Named, like the open-time gate: the archive records the app version that produced it, and that
        // version reads its own store — so the refusal can say which Amenbo to run instead of "the latest",
        // which nobody can act on offline.
        let app = &manifest.producer_app_version;
        return Err(Error::Invalid(
            Msg::new(format!(
                "this archive was produced by a newer Amenbo (v{app}) — its store is at format v{found}, past the v{max} this build reads. use Amenbo {app} or newer — nothing was changed"
            ))
            .coded(ErrorCode::InvalidRestoreArchiveNewer)
            .with("app", app)
            .with("found", found)
            .with("max", max),
        ));
    }
    Ok(())
}

/// Where the extracted blobs are staged: `stage/blobs/<hash>` — a blob-store layout, so
/// [`crate::blob::BlobStore::list`] can enumerate it back on the way out.
fn staged_blobs_dir(stage: &Path) -> PathBuf {
    stage.join(crate::blob::BLOBS_SUBDIR)
}

/// Resolve a tar entry name to its staging destination, or `None` to skip it. The snapshot goes to
/// `stage/store.sqlite`, the blobs to `stage/blobs/<hash>`.
///
/// This is the whole trust boundary for archive-controlled paths: a blob entry is placed **only** when
/// its tail parses as a `<64-char BLAKE3 hex>`, so a crafted `../../…` name, a `blobs/tmp/` leftover, or a
/// directory this build doesn't know is dropped rather than materialised.
fn staging_dest(name: &str, stage: &Path) -> Option<PathBuf> {
    if name == SNAPSHOT_ENTRY {
        return Some(stage.join(SNAPSHOT_ENTRY));
    }
    let hash = name.strip_prefix(&blobs_prefix())?;
    crate::blob::is_hash(hash).then(|| staged_blobs_dir(stage).join(hash))
}

/// Extract the snapshot and every blob from the tar into `stage`, streaming file-by-file (bounded
/// memory). Returns the staged snapshot's path (the blobs sit in [`staged_blobs_dir`]). Fails if the
/// archive carries no snapshot. Unrecognised entries are skipped — see [`staging_dest`].
///
/// This is the restore's longest stretch (it writes the whole archive back out), so it ticks
/// [`Phase::Unpacking`] **per entry written** and honours cancellation there: a boundary only at
/// the end would leave the progress modal saying nothing and the Cancel button pressable but inert for
/// as long as the unpacking takes. Nothing has to be undone on a cancel — the caller's [`DirGuard`]
/// discards the staging dir, and the live tree is not touched until the swap.
///
/// `expected` is what the manifest says the tar holds (snapshot + blobs) — see [`Phase::Unpacking`] on
/// why that total is advisory.
fn extract_snapshot(
    archive: &Path,
    stage: &Path,
    expected: u64,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<PathBuf> {
    let mut ar = tar::Archive::new(File::open(archive)?);
    let mut written = 0u64;
    for entry in ar.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        if let Some(dest) = staging_dest(&name, stage) {
            let tick = Progress { phase: Phase::Unpacking, done: written, total: Some(expected) };
            if progress(&tick).is_break() {
                return Err(restore_cancelled());
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            written += 1;
        }
    }

    let staged = stage.join(SNAPSHOT_ENTRY);
    if !staged.is_file() {
        return Err(Error::Invalid(
            Msg::new(format!("archive is missing the store snapshot (`{SNAPSHOT_ENTRY}`)"))
                .coded(ErrorCode::InvalidRestoreMissingSnapshot)
                .with("path", archive.display()),
        ));
    }
    Ok(staged)
}

/// What the staging step does to the archive's snapshot before it can be swapped in. The two callers want
/// opposite things from the same envelope, and the difference is not a knob but the whole intent.
#[derive(Clone, Copy)]
enum Staging {
    /// **Bring it forward**: run the version chain from the version the snapshot carries up to this
    /// build's, and verify it against this build's read model. What a user's `amenbo restore` means — the
    /// store has to be one the build doing the restoring can actually open.
    Migrate(&'static [Step]),
    /// **Put it back as it was taken**: no chain, and no current-schema verify. What a failed migration's
    /// rollback means ([`crate::migrate`]). Its archive holds the store as it was *before* the first step,
    /// and that older shape is the whole point of it: running the chain here would re-apply the very steps
    /// that just failed, and judging it by this build's read model would reject it for lacking what the
    /// migration was going to add. So the snapshot is proved structurally healthy (`integrity_check`) and
    /// swapped in unchanged — the store comes back at the version it started at, and the chain runs again
    /// when the migration is retried.
    AsTaken,
}

/// Bring one staged snapshot to the state it must be in before the swap — carried forward, or left exactly
/// as taken ([`Staging`]) — then verify it, all **in place** in the staging directory (the live tree is not
/// touched). Returns what the chain ran.
///
/// The authoritative generation gate gets the first word, on the real stamped generation (not the advisory
/// manifest) read off a bare connection — opening through the engine would touch the snapshot's schema
/// *before* the gate could refuse it. Then the archive's store is walked up the **version chain**: the same
/// numbered steps a live store is migrated with, applied from whatever version the snapshot carries to this
/// build's. This is what makes an archive a rewind point that outlives the build that wrote it — an old
/// snapshot the open alone cannot repair (it emits `CREATE TABLE IF NOT EXISTS` and nothing more, so a
/// column an older generation lacked stays missing) is either translated here or refused, never swapped in
/// half-repaired. The version stamp rides with the steps: the chain stamps inside each step's transaction,
/// so no snapshot is ever marked as having reached a version nothing carried it to. **No pre-migration
/// backup wraps this run** — unlike a live migration ([`crate::migrate`]), which must capture the store
/// before it moves it; here the archive *is* the "before", since what is migrated is only a copy of it in a
/// staging dir that a failure discards. `base_dir` is that staging directory — the snapshot's home, with
/// the archive's blobs beside it, mirroring the live layout — so a step whose half is on disk (an
/// attachment blob, the store directory's shape) finds the same tree here that it would find there; the
/// engine is dropped (file closed) before the bounded verify reopens the snapshot and before the swap moves
/// it. The chain comes in as an argument rather than from [`chain::STEPS`] so a test can drive one of its
/// own ([`restore_into`] passes the real one), and `progress` goes to the chain, which ticks
/// [`Phase::Migrating`] per step but is not a cancellation point (see [`chain::run`]) — the staging
/// directory is discarded on the next boundary anyway, so nothing is lost by waiting for it.
fn stage_snapshot(
    path: &Path,
    base_dir: &Path,
    staging: Staging,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<chain::Run> {
    // A manifest that lied about a too-new store past the pre-flight is still caught here by the actual
    // data — and, for the migrating staging, before this build's DDL touches that data's schema.
    crate::store::open::ensure_format_supported(&crate::store_engine::probe_format_stamp(path))?;

    let Staging::Migrate(steps) = staging else {
        // As taken: no chain, and no basis but the file's own structural health — see `Staging::AsTaken`.
        let conn = Connection::open(path).map_err(sqlite_at(path))?;
        integrity_check(&conn, path)?;
        let at = crate::store_engine::probe_format_version(path);
        return Ok(chain::Run { from: at, to: at, applied: Vec::new() });
    };

    let run = {
        let engine = crate::store_engine::StoreEngine::open(path)?;
        chain::run(&engine, base_dir, steps, progress)?
    };
    // The chain has just brought the snapshot to this build's shape, so this build's read model *is* the
    // right basis here (unlike a backup's copy of a store this build never opened — see the two verifies).
    verify_snapshot_current_schema(path)?;
    Ok(run)
}

/// A fresh staging directory (under the OS temp dir) for the extracted snapshots. The swap moves files
/// out of here via [`move_file`], which falls back to copy across filesystems, so the temp location is
/// safe even when it differs from the app-data volume.
fn restore_staging_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(".amenbo-backup-restore-{}", crate::tmpdir::suffix()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Best-effort removal of an engine's `-wal`/`-shm` sidecars (mirrors the single-store restore) so a
/// leftover sidecar can never be mis-associated with a swapped-in file.
pub(crate) fn remove_sidecars(db_path: &Path) {
    for ext in ["-wal", "-shm"] {
        let mut side = db_path.as_os_str().to_os_string();
        side.push(ext);
        let _ = std::fs::remove_file(PathBuf::from(side));
    }
}

/// Fold the live store's committed WAL frames into its main file before that file is renamed aside.
///
/// The aside **is** the rewind point, and it is a single file: the swap deletes the `-wal`/`-shm` left
/// behind under the old name, because a stale sidecar beside the swapped-in store would be catastrophic.
/// Without this checkpoint, every transaction committed since the last one — which lives in the WAL, not
/// the main file — would be deleted along with it, and "restore the aside" would silently rewind further
/// than the swap did. (The snapshot never had this problem: `VACUUM INTO` reads through the WAL.)
///
/// A `TRUNCATE` checkpoint writes nothing the store did not already contain, so this is not a mutation the
/// live store can lose by. It runs immediately before the swap, and the window where a new write can slip
/// into a fresh WAL is closed by excluding other writers — `amenbo restore` holds the store's swap lock
/// exclusive around this ([`crate::swap_lock`]).
/// Opened **without** `SQLITE_OPEN_CREATE`: a store that vanished between staging and the swap must fail
/// the swap, not be conjured back as an empty file that the rename then happily moves aside.
pub(crate) fn checkpoint(db_path: &Path) -> Result<()> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(db_path, flags).map_err(sqlite_at(db_path))?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(sqlite_at(db_path))?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").map_err(sqlite_at(db_path))?;
    Ok(())
}

/// Move `src` onto `dest`, preferring an atomic rename and falling back to copy+remove when the two are
/// on different filesystems (staging under the temp dir vs. the app-data volume).
pub(crate) fn move_file(src: &Path, dest: &Path) -> Result<()> {
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dest)?;
    let _ = std::fs::remove_file(src);
    Ok(())
}

/// Replace the live truth source `dest` with the contents of `source`, preserving the current `dest` as
/// `aside`, with **no window in which `dest` is absent** (the swap half of what [`crate::swap_lock`]
/// guards). The caller must already hold `dest`'s swap lock exclusive. An absence window would let an
/// opener arriving in the gap fabricate an **empty** store (`Connection::open` carries
/// `SQLITE_OPEN_CREATE`) whose writes the finishing copy would then silently eat, so:
///
/// 1. Clear the open handles the replace would trip over ([`crate::swap_lock`], `AMB-D-704`): this
///    process lets go of the connections it keeps across actions, and a store another program still holds
///    open is refused with `store_busy`. Asked **before anything here moves**, so a refusal leaves `dest`
///    as it stands with no `aside` and no `staging` beside it — and the swap lock the caller holds keeps a
///    new open from arriving between that answer and the rename.
/// 2. [`checkpoint`] folds `dest`'s committed WAL into its file, so the plain-copy `aside` is a complete
///    single-file rewind point (a no-op when the live engine was already closed and checkpointed, as
///    `Store::restore` leaves it).
/// 3. **Copy** `dest` to `aside` — `dest` stays in place.
/// 4. Materialise `source` beside `dest` as `staging` (same directory ⇒ the final rename is atomic even
///    when `source` lives on another filesystem, e.g. a temp staging dir), then **rename `staging` over
///    `dest`** in one step. `dest` goes straight from the old file to the new one; it is never absent.
///
/// `source` is copied, never consumed — a `restore` must not delete the snapshot it restores from.
/// On success `aside` remains as the safety backup; the caller records it for rollback and decides its
/// fate. A failure leaves `aside` (a faithful copy of the pre-swap `dest`) for the caller to restore from.
pub(crate) fn replace_truth_source(
    dest: &Path,
    source: &Path,
    aside: &Path,
    staging: &Path,
) -> Result<()> {
    swap_lock::release_local_connections();
    swap_lock::ensure_replaceable(dest)?;
    checkpoint(dest)?;
    std::fs::copy(dest, aside)?;
    std::fs::copy(source, staging)?;
    std::fs::rename(staging, dest)?;
    // The old `-wal`/`-shm` are keyed by filename, so after the rename they would be mis-associated with
    // the just-swapped-in file; clear them (the checkpoint above already truncated the live WAL, and a
    // `VACUUM INTO` snapshot carries none, so this is defensive).
    remove_sidecars(dest);
    Ok(())
}

/// Place the staged attachment bytes into the live blob store, returning how many were written.
/// **Additive and idempotent**: a hash the destination already holds is left alone, because the
/// bytes are identical by construction (content-addressing).
/// Nothing is ever removed, so this is safe to run before the destructive DB swap and needs no rollback —
/// a blob the aborted restore left behind is unreferenced garbage that `gc_blobs` reclaims.
///
/// Ticks [`Phase::Blobs`] per blob and honours cancellation there (the caller then puts the aside back).
/// A no-op for an archive that carried no blobs.
fn place_blobs(
    stage: &Path,
    dest_db: &Path,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<u64> {
    let Some(live_dir) = blobs_dir_of(dest_db) else {
        return Ok(0);
    };
    let staged_dir = staged_blobs_dir(stage);
    if !staged_dir.is_dir() {
        return Ok(0); // the store never attached anything
    }
    let staged = crate::blob::BlobStore::at(staged_dir.clone());
    let blobs = staged.list()?;
    let total = blobs.len() as u64;
    let mut written = 0u64;
    for (j, b) in blobs.iter().enumerate() {
        if progress(&Progress { phase: Phase::Blobs, done: j as u64, total: Some(total) }).is_break() {
            return Err(restore_cancelled());
        }
        let Some(src) = staged.path(&b.hash) else {
            continue;
        };
        let dest = live_dir.join(&b.hash);
        if dest.exists() {
            continue;
        }
        std::fs::create_dir_all(&live_dir)?;
        move_file(&src, &dest)?;
        written += 1;
    }
    Ok(written)
}

/// Restore a whole-device `.amenbo-backup` archive: destructively replace the live store with the one the
/// archive carries (a point-in-time rewind), forward-migrated to this build's generation, inside a
/// stage-and-swap envelope. `dest` is the live `store.sqlite` to replace ([`restore_dest`] is the
/// OS-layout default; tests pass their own). `stamp` names the `store.pre-restore-<stamp>.sqlite` aside.
/// The store's attachment bytes are placed into `blobs/` beside its restored truth source, additively —
/// see [`place_blobs`]. On an unreadable layout, a too-new generation (pre-flight or authoritative gate),
/// or a staging verification failure, the live tree is left untouched; on an I/O failure mid-swap the aside
/// goes back. `progress` observes the phases (and each blob) and may cancel at those boundaries.
///
/// **An unreleased build is refused here** ([`ensure_may_restore_over`], `AMB-D-378`): this is the other
/// path that runs the version chain, and the one a startup gate never sees — a live store already at this
/// build's format leaves nothing pending, and the restore then carries the archive forward regardless.
pub fn restore_into(
    archive: &Path,
    stamp: &str,
    dest: &Path,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<RestoreReport> {
    ensure_may_restore_over(dest)?;
    restore_staging(archive, stamp, dest, Staging::Migrate(chain::STEPS), progress)
}

/// The release-stamp gate, asked only when the restore targets **this device's live store**
/// ([`restore_dest`] — what the CLI and the GUI pass, and the only store a user's `amenbo restore`
/// replaces). A restore into a directory a caller named is not the irreversible act the gate is about: the
/// chain runs over a copy in a staging directory either way, and what it lands on is that caller's file.
///
/// The gate itself is [`crate::build_stamp::ensure_may_migrate`], unchanged and unduplicated — the rule for
/// "may this build carry data forward" has one home, and both migration paths ask it.
fn ensure_may_restore_over(dest: &Path) -> Result<()> {
    if dest != restore_dest() {
        return Ok(());
    }
    crate::build_stamp::ensure_may_migrate()
}

/// Restore an archive **as it was taken** — the rollback half of the same envelope, for a migration that
/// failed and must put the store back the way it found it ([`Staging::AsTaken`] says why the chain must not
/// run here). Not a user-facing gesture: `amenbo restore` is [`restore_into`], which brings the archive
/// forward.
pub(crate) fn rewind_into(
    archive: &Path,
    stamp: &str,
    dest: &Path,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<RestoreReport> {
    restore_staging(archive, stamp, dest, Staging::AsTaken, progress)
}

/// The restore envelope both entry points share; [`Staging`] is what they differ by. It also takes the
/// chain by argument rather than reading [`chain::STEPS`] directly, so a test can drive a chain of its own —
/// the shipped one is empty until a step lands, and an empty chain cannot show that an old archive is
/// carried forward.
fn restore_staging(
    archive: &Path,
    stamp: &str,
    dest: &Path,
    staging_policy: Staging,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<RestoreReport> {
    // 1. Read the manifest (no snapshot extraction — and the layout gate refuses an archive this build
    //    cannot read), then fast-fail on a too-new store generation.
    let manifest = read_manifest(archive)?;
    preflight_generation_gate(&manifest)?;

    // 2. Extract the snapshot to staging and, in staging, re-gate + bring it to the state the swap needs
    //    (`Staging`) + verify it. The live tree is untouched, so a failure here aborts with nothing to
    //    roll back.
    let stage = restore_staging_dir()?;
    let _guard = DirGuard(stage.clone());
    // The snapshot plus the blobs the manifest counted — the unpacking's total, known before extraction
    // because the manifest is the tar's first entry (see `Phase::Unpacking`).
    let entries = manifest.store.blob_count + 1;
    let staged = extract_snapshot(archive, &stage, entries, progress)?;
    if progress(&Progress { phase: Phase::Verifying, done: 0, total: Some(1) }).is_break() {
        return Err(restore_cancelled());
    }
    let migration = stage_snapshot(&staged, &stage, staging_policy, progress)?;

    // 3. The snapshot is green — swap it into the live tree. Hold the store's swap lock exclusive across
    //    the swap so no open reads it mid-replace (they get a clean `store_busy` instead). A store that
    //    does not exist yet has nothing to be absent, so it needs no guard.
    let _swap_guard = dest.exists().then(|| swap_lock::hold_for_swap(dest)).transpose()?;

    if progress(&Progress { phase: Phase::Copying, done: 0, total: Some(1) }).is_break() {
        return Err(restore_cancelled());
    }
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    // Blobs first: additive, so the attachment bytes are already in place the moment the rows that
    // reference them land — and an abort here has nothing destructive to undo.
    let blobs = place_blobs(&stage, dest, progress)?;

    if !dest.exists() {
        move_file(&staged, dest)?;
        // No predecessor, so no aside — and nothing this restore's rewind point could supersede.
        return Ok(RestoreReport { previous_saved_to: None, blobs, migration, superseded: Vec::new() });
    }
    // Replace with no absence window: checkpoint → copy the old file to the aside → atomically rename the
    // migrated snapshot into place. A failure leaves the aside, which is the live store's faithful copy,
    // so put it back.
    let aside = dest.with_file_name(format!("{PRE_RESTORE_PREFIX}{stamp}.sqlite"));
    let staging = dest.with_file_name(format!("store.incoming-{stamp}.sqlite"));
    match replace_truth_source(dest, &staged, &aside, &staging) {
        Ok(()) => {
            // The aside is now this store's rewind point, so the ones from earlier restores are not: they
            // are copies of stores nothing can go back to.
            let superseded = sweep_superseded(
                aside.parent().unwrap_or(Path::new(".")),
                PRE_RESTORE_PREFIX,
                ".sqlite",
                &aside,
            );
            Ok(RestoreReport {
                previous_saved_to: Some(aside.display().to_string()),
                blobs,
                migration,
                superseded,
            })
        }
        Err(e) => {
            let _ = std::fs::remove_file(&staging);
            if aside.is_file() {
                let _ = std::fs::rename(&aside, dest);
            }
            remove_sidecars(dest);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;

    fn scratch(tag: &str) -> PathBuf {
        amenbo_scratch::scratch(&format!("backup-{tag}"))
    }

    /// Open a real store in `dir` and seed it with `names` tasks — exercises the live engine the way a
    /// backup will (write path stamps `format_version`, so snapshots carry a real generation).
    fn seed_store(dir: &Path, names: &[&str]) {
        let mut s = crate::store::Store::open_at(Paths::at(dir.to_path_buf())).unwrap();
        for name in names {
            s.add_task(crate::ops::task::NewTask {
                title: (*name).into(),
                project_id: None,
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
                at_binding_id: None,
            })
            .unwrap();
        }
    }

    /// Ingest a blob into the store's blob dir, the way an `attach` would.
    fn ingest_blob(dir: &Path, bytes: &[u8]) -> String {
        let s = crate::store::Store::open_at(Paths::at(dir.to_path_buf())).unwrap();
        s.blobs().ingest_bytes(bytes).unwrap().hash
    }

    fn source(dir: &Path) -> StoreSource {
        StoreSource {
            db_path: dir.join(crate::config::STORE_FILE_NAME),
            bindings: Vec::new(),
        }
    }

    /// Extract an entry's bytes from the archive (read side of the round trip).
    fn read_entry(archive: &Path, name: &str) -> Option<Vec<u8>> {
        let mut ar = tar::Archive::new(File::open(archive).unwrap());
        for entry in ar.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap().to_str() == Some(name) {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
                return Some(buf);
            }
        }
        None
    }

    #[test]
    fn round_trips_the_manifest_and_the_snapshot() {
        let base = scratch("rt");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob", "carol"]);

        let dest = base.join(format!("backup.{ARCHIVE_EXT}"));
        let report = backup_from(&source(&a), &dest, &mut crate::progress::ignore).unwrap();

        assert!(report.bytes > 0);
        assert!(dest.is_file());

        // Manifest is present, first-written, and records the store with a real generation.
        let manifest: ArchiveManifest =
            serde_json::from_slice(&read_entry(&dest, MANIFEST_ENTRY).expect("manifest")).unwrap();
        assert_eq!(manifest.archive_layout_version, ARCHIVE_LAYOUT_VERSION);
        assert_eq!(manifest.producer_format_version, crate::model::FORMAT_VERSION);
        assert_eq!(manifest.store.schema_version, crate::model::SCHEMA_VERSION);
        assert_eq!(manifest.store.format_version, crate::model::FORMAT_VERSION);

        // Extract the snapshot to a fresh dir and prove it hydrates to the same live task count.
        let out = base.join("restored");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(
            out.join(crate::config::STORE_FILE_NAME),
            read_entry(&dest, SNAPSHOT_ENTRY).expect("snapshot"),
        )
        .unwrap();
        let restored = crate::store::Store::open_at(Paths::at(out)).unwrap();
        let db = crate::store_engine::hydrate_database(restored.read_model().conn()).unwrap();
        assert_eq!(db.tasks.len(), 3);
    }

    #[test]
    fn refuses_to_overwrite_existing_archive() {
        let base = scratch("nooverwrite");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["x"]);
        let dest = base.join(format!("backup.{ARCHIVE_EXT}"));
        std::fs::write(&dest, b"pre-existing").unwrap();
        let err = backup_from(&source(&a), &dest, &mut crate::progress::ignore).unwrap_err();
        assert!(err.to_string().contains("overwrite"));
    }

    /// A source that is not a database fails with the path in the message. SQLite's own
    /// `file is not a database` names nothing — without the path, neither the user nor the AI can tell
    /// which file to move aside.
    #[test]
    fn an_unreadable_source_names_the_file_it_choked_on() {
        let base = scratch("notadb");
        let bad = base.join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        let db = bad.join(crate::config::STORE_FILE_NAME);
        std::fs::write(&db, b"SQLite format 3\0 but encrypted, actually not a database").unwrap();

        let err = backup_from(
            &source(&bad),
            &base.join(format!("backup.{ARCHIVE_EXT}")),
            &mut crate::progress::ignore,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains(&db.display().to_string()), "it must name the path: {err}");
    }

    /// A failure surfacing through the read model (`StoreEngineError`) names the file too, exactly as a raw
    /// SQLite failure does — if the read paths (`query` / `store::read`) stay silent, all that is left is
    /// `no such table`, and nobody can say which file it happened on.
    #[test]
    fn a_read_names_the_file_it_was_reading() {
        let base = scratch("readname");
        let dir = base.join("a");
        std::fs::create_dir_all(&dir).unwrap();
        seed_store(&dir, &["x"]);
        let db = dir.join(crate::config::STORE_FILE_NAME);

        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("DROP TABLE project").unwrap();

        let err = crate::query::project_list(&conn, false).unwrap_err().to_string();
        assert!(err.contains(&db.display().to_string()), "it must name the path: {err}");
    }

    /// A directory is named as a directory on both faces. It is the plausible mistake here — `export`,
    /// the neighbouring command, wants one — and neither generic answer helps: `backup` would call it
    /// an existing archive, and `restore` would hand over the OS's `Is a directory`.
    #[test]
    fn a_directory_is_named_as_one_on_both_faces() {
        let base = scratch("dir-dest");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["x"]);
        let dir = base.join("somewhere");
        std::fs::create_dir_all(&dir).unwrap();

        let mut cb = |_p: &Progress| ControlFlow::Continue(());
        let err = backup_from(&source(&a), &dir, &mut cb).unwrap_err().to_string();
        assert!(err.contains("is a directory"), "backup must say it is a directory: {err}");
        assert!(!err.contains("existing archive"), "and must not call it an archive: {err}");

        let err = read_manifest(&dir).unwrap_err().to_string();
        assert!(err.contains("is a directory"), "restore must say it is a directory: {err}");
    }

    #[test]
    fn cancellation_removes_partial_archive() {
        let base = scratch("cancel");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["x"]);
        let dest = base.join(format!("backup.{ARCHIVE_EXT}"));
        // Cancel on the very first tick.
        let mut cb = |_p: &Progress| ControlFlow::Break(());
        let err = backup_from(&source(&a), &dest, &mut cb).unwrap_err();
        assert!(err.to_string().contains("backup"));
        assert!(!dest.exists(), "partial archive must be cleaned up on cancel");
    }

    /// A cancelled restore names the restore — not the backup — and leaves the live store as it found it
    /// (no swap, no aside).
    #[test]
    fn cancellation_of_a_restore_names_the_restore_and_rolls_back() {
        let base = scratch("cancel-restore");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob"]);
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();

        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        seed_store(&live, &["untouched"]);
        let live_db = live.join(crate::config::STORE_FILE_NAME);

        let mut cb = |_p: &Progress| ControlFlow::Break(());
        let err = restore_into(&archive, "20260714T000000Z", &live_db, &mut cb).unwrap_err();
        assert!(
            err.to_string().contains("restore"),
            "a cancelled restore must not report a cancelled backup: {err}"
        );
        assert_eq!(live_tasks(&live_db), 1, "the live store is untouched");
        assert!(
            !live.join("store.pre-restore-20260714T000000Z.sqlite").exists(),
            "a cancelled restore asides nothing"
        );
    }

    /// The restore's longest stretch reports itself: unpacking ticks once per entry it writes —
    /// the snapshot plus every blob — against the total the manifest already knew, so the modal has
    /// something to say from the first entry instead of standing on "preparing…" until it is over.
    #[test]
    fn unpacking_ticks_per_entry_against_the_manifest_total() {
        let base = scratch("unpack-ticks");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice"]);
        ingest_blob(&a, b"one");
        ingest_blob(&a, b"two");
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();

        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        let mut ticks: Vec<(u64, Option<u64>)> = Vec::new();
        let mut cb = |p: &Progress| {
            if p.phase == Phase::Unpacking {
                ticks.push((p.done, p.total));
            }
            ControlFlow::Continue(())
        };
        restore_into(
            &archive,
            "20260714T000000Z",
            &live.join(crate::config::STORE_FILE_NAME),
            &mut cb,
        )
        .unwrap();

        // The snapshot plus the two blobs, counted from 0 — and the total (3) was known before the first
        // byte came out of the tar.
        assert_eq!(ticks, vec![(0, Some(3)), (1, Some(3)), (2, Some(3))]);
    }

    /// Cancelling **during** the unpacking is honoured there, not at the phase's far end: the button is
    /// pressable for as long as the unpacking runs, so it has to do something. Nothing was written
    /// to the live tree yet, so there is nothing to undo — the staging dir is simply dropped.
    #[test]
    fn cancellation_mid_unpacking_stops_there_and_leaves_the_live_store_alone() {
        let base = scratch("cancel-unpack");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob"]);
        ingest_blob(&a, b"one");
        ingest_blob(&a, b"two");
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();

        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        seed_store(&live, &["untouched"]);
        let live_db = live.join(crate::config::STORE_FILE_NAME);

        // Break on the second unpacking tick — the snapshot is already staged, a blob is not.
        let mut seen = 0;
        let mut phases: Vec<Phase> = Vec::new();
        let mut cb = |p: &Progress| {
            phases.push(p.phase);
            if p.phase == Phase::Unpacking {
                seen += 1;
                if seen == 2 {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        };
        let err = restore_into(&archive, "20260714T000000Z", &live_db, &mut cb).unwrap_err();

        assert!(
            err.to_string().contains("restore"),
            "a cancelled unpacking is a cancelled restore: {err}"
        );
        assert!(
            !phases.contains(&Phase::Verifying) && !phases.contains(&Phase::Copying),
            "the cancel is honoured inside the unpacking, not at its far end: {phases:?}"
        );
        assert_eq!(live_tasks(&live_db), 1, "the live store is untouched");
        assert!(
            !live.join("store.pre-restore-20260714T000000Z.sqlite").exists(),
            "a cancelled restore asides nothing"
        );
        assert!(
            !live.join(crate::blob::BLOBS_SUBDIR).exists(),
            "a cancelled restore places no blobs"
        );
    }

    /// A store of an older generation — the way one that predates a table this build added actually looks
    /// on disk. Seeded through the live engine, then the table is dropped from the file (the engine only
    /// ever creates it).
    fn drop_table(dir: &Path, table: &str) {
        let conn = Connection::open(dir.join(crate::config::STORE_FILE_NAME)).unwrap();
        conn.execute(&format!("DROP TABLE \"{table}\""), []).unwrap();
    }

    /// Stamp a seeded store back to an older version — so it is a store an older build could have left,
    /// which is the only kind the chain has anything to do.
    fn stamp_version(dir: &Path, version: i64) {
        let conn = Connection::open(dir.join(crate::config::STORE_FILE_NAME)).unwrap();
        conn.execute(
            "INSERT INTO store_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![crate::store_engine::META_FORMAT_VERSION, version.to_string()],
        )
        .unwrap();
    }

    /// The pre-migration backup's whole purpose is to capture a store **this build has never opened**, so
    /// it cannot demand the tables this build would have created — an additive table would otherwise make
    /// every such store unbackupable, and therefore unmigratable, since the migration backs up first.
    #[test]
    fn backs_up_a_store_of_an_older_generation() {
        let base = scratch("oldgen");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob"]);
        drop_table(&a, "decision_edge");

        let dest = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &dest, &mut crate::progress::ignore)
            .expect("an older store is still a store, and a faithful copy of it is a valid rewind point");
        assert!(dest.is_file());
    }

    /// The other half of the basis swap: a snapshot that lost a table the source **does** hold is still a
    /// bad copy, and is still refused.
    #[test]
    fn refuses_a_snapshot_that_lost_a_table_the_source_holds() {
        let base = scratch("lost");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice"]);
        let live = a.join(crate::config::STORE_FILE_NAME);

        let snap = base.join("snap.sqlite");
        snapshot_into(&live, &snap).unwrap();
        verify_snapshot_mirrors_source(&snap, &live).expect("a faithful copy passes");

        Connection::open(&snap).unwrap().execute("DROP TABLE \"task_comment\"", []).unwrap();
        let err = verify_snapshot_mirrors_source(&snap, &live).unwrap_err();
        assert!(err.to_string().contains("task_comment"), "{err}");
    }

    /// A table of an older generation answers a `COUNT` just fine, so table-and-count alone would let a
    /// snapshot the version chain forgot to carry through the destructive swap, and the store would only
    /// fail at the first read. The staged-snapshot verify — the one whose basis is *this build's* read
    /// model — has to see the columns.
    #[test]
    fn refuses_a_staged_snapshot_whose_table_lost_a_column() {
        let base = scratch("lost-column");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice"]);

        let snap = base.join("snap.sqlite");
        snapshot_into(&a.join(crate::config::STORE_FILE_NAME), &snap).unwrap();
        verify_snapshot_current_schema(&snap).expect("a snapshot of this build's store passes");

        Connection::open(&snap).unwrap().execute("ALTER TABLE \"task\" DROP COLUMN \"title\"", []).unwrap();
        let err = verify_snapshot_current_schema(&snap).unwrap_err().to_string();
        assert!(err.contains("title") && err.contains("task"), "the failure must name the table and the column, got: {err}");
    }

    // ─────────────────────────────── restore ───────────────────────────────

    /// Task count of a store, opened from its containing dir.
    fn live_tasks(db_path: &Path) -> usize {
        let dir = db_path.parent().unwrap().to_path_buf();
        let s = crate::store::Store::open_at(Paths::at(dir)).unwrap();
        let db = crate::store_engine::hydrate_database(s.read_model().conn()).unwrap();
        db.tasks.len()
    }

    /// Raw task-row count of any db file — an aside is not named `store.sqlite`, so it cannot be opened
    /// as a store.
    fn task_rows(db_path: &Path) -> i64 {
        Connection::open(db_path)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM task", [], |r| r.get(0))
            .unwrap()
    }

    /// A minimal live task row, written straight through the engine (the caller holds the connection open
    /// so the commit stays in the WAL).
    fn put_task(engine: &crate::store_engine::StoreEngine, id: i64) {
        let now = crate::time::Timestamp::now().to_rfc3339_z();
        let text = |s: &str| rusqlite::types::Value::Text(s.to_string());
        engine
            .put_record(
                "task",
                id,
                &[
                    ("title", text("佐藤さんへ返信")),
                    ("status", text("todo")),
                    ("created_at", text(&now)),
                    ("updated_at", text(&now)),
                ],
            )
            .unwrap();
    }

    /// Build a `.amenbo-backup` archive by hand from raw manifest bytes and raw entries — for the reject /
    /// corrupt paths, which need a manifest this build would never write (a tampered generation, a layout
    /// it has no type for).
    fn build_archive(dest: &Path, manifest_json: &[u8], entries: &[(&str, &[u8])]) {
        let file = File::create(dest).unwrap();
        let mut b = tar::Builder::new(std::io::BufWriter::new(file));
        append_bytes(&mut b, MANIFEST_ENTRY, manifest_json).unwrap();
        for (name, bytes) in entries {
            append_bytes(&mut b, name, bytes).unwrap();
        }
        let mut inner = b.into_inner().unwrap();
        std::io::Write::flush(&mut inner).unwrap();
    }

    /// A manifest this build *would* write, at the given layout and store generation.
    fn manifest_json(layout: u32, format_version: i64) -> Vec<u8> {
        let m = ArchiveManifest {
            archive_layout_version: layout,
            created_at: "2026-07-13T00:00:00Z".into(),
            producer_app_version: "test".into(),
            producer_format_version: format_version,
            store: StoreEntry {
                bindings: Vec::new(),
                schema_version: crate::model::SCHEMA_VERSION.to_string(),
                format_version,
                blob_count: 0,
                blob_bytes: 0,
            },
        };
        serde_json::to_vec_pretty(&m).unwrap()
    }

    /// A whole-device restore: it recreates the store when the live tree has none, and replaces a present
    /// one after asiding its previous truth source — restored forward-migrated and green.
    #[test]
    fn restores_the_store_creating_then_replacing_with_aside() {
        let base = scratch("restore-rt");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob", "carol"]); // the archive: 3 live tasks

        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();

        // ① Empty live tree: the store is created.
        let live = base.join("live");
        let live_db = live.join(crate::config::STORE_FILE_NAME);
        let report =
            restore_into(&archive, "20260708T000000Z", &live_db, &mut crate::progress::ignore).unwrap();
        assert!(report.previous_saved_to.is_none(), "the absent store had no predecessor to aside");
        assert_eq!(live_tasks(&live_db), 3);

        // ② The store now present with *different* content: replaced, and its old engine kept aside.
        std::fs::remove_file(&live_db).unwrap();
        seed_store(&live, &["stale-one", "stale-two"]); // 2 live — must be overwritten + asided
        let report =
            restore_into(&archive, "20260709T000000Z", &live_db, &mut crate::progress::ignore).unwrap();
        assert!(report.previous_saved_to.is_some(), "the replaced store was asided");
        assert_eq!(live_tasks(&live_db), 3, "the archive's content won");
        let first_aside = live.join("store.pre-restore-20260709T000000Z.sqlite");
        assert!(first_aside.is_file(), "the replaced store's previous engine is kept aside");

        // ③ Restore again, and the new aside supersedes the old one — a store keeps **one** rewind point,
        //    the newest, because that is the only one anything can go back to.
        let report =
            restore_into(&archive, "20260710T000000Z", &live_db, &mut crate::progress::ignore).unwrap();
        assert!(live.join("store.pre-restore-20260710T000000Z.sqlite").is_file(), "the new aside is kept");
        assert!(!first_aside.exists(), "and the one it superseded is gone");
        assert_eq!(report.superseded, vec![first_aside.display().to_string()], "the report says so");
    }

    /// `hard-erase` stands on an archive of the whole store, and taking a new one sweeps what earlier
    /// erases left — each of those still carrying the very content its erase destroyed. An archive the
    /// *user* placed is not Amenbo's to sweep, so it stays.
    #[test]
    fn pre_erase_backup_keeps_only_the_newest_rewind_point() {
        let base = scratch("pre-erase-sweep");
        let dir = base.join("store");
        std::fs::create_dir_all(&dir).unwrap();
        seed_store(&dir, &["alice", "bob"]);

        let first =
            pre_erase_backup(&source(&dir), &dir, "20260713T000000Z", &mut crate::progress::ignore).unwrap();
        assert!(Path::new(&first.backup.path).is_file(), "the erase's rewind point is on disk");
        assert!(first.superseded.is_empty(), "the first erase had nothing to supersede");

        let user_backup = dir.join(format!("mine.{ARCHIVE_EXT}"));
        backup_from(&source(&dir), &user_backup, &mut crate::progress::ignore).unwrap();

        let second =
            pre_erase_backup(&source(&dir), &dir, "20260714T000000Z", &mut crate::progress::ignore).unwrap();
        assert!(Path::new(&second.backup.path).is_file(), "the new rewind point is kept");
        assert!(!Path::new(&first.backup.path).exists(), "and the one it superseded is gone");
        assert_eq!(second.superseded, vec![first.backup.path.clone()], "the report names what went");
        assert!(user_backup.is_file(), "an archive the user placed is not Amenbo's to sweep");
    }

    /// Strip a column from a store's `task` table — a store as an **older build** left it, before a step
    /// added the column. Unlike a missing table (which the open's `CREATE TABLE IF NOT EXISTS` puts back),
    /// nothing but a migration step can restore a column.
    fn drop_column(dir: &Path, column: &str) {
        Connection::open(dir.join(crate::config::STORE_FILE_NAME))
            .unwrap()
            .execute(&format!("ALTER TABLE task DROP COLUMN \"{column}\""), [])
            .unwrap();
    }

    /// Whether a restored store holds `column` on its `task` table.
    fn has_task_column(db_path: &Path, column: &str) -> bool {
        let conn = Connection::open(db_path).unwrap();
        let readable = conn.prepare(&format!("SELECT \"{column}\" FROM task LIMIT 1")).is_ok();
        readable
    }

    /// An archive an **older build** took — its store missing a column this build reads — is carried up the
    /// version chain *in staging* and swapped in whole. The open alone cannot repair it (it emits
    /// `CREATE TABLE IF NOT EXISTS` and nothing else, so the column stays missing), so without the chain
    /// what lands is a live store that breaks on the first read of that column.
    #[test]
    fn restores_an_older_archive_by_carrying_it_up_the_version_chain() {
        const READD_PRIORITY: &[Step] = &[Step {
            to: 3,
            name: "re-add task.priority",
            apply: chain::Apply::Sql("ALTER TABLE task ADD COLUMN priority TEXT;"),
        }];

        let base = scratch("restore-oldgen");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob"]);
        drop_column(&a, "priority");
        stamp_version(&a, chain::BASELINE_VERSION);

        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore)
            .expect("a faithful copy of an older store is what a rewind point is");

        let live = base.join("live");
        let live_db = live.join(crate::config::STORE_FILE_NAME);
        let report = restore_staging(
            &archive,
            "20260714T000000Z",
            &live_db,
            Staging::Migrate(READD_PRIORITY),
            &mut crate::progress::ignore,
        )
        .unwrap();

        assert_eq!(report.migration.applied, vec!["re-add task.priority"], "the chain ran on the way in");
        assert_eq!(report.migration.from, chain::BASELINE_VERSION);
        assert_eq!(report.migration.to, 3);
        // Read the file, not the store: a v3 store is past what this build's own gate opens, and the point
        // here is what was swapped in — the migrated snapshot, not the raw one.
        assert_eq!(
            crate::store_engine::probe_format_version(&live_db),
            3,
            "the restored store carries the version the chain took it to — stamped by the step, not blind"
        );
        assert_eq!(task_rows(&live_db), 2, "with the archive's rows");
        assert!(has_task_column(&live_db, "priority"), "and the column the old store lacked is back");
    }

    /// The rollback of a failed migration restores its archive **as taken**: the chain must not run over it,
    /// or the rewind would re-apply the very steps that just failed — and the store it puts back is the
    /// older one the migration started from, by definition. The user-facing restore is the opposite
    /// gesture, and this holds the two apart.
    #[test]
    fn a_rewind_puts_the_archive_back_as_taken_and_the_chain_does_not_run() {
        const ADDS_A_TABLE: &[Step] = &[Step {
            to: 3,
            name: "add later",
            apply: chain::Apply::Sql("CREATE TABLE later (x TEXT);"),
        }];

        let base = scratch("rewind");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice"]);
        stamp_version(&a, chain::BASELINE_VERSION);

        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();

        // ① The user-facing restore brings the archive forward: the live store ends at the chain's head.
        let live = base.join("live");
        let live_db = live.join(crate::config::STORE_FILE_NAME);
        restore_staging(
            &archive,
            "20260714T000000Z",
            &live_db,
            Staging::Migrate(ADDS_A_TABLE),
            &mut crate::progress::ignore,
        )
        .unwrap();
        assert_eq!(crate::store_engine::probe_format_version(&live_db), 3);

        // ② The rollback rewinds the same archive over it: no step runs, and the store is back at the
        //    version — and the shape — it was taken at.
        let report =
            rewind_into(&archive, "20260714T000001Z", &live_db, &mut crate::progress::ignore).unwrap();

        assert!(!report.migration.migrated(), "a rewind runs no step");
        assert_eq!(report.migration.to, chain::BASELINE_VERSION);
        assert_eq!(crate::store_engine::probe_format_version(&live_db), chain::BASELINE_VERSION);
        let conn = Connection::open(&live_db).unwrap();
        assert!(!has_table(&conn, &live_db, "later").unwrap(), "the chain's table is gone with the rewind");
        assert_eq!(task_rows(&live_db), 1, "and the archive's rows are back");
    }

    /// An archive from **before the consolidation** (a `stores` list this build has no type for) is
    /// refused by layout version, before its manifest is parsed and before the live tree is touched. The
    /// refusal has to be a sentence that names the way out — a `serde` error about a missing field is not
    /// one.
    #[test]
    fn restore_refuses_a_pre_consolidation_archive_without_touching_live() {
        let base = scratch("restore-legacy-layout");
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        // Exactly what a v4 build wrote: a list of stores, each naming its id and its path in the tar.
        let legacy = serde_json::json!({
            "archive_layout_version": 4,
            "created_at": "2026-07-08T00:00:00Z",
            "producer_app_version": "0.1.9",
            "producer_format_version": crate::model::FORMAT_VERSION,
            "stores": [{
                "store_id": "store",
                "display_name": null,
                "bindings": [],
                "schema_version": crate::model::SCHEMA_VERSION,
                "format_version": crate::model::FORMAT_VERSION,
                "entry_path": "stores/store/store.sqlite",
                "blob_count": 0,
                "blob_bytes": 0,
            }],
        });
        build_archive(
            &archive,
            &serde_json::to_vec_pretty(&legacy).unwrap(),
            &[("stores/store/store.sqlite", b"never read")],
        );

        // A live store the refusal must leave exactly as it found it.
        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        seed_store(&live, &["untouched"]);
        let live_db = live.join(crate::config::STORE_FILE_NAME);

        let err = restore_into(&archive, "20260708T000000Z", &live_db, &mut crate::progress::ignore)
            .unwrap_err();
        assert_eq!(err.code(), "invalid_restore_layout_too_old");
        assert!(
            err.to_string().contains("layout v4"),
            "the refusal names the layout it found and the way out: {err}"
        );
        // …and it sends that layout apart from the prose, so the screen writes the sentence itself.
        let fields: Vec<_> = err.fields().expect("the values ride along").iter().collect();
        assert_eq!(fields, vec![("layout", "4"), ("min", "5")]);
        assert_eq!(live_tasks(&live_db), 1, "the live store is untouched");
        assert!(
            !live.join("store.pre-restore-20260708T000000Z.sqlite").exists(),
            "a refused restore asides nothing"
        );
    }

    /// The pre-restore aside is the rewind point, so it must carry everything the live store had —
    /// including the transactions still sitting in its `-wal`, which the swap deletes along with the old
    /// filename.
    ///
    /// The store the restore runs against is a **copy** of one whose WAL was left unfolded, rather than the
    /// original with its connection still open. A swap is entitled to find nothing holding the file
    /// (`AMB-D-704`) — on Windows a held handle is refused outright — so a live connection is the wrong way
    /// to hold frames in the WAL. Copying the pair `store.sqlite` + `store.sqlite-wal` reproduces the state
    /// that matters here, and hands the restore a store nobody has open.
    #[test]
    fn the_aside_keeps_transactions_that_lived_only_in_the_wal() {
        let base = scratch("restore-wal");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob", "carol"]); // the archive: 3 tasks
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();

        // The store to be replaced, built in a scratch of its own: one task written through the normal
        // path…
        let built = base.join("built");
        std::fs::create_dir_all(&built).unwrap();
        let built_db = built.join(crate::config::STORE_FILE_NAME);
        seed_store(&built, &["山田さんに連絡する"]);

        // …and a second one committed only to the WAL: holding the connection open keeps the last-close
        // checkpoint from ever folding those frames into the main file.
        let holder = crate::store_engine::StoreEngine::open(&built_db).unwrap();
        put_task(&holder, 99); // A key seed_store's numbering does not reach, so the UPSERT overwrites nothing
        let wal = built_db.with_extension("sqlite-wal");
        assert!(std::fs::metadata(&wal).is_ok_and(|m| m.len() > 0), "the commit is in the WAL");

        // Take the main file and its WAL away as a pair, while the frames are still unfolded — the copy is
        // the live store the restore replaces, and no connection is attached to it. (`-shm` is left behind:
        // it is a rebuildable index, and the next open recovers it from the WAL.)
        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        let dest = live.join(crate::config::STORE_FILE_NAME);
        std::fs::copy(&built_db, &dest).unwrap();
        std::fs::copy(&wal, dest.with_extension("sqlite-wal")).unwrap();
        drop(holder);

        restore_into(&archive, "s", &dest, &mut crate::progress::ignore).unwrap();

        assert_eq!(live_tasks(&dest), 3, "the restored store carries the archive's tasks");
        let aside = dest.with_file_name("store.pre-restore-s.sqlite");
        assert_eq!(task_rows(&aside), 2, "the aside carries the WAL's commit, not just the main file");
    }

    /// A manifest declaring a store newer than this build refuses the whole archive up front — the live
    /// tree is never touched (pre-flight gate).
    #[test]
    fn preflight_rejects_a_too_new_generation_without_touching_live() {
        let base = scratch("restore-toonew");
        let too_new = crate::model::FORMAT_VERSION + 1;
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        build_archive(
            &archive,
            &manifest_json(ARCHIVE_LAYOUT_VERSION, too_new),
            &[(SNAPSHOT_ENTRY, b"never read")],
        );

        // A live sentinel that must remain byte-identical (restore must not reach the swap).
        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        let dest = live.join(crate::config::STORE_FILE_NAME);
        std::fs::write(&dest, b"LIVE-SENTINEL").unwrap();

        let err = restore_into(&archive, "s", &dest, &mut crate::progress::ignore).unwrap_err();
        assert!(
            err.to_string().contains("newer Amenbo"),
            "unexpected error: {err}"
        );
        assert_eq!(std::fs::read(&dest).unwrap(), b"LIVE-SENTINEL", "live tree must be untouched");
    }

    /// A corrupt snapshot fails in staging (bounded verify / forward-migration open) — before any swap
    /// — so the whole restore aborts with the live tree untouched (no partial application).
    #[test]
    fn aborts_before_swap_on_a_corrupt_snapshot() {
        let base = scratch("restore-corrupt");
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        // Not a SQLite database — the forward-migration open fails on it.
        build_archive(
            &archive,
            &manifest_json(ARCHIVE_LAYOUT_VERSION, crate::model::FORMAT_VERSION),
            &[(SNAPSHOT_ENTRY, b"not a database at all")],
        );

        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        let dest = live.join(crate::config::STORE_FILE_NAME);
        std::fs::write(&dest, b"LIVE-SENTINEL").unwrap();

        let err = restore_into(&archive, "s", &dest, &mut crate::progress::ignore).unwrap_err();
        assert!(!err.to_string().is_empty());
        assert_eq!(std::fs::read(&dest).unwrap(), b"LIVE-SENTINEL", "live tree must be untouched");
    }

    /// A manifest declaring a container layout newer than this build reads refuses the whole archive up
    /// front: a newer layout may hold entries this build cannot place, and half a restore is worse than
    /// none.
    #[test]
    fn preflight_rejects_a_newer_archive_layout_without_touching_live() {
        let base = scratch("restore-newlayout");
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        build_archive(
            &archive,
            &manifest_json(ARCHIVE_LAYOUT_VERSION + 1, crate::model::FORMAT_VERSION),
            &[(SNAPSHOT_ENTRY, b"never read")],
        );

        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        let dest = live.join(crate::config::STORE_FILE_NAME);
        std::fs::write(&dest, b"LIVE-SENTINEL").unwrap();

        let err = restore_into(&archive, "s", &dest, &mut crate::progress::ignore).unwrap_err();
        assert!(
            err.to_string().contains("layout v"),
            "unexpected error: {err}"
        );
        assert_eq!(std::fs::read(&dest).unwrap(), b"LIVE-SENTINEL", "live tree must be untouched");
    }

    // ─────────────────────────── attachment blobs ───────────────────────────

    /// An archive carries every attachment blob of the store, and a restore lands the bytes beside the
    /// restored truth source, so `attachment.blob_hash` still resolves on the destination machine.
    #[test]
    fn bundles_and_restores_attachment_blobs() {
        let base = scratch("blobs-rt");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice"]);
        let one = ingest_blob(&a, b"attached bytes");
        let two = ingest_blob(&a, b"more bytes");

        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        let report = backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();
        assert_eq!(report.blobs, 2);

        // The bytes sit flat at `blobs/<hash>`, and the manifest records the totals.
        assert_eq!(
            read_entry(&archive, &format!("blobs/{one}")).as_deref(),
            Some(&b"attached bytes"[..])
        );
        assert!(read_entry(&archive, &format!("blobs/{two}")).is_some());
        let manifest: ArchiveManifest =
            serde_json::from_slice(&read_entry(&archive, MANIFEST_ENTRY).unwrap()).unwrap();
        assert_eq!(manifest.store.blob_count, 2);
        assert_eq!(manifest.store.blob_bytes, 24);

        // Restore into an empty live tree: the blobs land in `blobs/` beside the restored store.
        let live = base.join("live");
        let dest = live.join(crate::config::STORE_FILE_NAME);
        let report = restore_into(&archive, "s", &dest, &mut crate::progress::ignore).unwrap();
        assert_eq!(report.blobs, 2);
        let bs = crate::blob::BlobStore::at(live.join(crate::blob::BLOBS_SUBDIR));
        assert_eq!(bs.read(&one).unwrap(), b"attached bytes");
        assert_eq!(bs.read(&two).unwrap(), b"more bytes");
    }

    /// Placing blobs is additive and idempotent: a hash the destination already holds is left alone (the
    /// bytes are identical by content-addressing), so a second restore writes none and destroys none.
    #[test]
    fn restoring_twice_leaves_present_blobs_alone() {
        let base = scratch("blobs-idem");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice"]);
        let attached = ingest_blob(&a, b"the attachment body");

        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();

        let live = base.join("live");
        let dest = live.join(crate::config::STORE_FILE_NAME);
        let first = restore_into(&archive, "s1", &dest, &mut crate::progress::ignore).unwrap();
        assert_eq!(first.blobs, 1);
        let second = restore_into(&archive, "s2", &dest, &mut crate::progress::ignore).unwrap();
        assert_eq!(second.blobs, 0, "an already-present blob is not rewritten");

        let bs = crate::blob::BlobStore::at(live.join(crate::blob::BLOBS_SUBDIR));
        assert_eq!(bs.read(&attached).unwrap(), b"the attachment body");
    }

    /// The trust boundary for archive-controlled paths: only the snapshot and a `blobs/<BLAKE3 hex>` byte
    /// are ever materialised. Everything else — a traversal attempt, an ingest-staging leftover, an unknown
    /// directory, the legacy `pinned/` nesting — is skipped rather than written.
    #[test]
    fn staging_dest_places_only_the_snapshot_and_content_addressed_blobs() {
        let stage = Path::new("/stage");
        let hash = "a".repeat(64);
        let dest = |name: &str| staging_dest(name, stage);

        assert_eq!(dest(SNAPSHOT_ENTRY), Some(stage.join(SNAPSHOT_ENTRY)));
        assert_eq!(
            dest(&format!("blobs/{hash}")),
            Some(stage.join(crate::blob::BLOBS_SUBDIR).join(&hash))
        );
        // Rejected: ingest staging, an unknown directory, a non-hash name, a traversal, the legacy
        // spellings, an unrelated entry.
        assert_eq!(dest("blobs/tmp/0123456789abcdef"), None);
        assert_eq!(dest(&format!("blobs/evil/{hash}")), None);
        assert_eq!(dest("blobs/NOT-A-HASH"), None);
        assert_eq!(dest(&format!("blobs/pinned/{hash}")), None);
        assert_eq!(dest("blobs/pinned/../../../etc/passwd"), None);
        assert_eq!(dest(&format!("stores/store/blobs/{hash}")), None);
        assert_eq!(dest("stores/store/store.sqlite"), None);
        assert_eq!(dest(MANIFEST_ENTRY), None);
    }

    /// An I/O failure in the swap puts the aside back, so a failed restore never leaves the live store
    /// half-replaced. The failure is injected where a real one lives — the aside the swap copies the live
    /// store to — by occupying that path with a directory.
    #[test]
    fn a_failed_swap_puts_the_live_store_back() {
        let base = scratch("restore-rollback");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob", "carol"]); // the archive: 3 tasks
        let archive = base.join(format!("backup.{ARCHIVE_EXT}"));
        backup_from(&source(&a), &archive, &mut crate::progress::ignore).unwrap();

        // The live store the restore would replace, with OLD content it must keep.
        let live = base.join("live");
        std::fs::create_dir_all(&live).unwrap();
        seed_store(&live, &["old-1", "old-2"]); // 2 live
        std::fs::create_dir_all(live.join("store.pre-restore-roll.sqlite")).unwrap();

        let dest = live.join(crate::config::STORE_FILE_NAME);
        restore_into(&archive, "roll", &dest, &mut crate::progress::ignore)
            .expect_err("the aside cannot be written");

        assert_eq!(live_tasks(&dest), 2, "the live store keeps its own content");
        assert!(
            !live.join("store.incoming-roll.sqlite").exists(),
            "and the file the swap was going to rename in is gone"
        );
    }

    /// The release-stamp gate stands at the front of a restore over this device's live store
    /// (`AMB-D-378`): a test binary carries no release stamp, so the refusal is what comes back — and it
    /// comes back *before* the archive is even read, which is why naming one that does not exist is safe
    /// here. Nothing in the live tree is touched either way.
    ///
    /// Skipped when the run is isolated by `AMENBO_HOME`, which is a deliberate arm of the gate (an
    /// isolated store is nobody's production data) rather than a case this test could assert.
    #[test]
    fn an_unstamped_build_cannot_restore_over_this_devices_store() {
        if crate::env::home().is_some() {
            return;
        }
        let missing = scratch("restore-gate").join(format!("nothing.{ARCHIVE_EXT}"));
        let err = restore_into(&missing, "S", &restore_dest(), &mut crate::progress::ignore)
            .expect_err("an unstamped build must not carry the live store forward");
        assert!(
            format!("{err:?}").contains("AMENBO_ALLOW_UNSTAMPED_MIGRATE"),
            "the gate refused, not the missing archive: {err:?}"
        );
    }

    /// A restore into a file the caller named is not gated — the archive is carried forward over *their*
    /// file, not over the device's store. This is also what keeps the suites above running: they restore
    /// into scratch directories from an unstamped test binary.
    #[test]
    fn a_restore_into_a_named_file_is_not_gated() {
        let dest = scratch("restore-ungated").join(crate::config::STORE_FILE_NAME);
        assert!(ensure_may_restore_over(&dest).is_ok());
    }

    /// SQLite names no file in `file is not a database` — so every failure that opened a file by path has
    /// to say which one, or nobody can act on it.
    #[test]
    fn a_failure_on_a_store_file_names_the_file() {
        let dir = scratch("names-the-file");
        let good = dir.join("good.sqlite");
        seed_store(&dir, &["生きているタスク"]);
        std::fs::rename(dir.join(crate::config::STORE_FILE_NAME), &good).unwrap();

        // Not a database at all.
        let junk = dir.join("junk.sqlite");
        std::fs::write(&junk, "これはデータベースではない").unwrap();

        for err in [
            verify_snapshot_current_schema(&junk).unwrap_err(),
            verify_snapshot_mirrors_source(&junk, &good).unwrap_err(),
            verify_snapshot_mirrors_source(&good, &junk).unwrap_err(),
            checkpoint(&junk).unwrap_err(),
        ] {
            assert!(
                err.to_string().contains("junk.sqlite"),
                "the failure must name the file it happened on, got: {err}"
            );
        }

        // A snapshot that is a healthy database but not the store it claims to copy names both files.
        let empty = dir.join("empty.sqlite");
        Connection::open(&empty).unwrap().execute_batch("CREATE TABLE unrelated (x)").unwrap();
        let err = verify_snapshot_mirrors_source(&empty, &good).unwrap_err().to_string();
        assert!(err.contains("empty.sqlite") && err.contains("good.sqlite"), "got: {err}");
    }
}
