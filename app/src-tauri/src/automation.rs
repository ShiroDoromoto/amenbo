//! **Reading an automation's definition, and saying whether it could be started** — the doors behind
//! the automations screen and its build screen.
//!
//! Core owns the ten definition tables and the writes that build them
//! ([`amenbo_core::ops::automation`]); nothing is built here. What this side does is resolve and
//! shape: a step reads its ways out, its inputs and its settings from the library action it points at
//! or from itself, and a screen that had to know which of the two declared a name would be drawing
//! the storage rather than the automation.
//!
//! **The launch check is core's, and is asked here rather than repeated**
//! ([`amenbo_core::ops::automation_run::check`]). It was written twice once, and the two lists
//! disagreed on five points — so a screen could say an automation was ready and the press then refuse
//! it (`AMB-T-5272`). What this door does is ask that one list and name each answer in a shape a
//! screen can draw beside the step it is about.
//!
//! **What this machine can start is passed in rather than asked here.** Whether an agent is
//! installed is a question about a login shell and the reader's own profile, and the face already
//! holds the answer it asked for the empty frame ([`crate::wake`]). Nothing having been asked is
//! `null` and not an empty list: an unanswered probe drawn as an answer would tell a reader with
//! four agents installed that they have none (`AMB-D-792`).

use amenbo_core::model::{
    Automation, AutomationCfg, AutomationExit, AutomationPort, AutomationPortDirection,
    AutomationPortOwner, AutomationStep,
};
use amenbo_core::ops::automation::declarer;
use amenbo_core::ops::automation_run::{self, Unmet};
use amenbo_core::store_engine::{read, StoreEngine};

use crate::commands::open_store_read;
use crate::dto::{
    AutomationCardDto, AutomationCfgDto, AutomationDetailDto, AutomationEdgeDto, AutomationExitDto,
    AutomationLaunchBlockDto, AutomationLaunchCheckDto, AutomationPortDto, AutomationStepDto,
    AutomationWireDto,
};
use crate::error::CmdError;

/// The automations of one project, in the order they were placed in.
///
/// Archived ones come too: what an archived automation is, is one that is kept out of the way rather
/// than gone, and which of the two lists shows it is the screen's to decide.
#[tauri::command]
pub fn automation_page(project_id: i64) -> Result<Vec<AutomationCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_page");
    let store = open_store_read()?;
    let engine = store.read_model();
    let conn = engine.conn();
    let mut cards = Vec::new();
    for (id, _) in read::automation_siblings(conn, project_id, None)? {
        let Some(row) = read::automation(conn, id)? else { continue };
        cards.push(AutomationCardDto {
            id: row.id,
            name: row.name,
            steps: read::automation_step_ids(conn, id)?.len(),
            archived: row.archived,
        });
    }
    Ok(cards)
}

/// One automation's whole definition, or nothing where that id names none.
#[tauri::command]
pub fn automation_detail(id: i64) -> Result<Option<AutomationDetailDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_detail");
    let store = open_store_read()?;
    let engine = store.read_model();
    let Some(row) = read::automation(engine.conn(), id)? else { return Ok(None) };
    Ok(Some(detail(engine, row)?))
}

/// Whether this automation could be started, and what is in the way — core's launch check, named for
/// a screen ([`amenbo_core::ops::automation_run::check`]).
///
/// `agents` is what this machine can start ([`crate::wake::wake_choices`]). `null` is "the face could
/// not ask", and then no step is judged on its agent: a reason drawn off an answer nobody got would
/// tell a reader to install what they already have.
///
/// **The workspace is not asked about here.** A closed one refuses the launch rather than the
/// definition, and it stops being true the moment a window opens — so it belongs to the press
/// ([`amenbo_core::ops::automation_run::launch`]) and not to the list a build screen draws.
#[tauri::command]
pub fn automation_launch_check(
    id: i64,
    agents: Option<Vec<String>>,
) -> Result<AutomationLaunchCheckDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_launch_check");
    let store = open_store_read()?;
    let unmet = automation_run::check(store.read_model().conn(), id, agents.as_deref())?;
    Ok(AutomationLaunchCheckDto {
        ready: unmet.is_empty(),
        blocks: unmet.iter().map(block_dto).collect(),
    })
}

/// One of core's reasons, as a screen draws it: what it is, which step it is about, and what on that
/// step. The words are the front end's — core's own English is what a surface holding no dictionary
/// falls back to ([`amenbo_core::ops::automation_run::Unmet::say`]).
fn block_dto(unmet: &Unmet) -> AutomationLaunchBlockDto {
    let (reason, step, at) = match unmet {
        Unmet::NoSteps => ("no_steps", None, None),
        Unmet::NoEntry => ("no_entry", None, None),
        Unmet::EntryTakesNoTask { step } => ("entry_takes_no_task", Some(step), None),
        Unmet::OpenExit { step, exit } => ("open_exit", Some(step), exit.as_ref()),
        Unmet::UnwiredInput { step, port } => ("unwired_input", Some(step), Some(port)),
        Unmet::UnansweredCfg { step, cfg } => ("unanswered_cfg", Some(step), Some(cfg)),
        Unmet::AgentMissing { step, agent } => ("agent_missing", Some(step), Some(agent)),
    };
    AutomationLaunchBlockDto { reason, step_name: step.cloned(), at: at.cloned() }
}

// ───────────────────────────── shaping ─────────────────────────────

/// One automation's ten tables, read and resolved into the one shape every part of the build screen
/// works from.
fn detail(engine: &StoreEngine, row: Automation) -> Result<AutomationDetailDto, CmdError> {
    let conn = engine.conn();
    let automation_id = row.id;
    let mut steps = Vec::new();
    for (step_id, _) in read::automation_step_siblings(conn, automation_id, None)? {
        let Some(step) = read::automation_step(conn, step_id)? else { continue };
        steps.push(step_dto(engine, step)?);
    }
    let mut edges = Vec::new();
    for edge_id in read::automation_edge_ids(conn, automation_id)? {
        let Some(edge) = read::automation_edge(conn, edge_id)? else { continue };
        edges.push(AutomationEdgeDto {
            id: edge.id,
            from_step_id: edge.from_step_id,
            exit_name: edge.exit_name,
            to_step_id: edge.to_step_id,
            ends: edge.ends.as_str(),
            max_times: edge.max_times,
        });
    }
    let mut wires = Vec::new();
    for wire_id in read::automation_wire_ids(conn, automation_id)? {
        let Some(wire) = read::automation_wire(conn, wire_id)? else { continue };
        wires.push(AutomationWireDto {
            id: wire.id,
            from_step_id: wire.from_step_id,
            from_exit_name: wire.from_exit_name,
            from_port_name: wire.from_port_name,
            to_step_id: wire.to_step_id,
            to_port_name: wire.to_port_name,
        });
    }
    Ok(AutomationDetailDto {
        id: row.id,
        project_id: row.project_id,
        name: row.name,
        notes: row.notes,
        preamble: row.preamble,
        entry_step_id: row.entry_step_id,
        archived: row.archived,
        steps,
        edges,
        wires,
    })
}

/// One step, with the action it points at read in: the prompt it runs on, the ways out it can leave
/// by, what it takes and what it is set to.
fn step_dto(engine: &StoreEngine, step: AutomationStep) -> Result<AutomationStepDto, CmdError> {
    let conn = engine.conn();
    let action = match step.action_id {
        Some(action_id) => read::automation_action(conn, action_id)?,
        None => None,
    };
    let (owner_kind, owner_id) = declarer(&step);
    let port_owner = match owner_kind {
        amenbo_core::model::AutomationOwner::Step => AutomationPortOwner::Step,
        amenbo_core::model::AutomationOwner::Action => AutomationPortOwner::Action,
    };

    let mut exits = Vec::new();
    for exit in read::automation_exits_of(conn, owner_kind, owner_id)? {
        exits.push(exit_dto(engine, exit)?);
    }
    let inputs = read::automation_ports_of(conn, port_owner, owner_id, AutomationPortDirection::In)?
        .into_iter()
        .map(port_dto)
        .collect();

    // An action declares and the step answers, and core is what puts the two rows back together —
    // the same pair the launch check reads, rather than a second reading of it.
    let settings = automation_run::settings_of(conn, &step)?.into_iter().map(cfg_dto).collect();

    Ok(AutomationStepDto {
        id: step.id,
        name: step.name,
        action_id: step.action_id,
        action_name: action.as_ref().map(|one| one.name.clone()),
        prompt: step
            .prompt
            .clone()
            .or_else(|| action.as_ref().map(|one| one.prompt.clone()))
            .unwrap_or_default(),
        agent: step.agent,
        model: step.model,
        interactive: step.interactive,
        work_dir_ref: step.work_dir_ref,
        report_to_task: step.report_to_task,
        show_history: step.show_history,
        exits,
        inputs,
        settings,
    })
}

fn exit_dto(engine: &StoreEngine, exit: AutomationExit) -> Result<AutomationExitDto, CmdError> {
    let conn = engine.conn();
    let outputs =
        read::automation_ports_of(conn, AutomationPortOwner::Exit, exit.id, AutomationPortDirection::Out)?
            .into_iter()
            .map(port_dto)
            .collect();
    Ok(AutomationExitDto { id: exit.id, name: exit.name, outputs })
}

fn port_dto(port: AutomationPort) -> AutomationPortDto {
    AutomationPortDto { name: port.name, kind: port.kind.as_str(), required: port.required }
}

fn cfg_dto(cfg: AutomationCfg) -> AutomationCfgDto {
    AutomationCfgDto {
        name: cfg.name,
        kind: cfg.kind.as_str(),
        required: cfg.required,
        options: cfg.options,
        value: cfg.value,
    }
}
