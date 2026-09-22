//! **Stopping a run** — pausing it, picking it up again, ending it, and the one cleanup every one of
//! those goes through.
//!
//! **A run ends by exactly one road** ([`ended`]). Four things end a run — the picture running out, a
//! person pressing stop, an input nothing filled, this machine having been restarted under it — and
//! each of them owes the same two acts: release the task the run was holding, and leave a line on that
//! task saying what became of it. Four roads would have been four places for one of the two to be
//! forgotten.
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
/// `status` is [`AutomationRunStatus::Done`] where the picture ran out and
/// [`AutomationRunStatus::Stopped`] everywhere else, and `reason` says which kind of stop it was —
/// left `None` for a run that simply reached the end, and for one stopped by something
/// [`AutomationStoppedReason`] does not name.
///
/// **The task goes back to `todo` only on a stop.** A run that finished left its task wherever its steps
/// put it, which is the outcome somebody asked for; a run that was cut off left it reserved by nobody,
/// and a task held by a run that is gone is one no session will ever pick up.
pub fn ended(
    tx: &WriteTx<'_>,
    before: AutomationRun,
    status: AutomationRunStatus,
    reason: Option<AutomationStoppedReason>,
) -> Result<Ended> {
    let now = Timestamp::now();
    let mut after = before.clone();
    after.status = status;
    after.stopped_reason = (status == AutomationRunStatus::Stopped).then_some(reason).flatten();
    after.pause_requested = false;
    after.ended_at = Some(now);
    after.updated_at = now;
    crate::ops::emit_update(tx, record::automation_run(&before), record::automation_run(&after))?;

    let stretch = close_stretch(tx, before.id, now)?;
    if status == AutomationRunStatus::Stopped {
        hand_the_task_back(tx, &after, stretch.as_ref(), reason)?;
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
    reason: Option<AutomationStoppedReason>,
) -> Result<()> {
    let Some(task_id) = stretch.and_then(|s| s.task_id) else { return Ok(()) };
    if read::task_status(tx.conn(), task_id)? == Some(TaskStatus::InProgress) {
        crate::ops::task::set_status(tx, task_id, TaskStatus::Todo)?;
    }
    crate::ops::comment::add_comment(tx, task_id, ActorKind::Ai, &said(tx, run, reason)?)?;
    Ok(())
}

/// What the line on the task says: that the run stopped, why, and how far it got.
///
/// The run's id is in it because that is the only handle a person has on a run from a task — the two
/// are not linked by a column, and a task that has been round three runs would otherwise say only that
/// one of them stopped.
fn said(
    tx: &WriteTx<'_>,
    run: &AutomationRun,
    reason: Option<AutomationStoppedReason>,
) -> Result<String> {
    let walked = read::automation_run_steps_of(tx.conn(), run.id)?;
    let reached = walked
        .last()
        .map(|step| match read::automation_run_def(tx.conn(), step.run_def_id) {
            Ok(Some(def)) => format!("It got as far as \"{}\".", def.name),
            _ => "It got as far as a step that is no longer there.".to_string(),
        })
        .unwrap_or_else(|| "It stopped before opening a step.".to_string());
    Ok(format!("{} {reached} (run {})", why(reason), run.id))
}

/// The first sentence of that line: what stopped the run, in the words each reason is worth.
fn why(reason: Option<AutomationStoppedReason>) -> &'static str {
    match reason {
        Some(AutomationStoppedReason::Crashed) => {
            "An automation run stopped: Amenbo was restarted while it was under way."
        }
        Some(AutomationStoppedReason::MaxTimes) => {
            "An automation run stopped: a way back was taken as often as it is allowed to be."
        }
        Some(AutomationStoppedReason::NoAgent) => {
            "An automation run stopped: the agent a step asked for could not be started."
        }
        Some(AutomationStoppedReason::ByHuman) => "An automation run was stopped.",
        Some(AutomationStoppedReason::NoWayOn) => {
            "An automation run stopped: nothing was left for it to open."
        }
        None => "An automation run stopped.",
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
        ended(tx, before, AutomationRunStatus::Stopped, None)?;
        return Err(Error::invalid(format!(
            "run '{run_id}' cannot go on: what followed the step it paused after is no longer in the \
             picture, so it has been stopped"
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
    let Some(step_id) = def.step_id else { return Ok(None) };
    let Some(edge) = read::automation_edge_for_exit(conn, step_id, last.exit_name.as_deref())?
    else {
        return Ok(None);
    };
    let Some(to) = edge.to_step_id else { return Ok(None) };
    Ok(read::automation_run_defs_of(conn, run.id)?.into_iter().find(|d| d.step_id == Some(to)))
}

/// **Stop a run now** — the terminal is closed wherever it is, and the cleanup runs.
///
/// This is what a person presses when pausing will not do, and what closing a run's pane means. It is
/// also the door a crash sweep and a loop out of turns come through, each naming its own reason.
pub fn stop(
    tx: &WriteTx<'_>,
    run_id: i64,
    reason: AutomationStoppedReason,
) -> Result<Ended> {
    let before = live_run(tx, run_id)?;
    if !under_way(before.status) {
        return Err(Error::invalid(format!(
            "run '{run_id}' is {} — it is over already",
            before.status.as_str()
        )));
    }
    ended(tx, before, AutomationRunStatus::Stopped, Some(reason))
}

/// **The runs this machine was in the middle of when it last shut down.**
///
/// Run at startup, before anything else touches a run. Every `running` row is one from a process that
/// is gone: a terminal does not outlive the app that drew it. Each is stopped as a crash, which hands
/// back its task and says so on it.
///
/// It answers what it stopped, in id order, so a face can say how many runs did not survive the
/// restart rather than leaving somebody to notice on their own.
pub fn sweep(tx: &WriteTx<'_>) -> Result<Vec<AutomationRun>> {
    let caught = read::automation_run_ids_running(tx.conn())?;
    let mut stopped = Vec::with_capacity(caught.len());
    for id in caught {
        let Some(run) = read::automation_run(tx.conn(), id)? else { continue };
        stopped.push(
            ended(tx, run, AutomationRunStatus::Stopped, Some(AutomationStoppedReason::Crashed))?.run,
        );
    }
    Ok(stopped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Automation, AutomationOwner, AutomationPortDirection, AutomationPortKind,
        AutomationPortOwner, AutomationStep,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewStep};
    use crate::ops::automation_report::{done, Next};
    use crate::ops::automation_run::{launch, Launcher};
    use crate::ops::automation_step::{open, Opened, Opening};
    use crate::ops::test_support::{mk_project, mk_task_in, with_tx};


    /// The picture these tests walk: a step that takes a task and goes on to a second, and the second
    /// closing the run. `back` gives the second a way round to itself, capped at one turn — a way back
    /// into the step that takes a task would begin a fresh stretch, which is the one case the limit
    /// deliberately does not count.
    struct Picture {
        automation: Automation,
        project: i64,
        first: AutomationStep,
        second: AutomationStep,
    }

    fn picture(tx: &WriteTx<'_>, back: bool) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(
            tx,
            project,
            NewAutomation { name: "1件やりきる".into(), ..Default::default() },
        )
        .expect("add automation");
        let first =
            automation::step_add(tx, automation.id, NewStep::with_prompt("調べる", "look", "claude"))
                .expect("first step");
        let second =
            automation::step_add(tx, automation.id, NewStep::with_prompt("直す", "fix", "claude"))
                .expect("second step");
        takes_a_task(tx, &first);
        automation::edge_add(tx, first.id, None, EdgeTarget::Go(second.id), None).expect("onward");
        if back {
            automation::exit_add(tx, AutomationOwner::Step, second.id, Some("again"))
                .expect("way back");
            automation::edge_add(tx, second.id, Some("again"), EdgeTarget::Go(second.id), Some(1))
                .expect("back");
        }
        automation::edge_add(tx, second.id, None, EdgeTarget::Done, None).expect("closes");
        let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, project, first, second }
    }

    /// Declare the `task_take` output that makes a step usable as the one a run starts on.
    fn takes_a_task(tx: &WriteTx<'_>, step: &AutomationStep) {
        let exit = read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, step.id, None)
            .expect("read")
            .expect("the unnamed way out");
        automation::port_add(
            tx,
            AutomationPortOwner::Exit,
            exit.id,
            AutomationPortDirection::Out,
            "タスク",
            AutomationPortKind::TaskTake,
            true,
        )
        .expect("output");
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

    fn def_of(tx: &WriteTx<'_>, run: &AutomationRun, step: &AutomationStep) -> AutomationRunDef {
        read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.step_id == Some(step.id))
            .expect("the step's snapshot")
    }

    fn opened(tx: &WriteTx<'_>, run: &AutomationRun, step: &AutomationStep) -> Opening {
        match open(tx, run.id, def_of(tx, run, step).id, None).expect("open") {
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
            stop(tx, stopped.id, AutomationStoppedReason::ByHuman).expect("stop");
            let live: Vec<i64> = read::automation_runs_live(tx.conn(), 20)
                .expect("live")
                .into_iter()
                .map(|one| one.id)
                .collect();
            assert!(live.contains(&running.id) && live.contains(&paused.id) && live.contains(&stopped.id));
            // Under way first and newest first within that, then the stops — which is the order the
            // tab draws, without a screen sorting what it was handed.
            let over = live.iter().position(|id| *id == stopped.id).expect("the stop is on it");
            assert_eq!(over, live.len() - 1, "a stop sits behind everything still going");

            // What is over is not on it. A finished run is read from the task it worked.
            ended(tx, running, AutomationRunStatus::Done, None).expect("done");
            let after = read::automation_runs_live(tx.conn(), 20).expect("live");
            assert!(after.iter().all(|one| one.status != AutomationRunStatus::Done));
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
                assert_eq!(status_of(tx, run.id), AutomationRunStatus::Stopped);
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

            let after = stop(tx, run.id, AutomationStoppedReason::ByHuman).expect("stop");
            assert_eq!(after.run.status, AutomationRunStatus::Stopped);
            assert_eq!(after.run.stopped_reason, Some(AutomationStoppedReason::ByHuman));
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

            assert_eq!(status_of(tx, run.id), AutomationRunStatus::Done);
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
            assert_eq!(status_of(tx, one.id), AutomationRunStatus::Stopped);
            assert_eq!(status_of(tx, another.id), AutomationRunStatus::Stopped);
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

    #[test]
    fn a_run_that_is_over_cannot_be_stopped_again() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            stop(tx, run.id, AutomationStoppedReason::ByHuman).expect("stop");
            let refused = stop(tx, run.id, AutomationStoppedReason::ByHuman)
                .expect_err("it is over already");
            assert!(refused.to_string().contains("over already"), "{refused}");
        });
    }
}
