//! Write reach — an AI facet must not **mutate** an entity outside the project it is bound to.
//!
//! The read side ([`super::read`]) is closed at two entry points: [`Reach::narrow`] for listings and
//! [`Reach::check`] for reference resolution. Every path that resolves a ref before mutating
//! (`task status`, `comment add`, `decision amend`, …) is therefore already closed — resolution rejects
//! anything out of reach. What is left are the two kinds of **mutation that never passes through a ref**,
//! and thus through no `resolve_*_ref` at all:
//!
//! - **Creating a new entity** (`task add` / `decision add` / `project add`) — there is no id yet, so we
//!   check the place it would go (the project) instead.
//! - **Taking an id directly** (comment ids, attachment ids, dimension / dimension-value ids) — these are
//!   not conversational refs, so nothing resolves them.
//!
//! The guard is not sprinkled across commands: it sits at the **single write entry point**
//! (`Store::write_one`). That entry point **demands the declaration of what is being mutated**
//! ([`WriteTarget`]) as an argument, so a new write wrapper cannot slip past the guard — omit the
//! declaration and it does not compile. The check runs inside the transaction and **before** the
//! mutation, so we never answer "the write went through, and here is an error".
//!
//! Resolving *which* project owns an entity (comment → task → project, attachment → target → project, …)
//! does not live here: the read side walks exactly the same path, so it lives in [`super::owner`]. With
//! one lookup shared by both sides, adding an entity type closes both at once.
//!
//! Only entities that belong to a project are in scope here. The store-wide surfaces that have no project
//! boundary (config, the binding registry, diagnostics / export) sit outside this guard.
//!
//! The declaration earns a second keep beyond the guard: [`projects_of`] reads the **projects a mutation
//! touches** off the same [`WriteTarget`]s, which is what moves each one's sync version (`AMB-D-582`).
//! The two questions differ — *may this write happen* and *whose version does it move* — but they are put
//! to the same declaration, so a write path that cannot forget the one cannot forget the other either.

use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::model::AttachmentTarget;
use crate::reach::{Closed, Reach};

use super::owner;

/// What this mutation touches. The owning project is looked up from it — and **an id that does not exist
/// has no owner**, so a narrowed reach cannot touch it (the same discipline as [`Reach::check`] on the
/// read side).
#[derive(Clone, Copy, Debug)]
pub(super) enum WriteTarget {
    Task(i64),
    Decision(i64),
    Project(i64),
    TaskComment(i64),
    DecisionComment(i64),
    Dimension(i64),
    DimensionValue(i64),
    Attachment(i64),
    /// An attachment's target (the polymorphic `target_type` + id).
    AttachTo(AttachmentTarget, i64),
    /// Where an entity about to be created would go (`None` = in no project at all).
    NewIn(Option<i64>),
    /// A new project itself. It is **always** outside a narrowed reach.
    NewProject,
}

/// Check the reach before mutating (out of reach ⇒ `out_of_reach`). Under `All` nothing is looked up, so
/// humans, the GUI and library use pay nothing for this.
pub(super) fn guard(conn: &Connection, reach: Reach, targets: &[WriteTarget]) -> Result<()> {
    let Some(bound) = reach.project() else {
        return Ok(());
    };
    for target in targets {
        check(conn, reach, bound, *target)?;
    }
    Ok(())
}

/// **The projects this mutation touches**, read off the very same declaration the guard above reads —
/// which is what makes the sync version (`AMB-D-582`) something a new write path cannot forget to move:
/// [`WriteTarget`] is already an argument it cannot omit.
///
/// Resolved **before** the mutation, like the guard, and that is not a convenience: a row about to be
/// deleted can still name its project, and a re-homing declares both ends (`WriteTarget::Task` for the
/// project it leaves, `WriteTarget::NewIn` for the one it lands in), so both move. Asked afterwards,
/// neither could be answered.
///
/// Unlike the guard this runs under every reach — the version is the store's own bookkeeping, not a
/// containment check, so a human's write moves it exactly as an AI's does. What it costs is the same
/// handful of indexed lookups [`owner`] does for the guard.
///
/// A target that belongs to no project ([`WriteTarget::NewProject`], an inbox task) contributes nothing:
/// there is no project whose version could move.
pub(super) fn projects_of(
    conn: &Connection,
    targets: &[WriteTarget],
) -> Result<std::collections::BTreeSet<i64>> {
    let mut projects = std::collections::BTreeSet::new();
    for target in targets {
        if let Some(project) = project_of(conn, *target)? {
            projects.insert(project);
        }
    }
    Ok(projects)
}

/// The project one target belongs to — the same walk [`check`] narrows on, without the judgement.
fn project_of(conn: &Connection, target: WriteTarget) -> Result<Option<i64>> {
    match target {
        WriteTarget::Task(id) => owner::task(conn, id),
        WriteTarget::Decision(id) => owner::decision(conn, id),
        WriteTarget::Project(id) => Ok(Some(id)),
        WriteTarget::TaskComment(id) => owner::task_comment(conn, id),
        WriteTarget::DecisionComment(id) => owner::decision_comment(conn, id),
        WriteTarget::Dimension(id) => owner::dimension(conn, id),
        WriteTarget::DimensionValue(id) => owner::dimension_value(conn, id),
        WriteTarget::Attachment(id) => owner::attachment(conn, id),
        WriteTarget::AttachTo(kind, id) => owner::attach_target(conn, kind, id),
        WriteTarget::NewIn(project) => Ok(project),
        // The project does not exist yet, so it has no version to move. Its first version is `0` — the
        // absent row — and the first write that names it carries it forward from there.
        WriteTarget::NewProject => Ok(None),
    }
}

fn check(conn: &Connection, reach: Reach, bound: i64, target: WriteTarget) -> Result<()> {
    match target {
        WriteTarget::Task(id) => reach.check(&crate::idref::task(id), owner::task(conn, id)?),
        WriteTarget::Decision(id) => reach.check(&crate::idref::decision(id), owner::decision(conn, id)?),
        WriteTarget::Project(id) => reach.check(&crate::idref::project(id), Some(id)),
        WriteTarget::TaskComment(id) => {
            reach.check(&crate::idref::task_comment(id), owner::task_comment(conn, id)?)
        }
        WriteTarget::DecisionComment(id) => {
            reach.check(&crate::idref::decision_comment(id), owner::decision_comment(conn, id)?)
        }
        WriteTarget::Dimension(id) => {
            reach.check(&crate::idref::render(crate::idref::RefKind::Dimension, id), owner::dimension(conn, id)?)
        }
        WriteTarget::DimensionValue(id) => {
            reach.check(&crate::idref::render(crate::idref::RefKind::DimensionValue, id), owner::dimension_value(conn, id)?)
        }
        WriteTarget::Attachment(id) => {
            reach.check(&crate::idref::render(crate::idref::RefKind::Attachment, id), owner::attachment(conn, id)?)
        }
        WriteTarget::AttachTo(kind, id) => reach.check(
            &owner::attach_target_ref(kind, id),
            owner::attach_target(conn, kind, id)?,
        ),
        // A new entity has no id yet, so we check the place it would go. "No project" (the inbox) is
        // outside a narrowed reach — nobody should be able to create an entity they can no longer touch.
        WriteTarget::NewIn(Some(project)) => reach.check(&crate::idref::project(project), Some(project)),
        WriteTarget::NewIn(None) => Err(cannot_create(reach, bound, "outside any project")),
        // A new project is by definition outside the binding: it could be created but never touched
        // again. We do not leave that asymmetry standing.
        WriteTarget::NewProject => Err(cannot_create(reach, bound, "a new project")),
    }
}

/// The wording for a creation that is out of reach. It says "you cannot create it there", never "it does
/// not exist" (the same discipline as [`crate::reach`]) — and, as on the read side, it names whatever
/// closed the reach, since that is what the writer can do something about.
fn cannot_create(reach: Reach, bound: i64, en_what: &str) -> Error {
    let bound = crate::idref::project(bound);
    Error::out_of_reach(match reach {
        Reach::Project { closed_by: Closed::Window, .. } => format!(
            "Creating {en_what} is outside project {bound}, the project this plugin was launched to \
             observe — a plugin writes only inside the window it fires in, which no argument widens."
        ),
        _ => format!(
            "Creating {en_what} is outside project {bound}, the project this folder is bound to — an AI \
             reaches only the project its .amenbo names. Ask a human to run this."
        ),
    })
}
