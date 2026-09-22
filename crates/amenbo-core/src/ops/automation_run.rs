//! **Launching an automation** — what refuses to start one, and the run the launch writes.
//!
//! Nothing on the definition side refuses an unfinished automation ([`crate::ops::automation`]): a step
//! with no way onward, an automation with no entry, a required setting nobody answered — each of them
//! saves, because building happens in whatever order the author likes. **This is where they are
//! refused**, which is the moment a person is actually about to be let down by them.
//!
//! **There are four entrances and one launch** — the CLI, a workspace's empty frame, a task's own
//! screen, and the automation tab. Putting the check and the copy in one place is what keeps them
//! from behaving differently depending on which was pressed.
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
    ActorKind, Automation, AutomationCfg, AutomationCfgOwner, AutomationEnds, AutomationExit,
    AutomationOwner, AutomationPictureOwner, AutomationPlacement, AutomationPortDirection,
    AutomationPortKind, AutomationPortOwner, AutomationRun, AutomationRunDef, AutomationRunStatus,
    AutomationRunStepStatus,
    RunDefCfg, RunDefExit, RunDefPort, ERROR_EXIT,
};
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
    /// Nothing is placed on the automation at all. Nothing else is worth saying about it.
    NoSteps,
    /// No placement is named as the entry, so there is nowhere for a run to start and nothing is
    /// reachable.
    NoEntry,
    /// An action standing on the picture has no step to open — none written yet, or none named as its
    /// entry. A run reaching that spot would have no terminal to put up.
    ActionEmpty { action: String },
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
            Unmet::NoSteps => "no action is placed on it".to_string(),
            Unmet::NoEntry => "no placement is named as the entry".to_string(),
            Unmet::ActionEmpty { action } => {
                format!("the action '{action}' placed on it has no step to start at")
            }
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

impl Unmet {
    /// The code naming **this** reason, so a screen can write it in the reader's language
    /// ([`crate::ErrorCode`], `AMB-D-413`). [`Unmet::say`] is the English the CLI prints and the
    /// fallback for a surface holding no dictionary; this is the other half of the same sentence.
    pub fn code(&self) -> ErrorCode {
        match self {
            Unmet::NoSteps => ErrorCode::NotReadyAutomationNoSteps,
            Unmet::NoEntry => ErrorCode::NotReadyAutomationNoEntry,
            Unmet::ActionEmpty { .. } => ErrorCode::NotReadyAutomationActionEmpty,
            Unmet::EntryTakesNoTask { .. } => ErrorCode::NotReadyAutomationEntryTakesNoTask,
            // The unnamed way out is a sentence of its own: there is no name to put in one.
            Unmet::OpenExit { exit: None, .. } => ErrorCode::NotReadyAutomationOpenExitUnnamed,
            Unmet::OpenExit { .. } => ErrorCode::NotReadyAutomationOpenExit,
            Unmet::UnwiredInput { .. } => ErrorCode::NotReadyAutomationUnwiredInput,
            Unmet::UnansweredCfg { .. } => ErrorCode::NotReadyAutomationUnansweredCfg,
            Unmet::AgentMissing { .. } => ErrorCode::NotReadyAutomationAgentMissing,
            Unmet::ModelMissing { .. } => ErrorCode::NotReadyAutomationModelMissing,
        }
    }

    /// This reason as a sentence that carries its values apart from its words: the code above, the
    /// English underneath, and every name the template interpolates as a field of its own.
    ///
    /// Public because both doors hand the same thing over — the refusal raises these as its parts, and
    /// the check's own list is drawn from them (`app/src-tauri/src/automation.rs`). Read twice, the two
    /// lists came apart (`AMB-T-5287`).
    pub fn msg(&self) -> Msg {
        let msg = Msg::new(self.say()).coded(self.code());
        match self {
            Unmet::NoSteps | Unmet::NoEntry => msg,
            Unmet::ActionEmpty { action } => msg.with("action", action),
            Unmet::EntryTakesNoTask { step } => msg.with("step", step),
            Unmet::OpenExit { step, exit } => match exit {
                Some(exit) => msg.with("step", step).with("exit", exit),
                None => msg.with("step", step),
            },
            Unmet::UnwiredInput { step, port } => msg.with("step", step).with("port", port),
            Unmet::UnansweredCfg { step, cfg } => msg.with("step", step).with("cfg", cfg),
            Unmet::AgentMissing { step, agent } => msg.with("step", step).with("agent", agent),
            Unmet::ModelMissing { step, model, .. } => msg.with("step", step).with("model", model),
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
/// The list is walked in display order, so a person reading it walks their own picture. Two of the nine
/// answer alone: an automation with nothing placed on it has nothing else to say about it, and one with
/// no entry has nothing reachable to say it about — every other check is asked of the placements a run
/// would actually walk, and with no entry that is none of them.
///
/// **What is asked of a placement is asked of the action standing on it**, except the agent and the
/// model: those are each inner step's own answer (`AMB-D-950`), so they are asked of every step the
/// action could open ([`steps_opened_by`]) rather than of the placement.
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
    let placements = read::automation_placements_of(conn, automation_id)?;
    if placements.is_empty() {
        return Ok(vec![Unmet::NoSteps]);
    }
    let Some(entry_id) = automation.entry_placement_id else {
        return Ok(vec![Unmet::NoEntry]);
    };
    let by_id: BTreeMap<i64, &AutomationPlacement> = placements.iter().map(|p| (p.id, p)).collect();
    let live = reachable(conn, entry_id, &by_id)?;

    let mut unmet = Vec::new();
    if let Some(entry) = by_id.get(&entry_id) {
        if !takes_a_task(conn, entry.action_id)? {
            unmet.push(Unmet::EntryTakesNoTask { step: action_name(conn, entry.action_id)? });
        }
    }
    for placement in placements.iter().filter(|p| live.contains(&p.id)) {
        let name = action_name(conn, placement.action_id)?;
        for exit in read::automation_exits_of(conn, AutomationOwner::Action, placement.action_id)? {
            // The error way out is the one nobody has to answer for. Every step and every action is
            // born carrying it (crate::ops::automation), so asking for an edge on each of them
            // would put one more thing to write on every action somebody places — for the case that
            // is already handled. Left alone it halts the run and calls a person, and an edge on it
            // is how somebody says otherwise.
            if exit.name.as_deref() == Some(ERROR_EXIT) {
                continue;
            }
            if !decided(conn, placement.id, exit.name.as_deref(), &by_id)? {
                unmet.push(Unmet::OpenExit { step: name.clone(), exit: exit.name.clone() });
            }
        }
        for port in read::automation_ports_of(
            conn,
            AutomationPortOwner::Action,
            placement.action_id,
            AutomationPortDirection::In,
        )? {
            if port.required && !fed(conn, placement, &port.name, &live, &by_id)? {
                unmet.push(Unmet::UnwiredInput { step: name.clone(), port: port.name });
            }
        }
        for cfg in settings_of(conn, placement)? {
            if cfg.required && cfg.value.is_none() {
                unmet.push(Unmet::UnansweredCfg { step: name.clone(), cfg: cfg.name });
            }
        }
        let steps = steps_opened_by(conn, placement.action_id)?;
        if steps.is_empty() {
            unmet.push(Unmet::ActionEmpty { action: name.clone() });
            continue;
        }
        for step in &steps {
            if let Some(startable) = startable {
                if !startable.iter().any(|id| id == &step.agent) {
                    unmet.push(Unmet::AgentMissing {
                        step: step.name.clone(),
                        agent: step.agent.clone(),
                    });
                }
            }
            // Asked of the step's own model, and only where the agent offered a list (`ModelsHere`).
            // A step naming no model is on whatever the provider's own settings have, which is not a
            // name this could judge.
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
    }
    Ok(unmet)
}

/// What a placement is called in a refusal: the name of the action standing on it.
fn action_name(conn: &Connection, action_id: i64) -> Result<String> {
    Ok(read::automation_action(conn, action_id)?
        .map(|a| a.name)
        .unwrap_or_else(|| format!("action '{action_id}'")))
}

/// **The steps a run opens when it reaches one action** — the one it starts at, and every step the
/// picture inside leads on to from there. An action with no entry opens none of them, which is what
/// [`Unmet::ActionEmpty`] is raised on.
///
/// Which of them one run walks is decided by the ways out taken while it goes; this is every one it
/// could walk, in display order, and that is what the agent and the model are asked of (`AMB-D-950`).
/// A step nothing inside leads to is left out for the reason [`reachable`] leaves a placement out: no
/// pane ever comes up on it, so refusing the launch over the agent it names would hold a run back for
/// a box still being drawn.
fn steps_opened_by(conn: &Connection, action_id: i64) -> Result<Vec<crate::model::AutomationStep>> {
    let Some(entry) = read::automation_action(conn, action_id)?.and_then(|a| a.entry_step_id) else {
        return Ok(Vec::new());
    };
    let steps = read::automation_action_steps_of(conn, action_id)?;
    let ids: BTreeSet<i64> = steps.iter().map(|s| s.id).collect();
    let mut seen = BTreeSet::new();
    let mut todo = vec![entry];
    while let Some(id) = todo.pop() {
        if !ids.contains(&id) || !seen.insert(id) {
            continue;
        }
        for edge in read::automation_edges_from(conn, AutomationPictureOwner::Action, id)? {
            if let Some(next) = edge.to_id {
                todo.push(next);
            }
        }
    }
    Ok(steps.into_iter().filter(|step| seen.contains(&step.id)).collect())
}

/// The placements a run could actually reach, walked from the entry along the edges that go on to
/// another placement. A placement nothing reaches is not checked: it cannot stop a run, and refusing to
/// launch over one would make an automation undeletable-in-practice while its author was still drawing
/// it.
fn reachable(
    conn: &Connection,
    entry_id: i64,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<BTreeSet<i64>> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![entry_id];
    while let Some(id) = todo.pop() {
        if !by_id.contains_key(&id) || !seen.insert(id) {
            continue;
        }
        let placement = by_id[&id];
        for exit in read::automation_exits_of(conn, AutomationOwner::Action, placement.action_id)? {
            let edge = read::automation_edge_for_exit(
                conn,
                AutomationPictureOwner::Automation,
                id,
                exit.name.as_deref(),
            )?;
            if let Some(next) = edge.and_then(|e| e.to_id) {
                todo.push(next);
            }
        }
    }
    Ok(seen)
}

/// Whether a way out has something set to happen after it. An edge that goes on to a placement of some
/// other automation — or to none — decides nothing, so it counts as undecided rather than as an edge.
fn decided(
    conn: &Connection,
    placement_id: i64,
    exit_name: Option<&str>,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<bool> {
    let Some(edge) = read::automation_edge_for_exit(
        conn,
        AutomationPictureOwner::Automation,
        placement_id,
        exit_name,
    )?
    else {
        return Ok(false);
    };
    Ok(match edge.to_id {
        Some(to) => by_id.contains_key(&to),
        None => true,
    })
}

/// Whether an action declares a `task_take` output on any of its ways out — the thing that makes a
/// placement of it usable as an entry, since the task it comes out holding is what the run is about
/// from there on.
fn takes_a_task(conn: &Connection, action_id: i64) -> Result<bool> {
    for exit in read::automation_exits_of(conn, AutomationOwner::Action, action_id)? {
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
        AutomationPortOwner::Exit,
        exit.id,
        AutomationPortDirection::Out,
    )?)
}

/// Whether anything actually reaches one input. A wire counts only where **both** halves hold: its far
/// end is declared — that placement's way out really hands on a port of that name — and that placement
/// is reachable from the entry. A wire whose far end was renamed underneath it is parted rather than
/// rewritten ([`crate::ops::automation`]), and a wire from a placement no run reaches would never carry
/// anything, so neither of them feeds an input.
fn fed(
    conn: &Connection,
    placement: &AutomationPlacement,
    port_name: &str,
    live: &BTreeSet<i64>,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<bool> {
    for wire in read::automation_wires_to_port(
        conn,
        AutomationPictureOwner::Automation,
        placement.id,
        port_name,
    )? {
        if !live.contains(&wire.from_id) {
            continue;
        }
        let Some(from) = by_id.get(&wire.from_id) else { continue };
        let exit = read::automation_exit_by_name(
            conn,
            AutomationOwner::Action,
            from.action_id,
            wire.from_exit_name.as_deref(),
        )?;
        let Some(exit) = exit else { continue };
        if outs_of(conn, &exit)?.iter().any(|p| p.name == wire.from_port_name) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// One placement's settings, **declaration and answer together**. Public because the build screen draws
/// the same pair and must not put them back together a second way. The action declares and carries no
/// answer; the placement answers on a row of its own under the same name
/// ([`crate::ops::automation::cfg_set`]), so the two have to be put back together here.
pub fn settings_of(conn: &Connection, placement: &AutomationPlacement) -> Result<Vec<AutomationCfg>> {
    let declared = read::automation_cfgs_of(conn, AutomationCfgOwner::Action, placement.action_id)?;
    let mut out = Vec::with_capacity(declared.len());
    for mut cfg in declared {
        cfg.value = read::automation_cfg_by_name(
            conn,
            AutomationCfgOwner::Placement,
            placement.id,
            &cfg.name,
        )?
        .and_then(|answer| answer.value);
        out.push(cfg);
    }
    Ok(out)
}

/// **Launch an automation**: check it, copy what is placed on it into a run, and start it.
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
/// The run is born `running`: nothing caps how many may be under way at once, so a launch never waits
/// (`AMB-D-947`). `started_at` is the moment of the launch itself.
pub fn launch(tx: &WriteTx<'_>, automation_id: i64, by: &Launcher<'_>) -> Result<AutomationRun> {
    let automation: Automation = read::automation(tx.conn(), automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    if automation.archived {
        return Err(Error::Invalid(
            Msg::new(format!(
                "automation '{}' is archived — bring it back before launching it",
                automation.name
            ))
            .coded(ErrorCode::InvalidAutomationArchived)
            .with("automation", &automation.name),
        ));
    }
    let unmet = check(tx.conn(), automation_id, by.startable, by.models)?;
    if !unmet.is_empty() {
        return Err(not_ready(&automation.name, &unmet));
    }
    // Only where somebody answered. A caller that cannot see the window says nothing rather than
    // `false`, and the run is made — a launch from a terminal is not a claim about what is on screen.
    if by.workspace_open == Some(false) {
        return Err(Error::Invalid(
            Msg::new(
                "the workspace is closed — a run draws its steps in its panes, so open it and launch again",
            )
            .coded(ErrorCode::InvalidAutomationWorkspaceClosed),
        ));
    }
    let now = Timestamp::now();
    let run = AutomationRun {
        id: read::next_id(tx.conn(), "automation_run")?,
        automation_id,
        project_id: automation.project_id,
        status: AutomationRunStatus::Running,
        pause_requested: false,
        stopped_reason: None,
        started_by_kind: by.by,
        started_at: Some(now),
        ended_at: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run(&run))?;
    for placement in read::automation_placements_of(tx.conn(), automation_id)? {
        let def = snapshot(tx, run.id, &placement, now)?;
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
        Msg::new(sentence).coded(ErrorCode::NotReadyAutomation).with("automation", name),
        |msg, one| msg.part(one.msg()),
    );
    Error::NotReady(msg)
}

/// **One placement, as it stands at this moment** — the copy a run reads from then on.
///
/// The ways out, the inputs and the settings are the placement's: the action's declarations with this
/// spot's answers written in. The prompt, the agent, the model and the three flags are the step the
/// action opens, resolved here rather than kept as a pointer — the action would otherwise be read
/// halfway through the run as whatever it had since been edited into.
///
/// One row per placement, which is one row per step while an action opens a single step. Walking the
/// whole column inside an action, and writing a row per step of it, is `AMB-T-5313`'s.
fn snapshot(
    tx: &WriteTx<'_>,
    run_id: i64,
    placement: &AutomationPlacement,
    now: Timestamp,
) -> Result<AutomationRunDef> {
    let conn = tx.conn();
    let action = read::automation_action(conn, placement.action_id)?
        .ok_or_else(|| not_found("action", placement.action_id))?;
    let step = steps_opened_by(conn, placement.action_id)?
        .into_iter()
        .next()
        .ok_or_else(|| Error::invalid(format!("action '{}' has no step to open", action.name)))?;
    let mut exits = Vec::new();
    for exit in read::automation_exits_of(conn, AutomationOwner::Action, action.id)? {
        let outs = outs_of(conn, &exit)?
            .into_iter()
            .map(|p| RunDefPort { name: p.name, kind: p.kind, required: p.required })
            .collect();
        exits.push(RunDefExit { name: exit.name.clone(), outs });
    }
    let ins: Vec<RunDefPort> = read::automation_ports_of(
        conn,
        AutomationPortOwner::Action,
        action.id,
        AutomationPortDirection::In,
    )?
    .into_iter()
    .map(|p| RunDefPort { name: p.name, kind: p.kind, required: p.required })
    .collect();
    let cfg: Vec<RunDefCfg> = settings_of(conn, placement)?
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
        placement_id: Some(placement.id),
        step_id: Some(step.id),
        name: action.name,
        prompt: Some(step.prompt),
        agent: step.agent,
        model: step.model,
        interactive: step.interactive,
        work_dir_ref: step.work_dir_ref,
        report_to_task: step.report_to_task,
        show_history: step.show_history,
        exits: serde_json::to_string(&exits).map_err(Error::from)?,
        ins: serde_json::to_string(&ins).map_err(Error::from)?,
        cfg: serde_json::to_string(&cfg).map_err(Error::from)?,
        created_at: now,
        updated_at: now,
    })
}

/// **The spot a run starts at** — the copy taken at launch of the automation's entry placement, or
/// `None` where the automation has since lost its entry or the run carries no copy of it.
///
/// It is read off the live definition's `entry_placement_id` and matched against the copies by
/// `placement_id`, because the copy itself does not say which of them the entry was: the picture is
/// walked from the entry along the edges, and a run that is under way has already walked past it.
pub fn entry_def(conn: &Connection, run_id: i64) -> Result<Option<AutomationRunDef>> {
    let Some(run) = read::automation_run(conn, run_id)? else {
        return Err(not_found("run", run_id));
    };
    let Some(entry) = read::automation(conn, run.automation_id)?.and_then(|a| a.entry_placement_id)
    else {
        return Ok(None);
    };
    Ok(read::automation_run_defs_of(conn, run_id)?
        .into_iter()
        .find(|def| def.placement_id == Some(entry)))
}

/// **What a run is waiting for**, in the three shapes a watcher has to tell apart.
///
/// The two that are not a step were one answer once, and a watcher that cannot tell them apart reads
/// a run it must leave alone and a run nobody will ever move again as the same thing — so the second
/// sits `running` for good, holding a task nobody is working.
#[derive(Debug, Clone)]
pub enum Waiting {
    /// This step is waiting to be opened. Boxed because the other two carry nothing, and a copy of a
    /// step is the whole of what one is.
    Step(Box<AutomationRunDef>),
    /// Nothing for anybody to do: a step is under way, or the run is not running at all.
    Nothing,
    /// **The run cannot go on.** Nothing is under way and nothing leads anywhere — the picture it was
    /// copied from has been changed under it, or the step it would start at is no longer in it. It
    /// will never move again on its own, so it is ended rather than looked at every second.
    ///
    /// **Nobody can walk a run into this on purpose**, which is why it is held by the tests below and
    /// by no scenario (`AMB-T-5307`). Every deliberate edit under a run is answered at the door it
    /// passes through: a report reads the picture live and stops the run there
    /// ([`crate::ops::automation_report::done`]), and so does picking a paused run up again
    /// ([`crate::ops::automation_stop::resume`]). What is left over is the second between a run
    /// becoming one with nothing open — launched, reported, resumed — and the watch's next look at
    /// it. An edit landing inside that second is the whole of what this answers, and a second is not
    /// something a hand can aim at.
    NoWayOn,
}

/// **The step this run is waiting to have opened**, or why there is none ([`Waiting`]).
///
/// A run that is `running` is either carrying a step out or standing between two of them, and only the
/// second is anybody's to act on. So this answers a step in exactly two cases — a run that has just
/// been launched and has no execution yet, where the answer is the entry ([`entry_def`]); and a run
/// whose last execution has reported, where the answer is read off the way out it took.
///
/// **It derives rather than remembers**, because the report already wrote down everything it takes:
/// the execution carries the way out, and the picture says what follows one
/// ([`crate::ops::automation_report::done`] resolved the same edge to decide whether the run goes on
/// at all). A second copy kept for the watcher's benefit would be a second thing to keep true.
///
/// **Deriving is also what puts a run in the way of `NoWayOn`.** A definition goes on being edited
/// while runs of it are out: a way out loses its edge, an entry is taken off, a step is deleted. The
/// report that walked the same edge a moment earlier found one; the read after it does not, and the
/// run standing between two steps has nowhere to stand towards.
pub fn next_def(conn: &Connection, run_id: i64) -> Result<Waiting> {
    let Some(run) = read::automation_run(conn, run_id)? else {
        return Err(not_found("run", run_id));
    };
    if run.status != AutomationRunStatus::Running {
        return Ok(Waiting::Nothing);
    }
    let Some(last) = read::automation_run_steps_of(conn, run_id)?.pop() else {
        // Nothing has run yet, so what is waiting to be opened is where the run starts. A run that
        // carries no copy of its entry cannot start at all: the entry was taken off the definition
        // between the launch and this look.
        return Ok(entry_def(conn, run_id)?
            .map_or(Waiting::NoWayOn, |def| Waiting::Step(Box::new(def))));
    };
    if last.status == AutomationRunStepStatus::Running {
        return Ok(Waiting::Nothing);
    }
    // Everything below is a run standing between two steps with nothing to stand towards. A way out
    // that closed or halted the run would have done so in the report that took it, so a run still
    // `running` here is one whose picture stopped leading anywhere after it had already left.
    let Some(from) = read::automation_run_def(conn, last.run_def_id)?.and_then(|def| def.placement_id)
    else {
        return Ok(Waiting::NoWayOn);
    };
    let Some(edge) = read::automation_edge_for_exit(
        conn,
        AutomationPictureOwner::Automation,
        from,
        last.exit_name.as_deref(),
    )?
    else {
        return Ok(Waiting::NoWayOn);
    };
    let Some(to) = edge.to_id.filter(|_| edge.ends == AutomationEnds::Go) else {
        return Ok(Waiting::NoWayOn);
    };
    Ok(read::automation_run_defs_of(conn, run_id)?
        .into_iter()
        .find(|def| def.placement_id == Some(to))
        .map_or(Waiting::NoWayOn, |def| Waiting::Step(Box::new(def))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AutomationAction, AutomationEdge, AutomationPlacement};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewStep};
    use crate::ops::test_support::{mk_placed, mk_project, only_step, with_tx};

    fn mk_automation(tx: &WriteTx<'_>, name: &str) -> Automation {
        let project = mk_project(tx, "amenbo");
        automation::add(tx, project, NewAutomation { name: name.into(), ..Default::default() })
            .expect("add automation")
    }

    /// What every test here starts from: one automation, one action of one step that takes a task and
    /// closes the run, placed once, and every way out of it decided. It launches as it stands, so each
    /// test can take one thing back off and watch the check find it.
    fn launchable(tx: &WriteTx<'_>) -> (Automation, AutomationAction, AutomationPlacement) {
        let automation = mk_automation(tx, "1件やりきる");
        let (action, placement) = mk_placed(tx, &automation, "取る", "take one", "claude");
        takes_task_on(tx, action.id, None);
        automation::edge_add(
            tx,
            AutomationPictureOwner::Automation,
            placement.id,
            None,
            EdgeTarget::Done,
            None,
        )
        .expect("edge");
        // Nothing is written for the error way out: it is carried from birth and halts unless
        // somebody says otherwise, which is what `an_error_way_out_nobody_answered_for_is_not_open`
        // holds this to.
        let automation =
            automation::set_entry(tx, automation.id, Some(placement.id)).expect("entry");
        (automation, action, placement)
    }

    /// Declare a `task_take` output on one way out of an action — what makes a placement of it usable
    /// as an entry.
    fn takes_task_on(tx: &WriteTx<'_>, action_id: i64, exit_name: Option<&str>) {
        let exit =
            read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, action_id, exit_name)
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

    /// Name a model on the one step an action holds.
    fn names_model(tx: &WriteTx<'_>, action: &AutomationAction, model: &str) {
        let step = only_step(tx, action);
        automation::step_update(
            tx,
            step.id,
            None,
            None,
            None,
            Some(Some(model)),
            None,
            None,
            None,
            None,
        )
        .expect("name a model");
    }

    /// The machine every test launches on: one that can start `claude`, with a window
    /// open.
    fn here<'a>(startable: &'a [String]) -> Launcher<'a> {
        Launcher {
            startable: Some(startable),
            models: nothing_asked(),
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
            let (automation, _, _) = launchable(tx);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
            );
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            assert_eq!(run.status, AutomationRunStatus::Running);
            assert!(run.started_at.is_some(), "a launch starts on the spot");
            assert_eq!(run.project_id, automation.project_id);
        });
    }

    #[test]
    fn an_automation_with_nothing_placed_on_it_says_only_that() {
        with_tx(|tx| {
            let automation = mk_automation(tx, "空");
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
            let (automation, _, _) = launchable(tx);
            automation::set_entry(tx, automation.id, None).expect("clear entry");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::NoEntry],
                "with no entry nothing is reachable, so every other check is asked of nothing",
            );
        });
    }

    /// **An action with nothing to open is refused** (`AMB-D-949`). A placement of it is drawn on the
    /// picture like any other, and a run reaching it would have no terminal to put up.
    #[test]
    fn a_placement_standing_on_an_action_with_no_step_is_refused() {
        with_tx(|tx| {
            let (automation, action, _) = launchable(tx);
            automation::action_set_entry(tx, action.id, None).expect("take the entry off");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::ActionEmpty { action: "取る".into() }],
            );
        });
    }

    #[test]
    fn an_entry_that_takes_no_task_is_refused() {
        with_tx(|tx| {
            let automation = mk_automation(tx, "1件やりきる");
            let (_, placement) = mk_placed(tx, &automation, "取る", "take one", "claude");
            automation::edge_add(
                tx,
                AutomationPictureOwner::Automation,
                placement.id,
                None,
                EdgeTarget::Done,
                None,
            )
            .expect("edge");
            automation::set_entry(tx, automation.id, Some(placement.id)).expect("entry");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::EntryTakesNoTask { step: "取る".into() }],
            );
        });
    }

    #[test]
    fn a_way_out_with_nothing_after_it_is_refused() {
        with_tx(|tx| {
            let (automation, action, _) = launchable(tx);
            let exit =
                automation::exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある"))
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
            let (automation, _, _) = launchable(tx);
            let unmet = check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check");
            assert_eq!(unmet, vec![], "the error way out is carried from birth, not written");
        });
    }

    #[test]
    fn a_required_input_nothing_reaches_is_refused() {
        with_tx(|tx| {
            let (automation, action, _) = launchable(tx);
            automation::port_add(
                tx,
                AutomationPortOwner::Action,
                action.id,
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
    fn a_wire_from_a_placement_no_run_reaches_does_not_feed_an_input() {
        with_tx(|tx| {
            let (automation, entry_action, entry) = launchable(tx);
            // A second placement, wired into the entry's input but reached by nothing: the run would
            // walk straight past it, so what it hands on never arrives.
            let (orphan_action, orphan) = mk_placed(tx, &automation, "書く", "write it", "claude");
            let exit = read::automation_exit_by_name(
                tx.conn(),
                AutomationOwner::Action,
                orphan_action.id,
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
                AutomationPortOwner::Action,
                entry_action.id,
                AutomationPortDirection::In,
                "下書き",
                AutomationPortKind::Value,
                true,
            )
            .expect("in");
            automation::wire_add(
                tx,
                AutomationPictureOwner::Automation,
                orphan.id,
                None,
                "下書き",
                entry.id,
                "下書き",
            )
            .expect("wire");
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
            let (automation, action, placement) = launchable(tx);
            automation::cfg_add(
                tx,
                action.id,
                "作業フォルダ",
                crate::model::AutomationCfgKind::Folder,
                true,
                None,
            )
            .expect("cfg");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnansweredCfg { step: "取る".into(), cfg: "作業フォルダ".into() }],
                "the declaration is the action's and the answer is the placement's",
            );
            automation::cfg_set(tx, placement.id, "作業フォルダ", Some("\"~/work\"")).expect("answer");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"), vec![]);
        });
    }

    #[test]
    fn an_agent_this_machine_cannot_start_is_refused_and_an_unasked_machine_is_not() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
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

    /// **A second step inside an action**, put in on the line its first step leaves the action by — so
    /// the one it starts at goes on to this one, and this one is what leaves the action from here.
    fn goes_on_to(tx: &WriteTx<'_>, action: &AutomationAction, name: &str, agent: &str) {
        let entry = only_step(tx, action);
        let leaves_by =
            read::automation_edge_for_exit(tx.conn(), AutomationPictureOwner::Action, entry.id, None)
                .expect("read the line out of the action")
                .expect("the step an action is written with leaves it by its unnamed way out");
        automation::step_insert(tx, leaves_by.id, NewStep::new(name, "続ける", agent), &[], &[])
            .expect("the second step");
    }

    #[test]
    fn a_step_further_inside_an_action_is_asked_for_its_agent_too() {
        with_tx(|tx| {
            let (automation, action, _) = launchable(tx);
            goes_on_to(tx, &action, "書く", "codex");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::AgentMissing { step: "書く".into(), agent: "codex".into() }],
                "the check walks the picture inside the action, not its entry alone",
            );
        });
    }

    #[test]
    fn a_step_inside_an_action_that_nothing_leads_to_is_left_out() {
        with_tx(|tx| {
            let (automation, action, _) = launchable(tx);
            automation::step_add(tx, action.id, NewStep::new("書く", "続ける", "codex"))
                .expect("a step with no line into it");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
                "no pane comes up on it, so the agent it names cannot hold the launch back",
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
            let (automation, action, _) = launchable(tx);
            names_model(tx, &action, "opus-9");
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
            let (automation, action, _) = launchable(tx);
            names_model(tx, &action, "opus-9");
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
            let (automation, _, _) = launchable(tx);
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
            let (automation, _, _) = launchable(tx);
            let startable = claude();
            let closed = Launcher { workspace_open: Some(false), ..here(&startable) };
            assert!(launch(tx, automation.id, &closed).is_err());
            automation::update(tx, automation.id, None, None, Some(true)).expect("archive");
            assert!(launch(tx, automation.id, &here(&claude())).is_err());
        });
    }

    #[test]
    fn the_refusal_carries_one_part_per_reason() {
        with_tx(|tx| {
            let (automation, action, _) = launchable(tx);
            automation::exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある"))
                .expect("exit");
            let err = launch(tx, automation.id, &here(&[])).expect_err("refused");
            let Error::NotReady(msg) = err else { panic!("a launch that cannot go ahead is not_ready") };
            assert_eq!(msg.parts().len(), 2, "one open way out and one agent this machine has not");
            // Every sentence names itself, so a screen writes the whole refusal in the reader's own
            // language rather than the outer line in theirs and the reasons in English (`AMB-D-413`).
            assert_eq!(msg.code(), Some(ErrorCode::NotReadyAutomation));
            assert_eq!(
                msg.parts().iter().map(|p| p.code()).collect::<Vec<_>>(),
                vec![
                    Some(ErrorCode::NotReadyAutomationOpenExit),
                    Some(ErrorCode::NotReadyAutomationAgentMissing),
                ],
            );
            // And carries the values those sentences are built from, under the names the templates
            // interpolate them by — a part with a hole where the step's name goes reads as `{step}`.
            let named: Vec<&str> = msg.parts()[1].fields().iter().map(|(key, _)| key).collect();
            assert_eq!(named, vec!["step", "agent"]);
        });
    }

    #[test]
    fn the_two_refusals_that_stand_alone_name_themselves_too() {
        with_tx(|tx| {
            let startable = claude();
            let (automation, _, _) = launchable(tx);
            automation::update(tx, automation.id, None, None, Some(true)).expect("archive");
            let err = launch(tx, automation.id, &here(&startable)).expect_err("archived");
            let Error::Invalid(msg) = err else { panic!("an archived automation is invalid") };
            assert_eq!(msg.code(), Some(ErrorCode::InvalidAutomationArchived));
            assert_eq!(
                msg.fields().iter().map(|(key, _)| key).collect::<Vec<_>>(),
                vec!["automation"],
            );

            automation::update(tx, automation.id, None, None, Some(false)).expect("bring back");
            let closed = Launcher { workspace_open: Some(false), ..here(&startable) };
            let err = launch(tx, automation.id, &closed).expect_err("closed");
            let Error::Invalid(msg) = err else { panic!("a closed workspace is invalid") };
            assert_eq!(msg.code(), Some(ErrorCode::InvalidAutomationWorkspaceClosed));
        });
    }

    #[test]
    fn what_is_placed_is_copied_into_the_run_and_stops_following_the_definition() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            let defs = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert_eq!(defs.len(), 1);
            assert_eq!(defs[0].name, "取る");
            assert_eq!(defs[0].prompt.as_deref(), Some("take one"));
            assert_eq!(defs[0].placement_id, Some(placement.id));
            assert_eq!(defs[0].step_id, Some(step.id));
            let exits: Vec<RunDefExit> = serde_json::from_str(&defs[0].exits).expect("exits");
            assert_eq!(exits.len(), 2, "the unnamed way out and the error one");
            assert_eq!(exits[0].outs[0].kind, AutomationPortKind::TaskTake);

            automation::action_update(tx, action.id, Some("取り直す"))
                .expect("edit the definition under the run");
            automation::step_update(
                tx,
                step.id,
                None,
                Some("take another"),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("rewrite the prompt under the run");
            let defs = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert_eq!(defs[0].name, "取る", "the copy is what the run reads from here on");
            assert_eq!(defs[0].prompt.as_deref(), Some("take one"));
        });
    }

    /// What a run is waiting to have opened, at each of the three moments there is an answer to it.
    ///
    /// **It is derived and not remembered**, which is what this holds: the report writes the way out
    /// down, and the picture says what follows one — so a watcher asking later reads the same answer
    /// the report acted on, without a second copy being kept for it (`AMB-D-945`).
    #[test]
    fn what_a_run_is_waiting_to_have_opened_is_read_off_what_it_has_already_done() {
        with_tx(|tx| {
            let (automation, _, placement) = launchable(tx);
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");

            // Nothing has run yet, so what is waiting is where the run starts.
            let Waiting::Step(first) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the entry is what a fresh run waits for")
            };
            assert_eq!(first.placement_id, Some(placement.id));

            // A step under way is nobody's to open a second time.
            let opening = match crate::ops::automation_step::open(tx, run.id, first.id, None)
                .expect("open")
            {
                crate::ops::automation_step::Opened::Ready(ready) => *ready,
                crate::ops::automation_step::Opened::Stopped { missing, .. } => {
                    panic!("stopped for {missing:?}")
                }
                crate::ops::automation_step::Opened::NoAgent { agent, .. } => {
                    panic!("cannot start {agent}")
                }
            };
            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Nothing));

            // The entry hands a task on through that way out, so the task is taken before it can
            // report — the refusal that guards a step saying it is done with nothing to show.
            let task = crate::ops::test_support::mk_task_in(tx, "一件", Some(automation.project_id));
            crate::ops::automation_report::take(tx, opening.run_step.id, task).expect("take");

            // And once it has reported, the answer is read off the way out it took — here the unnamed
            // one, which closes the run, so there is nothing waiting and the run is no longer running.
            crate::ops::automation_report::done(tx, opening.run_step.id, None, "did it")
                .expect("report");
            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Nothing));
            assert_eq!(
                read::automation_run(tx.conn(), run.id).expect("read").expect("the run").status,
                AutomationRunStatus::Done,
            );
        });
    }

    /// **A run with nowhere to go says so**, rather than reading as a run somebody is about to move.
    ///
    /// The two are one answer to look at — nothing is open either way — and telling them apart is the
    /// whole of why there are three. A definition goes on being edited while runs of it are out, so a
    /// run that has lost the spot it would start at is a shape that happens rather than one that
    /// cannot; left as "nothing to do" it holds its task for the rest of the session.
    #[test]
    fn a_run_that_has_lost_the_spot_it_would_start_at_says_it_cannot_go_on() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Step(_)));

            // Taken off the definition while the run is out. The run's own copies are still there —
            // what it has lost is the one saying where to start.
            crate::ops::automation::set_entry(tx, automation.id, None).expect("entry off");

            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::NoWayOn));
            assert_eq!(
                read::automation_run(tx.conn(), run.id).expect("read").expect("the run").status,
                AutomationRunStatus::Running,
                "reading it says nothing about it — ending it is the watch's",
            );
        });
    }

    /// A picture with somewhere to stand towards: the entry takes a task and leaves through its
    /// unnamed way out into a second placement, which closes the run. What the tests about a run
    /// standing between two spots start from — [`launchable`]'s single placement closes the run on the
    /// spot and never stands anywhere.
    fn two_spots(tx: &WriteTx<'_>) -> (Automation, AutomationPlacement, AutomationEdge) {
        let automation = mk_automation(tx, "取って読む");
        let (first_action, first) = mk_placed(tx, &automation, "取る", "take one", "claude");
        takes_task_on(tx, first_action.id, None);
        let (_, second) = mk_placed(tx, &automation, "読む", "read it back", "claude");
        automation::edge_add(
            tx,
            AutomationPictureOwner::Automation,
            second.id,
            None,
            EdgeTarget::Done,
            None,
        )
        .expect("edge");
        let onward = automation::edge_add(
            tx,
            AutomationPictureOwner::Automation,
            first.id,
            None,
            EdgeTarget::Go(second.id),
            None,
        )
        .expect("edge");
        automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        (automation, first, onward)
    }

    /// Walk that picture as far as the gap between its two spots: the entry opened, a task taken, and
    /// a report that left through the way out leading on. The run is left `running` with nothing
    /// open, which is the one state [`Waiting`]'s three answers are told apart in.
    fn standing_between(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let run = launch(tx, automation.id, &here(&claude())).expect("launch");
        let Waiting::Step(entry) = next_def(tx.conn(), run.id).expect("next") else {
            panic!("the entry is what a fresh run waits for")
        };
        let opening = match crate::ops::automation_step::open(tx, run.id, entry.id, None).expect("open") {
            crate::ops::automation_step::Opened::Ready(ready) => *ready,
            crate::ops::automation_step::Opened::Stopped { missing, .. } => {
                panic!("stopped for {missing:?}")
            }
            crate::ops::automation_step::Opened::NoAgent { agent, .. } => {
                panic!("cannot start {agent}")
            }
        };
        let task = crate::ops::test_support::mk_task_in(tx, "一件", Some(automation.project_id));
        crate::ops::automation_report::take(tx, opening.run_step.id, task).expect("take");
        crate::ops::automation_report::done(tx, opening.run_step.id, None, "did it")
            .expect("report");
        assert!(
            matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Step(_)),
            "with the picture as it stands, the second spot is what it waits for",
        );
        read::automation_run(tx.conn(), run.id).expect("read").expect("the run")
    }

    /// **The spot it left from is no longer in the picture.** The run's copy of it is still there —
    /// that is what a copy is for — but the copy no longer names a live placement, and a way out is
    /// read off the live picture.
    #[test]
    fn a_run_whose_spot_was_taken_out_from_under_it_says_it_cannot_go_on() {
        with_tx(|tx| {
            let (automation, first, _) = two_spots(tx);
            let run = standing_between(tx, &automation);

            automation::placement_delete(tx, first.id).expect("take the placement off");

            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::NoWayOn));
            assert_eq!(
                read::automation_run(tx.conn(), run.id).expect("read").expect("the run").status,
                AutomationRunStatus::Running,
                "reading it says nothing about it — ending it is the watch's",
            );
        });
    }

    /// **The way out it left through decides nothing now.** The edge is gone, so the picture has no
    /// answer for a run that has already taken it.
    #[test]
    fn a_run_whose_way_out_lost_its_edge_says_it_cannot_go_on() {
        with_tx(|tx| {
            let (automation, _, onward) = two_spots(tx);
            let run = standing_between(tx, &automation);

            automation::edge_delete(tx, onward.id).expect("delete the edge");

            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::NoWayOn));
        });
    }

    /// **The way out ends the run now instead of leading on.** Nothing is opened for an edge that
    /// closes or stops: the run that has already left through it is standing towards an ending it
    /// cannot reach by itself.
    #[test]
    fn a_run_whose_way_out_now_ends_the_run_says_it_cannot_go_on() {
        with_tx(|tx| {
            let (automation, _, onward) = two_spots(tx);
            let run = standing_between(tx, &automation);

            automation::edge_update(tx, onward.id, Some(EdgeTarget::Halt), None)
                .expect("point it at an ending");

            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::NoWayOn));
        });
    }

    /// **It leads to a spot the run never copied down.** A placement added after the launch is not in
    /// the run's own copies, and a run reads its copies rather than the live picture — so an edge
    /// pointed at one leads nowhere this run can go.
    #[test]
    fn a_run_sent_to_a_spot_added_after_it_launched_says_it_cannot_go_on() {
        with_tx(|tx| {
            let (automation, _, onward) = two_spots(tx);
            let run = standing_between(tx, &automation);

            let (_, late) = mk_placed(tx, &automation, "直す", "fix it", "claude");
            automation::edge_update(tx, onward.id, Some(EdgeTarget::Go(late.id)), None)
                .expect("point it at the new placement");

            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::NoWayOn));
        });
    }

    /// **A second launch does not wait for the first** (`AMB-D-947`). Nothing caps how many runs may be
    /// under way, so both are `running` from the moment they are made and both carry a `started_at`.
    #[test]
    fn a_second_launch_starts_beside_the_first_rather_than_behind_it() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let startable = claude();
            let first = launch(tx, automation.id, &here(&startable)).expect("launch");
            let second = launch(tx, automation.id, &here(&startable)).expect("launch");

            for run in [&first, &second] {
                assert_eq!(run.status, AutomationRunStatus::Running);
                assert!(run.started_at.is_some(), "a launch starts on the spot");
            }
            assert_eq!(
                read::automation_run_ids_running(tx.conn()).expect("running").len(),
                2,
                "both are going at once",
            );
        });
    }
}
