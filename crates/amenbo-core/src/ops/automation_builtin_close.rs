//! **The built-in that closes a task** — mark the task the run is on done (`AMB-D-964`).
//!
//! What a prompt used to be asked to do with `task commit add` and `task done`, done here in the
//! transaction the step is opened in. A step that forgot either left the task in progress with no run
//! holding it; this one cannot forget.
//!
//! **The commit arrives as a value** on [`COMMIT`]. Amenbo cannot know what was merged, so the step
//! that merged it hands the SHA on, and it is recorded as `task commit add` records one. Nothing on
//! that input is not a failure: a task can be closed with no commit to show for it.
//!
//! **The report the task keeps is the last one an agent gave** in this stretch — the step that did
//! the work says what was done, and a built-in has nothing to add to it (`AMB-D-963`). It is written
//! before the task is closed, since a closed task takes no comment, and it is not written twice where
//! that step already carried it onto the task.
//!
//! It leaves by the done way out once the task is closed. A task already done is left as it is and
//! leaves the same way; one decided against, or no task at all, leaves by the error way out.

use crate::error::{Error, Result};
use crate::model::{
    ActorKind, AutomationPortKind, AutomationRunStep, AutomationRunStepStatus, TaskStatus, DONE_EXIT,
};
use crate::ops::automation_builtin::{Builtin, BuiltinExit, BuiltinPort, Carried, Carry, Work};
use crate::run_wording::builtin as say;
use crate::store_engine::{read, WriteTx};

/// The input the commit's SHA is handed in on.
pub const COMMIT: &str = "コミット";

pub(crate) const CLOSE_TASK: Builtin = Builtin {
    key: "close_task",
    name: "タスクを閉じる",
    does: "いま扱っているタスクを完了にする。コミットを受け取ったら、その SHA も記録する",
    settings: &[],
    ins: &[BuiltinPort { name: COMMIT, kind: AutomationPortKind::Value, required: false }],
    exits: &[BuiltinExit { name: DONE_EXIT, outs: &[] }],
    waits: None,
    chooses: None,
    work: Work::InStore(close),
};

fn close(carry: &Carry<'_, '_>) -> Result<Carried> {
    let tx = carry.tx;
    let lang = tx.language();
    let task_id = carry.task_id.ok_or_else(|| Error::invalid(say(lang, "noTaskToClose", &[])))?;
    let task = read::task(tx.conn(), task_id)?
        .ok_or_else(|| Error::not_found(format!("task AMB-T-{task_id}")))?;
    let named = format!("AMB-T-{task_id}");
    match task.status {
        TaskStatus::Done => {
            return Ok(Carried { exit: DONE_EXIT, report: say(lang, "doneAlready", &[("task", &named)]) });
        }
        TaskStatus::Rejected => {
            return Err(Error::invalid(say(lang, "rejected", &[("task", &named)])));
        }
        _ => {}
    }
    let mut recorded = None;
    if let Some(sha) = carry.input(COMMIT).map(str::trim).filter(|s| !s.is_empty()) {
        // The door's refusal is the CLI's sentence; the one the run keeps is the reader's.
        let (commit, _) = crate::ops::commit::add(tx, task_id, sha, Some(ActorKind::Ai)).map_err(|e| {
            match e.code() {
                "invalid_commit_sha" => Error::invalid(say(lang, "badSha", &[("sha", sha)])),
                _ => e,
            }
        })?;
        recorded = Some(commit.sha);
    }
    if let Some(reported) = last_report(tx, carry.run_step)? {
        if !already_on_the_task(tx, task_id, reported.id)? {
            crate::ops::comment::add_report_comment(tx, task_id, ActorKind::Ai, &reported.report, reported.id)?;
        }
    }
    crate::ops::task::set_completed(tx, task_id, true)?;
    let report = match recorded {
        Some(sha) => say(lang, "closedRecorded", &[("task", &named), ("title", &task.title), ("sha", &sha)]),
        None => say(lang, "closed", &[("task", &named), ("title", &task.title)]),
    };
    Ok(Carried { exit: DONE_EXIT, report })
}

/// **The last report an agent gave in this stretch**, or `None` where no agent's step has reported
/// anything yet. A built-in's own line is the run's record of it, not a report on the work.
fn last_report(tx: &WriteTx<'_>, this: &AutomationRunStep) -> Result<Option<AutomationRunStep>> {
    let Some(stretch) = this.run_task_id else {
        return Ok(None);
    };
    let mut last = None;
    for execution in read::automation_run_steps_of_task(tx.conn(), stretch)? {
        if execution.id == this.id
            || execution.status != AutomationRunStepStatus::Done
            || execution.report.trim().is_empty()
        {
            continue;
        }
        let by_an_agent = read::automation_run_def(tx.conn(), execution.run_def_id)?
            .is_some_and(|def| def.builtin.is_none());
        if by_an_agent {
            last = Some(execution);
        }
    }
    Ok(last)
}

/// Whether that step's report is on the task already — carried there by the step itself, which was
/// built to report to the task.
fn already_on_the_task(tx: &WriteTx<'_>, task_id: i64, run_step_id: i64) -> Result<bool> {
    for id in read::task_comment_ids(tx.conn(), task_id)? {
        if read::task_comment(tx.conn(), id)?.is_some_and(|c| c.automation_run_step_id == Some(run_step_id)) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Automation, AutomationPictureOwner, AutomationPlacement, AutomationRun, AutomationRunStatus,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::automation_report::{self, Next, Produced};
    use crate::ops::automation_run::{launch, nothing_asked, Launcher};
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::ops::test_support::{mk_out, mk_placed, mk_project, mk_task_in, only_step, out_port, with_tx};

    /// Take a task, work on it — handing the commit on — and close it: the line a run walks for each
    /// task, and the one `AMB-D-964` puts this built-in at the end of.
    struct Picture {
        automation: Automation,
        work: AutomationPlacement,
        work_step: i64,
    }

    fn picture(tx: &WriteTx<'_>, project: i64) -> Picture {
        let automation =
            automation::add(tx, project, NewAutomation { name: "close".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place take");
        let (work_action, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        mk_out(tx, &work_action, None, COMMIT, AutomationPortKind::Value, false);
        let close = automation::placement_add(tx, automation.id, action(tx, "close_task").expect("close").id)
            .expect("place close");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(close.id), None).expect("work → close");
        automation::edge_add(tx, on, close.id, None, EdgeTarget::Done, None).expect("close → done");
        automation::wire_add(tx, on, work.id, None, COMMIT, close.id, COMMIT).expect("wire the commit");
        let automation = automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");
        let work_step = only_step(tx, &work_action).id;
        Picture { automation, work, work_step }
    }

    fn for_ai(tx: &WriteTx<'_>, project: i64) -> i64 {
        let id = mk_task_in(tx, "直すもの", Some(project));
        crate::ops::task::set_assignee(tx, id, Some(ActorKind::Ai)).expect("give it to the AI");
        id
    }

    /// Launch, take the task, and open the agent's step — which the test reports for, as the agent
    /// would — and answer where the run went from there.
    fn walk(tx: &WriteTx<'_>, p: &Picture, commit: Option<&str>, report: &str) -> (AutomationRun, Next) {
        let claude = ["claude".to_string()];
        let by = Launcher {
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        let run = launch(tx, p.automation.id, &by).expect("launch");
        let entry = read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.entry)
            .expect("the entry");
        let Opened::Carried { next: Next::Step(work), .. } = open(tx, run.id, entry.id, Some(&claude)).expect("take")
        else {
            panic!("the take goes on to the work");
        };
        assert_eq!(work.placement_id, Some(p.work.id));
        let Opened::Ready(opening) = open(tx, run.id, work.id, Some(&claude)).expect("open the work") else {
            panic!("the work opens a terminal");
        };
        let run_step = opening.run_step.id;
        if let Some(sha) = commit {
            let port = out_port(tx, run_step, None, COMMIT);
            automation_report::out(tx, run_step, port, Produced::Value(sha)).expect("hand the commit on");
        }
        let exits: Vec<crate::model::RunDefExit> = serde_json::from_str(&opening.run_def.exits).expect("exits");
        let done = exits.iter().find(|e| e.name == DONE_EXIT).map(|e| e.id);
        let Next::Step(close) = automation_report::done(tx, run_step, done, report).expect("report") else {
            panic!("the work goes on to the close");
        };
        let Opened::Carried { next, .. } = open(tx, run.id, close.id, Some(&claude)).expect("close") else {
            panic!("a built-in is carried out");
        };
        (run, next)
    }

    fn comments(tx: &WriteTx<'_>, task: i64) -> Vec<String> {
        read::task_comment_ids(tx.conn(), task)
            .expect("ids")
            .into_iter()
            .filter_map(|id| read::task_comment(tx.conn(), id).expect("read"))
            .map(|c| c.text)
            .collect()
    }

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    /// **It closes the task, records the commit handed to it, and leaves the agent's report on it** —
    /// and the run, with its task closed, ends as completed.
    #[test]
    fn it_closes_the_task_with_the_commit_and_the_agent_s_report() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let p = picture(tx, project);
            let task = for_ai(tx, project);

            let (run, next) = walk(tx, &p, Some(SHA), "Fixed it.\n\nThe test covers it.");
            assert!(matches!(next, Next::Closed(_)), "{next:?}");
            assert_eq!(read::task_status(tx.conn(), task).expect("read"), Some(TaskStatus::Done));
            let shas: Vec<String> =
                read::task_commits(tx.conn(), task).expect("commits").into_iter().map(|c| c.sha).collect();
            assert_eq!(shas, vec![SHA.to_string()]);
            assert_eq!(comments(tx, task), vec!["Fixed it.\n\nThe test covers it.".to_string()]);
            let run = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(run.status, AutomationRunStatus::Completed);
        });
    }

    /// **No commit handed on is no failure** — a task can be closed with nothing merged.
    #[test]
    fn with_no_commit_it_closes_the_task_all_the_same() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let p = picture(tx, project);
            let task = for_ai(tx, project);

            let (_, next) = walk(tx, &p, None, "Nothing to merge.");
            assert!(matches!(next, Next::Closed(_)), "{next:?}");
            assert_eq!(read::task_status(tx.conn(), task).expect("read"), Some(TaskStatus::Done));
            assert!(read::task_commits(tx.conn(), task).expect("commits").is_empty());
        });
    }

    /// **A report the step carried onto the task itself is not written there twice.**
    #[test]
    fn a_report_already_on_the_task_is_not_written_again() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let p = picture(tx, project);
            automation::step_update(tx, p.work_step, None, None, None, None, Some(true), None, None, None, None)
                .expect("report to the task");
            let task = for_ai(tx, project);

            walk(tx, &p, None, "Fixed it.");
            assert_eq!(comments(tx, task), vec!["Fixed it.".to_string()]);
            assert_eq!(read::task_status(tx.conn(), task).expect("read"), Some(TaskStatus::Done));
        });
    }

    /// **A commit that is not a SHA is refused**, and the step leaves by the error way out rather than
    /// close the task with the wrong thing recorded on it.
    #[test]
    fn a_commit_that_is_no_sha_fails_the_step_and_does_not_close_the_task() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let p = picture(tx, project);
            let task = for_ai(tx, project);

            let (_, next) = walk(tx, &p, Some("main"), "Merged.");
            assert!(matches!(next, Next::Halted(_)), "nothing follows the error way out: {next:?}");
            assert_ne!(read::task_status(tx.conn(), task).expect("read"), Some(TaskStatus::Done));
            assert!(read::task_commits(tx.conn(), task).expect("commits").is_empty());
        });
    }
}
