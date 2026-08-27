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
//! **The row being asked about is the launch catalog's** ([`crate::harness::LAUNCHES`]), which is the
//! wider of the two: a provider Amenbo cannot write a session-start hook for is still one a pane opens
//! on. The trace is read off the wiring catalog because that is the table a folder leaves traces of, so
//! a startable provider with no wiring row simply traces nothing — startable, and unspoken for.
//!
//! **Being installed decides what a press can open; the trace decides what it comes up on**
//! (`AMB-D-792`). The two answer different questions — *can this machine run it* and *is it worked
//! with here* — and the second used to overrule the first: the product of the two was the row, so a
//! folder holding `.claude` and nothing else lost Codex CLI and Gemini CLI off it entirely, which
//! reads as a dead end to somebody who wants one of them.
//!
//! So the row is every catalogued provider ([`offered`]), the ones this machine has are the ones a
//! press can open ([`startable`]), and the trace has moved to the rank below — where it says which of
//! them to come up on rather than which of them exist. What is **not** startable is drawn and not
//! pressed: leaving it off the row is what makes a tool unfindable, and starting it would open a pane
//! on `command not found`.
//!
//! **What is said about a provider stops at "installed".** Whether the person is signed in, whether
//! their subscription is current, whether the version still speaks the flags — none of it is knowable
//! before the program runs, and a face that guessed would be telling the reader something it made up
//! (`AMB-T-3591`). The vocabulary here is **installed / not installed**, and nothing wider.
//!
//! **Four things are looked at in turn, and the first that holds is the answer** ([`settle`]):
//!
//! | | kept against | written by |
//! |---|---|---|
//! | 1 | the project ([`crate::config::Config::agent_for`]) | the project's own settings, and nothing else |
//! | 2 | the person ([`crate::config::Config::last_agent`]) | every press that opens a pane with one |
//! | 3 | the folder ([`Candidate::traced`]) | the provider itself, by leaving its directory in the folder |
//! | 4 | — | nothing: this is the run before anybody has chosen |
//!
//! **The project is above the person because it is the pinned answer.** Somebody who works with one
//! agent everywhere never touches it, and rank 2 carries their choice from the first project into
//! every one after it; somebody who works differently in one repository pins it there, and the pin
//! outranks the habit. Neither is kept per folder: one project can bind several, which would give it
//! as many answers as it has folders.
//!
//! **The trace is under both because nobody gave it as an answer.** A folder holding `.claude` says
//! an agent has been run here, which is a good guess and not a decision — so it settles the run where
//! neither answer holds, and gives way the moment one does. Where a folder traces several, it settles
//! nothing: two guesses are not an answer either.
//!
//! **Rank 4 is the first run, and it is drawn as "nothing is chosen" rather than guessed at.** The
//! one exception is a machine with a single startable agent, where there is no question to put.
//!
//! A kept answer that stops being startable — the tool was removed — is not an error and not a
//! question either: it simply stops being the answer, and the rank underneath takes over.
//!
//! **The trace still reads per folder**, because that is where a provider leaves one. What a project
//! traces is what any of its folders traces: a preference shown in one of them is the project's, and
//! a face gathers the folders before it asks (`app/src-tauri/src/wake.rs`).

use crate::harness::{self, Launch, Wiring};

/// **The pane's own shell** — a prompt in the folder with nothing started at it, standing among the
/// agents as one more thing a person opens with (`app/src/talk/terminal.ts`).
///
/// It has no row in [`harness::LAUNCHES`] and never will: it is the *absence* of an agent, and what
/// starts it is the folder's own login shell. It is here because it is a value the person's own
/// answer can hold ([`crate::config::Config::last_agent`]) — somebody who opened a plain prompt last
/// time gets one again — while the project's answer never holds it: "which agent do you work with
/// here" is not a question a shell answers.
pub const SHELL: &str = "shell";

/// One provider as a place to open a pane on: what the catalog says about it, and what this folder
/// and this machine say about it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Candidate {
    /// The provider this answers for ([`Launch::id`]).
    pub id: &'static str,
    /// The provider's own name for itself, so a face can render a row without a second lookup.
    pub label: &'static str,
    /// What it is started as ([`Launch::command`]) — shown, because a reader told a tool is missing
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
    /// One agent, and nothing to ask: the [`Launch::id`] to start.
    Settled(&'static str),
    /// Several, so the person picks — the ids a press can open ([`startable`]), in catalog order.
    /// What the face *draws* is wider than this ([`offered`]).
    Ask(Vec<&'static str>),
    /// Nothing on this machine can be started, so there is no answer to settle on. The row is still
    /// drawn — every provider is on it, none of them pressable (`AMB-D-792`) — which is what tells a
    /// reader with no agent installed that there is something to install.
    Nothing,
}

/// Every provider as a candidate, in [`harness::LAUNCHES`] order — the startable ones, which is what
/// the question is about. `found` answers only for the providers that also have a wiring row, and one
/// it says nothing about is one no folder can have traced.
///
/// `installed` is asked of the caller rather than performed here, because what counts as installed is
/// what the *pane's* shell can find: the same login-and-interactive shell, with the same profile and
/// the same `PATH` (`app/src-tauri/src/launch.rs`). A probe run against this process's environment
/// would answer for a desktop launch's thin `PATH` and be wrong in both directions (`AMB-T-3546`).
pub fn candidates(found: &[Wiring], installed: impl Fn(&str) -> bool) -> Vec<Candidate> {
    harness::LAUNCHES
        .iter()
        .map(|launch| Candidate {
            id: launch.id,
            label: launch.label,
            command: launch.command,
            traced: found
                .iter()
                .find(|one| one.id == launch.id)
                .is_some_and(|one| one.traced),
            installed: installed(launch.command),
        })
        .collect()
}

/// The candidates a face draws, in catalog order — **every one of them** (`AMB-D-792`).
///
/// Nothing is filtered out here, which is the point: a provider left off the row is one the reader
/// cannot install their way onto, and neither of the two facts about a candidate is a reason to leave
/// it off. Not the trace, which says what is worked with here and not what exists. Not being
/// installed either — a tool a reader has yet to install is exactly the one the row has to be able
/// to tell them about, and it is drawn unpressable rather than left out (`app/src/shell/EmptySlot.tsx`).
///
/// It is a function rather than the slice itself because *what a face draws* is a question worth
/// having one answer to, and because the answer has changed once already.
pub fn offered(candidates: &[Candidate]) -> Vec<&Candidate> {
    candidates.iter().collect()
}

/// The candidates a press can actually open, in catalog order: [`offered`] minus what this machine
/// has not got.
///
/// This is the half that must never be widened by mistake. What is settled on here is started
/// ([`crate::harness::Launch::command`]), so a row that is not installed reaching this would open a
/// pane on `command not found` — which is the one outcome the two lists exist to keep apart.
pub fn startable(candidates: &[Candidate]) -> Vec<&Candidate> {
    candidates.iter().filter(|one| one.installed).collect()
}

/// The answer: the project's if it still holds, else the person's, else the folder's trace, else
/// nothing — the four ranks the module docs set out, in that order.
///
/// `remembered` is what this project pinned ([`crate::config::Config::agent_for`]) and `last` is
/// what this person last opened with ([`crate::config::Config::last_agent`]).
///
/// **Every rank is read against [`startable`] and never against [`offered`]**, which is what keeps
/// a row drawn for a tool the reader might install from becoming a pane opened on one they have not.
/// An answer whose tool has gone is not an error and not a question about itself: it stops holding,
/// and the rank underneath answers instead. [`SHELL`] is the one value that always holds, since a
/// folder always has a prompt — but only where there is something to open at all, because a machine
/// with nothing startable has one terminal and no choice to make about it.
///
/// [`Choice::Ask`] is the run before anybody has chosen, and it is put to the person as a row with
/// **nothing on it** rather than as a guess.
pub fn settle(remembered: Option<&str>, last: Option<&str>, candidates: &[Candidate]) -> Choice {
    let startable = startable(candidates);
    if startable.is_empty() {
        return Choice::Nothing;
    }
    let holds = |kept: &str| startable.iter().find(|c| c.id == kept).map(|one| one.id);
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
    // The folder's own guess, and only where it points at one thing: a folder tracing two says which
    // agents have been run here, which is not the same as saying which one to open with now.
    if let [one] = traced(&startable).as_slice() {
        return Choice::Settled(one.id);
    }
    match startable.as_slice() {
        // One thing to open with is not a question: asking about a row with a single answer on it
        // would be asking the person to agree with the machine.
        [one] => Choice::Settled(one.id),
        several => Choice::Ask(several.iter().map(|c| c.id).collect()),
    }
}

/// The startable candidates this folder shows a trace of, in catalog order.
fn traced<'a>(startable: &[&'a Candidate]) -> Vec<&'a Candidate> {
    startable.iter().copied().filter(|one| one.traced).collect()
}

/// The launch row an id names, for a caller holding a [`Choice::Settled`].
pub fn started_as(id: &str) -> Option<&'static Launch> {
    harness::find_launch(id)
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

    /// The folder's own guess, where nobody has given an answer: one traced agent this machine has
    /// is what the run comes up on, and the machine's other tools do not make it a question.
    #[test]
    fn a_folder_that_traces_one_installed_agent_is_not_asked_about() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "gemini"]);
        assert_eq!(settle(None, None, &c), Choice::Settled("claude-code"));
    }

    /// A folder tracing two has said which agents have been run in it, which is not an answer to
    /// which one to open with — so nothing is settled, and what is asked about is everything this
    /// machine can start rather than the two.
    #[test]
    fn a_folder_that_traces_several_settles_nothing() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "codex", "gemini"]);
        assert_eq!(
            settle(None, None, &c),
            Choice::Ask(vec!["claude-code", "codex-cli", "gemini-cli"])
        );
    }

    /// The trace is a rank and not a filter (`AMB-D-792`): it is read under both answers a person
    /// gave, and it gives way to either. The old shape had it deciding the row instead, which is
    /// what took Codex CLI off a folder holding `.claude`.
    #[test]
    fn a_trace_gives_way_to_an_answer_somebody_gave() {
        let c = built(&["codex-cli"], &["claude", "codex"]);
        assert_eq!(settle(None, Some("claude-code"), &c), Choice::Settled("claude-code"));
        assert_eq!(settle(Some("claude-code"), None, &c), Choice::Settled("claude-code"));
        // And with neither given, the trace is what is left to answer with.
        assert_eq!(settle(None, None, &c), Choice::Settled("codex-cli"));
    }

    /// A trace this machine cannot act on settles nothing — the rank reads what can be started, so
    /// what is left is the question rather than a pane opened on `command not found`.
    #[test]
    fn a_trace_for_a_tool_that_is_not_installed_settles_nothing() {
        let c = built(&["claude-code"], &["codex", "gemini"]);
        assert_eq!(settle(None, None, &c), Choice::Ask(vec!["codex-cli", "gemini-cli"]));
    }

    /// The row is the whole catalog, whatever this folder traces and whatever this machine has: a
    /// provider left off it is one the reader cannot install their way onto. What being installed
    /// decides is [`startable`], which is what a press is allowed to open.
    #[test]
    fn the_row_is_drawn_whole_and_only_the_press_is_narrowed() {
        let c = built(&["claude-code"], &["claude"]);
        assert_eq!(offered(&c).len(), harness::LAUNCHES.len(), "every provider is on the row");
        assert_eq!(
            startable(&c).iter().map(|one| one.id).collect::<Vec<_>>(),
            ["claude-code"],
            "and only what this machine has can be pressed",
        );
        // Nothing installed at all is still a whole row — that is what tells a reader with no agent
        // there is something to install.
        let bare = built(&[], &[]);
        assert_eq!(offered(&bare).len(), harness::LAUNCHES.len());
        assert!(startable(&bare).is_empty());
    }

    /// A folder that traces nothing has said nothing, so what is asked about is what the machine
    /// has. An install notice here would be shown to a reader whose tools are all installed.
    #[test]
    fn a_folder_with_no_trace_falls_back_to_what_the_machine_has() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(settle(None, None, &c), Choice::Ask(vec!["claude-code", "codex-cli"]));
        assert_eq!(settle(None, None, &built(&[], &["gemini"])), Choice::Settled("gemini-cli"));
    }

    /// A trace this machine cannot act on is not something a press may open. Starting it would open
    /// the pane on `command not found`, which is the one outcome the two lists exist to keep apart —
    /// it stays on the row, and the one installed agent is what the run settles on.
    #[test]
    fn a_traced_agent_that_is_not_installed_is_never_pressed() {
        let c = built(&["claude-code"], &["codex"]);
        assert!(!startable(&c).iter().any(|one| one.id == "claude-code"));
        assert!(offered(&c).iter().any(|one| one.id == "claude-code"), "and it is still drawn");
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

    /// A provider the wiring catalog does not list is still one this machine can open a pane on. It
    /// traces nothing — no folder can trace a wiring that has no row — so it comes up under the
    /// fallback, which is where a machine's own tools are answered for.
    #[test]
    fn a_provider_with_no_wiring_row_is_offered_on_being_installed() {
        let c = built(&[], &["opencode"]);
        assert_eq!(settle(None, None, &c), Choice::Settled("opencode"));
        // And it loses to a folder's trace like anything else does: what is traced and installed is
        // the answer while there is one.
        let c = built(&["claude-code"], &["opencode", "claude"]);
        assert_eq!(settle(None, None, &c), Choice::Settled("claude-code"));
    }

    /// Every id on the row names a launch row, which is what lets a caller turn the answer into a
    /// command without a second table.
    #[test]
    fn what_is_offered_can_always_be_started() {
        for one in offered(&built(&[], &["claude", "codex", "gemini", "copilot", "cursor-agent"])) {
            assert_eq!(started_as(one.id).map(|h| h.command), Some(one.command));
        }
    }
}
