//! **What an enabled plugin says for itself at the AI's entry point** (`AMB-D-437`) — the `plugins` key
//! of `amenbo agent --json`, built from what is installed rather than from anything written here.
//!
//! An AI is pointed at one document to learn how to work in a folder, and a plugin the user installed and
//! enabled is part of the answer that document owes: enabling it was an instruction, and an entry point
//! that does not carry it leaves the AI working without something the user put there on purpose.
//!
//! **amenbo holds no words of its own about a plugin.** What rides here is the author's `agent` block
//! ([`AgentGuide`](crate::plugin_manifest::AgentGuide)), read off the manifest beside the binary at the
//! moment the entry point is asked. No plugin's name appears in amenbo's source (`AMB-D-346`), so a plugin
//! renamed, retired or replaced takes its own sentences with it and leaves no stale line behind. The one
//! thing amenbo does add is the **calling form**: an author writes their own command face, and
//! [`entry`] puts `<amenbo> plugin run <name>` in front of it, so what an AI reads is a line it can type.
//!
//! **Which plugins reach here is the caller's to decide, and it is the callable ones.** This module takes
//! the plugins it is handed and describes them; the gates — installed, enabled in *this* project, and
//! compatible with this build — are asked where the store is (`AMB-D-351`/`AMB-D-359`,
//! [`plugin_invoke::prepare`](crate::plugin_invoke::prepare) is the same set for an actual call). A plugin
//! the entry point named but a call would refuse is worse than one it left out.
//!
//! **A plugin that wrote no `agent` block still appears**, with what its manifest says regardless — its
//! one-line description and what it observes. It said nothing about how to drive it, which is different
//! from it not being here: an AI that can see an enabled plugin firing on `task.done` can reason about
//! what it is watching, and nothing amenbo could invent would be more true than the author's silence.

use serde_json::{json, Map, Value};

use crate::plugin_subscribe::InstalledPlugin;

/// The `plugins` array as the entry point carries it — one [`entry`] per plugin handed in, in the order
/// they arrive (the installed set is name-sorted, so it reads the same as `plugin list`).
///
/// `command_name` is what amenbo is called on this machine ([`Paths::command_name`](crate::config::Paths::command_name)),
/// so a build reached by another name hands out lines that name it too.
pub fn entries(plugins: &[&InstalledPlugin], command_name: &str) -> Value {
    Value::Array(plugins.iter().map(|p| entry(p, command_name)).collect())
}

/// One plugin's entry: who it is, when to reach for it, what to type, and what it watches.
///
/// Only `name` and `desc` are always there — they are the manifest's required fields. `when` and
/// `commands` come from the author's `agent` block and are absent when it is (or when it names no
/// command), and `events` is absent for a plugin that subscribes to nothing. An absent key says the
/// author wrote nothing there, which is the truth; an empty one would spend a reader's attention to say
/// the same.
pub fn entry(plugin: &InstalledPlugin, command_name: &str) -> Value {
    let manifest = &plugin.manifest;
    let mut out = Map::new();
    out.insert("name".into(), json!(plugin.name));
    out.insert("desc".into(), json!(manifest.desc));

    if let Some(agent) = &manifest.agent {
        out.insert("when".into(), json!(agent.when));
        if !agent.commands.is_empty() {
            let commands: Vec<Value> = agent
                .commands
                .iter()
                .map(|c| {
                    json!({
                        // The author wrote the face; the calling form is amenbo's, assembled from the name
                        // it just read off disk (`AMB-D-437`).
                        "cmd": format!("{command_name} plugin run {} {}", plugin.name, c.cmd),
                        "does": c.does,
                    })
                })
                .collect();
            out.insert("commands".into(), Value::Array(commands));
        }
    }

    if !manifest.events.is_empty() {
        // The names alone: which face a hook fires on and whether it replies are the dispatcher's
        // business (`AMB-D-383`), and nothing an AI reading the entry point acts on.
        let events: Vec<Value> = manifest.events.iter().map(|e| json!(e.event)).collect();
        out.insert("events".into(), Value::Array(events));
    }

    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{AgentCommand, AgentGuide, EventSubscription, Manifest, Os};
    use std::path::PathBuf;

    /// An installed plugin carrying `agent` and `events` as the author wrote them.
    fn installed(name: &str, agent: Option<AgentGuide>, events: &[&str]) -> InstalledPlugin {
        InstalledPlugin {
            name: name.to_string(),
            program: PathBuf::from(format!("/plugins/{name}/{name}")),
            manifest: Manifest {
                name: name.to_string(),
                desc: "Isolate each task in its own git worktree".into(),
                author: "amenbo".into(),
                repo: "alice/amenbo-plugin-worktree".into(),
                os: vec![Os::Macos],
                category: "workflow".into(),
                url: "https://example.test/x.tar.gz".into(),
                checksum: format!("sha256:{}", "a".repeat(64)),
                signature: None,
                assets: Default::default(),
                official: true,
                detail_sum: None,
                payload_v: 1,
                min_amenbo: None,
                config: Vec::new(),
                events: events.iter().map(|e| EventSubscription::new(*e)).collect(),
                agent,
            },
            origin: None,
        }
    }

    fn guide() -> AgentGuide {
        AgentGuide {
            when: "Starting work on a task that will produce commits".into(),
            commands: vec![
                AgentCommand {
                    cmd: "start <task-id>".into(),
                    does: "Cuts a worktree outside the repo and returns the cd line to eval".into(),
                },
                AgentCommand { cmd: "finish <task-id>".into(), does: "Tears it down".into() },
            ],
        }
    }

    /// The whole of one entry, and the one thing amenbo contributes to it: the calling form in front of
    /// the author's own face.
    #[test]
    fn a_plugin_that_wrote_a_block_hands_back_a_line_an_ai_can_type() {
        let plugin = installed("worktree", Some(guide()), &["task.status_changed"]);
        assert_eq!(
            entry(&plugin, "amenbo"),
            json!({
                "name": "worktree",
                "desc": "Isolate each task in its own git worktree",
                "when": "Starting work on a task that will produce commits",
                "commands": [
                    {
                        "cmd": "amenbo plugin run worktree start <task-id>",
                        "does": "Cuts a worktree outside the repo and returns the cd line to eval",
                    },
                    { "cmd": "amenbo plugin run worktree finish <task-id>", "does": "Tears it down" },
                ],
                "events": ["task.status_changed"],
            })
        );
    }

    /// The name in the calling form is the one read off disk, and the command word is this build's — a
    /// build reached by another name hands out lines that name it.
    #[test]
    fn the_calling_form_is_built_from_the_name_and_the_command_word() {
        let plugin = installed("renamed-later", Some(guide()), &[]);
        let out = entry(&plugin, "amenbo-dev");
        assert_eq!(out["commands"][0]["cmd"], "amenbo-dev plugin run renamed-later start <task-id>");
    }

    /// An observation-only plugin names its occasion and stops there — no `commands` key invented for it.
    #[test]
    fn a_plugin_with_no_commands_names_its_occasion_alone() {
        let guide = AgentGuide { when: "It only watches".into(), commands: vec![] };
        let out = entry(&installed("watcher", Some(guide), &["task.done"]), "amenbo");
        assert_eq!(out["when"], "It only watches");
        assert!(out.get("commands").is_none(), "no commands written ⇒ no commands key");
        assert_eq!(out["events"], json!(["task.done"]));
    }

    /// A plugin whose author wrote no block is still here — with what the manifest says regardless.
    #[test]
    fn a_plugin_that_wrote_nothing_is_still_named() {
        let out = entry(&installed("quiet", None, &["comment.added"]), "amenbo");
        assert_eq!(out["name"], "quiet");
        assert_eq!(out["desc"], "Isolate each task in its own git worktree");
        assert!(out.get("when").is_none(), "silence is not a sentence amenbo writes for them");
        assert!(out.get("commands").is_none());
        assert_eq!(out["events"], json!(["comment.added"]), "what it watches is still readable");
    }

    /// Subscribing to nothing leaves no key: a command-only plugin does not carry an empty list.
    #[test]
    fn a_plugin_that_watches_nothing_carries_no_events_key() {
        let out = entry(&installed("caller", Some(guide()), &[]), "amenbo");
        assert!(out.get("events").is_none());
    }

    /// The array is what the caller handed over, in that order — the gates are asked before this.
    #[test]
    fn the_key_is_the_plugins_it_was_given_in_order() {
        let a = installed("alpha", Some(guide()), &[]);
        let b = installed("beta", None, &[]);
        let out = entries(&[&a, &b], "amenbo");
        assert_eq!(out.as_array().map(Vec::len), Some(2));
        assert_eq!(out[0]["name"], "alpha");
        assert_eq!(out[1]["name"], "beta");
        assert_eq!(entries(&[], "amenbo"), json!([]), "nothing callable here is an empty list");
    }
}
