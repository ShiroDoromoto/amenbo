//! Where throwaway files go: the unique suffix that names one, and the startup repair that makes sure
//! the directory they go *in* is still there.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Disown an inherited `TMPDIR` whose directory is gone, so this process and everything it starts fall
/// back to the OS default instead of a path that no longer exists.
///
/// **`TMPDIR` is inherited, and what it names need not outlive whoever exported it.** The macOS `.pkg`
/// installer runs its postinstall inside a sandbox (`/private/tmp/PKInstallSandbox.<x>/tmp`), and removes
/// that sandbox as soon as the install finishes. The postinstall's last act is to relaunch the freshly
/// installed app with `open`, and `open` hands it the environment it was called with — verified, and as
/// `open(1)` says: "opened applications inherit environment variables just as if you had launched the
/// application directly through its full path." So the app that greets a user right after an install is
/// pointed at a directory that was deleted seconds earlier, and so is every plugin it starts: the viewer's
/// "put the app on this device" came back `stat /private/tmp/PKInstallSandbox.<x>/tmp: no such file or
/// directory`, and from the user's side the button simply did nothing (`AMB-T-3461`).
///
/// Stripping the variable at the installer's end would close *that* path. This closes the class: whatever
/// hands us a dead `TMPDIR` — an installer, a scheduler, a shell that exported one and cleaned it up — the
/// first thing this process does is stop believing it.
///
/// **Call it at the very top of `main`, before any thread exists.** Removing a variable mutates the
/// process-wide environment, which is only sound while this is the only thread reading it.
///
/// Nothing happens where there is nothing wrong: an unset `TMPDIR`, or one whose directory is there, is
/// left exactly as it was. A path that exists but is not a directory counts as gone — `std::env::temp_dir`
/// would hand it out just the same, and every use of it would fail.
pub fn forget_if_gone() {
    let Some(dir) = crate::env::tmpdir() else { return };
    if std::path::Path::new(&dir).is_dir() {
        return;
    }
    std::env::remove_var(crate::env::TMPDIR_VAR);
}

/// A collision-free suffix for the name of a working directory under `std::env::temp_dir()`.
///
/// The only neighbours it can have are another process running at the same time, and another thread — or
/// another call — inside this one, so the triple of process id, monotonic counter and current time is
/// enough: the time alone collides between two calls in the same millisecond, and the counter alone
/// collides across processes.
///
/// This is **not an identifier** — a record's id is an INTEGER primary key. All this makes is a file name
/// nobody else will take.
pub(crate) fn suffix() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}-{:x}", std::process::id(), nanos, seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Back-to-back calls in one process always differ, even inside the same millisecond.
    #[test]
    fn consecutive_suffixes_are_distinct() {
        let s: std::collections::HashSet<String> = (0..64).map(|_| suffix()).collect();
        assert_eq!(s.len(), 64);
    }
}
