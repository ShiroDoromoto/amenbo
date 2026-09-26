//! **What a stopped run says on its task**, in the language the reader chose (`AMB-D-976`) — and what a
//! built-in says of how it finished ([`builtin`]), which is the report its step keeps and, where the run
//! stops there, the line under that one.
//!
//! The line is core's own — nobody typed it — and it is kept as text, the way every comment is. So it is
//! worded when it is written, in the language the settings name then, and left as it was when they
//! change: the GUI cannot word it again when it shows it, and the CLI and an agent read the same text a
//! person does.
//!
//! The words are the GUI's (`auto.say.*`), baked into [`crate::notify_wording_table`] beside the
//! notifications' by `scripts/gen-notify-wording.mjs` — a notification is the other thing core writes
//! for a person to read and cannot word again later, and it is put together the same way
//! ([`crate::notify_wording`]). A key a language has not translated is said in English, one key at a
//! time.

use crate::notify_wording_table::{Wording, WORDINGS};

/// The language that is always complete, and the one a key nobody translated is said in.
const FALLBACK: &str = "en";

/// **How far the run got** — the second sentence of the line.
#[derive(Clone, Copy, Debug)]
pub enum Reached<'a> {
    /// It was on this step, by the name the step goes by.
    Step(&'a str),
    /// It was on a step that has since been taken out of the picture.
    Gone,
    /// It stopped before it opened any step.
    Nothing,
}

/// The line: what ended the run (`why`, one of the `auto.say.*` keys the endings are worded under), how
/// far it got, and the run's id.
pub fn stopped(language: &str, why: &str, reached: Reached<'_>, run: i64) -> String {
    let reached = match reached {
        Reached::Step(step) => say(language, "reached").replace("{step}", step),
        Reached::Gone => say(language, "reachedGone").to_string(),
        Reached::Nothing => say(language, "reachedNothing").to_string(),
    };
    say(language, "line")
        .replace("{why}", say(language, why))
        .replace("{reached}", &reached)
        .replace("{run}", &run.to_string())
}

/// **What a built-in says of how it finished** — the sentence under `auto.say.bi.<key>`, with each
/// `{slot}` filled from `slots`. The values go in as they are: a task's title or a path is nobody's to
/// translate.
pub fn builtin(language: &str, key: &str, slots: &[(&str, &str)]) -> String {
    let key = format!("bi.{key}");
    slots.iter().fold(say(language, &key).to_string(), |said, (slot, value)| {
        said.replace(&format!("{{{slot}}}"), value)
    })
}

/// One sentence, in `language` if it has one and in English if not. A key English lacks too is a
/// mistake in this file, and says itself rather than leaving a hole.
fn say<'k>(language: &str, key: &'k str) -> &'k str {
    lookup(language, key).or_else(|| lookup(FALLBACK, key)).unwrap_or(key)
}

fn lookup(language: &str, key: &str) -> Option<&'static str> {
    let row: &Wording = WORDINGS.iter().find(|row| row.language == language)?;
    row.runs.binary_search_by(|(k, _)| k.cmp(&key)).ok().map(|i| row.runs[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LANGUAGES;

    /// Every key the line is built from. The ending keys are the ones [`crate::ops::automation_stop`]
    /// names; a key missing here would print itself at the reader.
    const KEYS: &[&str] = &[
        "crashed", "maxTimes", "noAgent", "noInput", "noWayOn", "leftTaskOpen", "halted", "canceled",
        "completed", "reached", "reachedGone", "reachedNothing", "line",
    ];

    /// Every language Amenbo is read in says every sentence of the line — a language added to the
    /// dictionary and not regenerated would write English with nothing saying so.
    #[test]
    fn every_language_amenbo_offers_says_the_whole_line() {
        for code in LANGUAGES {
            for key in KEYS {
                assert!(lookup(code, key).is_some(), "{code} has no auto.say.{key}");
            }
        }
    }

    /// The slots a sentence carries are the ones English carries, or the line prints a slot at the
    /// reader, or loses the run's id.
    #[test]
    fn every_language_keeps_the_slots_english_carries() {
        for row in WORDINGS {
            for (key, template) in row.runs {
                let Some(reference) = lookup(FALLBACK, key) else { continue };
                for slot in ["{why}", "{reached}", "{run}", "{step}"] {
                    assert_eq!(
                        template.contains(slot),
                        reference.contains(slot),
                        "{}: auto.say.{key} disagrees with English about {slot}",
                        row.language
                    );
                }
            }
        }
    }

    #[test]
    fn the_line_is_written_in_the_language_asked_for() {
        assert_eq!(
            stopped("ja", "canceled", Reached::Step("レビュー"), 7),
            "オートメーションの実行が中止されました。「レビュー」まで進みました。（実行 7）"
        );
        assert_eq!(
            stopped("en", "canceled", Reached::Nothing, 7),
            "An automation run was canceled. It stopped before opening a step. (run 7)"
        );
    }

    /// **Every built-in sentence English has, every language has too, with the same slots** — a report
    /// a language lacks is written in English, and one that lost `{task}` or `{path}` loses what the
    /// report is about.
    #[test]
    fn every_language_says_every_builtin_report_with_englishs_slots() {
        let slots = |template: &str| -> Vec<String> {
            let mut found: Vec<String> = template
                .split('{')
                .skip(1)
                .filter_map(|rest| rest.split_once('}'))
                .map(|(slot, _)| slot.to_string())
                .collect();
            found.sort();
            found.dedup();
            found
        };
        let english = WORDINGS.iter().find(|row| row.language == FALLBACK).expect("English");
        let builtin: Vec<&(&str, &str)> = english.runs.iter().filter(|(key, _)| key.starts_with("bi.")).collect();
        assert!(!builtin.is_empty(), "no auto.say.bi.* in English");
        for code in LANGUAGES {
            for (key, reference) in &builtin {
                let said = lookup(code, key).unwrap_or_else(|| panic!("{code} has no auto.say.{key}"));
                assert_eq!(slots(said), slots(reference), "{code}: auto.say.{key} disagrees with English");
            }
        }
    }

    #[test]
    fn a_builtin_report_is_filled_in_the_language_asked_for() {
        let slots = [("task", "AMB-T-7"), ("title", "直す")];
        assert_eq!(builtin("ja", "took", &slots), "AMB-T-7 直す に着手しました");
        assert_eq!(builtin("en", "took", &slots), "took AMB-T-7 直す");
        assert_eq!(builtin("xx", "took", &slots), "took AMB-T-7 直す");
    }

    #[test]
    fn a_language_with_no_row_is_written_in_english() {
        assert_eq!(
            stopped("xx", "completed", Reached::Gone, 3),
            "An automation run completed. It got as far as a step that is no longer there. (run 3)"
        );
    }
}
