//! How a record's ref is written where output names it — the `AMB-` namespace, in one place, so
//! every command spells a task, a decision, a comment, an axis and a project the same way.
//!
//! It is also where a ref stops being a string and becomes somewhere to go. Inside a pane of the talk
//! window, a task's and a decision's ref are wrapped in OSC 8 — the escape that tells a terminal "these
//! characters are a link, and this is where it points" — so the pane can open the record without
//! matching text on the way. Matching is what breaks: a TUI wraps at the pane's width and elides with
//! `…`, and a ref split across two rows or cut in half is a ref no pattern finds. This is our own
//! output, so we can say where it points instead of leaving anyone to work it out.
//!
//! **Nothing outside the pane is given a link.** Terminals that understand OSC 8 would make it
//! clickable, and clicking would reach an `amenbo://` handler nobody registered — a link that looks
//! live and does nothing, which is the same lie as a command that answers "ok" and shows the person
//! nothing (`AMB-D-749`). Outside a pane the output is exactly the characters it always was.

use std::sync::OnceLock;

use amenbo_core::idref::{self, RefKind};

/// Whether this run's refs are written as links. Answered once, before any output, because it cannot
/// change mid-run: it is a fact about the terminal this process was started in.
static AS_LINKS: OnceLock<bool> = OnceLock::new();

/// Settle whether refs are written as links, from the two things that decide it.
///
/// The pane, because it is the only surface that can act on one. And stdout being a terminal, because a
/// redirect and a pipe both look like this and neither renders an escape — what would land there is the
/// escape bytes themselves, in the middle of text something else is about to read.
///
/// `--json` is not consulted: the machine face is built by `print_json`, and no ref written here reaches
/// it. Nor is `--no-color`: a link is not a colour, and someone who turned hues off did not ask for the
/// records to stop being reachable.
pub(crate) fn settle_link_rendering() {
    let in_a_pane = amenbo_core::session::surface().is_some();
    let _ = AS_LINKS.set(in_a_pane && std::io::IsTerminal::is_terminal(&std::io::stdout()));
}

/// One ref, linked where this run writes links and where the kind names a destination
/// ([`idref::url`]). Everything else comes back as the characters alone.
///
/// The escape is `OSC 8 ; ; <uri> ST <text> OSC 8 ; ; ST`: an empty parameter field, because the only
/// parameter anyone defines is an `id=` used to join runs of one link split across lines, and a ref is
/// never split — it is written whole, here, in one piece. `ST` is `ESC \`, the form every terminal that
/// implements this reads.
fn spell(kind: RefKind, id: i64) -> String {
    spell_as(AS_LINKS.get().copied().unwrap_or(false), kind, id)
}

/// [`spell`], with the run-wide answer handed in. Separate so both sides of it can be asked directly: the
/// answer is settled once per process and a suite is not a process per test.
fn spell_as(as_links: bool, kind: RefKind, id: i64) -> String {
    let text = idref::render(kind, id);
    let Some(url) = as_links.then(|| idref::url(kind, id)).flatten() else {
        return text;
    };
    format!("\u{1b}]8;;{url}\u{1b}\\{text}\u{1b}]8;;\u{1b}\\")
}

/// What a decision is called (`AMB-D-<n>`). The id is the conversational number, so reading the row would
/// tell us nothing the ref does not already carry — like [`task_label`], it is built straight from the id.
pub(crate) fn decision_label(id: i64) -> String {
    spell(RefKind::Decision, id)
}

/// What a task is called (`AMB-T-<n>`). The id is the conversational number, so the truth source is never
/// queried.
pub(crate) fn task_label(id: i64) -> String {
    spell(RefKind::Task, id)
}

/// What a task comment is called (`AMB-TC-<n>`), and what a decision comment is called (`AMB-DC-<n>`). A
/// comment carries no conversational number of its own, so this ref is the only handle `comment rm` /
/// `comment attach` can be given — and the two tables number independently, so which spelling a caller
/// reaches for is decided by the table it just wrote, never by the id (`AMB-D-377`).
pub(crate) fn task_comment_label(id: i64) -> String {
    spell(RefKind::TaskComment, id)
}

pub(crate) fn decision_comment_label(id: i64) -> String {
    spell(RefKind::DecisionComment, id)
}

/// What a dimension is called (`AMB-DIM-<n>`), and one of its values (`AMB-DIMV-<n>`). Both resolve by name
/// too, so the ref is the tie-breaker rather than the everyday handle.
pub(crate) fn dimension_label(id: i64) -> String {
    spell(RefKind::Dimension, id)
}

pub(crate) fn dimension_value_label(id: i64) -> String {
    spell(RefKind::DimensionValue, id)
}

/// What a project is called (`AMB-P-<n>`), taken from the id as the probe reports it (a decimal string). An
/// unreadable id is shown as it came, rather than dressed up as a ref it is not.
pub(crate) fn project_label(id: &str) -> String {
    match id.parse::<i64>() {
        Ok(n) => spell(RefKind::Project, n),
        Err(_) => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where no link is written, a label is the characters and nothing else — which is what every
    /// terminal outside a pane, every pipe and every redirect gets.
    #[test]
    fn a_label_outside_a_pane_carries_no_escape() {
        for label in [
            spell_as(false, RefKind::Task, 3595),
            spell_as(false, RefKind::Decision, 749),
            spell_as(false, RefKind::TaskComment, 1),
            spell_as(false, RefKind::Dimension, 11),
        ] {
            assert!(!label.contains('\u{1b}'), "no escape reaches a plain terminal: {label:?}");
        }
        assert_eq!(task_label(3595), "AMB-T-3595", "and the suite itself is outside one");
        assert_eq!(project_label("not a number"), "not a number", "an id that is no ref is shown as it came");
    }

    /// Inside a pane, a task and a decision are wrapped in the escape that says where they lead — and the
    /// characters between the two halves are exactly the ref, so what a person reads is unchanged.
    #[test]
    fn inside_a_pane_the_two_records_the_board_opens_are_written_as_links() {
        let linked = spell_as(true, RefKind::Task, 3595);
        assert_eq!(
            linked,
            "\u{1b}]8;;amenbo://task/3595\u{1b}\\AMB-T-3595\u{1b}]8;;\u{1b}\\",
            "the escape opens with the destination and closes with an empty one",
        );
        assert!(
            spell_as(true, RefKind::Decision, 749).contains("amenbo://decision/749"),
            "a decision leads to its own record",
        );
    }

    /// A kind with nowhere to lead is left as text even inside a pane. Wrapping it would put a live-looking
    /// link on the screen that opens nothing, which is the failure this whole layer is built to avoid.
    #[test]
    fn a_kind_with_nowhere_to_lead_is_left_as_text_inside_a_pane_too() {
        for nowhere in [RefKind::TaskComment, RefKind::DecisionComment, RefKind::Dimension, RefKind::Project] {
            let label = spell_as(true, nowhere, 1);
            assert!(!label.contains('\u{1b}'), "{nowhere:?} names no destination, so it is not dressed as one");
            assert_eq!(label, idref::render(nowhere, 1));
        }
    }
}
