//! The plugin execution substrate, run against real child processes. These pin the substrate's
//! promises: the payload reaches the child on stdin and in the environment, its stdout / stderr / exit
//! code come back faithfully, a plugin that runs and fails is an `Ok` (not an error), and a payload
//! larger than a pipe buffer still round-trips (the reason the stdin write is on its own thread).
//!
//! The round-trip cases run a small shell script, so they are `#[cfg(unix)]`; the spawn-failure case is
//! portable. The Windows spawn path is the shared `sys::command` wrapper the whole codebase uses.

use amenbo_core::plugin_exec::PluginInvocation;

/// Spawning something that is not there is the substrate's `Err` — the one failure it owns.
#[test]
fn a_missing_program_is_a_spawn_error() {
    let inv = PluginInvocation::new("this-plugin-does-not-exist-amenbo-xyz");
    assert!(inv.run().is_err(), "no such executable → Err");
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use amenbo_core::plugin_exec::PluginInvocation;

    /// Write `body` as an executable script and return its path.
    fn script(name: &str, body: &str) -> PathBuf {
        let dir = amenbo_scratch::scratch("plugin-exec");
        let file = dir.join(name);
        std::fs::write(&file, body).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        file
    }

    /// The payload reaches the child on stdin, and its stdout comes back verbatim.
    #[test]
    fn the_payload_arrives_on_stdin_and_stdout_comes_back() {
        let echo = script("echo.sh", "#!/bin/sh\ncat\n");
        let out = PluginInvocation::new(&echo).stdin_json(r#"{"event":"task.created","id":7}"#).run().unwrap();
        assert!(out.succeeded());
        assert_eq!(out.code, Some(0));
        assert_eq!(out.stdout, r#"{"event":"task.created","id":7}"#);
        assert_eq!(out.stderr, "");
    }

    /// Environment variables the caller set are visible to the child — the parse-free path.
    #[test]
    fn env_vars_reach_the_child() {
        let show = script("env.sh", "#!/bin/sh\nprintf '%s' \"$AMENBO_EVENT\"\n");
        let out = PluginInvocation::new(&show).env("AMENBO_EVENT", "comment.added").run().unwrap();
        assert_eq!(out.stdout, "comment.added");
    }

    /// stderr and a non-zero exit come back as data, not as an error: the plugin ran, it just failed.
    #[test]
    fn a_nonzero_exit_is_captured_not_raised() {
        let fail = script("fail.sh", "#!/bin/sh\necho 'went wrong' >&2\nexit 3\n");
        let out = PluginInvocation::new(&fail).run().unwrap();
        assert_eq!(out.code, Some(3));
        assert!(!out.succeeded());
        assert!(out.stderr.contains("went wrong"), "stderr captured: {:?}", out.stderr);
    }

    /// A payload larger than a pipe buffer round-trips: the child fills stdout while we are still
    /// writing stdin, and the threaded writer keeps both pipes draining so neither blocks the other.
    #[test]
    fn a_large_payload_does_not_deadlock() {
        let echo = script("echo-big.sh", "#!/bin/sh\ncat\n");
        let big = "x".repeat(512 * 1024); // well past the ~64 KiB pipe buffer
        let out = PluginInvocation::new(&echo).stdin_json(big.clone()).run().unwrap();
        assert!(out.succeeded());
        assert_eq!(out.stdout.len(), big.len());
    }

    /// A child that finishes inside the bound comes back as `Some`, with its output — the bounded wait is
    /// the unbounded one when nothing overruns.
    #[test]
    fn a_quick_child_finishes_within_the_timeout() {
        let echo = script("timeout-quick.sh", "#!/bin/sh\ncat\n");
        let out = PluginInvocation::new(&echo)
            .stdin_json("hello")
            .spawn()
            .unwrap()
            .wait_timeout(Duration::from_secs(5))
            .unwrap();
        let out = out.expect("finished on its own, not timed out");
        assert!(out.succeeded());
        assert_eq!(out.stdout, "hello");
    }

    /// A child that overruns the bound is killed: the wait returns `None` well before the child would have
    /// finished, and the marker its tail would have written never appears — proof the kill landed, not
    /// that we merely stopped watching.
    #[test]
    fn an_overrunning_child_is_killed() {
        let dir = amenbo_scratch::scratch("plugin-exec");
        let marker = dir.join("overrun.marker");
        let _ = std::fs::remove_file(&marker);
        let slow = script(
            "timeout-slow.sh",
            &format!("#!/bin/sh\nsleep 5\ntouch '{}'\n", marker.display()),
        );

        let start = Instant::now();
        let out = PluginInvocation::new(&slow).spawn().unwrap().wait_timeout(Duration::from_millis(200)).unwrap();
        assert!(out.is_none(), "the overrunning child times out");
        assert!(start.elapsed() < Duration::from_secs(3), "the wait gives up near the bound, not at sleep's end");
        assert!(!marker.exists(), "the child was killed before its tail could run");
    }
}
