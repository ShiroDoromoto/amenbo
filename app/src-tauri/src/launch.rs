//! What a terminal is started as, on each of the three operating systems.
//!
//! A pane is a real terminal (`AMB-D-747`), and the program in it has to find the same tools the
//! person would find in their own terminal. That is not what an application started from a desktop
//! inherits. On macOS the environment a `.app` is launched with carries `PATH=/usr/bin:/bin:
//! /usr/sbin:/sbin` and nothing else — no Homebrew, no `~/.local/bin`, no version manager — so
//! `claude` is not there to be found (`AMB-T-3546`). On Linux the same thinness comes from
//! `/etc/environment` by way of the display manager, and under Wayland not even `~/.profile` is read
//! to widen it (`AMB-T-3566`). Windows is the exception: a process started from Explorer gets the
//! registry's `PATH` whole, and nothing has to be done to it (`AMB-T-3565`).
//!
//! **The fat path is the user's shell, so the shell is what is started.** A login *and* interactive
//! shell, both: `-l` alone reads the profile, and people put their `PATH` in `.zshrc` — dropping
//! `-i` is what makes a tool that is installed look like a tool that is not (`AMB-T-3546`).
//!
//! **Detecting a tool and starting it go through here, together.** A probe that resolves a command
//! against one environment while the terminal runs in another can only be wrong: it finds what is
//! not startable, or misses what is. So both are the same call — [`crate::launch::command`] with
//! something to run is the probe, the same with nothing is the pane, and there is no second way to
//! spell it.
//!
//! **Nothing here elevates.** On Windows an administrator process will not traverse a junction a
//! standard user made, which is where scoop keeps every one of its packages: run elevated and the
//! tools do not merely lose their `PATH` entry, they cannot be reached at all (`AMB-T-3565`). A
//! terminal is started with the token this process already has, and never through a mechanism that
//! asks for another one.
//!
//! **What cannot be refused is a token this process was already started with.** Elevation is
//! inherited, and there is no way to hand it back: an app the user started as administrator starts
//! its terminals as administrator too. Everything downstream still tells the truth — the probe and
//! the pane share this one route, so a tool behind a junction is reported missing and is missing —
//! but "missing" is not what happened, and the person who installed it has no way to tell those two
//! apart. So the state is reported instead of hidden: `elevated` below is what the surface asks, and
//! saying it is the whole of the remedy available here.

use std::ffi::OsString;
use std::path::PathBuf;

use portable_pty::CommandBuilder;

/// What the terminal calls itself to the program inside it. xterm.js draws the pane, and what it
/// implements is xterm's vocabulary with 256 colours in it, so this is a name a TUI can look up and
/// be right about. It has to be said out loud because a desktop launch carries no `TERM` at all, and
/// a program that finds none assumes a terminal that can do nothing.
const TERM: &str = "xterm-256color";

/// The flag that hands a shell one command to run instead of a prompt.
#[cfg(unix)]
const RUN: &str = "-c";
#[cfg(windows)]
const RUN: &str = "-Command";

/// The character set the terminal speaks, for a launch that arrived without one.
///
/// A desktop launch carries no `LANG`, and a program that finds none draws its box borders and its
/// symbols in ASCII or in mojibake. What is named is a locale that is **certain to exist**: macOS
/// ships `en_US.UTF-8` in its database, and glibc answers `C.UTF-8` without one being generated. The
/// language is not the point and is not being chosen here — the encoding is.
#[cfg(target_os = "macos")]
const LANG: &str = "en_US.UTF-8";
#[cfg(all(unix, not(target_os = "macos")))]
const LANG: &str = "C.UTF-8";

/// Build the command a terminal is started with: the user's own shell, with the environment a
/// terminal owes the program inside it, in `cwd` (the user's home, with none given).
///
/// With `run` given, the shell is handed that one command instead of a prompt — the same shell, the
/// same profile, the same `PATH`. That is the whole reason detection is spelled this way: what a
/// probe finds here is what the pane will be able to start.
pub fn command(cwd: Option<PathBuf>, run: Option<&str>) -> CommandBuilder {
    let (program, login) = shell();
    let mut cmd = CommandBuilder::new(program);
    cmd.args(login);
    if let Some(run) = run {
        cmd.arg(RUN);
        cmd.arg(run);
    }
    if let Some(dir) = cwd {
        cmd.cwd(dir);
    }
    describe_terminal(&mut cmd);
    cmd
}

/// Whether this process holds an administrator's token — which on Windows is the same thing as
/// saying the tools the user installed for themselves are unreachable from every terminal it opens.
///
/// The consequence is not a thinner `PATH`, which a person could reason about. scoop keeps each of
/// its packages behind a junction the standard user created, and an elevated process refuses to
/// traverse one at all: the entry is on the `PATH`, the folder is there, and opening it fails with
/// an untrusted mount point (`AMB-T-3565`). The terminal opens, the probe agrees nothing is
/// installed, and the person who installed it reads a broken Amenbo.
///
/// Named for the fact rather than for the consequence, because the fact is the part that is
/// certain: what is measured is this process's token, not whether any particular tool can be
/// reached. Drawing the conclusion is the surface's, and so is the way out — there is none from in
/// here, because a token cannot be given back.
#[tauri::command]
pub fn elevated() -> bool {
    holds_an_administrator_token()
}

/// The Windows answer: the elevation flag on this process's own access token.
///
/// A token is asked rather than a group membership: an administrator who has not been elevated is
/// still a member of the administrators group, and answering from membership would warn every one
/// of them about a state they are not in. `TokenElevation` is what UAC actually split.
///
/// A token that cannot be opened or read is reported as not elevated. This decides whether a
/// warning is shown, so the failure that costs least is the silent one: a machine that will not
/// answer is overwhelmingly a machine running as the ordinary user, and a warning shown to somebody
/// whose tools work is a warning they learn to dismiss.
#[cfg(windows)]
fn holds_an_administrator_token() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: the pseudo-handle `GetCurrentProcess` returns needs no closing and is valid for the
    // call, and `token` is owned here and outlives it. On success it holds a real handle, closed
    // below on both paths out.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut written = 0u32;
    // SAFETY: `token` was opened with `TOKEN_QUERY`, which is the right the class needs, and the
    // buffer handed over is a `TOKEN_ELEVATION` given with its own size — the class writes exactly
    // that struct, so the length cannot be short.
    let read = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            std::ptr::from_mut(&mut elevation).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut written,
        )
    };
    // SAFETY: `token` came from a successful `OpenProcessToken` and is not used again.
    unsafe { CloseHandle(token) };
    read != 0 && elevation.TokenIsElevated != 0
}

/// Every other operating system, where the question does not arise.
///
/// What is being reported is not "running with more rights than usual" in general — it is one
/// Windows behaviour, an elevated process refusing a standard user's junction, and the tool layout
/// that walks into it. macOS and Linux have no junctions and no scoop, and an app the user launched
/// from their desktop is running as them. Answering `true` anywhere here would put a warning about
/// Windows in front of somebody who is not on Windows.
#[cfg(not(windows))]
fn holds_an_administrator_token() -> bool {
    false
}

/// Tell the program what terminal it is in, without overruling anything already said.
///
/// A value inherited from the desktop session was chosen by whoever configured that session, and a
/// shell profile can overrule this in turn — these are a floor for a launch that arrived with
/// nothing, not a setting. `LC_ALL` counts as a locale being set, because it is the one that wins:
/// naming `LANG` beside it would be writing something that has no effect.
fn describe_terminal(cmd: &mut CommandBuilder) {
    if amenbo_core::env::term().is_none() {
        cmd.env("TERM", TERM);
    }
    #[cfg(unix)]
    if amenbo_core::env::locale().is_none() {
        cmd.env("LANG", LANG);
    }
}

/// The program a terminal starts, and the arguments that make it the shell the user signed in with.
#[cfg(unix)]
fn shell() -> (OsString, Vec<OsString>) {
    (login_shell(), vec!["-l".into(), "-i".into()])
}

/// The user's login shell, read from the account database rather than from `SHELL`.
///
/// `SHELL` describes the shell of whatever session set it, which for a desktop launch is the
/// session's, not a choice the user made — and a process several launches deep can be carrying one
/// that was inherited from something else entirely. The account database is where the user's answer
/// actually lives. `SHELL` is still the fallback, because it is right far more often than
/// `/bin/sh` is.
#[cfg(unix)]
fn login_shell() -> OsString {
    passwd_shell()
        .or_else(amenbo_core::env::shell)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| OsString::from("/bin/sh"))
}

/// This user's `pw_shell`, or nothing if the account database cannot answer.
///
/// The reentrant form is used because this runs on whichever thread opened the pane; the buffer it
/// fills is sized once and a shell path that would not fit in four kilobytes is treated as no
/// answer, which lands on the same fallback as a user with no entry at all.
#[cfg(unix)]
fn passwd_shell() -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt as _;

    let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    // SAFETY: `passwd` and `found` are owned here and outlive the call, and `buf` is handed over
    // with its own length. On success `found` points at `passwd`, whose `pw_shell` points into
    // `buf` — which is why the bytes are copied out before either goes out of scope.
    let rc = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut passwd,
            buf.as_mut_ptr(),
            buf.len(),
            &mut found,
        )
    };
    if rc != 0 || found.is_null() || passwd.pw_shell.is_null() {
        return None;
    }
    // SAFETY: `pw_shell` is a NUL-terminated string inside `buf`, which is still alive here.
    let shell = unsafe { std::ffi::CStr::from_ptr(passwd.pw_shell) };
    Some(OsString::from_vec(shell.to_bytes().to_vec()))
}

/// The program a terminal starts on Windows, where there is no login shell to ask about.
///
/// Windows has no `pw_shell` and no environment variable naming the user's shell, so the choice is
/// made here: PowerShell 7 where it is installed, and Windows PowerShell — on every Windows since
/// 7 — where it is not. `cmd.exe` is not reached for, because what [`command`] hands a shell is a
/// command line, and cmd's quoting is not the one the rest of this route speaks.
///
/// No login flag is passed, and none is wanted: the `PATH` is already whole, and there is nothing
/// for a profile to widen (`AMB-T-3565`).
#[cfg(windows)]
fn shell() -> (OsString, Vec<OsString>) {
    let program = on_path("pwsh.exe").unwrap_or_else(|| OsString::from("powershell.exe"));
    (program, vec!["-NoLogo".into()])
}

/// Where `name` sits on the `PATH`, if it sits on it at all.
#[cfg(windows)]
fn on_path(name: &str) -> Option<OsString> {
    let path = amenbo_core::env::path()?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .map(OsString::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arguments a shell is handed, as strings, for asserting on.
    fn argv(cmd: &CommandBuilder) -> Vec<String> {
        cmd.get_argv()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// A pane gets a login **and** interactive shell. Both halves are asserted because dropping
    /// either is a silent failure rather than a loud one: the terminal still opens, and the tools
    /// the user keeps on the half that went unread are simply not there.
    #[cfg(unix)]
    #[test]
    fn the_pane_gets_a_login_and_interactive_shell() {
        let args = argv(&command(None, None));
        assert!(!args[0].is_empty(), "no shell was named: {args:?}");
        assert!(args.contains(&"-l".to_string()), "not a login shell: {args:?}");
        assert!(args.contains(&"-i".to_string()), "not interactive: {args:?}");
    }

    /// Detection is the same launch with something to run, which is the point of the whole module:
    /// the shell and its flags are identical, and only the command is added on the end.
    #[test]
    fn detecting_a_tool_is_the_same_launch_with_a_command_on_the_end() {
        let pane = argv(&command(None, None));
        let probe = argv(&command(None, Some("command -v claude")));

        assert_eq!(probe[..pane.len()], pane[..], "a probe took a different shell");
        assert_eq!(
            probe[pane.len()..],
            [RUN.to_string(), "command -v claude".to_string()],
            "the command was not handed over whole"
        );
    }

    /// The terminal says what it is only when the launch arrived without an answer. Which branch is
    /// taken depends on the environment the tests are run in, so both are asserted against it
    /// rather than against a fixed expectation — what must not happen is overwriting a value the
    /// session already chose.
    ///
    /// What is read back is the **added** environment alone. A builder starts out holding the whole
    /// of this process's environment, so asking it for a value would answer with what was inherited
    /// and never show whether anything was written over it.
    #[test]
    fn what_the_terminal_says_about_itself_never_overrules_the_session() {
        let cmd = command(None, None);
        let added: Vec<(&str, &str)> = cmd.iter_extra_env_as_str().collect();
        let set = |key: &str| {
            added
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        };

        match amenbo_core::env::term() {
            Some(_) => assert_eq!(set("TERM"), None, "the session's TERM was overwritten"),
            None => assert_eq!(set("TERM"), Some(TERM.to_string()), "TERM went unsaid"),
        }
        #[cfg(unix)]
        match amenbo_core::env::locale() {
            Some(_) => assert_eq!(set("LANG"), None, "the session's locale was overwritten"),
            None => assert_eq!(set("LANG"), Some(LANG.to_string()), "the locale went unsaid"),
        }
    }

    /// The question elevation answers is Windows's alone, so everywhere else the answer is the
    /// constant that keeps a warning about junctions off a screen that has none to warn about.
    #[cfg(not(windows))]
    #[test]
    fn nothing_outside_windows_is_reported_as_elevated() {
        assert!(!elevated(), "a warning about Windows junctions was raised off Windows");
    }

    /// On Windows the answer is read from the operating system, so what can be asserted is that
    /// reading it is repeatable: the token is opened and closed on every call, and an answer that
    /// changed between two of them would mean the handle path, not the machine, decided it.
    #[cfg(windows)]
    #[test]
    fn the_same_process_gets_the_same_answer_every_time() {
        let first = elevated();
        for _ in 0..64 {
            assert_eq!(elevated(), first, "the answer moved without the process changing");
        }
    }

    /// The folder is the pane's, and with none named the shell starts wherever it would have.
    #[test]
    fn the_shell_starts_in_the_folder_it_was_given() {
        assert_eq!(command(None, None).get_cwd(), None);
        assert_eq!(
            command(Some(PathBuf::from("/tmp")), None).get_cwd(),
            Some(&OsString::from("/tmp"))
        );
    }
}
