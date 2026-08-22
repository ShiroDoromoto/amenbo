//! The command face driven end to end, against a real store and a real child process: an installed
//! plugin, its gate, and what comes back when it is called (`AMB-D-353`).
//!
//! The unit tests beside the module pin the pieces (the stdin document, what one outcome logs). These pin
//! the seam nobody else can: that the gate actually refuses, that a caller's words reach the plugin's
//! argv, and that a plugin's two streams come back as the two halves the contract names — stdout the
//! return value, stderr the diagnostic. The scripts make these `#[cfg(unix)]`, like the hook runner's.

#![cfg(unix)]

use amenbo_core::plugin_layer::Layer;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

use amenbo_core::config::Paths;
use amenbo_core::plugin_command::CommandOutcome;
use amenbo_core::plugin_invoke;
use amenbo_core::plugin_check::Checked;
use amenbo_core::plugin_trust::enable;
use amenbo_core::Store;

/// A store with one plugin installed by hand — the manifest an install would have written, plus a shell
/// script standing in for the executable — and one project to call it in, since every gate is a project's
/// (`AMB-D-434`).
fn store_with_plugin(label: &str, name: &str, script: &str) -> (Store, i64) {
    store_with_settings(label, name, script, None)
}

/// The same, for a plugin whose manifest declares a `settings` block — the check and the operations its
/// form may raise (`AMB-D-664`).
fn store_with_settings(
    label: &str,
    name: &str,
    script: &str,
    settings: Option<serde_json::Value>,
) -> (Store, i64) {
    store_with_scope(label, name, script, settings, None)
}

/// The same again, for a plugin whose manifest declares the layer it lives at (`AMB-D-601`) — `None` writes
/// no `scope`, which is the `project` every other test here works with.
fn store_with_scope(
    label: &str,
    name: &str,
    script: &str,
    settings: Option<serde_json::Value>,
    scope: Option<&str>,
) -> (Store, i64) {
    let base = amenbo_scratch::scratch(label);
    let paths = Paths::at(base.clone());
    let home = paths.plugin_dir(name);
    std::fs::create_dir_all(&home).unwrap();
    let mut manifest = serde_json::json!({
        "name": name,
        "desc": "a plugin that answers",
        "author": "amenbo",
        "repo": "alice/amenbo-plugin-test",
        "os": ["macos", "linux"],
        "category": "workflow",
        "url": "https://example.invalid/plugin.tar.gz",
        "checksum": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    });
    if let Some(settings) = settings {
        manifest["settings"] = settings;
    }
    if let Some(scope) = scope {
        manifest["scope"] = serde_json::Value::String(scope.to_string());
    }
    std::fs::write(home.join("manifest.json"), manifest.to_string()).unwrap();
    let program = home.join(name);
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut store = Store::open_at(paths).unwrap();
    let project = store
        .project_add(amenbo_core::ops::project::NewProject {
            name: "scenario".into(),
            view: amenbo_core::model::View::List,
            notes: String::new(),
            color: None,
        })
        .unwrap()
        .id;
    (store, project)
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

/// Installed is not enabled (`AMB-D-351`): a plugin nobody consented to run is refused, and the caller is
/// told so rather than handed an empty return value.
#[test]
fn an_installed_but_disabled_plugin_is_refused() {
    let (store, project) = store_with_plugin("invoke-gate", "quiet", "#!/bin/sh\nprintf 'cd /w\\n'\n");

    let err = plugin_invoke::call(&store, "quiet", &[], Some(project)).unwrap_err();
    assert!(
        err.message_en().contains("not enabled"),
        "the refusal names the gate, not something vaguer: {}",
        err.message_en()
    );
}

/// A plugin that was never installed is refused by name — the caller named something that is not here.
#[test]
fn an_unknown_plugin_is_refused() {
    let (store, project) = store_with_plugin("invoke-unknown", "here", "#!/bin/sh\nexit 0\n");

    let err = plugin_invoke::call(&store, "elsewhere", &[], Some(project)).unwrap_err();
    assert_eq!(err.code(), "not_found_plugin_installed", "{}", err.message_en());
}

/// The whole contract in one run: the caller's words reach argv untouched, stdout comes back as the return
/// value, stderr as the diagnostic, and the stdin document leads with `v` (`AMB-D-349`).
#[test]
fn an_enabled_plugin_returns_its_stdout_and_relays_its_stderr() {
    let (mut store, project) = store_with_plugin(
        "invoke-run",
        "worktree",
        "#!/bin/sh\ncat >/dev/null\nprintf 'cd /w/%s\\n' \"$2\"\nprintf 'task %s ready\\n' \"$2\" 1>&2\n",
    );
    enable(&mut store, "worktree", Layer::Project(project), &[], &amenbo_core::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared)
        .unwrap();

    let outcome =
        plugin_invoke::call(&store, "worktree", &args(&["start", "123"]), Some(project)).unwrap();
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
    let (mut store, project) =
        store_with_plugin("invoke-stdin", "echoer", "#!/bin/sh\ncat\nexit 0\n");
    enable(&mut store, "echoer", Layer::Project(project), &[], &amenbo_core::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared)
        .unwrap();

    let outcome = plugin_invoke::call(&store, "echoer", &[], Some(project)).unwrap();
    assert_eq!(outcome.value(), Some(r#"{"v":1}"#), "the wire document leads with v");
}

/// A non-zero exit is a failed call: the return value is discarded and the diagnostic is what comes back
/// (`AMB-D-354`). The run is on the execution log either way (`AMB-D-361`).
#[test]
fn a_failing_plugin_discards_its_return_value_and_lands_on_the_log() {
    let (mut store, project) = store_with_plugin(
        "invoke-fail",
        "broken",
        "#!/bin/sh\ncat >/dev/null\nprintf 'half-written\\n'\nprintf 'boom\\n' 1>&2\nexit 3\n",
    );
    enable(&mut store, "broken", Layer::Project(project), &[], &amenbo_core::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared)
        .unwrap();

    let outcome = plugin_invoke::call(&store, "broken", &[], Some(project)).unwrap();
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
    use amenbo_core::plugin_callback::{REACH_ENV, STORE_ENV};

    let (mut store, project) = store_with_plugin(
        "invoke-callback",
        "reader",
        &format!("#!/bin/sh\ncat >/dev/null\nprintf '%s|%s\\n' \"${STORE_ENV}\" \"${REACH_ENV}\"\n"),
    );
    enable(&mut store, "reader", Layer::Project(project), &[], &amenbo_core::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared)
        .unwrap();

    let outcome = plugin_invoke::call(&store, "reader", &[], Some(project)).unwrap();
    let base = store.paths.base_dir.to_string_lossy();
    // The gate it fired through is the project's, and so is the window (`AMB-D-406`).
    let reach = amenbo_core::idref::project(project);
    assert_eq!(outcome.value(), Some(format!("{base}|{reach}\n").as_str()));
}

/// A device-wide plugin runs from outside any project (`AMB-D-601`): the GUI's device row hands no project,
/// and the layer is already `Device`, so nothing may demand an id on the way to a window that opens anyway.
/// Before this, the read-back path asked for one regardless and the run died with
/// `invalid_plugin_project_required` — a refusal the declaration says should never have been reachable.
#[test]
fn a_device_wide_plugin_runs_with_no_project_and_reads_the_whole_device() {
    use amenbo_core::plugin_callback::{ALL_REACH, REACH_ENV, STORE_ENV};

    let (mut store, _project) = store_with_scope(
        "invoke-device-wide",
        "viewer",
        &format!("#!/bin/sh\ncat >/dev/null\nprintf '%s|%s\\n' \"${STORE_ENV}\" \"${REACH_ENV}\"\n"),
        None,
        Some("machine"),
    );
    enable(&mut store, "viewer", Layer::Device, &[], &amenbo_core::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared).unwrap();

    let outcome = plugin_invoke::call(&store, "viewer", &[], None).unwrap();
    let base = store.paths.base_dir.to_string_lossy();
    assert_eq!(outcome.value(), Some(format!("{base}|{ALL_REACH}\n").as_str()));
}

/// One operation the settings face declares: a test send, asking for a token at the press.
fn settings_block() -> serde_json::Value {
    serde_json::json!({
        "check": "config check",
        "actions": [{
            "cmd": "config test",
            "label": "Send a test message",
            "ask": [{ "key": "api_token", "label": "API token", "secret": true }],
        }],
    })
}

/// A press, end to end (`AMB-D-664`): the words the *manifest* declared reach argv, and the value the
/// person typed reaches the child on its environment — and nowhere else. The second press, with nothing
/// typed, is the proof that nothing was kept: the same button hands over an empty value rather than the
/// one before it.
#[test]
fn a_declared_operation_runs_the_manifests_words_with_the_value_asked_for_at_the_press() {
    let (mut store, project) = store_with_settings(
        "invoke-declared",
        "slack",
        "#!/bin/sh\ncat >/dev/null\nprintf '%s %s|%s\\n' \"$1\" \"$2\" \"${AMENBO_ASK_API_TOKEN}\"\n",
        Some(settings_block()),
    );
    enable(&mut store, "slack", Layer::Project(project), &[], &amenbo_core::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared)
        .unwrap();

    let supplied = BTreeMap::from([("api_token".to_string(), "xoxb-typed-once".to_string())]);
    let outcome =
        plugin_invoke::call_declared(&store, "slack", "config test", &supplied, Some(project))
            .unwrap();
    assert_eq!(outcome.value(), Some("config test|xoxb-typed-once\n"));

    // Nothing kept it: not the store, which is what the next press reads nothing out of, and not the
    // execution log, which holds the run's stderr and no value at all (`AMB-D-361`).
    let again =
        plugin_invoke::call_declared(&store, "slack", "config test", &BTreeMap::new(), Some(project))
            .unwrap();
    assert_eq!(again.value(), Some("config test|\n"), "a value that is never stored is never re-sent");
    let log = std::fs::read_to_string(store.paths.plugin_log_file()).unwrap();
    assert!(!log.contains("xoxb-typed-once"), "the asked value is off the log");
}

/// The run lands on the execution log under the face that raised it — a line nobody typed
/// (`AMB-D-361`/`AMB-D-664`).
#[test]
fn a_press_is_logged_as_a_settings_run() {
    let (mut store, project) = store_with_settings(
        "invoke-declared-log",
        "slack",
        "#!/bin/sh\ncat >/dev/null\nprintf 'no channel configured\\n' 1>&2\nexit 4\n",
        Some(settings_block()),
    );
    enable(&mut store, "slack", Layer::Project(project), &[], &amenbo_core::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared)
        .unwrap();

    let outcome =
        plugin_invoke::call_declared(&store, "slack", "config test", &BTreeMap::new(), Some(project))
            .unwrap();
    assert_eq!(outcome.value(), None, "a failed press relays no value");

    let logged = amenbo_core::plugin_log::recent(&store.paths.plugin_log_file(), "slack");
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0].event, plugin_invoke::SETTINGS_LOG_EVENT);
    assert_eq!(logged[0].code, Some(4));
    assert_eq!(logged[0].stderr, "no channel configured\n", "the author's line is the log's material");
}

/// What a form may raise is what the manifest named in advance (`AMB-D-522`): the plugin's own faces are
/// no more reachable from here than anyone else's.
#[test]
fn a_call_outside_the_declaration_is_refused_before_anything_runs() {
    let (mut store, project) = store_with_settings(
        "invoke-declared-undeclared",
        "slack",
        "#!/bin/sh\nprintf 'ran\\n'\n",
        Some(settings_block()),
    );
    enable(&mut store, "slack", Layer::Project(project), &[], &amenbo_core::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared)
        .unwrap();

    let err =
        plugin_invoke::call_declared(&store, "slack", "config wipe", &BTreeMap::new(), Some(project))
            .unwrap_err();
    assert!(err.message_en().contains("settings face"), "{}", err.message_en());
    assert!(
        amenbo_core::plugin_log::recent(&store.paths.plugin_log_file(), "slack").is_empty(),
        "a call that was never raised has no run to log",
    );
}

/// The gate holds on this face too (`AMB-D-351`): a plugin nobody has enabled runs nothing, declared or
/// not. The check that runs *at* the moment of enabling is the enabling path's own.
#[test]
fn a_disabled_plugin_raises_nothing_from_its_settings_face() {
    let (store, project) = store_with_settings(
        "invoke-declared-gate",
        "slack",
        "#!/bin/sh\nprintf 'ran\\n'\n",
        Some(settings_block()),
    );

    let err =
        plugin_invoke::call_declared(&store, "slack", "config test", &BTreeMap::new(), Some(project))
            .unwrap_err();
    assert!(err.message_en().contains("not enabled"), "{}", err.message_en());
}
