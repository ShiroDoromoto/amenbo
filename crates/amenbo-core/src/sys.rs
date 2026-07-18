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
