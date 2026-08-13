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
    let plugin = plugin_installed::read(&store.paths, name)?;
    crate::plugin_compat::check(&plugin.manifest).map_err(|why| why.into_error(name))?;

    // The gate and the settings are read at the layer the author declared, not at the folder this ran in
    // (`AMB-D-601`); for the ordinary `scope: project` plugin the two are the same thing.
    let layer = crate::plugin_layer::Layer::of(plugin.manifest.scope, project)?;
    if !effective_enabled_in(store, name, layer)? {
        let cmd = crate::config::Paths::command_name();
        return Err(crate::error::Error::invalid(
            format!(
                "plugin '{name}' is installed but not enabled — `{cmd} plugin enable {name}` opens its gate"
            ),
        ));
    }

    assemble(store, &plugin, args, project, layer)
}

/// Everything [`prepare`] does **but the gate** — the assembly on its own, for the one call amenbo raises
/// before a plugin is enabled (`AMB-D-664`: the settings check, [`crate::plugin_check`]).
///
/// It is not a way past the consent. A check raised by an enable has the consent of the hand that pressed
/// enable (`AMB-D-351`), which is why that one call may be assembled without a gate to ask; every other
/// road into a plugin comes through [`prepare`], where the gate is the first thing asked. The plugin is
/// handed in already read, since a caller in this position has the manifest in hand and re-reading it off
/// disk would be a second chance for the two to disagree.
///
/// The settings, the read-back path and the payload version are the same as any other run's: what a check
/// receives is what `plugin run` would hand it, because it *is* the plugin's own command face
/// (`AMB-D-353`).
pub(crate) fn assemble(
    store: &Store,
    plugin: &crate::plugin_subscribe::InstalledPlugin,
    args: &[String],
    project: Option<i64>,
    layer: crate::plugin_layer::Layer,
) -> Result<PluginInvocation> {
    let name = plugin.name.as_str();
    let injection = plugin_inject::resolve(store, name, &plugin.manifest.config, layer)?;
    let mut invocation =
        PluginInvocation::new(plugin.program.clone()).stdin_json(command_stdin(injection.text));
    for arg in args {
        invocation = invocation.arg(arg.clone());
    }
    for (key, value) in injection.env {
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
    let invocation = prepare(store, name, args, project)?;
    let log = store.paths.plugin_log_file();
    let outcome = match plugin_command::run(&invocation) {
        Ok(outcome) => outcome,
        Err(e) => {
            plugin_log::record(
                &log,
                &plugin_log::Run {
                    plugin: name.to_string(),
                    event: LOG_EVENT,
                    outcome: plugin_log::Outcome::NotLaunched,
                    code: None,
                    elapsed: std::time::Duration::ZERO,
                    stderr: e.to_string(),
                },
            );
            return Err(e.into());
        }
    };
    plugin_log::record(&log, &run_line(name, &outcome));
    Ok(outcome)
}

/// One finished command run as the execution log takes it. The plugin's stderr is logged whichever way it
/// exited — a successful run's summary is as much of an answer to *what did this do* as a failure's reason,
/// and the log holds only stderr, never the return value (a caller already has that). The duration is not
/// carried: the command face hands back a [`CommandOutcome`], which is the contract's whole reading of a
/// run and holds no clock — how long it took is a hook's timeout material, not a command's.
fn run_line(name: &str, outcome: &CommandOutcome) -> plugin_log::Run {
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
        event: LOG_EVENT,
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
            &CommandOutcome::Failed { code: Some(3), diagnostic: "boom".to_string() },
        );
        assert_eq!(line.outcome, plugin_log::Outcome::Failed);
        assert_eq!(line.code, Some(3));
        assert_eq!(line.stderr, "boom", "the author's stderr is what the log is for");
    }

    #[test]
    fn a_signalled_death_logs_as_failed_with_no_code() {
        let line =
            run_line("worktree", &CommandOutcome::Failed { code: None, diagnostic: String::new() });
        assert_eq!(line.outcome, plugin_log::Outcome::Failed);
        assert_eq!(line.code, None);
    }
}
