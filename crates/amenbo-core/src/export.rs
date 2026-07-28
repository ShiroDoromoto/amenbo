//! Export — the heart of owning your own data.
//!
//! It exists for exactly one purpose, **moving to another tool**, so there is exactly one road:
//! **everything, as JSON**. No excerpts, and no presentation formats (markdown, csv) — neither helps you
//! leave.
//!
//! - **An export directory** ([`export_bundle`]): `export.json` — a plain shape, 1:1 with the logical
//!   schema — beside `attachments/`, holding each attachment's bytes under a path that names the target it
//!   hung on. This is the migration artifact, and it is **one-way**: nothing reads it back into amenbo.
//!   (Getting your own data *back* is what backup and restore are for.)
//! - Without `--out`: the same JSON, simply streamed (for a pipe — attachment bytes have nowhere to go).

use std::io::Write;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::progress::{Phase, Progress};
use crate::store_engine::schema::{Dataset, DATASETS};
use crate::time::Timestamp;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ─────────────────── whole-device streaming export ───────────────────
//
// The migration-facing export of the whole device — one database, and the only export there is: nothing
// narrower than everything, nothing but JSON.
//
// It never constructs a `Database`: it opens the store read-only at the SQLite `Connection` layer and
// streams **one row at a time** straight to a `Write` (`serde_json::to_writer`), so peak memory is a
// small constant regardless of store size. Export is for moving data to *other tools*, so the shape is
// deliberately plain and generic — each read-model table becomes a JSON array of `{column: value}` objects
// read straight off the row, not a typed model round-trip (the user's AI adapts it; strict reversibility
// is a non-goal).
//
// A single stream file — **no splitting**. The row loop wraps each materialized row in a
// `crate::perf::MaterializedRows` guard so the `scale` guard can prove the peak stays N-independent.

/// Container/layout version of the whole-device JSON export (distinct from the read-model
/// `schema_version`). Bump only if the envelope below changes shape — it is a label for whoever reads the
/// file *outside* amenbo, since nothing reads it back in. Version `2` is
/// `{"amenbo_export": …, "tables": {…}}`.
const EXPORT_VERSION: u32 = 2;

/// Format tag written into the export header so a reader can recognise the file at a glance.
const EXPORT_FORMAT: &str = "amenbo-export-json";

/// Header object of a whole-device export (`amenbo_export` key). Records the producing binary and the
/// envelope version so a consumer knows what it is holding before reading the (potentially huge)
/// `tables` object.
#[derive(Debug, Clone, Serialize)]
struct ExportHeader {
    /// Format tag ([`EXPORT_FORMAT`]).
    format: &'static str,
    /// Envelope layout version ([`EXPORT_VERSION`]).
    format_version: u32,
    /// Read-model schema version of the producing binary ([`crate::model::SCHEMA_VERSION`]).
    schema_version: &'static str,
    /// Producing binary's human-readable version.
    app_version: &'static str,
    /// When the export was produced (RFC3339).
    exported_at: String,
}

/// Error returned when a progress callback asks to cancel a streaming export.
fn cancelled() -> Error {
    Error::invalid("export cancelled")
}

/// Serialize `value` as JSON straight to `w` (bounded — one value, not the whole document).
fn write_json<T: Serialize>(w: &mut impl Write, value: &T) -> Result<()> {
    serde_json::to_writer(w, value).map_err(Error::from)
}

/// Map one SQLite cell to a JSON value. All read-model columns are TEXT/INTEGER today; `Real`/`Blob`
/// are handled defensively (a blob is base64 so the export never loses or corrupts bytes).
fn cell_to_json(v: rusqlite::types::ValueRef<'_>) -> serde_json::Value {
    use base64::Engine as _;
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => serde_json::Value::from(i),
        ValueRef::Real(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        ValueRef::Text(bytes) => serde_json::Value::String(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Blob(bytes) => {
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
    }
}

/// Whether `table` exists in `conn` (an older/partial store may lack a table a newer registry
/// declares; such a table exports as an empty array rather than failing the whole device). Raw SQL by
/// necessity: the question is precisely *"does the store have the table the registry declares?"*, so the
/// answer has to come from the store's own catalogue (`sqlite_master`) — a table `col::` does not name, and
/// could not answer with if it did.
fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )
    .map_err(crate::error::sqlite_on(conn))
}

/// Stream one read-model table as a JSON array of `{column: value}` objects, **one row at a time**
/// (O(1) memory). Every row is live, so the whole table streams. Cancellation is polled every
/// [`CANCEL_POLL_ROWS`] rows (cheap — a whole-table scan can't be cut mid-statement, but a huge table
/// still yields often enough). The caller has already written the `"table":` key and expects this to
/// emit the `[...]` value. With a `bundle`, the `attachment` table's rows also carry the blob out to disk
/// as they pass through, and each row gains the `export_path` it was written to. Those rows poll **every**
/// row, because one of them is a whole file — the cost is bytes, not rows. A single file is still copied
/// whole (`std::fs::copy` is one call): we do not chunk it to make the copy interruptible, so cancelling
/// lands between attachments, not inside one.
fn stream_table(
    conn: &Connection,
    dataset: &Dataset,
    w: &mut impl Write,
    bundle: Option<&mut AttachmentBundle>,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<()> {
    let mut bundle = bundle.filter(|_| dataset.table == ATTACHMENT_TABLE);
    if !table_exists(conn, dataset.table)? {
        w.write_all(b"[]")?;
        return Ok(());
    }

    // `id` (the row key) plus every registry column; the audit columns are included by `all_columns`.
    // Quote each identifier so it can never be mistaken for SQL. Every row is live, so the whole table
    // streams — there is no tombstone to filter out.
    let cols: Vec<&str> =
        std::iter::once("id").chain(dataset.all_columns().map(|c| c.name)).collect();
    let select_list = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let sql = format!("SELECT {select_list} FROM \"{}\"", dataset.table);

    let fail = crate::error::sqlite_on(conn);
    let mut stmt = conn.prepare(&sql).map_err(&fail)?;
    let mut rows = stmt.query([]).map_err(&fail)?;

    w.write_all(b"[")?;
    let mut first = true;
    let mut seen: u64 = 0;
    while let Some(row) = rows.next().map_err(&fail)? {
        // Exactly one row lives in memory at a time (the streaming invariant the scale guard checks).
        let _held = crate::perf::MaterializedRows::hold(1);
        let mut obj = serde_json::Map::with_capacity(cols.len());
        for (i, name) in cols.iter().enumerate() {
            obj.insert((*name).to_string(), cell_to_json(row.get_ref(i).map_err(&fail)?));
        }
        if let Some(bundle) = bundle.as_deref_mut() {
            let placed = bundle.place(&obj)?;
            obj.insert(
                EXPORT_PATH_FIELD.to_string(),
                placed.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
            );
        }
        if !first {
            w.write_all(b",")?;
        }
        first = false;
        write_json(w, &serde_json::Value::Object(obj))?;

        seen += 1;
        // Rows that carry bytes are polled **every** row: a row here is a whole file copied out, so the
        // work is measured in bytes, not rows. Waiting 256 of them means an export of a handful of huge
        // attachments never reaches the callback at all — the progress bar stands still and the Cancel
        // button does nothing until the copying is over.
        let every = if bundle.is_some() { 1 } else { CANCEL_POLL_ROWS };
        if seen.is_multiple_of(every)
            && progress(&Progress {
                phase: Phase::Exporting,
                done: seen,
                total: None,
            })
            .is_break()
        {
            return Err(cancelled());
        }
    }
    w.write_all(b"]")?;
    Ok(())
}

/// How often the row loop polls the progress callback for cancellation. Coarse enough to add no real
/// overhead, fine enough that a multi-million-row table still yields promptly. The attachment rows of a
/// bundling export poll every row instead — see [`stream_table`].
const CANCEL_POLL_ROWS: u64 = 256;

/// Stream a store's whole read model — the `{ "task": [...], "project": [...], … }` object — to `w`,
/// one row at a time (O(1) memory). `datasets` is the registry to walk ([`DATASETS`]). Public so the
/// scale guard can exercise the streaming core directly against an in-memory connection; the
/// whole-device export writes it as the document's `tables` value. `bundle` is `Some` when the export is
/// writing a directory — the attachment rows then also carry their bytes out (see [`AttachmentBundle`]);
/// `None` streams records only (stdout).
pub fn stream_store_tables(
    conn: &Connection,
    datasets: &[Dataset],
    w: &mut impl Write,
    mut bundle: Option<&mut AttachmentBundle>,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<()> {
    w.write_all(b"{")?;
    for (i, dataset) in datasets.iter().enumerate() {
        if i > 0 {
            w.write_all(b",")?;
        }
        write_json(w, &dataset.table)?;
        w.write_all(b":")?;
        stream_table(conn, dataset, w, bundle.as_deref_mut(), progress)?;
    }
    w.write_all(b"}")?;
    Ok(())
}

// ─────────────────── attachment bytes: the export directory ───────────────────
//
// Export is **one-way**: nothing reads it back into amenbo, so whatever it fails to carry is simply lost
// on the way out. Attachment metadata alone (`blob_hash` — a content address that means nothing outside
// this device) is not "your data, taken with you". So the migration-facing export is a **directory**:
// `export.json` next to `attachments/`, the blobs laid out under the target they hang on. Bytes are copied
// file-to-file as the attachment rows stream past, so the O(1)-memory invariant the scale guard proves
// still holds — no blob is ever held in memory.

/// The records file inside an export directory.
pub const BUNDLE_JSON_NAME: &str = "export.json";
/// The attachment subdirectory inside an export directory.
pub const BUNDLE_ATTACHMENTS_DIR: &str = "attachments";
/// Read-model table whose rows carry blobs (the one table [`AttachmentBundle`] acts on).
const ATTACHMENT_TABLE: &str = "attachment";
/// Field appended to an exported attachment row: the relative path its bytes were written to.
/// `null` for a URL attachment, or when the blob is no longer on disk. Written because the filename is
/// sanitised for the filesystem — a consumer must not have to guess how.
const EXPORT_PATH_FIELD: &str = "export_path";
/// Cap on the filename component taken from the attachment (whole path stays well inside every OS limit).
const MAX_NAME_CHARS: usize = 80;

/// What an export directory ended up holding.
#[derive(Debug, Clone, Serialize)]
pub struct BundleReport {
    /// The directory that was written.
    pub path: String,
    /// Bytes written into that directory — `export.json` **plus every attachment file**: the artifact is
    /// the directory, so its size is the directory's.
    pub bytes: u64,
    /// Attachment files written under `attachments/`.
    pub attachments: u64,
    /// Attachments whose blob was no longer on disk — their `export_path` is `null` (never silent).
    pub missing: u64,
}

/// Copies each attachment's bytes out to `attachments/<target_type>-<target_id>/<id>-<filename>` as the
/// row streams past, and hands back the relative path to write into the row. Owns no bytes: the copy is
/// file-to-file.
pub struct AttachmentBundle {
    blobs: crate::blob::BlobStore,
    dir: std::path::PathBuf,
    written: u64,
    /// Bytes copied out (the size the export directory owes to attachments).
    bytes: u64,
    missing: u64,
}

impl AttachmentBundle {
    /// `blobs_dir` is the store's `blobs/` directory; `dir` is the export directory (its
    /// `attachments/` subdirectory is created lazily — an export with no attachment leaves no empty dir).
    pub fn new(blobs_dir: &Path, dir: &Path) -> Self {
        Self {
            blobs: crate::blob::BlobStore::at(blobs_dir.to_path_buf()),
            dir: dir.join(BUNDLE_ATTACHMENTS_DIR),
            written: 0,
            bytes: 0,
            missing: 0,
        }
    }

    /// Write one attachment row's blob out. Returns the relative path recorded in the row, or `None`
    /// for a row that carries no bytes (a URL attachment, or a blob that is no longer on disk).
    fn place(&mut self, row: &serde_json::Map<String, serde_json::Value>) -> Result<Option<String>> {
        let s = |k: &str| row.get(k).and_then(|v| v.as_str());
        let Some(hash) = s("blob_hash") else { return Ok(None) }; // URL attachment
        let Some(src) = self.blobs.path(hash) else {
            // The row outlived its bytes (a hand-pruned blobs/ dir). Say so in the count rather than
            // pretending the export is complete.
            self.missing += 1;
            return Ok(None);
        };

        let id = row.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        let target = format!(
            "{}-{}",
            safe_name(s("target_type").unwrap_or("attachment")),
            row.get("target_id").and_then(|v| v.as_i64()).unwrap_or(0),
        );
        // `id` prefixes the name, so two same-named files on the same target cannot collide.
        let name = format!("{id}-{}", safe_name(s("filename").unwrap_or("")));

        let dir = self.dir.join(&target);
        std::fs::create_dir_all(&dir)?;
        self.bytes += std::fs::copy(&src, dir.join(&name))?;
        self.written += 1;
        Ok(Some(format!("{BUNDLE_ATTACHMENTS_DIR}/{target}/{name}")))
    }
}

/// One path component of an attachment's own filename, made safe for every OS: anything that is not a
/// letter, digit, or one of `. - _ ( )` becomes `_` (so a separator, a control char, or a shell
/// metacharacter cannot escape the directory), leading/trailing dots and spaces go (Windows), and the
/// length is capped. Japanese and other non-ASCII letters survive — the point is a *readable* name.
fn safe_name(raw: &str) -> String {
    let mapped: String = raw
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '(' | ')') { c } else { '_' })
        .collect();
    let trimmed = mapped.trim_matches(|c: char| c == '.' || c == '_');
    let capped: String = trimmed.chars().take(MAX_NAME_CHARS).collect();
    if capped.is_empty() {
        "attachment".to_string()
    } else {
        capped
    }
}

/// Stream a whole-device export of the database at `db_path` to `w` as a single JSON document (one stream
/// file, no splitting). Kept separate from [`crate::config::Paths`] resolution — like
/// [`crate::archive::backup_from`] — so it is unit-testable against a hand-built store; the OS-glue entry
/// point is [`export_json`]. The document is `{"amenbo_export": <header>, "tables": {…}}`: one database, so
/// there is no per-store envelope to write. The source is opened **read-only** at the connection layer (no
/// migration, no `Database` hydrate) and its datasets streamed a row at a time. On a progress cancellation
/// the caller owns removing the partial output; this returns the cancellation error at the next boundary.
pub fn export_json_from(
    db_path: &Path,
    w: &mut impl Write,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<()> {
    stream_json(db_path, w, None, progress)
}

/// The shared body of the two whole-device JSON paths: records only (`bundle` = `None`, the stdout
/// stream) or records plus attachment bytes (`bundle` = `Some`, the export directory).
fn stream_json(
    db_path: &Path,
    w: &mut impl Write,
    bundle: Option<&mut AttachmentBundle>,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<()> {
    let header = ExportHeader {
        format: EXPORT_FORMAT,
        format_version: EXPORT_VERSION,
        schema_version: crate::model::SCHEMA_VERSION,
        app_version: APP_VERSION,
        exported_at: Timestamp::now().0.to_rfc3339(),
    };

    if progress(&Progress {
        phase: Phase::Exporting,
        done: 0,
        total: Some(1),
    })
    .is_break()
    {
        return Err(cancelled());
    }

    // Open read-only — export must never mutate (or migrate) the source it is copying out of.
    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(crate::error::sqlite_at(db_path))?;

    w.write_all(b"{\"amenbo_export\":")?;
    write_json(w, &header)?;
    w.write_all(b",\"tables\":")?;
    stream_store_tables(&conn, DATASETS, w, bundle, progress)?;
    w.write_all(b"}")?;
    Ok(())
}

/// Stream a whole-device export of **this device** to `w` — the single database at
/// `<app-data>/store.sqlite` (or `<AMENBO_HOME>/store.sqlite`). Thin OS-layout glue over
/// [`export_json_from`]; refuses when this device holds no store yet (there is nothing to export, and an
/// empty document would look like a successful one). **Records only** — a stream has nowhere to put the
/// attachment bytes. The complete migration artifact is the export *directory* ([`export_bundle`]).
pub fn export_json(
    w: &mut impl Write,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<()> {
    let db_path = crate::config::resolve_store_file(&crate::config::Paths::user_base());
    if !db_path.is_file() {
        return Err(Error::invalid("nothing to export: this device holds no store"));
    }
    export_json_from(&db_path, w, progress)
}

/// Where an export is built before it earns its name: a sibling of `dest`, so the rename that puts it in
/// place is within one directory — hence one filesystem, hence atomic. The leading dot and the
/// [`crate::tmpdir::suffix`] say "nobody's data" to whoever finds one left by a killed process.
fn staging_beside(dest: &Path) -> PathBuf {
    let name = format!(".amenbo-export-{}", crate::tmpdir::suffix());
    match dest.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

/// Write the migration-facing export of the database at `db_path` as a **directory** at `dest`:
/// `export.json` plus `attachments/` carrying every blob's bytes. Kept free of [`crate::config::Paths`]
/// resolution so it is testable against a hand-built store; the OS-glue entry point is [`export_bundle`].
/// `dest` must not exist yet, or be an empty directory — an export must never fold into someone else's
/// files.
///
/// **`dest` appears only once the export is whole**: every byte is written into a staging directory
/// beside it ([`staging_beside`]) and renamed into place at the end, so a run that dies partway leaves no
/// `export.json` at all rather than a truncated one wearing a normal name. Export is the whole of the
/// no-lock-in promise, and a half-file that parses as garbage on the far side breaks it in the worst
/// way — you find out after the data is gone. Removing the partial on the `Err` path is not enough
/// on its own: the progress sink is the caller's code and can *panic* (the CLI writes its line with
/// `eprintln!`, which panics when its pipe is closed — `export --out d 2>&1 | head -3`), and an unwind
/// runs no `Err` arm. So the staging directory is held by a [`crate::archive::DirGuard`], which cleans up
/// on both paths; the rename is what makes `dest` all-or-nothing.
pub fn export_bundle_from(
    db_path: &Path,
    blobs_dir: &Path,
    dest: &Path,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<BundleReport> {
    if dest.exists() {
        let empty = dest.is_dir()
            && std::fs::read_dir(dest).map(|mut d| d.next().is_none()).unwrap_or(false);
        if !empty {
            return Err(Error::invalid(format!("cannot export into {}: it already exists", dest.display())));
        }
    }

    let staging = staging_beside(dest);
    let _guard = crate::archive::DirGuard(staging.clone());
    std::fs::create_dir_all(&staging)?;

    let mut bundle = AttachmentBundle::new(blobs_dir, &staging);
    {
        let mut w = std::io::BufWriter::new(std::fs::File::create(staging.join(BUNDLE_JSON_NAME))?);
        stream_json(db_path, &mut w, Some(&mut bundle), progress)?;
        w.flush()?;
    }

    // An empty `dest` was accepted above, and a rename needs the name free. Removing it opens a window
    // in which `dest` is absent — harmless, since what it held was nothing.
    if dest.exists() {
        std::fs::remove_dir(dest)?;
    }
    std::fs::rename(&staging, dest)?;

    // What was written is one directory, so its size is stated as the directory's.
    let json_bytes = std::fs::metadata(dest.join(BUNDLE_JSON_NAME)).map(|m| m.len()).unwrap_or(0);
    Ok(BundleReport {
        path: dest.display().to_string(),
        bytes: json_bytes + bundle.bytes,
        attachments: bundle.written,
        missing: bundle.missing,
    })
}

/// Write **this device's** export directory to `dest` — thin OS-layout glue over
/// [`export_bundle_from`], siblings with [`export_json`]. Refuses when this device holds no
/// store yet.
pub fn export_bundle(
    dest: &Path,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<BundleReport> {
    let base = crate::config::Paths::user_base();
    let db_path = crate::config::resolve_store_file(&base);
    if !db_path.is_file() {
        return Err(Error::invalid("nothing to export: this device holds no store"));
    }
    export_bundle_from(&db_path, &base.join(crate::blob::BLOBS_SUBDIR), dest, progress)
}

#[cfg(test)]
mod export_tests {
    use super::*;
    use crate::config::Paths;
    use crate::store::Store;
    use std::path::Path;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = amenbo_scratch::scratch(&format!("export-{tag}"));
        dir
    }

    /// Open a real store in `dir` and seed it with `titles` tasks (persisted to `store.sqlite`).
    fn seed_store(dir: &Path, titles: &[&str]) {
        let mut s = Store::open_at(Paths::at(dir.to_path_buf())).unwrap();
        for title in titles {
            s.add_task(crate::ops::task::NewTask {
                title: (*title).into(),
                project_id: None,
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
            })
            .unwrap();
        }
    }

    fn store_file(dir: &Path) -> std::path::PathBuf {
        dir.join(crate::config::STORE_FILE_NAME)
    }

    /// The document carries the store's tables directly — no `stores[]`, no per-store envelope.
    #[test]
    fn export_streams_the_single_database_as_valid_json() {
        let base = scratch("rt");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice", "bob", "carol"]);

        let json = {
            let mut buf = Vec::new();
            export_json_from(&store_file(&a), &mut buf, &mut crate::progress::ignore).unwrap();
            String::from_utf8(buf).unwrap()
        };

        // Parses as JSON (proves the hand-written streaming produced well-formed output).
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(doc["amenbo_export"]["format"], EXPORT_FORMAT);
        assert_eq!(doc["amenbo_export"]["format_version"], EXPORT_VERSION);
        assert!(doc["stores"].is_null(), "there is no per-store envelope");

        // The tasks are all present, read straight off the row (title column preserved).
        let titles = task_titles(&doc);
        assert_eq!(titles.len(), 3);
        assert!(titles.contains(&"alice") && titles.contains(&"bob") && titles.contains(&"carol"));
    }

    /// A delete is physical, so a deleted task is in **no** export — there is nothing left for an
    /// `--include-deleted` flag to surface.
    #[test]
    fn a_deleted_task_is_in_no_export() {
        let base = scratch("del");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();

        // Seed three tasks, then delete one.
        let mut s = Store::open_at(Paths::at(a.clone())).unwrap();
        let mut ids = Vec::new();
        for title in ["keep1", "drop", "keep2"] {
            let t = s.add_task(crate::ops::task::NewTask {
                title: title.into(),
                project_id: None,
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
            })
            .unwrap();
            ids.push(t.id);
        }
        s.delete_task(ids[1], crate::model::ActorKind::Human).unwrap();

        let exported = export_one(&store_file(&a));
        let titles = task_titles(&exported);
        assert_eq!(titles.len(), 2, "the deleted row is gone, not tombstoned");
        assert!(!titles.contains(&"drop"), "the deleted task is not in the export");
    }

    #[test]
    fn cancellation_surfaces_as_an_error() {
        let base = scratch("cancel");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["x"]);

        // Cancel on the very first tick.
        let mut buf = Vec::new();
        let mut cb = |_p: &Progress| ControlFlow::Break(());
        let err = export_json_from(&store_file(&a), &mut buf, &mut cb).unwrap_err();
        assert!(err.to_string().contains("cancel"));
    }

    /// Cancelling reaches the copying. A store with a handful of attachments has far fewer than
    /// `CANCEL_POLL_ROWS` rows to pass, so a row-count poll never fires while the bytes are being
    /// written — the user watches a still progress bar and a dead Cancel button until it is over.
    #[test]
    fn cancelling_lands_between_attachments_not_only_after_them() {
        let base = scratch("cancel-bundle");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();

        let big = {
            let mut s = Store::open_at(Paths::at(a.clone())).unwrap();
            let t = s
                .add_task(crate::ops::task::NewTask {
                    title: "alice".into(),
                    project_id: None,
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: None,
                })
                .unwrap();
            // Two attachments — the second must never be copied once the first tick asks to stop.
            for bytes in [b"one".as_slice(), b"two".as_slice()] {
                let blob = s.blobs().ingest_bytes(bytes).unwrap();
                s.attach_blob(
                    crate::model::AttachmentTarget::Task,
                    t.id,
                    &blob.hash,
                    "f.bin",
                    None,
                    blob.size_bytes as i64,
                    crate::model::ActorKind::Ai,
                )
                .unwrap();
            }
            a.join(crate::blob::BLOBS_SUBDIR)
        };

        // Break on the first tick that reports an attachment row (`done` counts rows within the table).
        let dest = base.join("out");
        let mut seen_attachment_tick = false;
        let mut cb = |p: &Progress| {
            if p.done > 0 && !seen_attachment_tick {
                seen_attachment_tick = true;
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        };
        let err =
            export_bundle_from(&store_file(&a), &big, &dest, &mut cb).unwrap_err();
        assert!(err.to_string().contains("cancel"));
        assert!(!dest.exists(), "a cancelled export leaves no half-written directory");
    }

    /// A progress sink is the caller's code, and it can panic rather than return: the CLI writes its
    /// progress line with `eprintln!`, which panics when the pipe it writes to is gone
    /// (`amenbo export --out d 2>&1 | head -3` — the reader leaves after three lines). An unwind runs no
    /// `Err` arm, so cleanup hung off the error path is exactly the cleanup that does not happen here —
    /// and what stayed behind was a truncated `export.json` under the name of a finished export.
    #[test]
    fn a_progress_sink_that_panics_leaves_no_export_behind() {
        let base = scratch("panic-bundle");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();

        let blobs = {
            let mut s = Store::open_at(Paths::at(a.clone())).unwrap();
            let t = s
                .add_task(crate::ops::task::NewTask {
                    title: "alice".into(),
                    project_id: None,
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: None,
                })
                .unwrap();
            // Two attachments: the rows poll every tick, so the panic lands with `export.json` part-written.
            for bytes in [b"one".as_slice(), b"two".as_slice()] {
                let blob = s.blobs().ingest_bytes(bytes).unwrap();
                s.attach_blob(
                    crate::model::AttachmentTarget::Task,
                    t.id,
                    &blob.hash,
                    "f.bin",
                    None,
                    blob.size_bytes as i64,
                    crate::model::ActorKind::Ai,
                )
                .unwrap();
            }
            a.join(crate::blob::BLOBS_SUBDIR)
        };

        let dest = base.join("out");
        let mut seen_attachment_tick = false;
        let mut cb = |p: &Progress| {
            if p.done > 0 && !seen_attachment_tick {
                seen_attachment_tick = true;
                panic!("the progress sink died mid-stream");
            }
            ControlFlow::Continue(())
        };
        let db = store_file(&a);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            export_bundle_from(&db, &blobs, &dest, &mut cb)
        }));

        assert!(caught.is_err(), "the sink's panic propagates — it is not an error to swallow");
        assert!(!dest.exists(), "an export that died mid-stream must not exist under a finished name");
        // Nor may the staging directory outlive the unwind that abandoned it.
        let leftovers: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".amenbo-export-"))
            .collect();
        assert!(leftovers.is_empty(), "the staging dir was left behind: {leftovers:?}");
    }

    /// The export directory carries the attachment's **bytes**, laid out under the target it hangs on, and
    /// the row says where they went — that is what makes a one-way export a migration rather than a lossy
    /// dump.
    #[test]
    fn the_export_directory_carries_the_attachment_bytes_and_says_where_they_went() {
        let base = scratch("bundle");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();

        let (task_id, hash) = {
            let mut s = Store::open_at(Paths::at(a.clone())).unwrap();
            let t = s
                .add_task(crate::ops::task::NewTask {
                    title: "alice".into(),
                    project_id: None,
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: None,
                })
                .unwrap();
            let blob = s.blobs().ingest_bytes(b"the bytes").unwrap();
            s.attach_blob(
                crate::model::AttachmentTarget::Task,
                t.id,
                &blob.hash,
                "設計 メモ:v1.png",
                Some("image/png"),
                blob.size_bytes as i64,
                crate::model::ActorKind::Ai,
            )
            .unwrap();
            // A URL attachment has no bytes to carry — it must not invent a file.
            s.attach_url(
                crate::model::AttachmentTarget::Task,
                t.id,
                "https://example.com/x",
                None,
                crate::model::ActorKind::Ai,
            )
            .unwrap();
            (t.id, blob.hash)
        };

        let dest = base.join("out");
        let report = export_bundle_from(
            &store_file(&a),
            &a.join(crate::blob::BLOBS_SUBDIR),
            &dest,
            &mut crate::progress::ignore,
        )
        .unwrap();
        assert_eq!(report.attachments, 1, "the blob attachment is carried out");
        assert_eq!(report.missing, 0);

        let doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dest.join(BUNDLE_JSON_NAME)).unwrap()).unwrap();
        let rows = doc["tables"]["attachment"].as_array().unwrap();
        assert_eq!(rows.len(), 2);

        let blob_row = rows.iter().find(|r| r["blob_hash"].as_str() == Some(&hash)).unwrap();
        let rel = blob_row[EXPORT_PATH_FIELD].as_str().expect("the row says where its bytes went");
        // The path names the target it hung on — readable, and joinable back to the JSON row.
        assert!(rel.starts_with(&format!("attachments/task-{task_id}/")), "{rel}");
        assert!(rel.ends_with(".png"), "the original name survives sanitising: {rel}");
        assert_eq!(std::fs::read(dest.join(rel)).unwrap(), b"the bytes");

        let url_row = rows.iter().find(|r| r["blob_hash"].is_null()).unwrap();
        assert!(url_row[EXPORT_PATH_FIELD].is_null(), "a URL attachment carries no file");

        // The size is the directory's, not the JSON's — an export whose bytes are mostly attachments must
        // not report the size of `export.json` alone.
        let json_bytes = std::fs::metadata(dest.join(BUNDLE_JSON_NAME)).unwrap().len();
        assert_eq!(
            report.bytes,
            json_bytes + b"the bytes".len() as u64,
            "the report counts the attachment files it wrote, not just export.json",
        );
    }

    /// The bytes are copied file-to-file as the rows stream past, so bundling holds no more rows in
    /// memory than the plain stream does (the O(1) invariant `export_scaling_guard` proves).
    #[test]
    fn bundling_the_bytes_holds_no_more_rows_in_memory() {
        let base = scratch("bundle-o1");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        {
            let mut s = Store::open_at(Paths::at(a.clone())).unwrap();
            for i in 0..20 {
                let t = s
                    .add_task(crate::ops::task::NewTask {
                        title: format!("t{i}"),
                        project_id: None,
                        due_on: None,
                        start_on: None,
                        priority: None,
                        notes: String::new(),
                        created_by_kind: None,
                    })
                    .unwrap();
                let blob = s.blobs().ingest_bytes(format!("bytes {i}").as_bytes()).unwrap();
                s.attach_blob(
                    crate::model::AttachmentTarget::Task,
                    t.id,
                    &blob.hash,
                    "a.bin",
                    None,
                    blob.size_bytes as i64,
                    crate::model::ActorKind::Ai,
                )
                .unwrap();
            }
        }

        crate::perf::reset_row_watermark();
        let report = export_bundle_from(
            &store_file(&a),
            &a.join(crate::blob::BLOBS_SUBDIR),
            &base.join("out"),
            &mut crate::progress::ignore,
        )
        .unwrap();
        assert_eq!(report.attachments, 20);
        assert!(
            crate::perf::peak_materialized_rows() <= 4,
            "bundling must stream: peak {} rows",
            crate::perf::peak_materialized_rows()
        );
    }

    /// An empty directory is a place, not someone's files — pointing at one the shell just made
    /// (`mkdir out && amenbo export --out out`) is ordinary, so it is accepted. It is the one destination
    /// the export has to make way for: the staging directory is renamed onto the name, and a rename wants
    /// it free.
    #[test]
    fn exporting_into_an_existing_empty_directory_is_accepted() {
        let base = scratch("empty-dest");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice"]);

        let dest = base.join("out");
        std::fs::create_dir_all(&dest).unwrap();

        let report = export_bundle_from(
            &store_file(&a),
            &a.join(crate::blob::BLOBS_SUBDIR),
            &dest,
            &mut crate::progress::ignore,
        )
        .unwrap();
        assert_eq!(report.path, dest.display().to_string());
        let doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dest.join(BUNDLE_JSON_NAME)).unwrap()).unwrap();
        assert_eq!(task_titles(&doc), vec!["alice"], "the export landed under the name it was given");
    }

    /// An export must never fold into someone else's files.
    #[test]
    fn exporting_into_an_occupied_place_is_refused() {
        let base = scratch("occupied");
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        seed_store(&a, &["alice"]);

        let dest = base.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("mine.txt"), b"keep me").unwrap();

        let err = export_bundle_from(
            &store_file(&a),
            &a.join(crate::blob::BLOBS_SUBDIR),
            &dest,
            &mut crate::progress::ignore,
        )
        .unwrap_err();
        assert!(err.to_string().contains("exists"));
        assert!(dest.join("mine.txt").is_file(), "the refusal touched nothing");
    }

    fn export_one(db_path: &Path) -> serde_json::Value {
        let mut buf = Vec::new();
        export_json_from(db_path, &mut buf, &mut crate::progress::ignore).unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    fn task_titles(doc: &serde_json::Value) -> Vec<&str> {
        doc["tables"]["task"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap())
            .collect()
    }
}
