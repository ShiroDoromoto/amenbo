//! The plugin execution substrate: start a plugin and hand it a JSON payload.
//!
//! A plugin is a program — any executable, in any language — not a linked-in binding. amenbo starts it,
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
//! waits, and reports. It offers the wait two ways — [`RunningPlugin::wait`] blocks until the child exits,
//! [`RunningPlugin::wait_timeout`] gives up and kills it after a bound the *caller* names — but it never
//! decides how long that bound is, nor what the output means.

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
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn()?;

        // Take stdin and write the payload from another thread — see the deadlock note above. A write
        // error (the child closed stdin early, or exited before reading) is not fatal: the child made
        // its own choice, and its exit code / output is what we report.
        let mut stdin = child.stdin.take().expect("stdin was configured piped");
        let payload = self.stdin_json.clone().into_bytes();
        let writer = std::thread::spawn(move || {
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
/// finish — [`wait`](Self::wait) for as long as it takes, [`wait_timeout`](Self::wait_timeout) up to a
/// bound — and collect what it left. Either wait reaps the child and joins the drain threads, so there is
/// no way to hold one of these without eventually reaping the process.
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

/// How often the bounded wait re-checks a still-running child. Small enough that a hook's kill is prompt,
/// large enough that a slow plugin does not spin a core while we wait it out.
const POLL: Duration = Duration::from_millis(20);

impl RunningPlugin {
    /// Wait for the child to exit however long it takes, then collect its output.
    pub fn wait(self) -> std::io::Result<PluginOutput> {
        let RunningPlugin { mut child, writer, stdout, stderr, started } = self;
        let status = child.wait()?;
        Ok(finish(writer, stdout, stderr, status.code(), started.elapsed()))
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
