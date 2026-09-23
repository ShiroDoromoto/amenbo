//! Opening one step of a run: the text its terminal is launched with, and the values it is handed.
//!
//! **A step's agent never goes looking.** By the time the terminal is open, everything the step is to
//! work from is already written into the prompt — the preamble, what the run has done so far, and
//! the values wired into it. An agent that had to fetch would need a
//! vocabulary for fetching, and every step would spend its first turns on it.
//!
//! **What is read from the snapshot, and what is not.** The step's own declarations — its ways out,
//! its inputs, its settings — and the wires joined to its inputs come from
//! [`crate::model::AutomationRunDef`], the copy taken at launch, so editing an automation cannot
//! change what a run under way is doing (`AMB-D-961`). The preamble is not: no row holds it, so it is
//! composed from the build ([`crate::agents::preamble`]) at every launch.
//!
//! **A value travels along a wire and along nothing else.** A later step is handed what an earlier one
//! put on a way out *that a wire joins to this input* — a name matching by accident is not a
//! connection, and the specification says so rather than letting a rename quietly re-plumb a run.
//!
//! **Where it ends is `Opened`.** A required input with nothing to fill it is not an error to be
//! reported and forgotten: the run is stopped, in the same transaction, so what the store holds
//! afterwards says a person is owed a look. Handing the task back is the one place every way a run can
//! end is handled ([`super::automation_stop`]).

use crate::error::{Error, Result};
use std::collections::BTreeSet;
use crate::model::{
    AutomationPortDirection, AutomationPortKind, AutomationRun,
    AutomationRunDef, AutomationRunStatus, AutomationRunStep, AutomationRunStepStatus, AutomationRunTask, AutomationRunValue,
    AutomationStoppedReason, RunDefExit, RunDefIn, RunDefPort, ERROR_EXIT,
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

/// The three ways opening a step can end.
#[derive(Clone, Debug)]
pub enum Opened {
    /// Open a terminal on this.
    Ready(Box<Opening>),
    /// A required input had nothing wired into it that has actually been produced, so no terminal was
    /// opened and the run was stopped. `missing` names the inputs, for the sentence a person reads,
    Stopped { run: AutomationRun, missing: Vec<String> },
    /// **The agent this step asks for is not one this machine can start**, so no terminal was opened
    /// and the run was stopped with [`AutomationStoppedReason::NoAgent`]. `agent` is the one that was
    /// asked for, which is not on the run row and is what the sentence needs.
    NoAgent { run: AutomationRun, agent: String },
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
///
/// **`startable` is what this machine can start**, handed in for the reason
/// [`crate::ops::automation_run::Launcher`] hands it in: the store cannot see a person's `PATH`.
/// `None` is nobody asked, and then no step is judged on its agent (`AMB-D-792`).
pub fn open(
    tx: &WriteTx<'_>,
    run_id: i64,
    run_def_id: i64,
    startable: Option<&[String]>,
) -> Result<Opened> {
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
    // **The agent is asked for again here.** The launch check asked it of every step the run can
    // reach (`crate::ops::automation_run::check`), but that was once, and a run is out for as long
    // as its work takes — an agent uninstalled in the middle of one leaves every step after it with
    // nothing to open. A run left `running` on that would hold its task for the rest of the session,
    // so it is ended here with the reason that says which of the five this is.
    if let Some(startable) = startable {
        if !startable.iter().any(|id| id == &def.agent) {
            let stopped = gave_up(tx, run)?;
            return Ok(Opened::NoAgent { run: stopped.run, agent: def.agent });
        }
    }
    let exits: Vec<RunDefExit> = serde_json::from_str(&def.exits).map_err(Error::from)?;
    let ins: Vec<RunDefIn> = serde_json::from_str(&def.ins).map_err(Error::from)?;

    // The stretch this execution belongs to, decided before anything is written: a step that takes a
    // fresh task starts one, and every other step joins whatever is under way.
    let opens_a_stretch = exits
        .iter()
        .any(|e| e.outs.iter().any(|p| p.kind == AutomationPortKind::TaskTake));
    let current = read::automation_run_task_last(conn, run_id)?;

    // Every input, and what is actually standing ready to fill it.
    let mut handed: Vec<Handed> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for input in &ins {
        match latest_for(tx, input, current.as_ref(), opens_a_stretch)? {
            Some(found) => handed.push(found),
            None if input.port.required => missing.push(input.port.name.clone()),
            None => {}
        }
    }
    if !missing.is_empty() {
        let stopped = stop(tx, run)?;
        return Ok(Opened::Stopped { run: stopped.run, missing });
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
    let text = compose(tx, &def, &exits, &handed, stretch.as_ref())?;
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

/// **What is standing ready for one input.** Only what a wire joined to it at launch counts
/// ([`RunDefIn::from`]), and only what was produced within the stretch under way — a value from the
/// task before this one is about a task this step is not working on.
///
/// Where several wires feed one input — two ways out that cannot both be taken, or a step visited
/// twice — the newest wins, `seq` being the move number the producing execution was.
///
/// A step that opens a stretch of its own is handed nothing: the stretch it would read from has not
/// begun, and the one before it belongs to another task.
fn latest_for(
    tx: &WriteTx<'_>,
    input: &RunDefIn,
    stretch: Option<&AutomationRunTask>,
    opens_a_stretch: bool,
) -> Result<Option<Handed>> {
    let conn = tx.conn();
    let (Some(stretch), false) = (stretch, opens_a_stretch) else {
        return Ok(None);
    };
    if input.from.is_empty() {
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
            let joined = input.from.iter().any(|source| {
                Some(source.placement_id) == from_def.placement_id
                    && Some(source.step_id) == from_def.step_id
                    && source.exit_id == value.exit_id
                    && source.port == value.name
            });
            if !joined {
                continue;
            }
            if best.as_ref().is_none_or(|(seq, _)| execution.seq >= *seq) {
                best = Some((execution.seq, value));
            }
        }
    }
    Ok(best.map(|(_, from)| Handed { port: input.port.clone(), from }))
}

/// Fail a run because a required input had nothing to fill it, through the one cleanup every ending
/// goes through ([`super::automation_stop::ended`]) — so the task is not left reserved by a run that
/// is over.
fn stop(tx: &WriteTx<'_>, before: AutomationRun) -> Result<Ended> {
    super::automation_stop::ended(
        tx,
        before,
        super::automation_stop::Ending::Failed(AutomationStoppedReason::NoInput),
    )
}

/// Stop a run because the agent its next step asks for is not one this machine can start, through the
/// same cleanup — the task goes back and the line on it says why.
///
fn gave_up(tx: &WriteTx<'_>, before: AutomationRun) -> Result<Ended> {
    super::automation_stop::ended(
        tx,
        before,
        super::automation_stop::Ending::Failed(AutomationStoppedReason::NoAgent),
    )
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
        exit_id: None,
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
        exit_id: None,
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
/// prompt — is the person's own and arrives in whatever language it was written in; what is added
/// around it, the preamble included, is the frame.
fn compose(
    tx: &WriteTx<'_>,
    def: &AutomationRunDef,
    exits: &[RunDefExit],
    handed: &[Handed],
    stretch: Option<&AutomationRunTask>,
) -> Result<String> {
    let mut out = String::new();
    push_block(&mut out, &crate::agents::preamble(crate::config::Paths::command_name()));
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
/// rather than left as a gap: a step handed no documents and no values should read as one with
/// nothing to say about them, not as one with white space where they would have gone.
fn push_block(out: &mut String, block: &str) {
    if block.trim().is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(block.trim_end());
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
        let def = read::automation_run_def(conn, execution.run_def_id)?;
        let left_by = def.as_ref().and_then(|d| exit_in(d, execution.exit_id));
        let name = def.map(|d| d.name).unwrap_or_else(|| "a step".to_string());
        let first = execution.report.lines().next().unwrap_or("").trim().to_string();
        let said = match first.is_empty() {
            true => String::new(),
            false => format!(": {first}"),
        };
        lines.push(format!("{}. {name} — left through {}{said}", execution.seq, named(left_by.as_deref())));
    }
    if lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!("## What has happened so far\n\n{}", lines.join("\n"))))
}

/// The name of the way out an execution left by, read from the run's copy of the step it ran — the
/// live row may have been renamed or deleted since. `None` is the unnamed one, and a way out the copy
/// does not hold reads as that too.
fn exit_in(def: &AutomationRunDef, exit_id: Option<i64>) -> Option<String> {
    let exits: Vec<RunDefExit> = serde_json::from_str(&def.exits).ok()?;
    exits.into_iter().find(|e| Some(e.id) == exit_id).and_then(|e| e.name)
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
/// to carry, and the one thing every step owes. Each way out is listed with the id `step-done --exit`
/// takes (`AMB-D-961`) — a name is what a person reads, and the unnamed way out has none to type.
///
/// **The error way out is named but not offered.** It is where a step that fell over goes, and a step
/// choosing it on purpose is saying it failed — which is a real answer, and a different one from
/// leaving through a way out somebody drew for the case.
///
/// **The command is named per kind, and only for the kinds this step declares.** There is no one verb
/// that puts every kind down: the task the run is about is reserved and declared in one act by
/// `automation step-take`, because splitting the two leaves a task `in_progress` that nothing can hand back
/// when the agent dies between them ([`crate::ops::automation_report::take`]). A text that said "put
/// each one down with `out`" was therefore wrong on exactly the step every automation has to start
/// with — and an agent does what the text says (`AMB-T-5279`).
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
        lines.push(format!("- `--exit {}` — {} — {outs}", exit.id, named(exit.name.as_deref())));
    }
    let kinds: BTreeSet<AutomationPortKind> =
        exits.iter().flat_map(|e| e.outs.iter().map(|p| p.kind)).collect();
    if !kinds.is_empty() {
        lines.push(String::new());
        lines.push("Put each one down before you finish:".to_string());
        for kind in &kinds {
            lines.push(match kind {
                AutomationPortKind::Value => {
                    format!("- a value — `{cli} automation step-out <name>=<value>`")
                }
                AutomationPortKind::File => {
                    format!("- a file — `{cli} automation step-out <name> --file <path>`")
                }
                // Reserving and declaring are one command, so this one is not `out` and never can be.
                AutomationPortKind::TaskTake => format!(
                    "- the task this step takes — `{cli} automation step-take <task>`, which reserves it \
                     and hands it on in one act. It is refused for a task somebody else already holds."
                ),
                AutomationPortKind::TaskMake => format!(
                    "- a task you raised along the way — `{cli} automation step-out <name>=<task>`. It is \
                     not the task this run is working: nothing reserves it, and whoever comes to it \
                     next picks it up."
                ),
            });
        }
    }
    lines.push(String::new());
    lines.push(format!(
        "Then finish with `{cli} automation step-done --exit <id> --report -`, the id being the way out's \
         from the list above. A report is owed whichever way out you take."
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ActorKind, Automation, AutomationAction, AutomationPictureOwner, AutomationPlacement,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_run::{launch, Launcher};
    use crate::ops::test_support::{mk_exit, mk_in, mk_out, mk_placed, mk_project, with_tx};

    /// The picture every test here starts from: a spot that takes a task and hands a note on through
    /// "found", and a second spot that is wired to read that note. Both ways out of both are decided,
    /// so it launches as it stands.
    struct Picture {
        automation: Automation,
        first_action: AutomationAction,
        first: AutomationPlacement,
        second: AutomationPlacement,
    }

    fn picture(tx: &WriteTx<'_>, required_in: bool, wired: bool) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(
            tx,
            project,
            NewAutomation { name: "1件やりきる".into(), notes: String::new() },
        )
        .expect("add automation");
        let (first_action, first) = mk_placed(tx, &automation, "調べる", "look at it", "claude");
        let (second_action, second) =
            mk_placed(tx, &automation, "直す", "fix what the note says", "claude");

        // The first spot takes the task on its unnamed way out, and hands a note on through "found".
        mk_out(tx, &first_action, None, "タスク", AutomationPortKind::TaskTake, true);
        mk_exit(tx, &first_action, "found");
        mk_out(tx, &first_action, Some("found"), "note", AutomationPortKind::Value, false);
        mk_in(tx, &second_action, "note", AutomationPortKind::Value, required_in);
        if wired {
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
        }

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
        let automation =
            automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, first_action, first, second }
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

    /// The snapshot taken of one live spot at launch.
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

    fn ready(opened: Opened) -> Opening {
        match opened {
            Opened::Ready(opening) => *opening,
            Opened::Stopped { missing, .. } => panic!("stopped for {missing:?}"),
            Opened::NoAgent { agent, .. } => panic!("cannot start {agent}"),
        }
    }

    /// Finish a step execution the way the report will: the way out it left through, the report it
    /// wrote, and one value on that way out.
    fn reported(tx: &WriteTx<'_>, run_step: &AutomationRunStep, exit: &str, report: &str, note: &str) {
        let now = Timestamp::now();
        let mut done = run_step.clone();
        done.exit_id = crate::ops::test_support::way_out(tx, run_step.id, exit);
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
            exit_id: done.exit_id,
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
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));

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
                p.first_action.id,
                "作業フォルダ",
                crate::model::AutomationCfgKind::Folder,
                true,
                None,
            )
            .expect("setting");
            automation::cfg_set(tx, p.first.id, "作業フォルダ", Some("\"/work/here\"")).expect("answer");
            automation::step_update(
                tx,
                p.first_action.entry_step_id.expect("an entry step"),
                None,
                None,
                None,
                Some(Some("作業フォルダ")),
                None,
                None,
            )
            .expect("point the step at it");
            let run = a_run(tx, &p.automation);
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
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
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            assert_eq!(opening.folder, None);
        });
    }

    #[test]
    fn the_text_carries_the_preamble_the_prompt_and_the_ways_out() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let text = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open")).text;

            assert!(text.starts_with("You are one step of an automation run"), "{text}");
            assert!(text.contains("## What to do\n\nlook at it"), "{text}");
            assert!(text.contains("\"found\" — `note` (value)"), "{text}");
            assert!(text.contains("the unnamed way out — `タスク` (task_take, required)"), "{text}");
            assert!(text.contains("the error way out — nothing to hand on"), "{text}");
            assert!(
                !text.contains("## What you have been handed"),
                "the first step is handed nothing: {text}"
            );
        });
    }

    /// **The preamble is Amenbo's, not the automation's** (`AMB-D-952`). No row carries it, so every
    /// run of every automation opens on the same sentences — and since nobody types them any more,
    /// they may name the commands a step reads its run back with (`AMB-T-5325`).
    #[test]
    fn every_step_opens_on_the_standing_sentences_and_is_told_how_to_read_what_came_before() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let text = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open")).text;

            assert!(text.starts_with(&crate::agents::preamble("amenbo")), "{text}");
            assert!(text.contains("`amenbo automation run-show <run>`"), "{text}");
            assert!(text.contains("`amenbo attach show`"), "{text}");
        });
    }

    /// **The text names the command each kind is actually handed on with** (`AMB-T-5279`).
    ///
    /// A step's agent does what the text says. It said "put each one down with `automation step-out`" for
    /// every kind, and the one kind every automation has to start with — the task the run is about —
    /// cannot be put down that way at all: reserving and declaring it are one act, so a step following
    /// the text was refused every time.
    #[test]
    fn the_text_names_a_command_per_kind_and_not_out_for_the_task_it_takes() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            mk_out(tx, &p.first_action, Some("found"), "raised", AutomationPortKind::TaskMake, false);
            let run = a_run(tx, &p.automation);
            let text = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open")).text;

            assert!(text.contains("- a value — `amenbo automation step-out <name>=<value>`"), "{text}");
            assert!(
                text.contains("the task this step takes — `amenbo automation step-take <task>`"),
                "{text}"
            );
            assert!(
                text.contains("a task you raised along the way — `amenbo automation step-out <name>=<task>`"),
                "{text}"
            );
            // The line that was wrong: one command for every kind.
            assert!(!text.contains("Put each one down with `amenbo automation step-out"), "{text}");
            // A kind this step declares nothing of is not explained at it.
            assert!(!text.contains("a file — `amenbo automation step-out"), "{text}");
        });
    }

    #[test]
    fn a_value_travels_along_the_wire_and_is_written_down_under_the_reading_step_s_name() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.\nAnd more below.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
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
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
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
            let once = ready(open(tx, run.id, def.id, None).expect("open"));
            reported(tx, &once.run_step, "found", "First time.", "the first note");
            let twice = ready(open(tx, run.id, def.id, None).expect("open again"));
            reported(tx, &twice.run_step, "found", "Second time.", "the second note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
            let handed =
                read::automation_run_values_of(tx.conn(), second.run_step.id).expect("values");
            assert_eq!(handed[0].value.as_deref(), Some("the second note"));
        });
    }

    /// **A value travels along the wires the run launched with** (`AMB-D-961`). The copy holds them, so a
    /// picture that has since lost its wire — written straight to the table here, since a definition a
    /// run is using refuses the edit — still hands the note on.
    #[test]
    fn a_value_travels_along_the_wire_the_run_launched_with_after_the_picture_loses_it() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            let run = a_run(tx, &p.automation);
            tx.conn().execute("DELETE FROM automation_wire", []).expect("the picture moves on");

            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");
            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
            let handed =
                read::automation_run_values_of(tx.conn(), second.run_step.id).expect("values");
            assert_eq!(handed.len(), 1);
            assert_eq!(handed[0].value.as_deref(), Some("the note"));
        });
    }

    #[test]
    fn a_required_input_with_nothing_in_it_stops_the_run_without_opening_anything() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            let run = a_run(tx, &p.automation);
            let before = read::automation_run_steps_of(tx.conn(), run.id).expect("read").len();

            match open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open") {
                Opened::Ready(_) => panic!("nothing has produced the note"),
                Opened::Stopped { run: stopped, missing, .. } => {
                    assert_eq!(missing, vec!["note".to_string()]);
                    assert_eq!(stopped.status, AutomationRunStatus::Failed);
                    assert!(stopped.ended_at.is_some());
                    assert_eq!(
                        stopped.stopped_reason,
                        Some(AutomationStoppedReason::NoInput),
                        "the record says what happened",
                    );
                }
                Opened::NoAgent { agent, .. } => panic!("cannot start {agent}"),
            }
            assert_eq!(
                read::automation_run_steps_of(tx.conn(), run.id).expect("read").len(),
                before,
                "no half-opened execution is left behind",
            );
        });
    }

    /// **The agent went away while the run was out**, which the launch check cannot have caught: it
    /// asked once, at the press, and this run has been standing since. What this holds is that the
    /// run is ended rather than left `running` on a step nothing can open — with the reason that says
    /// which ending it was, so the row on the running tab reads apart from a stop by hand.
    #[test]
    fn a_step_whose_agent_this_machine_cannot_start_any_more_stops_the_run() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let before = read::automation_run_steps_of(tx.conn(), run.id).expect("read").len();

            // Everything this machine can start, and "claude" — what both steps ask for — is not in it.
            let here = ["codex".to_string()];
            match open(tx, run.id, def_of(tx, &run, &p.first).id, Some(&here)).expect("open") {
                Opened::Ready(_) => panic!("nothing here can start claude"),
                Opened::Stopped { missing, .. } => panic!("stopped for {missing:?}"),
                Opened::NoAgent { run: stopped, agent } => {
                    assert_eq!(agent, "claude");
                    assert_eq!(stopped.status, AutomationRunStatus::Failed);
                    assert!(stopped.ended_at.is_some());
                    assert_eq!(
                        stopped.stopped_reason,
                        Some(AutomationStoppedReason::NoAgent),
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

    /// **Nobody asked, so no step is judged on its agent** (`AMB-D-792`). A machine that has never
    /// probed has no list, and reading its silence as "nothing is installed" would stop every run on
    /// it.
    #[test]
    fn a_machine_that_was_never_asked_judges_no_step_on_its_agent() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);

            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            assert_eq!(opening.run_def.agent, "claude");
        });
    }

    #[test]
    fn an_input_nobody_needs_is_simply_absent() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
            assert!(!opening.text.contains("## What you have been handed"), "{}", opening.text);
        });
    }

    #[test]
    fn the_story_is_one_line_a_step_and_stops_at_the_first_line_of_a_report() {
        with_tx(|tx| {
            let p = picture(tx, true, true);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.\nAnd more below.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
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
            automation::step_update(tx, p.second.id, None, None, None, None, None, Some(false))
            .expect("history off");
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
            assert!(!second.text.contains("## What has happened so far"), "{}", second.text);
            assert!(second.text.contains("- note: the note"), "the values still go: {}", second.text);
        });
    }

    #[test]
    fn only_a_running_run_opens_a_step() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let def = def_of(tx, &run, &p.first).id;
            let stopped = stop(tx, run.clone()).expect("stop");
            let refused = open(tx, stopped.run.id, def, None).expect_err("a failed run opens nothing");
            assert!(refused.to_string().contains("failed"), "{refused}");
        });
    }
}
