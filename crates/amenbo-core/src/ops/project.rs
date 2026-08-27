//! Project operations.
//!
//! **Writes are engine SQL, directly.** Each mutator takes a [`WriteTx`] (`BEGIN IMMEDIATE`) opened by the
//! caller, and does its reads (the `before` snapshot, the siblings' `order_key`s, the set of slugs already
//! taken) and its writes **inside that one transaction**. The transaction is opened by the write wrappers
//! on [`crate::Store`] (`project_add` / `project_update` / …), and the CLI and GUI call nothing else.

use crate::error::{Error, ErrorCode, Result};
use crate::model::{Project, View};
use crate::ops::{emit_create, emit_update, place, Noun, Position};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// The word for this entity (the English/Japanese pair a `not_found` message is phrased with).
pub(crate) const NOUN: Noun = Noun { en: "project", code: ErrorCode::NotFoundProject };

pub struct NewProject {
    pub name: String,
    pub view: View,
    pub notes: String,
    pub color: Option<String>,
}

/// Create a project. **There are two read-then-writes here** — the siblings' `order_key`s, and the set of
/// slugs already taken. Both are read inside the same `BEGIN IMMEDIATE` transaction as the write: read
/// them outside it and two concurrent writers see the same set, **take the same slug**, and the
/// `project_by_slug` unique index throws out whichever commits second.
pub fn add(tx: &WriteTx<'_>, input: NewProject) -> Result<Project> {
    if input.name.trim().is_empty() {
        return Err(Error::invalid("a project name cannot be empty"));
    }
    let sibs = read::project_siblings(tx.conn(), None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let slug = crate::slug::unique(&read::taken_project_slugs(tx.conn())?, &input.name);
    let now = Timestamp::now();
    let project = Project {
        id: read::next_id(tx.conn(), "project")?,
        name: input.name,
        notes: input.notes,
        color: input.color,
        default_view: input.view,
        archived: false,
        order_key,
        // A human-readable identifier derived from the name, unique on this device. It does not follow a
        // later rename: `.amenbo` carries it as matching material, so a slug that moves is a match that
        // means nothing (the id is what is authoritative).
        slug: Some(slug),
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::project(&project))?;
    Ok(project)
}

/// Read the `before` snapshot of a live project **from this transaction**.
fn live_before(tx: &WriteTx<'_>, id: i64) -> Result<Project> {
    read::project(tx.conn(), id)?.ok_or_else(|| NOUN.not_found(id.to_string()))
}

#[derive(Default)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub notes: Option<String>,
    pub view: Option<View>,
    pub color: Option<String>,
}

pub fn update(tx: &WriteTx<'_>, id: i64, patch: ProjectPatch) -> Result<Project> {
    let before = live_before(tx, id)?;
    let mut p = before.clone();
    if let Some(name) = patch.name {
        if name.trim().is_empty() {
            return Err(Error::invalid("a project name cannot be empty"));
        }
        p.name = name;
    }
    if let Some(notes) = patch.notes {
        p.notes = notes;
    }
    if let Some(view) = patch.view {
        p.default_view = view;
    }
    if let Some(color) = patch.color {
        p.color = Some(color);
    }
    p.updated_at = Timestamp::now();
    emit_update(tx, record::project(&before), record::project(&p))?;
    Ok(p)
}

/// Reorder a project. The siblings are read inside the same transaction as the write.
pub fn move_to(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<Project> {
    let sibs = read::project_siblings(tx.conn(), Some(id))?;
    let key = place(&sibs, &pos)?;
    let before = live_before(tx, id)?;
    let after = Project { order_key: key, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::project(&before), record::project(&after))?;
    Ok(after)
}

pub fn set_archived(tx: &WriteTx<'_>, id: i64, archived: bool) -> Result<Project> {
    let before = live_before(tx, id)?;
    let after = Project { archived, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::project(&before), record::project(&after))?;
    Ok(after)
}

/// Physically delete a project, **subtree and all** — its tasks, decisions and dimensions, each with its
/// own children (comments, dependency edges, links, attachments, dimension values, assignments) deleted
/// first, and only then the project row itself. (Keeping a project around but out of the way, without
/// destroying anything, is what archiving is for — [`set_archived`].) Children go first because the schema
/// insists: `task` / `decision` / `dimension`.`project_id` are all `RESTRICT`, so deleting a project out
/// from under a surviving child fails loudly rather than orphaning it quietly. This is the **row level**
/// of the delete only; the destructive teardown that follows — releasing the folders bound to the project,
/// and reclaiming the attachment bytes nothing references any more — belongs to the layer above
/// ([`crate::project_teardown::teardown_deleted_project`]). The subtree commits as one transaction: cut it
/// halfway and live tasks are left stranded in a project that no longer exists. The subtree is read
/// **inside that same transaction** too — read it outside and a task another writer added in between is
/// missed.
pub fn delete(tx: &WriteTx<'_>, id: i64) -> Result<Vec<String>> {
    let project_before = live_before(tx, id)?;
    let mut orphaned = Vec::new();
    for task_id in read::task_ids_in_project(tx.conn(), id)? {
        orphaned.extend(crate::ops::task::delete_subtree(tx, task_id)?);
    }
    for decision_id in read::decision_ids_in_project(tx.conn(), id)? {
        orphaned.extend(crate::ops::decision::delete_subtree(tx, decision_id)?);
    }
    // A dimension has no polymorphic children; its own subtree is its values and the assignments on them.
    for dimension_id in read::dimension_ids_in_project(tx.conn(), id)? {
        crate::ops::dimension::delete_subtree(tx, dimension_id)?;
    }
    // Nor has the project itself: what you can attach to is the four things `attachment.target_type` is
    // constrained to (`task` / `decision` / `task_comment` / `decision_comment`), and a project is not one
    // of them — so there is nothing polymorphic left here to sweep.
    tx.delete_record("project", project_before.id)?;
    Ok(orphaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_decision_in, mk_project, mk_task_in, with_tx};

    /// Deleting a project **physically deletes** its subtree — nothing is left orphaned. Its tasks, their
    /// dependency edges, and the project's decisions and dimensions all go; another project's tasks are
    /// not dragged along.
    #[test]
    fn delete_removes_the_member_tasks_and_their_edges() {
        with_tx(|tx| {
            let p = mk_project(tx, "消えるPJ");
            let other = mk_project(tx, "残るPJ");
            let t1 = mk_task_in(tx, "属タスク1", Some(p));
            let t2 = mk_task_in(tx, "属タスク2", Some(p));
            let survivor = mk_task_in(tx, "別PJのタスク", Some(other));
            // A dependency edge between two member tasks (expected to go with them).
            crate::ops::dependency::add(tx, t1, t2, None).unwrap();

            delete(tx, p).unwrap();

            assert!(read::project(tx.conn(), p).unwrap().is_none(), "the project's own row goes");
            assert!(read::task(tx.conn(), t1).unwrap().is_none(), "member task 1 goes with it");
            assert!(read::task(tx.conn(), t2).unwrap().is_none(), "member task 2 goes with it");
            // A task in another project does not live or die with this one.
            assert!(read::task_live(tx.conn(), survivor).unwrap(), "another project's task survives");
            // The dependency edge goes with the tasks it names — nothing dangles.
            assert!(read::dependency_id(tx.conn(), t1, t2).unwrap().is_none());
        });
    }

    /// The subtree includes what a decision is classified as (`AMB-D-781`). This is the order the sweep
    /// is checked in: decisions go before axes, so a classification left behind would hold the axis's
    /// value by `RESTRICT` and fail the delete here rather than anywhere a person could see it coming.
    #[test]
    fn delete_gets_past_the_classifications_its_decisions_carried() {
        with_tx(|tx| {
            let p = mk_project(tx, "消えるPJ");
            let k = mk_decision_in(tx, "分類のついた決定", p);
            let t = mk_task_in(tx, "分類のついたタスク", Some(p));
            let axis = crate::ops::dimension::add(
                tx,
                p,
                crate::ops::dimension::NewDimension {
                    name: "カテゴリー".to_string(),
                    ..crate::ops::dimension::NewDimension::default()
                },
            )
            .unwrap();
            let value = crate::ops::dimension::value_add(tx, axis.id, "A", None).unwrap();
            crate::ops::dimension::set(tx, t, value.id).unwrap();
            crate::ops::dimension::set_on_decision(tx, k, value.id).unwrap();

            delete(tx, p).unwrap();

            assert!(read::project(tx.conn(), p).unwrap().is_none(), "the project's own row goes");
            assert!(read::decision(tx.conn(), k).unwrap().is_none());
            assert!(read::dimension(tx.conn(), axis.id).unwrap().is_none());
            assert!(read::dimension_value(tx.conn(), value.id).unwrap().is_none());
        });
    }

    /// A slug is derived from the name at creation time and is unique on the device (a collision escapes
    /// through a numeric suffix).
    #[test]
    fn slugs_are_derived_from_the_name_and_unique() {
        with_tx(|tx| {
            // The only ASCII either name has is "amenbo" (the full-width characters drop out), so both
            // land on the same base slug.
            let p1 = mk_project(tx, "amenbo 開発");
            let p2 = mk_project(tx, "amenbo 本番");
            let slug = |id: i64| read::project(tx.conn(), id).unwrap().unwrap().slug.unwrap();
            assert_eq!(slug(p1), "amenbo");
            assert_eq!(slug(p2), "amenbo-2");
            // A name with no ASCII alphanumerics at all still gets a slug, and it is still unique.
            let p3 = mk_project(tx, "日本語だけ");
            let p4 = mk_project(tx, "全角のみ");
            assert_eq!(slug(p3), "project");
            assert_eq!(slug(p4), "project-2");
        });
    }

    /// Renaming a project leaves its slug where it was, so the matching material `.amenbo` holds keeps
    /// meaning something.
    #[test]
    fn renaming_a_project_leaves_its_slug_alone() {
        with_tx(|tx| {
            let p = mk_project(tx, "alpha");
            update(tx, p, ProjectPatch { name: Some("beta".into()), ..Default::default() }).unwrap();
            let after = read::project(tx.conn(), p).unwrap().unwrap();
            assert_eq!(after.name, "beta");
            assert_eq!(after.slug.as_deref(), Some("alpha"));
        });
    }

}
