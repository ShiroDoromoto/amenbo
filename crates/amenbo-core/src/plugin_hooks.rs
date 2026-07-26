//! The observation-hook **runner**: run one plugin for one event, and never let a slow or broken one
//! touch the main flow.
//!
//! A hook is a *post-only observer* — it runs after the write is committed and durable, so it cannot fail
//! or veto it — and this module holds the policy that keeps that promise. There are two ways in, and they
//! differ only in what becomes of the result:
//!
//! - **[`run_queued`] — the queue runner's** (`AMB-D-399`). A runner process works one plugin's queue, one
//!   event at a time, and waits each plugin out however long it takes: nothing is behind it but the rest of
//!   that plugin's own queue, and a plugin killed mid-work is the half-done outside effect the queue exists
//!   to prevent.
//! - **[`run_reply`] — the advice path** (`AMB-D-383`). A `reply:true` hook is run inline, under
//!   [`REPLY_TIMEOUT`], and its stderr handed back to the caller waiting on it.
//!
//! What both share:
//!
//! - **Warn only.** Anything but a clean exit — would not spawn, exited non-zero, killed for running too
//!   long — is a [`tracing::warn`] and nothing more. A hook's stdout carries no return value (that is the
//!   business of the synchronous command face), so a broken result is ignored, never fatal — the same
//!   non-fatal policy the activity ledger already follows.
//! - **Recorded.** Warning into a log nobody reads is how "why did nothing happen" goes unanswered, so
//!   every run — the clean one included — is also written to the execution log ([`crate::plugin_log`],
//!   `AMB-D-361`) when the caller names one. What is written is the outcome, the code, the duration and
//!   the plugin's stderr; the invocation, and with it every injected secret, never leaves this module.
//!
//! What plugins listen for a given event, and the payload each is handed, are the mapping layer's to
//! decide; this runner takes an invocation already built for one fired event and runs it. It does not
//! decide *when* either — the fan-out queues the event and a runner process gets to it
//! ([`crate::plugin_dispatch`], [`crate::plugin_runner`]).

use std::path::Path;
use std::time::Duration;

use crate::plugin_exec::PluginInvocation;
use crate::plugin_log::{self, Outcome, Run};

/// How long a `reply:true` hook may run before its reply is given up on (`AMB-D-383`). It is the one bound
/// here, because a reply is the one hook run **synchronously** — the caller (the AI at a CLI command) is
/// waiting on it — so this is what keeps a slow or wedged advice hook from stalling the command it rode in
/// on. Overrun means no reply, not a stalled command: the run is still logged, the drain moves on.
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

/// Run one `reply:true` hook synchronously and hand its **advice** back to the caller (`AMB-D-383`).
///
/// This is the consumed-reply path [`run_replies`](crate::plugin_dispatch::run_replies) takes on the CLI
/// face: the hook is run inline, recorded in the execution log exactly as [`run_queued`] records a queued
/// one, and its stderr is returned for the caller to surface. A reply
/// is carried only when the hook actually **ran** and wrote something — a clean or non-zero exit with
/// non-empty stderr; the exit code is not the gate, since stderr is the advice channel (`AMB-D-353`), not
/// stdout. A timeout (`AMB-D-383`: overrun is dropped) or a failure to launch produces no reply — nothing
/// useful was said — while the log still carries the warning [`execute`] emitted.
pub fn run_reply(hook: &Hook, timeout: Duration, log: Option<&Path>) -> Option<String> {
    let recorded = execute(hook, Some(timeout), None);
    if let Some(path) = log {
        plugin_log::record(path, &recorded);
    }
    match recorded.outcome {
        Outcome::Ok | Outcome::Failed if !recorded.stderr.is_empty() => Some(recorded.stderr),
        _ => None,
    }
}

/// What a caller wants done at intervals while its hook runs, and how often (`AMB-T-2174`).
///
/// The queue runner's, and the reason this path takes anything besides a hook: a runner holds its plugin's
/// lease while it works, and a run longer than that lease's horizon would otherwise have its queue taken
/// over mid-run. `beat` is called on the waiting thread, so it must return promptly — it is not a place to
/// do work, only to say *still here*.
#[derive(Clone, Copy)]
pub struct Heartbeat<'a> {
    /// How long between calls, measured from the spawn.
    pub every: Duration,
    /// What to do at each of them.
    pub beat: &'a dyn Fn(),
}

/// Run one queued hook **on this thread, to its end**, and record it (`AMB-D-399`).
///
/// This is the queue runner's path ([`crate::plugin_runner`]): it spawns nothing of its own — a runner is
/// a process working one plugin's events one at a time, in the order they were queued — and it names no
/// timeout. Nothing waits behind it but the rest of its own plugin's queue, and a plugin killed mid-work is
/// exactly the half-done outside effect the queue was split off to stop. So a slow plugin is waited on for
/// as long as it takes, and what a plugin that never returns holds is its own queue.
///
/// `heartbeat` is what the caller does while that wait goes on ([`Heartbeat`]). A caller with nothing to do
/// meanwhile passes `None` and the wait is a plain blocking one.
pub fn run_queued(hook: &Hook, log: Option<&Path>, heartbeat: Option<Heartbeat<'_>>) {
    let recorded = execute(hook, None, heartbeat);
    if let Some(path) = log {
        plugin_log::record(path, &recorded);
    }
}

/// Run one hook under `bound`, warn on anything but a clean exit, and build the [`Run`] to record. Shared
/// by the queue runner's path ([`run_queued`]) and the synchronous reply path ([`run_reply`]): the only
/// difference between them is what becomes of the result — logged and dropped, or logged and returned — so
/// the running, the end-arms, and the warnings all live here, once.
///
/// A `bound` of `None` waits for the plugin to finish, however long it takes. That is the queue runner's
/// bound, because a queue runner has no one to keep waiting: nothing is behind it but the rest of its own
/// plugin's queue (`AMB-D-399`). A bound is what a reply needs — a caller is waiting on it.
///
/// The two never come together: a `heartbeat` is for the unbounded wait, where a caller has to keep saying
/// it is still there for as long as that takes, and a bound already caps how long that is.
fn execute(hook: &Hook, bound: Option<Duration>, heartbeat: Option<Heartbeat<'_>>) -> Run {
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
    let waited = hook.invocation.spawn().and_then(|running| match (bound, heartbeat) {
        (Some(timeout), _) => running.wait_timeout(timeout),
        (None, Some(hb)) => running.wait_watched(hb.every, hb.beat).map(Some),
        (None, None) => running.wait().map(Some),
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
