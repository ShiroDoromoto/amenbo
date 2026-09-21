//! **Reading an automation's definition, and saying whether it could be started** — the doors behind
//! the automations screen and its build screen.
//!
//! Core owns the ten definition tables and the writes that build them
//! ([`amenbo_core::ops::automation`]); nothing is built here. What this side does is resolve and
//! shape: a step reads its ways out, its inputs and its settings from the library action it points at
//! or from itself, and a screen that had to know which of the two declared a name would be drawing
//! the storage rather than the automation.
//!
//! **The launch check here is the picture, not the ruling.** What refuses a launch is the launch
//! itself, which writes a run and reads the machine at the moment of the press. This one is read
//! before anybody presses, so the build screen can say what is in the way while it is still being
//! built — and so the button is not offered where pressing it would only fail.
//!
//! **What this machine can start is passed in rather than asked here.** Whether an agent is
//! installed is a question about a login shell and the reader's own profile, and the face already
//! holds the answer it asked for the empty frame ([`crate::wake`]). Handing it down keeps every rule
//! about a definition in one place and puts no second probe behind a screen that only draws.

use amenbo_core::model::{
    Automation, AutomationCfg, AutomationEnds, AutomationExit, AutomationPort,
    AutomationPortDirection, AutomationPortOwner, AutomationStep, ERROR_EXIT,
};
use amenbo_core::ops::automation::declarer;
use amenbo_core::store_engine::{read, StoreEngine};

use crate::commands::open_store_read;
use crate::dto::{
    AutomationCardDto, AutomationCfgDto, AutomationDetailDto, AutomationEdgeDto, AutomationExitDto,
    AutomationLaunchBlockDto, AutomationLaunchCheckDto, AutomationPortDto, AutomationStepDto,
    AutomationWireDto,
};
use crate::error::CmdError;

/// The automations of one project, in the order they were placed in.
///
/// Archived ones come too: what an archived automation is, is one that is kept out of the way rather
/// than gone, and which of the two lists shows it is the screen's to decide.
#[tauri::command]
pub fn automation_page(project_id: i64) -> Result<Vec<AutomationCardDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_page");
    let store = open_store_read()?;
    let engine = store.read_model();
    let conn = engine.conn();
    let mut cards = Vec::new();
    for (id, _) in read::automation_siblings(conn, project_id, None)? {
        let Some(row) = read::automation(conn, id)? else { continue };
        cards.push(AutomationCardDto {
            id: row.id,
            name: row.name,
            steps: read::automation_step_ids(conn, id)?.len(),
            archived: row.archived,
        });
    }
    Ok(cards)
}

/// One automation's whole definition, or nothing where that id names none.
#[tauri::command]
pub fn automation_detail(id: i64) -> Result<Option<AutomationDetailDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_detail");
    let store = open_store_read()?;
    let engine = store.read_model();
    let Some(row) = read::automation(engine.conn(), id)? else { return Ok(None) };
    Ok(Some(detail(engine, row)?))
}

/// Whether this automation could be started, and everything standing in the way of it.
///
/// `agents` is what this machine can start ([`crate::wake::wake_choices`]). An empty list is "the
/// face could not ask", and then no step is judged on its agent: a reason drawn off an answer nobody
/// got would tell a reader to install what they already have.
///
/// `workspaceOpen` is whether there is a workspace to open the run's panes in. It is asked separately
/// from the five reasons a definition can be unfinished, because it is not about the definition at
/// all — the same automation is launchable again the moment a workspace is up.
#[tauri::command]
pub fn automation_launch_check(
    id: i64,
    agents: Vec<String>,
    workspace_open: bool,
) -> Result<AutomationLaunchCheckDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("automation_launch_check");
    let store = open_store_read()?;
    let engine = store.read_model();
    let Some(row) = read::automation(engine.conn(), id)? else {
        return Ok(AutomationLaunchCheckDto { ready: false, blocks: Vec::new() });
    };
    let detail = detail(engine, row)?;
    Ok(check(&detail, &agents, workspace_open))
}

// ───────────────────────────── shaping ─────────────────────────────

/// One automation's ten tables, read and resolved into the one shape every part of the build screen
/// works from.
fn detail(engine: &StoreEngine, row: Automation) -> Result<AutomationDetailDto, CmdError> {
    let conn = engine.conn();
    let automation_id = row.id;
    let mut steps = Vec::new();
    for (step_id, _) in read::automation_step_siblings(conn, automation_id, None)? {
        let Some(step) = read::automation_step(conn, step_id)? else { continue };
        steps.push(step_dto(engine, step)?);
    }
    let mut edges = Vec::new();
    for edge_id in read::automation_edge_ids(conn, automation_id)? {
        let Some(edge) = read::automation_edge(conn, edge_id)? else { continue };
        edges.push(AutomationEdgeDto {
            id: edge.id,
            from_step_id: edge.from_step_id,
            exit_name: edge.exit_name,
            to_step_id: edge.to_step_id,
            ends: edge.ends.as_str(),
            max_times: edge.max_times,
        });
    }
    let mut wires = Vec::new();
    for wire_id in read::automation_wire_ids(conn, automation_id)? {
        let Some(wire) = read::automation_wire(conn, wire_id)? else { continue };
        wires.push(AutomationWireDto {
            id: wire.id,
            from_step_id: wire.from_step_id,
            from_exit_name: wire.from_exit_name,
            from_port_name: wire.from_port_name,
            to_step_id: wire.to_step_id,
            to_port_name: wire.to_port_name,
        });
    }
    Ok(AutomationDetailDto {
        id: row.id,
        project_id: row.project_id,
        name: row.name,
        notes: row.notes,
        preamble: row.preamble,
        entry_step_id: row.entry_step_id,
        archived: row.archived,
        steps,
        edges,
        wires,
    })
}

/// One step, with the action it points at read in: the prompt it runs on, the ways out it can leave
/// by, what it takes and what it is set to.
fn step_dto(engine: &StoreEngine, step: AutomationStep) -> Result<AutomationStepDto, CmdError> {
    let conn = engine.conn();
    let action = match step.action_id {
        Some(action_id) => read::automation_action(conn, action_id)?,
        None => None,
    };
    let (owner_kind, owner_id) = declarer(&step);
    let port_owner = match owner_kind {
        amenbo_core::model::AutomationOwner::Step => AutomationPortOwner::Step,
        amenbo_core::model::AutomationOwner::Action => AutomationPortOwner::Action,
    };

    let mut exits = Vec::new();
    for exit in read::automation_exits_of(conn, owner_kind, owner_id)? {
        exits.push(exit_dto(engine, exit)?);
    }
    let inputs = read::automation_ports_of(conn, port_owner, owner_id, AutomationPortDirection::In)?
        .into_iter()
        .map(port_dto)
        .collect();

    // An action declares and the step answers, so the two rows under one name are folded into one:
    // the declaration decides the kind and whether it is required, and the step's row carries the
    // value. A step that carries its own prompt declares and answers in the same row.
    let answers = read::automation_cfgs_of(conn, amenbo_core::model::AutomationOwner::Step, step.id)?;
    let settings = read::automation_cfgs_of(conn, owner_kind, owner_id)?
        .into_iter()
        .map(|declared| {
            let value = answers
                .iter()
                .find(|answer| answer.name == declared.name)
                .and_then(|answer| answer.value.clone())
                .or_else(|| declared.value.clone());
            cfg_dto(declared, value)
        })
        .collect();

    Ok(AutomationStepDto {
        id: step.id,
        name: step.name,
        action_id: step.action_id,
        action_name: action.as_ref().map(|one| one.name.clone()),
        prompt: step
            .prompt
            .clone()
            .or_else(|| action.as_ref().map(|one| one.prompt.clone()))
            .unwrap_or_default(),
        agent: step.agent,
        model: step.model,
        interactive: step.interactive,
        work_dir_ref: step.work_dir_ref,
        report_to_task: step.report_to_task,
        show_history: step.show_history,
        exits,
        inputs,
        settings,
    })
}

fn exit_dto(engine: &StoreEngine, exit: AutomationExit) -> Result<AutomationExitDto, CmdError> {
    let conn = engine.conn();
    let outputs =
        read::automation_ports_of(conn, AutomationPortOwner::Exit, exit.id, AutomationPortDirection::Out)?
            .into_iter()
            .map(port_dto)
            .collect();
    Ok(AutomationExitDto { id: exit.id, name: exit.name, outputs })
}

fn port_dto(port: AutomationPort) -> AutomationPortDto {
    AutomationPortDto { name: port.name, kind: port.kind.as_str(), required: port.required }
}

fn cfg_dto(declared: AutomationCfg, value: Option<String>) -> AutomationCfgDto {
    AutomationCfgDto {
        name: declared.name,
        kind: declared.kind.as_str(),
        required: declared.required,
        options: declared.options,
        value,
    }
}

// ───────────────────────────── the launch check ─────────────────────────────

/// Everything standing in the way of starting this definition.
///
/// **Only the steps a run would actually reach are judged.** The walk is from the entry step along
/// the edges that go on to another step; a step nobody arrives at contributes nothing, because it is
/// half-built scaffolding rather than a fault in what would run. An automation with no entry step
/// reaches nothing at all, and comes out as `task_undecided`: with no step to open on, nothing ever
/// picks up the task the run is about.
fn check(
    detail: &AutomationDetailDto,
    agents: &[String],
    workspace_open: bool,
) -> AutomationLaunchCheckDto {
    let mut blocks = Vec::new();
    if !workspace_open {
        blocks.push(AutomationLaunchBlockDto {
            reason: "workspace_closed",
            step_id: None,
            step_name: None,
            at: None,
        });
    }
    if detail.steps.is_empty() {
        // The only reason worth saying on its own: every other one is about a step, and there are none.
        blocks.push(AutomationLaunchBlockDto {
            reason: "no_steps",
            step_id: None,
            step_name: None,
            at: None,
        });
        return AutomationLaunchCheckDto { ready: false, blocks };
    }

    let reached = reachable(detail);
    for step in detail.steps.iter().filter(|step| reached.contains(&step.id)) {
        for exit in &step.exits {
            // The error way out is the one nobody has to answer for: left alone it stops the run and
            // calls a person, and the picture only draws it where somebody changed that.
            if exit.name.as_deref() == Some(ERROR_EXIT) {
                continue;
            }
            let decided = detail
                .edges
                .iter()
                .any(|edge| edge.from_step_id == step.id && edge.exit_name == exit.name);
            if !decided {
                blocks.push(AutomationLaunchBlockDto {
                    reason: "exit_without_next",
                    step_id: Some(step.id),
                    step_name: Some(step.name.clone()),
                    at: exit.name.clone(),
                });
            }
        }
        for input in step.inputs.iter().filter(|port| port.required) {
            let fed = detail
                .wires
                .iter()
                .find(|wire| wire.to_step_id == step.id && wire.to_port_name == input.name);
            // Nothing feeding it, and something feeding it from a step the run never arrives at, are
            // the same thing to whoever presses: the value is not there when the step asks for it.
            if !fed.is_some_and(|wire| reached.contains(&wire.from_step_id)) {
                blocks.push(AutomationLaunchBlockDto {
                    reason: "input_unfed",
                    step_id: Some(step.id),
                    step_name: Some(step.name.clone()),
                    at: Some(input.name.clone()),
                });
            }
        }
        if !agents.is_empty() && !agents.iter().any(|one| one == &step.agent) {
            blocks.push(AutomationLaunchBlockDto {
                reason: "agent_not_here",
                step_id: Some(step.id),
                step_name: Some(step.name.clone()),
                at: Some(step.agent.clone()),
            });
        }
    }

    let takes_a_task = detail
        .steps
        .iter()
        .filter(|step| reached.contains(&step.id))
        .any(|step| step.inputs.iter().any(|port| port.kind == "task_take"));
    if !takes_a_task {
        blocks.push(AutomationLaunchBlockDto {
            reason: "task_undecided",
            step_id: None,
            step_name: None,
            at: None,
        });
    }

    AutomationLaunchCheckDto { ready: blocks.is_empty(), blocks }
}

/// The steps a run would arrive at, walked from the entry step along the edges that go on.
fn reachable(detail: &AutomationDetailDto) -> std::collections::HashSet<i64> {
    let mut seen = std::collections::HashSet::new();
    let Some(entry) = detail.entry_step_id else { return seen };
    if !detail.steps.iter().any(|step| step.id == entry) {
        return seen;
    }
    let mut walking = vec![entry];
    seen.insert(entry);
    while let Some(at) = walking.pop() {
        for edge in detail.edges.iter().filter(|edge| edge.from_step_id == at) {
            if edge.ends != AutomationEnds::Go.as_str() {
                continue;
            }
            let Some(next) = edge.to_step_id else { continue };
            if detail.steps.iter().any(|step| step.id == next) && seen.insert(next) {
                walking.push(next);
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(name: &str, kind: &'static str, required: bool) -> AutomationPortDto {
        AutomationPortDto { name: name.to_string(), kind, required }
    }

    fn step(id: i64, name: &str) -> AutomationStepDto {
        AutomationStepDto {
            id,
            name: name.to_string(),
            action_id: None,
            action_name: None,
            prompt: "do it".to_string(),
            agent: "claude-code".to_string(),
            model: None,
            interactive: false,
            work_dir_ref: None,
            report_to_task: false,
            show_history: true,
            // The unnamed way out and the error one, which is what core gives every step at birth.
            exits: vec![
                AutomationExitDto { id: id * 10, name: None, outputs: Vec::new() },
                AutomationExitDto {
                    id: id * 10 + 1,
                    name: Some(ERROR_EXIT.to_string()),
                    outputs: Vec::new(),
                },
            ],
            inputs: vec![port("task", "task_take", true)],
            settings: Vec::new(),
        }
    }

    fn done_edge(id: i64, from: i64) -> AutomationEdgeDto {
        AutomationEdgeDto {
            id,
            from_step_id: from,
            exit_name: None,
            to_step_id: None,
            ends: "done",
            max_times: None,
        }
    }

    /// One step, fed, with its way out decided — the smallest automation that can be started.
    fn ready_one() -> AutomationDetailDto {
        let mut only = step(1, "はじめ");
        only.inputs = vec![port("task", "task_take", false)];
        AutomationDetailDto {
            id: 7,
            project_id: 1,
            name: "朝の巡回".to_string(),
            notes: String::new(),
            preamble: String::new(),
            entry_step_id: Some(1),
            archived: false,
            steps: vec![only],
            edges: vec![done_edge(100, 1)],
            wires: Vec::new(),
        }
    }

    #[test]
    fn a_finished_automation_is_ready() {
        let verdict = check(&ready_one(), &["claude-code".to_string()], true);
        assert!(verdict.ready, "{:?}", verdict.blocks.iter().map(|b| b.reason).collect::<Vec<_>>());
    }

    #[test]
    fn no_steps_is_said_on_its_own() {
        let mut detail = ready_one();
        detail.steps.clear();
        detail.edges.clear();
        detail.entry_step_id = None;
        let verdict = check(&detail, &[], true);
        assert!(!verdict.ready);
        assert_eq!(verdict.blocks.iter().map(|b| b.reason).collect::<Vec<_>>(), ["no_steps"]);
    }

    #[test]
    fn a_way_out_with_nothing_after_it_is_named() {
        let mut detail = ready_one();
        detail.edges.clear();
        let verdict = check(&detail, &[], true);
        let named: Vec<_> = verdict
            .blocks
            .iter()
            .filter(|b| b.reason == "exit_without_next")
            .map(|b| (b.step_name.clone(), b.at.clone()))
            .collect();
        assert_eq!(named, [(Some("はじめ".to_string()), None)]);
    }

    #[test]
    fn the_error_way_out_is_not_one_to_answer_for() {
        let verdict = check(&ready_one(), &[], true);
        assert!(verdict.blocks.iter().all(|b| b.at.as_deref() != Some(ERROR_EXIT)));
    }

    #[test]
    fn a_required_input_nobody_feeds_is_named() {
        let mut detail = ready_one();
        detail.steps[0].inputs = vec![port("folder", "value", true)];
        let verdict = check(&detail, &[], true);
        let named: Vec<_> = verdict
            .blocks
            .iter()
            .filter(|b| b.reason == "input_unfed")
            .map(|b| b.at.clone())
            .collect();
        assert_eq!(named, [Some("folder".to_string())]);
    }

    #[test]
    fn an_input_fed_from_a_step_the_run_never_reaches_is_unfed() {
        let mut detail = ready_one();
        detail.steps[0].inputs = vec![port("task", "task_take", false), port("note", "value", true)];
        detail.steps.push(step(2, "はなれ"));
        detail.wires.push(AutomationWireDto {
            id: 300,
            from_step_id: 2,
            from_exit_name: None,
            from_port_name: "out".to_string(),
            to_step_id: 1,
            to_port_name: "note".to_string(),
        });
        let verdict = check(&detail, &[], true);
        assert!(verdict.blocks.iter().any(|b| b.reason == "input_unfed" && b.at.as_deref() == Some("note")));
    }

    #[test]
    fn a_step_the_run_never_reaches_is_not_judged() {
        let mut detail = ready_one();
        let mut stray = step(2, "はなれ");
        stray.exits = vec![AutomationExitDto { id: 20, name: None, outputs: Vec::new() }];
        detail.steps.push(stray);
        let verdict = check(&detail, &["claude-code".to_string()], true);
        assert!(verdict.ready, "{:?}", verdict.blocks.iter().map(|b| b.reason).collect::<Vec<_>>());
    }

    #[test]
    fn an_agent_this_machine_cannot_start_is_named() {
        let mut detail = ready_one();
        detail.steps[0].agent = "codex-cli".to_string();
        let verdict = check(&detail, &["claude-code".to_string()], true);
        let named: Vec<_> = verdict
            .blocks
            .iter()
            .filter(|b| b.reason == "agent_not_here")
            .map(|b| b.at.clone())
            .collect();
        assert_eq!(named, [Some("codex-cli".to_string())]);
    }

    #[test]
    fn no_answer_about_this_machine_judges_no_agent() {
        let mut detail = ready_one();
        detail.steps[0].agent = "codex-cli".to_string();
        let verdict = check(&detail, &[], true);
        assert!(verdict.blocks.iter().all(|b| b.reason != "agent_not_here"));
    }

    #[test]
    fn nothing_that_picks_up_a_task_is_said_once() {
        let mut detail = ready_one();
        detail.steps[0].inputs = Vec::new();
        let verdict = check(&detail, &[], true);
        assert_eq!(
            verdict.blocks.iter().filter(|b| b.reason == "task_undecided").count(),
            1
        );
    }

    #[test]
    fn an_automation_with_no_entry_step_decides_no_task() {
        let mut detail = ready_one();
        detail.entry_step_id = None;
        let verdict = check(&detail, &[], true);
        assert_eq!(verdict.blocks.iter().map(|b| b.reason).collect::<Vec<_>>(), ["task_undecided"]);
    }

    #[test]
    fn a_closed_workspace_is_said_apart_from_the_definition() {
        let verdict = check(&ready_one(), &["claude-code".to_string()], false);
        assert!(!verdict.ready);
        assert_eq!(verdict.blocks.iter().map(|b| b.reason).collect::<Vec<_>>(), ["workspace_closed"]);
    }
}
