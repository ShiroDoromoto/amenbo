//! **Which models an agent can be started on** — asked of the agent's own command, never held here
//! (`AMB-D-865`).
//!
//! Amenbo has no table of model names and will not grow one. A table goes stale on its own: the one
//! written in `AMB-T-3814` was wrong in three places within a day, and the six providers themselves
//! keep none — every one of them fetches its list from its vendor's server (`AMB-T-4579`). Amenbo
//! cannot go to those servers without holding somebody's key, and the CLI already holds one, so the
//! question goes to the CLI and the key never moves.
//!
//! **What a row here says is how to ask, not what the answer is.** [`Ask`] is the arguments the
//! provider's own command takes and the shape its answer comes back in — a column on
//! [`crate::harness::Launch`], the same way the prompt flag is. The six shapes have nothing in
//! common ([`Reading`]), which is a fact about the providers rather than a design: one prints JSON,
//! one prints bare lines, one prints `id - label`, one writes its aliases into a sentence of English
//! help, one has to be spoken to over ACP, and one cannot be asked at all.
//!
//! | id | asked | read back |
//! |---|---|---|
//! | `codex-cli` | `codex debug models` | JSON, `slug` straight onto `-m`; `visibility: hide` rows dropped |
//! | `opencode` | `opencode models` | one `provider/model` per line |
//! | `cursor` | `cursor-agent --list-models` | `id - label` per line; **exit 1 when not signed in** |
//! | `gemini-cli` | `gemini --acp`, then ACP `session/new` | `models.availableModels` |
//! | `claude-code` | `claude --help` | the aliases named in the `--model` paragraph |
//! | `github-copilot` | — | nothing to ask: it has no list door (`AMB-T-4581`) |
//!
//! **The id and the name are carried apart** ([`Model`]). For four of the five they are the same
//! word, and for `gemini-cli` they are not: what its ACP answer calls
//! `gemini-3.1-pro-preview-customtools` is the model its own TUI draws as `gemini-3.1-pro-preview`,
//! so a face showing the id would be showing a spelling the person has never seen, and a launch
//! passing the name would be passing one the CLI does not know.
//!
//! **Nothing here runs anything.** Starting a provider means the reader's own login shell, with the
//! reader's own `PATH` (`app/src-tauri/src/launch.rs`) — the same reason [`crate::wake`] asks its
//! caller what is installed rather than looking itself. So this module says what to run and reads
//! what came back, and the running is the caller's (`app/src-tauri/src/agent_models.rs`).
//!
//! **Every failure is an empty list.** Not signed in, no such command, a version that has dropped
//! the flag, an answer in a shape this does not know — all of them come back as "no models to
//! offer", because the only thing a face does with this is draw a row of choices, and there is
//! always the one road that needs no list at all: start the agent and use its own picker.

use std::path::Path;

use serde::Serialize;

/// One model a provider can be started on.
///
/// **Two spellings, because the providers do not agree that there is one.** [`id`](Model::id) is
/// what goes behind the model flag ([`crate::harness::Launch`]), and [`label`](Model::label) is what
/// a person is shown. Keeping the second is also what lets a face draw a model that was chosen once
/// and remembered when the CLI cannot be reached to ask again (`AMB-T-4588`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Model {
    /// The spelling the provider takes — handed to its model flag exactly as it came back.
    pub id: String,
    /// The provider's own name for it, for a person to read. The same text as
    /// [`id`](Model::id) where the provider draws no distinction.
    pub label: String,
}

/// How one provider is asked what it can be started on — a column on [`crate::harness::Launch`]
/// (`AMB-D-865`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ask {
    /// The arguments the provider's own command ([`crate::harness::Launch::command`]) is asked with.
    pub args: &'static [&'static str],
    /// The shape the answer comes back in.
    pub reading: Reading,
}

/// The shape one provider's answer comes back in.
///
/// A variant per provider, because the answers really are that different — see the module docs. What
/// is **not** here is a provider with no way of being asked: that is `models: None` on the launch
/// row, so "there is nothing to run" never has to be spelled as a run that returns nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// A JSON document with a `models` array of `{ slug, display_name, visibility }` — Codex CLI's.
    /// A row marked `visibility: "hide"` is one the provider does not put in front of people
    /// (`gpt-reserve`, `codex-auto-review`), so it is dropped here too.
    CodexCatalog,
    /// One `provider/model` per line and nothing else on it — OpenCode's. The whole line is the
    /// spelling `-m` takes, qualifier and all.
    QualifiedLines,
    /// `id - label` per line, under a heading and above a tip — Cursor's. A row can carry its
    /// current-and-default state in the label, which is state rather than name and is left off.
    IdThenLabel,
    /// The aliases written into the `--model` paragraph of a provider's own `--help` — Claude Code's,
    /// which has no list door at all (`AMB-T-4581`).
    ///
    /// **The most brittle of the five, knowingly.** It reads a sentence of English written for a
    /// person, so a reworded help is an empty list rather than a wrong one — which is why the
    /// paragraph is cut at the words that introduce the full-name example, and why only the words in
    /// quotes are taken.
    HelpAliases,
    /// The `models.availableModels` of an ACP `session/new` answer — Gemini CLI's, which has no list
    /// door either and no help text naming one.
    ///
    /// **This is the one that has to be spoken to** ([`said_to`]): the provider is started as an ACP
    /// agent, told hello, asked for a session, and the session it hands back carries the list. It
    /// does not end on its own afterwards ([`answered`]), because an agent that has been given a
    /// session is waiting to be worked with.
    AcpSession,
}

/// The JSON-RPC id the ACP handshake's hello is sent under.
const ACP_HELLO: u32 = 1;

/// The JSON-RPC id the ACP session request is sent under.
const ACP_SESSION: u32 = 2;

/// The ACP revision this speaks. One number, and the only one Gemini CLI has answered on.
const ACP_VERSION: u32 = 1;

/// What has to be written to the provider's stdin before it will answer, one message per line.
///
/// Empty for every provider that answers a command line, which is five of the six: they print and
/// exit, and there is nothing to say to them. The ACP one is the exception — an agent is a thing you
/// talk to, so the question is two messages, hello and then a session, and `cwd` is the folder the
/// session is opened in (an ACP agent requires an absolute one, and never reads it here).
pub fn said_to(ask: &Ask, cwd: &Path) -> Vec<String> {
    match ask.reading {
        Reading::AcpSession => vec![
            rpc(
                ACP_HELLO,
                "initialize",
                serde_json::json!({
                    "protocolVersion": ACP_VERSION,
                    // Nothing of the reader's is offered: this session is opened to be asked one
                    // question and killed, and an agent that could read and write files through it
                    // would be one Amenbo had handed a folder to on nobody's say-so.
                    "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false } },
                }),
            ),
            rpc(
                ACP_SESSION,
                "session/new",
                serde_json::json!({ "cwd": cwd, "mcpServers": [] }),
            ),
        ],
        _ => Vec::new(),
    }
}

/// One JSON-RPC request, on one line.
fn rpc(id: u32, method: &str, params: serde_json::Value) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

/// Whether what the provider has printed so far already holds the answer — the signal to stop
/// reading and kill it.
///
/// **Only the ACP one needs this.** The other five print their list and exit, so their answer ends
/// where their output does; an ACP agent handed a session stays up waiting to be worked with, and a
/// caller reading to the end of its output would read until the deadline every time.
pub fn answered(ask: &Ask, printed: &str) -> bool {
    matches!(ask.reading, Reading::AcpSession) && !read(ask, printed).is_empty()
}

/// The models in what the provider printed — **never an error**, see the module docs.
///
/// An answer this cannot make sense of is an empty list, and so is an empty answer: what a caller
/// does with either is the same thing, and there is no repair a person could make from being told
/// which of the two it was.
pub fn read(ask: &Ask, printed: &str) -> Vec<Model> {
    match ask.reading {
        Reading::CodexCatalog => codex_catalog(printed),
        Reading::QualifiedLines => qualified_lines(printed),
        Reading::IdThenLabel => id_then_label(printed),
        Reading::HelpAliases => help_aliases(printed),
        Reading::AcpSession => acp_session(printed),
    }
}

/// Codex CLI's `debug models`: the `models` array, minus the rows it marks as hidden.
///
/// The document is a large one — every row carries the whole of its own system prompt — so only the
/// three fields that answer the question are named, and everything else is passed over unread.
fn codex_catalog(printed: &str) -> Vec<Model> {
    #[derive(serde::Deserialize)]
    struct Catalog {
        models: Vec<Row>,
    }
    #[derive(serde::Deserialize)]
    struct Row {
        slug: String,
        display_name: Option<String>,
        visibility: Option<String>,
    }

    let Ok(catalog) = serde_json::from_str::<Catalog>(printed) else {
        return Vec::new();
    };
    catalog
        .models
        .into_iter()
        .filter(|row| row.visibility.as_deref() != Some("hide"))
        .map(|row| Model {
            label: row.display_name.clone().unwrap_or_else(|| row.slug.clone()),
            id: row.slug,
        })
        .collect()
}

/// OpenCode's `models`: the lines that are one qualified name and nothing else.
///
/// The qualifier is part of the spelling `-m` takes, so a line without a `/` in it is not one of
/// these — which is what keeps a greeting or a warning that reached stdout off the list.
fn qualified_lines(printed: &str) -> Vec<Model> {
    printed
        .lines()
        .map(str::trim)
        .filter(|line| line.contains('/') && !line.contains(char::is_whitespace))
        .map(|line| Model { id: line.to_string(), label: line.to_string() })
        .collect()
}

/// Cursor's `--list-models`: the `id - label` rows, past the heading and short of the tip.
///
/// Neither of those two is filtered out by name — the id has to be one word, which the heading's is
/// not, and the tip's separator is inside a sentence rather than between two halves of a line.
fn id_then_label(printed: &str) -> Vec<Model> {
    printed
        .lines()
        .filter_map(|line| line.trim().split_once(" - "))
        .filter(|(id, _)| !id.is_empty() && !id.contains(char::is_whitespace))
        .map(|(id, label)| Model { id: id.to_string(), label: named(label).to_string() })
        .collect()
}

/// A Cursor label with the state it may be carrying taken off it: `Auto (current, default)` is the
/// model called `Auto`, and which one is current is a fact about the reader's settings rather than
/// part of its name.
fn named(label: &str) -> &str {
    let Some((name, state)) = label.rsplit_once(" (") else {
        return label.trim();
    };
    let Some(state) = state.strip_suffix(')') else {
        return label.trim();
    };
    if state.split(',').map(str::trim).all(|word| matches!(word, "current" | "default")) {
        name.trim()
    } else {
        label.trim()
    }
}

/// The word a provider's `--model` help is described by, and where the paragraph about it starts.
const MODEL_FLAG: &str = "--model";

/// The phrase that ends the part of that paragraph worth reading: what follows it is an example of a
/// full model name, which is not an alias and must not be offered as one.
const FULL_NAME: &str = "full name";

/// Claude Code's `--help`: the aliases quoted in its `--model` paragraph.
///
/// The paragraph is the line naming the flag plus the indented lines under it, and it ends at the
/// next line that starts a flag of its own. What is taken out of it is the quoted words before
/// [`FULL_NAME`] — which is the sentence boundary between "here are the aliases" and "or write the
/// whole name, like this".
fn help_aliases(printed: &str) -> Vec<Model> {
    let mut lines = printed.lines().skip_while(|line| !line.trim_start().starts_with(MODEL_FLAG));
    let Some(first) = lines.next() else {
        return Vec::new();
    };
    let mut paragraph = first.to_string();
    for line in lines {
        if line.trim_start().starts_with('-') || line.trim().is_empty() {
            break;
        }
        paragraph.push(' ');
        paragraph.push_str(line.trim());
    }
    let aliases = match paragraph.split_once(FULL_NAME) {
        Some((before, _)) => before.to_string(),
        None => paragraph,
    };
    quoted_words(&aliases)
        .map(|word| Model { id: word.to_string(), label: word.to_string() })
        .collect()
}

/// The single-quoted words in a line of help text, in the order they are written.
///
/// One word only: a quoted phrase is prose being quoted, and a model name has no spaces in it.
fn quoted_words(text: &str) -> impl Iterator<Item = &str> {
    text.split('\'')
        // The text between the first and second quote is quoted, between the second and third is
        // not, and so on — so it is the odd pieces that were inside quotes.
        .skip(1)
        .step_by(2)
        .filter(|word| !word.is_empty() && !word.contains(char::is_whitespace))
}

/// Gemini CLI's ACP answer: the `models.availableModels` of the session it opened.
///
/// The lines are read one at a time and the first that carries a session's models is the answer —
/// an agent writes what it likes on the way there (its own hello, a notification), and none of it
/// has the shape being looked for.
fn acp_session(printed: &str) -> Vec<Model> {
    #[derive(serde::Deserialize)]
    struct Message {
        result: Option<SessionResult>,
    }
    #[derive(serde::Deserialize)]
    struct SessionResult {
        models: Option<Models>,
    }
    #[derive(serde::Deserialize)]
    struct Models {
        #[serde(rename = "availableModels")]
        available: Vec<Row>,
    }
    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(rename = "modelId")]
        model_id: String,
        name: Option<String>,
    }

    printed
        .lines()
        .filter_map(|line| serde_json::from_str::<Message>(line).ok())
        .find_map(|message| message.result?.models)
        .map(|models| {
            models
                .available
                .into_iter()
                .map(|row| Model {
                    label: row.name.clone().unwrap_or_else(|| row.model_id.clone()),
                    id: row.model_id,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness;

    /// The ask on the launch row with this id, for the readers' tests to be written against the real
    /// table rather than against a shape invented here.
    fn ask(id: &str) -> &'static Ask {
        harness::find_launch(id)
            .expect("the launch catalog lists it")
            .models
            .as_ref()
            .expect("this provider can be asked")
    }

    /// Codex prints its whole catalog, hidden rows and system prompts included. What comes back is
    /// the rows it shows people, spelled the way `-m` takes them.
    #[test]
    fn codex_hidden_rows_are_left_off_and_the_slug_is_the_spelling() {
        let printed = r#"{"models":[
            {"slug":"gpt-5.5","display_name":"GPT-5.5","visibility":"show","model_messages":{"instructions_template":"You are Codex…"}},
            {"slug":"gpt-reserve","display_name":"GPT-Reserve","visibility":"hide"},
            {"slug":"gpt-5.5-mini"}
        ]}"#;
        assert_eq!(
            read(ask("codex-cli"), printed),
            vec![
                Model { id: "gpt-5.5".into(), label: "GPT-5.5".into() },
                // No display name of its own: the spelling stands in, rather than the row going missing.
                Model { id: "gpt-5.5-mini".into(), label: "gpt-5.5-mini".into() },
            ]
        );
    }

    /// OpenCode's line is the whole spelling, qualifier and all — and a line that is not one is not a
    /// model, however much it looks like output.
    #[test]
    fn opencode_takes_the_qualified_lines_and_nothing_else() {
        let printed = "opencode/big-pickle\nlmstudio/qwen/qwen3-coder-30b\n\nfetching models…\nno models found\n";
        assert_eq!(
            read(ask("opencode"), printed),
            vec![
                Model { id: "opencode/big-pickle".into(), label: "opencode/big-pickle".into() },
                Model {
                    id: "lmstudio/qwen/qwen3-coder-30b".into(),
                    label: "lmstudio/qwen/qwen3-coder-30b".into(),
                },
            ]
        );
    }

    /// Cursor draws a heading, the rows, and a tip. Only the rows are models, and which one is
    /// current is not part of a model's name.
    #[test]
    fn cursor_rows_are_read_past_the_heading_and_the_tip() {
        let printed = "Available models\n\n\
             auto - Auto (current, default)\n\
             gpt-5.3-codex - Codex 5.3\n\
             glm-5.2-max - GLM 5.2 Max (beta)\n\n\
             Tip: use --model <id> (or /model <id> in interactive mode) to switch.\n";
        assert_eq!(
            read(ask("cursor"), printed),
            vec![
                Model { id: "auto".into(), label: "Auto".into() },
                Model { id: "gpt-5.3-codex".into(), label: "Codex 5.3".into() },
                // A parenthetical that is not state stays: it is part of what the provider calls it.
                Model { id: "glm-5.2-max".into(), label: "GLM 5.2 Max (beta)".into() },
            ]
        );
    }

    /// Claude Code names its aliases in a sentence, and the same sentence goes on to give an example
    /// of a full model name. The example is not an alias, and the flags underneath are not models.
    #[test]
    fn claude_takes_the_aliases_and_not_the_full_name_example() {
        let printed = "  --settings <file>                     Path to a settings file\n\
             \x20 --model <model>                       Model for the current session. Provide\n\
             \x20                                       an alias for the latest model (e.g.\n\
             \x20                                       'fable', 'opus', or 'sonnet') or a\n\
             \x20                                       model's full name (e.g.\n\
             \x20                                       'claude-fable-5').\n\
             \x20 --agents <json>                       JSON object with 'agents' in it\n";
        assert_eq!(
            read(ask("claude-code"), printed),
            vec![
                Model { id: "fable".into(), label: "fable".into() },
                Model { id: "opus".into(), label: "opus".into() },
                Model { id: "sonnet".into(), label: "sonnet".into() },
            ]
        );
    }

    /// A help text that no longer talks about models this way is an empty list, not a wrong one —
    /// the whole risk this reader is taken on knowing about.
    #[test]
    fn a_help_text_without_the_paragraph_says_nothing() {
        assert!(read(ask("claude-code"), "  --print   Print the response and exit\n").is_empty());
    }

    /// Gemini's ACP answer arrives after its own hello, and the ids it hands over are not the names
    /// its own screen draws — the one provider where the two have to be carried apart.
    #[test]
    fn gemini_reads_the_session_answer_and_keeps_both_spellings() {
        let printed = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1,\"agentInfo\":{\"name\":\"gemini-cli\"}}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"s-1\",\"models\":{\"availableModels\":[\
             {\"modelId\":\"auto\",\"name\":\"Auto\"},\
             {\"modelId\":\"gemini-3.1-pro-preview-customtools\",\"name\":\"gemini-3.1-pro-preview\"}],\
             \"currentModelId\":\"auto\"}}}\n";
        assert_eq!(
            read(ask("gemini-cli"), printed),
            vec![
                Model { id: "auto".into(), label: "Auto".into() },
                Model {
                    id: "gemini-3.1-pro-preview-customtools".into(),
                    label: "gemini-3.1-pro-preview".into(),
                },
            ]
        );
    }

    /// The ACP one is read as it arrives, because it never ends: the caller stops at the line that
    /// answers, and until then there is nothing to stop at.
    #[test]
    fn only_the_acp_answer_says_when_to_stop_reading() {
        let acp = ask("gemini-cli");
        let hello = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1}}\n";
        assert!(!answered(acp, hello), "a hello is not the answer");
        let session = format!(
            "{hello}{{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{{\"models\":{{\"availableModels\":[{{\"modelId\":\"auto\"}}]}}}}}}\n"
        );
        assert!(answered(acp, &session));
        // The five that print and exit end where their output ends, so nothing is ever waited for.
        assert!(!answered(ask("codex-cli"), "{\"models\":[{\"slug\":\"gpt-5.5\"}]}"));
    }

    /// The ACP question is two messages, and the second opens the session in the folder it was given.
    #[test]
    fn the_acp_question_is_a_hello_and_then_a_session() {
        let said = said_to(ask("gemini-cli"), Path::new("/tmp/here"));
        assert_eq!(said.len(), 2, "two messages: {said:?}");
        assert!(said[0].contains("\"method\":\"initialize\""), "{}", said[0]);
        assert!(said[1].contains("\"method\":\"session/new\""), "{}", said[1]);
        assert!(said[1].contains("/tmp/here"), "the session opens where it was told: {}", said[1]);
        // Every one of the others answers a command line, with nothing said to it.
        assert!(said_to(ask("codex-cli"), Path::new("/tmp/here")).is_empty());
    }

    /// Nothing that comes back in a shape this does not know is ever an error, and none of it is
    /// ever a model. Each reader is given the others' output, which is the realistic shape of a
    /// provider that changed under us.
    #[test]
    fn output_in_the_wrong_shape_is_no_models_rather_than_a_failure() {
        let outputs = [
            "",
            "Authentication required. Run 'agent login'.\n",
            "{\"models\":[{\"slug\":\"gpt-5.5\"}]}",
            "opencode/big-pickle\n",
            "auto - Auto\n",
            "<html><body>502 Bad Gateway</body></html>",
        ];
        for id in ["codex-cli", "opencode", "cursor", "claude-code", "gemini-cli"] {
            for printed in outputs {
                let found = read(ask(id), printed);
                assert!(
                    found.iter().all(|one| !one.id.is_empty()),
                    "{id} made a model with no spelling out of {printed:?}"
                );
            }
        }
        // The one shape that is a real answer for somebody else, read by the one it is not for.
        assert!(read(ask("codex-cli"), "auto - Auto\n").is_empty());
        assert!(read(ask("cursor"), "{\"models\":[{\"slug\":\"gpt-5.5\"}]}").is_empty());
    }

    /// The provider with no list door has no ask at all — "nothing to run" is the absence of a row
    /// rather than a run that comes back empty (`AMB-T-4581`).
    #[test]
    fn github_copilot_is_not_asked_because_there_is_nothing_to_ask() {
        assert!(
            harness::find_launch("github-copilot").expect("catalogued").models.is_none(),
            "copilot has no list command and no ACP model option"
        );
    }

    /// Every other catalogued provider can be asked, and what it is asked is its own command with
    /// arguments — never a shell line, and never another program's name.
    #[test]
    fn every_other_provider_is_asked_with_its_own_command() {
        for launch in harness::LAUNCHES {
            let Some(ask) = &launch.models else {
                continue;
            };
            assert!(!ask.args.is_empty(), "{} is asked with nothing", launch.id);
            assert!(
                ask.args.iter().all(|arg| !arg.contains(char::is_whitespace)),
                "{} is asked with an argument that is really two",
                launch.id
            );
        }
    }
}
