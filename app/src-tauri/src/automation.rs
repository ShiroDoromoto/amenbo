//! **Reading an automation's definition, saying whether it could be started, and running one** — the
//! doors behind the automations screen, its build screen, the "running" tab and the pane a run is
//! drawn in.
//!
//! Core owns the ten definition tables, the writes that build them
//! ([`amenbo_core::ops::automation`]) and the read that resolves them
//! ([`amenbo_core::ops::automation_view`]) — a step's ways out, its inputs and its settings come
//! already read off the library action it points at or off itself. Nothing is built or resolved
//! here. What this side does is name: the same rows under the names a screen draws them by.
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
//!
//! **Whatever frees a lane owes the next run a terminal.** Core hands a run back with the one a
//! freed lane promoted ([`amenbo_core::ops::automation_stop::Ended`]), and promotion writes
//! `running` and nothing else — so the three doors that can end a run, and the one that opens a
//! step, all go through the same walk (`follow` below). Dropped, a promoted run sits `running` with no
//! terminal under it and the queue stops moving.

use amenbo_core::model::{
    ActorKind, AutomationCfg, AutomationPort, AutomationPortDirection, AutomationPortKind,
    AutomationPortOwner, AutomationRunStatus, AutomationStoppedReason,
};
use amenbo_core::ops::automation::{NewStep, StepSource};
use amenbo_core::ops::automation_run::{self, Unmet};
use amenbo_core::ops::automation_stop::{Ended, Paused, Resumed, TookALane};
use amenbo_core::ops::automation_step::Opened;
use amenbo_core::ops::automation_view;
use amenbo_core::store_engine::read;

use crate::commands::{open_store_read, with_store_mut};
use crate::dto::{
    AutomationActionCardDto, AutomationCardDto, AutomationCfgDto, AutomationDetailDto,
    AutomationEdgeDto, AutomationExitDto, AutomationLaunchBlockDto, AutomationLaunchCheckDto,
    AutomationPortDto, AutomationRunCardDto, AutomationRunStartedDto, AutomationRunTaskDto,
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
    let cards = automation_view::cards(store.read_model().conn(), project_id)?;
    Ok(cards
        .into_iter()
        .map(|card| AutomationCardDto {
            id: card.automation.id,
            name: card.automation.name,
            steps: card.steps,
            archived: card.automation.archived,
        })
        .collect())
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
    let cards = automation_view::action_cards(store.read_model().conn(), Some(project_id))?;
    Ok(cards
        .into_iter()
        .map(|card| AutomationActionCardDto {
            id: card.action.id,
            name: card.action.name,
            prompt: card.action.prompt,
            // The shelf travels as the fact the screen draws rather than as the project id: a screen
            // inside one project would only ever read an id back as "mine" or "the device's".
            global: card.action.project_id.is_none(),
            used_by: card.used_by,
        })
        .collect())
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

/// **Change one step of an automation.** Only what is `Some` is written.
///
/// `action` and `prompt` are the two halves of where the prompt comes from, and exactly one may be
/// given: switching from one to the other takes the step's ways out, its settings and its inputs with
/// it, because those are read off whichever of the two declares them
/// ([`amenbo_core::ops::automation::step_update`]). The panel puts that switch on one control for the
/// same reason — there is no state where a step has both and none where it has neither.
///
/// `model` and `work_dir` are each a field with a third answer: written, cleared, or left alone. The
/// pair of arguments says which — `clear_model` beats a `model` beside it, and the same for the
/// folder — rather than a single `Option<Option<..>>`, which does not cross the IPC boundary as a
/// shape a screen can write.
///
/// The ack names the library as well as the definition. Pointing a step at an action, or away from
/// one, moves how many automations that action is used by, which is a column of the actions tab
/// (`automations_using`).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_step_edit(
    id: i64,
    name: Option<String>,
    action: Option<i64>,
    prompt: Option<String>,
    agent: Option<String>,
    model: Option<String>,
    clear_model: Option<bool>,
    interactive: Option<bool>,
    work_dir: Option<String>,
    clear_work_dir: Option<bool>,
    report_to_task: Option<bool>,
    history: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let source = match (action, prompt.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(amenbo_core::Error::invalid(
                "a step runs a library action or carries a prompt of its own, never both",
            )
            .into())
        }
        (Some(action), None) => Some(StepSource::Action(action)),
        (None, Some(prompt)) => Some(StepSource::Prompt(prompt.to_string())),
        (None, None) => None,
    };
    let model = match (clear_model, model.as_deref()) {
        (Some(true), _) => Some(None),
        (_, Some(model)) => Some(Some(model)),
        _ => None,
    };
    let work_dir = match (clear_work_dir, work_dir.as_deref()) {
        (Some(true), _) => Some(None),
        (_, Some(name)) => Some(Some(name)),
        _ => None,
    };
    with_store_mut(|store| {
        store.automation_step_update(
            id,
            name.as_deref(),
            source,
            agent.as_deref(),
            model,
            interactive,
            work_dir,
            report_to_task,
            history,
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Answer one setting on one step**, or leave it unanswered with no `value`.
///
/// The answer travels as the JSON its kind takes — a string for a folder, a choice and a text, a
/// number for a number, and an object naming each part of a task filter. The shape is the screen's to
/// build, because the control that took it is the screen's too
/// (`app/src/screens/automationCfg.ts`); core keeps the text as it is handed and the run reads it
/// ([`amenbo_core::ops::automation::cfg_set`]).
///
/// A step running a library action answers on a row of its own under the declared name, so a second
/// step running the same action is not answering for both. That split is core's, and this door does
/// not have to know which of the two it is writing.
#[tauri::command]
pub fn automation_cfg_answer(
    step_id: i64,
    name: String,
    value: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_cfg_set(step_id, &name, value.as_deref())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Put a step in on a line** — the one road by which a step joins a picture already drawn.
///
/// `inputs` is a flat list of triples the screen sends as `[name, kind, required]`, because a struct
/// per row would be one more shape to keep in step across the boundary for three fields. An unknown
/// kind is refused here rather than stored: the four are the port kinds core knows
/// ([`amenbo_core::model::AutomationPortKind`]).
///
/// The whole press is one transaction, edges and all
/// ([`amenbo_core::ops::automation::step_insert`]): a step left behind with the line still running
/// past it is a picture nobody asked for.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_step_insert(
    edge_id: i64,
    name: String,
    action: Option<i64>,
    prompt: Option<String>,
    agent: String,
    model: Option<String>,
    interactive: bool,
    exits: Vec<String>,
    inputs: Vec<(String, String, bool)>,
) -> Result<WriteAck, CmdError> {
    let source = match (action, prompt.as_deref()) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(amenbo_core::Error::invalid(
                "a step runs a library action or carries a prompt of its own",
            )
            .into())
        }
        (Some(action), None) => StepSource::Action(action),
        (None, Some(prompt)) => StepSource::Prompt(prompt.to_string()),
    };
    let mut ports = Vec::with_capacity(inputs.len());
    for (name, kind, required) in inputs {
        let kind = AutomationPortKind::parse(&kind).ok_or_else(|| {
            amenbo_core::Error::invalid(format!(
                "'{kind}' is not something a port carries — value, file, task_take or task_make"
            ))
        })?;
        ports.push((name, kind, required));
    }
    let new = NewStep {
        name,
        source,
        agent,
        model,
        interactive,
        work_dir_ref: None,
        report_to_task: false,
        show_history: true,
    };
    with_store_mut(|store| {
        store.automation_step_insert(edge_id, new, &exits, &ports)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Declare what a way out hands on.**
///
/// It belongs to the way out and not to the step, because what is handed on is produced by leaving
/// through that particular way out — a step with three ways out hands on three different things
/// ([`amenbo_core::ops::automation::port_add`]).
#[tauri::command]
pub fn automation_output_add(
    exit_id: i64,
    name: String,
    kind: String,
    required: bool,
) -> Result<WriteAck, CmdError> {
    let kind = AutomationPortKind::parse(&kind).ok_or_else(|| {
        amenbo_core::Error::invalid(format!(
            "'{kind}' is not something a port carries — value, file, task_take or task_make"
        ))
    })?;
    with_store_mut(|store| {
        store.automation_port_add(
            AutomationPortOwner::Exit,
            exit_id,
            AutomationPortDirection::Out,
            &name,
            kind,
            required,
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Say what fills one of a step's inputs**, by naming the way out and the output it comes from.
///
/// Drawing the same wire twice answers the one already drawn rather than writing a second row
/// ([`amenbo_core::ops::automation::wire_add`]), so the screen's control can send what the reader
/// picked without first working out whether anything was there.
#[tauri::command]
pub fn automation_wire_set(
    from_step_id: i64,
    from_exit_name: Option<String>,
    from_port_name: String,
    to_step_id: i64,
    to_port_name: String,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_wire_add(
            from_step_id,
            from_exit_name.as_deref(),
            &from_port_name,
            to_step_id,
            &to_port_name,
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Take a wire away**, leaving the input it fed with nothing reaching it.
#[tauri::command]
pub fn automation_wire_clear(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_wire_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// One automation's whole definition, or nothing where that id names none.
#[tauri::command]
pub fn automation_detail(id: i64) -> Result<Option<AutomationDetailDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_detail");
    let store = open_store_read()?;
    Ok(automation_view::detail(store.read_model().conn(), id)?.map(detail_dto))
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

/// **Start a run of this automation** — the press behind the build screen's "start".
///
/// What the store cannot answer is handed in, each from the side that holds it
/// ([`amenbo_core::ops::automation_run::Launcher`]). `agents` is what this machine can start, asked
/// by the face for the empty frame and passed on here rather than probed again; `null` is "nobody
/// asked", and then no step is judged on its agent (`AMB-D-792`). `workspace_open` is the screen's
/// to answer: the workspace is a face of this window in one shape of the app and a window of its own
/// in the other (`AMB-D-753`), and which of those is standing is not a thing the store or this door
/// can see. It is always answered from here, never left unsaid: core takes `None` from a caller that
/// cannot see a window at all, and a press made on a screen is not one of those.
///
/// **A refusal comes back as a refusal.** Core raises the archived automation, the launch check and
/// the closed workspace in that order, each with a sentence the screen can put in front of a person
/// ([`amenbo_core::ops::automation_run::launch`]) — so nothing is judged twice here.
///
/// **A run that took a lane is opened on its first step before this answers.** The pane the run is
/// drawn in is stood by the workspace when it hears that step, so a launch that stopped short of it
/// would be a press that wrote a row and left the screen unchanged. A queued run opens nothing: what
/// wakes it is a lane being handed back (`AMB-T-5246`, `AMB-T-5247`).
#[tauri::command]
pub fn automation_launch(
    app: tauri::AppHandle,
    id: i64,
    agents: Option<Vec<String>>,
    workspace_open: bool,
) -> Result<AutomationRunStartedDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_launch");
    let paths = amenbo_core::config::Paths::resolve()?;
    let lanes = amenbo_core::config::Config::load(&paths.config_file).automation_lanes;
    // The models are read here, as the check reads them (`automation_launch_check`): the press is
    // inside the process that keeps the answers, so a step naming a model its agent does not have is
    // refused at the press rather than met inside the pane the run just opened.
    let offered = crate::agent_models::offered_here();
    let by = automation_run::Launcher {
        startable: agents.as_deref(),
        models: &offered,
        lanes,
        workspace_open: Some(workspace_open),
        by: Some(ActorKind::Human),
    };
    let run = with_store_mut(|store| Ok(store.automation_launch(id, &by)?))?;
    let queued = !run.status.holds_a_lane();
    if !queued {
        automation_step_open(app, run.id, None)?;
    }
    Ok(AutomationRunStartedDto { run: run.id, queued })
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
    let mut out = Vec::new();
    for run in read::automation_runs_live(store.read_model().conn(), STOPPED_SHOWN)? {
        out.push(run_card(&store, run)?);
    }
    Ok(out)
}

/// One run as the tab draws it: what it is, how far in, and what it is on.
fn run_card(
    store: &amenbo_core::Store,
    run: amenbo_core::model::AutomationRun,
) -> Result<AutomationRunCardDto, CmdError> {
    let conn = store.read_model().conn();
    let steps = read::automation_run_steps_of(conn, run.id)?;
    // The step it is on, or the last one it ran — read through the run's own copy of the definition,
    // which is what says what was asked at launch rather than what the automation says now.
    let step_name = match steps.last() {
        Some(last) => read::automation_run_def(conn, last.run_def_id)?.map(|def| def.name),
        None => None,
    };
    // The stretch it is in now. A run walks one per task, and a run between tasks is on none.
    let stretch = read::automation_run_task_last(conn, run.id)?.map(|one| one.id);
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
        task: worked_task(store, stretch)?,
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
/// ([`amenbo_core::ops::automation_stop::pause`]). Pressed on a row of the "running" tab.
///
/// Pressed on a run with nothing under way it takes effect on the spot, and then a lane may come free
/// here — which is why this door takes the window: whatever was waiting is promoted inside the same
/// press, and a promoted run needs a terminal opening on it.
///
/// **It is not a `WriteAck` write**, for the reason [`automation_run_stop`] is not: what it moves is a
/// run, and every screen drawing one is already following the change feed.
#[tauri::command]
pub fn automation_run_pause(app: tauri::AppHandle, run_id: i64) -> Result<(), CmdError> {
    let lanes = lanes()?;
    let mut store = crate::commands::open_store()?;
    let woke = match store.automation_pause(run_id, lanes)? {
        Paused::Asked(_) => None,
        Paused::Now(ended) => ended.woke,
    };
    follow(&app, &mut store, woke, lanes)
}

/// **Pick a paused run up again** ([`amenbo_core::ops::automation_stop::resume`]). It opens a terminal
/// on the step its last one led to where a lane is free, and joins the queue where none is.
#[tauri::command]
pub fn automation_run_resume(app: tauri::AppHandle, run_id: i64) -> Result<(), CmdError> {
    let lanes = lanes()?;
    let mut store = crate::commands::open_store()?;
    match store.automation_resume(run_id, lanes)? {
        Resumed::Step { run, next } => {
            drive(&app, &mut store, run.id, next.id, lanes)?;
        }
        Resumed::Queued(_) => {}
    }
    Ok(())
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
                    seq: ready.run_step.seq,
                    name: def.name.clone(),
                    task: worked_task(store, ready.run_step.run_task_id)?,
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
    // The run has just moved, so the thread that keeps it going looks again now rather than sleeping
    // out the interval it was on (`crate::automation_watch`). Called from the watch's own path too,
    // where it costs nothing: that loop is about to come round anyway.
    crate::automation_watch::wake();
    let dto = AutomationStepOpenDto { run: run_id, project, step, missing };
    if let Err(e) = app.emit(STEP_EVENT, dto.clone()) {
        log::warn!("failed to emit {STEP_EVENT}: {e}");
    }
    Ok((dto, woke))
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
/// (`app/src/shell/TerminalPane.tsx`), and what the "running" tab's third button presses.
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
pub fn automation_run_stop(app: tauri::AppHandle, run_id: i64) -> Result<bool, CmdError> {
    let lanes = lanes()?;
    let mut store = crate::commands::open_store()?;
    let Some(ended) = stop_if_going(&mut store, run_id, lanes)? else { return Ok(false) };
    follow(&app, &mut store, ended.woke, lanes)?;
    Ok(true)
}

/// The half of the stop that has no window in it: stop the run where it is still going, and answer
/// `None` where there was nothing to stop. Split out so the "already over" arm can be tested without
/// an app to hand ([`automation_run_stop`] is the whole of it, the lane included).
fn stop_if_going(
    store: &mut amenbo_core::Store,
    run_id: i64,
    lanes: i64,
) -> Result<Option<Ended>, CmdError> {
    let going = match read::automation_run(store.read_model().conn(), run_id)? {
        Some(run) => matches!(
            run.status,
            AutomationRunStatus::Running | AutomationRunStatus::Queued | AutomationRunStatus::Paused
        ),
        // A run nobody can find is one nothing can be stopped about, and the pane is going either
        // way. Saying so is the whole of what is left to do.
        None => false,
    };
    if !going {
        return Ok(None);
    }
    Ok(Some(store.automation_stop(run_id, AutomationStoppedReason::ByHuman, lanes)?))
}

// ───────────────────────────── shaping ─────────────────────────────
//
// Core resolves (amenbo_core::ops::automation_view); what is left here is naming. Nothing below
// reads the store — a DTO is the same rows under the names a screen draws them by.

/// One automation's whole definition, under the names the build screen draws it by.
fn detail_dto(view: automation_view::AutomationView) -> AutomationDetailDto {
    let a = view.automation;
    AutomationDetailDto {
        id: a.id,
        project_id: a.project_id,
        name: a.name,
        notes: a.notes,
        preamble: a.preamble,
        entry_step_id: a.entry_step_id,
        archived: a.archived,
        steps: view.steps.into_iter().map(step_dto).collect(),
        edges: view
            .edges
            .into_iter()
            .map(|edge| AutomationEdgeDto {
                id: edge.id,
                from_step_id: edge.from_step_id,
                exit_name: edge.exit_name,
                to_step_id: edge.to_step_id,
                ends: edge.ends.as_str(),
                max_times: edge.max_times,
            })
            .collect(),
        wires: view
            .wires
            .into_iter()
            .map(|wire| AutomationWireDto {
                id: wire.id,
                from_step_id: wire.from_step_id,
                from_exit_name: wire.from_exit_name,
                from_port_name: wire.from_port_name,
                to_step_id: wire.to_step_id,
                to_port_name: wire.to_port_name,
            })
            .collect(),
    }
}

/// One step, with the prompt it runs on already chosen between its own and its action's.
fn step_dto(view: automation_view::StepView) -> AutomationStepDto {
    let step = view.step;
    AutomationStepDto {
        id: step.id,
        name: step.name,
        action_id: step.action_id,
        action_name: view.action.map(|one| one.name),
        prompt: view.prompt,
        agent: step.agent,
        model: step.model,
        interactive: step.interactive,
        work_dir_ref: step.work_dir_ref,
        report_to_task: step.report_to_task,
        show_history: step.show_history,
        exits: view.exits.into_iter().map(exit_dto).collect(),
        inputs: view.inputs.into_iter().map(port_dto).collect(),
        settings: view.settings.into_iter().map(cfg_dto).collect(),
    }
}

fn exit_dto(view: automation_view::ExitView) -> AutomationExitDto {
    AutomationExitDto {
        id: view.exit.id,
        name: view.exit.name,
        outputs: view.outputs.into_iter().map(port_dto).collect(),
    }
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

        let lanes = lanes().expect("the lane count");
        let mut store = crate::commands::open_store().expect("the store");
        assert!(stop_if_going(&mut store, 404, lanes)
            .expect("a run nobody can find is not an error")
            .is_none());
    }
}
