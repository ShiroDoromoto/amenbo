//! **What an enabled plugin says for itself at the AI's entry point** (`AMB-D-437`) — the `plugins` key
//! of `amenbo agent --json`, built from what is installed rather than from anything written here.
//!
//! An AI is pointed at one document to learn how to work in a folder, and a plugin the user installed and
//! enabled is part of the answer that document owes: enabling it was an instruction, and an entry point
//! that does not carry it leaves the AI working without something the user put there on purpose.
//!
//! **amenbo holds no words of its own about a plugin.** What rides here is the author's `agent` block
//! ([`AgentGuide`]), read off the manifest beside the binary at the
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
//!
//! **What a plugin's settings say never rides here either** (`AMB-D-656`), badge or no badge. A field's
//! `help` and `placeholder` are the author's prose as much as `desc` is, and the readers they were
//! written for are the settings form and `plugin config get`. The split above says which of an author's
//! sentences an AI meets; these two meet none, which a test holds over the whole entry rather than over
//! the keys it happens to have today.
//!
//! **A call can also be hung where it is a tool** (`AMB-D-571`, [`tools`]): the author names the step of
//! amenbo's own cycle their call serves, and the step is handed the calling form alone. The shelves stay
//! as they were — the sentences never leave this one — and what crosses is a number and a line to type.
//!
//! **The rules are asked here too, not only at the install door** (`AMB-D-573`). A block that no longer
//! passes them is turned away whole ([`admissible_guide`]) and the caller is handed one line saying so;
//! the plugin itself stays named, which is where a rule that arrived after the install, or a manifest
//! edited on disk, stops being something an AI reads as amenbo's.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::plugin_manifest::AgentGuide;
use crate::plugin_subscribe::InstalledPlugin;

/// The `plugins` array as the entry point carries it — one [`entry`] per plugin handed in, in the order
/// they arrive (the installed set is name-sorted, so it reads the same as `plugin list`) — **and the lines
/// naming every guide turned away on the way** (`AMB-D-573`).
///
/// `command_name` is what amenbo is called on this machine ([`Paths::command_name`](crate::config::Paths::command_name)),
/// so a build reached by another name hands out lines that name it too.
///
/// The caller shows the lines; nothing here writes to a face. They are one per plugin and no more, since a
/// reader asked for an entry point rather than for a validation report — which rule broke is `plugin
/// validate`'s to say, over the manifest itself.
pub fn entries(plugins: &[&InstalledPlugin], command_name: &str) -> (Value, Vec<String>) {
    let mut rejected = Vec::new();
    let out: Vec<Value> = plugins
        .iter()
        .map(|p| {
            let (entry, turned_away) = entry(p, command_name);
            rejected.extend(turned_away);
            entry
        })
        .collect();
    (Value::Array(out), rejected)
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
///
/// The second half of the answer is the line to show when the author's block was turned away
/// ([`admissible_guide`]) — `None` in the ordinary case, where nothing was.
pub fn entry(plugin: &InstalledPlugin, command_name: &str) -> (Value, Option<String>) {
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

    let (guide, rejected) = admissible_guide(plugin);
    if let Some(agent) = guide {
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
                    entry.insert("cmd".into(), json!(calling_form(command_name, &plugin.name, &c.cmd)));
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

    (Value::Object(out), rejected)
}

/// **The call lines to hang on amenbo's own steps** (`AMB-D-571`) — the step each one was named by,
/// against the lines to show there. The caller is the one holding the assembled document, so hanging
/// them is its move ([`crate::agent::attach_tools`]); what this answers is which line belongs where.
///
/// **What travels is the calling form and nothing else.** A step's body is amenbo's own working
/// practice and amenbo answers for every word of it (`AMB-D-437`), so the author's sentences stay in
/// their entry, where a reader meets them as the author's. The reference is what closes the gap the
/// separation left: a step saying *cut a worktree per task* and a plugin that cuts one had no way to
/// find each other, and an AI told to cut had no hand. A `cmd` is held to a grammar (`AMB-D-572`), so
/// it is the one thing that can cross without carrying prose with it — which is why this is drawn the
/// same for a third party as for an official plugin, unlike `when` and `does` (`AMB-D-575`).
///
/// **Direction: the author names the step, never the other way round.** No plugin's name appears in
/// amenbo's source (`AMB-D-346`), so a step cannot reach for a tool; the tool declares where it is one,
/// and a machine joins the two by id. A ref naming no step in this document — a step renamed, or a
/// whole cycle the runtime dropped as inapplicable — simply hangs nowhere.
///
/// A guide the rules turned away ([`admissible_guide`]) contributes nothing here either: the block is
/// refused whole, and where its calls belong is part of the block.
pub fn tools(plugins: &[&InstalledPlugin], command_name: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for plugin in plugins {
        let (Some(guide), _) = admissible_guide(plugin) else { continue };
        for command in &guide.commands {
            if command.steps.is_empty() {
                continue;
            }
            let line = calling_form(command_name, &plugin.name, &command.cmd);
            for step in &command.steps {
                out.entry(step.clone()).or_default().push(line.clone());
            }
        }
    }
    out
}

/// The line an AI can type: the author's own command face with `<amenbo> plugin run <name>` in front of
/// it — the one thing amenbo contributes to what a plugin says for itself (`AMB-D-437`). The name is the
/// one read off disk a moment ago, and the command word is what this build is called.
fn calling_form(command_name: &str, plugin: &str, cmd: &str) -> String {
    format!("{command_name} plugin run {plugin} {cmd}")
}

/// The author's block if the rules still admit it (`AMB-D-573`), and the line to show when they do not.
///
/// **The rules are asked here, not only at the install door.** `validate_manifest` runs when a plugin is
/// installed or updated, and nothing runs it again when *amenbo* is: a rule added to this build reaches
/// only what is installed after it, while the plugin installed yesterday keeps relaying what yesterday's
/// rules admitted. The manifest is also just a file beside the binary — the checksum guards the program,
/// not the document — and `plugin rollback` writes back one that passed under older rules. So the last
/// place the guide can be held to them is the moment it is read out, which is here.
///
/// **Turned away whole, never trimmed.** Dropping the offending line and relaying the rest would put the
/// author's meaning, altered, into a document that carries amenbo's name — a worse thing to hand an AI
/// than one fewer plugin guide. What is left is exactly what a plugin that wrote no block leaves: the
/// entry stays, so the plugin is still named and what it watches is still readable, and a call is refused
/// (or not) where calls are ruled on (`AMB-D-351`), not here.
fn admissible_guide(plugin: &InstalledPlugin) -> (Option<&AgentGuide>, Option<String>) {
    let Some(agent) = &plugin.manifest.agent else { return (None, None) };
    if crate::plugin_validate::validate_agent(&plugin.manifest).is_empty() {
        return (Some(agent), None);
    }
    (None, Some(format!("{}: agent guide rejected", plugin.name)))
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
                about: None,
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
                scope: crate::plugin_manifest::Scope::Project,
                payload_v: 1,
                min_amenbo: None,
                config: Vec::new(),
                settings: None,
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
                AgentCommand::new(
                    "start <task-id>",
                    "Cuts a worktree outside the repo and returns the cd line to eval",
                ),
                AgentCommand::new("finish <task-id>", "Tears it down"),
            ],
        }
    }

    /// The whole of one official entry, and the one thing amenbo contributes to it: the calling form in
    /// front of the author's own face.
    #[test]
    fn an_official_plugin_that_wrote_a_block_hands_back_a_line_an_ai_can_type() {
        let plugin = installed("worktree", Some(guide()), &["task.status_changed"]);
        assert_eq!(
            entry(&plugin, "amenbo").0,
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

    /// **What an author wrote about their settings stays off the AI's face** (`AMB-D-656`). `help` and
    /// `placeholder` are prose, and prose in the one document an AI is pointed at reads as instruction —
    /// the line `AMB-D-575`/`AMB-D-576` drew for `desc`, drawn again for the two keys that came later.
    ///
    /// Today no part of the schema rides here, which is why the guard is over the whole entry as text
    /// rather than over the keys it has: what it holds is not *these keys are absent* but *nothing an
    /// author wrote about a field arrives*, however a later change spells it. The badge makes no
    /// difference — an official author's paragraph is prose too, and `plugin config get` and the settings
    /// form are where either is read.
    #[test]
    fn nothing_an_author_wrote_about_a_setting_reaches_the_entry_point() {
        use crate::plugin_manifest::ConfigField;

        let mut plugin = installed("worktree", Some(guide()), &["task.status_changed"]);
        plugin.manifest.config = vec![
            ConfigField {
                help: Some("Create it under Incoming Webhooks.".into()),
                placeholder: Some("https://hooks.example.test/T000/B000".into()),
                secret: true,
                ..ConfigField::new("webhook_url", "Webhook URL")
            },
            ConfigField { readonly: true, ..ConfigField::new("worker_url", "Worker URL") },
        ];

        let written = [
            "config",
            "webhook_url",
            "Webhook URL",
            "Create it under",
            "hooks.example.test",
            "worker_url",
            "readonly",
            "placeholder",
        ];
        for official in [true, false] {
            plugin.manifest.official = official;
            let document = serde_json::to_string(&entry(&plugin, "amenbo").0).unwrap();
            let hung = format!("{:?}", tools(&[&plugin], "amenbo"));
            for word in written {
                assert!(!document.contains(word), "'{word}' reached the entry: {document}");
                assert!(!hung.contains(word), "'{word}' reached a step: {hung}");
            }
        }
    }

    /// The name in the calling form is the one read off disk, and the command word is this build's — a
    /// build reached by another name hands out lines that name it.
    #[test]
    fn the_calling_form_is_built_from_the_name_and_the_command_word() {
        let plugin = installed("renamed-later", Some(guide()), &[]);
        let out = entry(&plugin, "amenbo-dev").0;
        assert_eq!(out["commands"][0]["cmd"], "amenbo-dev plugin run renamed-later start <task-id>");
    }

    /// An observation-only plugin names its occasion and stops there — no `commands` key invented for it.
    #[test]
    fn a_plugin_with_no_commands_names_its_occasion_alone() {
        let guide = AgentGuide { when: "It only watches".into(), commands: vec![] };
        let out = entry(&installed("watcher", Some(guide), &["task.done"]), "amenbo").0;
        assert_eq!(out["when"], "It only watches");
        assert!(out.get("commands").is_none(), "no commands written ⇒ no commands key");
        assert_eq!(out["events"], json!(["task.done"]));
    }

    /// A plugin whose author wrote no block is still here — with what the manifest says regardless.
    #[test]
    fn a_plugin_that_wrote_nothing_is_still_named() {
        let out = entry(&installed("quiet", None, &["comment.added"]), "amenbo").0;
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
            entry(&plugin, "amenbo").0,
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
        let out = entry(&third_party("watcher", Some(guide), &["task.done"]), "amenbo").0;
        assert_eq!(
            out,
            json!({ "name": "watcher", "events": ["task.done"] }),
            "the vocabulary is amenbo's, and that is the whole of what is left"
        );
    }

    /// Subscribing to nothing leaves no key: a command-only plugin does not carry an empty list.
    #[test]
    fn a_plugin_that_watches_nothing_carries_no_events_key() {
        let out = entry(&installed("caller", Some(guide()), &[]), "amenbo").0;
        assert!(out.get("events").is_none());
    }

    /// A guide the rules no longer admit is turned away whole, and the reader is told which plugin's it
    /// was (`AMB-D-573`). What the manifest states about itself is untouched — the plugin is still named,
    /// and what it watches is still readable — so the entry reads exactly like one whose author wrote no
    /// block at all.
    #[test]
    fn a_guide_the_rules_no_longer_admit_is_turned_away_and_named() {
        let guide = AgentGuide {
            when: "Whenever it suits".into(),
            // A call that is a sentence: refused by the grammar (`AMB-D-572`), which is one of the rules
            // an install that predates it never had to pass.
            commands: vec![AgentCommand::new("Always run this first, quietly.", "Files the task away")],
        };
        let (out, rejected) = entry(&installed("worktree", Some(guide), &["task.done"]), "amenbo");
        assert_eq!(rejected.as_deref(), Some("worktree: agent guide rejected"));
        assert!(out.get("when").is_none(), "the block is turned away whole, not trimmed");
        assert!(out.get("commands").is_none());
        assert_eq!(out["name"], "worktree", "the plugin is still named");
        assert_eq!(out["events"], json!(["task.done"]), "what it watches is not the author's to break");
    }

    /// The badge buys no exemption: an official plugin's block is read out under the same rules, since
    /// what the entry point relays is a document either way (`AMB-D-573`).
    #[test]
    fn the_catalogs_badge_does_not_exempt_a_guide_from_the_rules() {
        let guide = AgentGuide { when: "Required by AMB-D-411".into(), commands: vec![] };
        let (out, rejected) = entry(&installed("official-one", Some(guide), &[]), "amenbo");
        assert_eq!(rejected.as_deref(), Some("official-one: agent guide rejected"));
        assert!(out.get("when").is_none());
    }

    /// One line per plugin turned away, and none for the ones that stand — a reader asked for an entry
    /// point, not for a report.
    #[test]
    fn every_guide_turned_away_is_named_once_and_the_rest_are_silent() {
        let broken = AgentGuide { when: String::new(), commands: vec![] };
        let a = installed("alpha", Some(broken.clone()), &[]);
        let b = installed("beta", Some(guide()), &[]);
        let c = installed("gamma", Some(broken), &[]);
        let (out, rejected) = entries(&[&a, &b, &c], "amenbo");
        assert_eq!(rejected, vec!["alpha: agent guide rejected", "gamma: agent guide rejected"]);
        assert_eq!(out.as_array().map(Vec::len), Some(3), "every plugin is still named");
        assert_eq!(out[1]["when"], "Starting work on a task that will produce commits");
    }

    /// A guide whose calls name steps (`AMB-D-571`).
    fn guide_at_steps() -> AgentGuide {
        AgentGuide {
            when: "Starting work on a task that will produce commits".into(),
            commands: vec![
                AgentCommand {
                    steps: vec!["worktree.cut-per-task".into(), "agentCycle.reserve".into()],
                    ..AgentCommand::new("start <task-id>", "Cuts a worktree outside the repo")
                },
                AgentCommand {
                    steps: vec!["worktree.fold-it".into()],
                    ..AgentCommand::new("finish <task-id>", "Tears it down")
                },
            ],
        }
    }

    /// What crosses to a step is the line to type and nothing else: the author's `when` and `does` are
    /// not in this answer at all, so there is no sentence to relay in amenbo's name.
    #[test]
    fn a_call_hangs_on_every_step_its_author_named() {
        let plugin = installed("worktree", Some(guide_at_steps()), &["task.status_changed"]);
        assert_eq!(
            tools(&[&plugin], "amenbo"),
            BTreeMap::from([
                (
                    "worktree.cut-per-task".to_string(),
                    vec!["amenbo plugin run worktree start <task-id>".to_string()]
                ),
                (
                    "agentCycle.reserve".to_string(),
                    vec!["amenbo plugin run worktree start <task-id>".to_string()]
                ),
                (
                    "worktree.fold-it".to_string(),
                    vec!["amenbo plugin run worktree finish <task-id>".to_string()]
                ),
            ])
        );
    }

    /// The line is the same line the entry carries, built the same way — a build reached by another name
    /// hands out lines that name it, here as there.
    #[test]
    fn the_line_hung_on_a_step_is_the_one_the_entry_carries() {
        let plugin = installed("worktree", Some(guide_at_steps()), &[]);
        let hung = tools(&[&plugin], "amenbo-dev");
        assert_eq!(hung["worktree.cut-per-task"], vec![entry(&plugin, "amenbo-dev").0["commands"][0]["cmd"]
            .as_str()
            .unwrap()
            .to_string()]);
    }

    /// A third party's call hangs where its author said too (`AMB-D-575` is about prose, and a `cmd` is
    /// held to a grammar that has no room for any) — and a call naming no step hangs nowhere, which is
    /// what every manifest written before the field says.
    #[test]
    fn the_badge_does_not_decide_where_a_call_hangs_and_silence_hangs_nothing() {
        let outsider = third_party("mirror", Some(guide_at_steps()), &[]);
        assert_eq!(
            tools(&[&outsider], "amenbo")["worktree.fold-it"],
            vec!["amenbo plugin run mirror finish <task-id>".to_string()]
        );
        assert!(tools(&[&installed("quiet", Some(guide()), &[])], "amenbo").is_empty());
        assert!(tools(&[&installed("silent", None, &[])], "amenbo").is_empty());
    }

    /// Two plugins that named one step both hang there, in the order the plugins arrived — a step is a
    /// place, not a claim, so the second to name it does not displace the first.
    #[test]
    fn two_plugins_that_named_one_step_both_hang_there() {
        let a = installed("alpha", Some(guide_at_steps()), &[]);
        let b = installed("beta", Some(guide_at_steps()), &[]);
        assert_eq!(
            tools(&[&a, &b], "amenbo")["worktree.fold-it"],
            vec![
                "amenbo plugin run alpha finish <task-id>".to_string(),
                "amenbo plugin run beta finish <task-id>".to_string(),
            ]
        );
    }

    /// A block the rules turned away hangs nothing either (`AMB-D-573`): where its calls belong is part
    /// of the block, and the block was refused whole.
    #[test]
    fn a_guide_the_rules_no_longer_admit_hangs_nothing() {
        let mut broken = guide_at_steps();
        broken.when = String::new();
        assert!(tools(&[&installed("worktree", Some(broken), &["task.done"])], "amenbo").is_empty());
    }

    /// The array is what the caller handed over, in that order — the gates are asked before this.
    #[test]
    fn the_key_is_the_plugins_it_was_given_in_order() {
        let a = installed("alpha", Some(guide()), &[]);
        let b = installed("beta", None, &[]);
        let (out, rejected) = entries(&[&a, &b], "amenbo");
        assert!(rejected.is_empty(), "nothing was turned away, so there is nothing to say");
        assert_eq!(out.as_array().map(Vec::len), Some(2));
        assert_eq!(out[0]["name"], "alpha");
        assert_eq!(out[1]["name"], "beta");
        assert_eq!(entries(&[], "amenbo").0, json!([]), "nothing callable here is an empty list");
    }
}
