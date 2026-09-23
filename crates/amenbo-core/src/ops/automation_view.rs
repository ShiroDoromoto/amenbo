//! **Reading a definition back**, with the declarations each box runs under already resolved — the one
//! shape both a build screen and a terminal read it in.
//!
//! The ten definition tables are written by [`super::automation`]; nothing here writes. What this
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
    AutomationOwner, AutomationPictureOwner, AutomationPlacement, AutomationPlacementStep,
    AutomationPort,
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
/// order (`AMB-D-961`). While there is one, every rewrite of the automation is refused
/// ([`super::automation`]), and a build screen reads the same answer to hold its fields shut and name
/// the runs a reader would have to end.
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
    let mut steps = Vec::new();
    for step in read::automation_action_steps_of(conn, placement.action_id)? {
        let chosen = read::automation_placement_step_for(conn, placement.id, step.id)?;
        steps.push(PlacementStepView { step, chosen });
    }
    Ok(PlacementView { placement, action, entry_step, exits, inputs, settings, steps })
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
    use crate::ops::automation::{add, NewAutomation};
    use crate::ops::test_support::{mk_project, with_tx};

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
}
