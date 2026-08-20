//! The command face: run a plugin synchronously and hand its stdout back as the invoking command's
//! return value.
//!
//! Amenbo has two plugin faces (`AMB-D-352`). The observation hook is asynchronous and fire-and-forget;
//! this is the other one — the **command**, an explicit invocation whose caller *waits for a return
//! value*. The contract, lifted verbatim from the shape `devtool task start` already proves in the field
//! (`AMB-D-353`):
//!
//! - **stdout is the machine return value** — line-based text Amenbo hands back to whoever invoked the
//!   command (the AI), as that command's result. `devtool` emits one `cd <dir>` line the caller `eval`s;
//!   the value is opaque to Amenbo, which only relays it.
//! - **stderr is human diagnostics** — a summary, an error, context — never the return value.
//! - **the exit code is success or failure.** A non-zero (or signalled) exit is a failed call: the
//!   return value is *not* used and not handed back — the caller is told it failed instead (`AMB-D-354`,
//!   the command face's half: broken return values are never consumed).
//!
//! This layer builds that policy on top of [`PluginInvocation::run`](crate::plugin_exec::PluginInvocation::run),
//! which is the face-agnostic substrate (spawn, feed the JSON payload, wait, capture). It adds *what the
//! captured output means* for a command, and nothing else: the synchronous wait is the substrate's; a
//! timeout, if any, is the caller's discretion here (`AMB-D-352`), not imposed by this face.
//!
//! Output-format validation beyond the exit code — the shared check `plugin validate` also exposes — is
//! `AMB-T-1988`'s single home; v1's line-based stdout has no shape to reject past "it ran and exited 0",
//! so [`interpret`] gates on the exit code alone and leaves that seam for the validation layer to tighten.

use crate::plugin_exec::{PluginInvocation, PluginOutput};

/// What a command-plugin invocation resolved to, under the command contract (`AMB-D-353`/`AMB-D-354`).
///
/// The two arms are the whole contract: a run either produced a return value Amenbo relays, or it failed
/// and there is nothing to relay. There is no third "succeeded but ignore the output" state — a command
/// is invoked *for* its return value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// The plugin exited cleanly. `value` is its stdout — the machine return value Amenbo hands back to
    /// the caller as the result of the invoking command, verbatim (any trailing newline included; the
    /// consumer, e.g. a shell `eval`, strips what it does not want). `diagnostic` is its stderr, which a
    /// clean exit carries just as a failed one does: the summary a plugin writes beside its return value is
    /// the human half of the same run (`devtool task start` prints one), and a face that dropped it would
    /// hand the caller a bare directory where the plugin wrote a paragraph.
    Returned { value: String, diagnostic: String },
    /// The plugin failed: a non-zero exit, or a signal (`code` is `None`). The return value is unusable
    /// and is not handed back. `diagnostic` is the plugin's stderr — the human-facing account of why.
    Failed { code: Option<i32>, diagnostic: String },
}

impl CommandOutcome {
    /// The relayed return value, or `None` when the call failed. A convenience for a caller that only
    /// wants the value and treats any failure the same way.
    pub fn value(&self) -> Option<&str> {
        match self {
            CommandOutcome::Returned { value, .. } => Some(value),
            CommandOutcome::Failed { .. } => None,
        }
    }
}

/// Read a finished plugin's captured output as a command outcome.
///
/// The gate is the exit code: cleanly exited (code 0) yields [`CommandOutcome::Returned`] carrying
/// stdout; anything else yields [`CommandOutcome::Failed`] carrying the exit code. Both arms carry stderr
/// — it is the run's diagnostics either way, and only the *return value* is conditional. On failure stdout
/// is discarded — a command's broken return value is never consumed (`AMB-D-354`). This is the one
/// place the command face decides what output means; the validation layer (`AMB-T-1988`) tightens the
/// success arm when a structured return is added.
pub fn interpret(out: PluginOutput) -> CommandOutcome {
    if out.succeeded() {
        CommandOutcome::Returned { value: out.stdout, diagnostic: out.stderr }
    } else {
        CommandOutcome::Failed { code: out.code, diagnostic: out.stderr }
    }
}

/// Run a command plugin to completion and read the outcome under the command contract.
///
/// Synchronous by contract (`AMB-D-352`): the caller is waiting for the return value, so this blocks
/// until the plugin exits. A spawn failure (no such executable) is the returned `Err` — the invocation
/// never started; a plugin that ran and exited non-zero is an `Ok` holding [`CommandOutcome::Failed`],
/// since "it ran and failed" is an outcome, not an error of ours to raise.
pub fn run(invocation: &PluginInvocation) -> std::io::Result<CommandOutcome> {
    Ok(interpret(invocation.run()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A finished run, as this layer reads one. `elapsed` is zero throughout: how long the plugin took is
    /// the execution log's material, and none of the rules here look at it.
    fn out(code: Option<i32>, stdout: &str, stderr: &str) -> PluginOutput {
        PluginOutput {
            code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            elapsed: std::time::Duration::ZERO,
        }
    }

    #[test]
    fn a_clean_exit_relays_stdout_as_the_return_value() {
        let outcome = interpret(out(Some(0), "cd /tmp/wt\n", "task 7 ready"));
        assert_eq!(
            outcome,
            CommandOutcome::Returned {
                value: "cd /tmp/wt\n".to_string(),
                diagnostic: "task 7 ready".to_string(),
            }
        );
        assert_eq!(outcome.value(), Some("cd /tmp/wt\n"), "the caller relays the value verbatim");
    }

    #[test]
    fn an_empty_stdout_on_a_clean_exit_is_a_valid_empty_return() {
        // A command with no return value succeeds with an empty value — not a failure.
        assert_eq!(
            interpret(out(Some(0), "", "done")),
            CommandOutcome::Returned { value: String::new(), diagnostic: "done".to_string() }
        );
    }

    #[test]
    fn a_nonzero_exit_fails_and_discards_the_return_value() {
        // Even with stdout present, a non-zero exit means the return value is broken and unused.
        let outcome = interpret(out(Some(2), "half-written", "boom: bad input"));
        assert_eq!(outcome, CommandOutcome::Failed { code: Some(2), diagnostic: "boom: bad input".to_string() });
        assert_eq!(outcome.value(), None, "a failed call relays no value");
    }

    #[test]
    fn a_signalled_death_fails_with_no_code() {
        assert_eq!(
            interpret(out(None, "", "")),
            CommandOutcome::Failed { code: None, diagnostic: String::new() }
        );
    }

    // A real subprocess needs a shell, so this one is unix-only — the same gate the substrate's
    // round-trip tests use; the portable spawn path is exercised in `tests/plugin_exec.rs`.
    #[cfg(unix)]
    #[test]
    fn run_drives_a_real_plugin_and_reads_its_outcome() {
        // A shell plugin that echoes a return value on stdout and a diagnostic on stderr, exit 0.
        let inv = PluginInvocation::new("/bin/sh")
            .arg("-c")
            .arg("printf 'cd /w'; printf 'note\\n' 1>&2");
        assert_eq!(
            run(&inv).unwrap(),
            CommandOutcome::Returned {
                value: "cd /w".to_string(),
                diagnostic: "note\n".to_string(),
            }
        );

        // A plugin that exits non-zero fails, and its stderr is the diagnostic.
        let bad = PluginInvocation::new("/bin/sh").arg("-c").arg("printf 'oops\\n' 1>&2; exit 3");
        assert_eq!(
            run(&bad).unwrap(),
            CommandOutcome::Failed { code: Some(3), diagnostic: "oops\n".to_string() }
        );
    }
}
