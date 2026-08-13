//! The command face's **caller** — resolve one installed plugin, run it synchronously, and hand its
//! stdout back as the invoking command's return value (`AMB-D-346`'s `run`).
//!
//! [`plugin_command`] is the command contract as *policy* over a finished run
//! (`AMB-D-353`/`AMB-D-354`): stdout is the machine return value, stderr is diagnostics, the exit code is
//! the verdict. It reads no store and knows nothing about what is installed. This module is the half that
//! stands between that policy and the machine: given a plugin's **name** and the arguments to hand it, it
//! assembles the invocation from the same state the observation face resolves from
//! ([`plugin_subscribe`](crate::plugin_subscribe)), runs it, and records the run.
//!
//! **The same four gates as an observation, minus the subscription.** A command is an explicit call, so
//! there is no event to be subscribed to — the caller named the plugin. What is left is:
//!
//! - **installed** — read by name off disk ([`plugin_installed::read`]),
//!   which errors rather than shrugging: a caller who named a plugin is owed the reason it did not run;
//! - **enabled** — the gate at the layer its author declared (`AMB-D-434`/`AMB-D-351`/`AMB-D-601`: the
//!   project it is called in, or the device; `install ≠ enable`, and running a plugin's arbitrary code is
//!   exactly what the consent is for);
//! - **compatible** — this amenbo speaks the payload contract it reads and clears its version floor
//!   (`AMB-D-359`);
//! - **its config resolves** — secrets to environment variables, text settings to stdin (`AMB-D-356`).
//!
//! Every one of them **refuses** here rather than skipping. That is the difference between the faces: an
//! observation drops one plugin and fires the rest (`AMB-D-352` — best effort, nobody is waiting), while a
//! command has one plugin and a caller holding out a hand for its return value. Silence would read as an
//! empty return.
//!
//! **What the plugin receives.** Its arguments verbatim on argv — the caller's words, which amenbo neither
//! parses nor rewrites (`AMB-D-346`: the workflow-specific meaning is the plugin's) — and on stdin the
//! smallest document the wire contract allows: `v` first (`AMB-D-349`), plus its own non-secret config under
//! `config` when it has any ([`command_stdin`]). There is no event payload, because no event fired.
//!
//! ```json
//! { "v": 1, "config": { "base": "main" } }
//! ```
//!
//! In its environment it receives its secret settings (`AMB-D-356`) and the read-back path
//! ([`plugin_callback`], `AMB-D-406`): the store to call `amenbo` into, and the window to read it through —
//! the gate this run just passed, since what a plugin may observe is what it may read. A plugin declaring
//! `scope: machine` passed the device's gate, so its window is the device (`AMB-D-601`).
//!
//! **The run is logged like any other** (`AMB-D-361`): the execution log answers *why did nothing happen*,
//! and a command that refused to launch or exited non-zero is as much that question's material as a silent
//! hook. It is filed under the [`LOG_EVENT`] pseudo-event, since no event named this run.
//!
//! **The settings face raises calls through here too** ([`call_declared`], `AMB-D-664`). The form has no
//! second protocol: an operation a user presses is this same command run, through these same gates, with
//! the same values already injected. Two things are its own. The words are the **manifest's** — `cmd` names
//! one of the calls the author declared under `settings`, and a caller chooses among them rather than
//! writing one, so nothing a form can reach takes arguments from whoever raised it (`AMB-D-522`). And a
//! press may carry values that **no store ever holds** — a token pasted once — handed to that one child on
//! its environment ([`plugin_inject::asked`]) and gone when it exits. Those runs land on the log under
//! [`SETTINGS_LOG_EVENT`], since nobody typed them.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::error::Result;
use crate::plugin_command::{self, CommandOutcome};
use crate::plugin_callback;
use crate::plugin_exec::PluginInvocation;
use crate::plugin_inject;
use crate::plugin_installed;
use crate::plugin_log;
use crate::plugin_payload::VERSION;
use crate::plugin_trust::{effective_enabled_in, require_project};
use crate::store::Store;

/// What the execution log records in the `event` column for a command run. Not one of the v1 event names
/// ([`V1_EVENTS`](crate::plugin_payload::V1_EVENTS), all of them dotted) — nothing fired this run, a caller
/// asked for it — so the log stays readable as "which face was this" without pretending an event happened.
pub const LOG_EVENT: &str = "command";

/// What the execution log records for a run the **settings face** raised (`AMB-D-664`) — a check after a
/// save, an operation a user pressed. A line of its own beside [`LOG_EVENT`]: both are the same face
/// running the same way, and the one question the log is for (*why did nothing happen*) is answered
/// differently by "the CLI called this" and "the form did" — nobody typed the second one.
pub const SETTINGS_LOG_EVENT: &str = "settings";

/// The JSON document a command plugin reads on stdin: `v` first (`AMB-D-349`), and the plugin's own
/// non-secret config under `config` when it has any (`AMB-D-356`).
///
/// An empty config adds no key, exactly as the observation face's payload does — so a plugin with no text
/// settings receives the bare version marker rather than an empty object to special-case.
///
/// Built through a struct rather than by inserting into a [`Map`]: a `serde_json` object is ordered by key
/// unless the crate is built to preserve insertion order, so a document assembled that way emits `config`
/// ahead of `v` — the one thing `AMB-D-349` asks not to happen. A struct's fields serialize in declaration
/// order, which is the wire order, and the same reason [`Payload`](crate::plugin_payload::Payload) is one.
pub fn command_stdin(config: Map<String, Value>) -> String {
    #[derive(serde::Serialize)]
    struct CommandInput {
        v: u32,
        #[serde(skip_serializing_if = "Map::is_empty")]
        config: Map<String, Value>,
    }
    serde_json::to_string(&CommandInput { v: VERSION, config }).unwrap_or_default()
}

/// Assemble the invocation for a command run: the gates, the config injection, and the stdin document.
///
/// Split from [`call`] so the assembly is testable without spawning anything, and so a caller that wants to
/// inspect what would run (or run it under its own policy — a timeout is the caller's, `AMB-D-352`) can.
/// `project` is the run's project context, which a command has and an observation does not: an invocation
/// happens inside one bound folder, so the window a run reads the store through is answerable here. The
/// gate and the settings are answered at the layer the author declared instead (`AMB-D-601`) — for a
/// project's plugin that is this same folder's, and outside one there is no gate to ask (`AMB-D-434`), which
/// is what the refusal says.
pub fn prepare(
    store: &Store,
    name: &str,
    args: &[String],
    project: Option<i64>,
) -> Result<PluginInvocation> {
    assemble(store, name, Call::Free(args), Gate::Open, project)
}

/// Assemble the invocation for a call the **settings face** raises (`AMB-D-664`): the plugin's own
/// `settings.check`, or one of its `settings.actions`, named by the `cmd` the manifest declared.
///
/// The difference from [`prepare`] is what may be called and what rides along. The words are the
/// manifest's — `cmd` is looked up among the declarations and its own words become argv, so a caller
/// chooses *which* declared call runs and never what it is handed (`AMB-D-522`: a call taking a caller's
/// arguments stays `plugin run`'s, on the CLI). A `cmd` the manifest does not declare is refused. On top
/// of the config every run receives, the values `supplied` at the press ride as environment variables for
/// this run alone and are stored nowhere ([`plugin_inject::asked`]).
///
/// The gate is the same one, and it holds: what a form raises, it raises on an enabled plugin. The one
/// call that runs before the gate is the check at the moment of enabling — that is the enabling path's
/// own (`AMB-D-351`: the hand that pressed enable is the consent), not this one.
pub fn prepare_declared(
    store: &Store,
    name: &str,
    cmd: &str,
    supplied: &BTreeMap<String, String>,
    project: Option<i64>,
) -> Result<PluginInvocation> {
    assemble(store, name, Call::Declared { cmd, supplied }, Gate::Open, project)
}

/// Assemble the plugin's own `settings.check` (`AMB-D-664`) — the one call raised **without asking the
/// gate**, and the reason it is not [`prepare_declared`]'s road.
///
/// Everything else is that road's: `cmd` is looked up among the manifest's declarations, the settings are
/// injected, the read-back path is the same. What differs is the gate. The moment that decides this is the
/// enable, where there is no open gate to find: running this code is what the hand on the switch consented
/// to (`AMB-D-351`), and what the gate does is what this run's answer decides
/// ([`crate::plugin_check`]). The same road serves the check after a save, where the gate is open anyway.
///
/// It carries no asked values: a check judges what is saved, and what a press asks for is not saved.
pub fn prepare_check(
    store: &Store,
    name: &str,
    cmd: &str,
    project: Option<i64>,
) -> Result<PluginInvocation> {
    let supplied = BTreeMap::new();
    assemble(store, name, Call::Declared { cmd, supplied: &supplied }, Gate::Pressed, project)
}

/// Whether a run has to find the gate already open (`AMB-D-351`).
enum Gate {
    /// Every ordinary road: a plugin runs because somebody enabled it, and a shut gate refuses the run.
    Open,
    /// The one exception (`AMB-D-664`): the check raised while an enable is being decided. There is no gate
    /// to find open yet — the consent is the press itself, and what this run answers is whether the gate
    /// opens at all.
    Pressed,
}

/// What a run was asked to call — the two faces, as the assembly reads them.
enum Call<'a> {
    /// `plugin run`'s: the caller's own words, handed to the plugin verbatim (`AMB-D-346`).
    Free(&'a [String]),
    /// The settings face's (`AMB-D-664`): a call the manifest declared, and the values asked for at the
    /// press.
    Declared { cmd: &'a str, supplied: &'a BTreeMap<String, String> },
}

impl Call<'_> {
    /// The pseudo-event this call is filed under on the execution log.
    fn log_event(&self) -> &'static str {
        match self {
            Call::Free(_) => LOG_EVENT,
            Call::Declared { .. } => SETTINGS_LOG_EVENT,
        }
    }
}

/// One declared call, resolved into what a run is made of (`AMB-D-664`) — the words the manifest wrote,
/// and the environment this press's asked values ride on.
#[derive(Debug)]
struct Raised {
    /// The declared line's own words, as argv.
    args: Vec<String>,
    /// `(name, value)` per declared `ask`, ready to set on the invocation.
    asked: Vec<(String, String)>,
}

/// The words a declared call runs with, and the environment its asked values ride on (`AMB-D-664`).
///
/// The lookup is the whole guard: `cmd` must be the manifest's `settings.check` or one of its
/// `settings.actions[].cmd`, matched whole. A plugin that declares no `settings` block declares no call
/// this face can raise, and says so the same way.
///
/// The declared line is split into words and handed over as it stands — amenbo neither parses nor rewrites
/// what a plugin's own command face is called with (`AMB-D-346`), and the line is held to the call grammar
/// before it ever reaches a manifest on disk (`AMB-D-572`, [`crate::plugin_validate`]).
fn declared_call(
    manifest: &crate::plugin_manifest::Manifest,
    name: &str,
    cmd: &str,
    supplied: &BTreeMap<String, String>,
) -> Result<Raised> {
    let ask = manifest
        .settings
        .as_ref()
        .and_then(|settings| {
            if settings.check.as_deref() == Some(cmd) {
                return Some(&[][..]);
            }
            settings.actions.iter().find(|action| action.cmd == cmd).map(|action| &action.ask[..])
        })
        .ok_or_else(|| {
            crate::error::Error::invalid(format!(
                "plugin '{name}' does not offer '{cmd}' on its settings face — only the check and the \
                 operations its manifest declares can be raised from there"
            ))
        })?;
    Ok(Raised {
        args: cmd.split_whitespace().map(str::to_string).collect(),
        asked: plugin_inject::asked(ask, supplied)?,
    })
}

/// The body of [`prepare`] and [`prepare_declared`] — the gates and the injection they share, and the one
/// question they answer differently: what this run is called with.
fn assemble(
    store: &Store,
    name: &str,
    call: Call<'_>,
    gate: Gate,
    project: Option<i64>,
) -> Result<PluginInvocation> {
    let plugin = plugin_installed::read(&store.paths, name)?;
    crate::plugin_compat::check(&plugin.manifest).map_err(|why| why.into_error(name))?;

    // The gate and the settings are read at the layer the author declared, not at the folder this ran in
    // (`AMB-D-601`); for the ordinary `scope: project` plugin the two are the same thing.
    let layer = crate::plugin_layer::Layer::of(plugin.manifest.scope, project)?;
    if matches!(gate, Gate::Open) && !effective_enabled_in(store, name, layer)? {
        let cmd = crate::config::Paths::command_name();
        return Err(crate::error::Error::invalid(
            format!(
                "plugin '{name}' is installed but not enabled — `{cmd} plugin enable {name}` opens its gate"
            ),
        ));
    }

    // What this run is called with: the caller's words, or the ones the manifest declared — and, only on
    // the declared road, the values this press asks for and nothing keeps (`AMB-D-664`).
    let Raised { args, asked } = match call {
        Call::Free(args) => Raised { args: args.to_vec(), asked: Vec::new() },
        Call::Declared { cmd, supplied } => declared_call(&plugin.manifest, name, cmd, supplied)?,
    };

    let injection = plugin_inject::resolve(store, name, &plugin.manifest.config, layer)?;
    let mut invocation =
        PluginInvocation::new(plugin.program).stdin_json(command_stdin(injection.text));
    for arg in args {
        invocation = invocation.arg(arg);
    }
    for (key, value) in injection.env.into_iter().chain(asked) {
        invocation = invocation.env(key, value);
    }
    // The read-back path (`AMB-D-406`): the store to call into, and the window to read it through — which is
    // the gate this run just passed, since what a plugin may observe is what it may read. Which gate that is
    // follows the layer the author declared (`AMB-D-601`): this project, or the whole device.
    let project = require_project(project)?;
    for (key, value) in plugin_callback::env(
        &store.paths.base_dir,
        plugin_callback::reach_of(plugin.manifest.scope, project),
    ) {
        invocation = invocation.env(key, value);
    }
    Ok(invocation)
}

/// Run a command plugin by name and read its outcome under the command contract
/// (`AMB-D-353`/`AMB-D-354`).
///
/// Blocks until the plugin exits — the caller is waiting for the return value. A gate that refuses, or a
/// plugin that cannot be spawned, is the returned `Err`; a plugin that ran and exited non-zero is an `Ok`
/// holding [`CommandOutcome::Failed`], since "it ran and failed" is an outcome the caller is owed, stderr
/// and all.
pub fn call(
    store: &Store,
    name: &str,
    args: &[String],
    project: Option<i64>,
) -> Result<CommandOutcome> {
    run(store, name, Call::Free(args), project)
}

/// Raise a call the settings face declared (`AMB-D-664`) and read its outcome under the same command
/// contract [`call`] reads (`AMB-D-353`/`AMB-D-354`).
///
/// `cmd` names one of the plugin's declarations ([`prepare_declared`]); `supplied` carries the values it
/// asked for at the press, which reach the child and go no further — this face stores nothing.
///
/// Blocks until the plugin exits, with no bound on the wait: an operation is a person pressing a button
/// and then watching, and what it is doing behind that (a `setup` standing a Worker up) is the author's
/// to take as long as it takes (`AMB-D-664`). The bounded one is the check at the moment of enabling,
/// which is the enabling path's own.
pub fn call_declared(
    store: &Store,
    name: &str,
    cmd: &str,
    supplied: &BTreeMap<String, String>,
    project: Option<i64>,
) -> Result<CommandOutcome> {
    run(store, name, Call::Declared { cmd, supplied }, project)
}

/// The body of [`call`] and [`call_declared`]: assemble, run to completion, and record the run on the
/// execution log under the face that raised it (`AMB-D-361`).
fn run(
    store: &Store,
    name: &str,
    call: Call<'_>,
    project: Option<i64>,
) -> Result<CommandOutcome> {
    let event = call.log_event();
    let invocation = assemble(store, name, call, Gate::Open, project)?;
    let log = store.paths.plugin_log_file();
    let outcome = match plugin_command::run(&invocation) {
        Ok(outcome) => outcome,
        Err(e) => {
            plugin_log::record(
                &log,
                &plugin_log::Run {
                    plugin: name.to_string(),
                    event,
                    outcome: plugin_log::Outcome::NotLaunched,
                    code: None,
                    elapsed: std::time::Duration::ZERO,
                    stderr: e.to_string(),
                },
            );
            return Err(e.into());
        }
    };
    plugin_log::record(&log, &run_line(name, event, &outcome));
    Ok(outcome)
}

/// One finished command run as the execution log takes it. The plugin's stderr is logged whichever way it
/// exited — a successful run's summary is as much of an answer to *what did this do* as a failure's reason,
/// and the log holds only stderr, never the return value (a caller already has that). The duration is not
/// carried: the command face hands back a [`CommandOutcome`], which is the contract's whole reading of a
/// run and holds no clock — how long it took is a hook's timeout material, not a command's.
fn run_line(name: &str, event: &'static str, outcome: &CommandOutcome) -> plugin_log::Run {
    let (result, code, stderr) = match outcome {
        CommandOutcome::Returned { diagnostic, .. } => {
            (plugin_log::Outcome::Ok, Some(0), diagnostic.clone())
        }
        CommandOutcome::Failed { code, diagnostic } => {
            (plugin_log::Outcome::Failed, *code, diagnostic.clone())
        }
    };
    plugin_log::Run {
        plugin: name.to_string(),
        event,
        outcome: result,
        code,
        elapsed: std::time::Duration::ZERO,
        stderr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stdin_document_leads_with_the_version_and_omits_an_empty_config() {
        assert_eq!(command_stdin(Map::new()), r#"{"v":1}"#);
    }

    #[test]
    fn a_plugins_text_config_rides_under_the_config_key() {
        let mut config = Map::new();
        config.insert("base".to_string(), Value::from("main"));
        assert_eq!(command_stdin(config), r#"{"v":1,"config":{"base":"main"}}"#);
    }

    #[test]
    fn a_clean_run_logs_as_ok_and_keeps_its_summary() {
        let line = run_line(
            "worktree",
            LOG_EVENT,
            &CommandOutcome::Returned {
                value: "cd /w\n".to_string(),
                diagnostic: "task 7 ready".to_string(),
            },
        );
        assert_eq!(line.plugin, "worktree");
        assert_eq!(line.event, LOG_EVENT);
        assert_eq!(line.outcome, plugin_log::Outcome::Ok);
        assert_eq!(line.code, Some(0));
        assert_eq!(line.stderr, "task 7 ready", "the summary is the log's material too");
    }

    #[test]
    fn a_failed_run_logs_its_code_and_keeps_the_diagnostic() {
        let line = run_line(
            "worktree",
            LOG_EVENT,
            &CommandOutcome::Failed { code: Some(3), diagnostic: "boom".to_string() },
        );
        assert_eq!(line.outcome, plugin_log::Outcome::Failed);
        assert_eq!(line.code, Some(3));
        assert_eq!(line.stderr, "boom", "the author's stderr is what the log is for");
    }

    #[test]
    fn a_signalled_death_logs_as_failed_with_no_code() {
        let line = run_line(
            "worktree",
            LOG_EVENT,
            &CommandOutcome::Failed { code: None, diagnostic: String::new() },
        );
        assert_eq!(line.outcome, plugin_log::Outcome::Failed);
        assert_eq!(line.code, None);
    }

    /// A run the form raised is one line on the log, and it says which face raised it — the whole of what
    /// `AMB-D-664` adds to the execution log.
    #[test]
    fn a_run_the_settings_face_raised_is_logged_as_its_own_kind_of_line() {
        let line = run_line(
            "slack",
            SETTINGS_LOG_EVENT,
            &CommandOutcome::Returned { value: String::new(), diagnostic: "sent".to_string() },
        );
        assert_eq!(line.event, SETTINGS_LOG_EVENT);
        assert_ne!(SETTINGS_LOG_EVENT, LOG_EVENT, "the two faces are told apart on the log");
    }

    /// A manifest as it is read off disk, carrying whatever `settings` block is handed in (`None` for a
    /// plugin that declares none).
    fn manifest(settings: Option<Value>) -> crate::plugin_manifest::Manifest {
        let mut doc = serde_json::json!({
            "name": "slack",
            "desc": "post to a channel",
            "author": "amenbo",
            "repo": "alice/amenbo-plugin-slack",
            "os": ["macos"],
            "category": "notify",
            "url": "https://example.test/x.tar.gz",
            "checksum": format!("sha256:{}", "a".repeat(64)),
        });
        if let Some(settings) = settings {
            doc["settings"] = settings;
        }
        serde_json::from_value(doc).unwrap()
    }

    /// A `settings` block with both faces on it: one check, and one operation asking for a value nothing
    /// keeps.
    fn with_settings() -> crate::plugin_manifest::Manifest {
        manifest(Some(serde_json::json!({
            "check": "config check",
            "actions": [{
                "cmd": "config test",
                "label": "Send a test message",
                "ask": [{ "key": "api_token", "label": "API token", "secret": true }],
            }],
        })))
    }

    /// The declared line's own words become argv, and the values the press collected ride on the
    /// environment under their own prefix (`AMB-D-664`).
    #[test]
    fn a_declared_call_runs_the_words_the_manifest_wrote() {
        let supplied = BTreeMap::from([("api_token".to_string(), "t-1".to_string())]);
        let Raised { args, asked } =
            declared_call(&with_settings(), "slack", "config test", &supplied).unwrap();
        assert_eq!(args, vec!["config".to_string(), "test".to_string()]);
        assert_eq!(asked, vec![("AMENBO_ASK_API_TOKEN".to_string(), "t-1".to_string())]);
    }

    /// The check is a declaration like any other, and it asks for nothing.
    #[test]
    fn the_check_is_reachable_by_the_line_that_declared_it() {
        let Raised { args, asked } =
            declared_call(&with_settings(), "slack", "config check", &BTreeMap::new()).unwrap();
        assert_eq!(args, vec!["config".to_string(), "check".to_string()]);
        assert!(asked.is_empty(), "a check asks for nothing at the press");
    }

    /// Only what the manifest named is reachable from the form (`AMB-D-522`) — a line the author never
    /// wrote is refused, whether it is the plugin's own face or not.
    #[test]
    fn a_call_the_manifest_never_declared_is_refused() {
        let err = declared_call(&with_settings(), "slack", "config wipe", &BTreeMap::new())
            .unwrap_err();
        assert!(
            err.message_en().contains("does not offer"),
            "the refusal names the face: {}",
            err.message_en()
        );
    }

    /// A plugin with no `settings` block offers this face nothing at all.
    #[test]
    fn a_plugin_with_no_settings_block_offers_the_form_nothing() {
        let bare = manifest(None);
        assert!(declared_call(&bare, "quiet", "config check", &BTreeMap::new()).is_err());
    }

    /// A value nobody asked for is refused rather than quietly handed over.
    #[test]
    fn a_value_the_operation_never_asked_for_is_refused() {
        let supplied = BTreeMap::from([("password".to_string(), "hunter2".to_string())]);
        let err = declared_call(&with_settings(), "slack", "config test", &supplied).unwrap_err();
        assert!(err.message_en().contains("password"), "{}", err.message_en());
    }
}
