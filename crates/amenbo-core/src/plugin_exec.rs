//! The plugin execution substrate: start a plugin and hand it a JSON payload.
//!
//! A plugin is a program — any executable, in any language — not a linked-in binding. Amenbo starts it,
//! passes the payload two ways so the plugin can take whichever it likes, and captures what it wrote and
//! how it exited:
//!
//! - **stdin** carries the whole JSON document (a program with a JSON parser reads it here);
//! - **the environment** carries whatever key/values the caller chose to set alongside it (so a plain
//!   shell plugin can read a field with `$AMENBO_…` and never parse JSON).
//!
//! This is the common ground under both plugin faces. The asynchronous, fire-and-forget hook runner and
//! the synchronous command caller each build their own policy — a timeout, what stdout *means*, whether
//! a non-zero exit is fatal — on top of this substrate. This layer holds none of that: it spawns, feeds,
//! waits, and reports. It offers the wait three ways — [`RunningPlugin::wait`] blocks until the child exits,
//! [`RunningPlugin::wait_timeout`] gives up and kills it after a bound the *caller* names, and
//! [`RunningPlugin::wait_watched`] waits as long as the first while handing the caller the thread at an
//! interval — but it never decides how long a bound is, what an interval is for, nor what the output means.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A plugin to run, and everything it is handed. Owned, so a caller can build it and move it onto a
/// thread (the hook runner launches off the write path).
#[derive(Debug, Clone, Default)]
pub struct PluginInvocation {
    /// The executable to run.
    pub program: PathBuf,
    /// Arguments after the program. v1 observation hooks pass none; the command face uses them.
    pub args: Vec<String>,
    /// The JSON document written to the child's stdin.
    pub stdin_json: String,
    /// Environment variables set on the child, on top of the inherited environment.
    pub env: Vec<(String, String)>,
}

impl PluginInvocation {
    /// A run of `program` with nothing else set yet.
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self { program: program.into(), ..Self::default() }
    }

    /// Set the JSON document handed to the child on stdin. Builder-style.
    #[must_use]
    pub fn stdin_json(mut self, json: impl Into<String>) -> Self {
        self.stdin_json = json.into();
        self
    }

    /// Add an environment variable the child will see. Builder-style; call once per variable.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Add a command-line argument. Builder-style; call once per argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Run the plugin to completion and capture its output — the unbounded wait.
    ///
    /// A convenience for the command face, which wants the child's whole output and has its own idea of
    /// how long to wait: it is exactly [`spawn`](Self::spawn) followed by [`RunningPlugin::wait`]. Failing
    /// to spawn (no such executable, not runnable) is the returned `Err`; a plugin that runs and exits
    /// non-zero is an `Ok` whose [`PluginOutput::code`] says so — "it ran and failed" is not this layer's
    /// error to raise.
    pub fn run(&self) -> std::io::Result<PluginOutput> {
        self.spawn()?.wait()
    }

    /// Start the plugin and begin draining its output, handing back a [`RunningPlugin`] to wait on.
    ///
    /// This is the shared mechanism under both faces. The payload is written, and stdout/stderr are read,
    /// each on its own thread, so a child that floods stdout before it drains stdin cannot deadlock us
    /// against a full pipe — writer and readers make progress independently. Only the spawn itself can
    /// fail here (no such executable, not runnable); everything the child then does is reported by the
    /// wait. The caller picks the wait: unbounded ([`RunningPlugin::wait`]) or bounded
    /// ([`RunningPlugin::wait_timeout`]).
    pub fn spawn(&self) -> std::io::Result<RunningPlugin> {
        let mut cmd = crate::sys::command(&self.program);
        cmd.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Before the caller's own variables, so a caller that names `PATH` itself still wins.
        if let Some(path) = reachable_path() {
            cmd.env("PATH", path);
        }
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn()?;

        // Take stdin and write the payload from another thread — see the deadlock note above. A write
        // error (the child closed stdin early, or exited before reading) is not fatal: the child made
        // its own choice, and its exit code / output is what we report.
        let mut stdin = child.stdin.take().expect("stdin was configured piped");
        // The CLI runs with SIGPIPE at its default disposition (so `amenbo … | head` ends cleanly), which
        // would otherwise kill the whole process the moment a hook that never reads stdin closes this pipe
        // early (`AMB-D-352`). Guard the write two ways, one per platform: take SIGPIPE off this fd on
        // macOS, and block it on the writer thread for Linux — so the closed pipe comes back as the EPIPE
        // dropped below, never a signal.
        crate::sys::suppress_child_stdin_sigpipe(&stdin);
        let payload = self.stdin_json.clone().into_bytes();
        let writer = std::thread::spawn(move || {
            crate::sys::block_sigpipe_on_current_thread();
            let _ = stdin.write_all(&payload);
            // Dropping `stdin` closes the pipe, so the child reads EOF.
        });

        // Drain stdout and stderr each on their own thread. Reading them out of band is what lets the
        // bounded wait poll the child for exit without a full pipe wedging it — and it keeps `wait`'s
        // no-deadlock promise for a child that talks a lot before it reads.
        let mut child_stdout = child.stdout.take().expect("stdout was configured piped");
        let stdout = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = child_stdout.read_to_end(&mut buf);
            buf
        });
        let mut child_stderr = child.stderr.take().expect("stderr was configured piped");
        let stderr = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = child_stderr.read_to_end(&mut buf);
            buf
        });

        Ok(RunningPlugin { child, writer, stdout, stderr, started: Instant::now() })
    }
}

/// A spawned plugin, its stdin fed and its stdout/stderr draining on background threads. Wait for it to
/// finish — [`wait`](Self::wait) for as long as it takes, [`wait_watched`](Self::wait_watched) for as long
/// with the thread lent back at an interval, [`wait_timeout`](Self::wait_timeout) up to a bound — and
/// collect what it left. Every wait reaps the child and joins the drain threads, so there is no way to hold
/// one of these without eventually reaping the process.
pub struct RunningPlugin {
    child: Child,
    writer: JoinHandle<()>,
    stdout: JoinHandle<Vec<u8>>,
    stderr: JoinHandle<Vec<u8>>,
    /// When the child was spawned, so a finished run can report how long it took
    /// ([`PluginOutput::elapsed`]). Measured from the spawn rather than from the caller's wait: the two
    /// waits start at different moments, and what an observer wants to know is how long the plugin ran.
    started: Instant,
}

/// How often the polling waits re-check a still-running child. Small enough that a hook's kill, and a
/// watched wait's return, are prompt; large enough that a slow plugin does not spin a core while we wait it
/// out.
const POLL: Duration = Duration::from_millis(20);

impl RunningPlugin {
    /// Wait for the child to exit however long it takes, then collect its output.
    pub fn wait(self) -> std::io::Result<PluginOutput> {
        let RunningPlugin { mut child, writer, stdout, stderr, started } = self;
        let status = child.wait()?;
        Ok(finish(writer, stdout, stderr, status.code(), started.elapsed()))
    }

    /// Wait exactly as [`wait`](Self::wait) does — however long the child takes, and it is never killed —
    /// but hand the waiting thread back to `tick` every `every` while it runs (`AMB-T-2174`).
    ///
    /// The queue runner's wait. What it waits on is a plugin with no bound at all, and what it has to do
    /// meanwhile is push its lease out, which is a matter of a short transaction on the connection it
    /// already holds ([`plugin_runner`](crate::plugin_runner)). That is why this is an interval and not a
    /// thread: the caller is doing nothing but waiting, so it can be lent its own thread periodically and
    /// needs no second connection to reach the store from.
    ///
    /// `tick` is therefore expected to return promptly — it is the wait, and a long one delays noticing
    /// that the child has exited by however long it takes. The interval is measured the way the bound is,
    /// from the **spawn**, so the first call lands one interval into the run and not at its start.
    pub fn wait_watched(self, every: Duration, tick: &dyn Fn()) -> std::io::Result<PluginOutput> {
        let RunningPlugin { mut child, writer, stdout, stderr, started } = self;
        let mut next = every;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(finish(writer, stdout, stderr, status.code(), started.elapsed()));
            }
            if started.elapsed() >= next {
                tick();
                // Measured from where the tick left off, so a slow one drops beats rather than queuing
                // them up to be made back-to-back.
                next = started.elapsed() + every;
            }
            std::thread::sleep(POLL);
        }
    }

    /// Wait up to `timeout` for the child to exit; if it overruns, **kill it** and report the timeout.
    ///
    /// `Ok(Some(output))` is a child that finished on its own within the bound; `Ok(None)` is one that
    /// ran too long and was killed — the hook face treats that as just another non-success to warn about,
    /// so its half-written output is dropped rather than returned. A kill needs the child handle, which is
    /// why the bounded wait lives here and not on top of `run()`: `run()` has already given the child away
    /// to its own `wait`.
    ///
    /// The bound is measured from the **spawn**, not from the moment this is called — the same clock
    /// [`PluginOutput::elapsed`] reports, so "it ran for N ms" and "it overran the bound" cannot disagree.
    pub fn wait_timeout(self, timeout: Duration) -> std::io::Result<Option<PluginOutput>> {
        let RunningPlugin { mut child, writer, stdout, stderr, started } = self;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(Some(finish(writer, stdout, stderr, status.code(), started.elapsed())));
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                // Return the moment the child is reaped — do **not** join the drain threads here. A
                // killed child can leave a grandchild behind (a `sh` that spawned a `sleep`, say) still
                // holding the inherited pipe write-ends, so `read_to_end` would not see EOF until that
                // grandchild also exits — which is the very overrun we are cutting short. Detach the
                // threads instead; they end on their own when the pipes finally close, dropping what
                // they read. Returning promptly at the bound is the whole point of the bound.
                drop((writer, stdout, stderr));
                return Ok(None);
            }
            std::thread::sleep(POLL);
        }
    }
}

/// Join the writer and the two drain threads and assemble the captured output. The child has already been
/// reaped by the caller; this only collects what its pipes carried.
fn finish(
    writer: JoinHandle<()>,
    stdout: JoinHandle<Vec<u8>>,
    stderr: JoinHandle<Vec<u8>>,
    code: Option<i32>,
    elapsed: Duration,
) -> PluginOutput {
    let _ = writer.join();
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    PluginOutput {
        code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        elapsed,
    }
}

/// The `PATH` a plugin is started with: Amenbo's own directory in front, then whatever this process
/// inherited (`AMB-D-716`).
///
/// `AMB-D-406` tells a plugin author to read a record back by running `amenbo <read command> --json`,
/// found the way any program is found — on `PATH`. That is the only route that works in every language,
/// and it is a promise about the environment the plugin is started in, which is Amenbo's to keep.
///
/// **It was not kept where nobody typed the command.** A plugin fired from a terminal inherits the shell's
/// `PATH` and finds Amenbo there; one fired from the hourly tick inherits the scheduler's, and macOS's
/// launchd hands out `/usr/bin:/bin:/usr/sbin:/sbin` — which never holds the `~/.local/bin` the shipped CLI
/// sits in. The plugin's call came back "no such command", and the notification went out stripped of
/// everything it could not read.
///
/// **In front, not appended**, so the build that started the plugin is the one it calls back into: a device
/// carrying a second Amenbo further down `PATH` would otherwise answer for a store this run never opened.
/// The inherited entries follow unchanged, so every other command a plugin reaches for is where it was.
///
/// `None` where there is nothing to put in front — no `current_exe`, no parent directory, or a path that
/// cannot be joined — and the child is then left with exactly the environment it would have had.
fn reachable_path() -> Option<std::ffi::OsString> {
    let own_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let inherited = crate::env::path().unwrap_or_default();
    let dirs = std::iter::once(own_dir).chain(std::env::split_paths(&inherited));
    std::env::join_paths(dirs).ok()
}

/// What a finished plugin left behind. The output is captured as text — the plugin contract is a
/// text one (line-based stdout, human diagnostics on stderr) — decoded lossily so invalid bytes never
/// fail the capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginOutput {
    /// The exit code, or `None` when a signal killed the child (Unix) — there is no code in that case.
    pub code: Option<i32>,
    /// Everything the child wrote to stdout.
    pub stdout: String,
    /// Everything the child wrote to stderr.
    pub stderr: String,
    /// How long the plugin ran, measured from the spawn to the moment it was reaped. Captured here
    /// because this is the only layer that knows both ends: a caller above it sees the invocation and the
    /// result, never the child's lifetime.
    pub elapsed: Duration,
}

impl PluginOutput {
    /// Whether the plugin exited cleanly (code 0). A signalled death (`code == None`) is not success.
    pub fn succeeded(&self) -> bool {
        self.code == Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The promise `AMB-D-406` makes to a plugin author — "run `amenbo …`" — is kept by the environment the
    /// child is started in, so the directory Amenbo is running from has to lead its `PATH` (`AMB-D-716`).
    #[test]
    fn amenbos_own_directory_leads_the_path_a_plugin_is_given() {
        let path = reachable_path().expect("this test binary has a path, and that path has a parent");
        let first = std::env::split_paths(&path).next().expect("the joined path is not empty");
        let own_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        assert_eq!(first, own_dir);
    }

    /// Everything else a plugin reaches for has to stay where it was, so what follows the one entry we put
    /// in front is the inherited `PATH`, in its own order and untouched.
    #[test]
    fn the_inherited_path_follows_it_unchanged() {
        let inherited: Vec<_> =
            std::env::split_paths(&crate::env::path().unwrap_or_default()).collect();
        let path = reachable_path().expect("this test binary has a path, and that path has a parent");
        let behind: Vec<_> = std::env::split_paths(&path).skip(1).collect();
        assert_eq!(behind, inherited);
    }

    /// A fire-and-forget hook that never reads its stdin and exits at once must not take Amenbo down with
    /// it (`AMB-D-352` — a hook failing changes nothing). The CLI runs with SIGPIPE at its default
    /// disposition (so `amenbo … | head` ends cleanly), where an unguarded write to the now-closed stdin
    /// pipe ends the whole process by signal; the spawn guards it (SIGPIPE off the fd on macOS, blocked on
    /// the writer thread on Linux) so the closed pipe comes back as an EPIPE it drops instead. The payload
    /// is far larger than any pipe buffer, so the write is still in flight when the child exits — the
    /// losing side of the race the guard must win every time, which is why this reproduces deterministically
    /// what the CLI e2e only flaked on.
    #[cfg(unix)]
    #[test]
    fn a_hook_that_ignores_a_large_stdin_and_exits_does_not_signal_us_down() {
        // Reproduce the CLI's disposition for the length of this test, then restore it — a shared-process
        // run (plain `cargo test`) must not inherit a fatal SIGPIPE from us.
        // SAFETY: `signal` is async-signal-safe; this runs on the test thread before the spawn, and the
        // prior handler is captured to put back below.
        let prev = unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };

        let payload = "x".repeat(4 << 20); // 4 MiB, well past any pipe buffer: the write must block, then EPIPE
        let out = PluginInvocation::new("/bin/sh")
            .arg("-c")
            .arg("exit 0")
            .stdin_json(payload)
            .run()
            .expect("a hook that ignores stdin still runs to completion");
        assert_eq!(out.code, Some(0), "the child exited cleanly and we lived to report it");

        // SAFETY: restore the disposition the test found, same contract as above.
        unsafe { libc::signal(libc::SIGPIPE, prev) };
    }

    /// The child's own environment is what matters, not what we computed: a plugin asked for its `PATH`
    /// reads Amenbo's directory first.
    #[cfg(unix)]
    #[test]
    fn a_plugin_reads_that_path_out_of_its_own_environment() {
        let out = PluginInvocation::new("/bin/sh")
            .arg("-c")
            .arg("printf %s \"$PATH\"")
            .run()
            .expect("a plugin that prints its PATH runs");
        let first = out.stdout.split(':').next().expect("the child had a PATH");
        let own_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        assert_eq!(std::path::Path::new(first), own_dir);
    }

    /// A caller that sets `PATH` itself is making a deliberate choice about the child's environment, and
    /// the one entry Amenbo puts in front is not allowed to overrule it.
    #[cfg(unix)]
    #[test]
    fn a_caller_that_names_its_own_path_keeps_it() {
        let out = PluginInvocation::new("/bin/sh")
            .arg("-c")
            .arg("printf %s \"$PATH\"")
            .env("PATH", "/nowhere")
            .run()
            .expect("a plugin that prints its PATH runs");
        assert_eq!(out.stdout, "/nowhere");
    }
}
