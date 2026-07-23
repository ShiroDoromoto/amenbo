//! The observation-hook runner, driven against real child processes. These pin the fire-and-forget
//! promises: a clean hook runs to completion, several hooks run independently of one another, and [`fire`]
//! returns to the caller before a slow hook is anywhere near done — the write path is never held up.
//!
//! The kill-on-timeout mechanism itself is pinned on the substrate (`plugin_exec`'s bounded wait); here we
//! only need to know the runner reaches it. The scripts make these `#[cfg(unix)]`.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use amenbo_core::plugin_exec::PluginInvocation;
use amenbo_core::plugin_hooks::{fire_with_timeout, run_reply, Hook};

/// A per-hook bound generous enough that even a fork-storm-saturated machine (the whole `make test`
/// gate running in parallel) reaps a trivial hook well inside it — so these tests never invert on the
/// kill-on-timeout path they are not testing. The real [`HOOK_TIMEOUT`](amenbo_core::plugin_hooks::HOOK_TIMEOUT)
/// policy default is exercised where it matters (the kill itself lives in `plugin_exec`'s bounded wait).
const GENEROUS: Duration = Duration::from_secs(60);

/// Write `body` as an executable script and return its path.
fn script(name: &str, body: &str) -> PathBuf {
    let dir = amenbo_scratch::scratch("plugin-hooks");
    let file = dir.join(name);
    std::fs::write(&file, body).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    file
}

fn marker(name: &str) -> PathBuf {
    let path = amenbo_scratch::scratch("plugin-hooks").join(name);
    let _ = std::fs::remove_file(&path);
    path
}

/// The hook the runner is handed: a plugin name, the event that fired it, and the script to run. These
/// tests pin the runner's behaviour, not what it logs, so the event is simply a real one.
fn hook_for(plugin: &str, program: &PathBuf) -> Hook {
    Hook::new(plugin, "task.created", PluginInvocation::new(program))
}

/// A hook that exits cleanly runs to completion — its side effect (a marker file) is there once the
/// launched thread joins.
#[test]
fn a_clean_hook_runs_to_completion() {
    let done = marker("clean.marker");
    let hook = script("clean.sh", &format!("#!/bin/sh\ntouch '{}'\n", done.display()));

    for handle in fire_with_timeout(vec![hook_for("clean", &hook)], GENEROUS, None) {
        handle.join().unwrap();
    }
    assert!(done.exists(), "the clean hook's side effect landed");
}

/// Every plugin passed for one event is launched, each on its own thread — one firing drives them all,
/// and both markers are there once the threads join.
#[test]
fn fire_launches_every_plugin_independently() {
    let a = marker("indep-a.marker");
    let b = marker("indep-b.marker");
    let hook_a = script("indep-a.sh", &format!("#!/bin/sh\ntouch '{}'\n", a.display()));
    let hook_b = script("indep-b.sh", &format!("#!/bin/sh\ntouch '{}'\n", b.display()));

    for handle in fire_with_timeout(vec![hook_for("a", &hook_a), hook_for("b", &hook_b)], GENEROUS, None) {
        handle.join().unwrap();
    }
    assert!(a.exists() && b.exists(), "both hooks ran");
}

/// `fire` is fire-and-forget: it hands back the moment the threads are launched, long before a hook that
/// takes seconds is done. The write path calling it is never blocked on plugin work. We still join the
/// handle afterwards, so the child is reaped cleanly (it finishes well inside the generous bound).
///
/// The return-promptness bound (1s) and the hook's delay (3s) are spread apart on purpose: under gate-load
/// saturation `fire`'s thread launch can be slow and the `sleep` still holds, so the ordering the test
/// pins — fire returns *before* the hook finishes — survives without the bound and the delay ever crossing.
#[test]
fn fire_returns_before_a_hook_finishes() {
    let done = marker("slow.marker");
    let hook = script("slow.sh", &format!("#!/bin/sh\nsleep 3\ntouch '{}'\n", done.display()));

    let start = Instant::now();
    let handles = fire_with_timeout(vec![hook_for("slow", &hook)], GENEROUS, None);
    let launched = start.elapsed();
    assert!(launched < Duration::from_secs(1), "fire returned promptly, not after the hook: {launched:?}");
    assert!(!done.exists(), "the slow hook has not finished yet when fire returns");

    for handle in handles {
        handle.join().unwrap();
    }
    assert!(done.exists(), "the hook did finish once we waited it out");
}

/// The runner records what it ran (`AMB-D-361`): a clean hook and a failing one both land in the
/// execution log, the failure carrying the exit code and the stderr its author wrote — which is the whole
/// point, since a hook can never report a failure any other way.
#[test]
fn every_run_lands_in_the_execution_log_with_its_diagnosis() {
    use amenbo_core::plugin_log::{self, Outcome};

    let log = amenbo_scratch::scratch("plugin-hooks-log").join(plugin_log::FILE_NAME);
    let _ = std::fs::remove_file(&log);

    let good = script("logged-ok.sh", "#!/bin/sh\nexit 0\n");
    let bad = script("logged-bad.sh", "#!/bin/sh\necho 'no such channel' >&2\nexit 3\n");
    let hooks = vec![hook_for("good", &good), hook_for("bad", &bad)];

    for handle in fire_with_timeout(hooks, GENEROUS, Some(&log)) {
        handle.join().unwrap();
    }

    let lines = plugin_log::read(&log);
    assert_eq!(lines.len(), 2, "both runs are recorded, the clean one included");
    let ok = lines.iter().find(|l| l.plugin == "good").expect("the clean run");
    assert_eq!(ok.outcome, Outcome::Ok);
    assert_eq!(ok.code, Some(0));
    assert_eq!(ok.event, "task.created", "the event that fired it is on the line");

    let failed = lines.iter().find(|l| l.plugin == "bad").expect("the failing run");
    assert_eq!(failed.outcome, Outcome::Failed);
    assert_eq!(failed.code, Some(3));
    assert!(failed.stderr.contains("no such channel"), "the author's diagnosis: {}", failed.stderr);

    let _ = std::fs::remove_file(&log);
}

/// A hook whose program is not there never launches — and that, too, is recorded: "it was never run" is a
/// different answer from "it ran and did nothing", and the log has to tell them apart.
#[test]
fn a_hook_that_cannot_launch_is_recorded_as_such() {
    use amenbo_core::plugin_log::{self, Outcome};

    let log = amenbo_scratch::scratch("plugin-hooks-log").join("not-launched.jsonl");
    let _ = std::fs::remove_file(&log);

    let missing = PathBuf::from("/nonexistent/amenbo-hook-that-is-not-there");
    for handle in fire_with_timeout(vec![hook_for("ghost", &missing)], GENEROUS, Some(&log)) {
        handle.join().unwrap();
    }

    let lines = plugin_log::read(&log);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].outcome, Outcome::NotLaunched);
    assert_eq!(lines[0].code, None, "there was no child to have an exit code");
    assert!(!lines[0].stderr.is_empty(), "why it could not be launched is on the line");

    let _ = std::fs::remove_file(&log);
}

/// A secret injected into a plugin's environment cannot reach the log: the runner is handed the
/// invocation, and hands the log only the outcome, the code, the duration and stderr. Proven end to end,
/// against the file's own bytes — a plugin that *echoes its own secret to stderr* has published it itself,
/// so the hook here keeps it to itself, which is the case the exclusion has to hold for.
#[test]
fn an_injected_secret_never_reaches_the_log() {
    use amenbo_core::plugin_log::FILE_NAME;

    let log = amenbo_scratch::scratch("plugin-hooks-secret").join(FILE_NAME);
    let _ = std::fs::remove_file(&log);

    let hook = script("secret.sh", "#!/bin/sh\necho 'ran' >&2\nexit 1\n");
    let invocation = PluginInvocation::new(&hook).env("AMENBO_CONFIG_TOKEN", "T0P-53CR3T");

    for handle in fire_with_timeout(
        vec![Hook::new("secretive", "task.created", invocation)],
        GENEROUS,
        Some(&log),
    ) {
        handle.join().unwrap();
    }

    let raw = std::fs::read_to_string(&log).unwrap();
    assert!(raw.contains("ran"), "the run itself was recorded: {raw}");
    assert!(!raw.contains("T0P-53CR3T"), "the injected secret is not in the log: {raw}");

    let _ = std::fs::remove_file(&log);
}

// ───────────────────── the synchronous reply path (`AMB-D-383`) ────────────────────────────────────

/// `run_reply` runs its hook synchronously and hands back what it wrote to stderr — the advice a `reply:true`
/// worktree hook produces, which the caller surfaces. The stderr is returned whatever the exit code, since
/// stderr is the advice channel (`AMB-D-353`), and the run is also recorded in the execution log.
#[test]
fn run_reply_returns_the_hooks_stderr_and_logs_it() {
    use amenbo_core::plugin_log::{self, Outcome};

    let log = amenbo_scratch::scratch("plugin-hooks-reply").join(plugin_log::FILE_NAME);
    let _ = std::fs::remove_file(&log);

    let advice = script("advice.sh", "#!/bin/sh\necho 'run the worktree' >&2\nexit 0\n");
    let reply = run_reply(&hook_for("worktree", &advice), GENEROUS, Some(&log));
    assert_eq!(reply.as_deref().map(str::trim), Some("run the worktree"));

    let lines = plugin_log::read(&log);
    assert_eq!(lines.len(), 1, "the reply run is recorded too, not just relayed");
    assert_eq!(lines[0].outcome, Outcome::Ok);

    let _ = std::fs::remove_file(&log);
}

/// A reply hook that writes nothing to stderr yields no reply — an empty reply is not carried, so the caller
/// has nothing to surface. It still ran and was logged.
#[test]
fn run_reply_carries_nothing_when_the_hook_is_silent() {
    let quiet = script("quiet.sh", "#!/bin/sh\nexit 0\n");
    assert!(run_reply(&hook_for("worktree", &quiet), GENEROUS, None).is_none());
}

/// A reply hook that overruns its bound is killed and yields no reply (`AMB-D-383`: overrun is dropped) —
/// the caller is not made to wait on a wedged advice hook. The short bound and the long sleep are spread far
/// apart so the drop is on the overrun, not on a slow spawn.
#[test]
fn run_reply_drops_an_overrunning_hook() {
    let start = Instant::now();
    let slow = script("slow-advice.sh", "#!/bin/sh\nsleep 3\necho 'too late' >&2\n");
    let reply = run_reply(&hook_for("worktree", &slow), Duration::from_millis(300), None);
    assert!(reply.is_none(), "an overrunning reply hook carries nothing back");
    assert!(start.elapsed() < Duration::from_secs(3), "and it did not wait the hook out");
}
