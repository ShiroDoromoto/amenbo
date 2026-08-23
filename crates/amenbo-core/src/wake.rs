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
//! **Asking is once per folder.** A folder with several startable agents is a folder whose answer is
//! the person's, so it is asked for and then kept ([`crate::config::Config::agent_for`]) rather than
//! put again every time a pane opens. A remembered answer that stops being startable — the tool was
//! removed — is not an error and not a question either: it simply stops being the answer, and the
//! rank underneath takes over.

use crate::harness::{self, Harness, Wiring};

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

/// The candidates a face offers, in catalog order: the ones this folder traces **and** this machine
/// has, or — when the product is empty — every one this machine has.
///
/// The fallback is not a weakening of the rule. A folder with no trace has said nothing, and the
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

/// The folder's answer: what was remembered if it still holds, otherwise what [`offered`] leaves.
///
/// A remembered id is honoured only while it is still startable, and only while it is still one of
/// the offered — a folder that has since grown a trace for the agent actually used in it should not
/// be held to an answer given before it did.
pub fn settle(remembered: Option<&str>, candidates: &[Candidate]) -> Choice {
    let offered = offered(candidates);
    if let Some(kept) = remembered {
        if let Some(one) = offered.iter().find(|c| c.id == kept) {
            return Choice::Settled(one.id);
        }
    }
    match offered.as_slice() {
        [] => Choice::Nothing,
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
        assert_eq!(settle(None, &c), Choice::Settled("claude-code"));
    }

    /// Several traced and installed is the one case a person is asked, and the offer is those —
    /// not the whole catalog, and not the machine's other tools.
    #[test]
    fn several_in_the_product_are_offered_and_nothing_else_is() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "codex", "gemini"]);
        assert_eq!(
            settle(None, &c),
            Choice::Ask(vec!["claude-code", "codex-cli"])
        );
    }

    /// A folder that traces nothing has said nothing, so what is asked about is what the machine
    /// has. An install notice here would be shown to a reader whose tools are all installed.
    #[test]
    fn a_folder_with_no_trace_falls_back_to_what_the_machine_has() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(settle(None, &c), Choice::Ask(vec!["claude-code", "codex-cli"]));
        assert_eq!(settle(None, &built(&[], &["gemini"])), Choice::Settled("gemini-cli"));
    }

    /// A trace this machine cannot act on is not an answer. Starting it would open the pane on
    /// `command not found`, which is the one outcome the multiplication exists to rule out.
    #[test]
    fn a_traced_agent_that_is_not_installed_is_never_offered() {
        let c = built(&["claude-code"], &["codex"]);
        assert!(!offered(&c).iter().any(|one| one.id == "claude-code"));
        assert_eq!(settle(None, &c), Choice::Settled("codex-cli"));
    }

    /// Nothing installed is the install notice's case, and the only one.
    #[test]
    fn nothing_installed_is_nothing_to_open_with() {
        assert_eq!(settle(None, &built(&["claude-code"], &[])), Choice::Nothing);
        assert_eq!(settle(Some("claude-code"), &built(&["claude-code"], &[])), Choice::Nothing);
    }

    /// What was remembered wins while it still holds — that is the whole of "asked once".
    #[test]
    fn the_remembered_answer_is_the_answer() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "codex"]);
        assert_eq!(settle(Some("codex-cli"), &c), Choice::Settled("codex-cli"));
    }

    /// A remembered answer whose tool has gone is not an error and not a question about itself: the
    /// rank underneath simply answers instead.
    #[test]
    fn a_remembered_answer_that_stopped_being_startable_gives_way() {
        let c = built(&["claude-code", "codex-cli"], &["codex"]);
        assert_eq!(settle(Some("claude-code"), &c), Choice::Settled("codex-cli"));
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
