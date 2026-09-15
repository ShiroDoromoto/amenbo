//! **The delivery log** — what could not be carried out of a write, and when (`AMB-D-361`, `AMB-D-352`).
//!
//! Carrying a write out is best-effort by design: a notification that will not go is dropped rather than
//! retried, and a span of events retention trimmed before anyone walked it cannot be replayed. That is what
//! keeps the write path safe, and it is also what leaves a user with no way to answer *why did nothing
//! arrive*. This file is that answer.
//!
//! It holds the two things nobody else can report:
//!
//! - a **gap** — [`crate::outbox_drive`] found its cursor behind the outbox's retention watermark, so the
//!   events between the two reached no reader at all. What was lost cannot be named; the fact and its
//!   instant are the whole content.
//! - a **refusal** — a notification the far side would not take ([`crate::notify_dispatch`]): a relay that
//!   rejected the account, a webhook that answered 404.
//!
//! Both are written by a process with nowhere else to write. A sender is detached, with its stdio closed
//! and nobody waiting on it, and a drive rides a command whose output is the user's own — so a line here is
//! the only trace either one leaves.
//!
//! - **Its own file**, `<base>/delivery.jsonl` ([`crate::config::Paths::delivery_log_file`]). Not the
//!   activity ledger: activity narrates what happened to the *user's work*, and a webhook's status code is
//!   not one of those events (`AMB-D-361` — the ledger's purity is the point).
//! - **Machine-local, and outside every backup and export.** It is about what *this* machine could not
//!   send, so carrying it to another device would say nothing there.
//! - **No secret ever reaches it.** A line carries an instant, which of the two things happened, and — for
//!   a refusal — the target's id and the reason the far side gave. The webhook URL, the mail password and
//!   the message text are none of them fields here, so the exclusion is structural rather than a filter
//!   that could be forgotten.
//! - **Bounded by construction**: the last [`KEEP_LINES`] lines, each at most [`MAX_LINE_BYTES`].
//! - **Never fatal.** A log this cannot write is a `warn` and nothing else; what it describes has already
//!   happened.
//!
//! **Concurrency, the way the activity ledger does it** ([`crate::activity_log`]): several Amenbo processes
//! can be carrying at once, so an append is one `write` on an `O_APPEND` handle — atomic in its
//! offset-and-write, hence the line cap and the deliberate [`std::io::Write::write`] over `write_all`. The
//! trim is a read-modify-write, which atomic appends cannot protect, so it takes a `try_lock` sidecar and
//! **skips** when someone else holds it: overshooting for a moment is harmless, and blocking a write path
//! on a log rotation would not be.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::time::Timestamp;

/// File name of the delivery log, kept beside the truth source in the base directory.
pub const FILE_NAME: &str = "delivery.jsonl";

/// File name of the sidecar that serialises the trim (never taken for an append).
pub const LOCK_NAME: &str = "delivery.jsonl.lock";

/// Schema version stamped on every line this build writes. A line of any other version is skipped by the
/// reader.
pub const LINE_VERSION: i64 = 1;

/// How many lines the log keeps. One ring for the whole file, unlike the plugin execution log's per-plugin
/// one: there is no owner here to starve a quiet one out, only two kinds of line and both of them rare.
pub const KEEP_LINES: usize = 200;

/// How much of one refusal's reason is kept. A relay that answers with a page of HTML gets its head; what
/// names the cause is at the front of it.
pub const MAX_WHY_BYTES: usize = 1024;

/// Cap on one line, everything included. Above this the OS no longer guarantees the append is a single
/// atomic `write`, so a line that would exceed it drops its reason rather than risking an interleaved line.
pub const MAX_LINE_BYTES: usize = 4 * 1024;

/// The file size past which an append also runs the trim. Comfortably above what [`KEEP_LINES`] lines come
/// to, so the read-modify-write happens rarely rather than on every append.
const TRIM_AT_BYTES: u64 = 128 * 1024;

/// Which of the two things happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Retention had trimmed the outbox past the drive's cursor, so a span of events was never walked and
    /// reached nobody (`AMB-D-352`). What was lost cannot be named — the rows are gone — so the line
    /// records that it happened and when, and no more.
    Gap,
    /// A notification the far side would not take (`AMB-D-885`). It is dropped rather than retried, so this
    /// line is the only trace there is that somebody was not told.
    Refused,
}

impl Outcome {
    /// The stored spelling, shared by the writer and the reader so the two cannot drift.
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Gap => "gap",
            Outcome::Refused => "refused",
        }
    }

    /// Read a stored spelling back; `None` for one this build does not know.
    pub fn parse(s: &str) -> Option<Outcome> {
        match s {
            "gap" => Some(Outcome::Gap),
            "refused" => Some(Outcome::Refused),
            _ => None,
        }
    }
}

/// One line read back. The reader is **tolerant by contract**, as the activity ledger's is: a line this
/// build cannot make sense of — an unknown version, a missing field, the debris of a short write — is
/// skipped rather than failing the read, because one bad byte must not cost a user the whole log.
#[derive(Clone, Debug)]
pub struct Line {
    /// When it happened.
    pub at: Timestamp,
    /// Which of the two it was.
    pub outcome: Outcome,
    /// What the far side said, for a refusal. Empty on a gap, which has nothing to say beyond itself.
    pub why: String,
}

/// Record a gap — a span of events the drive could never carry out because retention passed its cursor.
pub fn record_gap(path: &Path) {
    record(path, Outcome::Gap, "");
}

/// Record a notification the far side would not take. `why` is the reason as it came back, trimmed to
/// [`MAX_WHY_BYTES`].
pub fn record_refused(path: &Path, why: &str) {
    record(path, Outcome::Refused, why);
}

/// Every line of the log, oldest first. A missing file is an empty log (nothing has failed on this machine
/// yet), not a failure. Reading the whole file is right *here* and nowhere else: the file is bounded by
/// construction — [`KEEP_LINES`] lines — so "the whole log" is a window already.
pub fn read(path: &Path) -> Vec<Line> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    String::from_utf8_lossy(&bytes).lines().filter_map(parse_line).collect()
}

/// Append one line, then trim if the file has outgrown [`TRIM_AT_BYTES`]. **Infallible by contract**: what
/// this describes has already happened, so a log that cannot be written is a `warn` and nothing more.
fn record(path: &Path, outcome: Outcome, why: &str) {
    let mut line = to_line(outcome, why, MAX_WHY_BYTES);
    if line.len() > MAX_LINE_BYTES {
        // A reason cut to MAX_WHY_BYTES can still outgrow the line: JSON escapes a control character
        // to six bytes, so a relay answering with a binary body reaches the cap on a fraction of it. The
        // reason is the only field that can grow, so it is the only one worth dropping — and a line that
        // says a refusal happened without saying why still says the thing a reader came for.
        line = to_line(outcome, "", 0);
    }
    match write_line(path, &line) {
        Ok(size) if size > TRIM_AT_BYTES => trim(path),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "delivery log: append failed; the line is dropped"),
    }
}

/// One line's bytes, newline included — `why` cut to at most `keep` bytes, on a character boundary.
fn to_line(outcome: Outcome, why: &str, keep: usize) -> Vec<u8> {
    let mut end = keep.min(why.len());
    while end > 0 && !why.is_char_boundary(end) {
        end -= 1;
    }
    let value = json!({
        "v": LINE_VERSION,
        "at": Timestamp::now().to_rfc3339_z(),
        "outcome": outcome.as_str(),
        "why": &why[..end],
    });
    let mut line = value.to_string().into_bytes();
    line.push(b'\n');
    line
}

fn parse_line(line: &str) -> Option<Line> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("v").and_then(Value::as_i64) != Some(LINE_VERSION) {
        return None; // a version this build does not know how to read
    }
    Some(Line {
        at: Timestamp::parse_rfc3339(v.get("at")?.as_str()?)?,
        outcome: Outcome::parse(v.get("outcome")?.as_str()?)?,
        why: v.get("why").and_then(Value::as_str).unwrap_or("").to_string(),
    })
}

/// Append `line` with one atomic `write` and return the file's size afterwards. The handle is opened and
/// closed here — nothing holds it across calls, so Windows can still replace the file under a trim.
fn write_line(path: &Path, line: &[u8]) -> std::io::Result<u64> {
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    // Deliberately `write`, not `write_all`: a short write must not be retried, because the retry would be
    // a second syscall and another writer's line could land between the halves.
    let written = file.write(line)?;
    if written < line.len() {
        tracing::warn!(written, len = line.len(), "delivery log: short write; line left truncated");
    }
    file.metadata().map(|m| m.len())
}

/// Cut the log back to its last [`KEEP_LINES`] lines. Skipped — not queued — when another writer is already
/// trimming: the file is a debugging aid, and a write path must never wait on it.
fn trim(path: &Path) {
    let Some(_lock) = try_lock(&lock_path(path)) else { return };
    if let Err(e) = trim_locked(path) {
        tracing::warn!(error = %e, "delivery log: trim failed; the file keeps growing");
    }
}

fn trim_locked(path: &Path) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    if bytes.len() as u64 <= TRIM_AT_BYTES {
        return Ok(()); // another writer already trimmed it while we waited for the lock
    }
    let kept = keep_last(&String::from_utf8_lossy(&bytes));
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, kept)?;
    std::fs::rename(&tmp, path)
}

/// The last [`KEEP_LINES`] readable lines, in their original order.
///
/// It works on the raw lines rather than parsed [`Line`]s so that a line this build cannot parse is dropped
/// by the very same pass — the trim is the one place where a file written by another generation is
/// rewritten, and keeping debris it cannot read would leave it there for good.
fn keep_last(text: &str) -> String {
    let mut kept: Vec<&str> = text.lines().rev().filter(|l| parse_line(l).is_some()).take(KEEP_LINES).collect();
    kept.reverse();
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// The trim lock beside `path`.
fn lock_path(path: &Path) -> PathBuf {
    path.with_file_name(LOCK_NAME)
}

/// The trim lock, held for the length of a trim.
///
/// It releases **by asking**, not by closing the fd: a close's release lands microseconds late, and a
/// writer that arrives inside that window skips a trim it could have performed. Same reasoning, and the
/// same measured lag, as [`crate::swap_lock::SwapGuard`].
struct TrimLock {
    file: File,
}

impl Drop for TrimLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Take the trim lock, or `None` when someone else holds it (or the sidecar cannot be made).
fn try_lock(path: &Path) -> Option<TrimLock> {
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).ok()?;
    match file.try_lock() {
        Ok(()) => Some(TrimLock { file }),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Error(e)) => {
            tracing::warn!(error = %e, "delivery log: cannot take the trim lock");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(tag: &str) -> PathBuf {
        amenbo_scratch::scratch(&format!("delivery-log-{tag}")).join(FILE_NAME)
    }

    /// The two kinds of line go in and come back out, oldest first, each saying which it was.
    #[test]
    fn both_kinds_read_back_in_the_order_they_happened() {
        let path = log("both");
        record_gap(&path);
        record_refused(&path, "slack: 404");

        let lines = read(&path);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].outcome, Outcome::Gap);
        assert!(lines[0].why.is_empty(), "a gap has nothing to say beyond itself");
        assert_eq!(lines[1].outcome, Outcome::Refused);
        assert_eq!(lines[1].why, "slack: 404");
    }

    /// A log nothing has written to is empty rather than an error — the state of nearly every machine.
    #[test]
    fn a_missing_file_reads_as_an_empty_log() {
        assert!(read(&log("missing").with_file_name("nothing-here.jsonl")).is_empty());
    }

    /// A reason too long for one atomic append costs its reason and not its line: the fact that somebody
    /// was not told is what a reader came for, and it survives.
    #[test]
    fn a_reason_too_long_for_a_line_is_dropped_and_the_line_kept() {
        let path = log("long");
        record_refused(&path, &"x".repeat(MAX_LINE_BYTES * 2));

        let lines = read(&path);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].outcome, Outcome::Refused);
        assert!(lines[0].why.len() <= MAX_WHY_BYTES);
    }

    /// A line this build cannot read is skipped rather than failing the read around it.
    #[test]
    fn debris_is_skipped_and_the_rest_still_reads() {
        let path = log("debris");
        record_gap(&path);
        let mut file = OpenOptions::new().append(true).create(true).open(&path).unwrap();
        file.write_all(b"{not json\n{\"v\":99,\"at\":\"\",\"outcome\":\"gap\"}\n").unwrap();
        drop(file);
        record_refused(&path, "mail: auth refused");

        let lines = read(&path);
        assert_eq!(lines.len(), 2, "the two readable lines, and neither of the two unreadable ones");
        assert_eq!(lines[0].outcome, Outcome::Gap);
        assert_eq!(lines[1].outcome, Outcome::Refused);
    }

    /// The trim keeps the newest [`KEEP_LINES`] and drops what this build cannot parse, in one pass.
    #[test]
    fn the_trim_keeps_the_newest_lines() {
        let text: String = (0..KEEP_LINES + 50)
            .map(|i| String::from_utf8(to_line(Outcome::Refused, &format!("n{i}"), 8)).unwrap())
            .collect();
        let kept = keep_last(&(text + "{not json\n"));
        let lines: Vec<Line> = kept.lines().filter_map(parse_line).collect();
        assert_eq!(lines.len(), KEEP_LINES);
        assert_eq!(lines.last().unwrap().why, format!("n{}", KEEP_LINES + 49), "the newest line survives");
    }
}
