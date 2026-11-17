//! **The Viewer's send log** — `<base>/logs/viewer-send.log`, what a carrier and a setup leave behind
//! (`AMB-D-382`).
//!
//! A carrier is a process of its own with its stdio closed and nobody waiting on it (`AMB-D-884`), and
//! neither face installs a `tracing` subscriber that would catch what it says (`AMB-D-290`). So a send
//! that did not go left nothing to read. This file is where it goes now, from either face: the CLI's
//! `viewer-carrier` and the app's `--viewer-carrier` both run [`super::send::carry_process`], which
//! writes here.
//!
//! - **Beside `amenbo.log` and `perf.log`, and mixed into neither.** Same folder, so a diagnosis is one
//!   folder to ask for; its own file, because the app's diagnostic log is the `log` facade's and perf is
//!   `target="perf"` only. Lines on that target are left out of this one.
//! - **Scoped, never global.** [`within`] holds the subscriber for the length of one call on this thread.
//!   Setup runs inside the app, whose global `tracing` subscriber belongs to perf; taking it over would
//!   cost the app its perf log.
//! - **Diagnostics, never content.** Counts, waits, sequence numbers, ids and the reason something
//!   failed. What a send carries is sealed before it leaves, and no line here names a task, a decision, a
//!   title, a comment, an attachment or a secret.
//! - **It cannot grow without bound.** The file rotates at [`MAX_FILE_BYTES`] and only
//!   [`ARCHIVED_GENERATIONS`] rotated copies are kept beside it, so the whole log stays under
//!   [`CEILING_BYTES`]. The copies have fixed names, so the bound holds however rotations overlap.
//! - **Never fatal.** A line this cannot write is dropped; what it describes has already happened.
//!
//! **Several carriers can be writing at once** — one starts at each write, before it takes its turn. An
//! append is one `write` on an `O_APPEND` handle opened for that line, so lines do not interleave, and a
//! rotation is two renames that either land or are left to the next line. Two carriers rotating together
//! can push one generation out early; neither can add a fourth file.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tracing::Level;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::Layer as _;

/// The folder the log lives in under the base directory — the one `amenbo.log` and `perf.log` are in.
pub const DIR_NAME: &str = "logs";

/// The stem of the active file, written as `<stem>.log`; rotated generations are `<stem>.<n>.log`.
const FILE_STEM: &str = "viewer-send";

/// How large one generation may get before it rotates. The same size `amenbo.log` rotates at.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// How many rotated generations are kept **besides** the file being written.
const ARCHIVED_GENERATIONS: u32 = 2;

/// The most the whole send log can occupy: every archive at its rotation size, plus the active file.
const CEILING_BYTES: u64 = MAX_FILE_BYTES * (ARCHIVED_GENERATIONS as u64 + 1);

/// The ceiling is what lets this log be on by default, so it is checked where it cannot be skipped: raise
/// either number past a few MiB and the crate stops compiling. Doing that means changing what `AMB-D-382`
/// promised the user's disk, which is a decision to take again rather than a constant to bump.
const _: () = assert!(CEILING_BYTES <= 4 * 1024 * 1024);

/// The target perf measures on (`app/src-tauri/src/perf.rs`). Its lines belong in `perf.log` only.
const PERF_TARGET: &str = "perf";

/// The active file under `base_dir`.
pub fn file_at(base_dir: &Path) -> PathBuf {
    base_dir.join(DIR_NAME).join(format!("{FILE_STEM}.log"))
}

/// Run `f` with what it logs on this thread written to `<base_dir>/logs/viewer-send.log`.
pub fn within<T>(base_dir: &Path, f: impl FnOnce() -> T) -> T {
    let writer = SendLog { dir: base_dir.join(DIR_NAME) };
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_filter(tracing_subscriber::filter::filter_fn(|meta| {
            meta.target() != PERF_TARGET && *meta.level() <= Level::INFO
        }));
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, f)
}

/// Where the lines go. A line opens the file, appends, and closes it again, so nothing is held between
/// lines that a rotation by another carrier would leave pointing at the wrong file.
#[derive(Clone)]
struct SendLog {
    dir: PathBuf,
}

impl<'a> MakeWriter<'a> for SendLog {
    type Writer = SendLog;
    fn make_writer(&'a self) -> SendLog {
        self.clone()
    }
}

impl Write for SendLog {
    /// One formatted event arrives as one call (`fmt` formats into a buffer and writes it whole).
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        append(&self.dir, buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn generation(dir: &Path, n: u32) -> PathBuf {
    if n == 0 {
        dir.join(format!("{FILE_STEM}.log"))
    } else {
        dir.join(format!("{FILE_STEM}.{n}.log"))
    }
}

/// Append `line` with one `write`, and say how much of it went.
fn append(dir: &Path, line: &[u8]) -> io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let active = generation(dir, 0);
    let held = std::fs::metadata(&active).map(|m| m.len()).unwrap_or(0);
    if held > 0 && held + line.len() as u64 > MAX_FILE_BYTES {
        rotate(dir);
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&active)?;
    file.write(line)
}

/// Move every generation one place along, oldest first, so the oldest is the one written over. A rename
/// that fails — the file already moved by another carrier, or held open on Windows — is left: the next
/// line tries again, and the names stay the same three either way.
fn rotate(dir: &Path) {
    for n in (0..ARCHIVED_GENERATIONS).rev() {
        let _ = std::fs::rename(generation(dir, n), generation(dir, n + 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(tag: &str) -> PathBuf {
        let dir = amenbo_scratch::scratch(&format!("viewer-send-log-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn what_is_said_within_lands_in_the_send_log() {
        let base = base("lands");
        within(&base, || {
            tracing::info!(placed = 3, "a Viewer carrier took its turn");
            tracing::warn!("a Viewer carrier could not place what has moved");
        });
        let text = std::fs::read_to_string(file_at(&base)).unwrap();
        assert!(text.contains("placed=3"), "{text}");
        assert!(text.contains("could not place"), "{text}");
        assert!(file_at(&base).starts_with(base.join("logs")));
    }

    #[test]
    fn perf_and_debug_lines_are_left_out() {
        let base = base("left-out");
        within(&base, || {
            tracing::info!(target: "perf", "a measured span");
            tracing::debug!("a detail");
            tracing::info!("kept");
        });
        let text = std::fs::read_to_string(file_at(&base)).unwrap();
        assert!(!text.contains("a measured span"), "{text}");
        assert!(!text.contains("a detail"), "{text}");
        assert!(text.contains("kept"), "{text}");
    }

    #[test]
    fn nothing_is_written_outside_within() {
        let base = base("outside");
        within(&base, || {});
        tracing::info!("after");
        assert!(!file_at(&base).exists());
    }

    #[test]
    fn the_log_never_holds_more_than_its_generations() {
        let base = base("rotates");
        let dir = base.join(DIR_NAME);
        let line = vec![b'x'; 64 * 1024];
        // Enough to fill every generation several times over.
        for _ in 0..(CEILING_BYTES / line.len() as u64) * 3 {
            append(&dir, &line).unwrap();
        }
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["viewer-send.1.log", "viewer-send.2.log", "viewer-send.log"]);
        for name in &names {
            let len = std::fs::metadata(dir.join(name)).unwrap().len();
            assert!(len <= MAX_FILE_BYTES, "{name} is {len} bytes");
        }
    }
}
