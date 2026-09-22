//! **Stopping a run** — pausing it, picking it up again, ending it, and the one cleanup every one of
//! those goes through.
//!
//! **A run leaves a lane by exactly one road** ([`ended`]). Four things end a run — the picture running
//! out, a person pressing stop, an input nothing filled, this machine having been restarted under it —
//! and each of them owes the same three acts: hand the lane back so whatever is queued can start,
//! release the task the run was holding, and leave a line on that task saying what became of it. Four
//! roads would have been four places for one of the three to be forgotten, and the one most easily
//! forgotten is the lane: a run that ended without handing it back takes a lane with it for as long as
//! the store lives.
//!
//! **Pausing is a request, not a stop.** A step under way cannot be cut in half — it is an agent in a
//! terminal, mid-sentence — so pressing pause writes `pause_requested` and the run goes on until that
//! step reports. What reads the flag is [`crate::ops::automation_report::done`], which is the only
//! place that knows a step has finished. A queued run has no step under way and pauses on the spot.
//!
//! **Paused holds its task and gives up its lane.** The two go together: the work is half done and
//! nobody else should take it, while the lane is a terminal on this machine and there is nothing in it.
//! Stopping does the opposite with the task — hands it back to `todo` — because a run that was cut off
//! left no one carrying it.
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
use crate::ops::automation_run::promote_next;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// **A run that has stopped, and the one a lane coming free woke up.**
///
/// The two travel together because the second is a consequence of the first that only this side knows:
/// a lane handed back promotes whatever has waited longest, and the driver that stopped one run is the
/// one that now has to open the next step of another. Dropped here, a promoted run would sit `running`
/// with no terminal under it.
#[derive(Clone, Debug)]
pub struct Ended {
    pub run: AutomationRun,
    /// The run that took the lane this one gave up, where one was waiting.
    pub woke: Option<AutomationRun>,
}

/// What pressing pause did.
#[derive(Clone, Debug)]
pub enum Paused {
    /// A step is under way. The run is still `running` and stops at the end of it, which is where its
    /// lane is handed back too.
    Asked(AutomationRun),
    /// Nothing was under way, so it is `paused` already.
    Now(Ended),
}

/// What picking a paused run up again did.
#[derive(Clone, Debug)]
pub enum Resumed {
    /// A lane was free: open a terminal on this step ([`crate::ops::automation_step::open`] takes it
    /// from here).
    Step { run: AutomationRun, next: Box<AutomationRunDef> },
    /// Every lane is held, so it waits its turn — the same queue a launch joins.
    Queued(AutomationRun),
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
    matches!(
        status,
        AutomationRunStatus::Running | AutomationRunStatus::Queued | AutomationRunStatus::Paused
    )
}

// ───────────────────────────── the one cleanup ─────────────────────────────

/// **End a run and hand back everything it was holding.** Every way a run can end comes through here.
///
/// `status` is [`AutomationRunStatus::Done`] where the picture ran out and
/// [`AutomationRunStatus::Stopped`] everywhere else, and `reason` says which of the four stops it was —
/// left `None` for a run that simply reached the end, and for one stopped by something the four do not
/// name.
///
/// **The task goes back to `todo` only on a stop.** A run that finished left its task wherever its steps
/// put it, which is the outcome somebody asked for; a run that was cut off left it reserved by nobody,
/// and a task held by a run that is gone is one no session will ever pick up.
///
/// `lanes` is [`crate::config::Config::automation_lanes`], carried in because the store does not hold
/// the reader's settings.
pub fn ended(
    tx: &WriteTx<'_>,
    before: AutomationRun,
    status: AutomationRunStatus,
    reason: Option<AutomationStoppedReason>,
    lanes: i64,
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
    let woke = release(tx, &before, lanes)?;
    Ok(Ended { run: after, woke })
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

/// The first sentence of that line: what stopped the run, in the words each of the four is worth.
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
        None => "An automation run stopped.",
    }
}

/// Hand the lane back, where the run was holding one, and wake whatever has waited longest. A run that
/// was queued or paused held none, and then there is nothing to hand back — asking anyway would promote
/// a second run on the strength of a lane nobody released.
fn release(
    tx: &WriteTx<'_>,
    before: &AutomationRun,
    lanes: i64,
) -> Result<Option<AutomationRun>> {
    if !before.status.holds_a_lane() {
        return Ok(None);
    }
    promote_next(tx, lanes)
}

// ───────────────────────────── pause, resume, stop ─────────────────────────────

/// **Ask a run to pause.** It stops at the end of the step under way, not in the middle of one.
///
/// A queued run is paused on the spot: there is no step to wait for, and leaving it queued would have it
/// start the moment a lane came free, which is the opposite of what was pressed.
pub fn pause(tx: &WriteTx<'_>, run_id: i64, lanes: i64) -> Result<Paused> {
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
        AutomationRunStatus::Queued => Ok(Paused::Now(settle(tx, before, lanes)?)),
        AutomationRunStatus::Paused => Ok(Paused::Now(Ended { run: before, woke: None })),
        other => Err(Error::invalid(format!(
            "run '{run_id}' is {}, and only a run still going can be paused",
            other.as_str()
        ))),
    }
}

/// Put a run into `paused`: it keeps its task and gives up its lane.
///
/// It does not go through [`ended`], and the difference is the whole point — a paused run has not
/// ended. `ended_at` stays empty, the task stays reserved, and nothing is written on it, because there
/// is nothing to tell somebody yet.
pub fn settle(tx: &WriteTx<'_>, before: AutomationRun, lanes: i64) -> Result<Ended> {
    let now = Timestamp::now();
    let mut after = before.clone();
    after.status = AutomationRunStatus::Paused;
    after.pause_requested = false;
    after.updated_at = now;
    crate::ops::emit_update(tx, record::automation_run(&before), record::automation_run(&after))?;
    let woke = release(tx, &before, lanes)?;
    Ok(Ended { run: after, woke })
}

/// **Pick a paused run up again**, from the way out the last step left through.
///
/// The picture is walked live, the way every other walk of it is: which step comes next is the shape of
/// the automation rather than anything the pause wrote down. A run whose last step left through a way
/// out that now decides nothing is stopped rather than resumed — the same answer the report door gives,
/// since the picture was edited underneath it either way.
pub fn resume(tx: &WriteTx<'_>, run_id: i64, lanes: i64) -> Result<Resumed> {
    let before = live_run(tx, run_id)?;
    if before.status != AutomationRunStatus::Paused {
        return Err(Error::invalid(format!(
            "run '{run_id}' is {}, and only a paused one is picked up again",
            before.status.as_str()
        )));
    }
    let Some(next) = next_after_the_pause(tx.conn(), &before)? else {
        ended(tx, before, AutomationRunStatus::Stopped, None, lanes)?;
        return Err(Error::invalid(format!(
            "run '{run_id}' cannot go on: what followed the step it paused after is no longer in the \
             picture, so it has been stopped"
        )));
    };
    let now = Timestamp::now();
    let held = read::automation_run_ids_running(tx.conn())?.len() as i64;
    let mut after = before.clone();
    after.status =
        if held < lanes { AutomationRunStatus::Running } else { AutomationRunStatus::Queued };
    after.updated_at = now;
    if after.status.holds_a_lane() && after.started_at.is_none() {
        after.started_at = Some(now);
    }
    crate::ops::emit_update(tx, record::automation_run(&before), record::automation_run(&after))?;
    match after.status {
        AutomationRunStatus::Running => Ok(Resumed::Step { run: after, next: Box::new(next) }),
        _ => Ok(Resumed::Queued(after)),
    }
}

/// **What a run that has just taken a lane does next.**
#[derive(Clone, Debug)]
pub enum TookALane {
    /// Open a terminal on this step.
    Step(Box<AutomationRunDef>),
    /// It has nothing left to open — the picture lost the step its last one led to — so it was
    /// stopped. `Ended::woke` is whatever took the lane it had just been given.
    Lost(Ended),
}

/// **Take a promoted run from `running` to a terminal, or end it.**
///
/// A lane handed back promotes whatever has waited longest ([`crate::ops::automation_run::promote_next`]),
/// which writes `running` and nothing else; the driver that freed the lane then owes that run a step.
/// Where the picture no longer says which step that is, the run is ended here rather than left
/// `running` with no terminal under it — the same answer [`resume`] gives for the same reason.
pub fn took_a_lane(tx: &WriteTx<'_>, run: &AutomationRun, lanes: i64) -> Result<TookALane> {
    match next_for(tx.conn(), run)? {
        Some(def) => Ok(TookALane::Step(Box::new(def))),
        None => Ok(TookALane::Lost(ended(
            tx,
            run.clone(),
            AutomationRunStatus::Stopped,
            None,
            lanes,
        )?)),
    }
}

/// **The step a run coming off the queue opens next.**
///
/// A lane handed back promotes whatever has waited longest ([`crate::ops::automation_run::promote_next`]),
/// and whoever did the promoting then has to open a step of it — a promoted run left alone sits `running`
/// with no terminal under it. Which step that is depends on how far it had got: a run that has never run
/// one starts at the entry, and one that was paused part-way carries on from the way out its last step
/// left through.
pub fn next_for(conn: &Connection, run: &AutomationRun) -> Result<Option<AutomationRunDef>> {
    if read::automation_run_steps_of(conn, run.id)?.is_empty() {
        return crate::ops::automation_run::entry_def(conn, run.id);
    }
    next_after_the_pause(conn, run)
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
    lanes: i64,
) -> Result<Ended> {
    let before = live_run(tx, run_id)?;
    if !under_way(before.status) {
        return Err(Error::invalid(format!(
            "run '{run_id}' is {} — it is over already",
            before.status.as_str()
        )));
    }
    ended(tx, before, AutomationRunStatus::Stopped, Some(reason), lanes)
}

/// **The runs this machine was in the middle of when it last shut down.**
///
/// Run at startup, before anything else touches a run. Every `running` and `queued` row is one from a
/// process that is gone: a terminal does not outlive the app that drew it, and a queue nobody is
/// holding will never be promoted. Both are stopped as crashes, which hands back their tasks and says
/// so on them.
///
/// It answers what it stopped, in id order, so a face can say how many runs did not survive the
/// restart rather than leaving somebody to notice on their own.
pub fn sweep(tx: &WriteTx<'_>, lanes: i64) -> Result<Vec<AutomationRun>> {
    // The queue goes first. A lane handed back promotes whatever has waited longest, and promoting a
    // run this sweep is about to stop would raise it into `running` only to put it down again.
    let mut caught = read::automation_run_ids_queued(tx.conn())?;
    caught.extend(read::automation_run_ids_running(tx.conn())?);
    let mut stopped = Vec::with_capacity(caught.len());
    for id in caught {
        let Some(run) = read::automation_run(tx.conn(), id)? else { continue };
        stopped.push(
            ended(
                tx,
                run,
                AutomationRunStatus::Stopped,
                Some(AutomationStoppedReason::Crashed),
                lanes,
            )?
            .run,
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

    /// How many lanes the machine these tests run on has, except where one of them says otherwise.
    const LANES: i64 = 3;

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

    fn a_run(tx: &WriteTx<'_>, automation: &Automation, lanes: i64) -> AutomationRun {
        let startable = vec!["claude".to_string()];
        let by = Launcher {
            startable: Some(&startable),
            models: crate::ops::automation_run::nothing_asked(),
            lanes,
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
        match open(tx, run.id, def_of(tx, run, step).id, LANES).expect("open") {
            Opened::Ready(opening) => *opening,
            Opened::Stopped { missing, .. } => panic!("stopped for {missing:?}"),
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
    fn a_run_that_took_a_lane_is_given_the_step_it_starts_at() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let first = a_run(tx, &p.automation, 1);
            let waiting = a_run(tx, &p.automation, 1);
            assert_eq!(status_of(tx, waiting.id), AutomationRunStatus::Queued, "one lane, two runs");

            let ended = stop(tx, first.id, AutomationStoppedReason::ByHuman, 1).expect("stop");
            let woke = ended.woke.expect("the lane went to the one that was waiting");
            assert_eq!(woke.id, waiting.id);
            // Promotion writes `running` and nothing else, so what the driver is owed is the step —
            // without it the run sits `running` with no terminal under it.
            match took_a_lane(tx, &woke, 1).expect("took a lane") {
                TookALane::Step(def) => assert_eq!(def.step_id, Some(p.first.id), "the entry"),
                TookALane::Lost(_) => panic!("it has a step to start at"),
            }
        });
    }

    #[test]
    fn a_run_whose_picture_lost_its_entry_is_stopped_rather_than_left_running() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let first = a_run(tx, &p.automation, 1);
            let waiting = a_run(tx, &p.automation, 1);
            automation::set_entry(tx, p.automation.id, None).expect("entry taken off");

            let ended = stop(tx, first.id, AutomationStoppedReason::ByHuman, 1).expect("stop");
            let woke = ended.woke.expect("promoted");
            assert!(
                matches!(took_a_lane(tx, &woke, 1).expect("took a lane"), TookALane::Lost(_)),
                "nothing to open",
            );
            assert_eq!(status_of(tx, waiting.id), AutomationRunStatus::Stopped);
        });
    }

    #[test]
    fn the_running_index_holds_what_is_not_over_and_the_stops_behind_it() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let running = a_run(tx, &p.automation, 1);
            let queued = a_run(tx, &p.automation, 1);
            let stopped = a_run(tx, &p.automation, 1);
            stop(tx, stopped.id, AutomationStoppedReason::ByHuman, 1).expect("stop");
            // Ending one hands its lane on, so the third run is the one left queued.
            let live: Vec<i64> = read::automation_runs_live(tx.conn(), 20)
                .expect("live")
                .into_iter()
                .map(|one| one.id)
                .collect();
            assert!(live.contains(&running.id) && live.contains(&queued.id) && live.contains(&stopped.id));
            // Under way first and newest first within that, then the stops — which is the order the
            // tab draws, without a screen sorting what it was handed.
            let over = live.iter().position(|id| *id == stopped.id).expect("the stop is on it");
            assert_eq!(over, live.len() - 1, "a stop sits behind everything still going");

            // What is over is not on it. A finished run is read from the task it worked.
            ended(tx, running, AutomationRunStatus::Done, None, 1).expect("done");
            let after = read::automation_runs_live(tx.conn(), 20).expect("live");
            assert!(after.iter().all(|one| one.status != AutomationRunStatus::Done));
        });
    }

    #[test]
    fn pausing_a_running_run_waits_for_the_step_under_way() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation, LANES);
            let step = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, step.run_step.id);

            let asked = pause(tx, run.id, LANES).expect("pause");
            assert!(matches!(asked, Paused::Asked(_)), "a step is under way");
            assert_eq!(status_of(tx, run.id), AutomationRunStatus::Running, "not cut in half");

            let next = done(tx, step.run_step.id, None, "Looked at it.", LANES).expect("done");
            assert!(matches!(next, Next::Paused(_)), "the pause is answered at the end of the step");
            assert_eq!(status_of(tx, run.id), AutomationRunStatus::Paused);
            assert!(
                read::automation_run_ids_running(tx.conn()).expect("running").is_empty(),
                "a paused run gives its lane back",
            );
        });
    }

    #[test]
    fn a_paused_run_keeps_the_task_it_was_working() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation, LANES);
            let step = opened(tx, &run, &p.first);
            let task = a_task_in_hand(tx, p.project, step.run_step.id);
            pause(tx, run.id, LANES).expect("pause");
            done(tx, step.run_step.id, None, "Looked at it.", LANES).expect("done");
            assert_eq!(
                read::task_status(tx.conn(), task).expect("read"),
                Some(TaskStatus::InProgress),
                "the work is half done and nobody else should take it",
            );
        });
    }

    #[test]
    fn pausing_a_queued_run_pauses_it_on_the_spot() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let holding = a_run(tx, &p.automation, 1);
            assert_eq!(holding.status, AutomationRunStatus::Running);
            let waiting = a_run(tx, &p.automation, 1);
            assert_eq!(waiting.status, AutomationRunStatus::Queued);

            let paused = pause(tx, waiting.id, 1).expect("pause");
            assert!(matches!(paused, Paused::Now(_)), "nothing was under way");
            assert_eq!(status_of(tx, waiting.id), AutomationRunStatus::Paused);
            assert_eq!(status_of(tx, holding.id), AutomationRunStatus::Running, "untouched");
        });
    }

    #[test]
    fn resuming_opens_the_step_after_the_one_it_paused_on() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation, LANES);
            let step = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, step.run_step.id);
            pause(tx, run.id, LANES).expect("pause");
            done(tx, step.run_step.id, None, "Looked at it.", LANES).expect("done");

            match resume(tx, run.id, LANES).expect("resume") {
                Resumed::Step { run: after, next } => {
                    assert_eq!(after.status, AutomationRunStatus::Running);
                    assert_eq!(next.step_id, Some(p.second.id), "it goes on where it left off");
                }
                Resumed::Queued(_) => panic!("a lane was free"),
            }
        });
    }

    #[test]
    fn resuming_with_every_lane_held_joins_the_queue() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation, 1);
            let step = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, step.run_step.id);
            pause(tx, run.id, 1).expect("pause");
            done(tx, step.run_step.id, None, "Looked at it.", 1).expect("done");
            // Somebody else took the lane while this one was paused.
            let other = a_run(tx, &p.automation, 1);
            assert_eq!(other.status, AutomationRunStatus::Running);

            let resumed = resume(tx, run.id, 1).expect("resume");
            assert!(matches!(resumed, Resumed::Queued(_)), "it waits its turn like any launch");
            assert_eq!(status_of(tx, run.id), AutomationRunStatus::Queued);
        });
    }

    #[test]
    fn stopping_hands_the_task_back_and_says_so_on_it() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation, LANES);
            let step = opened(tx, &run, &p.first);
            let task = a_task_in_hand(tx, p.project, step.run_step.id);

            let after = stop(tx, run.id, AutomationStoppedReason::ByHuman, LANES).expect("stop");
            assert_eq!(after.run.status, AutomationRunStatus::Stopped);
            assert_eq!(after.run.stopped_reason, Some(AutomationStoppedReason::ByHuman));
            assert!(after.woke.is_none(), "nothing was waiting for the lane");
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
            let run = a_run(tx, &p.automation, LANES);
            let first = opened(tx, &run, &p.first);
            let task = a_task_in_hand(tx, p.project, first.run_step.id);
            done(tx, first.run_step.id, None, "Looked at it.", LANES).expect("done");
            let second = opened(tx, &run, &p.second);
            done(tx, second.run_step.id, None, "Fixed it.", LANES).expect("done");

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
    fn the_lane_a_run_gives_up_wakes_whatever_waited_longest() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let holding = a_run(tx, &p.automation, 1);
            let waiting = a_run(tx, &p.automation, 1);
            assert_eq!(waiting.status, AutomationRunStatus::Queued);

            let ended = stop(tx, holding.id, AutomationStoppedReason::ByHuman, 1).expect("stop");
            assert_eq!(
                status_of(tx, waiting.id),
                AutomationRunStatus::Running,
                "the lane went straight to the next in line",
            );
            assert_eq!(
                ended.woke.map(|r| r.id),
                Some(waiting.id),
                "and whoever stopped the first run is told which one to open a step of",
            );
        });
    }

    #[test]
    fn a_restart_stops_what_was_running_and_what_was_queued() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let holding = a_run(tx, &p.automation, 1);
            let waiting = a_run(tx, &p.automation, 1);

            let caught = sweep(tx, 1).expect("sweep");
            assert_eq!(caught.len(), 2, "a terminal does not outlive the app that drew it");
            assert_eq!(status_of(tx, holding.id), AutomationRunStatus::Stopped);
            assert_eq!(status_of(tx, waiting.id), AutomationRunStatus::Stopped);
            assert!(caught.iter().all(|r| r.stopped_reason
                == Some(AutomationStoppedReason::Crashed)));
        });
    }

    #[test]
    fn a_way_back_taken_more_often_than_it_may_stops_the_run() {
        with_tx(|tx| {
            let p = picture(tx, true);
            let run = a_run(tx, &p.automation, LANES);
            let first = opened(tx, &run, &p.first);
            a_task_in_hand(tx, p.project, first.run_step.id);
            done(tx, first.run_step.id, None, "Looked at it.", LANES).expect("done");

            // Round once: the way back is within its one turn.
            let second = opened(tx, &run, &p.second);
            let next = done(tx, second.run_step.id, Some("again"), "Not yet.", LANES).expect("done");
            assert!(matches!(next, Next::Step(_)), "one turn is what it is allowed");

            // Round twice: the same way out, the same task — one turn too many.
            let twice = opened(tx, &run, &p.second);
            let stopped = done(tx, twice.run_step.id, Some("again"), "Still not.", LANES)
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
            let run = a_run(tx, &p.automation, LANES);
            stop(tx, run.id, AutomationStoppedReason::ByHuman, LANES).expect("stop");
            let refused = stop(tx, run.id, AutomationStoppedReason::ByHuman, LANES)
                .expect_err("it is over already");
            assert!(refused.to_string().contains("over already"), "{refused}");
        });
    }
}
