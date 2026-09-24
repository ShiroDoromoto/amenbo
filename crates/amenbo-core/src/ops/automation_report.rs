//! What a step says back, and what the run does with it: the task it took, the things it produced, and
//! the way out it left through.
//!
//! **The end of a step is something it says, never something watched for.** A process that exits has
//! told us only that it is gone — not whether the work was done, which way out was taken, or what came
//! of it. So a step ends by reporting ([`done`]), and a process that dies without one is a crash, which
//! is a different fact and handled where a run is stopped ([`super::automation_run`]).
//!
//! **Three doors, in the order a step walks them.** [`take`] reserves the task the run is about and
//! declares it in one act, since a task reserved but not declared is one nobody can trace back to the
//! run. [`out`] puts down each thing the step produced, on the port it was declared as. [`done`] names
//! the way out, and that way out is the whole condition the next step is chosen by.
//!
//! **An output is one way out's, and a value is put down on it.** Two ways out of one step may each
//! declare an output called `report`, and they are two port rows with two ids (`AMB-D-961`). [`out`]
//! names the row, so the way out a value belongs to is known the moment it is put down. A value put down
//! on another way out than the one the step leaves by is kept as the record of what the step did, and
//! handed on by nothing: a wire finds a value by the way out and the port it left through.

use crate::error::{Error, Result};
use crate::model::{
    ActorKind, AttachmentTarget, AutomationPictureOwner, AutomationPortDirection,
    AutomationPortKind, AutomationRun, AutomationRunDef, AutomationRunStep,
    AutomationRunStepStatus, AutomationRunTask, AutomationRunValue, AutomationStoppedReason,
    RunDefExit, RunDefLine, RunDefPort, Task, TaskStatus, ERROR_EXIT,
};
use crate::ops::automation_run::{self, Onward};
use crate::ops::automation_stop::{self, Ended, Ending};
use crate::ops::emit_create;
use rusqlite::Connection;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// What one thing a step produced actually is. Which of the three a port takes is its declared kind's
/// to say, and a payload that does not match it is refused rather than written into a column nobody
/// will read.
#[derive(Clone, Copy, Debug)]
pub enum Produced<'a> {
    /// Text, for a `value` port.
    Value(&'a str),
    /// A file, already attached to this step execution, for a `file` port.
    File(i64),
    /// A task this step created, for a `task_make` port.
    Task(i64),
}

/// What the run does next, once a step has reported.
#[derive(Clone, Debug)]
pub enum Next {
    /// Open a terminal on this step ([`super::automation_step::open`] takes it from here).
    Step(Box<AutomationRunDef>),
    /// The run is over ([`super::automation_stop::Ended`]).
    Closed(Ended),
    /// The run is stopped and a person is owed a look — either because the way out says so, or because
    /// nothing says what happens after it.
    Halted(Ended),
    /// Somebody pressed pause while this step was under way, and this is the end of it. The next step
    /// is not opened; the run keeps its task ([`super::automation_stop::resume`] picks it up from the
    /// same way out).
    Paused(Ended),
}

/// `<what> '<id>' not found`, the uncoded refusal the automation entities take
/// ([`crate::ops::automation`] says why).
fn not_found(what: &str, id: i64) -> Error {
    Error::not_found(format!("{what} '{id}' not found"))
}

/// The execution a step is speaking for, refused unless it is still running: a step that has reported
/// has had its way out read and its values stamped, and a second report would be rewriting history
/// somebody has already acted on.
fn live_execution(tx: &WriteTx<'_>, run_step_id: i64) -> Result<AutomationRunStep> {
    let run_step = read::automation_run_step(tx.conn(), run_step_id)?
        .ok_or_else(|| not_found("step execution", run_step_id))?;
    if run_step.status != AutomationRunStepStatus::Running {
        return Err(Error::invalid(format!(
            "step execution '{run_step_id}' is {}, and only a running one still has anything to say",
            run_step.status.as_str()
        )));
    }
    Ok(run_step)
}

/// The step as it stood at launch — what this execution was asked to do, and the only thing its
/// declarations are read from.
fn def_of(tx: &WriteTx<'_>, run_step: &AutomationRunStep) -> Result<AutomationRunDef> {
    read::automation_run_def(tx.conn(), run_step.run_def_id)?
        .ok_or_else(|| not_found("step of a run", run_step.run_def_id))
}

/// The ways out this step was launched carrying, and what each of them hands on.
fn exits_of(def: &AutomationRunDef) -> Result<Vec<RunDefExit>> {
    serde_json::from_str(&def.exits).map_err(Error::from)
}

/// Find one declared output by its id, and the way out it hangs on.
fn declared_out(exits: &[RunDefExit], port_id: i64) -> Option<(&RunDefExit, &RunDefPort)> {
    exits.iter().find_map(|e| e.outs.iter().find(|p| p.id == port_id).map(|p| (e, p)))
}

/// **What one of this step's declared outputs carries**, by its id — `None` where the step declares no
/// output of that id.
///
/// It is read **before** a value is put down, because what a port takes decides which command says it:
/// a value and a file go down with `automation step-out`, the task the run is about is reserved and handed
/// on in one act by [`take`], and a task the step raised along the way goes down with `out` too, as an
/// id. A caller that guessed would be refused by [`put`] with a sentence about kinds, which is not the
/// sentence somebody typing needs.
pub fn out_kind(
    conn: &Connection,
    run_step_id: i64,
    port_id: i64,
) -> Result<Option<AutomationPortKind>> {
    let run_step = read::automation_run_step(conn, run_step_id)?
        .ok_or_else(|| not_found("step execution", run_step_id))?;
    let def = read::automation_run_def(conn, run_step.run_def_id)?
        .ok_or_else(|| not_found("step of a run", run_step.run_def_id))?;
    Ok(declared_out(&exits_of(&def)?, port_id).map(|(_, port)| port.kind))
}

// ───────────────────────── take ─────────────────────────

/// **Take the task this stretch of the run is about**: reserve it and declare it in one act.
///
/// Reserving alone would leave a task held by nobody the store can name; declaring alone would hand a
/// task on to later steps while another session was working it. So the two are one command, and it
/// succeeds only from `todo` — the compare-and-swap every reservation goes through
/// ([`crate::ops::task::set_status`]), refusal and all.
///
/// **A step takes its task on one way out.** Where the step declares a `task_take` output on several,
/// there is no answer to which of them the task was handed on through, and one picked here would be
/// wrong half the time.
pub fn take(tx: &WriteTx<'_>, run_step_id: i64, task_id: i64) -> Result<Task> {
    let run_step = live_execution(tx, run_step_id)?;
    let def = def_of(tx, &run_step)?;
    let exits = exits_of(&def)?;
    let takes: Vec<(&RunDefExit, &RunDefPort)> = exits
        .iter()
        .flat_map(|e| e.outs.iter().filter(|p| p.kind == AutomationPortKind::TaskTake).map(move |p| (e, p)))
        .collect();
    let (exit, port) = match takes.len() {
        1 => takes[0],
        0 => {
            return Err(Error::invalid(format!(
                "step '{}' declares no task to take — a step that takes one says so on a way out",
                def.name
            )))
        }
        n => {
            return Err(Error::invalid(format!(
                "step '{}' declares a task to take on {n} ways out, and which of them handed it on is \
                 not a thing this can answer — declare it on one",
                def.name
            )))
        }
    };
    let task = crate::ops::task::set_status(tx, task_id, TaskStatus::InProgress)?;
    // The stretch is what the task belongs to: a run walks several in turn, and every step of this one
    // is about this task from here on.
    if let Some(stretch_id) = run_step.run_task_id {
        if let Some(before) = read::automation_run_task(tx.conn(), stretch_id)? {
            let mut after = before.clone();
            after.task_id = Some(task.id);
            after.updated_at = Timestamp::now();
            crate::ops::emit_update(
                tx,
                record::automation_run_task(&before),
                record::automation_run_task(&after),
            )?;
        }
    }
    put(tx, &run_step, port, exit, Produced::Task(task.id))?;
    Ok(task)
}

// ───────────────────────── out ─────────────────────────

/// **Put down one thing this step produced**, on the output its id names — which is also the way out
/// it belongs to, for the reason this module opens with.
///
/// What is checked is that the id is one of this step's outputs and that what is being put down is the
/// kind the declaration asked for — a file written into a `value` port would be a row no wire could
/// carry and no screen could draw. An id that is not an output here is refused naming the ones that
/// are, so the agent types it again from the list.
///
/// Putting the same output down twice replaces the first: a step that corrects itself before reporting
/// is saying the later one is the answer, and two rows on one port would leave a wire choosing.
pub fn out(
    tx: &WriteTx<'_>,
    run_step_id: i64,
    port_id: i64,
    produced: Produced<'_>,
) -> Result<AutomationRunValue> {
    let run_step = live_execution(tx, run_step_id)?;
    let def = def_of(tx, &run_step)?;
    let exits = exits_of(&def)?;
    let (exit, port) = declared_out(&exits, port_id).ok_or_else(|| {
        let declared = exits
            .iter()
            .flat_map(|e| e.outs.iter())
            .map(|p| format!("{} ({})", p.id, p.name))
            .collect::<Vec<_>>();
        Error::invalid(match declared.is_empty() {
            true => format!("step '{}' declares nothing to hand on", def.name),
            false => format!(
                "step '{}' declares no output '{port_id}' — the ones it does: {}",
                def.name,
                declared.join(", ")
            ),
        })
    })?;
    put(tx, &run_step, port, exit, produced)
}

/// Write one produced value down on its output, replacing whatever stood on the same one.
fn put(
    tx: &WriteTx<'_>,
    run_step: &AutomationRunStep,
    port: &RunDefPort,
    exit: &RunDefExit,
    produced: Produced<'_>,
) -> Result<AutomationRunValue> {
    let (value, attachment_id, task_id) = match (port.kind, produced) {
        (AutomationPortKind::Value, Produced::Value(v)) => (Some(v.to_string()), None, None),
        (AutomationPortKind::File, Produced::File(id)) => {
            let held = read::attachment(tx.conn(), id)?
                .ok_or_else(|| not_found("attachment", id))?;
            if held.target_type != AttachmentTarget::AutomationRunStep
                || held.target_id != run_step.id
            {
                return Err(Error::invalid(format!(
                    "attachment '{id}' does not hang off this step execution — attach the file to it \
                     first, then hand it on"
                )));
            }
            (None, Some(id), None)
        }
        (AutomationPortKind::TaskTake | AutomationPortKind::TaskMake, Produced::Task(id)) => {
            (None, None, Some(id))
        }
        (kind, _) => {
            return Err(Error::invalid(format!(
                "'{}' hands on a {}, and what was put down is not one",
                port.name,
                kind.as_str()
            )))
        }
    };
    for standing in read::automation_run_values_of(tx.conn(), run_step.id)? {
        if standing.direction == AutomationPortDirection::Out && standing.port_id == port.id {
            tx.delete_record("automation_run_value", standing.id)?;
        }
    }
    let now = Timestamp::now();
    let written = AutomationRunValue {
        id: read::next_id(tx.conn(), "automation_run_value")?,
        run_step_id: run_step.id,
        direction: AutomationPortDirection::Out,
        exit_id: Some(exit.id),
        port_id: port.id,
        kind: port.kind,
        value,
        attachment_id,
        task_id,
        from_run_step_id: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run_value(&written))?;
    Ok(written)
}

// ───────────────────────── done ─────────────────────────

/// **The step is finished**: it names the way out it took, hands its report over, and the run reads the
/// picture to see what comes next.
///
/// **The way out is named by its id** (`AMB-D-961`), as the step's copy keys it — the unnamed one
/// included, which can also be said by leaving `--exit` off. An id the step does not declare is refused
/// before anything is written, naming the ones it does: the way out is the condition the picture reads
/// to pick what comes next, so one nobody drew cannot be walked, and reading it as the error one
/// stopped the run for a person over a slip of the pen (`AMB-T-5385`). The step stays running and the
/// agent types it again from the list. The error way out is always among them.
///
/// **A report is owed.** The one step that owes none is the one that went looking for a task and found
/// none: there was nothing to report on. That step's stretch is taken back down with it, which is what
/// leaves `run_task_id` empty for exactly the case the schema describes.
///
/// **What is refused is a missing required output**, named, before anything is written — a step that
/// said it was done while the thing it was built to hand on is absent has not finished, and letting the
/// run walk on would starve the next step at a distance from the cause.
pub fn done(
    tx: &WriteTx<'_>,
    run_step_id: i64,
    exit_id: Option<i64>,
    report: &str,
) -> Result<Next> {
    let run_step = live_execution(tx, run_step_id)?;
    let def = def_of(tx, &run_step)?;
    let exits = exits_of(&def)?;
    let stretch = match run_step.run_task_id {
        Some(id) => read::automation_run_task(tx.conn(), id)?,
        None => None,
    };
    let took_a_task = stretch.as_ref().is_some_and(|s| s.task_id.is_some());

    // The way out, as the step's copy declares it: by its id, or — left unsaid — the unnamed one.
    let Some(taken) = exits.iter().find(|e| match exit_id {
        Some(id) => e.id == id,
        None => e.name.is_none(),
    }) else {
        return Err(undeclared(&def.name, exit_id, &exits));
    };

    if report.trim().is_empty() && took_a_task {
        return Err(Error::invalid(
            "a step owes a report of what it did — only one that went looking for a task and found \
             none has nothing to report on",
        ));
    }

    let produced = read::automation_run_values_of(tx.conn(), run_step.id)?;
    let missing: Vec<String> = taken
        .outs
        .iter()
        .filter(|p| p.required)
        .filter(|p| {
            !produced
                .iter()
                .any(|v| v.direction == AutomationPortDirection::Out && v.port_id == p.id)
        })
        .map(|p| format!("{} ({})", p.id, p.name))
        .collect();
    if !missing.is_empty() {
        return Err(Error::invalid(format!(
            "step '{}' has not handed on what leaving through {} requires: {}",
            def.name,
            named(taken.name.as_deref()),
            missing.join(", "),
        )));
    }

    let now = Timestamp::now();
    let mut ended = run_step.clone();
    ended.exit_id = Some(taken.id);
    ended.report = report.to_string();
    ended.status = AutomationRunStepStatus::Done;
    ended.ended_at = Some(now);
    ended.updated_at = now;
    crate::ops::emit_update(
        tx,
        record::automation_run_step(&run_step),
        record::automation_run_step(&ended),
    )?;

    // A step that closed its task with `task done --report` has already said its piece there; a closed
    // task is read by nobody, and the report stays on the run's history (`AMB-D-963`).
    if def.report_to_task && !report.trim().is_empty() {
        if let Some(task_id) = stretch.as_ref().and_then(|s| s.task_id) {
            if !closed(tx, task_id)? {
                crate::ops::comment::add_report_comment(
                    tx,
                    task_id,
                    ActorKind::Ai,
                    report,
                    ended.id,
                )?;
            }
        }
    }
    if !took_a_task {
        no_task_after_all(tx, &ended, stretch.as_ref(), now)?;
    }
    whats_next(tx, &def, &ended, taken.id)
}

/// Is the task closed, so that a line on it would be read by nobody? The same test core refuses a
/// comment by, so the run and the refusal cannot drift apart.
pub(crate) fn closed(tx: &WriteTx<'_>, task_id: i64) -> Result<bool> {
    Ok(read::task_status(tx.conn(), task_id)?.is_some_and(|status| status.is_closed()))
}

/// The refusal of a way out the step does not declare, carrying the ones it does as they are typed, so
/// the agent can say it again from here rather than go and look.
fn undeclared(step: &str, said: Option<i64>, exits: &[RunDefExit]) -> Error {
    let said = match said {
        Some(id) => format!("--exit {id}"),
        None => "left unnamed (--exit left off)".to_string(),
    };
    let ways: Vec<String> = exits
        .iter()
        .map(|e| {
            let what = match e.name.as_deref() {
                Some(ERROR_EXIT) => "the error way out, for a step that could not finish".to_string(),
                Some(name) => format!("\"{name}\""),
                None => "the unnamed way out".to_string(),
            };
            format!("--exit {} ({what})", e.id)
        })
        .collect();
    Error::invalid(format!(
        "step '{step}' does not declare a way out {said}, so nothing was recorded and the step is still \
         running. Finish it again with one it declares: {}",
        ways.join("; "),
    ))
}

/// A step that went looking for a task and found none leaves no stretch behind it. The row was raised
/// when the step opened, before anything could know whether a task would turn up; with none, it is a
/// stretch of a run about nothing, and the schema says such an execution carries no stretch at all.
fn no_task_after_all(
    tx: &WriteTx<'_>,
    ended: &AutomationRunStep,
    stretch: Option<&AutomationRunTask>,
    now: Timestamp,
) -> Result<()> {
    let Some(stretch) = stretch else { return Ok(()) };
    let mut loose = ended.clone();
    loose.run_task_id = None;
    loose.updated_at = now;
    crate::ops::emit_update(
        tx,
        record::automation_run_step(ended),
        record::automation_run_step(&loose),
    )?;
    // Only where this execution was the whole of it: a stretch several steps walked is a record of
    // those steps, whether or not a task was ever named.
    let walked = read::automation_run_steps_of_task(tx.conn(), stretch.id)?;
    if walked.iter().all(|s| s.id == ended.id) {
        tx.delete_record("automation_run_task", stretch.id)?;
    }
    Ok(())
}

/// Say what happens after the way out that was taken, as the step's copy says.
///
/// **The line is read off the copy** (`AMB-D-961`): what follows each way out was resolved at launch,
/// across the action's edge where the line inside returns to one of the action's ways out
/// ([`automation_run::onward`]). A run under way reads no picture but its own copies.
///
/// **A way out nothing decides stops the run.** The launch check refuses an automation with one, so
/// reaching this means a copy carried in from before a launch copied the lines — and walking on from
/// a way out that says nothing would be the run choosing for itself.
///
/// **A pause that was asked for is answered here and nowhere else**, because this is the one moment a
/// step is known to have finished. It is read last of all: a picture that has run out is over, and
/// pausing a run that has ended would leave one nobody could pick up again.
fn whats_next(
    tx: &WriteTx<'_>,
    def: &AutomationRunDef,
    ended: &AutomationRunStep,
    taken: i64,
) -> Result<Next> {
    let conn = tx.conn();
    let run = read::automation_run(conn, def.run_id)?.ok_or_else(|| not_found("run", def.run_id))?;
    match automation_run::onward(conn, def, Some(taken))? {
        Onward::Done => {
            Ok(Next::Closed(automation_stop::ended(tx, run, Ending::Completed)?))
        }
        // The way out calls a person: an ending the author of the picture chose.
        Onward::Halt => Ok(Next::Halted(failed(tx, run, AutomationStoppedReason::Halted)?)),
        // Nothing says what follows the way out — or it leads somewhere the run never copied down,
        // such as a placement added after the launch. The run has no snapshot of that and will not
        // read a live one, so there is nowhere to go.
        Onward::Nowhere => Ok(Next::Halted(failed(tx, run, AutomationStoppedReason::NoWayOn)?)),
        Onward::Go { def: next, line } => {
            if over_its_turns(tx, def, ended, &line)? {
                return Ok(Next::Halted(failed(tx, run, AutomationStoppedReason::MaxTimes)?));
            }
            if run.pause_requested {
                return Ok(Next::Paused(automation_stop::settle(tx, run)?));
            }
            Ok(Next::Step(next))
        }
    }
}

/// Fail the run, through the one cleanup every ending goes through
/// ([`super::automation_stop::ended`]).
fn failed(tx: &WriteTx<'_>, run: AutomationRun, reason: AutomationStoppedReason) -> Result<Ended> {
    automation_stop::ended(tx, run, Ending::Failed(reason))
}

/// **Has this way back been taken as often as it is allowed to be?**
///
/// What is counted is this edge, within this stretch of the run: how many times the spot it leaves
/// from has already reported that way out for the task under way. The count starts again at every
/// task, because the limit is there to catch a review that never converges on one piece of work rather
/// than to cap how much work a run may do.
///
/// **The spot is the one the edge is drawn from.** A line inside an action leaves one of its steps, and
/// is counted on that step within the placement the run is walking — the same action placed twice is
/// two loops, not one. A line on the automation leaves a placement by one of the action's ways out, and
/// is counted on every step of that placement that returned to it.
///
/// An edge with no limit is never over its turns — which is the right answer for one leading into a
/// spot that takes a fresh task, since that edge is walked once per task by design
/// ([`crate::model::AutomationEdge::max_times`]). **Nor is a line that went down** when the run was
/// launched: the copy keeps a limit on a way back alone ([`crate::ops::automation::lines_back`]).
///
/// The execution that has just reported is counted with the rest: it is already stamped `done` and
/// carrying its way out by the time this is asked, so the count is how many times the edge would have
/// been taken including this one.
///
/// **Everything is read off the run's copies** (`AMB-D-961`): the line as it was copied at launch, and
/// which of the action's ways out each step's way out returned to ([`RunDefExit::returns_to`]).
fn over_its_turns(
    tx: &WriteTx<'_>,
    from: &AutomationRunDef,
    ended: &AutomationRunStep,
    edge: &RunDefLine,
) -> Result<bool> {
    let Some(limit) = edge.max_times else { return Ok(false) };
    // A step that went looking for a task and found none left no stretch behind it, and a per-task
    // limit has nothing to count against.
    let Some(stretch) = ended.run_task_id else { return Ok(false) };
    let conn = tx.conn();
    let mut taken = 0;
    for step in read::automation_run_steps_of_task(conn, stretch)? {
        let Some(def) = read::automation_run_def(conn, step.run_def_id)? else { continue };
        let Some(step_id) = def.step_id else { continue };
        let took_it = match edge.picture {
            AutomationPictureOwner::Action => {
                def.placement_id == from.placement_id
                    && step_id == edge.from_id
                    && step.exit_id == Some(edge.exit_id)
            }
            AutomationPictureOwner::Automation => {
                def.placement_id == Some(edge.from_id)
                    && match step.exit_id {
                        Some(exit) => {
                            exits_of(&def)?.iter().find(|e| e.id == exit).and_then(|e| e.returns_to)
                                == Some(edge.exit_id)
                        }
                        None => false,
                    }
            }
        };
        if took_it {
            taken += 1;
        }
    }
    Ok(taken > limit)
}

/// How a way out is spoken of in a sentence: by its name, or as the unnamed one.
fn named(exit: Option<&str>) -> String {
    match exit {
        Some(ERROR_EXIT) => "the error way out".to_string(),
        Some(name) => format!("\"{name}\""),
        None => "the unnamed way out".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Automation, AutomationAction, AutomationPictureOwner, AutomationPlacement,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_run::{launch, Launcher};
    use crate::ops::automation_step::{open, Opened, Opening};
    use crate::ops::test_support::{
        mk_exit, mk_in, mk_out, mk_placed, mk_project, out_port, way_out, with_tx,
    };

    /// The picture these tests walk: a spot that takes a task and hands a note on through "found", and
    /// a second spot wired to read it. Both ways out of both are decided, so it launches.
    struct Picture {
        automation: Automation,
        project: i64,
        first_action: AutomationAction,
        first: AutomationPlacement,
        second: AutomationPlacement,
    }

    fn picture(tx: &WriteTx<'_>, note_required: bool) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(
            tx,
            project,
            NewAutomation { name: "1件やりきる".into(), notes: String::new() },
        )
        .expect("add automation");
        let (first_action, first) = mk_placed(tx, &automation, "調べる", "look at it", "claude");
        let (second_action, second) = mk_placed(tx, &automation, "直す", "fix it", "claude");

        mk_exit(tx, &first_action, "found");
        mk_out(tx, &first_action, Some("found"), "タスク", AutomationPortKind::TaskTake, true);
        mk_out(tx, &first_action, Some("found"), "note", AutomationPortKind::Value, note_required);
        mk_in(tx, &second_action, "note", AutomationPortKind::Value, false);
        automation::wire_add(
            tx,
            AutomationPictureOwner::Automation,
            first.id,
            Some("found"),
            "note",
            second.id,
            "note",
        )
        .expect("wire");

        let on = AutomationPictureOwner::Automation;
        automation::edge_add(tx, on, first.id, Some("found"), EdgeTarget::Go(second.id), None)
            .expect("onward");
        automation::edge_add(tx, on, first.id, None, EdgeTarget::Done, None).expect("closes");
        automation::edge_add(tx, on, first.id, Some(ERROR_EXIT), EdgeTarget::Halt, None)
            .expect("error");
        automation::edge_add(tx, on, second.id, None, EdgeTarget::Done, None)
            .expect("second closes");
        automation::edge_add(tx, on, second.id, Some(ERROR_EXIT), EdgeTarget::Halt, None)
            .expect("second error");
        let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, project, first_action, first, second }
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
        placement: &AutomationPlacement,
    ) -> AutomationRunDef {
        read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.placement_id == Some(placement.id))
            .expect("the spot's snapshot")
    }

    fn opened(tx: &WriteTx<'_>, run: &AutomationRun, placement: &AutomationPlacement) -> Opening {
        match open(tx, run.id, def_of(tx, run, placement).id, None).expect("open") {
            Opened::Ready(opening) => *opening,
            Opened::Stopped { missing, .. } => panic!("stopped for {missing:?}"),
            Opened::NoAgent { agent, .. } => panic!("cannot start {agent}"),
            Opened::Carried { .. } => panic!("not a built-in"),
            Opened::LeftTaskOpen { .. } => panic!("left a task open"),
        }
    }

    /// A task to work on, ready to be reserved.
    fn a_task(tx: &WriteTx<'_>, project: i64, title: &str) -> Task {
        let task = crate::ops::task::add(
            tx,
            crate::ops::task::NewTask {
                title: title.to_string(),
                project_id: Some(project),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(ActorKind::Ai),
                at_binding_id: None,
                made_in: None,
            },
        )
        .expect("task");
        crate::ops::task::finish_creating(tx, task.id).expect("finish creating")
    }

    fn outs(tx: &WriteTx<'_>, run_step_id: i64) -> Vec<AutomationRunValue> {
        read::automation_run_values_of(tx.conn(), run_step_id)
            .expect("values")
            .into_iter()
            .filter(|v| v.direction == AutomationPortDirection::Out)
            .collect()
    }

    /// **What a name was declared to take is readable before anything is put down** (`AMB-T-5279`).
    ///
    /// It is what lets the side a person types on say the right command per kind, rather than letting
    /// them find out from a refusal about kinds.
    #[test]
    fn what_a_declared_name_carries_is_readable_by_that_name() {
        with_tx(|tx| {
            let p = picture(tx, false);
            mk_out(tx, &p.first_action, Some("found"), "raised", AutomationPortKind::TaskMake, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first).run_step;

            let kind = |name: &str| {
                let port = match name {
                    "nothing of the sort" => 999_999,
                    name => out_port(tx, step.id, None, name),
                };
                out_kind(tx.conn(), step.id, port).expect("read")
            };
            assert_eq!(kind("note"), Some(AutomationPortKind::Value));
            assert_eq!(kind("タスク"), Some(AutomationPortKind::TaskTake));
            assert_eq!(kind("raised"), Some(AutomationPortKind::TaskMake));
            assert_eq!(kind("nothing of the sort"), None, "a name nobody declared");
        });
    }

    /// **A task the step raised along the way is handed on like anything else, and is not reserved.**
    ///
    /// It is a product of the step, not the task the run is working: nothing moves its status, and the
    /// run's stretch goes on naming the task it took ([`take`]).
    #[test]
    fn a_task_raised_along_the_way_is_handed_on_without_being_reserved() {
        with_tx(|tx| {
            let p = picture(tx, false);
            mk_out(tx, &p.first_action, Some("found"), "raised", AutomationPortKind::TaskMake, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first).run_step;
            let working = a_task(tx, p.project, "調べる");
            take(tx, step.id, working.id).expect("take");
            let raised = a_task(tx, p.project, "あとで直す");

            out(tx, step.id, out_port(tx, step.id, None, "raised"), Produced::Task(raised.id)).expect("hand it on");

            let handed = outs(tx, step.id);
            let one = handed.iter().find(|v| v.port_id == out_port(tx, step.id, None, "raised")).expect("the raised task");
            assert_eq!(one.task_id, Some(raised.id));
            assert_eq!(
                read::task_status(tx.conn(), raised.id).expect("read"),
                Some(crate::model::TaskStatus::Todo),
                "nothing reserved it — whoever comes to it next picks it up",
            );
            let stretch = read::automation_run_task_last(tx.conn(), run.id)
                .expect("read")
                .expect("the stretch");
            assert_eq!(stretch.task_id, Some(working.id), "the run is still working the task it took");
        });
    }

    #[test]
    fn taking_a_task_reserves_it_and_declares_it_in_one_act() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — the one to work");

            let taken = take(tx, step.run_step.id, task.id).expect("take");
            assert_eq!(taken.status, TaskStatus::InProgress);

            let stretch = read::automation_run_task(tx.conn(), step.run_step.run_task_id.unwrap())
                .expect("read")
                .expect("the stretch");
            assert_eq!(stretch.task_id, Some(task.id), "the stretch is about this task");

            let declared = outs(tx, step.run_step.id);
            assert_eq!(declared.len(), 1);
            assert_eq!(declared[0].kind, AutomationPortKind::TaskTake);
            assert_eq!(declared[0].task_id, Some(task.id));
            assert_eq!(
                declared[0].exit_id,
                way_out(tx, step.run_step.id, "found"),
                "the one way out that declares it was never in question",
            );
        });
    }

    #[test]
    fn a_task_somebody_else_holds_is_not_taken() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — already held");
            crate::ops::task::set_status(tx, task.id, TaskStatus::InProgress).expect("reserve");

            let refused = take(tx, step.run_step.id, task.id).expect_err("already reserved");
            assert!(refused.to_string().contains("cannot reserve"), "{refused}");
            assert!(outs(tx, step.run_step.id).is_empty(), "nothing was declared either");
        });
    }

    /// **An output is one way out's** (`AMB-D-961`). Two ways out declaring an output of one name are two
    /// ports, and a value put down on one belongs to its way out from that moment: leaving by the other
    /// way out finds its own output still missing, and the value on the first stays behind with the way
    /// out it was put down on.
    #[test]
    fn a_value_is_put_down_on_one_way_out_s_output() {
        with_tx(|tx| {
            let p = picture(tx, true);
            mk_out(tx, &p.first_action, None, "note", AutomationPortKind::Value, true);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first).run_step;
            let found = way_out(tx, step.id, "found");
            let on_found = out_port(tx, step.id, found, "note");
            let unnamed = exits_of(&read::automation_run_def(tx.conn(), step.run_def_id).unwrap().unwrap())
                .unwrap()
                .into_iter()
                .find(|e| e.name.is_none())
                .expect("the unnamed way out")
                .id;
            let on_unnamed = out_port(tx, step.id, Some(unnamed), "note");
            assert_ne!(on_found, on_unnamed, "one name, two ports");

            let put = out(tx, step.id, on_found, Produced::Value("for found")).expect("out");
            assert_eq!(put.exit_id, found, "it is the way out's the moment it is put down");
            let refused = done(tx, step.id, Some(unnamed), "Nothing to fix.").expect_err("still missing");
            assert!(refused.to_string().contains(&format!("{on_unnamed} (note)")), "{refused}");

            out(tx, step.id, on_unnamed, Produced::Value("for the unnamed")).expect("out");
            done(tx, step.id, Some(unnamed), "Nothing to fix.").expect("done");
            let kept = outs(tx, step.id);
            assert_eq!(kept.len(), 2, "the value on the other way out is kept as the record");
            assert!(kept.iter().any(|v| v.port_id == on_found && v.exit_id == found));
        });
    }

    #[test]
    fn what_is_put_down_has_to_be_declared_and_of_the_declared_kind() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);

            let unknown = out(tx, step.run_step.id, 999_999, Produced::Value("x"))
                .expect_err("nobody declared it");
            assert!(unknown.to_string().contains("declares no output '999999'"), "{unknown}");
            assert!(unknown.to_string().contains("(note)"), "and names the ones it does: {unknown}");

            let wrong_kind = out(tx, step.run_step.id, out_port(tx, step.run_step.id, None, "note"), Produced::Task(1))
                .expect_err("a note is a value");
            assert!(wrong_kind.to_string().contains("hands on a value"), "{wrong_kind}");

            out(tx, step.run_step.id, out_port(tx, step.run_step.id, None, "note"), Produced::Value("what I found")).expect("out");
        });
    }

    #[test]
    fn putting_the_same_name_down_twice_leaves_the_later_one() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);

            out(tx, step.run_step.id, out_port(tx, step.run_step.id, None, "note"), Produced::Value("first")).expect("out");
            out(tx, step.run_step.id, out_port(tx, step.run_step.id, None, "note"), Produced::Value("second")).expect("out again");
            let standing = outs(tx, step.run_step.id);
            assert_eq!(standing.len(), 1);
            assert_eq!(standing[0].value.as_deref(), Some("second"));
        });
    }

    #[test]
    fn the_way_out_is_stamped_across_what_was_put_down_and_the_run_walks_on() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — the one to work");
            take(tx, step.run_step.id, task.id).expect("take");
            out(tx, step.run_step.id, out_port(tx, step.run_step.id, None, "note"), Produced::Value("what I found")).expect("out");

            let found = way_out(tx, step.run_step.id, "found");
            let next = done(tx, step.run_step.id, found, "Looked at it.").expect("done");
            match next {
                Next::Step(def) => assert_eq!(def.step_id, Some(p.second.id)),
                other => panic!("the edge goes on to the second step: {other:?}"),
            }
            let note = outs(tx, step.run_step.id)
                .into_iter()
                .find(|v| v.port_id == out_port(tx, step.run_step.id, None, "note"))
                .expect("the note");
            assert_eq!(note.exit_id, found);

            let ended = read::automation_run_step(tx.conn(), step.run_step.id)
                .expect("read")
                .expect("the execution");
            assert_eq!(ended.status, AutomationRunStepStatus::Done);
            assert_eq!(ended.exit_id, found);
            assert_eq!(ended.report, "Looked at it.");
        });
    }

    #[test]
    fn a_required_output_that_is_missing_is_named_and_the_step_does_not_end() {
        with_tx(|tx| {
            let p = picture(tx, true);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — the one to work");
            take(tx, step.run_step.id, task.id).expect("take");

            let refused = done(tx, step.run_step.id, way_out(tx, step.run_step.id, "found"), "Looked at it.")
                .expect_err("the note is required");
            assert!(refused.to_string().contains("note"), "{refused}");
            let still = read::automation_run_step(tx.conn(), step.run_step.id)
                .expect("read")
                .expect("the execution");
            assert_eq!(still.status, AutomationRunStepStatus::Running, "it has not ended");
        });
    }

    #[test]
    fn a_report_is_owed_by_every_step_that_took_a_task() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — the one to work");
            take(tx, step.run_step.id, task.id).expect("take");

            let refused =
                done(tx, step.run_step.id, way_out(tx, step.run_step.id, "found"), "   ")
                .expect_err("a report is owed");
            assert!(refused.to_string().contains("owes a report"), "{refused}");
        });
    }

    #[test]
    fn a_step_that_found_no_task_owes_no_report_and_leaves_no_stretch() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let stretch_id = step.run_step.run_task_id.expect("a stretch was opened");

            let next = done(tx, step.run_step.id, None, "")
                .expect("done with nothing to say");
            assert!(matches!(next, Next::Closed(_)), "the unnamed way out closes the run: {next:?}");
            let ended = read::automation_run_step(tx.conn(), step.run_step.id)
                .expect("read")
                .expect("the execution");
            assert_eq!(ended.run_task_id, None, "it went looking and found none");
            assert!(
                read::automation_run_task(tx.conn(), stretch_id).expect("read").is_none(),
                "a stretch of a run about nothing is not kept",
            );
        });
    }

    #[test]
    fn a_way_out_nobody_declared_is_refused_naming_the_declared_ones_and_the_step_stays_running() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — the one to work");
            take(tx, step.run_step.id, task.id).expect("take");

            // An id no way out of this step carries — another step's, or none at all.
            let found = way_out(tx, step.run_step.id, "found").expect("found");
            let error = way_out(tx, step.run_step.id, ERROR_EXIT).expect("the error way out");
            for said in [9_999, -1] {
                let err = done(tx, step.run_step.id, Some(said), "Did the thing.")
                    .expect_err("a way out the step does not declare");
                assert_eq!(err.code(), "invalid_value");
                let message = err.to_string();
                assert!(message.contains(&format!("--exit {said}")), "what was said: {message}");
                assert!(message.contains(&format!("--exit {found} (\"found\")")), "{message}");
                assert!(message.contains("(the unnamed way out)"), "{message}");
                assert!(message.contains(&format!("--exit {error} (the error way out")), "{message}");
            }
            let still = read::automation_run_step(tx.conn(), step.run_step.id)
                .expect("read")
                .expect("the execution");
            assert_eq!(still.status, AutomationRunStepStatus::Running, "nothing was recorded");
            assert_eq!(still.exit_id, None);
            assert!(still.report.is_empty(), "{}", still.report);

            // Said again from the list, it finishes.
            done(tx, step.run_step.id, Some(error), "Could not.").expect("the error way out");
        });
    }

    #[test]
    fn a_step_built_to_report_to_its_task_leaves_the_line_with_its_provenance() {
        with_tx(|tx| {
            let p = picture(tx, false);
            automation::step_update(tx, p.first.id, None, None, None, None, Some(true), None)
            .expect("report to task");
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — the one to work");
            take(tx, step.run_step.id, task.id).expect("take");
            done(tx, step.run_step.id, way_out(tx, step.run_step.id, "found"), "Looked at it.")
                .expect("done");

            let ids = read::task_comment_ids(tx.conn(), task.id).expect("comments");
            assert_eq!(ids.len(), 1);
            let line = read::task_comment(tx.conn(), ids[0]).expect("read").expect("the line");
            assert_eq!(line.text, "Looked at it.");
            assert_eq!(
                line.automation_run_step_id,
                Some(step.run_step.id),
                "which step of which run carried it",
            );
        });
    }

    /// **A step that closed its task still finishes, and leaves no line on it** (`AMB-D-963`).
    ///
    /// The agent closes the task with `task done --report` inside the step; the step's own report then
    /// goes to the run's history alone, and `step-done` must not fail on the task being closed.
    #[test]
    fn a_step_that_closed_its_task_finishes_without_a_line_on_it() {
        with_tx(|tx| {
            let p = picture(tx, false);
            automation::step_update(tx, p.first.id, None, None, None, None, Some(true), None)
                .expect("report to task");
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "閉じる");
            take(tx, step.run_step.id, task.id).expect("take");
            crate::ops::task::set_status(tx, task.id, TaskStatus::Done).expect("the agent closes it");

            done(tx, step.run_step.id, way_out(tx, step.run_step.id, "found"), "Looked at it.")
                .expect("the step finishes all the same");

            assert!(
                read::task_comment_ids(tx.conn(), task.id).expect("comments").is_empty(),
                "a closed task is read by nobody",
            );
            let ended = read::automation_run_step(tx.conn(), step.run_step.id)
                .expect("read")
                .expect("the step");
            assert_eq!(ended.report, "Looked at it.", "the report stays on the run's history");
        });
    }

    #[test]
    fn a_step_that_has_already_reported_has_nothing_more_to_say() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            done(tx, step.run_step.id, None, "")
                .expect("done");

            let refused = out(tx, step.run_step.id, out_port(tx, step.run_step.id, None, "note"), Produced::Value("late"))
                .expect_err("it has ended");
            assert!(refused.to_string().contains("only a running one"), "{refused}");
        });
    }
}
