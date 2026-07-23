//! The one place an id becomes the string a user sees — every exposed amenbo ref is `AMB-<kind>-<n>`.
//!
//! The value is untouched: a ref renders the INTEGER primary key, which *is* the conversational number.
//! Only the spelling is namespaced.
//!
//! **Why the `AMB-` namespace.** A bare `T-<n>` collides with other trackers a user runs alongside amenbo
//! (Jira keys are free-form, single-letter ones included), so no amount of checking the store apart tells a
//! foreign `T-<n>` from ours — the numbers coincide. `AMB-` makes the ref self-declaring instead, which is
//! what lets [`crate::agent`]'s lint contract key on a pure pattern with no store to consult.
//!
//! **Why one module.** This module owns the spelling in both directions — [`render`] writes it, [`strip`] /
//! [`strip_namespace`] read it back off — so a parser can never fall out of step with what the screen shows,
//! and no surface spells a ref its own way.

/// The prefix every user-visible amenbo ref carries.
pub const NAMESPACE: &str = "AMB";

/// What an exposed ref names. The letter is display-only for every kind but [`RefKind::Task`] and
/// [`RefKind::Decision`] — those two are the conversational number spaces a user types back at us, so their
/// codes are also parsed (see [`crate::ops::task::parse_typed_ref`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefKind {
    Task,
    Decision,
    Project,
    /// A comment on a task. Its own table, its own `AUTOINCREMENT`, so its own code (`AMB-D-377`):
    /// [`RefKind::DecisionComment`] numbers independently, and the two collide at the same id.
    TaskComment,
    /// A comment on a decision — the other comment space (see [`RefKind::TaskComment`]).
    DecisionComment,
    Dimension,
    DimensionValue,
    Attachment,
}

impl RefKind {
    /// Every kind there is. A reader that must cover all of them — the lint that catches a leaked ref
    /// ([`crate::lint`]) — walks this instead of listing the codes again, so a kind cannot be spelled by
    /// the renderer and missed by the reader. Hand-written, and held to the enum by `all_holds_every_kind`.
    pub const ALL: &'static [RefKind] = &[
        RefKind::Task,
        RefKind::Decision,
        RefKind::Project,
        RefKind::TaskComment,
        RefKind::DecisionComment,
        RefKind::Dimension,
        RefKind::DimensionValue,
        RefKind::Attachment,
    ];

    /// The kind's code, as it appears between the namespace and the number.
    pub const fn code(self) -> &'static str {
        match self {
            RefKind::Task => "T",
            RefKind::Decision => "D",
            RefKind::Project => "P",
            RefKind::TaskComment => "TC",
            RefKind::DecisionComment => "DC",
            RefKind::Dimension => "DIM",
            RefKind::DimensionValue => "DIMV",
            RefKind::Attachment => "ATT",
        }
    }
}

/// Render an id as its exposed ref: `AMB-T-<n>`, `AMB-D-<n>`, …
pub fn render(kind: RefKind, id: i64) -> String {
    format!("{NAMESPACE}-{}-{id}", kind.code())
}

/// A task's ref: `AMB-T-<n>`.
pub fn task(id: i64) -> String {
    render(RefKind::Task, id)
}

/// A decision's ref: `AMB-D-<n>`.
pub fn decision(id: i64) -> String {
    render(RefKind::Decision, id)
}

/// A project's ref: `AMB-P-<n>`.
pub fn project(id: i64) -> String {
    render(RefKind::Project, id)
}

/// A task comment's ref: `AMB-TC-<n>`.
pub fn task_comment(id: i64) -> String {
    render(RefKind::TaskComment, id)
}

/// A decision comment's ref: `AMB-DC-<n>`.
pub fn decision_comment(id: i64) -> String {
    render(RefKind::DecisionComment, id)
}

/// Drop a leading `AMB-` (case-insensitive), leaving whatever followed it. Input without one comes back
/// untouched, so a caller can strip unconditionally — reading is the loose side (see [`strip`]).
pub fn strip_namespace(s: &str) -> &str {
    match s.split_once('-') {
        Some((head, rest)) if head.eq_ignore_ascii_case(NAMESPACE) => rest,
        _ => s,
    }
}

/// Drop a leading `AMB-<kind>-` (case-insensitive) for **this kind only**, leaving the bare number. Input
/// without one comes back untouched.
///
/// Kind-scoped on purpose: a ref names one number space, so `AMB-T-<n>` handed to a project resolver must
/// not quietly resolve project `<n>`. Stripping only the caller's own code makes the wrong space fail to
/// parse, which is how the resolver already treats a reference it cannot read.
pub fn strip(kind: RefKind, s: &str) -> &str {
    let rest = strip_namespace(s);
    if rest.len() == s.len() {
        return s; // no namespace, so no kind code to strip either — a bare `<n>` stays `<n>`
    }
    match rest.split_once('-') {
        Some((code, num)) if code.eq_ignore_ascii_case(kind.code()) => num,
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_every_kind_under_the_namespace() {
        assert_eq!(task(12), "AMB-T-12");
        assert_eq!(decision(12), "AMB-D-12");
        assert_eq!(project(12), "AMB-P-12");
        assert_eq!(task_comment(12), "AMB-TC-12");
        assert_eq!(decision_comment(12), "AMB-DC-12");
        assert_eq!(render(RefKind::Dimension, 3), "AMB-DIM-3");
        assert_eq!(render(RefKind::DimensionValue, 3), "AMB-DIMV-3");
        assert_eq!(render(RefKind::Attachment, 3), "AMB-ATT-3");
    }

    /// `ALL` is written by hand, so this match is what keeps it honest: it is exhaustive, so a kind added
    /// to the enum stops this test compiling. Write the arm here **and** list the kind in `ALL` — that list
    /// is where a reader that must cover every kind (the lint) gets them from.
    #[test]
    fn all_holds_every_kind() {
        for kind in RefKind::ALL {
            match kind {
                RefKind::Task
                | RefKind::Decision
                | RefKind::Project
                | RefKind::TaskComment
                | RefKind::DecisionComment
                | RefKind::Dimension
                | RefKind::DimensionValue
                | RefKind::Attachment => {}
            }
        }
        // Each code is distinct, so no two kinds can be read as one another.
        let mut codes: Vec<&str> = RefKind::ALL.iter().map(|k| k.code()).collect();
        codes.sort_unstable();
        let unique = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), unique, "two kinds share a code");
    }

    /// The point of the namespace: a foreign tracker's `T-123` never looks like ours, whatever its number.
    #[test]
    fn a_rendered_ref_is_self_declaring() {
        for kind in [RefKind::Task, RefKind::Decision, RefKind::Project, RefKind::TaskComment] {
            let rendered = render(kind, 123);
            assert!(rendered.starts_with("AMB-"), "{rendered} does not declare itself");
            assert!(!rendered.contains('#'), "{rendered} still carries a bare number");
        }
    }

    /// What we render is what we read back — the round-trip the namespace would otherwise break.
    #[test]
    fn strip_undoes_render() {
        for kind in [RefKind::Task, RefKind::Decision, RefKind::Project, RefKind::TaskComment] {
            assert_eq!(strip(kind, &render(kind, 123)), "123");
        }
        assert_eq!(strip(RefKind::Task, "amb-t-7"), "7", "the prefix folds case");
    }

    #[test]
    fn strip_leaves_a_bare_number_and_a_name_alone() {
        assert_eq!(strip(RefKind::Project, "5"), "5");
        assert_eq!(strip(RefKind::Project, "amenbo"), "amenbo");
        assert_eq!(strip(RefKind::Project, "Q3-roadmap"), "Q3-roadmap");
    }

    /// A ref from the wrong space stays unparsed, rather than resolving to that number here.
    #[test]
    fn strip_is_scoped_to_its_own_kind() {
        assert_eq!(strip(RefKind::Project, "AMB-T-5"), "AMB-T-5");
        assert_eq!(strip(RefKind::Task, "AMB-D-5"), "AMB-D-5");
        assert_eq!(strip(RefKind::Dimension, "AMB-DIMV-5"), "AMB-DIMV-5");
        // The two comment spaces are the reason the code was split (`AMB-D-377`): the same id names a row
        // in each table, so neither ref may be read as the other's — nor as the decision's it prefixes.
        assert_eq!(strip(RefKind::TaskComment, "AMB-DC-5"), "AMB-DC-5");
        assert_eq!(strip(RefKind::DecisionComment, "AMB-TC-5"), "AMB-TC-5");
        assert_eq!(strip(RefKind::Decision, "AMB-DC-5"), "AMB-DC-5");
    }
}
