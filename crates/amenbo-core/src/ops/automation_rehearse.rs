//! **A test run** — an automation walked from its entry to its end with nothing carried out, so a
//! person can see how it would go before anything real is set moving.
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
//! purpose.
//!
//! **Nothing it writes is kept.** The whole walk is one write transaction that is never committed
//! ([`crate::store::Store::automation_rehearse`]): the run, its steps, the placeholder tasks and the
//! lines a run leaves on its task are all gone the moment it returns, and nobody watching the store sees
//! any of them. What survives is the account handed back ([`Rehearsal`]).
//!
//! **Which way out a step takes is chosen here**, since nobody is doing the work that would decide it.
//! The first time a step is reached it leaves by the first way out it declares; each time it is reached
//! again, by the next one. That walks a picture that goes round — a step that takes tasks until there
//! are none — once round and then out. The error way out is never chosen, and neither is one a
//! built-in's answers say it never leaves by. A step reached again with no way out left untried ends the
//! walk ([`Cut::Looped`]).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::{
    ActorKind, AttachmentTarget, AutomationPortKind, AutomationRunDef, AutomationRunStatus,
    AutomationStoppedReason, RunDefExit, TaskStatus, ERROR_EXIT,
};
use crate::ops::automation_builtin_close;
use crate::ops::automation_builtin_hand_back;
use crate::ops::automation_report::{self, Next, Produced};
use crate::ops::automation_run::{self, HandedAtLaunch, HandedTask, Launcher, ReadsFrom, Waiting};
use crate::ops::automation_step::{self, Opened};
use crate::ops::task::{self, NewTask};
use crate::store_engine::{read, WriteTx};

/// **How many steps a test run opens at most.** A picture whose lines are all limited ends well before
/// this; one that is not would otherwise walk round for ever.
const MOST: usize = 200;

/// **What a test run found** — every step it opened, in order, and how it ended.
#[derive(Clone, Debug, Serialize)]
pub struct Rehearsal {
    pub steps: Vec<Rehearsed>,
    /// Where the run stood when the walk ended: `completed` where the picture ran out, `failed` where
    /// the run itself would have been stopped (`stopped_reason` saying why), or `running` where the walk
    /// was cut short ([`Rehearsal::cut`]).
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

/// **Walk an automation from its entry to its end with nothing carried out** — see the module.
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
    let run = automation_run::launch_handing_past_the_limit(tx, automation_id, &by, handed, ReadsFrom::Draft)?;
    let mut rehearsal = Rehearsal {
        steps: Vec::new(),
        status: run.status,
        stopped_reason: None,
        missing: Vec::new(),
        no_agent: None,
        cut: None,
    };
    let mut visits: BTreeMap<i64, usize> = BTreeMap::new();
    while let Waiting::Step(def) = automation_run::next_def(tx.conn(), run.id)? {
        if rehearsal.steps.len() >= MOST {
            rehearsal.cut = Some(Cut::TooLong);
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
            // with the last still open has been failed, which is read off the run below.
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
        stand_in(tx, &run, opening.run_step.id, &def, exit)?;
        let report = crate::run_wording::rehearsal(tx.language(), "report", &[("exit", &exit.name)]);
        if let Next::Closed(_) | Next::Halted(_) | Next::Paused(_) =
            automation_report::done(tx, opening.run_step.id, Some(exit.id), &report)?
        {
            break;
        }
    }
    let ended = read::automation_run(tx.conn(), run.id)?.unwrap_or(run);
    rehearsal.status = ended.status;
    rehearsal.stopped_reason = ended.stopped_reason;
    Ok(rehearsal)
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
/// kind — and, for the built-ins that close the task or hand it back to a person, close the one the run
/// holds or put it back in `todo`, so the next task it takes is not taken with this one still open.
fn stand_in(
    tx: &WriteTx<'_>,
    run: &crate::model::AutomationRun,
    run_step_id: i64,
    def: &AutomationRunDef,
    exit: &RunDefExit,
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
                let task = made_up(tx, run, def)?;
                automation_report::take(tx, run_step_id, task)?;
            }
            AutomationPortKind::TaskMake => {
                let task = made_up(tx, run, def)?;
                automation_report::out(tx, run_step_id, port.id, Produced::Task(task))?;
            }
        }
    }
    let lets_go = match def.builtin.as_deref() {
        Some(key) if key == automation_builtin_close::CLOSE_TASK.key => Some(TaskStatus::Done),
        Some(key) if key == automation_builtin_hand_back::HAND_BACK_TASK.key => Some(TaskStatus::Todo),
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

/// **A task made up for a step to hand on.** The built-in that files a task files it from what was
/// handed over at launch, so a placeholder filed in its place carries that title and those notes, and
/// the prompts after it read what the person typed; every other one is named as a test run's.
///
/// It is filed in no project, so no axis the project requires holds its creation.
fn made_up(tx: &WriteTx<'_>, run: &crate::model::AutomationRun, def: &AutomationRunDef) -> Result<i64> {
    let handed = match def.builtin.as_deref() {
        Some(crate::ops::automation_builtin_make::KEY) => HandedTask::of(run)?,
        _ => None,
    };
    let title = handed
        .as_ref()
        .map(|h| h.title.clone())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| crate::run_wording::rehearsal(tx.language(), "task", &[]));
    let notes = handed.and_then(|h| h.notes).unwrap_or_default();
    let filed = task::add(
        tx,
        NewTask {
            title,
            project_id: None,
            due_on: None,
            start_on: None,
            priority: None,
            notes,
            created_by_kind: Some(ActorKind::Ai),
            at_binding_id: None,
            made_in: None,
        },
    )?;
    Ok(task::finish_creating(tx, filed.id, ActorKind::Ai)?.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Automation, AutomationPictureOwner, DONE_EXIT};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewScript};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_builtin_close::COMMIT;
    use crate::ops::automation_builtin_make::{MADE_AND_TAKEN, TAKE_IT, WHAT_THEN};
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::automation_run::nothing_asked;
    use crate::ops::test_support::{mk_closed_after, mk_out, mk_placed, mk_project, mk_task_in, only_step, with_tx};

    fn walk(tx: &WriteTx<'_>, automation: &Automation, handed: &HandedAtLaunch) -> Rehearsal {
        let claude = ["claude".to_string()];
        let by = Launcher {
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

    /// **A picture that goes round is walked once round and then out** — the take leaves by the way out
    /// that took a task the first time and by the one that found none the second, the work is handed
    /// placeholders and its prompt is composed, and no real task is taken or closed on the way.
    #[test]
    fn a_picture_that_goes_round_is_walked_once_round_and_out() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let real = mk_task_in(tx, "直すもの", Some(project));
            crate::ops::task::set_assignee(tx, real, Some(ActorKind::Ai), ActorKind::Ai).expect("give it to the AI");
            let automation =
                automation::add(tx, project, NewAutomation { name: "round".into(), ..Default::default() })
                    .expect("automation");
            let on = AutomationPictureOwner::Automation;
            let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
                .expect("place take");
            let (work_action, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
            mk_out(tx, &work_action, None, COMMIT, AutomationPortKind::Value, true);
            let close = automation::placement_add(tx, automation.id, action(tx, "close_task").expect("close").id)
                .expect("place close");
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
            automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
            automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(close.id), None).expect("work → close");
            automation::edge_add(tx, on, close.id, None, EdgeTarget::Go(take.id), None).expect("close → take");
            automation::wire_add(tx, on, work.id, None, COMMIT, close.id, COMMIT).expect("wire the commit");
            let automation = automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");

            let rehearsal = walk(tx, &automation, &HandedAtLaunch::default());
            assert_eq!(
                walked(&rehearsal),
                vec![
                    ("タスクに着手する", TAKEN),
                    ("work", DONE_EXIT),
                    ("タスクを閉じる", DONE_EXIT),
                    ("タスクに着手する", NONE_TO_TAKE),
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
                automation::add(tx, project, NewAutomation { name: "back".into(), ..Default::default() })
                    .expect("automation");
            let on = AutomationPictureOwner::Automation;
            let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
                .expect("place take");
            let (_, work) = mk_placed(tx, &automation, "look", "look at it", "claude");
            let back = automation::placement_add(tx, automation.id, action(tx, "hand_back_task").expect("back").id)
                .expect("place hand back");
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
            automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
            automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(back.id), None).expect("work → back");
            automation::edge_add(tx, on, back.id, None, EdgeTarget::Go(take.id), None).expect("back → take");
            let automation = automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");

            let rehearsal = walk(tx, &automation, &HandedAtLaunch::default());
            assert_eq!(
                walked(&rehearsal),
                vec![
                    ("タスクに着手する", TAKEN),
                    ("look", DONE_EXIT),
                    ("タスクを人に返す", DONE_EXIT),
                    ("タスクに着手する", NONE_TO_TAKE),
                ],
                "{rehearsal:?}",
            );
            assert_eq!(rehearsal.status, AutomationRunStatus::Completed);
        });
    }

    /// **An entry that files a task files a made-up one from what was handed**, and leaves by the way out
    /// its answers choose rather than the first it declares.
    #[test]
    fn an_entry_that_files_a_task_leaves_by_the_way_out_its_answers_choose() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "entry".into(), ..Default::default() })
                    .expect("automation");
            let make = automation::placement_add(tx, automation.id, action(tx, "make_task").expect("make").id)
                .expect("place it");
            automation::cfg_set(tx, make.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
                .expect("take it");
            let (_, work) = mk_placed(tx, &automation, "work", "write it", "claude");
            let on = AutomationPictureOwner::Automation;
            automation::edge_add(tx, on, make.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(work.id), None)
                .expect("onward");
            mk_closed_after(tx, &automation, work.id, None);
            let automation = automation::set_entry(tx, automation.id, Some(make.id)).expect("entry");

            let handed = HandedAtLaunch { title: Some("梅雨どきの家事の記事".into()), ..Default::default() };
            let rehearsal = walk(tx, &automation, &handed);
            let exits: Vec<&str> = rehearsal.steps.iter().map(|s| s.exit.as_str()).collect();
            assert_eq!(exits, vec![MADE_AND_TAKEN, DONE_EXIT, DONE_EXIT], "{rehearsal:?}");
            assert_eq!(rehearsal.status, AutomationRunStatus::Completed);
            let filed = read::task_ids_in_project(tx.conn(), project).expect("tasks");
            assert!(filed.is_empty(), "nothing is filed in the project: {filed:?}");
        });
    }

    /// **A way out that hands on a file is walked through** — the placeholder file is put down as a row
    /// the schema holds, and the walk goes on to the end.
    #[test]
    fn a_way_out_that_hands_on_a_file_is_walked_through() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "file".into(), ..Default::default() })
                    .expect("automation");
            let make = automation::placement_add(tx, automation.id, action(tx, "make_task").expect("make").id)
                .expect("place it");
            automation::cfg_set(tx, make.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
                .expect("take it");
            let (plan_action, plan) = mk_placed(tx, &automation, "plan", "write the plan", "claude");
            mk_out(tx, &plan_action, None, "plan", AutomationPortKind::File, true);
            let on = AutomationPictureOwner::Automation;
            automation::edge_add(tx, on, make.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(plan.id), None)
                .expect("onward");
            mk_closed_after(tx, &automation, plan.id, None);
            let automation = automation::set_entry(tx, automation.id, Some(make.id)).expect("entry");

            let handed = HandedAtLaunch { title: Some("梅雨どきの家事の記事".into()), ..Default::default() };
            let rehearsal = walk(tx, &automation, &handed);
            let exits: Vec<&str> = rehearsal.steps.iter().map(|s| s.exit.as_str()).collect();
            assert_eq!(exits, vec![MADE_AND_TAKEN, DONE_EXIT, DONE_EXIT], "{rehearsal:?}");
            assert_eq!(rehearsal.steps[1].name, "plan", "{rehearsal:?}");
            assert_eq!(rehearsal.status, AutomationRunStatus::Completed, "{rehearsal:?}");
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
                automation::add(tx, project, NewAutomation { name: "script".into(), ..Default::default() })
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
            automation::step_update(tx, step.id, None, None, None, None, None, None, None, None, None, script)
                .expect("make it a script");
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
            automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
            mk_closed_after(tx, &automation, work.id, None);
            let automation = automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");

            let claude = ["claude".to_string()];
            let by = Launcher { startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };
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
