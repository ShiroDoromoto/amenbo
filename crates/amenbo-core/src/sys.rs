//! Small OS-level helpers shared across the crates.

use std::ffi::OsStr;
use std::process::Command;

/// A [`Command`] that never flashes a console window on Windows.
///
/// A GUI process has no console of its own, so when it spawns a console program — git, powershell, cmd —
/// Windows gives the child a fresh console, which blinks on screen and vanishes. amenbo probes git on
/// nearly every action (the hooks are looked up per command), so without this a window flickers on almost
/// every interaction. `CREATE_NO_WINDOW` tells Windows to give the child no console at all; a child that
/// only reads/writes pipes (which is all of ours) needs none. On every other OS this is a plain
/// [`Command::new`] — the flag does not exist and nothing flashes there.
///
/// Use this in place of [`Command::new`] for every subprocess amenbo runs to do its own work. The one
/// exception is deliberately launching a *visible* window (opening a terminal): there the flag still helps,
/// because it hides only the throwaway launcher (`cmd /C start …`) while the window `start` opens is the
/// child's own and appears as intended.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — the child runs with no console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Keep a write to a plugin's stdin from ending amenbo with `SIGPIPE` when the plugin closes that pipe
/// early — the Linux half of the guard. Call it on the thread that does the write.
///
/// The CLI hands `SIGPIPE` back to the kernel at startup (`restore_sigpipe`) so `amenbo … | head` ends
/// cleanly when the reader walks away — a process-wide disposition, since `SIGPIPE`'s is not per-fd.
/// But the thread that writes an event payload into a plugin's stdin must survive the plugin closing
/// that pipe early: a fire-and-forget hook that never reads stdin is a legitimate hook (`AMB-D-352` —
/// a hook failing changes nothing), and with the default disposition that write would kill the whole
/// process instead.
///
/// On Linux a `SIGPIPE` from `write()` is directed at the thread that wrote, so blocking it there turns
/// the write into an `EPIPE` the caller can drop; the pending signal is thread-private and discarded when
/// this short-lived writer thread exits. macOS does **not** confine it that way — a blocked `SIGPIPE` is
/// taken by another thread that is not blocking it — so the per-fd [`suppress_child_stdin_sigpipe`] is what
/// covers macOS, and this and it are applied together.
///
/// A no-op where there is no `SIGPIPE` (Windows), where a closed pipe is already an ordinary write error.
#[cfg(unix)]
pub fn block_sigpipe_on_current_thread() {
    // SAFETY: `pthread_sigmask`, `sigemptyset` and `sigaddset` are async-signal-safe and used here
    // exactly as documented — add `SIGPIPE` to this thread's block mask. The old mask is not read back:
    // this runs on a short-lived writer thread that exits after one write, taking any pending `SIGPIPE`
    // with it, so there is nothing to restore.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGPIPE);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
}

/// A no-op on a platform without `SIGPIPE`: a closed pipe surfaces as an ordinary write error there.
#[cfg(not(unix))]
pub fn block_sigpipe_on_current_thread() {}

/// Tell the kernel that writes to a child's stdin pipe must never raise `SIGPIPE` — they return `EPIPE`
/// instead — the macOS half of the guard, set on the fd itself (`F_SETNOSIGPIPE`).
///
/// macOS needs this because it does not confine a `write()`'s `SIGPIPE` to the writing thread the way
/// Linux does: [`block_sigpipe_on_current_thread`] alone leaves another thread free to take the signal and
/// end the process. Setting it on the fd removes the signal at the source, for every thread, without
/// touching the process-wide disposition the CLI relies on for its own stdout.
///
/// Only macOS has `F_SETNOSIGPIPE` (it is a BSD `fcntl`); on Linux there is no per-fd equivalent for a
/// pipe, and the thread block is the guard instead, so this is a no-op there. A failure is ignored: the
/// worst case is that writes keep their default `SIGPIPE` behaviour, which the thread block still guards.
#[cfg(target_os = "macos")]
pub fn suppress_child_stdin_sigpipe(stdin: &std::process::ChildStdin) {
    use std::os::unix::io::AsRawFd;
    // <sys/fcntl.h>: `#define F_SETNOSIGPIPE 73`. Not yet surfaced by the libc crate, so it is named here.
    const F_SETNOSIGPIPE: libc::c_int = 73;
    // SAFETY: `fcntl` with a live fd (owned by `stdin`, which outlives this call) and a valid command.
    unsafe {
        libc::fcntl(stdin.as_raw_fd(), F_SETNOSIGPIPE, 1);
    }
}

/// A no-op off macOS: Linux has no per-fd `SIGPIPE` suppression for a pipe (the thread block guards it
/// there), and Windows has no `SIGPIPE` at all.
#[cfg(not(target_os = "macos"))]
pub fn suppress_child_stdin_sigpipe(_stdin: &std::process::ChildStdin) {}
