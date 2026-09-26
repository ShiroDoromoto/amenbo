//! **The built-in that splits by an axis** — send the task the run is on down the way out named by its
//! value on one axis (`AMB-D-972`).
//!
//! What a task needs after it is taken can differ by its classification — an engineer's task, a
//! designer's, a planner's. Splitting on the axis lets one automation hold the worktree and the close
//! for all of them, and the branch is still only "which way out it left by": a line carries no
//! condition (`AutomationEdge`).
//!
//! **Its ways out are the axis's values**, not names the code holds, so it is the one built-in there is
//! one of per axis. Its library action is written in the axis's project the first time it is placed on
//! that axis ([`action`]), marked with the axis, and found again by it. The axis is chosen where it is
//! placed and not changed after: another axis is another action.
//!
//! **The ways out follow the axis** ([`follow`]). A value added, renamed, moved or deleted is written
//! onto every action that splits by the axis in the same transaction — renamed rather than rewritten, so
//! the lines an automation hangs on it stay. A value added after a run was launched has no way out on
//! that run's copy, so a task carrying it leaves by [`UNSORTED`], as a task carrying no value does.
//!
//! **Only an axis a task holds one value of** can be split by, since a task holding two would have two
//! ways out to leave by. An axis split by is not widened to hold several afterwards
//! ([`refuse_to_widen`]).

use crate::error::{Error, Result};
use crate::model::{DimensionAppliesTo, DimensionCardinality, ERROR_EXIT};
use crate::ops::automation;
use crate::ops::automation_builtin::{write_action, Builtin, BuiltinExit, Carry, Named, Work};
use crate::run_wording::builtin as say;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// The way out a task carrying no value on the axis leaves by — and one carrying a value its run's copy
/// has no way out for.
pub const UNSORTED: &str = "分類なし";

pub(crate) const SPLIT_BY_DIM: Builtin = Builtin {
    key: "split_by_dim",
    name: "分類で分ける",
    does: "いま扱っているタスクを、選んだ軸の値で分ける。値ごとに出口があり、値が付いていないタスクは「分類なし」から出る",
    settings: &[],
    ins: &[],
    exits: &[BuiltinExit { name: UNSORTED, outs: &[] }],
    waits: None,
    chooses: None,
    work: Work::Named(split),
};

/// **The library action that splits by this axis**, written in the axis's project the first time it is
/// asked for and found again by the axis after that.
///
/// Refused for an axis a task can hold several values of, and for one that classifies decisions alone.
pub(crate) fn action(tx: &WriteTx<'_>, axis: i64) -> Result<crate::model::AutomationAction> {
    let dimension = read::dimension(tx.conn(), axis)?
        .ok_or_else(|| crate::ops::dimension::NOUN.not_found(axis.to_string()))?;
    if dimension.cardinality != DimensionCardinality::Single {
        return Err(Error::invalid(format!(
            "'{}' lets a task hold several values, so a task would have more than one way out to leave by \
             — split by an axis that holds one",
            dimension.name
        )));
    }
    if dimension.applies_to == DimensionAppliesTo::Decision {
        return Err(Error::invalid(format!(
            "'{}' classifies decisions alone, so no task carries a value on it to split by",
            dimension.name
        )));
    }
    if let Some(written) = read::automation_action_builtin_on(tx.conn(), SPLIT_BY_DIM.key, axis)? {
        return Ok(written);
    }
    let action = write_action(tx, &SPLIT_BY_DIM, Some(dimension.project_id), Some(axis))?;
    automation::builtin_exits_follow(tx, action.id, &ways_out(tx, axis)?, None)?;
    Ok(action)
}

/// **The ways out a split by this axis leaves by**, in the axis's order: one per value, closed ones
/// too — a task already filed under a closed value still carries it — and [`UNSORTED`] last.
///
/// A value whose name is already a way out here gives none of its own: two values of one name, or a
/// value named [`UNSORTED`] or the error way out's name, leave by the one that name already has.
fn ways_out(tx: &WriteTx<'_>, axis: i64) -> Result<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    for (value_id, _) in read::dimension_value_siblings(tx.conn(), axis, None)? {
        let Some(value) = read::dimension_value(tx.conn(), value_id)? else { continue };
        if value.name != UNSORTED && value.name != ERROR_EXIT && !names.contains(&value.name) {
            names.push(value.name);
        }
    }
    names.push(UNSORTED.to_string());
    Ok(names)
}

/// **Bring every action that splits by this axis to its values**, after a value was added, renamed,
/// moved or deleted. `renamed` is the value's name before and after, where it was renamed.
pub(crate) fn follow(tx: &WriteTx<'_>, axis: i64, renamed: Option<(&str, &str)>) -> Result<()> {
    let actions = read::automation_actions_splitting(tx.conn(), axis)?;
    if actions.is_empty() {
        return Ok(());
    }
    let wanted = ways_out(tx, axis)?;
    for action in actions {
        automation::builtin_exits_follow(tx, action.id, &wanted, renamed)?;
    }
    Ok(())
}

/// **An axis that is going leaves the actions that split by it with none**, said as a write so the
/// change is carried like any other. Carrying one out afterwards falls over rather than guess.
pub(crate) fn forget(tx: &WriteTx<'_>, axis: i64) -> Result<()> {
    for before in read::automation_actions_splitting(tx.conn(), axis)? {
        let after = crate::model::AutomationAction {
            builtin_dimension_id: None,
            updated_at: Timestamp::now(),
            ..before.clone()
        };
        crate::ops::emit_update(tx, record::automation_action(&before), record::automation_action(&after))?;
    }
    Ok(())
}

/// **An axis split by stays one a task holds one value of** — widened, a task could carry two values and
/// have two ways out to leave by.
pub(crate) fn refuse_to_widen(tx: &WriteTx<'_>, axis: i64, name: &str) -> Result<()> {
    if read::automation_actions_splitting(tx.conn(), axis)?.is_empty() {
        return Ok(());
    }
    Err(Error::invalid(format!(
        "'{name}' is split by in an automation, so a task has to go on holding one value of it — a task \
         holding two would have two ways out to leave by"
    )))
}

/// Leave by the way out of the task's value on the axis, or by [`UNSORTED`].
fn split(carry: &Carry<'_, '_>) -> Result<Named> {
    let tx = carry.tx;
    let lang = tx.language();
    let task_id = carry.task_id.ok_or_else(|| Error::invalid(say(lang, "noTaskToSplit", &[])))?;
    let axis = axis_of(carry)?;
    let dimension = read::dimension(tx.conn(), axis)?
        .ok_or_else(|| crate::ops::dimension::NOUN.not_found(axis.to_string()))?;
    let value = match read::task_dimension_assignments(tx.conn(), task_id)?
        .into_iter()
        .find(|(on, _)| *on == axis)
    {
        Some((_, value_id)) => read::dimension_value(tx.conn(), value_id)?,
        None => None,
    };
    let declared = |name: &str| name != ERROR_EXIT && carry.exits.iter().any(|e| e.name == name);
    let task = format!("AMB-T-{task_id}");
    let (exit, report) = match value {
        Some(value) if declared(&value.name) => {
            let report = say(lang, "split", &[("task", &task), ("value", &value.name), ("axis", &dimension.name)]);
            (value.name, report)
        }
        Some(value) => (
            UNSORTED.to_string(),
            say(lang, "splitLate", &[("task", &task), ("value", &value.name), ("axis", &dimension.name)]),
        ),
        None => (UNSORTED.to_string(), say(lang, "splitNone", &[("task", &task), ("axis", &dimension.name)])),
    };
    Ok(Named { exit, report })
}

/// The axis the action this step was copied from splits by. A step deleted since the launch, or an
/// axis deleted from under the action, leaves nothing to split by.
fn axis_of(carry: &Carry<'_, '_>) -> Result<i64> {
    let conn = carry.tx.conn();
    let def = read::automation_run_def(conn, carry.run_step.run_def_id)?
        .ok_or_else(|| Error::invalid("the step's copy is gone"))?;
    let step = match def.step_id {
        Some(id) => read::automation_action_step(conn, id)?,
        None => None,
    };
    let action = match step {
        Some(step) => read::automation_action(conn, step.action_id)?,
        None => None,
    };
    action
        .and_then(|a| a.builtin_dimension_id)
        .ok_or_else(|| Error::invalid(say(carry.tx.language(), "axisGone", &[])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ActorKind, Automation, AutomationOwner, AutomationPictureOwner, AutomationPlacement,
        AutomationRun, DimensionRole,
    };
    use crate::ops::automation::{EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{check, launch, nothing_asked, Launcher, Unmet};
    use crate::ops::automation_step::Opened;
    use crate::ops::dimension::{self, NewDimension};
    use crate::ops::test_support::{mk_project, mk_task_in, open, with_tx};

    fn axis(tx: &WriteTx<'_>, project: i64, name: &str, values: &[&str]) -> i64 {
        let axis = dimension::add(
            tx,
            project,
            NewDimension {
                name: name.to_string(),
                notes: String::new(),
                cardinality: DimensionCardinality::Single,
                ordered: true,
                role: DimensionRole::None,
                show_on_card: false,
                required: false,
                applies_to: DimensionAppliesTo::Both,
                slug: None,
            },
        )
        .expect("axis")
        .id;
        for value in values {
            dimension::value_add(tx, axis, value, None).expect("value");
        }
        axis
    }

    fn value_id(tx: &WriteTx<'_>, axis: i64, name: &str) -> i64 {
        read::dimension_value_siblings(tx.conn(), axis, None)
            .expect("values")
            .into_iter()
            .map(|(id, _)| id)
            .find(|id| read::dimension_value(tx.conn(), *id).expect("read").expect("row").name == name)
            .expect("the value")
    }

    fn exit_names(tx: &WriteTx<'_>, owner: AutomationOwner, id: i64) -> Vec<String> {
        read::automation_exits_of(tx.conn(), owner, id).expect("exits").into_iter().map(|e| e.name).collect()
    }

    /// Both halves of the action: its own ways out, and those of the one step inside it.
    fn both(tx: &WriteTx<'_>, action_id: i64) -> (Vec<String>, Vec<String>) {
        let action = read::automation_action(tx.conn(), action_id).expect("read").expect("action");
        let step = action.entry_step_id.expect("the step");
        (exit_names(tx, AutomationOwner::Action, action_id), exit_names(tx, AutomationOwner::Step, step))
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// **One action per axis, in the axis's project, with a way out per value and [`UNSORTED`] last.**
    #[test]
    fn the_action_is_written_once_per_axis_with_a_way_out_per_value() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let role = axis(tx, project, "職能", &["技術", "デザイン"]);
            let written = action(tx, role).expect("write");
            assert_eq!(written.builtin.as_deref(), Some("split_by_dim"));
            assert_eq!(written.builtin_dimension_id, Some(role));
            assert_eq!(written.project_id, Some(project), "the axis's project's library");
            assert_eq!(action(tx, role).expect("again").id, written.id, "one per axis");
            let expected = names(&[ERROR_EXIT, "技術", "デザイン", UNSORTED]);
            assert_eq!(both(tx, written.id), (expected.clone(), expected));

            let other = axis(tx, project, "規模", &["小"]);
            assert_ne!(action(tx, other).expect("another axis").id, written.id, "another axis, another action");

            let refused = crate::ops::automation_builtin::action(tx, "split_by_dim");
            assert!(refused.is_err(), "the split is asked for with an axis");
            let refused = crate::ops::automation_builtin::action_on(tx, "close_task", Some(role));
            assert!(refused.is_err(), "an axis is refused for a built-in that splits by none");
        });
    }

    /// **Only an axis a task holds one value of**, and one tasks carry at all.
    #[test]
    fn an_axis_holding_several_values_is_not_split_by_nor_widened_once_it_is() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let tags = axis(tx, project, "タグ", &["a"]);
            dimension::update(tx, tags, None, None, Some(DimensionCardinality::Multi), None, None, None, None, None, None)
                .expect("widen");
            assert!(action(tx, tags).is_err(), "a task could hold two of its values");

            let role = axis(tx, project, "職能", &["技術"]);
            action(tx, role).expect("split by it");
            let widened =
                dimension::update(tx, role, None, None, Some(DimensionCardinality::Multi), None, None, None, None, None, None);
            assert!(widened.is_err(), "an axis split by keeps holding one value");
        });
    }

    /// **The ways out follow the axis**: a value added gets one, a rename keeps the line hung on it, a
    /// move reorders them, and a delete takes its way out with it.
    #[test]
    fn the_ways_out_follow_the_axis_values() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let role = axis(tx, project, "職能", &["技術", "デザイン"]);
            let split = action(tx, role).expect("write");
            let automation =
                automation::add(tx, project, NewAutomation { name: "split".into(), ..Default::default() })
                    .expect("automation");
            let placed = automation::placement_add_by_hand(tx, automation.id, split.id).expect("place");
            let on = AutomationPictureOwner::Automation;
            let line = automation::edge_add(tx, on, placed.id, Some("技術"), EdgeTarget::Done, None).expect("line");

            let planning = dimension::value_add(tx, role, "企画", None).expect("add");
            assert_eq!(both(tx, split.id).0, names(&[ERROR_EXIT, "技術", "デザイン", "企画", UNSORTED]));

            let tech = value_id(tx, role, "技術");
            dimension::value_rename(tx, tech, "エンジニア").expect("rename");
            let (on_action, on_step) = both(tx, split.id);
            assert_eq!(on_action, names(&[ERROR_EXIT, "エンジニア", "デザイン", "企画", UNSORTED]));
            assert_eq!(on_step, on_action);
            let kept = read::automation_edge(tx.conn(), line.id).expect("read");
            assert!(kept.is_some(), "the line hung on the renamed value's way out stays");

            dimension::value_move(tx, planning.id, crate::ops::Position::Top).expect("move");
            assert_eq!(both(tx, split.id).0, names(&[ERROR_EXIT, "企画", "エンジニア", "デザイン", UNSORTED]));

            dimension::value_delete(tx, tech, None).expect("delete");
            assert_eq!(both(tx, split.id).0, names(&[ERROR_EXIT, "企画", "デザイン", UNSORTED]));
            assert!(read::automation_edge(tx.conn(), line.id).expect("read").is_none(), "its line goes with it");

            dimension::delete(tx, role).expect("delete the axis");
            let split = read::automation_action(tx.conn(), split.id).expect("read").expect("action");
            assert_eq!(split.builtin_dimension_id, None, "an axis gone leaves it with none");
        });
    }

    struct Picture {
        automation: Automation,
        project: i64,
        role: i64,
        split: AutomationPlacement,
    }

    /// Take a task, split it by the axis of roles, and close the run from every way out of the split.
    fn picture(tx: &WriteTx<'_>) -> Picture {
        let project = mk_project(tx, "amenbo");
        let role = axis(tx, project, "職能", &["技術", "デザイン"]);
        let automation =
            automation::add(tx, project, NewAutomation { name: "split".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let take = crate::ops::automation_builtin::action(tx, "take_task").expect("take");
        let take = automation::placement_add(tx, automation.id, take.id).expect("place take");
        let split = automation::placement_add(tx, automation.id, action(tx, role).expect("split").id)
            .expect("place the split");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(split.id), None).expect("take → split");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        for name in ["技術", "デザイン", UNSORTED] {
            crate::ops::test_support::mk_closed_after(tx, &automation, split.id, Some(name));
        }
        let automation = automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");
        Picture { automation, project, role, split }
    }

    fn launched(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let by = Launcher { startable: None, models: nothing_asked(), workspace_open: Some(true), by: Some(ActorKind::Ai) };
        launch(tx, automation.id, &by).expect("launch")
    }

    /// Launch, take the one task there is, and carry the split out. What comes back is the way out it
    /// left by and its report.
    fn walked(tx: &WriteTx<'_>, p: &Picture) -> (String, String) {
        let run = launched(tx, &p.automation);
        let entry = read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.entry)
            .expect("the entry");
        let Opened::Carried { next: Next::Step(split), .. } = open(tx, run.id, entry.id, None).expect("take") else {
            panic!("the take goes on to the split");
        };
        let run_step_id = match open(tx, run.id, split.id, None).expect("split") {
            Opened::Carried { run_step_id, .. } => run_step_id,
            other => panic!("the split is carried out, not {other:?}"),
        };
        let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
        let exits: Vec<crate::model::RunDefExit> = serde_json::from_str(&split.exits).expect("exits");
        let left = exits.into_iter().find(|e| Some(e.id) == ran.exit_id).expect("a way out it declares");
        (left.name, ran.report)
    }

    fn for_ai(tx: &WriteTx<'_>, p: &Picture) -> i64 {
        let id = mk_task_in(tx, "やること", Some(p.project));
        crate::ops::task::set_assignee(tx, id, Some(ActorKind::Ai)).expect("give it to the AI");
        id
    }

    /// **A task leaves by the way out of its value.**
    #[test]
    fn a_task_leaves_by_the_way_out_of_its_value() {
        with_tx(|tx| {
            let p = picture(tx);
            let unmet = check(tx.conn(), p.automation.id, None, nothing_asked()).expect("check");
            assert!(unmet.is_empty(), "every way out of the split has a line: {unmet:?}");
            let task = for_ai(tx, &p);
            dimension::set(tx, task, value_id(tx, p.role, "デザイン")).expect("classify");
            let (left, report) = walked(tx, &p);
            assert_eq!(left, "デザイン");
            assert!(report.contains("デザイン"), "{report}");
        });
    }

    /// **A task with no value leaves by [`UNSORTED`]**, and so does one whose value was added after the
    /// launch — the run's copy has no way out for it.
    #[test]
    fn a_task_with_no_value_or_a_value_added_since_leaves_unsorted() {
        with_tx(|tx| {
            let p = picture(tx);
            for_ai(tx, &p);
            assert_eq!(walked(tx, &p).0, UNSORTED);
        });
        with_tx(|tx| {
            let p = picture(tx);
            let task = for_ai(tx, &p);
            let run = launched(tx, &p.automation);
            let planning = dimension::value_add(tx, p.role, "企画", None).expect("added under the run");
            dimension::set(tx, task, planning.id).expect("classify");
            let entry = read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.entry)
                .expect("the entry");
            let Opened::Carried { next: Next::Step(split), .. } = open(tx, run.id, entry.id, None).expect("take")
            else {
                panic!("the take goes on to the split");
            };
            let Opened::Carried { run_step_id, .. } = open(tx, run.id, split.id, None).expect("split") else {
                panic!("the split is carried out");
            };
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            let exits: Vec<crate::model::RunDefExit> = serde_json::from_str(&split.exits).expect("exits");
            let left = exits.into_iter().find(|e| Some(e.id) == ran.exit_id).expect("declared");
            assert_eq!(left.name, UNSORTED);
            assert!(ran.report.contains("企画"), "{}", ran.report);
        });
    }

    /// **A value added gives a way out with no line yet**, and the launch check asks for one.
    #[test]
    fn a_value_added_asks_for_a_line_before_the_next_launch() {
        with_tx(|tx| {
            let p = picture(tx);
            dimension::value_add(tx, p.role, "企画", None).expect("add");
            let unmet = check(tx.conn(), p.automation.id, None, nothing_asked()).expect("check");
            assert!(
                unmet.iter().any(|u| matches!(u, Unmet::OpenExit { exit, placement, .. } if exit == "企画" && *placement == p.split.id)),
                "the new way out has nowhere to go: {unmet:?}",
            );
        });
    }
}
