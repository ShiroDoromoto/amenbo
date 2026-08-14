//! `amenbo mcp` — the MCP server: JSON-RPC over stdin/stdout, for an AI whose host cannot open a
//! folder of its own (`AMB-D-665`).
//!
//! It is a mediator, not a second implementation. Every tool call re-runs this executable
//! (`current_exe`) as a child process with its working directory fixed to the folder the server was
//! started for, and hands the host what that run wrote. So the startup, the integrity check, the
//! pointer repair and the reach are the CLI's own, decided once and in one place — and a folder that
//! is not bound, or a store this build cannot read, is refused in the same words a person typing there
//! would read.
//!
//! One server serves one folder (`AMB-D-666`): the folder arrives as `--dir` at startup, and nothing a
//! caller sends can move it. That is what keeps `--project`, which is a person's word and not an AI's,
//! out of reach from this side.
//!
//! This build carries the two reading tools alone — `agent` and `agent_command`. Neither writes, so
//! there is nothing here for a facet to name and nothing to shape the caller's own arguments against;
//! `run`, which takes both on, is its own step.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde_json::{json, Value};

use amenbo_core::agent::VERSION;
use amenbo_core::config::Paths;

/// The protocol revisions this server can speak, newest first. A caller naming one of these is
/// answered in its own revision; anything else is answered in the newest, which is what the spec's
/// negotiation asks for — the caller then decides whether it can live with the offer.
const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC's own codes, the only ones this server originates. A tool that ran and failed is **not**
/// one of these: that is a result with `isError`, so the model reads what went wrong instead of the
/// host swallowing it as a transport fault.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What one run of the child left behind: whether it succeeded, and the text to hand back.
pub struct Ran {
    pub ok: bool,
    pub text: String,
}

/// How a tool call reaches amenbo. The server's own implementation re-runs this executable; the tests
/// put a recorder here, so what the argument shaping produces can be read without forking anything.
pub trait CallAmenbo {
    fn call(&self, args: &[String]) -> Ran;
}

/// The real caller: this executable again, in the folder the server was started for.
struct SelfCall {
    dir: PathBuf,
}

impl CallAmenbo for SelfCall {
    fn call(&self, args: &[String]) -> Ran {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => return Ran { ok: false, text: format!("Cannot find the amenbo executable: {e}") },
        };
        // No shell in between (`AMB-D-667`): the executable is invoked directly, so nothing in an
        // argument is read as syntax by anybody. stdin is closed because this process's own stdin is
        // the protocol stream — a child inheriting it would eat the host's next request.
        let out = std::process::Command::new(exe)
            .args(args)
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .output();
        match out {
            Err(e) => Ran { ok: false, text: format!("Cannot run amenbo in {}: {e}", self.dir.display()) },
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim_end().to_string();
                if out.status.success() {
                    return Ran { ok: true, text: stdout };
                }
                // A refusal is written on stderr, as JSON when `--json` was asked for — which is what
                // every call here asks for. Fall back to stdout, and to the exit code itself, so a
                // failure never arrives as an empty answer.
                let text = [stderr, stdout]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or_else(|| match out.status.code() {
                        Some(code) => format!("amenbo exited with {code} and said nothing."),
                        None => "amenbo was ended before it said anything.".to_string(),
                    });
                Ran { ok: false, text }
            }
        }
    }
}

/// Serve the folder until the host closes the stream. The return value is this process's exit code.
///
/// Both streams belong to the protocol: nothing else may be written to stdout, and what this server
/// has to say for itself goes to stderr, where the host logs it.
pub fn serve(dir: &Path) -> i32 {
    let caller = SelfCall { dir: dir.to_path_buf() };
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                eprintln!("amenbo mcp: cannot read the request stream: {e}");
                return 1;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let Some(reply) = respond(&line, &caller) else { continue };
        // A host that walked away is not an error to report: it is the end of the session, and the
        // next read would say so anyway.
        if writeln!(stdout, "{reply}").and_then(|()| stdout.flush()).is_err() {
            return 0;
        }
    }
    0
}

/// Answer one message. `None` is a notification — a message with no `id`, which the protocol says is
/// never replied to, however it turned out.
fn respond(line: &str, caller: &dyn CallAmenbo) -> Option<Value> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(msg) => msg,
        // Nothing was parsed, so there is no id to answer under. Null is what the protocol asks for.
        Err(e) => return Some(error(Value::Null, PARSE_ERROR, &format!("Invalid JSON: {e}"))),
    };
    // A batch — an array of messages — is the one shape that parses and still carries no id to answer
    // under. The revision this server prefers has no batching in it, and answering under a null id is
    // what leaves the host something to read instead of a silence it waits out.
    if !msg.is_object() {
        return Some(error(Value::Null, INVALID_REQUEST, "A message must be one JSON object; batches are not served."));
    }
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str);
    let Some(method) = method else {
        return id.map(|id| error(id, INVALID_REQUEST, "A request must name a method."));
    };
    // A notification is answered by acting, not by replying. The two this server can be sent —
    // `notifications/initialized` and `notifications/cancelled` — need nothing done, and anything else
    // is one it does not know: silence is the whole of the protocol's answer either way.
    let id = id?;
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => reply(id, initialize(&params)),
        "ping" => reply(id, json!({})),
        "tools/list" => reply(id, json!({ "tools": tools() })),
        "tools/call" => match call(&params, caller) {
            Ok(result) => reply(id, result),
            Err((code, message)) => error(id, code, &message),
        },
        other => error(id, METHOD_NOT_FOUND, &format!("No method named '{other}'.")),
    })
}

/// The handshake. The caller's revision is echoed back when this build knows it, so a host that speaks
/// an older one is not told to speak a newer.
fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        // Tools, and nothing else: no resources, no prompts, and no list that changes under the host.
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": Paths::command_name(), "version": VERSION },
        "instructions": "Call `agent` first and follow what it says — it is how work is done in this folder, in full. `agent_command` pulls one command's flags, arguments and examples when you are about to use it.",
    })
}

/// The tools this build serves. Descriptions are written for the model that reads the list, not for a
/// person: what the tool answers, and when to reach for it.
fn tools() -> Value {
    json!([
        {
            "name": "agent",
            "description": "How to work in this folder, in full: the working practice to follow, the rules that bind it, and an index of every command by name. Call this first, before anything else, and follow what it says. It also reports what is installed here and whether this build is current.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        },
        {
            "name": "agent_command",
            "description": "One command's full spec — summary, arguments, flags, examples — pulled by the name the index in `agent` lists it under (compound names carry a space, as in `task add`). Read this before using a command instead of guessing a flag from its name.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "the command's name as the agent index lists it, for example `task add` or `decision show`",
                    },
                },
                "required": ["command"],
                "additionalProperties": false,
            },
        },
    ])
}

/// Run one tool call. `Err` is a protocol fault — a tool nobody serves, an argument that is not there —
/// which is the caller's mistake to correct. A tool that ran and was refused comes back as `Ok` with
/// `isError`, so the refusal reaches the model in amenbo's own words.
fn call(params: &Value, caller: &dyn CallAmenbo) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| (INVALID_PARAMS, "A tool call must name a tool.".to_string()))?;
    let args = params.get("arguments").unwrap_or(&Value::Null);
    let argv = argv_for(name, args)?;
    let ran = caller.call(&argv);
    Ok(json!({
        "content": [{ "type": "text", "text": ran.text }],
        "isError": !ran.ok,
    }))
}

/// What one tool call becomes on the command line. Everything a tool takes is named here — a caller's
/// arguments never reach the child as they arrived, which is what keeps this build's two tools to the
/// reading they are.
fn argv_for(name: &str, args: &Value) -> Result<Vec<String>, (i64, String)> {
    let line: Vec<&str> = match name {
        "agent" => vec!["agent", "--json"],
        "agent_command" => {
            let command = args
                .get("command")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .ok_or_else(|| {
                    (INVALID_PARAMS, "agent_command takes `command`: the name of the command to describe.".to_string())
                })?;
            return Ok(["agent", "--command", command, "--json"].iter().map(|a| (*a).to_string()).collect());
        }
        other => return Err((INVALID_PARAMS, format!("No tool named '{other}'."))),
    };
    Ok(line.iter().map(|a| (*a).to_string()).collect())
}

fn reply(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A caller that records what it was asked to run and answers with a fixed reply — the shaping is
    /// the subject here, and forking the binary would say nothing more about it.
    struct Recorder {
        seen: RefCell<Vec<Vec<String>>>,
        ok: bool,
    }

    impl Recorder {
        fn new() -> Recorder {
            Recorder { seen: RefCell::new(Vec::new()), ok: true }
        }

        fn failing() -> Recorder {
            Recorder { seen: RefCell::new(Vec::new()), ok: false }
        }

        fn last(&self) -> Vec<String> {
            self.seen.borrow().last().cloned().expect("nothing was run")
        }
    }

    impl CallAmenbo for Recorder {
        fn call(&self, args: &[String]) -> Ran {
            self.seen.borrow_mut().push(args.to_vec());
            Ran { ok: self.ok, text: "what amenbo wrote".to_string() }
        }
    }

    fn ask(line: &str, caller: &dyn CallAmenbo) -> Value {
        respond(line, caller).expect("a request is always answered")
    }

    #[test]
    fn the_handshake_answers_in_the_caller_s_own_revision() {
        let caller = Recorder::new();
        let out = ask(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
            &caller,
        );
        assert_eq!(out["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(out["id"], 1);
        assert_eq!(out["jsonrpc"], "2.0");
        assert!(out["result"]["capabilities"]["tools"].is_object(), "tools are what this server offers");
        assert_eq!(out["result"]["serverInfo"]["version"], VERSION);
    }

    #[test]
    fn a_revision_this_build_does_not_know_is_answered_with_the_newest_it_speaks() {
        let caller = Recorder::new();
        let out = ask(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
            &caller,
        );
        assert_eq!(out["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn the_listing_carries_the_two_reading_tools() {
        let caller = Recorder::new();
        let out = ask(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &caller);
        let tools = out["result"]["tools"].as_array().expect("tools is an array");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, vec!["agent", "agent_command"], "this build serves the reading two");
        for tool in tools {
            assert!(
                tool["description"].as_str().is_some_and(|d| !d.is_empty()),
                "a tool nobody described is one the model cannot pick: {tool}",
            );
            assert_eq!(tool["inputSchema"]["type"], "object", "every tool declares an object schema: {tool}");
        }
    }

    #[test]
    fn agent_runs_the_entry_point() {
        let caller = Recorder::new();
        let out = ask(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"agent","arguments":{}}}"#, &caller);
        assert_eq!(caller.last(), vec!["agent", "--json"]);
        assert_eq!(out["result"]["content"][0]["text"], "what amenbo wrote");
        assert_eq!(out["result"]["isError"], false);
    }

    #[test]
    fn agent_command_carries_the_name_it_was_given() {
        let caller = Recorder::new();
        ask(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"agent_command","arguments":{"command":"task add"}}}"#,
            &caller,
        );
        assert_eq!(caller.last(), vec!["agent", "--command", "task add", "--json"]);
    }

    #[test]
    fn agent_command_without_a_name_is_the_caller_s_mistake_and_runs_nothing() {
        let caller = Recorder::new();
        let out = ask(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"agent_command","arguments":{"command":"  "}}}"#,
            &caller,
        );
        assert_eq!(out["error"]["code"], INVALID_PARAMS);
        assert!(caller.seen.borrow().is_empty(), "nothing is run for a call that cannot be shaped");
    }

    #[test]
    fn a_tool_nobody_serves_is_refused_by_name() {
        let caller = Recorder::new();
        let out = ask(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"run","arguments":{}}}"#, &caller);
        assert_eq!(out["error"]["code"], INVALID_PARAMS);
        assert!(out["error"]["message"].as_str().is_some_and(|m| m.contains("run")), "the refusal names it");
        assert!(caller.seen.borrow().is_empty());
    }

    /// A refusal amenbo wrote reaches the model as the tool's own answer, not as a transport fault —
    /// otherwise the host reports "the tool broke" and what amenbo actually said is lost.
    #[test]
    fn a_refusal_comes_back_as_the_tool_s_result() {
        let caller = Recorder::failing();
        let out = ask(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"agent","arguments":{}}}"#, &caller);
        assert!(out.get("error").is_none(), "a tool that ran is not a protocol fault: {out}");
        assert_eq!(out["result"]["isError"], true);
        assert_eq!(out["result"]["content"][0]["text"], "what amenbo wrote");
    }

    #[test]
    fn a_notification_is_never_answered() {
        let caller = Recorder::new();
        assert!(respond(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &caller).is_none());
        assert!(respond(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{}}"#, &caller).is_none());
        assert!(caller.seen.borrow().is_empty());
    }

    #[test]
    fn a_method_this_server_does_not_serve_says_so_under_the_caller_s_id() {
        let caller = Recorder::new();
        let out = ask(r#"{"jsonrpc":"2.0","id":"abc","method":"resources/list"}"#, &caller);
        assert_eq!(out["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(out["id"], "abc");
    }

    #[test]
    fn text_that_is_not_json_is_answered_under_a_null_id() {
        let caller = Recorder::new();
        let out = ask("{not json", &caller);
        assert_eq!(out["error"]["code"], PARSE_ERROR);
        assert_eq!(out["id"], Value::Null);
    }

    /// A batch parses and names no id, so silence would be indistinguishable from a request lost —
    /// and the host would wait it out.
    #[test]
    fn a_batch_is_answered_rather_than_swallowed() {
        let caller = Recorder::new();
        let out = ask(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#, &caller);
        assert_eq!(out["error"]["code"], INVALID_REQUEST);
        assert_eq!(out["id"], Value::Null);
    }

    #[test]
    fn a_request_naming_no_method_is_answered_and_a_notification_naming_none_is_not() {
        let caller = Recorder::new();
        let out = ask(r#"{"jsonrpc":"2.0","id":8}"#, &caller);
        assert_eq!(out["error"]["code"], INVALID_REQUEST);
        assert!(respond(r#"{"jsonrpc":"2.0"}"#, &caller).is_none());
    }

    /// One line out per line in, and no newline inside it — the framing this transport is.
    #[test]
    fn an_answer_is_one_line() {
        let caller = Recorder::new();
        let out = ask(r#"{"jsonrpc":"2.0","id":9,"method":"tools/list"}"#, &caller);
        let line = serde_json::to_string(&out).unwrap();
        assert!(!line.contains('\n'), "a reply that wraps splits into two messages: {line}");
    }
}
