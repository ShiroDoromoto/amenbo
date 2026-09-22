//! **Reading an automation's definition back**, with the declarations each step runs under already
//! resolved — the one shape both a build screen and a terminal read it in.
//!
//! The ten definition tables are written by [`super::automation`]; nothing here writes. What this
//! module does is the resolving: a step reads its ways out, its inputs and its settings from the
//! library action it points at or from itself ([`super::automation::declarer`]), and a caller that had
//! to know which of the two declared a name would be reading the storage rather than the automation.
//!
//! **It lives in core because two sides read it.** The build screen fetches one whole definition and
//! the terminal prints one; resolved twice, the two would drift, and an automation that read one way
//! on screen and another way in a terminal is the same automation in name only.

use rusqlite::Connection;
use serde::Serialize;

use crate::model::{
    Automation, AutomationAction, AutomationCfg, AutomationEdge, AutomationExit, AutomationNote,
    AutomationOwner, AutomationPort, AutomationPortDirection, AutomationPortOwner, AutomationStep,
    AutomationWire,
};
use crate::store_engine::read;
use crate::Result;

/// **One automation in a list**: the row itself, and how many steps it is built out of.
///
/// The count comes with the row because "what is this" and "is it built yet" are the two things a
/// list is read for.
#[derive(Clone, Debug, Serialize)]
pub struct AutomationCard {
    pub automation: Automation,
    pub steps: usize,
}

/// **One library action in a list**, with how many automations run it.
///
/// `used_by` counts automations, not steps: two steps of one automation pointing at the same action
/// is one automation whose runs change when the prompt is rewritten, and that is the number a warning
/// beside the prompt is about.
#[derive(Clone, Debug, Serialize)]
pub struct ActionCard {
    pub action: AutomationAction,
    pub used_by: usize,
}

/// **One automation's whole definition** — every step with what it runs under, what joins them, and
/// the documents its steps share.
#[derive(Clone, Debug, Serialize)]
pub struct AutomationView {
    pub automation: Automation,
    pub steps: Vec<StepView>,
    pub edges: Vec<AutomationEdge>,
    pub wires: Vec<AutomationWire>,
    pub notes: Vec<NoteView>,
}

/// **One step**, with the action it points at read in and the declarations it runs under resolved.
#[derive(Clone, Debug, Serialize)]
pub struct StepView {
    pub step: AutomationStep,
    /// The library action this step runs, or `None` where it carries its own prompt.
    pub action: Option<AutomationAction>,
    /// The prompt it runs on — its own, or the one it reads off the action.
    pub prompt: String,
    pub exits: Vec<ExitView>,
    /// What this step takes in, in declaration order.
    pub inputs: Vec<AutomationPort>,
    /// Declared by whichever of the step and its action declares, answered on the step.
    pub settings: Vec<AutomationCfg>,
}

/// **A way out**, and what leaving through it hands on.
#[derive(Clone, Debug, Serialize)]
pub struct ExitView {
    pub exit: AutomationExit,
    pub outputs: Vec<AutomationPort>,
}

/// **A shared document**, and the steps it is handed to.
#[derive(Clone, Debug, Serialize)]
pub struct NoteView {
    pub note: AutomationNote,
    pub step_ids: Vec<i64>,
}

/// **One library action in full**: the prompt, what it declares, and how many automations run it.
#[derive(Clone, Debug, Serialize)]
pub struct ActionView {
    pub action: AutomationAction,
    pub used_by: usize,
    pub exits: Vec<ExitView>,
    pub inputs: Vec<AutomationPort>,
    /// The declarations alone. An action's rows carry no answer — the step that uses it holds that.
    pub settings: Vec<AutomationCfg>,
}

/// The automations of one project, in the order they were placed in.
///
/// Archived ones come too: what an archived automation is, is one kept out of the way rather than
/// gone, and which of the two a caller shows is the caller's to decide.
pub fn cards(conn: &Connection, project_id: i64) -> Result<Vec<AutomationCard>> {
    let mut out = Vec::new();
    for (id, _) in read::automation_siblings(conn, project_id, None)? {
        let Some(automation) = read::automation(conn, id)? else { continue };
        out.push(AutomationCard { automation, steps: read::automation_step_ids(conn, id)?.len() });
    }
    Ok(out)
}

/// **The library a project reaches** — the device's own actions first, then the project's.
///
/// `project_id` `None` is the device's shelf alone, which is what every project on this machine
/// reaches. With a project named, both answer as one list: what a step here could be pointed at is
/// one list, and which shelf holds a given action is a column of it rather than a second list to go
/// and look in.
pub fn action_cards(conn: &Connection, project_id: Option<i64>) -> Result<Vec<ActionCard>> {
    let mut out = Vec::new();
    for reach in shelves(project_id) {
        for (id, _) in read::automation_action_siblings(conn, reach, None)? {
            let Some(action) = read::automation_action(conn, id)? else { continue };
            out.push(ActionCard { action, used_by: used_by(conn, id)? });
        }
    }
    Ok(out)
}

/// Which shelves a listing walks, in the order they are shown: the device's, then the project's own
/// where one is named.
fn shelves(project_id: Option<i64>) -> Vec<Option<i64>> {
    match project_id {
        Some(p) => vec![None, Some(p)],
        None => vec![None],
    }
}

/// **How many automations run this action** — the steps pointing at it, counted by the automation
/// they sit in.
pub fn used_by(conn: &Connection, action_id: i64) -> Result<usize> {
    let mut seen = std::collections::BTreeSet::new();
    for step_id in read::automation_step_ids_using_action(conn, action_id)? {
        let Some(step) = read::automation_step(conn, step_id)? else { continue };
        seen.insert(step.automation_id);
    }
    Ok(seen.len())
}

/// One automation's ten tables, read and resolved — or `None` where that id names none.
pub fn detail(conn: &Connection, id: i64) -> Result<Option<AutomationView>> {
    let Some(automation) = read::automation(conn, id)? else { return Ok(None) };
    let mut steps = Vec::new();
    for (step_id, _) in read::automation_step_siblings(conn, id, None)? {
        let Some(step) = read::automation_step(conn, step_id)? else { continue };
        steps.push(step_view(conn, step)?);
    }
    let mut edges = Vec::new();
    for edge_id in read::automation_edge_ids(conn, id)? {
        if let Some(edge) = read::automation_edge(conn, edge_id)? {
            edges.push(edge);
        }
    }
    let mut wires = Vec::new();
    for wire_id in read::automation_wire_ids(conn, id)? {
        if let Some(wire) = read::automation_wire(conn, wire_id)? {
            wires.push(wire);
        }
    }
    let mut notes = Vec::new();
    for (note_id, _) in read::automation_note_siblings(conn, id, None)? {
        let Some(note) = read::automation_note(conn, note_id)? else { continue };
        let mut step_ids = Vec::new();
        for link_id in read::automation_step_note_ids_of_note(conn, note_id)? {
            if let Some(link) = read::automation_step_note(conn, link_id)? {
                step_ids.push(link.step_id);
            }
        }
        notes.push(NoteView { note, step_ids });
    }
    Ok(Some(AutomationView { automation, steps, edges, wires, notes }))
}

/// One library action in full, or `None` where that id names none.
pub fn action_detail(conn: &Connection, id: i64) -> Result<Option<ActionView>> {
    let Some(action) = read::automation_action(conn, id)? else { return Ok(None) };
    let exits = exit_views(conn, AutomationOwner::Action, id)?;
    let inputs = ports_of(conn, AutomationPortOwner::Action, id, AutomationPortDirection::In)?;
    let settings = read::automation_cfgs_of(conn, AutomationOwner::Action, id)?;
    Ok(Some(ActionView { action, used_by: used_by(conn, id)?, exits, inputs, settings }))
}

/// One step, with the action it points at read in: the prompt it runs on, the ways out it can leave
/// by, what it takes and what it is set to.
fn step_view(conn: &Connection, step: AutomationStep) -> Result<StepView> {
    let action = match step.action_id {
        Some(action_id) => read::automation_action(conn, action_id)?,
        None => None,
    };
    let (owner_kind, owner_id) = super::automation::declarer(&step);
    let port_owner = match owner_kind {
        AutomationOwner::Step => AutomationPortOwner::Step,
        AutomationOwner::Action => AutomationPortOwner::Action,
    };
    let exits = exit_views(conn, owner_kind, owner_id)?;
    let inputs = ports_of(conn, port_owner, owner_id, AutomationPortDirection::In)?;
    // An action declares and the step answers, and core is what puts the two rows back together — the
    // same pair the launch check reads, rather than a second reading of it.
    let settings = super::automation_run::settings_of(conn, &step)?;
    let prompt = step
        .prompt
        .clone()
        .or_else(|| action.as_ref().map(|one| one.prompt.clone()))
        .unwrap_or_default();
    Ok(StepView { step, action, prompt, exits, inputs, settings })
}

/// The ways out one owner declares, each with what leaving through it hands on.
fn exit_views(conn: &Connection, owner_kind: AutomationOwner, owner_id: i64) -> Result<Vec<ExitView>> {
    let mut out = Vec::new();
    for exit in read::automation_exits_of(conn, owner_kind, owner_id)? {
        let outputs =
            ports_of(conn, AutomationPortOwner::Exit, exit.id, AutomationPortDirection::Out)?;
        out.push(ExitView { exit, outputs });
    }
    Ok(out)
}

fn ports_of(
    conn: &Connection,
    owner: AutomationPortOwner,
    owner_id: i64,
    direction: AutomationPortDirection,
) -> Result<Vec<AutomationPort>> {
    Ok(read::automation_ports_of(conn, owner, owner_id, direction)?)
}
