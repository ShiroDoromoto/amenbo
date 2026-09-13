//! **What a notification says one event was**, in the language the reader chose (`AMB-D-885`).
//!
//! One event becomes one line, and this is where that line is put together. The words themselves are
//! not here — they are in the GUI's dictionaries, baked into [`crate::notify_wording_table`] by
//! `scripts/gen-notify-wording.mjs` — so a language is a row there and nothing here.
//!
//! **The language is not a setting of the feature's.** The reader already told Amenbo which language
//! they read in ([`crate::config::Config::language`]), and asking a second time would make two
//! answers that can disagree. A code with no row of its own is written in English, because a message
//! saying what happened, to what, and when still does its job in the wrong language, and not sending
//! it does not.
//!
//! **The status words are Amenbo's own**, read out of the same dictionary rather than worded again: a
//! line calling a state one thing while the app calls it another leaves the reader looking for
//! something that is not there under that name.
//!
//! **Four slots and never more.** Three of them hold something this module must not translate — the
//! name the reader gave their AI, the record's own ref, a project's slug or an assignee's facet — and
//! the fourth is an event name off the wire.
//!
//! **A subject is worded here too, and separately** ([`subject_what`], [`subject_count`]). It is not the
//! line shortened: a line says who did what to which record, a subject says only what happened. Both are
//! read out of the same dictionary, so the two halves of one message cannot end up in two languages.

use crate::notify_wording_table::{Wording, WORDINGS};
use crate::plugin_payload::name;

/// The language every other one is read against: the one that is always complete. A code this build
/// has never heard of falls back to it whole, and a key nobody has translated falls back to it on its
/// own — so a dictionary can arrive in pieces without any of them being the piece that leaves a line
/// blank.
const FALLBACK: &str = "en";

/// What a line is filled in from: the event, and the three things a sentence may name.
#[derive(Clone, Copy, Debug, Default)]
pub struct Said<'a> {
    /// One of [`crate::plugin_payload::V1_EVENTS`].
    pub event: &'a str,
    /// Whoever drove the write, by the name they go by. Empty on the two events nobody drove.
    pub who: &'a str,
    /// Which record it was — the ref, as the reader would search for it.
    pub what: &'a str,
    /// **The second thing the event names**, or nothing where it did not arrive.
    ///
    /// Three kinds reach this one slot and two of them are already the reader's words by the time
    /// they do: a status said the way Amenbo says it ([`status_word`]), and an assignee said by the
    /// name that facet goes by. The third is a project's slug, which is the store's own value and the
    /// one the reader would go and search for, so it passes through untouched.
    pub state: Option<&'a str>,
}

/// One event as a line.
///
/// The form is chosen by whether the second thing the event names arrived: a status change that says
/// which status, an assignment that says to whom, read better than the bare pair — and the bare pair
/// is what an older build's row, or a comment on an Amenbo too old to carry its parent, leaves.
pub fn line(language: &str, said: &Said<'_>) -> String {
    let key = key_for(said.event, said.state.is_some());
    let template = says(language, key)
        .or_else(|| says(language, bare_of(key)))
        .or_else(|| says(language, "unknown"))
        .unwrap_or("{who} acted on {what} ({event})");
    template
        .replace("{who}", said.who)
        .replace("{what}", said.what)
        .replace("{state}", said.state.unwrap_or_default())
        .replace("{event}", said.event)
}

/// **What a subject says one event was** — the same event as [`line()`], in the fewest words that still
/// say it.
///
/// A line says who did what to which record; a subject says only what happened, the record's own ref
/// following it. That is not the same sentence shortened: a subject is read in a strip that shows thirty
/// or forty characters, and the half of it that would survive a cut is the half the reader already knows.
///
/// `state` is the second thing the event names, already in the reader's words ([`status_word`]) — the
/// only slot a subject phrase carries, and the one that decides which of the two forms a status change
/// is said in. Without it the bare form is said, rather than a phrase left hanging on its preposition.
/// An event with no phrase of its own is named by its own code, which beats a blank subject for a caller
/// that hands one over.
pub fn subject_what(language: &str, event: &str, state: Option<&str>) -> String {
    let key = subject_key(event, state.is_some());
    match says(language, key).or_else(|| says(language, subject_key(event, false))) {
        Some(template) => template.replace("{state}", state.unwrap_or_default()),
        None => event.to_string(),
    }
}

/// **What a subject says a burst was** — how many, since by then the one thing the events still have in
/// common is the project and their number.
///
/// Two forms and never more. Most languages outside Europe change nothing at one, and the ones that do
/// change it once; the fuller plural families are dodged by the wording itself ("Обновления: {n}"), which
/// is what keeps this from having to carry a rule per language. `countOne` is written in every dictionary
/// even where it repeats `count`, so a correction to one cannot leave the other behind.
///
/// It counts one as readily as five: a line an older build left carries no event to name, and a message
/// carrying that single line has nothing to say but how many.
pub fn subject_count(language: &str, n: usize) -> String {
    let key = if n == 1 { "subject.countOne" } else { "subject.count" };
    let template = says(language, key)
        .or_else(|| says(language, "subject.count"))
        .unwrap_or("{n} updates");
    template.replace("{n}", &n.to_string())
}

/// Which key a subject phrase is written under. A fourteenth event has none, and
/// [`subject_what()`] names it by its code rather than saying nothing.
fn subject_key(event: &str, elaborated: bool) -> &'static str {
    match (event, elaborated) {
        (name::TASK_CREATED, _) => "subject.taskCreated",
        (name::TASK_STATUS_CHANGED, true) => "subject.statusChanged",
        (name::TASK_STATUS_CHANGED, false) => "subject.statusChangedBare",
        (name::TASK_DONE, _) => "subject.taskDone",
        (name::TASK_REJECTED, _) => "subject.taskRejected",
        (name::TASK_ASSIGNED, _) => "subject.taskAssigned",
        (name::TASK_MOVED, _) => "subject.taskMoved",
        (name::TASK_DELETED, _) => "subject.taskDeleted",
        (name::DECISION_ACCEPTED, _) => "subject.decisionAccepted",
        (name::DECISION_REJECTED, _) => "subject.decisionRejected",
        (name::COMMENT_ADDED, _) => "subject.commentAdded",
        (name::COMMENT_REMOVED, _) => "subject.commentRemoved",
        (name::TASK_DUE, _) => "subject.taskDue",
        (name::TASK_DUE_TOMORROW, _) => "subject.taskDueTomorrow",
        _ => "subject.unknown",
    }
}

/// The line a test message carries — the one sentence here no event produces, since somebody pressed
/// a button. It is worded with the rest because it lands in the same channel as every other.
pub fn test_line(language: &str) -> String {
    says(language, "test").unwrap_or("Test message from Amenbo.").to_string()
}

/// Amenbo's own word for a state, or the bare value where the dictionary has none — which is what a
/// state off the wire that this build has no word for should read as.
pub fn status_word(language: &str, status: &str) -> String {
    let found = wording(language)
        .and_then(|row| lookup(row.statuses, status))
        .or_else(|| wording(FALLBACK).and_then(|row| lookup(row.statuses, status)));
    found.unwrap_or(status).to_string()
}

/// Which key an event is said under, and in which of its two forms.
///
/// An event this build has no key for is not a hole: [`line()`] falls through to the `unknown` sentence,
/// which names the event rather than saying nothing.
fn key_for(event: &str, elaborated: bool) -> &'static str {
    match (event, elaborated) {
        (name::TASK_CREATED, _) => "taskCreated",
        (name::TASK_STATUS_CHANGED, true) => "statusChanged",
        (name::TASK_STATUS_CHANGED, false) => "statusChangedBare",
        (name::TASK_DONE, _) => "taskDone",
        (name::TASK_REJECTED, _) => "taskRejected",
        (name::TASK_ASSIGNED, true) => "taskAssigned",
        (name::TASK_ASSIGNED, false) => "taskAssignedBare",
        (name::TASK_MOVED, true) => "taskMoved",
        (name::TASK_MOVED, false) => "taskMovedBare",
        (name::TASK_DELETED, _) => "taskDeleted",
        (name::DECISION_ACCEPTED, _) => "decisionAccepted",
        (name::DECISION_REJECTED, _) => "decisionRejected",
        (name::COMMENT_ADDED, true) => "commentAdded",
        (name::COMMENT_ADDED, false) => "commentAddedBare",
        (name::COMMENT_REMOVED, true) => "commentRemoved",
        (name::COMMENT_REMOVED, false) => "commentRemovedBare",
        (name::TASK_DUE, _) => "taskDue",
        (name::TASK_DUE_TOMORROW, _) => "taskDueTomorrow",
        _ => "unknown",
    }
}

/// The bare form of a key that has one, so a language which translated only that form still answers.
fn bare_of(key: &'static str) -> &'static str {
    match key {
        "statusChanged" => "statusChangedBare",
        "taskAssigned" => "taskAssignedBare",
        "taskMoved" => "taskMovedBare",
        "commentAdded" => "commentAddedBare",
        "commentRemoved" => "commentRemovedBare",
        other => other,
    }
}

/// One sentence, in this language or in English.
fn says(language: &str, key: &str) -> Option<&'static str> {
    wording(language)
        .and_then(|row| lookup(row.says, key))
        .or_else(|| wording(FALLBACK).and_then(|row| lookup(row.says, key)))
}

/// The row for a language code, matched exactly as the store spells it (`pt-BR`, not `pt-br`).
fn wording(language: &str) -> Option<&'static Wording> {
    WORDINGS.iter().find(|row| row.language == language)
}

fn lookup(pairs: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    pairs.iter().find(|(at, _)| *at == key).map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LANGUAGES;
    use crate::plugin_payload::V1_EVENTS;

    /// Every language Amenbo is read in has a row, and every row says every one of the thirteen.
    /// A language added to the dictionary and not regenerated here would write its lines in English
    /// with nothing saying so.
    #[test]
    fn every_language_amenbo_offers_is_written_here() {
        for code in LANGUAGES {
            let row = wording(code).unwrap_or_else(|| panic!("{code} has no wording"));
            for event in V1_EVENTS {
                if event == name::STORE_CHANGED {
                    continue;
                }
                let key = key_for(event, false);
                assert!(
                    lookup(row.says, key).is_some(),
                    "{code} has no sentence for {event} ({key})"
                );
            }
            assert!(lookup(row.says, "test").is_some(), "{code} has no test line");
            assert!(lookup(row.says, "unknown").is_some(), "{code} has no unknown line");
            for event in V1_EVENTS {
                if event == name::STORE_CHANGED {
                    continue;
                }
                let key = subject_key(event, false);
                assert!(
                    lookup(row.says, key).is_some(),
                    "{code} has no subject phrase for {event} ({key})"
                );
            }
            // Both forms, even where they are the same words: what keeps a correction to one from
            // leaving the other saying what the language no longer says.
            assert!(lookup(row.says, "subject.count").is_some(), "{code} cannot count");
            assert!(lookup(row.says, "subject.countOne").is_some(), "{code} cannot count one");
        }
    }

    /// The slots a sentence carries have to be the slots English carries, or the line comes out with
    /// a hole where the ref should have been — or with the slot itself printed at the reader.
    #[test]
    fn every_language_keeps_the_slots_english_carries() {
        let english = wording(FALLBACK).expect("English");
        for row in WORDINGS {
            for (key, template) in row.says {
                let Some(reference) = lookup(english.says, key) else { continue };
                for slot in ["{who}", "{what}", "{state}", "{event}", "{n}"] {
                    assert_eq!(
                        template.contains(slot),
                        reference.contains(slot),
                        "{}: {key} disagrees with English about {slot}",
                        row.language
                    );
                }
            }
        }
    }

    /// The status a task moved to is named, not merely that it moved — and it is named with Amenbo's
    /// own word, so the reader finds that state under that name in the app.
    #[test]
    fn a_status_change_names_the_status_in_the_readers_words() {
        let said = Said {
            event: name::TASK_STATUS_CHANGED,
            who: "AI",
            what: "AMB-T-1",
            state: Some(&status_word("ja", "done")),
        };
        assert_eq!(line("ja", &said), "AI が AMB-T-1 を完了にしました");
        let english = Said { state: Some(&status_word("en", "done")), ..said };
        assert_eq!(line("en", &english), "AI moved AMB-T-1 to Done");
    }

    /// Without the second thing, the bare form is said rather than a sentence with a hole in it.
    #[test]
    fn a_sentence_missing_its_second_thing_says_the_bare_form() {
        let said = Said { event: name::TASK_STATUS_CHANGED, who: "AI", what: "AMB-T-1", state: None };
        assert_eq!(line("en", &said), "AI moved AMB-T-1");
        assert!(!line("en", &said).contains('{'));
    }

    /// Two of the thirteen name nobody: a day arriving is not something anyone did, so those
    /// sentences open with the record instead.
    #[test]
    fn a_day_arriving_is_attributed_to_nobody() {
        let said = Said { event: name::TASK_DUE, who: "", what: "AMB-T-1", state: None };
        assert_eq!(line("en", &said), "AMB-T-1 is due");
    }

    /// A language nobody has a row for is written in English rather than left blank.
    #[test]
    fn a_language_with_no_row_falls_back_whole() {
        let said = Said { event: name::TASK_DONE, who: "AI", what: "AMB-T-1", state: None };
        assert_eq!(line("is", &said), line("en", &said));
        assert_eq!(status_word("is", "blocked"), "Blocked");
    }

    /// A fourteenth event names itself, which beats an empty message for a caller that hands one over.
    #[test]
    fn an_event_with_no_sentence_names_itself() {
        let said = Said { event: "task.invented", who: "AI", what: "AMB-T-1", state: None };
        assert!(line("en", &said).contains("task.invented"));
    }

    /// A status off the wire that the dictionary has no word for is shown as it came, rather than as
    /// a blank.
    #[test]
    fn a_status_with_no_word_is_shown_as_it_came() {
        assert_eq!(status_word("ja", "parked"), "parked");
    }

    /// A subject says what happened and stops there — no actor, no record. The line beside it is the
    /// one that says those, and a subject repeating them would spend its thirty readable characters
    /// on what the reader can already see.
    #[test]
    fn a_subject_says_what_happened_and_nothing_else() {
        assert_eq!(subject_what("ja", name::TASK_DONE, None), "タスクを完了");
        assert_eq!(subject_what("en", name::TASK_DONE, None), "Task finished");
    }

    /// The one slot a subject phrase carries is the state, and it is filled with Amenbo's own word —
    /// the same one the line uses, so the two halves of a message name a state the same way.
    #[test]
    fn a_status_change_names_the_status_in_the_subject_too() {
        let state = status_word("ja", "done");
        assert_eq!(subject_what("ja", name::TASK_STATUS_CHANGED, Some(&state)), "タスクのステータスを完了に変更");
    }

    /// Without the state the bare form is said rather than a phrase hanging on its preposition — an
    /// older build's row carries no status to name.
    #[test]
    fn a_status_change_with_no_status_leaves_no_hole() {
        let said = subject_what("en", name::TASK_STATUS_CHANGED, None);
        assert!(!said.contains('{'), "{said}");
        assert_eq!(said, "Task status changed");
    }

    /// A fourteenth event names itself here as well, which beats a subject that says nothing.
    #[test]
    fn an_event_with_no_phrase_names_itself() {
        assert_eq!(subject_what("en", "task.invented", None), "task.invented");
    }

    /// One is said its own way where a language says it its own way, and the same way where it does
    /// not — which is most of them.
    #[test]
    fn a_count_of_one_is_said_the_way_the_language_says_it() {
        assert_eq!(subject_count("en", 1), "1 update");
        assert_eq!(subject_count("en", 4), "4 updates");
        assert_eq!(subject_count("ja", 1), "更新 1件");
        assert_eq!(subject_count("ja", 4), "更新 4件");
    }

    /// A language with no row of its own counts in English, rather than counting in nothing.
    #[test]
    fn a_language_with_no_row_counts_in_english() {
        assert_eq!(subject_count("is", 3), subject_count("en", 3));
        assert_eq!(subject_what("is", name::TASK_DUE, None), subject_what("en", name::TASK_DUE, None));
    }
}
