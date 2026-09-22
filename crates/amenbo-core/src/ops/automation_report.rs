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
//! run. [`out`] puts down each thing the step produced, under the name its port was declared with.
//! [`done`] names the way out, and that name is the whole condition the next step is chosen by.
//!
//! **A way out is stamped onto what was produced at the end, not at the moment of production.** Two
//! ways out of one step may each declare an output called `report`, and until the step says which it
//! took there is no answer to which one a value belongs to. So [`out`] writes the value with no way out
//! against it and [`done`] stamps the taken one across them — which also means a step that died
//! half-way leaves values nothing can read, since a wire finds a value by the way out it left through.

use crate::error::{Error, Result};
use crate::model::{
    ActorKind, AttachmentTarget, AutomationEnds, AutomationPortDirection, AutomationPortKind,
    AutomationRun, AutomationRunDef, AutomationRunStatus, AutomationRunStep, AutomationRunStepStatus,
    AutomationRunTask, AutomationRunValue, AutomationStoppedReason, RunDefExit, RunDefPort, Task,
    TaskStatus, ERROR_EXIT,
};
use crate::ops::automation_stop::{self, Ended};
use crate::ops::emit_create;
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
    /// The run is over and the lane is free. `woke` is the run that took that lane, where one was
    /// waiting — a step of **that** run is what to open next ([`super::automation_stop::Ended`]).
    Closed(Ended),
    /// The run is stopped and a person is owed a look — either because the way out says so, or because
    /// nothing says what happens after it.
    Halted(Ended),
    /// Somebody pressed pause while this step was under way, and this is the end of it. The next step
    /// is not opened; the run keeps its task and gives up its lane
    /// ([`super::automation_stop::resume`] picks it up from the same way out).
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

/// Find one declared output by name, wherever among the ways out it was declared. The name is enough:
/// which way out it belongs to is settled at [`done`], not here.
fn declared_out<'a>(exits: &'a [RunDefExit], name: &str) -> Option<&'a RunDefPort> {
    exits.iter().find_map(|e| e.outs.iter().find(|p| p.name == name))
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
    put(tx, &run_step, port, Some(exit), Produced::Task(task.id))?;
    Ok(task)
}

// ───────────────────────── out ─────────────────────────

/// **Put down one thing this step produced**, under the name its port was declared with.
///
/// The way out it belongs to is left open here and stamped at [`done`], for the reason this module
/// opens with. What is checked is that the name was declared at all and that what is being put down is
/// the kind the declaration asked for — a file written into a `value` port would be a row no wire could
/// carry and no screen could draw.
///
/// Putting the same name down twice replaces the first: a step that corrects itself before reporting is
/// saying the later one is the answer, and two rows under one name would leave a wire choosing.
pub fn out(
    tx: &WriteTx<'_>,
    run_step_id: i64,
    name: &str,
    produced: Produced<'_>,
) -> Result<AutomationRunValue> {
    let run_step = live_execution(tx, run_step_id)?;
    let def = def_of(tx, &run_step)?;
    let exits = exits_of(&def)?;
    let port = declared_out(&exits, name).ok_or_else(|| {
        Error::invalid(format!("step '{}' declares nothing called '{name}' to hand on", def.name))
    })?;
    put(tx, &run_step, port, None, produced)
}

/// Write one produced value down, replacing whatever stood under the same name.
///
/// `exit` is given only where the way out is already settled — [`take`], whose task came out of the one
/// way out that declares it. Everything else is stamped at [`done`].
fn put(
    tx: &WriteTx<'_>,
    run_step: &AutomationRunStep,
    port: &RunDefPort,
    exit: Option<&RunDefExit>,
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
        if standing.direction == AutomationPortDirection::Out && standing.name == port.name {
            tx.delete_record("automation_run_value", standing.id)?;
        }
    }
    let now = Timestamp::now();
    let written = AutomationRunValue {
        id: read::next_id(tx.conn(), "automation_run_value")?,
        run_step_id: run_step.id,
        direction: AutomationPortDirection::Out,
        exit_name: exit.and_then(|e| e.name.clone()),
        name: port.name.clone(),
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
/// **A name nobody declared is read as the error way out, and what was said is kept.** An agent that
/// invents a way out has not done what was asked, and guessing which of the real ones it meant would
/// send the run down a road on a guess. The claim goes in front of the report so a person reading the
/// run can see what happened rather than only that it errored.
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
    exit_name: Option<&str>,
    report: &str,
    lanes: i64,
) -> Result<Next> {
    let run_step = live_execution(tx, run_step_id)?;
    let def = def_of(tx, &run_step)?;
    let exits = exits_of(&def)?;
    let stretch = match run_step.run_task_id {
        Some(id) => read::automation_run_task(tx.conn(), id)?,
        None => None,
    };
    let took_a_task = stretch.as_ref().is_some_and(|s| s.task_id.is_some());

    // The way out, as the step named it and as the run is able to read it.
    let (taken, misnamed) = match exit_name {
        Some(ERROR_EXIT) => (Some(ERROR_EXIT.to_string()), None),
        named => match exits.iter().any(|e| e.name.as_deref() == named) {
            true => (named.map(str::to_string), None),
            false => (Some(ERROR_EXIT.to_string()), named.map(str::to_string)),
        },
    };

    if report.trim().is_empty() && took_a_task {
        return Err(Error::invalid(
            "a step owes a report of what it did — only one that went looking for a task and found \
             none has nothing to report on",
        ));
    }

    let produced = read::automation_run_values_of(tx.conn(), run_step.id)?;
    let missing: Vec<String> = exits
        .iter()
        .filter(|e| e.name.as_deref() == taken.as_deref())
        .flat_map(|e| e.outs.iter())
        .filter(|p| p.required)
        .filter(|p| {
            !produced
                .iter()
                .any(|v| v.direction == AutomationPortDirection::Out && v.name == p.name)
        })
        .map(|p| p.name.clone())
        .collect();
    if !missing.is_empty() {
        return Err(Error::invalid(format!(
            "step '{}' has not handed on what leaving through {} requires: {}",
            def.name,
            named(taken.as_deref()),
            missing.join(", "),
        )));
    }

    let now = Timestamp::now();
    // Every value this step put down belongs to the way out it turned out to take. A value already
    // carrying one came from `take`, where the way out was never in question.
    for standing in produced {
        if standing.direction != AutomationPortDirection::Out || standing.exit_name.is_some() {
            continue;
        }
        let mut stamped = standing.clone();
        stamped.exit_name = taken.clone();
        stamped.updated_at = now;
        crate::ops::emit_update(
            tx,
            record::automation_run_value(&standing),
            record::automation_run_value(&stamped),
        )?;
    }

    let written = match &misnamed {
        Some(said) => format!(
            "Said it left through \"{said}\", which this step does not declare.\n\n{report}"
        ),
        None => report.to_string(),
    };
    let mut ended = run_step.clone();
    ended.exit_name = taken.clone();
    ended.report = written.clone();
    ended.status = AutomationRunStepStatus::Done;
    ended.ended_at = Some(now);
    ended.updated_at = now;
    crate::ops::emit_update(
        tx,
        record::automation_run_step(&run_step),
        record::automation_run_step(&ended),
    )?;

    if def.report_to_task && !written.trim().is_empty() {
        if let Some(task_id) = stretch.as_ref().and_then(|s| s.task_id) {
            crate::ops::comment::add_report_comment(
                tx,
                task_id,
                ActorKind::Ai,
                &written,
                ended.id,
            )?;
        }
    }
    if !took_a_task {
        no_task_after_all(tx, &ended, stretch.as_ref(), now)?;
    }
    whats_next(tx, &def, &ended, taken.as_deref(), lanes)
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

/// Read the picture and say what happens after the way out that was taken.
///
/// **The edges are read live**, like every other walk of the picture: which step comes next is the
/// shape of the automation rather than the step's own declaration, and a run under way follows the
/// shape as it stands.
///
/// **A way out nothing decides stops the run.** The launch check refuses an automation with one, so
/// reaching this means the picture was edited underneath a run — and walking on from a way out that
/// says nothing would be the run choosing for itself.
///
/// **A pause that was asked for is answered here and nowhere else**, because this is the one moment a
/// step is known to have finished. It is read last of all: a picture that has run out is over, and
/// pausing a run that has ended would leave one nobody could pick up again.
fn whats_next(
    tx: &WriteTx<'_>,
    def: &AutomationRunDef,
    ended: &AutomationRunStep,
    taken: Option<&str>,
    lanes: i64,
) -> Result<Next> {
    let conn = tx.conn();
    let run = read::automation_run(conn, def.run_id)?.ok_or_else(|| not_found("run", def.run_id))?;
    let edge = match def.step_id {
        Some(step_id) => read::automation_edge_for_exit(conn, step_id, taken)?,
        None => None,
    };
    let Some(edge) = edge else {
        return Ok(Next::Halted(stopped(tx, run, None, lanes)?));
    };
    match edge.ends {
        AutomationEnds::Done => Ok(Next::Closed(automation_stop::ended(
            tx,
            run,
            AutomationRunStatus::Done,
            None,
            lanes,
        )?)),
        AutomationEnds::Halt => Ok(Next::Halted(stopped(tx, run, None, lanes)?)),
        AutomationEnds::Go => {
            let to = edge.to_step_id;
            let next = read::automation_run_defs_of(conn, run.id)?
                .into_iter()
                .find(|d| d.step_id == to && to.is_some());
            // The edge goes to a step the run never copied down — one added after the launch. The run
            // has no snapshot of it and will not read a live one, so there is nowhere to go.
            let Some(next) = next else {
                return Ok(Next::Halted(stopped(tx, run, None, lanes)?));
            };
            if over_its_turns(tx, ended, &edge)? {
                let reason = Some(AutomationStoppedReason::MaxTimes);
                return Ok(Next::Halted(stopped(tx, run, reason, lanes)?));
            }
            if run.pause_requested {
                return Ok(Next::Paused(automation_stop::settle(tx, run, lanes)?));
            }
            Ok(Next::Step(Box::new(next)))
        }
    }
}

/// Stop the run, through the one cleanup every ending goes through
/// ([`super::automation_stop::ended`]).
fn stopped(
    tx: &WriteTx<'_>,
    run: AutomationRun,
    reason: Option<AutomationStoppedReason>,
    lanes: i64,
) -> Result<Ended> {
    automation_stop::ended(tx, run, AutomationRunStatus::Stopped, reason, lanes)
}

/// **Has this way back been taken as often as it is allowed to be?**
///
/// What is counted is this edge, within this stretch of the run: how many times the step it leaves
/// from has already reported that way out for the task under way. The count starts again at every
/// task, because the limit is there to catch a review that never converges on one piece of work rather
/// than to cap how much work a run may do.
///
/// An edge with no limit is never over its turns — which is the right answer for one leading into a
/// step that takes a fresh task, since that edge is walked once per task by design
/// ([`crate::model::AutomationEdge::max_times`]).
///
/// The execution that has just reported is counted with the rest: it is already stamped `done` and
/// carrying its way out by the time this is asked, so the count is how many times the edge would have
/// been taken including this one.
fn over_its_turns(
    tx: &WriteTx<'_>,
    ended: &AutomationRunStep,
    edge: &crate::model::AutomationEdge,
) -> Result<bool> {
    let Some(limit) = edge.max_times else { return Ok(false) };
    // A step that went looking for a task and found none left no stretch behind it, and a per-task
    // limit has nothing to count against.
    let Some(stretch) = ended.run_task_id else { return Ok(false) };
    let mut taken = 0;
    for step in read::automation_run_steps_of_task(tx.conn(), stretch)? {
        if step.exit_name.as_deref() != edge.exit_name.as_deref() {
            continue;
        }
        let Some(def) = read::automation_run_def(tx.conn(), step.run_def_id)? else { continue };
        if def.step_id == Some(edge.from_step_id) {
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
    use crate::model::{Automation, AutomationOwner, AutomationPortOwner, AutomationStep};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewStep};
    use crate::ops::automation_run::{launch, Launcher};
    use crate::ops::automation_step::{open, Opened, Opening};
    use crate::ops::test_support::{mk_project, with_tx};

    /// How many lanes the machine these tests run on has. Three, so that handing one back has
    /// somewhere to put it and nothing here is testing a queue by accident.
    const LANES: i64 = 3;

    /// The picture these tests walk: a step that takes a task and hands a note on through "found", and
    /// a second step wired to read it. Both ways out of both steps are decided, so it launches.
    struct Picture {
        automation: Automation,
        project: i64,
        first: AutomationStep,
        second: AutomationStep,
    }

    fn picture(tx: &WriteTx<'_>, note_required: bool) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(
            tx,
            project,
            NewAutomation { name: "1件やりきる".into(), notes: String::new(), preamble: String::new() },
        )
        .expect("add automation");
        let first = automation::step_add(
            tx,
            automation.id,
            NewStep::with_prompt("調べる", "look at it", "claude"),
        )
        .expect("first step");
        let second = automation::step_add(
            tx,
            automation.id,
            NewStep::with_prompt("直す", "fix it", "claude"),
        )
        .expect("second step");

        automation::exit_add(tx, AutomationOwner::Step, first.id, Some("found")).expect("way out");
        out_on(tx, &first, Some("found"), "タスク", AutomationPortKind::TaskTake, true);
        out_on(tx, &first, Some("found"), "note", AutomationPortKind::Value, note_required);
        automation::port_add(
            tx,
            AutomationPortOwner::Step,
            second.id,
            AutomationPortDirection::In,
            "note",
            AutomationPortKind::Value,
            false,
        )
        .expect("input");
        automation::wire_add(tx, first.id, Some("found"), "note", second.id, "note").expect("wire");

        automation::edge_add(tx, first.id, Some("found"), EdgeTarget::Go(second.id), None)
            .expect("onward");
        automation::edge_add(tx, first.id, None, EdgeTarget::Done, None).expect("closes");
        automation::edge_add(tx, first.id, Some(ERROR_EXIT), EdgeTarget::Halt, None).expect("error");
        automation::edge_add(tx, second.id, None, EdgeTarget::Done, None).expect("second closes");
        automation::edge_add(tx, second.id, Some(ERROR_EXIT), EdgeTarget::Halt, None)
            .expect("second error");
        let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, project, first, second }
    }

    fn out_on(
        tx: &WriteTx<'_>,
        step: &AutomationStep,
        exit_name: Option<&str>,
        name: &str,
        kind: AutomationPortKind,
        required: bool,
    ) {
        let exit =
            read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, step.id, exit_name)
                .expect("read")
                .expect("the way out");
        automation::port_add(
            tx,
            AutomationPortOwner::Exit,
            exit.id,
            AutomationPortDirection::Out,
            name,
            kind,
            required,
        )
        .expect("output");
    }

    fn a_run(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let startable = vec!["claude".to_string()];
        let by = Launcher {
            startable: Some(&startable),
            lanes: 3,
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
                declared[0].exit_name.as_deref(),
                Some("found"),
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

    #[test]
    fn what_is_put_down_has_to_be_declared_and_of_the_declared_kind() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);

            let unknown = out(tx, step.run_step.id, "nothing", Produced::Value("x"))
                .expect_err("nobody declared it");
            assert!(unknown.to_string().contains("nothing called 'nothing'"), "{unknown}");

            let wrong_kind = out(tx, step.run_step.id, "note", Produced::Task(1))
                .expect_err("a note is a value");
            assert!(wrong_kind.to_string().contains("hands on a value"), "{wrong_kind}");

            out(tx, step.run_step.id, "note", Produced::Value("what I found")).expect("out");
        });
    }

    #[test]
    fn putting_the_same_name_down_twice_leaves_the_later_one() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);

            out(tx, step.run_step.id, "note", Produced::Value("first")).expect("out");
            out(tx, step.run_step.id, "note", Produced::Value("second")).expect("out again");
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
            out(tx, step.run_step.id, "note", Produced::Value("what I found")).expect("out");

            let next = done(tx, step.run_step.id, Some("found"), "Looked at it.", LANES)
                .expect("done");
            match next {
                Next::Step(def) => assert_eq!(def.step_id, Some(p.second.id)),
                other => panic!("the edge goes on to the second step: {other:?}"),
            }
            let note = outs(tx, step.run_step.id)
                .into_iter()
                .find(|v| v.name == "note")
                .expect("the note");
            assert_eq!(note.exit_name.as_deref(), Some("found"));

            let ended = read::automation_run_step(tx.conn(), step.run_step.id)
                .expect("read")
                .expect("the execution");
            assert_eq!(ended.status, AutomationRunStepStatus::Done);
            assert_eq!(ended.exit_name.as_deref(), Some("found"));
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

            let refused = done(tx, step.run_step.id, Some("found"), "Looked at it.", LANES)
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
                done(tx, step.run_step.id, Some("found"), "   ", LANES)
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

            let next = done(tx, step.run_step.id, None, "", LANES)
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
    fn a_way_out_nobody_declared_is_read_as_the_error_one_and_what_was_said_is_kept() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — the one to work");
            take(tx, step.run_step.id, task.id).expect("take");

            let next = done(tx, step.run_step.id, Some("all good"), "Did the thing.", LANES)
                .expect("done");
            assert!(matches!(next, Next::Halted(_)), "the error way out halts here: {next:?}");
            let ended = read::automation_run_step(tx.conn(), step.run_step.id)
                .expect("read")
                .expect("the execution");
            assert_eq!(ended.exit_name.as_deref(), Some(ERROR_EXIT));
            assert!(ended.report.contains("\"all good\""), "{}", ended.report);
            assert!(ended.report.contains("Did the thing."), "{}", ended.report);
        });
    }

    #[test]
    fn a_step_built_to_report_to_its_task_leaves_the_line_with_its_provenance() {
        with_tx(|tx| {
            let p = picture(tx, false);
            automation::step_update(
                tx,
                p.first.id,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(true),
                None,
            )
            .expect("report to task");
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            let task = a_task(tx, p.project, "SCENARIO SEED — the one to work");
            take(tx, step.run_step.id, task.id).expect("take");
            done(tx, step.run_step.id, Some("found"), "Looked at it.", LANES)
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

    #[test]
    fn a_step_that_has_already_reported_has_nothing_more_to_say() {
        with_tx(|tx| {
            let p = picture(tx, false);
            let run = a_run(tx, &p.automation);
            let step = opened(tx, &run, &p.first);
            done(tx, step.run_step.id, None, "", LANES)
                .expect("done");

            let refused = out(tx, step.run_step.id, "note", Produced::Value("late"))
                .expect_err("it has ended");
            assert!(refused.to_string().contains("only a running one"), "{refused}");
        });
    }
}
