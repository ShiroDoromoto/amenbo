//! Building an automation's definition — the tables of the definition side. Of the run side
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

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::model::{
    AttachmentTarget, Automation, AutomationAction, AutomationActionVersion, AutomationCfg,
    AutomationCfgKind,
    AutomationCfgOwner, AutomationEdge, AutomationEnds, AutomationExit,
    AutomationOwner, AutomationPictureOwner, AutomationPlacement, AutomationPlacementStep,
    AutomationPort, AutomationPortDirection, AutomationPortKind, AutomationPortOwner, AutomationStep,
    AutomationVersion, AutomationWire, StepScript, ACTION_BOUNDARY, DEFAULT_MAX_TIMES,
    DEFAULT_SCRIPT_TIMEOUT_MINUTES, DONE_EXIT,
    ERROR_EXIT, MAX_SCRIPT_TIMEOUT_MINUTES,
};
use crate::ops::{automation_builtin, automation_run, emit_create, emit_update, place, Position};
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
        let article = if what.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
        return Err(Error::invalid(format!("{article} {what} name cannot be empty")));
    }
    Ok(s.to_string())
}

/// The name of a way out, as a person may give it. [`ERROR_EXIT`] is refused: every owner is born
/// carrying that one, and a second row under the same name would leave an edge naming either of them.
fn checked_exit_name(name: &str) -> Result<String> {
    let s = checked_name("way out", name)?;
    if s == ERROR_EXIT {
        return Err(Error::Invalid(
            Msg::new(format!(
                "'{ERROR_EXIT}' is the error way out's own name — every step and every action carries it \
                 already, so it cannot be given to another"
            ))
            .coded(ErrorCode::InvalidAutomationExitReserved)
            .with("name", ERROR_EXIT),
        ));
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

// ───────────────────────────── a definition built into Amenbo ─────────────────────────────

/// The definition an op is about to rewrite: an automation's picture, a library action's insides, or
/// what one step of an action declares.
///
/// A run going on it does not stop the rewrite (`AMB-D-1015`): the run works from the copy it took at
/// launch, and the new definition is read at the next launch, or when a run paused before its next
/// task is resumed and copies it down afresh.
#[derive(Clone, Copy, Debug)]
enum Def {
    Automation,
    Action(i64),
    /// One step's own declarations — its fields, its ways out and its ports. It is the step's own for
    /// whether it is a built-in, and its action's after that.
    Step(i64),
}

/// **A built-in is not rewritten by hand** (`AMB-D-964`). Its ways out, its outputs and its settings are
/// the ones Amenbo's code leaves by and reads ([`crate::ops::automation_builtin`]), so a rename here would
/// leave the picture naming a way out the code never takes.
///
/// A built-in action refuses every rewrite of its insides. A built-in step is only ever written inside
/// one (`AMB-D-969`), but a store kept from before that may hold one inside an action somebody wrote:
/// that step still refuses a rewrite of what it declares, while the lines drawn to and from it are that
/// action's, and so is taking it off.
///
/// The rows are written from the definition before the key is set on them, which is how the writes
/// that build a built-in get past this.
fn not_built_in(tx: &WriteTx<'_>, def: Def) -> Result<()> {
    let refused = match def {
        Def::Automation => None,
        Def::Action(id) => {
            let action = live_action(tx, id)?;
            action.builtin.map(|_| format!("action '{}'", action.name))
        }
        Def::Step(id) => {
            let step = live_step(tx, id)?;
            match step.builtin {
                Some(_) => Some(format!("step '{}'", step.name)),
                None => live_action(tx, step.action_id)?
                    .builtin
                    .map(|_| format!("step '{}'", step.name)),
            }
        }
    };
    match refused {
        None => Ok(()),
        Some(what) => Err(Error::invalid(format!(
            "{what} is built into Amenbo — what it does, its ways out and its settings are Amenbo's own \
             and cannot be edited"
        ))),
    }
}

/// The definition a way out, or what declares one, belongs to: a step's is its action's, an action's
/// is its own.
fn def_of_declarer(tx: &WriteTx<'_>, owner_kind: AutomationOwner, owner_id: i64) -> Result<Def> {
    Ok(match owner_kind {
        AutomationOwner::Step => {
            live_step(tx, owner_id)?;
            Def::Step(owner_id)
        }
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
        AutomationCfgOwner::Placement => {
            live_placement(tx, owner_id)?;
            Def::Automation
        }
    })
}

/// The definition a line is drawn in.
fn def_of_picture(owner_kind: AutomationPictureOwner, owner_id: i64) -> Def {
    match owner_kind {
        AutomationPictureOwner::Automation => Def::Automation,
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

/// The two ways out every step and every action is born with: [`DONE_EXIT`], which is all an owner
/// with a single way out needs, and the error one, which nobody can delete. Written at the moment the
/// owner is created so that an edge or a port has somewhere to hang from the first command onwards.
fn born_with_exits(tx: &WriteTx<'_>, owner_kind: AutomationOwner, owner_id: i64) -> Result<()> {
    add_exit_row(tx, owner_kind, owner_id, DONE_EXIT.to_string())?;
    add_exit_row(tx, owner_kind, owner_id, ERROR_EXIT.to_string())?;
    Ok(())
}

/// Write one `automation_exit` row at the bottom of its owner's list. The name is taken as given — the
/// checking is the caller's, which is what lets [`born_with_exits`] write the one name a caller may not.
fn add_exit_row(
    tx: &WriteTx<'_>,
    owner_kind: AutomationOwner,
    owner_id: i64,
    name: String,
) -> Result<AutomationExit> {
    let sibs = read::automation_exit_siblings(tx.conn(), owner_kind, owner_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_exit")?;
    let exit = AutomationExit { id, owner_kind, owner_id, name, order_key, created_at: now, updated_at: now };
    emit_create(tx, record::automation_exit(&exit))?;
    Ok(exit)
}

/// The pictures a delete in the library takes lines from: the action's own. A placement points at a
/// saved version of the action (`AMB-D-1000`), and that version keeps the row's id (`AMB-D-961`), so
/// an automation's lines keyed to a way out or a port deleted here still mean what they meant.
const INSIDE: &[AutomationPictureOwner] = &[AutomationPictureOwner::Action];

/// Delete one way out, the outputs declared on it, and the lines of `pictures` keyed to it — an edge
/// that leaves by it or returns to it, and a wire that carries what it hands on. A line keyed to a row
/// that is gone decides nothing and carries nothing, which is why it goes with the row (`AMB-D-961`).
fn delete_exit_row(tx: &WriteTx<'_>, pictures: &[AutomationPictureOwner], exit_id: i64) -> Result<()> {
    for &on in pictures {
        let (edges, wires) = read::automation_line_ids_on_exit(tx.conn(), on, exit_id)?;
        for wire in wires {
            tx.delete_record("automation_wire", wire)?;
        }
        for edge in edges {
            tx.delete_record("automation_edge", edge)?;
        }
    }
    for port in read::automation_port_ids(tx.conn(), AutomationPortOwner::Exit, exit_id)? {
        delete_port_row(tx, pictures, port)?;
    }
    tx.delete_record("automation_exit", exit_id)?;
    Ok(())
}

/// **Make a built-in's ways out the ones named, in that order** — for the built-in whose ways out are an
/// axis's values (`AMB-D-972`, [`crate::ops::automation_builtin_split`]). Both halves follow: the ways
/// out of the one step inside the action and of the action itself, with the action's line that returns
/// each from the step to the action. The error way out is left where it is.
///
/// `renamed` is a value renamed in the same stroke. Its way out is renamed rather than deleted and
/// written again, so the lines an automation hangs on it stay (`AMB-D-961`). A value gone takes the
/// automations' lines on its way out too: the built-in is rewritten where it stands rather than saved
/// as a new version, so no placement still reads the way out.
///
/// **Written past the guard.** What it writes is Amenbo's own, from the axis, so the guard that refuses
/// a person's edit to a built-in is not for it.
pub(crate) fn builtin_exits_follow(
    tx: &WriteTx<'_>,
    action_id: i64,
    wanted: &[String],
    renamed: Option<(&str, &str)>,
) -> Result<()> {
    let action = live_action(tx, action_id)?;
    let step_id = action
        .entry_step_id
        .ok_or_else(|| Error::invalid(format!("the built-in action '{}' holds no step", action.name)))?;
    let owners = [(AutomationOwner::Step, step_id), (AutomationOwner::Action, action_id)];
    let named = |kind, id, name: &str| read::automation_exit_by_name(tx.conn(), kind, id, Some(name));
    if let Some((from, to)) = renamed {
        for (kind, id) in owners {
            if let (Some(before), None) = (named(kind, id, from)?, named(kind, id, to)?) {
                let after = AutomationExit { name: to.to_string(), updated_at: Timestamp::now(), ..before.clone() };
                emit_update(tx, record::automation_exit(&before), record::automation_exit(&after))?;
            }
        }
    }
    for (kind, id) in owners {
        for exit in read::automation_exits_of(tx.conn(), kind, id)? {
            if exit.name != ERROR_EXIT && !wanted.contains(&exit.name) {
                delete_exit_row(
                    tx,
                    &[AutomationPictureOwner::Action, AutomationPictureOwner::Automation],
                    exit.id,
                )?;
            }
        }
    }
    for name in wanted {
        let from = match named(AutomationOwner::Step, step_id, name)? {
            Some(exit) => exit,
            None => add_exit_row(tx, AutomationOwner::Step, step_id, name.clone())?,
        };
        let to = match named(AutomationOwner::Action, action_id, name)? {
            Some(exit) => exit,
            None => add_exit_row(tx, AutomationOwner::Action, action_id, name.clone())?,
        };
        if read::automation_edge_for_exit(tx.conn(), AutomationPictureOwner::Action, step_id, from.id)?.is_none() {
            let sibs = read::automation_edge_siblings(tx.conn(), AutomationPictureOwner::Action, action_id, None)?;
            let now = Timestamp::now();
            let edge = AutomationEdge {
                id: read::next_id(tx.conn(), "automation_edge")?,
                owner_kind: AutomationPictureOwner::Action,
                owner_id: action_id,
                from_id: step_id,
                exit_id: from.id,
                to_id: None,
                ends: AutomationEnds::Exit,
                exit_to_id: Some(to.id),
                max_times: None,
                order_key: place(&sibs, &Position::Bottom)?,
                created_at: now,
                updated_at: now,
            };
            emit_create(tx, record::automation_edge(&edge))?;
        }
    }
    // In the axis's order, after the error way out — only where it has moved, so a value added at the
    // bottom rewrites nothing else.
    for (kind, id) in owners {
        let standing: Vec<String> = read::automation_exits_of(tx.conn(), kind, id)?
            .into_iter()
            .map(|e| e.name)
            .filter(|n| n != ERROR_EXIT)
            .collect();
        if standing == wanted {
            continue;
        }
        for name in wanted {
            let before = named(kind, id, name)?.ok_or_else(|| Error::invalid("the way out was not written"))?;
            let sibs = read::automation_exit_siblings(tx.conn(), kind, id, Some(before.id))?;
            let after = AutomationExit {
                order_key: place(&sibs, &Position::Bottom)?,
                updated_at: Timestamp::now(),
                ..before.clone()
            };
            emit_update(tx, record::automation_exit(&before), record::automation_exit(&after))?;
        }
    }
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
        delete_exit_row(tx, INSIDE, exit)?;
    }
    for port in read::automation_port_ids(tx.conn(), port_owner, owner_id)? {
        delete_port_row(tx, INSIDE, port)?;
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
    write_action(tx, project_id, name, note, false)
}

/// [`action_add`], with whether the action is born still being written (`AMB-D-1005`) — which only the
/// ops that make one on the spot where it is placed say yes to.
fn write_action(
    tx: &WriteTx<'_>,
    project_id: Option<i64>,
    name: &str,
    note: &str,
    draft: bool,
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
        builtin: None,
        builtin_dimension_id: None,
        builtin_version: None,
        draft,
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
/// **Renaming parts nothing.** A placement points at the action by key (`automation_placement.action_id`),
/// and edges and wires key the ways out and the ports it declares (`AMB-D-961`), so renaming any of them
/// leaves every line where it was ([`exit_rename`], [`port_update`]).
pub fn action_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    note: Option<&str>,
) -> Result<AutomationAction> {
    let before = live_action(tx, id)?;
    not_built_in(tx, Def::Action(id))?;
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
    not_built_in(tx, Def::Action(action_id))?;
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
/// The two ways out the pair is born with are joined as well — [`DONE_EXIT`], so that an action
/// nobody gave a way out of its own still leaves, and the error one, so that a step that fell over leaves by
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
    let born_with = [Some(DONE_EXIT.to_string()), Some(ERROR_EXIT.to_string())];
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
    not_built_in(tx, Def::Action(id))?;
    if let Some(project_id) = project_id {
        if read::project(tx.conn(), project_id)?.is_none() {
            return Err(crate::ops::project::NOUN.not_found(project_id.to_string()));
        }
        let elsewhere = automations_placing_outside(tx, id, project_id)?;
        if !elsewhere.is_empty() {
            let project = read::project_name(tx.conn(), project_id)?.unwrap_or_default();
            let said: Vec<String> = elsewhere.iter().map(|one| one.said.clone()).collect();
            let names: Vec<String> = elsewhere.iter().map(|one| one.name.clone()).collect();
            return Err(Error::Invalid(
                Msg::new(format!(
                    "action '{id}' is placed on automations of other projects — {} — take it off them \
                     before moving it into project {}",
                    said.join(", "),
                    crate::idref::project(project_id),
                ))
                .coded(ErrorCode::InvalidActionPlacedElsewhere)
                .with("automations", names.join(", "))
                .with("project", project),
            ));
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
/// One automation outside the project that stands in the way: said in full for the English sentence,
/// and by its name alone for the screen's, which draws the project it is moving into beside it.
struct Outside {
    said: String,
    name: String,
}

fn automations_placing_outside(
    tx: &WriteTx<'_>,
    action_id: i64,
    project_id: i64,
) -> Result<Vec<Outside>> {
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
        named.push(Outside {
            said: format!(
                "automation '{}' ({}) in project {} '{project}'",
                automation.name,
                automation.id,
                crate::idref::project(automation.project_id),
            ),
            name: automation.name,
        });
    }
    Ok(named)
}

/// **Save what is inside an action as its next version** — the steps, the ways out of the action and of
/// each step, the inputs and the outputs on those ways out, the settings the action declares, the edges
/// and wires drawn inside it, and the step it opens first. The copy is never rewritten: writing on in the
/// action changes its tables and leaves this as it was.
///
/// Every row goes in as its own record under its own id, so the lines in the copy key their ways out and
/// ports exactly as the rows did (`AMB-D-961`). The number is one past the action's newest, and 1 for
/// its first.
///
/// Refused for a built-in: its versions are actions of their own (`AMB-D-1000`).
pub fn action_version_add(tx: &WriteTx<'_>, action_id: i64) -> Result<AutomationActionVersion> {
    let action = live_action(tx, action_id)?;
    not_built_in(tx, Def::Action(action_id))?;
    let conn = tx.conn();
    let (into, out_of) = (AutomationPortDirection::In, AutomationPortDirection::Out);
    let steps = read::automation_action_steps_of(conn, action_id)?;
    let mut exits = read::automation_exits_of(conn, AutomationOwner::Action, action_id)?;
    let mut ports = read::automation_ports_of(conn, AutomationPortOwner::Action, action_id, into)?;
    for step in &steps {
        exits.extend(read::automation_exits_of(conn, AutomationOwner::Step, step.id)?);
        ports.extend(read::automation_ports_of(conn, AutomationPortOwner::Step, step.id, into)?);
    }
    for exit in &exits {
        ports.extend(read::automation_ports_of(conn, AutomationPortOwner::Exit, exit.id, out_of)?);
    }
    let cfgs = read::automation_cfgs_of(conn, AutomationCfgOwner::Action, action_id)?;
    let edges = read::automation_edges_of(conn, AutomationPictureOwner::Action, action_id)?;
    let wires = read::automation_wires_of(conn, AutomationPictureOwner::Action, action_id)?;
    let version = read::automation_action_version_latest(conn, action_id)?.map_or(1, |v| v.version + 1);
    let now = Timestamp::now();
    let saved = AutomationActionVersion {
        id: read::next_id(conn, "automation_action_version")?,
        action_id,
        version,
        entry_step_id: action.entry_step_id,
        steps: serde_json::to_string(&steps).map_err(Error::from)?,
        exits: serde_json::to_string(&exits).map_err(Error::from)?,
        ports: serde_json::to_string(&ports).map_err(Error::from)?,
        cfgs: serde_json::to_string(&cfgs).map_err(Error::from)?,
        edges: serde_json::to_string(&edges).map_err(Error::from)?,
        wires: serde_json::to_string(&wires).map_err(Error::from)?,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_action_version(&saved))?;
    Ok(saved)
}

/// Delete a library action with everything inside it — the picture its steps are drawn into, the steps
/// with their own declarations, the ways out, ports and settings the action declared, and the versions
/// saved of it.
///
/// **Refused while it is placed**, naming how many placements: the placement would be left standing on
/// nothing, and what should stand there instead is not this op's to guess.
pub fn action_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let action = live_action(tx, id)?;
    not_built_in(tx, Def::Action(id))?;
    let users = read::automation_placement_ids_using_action(tx.conn(), id)?;
    if !users.is_empty() {
        return Err(Error::Invalid(
            Msg::new(format!(
                "{} placement(s) stand on this action — take them off the pictures they are on before \
                 deleting it",
                users.len()
            ))
            .coded(ErrorCode::InvalidActionStillPlaced)
            .with("count", users.len()),
        ));
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
    for version in read::automation_action_version_ids(tx.conn(), id)? {
        tx.delete_record("automation_action_version", version)?;
    }
    tx.delete_record("automation_action", id)?;
    Ok(())
}

/// **Say an action made on the spot is written** (`AMB-D-1005`) — one of the two ways out of being
/// still written, and the one that keeps it. It is taken with nothing inside the action, too: an action
/// with no step is the launch check's to refuse (`ActionEmpty`), not this op's.
///
/// Finishing an action that is already finished hands it straight back and writes nothing, the way
/// [`crate::ops::task::finish_creating`] does and for its reason.
pub fn action_finish_creating(tx: &WriteTx<'_>, id: i64) -> Result<AutomationAction> {
    let before = live_action(tx, id)?;
    if !before.draft {
        return Ok(before);
    }
    let after = AutomationAction { draft: false, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::automation_action(&before), record::automation_action(&after))?;
    Ok(after)
}

/// **Give up an action made on the spot** (`AMB-D-1005`) — the other way out of being still written.
/// The action goes, and so does every placement standing on it, in one act: what was made on the spot
/// was the pair, and half of it left behind is a picture nobody asked for.
///
/// **The lines go back to how they were before it was placed.** A line that ran into the placement is
/// pointed on to wherever the placement's own way out went, which is where it pointed before the
/// action was put in on it ([`splice_onto_edge`]); where the placement's way out says nothing, the way
/// out that led into it goes back to saying nothing ([`placement_insert_new_at_exit`]).
///
/// Refused for an action that is not being written: that one is kept, and taking it away is
/// [`action_delete`]'s, which asks first that nothing stands on it.
pub fn action_abandon(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let action = live_action(tx, id)?;
    if !action.draft {
        return Err(Error::invalid(format!(
            "action '{id}' is not being written, so there is nothing to give up — delete it with \
             `automation action-rm` once nothing stands on it"
        )));
    }
    for placement in read::automation_placement_ids_using_action(tx.conn(), id)? {
        take_off_as_before(tx, placement)?;
    }
    action_delete(tx, id)
}

/// Take a placement off and join the lines that ran into it to where its way out went — the reverse of
/// [`splice_onto_edge`] and [`hang_on_exit`], for [`action_abandon`].
fn take_off_as_before(tx: &WriteTx<'_>, placement_id: i64) -> Result<()> {
    let owner_kind = AutomationPictureOwner::Automation;
    let placement = live_placement(tx, placement_id)?;
    let done = box_exit(tx, owner_kind, placement_id, None)?;
    let onward = read::automation_edge_for_exit(tx.conn(), owner_kind, placement_id, done.id)?
        .filter(|edge| edge.to_id != Some(placement_id));
    let into: Vec<AutomationEdge> = read::automation_edges_of(tx.conn(), owner_kind, placement.automation_id)?
        .into_iter()
        .filter(|edge| edge.ends == AutomationEnds::Go && edge.to_id == Some(placement_id))
        .filter(|edge| edge.from_id != placement_id)
        .collect();
    for edge in into {
        let Some(onward) = &onward else {
            edge_delete(tx, edge.id)?;
            continue;
        };
        // The limit the line carried into the placement is the one it carried before, so a line going
        // on to a box keeps it; one that closes or stops the run carries none.
        let limit = match onward.ends {
            AutomationEnds::Go => None,
            _ => Some(None),
        };
        edge_update(tx, edge.id, Some(edge_target(tx, onward)?), limit)?;
    }
    placement_delete(tx, placement_id)
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
/// of the lists. A run of the automation going on does not stop it.
pub fn update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    notes: Option<&str>,
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

/// **Name any placement as the entry, for a test.** No op does this any more: the entry is the first
/// thing put on a picture, one of the built-ins a run can start at, and it is changed by
/// [`entry_replace`] (`AMB-D-977`). A store written before that can still name any placement — an
/// action somebody wrote, taking a task itself — and the run side goes on answering for that shape;
/// this is how its tests draw it.
#[cfg(test)]
pub(crate) fn set_entry(
    tx: &WriteTx<'_>,
    automation_id: i64,
    placement_id: Option<i64>,
) -> Result<Automation> {
    if let Some(placement_id) = placement_id {
        let placement = live_placement(tx, placement_id)?;
        if placement.automation_id != automation_id {
            return Err(Error::invalid(format!(
                "placement '{placement_id}' belongs to another automation, so it cannot be this one's \
                 entry"
            )));
        }
    }
    name_entry(tx, automation_id, placement_id)
}

/// Write which placement a run opens first, `None` for none. What may be named is the callers' to ask.
fn name_entry(tx: &WriteTx<'_>, automation_id: i64, placement_id: Option<i64>) -> Result<Automation> {
    let before = live_automation(tx, automation_id)?;
    let mut after = before.clone();
    after.entry_placement_id = placement_id;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation(&before), record::automation(&after))?;
    Ok(after)
}

/// **Change what a run starts at**, to another of the built-ins it can start at (`AMB-D-977`). `key` is
/// the built-in's key ([`automation_builtin::entries`]).
///
/// The entry keeps its spot, and every line drawn to it stays: what changes is the action standing
/// there. What belonged to the one it replaces goes with it — the lines out of its ways out, the
/// wires to and from its ports, and the answers written for its settings — because the new one declares
/// ways out, ports and settings of its own. The placements after it stay on the picture, for the reader
/// to join up again. The new one comes with what a built-in standing first answers ([`answer_as_entry`]).
///
/// **An automation with placements and no entry** — kept from a store written before the entry was
/// the first placement — takes the built-in as a new placement, standing alone, and starts at it.
/// There is no entry to replace, and no other road to one.
///
/// Refused on an automation with nothing on it: the first placement is where the entry is chosen
/// ([`placement_add`]).
pub fn entry_replace(tx: &WriteTx<'_>, automation_id: i64, key: &str) -> Result<AutomationPlacement> {
    let automation = live_automation(tx, automation_id)?;
    if !automation_builtin::starts_a_run(key) {
        return Err(not_an_entry(key));
    }
    let Some(entry_id) = automation.entry_placement_id else {
        if read::automation_placement_ids(tx.conn(), automation_id)?.is_empty() {
            return Err(Error::invalid(
                "nothing is placed on this automation yet — the first thing placed on it is what a run \
                 starts at, so place the built-in instead",
            ));
        }
        let action = automation_builtin::action(tx, key)?;
        let placement = put_placement(tx, &automation, action.id)?;
        name_entry(tx, automation_id, Some(placement.id))?;
        answer_as_entry(tx, &placement, Some(key))?;
        return Ok(placement);
    };
    let before = live_placement(tx, entry_id)?;
    let action = automation_builtin::action(tx, key)?;
    if before.action_id == action.id {
        return Ok(before);
    }
    let on = AutomationPictureOwner::Automation;
    for wire in read::automation_wire_ids_naming_box(tx.conn(), on, entry_id)? {
        tx.delete_record("automation_wire", wire)?;
    }
    for edge in read::automation_edges_from(tx.conn(), on, entry_id)? {
        tx.delete_record("automation_edge", edge.id)?;
    }
    delete_cfgs(tx, AutomationCfgOwner::Placement, entry_id)?;
    for chosen in read::automation_placement_step_ids(tx.conn(), entry_id)? {
        tx.delete_record("automation_placement_step", chosen)?;
    }
    let mut after = before.clone();
    after.action_id = action.id;
    after.version = None;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_placement(&before), record::automation_placement(&after))?;
    answer_as_entry(tx, &after, Some(key))?;
    Ok(after)
}

/// The refusal of a built-in, or an action, that a run cannot start at — naming the ones it can.
fn not_an_entry(what: &str) -> Error {
    Error::invalid(format!(
        "a run cannot start at {what} — it starts at one of the built-ins {}",
        automation_builtin::entries().iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(", ")
    ))
}

/// **Save the automation as its next version, once the launch check passes.** Every op that writes on
/// the automation writes on its tables, and those are the draft; this is what makes the draft the
/// definition a launch can stand on.
///
/// Refused as `not_ready` with the launch check's reasons ([`automation_run::check`]), asked without the
/// agents and models this machine has: those belong to the machine the run is launched on, not to the
/// definition. Not refused while a run of it goes on (`AMB-D-1015`), nor while it is archived — archiving
/// keeps it out of the lists and says nothing about its picture.
///
/// A draft with nothing changed since the newest version saves nothing, and that version is what comes
/// back.
pub fn save(tx: &WriteTx<'_>, automation_id: i64) -> Result<AutomationVersion> {
    let automation = live_automation(tx, automation_id)?;
    let unmet = automation_run::check(tx.conn(), automation_id, None, automation_run::nothing_asked())?;
    if !unmet.is_empty() {
        return Err(automation_run::not_ready("save", &automation.name, &unmet));
    }
    if let Some(latest) = read::automation_version_latest(tx.conn(), automation_id)? {
        if Picture::read(tx.conn(), &automation)?.is(&latest)? {
            return Ok(latest);
        }
    }
    version_add(tx, automation_id)
}

/// **Does the draft hold anything its newest saved version does not?** For an automation nobody has
/// saved, anything on its picture at all is unsaved. Its name and notes are not part of a version, so
/// writing them leaves this as it was.
pub fn unsaved(conn: &Connection, automation_id: i64) -> Result<bool> {
    let automation =
        read::automation(conn, automation_id)?.ok_or_else(|| not_found("automation", automation_id))?;
    let draft = Picture::read(conn, &automation)?;
    match read::automation_version_latest(conn, automation_id)? {
        Some(latest) => Ok(!draft.is(&latest)?),
        None => Ok(draft.entry_placement_id.is_some() || !draft.placements.is_empty()),
    }
}

/// **The automation's picture as its tables hold it** — the rows a version is saved from, with the
/// version of its action each placement stands on, the answers written for their settings, the agents
/// chosen for their steps, the edges and wires drawn on the automation, and the placement it opens first.
struct Picture {
    entry_placement_id: Option<i64>,
    placements: Vec<AutomationPlacement>,
    cfgs: Vec<AutomationCfg>,
    placement_steps: Vec<AutomationPlacementStep>,
    edges: Vec<AutomationEdge>,
    wires: Vec<AutomationWire>,
}

impl Picture {
    fn read(conn: &Connection, automation: &Automation) -> Result<Picture> {
        let placements = read::automation_placements_of(conn, automation.id)?;
        let mut cfgs = Vec::new();
        let mut placement_steps = Vec::new();
        for placement in &placements {
            cfgs.extend(read::automation_cfgs_of(conn, AutomationCfgOwner::Placement, placement.id)?);
            placement_steps.extend(read::automation_placement_steps_of(conn, placement.id)?);
        }
        Ok(Picture {
            entry_placement_id: automation.entry_placement_id,
            placements,
            cfgs,
            placement_steps,
            edges: read::automation_edges_of(conn, AutomationPictureOwner::Automation, automation.id)?,
            wires: read::automation_wires_of(conn, AutomationPictureOwner::Automation, automation.id)?,
        })
    }

    /// Whether `saved` holds this picture, row for row. Every op that writes a row moves its
    /// `updated_at`, so a row written since the save differs from its copy even where it was written back
    /// to the same value.
    fn is(&self, saved: &AutomationVersion) -> Result<bool> {
        fn same<T: serde::Serialize>(rows: &[T], saved: &str) -> Result<bool> {
            let saved: serde_json::Value = serde_json::from_str(saved).map_err(Error::from)?;
            Ok(serde_json::to_value(rows).map_err(Error::from)? == saved)
        }
        Ok(self.entry_placement_id == saved.entry_placement_id
            && same(&self.placements, &saved.placements)?
            && same(&self.cfgs, &saved.cfgs)?
            && same(&self.placement_steps, &saved.placement_steps)?
            && same(&self.edges, &saved.edges)?
            && same(&self.wires, &saved.wires)?)
    }
}

/// **Write the automation's picture down as its next version** ([`Picture`]), without asking whether it
/// launches — [`save`] is the op that asks. The copy is never rewritten: writing on in the automation
/// changes its tables and leaves this as it was.
///
/// Every row goes in as its own record under its own id, as [`action_version_add`]'s do (`AMB-D-961`).
/// The number is one past the automation's newest, and 1 for its first.
pub fn version_add(tx: &WriteTx<'_>, automation_id: i64) -> Result<AutomationVersion> {
    let automation = live_automation(tx, automation_id)?;
    let conn = tx.conn();
    let picture = Picture::read(conn, &automation)?;
    let version = read::automation_version_latest(conn, automation_id)?.map_or(1, |v| v.version + 1);
    let now = Timestamp::now();
    let saved = AutomationVersion {
        id: read::next_id(conn, "automation_version")?,
        automation_id,
        version,
        entry_placement_id: picture.entry_placement_id,
        placements: serde_json::to_string(&picture.placements).map_err(Error::from)?,
        cfgs: serde_json::to_string(&picture.cfgs).map_err(Error::from)?,
        placement_steps: serde_json::to_string(&picture.placement_steps).map_err(Error::from)?,
        edges: serde_json::to_string(&picture.edges).map_err(Error::from)?,
        wires: serde_json::to_string(&picture.wires).map_err(Error::from)?,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_version(&saved))?;
    Ok(saved)
}

/// Delete an automation and everything built onto it — wires, edges, the placements with the answers
/// written on them, and the versions saved of it. The library actions those placements stood on are
/// left where they are: the library outlives any one picture.
///
/// **Refused while a run stands behind it**, naming how many (`invalid_automation_has_runs`). A run
/// carries its own copy of the steps and would go on reading correctly, but it is filed under the
/// automation it was launched from, and deleting that leaves the record unable to say what was run.
/// Archiving is what takes such an automation out of the way.
pub fn delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let automation = live_automation(tx, id)?;
    let runs = read::automation_run_ids(tx.conn(), id)?;
    if !runs.is_empty() {
        return Err(Error::Invalid(
            Msg::new(format!(
                "{} run(s) were launched from this automation — it is what they are filed under, so it \
                 cannot be deleted",
                runs.len()
            ))
            .coded(ErrorCode::InvalidAutomationHasRuns)
            .with("count", runs.len()),
        ));
    }
    for wire in read::automation_wire_ids(tx.conn(), AutomationPictureOwner::Automation, id)? {
        tx.delete_record("automation_wire", wire)?;
    }
    for edge in read::automation_edge_ids(tx.conn(), AutomationPictureOwner::Automation, id)? {
        tx.delete_record("automation_edge", edge)?;
    }
    // The entry is a reference into the placements that are about to go, so it is dropped before them.
    if automation.entry_placement_id.is_some() {
        name_entry(tx, id, None)?;
    }
    for placement in read::automation_placement_ids(tx.conn(), id)? {
        delete_placement_row(tx, placement)?;
    }
    for version in read::automation_version_ids(tx.conn(), id)? {
        tx.delete_record("automation_version", version)?;
    }
    tx.delete_record("automation", id)?;
    Ok(())
}

/// Delete one run and everything filed under it — the values each step execution carried, the
/// executions themselves with whatever was attached to them, the tasks the run worked on, and the step
/// snapshots it took at launch and on each resume that copied it down afresh. Returns the blob hashes
/// those attachments pointed at, for the caller to reclaim once the transaction commits.
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
    // What a person handed over at launch hangs off the run itself (`AMB-D-970`).
    orphaned.extend(crate::ops::sweep_polymorphic(tx, AttachmentTarget::AutomationRun, id)?);
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
///
/// **The first thing put on an automation is what a run starts at** (`AMB-D-977`): it has to be one of
/// the built-ins a run can start at ([`automation_builtin::entries`]), and it is named the entry as it
/// is put down. Any other is refused there, naming the ones there are — a run could not start at it, and
/// the picture would only find out at launch. It comes with what a built-in standing first answers
/// ([`answer_as_entry`]).
pub fn placement_add(
    tx: &WriteTx<'_>,
    automation_id: i64,
    action_id: i64,
) -> Result<AutomationPlacement> {
    let automation = live_automation(tx, automation_id)?;
    checked_action(tx, &automation, action_id)?;
    if !read::automation_placement_ids(tx.conn(), automation_id)?.is_empty() {
        return put_placement(tx, &automation, action_id);
    }
    let action = live_action(tx, action_id)?;
    if !action.builtin.as_deref().is_some_and(automation_builtin::starts_a_run) {
        let what = match &action.builtin {
            Some(key) => format!("the built-in '{key}'"),
            None => format!("action '{}'", action.name),
        };
        return Err(Error::invalid(format!(
            "{} — nothing is placed on this automation yet, and what is placed first is where a run \
             starts",
            not_an_entry(&what)
        )));
    }
    let placement = put_placement(tx, &automation, action_id)?;
    name_entry(tx, automation_id, Some(placement.id))?;
    answer_as_entry(tx, &placement, action.builtin.as_deref())?;
    Ok(placement)
}

/// **What a built-in answers when it comes to stand first**, before anyone has answered it (`AMB-T-5795`).
///
/// The built-in that files a task leaves it not started unless told otherwise, and a run that starts
/// there with the task left not started works no task — the launch check refuses exactly that
/// (`not_ready_automation_entry_takes_no_task`). So standing first, it is answered the one way a run can
/// start from: take the task it files. The answer is an ordinary one, to be changed like any other.
fn answer_as_entry(tx: &WriteTx<'_>, placement: &AutomationPlacement, key: Option<&str>) -> Result<()> {
    use crate::ops::automation_builtin_make::{KEY, TAKE_IT, WHAT_THEN};
    if key != Some(KEY) {
        return Ok(());
    }
    let take = serde_json::to_string(TAKE_IT).map_err(Error::from)?;
    cfg_set(tx, placement.id, WHAT_THEN, Some(&take))?;
    Ok(())
}

/// **Put an action on an automation, for a test, whatever stands on it already.** [`placement_add`]
/// refuses anything but a built-in a run can start at as the first placement (`AMB-D-977`); a store
/// written before that can hold a picture that began with any action, and this is how the tests of
/// the run side draw one.
#[cfg(test)]
pub(crate) fn placement_add_by_hand(
    tx: &WriteTx<'_>,
    automation_id: i64,
    action_id: i64,
) -> Result<AutomationPlacement> {
    let automation = live_automation(tx, automation_id)?;
    checked_action(tx, &automation, action_id)?;
    put_placement(tx, &automation, action_id)
}

/// Write one placement row at the bottom of an automation's list — what [`placement_add`] does once it
/// has asked what it asks.
///
/// It stands on the action's newest saved version, or on none where nobody has saved one — which is
/// every built-in, whose `action_id` already names one version (`AMB-D-1000`).
fn put_placement(tx: &WriteTx<'_>, automation: &Automation, action_id: i64) -> Result<AutomationPlacement> {
    let sibs = read::automation_placement_siblings(tx.conn(), automation.id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let version = read::automation_action_version_latest(tx.conn(), action_id)?.map(|v| v.version);
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_placement")?;
    let placement = AutomationPlacement {
        id,
        automation_id: automation.id,
        action_id,
        version,
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

/// **Make an empty action and put it in on a line.** Where it goes is decided by the press, before
/// there is anything in it: the reader goes on to build the action and comes back to find it standing
/// where they meant it to (`AMB-D-956`). A picture with nothing on it has no line to press, and what
/// goes there first is one of the built-ins ([`placement_add`], `AMB-D-977`).
///
/// **What it takes is a name and a library, and nothing else.** The inside of an action is its steps,
/// and each step carries its own prompt, ways out and outputs — so it is built on the action's own
/// screen, and a dialog that took part of it here would be a second place to declare the same thing
/// (`AMB-D-954`). Until a step is written in it, the launch check refuses the automation on it
/// ([`crate::ops::automation_run`]'s `ActionEmpty`).
///
/// It is one act because half of it is a picture nobody asked for: an action in the library that
/// nothing stands on, or a line running past a spot that was meant to be on it.
///
/// **The action is born still being written** (`AMB-D-1005`), and the reader leaves it one of two ways:
/// [`action_finish_creating`] keeps it, and [`action_abandon`] takes it and its placement away again.
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
        let action = write_action(tx, shelf.under(project_id), name, "", true)?;
        let placement = placement_add(tx, automation_id, action.id)?;
        splice_onto_edge(tx, &edge, placement.id)?;
        return Ok(placement);
    }
    Err(Error::invalid(
        "that line is drawn inside an action, and what stands on one is a step — put a step in there \
         instead",
    ))
}

/// **Put an action on after a way out that says nothing yet** — [`placement_insert`] for a way out
/// with no line on it. The way out comes to point at the new placement, and the new placement's own
/// ways out are left saying nothing, for the reader to decide next.
///
/// It is not the "add at the end" [`splice_onto_edge`] has no room for: the way out that was pressed
/// points at the new placement, so it is never a box nothing points at. Without it, a way out with no
/// line has no `+` to press, and the picture cannot grow past its first box until something else is
/// decided there (`AMB-D-1003`).
pub fn placement_insert_at_exit(
    tx: &WriteTx<'_>,
    from_id: i64,
    exit_name: Option<&str>,
    action_id: i64,
) -> Result<AutomationPlacement> {
    open_exit(tx, AutomationPictureOwner::Automation, from_id, exit_name)?;
    let automation_id = live_placement(tx, from_id)?.automation_id;
    let placement = placement_add(tx, automation_id, action_id)?;
    hang_on_exit(tx, AutomationPictureOwner::Automation, from_id, exit_name, placement.id)?;
    Ok(placement)
}

/// **Make an empty action and put it on after a way out that says nothing yet** —
/// [`placement_insert_new`] for a way out with no line on it, and [`placement_insert_at_exit`] for an
/// action made on the spot.
pub fn placement_insert_new_at_exit(
    tx: &WriteTx<'_>,
    from_id: i64,
    exit_name: Option<&str>,
    shelf: ActionShelf,
    name: &str,
) -> Result<AutomationPlacement> {
    open_exit(tx, AutomationPictureOwner::Automation, from_id, exit_name)?;
    let automation_id = live_placement(tx, from_id)?.automation_id;
    let project_id = live_automation(tx, automation_id)?.project_id;
    let action = write_action(tx, shelf.under(project_id), name, "", true)?;
    let placement = placement_add(tx, automation_id, action.id)?;
    hang_on_exit(tx, AutomationPictureOwner::Automation, from_id, exit_name, placement.id)?;
    Ok(placement)
}

/// **An action put on straight after the entry that files a task, and the built-in Amenbo put on after
/// it** (`AMB-T-5797`) — what the build screen's four ways of putting an action on answer with.
#[derive(Clone, Debug)]
pub struct Placed {
    /// The placement the reader put on.
    pub placement: AutomationPlacement,
    /// The built-in that closes the task, put on after it by [`close_after_filed`] — `None` where
    /// nothing was put on.
    pub closer: Option<AutomationPlacement>,
}

/// **Close the task a run files, after the first action that works on it** (`AMB-T-5797`).
///
/// A run that starts by filing a task holds that task, and a picture that lets it go on without
/// closing it is refused at the launch (`AMB-D-967`). The reader who put one action after the entry
/// has nearly always said the whole of what the run does, so the built-in that closes the task is put
/// on after the action's done way out, and that one ends the run. It is an ordinary placement: the
/// reader takes it off or moves it like any other.
///
/// It is put on only where nothing has been decided that it would undo: the placement stands straight
/// after an entry that files a task (not after its error way out), its done way out still says
/// nothing, and no placement on the picture closes the task already.
pub fn close_after_filed(
    tx: &WriteTx<'_>,
    from_id: i64,
    exit_name: Option<&str>,
    placed: &AutomationPlacement,
) -> Result<Option<AutomationPlacement>> {
    if exit_name == Some(ERROR_EXIT) {
        return Ok(None);
    }
    let automation = live_automation(tx, placed.automation_id)?;
    if automation.entry_placement_id != Some(from_id) {
        return Ok(None);
    }
    let builtin_of = |action_id: i64| -> Result<Option<String>> {
        Ok(read::automation_action(tx.conn(), action_id)?.and_then(|a| a.builtin))
    };
    let entry = live_placement(tx, from_id)?;
    if builtin_of(entry.action_id)?.as_deref() != Some(super::automation_builtin_make::MAKE_TASK.key) {
        return Ok(None);
    }
    let close_key = super::automation_builtin_close::CLOSE_TASK.key;
    for one in read::automation_placements_of(tx.conn(), automation.id)? {
        if builtin_of(one.action_id)?.as_deref() == Some(close_key) {
            return Ok(None);
        }
    }
    // A done way out renamed away, or one that already goes somewhere, is the reader's decision.
    let Some(done) =
        read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, placed.action_id, None)?
    else {
        return Ok(None);
    };
    if read::automation_edge_for_exit(tx.conn(), AutomationPictureOwner::Automation, placed.id, done.id)?
        .is_some()
    {
        return Ok(None);
    }
    let close = automation_builtin::action(tx, close_key)?;
    let closer = placement_insert_at_exit(tx, placed.id, None, close.id)?;
    edge_add(tx, AutomationPictureOwner::Automation, closer.id, None, EdgeTarget::Done, None)?;
    Ok(Some(closer))
}

/// **The way out a box is to be followed on from**, refused when it already says what happens after
/// it: that one has a line, and a box goes in on the line ([`placement_insert`], [`step_insert`]).
/// Asked before anything is written, so the refusal names the way out rather than an edge.
fn open_exit(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    from_id: i64,
    exit_name: Option<&str>,
) -> Result<AutomationExit> {
    let exit = box_exit(tx, owner_kind, from_id, exit_name)?;
    if read::automation_edge_for_exit(tx.conn(), owner_kind, from_id, exit.id)?.is_some() {
        return Err(Error::invalid(format!(
            "'{}' already says what happens after it — put the {} in on that line instead",
            exit.name,
            box_word(owner_kind)
        )));
    }
    Ok(exit)
}

/// The one line from a way out that said nothing to the box put on after it. It carries the standing
/// limit, as the lines [`splice_onto_edge`] writes into a box do.
fn hang_on_exit(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    from_id: i64,
    exit_name: Option<&str>,
    new_box: i64,
) -> Result<()> {
    edge_add(tx, owner_kind, from_id, exit_name, EdgeTarget::Go(new_box), Some(DEFAULT_MAX_TIMES))?;
    Ok(())
}

/// Reorder a placement within its automation. It moves it in the lists alone — the picture is walked
/// from the entry along the edges, and no order here reaches it.
pub fn placement_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationPlacement> {
    let before = live_placement(tx, id)?;
    let sibs = read::automation_placement_siblings(tx.conn(), before.automation_id, Some(id))?;
    let mut after = before.clone();
    after.order_key = place(&sibs, &pos)?;
    after.updated_at = Timestamp::now();
    emit_update(tx, record::automation_placement(&before), record::automation_placement(&after))?;
    Ok(after)
}

/// **Move a placement onto another saved version of its action** (`AMB-D-1000`) — a newer one, or back
/// to an older one. Amenbo never does this by itself; a person or an AI does it here, on the draft, and
/// it reaches a run once the automation is saved.
///
/// **What the new version does not declare goes from the draft with it**: an edge on a way out it has
/// no longer, a wire from an output or into an input it has no longer, and the choice of who carries out
/// a step it holds no longer. Ways out and ports are matched by id, never by name (`AMB-D-961`), so one
/// renamed between the two versions keeps its lines. The answers to its settings stay, matched by name
/// as ever: one the new version does not declare is read by nothing, and moving back to the old version
/// wants it again.
///
/// Moving onto the version it already stands on writes nothing. Refused for a built-in, whose
/// `action_id` already names one version, and for a version the action does not have. Not refused while
/// a run of the automation goes on (`AMB-D-1015`).
pub fn placement_version_set(tx: &WriteTx<'_>, id: i64, version: i64) -> Result<AutomationPlacement> {
    let before = live_placement(tx, id)?;
    not_built_in(tx, Def::Action(before.action_id))?;
    let saved = read::automation_action_version(tx.conn(), before.action_id, version)?.ok_or_else(|| {
        Error::not_found(format!("version {version} of action '{}' not found", before.action_id))
    })?;
    if before.version == Some(version) {
        return Ok(before);
    }
    fn ids<T: serde::de::DeserializeOwned>(json: &str, id: fn(&T) -> i64) -> Result<BTreeSet<i64>> {
        let rows: Vec<T> = serde_json::from_str(json).map_err(Error::from)?;
        Ok(rows.iter().map(id).collect())
    }
    let steps = ids(&saved.steps, |s: &AutomationStep| s.id)?;
    let exits = ids(&saved.exits, |e: &AutomationExit| e.id)?;
    let ports = ids(&saved.ports, |p: &AutomationPort| p.id)?;
    let on = AutomationPictureOwner::Automation;
    for edge in read::automation_edges_of(tx.conn(), on, before.automation_id)? {
        if edge.from_id == id && !exits.contains(&edge.exit_id) {
            tx.delete_record("automation_edge", edge.id)?;
        }
    }
    for wire in read::automation_wires_of(tx.conn(), on, before.automation_id)? {
        let from_gone = wire.from_id == id
            && (wire.from_exit_id.is_some_and(|exit| !exits.contains(&exit))
                || !ports.contains(&wire.from_port_id));
        let to_gone = wire.to_id == id && !ports.contains(&wire.to_port_id);
        if from_gone || to_gone {
            tx.delete_record("automation_wire", wire.id)?;
        }
    }
    for chosen in read::automation_placement_steps_of(tx.conn(), id)? {
        if !steps.contains(&chosen.step_id) {
            tx.delete_record("automation_placement_step", chosen.id)?;
        }
    }
    let after = AutomationPlacement { version: Some(version), updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::automation_placement(&before), record::automation_placement(&after))?;
    Ok(after)
}

/// Take a placement off its automation, with the answers written on it and every edge and wire
/// naming it at either end. The action it stood on is untouched.
///
/// **The entry comes off last** (`AMB-D-977`). While anything else is on the picture it is refused —
/// a picture with no entry has nowhere to start, and nothing but a first placement names one — and
/// [`entry_replace`] is the way to change it. Taken off alone, it leaves the picture empty, and the
/// next thing placed is the entry again.
pub fn placement_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let placement = live_placement(tx, id)?;
    let automation = live_automation(tx, placement.automation_id)?;
    if automation.entry_placement_id == Some(id) {
        let others = read::automation_placement_ids(tx.conn(), automation.id)?.len() - 1;
        if others > 0 {
            return Err(Error::invalid(format!(
                "placement '{id}' is where a run of this automation starts, and {others} other \
                 placement(s) are still on it — take those off first, or change what it starts at with \
                 `automation entry-replace`"
            )));
        }
        // `entry_placement_id` is `RESTRICT`, and that check bites at the statement rather than at the
        // commit — so the reference is dropped before the row it names, not after.
        name_entry(tx, automation.id, None)?;
    }
    delete_placement_row(tx, id)
}

/// One placement and everything hanging off it, with no word about the entry — what [`delete`] walks,
/// where the entry has already been dropped and the automation itself is going anyway.
fn delete_placement_row(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    delete_lines_naming_box(tx, AutomationPictureOwner::Automation, id)?;
    delete_cfgs(tx, AutomationCfgOwner::Placement, id)?;
    for chosen in read::automation_placement_step_ids(tx.conn(), id)? {
        tx.delete_record("automation_placement_step", chosen)?;
    }
    tx.delete_record("automation_placement", id)?;
    Ok(())
}

/// **Choose who carries one step out at one placement** — the agent, and the model where one is
/// named (`AMB-D-960`). `model` `None` leaves the agent's own default.
///
/// The step has to be one of the action standing on the placement: a choice for any other step would
/// be written and never read. The row is written the first time somebody chooses and rewritten after
/// that, the way [`cfg_set`] answers a setting.
pub fn placement_step_set(
    tx: &WriteTx<'_>,
    placement_id: i64,
    step_id: i64,
    agent: &str,
    model: Option<&str>,
) -> Result<AutomationPlacementStep> {
    let placement = live_placement(tx, placement_id)?;
    checked_step_of_placement(tx, &placement, step_id)?;
    if let Some(key) = live_step(tx, step_id)?.builtin {
        return Err(Error::invalid(format!(
            "step '{step_id}' is the built-in '{key}', which Amenbo carries out itself — nobody is \
             chosen to carry it out"
        )));
    }
    let agent = checked_name("agent", agent)?;
    let model = match model {
        Some(model) => Some(checked_name("model", model)?),
        None => None,
    };
    let now = Timestamp::now();
    if let Some(before) = read::automation_placement_step_for(tx.conn(), placement_id, step_id)? {
        let mut after = before.clone();
        after.agent = agent;
        after.model = model;
        after.updated_at = now;
        emit_update(
            tx,
            record::automation_placement_step(&before),
            record::automation_placement_step(&after),
        )?;
        return Ok(after);
    }
    let chosen = AutomationPlacementStep {
        id: read::next_id(tx.conn(), "automation_placement_step")?,
        placement_id,
        step_id,
        agent,
        model,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_placement_step(&chosen))?;
    Ok(chosen)
}

/// **Write the default agent onto every step of a placement that nobody has chosen for yet** — what
/// placing an action leaves behind, so the steps do not each have to be chosen again before a launch
/// will take them. `agent` is the answer a pane would open with ([`crate::wake::step_agent`]); `None`
/// leaves the steps as they are, with nobody chosen.
///
/// **What is written belongs to the placement from then on.** It is an ordinary choice, the one
/// [`placement_step_set`] writes, and a later change to the default does not reach it. The model is
/// left out, which is the agent's own default. A built-in step is skipped: Amenbo carries it out
/// itself (`AMB-D-964`).
pub fn placement_steps_default(
    tx: &WriteTx<'_>,
    placement_id: i64,
    agent: Option<&str>,
) -> Result<()> {
    if agent.is_none() {
        return Ok(());
    }
    let placement = live_placement(tx, placement_id)?;
    for step in read::automation_action_steps_of(tx.conn(), placement.action_id)? {
        placement_step_default(tx, placement_id, &step, agent)?;
    }
    Ok(())
}

/// **Write the default agent onto one step at one placement, where nobody has chosen for it yet** —
/// [`placement_steps_default`] for a single step. It is also what a step added to an action already
/// placed is given at each of its placements (`AMB-T-5531`), so the new step does not stand there
/// with nobody chosen while the steps placed with the action have somebody.
///
/// A choice already made stays, a built-in is passed by, and `None` writes nothing.
pub fn placement_step_default(
    tx: &WriteTx<'_>,
    placement_id: i64,
    step: &AutomationStep,
    agent: Option<&str>,
) -> Result<()> {
    let Some(agent) = agent else { return Ok(()) };
    if step.builtin.is_some()
        || read::automation_placement_step_for(tx.conn(), placement_id, step.id)?.is_some()
    {
        return Ok(());
    }
    placement_step_set(tx, placement_id, step.id, agent, None)?;
    Ok(())
}

/// **Take back the choice of who carries one step out at one placement**, leaving nobody chosen there.
/// A launch that would open the step is then refused until somebody chooses again. Nothing chosen is
/// nothing to take back, and that is not an error.
pub fn placement_step_clear(tx: &WriteTx<'_>, placement_id: i64, step_id: i64) -> Result<()> {
    let placement = live_placement(tx, placement_id)?;
    checked_step_of_placement(tx, &placement, step_id)?;
    if let Some(chosen) = read::automation_placement_step_for(tx.conn(), placement_id, step_id)? {
        tx.delete_record("automation_placement_step", chosen.id)?;
    }
    Ok(())
}

/// The step a choice is made for has to be inside the action standing on the placement.
fn checked_step_of_placement(
    tx: &WriteTx<'_>,
    placement: &AutomationPlacement,
    step_id: i64,
) -> Result<()> {
    let step = live_step(tx, step_id)?;
    if step.action_id != placement.action_id {
        return Err(Error::invalid(format!(
            "step '{step_id}' is not inside the action placed at placement '{}'",
            placement.id
        )));
    }
    Ok(())
}

// ───────────────────────────── steps ─────────────────────────────

/// What a new step is made of. `show_history`, `show_notes`, `show_decisions` and `show_comments` start
/// on — a step is handed the run's story so far and the task it is on (its notes, the decisions linked to
/// it and its comments) unless somebody says otherwise — while `interactive` and `report_to_task` start
/// off.
#[derive(Clone, Debug)]
pub struct NewStep {
    pub name: String,
    pub prompt: String,
    pub interactive: bool,
    /// The name of the setting or the input the working folder is taken from — a name, not a path.
    pub work_dir_ref: Option<String>,
    pub report_to_task: bool,
    pub show_history: bool,
    pub show_notes: bool,
    pub show_decisions: bool,
    pub show_comments: bool,
    /// The script the step is, or `None` for a step an agent carries out (`AMB-D-1016`).
    pub script: Option<NewScript>,
}

/// A script as it is written: the timeout may be left out, and then it is
/// [`DEFAULT_SCRIPT_TIMEOUT_MINUTES`].
#[derive(Clone, Debug)]
pub struct NewScript {
    pub program: String,
    pub args: Vec<String>,
    pub timeout_minutes: Option<i64>,
}

impl NewStep {
    /// A step with the six flags where they start.
    pub fn new(name: &str, prompt: &str) -> NewStep {
        NewStep {
            name: name.to_string(),
            prompt: prompt.to_string(),
            interactive: false,
            work_dir_ref: None,
            report_to_task: false,
            show_history: true,
            show_notes: true,
            show_decisions: true,
            show_comments: true,
            script: None,
        }
    }
}

/// A script as it may be saved (`AMB-D-1016`). The program is named by its full path, wherever it is —
/// the folder it would be looked up from is not the step's to know. An argument is one line, because
/// that is how they are written down. The timeout is between 1 and [`MAX_SCRIPT_TIMEOUT_MINUTES`]
/// minutes.
fn checked_script(new: NewScript) -> Result<StepScript> {
    if !std::path::Path::new(&new.program).is_absolute() {
        return Err(Error::invalid(format!(
            "a script's program is named by its full path, and '{}' is not one",
            new.program
        )));
    }
    if new.args.iter().any(|arg| arg.contains(['\n', '\r'])) {
        return Err(Error::invalid("a script's argument is one line — put each on a line of its own"));
    }
    let timeout_minutes = new.timeout_minutes.unwrap_or(DEFAULT_SCRIPT_TIMEOUT_MINUTES);
    if !(1..=MAX_SCRIPT_TIMEOUT_MINUTES).contains(&timeout_minutes) {
        return Err(Error::invalid(format!(
            "a script's timeout is 1 to {MAX_SCRIPT_TIMEOUT_MINUTES} minutes, not {timeout_minutes}"
        )));
    }
    Ok(StepScript { program: new.program, args: new.args, timeout_minutes })
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
///
/// **An action with no entry takes this step as its entry** (`AMB-T-5517`), so the first step written
/// is the one a placement opens first, and an action with steps never stands there without one.
pub fn step_add(tx: &WriteTx<'_>, action_id: i64, new: NewStep) -> Result<AutomationStep> {
    let action = live_action(tx, action_id)?;
    not_built_in(tx, Def::Action(action_id))?;
    let name = checked_name("step", &new.name)?;
    let script = new.script.map(checked_script).transpose()?;
    let sibs = read::automation_action_step_siblings(tx.conn(), action_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_action_step")?;
    let step = AutomationStep {
        id,
        action_id,
        name,
        prompt: new.prompt,
        builtin: None,
        script,
        interactive: new.interactive,
        work_dir_ref: new.work_dir_ref,
        report_to_task: new.report_to_task,
        show_history: new.show_history,
        show_notes: new.show_notes,
        show_decisions: new.show_decisions,
        show_comments: new.show_comments,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_action_step(&step))?;
    born_with_exits(tx, AutomationOwner::Step, id)?;
    if action.entry_step_id.is_none() {
        action_set_entry(tx, action_id, Some(id))?;
    }
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
    let onward = edge_target(tx, edge)?;
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

/// Where an edge goes, said the way [`edge_update`] takes it.
fn edge_target(tx: &WriteTx<'_>, edge: &AutomationEdge) -> Result<EdgeTarget> {
    Ok(match edge.ends {
        AutomationEnds::Go => EdgeTarget::Go(
            edge.to_id.ok_or_else(|| Error::invalid("the way out goes on to nothing"))?,
        ),
        AutomationEnds::Exit => EdgeTarget::Exit(match edge.exit_to_id {
            Some(id) => Some(live_exit(tx, id)?.name),
            None => None,
        }),
        AutomationEnds::Done => EdgeTarget::Done,
        AutomationEnds::Halt => EdgeTarget::Halt,
    })
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
    let step = write_step(tx, action_id, new, exits, inputs)?;
    splice_onto_edge(tx, &edge, step.id)?;
    Ok(step)
}

/// **Put a step on after a way out that says nothing yet**, inside one action —
/// [`placement_insert_at_exit`]'s twin, and [`step_insert`] for a way out with no line on it. One
/// transaction, for the reason given there.
pub fn step_insert_at_exit(
    tx: &WriteTx<'_>,
    from_id: i64,
    exit_name: Option<&str>,
    new: NewStep,
    exits: &[String],
    inputs: &[(String, AutomationPortKind, bool)],
) -> Result<AutomationStep> {
    open_exit(tx, AutomationPictureOwner::Action, from_id, exit_name)?;
    let action_id = live_step(tx, from_id)?.action_id;
    let step = write_step(tx, action_id, new, exits, inputs)?;
    hang_on_exit(tx, AutomationPictureOwner::Action, from_id, exit_name, step.id)?;
    Ok(step)
}

/// A step with the ways out and the inputs the dialog took — what [`step_insert`] and
/// [`step_insert_at_exit`] write before the line that reaches it.
fn write_step(
    tx: &WriteTx<'_>,
    action_id: i64,
    new: NewStep,
    exits: &[String],
    inputs: &[(String, AutomationPortKind, bool)],
) -> Result<AutomationStep> {
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
    Ok(step)
}

/// Change a step. Only the `Some` fields are written. `script` turns the step into a script, or with
/// `Some(None)` back into a step an agent carries out; it is written whole.
#[allow(clippy::too_many_arguments)]
pub fn step_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    prompt: Option<&str>,
    interactive: Option<bool>,
    work_dir_ref: Option<Option<&str>>,
    report_to_task: Option<bool>,
    show_history: Option<bool>,
    show_notes: Option<bool>,
    show_decisions: Option<bool>,
    show_comments: Option<bool>,
    script: Option<Option<NewScript>>,
) -> Result<AutomationStep> {
    let before = live_step(tx, id)?;
    not_built_in(tx, Def::Step(id))?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = checked_name("step", name)?;
    }
    if let Some(prompt) = prompt {
        after.prompt = prompt.to_string();
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
    if let Some(show_notes) = show_notes {
        after.show_notes = show_notes;
    }
    if let Some(show_decisions) = show_decisions {
        after.show_decisions = show_decisions;
    }
    if let Some(show_comments) = show_comments {
        after.show_comments = show_comments;
    }
    if let Some(script) = script {
        after.script = script.map(checked_script).transpose()?;
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
    not_built_in(tx, Def::Action(before.action_id))?;
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
/// **Deleting the entry hands it to the first of the steps left** (`AMB-T-5517`), in the order the
/// lists show them. An action under construction has to be able to lose any step, and refusing here
/// would strand whichever one was named the entry first. Only the last step going leaves the action
/// with no entry, and an action with nothing to open is refused at the launch check.
pub fn step_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let step = live_step(tx, id)?;
    not_built_in(tx, Def::Action(step.action_id))?;
    let action = live_action(tx, step.action_id)?;
    // `entry_step_id` is `RESTRICT`, and that check bites at the statement rather than at the commit —
    // so the reference is moved off the row before the row goes, not after.
    if action.entry_step_id == Some(id) {
        let left = read::automation_action_step_siblings(tx.conn(), action.id, Some(id))?;
        action_set_entry(tx, action.id, left.first().map(|(first, _)| *first))?;
    }
    delete_step_row(tx, id)
}

/// One step and everything hanging off it, with no word about the entry — what [`action_delete`] walks,
/// where the entry has already been dropped and the action itself is going anyway.
fn delete_step_row(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    delete_lines_naming_box(tx, AutomationPictureOwner::Action, id)?;
    delete_declarations(tx, AutomationOwner::Step, AutomationPortOwner::Step, id)?;
    // Who each placement chose for it stays: the version a placement points at keeps the step's id.
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
/// leave it naming either. `None` is [`DONE_EXIT`] — the way to put it back on an owner it was deleted
/// from.
pub fn exit_add(
    tx: &WriteTx<'_>,
    owner_kind: AutomationOwner,
    owner_id: i64,
    name: Option<&str>,
) -> Result<AutomationExit> {
    checked_declarer(tx, owner_kind, owner_id)?;
    not_built_in(tx, def_of_declarer(tx, owner_kind, owner_id)?)?;
    let name = checked_exit_name(name.unwrap_or(DONE_EXIT))?;
    if read::automation_exit_by_name(tx.conn(), owner_kind, owner_id, Some(&name))?.is_some() {
        return Err(exit_taken(&name));
    }
    add_exit_row(tx, owner_kind, owner_id, name)
}

/// A second way out under a name one on the same box already has — an edge names a way out by the box
/// and the name together, so two would leave it naming either.
fn exit_taken(name: &str) -> Error {
    Error::Invalid(
        Msg::new(format!("a way out called '{name}' is already declared here"))
            .coded(ErrorCode::InvalidAutomationExitTaken)
            .with("name", name),
    )
}

/// Rename a way out.
///
/// **Whatever hangs on it stays.** Edges and wires key a way out by its row, not its name
/// (`AMB-D-961`), so a rename changes what the picture says and nothing about how it is joined.
///
/// [`ERROR_EXIT`]'s row is refused at both ends: it may not be renamed, and no other may take its name.
/// **A way out keeps a name**: `None` is refused, since a way out with none is drawn blank on its line.
pub fn exit_rename(tx: &WriteTx<'_>, id: i64, name: Option<&str>) -> Result<AutomationExit> {
    let before = live_exit(tx, id)?;
    not_built_in(tx, def_of_declarer(tx, before.owner_kind, before.owner_id)?)?;
    if before.name == ERROR_EXIT {
        return Err(Error::invalid(
            "the error way out's name is fixed — every step and every action is read as carrying it",
        ));
    }
    let Some(name) = name else {
        return Err(Error::invalid("a way out keeps a name — give it the new one"));
    };
    let name = checked_exit_name(name)?;
    if let Some(holder) =
        read::automation_exit_by_name(tx.conn(), before.owner_kind, before.owner_id, Some(&name))?
    {
        if holder.id != id {
            return Err(exit_taken(&name));
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
    not_built_in(tx, def_of_declarer(tx, before.owner_kind, before.owner_id)?)?;
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
    not_built_in(tx, def_of_declarer(tx, exit.owner_kind, exit.owner_id)?)?;
    if exit.name == ERROR_EXIT {
        return Err(Error::invalid(
            "the error way out cannot be deleted — every step and every action carries one",
        ));
    }
    delete_exit_row(tx, INSIDE, id)
}

/// Declare a port: what a step or an action takes in (`In`, on that step or action) or what a way out
/// hands on (`Out`, on that way out).
///
/// The two directions hang on different owners, and neither is sayable on the other's: an input belongs
/// to the box that reads it, while an output belongs to the way out that produced it, which is what
/// lets a review step hand on a file only when it left through "something to fix".
///
/// **The task a run works is not handed on from here** (`AMB-D-964`): an output carrying it is refused
/// ([`not_the_task_taken`]). Only a built-in takes that task, and nothing a step can do declares it.
pub fn port_add(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPortOwner,
    owner_id: i64,
    direction: AutomationPortDirection,
    name: &str,
    kind: AutomationPortKind,
    required: bool,
) -> Result<AutomationPort> {
    not_the_task_taken(direction, kind)?;
    declare_port(tx, owner_kind, owner_id, direction, name, kind, required)
}

/// **An output does not carry the task the run works** (`AMB-D-964`). The run takes it at a built-in —
/// `take_task` or `make_task` — and a step has no command to hand it on, so a step whose way out
/// required one could never leave by it. An input of that kind stays: it is how a step reads the task
/// the built-in took.
fn not_the_task_taken(direction: AutomationPortDirection, kind: AutomationPortKind) -> Result<()> {
    if direction == AutomationPortDirection::Out && kind == AutomationPortKind::TaskTake {
        return Err(Error::invalid(
            "a way out cannot hand on the task the run works (task_take) — only a built-in takes it. \
             Put the built-in 'take_task' or 'make_task' before this action, and read the task it \
             hands on with an input",
        ));
    }
    Ok(())
}

/// [`port_add`] without asking what the output carries — how a built-in's rows are written, since the
/// ones that take a task declare exactly the output a person's action is refused.
pub(crate) fn declare_port(
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
    not_built_in(tx, def_of_port_owner(tx, owner_kind, owner_id)?)?;
    if read::automation_port_by_name(tx.conn(), owner_kind, owner_id, direction, &name)?.is_some() {
        let (said, code) = match direction {
            AutomationPortDirection::In => ("input", ErrorCode::InvalidAutomationInputTaken),
            AutomationPortDirection::Out => ("output", ErrorCode::InvalidAutomationOutputTaken),
        };
        return Err(Error::Invalid(
            Msg::new(format!("an {said} called '{name}' is already declared here"))
                .coded(code)
                .with("name", &name),
        ));
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
/// Renaming leaves every wire on the port standing: a wire keys the port it joins, not its name
/// (`AMB-D-961`).
pub fn port_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    kind: Option<AutomationPortKind>,
    required: Option<bool>,
) -> Result<AutomationPort> {
    let before = live_port(tx, id)?;
    not_built_in(tx, def_of_port_owner(tx, before.owner_kind, before.owner_id)?)?;
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
        not_the_task_taken(before.direction, kind)?;
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
    not_built_in(tx, def_of_port_owner(tx, before.owner_kind, before.owner_id)?)?;
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

/// Delete a port, and every wire inside the action keyed to it at either end — a wire from or into a
/// port that is gone carries nothing ([`INSIDE`]).
pub fn port_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let port = live_port(tx, id)?;
    not_built_in(tx, def_of_port_owner(tx, port.owner_kind, port.owner_id)?)?;
    delete_port_row(tx, INSIDE, id)
}

/// One port and the wires of `pictures` keyed to it — what [`port_delete`] does, and what every sweep
/// that takes a port with its owner does.
fn delete_port_row(tx: &WriteTx<'_>, pictures: &[AutomationPictureOwner], id: i64) -> Result<()> {
    for &on in pictures {
        for wire in read::automation_wire_ids_naming_port(tx.conn(), on, id)? {
            tx.delete_record("automation_wire", wire)?;
        }
    }
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
    not_built_in(tx, Def::Action(action_id))?;
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
/// never read, and never shown. **It is a JSON array of distinct, non-empty strings**, at least one of
/// them: anything else leaves every answer either refused or unchecked ([`answer_misfit`]).
fn checked_options(kind: AutomationCfgKind, options: Option<&str>) -> Result<()> {
    let Some(options) = options else { return Ok(()) };
    if kind != AutomationCfgKind::Choice {
        return Err(Error::invalid(format!(
            "a list of choices belongs to a 'choice' setting — this one is '{}'",
            kind.as_str()
        )));
    }
    let list = choices(options).ok_or_else(|| {
        Error::invalid(format!(
            "the choices {options} are not a JSON array of strings, like [\"one\",\"two\"]"
        ))
    })?;
    if list.is_empty() {
        return Err(Error::invalid("a list of choices needs at least one choice"));
    }
    let mut seen = BTreeSet::new();
    for one in &list {
        if one.trim().is_empty() {
            return Err(Error::invalid("a choice cannot be empty"));
        }
        if !seen.insert(one.as_str()) {
            return Err(Error::invalid(format!("the choice '{one}' is listed twice")));
        }
    }
    Ok(())
}

/// A choice list read as the strings it holds, or `None` where it is not a JSON array of strings.
fn choices(options: &str) -> Option<Vec<String>> {
    let serde_json::Value::Array(list) = serde_json::from_str(options).ok()? else {
        return None;
    };
    list.into_iter().map(|one| one.as_str().map(str::to_string)).collect()
}

/// **Why an answer does not fit the setting it answers**, or `None` where it does. Read the same way when
/// the answer is written ([`cfg_set`]) and when a run is launched on it (`automation_run::check`), so an
/// answer written before this was checked, or left behind when its declaration changed, is refused at
/// the launch rather than read as something nobody wrote.
///
/// The JSON's own type is what each kind takes: an object of parts for a task filter, a whole number of
/// zero or more for a number, and a string for the other three — for a choice, one of the declared
/// choices where there are any. A task filter's parts are read as the filter the run will search with.
pub fn answer_misfit(kind: AutomationCfgKind, options: Option<&str>, value: &str) -> Option<String> {
    use serde_json::Value;
    let Ok(read) = serde_json::from_str::<Value>(value) else {
        return Some(format!("{value} is not JSON"));
    };
    match (kind, &read) {
        (AutomationCfgKind::TaskFilter, Value::Object(parts)) => taskfilter_misfit(parts, value),
        (AutomationCfgKind::TaskFilter, _) => {
            Some(format!("a task filter is answered with its parts, and {value} is not"))
        }
        (AutomationCfgKind::Number, Value::Number(n)) if n.as_i64().is_some_and(|n| n >= 0) => None,
        (AutomationCfgKind::Number, _) => Some(format!("{value} is not a whole number of zero or more")),
        (AutomationCfgKind::Choice, Value::String(one)) => {
            let list = options.and_then(choices)?;
            (!list.contains(one)).then(|| format!("'{one}' is not one of the choices {}", list.join(", ")))
        }
        (AutomationCfgKind::Folder | AutomationCfgKind::Text, Value::String(_)) => None,
        (_, _) => Some(format!("a '{}' setting is answered with a string, and {value} is not one", kind.as_str())),
    }
}

/// A task filter's parts: each a string or a list of strings, the order one `task list --sort` takes,
/// and the whole a filter the grammar reads.
fn taskfilter_misfit(parts: &serde_json::Map<String, serde_json::Value>, value: &str) -> Option<String> {
    use crate::ops::automation_step::{taskfilter_expr, TASKFILTER_SORT_KEY};
    for (key, part) in parts {
        if key == TASKFILTER_SORT_KEY {
            match part.as_str() {
                Some(sort) if read::is_task_sort(sort) => continue,
                _ => return Some(format!("{part} is not an order `task list --sort` takes")),
            }
        }
        let strings = match part {
            serde_json::Value::String(_) => true,
            serde_json::Value::Array(many) => many.iter().all(serde_json::Value::is_string),
            _ => false,
        };
        if !strings {
            return Some(format!("the part '{key}' of a task filter is {part}, not a string or a list of them"));
        }
    }
    let expr = taskfilter_expr(value).unwrap_or_default();
    crate::query::Filter::parse(&expr, crate::time::today()).err().map(|e| e.to_string())
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
///
/// **A new name carries every placement's answer with it** (`AMB-T-5657`). The answer is found by the
/// declaration's name ([`cfg_set`]), so a rename that left it behind would drop it from view while its
/// row stayed in the store. A placement that already holds a row under the new name is refused rather
/// than overwritten, and an answer is not renamed on its own — its name is the declaration's.
pub fn cfg_update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    kind: Option<AutomationCfgKind>,
    required: Option<bool>,
    options: Option<Option<&str>>,
) -> Result<AutomationCfg> {
    let before = live_cfg(tx, id)?;
    not_built_in(tx, def_of_cfg_owner(tx, before.owner_kind, before.owner_id)?)?;
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
        if name != before.name {
            match before.owner_kind {
                AutomationCfgOwner::Placement => {
                    return Err(Error::invalid(format!(
                        "an answer is called by the name its action declares — rename setting \
                         '{}' on the action, and every placement's answer follows",
                        before.name
                    )));
                }
                AutomationCfgOwner::Action => {
                    carry_answers_to(tx, before.owner_id, &before.name, &name)?;
                }
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

/// Rename the answer every placement of `action_id` gives to setting `from`. Every placement is checked
/// before any row is written, so a refusal leaves no answer half-carried.
fn carry_answers_to(tx: &WriteTx<'_>, action_id: i64, from: &str, to: &str) -> Result<()> {
    let mut answers = Vec::new();
    for placement in read::automation_placement_ids_using_action(tx.conn(), action_id)? {
        if let Some(held) =
            read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, placement, to)?
        {
            return Err(Error::invalid(format!(
                "placement '{placement}' already holds an answer called '{to}' ({}) that no setting \
                 declares — take it off with `automation cfg-rm {}`, then rename",
                held.id, held.id
            )));
        }
        if let Some(answer) =
            read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, placement, from)?
        {
            answers.push(answer);
        }
    }
    for before in answers {
        let mut after = before.clone();
        after.name = to.to_string();
        after.updated_at = Timestamp::now();
        emit_update(tx, record::automation_cfg(&before), record::automation_cfg(&after))?;
    }
    Ok(())
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
    let name = checked_name("setting", name)?;
    let declared =
        read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Action, placement.action_id, &name)?;
    let answered =
        read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, placement.id, &name)?;
    // The declaration says what fits: the placement's row copied it at birth, and the declaration may
    // have changed since.
    let Some(declares) = declared.as_ref().or(answered.as_ref()) else {
        return Err(Error::not_found(format!("no setting called '{name}' is declared here")));
    };
    if let Some(value) = value {
        if let Some(why) = answer_misfit(declares.kind, declares.options.as_deref(), value) {
            let msg = Msg::new(format!("the setting '{name}' cannot take this answer: {why}"));
            // A number is the one kind whose answer is typed rather than picked, so it is the one a
            // person can get wrong from the screen; every other misfit is a caller writing the JSON by
            // hand, and keeps the family code with the reason in English.
            return Err(Error::Invalid(match declares.kind {
                AutomationCfgKind::Number => msg
                    .coded(ErrorCode::InvalidAutomationCfgNotACount)
                    .with("cfg", &name)
                    .with("value", value),
                _ => msg,
            }));
        }
    }
    filter_names_what_is_there(tx, declares.kind, value)?;
    if let Some(before) = answered {
        let mut after = before.clone();
        after.value = value.map(str::to_string);
        after.updated_at = Timestamp::now();
        emit_update(tx, record::automation_cfg(&before), record::automation_cfg(&after))?;
        return Ok(after);
    }
    let Some(declared) = declared else {
        return Err(Error::not_found(format!("no setting called '{name}' is declared here")));
    };
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

/// **A task filter's answer is read the way the run will read it** — parsed, and its axes and values
/// looked up (`AMB-T-5551`). A grammar check alone lets `dim:` name an axis or a value that is not
/// there, and the run then falls over at `task list` long after the person who wrote it has gone.
/// An answer of another kind, or one that is not an object of parts, is not a filter here.
fn filter_names_what_is_there(tx: &WriteTx<'_>, kind: AutomationCfgKind, value: Option<&str>) -> Result<()> {
    if kind != AutomationCfgKind::TaskFilter {
        return Ok(());
    }
    let Some(expr) = value.and_then(crate::ops::automation_step::taskfilter_expr) else {
        return Ok(());
    };
    let mut filter = crate::query::Filter::parse(&expr, crate::time::today())?;
    filter.resolve(tx.conn())
}

/// Reorder a setting within its owner's list.
pub fn cfg_move(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<AutomationCfg> {
    let before = live_cfg(tx, id)?;
    not_built_in(tx, def_of_cfg_owner(tx, before.owner_kind, before.owner_id)?)?;
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
    not_built_in(tx, def_of_cfg_owner(tx, cfg.owner_kind, cfg.owner_id)?)?;
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
        let n = exit_name.unwrap_or(DONE_EXIT);
        Error::not_found(format!("{what} '{box_id}' has no way out called '{n}'"))
    })
}

/// What an edge does once its way out is taken.
#[derive(Clone, Debug)]
pub enum EdgeTarget {
    /// Open the next box.
    Go(i64),
    /// Leave the action this picture is inside, by the way out it declares under this name — `None`
    /// being [`DONE_EXIT`]. An action's picture only: an automation's has nothing outside it. The
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
        || {
            let n = exit_to.unwrap_or(DONE_EXIT);
            Error::not_found(format!("this action has no way out called '{n}'"))
        },
    )
}

/// **Which lines of one picture go back** — the edges a limit on how often they are taken is counted on
/// ([`AutomationEdge::max_times`]).
///
/// A walk down from the entry, depth first, following each box's `go` edges in the order given: an edge
/// to a box still on the path the walk came down by goes back. What the entry does not reach is walked
/// afterwards, box by box in the order given, so every edge has an answer. `boxes` and `edges` are the
/// picture in display order — which decides the answer where a loop could be read from either end — and
/// the GUI draws the same edges dashed by the same walk (`app/src/screens/automationLayout.ts`, `walk`).
///
/// **A line that goes forward is never counted**, whatever it carries. Only a way back can spin, and a
/// limit on a line going down would stop a run on a number nobody is shown. Whether a line goes back
/// depends on every other line of the picture, so it is read here, off the whole picture, rather than
/// kept on the edge.
pub fn lines_back(entry: Option<i64>, boxes: &[i64], edges: &[AutomationEdge]) -> BTreeSet<i64> {
    let known: BTreeSet<i64> = boxes.iter().copied().collect();
    let mut out: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    for edge in edges {
        let Some(to) = edge.to_id.filter(|to| edge.ends == AutomationEnds::Go && known.contains(to))
        else {
            continue;
        };
        out.entry(edge.from_id).or_default().push((edge.id, to));
    }
    let mut back = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let starts = entry.filter(|id| known.contains(id)).into_iter().chain(boxes.iter().copied());
    for start in starts {
        if !seen.insert(start) {
            continue;
        }
        // The path down, each box with how many of its edges have been followed.
        let mut path: Vec<(i64, usize)> = vec![(start, 0)];
        while let Some((id, next)) = path.last_mut() {
            let Some(&(edge_id, to)) = out.get(id).and_then(|lines| lines.get(*next)) else {
                path.pop();
                continue;
            };
            *next += 1;
            if path.iter().any(|(on, _)| *on == to) {
                back.insert(edge_id);
            } else if seen.insert(to) {
                path.push((to, 0));
            }
        }
    }
    back
}

/// [`lines_back`] of the picture one box is drawn on, read off the store.
pub(crate) fn lines_back_on(
    conn: &Connection,
    owner_kind: AutomationPictureOwner,
    owner_id: i64,
) -> Result<BTreeSet<i64>> {
    let (entry, boxes) = match owner_kind {
        AutomationPictureOwner::Automation => (
            read::automation(conn, owner_id)?.and_then(|a| a.entry_placement_id),
            read::automation_placements_of(conn, owner_id)?.iter().map(|p| p.id).collect::<Vec<_>>(),
        ),
        AutomationPictureOwner::Action => (
            read::automation_action(conn, owner_id)?.and_then(|a| a.entry_step_id),
            read::automation_action_steps_of(conn, owner_id)?.iter().map(|s| s.id).collect(),
        ),
    };
    let edges = read::automation_edges_of(conn, owner_kind, owner_id)?;
    Ok(lines_back(entry, &boxes, &edges))
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
            return Err(Error::Invalid(
                Msg::new("a limit of how often an edge may be taken is at least 1")
                    .coded(ErrorCode::InvalidAutomationLimitBelowOne)
                    .with("value", n),
            ));
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
/// [`crate::model::DEFAULT_MAX_TIMES`] is what the surface above puts there when nobody said. **It is
/// counted only while the edge goes back** ([`lines_back`]), so it is written whichever way the edge
/// goes: one drawn going down can come to go back as the rest of the picture is drawn.
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
    not_built_in(tx, def_of_picture(owner_kind, owner_id))?;
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
        return Err(Error::invalid(format!("'{}' already says what happens after it", exit.name)));
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
    not_built_in(tx, def_of_picture(before.owner_kind, before.owner_id))?;
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
        // The limit was the old line's, counting how often it went on to a box. A line that now ends
        // the run, or leaves the action, is taken once, so the limit goes with the box it counted —
        // unless one is given here too, which is refused below as saying two things at once.
        if ends != AutomationEnds::Go {
            after.max_times = None;
        }
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
    not_built_in(tx, def_of_picture(edge.owner_kind, edge.owner_id))?;
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
    draw_wire(tx, owner_kind, from_id, from_exit_name, from_port_name, to_id, to_port_name).map(|(wire, _)| wire)
}

/// [`wire_add`], saying besides whether the wire was drawn just now: `false` where the same one was
/// there already, which is answered as it stands and written nothing for. That is what lets a person
/// who draws it again be told nothing was added rather than that it was.
pub fn draw_wire(
    tx: &WriteTx<'_>,
    owner_kind: AutomationPictureOwner,
    from_id: i64,
    from_exit_name: Option<&str>,
    from_port_name: &str,
    to_id: i64,
    to_port_name: &str,
) -> Result<(AutomationWire, bool)> {
    let owner_id = wire_picture(tx, owner_kind, from_id, to_id)?;
    not_built_in(tx, def_of_picture(owner_kind, owner_id))?;
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
        out.id,
        to_id,
        into.id,
    )? {
        return Ok((drawn, false));
    }
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "automation_wire")?;
    let wire = AutomationWire {
        id,
        owner_kind,
        owner_id,
        from_id,
        from_exit_id,
        from_port_id: out.id,
        to_id,
        to_port_id: into.id,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_wire(&wire))?;
    Ok((wire, true))
}

/// Delete a wire. The box then reads nothing on that input unless another wire lands on it.
pub fn wire_delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let wire = live_wire(tx, id)?;
    not_built_in(tx, def_of_picture(wire.owner_kind, wire.owner_id))?;
    tx.delete_record("automation_wire", id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{exit_id, mk_project, with_tx};

    /// A `go` edge between two boxes, as [`lines_back`] reads one — nothing but its ends matter there.
    fn line(id: i64, from_id: i64, to_id: i64) -> AutomationEdge {
        let now = Timestamp::now();
        AutomationEdge {
            id,
            owner_kind: AutomationPictureOwner::Action,
            owner_id: 1,
            from_id,
            exit_id: id,
            to_id: Some(to_id),
            ends: AutomationEnds::Go,
            exit_to_id: None,
            max_times: Some(DEFAULT_MAX_TIMES),
            order_key: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// **Of a loop, only the line back to where it began goes back** (`AMB-T-5440`): in 7→8→7, the line
    /// 8→7. The line down 7→8 is taken once per turn and is not what spins.
    #[test]
    fn of_a_loop_only_the_line_back_goes_back() {
        let back = lines_back(Some(7), &[7, 8, 9], &[line(1, 7, 8), line(2, 8, 7), line(3, 8, 9)]);
        assert_eq!(back, BTreeSet::from([2]));
    }

    /// A box that goes back to itself goes back; a line to a box already walked by another way is not
    /// one, however high up it is drawn.
    #[test]
    fn a_line_to_itself_goes_back_and_one_across_does_not() {
        let edges = [line(1, 1, 2), line(2, 1, 3), line(3, 2, 3), line(4, 3, 3)];
        assert_eq!(lines_back(Some(1), &[1, 2, 3], &edges), BTreeSet::from([4]));
    }

    /// **Whether a line goes back turns on the rest of the picture**, which is why the limit is written
    /// on every line into a box: 8→7 goes down from where nothing reaches, until 7→8 is drawn.
    #[test]
    fn a_line_comes_to_go_back_when_the_loop_is_closed() {
        let alone = lines_back(Some(6), &[6, 7, 8], &[line(1, 6, 7), line(2, 8, 7)]);
        assert!(alone.is_empty(), "8 is walked on its own, and 7 is not on its way down");
        let closed = lines_back(Some(6), &[6, 7, 8], &[line(1, 6, 7), line(2, 8, 7), line(3, 7, 8)]);
        assert_eq!(closed, BTreeSet::from([2]));
    }

    /// What the entry does not reach is still read, so a loop drawn before it is joined up goes back
    /// too; with no entry, the walk starts at the first box.
    #[test]
    fn a_loop_nothing_reaches_still_has_a_way_back() {
        let edges = [line(1, 2, 3), line(2, 3, 2)];
        assert_eq!(lines_back(Some(1), &[1, 2, 3], &edges), BTreeSet::from([2]));
        assert_eq!(lines_back(None, &[3, 2], &edges), BTreeSet::from([1]));
    }

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
            NewStep::new(name, "do it"),
            &[],
            &[],
        )
        .expect("write the action");
        let placement = placement_add_by_hand(tx, automation.id, action.id).expect("place it");
        (action, placement)
    }

    /// The one step a freshly written action holds.
    fn only_step(tx: &WriteTx<'_>, action: &AutomationAction) -> AutomationStep {
        live_step(tx, action.entry_step_id.expect("an entry step")).expect("read the step")
    }

    fn exit_names(tx: &WriteTx<'_>, owner: AutomationOwner, owner_id: i64) -> Vec<String> {
        read::automation_exits_of(tx.conn(), owner, owner_id)
            .expect("read exits")
            .into_iter()
            .map(|e| e.name)
            .collect()
    }

    #[test]
    fn a_step_is_born_with_the_way_out_called_done_and_the_error_one() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "実装する");
            let step = only_step(tx, &action);
            assert_eq!(
                exit_names(tx, AutomationOwner::Step, step.id),
                vec![DONE_EXIT.to_string(), ERROR_EXIT.to_string()],
                "both are written at birth, the error one last so it sits at the bottom of the list",
            );
            assert_eq!(
                exit_names(tx, AutomationOwner::Action, action.id),
                vec![DONE_EXIT.to_string(), ERROR_EXIT.to_string()],
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
                NewStep::new("点検する", "見る"),
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
            let port = |owner, owner_id| {
                read::automation_port_by_name(tx.conn(), owner, owner_id, AutomationPortDirection::In, "差分")
                    .expect("read")
                    .expect("the input")
                    .id
            };
            assert_eq!(wires[0].from_port_id, port(AutomationPortOwner::Action, action.id));
            assert_eq!(wires[0].to_id, step.id);
            assert_eq!(wires[0].to_port_id, port(AutomationPortOwner::Step, step.id));
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
    fn a_way_out_does_not_hand_on_the_task_the_run_works() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "点検する");
            let step = only_step(tx, &action);
            for (owner, owner_id) in
                [(AutomationOwner::Step, step.id), (AutomationOwner::Action, action.id)]
            {
                let exit = read::automation_exit_by_name(tx.conn(), owner, owner_id, None)
                    .expect("read the way out")
                    .expect("the done way out");
                let refused = port_add(
                    tx,
                    AutomationPortOwner::Exit,
                    exit.id,
                    AutomationPortDirection::Out,
                    "タスク",
                    AutomationPortKind::TaskTake,
                    true,
                )
                .expect_err("a way out handing on the task taken is refused");
                assert!(format!("{refused}").contains("take_task"), "{refused}");

                // Declared as something else, it cannot be turned into it either.
                let note = port_add(
                    tx,
                    AutomationPortOwner::Exit,
                    exit.id,
                    AutomationPortDirection::Out,
                    "メモ",
                    AutomationPortKind::Value,
                    true,
                )
                .expect("declare a value");
                port_update(tx, note.id, None, Some(AutomationPortKind::TaskTake), None)
                    .expect_err("turning an output into the task taken is refused");
                assert_eq!(live_port(tx, note.id).expect("read").kind, AutomationPortKind::Value);
            }

            // Reading the task a built-in took is an input, and that stays.
            let input = port_add(
                tx,
                AutomationPortOwner::Step,
                step.id,
                AutomationPortDirection::In,
                "タスク",
                AutomationPortKind::TaskTake,
                true,
            )
            .expect("an input reads the task taken");
            port_update(tx, input.id, None, Some(AutomationPortKind::TaskTake), Some(false))
                .expect("an input may stay the task taken");
        });
    }

    #[test]
    fn what_a_step_hands_on_reaches_the_way_out_of_the_action_it_leaves_by() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "点検する");
            let step = only_step(tx, &action);
            // The output both sides declare: on the step's done way out, and on the action's.
            for (owner, owner_id) in
                [(AutomationOwner::Step, step.id), (AutomationOwner::Action, action.id)]
            {
                let exit = read::automation_exit_by_name(tx.conn(), owner, owner_id, None)
                    .expect("read the way out")
                    .expect("the done way out");
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
                NewStep::new("点検する", "見る"),
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
    fn an_action_made_on_a_line_lands_on_the_library_it_was_told_to() {
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

            let mine = placement_insert_new(tx, edge.id, ActionShelf::Project, "下ごしらえ")
                .expect("make it on the project's shelf");
            let onward = edge_on(tx, AutomationPictureOwner::Automation, mine.id, None)
                .expect("read edge")
                .expect("the line the new one goes on along");
            let shared = placement_insert_new(tx, onward.id, ActionShelf::Device, "見直す")
                .expect("make it on the device's shelf");

            assert_eq!(
                live_action(tx, mine.action_id).unwrap().project_id,
                Some(automation.project_id),
                "the project's shelf is the automation's own project",
            );
            assert_eq!(
                live_action(tx, shared.action_id).unwrap().project_id,
                None,
                "and the device's belongs to no project at all",
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
            // leaves the action by its done way out.
            let edge = edge_on(tx,
                AutomationPictureOwner::Action,
                first.id,
                None,
            )
            .expect("read the edge")
            .expect("the action leaves by its done way out");
            assert_eq!(edge.ends, AutomationEnds::Exit);

            let put = step_insert(
                tx,
                edge.id,
                NewStep::new("実装する", "やる"),
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
                vec![DONE_EXIT.to_string(), ERROR_EXIT.to_string(), "直すところがある".to_string()],
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
                step_insert(tx, edge.id, NewStep::new("実装する", "やる"), &[], &[]).is_err(),
                "what stands on an automation is a placement, never a step",
            );
        });
    }

    #[test]
    fn an_action_put_on_after_a_way_out_that_says_nothing_is_pointed_at_and_says_nothing_itself() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");
            let (action, _) = mk_placed(tx, &automation, "実装する");

            let put = placement_insert_at_exit(tx, first.id, None, action.id).expect("put it on");

            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Automation, first.id, None),
                (AutomationEnds::Go, Some(put.id)),
                "the way out that said nothing now opens the new placement",
            );
            assert_eq!(
                edge_on(tx, AutomationPictureOwner::Automation, first.id, None)
                    .unwrap()
                    .unwrap()
                    .max_times,
                Some(DEFAULT_MAX_TIMES),
            );
            assert!(
                edge_on(tx, AutomationPictureOwner::Automation, put.id, None).unwrap().is_none(),
                "and the new placement's own way out is left for the reader to decide",
            );
        });
    }

    #[test]
    fn an_action_made_after_a_way_out_that_says_nothing_is_born_empty_on_the_shelf_it_was_told() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");

            let made = placement_insert_new_at_exit(tx, first.id, None, ActionShelf::Device, "書く")
                .expect("make one there");

            let action = live_action(tx, made.action_id).unwrap();
            assert_eq!((action.name.as_str(), action.project_id, action.entry_step_id), ("書く", None, None));
            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Automation, first.id, None),
                (AutomationEnds::Go, Some(made.id)),
            );
            assert!(edge_on(tx, AutomationPictureOwner::Automation, made.id, None).unwrap().is_none());
        });
    }

    /// **Only an action made on the spot is born still being written** (`AMB-D-1005`).
    #[test]
    fn an_action_made_on_the_spot_is_born_still_being_written_and_no_other_is() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (kept, first) = mk_placed(tx, &automation, "取る");
            let edge = edge_add(tx, AutomationPictureOwner::Automation, first.id, None, EdgeTarget::Done, None)
                .expect("close the task after it");

            let on_line = placement_insert_new(tx, edge.id, ActionShelf::Project, "書く").expect("on a line");
            let after = placement_insert_new_at_exit(tx, on_line.id, Some(ERROR_EXIT), ActionShelf::Project, "直す")
                .expect("after a way out that says nothing");

            assert!(live_action(tx, on_line.action_id).unwrap().draft);
            assert!(live_action(tx, after.action_id).unwrap().draft);
            assert!(!kept.draft, "an action added to the library by itself is not");
        });
    }

    /// **Finishing takes an empty action too, and finishing twice writes nothing** (`AMB-D-1005`).
    #[test]
    fn finishing_an_action_keeps_it_even_when_it_is_empty() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");
            let made = placement_insert_new_at_exit(tx, first.id, None, ActionShelf::Project, "書く")
                .expect("make one");

            let finished = action_finish_creating(tx, made.action_id).expect("finish it, empty as it is");
            assert!(!finished.draft);
            assert_eq!(finished.entry_step_id, None);
            let again = action_finish_creating(tx, made.action_id).expect("finish it again");
            assert_eq!(again.updated_at, finished.updated_at, "nothing is written the second time");
            assert_eq!(live_placement(tx, made.id).unwrap().action_id, made.action_id, "it stays placed");
        });
    }

    /// **Giving up an action made on a line puts the line back** where it went before (`AMB-D-1005`),
    /// with the limit it carried.
    #[test]
    fn giving_up_an_action_made_on_a_line_puts_the_line_back_as_it_was() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");
            let (_, last) = mk_placed(tx, &automation, "見直す");
            let to_last =
                edge_add(tx, AutomationPictureOwner::Automation, first.id, None, EdgeTarget::Go(last.id), Some(3))
                    .expect("go on to the last");
            let to_done =
                edge_add(tx, AutomationPictureOwner::Automation, last.id, None, EdgeTarget::Done, None)
                    .expect("close the task after it");

            let between = placement_insert_new(tx, to_last.id, ActionShelf::Project, "書く").expect("between");
            let closing = placement_insert_new(tx, to_done.id, ActionShelf::Device, "閉じる").expect("at the end");
            action_abandon(tx, between.action_id).expect("give up the one between");
            action_abandon(tx, closing.action_id).expect("give up the one at the end");

            let back = edge_on(tx, AutomationPictureOwner::Automation, first.id, None).unwrap().unwrap();
            assert_eq!((back.ends, back.to_id, back.max_times), (AutomationEnds::Go, Some(last.id), Some(3)));
            let back = edge_on(tx, AutomationPictureOwner::Automation, last.id, None).unwrap().unwrap();
            assert_eq!((back.ends, back.to_id, back.max_times), (AutomationEnds::Done, None, None));
            assert_eq!(read::automation_placement_ids(tx.conn(), automation.id).unwrap(), vec![first.id, last.id]);
            assert!(read::automation_action(tx.conn(), between.action_id).unwrap().is_none());
            assert!(read::automation_action(tx.conn(), closing.action_id).unwrap().is_none());
        });
    }

    /// **Giving up an action made after a way out that said nothing leaves it saying nothing again**
    /// (`AMB-D-1005`).
    #[test]
    fn giving_up_an_action_made_after_a_way_out_leaves_that_way_out_saying_nothing() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");
            let made = placement_insert_new_at_exit(tx, first.id, None, ActionShelf::Project, "書く")
                .expect("make one");

            action_abandon(tx, made.action_id).expect("give it up");

            assert!(edge_on(tx, AutomationPictureOwner::Automation, first.id, None).unwrap().is_none());
            assert_eq!(read::automation_placement_ids(tx.conn(), automation.id).unwrap(), vec![first.id]);
        });
    }

    /// **An action that is not being written is not given up** — deleting it is `action_delete`'s.
    #[test]
    fn an_action_that_is_not_being_written_is_not_given_up() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");
            let made = placement_insert_new_at_exit(tx, first.id, None, ActionShelf::Project, "書く")
                .expect("make one");
            action_finish_creating(tx, made.action_id).expect("finish it");

            let refused = action_abandon(tx, made.action_id).expect_err("it is kept");
            assert!(format!("{refused}").contains("is not being written"), "{refused}");
            assert!(live_placement(tx, made.id).is_ok(), "and it stays placed");
        });
    }

    #[test]
    fn a_way_out_that_already_says_something_takes_nothing_put_on_after_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, first) = mk_placed(tx, &automation, "取る");
            edge_add(tx, AutomationPictureOwner::Automation, first.id, None, EdgeTarget::Done, None)
                .expect("close the task after it");
            let before = read::automation_placement_ids(tx.conn(), automation.id).unwrap().len();

            let refused = placement_insert_new_at_exit(tx, first.id, None, ActionShelf::Project, "書く")
                .expect_err("that way out has a line, and a box goes in on the line");
            assert!(format!("{refused}").contains("already says what happens after it"), "{refused}");
            assert_eq!(
                read::automation_placement_ids(tx.conn(), automation.id).unwrap().len(),
                before,
                "nothing is placed",
            );
        });
    }

    #[test]
    fn a_step_put_on_after_a_way_out_that_says_nothing_is_pointed_at_by_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "取る");
            let first = only_step(tx, &action);
            exit_add(tx, AutomationOwner::Step, first.id, Some("直す")).expect("a way out with no line");

            let put = step_insert_at_exit(
                tx,
                first.id,
                Some("直す"),
                NewStep::new("直す", "やる"),
                &["もう一度".to_string()],
                &[],
            )
            .expect("put it on");

            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Action, first.id, Some("直す")),
                (AutomationEnds::Go, Some(put.id)),
            );
            assert_eq!(
                edge_of(tx, AutomationPictureOwner::Action, first.id, None),
                (AutomationEnds::Exit, None),
                "the other way out keeps what it said",
            );
            assert!(edge_on(tx, AutomationPictureOwner::Action, put.id, None).unwrap().is_none());
            assert_eq!(
                exit_names(tx, AutomationOwner::Step, put.id),
                vec![DONE_EXIT.to_string(), ERROR_EXIT.to_string(), "もう一度".to_string()],
            );
            assert!(
                step_insert_at_exit(tx, first.id, None, NewStep::new("また", "やる"), &[], &[])
                    .is_err(),
                "the done way out already leaves the action",
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
    fn a_way_out_keeps_a_name_and_the_one_left_unsaid_is_done() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "実装する");
            let step = only_step(tx, &action);
            let done = read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, step.id, None)
                .expect("read")
                .expect("a way out left unsaid is the done one");
            assert_eq!(done.name, DONE_EXIT);
            assert!(exit_rename(tx, done.id, None).is_err(), "a way out may not be left without a name");
            exit_delete(tx, done.id).expect("完了 is an ordinary way out, and may go");
            let back = exit_add(tx, AutomationOwner::Step, step.id, None).expect("put it back");
            assert_eq!(back.name, DONE_EXIT, "put back under its name, not without one");
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

    /// The code a refusal names itself by and the fields its sentence is built from, for the refusals
    /// the build screen lets a person walk into — each is written in the reader's language from those
    /// (`AMB-D-413`), so a missing field reads as a hole in the sentence.
    fn coded(err: Error) -> (Option<ErrorCode>, Vec<String>) {
        let Error::Invalid(msg) = err else { panic!("a refusal to build is invalid, not {err:?}") };
        (msg.code(), msg.fields().iter().map(|(key, _)| key.to_string()).collect())
    }

    #[test]
    fn the_refusals_the_build_screen_can_walk_into_name_themselves() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placement) = mk_placed(tx, &automation, "実装する");
            let step = only_step(tx, &action);
            let names = |code| (Some(code), vec!["name".to_string()]);

            let reserved = exit_add(tx, AutomationOwner::Step, step.id, Some(ERROR_EXIT)).unwrap_err();
            assert_eq!(coded(reserved), names(ErrorCode::InvalidAutomationExitReserved));
            let again = exit_add(tx, AutomationOwner::Step, step.id, Some(DONE_EXIT)).unwrap_err();
            assert_eq!(coded(again), names(ErrorCode::InvalidAutomationExitTaken));
            let other = exit_add(tx, AutomationOwner::Step, step.id, Some("直す")).expect("another way out");
            let onto = exit_rename(tx, other.id, Some(DONE_EXIT)).unwrap_err();
            assert_eq!(coded(onto), names(ErrorCode::InvalidAutomationExitTaken));

            let input = || {
                port_add(
                    tx,
                    AutomationPortOwner::Step,
                    step.id,
                    AutomationPortDirection::In,
                    "差分",
                    AutomationPortKind::File,
                    true,
                )
            };
            input().expect("an input");
            assert_eq!(coded(input().unwrap_err()), names(ErrorCode::InvalidAutomationInputTaken));
            let output = || {
                port_add(
                    tx,
                    AutomationPortOwner::Exit,
                    other.id,
                    AutomationPortDirection::Out,
                    "差分",
                    AutomationPortKind::File,
                    true,
                )
            };
            output().expect("an output");
            assert_eq!(coded(output().unwrap_err()), names(ErrorCode::InvalidAutomationOutputTaken));

            cfg_add(tx, action.id, "秒", AutomationCfgKind::Number, false, None).expect("number");
            cfg_add(tx, action.id, "どれ", AutomationCfgKind::Choice, false, Some(r#"["a"]"#))
                .expect("choice");
            for wrong in ["-5", "1.5"] {
                let refused = cfg_set(tx, placement.id, "秒", Some(wrong)).unwrap_err();
                assert_eq!(
                    coded(refused),
                    (Some(ErrorCode::InvalidAutomationCfgNotACount), vec!["cfg".to_string(), "value".to_string()]),
                );
            }
            // A choice is picked from a list on the screen, so a misfit there is a caller's own JSON and
            // keeps the family code.
            assert_eq!(coded(cfg_set(tx, placement.id, "どれ", Some(r#""z""#)).unwrap_err()).0, None);

            let (_, next) = mk_placed(tx, &automation, "点検する");
            let edge = edge_add(
                tx,
                AutomationPictureOwner::Automation,
                placement.id,
                None,
                EdgeTarget::Go(next.id),
                Some(3),
            )
            .expect("go on");
            let below = edge_update(tx, edge.id, None, Some(Some(0))).unwrap_err();
            assert_eq!(
                coded(below),
                (Some(ErrorCode::InvalidAutomationLimitBelowOne), vec!["value".to_string()]),
            );
        });
    }

    #[test]
    fn pointing_a_line_at_an_ending_drops_the_limit_it_carried() {
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
                Some(10),
            )
            .expect("go on, as the screen draws it");
            let done = edge_update(tx, edge.id, Some(EdgeTarget::Done), None)
                .expect("the screen switches the line to an ending and says nothing of the limit");
            assert_eq!((done.ends, done.max_times), (AutomationEnds::Done, None));
            assert!(
                edge_update(tx, edge.id, Some(EdgeTarget::Halt), Some(Some(3))).is_err(),
                "a limit given with an ending is still refused",
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

    /// **A wire keys the ports at its two ends** (`AMB-D-961`): renaming either port leaves the wire on
    /// it, and so does deleting one from the action — the version a placement points at still declares
    /// it (`AMB-D-1000`).
    #[test]
    fn an_automations_wire_stays_on_a_renamed_port_and_on_a_deleted_one() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (from_action, from) = mk_placed(tx, &automation, "実装する");
            let (to_action, to) = mk_placed(tx, &automation, "点検する");
            let exit = read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, from_action.id, None)
                .expect("read")
                .expect("the done way out");
            let out = port_add(
                tx,
                AutomationPortOwner::Exit,
                exit.id,
                AutomationPortDirection::Out,
                "差分",
                AutomationPortKind::File,
                true,
            )
            .expect("declare the output");
            let into = port_add(
                tx,
                AutomationPortOwner::Action,
                to_action.id,
                AutomationPortDirection::In,
                "差分",
                AutomationPortKind::File,
                true,
            )
            .expect("declare the input");
            let on = AutomationPictureOwner::Automation;
            let wire = wire_add(tx, on, from.id, None, "差分", to.id, "差分").expect("wire");
            assert_eq!((wire.from_port_id, wire.to_port_id), (out.id, into.id));

            port_update(tx, out.id, Some("変更点"), None, None).expect("rename the output");
            port_update(tx, into.id, Some("見る差分"), None, None).expect("rename the input");
            assert_eq!(
                read::automation_wire(tx.conn(), wire.id).expect("read").map(|w| (w.from_port_id, w.to_port_id)),
                Some((out.id, into.id)),
                "both renames leave the wire where it was",
            );

            port_delete(tx, into.id).expect("delete the input");
            assert_eq!(
                read::automation_wire(tx.conn(), wire.id).expect("read").map(|w| (w.from_port_id, w.to_port_id)),
                Some((out.id, into.id)),
                "and so does the delete",
            );
        });
    }

    /// **Deleting a way out from an action leaves an automation's line on it** (`AMB-D-1000`): the
    /// version a placement points at still declares it, under the same id (`AMB-D-961`).
    #[test]
    fn an_automations_edge_stays_on_a_way_out_deleted_from_the_action() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placed) = mk_placed(tx, &automation, "点検する");
            let exit =
                exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある")).expect("add exit");
            let edge = edge_add(
                tx,
                AutomationPictureOwner::Automation,
                placed.id,
                Some("直すところがある"),
                EdgeTarget::Done,
                None,
            )
            .expect("add edge");

            exit_delete(tx, exit.id).expect("delete the way out");
            assert_eq!(
                read::automation_edge(tx.conn(), edge.id).expect("read").map(|e| e.exit_id),
                Some(exit.id),
            );
        });
    }

    /// **Moving a placement onto a new version takes off what that version no longer declares**
    /// (`AMB-D-1000`): the edge on a way out it dropped, the wire from an output it dropped, and the
    /// choice for a step it dropped. What it still declares keeps its lines, renamed or not.
    #[test]
    fn moving_a_placement_onto_a_version_drops_the_lines_to_what_it_no_longer_declares() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placed) = mk_placed(tx, &automation, "実装する");
            let (to_action, to) = mk_placed(tx, &automation, "点検する");
            let kept_step = only_step(tx, &action);
            let gone_step = step_add(tx, action.id, NewStep::new("書き直す", "do it")).expect("add step");
            let gone_exit =
                exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある")).expect("add exit");
            let done = read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, action.id, None)
                .expect("read")
                .expect("the done way out");
            let mut outs = Vec::new();
            for name in ["差分", "ログ"] {
                outs.push(
                    port_add(
                        tx,
                        AutomationPortOwner::Exit,
                        done.id,
                        AutomationPortDirection::Out,
                        name,
                        AutomationPortKind::File,
                        true,
                    )
                    .expect("declare the output"),
                );
                port_add(
                    tx,
                    AutomationPortOwner::Action,
                    to_action.id,
                    AutomationPortDirection::In,
                    name,
                    AutomationPortKind::File,
                    true,
                )
                .expect("declare the input");
            }
            let on = AutomationPictureOwner::Automation;
            let kept_wire = wire_add(tx, on, placed.id, None, "差分", to.id, "差分").expect("wire");
            let gone_wire = wire_add(tx, on, placed.id, None, "ログ", to.id, "ログ").expect("wire");
            let kept_edge = edge_add(tx, on, placed.id, None, EdgeTarget::Go(to.id), None).expect("add edge");
            let gone_edge = edge_add(tx, on, placed.id, Some("直すところがある"), EdgeTarget::Done, None)
                .expect("add edge");
            for step in [kept_step.id, gone_step.id] {
                placement_step_set(tx, placed.id, step, "claude", None).expect("choose");
            }
            action_version_add(tx, action.id).expect("save version 1");
            let placed = placement_version_set(tx, placed.id, 1).expect("onto version 1");

            exit_delete(tx, gone_exit.id).expect("delete the way out");
            port_delete(tx, outs[1].id).expect("delete the output");
            port_update(tx, outs[0].id, Some("変更点"), None, None).expect("rename the output");
            step_delete(tx, gone_step.id).expect("delete the step");
            action_version_add(tx, action.id).expect("save version 2");
            let moved = placement_version_set(tx, placed.id, 2).expect("onto version 2");

            assert_eq!(moved.version, Some(2));
            assert_eq!(
                read::automation_placement(tx.conn(), placed.id).expect("read").and_then(|p| p.version),
                Some(2),
            );
            let edges: Vec<i64> =
                read::automation_edges_of(tx.conn(), on, automation.id).expect("read").iter().map(|e| e.id).collect();
            assert!(edges.contains(&kept_edge.id) && !edges.contains(&gone_edge.id), "{edges:?}");
            let wires: Vec<i64> =
                read::automation_wires_of(tx.conn(), on, automation.id).expect("read").iter().map(|w| w.id).collect();
            assert_eq!(wires, vec![kept_wire.id], "the renamed output keeps its wire; {} goes", gone_wire.id);
            let chosen: Vec<i64> = read::automation_placement_steps_of(tx.conn(), placed.id)
                .expect("read")
                .iter()
                .map(|c| c.step_id)
                .collect();
            assert_eq!(chosen, vec![kept_step.id]);
        });
    }

    /// A wire into an input the new version dropped goes with it, as one out of an output does.
    #[test]
    fn moving_a_placement_onto_a_version_drops_the_wire_into_an_input_it_no_longer_declares() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (from_action, from) = mk_placed(tx, &automation, "実装する");
            let (to_action, to) = mk_placed(tx, &automation, "点検する");
            let done = read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, from_action.id, None)
                .expect("read")
                .expect("the done way out");
            port_add(
                tx,
                AutomationPortOwner::Exit,
                done.id,
                AutomationPortDirection::Out,
                "差分",
                AutomationPortKind::File,
                true,
            )
            .expect("declare the output");
            let into = port_add(
                tx,
                AutomationPortOwner::Action,
                to_action.id,
                AutomationPortDirection::In,
                "差分",
                AutomationPortKind::File,
                true,
            )
            .expect("declare the input");
            let on = AutomationPictureOwner::Automation;
            wire_add(tx, on, from.id, None, "差分", to.id, "差分").expect("wire");
            action_version_add(tx, to_action.id).expect("save version 1");
            port_delete(tx, into.id).expect("delete the input");
            action_version_add(tx, to_action.id).expect("save version 2");

            placement_version_set(tx, to.id, 1).expect("onto version 1");
            assert_eq!(read::automation_wires_of(tx.conn(), on, automation.id).expect("read").len(), 1);
            placement_version_set(tx, to.id, 2).expect("onto version 2");
            assert!(read::automation_wires_of(tx.conn(), on, automation.id).expect("read").is_empty());
        });
    }

    /// The version it stands on writes nothing; a version the action does not have, and a built-in,
    /// are refused.
    #[test]
    fn a_placement_moves_only_onto_a_version_its_action_has() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let entry = entry_placed(tx, &automation, automation_builtin::entries()[0]);
            placement_version_set(tx, entry.id, 1).expect_err("a built-in has no versions to move onto");
            let (action, placed) = mk_placed(tx, &automation, "実装する");
            placement_version_set(tx, placed.id, 1).expect_err("nobody has saved version 1");
            action_version_add(tx, action.id).expect("save version 1");
            let moved = placement_version_set(tx, placed.id, 1).expect("onto version 1");
            let again = placement_version_set(tx, placed.id, 1).expect("onto it again");
            assert_eq!(again.updated_at, moved.updated_at, "nothing is written");
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
            .expect("the done way out");
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
            let (wire, drawn) = draw_wire(
                tx,
                AutomationPictureOwner::Automation,
                from.id,
                None,
                "差分",
                to.id,
                "差分",
            )
            .expect("draw the wire");
            assert!(drawn, "the first wire was said to be there already");
            let (again, drawn_again) = draw_wire(
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
            assert!(!drawn_again, "the same wire drawn again was said to be drawn just now");
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
                .expect("the done way out");
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

    /// **A task filter naming an axis or a value that is not there is refused when it is written**
    /// (`AMB-T-5551`), not when a run reads it — and two `dim` values are two tokens, so naming two
    /// axes that are there goes through.
    #[test]
    fn a_task_filter_is_refused_for_an_axis_or_value_that_is_not_there() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placement) = mk_placed(tx, &automation, "取る");
            cfg_add(tx, action.id, "受信箱", AutomationCfgKind::TaskFilter, true, None)
                .expect("declare the setting");
            for (axis, value) in [("テーマ", "メイン"), ("フェーズ", "運用第2期")] {
                let dim = crate::ops::dimension::add(
                    tx,
                    automation.project_id,
                    crate::ops::dimension::NewDimension {
                        name: axis.to_string(),
                        notes: String::new(),
                        cardinality: crate::model::DimensionCardinality::Single,
                        ordered: false,
                        role: crate::model::DimensionRole::None,
                        show_on_card: false,
                        required: false,
                        applies_to: crate::model::DimensionAppliesTo::Both,
                        sequential: false,
                        slug: None,
                    },
                )
                .expect("add the axis");
                crate::ops::dimension::value_add(tx, dim.id, value, None).expect("add the value");
            }
            cfg_set(
                tx,
                placement.id,
                "受信箱",
                Some(r#"{"dim":["テーマ=メイン","フェーズ=運用第2期"]}"#),
            )
            .expect("two axes that are there");
            assert!(cfg_set(tx, placement.id, "受信箱", Some(r#"{"dim":["テーマ=ない"]}"#)).is_err(), "no such value");
            assert!(cfg_set(tx, placement.id, "受信箱", Some(r#"{"dim":["ない=メイン"]}"#)).is_err(), "no such axis");
        });
    }

    /// **A renamed setting keeps every placement's answer** (`AMB-T-5657`): the answer is read by the
    /// declaration's name, so one left under the old name would drop out of view.
    #[test]
    fn a_renamed_setting_carries_every_placements_answer() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, here) = mk_placed(tx, &automation, "数える");
            let there = placement_add(tx, automation.id, action.id).expect("place it again");
            let declared = cfg_add(tx, action.id, "数", AutomationCfgKind::Text, false, None)
                .expect("declare it");
            cfg_set(tx, here.id, "数", Some("\"abc\"")).expect("answer here");
            cfg_update(tx, declared.id, Some("個数"), None, None, None).expect("rename it");
            let answer = |placement_id: i64, name: &str| {
                read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, placement_id, name)
                    .expect("read")
            };
            assert_eq!(answer(here.id, "個数").and_then(|a| a.value).as_deref(), Some("\"abc\""));
            assert!(answer(here.id, "数").is_none(), "nothing is left under the old name");
            assert!(answer(there.id, "個数").is_none(), "an unanswered placement stays unanswered");
        });
    }

    /// A rename that would land an answer on a row already there is refused, and changes nothing; an
    /// answer is not renamed away from the name its action declares.
    #[test]
    fn a_rename_that_cannot_carry_the_answer_is_refused() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placement) = mk_placed(tx, &automation, "数える");
            let declared = cfg_add(tx, action.id, "数", AutomationCfgKind::Text, false, None)
                .expect("declare it");
            let answered = cfg_set(tx, placement.id, "数", Some("\"abc\"")).expect("answer it");
            assert!(
                cfg_update(tx, answered.id, Some("個数"), None, None, None).is_err(),
                "an answer takes its name from the declaration",
            );
            let old = cfg_add(tx, action.id, "個数", AutomationCfgKind::Text, false, None)
                .expect("declare the other");
            cfg_set(tx, placement.id, "個数", Some("\"def\"")).expect("answer the other");
            cfg_delete(tx, old.id).expect("its declaration goes, its answer stays");
            assert!(cfg_update(tx, declared.id, Some("個数"), None, None, None).is_err());
            let kept = read::automation_cfg(tx.conn(), declared.id).expect("read").expect("the row");
            assert_eq!(kept.name, "数", "the refused rename wrote nothing");
            let answer =
                read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, placement.id, "数")
                    .expect("read")
                    .expect("the answer");
            assert_eq!(answer.value.as_deref(), Some("\"abc\""));
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

    /// **An answer is refused when it is not what its kind takes** — the task filter answered with a
    /// choice, which the take then read as no narrowing and took a person's task with (`AMB-T-5648`), a
    /// number below zero or not whole, a choice that is not listed, a string that is not one.
    #[test]
    fn an_answer_its_kind_does_not_take_is_refused() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, placement) = mk_placed(tx, &automation, "取る");
            cfg_add(tx, action.id, "受信箱", AutomationCfgKind::TaskFilter, false, None).expect("filter");
            cfg_add(tx, action.id, "秒", AutomationCfgKind::Number, false, None).expect("number");
            cfg_add(tx, action.id, "どれ", AutomationCfgKind::Choice, false, Some(r#"["a","b"]"#))
                .expect("choice");
            cfg_add(tx, action.id, "観点", AutomationCfgKind::Text, false, None).expect("text");
            for (name, wrong) in [
                ("受信箱", r#""x""#),
                ("受信箱", "[]"),
                ("受信箱", r#"{"priority":3}"#),
                ("受信箱", r#"{"sort":"nowhere"}"#),
                ("受信箱", r#"{"nosuch":["x"]}"#),
                ("秒", "-5"),
                ("秒", "1.5"),
                ("秒", r#""5""#),
                ("どれ", r#""c""#),
                ("どれ", "1"),
                ("観点", "7"),
                ("観点", "not json"),
            ] {
                assert!(cfg_set(tx, placement.id, name, Some(wrong)).is_err(), "{name} took {wrong}");
            }
            for (name, right) in [
                ("受信箱", r#"{"priority":["high"],"sort":"due"}"#),
                ("秒", "0"),
                ("どれ", r#""b""#),
                ("観点", r#""速さ""#),
            ] {
                cfg_set(tx, placement.id, name, Some(right)).unwrap_or_else(|e| panic!("{name} {right}: {e}"));
            }
        });
    }

    /// **A choice list is a JSON array of distinct choices, at least one of them.**
    #[test]
    fn a_broken_list_of_choices_is_refused() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, _) = mk_placed(tx, &automation, "取る");
            for broken in ["notjson", "[]", r#"["a","a"]"#, r#"["a",""]"#, r#"["a",1]"#, r#""a""#] {
                let refused = cfg_add(tx, action.id, "どれ", AutomationCfgKind::Choice, false, Some(broken));
                assert!(refused.is_err(), "{broken}");
            }
            let cfg = cfg_add(tx, action.id, "どれ", AutomationCfgKind::Choice, false, Some(r#"["a","b"]"#))
                .expect("a list that is one");
            assert!(cfg_update(tx, cfg.id, None, None, None, Some(Some("[]"))).is_err(), "nor by an update");
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
                NewStep::new("点検する", "look"),
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
                        DONE_EXIT.to_string(),
                        ERROR_EXIT.to_string(),
                        "直すところがある".to_string()
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
            entry_placed(tx, &automation, "take_task");
            placement_add(tx, automation.id, mine.id).expect("mine");
            placement_add(tx, automation.id, shared.id).expect("shared");
            assert!(
                placement_add(tx, automation.id, theirs.id).is_err(),
                "another project's prompt is not read across the boundary",
            );
        });
    }

    /// The built-in of `key`, placed on an automation with nothing on it — which makes it the entry.
    fn entry_placed(tx: &WriteTx<'_>, automation: &Automation, key: &str) -> AutomationPlacement {
        let action = automation_builtin::action(tx, key).expect("the built-in's action");
        placement_add(tx, automation.id, action.id).expect("place the entry")
    }

    /// **The first placement is the entry, and only a built-in a run can start at is one**
    /// (`AMB-D-977`). Anything else is refused there, naming the ones there are; once the entry
    /// stands, any action goes on after it.
    #[test]
    fn the_first_placement_is_the_entry_and_one_of_the_built_ins_a_run_starts_at() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let written = action_add(tx, Some(automation.project_id), "調べる", "").expect("add action");
            let refused = placement_add(tx, automation.id, written.id).expect_err("not a start");
            for key in automation_builtin::entries() {
                assert!(refused.to_string().contains(key), "{refused}");
            }
            let close = automation_builtin::action(tx, "close_task").expect("built-in");
            assert!(placement_add(tx, automation.id, close.id).is_err(), "a built-in a run does not start at");
            assert!(read::automation_placement_ids(tx.conn(), automation.id).unwrap().is_empty());

            for key in automation_builtin::entries() {
                let other = add(tx, automation.project_id, NewAutomation { name: (*key).into(), ..Default::default() })
                    .expect("add automation");
                let entry = entry_placed(tx, &other, key);
                assert_eq!(live_automation(tx, other.id).unwrap().entry_placement_id, Some(entry.id));
            }

            let entry = entry_placed(tx, &automation, "fetch");
            let after = placement_add(tx, automation.id, written.id).expect("anything goes on after it");
            let automation = live_automation(tx, automation.id).unwrap();
            assert_eq!(automation.entry_placement_id, Some(entry.id), "the second is not the entry");
            assert_ne!(after.id, entry.id);
        });
    }

    /// **The entry comes off last** (`AMB-D-977`): refused while anything else is on the picture,
    /// and taken off alone it leaves the picture empty, the next placement being the entry again.
    #[test]
    fn the_entry_comes_off_last() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let one = entry_placed(tx, &automation, "take_task");
            let written = action_add(tx, Some(automation.project_id), "実装する", "").expect("add action");
            let two = placement_add(tx, automation.id, written.id).expect("place");
            edge_add(tx, AutomationPictureOwner::Automation, one.id, Some("着手した"), EdgeTarget::Go(two.id), None)
                .expect("add edge");
            let refused = placement_delete(tx, one.id).expect_err("others are on it");
            assert!(refused.to_string().contains("entry-replace"), "{refused}");

            placement_delete(tx, two.id).expect("take the other off");
            assert!(read::automation_edge_ids(tx.conn(), AutomationPictureOwner::Automation, automation.id)
                .unwrap()
                .is_empty());
            placement_delete(tx, one.id).expect("the entry alone comes off");
            assert_eq!(live_automation(tx, automation.id).unwrap().entry_placement_id, None);
            assert!(read::automation_placement_ids(tx.conn(), automation.id).unwrap().is_empty());
            let again = entry_placed(tx, &automation, "make_task");
            assert_eq!(live_automation(tx, automation.id).unwrap().entry_placement_id, Some(again.id));
        });
    }

    /// **Replacing the entry keeps its spot and what comes after it** (`AMB-D-977`): the lines out
    /// of the old one's ways out, its wires and its answers go; the lines to it and the placements
    /// after it stay.
    #[test]
    fn replacing_the_entry_keeps_its_spot_and_drops_what_the_old_one_declared() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let on = AutomationPictureOwner::Automation;
            let entry = entry_placed(tx, &automation, "fetch");
            let after_action = action_from_prompt(
                tx,
                Some(automation.project_id),
                NewStep::new("読む", "read it"),
                &[],
                &[],
            )
            .expect("write the action");
            let after = placement_add(tx, automation.id, after_action.id).expect("place");
            crate::ops::test_support::mk_in(tx, &after_action, "中身", AutomationPortKind::Value, false);
            edge_add(tx, on, entry.id, Some("取ってきた"), EdgeTarget::Go(after.id), None).expect("out");
            wire_add(tx, on, entry.id, Some("取ってきた"), "中身", after.id, "中身").expect("wire");
            edge_add(tx, on, after.id, None, EdgeTarget::Go(entry.id), None).expect("back to the entry");
            cfg_set(tx, entry.id, "形式", Some("\"URL\"")).expect("answer");

            let replaced = entry_replace(tx, automation.id, "take_task").expect("replace");
            assert_eq!(replaced.id, entry.id, "the spot is kept");
            let take = automation_builtin::action(tx, "take_task").expect("built-in");
            assert_eq!(live_placement(tx, entry.id).unwrap().action_id, take.id);
            assert_eq!(live_automation(tx, automation.id).unwrap().entry_placement_id, Some(entry.id));
            assert!(read::automation_edges_from(tx.conn(), on, entry.id).unwrap().is_empty(), "lines out go");
            assert!(read::automation_wire_ids_naming_box(tx.conn(), on, entry.id).unwrap().is_empty(), "wires go");
            assert!(
                read::automation_cfg_ids(tx.conn(), AutomationCfgOwner::Placement, entry.id).unwrap().is_empty(),
                "the old one's answers go",
            );
            assert_eq!(
                read::automation_edges_from(tx.conn(), on, after.id).unwrap().len(),
                1,
                "the line to the entry stays",
            );
            assert!(read::automation_placement(tx.conn(), after.id).unwrap().is_some(), "what comes after stays");

            assert!(entry_replace(tx, automation.id, "close_task").is_err(), "not one a run starts at");
            assert!(entry_replace(tx, automation.id, "no_such").is_err());
            let empty = add(tx, automation.project_id, NewAutomation { name: "空".into(), ..Default::default() })
                .expect("add automation");
            assert!(entry_replace(tx, empty.id, "take_task").is_err(), "the first placement chooses it");
        });
    }

    /// **The first action after an entry that files a task brings the built-in that closes it**
    /// (`AMB-T-5797`): put on after the action's done way out, ending the run — and only where nothing
    /// the reader decided would be undone by it.
    #[test]
    fn the_first_action_after_filing_a_task_brings_the_one_that_closes_it() {
        use crate::ops::automation_builtin_make::MADE_AND_TAKEN;
        let on = AutomationPictureOwner::Automation;
        let action = |tx: &WriteTx<'_>, automation: &Automation| {
            action_from_prompt(tx, Some(automation.project_id), NewStep::new("書く", "write it"), &[], &[])
                .expect("write the action")
        };
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let entry = entry_placed(tx, &automation, "make_task");
            let write = action(tx, &automation);
            let placed = placement_insert_at_exit(tx, entry.id, Some(MADE_AND_TAKEN), write.id).expect("place");
            let closer = close_after_filed(tx, entry.id, Some(MADE_AND_TAKEN), &placed)
                .expect("close")
                .expect("put on");
            let close = automation_builtin::action(tx, "close_task").expect("built-in");
            assert_eq!(closer.action_id, close.id);
            let out = read::automation_edges_from(tx.conn(), on, placed.id).unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].to_id, Some(closer.id), "the action's done way out goes to it");
            let end = read::automation_edges_from(tx.conn(), on, closer.id).unwrap();
            assert_eq!(end.len(), 1);
            assert_eq!(end[0].ends, AutomationEnds::Done, "and it ends the run");

            // A second action after the entry finds the task already closed on the picture.
            let again = action(tx, &automation);
            let other = placement_add(tx, automation.id, again.id).expect("place");
            assert!(close_after_filed(tx, entry.id, Some(MADE_AND_TAKEN), &other).unwrap().is_none());
        });
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let entry = entry_placed(tx, &automation, "make_task");
            let write = action(tx, &automation);
            let placed = placement_insert_at_exit(tx, entry.id, Some(ERROR_EXIT), write.id).expect("place");
            assert!(
                close_after_filed(tx, entry.id, Some(ERROR_EXIT), &placed).unwrap().is_none(),
                "not after the entry's error way out",
            );
        });
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let entry = entry_placed(tx, &automation, "take_task");
            let write = action(tx, &automation);
            let placed = placement_insert_at_exit(tx, entry.id, Some("着手した"), write.id).expect("place");
            assert!(
                close_after_filed(tx, entry.id, Some("着手した"), &placed).unwrap().is_none(),
                "an entry that takes a task is left to the reader",
            );
        });
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let entry = entry_placed(tx, &automation, "make_task");
            let write = action(tx, &automation);
            let placed = placement_insert_at_exit(tx, entry.id, Some(MADE_AND_TAKEN), write.id).expect("place");
            edge_add(tx, on, placed.id, None, EdgeTarget::Halt, None).expect("decided");
            assert!(
                close_after_filed(tx, entry.id, Some(MADE_AND_TAKEN), &placed).unwrap().is_none(),
                "a done way out the reader decided is left alone",
            );
        });
    }

    /// **The built-in that files a task, standing first, takes the task it files** (`AMB-T-5795`): left
    /// not started, the run would work no task and the launch check would refuse it. Placed first or
    /// replaced in, it comes answered that way; anywhere else it is left unanswered, and so is any
    /// other entry.
    #[test]
    fn filing_a_task_as_the_entry_takes_the_task_it_files() {
        use crate::ops::automation_builtin_make::{TAKE_IT, WHAT_THEN};
        let answer = |tx: &WriteTx<'_>, placement: i64| {
            read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, placement, WHAT_THEN)
                .unwrap()
                .and_then(|cfg| cfg.value)
        };
        let take = Some(serde_json::to_string(TAKE_IT).unwrap());
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let entry = entry_placed(tx, &automation, "make_task");
            assert_eq!(answer(tx, entry.id), take, "placed first");

            let make = automation_builtin::action(tx, "make_task").expect("built-in");
            let later = placement_add(tx, automation.id, make.id).expect("place it again");
            assert_eq!(answer(tx, later.id), None, "not first, so not answered");

            let other = add(tx, automation.project_id, NewAutomation { name: "差し替え".into(), ..Default::default() })
                .expect("add automation");
            let fetch = entry_placed(tx, &other, "fetch");
            let replaced = entry_replace(tx, other.id, "make_task").expect("replace");
            assert_eq!(replaced.id, fetch.id);
            assert_eq!(answer(tx, replaced.id), take, "replaced in");
            entry_replace(tx, other.id, "take_task").expect("replace again");
            assert!(
                read::automation_cfg_ids(tx.conn(), AutomationCfgOwner::Placement, fetch.id).unwrap().is_empty(),
                "the answer goes with it",
            );
        });
    }

    /// **A picture kept with placements and no entry takes one by replacing** — the built-in is put
    /// down on its own and named the entry, since nothing else names one any more.
    #[test]
    fn a_picture_with_no_entry_takes_one_by_replacing() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, kept) = mk_placed(tx, &automation, "取る");
            let placed = entry_replace(tx, automation.id, "make_task").expect("replace");
            assert_ne!(placed.id, kept.id);
            assert_eq!(live_automation(tx, automation.id).unwrap().entry_placement_id, Some(placed.id));
            assert_eq!(read::automation_placement_ids(tx.conn(), automation.id).unwrap().len(), 2);
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

    /// `AMB-T-5517`: the first step written into an empty action becomes its entry, and a later one does
    /// not take the entry away from it.
    #[test]
    fn the_first_step_of_an_action_is_its_entry() {
        with_tx(|tx| {
            let action = action_add(tx, None, "取る", "").expect("add action");
            let first = step_add(tx, action.id, NewStep::new("一つ目", "one")).expect("add step");
            assert_eq!(live_action(tx, action.id).unwrap().entry_step_id, Some(first.id));
            step_add(tx, action.id, NewStep::new("二つ目", "two")).expect("add step");
            assert_eq!(
                live_action(tx, action.id).unwrap().entry_step_id,
                Some(first.id),
                "a second step leaves the entry where it was",
            );
        });
    }

    /// A program named by its full path, wherever this machine keeps one.
    fn a_program() -> String {
        std::env::temp_dir().join("check.sh").to_string_lossy().into_owned()
    }

    fn a_script(timeout_minutes: Option<i64>) -> NewScript {
        NewScript {
            program: a_program(),
            args: vec!["--filter".into(), "a b".into(), String::new()],
            timeout_minutes,
        }
    }

    /// `AMB-D-1016`: a script step keeps its program, its arguments as they were written — spaces and
    /// an empty one included — and its timeout, and reads back as it was saved.
    #[test]
    fn a_script_step_reads_back_as_it_was_saved() {
        with_tx(|tx| {
            let action = action_add(tx, None, "確かめる", "").expect("add action");
            let step = step_add(
                tx,
                action.id,
                NewStep { script: Some(a_script(Some(90))), ..NewStep::new("CI を待つ", "") },
            )
            .expect("add a script step");
            let saved = StepScript {
                program: a_program(),
                args: vec!["--filter".into(), "a b".into(), String::new()],
                timeout_minutes: 90,
            };
            assert_eq!(step.script.as_ref(), Some(&saved));
            assert_eq!(live_step(tx, step.id).unwrap().script, Some(saved));

            let agents = step_add(tx, action.id, NewStep::new("読む", "read")).expect("add step");
            assert_eq!(live_step(tx, agents.id).unwrap().script, None, "a step an agent carries out is not one");
        });
    }

    /// A script whose timeout is not written runs for the default, and one over the ceiling, or under a
    /// minute, is refused.
    #[test]
    fn a_script_runs_for_thirty_minutes_unless_told_and_never_past_six_hours() {
        with_tx(|tx| {
            let action = action_add(tx, None, "確かめる", "").expect("add action");
            let script = |t| NewStep { script: Some(a_script(t)), ..NewStep::new("待つ", "") };
            let step = step_add(tx, action.id, script(None)).expect("add a script step");
            assert_eq!(step.script.map(|s| s.timeout_minutes), Some(DEFAULT_SCRIPT_TIMEOUT_MINUTES));
            assert_eq!(DEFAULT_SCRIPT_TIMEOUT_MINUTES, 30);
            assert_eq!(MAX_SCRIPT_TIMEOUT_MINUTES, 360);
            step_add(tx, action.id, script(Some(MAX_SCRIPT_TIMEOUT_MINUTES))).expect("six hours is allowed");
            for refused in [MAX_SCRIPT_TIMEOUT_MINUTES + 1, 0, -5] {
                assert!(
                    matches!(step_add(tx, action.id, script(Some(refused))), Err(Error::Invalid(_))),
                    "{refused} minutes"
                );
            }
        });
    }

    /// A program not named by its full path is refused, and so is an argument of more than one line.
    #[test]
    fn a_script_names_its_program_by_its_full_path_and_one_argument_a_line() {
        with_tx(|tx| {
            let action = action_add(tx, None, "確かめる", "").expect("add action");
            for program in ["check.sh", "scripts/check.sh", ""] {
                let new = NewScript { program: program.into(), args: Vec::new(), timeout_minutes: None };
                assert!(
                    matches!(
                        step_add(tx, action.id, NewStep { script: Some(new), ..NewStep::new("待つ", "") }),
                        Err(Error::Invalid(_))
                    ),
                    "{program:?}"
                );
            }
            let two_lines = NewScript { program: a_program(), args: vec!["a\nb".into()], timeout_minutes: None };
            assert!(matches!(
                step_add(tx, action.id, NewStep { script: Some(two_lines), ..NewStep::new("待つ", "") }),
                Err(Error::Invalid(_))
            ));
        });
    }

    /// A step is turned into a script and back by rewriting it, and the rest of it is left as it was.
    #[test]
    fn a_step_is_turned_into_a_script_and_back() {
        with_tx(|tx| {
            let action = action_add(tx, None, "確かめる", "").expect("add action");
            let step = step_add(tx, action.id, NewStep::new("待つ", "wait")).expect("add step");
            let update = |script| {
                step_update(tx, step.id, None, None, None, None, None, None, None, None, None, script)
            };
            update(Some(Some(a_script(None)))).expect("make it a script");
            let read = live_step(tx, step.id).unwrap();
            assert_eq!(read.script.map(|s| s.timeout_minutes), Some(DEFAULT_SCRIPT_TIMEOUT_MINUTES));
            assert_eq!(read.prompt, "wait");

            update(None).expect("leave the script alone");
            assert!(live_step(tx, step.id).unwrap().script.is_some());

            let refused = NewScript { timeout_minutes: Some(361), ..a_script(None) };
            assert!(matches!(update(Some(Some(refused))), Err(Error::Invalid(_))));

            update(Some(None)).expect("back to an agent's step");
            assert_eq!(live_step(tx, step.id).unwrap().script, None);
        });
    }

    /// `AMB-T-5517`: deleting the entry hands it to the first of the steps left, in list order — not
    /// to the oldest, and not to none.
    #[test]
    fn deleting_the_entry_step_hands_the_entry_to_the_first_one_left() {
        with_tx(|tx| {
            let action = action_add(tx, None, "取る", "").expect("add action");
            let first = step_add(tx, action.id, NewStep::new("一つ目", "one")).expect("add step");
            let second = step_add(tx, action.id, NewStep::new("二つ目", "two")).expect("add step");
            let third = step_add(tx, action.id, NewStep::new("三つ目", "three")).expect("add step");
            step_move(tx, third.id, Position::Top).expect("move the third to the top");
            step_delete(tx, first.id).expect("delete the entry");
            assert_eq!(live_action(tx, action.id).unwrap().entry_step_id, Some(third.id));
            step_delete(tx, second.id).expect("delete a step that is not the entry");
            assert_eq!(live_action(tx, action.id).unwrap().entry_step_id, Some(third.id));
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
            let refused = action_delete(tx, action.id).expect_err("placed");
            // The row it is pressed on shows the refusal in the reader's language, so it names itself
            // and carries the count its sentence is written with.
            assert_eq!(refused.code(), "invalid_action_still_placed");
            let fields: Vec<_> = refused.fields().expect("values ride along").iter().collect();
            assert_eq!(fields, vec![("count", "1")]);
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

    /// The rows one JSON field of a saved version holds.
    fn saved<T: serde::de::DeserializeOwned>(json: &str) -> Vec<T> {
        serde_json::from_str(json).expect("a saved version reads back")
    }

    /// **A saved version is the action as it stood**, every row under its own id, and writing on in the
    /// action afterwards leaves it as it was.
    #[test]
    fn a_saved_version_keeps_the_action_as_it_stood() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let action = action_from_prompt(
                tx,
                Some(project),
                NewStep::new("点検する", "見る"),
                &["直すところがある".to_string()],
                &[("差分".to_string(), AutomationPortKind::File, true)],
            )
            .expect("write the action");
            let step = only_step(tx, &action);
            let fix = exit_id(tx, AutomationOwner::Step, step.id, Some("直すところがある"));
            let found = port_add(
                tx,
                AutomationPortOwner::Exit,
                fix,
                AutomationPortDirection::Out,
                "指摘",
                AutomationPortKind::Value,
                true,
            )
            .expect("declare an output");
            cfg_add(tx, action.id, "リポジトリの場所", AutomationCfgKind::Folder, true, None)
                .expect("declare a setting");

            let first = action_version_add(tx, action.id).expect("save");
            assert_eq!(first.version, 1);
            assert_eq!(first.entry_step_id, Some(step.id));
            let steps: Vec<AutomationStep> = saved(&first.steps);
            assert_eq!(
                steps.iter().map(|s| (s.id, s.prompt.as_str())).collect::<Vec<_>>(),
                vec![(step.id, "見る")]
            );
            let mut exits: Vec<i64> = saved::<AutomationExit>(&first.exits).iter().map(|e| e.id).collect();
            exits.sort_unstable();
            let owners = [(AutomationOwner::Action, action.id), (AutomationOwner::Step, step.id)];
            let mut live: Vec<i64> = owners
                .into_iter()
                .flat_map(|(owner, id)| read::automation_exit_ids(tx.conn(), owner, id).expect("read"))
                .collect();
            live.sort_unstable();
            assert_eq!(exits, live, "the ways out of the action and of its step");
            let ports: Vec<AutomationPort> = saved(&first.ports);
            assert!(ports.iter().any(|p| p.id == found.id), "the output on the step's way out");
            assert_eq!(
                ports.iter().filter(|p| p.direction == AutomationPortDirection::In).count(),
                2,
                "the action's input and the step's"
            );
            let cfgs: Vec<AutomationCfg> = saved(&first.cfgs);
            assert_eq!(cfgs.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["リポジトリの場所"]);
            let edges: Vec<AutomationEdge> = saved(&first.edges);
            let live_edges =
                read::automation_edge_ids(tx.conn(), AutomationPictureOwner::Action, action.id).expect("read");
            assert_eq!(edges.len(), live_edges.len());
            let wires: Vec<AutomationWire> = saved(&first.wires);
            assert_eq!(wires.len(), 1, "the input carried from the action into the step");

            let prompt = Some("もう一度見る");
            step_update(tx, step.id, None, prompt, None, None, None, None, None, None, None, None)
                .expect("write on");
            port_delete(tx, found.id).expect("take the output away");
            let second = action_version_add(tx, action.id).expect("save again");
            assert_eq!(second.version, 2);
            assert_eq!(saved::<AutomationStep>(&second.steps)[0].prompt, "もう一度見る");
            assert!(!saved::<AutomationPort>(&second.ports).iter().any(|p| p.id == found.id));

            let latest = read::automation_action_version_latest(tx.conn(), action.id).expect("read");
            assert_eq!(latest.map(|v| v.version), Some(2));
            assert_eq!(saved::<AutomationStep>(&first.steps)[0].prompt, "見る", "the first stays as saved");
        });
    }

    /// A placement stands on the newest version there is when it is put down, and keeps it after the
    /// action is saved again; one put down before any save, and a built-in's, stand on none.
    #[test]
    fn a_placement_stands_on_the_newest_version_when_it_is_put_down() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let take = automation_builtin::action(tx, "take_task").expect("built-in");
            let first = placement_add(tx, automation.id, take.id).expect("place the built-in");
            assert_eq!(first.version, None, "a built-in's action is already one version");
            let (action, unsaved) = mk_placed(tx, &automation, "点検");
            assert_eq!(unsaved.version, None, "nobody has saved the action yet");
            action_version_add(tx, action.id).expect("save");
            let saved = placement_add(tx, automation.id, action.id).expect("place it again");
            assert_eq!(saved.version, Some(1));
            action_version_add(tx, action.id).expect("save again");
            let kept = read::automation_placement(tx.conn(), saved.id).expect("read").expect("there");
            assert_eq!(kept.version, Some(1), "a placement is not moved onto a newer version by itself");
            assert!(action_version_add(tx, take.id).is_err(), "a built-in is not saved by hand");
        });
    }

    /// Deleting an action takes the versions saved of it.
    #[test]
    fn the_versions_of_an_action_go_with_it() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let action = action_from_prompt(tx, Some(project), NewStep::new("点検する", "見る"), &[], &[])
                .expect("write the action");
            action_version_add(tx, action.id).expect("save");
            action_delete(tx, action.id).expect("delete");
            assert!(read::automation_action_version_ids(tx.conn(), action.id).expect("read").is_empty());
        });
    }

    /// **A saved version is the automation's picture as it stood**, every row under its own id, and
    /// placing on afterwards leaves it as it was.
    #[test]
    fn a_saved_version_keeps_the_automation_as_it_stood() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, here) = mk_placed(tx, &automation, "点検");
            let step = only_step(tx, &action);
            placement_step_set(tx, here.id, step.id, "claude", Some("opus")).expect("choose");

            let first = version_add(tx, automation.id).expect("save");
            assert_eq!(first.version, 1);
            let placements: Vec<AutomationPlacement> = saved(&first.placements);
            assert_eq!(placements.iter().map(|p| p.id).collect::<Vec<_>>(), vec![here.id]);
            let chosen: Vec<AutomationPlacementStep> = saved(&first.placement_steps);
            assert_eq!(
                chosen.iter().map(|c| (c.step_id, c.agent.as_str())).collect::<Vec<_>>(),
                vec![(step.id, "claude")]
            );
            let edges: Vec<AutomationEdge> = saved(&first.edges);
            let live =
                read::automation_edge_ids(tx.conn(), AutomationPictureOwner::Automation, automation.id)
                    .expect("read");
            assert_eq!(edges.len(), live.len());

            let (_, there) = mk_placed(tx, &automation, "直す");
            let second = version_add(tx, automation.id).expect("save again");
            assert_eq!(second.version, 2);
            assert_eq!(saved::<AutomationPlacement>(&second.placements).len(), 2);
            assert!(saved::<AutomationPlacement>(&second.placements).iter().any(|p| p.id == there.id));

            let latest = read::automation_version_latest(tx.conn(), automation.id).expect("read");
            assert_eq!(latest.map(|v| v.version), Some(2));
            assert_eq!(saved::<AutomationPlacement>(&first.placements).len(), 1, "the first stays as saved");
        });
    }

    /// Deleting an automation takes the versions saved of it.
    #[test]
    fn the_versions_of_an_automation_go_with_it() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            mk_placed(tx, &automation, "点検");
            version_add(tx, automation.id).expect("save");
            delete(tx, automation.id).expect("delete");
            assert!(read::automation_version_ids(tx.conn(), automation.id).expect("read").is_empty());
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
            assert_eq!(refused.code(), "invalid_action_placed_elsewhere");
            let fields: Vec<_> = refused.fields().expect("values ride along").iter().collect();
            assert_eq!(fields, vec![("automations", here.name.as_str()), ("project", "別")]);
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

    /// **Who carries a step out is chosen where the action is placed** (`AMB-D-960`): one row per
    /// placement and step, rewritten when chosen again, and each placement of the same action answers
    /// for itself.
    #[test]
    fn a_step_s_agent_is_chosen_per_placement_and_rewritten_in_place() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, here) = mk_placed(tx, &automation, "調べる");
            let there = placement_add(tx, automation.id, action.id).expect("placed twice");
            let step = only_step(tx, &action);

            let first = placement_step_set(tx, here.id, step.id, "claude", Some("opus")).expect("choose");
            let again = placement_step_set(tx, here.id, step.id, "codex", None).expect("choose again");
            assert_eq!(again.id, first.id, "choosing again rewrites the row rather than adding one");
            assert_eq!((again.agent.as_str(), again.model.as_deref()), ("codex", None));
            placement_step_set(tx, there.id, step.id, "claude", None).expect("the other spot");

            let chosen = |p: i64| read::automation_placement_step_for(tx.conn(), p, step.id).unwrap();
            assert_eq!(chosen(here.id).map(|c| c.agent), Some("codex".to_string()));
            assert_eq!(chosen(there.id).map(|c| c.agent), Some("claude".to_string()));

            placement_step_clear(tx, there.id, step.id).expect("take it back");
            assert!(chosen(there.id).is_none());
            placement_step_clear(tx, there.id, step.id).expect("nothing chosen is nothing to take back");
        });
    }

    /// **Placing writes the default onto the steps nobody has chosen for**, leaves a choice already
    /// made alone, writes nothing where there is no default, and passes a built-in by.
    #[test]
    fn the_default_agent_is_written_only_where_nobody_has_chosen() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, here) = mk_placed(tx, &automation, "調べる");
            let step = only_step(tx, &action);
            let chosen = |p: i64| {
                read::automation_placement_step_for(tx.conn(), p, step.id)
                    .unwrap()
                    .map(|c| (c.agent, c.model))
            };

            placement_steps_default(tx, here.id, None).expect("no default");
            assert_eq!(chosen(here.id), None, "no default leaves nobody chosen");

            placement_steps_default(tx, here.id, Some("claude-code")).expect("default");
            assert_eq!(chosen(here.id), Some(("claude-code".to_string(), None)));

            placement_step_set(tx, here.id, step.id, "codex-cli", Some("o3")).expect("choose");
            placement_steps_default(tx, here.id, Some("claude-code")).expect("default again");
            assert_eq!(
                chosen(here.id),
                Some(("codex-cli".to_string(), Some("o3".to_string()))),
                "a choice already made stays",
            );

            let builtin = crate::ops::automation_builtin::action(tx, "take_task").expect("built-in");
            let spot = placement_add(tx, automation.id, builtin.id).expect("place it");
            placement_steps_default(tx, spot.id, Some("claude-code")).expect("a built-in is passed by");
            assert!(read::automation_placement_step_ids(tx.conn(), spot.id).unwrap().is_empty());
        });
    }

    /// A choice for a step of some other action would be written and never read, so it is refused.
    #[test]
    fn a_choice_for_a_step_outside_the_placed_action_is_refused() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (_, here) = mk_placed(tx, &automation, "調べる");
            let (elsewhere, _) = mk_placed(tx, &automation, "直す");
            let step = only_step(tx, &elsewhere);
            assert!(placement_step_set(tx, here.id, step.id, "claude", None).is_err());
            assert!(placement_step_set(tx, here.id, step.id, " ", None).is_err());
        });
    }

    /// A choice goes with the placement taken off, and stays when the step is deleted from the action —
    /// the version a placement points at still holds the step (`AMB-D-1000`).
    #[test]
    fn a_choice_goes_with_its_placement_and_stays_when_its_step_goes() {
        with_tx(|tx| {
            let automation = mk_automation(tx);
            let (action, here) = mk_placed(tx, &automation, "調べる");
            let there = placement_add(tx, automation.id, action.id).expect("placed twice");
            let step = only_step(tx, &action);
            placement_step_set(tx, here.id, step.id, "claude", None).expect("choose here");
            placement_step_set(tx, there.id, step.id, "claude", None).expect("choose there");

            placement_delete(tx, there.id).expect("take one spot off");
            assert!(read::automation_placement_step_ids(tx.conn(), there.id).unwrap().is_empty());
            assert_eq!(read::automation_placement_step_ids(tx.conn(), here.id).unwrap().len(), 1);

            step_delete(tx, step.id).expect("delete the step");
            assert_eq!(read::automation_placement_step_ids(tx.conn(), here.id).unwrap().len(), 1);
        });
    }
}

/// **A definition a run is going on is rewritten all the same** (`AMB-D-1015`): no op that rewrites one
/// refuses because a run launched from it is `running` or `paused`, and the run goes on with the copy it
/// took at launch.
#[cfg(test)]
mod rewritten_under_a_run {
    use super::*;
    use crate::model::{AutomationRun, AutomationRunStatus};
    use crate::ops::automation_run::{launch_past_the_task_checks as launch, nothing_asked, Launcher};
    use crate::ops::automation_stop;
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
        cfg_set(tx, first.id, "depth", Some("\"shallow\"")).expect("answer");
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

    fn paused(tx: &WriteTx<'_>, run: &AutomationRun) -> AutomationRun {
        automation_stop::pause(tx, run.id).expect("pause");
        let pausing = read::automation_run(tx.conn(), run.id).expect("read").expect("the run");
        automation_stop::settle(tx, pausing, crate::model::AutomationPauseKind::EndOfAction).expect("settle");
        let paused = read::automation_run(tx.conn(), run.id).expect("read").expect("the run");
        assert_eq!(paused.status, AutomationRunStatus::Paused);
        paused
    }

    /// One copy a run took at launch: its name, prompt, agent, ways out, inputs, settings and whether it
    /// is the entry.
    type Copy = (String, Option<String>, String, String, String, String, bool);

    /// What a run copied at launch, without the ways back to the live definition — those let go of a
    /// placement or a step that is taken off.
    fn copied(tx: &WriteTx<'_>, run: &AutomationRun) -> Vec<Copy> {
        read::automation_run_defs_of(tx.conn(), run.id)
            .expect("read")
            .into_iter()
            .map(|d| (d.name, d.prompt, d.agent, d.exits, d.ins, d.cfg, d.entry))
            .collect()
    }

    /// Not refused for the run: it goes through, or it is refused for a reason of its own.
    fn not_held<T: std::fmt::Debug>(what: &str, result: Result<T>) {
        if let Err(err) = result {
            assert_ne!(err.code(), "conflict", "{what}: {err}");
        }
    }

    /// Every rewrite of either definition — the automation's picture and the actions placed on it. One
    /// rewrite can take away what a later one names, so a later one may be refused for that; none is
    /// refused for the run.
    fn every_rewrite(tx: &WriteTx<'_>, p: &Picture) {
        let step = only_step(tx, &p.first_action);
        let action = p.first_action.id;
        let found = read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, action, Some("found"))
            .expect("read")
            .expect("the way out");
        let note = read::automation_port_ids(tx.conn(), AutomationPortOwner::Exit, found.id)
            .expect("read")[0];
        let inner_edge = read::automation_edge_ids(tx.conn(), AutomationPictureOwner::Action, action)
            .expect("read")[0];
        let inner_wire = read::automation_wire_ids(tx.conn(), AutomationPictureOwner::Action, action)
            .expect("read")[0];
        let answer =
            read::automation_cfg_by_name(tx.conn(), AutomationCfgOwner::Placement, p.first.id, "depth")
                .expect("read")
                .expect("the answer");

        // The automation's picture.
        not_held("rename", update(tx, p.automation.id, Some("別名"), None, None));
        not_held("archive", update(tx, p.automation.id, None, None, Some(true)));
        not_held("entry", entry_replace(tx, p.automation.id, "fetch"));
        not_held("delete", delete(tx, p.automation.id));
        not_held("place", placement_add(tx, p.automation.id, p.second_action.id));
        not_held("place on a line", placement_insert(tx, p.onward.id, p.second_action.id));
        not_held(
            "place new on a line",
            placement_insert_new(tx, p.onward.id, ActionShelf::Project, "新しい"),
        );
        not_held("reorder a placement", placement_move(tx, p.first.id, Position::Bottom));
        not_held("take a placement off", placement_delete(tx, p.first.id));
        not_held("answer a setting", cfg_set(tx, p.first.id, "depth", Some("\"deep\"")));
        not_held("rewrite an answer", cfg_update(tx, answer.id, None, None, Some(true), None));
        not_held("reorder an answer", cfg_move(tx, answer.id, Position::Top));
        not_held("take an answer off", cfg_delete(tx, answer.id));
        not_held("choose an agent", placement_step_set(tx, p.first.id, step.id, "codex", None));
        not_held("take the agent back", placement_step_clear(tx, p.first.id, step.id));
        not_held(
            "draw a line",
            edge_add(tx, AutomationPictureOwner::Automation, p.first.id, Some(ERROR_EXIT), EdgeTarget::Done, None),
        );
        not_held("redraw a line", edge_update(tx, p.onward.id, Some(EdgeTarget::Done), None));
        not_held("rub a line out", edge_delete(tx, p.onward.id));
        not_held(
            "wire",
            wire_add(tx, AutomationPictureOwner::Automation, p.first.id, Some("found"), "note", p.first.id, "note"),
        );
        not_held("unwire", wire_delete(tx, p.wire.id));

        // An action placed on it.
        not_held("rename the action", action_update(tx, action, Some("別名"), None));
        not_held("the action's entry", action_set_entry(tx, action, None));
        not_held("move the action's reach", action_set_scope(tx, action, None));
        not_held("delete the action", action_delete(tx, action));
        not_held("add a step", step_add(tx, action, NewStep::new("もう一つ", "again")));
        not_held(
            "add a step on a line",
            step_insert(tx, inner_edge, NewStep::new("もう一つ", "again"), &[], &[]),
        );
        not_held(
            "rewrite a step",
            step_update(tx, step.id, None, Some("again"), None, None, None, None, None, None, None, None),
        );
        not_held("reorder a step", step_move(tx, step.id, Position::Bottom));
        not_held("delete a step", step_delete(tx, step.id));
        not_held("add a way out", exit_add(tx, AutomationOwner::Action, action, Some("other")));
        not_held("add a step's way out", exit_add(tx, AutomationOwner::Step, step.id, Some("other")));
        not_held("rename a way out", exit_rename(tx, found.id, Some("seen")));
        not_held("reorder a way out", exit_move(tx, found.id, Position::Top));
        not_held("delete a way out", exit_delete(tx, found.id));
        not_held(
            "add an output",
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
        not_held(
            "add an input",
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
        not_held("rewrite a port", port_update(tx, note, Some("memo"), None, None));
        not_held("reorder a port", port_move(tx, note, Position::Top));
        not_held("delete a port", port_delete(tx, note));
        not_held(
            "declare a setting",
            cfg_add(tx, action, "breadth", AutomationCfgKind::Text, false, None),
        );
        not_held("rewrite a setting", cfg_update(tx, p.cfg.id, Some("width"), None, None, None));
        not_held("reorder a setting", cfg_move(tx, p.cfg.id, Position::Top));
        not_held("delete a setting", cfg_delete(tx, p.cfg.id));
        not_held(
            "draw a line inside",
            edge_add(tx, AutomationPictureOwner::Action, step.id, Some(ERROR_EXIT), EdgeTarget::Halt, None),
        );
        not_held("redraw a line inside", edge_update(tx, inner_edge, None, Some(Some(3))));
        not_held("rub a line out inside", edge_delete(tx, inner_edge));
        not_held("unwire inside", wire_delete(tx, inner_wire));
    }

    #[test]
    fn no_rewrite_is_refused_while_a_run_is_running_and_the_run_keeps_its_copy() {
        with_tx(|tx| {
            let p = picture(tx);
            let run = launched(tx, &p.automation);
            let before = copied(tx, &run);
            every_rewrite(tx, &p);
            assert_eq!(copied(tx, &run), before);
        });
    }

    #[test]
    fn no_rewrite_is_refused_while_a_run_is_paused_and_the_run_keeps_its_copy() {
        with_tx(|tx| {
            let p = picture(tx);
            let run = paused(tx, &launched(tx, &p.automation));
            let before = copied(tx, &run);
            every_rewrite(tx, &p);
            assert_eq!(copied(tx, &run), before);
        });
    }

    /// **Renaming, archiving and moving an action's reach are written**, not only left unrefused, while a
    /// run is running and while it is paused.
    #[test]
    fn rename_archive_and_scope_are_written_while_a_run_is_running_or_paused() {
        for pause in [false, true] {
            with_tx(|tx| {
                let p = picture(tx);
                let run = launched(tx, &p.automation);
                let run = if pause { paused(tx, &run) } else { run };
                let before = copied(tx, &run);

                let renamed = update(tx, p.automation.id, Some("別名"), None, None).expect("rename");
                assert_eq!(renamed.name, "別名");
                let archived = update(tx, p.automation.id, None, None, Some(true)).expect("archive");
                assert!(archived.archived);
                let moved = action_set_scope(tx, p.first_action.id, None).expect("to the device");
                assert_eq!(moved.project_id, None);
                let step = only_step(tx, &p.first_action);
                step_update(tx, step.id, None, Some("look again"), None, None, None, None, None, None, None, None)
                    .expect("rewrite a step");

                assert_eq!(copied(tx, &run), before);
            });
        }
    }
}
