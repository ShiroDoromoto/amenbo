//! **Which AI a folder is opened with**, out of the ones this machine can actually start.
//!
//! A pane is a real terminal (`AMB-D-747`), so opening one means starting a program — and the
//! program is whichever agent the person works with *here*. Two things answer that, and neither
//! answers it alone:
//!
//! | | reads | what it says |
//! |---|---|---|
//! | the folder's trace | [`crate::harness::probe`] | which provider is worked with in this folder |
//! | this machine | the caller's `PATH` probe | which provider can be started at all |
//!
//! **The product of the two is the answer, and the second is the floor.** A trace is a folder's
//! preference, so a provider that left one and is not installed is a preference this machine cannot
//! act on — starting it would open a pane on `command not found`. An installed provider that left no
//! trace is the opposite: startable, and merely unspoken for. So the two ranks below are the product
//! first and the floor second, and what is never offered is a provider that is not installed.
//!
//! **What is said about a provider stops at "installed".** Whether the person is signed in, whether
//! their subscription is current, whether the version still speaks the flags — none of it is knowable
//! before the program runs, and a face that guessed would be telling the reader something it made up
//! (`AMB-T-3591`). The vocabulary here is **installed / not installed**, and nothing wider.
//!
//! **Three things are looked at in turn, and the first that holds is the answer** ([`settle`]):
//!
//! | | kept against | written by |
//! |---|---|---|
//! | 1 | the project ([`crate::config::Config::agent_for`]) | the project's own settings, and nothing else |
//! | 2 | the person ([`crate::config::Config::last_agent`]) | every press that opens a pane with one |
//! | 3 | — | nothing: this is the run before anybody has chosen |
//!
//! **The project is above the person because it is the pinned answer.** Somebody who works with one
//! agent everywhere never touches it, and rank 2 carries their choice from the first project into
//! every one after it; somebody who works differently in one repository pins it there, and the pin
//! outranks the habit. Neither is kept per folder: one project can bind several, which would give it
//! as many answers as it has folders.
//!
//! **Rank 3 is the first run, and it is drawn as "nothing is chosen" rather than guessed at.** The
//! one exception is a machine with a single startable agent, where there is no question to put.
//!
//! A kept answer that stops being startable — the tool was removed — is not an error and not a
//! question either: it simply stops being the answer, and the rank underneath takes over.
//!
//! **The trace still reads per folder**, because that is where a provider leaves one. What a project
//! traces is what any of its folders traces: a preference shown in one of them is the project's, and
//! a face gathers the folders before it asks (`app/src-tauri/src/wake.rs`).

use crate::harness::{self, Harness, Wiring};

/// **The pane's own shell** — a prompt in the folder with nothing started at it, standing among the
/// agents as one more thing a person opens with (`app/src/talk/terminal.ts`).
///
/// It has no row in [`harness::HARNESSES`] and never will: it is the *absence* of an agent, and what
/// starts it is the folder's own login shell. It is here because it is a value the person's own
/// answer can hold ([`crate::config::Config::last_agent`]) — somebody who opened a plain prompt last
/// time gets one again — while the project's answer never holds it: "which agent do you work with
/// here" is not a question a shell answers.
pub const SHELL: &str = "shell";

/// One provider as a place to open a pane on: what the catalog says about it, and what this folder
/// and this machine say about it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Candidate {
    /// The harness this answers for ([`Harness::id`]).
    pub id: &'static str,
    /// The provider's own name for itself, so a face can render a row without a second lookup.
    pub label: &'static str,
    /// What it is started as ([`Harness::command`]) — shown, because a reader told a tool is missing
    /// needs the word to type to install it.
    pub command: &'static str,
    /// Whether this folder shows a trace of the provider being used here ([`Wiring::traced`]).
    pub traced: bool,
    /// Whether the command is on the `PATH` of the shell a pane starts with. Only ever this — see
    /// the module docs on what it deliberately does not claim.
    pub installed: bool,
}

/// What to do with the folder's answer once it has been worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// One agent, and nothing to ask: the [`Harness::id`] to start.
    Settled(&'static str),
    /// Several, so the person picks — the ids to offer, in catalog order.
    Ask(Vec<&'static str>),
    /// Nothing on this machine can be started. The face has an install notice to put, not a question.
    Nothing,
}

/// Every provider as a candidate, in [`harness::HARNESSES`] order.
///
/// `installed` is asked of the caller rather than performed here, because what counts as installed is
/// what the *pane's* shell can find: the same login-and-interactive shell, with the same profile and
/// the same `PATH` (`app/src-tauri/src/launch.rs`). A probe run against this process's environment
/// would answer for a desktop launch's thin `PATH` and be wrong in both directions (`AMB-T-3546`).
pub fn candidates(found: &[Wiring], installed: impl Fn(&str) -> bool) -> Vec<Candidate> {
    harness::HARNESSES
        .iter()
        .map(|h| Candidate {
            id: h.id,
            label: h.label,
            command: h.command,
            traced: found
                .iter()
                .find(|one| one.id == h.id)
                .is_some_and(|one| one.traced),
            installed: installed(h.command),
        })
        .collect()
}

/// The candidates a face offers, in catalog order: the ones the folders trace **and** this machine
/// has, or — when the product is empty — every one this machine has.
///
/// The fallback is not a weakening of the rule. Folders with no trace have said nothing, and the
/// question then is only which of the installed agents to open; treating that as "none" would put an
/// install notice in front of a reader whose tools are all installed.
pub fn offered(candidates: &[Candidate]) -> Vec<&Candidate> {
    let preferred: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.traced && c.installed)
        .collect();
    if !preferred.is_empty() {
        return preferred;
    }
    candidates.iter().filter(|c| c.installed).collect()
}

/// The answer: the project's if it still holds, else the person's, else what [`offered`] leaves.
///
/// `remembered` is what this project pinned ([`crate::config::Config::agent_for`]) and `last` is
/// what this person last opened with ([`crate::config::Config::last_agent`]) — the two ranks the
/// module docs set out, in that order.
///
/// **Either is honoured only while it is still startable**, and only while it is still one of the
/// offered: a project whose folders have since grown a trace for the agent actually used in it
/// should not be held to an answer given before they did. [`SHELL`] is the one value that always
/// holds, since a folder always has a prompt — but only where there is something to open at all,
/// because a machine with nothing startable has one terminal and no choice to make about it.
///
/// [`Choice::Ask`] is the run before anybody has chosen, and it is put to the person as a row with
/// **nothing on it** rather than as a guess.
pub fn settle(remembered: Option<&str>, last: Option<&str>, candidates: &[Candidate]) -> Choice {
    let offered = offered(candidates);
    if offered.is_empty() {
        return Choice::Nothing;
    }
    let holds = |kept: &str| offered.iter().find(|c| c.id == kept).map(|one| one.id);
    if let Some(id) = remembered.and_then(holds) {
        return Choice::Settled(id);
    }
    if let Some(kept) = last {
        if kept == SHELL {
            return Choice::Settled(SHELL);
        }
        if let Some(id) = holds(kept) {
            return Choice::Settled(id);
        }
    }
    match offered.as_slice() {
        // One thing to open with is not a question: asking about a row with a single answer on it
        // would be asking the person to agree with the machine.
        [one] => Choice::Settled(one.id),
        several => Choice::Ask(several.iter().map(|c| c.id).collect()),
    }
}

/// The catalog row an id names, for a caller holding a [`Choice::Settled`].
pub fn started_as(id: &str) -> Option<&'static Harness> {
    harness::find(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Candidates built from a trace list and a set of installed commands, for asserting on.
    fn built(traced: &[&str], installed: &[&str]) -> Vec<Candidate> {
        let found: Vec<Wiring> = harness::HARNESSES
            .iter()
            .map(|h| Wiring {
                id: h.id,
                label: h.label,
                wired_at: None,
                traced: traced.contains(&h.id),
            })
            .collect();
        candidates(&found, |cmd| installed.contains(&cmd))
    }

    /// The product decides while it has anything in it: an agent this machine has but this folder
    /// has never been opened with does not compete with the one the folder traces.
    #[test]
    fn a_folder_that_traces_one_installed_agent_is_not_asked_about() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "gemini"]);
        assert_eq!(settle(None, None, &c), Choice::Settled("claude-code"));
    }

    /// Several traced and installed is the one case a person is asked, and the offer is those —
    /// not the whole catalog, and not the machine's other tools.
    #[test]
    fn several_in_the_product_are_offered_and_nothing_else_is() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "codex", "gemini"]);
        assert_eq!(
            settle(None, None, &c),
            Choice::Ask(vec!["claude-code", "codex-cli"])
        );
    }

    /// A folder that traces nothing has said nothing, so what is asked about is what the machine
    /// has. An install notice here would be shown to a reader whose tools are all installed.
    #[test]
    fn a_folder_with_no_trace_falls_back_to_what_the_machine_has() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(settle(None, None, &c), Choice::Ask(vec!["claude-code", "codex-cli"]));
        assert_eq!(settle(None, None, &built(&[], &["gemini"])), Choice::Settled("gemini-cli"));
    }

    /// A trace this machine cannot act on is not an answer. Starting it would open the pane on
    /// `command not found`, which is the one outcome the multiplication exists to rule out.
    #[test]
    fn a_traced_agent_that_is_not_installed_is_never_offered() {
        let c = built(&["claude-code"], &["codex"]);
        assert!(!offered(&c).iter().any(|one| one.id == "claude-code"));
        assert_eq!(settle(None, None, &c), Choice::Settled("codex-cli"));
    }

    /// Nothing installed is the install notice's case, and the only one.
    #[test]
    fn nothing_installed_is_nothing_to_open_with() {
        assert_eq!(settle(None, None, &built(&["claude-code"], &[])), Choice::Nothing);
        assert_eq!(settle(Some("claude-code"), None, &built(&["claude-code"], &[])), Choice::Nothing);
        // Not even the shell, which is the whole of what such a machine has: what is put there is
        // the install notice, and the shell stands under it (`app/src/talk/agent.ts`).
        assert_eq!(settle(None, Some(SHELL), &built(&["claude-code"], &[])), Choice::Nothing);
    }

    /// What was remembered wins while it still holds — that is the whole of "asked once".
    #[test]
    fn the_remembered_answer_is_the_answer() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "codex"]);
        assert_eq!(settle(Some("codex-cli"), None, &c), Choice::Settled("codex-cli"));
    }

    /// A remembered answer whose tool has gone is not an error and not a question about itself: the
    /// rank underneath simply answers instead.
    #[test]
    fn a_remembered_answer_that_stopped_being_startable_gives_way() {
        let c = built(&["claude-code", "codex-cli"], &["codex"]);
        assert_eq!(settle(Some("claude-code"), None, &c), Choice::Settled("codex-cli"));
    }

    /// The person's own answer carries into a project that has pinned nothing — which is the whole
    /// of "the one I chose last time is the one that is chosen".
    #[test]
    fn what_the_person_last_opened_with_answers_where_the_project_has_not() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(settle(None, Some("codex-cli"), &c), Choice::Settled("codex-cli"));
    }

    /// The project's pin outranks the habit: somebody who fixed one project on an agent is not
    /// carried off it by what they last reached for somewhere else.
    #[test]
    fn the_project_outranks_the_person() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(
            settle(Some("claude-code"), Some("codex-cli"), &c),
            Choice::Settled("claude-code"),
        );
    }

    /// A plain prompt is an answer to "what did you open with last time", though it is never one to
    /// "which agent does this project work with".
    #[test]
    fn the_person_may_have_last_opened_a_plain_shell() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(settle(None, Some(SHELL), &c), Choice::Settled(SHELL));
    }

    /// The person's answer gives way the same as the project's when its tool has gone — and what is
    /// left is the question, not an error.
    #[test]
    fn a_persons_answer_that_stopped_being_startable_gives_way() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(
            settle(None, Some("gemini-cli"), &c),
            Choice::Ask(vec!["claude-code", "codex-cli"]),
        );
    }

    /// Nobody has chosen yet and there is more than one thing to choose between: the person is
    /// asked, and the row comes up with nothing on it (`app/src/shell/EmptySlot.tsx`).
    #[test]
    fn the_first_run_is_asked_rather_than_guessed_at() {
        assert_eq!(
            settle(None, None, &built(&[], &["claude", "codex"])),
            Choice::Ask(vec!["claude-code", "codex-cli"]),
        );
    }

    /// Every offered id names a catalog row, which is what lets a caller turn the answer into a
    /// command without a second table.
    #[test]
    fn what_is_offered_can_always_be_started() {
        for one in offered(&built(&[], &["claude", "codex", "gemini", "copilot", "cursor-agent"])) {
            assert_eq!(started_as(one.id).map(|h| h.command), Some(one.command));
        }
    }
}
