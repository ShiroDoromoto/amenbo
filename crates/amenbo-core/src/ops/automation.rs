//! Building an automation's definition — the ten tables of the definition side. Of the run side there
//! is one op here, [`run_delete`], and it is a sweep rather than a launch: the definition and what was
//! launched from it go down together when the project does.
//!
//! An automation is a set of steps, the ways out of each of them, and what happens after each way out is
//! taken. It carries no order of its own: the picture is walked from
//! [`crate::model::Automation::entry_step_id`] along the edges, and `order_key` records only the order
//! things were added in, for the lists that have to show them somewhere.
//!
//! **A step reads its ways out, its settings and its inputs from one of two places** — the library
//! action it points at, or itself ([`declarer`]). That is what lets the same action be run at two places
//! in one automation: the two steps share the declarations and each carries its own answers.
//!
//! **Names, not keys, join the parts.** An edge names the way out it hangs on, and a wire names both
//! ends it joins, because an action's ports can be re-declared underneath a step that points at it and a
//! key would then name a row that is gone (`automation_wire`'s note in
//! [`crate::store_engine::schema`]). The cost is that renaming a declaration parts whatever named it,
//! which is the behaviour the specification asks for rather than an oversight.
//!
//! **Nothing here refuses an unfinished automation.** A step with no way onward, an automation with no
//! entry, a required setting nobody answered — each of them saves. What refuses them is the launch
//! check, which is where a person is actually about to be let down by them.
//!
//! **Writes go straight to SQL through the engine.** Every mutator takes the [`WriteTx`]
//! (`BEGIN IMMEDIATE`) its caller opened and does its reads — the `before` snapshot, the new id, the
//! sibling `order_key`, the name it has to find free — inside that same transaction. The subtree
//! deletes ride on one transaction too: apply them half-way and a step's ways out outlive the step.

use crate::error::{Error, Result};
use crate::model::{
    AttachmentTarget, Automation, AutomationAction, AutomationCfg, AutomationCfgKind, AutomationEdge,
    AutomationEnds, AutomationExit, AutomationNote, AutomationOwner, AutomationPort,
    AutomationPortDirection, AutomationPortKind, AutomationPortOwner, AutomationStep,
    AutomationStepNote, AutomationWire, ERROR_EXIT,
};
use crate::ops::{emit_create, emit_update, place, Position};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

// ───────────────────────────── shared refusals and checks ─────────────────────────────

/// `<what> '<id>' not found`. The automation entities carry no conversational ref of their own — they
/// are named by the automation they sit in, not by a number a person types back — so they take the
/// uncoded refusal rather than a [`crate::ops::Noun`]. A code is split off a family only where the GUI
/// puts the refusal in front of a person, and no screen shows these yet.
fn not_found(what: &str, id: i64) -> Error {
    Error::not_found(format!("{what} '{id}' not found"))
}

/// A name that is not blank. Whitespace inside is fine here, unlike a dimension's: nothing filters on
/// these, and a step whose name is a sentence in the language the store is written in is the ordinary
/// case.
fn checked_name(what: &str, name: &str) -> Result<String> {
    let s = name.trim();
    if s.is_empty() {
        return Err(Error::invalid(format!("a {what} name cannot be empty")));
    }
    Ok(s.to_string())
}

/// The name of a way out, as a person may give it. [`ERROR_EXIT`] is refused: every owner is born
/// carrying that one, and a second row under the same name would leave an edge naming either of them.
fn checked_exit_name(name: &str) -> Result<String> {
    let s = checked_name("way out", name)?;
    if s == ERROR_EXIT {
        return Err(Error::invalid(format!(
            "'{ERROR_EXIT}' is the error way out's own name — every step and every action carries it \
             already, so it cannot be given to another"
        )));
    }
    Ok(s)
}

fn live_automation(tx: &WriteTx<'_>, id: i64) -> Result<Automation> {
    read::automation(tx.conn(), id)?.ok_or_else(|| not_found("automation", id))
}

fn live_action(tx: &WriteTx<'_>, id: i64) -> Result<AutomationAction> {
    read::automation_action(tx.conn(), id)?.ok_or_else(|| not_found("action", id))
}

fn live_step(tx: &WriteTx<'_>, id: i64) -> Result<AutomationStep> {
    read::automation_step(tx.conn(), id)?.ok_or_else(|| not_found("step", id))
}

fn live_note(tx: &WriteTx<'_>, id: i64) -> Result<AutomationNote> {
    read::automation_note(tx.conn(), id)?.ok_or_else(|| not_found("shared document", id))
}

fn live_exit(tx: &WriteTx<'_>, id: i64) -> Result<AutomationExit> {
    read::automation_exit(tx.conn(), id)?.ok_or_else(|| not_found("way out", id))
}

fn live_port(tx: &WriteTx<'_>, id: i64) -> Result<AutomationPort> {
    read::automation_port(tx.conn(), id)?.ok_or_else(|| not_found("port", id))
}

fn live_cfg(tx: &WriteTx<'_>, id: i64) -> Result<AutomationCfg> {
    read::automation_cfg(tx.conn(), id)?.ok_or_else(|| not_found("setting", id))
}

fn live_edge(tx: &WriteTx<'_>, id: i64) -> Result<AutomationEdge> {
    read::automation_edge(tx.conn(), id)?.ok_or_else(|| not_found("edge", id))
}

fn live_wire(tx: &WriteTx<'_>, id: i64) -> Result<AutomationWire> {
    read::automation_wire(tx.conn(), id)?.ok_or_else(|| not_found("wire", id))
}

/// **Where a step's declarations are read from**: the library action it points at, or the step itself.
/// One answer, asked by everything that resolves a name on a step — an edge's way out, a wire's ends, a
/// setting's declaration.
pub fn declarer(step: &AutomationStep) -> (AutomationOwner, i64) {
    match step.action_id {
        Some(action_id) => (AutomationOwner::Action, action_id),
        None => (AutomationOwner::Step, step.id),
    }
}

/// The same pair as a port's owner, which admits a third kind this one does not.
pub(crate) fn port_declarer(step: &AutomationStep) -> (AutomationPortOwner, i64) {
    match step.action_id {
        Some(action_id) => (AutomationPortOwner::Action, action_id),
        None => (AutomationPortOwner::Step, step.id),
    }
}

/// The two ways out every step and every action is born with: the unnamed one, which is all a step with
/// a single way out needs, and the error one, which nobody can delete. Written at the moment the owner
/// is created so that an edge or a port has somewhere to hang from the first command onwards.
fn born_with_exits(tx: &WriteTx<'_>, owner_kind: AutomationOwner, owner_id: i64) -> Result<()> {
    add_exit_row(tx, owner_kind, owner_id, None)?;
    add_exit_row(tx, owner_kind, owner_id, Some(ERROR_EXIT.to_string()))?;
    Ok(())
}

/// Write one `automation_exit` row at the bottom of its owner's list. The name is taken as given — the
/// checking is the caller's, which is what lets [`born_with_exits`] write the one name a caller may not.
fn add_exit_row(
    tx: &WriteTx<'_>,
    owner_kind: AutomationOwner,
    owner_id: i64,
    name: Option<String>,
) -> Result<AutomationExit> {
    let sibs = read::automation_exit_siblings(tx.conn(), owner_kind, owner_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_exit")?;
    let exit = AutomationExit { id, owner_kind, owner_id, name, order_key, created_at: now, updated_at: now };
    emit_create(tx, record::automation_exit(&exit))?;
    Ok(exit)
}

/// Delete one way out and the outputs declared on it. An edge or a wire naming it is **not** swept: they
/// name it by the name they were written with, and the specification keeps that parting visible rather
/// than silently rewriting the picture around it.
fn delete_exit_row(tx: &WriteTx<'_>, exit_id: i64) -> Result<()> {
    for port in read::automation_port_ids(tx.conn(), AutomationPortOwner::Exit, exit_id)? {
        tx.delete_record("automation_port", port)?;
    }
    tx.delete_record("automation_exit", exit_id)?;
    Ok(())
}

/// Delete every way out, port and setting one declarer carries — what goes when a step or an action
/// does.
fn delete_declarations(
    tx: &WriteTx<'_>,
    owner: AutomationOwner,
    port_owner: AutomationPortOwner,
    owner_id: i64,
) -> Result<()> {
    for exit in read::automation_exit_ids(tx.conn(), owner, owner_id)? {
        delete_exit_row(tx, exit)?;
    }
    for port in read::automation_port_ids(tx.conn(), port_owner, owner_id)? {
        tx.delete_record("automation_port", port)?;
    }
    for cfg in read::automation_cfg_ids(tx.conn(), owner, owner_id)? {
        tx.delete_record("automation_cfg", cfg)?;
    }
    Ok(())
}

// ───────────────────────────── the library (automation_action) ─────────────────────────────

/// Add a prompt to the library. `project_id` `None` puts it in the device's own, where every project on
/// this machine reaches it; `Some` puts it in one project's.
///
/// It is born carrying the two ways out every declarer has ([`born_with_exits`]). It names no agent and
/// no model — who is asked to carry the prompt out is the step's answer, so two automations can run the
/// same action with different agents.
pub fn action_add(
    tx: &WriteTx<'_>,
    project_id: Option<i64>,
    name: &str,
    prompt: &str,
) -> Result<AutomationAction> {
    let name = checked_name("action", name)?;
    if let Some(project_id) = project_id {
        if read::project(tx.conn(), project_id)?.is_none() {
            return Err(crate::ops::project::NOUN.not_found(project_id.to_string()));
        }
    }
    let sibs = read::automation_action_siblings(tx.conn(), project_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_action")?;
    let action = AutomationAction {
        id,
        project_id,
        name,
        prompt: prompt.to_string(),
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_action(&action))?;
    born_with_exits(tx, AutomationOwner::Action, id)?;
    Ok(action)
}

/// Rename a library action, or rewrite its prompt. Only the `Some` fields are written.
///
/// **Renaming the action is safe; renaming what it declares is not.** A step points at the action by
/// key (`automation_step.action_id`), so nothing parts here — while renaming one of its ways out or its
/// ports parts every edge and wire that named the old one ([`exit_rename`], [`port_update`]).
pub fn action_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    prompt: Option<&str>,
) -> Result<AutomationAction> {
    let before = live_action(tx, id)?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = checked_name("action", name)?;
    }
    if let Some(prompt) = prompt {
        after.prompt = prompt.to_string();
    }
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_action(&before), record::automation_action(&after))?;
    Ok(after)
}

/// **Raise a step's own prompt into the library**: make an action of it, move the step's declarations
/// onto that action, and point the step at it. One act, because half of it is an automation that has
/// lost its wiring.
///
/// **The declarations move; they are not copied.** A step that runs an action declares nothing of its
/// own ([`declarer`]), so leaving the step's rows where they are would put the same ways out in two
/// places with nothing keeping them in step. Edges and wires name a way out by its **name**, not by its
/// id (the specification's, so that one action used twice in an automation is still unambiguous), so
/// the picture around the step goes on reading once the action declares the same names.
///
/// **A setting's answer stays on the step.** What moves is the declaration — the name, the kind,
/// whether it is required, the choices — and the answer somebody gave is the step's own business, which
/// is exactly the pair [`cfg_set`] keeps for a step that already runs an action. An answer dropped here
/// would be a launch check that starts refusing a definition nobody edited.
///
/// `project_id` is which library it lands in: `None` the device's, where every project on this machine
/// reaches it, and `Some` one project's. The device's is the wider reach, and a step in any project can
/// point at it.
///
/// Refused for a step that already runs an action — there is nothing of its own left to raise.
pub fn action_from_step(
    tx: &WriteTx<'_>,
    step_id: i64,
    project_id: Option<i64>,
    name: &str,
) -> Result<AutomationAction> {
    let before = live_step(tx, step_id)?;
    if before.action_id.is_some() {
        return Err(Error::invalid(format!(
            "step '{}' already runs a library action — what it carries is the action's, and there is \
             nothing of its own to raise",
            before.name
        )));
    }
    let automation = live_automation(tx, before.automation_id)?;
    // The library it lands in has to be one this step can point at afterwards, which is the same reach
    // the pull-down offers: this project's, or the device's.
    if let Some(project_id) = project_id {
        if project_id != automation.project_id {
            return Err(Error::invalid(
                "a step reaches its own project's library and the device's, and no other".to_string(),
            ));
        }
    }
    let action = action_add(tx, project_id, name, before.prompt.as_deref().unwrap_or_default())?;

    // The two born ways out are already on the action, so only the named ones are added — and each
    // one's outputs go onto whichever row of the action carries that name.
    for exit in read::automation_exits_of(tx.conn(), AutomationOwner::Step, step_id)? {
        let onto = match read::automation_exit_by_name(
            tx.conn(),
            AutomationOwner::Action,
            action.id,
            exit.name.as_deref(),
        )? {
            Some(already) => already,
            None => add_exit_row(tx, AutomationOwner::Action, action.id, exit.name.clone())?,
        };
        for port in read::automation_ports_of(
            tx.conn(),
            AutomationPortOwner::Exit,
            exit.id,
            AutomationPortDirection::Out,
        )? {
            port_add(
                tx,
                AutomationPortOwner::Exit,
                onto.id,
                AutomationPortDirection::Out,
                &port.name,
                port.kind,
                port.required,
            )?;
        }
    }
    for port in read::automation_ports_of(
        tx.conn(),
        AutomationPortOwner::Step,
        step_id,
        AutomationPortDirection::In,
    )? {
        port_add(
            tx,
            AutomationPortOwner::Action,
            action.id,
            AutomationPortDirection::In,
            &port.name,
            port.kind,
            port.required,
        )?;
    }
    // Read before the step's rows go, because each one is both the declaration and the answer until it
    // is split in two.
    let answered = read::automation_cfgs_of(tx.conn(), AutomationOwner::Step, step_id)?;
    for cfg in &answered {
        cfg_add(
            tx,
            AutomationOwner::Action,
            action.id,
            &cfg.name,
            cfg.kind,
            cfg.required,
            cfg.options.as_deref(),
        )?;
    }

    let mut after = before.clone();
    after.action_id = Some(action.id);
    after.prompt = None;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_step(&before), record::automation_step(&after))?;
    delete_declarations(tx, AutomationOwner::Step, AutomationPortOwner::Step, step_id)?;

    // And the answers back, on rows that now carry nothing else.
    for cfg in answered {
        if cfg.value.is_some() {
            write_cfg_row(
                tx,
                AutomationOwner::Step,
                step_id,
                cfg.name,
                cfg.kind,
                cfg.required,
                cfg.options,
                cfg.value,
            )?;
        }
    }
    Ok(action)
}

/// Reorder a library action within its own library.
pub fn action_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationAction> {
    let before = live_action(tx, id)?;
    let sibs = read::automation_action_siblings(tx.conn(), before.project_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_action(&before), record::automation_action(&after))?;
    Ok(after)
}

/// Delete a library action, with the ways out, ports and settings it declared.
///
/// **Refused while a step runs it**, naming how many do: the step would be left pointing at a prompt
/// that is gone, and which prompt it should carry instead is not this op's to guess.
pub fn action_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    live_action(tx, id)?;
    let users = read::automation_step_ids_using_action(tx.conn(), id)?;
    if !users.is_empty() {
        return Err(Error::invalid(format!(
            "{} step(s) run this action — point them somewhere else before deleting it",
            users.len()
        )));
    }
    delete_declarations(tx, AutomationOwner::Action, AutomationPortOwner::Action, id)?;
    tx.delete_record("automation_action", id)?;
    Ok(())
}

// ───────────────────────────── the automation itself ─────────────────────────────

/// What a new automation is made of. `preamble` is prepended to every step's launch, so it is kept
/// short — the material a prompt would otherwise repeat belongs in a shared document ([`note_add`]).
#[derive(Clone, Debug, Default)]
pub struct NewAutomation {
    pub name: String,
    pub notes: String,
    pub preamble: String,
}

/// Create an automation. It is born with no steps and no entry; what refuses to launch it is the launch
/// check, not this.
pub fn add(tx: &WriteTx<'_>, project_id: i64, new: NewAutomation) -> Result<Automation> {
    let name = checked_name("automation", &new.name)?;
    if read::project(tx.conn(), project_id)?.is_none() {
        return Err(crate::ops::project::NOUN.not_found(project_id.to_string()));
    }
    let sibs = read::automation_siblings(tx.conn(), project_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation")?;
    let automation = Automation {
        id,
        project_id,
        name,
        notes: new.notes,
        preamble: new.preamble,
        entry_step_id: None,
        archived: false,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation(&automation))?;
    Ok(automation)
}

/// Change an automation's name, notes, preamble, or whether it is archived. Only the `Some` fields are
/// written. Archiving takes nothing away and stops nothing already running: it is what keeps an
/// automation nobody launches any more out of the lists.
pub fn update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    notes: Option<&str>,
    preamble: Option<&str>,
    archived: Option<bool>,
) -> Result<Automation> {
    let before = live_automation(tx, id)?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = checked_name("automation", name)?;
    }
    if let Some(notes) = notes {
        after.notes = notes.to_string();
    }
    if let Some(preamble) = preamble {
        after.preamble = preamble.to_string();
    }
    if let Some(archived) = archived {
        after.archived = archived;
    }
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation(&before), record::automation(&after))?;
    Ok(after)
}

/// Reorder an automation within its project.
pub fn move_to(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<Automation> {
    let before = live_automation(tx, id)?;
    let sibs = read::automation_siblings(tx.conn(), before.project_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation(&before), record::automation(&after))?;
    Ok(after)
}

/// Name the step a run opens its first terminal on, or clear it with `None`.
///
/// The step has to be one of this automation's. Whether it is a step that takes a task — the thing that
/// actually makes it a usable entry — is the launch check's to ask: an automation is built in whatever
/// order its author likes, and refusing the entry until the port exists would make the order the tool's
/// to choose.
pub fn set_entry(tx: &WriteTx<'_>, automation_id: i64, step_id: Option<i64>) -> Result<Automation> {
    let before = live_automation(tx, automation_id)?;
    if let Some(step_id) = step_id {
        let step = live_step(tx, step_id)?;
        if step.automation_id != automation_id {
            return Err(Error::invalid(format!(
                "step '{step_id}' belongs to another automation, so it cannot be this one's entry"
            )));
        }
    }
    let mut after = before.clone();
    after.entry_step_id = step_id;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation(&before), record::automation(&after))?;
    Ok(after)
}

/// Delete an automation and everything built into it — wires, edges, document links, steps with their
/// declarations, and the shared documents themselves.
///
/// **Refused while a run stands behind it**, naming how many. A run carries its own copy of the steps
/// and would go on reading correctly, but it is filed under the automation it was launched from, and
/// deleting that leaves the record unable to say what was run.
pub fn delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let automation = live_automation(tx, id)?;
    let runs = read::automation_run_ids(tx.conn(), id)?;
    if !runs.is_empty() {
        return Err(Error::invalid(format!(
            "{} run(s) were launched from this automation — it is what they are filed under, so it \
             cannot be deleted",
            runs.len()
        )));
    }
    for wire in read::automation_wire_ids(tx.conn(), id)? {
        tx.delete_record("automation_wire", wire)?;
    }
    for edge in read::automation_edge_ids(tx.conn(), id)? {
        tx.delete_record("automation_edge", edge)?;
    }
    // The entry is a reference into the steps that are about to go, so it is dropped before them.
    if automation.entry_step_id.is_some() {
        set_entry(tx, id, None)?;
    }
    for step in read::automation_step_ids(tx.conn(), id)? {
        delete_step_row(tx, step)?;
    }
    for note in read::automation_note_ids(tx.conn(), id)? {
        tx.delete_record("automation_note", note)?;
    }
    tx.delete_record("automation", id)?;
    Ok(())
}

/// Delete one run and everything filed under it — the values each step execution carried, the
/// executions themselves with whatever was attached to them, the tasks the run worked on, and the step
/// snapshots it took at launch. Returns the blob hashes those attachments pointed at, for the caller to
/// reclaim once the transaction commits.
///
/// **Nothing refuses this, and nothing calls it but the project delete.** A run is the record of what
/// happened, so there is no reason to reach for it while the project it is filed under is still there —
/// and no reason to keep it once that project is gone.
///
/// The order is what the `RESTRICT` clauses insist on: a value names the execution that produced it as
/// well as the one that received it (`from_run_step_id`), so every value of the run goes before any
/// execution does; and an execution names the task row and the snapshot it was run from, so those go
/// after it.
pub(crate) fn run_delete(tx: &WriteTx<'_>, id: i64) -> Result<Vec<String>> {
    let steps = read::automation_run_step_ids(tx.conn(), id)?;
    for step in &steps {
        for value in read::automation_run_value_ids(tx.conn(), *step)? {
            tx.delete_record("automation_run_value", value)?;
        }
    }
    let mut orphaned = Vec::new();
    for step in steps {
        orphaned.extend(crate::ops::sweep_polymorphic(tx, AttachmentTarget::AutomationRunStep, step)?);
        tx.delete_record("automation_run_step", step)?;
    }
    for run_task in read::automation_run_task_ids(tx.conn(), id)? {
        tx.delete_record("automation_run_task", run_task)?;
    }
    for def in read::automation_run_def_ids(tx.conn(), id)? {
        tx.delete_record("automation_run_def", def)?;
    }
    tx.delete_record("automation_run", id)?;
    Ok(orphaned)
}

// ───────────────────────────── shared documents ─────────────────────────────

/// Write a shared document for one automation. Long is fine here: this is where the material a prompt
/// would otherwise repeat is written once, and [`note_link`] says which steps are handed it.
pub fn note_add(tx: &WriteTx<'_>, automation_id: i64, name: &str, body: &str) -> Result<AutomationNote> {
    live_automation(tx, automation_id)?;
    let name = checked_name("shared document", name)?;
    let sibs = read::automation_note_siblings(tx.conn(), automation_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_note")?;
    let note = AutomationNote {
        id,
        automation_id,
        name,
        body: body.to_string(),
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_note(&note))?;
    Ok(note)
}

/// Rename a shared document, or rewrite it. Only the `Some` fields are written.
pub fn note_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    body: Option<&str>,
) -> Result<AutomationNote> {
    let before = live_note(tx, id)?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = checked_name("shared document", name)?;
    }
    if let Some(body) = body {
        after.body = body.to_string();
    }
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_note(&before), record::automation_note(&after))?;
    Ok(after)
}

/// Reorder a shared document within its automation.
pub fn note_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationNote> {
    let before = live_note(tx, id)?;
    let sibs = read::automation_note_siblings(tx.conn(), before.automation_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_note(&before), record::automation_note(&after))?;
    Ok(after)
}

/// Delete a shared document, taking the links that hand it to steps.
pub fn note_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    live_note(tx, id)?;
    for link in read::automation_step_note_ids_of_note(tx.conn(), id)? {
        tx.delete_record("automation_step_note", link)?;
    }
    tx.delete_record("automation_note", id)?;
    Ok(())
}

/// Hand a shared document to a step. Both ends have to sit in the same automation, and handing the same
/// document to the same step twice answers the link already there rather than writing a second one.
pub fn note_link(tx: &WriteTx<'_>, step_id: i64, note_id: i64) -> Result<AutomationStepNote> {
    let step = live_step(tx, step_id)?;
    let note = live_note(tx, note_id)?;
    if step.automation_id != note.automation_id {
        return Err(Error::invalid(
            "a step is handed the shared documents of its own automation, not another's",
        ));
    }
    for id in read::automation_step_note_ids(tx.conn(), step_id)? {
        let link = read::automation_step_note(tx.conn(), id)?;
        if let Some(link) = link {
            if link.note_id == note_id {
                return Ok(link);
            }
        }
    }
    let sibs = read::automation_step_note_siblings(tx.conn(), step_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_step_note")?;
    let link = AutomationStepNote { id, step_id, note_id, order_key, created_at: now, updated_at: now };
    emit_create(tx, record::automation_step_note(&link))?;
    Ok(link)
}

/// Stop handing a shared document to a step. Answers whether there was a link to take.
pub fn note_unlink(tx: &WriteTx<'_>, step_id: i64, note_id: i64) -> Result<bool> {
    for id in read::automation_step_note_ids(tx.conn(), step_id)? {
        if let Some(link) = read::automation_step_note(tx.conn(), id)? {
            if link.note_id == note_id {
                tx.delete_record("automation_step_note", id)?;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

// ───────────────────────────── steps ─────────────────────────────

/// Where a step's prompt comes from. The two are exclusive, which is what keeps a step from carrying a
/// prompt nobody reads beside an action it actually runs.
#[derive(Clone, Debug)]
pub enum StepSource {
    /// Run a library action. The step's ways out, settings and inputs are then the action's.
    Action(i64),
    /// Carry a prompt of this step's own.
    Prompt(String),
}

/// What a new step is made of. `show_history` starts on — a step is handed the run's story so far unless
/// somebody says otherwise — while `interactive` and `report_to_task` start off.
#[derive(Clone, Debug)]
pub struct NewStep {
    pub name: String,
    pub source: StepSource,
    pub agent: String,
    pub model: Option<String>,
    pub interactive: bool,
    /// The name of the setting or the input the working folder is taken from — a name, not a path.
    pub work_dir_ref: Option<String>,
    pub report_to_task: bool,
    pub show_history: bool,
}

impl NewStep {
    /// A step carrying its own prompt, with the three flags where they start.
    pub fn with_prompt(name: &str, prompt: &str, agent: &str) -> NewStep {
        NewStep {
            name: name.to_string(),
            source: StepSource::Prompt(prompt.to_string()),
            agent: agent.to_string(),
            model: None,
            interactive: false,
            work_dir_ref: None,
            report_to_task: false,
            show_history: true,
        }
    }

    /// A step running a library action, with the three flags where they start.
    pub fn with_action(name: &str, action_id: i64, agent: &str) -> NewStep {
        NewStep {
            name: name.to_string(),
            source: StepSource::Action(action_id),
            agent: agent.to_string(),
            model: None,
            interactive: false,
            work_dir_ref: None,
            report_to_task: false,
            show_history: true,
        }
    }
}

/// The library action a step may point at has to be within reach of the automation's project: its own
/// project's library, or the device's. Another project's is refused — the prompt would be read across a
/// boundary that is there to keep one project's context out of another's.
fn checked_action(tx: &WriteTx<'_>, automation: &Automation, action_id: i64) -> Result<()> {
    let action = live_action(tx, action_id)?;
    match action.project_id {
        None => Ok(()),
        Some(p) if p == automation.project_id => Ok(()),
        Some(_) => Err(Error::invalid(format!(
            "action '{action_id}' is in another project's library — a step reaches its own project's \
             library and the device's, and no other"
        ))),
    }
}

/// Add a step to an automation. A step carrying its own prompt is born with the two ways out every
/// declarer has; one pointing at an action reads the action's instead, so it declares none of its own.
pub fn step_add(tx: &WriteTx<'_>, automation_id: i64, new: NewStep) -> Result<AutomationStep> {
    let automation = live_automation(tx, automation_id)?;
    let name = checked_name("step", &new.name)?;
    let agent = checked_name("agent", &new.agent)?;
    let (action_id, prompt) = match &new.source {
        StepSource::Action(action_id) => {
            checked_action(tx, &automation, *action_id)?;
            (Some(*action_id), None)
        }
        StepSource::Prompt(prompt) => (None, Some(prompt.clone())),
    };
    let sibs = read::automation_step_siblings(tx.conn(), automation_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_step")?;
    let step = AutomationStep {
        id,
        automation_id,
        name,
        action_id,
        prompt,
        agent,
        model: new.model,
        interactive: new.interactive,
        work_dir_ref: new.work_dir_ref,
        report_to_task: new.report_to_task,
        show_history: new.show_history,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_step(&step))?;
    if step.action_id.is_none() {
        born_with_exits(tx, AutomationOwner::Step, id)?;
    }
    Ok(step)
}

/// Change a step. Only the `Some` fields are written, and `source` is the one that moves more than a
/// column.
///
/// **Switching where the prompt comes from takes the declarations with it.** A step that starts
/// carrying its own prompt and is pointed at an action loses the ways out, ports and settings it
/// declared — the action's are what it reads from then on, and leaving its own behind would leave rows
/// nothing can reach. A step going the other way is born with the two ways out, exactly as a new one is.
/// What is **not** carried over either way is the wiring: an edge or a wire naming a way out that is no
/// longer declared stops resolving, which is the same parting a rename causes and is visible in the
/// picture.
#[allow(clippy::too_many_arguments)]
pub fn step_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    source: Option<StepSource>,
    agent: Option<&str>,
    model: Option<Option<&str>>,
    interactive: Option<bool>,
    work_dir_ref: Option<Option<&str>>,
    report_to_task: Option<bool>,
    show_history: Option<bool>,
) -> Result<AutomationStep> {
    let before = live_step(tx, id)?;
    let automation = live_automation(tx, before.automation_id)?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = checked_name("step", name)?;
    }
    if let Some(agent) = agent {
        after.agent = checked_name("agent", agent)?;
    }
    if let Some(model) = model {
        after.model = model.map(str::to_string);
    }
    if let Some(interactive) = interactive {
        after.interactive = interactive;
    }
    if let Some(work_dir_ref) = work_dir_ref {
        after.work_dir_ref = work_dir_ref.map(str::to_string);
    }
    if let Some(report_to_task) = report_to_task {
        after.report_to_task = report_to_task;
    }
    if let Some(show_history) = show_history {
        after.show_history = show_history;
    }
    let mut drop_own_declarations = false;
    let mut raise_own_exits = false;
    if let Some(source) = source {
        match source {
            StepSource::Action(action_id) => {
                checked_action(tx, &automation, action_id)?;
                drop_own_declarations = before.action_id.is_none();
                after.action_id = Some(action_id);
                after.prompt = None;
            }
            StepSource::Prompt(prompt) => {
                raise_own_exits = before.action_id.is_some();
                after.action_id = None;
                after.prompt = Some(prompt);
            }
        }
    }
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_step(&before), record::automation_step(&after))?;
    if drop_own_declarations {
        delete_declarations(tx, AutomationOwner::Step, AutomationPortOwner::Step, id)?;
    }
    if raise_own_exits {
        born_with_exits(tx, AutomationOwner::Step, id)?;
    }
    Ok(after)
}

/// Reorder a step within its automation. It moves the step in the lists alone — the picture is walked
/// from the entry along the edges, and no order here reaches it.
pub fn step_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationStep> {
    let before = live_step(tx, id)?;
    let sibs = read::automation_step_siblings(tx.conn(), before.automation_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_step(&before), record::automation_step(&after))?;
    Ok(after)
}

/// Delete a step, with its own declarations, its document links, and every edge and wire naming it at
/// either end.
///
/// **Deleting the entry clears it.** An automation under construction has to be able to lose any step,
/// and refusing here would strand whichever one was named the entry first; an automation left without
/// one is refused at the launch check, where a person is about to be let down by it.
pub fn step_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let step = live_step(tx, id)?;
    let automation = live_automation(tx, step.automation_id)?;
    // `entry_step_id` is `RESTRICT`, and that check bites at the statement rather than at the commit —
    // so the reference is dropped before the row it names, not after.
    if automation.entry_step_id == Some(id) {
        set_entry(tx, automation.id, None)?;
    }
    delete_step_row(tx, id)
}

/// One step and everything hanging off it, with no word about the entry — what [`delete`] walks, where
/// the entry has already been dropped and the automation itself is going anyway.
fn delete_step_row(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    for wire in read::automation_wire_ids_naming_step(tx.conn(), id)? {
        tx.delete_record("automation_wire", wire)?;
    }
    for edge in read::automation_edge_ids_naming_step(tx.conn(), id)? {
        tx.delete_record("automation_edge", edge)?;
    }
    for link in read::automation_step_note_ids(tx.conn(), id)? {
        tx.delete_record("automation_step_note", link)?;
    }
    delete_declarations(tx, AutomationOwner::Step, AutomationPortOwner::Step, id)?;
    tx.delete_record("automation_step", id)?;
    Ok(())
}

// ───────────────────────────── ways out, ports, settings ─────────────────────────────

/// The owner a declaration may be written on. A step that runs a library action declares nothing of its
/// own — it reads the action's — so writing one on it is refused rather than left to sit unread.
fn checked_declarer(tx: &WriteTx<'_>, owner_kind: AutomationOwner, owner_id: i64) -> Result<()> {
    match owner_kind {
        AutomationOwner::Action => {
            live_action(tx, owner_id)?;
            Ok(())
        }
        AutomationOwner::Step => {
            let step = live_step(tx, owner_id)?;
            match step.action_id {
                None => Ok(()),
                Some(action_id) => Err(Error::invalid(format!(
                    "step '{owner_id}' runs action '{action_id}', so what it declares is the action's \
                     — write it there, or give the step a prompt of its own first"
                ))),
            }
        }
    }
}

/// Add a way out to a step or a library action. [`ERROR_EXIT`] is refused as a name, and so is one
/// already taken on the same owner: an edge names a way out by name, and two rows under one name would
/// leave it naming either.
pub fn exit_add(
    tx: &WriteTx<'_>,
    owner_kind: AutomationOwner,
    owner_id: i64,
    name: Option<&str>,
) -> Result<AutomationExit> {
    checked_declarer(tx, owner_kind, owner_id)?;
    let name = name.map(checked_exit_name).transpose()?;
    if read::automation_exit_by_name(tx.conn(), owner_kind, owner_id, name.as_deref())?.is_some() {
        return Err(Error::invalid(match &name {
            Some(n) => format!("a way out called '{n}' is already declared here"),
            None => "the unnamed way out is already declared here".to_string(),
        }));
    }
    add_exit_row(tx, owner_kind, owner_id, name)
}

/// Rename a way out.
///
/// **Whatever named the old name is parted from it.** Edges and wires name a way out by its name, so
/// renaming one leaves them pointing at a name nobody declares — visibly, in the picture, which is what
/// the specification asks for rather than a silent rewrite of the graph around it.
///
/// [`ERROR_EXIT`]'s row is refused at both ends: it may not be renamed, and no other may take its name.
pub fn exit_rename(tx: &WriteTx<'_>, id: i64, name: Option<&str>) -> Result<AutomationExit> {
    let before = live_exit(tx, id)?;
    if before.name.as_deref() == Some(ERROR_EXIT) {
        return Err(Error::invalid(
            "the error way out's name is fixed — every step and every action is read as carrying it",
        ));
    }
    let name = name.map(checked_exit_name).transpose()?;
    if let Some(holder) =
        read::automation_exit_by_name(tx.conn(), before.owner_kind, before.owner_id, name.as_deref())?
    {
        if holder.id != id {
            return Err(Error::invalid(match &name {
                Some(n) => format!("a way out called '{n}' is already declared here"),
                None => "the unnamed way out is already declared here".to_string(),
            }));
        }
    }
    let mut after = before.clone();
    after.name = name;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_exit(&before), record::automation_exit(&after))?;
    Ok(after)
}

/// Reorder a way out within its owner's list.
pub fn exit_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationExit> {
    let before = live_exit(tx, id)?;
    let sibs =
        read::automation_exit_siblings(tx.conn(), before.owner_kind, before.owner_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_exit(&before), record::automation_exit(&after))?;
    Ok(after)
}

/// Delete a way out, with the outputs declared on it. [`ERROR_EXIT`]'s row is refused — every step
/// carries it, and an edge may name it whether or not anyone wrote one.
pub fn exit_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let exit = live_exit(tx, id)?;
    if exit.name.as_deref() == Some(ERROR_EXIT) {
        return Err(Error::invalid(
            "the error way out cannot be deleted — every step and every action carries one",
        ));
    }
    delete_exit_row(tx, id)
}

/// Declare a port: what a step takes in (`In`, on a step or an action) or what a way out hands on
/// (`Out`, on that way out).
///
/// The two directions hang on different owners, and neither is sayable on the other's: an input belongs
/// to the step that reads it, while an output belongs to the way out that produced it, which is what
/// lets a review step hand on a file only when it left through "something to fix".
pub fn port_add(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPortOwner,
    owner_id: i64,
    direction: AutomationPortDirection,
    name: &str,
    kind: AutomationPortKind,
    required: bool,
) -> Result<AutomationPort> {
    let name = checked_name("port", name)?;
    match (direction, owner_kind.declarer()) {
        (AutomationPortDirection::In, Some(declarer)) => checked_declarer(tx, declarer, owner_id)?,
        (AutomationPortDirection::In, None) => {
            return Err(Error::invalid(
                "an input belongs to the step or the action that reads it, not to a way out",
            ))
        }
        (AutomationPortDirection::Out, None) => {
            live_exit(tx, owner_id)?;
        }
        (AutomationPortDirection::Out, Some(_)) => {
            return Err(Error::invalid(
                "an output belongs to the way out that produced it, not to the step itself",
            ))
        }
    }
    if read::automation_port_by_name(tx.conn(), owner_kind, owner_id, direction, &name)?.is_some() {
        return Err(Error::invalid(format!(
            "a {} called '{name}' is already declared here",
            match direction {
                AutomationPortDirection::In => "input",
                AutomationPortDirection::Out => "output",
            }
        )));
    }
    let sibs = read::automation_port_siblings(tx.conn(), owner_kind, owner_id, direction, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_port")?;
    let port = AutomationPort {
        id,
        owner_kind,
        owner_id,
        direction,
        name,
        kind,
        required,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_port(&port))?;
    Ok(port)
}

/// Change a port's name, what it carries, or whether it is required. Only the `Some` fields are written.
///
/// Renaming parts every wire that named the old name, for the reason [`exit_rename`] gives.
pub fn port_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    kind: Option<AutomationPortKind>,
    required: Option<bool>,
) -> Result<AutomationPort> {
    let before = live_port(tx, id)?;
    let mut after = before.clone();
    if let Some(name) = name {
        let name = checked_name("port", name)?;
        if let Some(holder) = read::automation_port_by_name(
            tx.conn(),
            before.owner_kind,
            before.owner_id,
            before.direction,
            &name,
        )? {
            if holder.id != id {
                return Err(Error::invalid(format!("a port called '{name}' is already declared here")));
            }
        }
        after.name = name;
    }
    if let Some(kind) = kind {
        after.kind = kind;
    }
    if let Some(required) = required {
        after.required = required;
    }
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_port(&before), record::automation_port(&after))?;
    Ok(after)
}

/// Reorder a port within its owner's list for that direction.
pub fn port_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationPort> {
    let before = live_port(tx, id)?;
    let sibs = read::automation_port_siblings(
        tx.conn(),
        before.owner_kind,
        before.owner_id,
        before.direction,
        Some(id),
    )?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_port(&before), record::automation_port(&after))?;
    Ok(after)
}

/// Delete a port. The wires that named it are left where they are, parted, for the reason
/// [`exit_rename`] gives.
pub fn port_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    live_port(tx, id)?;
    tx.delete_record("automation_port", id)?;
    Ok(())
}

/// Declare a setting on a step or a library action: its name, what kind of thing it is, and whether it
/// has to be answered. `options` is the choice list, as JSON, and belongs to `kind = Choice` alone.
///
/// An action's row is the declaration by itself, so its `value` stays empty; a step's own declaration is
/// answered on the same row by [`cfg_set`].
pub fn cfg_add(
    tx: &WriteTx<'_>,
    owner_kind: AutomationOwner,
    owner_id: i64,
    name: &str,
    kind: AutomationCfgKind,
    required: bool,
    options: Option<&str>,
) -> Result<AutomationCfg> {
    checked_declarer(tx, owner_kind, owner_id)?;
    let name = checked_name("setting", name)?;
    checked_options(kind, options)?;
    if read::automation_cfg_by_name(tx.conn(), owner_kind, owner_id, &name)?.is_some() {
        return Err(Error::invalid(format!("a setting called '{name}' is already declared here")));
    }
    write_cfg_row(tx, owner_kind, owner_id, name, kind, required, options.map(str::to_string), None)
}

/// A choice list belongs to a choice and to nothing else — carried on another kind it would be written,
/// never read, and never shown.
fn checked_options(kind: AutomationCfgKind, options: Option<&str>) -> Result<()> {
    if options.is_some() && kind != AutomationCfgKind::Choice {
        return Err(Error::invalid(format!(
            "a list of choices belongs to a 'choice' setting — this one is '{}'",
            kind.as_str()
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_cfg_row(
    tx: &WriteTx<'_>,
    owner_kind: AutomationOwner,
    owner_id: i64,
    name: String,
    kind: AutomationCfgKind,
    required: bool,
    options: Option<String>,
    value: Option<String>,
) -> Result<AutomationCfg> {
    let sibs = read::automation_cfg_siblings(tx.conn(), owner_kind, owner_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_cfg")?;
    let cfg = AutomationCfg {
        id,
        owner_kind,
        owner_id,
        name,
        kind,
        required,
        options,
        value,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_cfg(&cfg))?;
    Ok(cfg)
}

/// Change a setting's declaration — its name, its kind, whether it is required, its choice list. Only
/// the `Some` fields are written.
pub fn cfg_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    kind: Option<AutomationCfgKind>,
    required: Option<bool>,
    options: Option<Option<&str>>,
) -> Result<AutomationCfg> {
    let before = live_cfg(tx, id)?;
    let mut after = before.clone();
    if let Some(name) = name {
        let name = checked_name("setting", name)?;
        if let Some(holder) =
            read::automation_cfg_by_name(tx.conn(), before.owner_kind, before.owner_id, &name)?
        {
            if holder.id != id {
                return Err(Error::invalid(format!(
                    "a setting called '{name}' is already declared here"
                )));
            }
        }
        after.name = name;
    }
    if let Some(kind) = kind {
        after.kind = kind;
    }
    if let Some(options) = options {
        after.options = options.map(str::to_string);
    }
    checked_options(after.kind, after.options.as_deref())?;
    if let Some(required) = required {
        after.required = required;
    }
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_cfg(&before), record::automation_cfg(&after))?;
    Ok(after)
}

/// **Answer a setting on one step.** The answer is JSON, and `None` clears it.
///
/// Where the answer is written depends on where the declaration is. A step with a prompt of its own
/// declared the setting itself, and the answer goes on that row. A step running a library action reads
/// the declaration from the action, whose row carries no answer — so the step takes a row of its own
/// under the same name, born from the action's declaration, and the answer goes there. That is what lets
/// the same action be run at two places in one automation with two different answers.
pub fn cfg_set(tx: &WriteTx<'_>, step_id: i64, name: &str, value: Option<&str>) -> Result<AutomationCfg> {
    let step = live_step(tx, step_id)?;
    let name = checked_name("setting", name)?;
    if let Some(before) = read::automation_cfg_by_name(tx.conn(), AutomationOwner::Step, step.id, &name)? {
        let mut after = before.clone();
        after.value = value.map(str::to_string);
        after.updated_at = Timestamp::now();
        emit_update(tx, record::automation_cfg(&before), record::automation_cfg(&after))?;
        return Ok(after);
    }
    let (owner_kind, owner_id) = declarer(&step);
    let declared = read::automation_cfg_by_name(tx.conn(), owner_kind, owner_id, &name)?
        .ok_or_else(|| Error::not_found(format!("no setting called '{name}' is declared here")))?;
    write_cfg_row(
        tx,
        AutomationOwner::Step,
        step.id,
        declared.name,
        declared.kind,
        declared.required,
        declared.options,
        value.map(str::to_string),
    )
}

/// Reorder a setting within its owner's list.
pub fn cfg_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationCfg> {
    let before = live_cfg(tx, id)?;
    let sibs = read::automation_cfg_siblings(tx.conn(), before.owner_kind, before.owner_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_cfg(&before), record::automation_cfg(&after))?;
    Ok(after)
}

/// Delete a setting — the declaration, or one step's answer to a library action's.
pub fn cfg_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    live_cfg(tx, id)?;
    tx.delete_record("automation_cfg", id)?;
    Ok(())
}

// ───────────────────────────── what runs after what ─────────────────────────────

/// The way out a step leaves through, resolved the way an edge or a wire names it: by name, against
/// whichever of the step and its action declares the ways out.
fn step_exit(
    tx: &WriteTx<'_>,
    step: &AutomationStep,
    exit_name: Option<&str>,
) -> Result<AutomationExit> {
    let (owner_kind, owner_id) = declarer(step);
    read::automation_exit_by_name(tx.conn(), owner_kind, owner_id, exit_name)?.ok_or_else(|| {
        match exit_name {
            Some(n) => Error::not_found(format!(
                "step '{}' has no way out called '{n}'",
                step.id
            )),
            None => Error::not_found(format!("step '{}' has no unnamed way out", step.id)),
        }
    })
}

/// What an edge does once its way out is taken.
#[derive(Clone, Copy, Debug)]
pub enum EdgeTarget {
    /// Open the next step's terminal.
    Go(i64),
    /// Close the run.
    Done,
    /// Stop the run and call a person.
    Halt,
}

impl EdgeTarget {
    fn parts(self) -> (AutomationEnds, Option<i64>) {
        match self {
            EdgeTarget::Go(to) => (AutomationEnds::Go, Some(to)),
            EdgeTarget::Done => (AutomationEnds::Done, None),
            EdgeTarget::Halt => (AutomationEnds::Halt, None),
        }
    }
}

/// Say what happens after one step leaves through one way out.
///
/// `max_times` caps how often the edge may be taken **for one task**; it is counted afresh at the next
/// task, so it stops a loop that never converges without stopping a review that goes round three times.
/// `None` is no limit, which is the right answer for an edge into a step that takes a fresh task, since
/// the count would restart there anyway. It is carried only by `Go`: an edge that closes or stops the
/// run is taken once and has nothing to count. On a `Go` edge the caller's `None` means no limit, and
/// [`crate::model::DEFAULT_MAX_TIMES`] is what the surface above puts there when nobody said.
///
/// **One way out decides one thing**, so a second edge on the same way out is refused rather than
/// leaving the run to pick between them.
pub fn edge_add(
    tx: &WriteTx<'_>,
    from_step_id: i64,
    exit_name: Option<&str>,
    target: EdgeTarget,
    max_times: Option<i64>,
) -> Result<AutomationEdge> {
    let from = live_step(tx, from_step_id)?;
    step_exit(tx, &from, exit_name)?;
    let (ends, to_step_id) = target.parts();
    if let Some(to_step_id) = to_step_id {
        let to = live_step(tx, to_step_id)?;
        if to.automation_id != from.automation_id {
            return Err(Error::invalid(format!(
                "step '{to_step_id}' belongs to another automation — an edge stays inside one"
            )));
        }
    }
    if max_times.is_some() && ends != AutomationEnds::Go {
        return Err(Error::invalid(
            "a limit counts how often an edge is taken, and an edge that closes or stops the run is \
             taken once",
        ));
    }
    if let Some(n) = max_times {
        if n < 1 {
            return Err(Error::invalid("a limit of how often an edge may be taken is at least 1"));
        }
    }
    if read::automation_edge_for_exit(tx.conn(), from_step_id, exit_name)?.is_some() {
        return Err(Error::invalid(match exit_name {
            Some(n) => format!("'{n}' already says what happens after it"),
            None => "the unnamed way out already says what happens after it".to_string(),
        }));
    }
    let sibs = read::automation_edge_siblings(tx.conn(), from.automation_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_edge")?;
    let edge = AutomationEdge {
        id,
        automation_id: from.automation_id,
        from_step_id,
        exit_name: exit_name.map(str::to_string),
        to_step_id,
        ends,
        max_times,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_edge(&edge))?;
    Ok(edge)
}

/// Change where an edge goes, or how often it may be taken. Only the `Some` fields are written, and the
/// way out it hangs on is not among them — that pair is what the edge *is*, so pointing it at another
/// way out is a delete and an add.
pub fn edge_update(
    tx: &WriteTx<'_>,
    id: i64,
    target: Option<EdgeTarget>,
    max_times: Option<Option<i64>>,
) -> Result<AutomationEdge> {
    let before = live_edge(tx, id)?;
    let mut after = before.clone();
    if let Some(target) = target {
        let (ends, to_step_id) = target.parts();
        if let Some(to_step_id) = to_step_id {
            let to = live_step(tx, to_step_id)?;
            if to.automation_id != before.automation_id {
                return Err(Error::invalid(format!(
                    "step '{to_step_id}' belongs to another automation — an edge stays inside one"
                )));
            }
        }
        after.ends = ends;
        after.to_step_id = to_step_id;
    }
    if let Some(max_times) = max_times {
        after.max_times = max_times;
    }
    if after.max_times.is_some() && after.ends != AutomationEnds::Go {
        return Err(Error::invalid(
            "a limit counts how often an edge is taken, and an edge that closes or stops the run is \
             taken once",
        ));
    }
    if let Some(n) = after.max_times {
        if n < 1 {
            return Err(Error::invalid("a limit of how often an edge may be taken is at least 1"));
        }
    }
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_edge(&before), record::automation_edge(&after))?;
    Ok(after)
}

/// Delete an edge. The way out is then read as saying nothing, which for the error one means stopping
/// the run and calling a person, and for any other means the run has nowhere to go.
pub fn edge_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    live_edge(tx, id)?;
    tx.delete_record("automation_edge", id)?;
    Ok(())
}

/// Join what one way out hands on to what a later step takes in.
///
/// Both ends are checked against what is declared, and the two have to carry the same kind of thing — a
/// file into a file, a value into a value. Several wires may land on one input: which of them the step
/// actually reads is the run's to decide, from whichever wrote last in the stretch it is in.
///
/// **Drawing the same wire twice answers the one already drawn** rather than writing a second row.
pub fn wire_add(
    tx: &WriteTx<'_>,
    from_step_id: i64,
    from_exit_name: Option<&str>,
    from_port_name: &str,
    to_step_id: i64,
    to_port_name: &str,
) -> Result<AutomationWire> {
    let from = live_step(tx, from_step_id)?;
    let to = live_step(tx, to_step_id)?;
    if from.automation_id != to.automation_id {
        return Err(Error::invalid(
            "a wire joins two steps of one automation — these are in two",
        ));
    }
    let exit = step_exit(tx, &from, from_exit_name)?;
    let out = read::automation_port_by_name(
        tx.conn(),
        AutomationPortOwner::Exit,
        exit.id,
        AutomationPortDirection::Out,
        from_port_name,
    )?
    .ok_or_else(|| {
        Error::not_found(format!("that way out of step '{from_step_id}' hands on no '{from_port_name}'"))
    })?;
    let (to_owner, to_owner_id) = port_declarer(&to);
    let into = read::automation_port_by_name(
        tx.conn(),
        to_owner,
        to_owner_id,
        AutomationPortDirection::In,
        to_port_name,
    )?
    .ok_or_else(|| Error::not_found(format!("step '{to_step_id}' takes in no '{to_port_name}'")))?;
    if out.kind != into.kind {
        return Err(Error::invalid(format!(
            "'{from_port_name}' carries a {} and '{to_port_name}' takes a {} — a wire joins two of one \
             kind",
            out.kind.as_str(),
            into.kind.as_str()
        )));
    }
    if let Some(drawn) = read::automation_wire_between(
        tx.conn(),
        from_step_id,
        from_exit_name,
        from_port_name,
        to_step_id,
        to_port_name,
    )? {
        return Ok(drawn);
    }
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_wire")?;
    let wire = AutomationWire {
        id,
        automation_id: from.automation_id,
        from_step_id,
        from_exit_name: from_exit_name.map(str::to_string),
        from_port_name: from_port_name.to_string(),
        to_step_id,
        to_port_name: to_port_name.to_string(),
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_wire(&wire))?;
    Ok(wire)
}

/// Delete a wire. The step then reads nothing on that input unless another wire lands on it.
pub fn wire_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    live_wire(tx, id)?;
    tx.delete_record("automation_wire", id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, with_tx};

    /// One automation with one step carrying its own prompt — the shape nearly every test here starts
    /// from.
    fn mk_step(tx: &WriteTx<'_>, automation_id: i64, name: &str) -> AutomationStep {
        step_add(tx, automation_id, NewStep::with_prompt(name, "do it", "claude")).expect("add step")
    }

    fn mk_automation(tx: &WriteTx<'_>) -> Automation {
        let project = mk_project(tx, "amenbo");
        add(tx, project, NewAutomation { name: "1件やりきる".into(), ..Default::default() })
            .expect("add automation")
    }

    fn exit_names(tx: &WriteTx<'_>, owner: AutomationOwner, owner_id: i64) -> Vec<Option<String>> {
        read::automation_exits_of(tx.conn(), owner, owner_id)
            .expect("read exits")
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    #[test]
    fn a_step_is_born_with_the_unnamed_way_out_and_the_error_one() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "実装する");
            assert_eq!(
                exit_names(tx, AutomationOwner::Step, step.id),
                vec![None, Some(ERROR_EXIT.to_string())],
                "both are written at birth, the error one last so it sits at the bottom of the list",
            );
        });
    }

    #[test]
    fn a_step_running_an_action_declares_nothing_of_its_own() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let action = action_add(tx, Some(automation.project_id), "点検する", "look at it")
                .expect("add action");
            let step = step_add(tx, automation.id, NewStep::with_action("点検", action.id, "claude"))
                .expect("add step");
            assert!(
                exit_names(tx, AutomationOwner::Step, step.id).is_empty(),
                "its ways out are the action's",
            );
            assert_eq!(
                exit_names(tx, AutomationOwner::Action, action.id),
                vec![None, Some(ERROR_EXIT.to_string())],
            );
            let refused = exit_add(tx, AutomationOwner::Step, step.id, Some("直すところがある"));
            assert!(refused.is_err(), "a declaration on such a step would never be read");
        });
    }

    #[test]
    fn the_error_way_out_is_neither_renamed_nor_deleted() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "実装する");
            let error = read::automation_exit_by_name(
                tx.conn(),
                AutomationOwner::Step,
                step.id,
                Some(ERROR_EXIT),
            )
            .expect("read")
            .expect("the error way out");
            assert!(exit_rename(tx, error.id, Some("落ちた")).is_err());
            assert!(exit_delete(tx, error.id).is_err());
            assert!(
                exit_add(tx, AutomationOwner::Step, step.id, Some(ERROR_EXIT)).is_err(),
                "nor may another take its name",
            );
        });
    }

    #[test]
    fn a_way_out_says_one_thing_about_what_happens_next() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let one = mk_step(tx, automation.id, "実装する");
            let two = mk_step(tx, automation.id, "点検する");
            let edge = edge_add(tx, one.id, None, EdgeTarget::Go(two.id), Some(5)).expect("add edge");
            assert_eq!(edge.ends, AutomationEnds::Go);
            assert_eq!(edge.max_times, Some(5));
            assert!(
                edge_add(tx, one.id, None, EdgeTarget::Done, None).is_err(),
                "a second edge on the same way out would leave the run to pick",
            );
        });
    }

    #[test]
    fn an_edge_that_closes_the_run_counts_nothing() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "実装する");
            assert!(edge_add(tx, step.id, None, EdgeTarget::Done, Some(3)).is_err());
            let edge = edge_add(tx, step.id, None, EdgeTarget::Done, None).expect("add edge");
            assert!(edge_update(tx, edge.id, None, Some(Some(3))).is_err());
        });
    }

    #[test]
    fn an_edge_stays_inside_one_automation() {
        with_tx(|tx| {
            let here = mk_automation(tx);
            let there =
                add(tx, here.project_id, NewAutomation { name: "別".into(), ..Default::default() })
                    .expect("add automation");
            let from = mk_step(tx, here.id, "実装する");
            let to = mk_step(tx, there.id, "点検する");
            assert!(edge_add(tx, from.id, None, EdgeTarget::Go(to.id), None).is_err());
        });
    }

    #[test]
    fn an_edge_names_a_way_out_that_is_declared() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "点検する");
            assert!(
                edge_add(tx, step.id, Some("直すところがある"), EdgeTarget::Done, None).is_err(),
                "nothing declares it yet",
            );
            exit_add(tx, AutomationOwner::Step, step.id, Some("直すところがある")).expect("add exit");
            edge_add(tx, step.id, Some("直すところがある"), EdgeTarget::Done, None).expect("add edge");
        });
    }

    #[test]
    fn a_wire_joins_two_ports_of_one_kind() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let from = mk_step(tx, automation.id, "実装する");
            let to = mk_step(tx, automation.id, "点検する");
            let exit = read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, from.id, None)
                .expect("read")
                .expect("the unnamed way out");
            port_add(
                tx,
                AutomationPortOwner::Exit,
                exit.id,
                AutomationPortDirection::Out,
                "差分",
                AutomationPortKind::File,
                true,
            )
            .expect("declare the output");
            port_add(
                tx,
                AutomationPortOwner::Step,
                to.id,
                AutomationPortDirection::In,
                "差分",
                AutomationPortKind::Value,
                true,
            )
            .expect("declare the input");
            assert!(
                wire_add(tx, from.id, None, "差分", to.id, "差分").is_err(),
                "a file does not go into a value",
            );
            let into = read::automation_port_by_name(
                tx.conn(),
                AutomationPortOwner::Step,
                to.id,
                AutomationPortDirection::In,
                "差分",
            )
            .expect("read")
            .expect("the input");
            port_update(tx, into.id, None, Some(AutomationPortKind::File), None).expect("retype it");
            let wire = wire_add(tx, from.id, None, "差分", to.id, "差分").expect("draw the wire");
            let again = wire_add(tx, from.id, None, "差分", to.id, "差分").expect("draw it again");
            assert_eq!(wire.id, again.id, "the same wire twice is the one wire");
        });
    }

    #[test]
    fn an_input_hangs_on_the_step_and_an_output_on_the_way_out() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "実装する");
            let exit = read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, step.id, None)
                .expect("read")
                .expect("the unnamed way out");
            assert!(
                port_add(
                    tx,
                    AutomationPortOwner::Step,
                    step.id,
                    AutomationPortDirection::Out,
                    "差分",
                    AutomationPortKind::File,
                    true
                )
                .is_err(),
                "an output belongs to the way out",
            );
            assert!(
                port_add(
                    tx,
                    AutomationPortOwner::Exit,
                    exit.id,
                    AutomationPortDirection::In,
                    "差分",
                    AutomationPortKind::File,
                    true
                )
                .is_err(),
                "an input belongs to the step",
            );
        });
    }

    #[test]
    fn a_step_answers_a_library_actions_setting_on_a_row_of_its_own() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let action = action_add(tx, Some(automation.project_id), "取る", "take one")
                .expect("add action");
            cfg_add(
                tx,
                AutomationOwner::Action,
                action.id,
                "どのタスクを取るか",
                AutomationCfgKind::TaskFilter,
                true,
                None,
            )
            .expect("declare the setting");
            let step = step_add(tx, automation.id, NewStep::with_action("取る", action.id, "claude"))
                .expect("add step");
            let answered = cfg_set(tx, step.id, "どのタスクを取るか", Some("{\"status\":\"todo\"}"))
                .expect("answer it");
            assert_eq!(answered.owner_kind, AutomationOwner::Step);
            assert_eq!(answered.owner_id, step.id);
            assert_eq!(answered.kind, AutomationCfgKind::TaskFilter, "born from the declaration");
            assert!(answered.required);
            let declaration = read::automation_cfg_by_name(
                tx.conn(),
                AutomationOwner::Action,
                action.id,
                "どのタスクを取るか",
            )
            .expect("read")
            .expect("the declaration");
            assert_eq!(declaration.value, None, "the action's row stays a declaration");
            let again = cfg_set(tx, step.id, "どのタスクを取るか", Some("{\"status\":\"blocked\"}"))
                .expect("answer it again");
            assert_eq!(again.id, answered.id, "the second answer rewrites the first row");
        });
    }

    #[test]
    fn a_setting_nobody_declared_cannot_be_answered() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "実装する");
            assert!(cfg_set(tx, step.id, "どのタスクを取るか", Some("{}")).is_err());
        });
    }

    #[test]
    fn a_list_of_choices_belongs_to_a_choice() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "実装する");
            assert!(cfg_add(
                tx,
                AutomationOwner::Step,
                step.id,
                "どれ",
                AutomationCfgKind::Text,
                false,
                Some("[\"a\",\"b\"]")
            )
            .is_err());
            cfg_add(
                tx,
                AutomationOwner::Step,
                step.id,
                "どれ",
                AutomationCfgKind::Choice,
                false,
                Some("[\"a\",\"b\"]"),
            )
            .expect("a choice carries its list");
        });
    }

    #[test]
    fn pointing_a_step_at_an_action_takes_its_own_declarations_with_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "点検する");
            exit_add(tx, AutomationOwner::Step, step.id, Some("直すところがある")).expect("add exit");
            let action = action_add(tx, Some(automation.project_id), "点検", "look").expect("add action");
            step_update(
                tx,
                step.id,
                None,
                Some(StepSource::Action(action.id)),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("point it at the action");
            assert!(exit_names(tx, AutomationOwner::Step, step.id).is_empty());
            let back = step_update(
                tx,
                step.id,
                None,
                Some(StepSource::Prompt("look again".into())),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("give it a prompt again");
            assert_eq!(back.action_id, None);
            assert_eq!(
                exit_names(tx, AutomationOwner::Step, step.id),
                vec![None, Some(ERROR_EXIT.to_string())],
                "it is born with the two again, exactly as a new step is",
            );
        });
    }

    /// **Raising a step moves what it declared, and leaves the answers where they were** (`AMB-T-5277`).
    ///
    /// The ways out have to arrive under the same names, because an edge and a wire name one by its
    /// name: a raise that dropped them would take the picture around the step apart. The answers stay on
    /// the step, which is where a step running an action keeps them anyway.
    #[test]
    fn raising_a_step_moves_its_declarations_and_keeps_its_answers() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "点検する");
            exit_add(tx, AutomationOwner::Step, step.id, Some("直すところがある")).expect("way out");
            let exit = read::automation_exit_by_name(
                tx.conn(),
                AutomationOwner::Step,
                step.id,
                Some("直すところがある"),
            )
            .expect("read")
            .expect("the way out");
            port_add(
                tx,
                AutomationPortOwner::Exit,
                exit.id,
                AutomationPortDirection::Out,
                "指摘",
                AutomationPortKind::File,
                true,
            )
            .expect("output");
            port_add(
                tx,
                AutomationPortOwner::Step,
                step.id,
                AutomationPortDirection::In,
                "差分",
                AutomationPortKind::File,
                true,
            )
            .expect("input");
            cfg_add(
                tx,
                AutomationOwner::Step,
                step.id,
                "どこまで見るか",
                AutomationCfgKind::Text,
                false,
                None,
            )
            .expect("setting");
            cfg_set(tx, step.id, "どこまで見るか", Some("\"全部\"")).expect("answer it");

            let action = action_from_step(tx, step.id, Some(automation.project_id), "点検").expect("raise");

            assert_eq!(action.prompt, "do it", "the prompt went with it");
            let after = read::automation_step(tx.conn(), step.id).expect("read").expect("the step");
            assert_eq!(after.action_id, Some(action.id));
            assert_eq!(after.prompt, None, "two copies of one prompt is what this avoids");

            // The names an edge or a wire would be pointing at.
            assert_eq!(
                exit_names(tx, AutomationOwner::Action, action.id),
                vec![None, Some(ERROR_EXIT.to_string()), Some("直すところがある".to_string())],
            );
            assert!(
                exit_names(tx, AutomationOwner::Step, step.id).is_empty(),
                "the step declares nothing of its own now",
            );
            let raised = read::automation_exit_by_name(
                tx.conn(),
                AutomationOwner::Action,
                action.id,
                Some("直すところがある"),
            )
            .expect("read")
            .expect("the way out");
            let outs = read::automation_ports_of(
                tx.conn(),
                AutomationPortOwner::Exit,
                raised.id,
                AutomationPortDirection::Out,
            )
            .expect("read");
            assert_eq!(outs.len(), 1);
            assert_eq!(outs[0].name, "指摘");
            assert!(outs[0].required);
            let ins = read::automation_ports_of(
                tx.conn(),
                AutomationPortOwner::Action,
                action.id,
                AutomationPortDirection::In,
            )
            .expect("read");
            assert_eq!(ins.len(), 1, "the input reads on the action now");
            assert_eq!(ins[0].name, "差分");

            // The declaration is the action's and the answer is still the step's.
            let declared = read::automation_cfg_by_name(
                tx.conn(),
                AutomationOwner::Action,
                action.id,
                "どこまで見るか",
            )
            .expect("read")
            .expect("the declaration");
            assert_eq!(declared.value, None);
            let answer = read::automation_cfg_by_name(
                tx.conn(),
                AutomationOwner::Step,
                step.id,
                "どこまで見るか",
            )
            .expect("read")
            .expect("the answer");
            assert_eq!(answer.value.as_deref(), Some("\"全部\""));

            assert!(
                action_from_step(tx, step.id, None, "もう一度").is_err(),
                "there is nothing of its own left to raise",
            );
        });
    }

    #[test]
    fn a_step_reaches_its_own_projects_library_and_the_devices() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let mine = action_add(tx, Some(automation.project_id), "自前", "x").expect("add action");
            let shared = action_add(tx, None, "共有", "x").expect("add action");
            let other_project = mk_project(tx, "別の企画");
            let theirs = action_add(tx, Some(other_project), "他所", "x").expect("add action");
            step_add(tx, automation.id, NewStep::with_action("a", mine.id, "claude")).expect("mine");
            step_add(tx, automation.id, NewStep::with_action("b", shared.id, "claude")).expect("shared");
            assert!(
                step_add(tx, automation.id, NewStep::with_action("c", theirs.id, "claude")).is_err(),
                "another project's prompt is not read across the boundary",
            );
        });
    }

    #[test]
    fn deleting_a_step_clears_the_entry_and_takes_the_edges_naming_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let one = mk_step(tx, automation.id, "取る");
            let two = mk_step(tx, automation.id, "実装する");
            set_entry(tx, automation.id, Some(one.id)).expect("name the entry");
            edge_add(tx, one.id, None, EdgeTarget::Go(two.id), None).expect("add edge");
            step_delete(tx, one.id).expect("delete the step");
            let after = live_automation(tx, automation.id).expect("read it back");
            assert_eq!(after.entry_step_id, None);
            assert!(read::automation_edge_ids(tx.conn(), automation.id).expect("read").is_empty());
            assert!(
                read::automation_exit_ids(tx.conn(), AutomationOwner::Step, one.id)
                    .expect("read")
                    .is_empty(),
                "its ways out went with it",
            );
        });
    }

    #[test]
    fn the_entry_is_a_step_of_this_automation() {
        with_tx(|tx| {
            let here = mk_automation(tx);
            let there =
                add(tx, here.project_id, NewAutomation { name: "別".into(), ..Default::default() })
                    .expect("add automation");
            let elsewhere = mk_step(tx, there.id, "実装する");
            assert!(set_entry(tx, here.id, Some(elsewhere.id)).is_err());
        });
    }

    #[test]
    fn deleting_an_automation_takes_everything_built_into_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let one = mk_step(tx, automation.id, "取る");
            let two = mk_step(tx, automation.id, "実装する");
            set_entry(tx, automation.id, Some(one.id)).expect("name the entry");
            edge_add(tx, one.id, None, EdgeTarget::Go(two.id), None).expect("add edge");
            let note = note_add(tx, automation.id, "運転規約", "…").expect("add note");
            note_link(tx, one.id, note.id).expect("hand it over");
            delete(tx, automation.id).expect("delete the automation");
            assert!(read::automation(tx.conn(), automation.id).expect("read").is_none());
            assert!(read::automation_step_ids(tx.conn(), automation.id).expect("read").is_empty());
            assert!(read::automation_note_ids(tx.conn(), automation.id).expect("read").is_empty());
            assert!(read::automation_step_note_ids(tx.conn(), one.id).expect("read").is_empty());
        });
    }

    #[test]
    fn a_library_action_a_step_runs_is_not_deleted() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let action = action_add(tx, Some(automation.project_id), "点検", "look").expect("add action");
            let step = step_add(tx, automation.id, NewStep::with_action("点検", action.id, "claude"))
                .expect("add step");
            assert!(action_delete(tx, action.id).is_err());
            step_delete(tx, step.id).expect("delete the step");
            action_delete(tx, action.id).expect("now it goes");
            assert!(
                read::automation_exit_ids(tx.conn(), AutomationOwner::Action, action.id)
                    .expect("read")
                    .is_empty(),
                "its ways out went with it",
            );
        });
    }

    #[test]
    fn a_shared_document_is_handed_to_a_step_of_its_own_automation() {
        with_tx(|tx| {
            let here = mk_automation(tx);
            let there =
                add(tx, here.project_id, NewAutomation { name: "別".into(), ..Default::default() })
                    .expect("add automation");
            let step = mk_step(tx, there.id, "実装する");
            let note = note_add(tx, here.id, "運転規約", "…").expect("add note");
            assert!(note_link(tx, step.id, note.id).is_err());
        });
    }

    #[test]
    fn handing_the_same_document_twice_is_the_one_link() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let step = mk_step(tx, automation.id, "実装する");
            let note = note_add(tx, automation.id, "運転規約", "…").expect("add note");
            let first = note_link(tx, step.id, note.id).expect("hand it over");
            let second = note_link(tx, step.id, note.id).expect("hand it over again");
            assert_eq!(first.id, second.id);
            assert!(note_unlink(tx, step.id, note.id).expect("take it back"));
            assert!(!note_unlink(tx, step.id, note.id).expect("nothing left to take"));
        });
    }

    #[test]
    fn a_name_is_free_within_its_owner_and_nowhere_wider() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let one = mk_step(tx, automation.id, "点検する");
            let two = mk_step(tx, automation.id, "直す");
            exit_add(tx, AutomationOwner::Step, one.id, Some("直すところがある")).expect("add exit");
            assert!(
                exit_add(tx, AutomationOwner::Step, one.id, Some("直すところがある")).is_err(),
                "twice on one step is one name too many",
            );
            exit_add(tx, AutomationOwner::Step, two.id, Some("直すところがある"))
                .expect("another step is another list");
        });
    }
}
