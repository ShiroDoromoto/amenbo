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
//! a non-zero exit is fatal — on top of [`PluginInvocation::run`]. This layer holds none of that: it
//! spawns, feeds, waits, and reports. Nothing here decides how long to wait or what the output means.

use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;

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

    /// Run the plugin to completion and capture its output.
    ///
    /// The payload is written on a thread, so a child that floods stdout before it drains stdin cannot
    /// deadlock us against a full pipe — the reader and the writer make progress independently. Failing
    /// to spawn (no such executable, not runnable) is the returned `Err`; a plugin that runs and exits
    /// non-zero is an `Ok` whose [`PluginOutput::code`] says so — "it ran and failed" is not this
    /// layer's error to raise.
    pub fn run(&self) -> std::io::Result<PluginOutput> {
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

        let out = child.wait_with_output()?;
        // The writer has nothing left to do once the child has exited; join to reap it.
        let _ = writer.join();

        Ok(PluginOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
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
}

impl PluginOutput {
    /// Whether the plugin exited cleanly (code 0). A signalled death (`code == None`) is not success.
    pub fn succeeded(&self) -> bool {
        self.code == Some(0)
    }
}
