//! **Reading an automation's definition, saying whether it could be started, and running one** — the
//! doors behind the automations screen, its build screen, the "running" tab and the pane a run is
//! drawn in.
//!
//! Core owns the eleven definition tables, the writes that build them
//! ([`amenbo_core::ops::automation`]) and the read that resolves them
//! ([`amenbo_core::ops::automation_view`]) — a spot's ways out, its inputs and its settings come
//! already read off the library action standing on it. Nothing is built or resolved here. What this
//! side does is name: the same rows under the names a screen draws them by.
//!
//! **Three layers, and this side names the one the picture is drawn in** (`AMB-D-949`). A box on the
//! build screen is a placement; what it declares is the action's, and what it runs on is that
//! action's step. A door here takes whichever of the three the write belongs to, so a screen never
//! has to walk from one to another to find out where a field lives.
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
    ActorKind, AutomationCfg, AutomationCfgKind, AutomationCfgOwner, AutomationOwner,
    AutomationPictureOwner, AutomationPort, AutomationPortDirection, AutomationPortKind,
    AutomationPortOwner, AutomationRunStatus, AutomationStoppedReason,
};
use amenbo_core::ops::automation::{NewAutomation, NewStep};
use amenbo_core::ops::automation_run::{self, Unmet};
use amenbo_core::ops::automation_stop::Ended;
use amenbo_core::ops::automation_step::Opened;
use amenbo_core::ops::automation_view;
use amenbo_core::store_engine::read;

use crate::commands::{open_store_read, with_store_mut};
use crate::dto::{
    AutomationActionCardDto, AutomationCardDto, AutomationCfgDto, AutomationDetailDto,
    AutomationEdgeDto, AutomationExitDto, AutomationLaunchBlockDto, AutomationLaunchCheckDto,
    AutomationPlacementDto, AutomationPortDto, AutomationRunCardDto, AutomationRunStartedDto,
    AutomationRunTaskDto, AutomationStepOpenDto, AutomationStepRunDto, AutomationWireDto, WriteAck,
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
            placements: card.placements,
            archived: card.automation.archived,
        })
        .collect())
}

/// **Make an automation**, born with no steps and no entry
/// ([`amenbo_core::ops::automation::add`]).
///
/// A name is all it takes. The notes are written on the build screen, once there is a picture to
/// write them about — asking for them at the press would put a form in front of the one road into
/// the screen where the work actually happens. The preamble is written nowhere: it is Amenbo's own
/// fixed sentence at the head of every launch, and the column goes with it (`AMB-T-5325`).
///
/// The ack names the new automation, which is what the screen opens the build screen on: a creation
/// that answered with the scope alone would leave the press having to go and find the row that was
/// not there a moment ago.
#[tauri::command]
pub fn automation_add(project_id: i64, name: String) -> Result<WriteAck, CmdError> {
    let made = with_store_mut(|store| {
        Ok(store.automation_add(project_id, NewAutomation { name, ..Default::default() })?)
    })?;
    Ok(WriteAck::new(&["automations"]).automation(made.id))
}

/// **Rename an automation, rewrite its notes, or put it out of the way.** Only what is `Some` is
/// written.
///
/// Archiving takes nothing away and stops nothing already running
/// ([`amenbo_core::ops::automation::update`]). It is what keeps a definition nobody launches any
/// more out of a reader's way, so the row stays in the list carrying the mark rather than leaving
/// it — which is why `automation_page` goes on answering with archived ones in it.
///
/// The preamble is not one of the three. It is the fixed sentence Amenbo puts at the head of every
/// launch rather than anything this automation holds, and the column goes with it (`AMB-T-5325`).
#[tauri::command]
pub fn automation_edit(
    id: i64,
    name: Option<String>,
    notes: Option<String>,
    archived: Option<bool>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_update(id, name.as_deref(), notes.as_deref(), None, archived)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Delete an automation and everything built into it** — its steps with their declarations, the
/// edges and wires between them, and the documents they share
/// ([`amenbo_core::ops::automation::delete`]).
///
/// **Core refuses it while a run stands behind it**, naming how many. A run carries its own copy of
/// the steps and would go on reading correctly, but it is filed under the automation it was
/// launched from — so the refusal is what keeps the record able to say what was run. It reaches the
/// screen as the sentence core wrote, which is why nothing is re-asked here before the write.
#[tauri::command]
pub fn automation_remove(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **The library this project reaches** — the device's own actions first, then the project's own.
///
/// The two libraries answer as one list because they are one list on screen: what a reader is
/// choosing between is every action this automation could place, and which of the two holds one is a
/// column of that list rather than a second list to go and look in.
#[tauri::command]
pub fn automation_action_page(project_id: i64) -> Result<Vec<AutomationActionCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_action_page");
    let store = open_store_read()?;
    let conn = store.read_model().conn();
    let cards = automation_view::action_cards(conn, Some(project_id))?;
    let mut out = Vec::with_capacity(cards.len());
    for card in cards {
        // The prompt on the row is the one the action opens with. An action of several steps has
        // more than one, and the row then names the first — the whole list is the action's own
        // screen's (`AMB-T-5315`).
        let opens = match card.action.entry_step_id {
            Some(id) => read::automation_action_step(conn, id)?,
            None => None,
        };
        out.push(AutomationActionCardDto {
            id: card.action.id,
            name: card.action.name,
            prompt: opens.as_ref().map(|s| s.prompt.clone()).unwrap_or_default(),
            entry_step_id: opens.map(|s| s.id),
            steps: card.steps,
            // The shelf travels as the fact the screen draws rather than as the project id: a screen
            // inside one project would only ever read an id back as "mine" or "the device's".
            global: card.action.project_id.is_none(),
            used_by: card.used_by,
        });
    }
    Ok(out)
}

/// **Make a library action** — a name, and which library it lands in.
///
/// **The prompt is not asked for here.** It is written in the box the list opens on the row
/// ([`automation_action_edit`]), which is the one place a prompt is written: a second field writing
/// the same text would be a second place to keep in step.
///
/// It is born holding one step, and that step is what the action opens — so the box the screen opens
/// on the new row has a row to write the prompt on. Who is asked to carry it out is not settled here
/// either, and until somebody picks an agent the run check says so
/// ([`amenbo_core::ops::automation_run::Unmet::AgentMissing`]). An action of several steps is written
/// from its own screen (`AMB-T-5315`).
///
/// `project` is which library it lands in — the project's own, or the device's where every project
/// on this machine reaches it.
#[tauri::command]
pub fn automation_action_add(project: Option<i64>, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_from_prompt(project, NewStep::new(&name, "", ""), &[], &[])?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automationActions"]))
}

/// **Rename a library action, and rewrite the prompt the step it opens runs on.** Only what is
/// `Some` is written, and `step` names the row the prompt is written on — the action's own entry, as
/// the listing hands it back.
///
/// The rewrite reaches every placement of this action, which is what the library is for — and what
/// the screen says before the box is opened. It does not reach a run already under way: a run takes
/// its copy at the launch ([`amenbo_core::ops::automation_run`]), so what a running step carries is
/// settled and this cannot reach back into it.
#[tauri::command]
pub fn automation_action_edit(
    id: i64,
    name: Option<String>,
    step: Option<i64>,
    prompt: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_update(id, name.as_deref())?;
        if let (Some(step), Some(prompt)) = (step, prompt.as_deref()) {
            store.automation_step_update(
                step,
                None,
                Some(prompt),
                None,
                None,
                None,
                None,
                None,
                None,
            )?;
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Change the step one library action opens.** Only what is `Some` is written.
///
/// The fields are the step's, not the placement's: a prompt, who is asked to carry it out, the model
/// and the three flags all belong to the terminal that is stood up, and an action holds the steps
/// (`AMB-D-950`). Writing one reaches every placement of that action, which is what the library is
/// for.
///
/// `model` and `work_dir` are each a field with a third answer: written, cleared, or left alone. The
/// pair of arguments says which — `clear_model` beats a `model` beside it, and the same for the
/// folder — rather than a single `Option<Option<..>>`, which does not cross the IPC boundary as a
/// shape a screen can write.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_step_edit(
    id: i64,
    name: Option<String>,
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
            prompt.as_deref(),
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

/// **Take one action off a picture**, with the answers written on it and every line naming it. The
/// action itself is untouched: the library outlives any one picture.
#[tauri::command]
pub fn automation_placement_remove(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_placement_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

// ───────────────────────── what an action declares ─────────────────────────
//
// A way out, a port and a setting are the action's, never the placement's — which is what lets one
// action be placed twice and read the same both times. So every door below names the action, and the
// one door that writes an answer names the placement (`automation_cfg_answer`).
//
// There is no id on a setting or an input for a screen to send: the two rows a setting has — the
// action's declaration and each placement's answer — are folded into the one row a screen draws
// (cfg_dto), so a name is what turns into a row here.

/// A declaration this action was expected to carry and does not — the panel naming a row that has
/// since gone, which is what a definition re-read after somebody else's write looks like.
fn undeclared(what: &str, action_id: i64, name: &str) -> CmdError {
    amenbo_core::Error::not_found(format!("action '{action_id}' declares no {what} called '{name}'"))
        .into()
}

/// One of this action's ways out, by the name the panel holds it under. `None` is the unnamed one,
/// which is a row like any other and is named by having no name.
fn exit_row(
    store: &amenbo_core::Store,
    action_id: i64,
    name: Option<&str>,
) -> Result<i64, CmdError> {
    let found = read::automation_exit_by_name(
        store.read_model().conn(),
        AutomationOwner::Action,
        action_id,
        name,
    )?;
    found.map(|one| one.id).ok_or_else(|| match name {
        Some(name) => undeclared("way out", action_id, name),
        None => amenbo_core::Error::not_found(format!(
            "action '{action_id}' declares no unnamed way out"
        ))
        .into(),
    })
}

/// One of this action's settings, by name.
fn cfg_row(store: &amenbo_core::Store, action_id: i64, name: &str) -> Result<i64, CmdError> {
    let found = read::automation_cfg_by_name(
        store.read_model().conn(),
        AutomationCfgOwner::Action,
        action_id,
        name,
    )?;
    found.map(|one| one.id).ok_or_else(|| undeclared("setting", action_id, name))
}

/// One of this action's inputs, by name.
fn input_row(store: &amenbo_core::Store, action_id: i64, name: &str) -> Result<i64, CmdError> {
    let found = read::automation_port_by_name(
        store.read_model().conn(),
        AutomationPortOwner::Action,
        action_id,
        AutomationPortDirection::In,
        name,
    )?;
    found.map(|one| one.id).ok_or_else(|| undeclared("input", action_id, name))
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

/// **Declare another way out of this action.** Every action is born carrying the unnamed one and the
/// error one, so this is the second and every one after it — and `*` is refused as a name, that one
/// being carried already ([`amenbo_core::ops::automation::exit_add`]).
#[tauri::command]
pub fn automation_exit_declare(action_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_exit_add(AutomationOwner::Action, action_id, Some(&name))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Rename one way out**, `to` being `null` for the unnamed one.
///
/// **Every edge and every wire that named the old name is parted from it.** They name a way out by
/// name, and core leaves them pointing at a name nobody declares rather than rewriting the graph
/// around them — the parting is then visible in the picture, which is where a reader can act on it
/// ([`amenbo_core::ops::automation::exit_rename`]).
#[tauri::command]
pub fn automation_exit_rename(
    action_id: i64,
    from: Option<String>,
    to: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let id = exit_row(store, action_id, from.as_deref())?;
        store.automation_exit_rename(id, to.as_deref())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Take one way out away**, with the outputs declared on it. The error one is refused by core:
/// every action carries it whether or not a row says so.
#[tauri::command]
pub fn automation_exit_remove(action_id: i64, name: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let id = exit_row(store, action_id, name.as_deref())?;
        store.automation_exit_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Declare a setting on this action** — the name it is answered under, the kind of answer it takes,
/// and whether it has to be answered. `options` is the choice list, as JSON, and belongs to
/// `choice` alone. Each placement of the action answers it on a row of its own.
#[tauri::command]
pub fn automation_cfg_declare(
    action_id: i64,
    name: String,
    kind: String,
    required: bool,
    options: Option<String>,
) -> Result<WriteAck, CmdError> {
    let kind = cfg_kind(&kind)?;
    with_store_mut(|store| {
        store.automation_cfg_add(action_id, &name, kind, required, options.as_deref())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
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
    action_id: i64,
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
        let id = cfg_row(store, action_id, &name)?;
        store.automation_cfg_update(id, rename.as_deref(), kind, required, options)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Take a setting away**, leaving the answers written for it on the placements it was declared to.
#[tauri::command]
pub fn automation_cfg_remove(action_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let id = cfg_row(store, action_id, &name)?;
        store.automation_cfg_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Declare an input on this action** — what it takes in, and whether a run may open a placement of
/// it with nothing reaching that input. An output belongs to the way out that produced it and is not
/// sayable here ([`amenbo_core::ops::automation::port_add`]).
#[tauri::command]
pub fn automation_input_declare(
    action_id: i64,
    name: String,
    kind: String,
    required: bool,
) -> Result<WriteAck, CmdError> {
    let kind = port_kind(&kind)?;
    with_store_mut(|store| {
        store.automation_port_add(
            AutomationPortOwner::Action,
            action_id,
            AutomationPortDirection::In,
            &name,
            kind,
            required,
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Change an input's declaration.** Only what is `Some` is written; renaming parts every wire that
/// named the old name, for [`automation_exit_rename`]'s reason.
#[tauri::command]
pub fn automation_input_edit(
    action_id: i64,
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    required: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let kind = kind.as_deref().map(port_kind).transpose()?;
    with_store_mut(|store| {
        let id = input_row(store, action_id, &name)?;
        store.automation_port_update(id, rename.as_deref(), kind, required)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Take an input away.** The wires that fed it are left where they are, parted.
#[tauri::command]
pub fn automation_input_remove(action_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let id = input_row(store, action_id, &name)?;
        store.automation_port_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Answer one setting on one placement**, or leave it unanswered with no `value`.
///
/// The answer travels as the JSON its kind takes — a string for a folder, a choice and a text, a
/// number for a number, and an object naming each part of a task filter. The shape is the screen's to
/// build, because the control that took it is the screen's too
/// (`app/src/screens/automationCfg.ts`); core keeps the text as it is handed and the run reads it
/// ([`amenbo_core::ops::automation::cfg_set`]).
///
/// Each placement answers on a row of its own under the declared name, so one action placed twice is
/// not answered for both at once. That split is core's, and this door does not have to know which of
/// the two rows it is writing.
#[tauri::command]
pub fn automation_cfg_answer(
    placement_id: i64,
    name: String,
    value: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_cfg_set(placement_id, &name, value.as_deref())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Put an action in on a line** — the one road by which a box joins a picture already drawn.
///
/// `action` places one already in the library. With a `prompt` instead, an action is written from it
/// first and placed: what stands on a picture is always a placement of an action, and writing the
/// prompt where the reader is looking is what keeps that from being two screens (`AMB-T-5317`).
///
/// `inputs` is a flat list of triples the screen sends as `[name, kind, required]`, because a struct
/// per row would be one more shape to keep in step across the boundary for three fields. An unknown
/// kind is refused here rather than stored: the four are the port kinds core knows
/// ([`amenbo_core::model::AutomationPortKind`]).
///
/// The whole press is one transaction, edges and all: a box left behind with the line still running
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
    let mut ports = Vec::with_capacity(inputs.len());
    for (name, kind, required) in inputs {
        ports.push((name, port_kind(&kind)?, required));
    }
    match (action, prompt) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(amenbo_core::Error::invalid(
                "say what stands here — a library action, or a prompt to write one from",
            )
            .into())
        }
        (Some(action), None) => {
            with_store_mut(|store| {
                store.automation_placement_insert(edge_id, action)?;
                Ok(())
            })?;
        }
        (None, Some(prompt)) => {
            let new = NewStep {
                name,
                prompt,
                agent,
                model,
                interactive,
                work_dir_ref: None,
                report_to_task: false,
                show_history: true,
            };
            with_store_mut(|store| {
                store.automation_placement_insert_from_prompt(edge_id, new, &exits, &ports)?;
                Ok(())
            })?;
        }
    }
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

/// **Say what fills one of a spot's inputs**, by naming the way out and the output it comes from.
///
/// Drawing the same wire twice answers the one already drawn rather than writing a second row
/// ([`amenbo_core::ops::automation::wire_add`]), so the screen's control can send what the reader
/// picked without first working out whether anything was there.
#[tauri::command]
pub fn automation_wire_set(
    from_placement_id: i64,
    from_exit_name: Option<String>,
    from_port_name: String,
    to_placement_id: i64,
    to_port_name: String,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_wire_add(
            AutomationPictureOwner::Automation,
            from_placement_id,
            from_exit_name.as_deref(),
            &from_port_name,
            to_placement_id,
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
        entry_placement_id: a.entry_placement_id,
        archived: a.archived,
        placements: view.placements.into_iter().map(placement_dto).collect(),
        edges: view
            .edges
            .into_iter()
            .map(|edge| AutomationEdgeDto {
                id: edge.id,
                from_placement_id: edge.from_id,
                exit_name: edge.exit_name,
                to_placement_id: edge.to_id,
                ends: edge.ends.as_str(),
                max_times: edge.max_times,
            })
            .collect(),
        wires: view
            .wires
            .into_iter()
            .map(|wire| AutomationWireDto {
                id: wire.id,
                from_placement_id: wire.from_id,
                from_exit_name: wire.from_exit_name,
                from_port_name: wire.from_port_name,
                to_placement_id: wire.to_id,
                to_port_name: wire.to_port_name,
            })
            .collect(),
    }
}

/// One spot on the picture: the action standing there, and the step it opens first flattened onto it
/// so the panel writes through one shape.
///
/// **Flattened because an action opens one step today.** Drawing the whole column inside an action is
/// its own screen's (`AMB-T-5315`); until there is one, a spot reads exactly as the step it was folded
/// out of did.
fn placement_dto(view: automation_view::PlacementView) -> AutomationPlacementDto {
    let placement = view.placement;
    let action = view.action;
    let opens = view.entry_step;
    AutomationPlacementDto {
        id: placement.id,
        name: action.as_ref().map(|one| one.name.clone()).unwrap_or_default(),
        action_id: placement.action_id,
        step_id: opens.as_ref().map(|s| s.id),
        prompt: opens.as_ref().map(|s| s.prompt.clone()).unwrap_or_default(),
        agent: opens.as_ref().map(|s| s.agent.clone()).unwrap_or_default(),
        model: opens.as_ref().and_then(|s| s.model.clone()),
        interactive: opens.as_ref().is_some_and(|s| s.interactive),
        work_dir_ref: opens.as_ref().and_then(|s| s.work_dir_ref.clone()),
        report_to_task: opens.as_ref().is_some_and(|s| s.report_to_task),
        show_history: opens.as_ref().map_or(true, |s| s.show_history),
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
