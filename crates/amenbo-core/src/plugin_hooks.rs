//! The observation-hook runner: launch the plugins listening for a fired event, off the write path, and
//! never let a slow or broken one touch the main flow.
//!
//! This is the asynchronous, fire-and-forget face of the plugin contract. A hook is a *post-only
//! observer* — it runs after the write is committed and durable, so it cannot fail or veto it — and this
//! module holds the policy that keeps that promise:
//!
//! - **Fire-and-forget.** Each plugin is started on its own background thread and the caller returns at
//!   once; the CLI/GUI it launched from is not blocked for a millisecond waiting on plugin work.
//! - **Timeout.** A hook that overruns [`HOOK_TIMEOUT`] is **killed** — a runaway observer leaks a process
//!   but never wedges the runner.
//! - **Warn only.** Anything but a clean exit — would not spawn, exited non-zero, killed for running too
//!   long — is a [`tracing::warn`] and nothing more. A hook's stdout carries no return value (that is the
//!   business of the synchronous command face), so a broken result is ignored, never fatal — the same
//!   non-fatal policy the activity ledger already follows.
//! - **Recorded.** Warning into a log nobody reads is how "why did nothing happen" goes unanswered, so
//!   every run — the clean one included — is also written to the execution log ([`crate::plugin_log`],
//!   `AMB-D-361`) when the caller names one. What is written is the outcome, the code, the duration and
//!   the plugin's stderr; the invocation, and with it every injected secret, never leaves this module.
//! - **Independent.** Each plugin runs on its own thread, so one that hangs or dies takes none of the
//!   others down with it.
//!
//! What plugins listen for a given event, and the payload each is handed, are the mapping layer's to
//! decide; this runner takes the invocations already built for one fired event and launches them. It does
//! not decide *when* to fire either — the write path pumps the event layer and calls [`fire`] with the
//! invocations for each event, after the commit.

use std::path::Path;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::plugin_exec::PluginInvocation;
use crate::plugin_log::{self, Outcome, Run};

/// How long a single observation hook may run before it is killed. A hook is a fire-and-forget observer
/// whose output nobody waits on, so the bound is not there to keep anyone quick — it only has to stop a
/// runaway from leaking a process for good.
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a `reply:true` hook may run before its reply is given up on (`AMB-D-383`). Shorter than
/// [`HOOK_TIMEOUT`] because a reply is run **synchronously** — the caller (the AI at a CLI command) is
/// waiting on it — so the bound is what keeps a slow or wedged advice hook from stalling the command it
/// rode in on. Overrun means no reply, not a stalled command: the run is still logged, the drain moves on.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

/// One hook to launch: **which plugin**, **for which event**, and the invocation carrying the payload.
///
/// The invocation alone would run just as well — but it names only a program path, and by the time the
/// dispatcher has folded a page of events into a list of invocations, which plugin and which event each
/// came from is gone. Carrying the two names down to the runner is what lets a warning (and, later, an
/// execution log) say *slack failed on task.created* rather than *a program at this path failed*.
///
/// The event is `&'static str` because it always is one — the dispatcher only ever fires the contract's
/// own names ([`V1_EVENTS`](crate::plugin_payload::V1_EVENTS)) — which also makes a hook trivially
/// movable onto its own thread.
#[derive(Debug, Clone)]
pub struct Hook {
    /// The plugin's name, as the installed registry knows it.
    pub plugin: String,
    /// The event that fired this hook.
    pub event: &'static str,
    /// The plugin to run, its payload already on stdin.
    pub invocation: PluginInvocation,
}

impl Hook {
    /// A hook for `plugin`, fired by `event`, running `invocation`.
    pub fn new(plugin: impl Into<String>, event: &'static str, invocation: PluginInvocation) -> Self {
        Self { plugin: plugin.into(), event, invocation }
    }
}

/// Launch every invocation as an observation hook, each on its own thread, and return at once.
///
/// This is the fire-and-forget seam: the caller does not wait, does not learn whether any plugin
/// succeeded, and cannot be failed by one. Pass the [`Hook`]s the mapping layer built (one per listening
/// plugin per fired event), after the write is committed.
///
/// `log` is the execution log to record each run in (`AMB-D-361`) — a real caller passes
/// [`Paths::plugin_log_file`](crate::config::Paths::plugin_log_file); `None` runs the hooks and records
/// nothing, which is what a test exercising the runner itself wants.
///
/// The returned handles are the launched threads. **Dropping them forgets the hooks** — the true
/// fire-and-forget a long-lived GUI wants. A short-lived process that is about to exit (a one-shot CLI
/// invocation) can instead *join* them first, so the hooks it started are not cut short when the process
/// dies; whether to wait that moment out is the caller's call, not the runner's.
#[must_use = "drop the handles to forget the hooks, or join them before a short-lived process exits"]
pub fn fire(hooks: Vec<Hook>, log: Option<&Path>) -> Vec<JoinHandle<()>> {
    fire_with_timeout(hooks, HOOK_TIMEOUT, log)
}

/// [`fire`] under a caller-named per-hook bound instead of the [`HOOK_TIMEOUT`] policy default.
///
/// The default is what the write path wants; this is the seam for a caller that must name its own bound —
/// notably a test running against real child processes on a saturated machine, where a hook the policy
/// bound would comfortably clear can still overrun it under load. Widening the bound there keeps the test
/// pinned on the fire-and-forget behaviour, not on how fast a loaded kernel happens to schedule a `touch`.
#[must_use = "drop the handles to forget the hooks, or join them before a short-lived process exits"]
pub fn fire_with_timeout(
    hooks: Vec<Hook>,
    timeout: Duration,
    log: Option<&Path>,
) -> Vec<JoinHandle<()>> {
    let log = log.map(Path::to_path_buf);
    hooks
        .into_iter()
        .map(move |hook| {
            let log = log.clone();
            std::thread::spawn(move || run_one(&hook, timeout, log.as_deref()))
        })
        .collect()
}

/// Run one fire-and-forget hook under `timeout` and record it in the execution log (`AMB-D-361`). Never
/// returns anything — the fire-and-forget face has nowhere to hand an error or an output to, so a run
/// becomes a log line and stops there. The running itself, and the warning on anything but a clean exit,
/// are [`execute`]'s; this only decides the result's fate: log it and drop it.
fn run_one(hook: &Hook, timeout: Duration, log: Option<&Path>) {
    let recorded = execute(hook, Some(timeout));
    if let Some(path) = log {
        plugin_log::record(path, &recorded);
    }
}

/// Run one `reply:true` hook synchronously and hand its **advice** back to the caller (`AMB-D-383`).
///
/// This is the consumed-reply path [`run_replies`](crate::plugin_dispatch::run_replies) takes on the CLI
/// face: the hook is run inline (not on a background thread), recorded in the execution log exactly as
/// [`run_one`] records a fire-and-forget one, and its stderr is returned for the caller to surface. A reply
/// is carried only when the hook actually **ran** and wrote something — a clean or non-zero exit with
/// non-empty stderr; the exit code is not the gate, since stderr is the advice channel (`AMB-D-353`), not
/// stdout. A timeout (`AMB-D-383`: overrun is dropped) or a failure to launch produces no reply — nothing
/// useful was said — while the log still carries the warning [`execute`] emitted.
pub fn run_reply(hook: &Hook, timeout: Duration, log: Option<&Path>) -> Option<String> {
    let recorded = execute(hook, Some(timeout));
    if let Some(path) = log {
        plugin_log::record(path, &recorded);
    }
    match recorded.outcome {
        Outcome::Ok | Outcome::Failed if !recorded.stderr.is_empty() => Some(recorded.stderr),
        _ => None,
    }
}

/// Run one queued hook **on this thread, to its end**, and record it (`AMB-D-399`).
///
/// This is the queue runner's path ([`crate::plugin_runner`]), and it differs from [`fire`] in both halves:
/// it does not spawn a thread — the runner *is* the thread, and one plugin's events are run one at a time,
/// in the order they were queued — and it names no timeout. The five-second kill exists to stop a hook the
/// write path launched from leaking a process for good; a runner is not the write path, nothing waits
/// behind it but the rest of its own plugin's queue, and a plugin killed mid-work is exactly the half-done
/// outside effect the queue was split off to stop. So a slow plugin is waited on, and only a plugin that
/// never returns holds its own queue — which its lease's horizon then hands to the next runner.
pub fn run_queued(hook: &Hook, log: Option<&Path>) {
    let recorded = execute(hook, None);
    if let Some(path) = log {
        plugin_log::record(path, &recorded);
    }
}

/// Run one hook under `bound`, warn on anything but a clean exit, and build the [`Run`] to record. Shared
/// by the fire-and-forget path ([`run_one`]), the synchronous reply path ([`run_reply`]) and the queue
/// runner's ([`run_queued`]): the only difference between them is what becomes of the result — logged and
/// dropped, or logged and returned — so the running, the end-arms, and the warnings all live here, once.
///
/// `None` waits for the plugin to finish, however long it takes. That is the queue runner's bound, because
/// a queue runner has no one to keep waiting: nothing is behind it but the rest of its own plugin's queue
/// (`AMB-D-399`). A bound is what the other two need — a caller is waiting on the reply, and a fire the
/// write path launched must not leak a process for good.
fn execute(hook: &Hook, bound: Option<Duration>) -> Run {
    let program = hook.invocation.program.display().to_string();
    let run = |outcome, code, elapsed, stderr: &str| Run {
        plugin: hook.plugin.clone(),
        event: hook.event,
        outcome,
        code,
        elapsed,
        stderr: stderr.to_string(),
    };
    // Only these fields ever reach the log: the invocation — whose env carries the plugin's injected
    // secrets (`AMB-D-356`) — stays in this function.
    let waited = hook.invocation.spawn().and_then(|running| match bound {
        Some(timeout) => running.wait_timeout(timeout),
        None => running.wait().map(Some),
    });
    let recorded = match waited {
        Ok(Some(output)) if output.succeeded() => {
            run(Outcome::Ok, output.code, output.elapsed, &output.stderr)
        }
        Ok(Some(output)) => {
            tracing::warn!(
                plugin = %hook.plugin,
                event = %hook.event,
                program = %program,
                code = ?output.code,
                elapsed_ms = output.elapsed.as_millis() as u64,
                "plugin hook exited without success; ignored"
            );
            run(Outcome::Failed, output.code, output.elapsed, &output.stderr)
        }
        Ok(None) => {
            // Unreachable without a bound: an unbounded wait either finishes or never returns, so the
            // arm is the bounded callers' and the elapsed it records is the bound they named.
            let timeout = bound.unwrap_or_default();
            tracing::warn!(
                plugin = %hook.plugin,
                event = %hook.event,
                program = %program,
                timeout_ms = timeout.as_millis() as u64,
                "plugin hook timed out and was killed; ignored"
            );
            // A killed hook has no code and no stderr to hand back — the child was reaped, not read.
            run(Outcome::TimedOut, None, timeout, "")
        }
        Err(error) => {
            tracing::warn!(
                plugin = %hook.plugin,
                event = %hook.event,
                program = %program,
                error = %error,
                "plugin hook could not be launched; ignored"
            );
            // The launch failure itself is the diagnosis here, and it is amenbo's message, not the
            // plugin's — there was no child to write one.
            run(Outcome::NotLaunched, None, Duration::ZERO, &error.to_string())
        }
    };
    recorded
}
