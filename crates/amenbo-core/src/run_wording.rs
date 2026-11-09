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

use crate::error::Error;
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

/// **What a failed run's filed task says, in its notes** — everything a person or a role it reaches
/// needs to judge what happened (`AMB-D-1139`).
pub struct Failure<'a> {
    /// What ended the run — one of the `auto.say.*` keys the endings are worded under.
    pub why: &'a str,
    /// What the program of the step it stopped on exited with, where that was heard.
    pub exit_code: Option<i64>,
    /// The step it stopped on.
    pub reached: Reached<'a>,
    /// Where that step comes in the steps the run opened, counting from one.
    pub step_no: usize,
    /// The report that step left. `None` where it left none.
    pub report: Option<&'a str>,
    pub run: i64,
    /// The task the run was working. `None` for a run that held no task.
    pub task: Option<FailedTask<'a>>,
}

/// The task a failed run was working, and how many times in a row it has stopped, this time counted.
pub struct FailedTask<'a> {
    pub id: i64,
    pub title: &'a str,
    pub streak: usize,
}

/// The notes of a failed run's filed task: a heading naming the task, then one item each for why it
/// stopped, the step, that step's report, the run, the task, and how many times in a row that task has
/// stopped. A report of more than one line is set under its item, indented, so the list holds.
pub fn failure(language: &str, failure: &Failure<'_>) -> String {
    let item = |label: &str, value: &str| -> String {
        let line = say(language, "fail.item").replace("{label}", say(language, label));
        let value = value.trim_end();
        if !value.contains('\n') {
            return format!("- {}", line.replace("{value}", value.trim_start()));
        }
        let indented: Vec<String> =
            value.lines().map(|l| if l.is_empty() { String::new() } else { format!("  {l}") }).collect();
        format!("- {}\n\n{}", line.replace("{value}", "").trim_end(), indented.join("\n"))
    };
    let why = match failure.exit_code {
        Some(code) => say(language, "fail.exitCode")
            .replace("{why}", say(language, failure.why))
            .replace("{code}", &code.to_string()),
        None => say(language, failure.why).to_string(),
    };
    let step = match failure.reached {
        Reached::Step(step) => say(language, "fail.stepAt")
            .replace("{n}", &failure.step_no.to_string())
            .replace("{step}", step),
        Reached::Gone => say(language, "reachedGone").to_string(),
        Reached::Nothing => say(language, "reachedNothing").to_string(),
    };
    let report = failure.report.unwrap_or(say(language, "fail.noReport"));
    let mut items = vec![
        item("fail.why", &why),
        item("fail.step", &step),
        item("fail.report", report),
        item("fail.run", &say(language, "fail.runId").replace("{run}", &failure.run.to_string())),
    ];
    let mut heading = None;
    if let Some(task) = &failure.task {
        let named = format!("AMB-T-{}", task.id);
        heading = Some(say(language, "fail.heading").replace("{task}", &named).replace("{title}", task.title));
        items.push(item("fail.task", &format!("{named} {}", task.title)));
        items.push(item("fail.streak", &say(language, "fail.times").replace("{n}", &task.streak.to_string())));
    }
    let list = items.join("\n");
    match heading {
        Some(heading) => format!("{heading}\n\n{list}"),
        None => list,
    }
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

/// **What a test run puts where the work would have been** ([`crate::ops::automation_rehearse`]) — the
/// sentence under `auto.say.test.<key>`, with each `{slot}` filled from `slots` as [`builtin`] fills its.
pub fn rehearsal(language: &str, key: &str, slots: &[(&str, &str)]) -> String {
    let key = format!("test.{key}");
    slots.iter().fold(say(language, &key).to_string(), |said, (slot, value)| {
        said.replace(&format!("{{{slot}}}"), value)
    })
}

/// **A refusal a built-in was turned away by, in the reader's language** — the screen's template for its
/// code (`err`, the one `errLabel` writes a refusal from) with its fields filled, and its parts, where it
/// was composed of some, each from its own template and joined with `reasonSep`. A refusal whose code
/// holds no template is said the way the CLI says it: in English, since nothing finer can be told of it.
pub fn error(language: &str, error: &Error) -> String {
    let Some(template) = refusal(language, error.code()) else {
        return error.to_string();
    };
    let said = fill(template, error.fields().map(|f| f.iter()).into_iter().flatten());
    if error.parts().is_empty() {
        return said;
    }
    let reasons: Vec<String> = error
        .parts()
        .iter()
        .map(|part| match part.code().and_then(|code| refusal(language, code.as_str())) {
            Some(template) => fill(template, part.fields().iter()),
            None => part.en().to_string(),
        })
        .collect();
    let sep = refusal(language, "reasonSep").unwrap_or("; ");
    said.replace("{reasons}", &reasons.join(sep))
}

/// The screen's template for one refusal's code, in `language` if it has one and in English if not.
fn refusal(language: &str, code: &str) -> Option<&'static str> {
    let find = |language: &str| {
        let row: &Wording = WORDINGS.iter().find(|row| row.language == language)?;
        row.errs.binary_search_by(|(k, _)| k.cmp(&code)).ok().map(|i| row.errs[i].1)
    };
    find(language).or_else(|| find(FALLBACK))
}

/// A template with each `{field}` it names filled; one nobody sent stays as it is, as on the screen.
fn fill<'v>(template: &str, fields: impl Iterator<Item = (&'static str, &'v str)>) -> String {
    fields.fold(template.to_string(), |said, (key, value)| said.replace(&format!("{{{key}}}"), value))
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
        "crashed", "maxTimes", "noAgent", "noInput", "noWayOn", "leftTaskOpen", "failedCheck", "halted",
        "canceled", "completed", "reached", "reachedGone", "reachedNothing", "line", "fail.heading",
        "fail.item", "fail.why", "fail.exitCode", "fail.step", "fail.stepAt", "fail.report", "fail.noReport",
        "fail.run", "fail.runId", "fail.task", "fail.streak", "fail.times",
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
                for slot in
                    ["{why}", "{reached}", "{run}", "{step}", "{label}", "{value}", "{task}", "{title}", "{n}", "{code}"]
                {
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

    /// **A refusal is said from the screen's template for its code**, in the language asked for, with
    /// its values filled — and one whose code holds no template is said the way the CLI says it.
    #[test]
    fn a_refusal_is_said_from_the_screens_template_for_its_code() {
        use crate::error::{ErrorCode, Msg};
        let closed = Error::Invalid(
            Msg::new("'分類' names the value '済', which is closed")
                .coded(ErrorCode::InvalidMakeTaskValueClosed)
                .with("value", "済"),
        );
        assert_eq!(error("ja", &closed), "「分類」が指す値「済」は閉じています");
        assert_eq!(error("en", &closed), "“Classification” names the value “済”, which is closed");
        let plain = Error::invalid("nothing names this one");
        assert_eq!(error("ja", &plain), plain.to_string());
    }

    #[test]
    fn the_line_is_written_in_the_language_asked_for() {
        assert_eq!(
            stopped("ja", "canceled", Reached::Step("レビュー"), 7),
            "オートメーションの実行がキャンセルされました。「レビュー」まで進みました。（実行 7）"
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

    /// **A failed run's notes name every part of what happened**, in the order the screen shows them,
    /// in the language asked for — and a report of several lines is set under its item.
    #[test]
    fn a_failure_is_written_item_by_item_in_the_language_asked_for() {
        let task = FailedTask { id: 150, title: "BGM を作る", streak: 3 };
        let failure = Failure {
            why: "crashed",
            exit_code: Some(2),
            reached: Reached::Step("書き出す"),
            step_no: 4,
            report: Some("export.sh: sox が見つかりません"),
            run: 540,
            task: Some(task),
        };
        assert_eq!(
            super::failure("ja", &failure),
            format!(
                "止まりました：AMB-T-150 BGM を作る\n\n\
                 - 止まった理由：{}（終了コード 2）\n\
                 - 止まったステップ：4. 書き出す\n\
                 - そのステップの報告：export.sh: sox が見つかりません\n\
                 - 実行の番号：#540\n\
                 - 元のタスク：AMB-T-150 BGM を作る\n\
                 - 続けて止まった回数：3 回目",
                say("ja", "crashed"),
            )
        );
        let bare = Failure { exit_code: None, reached: Reached::Nothing, report: Some("one\ntwo"), task: None, ..failure };
        assert_eq!(
            super::failure("en", &bare),
            format!(
                "- Why it stopped: {}\n- The step it stopped on: It stopped before opening a step.\n\
                 - That step's report:\n\n  one\n  two\n- The run: #540",
                say("en", "crashed"),
            )
        );
    }

    #[test]
    fn a_language_with_no_row_is_written_in_english() {
        assert_eq!(
            stopped("xx", "completed", Reached::Gone, 3),
            "An automation run completed. It got as far as a step that is no longer there. (run 3)"
        );
    }
}
