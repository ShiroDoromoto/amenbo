//! **Reading an automation's definition, saying whether it could be started, and running one** — the
//! doors behind the automations screen, its build screen, the "running" tab and the pane a run is
//! drawn in.
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
//! **A step that is ready to run is told to the window rather than answered back**, as an event.
//! The press that starts a run is on the ledger and the pane it opens is in the workspace — the same
//! window in one shape of the app and the other window in the other (`AMB-D-753`) — so an answer
//! handed back to whoever pressed would reach a screen with no pane to stand it in.
//!
//! **Whatever frees a lane owes the next run a terminal.** Core hands a run back with the one a
//! freed lane promoted ([`amenbo_core::ops::automation_stop::Ended`]), and promotion writes
//! `running` and nothing else — so the three doors that can end a run, and the one that opens a
//! step, all go through the same walk (`follow` below). Dropped, a promoted run sits `running` with no
//! terminal under it and the queue stops moving.

use amenbo_core::model::{
    Automation, AutomationCfg, AutomationExit, AutomationPort, AutomationPortDirection,
    AutomationPortOwner, AutomationStep, AutomationStoppedReason,
};
use amenbo_core::ops::automation::declarer;
use amenbo_core::ops::automation_run::{self, Unmet};
use amenbo_core::ops::automation_stop::{Paused, Resumed, TookALane};
use amenbo_core::ops::automation_step::Opened;
use amenbo_core::store_engine::{read, StoreEngine};

use crate::commands::open_store_read;
use crate::dto::{
    AutomationCardDto, AutomationCfgDto, AutomationDetailDto, AutomationEdgeDto, AutomationExitDto,
    AutomationLaunchBlockDto, AutomationLaunchCheckDto, AutomationPortDto, AutomationRunCardDto,
    AutomationStepDto, AutomationStepOpenDto, AutomationStepRunDto, AutomationWireDto, WriteAck,
};
use crate::error::CmdError;
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

/// **How many stopped runs the "running" tab is shown.** A stop is kept on the list so that a failure
/// nobody was watching is still seen — not so that every failure since the store was made is listed.
/// What a run did months ago is read from the task it worked ([`amenbo_core::store::Store::automation_runs_for_task`]).
const STOPPED_SHOWN: usize = 20;

/// **What is under way right now**, across every project — the rows of the "running" tab.
///
/// It crosses projects for the same reason [`automation_lanes_held`] does, and it is the tab that says
/// which project each of those lanes is in. Runs that are `done` are not here: what a finished run did
/// is reached from the task it worked or the automation it came from, never searched for
/// (`amenbo_core::store::Store`'s run reads).
#[tauri::command]
pub fn automation_running_page() -> Result<Vec<AutomationRunCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_running_page");
    let store = open_store_read()?;
    let engine = store.read_model();
    let mut out = Vec::new();
    for run in read::automation_runs_live(engine.conn(), STOPPED_SHOWN)? {
        out.push(run_card(engine, run)?);
    }
    Ok(out)
}

/// One run as the tab draws it: what it is, how far in, and what it is on.
fn run_card(
    engine: &StoreEngine,
    run: amenbo_core::model::AutomationRun,
) -> Result<AutomationRunCardDto, CmdError> {
    let conn = engine.conn();
    let steps = read::automation_run_steps_of(conn, run.id)?;
    // The step it is on, or the last one it ran — read through the run's own copy of the definition,
    // which is what says what was asked at launch rather than what the automation says now.
    let step_name = match steps.last() {
        Some(last) => read::automation_run_def(conn, last.run_def_id)?.map(|def| def.name),
        None => None,
    };
    // The stretch it is in now. A run walks one per task, and a run between tasks is on none.
    let task = match read::automation_run_task_last(conn, run.id)? {
        Some(stretch) => stretch.task_id,
        None => None,
    };
    let task_title = match task {
        Some(id) => read::task(conn, id)?.map(|one| one.title),
        None => None,
    };
    Ok(AutomationRunCardDto {
        run: run.id,
        project: run.project_id,
        project_name: read::project(conn, run.project_id)?.map(|p| p.name).unwrap_or_default(),
        automation: run.automation_id,
        automation_name: read::automation(conn, run.automation_id)?
            .map(|one| one.name)
            .unwrap_or_default(),
        status: run.status.as_str(),
        pause_requested: run.pause_requested,
        stopped_reason: run.stopped_reason.map(|one| one.as_str()),
        step_name,
        steps_done: steps.len(),
        // A stretch whose task has since been deleted keeps the id and answers no title, which is
        // what `task_id` being `Some` with nothing beside it means on the row.
        task_id: task,
        task_title,
    })
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
    let lanes = lanes()?;
    let mut store = crate::commands::open_store()?;
    let def_id = match run_def_id {
        Some(id) => id,
        // Uncoded on purpose. An automation with no entry is refused at the launch check
        // (`amenbo_core::ops::automation_run::Unmet::NoEntry`), so the only way to reach this is
        // the entry being taken off while a run of it is under way — which no screen puts in
        // front of anybody, and a code is split off a family where one does.
        None => {
            automation_run::entry_def(store.read_model().conn(), run_id)?
                .ok_or_else(|| {
                    CmdError::from(amenbo_core::error::Error::invalid(format!(
                        "run '{run_id}' has no step to start at — its automation lost its entry"
                    )))
                })?
                .id
        }
    };
    drive(&app, &mut store, run_id, def_id, lanes)
}

/// **Pause a run** — it settles at the end of the step under way, and hands its lane back there
/// ([`amenbo_core::ops::automation_stop::pause`]).
///
/// Pressed on a run with nothing under way it takes effect on the spot, and then a lane may come free
/// here — which is why this door takes the window: whatever was waiting is promoted inside the same
/// press, and a promoted run needs a terminal opening on it.
#[tauri::command]
pub fn automation_run_pause(app: tauri::AppHandle, run: i64) -> Result<WriteAck, CmdError> {
    let lanes = lanes()?;
    let mut store = crate::commands::open_store()?;
    let woke = match store.automation_run_pause(run, lanes)? {
        Paused::Asked(_) => None,
        Paused::Now(ended) => ended.woke,
    };
    follow(&app, &mut store, woke, lanes)?;
    Ok(ack())
}

/// **Pick a paused run up again** ([`amenbo_core::ops::automation_stop::resume`]). It opens a terminal
/// on the step its last one led to where a lane is free, and joins the queue where none is.
#[tauri::command]
pub fn automation_run_resume(app: tauri::AppHandle, run: i64) -> Result<WriteAck, CmdError> {
    let lanes = lanes()?;
    let mut store = crate::commands::open_store()?;
    match store.automation_run_resume(run, lanes)? {
        Resumed::Step { run, next } => {
            drive(&app, &mut store, run.id, next.id, lanes)?;
        }
        Resumed::Queued(_) => {}
    }
    Ok(ack())
}

/// **Stop a run now** ([`amenbo_core::ops::automation_stop::stop`]) — the lane goes back, the task it
/// was holding goes back to `todo`, and a line is left on that task saying what became of it.
///
/// The reason is `by_human` and is not asked for: this door is a button, and the three other reasons
/// are ones the machine finds rather than ones a person presses.
#[tauri::command]
pub fn automation_run_stop(app: tauri::AppHandle, run: i64) -> Result<WriteAck, CmdError> {
    let lanes = lanes()?;
    let mut store = crate::commands::open_store()?;
    let ended = store.automation_run_stop(run, AutomationStoppedReason::ByHuman, lanes)?;
    follow(&app, &mut store, ended.woke, lanes)?;
    Ok(ack())
}

/// What every one of the three writes above invalidates: the "running" tab, and the band that counts
/// the lanes with it. The task a stop handed back is named too — it went to `todo` and picked up a
/// comment, and the pane reading it is the one place that would otherwise go on drawing the old
/// status.
fn ack() -> WriteAck {
    WriteAck::new(&["automationRuns", "tasks"])
}

/// How many runs may be under way at once. It is a setting and lives outside the store, so every door
/// that can free a lane reads it here and hands it down: a step that cannot be opened stops the run,
/// and stopping one hands its lane back to whatever was waiting for it.
fn lanes() -> Result<i64, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    Ok(amenbo_core::config::Config::load(&paths.config_file).automation_lanes)
}

/// **Open a step, tell the workspace, and then follow the lane** — the whole of what a press that
/// moves a run owes.
fn drive(
    app: &tauri::AppHandle,
    store: &mut amenbo_core::Store,
    run_id: i64,
    def_id: i64,
    lanes: i64,
) -> Result<AutomationStepOpenDto, CmdError> {
    let (dto, woke) = open_one(app, store, run_id, def_id, lanes)?;
    follow(app, store, woke, lanes)?;
    Ok(dto)
}

/// **Give every run a freed lane woke a terminal of its own.**
///
/// A run ending hands its lane back, and whatever has waited longest is promoted to `running` in the
/// same transaction — with nothing running in it. Core is explicit that the driver which freed the
/// lane is the one that owes it a step ([`amenbo_core::ops::automation_stop::Ended`]), and one
/// promotion can lead to another: the step this opens may itself stop for a missing input, freeing the
/// lane again. So it is a walk and not a single hop, and it ends because every turn of it either
/// stands a terminal up or ends a run, and there are finitely many runs to end.
fn follow(
    app: &tauri::AppHandle,
    store: &mut amenbo_core::Store,
    woke: Option<amenbo_core::model::AutomationRun>,
    lanes: i64,
) -> Result<(), CmdError> {
    let mut next = woke;
    while let Some(run) = next.take() {
        next = match store.automation_run_took_a_lane(run.id, lanes)? {
            TookALane::Step(def) => open_one(app, store, run.id, def.id, lanes)?.1,
            TookALane::Lost(ended) => ended.woke,
        };
    }
    Ok(())
}

/// One step opened and told to the window, with whatever run a freed lane woke handed back.
///
/// The answer and the event carry the same thing. The event is what the workspace acts on, and the
/// answer is for the caller to know what happened — a run stopped for a missing input opens no
/// terminal, and the press that started it is owed that sentence.
fn open_one(
    app: &tauri::AppHandle,
    store: &mut amenbo_core::Store,
    run_id: i64,
    def_id: i64,
    lanes: i64,
) -> Result<(AutomationStepOpenDto, Option<amenbo_core::model::AutomationRun>), CmdError> {
    let opened = store.automation_step_open(run_id, def_id, lanes)?;
    let (project, step, missing, woke) = match opened {
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
                    name: def.name.clone(),
                    say: ready.text.clone(),
                    agent: def.agent.clone(),
                    model: def.model.clone(),
                    folder: ready.folder.clone(),
                    interactive: def.interactive,
                }),
                Vec::new(),
                None,
            )
        }
        // What a step's pane is told about is its own step, so the run a freed lane woke is not in the
        // event — it is handed back for the walk above to open in a press of its own.
        Opened::Stopped { run, missing, woke } => (run.project_id, None, missing, woke),
    };
    let dto = AutomationStepOpenDto { run: run_id, project, step, missing };
    if let Err(e) = app.emit(STEP_EVENT, dto.clone()) {
        log::warn!("failed to emit {STEP_EVENT}: {e}");
    }
    Ok((dto, woke))
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
