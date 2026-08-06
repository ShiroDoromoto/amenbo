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
//! **A plugin that wrote no `agent` block still appears**, with what its manifest says regardless — what
//! it observes, and for an official one its description besides. It said nothing about how to drive it,
//! which is different from it not being here: an AI that can see an enabled plugin firing on `task.done`
//! can reason about what it is watching, and nothing amenbo could invent would be more true than the
//! author's silence.
//!
//! **The author's prose rides here only for an official plugin** (`AMB-D-575`). Whether a sentence is an
//! instruction to the reading AI is not something a machine can rule on (`AMB-D-572`), and the review that
//! would catch it does not exist, so the split is drawn where a machine cannot be wrong: `official` is the
//! catalog's badge and no third party can self-grant it (`AMB-D-347`). A third party's entry carries `cmd`
//! — held to a grammar, so no prose fits in it — and nothing else its author wrote anywhere in the
//! manifest: no `when`, no `does` beside the line, and no `desc` (`AMB-D-576`), which is a required field
//! and so would otherwise be the one sentence every third party still gets to place here. It is not
//! filtered out; there is no field for it to land in.
//!
//! **`desc` is not gone, it is elsewhere**: `plugin list` is the face a person reads, and every plugin's
//! description is written there whoever wrote it (`AMB-D-576`). What the split decides is which reader a
//! sentence reaches, not whether it is kept.

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
/// Only `name` is always there. `desc` is the manifest's other required field, but it is the author's
/// sentence, so it rides for an official plugin alone (`AMB-D-576`); `when` and `commands` come from the
/// author's `agent` block and are absent when it is (or when it names no command); and `events` is absent
/// for a plugin that subscribes to nothing. An absent key says nothing was written there, or that what
/// was is not this reader's — an empty one would spend their attention to say the same.
///
/// **`desc`, `when` and `does` are an official plugin's alone** (`AMB-D-575`/`AMB-D-576`): for a third
/// party, the block still yields its commands, but each one is the `cmd` line by itself. A third party
/// that named no command is down to its name and what it watches.
pub fn entry(plugin: &InstalledPlugin, command_name: &str) -> Value {
    let manifest = &plugin.manifest;
    // The badge is the catalog's, never self-declared (`AMB-D-347`), which is what makes this the one
    // split a machine can draw without ever being wrong about it.
    let official = manifest.official;
    let mut out = Map::new();
    out.insert("name".into(), json!(plugin.name));
    if official {
        // Required of every manifest, and still the author's own line — so it goes to `plugin list`,
        // where a person reads it, and not here (`AMB-D-576`).
        out.insert("desc".into(), json!(manifest.desc));
    }

    if let Some(agent) = &manifest.agent {
        if official {
            out.insert("when".into(), json!(agent.when));
        }
        if !agent.commands.is_empty() {
            let commands: Vec<Value> = agent
                .commands
                .iter()
                .map(|c| {
                    // The author wrote the face; the calling form is amenbo's, assembled from the name
                    // it just read off disk (`AMB-D-437`). `cmd` is held to a grammar (`AMB-D-572`), so
                    // it is the one thing a third party's sentence cannot ride in on.
                    let mut entry = Map::new();
                    entry.insert(
                        "cmd".into(),
                        json!(format!("{command_name} plugin run {} {}", plugin.name, c.cmd)),
                    );
                    if official {
                        entry.insert("does".into(), json!(c.does));
                    }
                    Value::Object(entry)
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

    /// An installed **official** plugin carrying `agent` and `events` as the author wrote them.
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

    /// The same plugin without the catalog's badge — what anyone outside the amenbo team installs as.
    fn third_party(name: &str, agent: Option<AgentGuide>, events: &[&str]) -> InstalledPlugin {
        let mut p = installed(name, agent, events);
        p.manifest.official = false;
        p
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

    /// The whole of one official entry, and the one thing amenbo contributes to it: the calling form in
    /// front of the author's own face.
    #[test]
    fn an_official_plugin_that_wrote_a_block_hands_back_a_line_an_ai_can_type() {
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

    /// A third party gets the line and nothing it wrote around it (`AMB-D-575`/`AMB-D-576`) — the calling
    /// form is still assembled, so the plugin stays callable, but the prose has nowhere to land. `desc` is
    /// required of every manifest and still absent here: required is not the same as addressed to this
    /// reader.
    #[test]
    fn a_third_party_carries_the_line_alone() {
        let plugin = third_party("worktree", Some(guide()), &["task.status_changed"]);
        assert_eq!(
            entry(&plugin, "amenbo"),
            json!({
                "name": "worktree",
                "commands": [
                    { "cmd": "amenbo plugin run worktree start <task-id>" },
                    { "cmd": "amenbo plugin run worktree finish <task-id>" },
                ],
                "events": ["task.status_changed"],
            })
        );
    }

    /// A third party that only watches wrote nothing but prose, so it is down to the two things it did
    /// not write: the name the catalog knows it by, and the events amenbo named.
    #[test]
    fn a_third_party_that_named_no_command_is_down_to_its_name_and_its_events() {
        let guide = AgentGuide { when: "It only watches".into(), commands: vec![] };
        let out = entry(&third_party("watcher", Some(guide), &["task.done"]), "amenbo");
        assert_eq!(
            out,
            json!({ "name": "watcher", "events": ["task.done"] }),
            "the vocabulary is amenbo's, and that is the whole of what is left"
        );
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
