//! Building an automation's definition — the nine tables of the definition side. Of the run side
//! there is one op here, [`run_delete`], and it is a sweep rather than a launch: the definition and what
//! was launched from it go down together when the project does.
//!
//! **Three layers, one word each** (`AMB-D-949`). An automation places library actions; an action holds
//! steps; one step is one terminal. It stops there — an action places no action — so nothing here has to
//! say which of two things a "step" is.
//!
//! **Two pictures, drawn the same way.** An automation's boxes are placements and an action's are
//! steps, but a line is a line either way: `automation_edge` and `automation_wire` carry which picture
//! they are on, and the ops here take that pair rather than assuming one of them. What a box declares is
//! read one hop off: a placement reads its action's ways out and inputs, a step reads its own
//! ([`box_declarer`]).
//!
//! **What an action declares is joined to what is inside it, never merely spelled the same.** Three
//! lines cross that edge, and all three are the ordinary ones: an [`EdgeTarget::Exit`] edge says which
//! way out of the action a way out of a step returns to, and a wire with [`ACTION_BOUNDARY`] at one end
//! carries an input the action was handed into a step, or a value a step produced out of the way out the
//! run is leaving by. Two names that happen to match join nothing.
//!
//! **A setting is declared by an action and answered by a placement.** That is what lets one action be
//! placed twice on one automation with two different answers, and it is why a step declares none: an
//! action holds several steps, and each of them reaches a setting by the name the action gave it.
//!
//! **A way out is keyed; a port is named.** An edge keys the way out it hangs on and a wire the way
//! out it leaves by, so renaming a way out leaves every line on it standing, and deleting one takes
//! the lines with it (`AMB-D-961`). The ports at a wire's two ends are still named, and renaming one
//! parts the wire from it.
//!
//! **Nothing here refuses an unfinished automation.** A box with no way onward, an automation with no
//! entry, a required setting nobody answered — each of them saves. What refuses them is the launch
//! check, which is where a person is actually about to be let down by them.
//!
//! **Writes go straight to SQL through the engine.** Every mutator takes the [`WriteTx`]
//! (`BEGIN IMMEDIATE`) its caller opened and does its reads — the `before` snapshot, the new id, the
//! sibling `order_key`, the name it has to find free — inside that same transaction. The subtree
//! deletes ride on one transaction too: apply them half-way and a step's ways out outlive the step.

use crate::error::{Error, Result};
use crate::model::{
    AttachmentTarget, Automation, AutomationAction, AutomationCfg, AutomationCfgKind,
    AutomationCfgOwner, AutomationEdge, AutomationEnds, AutomationExit,
    AutomationOwner, AutomationPictureOwner, AutomationPlacement,
    AutomationPort, AutomationPortDirection, AutomationPortKind, AutomationPortOwner, AutomationStep,
    AutomationWire, ACTION_BOUNDARY, DEFAULT_MAX_TIMES, ERROR_EXIT,
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

fn live_placement(tx: &WriteTx<'_>, id: i64) -> Result<AutomationPlacement> {
    read::automation_placement(tx.conn(), id)?.ok_or_else(|| not_found("placement", id))
}

fn live_step(tx: &WriteTx<'_>, id: i64) -> Result<AutomationStep> {
    read::automation_action_step(tx.conn(), id)?.ok_or_else(|| not_found("step", id))
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

// ───────────────────────────── a definition a run is going on ─────────────────────────────

/// The definition an op is about to rewrite: an automation's picture, or a library action's insides.
#[derive(Clone, Copy, Debug)]
enum Def {
    Automation(i64),
    Action(i64),
}

/// **A definition a run is going on is not rewritten** (`AMB-D-961`) — not while a run launched from it
/// is `running` or `paused`, since a paused one can be picked up again.
///
/// The run works from the copy it took at launch, so a rewrite would not steer it; what it would do is
/// leave the picture a person reads saying something the run is not doing. Refusing is also what keeps
/// the run side simple: nothing there has to reconcile the copy with a definition that moved under it.
///
/// An action is in use wherever it is placed, on any project's automation — a device-wide action placed
/// by another project is held by that project's run all the same. The refusal names the runs, since
/// ending them is the way to edit again. Running a run — pausing, resuming, stopping — is not a rewrite
/// of the definition and never comes here.
fn not_under_a_run(tx: &WriteTx<'_>, def: Def) -> Result<()> {
    #[cfg(test)]
    if PAST_THE_GUARD.with(std::cell::Cell::get) {
        return Ok(());
    }
    let automations = match def {
        Def::Automation(id) => vec![id],
        Def::Action(id) => {
            let mut ids = Vec::new();
            for placement in read::automation_placement_ids_using_action(tx.conn(), id)? {
                let automation_id = live_placement(tx, placement)?.automation_id;
                if !ids.contains(&automation_id) {
                    ids.push(automation_id);
                }
            }
            ids
        }
    };
    let mut runs = Vec::new();
    for automation_id in automations {
        runs.extend(read::automation_run_ids_under_way(tx.conn(), automation_id)?);
    }
    if runs.is_empty() {
        return Ok(());
    }
    runs.sort_unstable();
    let what = match def {
        Def::Automation(id) => format!("automation '{}'", live_automation(tx, id)?.name),
        Def::Action(id) => format!("action '{}'", live_action(tx, id)?.name),
    };
    let named = runs.iter().map(i64::to_string).collect::<Vec<_>>().join(", ");
    Err(Error::conflict(format!(
        "{what} is in use by run {named}, which is running or paused — its definition cannot change \
         until the run ends. Stop it with `automation stop <run>`, or let it finish, then edit."
    )))
}

#[cfg(test)]
thread_local! {
    static PAST_THE_GUARD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// **Rewrite a definition a run is going on, for a test.** A store written by a build that let the
/// definition be edited under a run can still hold a run whose picture moved, and the run side goes on
/// answering for that shape; this is how its tests draw it now that no op will.
#[cfg(test)]
pub(crate) fn past_the_guard<T>(rewrite: impl FnOnce() -> T) -> T {
    PAST_THE_GUARD.with(|g| g.set(true));
    let out = rewrite();
    PAST_THE_GUARD.with(|g| g.set(false));
    out
}

/// The definition a way out, or what declares one, belongs to: a step's is its action's, an action's
/// is its own.
fn def_of_declarer(tx: &WriteTx<'_>, owner_kind: AutomationOwner, owner_id: i64) -> Result<Def> {
    Ok(match owner_kind {
        AutomationOwner::Step => Def::Action(live_step(tx, owner_id)?.action_id),
        AutomationOwner::Action => Def::Action(owner_id),
    })
}

/// The definition a port belongs to — through the way out it hangs off, where it hangs off one.
fn def_of_port_owner(tx: &WriteTx<'_>, owner_kind: AutomationPortOwner, owner_id: i64) -> Result<Def> {
    match owner_kind {
        AutomationPortOwner::Step => def_of_declarer(tx, AutomationOwner::Step, owner_id),
        AutomationPortOwner::Action => def_of_declarer(tx, AutomationOwner::Action, owner_id),
        AutomationPortOwner::Exit => {
            let exit = live_exit(tx, owner_id)?;
            def_of_declarer(tx, exit.owner_kind, exit.owner_id)
        }
    }
}

/// The definition a setting belongs to: an action's declaration is the action's, a placement's answer
/// is the automation's it stands on.
fn def_of_cfg_owner(tx: &WriteTx<'_>, owner_kind: AutomationCfgOwner, owner_id: i64) -> Result<Def> {
    Ok(match owner_kind {
        AutomationCfgOwner::Action => Def::Action(owner_id),
        AutomationCfgOwner::Placement => Def::Automation(live_placement(tx, owner_id)?.automation_id),
    })
}

/// The definition a line is drawn in.
fn def_of_picture(owner_kind: AutomationPictureOwner, owner_id: i64) -> Def {
    match owner_kind {
        AutomationPictureOwner::Automation => Def::Automation(owner_id),
        AutomationPictureOwner::Action => Def::Action(owner_id),
    }
}

/// **Where a box's declarations are read from.** A placement reads the action standing on it; a step
/// reads itself. One answer, asked by everything that resolves a name on a box — an edge's way out, a
/// wire's ends.
///
/// It refuses a box that is not in the picture it was asked about, which is the check every edge and
/// wire op would otherwise make twice.
fn box_declarer(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    box_id: i64,
) -> Result<(AutomationOwner, i64)> {
    match owner_kind {
        AutomationPictureOwner::Automation => {
            let placement = live_placement(tx, box_id)?;
            Ok((AutomationOwner::Action, placement.action_id))
        }
        AutomationPictureOwner::Action => {
            let step = live_step(tx, box_id)?;
            Ok((AutomationOwner::Step, step.id))
        }
    }
}

/// The same pair as a port's owner, which admits a third kind this one does not.
fn box_port_declarer(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    box_id: i64,
) -> Result<(AutomationPortOwner, i64)> {
    let (declarer, id) = box_declarer(tx, owner_kind, box_id)?;
    Ok((
        match declarer {
            AutomationOwner::Action => AutomationPortOwner::Action,
            AutomationOwner::Step => AutomationPortOwner::Step,
        },
        id,
    ))
}

/// **Which picture a box is drawn on** — the automation a placement sits on, or the action a step is
/// held by. What an edge checks both of its ends against, since a line stays inside one picture.
fn box_picture(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    box_id: i64,
) -> Result<i64> {
    match owner_kind {
        AutomationPictureOwner::Automation => Ok(live_placement(tx, box_id)?.automation_id),
        AutomationPictureOwner::Action => Ok(live_step(tx, box_id)?.action_id),
    }
}

/// What a box is called where a refusal has to name it: an automation's boxes are placements, an
/// action's are steps.
fn box_word(owner_kind: AutomationPictureOwner) -> &'static str {
    match owner_kind {
        AutomationPictureOwner::Automation => "placement",
        AutomationPictureOwner::Action => "step",
    }
}

/// The two ways out every step and every action is born with: the unnamed one, which is all an owner
/// with a single way out needs, and the error one, which nobody can delete. Written at the moment the
/// owner is created so that an edge or a port has somewhere to hang from the first command onwards.
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

/// Delete one way out, the outputs declared on it, and the lines keyed to it — an edge that leaves by
/// it or returns to it, and a wire that carries what it hands on. A line keyed to a row that is gone
/// decides nothing and carries nothing, which is why it goes with the row (`AMB-D-961`).
fn delete_exit_row(tx: &WriteTx<'_>, exit_id: i64) -> Result<()> {
    let (edges, wires) = read::automation_line_ids_on_exit(tx.conn(), exit_id)?;
    for wire in wires {
        tx.delete_record("automation_wire", wire)?;
    }
    for edge in edges {
        tx.delete_record("automation_edge", edge)?;
    }
    for port in read::automation_port_ids(tx.conn(), AutomationPortOwner::Exit, exit_id)? {
        tx.delete_record("automation_port", port)?;
    }
    tx.delete_record("automation_exit", exit_id)?;
    Ok(())
}

/// Delete every way out and port one owner declares — what goes when a step or an action does. The
/// settings are swept apart from these, since an action's and a placement's are the two halves of one
/// declaration ([`delete_cfgs`]).
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
    Ok(())
}

/// Delete the settings one half of the pair carries: an action's declarations, or one placement's
/// answers to them.
fn delete_cfgs(tx: &WriteTx<'_>, owner: AutomationCfgOwner, owner_id: i64) -> Result<()> {
    for cfg in read::automation_cfg_ids(tx.conn(), owner, owner_id)? {
        tx.delete_record("automation_cfg", cfg)?;
    }
    Ok(())
}

/// Delete every line of one picture that names one box at either end — what goes when the box does,
/// since an edge pointing at a box that is gone decides nothing and a wire naming it carries nothing.
fn delete_lines_naming_box(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    box_id: i64,
) -> Result<()> {
    for wire in read::automation_wire_ids_naming_box(tx.conn(), owner_kind, box_id)? {
        tx.delete_record("automation_wire", wire)?;
    }
    for edge in read::automation_edge_ids_naming_box(tx.conn(), owner_kind, box_id)? {
        tx.delete_record("automation_edge", edge)?;
    }
    Ok(())
}

// ───────────────────────────── the library (automation_action) ─────────────────────────────

/// Add an action to the library. `project_id` `None` puts it in the device's own, where every project on
/// this machine reaches it; `Some` puts it in one project's.
///
/// It is born empty — no steps, no entry — and carrying the two ways out every declarer has
/// ([`born_with_exits`]), so a placement of it can be drawn into a picture before its insides are
/// written. It names no agent and no model: who is asked to carry a prompt out is each step's answer.
///
/// `note` is what the action is for, for whoever builds with it. It is drawn on the build screen and
/// never carried into a launch, which is `automation.notes`' reading one layer up.
pub fn action_add(
    tx: &WriteTx<'_>,
    project_id: Option<i64>,
    name: &str,
    note: &str,
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
        note: note.to_string(),
        entry_step_id: None,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_action(&action))?;
    born_with_exits(tx, AutomationOwner::Action, id)?;
    Ok(action)
}

/// Rename a library action, or rewrite what it is for. Only the `Some` fields are written.
///
/// **Renaming the action is safe; renaming what it declares is not.** A placement points at the action
/// by key (`automation_placement.action_id`), so nothing parts here — while renaming one of its ways out
/// or its ports parts every edge and wire that named the old one ([`exit_rename`], [`port_update`]).
pub fn action_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    note: Option<&str>,
) -> Result<AutomationAction> {
    let before = live_action(tx, id)?;
    not_under_a_run(tx, Def::Action(id))?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = checked_name("action", name)?;
    }
    if let Some(note) = note {
        after.note = note.to_string();
    }
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_action(&before), record::automation_action(&after))?;
    Ok(after)
}

/// Name the step a run opens first when it reaches a placement of this action, or clear it with `None`.
/// The step has to be one of this action's.
pub fn action_set_entry(
    tx: &WriteTx<'_>,
    action_id: i64,
    step_id: Option<i64>,
) -> Result<AutomationAction> {
    let before = live_action(tx, action_id)?;
    not_under_a_run(tx, Def::Action(action_id))?;
    if let Some(step_id) = step_id {
        let step = live_step(tx, step_id)?;
        if step.action_id != action_id {
            return Err(Error::invalid(format!(
                "step '{step_id}' belongs to another action, so it cannot be this one's entry"
            )));
        }
    }
    let mut after = before.clone();
    after.entry_step_id = step_id;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_action(&before), record::automation_action(&after))?;
    Ok(after)
}

/// **Write a whole action in one act**: the action, one step carrying the prompt, the ways out and
/// inputs they are declared with, and the lines that join the two. It is what a build screen reaches for
/// where a person is writing a prompt rather than picking one out of the library (`AMB-T-5317`).
///
/// **The declarations are written on both, and joined.** The action's are what a placement of it is
/// wired by, and the step's are what the picture inside the action is drawn with. Carrying the same name
/// is not what joins them: each way out of the step is given an `Exit` edge onto the action's way out of
/// that name, and each input the action declares is wired from the boundary onto the step's. An action
/// of one step is the degenerate case of the mapping, not the absence of one.
///
/// The two ways out the pair is born with are joined as well — the unnamed one, so that an action
/// nobody named a way out of still leaves, and the error one, so that a step that fell over leaves by
/// the action's error way out and the picture the placement stands on decides what to do about it,
/// instead of the run halting inside an action the outer picture never sees.
pub fn action_from_prompt(
    tx: &WriteTx<'_>,
    project_id: Option<i64>,
    new: NewStep,
    exits: &[String],
    inputs: &[(String, AutomationPortKind, bool)],
) -> Result<AutomationAction> {
    let action = action_add(tx, project_id, &new.name, "")?;
    let step = step_add(tx, action.id, new)?;
    for name in exits {
        exit_add(tx, AutomationOwner::Action, action.id, Some(name))?;
        exit_add(tx, AutomationOwner::Step, step.id, Some(name))?;
    }
    let born_with = [None, Some(ERROR_EXIT.to_string())];
    for name in exits.iter().cloned().map(Some).chain(born_with) {
        edge_add(
            tx,
            AutomationPictureOwner::Action,
            step.id,
            name.as_deref(),
            EdgeTarget::Exit(name.clone()),
            None,
        )?;
    }
    for (name, kind, required) in inputs {
        port_add(
            tx,
            AutomationPortOwner::Action,
            action.id,
            AutomationPortDirection::In,
            name,
            *kind,
            *required,
        )?;
        port_add(
            tx,
            AutomationPortOwner::Step,
            step.id,
            AutomationPortDirection::In,
            name,
            *kind,
            *required,
        )?;
        wire_add(
            tx,
            AutomationPictureOwner::Action,
            ACTION_BOUNDARY,
            None,
            name,
            step.id,
            name,
        )?;
    }
    action_set_entry(tx, action.id, Some(step.id))
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

/// **Move a library action to another library** — the device's own (`None`) or one project's — and put
/// it at the bottom there. Nothing under it moves with it, because nothing under it names a library:
/// its steps, declarations and pictures all hang off the action, and the reach of each is walked through
/// it.
///
/// **Refused while a placement would be left out of reach** of the library it lands in: an automation
/// reaches its own project's library and the device's ([`checked_action`]), so into project P the
/// action may carry only placements on P's automations. Out to the device's library is never refused.
/// The refusal names every automation that stands in the way and the project it is in — the same shape
/// as [`action_delete`]'s — and nothing is copied to get round it: two copies of one action would be
/// two things to keep in step (`AMB-D-954`).
pub fn action_set_scope(
    tx: &WriteTx<'_>,
    id: i64,
    project_id: Option<i64>,
) -> Result<AutomationAction> {
    let before = live_action(tx, id)?;
    if before.project_id == project_id {
        return Ok(before);
    }
    not_under_a_run(tx, Def::Action(id))?;
    if let Some(project_id) = project_id {
        if read::project(tx.conn(), project_id)?.is_none() {
            return Err(crate::ops::project::NOUN.not_found(project_id.to_string()));
        }
        let elsewhere = automations_placing_outside(tx, id, project_id)?;
        if !elsewhere.is_empty() {
            return Err(Error::invalid(format!(
                "action '{id}' is placed on automations of other projects — {} — take it off them \
                 before moving it into project {}",
                elsewhere.join(", "),
                crate::idref::project(project_id),
            )));
        }
    }
    let sibs = read::automation_action_siblings(tx.conn(), project_id, Some(id))?;
    let mut after = before.clone();
    after.project_id = project_id;
    after.order_key = place(&sibs, &Position::Bottom)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_action(&before), record::automation_action(&after))?;
    Ok(after)
}

/// The automations outside `project_id` that place this action, each named once with its project —
/// `automation '<name>' (<id>) in project <ref> '<name>'`, in the order the placements were made.
fn automations_placing_outside(
    tx: &WriteTx<'_>,
    action_id: i64,
    project_id: i64,
) -> Result<Vec<String>> {
    let mut seen = Vec::new();
    let mut named = Vec::new();
    for placement in read::automation_placement_ids_using_action(tx.conn(), action_id)? {
        let placement = live_placement(tx, placement)?;
        let automation = live_automation(tx, placement.automation_id)?;
        if automation.project_id == project_id || seen.contains(&automation.id) {
            continue;
        }
        seen.push(automation.id);
        let project = read::project_name(tx.conn(), automation.project_id)?.unwrap_or_default();
        named.push(format!(
            "automation '{}' ({}) in project {} '{project}'",
            automation.name,
            automation.id,
            crate::idref::project(automation.project_id),
        ));
    }
    Ok(named)
}

/// Delete a library action with everything inside it — the picture its steps are drawn into, the steps
/// with their own declarations, and the ways out, ports and settings the action declared.
///
/// **Refused while it is placed**, naming how many placements: the placement would be left standing on
/// nothing, and what should stand there instead is not this op's to guess.
pub fn action_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let action = live_action(tx, id)?;
    not_under_a_run(tx, Def::Action(id))?;
    let users = read::automation_placement_ids_using_action(tx.conn(), id)?;
    if !users.is_empty() {
        return Err(Error::invalid(format!(
            "{} placement(s) stand on this action — take them off the pictures they are on before \
             deleting it",
            users.len()
        )));
    }
    for wire in read::automation_wire_ids(tx.conn(), AutomationPictureOwner::Action, id)? {
        tx.delete_record("automation_wire", wire)?;
    }
    for edge in read::automation_edge_ids(tx.conn(), AutomationPictureOwner::Action, id)? {
        tx.delete_record("automation_edge", edge)?;
    }
    // The entry is a reference into the steps that are about to go, so it is dropped before them.
    if action.entry_step_id.is_some() {
        action_set_entry(tx, id, None)?;
    }
    for step in read::automation_action_step_ids(tx.conn(), id)? {
        delete_step_row(tx, step)?;
    }
    delete_declarations(tx, AutomationOwner::Action, AutomationPortOwner::Action, id)?;
    delete_cfgs(tx, AutomationCfgOwner::Action, id)?;
    tx.delete_record("automation_action", id)?;
    Ok(())
}

// ───────────────────────────── the automation itself ─────────────────────────────

/// What a new automation is made of. What its steps are told before their own prompts is not part of
/// it: that is Amenbo's own ([`crate::agents::preamble`]).
#[derive(Clone, Debug, Default)]
pub struct NewAutomation {
    pub name: String,
    pub notes: String,
}

/// Create an automation. It is born with nothing placed on it and no entry; what refuses to launch it is
/// the launch check, not this.
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
        entry_placement_id: None,
        archived: false,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation(&automation))?;
    Ok(automation)
}

/// Change an automation's name, notes, or whether it is archived. Only the `Some` fields are
/// written. Archiving takes nothing away: it is what keeps an automation nobody launches any more out
/// of the lists. Like every rewrite it is refused while a run of the automation is going
/// ([`not_under_a_run`]).
pub fn update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    notes: Option<&str>,
    archived: Option<bool>,
) -> Result<Automation> {
    let before = live_automation(tx, id)?;
    not_under_a_run(tx, Def::Automation(id))?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = checked_name("automation", name)?;
    }
    if let Some(notes) = notes {
        after.notes = notes.to_string();
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

/// Name the placement a run opens first, or clear it with `None`.
///
/// The placement has to be one of this automation's. Whether the action standing there takes a task —
/// the thing that actually makes it a usable entry — is the launch check's to ask: an automation is
/// built in whatever order its author likes, and refusing the entry until the port exists would make
/// the order the tool's to choose.
pub fn set_entry(
    tx: &WriteTx<'_>,
    automation_id: i64,
    placement_id: Option<i64>,
) -> Result<Automation> {
    let before = live_automation(tx, automation_id)?;
    not_under_a_run(tx, Def::Automation(automation_id))?;
    if let Some(placement_id) = placement_id {
        let placement = live_placement(tx, placement_id)?;
        if placement.automation_id != automation_id {
            return Err(Error::invalid(format!(
                "placement '{placement_id}' belongs to another automation, so it cannot be this one's \
                 entry"
            )));
        }
    }
    let mut after = before.clone();
    after.entry_placement_id = placement_id;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation(&before), record::automation(&after))?;
    Ok(after)
}

/// Delete an automation and everything built onto it — wires, edges, and the placements with the
/// answers written on them. The library actions those placements stood on are left where they are:
/// the library outlives any one picture.
///
/// **Refused while a run stands behind it**, naming how many. A run carries its own copy of the steps
/// and would go on reading correctly, but it is filed under the automation it was launched from, and
/// deleting that leaves the record unable to say what was run.
pub fn delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let automation = live_automation(tx, id)?;
    not_under_a_run(tx, Def::Automation(id))?;
    let runs = read::automation_run_ids(tx.conn(), id)?;
    if !runs.is_empty() {
        return Err(Error::invalid(format!(
            "{} run(s) were launched from this automation — it is what they are filed under, so it \
             cannot be deleted",
            runs.len()
        )));
    }
    for wire in read::automation_wire_ids(tx.conn(), AutomationPictureOwner::Automation, id)? {
        tx.delete_record("automation_wire", wire)?;
    }
    for edge in read::automation_edge_ids(tx.conn(), AutomationPictureOwner::Automation, id)? {
        tx.delete_record("automation_edge", edge)?;
    }
    // The entry is a reference into the placements that are about to go, so it is dropped before them.
    if automation.entry_placement_id.is_some() {
        set_entry(tx, id, None)?;
    }
    for placement in read::automation_placement_ids(tx.conn(), id)? {
        delete_placement_row(tx, placement)?;
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

// ───────────────────────────── placements ─────────────────────────────

/// **Put an action on an automation.** The row carries nothing of the action's own: what it holds is
/// which action stands here, and — through the answers written on it and the lines drawn to it — what
/// belongs to this spot rather than to the library.
///
/// The action has to be within reach of the automation's project: its own project's library, or the
/// device's. Another project's is refused — the prompt would be read across a boundary that is there to
/// keep one project's context out of another's.
pub fn placement_add(
    tx: &WriteTx<'_>,
    automation_id: i64,
    action_id: i64,
) -> Result<AutomationPlacement> {
    let automation = live_automation(tx, automation_id)?;
    not_under_a_run(tx, Def::Automation(automation_id))?;
    checked_action(tx, &automation, action_id)?;
    let sibs = read::automation_placement_siblings(tx.conn(), automation_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_placement")?;
    let placement = AutomationPlacement {
        id,
        automation_id,
        action_id,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_placement(&placement))?;
    Ok(placement)
}

/// **Put an action in on a line**, which is the one road by which a placement joins a picture already
/// drawn — the automation's twin of [`step_insert`]. The two edges it writes are that op's, and the
/// reasons are there.
pub fn placement_insert(
    tx: &WriteTx<'_>,
    edge_id: i64,
    action_id: i64,
) -> Result<AutomationPlacement> {
    let edge = live_edge(tx, edge_id)?;
    let automation_id = box_picture(tx, AutomationPictureOwner::Automation, edge.from_id)?;
    let placement = placement_add(tx, automation_id, action_id)?;
    splice_onto_edge(tx, &edge, placement.id)?;
    Ok(placement)
}

/// Which library an action written at the picture lands in: the device's own, which every project on
/// this machine reaches, or the automation's own project's.
///
/// It is these two and not a project id because an automation reaches no other project's library
/// ([`checked_action`]) — naming one would only be a way of asking for a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionShelf {
    /// The device's own library.
    Device,
    /// The library of the project the automation being built belongs to.
    Project,
}

impl ActionShelf {
    /// The `project_id` an action on this shelf is born with, for an automation in `project_id`.
    fn under(self, project_id: i64) -> Option<i64> {
        match self {
            ActionShelf::Device => None,
            ActionShelf::Project => Some(project_id),
        }
    }
}

/// **Make an empty action and put it on a picture**, standing on its own with no line reaching it —
/// the press a build screen makes where the picture has no line to put one in on, which is every
/// picture with nothing on it yet (`AMB-D-956`).
///
/// **What it takes is a name and a library, and nothing else.** The inside of an action is its steps,
/// and each step carries its own prompt, ways out and outputs — so it is built on the action's own
/// screen, and a dialog that took part of it here would be a second place to declare the same thing
/// (`AMB-D-954`). Until a step is written in it, the launch check refuses the automation on it
/// ([`crate::ops::automation_run`]'s `ActionEmpty`).
///
/// It is one act for [`placement_insert_new`]'s reason: half of it is an action in the library that
/// nothing stands on.
pub fn placement_add_new(
    tx: &WriteTx<'_>,
    automation_id: i64,
    shelf: ActionShelf,
    name: &str,
) -> Result<AutomationPlacement> {
    let automation = live_automation(tx, automation_id)?;
    not_under_a_run(tx, Def::Automation(automation_id))?;
    let action = action_add(tx, shelf.under(automation.project_id), name, "")?;
    placement_add(tx, automation_id, action.id)
}

/// **Make an empty action and put it in on a line** — [`placement_add_new`] for a picture already
/// drawn. Where it goes is decided by the press, before there is anything in it: the reader goes on
/// to build the action and comes back to find it standing where they meant it to (`AMB-D-956`).
///
/// It is one act because half of it is a picture nobody asked for: an action in the library that
/// nothing stands on, or a line running past a spot that was meant to be on it.
///
/// Which library it lands in is the dialog's answer ([`ActionShelf`]), not this op's: an action
/// made here is an ordinary action, and where an ordinary action is kept is a choice its author
/// makes.
pub fn placement_insert_new(
    tx: &WriteTx<'_>,
    edge_id: i64,
    shelf: ActionShelf,
    name: &str,
) -> Result<AutomationPlacement> {
    let edge = live_edge(tx, edge_id)?;
    if edge.owner_kind != AutomationPictureOwner::Action {
        let automation_id = box_picture(tx, AutomationPictureOwner::Automation, edge.from_id)?;
        let project_id = live_automation(tx, automation_id)?.project_id;
        let action = action_add(tx, shelf.under(project_id), name, "")?;
        let placement = placement_add(tx, automation_id, action.id)?;
        splice_onto_edge(tx, &edge, placement.id)?;
        return Ok(placement);
    }
    Err(Error::invalid(
        "that line is drawn inside an action, and what stands on one is a step — put a step in there \
         instead",
    ))
}

/// Reorder a placement within its automation. It moves it in the lists alone — the picture is walked
/// from the entry along the edges, and no order here reaches it.
pub fn placement_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationPlacement> {
    let before = live_placement(tx, id)?;
    not_under_a_run(tx, Def::Automation(before.automation_id))?;
    let sibs = read::automation_placement_siblings(tx.conn(), before.automation_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_placement(&before), record::automation_placement(&after))?;
    Ok(after)
}

/// Take a placement off its automation, with the answers written on it and every edge and wire
/// naming it at either end. The action it stood on is untouched.
///
/// **Taking the entry off clears it.** An automation under construction has to be able to lose any
/// placement, and refusing here would strand whichever one was named the entry first.
pub fn placement_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let placement = live_placement(tx, id)?;
    not_under_a_run(tx, Def::Automation(placement.automation_id))?;
    let automation = live_automation(tx, placement.automation_id)?;
    // `entry_placement_id` is `RESTRICT`, and that check bites at the statement rather than at the
    // commit — so the reference is dropped before the row it names, not after.
    if automation.entry_placement_id == Some(id) {
        set_entry(tx, automation.id, None)?;
    }
    delete_placement_row(tx, id)
}

/// One placement and everything hanging off it, with no word about the entry — what [`delete`] walks,
/// where the entry has already been dropped and the automation itself is going anyway.
fn delete_placement_row(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    delete_lines_naming_box(tx, AutomationPictureOwner::Automation, id)?;
    delete_cfgs(tx, AutomationCfgOwner::Placement, id)?;
    tx.delete_record("automation_placement", id)?;
    Ok(())
}

// ───────────────────────────── steps ─────────────────────────────

/// What a new step is made of. `show_history` starts on — a step is handed the run's story so far unless
/// somebody says otherwise — while `interactive` and `report_to_task` start off.
#[derive(Clone, Debug)]
pub struct NewStep {
    pub name: String,
    pub prompt: String,
    pub agent: String,
    pub model: Option<String>,
    pub interactive: bool,
    /// The name of the setting or the input the working folder is taken from — a name, not a path.
    pub work_dir_ref: Option<String>,
    pub report_to_task: bool,
    pub show_history: bool,
}

impl NewStep {
    /// A step with the three flags where they start.
    pub fn new(name: &str, prompt: &str, agent: &str) -> NewStep {
        NewStep {
            name: name.to_string(),
            prompt: prompt.to_string(),
            agent: agent.to_string(),
            model: None,
            interactive: false,
            work_dir_ref: None,
            report_to_task: false,
            show_history: true,
        }
    }
}

/// The library action a placement may stand on has to be within reach of the automation's project: its
/// own project's library, or the device's. Another project's is refused — the prompt would be read
/// across a boundary that is there to keep one project's context out of another's.
fn checked_action(tx: &WriteTx<'_>, automation: &Automation, action_id: i64) -> Result<()> {
    let action = live_action(tx, action_id)?;
    match action.project_id {
        None => Ok(()),
        Some(p) if p == automation.project_id => Ok(()),
        Some(_) => Err(Error::invalid(format!(
            "action '{action_id}' is in another project's library — an automation reaches its own \
             project's library and the device's, and no other"
        ))),
    }
}

/// Add a step to a library action. It is born carrying the two ways out every declarer has.
pub fn step_add(tx: &WriteTx<'_>, action_id: i64, new: NewStep) -> Result<AutomationStep> {
    live_action(tx, action_id)?;
    not_under_a_run(tx, Def::Action(action_id))?;
    let name = checked_name("step", &new.name)?;
    let agent = checked_name("agent", &new.agent)?;
    let sibs = read::automation_action_step_siblings(tx.conn(), action_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_action_step")?;
    let step = AutomationStep {
        id,
        action_id,
        name,
        prompt: new.prompt,
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
    emit_create(tx, record::automation_action_step(&step))?;
    born_with_exits(tx, AutomationOwner::Step, id)?;
    Ok(step)
}

/// **Put a box in on a line**, which is the one road by which a box joins a picture already drawn: the
/// way out that was pressed comes to point at the new box, and the new box goes on to the target that
/// way out named — so nothing that was decided is lost, and the new box is never left with nothing
/// pointing at it. A box nothing points at is a box no run reaches, which is why there is no "add at the
/// end".
///
/// The limit on how often an edge may be taken follows the pressed edge where that one carries a limit.
/// A way out that closes or stops the run carries none, so both edges take the standing limit
/// ([`crate::model::DEFAULT_MAX_TIMES`]) — a box to go on to is what makes a limit mean anything.
fn splice_onto_edge(tx: &WriteTx<'_>, edge: &AutomationEdge, new_box: i64) -> Result<()> {
    let onward = match edge.ends {
        AutomationEnds::Go => EdgeTarget::Go(
            edge.to_id.ok_or_else(|| Error::invalid("the way out goes on to nothing"))?,
        ),
        AutomationEnds::Exit => EdgeTarget::Exit(match edge.exit_to_id {
            Some(id) => live_exit(tx, id)?.name,
            None => None,
        }),
        AutomationEnds::Done => EdgeTarget::Done,
        AutomationEnds::Halt => EdgeTarget::Halt,
    };
    let carried = match edge.ends {
        AutomationEnds::Go => edge.max_times,
        _ => Some(DEFAULT_MAX_TIMES),
    };
    edge_update(tx, edge.id, Some(EdgeTarget::Go(new_box)), Some(carried))?;
    let onward_limit = match onward {
        EdgeTarget::Go(_) => Some(DEFAULT_MAX_TIMES),
        _ => None,
    };
    edge_add(tx, edge.owner_kind, new_box, None, onward, onward_limit)?;
    Ok(())
}

/// **Put a step in on a line** inside one action — [`splice_onto_edge`] with the step written first.
///
/// **It is one transaction.** The step, the ways out and the inputs it is written with, and the two
/// edges are one act as far as a reader is concerned: a press that leaves a step behind with the line
/// running past it is a picture nobody asked for.
///
/// `exits` and `inputs` are what the dialog took.
pub fn step_insert(
    tx: &WriteTx<'_>,
    edge_id: i64,
    new: NewStep,
    exits: &[String],
    inputs: &[(String, AutomationPortKind, bool)],
) -> Result<AutomationStep> {
    let edge = live_edge(tx, edge_id)?;
    if edge.owner_kind != AutomationPictureOwner::Action {
        return Err(Error::invalid(
            "that line is drawn on an automation, and what stands on one is a placement — put an \
             action in there instead",
        ));
    }
    let action_id = box_picture(tx, AutomationPictureOwner::Action, edge.from_id)?;
    let step = step_add(tx, action_id, new)?;
    for name in exits {
        exit_add(tx, AutomationOwner::Step, step.id, Some(name))?;
    }
    for (name, kind, required) in inputs {
        port_add(
            tx,
            AutomationPortOwner::Step,
            step.id,
            AutomationPortDirection::In,
            name,
            *kind,
            *required,
        )?;
    }
    splice_onto_edge(tx, &edge, step.id)?;
    Ok(step)
}

/// Change a step. Only the `Some` fields are written.
#[allow(clippy::too_many_arguments)]
pub fn step_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    prompt: Option<&str>,
    agent: Option<&str>,
    model: Option<Option<&str>>,
    interactive: Option<bool>,
    work_dir_ref: Option<Option<&str>>,
    report_to_task: Option<bool>,
    show_history: Option<bool>,
) -> Result<AutomationStep> {
    let before = live_step(tx, id)?;
    not_under_a_run(tx, Def::Action(before.action_id))?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = checked_name("step", name)?;
    }
    if let Some(prompt) = prompt {
        after.prompt = prompt.to_string();
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
    after.updated_at = Timestamp::now();
    emit_update(
        tx,
        record::automation_action_step(&before),
        record::automation_action_step(&after),
    )?;
    Ok(after)
}

/// Reorder a step within its action. It moves the step in the lists alone — the picture is walked from
/// the entry along the edges, and no order here reaches it.
pub fn step_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationStep> {
    let before = live_step(tx, id)?;
    not_under_a_run(tx, Def::Action(before.action_id))?;
    let sibs = read::automation_action_step_siblings(tx.conn(), before.action_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(
        tx,
        record::automation_action_step(&before),
        record::automation_action_step(&after),
    )?;
    Ok(after)
}

/// Delete a step, with its declarations and every edge and wire naming it at either end.
///
/// **Deleting the entry clears it.** An action under construction has to be able to lose any step, and
/// refusing here would strand whichever one was named the entry first; an action left without one is
/// refused at the launch check, where a person is about to be let down by it.
pub fn step_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let step = live_step(tx, id)?;
    not_under_a_run(tx, Def::Action(step.action_id))?;
    let action = live_action(tx, step.action_id)?;
    // `entry_step_id` is `RESTRICT`, and that check bites at the statement rather than at the commit —
    // so the reference is dropped before the row it names, not after.
    if action.entry_step_id == Some(id) {
        action_set_entry(tx, action.id, None)?;
    }
    delete_step_row(tx, id)
}

/// One step and everything hanging off it, with no word about the entry — what [`action_delete`] walks,
/// where the entry has already been dropped and the action itself is going anyway.
fn delete_step_row(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    delete_lines_naming_box(tx, AutomationPictureOwner::Action, id)?;
    delete_declarations(tx, AutomationOwner::Step, AutomationPortOwner::Step, id)?;
    tx.delete_record("automation_action_step", id)?;
    Ok(())
}

// ───────────────────────────── ways out, ports, settings ─────────────────────────────

/// The owner a way out or a port may be declared on. Both kinds declare their own, so this is the
/// existence check and nothing more.
fn checked_declarer(tx: &WriteTx<'_>, owner_kind: AutomationOwner, owner_id: i64) -> Result<()> {
    match owner_kind {
        AutomationOwner::Action => {
            live_action(tx, owner_id)?;
        }
        AutomationOwner::Step => {
            live_step(tx, owner_id)?;
        }
    }
    Ok(())
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
    not_under_a_run(tx, def_of_declarer(tx, owner_kind, owner_id)?)?;
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
/// **Whatever hangs on it stays.** Edges and wires key a way out by its row, not its name
/// (`AMB-D-961`), so a rename changes what the picture says and nothing about how it is joined.
///
/// [`ERROR_EXIT`]'s row is refused at both ends: it may not be renamed, and no other may take its name.
pub fn exit_rename(tx: &WriteTx<'_>, id: i64, name: Option<&str>) -> Result<AutomationExit> {
    let before = live_exit(tx, id)?;
    not_under_a_run(tx, def_of_declarer(tx, before.owner_kind, before.owner_id)?)?;
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
    not_under_a_run(tx, def_of_declarer(tx, before.owner_kind, before.owner_id)?)?;
    let sibs =
        read::automation_exit_siblings(tx.conn(), before.owner_kind, before.owner_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_exit(&before), record::automation_exit(&after))?;
    Ok(after)
}

/// Delete a way out, with the outputs declared on it and the lines keyed to it ([`delete_exit_row`]).
/// [`ERROR_EXIT`]'s row is refused — every step carries it, and an edge may hang on it whether or not
/// anyone wrote one.
pub fn exit_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let exit = live_exit(tx, id)?;
    not_under_a_run(tx, def_of_declarer(tx, exit.owner_kind, exit.owner_id)?)?;
    if exit.name.as_deref() == Some(ERROR_EXIT) {
        return Err(Error::invalid(
            "the error way out cannot be deleted — every step and every action carries one",
        ));
    }
    delete_exit_row(tx, id)
}

/// Declare a port: what a step or an action takes in (`In`, on that step or action) or what a way out
/// hands on (`Out`, on that way out).
///
/// The two directions hang on different owners, and neither is sayable on the other's: an input belongs
/// to the box that reads it, while an output belongs to the way out that produced it, which is what
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
    not_under_a_run(tx, def_of_port_owner(tx, owner_kind, owner_id)?)?;
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
    not_under_a_run(tx, def_of_port_owner(tx, before.owner_kind, before.owner_id)?)?;
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
    not_under_a_run(tx, def_of_port_owner(tx, before.owner_kind, before.owner_id)?)?;
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
    let port = live_port(tx, id)?;
    not_under_a_run(tx, def_of_port_owner(tx, port.owner_kind, port.owner_id)?)?;
    tx.delete_record("automation_port", id)?;
    Ok(())
}

/// Declare a setting on a library action: its name, what kind of thing it is, and whether it has to be
/// answered. `options` is the choice list, as JSON, and belongs to `kind = Choice` alone.
///
/// The row is the declaration by itself, so its `value` stays empty; each placement of the action
/// answers it on a row of its own ([`cfg_set`]).
pub fn cfg_add(
    tx: &WriteTx<'_>,
    action_id: i64,
    name: &str,
    kind: AutomationCfgKind,
    required: bool,
    options: Option<&str>,
) -> Result<AutomationCfg> {
    live_action(tx, action_id)?;
    not_under_a_run(tx, Def::Action(action_id))?;
    let name = checked_name("setting", name)?;
    checked_options(kind, options)?;
    if read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Action, action_id, &name)?.is_some()
    {
        return Err(Error::invalid(format!("a setting called '{name}' is already declared here")));
    }
    write_cfg_row(
        tx,
        AutomationCfgOwner::Action,
        action_id,
        name,
        kind,
        required,
        options.map(str::to_string),
        None,
    )
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
    owner_kind: AutomationCfgOwner,
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
    not_under_a_run(tx, def_of_cfg_owner(tx, before.owner_kind, before.owner_id)?)?;
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

/// **Answer a setting on one placement.** The answer is JSON, and `None` clears it.
///
/// The declaration is the action's and carries no answer, so the placement takes a row of its own under
/// the same name, born from that declaration, and the answer goes there. That is what lets the same
/// action be placed twice on one automation with two different answers.
pub fn cfg_set(
    tx: &WriteTx<'_>,
    placement_id: i64,
    name: &str,
    value: Option<&str>,
) -> Result<AutomationCfg> {
    let placement = live_placement(tx, placement_id)?;
    not_under_a_run(tx, Def::Automation(placement.automation_id))?;
    let name = checked_name("setting", name)?;
    if let Some(before) =
        read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, placement.id, &name)?
    {
        let mut after = before.clone();
        after.value = value.map(str::to_string);
        after.updated_at = Timestamp::now();
        emit_update(tx, record::automation_cfg(&before), record::automation_cfg(&after))?;
        return Ok(after);
    }
    let declared =
        read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Action, placement.action_id, &name)?
            .ok_or_else(|| Error::not_found(format!("no setting called '{name}' is declared here")))?;
    write_cfg_row(
        tx,
        AutomationCfgOwner::Placement,
        placement.id,
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
    not_under_a_run(tx, def_of_cfg_owner(tx, before.owner_kind, before.owner_id)?)?;
    let sibs = read::automation_cfg_siblings(tx.conn(), before.owner_kind, before.owner_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_cfg(&before), record::automation_cfg(&after))?;
    Ok(after)
}

/// Delete a setting — the action's declaration, or one placement's answer to it.
pub fn cfg_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let cfg = live_cfg(tx, id)?;
    not_under_a_run(tx, def_of_cfg_owner(tx, cfg.owner_kind, cfg.owner_id)?)?;
    tx.delete_record("automation_cfg", id)?;
    Ok(())
}

// ───────────────────────────── what runs after what ─────────────────────────────

/// The way out a box leaves through, resolved the way an edge or a wire names it: by name, against
/// whichever of the two declares the ways out on that picture ([`box_declarer`]).
fn box_exit(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    box_id: i64,
    exit_name: Option<&str>,
) -> Result<AutomationExit> {
    let (declarer, declarer_id) = box_declarer(tx, owner_kind, box_id)?;
    read::automation_exit_by_name(tx.conn(), declarer, declarer_id, exit_name)?.ok_or_else(|| {
        let what = box_word(owner_kind);
        match exit_name {
            Some(n) => Error::not_found(format!("{what} '{box_id}' has no way out called '{n}'")),
            None => Error::not_found(format!("{what} '{box_id}' has no unnamed way out")),
        }
    })
}

/// What an edge does once its way out is taken.
#[derive(Clone, Debug)]
pub enum EdgeTarget {
    /// Open the next box.
    Go(i64),
    /// Leave the action this picture is inside, by the way out it declares under this name — `None`
    /// being its unnamed one. An action's picture only: an automation's has nothing outside it. The
    /// name is how the way out is said; what the edge keeps is its row ([`AutomationEdge::exit_to_id`]).
    Exit(Option<String>),
    /// Close the run.
    Done,
    /// Stop the run and call a person.
    Halt,
}

impl EdgeTarget {
    fn parts(self) -> (AutomationEnds, Option<i64>, Option<String>) {
        match self {
            EdgeTarget::Go(to) => (AutomationEnds::Go, Some(to), None),
            EdgeTarget::Exit(name) => (AutomationEnds::Exit, None, name),
            EdgeTarget::Done => (AutomationEnds::Done, None, None),
            EdgeTarget::Halt => (AutomationEnds::Halt, None, None),
        }
    }
}

/// The way out of the action a picture is inside that an `Exit` edge returns to, resolved by name
/// against that action's own declarations.
///
/// It refuses an automation's picture before it refuses a name: an automation is not inside anything, so
/// there is no way out of it to return to and the edge that says otherwise is the caller's mistake.
fn returning_exit(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    owner_id: i64,
    exit_to: Option<&str>,
) -> Result<AutomationExit> {
    if owner_kind != AutomationPictureOwner::Action {
        return Err(Error::invalid(
            "only a picture inside an action has a way out to return to — an automation's picture ends \
             the run instead",
        ));
    }
    read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, owner_id, exit_to)?.ok_or_else(
        || match exit_to {
            Some(n) => Error::not_found(format!("this action has no way out called '{n}'")),
            None => Error::not_found("this action has no unnamed way out".to_string()),
        },
    )
}

/// A limit counts something that can be taken twice, and it counts at least once.
fn checked_max_times(max_times: Option<i64>, ends: AutomationEnds) -> Result<()> {
    if max_times.is_some() && ends != AutomationEnds::Go {
        return Err(Error::invalid(
            "a limit counts how often an edge is taken, and an edge that leaves the action, closes the \
             run or stops it is taken once",
        ));
    }
    if let Some(n) = max_times {
        if n < 1 {
            return Err(Error::invalid("a limit of how often an edge may be taken is at least 1"));
        }
    }
    Ok(())
}

/// Say what happens after one box leaves through one way out.
///
/// `max_times` caps how often the edge may be taken **for one task**; it is counted afresh at the next
/// task, so it stops a loop that never converges without stopping a review that goes round three times.
/// `None` is no limit, which is the right answer for an edge into a box that takes a fresh task, since
/// the count would restart there anyway. It is carried only by `Go`: an edge that closes or stops the
/// run is taken once and has nothing to count. On a `Go` edge the caller's `None` means no limit, and
/// [`crate::model::DEFAULT_MAX_TIMES`] is what the surface above puts there when nobody said.
///
/// **One way out decides one thing**, so a second edge on the same way out is refused rather than
/// leaving the run to pick between them.
///
/// [`EdgeTarget::Exit`] is an action's picture's alone, and the way out it names has to be one the
/// action itself declares — that pair is the whole of what joins an action's declarations to the steps
/// inside it.
pub fn edge_add(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    from_id: i64,
    exit_name: Option<&str>,
    target: EdgeTarget,
    max_times: Option<i64>,
) -> Result<AutomationEdge> {
    let owner_id = box_picture(tx, owner_kind, from_id)?;
    not_under_a_run(tx, def_of_picture(owner_kind, owner_id))?;
    let exit = box_exit(tx, owner_kind, from_id, exit_name)?;
    let (ends, to_id, exit_to) = target.parts();
    if let Some(to_id) = to_id {
        if box_picture(tx, owner_kind, to_id)? != owner_id {
            return Err(Error::invalid(format!(
                "{} '{to_id}' is drawn on another picture — an edge stays inside one",
                box_word(owner_kind)
            )));
        }
    }
    let exit_to_id = match ends {
        AutomationEnds::Exit => Some(returning_exit(tx, owner_kind, owner_id, exit_to.as_deref())?.id),
        _ => None,
    };
    checked_max_times(max_times, ends)?;
    if read::automation_edge_for_exit(tx.conn(), owner_kind, from_id, exit.id)?.is_some() {
        return Err(Error::invalid(match exit_name {
            Some(n) => format!("'{n}' already says what happens after it"),
            None => "the unnamed way out already says what happens after it".to_string(),
        }));
    }
    let sibs = read::automation_edge_siblings(tx.conn(), owner_kind, owner_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_edge")?;
    let edge = AutomationEdge {
        id,
        owner_kind,
        owner_id,
        from_id,
        exit_id: exit.id,
        to_id,
        ends,
        exit_to_id,
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
    not_under_a_run(tx, def_of_picture(before.owner_kind, before.owner_id))?;
    let mut after = before.clone();
    if let Some(target) = target {
        let (ends, to_id, exit_to) = target.parts();
        if let Some(to_id) = to_id {
            if box_picture(tx, before.owner_kind, to_id)? != before.owner_id {
                return Err(Error::invalid(format!(
                    "{} '{to_id}' is drawn on another picture — an edge stays inside one",
                    box_word(before.owner_kind)
                )));
            }
        }
        after.exit_to_id = match ends {
            AutomationEnds::Exit => {
                Some(returning_exit(tx, before.owner_kind, before.owner_id, exit_to.as_deref())?.id)
            }
            _ => None,
        };
        after.ends = ends;
        after.to_id = to_id;
    }
    if let Some(max_times) = max_times {
        after.max_times = max_times;
    }
    checked_max_times(after.max_times, after.ends)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_edge(&before), record::automation_edge(&after))?;
    Ok(after)
}

/// Delete an edge. The way out is then read as saying nothing, which for the error one means stopping
/// the run and calling a person, and for any other means the run has nowhere to go.
pub fn edge_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let edge = live_edge(tx, id)?;
    not_under_a_run(tx, def_of_picture(edge.owner_kind, edge.owner_id))?;
    tx.delete_record("automation_edge", id)?;
    Ok(())
}

/// The picture a wire is drawn on, read off whichever of its ends is a box — since one of them may be
/// [`ACTION_BOUNDARY`], which belongs to no picture until the other end says which.
///
/// It refuses a boundary on an automation's picture, and a wire with a boundary at both ends: the first
/// has nothing outside it to reach, and the second joins the action to itself.
fn wire_picture(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    from_id: i64,
    to_id: i64,
) -> Result<i64> {
    if from_id != ACTION_BOUNDARY && to_id != ACTION_BOUNDARY {
        let owner_id = box_picture(tx, owner_kind, from_id)?;
        if box_picture(tx, owner_kind, to_id)? != owner_id {
            return Err(Error::invalid("a wire joins two boxes of one picture — these are on two"));
        }
        return Ok(owner_id);
    }
    if owner_kind != AutomationPictureOwner::Action {
        return Err(Error::invalid(
            "an automation's picture has no boundary to reach across — every wire on it joins two \
             placements",
        ));
    }
    if from_id == ACTION_BOUNDARY && to_id == ACTION_BOUNDARY {
        return Err(Error::invalid(
            "a wire from the action to itself hands nothing on — one of its ends is a step",
        ));
    }
    box_picture(tx, owner_kind, if from_id == ACTION_BOUNDARY { to_id } else { from_id })
}

/// Join what one way out hands on to what a later box takes in.
///
/// Both ends are checked against what is declared, and the two have to carry the same kind of thing — a
/// file into a file, a value into a value. Several wires may land on one input: which of them the step
/// actually reads is the run's to decide, from whichever wrote last in the stretch it is in.
///
/// **Inside an action, either end may be [`ACTION_BOUNDARY`]** — the action itself — and that is how
/// what it declares reaches what is inside it:
///
/// - out of the boundary comes an input the action declares, so `from_exit_name` has to be `None`: an
///   action's inputs hang on the action and not on a way out of it.
/// - into the boundary goes an output declared on a way out of the action. Which way out is not asked
///   for — it is the one the wire's source returns to, so the `Exit` edge has to be drawn first and the
///   refusal says so. That is what keeps a value from being handed out of a way the run never left by.
///
/// **Drawing the same wire twice answers the one already drawn** rather than writing a second row.
pub fn wire_add(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    from_id: i64,
    from_exit_name: Option<&str>,
    from_port_name: &str,
    to_id: i64,
    to_port_name: &str,
) -> Result<AutomationWire> {
    let owner_id = wire_picture(tx, owner_kind, from_id, to_id)?;
    not_under_a_run(tx, def_of_picture(owner_kind, owner_id))?;
    let mut from_exit_id = None;
    let out = if from_id == ACTION_BOUNDARY {
        if from_exit_name.is_some() {
            return Err(Error::invalid(
                "an action hands its inputs on from itself, not from one of its ways out",
            ));
        }
        read::automation_port_by_name(
            tx.conn(),
            AutomationPortOwner::Action,
            owner_id,
            AutomationPortDirection::In,
            from_port_name,
        )?
        .ok_or_else(|| Error::not_found(format!("this action takes in no '{from_port_name}'")))?
    } else {
        let exit = box_exit(tx, owner_kind, from_id, from_exit_name)?;
        from_exit_id = Some(exit.id);
        read::automation_port_by_name(
            tx.conn(),
            AutomationPortOwner::Exit,
            exit.id,
            AutomationPortDirection::Out,
            from_port_name,
        )?
        .ok_or_else(|| {
            Error::not_found(format!(
                "that way out of {} '{from_id}' hands on no '{from_port_name}'",
                box_word(owner_kind)
            ))
        })?
    };
    let into = if to_id == ACTION_BOUNDARY {
        let returns_by = match from_exit_id {
            Some(exit_id) => read::automation_edge_for_exit(tx.conn(), owner_kind, from_id, exit_id)?,
            None => None,
        }
        .filter(|edge| edge.ends == AutomationEnds::Exit)
            .ok_or_else(|| {
                Error::invalid(format!(
                    "that way out of step '{from_id}' does not leave the action — say which way out of \
                     the action it returns to before handing anything out of it"
                ))
            })?;
        let exit_id = returns_by
            .exit_to_id
            .ok_or_else(|| Error::invalid("that line leaves the action by no way out"))?;
        let exit = live_exit(tx, exit_id)?;
        read::automation_port_by_name(
            tx.conn(),
            AutomationPortOwner::Exit,
            exit.id,
            AutomationPortDirection::Out,
            to_port_name,
        )?
        .ok_or_else(|| {
            Error::not_found(format!(
                "that way out of this action hands on no '{to_port_name}'"
            ))
        })?
    } else {
        let (to_owner, to_owner_id) = box_port_declarer(tx, owner_kind, to_id)?;
        read::automation_port_by_name(
            tx.conn(),
            to_owner,
            to_owner_id,
            AutomationPortDirection::In,
            to_port_name,
        )?
        .ok_or_else(|| {
            Error::not_found(format!(
                "{} '{to_id}' takes in no '{to_port_name}'",
                box_word(owner_kind)
            ))
        })?
    };
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
        owner_kind,
        from_id,
        from_exit_id,
        from_port_name,
        to_id,
        to_port_name,
    )? {
        return Ok(drawn);
    }
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_wire")?;
    let wire = AutomationWire {
        id,
        owner_kind,
        owner_id,
        from_id,
        from_exit_id,
        from_port_name: from_port_name.to_string(),
        to_id,
        to_port_name: to_port_name.to_string(),
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_wire(&wire))?;
    Ok(wire)
}

/// Delete a wire. The box then reads nothing on that input unless another wire lands on it.
pub fn wire_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let wire = live_wire(tx, id)?;
    not_under_a_run(tx, def_of_picture(wire.owner_kind, wire.owner_id))?;
    tx.delete_record("automation_wire", id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{exit_id, mk_project, with_tx};

    /// The edge hanging on one way out of one box, the way out said by its name — what a test reads
    /// where the store keys it (`AMB-D-961`).
    fn edge_on(
        tx: &WriteTx<'_>,
        owner_kind: AutomationPictureOwner,
        box_id: i64,
        exit: Option<&str>,
    ) -> Result<Option<AutomationEdge>> {
        let exit = box_exit(tx, owner_kind, box_id, exit)?;
        Ok(read::automation_edge_for_exit(tx.conn(), owner_kind, box_id, exit.id)?)
    }

    fn mk_automation(tx: &WriteTx<'_>) -> Automation {
        let project = mk_project(tx, "amenbo");
        add(tx, project, NewAutomation { name: "1件やりきる".into(), ..Default::default() })
            .expect("add automation")
    }

    /// One library action holding one step, and a placement of it — the shape nearly every test here
    /// starts from, and the shape every v52 step was folded into.
    fn mk_placed(
        tx: &WriteTx<'_>,
        automation: &Automation,
        name: &str,
    ) -> (AutomationAction, AutomationPlacement) {
        let action = action_from_prompt(
            tx,
            Some(automation.project_id),
            NewStep::new(name, "do it", "claude"),
            &[],
            &[],
        )
        .expect("write the action");
        let placement = placement_add(tx, automation.id, action.id).expect("place it");
        (action, placement)
    }

    /// The one step a freshly written action holds.
    fn only_step(tx: &WriteTx<'_>, action: &AutomationAction) -> AutomationStep {
        live_step(tx, action.entry_step_id.expect("an entry step")).expect("read the step")
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
            let (action, _) = mk_placed(tx, &automation, "実装する");
            let step = only_step(tx, &action);
            assert_eq!(
                exit_names(tx, AutomationOwner::Step, step.id),
                vec![None, Some(ERROR_EXIT.to_string())],
                "both are written at birth, the error one last so it sits at the bottom of the list",
            );
            assert_eq!(
                exit_names(tx, AutomationOwner::Action, action.id),
                vec![None, Some(ERROR_EXIT.to_string())],
                "and the action carries the pair a placement of it is left by",
            );
        });
    }

    #[test]
    fn an_action_written_from_a_prompt_is_joined_to_the_step_inside_it() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let action = action_from_prompt(
                tx,
                Some(project),
                NewStep::new("点検する", "見る", "claude"),
                &["直すところがある".to_string()],
                &[("差分".to_string(), AutomationPortKind::File, true)],
            )
            .expect("write the action");
            let step = only_step(tx, &action);

            for name in [None, Some(ERROR_EXIT), Some("直すところがある")] {
                let edge = edge_on(tx,
                    AutomationPictureOwner::Action,
                    step.id,
                    name,
                )
                .expect("read the edge")
                .expect("every way out of the step leaves the action");
                assert_eq!(edge.ends, AutomationEnds::Exit);
                assert_eq!(
                    edge.exit_to_id,
                    Some(exit_id(tx, AutomationOwner::Action, action.id, name)),
                    "and it returns to the action's way out of that name",
                );
            }

            let wires = read::automation_wires_of(tx.conn(), AutomationPictureOwner::Action, action.id)
                .expect("read the wires");
            assert_eq!(wires.len(), 1);
            assert_eq!(wires[0].from_id, ACTION_BOUNDARY, "out of the action itself");
            assert_eq!(wires[0].from_exit_id, None);
            assert_eq!(wires[0].from_port_name, "差分");
            assert_eq!(wires[0].to_id, step.id);
            assert_eq!(wires[0].to_port_name, "差分");
        });
    }

    #[test]
    fn an_automations_picture_has_no_way_out_of_itself_to_return_to() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, placement) = mk_placed(tx, &automation, "取る");
            let refused = edge_add(
                tx,
                AutomationPictureOwner::Automation,
                placement.id,
                Some(ERROR_EXIT),
                EdgeTarget::Exit(None),
                None,
            )
            .expect_err("an automation is inside nothing");
            assert!(format!("{refused}").contains("ends the run"), "{refused}");
        });
    }

    #[test]
    fn a_line_out_of_an_action_names_a_way_out_that_action_declares() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, _) = mk_placed(tx, &automation, "取る");
            let (action, _) = mk_placed(tx, &automation, "点検する");
            let step = only_step(tx, &action);
            let refused = edge_add(
                tx,
                AutomationPictureOwner::Action,
                step.id,
                Some(ERROR_EXIT),
                EdgeTarget::Exit(Some("書いていない".to_string())),
                None,
            )
            .expect_err("the action declares no such way out");
            assert!(format!("{refused}").contains("書いていない"), "{refused}");
        });
    }

    #[test]
    fn what_a_step_hands_on_reaches_the_way_out_of_the_action_it_leaves_by() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "点検する");
            let step = only_step(tx, &action);
            // The output both sides declare: on the step's unnamed way out, and on the action's.
            for (owner, owner_id) in
                [(AutomationOwner::Step, step.id), (AutomationOwner::Action, action.id)]
            {
                let exit = read::automation_exit_by_name(tx.conn(), owner, owner_id, None)
                    .expect("read the way out")
                    .expect("the unnamed way out");
                port_add(
                    tx,
                    AutomationPortOwner::Exit,
                    exit.id,
                    AutomationPortDirection::Out,
                    "指摘",
                    AutomationPortKind::File,
                    true,
                )
                .expect("declare the output");
            }

            let wire = wire_add(
                tx,
                AutomationPictureOwner::Action,
                step.id,
                None,
                "指摘",
                ACTION_BOUNDARY,
                "指摘",
            )
            .expect("hand it out of the action");
            assert_eq!(wire.owner_id, action.id, "the picture is read off the end that is a step");
            assert_eq!(wire.to_id, ACTION_BOUNDARY);
        });
    }

    #[test]
    fn nothing_is_handed_out_of_a_way_out_that_does_not_leave_the_action() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "点検する");
            let step = only_step(tx, &action);
            let error = read::automation_exit_by_name(
                tx.conn(),
                AutomationOwner::Step,
                step.id,
                Some(ERROR_EXIT),
            )
            .expect("read the way out")
            .expect("the error way out");
            port_add(
                tx,
                AutomationPortOwner::Exit,
                error.id,
                AutomationPortDirection::Out,
                "言いぶん",
                AutomationPortKind::Value,
                false,
            )
            .expect("declare the output");
            // The error way out of this step leaves the action, so take that line away first.
            let drawn = edge_on(tx,
                AutomationPictureOwner::Action,
                step.id,
                Some(ERROR_EXIT),
            )
            .expect("read the edge")
            .expect("an edge");
            edge_delete(tx, drawn.id).expect("take the line away");

            let refused = wire_add(
                tx,
                AutomationPictureOwner::Action,
                step.id,
                Some(ERROR_EXIT),
                "言いぶん",
                ACTION_BOUNDARY,
                "言いぶん",
            )
            .expect_err("that way out goes nowhere out of the action");
            assert!(format!("{refused}").contains("does not leave the action"), "{refused}");
        });
    }

    #[test]
    fn an_automations_picture_has_no_boundary_to_wire_across() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, placement) = mk_placed(tx, &automation, "取る");
            let refused = wire_add(
                tx,
                AutomationPictureOwner::Automation,
                ACTION_BOUNDARY,
                None,
                "差分",
                placement.id,
                "差分",
            )
            .expect_err("an automation has no outside");
            assert!(format!("{refused}").contains("no boundary"), "{refused}");
        });
    }

    #[test]
    fn an_action_hands_its_inputs_on_from_itself_and_not_from_a_way_out() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let action = action_from_prompt(
                tx,
                Some(project),
                NewStep::new("点検する", "見る", "claude"),
                &[],
                &[("差分".to_string(), AutomationPortKind::File, true)],
            )
            .expect("write the action");
            let step = only_step(tx, &action);
            let refused = wire_add(
                tx,
                AutomationPictureOwner::Action,
                ACTION_BOUNDARY,
                Some(ERROR_EXIT),
                "差分",
                step.id,
                "差分",
            )
            .expect_err("an input hangs on the action");
            assert!(format!("{refused}").contains("not from one of its ways out"), "{refused}");
        });
    }

    /// What an edge says, as a pair a test can read at a glance.
    fn edge_of(
        tx: &WriteTx<'_>,
        owner_kind: AutomationPictureOwner,
        from_id: i64,
        exit: Option<&str>,
    ) -> (AutomationEnds, Option<i64>) {
        let edge = edge_on(tx, owner_kind, from_id, exit)
            .expect("read edge")
            .expect("an edge on that way out");
        (edge.ends, edge.to_id)
    }

    #[test]
    fn an_action_made_at_the_picture_lands_on_the_library_it_was_told_to() {
        with_tx(|tx| {
            let automation = mk_automation(tx);

            let mine = placement_add_new(tx, automation.id, ActionShelf::Project, "下ごしらえ")
                .expect("make it on the project's shelf");
            let shared = placement_add_new(tx, automation.id, ActionShelf::Device, "見直す")
                .expect("make it on the device's shelf");

            assert_eq!(
                live_action(tx, live_placement(tx, mine.id).unwrap().action_id)
                    .unwrap()
                    .project_id,
                Some(automation.project_id),
                "the project's shelf is the automation's own project",
            );
            assert_eq!(
                live_action(tx, live_placement(tx, shared.id).unwrap().action_id)
                    .unwrap()
                    .project_id,
                None,
                "and the device's belongs to no project at all",
            );
            assert!(
                read::automation_edge_ids_naming_box(
                    tx.conn(),
                    AutomationPictureOwner::Automation,
                    mine.id,
                )
                .unwrap()
                .is_empty(),
                "a box put down this way stands on its own — no line was pressed to put it in on",
            );
        });
    }

    #[test]
    fn an_action_made_on_a_line_is_born_empty_and_takes_over_where_the_line_went() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");
            let edge = edge_add(
                tx,
                AutomationPictureOwner::Automation,
                first.id,
                None,
                EdgeTarget::Done,
                None,
            )
            .expect("close the task after it");

            let made = placement_insert_new(tx, edge.id, ActionShelf::Project, "書く")
                .expect("make one on the line");

            let action = live_action(tx, made.action_id).unwrap();
            assert_eq!(action.name, "書く");
            assert_eq!(
                action.entry_step_id, None,
                "nothing is written in it — the inside is built on the action's own screen",
            );
            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Automation, first.id, None),
                (AutomationEnds::Go, Some(made.id)),
                "the pressed way out now opens the new one",
            );
            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Automation, made.id, None),
                (AutomationEnds::Done, None),
                "and the new one goes on to where the line used to",
            );
        });
    }

    #[test]
    fn an_action_put_in_on_a_line_takes_over_where_that_line_went() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");
            let (_, last) = mk_placed(tx, &automation, "見直す");
            let edge = edge_add(
                tx,
                AutomationPictureOwner::Automation,
                first.id,
                None,
                EdgeTarget::Go(last.id),
                Some(3),
            )
            .expect("edge");
            let (action, _) = mk_placed(tx, &automation, "実装する");

            let put = placement_insert(tx, edge.id, action.id).expect("insert");

            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Automation, first.id, None),
                (AutomationEnds::Go, Some(put.id)),
                "the way out that was pressed now points at the new placement",
            );
            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Automation, put.id, None),
                (AutomationEnds::Go, Some(last.id)),
                "and the new placement goes on to where that way out used to reach",
            );
            assert_eq!(
                edge_on(tx,
                    AutomationPictureOwner::Automation,
                    first.id,
                    None
                )
                .unwrap()
                .unwrap()
                .max_times,
                Some(3),
                "the limit the pressed edge carried is the pressed edge's still",
            );
        });
    }

    #[test]
    fn a_step_put_in_on_the_line_out_of_an_action_leaves_by_it_instead() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "取る");
            let first = only_step(tx, &action);
            // The line the new step goes in on is the one the action was written with: its one step
            // leaves the action by its unnamed way out.
            let edge = edge_on(tx,
                AutomationPictureOwner::Action,
                first.id,
                None,
            )
            .expect("read the edge")
            .expect("the action leaves by its unnamed way out");
            assert_eq!(edge.ends, AutomationEnds::Exit);

            let put = step_insert(
                tx,
                edge.id,
                NewStep::new("実装する", "やる", "claude"),
                &["直すところがある".to_string()],
                &[("要件".to_string(), AutomationPortKind::Value, true)],
            )
            .expect("insert");

            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Action, first.id, None),
                (AutomationEnds::Go, Some(put.id))
            );
            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Action, put.id, None),
                (AutomationEnds::Exit, None),
                "leaving the action is what the new step goes on to do",
            );
            assert_eq!(
                edge_on(tx,
                    AutomationPictureOwner::Action,
                    first.id,
                    None
                )
                .unwrap()
                .unwrap()
                .max_times,
                Some(DEFAULT_MAX_TIMES),
                "a way out that left the action carried no limit, and now it needs one",
            );
            assert_eq!(
                exit_names(tx, AutomationOwner::Step, put.id),
                vec![None, Some(ERROR_EXIT.to_string()), Some("直すところがある".to_string())],
                "what the dialog wrote is declared on the step it made",
            );
            let inputs = read::automation_ports_of(
                tx.conn(),
                AutomationPortOwner::Step,
                put.id,
                AutomationPortDirection::In,
            )
            .expect("read inputs");
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].name, "要件");
        });
    }

    #[test]
    fn a_line_on_an_automation_takes_no_step_put_in_on_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, only) = mk_placed(tx, &automation, "取る");
            let edge = edge_add(
                tx,
                AutomationPictureOwner::Automation,
                only.id,
                None,
                EdgeTarget::Done,
                None,
            )
            .expect("edge");
            assert!(
                step_insert(tx, edge.id, NewStep::new("実装する", "やる", "claude"), &[], &[]).is_err(),
                "what stands on an automation is a placement, never a step",
            );
        });
    }

    #[test]
    fn the_error_way_out_is_neither_renamed_nor_deleted() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "実装する");
            let step = only_step(tx, &action);
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
            let (_, one) = mk_placed(tx, &automation, "実装する");
            let (_, two) = mk_placed(tx, &automation, "点検する");
            let edge = edge_add(
                tx,
                AutomationPictureOwner::Automation,
                one.id,
                None,
                EdgeTarget::Go(two.id),
                Some(5),
            )
            .expect("add edge");
            assert_eq!(edge.ends, AutomationEnds::Go);
            assert_eq!(edge.max_times, Some(5));
            assert!(
                edge_add(
                    tx,
                    AutomationPictureOwner::Automation,
                    one.id,
                    None,
                    EdgeTarget::Done,
                    None
                )
                .is_err(),
                "a second edge on the same way out would leave the run to pick",
            );
        });
    }

    #[test]
    fn an_edge_that_closes_the_picture_counts_nothing() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, only) = mk_placed(tx, &automation, "実装する");
            assert!(edge_add(
                tx,
                AutomationPictureOwner::Automation,
                only.id,
                None,
                EdgeTarget::Done,
                Some(3)
            )
            .is_err());
            let edge = edge_add(
                tx,
                AutomationPictureOwner::Automation,
                only.id,
                None,
                EdgeTarget::Done,
                None,
            )
            .expect("add edge");
            assert!(edge_update(tx, edge.id, None, Some(Some(3))).is_err());
        });
    }

    #[test]
    fn an_edge_stays_inside_one_picture() {
        with_tx(|tx| {
            let here = mk_automation(tx);
            let there =
                add(tx, here.project_id, NewAutomation { name: "別".into(), ..Default::default() })
                    .expect("add automation");
            let (_, from) = mk_placed(tx, &here, "実装する");
            let (_, to) = mk_placed(tx, &there, "点検する");
            assert!(edge_add(
                tx,
                AutomationPictureOwner::Automation,
                from.id,
                None,
                EdgeTarget::Go(to.id),
                None
            )
            .is_err());
        });
    }

    #[test]
    fn an_edge_names_a_way_out_that_is_declared() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placement) = mk_placed(tx, &automation, "点検する");
            assert!(
                edge_add(
                    tx,
                    AutomationPictureOwner::Automation,
                    placement.id,
                    Some("直すところがある"),
                    EdgeTarget::Done,
                    None
                )
                .is_err(),
                "nothing declares it yet",
            );
            exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある"))
                .expect("add exit");
            edge_add(
                tx,
                AutomationPictureOwner::Automation,
                placement.id,
                Some("直すところがある"),
                EdgeTarget::Done,
                None,
            )
            .expect("add edge");
        });
    }

    #[test]
    fn a_wire_joins_two_ports_of_one_kind() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (from_action, from) = mk_placed(tx, &automation, "実装する");
            let (to_action, to) = mk_placed(tx, &automation, "点検する");
            let exit = read::automation_exit_by_name(
                tx.conn(),
                AutomationOwner::Action,
                from_action.id,
                None,
            )
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
                AutomationPortOwner::Action,
                to_action.id,
                AutomationPortDirection::In,
                "差分",
                AutomationPortKind::Value,
                true,
            )
            .expect("declare the input");
            assert!(
                wire_add(
                    tx,
                    AutomationPictureOwner::Automation,
                    from.id,
                    None,
                    "差分",
                    to.id,
                    "差分"
                )
                .is_err(),
                "a file does not go into a value",
            );
            let into = read::automation_port_by_name(
                tx.conn(),
                AutomationPortOwner::Action,
                to_action.id,
                AutomationPortDirection::In,
                "差分",
            )
            .expect("read")
            .expect("the input");
            port_update(tx, into.id, None, Some(AutomationPortKind::File), None).expect("retype it");
            let wire = wire_add(
                tx,
                AutomationPictureOwner::Automation,
                from.id,
                None,
                "差分",
                to.id,
                "差分",
            )
            .expect("draw the wire");
            let again = wire_add(
                tx,
                AutomationPictureOwner::Automation,
                from.id,
                None,
                "差分",
                to.id,
                "差分",
            )
            .expect("draw it again");
            assert_eq!(wire.id, again.id, "the same wire twice is the one wire");
        });
    }

    #[test]
    fn an_input_hangs_on_the_step_and_an_output_on_the_way_out() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "実装する");
            let step = only_step(tx, &action);
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
    fn a_placement_answers_its_actions_setting_on_a_row_of_its_own() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placement) = mk_placed(tx, &automation, "取る");
            cfg_add(
                tx,
                action.id,
                "どのタスクを取るか",
                AutomationCfgKind::TaskFilter,
                true,
                None,
            )
            .expect("declare the setting");
            let answered =
                cfg_set(tx, placement.id, "どのタスクを取るか", Some("{\"status\":\"todo\"}"))
                    .expect("answer it");
            assert_eq!(answered.owner_kind, AutomationCfgOwner::Placement);
            assert_eq!(answered.owner_id, placement.id);
            assert_eq!(answered.kind, AutomationCfgKind::TaskFilter, "born from the declaration");
            assert!(answered.required);
            let declaration = read::automation_cfg_by_name(
                tx.conn(),
                AutomationCfgOwner::Action,
                action.id,
                "どのタスクを取るか",
            )
            .expect("read")
            .expect("the declaration");
            assert_eq!(declaration.value, None, "the action's row stays a declaration");
            let again =
                cfg_set(tx, placement.id, "どのタスクを取るか", Some("{\"status\":\"blocked\"}"))
                    .expect("answer it again");
            assert_eq!(again.id, answered.id, "the second answer rewrites the first row");
        });
    }

    /// Two placements of one action answer apart — the whole reason the answer is not the action's.
    #[test]
    fn one_action_placed_twice_is_answered_twice() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, here) = mk_placed(tx, &automation, "取る");
            let there = placement_add(tx, automation.id, action.id).expect("place it again");
            cfg_add(tx, action.id, "どこまで", AutomationCfgKind::Text, false, None)
                .expect("declare it");
            cfg_set(tx, here.id, "どこまで", Some("\"全部\"")).expect("answer here");
            cfg_set(tx, there.id, "どこまで", Some("\"半分\"")).expect("answer there");
            let read_back = |placement_id: i64| {
                read::automation_cfg_by_name(
                    tx.conn(),
                    AutomationCfgOwner::Placement,
                    placement_id,
                    "どこまで",
                )
                .expect("read")
                .expect("the answer")
                .value
            };
            assert_eq!(read_back(here.id).as_deref(), Some("\"全部\""));
            assert_eq!(read_back(there.id).as_deref(), Some("\"半分\""));
        });
    }

    #[test]
    fn a_setting_nobody_declared_cannot_be_answered() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, placement) = mk_placed(tx, &automation, "実装する");
            assert!(cfg_set(tx, placement.id, "どのタスクを取るか", Some("{}")).is_err());
        });
    }

    #[test]
    fn a_list_of_choices_belongs_to_a_choice() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "実装する");
            assert!(cfg_add(
                tx,
                action.id,
                "どれ",
                AutomationCfgKind::Text,
                false,
                Some("[\"a\",\"b\"]")
            )
            .is_err());
            cfg_add(
                tx,
                action.id,
                "どれ",
                AutomationCfgKind::Choice,
                false,
                Some("[\"a\",\"b\"]"),
            )
            .expect("a choice carries its list");
        });
    }

    /// **Writing an action from one prompt declares the same names twice over** (`AMB-T-5310`): on the
    /// action, which is what a placement of it is wired by, and on the step, which is what the picture
    /// inside it is drawn with.
    #[test]
    fn an_action_written_from_one_prompt_declares_on_both_halves() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let action = action_from_prompt(
                tx,
                Some(automation.project_id),
                NewStep::new("点検する", "look", "claude"),
                &["直すところがある".to_string()],
                &[("差分".to_string(), AutomationPortKind::File, true)],
            )
            .expect("write the action");
            let step = only_step(tx, &action);
            assert_eq!(step.prompt, "look", "the prompt is the step's");
            assert_eq!(action.entry_step_id, Some(step.id));
            for owner in [AutomationOwner::Action, AutomationOwner::Step] {
                let owner_id = match owner {
                    AutomationOwner::Action => action.id,
                    AutomationOwner::Step => step.id,
                };
                assert_eq!(
                    exit_names(tx, owner, owner_id),
                    vec![
                        None,
                        Some(ERROR_EXIT.to_string()),
                        Some("直すところがある".to_string())
                    ],
                );
                let port_owner = match owner {
                    AutomationOwner::Action => AutomationPortOwner::Action,
                    AutomationOwner::Step => AutomationPortOwner::Step,
                };
                let ins = read::automation_ports_of(
                    tx.conn(),
                    port_owner,
                    owner_id,
                    AutomationPortDirection::In,
                )
                .expect("read");
                assert_eq!(ins.len(), 1);
                assert_eq!(ins[0].name, "差分");
            }
        });
    }

    #[test]
    fn an_automation_reaches_its_own_projects_library_and_the_devices() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let mine = action_add(tx, Some(automation.project_id), "自前", "").expect("add action");
            let shared = action_add(tx, None, "共有", "").expect("add action");
            let other_project = mk_project(tx, "別の企画");
            let theirs = action_add(tx, Some(other_project), "他所", "").expect("add action");
            placement_add(tx, automation.id, mine.id).expect("mine");
            placement_add(tx, automation.id, shared.id).expect("shared");
            assert!(
                placement_add(tx, automation.id, theirs.id).is_err(),
                "another project's prompt is not read across the boundary",
            );
        });
    }

    #[test]
    fn taking_a_placement_off_clears_the_entry_and_takes_the_edges_naming_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, one) = mk_placed(tx, &automation, "取る");
            let (_, two) = mk_placed(tx, &automation, "実装する");
            set_entry(tx, automation.id, Some(one.id)).expect("name the entry");
            edge_add(
                tx,
                AutomationPictureOwner::Automation,
                one.id,
                None,
                EdgeTarget::Go(two.id),
                None,
            )
            .expect("add edge");
            placement_delete(tx, one.id).expect("take it off");
            let after = live_automation(tx, automation.id).expect("read it back");
            assert_eq!(after.entry_placement_id, None);
            assert!(read::automation_edge_ids(
                tx.conn(),
                AutomationPictureOwner::Automation,
                automation.id
            )
            .expect("read")
            .is_empty());
        });
    }

    #[test]
    fn deleting_a_step_clears_the_actions_entry_and_takes_the_edges_naming_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "取る");
            let step = only_step(tx, &action);
            step_delete(tx, step.id).expect("delete the step");
            let after = live_action(tx, action.id).expect("read it back");
            assert_eq!(after.entry_step_id, None);
            assert!(
                read::automation_exit_ids(tx.conn(), AutomationOwner::Step, step.id)
                    .expect("read")
                    .is_empty(),
                "its ways out went with it",
            );
        });
    }

    #[test]
    fn the_entry_is_a_placement_of_this_automation() {
        with_tx(|tx| {
            let here = mk_automation(tx);
            let there =
                add(tx, here.project_id, NewAutomation { name: "別".into(), ..Default::default() })
                    .expect("add automation");
            let (_, elsewhere) = mk_placed(tx, &there, "実装する");
            assert!(set_entry(tx, here.id, Some(elsewhere.id)).is_err());
        });
    }

    #[test]
    fn deleting_an_automation_takes_everything_built_onto_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, one) = mk_placed(tx, &automation, "取る");
            let (_, two) = mk_placed(tx, &automation, "実装する");
            set_entry(tx, automation.id, Some(one.id)).expect("name the entry");
            edge_add(
                tx,
                AutomationPictureOwner::Automation,
                one.id,
                None,
                EdgeTarget::Go(two.id),
                None,
            )
            .expect("add edge");
            delete(tx, automation.id).expect("delete the automation");
            assert!(read::automation(tx.conn(), automation.id).expect("read").is_none());
            assert!(read::automation_placement_ids(tx.conn(), automation.id)
                .expect("read")
                .is_empty());
            assert!(
                read::automation_action(tx.conn(), action.id).expect("read").is_some(),
                "the library outlives any one picture",
            );
        });
    }

    #[test]
    fn a_library_action_standing_on_a_picture_is_not_deleted() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placement) = mk_placed(tx, &automation, "点検");
            assert!(action_delete(tx, action.id).is_err());
            placement_delete(tx, placement.id).expect("take the placement off");
            action_delete(tx, action.id).expect("now it goes");
            assert!(
                read::automation_exit_ids(tx.conn(), AutomationOwner::Action, action.id)
                    .expect("read")
                    .is_empty(),
                "its ways out went with it",
            );
            assert!(
                read::automation_action_step_ids(tx.conn(), action.id).expect("read").is_empty(),
                "and so did the steps inside it",
            );
        });
    }

    #[test]
    fn a_name_is_free_within_its_owner_and_nowhere_wider() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (one, _) = mk_placed(tx, &automation, "点検する");
            let (two, _) = mk_placed(tx, &automation, "直す");
            exit_add(tx, AutomationOwner::Action, one.id, Some("直すところがある")).expect("add exit");
            assert!(
                exit_add(tx, AutomationOwner::Action, one.id, Some("直すところがある")).is_err(),
                "twice on one action is one name too many",
            );
            exit_add(tx, AutomationOwner::Action, two.id, Some("直すところがある"))
                .expect("another action is another list");
        });
    }

    #[test]
    fn an_action_moves_out_to_the_devices_library_with_its_placements() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placement) = mk_placed(tx, &automation, "取る");
            let moved = action_set_scope(tx, action.id, None).expect("out is never refused");
            assert_eq!(moved.project_id, None);
            assert_eq!(
                live_placement(tx, placement.id).expect("read").action_id,
                action.id,
                "the placement still stands on it",
            );
            assert_eq!(only_step(tx, &moved).action_id, action.id, "and the step went with it");
        });
    }

    #[test]
    fn an_action_moves_into_the_project_its_placements_are_in() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "取る");
            action_set_scope(tx, action.id, None).expect("out");
            let back = action_set_scope(tx, action.id, Some(automation.project_id))
                .expect("every placement is in this project");
            assert_eq!(back.project_id, Some(automation.project_id));

            let unplaced = action_add(tx, None, "点検する", "").expect("add");
            let other = mk_project(tx, "別");
            let moved = action_set_scope(tx, unplaced.id, Some(other))
                .expect("an action placed nowhere goes anywhere");
            assert_eq!(moved.project_id, Some(other));
        });
    }

    #[test]
    fn an_action_placed_in_another_project_is_not_moved_into_this_one() {
        with_tx(|tx| {
            let here = mk_automation(tx);
            let (action, _) = mk_placed(tx, &here, "取る");
            action_set_scope(tx, action.id, None).expect("out");
            let other = mk_project(tx, "別");
            let refused = action_set_scope(tx, action.id, Some(other)).expect_err("placed elsewhere");
            let said = refused.to_string();
            assert!(said.contains(&here.name), "it names the automation: {said}");
            assert!(
                said.contains(&crate::idref::project(here.project_id)),
                "and the project it is in: {said}",
            );
            assert_eq!(
                live_action(tx, action.id).expect("read").project_id,
                None,
                "and nothing moved",
            );
        });
    }

    #[test]
    fn a_moved_action_goes_to_the_bottom_of_its_new_library() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let first = action_add(tx, None, "取る", "").expect("add");
            let second = action_add(tx, None, "点検する", "").expect("add");
            let mine = action_add(tx, Some(project), "直す", "").expect("add");
            action_set_scope(tx, mine.id, None).expect("out");
            let ids: Vec<i64> = read::automation_action_siblings(tx.conn(), None, None)
                .expect("read")
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            assert_eq!(ids, vec![first.id, second.id, mine.id]);
        });
    }
}

/// **A definition a run is going on is held** (`AMB-D-961`): every op that rewrites one is refused
/// while a run launched from it is `running` or `paused`, and lets go once the run has ended.
#[cfg(test)]
mod held_by_a_run {
    use super::*;
    use crate::model::{AutomationRun, AutomationRunStatus};
    use crate::ops::automation_run::{launch, nothing_asked, Launcher};
    use crate::ops::automation_stop::{self, Ending};
    use crate::ops::test_support::{mk_exit, mk_in, mk_out, mk_placed, mk_project, only_step, with_tx};

    /// Two spots joined by an edge and a wire, each action carrying a way out, an input, an output and
    /// a setting with its answer — one of everything an op here can rewrite.
    struct Picture {
        automation: Automation,
        first_action: AutomationAction,
        first: AutomationPlacement,
        second_action: AutomationAction,
        onward: AutomationEdge,
        wire: AutomationWire,
        cfg: AutomationCfg,
    }

    fn picture(tx: &WriteTx<'_>) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation = add(tx, project, NewAutomation { name: "1件やりきる".into(), ..Default::default() })
            .expect("add automation");
        let (first_action, first) = mk_placed(tx, &automation, "調べる", "look at it", "claude");
        mk_exit(tx, &first_action, "found");
        mk_out(tx, &first_action, Some("found"), "note", AutomationPortKind::Value, false);
        let (second_action, second) = mk_placed(tx, &automation, "直す", "fix it", "claude");
        mk_in(tx, &second_action, "note", AutomationPortKind::Value, false);
        // An entry has to take a task for the run to launch, and either action may be the entry.
        mk_out(tx, &first_action, Some("found"), "task", AutomationPortKind::TaskTake, true);
        mk_out(tx, &second_action, None, "task", AutomationPortKind::TaskTake, false);
        let on = AutomationPictureOwner::Automation;
        let onward = edge_add(tx, on, first.id, Some("found"), EdgeTarget::Go(second.id), None)
            .expect("edge");
        edge_add(tx, on, first.id, None, EdgeTarget::Done, None).expect("edge");
        edge_add(tx, on, second.id, None, EdgeTarget::Done, None).expect("edge");
        let wire = wire_add(tx, on, first.id, Some("found"), "note", second.id, "note").expect("wire");
        let cfg = cfg_add(tx, first_action.id, "depth", AutomationCfgKind::Text, false, None)
            .expect("setting");
        cfg_set(tx, first.id, "depth", Some("shallow")).expect("answer");
        set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, first_action, first, second_action, onward, wire, cfg }
    }

    fn launched(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let startable = vec!["claude".to_string()];
        let launcher = Launcher {
            startable: Some(&startable),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(crate::model::ActorKind::Ai),
        };
        launch(tx, automation.id, &launcher).expect("launch")
    }

    /// The refusal a held definition answers with: a conflict naming the run that holds it.
    fn held<T: std::fmt::Debug>(what: &str, run: &AutomationRun, result: Result<T>) {
        let err = result.expect_err(what);
        assert_eq!(err.code(), "conflict", "{what}: {err}");
        assert!(err.to_string().contains(&format!("run {}", run.id)), "{what}: {err}");
    }

    /// Every rewrite of either definition — the automation's picture and the actions placed on it.
    fn every_rewrite(tx: &WriteTx<'_>, p: &Picture, run: &AutomationRun) {
        let step = only_step(tx, &p.first_action);
        let action = p.first_action.id;
        let found = read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, action, Some("found"))
            .expect("read")
            .expect("the way out");
        let note = read::automation_port_ids(tx.conn(), AutomationPortOwner::Exit, found.id)
            .expect("read")[0];
        let inner_edge = read::automation_edge_ids(tx.conn(), AutomationPictureOwner::Action, action)
            .expect("read")[0];
        let answer =
            read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, p.first.id, "depth")
                .expect("read")
                .expect("the answer");

        // The automation's picture.
        held("rename", run, update(tx, p.automation.id, Some("別名"), None, None));
        held("archive", run, update(tx, p.automation.id, None, None, Some(true)));
        held("entry", run, set_entry(tx, p.automation.id, None));
        held("delete", run, delete(tx, p.automation.id));
        held("place", run, placement_add(tx, p.automation.id, p.second_action.id));
        held("place new", run, placement_add_new(tx, p.automation.id, ActionShelf::Project, "新しい"));
        held("place on a line", run, placement_insert(tx, p.onward.id, p.second_action.id));
        held(
            "place new on a line",
            run,
            placement_insert_new(tx, p.onward.id, ActionShelf::Project, "新しい"),
        );
        held("reorder a placement", run, placement_move(tx, p.first.id, Position::Bottom));
        held("take a placement off", run, placement_delete(tx, p.first.id));
        held("answer a setting", run, cfg_set(tx, p.first.id, "depth", Some("deep")));
        held("rewrite an answer", run, cfg_update(tx, answer.id, None, None, Some(true), None));
        held("reorder an answer", run, cfg_move(tx, answer.id, Position::Top));
        held("take an answer off", run, cfg_delete(tx, answer.id));
        held(
            "draw a line",
            run,
            edge_add(tx, AutomationPictureOwner::Automation, p.first.id, Some(ERROR_EXIT), EdgeTarget::Done, None),
        );
        held("redraw a line", run, edge_update(tx, p.onward.id, Some(EdgeTarget::Done), None));
        held("rub a line out", run, edge_delete(tx, p.onward.id));
        held(
            "wire",
            run,
            wire_add(tx, AutomationPictureOwner::Automation, p.first.id, Some("found"), "note", p.first.id, "note"),
        );
        held("unwire", run, wire_delete(tx, p.wire.id));

        // An action placed on it.
        held("rename the action", run, action_update(tx, action, Some("別名"), None));
        held("the action's entry", run, action_set_entry(tx, action, None));
        held("move the action's reach", run, action_set_scope(tx, action, None));
        held("delete the action", run, action_delete(tx, action));
        held("add a step", run, step_add(tx, action, NewStep::new("もう一つ", "again", "claude")));
        held(
            "add a step on a line",
            run,
            step_insert(tx, inner_edge, NewStep::new("もう一つ", "again", "claude"), &[], &[]),
        );
        held(
            "rewrite a step",
            run,
            step_update(tx, step.id, None, Some("again"), None, None, None, None, None, None),
        );
        held("reorder a step", run, step_move(tx, step.id, Position::Bottom));
        held("delete a step", run, step_delete(tx, step.id));
        held("add a way out", run, exit_add(tx, AutomationOwner::Action, action, Some("other")));
        held("add a step's way out", run, exit_add(tx, AutomationOwner::Step, step.id, Some("other")));
        held("rename a way out", run, exit_rename(tx, found.id, Some("seen")));
        held("reorder a way out", run, exit_move(tx, found.id, Position::Top));
        held("delete a way out", run, exit_delete(tx, found.id));
        held(
            "add an output",
            run,
            port_add(
                tx,
                AutomationPortOwner::Exit,
                found.id,
                AutomationPortDirection::Out,
                "more",
                AutomationPortKind::Value,
                false,
            ),
        );
        held(
            "add an input",
            run,
            port_add(
                tx,
                AutomationPortOwner::Step,
                step.id,
                AutomationPortDirection::In,
                "more",
                AutomationPortKind::Value,
                false,
            ),
        );
        held("rewrite a port", run, port_update(tx, note, Some("memo"), None, None));
        held("reorder a port", run, port_move(tx, note, Position::Top));
        held("delete a port", run, port_delete(tx, note));
        held(
            "declare a setting",
            run,
            cfg_add(tx, action, "breadth", AutomationCfgKind::Text, false, None),
        );
        held("rewrite a setting", run, cfg_update(tx, p.cfg.id, Some("width"), None, None, None));
        held("reorder a setting", run, cfg_move(tx, p.cfg.id, Position::Top));
        held("delete a setting", run, cfg_delete(tx, p.cfg.id));
        held(
            "draw a line inside",
            run,
            edge_add(tx, AutomationPictureOwner::Action, step.id, Some(ERROR_EXIT), EdgeTarget::Halt, None),
        );
        held("redraw a line inside", run, edge_update(tx, inner_edge, None, Some(Some(3))));
        held("rub a line out inside", run, edge_delete(tx, inner_edge));
        let inner_wire = read::automation_wire_ids(tx.conn(), AutomationPictureOwner::Action, action)
            .expect("read")[0];
        held("unwire inside", run, wire_delete(tx, inner_wire));
    }

    #[test]
    fn every_rewrite_is_refused_while_a_run_is_running_or_paused_and_allowed_once_it_ends() {
        with_tx(|tx| {
            let p = picture(tx);
            let run = launched(tx, &p.automation);
            every_rewrite(tx, &p, &run);

            automation_stop::pause(tx, run.id).expect("pause");
            let paused = read::automation_run(tx.conn(), run.id).expect("read").expect("the run");
            automation_stop::settle(tx, paused).expect("settle");
            let paused = read::automation_run(tx.conn(), run.id).expect("read").expect("the run");
            assert_eq!(paused.status, AutomationRunStatus::Paused);
            every_rewrite(tx, &p, &paused);

            automation_stop::stop(tx, run.id, Ending::Canceled).expect("stop");
            update(tx, p.automation.id, Some("別名"), None, None).expect("the automation is free");
            action_update(tx, p.first_action.id, Some("別名"), None).expect("its action is free");
        });
    }

    /// **The order of a list is not a definition.** Where an automation or an action sits among its
    /// neighbours in the sidebar and the library is nothing a run reads.
    #[test]
    fn reordering_the_sidebar_or_the_library_is_not_held() {
        with_tx(|tx| {
            let p = picture(tx);
            launched(tx, &p.automation);
            move_to(tx, p.automation.id, Position::Top).expect("the sidebar's order");
            action_move(tx, p.first_action.id, Position::Top).expect("the library's order");
        });
    }

    /// **A device-wide action is held by any project's run.** Placed by another project's automation,
    /// it is that project's run the refusal names.
    #[test]
    fn a_device_wide_action_is_held_by_the_run_of_whichever_project_placed_it() {
        with_tx(|tx| {
            let p = picture(tx);
            action_set_scope(tx, p.second_action.id, None).expect("to the device");
            let elsewhere = mk_project(tx, "other");
            let theirs = add(tx, elsewhere, NewAutomation { name: "よそ".into(), ..Default::default() })
                .expect("add");
            let placed = placement_add(tx, theirs.id, p.second_action.id).expect("placed there too");
            edge_add(tx, AutomationPictureOwner::Automation, placed.id, None, EdgeTarget::Done, None)
                .expect("edge");
            set_entry(tx, theirs.id, Some(placed.id)).expect("entry");
            let run = launched(tx, &theirs);

            held("the action", &run, action_update(tx, p.second_action.id, Some("別名"), None));
            update(tx, p.automation.id, Some("別名"), None, None)
                .expect("this project's automation has no run going");
        });
    }

    /// **No op that rewrites a definition forgets to ask.** Each `pub fn` here either asks
    /// [`not_under_a_run`] itself, or is named below with the reason it need not.
    #[test]
    fn every_op_that_rewrites_a_definition_asks_whether_a_run_is_going_on_it() {
        let source = include_str!("automation.rs");
        let ops = &source[..source.find("#[cfg(test)]\nmod tests").expect("the tests")];
        let need_not = [
            // Born with nothing launched from it yet.
            "action_add",
            "add",
            // The order of a list, not a definition.
            "action_move",
            "move_to",
            // Only through ops that ask: `action_add` then `step_add`, `placement_add`, `step_add`.
            "action_from_prompt",
            "placement_insert",
            "placement_insert_new",
            "step_insert",
        ];
        let mut forgot = Vec::new();
        for (at, _) in ops.match_indices("\npub fn ") {
            let rest = &ops[at + "\npub fn ".len()..];
            let name = &rest[..rest.find(['(', '<']).expect("a signature")];
            let body = &rest[..rest.find("\n}\n").expect("the end of the fn")];
            if !body.contains("not_under_a_run(") && !need_not.contains(&name) {
                forgot.push(name.to_string());
            }
        }
        assert!(forgot.is_empty(), "these ops rewrite a definition without asking: {forgot:?}");
    }
}
