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
//! - **Independent.** Each plugin runs on its own thread, so one that hangs or dies takes none of the
//!   others down with it.
//!
//! What plugins listen for a given event, and the payload each is handed, are the mapping layer's to
//! decide; this runner takes the invocations already built for one fired event and launches them. It does
//! not decide *when* to fire either — the write path pumps the event layer and calls [`fire`] with the
//! invocations for each event, after the commit.

use std::thread::JoinHandle;
use std::time::Duration;

use crate::plugin_exec::PluginInvocation;

/// How long a single observation hook may run before it is killed. A hook is a fire-and-forget observer
/// whose output nobody waits on, so the bound is not there to keep anyone quick — it only has to stop a
/// runaway from leaking a process for good.
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(5);

/// Launch every invocation as an observation hook, each on its own thread, and return at once.
///
/// This is the fire-and-forget seam: the caller does not wait, does not learn whether any plugin
/// succeeded, and cannot be failed by one. Pass the invocations the mapping layer built for a single fired
/// event (one per listening plugin); call it once per event, after the write is committed.
///
/// The returned handles are the launched threads. **Dropping them forgets the hooks** — the true
/// fire-and-forget a long-lived GUI wants. A short-lived process that is about to exit (a one-shot CLI
/// invocation) can instead *join* them first, so the hooks it started are not cut short when the process
/// dies; whether to wait that moment out is the caller's call, not the runner's.
#[must_use = "drop the handles to forget the hooks, or join them before a short-lived process exits"]
pub fn fire(plugins: Vec<PluginInvocation>) -> Vec<JoinHandle<()>> {
    plugins
        .into_iter()
        .map(|plugin| std::thread::spawn(move || run_one(&plugin, HOOK_TIMEOUT)))
        .collect()
}

/// Run one hook under `timeout` and warn on anything but a clean exit. Never returns — the hook face has
/// nowhere to return an error to, so a failure becomes a log line and stops there.
fn run_one(plugin: &PluginInvocation, timeout: Duration) {
    let program = plugin.program.display().to_string();
    match plugin.spawn().and_then(|running| running.wait_timeout(timeout)) {
        Ok(Some(output)) if output.succeeded() => {}
        Ok(Some(output)) => tracing::warn!(
            plugin = %program,
            code = ?output.code,
            "plugin hook exited without success; ignored"
        ),
        Ok(None) => tracing::warn!(
            plugin = %program,
            timeout_ms = timeout.as_millis() as u64,
            "plugin hook timed out and was killed; ignored"
        ),
        Err(error) => tracing::warn!(
            plugin = %program,
            error = %error,
            "plugin hook could not be launched; ignored"
        ),
    }
}
