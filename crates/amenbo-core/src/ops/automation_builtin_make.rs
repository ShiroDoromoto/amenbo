//! **The built-in that files a task** — one task, from the title and the notes it is handed (`AMB-D-971`).
//!
//! What a person hands a run and what a run fetches become a task before anything goes on
//! (`AMB-D-970`), so the rest of the run is the same road a taken task walks. Filing it here rather than
//! in a prompt keeps it to the one transaction the step is opened in, the way taking one is
//! ([`super::automation_builtin_take`]).
//!
//! **It can take the task it filed** ([`WHAT_THEN`] answered [`TAKE_IT`]): in progress from the moment it
//! exists, and the task this run works from here on. Filed and reserved as two acts, the task would stand
//! `todo` between them, and a run waiting for a task to take looks once a second.
//!
//! **Everything the task is filed with is decided where it is placed**: what it depends on, how it is
//! classified, who it is given to and how urgent it is. A task that is not to be picked up yet says so by
//! those, never by `blocked` — that is for a task nobody can move (`AMB-D-966`).
//!
//! **Its creation is finished here.** A task left being created is one nobody can reserve.
//!
//! **A setting it cannot follow files nothing.** A built-in that falls over leaves by the error way out
//! inside the transaction that opened its step, and nothing takes back what it wrote before that — so
//! every refusal the writes would raise is asked first ([`refusal`]): a classification it cannot find or
//! may not use, a required axis left empty, a task to depend on that would keep the new one from being
//! taken.

use crate::error::{Error, Result};
use crate::model::{ActorKind, AutomationCfgKind, AutomationPortKind, Priority};
use crate::ops::automation_builtin::{
    Builtin, BuiltinExit, BuiltinPort, BuiltinSetting, Carried, Carry, Chooses, Work,
};
use crate::ops::automation_report::{self, Produced};
use crate::ops::task::{self, NewTask};
use crate::store_engine::read;

/// The way out it leaves by once it has filed a task and left it not started.
pub const MADE: &str = "起票した";
/// The way out it leaves by once it has filed a task and taken it.
pub const MADE_AND_TAKEN: &str = "起票して着手した";
/// The output the task is handed on through, on either way out.
pub const TASK: &str = "タスク";
/// The input the task's title is handed in through.
pub const TITLE: &str = "タイトル";
/// The input the task's notes are handed in through.
pub const NOTES: &str = "本文";
/// The setting that says whether it takes the task it filed.
pub const WHAT_THEN: &str = "起票したタスク";
/// The choice on [`WHAT_THEN`] that leaves the task not started — also what it does left unanswered.
pub const LEAVE_IT: &str = "未着手のまま、出口「起票した」へ進む";
/// The choice on [`WHAT_THEN`] that takes it.
pub const TAKE_IT: &str = "進行中にして、出口「起票して着手した」へ進む";
/// The setting that says what the task depends on.
pub const DEPENDS_ON: &str = "依存させる相手";
/// The choice on [`DEPENDS_ON`] that makes it depend on the task this run works.
pub const THE_RUNS_TASK: &str = "この run が扱っているタスク";
/// The setting that classifies it: one `axis=value` a line.
pub const CLASSIFY: &str = "分類";
/// The setting that says who it is given to.
pub const ASSIGNEE: &str = "担当";
/// The setting that says how urgent it is.
pub const PRIORITY: &str = "優先度";
/// The choice on every choice setting here that sets nothing — also what it does left unanswered.
pub const NONE: &str = "なし";
const HUMAN: &str = "人";
const AI: &str = "AI";
const HIGH: &str = "高";
const MEDIUM: &str = "中";
const LOW: &str = "低";

pub(super) const MAKE_TASK: Builtin = Builtin {
    key: "make_task",
    name: "タスクを起票する",
    does: "受け取ったタイトルと本文で、タスクを1件起票する。設定で、起票と同時に進行中にし、この run で扱える",
    settings: &[
        BuiltinSetting {
            name: WHAT_THEN,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["未着手のまま、出口「起票した」へ進む","進行中にして、出口「起票して着手した」へ進む"]"#),
        },
        BuiltinSetting {
            name: DEPENDS_ON,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["なし","この run が扱っているタスク"]"#),
        },
        BuiltinSetting { name: CLASSIFY, kind: AutomationCfgKind::Text, required: false, options: None },
        BuiltinSetting {
            name: ASSIGNEE,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["なし","人","AI"]"#),
        },
        BuiltinSetting {
            name: PRIORITY,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["なし","高","中","低"]"#),
        },
    ],
    ins: &[
        BuiltinPort { name: TITLE, kind: AutomationPortKind::Value, required: true },
        BuiltinPort { name: NOTES, kind: AutomationPortKind::Value, required: false },
    ],
    exits: &[
        BuiltinExit {
            name: MADE,
            outs: &[BuiltinPort { name: TASK, kind: AutomationPortKind::TaskMake, required: true }],
        },
        BuiltinExit {
            name: MADE_AND_TAKEN,
            outs: &[BuiltinPort { name: TASK, kind: AutomationPortKind::TaskTake, required: true }],
        },
    ],
    waits: None,
    chooses: Some(Chooses { setting: WHAT_THEN, answer: TAKE_IT, chosen: MADE_AND_TAKEN, otherwise: MADE }),
    work: Work::InStore(make),
};

fn make(carry: &Carry<'_, '_>) -> Result<Carried> {
    let takes = choice(carry, WHAT_THEN)?.as_deref() == Some(TAKE_IT);
    let title = carry.input(TITLE).map(str::trim).unwrap_or_default();
    // Read before anything is written, so a setting that cannot be followed files nothing.
    let depends_on = match choice(carry, DEPENDS_ON)?.as_deref() {
        Some(THE_RUNS_TASK) => Some(the_runs_task(carry, takes)?),
        _ => None,
    };
    let values = classification(carry)?;
    refusal(carry, &values, depends_on.filter(|_| takes))?;
    let assignee = match choice(carry, ASSIGNEE)?.as_deref() {
        Some(HUMAN) => Some(ActorKind::Human),
        Some(AI) => Some(ActorKind::Ai),
        _ => None,
    };
    let priority = match choice(carry, PRIORITY)?.as_deref() {
        Some(HIGH) => Some(Priority::High),
        Some(MEDIUM) => Some(Priority::Medium),
        Some(LOW) => Some(Priority::Low),
        _ => None,
    };

    let tx = carry.tx;
    let filed = task::add(
        tx,
        NewTask {
            title: title.to_string(),
            project_id: Some(carry.run.project_id),
            due_on: None,
            start_on: None,
            priority,
            notes: carry.input(NOTES).unwrap_or_default().to_string(),
            created_by_kind: Some(ActorKind::Ai),
            at_binding_id: None,
            made_in: None,
        },
    )?;
    for (_, value) in values {
        crate::ops::dimension::set(tx, filed.id, value)?;
    }
    if assignee.is_some() {
        task::set_assignee(tx, filed.id, assignee)?;
    }
    if let Some(blocker) = depends_on {
        crate::ops::dependency::add(tx, filed.id, blocker, Some(ActorKind::Ai))?;
    }
    let filed = task::finish_creating(tx, filed.id)?;

    if takes {
        let taken = automation_report::take(tx, carry.run_step.id, filed.id)?;
        return Ok(Carried {
            exit: MADE_AND_TAKEN,
            report: format!("filed and took AMB-T-{} {}", taken.id, taken.title),
        });
    }
    carry.put(MADE, TASK, Produced::Task(filed.id))?;
    Ok(Carried { exit: MADE, report: format!("filed AMB-T-{} {}", filed.id, filed.title) })
}

/// The choice written for one setting, or `None` where nobody answered it.
fn choice(carry: &Carry<'_, '_>, name: &str) -> Result<Option<String>> {
    carry.setting(name).map(|value| serde_json::from_str::<String>(value).map_err(Error::from)).transpose()
}

/// **The task this run works**, for the new one to depend on. Taking the new one opens a stretch of its
/// own, so the task meant is the one the stretch before it worked — closed by then, since a run takes no
/// task with another still open (`AMB-D-967`).
fn the_runs_task(carry: &Carry<'_, '_>, takes: bool) -> Result<i64> {
    let found = match takes {
        false => carry.task_id,
        true => read::automation_run_tasks_of(carry.tx.conn(), carry.run.id)?
            .into_iter()
            .rev()
            .filter(|stretch| Some(stretch.id) != carry.run_step.run_task_id)
            .find_map(|stretch| stretch.task_id),
    };
    found.ok_or_else(|| {
        Error::invalid(format!(
            "'{DEPENDS_ON}' is answered '{THE_RUNS_TASK}', and this run works no task yet"
        ))
    })
}

/// **What the writes would refuse, asked before any of them is made** (the module's last paragraph): a
/// closed value, a required axis with no value, and — where the new task is taken — a task it depends on
/// that is not closed yet, which would leave it not ready to take.
fn refusal(carry: &Carry<'_, '_>, values: &[(i64, i64)], taken_after: Option<i64>) -> Result<()> {
    let conn = carry.tx.conn();
    for &(_, value_id) in values {
        let value = read::dimension_value(conn, value_id)?
            .ok_or_else(|| crate::ops::dimension::VALUE_NOUN.not_found(value_id.to_string()))?;
        if value.closed {
            return Err(Error::invalid(format!("'{CLASSIFY}' names the value '{}', which is closed", value.name)));
        }
    }
    let empty: Vec<String> =
        read::required_dimensions(conn, carry.run.project_id, crate::model::ClassifiedSide::Task)?
            .into_iter()
            .filter(|(axis_id, _)| !values.iter().any(|(on, _)| on == axis_id))
            .map(|(_, name)| name)
            .collect();
    if !empty.is_empty() {
        return Err(Error::invalid(format!(
            "'{CLASSIFY}' gives no value on {}, which this project requires",
            empty.join(", ")
        )));
    }
    if let Some(blocker) = taken_after {
        let open = read::task(conn, blocker)?.is_some_and(|task| !task.status.is_closed());
        if open {
            return Err(Error::invalid(format!(
                "the task to take would depend on AMB-T-{blocker}, which is not closed, and could not be taken"
            )));
        }
    }
    Ok(())
}

/// **The values [`CLASSIFY`] names**, one `axis=value` a line, each looked up among this run's project's
/// axes, as (axis, value). A line that names no axis or no value there is refused rather than skipped: a
/// task filed without the classification it was meant to carry is one the filters that should find it
/// pass over.
fn classification(carry: &Carry<'_, '_>) -> Result<Vec<(i64, i64)>> {
    let Some(written) = choice(carry, CLASSIFY)? else {
        return Ok(Vec::new());
    };
    let conn = carry.tx.conn();
    let mut values = Vec::new();
    for line in written.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((axis, value)) = line.split_once('=') else {
            return Err(Error::invalid(format!("'{CLASSIFY}' reads one `axis=value` a line, and '{line}' is not one")));
        };
        let (axis, value) = (axis.trim(), value.trim());
        let axis_id = crate::ops::pick_id(
            read::resolve_dimension_in(conn, Some(carry.run.project_id), axis)?,
            axis,
            || crate::ops::dimension::NOUN.not_found(axis),
        )?;
        let value_id = crate::ops::pick_id(read::resolve_dimension_value_in(conn, axis_id, value)?, value, || {
            crate::ops::dimension::VALUE_NOUN.not_found(format!("{axis}={value}"))
        })?;
        values.push((axis_id, value_id));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Automation, AutomationPictureOwner, AutomationPlacement, AutomationRun, AutomationRunStatus,
        TaskStatus,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{
        check, launch_leaving_the_task_open as launch, nothing_asked, Launcher, Unmet,
    };
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::{mk_out, mk_placed, mk_project, open, way_out, with_tx};
    use crate::store_engine::WriteTx;

    /// An agent's step that takes a task and hands on a title and notes, the built-in after it, and an
    /// agent's step after that on either way out. Nothing on the picture closes the first step's task —
    /// each test closes it by hand where it has to be — so it is launched past that check.
    struct Picture {
        automation: Automation,
        project: i64,
        first: AutomationPlacement,
        make: AutomationPlacement,
    }

    fn picture(tx: &WriteTx<'_>) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation =
            automation::add(tx, project, NewAutomation { name: "file".into(), ..Default::default() })
                .expect("automation");
        let (first_action, first) = mk_placed(tx, &automation, "Take", "take one", "claude");
        crate::ops::test_support::mk_exit(tx, &first_action, "found");
        mk_out(tx, &first_action, Some("found"), "task", AutomationPortKind::TaskTake, true);
        mk_out(tx, &first_action, Some("found"), "title", AutomationPortKind::Value, true);
        mk_out(tx, &first_action, Some("found"), "notes", AutomationPortKind::Value, true);
        let written = action(tx, "make_task").expect("the built-in's action");
        let make = automation::placement_add(tx, automation.id, written.id).expect("place it");
        let on = AutomationPictureOwner::Automation;
        automation::wire_add(tx, on, first.id, Some("found"), "title", make.id, TITLE).expect("title");
        automation::wire_add(tx, on, first.id, Some("found"), "notes", make.id, NOTES).expect("notes");
        automation::edge_add(tx, on, first.id, Some("found"), EdgeTarget::Go(make.id), None).expect("on");
        automation::edge_add(tx, on, first.id, None, EdgeTarget::Done, None).expect("closes");
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        for exit in [MADE, MADE_AND_TAKEN] {
            automation::edge_add(tx, on, make.id, Some(exit), EdgeTarget::Go(work.id), None).expect("onward");
        }
        crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
        let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, project, first, make }
    }

    fn answer(tx: &WriteTx<'_>, p: &Picture, setting: &str, value: &str) {
        automation::cfg_set(tx, p.make.id, setting, Some(&serde_json::to_string(value).expect("json")))
            .expect("answer");
    }

    fn launched(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let claude = ["claude".to_string()];
        let by = Launcher {
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        launch(tx, automation.id, &by).expect("launch")
    }

    /// Walk the first step — take a task, hand on a title and notes — close that task, and carry the
    /// built-in out. What comes back is the task the first step took, the built-in's execution and what
    /// the run does next.
    fn carried(tx: &WriteTx<'_>, p: &Picture, close_first: bool) -> (AutomationRun, i64, i64, Next) {
        let run = launched(tx, &p.automation);
        let claude = ["claude".to_string()];
        let def_of = |placement: i64| {
            read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.placement_id == Some(placement))
                .expect("the spot's copy")
        };
        let opening = match open(tx, run.id, def_of(p.first.id).id, Some(&claude)).expect("open") {
            Opened::Ready(opening) => *opening,
            other => panic!("the agent's step opens a terminal, not {other:?}"),
        };
        let first = crate::ops::test_support::mk_task_in(tx, "the first", Some(p.project));
        automation_report::take(tx, opening.run_step.id, first).expect("take");
        for (port, value) in [("title", "  a follow-up  "), ("notes", "what to do")] {
            let id = crate::ops::test_support::out_port(tx, opening.run_step.id, None, port);
            automation_report::out(tx, opening.run_step.id, id, Produced::Value(value)).expect("out");
        }
        let found = way_out(tx, opening.run_step.id, "found");
        automation_report::done(tx, opening.run_step.id, found, "took one").expect("done");
        if close_first {
            task::set_status(tx, first, TaskStatus::Done).expect("close the first");
        }
        match open(tx, run.id, def_of(p.make.id).id, Some(&claude)).expect("open") {
            Opened::Carried { run_step_id, next } => (run, first, run_step_id, next),
            other => panic!("a built-in is carried out, not {other:?}"),
        }
    }

    /// The task the built-in handed on, and the way out it left by.
    fn handed(tx: &WriteTx<'_>, run_step_id: i64) -> (crate::model::Task, Option<i64>) {
        let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
        let id = read::automation_run_values_of(tx.conn(), run_step_id)
            .expect("values")
            .into_iter()
            .find_map(|v| v.task_id)
            .unwrap_or_else(|| panic!("no task handed on: {}", ran.report));
        (read::task(tx.conn(), id).expect("read").expect("task"), ran.exit_id)
    }

    /// **Left unanswered, it files one task and leaves it not started** — titled and noted from what it
    /// was handed, its creation finished, nothing else set — and the run stays on the task it had.
    #[test]
    fn unanswered_it_files_one_task_and_leaves_it_not_started() {
        with_tx(|tx| {
            let p = picture(tx);
            let (run, first, run_step_id, _) = carried(tx, &p, false);
            let (filed, exit) = handed(tx, run_step_id);
            assert_eq!(exit, way_out(tx, run_step_id, MADE));
            assert_eq!(filed.title, "a follow-up");
            assert_eq!(filed.notes, "what to do");
            assert_eq!(filed.status, TaskStatus::Todo);
            assert!(!filed.draft, "its creation is finished");
            assert_eq!(filed.project_id, Some(p.project));
            assert_eq!(filed.assignee_kind, None);
            assert_eq!(filed.priority, None);
            let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
            assert_eq!(stretch.task_id, Some(first), "the run is still on the task it took");
        });
    }

    /// **Set to take it, the task it files is in progress and is what the run works** — a stretch of
    /// its own, handed on as the task taken.
    #[test]
    fn set_to_take_it_the_task_it_files_is_the_one_the_run_works() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, WHAT_THEN, TAKE_IT);
            let (run, first, run_step_id, next) = carried(tx, &p, true);
            let (filed, exit) = handed(tx, run_step_id);
            assert_eq!(exit, way_out(tx, run_step_id, MADE_AND_TAKEN));
            assert_eq!(filed.status, TaskStatus::InProgress, "reserved in the act of filing");
            assert!(matches!(next, Next::Step(_)), "the run goes on: {next:?}");
            let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
            assert_eq!(stretch.task_id, Some(filed.id));
            assert_ne!(stretch.task_id, Some(first));
        });
    }

    /// **Taking it with the task before still open is refused**, as for any step that takes one
    /// (`AMB-D-967`) — nothing is filed.
    #[test]
    fn taking_it_with_the_task_before_open_is_refused() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, WHAT_THEN, TAKE_IT);
            let run = launched(tx, &p.automation);
            let claude = ["claude".to_string()];
            let defs = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            let def = |placement: i64| defs.iter().find(|d| d.placement_id == Some(placement)).expect("copy").id;
            let opening = match open(tx, run.id, def(p.first.id), Some(&claude)).expect("open") {
                Opened::Ready(opening) => *opening,
                other => panic!("{other:?}"),
            };
            let first = crate::ops::test_support::mk_task_in(tx, "the first", Some(p.project));
            automation_report::take(tx, opening.run_step.id, first).expect("take");
            let before = read::automation_run_steps_of(tx.conn(), run.id).expect("steps").len();
            match open(tx, run.id, def(p.make.id), Some(&claude)).expect("open") {
                Opened::LeftTaskOpen { run } => assert_eq!(run.status, AutomationRunStatus::Failed),
                other => panic!("the task before is still in progress, so {other:?} is wrong"),
            }
            assert_eq!(read::automation_run_steps_of(tx.conn(), run.id).expect("steps").len(), before);
        });
    }

    /// **What it is set with is on the task it files**: the classification, who it is given to, how
    /// urgent it is and the run's task as what it depends on.
    #[test]
    fn what_it_is_set_with_is_on_the_task_it_files() {
        with_tx(|tx| {
            let p = picture(tx);
            let axis = crate::ops::dimension::add(
                tx,
                p.project,
                crate::ops::dimension::NewDimension { name: "職能".into(), ..Default::default() },
            )
            .expect("axis");
            let value = crate::ops::dimension::value_add(tx, axis.id, "実装", None).expect("value");
            answer(tx, &p, CLASSIFY, "職能=実装\n");
            answer(tx, &p, ASSIGNEE, AI);
            answer(tx, &p, PRIORITY, HIGH);
            answer(tx, &p, DEPENDS_ON, THE_RUNS_TASK);
            let (_, first, run_step_id, _) = carried(tx, &p, false);
            let (filed, _) = handed(tx, run_step_id);
            assert_eq!(filed.assignee_kind, Some(ActorKind::Ai));
            assert_eq!(filed.priority, Some(Priority::High));
            assert!(read::assignment_id(tx.conn(), filed.id, value.id).expect("read").is_some());
            assert!(read::dependency_id(tx.conn(), filed.id, first).expect("read").is_some());
        });
    }

    /// **Taking it and depending on the run's task means the task the stretch before worked.**
    #[test]
    fn taking_it_the_runs_task_is_the_one_before() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, WHAT_THEN, TAKE_IT);
            answer(tx, &p, DEPENDS_ON, THE_RUNS_TASK);
            let (_, first, run_step_id, _) = carried(tx, &p, true);
            let (filed, exit) = handed(tx, run_step_id);
            assert_eq!(exit, way_out(tx, run_step_id, MADE_AND_TAKEN));
            assert!(read::dependency_id(tx.conn(), filed.id, first).expect("read").is_some());
        });
    }

    /// **A classification it cannot find files nothing** and leaves by the error way out, naming it.
    #[test]
    fn a_classification_it_cannot_find_files_nothing() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, CLASSIFY, "職能=実装");
            let (_, first, run_step_id, next) = carried(tx, &p, false);
            assert!(matches!(next, Next::Halted(_)), "{next:?}");
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert!(ran.report.contains("職能"), "{}", ran.report);
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, crate::model::ERROR_EXIT));
            let filed = read::automation_run_values_of(tx.conn(), run_step_id).expect("values");
            assert!(filed.iter().all(|v| v.task_id.is_none()), "nothing handed on");
            assert!(read::task(tx.conn(), first + 1).expect("read").is_none(), "and nothing filed");
        });
    }

    /// **The launch check reads the setting**: the way out it will not leave by needs no line, and only
    /// set to take the task it files can it be the entry.
    #[test]
    fn the_launch_check_asks_only_the_way_out_it_is_set_to_leave_by() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "entry".into(), ..Default::default() })
                    .expect("automation");
            let written = action(tx, "make_task").expect("the built-in's action");
            let make = automation::placement_add(tx, automation.id, written.id).expect("place it");
            let on = AutomationPictureOwner::Automation;
            let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
            automation::edge_add(tx, on, make.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(work.id), None)
                .expect("onward");
            crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
            let automation = automation::set_entry(tx, automation.id, Some(make.id)).expect("entry");
            let startable = ["claude".to_string()];
            let unmet = |tx: &WriteTx<'_>| check(tx.conn(), automation.id, Some(&startable), nothing_asked()).expect("check");

            let left = unmet(tx);
            assert!(left.iter().any(|u| matches!(u, Unmet::EntryTakesNoTask { .. })), "{left:?}");
            assert!(left.iter().any(|u| matches!(u, Unmet::OpenExit { exit, .. } if exit == MADE)), "{left:?}");

            automation::cfg_set(tx, make.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
                .expect("take it");
            let taken = unmet(tx);
            // The title is handed in by nothing here, which is its own reason and not the one asked.
            let asked: Vec<_> = taken
                .iter()
                .filter(|u| matches!(u, Unmet::EntryTakesNoTask { .. } | Unmet::OpenExit { .. }))
                .collect();
            assert!(asked.is_empty(), "{asked:?}");
        });
    }

    /// The choices written out on the setting are the ones the code reads, the one it does unanswered
    /// first.
    #[test]
    fn the_choices_offered_are_the_ones_it_reads() {
        let offered = |name: &str| -> Vec<String> {
            let options = MAKE_TASK.settings.iter().find(|s| s.name == name).and_then(|s| s.options);
            serde_json::from_str(options.expect("a choice list")).expect("JSON")
        };
        assert_eq!(offered(WHAT_THEN), vec![LEAVE_IT, TAKE_IT]);
        assert_eq!(offered(DEPENDS_ON), vec![NONE, THE_RUNS_TASK]);
        assert_eq!(offered(ASSIGNEE), vec![NONE, HUMAN, AI]);
        assert_eq!(offered(PRIORITY), vec![NONE, HIGH, MEDIUM, LOW]);
    }
}
