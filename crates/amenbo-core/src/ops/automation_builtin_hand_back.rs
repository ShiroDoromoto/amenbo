//! **The built-in that hands a task back to a person** — put the task the run is on back in `todo`,
//! given to the one who launched the run, and go on (`AMB-D-967`).
//!
//! A run could let go of its task two ways before this: close it, or stop and call a person. The second
//! fails the run, and a run that stops takes no task after it. Where the author has decided that a task
//! of some kind is a person's rather than the run's — one outside the project, say — that is a branch of
//! the picture, not a failure, so the run hands the task over here and carries on to whatever the
//! picture draws next.
//!
//! **It is handed back as a run that stops hands it back** ([`crate::ops::automation_stop`],
//! `AMB-D-966`): out of `in_progress` into `todo`, and given to the human, so no run takes it again at
//! once. Taking it up means handing it back to the AI.
//!
//! **The report the task keeps is the last one an agent gave** in this stretch, as the built-in that
//! closes a task keeps it ([`crate::ops::automation_builtin_close`]): that is where the agent said why the
//! task is a person's. It is not written twice where that step already carried it onto the task.
//!
//! **The stretch ends here**, not where the run next takes a task or stops. Back in `todo`, the task is
//! any run's or session's to reserve at once (`AMB-D-139`), and from then on what it is in is theirs: a
//! run that takes its next task, or ends, does not answer for it as one it left open, nor hand it back a
//! second time ([`crate::ops::automation_stop`]).
//!
//! It leaves by the done way out. A task already closed is left as it is and leaves the same way; no
//! task at all leaves by the error way out.

use crate::error::{Error, Result};
use crate::model::{ActorKind, TaskStatus, DONE_EXIT};
use crate::ops::automation_builtin::{Builtin, BuiltinExit, Carried, Carry, Work};
use crate::ops::automation_builtin_close::{already_on_the_task, last_report};
use crate::run_wording::builtin as say;
use crate::store_engine::{read, record};
use crate::time::Timestamp;

pub(crate) const HAND_BACK_TASK: Builtin = Builtin {
    key: "hand_back_task",
    version: 1,
    name: "タスクを人に返す",
    does: "いま扱っているタスクを todo に戻し、実行を起動した人の担当にする。実行は止めずに先へ進む",
    steps: &[
        "いま扱っているタスクを読む。既に閉じていれば、何もせずに「完了」から出る",
        "このタスクで AI が最後に出した報告を、タスクのコメントに残す。既に残っていれば、もう一度は残さない",
        "進行中なら todo に戻し、担当を人にする",
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
    work: Work::InStore(hand_back),
};

fn hand_back(carry: &Carry<'_, '_>) -> Result<Carried> {
    let tx = carry.tx;
    let lang = tx.language();
    let task_id = carry.task_id.ok_or_else(|| Error::invalid(say(lang, "noTaskToHandBack", &[])))?;
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
    crate::ops::task::set_assignee(tx, task_id, Some(ActorKind::Human), ActorKind::Ai)?;
    end_stretch(carry)?;
    Ok(Carried { exit: DONE_EXIT, report: say(lang, "handedBack", &[("task", &named), ("title", &task.title)]) })
}

/// Close the stretch this step is in, the one the task was taken in.
fn end_stretch(carry: &Carry<'_, '_>) -> Result<()> {
    let Some(stretch_id) = carry.run_step.run_task_id else { return Ok(()) };
    let Some(before) = read::automation_run_task(carry.tx.conn(), stretch_id)? else { return Ok(()) };
    if before.ended_at.is_some() {
        return Ok(());
    }
    let now = Timestamp::now();
    let mut ended = before.clone();
    ended.ended_at = Some(now);
    ended.updated_at = now;
    crate::ops::emit_update(carry.tx, record::automation_run_task(&before), record::automation_run_task(&ended))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Automation, AutomationPictureOwner, AutomationPlacement, AutomationRunStatus, AutomationRunStepStatus,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN, WAIT, WHEN_NONE};
    use crate::ops::automation_report::{self, Next};
    use crate::ops::automation_run::{launch, nothing_asked, Launcher};
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::lifecycle::name::{COMMENT_ADDED, TASK_ASSIGNED, TASK_STATUS_CHANGED};
    use crate::ops::test_support::{by_ai, events_after, outbox_head};
    use crate::ops::test_support::{mk_placed, mk_project, mk_task_in, with_tx};
    use crate::store_engine::WriteTx;

    /// Take a task, look at it, and hand it back — then take the next one: the loop a run goes round when
    /// the author sends some tasks to a person rather than stopping for them.
    struct Picture {
        automation: Automation,
        take: AutomationPlacement,
        work: AutomationPlacement,
    }

    /// `waits` sets the take to wait for a task rather than end the run where there is none.
    fn picture(tx: &WriteTx<'_>, project: i64, waits: bool) -> Picture {
        let automation =
            automation::add(tx, project, NewAutomation { name: "hand back".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place take");
        let (_, work) = mk_placed(tx, &automation, "look", "look at it", "claude");
        let back = automation::placement_add(tx, automation.id, action(tx, "hand_back_task").expect("back").id)
            .expect("place hand back");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
        if waits {
            automation::cfg_set(tx, take.id, WHEN_NONE, Some(&format!("\"{WAIT}\""))).expect("wait");
        } else {
            automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        }
        automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(back.id), None).expect("work → back");
        automation::edge_add(tx, on, back.id, None, EdgeTarget::Go(take.id), None).expect("back → take");
        let automation = automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");
        Picture { automation, take, work }
    }

    fn for_ai(tx: &WriteTx<'_>, project: i64, title: &str) -> i64 {
        let id = mk_task_in(tx, title, Some(project));
        crate::ops::task::set_assignee(tx, id, Some(ActorKind::Ai), ActorKind::Ai).expect("give it to the AI");
        id
    }

    fn comments(tx: &WriteTx<'_>, task: i64) -> Vec<String> {
        read::task_comment_ids(tx.conn(), task)
            .expect("ids")
            .into_iter()
            .filter_map(|id| read::task_comment(tx.conn(), id).expect("read"))
            .map(|c| c.text)
            .collect()
    }

    /// **The task goes back to a person, and the run goes on to take the next one** — the launch finds
    /// no line that leaves the task open, the handed-back task is in `todo` with the human and the
    /// agent's report on it, and the run is still going with the next task in hand.
    #[test]
    fn it_hands_the_task_to_a_person_and_the_run_takes_the_next() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let p = picture(tx, project, false);
            let first = for_ai(tx, project, "外のもの");
            let claude = ["claude".to_string()];
            let by = Launcher {
                startable: Some(&claude),
                models: nothing_asked(),
                workspace_open: Some(true),
                by: Some(ActorKind::Ai),
            };
            let run = launch(tx, p.automation.id, &by).expect("the hand back lets the task go");
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
            assert_eq!(work.placement_id, Some(p.work.id));
            let Opened::Ready(opening) = open(tx, run.id, work.id, Some(&claude)).expect("open the work") else {
                panic!("the work opens a terminal");
            };
            let exits: Vec<crate::model::RunDefExit> =
                serde_json::from_str(&opening.run_def.exits).expect("exits");
            let done = exits.iter().find(|e| e.name == DONE_EXIT).map(|e| e.id);
            let Next::Step(back) =
                automation_report::done(tx, opening.run_step.id, done, "This is not this project's.").expect("report")
            else {
                panic!("the work goes on to the hand back");
            };
            let second = for_ai(tx, project, "中のもの");
            let head = outbox_head(tx);
            let Opened::Carried { next: Next::Step(take), .. } =
                open(tx, run.id, back.id, Some(&claude)).expect("hand back")
            else {
                panic!("a built-in is carried out, and the run goes on");
            };
            assert_eq!(take.placement_id, Some(p.take.id));
            // The person it goes back to hears of it: each write is announced as the AI's (`AMB-D-473`).
            assert_eq!(
                events_after(tx, head),
                vec![
                    by_ai(COMMENT_ADDED, None),
                    by_ai(TASK_STATUS_CHANGED, Some("todo")),
                    by_ai(TASK_ASSIGNED, Some("human")),
                ]
            );

            let handed = read::task(tx.conn(), first).expect("read").expect("task");
            assert_eq!(handed.status, TaskStatus::Todo);
            assert_eq!(handed.assignee_kind, Some(ActorKind::Human));
            assert_eq!(comments(tx, first), vec!["This is not this project's.".to_string()]);

            let Opened::Carried { next: Next::Step(_), .. } =
                open(tx, run.id, take.id, Some(&claude)).expect("take the next")
            else {
                panic!("the take goes on to the work with the next task");
            };
            assert_eq!(read::task_status(tx.conn(), second).expect("read"), Some(TaskStatus::InProgress));
            assert_eq!(read::task_status(tx.conn(), first).expect("read"), Some(TaskStatus::Todo));
            let run = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(run.status, AutomationRunStatus::Running);
            let failed = read::automation_run_steps_of(tx.conn(), run.id)
                .expect("steps")
                .into_iter()
                .any(|s| s.status == AutomationRunStepStatus::Failed);
            assert!(!failed, "nothing on the way failed");
        });
    }

    fn launched(tx: &WriteTx<'_>, p: &Picture) -> crate::model::AutomationRun {
        let claude = ["claude".to_string()];
        let by = Launcher {
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        launch(tx, p.automation.id, &by).expect("launch")
    }

    fn entry_of(tx: &WriteTx<'_>, run_id: i64) -> i64 {
        read::automation_run_defs_of(tx.conn(), run_id)
            .expect("defs")
            .into_iter()
            .find(|d| d.entry)
            .expect("the entry")
            .id
    }

    /// Launch the picture, take the one task there is, look at it, and hand it back — leaving the run
    /// before its take again. Answers the run and the take's step.
    fn handed_back(tx: &WriteTx<'_>, p: &Picture) -> (crate::model::AutomationRun, i64) {
        let claude = ["claude".to_string()];
        let run = launched(tx, p);
        let Opened::Carried { next: Next::Step(work), .. } =
            open(tx, run.id, entry_of(tx, run.id), Some(&claude)).expect("take")
        else {
            panic!("the take goes on to the work");
        };
        let Opened::Ready(opening) = open(tx, run.id, work.id, Some(&claude)).expect("open the work") else {
            panic!("the work opens a terminal");
        };
        let Next::Step(back) =
            automation_report::done(tx, opening.run_step.id, None, "Not ours.").expect("report")
        else {
            panic!("the work goes on to the hand back");
        };
        let Opened::Carried { next: Next::Step(take), .. } =
            open(tx, run.id, back.id, Some(&claude)).expect("hand back")
        else {
            panic!("the hand back goes on to the take");
        };
        (run, take.id)
    }

    /// **Another run that takes the handed-back task at once does not fail this one** — the task is no
    /// longer this run's, so coming to the end with it in progress leaves nothing open of its own.
    #[test]
    fn a_task_another_run_takes_at_once_is_not_one_this_run_left_open() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let p = picture(tx, project, false);
            let task = for_ai(tx, project, "外のもの");
            let (a, take) = handed_back(tx, &p);

            crate::ops::task::set_assignee(tx, task, Some(ActorKind::Ai), ActorKind::Human).expect("to the AI");
            let b = launched(tx, &p);
            let Opened::Carried { next: Next::Step(_), .. } =
                open(tx, b.id, entry_of(tx, b.id), Some(&["claude".to_string()])).expect("take")
            else {
                panic!("the other run takes the task");
            };
            let held = read::automation_run_task_last(tx.conn(), b.id).expect("read").expect("stretch");
            assert_eq!(held.task_id, Some(task), "the other run holds it");

            let Opened::Carried { next: Next::Closed(ended), .. } =
                open(tx, a.id, take, Some(&[])).expect("take again")
            else {
                panic!("with nothing left to take, the run comes to its end");
            };
            assert_eq!(ended.run.status, AutomationRunStatus::Completed, "{:?}", ended.run.stopped_reason);
            assert_eq!(read::task_status(tx.conn(), task).expect("read"), Some(TaskStatus::InProgress));
            let b = read::automation_run(tx.conn(), b.id).expect("read").expect("run");
            assert_eq!(b.status, AutomationRunStatus::Running, "the other run still holds it");
        });
    }

    /// **A run waiting for its next task does not fail when the one it handed back is reserved** — the
    /// take that waits is not asked about a task this run has let go of.
    #[test]
    fn a_run_waiting_to_take_is_not_failed_by_a_reservation_of_what_it_handed_back() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let p = picture(tx, project, true);
            let task = for_ai(tx, project, "外のもの");
            let (run, take) = handed_back(tx, &p);

            crate::ops::task::set_status(tx, task, TaskStatus::InProgress, ActorKind::Human)
                .expect("a session reserves it");
            match open(tx, run.id, take, Some(&[])).expect("take again") {
                Opened::Waiting { run } => assert_eq!(run.status, AutomationRunStatus::Running),
                other => panic!("with nothing to take it waits, not {other:?}"),
            }
            assert_eq!(read::task_status(tx.conn(), task).expect("read"), Some(TaskStatus::InProgress));
        });
    }
}
