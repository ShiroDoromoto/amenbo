//! Turns a doctor issue into **one English sentence** — the CLI's face. All core hands back is a
//! [`DoctorIssueKind`] (the template id) and `params` (what differs); each face writes the sentence
//! a reader actually sees. The GUI is the only face a human reads directly, so that one is
//! localized and the CLI stays English-only. Hence the sentences here are English and the fixes
//! they suggest are **CLI commands** — the GUI renders the same kinds in its own language, pointing
//! at its own affordances (`app/src/core/i18n/locales/`). Templates map one-to-one onto kinds, with the
//! exhaustive `match` enforced by the compiler: add a kind in core and the build stays broken until
//! this face writes its sentence, so nothing ships with the English missing.

use amenbo_core::config::Paths;
use amenbo_core::validate::{DoctorIssue, DoctorIssueKind};

/// A value out of `params`. Core's [`DoctorIssue::new`] checks the keys are exactly the expected
/// ones, so nothing can be missing here — and if it somehow were, fall back to `?` and still print
/// a readable sentence rather than panicking in the face.
fn p<'a>(issue: &'a DoctorIssue, key: &str) -> &'a str {
    issue.params.get(key).map(String::as_str).unwrap_or("?")
}

/// What is broken.
pub fn message(issue: &DoctorIssue) -> String {
    let (project, dep, path, dir) =
        (p(issue, "project"), p(issue, "dep"), p(issue, "path"), p(issue, "dir"));
    match issue.kind {
        DoctorIssueKind::SelfDependency => {
            format!("Dependency {dep} points at itself (task_id == blocked_by_id).")
        }
        DoctorIssueKind::DuplicateOrderKey => format!(
            "Project {project} has tasks sharing the order key '{}'.",
            p(issue, "order_key")
        ),
        DoctorIssueKind::StaleManagedBlock => format!(
            "The Amenbo managed block in {path} is stale (v{} < v{}): a binary update changed the template.",
            p(issue, "version"),
            p(issue, "current"),
        ),
        DoctorIssueKind::LegacyPointer | DoctorIssueKind::LegacyPointerAmbiguous => format!(
            "{path} is an old-format pointer (its project id is not the current integer key). \
             Nothing rewrites the `.amenbo` files scattered across folders on your behalf."
        ),
        DoctorIssueKind::MissingPointer => format!(
            "{dir} is recorded as a folder bound to project #{project}, but it has no `.amenbo`: \
             an AI started there does not resolve to that project."
        ),
        DoctorIssueKind::MissingPointerAmbiguous => format!(
            "{dir} is recorded as a bound folder ({}), but it has no `.amenbo`: an AI started there \
             does not resolve to a project.",
            p(issue, "claims"),
        ),
        DoctorIssueKind::OrphanBinding => format!(
            "{dir} is recorded as a bound folder, but no live project claims it \
             (an index leftover from a deleted project)."
        ),
        DoctorIssueKind::DeadRef => format!(
            "The body at {} points at {}: refs that resolve to nothing, so a reader sent after one \
             finds nothing there.",
            p(issue, "at"),
            p(issue, "refs"),
        ),
        DoctorIssueKind::StartAfterDue => format!(
            "Task {} is declared to start on {} but was due on {}: it stays out of the mailbox until \
             a day that has already passed its deadline.",
            p(issue, "task"),
            p(issue, "start_on"),
            p(issue, "due_on"),
        ),
        DoctorIssueKind::ProjectWithoutFolder => format!(
            "Project #{project} '{}' has no folder on this machine leading to it: there is nowhere to \
             operate it from, and an AI cannot reach it at all.",
            p(issue, "name"),
        ),
        DoctorIssueKind::UnwiredFolder => format!(
            "{dir} does not start its AI on Amenbo: {} is set up here and is not wired to run it at \
             session start, so an AI reads the guidance only if it goes looking.",
            p(issue, "tools"),
        ),
        DoctorIssueKind::UnwiredFolderAmbiguous => format!(
            "{dir} does not start its AI on Amenbo, and shows no sign of which tool is used there, so \
             nothing can say which wiring is missing."
        ),
    }
}

/// How to fix it — in terms of **what the CLI offers**.
pub fn fix_hint(issue: &DoctorIssue) -> String {
    let project = p(issue, "project");
    let cmd = Paths::command_name();
    match issue.kind {
        DoctorIssueKind::SelfDependency => format!("Drop the edge with `{cmd} task undepend`."),
        DoctorIssueKind::DuplicateOrderKey => format!(
            "Re-order the tasks (`{cmd} task move <task> --top/--bottom`) and the duplicate is gone."
        ),
        DoctorIssueKind::StaleManagedBlock => format!(
            "Run {cmd} in that folder and the block follows this binary on its own; \
             `{cmd} sync-guide` does every bound folder at once (one folder: `--dir`)."
        ),
        DoctorIssueKind::LegacyPointer => format!(
            "Run {cmd} in that folder and the pointer rewrites itself to the current format \
             (project #{project}); explicitly: `{cmd} bind --project {project}`."
        ),
        DoctorIssueKind::MissingPointer => format!(
            "Run `{cmd} init` in that folder to restore the pointer; explicitly: \
             `{cmd} bind --project {project}`."
        ),
        DoctorIssueKind::LegacyPointerAmbiguous | DoctorIssueKind::MissingPointerAmbiguous => {
            format!(
                "The binding does not resolve to a single project - pick one: \
                 `{cmd} bind --project <name or id>`."
            )
        }
        DoctorIssueKind::OrphanBinding => format!(
            "`{cmd} doctor --fix` forgets it from the index (neither the folder nor its `.amenbo` is touched)."
        ),
        DoctorIssueKind::DeadRef => format!(
            "Edit the body it names - `{cmd} task update <task> --notes` for a task's notes, \
             `{cmd} decision edit <decision> --body` for a proposed decision - and drop the ref, or point \
             it at what stands in its place. Nothing rewrites a body on your behalf - only a person knows \
             what it meant to say."
        ),
        DoctorIssueKind::StartAfterDue => format!(
            "Correct whichever of the two declarations is wrong: `{cmd} task update <task> --start <date>` \
             or `--due <date>`. Nothing picks a winner between them on your behalf - a start day that \
             falls after the deadline could mean either one was mistyped."
        ),
        DoctorIssueKind::ProjectWithoutFolder => format!(
            "Link a folder to it: `{cmd} bind --project <name or id>` in the folder you work in. \
             Archive it instead (`{cmd} project archive`) if you have stopped working on it - an \
             archived project is not asked for a folder."
        ),
        DoctorIssueKind::UnwiredFolder | DoctorIssueKind::UnwiredFolderAmbiguous => format!(
            "`{cmd} agent-hook snippet <tool>` prints the text that asks an AI to wire it; give that \
             text to the AI you run in the folder. Amenbo writes no settings file on your behalf, so \
             nothing here ends until the wiring lands in theirs."
        ),
    }
}

/// How many of a kind's issues the terminal lists before it stops naming them one by one.
///
/// A real store, measured: 411 dead refs — 824 lines, and 411 copies of one fix hint. A screen that
/// long is not read, and burying a lone `self_dependency` under it is how the terminal stops being a face at
/// all. The cap is per kind, so a rare issue is never crowded out by a common one, and the count and the
/// remainder are always printed: the reader is told exactly what was withheld and where the whole list is.
const HUMAN_LIST_CAP: usize = 10;

/// Write the issues out for a terminal: **grouped by kind**, capped, with the fix hint once per group.
///
/// The grouping is what the shape already says — a fix hint belongs to the kind, not to the issue, so
/// printing it per issue was always a copy. Only the human face folds like this; `--json` carries every
/// issue, whole and ungrouped, because a machine reader is not the one drowning.
pub fn print_grouped(issues: &[DoctorIssue], mut out: impl FnMut(String)) {
    for kind in DoctorIssueKind::ALL {
        let of_kind: Vec<&DoctorIssue> = issues.iter().filter(|i| i.kind == *kind).collect();
        let Some(first) = of_kind.first() else { continue };
        out(format!("  [{}] {} ({}):", first.severity, kind.as_str(), of_kind.len()));
        for issue in of_kind.iter().take(HUMAN_LIST_CAP) {
            out(format!("    {}", message(issue)));
        }
        if let Some(withheld) = of_kind.len().checked_sub(HUMAN_LIST_CAP).filter(|n| *n > 0) {
            out(format!(
                "    … and {withheld} more (the full list is in `{} doctor --json`)",
                Paths::command_name()
            ));
        }
        out(format!("      → {}", fix_hint(first)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(kind: DoctorIssueKind, target: &str) -> DoctorIssue {
        let params: Vec<(&str, &str)> = kind.param_keys().iter().map(|k| (*k, target)).collect();
        DoctorIssue::new(kind, target, &params)
    }

    /// A common issue must not push a rare one off the screen, and what was withheld is said out loud —
    /// a list that silently stops reads as a list that ended.
    #[test]
    fn a_long_list_is_capped_per_kind_and_says_what_it_withheld() {
        let mut issues: Vec<DoctorIssue> =
            (0..30).map(|n| issue(DoctorIssueKind::DeadRef, &format!("task:{n}"))).collect();
        issues.push(issue(DoctorIssueKind::SelfDependency, "task_dependency:1"));

        let mut lines = Vec::new();
        print_grouped(&issues, |l| lines.push(l));
        let out = lines.join("\n");

        assert!(out.contains("[error] self_dependency (1):"), "the rare kind survives the flood: {out}");
        assert!(out.contains("… and 20 more"), "the remainder is named: {out}");
        assert_eq!(
            lines.iter().filter(|l| l.contains("Edit the body it names")).count(),
            1,
            "the fix hint belongs to the kind, so it is written once: {out}",
        );
        assert!(out.lines().count() < 20, "a capped report stays a screenful: {out}");
    }

    /// A kind with no issues is not a heading over nothing.
    #[test]
    fn a_kind_nothing_raised_is_not_printed() {
        let mut lines = Vec::new();
        print_grouped(&[issue(DoctorIssueKind::OrphanBinding, "/tmp/x")], |l| lines.push(l));

        assert_eq!(lines.iter().filter(|l| l.contains("dead_ref")).count(), 0, "{lines:?}");
    }

    /// The CLI face is English-only: no kind's sentence may carry a non-ASCII byte.
    /// It also checks the template really substitutes its `params` — no `{...}` hole left behind.
    #[test]
    fn every_issue_reads_as_english_with_its_params_filled_in() {
        for kind in DoctorIssueKind::ALL {
            let params: Vec<(&str, &str)> = kind
                .param_keys()
                .iter()
                .map(|k| (*k, "X7"))
                .collect();
            let issue = DoctorIssue::new(*kind, "target", &params);
            for text in [message(&issue), fix_hint(&issue)] {
                assert!(
                    text.is_ascii(),
                    "{}: the CLI face is English-only, got {text:?}",
                    kind.as_str()
                );
                assert!(
                    !text.contains('{') && !text.contains('}'),
                    "{}: an unfilled placeholder is left in {text:?}",
                    kind.as_str()
                );
            }
            assert!(
                message(&issue).contains("X7"),
                "{}: the message never mentions what is broken",
                kind.as_str()
            );
        }
    }
}
