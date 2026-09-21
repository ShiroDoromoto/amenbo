//! Opening one step of a run: the text its terminal is launched with, and the values it is handed.
//!
//! **A step's agent never goes looking.** By the time the terminal is open, everything the step is to
//! work from is already written into the prompt — the preamble, the documents its automation shares,
//! what the run has done so far, and the values wired into it. An agent that had to fetch would need a
//! vocabulary for fetching, and every step would spend its first turns on it.
//!
//! **What is read from the snapshot, and what is read live.** The step's own declarations — its ways
//! out, its inputs, its settings — come from [`crate::model::AutomationRunDef`], the copy taken at
//! launch, so editing an automation cannot change what a run under way is doing. Three things are read
//! live because there is no copy of them: the preamble, the shared documents, and the wires. A wire is
//! the picture rather than the step, and the picture is walked afresh at every move; the other two are
//! the words a person writes for the run and would be worth correcting mid-run rather than frozen.
//!
//! **A value travels along a wire and along nothing else.** A later step is handed what an earlier one
//! put on a way out *that a wire joins to this input* — a name matching by accident is not a
//! connection, and the specification says so rather than letting a rename quietly re-plumb a run.
//!
//! **Where it ends is `Opened`.** A required input with nothing to fill it is not an error to be
//! reported and forgotten: the run is stopped, in the same transaction, so what the store holds
//! afterwards says a person is owed a look. Handing the lane back and waking whatever was queued is the
//! caller's, which is the one place every way a run can end is handled ([`super::automation_run`]).

use crate::error::{Error, Result};
use crate::model::{
    AutomationPortDirection, AutomationPortKind, AutomationRun, AutomationRunDef, AutomationRunStatus,
    AutomationRunStep, AutomationRunStepStatus, AutomationRunTask, AutomationRunValue, RunDefExit,
    RunDefPort, ERROR_EXIT,
};
use crate::ops::automation_stop::Ended;
use crate::ops::emit_create;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// What a step's terminal is opened with.
#[derive(Clone, Debug)]
pub struct Opening {
    /// The execution row this step is being run under — what a report, a value or an attachment hangs
    /// off from here on.
    pub run_step: AutomationRunStep,
    /// The step as it stood at launch — the agent to start, the model, whether it may wait for a
    /// person, and where its working folder is taken from.
    pub run_def: AutomationRunDef,
    /// The whole text handed to the agent, preamble first and the step's own prompt after the material
    /// it is to read.
    pub text: String,
    /// **The folder the terminal is opened in**, or `None` where the step names none and the pane
    /// opens wherever a pane of that project opens.
    ///
    /// It is resolved here rather than carried from where the automation was built, because what
    /// `work_dir_ref` holds is a name and not a path ([`working_folder`]): the answer is given once,
    /// on the setting or the input it names, and every step that reads it reads the same one.
    pub folder: Option<String>,
}

/// The two ways opening a step can end.
#[derive(Clone, Debug)]
pub enum Opened {
    /// Open a terminal on this.
    Ready(Box<Opening>),
    /// A required input had nothing wired into it that has actually been produced, so no terminal was
    /// opened and the run was stopped. `missing` names the inputs, for the sentence a person reads,
    /// and `woke` is the run that took the lane this one gave up, where one was waiting.
    Stopped { run: AutomationRun, missing: Vec<String>, woke: Option<AutomationRun> },
}

/// `<what> '<id>' not found`, the uncoded refusal the automation entities take
/// ([`crate::ops::automation`] says why).
fn not_found(what: &str, id: i64) -> Error {
    Error::not_found(format!("{what} '{id}' not found"))
}

/// **Open one step of a run**: work out what it is handed, write the execution down, and build the text
/// its terminal is launched with.
///
/// The order is deliberate. Everything is resolved before anything is written, so a run that is about
/// to be stopped for a missing input does not first leave a half-opened execution behind it.
///
/// **A step that takes a fresh task opens a new stretch of the run**; every other step joins the one
/// under way. That is what bounds the story a step is told: a run that goes round three tasks tells
/// each step about its own task and not about the two before it.
pub fn open(tx: &WriteTx<'_>, run_id: i64, run_def_id: i64, lanes: i64) -> Result<Opened> {
    let conn = tx.conn();
    let run = read::automation_run(conn, run_id)?.ok_or_else(|| not_found("run", run_id))?;
    if run.status != AutomationRunStatus::Running {
        return Err(Error::invalid(format!(
            "run '{run_id}' is {}, and only a running one opens a step",
            run.status.as_str()
        )));
    }
    let def = read::automation_run_def(conn, run_def_id)?
        .ok_or_else(|| not_found("step of a run", run_def_id))?;
    if def.run_id != run_id {
        return Err(Error::invalid(format!(
            "step '{run_def_id}' belongs to another run — a run opens its own steps"
        )));
    }
    let exits: Vec<RunDefExit> = serde_json::from_str(&def.exits).map_err(Error::from)?;
    let ins: Vec<RunDefPort> = serde_json::from_str(&def.ins).map_err(Error::from)?;

    // The stretch this execution belongs to, decided before anything is written: a step that takes a
    // fresh task starts one, and every other step joins whatever is under way.
    let opens_a_stretch = exits
        .iter()
        .any(|e| e.outs.iter().any(|p| p.kind == AutomationPortKind::TaskTake));
    let current = read::automation_run_task_last(conn, run_id)?;

    // Every input, and what is actually standing ready to fill it.
    let mut handed: Vec<Handed> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for port in &ins {
        match latest_for(tx, &def, port, current.as_ref(), opens_a_stretch)? {
            Some(found) => handed.push(found),
            None if port.required => missing.push(port.name.clone()),
            None => {}
        }
    }
    if !missing.is_empty() {
        let stopped = stop(tx, run, lanes)?;
        return Ok(Opened::Stopped { run: stopped.run, missing, woke: stopped.woke });
    }

    let now = Timestamp::now();
    let stretch = match (opens_a_stretch, &current) {
        (true, _) => Some(new_stretch(tx, run_id, current.as_ref(), now)?),
        (false, Some(open_now)) => Some(open_now.clone()),
        (false, None) => None,
    };
    let run_step = new_execution(tx, &run, &def, stretch.as_ref(), now)?;
    for found in &handed {
        write_in(tx, &run_step, found, now)?;
    }
    let text = compose(tx, &run, &def, &exits, &handed, stretch.as_ref())?;
    let folder = working_folder(&def, &handed)?;
    Ok(Opened::Ready(Box::new(Opening { run_step, run_def: def, text, folder })))
}

/// **Where this step's terminal is opened.** `work_dir_ref` names a setting or an input rather than
/// holding a path, so the answer is given once — while the automation was built, or by whatever step
/// produced the value — and read here at the moment the terminal is started.
///
/// The settings are asked first, and the inputs after. A setting is the answer somebody wrote for this
/// step, while an input is whatever the step before it handed over; where a name is both, what was
/// written for the step is the more deliberate of the two.
///
/// A name that matches neither is `None` rather than a refusal: the pane opens where a pane of that
/// project opens, and a step that could not be run at all is the launch check's to have refused
/// ([`crate::ops::automation_run::check`]).
fn working_folder(def: &AutomationRunDef, handed: &[Handed]) -> Result<Option<String>> {
    let Some(name) = def.work_dir_ref.as_deref() else {
        return Ok(None);
    };
    let cfg: Vec<crate::model::RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    if let Some(answer) = cfg.iter().find(|one| one.name == name) {
        return Ok(answer.value.as_deref().map(written_as_a_path));
    }
    Ok(handed.iter().find(|one| one.port.name == name).and_then(|one| one.from.value.clone()))
}

/// One setting's answer as a path. A setting's value is JSON (`automation_cfg.value`), so a folder is
/// written `"/work/here"` — quotes and all — and a value that is not a JSON string is taken as it
/// stands rather than refused, a path being the one thing this field is ever asked for.
fn written_as_a_path(value: &str) -> String {
    serde_json::from_str::<String>(value).unwrap_or_else(|_| value.to_string())
}

/// One value standing ready for one input: the port it fills, and the row it is copied from.
struct Handed {
    port: RunDefPort,
    from: AutomationRunValue,
}

/// **What is standing ready for one input.** Only what a wire joins to it counts, and only what was
/// produced within the stretch under way — a value from the task before this one is about a task this
/// step is not working on.
///
/// Where several wires feed one input — two ways out that cannot both be taken, or a step visited
/// twice — the newest wins, `seq` being the move number the producing execution was.
///
/// A step that opens a stretch of its own is handed nothing: the stretch it would read from has not
/// begun, and the one before it belongs to another task.
fn latest_for(
    tx: &WriteTx<'_>,
    def: &AutomationRunDef,
    port: &RunDefPort,
    stretch: Option<&AutomationRunTask>,
    opens_a_stretch: bool,
) -> Result<Option<Handed>> {
    let conn = tx.conn();
    let (Some(step_id), Some(stretch), false) = (def.step_id, stretch, opens_a_stretch) else {
        return Ok(None);
    };
    let wires = read::automation_wires_to_port(conn, step_id, &port.name)?;
    if wires.is_empty() {
        return Ok(None);
    }
    let mut best: Option<(i64, AutomationRunValue)> = None;
    for execution in read::automation_run_steps_of_task(conn, stretch.id)? {
        let Some(from_def) = read::automation_run_def(conn, execution.run_def_id)? else {
            continue;
        };
        for value in read::automation_run_values_of(conn, execution.id)? {
            if value.direction != AutomationPortDirection::Out {
                continue;
            }
            let joined = wires.iter().any(|w| {
                Some(w.from_step_id) == from_def.step_id
                    && w.from_exit_name.as_deref() == value.exit_name.as_deref()
                    && w.from_port_name == value.name
            });
            if !joined {
                continue;
            }
            if best.as_ref().is_none_or(|(seq, _)| execution.seq >= *seq) {
                best = Some((execution.seq, value));
            }
        }
    }
    Ok(best.map(|(_, from)| Handed { port: port.clone(), from }))
}

/// Stop a run because a required input had nothing to fill it, through the one cleanup every ending
/// goes through ([`super::automation_stop::ended`]) — so the lane goes back and the task is not left
/// reserved by a run that is over.
///
/// `stopped_reason` is left empty on purpose: the four it offers are a crash, a loop that ran out of
/// turns, an agent that was not there and a person who said stop, and this is none of them. Writing the
/// nearest one would make the record say something that did not happen.
fn stop(tx: &WriteTx<'_>, before: AutomationRun, lanes: i64) -> Result<Ended> {
    super::automation_stop::ended(tx, before, AutomationRunStatus::Stopped, None, lanes)
}

/// Begin the next stretch of a run, and close the one before it.
fn new_stretch(
    tx: &WriteTx<'_>,
    run_id: i64,
    before: Option<&AutomationRunTask>,
    now: Timestamp,
) -> Result<AutomationRunTask> {
    if let Some(before) = before {
        if before.ended_at.is_none() {
            let mut ended = before.clone();
            ended.ended_at = Some(now);
            ended.updated_at = now;
            crate::ops::emit_update(
                tx,
                record::automation_run_task(before),
                record::automation_run_task(&ended),
            )?;
        }
    }
    let stretch = AutomationRunTask {
        id: read::next_id(tx.conn(), "automation_run_task")?,
        run_id,
        seq: before.map_or(1, |b| b.seq + 1),
        task_id: None,
        started_at: Some(now),
        ended_at: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run_task(&stretch))?;
    Ok(stretch)
}

/// Write the execution row. `seq` is the move number within the whole run, so a step the run comes back
/// to is told apart from the first visit by it.
fn new_execution(
    tx: &WriteTx<'_>,
    run: &AutomationRun,
    def: &AutomationRunDef,
    stretch: Option<&AutomationRunTask>,
    now: Timestamp,
) -> Result<AutomationRunStep> {
    let moves = read::automation_run_steps_of(tx.conn(), run.id)?;
    let run_step = AutomationRunStep {
        id: read::next_id(tx.conn(), "automation_run_step")?,
        run_id: run.id,
        run_def_id: def.id,
        run_task_id: stretch.map(|s| s.id),
        seq: moves.iter().map(|s| s.seq).max().unwrap_or(0) + 1,
        exit_name: None,
        report: String::new(),
        status: AutomationRunStepStatus::Running,
        started_at: Some(now),
        ended_at: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run_step(&run_step))?;
    Ok(run_step)
}

/// Write down that a value was handed over — under **this step's** name for it, since that is the name
/// the prompt spells and the one a reader asks the question with.
fn write_in(
    tx: &WriteTx<'_>,
    run_step: &AutomationRunStep,
    handed: &Handed,
    now: Timestamp,
) -> Result<AutomationRunValue> {
    let value = AutomationRunValue {
        id: read::next_id(tx.conn(), "automation_run_value")?,
        run_step_id: run_step.id,
        direction: AutomationPortDirection::In,
        // What way out it left by is the producing side's fact, and it is kept on that row. Here it
        // would answer a question nobody asks of an input.
        exit_name: None,
        name: handed.port.name.clone(),
        kind: handed.port.kind,
        value: handed.from.value.clone(),
        attachment_id: handed.from.attachment_id,
        task_id: handed.from.task_id,
        from_run_step_id: Some(handed.from.run_step_id),
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run_value(&value))?;
    Ok(value)
}

// ───────────────────────── the text ─────────────────────────

/// The whole launch text, in the order the specification sets: what holds for every step, then what
/// holds for this one, then what it is being asked to do, and last how to hand the work back.
///
/// **English, like every other sentence this crate writes.** The words that carry the work — the
/// preamble, the shared documents, the prompt — are the person's own and arrive in whatever language
/// they were written in; what is added around them is the frame.
fn compose(
    tx: &WriteTx<'_>,
    run: &AutomationRun,
    def: &AutomationRunDef,
    exits: &[RunDefExit],
    handed: &[Handed],
    stretch: Option<&AutomationRunTask>,
) -> Result<String> {
    let conn = tx.conn();
    let mut out = String::new();
    if let Some(automation) = read::automation(conn, run.automation_id)? {
        push_block(&mut out, automation.preamble.trim());
    }
    for (name, body) in shared_documents(tx, def)? {
        push_block(&mut out, &format!("## {name}\n\n{}", body.trim()));
    }
    if def.show_history {
        if let Some(story) = story_so_far(tx, stretch)? {
            push_block(&mut out, &story);
        }
    }
    if !handed.is_empty() {
        let lines: Vec<String> = handed.iter().map(|h| format!("- {}", one_value(h))).collect();
        push_block(&mut out, &format!("## What you have been handed\n\n{}", lines.join("\n")));
    }
    push_block(&mut out, &format!("## What to do\n\n{}", def.prompt.as_deref().unwrap_or("").trim()));
    push_block(&mut out, &handing_back(exits));
    Ok(out)
}

/// Add one block, with a blank line between it and whatever came before. An empty one is left out
/// rather than left as a gap: an automation with no preamble should read as one that has nothing to say
/// first, not as one that opens on white space.
fn push_block(out: &mut String, block: &str) {
    if block.trim().is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(block.trim_end());
}

/// The documents this step is handed, in the order they were hung on it. A step whose definition has
/// since been deleted is handed none — the links went with it.
fn shared_documents(tx: &WriteTx<'_>, def: &AutomationRunDef) -> Result<Vec<(String, String)>> {
    let conn = tx.conn();
    let Some(step_id) = def.step_id else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (link_id, _) in read::automation_step_note_siblings(conn, step_id, None)? {
        let Some(link) = read::automation_step_note(conn, link_id)? else {
            continue;
        };
        if let Some(note) = read::automation_note(conn, link.note_id)? {
            out.push((note.name, note.body));
        }
    }
    Ok(out)
}

/// What the run has done on **this task** so far — one line per step that has already reported, in the
/// order they ran.
///
/// The first line of a report and no more: a step is being told what happened, not handed every word of
/// it. The whole report stays on the execution row for whoever reads the run back.
fn story_so_far(tx: &WriteTx<'_>, stretch: Option<&AutomationRunTask>) -> Result<Option<String>> {
    let conn = tx.conn();
    let Some(stretch) = stretch else {
        return Ok(None);
    };
    let mut lines = Vec::new();
    for execution in read::automation_run_steps_of_task(conn, stretch.id)? {
        if execution.status == AutomationRunStepStatus::Running {
            continue;
        }
        let name = read::automation_run_def(conn, execution.run_def_id)?
            .map(|d| d.name)
            .unwrap_or_else(|| "a step".to_string());
        let first = execution.report.lines().next().unwrap_or("").trim().to_string();
        let said = match first.is_empty() {
            true => String::new(),
            false => format!(": {first}"),
        };
        lines.push(format!("{}. {name} — left through {}{said}", execution.seq, named(execution.exit_name.as_deref())));
    }
    if lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!("## What has happened so far\n\n{}", lines.join("\n"))))
}

/// How a way out is spoken of in a sentence: by its name, or as the unnamed one.
fn named(exit: Option<&str>) -> String {
    match exit {
        Some(ERROR_EXIT) => "the error way out".to_string(),
        Some(name) => format!("\"{name}\""),
        None => "the unnamed way out".to_string(),
    }
}

/// One handed value on one line. What is worth printing is the kind's to say: a value is its text, a
/// file and a task are named by what they point at, since the text of neither is in the row.
fn one_value(handed: &Handed) -> String {
    let name = &handed.port.name;
    match handed.port.kind {
        AutomationPortKind::Value => {
            format!("{name}: {}", handed.from.value.as_deref().unwrap_or(""))
        }
        AutomationPortKind::File => match handed.from.attachment_id {
            Some(id) => format!("{name}: the file attached as AMB-ATT-{id}"),
            None => format!("{name}: a file that was not kept"),
        },
        AutomationPortKind::TaskTake | AutomationPortKind::TaskMake => {
            match handed.from.task_id {
                Some(id) => format!("{name}: AMB-T-{id}"),
                None => format!("{name}: a task that is gone"),
            }
        }
    }
}

/// How to hand the work back: the ways out this step may leave through, what each of them is declared
/// to carry, and the one thing every step owes.
///
/// **The error way out is named but not offered.** It is where a step that fell over goes, and a step
/// choosing it on purpose is saying it failed — which is a real answer, and a different one from
/// leaving through a way out somebody drew for the case.
fn handing_back(exits: &[RunDefExit]) -> String {
    let cli = crate::config::Paths::command_name();
    let mut lines = vec![format!("## How to hand your work back\n")];
    lines.push(format!(
        "Say which way out you took, and what you are handing on, with `{cli} automation`. \
         The run reads nothing else you write."
    ));
    lines.push(String::new());
    for exit in exits {
        let outs = match exit.outs.is_empty() {
            true => "nothing to hand on".to_string(),
            false => exit
                .outs
                .iter()
                .map(|p| match p.required {
                    true => format!("`{}` ({}, required)", p.name, p.kind.as_str()),
                    false => format!("`{}` ({})", p.name, p.kind.as_str()),
                })
                .collect::<Vec<_>>()
                .join(", "),
        };
        lines.push(format!("- {} — {outs}", named(exit.name.as_deref())));
    }
    lines.push(String::new());
    lines.push(format!(
        "Put each one down with `{cli} automation out <name>=<value>` (a file with `--file <path>`), \
         then finish with `{cli} automation done --exit \"<way out>\" --report -`. A report is owed \
         whichever way out you take."
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Automation, AutomationOwner, AutomationPortOwner, AutomationStep, ActorKind,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewStep};
    use crate::ops::automation_run::{launch, Launcher};
    use crate::ops::test_support::{mk_project, with_tx};

    /// How many lanes the machine these tests run on has. Three, so that handing one back has
    /// somewhere to put it and nothing here is testing a queue by accident.
    const LANES: i64 = 3;

    /// The picture every test here starts from: a step that takes a task and hands a note on through
    /// "found", and a second step that is wired to read that note. Both ways out of both steps are
    /// decided, so it launches as it stands.
    struct Picture {
        automation: Automation,
        first: AutomationStep,
        second: AutomationStep,
    }

    fn picture(tx: &WriteTx<'_>, required_in: bool, wired: bool) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(
            tx,
            project,
            NewAutomation {
                name: "1件やりきる".into(),
                notes: String::new(),
                preamble: "You are one step of a run.".into(),
            },
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
            NewStep::with_prompt("直す", "fix what the note says", "claude"),
        )
        .expect("second step");

        // The first step takes the task on its unnamed way out, and hands a note on through "found".
        out_on(tx, &first, None, "タスク", AutomationPortKind::TaskTake, true);
        automation::exit_add(tx, AutomationOwner::Step, first.id, Some("found")).expect("way out");
        out_on(tx, &first, Some("found"), "note", AutomationPortKind::Value, false);
        automation::port_add(
            tx,
            AutomationPortOwner::Step,
            second.id,
            AutomationPortDirection::In,
            "note",
            AutomationPortKind::Value,
            required_in,
        )
        .expect("input");
        if wired {
            automation::wire_add(tx, first.id, Some("found"), "note", second.id, "note")
                .expect("wire");
        }

        automation::edge_add(tx, first.id, Some("found"), EdgeTarget::Go(second.id), None)
            .expect("onward");
        automation::edge_add(tx, first.id, None, EdgeTarget::Done, None).expect("closes");
        automation::edge_add(tx, first.id, Some(ERROR_EXIT), EdgeTarget::Halt, None).expect("error");
        automation::edge_add(tx, second.id, None, EdgeTarget::Done, None).expect("second closes");
        automation::edge_add(tx, second.id, Some(ERROR_EXIT), EdgeTarget::Halt, None)
            .expect("second error");
        let automation =
            automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, first, second }
    }

    /// Declare an output on one way out of a step.
    fn out_on(
        tx: &WriteTx<'_>,
        step: &AutomationStep,
        exit_name: Option<&str>,
        name: &str,
        kind: AutomationPortKind,
        required: bool,
    ) {
        let exit = read::automation_exit_by_name(
            tx.conn(),
            AutomationOwner::Step,
            step.id,
            exit_name,
        )
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
            workspace_open: true,
            by: Some(ActorKind::Ai),
        };
        launch(tx, automation.id, &by).expect("launch")
    }

    /// The snapshot taken of one live step at launch.
    fn def_of(tx: &WriteTx<'_>, run: &AutomationRun, step: &AutomationStep) -> AutomationRunDef {
        read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.step_id == Some(step.id))
            .expect("the step's snapshot")
    }

    fn ready(opened: Opened) -> Opening {
        match opened {
            Opened::Ready(opening) => *opening,
            Opened::Stopped { missing, .. } => panic!("stopped for {missing:?}"),
        }
    }

    /// Finish a step execution the way the report will: the way out it left through, the report it
    /// wrote, and one value on that way out.
    fn reported(tx: &WriteTx<'_>, run_step: &AutomationRunStep, exit: &str, report: &str, note: &str) {
        let now = Timestamp::now();
        let mut done = run_step.clone();
        done.exit_name = Some(exit.to_string());
        done.report = report.to_string();
        done.status = AutomationRunStepStatus::Done;
        done.ended_at = Some(now);
        done.updated_at = now;
        crate::ops::emit_update(
            tx,
            record::automation_run_step(run_step),
            record::automation_run_step(&done),
        )
        .expect("finish");
        let value = AutomationRunValue {
            id: read::next_id(tx.conn(), "automation_run_value").expect("id"),
            run_step_id: run_step.id,
            direction: AutomationPortDirection::Out,
            exit_name: Some(exit.to_string()),
            name: "note".to_string(),
            kind: AutomationPortKind::Value,
            value: Some(note.to_string()),
            attachment_id: None,
            task_id: None,
            from_run_step_id: None,
            created_at: now,
            updated_at: now,
        };
        emit_create(tx, record::automation_run_value(&value)).expect("out value");
    }

    #[test]
    fn the_entry_step_opens_a_stretch_and_is_the_first_move() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, LANES).expect("open"));

            assert_eq!(opening.run_step.seq, 1, "the first move of the run");
            assert_eq!(opening.run_step.status, AutomationRunStepStatus::Running);
            let stretch = read::automation_run_task_last(tx.conn(), run.id)
                .expect("read")
                .expect("a stretch was opened");
            assert_eq!(stretch.seq, 1);
            assert_eq!(opening.run_step.run_task_id, Some(stretch.id));
            assert_eq!(stretch.task_id, None, "nothing has handed a task over yet");
        });
    }

    /// Where a step's terminal is opened: the setting `work_dir_ref` names, and the input of that name
    /// where no setting carries it.
    ///
    /// **The value is JSON, and the folder is not.** A setting's answer is written `"/work/here"` —
    /// quotes and all — and a path handed to a terminal with its quotes still on it is a folder
    /// nothing can find.
    #[test]
    fn the_terminal_opens_in_the_folder_the_step_names() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            automation::cfg_add(
                tx,
                AutomationOwner::Step,
                p.first.id,
                "作業フォルダ",
                crate::model::AutomationCfgKind::Folder,
                true,
                None,
            )
            .expect("setting");
            automation::cfg_set(tx, p.first.id, "作業フォルダ", Some("\"/work/here\"")).expect("answer");
            automation::step_update(
                tx,
                p.first.id,
                None,
                None,
                None,
                None,
                None,
                Some(Some("作業フォルダ")),
                None,
                None,
            )
            .expect("point the step at it");
            let run = a_run(tx, &p.automation);
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.first).id).expect("open"));
            assert_eq!(opening.folder.as_deref(), Some("/work/here"));
        });
    }

    /// A step that names no folder opens where a pane of its project opens — which is somebody else's
    /// answer, and this one says nothing about it.
    #[test]
    fn a_step_that_names_no_folder_answers_none() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.first).id).expect("open"));
            assert_eq!(opening.folder, None);
        });
    }

    #[test]
    fn the_text_carries_the_preamble_the_documents_the_prompt_and_the_ways_out() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let note = automation::note_add(tx, p.automation.id, "House style", "Short lines.")
                .expect("document");
            automation::note_link(tx, p.first.id, note.id).expect("link");
            let run = a_run(tx, &p.automation);
            let text = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, LANES).expect("open")).text;

            assert!(text.starts_with("You are one step of a run."), "{text}");
            assert!(text.contains("## House style\n\nShort lines."), "{text}");
            assert!(text.contains("## What to do\n\nlook at it"), "{text}");
            assert!(text.contains("\"found\" — `note` (value)"), "{text}");
            assert!(text.contains("the unnamed way out — `タスク` (task_take, required)"), "{text}");
            assert!(text.contains("the error way out — nothing to hand on"), "{text}");
            assert!(
                !text.contains("What you have been handed"),
                "the first step is handed nothing: {text}"
            );
        });
    }

    #[test]
    fn a_value_travels_along_the_wire_and_is_written_down_under_the_reading_step_s_name() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, LANES).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.\nAnd more below.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, LANES).expect("open"));
            assert_eq!(second.run_step.seq, 2);
            assert_eq!(
                second.run_step.run_task_id, first.run_step.run_task_id,
                "a step that takes no task joins the stretch under way",
            );
            let handed = read::automation_run_values_of(tx.conn(), second.run_step.id)
                .expect("values");
            assert_eq!(handed.len(), 1);
            assert_eq!(handed[0].direction, AutomationPortDirection::In);
            assert_eq!(handed[0].name, "note");
            assert_eq!(handed[0].value.as_deref(), Some("the note"));
            assert_eq!(
                handed[0].from_run_step_id,
                Some(first.run_step.id),
                "the chain back to what produced it",
            );
            assert!(second.text.contains("- note: the note"), "{}", second.text);
        });
    }

    #[test]
    fn a_name_that_matches_without_a_wire_is_not_handed_over() {
        with_tx(|tx| {
            let p = picture(tx, false, false);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, LANES).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, LANES).expect("open"));
            assert!(
                read::automation_run_values_of(tx.conn(), second.run_step.id)
                    .expect("values")
                    .is_empty(),
                "the two ports share a name and nothing joins them",
            );
        });
    }

    #[test]
    fn the_newest_of_several_producers_is_the_one_handed_over() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            let run = a_run(tx, &p.automation);
            let def = def_of(tx, &run, &p.first);
            let once = ready(open(tx, run.id, def.id, LANES).expect("open"));
            reported(tx, &once.run_step, "found", "First time.", "the first note");
            let twice = ready(open(tx, run.id, def.id, LANES).expect("open again"));
            reported(tx, &twice.run_step, "found", "Second time.", "the second note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, LANES).expect("open"));
            let handed =
                read::automation_run_values_of(tx.conn(), second.run_step.id).expect("values");
            assert_eq!(handed[0].value.as_deref(), Some("the second note"));
        });
    }

    #[test]
    fn a_required_input_with_nothing_in_it_stops_the_run_without_opening_anything() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            let run = a_run(tx, &p.automation);
            let before = read::automation_run_steps_of(tx.conn(), run.id).expect("read").len();

            match open(tx, run.id, def_of(tx, &run, &p.second).id, LANES).expect("open") {
                Opened::Ready(_) => panic!("nothing has produced the note"),
                Opened::Stopped { run: stopped, missing, .. } => {
                    assert_eq!(missing, vec!["note".to_string()]);
                    assert_eq!(stopped.status, AutomationRunStatus::Stopped);
                    assert!(stopped.ended_at.is_some());
                    assert_eq!(
                        stopped.stopped_reason, None,
                        "none of the four it offers is what happened",
                    );
                }
            }
            assert_eq!(
                read::automation_run_steps_of(tx.conn(), run.id).expect("read").len(),
                before,
                "no half-opened execution is left behind",
            );
        });
    }

    #[test]
    fn an_input_nobody_needs_is_simply_absent() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, LANES).expect("open"));
            assert!(!opening.text.contains("What you have been handed"), "{}", opening.text);
        });
    }

    #[test]
    fn the_story_is_one_line_a_step_and_stops_at_the_first_line_of_a_report() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, LANES).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.\nAnd more below.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, LANES).expect("open"));
            assert!(
                second.text.contains("1. 調べる — left through \"found\": Found one thing."),
                "{}",
                second.text,
            );
            assert!(!second.text.contains("And more below"), "{}", second.text);
        });
    }

    #[test]
    fn a_step_told_to_go_without_the_story_is_not_given_it() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            automation::step_update(
                tx,
                p.second.id,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(false),
            )
            .expect("history off");
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, LANES).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, LANES).expect("open"));
            assert!(!second.text.contains("What has happened so far"), "{}", second.text);
            assert!(second.text.contains("- note: the note"), "the values still go: {}", second.text);
        });
    }

    #[test]
    fn only_a_running_run_opens_a_step() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let def = def_of(tx, &run, &p.first).id;
            let stopped = stop(tx, run.clone(), LANES).expect("stop");
            let refused = open(tx, stopped.run.id, def, LANES).expect_err("a stopped run opens nothing");
            assert!(refused.to_string().contains("stopped"), "{refused}");
        });
    }
}
