//! Opening one step of a run: the text its terminal is launched with, and the values it is handed.
//!
//! **A step's agent never goes looking.** By the time the terminal is open, everything the step is to
//! work from is already written into the prompt — the preamble, the task the run is on, what the run
//! has done so far, the values wired into it, and the settings answered where it was placed. An agent that had to fetch would need a
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
    AutomationCfgKind, AutomationPortDirection, AutomationPortKind, AutomationRun,
    AutomationRunDef, AutomationRunStatus, AutomationRunStep, AutomationRunStepStatus, AutomationRunTask, AutomationRunValue,
    AutomationStoppedReason, RunDefCfg, RunDefExit, RunDefIn, RunDefPort, ERROR_EXIT,
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

/// The six ways opening a step can end.
#[derive(Clone, Debug)]
pub enum Opened {
    /// Open a terminal on this.
    Ready(Box<Opening>),
    /// **The step was a built-in, and it has been carried out** (`AMB-D-964`). No terminal is opened:
    /// the step has already reported, and `next` is what its way out leads to — read and acted on
    /// exactly as a report from an agent's step would be ([`super::automation_report::Next`]).
    Carried { run_step_id: i64, next: super::automation_report::Next },
    /// A required input had nothing wired into it that has actually been produced, so no terminal was
    /// opened and the run was stopped. `missing` names the inputs, for the sentence a person reads,
    Stopped { run: AutomationRun, missing: Vec<String> },
    /// **The agent this step asks for is not one this machine can start**, so no terminal was opened
    /// and the run was stopped with [`AutomationStoppedReason::NoAgent`]. `agent` is the one that was
    /// asked for, which is not on the run row and is what the sentence needs.
    NoAgent { run: AutomationRun, agent: String },
    /// **This step takes a fresh task while the one before is still in progress**, so no terminal was
    /// opened and the run was failed with [`AutomationStoppedReason::LeftTaskOpen`] (`AMB-D-967`).
    LeftTaskOpen { run: AutomationRun },
    /// **The step is a built-in set to wait, and nothing has turned up for it yet** (`AMB-D-969`).
    /// Nothing was written, so the run still stands before this step and the next look opens it again.
    /// Where a pause had been asked for, `run` has been paused here instead: a waiting step never
    /// reports, so this is where the pause takes hold.
    Waiting { run: AutomationRun },
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
///
/// **`outside` is a built-in's work that drives git, already done** before this transaction
/// ([`super::automation_builtin::work_outside`]), so the fetch does not hold every other writer to
/// the store while it waits on the remote. Here it is only written down.
pub fn open(
    tx: &WriteTx<'_>,
    run_id: i64,
    run_def_id: i64,
    startable: Option<&[String]>,
    outside: Option<super::automation_builtin::DoneOutside>,
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
    // A built-in names no agent: Amenbo carries it out itself.
    if let (Some(startable), None) = (startable, def.builtin.as_ref()) {
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
    // **The task before is closed before the next is taken** (`AMB-D-967`). The launch check refuses
    // a picture with a line that skips closing it, so this is a safety net for a line the check
    // missed: the run fails rather than leave that task reserved by nobody.
    if opens_a_stretch && super::automation_stop::left_open(tx, current.as_ref())? {
        let stopped = super::automation_stop::ended(
            tx,
            run,
            super::automation_stop::Ending::Failed(AutomationStoppedReason::LeftTaskOpen),
        )?;
        return Ok(Opened::LeftTaskOpen { run: stopped.run });
    }

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
    // Asked before anything is written, so a built-in that is waiting leaves no execution behind it
    // on each look (`super::automation_builtin::Waits`).
    if super::automation_builtin::waiting(conn, &run, &def)? {
        let run = match run.pause_requested {
            true => super::automation_stop::settle(tx, run)?.run,
            false => run,
        };
        return Ok(Opened::Waiting { run });
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
    if def.builtin.is_some() {
        let ins: Vec<(String, Option<String>)> =
            handed.iter().map(|h| (h.port.name.clone(), h.from.value.clone())).collect();
        let task_id = stretch.as_ref().and_then(|s| s.task_id);
        let next =
            super::automation_builtin::carry_out(tx, &run, &run_step, &def, &exits, &ins, task_id, outside)?;
        return Ok(Opened::Carried { run_step_id: run_step.id, next });
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
    let cfg: Vec<RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    if let Some(answer) = cfg.iter().find(|one| one.name == name) {
        return Ok(answer.value.as_deref().map(unquoted));
    }
    Ok(handed.iter().find(|one| one.port.name == name).and_then(|one| one.from.value.clone()))
}

/// One setting's answer as the text it says. A setting's value is JSON (`automation_cfg.value`), so a
/// folder is written `"/work/here"` — quotes and all — and a value that is not a JSON string (a number,
/// or one written by hand) is taken as it stands rather than refused.
fn unquoted(value: &str) -> String {
    serde_json::from_str::<String>(value).unwrap_or_else(|_| value.to_string())
}

/// **A task filter's answer as the expression `task list --filter` takes.**
///
/// The answer is kept as one list per part — `{"status":["todo"],"ready":["yes"]}` — because that is the
/// shape the build screen presses chips into. The expression is the reading filterGrammar gives it: the
/// values of one part comma-joined (any-of), and the parts space-joined (both). A part written as one
/// string rather than a list is read as a list of one.
///
/// The order the tasks are taken in is kept beside the parts, under [`TASKFILTER_SORT_KEY`], and is not
/// one of them: it says which comes first, not which are in ([`taskfilter_sort`]).
///
/// `None` is an answer that is not an object of parts, or one with no part in it. Nothing is checked
/// against the grammar here: `automation cfg-set` parses the expression before it writes the answer.
pub fn taskfilter_expr(value: &str) -> Option<String> {
    let serde_json::Value::Object(parts) = serde_json::from_str(value).ok()? else {
        return None;
    };
    let mut out = Vec::new();
    for (key, part) in parts {
        if key == TASKFILTER_SORT_KEY {
            continue;
        }
        let values: Vec<String> = match part {
            serde_json::Value::String(one) => vec![one],
            serde_json::Value::Array(many) => many
                .into_iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        };
        if !values.is_empty() {
            out.push(format!("{key}:{}", values.join(",")));
        }
    }
    (!out.is_empty()).then(|| out.join(" "))
}

/// The key a task filter's answer keeps its order under — the value `task list --sort` takes.
pub const TASKFILTER_SORT_KEY: &str = "sort";

/// The order a task filter is taken in when its answer names none: the highest priority first, which is
/// what "the top one" means to a person reading a queue.
pub const TASKFILTER_SORT_DEFAULT: &str = "priority";

/// **The order a task filter's answer takes its tasks in**, as `task list --sort` spells it. An answer
/// that names none — or is not an object at all — is taken highest priority first
/// ([`TASKFILTER_SORT_DEFAULT`]), so a step always runs a list whose top is the same one every time.
pub fn taskfilter_sort(value: &str) -> String {
    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|v| v.get(TASKFILTER_SORT_KEY)?.as_str().map(str::to_string))
        .filter(|sort| !sort.is_empty())
        .unwrap_or_else(|| TASKFILTER_SORT_DEFAULT.to_string())
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
                    && source.port_id == value.port_id
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
        report_withheld: false,
        status: AutomationRunStepStatus::Running,
        started_at: Some(now),
        ended_at: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run_step(&run_step))?;
    Ok(run_step)
}

/// Write down that a value was handed over — under **this step's** input it filled, since that is the
/// port the prompt spells and the one a reader asks the question with.
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
        port_id: handed.port.id,
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

/// The whole launch text, in the order the specification sets: what holds for every step — ending on
/// the task the run is working, once one has been taken ([`working_on`]) — then what holds for this one, then what it is being asked to do, and last how to hand the work back.
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
    let mut preamble = crate::agents::preamble(crate::config::Paths::command_name());
    if let Some(task) = stretch.and_then(|s| s.task_id) {
        preamble.push_str(&format!("\n\n{}", working_on(task)));
    }
    push_block(&mut out, &preamble);
    let parts = TaskParts { notes: def.show_notes, decisions: def.show_decisions, comments: def.show_comments };
    if parts.any() {
        if let Some(task) = stretch.and_then(|s| s.task_id) {
            push_block(&mut out, &the_task(tx, task, parts)?);
        }
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
    let cfg: Vec<RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    if !cfg.is_empty() {
        let lines: Vec<String> = cfg.iter().map(|c| format!("- {}", one_setting(c))).collect();
        push_block(&mut out, &format!("## Your settings\n\n{}", lines.join("\n")));
    }
    push_block(&mut out, &format!("## What to do\n\n{}", def.prompt.as_deref().unwrap_or("").trim()));
    push_block(&mut out, &handing_back(exits));
    Ok(out)
}

/// **The task this run is on**, the line the preamble ends with (`AMB-T-5413`).
///
/// It is written at every launch rather than carried along a wire, so a step knows its task wherever it
/// is placed and nobody has to join anything for it. The step that takes the task is not told one:
/// it opens a stretch of its own, and the stretch has no task until that step takes it.
fn working_on(task: i64) -> String {
    format!("The task this run is working on now is AMB-T-{task}.")
}

/// Which parts of the task the run is on a step is handed — each on a switch of its own, all three on
/// unless somebody turns one off.
#[derive(Clone, Copy)]
struct TaskParts {
    notes: bool,
    decisions: bool,
    comments: bool,
}

impl TaskParts {
    fn any(self) -> bool {
        self.notes || self.decisions || self.comments
    }
}

/// **The task this run is on, as the store holds it when the step opens** (`AMB-D-965`): its title, then
/// those of its notes, the decisions linked to it and its comments the step is handed, each whole and
/// oldest first.
///
/// It is written here so the step does not spend its first turns fetching it with `task show`,
/// `decision show` and `comment list` — and cannot forget to. What a task that this one depends on
/// left in the code is not here: reading that stays the agent's work.
fn the_task(tx: &WriteTx<'_>, task_id: i64, parts: TaskParts) -> Result<String> {
    let conn = tx.conn();
    let task = read::task(conn, task_id)?.ok_or_else(|| not_found("task", task_id))?;
    let mut out = format!("## The task this run is on\n\n**AMB-T-{}** — {}", task.id, task.title.trim());
    if parts.notes && !task.notes.trim().is_empty() {
        out.push_str(&format!("\n\n### Its notes\n\n{}", task.notes.trim()));
    }
    let mut decisions = Vec::new();
    for linked in if parts.decisions { read::decisions_for_task(conn, task_id)? } else { Vec::new() } {
        let body = read::decision(conn, linked.id)?.map(|d| d.body).unwrap_or_default();
        decisions.push(format!(
            "**AMB-D-{}** — {} ({})\n\n{}",
            linked.id,
            linked.title.trim(),
            linked.status,
            body.trim()
        ));
    }
    if !decisions.is_empty() {
        out.push_str(&format!("\n\n### The decisions linked to it\n\n{}", decisions.join("\n\n")));
    }
    let mut comments = Vec::new();
    for id in if parts.comments { read::task_comment_ids(conn, task_id)? } else { Vec::new() } {
        let Some(comment) = read::task_comment(conn, id)? else {
            continue;
        };
        let who = match comment.author_kind {
            Some(crate::model::ActorKind::Ai) => "AI",
            _ => "human",
        };
        let when = comment.posted_at.unwrap_or(comment.created_at).to_rfc3339_z();
        comments.push(format!("**AMB-TC-{}** — {who}, {when}\n\n{}", comment.id, comment.text.trim()));
    }
    if !comments.is_empty() {
        out.push_str(&format!("\n\n### Its comments, oldest first\n\n{}", comments.join("\n\n")));
    }
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
        lines.push(format!("{}. {name} — left through {}{said}", execution.seq, left_by.as_deref().map_or_else(|| "no way out".to_string(), named)));
    }
    if lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!("## What has happened so far\n\n{}", lines.join("\n"))))
}

/// The name of the way out an execution left by, read from the run's copy of the step it ran — the
/// live row may have been renamed or deleted since. `None` where it left by none, or by one the copy
/// does not hold.
fn exit_in(def: &AutomationRunDef, exit_id: Option<i64>) -> Option<String> {
    let exits: Vec<RunDefExit> = serde_json::from_str(&def.exits).ok()?;
    exits.into_iter().find(|e| Some(e.id) == exit_id).map(|e| e.name)
}

/// How a way out is spoken of in a sentence: by its name, and the error one as what it is.
fn named(exit: &str) -> String {
    match exit {
        ERROR_EXIT => "the error way out".to_string(),
        name => format!("\"{name}\""),
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

/// One setting on one line: its name, and the answer written for the spot this step was placed at.
///
/// **A task filter is written as the command that lists what it means.** The agent reads the queue with
/// `task list` anyway, and an expression it can pass along as it stands is one it cannot mistranscribe.
/// The order is always spelled, the default included: a list without one comes back in the board's own
/// order, and "the top one" would then be whichever task somebody last dragged up.
/// It is joined with `=`, because a descending order starts with `-` and would otherwise be read as an
/// option of its own.
/// Every other kind is the text of its answer. A setting nobody answered says so, rather than being left
/// out: a prompt that names it would otherwise be pointing at nothing.
fn one_setting(cfg: &RunDefCfg) -> String {
    let name = &cfg.name;
    let Some(value) = cfg.value.as_deref() else {
        return format!("{name}: not answered");
    };
    match (cfg.kind, taskfilter_expr(value)) {
        (AutomationCfgKind::TaskFilter, Some(expr)) => {
            let cli = crate::config::Paths::command_name();
            let sort = taskfilter_sort(value);
            format!("{name}: the tasks `{cli} task list --filter \"{expr}\" --sort={sort}` lists, in that order")
        }
        _ => format!("{name}: {}", unquoted(value)),
    }
}

/// How to hand the work back: the ways out this step may leave through, what each of them is declared
/// to carry, and the one thing every step owes. Each way out is listed with the id `step-done --exit`
/// takes, and each output with the id `step-out` takes (`AMB-D-961`) — a name is what a person reads,
/// and the id is what cannot be mistyped. An output is one way out's: two ways out may each declare
/// one of the same name, and the id is what says which of them a value is put down on.
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
                    true => format!("`{}` {} ({}, required)", p.id, p.name, p.kind.as_str()),
                    false => format!("`{}` {} ({})", p.id, p.name, p.kind.as_str()),
                })
                .collect::<Vec<_>>()
                .join(", "),
        };
        lines.push(format!("- `--exit {}` — {} — {outs}", exit.id, named(&exit.name)));
    }
    let kinds: BTreeSet<AutomationPortKind> =
        exits.iter().flat_map(|e| e.outs.iter().map(|p| p.kind)).collect();
    if !kinds.is_empty() {
        lines.push(String::new());
        lines.push(
            "Put down the ones listed under the way out you take, by the id in front of each, before \
             you finish:"
                .to_string(),
        );
        for kind in &kinds {
            lines.push(match kind {
                AutomationPortKind::Value => {
                    format!("- a value — `{cli} automation step-out <id>=<value>`")
                }
                AutomationPortKind::File => {
                    format!("- a file — `{cli} automation step-out <id> --file <path>`")
                }
                // Reserving and declaring are one command, so this one is not `out` and never can be.
                AutomationPortKind::TaskTake => format!(
                    "- the task this step takes — `{cli} automation step-take <task>`, which reserves it \
                     and hands it on in one act. It is refused for a task somebody else already holds."
                ),
                AutomationPortKind::TaskMake => format!(
                    "- a task you raised along the way — `{cli} automation step-out <id>=<task>`. It is \
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
    lines.push(String::new());
    lines.push(format!(
        "If you close the task with `{cli} task done --report`, that report is what the task carries. \
         The one you give `step-done` stays on the run's history."
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::open;
    use crate::model::{
        ActorKind, Automation, AutomationAction, AutomationPictureOwner, AutomationPlacement,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_run::{launch_leaving_the_task_open as launch, Launcher};
    use crate::ops::test_support::{mk_exit, mk_in, mk_out, mk_placed, mk_project, with_tx};

    /// The picture every test here starts from: a spot that takes a task and hands a note on through
    /// "found", and a second spot that is wired to read that note. Both ways out of both are decided,
    /// so it launches as it stands.
    struct Picture {
        automation: Automation,
        first_action: AutomationAction,
        first: AutomationPlacement,
        second_action: AutomationAction,
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

        // The first spot takes the task on its done way out, and hands a note on through "found".
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
        Picture { automation, first_action, first, second_action, second }
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
            Opened::Carried { .. } | Opened::Waiting { .. } => panic!("not a built-in"),
            Opened::LeftTaskOpen { .. } => panic!("left a task open"),
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
            port_id: crate::ops::test_support::out_port(tx, run_step.id, done.exit_id, "note"),
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
                None,
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

    /// **A step is told the task its run is on, at the end of the preamble** (`AMB-T-5413`) — without a
    /// wire, so it knows its task wherever it is placed. The step that takes it is told none: its
    /// stretch has no task until it takes one.
    #[test]
    fn the_preamble_ends_on_the_task_once_one_is_taken() {
        with_tx(|tx| {
            let p = picture(tx, false, false);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            assert!(!first.text.contains("working on now"), "{}", first.text);

            let task = crate::ops::test_support::mk_task(tx, "直すもの");
            crate::ops::automation_report::take(tx, first.run_step.id, task).expect("take");
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
            let preamble = crate::agents::preamble("amenbo");
            let expected = format!("{preamble}\n\nThe task this run is working on now is AMB-T-{task}.\n\n");
            assert!(second.text.starts_with(&expected), "{}", second.text);
        });
    }

    #[test]
    fn the_text_carries_the_preamble_the_prompt_and_the_ways_out() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let opening = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            let text = opening.text;

            assert!(text.starts_with("You are one step of an automation run"), "{text}");
            assert!(text.contains("## What to do\n\nlook at it"), "{text}");
            let note = crate::ops::test_support::out_port(tx, opening.run_step.id, None, "note");
            let task = crate::ops::test_support::out_port(tx, opening.run_step.id, None, "タスク");
            assert!(text.contains(&format!("\"found\" — `{note}` note (value)")), "{text}");
            assert!(
                text.contains(&format!("\"完了\" — `{task}` タスク (task_take, required)")),
                "{text}"
            );
            assert!(text.contains("the error way out — nothing to hand on"), "{text}");
            assert!(
                !text.contains("## What you have been handed"),
                "the first step is handed nothing: {text}"
            );
        });
    }

    /// **What was answered where a step was placed reaches its agent** (`AMB-T-5410`). Before, only the
    /// setting `work_dir_ref` names was read, and a queue somebody chose on the build screen had to be
    /// written into the prompt by hand — so the same action placed twice could not take two queues.
    #[test]
    fn the_text_carries_the_settings_answered_where_the_step_was_placed() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let kind = crate::model::AutomationCfgKind::TaskFilter;
            automation::cfg_add(tx, p.first_action.id, "受信箱", kind, true, None).expect("filter");
            let kind = crate::model::AutomationCfgKind::Text;
            automation::cfg_add(tx, p.first_action.id, "観点", kind, true, None).expect("text");
            let kind = crate::model::AutomationCfgKind::Number;
            automation::cfg_add(tx, p.first_action.id, "上限", kind, false, None).expect("number");
            let filter = r#"{"assignee":["me-ai"],"status":["todo","blocked"]}"#;
            automation::cfg_set(tx, p.first.id, "受信箱", Some(filter)).expect("answer the filter");
            automation::cfg_set(tx, p.first.id, "観点", Some("\"速さ\"")).expect("answer the text");
            let run = a_run(tx, &p.automation);
            let text = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open")).text;

            let cli = crate::config::Paths::command_name();
            assert!(text.contains("## Your settings"), "{text}");
            assert!(
                text.contains(&format!(
                    "- 受信箱: the tasks `{cli} task list --filter \"assignee:me-ai status:todo,blocked\" --sort=priority` lists, in that order"
                )),
                "{text}"
            );
            assert!(text.contains("- 観点: 速さ"), "the quotes are JSON's, not the answer's: {text}");
            assert!(text.contains("- 上限: not answered"), "{text}");
            let settings = text.find("## Your settings").expect("settings");
            let todo = text.find("## What to do").expect("what to do");
            assert!(settings < todo, "the settings come before the prompt that reads them: {text}");
        });
    }

    #[test]
    fn a_step_placed_with_no_settings_is_told_of_none() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let run = a_run(tx, &p.automation);
            let text = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open")).text;
            assert!(!text.contains("## Your settings"), "{text}");
        });
    }

    /// The expression is what `task list --filter` reads: one part's values any-of, the parts both —
    /// and a part written as one string is a list of one.
    #[test]
    fn a_task_filter_answer_reads_as_the_filter_expression() {
        assert_eq!(
            taskfilter_expr(r#"{"status":["todo","blocked"],"ready":["yes"]}"#).as_deref(),
            Some("ready:yes status:todo,blocked")
        );
        assert_eq!(taskfilter_expr(r#"{"status":"todo"}"#).as_deref(), Some("status:todo"));
        assert_eq!(taskfilter_expr(r#"{"status":[]}"#), None, "no part left");
        assert_eq!(taskfilter_expr(r#""todo""#), None, "not an object of parts");
        assert_eq!(taskfilter_expr("status:todo"), None, "not JSON");
    }

    /// **The order rides beside the parts, not among them** (`AMB-T-5411`): it says which task comes
    /// first, and a `sort:` in the filter expression would be refused as a key `--filter` does not have.
    /// An answer that names no order is taken highest priority first.
    #[test]
    fn a_task_filter_answer_keeps_its_order_apart_from_its_parts() {
        let answer = r#"{"status":["todo"],"sort":"-due"}"#;
        assert_eq!(taskfilter_expr(answer).as_deref(), Some("status:todo"));
        assert_eq!(taskfilter_sort(answer), "-due");
        assert_eq!(taskfilter_sort(r#"{"status":["todo"]}"#), "priority", "none named");
        assert_eq!(taskfilter_sort(r#"{"status":["todo"],"sort":""}"#), "priority", "an empty order is none");
        assert_eq!(taskfilter_expr(r#"{"sort":"due"}"#), None, "an order alone narrows nothing");
    }

    /// The order an answer names is the one the step is handed, spelled after the filter.
    #[test]
    fn the_settings_line_spells_the_order_the_answer_names() {
        let cfg = RunDefCfg {
            name: "受信箱".to_string(),
            kind: crate::model::AutomationCfgKind::TaskFilter,
            required: true,
            options: None,
            value: Some(r#"{"status":["todo"],"sort":"-created"}"#.to_string()),
        };
        let cli = crate::config::Paths::command_name();
        assert_eq!(
            one_setting(&cfg),
            format!("受信箱: the tasks `{cli} task list --filter \"status:todo\" --sort=-created` lists, in that order")
        );
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

            assert!(text.contains("- a value — `amenbo automation step-out <id>=<value>`"), "{text}");
            assert!(
                text.contains("the task this step takes — `amenbo automation step-take <task>`"),
                "{text}"
            );
            assert!(
                text.contains("a task you raised along the way — `amenbo automation step-out <id>=<task>`"),
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
            assert_eq!(handed[0].port_id, crate::ops::test_support::in_port(tx, &second.run_step, "note"));
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
                Opened::Carried { .. } | Opened::Waiting { .. } => panic!("not a built-in"),
                Opened::LeftTaskOpen { .. } => panic!("left a task open"),
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
                Opened::Carried { .. } | Opened::Waiting { .. } => panic!("not a built-in"),
                Opened::LeftTaskOpen { .. } => panic!("left a task open"),
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
            automation::step_update(tx, p.second.id, None, None, None, None, None, Some(false), None, None, None)
            .expect("history off");
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
            assert!(!second.text.contains("## What has happened so far"), "{}", second.text);
            assert!(second.text.contains("- note: the note"), "the values still go: {}", second.text);
        });
    }

    /// A task with notes, one decision linked to it and one comment, taken by the first step of `p`.
    fn a_task_with_its_context(tx: &WriteTx<'_>, p: &Picture) -> (i64, i64, i64) {
        let project = p.automation.project_id;
        let task = crate::ops::test_support::mk_task_in(tx, "直すもの", Some(project));
        crate::ops::task::update(
            tx,
            task,
            crate::ops::task::TaskPatch { notes: Some("## やること\n壊れた所を直す".into()), ..Default::default() },
        )
        .expect("notes");
        let decision = crate::ops::test_support::mk_decision_in(tx, "直し方を決めた", project);
        crate::ops::decision::update(
            tx,
            decision,
            crate::ops::decision::DecisionPatch { body: Some("こう直す".into()), ..Default::default() },
        )
        .expect("body");
        crate::ops::decision::finish_writing(tx, decision, None).expect("settled");
        crate::ops::decision::link(tx, decision, task).expect("link");
        let comment = crate::ops::comment::add_comment(tx, task, ActorKind::Human, "ここも見て").expect("comment");
        (task, decision, comment.id)
    }

    /// **A step is handed the task its run is on, whole** (`AMB-D-965`) — its notes, the decision linked to
    /// it and its comment — so it does not go looking. The step that takes the task is handed none: it
    /// opens before there is one.
    #[test]
    fn a_step_is_handed_the_task_with_its_decisions_and_comments() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let (task, decision, comment) = a_task_with_its_context(tx, &p);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            assert!(!first.text.contains("## The task this run is on"), "{}", first.text);

            crate::ops::automation_report::take(tx, first.run_step.id, task).expect("take");
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
            let text = second.text;
            assert!(text.contains(&format!("## The task this run is on\n\n**AMB-T-{task}** — 直すもの")), "{text}");
            assert!(text.contains("### Its notes\n\n## やること\n壊れた所を直す"), "{text}");
            assert!(
                text.contains(&format!("### The decisions linked to it\n\n**AMB-D-{decision}** — 直し方を決めた (decided)")),
                "{text}"
            );
            assert!(text.contains("こう直す"), "{text}");
            assert!(text.contains(&format!("### Its comments, oldest first\n\n**AMB-TC-{comment}** — human, ")), "{text}");
            assert!(text.contains("ここも見て"), "{text}");
            // It comes before what the run has done, which is read against it.
            let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("{s} in {text}"));
            assert!(at("## The task this run is on") < at("## What has happened so far"), "{text}");
        });
    }

    #[test]
    fn a_step_told_to_go_without_the_task_is_not_given_it() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let second_step = crate::ops::test_support::only_step(tx, &p.second_action);
            automation::step_update(
                tx,
                second_step.id,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(false),
                Some(false),
                Some(false),
            )
            .expect("task off");
            let (task, _, _) = a_task_with_its_context(tx, &p);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            crate::ops::automation_report::take(tx, first.run_step.id, task).expect("take");
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let second = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open"));
            assert!(!second.text.contains("## The task this run is on"), "{}", second.text);
            assert!(second.text.contains("working on now is AMB-T-"), "the preamble still names it: {}", second.text);
        });
    }

    /// **Each part of the task is on a switch of its own**: a step that keeps the notes and lets the
    /// decisions and the comments go is handed the task's title and notes, and nothing of the other two.
    #[test]
    fn a_step_handed_the_notes_alone_is_given_neither_the_decisions_nor_the_comments() {
        with_tx(|tx| {
            let p = picture(tx, false, true);
            let second_step = crate::ops::test_support::only_step(tx, &p.second_action);
            automation::step_update(
                tx,
                second_step.id,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(false),
                Some(false),
            )
            .expect("decisions and comments off");
            let (task, _, _) = a_task_with_its_context(tx, &p);
            let run = a_run(tx, &p.automation);
            let first = ready(open(tx, run.id, def_of(tx, &run, &p.first).id, None).expect("open"));
            crate::ops::automation_report::take(tx, first.run_step.id, task).expect("take");
            reported(tx, &first.run_step, "found", "Found one thing.", "the note");

            let text = ready(open(tx, run.id, def_of(tx, &run, &p.second).id, None).expect("open")).text;
            assert!(text.contains(&format!("## The task this run is on\n\n**AMB-T-{task}** — 直すもの")), "{text}");
            assert!(text.contains("### Its notes\n\n## やること\n壊れた所を直す"), "{text}");
            assert!(!text.contains("### The decisions linked to it"), "{text}");
            assert!(!text.contains("こう直す"), "{text}");
            assert!(!text.contains("### Its comments"), "{text}");
            assert!(!text.contains("ここも見て"), "{text}");
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
