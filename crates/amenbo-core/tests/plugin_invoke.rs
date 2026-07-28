//! The command face driven end to end, against a real store and a real child process: an installed
//! plugin, its gate, and what comes back when it is called (`AMB-D-353`).
//!
//! The unit tests beside the module pin the pieces (the stdin document, what one outcome logs). These pin
//! the seam nobody else can: that the gate actually refuses, that a caller's words reach the plugin's
//! argv, and that a plugin's two streams come back as the two halves the contract names — stdout the
//! return value, stderr the diagnostic. The scripts make these `#[cfg(unix)]`, like the hook runner's.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use amenbo_core::config::Paths;
use amenbo_core::plugin_command::CommandOutcome;
use amenbo_core::plugin_invoke;
use amenbo_core::plugin_trust::{enable, Gate};
use amenbo_core::Store;

/// A store with one plugin installed by hand: the manifest an install would have written, plus a shell
/// script standing in for the executable. Declared `scope: machine` so the gate is the device's and these
/// tests need no bound project — what a project-scoped gate does is the trust boundary's own material.
fn store_with_plugin(label: &str, name: &str, script: &str) -> Store {
    let base = amenbo_scratch::scratch(label);
    let paths = Paths::at(base.clone());
    let home = paths.plugin_dir(name);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("manifest.json"),
        serde_json::json!({
            "name": name,
            "desc": "a plugin that answers",
            "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo-plugin-test",
            "os": ["macos", "linux"],
            "category": "workflow",
            "scope": "machine",
            "url": "https://example.invalid/plugin.tar.gz",
            "checksum": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        })
        .to_string(),
    )
    .unwrap();
    let program = home.join(name);
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    Store::open_at(paths).unwrap()
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

/// Installed is not enabled (`AMB-D-351`): a plugin nobody consented to run is refused, and the caller is
/// told so rather than handed an empty return value.
#[test]
fn an_installed_but_disabled_plugin_is_refused() {
    let store = store_with_plugin("invoke-gate", "quiet", "#!/bin/sh\nprintf 'cd /w\\n'\n");

    let err = plugin_invoke::call(&store, "quiet", &[], None).unwrap_err();
    assert!(
        err.message_en().contains("not enabled"),
        "the refusal names the gate, not something vaguer: {}",
        err.message_en()
    );
}

/// A plugin that was never installed is refused by name — the caller named something that is not here.
#[test]
fn an_unknown_plugin_is_refused() {
    let store = store_with_plugin("invoke-unknown", "here", "#!/bin/sh\nexit 0\n");

    let err = plugin_invoke::call(&store, "elsewhere", &[], None).unwrap_err();
    assert_eq!(err.code(), "not_found_plugin_installed", "{}", err.message_en());
}

/// The whole contract in one run: the caller's words reach argv untouched, stdout comes back as the return
/// value, stderr as the diagnostic, and the stdin document leads with `v` (`AMB-D-349`).
#[test]
fn an_enabled_plugin_returns_its_stdout_and_relays_its_stderr() {
    let mut store = store_with_plugin(
        "invoke-run",
        "worktree",
        "#!/bin/sh\ncat >/dev/null\nprintf 'cd /w/%s\\n' \"$2\"\nprintf 'task %s ready\\n' \"$2\" 1>&2\n",
    );
    enable(&mut store, "worktree", Gate::Machine, &[], |_| true).unwrap();

    let outcome = plugin_invoke::call(&store, "worktree", &args(&["start", "123"]), None).unwrap();
    assert_eq!(
        outcome,
        CommandOutcome::Returned {
            value: "cd /w/123\n".to_string(),
            diagnostic: "task 123 ready\n".to_string(),
        }
    );
}

/// The stdin document a command plugin reads: `v` first, and nothing else for a plugin with no settings.
#[test]
fn the_plugin_reads_the_version_marker_on_stdin() {
    let mut store =
        store_with_plugin("invoke-stdin", "echoer", "#!/bin/sh\ncat\nexit 0\n");
    enable(&mut store, "echoer", Gate::Machine, &[], |_| true).unwrap();

    let outcome = plugin_invoke::call(&store, "echoer", &[], None).unwrap();
    assert_eq!(outcome.value(), Some(r#"{"v":1}"#), "the wire document leads with v");
}

/// A non-zero exit is a failed call: the return value is discarded and the diagnostic is what comes back
/// (`AMB-D-354`). The run is on the execution log either way (`AMB-D-361`).
#[test]
fn a_failing_plugin_discards_its_return_value_and_lands_on_the_log() {
    let mut store = store_with_plugin(
        "invoke-fail",
        "broken",
        "#!/bin/sh\ncat >/dev/null\nprintf 'half-written\\n'\nprintf 'boom\\n' 1>&2\nexit 3\n",
    );
    enable(&mut store, "broken", Gate::Machine, &[], |_| true).unwrap();

    let outcome = plugin_invoke::call(&store, "broken", &[], None).unwrap();
    assert_eq!(
        outcome,
        CommandOutcome::Failed { code: Some(3), diagnostic: "boom\n".to_string() }
    );
    assert_eq!(outcome.value(), None, "a failed call relays no value");

    let logged = amenbo_core::plugin_log::recent(&store.paths.plugin_log_file(), "broken");
    assert_eq!(logged.len(), 1, "the failed run is on the log");
    assert_eq!(logged[0].event, plugin_invoke::LOG_EVENT);
    assert_eq!(logged[0].code, Some(3));
}

/// The read-back path arrives in the child's own environment (`AMB-D-406`): the store to call `amenbo` into,
/// and the window to read it through. Read by a shell plugin with `$AMENBO_…` and nothing else — which is
/// the point of putting it in the environment rather than on stdin.
#[test]
fn a_called_plugin_reads_the_store_and_its_window_out_of_its_environment() {
    use amenbo_core::plugin_callback::{ALL_REACH, REACH_ENV, STORE_ENV};

    let mut store = store_with_plugin(
        "invoke-callback",
        "reader",
        &format!("#!/bin/sh\ncat >/dev/null\nprintf '%s|%s\\n' \"${STORE_ENV}\" \"${REACH_ENV}\"\n"),
    );
    enable(&mut store, "reader", Gate::Machine, &[], |_| true).unwrap();

    let outcome = plugin_invoke::call(&store, "reader", &[], None).unwrap();
    let base = store.paths.base_dir.to_string_lossy();
    // `scope: machine`, so the gate is the device's and so is the window.
    assert_eq!(outcome.value(), Some(format!("{base}|{ALL_REACH}\n").as_str()));
}
