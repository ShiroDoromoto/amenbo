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
//! **And beside it, what the reader registered** ([`crate::config::Config::custom_agents`],
//! `AMB-D-794`). The catalog is a shortcut and not a census — this field is a fast-moving one, and the
//! table goes out of date faster than it can be corrected — so a command the reader wrote themselves
//! stands as a candidate on the same terms as a catalogued one. It traces nothing, for the same
//! reason a provider with no wiring row traces nothing, and it is judged installed by the first word
//! of its line and by nothing else.
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

use crate::config::CustomAgent;
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
///
/// **A row the reader registered stands here too** ([`CustomAgent`], `AMB-D-794`), which is why the
/// strings are owned: the catalog is `'static` and a registration is not. Everything the ranks below
/// do is done to both kinds alike — a registered command that is not installed is not offered, and
/// one this project pinned is the answer while it holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Candidate {
    /// The provider this answers for ([`Launch::id`], or [`CustomAgent::id`]).
    pub id: String,
    /// The provider's own name for itself, so a face can render a row without a second lookup.
    pub label: String,
    /// What it is started as ([`Launch::command`]) — shown, because a reader told a tool is missing
    /// needs the word to type to install it. For a registered row it is the first word of the line
    /// ([`CustomAgent::command`]), which is the whole of what can be looked for.
    pub command: String,
    /// The registered command line, as written — present **exactly** for a row the reader
    /// registered, and the one thing that tells the two kinds apart.
    ///
    /// A face draws it because what is registered runs in a terminal, and somebody choosing a row
    /// should be able to read what pressing it starts.
    pub line: Option<String>,
    /// Whether this folder shows a trace of the provider being used here ([`Wiring::traced`]).
    ///
    /// Always `false` for a registered row: a trace is a wiring catalog's row and a registration has
    /// none, so nothing a folder holds can point at one.
    pub traced: bool,
    /// Whether the command is on the `PATH` of the shell a pane starts with. Only ever this — see
    /// the module docs on what it deliberately does not claim.
    pub installed: bool,
}

/// What to do with the folder's answer once it has been worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// One agent, and nothing to ask: the id to start ([`Candidate::id`]).
    Settled(String),
    /// Several, so the person picks — the ids to offer, in catalog order then registration order.
    Ask(Vec<String>),
    /// Nothing on this machine can be started. The face has an install notice to put, not a question.
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
pub fn candidates(
    found: &[Wiring],
    registered: &[CustomAgent],
    installed: impl Fn(&str) -> bool,
) -> Vec<Candidate> {
    let catalogued = harness::LAUNCHES.iter().map(|launch| Candidate {
        id: launch.id.to_string(),
        label: launch.label.to_string(),
        command: launch.command.to_string(),
        line: None,
        traced: found
            .iter()
            .find(|one| one.id == launch.id)
            .is_some_and(|one| one.traced),
        installed: installed(launch.command),
    });
    // The reader's own rows come after the catalog's, which is the order a face draws them in
    // (`app/src/shell/EmptySlot.tsx`): the shortcut first, then what the shortcut did not cover.
    let own = registered.iter().map(|one| Candidate {
        id: one.id.clone(),
        label: one.label.clone(),
        command: one.command().to_string(),
        line: Some(one.line.clone()),
        // Nothing can have traced it: a trace is left against a wiring row, and a registration has
        // none. It reaches the offer through the fallback, the same as a startable provider that
        // the wiring catalog does not list.
        traced: false,
        installed: installed(one.command()),
    });
    catalogued.chain(own).collect()
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
    let holds = |kept: &str| offered.iter().find(|c| c.id == kept).map(|one| one.id.clone());
    if let Some(id) = remembered.and_then(holds) {
        return Choice::Settled(id);
    }
    if let Some(kept) = last {
        if kept == SHELL {
            return Choice::Settled(SHELL.to_string());
        }
        if let Some(id) = holds(kept) {
            return Choice::Settled(id);
        }
    }
    match offered.as_slice() {
        // One thing to open with is not a question: asking about a row with a single answer on it
        // would be asking the person to agree with the machine.
        [one] => Choice::Settled(one.id.clone()),
        several => Choice::Ask(several.iter().map(|c| c.id.clone()).collect()),
    }
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
        with_registered(traced, installed, &[])
    }

    /// The same, with commands the reader registered standing beside the catalog's.
    fn with_registered(
        traced: &[&str],
        installed: &[&str],
        registered: &[(&str, &str)],
    ) -> Vec<Candidate> {
        let found: Vec<Wiring> = harness::HARNESSES
            .iter()
            .map(|h| Wiring {
                id: h.id,
                label: h.label,
                wired_at: None,
                traced: traced.contains(&h.id),
            })
            .collect();
        let mut config = crate::config::Config::default();
        for (label, line) in registered {
            config.register_agent(label, line).expect("a registered row");
        }
        candidates(&found, config.custom_agents(), |cmd| installed.contains(&cmd))
    }

    /// `Choice::Settled` for a literal, so the assertions read as they did when ids were `'static`.
    fn settled(id: &str) -> Choice {
        Choice::Settled(id.to_string())
    }

    /// `Choice::Ask` for literals, same reason.
    fn ask(ids: &[&str]) -> Choice {
        Choice::Ask(ids.iter().map(|one| one.to_string()).collect())
    }

    /// The product decides while it has anything in it: an agent this machine has but this folder
    /// has never been opened with does not compete with the one the folder traces.
    #[test]
    fn a_folder_that_traces_one_installed_agent_is_not_asked_about() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "gemini"]);
        assert_eq!(settle(None, None, &c), settled("claude-code"));
    }

    /// Several traced and installed is the one case a person is asked, and the offer is those —
    /// not the whole catalog, and not the machine's other tools.
    #[test]
    fn several_in_the_product_are_offered_and_nothing_else_is() {
        let c = built(&["claude-code", "codex-cli"], &["claude", "codex", "gemini"]);
        assert_eq!(
            settle(None, None, &c),
            ask(&["claude-code", "codex-cli"])
        );
    }

    /// A folder that traces nothing has said nothing, so what is asked about is what the machine
    /// has. An install notice here would be shown to a reader whose tools are all installed.
    #[test]
    fn a_folder_with_no_trace_falls_back_to_what_the_machine_has() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(settle(None, None, &c), ask(&["claude-code", "codex-cli"]));
        assert_eq!(settle(None, None, &built(&[], &["gemini"])), settled("gemini-cli"));
    }

    /// A trace this machine cannot act on is not an answer. Starting it would open the pane on
    /// `command not found`, which is the one outcome the multiplication exists to rule out.
    #[test]
    fn a_traced_agent_that_is_not_installed_is_never_offered() {
        let c = built(&["claude-code"], &["codex"]);
        assert!(!offered(&c).iter().any(|one| one.id == "claude-code"));
        assert_eq!(settle(None, None, &c), settled("codex-cli"));
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
        assert_eq!(settle(Some("codex-cli"), None, &c), settled("codex-cli"));
    }

    /// A remembered answer whose tool has gone is not an error and not a question about itself: the
    /// rank underneath simply answers instead.
    #[test]
    fn a_remembered_answer_that_stopped_being_startable_gives_way() {
        let c = built(&["claude-code", "codex-cli"], &["codex"]);
        assert_eq!(settle(Some("claude-code"), None, &c), settled("codex-cli"));
    }

    /// The person's own answer carries into a project that has pinned nothing — which is the whole
    /// of "the one I chose last time is the one that is chosen".
    #[test]
    fn what_the_person_last_opened_with_answers_where_the_project_has_not() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(settle(None, Some("codex-cli"), &c), settled("codex-cli"));
    }

    /// The project's pin outranks the habit: somebody who fixed one project on an agent is not
    /// carried off it by what they last reached for somewhere else.
    #[test]
    fn the_project_outranks_the_person() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(
            settle(Some("claude-code"), Some("codex-cli"), &c),
            settled("claude-code"),
        );
    }

    /// A plain prompt is an answer to "what did you open with last time", though it is never one to
    /// "which agent does this project work with".
    #[test]
    fn the_person_may_have_last_opened_a_plain_shell() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(settle(None, Some(SHELL), &c), settled(SHELL));
    }

    /// The person's answer gives way the same as the project's when its tool has gone — and what is
    /// left is the question, not an error.
    #[test]
    fn a_persons_answer_that_stopped_being_startable_gives_way() {
        let c = built(&[], &["claude", "codex"]);
        assert_eq!(
            settle(None, Some("gemini-cli"), &c),
            ask(&["claude-code", "codex-cli"]),
        );
    }

    /// Nobody has chosen yet and there is more than one thing to choose between: the person is
    /// asked, and the row comes up with nothing on it (`app/src/shell/EmptySlot.tsx`).
    #[test]
    fn the_first_run_is_asked_rather_than_guessed_at() {
        assert_eq!(
            settle(None, None, &built(&[], &["claude", "codex"])),
            ask(&["claude-code", "codex-cli"]),
        );
    }

    /// A provider the wiring catalog does not list is still one this machine can open a pane on. It
    /// traces nothing — no folder can trace a wiring that has no row — so it comes up under the
    /// fallback, which is where a machine's own tools are answered for.
    #[test]
    fn a_provider_with_no_wiring_row_is_offered_on_being_installed() {
        let c = built(&[], &["opencode"]);
        assert_eq!(settle(None, None, &c), settled("opencode"));
        // And it loses to a folder's trace like anything else does: what is traced and installed is
        // the answer while there is one.
        let c = built(&["claude-code"], &["opencode", "claude"]);
        assert_eq!(settle(None, None, &c), settled("claude-code"));
    }

    /// A registered command is a candidate on the same terms as a catalogued one: installed decides
    /// whether it is offered, and nothing about it being the reader's own changes that.
    #[test]
    fn a_registered_command_stands_beside_the_catalogued_ones() {
        let c = with_registered(&[], &["mine"], &[("Mine", "mine --model big")]);
        assert_eq!(settle(None, None, &c), settled("custom:1"));
        // The first word is what was looked for; the whole line is carried for the face to draw.
        let row = c.iter().find(|one| one.id == "custom:1").expect("the registered row");
        assert_eq!(row.command, "mine");
        assert_eq!(row.line.as_deref(), Some("mine --model big"));
        assert!(!row.traced, "nothing can have traced a row the wiring catalog does not list");
    }

    /// The first word is the whole of the judgment. A line whose program is not there is not
    /// offered, exactly as an uninstalled catalog row is not — the flags after it are nobody's to
    /// vouch for.
    #[test]
    fn a_registered_command_whose_program_is_missing_is_not_offered() {
        let c = with_registered(&[], &["claude"], &[("Mine", "mine --model big")]);
        assert!(!offered(&c).iter().any(|one| one.id == "custom:1"));
        assert_eq!(settle(None, None, &c), settled("claude-code"));
    }

    /// A registered row is an answer the ranks can hold, and it gives way when it stops holding —
    /// which is what keeps a deleted registration from being returned as the thing to start.
    #[test]
    fn a_registered_answer_holds_and_gives_way_like_any_other() {
        let c = with_registered(&[], &["claude", "mine"], &[("Mine", "mine")]);
        assert_eq!(settle(Some("custom:1"), None, &c), settled("custom:1"));
        assert_eq!(settle(None, Some("custom:1"), &c), settled("custom:1"));
        // Registered, then deleted: the id is still written down, and the rank underneath answers.
        let gone = built(&[], &["claude"]);
        assert_eq!(settle(Some("custom:1"), None, &gone), settled("claude-code"));
        assert_eq!(settle(None, Some("custom:1"), &gone), settled("claude-code"));
    }

    /// The catalog is drawn first and the reader's own after it, which is the order a face lays the
    /// rows out in.
    #[test]
    fn the_readers_own_rows_come_after_the_catalogues() {
        let c = with_registered(&[], &["claude", "mine"], &[("Mine", "mine")]);
        let ids: Vec<&str> = c.iter().map(|one| one.id.as_str()).collect();
        assert_eq!(ids.last(), Some(&"custom:1"));
        assert_eq!(settle(None, None, &c), ask(&["claude-code", "custom:1"]));
    }

    /// Every offered id names a launch row, which is what lets a caller turn the answer into a
    /// command without a second table.
    #[test]
    fn what_is_offered_can_always_be_started() {
        for one in offered(&built(&[], &["claude", "codex", "gemini", "copilot", "cursor-agent"])) {
            assert_eq!(started_as(&one.id).map(|h| h.command), Some(one.command.as_str()));
        }
    }
}
