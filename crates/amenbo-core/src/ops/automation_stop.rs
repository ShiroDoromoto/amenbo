//! **Stopping a run** — pausing it, picking it up again, ending it, and the one cleanup every one of
//! those goes through.
//!
//! **A run ends by exactly one road** ([`ended`]), and says which of three endings it was
//! ([`Ending`], `AMB-D-955`): completed, failed or canceled. Every ending but completed owes the same
//! two acts: release the task the run was holding, and leave a line on that task saying what became of
//! it. A road per ending would have been a place per ending for one of the two to be forgotten.
//!
//! **Pausing is a request, not a stop.** A step under way cannot be cut in half — it is an agent in a
//! terminal, mid-sentence — so pressing pause writes `pause_requested` and the run goes on until that
//! step reports. What reads the flag is [`crate::ops::automation_report::done`], which is the only
//! place that knows a step has finished.
//!
//! **Paused holds its task.** The work is half done and nobody else should take it. Stopping does the
//! opposite — hands the task back to `todo` — because a run that was cut off left no one carrying it.
//!
//! **Nothing here watches for a crash.** A process that died says nothing, and core has no window to
//! ask; what it has is the fact that a run marked `running` cannot be running in a process that no
//! longer exists. So the check is made once, at startup ([`sweep`]), by whoever opens the store.

use crate::error::{Error, Result};
use rusqlite::Connection;
use crate::model::{
    ActorKind, AutomationRun, AutomationRunDef, AutomationRunStatus, AutomationRunTask,
    AutomationStoppedReason, TaskStatus,
};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// **A run that has stopped.**
#[derive(Clone, Debug)]
pub struct Ended {
    pub run: AutomationRun,
}

/// **How a run ended** (`AMB-D-955`) — the status it lands on, and the reason only a failure carries.
///
/// One value rather than a status and a reason side by side, so that a reason on anything but a failure,
/// or a failure with none, is not something a caller can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// The picture ran out.
    Completed,
    /// It could not get to the end of the picture, for this reason.
    Failed(AutomationStoppedReason),
    /// A person said stop — pressed it, or closed the run's pane.
    Canceled,
}

impl Ending {
    pub fn status(self) -> AutomationRunStatus {
        match self {
            Ending::Completed => AutomationRunStatus::Completed,
            Ending::Failed(_) => AutomationRunStatus::Failed,
            Ending::Canceled => AutomationRunStatus::Canceled,
        }
    }

    pub fn reason(self) -> Option<AutomationStoppedReason> {
        match self {
            Ending::Failed(reason) => Some(reason),
            Ending::Completed | Ending::Canceled => None,
        }
    }

    /// Whether the run was cut off before the picture ran out — the endings that hand the task back.
    fn cut_short(self) -> bool {
        !matches!(self, Ending::Completed)
    }
}

/// What pressing pause did.
#[derive(Clone, Debug)]
pub enum Paused {
    /// A step is under way. The run is still `running` and stops at the end of it.
    Asked(AutomationRun),
    /// Nothing was under way, so it is `paused` already.
    Now(Ended),
}

/// **What picking a paused run up again did** — the run, now `running`, and the step it picks up at.
///
/// `next` is the answer this walked the picture for, handed back so the caller can say so. Opening it
/// is nobody's here: the run is `running` with nothing open, which is what the watch acts on
/// (`AMB-D-945`).
#[derive(Clone, Debug)]
pub struct Resumed {
    pub run: AutomationRun,
    pub next: Box<AutomationRunDef>,
}

/// `<what> '<id>' not found`, the uncoded refusal the automation entities take
/// ([`crate::ops::automation`] says why).
fn not_found(what: &str, id: i64) -> Error {
    Error::not_found(format!("{what} '{id}' not found"))
}

fn live_run(tx: &WriteTx<'_>, run_id: i64) -> Result<AutomationRun> {
    read::automation_run(tx.conn(), run_id)?.ok_or_else(|| not_found("run", run_id))
}

/// Whether a run is still going — the states a pause or a stop has anything to act on.
fn under_way(status: AutomationRunStatus) -> bool {
    matches!(status, AutomationRunStatus::Running | AutomationRunStatus::Paused)
}

// ───────────────────────────── the one cleanup ─────────────────────────────

/// **End a run and hand back the task it was holding.** Every way a run can end comes through here.
///
/// **The task goes back to `todo` only when the run was cut short** — failed or canceled. A run that
/// completed left its task wherever its steps put it, which is the outcome somebody asked for; a run
/// that was cut off left it reserved by nobody, and a task held by a run that is gone is one no session
/// will ever pick up.
pub fn ended(tx: &WriteTx<'_>, before: AutomationRun, ending: Ending) -> Result<Ended> {
    let now = Timestamp::now();
    let mut after = before.clone();
    after.status = ending.status();
    after.stopped_reason = ending.reason();
    after.pause_requested = false;
    after.ended_at = Some(now);
    after.updated_at = now;
    crate::ops::emit_update(tx, record::automation_run(&before), record::automation_run(&after))?;

    let stretch = close_stretch(tx, before.id, now)?;
    if ending.cut_short() {
        hand_the_task_back(tx, &after, stretch.as_ref(), ending)?;
    }
    Ok(Ended { run: after })
}

/// Close the stretch the run was in, and answer which one it was. A stretch already closed is left
/// alone: a run that ended between two tasks has nothing open.
fn close_stretch(
    tx: &WriteTx<'_>,
    run_id: i64,
    now: Timestamp,
) -> Result<Option<AutomationRunTask>> {
    let Some(before) = read::automation_run_task_last(tx.conn(), run_id)? else {
        return Ok(None);
    };
    if before.ended_at.is_some() {
        return Ok(Some(before));
    }
    let mut ended = before.clone();
    ended.ended_at = Some(now);
    ended.updated_at = now;
    crate::ops::emit_update(
        tx,
        record::automation_run_task(&before),
        record::automation_run_task(&ended),
    )?;
    Ok(Some(ended))
}

/// Give the task the run was working back, and say on it what became of the run.
///
/// **Only a task the run itself reserved** is handed back: a step takes its task from `todo` to
/// `in_progress` ([`crate::ops::automation_report::take`]), and anything else it is in by now was put
/// there by the work rather than by the reservation. A task somebody finished mid-run does not reopen
/// because the run was then stopped.
///
/// The line is left whatever the task's state, because what a person needs is to know the run is not
/// coming back — and where it got to, so they can judge what is half done.
fn hand_the_task_back(
    tx: &WriteTx<'_>,
    run: &AutomationRun,
    stretch: Option<&AutomationRunTask>,
    ending: Ending,
) -> Result<()> {
    let Some(task_id) = stretch.and_then(|s| s.task_id) else { return Ok(()) };
    if read::task_status(tx.conn(), task_id)? == Some(TaskStatus::InProgress) {
        crate::ops::task::set_status(tx, task_id, TaskStatus::Todo)?;
    }
    crate::ops::comment::add_comment(tx, task_id, ActorKind::Ai, &said(tx, run, ending)?)?;
    Ok(())
}

/// What the line on the task says: that the run stopped, why, and how far it got.
///
/// The run's id is in it because that is the only handle a person has on a run from a task — the two
/// are not linked by a column, and a task that has been round three runs would otherwise say only that
/// one of them stopped.
fn said(tx: &WriteTx<'_>, run: &AutomationRun, ending: Ending) -> Result<String> {
    let walked = read::automation_run_steps_of(tx.conn(), run.id)?;
    let reached = walked
        .last()
        .map(|step| match read::automation_run_def(tx.conn(), step.run_def_id) {
            Ok(Some(def)) => format!("It got as far as \"{}\".", def.name),
            _ => "It got as far as a step that is no longer there.".to_string(),
        })
        .unwrap_or_else(|| "It stopped before opening a step.".to_string());
    Ok(format!("{} {reached} (run {})", why(ending), run.id))
}

/// The first sentence of that line: what ended the run, in the words each ending is worth.
fn why(ending: Ending) -> &'static str {
    match ending {
        Ending::Failed(AutomationStoppedReason::Crashed) => {
            "An automation run failed: Amenbo was restarted while it was under way."
        }
        Ending::Failed(AutomationStoppedReason::MaxTimes) => {
            "An automation run failed: a way back was taken as often as it is allowed to be."
        }
        Ending::Failed(AutomationStoppedReason::NoAgent) => {
            "An automation run failed: the agent a step asked for could not be started."
        }
        Ending::Failed(AutomationStoppedReason::NoInput) => {
            "An automation run failed: a step's required input had nothing to fill it."
        }
        Ending::Failed(AutomationStoppedReason::NoWayOn) => {
            "An automation run failed: nothing was left for it to open."
        }
        Ending::Failed(AutomationStoppedReason::Halted) => {
            "An automation run stopped to call a person: a step left through a way out that asks for one."
        }
        Ending::Canceled => "An automation run was canceled.",
        Ending::Completed => "An automation run completed.",
    }
}

// ───────────────────────────── pause, resume, stop ─────────────────────────────

/// **Ask a run to pause.** It stops at the end of the step under way, not in the middle of one.
pub fn pause(tx: &WriteTx<'_>, run_id: i64) -> Result<Paused> {
    let before = live_run(tx, run_id)?;
    match before.status {
        AutomationRunStatus::Running => {
            if before.pause_requested {
                return Ok(Paused::Asked(before));
            }
            let now = Timestamp::now();
            let mut after = before.clone();
            after.pause_requested = true;
            after.updated_at = now;
            crate::ops::emit_update(
                tx,
                record::automation_run(&before),
                record::automation_run(&after),
            )?;
            Ok(Paused::Asked(after))
        }
        AutomationRunStatus::Paused => Ok(Paused::Now(Ended { run: before })),
        other => Err(Error::invalid(format!(
            "run '{run_id}' is {}, and only a run still going can be paused",
            other.as_str()
        ))),
    }
}

/// Put a run into `paused`: it keeps the task it is working.
///
/// It does not go through [`ended`], and the difference is the whole point — a paused run has not
/// ended. `ended_at` stays empty, the task stays reserved, and nothing is written on it, because there
/// is nothing to tell somebody yet.
pub fn settle(tx: &WriteTx<'_>, before: AutomationRun) -> Result<Ended> {
    let now = Timestamp::now();
    let mut after = before.clone();
    after.status = AutomationRunStatus::Paused;
    after.pause_requested = false;
    after.updated_at = now;
    crate::ops::emit_update(tx, record::automation_run(&before), record::automation_run(&after))?;
    Ok(Ended { run: after })
}

/// **Pick a paused run up again**, from the way out the last step left through.
///
/// The picture is walked live, the way every other walk of it is: which step comes next is the shape of
/// the automation rather than anything the pause wrote down. A run whose last step left through a way
/// out that now decides nothing is stopped rather than resumed — the same answer the report door gives,
/// since the picture was edited underneath it either way.
pub fn resume(tx: &WriteTx<'_>, run_id: i64) -> Result<Resumed> {
    let before = live_run(tx, run_id)?;
    if before.status != AutomationRunStatus::Paused {
        return Err(Error::invalid(format!(
            "run '{run_id}' is {}, and only a paused one is picked up again",
            before.status.as_str()
        )));
    }
    let Some(next) = next_after_the_pause(tx.conn(), &before)? else {
        ended(tx, before, Ending::Failed(AutomationStoppedReason::NoWayOn))?;
        return Err(Error::invalid(format!(
            "run '{run_id}' cannot go on: what followed the step it paused after is no longer in the \
             picture, so it has failed"
        )));
    };
    let now = Timestamp::now();
    let mut after = before.clone();
    after.status = AutomationRunStatus::Running;
    after.updated_at = now;
    if after.started_at.is_none() {
        after.started_at = Some(now);
    }
    crate::ops::emit_update(tx, record::automation_run(&before), record::automation_run(&after))?;
    Ok(Resumed { run: after, next: Box::new(next) })
}

/// The step a paused run opens next: the one the last finished step's way out leads to.
fn next_after_the_pause(
    conn: &Connection,
    run: &AutomationRun,
) -> Result<Option<AutomationRunDef>> {
    let Some(last) = read::automation_run_steps_of(conn, run.id)?.pop() else { return Ok(None) };
    let Some(def) = read::automation_run_def(conn, last.run_def_id)? else { return Ok(None) };
    Ok(match crate::ops::automation_run::onward(conn, &def, last.exit_name.as_deref())? {
        crate::ops::automation_run::Onward::Go { def, .. } => Some(*def),
        _ => None,
    })
}

/// **Stop a run now** — the terminal is closed wherever it is, and the cleanup runs.
///
/// This is what a person presses when pausing will not do, and what closing a run's pane means — both
/// [`Ending::Canceled`]. It is also the door the watch comes through when a run has nowhere left to go,
/// naming the failure. A run is not completed from here: that is the picture running out.
pub fn stop(tx: &WriteTx<'_>, run_id: i64, ending: Ending) -> Result<Ended> {
    if ending == Ending::Completed {
        return Err(Error::invalid(format!(
            "run '{run_id}' is completed by running out of picture, not by being stopped"
        )));
    }
    let before = live_run(tx, run_id)?;
    if !under_way(before.status) {
        return Err(Error::invalid(format!(
            "run '{run_id}' is {} — it is over already",
            before.status.as_str()
        )));
    }
    ended(tx, before, ending)
}

/// **Say a failed run has been seen** (`AMB-D-955`). It leaves the top of the runs tab for the
/// history under it; nothing else about the run changes.
///
/// Only a failure is acknowledged: a completed or canceled run needs nobody, and one still going is
/// not over. Acknowledging one that already is answers it as it stands rather than moving the time.
pub fn acknowledge(tx: &WriteTx<'_>, run_id: i64) -> Result<AutomationRun> {
    let before = live_run(tx, run_id)?;
    if before.status != AutomationRunStatus::Failed {
        return Err(Error::invalid(format!(
            "run '{run_id}' is {}, and only a failed run is acknowledged",
            before.status.as_str()
        )));
    }
    if before.acknowledged_at.is_some() {
        return Ok(before);
    }
    let now = Timestamp::now();
    let mut after = before.clone();
    after.acknowledged_at = Some(now);
    after.updated_at = now;
    crate::ops::emit_update(tx, record::automation_run(&before), record::automation_run(&after))?;
    Ok(after)
}

/// **The runs this machine was in the middle of when it last shut down.**
///
/// Run at startup, before anything else touches a run. Every `running` row is one from a process that
/// is gone: a terminal does not outlive the app that drew it. Each fails as a crash, which hands back
/// its task and says so on it.
///
/// It answers what it stopped, in id order, so a face can say how many runs did not survive the
/// restart rather than leaving somebody to notice on their own.
pub fn sweep(tx: &WriteTx<'_>) -> Result<Vec<AutomationRun>> {
    let caught = read::automation_run_ids_running(tx.conn())?;
    let mut stopped = Vec::with_capacity(caught.len());
    for id in caught {
        let Some(run) = read::automation_run(tx.conn(), id)? else { continue };
        stopped.push(
            ended(tx, run, Ending::Failed(AutomationStoppedReason::Crashed))?.run,
        );
    }
    Ok(stopped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Automation, AutomationPortKind};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_report::{done, Next};
    use crate::ops::automation_run::{launch, Launcher};
    use crate::ops::automation_step::{open, Opened, Opening};
    use crate::ops::test_support::{mk_exit, mk_out, mk_placed, mk_project, mk_task_in, with_tx};


    /// The picture these tests walk: a spot that takes a task and goes on to a second, and the second
    /// closing the run. `back` gives the second a way round to itself, capped at one turn — a way back
    /// into the spot that takes a task would begin a fresh stretch, which is the one case the limit
    /// deliberately does not count.
    struct Picture {
        automation: Automation,
        project: i64,
        first: crate::model::AutomationPlacement,
        second: crate::model::AutomationPlacement,
    }

    fn picture(tx: &WriteTx<'_>, back: bool) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(
            tx,
            project,
            NewAutomation { name: "1件やりきる".into(), ..Default::default() },
        )
        .expect("add automation");
        let (first_action, first) = mk_placed(tx, &automation, "調べる", "look", "claude");
        let (second_action, second) = mk_placed(tx, &automation, "直す", "fix", "claude");
        mk_out(tx, &first_action, None, "タスク", AutomationPortKind::TaskTake, true);
        let on = crate::model::AutomationPictureOwner::Automation;
        automation::edge_add(tx, on, first.id, None, EdgeTarget::Go(second.id), None)
            .expect("onward");
        if back {
            mk_exit(tx, &second_action, "again");
            automation::edge_add(
                tx,
                on,
                second.id,
                Some("again"),
                EdgeTarget::Go(second.id),
                Some(1),
            )
            .expect("back");
        }
        automation::edge_add(tx, on, second.id, None, EdgeTarget::Done, None).expect("closes");
        let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, project, first, second }
    }

    fn a_run(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let startable = vec!["claude".to_string()];
        let by = Launcher {
            startable: Some(&startable),
            models: crate::ops::automation_run::nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        launch(tx, automation.id, &by).expect("launch")
    }

    fn def_of(
        tx: &WriteTx<'_>,
        run: &AutomationRun,
        placement: &crate::model::AutomationPlacement,
    ) -> AutomationRunDef {
        read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.placement_id == Some(placement.id))
            .expect("the spot's snapshot")
    }

    fn opened(
        tx: &WriteTx<'_>,
        run: &AutomationRun,
        placement: &crate::model::AutomationPlacement,
    ) -> Opening {
        match open(tx, run.id, def_of(tx, run, placement).id, None).expect("open") {
            Opened::Ready(opening) => *opening,
            Opened::Stopped { missing, .. } => panic!("stopped for {missing:?}"),
            Opened::NoAgent { agent, .. } => panic!("cannot start {agent}"),
        }
    }

    /// A task of the project, taken by the step that is under way — what a stop has to hand back.
    fn a_task_in_hand(tx: &WriteTx<'_>, project: i64, run_step_id: i64) -> i64 {
        let task = mk_task_in(tx, "見る", Some(project));
        crate::ops::automation_report::take(tx, run_step_id, task).expect("take");
        task
    }

    /// What has been said on a task, in the order it was said.
    fn comments_on(tx: &WriteTx<'_>, task_id: i64) -> Vec<String> {
        read::task_comment_ids(tx.conn(), task_id)
            .expect("ids")
            .into_iter()
            .filter_map(|id| read::task_comment(tx.conn(), id).expect("comment"))
            .map(|c| c.text)
            .collect()
    }

    fn status_of(tx: &WriteTx<'_>, run_id: i64) -> AutomationRunStatus {
        read::automation_run(tx.conn(), run_id).expect("read").expect("the run").status
    }

    #[test]
    fn the_running_index_holds_what_is_not_over_and_the_stops_behind_it() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let running = a_run(tx, &p.automation);
            let paused = a_run(tx, &p.automation);
            let stopped = a_run(tx, &p.automation);
            settle(tx, paused.clone()).expect("pause");
            ended(tx, stopped.clone(), Ending::Failed(AutomationStoppedReason::Crashed)).expect("fail");
            let live: Vec<i64> = read::automation_runs_live(tx.conn())
                .expect("live")
                .into_iter()
                .map(|one| one.id)
                .collect();
            assert!(live.contains(&running.id) && live.contains(&paused.id) && live.contains(&stopped.id));
            // Under way first and newest first within that, then the failures — which is the order the
            // tab draws, without a screen sorting what it was handed.
            let over = live.iter().position(|id| *id == stopped.id).expect("the failure is on it");
            assert_eq!(over, live.len() - 1, "a failure sits behind everything still going");

            // What is over and needs nobody is not on it: it is the history's.
            ended(tx, running.clone(), Ending::Completed).expect("completed");
            let after = read::automation_runs_live(tx.conn()).expect("live");
            assert!(after.iter().all(|one| one.id != running.id));
            let history = read::automation_runs_history(tx.conn(), 20).expect("history");
            assert_eq!(history.iter().map(|one| one.id).collect::<Vec<_>>(), vec![running.id]);
        });
    }

    /// What the app ending leaves behind, and what the next launch does with it.
    ///
    /// **Every run that was going is caught, not just the one with a task in hand.** Each was waiting
    /// for a terminal in a window that is gone.
    #[test]
    fn a_launch_stops_every_run_the_last_one_left_standing_and_hands_their_tasks_back() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let going = a_run(tx, &p.automation);
            let other = a_run(tx, &p.automation);
            assert_eq!(status_of(tx, going.id), AutomationRunStatus::Running);
            assert_eq!(status_of(tx, other.id), AutomationRunStatus::Running);
            let opening = opened(tx, &going, &p.first);
            let task = a_task_in_hand(tx, p.automation.project_id, opening.run_step.id);

            let swept = sweep(tx).expect("sweep");
            assert_eq!(swept.len(), 2, "both were going when the window went");

            for run in [&going, &other] {
                assert_eq!(status_of(tx, run.id), AutomationRunStatus::Failed);
            }
            assert_eq!(
                read::automation_run(tx.conn(), going.id).expect("read").expect("it").stopped_reason,
                Some(AutomationStoppedReason::Crashed),
            );
            // The task the run was holding is back where somebody can pick it up, and it says why.
            assert_eq!(
                read::task(tx.conn(), task).expect("read").expect("the task").status,
                crate::model::TaskStatus::Todo,
            );
            assert!(!comments_on(tx, task).is_empty(), "and it was told what happened");
        });
    }

    #[test]
    fn pausing_a_running_run_waits_for_the_step_under_way() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, step.run_step.id);

            let asked = pause(tx, run.id).expect("pause");
            assert!(matches!(asked, Paused::Asked(_)), "a step is under way");
            assert_eq!(status_of(tx, run.id), AutomationRunStatus::Running, "not cut in half");

            let next = done(tx, step.run_step.id, None, "Looked at it.").expect("done");
            assert!(matches!(next, Next::Paused(_)), "the pause is answered at the end of the step");
            assert_eq!(status_of(tx, run.id), AutomationRunStatus::Paused);
            assert!(
                read::automation_run_ids_running(tx.conn()).expect("running").is_empty(),
                "and it is no longer one of the runs that are going",
            );
        });
    }

    #[test]
    fn a_paused_run_keeps_the_task_it_was_working() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task_in_hand(tx, p.project, step.run_step.id);
            pause(tx, run.id).expect("pause");
            done(tx, step.run_step.id, None, "Looked at it.").expect("done");
            assert_eq!(
                read::task_status(tx.conn(), task).expect("read"),
                Some(TaskStatus::InProgress),
                "the work is half done and nobody else should take it",
            );
        });
    }

    #[test]
    fn resuming_opens_the_step_after_the_one_it_paused_on() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, step.run_step.id);
            pause(tx, run.id).expect("pause");
            done(tx, step.run_step.id, None, "Looked at it.").expect("done");

            let Resumed { run: after, next } = resume(tx, run.id).expect("resume");
            assert_eq!(after.status, AutomationRunStatus::Running);
            assert_eq!(next.step_id, Some(p.second.id), "it goes on where it left off");
        });
    }

    #[test]
    fn stopping_hands_the_task_back_and_says_so_on_it() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task_in_hand(tx, p.project, step.run_step.id);

            let after = stop(tx, run.id, Ending::Canceled).expect("stop");
            assert_eq!(after.run.status, AutomationRunStatus::Canceled);
            assert_eq!(after.run.stopped_reason, None, "a person who said stop needs no reason");
            assert_eq!(
                read::task_status(tx.conn(), task).expect("read"),
                Some(TaskStatus::Todo),
                "a task held by a run that is gone is one nobody picks up",
            );
            let said = comments_on(tx, task);
            assert_eq!(said.len(), 1, "one line, saying the run is not coming back");
            assert!(said[0].contains("調べる"), "{}", said[0]);
            assert!(said[0].contains(&format!("run {}", run.id)), "{}", said[0]);
        });
    }

    #[test]
    fn a_run_that_reached_the_end_leaves_its_task_where_the_steps_put_it() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let first = opened(tx, &run, &p.first);
            let task = a_task_in_hand(tx, p.project, first.run_step.id);
            done(tx, first.run_step.id, None, "Looked at it.").expect("done");
            let second = opened(tx, &run, &p.second);
            done(tx, second.run_step.id, None, "Fixed it.").expect("done");

            assert_eq!(status_of(tx, run.id), AutomationRunStatus::Completed);
            assert_eq!(
                read::task_status(tx.conn(), task).expect("read"),
                Some(TaskStatus::InProgress),
                "the outcome is what the steps made of it, not what the ending does to it",
            );
            assert!(
                comments_on(tx, task).is_empty(),
                "nothing went wrong, so there is nothing to tell anybody",
            );
        });
    }

    #[test]
    fn a_restart_stops_every_run_that_was_going() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let one = a_run(tx, &p.automation);
            let another = a_run(tx, &p.automation);

            let caught = sweep(tx).expect("sweep");
            assert_eq!(caught.len(), 2, "a terminal does not outlive the app that drew it");
            assert_eq!(status_of(tx, one.id), AutomationRunStatus::Failed);
            assert_eq!(status_of(tx, another.id), AutomationRunStatus::Failed);
            assert!(caught.iter().all(|r| r.stopped_reason
                == Some(AutomationStoppedReason::Crashed)));
        });
    }

    #[test]
    fn a_way_back_taken_more_often_than_it_may_stops_the_run() {
        with_tx(|tx| {
            let p = picture(tx, true);
            let run = a_run(tx, &p.automation);
            let first = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, first.run_step.id);
            done(tx, first.run_step.id, None, "Looked at it.").expect("done");

            // Round once: the way back is within its one turn.
            let second = opened(tx, &run, &p.second);
            let next = done(tx, second.run_step.id, Some("again"), "Not yet.").expect("done");
            assert!(matches!(next, Next::Step(_)), "one turn is what it is allowed");

            // Round twice: the same way out, the same task — one turn too many.
            let twice = opened(tx, &run, &p.second);
            let stopped = done(tx, twice.run_step.id, Some("again"), "Still not.")
                .expect("done");
            assert!(matches!(stopped, Next::Halted(_)));
            assert_eq!(
                read::automation_run(tx.conn(), run.id).expect("read").expect("run").stopped_reason,
                Some(AutomationStoppedReason::MaxTimes),
            );
        });
    }

    /// A failure leaves the top of the tab once somebody says they saw it, and lands in the history.
    /// Nothing but a failure can be marked, and marking it twice keeps the first time.
    #[test]
    fn a_failure_someone_has_seen_moves_to_the_history() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let failed = a_run(tx, &p.automation);
            ended(tx, failed.clone(), Ending::Failed(AutomationStoppedReason::Crashed)).expect("fail");
            let canceled = a_run(tx, &p.automation);
            stop(tx, canceled.id, Ending::Canceled).expect("cancel");

            let live = read::automation_runs_live(tx.conn()).expect("live");
            assert!(live.iter().any(|one| one.id == failed.id), "a failure waits to be seen");
            assert!(live.iter().all(|one| one.id != canceled.id), "a cancel needs nobody");

            let seen = acknowledge(tx, failed.id).expect("acknowledge");
            assert!(seen.acknowledged_at.is_some());
            let live = read::automation_runs_live(tx.conn()).expect("live");
            assert!(live.iter().all(|one| one.id != failed.id));
            let history: Vec<i64> = read::automation_runs_history(tx.conn(), 20)
                .expect("history")
                .into_iter()
                .map(|one| one.id)
                .collect();
            assert_eq!(history, vec![canceled.id, failed.id], "newest first");

            let again = acknowledge(tx, failed.id).expect("again");
            assert_eq!(again.acknowledged_at, seen.acknowledged_at, "the first time is kept");
            let refused = acknowledge(tx, canceled.id).expect_err("only a failure");
            assert!(refused.to_string().contains("only a failed run"), "{refused}");
        });
    }

    #[test]
    fn a_run_that_is_over_cannot_be_stopped_again() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            stop(tx, run.id, Ending::Canceled).expect("stop");
            let refused = stop(tx, run.id, Ending::Canceled).expect_err("it is over already");
            assert!(refused.to_string().contains("over already"), "{refused}");
        });
    }

    /// **An action of two steps, placed after a spot that takes a task.** The first step inside goes on
    /// to the second, and the second returns to the action's unnamed way out, which closes the run on
    /// the automation's picture. `back` gives the second step a way round to itself inside the action,
    /// capped at one turn.
    struct Inside {
        automation: Automation,
        project: i64,
        first: crate::model::AutomationPlacement,
        second: crate::model::AutomationPlacement,
        write: crate::model::AutomationStep,
        review: crate::model::AutomationStep,
    }

    fn two_steps_inside(tx: &WriteTx<'_>, back: bool) -> Inside {
        use crate::model::AutomationPictureOwner::{Action, Automation as OnAutomation};
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(
            tx,
            project,
            NewAutomation { name: "書いて見直す".into(), ..Default::default() },
        )
        .expect("add automation");
        let (first_action, first) = mk_placed(tx, &automation, "調べる", "look", "claude");
        let (second_action, second) = mk_placed(tx, &automation, "書く", "write", "claude");
        mk_out(tx, &first_action, None, "タスク", AutomationPortKind::TaskTake, true);
        automation::edge_add(tx, OnAutomation, first.id, None, EdgeTarget::Go(second.id), None)
            .expect("onward");
        automation::edge_add(tx, OnAutomation, second.id, None, EdgeTarget::Done, None)
            .expect("closes");

        let write = crate::ops::test_support::only_step(tx, &second_action);
        let review = automation::step_add(
            tx,
            second_action.id,
            automation::NewStep::new("見直す", "review", "claude"),
        )
        .expect("second step");
        // The first step no longer leaves the action: it goes on to the second inside it.
        let leaves = read::automation_edge_for_exit(tx.conn(), Action, write.id, None)
            .expect("read")
            .expect("the line out of the first step");
        automation::edge_update(tx, leaves.id, Some(EdgeTarget::Go(review.id)), None)
            .expect("on to the second step");
        automation::edge_add(tx, Action, review.id, None, EdgeTarget::Exit(None), None)
            .expect("the second step leaves the action");
        if back {
            automation::exit_add(tx, crate::model::AutomationOwner::Step, review.id, Some("again"))
                .expect("way back");
            automation::edge_add(tx, Action, review.id, Some("again"), EdgeTarget::Go(review.id), Some(1))
                .expect("back");
        }
        let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Inside { automation, project, first, second, write, review }
    }

    /// The copy of one step of one placement.
    fn copy_of(
        tx: &WriteTx<'_>,
        run: &AutomationRun,
        placement: &crate::model::AutomationPlacement,
        step: &crate::model::AutomationStep,
    ) -> AutomationRunDef {
        read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.placement_id == Some(placement.id) && d.step_id == Some(step.id))
            .expect("the step's copy")
    }

    fn opened_step(tx: &WriteTx<'_>, run: &AutomationRun, def: &AutomationRunDef) -> Opening {
        match open(tx, run.id, def.id, None).expect("open") {
            Opened::Ready(opening) => *opening,
            Opened::Stopped { missing, .. } => panic!("stopped for {missing:?}"),
            Opened::NoAgent { agent, .. } => panic!("cannot start {agent}"),
        }
    }

    fn stepped_to(next: Next) -> AutomationRunDef {
        match next {
            Next::Step(def) => *def,
            other => panic!("expected a step, got {other:?}"),
        }
    }

    /// **A launch opens every step inside a placed action into one column** (`AMB-T-5313`), each row
    /// saying which placement it came from — and the run walks the steps inside before it leaves the
    /// action by the way out the last of them returns to.
    #[test]
    fn a_run_walks_the_steps_inside_an_action_and_leaves_it_where_the_last_returns() {
        with_tx(|tx| {
            let p = two_steps_inside(tx, false);
            let run = a_run(tx, &p.automation);
            let defs = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert_eq!(defs.len(), 3, "one row for the first placement, two for the second");
            let second_rows: Vec<_> =
                defs.iter().filter(|d| d.placement_id == Some(p.second.id)).collect();
            assert_eq!(
                second_rows.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
                vec!["書く", "見直す"],
                "a row per step, named after the step",
            );

            let first = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, first.run_step.id);
            let next = stepped_to(done(tx, first.run_step.id, None, "Looked.").expect("done"));
            assert_eq!(next.id, copy_of(tx, &run, &p.second, &p.write).id, "the action's entry first");

            let write = opened_step(tx, &run, &next);
            let next = stepped_to(done(tx, write.run_step.id, None, "Wrote.").expect("done"));
            assert_eq!(next.id, copy_of(tx, &run, &p.second, &p.review).id, "then on inside it");
            match crate::ops::automation_run::next_def(tx.conn(), run.id).expect("next") {
                crate::ops::automation_run::Waiting::Step(def) => assert_eq!(def.id, next.id),
                other => panic!("the watcher reads the same step: {other:?}"),
            }

            let review = opened_step(tx, &run, &next);
            let over = done(tx, review.run_step.id, None, "Reviewed.").expect("done");
            assert!(matches!(over, Next::Closed(_)), "the action's way out closes the run: {over:?}");
        });
    }

    /// **A value crosses the action's edge only along the wires drawn to it.** The step that produced
    /// it wires it into the action's way out; the automation wires that on to the next placement; and
    /// the action there wires its input on to the step inside.
    #[test]
    fn a_value_reaches_a_step_inside_an_action_along_the_wires_across_both_edges() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = automation::add(
                tx,
                project,
                NewAutomation { name: "渡す".into(), ..Default::default() },
            )
            .expect("add automation");
            let (first_action, first) = mk_placed(tx, &automation, "調べる", "look", "claude");
            let (second_action, second) = mk_placed(tx, &automation, "直す", "fix", "claude");
            mk_out(tx, &first_action, None, "タスク", AutomationPortKind::TaskTake, true);
            mk_out(tx, &first_action, None, "note", AutomationPortKind::Value, false);
            crate::ops::test_support::mk_in(tx, &second_action, "note", AutomationPortKind::Value, true);
            let on = crate::model::AutomationPictureOwner::Automation;
            automation::wire_add(tx, on, first.id, None, "note", second.id, "note").expect("wire");
            automation::edge_add(tx, on, first.id, None, EdgeTarget::Go(second.id), None)
                .expect("onward");
            automation::edge_add(tx, on, second.id, None, EdgeTarget::Done, None).expect("closes");
            let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");

            let run = a_run(tx, &automation);
            let looked = opened(tx, &run, &first);
            a_task_in_hand(tx, project, looked.run_step.id);
            crate::ops::automation_report::out(
                tx,
                looked.run_step.id,
                "note",
                crate::ops::automation_report::Produced::Value("the third paragraph"),
            )
            .expect("out");
            let next = stepped_to(done(tx, looked.run_step.id, None, "Looked.").expect("done"));
            let fixing = opened_step(tx, &run, &next);
            assert!(fixing.text.contains("the third paragraph"), "{}", fixing.text);
        });
    }

    /// **A way back drawn inside an action is held to its limit there**, counted on the step it leaves
    /// within the placement the run is walking.
    #[test]
    fn a_way_back_inside_an_action_taken_too_often_stops_the_run() {
        with_tx(|tx| {
            let p = two_steps_inside(tx, true);
            let run = a_run(tx, &p.automation);
            let first = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, first.run_step.id);
            let next = stepped_to(done(tx, first.run_step.id, None, "Looked.").expect("done"));
            let write = opened_step(tx, &run, &next);
            let next = stepped_to(done(tx, write.run_step.id, None, "Wrote.").expect("done"));

            let review = opened_step(tx, &run, &next);
            let next = stepped_to(done(tx, review.run_step.id, Some("again"), "Not yet.").expect("done"));
            assert_eq!(next.id, copy_of(tx, &run, &p.second, &p.review).id, "one turn is allowed");
            let twice = opened_step(tx, &run, &next);
            let stopped = done(tx, twice.run_step.id, Some("again"), "Still not.").expect("done");
            assert!(matches!(stopped, Next::Halted(_)));
            assert_eq!(
                read::automation_run(tx.conn(), run.id).expect("read").expect("run").stopped_reason,
                Some(AutomationStoppedReason::MaxTimes),
            );
        });
    }
}
