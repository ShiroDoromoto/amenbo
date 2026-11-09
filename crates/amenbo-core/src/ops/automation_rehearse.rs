//! **A test run** — an automation walked role by role, each from where its round starts to its end, with
//! nothing carried out, so a person can see how it would go before anything real is set moving.
//!
//! **Each role is walked in turn** (`AMB-D-1123`), in the order the roles are defined in: a task of
//! that role is filed, a run is launched in its seat, and that run is walked from where the role's round
//! starts — the step that takes a task — to its end. The next role is walked once the run before it has
//! completed. An automation none of whose roles names where its round starts is refused by the launch
//! check, as a launch of it is.
//!
//! **It walks the same road a run does.** The launch check refuses it where it would refuse a launch,
//! the steps are copied into a run, each step is opened by [`super::automation_step`] — which resolves
//! what is wired into it, reads the answers where it was placed and composes the whole prompt an agent
//! would be started on — and each report goes through [`super::automation_report::done`], which is what
//! decides where the run goes next. A second walker written for tests alone would be a second reading of
//! the picture, and it would drift from the one that runs.
//!
//! **It tries the draft** ([`automation_run::ReadsFrom::Draft`]) — the automation as it is being written,
//! saved or not — where a launch reads the saved definition.
//!
//! **What is stood in for is the work**: no agent is started, and no built-in is carried out — no task
//! is filed, taken or closed, no worktree is cut or folded, nothing is fetched or merged. In their place
//! every output of the way out a step leaves by is put down as a placeholder
//! ([`crate::run_wording::rehearsal`]), and a step that hands on a task hands on one made up for the
//! purpose, filed under the role of the seat the run sits in, so the round goes on where that role
//! starts.
//!
//! **Nothing it writes is kept.** The whole walk is one write transaction that is never committed
//! ([`crate::store::Store::automation_rehearse`]): the run, its steps, the task filed from what was
//! handed over ([`HandedAtLaunch`]) with the first run, the placeholder tasks and the lines a run leaves
//! on its task are all gone the moment it returns, and nobody watching the store sees any of them. What
//! survives is the account handed back ([`Rehearsal`]).
//!
//! **Which way out a step takes is chosen here**, since nobody is doing the work that would decide it.
//! The first time a step is reached it leaves by the first way out it declares; each time it is reached
//! again, by the next one. The error way out is never chosen, and neither is one a built-in's answers
//! say it never leaves by. A step reached again with no way out left untried ends the walk
//! ([`Cut::Looped`]). Which ways out were taken is counted again for each role's run.
//!
//! **The built-in that takes a task, reached again, finds none** (`AMB-D-1130`): the run ends there
//! before the step, completed, as a real one with nothing to take does
//! ([`automation_run::end_with_no_task`]). That walks a picture that goes round — a round that comes
//! back to take the next task — once round and then out.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::{
    ActorKind, AttachmentTarget, AutomationPortKind, AutomationRun, AutomationRunDef, AutomationRunStatus,
    AutomationStoppedReason, RunDefExit, TaskStatus, ERROR_EXIT,
};
use crate::ops::automation_builtin_close;
use crate::ops::automation_builtin_defer;
use crate::ops::automation_builtin_hand_back;
use crate::ops::automation_builtin_take;
use crate::ops::automation_report::{self, Next, Produced};
use crate::ops::automation_run::{self, HandedAtLaunch, Launcher, ReadsFrom, Waiting};
use crate::ops::automation_step::{self, Opened};
use crate::ops::task::{self, NewTask};
use crate::store_engine::{read, record, WriteTx};

/// **How many steps a test run opens at most.** A picture whose lines are all limited ends well before
/// this; one that is not would otherwise walk round for ever.
const MOST: usize = 200;

/// **What a test run found** — every step it opened, role after role in order, and how it ended.
#[derive(Clone, Debug, Serialize)]
pub struct Rehearsal {
    pub steps: Vec<Rehearsed>,
    /// Where the last run walked stood when the walk ended: `completed` where the picture ran out for
    /// every role, `failed` where that run would have been stopped (`stopped_reason` saying why), or
    /// `running` where the walk was cut short ([`Rehearsal::cut`]). A run that does not complete ends the
    /// walk, and the roles after it are not walked.
    pub status: AutomationRunStatus,
    pub stopped_reason: Option<AutomationStoppedReason>,
    /// The required inputs the last step opened found nothing wired into — the run is stopped there, as
    /// a real one would be. Empty otherwise.
    pub missing: Vec<String>,
    /// The agent the last step asked for that this machine cannot start. `None` otherwise.
    pub no_agent: Option<String>,
    /// Why the walk ended before the run did, or `None` where the run ended on its own.
    pub cut: Option<Cut>,
}

/// **Why a test run stopped walking** before the run came to an end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cut {
    /// A step was reached again with every way out it can take already taken.
    Looped,
    /// [`MOST`] steps were opened.
    TooLong,
}

/// **One step a test run opened.**
#[derive(Clone, Debug, Serialize)]
pub struct Rehearsed {
    /// The role whose seat the run that opened it sat in, or `None` for the seats of no role.
    pub role_id: Option<i64>,
    /// The placement on the automation's picture it was opened from.
    pub placement_id: Option<i64>,
    /// The step inside the placed action.
    pub step_id: Option<i64>,
    pub name: String,
    /// The built-in it is, or `None` for an agent's step.
    pub builtin: Option<String>,
    /// The agent that would have been started, and its model. `None` for a built-in.
    pub agent: Option<String>,
    pub model: Option<String>,
    /// **The whole text the agent would have been started on** — preamble, the task, what came before,
    /// what is wired in and the step's own prompt. `None` for a built-in.
    pub prompt: Option<String>,
    /// The folder its terminal would have been opened in, where the step names one.
    pub folder: Option<String>,
    /// The way out it was taken to leave by.
    pub exit: String,
}

/// **Walk an automation role by role, each from where its round starts to its end, with nothing
/// carried out** — see the module.
///
/// `tx` is expected to be thrown away by the caller: everything written here is only there to be read
/// back by the next step.
pub fn rehearse(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    handed: &HandedAtLaunch,
) -> Result<Rehearsal> {
    // No pane is opened, so whether the window is open is nobody's concern here.
    let by = Launcher { workspace_open: None, ..*by };
    let mut rehearsal = Rehearsal {
        steps: Vec::new(),
        status: AutomationRunStatus::Completed,
        stopped_reason: None,
        missing: Vec::new(),
        no_agent: None,
        cut: None,
    };
    let nothing = HandedAtLaunch::default();
    let project_id = read::automation(tx.conn(), automation_id)?
        .ok_or_else(|| Error::not_found(format!("automation '{automation_id}' not found")))?
        .project_id;
    let seats = automation_run::rehearsal_seats(tx.conn(), automation_id, ReadsFrom::Draft)?;
    for (i, role_id) in seats.into_iter().enumerate() {
        // What was handed is filed once, with the first run, as a launch files it.
        let handed = if i == 0 { handed } else { &nothing };
        let mut filed = Some(made_up(tx, project_id, role_id)?);
        let run =
            automation_run::launch_in_seat_past_the_limit(tx, automation_id, &by, handed, ReadsFrom::Draft, role_id)?;
        walk_run(tx, &run, &by, &mut filed, &mut rehearsal)?;
        let ended = read::automation_run(tx.conn(), run.id)?.unwrap_or(run);
        rehearsal.status = ended.status;
        rehearsal.stopped_reason = ended.stopped_reason;
        let walked_out = ended.status == AutomationRunStatus::Completed
            && rehearsal.cut.is_none()
            && rehearsal.missing.is_empty()
            && rehearsal.no_agent.is_none();
        if !walked_out {
            break;
        }
    }
    Ok(rehearsal)
}

/// **Walk one run of a test run** from where it starts to its end, putting every step it opens on
/// `rehearsal`. `filed` is the task filed under the run's role before it was launched, which the first
/// step that takes a task takes.
fn walk_run(
    tx: &WriteTx<'_>,
    run: &AutomationRun,
    by: &Launcher<'_>,
    filed: &mut Option<i64>,
    rehearsal: &mut Rehearsal,
) -> Result<()> {
    let mut visits: BTreeMap<i64, usize> = BTreeMap::new();
    while let Waiting::Step(def) = automation_run::next_def(tx.conn(), run.id)? {
        if rehearsal.steps.len() >= MOST {
            rehearsal.cut = Some(Cut::TooLong);
            break;
        }
        if visits.get(&def.id).is_some_and(|seen| *seen > 0)
            && def.builtin.as_deref() == Some(automation_builtin_take::TAKE_TASK.key)
        {
            let standing = read::automation_run(tx.conn(), run.id)?
                .ok_or_else(|| Error::not_found(format!("automation run '{}' not found", run.id)))?;
            automation_run::end_with_no_task(tx, standing)?;
            break;
        }
        let opening = match automation_step::open_rehearsing(tx, run.id, def.id, by.startable)? {
            Opened::Ready(opening) => *opening,
            Opened::Stopped { missing, .. } => {
                rehearsal.missing = missing;
                break;
            }
            Opened::NoAgent { agent, .. } => {
                rehearsal.no_agent = Some(agent);
                break;
            }
            // A built-in is never carried out, held or waited on here; a run that took a fresh task
            // with the last still open has been failed, which is read off the run by the caller.
            _ => break,
        };
        let def = opening.run_def;
        let exits: Vec<RunDefExit> = serde_json::from_str(&def.exits).map_err(Error::from)?;
        let seen = visits.entry(def.id).or_default();
        let Some(exit) = choose(&def, &exits, *seen)? else {
            rehearsal.cut = Some(Cut::Looped);
            break;
        };
        *seen += 1;
        let builtin = def.builtin.is_some();
        rehearsal.steps.push(Rehearsed {
            role_id: run.role_id,
            placement_id: def.placement_id,
            step_id: def.step_id,
            name: def.name.clone(),
            builtin: def.builtin.clone(),
            agent: (!builtin).then(|| def.agent.clone()),
            model: def.model.clone().filter(|_| !builtin),
            prompt: (!builtin).then_some(opening.text),
            folder: opening.folder,
            exit: exit.name.clone(),
        });
        stand_in(tx, run, opening.run_step.id, &def, exit, filed)?;
        let report = crate::run_wording::rehearsal(tx.language(), "report", &[("exit", &exit.name)]);
        if let Next::Closed(_) | Next::Halted(_) | Next::Paused(_) =
            automation_report::done(tx, opening.run_step.id, Some(exit.id), &report)?
        {
            break;
        }
    }
    Ok(())
}

/// **The way out a step reached for the `seen`+1-th time leaves by** — the module says which. `None`
/// where every one it can take has been taken already.
fn choose<'e>(def: &AutomationRunDef, exits: &'e [RunDefExit], seen: usize) -> Result<Option<&'e RunDefExit>> {
    let never = crate::ops::automation_builtin::never_leaves_by_def(def)?;
    let open: Vec<&RunDefExit> =
        exits.iter().filter(|e| e.name != ERROR_EXIT && Some(e.name.as_str()) != never).collect();
    Ok(match open.is_empty() {
        // A step that declares nothing but the error way out can leave by nothing else, once.
        true => exits.iter().find(|e| e.name == ERROR_EXIT).filter(|_| seen == 0),
        false => open.get(seen).copied(),
    })
}

/// **Put down what the step would have handed on through `exit`**, each output a placeholder of its
/// kind — and, for the built-ins that close the task, hand it back to a person or put it off, close the
/// one the run holds or put it back in `todo`, so the next task it takes is not taken with this one still
/// open.
///
/// A task taken is `filed` — the one filed under the run's role before it was launched — the first time,
/// and one made up under that role after that.
fn stand_in(
    tx: &WriteTx<'_>,
    run: &AutomationRun,
    run_step_id: i64,
    def: &AutomationRunDef,
    exit: &RunDefExit,
    filed: &mut Option<i64>,
) -> Result<()> {
    for port in &exit.outs {
        match port.kind {
            AutomationPortKind::Value => {
                let value = crate::run_wording::rehearsal(tx.language(), "value", &[("port", &port.name)]);
                automation_report::out(tx, run_step_id, port.id, Produced::Value(&value))?;
            }
            AutomationPortKind::File => {
                // A row naming the empty bytes, which are never put in the blob store: nothing reads the
                // file before the walk is thrown away, but the hash still has to be one the schema holds.
                let empty = blake3::hash(b"").to_hex();
                let file = crate::ops::attachment::add_blob(
                    tx,
                    AttachmentTarget::AutomationRunStep,
                    run_step_id,
                    &empty,
                    &port.name,
                    None,
                    0,
                    ActorKind::Ai,
                )?;
                automation_report::out(tx, run_step_id, port.id, Produced::File(file.id))?;
            }
            AutomationPortKind::TaskTake => {
                let task = match filed.take() {
                    Some(task) => task,
                    None => made_up(tx, run.project_id, run.role_id)?,
                };
                // The task names the role the run sits in, so its round goes on where that role starts.
                automation_report::take(tx, run_step_id, task)?;
            }
            AutomationPortKind::TaskMake => {
                let task = made_up(tx, run.project_id, run.role_id)?;
                automation_report::out(tx, run_step_id, port.id, Produced::Task(task))?;
            }
        }
    }
    let lets_go = match def.builtin.as_deref() {
        Some(key) if key == automation_builtin_close::CLOSE_TASK.key => Some(TaskStatus::Done),
        Some(key) if key == automation_builtin_hand_back::HAND_BACK_TASK.key => Some(TaskStatus::Todo),
        Some(key) if key == automation_builtin_defer::DEFER_TASK.key => Some(TaskStatus::Todo),
        _ => None,
    };
    let Some(into) = lets_go else { return Ok(()) };
    let step = read::automation_run_step(tx.conn(), run_step_id)?;
    let stretch = match step.and_then(|s| s.run_task_id) {
        Some(id) => read::automation_run_task(tx.conn(), id)?,
        None => None,
    };
    if let Some(task_id) = stretch.and_then(|s| s.task_id) {
        if read::task_status(tx.conn(), task_id)? == Some(TaskStatus::InProgress) {
            task::set_status(tx, task_id, into, ActorKind::Ai)?;
        }
    }
    Ok(())
}

/// **A task made up for a step to hand on**, named as a test run's, filed in the automation's project
/// under `role_id` — a task names a role only in the project of the automation that defines it.
///
/// Its creation is finished without asking for the axes the project requires: it stands in for a task
/// whose classification is nobody's to choose here, and it is thrown away with the walk.
fn made_up(tx: &WriteTx<'_>, project_id: i64, role_id: Option<i64>) -> Result<i64> {
    let filed = task::add(
        tx,
        NewTask {
            title: crate::run_wording::rehearsal(tx.language(), "task", &[]),
            project_id: Some(project_id),
            due: None,
            start: None,
            priority: None,
            notes: String::new(),
            created_by_kind: Some(ActorKind::Ai),
            at_binding_id: None,
            role_id,
            made_in_run_step_id: None,
            made_in: None,
        },
    )?;
    let finished = crate::model::Task { draft: false, ..filed.clone() };
    crate::ops::emit_update(tx, record::task(&filed), record::task(&finished))?;
    Ok(finished.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Automation, AutomationPictureOwner, DONE_EXIT};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewRole, NewScript};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_builtin_close::COMMIT;
    use crate::ops::automation_builtin_make::{MADE_AND_TAKEN, TAKE_IT, WHAT_THEN};
    use crate::ops::automation_builtin_take::TAKEN;
    use crate::ops::automation_run::nothing_asked;
    use crate::ops::test_support::{
        mk_closed_after, mk_out, mk_placed, mk_project, mk_task_in, only_step, start_at, with_tx,
    };

    fn walk(tx: &WriteTx<'_>, automation: &Automation, handed: &HandedAtLaunch) -> Rehearsal {
        let claude = ["claude".to_string()];
        let by = Launcher {
            default_agent: None,
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(false),
            by: Some(ActorKind::Human),
        };
        rehearse(tx, automation.id, &by, handed).expect("a test run")
    }

    fn walked(rehearsal: &Rehearsal) -> Vec<(&str, &str)> {
        rehearsal.steps.iter().map(|s| (s.name.as_str(), s.exit.as_str())).collect()
    }

    /// A take that goes on to an agent's step named `name`, then closes the task and goes round to the
    /// take again.
    fn mk_round(tx: &WriteTx<'_>, automation: &Automation, name: &str) -> i64 {
        let on = AutomationPictureOwner::Automation;
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place take");
        let (_, work) = mk_placed(tx, automation, name, "work on it", "claude");
        let close = automation::placement_add(tx, automation.id, action(tx, "close_task").expect("close").id)
            .expect("place close");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
        automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(close.id), None).expect("work → close");
        automation::edge_add(tx, on, close.id, None, EdgeTarget::Go(take.id), None).expect("close → take");
        take.id
    }

    /// **A picture that goes round is walked once round and then out** — the take leaves by the way out
    /// that took a task the first time, and the run ends before it the second, the work is handed
    /// placeholders and its prompt is composed, and no real task is taken or closed on the way.
    #[test]
    fn a_picture_that_goes_round_is_walked_once_round_and_out() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let real = mk_task_in(tx, "直すもの", Some(project));
            crate::ops::task::set_assignee(tx, real, Some(ActorKind::Ai), ActorKind::Ai).expect("give it to the AI");
            let automation =
                automation::add(tx, project, NewAutomation { name: "round".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                    .expect("automation");
            let on = AutomationPictureOwner::Automation;
            let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
                .expect("place take");
            let (work_action, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
            mk_out(tx, &work_action, None, COMMIT, AutomationPortKind::Value, true);
            let close = automation::placement_add(tx, automation.id, action(tx, "close_task").expect("close").id)
                .expect("place close");
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
            automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(close.id), None).expect("work → close");
            automation::edge_add(tx, on, close.id, None, EdgeTarget::Go(take.id), None).expect("close → take");
            automation::wire_add(tx, on, work.id, None, COMMIT, close.id, COMMIT).expect("wire the commit");
            start_at(tx, automation.id, take.id);

            let rehearsal = walk(tx, &automation, &HandedAtLaunch::default());
            assert_eq!(
                walked(&rehearsal),
                vec![
                    ("タスクに着手する", TAKEN),
                    ("work", DONE_EXIT),
                    ("タスクを閉じる", DONE_EXIT),
                ],
                "{rehearsal:?}",
            );
            assert_eq!(rehearsal.status, AutomationRunStatus::Completed);
            assert_eq!(rehearsal.cut, None);
            let work = &rehearsal.steps[1];
            assert_eq!(work.agent.as_deref(), Some("claude"));
            assert!(work.prompt.as_deref().is_some_and(|p| p.contains("work on it")), "{work:?}");
            assert!(rehearsal.steps[0].prompt.is_none(), "a built-in is started on no prompt");
            assert_eq!(read::task_status(tx.conn(), real).expect("read"), Some(TaskStatus::Todo), "the real task is left alone");
        });
    }

    /// **A picture that hands its task back to a person goes round as one that closes it** — the stand-in
    /// puts the task back in `todo`, so the take after it is not refused for leaving it open.
    #[test]
    fn a_picture_that_hands_the_task_back_goes_round_and_out() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "back".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                    .expect("automation");
            let on = AutomationPictureOwner::Automation;
            let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
                .expect("place take");
            let (_, work) = mk_placed(tx, &automation, "look", "look at it", "claude");
            let back = automation::placement_add(tx, automation.id, action(tx, "hand_back_task").expect("back").id)
                .expect("place hand back");
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
            automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(back.id), None).expect("work → back");
            automation::edge_add(tx, on, back.id, None, EdgeTarget::Go(take.id), None).expect("back → take");
            start_at(tx, automation.id, take.id);

            let rehearsal = walk(tx, &automation, &HandedAtLaunch::default());
            assert_eq!(
                walked(&rehearsal),
                vec![
                    ("タスクに着手する", TAKEN),
                    ("look", DONE_EXIT),
                    ("タスクを人に返す", DONE_EXIT),
                ],
                "{rehearsal:?}",
            );
            assert_eq!(rehearsal.status, AutomationRunStatus::Completed);
        });
    }

    /// **A built-in that files a task leaves by the way out its answers choose** rather than the first it
    /// declares — here set to take the task it files, after the round before it closed its own. Each
    /// closed round goes round to the take where the role starts, which finds nothing the second time.
    #[test]
    fn a_built_in_that_files_a_task_leaves_by_the_way_out_its_answers_choose() {
        with_tx(|tx| {
            use crate::ops::automation_builtin_make::TITLE;
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "file".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                    .expect("automation");
            let on = AutomationPictureOwner::Automation;
            let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
                .expect("place take");
            let (write_action, write) = mk_placed(tx, &automation, "write", "write the next one", "claude");
            mk_out(tx, &write_action, None, "title", AutomationPortKind::Value, true);
            let close = automation::placement_add(tx, automation.id, action(tx, "close_task").expect("close").id)
                .expect("place close");
            let make = automation::placement_add(tx, automation.id, action(tx, "make_task").expect("make").id)
                .expect("place make");
            automation::cfg_set(tx, make.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
                .expect("take it");
            let (_, work) = mk_placed(tx, &automation, "work", "write it", "claude");
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(write.id), None).expect("take → write");
            automation::edge_add(tx, on, write.id, None, EdgeTarget::Go(close.id), None).expect("write → close");
            automation::edge_add(tx, on, close.id, None, EdgeTarget::Go(make.id), None).expect("close → make");
            automation::edge_add(tx, on, make.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(work.id), None)
                .expect("onward");
            automation::wire_add(tx, on, write.id, None, "title", make.id, TITLE).expect("wire the title");
            mk_closed_after(tx, &automation, work.id, None);
            start_at(tx, automation.id, take.id);

            let rehearsal = walk(tx, &automation, &HandedAtLaunch::default());
            let exits: Vec<&str> = rehearsal.steps.iter().map(|s| s.exit.as_str()).collect();
            assert_eq!(
                exits,
                vec![TAKEN, DONE_EXIT, DONE_EXIT, MADE_AND_TAKEN, DONE_EXIT, DONE_EXIT],
                "{rehearsal:?}",
            );
            assert_eq!(rehearsal.status, AutomationRunStatus::Completed);
            let placeholder = crate::run_wording::rehearsal(tx.language(), "task", &[]);
            for id in read::task_ids_in_project(tx.conn(), project).expect("tasks") {
                let filed = read::task(tx.conn(), id).expect("read").expect("the task");
                assert_eq!(filed.title, placeholder, "only the test run's placeholders are filed: {filed:?}");
            }
        });
    }

    /// **A way out that hands on a file is walked through** — the placeholder file is put down as a row
    /// the schema holds, and the walk goes on to the end. What was handed is filed as a task before the
    /// walk, as a launch files it, and the take takes a made-up one in its place. The closed round goes
    /// round to the take, which finds nothing the second time.
    #[test]
    fn a_way_out_that_hands_on_a_file_is_walked_through() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "file".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                    .expect("automation");
            let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
                .expect("place take");
            let (plan_action, plan) = mk_placed(tx, &automation, "plan", "write the plan", "claude");
            mk_out(tx, &plan_action, None, "plan", AutomationPortKind::File, true);
            let on = AutomationPictureOwner::Automation;
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(plan.id), None).expect("take → plan");
            mk_closed_after(tx, &automation, plan.id, None);
            start_at(tx, automation.id, take.id);

            let handed = HandedAtLaunch { title: Some("梅雨どきの家事の記事".into()), ..Default::default() };
            let rehearsal = walk(tx, &automation, &handed);
            let exits: Vec<&str> = rehearsal.steps.iter().map(|s| s.exit.as_str()).collect();
            assert_eq!(exits, vec![TAKEN, DONE_EXIT, DONE_EXIT], "{rehearsal:?}");
            assert_eq!(rehearsal.steps[1].name, "plan", "{rehearsal:?}");
            assert_eq!(rehearsal.status, AutomationRunStatus::Completed, "{rehearsal:?}");
            assert_eq!(rehearsal.cut, None);
        });
    }

    /// **Each role is walked in turn, in the order the roles are defined in** — a run in the seat of
    /// each, from the take where its round starts to its end, the one after the other.
    #[test]
    fn each_role_is_walked_in_turn_from_where_it_starts() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "roles".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                    .expect("automation");
            let first = mk_round(tx, &automation, "draft");
            let second = mk_round(tx, &automation, "finish");
            let drafting = start_at(tx, automation.id, first);
            let finishing = NewRole { name: "仕上げ".into(), entry_placement_id: Some(second), ..NewRole::default() };
            let finishing = automation::role_add(tx, automation.id, finishing).expect("another role").id;

            let rehearsal = walk(tx, &automation, &HandedAtLaunch::default());
            assert_eq!(
                walked(&rehearsal),
                vec![
                    ("タスクに着手する", TAKEN),
                    ("draft", DONE_EXIT),
                    ("タスクを閉じる", DONE_EXIT),
                    ("タスクに着手する", TAKEN),
                    ("finish", DONE_EXIT),
                    ("タスクを閉じる", DONE_EXIT),
                ],
                "{rehearsal:?}",
            );
            let roles: Vec<Option<i64>> = rehearsal.steps.iter().map(|s| s.role_id).collect();
            assert_eq!(roles, [[Some(drafting); 3], [Some(finishing); 3]].concat(), "{rehearsal:?}");
            assert_eq!(rehearsal.status, AutomationRunStatus::Completed);
            assert_eq!(rehearsal.cut, None);
        });
    }

    /// **A script whose program is not there is refused before the walk** (`AMB-D-1016`) — the test run
    /// goes through the launch check, so it is turned down where a launch would be.
    #[test]
    fn a_script_whose_program_is_not_there_is_refused_before_the_walk() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "script".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                    .expect("automation");
            let on = AutomationPictureOwner::Automation;
            let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
                .expect("place take");
            let (work_action, work) = mk_placed(tx, &automation, "work", "", "claude");
            let program = std::env::temp_dir().join("amenbo-no-such-folder").join("check.sh");
            let script =
                NewScript { program: program.to_string_lossy().into_owned(), args: Vec::new(), timeout_minutes: None };
            let step = only_step(tx, &work_action);
            let script = Some(Some(script));
            automation::step_update(tx, step.id, None, None, None, None, None, None, None, None, None, script, None)
                .expect("make it a script");
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
            mk_closed_after(tx, &automation, work.id, None);
            start_at(tx, automation.id, take.id);

            let claude = ["claude".to_string()];
            let by = Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };
            let Err(Error::NotReady(msg)) = rehearse(tx, automation.id, &by, &HandedAtLaunch::default()) else {
                panic!("a test run is refused where a launch would be")
            };
            assert_eq!(
                msg.parts().iter().map(|p| p.code()).collect::<Vec<_>>(),
                vec![Some(crate::error::ErrorCode::NotReadyAutomationScriptMissing)],
            );
        });
    }
}
