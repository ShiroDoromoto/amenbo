//! The activity ledger — the file the timeline's **system events** live in.
//!
//! Activity is a bounded viewing stream, not a system of record: every permanent fact already lives in a
//! first-class column, so the events that narrate *how* a task got there may age out. They live outside the
//! database, in one append-only JSONL file beside it, capped at [`MAX_BYTES`] and self-compacting.
//!
//! - **One file for the store** — `<base>/activity.jsonl` ([`crate::config::Paths::activity_file`]). Each
//!   line carries its own `project` (the file cannot join against the DB).
//! - **One line per event**, JSON, LF-terminated, at most [`MAX_LINE_BYTES`]; an oversized `event` payload
//!   is dropped down to its `kind` with `"truncated": true` rather than being allowed to grow the line.
//! - **Written after the commit** succeeds ([`crate::store::Store::add_system_event`]). Crash before the
//!   commit and no line appears; crash after it and the line is lost. It never duplicates, and it falls to
//!   the side of losing a line — that asymmetry is the whole point of not being a system of record.
//! - **Every system event lives only here** ([`event`]) — no table carries a copy.
//! - **Concurrency without a lock, for the append.** `O_APPEND` (POSIX) / `FILE_APPEND_DATA` (Windows)
//!   makes a *single* `write` syscall atomic in its offset-and-write, so concurrent writers never
//!   interleave their lines. This is why the line is capped, why the handle is opened and closed per line,
//!   and why this module calls [`std::io::Write::write`] and never `write_all` — the latter loops on a
//!   partial write, and another process's line can land in the middle of the two halves.
//! - **Concurrency with a lock, for the compaction.** Trimming the file to its newer half is a
//!   read-modify-write, which atomic appends cannot protect. It takes an exclusive `activity.jsonl.lock`
//!   with `try_lock` and simply **skips** when another writer holds it — overshooting the cap for a moment
//!   is harmless, and blocking a mutation on a log rotation would not be.
//!
//! Failure is never fatal here: a full disk or a read-only directory produces a `warn` and the mutation
//! carries on.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::model::ActorKind;
use crate::time::Timestamp;

/// File name of the ledger, kept beside the truth source in the base directory.
pub const FILE_NAME: &str = "activity.jsonl";

/// File name of the sidecar that serialises compaction (never taken for an append).
pub const LOCK_NAME: &str = "activity.jsonl.lock";

/// Schema version stamped on every line this build writes. A line of any other version is skipped by the
/// reader.
pub const LINE_VERSION: i64 = 2;

/// Cap on one line, payload included. Above this the OS no longer guarantees the append is one atomic
/// `write`, so an event that would exceed it is truncated rather than risking an interleaved line.
pub const MAX_LINE_BYTES: usize = 16 * 1024;

/// Cap on the whole file. At the ~200 B a line runs to, that is months of the busiest use — old enough
/// that losing the tail costs nothing.
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// One system event, as it goes to the ledger. `project` / `task` / `decision` are row keys, and each is
/// optional because an event may outlive the row it names (a deletion), belong to an unfiled task, or name
/// no task at all (a project or decision event). The subject keys are **flat, one per entity kind**, rather
/// than a `("kind", id)` pair, so a line can name several at once — a decision deleted out of a project
/// names both — and so a reader can filter on one key without parsing the payload. A new key is additive
/// and keeps the line at its version: an older line simply does not carry it and reads back as `None`.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Activity sequence id, minted in the DB (the ledger and `task_comment` share the counter) so the two
    /// halves of the timeline can be merged on one total order.
    pub id: i64,
    pub at: Timestamp,
    /// The facet that caused the event. `None` = unknown.
    pub actor: Option<ActorKind>,
    pub project: Option<i64>,
    pub task: Option<i64>,
    pub decision: Option<i64>,
    /// The event payload (`{"kind": "task.status_changed", …}` — [`event`]).
    pub event: Value,
}

/// The payloads of every system event — the whole vocabulary the timeline narrates with. An event is a
/// ledger line and nothing else, so one module holds every payload that can become one. **Deletions: task,
/// project and decision, and no others.** Deleting a comment leaves no trace at all, deliberately — a
/// mis-posted comment vanishes rather than leaving a record of the retraction — and neither does removing
/// an attachment, a dimension or one of its values.
pub mod event {
    use serde_json::{json, Value};

    /// A task was created.
    pub fn task_created(title: &str) -> Value {
        json!({ "kind": "task.created", "title": title })
    }

    /// A task's status moved.
    pub fn task_status_changed(old: &str, new: &str) -> Value {
        json!({ "kind": "task.status_changed", "field": "status", "old": old, "new": new })
    }

    /// A task was assigned to a facet (`Some`), or unassigned (`None`).
    pub fn task_assigned(to_kind: Option<&str>) -> Value {
        json!({ "kind": "task.assigned", "to_kind": to_kind })
    }

    /// A task was re-homed to another project (`None` = out of every project).
    pub fn task_moved(project: Option<&str>) -> Value {
        json!({ "kind": "task.moved", "project": project })
    }

    /// A task's blocker `by` reached done, so the task is ready to be picked up.
    pub fn task_unblocked(by: &str) -> Value {
        json!({ "kind": "task.unblocked", "by": by })
    }

    /// A task is gone. The line still carries its `task` id and project, so the timeline can place it.
    pub fn task_deleted(title: Option<&str>) -> Value {
        json!({ "kind": "task.deleted", "title": title })
    }

    /// A project is gone, and with it the subtree it took down (`ops::project::delete`). The counts say
    /// how much went with it — the tasks and decisions themselves get no line of their own, because a
    /// cascade of thousands would say nothing the one line does not.
    pub fn project_deleted(name: Option<&str>, tasks: usize, decisions: usize) -> Value {
        json!({ "kind": "project.deleted", "name": name, "tasks": tasks, "decisions": decisions })
    }

    /// A decision was put up for discussion — the moment `proposed` began.
    ///
    /// The column says a decision *is* proposed; it cannot say who put it up or which pane they were
    /// in, and `status_changed_at` is overwritten by the verdict. That is the gap this line fills:
    /// a proposal nobody ever settled is only findable if something recorded that it was made
    /// (`AMB-T-3600`, `AMB-T-3639`).
    pub fn decision_proposed(title: &str) -> Value {
        json!({ "kind": "decision.proposed", "title": title })
    }

    /// A decision is gone.
    pub fn decision_deleted(title: Option<&str>) -> Value {
        json!({ "kind": "decision.deleted", "title": title })
    }
}

impl Entry {
    /// The line as it is written: one JSON object plus its LF. Returns `None` when even the truncated form
    /// does not fit in [`MAX_LINE_BYTES`], which cannot happen for the events this build emits.
    fn to_line(&self) -> Option<Vec<u8>> {
        let line = encode(self, &self.event);
        if line.len() <= MAX_LINE_BYTES {
            return Some(line);
        }
        let kind = self.event.get("kind").and_then(Value::as_str).unwrap_or("unknown");
        let line = encode(self, &json!({ "kind": kind, "truncated": true }));
        (line.len() <= MAX_LINE_BYTES).then_some(line)
    }
}

fn encode(entry: &Entry, event: &Value) -> Vec<u8> {
    let obj = json!({
        "v": LINE_VERSION,
        "id": entry.id,
        "at": entry.at.to_rfc3339_z(),
        "actor": entry.actor.map(|a| a.as_str()),
        "project": entry.project,
        "task": entry.task,
        "decision": entry.decision,
        "event": event,
    });
    let mut line = serde_json::to_vec(&obj).unwrap_or_default();
    line.push(b'\n');
    line
}

/// One line read back from the ledger — an [`Entry`] that survived the round trip. The reader is
/// **tolerant by contract**: a line this build cannot make sense of is skipped, never an error. The ledger
/// is written by whatever build happened to be installed, trimmed on a line boundary by whichever writer
/// crossed the cap, and (in theory) can end in the debris of a short write — so an unknown `v`, an unknown
/// `event.kind`, a missing field or a torn line must cost the reader nothing more than that line. Failing
/// the read instead would take the whole timeline down with one bad byte.
#[derive(Clone, Debug)]
pub struct Line {
    pub id: i64,
    pub at: Timestamp,
    pub actor: Option<ActorKind>,
    pub project: Option<i64>,
    pub task: Option<i64>,
    pub decision: Option<i64>,
    pub event: Value,
}

/// Every line of the ledger, in the order they were written. A missing file is an empty ledger (a store
/// that has not caused a system event yet), not a failure. **The whole file, in memory** — only for the
/// handful of callers that genuinely rewrite it, where the file *is* the subject and the pass happens once,
/// offline. A reader answering a user's request must not use it: it pays O(ledger) time and memory to show
/// a window of a few rows, which is exactly the design a growing file breaks. Read backwards instead
/// ([`rev_lines`]).
pub fn read(path: &Path) -> Vec<Line> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    String::from_utf8_lossy(&bytes).lines().filter_map(parse_line).collect()
}

/// How much of the file one backward read pulls in. Big enough that a window of a few dozen lines
/// (~200 B each) is answered by one read, small enough that it is never the file.
const REV_CHUNK: usize = 64 * 1024;

/// The ledger's lines **newest first**, read backwards from the end in [`REV_CHUNK`] blocks.
///
/// This is what makes reading the timeline cost the *window* instead of the *file*. The reader stops
/// pulling as soon as it has what it was asked for — the newest N rows, or everything past a cursor — so a
/// ledger that has grown for years costs the same as a fresh one to show today's activity. Nothing is
/// indexed and nothing is cached: the ledger is append-only, so its end is the newest line by construction,
/// and walking back from there is the index.
///
/// It is as tolerant as [`read`]: a line this build cannot parse is skipped, never an error. A file that is
/// appended to while this iterator walks it is unaffected — it only ever reads the bytes that were already
/// there when it opened.
pub fn rev_lines(path: &Path) -> RevLines {
    let file = File::open(path).ok();
    let end = file.as_ref().and_then(|f| f.metadata().ok()).map(|m| m.len()).unwrap_or(0);
    RevLines { file, pos: end, buf: Vec::new(), read_bytes: 0 }
}

/// The iterator [`rev_lines`] returns. `read_bytes` is what a bounded caller cuts its scan on.
pub struct RevLines {
    file: Option<File>,
    /// The file is unread below this offset; everything above it is either in `buf` or already yielded.
    pos: u64,
    /// Bytes read but not yet yielded, from `pos` upward. Its head may be half a line — the rest of that
    /// line is still in the file.
    buf: Vec<u8>,
    read_bytes: u64,
}

impl RevLines {
    /// How many bytes of the ledger this iterator has pulled in so far. A caller that cannot bound its scan
    /// by *what it is looking for* (a name that may not be in the file at all) bounds it by this.
    pub fn read_bytes(&self) -> u64 {
        self.read_bytes
    }

    /// Pull the previous block into the head of `buf`. `false` when the file is exhausted.
    fn fill(&mut self) -> bool {
        use std::io::{Read, Seek, SeekFrom};
        let Some(file) = self.file.as_mut() else { return false };
        if self.pos == 0 {
            return false;
        }
        let want = REV_CHUNK.min(self.pos as usize);
        let start = self.pos - want as u64;
        let mut block = vec![0u8; want];
        if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut block).is_err() {
            self.file = None; // a truncated or unreadable file is an ended ledger, not a failure
            return false;
        }
        self.pos = start;
        self.read_bytes += want as u64;
        block.append(&mut self.buf);
        self.buf = block;
        true
    }
}

impl Iterator for RevLines {
    type Item = Line;

    fn next(&mut self) -> Option<Line> {
        loop {
            match self.buf.iter().rposition(|b| *b == b'\n') {
                // The bytes after the last newline are a whole line (the file's last line, or one already
                // cut from the tail of a block). An empty run is the trailing newline — nothing to yield.
                Some(nl) => {
                    let line = self.buf.split_off(nl + 1);
                    self.buf.pop(); // the newline itself
                    if let Some(l) = parse_line(&String::from_utf8_lossy(&line)) {
                        return Some(l);
                    }
                }
                // No newline left: the whole buffer is the head of a line whose start is further back — or,
                // once the file is exhausted, the first line of the ledger.
                None => {
                    if !self.fill() {
                        if self.buf.is_empty() {
                            return None;
                        }
                        let line = std::mem::take(&mut self.buf);
                        if let Some(l) = parse_line(&String::from_utf8_lossy(&line)) {
                            return Some(l);
                        }
                        return None;
                    }
                }
            }
        }
    }
}

fn parse_line(line: &str) -> Option<Line> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("v").and_then(Value::as_i64) != Some(LINE_VERSION) {
        return None; // a version this build does not know how to read
    }
    let id = v.get("id")?.as_i64()?;
    let at = Timestamp::parse_rfc3339(v.get("at")?.as_str()?)?;
    let key = |k: &str| v.get(k).and_then(Value::as_i64);
    Some(Line {
        id,
        at,
        actor: v.get("actor").and_then(Value::as_str).and_then(ActorKind::parse),
        project: key("project"),
        task: key("task"),
        decision: key("decision"),
        event: v.get("event")?.clone(),
    })
}

/// Replace the whole ledger with `entries`, oldest first, trimmed to [`MAX_BYTES`] — the one write that
/// is **not** an append.
///
/// A write that carries **older** events into a file that already has newer ones cannot append: the
/// compaction that trims "the newer half" trims by *position*, so appending old events would make it
/// throw away the new ones. It takes the compaction lock for the same reason compaction does — this is a
/// read-modify-write, which the atomic append cannot protect — and gives up (`false`) if another writer
/// holds it, so the caller can leave the source in place and try again on the next open.
pub fn rewrite(path: &Path, entries: &[Entry]) -> bool {
    let Some(_lock) = try_lock(&lock_path(path)) else { return false };
    let mut bytes: Vec<u8> = Vec::new();
    for e in entries {
        match e.to_line() {
            Some(line) => bytes.extend(line),
            None => tracing::warn!(id = e.id, "activity ledger: event does not fit in a line; dropped"),
        }
    }
    if bytes.len() as u64 > MAX_BYTES {
        // Keep the newest whole lines that fit — the same asymmetry the cap has everywhere else: old
        // system events are the ones allowed to go.
        let cut = bytes.len() - MAX_BYTES as usize;
        let keep = match bytes[cut..].iter().position(|b| *b == b'\n') {
            Some(i) => cut + i + 1,
            None => bytes.len(),
        };
        bytes.drain(0..keep);
    }
    let tmp = path.with_extension("jsonl.tmp");
    if let Err(e) = std::fs::write(&tmp, &bytes).and_then(|()| std::fs::rename(&tmp, path)) {
        tracing::warn!(error = %e, "activity ledger: could not rewrite the file");
        return false;
    }
    true
}

/// The compaction lock beside `path`.
fn lock_path(path: &Path) -> PathBuf {
    path.with_file_name(LOCK_NAME)
}

/// Append one event to the ledger at `path`, then compact if the file outgrew [`MAX_BYTES`].
/// **Infallible by contract**: the ledger is not the truth source, so a failure to record an event warns
/// and lets the mutation stand. Call it *after* the mutation's transaction has committed.
pub fn append(path: &Path, entry: &Entry) {
    let Some(line) = entry.to_line() else {
        tracing::warn!(id = entry.id, "activity ledger: event does not fit in a line; dropped");
        return;
    };
    match write_line(path, &line) {
        Ok(size) if size > MAX_BYTES => compact(path),
        Ok(_) => {}
        Err(e) => tracing::warn!(id = entry.id, error = %e, "activity ledger: append failed; event dropped"),
    }
}

/// Append `line` with one atomic `write` and return the file's size afterwards. The handle is opened and
/// closed here — nothing holds it across calls, so Windows can still replace the file under a compaction.
fn write_line(path: &Path, line: &[u8]) -> std::io::Result<u64> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    // Deliberately `write`, not `write_all`: a short write must not be retried, because the retry would be
    // a second syscall and another process's line could land between the halves.
    let written = file.write(line)?;
    if written < line.len() {
        tracing::warn!(written, len = line.len(), "activity ledger: short write; line left truncated");
    }
    file.metadata().map(|m| m.len())
}

/// Trim the ledger to its newer half, on a line boundary, by atomic rename. Skipped — not queued — when
/// another writer is already compacting, or when the file shrank back under the cap in the meantime.
fn compact(path: &Path) {
    let Some(_lock) = try_lock(&lock_path(path)) else { return };
    if let Err(e) = compact_locked(path) {
        tracing::warn!(error = %e, "activity ledger: compaction failed; the file keeps growing");
    }
}

fn compact_locked(path: &Path) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    if bytes.len() as u64 <= MAX_BYTES {
        return Ok(());
    }
    let keep = match bytes[bytes.len() / 2..].iter().position(|b| *b == b'\n') {
        // Everything after the first line break in the newer half — a whole number of lines.
        Some(i) => &bytes[bytes.len() / 2 + i + 1..],
        // No line break in the newer half: one line is longer than the cap allows, so start over empty
        // rather than keep a half line.
        None => &[][..],
    };
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, keep)?;
    std::fs::rename(&tmp, path)
}

/// The compaction lock, held for the length of a rotation.
///
/// It releases **by asking**, not by closing the fd: a close's release lands microseconds late, and a
/// writer that arrives inside that window skips a rotation it could have performed. Same reasoning, and
/// the same measured lag, as [`crate::swap_lock::SwapGuard`].
struct CompactionLock {
    file: File,
}

impl Drop for CompactionLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Take the compaction lock, or `None` when someone else holds it (or the sidecar cannot be made).
fn try_lock(path: &Path) -> Option<CompactionLock> {
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).ok()?;
    match file.try_lock() {
        Ok(()) => Some(CompactionLock { file }),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Error(e)) => {
            tracing::warn!(error = %e, "activity ledger: cannot take the compaction lock");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let dir = amenbo_scratch::scratch(&format!("activity-{tag}"));
        dir
    }

    fn entry(id: i64, event: Value) -> Entry {
        Entry { id, at: Timestamp::now(), actor: Some(ActorKind::Ai), project: Some(7), task: Some(990), decision: None, event }
    }

    fn lines(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).expect("every line is a whole JSON object"))
            .collect()
    }

    #[test]
    fn an_appended_line_carries_the_v2_schema() {
        let dir = dir("schema");
        let path = dir.join(FILE_NAME);

        append(&path, &entry(12, json!({ "kind": "task.created", "title": "Alice" })));

        let lines = lines(&path);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["v"], json!(2));
        assert_eq!(lines[0]["id"], json!(12));
        assert_eq!(lines[0]["actor"], json!("ai"));
        assert_eq!(lines[0]["project"], json!(7));
        assert_eq!(lines[0]["task"], json!(990));
        assert_eq!(lines[0]["event"]["kind"], json!("task.created"));
        assert!(lines[0]["at"].as_str().unwrap().ends_with('Z'));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The backward walk hands back the newest lines first, and a caller that wants a window stops long
    /// before the file does.
    #[test]
    fn the_ledger_is_read_backwards_from_its_end() {
        let dir = dir("rev");
        let path = dir.join(FILE_NAME);
        for id in 1..=5 {
            append(&path, &entry(id, json!({ "kind": "task.moved", "project": "p" })));
        }

        let ids: Vec<i64> = rev_lines(&path).map(|l| l.id).collect();
        assert_eq!(ids, vec![5, 4, 3, 2, 1], "newest first");
        assert_eq!(rev_lines(&path).take(2).map(|l| l.id).collect::<Vec<_>>(), vec![5, 4], "and it can stop");

        // A missing ledger is an empty one, never a failure.
        assert_eq!(rev_lines(&dir.join("nothing.jsonl")).count(), 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Reading a window costs the window, not the history it sits on — the property that keeps a ledger
    /// which has grown for years as cheap to show as a fresh one. Proven structurally, by what the
    /// reader pulls off the disk, not by a stopwatch.
    #[test]
    fn a_window_costs_the_window_not_the_whole_ledger() {
        let dir = dir("bounded");
        let path = dir.join(FILE_NAME);
        // Written in one go rather than 4,000 appends: the subject here is the reader, and each append is a
        // separate open/write/close (see the module doc).
        let mut file = std::fs::File::create(&path).unwrap();
        for id in 1..=4000 {
            let e = entry(id, json!({ "kind": "task.created", "title": "山田さんに連絡する" }));
            file.write_all(&encode(&e, &e.event)).unwrap();
        }
        drop(file);
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size > 8 * REV_CHUNK as u64, "a ledger big enough for the question to mean something");

        let mut lines = rev_lines(&path);
        let window: Vec<i64> = lines.by_ref().take(20).map(|l| l.id).collect();
        assert_eq!(window.first(), Some(&4000), "the newest line, without reading to it");
        assert_eq!(window.len(), 20);
        assert!(
            lines.read_bytes() <= REV_CHUNK as u64,
            "one window pulled {} bytes off a {size}-byte ledger",
            lines.read_bytes()
        );

        // And a line that straddles a block boundary still comes back whole.
        assert_eq!(rev_lines(&path).count(), 4000, "every line, and no line torn by the chunking");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Reading is tolerant by contract, backwards as much as forwards: debris — a torn tail, a line
    /// from a version this build does not know — costs that line and nothing more.
    #[test]
    fn the_backward_walk_skips_what_it_cannot_read() {
        let dir = dir("rev-junk");
        let path = dir.join(FILE_NAME);
        append(&path, &entry(1, json!({ "kind": "task.created", "title": "Alice" })));
        {
            use std::io::Write as _;
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            f.write_all(b"{\"v\":1,\"id\":2}\n").unwrap(); // a version this build cannot read
            f.write_all(b"{\"v\":2,\"id\":3,\"at\":\"nope\"}\n").unwrap(); // unparseable
            f.write_all(b"{\"v\":2,\"id\":4,\"at\"").unwrap(); // a short write, no newline
        }

        assert_eq!(rev_lines(&path).map(|l| l.id).collect::<Vec<_>>(), vec![1]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn appends_accumulate_in_order() {
        let dir = dir("order");
        let path = dir.join(FILE_NAME);

        for id in 1..=5 {
            append(&path, &entry(id, json!({ "kind": "task.moved", "project": "p" })));
        }

        let ids: Vec<i64> = lines(&path).iter().map(|l| l["id"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_oversized_event_is_truncated_to_its_kind() {
        let dir = dir("oversize");
        let path = dir.join(FILE_NAME);

        let huge = "x".repeat(MAX_LINE_BYTES * 2);
        append(&path, &entry(3, json!({ "kind": "task.created", "title": huge })));

        let lines = lines(&path);
        assert_eq!(lines[0]["event"], json!({ "kind": "task.created", "truncated": true }));
        assert!(std::fs::metadata(&path).unwrap().len() <= MAX_LINE_BYTES as u64);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Write an over-cap ledger in one go — 2,000 whole lines of ~4 KiB, the shape an append loop would
    /// have taken hours of syscalls to reach.
    fn oversized_ledger(path: &Path) -> i64 {
        let filler = "y".repeat(4 * 1024);
        let mut bytes = Vec::with_capacity(MAX_BYTES as usize + 4 * 1024);
        let mut id = 0;
        while (bytes.len() as u64) <= MAX_BYTES {
            id += 1;
            bytes.extend(entry(id, json!({ "kind": "task.created", "title": filler })).to_line().unwrap());
        }
        std::fs::write(path, &bytes).unwrap();
        id
    }

    #[test]
    fn outgrowing_the_cap_keeps_the_newer_half_on_a_line_boundary() {
        let dir = dir("compact");
        let path = dir.join(FILE_NAME);
        let newest = oversized_ledger(&path);
        let before = std::fs::metadata(&path).unwrap().len();

        compact(&path);

        let after = std::fs::metadata(&path).unwrap().len();
        assert!(after < before && after <= MAX_BYTES, "compaction trimmed the file: {before} → {after}");
        assert!(after > MAX_BYTES / 4, "it keeps the newer half, not everything: {after}");
        let lines = lines(&path); // parses ⇒ no half line survived the cut
        let kept: Vec<i64> = lines.iter().map(|l| l["id"].as_i64().unwrap()).collect();
        assert_eq!(kept.last(), Some(&newest), "the newest line is kept");
        assert!(kept[0] > 1, "the oldest lines are dropped");
        assert!(kept.windows(2).all(|w| w[1] == w[0] + 1), "the kept lines stay contiguous");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The append that crosses the cap is the one that trims — no separate sweep, no background task.
    #[test]
    fn the_append_that_crosses_the_cap_compacts() {
        let dir = dir("compact-on-append");
        let path = dir.join(FILE_NAME);
        let newest = oversized_ledger(&path) + 1;
        let over = std::fs::metadata(&path).unwrap().len();

        append(&path, &entry(newest, json!({ "kind": "task.created", "title": "the straw" })));

        let after = std::fs::metadata(&path).unwrap().len();
        assert!(after < over, "the ledger shrank on the append that outgrew the cap: {over} → {after}");
        let lines = lines(&path);
        assert_eq!(lines.last().unwrap()["event"]["title"], json!("the straw"), "the new line survives the trim");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Compaction is skipped — never queued, never blocking — while another writer holds the lock. The
    /// mutation it rode in on has already committed; a rotation is not worth waiting on.
    #[test]
    fn compaction_is_skipped_when_another_writer_holds_the_lock() {
        let dir = dir("compact-contended");
        let path = dir.join(FILE_NAME);
        oversized_ledger(&path);
        let over = std::fs::metadata(&path).unwrap().len();

        let held = try_lock(&lock_path(&path)).expect("the lock is free");
        compact(&path);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), over, "contended: left over the cap, untouched");

        drop(held);
        compact(&path);
        assert!(std::fs::metadata(&path).unwrap().len() < over, "the next writer trims it instead");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn concurrent_writers_do_not_interleave_their_lines() {
        let dir = dir("concurrent");
        let path = dir.join(FILE_NAME);

        // Threads share no handle: each append opens `O_APPEND`, writes once, closes — the same path a
        // second process takes.
        std::thread::scope(|s| {
            for t in 0..8 {
                let path = path.clone();
                s.spawn(move || {
                    for i in 0..50 {
                        let title = format!("{t}-{i}");
                        append(&path, &entry(t * 50 + i, json!({ "kind": "task.created", "title": title })));
                    }
                });
            }
        });

        let lines = lines(&path); // every line parses ⇒ no two writes tore each other
        assert_eq!(lines.len(), 400);

        std::fs::remove_dir_all(&dir).ok();
    }
}
