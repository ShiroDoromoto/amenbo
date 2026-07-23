//! The **plugin execution log** — why a plugin did, or did not, do something (`AMB-D-361`).
//!
//! A hook is fire-and-forget: nobody waits on it, nothing fails when it fails (`AMB-D-352`). That is what
//! makes the write path safe, and it is also what leaves a user with no way to answer *why did nothing
//! happen*. This file is that answer, and nothing more: the last runs of each plugin, each with the
//! diagnosis its author wrote to stderr (`AMB-D-353`).
//!
//! - **Its own file**, `<base>/plugin-runs.jsonl` ([`crate::config::Paths::plugin_log_file`]). Not the
//!   activity ledger: activity narrates what happened to the *user's work*, and a plugin's exit code is
//!   not one of those events (`AMB-D-361` — the ledger's purity is the point).
//! - **Machine-local, and outside every backup and export.** `backup` snapshots the truth source and its
//!   attachment bytes; `export` walks the record tables. A debugging log is neither, and it is about *this
//!   machine's* installs, so carrying it to another device would say nothing there.
//! - **No secret ever reaches it.** A plugin's secrets are injected as environment variables
//!   ([`crate::plugin_inject`], `AMB-D-356`), and this module is never handed the environment: a [`Run`]
//!   carries the plugin's name, the event, the outcome, the exit code, the duration and stderr. There is
//!   no field a secret could ride in — the exclusion is structural, not a filter that could be forgotten.
//! - **Bounded by construction** (`AMB-D-361`): the last [`RUNS_PER_PLUGIN`] runs of each plugin, each with
//!   at most [`MAX_STDERR_BYTES`] of stderr. The whole file is therefore bounded by what is installed, and
//!   there is no long history to rotate — a deeper one is a logging plugin's business, not amenbo's.
//! - **Never fatal.** A log this cannot write is a `warn` and nothing else; the hook it describes has
//!   already run.
//!
//! **Concurrency, the way the activity ledger does it** ([`crate::activity_log`]): hooks run on their own
//! threads, and several amenbo processes can fire at once, so an append is one `write` on an `O_APPEND`
//! handle — atomic in its offset-and-write, hence the line cap and the deliberate [`std::io::Write::write`]
//! over `write_all`. The trim that keeps each plugin's ring is a read-modify-write, which atomic appends
//! cannot protect, so it takes a `try_lock` sidecar and **skips** when someone else holds it: overshooting
//! the ring for a moment is harmless, and blocking a hook thread on a log rotation would not be.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::time::Timestamp;

/// File name of the execution log, kept beside the truth source in the base directory.
pub const FILE_NAME: &str = "plugin-runs.jsonl";

/// File name of the sidecar that serialises the trim (never taken for an append).
pub const LOCK_NAME: &str = "plugin-runs.jsonl.lock";

/// Schema version stamped on every line this build writes. A line of any other version is skipped by the
/// reader.
pub const LINE_VERSION: i64 = 1;

/// How many runs of **one plugin** the log keeps. The ring is per plugin so a chatty one cannot push a
/// quiet one's only failure out of the file — which is the failure a reader is most likely looking for.
pub const RUNS_PER_PLUGIN: usize = 50;

/// How much of one run's stderr is kept. A plugin that writes a megabyte of diagnostics gets its head and
/// its tail, with the middle elided: the two ends are where a cause and a summary sit, and keeping them
/// both costs one bounded line.
pub const MAX_STDERR_BYTES: usize = 4 * 1024;

/// Cap on one line, everything included. Above this the OS no longer guarantees the append is a single
/// atomic `write`, so a line that would exceed it drops its stderr rather than risking an interleaved line.
pub const MAX_LINE_BYTES: usize = 16 * 1024;

/// The file size past which an append also runs the trim. Comfortably above what the rings themselves come
/// to for a handful of plugins, so the read-modify-write happens rarely rather than on every hook.
const TRIM_AT_BYTES: u64 = 512 * 1024;

/// How one run ended — the four ways a hook can finish ([`crate::plugin_hooks`]), plus the one thing that
/// is not a run at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Exited cleanly (code 0).
    Ok,
    /// Ran to completion but did not exit 0 — including a signalled death, which carries no code.
    Failed,
    /// Overran the hook timeout and was killed.
    TimedOut,
    /// Never started: the program could not be spawned.
    NotLaunched,
    /// **Not a run.** Retention had trimmed the outbox past the dispatcher's cursor, so a span of events
    /// was never delivered to anybody (`Delivered::gapped`, `AMB-D-352`). What was lost cannot be named —
    /// the events are gone — so the line records that it happened and when, and no more.
    Gap,
}

impl Outcome {
    /// The stored spelling, shared by the writer and the reader so the two cannot drift.
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Failed => "failed",
            Outcome::TimedOut => "timed_out",
            Outcome::NotLaunched => "not_launched",
            Outcome::Gap => "gap",
        }
    }

    /// Read a stored spelling back; `None` for one this build does not know.
    pub fn parse(s: &str) -> Option<Outcome> {
        match s {
            "ok" => Some(Outcome::Ok),
            "failed" => Some(Outcome::Failed),
            "timed_out" => Some(Outcome::TimedOut),
            "not_launched" => Some(Outcome::NotLaunched),
            "gap" => Some(Outcome::Gap),
            _ => None,
        }
    }
}

/// One plugin run, as it goes to the log. Built by the hook runner, which is the only layer that knows all
/// of it at once — and which hands over exactly these fields, never the invocation, so the environment (and
/// with it every injected secret) has no path into this file.
#[derive(Clone, Debug)]
pub struct Run {
    /// The plugin's name, as the installed registry knows it.
    pub plugin: String,
    /// The event that fired it.
    pub event: &'static str,
    pub outcome: Outcome,
    /// The exit code, when there was one (`None` for a timeout kill, a signalled death, or a hook that
    /// never launched).
    pub code: Option<i32>,
    /// How long it ran. For a timeout this is the bound it was killed at; for a hook that never launched,
    /// zero.
    pub elapsed: Duration,
    /// What the plugin wrote to stderr — its diagnostics (`AMB-D-353`), capped at [`MAX_STDERR_BYTES`].
    /// Empty for an outcome with no child to have written any.
    pub stderr: String,
}

impl Run {
    /// The line as it is written: one JSON object plus its LF. Returns `None` when even the stderr-less
    /// form does not fit in [`MAX_LINE_BYTES`], which the field caps make impossible in practice.
    fn to_line(&self) -> Option<Vec<u8>> {
        let line = encode(self, &clamp_stderr(&self.stderr));
        if line.len() <= MAX_LINE_BYTES {
            return Some(line);
        }
        // A name long enough to blow the cap on its own is not worth a second guess: keep the fact of the
        // run and drop the text.
        let line = encode(self, "");
        (line.len() <= MAX_LINE_BYTES).then_some(line)
    }
}

/// One run's stderr, cut to [`MAX_STDERR_BYTES`] as a head and a tail with the middle elided. Cutting on a
/// char boundary keeps the line valid UTF-8 (a JSON string cannot carry half a code point).
fn clamp_stderr(stderr: &str) -> String {
    if stderr.len() <= MAX_STDERR_BYTES {
        return stderr.to_string();
    }
    let half = MAX_STDERR_BYTES / 2;
    let head_end = floor_boundary(stderr, half);
    let tail_start = ceil_boundary(stderr, stderr.len() - half);
    format!("{}\n…[elided]…\n{}", &stderr[..head_end], &stderr[tail_start..])
}

/// The greatest char boundary at or below `i`.
fn floor_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The least char boundary at or above `i`.
fn ceil_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn encode(run: &Run, stderr: &str) -> Vec<u8> {
    let obj = json!({
        "v": LINE_VERSION,
        "at": Timestamp::now().to_rfc3339_z(),
        "plugin": run.plugin,
        "event": run.event,
        "outcome": run.outcome.as_str(),
        "code": run.code,
        "elapsed_ms": run.elapsed.as_millis() as u64,
        "stderr": stderr,
    });
    let mut line = serde_json::to_vec(&obj).unwrap_or_default();
    line.push(b'\n');
    line
}

/// One line read back. The reader is **tolerant by contract**, as the activity ledger's is: a line this
/// build cannot make sense of — an unknown version, a missing field, the debris of a short write — is
/// skipped rather than failing the read, because one bad byte must not cost a user the whole log.
#[derive(Clone, Debug)]
pub struct Line {
    pub at: Timestamp,
    pub plugin: String,
    pub event: String,
    pub outcome: Outcome,
    pub code: Option<i32>,
    pub elapsed_ms: u64,
    pub stderr: String,
}

/// Record one run, then trim if the file has outgrown [`TRIM_AT_BYTES`]. **Infallible by contract**: the
/// hook has already run, so a log that cannot be written is a `warn` and nothing more.
pub fn record(path: &Path, run: &Run) {
    let Some(line) = run.to_line() else {
        tracing::warn!(plugin = %run.plugin, "plugin log: run does not fit in a line; dropped");
        return;
    };
    match write_line(path, &line) {
        Ok(size) if size > TRIM_AT_BYTES => trim(path),
        Ok(_) => {}
        Err(e) => tracing::warn!(plugin = %run.plugin, error = %e, "plugin log: append failed; run dropped"),
    }
}

/// Record a delivery gap — events the dispatcher could never deliver because retention passed its cursor
/// (`AMB-D-361`). It names no plugin, because the lost events were never resolved to one, and no span,
/// because what was trimmed is gone; the fact and its instant are the whole content.
pub fn record_gap(path: &Path) {
    record(
        path,
        &Run {
            plugin: String::new(),
            event: "",
            outcome: Outcome::Gap,
            code: None,
            elapsed: Duration::ZERO,
            stderr: String::new(),
        },
    );
}

/// Every line of the log, oldest first. A missing file is an empty log (nothing has fired on this machine
/// yet), not a failure. Reading the whole file is right *here* and nowhere else: the file is bounded by
/// construction — [`RUNS_PER_PLUGIN`] runs per installed plugin — so "the whole log" is a window already.
pub fn read(path: &Path) -> Vec<Line> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    String::from_utf8_lossy(&bytes).lines().filter_map(parse_line).collect()
}

/// The last runs of one plugin, newest first — what a face shows beside a plugin ("recent runs",
/// `AMB-D-361`). At most [`RUNS_PER_PLUGIN`] exist to find.
pub fn recent(path: &Path, plugin: &str) -> Vec<Line> {
    let mut rows: Vec<Line> = read(path).into_iter().filter(|l| l.plugin == plugin).collect();
    rows.reverse();
    rows
}

fn parse_line(line: &str) -> Option<Line> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("v").and_then(Value::as_i64) != Some(LINE_VERSION) {
        return None; // a version this build does not know how to read
    }
    Some(Line {
        at: Timestamp::parse_rfc3339(v.get("at")?.as_str()?)?,
        plugin: v.get("plugin")?.as_str()?.to_string(),
        event: v.get("event")?.as_str()?.to_string(),
        outcome: Outcome::parse(v.get("outcome")?.as_str()?)?,
        code: v.get("code").and_then(Value::as_i64).map(|c| c as i32),
        elapsed_ms: v.get("elapsed_ms").and_then(Value::as_u64).unwrap_or(0),
        stderr: v.get("stderr").and_then(Value::as_str).unwrap_or("").to_string(),
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
        tracing::warn!(written, len = line.len(), "plugin log: short write; line left truncated");
    }
    file.metadata().map(|m| m.len())
}

/// Cut the log back to each plugin's last [`RUNS_PER_PLUGIN`] runs. Skipped — not queued — when another
/// writer is already trimming: the file is a debugging aid, and a hook thread must never wait on it.
fn trim(path: &Path) {
    let Some(_lock) = try_lock(&lock_path(path)) else { return };
    if let Err(e) = trim_locked(path) {
        tracing::warn!(error = %e, "plugin log: trim failed; the file keeps growing");
    }
}

fn trim_locked(path: &Path) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    if bytes.len() as u64 <= TRIM_AT_BYTES {
        return Ok(()); // another writer already trimmed it while we waited for the lock
    }
    let kept = keep_rings(&String::from_utf8_lossy(&bytes));
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, kept)?;
    std::fs::rename(&tmp, path)
}

/// The lines worth keeping, in their original order: the last [`RUNS_PER_PLUGIN`] of each plugin, and the
/// same number of gap lines (which belong to no plugin and are the rarest thing in the file).
///
/// It works on the raw lines rather than parsed [`Line`]s so that a line this build cannot parse is dropped
/// by the very same pass — the trim is the one place where a file written by another generation is
/// rewritten, and keeping debris it cannot read would leave it there for good.
fn keep_rings(text: &str) -> String {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut kept: Vec<&str> = Vec::new();
    // Walk backwards: the newest run of a plugin is the last one to have been appended, so counting from
    // the end is what makes the ring keep the *recent* runs.
    for line in text.lines().rev() {
        let Some(parsed) = parse_line(line) else { continue };
        let n = counts.entry(parsed.plugin).or_default();
        if *n >= RUNS_PER_PLUGIN {
            continue;
        }
        *n += 1;
        kept.push(line);
    }
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

/// Take the trim lock, or `None` when someone else holds it (or the sidecar cannot be made).
fn try_lock(path: &Path) -> Option<File> {
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).ok()?;
    match file.try_lock() {
        Ok(()) => Some(file),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Error(e)) => {
            tracing::warn!(error = %e, "plugin log: cannot take the trim lock");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        amenbo_scratch::scratch(&format!("plugin-log-{tag}"))
    }

    fn run(plugin: &str, outcome: Outcome, stderr: &str) -> Run {
        Run {
            plugin: plugin.to_string(),
            event: "task.created",
            outcome,
            code: match outcome {
                Outcome::Ok => Some(0),
                Outcome::Failed => Some(2),
                _ => None,
            },
            elapsed: Duration::from_millis(12),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn a_recorded_run_reads_back_whole() {
        let dir = dir("roundtrip");
        let path = dir.join(FILE_NAME);

        record(&path, &run("slack", Outcome::Failed, "boom: no such channel"));

        let lines = read(&path);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].plugin, "slack");
        assert_eq!(lines[0].event, "task.created");
        assert_eq!(lines[0].outcome, Outcome::Failed);
        assert_eq!(lines[0].code, Some(2));
        assert_eq!(lines[0].elapsed_ms, 12);
        assert_eq!(lines[0].stderr, "boom: no such channel");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The whole vocabulary a hook can end with survives the round trip — including the gap, which names
    /// no plugin at all.
    #[test]
    fn every_outcome_round_trips() {
        let dir = dir("outcomes");
        let path = dir.join(FILE_NAME);

        for outcome in [Outcome::Ok, Outcome::Failed, Outcome::TimedOut, Outcome::NotLaunched] {
            record(&path, &run("slack", outcome, ""));
        }
        record_gap(&path);

        let got: Vec<Outcome> = read(&path).iter().map(|l| l.outcome).collect();
        assert_eq!(
            got,
            vec![Outcome::Ok, Outcome::Failed, Outcome::TimedOut, Outcome::NotLaunched, Outcome::Gap]
        );
        let gap = read(&path).pop().unwrap();
        assert_eq!(gap.plugin, "", "a gap belongs to no plugin");
        assert_eq!(gap.code, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A plugin that writes without end costs one bounded line: the head and the tail, and a mark where
    /// the middle went.
    #[test]
    fn a_runaway_stderr_is_cut_to_its_two_ends() {
        let dir = dir("stderr");
        let path = dir.join(FILE_NAME);
        let huge = format!("{}{}", "A".repeat(200_000), "Z".repeat(10));

        record(&path, &run("noisy", Outcome::Failed, &huge));

        let line = read(&path).pop().unwrap();
        assert!(line.stderr.len() <= MAX_STDERR_BYTES + 32, "kept: {}", line.stderr.len());
        assert!(line.stderr.starts_with("AAA"), "the head is kept");
        assert!(line.stderr.ends_with("ZZZ"), "and so is the tail");
        assert!(line.stderr.contains("elided"), "the cut is marked");
        // The file itself stays within the atomic-append cap.
        assert!(std::fs::metadata(&path).unwrap().len() <= MAX_LINE_BYTES as u64);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Multi-byte text is cut on a char boundary, so the line stays valid UTF-8 and reads back.
    #[test]
    fn a_multibyte_stderr_is_cut_on_a_char_boundary() {
        let dir = dir("utf8");
        let path = dir.join(FILE_NAME);

        record(&path, &run("noisy", Outcome::Failed, &"あ".repeat(10_000)));

        let line = read(&path).pop().unwrap();
        assert!(line.stderr.starts_with('あ') && line.stderr.ends_with('あ'));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The ring is per plugin: a chatty plugin cannot push a quiet one's only failure out of the file.
    #[test]
    fn the_ring_keeps_each_plugins_last_runs() {
        let text = {
            let dir = dir("ring");
            let path = dir.join(FILE_NAME);
            // One line from the quiet plugin, then far more than a ring's worth from the chatty one.
            record(&path, &run("quiet", Outcome::Failed, "the one failure"));
            for _ in 0..(RUNS_PER_PLUGIN * 2) {
                record(&path, &run("chatty", Outcome::Ok, ""));
            }
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::remove_dir_all(&dir).ok();
            text
        };

        let kept = keep_rings(&text);
        let lines: Vec<Line> = kept.lines().filter_map(parse_line).collect();
        assert_eq!(lines.iter().filter(|l| l.plugin == "chatty").count(), RUNS_PER_PLUGIN);
        assert_eq!(
            lines.iter().filter(|l| l.plugin == "quiet").count(),
            1,
            "the quiet plugin's only run survives the chatty one",
        );
    }

    /// The trim drops what it cannot read, rather than carrying another generation's debris forever.
    #[test]
    fn the_trim_drops_unreadable_lines() {
        let kept = keep_rings("{\"v\":999}\nnot json at all\n");
        assert_eq!(kept, "", "nothing readable, nothing kept");
    }

    /// A log that has never been written is an empty one, not a failure.
    #[test]
    fn a_missing_log_reads_empty() {
        let dir = dir("missing");
        assert!(read(&dir.join("nothing.jsonl")).is_empty());
        assert!(recent(&dir.join("nothing.jsonl"), "slack").is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `recent` answers per plugin, newest first — the shape a face shows beside one plugin.
    #[test]
    fn recent_is_one_plugins_runs_newest_first() {
        let dir = dir("recent");
        let path = dir.join(FILE_NAME);
        record(&path, &run("slack", Outcome::Ok, "first"));
        record(&path, &run("worktree", Outcome::Ok, "other plugin"));
        record(&path, &run("slack", Outcome::TimedOut, "second"));

        let rows = recent(&path, "slack");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].stderr, "second", "newest first");
        assert_eq!(rows[1].stderr, "first");

        std::fs::remove_dir_all(&dir).ok();
    }
}
