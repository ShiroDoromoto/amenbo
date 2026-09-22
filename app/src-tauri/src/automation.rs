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
//! **No door here carries a run forward.** A press starts one, pauses it, picks it up or stops it,
//! and that is the whole of what it does; what opens the step after is the thread that keeps runs
//! going (`crate::automation_watch`, `AMB-D-945`). A press leaves the run `running` with nothing
//! open, which is exactly the shape the watch is looking for. Every entrance doing its own "and then
//! open the next one" is what the watch was stood up to end, and the two that were left are gone
//! with this (`AMB-T-5289`).

use amenbo_core::model::{
    ActorKind, AutomationCfg, AutomationCfgKind, AutomationOwner, AutomationPort,
    AutomationPortDirection, AutomationPortKind, AutomationPortOwner, AutomationRunStatus,
    AutomationStoppedReason,
};
use amenbo_core::ops::automation::{NewStep, StepSource};
use amenbo_core::ops::automation_run::{self, Unmet};
use amenbo_core::ops::automation_stop::Ended;
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

/// **Make a library action** — a name, and which library it lands in.
///
/// **The prompt is not asked for here.** It is written in the box the list opens on the row
/// ([`automation_action_edit`]), which is the one place a prompt is written: a second field writing
/// the same column would be a second place to keep in step. The row is born carrying an empty
/// prompt, and the screen opens that box on it straight away.
///
/// `project` is which library it lands in — the project's own, or the device's where every project
/// on this machine reaches it ([`automation_action_from_step`]).
#[tauri::command]
pub fn automation_action_add(project: Option<i64>, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_add(project, &name, "")?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automationActions"]))
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

/// **Raise a step's own prompt into the library**: make an action of it, move the step's declarations
/// onto that action, and point the step at it
/// ([`amenbo_core::ops::automation::action_from_step`]).
///
/// It is the one road from the build screen into the library, which until now could only be filled
/// from the CLI. The declarations **move** rather than being copied — a step that runs an action
/// declares nothing of its own — and the picture around the step goes on reading because an edge and
/// a wire name a way out by its name.
///
/// `project` is which library it lands in: the project's own, or the device's where every project on
/// this machine reaches it. The device's is the wider reach, and the panel says which is which rather
/// than picking for the reader.
///
/// The ack names both: the definition, whose step now points somewhere else, and the library, which
/// has one more action in it.
#[tauri::command]
pub fn automation_action_from_step(
    step: i64,
    project: Option<i64>,
    name: String,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_from_step(step, project, &name)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
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

// ───────────────────────── what a step declares ─────────────────────────

/// **The step a declaration may be written on.** A step that runs a library action is refused.
///
/// Core refuses the *add* already ([`amenbo_core::ops::automation::cfg_add`] and its neighbours):
/// a step pointing at an action declares nothing of its own, and a row written on it would sit
/// unread. What core cannot see is the *edit*, because these doors name a declaration the way the
/// panel holds it — by the step and the name. There is no id on a setting or an input for the panel
/// to send: the two rows an action-backed step has under one name are folded into the one row a
/// screen draws ([`cfg_dto`]), so an id would be ambiguous. Resolved against such a step, a name
/// would find the row that carries its **answer** ([`automation_cfg_answer`]) and rewrite the
/// declaration nobody reads off it — silently. So the refusal is here, at the one place that turns a
/// name into a row.
fn declaring_step(store: &amenbo_core::Store, step_id: i64) -> Result<(), CmdError> {
    let step = read::automation_step(store.read_model().conn(), step_id)?
        .ok_or_else(|| no_step(step_id))?;
    match step.action_id {
        None => Ok(()),
        Some(action_id) => Err(amenbo_core::Error::invalid(format!(
            "step '{step_id}' runs action '{action_id}', so what it declares is the action's — \
             write it there, or give the step a prompt of its own first"
        ))
        .into()),
    }
}

fn no_step(step_id: i64) -> CmdError {
    amenbo_core::Error::not_found(format!("step '{step_id}' not found")).into()
}

/// A declaration this step was expected to carry and does not — the panel naming a row that has
/// since gone, which is what a definition re-read after somebody else's write looks like.
fn undeclared(what: &str, step_id: i64, name: &str) -> CmdError {
    amenbo_core::Error::not_found(format!("step '{step_id}' declares no {what} called '{name}'"))
        .into()
}

/// One of this step's ways out, by the name the panel holds it under. `None` is the unnamed one,
/// which is a row like any other and is named by having no name.
fn exit_row(store: &amenbo_core::Store, step_id: i64, name: Option<&str>) -> Result<i64, CmdError> {
    declaring_step(store, step_id)?;
    let found = read::automation_exit_by_name(
        store.read_model().conn(),
        AutomationOwner::Step,
        step_id,
        name,
    )?;
    found.map(|one| one.id).ok_or_else(|| match name {
        Some(name) => undeclared("way out", step_id, name),
        None => amenbo_core::Error::not_found(format!(
            "step '{step_id}' declares no unnamed way out"
        ))
        .into(),
    })
}

/// One of this step's settings, by name.
fn cfg_row(store: &amenbo_core::Store, step_id: i64, name: &str) -> Result<i64, CmdError> {
    declaring_step(store, step_id)?;
    let found =
        read::automation_cfg_by_name(store.read_model().conn(), AutomationOwner::Step, step_id, name)?;
    found.map(|one| one.id).ok_or_else(|| undeclared("setting", step_id, name))
}

/// One of this step's inputs, by name.
fn input_row(store: &amenbo_core::Store, step_id: i64, name: &str) -> Result<i64, CmdError> {
    declaring_step(store, step_id)?;
    let found = read::automation_port_by_name(
        store.read_model().conn(),
        AutomationPortOwner::Step,
        step_id,
        AutomationPortDirection::In,
        name,
    )?;
    found.map(|one| one.id).ok_or_else(|| undeclared("input", step_id, name))
}

/// The kind a setting takes its answer as, refused here rather than stored: no row may exist whose
/// kind no control knows how to draw.
fn cfg_kind(word: &str) -> Result<AutomationCfgKind, CmdError> {
    AutomationCfgKind::parse(word).ok_or_else(|| {
        amenbo_core::Error::invalid(format!(
            "'{word}' is not a kind a setting can be (taskfilter, folder, choice, number, text)"
        ))
        .into()
    })
}

/// What a port carries, refused here for [`cfg_kind`]'s reason. Asked by every door that takes one
/// from a screen — an input declared on a step, an output declared on a way out, and the inputs a
/// step arrives with — so all three refuse in the same words.
fn port_kind(word: &str) -> Result<AutomationPortKind, CmdError> {
    AutomationPortKind::parse(word).ok_or_else(|| {
        amenbo_core::Error::invalid(format!(
            "'{word}' is not something a port carries — value, file, task_take or task_make"
        ))
        .into()
    })
}

/// **Declare another way out of this step.** Every step is born carrying the unnamed one and the
/// error one, so this is the second and every one after it — and `*` is refused as a name, that one
/// being carried already ([`amenbo_core::ops::automation::exit_add`]).
#[tauri::command]
pub fn automation_exit_declare(step_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        declaring_step(store, step_id)?;
        store.automation_exit_add(AutomationOwner::Step, step_id, Some(&name))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Rename one way out**, `to` being `null` for the unnamed one.
///
/// **Every edge and every wire that named the old name is parted from it.** They name a way out by
/// name, and core leaves them pointing at a name nobody declares rather than rewriting the graph
/// around them — the parting is then visible in the picture, which is where a reader can act on it
/// ([`amenbo_core::ops::automation::exit_rename`]).
#[tauri::command]
pub fn automation_exit_rename(
    step_id: i64,
    from: Option<String>,
    to: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let id = exit_row(store, step_id, from.as_deref())?;
        store.automation_exit_rename(id, to.as_deref())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Take one way out away**, with the outputs declared on it. The error one is refused by core:
/// every step carries it whether or not a row says so.
#[tauri::command]
pub fn automation_exit_remove(step_id: i64, name: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let id = exit_row(store, step_id, name.as_deref())?;
        store.automation_exit_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Declare a setting on this step** — the name it is answered under, the kind of answer it takes,
/// and whether it has to be answered. `options` is the choice list, as JSON, and belongs to
/// `choice` alone.
#[tauri::command]
pub fn automation_cfg_declare(
    step_id: i64,
    name: String,
    kind: String,
    required: bool,
    options: Option<String>,
) -> Result<WriteAck, CmdError> {
    let kind = cfg_kind(&kind)?;
    with_store_mut(|store| {
        declaring_step(store, step_id)?;
        store.automation_cfg_add(
            AutomationOwner::Step,
            step_id,
            &name,
            kind,
            required,
            options.as_deref(),
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Change a setting's declaration.** Only what is `Some` is written, and `name` names the row
/// while `rename` is what it becomes.
///
/// `options` is a field with a third answer — written, cleared, or left alone — carried as the pair
/// `options` / `clear_options` for the reason [`automation_step_edit`]'s model is
/// (`Option<Option<..>>` does not cross the IPC boundary as a shape a screen can write).
/// **Moving a `choice` to another kind has to clear the list in the same call**: core refuses a
/// choice list on a kind that would never show it.
#[tauri::command]
pub fn automation_cfg_edit(
    step_id: i64,
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    required: Option<bool>,
    options: Option<String>,
    clear_options: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let kind = kind.as_deref().map(cfg_kind).transpose()?;
    let options = match (clear_options, options.as_deref()) {
        (Some(true), _) => Some(None),
        (_, Some(options)) => Some(Some(options)),
        _ => None,
    };
    with_store_mut(|store| {
        let id = cfg_row(store, step_id, &name)?;
        store.automation_cfg_update(id, rename.as_deref(), kind, required, options)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Take a setting away**, with the answer written on it.
#[tauri::command]
pub fn automation_cfg_remove(step_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let id = cfg_row(store, step_id, &name)?;
        store.automation_cfg_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Declare an input on this step** — what it takes in, and whether a run may open it with nothing
/// reaching that input. An output belongs to the way out that produced it and is not sayable here
/// ([`amenbo_core::ops::automation::port_add`]).
#[tauri::command]
pub fn automation_input_declare(
    step_id: i64,
    name: String,
    kind: String,
    required: bool,
) -> Result<WriteAck, CmdError> {
    let kind = port_kind(&kind)?;
    with_store_mut(|store| {
        declaring_step(store, step_id)?;
        store.automation_port_add(
            AutomationPortOwner::Step,
            step_id,
            AutomationPortDirection::In,
            &name,
            kind,
            required,
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Change an input's declaration.** Only what is `Some` is written; renaming parts every wire that
/// named the old name, for [`automation_exit_rename`]'s reason.
#[tauri::command]
pub fn automation_input_edit(
    step_id: i64,
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    required: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let kind = kind.as_deref().map(port_kind).transpose()?;
    with_store_mut(|store| {
        let id = input_row(store, step_id, &name)?;
        store.automation_port_update(id, rename.as_deref(), kind, required)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Take an input away.** The wires that fed it are left where they are, parted.
#[tauri::command]
pub fn automation_input_remove(step_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let id = input_row(store, step_id, &name)?;
        store.automation_port_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
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
        ports.push((name, port_kind(&kind)?, required));
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
    let kind = port_kind(&kind)?;
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

/// One of core's reasons, in the shape a refusal's own parts travel in: the code naming the sentence,
/// the values it is built from, and core's English underneath ([`crate::dto::AutomationLaunchBlockDto`]
/// says why the two are one shape). The words are the front end's.
fn block_dto(unmet: &Unmet) -> AutomationLaunchBlockDto {
    // Built from the very message the refusal would carry, rather than from a second reading of the
    // reason: the code, the values and the English are one answer, and asking `Unmet` twice is how the
    // two came apart before.
    let msg = unmet.msg();
    AutomationLaunchBlockDto {
        code: msg.code().map_or_else(String::new, |code| code.as_str().to_string()),
        message_en: msg.en().to_string(),
        fields: msg.fields().iter().map(|(key, value)| (key.to_string(), value.to_string())).collect(),
    }
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
/// **The run is opened by the watch, not here.** What opens a step is the one thread looking at what
/// is running (`AMB-D-945`), so no entrance into a run carries its own copy of "and then open the
/// next one". This press only nudges that thread ([`crate::automation_watch::wake`]), so the pane is
/// stood at once rather than at the end of its wait.
#[tauri::command]
pub fn automation_launch(
    id: i64,
    agents: Option<Vec<String>>,
    workspace_open: bool,
) -> Result<AutomationRunStartedDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_launch");
    // The models are read here, as the check reads them (`automation_launch_check`): the press is
    // inside the process that keeps the answers, so a step naming a model its agent does not have is
    // refused at the press rather than met inside the pane the run just opened.
    let offered = crate::agent_models::offered_here();
    let by = automation_run::Launcher {
        startable: agents.as_deref(),
        models: &offered,
        workspace_open: Some(workspace_open),
        by: Some(ActorKind::Human),
    };
    let run = with_store_mut(|store| Ok(store.automation_launch(id, &by)?))?;
    crate::automation_watch::wake();
    Ok(AutomationRunStartedDto { run: run.id })
}

/// **How many stopped runs the "running" tab is shown.** A stop is kept on the list so that a failure
/// nobody was watching is still seen — not so that every failure since the store was made is listed.
/// What a run did months ago is read from the task it worked ([`amenbo_core::store::Store::automation_runs_for_task`]).
const STOPPED_SHOWN: usize = 20;

/// **What is under way right now**, across every project — the rows of the "running" tab.
///
/// It crosses projects because a terminal does — what a run holds is a terminal on this machine, and
/// this machine is not divided up per project. Runs that are `done` are not here: what a finished
/// run did is reached from the task it worked or the automation it came from, never searched for
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
    open_one(&app, &mut store, run_id, def_id)
}

/// **Pause a run** — it settles at the end of the step under way
/// ([`amenbo_core::ops::automation_stop::pause`]). Pressed on a row of the "running" tab.
///
/// **It is not a `WriteAck` write**, for the reason [`automation_run_stop`] is not: what it moves is a
/// run, and every screen drawing one is already following the change feed.
#[tauri::command]
pub fn automation_run_pause(run_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_pause(run_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Pick a paused run up again** ([`amenbo_core::ops::automation_stop::resume`]).
///
/// **The step it picks up at is not opened here** (`AMB-D-945`). `resume` writes `running` and the
/// run then looks exactly like every other run standing between two steps — which the watch reads off
/// what the run has already done ([`amenbo_core::ops::automation_run::next_def`]), the same answer
/// `resume` worked out to decide whether it could go on at all. Opening it here would be that answer
/// arrived at twice.
#[tauri::command]
pub fn automation_run_resume(run_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_resume(run_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// One step opened and told to the window.
///
/// The answer and the event carry the same thing. The event is what the workspace acts on, and the
/// answer is for the caller to know what happened — a run stopped for a missing input opens no
/// terminal, and whoever asked for it is owed that sentence.
fn open_one(
    app: &tauri::AppHandle,
    store: &mut amenbo_core::Store,
    run_id: i64,
    def_id: i64,
) -> Result<AutomationStepOpenDto, CmdError> {
    // What this machine can start, as the device's settings last had it from a probe
    // (`crate::wake`). Taken before the write because the store is borrowed for it, and `None` where
    // nothing has ever probed — which is nobody asked, not "nothing is installed" (`AMB-D-792`).
    let startable: Option<Vec<String>> = store.config.installed_agents().map(<[String]>::to_vec);
    let opened = store.automation_step_open(run_id, def_id, startable.as_deref())?;
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
                    task: worked_task(store, ready.run_step.run_task_id)?,
                    say: ready.text.clone(),
                    agent: def.agent.clone(),
                    model: def.model.clone(),
                    folder: ready.folder.clone(),
                    interactive: def.interactive,
                }),
                Vec::new(),
            )
        }
        // What a step's pane is told about is its own step. No other run is in the event and none is
        // opened here: what the watch looks for is a run `running` with nothing open (`AMB-D-945`).
        Opened::Stopped { run, missing, .. } => (run.project_id, None, missing),
        // Nothing is named in the event for this one. `missing` is about inputs, and what a reader is
        // owed here is on the run's own row — the running tab says which ending it was, and the line
        // core left on the task says how far it got (`amenbo_core::ops::automation_stop`).
        Opened::NoAgent { run, agent } => {
            log::info!("run {run_id} asked for {agent}, which this machine cannot start");
            (run.project_id, None, Vec::new())
        }
    };
    // The run has just moved, so the thread that keeps it going looks again now rather than sleeping
    // out the interval it was on (`crate::automation_watch`). Called from the watch's own path too,
    // where it costs nothing: that loop is about to come round anyway.
    crate::automation_watch::wake();
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
/// (`app/src/shell/TerminalPane.tsx`), and what the "running" tab's third button presses.
///
/// The cleanup is core's and is the same one every other stop goes through
/// ([`amenbo_core::ops::automation_stop::stop`]): the task the run was working goes to `todo`, and
/// a line on that task says the run is not coming back. The terminal
/// standing in the pane is the pane's own to end — it is a process this side started, and core has
/// no window to end one from.
///
/// **A run that is over already is not an error here.** The pane is closed by a person, and between
/// the last step reporting and the press there is a window in which the run has finished on its own;
/// a refusal then would put a red sentence in front of somebody who did nothing wrong. What comes
/// back says whether this press was the one that stopped it.
///
#[tauri::command]
pub fn automation_run_stop(run_id: i64) -> Result<bool, CmdError> {
    let mut store = crate::commands::open_store()?;
    if stop_if_going(&mut store, run_id)?.is_none() {
        return Ok(false);
    }
    crate::automation_watch::wake();
    Ok(true)
}

/// The half of the stop that has no window in it: stop the run where it is still going, and answer
/// `None` where there was nothing to stop. Split out so the "already over" arm can be tested without
/// an app to hand ([`automation_run_stop`] is the whole of it).
fn stop_if_going(
    store: &mut amenbo_core::Store,
    run_id: i64,
) -> Result<Option<Ended>, CmdError> {
    let going = match read::automation_run(store.read_model().conn(), run_id)? {
        Some(run) => {
            matches!(run.status, AutomationRunStatus::Running | AutomationRunStatus::Paused)
        }
        // A run nobody can find is one nothing can be stopped about, and the pane is going either
        // way. Saying so is the whole of what is left to do.
        None => false,
    };
    if !going {
        return Ok(None);
    }
    Ok(Some(store.automation_stop(run_id, AutomationStoppedReason::ByHuman)?))
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

        let mut store = crate::commands::open_store().expect("the store");
        assert!(stop_if_going(&mut store, 404)
            .expect("a run nobody can find is not an error")
            .is_none());
    }
}
