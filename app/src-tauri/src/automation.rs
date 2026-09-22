//! **Reading an automation's definition, saying whether it could be started, and opening a step of a
//! run** — the doors behind the automations screen, its build screen, and the pane a run is drawn in.
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
//!
//! **Which models it can start on is read here rather than passed in**, because that answer is kept on
//! this side ([`crate::agent_models`]) and the face never holds it. Read and never asked: a provider
//! nobody has put the question to says nothing, which leaves that step's model unjudged — the same
//! silence, drawn from the same rule.
//!
//! **A step that is ready to run is told to the window rather than answered back**, as an event.
//! The press that starts a run is on the ledger and the pane it opens is in the workspace — the same
//! window in one shape of the app and the other window in the other (`AMB-D-753`) — so an answer
//! handed back to whoever pressed would reach a screen with no pane to stand it in.

use amenbo_core::model::{
    Automation, AutomationCfg, AutomationExit, AutomationPort, AutomationPortDirection,
    AutomationPortOwner, AutomationRunStatus, AutomationStep, AutomationStoppedReason,
};
use amenbo_core::ops::automation::declarer;
use amenbo_core::ops::automation_run::{self, Unmet};
use amenbo_core::ops::automation_step::Opened;
use amenbo_core::store_engine::{read, StoreEngine};

use crate::commands::{open_store_read, with_store_mut};
use crate::dto::{
    AutomationActionCardDto, AutomationCardDto, AutomationCfgDto, AutomationDetailDto,
    AutomationEdgeDto, AutomationExitDto, AutomationLaunchBlockDto, AutomationLaunchCheckDto,
    AutomationPortDto, AutomationRunTaskDto, AutomationStepDto, AutomationStepOpenDto,
    AutomationStepRunDto, AutomationWireDto, WriteAck,
};
use crate::error::CmdError;
use std::collections::BTreeSet;
use tauri::Emitter;

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

/// **The library this project reaches** — the device's own actions first, then the project's own.
///
/// The two libraries answer as one list because they are one list on screen: what a reader is
/// choosing between is every prompt a step here could be pointed at, and which of the two holds one
/// is a column of that list rather than a second list to go and look in.
#[tauri::command]
pub fn automation_action_page(project_id: i64) -> Result<Vec<AutomationActionCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_action_page");
    let store = open_store_read()?;
    let engine = store.read_model();
    let conn = engine.conn();
    let mut cards = Vec::new();
    for (reach, global) in [(None, true), (Some(project_id), false)] {
        for (id, _) in read::automation_action_siblings(conn, reach, None)? {
            let Some(row) = read::automation_action(conn, id)? else { continue };
            cards.push(AutomationActionCardDto {
                id: row.id,
                name: row.name,
                prompt: row.prompt,
                global,
                used_by: automations_using(engine, id)?,
            });
        }
    }
    Ok(cards)
}

/// **Rename a library action, or rewrite its prompt.** Only what is `Some` is written.
///
/// The rewrite reaches every step pointing at this action, which is what the library is for — and
/// what the screen says before the box is opened. It does not reach a run already under way: a run
/// resolves each step's prompt as it opens the step, off the action as it stands at that moment
/// ([`amenbo_core::ops::automation_run`]), so what a running step carries is settled and this cannot
/// reach back into it.
#[tauri::command]
pub fn automation_action_edit(
    id: i64,
    name: Option<String>,
    prompt: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_update(id, name.as_deref(), prompt.as_deref())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automationActions"]))
}

/// **How many automations run this action**, counted by the automation each step is in.
///
/// Two steps of one automation pointing at the same action is one automation: what the number is
/// read for is how far a rewrite of the prompt carries, and that is measured in automations whose
/// runs change, not in places the pointer occurs.
fn automations_using(engine: &StoreEngine, action_id: i64) -> Result<usize, CmdError> {
    let conn = engine.conn();
    let mut seen = BTreeSet::new();
    for step_id in read::automation_step_ids_using_action(conn, action_id)? {
        let Some(step) = read::automation_step(conn, step_id)? else { continue };
        seen.insert(step.automation_id);
    }
    Ok(seen.len())
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
/// **The models are not passed in — they are read here**, off what each provider has already been asked
/// ([`crate::agent_models::offered_here`]). It is the same silence that `agents` takes and for the same
/// reason: an agent nobody has asked leaves its steps' models unjudged, rather than judged and failed.
/// The face has no part in it because the answers are kept on this side, and putting the question would
/// mean a login shell and a provider starting up behind every draw of a build screen.
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
    let unmet = automation_run::check(
        store.read_model().conn(),
        id,
        agents.as_deref(),
        &crate::agent_models::offered_here(),
    )?;
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
        // The model, not the agent, in the one slot a block carries: the row leads with the step, and a
        // step names one agent, so what the reader cannot see from the picture is which model it asked
        // for. Core's own sentence names both (`amenbo_core::ops::automation_run::Unmet::say`).
        Unmet::ModelMissing { step, model, .. } => ("model_missing", Some(step), Some(model)),
    };
    AutomationLaunchBlockDto { reason, step_name: step.cloned(), at: at.cloned() }
}

/// **How many lanes are held right now** — the runs that are `running`, across every project.
///
/// It crosses projects because the lanes do: what a lane holds is a terminal on this machine and the
/// attention of the person watching it, and neither is divided up per project. So this answers a bare
/// number, and the band that draws it says nothing about which project each one is in — the "running"
/// tab is where a reader goes to see that.
#[tauri::command]
pub fn automation_lanes_held() -> Result<i64, CmdError> {
    let store = open_store_read()?;
    Ok(read::automation_run_ids_running(store.read_model().conn())?.len() as i64)
}

/// The event the workspace hears when a step of a run is ready to be drawn.
///
/// It travels as an event rather than as an answer because of where the two ends are: the press that
/// starts a run is on the ledger, and the pane it opens is in the workspace — which is the same
/// window in one shape of the app and the other window in the other (`AMB-D-753`). An answer handed
/// back to the presser would reach a screen that has no pane to stand it in.
const STEP_EVENT: &str = "automation-step";

/// **Open one step of a run**: write the execution down, build the text its terminal is started on,
/// and tell the workspace to stand a terminal on it.
///
/// `run_def_id` names which step, and **`None` is the first one** — the copy of the automation's entry
/// step ([`amenbo_core::ops::automation_run::entry_def`]). That is the whole of what a launch knows to
/// ask for; which step comes after which is read from the way out the one before it took, and is the
/// job of the op that receives a report (`AMB-T-5246`).
///
/// The answer and the event carry the same thing. The event is what the workspace acts on, and the
/// answer is for the caller to know what happened — a run stopped for a missing input opens no
/// terminal, and the press that started it is owed that sentence.
#[tauri::command]
pub fn automation_step_open(
    app: tauri::AppHandle,
    run_id: i64,
    run_def_id: Option<i64>,
) -> Result<AutomationStepOpenDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_step_open");
    let def_id = match run_def_id {
        Some(id) => id,
        None => {
            let store = open_store_read()?;
            // Uncoded on purpose. An automation with no entry is refused at the launch check
            // (`amenbo_core::ops::automation_run::Unmet::NoEntry`), so the only way to reach this is
            // the entry being taken off while a run of it is under way — which no screen puts in
            // front of anybody, and a code is split off a family where one does.
            automation_run::entry_def(store.read_model().conn(), run_id)?
                .ok_or_else(|| {
                    CmdError::from(amenbo_core::error::Error::invalid(format!(
                        "run '{run_id}' has no step to start at — its automation lost its entry"
                    )))
                })?
                .id
        }
    };
    // How many runs may be under way at once. It is a setting and lives outside the store, so it is
    // read here and handed down: a step that cannot be opened stops the run, and stopping one hands
    // its lane back to whatever was waiting for it.
    let paths = amenbo_core::config::Paths::resolve()?;
    let lanes = amenbo_core::config::Config::load(&paths.config_file).automation_lanes;
    let mut store = crate::commands::open_store()?;
    let opened = store.automation_step_open(run_id, def_id, lanes)?;
    let (project, step, missing) = match opened {
        Opened::Ready(ready) => {
            let def = &ready.run_def;
            let run = read::automation_run(store.read_model().conn(), run_id)?
                .ok_or_else(|| CmdError::from(amenbo_core::error::Error::not_found(
                    format!("run '{run_id}' not found"),
                )))?;
            (
                run.project_id,
                Some(AutomationStepRunDto {
                    run_step: ready.run_step.id,
                    seq: ready.run_step.seq,
                    name: def.name.clone(),
                    task: worked_task(&store, ready.run_step.run_task_id)?,
                    say: ready.text.clone(),
                    agent: def.agent.clone(),
                    model: def.model.clone(),
                    folder: ready.folder.clone(),
                    interactive: def.interactive,
                }),
                Vec::new(),
            )
        }
        // `woke` is the run that took the lane this one gave up. It is the "running" tab's to draw
        // and not this door's: what a step's pane is told about is its own step.
        Opened::Stopped { run, missing, .. } => (run.project_id, None, missing),
    };
    let dto = AutomationStepOpenDto { run: run_id, project, step, missing };
    if let Err(e) = app.emit(STEP_EVENT, dto.clone()) {
        log::warn!("failed to emit {STEP_EVENT}: {e}");
    }
    Ok(dto)
}

/// **The task one stretch of a run is working**, read off the ledger for the pane's header.
///
/// `None` where the execution belongs to no stretch — a run whose steps take no task at all — and
/// where the task itself has been deleted since the stretch opened, the column being nulled rather
/// than the row going with it (`automation_run_task.task_id`).
fn worked_task(
    store: &amenbo_core::Store,
    run_task_id: Option<i64>,
) -> Result<Option<AutomationRunTaskDto>, CmdError> {
    let Some(stretch_id) = run_task_id else { return Ok(None) };
    let conn = store.read_model().conn();
    let Some(task_id) = read::automation_run_task(conn, stretch_id)?.and_then(|s| s.task_id) else {
        return Ok(None);
    };
    Ok(read::task_title(conn, task_id)?.map(|title| AutomationRunTaskDto {
        id: task_id,
        r#ref: amenbo_core::idref::task(task_id),
        title,
    }))
}

/// **Stop a run now** — what closing the pane a run is drawn in means
/// (`app/src/shell/TerminalPane.tsx`).
///
/// The cleanup is core's and is the same one every other stop goes through
/// ([`amenbo_core::ops::automation_stop::stop`]): the lane is handed back, the task the run was
/// working goes to `todo`, and a line on that task says the run is not coming back. The terminal
/// standing in the pane is the pane's own to end — it is a process this side started, and core has
/// no window to end one from.
///
/// **A run that is over already is not an error here.** The pane is closed by a person, and between
/// the last step reporting and the press there is a window in which the run has finished on its own;
/// a refusal then would put a red sentence in front of somebody who did nothing wrong. What comes
/// back says whether this press was the one that stopped it.
#[tauri::command]
pub fn automation_run_stop(run_id: i64) -> Result<bool, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let lanes = amenbo_core::config::Config::load(&paths.config_file).automation_lanes;
    let mut store = crate::commands::open_store()?;
    let over = match read::automation_run(store.read_model().conn(), run_id)? {
        Some(run) => !matches!(
            run.status,
            AutomationRunStatus::Running | AutomationRunStatus::Queued | AutomationRunStatus::Paused
        ),
        // A run nobody can find is one nothing can be stopped about, and the pane is going either
        // way. Saying so is the whole of what is left to do.
        None => true,
    };
    if over {
        return Ok(false);
    }
    store.automation_run_stop(run_id, AutomationStoppedReason::ByHuman, lanes)?;
    Ok(true)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::env_guard;

    /// **A press on a pane is never a refusal.** Closing a run's pane is a person being rid of the
    /// pane, and between the last step reporting and the press there is a window in which the run
    /// ended on its own — so a run that is over, or one whose rows are gone entirely, answers `false`
    /// rather than putting a red sentence in front of somebody who did nothing wrong.
    ///
    /// Both facts come out of the same door: what is refused by core is the stop
    /// ([`amenbo_core::ops::automation_stop::stop`], whose own cases cover the states), and what is
    /// answered here is whether this press was the one that stopped it.
    #[test]
    fn stopping_a_run_that_is_not_there_is_answered_rather_than_refused() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("automation-stop-gone");
        std::env::set_var("AMENBO_HOME", &tmp);
        amenbo_core::Store::open().unwrap();

        assert!(!automation_run_stop(404).expect("a run nobody can find is not an error"));
    }
}
