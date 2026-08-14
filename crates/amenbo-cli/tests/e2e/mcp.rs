//! The MCP server as a host meets it: a real `amenbo mcp` process, driven over its own two streams.
//!
//! What only a real process can show is the part the unit tests stub out — that a tool call reaches
//! amenbo at all, and that it reaches it **in the folder `--dir` names**. Every server here is
//! started from a folder that is bound to nothing, so an answer that could only have come from the
//! bound folder is the proof (`AMB-D-666`).

mod harness;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

use serde_json::{json, Value};

use harness::*;

/// A running server, and the two streams that are the whole of talking to it.
struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Server {
    /// Start one for `dir`, from `cwd` — which the tests deliberately make a folder that decides
    /// nothing, so nothing here can pass by accident.
    fn start(home: &std::path::Path, cwd: &std::path::Path, dir: &std::path::Path) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(cwd)
            .args(["mcp", "--dir", &dir.to_string_lossy()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to start the MCP server");
        let stdin = child.stdin.take().expect("the server's stdin");
        let stdout = BufReader::new(child.stdout.take().expect("the server's stdout"));
        Server { child, stdin, stdout }
    }

    /// Send a request and read its answer. One line each way — that is this transport's framing.
    fn ask(&mut self, id: i64, method: &str, params: Value) -> Value {
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.stdin, "{request}").expect("failed to write the request");
        self.stdin.flush().expect("failed to flush the request");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("failed to read the answer");
        assert!(!line.trim().is_empty(), "the server answered nothing to {method}");
        let reply: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON: {e}\n{line}"));
        assert_eq!(reply["id"], id, "the answer carries the request's own id");
        reply
    }

    /// Send a notification, which is never answered.
    fn tell(&mut self, method: &str) {
        let note = json!({ "jsonrpc": "2.0", "method": method });
        writeln!(self.stdin, "{note}").expect("failed to write the notification");
        self.stdin.flush().expect("failed to flush the notification");
    }

    /// Call one tool and hand back its result.
    fn call(&mut self, id: i64, name: &str, arguments: Value) -> Value {
        let reply = self.ask(id, "tools/call", json!({ "name": name, "arguments": arguments }));
        assert!(reply.get("error").is_none(), "a tool call is not a protocol fault: {reply}");
        reply["result"].clone()
    }

    /// Close the stream the way a host that is finished does, and wait for the server to leave.
    fn stop(mut self) {
        drop(self.stdin);
        let out = self.child.wait().expect("failed to wait for the server");
        assert_eq!(out.code(), Some(0), "a closed stream is the end of a session, not a failure");
    }
}

/// The one text a tool result carries.
fn text(result: &Value) -> String {
    result["content"][0]["text"].as_str().unwrap_or_else(|| panic!("no text in {result}")).to_string()
}

/// A folder bound to a project, and a folder bound to nothing — the two the tests need.
fn a_bound_and_an_unbound_folder(cli: &Cli) -> (std::path::PathBuf, std::path::PathBuf) {
    let bound = amenbo_scratch::scratch("mcp-bound");
    let unbound = amenbo_scratch::scratch("mcp-unbound");
    std::fs::create_dir_all(&bound).unwrap();
    std::fs::create_dir_all(&unbound).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&bound)
        .args(["init", "--name", "MCP", "--actor", "human"])
        .output()
        .expect("failed to run init");
    assert_eq!(exit_code(&out), 0, "init: {}", String::from_utf8_lossy(&out.stderr));
    (bound, unbound)
}

/// The handshake, the listing, and the reading tools — the whole of a session a host actually runs.
#[test]
fn a_host_shakes_hands_lists_the_tools_and_calls_them() {
    let cli = Cli::new();
    let (bound, unbound) = a_bound_and_an_unbound_folder(&cli);
    let mut server = Server::start(&cli.home, &unbound, &bound);

    let hello = server.ask(1, "initialize", json!({ "protocolVersion": "2025-06-18" }));
    assert_eq!(hello["result"]["protocolVersion"], "2025-06-18");
    assert!(hello["result"]["capabilities"]["tools"].is_object());
    assert!(hello["result"]["serverInfo"]["name"].as_str().is_some_and(|n| !n.is_empty()));
    // The host says it is ready, and is answered with silence — the next request proves the server
    // did not try to reply to it, since a stray line would land here instead.
    server.tell("notifications/initialized");

    let listed = server.ask(2, "tools/list", json!({}));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names, vec!["agent", "agent_command", "run"]);

    // `agent` answers with the entry point, and with the part of it only a **bound** folder has: the
    // server was started somewhere else, so this could only have been read where `--dir` pointed.
    let result = server.call(3, "agent", json!({}));
    assert_eq!(result["isError"], false, "the entry point is there to be read: {result}");
    let spec: Value = serde_json::from_str(&text(&result)).expect("the tool hands back amenbo's own JSON");
    assert!(spec["agentCycle"].is_object(), "the entry point carries the cycle");
    assert!(
        spec["setup_incomplete"].is_object(),
        "a folder bound to nothing has no setup report — this one came from the bound folder",
    );

    // `agent_command` pulls the one command it was asked for.
    let result = server.call(4, "agent_command", json!({ "command": "task add" }));
    assert_eq!(result["isError"], false, "{result}");
    let spec: Value = serde_json::from_str(&text(&result)).expect("one command's spec, as JSON");
    assert_eq!(spec["name"], "task add");
    assert!(spec["flags"].is_array(), "the detail layer is what this tool is for");

    server.stop();
}

/// A refusal amenbo wrote is the tool's own answer, in amenbo's own words. Anything else and the host
/// reports that the tool broke, and what amenbo said is lost.
#[test]
fn what_amenbo_refuses_reaches_the_caller_as_amenbo_wrote_it() {
    let cli = Cli::new();
    let (bound, unbound) = a_bound_and_an_unbound_folder(&cli);
    let mut server = Server::start(&cli.home, &unbound, &bound);

    let result = server.call(1, "agent_command", json!({ "command": "no-such-command" }));
    assert_eq!(result["isError"], true, "a command nobody registered is a refusal: {result}");
    let refusal: Value = serde_json::from_str(&text(&result)).expect("the refusal is amenbo's own JSON");
    assert_eq!(refusal["error"]["code"], "unknown_command");

    server.stop();
}

/// `run` types the caller's own words at amenbo, in the folder the server was given — and the facet it
/// types is this server's, whatever the caller wrote (`AMB-D-668`).
#[test]
fn run_reaches_the_bound_folder_and_the_facet_is_never_the_caller_s() {
    let cli = Cli::new();
    let (bound, unbound) = a_bound_and_an_unbound_folder(&cli);
    let mut server = Server::start(&cli.home, &unbound, &bound);

    // A write, with the caller naming the person as the actor. It lands as the AI's: an AI reads and
    // writes only inside the project its folder is bound to, and passing `human` through would take
    // both of those away.
    let result = server.call(
        1,
        "run",
        json!({ "args": ["task", "add", "--title", "over the wire", "--json", "--actor", "human"] }),
    );
    assert_eq!(result["isError"], false, "{result}");
    let added: Value = serde_json::from_str(&text(&result)).expect("amenbo's own JSON");
    assert_eq!(added["acted_facet"], "ai", "the server names the facet, not the caller: {added}");
    let id = added["task"]["ref"].as_str().expect("the new task's ref").to_string();

    // And the task is in the bound folder's project — the server's own folder is bound to nothing, so an
    // AI standing there would have reached no project at all.
    let result = server.call(2, "run", json!({ "args": ["task", "list", "--json"] }));
    assert_eq!(result["isError"], false, "{result}");
    let listed: Value = serde_json::from_str(&text(&result)).expect("amenbo's own JSON");
    let refs: Vec<&str> =
        listed["tasks"].as_array().expect("tasks").iter().filter_map(|t| t["ref"].as_str()).collect();
    assert!(refs.contains(&id.as_str()), "the write and the read are the same project: {refs:?}");

    server.stop();
}

/// `bind` would re-point the folder this server was given at another project, which is the one thing its
/// shape rests on (`AMB-D-666`). It is refused here, and the pointer stays where it was.
#[test]
fn bind_is_refused_and_the_folder_still_points_where_it_did() {
    let cli = Cli::new();
    let (bound, unbound) = a_bound_and_an_unbound_folder(&cli);
    let elsewhere = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&bound)
        .args(["project", "add", "--name", "Elsewhere", "--dir", &unbound.to_string_lossy(), "--json", "--actor", "human"])
        .output()
        .expect("failed to raise the other project");
    assert_eq!(exit_code(&elsewhere), 0, "{}", String::from_utf8_lossy(&elsewhere.stderr));

    let before = std::fs::read_to_string(bound.join(".amenbo")).expect("the pointer");
    let mut server = Server::start(&cli.home, &unbound, &bound);
    let result = server.call(1, "run", json!({ "args": ["bind", "--project", "Elsewhere"] }));
    assert_eq!(result["isError"], true, "bind is not served here: {result}");
    assert!(
        text(&result).contains("bind"),
        "the refusal names what was refused: {}",
        text(&result),
    );
    server.stop();

    assert_eq!(
        std::fs::read_to_string(bound.join(".amenbo")).expect("the pointer"),
        before,
        "nothing was written: the folder still belongs to the project it was given for",
    );
}

/// Nothing is added to the caller's words beyond the facet — `--yes` least of all, so a destructive
/// command still stops at the confirmation a person is the one to give.
#[test]
fn a_destructive_command_still_waits_for_the_confirmation() {
    let cli = Cli::new();
    let (bound, unbound) = a_bound_and_an_unbound_folder(&cli);
    let mut server = Server::start(&cli.home, &unbound, &bound);

    let added: Value = serde_json::from_str(&text(&server.call(
        1,
        "run",
        json!({ "args": ["task", "add", "--title", "to be deleted", "--json"] }),
    )))
    .expect("amenbo's own JSON");
    let id = added["task"]["ref"].as_str().expect("the new task's ref").to_string();

    let result = server.call(2, "run", json!({ "args": ["task", "delete", &id, "--json"] }));
    assert_eq!(result["isError"], true, "a destructive command is not waved through: {result}");
    let refusal: Value = serde_json::from_str(&text(&result)).expect("amenbo's own JSON");
    assert_eq!(refusal["error"]["code"], "confirmation_required");

    server.stop();
}

/// The caller's own mistakes are the protocol's, not a tool's: nothing is run for them.
#[test]
fn a_call_that_cannot_be_shaped_is_a_protocol_fault() {
    let cli = Cli::new();
    let (bound, unbound) = a_bound_and_an_unbound_folder(&cli);
    let mut server = Server::start(&cli.home, &unbound, &bound);

    let reply = server.ask(1, "tools/call", json!({ "name": "typo", "arguments": {} }));
    assert_eq!(reply["error"]["code"], -32602, "no tool goes by that name: {reply}");
    let reply = server.ask(2, "tools/call", json!({ "name": "agent_command", "arguments": {} }));
    assert_eq!(reply["error"]["code"], -32602, "`agent_command` needs a command: {reply}");
    let reply = server.ask(3, "resources/list", json!({}));
    assert_eq!(reply["error"]["code"], -32601, "no resources are served here: {reply}");
    // Still serving after three refusals — a mistake ends the call, not the session.
    let reply = server.ask(4, "ping", json!({}));
    assert!(reply["result"].is_object(), "{reply}");

    server.stop();
}

/// `--dir` naming no folder is refused where a person can read it, not once per tool call for the
/// life of a server nobody can use.
#[test]
fn a_dir_that_names_no_folder_is_refused_at_the_start() {
    let cli = Cli::new();
    let missing = cli.home.join("never-made");
    let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&cli.home)
        .args(["mcp", "--dir", &missing.to_string_lossy()])
        .stdin(Stdio::null())
        .output()
        .expect("failed to run the binary");
    assert_eq!(exit_code(&out), 2, "a bad argument is exit 2");
    assert!(out.stdout.is_empty(), "stdout belongs to the protocol, so a refusal is not written there");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--dir"),
        "the refusal names the argument: {}",
        String::from_utf8_lossy(&out.stderr),
    );
}
