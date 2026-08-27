//! Small OS-level helpers shared across the crates.

use std::ffi::OsStr;
use std::process::Command;

/// A [`Command`] that never flashes a console window on Windows.
///
/// A GUI process has no console of its own, so when it spawns a console program — git, powershell, cmd —
/// Windows gives the child a fresh console, which blinks on screen and vanishes. Amenbo probes git on
/// nearly every action (the hooks are looked up per command), so without this a window flickers on almost
/// every interaction. `CREATE_NO_WINDOW` tells Windows to give the child no console at all; a child that
/// only reads/writes pipes (which is all of ours) needs none. On every other OS this is a plain
/// [`Command::new`] — the flag does not exist and nothing flashes there.
///
/// Use this in place of [`Command::new`] for every subprocess Amenbo runs to do its own work. The one
/// exception is deliberately launching a *visible* window (opening a terminal): there the flag still helps,
/// because it hides only the throwaway launcher (`cmd /C start …`) while the window `start` opens is the
/// child's own and appears as intended. For git in particular, reach for [`git`] rather than naming it
/// here: on macOS a bare `"git"` can be a stub that asks the user to install a compiler.
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

/// Keep a write to a plugin's stdin from ending Amenbo with `SIGPIPE` when the plugin closes that pipe
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

/// The git to run, or `None` when this machine has none that can be run without asking the user to install
/// something. Every call Amenbo makes to git goes through this rather than through [`command`] with a bare
/// `"git"`, because on macOS the difference between the two is a dialog on the user's screen.
///
/// `/usr/bin/git` is not git: it is one of the xcrun stubs (it shares an inode with `/usr/bin/clang` and
/// `xcodebuild`), and when the Command Line Tools are not installed it answers *any* invocation by asking
/// the OS to install them — **before** it has read a single argument. The dialog that opens says «The "git"
/// command requires the command line developer tools», so from the user's side Amenbo demanded a compiler
/// on startup; Cancel does not settle it, since the next call raises a fresh one. And a `.app` walks
/// straight into it: launched from Finder its `PATH` is `/usr/bin:/bin:/usr/sbin:/sbin`, where the stub is
/// the only git there is, so the hook sweep that runs over the bound folders at startup fires it every
/// time (`AMB-T-3751`).
///
/// So the path is resolved once for the life of the process, and the stub is only ever run once something
/// that is *not* a stub has said the tools are really there:
///
/// 1. Look for `git` along `PATH` — reading the directories, not running anything. A Homebrew or MacPorts
///    git found here is the answer, and the rest of this never happens.
/// 2. If what `PATH` yields is `/usr/bin/git`, ask `xcode-select -p` whether there is an active developer
///    directory. `xcode-select` is a real program rather than a stub, so it fails *silently* when there is
///    none — which is exactly the answer needed, obtained without a dialog.
/// 3. Otherwise ask the user's login shell for an absolute path (`command -v git`). This is what
///    reaches a Homebrew git from inside a `.app`, whose `PATH` cannot see `/opt/homebrew/bin`. It costs a
///    shell startup (~40ms measured), which is why it is the fallback and not the opening move: a terminal
///    with a working git on `PATH` never pays it. `command -v` is a shell builtin and does not exec, so it
///    reads the stub's *name* without touching it — `git --version` here would raise the very dialog this
///    is avoiding.
/// 4. Nothing left: answer `None`, and never spawn git at all. Every caller already has a "git said
///    nothing" path, since a folder that is not a repository takes it too.
///
/// Off macOS this is [`command`]`("git")` unchanged. The stub mechanism is Apple's alone — on Windows and
/// Linux a missing git is a plain "not found" that the callers have always handled (`AMB-T-3748` measured
/// all three), so there is nothing to resolve and no reason to pay for resolving it.
#[cfg(target_os = "macos")]
pub fn git() -> Option<Command> {
    git_path::resolved().map(command)
}

/// Off macOS there is no stub to step around: let the OS resolve `git` on `PATH` at spawn time, as before.
#[cfg(not(target_os = "macos"))]
pub fn git() -> Option<Command> {
    Some(command("git"))
}

/// Where git is on this Mac — see [`git`] for why this is a question at all.
#[cfg(target_os = "macos")]
mod git_path {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::sync::OnceLock;

    /// The xcrun stub. A path equal to this one is the trap [`super::git`] describes; any other path is a
    /// real git binary and needs no further questions.
    const STUB: &str = "/usr/bin/git";

    /// The fence that separates the login shell's own chatter from the answer we asked it for — see
    /// [`from_login_shell`]. Any word does, so long as no profile would print it by itself.
    const MARK: &str = "--amenbo-git--";

    /// The resolved git, decided on the first call and kept for the life of the process. Held rather than
    /// re-derived because the login-shell fallback costs a shell startup, and because the answer is about
    /// the machine, which does not change under a running app. Install the tools mid-session and Amenbo
    /// finds git at the next launch — the same restart the `PATH` change would need anyway.
    pub fn resolved() -> Option<&'static Path> {
        static GIT: OnceLock<Option<PathBuf>> = OnceLock::new();
        GIT.get_or_init(|| {
            let path = crate::env::path();
            let shell = crate::env::shell().unwrap_or_else(|| "/bin/sh".into());
            choose(on_path(path.as_deref()), developer_tools_present, || from_login_shell(&shell))
        })
        .as_deref()
    }

    /// The judgement itself, with its three inputs handed in so it can be tested without a Mac in either
    /// state. The order is the point: `developer_tools` is not asked at all when `PATH` already yields a
    /// real git, and the login shell is not started unless nothing usable was found without it.
    fn choose(
        on_path: Option<PathBuf>,
        developer_tools: impl Fn() -> bool,
        from_login_shell: impl Fn() -> Option<PathBuf>,
    ) -> Option<PathBuf> {
        let mut tools: Option<bool> = None;
        let mut usable = |p: PathBuf| -> Option<PathBuf> {
            if p != Path::new(STUB) {
                return Some(p);
            }
            (*tools.get_or_insert_with(&developer_tools)).then_some(p)
        };
        if let Some(git) = on_path.and_then(&mut usable) {
            return Some(git);
        }
        from_login_shell().and_then(usable)
    }

    /// The first executable `git` along `PATH`, found by reading directories rather than by spawning
    /// anything — the whole point being to learn *which* git would run before running it. Only an absolute
    /// answer is taken: an empty `PATH` entry means the current directory, and a git found relative to
    /// wherever the process happens to stand is not a machine-wide fact.
    fn on_path(path: Option<&OsStr>) -> Option<PathBuf> {
        std::env::split_paths(path?).map(|dir| dir.join("git")).find(|p| p.is_absolute() && is_executable(p))
    }

    /// Is there a file here that could be exec'd? `metadata` follows symlinks, so a shim pointing at the
    /// real binary answers for the binary, which is what `PATH` lookup would do too.
    fn is_executable(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }

    /// Does this Mac have an active developer directory — that is, would `/usr/bin/git` forward to a real
    /// git instead of asking for one? Asked of `xcode-select`, which is the tool that owns the answer and,
    /// unlike the stubs, is a program in its own right: with no tools installed it exits non-zero and
    /// prints to stderr, opening nothing (`AMB-T-3748` confirmed this on a Mac with the tools removed).
    /// Cached because the same question is asked of `PATH`'s answer and of the login shell's.
    fn developer_tools_present() -> bool {
        static PRESENT: OnceLock<bool> = OnceLock::new();
        *PRESENT.get_or_init(|| {
            super::command("/usr/bin/xcode-select")
                .arg("-p")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
    }

    /// Ask the user's login shell where git is, for the case a thin `PATH` cannot answer: a `.app` started
    /// from Finder sees only `/usr/bin:/bin:/usr/sbin:/sbin`, while the shell reads the profile that puts
    /// `/opt/homebrew/bin` in front.
    ///
    /// `-l -i` so the files that set `PATH` are actually read — `.zshrc` and `.bashrc` are where a Homebrew
    /// `PATH` usually lives, and only an interactive shell reads them. `command -v` because it is a
    /// builtin: it reports the name without exec'ing it, which is the only way to ask about the stub
    /// without setting it off — `git --version` here would raise the dialog. stderr is discarded rather
    /// than parsed (fish writes terminal warnings there on every non-tty start), and stdin is closed so an
    /// interactive shell has nothing to wait for.
    ///
    /// The answer is fenced behind [`MARK`] because a login shell's stdout is not ours: a profile that
    /// greets, or prints the day's message, writes there first, and the first line would be that. Echoing a
    /// marker and reading the line after it is the one shape all four shells a Mac carries — sh, bash, zsh,
    /// fish — run identically; fish has no `VAR=$(…)`, so the usual way of tagging the answer is out.
    ///
    /// There is no time limit on the shell: a profile that hangs hangs this. That is the user's own shell
    /// hanging — their terminal does not open either — and it is reached only on a Mac whose `PATH` holds
    /// no usable git, so the case where it costs anything is already the case where nothing works.
    fn from_login_shell(shell: &OsStr) -> Option<PathBuf> {
        let out = super::command(shell)
            .args(["-l", "-i", "-c", &format!("echo {MARK}; command -v git")])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut lines = stdout.lines().map(str::trim);
        lines.find(|l| *l == MARK)?;
        let path = PathBuf::from(lines.next()?);
        (path.is_absolute() && is_executable(&path)).then_some(path)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::cell::Cell;

        fn brew() -> PathBuf {
            PathBuf::from("/opt/homebrew/bin/git")
        }

        /// A real git on `PATH` settles it — and settles it *without* spawning `xcode-select`. The count is
        /// the cost guard: this path runs on every CLI command, so a question asked here is a process
        /// started on every command.
        #[test]
        fn a_real_git_on_path_is_taken_without_asking_xcode_select() {
            let asked = Cell::new(0);
            let picked = choose(Some(brew()), || {
                asked.set(asked.get() + 1);
                true
            }, || panic!("the login shell must not be started when PATH already answers"));
            assert_eq!(picked, Some(brew()));
            assert_eq!(asked.get(), 0, "a path that is not the stub raises no question");
        }

        /// The stub is fine to run when the tools behind it are installed — that is the ordinary Mac, where
        /// `/usr/bin/git` is the only git and works.
        #[test]
        fn the_stub_is_taken_when_the_developer_tools_are_installed() {
            let picked =
                choose(Some(PathBuf::from(STUB)), || true, || panic!("a usable git needs no fallback"));
            assert_eq!(picked, Some(PathBuf::from(STUB)));
        }

        /// The bug: the tools are gone, so the stub would open the install dialog. The login shell finds the
        /// Homebrew git a `.app`'s `PATH` cannot see, and that is what gets run.
        #[test]
        fn without_the_tools_the_login_shell_is_what_finds_a_real_git() {
            let picked = choose(Some(PathBuf::from(STUB)), || false, || Some(brew()));
            assert_eq!(picked, Some(brew()));
        }

        /// And when the shell has nothing better to offer, the answer is "no git" rather than "run the stub
        /// and see" — the one outcome that keeps the dialog off the screen.
        #[test]
        fn a_stub_with_no_tools_and_no_alternative_is_no_git_at_all() {
            assert_eq!(choose(Some(PathBuf::from(STUB)), || false, || Some(PathBuf::from(STUB))), None);
            assert_eq!(choose(Some(PathBuf::from(STUB)), || false, || None), None);
            assert_eq!(choose(None, || false, || None), None);
        }

        /// A `PATH` with no git at all still reaches the shell — this is the `.app` whose `PATH` was
        /// trimmed to nothing useful, not a machine without git.
        #[test]
        fn an_empty_path_falls_through_to_the_login_shell() {
            assert_eq!(choose(None, || panic!("nothing asked about a stub that was never found"), || Some(brew())), Some(brew()));
        }

        /// A directory holding an executable file called `git`, and one holding a file of that name that
        /// nothing could run. What separates them is the only thing `PATH` lookup judges.
        fn dir_with_git(mode: u32) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let dir = amenbo_scratch::scratch("sys-git");
            std::fs::write(dir.join("git"), "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(dir.join("git"), std::fs::Permissions::from_mode(mode)).unwrap();
            dir
        }

        /// `PATH` is read in order and the first runnable `git` wins, exactly as a spawn would resolve it —
        /// which is the whole claim this lookup has to make good on, since it decides *before* the spawn.
        #[test]
        fn path_is_read_in_order_and_only_a_runnable_file_counts() {
            let runnable = dir_with_git(0o755);
            let unrunnable = dir_with_git(0o644);
            let empty = amenbo_scratch::scratch("sys-git-empty");

            let joined = |dirs: &[&Path]| std::env::join_paths(dirs).unwrap();
            assert_eq!(
                on_path(Some(&joined(&[&unrunnable, &runnable]))),
                Some(runnable.join("git")),
                "a file that cannot be exec'd is stepped over, not taken"
            );
            assert_eq!(on_path(Some(&joined(&[&empty, &unrunnable]))), None);
            assert_eq!(on_path(None), None, "no PATH at all is no answer, not a panic");
        }

        /// An empty `PATH` entry names the current directory, and a git found there is not an answer: where
        /// git is has to be a fact about the machine, not about where the process happens to stand. The
        /// entry is skipped without so much as a `stat`, since the path it would build is not absolute.
        #[test]
        fn a_git_found_relative_to_the_current_directory_is_not_taken() {
            let runnable = dir_with_git(0o755);
            let path = std::env::join_paths([Path::new(""), &runnable]).unwrap();
            assert_eq!(on_path(Some(&path)), Some(runnable.join("git")), "the relative entry is passed over");
            assert_eq!(on_path(Some(OsStr::new(""))), None, "and on its own it answers nothing");
        }

        /// A stand-in for the user's shell. It ignores the flags and runs the script it was handed, the way
        /// a real one would, but with `command` replaced by `answer` — so a test says what `command -v git`
        /// found without depending on the machine. It also greets on stdout and warns on stderr, which is
        /// what a real profile and a real fish do, and neither may reach the answer.
        fn shell_answering(answer: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let shell = amenbo_scratch::scratch("sys-git-shell").join("shell");
            let script = format!(
                "#!/bin/sh\n\
                 command() {{ {answer}; }}\n\
                 echo 'Welcome back!'\n\
                 echo 'Could not set up terminal' >&2\n\
                 shift 3\n\
                 eval \"$1\"\n"
            );
            std::fs::write(&shell, script).unwrap();
            std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
            shell
        }

        /// The fallback takes the shell's answer when it names a git that is really there — this is the
        /// Homebrew git a `.app`'s own `PATH` cannot see. The greeting the profile printed first is stepped
        /// over: the marker, not the top of stdout, is where the answer starts.
        #[test]
        fn the_login_shell_answer_is_taken_past_whatever_the_profile_printed() {
            let real = dir_with_git(0o755).join("git");
            let shell = shell_answering(&format!("echo '{}'", real.display()));
            assert_eq!(from_login_shell(shell.as_os_str()), Some(real));
        }

        /// And is not taken otherwise. A shell that says nothing, exits non-zero, or names something that is
        /// not there leaves Amenbo with no git — never with a path it would find out about by spawning it.
        #[test]
        fn a_login_shell_that_cannot_name_a_real_git_yields_nothing() {
            let silent = shell_answering("true");
            assert_eq!(from_login_shell(silent.as_os_str()), None, "no git found is no answer");

            let failing = shell_answering("echo /usr/bin/git; exit 127");
            assert_eq!(from_login_shell(failing.as_os_str()), None, "a non-zero exit is no answer");

            let phantom = shell_answering("echo /nowhere/git");
            assert_eq!(from_login_shell(phantom.as_os_str()), None, "a path with nothing at it is no answer");

            let relative = shell_answering("echo git");
            assert_eq!(from_login_shell(relative.as_os_str()), None, "a bare name is not a place");

            assert_eq!(from_login_shell(OsStr::new("/nonexistent-shell")), None, "an unstartable shell is no answer");
        }

        /// The tools are asked about at most once, however many candidates turn out to be the stub.
        #[test]
        fn the_developer_tools_are_asked_about_once() {
            let asked = Cell::new(0);
            let picked = choose(Some(PathBuf::from(STUB)), || {
                asked.set(asked.get() + 1);
                false
            }, || Some(PathBuf::from(STUB)));
            assert_eq!(picked, None);
            assert_eq!(asked.get(), 1, "the second stub reuses the first answer");
        }
    }
}
