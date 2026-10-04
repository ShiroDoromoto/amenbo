//! **Reading a definition back**, with the declarations each box runs under already resolved — the one
//! shape both a build screen and a terminal read it in.
//!
//! The ten definition tables are written by [`super::automation`]; nothing here writes. What this
//! module does is the resolving: a placement reads its ways out, its inputs and its settings off the
//! version of the library action it stands on, a step reads its own, and a caller that had to know which
//! of the two declared a name would be reading the storage rather than the picture.
//!
//! **Two pictures, two details.** [`detail`] reads one automation — the placements on it and the lines
//! between them — and [`action_detail`] reads one library action, which is a picture in its own right:
//! the steps inside it and the lines between those.
//!
//! **The draft, or the saved version.** Both read the draft — what the tables hold now — and say which
//! version was saved last and whether the draft holds more. [`saved_detail`] and [`action_saved_detail`]
//! read the newest saved version instead, which is what a launch uses.
//!
//! **It lives in core because two sides read it.** The build screen fetches one whole definition and
//! the terminal prints one; resolved twice, the two would drift, and an automation that read one way
//! on screen and another way in a terminal is the same automation in name only.

use rusqlite::Connection;
use serde::Serialize;

use crate::model::{
    Automation, AutomationAction, AutomationCfg, AutomationCfgOwner, AutomationEdge, AutomationExit,
    AutomationOwner, AutomationPictureOwner, AutomationPlacement, AutomationPlacementStep,
    AutomationPort,
    AutomationPortDirection, AutomationPortOwner, AutomationStep, AutomationWire,
};
use crate::ops::automation::ActionDef;
use crate::store_engine::read;
use crate::time::Timestamp;
use crate::{Error, Result};

/// **One automation in a list**: the row itself, and how many actions are placed on it.
///
/// The count comes with the row because "what is this" and "is it built yet" are the two things a
/// list is read for.
#[derive(Clone, Debug, Serialize)]
pub struct AutomationCard {
    pub automation: Automation,
    pub placements: usize,
}

/// **One automation in the list that spans every project**: its card, and the name of the project it
/// belongs to.
///
/// The name comes with the row because a list drawn from many projects is read by project first — an
/// automation's name alone says nothing until it says whose it is.
#[derive(Clone, Debug, Serialize)]
pub struct ProjectAutomationCard {
    pub project_name: String,
    pub card: AutomationCard,
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

/// **One library action in the list that spans every project**: its card, and the name of the project
/// whose shelf holds it — `None` for the device's own, which is no project's.
///
/// The name comes with the row for the reason [`ProjectAutomationCard`] gives: a list drawn from many
/// projects is read by whose each row is.
#[derive(Clone, Debug, Serialize)]
pub struct ProjectActionCard {
    pub project_name: Option<String>,
    pub card: ActionCard,
}

/// **One automation's whole definition** — every placement with what it runs under, and what joins
/// them.
#[derive(Clone, Debug, Serialize)]
pub struct AutomationView {
    pub automation: Automation,
    /// Which of the two this reads — the draft, or the newest saved version.
    pub showing: Showing,
    /// The newest saved version, or `None` for an automation nobody has saved.
    pub saved: Option<SavedVersion>,
    /// Does the draft hold anything the newest saved version does not?
    pub unsaved: bool,
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
    /// **The version of the action this placement stands on** — a built-in's off its record
    /// (`AMB-D-1000`) — or `None` where it stands on none.
    pub version: Option<i64>,
    /// The newest version of that action there is — for a built-in, the one this build defines; for
    /// another, its newest saved. `None` where it has none.
    pub latest_version: Option<i64>,
    /// The step a run opens first at this spot — the action's entry, read in so a caller drawing its
    /// prompt does not have to go back for it. `None` where the action holds none.
    pub entry_step: Option<AutomationStep>,
    /// The ways out the run can leave this spot by — the action's.
    pub exits: Vec<ExitView>,
    /// What this spot takes in, in declaration order — the action's.
    pub inputs: Vec<AutomationPort>,
    /// Declared by the action, answered here.
    pub settings: Vec<AutomationCfg>,
    /// Every step of the action, in display order, with who is chosen to carry it out at this spot
    /// (`AMB-D-960`).
    pub steps: Vec<PlacementStepView>,
}

/// **One step of the action standing on a placement**, and who carries it out there — `None` where
/// nobody is chosen yet, which the launch check refuses for a step a run could open.
#[derive(Clone, Debug, Serialize)]
pub struct PlacementStepView {
    pub step: AutomationStep,
    pub chosen: Option<AutomationPlacementStep>,
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
    /// Which of the two this reads — the draft, or the newest saved version. A built-in is never
    /// written on, so both read the same.
    pub showing: Showing,
    /// The newest saved version, or `None` for a built-in and for an action nobody has saved.
    pub saved: Option<SavedVersion>,
    /// Does the draft hold anything the newest saved version does not? Never, for a built-in.
    pub unsaved: bool,
    pub used_by: usize,
    /// Every placement of the action, with the version each stands on, in automation order.
    pub placed_at: Vec<PlacedAt>,
    pub steps: Vec<StepView>,
    pub edges: Vec<AutomationEdge>,
    pub wires: Vec<AutomationWire>,
    /// The ways out a placement of this action is left by.
    pub exits: Vec<ExitView>,
    pub inputs: Vec<AutomationPort>,
    /// The declarations alone. An action's rows carry no answer — the placement holds that.
    pub settings: Vec<AutomationCfg>,
}

/// **Which of the two a show reads**: the draft being written, or the newest saved version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Showing {
    Draft,
    Saved,
}

/// **One saved version, as a show names it**: its number, and when it was saved.
#[derive(Clone, Debug, Serialize)]
pub struct SavedVersion {
    pub version: i64,
    pub saved_at: Timestamp,
}

/// **One placement of an action**: the automation it sits on, and the version of the action it stands
/// on — `None` where it stands on none.
#[derive(Clone, Debug, Serialize)]
pub struct PlacedAt {
    pub automation_id: i64,
    pub placement_id: i64,
    pub version: Option<i64>,
}

/// **One step inside an action**: the prompt it runs on, the ways out it ends on, and what it takes in.
#[derive(Clone, Debug, Serialize)]
pub struct StepView {
    pub step: AutomationStep,
    pub exits: Vec<ExitView>,
    pub inputs: Vec<AutomationPort>,
}

impl AutomationView {
    /// **The name a port carries now**, by its id — what a wire keys at either end (`AMB-D-961`) and what
    /// a reader is shown. Read from the declarations the placements resolve, so a renamed port's wires
    /// read by the new name. `None` for an id no placement here declares.
    pub fn port_name(&self, port_id: i64) -> Option<&str> {
        self.placements
            .iter()
            .flat_map(|p| p.exits.iter().flat_map(|e| e.outputs.iter()).chain(p.inputs.iter()))
            .find(|port| port.id == port_id)
            .map(|port| port.name.as_str())
    }
}

impl ActionView {
    /// **The name a port carries now**, by its id — one of a step's or one the action declares at its
    /// edge, which are the ports the wires inside the action key. `None` for an id nothing here declares.
    pub fn port_name(&self, port_id: i64) -> Option<&str> {
        let steps = self
            .steps
            .iter()
            .flat_map(|s| s.exits.iter().flat_map(|e| e.outputs.iter()).chain(s.inputs.iter()));
        let edge = self.exits.iter().flat_map(|e| e.outputs.iter()).chain(self.inputs.iter());
        steps.chain(edge).find(|port| port.id == port_id).map(|port| port.name.as_str())
    }
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

/// **The automations of every project**, project by project in the sidebar's order, and within one
/// project in the order they were placed in.
///
/// Archived projects come too, the same as the running tab lists their runs: archiving a project
/// hides it from the sidebar, not the automations it still has from the list that spans them. `reach`
/// narrows the walk to one project, the way a bound session sees only that one.
pub fn every_card(conn: &Connection, reach: Option<i64>) -> Result<Vec<ProjectAutomationCard>> {
    let mut out = Vec::new();
    for project in read::project_list(conn, true)? {
        if reach.is_some_and(|pid| pid != project.id) {
            continue;
        }
        for card in cards(conn, project.id)? {
            out.push(ProjectAutomationCard { project_name: project.name.clone(), card });
        }
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
        out.extend(shelf_cards(conn, reach)?);
    }
    Ok(out)
}

/// **The library of every project** — the device's own actions first, then each project's own, project
/// by project in the sidebar's order.
///
/// Archived projects come too, for the reason [`every_card`] gives. `reach` narrows the walk to one
/// project, the way a bound session sees only that one; the device's shelf stays, since every project
/// reaches it.
pub fn every_action_card(conn: &Connection, reach: Option<i64>) -> Result<Vec<ProjectActionCard>> {
    let mut out: Vec<ProjectActionCard> = action_cards(conn, None)?
        .into_iter()
        .map(|card| ProjectActionCard { project_name: None, card })
        .collect();
    for project in read::project_list(conn, true)? {
        if reach.is_some_and(|pid| pid != project.id) {
            continue;
        }
        for card in shelf_cards(conn, Some(project.id))? {
            out.push(ProjectActionCard { project_name: Some(project.name.clone()), card });
        }
    }
    Ok(out)
}

/// The actions on one shelf — the device's for `None` — in the order they were placed in.
fn shelf_cards(conn: &Connection, reach: Option<i64>) -> Result<Vec<ActionCard>> {
    let mut out = Vec::new();
    for (id, _) in read::automation_action_siblings(conn, reach, None)? {
        let Some(action) = read::automation_action(conn, id)? else { continue };
        out.push(ActionCard {
            action,
            steps: read::automation_action_step_ids(conn, id)?.len(),
            used_by: used_by(conn, id)?,
        });
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
    Ok(automations_placing(conn, action_id)?.len())
}

/// **The automations an action is placed on**, each once, in id order — what a rewrite of the action
/// reaches, and so whose runs hold it.
pub fn automations_placing(conn: &Connection, action_id: i64) -> Result<Vec<i64>> {
    let mut seen = std::collections::BTreeSet::new();
    for placement_id in read::automation_placement_ids_using_action(conn, action_id)? {
        let Some(placement) = read::automation_placement(conn, placement_id)? else { continue };
        seen.insert(placement.automation_id);
    }
    Ok(seen.into_iter().collect())
}

/// **The runs holding an automation's definition** — its own that are `running` or `paused`, in id
/// order. Core does not refuse a rewrite for them (`AMB-D-1015`); a build screen reads this answer to
/// name the runs going on the definition it shows.
pub fn run_ids_holding_automation(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    let mut runs = read::automation_run_ids_under_way(conn, id)?;
    runs.sort_unstable();
    Ok(runs)
}

/// **The runs holding an action's definition** — those going on any automation that places it, in any
/// project, in id order.
pub fn run_ids_holding_action(conn: &Connection, action_id: i64) -> Result<Vec<i64>> {
    let mut runs = Vec::new();
    for automation_id in automations_placing(conn, action_id)? {
        runs.extend(run_ids_holding_automation(conn, automation_id)?);
    }
    runs.sort_unstable();
    Ok(runs)
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
    let saved = read::automation_version_latest(conn, id)?
        .map(|v| SavedVersion { version: v.version, saved_at: v.created_at });
    let unsaved = super::automation::unsaved(conn, id)?;
    Ok(Some(AutomationView {
        automation,
        showing: Showing::Draft,
        saved,
        unsaved,
        placements,
        edges,
        wires,
    }))
}

/// **One automation as its newest saved version holds it** — `None` where that id names none, refused
/// where nobody has saved it.
///
/// What belongs to a placement itself — the answers to its settings and who carries out its steps — is
/// read off the version. The action standing on it is read the way [`detail`] reads it.
pub fn saved_detail(conn: &Connection, id: i64) -> Result<Option<AutomationView>> {
    let Some(mut automation) = read::automation(conn, id)? else { return Ok(None) };
    let Some(version) = read::automation_version_latest(conn, id)? else {
        return Err(Error::conflict(format!("automation '{id}' has no saved version")));
    };
    let answers: Vec<AutomationCfg> = rows(&version.cfgs)?;
    let chosen: Vec<AutomationPlacementStep> = rows(&version.placement_steps)?;
    let mut placements = Vec::new();
    for placement in rows::<AutomationPlacement>(&version.placements)? {
        let mut view = placement_view(conn, placement)?;
        let placement_id = view.placement.id;
        for setting in &mut view.settings {
            setting.value = answers
                .iter()
                .find(|a| {
                    a.owner_kind == AutomationCfgOwner::Placement
                        && a.owner_id == placement_id
                        && a.name == setting.name
                })
                .and_then(|a| a.value.clone());
        }
        for step in &mut view.steps {
            step.chosen = chosen
                .iter()
                .find(|c| c.placement_id == placement_id && c.step_id == step.step.id)
                .cloned();
        }
        placements.push(view);
    }
    automation.entry_placement_id = version.entry_placement_id;
    Ok(Some(AutomationView {
        automation,
        showing: Showing::Saved,
        saved: Some(SavedVersion { version: version.version, saved_at: version.created_at }),
        unsaved: super::automation::unsaved(conn, id)?,
        placements,
        edges: rows(&version.edges)?,
        wires: rows(&version.wires)?,
    }))
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
    let (saved, unsaved) = match action.builtin {
        Some(_) => (None, false),
        None => (
            read::automation_action_version_latest(conn, id)?
                .map(|v| SavedVersion { version: v.version, saved_at: v.created_at }),
            super::automation::action_unsaved(conn, id)?,
        ),
    };
    Ok(Some(ActionView {
        placed_at: placed_at(conn, &action)?,
        action,
        showing: Showing::Draft,
        saved,
        unsaved,
        used_by: used_by(conn, id)?,
        steps,
        edges,
        wires,
        exits,
        inputs,
        settings,
    }))
}

/// **One library action as its newest saved version holds it** — `None` where that id names none,
/// refused where nobody has saved it. A built-in's rows are its version, so it reads as [`action_detail`]
/// does.
pub fn action_saved_detail(conn: &Connection, id: i64) -> Result<Option<ActionView>> {
    let Some(mut action) = read::automation_action(conn, id)? else { return Ok(None) };
    if action.builtin.is_some() {
        return Ok(action_detail(conn, id)?.map(|view| ActionView { showing: Showing::Saved, ..view }));
    }
    let Some(version) = read::automation_action_version_latest(conn, id)? else {
        return Err(Error::conflict(format!("action '{id}' has no saved version")));
    };
    let saved = SavedRows {
        exits: rows(&version.exits)?,
        ports: rows(&version.ports)?,
    };
    let steps = rows::<AutomationStep>(&version.steps)?
        .into_iter()
        .map(|step| StepView {
            exits: saved.exits(AutomationOwner::Step, step.id),
            inputs: saved.ports(AutomationPortOwner::Step, step.id, AutomationPortDirection::In),
            step,
        })
        .collect();
    action.entry_step_id = version.entry_step_id;
    Ok(Some(ActionView {
        placed_at: placed_at(conn, &action)?,
        action,
        showing: Showing::Saved,
        saved: Some(SavedVersion { version: version.version, saved_at: version.created_at }),
        unsaved: super::automation::action_unsaved(conn, id)?,
        used_by: used_by(conn, id)?,
        steps,
        edges: rows(&version.edges)?,
        wires: rows(&version.wires)?,
        exits: saved.exits(AutomationOwner::Action, id),
        inputs: saved.ports(AutomationPortOwner::Action, id, AutomationPortDirection::In),
        settings: rows(&version.cfgs)?,
    }))
}

/// The ways out and the ports a saved version of an action holds, read back by their owner.
struct SavedRows {
    exits: Vec<AutomationExit>,
    ports: Vec<AutomationPort>,
}

impl SavedRows {
    fn exits(&self, owner_kind: AutomationOwner, owner_id: i64) -> Vec<ExitView> {
        self.exits
            .iter()
            .filter(|e| e.owner_kind == owner_kind && e.owner_id == owner_id)
            .map(|exit| ExitView {
                outputs: self.ports(AutomationPortOwner::Exit, exit.id, AutomationPortDirection::Out),
                exit: exit.clone(),
            })
            .collect()
    }

    fn ports(
        &self,
        owner_kind: AutomationPortOwner,
        owner_id: i64,
        direction: AutomationPortDirection,
    ) -> Vec<AutomationPort> {
        self.ports
            .iter()
            .filter(|p| p.owner_kind == owner_kind && p.owner_id == owner_id && p.direction == direction)
            .cloned()
            .collect()
    }
}

/// The rows a saved version keeps as JSON.
fn rows<T: serde::de::DeserializeOwned>(json: &str) -> Result<Vec<T>> {
    serde_json::from_str(json).map_err(Error::from)
}

/// **Every placement of an action**, with the version each stands on, in automation order. Every
/// placement of a built-in stands on the version its record is.
fn placed_at(conn: &Connection, action: &AutomationAction) -> Result<Vec<PlacedAt>> {
    let mut out = Vec::new();
    for placement_id in read::automation_placement_ids_using_action(conn, action.id)? {
        let Some(placement) = read::automation_placement(conn, placement_id)? else { continue };
        let version = match action.builtin {
            Some(_) => action.builtin_version,
            None => placement.version,
        };
        out.push(PlacedAt { automation_id: placement.automation_id, placement_id, version });
    }
    out.sort_by_key(|p| (p.automation_id, p.placement_id));
    Ok(out)
}

/// **Which version of its action a placement stands on, and the newest there is.** A built-in's comes
/// off its record, and its newest is the one this build defines (`AMB-D-1000`).
fn versions_of(
    conn: &Connection,
    placement: &AutomationPlacement,
    action: Option<&AutomationAction>,
) -> Result<(Option<i64>, Option<i64>)> {
    if let Some(action) = action {
        if let Some(key) = &action.builtin {
            let latest = super::automation_builtin::find(key).map(|b| b.version);
            return Ok((action.builtin_version, latest));
        }
    }
    let latest = read::automation_action_version_latest(conn, placement.action_id)?.map(|v| v.version);
    Ok((placement.version, latest))
}

/// One placement, with the action standing on it read in — as the version the placement stands on holds
/// it ([`ActionDef::placed`]): the ways out it can be left by, what it takes in, what it is set to, and
/// the steps inside it.
fn placement_view(conn: &Connection, placement: AutomationPlacement) -> Result<PlacementView> {
    let action = read::automation_action(conn, placement.action_id)?;
    let def = ActionDef::placed(conn, &placement)?;
    let entry_step = def.entry_step_id.and_then(|id| def.steps.iter().find(|s| s.id == id)).cloned();
    let exits = def
        .action_exits()
        .into_iter()
        .map(|exit| ExitView { outputs: def.outs_of(exit.id), exit })
        .collect();
    let inputs = def.action_inputs();
    // An action declares and the placement answers, and core is what puts the two rows back together —
    // the same pair the launch check reads, rather than a second reading of it.
    let settings = super::automation_run::answered(conn, &placement, &def)?;
    let mut steps = Vec::new();
    for step in def.steps {
        let chosen = read::automation_placement_step_for(conn, placement.id, step.id)?;
        steps.push(PlacementStepView { step, chosen });
    }
    let (version, latest_version) = versions_of(conn, &placement, action.as_ref())?;
    Ok(PlacementView {
        placement,
        action,
        version,
        latest_version,
        entry_step,
        exits,
        inputs,
        settings,
        steps,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::automation::{action_add, add, NewAutomation};
    use crate::ops::test_support::{mk_project, with_tx};

    /// **A placement is read as the version it stands on** — its steps, its ways out and what it takes
    /// in — while the action itself is read as it is being written.
    #[test]
    fn a_placement_is_read_as_the_version_it_stands_on() {
        use crate::model::AutomationPortKind;
        use crate::ops::automation::{
            action_from_prompt, action_version_add, exit_add, placement_add_by_hand, port_add, step_update,
            NewStep,
        };
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                add(tx, project, NewAutomation { name: "1件やりきる".into(), ..Default::default() }).unwrap();
            let action = action_from_prompt(tx, Some(project), NewStep::new("点検", "見る"), &[], &[]).unwrap();
            action_version_add(tx, action.id).unwrap();
            let placement = placement_add_by_hand(tx, automation.id, action.id).unwrap();
            let step = action.entry_step_id.unwrap();
            step_update(tx, step, None, Some("もう一度見る"), None, None, None, None, None, None, None, None)
                .unwrap();
            exit_add(tx, AutomationOwner::Action, action.id, Some("直す")).unwrap();
            let into = AutomationPortDirection::In;
            port_add(tx, AutomationPortOwner::Action, action.id, into, "指摘", AutomationPortKind::Value, false)
                .unwrap();

            let view = detail(tx.conn(), automation.id).unwrap().unwrap();
            let placed = view.placements.iter().find(|p| p.placement.id == placement.id).unwrap();
            assert_eq!(placed.steps[0].step.prompt, "見る");
            assert_eq!(placed.entry_step.as_ref().map(|s| s.prompt.as_str()), Some("見る"));
            assert!(!placed.exits.iter().any(|x| x.exit.name == "直す"));
            assert!(!placed.inputs.iter().any(|p| p.name == "指摘"));
            let written = action_detail(tx.conn(), action.id).unwrap().unwrap();
            assert_eq!(written.steps[0].step.prompt, "もう一度見る", "the action is read as it is written");
            assert!(written.exits.iter().any(|x| x.exit.name == "直す"));
        });
    }

    /// **A show names the version saved last and whether the draft holds more**, and each placement the
    /// version it stands on beside the newest there is — a built-in's off its record and this build.
    #[test]
    fn a_show_names_the_saved_version_and_the_version_each_placement_stands_on() {
        use crate::ops::automation::{placement_add, placement_version_set, version_add, action_version_add};
        use crate::ops::test_support::mk_placed;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                add(tx, project, NewAutomation { name: "1件やりきる".into(), ..Default::default() }).unwrap();
            let take = crate::ops::automation_builtin::action(tx, "take_task").unwrap();
            let entry = placement_add(tx, automation.id, take.id).unwrap();
            let (action, placement) = mk_placed(tx, &automation, "点検", "見る", "claude");
            action_version_add(tx, action.id).unwrap();

            let view = detail(tx.conn(), automation.id).unwrap().unwrap();
            assert_eq!(view.showing, Showing::Draft);
            assert!(view.saved.is_none());
            assert!(view.unsaved, "never saved");
            let placed = |view: &AutomationView, id: i64| {
                let one = view.placements.iter().find(|p| p.placement.id == id).unwrap();
                (one.version, one.latest_version)
            };
            assert_eq!(placed(&view, placement.id), (None, Some(1)), "put down before the action was saved");
            let built_in = crate::ops::automation_builtin::find("take_task").map(|b| b.version);
            assert_eq!(placed(&view, entry.id), (take.builtin_version, built_in));

            placement_version_set(tx, placement.id, 1).unwrap();
            action_version_add(tx, action.id).unwrap();
            version_add(tx, automation.id).unwrap();
            let view = detail(tx.conn(), automation.id).unwrap().unwrap();
            assert_eq!(placed(&view, placement.id), (Some(1), Some(2)));
            assert_eq!(view.saved.as_ref().map(|v| v.version), Some(1));
            assert!(!view.unsaved);
        });
    }

    /// **`--saved` reads the newest saved version**: who carries a step out as it was saved, while the
    /// draft reads what was chosen after. One nobody has saved is refused.
    #[test]
    fn the_saved_view_reads_the_placement_as_it_was_saved() {
        use crate::ops::automation::{placement_step_set, version_add};
        use crate::ops::test_support::mk_placed;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                add(tx, project, NewAutomation { name: "1件やりきる".into(), ..Default::default() }).unwrap();
            let (action, placement) = mk_placed(tx, &automation, "点検", "見る", "claude");
            assert!(matches!(saved_detail(tx.conn(), automation.id), Err(Error::Conflict(_))));

            version_add(tx, automation.id).unwrap();
            placement_step_set(tx, placement.id, action.entry_step_id.unwrap(), "codex", None).unwrap();
            let agent = |view: &AutomationView| {
                let one = view.placements.iter().find(|p| p.placement.id == placement.id).unwrap();
                one.steps[0].chosen.as_ref().map(|c| c.agent.clone())
            };
            let draft = detail(tx.conn(), automation.id).unwrap().unwrap();
            assert_eq!(agent(&draft), Some("codex".to_string()));
            let saved = saved_detail(tx.conn(), automation.id).unwrap().unwrap();
            assert_eq!(saved.showing, Showing::Saved);
            assert_eq!(agent(&saved), Some("claude".to_string()));
            assert!(saved.unsaved, "the draft holds what was chosen after");
            assert!(saved_detail(tx.conn(), automation.id + 1000).unwrap().is_none());
        });
    }

    /// **An action's show reads its saved version, and where it is placed at which version.** A
    /// built-in is never unsaved, and its saved view is its rows.
    #[test]
    fn an_action_show_reads_the_saved_version_and_where_it_is_placed() {
        use crate::ops::automation::{action_from_prompt, action_version_add, step_update, NewStep};
        use crate::ops::test_support::mk_placed;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                add(tx, project, NewAutomation { name: "1件やりきる".into(), ..Default::default() }).unwrap();
            let fresh = action_from_prompt(tx, Some(project), NewStep::new("直す", "直す"), &[], &[]).unwrap();
            assert!(matches!(action_saved_detail(tx.conn(), fresh.id), Err(Error::Conflict(_))));

            let (action, placement) = mk_placed(tx, &automation, "点検", "見る", "claude");
            action_version_add(tx, action.id).unwrap();
            let step = action.entry_step_id.unwrap();
            step_update(tx, step, None, Some("もう一度見る"), None, None, None, None, None, None, None, None)
                .unwrap();

            let draft = action_detail(tx.conn(), action.id).unwrap().unwrap();
            assert_eq!(draft.steps[0].step.prompt, "もう一度見る");
            assert_eq!(draft.saved.as_ref().map(|v| v.version), Some(1));
            assert!(draft.unsaved);
            let placed: Vec<_> =
                draft.placed_at.iter().map(|p| (p.automation_id, p.placement_id, p.version)).collect();
            assert_eq!(placed, vec![(automation.id, placement.id, None)]);

            let saved = action_saved_detail(tx.conn(), action.id).unwrap().unwrap();
            assert_eq!(saved.showing, Showing::Saved);
            assert_eq!(saved.steps[0].step.prompt, "見る");
            assert_eq!(saved.action.entry_step_id, Some(step));

            let take = crate::ops::automation_builtin::action(tx, "take_task").unwrap();
            let built_in = action_detail(tx.conn(), take.id).unwrap().unwrap();
            assert!(built_in.saved.is_none() && !built_in.unsaved);
            let built_in = action_saved_detail(tx.conn(), take.id).unwrap().unwrap();
            assert_eq!(built_in.showing, Showing::Saved);
        });
    }

    fn names(cards: &[ProjectAutomationCard]) -> Vec<(String, String)> {
        cards
            .iter()
            .map(|one| (one.project_name.clone(), one.card.automation.name.clone()))
            .collect()
    }

    #[test]
    fn every_card_lists_each_projects_automations_with_its_name() {
        with_tx(|tx| {
            let amenbo = mk_project(tx, "amenbo");
            let site = mk_project(tx, "site");
            add(tx, amenbo, NewAutomation { name: "1件やりきる".into(), ..Default::default() }).unwrap();
            add(tx, site, NewAutomation { name: "記事を出す".into(), ..Default::default() }).unwrap();
            add(tx, amenbo, NewAutomation { name: "起票する".into(), ..Default::default() }).unwrap();

            let all = every_card(tx.conn(), None).unwrap();
            assert_eq!(
                names(&all),
                vec![
                    ("amenbo".into(), "1件やりきる".into()),
                    ("amenbo".into(), "起票する".into()),
                    ("site".into(), "記事を出す".into()),
                ]
            );
        });
    }

    #[test]
    fn every_card_keeps_an_archived_projects_automations() {
        with_tx(|tx| {
            let old = mk_project(tx, "old");
            add(tx, old, NewAutomation { name: "片付ける".into(), ..Default::default() }).unwrap();
            crate::ops::project::set_archived(tx, old, true).unwrap();

            let all = every_card(tx.conn(), None).unwrap();
            assert_eq!(names(&all), vec![("old".into(), "片付ける".into())]);
        });
    }

    #[test]
    fn every_card_through_a_bound_reach_is_that_project_alone() {
        with_tx(|tx| {
            let amenbo = mk_project(tx, "amenbo");
            let site = mk_project(tx, "site");
            add(tx, amenbo, NewAutomation { name: "1件やりきる".into(), ..Default::default() }).unwrap();
            add(tx, site, NewAutomation { name: "記事を出す".into(), ..Default::default() }).unwrap();

            let bound = every_card(tx.conn(), Some(site)).unwrap();
            assert_eq!(names(&bound), vec![("site".into(), "記事を出す".into())]);
        });
    }

    fn action_names(cards: &[ProjectActionCard]) -> Vec<(Option<String>, String)> {
        cards
            .iter()
            .map(|one| (one.project_name.clone(), one.card.action.name.clone()))
            .collect()
    }

    #[test]
    fn every_action_card_lists_the_devices_then_each_projects_with_its_name() {
        with_tx(|tx| {
            let amenbo = mk_project(tx, "amenbo");
            let site = mk_project(tx, "site");
            action_add(tx, Some(site), "記事を書く", "").unwrap();
            action_add(tx, None, "レビューする", "").unwrap();
            action_add(tx, Some(amenbo), "実装する", "").unwrap();

            let all = every_action_card(tx.conn(), None).unwrap();
            assert_eq!(
                action_names(&all),
                vec![
                    (None, "レビューする".into()),
                    (Some("amenbo".into()), "実装する".into()),
                    (Some("site".into()), "記事を書く".into()),
                ]
            );
        });
    }

    #[test]
    fn every_action_card_through_a_bound_reach_keeps_the_devices_shelf() {
        with_tx(|tx| {
            let amenbo = mk_project(tx, "amenbo");
            let site = mk_project(tx, "site");
            action_add(tx, None, "レビューする", "").unwrap();
            action_add(tx, Some(amenbo), "実装する", "").unwrap();
            action_add(tx, Some(site), "記事を書く", "").unwrap();

            let bound = every_action_card(tx.conn(), Some(site)).unwrap();
            assert_eq!(
                action_names(&bound),
                vec![(None, "レビューする".into()), (Some("site".into()), "記事を書く".into())]
            );
        });
    }
}
