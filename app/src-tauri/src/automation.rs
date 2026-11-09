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
    AutomationPortKind, AutomationPortOwner, AutomationRole, AutomationRunStatus,
    AutomationWire,
};
use amenbo_core::ops::automation::{
    ActionShelf, EdgeTarget, NewAutomation, NewRole, NewScript, NewStep, RoleChange,
};
use amenbo_core::ops::Position;
use amenbo_core::ops::automation_builtin;
use amenbo_core::ops::automation_rehearse::Cut;
use amenbo_core::ops::automation_run::{self, Carrier, ReadsFrom, Unmet};
use amenbo_core::ops::automation_stop::Ending;
use amenbo_core::ops::automation_stop::Ended;
use amenbo_core::ops::automation_step::Opened;
use amenbo_core::ops::automation_view;
use amenbo_core::store_engine::read;

use crate::commands::{open_store_read, with_store_mut};
use crate::dto::{
    AutomationActionCardDto, AutomationActionDetailDto, AutomationBuiltinDto,
    AutomationBuiltinExitDto, AutomationBuiltinRunDto, AutomationCardDto, AutomationCfgDto,
    AutomationDetailDto, AutomationEdgeDto, AutomationExitDto, AutomationFailuresToDto, AutomationHeldBackDto,
    AutomationHeldByRecordDto, AutomationHeldByValueDto, AutomationLaunchAsksDto,
    AutomationCarrierDto, AutomationLaunchBlockDto,
    AutomationLaunchCheckDto, AutomationLaunchFailedDto, AutomationLaunchIncidentDto, AutomationLaunchRoleStateDto,
    AutomationLaunchStateDto,
    AutomationPlacedOnDto, AutomationPlacementDto,
    AutomationPlacementStepDto, AutomationPortDto, AutomationRoleDto, AutomationRolePassDto, AutomationRunCardDto,
    AutomationRunEndingsDto,
    AutomationRunHistoryDto, AutomationRunIncidentDto, AutomationRunPassDto, AutomationRunStartedDto,
    AutomationRunTaskDto,
    AutomationSavedDto, AutomationStepDto, AutomationTriggerConditionDto, AutomationTriggerInputDto,
    AutomationTriggerTriedDto,
    AutomationStepOpenDto, AutomationStepRunDto, AutomationTestRunDto, AutomationTestStepDto,
    AutomationWaitingTaskDto, AutomationWireDto, EveryAutomationActionCardDto, EveryAutomationCardDto,
    ScheduleDto, WriteAck,
};
use crate::error::CmdError;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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
            saved: card.saved.map(saved_dto),
            unsaved: card.unsaved,
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
                saved: one.card.saved.map(saved_dto),
                unsaved: one.card.unsaved,
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

/// **Take an automation in from a file, as a new one** in `project_id`
/// ([`amenbo_core::ops::automation_import::import`]) — the file the terminal's `automation export` and
/// the build screen's "Export" write. Nothing in it is matched against what is already here: the
/// automation and every action it places are made anew, the device's library actions in the library
/// and the project's in `project_id`. `src` comes through the OS file picker.
///
/// The ack names the new automation, as [`automation_add`]'s does, so the list opens the build screen
/// on it; the actions are named too, since the ones the file carries are made anew.
#[tauri::command]
pub fn automation_import(project_id: i64, src: String) -> Result<WriteAck, CmdError> {
    let text = std::fs::read_to_string(&src).map_err(|e| CmdError::from(format!("cannot read '{src}': {e}")))?;
    let file = amenbo_core::ops::automation_import::parse(&text)?;
    let made = with_store_mut(|store| Ok(store.automation_import(project_id, &file)?))?;
    Ok(WriteAck::new(&["automations", "automationActions"]).automation(made.id))
}

/// **Rename an automation, rewrite its notes, put it out of the way, or choose who a failed round is
/// filed to and what it is classified under.** Only what is `Some` is written. How many of its runs may
/// be going at once is its roles' headcount ([`automation_role_edit`]), not one of these.
///
/// `failures_to` chooses a person or a role of this automation; core refuses a role of another one.
/// There is no taking it back to nobody from here: a launch is refused without one, so the screen only
/// ever moves it from one to another.
///
/// `failure_values` is the whole classification a failed round's task is filed with, by value ids —
/// `Some(vec![])` for none ([`amenbo_core::ops::automation::update`]). A value of another project, two
/// on one axis and a closed value the automation does not already hold are core's refusals. A required
/// axis left empty is written all the same, and the save and the launch check refuse it.
///
/// Archiving takes nothing away, and like every rewrite it goes through while a run of the automation
/// is going — the run keeps the copy it started with ([`amenbo_core::ops::automation::update`]). It
/// is what keeps a definition nobody launches any more out of a reader's way, so the row stays in the
/// list carrying the mark rather than leaving it — which is why `automation_page` goes on answering
/// with archived ones in it.
///
/// The preamble is not one of the three. It is what Amenbo puts at the head of every launch rather
/// than anything this automation holds ([`amenbo_core::agents::preamble`]).
#[tauri::command]
pub fn automation_edit(
    id: i64,
    name: Option<String>,
    notes: Option<String>,
    archived: Option<bool>,
    failures_to: Option<AutomationFailuresToDto>,
    failure_values: Option<Vec<i64>>,
) -> Result<WriteAck, CmdError> {
    let failures_to = failures_to.map(|to| Some(to.into()));
    with_store_mut(|store| {
        store.automation_update(
            id,
            name.as_deref(),
            notes.as_deref(),
            archived,
            failures_to,
            failure_values.as_deref(),
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Add a trigger to an automation**, after the ones it already has and enabled — what files a task on
/// its own and starts a run of this automation with it ([`amenbo_core::ops::automation::trigger_add`]).
///
/// When it is next asked is not the screen's to say: core counts it from the write. What is refused
/// — a wait under a minute, a task with no title, a role of another automation and the rest — is
/// core's refusal, passed on as it is.
#[tauri::command]
pub fn automation_trigger_add(automation_id: i64, trigger: AutomationTriggerInputDto) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_trigger_add(automation_id, trigger.into())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Write one trigger afresh**, named by its id ([`amenbo_core::ops::automation::trigger_update`]). It
/// keeps its place among the automation's triggers, whether it is enabled and when it last filed a
/// task; when it is next asked is counted afresh from the write. Core's refusals pass on as they are.
#[tauri::command]
pub fn automation_trigger_update(trigger_id: i64, trigger: AutomationTriggerInputDto) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_trigger_update(trigger_id, trigger.into())?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Take one trigger away from its automation** ([`amenbo_core::ops::automation::trigger_delete`]).
/// The others stay as they are, and so do the tasks it filed.
#[tauri::command]
pub fn automation_trigger_delete(trigger_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_trigger_delete(trigger_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Have one trigger be asked, or not** ([`amenbo_core::ops::automation::trigger_set_enabled`]). One
/// that is not files nothing; its `nextAt` stays where it was either way.
#[tauri::command]
pub fn automation_trigger_set_enabled(trigger_id: i64, enabled: bool) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_trigger_set_enabled(trigger_id, enabled)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **How many tasks a trigger's condition counts now, as it is typed** — before it is saved, so the
/// card can say beside it how many match ([`amenbo_core::ops::automation_run::condition_count_of`]).
/// The tasks are the automation's project's, matching `condition`'s filter and, where it names one,
/// worked under its role. It writes nothing; what writing the condition would refuse is refused here,
/// as core's refusal.
#[tauri::command]
pub fn automation_trigger_count(automation_id: i64, condition: AutomationTriggerConditionDto) -> Result<usize, CmdError> {
    let condition: amenbo_core::model::TriggerCondition = condition.into();
    let store = open_store_read()?;
    let counted = automation_run::condition_count_of(store.read_model().conn(), automation_id, &condition)?;
    Ok(counted.matched)
}

/// The tries of a trigger as it is typed that are running ([`automation_trigger_try`]), each by the tag
/// the card gave it, with what stops it.
fn trigger_tries() -> std::sync::MutexGuard<'static, HashMap<u64, Arc<AtomicBool>>> {
    static TRIES: OnceLock<Mutex<HashMap<u64, Arc<AtomicBool>>>> = OnceLock::new();
    // A poisoned lock guards a map still whole, so it is taken as it is.
    TRIES.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// **Try a trigger as it is typed, before it is saved** — whether its condition holds now, what the
/// program it asks first says, and what the task it would file says
/// ([`amenbo_core::ops::automation_run::trigger_draft`]). Nothing is written: no trigger, no task, no
/// run, and its `nextAt` is not moved. What writing it would refuse is refused here, as core's refusal.
///
/// Its programs may run for hours, so it is `async` and runs off the store thread, the store closed
/// before they start. `tag` is the card's name for this try: [`automation_trigger_try_stop`] with it
/// stops the programs, which then say nothing.
#[tauri::command]
pub async fn automation_trigger_try(
    automation_id: i64,
    trigger: AutomationTriggerInputDto,
    tag: u64,
) -> Result<AutomationTriggerTriedDto, CmdError> {
    let stop = Arc::new(AtomicBool::new(false));
    trigger_tries().insert(tag, stop.clone());
    let new: amenbo_core::ops::automation::NewTrigger = trigger.into();
    let ran = tauri::async_runtime::spawn_blocking(move || -> Result<AutomationTriggerTriedDto, CmdError> {
        let draft = {
            let store = open_store_read()?;
            automation_run::trigger_draft(store.read_model().conn(), automation_id, new)?
        };
        Ok(draft.try_now(|| stop.load(Ordering::Relaxed)).into())
    })
    .await;
    trigger_tries().remove(&tag);
    ran.map_err(|e| -> CmdError { format!("trying the trigger did not run: {e}").into() })?
}

/// **Stop the try the card named `tag`** ([`automation_trigger_try`]) — its programs are stopped and say
/// nothing. A try already over is left as it is.
#[tauri::command]
pub fn automation_trigger_try_stop(tag: u64) {
    if let Some(stop) = trigger_tries().get(&tag) {
        stop.store(true, Ordering::Relaxed);
    }
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
            saved: card.saved.map(saved_dto),
            unsaved: card.unsaved,
        });
    }
    Ok(out)
}

/// **The library of every project** — the device's own actions first, then each project's own,
/// project by project in the sidebar's order ([`automation_view::every_action_card`]).
///
/// What the sidebar's "actions" tab lists with "every project" chosen (`AMB-D-992`), so each row
/// carries whose shelf holds it. The window holds every project, so nothing narrows the walk. Built-ins
/// are left out for the reason [`automation_action_page`] gives.
#[tauri::command]
pub fn automation_action_page_everywhere() -> Result<Vec<EveryAutomationActionCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_action_page_everywhere");
    let store = open_store_read()?;
    let cards = automation_view::every_action_card(store.read_model().conn(), None)?;
    Ok(cards
        .into_iter()
        .filter(|one| one.card.action.builtin.is_none())
        .map(|one| EveryAutomationActionCardDto {
            project_id: one.card.action.project_id,
            project_name: one.project_name,
            card: AutomationActionCardDto {
                id: one.card.action.id,
                name: one.card.action.name,
                note: one.card.action.note,
                steps: one.card.steps,
                global: one.card.action.project_id.is_none(),
                used_by: one.card.used_by,
                saved: one.card.saved.map(saved_dto),
                unsaved: one.card.unsaved,
            },
        })
        .collect())
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
        // One action per version of it, and per axis for the one that splits by an axis — an automation
        // placing two of them is still one automation.
        let mut placing = std::collections::BTreeSet::new();
        for action in read::automation_actions_builtin(conn, one.key)? {
            placing.extend(automation_view::automations_placing(conn, action.id)?);
        }
        let used_by = placing.len();
        out.push(AutomationBuiltinDto {
            key: one.key.to_string(),
            name: one.name.to_string(),
            does: one.does.to_string(),
            steps: one.steps.iter().map(|line| line.to_string()).collect(),
            halts: one.halts.iter().map(|line| line.to_string()).collect(),
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
            starts_a_role_only: one.starts_a_role_only(),
        });
    }
    Ok(out)
}

fn builtin_port_dto(port: &automation_builtin::BuiltinPort) -> AutomationPortDto {
    AutomationPortDto { name: port.name.to_string(), kind: port.kind.as_str(), required: port.required }
}

/// **Put a built-in in on a line** ([`automation_step_insert`] for a built-in) — the way out that was
/// pressed comes to point at the new placement, in one transaction with the library action written
/// where none was yet. `axis` is the axis the one that splits by an axis splits by, and nothing for any
/// other (`AMB-D-973`).
#[tauri::command]
pub fn automation_builtin_insert(edge_id: i64, key: String, axis: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_builtin_insert(edge_id, &key, axis)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Put a built-in on after a way out that says nothing yet** — [`automation_builtin_insert`] for a
/// way out with no line to press. `axis` as for [`automation_builtin_insert`].
#[tauri::command]
pub fn automation_builtin_insert_at_exit(
    from_id: i64,
    exit_name: Option<String>,
    key: String,
    axis: Option<i64>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_builtin_insert_at_exit(from_id, exit_name.as_deref(), &key, axis)?;
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

/// **Rename a library action, rewrite what it is for, or say whether it takes one run at a time.**
/// Only what is `Some` is written. These are all that is the action's own to write: the prompt and the
/// flags belong to its steps ([`automation_step_edit`]), who carries each step out to where the action
/// is placed ([`automation_placement_step_set`]), and what it declares has its own doors.
///
/// The note is the automation's notes one layer down: drawn where it is built and on the library's
/// row, never carried into a launch (`AMB-D-952`).
///
/// **One run at a time is kept in a version**, so writing it goes into the draft and reaches a
/// placement only once the action is saved ([`amenbo_core::ops::automation::action_set_one_at_a_time`]).
/// It is written after the name and the note, as two writes.
///
/// **Renaming parts nothing.** A placement points at the action by key, so every picture standing on
/// it reads the new name at once. So does renaming one of its ways out, which edges and wires key by
/// its id (`AMB-D-961`); renaming a port parts every wire that named the old one
/// ([`amenbo_core::ops::automation::action_update`]).
#[tauri::command]
pub fn automation_action_edit(
    id: i64,
    name: Option<String>,
    note: Option<String>,
    one_at_a_time: Option<bool>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        if name.is_some() || note.is_some() {
            store.automation_action_update(id, name.as_deref(), note.as_deref())?;
        }
        if let Some(on) = one_at_a_time {
            store.automation_action_set_one_at_a_time(id, on)?;
        }
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

/// **Save the automation as its next version** ([`amenbo_core::ops::automation::save`]). Refused with
/// the reasons the build screen lists (`save_blocks`); not refused while a run of it goes on
/// (`AMB-D-1015`).
#[tauri::command]
pub fn automation_save(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_save(id)?;
        Ok(())
    })?;
    // A run of it standing at its entry takes the new version up on the watch's next look, so that
    // look is taken now (`AMB-D-1015`).
    crate::automation_watch::wake();
    Ok(WriteAck::new(&["automations"]))
}

/// **Throw away what is written on the automation since its newest version**
/// ([`amenbo_core::ops::automation::discard`]). Every row goes back under the id it was saved with
/// (`AMB-D-961`); a placement only the draft held is gone.
#[tauri::command]
pub fn automation_discard(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_discard(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Write the automation out as one file** at the path the user picked
/// ([`amenbo_core::ops::automation_export::export`]) — its newest saved version with the actions it
/// places, never what is being written on it. The same body the terminal's `automation export` writes.
/// `dest` comes through the OS save dialog, which is where overwrite confirmation happens.
#[tauri::command]
pub fn automation_export(id: i64, dest: String) -> Result<(), CmdError> {
    let store = open_store_read()?;
    let file = store.automation_export(id)?;
    let body = serde_json::to_string_pretty(&file).map_err(|e| CmdError::from(e.to_string()))?;
    std::fs::write(&dest, body + "\n").map_err(|e| format!("cannot write to '{dest}': {e}").into())
}

/// **Save what is inside the action as its next version** ([`amenbo_core::ops::automation::action_save`]).
/// Refused with the reasons the action build screen lists (`save_blocks`); not refused while a run of an
/// automation placing it goes on (`AMB-D-1015`). The first save of an action made on the spot finishes
/// its creation and points the placements standing on it at version 1 (`AMB-D-1005`). The automations
/// placing it read which version each stands on, so their list is told too.
#[tauri::command]
pub fn automation_action_save(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_save(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Throw away what is written inside the action since its newest version**
/// ([`amenbo_core::ops::automation::action_discard`]). Every row goes back under the id it was saved
/// with (`AMB-D-961`).
#[tauri::command]
pub fn automation_action_discard(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_discard(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Give up an action made on the spot** (`AMB-D-1005`,
/// [`amenbo_core::ops::automation::action_abandon`]): the action and the placement standing on it go
/// in one act, and the lines into the placement go back to how they were before it was placed.
#[tauri::command]
pub fn automation_action_abandon(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_action_abandon(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
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
///
/// The script is the same (`AMB-D-1016`): `clear_script` makes the step one an AI carries out again,
/// and a `program` writes the script whole, with the arguments and the timeout beside it.
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
    program: Option<String>,
    args: Option<Vec<String>>,
    timeout_minutes: Option<i64>,
    clear_script: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let work_dir = match (clear_work_dir, work_dir.as_deref()) {
        (Some(true), _) => Some(None),
        (_, Some(name)) => Some(Some(name)),
        _ => None,
    };
    let script = match clear_script {
        Some(true) => Some(None),
        _ => new_script(program, args, timeout_minutes).map(Some),
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
            script,
            None,
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
    program: Option<String>,
    args: Option<Vec<String>>,
    timeout_minutes: Option<i64>,
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
        expected_minutes: None,
        script: new_script(program, args, timeout_minutes),
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
/// — the one road a new action comes onto a picture by. The way out that was pressed comes to point at the new
/// placement, and the new placement goes on to where that way out used to, in one transaction; the
/// ack names the action it made, for the screen to go and build it.
#[tauri::command]
pub fn automation_placement_insert_new(
    edge_id: i64,
    name: String,
    shelf: String,
) -> Result<WriteAck, CmdError> {
    let shelf = action_shelf(&shelf)?;
    let placed =
        with_store_mut(|store| Ok(store.automation_placement_insert_new(edge_id, shelf, &name)?))?;
    Ok(WriteAck::new(&["automations", "automationActions"])
        .action(placed.placement.action_id)
        .placed(placed.closer.as_ref()))
}

/// **Make an empty action and put it on after a way out that says nothing yet**
/// ([`amenbo_core::ops::automation::placement_insert_new_at_exit`]) — [`automation_placement_insert_new`]
/// for a way out with no line to press. The way out comes to point at the new placement, whose own
/// ways out are left saying nothing; the ack names the action it made, as that one's does.
#[tauri::command]
pub fn automation_placement_insert_new_at_exit(
    from_id: i64,
    exit_name: Option<String>,
    name: String,
    shelf: String,
) -> Result<WriteAck, CmdError> {
    let shelf = action_shelf(&shelf)?;
    let placed = with_store_mut(|store| {
        Ok(store.automation_placement_insert_new_at_exit(from_id, exit_name.as_deref(), shelf, &name)?)
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"])
        .action(placed.placement.action_id)
        .placed(placed.closer.as_ref()))
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

/// The script a step is written with, as the screen sends it: a step with a `program` runs it, and one
/// without is carried out by an AI. Whether the program is a full path and the timeout is in range is
/// core's to say ([`amenbo_core::ops::automation::step_add`]).
fn new_script(
    program: Option<String>,
    args: Option<Vec<String>>,
    timeout_minutes: Option<i64>,
) -> Option<NewScript> {
    program.map(|program| NewScript { program, args: args.unwrap_or_default(), timeout_minutes })
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
    program: Option<String>,
    args: Option<Vec<String>>,
    timeout_minutes: Option<i64>,
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
        expected_minutes: None,
        script: new_script(program, args, timeout_minutes),
    };
    with_store_mut(|store| {
        store.automation_step_insert(edge_id, new, &exits, &ports)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]))
}

/// **Put a step on after a way out that says nothing yet, inside an action**
/// ([`amenbo_core::ops::automation::step_insert_at_exit`]) — [`automation_action_step_insert`] for a
/// way out with no line to press. The way out comes to point at the new step, whose own ways out are
/// left saying nothing.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_action_step_insert_at_exit(
    from_id: i64,
    exit_name: Option<String>,
    name: String,
    prompt: String,
    interactive: bool,
    program: Option<String>,
    args: Option<Vec<String>>,
    timeout_minutes: Option<i64>,
    exits: Vec<String>,
    inputs: Vec<(String, String, bool)>,
) -> Result<WriteAck, CmdError> {
    let mut ports = Vec::with_capacity(inputs.len());
    for (name, kind, required) in inputs {
        ports.push((name, port_kind(&kind)?, required));
    }
    let mut new = NewStep::new(&name, &prompt);
    new.interactive = interactive;
    new.script = new_script(program, args, timeout_minutes);
    with_store_mut(|store| {
        store.automation_step_insert_at_exit(from_id, exit_name.as_deref(), new, &exits, &ports)?;
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
            // Each version once, oldest first: an automation may place the action more than once, each
            // placement on a version of its own, and one that stands on none adds nothing.
            let mut versions: Vec<i64> = view
                .placed_at
                .iter()
                .filter(|p| p.automation_id == automation_id)
                .filter_map(|p| p.version)
                .collect();
            versions.sort_unstable();
            versions.dedup();
            placed_on.push(AutomationPlacedOnDto {
                id: one.id,
                name: one.name,
                project: one.project_id,
                versions,
            });
        }
    }
    // Asked the way the save asks it (`amenbo_core::ops::automation::action_save`): only what the action
    // answers for on its own.
    let save_blocks = automation_run::check_action(conn, id)?;
    Ok(Some(action_detail_dto(view, held_by, placed_on, &save_blocks)))
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

/// **Take an input away**, with the wires inside its action that fed it. An automation's wires into it
/// stay: the version a placement points at still declares it.
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
/// (`app/src/screens/automationCfg.ts`); core refuses an answer its kind does not take and keeps the
/// rest as it is handed ([`amenbo_core::ops::automation::cfg_set`]).
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

/// **Move a placement onto another saved version of its action** (`AMB-D-1000`) — Amenbo never moves
/// it by itself. A built-in's placement is refused
/// ([`amenbo_core::ops::automation::placement_version_set`]).
#[tauri::command]
pub fn automation_placement_version_set(placement_id: i64, version: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_placement_version_set(placement_id, version)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Define a role in an automation**, after the roles already there
/// ([`amenbo_core::ops::automation::role_add`]). Only the name is required. `headcount` is how many are
/// in it, 1 unless given; core refuses one outside 1 to [`amenbo_core::model::MAX_ROLE_HEADCOUNT`].
/// `model` is named beside `agent` and with no agent is refused by core: the model is the agent's model.
#[tauri::command]
pub fn automation_role_add(
    automation_id: i64,
    name: String,
    headcount: Option<u32>,
    icon: Option<String>,
    entry_placement_id: Option<i64>,
    agent: Option<String>,
    model: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_role_add(
            automation_id,
            NewRole { name, headcount, take_order: None, starts_by: None, icon, entry_placement_id, agent, model },
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Change a role's name, how many are in it, icon, the placement its round starts at, or its default
/// agent and model** ([`amenbo_core::ops::automation::role_update`]). Only the fields named are written.
/// A headcount outside 1 to [`amenbo_core::model::MAX_ROLE_HEADCOUNT`] is refused by core.
///
/// Each of the four taken-away fields has a third answer — written, cleared, or left alone — and is
/// carried as the pair `x` / `clear_x` for the reason [`automation_step_edit`]'s `work_dir` is: the
/// clear beats a value beside it. Clearing the agent clears the model with it.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn automation_role_edit(
    id: i64,
    name: Option<String>,
    headcount: Option<u32>,
    icon: Option<String>,
    clear_icon: Option<bool>,
    entry_placement_id: Option<i64>,
    clear_entry: Option<bool>,
    agent: Option<String>,
    clear_agent: Option<bool>,
    model: Option<String>,
    clear_model: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let change = RoleChange {
        name,
        headcount,
        take_order: None,
        starts_by: None,
        icon: write_or_clear(icon, clear_icon),
        entry_placement_id: write_or_clear(entry_placement_id, clear_entry),
        agent: write_or_clear(agent, clear_agent),
        model: write_or_clear(model, clear_model),
    };
    with_store_mut(|store| {
        store.automation_role_update(id, change)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// A field with three answers, from the pair a screen writes it with: cleared where `clear` says so,
/// written where a value came, left alone otherwise.
fn write_or_clear<T>(value: Option<T>, clear: Option<bool>) -> Option<Option<T>> {
    match (clear, value) {
        (Some(true), _) => Some(None),
        (_, Some(value)) => Some(Some(value)),
        _ => None,
    }
}

/// **Give a role that starts nowhere its start: the built-in that takes a task**
/// ([`amenbo_core::ops::automation::role_start_take_task`], `AMB-D-1123`). Each role takes from its
/// own waiting tasks, so that one built-in is the only start a role has. It is put on the picture and
/// made the role's start in one transaction, and its id comes back for the picture to mark. A role
/// that already starts somewhere is core's refusal, passed on.
#[tauri::command]
pub fn automation_role_start_take_task(id: i64) -> Result<WriteAck, CmdError> {
    let started = with_store_mut(|store| Ok(store.automation_role_start_take_task(id)?))?;
    Ok(WriteAck::new(&["automations", "automationActions"]).placed(Some(&started.placement)))
}

/// **Reorder a role among its automation's roles** ([`amenbo_core::ops::automation::role_move`]).
/// `position` is one of `top` / `bottom` / `before` / `after`, the last two placed against
/// `anchor_id` — another role of the same automation. No index is taken: the order is the roles'
/// order keys (`AMB-D-42`).
#[tauri::command]
pub fn automation_role_move(
    id: i64,
    position: String,
    anchor_id: Option<i64>,
) -> Result<WriteAck, CmdError> {
    let pos = match position.as_str() {
        "top" => Position::Top,
        "bottom" => Position::Bottom,
        "before" => Position::Before(anchor_id.ok_or("position 'before' needs an anchor_id to place against")?),
        "after" => Position::After(anchor_id.ok_or("position 'after' needs an anchor_id to place against")?),
        other => return Err(format!("position '{other}' is not one of top / bottom / before / after").into()),
    };
    with_store_mut(|store| {
        store.automation_role_move(id, pos)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["automations"]))
}

/// **Take a role out of its automation** ([`amenbo_core::ops::automation::role_delete`]). The
/// placement its round started at stays on the picture, and the tasks worked under it are left with
/// no role. The last role is not taken out — core's refusal, passed on.
#[tauri::command]
pub fn automation_role_remove(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.automation_role_delete(id)?;
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
    let placed = with_store_mut(|store| Ok(store.automation_placement_insert(edge_id, action)?))?;
    Ok(WriteAck::new(&["automations", "automationActions"]).placed(placed.closer.as_ref()))
}

/// **Put a library action on after a way out that says nothing yet**
/// ([`amenbo_core::ops::automation::placement_insert_at_exit`]) — [`automation_step_insert`] for a way
/// out with no line to press. The way out comes to point at the new placement, whose own ways out are
/// left saying nothing.
#[tauri::command]
pub fn automation_step_insert_at_exit(
    from_id: i64,
    exit_name: Option<String>,
    action: i64,
) -> Result<WriteAck, CmdError> {
    let placed = with_store_mut(|store| {
        Ok(store.automation_placement_insert_at_exit(from_id, exit_name.as_deref(), action)?)
    })?;
    Ok(WriteAck::new(&["automations", "automationActions"]).placed(placed.closer.as_ref()))
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
    Ok(WriteAck::new(&["automations", "automationActions"]))
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
    // Asked the way the save asks it (`amenbo_core::ops::automation::save`): without the agents and
    // models of this machine, which are the launch's to judge and not the definition's, and with
    // Amenbo's own default agent, which a step nobody chose for falls back to.
    let default_agent = store.automation_default_agent(id)?;
    let conn = store.read_model().conn();
    let save_blocks =
        automation_run::check(conn, id, None, automation_run::nothing_asked(), default_agent.as_deref())?;
    // Read off the draft, as `view` is.
    let mut who = Who {
        carriers: automation_run::carriers(conn, id, ReadsFrom::Draft, default_agent.as_deref())?,
        rounds: automation_run::rounds_by_placement(conn, id, ReadsFrom::Draft)?,
        passes: automation_run::passes_by_role(conn, id, ReadsFrom::Draft)?,
        passed_titles: BTreeMap::new(),
    };
    for task in who.passes.iter().filter_map(|pass| pass.last_task_id) {
        if let Some(title) = read::task_title(conn, task)? {
            who.passed_titles.insert(task, title);
        }
    }
    Ok(Some(detail_dto(view, held_by, &save_blocks, who, default_agent)))
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
    let default_agent = store.automation_default_agent(id)?;
    let unmet = automation_run::check(
        store.read_model().conn(),
        id,
        agents.as_deref(),
        &crate::agent_models::offered_here(),
        default_agent.as_deref(),
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

/// **What a launch of this automation asks for** — nothing (`AMB-D-1123`). Work is handed to an
/// automation as a task with a role, filed before a run rather than read by its entry at launch, so the
/// dialog every press opens asks for nothing; a task to hand over is filed as any other is.
#[tauri::command]
pub fn automation_launch_asks(id: i64) -> Result<AutomationLaunchAsksDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_launch_asks");
    let store = open_store_read()?;
    read::automation(store.read_model().conn(), id)?
        .ok_or_else(|| amenbo_core::Error::not_found(format!("automation '{id}' not found")))?;
    Ok(AutomationLaunchAsksDto { reads: "nothing".to_string(), axes: Vec::new() })
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
/// **One press runs side by side.** The run is started in a launch — the one standing for the
/// automation, or a new one — and the watch starts more in it up to the automation's roles' headcount
/// (`amenbo_core::ops::automation_run::seats`) while there are tasks to take
/// (`amenbo_core::ops::automation_run::top_up`). The answer names that launch.
///
/// **The press hands the run nothing.** Work reaches an automation as a task filed with one of its
/// roles, its files attached to that task (`AMB-D-1123`).
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
        default_agent: None,
        startable: agents.as_deref(),
        models: &offered,
        workspace_open: Some(workspace_open),
        by: Some(ActorKind::Human),
    };
    let started = with_store_mut(|store| {
        let started = store.automation_launch(id, &by)?;
        // A start that made no run still opened the launch the run started once a task turns up joins.
        let launch = match &started {
            automation_run::Started::Run(run) => run.launch_id,
            automation_run::Started::Waits(_) => {
                read::automation_launch_standing(store.read_model().conn(), id)?.map(|launch| launch.id)
            }
        };
        Ok((started, launch))
    })?;
    let (started, launch) = started;
    crate::automation_watch::wake();
    Ok(AutomationRunStartedDto {
        run: match started {
            automation_run::Started::Run(run) => Some(run.id),
            automation_run::Started::Waits(_) => None,
        },
        launch,
    })
}

/// **Walk this automation through as a test run** — the build screen's "test run"
/// ([`amenbo_core::ops::automation_rehearse`]). The launch's own check refuses it where it would refuse
/// a launch, and every step is opened on the road a run takes, with no agent started, no built-in
/// carried out and nothing kept.
///
/// **It is handed a title, notes and a classification**, filed as a task under the first role before
/// the walk. Nothing is kept, so there is no task for a file to go onto; no pane is opened, so whether
/// the workspace stands is nobody's concern. `agents` and the models are asked as the launch asks them,
/// so a step the press would be refused over is one the test run stops at.
#[tauri::command]
pub fn automation_test_run(
    id: i64,
    agents: Option<Vec<String>>,
    title: Option<String>,
    notes: Option<String>,
    classification: Option<Vec<(String, String)>>,
) -> Result<AutomationTestRunDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_test_run");
    let offered = crate::agent_models::offered_here();
    let by = automation_run::Launcher {
        default_agent: None,
        startable: agents.as_deref(),
        models: &offered,
        workspace_open: None,
        by: Some(ActorKind::Human),
    };
    let handed = automation_run::HandedAtLaunch {
        title,
        notes,
        classification: classification.unwrap_or_default(),
        ..Default::default()
    };
    let walked = with_store_mut(|store| Ok(store.automation_rehearse(id, &by, &handed)?))?;
    Ok(AutomationTestRunDto {
        steps: walked
            .steps
            .into_iter()
            .map(|step| AutomationTestStepDto {
                placement: step.placement_id,
                name: step.name,
                builtin: step.builtin,
                agent: step.agent,
                model: step.model,
                prompt: step.prompt,
                folder: step.folder,
                exit: step.exit,
            })
            .collect(),
        status: walked.status.as_str(),
        stopped_reason: walked.stopped_reason.map(|one| one.as_str()),
        missing: walked.missing,
        no_agent: walked.no_agent,
        cut: walked.cut.map(|cut| match cut {
            Cut::Looped => "looped",
            Cut::TooLong => "too_long",
        }),
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
    store.automation_acknowledge(run_id, ActorKind::Human)?;
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
    // Whether the step under way may wait for a person: read only of a step still open on a run still
    // going, for a step that has left or a run that has stopped waits for nobody.
    let interactive = run.status == amenbo_core::model::AutomationRunStatus::Running
        && steps.last().is_some_and(|last| last.ended_at.is_none())
        && last_def.as_ref().is_some_and(|def| def.interactive);
    let step_name = last_def.map(|def| def.name);
    // The version it goes on from: the newest copy of its entry, which a run back at its entry has
    // taken afresh before its first step is opened (`take_up_newer`).
    let version = amenbo_core::ops::automation_run::entry_def(conn, run.id)?
        .and_then(|def| def.automation_version);
    // The stretch it is in now. A run walks one per task, and a run between tasks is on none.
    let stretch = read::automation_run_task_last(conn, run.id)?;
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
        launch: run.launch_id,
        automation_name: read::automation(conn, run.automation_id)?
            .map(|one| one.name)
            .unwrap_or_default(),
        status: run.status.as_str(),
        started_at: run.started_at.map(|at| at.to_rfc3339_z()),
        ended_at: run.ended_at.map(|at| at.to_rfc3339_z()),
        pause_requested: run.pause_requested,
        pause_before_next_task: run.pause_before_next_task,
        pausable_before_next_task: amenbo_core::ops::automation_stop::pauses_before_next_task(
            conn, &run,
        )?,
        waiting: run.status == amenbo_core::model::AutomationRunStatus::Running
            && amenbo_core::ops::automation_run::is_waiting(conn, run.id)?,
        waits_for_room: run.status == amenbo_core::model::AutomationRunStatus::Running
            && amenbo_core::ops::automation_run::waits_for_room(conn, run.id)?.is_some(),
        stopped_reason: run.stopped_reason.map(|one| one.as_str()),
        version,
        step_name,
        interactive,
        builtin,
        action_name,
        placement,
        steps_done: steps.len(),
        exit_name,
        task: worked_task(store, stretch.as_ref().map(|one| one.id))?,
        role: stretch.as_ref().and_then(|one| one.role_id),
        role_started_at: stretch.as_ref().and_then(|one| one.started_at).map(|at| at.to_rfc3339_z()),
        incident: incident_of(store, stretch.as_ref())?,
        passes: passes_of(store, run.id, stretch.as_ref().map(|one| one.id))?,
        report_withheld,
        acknowledged: run.acknowledged_at.is_some(),
        acknowledged_by: run.acknowledged_by_kind.map(|by| by.as_str()),
        output_tail: None,
        exit_code: None,
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
    let mut cards = run_cards(&store, run_ids)?;
    for card in &mut cards {
        (card.output_tail, card.exit_code) = last_tail(&store, card.run)?;
    }
    Ok(cards)
}

/// **The run whose failed round the task `task_id` was filed as**
/// ([`read::automation_run_task_of_incident`]) — what the task's pane opens the round's terminal by.
/// `None` for a task no failed round was filed as, and where the run is gone.
///
/// Read whatever the run stands at, an acknowledged failure on the "history" tab included
/// (`AMB-D-955`): the task is the way back to the round that fell for as long as it stands.
#[tauri::command]
pub fn automation_run_of_incident(task_id: i64) -> Result<Option<AutomationRunCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_run_of_incident");
    let store = open_store_read()?;
    let Some(round) = read::automation_run_task_of_incident(store.read_model().conn(), task_id)? else {
        return Ok(None);
    };
    Ok(run_cards(&store, vec![round.run_id])?.into_iter().next())
}

/// **The failed rounds' tasks waiting in one launch for the role each was filed to**, in the order they
/// were filed ([`automation_run::incidents_waiting_in_launch`]) — what the run's pane draws a role
/// that an incident has reached by, whether or not a round of that role is under way. A task deleted
/// on the way is left out.
#[tauri::command]
pub fn automation_launch_incidents(launch_id: i64) -> Result<Vec<AutomationLaunchIncidentDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_launch_incidents");
    let store = open_store_read()?;
    let conn = store.read_model().conn();
    let mut out = Vec::new();
    for incident in automation_run::incidents_waiting_in_launch(conn, launch_id)? {
        let Some(title) = read::task_title(conn, incident.task_id)? else { continue };
        out.push(AutomationLaunchIncidentDto {
            task: incident.task_id,
            task_ref: amenbo_core::idref::task(incident.task_id),
            title,
            role: incident.role_id,
            from_role: incident.from_role_id,
            run: incident.run_id,
            run_task: incident.run_task_id,
            filed_at: incident.filed_at.map(|at| at.to_rfc3339_z()),
        });
    }
    Ok(out)
}

/// **One launch as its pane draws it, read at once** ([`automation_run::launch_state`]) — where the launch
/// stands, and for every role, in the order they are defined in, its headcount beside the definition's,
/// how many are under way and still to go, whether it is held, the runs in its seats, its tasks
/// waiting to be taken with the first three of them, and what keeps them from being taken where none
/// is (`AMB-D-999`). The pane draws one launch from this one read, and
/// reads it again when the store changes.
#[tauri::command]
pub fn automation_launch_state(launch_id: i64) -> Result<AutomationLaunchStateDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_launch_state");
    let store = open_store_read()?;
    let state = store.automation_launch_state(launch_id)?;
    let mut roles = Vec::with_capacity(state.roles.len());
    for role in state.roles {
        let counted = role.headcount;
        roles.push(AutomationLaunchRoleStateDto {
            role: counted.role_id,
            name: role.name,
            icon: role.icon,
            defined: counted.defined,
            headcount: counted.headcount,
            under_way: counted.under_way.len(),
            to_let_go: counted.to_let_go,
            held: counted.held,
            runs: run_cards(&store, counted.under_way)?,
            waiting: role.waiting,
            first_waiting: role
                .first_waiting
                .into_iter()
                .map(|task| AutomationWaitingTaskDto {
                    task: task.task_id,
                    task_ref: amenbo_core::idref::task(task.task_id),
                    title: task.title,
                    from_role: task.from_role_id,
                })
                .collect(),
            held_back: role.held_back.map(|held| held_back_dto(state.launch.project_id, held)),
        });
    }
    Ok(AutomationLaunchStateDto {
        launch: state.launch.id,
        project: state.launch.project_id,
        automation: state.launch.automation_id,
        automation_name: state.automation_name,
        status: state.launch.status.as_str(),
        max_headcount: amenbo_core::model::MAX_ROLE_HEADCOUNT,
        roles,
        failed: failed_in_launch(&store, launch_id)?,
    })
}

/// **The failed rounds of one launch**, newest first ([`automation_run::failed_rounds_in_launch`]) —
/// one line each under the launch's pane. A round whose filed task has since been deleted is left out.
fn failed_in_launch(store: &amenbo_core::Store, launch_id: i64) -> Result<Vec<AutomationLaunchFailedDto>, CmdError> {
    let conn = store.read_model().conn();
    let mut out = Vec::new();
    for round in automation_run::failed_rounds_in_launch(conn, launch_id)? {
        let Some(incident) = filed_as(store, round.incident_task_id, round.failed_at, round.seat)? else {
            continue;
        };
        let task = match round.task_id {
            Some(id) => read::task(conn, id)?.map(|_| amenbo_core::idref::task(id)),
            None => None,
        };
        out.push(AutomationLaunchFailedDto { run: round.run_id, role: round.role_id, task, incident });
    }
    Ok(out)
}

/// **What a run's last step left to be read** — what a script printed, once its program has ended
/// (`AMB-D-1016`), or the screen an agent's step had as it failed its run
/// ([`amenbo_core::ops::automation_stop::keep_failed_screen`]). An empty one is every other step — a
/// script still running among them, and a step failed as a crash on the way back up (`AMB-D-961`).
///
/// Beside it, what the agent's program exited with, where that was kept with its screen.
fn last_tail(store: &amenbo_core::Store, run_id: i64) -> Result<(Option<String>, Option<i64>), CmdError> {
    let steps = read::automation_run_steps_of(store.read_model().conn(), run_id)?;
    Ok(steps
        .into_iter()
        .last()
        .filter(|last| !last.output_tail.is_empty())
        .map_or((None, None), |last| (Some(last.output_tail), last.exit_code)))
}

/// **What is keeping a run waiting** (`AMB-D-999`) — the tasks the built-in it waits on could take but
/// for what stops them, counted by that. `None` for a run that is not waiting on a built-in.
///
/// Asked by the pane as the wait begins and again when the store changes; the watch never asks it,
/// since every task its filter matches is read.
#[tauri::command]
pub fn automation_run_held_back(run_id: i64) -> Result<Option<AutomationHeldBackDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_run_held_back");
    let store = open_store_read()?;
    let conn = store.read_model().conn();
    let Some(held) = automation_run::held_back(conn, run_id)? else {
        return Ok(None);
    };
    let Some(run) = read::automation_run(conn, run_id)? else {
        return Ok(None);
    };
    Ok(Some(held_back_dto(run.project_id, held)))
}

/// **What is keeping tasks waiting, as the GUI reads it** ([`automation_builtin::HeldBack`]) — for a run
/// ([`automation_run_held_back`]) and for each role of a launch ([`automation_launch_state`]) alike.
fn held_back_dto(project: i64, held: automation_builtin::HeldBack) -> AutomationHeldBackDto {
    let record = |r: automation_builtin::HeldByRecord| AutomationHeldByRecordDto {
        id: r.id,
        title: r.title,
        count: r.count,
    };
    AutomationHeldBackDto {
        project,
        tasks: held.tasks,
        values: held
            .values
            .into_iter()
            .map(|v| AutomationHeldByValueDto {
                dimension_id: v.dimension_id,
                axis: v.axis,
                value: v.value,
                count: v.count,
            })
            .collect(),
        blockers: held.blockers.into_iter().map(record).collect(),
        decisions: held.decisions.into_iter().map(record).collect(),
        not_started: held.not_started,
        first_start: held.first_start.map(ScheduleDto::from),
        drafts: held.drafts,
    }
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
/// `run_def_id` names which step, and **`None` is the first one** — the copy of the step the round of
/// the run's role starts at ([`amenbo_core::ops::automation_run::entry_def`]). That is the whole of what a launch knows to
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
        // Uncoded on purpose. An automation where a run has nowhere to start is refused at the launch
        // check (`amenbo_core::ops::automation_run::Unmet::RoleStartsNowhere`), so the only way to
        // reach this is where it starts being taken off while a run of it is under way — which no
        // screen puts in front of anybody, and a code is split off a family where one does.
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

/// **Pause a run** — it settles at the end of the action under way
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

/// **Pause a run before it takes its next task**
/// ([`amenbo_core::ops::automation_stop::pause_run_before_next_task`], `AMB-D-1019`). Not a
/// `WriteAck` write, for the reason [`automation_run_pause`] is not.
#[tauri::command]
pub fn automation_pause_before_next_task(run_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_pause_run_before_next_task(run_id)?;
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
///
/// The agents and the models are asked here as a launch asks them ([`automation_launch`]): a run
/// paused before its next task is copied down afresh and checked as a launch is.
#[tauri::command]
pub fn automation_run_resume(run_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    let startable = amenbo_core::wake::startable_ids(&store.config);
    let offered = crate::agent_models::offered_here();
    store.automation_resume(run_id, startable.as_deref(), &offered)?;
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
///
/// **Only for a run that is running or paused** ([`runs_still_standing`]): a run stopped — from its
/// pane or from the command line — is not told again, so the pane closed on it stays closed.
#[tauri::command]
pub fn automation_steps_standing(
    app: tauri::AppHandle,
) -> Result<Vec<AutomationStepOpenDto>, CmdError> {
    let kept: Vec<AutomationStepOpenDto> = {
        let standing = app.state::<StepsStanding>();
        let standing = standing.0.lock().expect("steps standing lock");
        standing
            .values()
            .filter(|one| {
                one.builtin.is_some()
                    || one
                        .step
                        .as_ref()
                        .is_some_and(|step| crate::pty::is_open(&app, &step.session))
            })
            .cloned()
            .collect()
    };
    if kept.is_empty() {
        return Ok(kept);
    }
    let store = open_store_read()?;
    let conn = store.read_model().conn();
    let live = runs_still_standing(kept.iter().map(|one| one.run), |run| {
        Ok(read::automation_run(conn, run)?.map(|it| it.status))
    })?;
    let mut open: Vec<AutomationStepOpenDto> =
        kept.into_iter().filter(|one| live.contains(&one.run)).collect();
    open.sort_by_key(|one| one.run);
    Ok(open)
}

/// **The runs whose pane still has something to stand on** — those running or paused. A paused run
/// may be taken up again (`AMB-D-1015`), and a run gone from the store, or over, has nothing left.
pub(crate) fn runs_still_standing(
    runs: impl IntoIterator<Item = i64>,
    mut status_of: impl FnMut(i64) -> Result<Option<AutomationRunStatus>, CmdError>,
) -> Result<HashSet<i64>, CmdError> {
    let mut live = HashSet::new();
    for run in runs {
        let status = status_of(run)?;
        if matches!(status, Some(AutomationRunStatus::Running | AutomationRunStatus::Paused)) {
            live.insert(run);
        }
    }
    Ok(live)
}

/// The runs that have something kept for a face coming up ([`StepsStanding`]).
pub(crate) fn runs_standing(app: &tauri::AppHandle) -> Vec<i64> {
    let standing = app.state::<StepsStanding>();
    let standing = standing.0.lock().expect("steps standing lock");
    standing.keys().copied().collect()
}

/// **Forget what was kept for a run that is over**, so a face coming up later does not stand its pane
/// again ([`StepsStanding`]).
pub(crate) fn forget_standing(app: &tauri::AppHandle, run: i64) {
    let standing = app.state::<StepsStanding>();
    standing.0.lock().expect("steps standing lock").remove(&run);
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
    // **Not where it would wait for a task** (`AMB-D-969`): nothing is carried out — the run ends there
    // and its seat is left for the launch to fill once a task turns up (`AMB-D-1126`).
    //
    // **Nor while it waits for room** in an action that takes one run at a time: nothing is carried
    // out until the action is free, and the watch opens it again on every look.
    let mut about_to = read::automation_run_def(store.read_model().conn(), def_id)?
        .filter(|def| def.builtin.is_some());
    if about_to.is_some()
        && (amenbo_core::ops::automation_run::is_waiting(store.read_model().conn(), run_id)?
            || amenbo_core::ops::automation_run::waits_for_room(store.read_model().conn(), run_id)?.is_some())
    {
        about_to = None;
    }
    if let Some(def) = about_to {
        let (run, builtin) = builtin_about_to(store, run_id, &def)?;
        // The card takes the pane over from the step before, whose program would otherwise go on with
        // no pane left to draw it.
        crate::pty::end_steps_of(app, run_id);
        tell(app, AutomationStepOpenDto {
            run: run_id,
            project: run.project_id,
            launch: run.launch_id,
            step: None,
            builtin: Some(builtin),
            session: None,
            missing: Vec::new(),
        });
    }
    let opened = store.automation_step_open(run_id, def_id, startable.as_deref())?;
    let mut script_to_run = None;
    let mut session = None;
    let (run, step, builtin, missing) = match opened {
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
                def.model.clone(),
                ready.text.clone(),
            )?;
            let automation = run.automation_id;
            (
                run,
                Some(AutomationStepRunDto {
                    run_step: ready.run_step.id,
                    automation,
                    placement: def.placement_id,
                    automation_name: automation_name(store, automation)?,
                    name: def.name.clone(),
                    action_name: placed_action_name(store, def.placement_id)?,
                    task: worked_task(store, ready.run_step.run_task_id)?,
                    session,
                    agent: def.agent.clone(),
                    model: def.model.clone(),
                    folder,
                    interactive: def.interactive,
                    expected_minutes: def.expected_minutes_or_default(),
                    started_at: ready.run_step.started_at.map(|at| at.to_rfc3339_z()),
                }),
                None,
                Vec::new(),
            )
        }
        // What a step's pane is told about is its own step. No other run is in the event and none is
        // opened here: what the watch looks for is a run `running` with nothing open (`AMB-D-945`).
        Opened::Stopped { run, missing, .. } => (run, None, None, missing),
        // Nothing is named in the event for this one. `missing` is about inputs, and what a reader is
        // owed here is on the run's own row — the running tab says which ending it was, and the line
        // core left on the task says how far it got (`amenbo_core::ops::automation_stop`).
        Opened::NoAgent { run, agent } => {
            log::info!("run {run_id} asked for {agent}, which this machine cannot start");
            (run, None, None, Vec::new())
        }
        // Nothing to name here either: the run failed before this step opened, and its row says why
        // (`AMB-D-967`). The task before is back in `todo`, with the line core left on it.
        Opened::LeftTaskOpen { run } => {
            log::warn!("run {run_id} went for its next task with the last one still in progress");
            (run, None, None, Vec::new())
        }
        // The run was paused before this step, which was not opened: before its next task
        // (`AMB-D-1009`), or where a pause asked for met a built-in that would wait. The paused run has
        // no pane to stand on until it is picked up again.
        Opened::Waiting { run } => (run, None, None, Vec::new()),
        // The built-in that takes a task found none to take (`AMB-D-969`): the run ended there with
        // nothing written, and its seat is filled once a task turns up (`AMB-D-1126`). Its pane is
        // taken down.
        Opened::LeftToWait { run } => {
            log::info!("run {run_id} found no task to take, and its seat is left for the next one");
            (run, None, None, Vec::new())
        }
        // The run's seat was one still to go — the headcount of its role lowered on the launch, or the
        // role stopped taking tasks for it — so it ended where it came to take its next task, with none
        // taken. Its pane is taken down.
        Opened::LetGo { run } => {
            log::info!("run {run_id} let its seat go before taking its next task");
            (run, None, None, Vec::new())
        }
        // The action this step goes into takes one run at a time, and another run is in it or came to
        // it first. Nothing was written and nothing moved, so the pane is left as it stands and the
        // watch is not woken: it opens the step again on its next look. A pause asked for while it
        // waited took hold here instead, and the paused run has no pane to stand on.
        Opened::WaitsForRoom { run, .. } => match run.status {
            amenbo_core::model::AutomationRunStatus::Running => {
                return Ok(AutomationStepOpenDto {
                    run: run_id,
                    project: run.project_id,
                    launch: run.launch_id,
                    step: None,
                    builtin: None,
                    session: None,
                    missing: Vec::new(),
                });
            }
            _ => (run, None, None, Vec::new()),
        },
        // A built-in that holds its step open — the one that waits (`AMB-D-983`). Nothing was carried
        // out and no terminal is opened: the step stands under way until its time has come, and the
        // pane stands on its card, saying when that is.
        Opened::Holding { run_step_id } => {
            log::info!("run {run_id} holds built-in step {run_step_id} open");
            let (run, builtin) = builtin_of_step(store, run_id, run_step_id)?;
            (run, None, Some(builtin), Vec::new())
        }
        // A built-in has already been carried out and has reported (`AMB-D-964`), so there is no
        // terminal to stand a pane on. The run is now standing between two steps — or has ended — and
        // the watch woken below reads which, the same as after an agent's report. What the pane is
        // told is the same card, now carried out, with the task it may have taken.
        Opened::Carried { run_step_id, .. } => {
            log::info!("run {run_id} carried out built-in step {run_step_id}");
            let (run, builtin) = builtin_of_step(store, run_id, run_step_id)?;
            (run, None, Some(builtin), Vec::new())
        }
        // A script step (`AMB-D-1016`): its execution stands under way, and its program is started
        // here on a terminal nobody writes into, whose session goes out with the step for the pane to
        // take up. It is waited for on a thread of its own (`run_script`) — it may take hours, and
        // nothing here waits for it.
        Opened::Script(script) => {
            let run_step_id = script.run_step.id;
            log::info!("run {run_id} opened script step {run_step_id} ({})", script.script.program);
            let (run, builtin) = builtin_of_step(store, run_id, run_step_id)?;
            // The script takes the pane over from the step before, as a built-in's card does.
            crate::pty::end_steps_of(app, run_id);
            let started = start_script(app, store, run_id, &script);
            session = started.as_ref().ok().map(|started| started.terminal.session.clone());
            script_to_run = Some((run_step_id, started));
            (run, None, Some(builtin), Vec::new())
        }
    };
    // The run has just moved, so the thread that keeps it going looks again now rather than sleeping
    // out the interval it was on (`crate::automation_watch`). Called from the watch's own path too,
    // where it costs nothing: that loop is about to come round anyway.
    crate::automation_watch::wake();
    let dto = AutomationStepOpenDto {
        run: run_id,
        project: run.project_id,
        launch: run.launch_id,
        step,
        builtin,
        session,
        missing,
    };
    tell(app, dto.clone());
    // Waited for only once its card has been told under way, so a program that ends at once is not
    // told ended before it.
    if let Some((run_step_id, started)) = script_to_run {
        let app = app.clone();
        std::thread::spawn(move || run_script(&app, run_id, run_step_id, started));
    }
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

/// **Take up what has been saved since a run's copy was taken, before the step its round starts at is
/// opened** (`AMB-D-1015`, [`automation_run::take_up_newer`]) — and answer the step to open now, or
/// `None` where the newer version did not pass and the run failed.
///
/// **Asked of the store before it is written to.** The watch asks this of every run standing before a
/// step on every look, and nearly always nothing has been saved since: where the round starts
/// ([`automation_run::entry_def`]) and the versions are read first, and the write is opened only where
/// there is something newer to take up.
///
/// **The card the run stood on is dropped**: opening the new entry tells its own card, rather than
/// handing back the old one ([`open_one`]). A run that failed is told
/// with nothing to stand on, which takes its pane down.
pub(crate) fn take_up_newer(
    app: &tauri::AppHandle,
    run_id: i64,
    def: amenbo_core::model::AutomationRunDef,
) -> Result<Option<amenbo_core::model::AutomationRunDef>, CmdError> {
    let newer = {
        let store = open_store_read()?;
        let conn = store.read_model().conn();
        let Some(run) = read::automation_run(conn, run_id)? else { return Ok(Some(def)) };
        if !matches!(automation_run::entry_def(conn, run_id)?, Some(start) if start.id == def.id) {
            return Ok(Some(def));
        }
        read::automation_version_latest(conn, run.automation_id)?
            .is_some_and(|saved| !matches!(def.automation_version, Some(copied) if copied >= saved.version))
    };
    if !newer {
        return Ok(Some(def));
    }
    let mut store = crate::commands::open_store()?;
    let startable = amenbo_core::wake::startable_ids(&store.config);
    let offered = crate::agent_models::offered_here();
    let taken = store.automation_take_up_newer(run_id, startable.as_deref(), &offered)?;
    match taken {
        automation_run::TakenUp::Same => Ok(Some(def)),
        automation_run::TakenUp::Copied(entry) => {
            log::info!("run {run_id} took up the newer saved version of its automation");
            let standing = app.state::<StepsStanding>();
            standing.0.lock().expect("steps standing lock").remove(&run_id);
            Ok(Some(*entry))
        }
        automation_run::TakenUp::Failed(run) => {
            log::info!("run {run_id} failed the check of the newer saved version of its automation");
            tell(app, AutomationStepOpenDto {
                run: run_id,
                project: run.project_id,
                launch: run.launch_id,
                step: None,
                builtin: None,
                session: None,
                missing: Vec::new(),
            });
            Ok(None)
        }
    }
}

/// **A built-in about to be carried out**, as its pane is told before Amenbo starts on it — with the
/// run, whose project and launch say which pane that is.
///
/// Nothing is written for it yet, so the task is the one the stretch under way is on. A built-in that takes a task opens a stretch of its
/// own (`amenbo_core::ops::automation_step::open`), so it is on none until it has taken one.
fn builtin_about_to(
    store: &amenbo_core::Store,
    run_id: i64,
    def: &amenbo_core::model::AutomationRunDef,
) -> Result<(amenbo_core::model::AutomationRun, AutomationBuiltinRunDto), CmdError> {
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
    let builtin = AutomationBuiltinRunDto {
        automation: run.automation_id,
        run_step: None,
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
        held_until: None,
        program: def.script.as_ref().map(|s| s.program.clone()),
        args: def.script.as_ref().map(|s| s.args.clone()),
        expected_minutes: def.script.as_ref().map(|_| def.expected_minutes_or_default()),
        started_at: None,
    };
    Ok((run, builtin))
}

/// **A built-in's step as its card draws it**, read off the execution — carried out, or held open
/// until its time comes (`AMB-D-983`). A held step is not finished and says when it ends; one that
/// has ended says the way out it left by. A script step's card is drawn by this too (`AMB-D-1016`),
/// with no key and the program it runs: under way while that runs, and finished once it has ended.
fn builtin_of_step(
    store: &amenbo_core::Store,
    run_id: i64,
    run_step_id: i64,
) -> Result<(amenbo_core::model::AutomationRun, AutomationBuiltinRunDto), CmdError> {
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
    let held = run_step.status == amenbo_core::model::AutomationRunStepStatus::Running;
    let held_until = match held {
        true => automation_builtin::held_until(conn, &run_step)?.map(|at| at.to_rfc3339_z()),
        false => None,
    };
    let builtin = AutomationBuiltinRunDto {
        automation: run.automation_id,
        run_step: Some(run_step_id),
        placement: def.placement_id,
        automation_name: automation_name(store, run.automation_id)?,
        name: def.name.clone(),
        key: def.builtin.clone().unwrap_or_default(),
        action_name: placed_action_name(store, def.placement_id)?,
        task: worked_task(store, run_step.run_task_id)?,
        finished: !held,
        waiting: false,
        looks_for: None,
        exit_name: left_by(&def, run_step.exit_id),
        held_until,
        program: def.script.as_ref().map(|s| s.program.clone()),
        args: def.script.as_ref().map(|s| s.args.clone()),
        expected_minutes: def.script.as_ref().map(|_| def.expected_minutes_or_default()),
        started_at: run_step.started_at.map(|at| at.to_rfc3339_z()),
    };
    Ok((run, builtin))
}

/// The automation a run was launched from, by the name it holds now — empty where it has gone.
fn automation_name(store: &amenbo_core::Store, automation_id: i64) -> Result<String, CmdError> {
    Ok(read::automation(store.read_model().conn(), automation_id)?.map(|one| one.name).unwrap_or_default())
}

/// **Tell the window again about a step whose task was taken after it was opened** (`AMB-T-5427`).
///
/// The task a stretch holds can change after its step was opened — deleted out from under it, say — by
/// another process, so nothing reaches here when it happens. (A step's own agent takes no task: a
/// built-in does, without a terminal, `AMB-D-964`.) What the watch does see is the run with its step
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

/// **End a held step of this run whose time has come**, and tell its pane (`AMB-D-983`) — answering
/// whether one did. Nothing else ends it: no timer is started when it opens, and the watch asks on the
/// look it already takes at a run with a step under way (`crate::automation_watch`).
///
/// From the report on it is the road an agent's report takes: the watch opens the next step on its next
/// look, and a pause asked for while it waited takes hold here. The card is told once more, carried out,
/// with the way out it left by — as a built-in carried out at once is ([`open_one`]).
pub(crate) fn time_up(app: &tauri::AppHandle, run_id: i64) -> Result<bool, CmdError> {
    let due = {
        let store = crate::commands::open_store_read()?;
        automation_builtin::due(store.read_model().conn(), run_id, amenbo_core::time::Timestamp::now())?
    };
    let Some(run_step_id) = due else { return Ok(false) };
    let mut store = crate::commands::open_store()?;
    store.automation_time_up(run_step_id)?;
    log::info!("run {run_id} waited out built-in step {run_step_id}");
    let (run, builtin) = builtin_of_step(&store, run_id, run_step_id)?;
    tell(app, AutomationStepOpenDto {
        run: run_id,
        project: run.project_id,
        launch: run.launch_id,
        step: None,
        builtin: Some(builtin),
        session: None,
        missing: Vec::new(),
    });
    crate::automation_watch::wake();
    Ok(true)
}

/// **A script step's program, started on its terminal** — what [`open_one`] leaves for [`run_script`].
struct StartedScript {
    prepared: amenbo_core::ops::automation_script::Prepared,
    terminal: crate::pty::ScriptTerminal,
    group: amenbo_core::sys::ProcessGroup,
}

/// **Start a script step's program** on a terminal nobody writes into (`crate::pty::open_script`),
/// with its folder written and its group taken, so stopping it stops every process it started
/// (`AMB-D-1016`) — or why it could not be, in the words the run's step is failed with.
///
/// An input whose file cannot be read out of the blob store is a program that could not be started,
/// and leaves by the error way out like one — rather than leaving the step under way with nothing
/// running it. A program whose group cannot be taken is ended again, for the same reason: one that
/// could not be stopped is not one to leave running.
fn start_script(
    app: &tauri::AppHandle,
    store: &amenbo_core::Store,
    run_id: i64,
    script: &amenbo_core::ops::automation_step::ScriptOpening,
) -> Result<StartedScript, String> {
    let given = script
        .given(&store.blobs())
        .map_err(|e| format!("its inputs could not be read: {}", CmdError::from(e).message_en))?;
    let prepared =
        amenbo_core::ops::automation_script::prepare(&script.script, &given, Some((run_id, script.run_step.id)))?;
    let terminal = crate::pty::open_script(app, run_id, &prepared).map_err(|e| e.message_en)?;
    let group = terminal
        .pid
        .ok_or_else(|| "the OS did not say which process it is".to_string())
        .and_then(|pid| amenbo_core::sys::ProcessGroup::adopt(pid).map_err(|e| e.to_string()));
    match group {
        Ok(group) => Ok(StartedScript { prepared, terminal, group }),
        Err(why) => {
            crate::pty::end_steps_of(app, run_id);
            Err(format!("it could not be held to be stopped: {why}"))
        }
    }
}

/// **Wait for a script step's program and write down what it came to** (`AMB-D-1016`), on the thread
/// [`open_one`] started for it.
///
/// What it printed is read off its terminal, where stdout and stderr were one stream, and its end is
/// kept with the escape sequences taken out ([`amenbo_core::ops::automation_script::tail_of`]). The
/// card is told again, ended, with the way out it left by, and the watch is woken to open the next
/// step — the road [`time_up`] takes. Where the step stops being under way while the program runs —
/// the run was force-cancelled, or ended some other way — the program is stopped with every process
/// it started ([`still_running`]), core refuses the write, and only the log says so.
///
/// Its terminal is left to the next step, as an agent's is, and not ended here: it ending by itself is
/// not a crash, since what it came to is written down here (`crate::pty::open_script`). Once the program
/// has ended the terminal stays for the run's pane to read what it wrote, until the next step takes it
/// away (`crate::pty::end_steps_of`) — the run being over does not (`crate::pty::runs_with_steps`).
///
/// Nothing here outlives the app: a step whose program was running when it went down is failed as a
/// crash on the way back up (`crate::automation_watch`).
fn run_script(
    app: &tauri::AppHandle,
    run_id: i64,
    run_step_id: i64,
    started: Result<StartedScript, String>,
) {
    use amenbo_core::ops::automation_script::{self, Ended, Ran};
    let (ran, session) = match started {
        Err(why) => {
            let ran = Ran { ended: Ended::NotStarted(why), output_tail: String::new() };
            (ran, None)
        }
        Ok(StartedScript { prepared, terminal, group }) => {
            let deadline = std::time::Instant::now() + prepared.timeout;
            let waited = automation_script::wait(
                &group,
                prepared.timeout,
                || terminal.exited(),
                || !still_running(run_step_id),
            );
            let output_tail = automation_script::tail_of(&terminal.output_until(deadline));
            let ran = Ran { ended: prepared.finish(waited), output_tail };
            (ran, Some(terminal.session))
        }
    };
    let told = crate::commands::open_store().and_then(|mut store| {
        store.automation_script_ran(run_step_id, &ran)?;
        builtin_of_step(&store, run_id, run_step_id)
    });
    match told {
        Ok((run, builtin)) => {
            log::info!("run {run_id} ran script step {run_step_id}");
            tell(app, AutomationStepOpenDto {
                run: run_id,
                project: run.project_id,
                launch: run.launch_id,
                step: None,
                builtin: Some(builtin),
                session,
                missing: Vec::new(),
            });
            crate::automation_watch::wake();
        }
        Err(e) => log::warn!("script step {run_step_id} of run {run_id} ended and was not written down: {e:?}"),
    }
}

/// **Is a script step still under way**, read from the store each time — so a run ended from anywhere,
/// a force-cancel from the CLI among them, stops the program and every process it started. A store that
/// cannot be read is taken as still running: the program then runs on to its end or its timeout.
fn still_running(run_step_id: i64) -> bool {
    let Ok(store) = crate::commands::open_store_read() else { return true };
    match read::automation_run_step(store.read_model().conn(), run_step_id) {
        Ok(Some(step)) => step.status == amenbo_core::model::AutomationRunStepStatus::Running,
        Ok(None) => false,
        Err(_) => true,
    }
}

/// **Where a step that names no folder is carried out, when its run is on no task with a working folder
/// on the disk**: the folder its project is bound to. The step's own folder and the task's are core's
/// answer ([`amenbo_core::ops::automation_step::Opening::folder`]), and this is asked only after both.
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
///
/// `screen` is what its terminal had on it as it ended, and `code` what its program exited with where
/// that could be asked. Where the run has failed — here, or by the step's own error way out before its
/// program ended — both are kept on the step ([`keep_failed_screen`]): the terminal leaves with its
/// program, and the task the round was filed as is read long after.
pub fn step_program_ended(run_step: i64, screen: &str, code: Option<i32>) {
    let ended = crate::commands::open_store().and_then(|mut store| {
        store.automation_step_ended(run_step).map_err(CmdError::from)
    });
    match ended {
        Ok(Some(ended)) => log::info!("run {} failed: step {run_step} ended without reporting", ended.run.id),
        Ok(None) => {}
        Err(e) => log::warn!("could not settle step {run_step} whose program ended: {e:?}"),
    }
    keep_failed_screen(run_step, screen, code);
}

/// **Keep the screen an agent's step had on its terminal, where the step failed its run**
/// ([`amenbo_core::ops::automation_stop::keep_failed_screen`]) — asked as the terminal goes, by its
/// program ending ([`step_program_ended`]) or by the app ending it for a run that is over
/// (`crate::automation_watch`). `code` is what the program exited with where it ended by itself, and
/// `None` where the app is ending it. A step whose run did not fail keeps nothing, and only the log
/// hears of a write that did not go through: the terminal is going either way.
pub(crate) fn keep_failed_screen(run_step: i64, screen: &str, code: Option<i32>) {
    let kept = crate::commands::open_store().and_then(|mut store| {
        store.automation_keep_failed_screen(run_step, screen, code.map(i64::from)).map_err(CmdError::from)
    });
    match kept {
        Ok(true) => log::info!("kept the screen of step {run_step}, which failed its run"),
        Ok(false) => {}
        Err(e) => log::warn!("could not keep the screen of step {run_step}: {e:?}"),
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

/// **The task the round `stretch` was filed as when it failed**, with when it failed — the end the
/// stretch was closed with as the task was filed — and the seat it failed in, as written on it
/// ([`amenbo_core::model::AutomationRunTask::seat`]). Nothing where the round filed none, or the task
/// has since been deleted.
fn incident_of(
    store: &amenbo_core::Store,
    stretch: Option<&amenbo_core::model::AutomationRunTask>,
) -> Result<Option<AutomationRunIncidentDto>, CmdError> {
    let Some(round) = stretch else { return Ok(None) };
    let Some(id) = round.incident_task_id else { return Ok(None) };
    filed_as(store, id, round.ended_at, round.seat.and_then(|n| u32::try_from(n).ok()))
}

/// **The task `id` a failed round was filed as**, with when the round failed and the seat it failed in,
/// as each failed round names it. Nothing where the task has since been deleted.
fn filed_as(
    store: &amenbo_core::Store,
    id: i64,
    ended: Option<amenbo_core::time::Timestamp>,
    seat: Option<u32>,
) -> Result<Option<AutomationRunIncidentDto>, CmdError> {
    let conn = store.read_model().conn();
    let Some(task) = read::task(conn, id)? else { return Ok(None) };
    Ok(Some(AutomationRunIncidentDto {
        id,
        r#ref: amenbo_core::idref::task(id),
        title: task.title,
        role: task.role_id,
        person: task.role_id.is_none()
            && task.assignee_kind == Some(ActorKind::Human),
        at: ended.map(|at| at.to_rfc3339_z()),
        seat,
    }))
}

/// **The tasks one run's rounds passed to another role**, each with its task's reference and title, and
/// whether the round `now` — the one the run is in now — passed it.
fn passes_of(
    store: &amenbo_core::Store,
    run_id: i64,
    now: Option<i64>,
) -> Result<Vec<AutomationRunPassDto>, CmdError> {
    let conn = store.read_model().conn();
    let mut out = Vec::new();
    for pass in automation_run::passes_of_run(conn, run_id)? {
        let title = match pass.task_id {
            Some(id) => read::task_title(conn, id)?,
            None => None,
        };
        let task = pass.task_id.filter(|_| title.is_some());
        out.push(AutomationRunPassDto {
            from_role: pass.from_role_id,
            to_role: pass.to_role_id,
            task,
            task_ref: task.map(amenbo_core::idref::task),
            task_title: title,
            current: now == Some(pass.run_task_id),
        });
    }
    Ok(out)
}

/// **Force-cancel a run** — stop it now, whatever it is in the middle of (`AMB-D-1002`). What the
/// force-cancel of a running run presses, once the person has said yes to what it may leave behind.
/// Closing the pane a run is drawn in does not stop it: that press is offered only once the run is
/// over (`app/src/shell/TerminalPane.tsx`).
///
/// The cleanup is core's and is the same one every other stop goes through
/// ([`amenbo_core::ops::automation_stop::stop`]): the task the run was working goes to `todo`, and
/// a line on that task says the run is not coming back. The terminal
/// standing in the pane is this side's to end — it is a process this side started, and core has
/// no window to end one from. The look the press wakes ends it (`crate::automation_watch`).
///
/// **A run that is over already is not an error here.** The press is a person's, and between
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

/// **Cancel a paused run** ([`amenbo_core::ops::automation_stop::cancel`], `AMB-D-1002`). Nothing is
/// under way in a paused run, so it is `canceled` on the spot and there is no terminal to end.
/// Refused for a run that is not paused: a running one is paused first, or force-cancelled
/// ([`automation_run_stop`]).
///
/// **It is not a `WriteAck` write**, for the reason [`automation_run_stop`] is not.
#[tauri::command]
pub fn automation_run_cancel(run_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_cancel(run_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Pause a launch** — every run of it still going settles at the end of the action under way, and the
/// watch starts no run in it until it is picked up again
/// ([`amenbo_core::ops::automation_stop::pause_launch`]). Not a `WriteAck` write, for the reason
/// [`automation_run_pause`] is not.
#[tauri::command]
pub fn automation_launch_pause(launch_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_launch_pause(launch_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Pause a launch before each of its runs takes its next task**
/// ([`amenbo_core::ops::automation_stop::pause_launch_before_next_task`], `AMB-D-1019`). Refused where a
/// run of it takes no tasks.
#[tauri::command]
pub fn automation_launch_pause_before_next_task(launch_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_launch_pause_before_next_task(launch_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Pick a paused launch up again** ([`amenbo_core::ops::automation_stop::resume_launch`]). Each run
/// is picked up as [`automation_run_resume`] picks one up, and the agents and models are asked the
/// same way; the steps are opened by the watch.
#[tauri::command]
pub fn automation_launch_resume(launch_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    let startable = amenbo_core::wake::startable_ids(&store.config);
    let offered = crate::agent_models::offered_here();
    store.automation_launch_resume(launch_id, startable.as_deref(), &offered)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Cancel a paused launch** ([`amenbo_core::ops::automation_stop::cancel_launch`], `AMB-D-1002`).
/// Refused while a run of it is running: the launch is paused first, or stopped
/// ([`automation_launch_stop`]).
#[tauri::command]
pub fn automation_launch_cancel(launch_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_launch_cancel(launch_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Stop a launch now** — the force-cancel of every run of it still going
/// ([`amenbo_core::ops::automation_stop::stop_launch`]). The terminals standing in their panes are ended
/// by the look the press wakes, as [`automation_run_stop`]'s are.
#[tauri::command]
pub fn automation_launch_stop(launch_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_launch_stop(launch_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Change how many are in one role for this launch alone**
/// ([`amenbo_core::ops::automation_run::launch_headcount_set`], `AMB-D-1123`). The definition is left
/// as it is. Core refuses a headcount outside 1 to the cap (`AMB-D-1124`) and a role the launch's
/// definition does not have. A raise is taken up by the watch's top-up, so the press wakes it; a cut
/// lets seats go as their tasks end. Not a `WriteAck` write, for the reason [`automation_run_pause`]
/// is not.
#[tauri::command]
pub fn automation_launch_headcount_set(launch_id: i64, role_id: i64, headcount: u32) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_launch_headcount_set(launch_id, role_id, headcount)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Put one role of this launch back at its definition's headcount**
/// ([`amenbo_core::ops::automation_run::launch_headcount_reset`]). The watch is woken for the seats a
/// return to a larger headcount opens.
#[tauri::command]
pub fn automation_launch_headcount_reset(launch_id: i64, role_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_launch_headcount_reset(launch_id, role_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Stop one role of a launch taking tasks once the task under way is done**
/// ([`amenbo_core::ops::automation_stop::hold_launch_role`]). Its runs waiting to take a task end on the
/// spot; the watch is woken to settle them. A role is held apart from the whole launch's pause before
/// the next task (`AMB-D-1019`), so core refuses it while the whole launch is paused, and for a launch
/// that has ended.
#[tauri::command]
pub fn automation_launch_role_hold(launch_id: i64, role_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_launch_role_hold(launch_id, role_id)?;
    crate::automation_watch::wake();
    Ok(())
}

/// **Take one held role of a launch up again** ([`amenbo_core::ops::automation_stop::release_launch_role`]).
/// Its empty seats are filled by the watch's top-up, so the press wakes it. Refused as
/// [`automation_launch_role_hold`] is.
#[tauri::command]
pub fn automation_launch_role_release(launch_id: i64, role_id: i64) -> Result<(), CmdError> {
    let mut store = crate::commands::open_store()?;
    store.automation_launch_role_release(launch_id, role_id)?;
    crate::automation_watch::wake();
    Ok(())
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

/// **Who works each placement and carries out each step** — core's answers, read beside the view and
/// handed to [`detail_dto`] so the shaping below reads no store.
struct Who {
    /// Keyed by (placement, step) ([`automation_run::carriers`]).
    carriers: BTreeMap<(i64, i64), Carrier>,
    /// Placement id → the role whose round reaches it ([`automation_run::rounds_by_placement`]).
    rounds: BTreeMap<i64, i64>,
    /// What each role's round files for another role ([`automation_run::passes_by_role`]).
    passes: Vec<automation_run::RolePass>,
    /// Task id → title, for the task each of `passes` last passed on that is still there.
    passed_titles: BTreeMap<i64, String>,
}

/// One automation's whole definition, under the names the build screen draws it by.
fn detail_dto(
    view: automation_view::AutomationView,
    held_by: Vec<AutomationRunCardDto>,
    save_blocks: &[Unmet],
    mut who: Who,
    default_agent: Option<String>,
) -> AutomationDetailDto {
    let names = exit_names(view.placements.iter().flat_map(|p| p.exits.iter()));
    let wires = view.wires.iter().map(|w| wire_dto(w, &names, |id| view.port_name(id))).collect();
    let a = view.automation;
    // What the build screen labels a task a role passes on by before a run has passed one from there:
    // the action standing on the spot that files it.
    let spot_names: BTreeMap<i64, String> = view
        .placements
        .iter()
        .map(|p| (p.placement.id, p.action.as_ref().map(|one| one.name.clone()).unwrap_or_default()))
        .collect();
    let roles = view
        .roles
        .into_iter()
        .map(|role| {
            let saved_headcount = view.saved_headcounts.get(&role.id).copied();
            role_dto(role, saved_headcount, &who.passes, &spot_names, &who.passed_titles)
        })
        .collect();
    AutomationDetailDto {
        id: a.id,
        project_id: a.project_id,
        name: a.name,
        notes: a.notes,
        archived: a.archived,
        triggers: a.triggers.into_iter().map(Into::into).collect(),
        failures_to: a.failures_to.map(Into::into),
        failure_values: a.failure_values.iter().map(|value| value.value_id).collect(),
        edges: view.edges.into_iter().map(|e| edge_dto(e, &names)).collect(),
        wires,
        roles,
        max_headcount: amenbo_core::model::MAX_ROLE_HEADCOUNT,
        placements: view.placements.into_iter().map(|p| placement_dto(p, &mut who)).collect(),
        held_by,
        saved: view.saved.map(saved_dto),
        unsaved: view.unsaved,
        save_blocks: save_blocks.iter().map(block_dto).collect(),
        default_agent,
    }
}

/// The newest saved version, as the build screen's head names it.
fn saved_dto(saved: automation_view::SavedVersion) -> AutomationSavedDto {
    AutomationSavedDto { version: saved.version, saved_at: saved.saved_at.to_rfc3339_z() }
}

/// One library action's whole definition, under the names the action build screen draws it by.
fn action_detail_dto(
    view: automation_view::ActionView,
    held_by: Vec<AutomationRunCardDto>,
    placed_on: Vec<AutomationPlacedOnDto>,
    save_blocks: &[Unmet],
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
        draft: action.draft,
        one_at_a_time: action.one_at_a_time,
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
        saved: view.saved.map(saved_dto),
        unsaved: view.unsaved,
        save_blocks: save_blocks.iter().map(block_dto).collect(),
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
        script: step.script.map(Into::into),
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
fn placement_dto(view: automation_view::PlacementView, who: &mut Who) -> AutomationPlacementDto {
    let placement = view.placement;
    let action = view.action;
    let opens = view.entry_step;
    let never_leaves_by = action.as_ref().and_then(|one| one.builtin.as_deref()).and_then(|key| {
        automation_builtin::never_leaves_by(key, |setting| {
            view.settings.iter().find(|cfg| cfg.name == setting).and_then(|cfg| cfg.value.as_deref())
        })
    });
    AutomationPlacementDto {
        id: placement.id,
        name: action.as_ref().map(|one| one.name.clone()).unwrap_or_default(),
        action_id: placement.action_id,
        role_id: who.rounds.get(&placement.id).copied(),
        global: action.as_ref().is_some_and(|one| one.project_id.is_none()),
        builtin: action.as_ref().and_then(|one| one.builtin.clone()),
        draft: action.as_ref().is_some_and(|one| one.draft),
        version: view.version,
        latest_version: view.latest_version,
        never_leaves_by: never_leaves_by.map(str::to_string),
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
                carrier: who.carriers.remove(&(placement.id, one.step.id)).map(carrier_dto),
                step_id: one.step.id,
                name: one.step.name,
                interactive: one.step.interactive,
                script: one.step.script.is_some(),
                builtin: one.step.builtin.is_some(),
                agent: one.chosen.as_ref().map(|c| c.agent.clone()),
                model: one.chosen.and_then(|c| c.model),
            })
            .collect(),
    }
}

fn role_dto(
    role: AutomationRole,
    saved_headcount: Option<u32>,
    passes: &[automation_run::RolePass],
    spot_names: &BTreeMap<i64, String>,
    passed_titles: &BTreeMap<i64, String>,
) -> AutomationRoleDto {
    let passes = passes
        .iter()
        .filter(|pass| pass.from_role_id == role.id)
        .map(|pass| {
            let last = pass.last_task_id.and_then(|task| passed_titles.get(&task).map(|title| (task, title)));
            AutomationRolePassDto {
                to_role: pass.to_role_id,
                placement: pass.placement_id,
                name: spot_names.get(&pass.placement_id).cloned().unwrap_or_default(),
                depends: pass.depends,
                task_ref: last.map(|(task, _)| amenbo_core::idref::task(task)),
                task_title: last.map(|(_, title)| title.clone()),
            }
        })
        .collect();
    AutomationRoleDto {
        id: role.id,
        name: role.name,
        headcount: role.headcount,
        saved_headcount,
        starts_by: role.starts_by.as_str().to_string(),
        icon: role.icon,
        entry_placement_id: role.entry_placement_id,
        agent: role.agent,
        model: role.model,
        passes,
    }
}

fn carrier_dto(carrier: Carrier) -> AutomationCarrierDto {
    AutomationCarrierDto {
        agent: carrier.agent,
        model: carrier.model,
        chosen_at: carrier.at.into(),
        agent_by_step: carrier.agent_by_step,
        model_by_step: carrier.model_by_step,
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
    use crate::dto::{AutomationTriggerDto, AutomationTriggerWhenDto};

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

    /// **A run stopped has nothing for a face coming up to stand on**, however it was stopped; a
    /// paused one does, since it may be taken up again. A run the store no longer has is left out
    /// with the ones that are over.
    #[test]
    fn only_runs_running_or_paused_still_stand() {
        let statuses: HashMap<i64, AutomationRunStatus> = HashMap::from([
            (1, AutomationRunStatus::Running),
            (2, AutomationRunStatus::Paused),
            (3, AutomationRunStatus::Completed),
            (4, AutomationRunStatus::Failed),
            (5, AutomationRunStatus::Canceled),
        ]);
        let live = runs_still_standing(1..=6, |run| Ok(statuses.get(&run).copied()))
            .expect("nothing to fail");
        assert_eq!(live, HashSet::from([1, 2]));
    }

    /// **What holds tasks back reaches the GUI as core counted it** (`AMB-D-999`): the value with its
    /// axis's id and how many it holds, the count of tasks, and the first start day as `YYYY-MM-DD`. A
    /// role of a launch with nothing held back leaves it out.
    #[test]
    fn what_holds_tasks_back_is_handed_on_as_counted() {
        let held = automation_builtin::HeldBack {
            tasks: 3,
            values: vec![automation_builtin::HeldByValue {
                dimension_id: 4,
                axis: "Release".into(),
                value: "v1".into(),
                count: 2,
            }],
            blockers: vec![automation_builtin::HeldByRecord { id: 9, title: "the part before".into(), count: 1 }],
            decisions: Vec::new(),
            not_started: 1,
            first_start: chrono::NaiveDate::from_ymd_opt(2026, 12, 1).map(amenbo_core::model::Schedule::Day),
            drafts: 0,
        };
        assert_eq!(
            serde_json::to_value(held_back_dto(2, held)).expect("json"),
            serde_json::json!({
                "project": 2,
                "tasks": 3,
                "values": [{ "dimensionId": 4, "axis": "Release", "value": "v1", "count": 2 }],
                "blockers": [{ "id": 9, "title": "the part before", "count": 1 }],
                "decisions": [],
                "notStarted": 1,
                "firstStart": { "kind": "day", "day": "2026-12-01" },
                "drafts": 0,
            }),
        );
        // A first start at an instant is handed on to the instant, not rounded to the day it falls on.
        let at = amenbo_core::time::Timestamp::parse_rfc3339("2026-12-01T05:00:00Z").expect("instant");
        let held = automation_builtin::HeldBack {
            tasks: 1,
            not_started: 1,
            first_start: Some(amenbo_core::model::Schedule::At(at)),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(held_back_dto(2, held)).expect("json")["firstStart"],
            serde_json::json!({ "kind": "at", "at": "2026-12-01T05:00:00Z" }),
        );

        let role = |held_back: Option<AutomationHeldBackDto>| {
            let dto = AutomationLaunchRoleStateDto {
                role: 1,
                name: "Build".into(),
                icon: None,
                defined: 1,
                headcount: 1,
                under_way: 0,
                to_let_go: 0,
                held: false,
                runs: Vec::new(),
                waiting: 0,
                first_waiting: Vec::new(),
                held_back,
            };
            serde_json::to_value(dto).expect("json")
        };
        assert!(role(None).get("heldBack").is_none(), "nothing held back is left out");
        let named = role(Some(held_back_dto(2, automation_builtin::HeldBack { tasks: 1, ..Default::default() })));
        assert_eq!(named["heldBack"]["tasks"], 1);
    }

    /// **A step's event names the launch its run belongs to**, for the window to find that launch's
    /// pane by it. A run that belongs to none — written before launches were kept — leaves it out.
    #[test]
    fn a_step_event_names_the_launch_of_its_run() {
        let json = |launch: Option<i64>| {
            let run =
                amenbo_core::model::AutomationRun { id: 7, project_id: 1, launch_id: launch, ..Default::default() };
            let dto = AutomationStepOpenDto {
                run: run.id,
                project: run.project_id,
                launch: run.launch_id,
                step: None,
                builtin: None,
                session: None,
                missing: Vec::new(),
            };
            serde_json::to_value(dto).expect("json")
        };
        assert_eq!(json(Some(3)), serde_json::json!({ "run": 7, "project": 1, "launch": 3, "missing": [] }));
        assert_eq!(json(None), serde_json::json!({ "run": 7, "project": 1, "missing": [] }));
    }

    /// **Where a step's agent was chosen goes out as a bare value** (`AMB-D-396`): its kind, and the
    /// role's id where a role was fallen back to — the words are the GUI's to put on it.
    #[test]
    fn where_a_carrier_was_chosen_goes_out_as_its_kind_and_role() {
        use amenbo_core::ops::automation_run::ChosenAt;
        let json = |at: ChosenAt| {
            let carrier =
                Carrier { agent: "claude".into(), model: None, at, agent_by_step: false, model_by_step: false };
            serde_json::to_value(carrier_dto(carrier)).expect("json")
        };
        assert_eq!(
            json(ChosenAt::Step),
            serde_json::json!({
                "agent": "claude",
                "chosenAt": { "kind": "step" },
                "agentByStep": false,
                "modelByStep": false,
            }),
        );
        assert_eq!(
            json(ChosenAt::Role { role_id: 7 }),
            serde_json::json!({
                "agent": "claude",
                "chosenAt": { "kind": "role", "roleId": 7 },
                "agentByStep": false,
                "modelByStep": false,
            }),
        );
        assert_eq!(json(ChosenAt::Amenbo)["chosenAt"], serde_json::json!({ "kind": "amenbo" }));
    }

    /// **A failed round's task goes out with when the round failed** — the end its stretch was closed
    /// with — for the line under the pane to say the time; a stretch with no end leaves it out.
    #[test]
    fn a_failed_rounds_task_goes_out_with_when_it_failed() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("automation-incident-at");
        std::env::set_var("AMENBO_HOME", &tmp);
        let mut store = amenbo_core::Store::open().unwrap();
        let task = store
            .add_task(amenbo_core::ops::task::NewTask {
                title: "Fix the build".into(),
                project_id: None,
                due: None,
                start: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(ActorKind::Ai),
                at_binding_id: None,
                role_id: None,
                made_in_run_step_id: None,
                made_in: None,
            })
            .unwrap();
        let ended = amenbo_core::time::Timestamp::parse_rfc3339("2026-11-08T05:05:00Z").expect("a time");
        let round = amenbo_core::model::AutomationRunTask {
            ended_at: Some(ended),
            incident_task_id: Some(task.id),
            seat: Some(2),
            ..Default::default()
        };

        let incident = incident_of(&store, Some(&round)).unwrap().expect("the task");
        assert_eq!(incident.id, task.id);
        assert_eq!(incident.at.as_deref(), Some(ended.to_rfc3339_z().as_str()));
        assert_eq!(incident.seat, Some(2), "the seat written on the round");

        let unseated = amenbo_core::model::AutomationRunTask { ended_at: None, seat: None, ..round.clone() };
        let json = serde_json::to_value(incident_of(&store, Some(&unseated)).unwrap().expect("the task"))
            .expect("json");
        assert!(json.get("at").is_none(), "a stretch with no end says no time");
        assert!(json.get("seat").is_none(), "nor a seat where none was written");
        let unfiled = amenbo_core::model::AutomationRunTask { incident_task_id: None, ..round };
        assert!(incident_of(&store, Some(&unfiled)).unwrap().is_none(), "a round that filed nothing has none");
        assert!(incident_of(&store, None).unwrap().is_none(), "nor a run with no round");
    }

    /// **A role carries what its own round passes on, and nothing of another's** — each to the role it
    /// is filed under, labelled with the action standing on the spot that files it, marked where the
    /// task depends on another, and with the title of the task a run last passed on from there.
    #[test]
    fn a_role_carries_what_its_own_round_passes_on() {
        use amenbo_core::ops::automation_run::RolePass;
        let role = |id: i64, name: &str| AutomationRole { id, name: name.into(), ..Default::default() };
        let passes = [
            RolePass { from_role_id: 1, to_role_id: 2, placement_id: 10, depends: true, last_task_id: Some(7) },
            RolePass { from_role_id: 1, to_role_id: 3, placement_id: 11, depends: false, last_task_id: None },
            RolePass { from_role_id: 2, to_role_id: 3, placement_id: 20, depends: false, last_task_id: None },
        ];
        let spots = BTreeMap::from([
            (10, "テストのタスクを起票する".to_string()),
            (11, "Report".to_string()),
            (20, "Report".to_string()),
        ]);
        let titles = BTreeMap::from([(7, "テストを足す：検索の絞り込み".to_string())]);
        let json = |id: i64, name: &str| {
            let dto = role_dto(role(id, name), None, &passes, &spots, &titles);
            serde_json::to_value(dto).expect("json")["passes"].clone()
        };
        assert_eq!(
            json(1, "Alice"),
            serde_json::json!([
                {
                    "toRole": 2, "placement": 10, "name": "テストのタスクを起票する", "depends": true,
                    "taskRef": "AMB-T-7", "taskTitle": "テストを足す：検索の絞り込み",
                },
                { "toRole": 3, "placement": 11, "name": "Report", "depends": false },
            ]),
        );
        assert_eq!(json(3, "Carol"), serde_json::json!([]), "a role whose round passes nothing");
    }

    /// **A role goes out with how many it has in the saved version** beside the draft's, and without it
    /// where the version holds no such role.
    #[test]
    fn a_role_goes_out_with_how_many_it_has_in_the_saved_version() {
        let role = AutomationRole { id: 1, name: "Alice".into(), headcount: 3, ..Default::default() };
        let json = |saved: Option<u32>| {
            serde_json::to_value(role_dto(role.clone(), saved, &[], &BTreeMap::new(), &BTreeMap::new())).expect("json")
        };
        let saved = json(Some(2));
        assert_eq!((saved["headcount"].as_u64(), saved["savedHeadcount"].as_u64()), (Some(3), Some(2)));
        assert_eq!(saved["startsBy"], "task", "a role nobody changed is started by a task");
        assert!(json(None).get("savedHeadcount").is_none(), "a role the saved version does not hold");
    }

    /// **A task passed on goes out with the roles at both ends and the task**, and leaves out what has
    /// since gone — a role taken out, a task deleted.
    #[test]
    fn a_task_passed_on_goes_out_with_its_roles_and_its_task() {
        let whole = AutomationRunPassDto {
            from_role: Some(1),
            to_role: Some(2),
            task: Some(7),
            task_ref: Some("AMB-T-7".into()),
            task_title: Some("Add the tests".into()),
            current: true,
        };
        assert_eq!(
            serde_json::to_value(&whole).expect("json"),
            serde_json::json!({
                "fromRole": 1, "toRole": 2, "task": 7, "taskRef": "AMB-T-7", "taskTitle": "Add the tests",
                "current": true,
            }),
        );
        let gone = AutomationRunPassDto {
            from_role: None,
            to_role: Some(2),
            task: None,
            task_ref: None,
            task_title: None,
            current: false,
        };
        assert_eq!(serde_json::to_value(&gone).expect("json"), serde_json::json!({ "toRole": 2 }));
    }

    /// **The four role doors write through core and read back on the detail.** A role is added with
    /// its agent and model or with a headcount, renamed and given another headcount, reordered against
    /// another, has its agent cleared — the model going with it — and is taken away. A model with no
    /// agent, a role that is not there and the last role taken out are core's refusals, passed on rather
    /// than reworded.
    #[test]
    fn roles_are_added_edited_reordered_and_removed() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("automation-roles");
        std::env::set_var("AMENBO_HOME", &tmp);
        let automation_id = {
            let mut store = amenbo_core::Store::open().unwrap();
            let project = store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "Roles".into(),
                    view: amenbo_core::model::View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap();
            store.automation_add(project.id, NewAutomation { name: "Dev".into(), ..Default::default() }).unwrap().id
        };
        let roles = || automation_detail(automation_id).unwrap().expect("the automation").roles;

        automation_role_add(automation_id, "Alice".into(), None, None, None, Some("claude".into()), Some("opus".into()))
            .unwrap();
        automation_role_add(automation_id, "Bob".into(), Some(2), Some("eye".into()), None, None, None).unwrap();
        assert!(
            automation_role_add(automation_id, "Alice".into(), None, None, None, None, Some("opus".into())).is_err(),
            "a model with no agent names nothing"
        );
        let names = |roles: &[AutomationRoleDto]| roles.iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        let listed = roles();
        assert_eq!(names(&listed), ["Devの係", "Alice", "Bob"], "after the one it was born with");
        let (born, alice, bob) = (listed[0].id, listed[1].id, listed[2].id);
        assert_eq!((listed[1].agent.as_deref(), listed[1].model.as_deref()), (Some("claude"), Some("opus")));
        assert_eq!((listed[1].headcount, listed[2].headcount), (1, 2), "one in a role unless given");

        automation_role_move(bob, "before".into(), Some(alice)).unwrap();
        assert_eq!(names(&roles()), ["Devの係", "Bob", "Alice"]);
        assert!(automation_role_move(bob, "before".into(), None).is_err(), "before needs an anchor");
        assert!(automation_role_move(bob, "middle".into(), None).is_err());

        automation_role_edit(bob, Some("Carol".into()), Some(3), None, Some(true), None, None, None, None, None, None).unwrap();
        automation_role_edit(alice, None, None, None, None, None, None, None, Some(true), None, None).unwrap();
        let listed = roles();
        assert_eq!(names(&listed), ["Devの係", "Carol", "Alice"]);
        assert_eq!(listed[1].icon, None, "the clear takes the icon away");
        assert_eq!(listed[1].headcount, 3);
        assert_eq!((listed[2].agent.as_deref(), listed[2].model.as_deref()), (None, None), "the model goes with the agent");
        assert!(
            automation_role_edit(alice, None, None, None, None, None, None, None, None, Some("opus".into()), None).is_err(),
            "a model with no agent is refused on an edit too"
        );

        automation_role_remove(bob).unwrap();
        assert_eq!(names(&roles()), ["Devの係", "Alice"]);
        assert!(automation_role_remove(bob).is_err(), "a role that is gone is not found");
        automation_role_remove(alice).unwrap();
        assert!(automation_role_remove(born).is_err(), "the last role is not taken out");
        assert_eq!(names(&roles()), ["Devの係"]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// **A role that starts nowhere is given the built-in that takes a task as its start**, and the
    /// write names that placement for the picture to mark. A second press on the same role is core's
    /// refusal, passed on, and puts nothing more on the picture.
    #[test]
    fn a_role_that_starts_nowhere_starts_at_the_built_in_that_takes_a_task() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("automation-role-start");
        std::env::set_var("AMENBO_HOME", &tmp);
        let automation_id = {
            let mut store = amenbo_core::Store::open().unwrap();
            let project = store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "Roles".into(),
                    view: amenbo_core::model::View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap();
            store.automation_add(project.id, NewAutomation { name: "Dev".into(), ..Default::default() }).unwrap().id
        };
        automation_role_add(automation_id, "Alice".into(), None, None, None, None, None).unwrap();
        let detail = || automation_detail(automation_id).unwrap().expect("the automation");
        let alice = detail().roles.iter().find(|r| r.name == "Alice").expect("Alice").id;
        assert_eq!(detail().roles.iter().find(|r| r.id == alice).unwrap().entry_placement_id, None);

        let ack = automation_role_start_take_task(alice).unwrap();
        let after = detail();
        let start = after.roles.iter().find(|r| r.id == alice).unwrap().entry_placement_id;
        assert_eq!(ack.placements.len(), 1, "the placement put on comes back");
        assert_eq!(start, Some(ack.placements[0]), "the role starts at it");
        let placed = after.placements.iter().find(|p| p.id == ack.placements[0]).expect("on the picture");
        assert_eq!(placed.builtin.as_deref(), Some("take_task"));

        assert!(automation_role_start_take_task(alice).is_err(), "a role that starts somewhere is refused");
        assert_eq!(detail().placements.len(), after.placements.len(), "and nothing more is put on");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// **A trigger goes out as bare values** (`AMB-D-396`): its `id` and `enabled`, each kind of `when`
    /// and `task` by its `kind`, with its own fields, the script and condition where it has them, and
    /// `nextAt` and `lastFiledAt` as RFC3339 instants. What is absent is left out rather than sent as
    /// `null`.
    #[test]
    fn a_trigger_goes_out_as_its_kinds_and_values() {
        use amenbo_core::model::{AutomationTrigger, StepScript, TriggerCondition, TriggerTask, TriggerWhen};
        let next_at = amenbo_core::time::Timestamp::now();
        let script = StepScript { program: "/bin/check".into(), args: vec!["-q".into()], timeout_minutes: 5 };
        let base = AutomationTrigger {
            id: 1,
            automation_id: 1,
            enabled: true,
            name: "受信箱".into(),
            role_id: None,
            when: TriggerWhen::Every { minutes: 60 },
            condition: None,
            script: None,
            task: TriggerTask::Fixed { title: "見る".into(), notes: "メモ".into() },
            next_at,
            last_filed_at: None,
            order_key: "a0".into(),
            created_at: next_at,
            updated_at: next_at,
        };
        let json = |t: AutomationTrigger| serde_json::to_value(AutomationTriggerDto::from(t)).expect("json");

        assert_eq!(
            json(base.clone()),
            serde_json::json!({
                "id": 1,
                "enabled": true,
                "name": "受信箱",
                "when": { "kind": "every", "minutes": 60 },
                "task": { "kind": "fixed", "title": "見る", "notes": "メモ" },
                "nextAt": next_at.to_rfc3339_z(),
            }),
        );
        let full = json(AutomationTrigger {
            role_id: Some(7),
            when: TriggerWhen::Tasks,
            condition: Some(TriggerCondition { filter: "status:todo".into(), role_id: None, below: 3 }),
            script: Some(script.clone()),
            task: TriggerTask::Script { script },
            ..base.clone()
        });
        assert_eq!(full["roleId"], 7);
        let filed_at = amenbo_core::time::Timestamp::now();
        let filed = json(AutomationTrigger { enabled: false, last_filed_at: Some(filed_at), ..base.clone() });
        assert_eq!(filed["enabled"], false);
        assert_eq!(filed["lastFiledAt"], filed_at.to_rfc3339_z());
        assert_eq!(full["when"], serde_json::json!({ "kind": "tasks" }));
        assert_eq!(full["condition"], serde_json::json!({ "filter": "status:todo", "below": 3 }));
        let program = serde_json::json!({ "program": "/bin/check", "args": ["-q"], "timeoutMinutes": 5 });
        assert_eq!(full["script"], program);
        assert_eq!(full["task"], serde_json::json!({ "kind": "script", "script": program }));
        assert_eq!(json(AutomationTrigger { when: TriggerWhen::Idle, ..base.clone() })["when"], serde_json::json!({ "kind": "idle" }));
        assert_eq!(json(AutomationTrigger { when: TriggerWhen::Script, ..base.clone() })["when"], serde_json::json!({ "kind": "script" }));
        assert_eq!(
            json(AutomationTrigger { task: TriggerTask::File { path: "/tmp/next.md".into() }, ..base })["task"],
            serde_json::json!({ "kind": "file", "path": "/tmp/next.md" }),
        );
    }

    /// **Who a failed round is filed to goes out and comes back in one form**: `{ kind: "human" }` or
    /// `{ kind: "role", roleId }`, read off core's and read back into it unchanged.
    #[test]
    fn who_failures_are_filed_to_reads_and_writes_in_one_form() {
        use amenbo_core::model::FailuresTo;
        for (to, wire) in [
            (FailuresTo::Human, serde_json::json!({ "kind": "human" })),
            (FailuresTo::Role { role_id: 7 }, serde_json::json!({ "kind": "role", "roleId": 7 })),
        ] {
            assert_eq!(serde_json::to_value(AutomationFailuresToDto::from(to)).expect("writes"), wire);
            let back = serde_json::from_value::<AutomationFailuresToDto>(wire).expect("reads");
            assert_eq!(FailuresTo::from(back), to);
        }
    }

    /// **The classification a failed round is filed with is written whole and read back on the detail.**
    /// A required axis left empty is a reason the save would be refused, by the axis's name; choosing a
    /// value on it takes the reason away, and writing none brings it back. Two values on one axis are
    /// core's refusal, passed on, and leave what was written before.
    #[test]
    fn the_classification_of_a_failed_round_is_written_whole_and_read_back() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("automation-failure-values");
        std::env::set_var("AMENBO_HOME", &tmp);
        let (automation_id, sound, picture) = {
            let mut store = amenbo_core::Store::open().unwrap();
            let project = store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "Failure".into(),
                    view: amenbo_core::model::View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap();
            let axis = store
                .dimension_add(
                    project.id,
                    amenbo_core::ops::dimension::NewDimension {
                        name: "テーマ".into(),
                        applies_to: amenbo_core::model::DimensionAppliesTo::Task,
                        ..Default::default()
                    },
                )
                .unwrap()
                .id;
            let sound = store.dimension_value_add(axis, "音", None, None).unwrap().id;
            let picture = store.dimension_value_add(axis, "絵", None, None).unwrap().id;
            // An axis with no values cannot be required, so it turns required once it has some.
            store.dimension_update(axis, None, None, None, None, None, None, Some(true), None, None, None).unwrap();
            let id = store.automation_add(project.id, NewAutomation { name: "Dev".into(), ..Default::default() }).unwrap().id;
            (id, sound, picture)
        };
        let detail = || automation_detail(automation_id).unwrap().expect("the automation");
        let empty_axes = || {
            detail()
                .save_blocks
                .into_iter()
                .filter(|block| block.code == "not_ready_automation_failure_axis_empty")
                .map(|block| block.fields.get("axis").cloned().unwrap_or_default())
                .collect::<Vec<_>>()
        };
        assert!(detail().failure_values.is_empty(), "born with none");
        assert_eq!(empty_axes(), vec!["テーマ".to_string()], "the required axis is a reason");

        automation_edit(automation_id, None, None, None, None, Some(vec![sound])).unwrap();
        assert_eq!(detail().failure_values, vec![sound]);
        assert!(empty_axes().is_empty(), "a value on it takes the reason away");

        assert!(
            automation_edit(automation_id, None, None, None, None, Some(vec![sound, picture])).is_err(),
            "two values on one axis are core's refusal",
        );
        assert_eq!(detail().failure_values, vec![sound], "a refusal writes nothing");

        automation_edit(automation_id, None, None, None, None, Some(vec![])).unwrap();
        assert!(detail().failure_values.is_empty(), "none written");
        assert_eq!(empty_axes(), vec!["テーマ".to_string()]);
    }

    /// **A trigger as a screen writes it reads into core's** ([`amenbo_core::ops::automation::NewTrigger`]):
    /// every kind of `when` and `task`, and what the screen leaves out — the name, the role, the
    /// condition, the script and a script's timeout — read as none.
    #[test]
    fn a_trigger_written_by_a_screen_reads_into_cores() {
        use amenbo_core::model::{TriggerCondition, TriggerWhen};
        use amenbo_core::ops::automation::{NewTrigger, NewTriggerTask};
        let read = |v: serde_json::Value| NewTrigger::from(serde_json::from_value::<AutomationTriggerInputDto>(v).expect("reads"));

        assert_eq!(
            read(serde_json::json!({
                "when": { "kind": "every", "minutes": 30 },
                "task": { "kind": "fixed", "title": "見る", "notes": "" },
            })),
            NewTrigger {
                name: String::new(),
                role_id: None,
                when: TriggerWhen::Every { minutes: 30 },
                condition: None,
                script: None,
                task: NewTriggerTask::Fixed { title: "見る".into(), notes: String::new() },
            },
        );
        let full = read(serde_json::json!({
            "name": "整える",
            "roleId": 4,
            "when": { "kind": "script" },
            "condition": { "filter": "status:todo", "below": 2 },
            "script": { "program": "/bin/ask", "args": ["a b"], "timeoutMinutes": 3 },
            "task": { "kind": "script", "script": { "program": "/bin/write" } },
        }));
        assert_eq!((full.name.as_str(), full.role_id, full.when), ("整える", Some(4), TriggerWhen::Script));
        assert_eq!(full.condition, Some(TriggerCondition { filter: "status:todo".into(), role_id: None, below: 2 }));
        assert_eq!(
            full.script,
            Some(NewScript { program: "/bin/ask".into(), args: vec!["a b".into()], timeout_minutes: Some(3) }),
        );
        assert_eq!(
            full.task,
            NewTriggerTask::Script(NewScript { program: "/bin/write".into(), args: vec![], timeout_minutes: None }),
        );
        let when = |v: serde_json::Value| read(serde_json::json!({ "when": v, "task": { "kind": "file", "path": "/x" } }));
        assert_eq!(when(serde_json::json!({ "kind": "idle" })).when, TriggerWhen::Idle);
        assert_eq!(when(serde_json::json!({ "kind": "tasks" })).task, NewTriggerTask::File { path: "/x".into() });
        assert!(
            serde_json::from_value::<AutomationTriggerInputDto>(serde_json::json!({
                "when": { "kind": "hourly" },
                "task": { "kind": "fixed", "title": "見る", "notes": "" },
            }))
            .is_err(),
            "a kind core does not know is refused"
        );
    }

    /// **The trigger commands write through core and read back on the detail.** Two are added and read
    /// back in the order they were added, each with its `id`, enabled and with no `lastFiledAt`. One is
    /// written again as another kind and keeps its id and place; one is switched off and on; one is taken
    /// away and the other stays. A wait under a minute is core's refusal, passed on, and leaves what was
    /// written before.
    #[test]
    fn triggers_are_added_rewritten_switched_and_taken_away() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("automation-trigger");
        std::env::set_var("AMENBO_HOME", &tmp);
        let automation_id = {
            let mut store = amenbo_core::Store::open().unwrap();
            let project = store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "Trigger".into(),
                    view: amenbo_core::model::View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap();
            store.automation_add(project.id, NewAutomation { name: "Dev".into(), ..Default::default() }).unwrap().id
        };
        let triggers = || automation_detail(automation_id).unwrap().expect("the automation").triggers;
        let input = |v: serde_json::Value| serde_json::from_value::<AutomationTriggerInputDto>(v).expect("reads");
        assert!(triggers().is_empty(), "born with none");

        let typed = |filter: &str| AutomationTriggerConditionDto { filter: filter.into(), role_id: None, below: 3 };
        assert_eq!(automation_trigger_count(automation_id, typed("status:todo")).unwrap(), 0, "no task matches yet");
        assert!(automation_trigger_count(automation_id, typed("project:Other")).is_err(), "a `project:` is refused");
        assert!(triggers().is_empty(), "counting writes nothing");

        let try_typed = |v: serde_json::Value| tauri::async_runtime::block_on(automation_trigger_try(automation_id, input(v), 7));
        assert_eq!(
            try_typed(serde_json::json!({
                "when": { "kind": "every", "minutes": 60 },
                "condition": { "filter": "status:todo", "below": 1 },
                "task": { "kind": "fixed", "title": "受信箱を見る", "notes": "メモ" },
            }))
            .unwrap(),
            AutomationTriggerTriedDto::Files { title: "受信箱を見る".into(), notes: "メモ".into() },
            "no task matches, so fewer than one do",
        );
        assert!(
            try_typed(serde_json::json!({
                "when": { "kind": "every", "minutes": 0 },
                "task": { "kind": "fixed", "title": "受信箱を見る", "notes": "" },
            }))
            .is_err(),
            "a wait under a minute is core's refusal",
        );
        assert!(trigger_tries().is_empty(), "a try over is forgotten");
        automation_trigger_try_stop(7);
        assert!(triggers().is_empty(), "trying writes nothing");

        automation_trigger_add(
            automation_id,
            input(serde_json::json!({
                "name": "受信箱",
                "when": { "kind": "every", "minutes": 60 },
                "task": { "kind": "fixed", "title": "受信箱を見る", "notes": "メモ" },
            })),
        )
        .unwrap();
        automation_trigger_add(
            automation_id,
            input(serde_json::json!({
                "name": "点検",
                "when": { "kind": "idle" },
                "task": { "kind": "fixed", "title": "点検する", "notes": "" },
            })),
        )
        .unwrap();
        let read_back = serde_json::to_value(triggers()).expect("json");
        assert_eq!(read_back.as_array().map(Vec::len), Some(2));
        assert_eq!(read_back[0]["name"], "受信箱");
        assert_eq!(read_back[1]["name"], "点検");
        assert_eq!(read_back[0]["enabled"], true);
        assert!(read_back[0].get("lastFiledAt").is_none(), "none filed yet");
        assert_eq!(read_back[0]["when"], serde_json::json!({ "kind": "every", "minutes": 60 }));
        assert_eq!(read_back[0]["task"], serde_json::json!({ "kind": "fixed", "title": "受信箱を見る", "notes": "メモ" }));
        let next_at = read_back[0]["nextAt"].as_str().expect("an instant");
        assert!(chrono::DateTime::parse_from_rfc3339(next_at).is_ok(), "{next_at} is RFC3339");
        let (inbox, check) = (triggers()[0].id, triggers()[1].id);
        assert_ne!(inbox, check);

        assert!(
            automation_trigger_update(
                inbox,
                input(serde_json::json!({
                    "when": { "kind": "every", "minutes": 0 },
                    "task": { "kind": "fixed", "title": "見る", "notes": "" },
                })),
            )
            .is_err(),
            "a wait under a minute is refused"
        );
        assert!(matches!(triggers()[0].when, AutomationTriggerWhenDto::Every { minutes: 60 }));

        automation_trigger_set_enabled(inbox, false).unwrap();
        assert!(!triggers()[0].enabled, "switched off");
        assert!(triggers()[1].enabled, "the other is left as it was");

        automation_trigger_update(
            inbox,
            input(serde_json::json!({
                "when": { "kind": "tasks" },
                "condition": { "filter": "status:todo", "below": 2 },
                "task": { "kind": "file", "path": std::env::temp_dir().join("next.md") },
            })),
        )
        .unwrap();
        let read_back = triggers();
        assert_eq!((read_back[0].id, read_back[1].id), (inbox, check), "it keeps its id and its place");
        assert!(matches!(read_back[0].when, AutomationTriggerWhenDto::Tasks));
        assert_eq!(read_back[0].condition, Some(AutomationTriggerConditionDto { filter: "status:todo".into(), role_id: None, below: 2 }));
        assert!(!read_back[0].enabled, "rewriting it leaves it switched off");

        automation_trigger_set_enabled(inbox, true).unwrap();
        assert!(triggers()[0].enabled, "switched on again");

        automation_trigger_delete(inbox).unwrap();
        let left: Vec<i64> = triggers().iter().map(|t| t.id).collect();
        assert_eq!(left, [check], "the other stays");
        assert!(automation_trigger_delete(inbox).is_err(), "a trigger that is gone is not found");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
