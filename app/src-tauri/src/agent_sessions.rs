//! **Reading back the handle a provider named itself** — the running half of
//! [`amenbo_core::agent_sessions`] (`AMB-D-869`).
//!
//! Three of the six are written down before they start: Amenbo decides the handle and hands it over
//! on the launch line ([`amenbo_core::harness::issue`]). OpenCode is the fourth — it names its own —
//! so the pane is started, and then its own list is asked which session appeared in that folder
//! (`AMB-T-4630`). The fifth is Codex, whose way back is a home rather than an id and is not this
//! module's (`AMB-T-4640`), and the sixth is Gemini, which has no way back to read (`AMB-T-4659`).
//!
//! **Asked in the reader's own login shell** ([`crate::launch::asking`]), for the reason every other
//! question about a provider is: the command has to be found the way the pane's shell finds it.
//!
//! **Which of the folder's sessions is the pane's is settled by when it appeared**, and by the
//! handles other panes have already been written down under
//! ([`amenbo_core::agent_sessions::newest_in`]). A folder somebody works in has a list of sessions
//! going back weeks, and the row that is this pane's is the one that was not there when it started.
//!
//! **Nothing here fails.** A provider that is not there, an answer in an unknown shape, a session
//! that never appears — every one of them is no handle written down, which is a pane that opens
//! normally now and comes back as a fresh session next run.

use std::io::Read as _;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use amenbo_core::agent_sessions::{self, Ask, Session};

/// How long a single ask is given before it is taken to have hung — a login shell plus a provider
/// starting up, the same floor [`crate::agent_models`] gives one.
const ASKING: Duration = Duration::from_secs(30);

/// How long the pane is watched for a session of its own to appear.
///
/// What is being waited for is the provider writing its first record, which is a program starting
/// up rather than a person saying anything — so the wait is wide enough for a slow machine and
/// short enough that a thread is not left on a pane that never made one. What running out costs is
/// the way back into that one pane, and nothing on the screen.
const APPEARING: Duration = Duration::from_secs(30);

/// How long between one look at the provider's list and the next, while the pane is starting.
const BETWEEN: Duration = Duration::from_millis(500);

/// The handle the session started in `folder` named itself, or `None` where none appeared before
/// the wait ran out.
///
/// `since` is when the pane was started and `taken` the handles other panes are already written
/// down under — the two together are what make one row of that folder's list this pane's
/// ([`amenbo_core::agent_sessions::newest_in`]).
pub fn appeared(
    command: &str,
    ask: &Ask,
    folder: &Path,
    since: i64,
    taken: &[String],
) -> Option<String> {
    let deadline = Instant::now() + APPEARING;
    loop {
        // Asked before the first sleep as well: a provider that had its record down before this
        // thread got here is one there is nothing to wait for.
        let sessions: Vec<Session> = agent_sessions::read(ask, &printed(command, ask));
        if let Some(found) = agent_sessions::newest_in(&sessions, folder, since, taken) {
            return Some(found.id.clone());
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(BETWEEN);
    }
}

/// What the provider printed when it was asked.
///
/// **stderr is dropped**, the same as when a provider is asked for its models: what it says there is
/// about the provider and never a handle.
fn printed(command: &str, ask: &Ask) -> String {
    let args: Vec<String> = ask.args.iter().map(|arg| (*arg).to_string()).collect();
    let mut cmd = crate::launch::asking(&crate::launch::command_line(command, &args));
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return String::new();
    };
    let read = child.stdout.take().map(|mut out| {
        let mut printed = String::new();
        let _ = out.read_to_string(&mut printed);
        printed
    });
    // The command prints and exits, so waiting on it is the whole of the reading. It is killed on
    // the way out all the same: a version that hung would otherwise be left behind for the life of
    // the app.
    let ended = wait_by(&mut child, Instant::now() + ASKING);
    if !ended {
        let _ = child.kill();
        let _ = child.wait();
        return String::new();
    }
    read.unwrap_or_default()
}

/// Whether the child ended before `deadline`.
fn wait_by(child: &mut std::process::Child, deadline: Instant) -> bool {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Err(_) => return false,
            Ok(None) => {}
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(BETWEEN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASK: Ask = Ask {
        args: &["session", "list", "--format", "json"],
        reading: amenbo_core::agent_sessions::Reading::JsonSessions,
    };

    /// A command that is not on this machine answers nothing, and does it without the ask failing —
    /// the shape every one of the failures takes.
    #[test]
    fn a_command_that_is_not_here_prints_nothing() {
        assert!(printed("amenbo-no-such-agent-command", &ASK).trim().is_empty());
        assert!(agent_sessions::read(&ASK, &printed("amenbo-no-such-agent-command", &ASK)).is_empty());
    }
}
