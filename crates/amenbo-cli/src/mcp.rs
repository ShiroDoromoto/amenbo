//! `amenbo mcp` — the MCP server: JSON-RPC over stdin/stdout, for an AI whose host cannot open a
//! folder of its own (`AMB-D-665`).
//!
//! It is a mediator, not a second implementation. Every tool call re-runs this executable
//! (`current_exe`) as a child process with its working directory fixed to the folder that call named,
//! and hands the host what that run wrote. So the startup, the integrity check, the pointer repair and
//! the reach are the CLI's own, decided once and in one place — and a folder that is not bound, or a
//! store this build cannot read, is refused in the same words a person typing there would read.
//!
//! One server serves the folders `--dir` was given, and a call names one of them (`AMB-D-679`). The
//! set arrives at startup and nothing a caller sends can widen it: what an AI chooses is which of the
//! person's folders this call is for, never which folders there are. That is also what keeps
//! `--project`, a person's word and not an AI's, out of reach from this side — a folder is named, and
//! which project it is remains the `.amenbo` pointer's answer, there.
//!
//! Naming one is required even when the set holds a single folder (`AMB-D-679`, form 3): a default
//! would put the call somewhere the caller never said, and the caller is the one thing here that can
//! be wrong about where it meant.
//!
//! Three tools (`AMB-D-667`): `agent` and `agent_command`, which read the spec, and `run`, which
//! carries the caller's own words to any of amenbo's commands. Passing them through is what keeps
//! `amenbo agent` the single description of what can be typed — one command per typed tool would put
//! that description in two places and spend a host's tool budget on it. What passing through costs is
//! that everything is allowed unless it is named, and two things are named: the facet is the server's
//! to declare (`AMB-D-668`), and `bind` / `init` are refused, since either would let an AI re-point the
//! folder it was given and step outside it (`AMB-D-666`).

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde_json::{json, Value};

use amenbo_core::agent::VERSION;
use amenbo_core::config::Paths;

use crate::output::CliError;
use crate::{flag_before_the_name, FACET_FLAG};

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

/// The facet this server declares for every call it makes, whatever the caller wrote (`AMB-D-668`).
const OUR_FACET: &str = "ai";

/// The commands `run` will not carry, whatever the caller writes (`AMB-D-667`). Both place the
/// `.amenbo` pointer, and this server's whole shape is a set of folders named by the person who
/// configured it (`AMB-D-666`, `AMB-D-679`) — a pointer the AI may rewrite is that shape undone. There
/// is no flag to get past this: a person who wants either types it in the folder itself.
const REFUSED: &[&str] = &["bind", "init"];

/// The tool argument every call carries: which of this server's folders it is for.
const FOLDER_ARG: &str = "folder";

/// What one run of the child left behind: whether it succeeded, and the text to hand back.
pub struct Ran {
    pub ok: bool,
    pub text: String,
}

/// How a tool call reaches amenbo, in the folder it named. The server's own implementation re-runs
/// this executable; the tests put a recorder here, so what the argument shaping produces can be read
/// without forking anything.
pub trait CallAmenbo {
    fn call(&self, dir: &Path, args: &[String]) -> Ran;
}

/// The real caller: this executable again, in the folder the call named.
struct SelfCall {
    /// The whole set, as the child is told it — one folder per line, under
    /// [`amenbo_core::env::MCP_DIRS_VAR`]. A child cannot work the set out for itself (its argv is the
    /// caller's words and its CWD is one folder), and the one refusal that has to name it — no pointer
    /// here, and no way to place one from this side — is written down there, in amenbo's own words.
    serving: String,
}

impl CallAmenbo for SelfCall {
    fn call(&self, dir: &Path, args: &[String]) -> Ran {
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => return Ran { ok: false, text: format!("Cannot find the amenbo executable: {e}") },
        };
        // No shell in between (`AMB-D-667`): the executable is invoked directly, so nothing in an
        // argument is read as syntax by anybody. stdin is closed because this process's own stdin is
        // the protocol stream — a child inheriting it would eat the host's next request.
        let out = std::process::Command::new(exe)
            .args(args)
            .current_dir(dir)
            .env(amenbo_core::env::MCP_DIRS_VAR, &self.serving)
            .stdin(Stdio::null())
            .output();
        match out {
            Err(e) => Ran { ok: false, text: format!("Cannot run amenbo in {}: {e}", dir.display()) },
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

/// `amenbo mcp --dir <path> [<path> …]` — serve a set of folders over MCP until the host closes the
/// stream (`AMB-D-665`, `AMB-D-679`).
///
/// The one thing decided here is the set, and it is decided once. A path that names no directory is
/// dropped now, on the terminal, rather than becoming a spawn failure inside every call that names it
/// for the life of the server — and dropped rather than fatal, because a set comes out of a host's own
/// free-text field (`AMB-T-3156`), where one mistyped path would otherwise take down every folder that
/// was written correctly. Nothing being left is the one refusal: there is then nowhere to serve.
///
/// Whether a folder that *is* there is bound, whether this build can read its store, whether it is a
/// worktree nobody should work in — none of those are asked here. They are the child's to answer, in
/// the folder it runs in, so the host reads amenbo's own words instead of a second opinion this process
/// formed about a folder it never opened.
///
/// It writes nothing on stdout of its own: that stream is the protocol's.
pub(crate) fn mcp_cmd(dirs: &[String]) -> Result<i32, CliError> {
    let mut folders: Vec<PathBuf> = Vec::new();
    let mut gone: Vec<&str> = Vec::new();
    for dir in dirs {
        let path = PathBuf::from(dir);
        if !path.is_dir() {
            gone.push(dir);
        } else if !folders.contains(&path) {
            // The same folder written twice is one folder. It would otherwise be listed twice in
            // every tool's description, which reads as two roads where there is one.
            folders.push(path);
        }
    }
    if folders.is_empty() {
        return Err(CliError {
            code: "invalid_value",
            message: format!(
                "None of the folders named with --dir is a folder on this machine ({}), so there is nowhere to serve.",
                gone.join(", ")
            ),
            hint: Some(format!(
                "Name the folders the projects are worked in — the ones holding their `.amenbo` — in the host's own settings: `{} mcp --dir <path> [<path> …]`.",
                Paths::command_name()
            )),
            exit: 2,
        });
    }
    for missing in gone {
        eprintln!("amenbo mcp: --dir '{missing}' is not a folder on this machine, so it is not served.");
    }
    Ok(serve(&folders))
}

/// Serve the folders until the host closes the stream. The return value is this process's exit code.
///
/// Both streams belong to the protocol: nothing else may be written to stdout, and what this server
/// has to say for itself goes to stderr, where the host logs it.
pub fn serve(dirs: &[PathBuf]) -> i32 {
    let caller = SelfCall { serving: listing(dirs) };
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
        let Some(reply) = respond(&line, &caller, dirs) else { continue };
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
fn respond(line: &str, caller: &dyn CallAmenbo, dirs: &[PathBuf]) -> Option<Value> {
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
        "tools/list" => reply(id, json!({ "tools": tools(dirs) })),
        "tools/call" => match call(&params, caller, dirs) {
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
        "instructions": "Every call names the folder it is for — the folders this server works in are listed on each tool. Call `agent` first for one of them and follow what it says: it is how work is done in that folder, in full. `agent_command` pulls one command's flags, arguments and examples when you are about to use it, and `run` types it.",
    })
}

/// The tools this build serves. Descriptions are written for the model that reads the list, not for a
/// person: what the tool answers, and when to reach for it.
///
/// Each one carries the folders it may be called for (`AMB-D-679`, form 7). A host reads the listing
/// before the model calls anything, so putting the set here is what lets the very first call name a
/// folder correctly — and it is answered per request, so a folder that was set up after this server
/// started reads as set up the next time the list is pulled.
fn tools(dirs: &[PathBuf]) -> Value {
    let served = offered(dirs);
    let folder = json!({
        "type": "string",
        "description": "which of this server's folders to work in — one of the ones listed in this tool's description, written exactly as it is listed there",
    });
    json!([
        {
            "name": "agent",
            "description": format!("How to work in one folder, in full: the working practice to follow, the rules that bind it, and an index of every command by name. Call this first, before anything else, and follow what it says. It also reports what is installed there and whether this build is current.\n\n{served}"),
            "inputSchema": {
                "type": "object",
                "properties": { FOLDER_ARG: folder },
                "required": [FOLDER_ARG],
                "additionalProperties": false,
            },
        },
        {
            "name": "agent_command",
            "description": format!("One command's full spec — summary, arguments, flags, examples — pulled by the name the index in `agent` lists it under (compound names carry a space, as in `task add`). Read this before using a command instead of guessing a flag from its name.\n\n{served}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    FOLDER_ARG: folder,
                    "command": {
                        "type": "string",
                        "description": "the command's name as the agent index lists it, for example `task add` or `decision show`",
                    },
                },
                "required": [FOLDER_ARG, "command"],
                "additionalProperties": false,
            },
        },
        {
            "name": "run",
            "description": format!("Run an amenbo command in one folder and hand back exactly what it wrote. The words are the ones you would type after `amenbo`, one per array element — pull the command's spec with `agent_command` first rather than guessing a flag. Add `--json` yourself where you want machine-readable output. Do not pass `--actor`: this server declares the facet, and one you write is dropped. `bind` and `init` are refused here — ask the person to run either in the folder itself.\n\n{served}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    FOLDER_ARG: folder,
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "the command and its flags, one word per element, as they would be typed after `amenbo` — for example [\"task\", \"list\", \"--filter\", \"status:todo\", \"--json\"]. Empty runs amenbo with no arguments, which is today's work",
                    },
                },
                "required": [FOLDER_ARG, "args"],
                "additionalProperties": false,
            },
        },
    ])
}

/// The folders this server was given, as one line each — what a caller copies a name out of, and what
/// the child is handed so a refusal can name the set it could have reached instead.
fn listing(dirs: &[PathBuf]) -> String {
    dirs.iter().map(|dir| dir.display().to_string()).collect::<Vec<_>>().join("\n")
}

/// The same set, written for the model that has to choose from it — every folder, and which of them
/// have a project set up in them at this moment.
///
/// The unset-up ones are named rather than dropped. Dropping them would make a folder the person
/// deliberately chose read as one this server was never given, and the answer to naming it would be
/// "out of reach" — which is not what is wrong with it. What is wrong is that nobody has linked it
/// yet, and that is a sentence the caller can hand to the person who can.
fn offered(dirs: &[PathBuf]) -> String {
    let lines = dirs
        .iter()
        .map(|dir| match amenbo_core::binding::find_upward(dir) {
            Some(_) => format!("\n  • {}", dir.display()),
            None => format!(
                "\n  • {} — no project is set up here yet; a call naming it is answered with that, and only a person can link it",
                dir.display()
            ),
        })
        .collect::<String>();
    format!("Folders this server works in — every call names exactly one of them, written as it is here:{lines}")
}

/// What a tool call comes to: a command line to run, or a refusal to hand back without running one.
enum Shaped {
    Run(Vec<String>),
    Refused(String),
}

/// Run one tool call. `Err` is a protocol fault — a tool nobody serves, an argument that is not there —
/// which is the caller's mistake to correct. Everything the model is meant to *read* comes back as `Ok`
/// with `isError`: amenbo's own refusal, and this server's, which the model has to see in order to hand
/// it to the person instead of trying again.
fn call(params: &Value, caller: &dyn CallAmenbo, dirs: &[PathBuf]) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| (INVALID_PARAMS, "A tool call must name a tool.".to_string()))?;
    let args = params.get("arguments").unwrap_or(&Value::Null);
    // The tool is read first, so a name nobody serves is answered as that rather than as a folder
    // missing from a call that was never going to run.
    let shaped = argv_for(name, args)?;
    let ran = match where_it_runs(name, args, dirs)? {
        // A call with nowhere to run is answered before what it would have run is: the words it
        // carried never got a folder to be typed in, whatever else was wrong with them.
        Where::OutOfReach(why) => Ran { ok: false, text: why },
        Where::In(dir) => match shaped {
            Shaped::Run(argv) => caller.call(dir, &argv),
            Shaped::Refused(why) => Ran { ok: false, text: why },
        },
    };
    Ok(json!({
        "content": [{ "type": "text", "text": ran.text }],
        "isError": !ran.ok,
    }))
}

/// Where one tool call runs: the folder it named, or the refusal for a folder this server was not
/// given.
enum Where<'a> {
    In(&'a Path),
    OutOfReach(String),
}

/// Read the folder a call named and find it in the set (`AMB-D-679`, forms 2–5).
///
/// `Err` is the caller's protocol mistake — no folder named at all, which no default fills in.
/// Naming one that is not in the set is not a mistake of that kind: it is a real answer the model has
/// to read, because what it says is that the person chose a different set, and the way back is in the
/// answer itself.
fn where_it_runs<'a>(name: &str, args: &Value, dirs: &'a [PathBuf]) -> Result<Where<'a>, (i64, String)> {
    let named = args
        .get(FOLDER_ARG)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|folder| !folder.is_empty())
        .ok_or_else(|| {
            (
                INVALID_PARAMS,
                format!("{name} takes `{FOLDER_ARG}`: which folder to work in. {}", offered(dirs)),
            )
        })?;
    Ok(match found(named, dirs) {
        Some(dir) => Where::In(dir),
        None => Where::OutOfReach(format!(
            "`{named}` is out_of_reach: this server was not given that folder, and the set is the person's to choose — nothing sent over this connection widens it.\n\n{}",
            offered(dirs)
        )),
    })
}

/// The folder in the set a caller's name stands for, if any.
///
/// The name is matched as written first, so one copied out of a tool's description always lands.
/// Failing that, the filesystem is asked what the two paths are: a trailing slash, a `.` in the
/// middle, or a symlinked route to the same folder is the same folder, and a caller that reached the
/// right place by another spelling should not be turned away from it.
fn found<'a>(named: &str, dirs: &'a [PathBuf]) -> Option<&'a Path> {
    let asked = Path::new(named);
    if let Some(hit) = dirs.iter().find(|dir| dir.as_path() == asked) {
        return Some(hit);
    }
    let real = amenbo_core::binding::canonical_dir(asked).ok()?;
    dirs.iter()
        .find(|dir| amenbo_core::binding::canonical_dir(dir).is_ok_and(|its| its == real))
        .map(PathBuf::as_path)
}

/// What one tool call becomes on the command line. The two reading tools name every word themselves, so
/// nothing a caller wrote reaches the child; `run` is the one that carries the caller's own words, and
/// what it does to them is [`shape`].
fn argv_for(name: &str, args: &Value) -> Result<Shaped, (i64, String)> {
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
            vec!["agent", "--command", command, "--json"]
        }
        "run" => {
            let words = args
                .get("args")
                .and_then(Value::as_array)
                .ok_or_else(|| (INVALID_PARAMS, "run takes `args`: the words to type after amenbo, one per element.".to_string()))?
                .iter()
                .map(|w| w.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()
                .ok_or_else(|| (INVALID_PARAMS, "run takes `args` as strings — one word per element, not a single line.".to_string()))?;
            return Ok(shape(&words));
        }
        other => return Err((INVALID_PARAMS, format!("No tool named '{other}'."))),
    };
    Ok(Shaped::Run(line.iter().map(|a| (*a).to_string()).collect()))
}

/// Turn the caller's own words into the command line the child is run with (`AMB-D-667`).
///
/// Two things happen to them and nothing else does. The facet the caller wrote is dropped and this
/// server's is put at the front (`AMB-D-668`) — a caller that could write `--actor human` would have its
/// writes recorded as the person's and its reach widened past the bound project, so the value is never
/// the caller's to choose. And `bind` / `init` are refused outright.
///
/// Where the caller's words stop being amenbo's is [`read_line`]'s answer: after `plugin run <name>`
/// every word is the plugin's (`AMB-D-346`), and a `--actor` standing there is a word on the plugin's
/// own face, not a facet for amenbo. Taking it away would quietly break a call amenbo never read.
fn shape(words: &[String]) -> Shaped {
    let line = read_line(words);
    if let Some(refused) = line.subcommand.as_deref().filter(|s| REFUSED.contains(s)) {
        return Shaped::Refused(format!(
            "`{refused}` is not served over MCP: it writes the pointer that says which project a folder is, and the folders this server works in are the person's to choose. Ask the person to run `{} {refused} …` in the folder itself, or to add the folder to a project in amenbo's own window.",
            Paths::command_name()
        ));
    }
    let mut argv = vec![FACET_FLAG.to_string(), OUR_FACET.to_string()];
    argv.extend(without_the_callers_facet(words, line.plugin_tail));
    Shaped::Run(argv)
}

/// How the caller's line reads to the parser: the subcommand it names, and where its words stop being
/// amenbo's own.
struct Line {
    /// The first word that is not one of amenbo's flags — `None` for a line that is all flags, which is
    /// amenbo with no command (today's work).
    subcommand: Option<String>,
    /// Everything from here on belongs to a plugin; `words.len()` when nothing does.
    plugin_tail: usize,
}

/// Walk the caller's words the way the parser will: over amenbo's own flags, to the subcommand, and — on
/// the one path that leads there — past `plugin run` to the plugin's name.
fn read_line(words: &[String]) -> Line {
    let mut i = 0;
    let mut subcommand = None;
    let mut path = ["plugin", "run"].iter();
    let mut step = path.next();
    while let Some(word) = words.get(i) {
        if let Some(takes_value) = flag_before_the_name(word) {
            i += if takes_value { 2 } else { 1 };
            continue;
        }
        if subcommand.is_none() {
            subcommand = Some(word.clone());
        }
        match step {
            Some(expected) if word == *expected => {
                i += 1;
                step = path.next();
            }
            // Any other word means this line goes somewhere else entirely, and all of it is amenbo's.
            _ => break,
        }
        if step.is_none() {
            // `plugin run` is complete. Amenbo's own flags may still stand ahead of the name, and the
            // name itself is amenbo's to read; everything after it is the plugin's.
            while let Some(takes_value) = words.get(i).and_then(|w| flag_before_the_name(w)) {
                i += if takes_value { 2 } else { 1 };
            }
            return Line { subcommand, plugin_tail: words.len().min(i + 1) };
        }
    }
    Line { subcommand, plugin_tail: words.len() }
}

/// The caller's words with every facet of theirs taken out of amenbo's own half of the line.
fn without_the_callers_facet(words: &[String], plugin_tail: usize) -> Vec<String> {
    let mut kept = Vec::with_capacity(words.len());
    let mut i = 0;
    while let Some(word) = words.get(i) {
        if i >= plugin_tail {
            kept.push(word.clone());
            i += 1;
            continue;
        }
        let head = word.split_once('=').map_or(word.as_str(), |(k, _)| k);
        if head != FACET_FLAG {
            kept.push(word.clone());
            i += 1;
            continue;
        }
        i += 1;
        // `--actor ai` is two words unless it was written as one. A flag standing where the value should
        // be is somebody else's to report, so it is left where it is rather than eaten.
        if !word.contains('=') && words.get(i).is_some_and(|v| !v.starts_with('-')) {
            i += 1;
        }
    }
    kept
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

    /// The set a test server stands for. These are names, not places: what a call does with one is
    /// decided by the set it was given, and nothing here opens a folder.
    const SHOP: &str = "/work/shop";
    const GREENHOUSE: &str = "/work/greenhouse";

    fn dirs() -> Vec<PathBuf> {
        vec![PathBuf::from(SHOP), PathBuf::from(GREENHOUSE)]
    }

    /// A caller that records what it was asked to run, and where, and answers with a fixed reply — the
    /// shaping is the subject here, and forking the binary would say nothing more about it.
    struct Recorder {
        seen: RefCell<Vec<(PathBuf, Vec<String>)>>,
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
            self.seen.borrow().last().cloned().expect("nothing was run").1
        }

        fn last_dir(&self) -> PathBuf {
            self.seen.borrow().last().cloned().expect("nothing was run").0
        }
    }

    impl CallAmenbo for Recorder {
        fn call(&self, dir: &Path, args: &[String]) -> Ran {
            self.seen.borrow_mut().push((dir.to_path_buf(), args.to_vec()));
            Ran { ok: self.ok, text: "what amenbo wrote".to_string() }
        }
    }

    fn ask(line: &str, caller: &dyn CallAmenbo) -> Value {
        respond(line, caller, &dirs()).expect("a request is always answered")
    }

    /// One tool call, for one of the server's folders.
    fn call_for(tool: &str, folder: &str, arguments: Value) -> String {
        let mut arguments = arguments;
        arguments[FOLDER_ARG] = json!(folder);
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": tool, "arguments": arguments },
        })
        .to_string()
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

    /// The words `run` was handed, as the child would have been given them.
    fn ran(args: &[&str]) -> Vec<String> {
        let caller = Recorder::new();
        let out = ask(&call_for("run", SHOP, json!({ "args": args })), &caller);
        assert_eq!(out["result"]["isError"], false, "{out}");
        caller.last()
    }

    #[test]
    fn the_listing_carries_the_three_tools_and_the_folders_each_may_be_called_for() {
        let caller = Recorder::new();
        let out = ask(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &caller);
        let tools = out["result"]["tools"].as_array().expect("tools is an array");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, vec!["agent", "agent_command", "run"], "the three the decision names");
        for tool in tools {
            let described = tool["description"].as_str().unwrap_or_default();
            assert!(!described.is_empty(), "a tool nobody described is one the model cannot pick: {tool}");
            assert_eq!(tool["inputSchema"]["type"], "object", "every tool declares an object schema: {tool}");
            // The set is on every tool, because a host reads the listing before the model calls
            // anything — this is where the first call learns what it may name (`AMB-D-679`, form 7).
            for dir in [SHOP, GREENHOUSE] {
                assert!(described.contains(dir), "`{dir}` is missing from {}: {described}", tool["name"]);
            }
            let required = tool["inputSchema"]["required"].as_array().expect("what it requires");
            assert!(
                required.iter().any(|arg| arg == FOLDER_ARG),
                "the folder is required, whatever else is: {tool}",
            );
        }
    }

    #[test]
    fn agent_runs_the_entry_point_in_the_folder_the_call_named() {
        let caller = Recorder::new();
        let out = ask(&call_for("agent", GREENHOUSE, json!({})), &caller);
        assert_eq!(caller.last(), vec!["agent", "--json"]);
        assert_eq!(caller.last_dir(), PathBuf::from(GREENHOUSE), "the call runs where it said");
        assert_eq!(out["result"]["content"][0]["text"], "what amenbo wrote");
        assert_eq!(out["result"]["isError"], false);
    }

    /// Every tool takes the folder, and none of them fills it in (`AMB-D-679`, form 3). A default
    /// would put the call somewhere the caller never said — which is the one thing an AI is able to be
    /// wrong about here.
    #[test]
    fn a_call_that_names_no_folder_runs_nothing_and_is_told_the_set() {
        for (tool, arguments) in [
            ("agent", json!({})),
            ("agent_command", json!({ "command": "task add" })),
            ("run", json!({ "args": ["status"] })),
        ] {
            let caller = Recorder::new();
            let call = json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": tool, "arguments": arguments },
            });
            let out = ask(&call.to_string(), &caller);
            assert_eq!(out["error"]["code"], INVALID_PARAMS, "{tool}: {out}");
            let said = out["error"]["message"].as_str().unwrap_or_default();
            assert!(said.contains(SHOP) && said.contains(GREENHOUSE), "{tool} says what it could be given: {said}");
            assert!(caller.seen.borrow().is_empty(), "{tool} ran with nowhere to run it");
        }
    }

    /// A folder the person did not choose is out of reach, and the answer carries the way back: the
    /// set itself, which is what the caller names in its next call (`AMB-D-679`, form 5).
    #[test]
    fn a_folder_outside_the_set_is_out_of_reach_and_the_answer_says_what_is_in_it() {
        let caller = Recorder::new();
        let out = ask(&call_for("run", "/work/somewhere-else", json!({ "args": ["status"] })), &caller);
        assert!(out.get("error").is_none(), "the refusal is the tool's answer, to be read: {out}");
        assert_eq!(out["result"]["isError"], true, "{out}");
        let said = out["result"]["content"][0]["text"].as_str().unwrap_or_default();
        assert!(said.contains("out_of_reach"), "the refusal names itself: {said}");
        assert!(said.contains(SHOP) && said.contains(GREENHOUSE), "and the set it could have named: {said}");
        assert!(caller.seen.borrow().is_empty(), "nothing runs outside the set");
    }

    /// The same folder written another way is the same folder: what a caller reached by a longer route
    /// is the one the person chose, and turning it away would be this server disagreeing with the
    /// filesystem about where a path leads.
    #[test]
    fn a_folder_spelled_another_way_is_still_the_one_in_the_set() {
        let here = std::env::temp_dir();
        let set = vec![here.clone()];
        let roundabout = here.join(".");
        assert_eq!(
            found(&roundabout.to_string_lossy(), &set).map(Path::to_path_buf),
            Some(here.clone()),
            "a path that leads to the folder is the folder",
        );
        assert!(found("/there/is/no/such/folder", &set).is_none(), "and one that leads elsewhere is not");
    }

    /// What the child is handed: the set, one folder per line. The refusal that has to name it is
    /// written where the child stands, so this is the only way it can know what else was on offer.
    #[test]
    fn the_child_is_told_the_whole_set_one_folder_to_a_line() {
        assert_eq!(listing(&dirs()), format!("{SHOP}\n{GREENHOUSE}"));
        assert_eq!(listing(&[]), "", "a set of nothing is never served, and reads as nothing");
    }

    #[test]
    fn agent_command_carries_the_name_it_was_given() {
        let caller = Recorder::new();
        ask(&call_for("agent_command", SHOP, json!({ "command": "task add" })), &caller);
        assert_eq!(caller.last(), vec!["agent", "--command", "task add", "--json"]);
    }

    #[test]
    fn agent_command_without_a_name_is_the_caller_s_mistake_and_runs_nothing() {
        let caller = Recorder::new();
        let out = ask(&call_for("agent_command", SHOP, json!({ "command": "  " })), &caller);
        assert_eq!(out["error"]["code"], INVALID_PARAMS);
        assert!(caller.seen.borrow().is_empty(), "nothing is run for a call that cannot be shaped");
    }

    #[test]
    fn run_carries_the_caller_s_own_words_and_names_the_facet_itself() {
        assert_eq!(
            ran(&["task", "list", "--filter", "status:todo", "--json"]),
            vec!["--actor", "ai", "task", "list", "--filter", "status:todo", "--json"],
        );
    }

    /// The whole of the facet rule: whatever the caller wrote is dropped, and this server's stands in
    /// its place. A caller that could write `human` would have its writes recorded as the person's.
    #[test]
    fn a_facet_the_caller_wrote_never_survives() {
        for written in [
            vec!["--actor", "human", "task", "add", "--title", "x"],
            vec!["task", "add", "--title", "x", "--actor", "human"],
            vec!["--actor=human", "task", "add", "--title", "x"],
            vec!["--actor", "ai", "task", "add", "--title", "x"],
        ] {
            let argv = ran(&written);
            assert_eq!(&argv[..2], ["--actor", "ai"], "the facet is named once, at the front: {argv:?}");
            assert_eq!(
                argv.iter().filter(|w| w.starts_with(FACET_FLAG)).count(),
                1,
                "no facet of the caller's is left anywhere: {argv:?}",
            );
            assert!(argv.contains(&"--title".to_string()), "the rest of the line is untouched: {argv:?}");
        }
    }

    /// A `--actor` with a flag where its value should be is somebody else's to report — eating the next
    /// word would turn a bad line into a different command.
    #[test]
    fn a_facet_with_no_value_does_not_swallow_what_follows() {
        assert_eq!(ran(&["--actor", "--json", "status"]), vec!["--actor", "ai", "--json", "status"]);
    }

    /// After `plugin run <name>` every word is the plugin's (`AMB-D-346`) — including one spelled like
    /// amenbo's facet, which amenbo never reads there.
    #[test]
    fn a_word_of_the_plugin_s_own_is_left_alone() {
        assert_eq!(
            ran(&["plugin", "run", "worktree", "start", "3127", "--actor", "human"]),
            vec!["--actor", "ai", "plugin", "run", "worktree", "start", "3127", "--actor", "human"],
        );
        // Amenbo's own flags may stand ahead of the name, and one there is still amenbo's.
        assert_eq!(
            ran(&["plugin", "run", "--json", "worktree", "--actor", "human"]),
            vec!["--actor", "ai", "plugin", "run", "--json", "worktree", "--actor", "human"],
        );
        // The path has to be complete: `plugin list` hands nothing to anybody.
        assert_eq!(
            ran(&["plugin", "list", "--actor", "human"]),
            vec!["--actor", "ai", "plugin", "list"],
        );
    }

    #[test]
    fn run_with_no_words_is_amenbo_with_no_command() {
        assert_eq!(ran(&[]), vec!["--actor", "ai"]);
    }

    /// Neither may be reached from here: both write the pointer that says which project this folder is,
    /// and the server was given one folder to work in.
    #[test]
    fn bind_and_init_are_refused_and_nothing_is_run() {
        for line in [
            vec!["bind", "--project", "somewhere-else"],
            vec!["init", "--name", "mine"],
            vec!["--json", "bind", "--project", "somewhere-else"],
        ] {
            let caller = Recorder::new();
            let out = ask(&call_for("run", SHOP, json!({ "args": line })), &caller);
            assert!(out.get("error").is_none(), "the refusal is the tool's answer, to be read: {out}");
            assert_eq!(out["result"]["isError"], true, "{out}");
            assert!(
                out["result"]["content"][0]["text"].as_str().is_some_and(|t| t.contains(line[0]) || t.contains("bind")),
                "the refusal names what was refused: {out}",
            );
            assert!(caller.seen.borrow().is_empty(), "nothing is run for a refused line");
        }
    }

    /// A word that merely reads like one of the two is not one: only the command position is.
    #[test]
    fn a_refused_name_written_somewhere_else_is_just_a_word() {
        assert_eq!(
            ran(&["task", "add", "--title", "bind"]),
            vec!["--actor", "ai", "task", "add", "--title", "bind"],
        );
    }

    #[test]
    fn run_without_words_to_run_is_the_caller_s_mistake() {
        let caller = Recorder::new();
        let out = ask(&call_for("run", SHOP, json!({})), &caller);
        assert_eq!(out["error"]["code"], INVALID_PARAMS);
        // One line where a list belongs is the mistake worth naming: it would otherwise be run as a
        // single word nobody answers to.
        let out = ask(&call_for("run", SHOP, json!({ "args": "task list" })), &caller);
        assert_eq!(out["error"]["code"], INVALID_PARAMS);
        let out = ask(&call_for("run", SHOP, json!({ "args": ["task", 2] })), &caller);
        assert_eq!(out["error"]["code"], INVALID_PARAMS);
        assert!(caller.seen.borrow().is_empty(), "nothing is run for a call that cannot be shaped");
    }

    /// `--yes` is never added (`AMB-D-667`): a destructive command asked for over MCP stops at amenbo's
    /// own confirmation, which is what makes it the person's to give.
    #[test]
    fn nothing_is_added_beyond_the_facet() {
        let argv = ran(&["project", "delete", "AMB-P-1"]);
        assert_eq!(argv, vec!["--actor", "ai", "project", "delete", "AMB-P-1"]);
    }

    #[test]
    fn a_tool_nobody_serves_is_refused_by_name() {
        let caller = Recorder::new();
        let out = ask(r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"typo","arguments":{}}}"#, &caller);
        assert_eq!(out["error"]["code"], INVALID_PARAMS);
        assert!(out["error"]["message"].as_str().is_some_and(|m| m.contains("typo")), "the refusal names it");
        assert!(caller.seen.borrow().is_empty());
    }

    /// A refusal amenbo wrote reaches the model as the tool's own answer, not as a transport fault —
    /// otherwise the host reports "the tool broke" and what amenbo actually said is lost.
    #[test]
    fn a_refusal_comes_back_as_the_tool_s_result() {
        let caller = Recorder::failing();
        let out = ask(&call_for("agent", SHOP, json!({})), &caller);
        assert!(out.get("error").is_none(), "a tool that ran is not a protocol fault: {out}");
        assert_eq!(out["result"]["isError"], true);
        assert_eq!(out["result"]["content"][0]["text"], "what amenbo wrote");
    }

    #[test]
    fn a_notification_is_never_answered() {
        let caller = Recorder::new();
        assert!(respond(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &caller, &dirs()).is_none());
        assert!(respond(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{}}"#, &caller, &dirs()).is_none());
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
        assert!(respond(r#"{"jsonrpc":"2.0"}"#, &caller, &dirs()).is_none());
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
