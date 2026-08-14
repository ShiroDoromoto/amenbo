//! The `mcp` domain: amenbo reached over a protocol rather than typed at.
//!
//! Every other domain here drives the shipped binary one invocation at a time and reads what it
//! printed. This one drives it as a **server**: started once for a folder, kept alive for the length
//! of the road, and spoken to in JSON-RPC over its two streams. That is the whole reason it is a
//! domain of its own — what a road walks is the protocol, and a protocol has a conversation where a
//! command has an exit code.
//!
//! **The binary is the same one, in the same isolated store.** A server that reached anywhere else
//! would be a second amenbo and would prove nothing about the one under test, so it is spawned
//! through the same invocation the rest of the driver uses: the run's `AMENBO_HOME`, and the folder
//! the step names.
//!
//! **What the road reads is what a host would.** A tool that ran and refused comes back as a result
//! marked in error rather than as a transport fault, which is what puts the reason in front of the
//! model instead of leaving the host to swallow it — so the assert asks about the result, and a
//! JSON-RPC `error` is a failure of this harness's own making.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

use amenbo_scenario::{Args, Domain};
use serde_json::{json, Value};

use crate::{req_bool, req_str, unmapped, Driver, Outcome};

/// The revision this harness speaks. It is one of the ones the server publishes, named on purpose: a
/// caller that asked for something else would be answered in the newest, and what a road wants to
/// know is that the revision it asked for is the revision it got.
const PROTOCOL: &str = "2025-06-18";

/// A server standing for one folder, and the conversation held with it.
pub(crate) struct Standing {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    /// The next id to send. Ids are the caller's to keep unique, and a reply is matched to its
    /// request by one — so they are counted here rather than reused.
    next_id: i64,
    /// The tools the server published when it came up.
    tools: Vec<String>,
    /// What the last `tools/call` came back with — the `result` object, whatever it says.
    last: Option<Value>,
}

impl Standing {
    /// Send one request and read its reply. The server answers a request before it reads the next,
    /// so the reply to read is the next line — a notification would break that, and this harness
    /// sends none.
    fn ask(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.stdin, "{line}").map_err(|e| format!("could not send `{method}`: {e}"))?;
        self.stdin.flush().map_err(|e| format!("could not send `{method}`: {e}"))?;

        let mut answer = String::new();
        match self.stdout.read_line(&mut answer) {
            Ok(0) => return Err(format!("the server closed its stream instead of answering `{method}`")),
            Ok(_) => {}
            Err(e) => return Err(format!("could not read the answer to `{method}`: {e}")),
        }
        let read: Value = serde_json::from_str(answer.trim())
            .map_err(|e| format!("the answer to `{method}` is not JSON-RPC: {e} — {answer}"))?;
        if let Some(error) = read.get("error") {
            return Err(format!("`{method}` came back as a protocol error: {error}"));
        }
        Ok(read["result"].clone())
    }
}

impl Drop for Standing {
    /// A server outlives every step of its road and nothing else. Closing its input is the way it is
    /// asked to stop — the same end a host gives it — and the kill is what keeps a run that ended
    /// early from leaving a process behind.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Driver<'_> {
    /// The server a road stood up, for an assert to read. An assert has no `serve` of its own to
    /// fall back on, so a road that asks about one it never stood is answered as the mistake it is
    /// rather than as a verdict.
    fn standing(&self) -> Result<&Standing, String> {
        self.server.as_ref().ok_or_else(|| {
            "no server is standing — this asks about one a `serve` step has to put there".to_string()
        })
    }

    pub(crate) fn mcp_action(&mut self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // Stand the server up for one folder, settle the handshake, and take what it publishes.
            // All three at once because none of them is a move a host makes on its own: a server is
            // started, and being spoken to at all is what the rest of the road is.
            "serve" => {
                let dir = self.folder(with)?;
                let mut child = Command::new(&self.bin)
                    .args(["mcp", "--dir"])
                    .arg(&dir)
                    .current_dir(&self.session.cwd)
                    .env("AMENBO_HOME", &self.session.home)
                    .env("AMENBO_UPDATE_CHECK", "0")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| format!("could not start the server for {}: {e}", dir.display()))?;
                let stdin = child.stdin.take().ok_or("the server took no input")?;
                let stdout = child.stdout.take().ok_or("the server offered no output")?;
                let mut standing = Standing {
                    child,
                    stdin,
                    stdout: BufReader::new(stdout),
                    next_id: 1,
                    tools: Vec::new(),
                    last: None,
                };

                let hello = standing.ask(
                    "initialize",
                    json!({
                        "protocolVersion": PROTOCOL,
                        "capabilities": {},
                        "clientInfo": { "name": "amenbo-verify", "version": "0" },
                    }),
                )?;
                let spoken = hello["protocolVersion"].as_str().unwrap_or_default().to_string();
                let published = standing.ask("tools/list", json!({}))?;
                standing.tools = published["tools"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                    .collect();

                let listed = standing.tools.join(", ");
                self.server = Some(standing);
                Ok(Outcome::action(format!(
                    "a server is standing for {} (speaking {spoken}, offering {listed})",
                    dir.display()
                )))
            }
            // One tool called, with the words the caller sends under it. The answer is kept for the
            // assert that follows, the way a plugin's return value is.
            "call" => {
                let tool = req_str(with, "tool")?.to_string();
                let mut arguments = serde_json::Map::new();
                if let Some(words) = with.get("args") {
                    let words = words
                        .as_sequence()
                        .ok_or("`args` is the words the tool is called with, so it is a list")?;
                    let mut said = Vec::new();
                    for word in words {
                        said.push(Value::String(
                            word.as_str().ok_or("every entry under `args` must be a string")?.to_string(),
                        ));
                    }
                    arguments.insert("args".to_string(), Value::Array(said));
                }
                // The one tool that takes something other than a line of words, named the way its
                // schema names it.
                if let Some(command) = with.get("command").and_then(|v| v.as_str()) {
                    arguments.insert("command".to_string(), Value::String(command.to_string()));
                }
                let standing = self
                    .server
                    .as_mut()
                    .ok_or("no server is standing — a call needs a `serve` before it")?;
                let result = standing.ask(
                    "tools/call",
                    json!({ "name": tool, "arguments": Value::Object(arguments) }),
                )?;
                let errored = result["isError"].as_bool().unwrap_or(false);
                let said = text_of(&result).len();
                standing.last = Some(result);
                Ok(Outcome::action(format!(
                    "called `{tool}` — it came back {} with {said} byte(s)",
                    if errored { "in error" } else { "cleanly" }
                )))
            }
            _ => Err(unmapped(Domain::Mcp, op)),
        }
    }

    pub(crate) fn mcp_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // What the standing server published when it came up.
            "offers" => {
                let tool = req_str(with, "tool")?;
                let present = req_bool(with, "present")?;
                let standing = self.standing()?;
                let found = standing.tools.iter().any(|one| one == tool);
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the server {} `{tool}` (it offers {}; expected it {}, {})",
                        if found { "offers" } else { "does not offer" },
                        match standing.tools.as_slice() {
                            [] => "nothing".to_string(),
                            tools => tools.join(", "),
                        },
                        if present { "offered" } else { "left out" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // And what the last call came back with. `ok` reads the result's own mark rather than the
            // transport: a tool that refused answered, and the model is meant to read the refusal.
            "answered" => {
                let standing = self.standing()?;
                let last = standing
                    .last
                    .as_ref()
                    .ok_or("nothing has been called yet, so there is no answer to read")?;
                let errored = last["isError"].as_bool().unwrap_or(false);
                let said = text_of(last);

                if let Some(want) = crate::opt_bool(with, "ok") {
                    let pass = errored != want;
                    return Ok(Outcome::assert(
                        pass,
                        format!(
                            "the call came back {} (expected {}, {}) — {said}",
                            if errored { "in error" } else { "cleanly" },
                            if want { "cleanly" } else { "in error" },
                            if pass { "as expected" } else { "MISMATCH" }
                        ),
                    ));
                }
                let want = req_str(with, "contains")?;
                let pass = said.contains(want);
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the answer {} `{want}` ({}) — {said}",
                        if pass { "carries" } else { "does not carry" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Mcp, op)),
        }
    }
}

/// Everything the answer said, as one string. A result carries its text in blocks, and a road asking
/// whether a word is in the answer means the answer whole — splitting it would make where a sentence
/// happened to be cut part of what is being tested.
fn text_of(result: &Value) -> String {
    result["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A server of the test's own, standing in for the shipped one: it reads a request per line and
    /// answers from a script written beside it. What is under test here is this harness's half of the
    /// conversation — the ids, the one-reply-per-request reading, and the way a result is read back —
    /// which is the half no scenario run can check on its own (a road that came back red would not say
    /// whether the fault was the server's or the questioner's).
    fn stand_in(answers: &[&str]) -> Standing {
        // One script per stand-in, not one per process: these tests run beside each other, and two
        // of them writing the same file would leave the second reading the first one's answers.
        static STOOD: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let nth = STOOD.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory to write into");
        let path = dir.join(format!("server-{nth}.sh"));
        // The last thing it does is take a question and go, whatever it answered before that: a
        // stand-in that simply exited would race the request being written, and a write that lost
        // that race is a broken pipe rather than the silence this is about.
        let script = answers
            .iter()
            .map(|answer| format!("read -r _line\nprintf '%s\\n' '{answer}'"))
            .chain(std::iter::once("read -r _line".to_string()))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("written");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("runnable");

        let mut child = Command::new(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in starts");
        let stdin = child.stdin.take().expect("its input");
        let stdout = child.stdout.take().expect("its output");
        Standing {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            tools: Vec::new(),
            last: None,
        }
    }

    /// The shape of one exchange: a request goes out under an id of its own, and the `result` comes
    /// back. Two of them in a row is what the handshake and the tool list are.
    #[test]
    fn each_request_carries_its_own_id_and_reads_back_one_reply() {
        let mut standing = stand_in(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18"}}"#,
            r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"run"}]}}"#,
        ]);

        let hello = standing.ask("initialize", json!({})).expect("answered");
        assert_eq!(hello["protocolVersion"], "2025-06-18");
        assert_eq!(standing.next_id, 2, "the next request takes the next id");

        let published = standing.ask("tools/list", json!({})).expect("answered");
        assert_eq!(published["tools"][0]["name"], "run");
    }

    /// A protocol error is this harness's own failure, not a verdict. A tool that ran and refused
    /// comes back as a *result* marked in error, which is what an assert reads — so an `error` object
    /// means the question was wrong, and the run says so instead of walking on.
    #[test]
    fn a_protocol_error_stops_the_step_rather_than_being_read_as_an_answer() {
        let mut standing =
            stand_in(&[r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"no such method"}}"#]);

        let refused = standing.ask("tools/list", json!({})).expect_err("a protocol error");
        assert!(refused.contains("protocol error"), "{refused}");
        assert!(refused.contains("no such method"), "the reason travels: {refused}");
    }

    /// A server that takes the question and goes is said so, rather than read as an empty answer —
    /// which would pass an assert about something nobody was told.
    #[test]
    fn a_server_that_closed_its_stream_is_not_an_empty_answer() {
        let mut standing = stand_in(&[]);

        let gone = standing.ask("initialize", json!({})).expect_err("nothing to read");
        assert!(gone.contains("closed its stream"), "{gone}");
    }

    /// Everything the answer said, however many blocks it arrived in. A road asking whether a word is
    /// in the answer means the answer whole.
    #[test]
    fn the_answer_is_read_as_one_text_however_it_was_blocked() {
        let result = json!({
            "content": [
                { "type": "text", "text": "first" },
                { "type": "text", "text": "second" },
            ],
            "isError": false,
        });
        assert_eq!(text_of(&result), "first\nsecond");
        assert_eq!(text_of(&json!({})), "", "an answer with no blocks says nothing");
    }
}
