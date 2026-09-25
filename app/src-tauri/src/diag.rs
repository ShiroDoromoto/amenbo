//! The GUI's diagnostic log — what `log::warn!` and its siblings leave behind on the machine the app
//! actually runs on. It is registered in **every** build (`AMB-D-382`): an incident on a user's machine
//! has to leave a trace, and asking them to reproduce it under a switch only catches the second one.
//!
//! Two properties make that safe to have on by default:
//!
//! - **It holds diagnostics, never content.** Failure reasons, counts, state transitions, ids. No task
//!   or decision body, title, comment, attachment name or secret ever goes into a `log::*!` here — if a
//!   line needs one to be useful, it does not belong in this file.
//! - **It cannot grow without bound.** The file rotates at [`MAX_FILE_BYTES`] and only
//!   [`ARCHIVED_GENERATIONS`] rotated copies survive beside it, so the whole log is capped at
//!   [`CEILING_BYTES`] — a few MiB, whatever the app does. Pruning runs both when the file is opened
//!   and at every rotation, so a long-running app and a restarted one settle at the same ceiling.
//!
//! It sits in [`logs_dir`] next to `perf.log`, so a diagnosis is one folder to ask for. The two are not
//! the same thing: perf measures (off by default, `target="perf"` only — see [`crate::perf`]), this one
//! explains. They also own different globals — `tracing` is the perf subscriber's, the `log` facade is
//! this plugin's.

use std::any::Any;
use std::panic::Location;
use std::path::PathBuf;

use tauri_plugin_log::{Builder, RotationStrategy, Target, TargetKind};

/// The active file, written as `<stem>.log`; rotated generations carry a timestamp after the stem.
const FILE_STEM: &str = "amenbo";

/// How large one generation may get before it rotates. The plugin's own default (40 KB, one file) is
/// too small to hold the run that led up to a problem.
const MAX_FILE_BYTES: u128 = 1024 * 1024;

/// How many rotated generations are kept **besides** the file being written. The plugin prunes to this
/// count on every rotation and again when it opens the file, so the log settles at this many archives
/// plus one active file — see [`CEILING_BYTES`] for what that costs.
const ARCHIVED_GENERATIONS: usize = 2;

/// The most the whole diagnostic log can occupy: every archive at its rotation size, plus the active
/// file. This is the number that has to stay small for the log to be on by default.
const CEILING_BYTES: u128 = MAX_FILE_BYTES * (ARCHIVED_GENERATIONS as u128 + 1);

/// The ceiling is what lets this log be on by default, so it is checked where it cannot be skipped:
/// raise either number past a few MiB and the crate stops compiling. Doing that means changing what
/// `AMB-D-382` promised the user's disk, which is a decision to take again rather than a constant to
/// bump.
///
/// The `+ 1` above is the file being written. `KeepSome(n)` counts the archives it keeps and excludes
/// the active file, so reading `n` as "files on disk" undercounts by one — a rotation run against the
/// real plugin left 3 archives behind `KeepSome(3)`.
const _: () = assert!(CEILING_BYTES <= 4 * 1024 * 1024);

/// The folder both logs live in: `logs/`, directly under the user-level app-data dir. `None` when that
/// root cannot be resolved at all — callers decide what a missing folder means for them, rather than
/// having a relative path invented for them (a bundled app's working directory is not writable).
pub(crate) fn logs_dir() -> Option<PathBuf> {
    amenbo_core::config::Paths::resolve().ok().map(|p| p.base_dir.join("logs"))
}

/// The diagnostic logger, configured per `AMB-D-382`. Falls back to the platform's own log directory
/// when [`logs_dir`] cannot be resolved: a log somewhere beats no log at all, and both are inside a
/// directory the app owns.
///
/// stdout is a debug-only target — a bundled app has no terminal attached to print to.
pub(crate) fn logger() -> Builder {
    let file = match logs_dir() {
        Some(path) => TargetKind::Folder { path, file_name: Some(FILE_STEM.to_string()) },
        None => TargetKind::LogDir { file_name: Some(FILE_STEM.to_string()) },
    };
    let mut builder = Builder::default()
        .clear_targets()
        .target(Target::new(file))
        .level(log::LevelFilter::Info)
        .max_file_size(MAX_FILE_BYTES)
        .rotation_strategy(RotationStrategy::KeepSome(ARCHIVED_GENERATIONS));
    if cfg!(debug_assertions) {
        builder = builder.target(Target::new(TargetKind::Stdout));
    }
    builder
}

/// Writes every panic into this log before the process goes on to unwind or abort. A release build on
/// Windows has no console (`windows_subsystem = "windows"`), so the message the default hook prints to
/// stderr is lost — and a panic inside a window procedure or a WebView2 callback cannot unwind, so the
/// process aborts (`0xc0000409`, fast-fail 7) with nothing on disk to say where it happened. This line
/// is what is left.
///
/// The hook it replaces still runs afterwards, so a debug build keeps its stderr message and backtrace.
/// Each record is flushed as it is written (fern flushes a writer after every record, and the plugin's
/// rotating file writes through on flush), and the explicit flush here keeps that true should the
/// target ever buffer — the abort that follows a panic runs no destructors.
///
/// Installed after the logger is registered: a panic earlier than that has no file to go to.
pub(crate) fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("{}", panic_line(info.location(), info.payload()));
        log::logger().flush();
        previous(info);
    }));
}

/// One line for a panic: which thread, where, and what it said. It takes the hook's parts rather than
/// the hook's argument, whose type name (`PanicHookInfo`) is newer than this crate's `rust-version`.
///
/// The message goes in whole. The panics most likely to carry a user's text are the standard library's
/// string-slicing ones, and those give indices and a length, not the string (`tests` holds that).
/// Messages written in this crate follow the rule at the top of this module.
fn panic_line(location: Option<&Location<'_>>, payload: &(dyn Any + Send)) -> String {
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("<unnamed>");
    let location = location
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string payload>");
    format!("panic in thread '{thread}' at {location}: {message}")
}

#[cfg(test)]
mod tests {
    use super::panic_line;

    /// What a slicing panic says about `s`, through the same formatting the hook uses.
    fn line_for_slicing(s: &str, end: usize) -> String {
        let payload = std::panic::catch_unwind(|| s[..end].len()).unwrap_err();
        panic_line(None, payload.as_ref())
    }

    // Older toolchains quoted the sliced string in these messages, and that string can be a task's
    // title. If a toolchain bump brings the quote back, these fail before the hook writes it to disk.
    #[test]
    fn a_char_boundary_panic_does_not_carry_the_string() {
        let line = line_for_slicing("über secret", 1);
        assert!(!line.contains("secret"), "{line}");
        assert!(line.contains("byte index 1 is not a char boundary"), "{line}");
    }

    #[test]
    fn an_out_of_bounds_panic_does_not_carry_the_string() {
        let line = line_for_slicing("secret", 99);
        assert!(!line.contains("secret"), "{line}");
        assert!(line.contains("byte index 99"), "{line}");
    }

    #[test]
    fn a_str_payload_is_written_as_it_is() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("called `Option::unwrap()` on a `None` value");
        assert_eq!(
            panic_line(None, payload.as_ref()),
            format!(
                "panic in thread '{}' at <unknown>: called `Option::unwrap()` on a `None` value",
                std::thread::current().name().unwrap_or("<unnamed>")
            )
        );
    }
}
