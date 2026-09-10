//! **Asking an agent's own command which models it can be started on** — the running half of
//! [`amenbo_core::agent_models`] (`AMB-D-865`).
//!
//! Core says how to ask and reads what came back; nothing that answers here is a name Amenbo holds.
//! The running is on this side for the reason the `PATH` probe is ([`crate::launch`]): the command
//! has to be found the way the pane's own shell would find it, which means the reader's login shell
//! with the reader's own profile, and a question resolved against this process's thin environment
//! would answer for a machine nobody is using.
//!
//! **Asked once, and then remembered for as long as Amenbo is running.** Every ask is a login shell
//! plus a provider starting up — a second or more each, and one of them is an ACP handshake — while
//! the answer is a fact about the machine and the reader's account, not about the pane being opened.
//! Opening panes is exactly what a person does over and over, so the answer is kept and the second
//! pane costs nothing.
//!
//! **An empty answer is kept too**, and that is a choice with a cost. A reader who signs into Cursor
//! while Amenbo is open goes on being offered no Cursor models until the next launch. The other way
//! round is worse: an unauthenticated provider answers instantly and emptily, so re-asking it would
//! put a shell startup in front of every pane, for ever, for the case that changes once. What is
//! never kept is a question that was not put — a provider with no ask ([`amenbo_core::harness::Launch::models`])
//! is answered without anything being run.
//!
//! **Nothing here fails.** Not signed in, not installed, a version that dropped the flag, an answer
//! in an unknown shape, a provider that never answers — every one of them is an empty list, because
//! the face's other road is always open: start the agent and use its own picker (`AMB-D-865`).

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use amenbo_core::agent_models::{self, Ask, Model};
use amenbo_core::harness::Launch;

use crate::dto::AgentModelDto;

/// How long a provider is given to answer before it is taken to have hung.
///
/// Wider than the `PATH` probe's, because this is a login shell **and** a program starting up on top
/// of it: the ACP handshake alone was 10 seconds on the machine it was measured on (`AMB-T-4581`).
/// What running out costs is a row of models that could have been offered; what it buys is that a
/// provider waiting on a network never holds a face open.
const ASKING: Duration = Duration::from_secs(30);

/// What each provider answered, for the life of the process — see the module docs on why an empty
/// answer is kept as an answer.
static ASKED: OnceLock<Mutex<HashMap<String, Vec<Model>>>> = OnceLock::new();

/// Which models an agent can be started on — the row a face draws before it opens a pane
/// (`app/src/shell/EmptySlot.tsx`).
///
/// `agent` is a catalogued provider's id ([`amenbo_core::harness::Launch::id`]). Anything else is an
/// empty list rather than a refusal: a command the reader registered themselves is a whole command
/// line of their own (`AMB-D-794`), and Amenbo has no idea which program is inside it or how that one
/// would be asked.
///
/// **Off the main thread.** A command with no `async` on it is run where the webview is drawn, and
/// what this one does is a login shell, a provider starting up on top of it, and [`ASKING`] behind
/// that — so the first press on a provider froze the whole window for as long as the answer took
/// (`AMB-T-4661`). Only the first press ever pays it, which is exactly the press a person meets.
///
/// An ask that did not finish is the empty row every other way of failing here is: what the face
/// does with no models is put up its own box, and that road is open whatever the reason.
#[tauri::command]
pub async fn agent_models(agent: String) -> Vec<AgentModelDto> {
    tauri::async_runtime::spawn_blocking(move || rows(&agent)).await.unwrap_or_default()
}

/// The row itself, on whichever thread asked for it — [`agent_models()`] without the door.
fn rows(agent: &str) -> Vec<AgentModelDto> {
    let Some(launch) = amenbo_core::harness::find_launch(agent) else {
        return Vec::new();
    };
    models(launch).into_iter().map(|one| AgentModelDto { id: one.id, label: one.label }).collect()
}

/// The models this provider answered with — kept from the first ask onward.
fn models(launch: &Launch) -> Vec<Model> {
    let Some(ask) = launch.models.as_ref() else {
        // Nothing to run, so nothing is remembered either: there was never a question to put.
        return Vec::new();
    };
    if let Some(kept) = ASKED.get_or_init(Mutex::default).lock().ok().and_then(|asked| asked.get(launch.id).cloned()) {
        return kept;
    }
    let found = agent_models::read(ask, &printed(launch.command, ask));
    if let Ok(mut asked) = ASKED.get_or_init(Mutex::default).lock() {
        asked.insert(launch.id.to_string(), found.clone());
    }
    found
}

/// What the provider printed when it was asked — empty for every way that can fail.
///
/// The shell is the pane's own ([`crate::launch::asking`]), and what it is handed is the provider's
/// command with the catalog's arguments quoted onto it, so a version of the question that reached a
/// shell as more command line is not possible.
///
/// **stderr is dropped.** A provider that will not answer says why there — Cursor names the four ways
/// to sign in — and none of it is a model. Whether to put any of that in front of the reader is a
/// question about a face, and this is not one.
fn printed(command: &str, ask: &Ask) -> String {
    let args: Vec<String> = ask.args.iter().map(|arg| (*arg).to_string()).collect();
    let mut cmd = crate::launch::asking(&crate::launch::command_line(command, &args));
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return String::new();
    };

    // Said before anything is read, and the pipe is held open afterwards: an ACP agent that is asked
    // in writing is also one that would take a closed stdin as the client hanging up.
    let mut stdin = child.stdin.take();
    if let Some(stdin) = stdin.as_mut() {
        for line in agent_models::said_to(ask, &here()) {
            if writeln!(stdin, "{line}").is_err() {
                break;
            }
        }
        let _ = stdin.flush();
    }

    let read = child.stdout.take().map(|out| {
        let (tx, rx) = mpsc::channel();
        // A line at a time rather than the whole of stdout, because one of the six never reaches an
        // end: an ACP agent holding a session waits to be worked with, so the answer has to be
        // recognised as it arrives — which is what `agent_models::answered` says.
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = Instant::now() + ASKING;
        let mut printed = String::new();
        while let Ok(line) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            printed.push_str(&line);
            printed.push('\n');
            if agent_models::answered(ask, &printed) {
                break;
            }
        }
        printed
    });

    // Whether it answered, ran past the deadline, or is an agent that was never going to end on its
    // own, it is reaped here rather than left behind for the length of the session.
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    read.unwrap_or_default()
}

/// The folder an ACP session is opened in. It is never worked in — the session is opened to be asked
/// one question and killed — but an ACP agent requires an absolute path, so it is given one that
/// exists: the reader's home, or wherever this process is standing.
fn here() -> PathBuf {
    amenbo_core::env::home_dir()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A provider with no way of being asked is answered without a shell being started — the one
    /// case where "no models" is known rather than found out.
    #[test]
    fn the_provider_with_nothing_to_ask_is_answered_without_running_anything() {
        let copilot = amenbo_core::harness::find_launch("github-copilot").expect("catalogued");
        assert!(copilot.models.is_none());
        assert!(models(copilot).is_empty());
    }

    /// An id that is not a catalogued provider is an empty row rather than a refusal: what the
    /// reader registered is a command line of their own, and Amenbo cannot ask it anything.
    #[test]
    fn an_id_the_catalog_does_not_list_is_an_empty_row() {
        assert!(rows("a-row-the-reader-wrote").is_empty());
        assert!(rows(amenbo_core::wake::SHELL).is_empty());
    }

    /// A command that is not on this machine answers nothing, and does it without the ask failing —
    /// the shape every one of the failures takes.
    #[test]
    fn a_command_that_is_not_here_prints_nothing() {
        let ask = Ask {
            args: &["--list-models"],
            reading: amenbo_core::agent_models::Reading::IdThenLabel,
        };
        assert!(printed("amenbo-no-such-agent-command", &ask).trim().is_empty());
    }
}
