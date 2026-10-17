//! **The built-in that puts a task off** — put the task the run is on back in `todo`, on whoever it is
//! on, and go on (`AMB-D-967`).
//!
//! The built-in that hands a task back to a person ([`crate::ops::automation_builtin_hand_back`]) gives
//! it to the human, so no run takes it again until a person hands it back to the AI. Where the task has
//! to wait for another — a fix it needs that lands first — that is a wait on a person it does not need.
//! Here the task keeps its assignee: once what it waits for is out of the way, a run whose filter finds
//! it takes it again by itself (`AMB-D-966`).
//!
//! **What keeps it from being taken again at once is not this built-in's** (`AMB-D-971`). A task on the
//! AI in `todo` with nothing it depends on is the first a take finds, so the step before this one files
//! what it waits for and makes it depend on that.
//!
//! **Everything else is as the hand back does it**: the last report an agent gave in this stretch goes on
//! the task once, and the stretch ends here. It leaves by the done way out. A task already closed is left
//! as it is and leaves the same way; no task at all leaves by the error way out.

use crate::error::{Error, Result};
use crate::model::{ActorKind, TaskStatus, DONE_EXIT};
use crate::ops::automation_builtin::{Builtin, BuiltinExit, Carried, Carry, Work};
use crate::ops::automation_builtin_close::{already_on_the_task, last_report};
use crate::ops::automation_builtin_hand_back::end_stretch;
use crate::run_wording::builtin as say;
use crate::store_engine::read;

pub(crate) const DEFER_TASK: Builtin = Builtin {
    key: "defer_task",
    version: 1,
    name: "タスクを後回しにする",
    does: "いま扱っているタスクを、担当を変えずに todo に戻す。実行は止めずに先へ進む",
    steps: &[
        "いま扱っているタスクを読む。既に閉じていれば、何もせずに「完了」から出る",
        "このタスクで AI が最後に出した報告を、タスクのコメントに残す。既に残っていれば、もう一度は残さない",
        "進行中なら todo に戻す。担当は変えない",
        "実行は止めずに、「完了」から出る",
    ],
    halts: &[
        "この実行がまだタスクを扱っていない",
    ],
    settings: &[],
    ins: &[],
    exits: &[BuiltinExit { name: DONE_EXIT, outs: &[] }],
    waits: None,
    chooses: None,
    work: Work::InStore(defer),
};

fn defer(carry: &Carry<'_, '_>) -> Result<Carried> {
    let tx = carry.tx;
    let lang = tx.language();
    let task_id = carry.task_id.ok_or_else(|| Error::invalid(say(lang, "noTaskToDefer", &[])))?;
    let task = read::task(tx.conn(), task_id)?
        .ok_or_else(|| Error::not_found(format!("task AMB-T-{task_id}")))?;
    let named = format!("AMB-T-{task_id}");
    if task.status.is_closed() {
        return Ok(Carried { exit: DONE_EXIT, report: say(lang, "closedAlready", &[("task", &named)]) });
    }
    if let Some(reported) = last_report(tx, carry.run_step)? {
        if !already_on_the_task(tx, task_id, reported.id)? {
            crate::ops::comment::add_report_comment(tx, task_id, ActorKind::Ai, &reported.report, reported.id)?;
        }
    }
    if task.status == TaskStatus::InProgress {
        crate::ops::task::set_status(tx, task_id, TaskStatus::Todo, ActorKind::Ai)?;
    }
    end_stretch(carry)?;
    Ok(Carried { exit: DONE_EXIT, report: say(lang, "deferred", &[("task", &named), ("title", &task.title)]) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Automation, AutomationPictureOwner, AutomationPlacement, AutomationRunStatus};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::automation_report::{self, Next};
    use crate::ops::automation_run::{launch, nothing_asked, Launcher};
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::lifecycle::name::{COMMENT_ADDED, TASK_STATUS_CHANGED};
    use crate::ops::test_support::{by_ai, events_after, outbox_head};
    use crate::ops::test_support::{mk_placed, mk_project, mk_task_in, with_tx};
    use crate::store_engine::WriteTx;

    /// Take a task, look at it, and put it off — then take the next one.
    struct Picture {
        automation: Automation,
        take: AutomationPlacement,
    }

    fn picture(tx: &WriteTx<'_>, project: i64) -> Picture {
        let automation =
            automation::add(tx, project, NewAutomation { name: "defer".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place take");
        let (_, work) = mk_placed(tx, &automation, "look", "look at it", "claude");
        let later = automation::placement_add(tx, automation.id, action(tx, "defer_task").expect("defer").id)
            .expect("place defer");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(later.id), None).expect("work → defer");
        automation::edge_add(tx, on, later.id, None, EdgeTarget::Go(take.id), None).expect("defer → take");
        let automation = automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");
        Picture { automation, take }
    }

    fn comments(tx: &WriteTx<'_>, task: i64) -> Vec<String> {
        read::task_comment_ids(tx.conn(), task)
            .expect("ids")
            .into_iter()
            .filter_map(|id| read::task_comment(tx.conn(), id).expect("read"))
            .map(|c| c.text)
            .collect()
    }

    /// **The task goes back to `todo` on the AI, and the run goes on** — the launch finds no line that
    /// leaves the task open, the task put off waits on the one filed for it rather than on a person, and
    /// the take after it finds nothing it can take and ends the run. Once what it waits for is closed, it
    /// is the next a take finds.
    #[test]
    fn it_puts_the_task_off_on_the_ai_and_the_run_goes_on() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let p = picture(tx, project);
            let task = mk_task_in(tx, "main の赤で落ちたもの", Some(project));
            crate::ops::task::set_assignee(tx, task, Some(ActorKind::Ai), ActorKind::Ai).expect("to the AI");
            let claude = ["claude".to_string()];
            let by = Launcher {
                startable: Some(&claude),
                models: nothing_asked(),
                workspace_open: Some(true),
                by: Some(ActorKind::Ai),
            };
            let run = launch(tx, p.automation.id, &by).expect("putting it off lets the task go");
            let entry = read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.entry)
                .expect("the entry");
            let Opened::Carried { next: Next::Step(work), .. } =
                open(tx, run.id, entry.id, Some(&claude)).expect("take")
            else {
                panic!("the take goes on to the work");
            };
            let Opened::Ready(opening) = open(tx, run.id, work.id, Some(&claude)).expect("open the work") else {
                panic!("the work opens a terminal");
            };
            // The step files the fix the task waits for, on a person here so the take does not find it.
            let fix = mk_task_in(tx, "main の赤を直す", Some(project));
            crate::ops::task::set_assignee(tx, fix, Some(ActorKind::Human), ActorKind::Ai).expect("to a person");
            crate::ops::dependency::add(tx, task, fix, None).expect("depend");
            let Next::Step(later) =
                automation_report::done(tx, opening.run_step.id, None, "Waits for the fix on main.").expect("report")
            else {
                panic!("the work goes on to the defer");
            };
            let head = outbox_head(tx);
            let Opened::Carried { next: Next::Step(take), .. } =
                open(tx, run.id, later.id, Some(&claude)).expect("defer")
            else {
                panic!("a built-in is carried out, and the run goes on");
            };
            assert_eq!(take.placement_id, Some(p.take.id));
            assert_eq!(
                events_after(tx, head),
                vec![by_ai(COMMENT_ADDED, None), by_ai(TASK_STATUS_CHANGED, Some("todo"))],
                "nobody else is given the task",
            );

            let deferred = read::task(tx.conn(), task).expect("read").expect("task");
            assert_eq!(deferred.status, TaskStatus::Todo);
            assert_eq!(deferred.assignee_kind, Some(ActorKind::Ai));
            assert_eq!(comments(tx, task), vec!["Waits for the fix on main.".to_string()]);

            let Opened::Carried { next: Next::Closed(ended), .. } =
                open(tx, run.id, take.id, Some(&claude)).expect("take again")
            else {
                panic!("with the task waiting on the fix, there is nothing to take and the run ends");
            };
            assert_eq!(ended.run.status, AutomationRunStatus::Completed, "{:?}", ended.run.stopped_reason);
            assert_eq!(read::task_status(tx.conn(), task).expect("read"), Some(TaskStatus::Todo));

            crate::ops::task::set_status(tx, fix, TaskStatus::Done, ActorKind::Human).expect("the fix lands");
            let again = launch(tx, p.automation.id, &by).expect("launch again");
            let entry = read::automation_run_defs_of(tx.conn(), again.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.entry)
                .expect("the entry");
            let Opened::Carried { next: Next::Step(_), .. } =
                open(tx, again.id, entry.id, Some(&claude)).expect("take")
            else {
                panic!("the task put off is taken again");
            };
            assert_eq!(read::task_status(tx.conn(), task).expect("read"), Some(TaskStatus::InProgress));
        });
    }
}
