//! The settings check driven end to end, against a real store and a real child process: an author's own
//! code raised at the enable, and what the gate does with what it says (`AMB-D-664`).
//!
//! The unit tests beside the module pin the reading (what is a verdict, what one run logs). These pin the
//! seams nobody else can: that the check runs **before** the gate rather than being refused by it, that a
//! verdict which is not a yes leaves the plugin off, that a silent check costs the same, and that the
//! refusal a caller is handed names the settings without repeating a word the plugin wrote. The scripts
//! make these `#[cfg(unix)]`, like the command face's.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use amenbo_core::config::Paths;
use amenbo_core::plugin_check::{self, Checked, Silence};
use std::time::Duration;
use amenbo_core::plugin_layer::Layer;
use amenbo_core::plugin_trust::{effective_enabled_in, enable};
use amenbo_core::Store;

/// A store with one plugin installed by hand — a manifest declaring one setting and a check to raise over
/// it, plus a shell script standing in for the executable — and one project to enable it in.
fn store_with_check(label: &str, script: &str) -> (Store, i64) {
    store_with_check_at(label, script, None)
}

/// The same, for a plugin whose manifest declares the layer it lives at (`AMB-D-601`) — `None` writes no
/// `scope`, which is the `project` every road but the device's walks.
fn store_with_check_at(label: &str, script: &str, scope: Option<&str>) -> (Store, i64) {
    let base = amenbo_scratch::scratch(label);
    let paths = Paths::at(base.clone());
    let home = paths.plugin_dir("mail");
    std::fs::create_dir_all(&home).unwrap();
    let mut manifest = serde_json::json!({
        "name": "mail",
        "desc": "a plugin that judges its own settings",
        "author": "amenbo",
        "repo": "amenbo/amenbo-plugin-test",
        "os": ["macos", "linux"],
        "category": "workflow",
        "url": "https://example.invalid/plugin.tar.gz",
        "checksum": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        "config": [{ "key": "smtp_user", "label": "User" }],
        "settings": { "check": "config check" },
    });
    if let Some(scope) = scope {
        manifest["scope"] = serde_json::Value::String(scope.to_string());
    }
    std::fs::write(home.join("manifest.json"), manifest.to_string()).unwrap();
    let program = home.join("mail");
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

/// How long a check is given here. Not the shipped bound (`plugin_check::TIMEOUT`): every road but one is
/// about what a check *said*, and holding those to a two-second wall clock makes them a race with whatever
/// else the machine is doing. The road that is about the bound names its own, far below it.
const PATIENT: Duration = Duration::from_secs(60);

/// Raise the check the way a face does, and try the gate on what it said.
fn enable_after_check(store: &mut Store, project: i64) -> amenbo_core::error::Result<Checked> {
    enable_after_check_within(store, project, PATIENT)
}

/// The same, with the bound named — the one road that is about a check running too long.
fn enable_after_check_within(
    store: &mut Store,
    project: i64,
    bound: Duration,
) -> amenbo_core::error::Result<Checked> {
    let plugin = amenbo_core::plugin_installed::read(&store.paths, "mail")?;
    let checked = plugin_check::run(store, &plugin, Some(project), bound)?;
    enable(store, "mail", Layer::Project(project), &plugin.manifest.fields(), |_| true, &checked)?;
    Ok(checked)
}

/// A shell plugin that answers `config check` with `verdict` and ignores every other call.
fn answering(verdict: &str) -> String {
    format!("#!/bin/sh\ncat >/dev/null\nprintf '%s' '{verdict}'\n")
}

/// A check that says yes opens the gate — and it ran while the plugin was still off, which is the whole
/// point: the hand that pressed enable is the consent (`AMB-D-351`).
#[test]
fn a_check_that_says_yes_opens_the_gate() {
    let (mut store, project) =
        store_with_check("check-yes", &answering(r#"{"v":1,"ok":true,"message":"signed in"}"#));

    let checked = enable_after_check(&mut store, project).unwrap();

    assert_eq!(checked.verdict().unwrap().message.as_deref(), Some("signed in"));
    assert!(effective_enabled_in(&store, "mail", Layer::Project(project)).unwrap());
}

/// A check that says no leaves the plugin off, and the refusal names the setting it spoke about — with
/// none of the author's own sentences in it (`AMB-D-664`).
#[test]
fn a_check_that_says_no_holds_the_gate_shut_and_keeps_its_sentences_off_the_refusal() {
    let (mut store, project) = store_with_check(
        "check-no",
        &answering(
            r#"{"v":1,"ok":false,"fields":{"smtp_user":"no such mailbox"},"message":"cannot sign in"}"#,
        ),
    );

    let err = enable_after_check(&mut store, project).unwrap_err();

    assert!(
        err.message_en().contains("smtp_user"),
        "the setting the check named is what a caller is told: {}",
        err.message_en()
    );
    for said in ["no such mailbox", "cannot sign in"] {
        assert!(
            !format!("{err:?}").contains(said),
            "the author's sentence is the settings form's, not this refusal's: {err:?}"
        );
    }
    assert!(!effective_enabled_in(&store, "mail", Layer::Project(project)).unwrap());
}

/// A check that exits non-zero has checked nothing, so the gate stays shut — and the run is on the
/// execution log, which is where *why was I not allowed to enable this* is answered (`AMB-D-361`).
#[test]
fn a_check_that_fails_has_checked_nothing_and_lands_on_the_log() {
    let (mut store, project) = store_with_check(
        "check-failed",
        "#!/bin/sh\ncat >/dev/null\nprintf '{\"v\":1,\"ok\":true}'\nprintf 'boom\\n' 1>&2\nexit 3\n",
    );

    let err = enable_after_check(&mut store, project).unwrap_err();

    assert!(err.message_en().contains("did not answer"), "{}", err.message_en());
    assert!(!effective_enabled_in(&store, "mail", Layer::Project(project)).unwrap());

    let logged = amenbo_core::plugin_log::recent(&store.paths.plugin_log_file(), "mail");
    assert_eq!(logged.len(), 1, "the run is on the log whichever way it ended");
    assert_eq!(logged[0].event, amenbo_core::plugin_invoke::SETTINGS_LOG_EVENT);
    assert_eq!(logged[0].code, Some(3));
    assert_eq!(logged[0].stderr, "boom\n");
}

/// A check that writes something this build cannot read has checked nothing either — a clean exit is not
/// a verdict (`AMB-D-354`).
#[test]
fn a_check_that_answers_with_nonsense_holds_the_gate_shut() {
    let (mut store, project) = store_with_check("check-nonsense", &answering("looks fine to me"));

    let err = enable_after_check(&mut store, project).unwrap_err();

    assert!(err.message_en().contains("did not answer"), "{}", err.message_en());
    assert!(!effective_enabled_in(&store, "mail", Layer::Project(project)).unwrap());
}

/// A check that overruns the bound is killed and the gate stays shut: somebody is waiting in front of a
/// screen, so a wedged check costs a refusal rather than a frozen window.
#[test]
fn a_check_that_overruns_is_killed_and_refuses() {
    let (mut store, project) =
        store_with_check("check-slow", "#!/bin/sh\ncat >/dev/null\nsleep 30\n");

    let err =
        enable_after_check_within(&mut store, project, Duration::from_millis(200)).unwrap_err();

    assert!(err.message_en().contains("did not answer"), "{}", err.message_en());
    assert!(!effective_enabled_in(&store, "mail", Layer::Project(project)).unwrap());
    let logged = amenbo_core::plugin_log::recent(&store.paths.plugin_log_file(), "mail");
    assert_eq!(logged[0].outcome, amenbo_core::plugin_log::Outcome::TimedOut);
}

/// The check is the plugin's own command face: it is handed the call the manifest named on argv, and the
/// settings on stdin, exactly as `plugin run` would (`AMB-D-353` / `AMB-D-356`). It answers `ok` with what
/// it was given, so the assertion is what the plugin saw.
#[test]
fn a_check_reads_the_call_and_the_settings_the_way_any_other_run_does() {
    let (mut store, project) = store_with_check(
        "check-heard",
        // The verdict's message is what this run heard: the two words of the call, and the document on
        // stdin with its quotes taken out, so that what it heard can ride inside a JSON string.
        "#!/bin/sh\nheard=$(cat | tr -d '\"')\nprintf '{\"v\":1,\"ok\":true,\"message\":\"%s %s %s\"}' \"$1\" \"$2\" \"$heard\"\n",
    );
    let field = amenbo_core::plugin_manifest::ConfigField::new("smtp_user", "User");
    amenbo_core::plugin_config::set(&mut store, &field, "mail", Layer::Project(project), "ada")
        .unwrap();

    let checked = enable_after_check(&mut store, project).unwrap();

    assert_eq!(
        checked.verdict().unwrap().message.as_deref(),
        Some("config check {v:1,config:{smtp_user:ada}}"),
    );
}

/// The same call is raised while the plugin is enabled — the moment after a save (`AMB-D-664`). It says
/// what it says; the gate is not moved by it, because nothing here switches a plugin off behind the user.
#[test]
fn the_check_after_a_save_answers_without_moving_the_gate() {
    let (mut store, project) = store_with_check("check-after-save", &answering(r#"{"v":1,"ok":true}"#));
    enable_after_check(&mut store, project).unwrap();

    // The values changed under it, and now the author's code says no.
    std::fs::write(
        store.paths.plugin_dir("mail").join("mail"),
        answering(r#"{"v":1,"ok":false,"fields":{"smtp_user":"no such mailbox"}}"#),
    )
    .unwrap();
    let plugin = amenbo_core::plugin_installed::read(&store.paths, "mail").unwrap();
    let checked = plugin_check::run(&store, &plugin, Some(project), PATIENT).unwrap();

    assert!(!checked.opens_the_gate(), "the answer is a no");
    assert_eq!(checked.verdict().unwrap().fields["smtp_user"], "no such mailbox");
    assert!(
        effective_enabled_in(&store, "mail", Layer::Project(project)).unwrap(),
        "an enabled plugin is not switched off behind the user",
    );
}

/// A plugin whose manifest names no check is unchanged: nothing is spawned, and the gate is the presence
/// check it always was.
#[test]
fn a_plugin_that_declares_no_check_raises_none() {
    let (mut store, project) = store_with_check("check-none", "#!/bin/sh\nexit 9\n");
    let manifest = amenbo_core::plugin_installed::manifest_path(&store.paths, "mail");
    let mut raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    raw.as_object_mut().unwrap().remove("settings");
    std::fs::write(&manifest, raw.to_string()).unwrap();

    let checked = enable_after_check(&mut store, project).unwrap();

    assert_eq!(checked, Checked::NotDeclared);
    assert_eq!(checked.silence(), None);
    assert!(effective_enabled_in(&store, "mail", Layer::Project(project)).unwrap());
    assert!(
        amenbo_core::plugin_log::recent(&store.paths.plugin_log_file(), "mail").is_empty(),
        "a check nobody declared leaves no run behind",
    );
}

/// A plugin that lost its executable is a check that would not start — fail-closed, like every other
/// silence (`AMB-D-354`).
#[test]
fn a_check_that_cannot_start_refuses_too() {
    let (mut store, project) = store_with_check("check-unlaunchable", &answering(r#"{"v":1,"ok":true}"#));
    std::fs::set_permissions(
        store.paths.plugin_dir("mail").join("mail"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();

    let plugin = amenbo_core::plugin_installed::read(&store.paths, "mail").unwrap();
    let checked = plugin_check::run(&store, &plugin, Some(project), PATIENT).unwrap();

    assert_eq!(checked.silence(), Some(Silence::NotLaunched));
    let err = enable(
        &mut store,
        "mail",
        Layer::Project(project),
        &plugin.manifest.fields(),
        |_| true,
        &checked,
    )
    .unwrap_err();
    assert!(err.message_en().contains("did not answer"), "{}", err.message_en());
    assert!(!effective_enabled_in(&store, "mail", Layer::Project(project)).unwrap());
}

/// The same check, raised from a face standing in no project — which is the only way a device-wide plugin
/// is ever enabled (`AMB-D-601`). The GUI's device row hands no project and the CLI outside a bound folder
/// has none to hand, so every road to this gate arrives without one; a build that asked for a project here
/// refused the enable outright, on a plugin whose window is the whole device and needs none.
#[test]
fn a_device_wide_check_runs_from_no_project_and_opens_the_device_gate() {
    let (mut store, _project) = store_with_check_at(
        "check-device-yes",
        &answering(r#"{"v":1,"ok":true,"message":"signed in"}"#),
        Some("machine"),
    );

    let plugin = amenbo_core::plugin_installed::read(&store.paths, "mail").unwrap();
    let checked = plugin_check::run(&store, &plugin, None, PATIENT).unwrap();
    enable(&mut store, "mail", Layer::Device, &plugin.manifest.fields(), |_| true, &checked).unwrap();

    assert_eq!(checked.verdict().unwrap().message.as_deref(), Some("signed in"));
    assert!(effective_enabled_in(&store, "mail", Layer::Device).unwrap());
}

/// And the same gate held shut by the same judgement. Fail-closed is not something the project layer owns:
/// a device-wide check that says no leaves the plugin off for the whole machine, which is the half that a
/// road only walking the yes could not tell apart from a check nobody raised.
#[test]
fn a_device_wide_check_that_says_no_holds_the_device_gate_shut() {
    let (mut store, _project) = store_with_check_at(
        "check-device-no",
        &answering(r#"{"v":1,"ok":false,"fields":{"smtp_user":"no such mailbox"}}"#),
        Some("machine"),
    );

    let plugin = amenbo_core::plugin_installed::read(&store.paths, "mail").unwrap();
    let checked = plugin_check::run(&store, &plugin, None, PATIENT).unwrap();
    let err = enable(&mut store, "mail", Layer::Device, &plugin.manifest.fields(), |_| true, &checked)
        .unwrap_err();

    assert!(err.message_en().contains("smtp_user"), "{}", err.message_en());
    assert!(!effective_enabled_in(&store, "mail", Layer::Device).unwrap());
}
