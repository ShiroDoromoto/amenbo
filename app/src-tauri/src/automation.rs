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
    ActorKind, AutomationCfg, AutomationCfgKind, AutomationCfgOwner, AutomationEdge,
    AutomationOwner, AutomationPictureOwner, AutomationPort, AutomationPortDirection,
    AutomationPortKind, AutomationPortOwner, AutomationRunStatus,
    AutomationWire,
};
use amenbo_core::ops::automation::{ActionShelf, EdgeTarget, NewAutomation, NewStep};
use amenbo_core::ops::automation_builtin;
use amenbo_core::ops::automation_run::{self, Unmet};
use amenbo_core::ops::automation_stop::Ending;
use amenbo_core::ops::automation_stop::Ended;
use amenbo_core::ops::automation_step::Opened;
use amenbo_core::ops::automation_view;
use amenbo_core::store_engine::read;

use crate::commands::{open_store_read, with_store_mut};
use crate::dto::{
    AutomationActionCardDto, AutomationActionDetailDto, AutomationBuiltinDto,
    AutomationBuiltinExitDto, AutomationBuiltinRunDto, AutomationCardDto, AutomationCfgDto,
    AutomationDetailDto, AutomationEdgeDto, AutomationExitDto, AutomationLaunchAsksDto,
    AutomationLaunchAxisDto, AutomationLaunchBlockDto,
    AutomationLaunchCheckDto, AutomationPlacedOnDto, AutomationPlacementDto,
    AutomationPlacementStepDto, AutomationPortDto, AutomationRunCardDto, AutomationRunEndingsDto,
    AutomationRunHistoryDto, AutomationRunStartedDto, AutomationRunTaskDto, AutomationStepDto,
    AutomationStepOpenDto, AutomationStepRunDto, AutomationWireDto, EveryAutomationCardDto,
    WriteAck,
};
use crate::error::CmdError;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

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
            notes: card.automation.notes,
            placements: card.placements,
            archived: card.automation.archived,
        })
        .collect())
}

/// **The automations of every project**, project by project in the sidebar's order — the list the
/// sidebar's "automations" tab draws ([`automation_view::every_card`]).
///
/// It is the one read the screen has that crosses projects on the definition side, so each row
/// carries its project. The window holds every project, so nothing narrows the walk.
#[tauri::command]
pub fn automation_page_everywhere() -> Result<Vec<EveryAutomationCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_page_everywhere");
    let store = open_store_read()?;
    let cards = automation_view::every_card(store.read_model().conn(), None)?;
    Ok(cards
        .into_iter()
        .map(|one| EveryAutomationCardDto {
            project_id: one.card.automation.project_id,
            project_name: one.project_name,
            card: AutomationCardDto {
                id: one.card.automation.id,
                name: one.card.automation.name,
                notes: one.card.automation.notes,
                placements: one.card.placements,
                archived: one.card.automation.archived,
            },
        })
        .collect())
}

/// **Make an automation**, born with no steps and no entry
/// ([`amenbo_core::ops::automation::add`]).
///
/// A name is all it takes. The notes are written on the build screen, once there is a picture to
/// write them about — asking for them at the press would put a form in front of the one road into
/// the screen where the work actually happens. The preamble is written nowhere: it is Amenbo's own
/// fixed sentences at the head of every launch ([`amenbo_core::agents::preamble`]).
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
/// Archiving takes nothing away, and like every rewrite it is refused while a run of the automation is
/// going ([`amenbo_core::ops::automation::update`]). It is what keeps a definition nobody launches any
/// more out of a reader's way, so the row stays in the list carrying the mark rather than leaving
/// it — which is why `automation_page` goes on answering with archived ones in it.
///
/// The preamble is not one of the three. It is what Amenbo puts at the head of every launch rather
/// than anything this automation holds ([`amenbo_core::agents::preamble`]).
#[tauri::command]
pub fn automation_edit(
    id: i64,
    name: Option<String>,
    notes: Option<String>,
    archived: Option<bool>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_update(id, name.as_deref(), notes.as_deref(), archived)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Change what a run starts at**, to another of the built-ins it can start at
/// ([`amenbo_core::ops::automation::entry_replace`], `AMB-D-977`). `key` is the built-in's key.
///
/// There is no press that names a placement as the entry: the first thing put on a picture is it, and
/// this is the one way to change it afterwards. The lines out of the old one go with it, so the build
/// screen asks before pressing it; the placements after it stay.
#[tauri::command]
pub fn automation_entry_replace(automation_id: i64, key: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_entry_replace(automation_id, &key)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Delete an automation and everything built into it** — its steps with their declarations and
/// the edges and wires between them ([`amenbo_core::ops::automation::delete`]).
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
///
/// `project_id` `None` is the device's library alone — what the sidebar's "actions" tab lists, since a
/// project's own actions are made and changed from that project (`AMB-D-954`).
#[tauri::command]
pub fn automation_action_page(
    project_id: Option<i64>,
) -> Result<Vec<AutomationActionCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_action_page");
    let store = open_store_read()?;
    let conn = store.read_model().conn();
    let cards = automation_view::action_cards(conn, project_id)?;
    let mut out = Vec::with_capacity(cards.len());
    // A built-in's library action is kept on the device's shelf, but it is not one of the device's
    // actions: it is listed under its own head, from the definition (`automation_builtin_page`).
    for card in cards.into_iter().filter(|card| card.action.builtin.is_none()) {
        out.push(AutomationActionCardDto {
            id: card.action.id,
            name: card.action.name,
            note: card.action.note,
            steps: card.steps,
            // The shelf travels as the fact the screen draws rather than as the project id: a screen
            // inside one project would only ever read an id back as "mine" or "the device's".
            global: card.action.project_id.is_none(),
            used_by: card.used_by,
        });
    }
    Ok(out)
}

/// **The built-ins** (`AMB-D-964`), in the order the library lists them — read off Amenbo's own
/// definition ([`amenbo_core::ops::automation_builtin::all`]), which is what the library draws under its
/// own head and what opening one reads.
///
/// A built-in's library action is only written the first time one is placed, so the definition is
/// what is listed; the action, where one was written, only says how many automations place it.
#[tauri::command]
pub fn automation_builtin_page() -> Result<Vec<AutomationBuiltinDto>, CmdError> {
    let store = open_store_read()?;
    let conn = store.read_model().conn();
    let mut out = Vec::new();
    for one in automation_builtin::all() {
        // The one that splits by an axis has an action per axis, and an automation placing two of them
        // is still one automation.
        let mut placing = std::collections::BTreeSet::new();
        for action in read::automation_actions_builtin(conn, one.key)? {
            placing.extend(automation_view::automations_placing(conn, action.id)?);
        }
        let used_by = placing.len();
        out.push(AutomationBuiltinDto {
            key: one.key.to_string(),
            name: one.name.to_string(),
            does: one.does.to_string(),
            settings: one
                .settings
                .iter()
                .map(|s| AutomationCfgDto {
                    name: s.name.to_string(),
                    kind: s.kind.as_str(),
                    required: s.required,
                    options: s.options.map(str::to_string),
                    value: None,
                })
                .collect(),
            inputs: one.ins.iter().map(builtin_port_dto).collect(),
            exits: one
                .exits
                .iter()
                .map(|e| AutomationBuiltinExitDto {
                    name: e.name.to_string(),
                    outputs: e.outs.iter().map(builtin_port_dto).collect(),
                })
                .collect(),
            used_by,
        });
    }
    Ok(out)
}

fn builtin_port_dto(port: &automation_builtin::BuiltinPort) -> AutomationPortDto {
    AutomationPortDto { name: port.name.to_string(), kind: port.kind.as_str(), required: port.required }
}

/// **Put a built-in on a picture**, standing on its own — how the first placement comes in, which is
/// one of the built-ins a run starts at (`AMB-D-977`). Its library action is written from the
/// definition the first time any automation places it
/// ([`amenbo_core::ops::automation_builtin::action_on`]). `axis` is the axis the one that splits by an
/// axis splits by, and nothing for any other (`AMB-D-973`).
#[tauri::command]
pub fn automation_builtin_place(automation_id: i64, key: String, axis: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_builtin_place(automation_id, &key, axis)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Put a built-in in on a line** ([`automation_step_insert`] for a built-in) — the way out that was
/// pressed comes to point at the new placement, in one transaction with the library action written
/// where none was yet. `axis` as for [`automation_builtin_place`].
#[tauri::command]
pub fn automation_builtin_insert(edge_id: i64, key: String, axis: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_builtin_insert(edge_id, &key, axis)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Make a library action** — a name, and which library it lands in.
///
/// **It is born empty**, which is what an action born from a name is: no steps, no entry, and the two
/// ways out every declarer carries ([`amenbo_core::ops::automation::action_add`]). The first step is
/// written in the build screen the press lands in ([`automation_step_add`], `AMB-T-5315`), which is
/// where its prompt is given — asking for one here would be asking before there is a step to write it
/// on.
///
/// `project` is which library it lands in — the project's own, or the device's where every project
/// on this machine reaches it.
#[tauri::command]
pub fn automation_action_add(project: Option<i64>, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_add(project, &name, "")?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automationActions"]))
}

/// **Rename a library action, or rewrite what it is for.** Only what is `Some` is written. The name
/// and the note are all that is the action's own to write: the prompt and the flags belong to its
/// steps ([`automation_step_edit`]), who carries each step out to where the action is placed
/// ([`automation_placement_step_set`]), and what it declares has its own doors.
///
/// The note is the automation's notes one layer down: drawn where it is built and on the library's
/// row, never carried into a launch (`AMB-D-952`).
///
/// **Renaming parts nothing.** A placement points at the action by key, so every picture standing on
/// it reads the new name at once. So does renaming one of its ways out, which edges and wires key by
/// its id (`AMB-D-961`); renaming a port parts every wire that named the old one
/// ([`amenbo_core::ops::automation::action_update`]).
#[tauri::command]
pub fn automation_action_edit(id: i64, name: Option<String>, note: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_update(id, name.as_deref(), note.as_deref())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Move a library action to another reach** — `null` to the device's library, a project's id to that
/// project's ([`amenbo_core::ops::automation::action_set_scope`]).
///
/// Into the device's library it always goes. Into a project it goes only while no automation of any
/// other project places it; otherwise core refuses and names each of those automations with its
/// project, which is the sentence the screen puts under the row. **Nothing is copied**: two actions of
/// the same words would part the moment one was rewritten (`AMB-D-954`).
///
/// Every picture standing on the action draws its reach, so the ack moves the automations as well as
/// the library.
#[tauri::command]
pub fn automation_action_set_scope(id: i64, project_id: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_set_scope(id, project_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Delete a library action with everything inside it** — its steps, their declarations and the
/// picture they are drawn into ([`amenbo_core::ops::automation::action_delete`]).
///
/// **Core refuses it while a placement stands on it**, saying how many: the placement would be left
/// standing on nothing, and what should stand there instead is not the list's to guess. The refusal
/// reaches the screen as core's sentence, the way a refused move does.
#[tauri::command]
pub fn automation_action_remove(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automationActions"]))
}

/// **Change the step one library action opens.** Only what is `Some` is written.
///
/// The fields are the step's, not the placement's: a prompt and the flags belong to the terminal
/// that is stood up, and an action holds the steps. Writing one reaches every placement of that action,
/// which is what the library is for. Who carries the step out is not among them — that is chosen where
/// the action is placed ([`automation_placement_step_set`], `AMB-D-960`).
///
/// `work_dir` is a field with a third answer: written, cleared, or left alone. The pair of arguments
/// says which — `clear_work_dir` beats a `work_dir` beside it — rather than a single
/// `Option<Option<..>>`, which does not cross the IPC boundary as a shape a screen can write.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_step_edit(
    id: i64,
    name: Option<String>,
    prompt: Option<String>,
    interactive: Option<bool>,
    work_dir: Option<String>,
    clear_work_dir: Option<bool>,
    report_to_task: Option<bool>,
    history: Option<bool>,
    task_notes: Option<bool>,
    task_decisions: Option<bool>,
    task_comments: Option<bool>,
) -> Result<WriteAck, CmdError> {
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
            interactive,
            work_dir,
            report_to_task,
            history,
            task_notes,
            task_decisions,
            task_comments,
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Add a step to an action**, with the ways out and inputs it is written with.
///
/// It is the one road into an action whose picture is still empty — every other way in is a line to
/// put a step on ([`automation_action_step_insert`]), and an action with no steps has none.
///
/// **A picture with nothing in it takes this step as its entry**, which is the step a placement of
/// the action opens first: the first box of an empty picture is the only one a run could open, and
/// leaving it unnamed would make the press half a press. Naming it is a second write, so an action
/// that gains the step and not the entry is what a refusal there leaves behind — visible on the
/// screen, where the entry is chosen.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_step_add(
    action_id: i64,
    name: String,
    prompt: String,
    interactive: bool,
    exits: Vec<String>,
    inputs: Vec<(String, String, bool)>,
) -> Result<WriteAck, CmdError> {
    let mut ports = Vec::with_capacity(inputs.len());
    for (name, kind, required) in inputs {
        ports.push((name, port_kind(&kind)?, required));
    }
    let new = NewStep {
        name,
        prompt,
        interactive,
        work_dir_ref: None,
        report_to_task: false,
        show_history: true,
        show_notes: true,
        show_decisions: true,
        show_comments: true,
    };
    with_store_mut(|store| {
        let first = read::automation_action_step_ids(store.read_model().conn(), action_id)?.is_empty();
        let step = store.automation_step_add(action_id, new)?;
        for name in &exits {
            store.automation_exit_add(AutomationOwner::Step, step.id, Some(name))?;
        }
        for (name, kind, required) in &ports {
            store.automation_port_add(
                AutomationPortOwner::Step,
                step.id,
                AutomationPortDirection::In,
                name,
                *kind,
                *required,
            )?;
        }
        if first {
            store.automation_action_set_entry(action_id, Some(step.id))?;
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Make an empty action and put it in on a line** ([`amenbo_core::ops::automation::placement_insert_new`])
/// — the one road a new action comes onto a picture by, since the first placement is one of the
/// built-ins a run starts at (`AMB-D-977`). The way out that was pressed comes to point at the new
/// placement, and the new placement goes on to where that way out used to, in one transaction; the
/// ack names the action it made, for the screen to go and build it.
#[tauri::command]
pub fn automation_placement_insert_new(
    edge_id: i64,
    name: String,
    shelf: String,
) -> Result<WriteAck, CmdError> {
    let shelf = action_shelf(&shelf)?;
    let placement =
        with_store_mut(|store| Ok(store.automation_placement_insert_new(edge_id, shelf, &name)?))?;
    Ok(WriteAck::new(&["automations", "automationActions"]).action(placement.action_id))
}

/// Which library an action written at a picture lands in, as the screen sends it. An unknown word is
/// refused here rather than guessed at: the two are what core knows
/// ([`amenbo_core::ops::automation::ActionShelf`]).
fn action_shelf(word: &str) -> Result<ActionShelf, CmdError> {
    match word {
        "device" => Ok(ActionShelf::Device),
        "project" => Ok(ActionShelf::Project),
        _ => Err(amenbo_core::Error::invalid(format!(
            "'{word}' is no library — say 'device' or 'project'"
        ))
        .into()),
    }
}

/// **Put a step in on a line inside an action** — the one road by which a step joins a picture
/// already drawn: the way out that was pressed comes to point at the new step, and the new step goes
/// on to whatever that way out used to reach ([`amenbo_core::ops::automation::step_insert`]).
///
/// **A step inside an action always carries its own prompt.** An action does not place actions
/// (`AMB-D-949`), so there is nothing to pick out of the library here — which is what tells this door
/// apart from [`automation_step_insert`], the one that puts an action in on an automation's line.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_action_step_insert(
    edge_id: i64,
    name: String,
    prompt: String,
    interactive: bool,
    exits: Vec<String>,
    inputs: Vec<(String, String, bool)>,
) -> Result<WriteAck, CmdError> {
    let mut ports = Vec::with_capacity(inputs.len());
    for (name, kind, required) in inputs {
        ports.push((name, port_kind(&kind)?, required));
    }
    let new = NewStep {
        name,
        prompt,
        interactive,
        work_dir_ref: None,
        report_to_task: false,
        show_history: true,
        show_notes: true,
        show_decisions: true,
        show_comments: true,
    };
    with_store_mut(|store| {
        store.automation_step_insert(edge_id, new, &exits, &ports)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Take a step out of its action**, with what it declared and every line naming it at either end.
///
/// **Losing the entry clears it** rather than being refused: an action under construction has to be
/// able to lose any step, and what an action left without an entry is, is one the launch check names
/// ([`amenbo_core::ops::automation::step_delete`]).
#[tauri::command]
pub fn automation_step_remove(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_step_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Name the step a placement of this action opens first**, or clear it with no `step`.
///
/// It is the action's own row and not a line on the picture: where a run enters is not something the
/// edges can say, the entry being the one box nothing points at.
#[tauri::command]
pub fn automation_action_entry_set(
    action_id: i64,
    step: Option<i64>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_set_entry(action_id, step)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// One library action's whole definition, or nothing where that id names none — the action build
/// screen's one read (`AMB-T-5315`).
#[tauri::command]
pub fn automation_action_detail(id: i64) -> Result<Option<AutomationActionDetailDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_action_detail");
    let store = open_store_read()?;
    let Some(view) = automation_view::action_detail(store.read_model().conn(), id)? else {
        return Ok(None);
    };
    let conn = store.read_model().conn();
    let held_by = run_cards(&store, automation_view::run_ids_holding_action(conn, id)?)?;
    let mut placed_on = Vec::new();
    for automation_id in automation_view::automations_placing(conn, id)? {
        if let Some(one) = read::automation(conn, automation_id)? {
            placed_on.push(AutomationPlacedOnDto { id: one.id, name: one.name, project: one.project_id });
        }
    }
    Ok(Some(action_detail_dto(view, held_by, placed_on)))
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

// ───────────────────────── what an action and its steps declare ─────────────────────────
//
// **Two declarers, one set of doors** (`AMB-D-949`). A way out and an input are declared either by a
// library action — which is what every placement of it is read and wired by — or by one step inside
// it, which is what the picture drawn inside the action is wired by. The row is the same shape
// whichever of the two says it (`amenbo_core::model::AutomationOwner`), so each door below takes
// `owner` and the id of that owner rather than being written twice.
//
// **A setting has one declarer and no second door.** It is the action's, and each placement answers
// it on a row of its own (`automation_cfg_answer`); a step declares none.
//
// There is no id on a setting or an input for a screen to send: the two rows a setting has — the
// action's declaration and each placement's answer — are folded into the one row a screen draws
// (cfg_dto), so a name is what turns into a row here.

/// Which of the two is declaring, as the screens spell it. Refused here rather than guessed: a door
/// that fell back to the action would write the outside contract where a step was meant.
fn declarer(word: &str) -> Result<AutomationOwner, CmdError> {
    AutomationOwner::parse(word).ok_or_else(|| {
        amenbo_core::Error::invalid(format!(
            "'{word}' is neither of the two that declare a way out or an input (step, action)"
        ))
        .into()
    })
}

/// A declaration the owner was expected to carry and does not — the panel naming a row that has
/// since gone, which is what a definition re-read after somebody else's write looks like.
fn undeclared(what: &str, owner: AutomationOwner, owner_id: i64, name: &str) -> CmdError {
    let who = owner.as_str();
    amenbo_core::Error::not_found(format!("{who} '{owner_id}' declares no {what} called '{name}'"))
        .into()
}

/// One of this owner's ways out, by the name the panel holds it under. `None` is the done one
/// ([`amenbo_core::model::DONE_EXIT`]).
fn exit_row(
    store: &amenbo_core::Store,
    owner: AutomationOwner,
    owner_id: i64,
    name: Option<&str>,
) -> Result<i64, CmdError> {
    let found =
        read::automation_exit_by_name(store.read_model().conn(), owner, owner_id, name)?;
    found.map(|one| one.id).ok_or_else(|| {
        undeclared("way out", owner, owner_id, name.unwrap_or(amenbo_core::model::DONE_EXIT))
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
    found
        .map(|one| one.id)
        .ok_or_else(|| undeclared("setting", AutomationOwner::Action, action_id, name))
}

/// One of this owner's inputs, by name.
fn input_row(
    store: &amenbo_core::Store,
    owner: AutomationOwner,
    owner_id: i64,
    name: &str,
) -> Result<i64, CmdError> {
    let found = read::automation_port_by_name(
        store.read_model().conn(),
        port_owner(owner),
        owner_id,
        AutomationPortDirection::In,
        name,
    )?;
    found.map(|one| one.id).ok_or_else(|| undeclared("input", owner, owner_id, name))
}

/// The port owner a declarer is, for the doors that write one.
fn port_owner(owner: AutomationOwner) -> AutomationPortOwner {
    match owner {
        AutomationOwner::Step => AutomationPortOwner::Step,
        AutomationOwner::Action => AutomationPortOwner::Action,
    }
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

/// **Declare another way out of this action, or of one step inside it.** Both are born carrying the
/// done one and the error one, so this is the second and every one after it — and `*` is refused
/// as a name, that one being carried already ([`amenbo_core::ops::automation::exit_add`]).
#[tauri::command]
pub fn automation_exit_declare(
    owner: String,
    owner_id: i64,
    name: String,
) -> Result<WriteAck, CmdError> {
    let owner = declarer(&owner)?;
    with_store_mut(|store| {
        store.automation_exit_add(owner, owner_id, Some(&name))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Rename one way out.** A way out keeps a name, so a `null` `to` is refused
/// ([`amenbo_core::ops::automation::exit_rename`]). Every edge and every wire on it stays on it: core
/// keys them by the way out's row, not its name.
#[tauri::command]
pub fn automation_exit_rename(
    owner: String,
    owner_id: i64,
    from: Option<String>,
    to: Option<String>,
) -> Result<WriteAck, CmdError> {
    let owner = declarer(&owner)?;
    with_store_mut(|store| {
        let id = exit_row(store, owner, owner_id, from.as_deref())?;
        store.automation_exit_rename(id, to.as_deref())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Take one way out away**, with the outputs declared on it. The error one is refused by core:
/// every declarer carries it whether or not a row says so.
#[tauri::command]
pub fn automation_exit_remove(
    owner: String,
    owner_id: i64,
    name: Option<String>,
) -> Result<WriteAck, CmdError> {
    let owner = declarer(&owner)?;
    with_store_mut(|store| {
        let id = exit_row(store, owner, owner_id, name.as_deref())?;
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
    owner: String,
    owner_id: i64,
    name: String,
    kind: String,
    required: bool,
) -> Result<WriteAck, CmdError> {
    let owner = declarer(&owner)?;
    let kind = port_kind(&kind)?;
    with_store_mut(|store| {
        store.automation_port_add(
            port_owner(owner),
            owner_id,
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
    owner: String,
    owner_id: i64,
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    required: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let owner = declarer(&owner)?;
    let kind = kind.as_deref().map(port_kind).transpose()?;
    with_store_mut(|store| {
        let id = input_row(store, owner, owner_id, &name)?;
        store.automation_port_update(id, rename.as_deref(), kind, required)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Take an input away.** The wires that fed it are left where they are, parted.
#[tauri::command]
pub fn automation_input_remove(
    owner: String,
    owner_id: i64,
    name: String,
) -> Result<WriteAck, CmdError> {
    let owner = declarer(&owner)?;
    with_store_mut(|store| {
        let id = input_row(store, owner, owner_id, &name)?;
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

/// **Choose who carries one step out at one placement** — the agent, and the model where one is
/// named — or, with no `agent`, leave nobody chosen (`AMB-D-960`). `model` absent is the agent's own
/// default.
///
/// It is the placement's to say because the same action placed on two pictures may be run by two
/// different models, and an action holds several steps, so the choice is made step by step
/// ([`amenbo_core::ops::automation::placement_step_set`]).
#[tauri::command]
pub fn automation_placement_step_set(
    placement_id: i64,
    step_id: i64,
    agent: Option<String>,
    model: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        match agent.as_deref() {
            Some(agent) => {
                store.automation_placement_step_set(placement_id, step_id, agent, model.as_deref())?;
            }
            None => store.automation_placement_step_clear(placement_id, step_id)?,
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Put a library action in on a line** — the one road by which an action already on the shelf joins
/// a picture already drawn. The way out that was pressed comes to point at the new placement, and the
/// new placement goes on to where that way out used to reach.
///
/// An action made on the spot comes in by [`automation_placement_insert_new`] instead: that one makes
/// the action as well, and hands its id back.
///
/// The whole press is one transaction, edges and all: a box left behind with the line still running
/// past it is a picture nobody asked for.
#[tauri::command]
pub fn automation_step_insert(edge_id: i64, action: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_placement_insert(edge_id, action)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

// ───────────────────────── what happens after a way out ─────────────────────────
//
// An edge belongs to the picture, so these doors name the placement it leaves and the placement it
// opens — never the action, whose ways out are declared elsewhere on this file. One way out decides
// one thing, and core refuses a second edge on the same one.

/// What a way out is said to do, as the screen sends it: the word, the box a `go` opens, and the way
/// out of the action an `exit` returns to (`None` there being the done one).
fn edge_target(
    ends: &str,
    to_id: Option<i64>,
    exit_to: Option<String>,
) -> Result<EdgeTarget, CmdError> {
    match (ends, to_id) {
        ("go", Some(to)) => Ok(EdgeTarget::Go(to)),
        ("go", None) => {
            Err(amenbo_core::Error::invalid("say which box this way out opens").into())
        }
        ("exit", _) => Ok(EdgeTarget::Exit(exit_to)),
        ("done", _) => Ok(EdgeTarget::Done),
        ("halt", _) => Ok(EdgeTarget::Halt),
        _ => Err(amenbo_core::Error::invalid(format!(
            "'{ends}' is not one of the things a way out does — open another box, leave the action, \
             close the task, or stop the run"
        ))
        .into()),
    }
}

/// **Say what happens after one box leaves through one way out**
/// ([`amenbo_core::ops::automation::edge_add`]) — a placement on an automation, a step inside an
/// action, as `picture` says.
///
/// **A new `go` edge is born capped**, at [`amenbo_core::model::DEFAULT_MAX_TIMES`], which is the
/// answer the command line gives the same silence: what a limit guards against is a loop that never
/// converges, and a builder who never thought about one is who that loop happens to. The limit is
/// then a field like any other (`automation_edge_edit`), and a picture that wants no limit says so
/// there. An edge that closes the task or stops the run is taken once and carries none — core
/// refuses one.
#[tauri::command]
pub fn automation_edge_add(
    picture: String,
    from_id: i64,
    exit_name: Option<String>,
    ends: String,
    to_id: Option<i64>,
    exit_to: Option<String>,
) -> Result<WriteAck, CmdError> {
    let picture = picture_owner(&picture)?;
    let target = edge_target(&ends, to_id, exit_to)?;
    let max_times = match target {
        EdgeTarget::Go(_) => Some(amenbo_core::model::DEFAULT_MAX_TIMES),
        _ => None,
    };
    with_store_mut(|store| {
        store.automation_edge_add(picture, from_id, exit_name.as_deref(), target, max_times)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Change where an edge goes, or how often it may be taken.** Only what is `Some` is written, and
/// `clear_max_times` is how the limit is taken away — an absent `max_times` leaves it alone
/// ([`amenbo_core::ops::automation::edge_update`]).
///
/// The way out it hangs on is not among the fields: that pair is what the edge *is*, so moving it to
/// another way out is a remove and an add.
#[tauri::command]
pub fn automation_edge_edit(
    id: i64,
    ends: Option<String>,
    to_id: Option<i64>,
    exit_to: Option<String>,
    max_times: Option<i64>,
    clear_max_times: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let target = match ends {
        Some(ends) => Some(edge_target(&ends, to_id, exit_to)?),
        None => None,
    };
    let max_times = match (clear_max_times, max_times) {
        (Some(true), _) => Some(None),
        (_, Some(n)) => Some(Some(n)),
        _ => None,
    };
    with_store_mut(|store| {
        store.automation_edge_update(id, target, max_times)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Take away what a way out said it did.** The way out is then read as saying nothing, which for
/// the error one means stopping the run and calling a person, and for any other means a run that
/// leaves through it has nowhere to go — which the launch check names
/// ([`amenbo_core::ops::automation::edge_delete`]).
#[tauri::command]
pub fn automation_edge_remove(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_edge_delete(id)?;
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

// ───────────────────────── the two pictures ─────────────────────────
//
// **One set of doors, two pictures** (`AMB-D-949`): the boxes on an automation are placements, the
// boxes inside an action are steps, and a line is the same row on either
// (`amenbo_core::model::AutomationPictureOwner`). So each door below takes `picture` and the ids of
// that picture's boxes.

/// Which picture a line is drawn on, as the screens spell it. Refused here rather than guessed, for
/// [`declarer`]'s reason.
fn picture_owner(word: &str) -> Result<AutomationPictureOwner, CmdError> {
    AutomationPictureOwner::parse(word).ok_or_else(|| {
        amenbo_core::Error::invalid(format!(
            "'{word}' is neither of the two pictures a line is drawn on (automation, action)"
        ))
        .into()
    })
}

/// **Say what fills one of a box's inputs**, by naming the way out and the output it comes from.
///
/// Drawing the same wire twice answers the one already drawn rather than writing a second row
/// ([`amenbo_core::ops::automation::wire_add`]), so the screen's control can send what the reader
/// picked without first working out whether anything was there.
#[tauri::command]
pub fn automation_wire_set(
    picture: String,
    from_id: i64,
    from_exit_name: Option<String>,
    from_port_name: String,
    to_id: i64,
    to_port_name: String,
) -> Result<WriteAck, CmdError> {
    let picture = picture_owner(&picture)?;
    with_store_mut(|store| {
        store.automation_wire_add(
            picture,
            from_id,
            from_exit_name.as_deref(),
            &from_port_name,
            to_id,
            &to_port_name,
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Take a wire away**, leaving the input it fed with nothing reaching it.
#[tauri::command]
pub fn automation_wire_clear(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_wire_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// One automation's whole definition, or nothing where that id names none.
#[tauri::command]
pub fn automation_detail(id: i64) -> Result<Option<AutomationDetailDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_detail");
    let store = open_store_read()?;
    let Some(view) = automation_view::detail(store.read_model().conn(), id)? else { return Ok(None) };
    let held_by =
        run_cards(&store, automation_view::run_ids_holding_automation(store.read_model().conn(), id)?)?;
    Ok(Some(detail_dto(view, held_by)))
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

/// **What a launch of this automation asks for** — what its entry reads at launch, so the dialog every
/// press opens asks for that and nothing else (`AMB-D-970`). Core answers it
/// ([`automation_run::launch_asks`]); an entry handed what it does not read would refuse the launch.
#[tauri::command]
pub fn automation_launch_asks(id: i64) -> Result<AutomationLaunchAsksDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_launch_asks");
    let store = open_store_read()?;
    let (reads, axes) = match automation_run::launch_asks(store.read_model().conn(), id)? {
        automation_run::LaunchAsks::Words => ("words", Vec::new()),
        automation_run::LaunchAsks::Task { axes } => ("task", axes),
        automation_run::LaunchAsks::Nothing => ("nothing", Vec::new()),
    };
    Ok(AutomationLaunchAsksDto {
        reads: reads.to_string(),
        axes: axes
            .into_iter()
            .map(|axis| AutomationLaunchAxisDto { name: axis.name, values: axis.values, required: axis.required })
            .collect(),
    })
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
///
/// **What the person hands over with the press** (`AMB-D-970`): `text`, and `files` by their paths on
/// this machine, for an agent's step as the entry; `title`, `notes` and `classification` (axis and
/// value, by name) for the built-in that files a task ([`automation_launch_asks`] says which). The files are ingested the way an attachment is ([`crate::commands::attachment_add`]):
/// the per-file cap checked, then streamed into the blob store. They go into the store before the
/// launch, and onto the run in the launch's own transaction ([`automation_run::HandedAtLaunch`]) — a
/// launch refused afterwards leaves a blob nothing names, which the blob sweep takes like any other.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_launch(
    id: i64,
    agents: Option<Vec<String>>,
    workspace_open: bool,
    text: Option<String>,
    files: Option<Vec<String>>,
    title: Option<String>,
    notes: Option<String>,
    classification: Option<Vec<(String, String)>>,
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
    let run = with_store_mut(|store| {
        let mut handed = automation_run::HandedAtLaunch {
            text,
            title,
            notes,
            classification: classification.unwrap_or_default(),
            ..Default::default()
        };
        for path in files.unwrap_or_default() {
            handed.files.push(handed_file(store, &path)?);
        }
        Ok(store.automation_launch(id, &by, &handed)?)
    })?;
    crate::automation_watch::wake();
    Ok(AutomationRunStartedDto { run: run.id })
}

/// One file handed over at launch, taken in from where it is on this machine: a regular file, within
/// the per-file cap, streamed into the blob store.
fn handed_file(
    store: &mut amenbo_core::Store,
    path: &str,
) -> Result<automation_run::HandedFile, CmdError> {
    let src = std::path::Path::new(path);
    let meta = std::fs::metadata(src).map_err(|e| format!("cannot read the file '{path}': {e}"))?;
    if !meta.is_file() {
        return Err(format!("'{path}' is not a regular file").into());
    }
    let filename = src.file_name().and_then(|n| n.to_str()).unwrap_or("attachment").to_string();
    let mime = amenbo_core::blob::mime_from_filename(&filename);
    store.config.attachment_limits.check_per_file(mime, meta.len())?;
    let blob = store.blobs().ingest_path(src)?;
    Ok(automation_run::HandedFile {
        blob_hash: blob.hash,
        filename,
        mime: mime.map(str::to_string),
        size_bytes: blob.size_bytes as i64,
    })
}

/// **What is under way right now**, across every project — the rows of the "running" tab.
///
/// It crosses projects because a terminal does — what a run holds is a terminal on this machine, and
/// this machine is not divided up per project. What is going comes first, then every failure nobody
/// has acknowledged ([`read::automation_runs_live`]). What is over and needs nobody is the "history"
/// tab's ([`automation_history_page`], `AMB-D-955`).
#[tauri::command]
pub fn automation_running_page() -> Result<Vec<AutomationRunCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_running_page");
    let store = open_store_read()?;
    let mut out = Vec::new();
    for run in read::automation_runs_live(store.read_model().conn())? {
        out.push(run_card(&store, run)?);
    }
    Ok(out)
}

/// **How many runs a page of the "history" tab holds.**
const HISTORY_PAGE: usize = 20;

/// **One page of the "history" tab** — completed, canceled, and acknowledged failures, newest first,
/// across every project (`AMB-D-955`).
///
/// A page at a time because the history only grows: the screen holds one page of it and no more.
/// `only` narrows it to one ending (`"completed"`, `"failed"`, `"canceled"`) and absent is all three;
/// `project_id` narrows it to one project's runs — the tab opened from that project — and absent is
/// every project's, the sidebar's (`AMB-D-954`); `page` counts from 0.
#[tauri::command]
pub fn automation_history_page(
    only: Option<String>,
    project_id: Option<i64>,
    page: usize,
) -> Result<AutomationRunHistoryDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_history_page");
    let only = match only.as_deref() {
        None => None,
        Some("completed") => Some(read::RunOutcome::Completed),
        Some("failed") => Some(read::RunOutcome::Failed),
        Some("canceled") => Some(read::RunOutcome::Canceled),
        // Uncoded: the screen offers the three and nothing else, so another word is a caller's
        // mistake rather than something a reader is shown.
        Some(other) => {
            return Err(CmdError::from(amenbo_core::error::Error::invalid(format!(
                "'{other}' is not an ending the history is narrowed to"
            ))))
        }
    };
    let store = open_store_read()?;
    let found = read::automation_runs_history(
        store.read_model().conn(),
        project_id,
        only,
        page * HISTORY_PAGE,
        HISTORY_PAGE,
    )?;
    let mut runs = Vec::with_capacity(found.runs.len());
    for run in found.runs {
        runs.push(run_card(&store, run)?);
    }
    let by_ending = AutomationRunEndingsDto {
        completed: found.by_ending.completed,
        failed: found.by_ending.failed,
        canceled: found.by_ending.canceled,
    };
    Ok(AutomationRunHistoryDto { runs, total: found.total, page_size: HISTORY_PAGE, by_ending })
}

/// **Say a failed run has been seen** ([`amenbo_core::ops::automation_stop::acknowledge`]) — pressed on
/// its row of the "running" tab, which it then leaves for the "history" tab (`AMB-D-955`).
///
/// **It is not a `WriteAck` write**, for the reason [`automation_run_stop`] is not: what it moves is a
/// run, and every screen drawing one is already following the change feed.
#[tauri::command]
pub fn automation_run_acknowledge(run_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_acknowledge(run_id)?;
    Ok(())
}

/// **The action a spot on the picture stands on**, named — the half a step's own name stopped saying
/// when a launch began opening a placement into a column of steps (`AMB-D-949`).
///
/// It is walked from the live picture rather than kept in the run's copy, the same way the row reads
/// the automation's name: what a reader is being told is which spot of the automation in front of
/// them this is, and a name the picture no longer holds would point at nothing. `None` where the spot
/// or its action has gone, and then what is drawn is the step alone.
fn placed_action_name(
    store: &amenbo_core::Store,
    placement_id: Option<i64>,
) -> Result<Option<String>, CmdError> {
    let Some(placement_id) = placement_id else { return Ok(None) };
    let conn = store.read_model().conn();
    let Some(placement) = read::automation_placement(conn, placement_id)? else { return Ok(None) };
    Ok(read::automation_action(conn, placement.action_id)?.map(|one| one.name))
}

/// One run as the tab draws it: what it is, how far in, and what it is on.
/// The rows of the runs named, in the order named — a run gone from under an id is left out.
fn run_cards(store: &amenbo_core::Store, ids: Vec<i64>) -> Result<Vec<AutomationRunCardDto>, CmdError> {
    let mut out = Vec::new();
    for id in ids {
        if let Some(run) = read::automation_run(store.read_model().conn(), id)? {
            out.push(run_card(store, run)?);
        }
    }
    Ok(out)
}

fn run_card(
    store: &amenbo_core::Store,
    run: amenbo_core::model::AutomationRun,
) -> Result<AutomationRunCardDto, CmdError> {
    let conn = store.read_model().conn();
    let steps = read::automation_run_steps_of(conn, run.id)?;
    // The step it is on, or the last one it ran — read through the run's own copy of the definition,
    // which is what says what was asked at launch rather than what the automation says now.
    let last_def = match steps.last() {
        Some(last) => read::automation_run_def(conn, last.run_def_id)?,
        None => None,
    };
    let action_name =
        placed_action_name(store, last_def.as_ref().and_then(|def| def.placement_id))?;
    let exit_name = last_def.as_ref().and_then(|def| left_by(def, steps.last()?.exit_id));
    let builtin = last_def.as_ref().and_then(|def| def.builtin.clone());
    let placement = last_def.as_ref().and_then(|def| def.placement_id);
    let step_name = last_def.map(|def| def.name);
    // The stretch it is in now. A run walks one per task, and a run between tasks is on none.
    let stretch = read::automation_run_task_last(conn, run.id)?.map(|one| one.id);
    // The steps that owed the task their report and could not leave it, the task being closed.
    let mut report_withheld = Vec::new();
    for step in steps.iter().filter(|one| one.report_withheld) {
        if let Some(def) = read::automation_run_def(conn, step.run_def_id)? {
            report_withheld.push(def.name);
        }
    }
    Ok(AutomationRunCardDto {
        run: run.id,
        project: run.project_id,
        project_name: read::project(conn, run.project_id)?.map(|p| p.name).unwrap_or_default(),
        automation: run.automation_id,
        automation_name: read::automation(conn, run.automation_id)?
            .map(|one| one.name)
            .unwrap_or_default(),
        status: run.status.as_str(),
        started_at: run.started_at.map(|at| at.to_rfc3339_z()),
        ended_at: run.ended_at.map(|at| at.to_rfc3339_z()),
        pause_requested: run.pause_requested,
        waiting: run.status == amenbo_core::model::AutomationRunStatus::Running
            && amenbo_core::ops::automation_run::is_waiting(conn, run.id)?,
        stopped_reason: run.stopped_reason.map(|one| one.as_str()),
        step_name,
        builtin,
        action_name,
        placement,
        steps_done: steps.len(),
        exit_name,
        task: worked_task(store, stretch)?,
        report_withheld,
        acknowledged: run.acknowledged_at.is_some(),
    })
}

/// **The way out one execution left through**, by the name the run's copy declared it under. `None`
/// while it has not left, and where the copy holds no way out of that id.
///
/// Read off the copy rather than the live step, for the reason the step's own name is: what a reader
/// is told is the way out as it stood when the run took it, and a way out renamed or taken off since
/// is still the one it left by.
fn left_by(def: &amenbo_core::model::AutomationRunDef, exit_id: Option<i64>) -> Option<String> {
    let exit_id = exit_id?;
    let exits: Vec<amenbo_core::model::RunDefExit> = serde_json::from_str(&def.exits).ok()?;
    exits.into_iter().find(|one| one.id == exit_id).map(|one| one.name)
}

/// **The runs a workspace's panes are drawing**, by id — what the row over each pane says the run's
/// state with (`app/src/talk/nameplate.ts`).
///
/// It is read by id rather than off the "running" tab's list, because a pane outlives the run it
/// draws: a run that has completed or been canceled leaves that list, and its pane is still up saying
/// how it ended. A run gone from under an id is left out.
#[tauri::command]
pub fn automation_run_cards(run_ids: Vec<i64>) -> Result<Vec<AutomationRunCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_run_cards");
    let store = open_store_read()?;
    run_cards(&store, run_ids)
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

/// **The step each run last opened**, as it was told to the window — kept for a workspace that was
/// not there to hear it.
///
/// The event reaches only a face that is up. The workspace is put up the first time it is asked for
/// (`app/src/shell/AppShell.tsx`), and a run started from the command line before then opened its
/// step with nobody listening: the terminal runs (`crate::pty::open_step`), and without this the
/// run's pane would never stand. A face coming up reads these ([`automation_steps_standing`]).
#[derive(Default)]
pub struct StepsStanding(Mutex<HashMap<i64, AutomationStepOpenDto>>);

/// **The steps whose terminal is still running**, one per run — what a workspace coming up stands
/// the runs' panes on. A step whose terminal has ended is left out: there is nothing for a pane to
/// take up, and the run's next step, if it has one, arrives by the event like any other. A built-in
/// Amenbo is still carrying out is kept, for the card its pane stands on ([`tell`]).
#[tauri::command]
pub fn automation_steps_standing(app: tauri::AppHandle) -> Vec<AutomationStepOpenDto> {
    let standing = app.state::<StepsStanding>();
    let standing = standing.0.lock().expect("steps standing lock");
    let mut open: Vec<AutomationStepOpenDto> = standing
        .values()
        .filter(|one| {
            one.builtin.is_some()
                || one.step.as_ref().is_some_and(|step| crate::pty::is_open(&app, &step.session))
        })
        .cloned()
        .collect();
    open.sort_by_key(|one| one.run);
    open
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
    //
    // **Through the catalog, because the two ends speak different words**: the probe remembers
    // commands (`claude`) and a step names an agent id (`claude-code`). Handing the remembered list
    // straight down stopped every run `no_agent` on its first step, on a launch the check had just
    // called ready (`amenbo_core::wake::startable_ids`).
    let startable: Option<Vec<String>> = amenbo_core::wake::startable_ids(&store.config);
    // **A built-in is told before it is carried out**, so the run's pane says what Amenbo is doing
    // for as long as it takes — a worktree cut from a fetch is not instant (`AMB-D-964`). Carried out,
    // it is told again below, in the same place.
    //
    // **Not while it waits** (`AMB-D-969`): the watch opens a waiting built-in on every look, and the
    // pane already stands on the card that says it is waiting.
    if let Some(def) = read::automation_run_def(store.read_model().conn(), def_id)?
        .filter(|def| def.builtin.is_some())
        .filter(|_| standing_wait(app, run_id).is_none())
    {
        let (project, builtin) = builtin_about_to(store, run_id, &def)?;
        // The card takes the pane over from the step before, whose program would otherwise go on with
        // no pane left to draw it.
        crate::pty::end_steps_of(app, run_id);
        tell(app, AutomationStepOpenDto {
            run: run_id,
            project,
            step: None,
            builtin: Some(builtin),
            missing: Vec::new(),
        });
    }
    let opened = store.automation_step_open(run_id, def_id, startable.as_deref())?;
    let (project, step, builtin, missing) = match opened {
        Opened::Ready(ready) => {
            let def = &ready.run_def;
            let run = read::automation_run(store.read_model().conn(), run_id)?
                .ok_or_else(|| CmdError::from(amenbo_core::error::Error::not_found(
                    format!("run '{run_id}' not found"),
                )))?;
            let folder = ready.folder.clone().or_else(|| project_folder(store, run.project_id));
            // Started here and not by the run's pane, which is drawn only where the reader is looking
            // (`crate::pty::open_step`).
            let session = crate::pty::open_step(
                app,
                run_id,
                ready.run_step.id,
                folder.clone(),
                def.agent.clone(),
                ready.text.clone(),
            )?;
            (
                run.project_id,
                Some(AutomationStepRunDto {
                    run_step: ready.run_step.id,
                    automation: run.automation_id,
                    placement: def.placement_id,
                    automation_name: automation_name(store, run.automation_id)?,
                    name: def.name.clone(),
                    action_name: placed_action_name(store, def.placement_id)?,
                    task: worked_task(store, ready.run_step.run_task_id)?,
                    session,
                    agent: def.agent.clone(),
                    model: def.model.clone(),
                    folder,
                    interactive: def.interactive,
                }),
                None,
                Vec::new(),
            )
        }
        // What a step's pane is told about is its own step. No other run is in the event and none is
        // opened here: what the watch looks for is a run `running` with nothing open (`AMB-D-945`).
        Opened::Stopped { run, missing, .. } => (run.project_id, None, None, missing),
        // Nothing is named in the event for this one. `missing` is about inputs, and what a reader is
        // owed here is on the run's own row — the running tab says which ending it was, and the line
        // core left on the task says how far it got (`amenbo_core::ops::automation_stop`).
        Opened::NoAgent { run, agent } => {
            log::info!("run {run_id} asked for {agent}, which this machine cannot start");
            (run.project_id, None, None, Vec::new())
        }
        // Nothing to name here either: the run failed before this step opened, and its row says why
        // (`AMB-D-967`). The task before is back in `todo`, with the line core left on it.
        Opened::LeftTaskOpen { run } => {
            log::warn!("run {run_id} went for its next task with the last one still in progress");
            (run.project_id, None, None, Vec::new())
        }
        // A built-in set to wait found nothing to act on, and nothing was written (`AMB-D-969`). The
        // pane stands on the card that says it is waiting — told once, since the watch opens it again
        // on every look. A pause asked for while it waited took hold here instead, and the paused
        // run has no pane to stand on until it is picked up again.
        Opened::Waiting { run } => {
            use amenbo_core::model::AutomationRunStatus::Running;
            match (run.status, standing_wait(app, run_id)) {
                (Running, Some(told)) => return Ok(told),
                (Running, None) => {
                    let def = read::automation_run_def(store.read_model().conn(), def_id)?.ok_or_else(|| {
                        CmdError::from(amenbo_core::error::Error::not_found(format!(
                            "step '{def_id}' of run '{run_id}' not found"
                        )))
                    })?;
                    let (project, mut builtin) = builtin_about_to(store, run_id, &def)?;
                    builtin.waiting = true;
                    builtin.looks_for = amenbo_core::ops::automation_builtin::looks_for(&def)?;
                    (project, None, Some(builtin), Vec::new())
                }
                _ => (run.project_id, None, None, Vec::new()),
            }
        }
        // A built-in has already been carried out and has reported (`AMB-D-964`), so there is no
        // terminal to stand a pane on. The run is now standing between two steps — or has ended — and
        // the watch woken below reads which, the same as after an agent's report. What the pane is
        // told is the same card, now carried out, with the task it may have taken.
        Opened::Carried { run_step_id, .. } => {
            log::info!("run {run_id} carried out built-in step {run_step_id}");
            let conn = store.read_model().conn();
            let run = read::automation_run(conn, run_id)?
                .ok_or_else(|| CmdError::from(amenbo_core::error::Error::not_found(
                    format!("run '{run_id}' not found"),
                )))?;
            let run_step = read::automation_run_step(conn, run_step_id)?.ok_or_else(|| {
                CmdError::from(amenbo_core::error::Error::not_found(format!(
                    "step '{run_step_id}' of run '{run_id}' not found"
                )))
            })?;
            let def = read::automation_run_def(conn, run_step.run_def_id)?.ok_or_else(|| {
                CmdError::from(amenbo_core::error::Error::not_found(format!(
                    "step '{run_step_id}' of run '{run_id}' has no copy to read"
                )))
            })?;
            let builtin = AutomationBuiltinRunDto {
                automation: run.automation_id,
                placement: def.placement_id,
                automation_name: automation_name(store, run.automation_id)?,
                name: def.name.clone(),
                key: def.builtin.clone().unwrap_or_default(),
                action_name: placed_action_name(store, def.placement_id)?,
                task: worked_task(store, run_step.run_task_id)?,
                finished: true,
                waiting: false,
                looks_for: None,
                exit_name: left_by(&def, run_step.exit_id),
            };
            (run.project_id, None, Some(builtin), Vec::new())
        }
    };
    // The run has just moved, so the thread that keeps it going looks again now rather than sleeping
    // out the interval it was on (`crate::automation_watch`). Called from the watch's own path too,
    // where it costs nothing: that loop is about to come round anyway.
    //
    // **Except where nothing moved**: a built-in that is waiting is opened again on every look, and
    // waking the watch from there would have it look again at once, round and round without a pause.
    if !matches!(builtin, Some(AutomationBuiltinRunDto { waiting: true, .. })) {
        crate::automation_watch::wake();
    }
    let dto = AutomationStepOpenDto { run: run_id, project, step, builtin, missing };
    tell(app, dto.clone());
    Ok(dto)
}

/// **Tell the window what a run's pane stands on now**, and keep it for a face that is not up to hear
/// it ([`StepsStanding`]). A step's terminal and a built-in still being carried out are kept; anything
/// else — a run stopped, a built-in done — has nothing a face coming up later would stand a pane on.
fn tell(app: &tauri::AppHandle, dto: AutomationStepOpenDto) {
    {
        let standing = app.state::<StepsStanding>();
        let mut standing = standing.0.lock().expect("steps standing lock");
        let stands = dto.step.is_some() || dto.builtin.as_ref().is_some_and(|b| !b.finished);
        match stands {
            true => standing.insert(dto.run, dto.clone()),
            false => standing.remove(&dto.run),
        };
    }
    if let Err(e) = app.emit(STEP_EVENT, dto) {
        log::warn!("failed to emit {STEP_EVENT}: {e}");
    }
}

/// What this run's pane stands on, where that is the card of a built-in that is waiting.
fn standing_wait(app: &tauri::AppHandle, run_id: i64) -> Option<AutomationStepOpenDto> {
    let standing = app.state::<StepsStanding>();
    let standing = standing.0.lock().expect("steps standing lock");
    standing.get(&run_id).filter(|one| one.builtin.as_ref().is_some_and(|b| b.waiting)).cloned()
}

/// **A built-in about to be carried out**, as its pane is told before Amenbo starts on it — and which
/// project's pane that is.
///
/// Nothing is written for it yet, so the task is the one the stretch under way is on. A built-in that takes a task opens a stretch of its
/// own (`amenbo_core::ops::automation_step::open`), so it is on none until it has taken one.
fn builtin_about_to(
    store: &amenbo_core::Store,
    run_id: i64,
    def: &amenbo_core::model::AutomationRunDef,
) -> Result<(i64, AutomationBuiltinRunDto), CmdError> {
    let conn = store.read_model().conn();
    let run = read::automation_run(conn, run_id)?.ok_or_else(|| {
        CmdError::from(amenbo_core::error::Error::not_found(format!("run '{run_id}' not found")))
    })?;
    let exits: Vec<amenbo_core::model::RunDefExit> =
        serde_json::from_str(&def.exits).map_err(amenbo_core::error::Error::from)?;
    let takes = exits.iter().any(|e| e.outs.iter().any(|p| p.kind == AutomationPortKind::TaskTake));
    let stretch = match takes {
        true => None,
        false => read::automation_run_task_last(conn, run_id)?.map(|s| s.id),
    };
    Ok((run.project_id, AutomationBuiltinRunDto {
        automation: run.automation_id,
        placement: def.placement_id,
        automation_name: automation_name(store, run.automation_id)?,
        name: def.name.clone(),
        key: def.builtin.clone().unwrap_or_default(),
        action_name: placed_action_name(store, def.placement_id)?,
        task: worked_task(store, stretch)?,
        finished: false,
        waiting: false,
        looks_for: None,
        exit_name: None,
    }))
}

/// The automation a run was launched from, by the name it holds now — empty where it has gone.
fn automation_name(store: &amenbo_core::Store, automation_id: i64) -> Result<String, CmdError> {
    Ok(read::automation(store.read_model().conn(), automation_id)?.map(|one| one.name).unwrap_or_default())
}

/// **Tell the window again about a step whose task was taken after it was opened** (`AMB-T-5427`).
///
/// A step that takes its task opens with none — which task it is about is what the step is there to
/// find out, and it says so with `automation step-take` from inside its terminal. That is the CLI, another
/// process, so nothing reaches here when it happens. What the watch does see is the run with its step
/// under way, once a second (`crate::automation_watch`), and this is where it compares what the window
/// was told with what the stretch holds now.
///
/// **The same event and the same step**, with only its task changed. The workspace reads a step it is
/// already standing on as the same terminal told again, so nothing is closed or opened for it and only
/// the row above the pane moves (`app/src/shell/WorkspaceFace.tsx`). What is kept for a face coming up
/// later ([`StepsStanding`]) is updated with it, so that face is not handed the empty one.
///
/// The lock is held across the read so a step opened meanwhile is not overwritten with the one before
/// it; nothing that takes it reads the store while holding it, so there is no order to go wrong.
pub(crate) fn retell_task(
    app: &tauri::AppHandle,
    store: &amenbo_core::Store,
    run_id: i64,
) -> Result<(), CmdError> {
    let dto = {
        let standing = app.state::<StepsStanding>();
        let mut standing = standing.0.lock().expect("steps standing lock");
        let Some(dto) = standing.get_mut(&run_id) else { return Ok(()) };
        let Some(step) = dto.step.as_mut() else { return Ok(()) };
        let Some(run_step) = read::automation_run_step(store.read_model().conn(), step.run_step)? else {
            return Ok(());
        };
        let task = worked_task(store, run_step.run_task_id)?;
        if task == step.task {
            return Ok(());
        }
        step.task = task;
        dto.clone()
    };
    if let Err(e) = app.emit(STEP_EVENT, dto) {
        log::warn!("failed to emit {STEP_EVENT}: {e}");
    }
    Ok(())
}

/// **Where a step that names no folder is carried out**: the folder its project is bound to.
///
/// A pane opened by a person asks which one where the project has several (`app/src/talk/agent.ts`),
/// but nobody is there to be asked when a run opens a step, so it is the first of them that is still
/// on the disk. `None` where the project has none, and the terminal then starts in the user's home.
fn project_folder(store: &amenbo_core::Store, project: i64) -> Option<String> {
    store
        .bindings()
        .dirs_for_project(project)
        .into_iter()
        .find(|dir| std::path::Path::new(dir).is_dir())
        .map(|dir| dir.to_string())
}

/// **The program carrying out a step ended by itself** (`crate::pty`). Where the step never
/// reported, its run fails as a crash ([`amenbo_core::ops::automation_stop::step_ended`]); where it
/// did, or the run is over, nothing moves. The screens drawing the run follow the change feed.
pub fn step_program_ended(run_step: i64) {
    let ended = crate::commands::open_store().and_then(|mut store| {
        store.automation_step_ended(run_step).map_err(CmdError::from)
    });
    match ended {
        Ok(Some(ended)) => log::info!("run {} failed: step {run_step} ended without reporting", ended.run.id),
        Ok(None) => {}
        Err(e) => log::warn!("could not settle step {run_step} whose program ended: {e:?}"),
    }
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
    let Some(stretch) = read::automation_run_task(conn, stretch_id)? else { return Ok(None) };
    let Some(task_id) = stretch.task_id else { return Ok(None) };
    Ok(read::task_title(conn, task_id)?.map(|title| AutomationRunTaskDto {
        id: task_id,
        r#ref: amenbo_core::idref::task(task_id),
        title,
        seq: stretch.seq,
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
    Ok(Some(store.automation_stop(run_id, Ending::Canceled)?))
}

// ───────────────────────────── shaping ─────────────────────────────
//
// Core resolves (amenbo_core::ops::automation_view); what is left here is naming. Nothing below
// reads the store — a DTO is the same rows under the names a screen draws them by.

/// One automation's whole definition, under the names the build screen draws it by.
fn detail_dto(
    view: automation_view::AutomationView,
    held_by: Vec<AutomationRunCardDto>,
) -> AutomationDetailDto {
    let names = exit_names(view.placements.iter().flat_map(|p| p.exits.iter()));
    let wires = view.wires.iter().map(|w| wire_dto(w, &names, |id| view.port_name(id))).collect();
    let a = view.automation;
    AutomationDetailDto {
        id: a.id,
        project_id: a.project_id,
        name: a.name,
        notes: a.notes,
        entry_placement_id: a.entry_placement_id,
        archived: a.archived,
        edges: view.edges.into_iter().map(|e| edge_dto(e, &names)).collect(),
        wires,
        placements: view.placements.into_iter().map(placement_dto).collect(),
        held_by,
    }
}

/// One library action's whole definition, under the names the action build screen draws it by.
fn action_detail_dto(
    view: automation_view::ActionView,
    held_by: Vec<AutomationRunCardDto>,
    placed_on: Vec<AutomationPlacedOnDto>,
) -> AutomationActionDetailDto {
    let names = exit_names(view.steps.iter().flat_map(|s| s.exits.iter()).chain(view.exits.iter()));
    let wires = view.wires.iter().map(|w| wire_dto(w, &names, |id| view.port_name(id))).collect();
    let action = view.action;
    AutomationActionDetailDto {
        id: action.id,
        name: action.name,
        note: action.note,
        global: action.project_id.is_none(),
        builtin: action.builtin,
        used_by: view.used_by,
        entry_step_id: action.entry_step_id,
        steps: view.steps.into_iter().map(step_dto).collect(),
        edges: view.edges.into_iter().map(|e| edge_dto(e, &names)).collect(),
        wires,
        exits: view.exits.into_iter().map(exit_dto).collect(),
        inputs: view.inputs.into_iter().map(port_dto).collect(),
        settings: view.settings.into_iter().map(cfg_dto).collect(),
        held_by,
        placed_on,
    }
}

/// One step of an action: the terminal it stands up, and what it declares inside the picture.
fn step_dto(view: automation_view::StepView) -> AutomationStepDto {
    let step = view.step;
    AutomationStepDto {
        id: step.id,
        name: step.name,
        prompt: step.prompt,
        interactive: step.interactive,
        work_dir_ref: step.work_dir_ref,
        report_to_task: step.report_to_task,
        show_history: step.show_history,
        show_notes: step.show_notes,
        show_decisions: step.show_decisions,
        show_comments: step.show_comments,
        exits: view.exits.into_iter().map(exit_dto).collect(),
        inputs: view.inputs.into_iter().map(port_dto).collect(),
    }
}

/// **The name of every way out a picture's lines can be keyed to**, by the row's id. A line keys its way
/// out (`AMB-D-961`), and a screen draws it by the name the way out carries now — which is what keeps
/// a renamed way out's lines on it on screen as in the store.
fn exit_names<'a>(
    exits: impl Iterator<Item = &'a automation_view::ExitView>,
) -> std::collections::HashMap<i64, String> {
    exits.map(|e| (e.exit.id, e.exit.name.clone())).collect()
}

/// The name a line's way out carries, `None` where the line keys none, or one no longer there.
fn exit_name(names: &std::collections::HashMap<i64, String>, id: Option<i64>) -> Option<String> {
    id.and_then(|id| names.get(&id).cloned())
}

/// One line of either picture. Which boxes its two ends name is the picture it came in.
fn edge_dto(edge: AutomationEdge, names: &std::collections::HashMap<i64, String>) -> AutomationEdgeDto {
    AutomationEdgeDto {
        id: edge.id,
        from_id: edge.from_id,
        // A line is taken off with the way out it keys, so the name is always there to read.
        exit_name: exit_name(names, Some(edge.exit_id)).unwrap_or_default(),
        to_id: edge.to_id,
        ends: edge.ends.as_str(),
        exit_to: exit_name(names, edge.exit_to_id),
        max_times: edge.max_times,
    }
}

/// One wire of either picture, read the way [`edge_dto`] reads a line.
///
/// Its two ports are keyed as its way out is (`AMB-D-961`), and drawn by the names they carry now — which
/// is what keeps a renamed port's wires on it on screen as in the store.
fn wire_dto<'a>(
    wire: &AutomationWire,
    names: &std::collections::HashMap<i64, String>,
    port_name: impl Fn(i64) -> Option<&'a str>,
) -> AutomationWireDto {
    AutomationWireDto {
        id: wire.id,
        from_id: wire.from_id,
        from_exit_name: exit_name(names, wire.from_exit_id),
        from_port_name: port_name(wire.from_port_id).unwrap_or_default().to_string(),
        to_id: wire.to_id,
        to_port_name: port_name(wire.to_port_id).unwrap_or_default().to_string(),
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
        global: action.as_ref().is_some_and(|one| one.project_id.is_none()),
        builtin: action.as_ref().and_then(|one| one.builtin.clone()),
        step_id: opens.as_ref().map(|s| s.id),
        prompt: opens.as_ref().map(|s| s.prompt.clone()).unwrap_or_default(),
        interactive: opens.as_ref().is_some_and(|s| s.interactive),
        work_dir_ref: opens.as_ref().and_then(|s| s.work_dir_ref.clone()),
        report_to_task: opens.as_ref().is_some_and(|s| s.report_to_task),
        show_history: opens.as_ref().map_or(true, |s| s.show_history),
        show_notes: opens.as_ref().map_or(true, |s| s.show_notes),
        show_decisions: opens.as_ref().map_or(true, |s| s.show_decisions),
        show_comments: opens.as_ref().map_or(true, |s| s.show_comments),
        exits: view.exits.into_iter().map(exit_dto).collect(),
        inputs: view.inputs.into_iter().map(port_dto).collect(),
        settings: view.settings.into_iter().map(cfg_dto).collect(),
        steps: view
            .steps
            .into_iter()
            .map(|one| AutomationPlacementStepDto {
                step_id: one.step.id,
                name: one.step.name,
                agent: one.chosen.as_ref().map(|c| c.agent.clone()),
                model: one.chosen.and_then(|c| c.model),
            })
            .collect(),
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
