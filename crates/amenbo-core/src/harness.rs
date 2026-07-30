//! The AI harnesses that can be wired to run `amenbo agent` when a session starts, and the snippet each
//! one is wired with (`AMB-D-440`). **Read-only, and paste-only**: nothing here writes to a user's
//! provider settings, and every snippet is a whole file's worth of configuration the user pastes for
//! themselves. amenbo asks, detects, and hands over the text — the wiring stays in the user's hands.
//!
//! **What a probe can claim, and what it cannot.** [`probe`] answers whether a folder's settings for a
//! harness *say* to run the launch command when a session starts. Whether the hook then fires, and
//! whether its output reaches the model, is outside amenbo: some providers load project-level settings
//! only under a trust prompt, some do not use a session-start hook's stdout for injection at all, and
//! versions have regressed on both. So the vocabulary here is **wired / unwired** — never "enabled", and
//! never a guarantee.
//!
//! **The catalog is a table, not a code path.** Every harness is one [`Harness`] row in [`HARNESSES`]:
//! where its settings live, how its session-start event is spelled, and the file to paste. Listing one
//! more paste-only provider is one more row — that a new entry costs no new branch is the condition the
//! shape is holding to (`AMB-D-440`), which is also why a provider needing plugin code or an IDE setting
//! does not belong here at all.
//!
//! **What the snippet injects is the launch instruction, not the spec.** Each template carries
//! [`crate::agents::launch_instruction`] — the same one line the managed block holds — and never the
//! output of `agent --json`, which is 40 KB an agent holding the instruction fetches for itself. The
//! block stays where it is: a wired folder is not a reason to strip it, and the hook adds reach over the
//! block, not content.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One AI harness amenbo knows how to be wired into — the catalog row (`AMB-D-440`).
///
/// The fields split in two: [`places`](Harness::places) and [`event`](Harness::event) are what a probe
/// reads, and [`paste_into`](Harness::paste_into) with [`template`](Harness::template) are what a user is
/// handed. Nothing here is a schema — the config shapes have nothing in common (JSON depth, event casing,
/// which key holds the command), which is why the whole of each one is carried as text.
pub struct Harness {
    /// The stable token a face names this harness by (`claude-code`), lowercase and hyphenated. It is a
    /// key, not a rendering: a user reads [`label`](Harness::label).
    pub id: &'static str,
    /// The product's own name for itself, as it appears in its documentation.
    pub label: &'static str,
    /// How this provider spells its session-start event. Matched **case-insensitively**, because that is
    /// the one thing the providers differ on that a probe would otherwise have to know per row
    /// (`SessionStart` here, `sessionStart` there).
    pub event: &'static str,
    /// Where this provider's settings live, relative to the folder. A path that is a **directory** on
    /// disk is read as every `*.json` directly inside it — the shape a provider that takes any filename
    /// under a hooks directory needs, with no second vocabulary for the ones that name a single file.
    pub places: &'static [&'static str],
    /// The file [`snippet`] is written for, so the offer can say where the text goes. One of
    /// [`places`](Harness::places): a snippet pasted where a probe does not look would read as unwired
    /// forever.
    pub paste_into: &'static str,
    /// The configuration to paste, with `{instruction}` standing in for the launch instruction.
    pub template: &'static str,
    /// How many JSON strings the `{instruction}` placeholder sits inside — 1 where the command is
    /// `echo '<instruction>'`, 2 where the command echoes a JSON document that carries it. The
    /// instruction is escaped that many times before it is substituted, so a provider that needs the
    /// text one layer deeper is a number, not a branch.
    pub json_layers: u8,
}

/// Every harness amenbo lists, in the order a face offers them. The five paste-only providers of the
/// first catalog (`AMB-D-440`).
pub static HARNESSES: &[Harness] = &[
    Harness {
        id: "claude-code",
        label: "Claude Code",
        event: "SessionStart",
        // `settings.local.json` is the same folder's settings kept out of the repository, and a user who
        // wired it there has wired it: leaving it out would ask them again forever.
        places: &[".claude/settings.json", ".claude/settings.local.json"],
        paste_into: ".claude/settings.json",
        // Plain stdout is what this one adds to the session, so the command is the instruction itself.
        template: r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "echo '{instruction}'"
          }
        ]
      }
    ]
  }
}"#,
        json_layers: 1,
    },
    Harness {
        id: "github-copilot",
        label: "GitHub Copilot CLI",
        event: "sessionStart",
        places: &[".github/hooks"],
        paste_into: ".github/hooks/amenbo.json",
        template: r#"{
  "version": 1,
  "hooks": {
    "sessionStart": [
      {
        "type": "command",
        "bash": "echo '{\"additionalContext\": \"{instruction}\"}'"
      }
    ]
  }
}"#,
        json_layers: 2,
    },
    Harness {
        id: "cursor",
        label: "Cursor",
        event: "sessionStart",
        places: &[".cursor/hooks.json"],
        paste_into: ".cursor/hooks.json",
        template: r#"{
  "version": 1,
  "hooks": {
    "sessionStart": [
      {
        "command": "echo '{\"additional_context\": \"{instruction}\"}'"
      }
    ]
  }
}"#,
        json_layers: 2,
    },
    Harness {
        id: "codex-cli",
        label: "Codex CLI",
        event: "SessionStart",
        // Two files, one wiring: this provider reads hooks from its own file or from inline tables in the
        // folder's `config.toml`, and either is where a user may have written it.
        places: &[".codex/hooks.json", ".codex/config.toml"],
        paste_into: ".codex/hooks.json",
        template: r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "echo '{\"hookSpecificOutput\":{\"hookEventName\":\"SessionStart\",\"additionalContext\":\"{instruction}\"}}'"
          }
        ]
      }
    ]
  }
}"#,
        json_layers: 2,
    },
    Harness {
        id: "gemini-cli",
        label: "Gemini CLI",
        event: "SessionStart",
        places: &[".gemini/settings.json"],
        paste_into: ".gemini/settings.json",
        template: r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "echo '{\"hookSpecificOutput\":{\"additionalContext\":\"{instruction}\"}}'"
          }
        ]
      }
    ]
  }
}"#,
        json_layers: 2,
    },
];

/// The harness with this [`id`](Harness::id), or `None` when nothing lists it.
pub fn find(id: &str) -> Option<&'static Harness> {
    HARNESSES.iter().find(|harness| harness.id == id)
}

/// The configuration to paste for one harness, with the launch instruction in place. `cmd` is the launch
/// command name ([`crate::config::Paths::command_name`]), so a dev-channel build offers a snippet that
/// calls the binary the user is actually running.
pub fn snippet(harness: &Harness, cmd: &str) -> String {
    let instruction = json_escaped(&crate::agents::launch_instruction(cmd), harness.json_layers);
    harness.template.replace("{instruction}", &instruction)
}

/// `text` as the body of a JSON string, `layers` deep. Nothing in the instruction needs escaping today —
/// which is exactly why this is here: the escape is what keeps that from being a property of the
/// sentence, so rewording it can never quietly emit a snippet that will not parse.
fn json_escaped(text: &str, layers: u8) -> String {
    let mut out = text.to_string();
    for _ in 0..layers {
        // serde_json writes the surrounding quotes; the body is what goes inside the template's own.
        let quoted = serde_json::to_string(&out).unwrap_or_else(|_| format!("\"{out}\""));
        out = quoted[1..quoted.len() - 1].to_string();
    }
    out
}

/// What a folder says about one harness: wired, and where it says so (`AMB-D-440`). `wired_at` is the
/// file the wiring was read from — a user told "unwired" while a file of theirs says otherwise needs to
/// know which file amenbo looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Wiring {
    /// The harness this answers for ([`Harness::id`]).
    pub id: &'static str,
    /// The harness's own name, so a face can render this row without a second lookup.
    pub label: &'static str,
    /// Where the wiring was found, relative to the probed folder — `None` when there is none.
    pub wired_at: Option<PathBuf>,
}

impl Wiring {
    /// Whether this folder is wired for the harness.
    pub fn wired(&self) -> bool {
        self.wired_at.is_some()
    }
}

/// Every harness's answer for `dir`, in [`HARNESSES`] order. `cmd` is the launch command name, so the
/// dev channel is not read as the production binary's wiring and the other way round.
///
/// The judgment is deliberately shallow: a settings file counts as wired when it names both the launch
/// command's `agent` call and the provider's session-start event. Neither on its own is the wiring — a
/// folder can call `agent` from some other hook, and a session-start hook can be doing something else
/// entirely — and parsing five schemas to say more would still not answer the question anybody actually
/// has, which is whether the hook fires. See the module docs.
pub fn probe(dir: &Path, cmd: &str) -> Vec<Wiring> {
    HARNESSES
        .iter()
        .map(|harness| Wiring {
            id: harness.id,
            label: harness.label,
            wired_at: wired_at(dir, harness, cmd),
        })
        .collect()
}

/// The first of a harness's [`places`](Harness::places) whose text carries the wiring, as a path relative
/// to `dir`.
fn wired_at(dir: &Path, harness: &Harness, cmd: &str) -> Option<PathBuf> {
    let call = format!("{cmd} agent");
    harness.places.iter().find_map(|place| {
        settings_files(dir, place).into_iter().find(|relative| {
            std::fs::read_to_string(dir.join(relative))
                .is_ok_and(|text| text.contains(&call) && contains_ignoring_case(&text, harness.event))
        })
    })
}

/// The settings files one `place` stands for, relative to `dir`: the path itself, or — when it is a
/// directory — every `*.json` directly inside it, name-sorted so a probe's answer does not depend on the
/// filesystem's order.
fn settings_files(dir: &Path, place: &str) -> Vec<PathBuf> {
    let path = dir.join(place);
    if !path.is_dir() {
        return vec![PathBuf::from(place)];
    }
    let Ok(entries) = std::fs::read_dir(&path) else { return Vec::new() };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.file_name())
        .filter(|name| Path::new(name).extension().is_some_and(|ext| ext == "json"))
        .map(|name| Path::new(place).join(name))
        .collect();
    found.sort();
    found
}

/// Whether `text` holds `needle`, ignoring case. Only the event token goes through here, and it is ASCII
/// in every provider's vocabulary.
fn contains_ignoring_case(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with one settings file in it.
    fn folder(tag: &str, place: &str, contents: &str) -> PathBuf {
        let dir = amenbo_scratch::scratch(&format!("harness-{tag}"));
        let file = dir.join(place);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, contents).unwrap();
        dir
    }

    fn wiring<'a>(found: &'a [Wiring], id: &str) -> &'a Wiring {
        found.iter().find(|w| w.id == id).expect("every harness answers")
    }

    /// The catalog's own shape: what a new row has to hold for the table to stay a table.
    #[test]
    fn every_row_is_addressable_pasteable_and_probed_where_it_is_pasted() {
        for harness in HARNESSES {
            assert_eq!(find(harness.id).map(|h| h.id), Some(harness.id), "{} is not findable", harness.id);
            assert!(
                harness.template.contains("{instruction}"),
                "{} has nowhere to put the instruction",
                harness.id
            );
            assert!((1..=2).contains(&harness.json_layers), "{} nests the instruction oddly", harness.id);
            // Pasting where the probe does not read would leave the user wired and told otherwise. A
            // directory place answers for the file the snippet names inside it.
            assert!(
                harness.places.iter().any(|place| harness.paste_into.starts_with(place)),
                "{} is pasted somewhere it is not probed",
                harness.id
            );
        }
        let ids: std::collections::BTreeSet<_> = HARNESSES.iter().map(|h| h.id).collect();
        assert_eq!(ids.len(), HARNESSES.len(), "two rows answer to one id");
    }

    /// The first string in `value` that runs a command, whatever key its provider hangs it on.
    fn echoed_command(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(text) => text.starts_with("echo ").then(|| text.clone()),
            serde_json::Value::Array(items) => items.iter().find_map(echoed_command),
            serde_json::Value::Object(fields) => fields.values().find_map(echoed_command),
            _ => None,
        }
    }

    /// The whole paste, all the way down: the file parses, the command it holds runs a single-quoted
    /// `echo`, and what that echo prints is the instruction — as text where the provider takes text, and
    /// as a JSON document carrying it where the provider takes one. This is the one test that would
    /// catch a template escaped one layer too few, which parses as a file and prints nonsense.
    #[test]
    fn every_snippet_is_json_whose_echo_prints_the_instruction() {
        let instruction = crate::agents::launch_instruction("amenbo");
        for harness in HARNESSES {
            let text = snippet(harness, "amenbo");
            assert!(!text.contains("{instruction}"), "{} left the placeholder in", harness.id);
            let parsed: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}\n{text}", harness.id));

            let command = echoed_command(&parsed).unwrap_or_else(|| panic!("{} echoes nothing", harness.id));
            let printed = command
                .strip_prefix("echo '")
                .and_then(|rest| rest.strip_suffix('\''))
                .unwrap_or_else(|| panic!("{}: not a single-quoted echo: {command}", harness.id));
            assert!(!printed.contains('\''), "{}: the echo would end early", harness.id);

            if harness.json_layers == 1 {
                assert_eq!(printed, instruction, "{}", harness.id);
                continue;
            }
            let payload: serde_json::Value = serde_json::from_str(printed)
                .unwrap_or_else(|e| panic!("{} prints no document: {e}\n{printed}", harness.id));
            assert!(
                payload.to_string().contains(&instruction),
                "{} prints a document without the instruction: {payload}",
                harness.id
            );
        }
    }

    /// The snippet calls the binary the user is running, not the product's name.
    #[test]
    fn the_snippet_names_the_running_command() {
        let text = snippet(find("claude-code").unwrap(), "amenbo-dev");
        assert!(text.contains("amenbo-dev agent --json"), "{text}");
    }

    /// Both halves are the wiring: the call, and the event it is wired to.
    #[test]
    fn a_call_and_a_session_start_event_in_one_file_is_the_wiring() {
        let dir = folder(
            "wired",
            ".claude/settings.json",
            &snippet(find("claude-code").unwrap(), "amenbo"),
        );
        let found = probe(&dir, "amenbo");
        assert_eq!(
            wiring(&found, "claude-code").wired_at,
            Some(PathBuf::from(".claude/settings.json"))
        );
        // The other four are not wired by Claude Code's file.
        assert!(found.iter().filter(|w| w.wired()).count() == 1, "{found:?}");
    }

    #[test]
    fn half_the_wiring_is_not_the_wiring() {
        // The event, with something else wired to it.
        let dir = folder("event-only", ".claude/settings.json", r#"{"hooks":{"SessionStart":[]}}"#);
        assert!(!wiring(&probe(&dir, "amenbo"), "claude-code").wired());

        // The call, from a hook that is not session start.
        let dir = folder("call-only", ".claude/settings.json", r#"{"hooks":{"PreToolUse":["amenbo agent"]}}"#);
        assert!(!wiring(&probe(&dir, "amenbo"), "claude-code").wired());
    }

    /// A folder nobody has wired answers for every harness, and answers no.
    #[test]
    fn an_untouched_folder_is_unwired_everywhere() {
        let dir = amenbo_scratch::scratch("harness-bare");
        let found = probe(&dir, "amenbo");
        assert_eq!(found.len(), HARNESSES.len());
        assert!(found.iter().all(|w| !w.wired()), "{found:?}");
    }

    /// A dev-channel wiring is not the production binary's, and the other way round.
    #[test]
    fn the_launch_command_has_to_match() {
        let dir = folder("channel", ".claude/settings.json", &snippet(find("claude-code").unwrap(), "amenbo-dev"));
        assert!(wiring(&probe(&dir, "amenbo-dev"), "claude-code").wired());
        assert!(!wiring(&probe(&dir, "amenbo"), "claude-code").wired());
    }

    /// A hooks directory answers for whatever the user named the file in it.
    #[test]
    fn a_directory_place_reads_every_json_the_user_put_there() {
        let dir = folder(
            "copilot",
            ".github/hooks/whatever-they-called-it.json",
            &snippet(find("github-copilot").unwrap(), "amenbo"),
        );
        assert_eq!(
            wiring(&probe(&dir, "amenbo"), "github-copilot").wired_at,
            Some(PathBuf::from(".github/hooks/whatever-they-called-it.json"))
        );
    }

    /// A provider reading its wiring from either of two files is wired by either.
    #[test]
    fn a_second_place_is_read_when_the_first_holds_nothing() {
        let toml = "[[hooks.SessionStart]]\nmatcher = \"*\"\n\n[[hooks.SessionStart.hooks]]\ntype = \"command\"\ncommand = 'echo amenbo agent'\n";
        let dir = folder("codex", ".codex/config.toml", toml);
        assert_eq!(
            wiring(&probe(&dir, "amenbo"), "codex-cli").wired_at,
            Some(PathBuf::from(".codex/config.toml"))
        );
    }

    /// The event token is matched however the provider spells it.
    #[test]
    fn the_event_is_matched_whatever_its_casing() {
        let dir = folder("casing", ".cursor/hooks.json", r#"{"hooks":{"SESSIONSTART":[{"command":"amenbo agent --json"}]}}"#);
        assert!(wiring(&probe(&dir, "amenbo"), "cursor").wired());
    }
}
