//! **Launching an automation** — what refuses to start one, the run the launch writes, and the lane
//! that decides whether it starts now or waits.
//!
//! Nothing on the definition side refuses an unfinished automation ([`crate::ops::automation`]): a step
//! with no way onward, an automation with no entry, a required setting nobody answered — each of them
//! saves, because building happens in whatever order the author likes. **This is where they are
//! refused**, which is the moment a person is actually about to be let down by them.
//!
//! **There are four entrances and one launch** — the CLI, a workspace's empty frame, a task's own
//! screen, and the automation tab. Putting the check, the copy and the lane in one place is what keeps
//! them from behaving differently depending on which was pressed.
//!
//! **The check ([`check`]) and the refusal ([`launch`]) are separate doors on purpose.** A build screen
//! draws the list while nobody has pressed anything, so it asks for the list; a launch that cannot go
//! ahead raises it as one refusal carrying a part per reason ([`Msg::part`]), the way a reservation
//! does. Neither of them holds a code of its own yet — the codes are split off a family where a screen
//! puts the refusal in front of a person (`app/src/core/errorCodes.ts`), and the screen that will draw
//! these is not built.
//!
//! **Three things the store cannot answer are handed in** ([`Launcher`]): which agents this machine can
//! actually start, which models each of them offers, and whether the workspace is open. The first two
//! are the reader's own login shell ([`crate::wake`], [`crate::agent_models`]) and the third a window on
//! their screen. Read here, they would be read from whatever process happened to be running — which is
//! the reason [`crate::ops::MadeIn`] is handed in too.
//!
//! **A run stops reading the definition the moment it starts.** Every step is copied into
//! `automation_run_def` at launch, so editing the automation afterwards cannot change what a run
//! already under way is doing, and a run stays readable months later when the automation it came from
//! has moved on.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::model::{
    ActorKind, Automation, AutomationCfg, AutomationExit, AutomationPortDirection,
    AutomationPortKind, AutomationRun, AutomationRunDef, AutomationRunStatus, AutomationStep,
    RunDefCfg, RunDefExit, RunDefPort, ERROR_EXIT,
};
use crate::ops::automation::{declarer, port_declarer};
use crate::ops::emit_create;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// **One thing the launch check found missing.** Eight of them, and every one is something a person can
/// go and fix in the build screen — which is why each names where it is rather than only what it is.
///
/// They are a type rather than eight sentences because both doors need them: the refusal writes them out
/// as English, and the build screen draws them as a list beside the step each belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unmet {
    /// The automation has no steps at all. Nothing else is worth saying about it.
    NoSteps,
    /// No step is named as the entry, so there is nowhere for a run to start and nothing is reachable.
    NoEntry,
    /// The entry declares no `task_take` output, so no step of the run would ever come to hold a task
    /// and every step after it would be about nothing.
    EntryTakesNoTask { step: String },
    /// A way out with nothing set to happen after it. The run would reach it and stop. The error way
    /// out is not one of these — it is carried from birth and halts unless somebody says otherwise.
    OpenExit { step: String, exit: Option<String> },
    /// A required input with nothing reaching it — no wire at all, or none whose far end is both
    /// declared and reachable from the entry.
    UnwiredInput { step: String, port: String },
    /// A required setting nobody answered while building.
    UnansweredCfg { step: String, cfg: String },
    /// A step asking for an agent this machine cannot start. A pane opened on it would come up on
    /// `command not found`.
    AgentMissing { step: String, agent: String },
    /// A step naming a model its agent does not offer here. The pane would come up, and the agent would
    /// turn the model down inside it — which is a refusal the reader only meets once the run is away.
    ///
    /// Only raised for an agent that has already been asked what it offers, and that answered with a
    /// list ([`ModelsHere`]).
    ModelMissing { step: String, agent: String, model: String },
}

impl Unmet {
    /// The English sentence, which is what the CLI prints and what any surface holding no dictionary
    /// falls back to.
    pub fn say(&self) -> String {
        match self {
            Unmet::NoSteps => "it has no steps".to_string(),
            Unmet::NoEntry => "no step is named as the entry".to_string(),
            Unmet::EntryTakesNoTask { step } => {
                format!("the entry '{step}' takes no task — declare a task_take output on one of its ways out")
            }
            Unmet::OpenExit { step, exit } => {
                format!("nothing is set to happen after {} of '{step}'", named(exit.as_deref()))
            }
            Unmet::UnwiredInput { step, port } => {
                format!("the required input '{port}' of '{step}' has nothing reaching it")
            }
            Unmet::UnansweredCfg { step, cfg } => {
                format!("the required setting '{cfg}' of '{step}' is unanswered")
            }
            Unmet::AgentMissing { step, agent } => {
                format!("'{step}' asks for '{agent}', which this machine cannot start")
            }
            Unmet::ModelMissing { step, agent, model } => {
                format!("'{step}' asks for the model '{model}', which '{agent}' here does not offer")
            }
        }
    }
}

/// How a way out is spoken of in a sentence: by its name, or as the unnamed one.
fn named(exit: Option<&str>) -> String {
    match exit {
        Some(name) => format!("the way out '{name}'"),
        None => "the unnamed way out".to_string(),
    }
}

/// **What the store cannot answer**, handed to [`launch`] by whoever pressed it.
pub struct Launcher<'a> {
    /// The agent ids a pane can actually be opened on ([`crate::wake::startable`]), or **`None` for a
    /// machine that has not been asked**.
    ///
    /// `None` is not an empty list and must not be read as one (`AMB-D-792`): the probe starts a login
    /// shell and can be abandoned on a deadline, and an unanswered probe drawn as an answer would tell
    /// a reader with four agents installed that they have none. Where it is `None` the agent check is
    /// not made, rather than made and failed.
    pub startable: Option<&'a [String]>,
    /// The models each agent offers on this machine ([`ModelsHere`]) — empty from a caller that has
    /// asked nobody, which is every caller outside the app.
    pub models: &'a ModelsHere,
    /// How many runs may hold a lane at once ([`crate::config::Config::automation_lanes`]).
    pub lanes: i64,
    /// Whether the talk window is open — `Some(false)` refuses, and **`None` is a caller that cannot
    /// see** (`AMB-D-792`'s discipline, the same one [`Launcher::startable`] takes).
    ///
    /// A run's steps are drawn in the window's panes, so a launch made with it closed has nowhere to
    /// put them, and opening it is the workspace's own act rather than this one's. But only a caller
    /// inside the app can answer: a terminal somewhere else knows nothing about what is on screen, and
    /// a `false` written there would refuse a launch the reader could see perfectly well. So it says
    /// nothing instead, and the run waits for whatever opens its first step.
    pub workspace_open: Option<bool>,
    /// Who pressed launch, or `None` from a caller that says nothing about itself.
    pub by: Option<ActorKind>,
}

/// **The models each agent offers here**, by agent id ([`crate::agent_models`]) — and only the agents
/// that have already been asked and answered with a list.
///
/// An agent **absent from the map is one this machine has nothing to say about**, and its steps' models
/// are left unjudged rather than judged and failed — the discipline [`Launcher::startable`] takes for the
/// same reason (`AMB-D-792`). Absent covers two cases that a screen cannot tell apart and need not:
/// nobody has asked it yet, and it was asked and could offer nothing (not signed in, no such flag, an
/// answer in a shape nothing reads). Both are "no list", never "no models".
///
/// Asking is what keeps it out of the map by default: one ask is a login shell plus a provider starting
/// up, and a check that put that behind every draw of a build screen would charge a reader seconds for
/// opening a picture (`AMB-D-865`).
pub type ModelsHere = BTreeMap<String, Vec<String>>;

/// **A caller that has asked no provider anything** — the map every [`ModelsHere`] slot takes where the
/// question cannot be put at all.
///
/// Handed back by reference rather than made at each call so a `Launcher` can hold it: a terminal is
/// outside the process that keeps the answers ([`crate::agent_models`]), and putting the question there
/// would start a provider per launch for a check the app has already made.
pub fn nothing_asked() -> &'static ModelsHere {
    static EMPTY: std::sync::OnceLock<ModelsHere> = std::sync::OnceLock::new();
    EMPTY.get_or_init(ModelsHere::new)
}

/// `<what> '<id>' not found`, the uncoded refusal the automation entities take
/// ([`crate::ops::automation`] says why).
fn not_found(what: &str, id: i64) -> Error {
    Error::not_found(format!("{what} '{id}' not found"))
}

/// **Is this automation ready to be launched?** An empty answer is yes.
///
/// The list is walked in display order, so a person reading it walks their own picture. Two of the eight
/// answer alone: an automation with no steps has nothing else to say about it, and one with no entry has
/// nothing reachable to say it about — every other check is asked of the steps a run would actually
/// walk, and with no entry that is none of them.
///
/// `startable` is [`Launcher::startable`], and `None` leaves the agent check unmade. `models` is
/// [`Launcher::models`], and an agent it says nothing about leaves that step's model check unmade.
pub fn check(
    conn: &Connection,
    automation_id: i64,
    startable: Option<&[String]>,
    models: &ModelsHere,
) -> Result<Vec<Unmet>> {
    let automation = read::automation(conn, automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    let steps = read::automation_steps_of(conn, automation_id)?;
    if steps.is_empty() {
        return Ok(vec![Unmet::NoSteps]);
    }
    let Some(entry_id) = automation.entry_step_id else {
        return Ok(vec![Unmet::NoEntry]);
    };
    let by_id: BTreeMap<i64, &AutomationStep> = steps.iter().map(|s| (s.id, s)).collect();
    let live = reachable(conn, entry_id, &by_id)?;

    let mut unmet = Vec::new();
    if let Some(entry) = by_id.get(&entry_id) {
        if !takes_a_task(conn, entry)? {
            unmet.push(Unmet::EntryTakesNoTask { step: entry.name.clone() });
        }
    }
    for step in steps.iter().filter(|s| live.contains(&s.id)) {
        for exit in read::automation_exits_of(conn, declarer(step).0, declarer(step).1)? {
            // The error way out is the one nobody has to answer for. Every step and every action is
            // born carrying it (crate::ops::automation), so asking for an edge on each of them
            // would put one more thing to write on every step somebody adds — for the case that is
            // already handled. Left alone it halts the run and calls a person, and an edge on it is
            // how somebody says otherwise.
            if exit.name.as_deref() == Some(ERROR_EXIT) {
                continue;
            }
            if !decided(conn, step.id, exit.name.as_deref(), &by_id)? {
                unmet.push(Unmet::OpenExit { step: step.name.clone(), exit: exit.name.clone() });
            }
        }
        let (port_kind, port_owner) = port_declarer(step);
        for port in read::automation_ports_of(conn, port_kind, port_owner, AutomationPortDirection::In)? {
            if port.required && !fed(conn, step, &port.name, &live, &by_id)? {
                unmet.push(Unmet::UnwiredInput { step: step.name.clone(), port: port.name });
            }
        }
        for cfg in settings_of(conn, step)? {
            if cfg.required && cfg.value.is_none() {
                unmet.push(Unmet::UnansweredCfg { step: step.name.clone(), cfg: cfg.name });
            }
        }
        if let Some(startable) = startable {
            if !startable.iter().any(|id| id == &step.agent) {
                unmet.push(Unmet::AgentMissing {
                    step: step.name.clone(),
                    agent: step.agent.clone(),
                });
            }
        }
        // Asked of the step's own model, and only where the agent offered a list (`ModelsHere`). A
        // step naming no model is on whatever the provider's own settings have, which is not a name
        // this could judge.
        if let (Some(model), Some(offered)) = (step.model.as_deref(), models.get(&step.agent)) {
            if !offered.iter().any(|id| id == model) {
                unmet.push(Unmet::ModelMissing {
                    step: step.name.clone(),
                    agent: step.agent.clone(),
                    model: model.to_string(),
                });
            }
        }
    }
    Ok(unmet)
}

/// The steps a run could actually reach, walked from the entry along the edges that go on to another
/// step. A step nothing reaches is not checked: it cannot stop a run, and refusing to launch over one
/// would make an automation undeletable-in-practice while its author was still drawing it.
fn reachable(
    conn: &Connection,
    entry_id: i64,
    by_id: &BTreeMap<i64, &AutomationStep>,
) -> Result<BTreeSet<i64>> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![entry_id];
    while let Some(id) = todo.pop() {
        if !by_id.contains_key(&id) || !seen.insert(id) {
            continue;
        }
        let step = by_id[&id];
        for exit in read::automation_exits_of(conn, declarer(step).0, declarer(step).1)? {
            let edge = read::automation_edge_for_exit(conn, id, exit.name.as_deref())?;
            if let Some(next) = edge.and_then(|e| e.to_step_id) {
                todo.push(next);
            }
        }
    }
    Ok(seen)
}

/// Whether a way out has something set to happen after it. An edge that goes on to a step of some other
/// automation — or to none — decides nothing, so it counts as undecided rather than as an edge.
fn decided(
    conn: &Connection,
    step_id: i64,
    exit_name: Option<&str>,
    by_id: &BTreeMap<i64, &AutomationStep>,
) -> Result<bool> {
    let Some(edge) = read::automation_edge_for_exit(conn, step_id, exit_name)? else {
        return Ok(false);
    };
    Ok(match edge.to_step_id {
        Some(to) => by_id.contains_key(&to),
        None => true,
    })
}

/// Whether a step declares a `task_take` output on any of its ways out — the thing that makes it usable
/// as an entry, since the task it comes out holding is what the run is about from there on.
fn takes_a_task(conn: &Connection, step: &AutomationStep) -> Result<bool> {
    for exit in read::automation_exits_of(conn, declarer(step).0, declarer(step).1)? {
        if outs_of(conn, &exit)?.iter().any(|p| p.kind == AutomationPortKind::TaskTake) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What one way out hands on. An output belongs to the way out that produced it, so this is the only
/// owner it is ever asked of.
fn outs_of(conn: &Connection, exit: &AutomationExit) -> Result<Vec<crate::model::AutomationPort>> {
    Ok(read::automation_ports_of(
        conn,
        crate::model::AutomationPortOwner::Exit,
        exit.id,
        AutomationPortDirection::Out,
    )?)
}

/// Whether anything actually reaches one input. A wire counts only where **both** halves hold: its far
/// end is declared — that step's way out really hands on a port of that name — and that step is
/// reachable from the entry. A wire whose far end was renamed underneath it is parted rather than
/// rewritten ([`crate::ops::automation`]), and a wire from a step no run reaches would never carry
/// anything, so neither of them feeds an input.
fn fed(
    conn: &Connection,
    step: &AutomationStep,
    port_name: &str,
    live: &BTreeSet<i64>,
    by_id: &BTreeMap<i64, &AutomationStep>,
) -> Result<bool> {
    for wire in read::automation_wires_to_port(conn, step.id, port_name)? {
        if !live.contains(&wire.from_step_id) {
            continue;
        }
        let Some(from) = by_id.get(&wire.from_step_id) else { continue };
        let exit = read::automation_exit_by_name(
            conn,
            declarer(from).0,
            declarer(from).1,
            wire.from_exit_name.as_deref(),
        )?;
        let Some(exit) = exit else { continue };
        if outs_of(conn, &exit)?.iter().any(|p| p.name == wire.from_port_name) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// One step's settings, **declaration and answer together**. Public because the build screen draws the
/// same pair and must not put them back together a second way ([`crate::ops::automation::cfg_set`]). A step carrying its own prompt declared
/// them itself and answers on the same row; a step running a library action reads the declaration from
/// the action and answers on a row of its own under the same name
/// ([`crate::ops::automation::cfg_set`]), so the two have to be put back together here.
pub fn settings_of(conn: &Connection, step: &AutomationStep) -> Result<Vec<AutomationCfg>> {
    let (owner_kind, owner_id) = declarer(step);
    let declared = read::automation_cfgs_of(conn, owner_kind, owner_id)?;
    if owner_kind == crate::model::AutomationOwner::Step {
        return Ok(declared);
    }
    let mut out = Vec::with_capacity(declared.len());
    for mut cfg in declared {
        cfg.value = read::automation_cfg_by_name(
            conn,
            crate::model::AutomationOwner::Step,
            step.id,
            &cfg.name,
        )?
        .and_then(|answer| answer.value);
        out.push(cfg);
    }
    Ok(out)
}

/// **Launch an automation**: check it, copy its steps into a run, and take a lane if one is free.
///
/// Three things refuse, in this order.
///
/// - **Archived.** Archiving is what keeps an automation nobody launches any more out of the lists, and
///   launching one straight past that would make the word mean nothing.
/// - **The check** ([`check`]), as one `not_ready` refusal carrying a part per reason.
/// - **The workspace being closed**, last on purpose. What the check found is wrong with the automation
///   and stays wrong after a window is opened, so saying "open a window" first would send somebody to do
///   that and then tell them the automation was never going to run.
///
/// The run is born `running` where a lane is free and `queued` where none is, and `started_at` marks the
/// first of those — so "launched" and "started" are two moments and a queue's wait is readable.
pub fn launch(tx: &WriteTx<'_>, automation_id: i64, by: &Launcher<'_>) -> Result<AutomationRun> {
    let automation: Automation = read::automation(tx.conn(), automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    if automation.archived {
        return Err(Error::invalid(format!(
            "automation '{}' is archived — bring it back before launching it",
            automation.name
        )));
    }
    let unmet = check(tx.conn(), automation_id, by.startable, by.models)?;
    if !unmet.is_empty() {
        return Err(not_ready(&automation.name, &unmet));
    }
    // Only where somebody answered. A caller that cannot see the window says nothing rather than
    // `false`, and the run is made — a launch from a terminal is not a claim about what is on screen.
    if by.workspace_open == Some(false) {
        return Err(Error::invalid(
            "the workspace is closed — a run draws its steps in its panes, so open it and launch again",
        ));
    }
    let now = Timestamp::now();
    let held = read::automation_run_ids_running(tx.conn())?.len() as i64;
    let status = if held < by.lanes { AutomationRunStatus::Running } else { AutomationRunStatus::Queued };
    let run = AutomationRun {
        id: read::next_id(tx.conn(), "automation_run")?,
        automation_id,
        project_id: automation.project_id,
        status,
        pause_requested: false,
        stopped_reason: None,
        started_by_kind: by.by,
        started_at: status.holds_a_lane().then_some(now),
        ended_at: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run(&run))?;
    for step in read::automation_steps_of(tx.conn(), automation_id)? {
        let def = snapshot(tx, run.id, &step, now)?;
        emit_create(tx, record::automation_run_def(&def))?;
    }
    Ok(run)
}

/// Build the body of the `not_ready` refusal: one refusal over a list of reasons whose length is only
/// known here. Each reason rides as a part rather than being folded into the sentence, because joining
/// them is punctuation and punctuation belongs to the language doing the reading — the same shape a
/// reservation's refusal takes ([`crate::ops::task`]).
fn not_ready(name: &str, unmet: &[Unmet]) -> Error {
    let sentence = format!(
        "cannot launch '{name}': {}",
        unmet.iter().map(Unmet::say).collect::<Vec<_>>().join("; ")
    );
    let msg = unmet.iter().fold(
        Msg::new(sentence).coded(ErrorCode::NotReady).with("automation", name),
        |msg, one| msg.part(Msg::new(one.say())),
    );
    Error::NotReady(msg)
}

/// **One step, as it stands at this moment** — the copy a run reads from then on.
///
/// The prompt is resolved here rather than kept as a pointer: a step running a library action would
/// otherwise read whatever that action was edited into halfway through the run. The three JSON columns
/// hold what has no column of its own, and they are written from the same two places every other read of
/// a step's declarations goes to ([`declarer`], [`port_declarer`]).
fn snapshot(
    tx: &WriteTx<'_>,
    run_id: i64,
    step: &AutomationStep,
    now: Timestamp,
) -> Result<AutomationRunDef> {
    let conn = tx.conn();
    let prompt = match step.action_id {
        None => step.prompt.clone(),
        Some(action_id) => read::automation_action(conn, action_id)?.map(|a| a.prompt),
    };
    let mut exits = Vec::new();
    for exit in read::automation_exits_of(conn, declarer(step).0, declarer(step).1)? {
        let outs = outs_of(conn, &exit)?
            .into_iter()
            .map(|p| RunDefPort { name: p.name, kind: p.kind, required: p.required })
            .collect();
        exits.push(RunDefExit { name: exit.name.clone(), outs });
    }
    let (port_kind, port_owner) = port_declarer(step);
    let ins: Vec<RunDefPort> =
        read::automation_ports_of(conn, port_kind, port_owner, AutomationPortDirection::In)?
            .into_iter()
            .map(|p| RunDefPort { name: p.name, kind: p.kind, required: p.required })
            .collect();
    let cfg: Vec<RunDefCfg> = settings_of(conn, step)?
        .into_iter()
        .map(|c| RunDefCfg {
            name: c.name,
            kind: c.kind,
            required: c.required,
            options: c.options,
            value: c.value,
        })
        .collect();
    Ok(AutomationRunDef {
        id: read::next_id(conn, "automation_run_def")?,
        run_id,
        step_id: Some(step.id),
        name: step.name.clone(),
        prompt,
        agent: step.agent.clone(),
        model: step.model.clone(),
        interactive: step.interactive,
        work_dir_ref: step.work_dir_ref.clone(),
        report_to_task: step.report_to_task,
        show_history: step.show_history,
        exits: serde_json::to_string(&exits).map_err(Error::from)?,
        ins: serde_json::to_string(&ins).map_err(Error::from)?,
        cfg: serde_json::to_string(&cfg).map_err(Error::from)?,
        created_at: now,
        updated_at: now,
    })
}

/// **The step a run starts at** — the copy taken at launch of the automation's entry step, or `None`
/// where the automation has since lost its entry or the run carries no copy of it.
///
/// It is read off the live definition's `entry_step_id` and matched against the copies by `step_id`,
/// because the copy itself does not say which of them the entry was: the picture is walked from the
/// entry along the edges, and a run that is under way has already walked past it.
pub fn entry_def(conn: &Connection, run_id: i64) -> Result<Option<AutomationRunDef>> {
    let Some(run) = read::automation_run(conn, run_id)? else {
        return Err(not_found("run", run_id));
    };
    let Some(entry) = read::automation(conn, run.automation_id)?.and_then(|a| a.entry_step_id) else {
        return Ok(None);
    };
    Ok(read::automation_run_defs_of(conn, run_id)?
        .into_iter()
        .find(|def| def.step_id == Some(entry)))
}

/// **A lane came free: wake the run that has waited longest**, and answer which one it was.
///
/// This is called by whatever handed a lane back — a run that finished, one that was paused, one that
/// was stopped — rather than by anything watching the clock. A queue polled on a timer would leave a run
/// sitting for however long the timer was, in exchange for nothing: the only moment the answer can
/// change is the moment a lane is released, and that moment is code, not a tick.
///
/// It wakes **one** run, because one lane came free. Answers `None` where nothing was waiting, and where
/// the lanes are full anyway — the caller does not have to know which of those it is, and a lane that
/// was released and immediately re-taken is not an error.
pub fn promote_next(tx: &WriteTx<'_>, lanes: i64) -> Result<Option<AutomationRun>> {
    if read::automation_run_ids_running(tx.conn())?.len() as i64 >= lanes {
        return Ok(None);
    }
    let Some(before) = read::automation_run_first_queued(tx.conn())? else {
        return Ok(None);
    };
    let now = Timestamp::now();
    let mut after = before.clone();
    after.status = AutomationRunStatus::Running;
    after.started_at = Some(now);
    after.updated_at = now;
    crate::ops::emit_update(tx, record::automation_run(&before), record::automation_run(&after))?;
    Ok(Some(after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AutomationOwner, AutomationPortOwner};
    use crate::ops::automation::{
        self, EdgeTarget, NewAutomation, NewStep, StepSource,
    };
    use crate::ops::test_support::{mk_project, with_tx};

    /// What every test here starts from: one automation, one step that takes a task and closes the run,
    /// and every way out of it decided. It launches as it stands, so each test can take one thing back
    /// off and watch the check find it.
    fn launchable(tx: &WriteTx<'_>) -> (Automation, AutomationStep) {
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(
            tx,
            project,
            NewAutomation { name: "1件やりきる".into(), ..Default::default() },
        )
        .expect("add automation");
        let step = automation::step_add(
            tx,
            automation.id,
            NewStep::with_prompt("取る", "take one", "claude"),
        )
        .expect("add step");
        takes_task_on(tx, &step, None);
        automation::edge_add(tx, step.id, None, EdgeTarget::Done, None).expect("edge");
        // Nothing is written for the error way out: it is carried from birth and halts unless
        // somebody says otherwise, which is what `an_error_way_out_nobody_answered_for_is_not_open`
        // holds this to.
        let automation = automation::set_entry(tx, automation.id, Some(step.id)).expect("entry");
        (automation, step)
    }

    /// Declare a `task_take` output on one way out of a step — what makes it usable as an entry.
    fn takes_task_on(tx: &WriteTx<'_>, step: &AutomationStep, exit_name: Option<&str>) {
        let exit = read::automation_exit_by_name(
            tx.conn(),
            declarer(step).0,
            declarer(step).1,
            exit_name,
        )
        .expect("read")
        .expect("the way out");
        automation::port_add(
            tx,
            AutomationPortOwner::Exit,
            exit.id,
            AutomationPortDirection::Out,
            "タスク",
            AutomationPortKind::TaskTake,
            true,
        )
        .expect("port");
    }

    /// The machine every test launches on: one that can start `claude`, with three lanes and a window
    /// open.
    fn here<'a>(startable: &'a [String]) -> Launcher<'a> {
        Launcher {
            startable: Some(startable),
            models: nothing_asked(),
            lanes: 3,
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        }
    }

    fn claude() -> Vec<String> {
        vec!["claude".to_string()]
    }

    #[test]
    fn a_finished_automation_passes_the_check_and_launches_running() {
        with_tx(|tx| {
            let (automation, _) = launchable(tx);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
            );
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            assert_eq!(run.status, AutomationRunStatus::Running, "a lane was free");
            assert!(run.started_at.is_some(), "taking a lane is what starts it");
            assert_eq!(run.project_id, automation.project_id);
        });
    }

    #[test]
    fn an_automation_with_no_steps_says_only_that() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = automation::add(
                tx,
                project,
                NewAutomation { name: "空".into(), ..Default::default() },
            )
            .expect("add");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::NoSteps],
                "nothing else is worth saying about it",
            );
        });
    }

    #[test]
    fn an_automation_with_no_entry_says_only_that() {
        with_tx(|tx| {
            let (automation, _) = launchable(tx);
            automation::set_entry(tx, automation.id, None).expect("clear entry");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::NoEntry],
                "with no entry nothing is reachable, so every other check is asked of nothing",
            );
        });
    }

    #[test]
    fn an_entry_that_takes_no_task_is_refused() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = automation::add(
                tx,
                project,
                NewAutomation { name: "1件やりきる".into(), ..Default::default() },
            )
            .expect("add");
            let step = automation::step_add(
                tx,
                automation.id,
                NewStep::with_prompt("取る", "take one", "claude"),
            )
            .expect("step");
            automation::edge_add(tx, step.id, None, EdgeTarget::Done, None).expect("edge");
            automation::set_entry(tx, automation.id, Some(step.id)).expect("entry");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::EntryTakesNoTask { step: "取る".into() }],
            );
        });
    }

    #[test]
    fn a_way_out_with_nothing_after_it_is_refused() {
        with_tx(|tx| {
            let (automation, step) = launchable(tx);
            let exit = automation::exit_add(tx, AutomationOwner::Step, step.id, Some("直すところがある"))
                .expect("exit");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::OpenExit { step: "取る".into(), exit: exit.name.clone() }],
                "it saves while building, and is refused at launch",
            );
        });
    }

    #[test]
    fn an_error_way_out_nobody_answered_for_is_not_open() {
        with_tx(|tx| {
            // `launchable` writes no edge on the error way out, so a check that asked for one would
            // refuse the automation every other test here launches.
            let (automation, _) = launchable(tx);
            let unmet = check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check");
            assert_eq!(unmet, vec![], "the error way out is carried from birth, not written");
        });
    }

    #[test]
    fn a_required_input_nothing_reaches_is_refused() {
        with_tx(|tx| {
            let (automation, step) = launchable(tx);
            automation::port_add(
                tx,
                AutomationPortOwner::Step,
                step.id,
                AutomationPortDirection::In,
                "下書き",
                AutomationPortKind::Value,
                true,
            )
            .expect("port");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnwiredInput { step: "取る".into(), port: "下書き".into() }],
            );
        });
    }

    #[test]
    fn a_wire_from_a_step_no_run_reaches_does_not_feed_an_input() {
        with_tx(|tx| {
            let (automation, entry) = launchable(tx);
            // A second step, wired into the entry's input but reached by nothing: the run would walk
            // straight past it, so what it hands on never arrives.
            let orphan = automation::step_add(
                tx,
                automation.id,
                NewStep::with_prompt("書く", "write it", "claude"),
            )
            .expect("step");
            let exit = read::automation_exit_by_name(
                tx.conn(),
                AutomationOwner::Step,
                orphan.id,
                None,
            )
            .expect("read")
            .expect("way out");
            automation::port_add(
                tx,
                AutomationPortOwner::Exit,
                exit.id,
                AutomationPortDirection::Out,
                "下書き",
                AutomationPortKind::Value,
                false,
            )
            .expect("out");
            automation::port_add(
                tx,
                AutomationPortOwner::Step,
                entry.id,
                AutomationPortDirection::In,
                "下書き",
                AutomationPortKind::Value,
                true,
            )
            .expect("in");
            automation::wire_add(tx, orphan.id, None, "下書き", entry.id, "下書き").expect("wire");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnwiredInput { step: "取る".into(), port: "下書き".into() }],
                "and the orphan's own ways out are not checked either — no run reaches them",
            );
        });
    }

    #[test]
    fn a_required_setting_nobody_answered_is_refused() {
        with_tx(|tx| {
            let (automation, step) = launchable(tx);
            automation::cfg_add(
                tx,
                AutomationOwner::Step,
                step.id,
                "作業フォルダ",
                crate::model::AutomationCfgKind::Folder,
                true,
                None,
            )
            .expect("cfg");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnansweredCfg { step: "取る".into(), cfg: "作業フォルダ".into() }],
            );
            automation::cfg_set(tx, step.id, "作業フォルダ", Some("\"~/work\"")).expect("answer");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"), vec![]);
        });
    }

    #[test]
    fn a_library_actions_setting_is_answered_on_the_step() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = automation::add(
                tx,
                project,
                NewAutomation { name: "1件やりきる".into(), ..Default::default() },
            )
            .expect("add");
            let action = automation::action_add(tx, Some(project), "取る", "take one").expect("action");
            automation::cfg_add(
                tx,
                AutomationOwner::Action,
                action.id,
                "絞り込み",
                crate::model::AutomationCfgKind::TaskFilter,
                true,
                None,
            )
            .expect("cfg");
            let step = automation::step_add(
                tx,
                automation.id,
                NewStep { source: StepSource::Action(action.id), ..NewStep::with_prompt("取る", "", "claude") },
            )
            .expect("step");
            takes_task_on(tx, &step, None);
            automation::edge_add(tx, step.id, None, EdgeTarget::Done, None).expect("edge");
            automation::set_entry(tx, automation.id, Some(step.id)).expect("entry");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnansweredCfg { step: "取る".into(), cfg: "絞り込み".into() }],
                "the declaration is the action's and the answer is the step's",
            );
            automation::cfg_set(tx, step.id, "絞り込み", Some("{}")).expect("answer");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"), vec![]);
        });
    }

    #[test]
    fn an_agent_this_machine_cannot_start_is_refused_and_an_unasked_machine_is_not() {
        with_tx(|tx| {
            let (automation, _) = launchable(tx);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&[]), nothing_asked()).expect("check"),
                vec![Unmet::AgentMissing { step: "取る".into(), agent: "claude".into() }],
            );
            assert_eq!(
                check(tx.conn(), automation.id, None, nothing_asked()).expect("check"),
                vec![],
                "a machine nobody asked is not a machine with nothing on it (AMB-D-792)",
            );
        });
    }

    /// The models an agent said it can be started on, as the app remembers them.
    fn offering(agent: &str, models: &[&str]) -> ModelsHere {
        ModelsHere::from([(
            agent.to_string(),
            models.iter().map(|one| (*one).to_string()).collect::<Vec<String>>(),
        )])
    }

    #[test]
    fn a_model_the_agent_does_not_offer_here_is_refused() {
        with_tx(|tx| {
            let (automation, step) = launchable(tx);
            automation::step_update(
                tx,
                step.id,
                None,
                None,
                None,
                Some(Some("opus-9")),
                None,
                None,
                None,
                None,
            )
            .expect("name a model");
            assert_eq!(
                check(
                    tx.conn(),
                    automation.id,
                    Some(&claude()),
                    &offering("claude", &["sonnet", "haiku"]),
                )
                .expect("check"),
                vec![Unmet::ModelMissing {
                    step: "取る".into(),
                    agent: "claude".into(),
                    model: "opus-9".into(),
                }],
            );
            assert_eq!(
                check(
                    tx.conn(),
                    automation.id,
                    Some(&claude()),
                    &offering("claude", &["sonnet", "opus-9"]),
                )
                .expect("check"),
                vec![],
                "a model the agent does offer is no reason at all",
            );
        });
    }

    #[test]
    fn a_model_is_judged_only_against_an_agent_that_answered() {
        with_tx(|tx| {
            let (automation, step) = launchable(tx);
            automation::step_update(
                tx,
                step.id,
                None,
                None,
                None,
                Some(Some("opus-9")),
                None,
                None,
                None,
                None,
            )
            .expect("name a model");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
                "an agent nobody has asked says nothing about its models (AMB-D-865)",
            );
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), &offering("codex-cli", &["gpt"]))
                    .expect("check"),
                vec![],
                "another agent's list is not this one's",
            );
        });
    }

    #[test]
    fn a_step_naming_no_model_is_not_judged_on_one() {
        with_tx(|tx| {
            let (automation, _) = launchable(tx);
            assert_eq!(
                check(
                    tx.conn(),
                    automation.id,
                    Some(&claude()),
                    &offering("claude", &["sonnet"]),
                )
                .expect("check"),
                vec![],
                "no model named is the provider's own settings, which this cannot judge",
            );
        });
    }

    #[test]
    fn an_archived_automation_and_a_closed_workspace_each_refuse_the_launch() {
        with_tx(|tx| {
            let (automation, _) = launchable(tx);
            let startable = claude();
            let closed = Launcher { workspace_open: Some(false), ..here(&startable) };
            assert!(launch(tx, automation.id, &closed).is_err());
            automation::update(tx, automation.id, None, None, None, Some(true)).expect("archive");
            assert!(launch(tx, automation.id, &here(&claude())).is_err());
        });
    }

    #[test]
    fn the_refusal_carries_one_part_per_reason() {
        with_tx(|tx| {
            let (automation, step) = launchable(tx);
            automation::exit_add(tx, AutomationOwner::Step, step.id, Some("直すところがある"))
                .expect("exit");
            let err = launch(tx, automation.id, &here(&[])).expect_err("refused");
            let Error::NotReady(msg) = err else { panic!("a launch that cannot go ahead is not_ready") };
            assert_eq!(msg.parts().len(), 2, "one open way out and one agent this machine has not");
        });
    }

    #[test]
    fn the_steps_are_copied_into_the_run_and_stop_following_the_definition() {
        with_tx(|tx| {
            let (automation, step) = launchable(tx);
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            let defs = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert_eq!(defs.len(), 1);
            assert_eq!(defs[0].name, "取る");
            assert_eq!(defs[0].prompt.as_deref(), Some("take one"));
            assert_eq!(defs[0].step_id, Some(step.id));
            let exits: Vec<RunDefExit> = serde_json::from_str(&defs[0].exits).expect("exits");
            assert_eq!(exits.len(), 2, "the unnamed way out and the error one");
            assert_eq!(exits[0].outs[0].kind, AutomationPortKind::TaskTake);

            automation::step_update(
                tx,
                step.id,
                Some("取り直す"),
                Some(StepSource::Prompt("take another".into())),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("edit the definition under the run");
            let defs = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert_eq!(defs[0].name, "取る", "the copy is what the run reads from here on");
        });
    }

    #[test]
    fn a_launch_with_every_lane_held_waits_its_turn_and_is_woken_when_one_comes_free() {
        with_tx(|tx| {
            let (automation, _) = launchable(tx);
            let startable = claude();
            let one_lane = Launcher { lanes: 1, ..here(&startable) };
            let first = launch(tx, automation.id, &one_lane).expect("launch");
            let second = launch(tx, automation.id, &one_lane).expect("launch");
            assert_eq!(first.status, AutomationRunStatus::Running);
            assert_eq!(second.status, AutomationRunStatus::Queued);
            assert!(second.started_at.is_none(), "a queued run has not started");

            assert!(
                promote_next(tx, 1).expect("promote").is_none(),
                "the lane is still held, so nothing is woken",
            );
            let mut done = first.clone();
            done.status = AutomationRunStatus::Done;
            crate::ops::emit_update(
                tx,
                record::automation_run(&first),
                record::automation_run(&done),
            )
            .expect("hand the lane back");
            let woken = promote_next(tx, 1).expect("promote").expect("the one that waited");
            assert_eq!(woken.id, second.id);
            assert_eq!(woken.status, AutomationRunStatus::Running);
            assert!(woken.started_at.is_some());
            assert!(promote_next(tx, 1).expect("promote").is_none(), "nothing left waiting");
        });
    }
}
