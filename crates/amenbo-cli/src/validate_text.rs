//! Turns a validate issue into **one English sentence** — the CLI's face. All core hands back is an
//! [`IssueRule`] (the template id) and what differs (target / field); the face writes the sentence a
//! reader actually sees, laid out exactly like doctor's `doctor_text`. validate has no face but the
//! CLI, so English alone covers it (the GUI is the only face a human reads directly, and that one is
//! localized; the CLI stays English-only). Templates map one-to-one onto rules, with the exhaustive
//! `match` enforced by the compiler: add a rule in core and the build stays broken until this face
//! writes its sentence, so nothing ships with the English missing.

use amenbo_core::config::Paths;
use amenbo_core::validate::{Issue, IssueRule};

/// How to fix it — as a **CLI command**, phrased so that whoever reads `--json` (an AI) can run it
/// as-is.
pub fn fix_hint(issue: &Issue) -> String {
    // target comes as `task:<id>` (validate only ever looks at tasks). What we hand back is a
    // command someone can type, so substitute the bare id with the prefix stripped off.
    let id = issue.target.strip_prefix("task:").unwrap_or(&issue.target);
    match issue.rule {
        IssueRule::Required => format!(
            "Give it a {}: `{} task update {id} --title \"...\"`.",
            issue.field,
            Paths::command_name()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(rule: IssueRule) -> Issue {
        Issue {
            target: "task:7".to_string(),
            field: "title".to_string(),
            rule,
            severity: "error".to_string(),
            got: String::new(),
            expected: "non-empty string".to_string(),
        }
    }

    /// The CLI face is English-only: no rule's sentence may carry a non-ASCII byte.
    /// It also checks the template really substitutes what differs (target / field).
    #[test]
    fn every_issue_reads_as_english_with_its_target_filled_in() {
        for rule in IssueRule::ALL {
            let text = fix_hint(&issue(*rule));
            assert!(text.is_ascii(), "{}: the CLI face is English-only, got {text:?}", rule.as_str());
            assert!(
                text.contains('7') && !text.contains("task:7"),
                "{}: the fix hint must name the task as a command argument, got {text:?}",
                rule.as_str()
            );
        }
    }
}
