//! **The built-in that takes a task** — find the task a run works next and reserve it (`AMB-D-964`).
//!
//! What a prompt used to be asked to do with `task list` and `step-take`, done here in the one
//! transaction the step is opened in: list the tasks the filter matches, in the order it asks for, and
//! reserve them from the top until one goes through. A task somebody else reserved first is not a
//! failure — the next one is tried.
//!
//! **Not started and ready are always asked for.** Only a `todo` task whose premises all hold can be
//! reserved, so the filter's own `status:` and `ready:` are dropped and those two put in their place:
//! a setting that asked for anything else would be asking for a task the reservation refuses.
//!
//! **Left unanswered, the filter is the tasks given to the AI** (`assignee:me-ai`). A run that stops to
//! call a person hands its task to that person (`AMB-D-966`); a filter that did not ask for the AI's
//! would take it straight back.
//!
//! It leaves by [`TAKEN`] with the task it reserved, or by [`NONE_TO_TAKE`] with nothing —
//! the step that went looking and found none, which owes no report and keeps no stretch
//! ([`super::automation_report::done`]).

use crate::error::Result;
use crate::model::{AutomationCfgKind, AutomationPortKind};
use crate::ops::automation_builtin::{Builtin, BuiltinExit, BuiltinPort, BuiltinSetting, Carried, Carry};
use crate::ops::automation_report;
use crate::ops::automation_step::{taskfilter_expr, taskfilter_sort, TASKFILTER_SORT_DEFAULT};
use crate::query::{self, ListParams};
use crate::reach::Reach;

/// The way out it leaves by once it has reserved a task.
pub const TAKEN: &str = "着手した";
/// The way out it leaves by when no task the filter matches could be reserved.
pub const NONE_TO_TAKE: &str = "着手できるタスクが無い";
/// The setting that says which tasks it takes, and in what order.
pub const FILTER: &str = "絞り込み";
/// The output the reserved task is handed on through.
pub const TASK: &str = "タスク";

/// What every search adds to the filter, whatever the setting says.
const TAKEABLE: &str = "status:todo ready:yes";
/// What the filter is when nobody answered it.
const UNANSWERED: &str = "assignee:me-ai";
/// How many candidates are read at a time. Most runs reserve the first; the rest are read only while
/// the ones before them were taken by somebody else.
const PAGE: usize = 20;

pub(super) const TAKE_TASK: Builtin = Builtin {
    key: "take_task",
    name: "タスクに着手する",
    does: "絞り込みに合う未着手で ready のタスクを並び順どおりに探し、先頭から予約して進行中にする",
    settings: &[BuiltinSetting { name: FILTER, kind: AutomationCfgKind::TaskFilter, required: false, options: None }],
    ins: &[],
    exits: &[
        BuiltinExit {
            name: Some(TAKEN),
            outs: &[BuiltinPort { name: TASK, kind: AutomationPortKind::TaskTake, required: true }],
        },
        BuiltinExit { name: Some(NONE_TO_TAKE), outs: &[] },
    ],
    run: take,
};

fn take(carry: &Carry<'_, '_>) -> Result<Carried> {
    let answer = carry.setting(FILTER);
    let expr = expression(answer);
    let sort = answer.map(taskfilter_sort).unwrap_or_else(|| TASKFILTER_SORT_DEFAULT.to_string());
    let reach = Reach::binding(carry.run.project_id);
    let mut offset = 0;
    loop {
        let page = query::list(
            carry.tx.conn(),
            reach,
            ListParams {
                filter_expr: Some(expr.clone()),
                sort: sort.clone(),
                limit: Some(PAGE),
                offset: Some(offset),
                ..Default::default()
            },
        )?;
        for candidate in &page.tasks {
            match automation_report::take(carry.tx, carry.run_step.id, candidate.id) {
                Ok(task) => {
                    return Ok(Carried {
                        exit: Some(TAKEN),
                        report: format!("took AMB-T-{} {}", task.id, task.title),
                    })
                }
                // Reserved by somebody else since the list was read, or no longer ready: the next one.
                Err(e) if matches!(e.code(), "already_reserved" | "not_ready") => continue,
                Err(e) => return Err(e),
            }
        }
        if page.tasks.len() < PAGE {
            break;
        }
        offset += PAGE;
    }
    Ok(Carried { exit: Some(NONE_TO_TAKE), report: format!("no task `{expr}` lists could be taken") })
}

/// **The filter it searches with**: the parts the setting chose, less `status:` and `ready:`, and the
/// two it always asks for after them.
fn expression(answer: Option<&str>) -> String {
    let chosen = match answer {
        None => Some(UNANSWERED.to_string()),
        Some(value) => {
            let mut parts = match serde_json::from_str::<serde_json::Value>(value) {
                Ok(serde_json::Value::Object(parts)) => parts,
                _ => serde_json::Map::new(),
            };
            parts.remove("status");
            parts.remove("ready");
            taskfilter_expr(&serde_json::Value::Object(parts).to_string())
        }
    };
    match chosen {
        Some(chosen) => format!("{chosen} {TAKEABLE}"),
        None => TAKEABLE.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ActorKind, Automation, AutomationPictureOwner, AutomationPlacement, AutomationRun,
        AutomationRunStatus, Priority, TaskStatus,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{launch, nothing_asked, Launcher};
    use crate::ops::automation_step::{open, Opened};
    use crate::ops::task::{self, TaskPatch};
    use crate::ops::test_support::{mk_project, mk_task_in, way_out, with_tx};
    use crate::store_engine::{read, WriteTx};

    /// An automation whose entry is the built-in, with both of its ways out closing the run.
    fn picture(tx: &WriteTx<'_>, project: i64) -> (Automation, AutomationPlacement) {
        let automation =
            automation::add(tx, project, NewAutomation { name: "take".into(), ..Default::default() })
                .expect("automation");
        let written = action(tx, "take_task").expect("the built-in's action");
        let spot = automation::placement_add(tx, automation.id, written.id).expect("place it");
        let on = AutomationPictureOwner::Automation;
        for exit in [TAKEN, NONE_TO_TAKE] {
            automation::edge_add(tx, on, spot.id, Some(exit), EdgeTarget::Done, None).expect("closes");
        }
        let automation = automation::set_entry(tx, automation.id, Some(spot.id)).expect("entry");
        (automation, spot)
    }

    /// Launch it and open the entry step, which the built-in carries out on the spot.
    fn carried(tx: &WriteTx<'_>, automation: &Automation) -> (AutomationRun, i64, Next) {
        let by = Launcher {
            startable: Some(&[]),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        let run = launch(tx, automation.id, &by).expect("launch");
        let entry = read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.builtin.as_deref() == Some("take_task"))
            .expect("the built-in's copy");
        match open(tx, run.id, entry.id, Some(&[])).expect("open") {
            Opened::Carried { run_step_id, next } => (run, run_step_id, next),
            other => panic!("a built-in is carried out, not {other:?}"),
        }
    }

    fn for_ai(tx: &WriteTx<'_>, title: &str, project: i64, priority: Option<Priority>) -> i64 {
        let id = mk_task_in(tx, title, Some(project));
        task::set_assignee(tx, id, Some(ActorKind::Ai)).expect("give it to the AI");
        if priority.is_some() {
            task::update(tx, id, TaskPatch { priority, ..Default::default() }).expect("priority");
        }
        id
    }

    fn status(tx: &WriteTx<'_>, id: i64) -> TaskStatus {
        read::task(tx.conn(), id).expect("read").expect("task").status
    }

    /// **It reserves the first task it can**: the AI's, not started, ready, in this project, highest
    /// priority first — and hands it on through [`TAKEN`], as the task the run now works.
    #[test]
    fn it_reserves_the_first_takeable_task_and_hands_it_on() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let elsewhere = mk_project(tx, "other");
            let (automation, _) = picture(tx, project);

            let low = for_ai(tx, "low", project, Some(Priority::Low));
            let high = for_ai(tx, "high", project, Some(Priority::High));
            let people = mk_task_in(tx, "a person's", Some(project));
            task::update(tx, people, TaskPatch { priority: Some(Priority::High), ..Default::default() })
                .expect("priority");
            let running = for_ai(tx, "running", project, Some(Priority::High));
            task::set_status(tx, running, TaskStatus::InProgress).expect("reserve");
            let waiting = for_ai(tx, "waiting", project, Some(Priority::High));
            crate::ops::dependency::add(tx, waiting, low, None).expect("depend");
            for_ai(tx, "not ours", elsewhere, Some(Priority::High));

            let (run, run_step_id, _) = carried(tx, &automation);
            assert_eq!(status(tx, high), TaskStatus::InProgress, "the highest the filter lets through");
            assert_eq!(status(tx, low), TaskStatus::Todo);
            assert_eq!(status(tx, people), TaskStatus::Todo, "a person's task is left to them");
            assert_eq!(status(tx, waiting), TaskStatus::Todo, "one that is not ready is not taken");

            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, TAKEN));
            let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
            assert_eq!(stretch.task_id, Some(high), "the run is on the task it took");
            let handed: Vec<_> = read::automation_run_values_of(tx.conn(), run_step_id)
                .expect("values")
                .into_iter()
                .filter_map(|v| v.task_id)
                .collect();
            assert_eq!(handed, vec![high]);
        });
    }

    /// **What the setting says about status and ready is not what it searches with** — a filter that
    /// asked for tasks under way would be asking for ones the reservation refuses.
    #[test]
    fn the_setting_cannot_ask_for_a_task_that_cannot_be_taken() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, spot) = picture(tx, project);
            let running = for_ai(tx, "running", project, None);
            task::set_status(tx, running, TaskStatus::InProgress).expect("reserve");
            let people = mk_task_in(tx, "a person's", Some(project));
            automation::cfg_set(
                tx,
                spot.id,
                FILTER,
                Some(r#"{"status":["in_progress"],"ready":["no"],"assignee":["none"],"sort":"-priority"}"#),
            )
            .expect("answer");

            let (_, run_step_id, _) = carried(tx, &automation);
            assert_eq!(status(tx, people), TaskStatus::InProgress, "the rest of the setting is kept");
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, TAKEN));
        });
    }

    /// **Nothing to take leaves by [`NONE_TO_TAKE`]** — with no task touched and no stretch kept.
    #[test]
    fn with_nothing_to_take_it_leaves_by_the_way_out_that_says_so() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = picture(tx, project);
            let people = mk_task_in(tx, "a person's", Some(project));

            let (run, run_step_id, next) = carried(tx, &automation);
            assert!(matches!(next, Next::Closed(_)), "the line after it closes the run: {next:?}");
            assert_eq!(status(tx, people), TaskStatus::Todo);
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, NONE_TO_TAKE));
            assert_eq!(ran.run_task_id, None, "it went looking and found none");
            let run = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(run.status, AutomationRunStatus::Completed);
        });
    }

    #[test]
    fn the_expression_drops_status_and_ready_and_asks_for_the_takeable() {
        assert_eq!(expression(None), "assignee:me-ai status:todo ready:yes");
        assert_eq!(
            expression(Some(r#"{"status":["done"],"ready":["no"],"priority":["high"]}"#)),
            "priority:high status:todo ready:yes",
        );
        assert_eq!(expression(Some(r#"{"sort":"due"}"#)), "status:todo ready:yes");
    }
}
