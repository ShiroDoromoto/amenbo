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
use amenbo_core::plugin_hooks::fire;

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

/// A hook that exits cleanly runs to completion — its side effect (a marker file) is there once the
/// launched thread joins.
#[test]
fn a_clean_hook_runs_to_completion() {
    let done = marker("clean.marker");
    let hook = script("clean.sh", &format!("#!/bin/sh\ntouch '{}'\n", done.display()));

    for handle in fire(vec![PluginInvocation::new(&hook)]) {
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

    for handle in fire(vec![PluginInvocation::new(&hook_a), PluginInvocation::new(&hook_b)]) {
        handle.join().unwrap();
    }
    assert!(a.exists() && b.exists(), "both hooks ran");
}

/// `fire` is fire-and-forget: it hands back the moment the threads are launched, long before a hook that
/// takes a second is done. The write path calling it is never blocked on plugin work. We still join the
/// handle afterwards, so the child is reaped cleanly (it finishes well inside the 5s hook timeout).
#[test]
fn fire_returns_before_a_hook_finishes() {
    let done = marker("slow.marker");
    let hook = script("slow.sh", &format!("#!/bin/sh\nsleep 1\ntouch '{}'\n", done.display()));

    let start = Instant::now();
    let handles = fire(vec![PluginInvocation::new(&hook)]);
    let launched = start.elapsed();
    assert!(launched < Duration::from_millis(300), "fire returned promptly, not after the hook: {launched:?}");
    assert!(!done.exists(), "the slow hook has not finished yet when fire returns");

    for handle in handles {
        handle.join().unwrap();
    }
    assert!(done.exists(), "the hook did finish once we waited it out");
}
