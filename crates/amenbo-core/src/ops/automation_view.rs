//! **Reading a definition back**, with the declarations each box runs under already resolved — the one
//! shape both a build screen and a terminal read it in.
//!
//! The eleven definition tables are written by [`super::automation`]; nothing here writes. What this
//! module does is the resolving: a placement reads its ways out, its inputs and its settings off the
//! library action standing on it, a step reads its own, and a caller that had to know which of the two
//! declared a name would be reading the storage rather than the picture.
//!
//! **Two pictures, two details.** [`detail`] reads one automation — the placements on it and the lines
//! between them — and [`action_detail`] reads one library action, which is a picture in its own right:
//! the steps inside it and the lines between those.
//!
//! **It lives in core because two sides read it.** The build screen fetches one whole definition and
//! the terminal prints one; resolved twice, the two would drift, and an automation that read one way
//! on screen and another way in a terminal is the same automation in name only.

use rusqlite::Connection;
use serde::Serialize;

use crate::model::{
    Automation, AutomationAction, AutomationCfg, AutomationCfgOwner, AutomationEdge, AutomationExit,
    AutomationOwner, AutomationPictureOwner, AutomationPlacement, AutomationPort,
    AutomationPortDirection, AutomationPortOwner, AutomationStep, AutomationWire,
};
use crate::store_engine::read;
use crate::Result;

/// **One automation in a list**: the row itself, and how many actions are placed on it.
///
/// The count comes with the row because "what is this" and "is it built yet" are the two things a
/// list is read for.
#[derive(Clone, Debug, Serialize)]
pub struct AutomationCard {
    pub automation: Automation,
    pub placements: usize,
}

/// **One library action in a list**, with how many steps it holds and how many automations place it.
///
/// `used_by` counts automations, not placements: one action placed twice on one automation is one
/// automation whose runs change when the prompt is rewritten, and that is the number a warning beside
/// the prompt is about.
#[derive(Clone, Debug, Serialize)]
pub struct ActionCard {
    pub action: AutomationAction,
    pub steps: usize,
    pub used_by: usize,
}

/// **One automation's whole definition** — every placement with what it runs under, and what joins
/// them.
#[derive(Clone, Debug, Serialize)]
pub struct AutomationView {
    pub automation: Automation,
    pub placements: Vec<PlacementView>,
    pub edges: Vec<AutomationEdge>,
    pub wires: Vec<AutomationWire>,
}

/// **One placement**, with the action standing on it read in and the declarations it runs under
/// resolved.
#[derive(Clone, Debug, Serialize)]
pub struct PlacementView {
    pub placement: AutomationPlacement,
    /// The library action placed here, or `None` where that row is gone from under it.
    pub action: Option<AutomationAction>,
    /// The step a run opens first at this spot — the action's entry, read in so a caller drawing the
    /// prompt and the agent does not have to go back for it. `None` where the action holds none.
    pub entry_step: Option<AutomationStep>,
    /// The ways out the run can leave this spot by — the action's.
    pub exits: Vec<ExitView>,
    /// What this spot takes in, in declaration order — the action's.
    pub inputs: Vec<AutomationPort>,
    /// Declared by the action, answered here.
    pub settings: Vec<AutomationCfg>,
}

/// **A way out**, and what leaving through it hands on.
#[derive(Clone, Debug, Serialize)]
pub struct ExitView {
    pub exit: AutomationExit,
    pub outputs: Vec<AutomationPort>,
}

/// **One library action in full**: the picture inside it, what it declares to the outside, and how many
/// automations place it.
#[derive(Clone, Debug, Serialize)]
pub struct ActionView {
    pub action: AutomationAction,
    pub used_by: usize,
    pub steps: Vec<StepView>,
    pub edges: Vec<AutomationEdge>,
    pub wires: Vec<AutomationWire>,
    /// The ways out a placement of this action is left by.
    pub exits: Vec<ExitView>,
    pub inputs: Vec<AutomationPort>,
    /// The declarations alone. An action's rows carry no answer — the placement holds that.
    pub settings: Vec<AutomationCfg>,
}

/// **One step inside an action**: the prompt it runs on, the ways out it ends on, and what it takes in.
#[derive(Clone, Debug, Serialize)]
pub struct StepView {
    pub step: AutomationStep,
    pub exits: Vec<ExitView>,
    pub inputs: Vec<AutomationPort>,
}

/// The automations of one project, in the order they were placed in.
///
/// Archived ones come too: what an archived automation is, is one kept out of the way rather than
/// gone, and which of the two a caller shows is the caller's to decide.
pub fn cards(conn: &Connection, project_id: i64) -> Result<Vec<AutomationCard>> {
    let mut out = Vec::new();
    for (id, _) in read::automation_siblings(conn, project_id, None)? {
        let Some(automation) = read::automation(conn, id)? else { continue };
        out.push(AutomationCard {
            automation,
            placements: read::automation_placement_ids(conn, id)?.len(),
        });
    }
    Ok(out)
}

/// **The library a project reaches** — the device's own actions first, then the project's.
///
/// `project_id` `None` is the device's shelf alone, which is what every project on this machine
/// reaches. With a project named, both answer as one list: what could be placed here is one list, and
/// which shelf holds a given action is a column of it rather than a second list to go and look in.
pub fn action_cards(conn: &Connection, project_id: Option<i64>) -> Result<Vec<ActionCard>> {
    let mut out = Vec::new();
    for reach in shelves(project_id) {
        for (id, _) in read::automation_action_siblings(conn, reach, None)? {
            let Some(action) = read::automation_action(conn, id)? else { continue };
            out.push(ActionCard {
                action,
                steps: read::automation_action_step_ids(conn, id)?.len(),
                used_by: used_by(conn, id)?,
            });
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

/// **How many automations place this action** — the placements standing on it, counted by the
/// automation they sit on.
pub fn used_by(conn: &Connection, action_id: i64) -> Result<usize> {
    let mut seen = std::collections::BTreeSet::new();
    for placement_id in read::automation_placement_ids_using_action(conn, action_id)? {
        let Some(placement) = read::automation_placement(conn, placement_id)? else { continue };
        seen.insert(placement.automation_id);
    }
    Ok(seen.len())
}

/// One automation read and resolved — or `None` where that id names none.
pub fn detail(conn: &Connection, id: i64) -> Result<Option<AutomationView>> {
    let Some(automation) = read::automation(conn, id)? else { return Ok(None) };
    let mut placements = Vec::new();
    for placement in read::automation_placements_of(conn, id)? {
        placements.push(placement_view(conn, placement)?);
    }
    let edges = read::automation_edges_of(conn, AutomationPictureOwner::Automation, id)?;
    let wires = read::automation_wires_of(conn, AutomationPictureOwner::Automation, id)?;
    Ok(Some(AutomationView { automation, placements, edges, wires }))
}

/// One library action in full, or `None` where that id names none.
pub fn action_detail(conn: &Connection, id: i64) -> Result<Option<ActionView>> {
    let Some(action) = read::automation_action(conn, id)? else { return Ok(None) };
    let mut steps = Vec::new();
    for step in read::automation_action_steps_of(conn, id)? {
        steps.push(step_view(conn, step)?);
    }
    let edges = read::automation_edges_of(conn, AutomationPictureOwner::Action, id)?;
    let wires = read::automation_wires_of(conn, AutomationPictureOwner::Action, id)?;
    let exits = exit_views(conn, AutomationOwner::Action, id)?;
    let inputs = ports_of(conn, AutomationPortOwner::Action, id, AutomationPortDirection::In)?;
    let settings = read::automation_cfgs_of(conn, AutomationCfgOwner::Action, id)?;
    Ok(Some(ActionView {
        action,
        used_by: used_by(conn, id)?,
        steps,
        edges,
        wires,
        exits,
        inputs,
        settings,
    }))
}

/// One placement, with the action standing on it read in: the ways out it can be left by, what it takes
/// in, and what it is set to.
fn placement_view(conn: &Connection, placement: AutomationPlacement) -> Result<PlacementView> {
    let action = read::automation_action(conn, placement.action_id)?;
    let entry_step = match action.as_ref().and_then(|one| one.entry_step_id) {
        Some(step_id) => read::automation_action_step(conn, step_id)?,
        None => None,
    };
    let exits = exit_views(conn, AutomationOwner::Action, placement.action_id)?;
    let inputs = ports_of(
        conn,
        AutomationPortOwner::Action,
        placement.action_id,
        AutomationPortDirection::In,
    )?;
    // An action declares and the placement answers, and core is what puts the two rows back together —
    // the same pair the launch check reads, rather than a second reading of it.
    let settings = super::automation_run::settings_of(conn, &placement)?;
    Ok(PlacementView { placement, action, entry_step, exits, inputs, settings })
}

/// One step of an action, with its own declarations read in.
fn step_view(conn: &Connection, step: AutomationStep) -> Result<StepView> {
    let exits = exit_views(conn, AutomationOwner::Step, step.id)?;
    let inputs = ports_of(conn, AutomationPortOwner::Step, step.id, AutomationPortDirection::In)?;
    Ok(StepView { step, exits, inputs })
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
