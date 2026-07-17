//! Content-addressed blob store — the byte-storage layer behind `blob`-mode attachments.
//!
//! The truth source (engine) carries only the attachment *metadata* (`blob_hash`/`filename`/`mime`/
//! `size_bytes`); the bytes never land there, because a truth source that grew with every attached file
//! would break local-first. The bytes instead live out-of-band in this content-addressed store under
//! `<store>/blobs/`, keyed by their **BLAKE3 digest** — the "fingerprint of the content", not a location.
//! That fingerprint is what gives us dedup, tamper detection, and cross-device identity for free.
//!
//! The layout is `<root>/<hash>`, where `<hash>` is the 64-char lowercase BLAKE3 hex digest of the bytes.
//! The on-disk layout *is* the bookkeeping: there is no separate index that could drift from the
//! filesystem. The read path also peers into a legacy `<root>/pinned/` ([`LEGACY_BLOB_SUBDIR`]) so a store
//! that still nests its bytes there — a source under backup, an old-layout archive being restored — is
//! still seen.
//!
//! Content addressing dedups for free: identical plaintext hashes to the same name, so a second ingest of
//! the same bytes is a no-op rather than a second copy. This is **local, plaintext** dedup only — no
//! cross-account dedup and no convergent encryption (which would leak "you have the same file as X"). The
//! cost is a little duplicated storage across accounts; the benefit is no leak.
//!
//! A blob's **refcount** is the number of live `blob`-mode attachments pointing at its hash; that is
//! derived from the attachment metadata (read-model), not stored here. [`BlobStore::gc`] is mark-and-
//! sweep: a present blob whose hash no attachment references (refcount 0 — the attachment was deleted) is
//! removed. Refcount is what removes a blob; nothing sheds one for capacity, because every blob is a local
//! original with no other copy to fall back on.
//!
//! Blobs are stored **plaintext**, content-addressed by `BLAKE3(bytes)`; on-device secrecy is delegated to
//! OS full-disk encryption (FileVault / BitLocker), matching the truth-source store. Ingest
//! ([`BlobStore::ingest_bytes`]/[`BlobStore::ingest_path`]) and read ([`BlobStore::read`]/
//! [`BlobStore::read_range`]) work directly on the bytes; the Range-streaming `amenboblob` viewer seeks
//! into the file via [`BlobStore::read_range`].

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Subdirectory, under a store's data dir, holding that store's content-addressed blobs
/// (`<store>/blobs/<hash>`). Shared with [`crate::store::Store::blobs`] and the whole-device
/// archive ([`crate::archive`]) so the layout is named in exactly one place.
pub const BLOBS_SUBDIR: &str = "blobs";

/// A nesting some stores still keep their blobs under (`<store>/blobs/pinned/<hash>`). The canonical
/// layout is the flat `<store>/blobs/<hash>`; this name is **read-only**, so a store that has not been
/// flattened yet — a source under backup, or an old-layout archive being restored — is still enumerated.
/// Nothing is ever written here again.
pub(crate) const LEGACY_BLOB_SUBDIR: &str = "pinned";

/// Staging dir for atomic writes (write to a unique temp file, then rename into place). Kept apart from
/// the hash-named files so the scans never see a half-written file.
const TMP_DIR: &str = "tmp";

/// A BLAKE3 hex digest is 32 bytes → 64 lowercase hex chars. Used to tell genuine blob files apart
/// from stray entries during a sweep so GC can never delete something it does not recognise.
const HASH_LEN: usize = 64;

/// How long a blob is spared from [`BlobStore::gc`] purely for being young.
///
/// An attach ingests the bytes **before** its attachment row commits (the row needs the hash), and
/// the CLI and the GUI write the same store from different processes. Between the two steps the
/// bytes are on disk with nothing referencing them yet — indistinguishable, to a sweep, from
/// garbage. A GC that ran right then would delete the file out from under an attach that then
/// commits a row pointing at nothing. The bytes are only reclaimed once they have been unreferenced
/// for longer than any in-flight attach could plausibly take; the cost of waiting is a stale blob
/// surviving until the next sweep, which is nothing.
pub const GC_MIN_AGE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// The content-address and byte length of a stored blob. Returned by ingest and the store listing;
/// both fields mirror the attachment metadata columns the caller persists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BlobRef {
    /// BLAKE3 hex digest — the content-address.
    pub hash: String,
    pub size_bytes: u64,
}

/// What a [`BlobStore::gc`] sweep removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GcReport {
    /// Number of unreferenced blobs deleted.
    pub removed: u64,
    /// Bytes reclaimed by those deletions.
    pub freed_bytes: u64,
}

// ── Capacity management ──────────────────────────────────────────────────────
//
// One guard, touching only what is safe to: a **per-file** size cap by file class — a loose
// destruction/runaway guard applied at ingest (an accidental multi-GB drop should bounce, not wedge
// the store), not a tight quota. Every blob a store holds is a local original whose durability backup
// carries, so there is nothing a capacity sweep may shed; refcount GC is the only removal.
//
// The defaults are deliberately loose, and config-overridable.

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

/// File-type class behind the per-file size cap, derived from the attachment MIME type. The
/// cap is per *class* (not per exact type) because its only job is a loose ceiling — images and
/// documents stay small, audio/video and unknown blobs get more room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileClass {
    Image,
    Audio,
    Video,
    Document,
    Other,
}

impl FileClass {
    /// Classify by MIME type. `None`/blank/unrecognised → [`FileClass::Other`].
    pub fn of_mime(mime: Option<&str>) -> FileClass {
        let m = mime.unwrap_or("").trim().to_ascii_lowercase();
        if m.starts_with("image/") {
            FileClass::Image
        } else if m.starts_with("audio/") {
            FileClass::Audio
        } else if m.starts_with("video/") {
            FileClass::Video
        } else if m.starts_with("text/") || m == "application/pdf" || m == "application/json" || m == "application/xml" {
            FileClass::Document
        } else {
            FileClass::Other
        }
    }

    /// Wire/display text used in capacity error messages.
    pub fn as_str(self) -> &'static str {
        match self {
            FileClass::Image => "image",
            FileClass::Audio => "audio",
            FileClass::Video => "video",
            FileClass::Document => "document",
            FileClass::Other => "other",
        }
    }
}

/// Guess an attachment's MIME type from its file-name extension — the metadata recorded at ingest,
/// consumed by the GUI viewer dispatch and the [`FileClass`] capacity bucket. A magic-byte
/// detector (`infer` etc.) is deliberately avoided: it adds a dependency yet can't tell
/// `text/markdown` from `text/csv` from `text/plain`, and the only consumers here are a coarse
/// bucket and a viewer hint, so an extension map suffices. Unknown extensions return `None` — the
/// caller leaves `mime` unset and the blob falls into [`FileClass::Other`].
pub fn mime_from_filename(name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    let m = match ext.as_str() {
        // images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "avif" => "image/avif",
        "heic" | "heif" => "image/heic",
        // audio
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        // video
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        // documents / text
        "pdf" => "application/pdf",
        "txt" | "text" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "json" => "application/json",
        "xml" => "application/xml",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "rs" | "py" | "js" | "ts" | "go" | "c" | "h" | "cpp" | "java" | "rb" | "sh" => "text/plain",
        // archives
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "tar" => "application/x-tar",
        _ => return None,
    };
    Some(m)
}

/// Per-file (by class) capacity limits. Every value is a byte count. Defaults are loose
/// (destruction/runaway guard, not a quota) and overridable via `amenbo config set attachment.*` — held on
/// [`crate::config::Config`] and threaded in by the caller, so the byte layer ([`BlobStore`]) stays
/// policy-free. A `config.json` written by an older build may carry keys this struct no longer has; serde
/// ignores the unknown field, so the key is left to rot rather than migrated out. Do **not** add
/// `deny_unknown_fields` here, which would make those stores unreadable for the sake of one dead key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityPolicy {
    /// Per-file cap for `image/*`.
    pub image_max: u64,
    /// Per-file cap for `audio/*`.
    pub audio_max: u64,
    /// Per-file cap for `video/*`.
    pub video_max: u64,
    /// Per-file cap for documents (`text/*`, PDF, JSON, XML).
    pub document_max: u64,
    /// Per-file cap for anything else.
    pub other_max: u64,
}

impl Default for CapacityPolicy {
    fn default() -> Self {
        // Loose ceilings: sized to bounce an accidental multi-GB drop, not to ration normal use.
        CapacityPolicy {
            image_max: 50 * MIB,
            audio_max: 200 * MIB,
            video_max: 2 * GIB,
            document_max: 100 * MIB,
            other_max: 200 * MIB,
        }
    }
}

impl CapacityPolicy {
    /// The per-file cap that applies to `class`.
    pub fn per_file_max(&self, class: FileClass) -> u64 {
        match class {
            FileClass::Image => self.image_max,
            FileClass::Audio => self.audio_max,
            FileClass::Video => self.video_max,
            FileClass::Document => self.document_max,
            FileClass::Other => self.other_max,
        }
    }

    /// Validate one file's size against the per-file cap for its MIME class. `Ok(())` if it
    /// fits; an `invalid_value` error naming the class and limit if it exceeds it. Apply this at
    /// ingest, before the bytes land in the store.
    pub fn check_per_file(&self, mime: Option<&str>, size: u64) -> Result<()> {
        let class = FileClass::of_mime(mime);
        let max = self.per_file_max(class);
        if size > max {
            return Err(Error::invalid(
                format!("attachment is {size} bytes, over the {} per-file limit of {max} bytes", class.as_str()),
                format!("添付が {size} バイトで、{} の per-file 上限 {max} バイトを超えています", class.as_str()),
            ));
        }
        Ok(())
    }
}

/// A content-addressed blob store rooted at one store's `blobs/` directory. Construct via
/// [`crate::store::Store::blobs`]; the bytes live alongside the engine under the store dir.
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    /// Root the store at `root` (`<store>/blobs`). Directories are created lazily on first write.
    pub fn at(root: PathBuf) -> BlobStore {
        BlobStore { root }
    }

    /// The directory hash-named blob files are written to and read from: the store's `blobs/` root. Every
    /// write lands here.
    fn dir(&self) -> PathBuf {
        self.root.clone()
    }

    /// The legacy `blobs/pinned/` nesting — **read-only**, present only on a store that has not been
    /// flattened yet ([`LEGACY_BLOB_SUBDIR`]).
    fn legacy_dir(&self) -> PathBuf {
        self.root.join(LEGACY_BLOB_SUBDIR)
    }

    fn loc(&self, hash: &str) -> PathBuf {
        self.dir().join(hash)
    }

    /// Whether these bytes are stored locally.
    pub fn has(&self, hash: &str) -> bool {
        self.path(hash).is_some()
    }

    /// Path to the stored bytes, or `None` if absent. The bytes are content-addressed, so callers
    /// may stream this path directly (the GUI viewer's stream protocol).
    pub fn path(&self, hash: &str) -> Option<PathBuf> {
        let flat = self.loc(hash);
        if flat.exists() {
            return Some(flat);
        }
        // A store that has not been flattened yet still keeps its bytes under `pinned/`.
        let legacy = self.legacy_dir().join(hash);
        legacy.exists().then_some(legacy)
    }

    /// Byte length of a present blob (from the filesystem), or `None` if absent.
    pub fn size(&self, hash: &str) -> Option<u64> {
        self.path(hash).and_then(|p| fs::metadata(p).ok()).map(|m| m.len())
    }

    /// Open a present blob for reading, or `NotFound` if these bytes are not stored locally.
    pub fn open(&self, hash: &str) -> Result<File> {
        match self.path(hash) {
            Some(p) => Ok(File::open(p)?),
            None => Err(Self::missing(hash)),
        }
    }

    /// Read a present blob fully into memory. Prefer [`Self::open`] for large blobs.
    pub fn read(&self, hash: &str) -> Result<Vec<u8>> {
        match self.path(hash) {
            Some(p) => Ok(fs::read(p)?),
            None => Err(Self::missing(hash)),
        }
    }

    /// Ingest bytes, returning their content-address. Dedups by content: a second ingest of the same
    /// bytes is a no-op.
    pub fn ingest_bytes(&self, bytes: &[u8]) -> Result<BlobRef> {
        let hash = blake3::hash(bytes).to_hex().to_string();
        let dest = self.loc(&hash);
        if !dest.exists() {
            self.write_atomic(&dest, bytes)?;
        }
        Ok(BlobRef { hash, size_bytes: bytes.len() as u64 })
    }

    /// Ingest a file, streaming the bytes through the hasher and into a temp file in one pass (no
    /// full read into memory — blobs can be large). Dedup follows [`Self::ingest_bytes`].
    pub fn ingest_path(&self, src: &Path) -> Result<BlobRef> {
        let tmp = self.new_tmp()?;
        let mut reader = File::open(src)?;
        let mut writer = File::create(&tmp)?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 64 * 1024];
        let mut size: u64 = 0;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            writer.write_all(&buf[..n])?;
            size += n as u64;
        }
        writer.sync_all()?;
        drop(writer);

        let hash = hasher.finalize().to_hex().to_string();
        let dest = self.loc(&hash);
        if dest.exists() {
            // Dedup: identical content already stored — discard the staged copy.
            let _ = fs::remove_file(&tmp);
        } else {
            ensure_parent(&dest)?;
            fs::rename(&tmp, &dest)?;
        }
        Ok(BlobRef { hash, size_bytes: size })
    }

    /// The byte length of a stored blob (its file size), or `None` if absent. Blobs are stored plaintext,
    /// so this is the plaintext length the Range viewer reports as the media total.
    pub fn plaintext_len(&self, hash: &str) -> Option<u64> {
        let path = self.path(hash)?;
        fs::metadata(&path).ok().map(|m| m.len())
    }

    /// Read the plaintext `[start, start+len)` window of a stored blob by seeking (blobs are plaintext at
    /// rest). This is what the Range-streaming viewer rides on so large media still seeks. `start` past the
    /// end yields empty; the range clamps to the end. `NotFound` if the bytes are absent.
    pub fn read_range(&self, hash: &str, start: u64, len: u64) -> Result<Vec<u8>> {
        use std::io::{Seek, SeekFrom};
        let path = self.path(hash).ok_or_else(|| Self::missing(hash))?;
        let mut f = File::open(&path)?;
        let total = f.metadata()?.len();
        let s = start.min(total);
        let e = start.saturating_add(len).min(total);
        if e <= s {
            return Ok(Vec::new());
        }
        f.seek(SeekFrom::Start(s))?;
        let mut buf = vec![0u8; (e - s) as usize];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Every blob physically present in this store, with its size. Scans the flat `blobs/` root and, for a
    /// store that has not been flattened yet, the legacy `blobs/pinned/`. Content-addressing makes a hash
    /// present in both identical, so the first (flat) sighting wins.
    pub fn list(&self) -> Result<Vec<BlobRef>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for dir in [self.dir(), self.legacy_dir()] {
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !is_hash(&name) || !seen.insert(name.clone()) {
                    continue;
                }
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push(BlobRef { hash: name, size_bytes: size });
            }
        }
        Ok(out)
    }

    /// Reclaim **one named blob** the caller has just established nothing references any more — the
    /// targeted twin of [`Self::gc`], for the delete path. A delete knows exactly which hashes it orphaned,
    /// so the bytes can go without walking the whole blob directory: reclaiming after `attach rm` costs one
    /// `stat` + one `unlink`, not a sweep whose price grows with every blob the store holds. `min_age`
    /// means the same thing it does in [`Self::gc`] and is enforced the same way: a blob younger than it —
    /// or one whose age the filesystem will not report — is kept, because it may be an attach in flight in
    /// another process that is about to reference these very bytes. What that spares here is not lost, only
    /// deferred: the sweep collects it later. Returns the bytes freed (`0` when the blob is absent, too
    /// young, or still referenced elsewhere — the caller must ask the read-model that last question;
    /// content-addressing means two attachments can share these bytes).
    pub fn reclaim(&self, hash: &str, min_age: std::time::Duration) -> Result<u64> {
        let Some(path) = self.path(hash) else { return Ok(0) };
        let Ok(meta) = fs::metadata(&path) else { return Ok(0) };
        let old_enough = meta
            .modified()
            .ok()
            .and_then(|written| std::time::SystemTime::now().duration_since(written).ok())
            .is_some_and(|age| age >= min_age);
        if !old_enough {
            return Ok(0);
        }
        let size = meta.len();
        fs::remove_file(&path)?;
        Ok(size)
    }

    /// Garbage-collect blobs no live attachment references (refcount 0). Mark-and-sweep: `referenced` is
    /// the set of hashes still pointed at by a live `blob` attachment (from the read-model, e.g.
    /// [`crate::store_engine::read::referenced_blob_hashes`]); any present blob outside it is removed.
    /// Stray non-hash files are left untouched. Returns how much was reclaimed. This is the catch-all: it
    /// collects what a targeted [`Self::reclaim`] could not (a blob still too young when its last reference
    /// went, bytes an interrupted delete left behind) and so stays the only thing that can promise a store
    /// holds no garbage. `min_age` spares a blob that was written less than that ago even when nothing
    /// references it — it may be an attach in flight in another process ([`GC_MIN_AGE`], which every caller
    /// in production passes). A blob whose age cannot be read is kept, so an unreadable timestamp can never
    /// cost data.
    pub fn gc(
        &self,
        referenced: &HashSet<String>,
        min_age: std::time::Duration,
    ) -> Result<GcReport> {
        let mut report = GcReport::default();
        let now = std::time::SystemTime::now();
        // Sweep the flat root and, for a store that has not been flattened, the legacy `pinned/` — so an
        // unreferenced blob is reclaimed wherever `Self::list` would have reported it from.
        for dir in [self.dir(), self.legacy_dir()] {
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !is_hash(&name) || referenced.contains(&name) {
                    continue;
                }
                let meta = entry.metadata()?;
                let age = meta
                    .modified()
                    .ok()
                    .and_then(|written| now.duration_since(written).ok());
                match age {
                    Some(age) if age >= min_age => {}
                    // Too young to sweep, or a filesystem that will not say — keep the bytes.
                    _ => continue,
                }
                let size = meta.len();
                fs::remove_file(entry.path())?;
                report.removed += 1;
                report.freed_bytes += size;
            }
        }
        Ok(report)
    }

    /// Atomically place `bytes` at `target`: write to a unique temp file (fsync), then rename. The
    /// rename is atomic, so a hash-named file is never observed half-written. Concurrent writers of
    /// the same content race to the same target with identical bytes — harmless.
    fn write_atomic(&self, target: &Path, bytes: &[u8]) -> Result<()> {
        let tmp = self.new_tmp()?;
        {
            let mut f = File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        ensure_parent(target)?;
        fs::rename(&tmp, target)?;
        Ok(())
    }

    /// A fresh, unique path in the staging dir (`<root>/tmp/<random>`).
    fn new_tmp(&self) -> Result<PathBuf> {
        let tmp_dir = self.root.join(TMP_DIR);
        fs::create_dir_all(&tmp_dir)?;
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).expect("failed to draw OS randomness");
        let name: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
        Ok(tmp_dir.join(name))
    }

    fn missing(hash: &str) -> Error {
        Error::not_found(
            format!("blob {hash} is not stored locally"),
            format!("blob {hash} はローカルに存在しません"),
        )
    }
}

/// Ensure the parent directory of `path` exists.
fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Whether `name` looks like a BLAKE3 hex digest (64 lowercase hex chars) — the guard that keeps a
/// sweep from touching anything that is not a genuine blob file, and that keeps a whole-device restore
/// ([`crate::archive`]) from writing an archive entry to any name but a content-address.
pub(crate) fn is_hash(name: &str) -> bool {
    name.len() == HASH_LEN && name.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn store() -> (BlobStore, tempdir::Holder) {
        let dir = tempdir::Holder::new();
        (BlobStore::at(dir.path().join("blobs")), dir)
    }

    #[test]
    fn ingest_is_content_addressed_and_stable() {
        let (bs, _d) = store();
        let a = bs.ingest_bytes(b"hello world").unwrap();
        let b = bs.ingest_bytes(b"hello world").unwrap();
        // Same content → same hash, and the digest is BLAKE3 of the bytes.
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.hash, blake3::hash(b"hello world").to_hex().to_string());
        assert_eq!(a.size_bytes, 11);
        assert!(bs.has(&a.hash));
        assert_eq!(bs.read(&a.hash).unwrap(), b"hello world");
    }

    #[test]
    fn ingest_writes_flat_not_under_pinned() {
        let (bs, _d) = store();
        let r = bs.ingest_bytes(b"flat").unwrap();
        // The byte lands directly under `blobs/`, with no `pinned/` level.
        assert!(bs.root.join(&r.hash).is_file());
        assert!(!bs.root.join(LEGACY_BLOB_SUBDIR).exists());
    }

    #[test]
    fn reads_a_store_not_yet_flattened_from_pinned() {
        let (bs, _d) = store();
        // A store of the old layout: the byte sits under the legacy `pinned/` nesting only.
        let hash = blake3::hash(b"legacy").to_hex().to_string();
        let legacy = bs.root.join(LEGACY_BLOB_SUBDIR);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join(&hash), b"legacy").unwrap();
        // The read path still finds it, and `list` reports it.
        assert!(bs.has(&hash));
        assert_eq!(bs.read(&hash).unwrap(), b"legacy");
        let listed = bs.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].hash, hash);
    }

    #[test]
    fn a_hash_in_both_layouts_is_listed_once() {
        let (bs, _d) = store();
        let r = bs.ingest_bytes(b"both").unwrap();
        // A half-migrated store could momentarily hold the byte flat *and* under `pinned/`; content-
        // addressing makes them identical, so `list` must not double-count.
        let legacy = bs.root.join(LEGACY_BLOB_SUBDIR);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join(&r.hash), b"both").unwrap();
        let listed = bs.list().unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn dedup_keeps_one_copy() {
        let (bs, _d) = store();
        let r = bs.ingest_bytes(b"dup").unwrap();
        bs.ingest_bytes(b"dup").unwrap();
        // Identical plaintext → one file, not two.
        let listed: Vec<_> = bs.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].hash, r.hash);
    }

    #[test]
    fn ingest_path_streams_same_hash_as_bytes() {
        let (bs, d) = store();
        let src = d.path().join("payload.bin");
        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 256) as u8).collect();
        fs::write(&src, &bytes).unwrap();
        let from_path = bs.ingest_path(&src).unwrap();
        assert_eq!(from_path.hash, blake3::hash(&bytes).to_hex().to_string());
        assert_eq!(from_path.size_bytes, bytes.len() as u64);
        assert_eq!(bs.read(&from_path.hash).unwrap(), bytes);
    }

    #[test]
    fn gc_removes_unreferenced_keeps_referenced() {
        let (bs, _d) = store();
        let keep = bs.ingest_bytes(b"keep").unwrap();
        let dropped = bs.ingest_bytes(b"drop me").unwrap();

        let referenced: HashSet<String> = [keep.hash.clone()].into_iter().collect();
        let report = bs.gc(&referenced, Duration::ZERO).unwrap();
        assert_eq!(report.removed, 1);
        assert_eq!(report.freed_bytes, dropped.size_bytes);
        assert!(bs.has(&keep.hash));
        assert!(!bs.has(&dropped.hash));
    }

    #[test]
    fn gc_leaves_stray_non_hash_files_alone() {
        let (bs, _d) = store();
        let r = bs.ingest_bytes(b"x").unwrap();
        let stray = bs.dir().join("README");
        fs::write(&stray, b"not a blob").unwrap();
        bs.gc(&HashSet::new(), Duration::ZERO).unwrap();
        // The hash-named blob is gone (unreferenced) but the stray file is untouched.
        assert!(!bs.has(&r.hash));
        assert!(stray.exists());
    }

    /// An unreferenced blob younger than `min_age` is an attach that may still be in flight in
    /// another process (the bytes land before the row commits), so the sweep leaves it alone.
    #[test]
    fn gc_spares_a_blob_too_young_to_judge() {
        let (bs, _d) = store();
        let fresh = bs.ingest_bytes(b"just written").unwrap();

        let report = bs.gc(&HashSet::new(), Duration::from_secs(60 * 60)).unwrap();

        assert_eq!(report.removed, 0);
        assert!(bs.has(&fresh.hash), "young unreferenced blob survives the sweep");
    }

    #[test]
    fn missing_blob_reads_as_not_found() {
        let (bs, _d) = store();
        let err = bs.read("deadbeef").unwrap_err();
        assert_eq!(err.code(), "not_found");
    }

    // ── Capacity management ──────────────────────────────────────────────────

    #[test]
    fn file_class_is_derived_from_mime() {
        assert_eq!(FileClass::of_mime(Some("image/png")), FileClass::Image);
        assert_eq!(FileClass::of_mime(Some("audio/mpeg")), FileClass::Audio);
        assert_eq!(FileClass::of_mime(Some("video/mp4")), FileClass::Video);
        assert_eq!(FileClass::of_mime(Some("text/markdown")), FileClass::Document);
        assert_eq!(FileClass::of_mime(Some("application/pdf")), FileClass::Document);
        assert_eq!(FileClass::of_mime(Some("application/zip")), FileClass::Other);
        // None/blank/garbage all fall to Other.
        assert_eq!(FileClass::of_mime(None), FileClass::Other);
        assert_eq!(FileClass::of_mime(Some("  ")), FileClass::Other);
    }

    #[test]
    fn mime_guessed_from_extension() {
        assert_eq!(mime_from_filename("a/b/photo.PNG"), Some("image/png"));
        assert_eq!(mime_from_filename("notes.md"), Some("text/markdown"));
        assert_eq!(mime_from_filename("report.pdf"), Some("application/pdf"));
        assert_eq!(mime_from_filename("clip.mp4"), Some("video/mp4"));
        // No/unknown extension → None (caller leaves mime unset → FileClass::Other).
        assert_eq!(mime_from_filename("Makefile"), None);
        assert_eq!(mime_from_filename("data.unknownext"), None);
        // The guessed mime feeds the capacity bucket consistently.
        assert_eq!(FileClass::of_mime(mime_from_filename("clip.mov")), FileClass::Video);
    }

    #[test]
    fn per_file_cap_is_loose_by_class_and_rejects_oversize() {
        let p = CapacityPolicy::default();
        // A small image fits; one byte over its class cap is rejected with invalid_value.
        assert!(p.check_per_file(Some("image/png"), 10).is_ok());
        let err = p.check_per_file(Some("image/png"), p.image_max + 1).unwrap_err();
        assert_eq!(err.code(), "invalid_value");
        // The cap is per class: a payload over the image cap still fits under the (larger) video cap.
        assert!(p.check_per_file(Some("video/mp4"), p.image_max + 1).is_ok());
        // Unknown/None classifies as `other` and uses its cap.
        assert!(p.check_per_file(None, p.other_max).is_ok());
        assert!(p.check_per_file(None, p.other_max + 1).is_err());
    }

    // ── Plaintext blobs ──────────────────────────────────────────────────────

    #[test]
    fn plaintext_ingest_read_addresses_the_bytes() {
        let (bs, _d) = store();
        let body = b"the attachment body";
        let r = bs.ingest_bytes(body).unwrap();
        // Blobs are plaintext at rest: the content-address is BLAKE3 of the bytes themselves, and
        // read/size return them verbatim.
        assert_eq!(r.hash, blake3::hash(body).to_hex().to_string());
        assert_eq!(r.size_bytes, body.len() as u64);
        assert_eq!(bs.size(&r.hash).unwrap(), body.len() as u64);
        assert_eq!(bs.read(&r.hash).unwrap(), body);
        assert_eq!(bs.plaintext_len(&r.hash), Some(body.len() as u64));
    }

    #[test]
    fn read_range_windows_and_clamps() {
        let (bs, _d) = store();
        // Multi-chunk blob: a range across a 64 KiB boundary returns the matching slice; ranges clamp
        // at the end and a start past the end yields empty.
        let body: Vec<u8> = (0..(64 * 1024 * 2 + 500)).map(|i| (i * 13 + 5) as u8).collect();
        let r = bs.ingest_bytes(&body).unwrap();
        let (s, l) = (64 * 1024u64 - 40, 200u64);
        assert_eq!(bs.read_range(&r.hash, s, l).unwrap(), body[s as usize..(s + l) as usize]);
        assert_eq!(bs.read_range(&r.hash, 0, body.len() as u64).unwrap(), body);
        // Tail clamps; past-the-end start is empty.
        assert_eq!(bs.read_range(&r.hash, body.len() as u64 - 5, 999).unwrap(), body[body.len() - 5..]);
        assert_eq!(bs.read_range(&r.hash, body.len() as u64, 10).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn read_range_missing_blob_is_not_found() {
        let (bs, _d) = store();
        let err = bs.read_range("deadbeef", 0, 10).unwrap_err();
        assert_eq!(err.code(), "not_found");
    }

    /// Minimal self-cleaning temp dir (no extra dev-dependency): a unique dir under the OS temp,
    /// removed on drop.
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub struct Holder(PathBuf);

        impl Holder {
            pub fn new() -> Holder {
                let mut nonce = [0u8; 16];
                getrandom::fill(&mut nonce).expect("failed to draw OS randomness");
                let name: String =
                    std::iter::once("amenbo-blob-test-".to_string()).chain(nonce.iter().map(|b| format!("{b:02x}"))).collect();
                let dir = std::env::temp_dir().join(name);
                std::fs::create_dir_all(&dir).unwrap();
                Holder(dir)
            }

            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Holder {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
