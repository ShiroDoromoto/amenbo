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
//! not startable, or misses what is. So both are spelled once, here: [`crate::launch::command`] is
//! the pane and [`crate::launch::installed`] is the probe — the same shell, the same flags, the same
//! environment floor — and there is no way to ask this question that goes around them.
//!
//! **Nothing here elevates.** On Windows an administrator process will not traverse a junction a
//! standard user made, which is where scoop keeps every one of its packages: run elevated and the
//! tools do not merely lose their `PATH` entry, they cannot be reached at all (`AMB-T-3565`). A
//! terminal is started with the token this process already has, and never through a mechanism that
//! asks for another one.

use std::ffi::OsString;
use std::io::Read as _;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;

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

/// Tell the program what terminal it is in, without overruling anything already said.
fn describe_terminal(cmd: &mut CommandBuilder) {
    for (key, value) in terminal_floor() {
        cmd.env(key, value);
    }
}

/// What a launch that arrived saying nothing is told about its terminal, as key and value.
///
/// A value inherited from the desktop session was chosen by whoever configured that session, and a
/// shell profile can overrule this in turn — these are a floor for a launch that arrived with
/// nothing, not a setting. `LC_ALL` counts as a locale being set, because it is the one that wins:
/// naming `LANG` beside it would be writing something that has no effect.
///
/// It is read off here rather than written twice because [`installed`] starts the same shell the
/// same way, and a probe running in a different environment than the pane is a probe that can only
/// be wrong — the invariant this whole module exists to hold.
fn terminal_floor() -> Vec<(&'static str, &'static str)> {
    let mut floor = Vec::new();
    if amenbo_core::env::term().is_none() {
        floor.push(("TERM", TERM));
    }
    #[cfg(unix)]
    if amenbo_core::env::locale().is_none() {
        floor.push(("LANG", LANG));
    }
    floor
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

/// How long a probe is given before it is taken to have hung.
///
/// A login shell reads the user's profile, and a profile is arbitrary code — one that blocks on a
/// network call or waits for something on a terminal that is not there would otherwise hold the pane
/// shut for good. What a timeout costs is the truthful answer for a machine that really is that
/// slow, which shows as "not installed" and a **search again** button; what it buys is that the
/// window always comes up.
const PROBE: Duration = Duration::from_secs(15);

/// The line a probe answers with, one per program it found.
const FOUND: &str = "amenbo-has ";

/// Which of `names` the pane's own shell can find on its `PATH`.
///
/// **One shell answers for all of them.** Starting a login-and-interactive shell means reading the
/// user's whole profile, which is the expensive part and is the same work whichever program is being
/// asked about — so the names go in as one script and come back as one list, rather than paying that
/// cost once per name.
///
/// Names are the catalog's ([`amenbo_core::harness::Harness::command`]), never a reader's, and are
/// held to that here as well: anything that is not a plain program name is dropped rather than
/// spliced into a shell script.
///
/// A probe that could not be started, or that ran past [`PROBE`], answers with what it had — for
/// this question an empty answer is "nothing found", which is a state the face already draws.
pub fn installed(names: &[&str]) -> Vec<String> {
    let names: Vec<&str> = names.iter().copied().filter(|n| plain(n)).collect();
    if names.is_empty() {
        return Vec::new();
    }
    let (program, login) = shell();
    let mut cmd = std::process::Command::new(program);
    cmd.args(login);
    cmd.arg(RUN);
    cmd.arg(script(&names));
    for (key, value) in terminal_floor() {
        cmd.env(key, value);
    }
    // Nothing is typed at it and nothing it complains about is an answer: a profile that greets the
    // user, or a shell grumbling that there is no terminal to take job control of, is noise on the
    // way to a list of program names.
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // A probe is not a terminal the user asked for; without this a console window flashes up.
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let Ok(mut child) = cmd.spawn() else {
        return Vec::new();
    };
    let text = child
        .stdout
        .take()
        .map(|mut out| {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let mut read = String::new();
                let _ = out.read_to_string(&mut read);
                let _ = tx.send(read);
            });
            rx.recv_timeout(PROBE).unwrap_or_default()
        })
        .unwrap_or_default();
    // A shell that has already exited is not killed by this, and one that ran past the deadline is —
    // either way the child is reaped here rather than left behind for the length of the session.
    let _ = child.kill();
    let _ = child.wait();

    text.lines()
        .filter_map(|line| line.trim().strip_prefix(FOUND))
        .map(str::to_string)
        .filter(|found| names.contains(&found.as_str()))
        .collect()
}

/// Whether a name is a plain program name — letters, digits, and the three separators the agents'
/// own commands are spelled with. The guard on what reaches the script.
fn plain(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The one command the probe shell is handed: ask after each name, and say the ones that answer.
#[cfg(unix)]
fn script(names: &[&str]) -> String {
    // `command -v` is the shell's own lookup rather than a separate program, so it is the same
    // answer the pane's shell would give when the user types the name.
    format!(
        "for n in {}; do command -v -- \"$n\" >/dev/null 2>&1 && printf '{FOUND}%s\\n' \"$n\"; done",
        names
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// The PowerShell form of [`script`]. `Get-Command` is this shell's own lookup, and it is asked
/// quietly because a name it does not know is not an error here.
#[cfg(windows)]
fn script(names: &[&str]) -> String {
    format!(
        "foreach ($n in {}) {{ if (Get-Command $n -ErrorAction SilentlyContinue) {{ '{FOUND}' + $n }} }}",
        names
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(",")
    )
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

    /// The probe is asked about the catalog's names, and answers with the ones this machine has —
    /// which on any machine includes the shell's own `command` builtin never being confused for a
    /// program that is not there. What is asserted is the shape of the answer rather than which
    /// tools happen to be installed on the machine running the test: it says only names it was
    /// asked about, and never one it was not.
    #[test]
    fn the_probe_answers_only_about_what_it_was_asked() {
        let found = installed(&["sh", "definitely-not-a-real-program-9x8"]);
        assert!(
            !found.iter().any(|f| f == "definitely-not-a-real-program-9x8"),
            "the probe claimed a program that is not there: {found:?}"
        );
        for one in &found {
            assert!(one == "sh", "the probe answered about something else: {found:?}");
        }
    }

    /// Nothing to ask about is answered without starting a shell at all — the expensive part of a
    /// probe is the profile, and there is no reason to read it to answer about nobody.
    #[test]
    fn an_empty_ask_starts_nothing() {
        assert_eq!(installed(&[]), Vec::<String>::new());
        assert_eq!(installed(&["not a name; rm -rf /"]), Vec::<String>::new());
    }

    /// Only a plain program name reaches the script. The names are the catalog's, so this is a
    /// guard on the shape rather than on a reader, and it is asserted because the day it stops
    /// holding is the day a catalog row becomes a shell injection.
    #[test]
    fn only_a_plain_program_name_reaches_the_script() {
        assert!(plain("claude"));
        assert!(plain("cursor-agent"));
        assert!(plain("some_tool.exe"));
        assert!(!plain(""));
        assert!(!plain("claude; rm -rf /"));
        assert!(!plain("$(whoami)"));
        assert!(!plain("a b"));
    }

    /// Every command the catalog lists is a name the probe will actually ask about — a row whose
    /// command this dropped would read as "not installed" on every machine, forever.
    #[test]
    fn every_catalogued_command_is_askable() {
        for harness in amenbo_core::harness::HARNESSES {
            assert!(plain(harness.command), "{} is not askable", harness.id);
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
