//! **The built-in that takes a task** — find the task a run works next and reserve it (`AMB-D-964`).
//!
//! What a prompt used to be asked to do with `task list` and `step-take`, done here in the one
//! transaction the step is opened in: list the tasks the filter matches, in the order the run's role
//! takes them in, and reserve them from the top until one goes through. A task somebody else reserved
//! first is not a failure — the next one is tried.
//!
//! **Not started and ready are always asked for.** Only a `todo` task whose premises all hold can be
//! reserved, so the filter's own `status:` and `ready:` are dropped and those two put in their place:
//! a setting that asked for anything else would be asking for a task the reservation refuses.
//!
//! **Only a task of the role the run is a seat of is taken** ([`roles_started`]): a run is bound to one
//! role at launch and keeps it until it ends, and a task is worked under the role it names, so one naming
//! none, or another role, is left — for a person, or for a run of that role. An automation defining no
//! role that says where its round starts takes any task the filter finds.
//!
//! **The role says in what order** ([`crate::model::TakeOrder`]): by priority, the oldest first within
//! one, until somebody chooses its due date, when it was made, or the order a person put the tasks in.
//! Only a run that takes tasks of no one role lists them in the order the filter's answer asks for.
//!
//! **Left unanswered, the filter is the tasks given to the AI** (`assignee:me-ai`). The built-in that
//! hands a task back to a person gives it to the human, and so does a run a person canceled; a filter
//! that did not ask for the AI's would take it straight back.
//!
//! It leaves by [`TAKEN`] with the task it reserved. There is no way out for nothing to take
//! (`AMB-D-1131`).
//!
//! **Where there is none, Amenbo waits for one** (`AMB-D-969`, `AMB-D-1130`) — always: a role whose
//! round starts here waits for a task of its own to turn up. A run or a session closing a task can make
//! another one ready, and so can a new one being filed, so there is no point at which none will ever
//! turn up. The run that found none ends before this step is opened, its seat left free with no run
//! under way, until a task turns up and a run is started on it in that seat
//! ([`crate::ops::automation_run::end_with_no_task`], [`crate::ops::automation_run::top_up`]). What is
//! keeping it waiting is counted apart from that look (`AMB-D-999`): the tasks the filter matches that
//! are not started and not ready, by what stops them ([`held_back`]).
//!
//! **A step opened with nothing it could reserve leaves by the error way out** — every task it listed
//! was reserved by somebody else, or stopped being ready, in the moment between the look that found one
//! and the reservation. The look and the opening are one transaction, so this is a safety net.

use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::model::{AutomationCfgKind, AutomationPortKind, AutomationRun, RunDefCfg, TakeOrder};
use crate::ops::automation_builtin::{
    answer, Builtin, BuiltinExit, BuiltinPort, BuiltinSetting, Carried, Carry, HeldBack, HeldByRecord,
    HeldByValue, NeedsTask, Work,
};
use crate::ops::automation_report;
use crate::ops::automation_run::{roles_started, take_order};
use crate::run_wording::builtin as say;
use crate::ops::automation_step::{taskfilter_expr, taskfilter_sort, TASKFILTER_SORT_DEFAULT};
use crate::query::{self, ListParams};
use crate::reach::Reach;
use crate::store_engine::read;

/// The way out it leaves by once it has reserved a task.
pub const TAKEN: &str = "着手した";
/// The setting that says which tasks it takes — and in what order, for a run that takes tasks of no one
/// role ([`take_order`]).
pub const FILTER: &str = "絞り込み";
/// The output the reserved task is handed on through.
pub const TASK: &str = "タスク";

/// What every search adds to the filter, whatever the setting says.
const TAKEABLE: &str = "status:todo ready:yes";
/// What counting the tasks held back adds instead: the same tasks, but the ones that cannot be taken.
const HELD: &str = "status:todo ready:no";
/// What the filter is when nobody answered it.
const UNANSWERED: &str = "assignee:me-ai";
/// How many candidates are read at a time. Most runs reserve the first; the rest are read only while
/// the ones before them were taken by somebody else.
const PAGE: usize = 20;
/// How many held-back tasks are read at a time while counting them. Every one is read, so the page
/// only bounds what one read holds.
const COUNT_PAGE: usize = 200;

pub(crate) const TAKE_TASK: Builtin = Builtin {
    key: "take_task",
    version: 2,
    name: "タスクに着手する",
    does: "絞り込みに合い、着手できる未着手のタスクを並び順どおりに探し、先頭から予約して進行中にする",
    steps: &[
        "設定の絞り込みに合うタスクのうち、未着手で着手できるものを、並び順どおりに探す。絞り込みが空なら、AI が担当のタスクを探す",
        "先頭のタスクから予約して進行中にし、「着手した」から出てそのタスクを渡す。ほかで先に予約されたタスクは飛ばして、次を試す",
        "着手できるタスクが無ければ、出るまで待つ。この実行はここで終わり、タスクが出たら、この役割の空いた席で次の実行が着手する",
    ],
    halts: &["着手できるタスクがあったのに、予約する前にすべてほかで予約された"],
    settings: &[BuiltinSetting { name: FILTER, kind: AutomationCfgKind::TaskFilter, required: false, options: None }],
    ins: &[],
    exits: &[
        BuiltinExit {
            name: TAKEN,
            outs: &[BuiltinPort { name: TASK, kind: AutomationPortKind::TaskTake, required: true }],
        },
    ],
    chooses: None,
    needs_task: NeedsTask::No,
    work: Work::InStore(take),
};

fn take(carry: &Carry<'_, '_>) -> Result<Carried> {
    let answer = carry.setting(FILTER);
    let expr = expression(answer);
    let mut offset = 0;
    loop {
        let page = search(carry.tx.conn(), carry.run, answer, PAGE, offset)?;
        for candidate in &page.tasks {
            match automation_report::take(carry.tx, carry.run_step.id, candidate.id) {
                Ok(task) => {
                    return Ok(Carried {
                        exit: TAKEN,
                        report: say(
                            carry.tx.language(),
                            "took",
                            &[("task", &format!("AMB-T-{}", task.id)), ("title", &task.title)],
                        ),
                    })
                }
                // Reserved by somebody else since the list was read, or no longer ready: the next one.
                Err(e) if matches!(e.code(), "already_reserved" | "not_ready") => continue,
                Err(e) => return Err(e),
            }
        }
        if page.tasks.len() < PAGE {
            break;
        }
        offset += PAGE;
    }
    Err(Error::invalid(say(carry.tx.language(), "noneToTake", &[("filter", &expr)])))
}

/// **Whether there is a task to take now** — the one question asked while it waits. One row is read,
/// however many the filter matches, and none is counted or sorted.
pub(crate) fn turned_up(conn: &Connection, project_id: i64, roles: &[i64], cfg: &[RunDefCfg]) -> Result<bool> {
    query::any_in_roles(conn, Reach::binding(project_id), &expression(answer(cfg, FILTER)), roles)
}

/// **What it looks for while it waits**, as a person reads it beside the automation: the filter it
/// searches with.
pub(crate) fn looks_for(cfg: &[RunDefCfg]) -> String {
    expression(answer(cfg, FILTER))
}

/// **What is keeping it waiting** (`AMB-D-999`): every task the filter matches that is not started and
/// not ready, counted by what stops it. The reasons are the five the list already carries on each row
/// (`view::is_ready`), so a reason counted here is one the reservation would refuse by.
pub(crate) fn held_back(conn: &Connection, project_id: i64, roles: &[i64], cfg: &[RunDefCfg]) -> Result<HeldBack> {
    let expr = with_asked(narrowing(answer(cfg, FILTER)), HELD);
    let mut held = HeldBack::default();
    let (mut blockers, mut decisions): (Counts, Counts) = (Vec::new(), Vec::new());
    let mut offset = 0;
    loop {
        let page = query::list_in_roles(
            conn,
            Reach::binding(project_id),
            ListParams {
                filter_expr: Some(expr.clone()),
                sort: TASKFILTER_SORT_DEFAULT.to_string(),
                limit: Some(COUNT_PAGE),
                offset: Some(offset),
                ..Default::default()
            },
            roles,
        )?;
        for task in &page.tasks {
            held.tasks += 1;
            for w in &task.waiting_on_values {
                match held.values.iter_mut().find(|v| v.dimension_id == w.dimension_id && v.value == w.value) {
                    Some(v) => v.count += 1,
                    None => held.values.push(HeldByValue {
                        dimension_id: w.dimension_id,
                        axis: w.axis.clone(),
                        value: w.value.clone(),
                        count: 1,
                    }),
                }
            }
            task.blocked_by_open.iter().for_each(|id| bump(&mut blockers, *id));
            task.blocked_by_decisions.iter().for_each(|id| bump(&mut decisions, *id));
            if let Some(start) = task.not_started_until {
                held.not_started += 1;
                // A day weighs in at the moment it begins for the reader, an instant at its own; at a tie
                // the day is named, as `waiting_on_start` names it.
                let weight = |s: crate::model::Schedule| (s.begins(), s.at().is_some());
                held.first_start = Some(match held.first_start {
                    Some(first) if weight(first) <= weight(start) => first,
                    _ => start,
                });
            }
            if task.draft {
                held.drafts += 1;
                if held.first_draft.is_none() {
                    held.first_draft = Some(task.id);
                }
            }
            if !task.waiting_on_values.is_empty() || !task.blocked_by_decisions.is_empty() || task.draft {
                held.awaits_person += 1;
            }
        }
        if page.tasks.len() < COUNT_PAGE {
            break;
        }
        offset += COUNT_PAGE;
    }
    held.blockers = titled(blockers, |id| Ok(read::task_title(conn, id)?))?;
    held.decisions = titled(decisions, |id| Ok(read::decision_title(conn, id)?))?;
    // Stable, so records holding as many keep the order they were first met in.
    held.values.sort_by_key(|v| std::cmp::Reverse(v.count));
    Ok(held)
}

/// How many tasks each record holds back, by its id, in the order the records were first met.
type Counts = Vec<(i64, usize)>;

fn bump(counts: &mut Counts, id: i64) {
    match counts.iter_mut().find(|(seen, _)| *seen == id) {
        Some((_, count)) => *count += 1,
        None => counts.push((id, 1)),
    }
}

/// The counted ids, each with its title, from the one holding the most down. A record gone since is
/// named by nothing rather than left out, since the tasks it held are still counted.
fn titled(
    mut counts: Counts,
    title: impl Fn(i64) -> Result<Option<String>>,
) -> Result<Vec<HeldByRecord>> {
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
        .into_iter()
        .map(|(id, count)| Ok(HeldByRecord { id, title: title(id)?.unwrap_or_default(), count }))
        .collect()
}

/// One page of the tasks the filter answered with lists — of those worked under the role the run is a
/// seat of, where it has one ([`roles_started`]).
///
/// **In the order that role takes its tasks in** ([`take_order`]), whatever order the filter's answer
/// asks for. A run that takes tasks of no role, or of more than one, lists them in the order the answer
/// asks for, and by priority where it asks for none.
fn search(
    conn: &Connection,
    run: &AutomationRun,
    answer: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<query::TaskListResult> {
    listed(conn, run.project_id, &roles_started(conn, run.id)?, answer, take_order(conn, run)?, limit, offset)
}

/// **The tasks a seat of `roles` would take at an entry answered `cfg`, as it would take them** — how
/// many there are, and the first `first` of them in the order it takes them in: that of the role
/// (`order`), else the one the filter's answer asks for. What a launch's pane reads as a role's tasks
/// waiting to be taken ([`crate::ops::automation_run::launch_state`]). A task a run has taken is under
/// way rather than not started, so it is not among them.
pub(crate) fn waiting_to_be_taken(
    conn: &Connection,
    project_id: i64,
    roles: &[i64],
    cfg: &[RunDefCfg],
    order: Option<TakeOrder>,
    first: usize,
) -> Result<query::TaskListResult> {
    listed(conn, project_id, roles, answer(cfg, FILTER), order, first, 0)
}

/// One page of the tasks the filter answered with lists, of those worked under one of `roles` —
/// [`search`] and [`waiting_to_be_taken`] read them the same way.
fn listed(
    conn: &Connection,
    project_id: i64,
    roles: &[i64],
    answer: Option<&str>,
    order: Option<TakeOrder>,
    limit: usize,
    offset: usize,
) -> Result<query::TaskListResult> {
    let sort = match order {
        Some(order) => order.sort().to_string(),
        None => answer.map(taskfilter_sort).unwrap_or_else(|| TASKFILTER_SORT_DEFAULT.to_string()),
    };
    query::list_in_roles(
        conn,
        Reach::binding(project_id),
        ListParams {
            filter_expr: Some(expression(answer)),
            sort,
            limit: Some(limit),
            offset: Some(offset),
            ..Default::default()
        },
        roles,
    )
}

/// **The filter it searches with**: the parts the setting chose, less `status:` and `ready:`, and the
/// two it always asks for after them.
///
/// **An answer that is not an object of parts is read as no answer**, never as no narrowing. Such an
/// answer is refused when it is written and at the launch; one that got past both would otherwise have
/// the run take any task in the project — a person's among them.
fn expression(answer: Option<&str>) -> String {
    with_asked(narrowing(answer), TAKEABLE)
}

/// The parts the setting chose, less `status:` and `ready:` — `None` where that leaves nothing.
fn narrowing(answer: Option<&str>) -> Option<String> {
    let parts = answer.and_then(|value| match serde_json::from_str::<serde_json::Value>(value) {
        Ok(serde_json::Value::Object(parts)) => Some(parts),
        _ => None,
    });
    match parts {
        None => Some(UNANSWERED.to_string()),
        Some(mut parts) => {
            parts.remove("status");
            parts.remove("ready");
            taskfilter_expr(&serde_json::Value::Object(parts).to_string())
        }
    }
}

fn with_asked(chosen: Option<String>, asked: &str) -> String {
    match chosen {
        Some(chosen) => format!("{chosen} {asked}"),
        None => asked.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ActorKind, Automation, AutomationPictureOwner, AutomationPlacement, AutomationPortKind,
        AutomationRun, AutomationRunStatus, AutomationStoppedReason, Priority, Schedule, TakeOrder, TaskStatus,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewRole};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{
        check, is_waiting, launch, launch_past_the_task_checks, next_def, nothing_asked, start, take_up_newer,
        Launcher, Started, TakenUp, Unmet, Waiting,
    };
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::ops::automation_stop;
    use crate::ops::task::{self, TaskPatch};
    use crate::ops::test_support::{give_to, mk_out, mk_placed, mk_project, mk_task_in, start_at, way_out, with_tx};
    use crate::store_engine::{read, WriteTx};

    /// An automation whose role starts its round at the built-in. What it takes goes on to an agent's
    /// step, since a run does not end with its task still in progress (`AMB-D-967`). Nothing has to
    /// follow it for nothing to take: there is no such way out, and a run with nothing to take ends
    /// before it.
    fn picture(tx: &WriteTx<'_>, project: i64) -> (Automation, AutomationPlacement) {
        let automation =
            automation::add(tx, project, NewAutomation { name: "take".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                .expect("automation");
        let written = action(tx, "take_task").expect("the built-in's action");
        let spot = automation::placement_add(tx, automation.id, written.id).expect("place it");
        let on = AutomationPictureOwner::Automation;
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        automation::edge_add(tx, on, spot.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("onward");
        crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
        start_at(tx, automation.id, spot.id);
        (as_it_stands(tx, &automation), spot)
    }

    /// The automation as it stands now, read back.
    fn as_it_stands(tx: &WriteTx<'_>, automation: &Automation) -> Automation {
        read::automation(tx.conn(), automation.id).expect("read").expect("automation")
    }

    /// The role an automation here was born with, which its round starts as.
    fn born(tx: &WriteTx<'_>, automation: &Automation) -> i64 {
        read::automation_roles_of(tx.conn(), automation.id).expect("roles")[0].id
    }

    /// A task for the AI of the role `automation`'s round starts as ([`born`]) — one a run of it takes.
    fn ours(tx: &WriteTx<'_>, title: &str, automation: &Automation, priority: Option<Priority>) -> i64 {
        let id = for_ai(tx, title, automation.project_id, priority);
        give_to(tx, id, born(tx, automation));
        id
    }

    /// The placement the step named `name` was copied from onto run `run`.
    fn placed(tx: &WriteTx<'_>, run: i64, name: &str) -> i64 {
        read::automation_run_defs_of(tx.conn(), run)
            .expect("defs")
            .into_iter()
            .find(|d| d.name == name)
            .and_then(|d| d.placement_id)
            .expect("the step's placement")
    }

    /// Launch it and open the step it starts at, which the built-in carries out on the spot.
    fn carried(tx: &WriteTx<'_>, automation: &Automation) -> (AutomationRun, i64, Next) {
        let run = launched(tx, automation);
        match open_entry(tx, &run) {
            Opened::Carried { run_step_id, next } => (run, run_step_id, next),
            other => panic!("a built-in is carried out, not {other:?}"),
        }
    }

    fn for_ai(tx: &WriteTx<'_>, title: &str, project: i64, priority: Option<Priority>) -> i64 {
        let id = mk_task_in(tx, title, Some(project));
        task::set_assignee(tx, id, Some(ActorKind::Ai), ActorKind::Ai).expect("give it to the AI");
        if priority.is_some() {
            task::update(tx, id, TaskPatch { priority, ..Default::default() }).expect("priority");
        }
        id
    }

    fn status(tx: &WriteTx<'_>, id: i64) -> TaskStatus {
        read::task(tx.conn(), id).expect("read").expect("task").status
    }

    /// **It reserves the first task it can**: the AI's, not started, ready, in this project, highest
    /// priority first — and hands it on through [`TAKEN`], as the task the run now works.
    #[test]
    fn it_reserves_the_first_takeable_task_and_hands_it_on() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let elsewhere = mk_project(tx, "other");
            let (automation, _) = picture(tx, project);

            let low = ours(tx, "low", &automation, Some(Priority::Low));
            let high = ours(tx, "high", &automation, Some(Priority::High));
            let people = mk_task_in(tx, "a person's", Some(project));
            give_to(tx, people, born(tx, &automation));
            task::update(tx, people, TaskPatch { priority: Some(Priority::High), ..Default::default() })
                .expect("priority");
            let running = ours(tx, "running", &automation, Some(Priority::High));
            task::set_status(tx, running, TaskStatus::InProgress, crate::model::ActorKind::Ai).expect("reserve");
            let waiting = ours(tx, "waiting", &automation, Some(Priority::High));
            crate::ops::dependency::add(tx, waiting, low, None).expect("depend");
            for_ai(tx, "not ours", elsewhere, Some(Priority::High));

            let (run, run_step_id, _) = carried(tx, &automation);
            assert_eq!(status(tx, high), TaskStatus::InProgress, "the highest the filter lets through");
            assert_eq!(status(tx, low), TaskStatus::Todo);
            assert_eq!(status(tx, people), TaskStatus::Todo, "a person's task is left to them");
            assert_eq!(status(tx, waiting), TaskStatus::Todo, "one that is not ready is not taken");

            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, TAKEN));
            let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
            assert_eq!(stretch.task_id, Some(high), "the run is on the task it took");
            let handed: Vec<_> = read::automation_run_values_of(tx.conn(), run_step_id)
                .expect("values")
                .into_iter()
                .filter_map(|v| v.task_id)
                .collect();
            assert_eq!(handed, vec![high]);
        });
    }

    /// **What the setting says about status and ready is not what it searches with** — a filter that
    /// asked for tasks under way would be asking for ones the reservation refuses.
    #[test]
    fn the_setting_cannot_ask_for_a_task_that_cannot_be_taken() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, spot) = picture(tx, project);
            let running = ours(tx, "running", &automation, None);
            task::set_status(tx, running, TaskStatus::InProgress, crate::model::ActorKind::Ai).expect("reserve");
            let people = mk_task_in(tx, "a person's", Some(project));
            give_to(tx, people, born(tx, &automation));
            automation::cfg_set(
                tx,
                spot.id,
                FILTER,
                Some(r#"{"status":["in_progress"],"ready":["no"],"assignee":["none"],"sort":"-priority"}"#),
            )
            .expect("answer");

            let (_, run_step_id, _) = carried(tx, &automation);
            assert_eq!(status(tx, people), TaskStatus::InProgress, "the rest of the setting is kept");
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, TAKEN));
        });
    }

    /// **With nothing to take, the run ends before the step** (`AMB-D-1130`) — completed, with no task
    /// touched, no execution written and no stretch kept. There is no way out for nothing to take.
    #[test]
    fn with_nothing_to_take_the_run_ends_before_the_step() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = picture(tx, project);
            let people = mk_task_in(tx, "a person's", Some(project));
            give_to(tx, people, born(tx, &automation));

            let run = launched(tx, &automation);
            let Opened::LeftToWait { run } = open_entry(tx, &run) else { panic!("the run ends before the step") };
            assert_eq!(run.status, AutomationRunStatus::Completed);
            assert_eq!(status(tx, people), TaskStatus::Todo);
            assert!(read::automation_run_steps_of(tx.conn(), run.id).expect("steps").is_empty(), "no step was opened");
            assert!(read::automation_run_task_last(tx.conn(), run.id).expect("read").is_none(), "no stretch");
        });
    }

    fn launched(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let claude = ["claude".to_string()];
        let by = Launcher {
            default_agent: None,
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        launch(tx, automation.id, &by).expect("launch")
    }

    fn open_entry(tx: &WriteTx<'_>, run: &AutomationRun) -> Opened {
        let Waiting::Step(entry) = next_def(tx.conn(), run.id).expect("next") else {
            panic!("the run stands before its entry");
        };
        assert_eq!(entry.builtin.as_deref(), Some("take_task"));
        open(tx, run.id, entry.id, Some(&[])).expect("open")
    }

    /// Who starts a run once a task turns up: nobody, with no agent probe and no window — as the app's
    /// watch starts one ([`crate::ops::automation_run::top_up`]).
    fn launcher() -> Launcher<'static> {
        Launcher { default_agent: None, startable: None, models: nothing_asked(), workspace_open: None, by: None }
    }

    /// **The launch standing for an automation**, with no run in it or with some.
    fn standing(tx: &WriteTx<'_>, automation: &Automation) -> i64 {
        read::automation_launch_standing(tx.conn(), automation.id).expect("read").expect("a launch stands").id
    }

    /// **With nothing to take, the run ends there and its seat is free again**
    /// (`AMB-D-1126`) — no step, no stretch, no run left standing, and nothing marked on the automation.
    /// Once a task turns up, the launch fills the seat with a run, and that run takes it.
    #[test]
    fn with_nothing_to_take_the_run_ends_and_a_run_is_started_once_a_task_turns_up() {
        use crate::ops::automation_run::{launches_to_top_up, top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            let startable = ["claude".to_string()];
            let unmet = check(tx.conn(), automation.id, Some(&startable), nothing_asked(), None).expect("check");
            assert!(unmet.is_empty(), "nothing has to follow it for nothing to take: {unmet:?}");

            let run = launched(tx, &automation);
            let launch = run.launch_id.expect("a launch");
            assert!(is_waiting(tx.conn(), run.id).expect("waiting"), "it stands before a take that would wait");
            match open_entry(tx, &run) {
                Opened::LeftToWait { run: ended } => assert_eq!(ended.status, AutomationRunStatus::Completed),
                other => panic!("with nothing to take the run ends, not {other:?}"),
            }
            assert!(read::automation_run_steps_of(tx.conn(), run.id).expect("steps").is_empty(), "nothing written");
            assert!(read::automation_run_task_last(tx.conn(), run.id).expect("read").is_none(), "no stretch");
            assert!(read::automation_run_ids_under_way(tx.conn(), automation.id).expect("runs").is_empty());
            assert!(launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty(), "nothing yet");
            assert!(top_up(tx, launch, &launcher()).expect("top up").is_none(), "nothing started");

            let turned_up = ours(tx, "new", &automation, None);
            assert_eq!(launches_to_top_up(tx.conn(), &[launch]).expect("ask"), vec![launch]);
            let next = top_up(tx, launch, &launcher()).expect("top up").expect("a run");
            assert_eq!(next.status, AutomationRunStatus::Running);
            assert_eq!(next.launch_id, Some(launch), "it joins the launch standing");
            let run_step_id = match open_entry(tx, &next) {
                Opened::Carried { run_step_id, .. } => run_step_id,
                other => panic!("the task that turned up is taken, not {other:?}"),
            };
            assert_eq!(status(tx, turned_up), TaskStatus::InProgress);
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, TAKEN));
        });
    }

    /// **A start with nothing to take makes no run**, and leaves the launch open with no run in it. A
    /// start again with still nothing to take makes none either, in the same launch. Once a task turns
    /// up, the launch fills its seat, and a start with a task to take makes a run as always.
    #[test]
    fn a_start_with_nothing_to_take_makes_no_run_and_opens_the_launch() {
        use crate::ops::automation_run::top_up;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            let Started::Waits(_) = start(tx, automation.id, &launcher()).expect("start") else {
                panic!("no run with nothing to take");
            };
            assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty(), "no run made");
            let launch = standing(tx, &automation);
            assert_eq!(launch_of(tx, launch).status, crate::model::AutomationLaunchStatus::Running);
            let Started::Waits(_) = start(tx, automation.id, &launcher()).expect("start") else {
                panic!("still nothing to take");
            };
            assert_eq!(standing(tx, &automation), launch, "the same launch");

            ours(tx, "new", &automation, None);
            let topped = top_up(tx, launch, &launcher()).expect("top up").expect("a run for the task");
            assert_eq!(topped.launch_id, Some(launch));
            ours(tx, "another", &automation, None);
            crate::ops::test_support::room_for(tx, automation.id, 2);
            let Started::Run(run) = start(tx, automation.id, &launcher()).expect("start") else {
                panic!("a task to take makes a run");
            };
            assert_eq!(run.status, AutomationRunStatus::Running);
        });
    }

    /// **A task turning up starts no run past the seats of its role** — nothing is written, and the
    /// launch is asked again later.
    #[test]
    fn a_task_turning_up_starts_no_run_past_the_seats_of_its_role() {
        use crate::ops::automation_run::{launches_to_top_up, top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            assert!(matches!(
                start(tx, automation.id, &launcher()).expect("start"),
                Started::Waits(_)
            ));
            let standing_run = launched(tx, &automation);
            let launch = standing(tx, &automation);
            ours(tx, "new", &automation, None);

            assert!(launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty(), "no room");
            assert!(top_up(tx, launch, &launcher()).expect("top up").is_none());
            assert_eq!(read::automation_run_ids(tx.conn(), automation.id).expect("runs"), vec![standing_run.id]);
        });
    }

    /// **A role with nothing to take holds back no other role** (`AMB-D-1126`): where a run of one role
    /// comes to its start and finds nothing, it ends, and a task filed under another role with a free
    /// seat is started on in the same launch.
    #[test]
    fn a_role_waiting_for_a_task_holds_back_no_other_role() {
        use crate::ops::automation_run::{launches_to_top_up, top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [(builder, _), (reviewer, _)]) = by_role(tx, project);

            let run = launched(tx, &automation);
            let launch = run.launch_id.expect("a launch");
            assert_eq!(run.role_id, Some(builder), "the first role, where none has a task");
            assert!(matches!(open_entry(tx, &run), Opened::LeftToWait { .. }));

            let reviewed = for_ai(tx, "to review", project, None);
            give_to(tx, reviewed, reviewer);
            assert_eq!(launches_to_top_up(tx.conn(), &[launch]).expect("ask"), vec![launch]);
            let next = top_up(tx, launch, &launcher()).expect("top up").expect("a run for the reviewer");
            assert_eq!(next.role_id, Some(reviewer), "the reviewer's seat is filled");
            open_entry(tx, &next);
            assert_eq!(status(tx, reviewed), TaskStatus::InProgress);
        });
    }

    /// **While a run stands before a take that would wait, what keeps it waiting is counted**
    /// (`AMB-D-999`): the tasks the filter matches that cannot be taken, by what stops them — a task
    /// stopped twice counted under both and once in the whole. Once a task it can take turns up, there
    /// is nothing to say.
    #[test]
    fn while_it_waits_the_tasks_it_cannot_take_are_counted_by_what_stops_them() {
        use crate::ops::automation_run::held_back as run_held_back;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            // A person's task, which the filter does not take, is what two of the AI's wait on.
            let first = mk_task_in(tx, "the part before", Some(project));
            let after_a = ours(tx, "after it, a", &automation, None);
            let after_b = ours(tx, "after it, b", &automation, None);
            crate::ops::dependency::add(tx, after_a, first, None).expect("depend");
            crate::ops::dependency::add(tx, after_b, first, None).expect("depend");
            // One of them also stands on a decision nobody has settled.
            let premise = crate::ops::decision::add(
                tx,
                crate::ops::decision::NewDecision {
                    proposed_by: None,
                    title: "the premise".to_string(),
                    body: String::new(),
                    project_id: project,
                    made_in: None,
                },
            )
            .expect("decision");
            crate::ops::decision::link(tx, premise.id, after_b).expect("link");
            // And one waits for its start day.
            let later = ours(tx, "later", &automation, None);
            let day = crate::time::today() + chrono::Days::new(30);
            task::update(tx, later, TaskPatch { start: Some(crate::model::Schedule::Day(day)), ..Default::default() }).expect("start");

            let run = launched(tx, &automation);
            let held = run_held_back(tx.conn(), run.id).expect("held back").expect("it is waiting");
            assert_eq!(held.tasks, 3, "each task once, however many things stop it");
            assert_eq!(
                held.blockers,
                vec![HeldByRecord { id: first, title: "the part before".into(), count: 2 }],
            );
            assert_eq!(
                held.decisions,
                vec![HeldByRecord { id: premise.id, title: "the premise".into(), count: 1 }],
            );
            assert_eq!((held.not_started, held.first_start), (1, Some(crate::model::Schedule::Day(day))));
            assert!(held.values.is_empty());
            assert_eq!(held.drafts, 0);
            assert_eq!(held.awaits_person, 1, "only the task on the decision waits for a person's hand");
            assert_eq!(held.first_draft, None);

            // A start at an instant before that day is the first, and it is carried to the instant.
            let sooner = ours(tx, "sooner", &automation, None);
            let at = crate::time::Timestamp(crate::time::Timestamp::now().0 + chrono::Duration::hours(2));
            task::update(tx, sooner, TaskPatch { start: Some(crate::model::Schedule::At(at)), ..Default::default() })
                .expect("start");
            let held = run_held_back(tx.conn(), run.id).expect("held back").expect("still waiting");
            assert_eq!((held.not_started, held.first_start), (2, Some(crate::model::Schedule::At(at))));
            let json = serde_json::to_value(&held).expect("json");
            assert_eq!(json["first_start"], serde_json::json!(at.local_date().to_string()), "the day, as it always was");
            assert_eq!(json["first_start_at"], serde_json::json!(at.to_rfc3339_z()), "and the instant beside it");

            ours(tx, "one it can take", &automation, None);
            assert_eq!(run_held_back(tx.conn(), run.id).expect("held back"), None, "no longer waiting");
        });
    }

    /// **A pause takes hold where the run comes to a take that would wait**, since a step never opened
    /// never reports; picked up again, the run starts at its entry as though it had just been launched,
    /// and with still nothing to take it ends there.
    #[test]
    fn a_pause_takes_hold_where_it_would_wait_and_a_resume_starts_it_again() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            let run = launched(tx, &automation);

            automation_stop::pause(tx, run.id).expect("pause");
            match open_entry(tx, &run) {
                Opened::Waiting { run: paused } => {
                    assert_eq!(paused.status, AutomationRunStatus::Paused);
                    assert!(!paused.pause_requested);
                    assert_eq!(
                        paused.pause_kind,
                        Some(crate::model::AutomationPauseKind::EndOfAction),
                        "the run itself was asked to pause, not to pause before its next task",
                    );
                }
                other => panic!("the pause takes hold, not {other:?}"),
            }
            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Nothing));

            let resumed = automation_stop::resume(tx, run.id, None, nothing_asked(), None).expect("resume");
            assert_eq!(resumed.run.pause_kind, None, "a running run is stopped at neither");
            assert_eq!(resumed.next.builtin.as_deref(), Some("take_task"), "back at its entry");
            assert!(matches!(open_entry(tx, &run), Opened::LeftToWait { .. }));
        });
    }

    /// The picture a run goes round: take a task, work on it, close it, and back to take the next —
    /// nothing to take ends the run there.
    fn round_picture(tx: &WriteTx<'_>, project: i64) -> Automation {
        let automation =
            automation::add(tx, project, NewAutomation { name: "round".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place take");
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        let close = automation::placement_add(tx, automation.id, action(tx, "close_task").expect("close").id)
            .expect("place close");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
        automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(close.id), None).expect("work → close");
        automation::edge_add(tx, on, close.id, None, EdgeTarget::Go(take.id), None).expect("close → take");
        start_at(tx, automation.id, take.id);
        as_it_stands(tx, &automation)
    }

    /// Report the agent's step open in this opening as done, and answer where the run went from there.
    fn report_done(tx: &WriteTx<'_>, opening: &crate::ops::automation_step::Opening) -> Next {
        let exits: Vec<crate::model::RunDefExit> = serde_json::from_str(&opening.run_def.exits).expect("exits");
        let done = exits.iter().find(|e| e.name == crate::model::DONE_EXIT).map(|e| e.id);
        crate::ops::automation_report::done(tx, opening.run_step.id, done, "done").expect("report")
    }

    /// **Launch the round, take the first task, and be asked to pause before the next while working on
    /// it** — then close it and come back round to take the next. Answers the run, the task it worked,
    /// and what opening the take again did.
    fn paused_after_the_first(tx: &WriteTx<'_>, automation: &Automation) -> (AutomationRun, i64, Opened) {
        let claude = ["claude".to_string()];
        let run = launched(tx, automation);
        let Opened::Carried { next: Next::Step(work), .. } = open_entry(tx, &run) else {
            panic!("the take goes on to the work");
        };
        let first = read::automation_run_task_last(tx.conn(), run.id)
            .expect("read")
            .and_then(|s| s.task_id)
            .expect("the task it took");
        let Opened::Ready(opening) = open(tx, run.id, work.id, Some(&claude)).expect("open the work") else {
            panic!("the work opens a terminal");
        };

        let automation_stop::Paused::Asked(asked) =
            automation_stop::pause_run_before_next_task(tx, run.id).expect("ask")
        else {
            panic!("the task under way is not cut off");
        };
        assert!(asked.pause_before_next_task);
        assert_eq!(asked.status, AutomationRunStatus::Running);

        let Next::Step(close) = report_done(tx, &opening) else { panic!("the work goes on to the close") };
        let Opened::Carried { next: Next::Step(take), .. } = open(tx, run.id, close.id, Some(&claude)).expect("close")
        else {
            panic!("the close goes back round to the take");
        };
        assert_eq!(status(tx, first), TaskStatus::Done, "the task it was on is finished first");
        let opened = open(tx, run.id, take.id, Some(&claude)).expect("open the take");
        (run, first, opened)
    }

    /// **Asked to pause before its next task, a run finishes the one it is on and pauses where it comes
    /// to take the next — without taking it** (`AMB-D-1009`). Picked up again, it takes the next.
    #[test]
    fn asked_to_pause_before_its_next_task_it_pauses_there_and_takes_it_once_resumed() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            let a = ours(tx, "a", &automation, Some(Priority::High));
            let b = ours(tx, "b", &automation, Some(Priority::Low));

            let (run, first, opened) = paused_after_the_first(tx, &automation);
            assert_eq!(first, a);
            match opened {
                Opened::Waiting { run: paused } => {
                    assert_eq!(paused.status, AutomationRunStatus::Paused);
                    assert!(!paused.pause_before_next_task, "the request is spent");
                    assert_eq!(
                        paused.pause_kind,
                        Some(crate::model::AutomationPauseKind::BeforeNextTask),
                        "but which pause it was is kept",
                    );
                }
                other => panic!("the run pauses before the take, not {other:?}"),
            }
            assert_eq!(status(tx, b), TaskStatus::Todo, "the next task is not taken");
            let paused = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert!(!automation_stop::pauses_before_next_task(&paused));

            let resumed = automation_stop::resume(tx, run.id, None, nothing_asked(), None).expect("resume");
            assert_eq!(resumed.next.builtin.as_deref(), Some("take_task"), "back at the take");
            assert_eq!(resumed.run.pause_kind, None);
            assert!(matches!(
                open(tx, run.id, resumed.next.id, Some(&[])).expect("open"),
                Opened::Carried { next: Next::Step(_), .. }
            ));
            assert_eq!(status(tx, b), TaskStatus::InProgress, "and the next task is taken");
        });
    }

    /// **Picked up after pausing before its next task, a run is copied down afresh from the automation as
    /// it stands now, and starts at the new copy's entry** (`AMB-D-1015`). The copy it launched with is
    /// kept: the steps it opened from it still point at it.
    #[test]
    fn picked_up_before_its_next_task_a_run_goes_on_the_automation_as_it_stands_now() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            ours(tx, "a", &automation, Some(Priority::High));
            let b = ours(tx, "b", &automation, Some(Priority::Low));
            let (run, _, opened) = paused_after_the_first(tx, &automation);
            assert!(matches!(opened, Opened::Waiting { .. }));
            let launched_with = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            let last_copied = launched_with.last().expect("a copy").id;
            let work = launched_with.iter().find(|d| d.name == "work").expect("the work's copy");
            let work_step = work.step_id.expect("the work's step");
            automation::step_update(
                tx,
                work_step,
                None,
                Some("work on it again"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("rewrite the work");
            assert!(
                launched_with.iter().all(|d| d.automation_version.is_none()),
                "launched before anybody saved the automation"
            );
            automation::version_add(tx, automation.id).expect("save the automation");

            let resumed = automation_stop::resume(tx, run.id, None, nothing_asked(), None).expect("resume");
            assert_eq!(resumed.run.status, AutomationRunStatus::Running);
            assert_eq!(resumed.next.builtin.as_deref(), Some("take_task"), "at the new copy's entry");
            assert!(resumed.next.id > last_copied, "copied down afresh");
            let Waiting::Step(waiting) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the run stands before a step");
            };
            assert_eq!(waiting.id, resumed.next.id, "the new copy's entry, not the take it paused before");
            let kept = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert_eq!(kept.len(), launched_with.len() * 2, "the copy it launched with is kept");
            assert!(
                kept.iter().filter(|d| d.id > last_copied).all(|d| d.automation_version == Some(1)),
                "the new copy records the version it was taken from"
            );
            for step in read::automation_run_steps_of(tx.conn(), run.id).expect("steps") {
                assert!(step.run_def_id <= last_copied, "what it opened before points at the old copy");
            }

            let Opened::Carried { next: Next::Step(next), .. } =
                open(tx, run.id, resumed.next.id, Some(&[])).expect("open")
            else {
                panic!("the take goes on to the work");
            };
            assert_eq!(status(tx, b), TaskStatus::InProgress, "the next task is taken");
            assert!(next.id > last_copied, "on the new copy");
            assert_eq!(next.prompt.as_deref(), Some("work on it again"), "the work as it was rewritten");
        });
    }

    /// **A run paused before its next task is not picked up on an automation that would not launch**
    /// (`AMB-D-1015`): the resume is refused, nothing is copied down, and the run stays paused.
    #[test]
    fn a_run_is_not_picked_up_on_an_automation_that_would_not_launch() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            ours(tx, "a", &automation, Some(Priority::High));
            let (run, _, _) = paused_after_the_first(tx, &automation);
            let copies = read::automation_run_defs_of(tx.conn(), run.id).expect("defs").len();
            start_at(tx, automation.id, placed(tx, run.id, "work"));

            let refused = automation_stop::resume(tx, run.id, None, nothing_asked(), None).expect_err("refused");
            assert!(matches!(refused, crate::error::Error::NotReady(_)), "{refused:?}");
            let paused = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(paused.status, AutomationRunStatus::Paused, "it stays paused");
            assert_eq!(paused.pause_kind, Some(crate::model::AutomationPauseKind::BeforeNextTask));
            assert_eq!(
                read::automation_run_defs_of(tx.conn(), run.id).expect("defs").len(),
                copies,
                "nothing is copied down",
            );
        });
    }

    /// **Canceled there, a run paused before its next task hands nothing back** — it holds no task.
    #[test]
    fn a_run_paused_before_its_next_task_is_canceled_with_nothing_to_hand_back() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            ours(tx, "a", &automation, Some(Priority::High));
            let b = ours(tx, "b", &automation, Some(Priority::Low));

            let (run, first, opened) = paused_after_the_first(tx, &automation);
            assert!(matches!(opened, Opened::Waiting { .. }));
            let ended = automation_stop::cancel(tx, run.id).expect("cancel");
            assert_eq!(ended.run.status, AutomationRunStatus::Canceled);
            assert_eq!(ended.run.pause_kind, None, "a run that is over is paused at neither");
            assert_eq!(status(tx, first), TaskStatus::Done, "the task it finished stays finished");
            assert_eq!(status(tx, b), TaskStatus::Todo, "and the next was never taken");
        });
    }

    /// **A run standing before a take that would wait pauses as soon as it is asked** (`AMB-D-1009`): it
    /// holds no task, so nothing waits for its next look. Picked up again, it is back at the take.
    #[test]
    fn a_run_standing_before_a_take_that_would_wait_is_paused_as_soon_as_it_is_asked() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            let run = launched(tx, &automation);
            assert!(is_waiting(tx.conn(), run.id).expect("waiting"));

            let automation_stop::Paused::Now(ended) =
                automation_stop::pause_run_before_next_task(tx, run.id).expect("ask")
            else {
                panic!("paused there and then");
            };
            assert_eq!(ended.run.status, AutomationRunStatus::Paused);
            let paused = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(paused.status, AutomationRunStatus::Paused);
            assert!(!paused.pause_before_next_task, "nothing is left asked");
            assert_eq!(paused.pause_kind, Some(crate::model::AutomationPauseKind::BeforeNextTask));
            assert!(read::automation_run_steps_of(tx.conn(), run.id).expect("steps").is_empty(), "nothing taken");

            let resumed = automation_stop::resume(tx, run.id, None, nothing_asked(), None).expect("resume");
            assert_eq!(resumed.next.builtin.as_deref(), Some("take_task"), "back at the take");
        });
    }

    /// **A stopped automation has no seat of its launch filled**, a task turning up or not, with no run
    /// to pause; and a run that comes back round to take its next task pauses there without taking it.
    #[test]
    fn stopping_the_automation_fills_no_seat_and_pauses_its_runs_before_their_next_task() {
        use crate::ops::automation_run::launches_to_top_up;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let waits = picture(tx, project).0;
            let run = launched(tx, &waits);
            let launch = run.launch_id.expect("a launch");
            assert!(matches!(open_entry(tx, &run), Opened::LeftToWait { .. }));
            let stopped = automation::stop(tx, waits.id, true).expect("stop");
            assert!(stopped.paused.is_empty(), "no run stands waiting for a task");
            ours(tx, "new", &waits, None);
            assert!(launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty(), "stopped");
            let again = automation::stop(tx, waits.id, true).expect("stop again");
            assert!(again.paused.is_empty(), "stopping a stopped one writes nothing");

            let rounds = round_picture(tx, project);
            let a = ours(tx, "a", &rounds, Some(Priority::High));
            let b = ours(tx, "b", &rounds, Some(Priority::Low));
            let claude = ["claude".to_string()];
            let run = launched(tx, &rounds);
            let Opened::Carried { next: Next::Step(work), .. } = open_entry(tx, &run) else {
                panic!("the take goes on to the work");
            };
            assert_eq!(status(tx, a), TaskStatus::InProgress);
            let Opened::Ready(opening) = open(tx, run.id, work.id, Some(&claude)).expect("open the work") else {
                panic!("the work opens a terminal");
            };
            automation::stop(tx, rounds.id, true).expect("stop");
            let Next::Step(close) = report_done(tx, &opening) else { panic!("the work goes on to the close") };
            let Opened::Carried { next: Next::Step(take), .. } = open(tx, run.id, close.id, Some(&claude)).expect("close")
            else {
                panic!("the close goes back round to the take");
            };
            assert_eq!(status(tx, a), TaskStatus::Done, "the round under way is finished");
            let Opened::Waiting { run: paused } = open(tx, run.id, take.id, Some(&claude)).expect("open the take")
            else {
                panic!("the run pauses before the take");
            };
            assert_eq!(paused.status, AutomationRunStatus::Paused);
            assert_eq!(paused.pause_kind, Some(crate::model::AutomationPauseKind::BeforeNextTask));
            assert_eq!(status(tx, b), TaskStatus::Todo, "the next task is not taken");
        });
    }

    /// **A run standing at its entry is copied down afresh once a newer version is saved**
    /// (`AMB-D-1015`), and starts at the new copy's entry. The copy it launched with is kept, and with
    /// nothing newer saved since, it is left as it is.
    #[test]
    fn a_run_standing_at_its_entry_takes_up_the_version_saved_since() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            let run = launched(tx, &automation);
            assert!(matches!(take_up_newer(tx, run.id, None, nothing_asked(), None).expect("look"), TakenUp::Same));
            let launched_with = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            let last_copied = launched_with.last().expect("a copy").id;
            assert!(launched_with.iter().all(|d| d.automation_version.is_none()), "taken from the draft");

            automation::version_add(tx, automation.id).expect("save the automation");
            let TakenUp::Copied(entry) = take_up_newer(tx, run.id, None, nothing_asked(), None).expect("look") else {
                panic!("copied down afresh");
            };
            assert!(entry.id > last_copied, "on the new copy");
            assert_eq!(entry.builtin.as_deref(), Some("take_task"));
            assert_eq!(entry.automation_version, Some(1));
            let Waiting::Step(waiting) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the run stands before a step");
            };
            assert_eq!(waiting.id, entry.id, "the new copy's entry is what is opened next");
            let kept = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert_eq!(kept.len(), launched_with.len() * 2, "the copy it launched with is kept");
            assert!(matches!(take_up_newer(tx, run.id, None, nothing_asked(), None).expect("look"), TakenUp::Same));
            let still = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(still.status, AutomationRunStatus::Running);
        });
    }

    /// **A run partway through a task is left on the copy it has**, newer version or not: only a run
    /// standing at its entry holds no task.
    #[test]
    fn a_run_partway_through_a_task_is_left_on_its_copy() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = picture(tx, project);
            ours(tx, "a", &automation, None);
            let (run, _, next) = carried(tx, &automation);
            assert!(matches!(next, Next::Step(_)), "on to the work");
            automation::version_add(tx, automation.id).expect("save the automation");
            let copies = read::automation_run_defs_of(tx.conn(), run.id).expect("defs").len();

            assert!(matches!(take_up_newer(tx, run.id, None, nothing_asked(), None).expect("look"), TakenUp::Same));
            assert_eq!(read::automation_run_defs_of(tx.conn(), run.id).expect("defs").len(), copies);
        });
    }

    /// **A newer version that would not launch fails the run standing at its entry**, with what it did
    /// not pass kept on the run. Nothing is copied down, and there is no task to hand back.
    #[test]
    fn a_run_standing_at_its_entry_fails_on_a_version_that_would_not_launch() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            let run = launched(tx, &automation);
            let copies = read::automation_run_defs_of(tx.conn(), run.id).expect("defs").len();
            start_at(tx, automation.id, placed(tx, run.id, "work"));
            automation::version_add(tx, automation.id).expect("save past the check");

            let TakenUp::Failed(failed) = take_up_newer(tx, run.id, None, nothing_asked(), None).expect("look") else {
                panic!("the run fails");
            };
            assert_eq!(failed.status, AutomationRunStatus::Failed);
            assert_eq!(failed.stopped_reason, Some(AutomationStoppedReason::FailedCheck));
            let kept = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(kept.stopped_reason, Some(AutomationStoppedReason::FailedCheck));
            let detail = kept.stopped_detail.expect("what it did not pass is kept");
            assert!(detail.starts_with("cannot launch 'take'"), "{detail}");
            assert_eq!(read::automation_run_defs_of(tx.conn(), run.id).expect("defs").len(), copies);
        });
    }

    /// **A newer version that starts by filing a task fails the run standing at its entry**: nothing
    /// reaches the title of the task it files, since nothing is handed to where a round starts
    /// (`AMB-D-1123`).
    #[test]
    fn a_run_standing_at_its_entry_fails_on_a_version_that_starts_by_filing_a_task() {
        use crate::ops::automation_builtin_make::{MADE_AND_TAKEN, TAKE_IT, TITLE, WHAT_THEN};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            let run = launched(tx, &automation);
            let written = action(tx, "make_task").expect("the built-in's action");
            let make = automation::placement_add(tx, automation.id, written.id).expect("place it");
            automation::cfg_set(tx, make.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
                .expect("take it");
            let work = placed(tx, run.id, "work");
            let on = AutomationPictureOwner::Automation;
            automation::edge_add(tx, on, make.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(work), None)
                .expect("onward");
            start_at(tx, automation.id, make.id);
            let claude = ["claude".to_string()];
            let unmet = check(tx.conn(), automation.id, Some(&claude), nothing_asked(), None).expect("check");
            assert!(
                unmet.iter().any(|u| matches!(u, Unmet::UnwiredInput { port, .. } if port == TITLE)),
                "nothing reaches the title: {unmet:?}",
            );
            automation::version_add(tx, automation.id).expect("save the automation");

            let TakenUp::Failed(failed) = take_up_newer(tx, run.id, None, nothing_asked(), None).expect("look") else {
                panic!("the run fails");
            };
            assert_eq!(failed.stopped_reason, Some(AutomationStoppedReason::FailedCheck));
            let detail = failed.stopped_detail.expect("what it did not pass is kept");
            assert!(detail.contains("has nothing reaching it"), "{detail}");
        });
    }

    /// **A run already asked to pause at the end of its action is answered as it stands**
    /// (`AMB-D-1019`): it stops sooner that way, so it is not asked to pause before its next task too.
    #[test]
    fn a_run_asked_to_pause_at_the_end_of_its_action_is_answered_as_it_stands() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let run = launched(tx, &picture(tx, project).0);
            let automation_stop::Paused::Asked(pausing) = automation_stop::pause(tx, run.id).expect("pause") else {
                panic!("asked to pause at the end of its action");
            };

            let automation_stop::Paused::Asked(asked) =
                automation_stop::pause_run_before_next_task(tx, run.id).expect("ask")
            else {
                panic!("still asked");
            };
            assert!(asked.pause_requested);
            assert!(!asked.pause_before_next_task, "not asked to pause before its next task too");
            assert_eq!(asked.updated_at, pausing.updated_at, "nothing is written");
        });
    }

    /// **Asked on its own, a run working a task is asked and the others are not** (`AMB-D-1019`): it
    /// stays `running` with the request on it, and asking again answers it as it stands. Another run of
    /// the same project is left running.
    #[test]
    fn asked_on_its_own_a_run_is_asked_and_no_other() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let rounds = round_picture(tx, project);
            ours(tx, "a", &rounds, Some(Priority::High));
            let run = launched(tx, &rounds);
            assert!(matches!(open_entry(tx, &run), Opened::Carried { next: Next::Step(_), .. }));
            let other = launched(tx, &picture(tx, project).0);

            let automation_stop::Paused::Asked(asked) =
                automation_stop::pause_run_before_next_task(tx, run.id).expect("ask")
            else {
                panic!("the task under way is not cut off");
            };
            assert_eq!(asked.status, AutomationRunStatus::Running);
            assert!(asked.pause_before_next_task);
            let automation_stop::Paused::Asked(again) =
                automation_stop::pause_run_before_next_task(tx, run.id).expect("ask again")
            else {
                panic!("still asked");
            };
            assert!(again.pause_before_next_task);
            assert_eq!(again.updated_at, asked.updated_at, "nothing is written twice");

            let other = read::automation_run(tx.conn(), other.id).expect("read").expect("run");
            assert_eq!(other.status, AutomationRunStatus::Running, "the other run goes on");
            assert!(!other.pause_before_next_task, "and is not asked");
        });
    }

    /// **A run that takes no tasks is asked all the same** (`AMB-D-1019`, `AMB-D-1127`): the request is
    /// written, and takes hold where the run comes back to where its round starts.
    #[test]
    fn a_run_that_takes_no_tasks_is_asked_all_the_same() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let taskless =
                automation::add(tx, project, NewAutomation { name: "no tasks".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                    .expect("automation");
            let (work_action, work) = mk_placed(tx, &taskless, "work", "work on it", "claude");
            mk_out(tx, &work_action, None, "task", AutomationPortKind::TaskTake, false);
            automation::edge_add(tx, AutomationPictureOwner::Automation, work.id, None, EdgeTarget::Done, None)
                .expect("done");
            start_at(tx, taskless.id, work.id);
            let claude = ["claude".to_string()];
            let by = Launcher {
                default_agent: None,
                startable: Some(&claude),
                models: nothing_asked(),
                workspace_open: Some(true),
                by: Some(ActorKind::Ai),
            };
            let plain = launch_past_the_task_checks(tx, taskless.id, &by).expect("launch");

            assert!(automation_stop::pauses_before_next_task(&plain), "asking it would do something");
            let asked = automation_stop::pause_run_before_next_task(tx, plain.id).expect("asked");
            assert!(matches!(asked, automation_stop::Paused::Asked(_)), "{asked:?}");
            let plain = read::automation_run(tx.conn(), plain.id).expect("read").expect("run");
            assert_eq!(plain.status, AutomationRunStatus::Running);
            assert!(plain.pause_before_next_task);
        });
    }

    /// **It reads no answer about what to do with nothing to take, and has no way out for it**
    /// (`AMB-D-1130`, `AMB-D-1131`): its one setting is the filter, and it leaves by [`TAKEN`] or by the
    /// error way out. So the launch check asks a line of nothing but [`TAKEN`].
    #[test]
    fn it_is_set_only_by_its_filter_and_leaves_only_having_taken_one() {
        let settings: Vec<&str> = TAKE_TASK.settings.iter().map(|s| s.name).collect();
        assert_eq!(settings, vec![FILTER]);
        let exits: Vec<&str> = TAKE_TASK.exits.iter().map(|e| e.name).collect();
        assert_eq!(exits, vec![TAKEN]);
        assert_eq!(TAKE_TASK.version, 2, "its ways out and settings changed from the first");
    }

    #[test]
    fn the_expression_drops_status_and_ready_and_asks_for_the_takeable() {
        assert_eq!(
            with_asked(narrowing(Some(r#"{"ready":["yes"],"priority":["high"]}"#)), HELD),
            "priority:high status:todo ready:no",
            "counting what is held back asks for the same tasks, the ones that cannot be taken",
        );
        assert_eq!(expression(None), "assignee:me-ai status:todo ready:yes");
        assert_eq!(
            expression(Some(r#"{"status":["done"],"ready":["no"],"priority":["high"]}"#)),
            "priority:high status:todo ready:yes",
        );
        assert_eq!(expression(Some(r#"{"sort":"due"}"#)), "status:todo ready:yes");
        assert_eq!(
            expression(Some(r#""x""#)),
            "assignee:me-ai status:todo ready:yes",
            "an answer that is not its parts narrows as no answer does, never to every task",
        );
    }

    /// An automation with two roles, each starting its round at a built-in that takes a task of its own
    /// (`AMB-D-1123`), which goes on to an agent's step that goes on to closing the task. Answers each
    /// role with the agent's step its round goes on to. The role it was born with, starting nowhere, is
    /// taken out.
    fn by_role(tx: &WriteTx<'_>, project: i64) -> (Automation, [(i64, AutomationPlacement); 2]) {
        let automation =
            automation::add(tx, project, NewAutomation { name: "roles".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                .expect("automation");
        let born = read::automation_roles_of(tx.conn(), automation.id).expect("roles").remove(0);
        let written = action(tx, "take_task").expect("the built-in's action");
        let on = AutomationPictureOwner::Automation;
        let mut roles = Vec::new();
        for name in ["実装", "レビュー"] {
            let take = automation::placement_add(tx, automation.id, written.id).expect("place it");
            let (_, work) = mk_placed(tx, &automation, name, "work on it", "claude");
            automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("onward");
            crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
            let new = NewRole { name: name.into(), entry_placement_id: Some(take.id), ..NewRole::default() };
            let role = automation::role_add(tx, automation.id, new).expect("a role");
            roles.push((role.id, work));
        }
        automation::role_delete(tx, born.id).expect("the role it was born with");
        let automation = as_it_stands(tx, &automation);
        let reviewer = roles.pop().expect("the reviewer");
        let builder = roles.pop().expect("the builder");
        (automation, [builder, reviewer])
    }

    /// **A round starts where its seat's role starts its round**, takes a task of that role there, and goes
    /// on down the line drawn from it (`AMB-D-1123`). A task with no role is not taken, however high it
    /// stands.
    #[test]
    fn a_round_starts_by_taking_a_task_where_its_role_starts() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [_, (reviewer, review)]) = by_role(tx, project);
            let claude = ["claude".to_string()];
            let unmet = check(tx.conn(), automation.id, Some(&claude), nothing_asked(), None).expect("check");
            assert!(unmet.is_empty(), "{unmet:?}");

            let roleless = for_ai(tx, "no role", project, Some(Priority::High));
            let reviewed = for_ai(tx, "to review", project, None);
            give_to(tx, reviewed, reviewer);

            let (run, _, next) = carried(tx, &automation);
            assert_eq!(status(tx, reviewed), TaskStatus::InProgress, "the task of a role is taken");
            assert_eq!(status(tx, roleless), TaskStatus::Todo, "one with no role is left for a person");
            let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
            assert_eq!((stretch.task_id, stretch.role_id), (Some(reviewed), Some(reviewer)));
            let Next::Step(def) = next else { panic!("the round goes on, not {next:?}") };
            assert_eq!(def.placement_id, Some(review.id), "where the reviewer's round starts");
        });
    }

    /// **Where every task left has no role, there is none to take**, and the run ends before the step.
    #[test]
    fn a_task_with_no_role_is_not_taken_where_roles_say_where_rounds_start() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = by_role(tx, project);
            let roleless = for_ai(tx, "no role", project, None);
            let run = launched(tx, &automation);
            assert!(matches!(open_entry(tx, &run), Opened::LeftToWait { .. }));
            assert_eq!(status(tx, roleless), TaskStatus::Todo);
        });
    }

    /// **Where a role starts is checked before a launch**, as a placement a line reaches is: nothing
    /// draws a line to it, and a run of the role starts there all the same.
    #[test]
    fn where_a_role_starts_is_checked_as_a_placement_a_run_reaches() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = by_role(tx, project);
            let (_, bare) = mk_placed(tx, &automation, "bare", "nothing after it", "claude");
            let new = NewRole { name: "設計".into(), entry_placement_id: Some(bare.id), ..NewRole::default() };
            automation::role_add(tx, automation.id, new).expect("a role");
            let claude = ["claude".to_string()];
            let unmet = check(tx.conn(), automation.id, Some(&claude), nothing_asked(), None).expect("check");
            assert!(
                unmet.iter().any(|u| matches!(u, Unmet::OpenExit { placement, .. } if *placement == Some(bare.id))),
                "its way out has nothing after it: {unmet:?}",
            );
        });
    }

    /// **A run is a seat of one role, and takes only that role's tasks.** It is bound at launch to the
    /// first role with a task to take, and a task of another role is left however high it stands.
    #[test]
    fn a_run_takes_only_the_tasks_of_the_role_it_is_a_seat_of() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [(builder, _), (reviewer, review)]) = by_role(tx, project);
            let reviewed = for_ai(tx, "to review", project, None);
            give_to(tx, reviewed, reviewer);

            let run = launched(tx, &automation);
            assert_eq!(run.role_id, Some(reviewer), "the first role with a task to take");
            let built = for_ai(tx, "to build", project, Some(Priority::High));
            give_to(tx, built, builder);

            let Opened::Carried { next, .. } = open_entry(tx, &run) else { panic!("the built-in is carried out") };
            assert_eq!(status(tx, reviewed), TaskStatus::InProgress, "its own role's task is taken");
            assert_eq!(status(tx, built), TaskStatus::Todo, "another role's is left, however high it stands");
            let Next::Step(def) = next else { panic!("the round goes on, not {next:?}") };
            assert_eq!(def.placement_id, Some(review.id));
            assert_eq!(read::automation_run(tx.conn(), run.id).expect("read").expect("run").role_id, Some(reviewer));
        });
    }

    /// **With no task to take, a run is a seat of the first role**, in the order they are defined in.
    #[test]
    fn a_run_with_no_task_to_take_is_a_seat_of_the_first_role() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [(builder, _), _]) = by_role(tx, project);
            assert_eq!(launched(tx, &automation).role_id, Some(builder));
        });
    }

    /// **Runs are started role by role, into a free seat** (`AMB-D-1123`). A role whose seats are all
    /// taken gets no more runs however many of its tasks wait, while another role with a free seat and a
    /// task of its own is given one. A launch with every seat taken is refused.
    #[test]
    fn runs_are_started_into_a_free_seat_of_the_role_with_a_task() {
        use crate::ops::automation_run::{launches_to_top_up, seats, top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [(builder, _), (reviewer, _)]) = by_role(tx, project);
            for title in ["build one", "build two"] {
                let built = for_ai(tx, title, project, None);
                give_to(tx, built, builder);
            }
            let reviewed = for_ai(tx, "to review", project, None);
            give_to(tx, reviewed, reviewer);

            let first = launched(tx, &automation);
            assert_eq!(first.role_id, Some(builder), "the first role with a task to take");
            let launch = first.launch_id.expect("a launch");
            open_entry(tx, &first);

            assert_eq!(launches_to_top_up(tx.conn(), &[launch]).expect("ask"), vec![launch]);
            let second = top_up(tx, launch, &launcher()).expect("top up").expect("a run for the reviewer");
            assert_eq!(second.role_id, Some(reviewer), "the builder's one seat is taken");
            open_entry(tx, &second);
            assert_eq!(status(tx, reviewed), TaskStatus::InProgress);

            let taken: Vec<(Option<i64>, bool)> =
                seats(tx.conn(), &automation).expect("seats").iter().map(|s| (s.role_id, s.free())).collect();
            assert_eq!(taken, vec![(Some(builder), false), (Some(reviewer), false)]);
            assert!(
                launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty(),
                "a builder's task still waits, and the builder has no free seat",
            );
            let err = launched_or_refused(tx, &automation).expect_err("every seat is taken");
            let crate::error::Error::Invalid(msg) = err else { panic!("a launch past its seats is invalid") };
            assert_eq!(msg.code(), Some(crate::error::ErrorCode::InvalidAutomationRunLimit));
        });
    }

    /// **A run keeps its role until it ends**: copied down again from a newer version, it is still a seat
    /// of the role it launched in, and does not take a task of another role that has turned up since.
    #[test]
    fn a_run_keeps_its_role_when_it_takes_up_a_newer_version() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [(builder, _), (reviewer, _)]) = by_role(tx, project);
            let run = launched(tx, &automation);
            assert_eq!(run.role_id, Some(builder));
            let reviewed = for_ai(tx, "to review", project, None);
            give_to(tx, reviewed, reviewer);

            automation::version_add(tx, automation.id).expect("save the automation");
            assert!(matches!(
                take_up_newer(tx, run.id, None, nothing_asked(), None).expect("look"),
                TakenUp::Copied(_)
            ));
            assert_eq!(read::automation_run(tx.conn(), run.id).expect("read").expect("run").role_id, Some(builder));
            open_entry(tx, &run);
            assert_eq!(status(tx, reviewed), TaskStatus::Todo, "a task of another role is not taken");
        });
    }

    /// **A run takes its role's tasks in the order the role takes them in** — by priority, the oldest
    /// first within one, until the role is given another: the nearest deadline, the oldest, or the order a
    /// person put them in. Each order picks a task the others do not, and none of them is the first task
    /// filed, so no order is passed by falling back on the tasks' ids.
    #[test]
    fn a_run_takes_its_roles_tasks_in_the_order_the_role_takes_them_in() {
        let cases = [
            (None, "high, made first"),
            (Some(TakeOrder::Priority), "high, made first"),
            (Some(TakeOrder::Due), "due first"),
            (Some(TakeOrder::Created), "made first of all"),
            (Some(TakeOrder::Order), "put first"),
        ];
        for (order, first) in cases {
            with_tx(|tx| {
                let project = mk_project(tx, "amenbo");
                let (automation, [_, (reviewer, _)]) = by_role(tx, project);
                if order.is_some() {
                    let change = automation::RoleChange { take_order: order, ..Default::default() };
                    automation::role_update(tx, reviewer, change).expect("the order");
                }
                let day = |d: &str| Schedule::Day(d.parse().expect("a day"));
                let mut ids = std::collections::BTreeMap::new();
                for (title, priority, made, due) in [
                    ("high, made later", Some(Priority::High), "2026-01-03T00:00:00Z", None),
                    ("high, made first", Some(Priority::High), "2026-01-01T00:00:00Z", Some(day("2026-12-31"))),
                    ("due first", Some(Priority::Low), "2026-01-02T00:00:00Z", Some(day("2026-12-01"))),
                    ("made first of all", None, "2025-12-31T00:00:00Z", None),
                    ("put first", None, "2026-01-05T00:00:00Z", None),
                ] {
                    let id = for_ai(tx, title, project, priority);
                    give_to(tx, id, reviewer);
                    if due.is_some() {
                        task::update(tx, id, TaskPatch { due, ..Default::default() }).expect("due");
                    }
                    tx.conn()
                        .execute("UPDATE task SET created_at = ?1 WHERE id = ?2", rusqlite::params![made, id])
                        .expect("made then");
                    ids.insert(title, id);
                }
                task::move_to(tx, ids["put first"], None, crate::ops::Position::Top, ActorKind::Human)
                    .expect("put it first");

                let run = launched(tx, &automation);
                assert_eq!(run.role_id, Some(reviewer));
                open_entry(tx, &run);
                for (title, id) in &ids {
                    let want = if *title == first { TaskStatus::InProgress } else { TaskStatus::Todo };
                    assert_eq!(status(tx, *id), want, "{order:?}: {title}");
                }
            });
        }
    }

    // ───────────────────────────── one launch, side by side ─────────────────────────────

    fn launch_of(tx: &WriteTx<'_>, launch: i64) -> crate::model::AutomationLaunch {
        read::automation_launch(tx.conn(), launch).expect("read").expect("launch")
    }

    fn run_of(tx: &WriteTx<'_>, run: i64) -> AutomationRun {
        read::automation_run(tx.conn(), run).expect("read").expect("run")
    }

    /// **One launch keeps its runs going side by side, up to its most, while there are tasks to take.**
    /// A run is added only once every run of the launch has taken its task, it joins the same launch
    /// and is started by whoever pressed it. A headcount saved while the launch runs waits for the next
    /// launch, and one raised on the launch makes room at once.
    #[test]
    fn one_launch_tops_up_to_its_most_while_there_are_tasks() {
        use crate::ops::automation_run::{launch_headcount_set, launches_to_top_up, top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = picture(tx, project);
            crate::ops::test_support::room_for(tx, automation.id, 2);
            for title in ["one", "two", "three", "four"] {
                ours(tx, title, &automation, None);
            }

            let first = launched(tx, &automation);
            let launch = first.launch_id.expect("a run is started in a launch");
            assert_eq!(launch_of(tx, launch).started_by_kind, Some(ActorKind::Ai));
            assert!(
                launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty(),
                "the first run has not taken its task yet",
            );
            open_entry(tx, &first);

            assert_eq!(launches_to_top_up(tx.conn(), &[launch]).expect("ask"), vec![launch]);
            let second = top_up(tx, launch, &launcher()).expect("top up").expect("a second run");
            assert_eq!(second.launch_id, Some(launch), "it joins the same launch");
            assert_eq!(second.started_by_kind, Some(ActorKind::Ai), "started by whoever pressed");
            open_entry(tx, &second);
            assert!(launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty(), "two is its most");
            assert!(top_up(tx, launch, &launcher()).expect("top up").is_none(), "nothing is written");

            let role = read::automation_roles_of(tx.conn(), automation.id).expect("roles")[0].id;
            automation::role_update(tx, role, automation::RoleChange { headcount: Some(3), ..Default::default() })
                .expect("raise the definition");
            automation::version_add(tx, automation.id).expect("save it");
            assert!(top_up(tx, launch, &launcher()).expect("top up").is_none(), "the next launch takes it up");

            launch_headcount_set(tx, launch, role, 3).expect("raise it for this launch");
            let third = top_up(tx, launch, &launcher()).expect("top up").expect("a third run");
            assert_eq!(third.launch_id, Some(launch));
            assert_eq!(
                read::automation_launch_run_ids_under_way(tx.conn(), launch).expect("runs"),
                vec![first.id, second.id, third.id],
            );
        });
    }

    /// The work of a round opened and reported done, and the close carried out after it — answering where
    /// the run went from the close.
    fn round_finished(tx: &WriteTx<'_>, run: i64, work: &crate::model::AutomationRunDef) -> Next {
        let claude = ["claude".to_string()];
        let Opened::Ready(opening) = open(tx, run, work.id, Some(&claude)).expect("open the work") else {
            panic!("the work opens a terminal");
        };
        let Next::Step(close) = report_done(tx, &opening) else { panic!("the work goes on to the close") };
        let Opened::Carried { next, .. } = open(tx, run, close.id, Some(&claude)).expect("close") else {
            panic!("the close is carried out");
        };
        next
    }

    /// Two runs of a round picture side by side in one launch, each on its task: the launch, its one role,
    /// and each run with the work its round stands before.
    fn two_at_work(
        tx: &WriteTx<'_>,
        automation: &Automation,
    ) -> (i64, i64, [(AutomationRun, Box<crate::model::AutomationRunDef>); 2]) {
        use crate::ops::automation_run::top_up;
        let first = launched(tx, automation);
        let launch = first.launch_id.expect("a launch");
        let Opened::Carried { next: Next::Step(first_work), .. } = open_entry(tx, &first) else {
            panic!("the take goes on to the work");
        };
        let second = top_up(tx, launch, &launcher()).expect("top up").expect("a second run");
        let Opened::Carried { next: Next::Step(second_work), .. } = open_entry(tx, &second) else {
            panic!("the take goes on to the work");
        };
        let role = read::automation_roles_of(tx.conn(), automation.id).expect("roles")[0].id;
        (launch, role, [(first, first_work), (second, second_work)])
    }

    /// **A headcount lowered on the launch stops no task under way.** The seat still to go goes as its run
    /// finishes its task: that run ends where it comes to take the next, completed, without taking one.
    /// The definition is left as it is.
    #[test]
    fn a_headcount_lowered_on_the_launch_lets_a_seat_go_as_its_run_finishes_a_round() {
        use crate::ops::automation_run::{launch_headcount_set, launch_headcounts, launches_to_top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            crate::ops::test_support::room_for(tx, automation.id, 2);
            for title in ["a", "b", "c", "d"] {
                ours(tx, title, &automation, None);
            }
            let (launch, role, [(first, first_work), (second, second_work)]) = two_at_work(tx, &automation);

            launch_headcount_set(tx, launch, role, 1).expect("lower it for this launch");
            let counted = launch_headcounts(tx.conn(), launch).expect("read");
            assert_eq!(counted.len(), 1);
            assert_eq!((counted[0].defined, counted[0].headcount), (2, 1), "the definition is left as it is");
            assert_eq!(counted[0].under_way, vec![first.id, second.id], "no task under way is stopped");
            assert_eq!(counted[0].to_let_go, 1, "one seat is still to go");
            assert!(launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty());

            let Next::Step(take) = round_finished(tx, first.id, &first_work) else { panic!("back round to the take") };
            let Opened::LetGo { run: let_go } = open(tx, first.id, take.id, Some(&[])).expect("open the take") else {
                panic!("the first to finish its task lets its seat go");
            };
            assert_eq!(let_go.status, AutomationRunStatus::Completed);
            let counted = launch_headcounts(tx.conn(), launch).expect("read");
            assert_eq!((counted[0].under_way.clone(), counted[0].to_let_go), (vec![second.id], 0));

            let Next::Step(take) = round_finished(tx, second.id, &second_work) else { panic!("back round to the take") };
            assert!(
                matches!(open(tx, second.id, take.id, Some(&[])).expect("open the take"), Opened::Carried { .. }),
                "the seat left takes its next task",
            );
        });
    }

    /// **A launch read at once names each role's tasks waiting to be taken**: how many there are, and
    /// the first of them in the order the role takes them in. The tasks the runs have taken are under
    /// way, and are not among them.
    #[test]
    fn a_launch_read_at_once_counts_each_roles_tasks_waiting_to_be_taken() {
        use crate::ops::automation_run::{launch_headcounts, launch_state, WAITING_NAMED};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            crate::ops::test_support::room_for(tx, automation.id, 2);
            let tasks: Vec<i64> = ["a", "b", "c", "d", "e", "f"].iter().map(|t| ours(tx, t, &automation, None)).collect();
            let (launch, role, _) = two_at_work(tx, &automation);

            let state = launch_state(tx.conn(), launch).expect("read");
            assert_eq!(state.launch.id, launch);
            assert_eq!(state.automation_name, "round");
            assert_eq!(state.roles.len(), 1);
            let one = &state.roles[0];
            assert_eq!(one.headcount, launch_headcounts(tx.conn(), launch).expect("read")[0]);
            assert_eq!(one.headcount.role_id, role);
            assert_eq!(one.waiting, 4, "the two taken are under way");
            let first: Vec<i64> = one.first_waiting.iter().map(|w| w.task_id).collect();
            assert_eq!(first, tasks[2..2 + WAITING_NAMED].to_vec(), "the first three, oldest first");
            assert_eq!(one.first_waiting[0].title, "c");
            assert!(one.first_waiting.iter().all(|w| w.from_role_id.is_none()), "none was passed on");
        });
    }

    /// **A launch read at once names what keeps a role's tasks from being taken, role by role**
    /// (`AMB-D-999`): where none of a role's tasks is there to take, the value still open before theirs
    /// and how many it holds back. A role with a task to take counts nothing, and the tasks of one role
    /// are not counted under the other's.
    #[test]
    fn a_launch_read_at_once_names_what_holds_each_roles_tasks_back() {
        use crate::model::DimensionRole;
        use crate::ops::automation_run::{launch_state, AwaitsPerson};
        use crate::ops::dimension::{self, NewDimension};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [(builder, _), (reviewer, _)]) = by_role(tx, project);
            let axis = dimension::add(
                tx,
                project,
                NewDimension {
                    name: "リリース".into(),
                    ordered: true,
                    role: DimensionRole::Closable,
                    sequential: true,
                    ..NewDimension::default()
                },
            )
            .expect("axis");
            let v1 = dimension::value_add(tx, axis.id, "v1", None).expect("v1");
            let v2 = dimension::value_add(tx, axis.id, "v2", None).expect("v2");
            let filed = |title: &str, role: i64, value: i64| {
                let id = for_ai(tx, title, project, None);
                give_to(tx, id, role);
                dimension::set(tx, id, value).expect("value");
                id
            };
            filed("build it", builder, v1.id);
            filed("held, a", reviewer, v2.id);
            filed("held, b", reviewer, v2.id);
            filed("held behind it too", builder, v2.id);
            let launch = launched(tx, &automation).launch_id.expect("a launch");

            let state = launch_state(tx.conn(), launch).expect("read");
            let [build, review] = &state.roles[..] else { panic!("two roles, not {:?}", state.roles) };
            assert_eq!(build.name, "実装");
            assert_eq!(build.waiting, 1);
            assert_eq!(build.held_back, None, "a role with a task to take is not kept waiting");
            let awaits = |tasks| Some(AwaitsPerson { tasks, dimension_id: Some(axis.id), decision_id: None, task_id: None });
            assert_eq!(
                build.awaits_person,
                awaits(1),
                "its task behind v1 waits for a person all the same, and the value is where to go",
            );
            assert_eq!(review.name, "レビュー");
            assert_eq!(review.waiting, 0);
            let held = review.held_back.as_ref().expect("the reviewer has nothing to take");
            assert_eq!(held.tasks, 2, "the builder's task held behind v1 is not counted here");
            assert_eq!(
                held.values,
                vec![HeldByValue { dimension_id: axis.id, axis: "リリース".into(), value: "v1".into(), count: 2 }],
            );
            assert_eq!(review.awaits_person, awaits(2));
        });
    }

    /// **Raised again before the seats have gone, the headcount calls the rest off**: the runs under way
    /// keep their seats, and each goes round as before.
    #[test]
    fn a_headcount_raised_again_calls_off_the_seats_still_to_go() {
        use crate::ops::automation_run::{launch_headcount_reset, launch_headcount_set, launch_headcounts};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            crate::ops::test_support::room_for(tx, automation.id, 2);
            for title in ["a", "b", "c", "d"] {
                ours(tx, title, &automation, None);
            }
            let (launch, role, [(first, first_work), _]) = two_at_work(tx, &automation);

            launch_headcount_set(tx, launch, role, 1).expect("lower it");
            assert_eq!(launch_headcounts(tx.conn(), launch).expect("read")[0].to_let_go, 1);
            let back = launch_headcount_reset(tx, launch, role).expect("back to the definition");
            assert_eq!(back.headcount, 2);
            assert_eq!(launch_headcounts(tx.conn(), launch).expect("read")[0].to_let_go, 0, "called off");

            let Next::Step(take) = round_finished(tx, first.id, &first_work) else { panic!("back round to the take") };
            assert!(
                matches!(open(tx, first.id, take.id, Some(&[])).expect("open the take"), Opened::Carried { .. }),
                "the run keeps its seat and takes its next task",
            );
        });
    }

    /// **A role stopped once its task is done finishes the tasks under way and takes no new one**: each
    /// run ends where it comes to take its next task, and its seats stay empty with tasks still waiting.
    /// The headcount stays as it was, and taken up again the role fills its seats back up to it.
    #[test]
    fn a_role_held_takes_no_new_task_until_it_is_taken_up_again() {
        use crate::ops::automation_run::{launch_headcounts, launches_to_top_up, seats, top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            crate::ops::test_support::room_for(tx, automation.id, 2);
            for title in ["a", "b", "c", "d"] {
                ours(tx, title, &automation, None);
            }
            let (launch, role, [(first, first_work), (second, second_work)]) = two_at_work(tx, &automation);

            let held = automation_stop::hold_launch_role(tx, launch, role).expect("stop the role");
            assert!(held.role.held);
            assert_eq!(held.role.headcount, 2, "the headcount stays as it was");
            assert!(held.runs.is_empty(), "both runs are at work, and go on");
            let counted = launch_headcounts(tx.conn(), launch).expect("read");
            assert!(counted[0].held);
            assert_eq!(counted[0].headcount, 2);
            assert_eq!(counted[0].under_way, vec![first.id, second.id], "no task under way is stopped");
            let again = automation_stop::hold_launch_role(tx, launch, role).expect("again");
            assert_eq!(
                (again.role.id, again.role.held, again.runs.len()),
                (held.role.id, true, 0),
                "a role already stopped is answered as it stands",
            );

            for (run, work) in [(&first, &first_work), (&second, &second_work)] {
                let Next::Step(take) = round_finished(tx, run.id, work) else { panic!("back round to the take") };
                let Opened::LetGo { run: let_go } = open(tx, run.id, take.id, Some(&[])).expect("open the take")
                else {
                    panic!("a run of a stopped role takes no new task");
                };
                assert_eq!(let_go.status, AutomationRunStatus::Completed);
            }
            assert!(seats(tx.conn(), &automation).expect("seats").iter().all(|s| !s.free()), "no seat is free");
            assert!(
                launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty(),
                "tasks still wait, and the role takes none",
            );
            assert_eq!(
                read::automation_launch(tx.conn(), launch).expect("read").expect("launch").status,
                crate::model::AutomationLaunchStatus::Running,
                "the launch stays open",
            );

            let released = automation_stop::release_launch_role(tx, launch, role).expect("take it up again");
            assert!(!released.held);
            assert_eq!(released.headcount, 2);
            assert!(!launch_headcounts(tx.conn(), launch).expect("read")[0].held);
            assert_eq!(launches_to_top_up(tx.conn(), &[launch]).expect("ask"), vec![launch]);
            let third = top_up(tx, launch, &launcher()).expect("top up").expect("a run in the seat");
            assert!(matches!(open_entry(tx, &third), Opened::Carried { .. }), "it takes a task again");
            assert!(top_up(tx, launch, &launcher()).expect("top up").is_some(), "back up to its headcount");
        });
    }

    /// **A role is stopped or taken up only while its launch runs**: not while the launch is paused as a
    /// whole, nor once it has ended, nor for a role the automation does not hold. Picking a launch up
    /// again leaves a role stopped as it was.
    #[test]
    fn a_role_is_held_only_while_its_launch_runs() {
        use crate::ops::automation_run::launch_headcounts;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            ours(tx, "a", &automation, None);
            let run = launched(tx, &automation);
            let launch = run.launch_id.expect("a launch");
            open_entry(tx, &run);
            let role = read::automation_roles_of(tx.conn(), automation.id).expect("roles")[0].id;

            assert!(matches!(
                automation_stop::hold_launch_role(tx, launch, role + 1000),
                Err(crate::error::Error::NotFound(_))
            ));
            automation_stop::hold_launch_role(tx, launch, role).expect("stop the role");
            automation_stop::pause_launch(tx, launch).expect("pause the launch");
            assert!(matches!(
                automation_stop::release_launch_role(tx, launch, role),
                Err(crate::error::Error::Invalid(_))
            ));
            assert!(matches!(
                automation_stop::hold_launch_role(tx, launch, role),
                Err(crate::error::Error::Invalid(_))
            ));
            automation_stop::resume_launch(tx, launch, None, nothing_asked(), None).expect("pick it up");
            assert!(launch_headcounts(tx.conn(), launch).expect("read")[0].held, "the role stays stopped");

            automation_stop::stop_launch(tx, launch).expect("stop");
            assert!(!launch_headcounts(tx.conn(), launch).expect("read")[0].held);
            assert!(matches!(
                automation_stop::release_launch_role(tx, launch, role),
                Err(crate::error::Error::Invalid(_))
            ));
        });
    }

    /// **How many are in a role for a launch is from 1 to 10** (`AMB-D-1124`), changed only on a launch
    /// under way and only for a role of the newest saved version. Once the launch ends every role reads
    /// as its definition again, and the next launch copies the definition as it then stands.
    #[test]
    fn a_launch_headcount_is_held_to_its_range_and_ends_with_the_launch() {
        use crate::ops::automation_run::{launch_headcount_set, launch_headcounts};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            ours(tx, "a", &automation, None);
            let run = launched(tx, &automation);
            let launch = run.launch_id.expect("a launch");
            let role = read::automation_roles_of(tx.conn(), automation.id).expect("roles")[0].id;

            for wrong in [0, crate::model::MAX_ROLE_HEADCOUNT + 1] {
                assert!(
                    matches!(launch_headcount_set(tx, launch, role, wrong), Err(crate::error::Error::Invalid(_))),
                    "a headcount of {wrong}",
                );
            }
            let most = launch_headcount_set(tx, launch, role, crate::model::MAX_ROLE_HEADCOUNT).expect("ten");
            assert_eq!(most.headcount, crate::model::MAX_ROLE_HEADCOUNT);
            assert!(
                matches!(launch_headcount_set(tx, launch, role + 1000, 2), Err(crate::error::Error::NotFound(_))),
                "a role that does not exist (`AMB-D-986`)",
            );

            automation::role_update(tx, role, automation::RoleChange { headcount: Some(3), ..Default::default() })
                .expect("change the definition");
            automation::version_add(tx, automation.id).expect("save it");
            assert_eq!(
                launch_headcounts(tx.conn(), launch).expect("read")[0].headcount,
                crate::model::MAX_ROLE_HEADCOUNT,
                "a saved headcount waits for the next launch",
            );

            automation_stop::stop_launch(tx, launch).expect("stop");
            let ended = launch_headcounts(tx.conn(), launch).expect("read");
            assert_eq!((ended[0].defined, ended[0].headcount), (3, 3), "back at the definition");
            assert!(matches!(launch_headcount_set(tx, launch, role, 2), Err(crate::error::Error::Invalid(_))));

            ours(tx, "b", &automation, None);
            let next = launched(tx, &automation).launch_id.expect("a launch of its own");
            assert_eq!(launch_headcounts(tx.conn(), next).expect("read")[0].headcount, 3, "copied afresh");
        });
    }

    /// **A launch with nothing to take is not topped up**, and it stays open past its last run until a
    /// person stops it: a task that turns up later starts a run in it.
    #[test]
    fn a_launch_stays_open_past_its_last_run_until_a_person_stops_it() {
        use crate::ops::automation_run::{launches_to_top_up, top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = picture(tx, project);
            crate::ops::test_support::room_for(tx, automation.id, 2);

            let run = launched(tx, &automation);
            let launch = run.launch_id.expect("a launch");
            assert!(launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty());
            open_entry(tx, &run);
            assert_eq!(run_of(tx, run.id).status, AutomationRunStatus::Completed);
            let open = launch_of(tx, launch);
            assert_eq!(open.status, crate::model::AutomationLaunchStatus::Running);
            assert!(open.ended_at.is_none());

            ours(tx, "later", &automation, None);
            let next = top_up(tx, launch, &launcher()).expect("top up").expect("a run for the task");
            assert_eq!(next.launch_id, Some(launch), "it starts in the launch left open");

            let stopped = automation_stop::stop_launch(tx, launch).expect("stop");
            assert_eq!(stopped.launch.status, crate::model::AutomationLaunchStatus::Ended);
            assert!(read::automation_launch_standing(tx.conn(), automation.id).expect("read").is_none());
            let after = launched(tx, &automation);
            assert_ne!(after.launch_id, Some(launch), "a press after it opens a launch of its own");
        });
    }

    /// **A launch whose runs found no task stays standing**, and the run started once one turns up joins
    /// it ([`crate::ops::automation_run::top_up`]). It stays standing once that run ends too.
    #[test]
    fn a_waiting_launch_stands_and_the_run_started_on_a_task_joins_it() {
        use crate::ops::automation_run::top_up;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;

            let run = launched(tx, &automation);
            let launch = run.launch_id.expect("a launch");
            assert!(matches!(open_entry(tx, &run), Opened::LeftToWait { .. }));
            assert_eq!(launch_of(tx, launch).status, crate::model::AutomationLaunchStatus::Running);

            ours(tx, "new", &automation, None);
            let next = top_up(tx, launch, &launcher()).expect("top up").expect("a run");
            assert_eq!(next.launch_id, Some(launch), "it joins the launch standing");
            open_entry(tx, &next);
            automation_stop::stop(tx, next.id, automation_stop::Ending::Canceled).expect("stop");
            assert_eq!(launch_of(tx, launch).status, crate::model::AutomationLaunchStatus::Running);
        });
    }

    /// Launch it, let the entry take a task, and fail the run there.
    fn failed_in_a_launch(tx: &WriteTx<'_>, automation: &Automation) -> (AutomationRun, i64) {
        let run = launched(tx, automation);
        let launch = run.launch_id.expect("a launch");
        open_entry(tx, &run);
        let failed = automation_stop::Ending::Failed(AutomationStoppedReason::Crashed);
        automation_stop::stop(tx, run.id, failed).expect("fail it");
        (run, launch)
    }

    /// **A failed round filed to a role reaches its launch as that role's incident** — the task, the role
    /// it was filed to, the round and role it came from, and when. The launch stays running for it, the
    /// run started for it takes it, and it waits no longer. Another launch has none.
    #[test]
    fn a_round_filed_to_a_role_reaches_its_launch_as_that_roles_incident() {
        use crate::model::{AutomationLaunchStatus, FailuresTo};
        use crate::ops::automation_run::{incidents_waiting_in_launch, top_up, LaunchIncident};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [(builder, _), (reviewer, _)]) = by_role(tx, project);
            let to = Some(Some(FailuresTo::Role { role_id: reviewer }));
            automation::update(tx, automation.id, None, None, None, to, None).expect("to the reviewer");
            let built = for_ai(tx, "to build", project, None);
            give_to(tx, built, builder);

            let (run, launch) = failed_in_a_launch(tx, &automation);
            let round = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("the round");
            let filed = round.incident_task_id.expect("the round was filed");
            assert_eq!(
                incidents_waiting_in_launch(tx.conn(), launch).expect("incidents"),
                vec![LaunchIncident {
                    task_id: filed,
                    role_id: reviewer,
                    run_id: run.id,
                    run_task_id: round.id,
                    from_role_id: Some(builder),
                    filed_at: round.ended_at,
                }],
            );
            assert_eq!(launch_of(tx, launch).status, AutomationLaunchStatus::Running, "the launch stays for it");

            let (other, _) = picture(tx, project);
            let elsewhere = launched(tx, &other).launch_id.expect("a launch");
            assert!(incidents_waiting_in_launch(tx.conn(), elsewhere).expect("incidents").is_empty());

            let next = top_up(tx, launch, &launcher()).expect("top up").expect("a run for the incident");
            open_entry(tx, &next);
            assert_eq!(status(tx, filed), TaskStatus::InProgress, "the reviewer's round takes it");
            assert!(incidents_waiting_in_launch(tx.conn(), launch).expect("incidents").is_empty());
        });
    }

    /// **A failed round filed to a person is no incident of a role**, and the launch stays open as any
    /// launch does.
    #[test]
    fn a_round_filed_to_a_person_is_no_incident_of_a_role() {
        use crate::ops::automation_run::incidents_waiting_in_launch;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, [(builder, _), _]) = by_role(tx, project);
            let built = for_ai(tx, "to build", project, None);
            give_to(tx, built, builder);

            let (run, launch) = failed_in_a_launch(tx, &automation);
            let round = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("the round");
            assert!(round.incident_task_id.is_some(), "filed to the person");
            assert!(incidents_waiting_in_launch(tx.conn(), launch).expect("incidents").is_empty());
            assert_eq!(launch_of(tx, launch).status, crate::model::AutomationLaunchStatus::Running);
        });
    }

    /// **A task whose round failed stands stopped in its launch until the task its failure was filed as
    /// closes** — filed to a person or to a role alike. Another launch has nothing that needs a hand.
    #[test]
    fn a_task_whose_round_failed_stands_stopped_until_its_failure_closes() {
        use crate::model::FailuresTo;
        use crate::ops::automation_run::{attention_in_launch, AttentionKind, LaunchAttention};
        for to_a_role in [false, true] {
            with_tx(|tx| {
                let project = mk_project(tx, "amenbo");
                let (automation, [(builder, _), (reviewer, _)]) = by_role(tx, project);
                if to_a_role {
                    let to = Some(Some(FailuresTo::Role { role_id: reviewer }));
                    automation::update(tx, automation.id, None, None, None, to, None).expect("to the reviewer");
                }
                let built = for_ai(tx, "to build", project, None);
                give_to(tx, built, builder);

                let (run, launch) = failed_in_a_launch(tx, &automation);
                let round = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("the round");
                let filed = round.incident_task_id.expect("the round was filed");
                assert_eq!(
                    attention_in_launch(tx.conn(), launch).expect("attention"),
                    vec![LaunchAttention {
                        kind: AttentionKind::Stopped,
                        task_id: Some(built),
                        title: Some("to build".into()),
                        role_id: Some(builder),
                        run_id: run.id,
                        run_task_id: Some(round.id),
                        incident_task_id: Some(filed),
                        step_name: None,
                        since: round.ended_at,
                    }],
                    "filed to a role: {to_a_role}",
                );
                let state = crate::ops::automation_run::launch_state(tx.conn(), launch).expect("state");
                assert_eq!(state.attention.len(), 1, "the pane reads it in the same read");

                let (other, _) = picture(tx, project);
                let elsewhere = launched(tx, &other).launch_id.expect("a launch");
                assert!(attention_in_launch(tx.conn(), elsewhere).expect("attention").is_empty());

                task::set_status(tx, filed, TaskStatus::Done, ActorKind::Human).expect("close the failure");
                assert!(attention_in_launch(tx.conn(), launch).expect("attention").is_empty(), "no longer stopped");
            });
        }
    }

    /// Launch `automation` (made [`by_role`]) with its builder's step made one that may wait for a person,
    /// a task for the builder, and open that step. Answers the run, its launch, the opening, the task
    /// and the builder.
    fn waiting_on_a_person(
        tx: &WriteTx<'_>,
    ) -> (AutomationRun, i64, crate::ops::automation_step::Opening, i64, i64) {
        let project = mk_project(tx, "amenbo");
        let (automation, [(builder, work), _]) = by_role(tx, project);
        let action = read::automation_action(tx.conn(), work.action_id).expect("read").expect("the action");
        let step = action.entry_step_id.expect("an entry step");
        automation::step_update(tx, step, None, None, Some(true), None, None, None, None, None, None, None, None)
            .expect("make the step interactive");
        let automation = as_it_stands(tx, &automation);
        let built = for_ai(tx, "to build", project, None);
        give_to(tx, built, builder);

        let claude = ["claude".to_string()];
        let run = launched(tx, &automation);
        let launch = run.launch_id.expect("a launch");
        let Opened::Carried { next: Next::Step(def), .. } = open_entry(tx, &run) else {
            panic!("the take goes on to the work");
        };
        let Opened::Ready(opening) = open(tx, run.id, def.id, Some(&claude)).expect("open the work") else {
            panic!("the work opens a terminal");
        };
        (run, launch, *opening, built, builder)
    }

    /// **A run standing on an open step that may wait for a person is named in its launch**, with the
    /// task it is on and its role, until the step leaves.
    #[test]
    fn a_run_on_a_step_that_waits_for_a_person_is_named_until_the_step_leaves() {
        use crate::ops::automation_run::{attention_in_launch, AttentionKind, LaunchAttention};
        with_tx(|tx| {
            let (run, launch, opening, built, builder) = waiting_on_a_person(tx);
            let round = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("the round");
            assert_eq!(
                attention_in_launch(tx.conn(), launch).expect("attention"),
                vec![LaunchAttention {
                    kind: AttentionKind::AwaitsPerson,
                    task_id: Some(built),
                    title: Some("to build".into()),
                    role_id: Some(builder),
                    run_id: run.id,
                    run_task_id: Some(round.id),
                    incident_task_id: None,
                    step_name: Some(opening.run_def.name.clone()),
                    since: opening.run_step.started_at,
                }],
            );

            report_done(tx, &opening);
            assert!(attention_in_launch(tx.conn(), launch).expect("attention").is_empty(), "the step has left");
        });
    }

    /// **A run waiting for a person that is stopped waits for nobody**, and its launch is left with nothing
    /// that needs a hand.
    #[test]
    fn a_run_waiting_for_a_person_is_no_longer_named_once_stopped() {
        use crate::ops::automation_run::attention_in_launch;
        with_tx(|tx| {
            let (run, launch, _, _, _) = waiting_on_a_person(tx);
            assert_eq!(attention_in_launch(tx.conn(), launch).expect("attention").len(), 1);

            automation_stop::stop(tx, run.id, automation_stop::Ending::Canceled).expect("stop");
            assert!(attention_in_launch(tx.conn(), launch).expect("attention").is_empty());
        });
    }

    /// **A start of an automation whose round starts at a take that would wait opens the launch the run
    /// joins later**, and it stays open with no run in it: only stopping the launch ends it.
    #[test]
    fn a_start_that_waits_opens_a_launch_that_stays_open() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = picture(tx, project).0;
            let started = start(tx, automation.id, &launcher()).expect("start");
            assert!(matches!(started, Started::Waits(_)), "nothing to take, so it waits");
            let launch = read::automation_launch_standing(tx.conn(), automation.id)
                .expect("read")
                .expect("a launch stands with no run");
            assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty());
            assert_eq!(launch_of(tx, launch.id).status, crate::model::AutomationLaunchStatus::Running);
        });
    }

    /// **A launch is paused, picked up and stopped as one.** Pausing asks each of its runs and starts no
    /// run in it; a press joining it is refused while it is paused. Picking it up takes the asks back.
    /// Canceling is refused while a run is running. Stopping ends every run and the launch with them,
    /// each run handing its task back.
    #[test]
    fn a_launch_is_paused_picked_up_and_stopped_as_one() {
        use crate::ops::automation_run::{launches_to_top_up, top_up};
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = picture(tx, project);
            crate::ops::test_support::room_for(tx, automation.id, 3);
            let tasks: Vec<i64> = ["one", "two", "three"].iter().map(|t| ours(tx, t, &automation, None)).collect();

            let first = launched(tx, &automation);
            let launch = first.launch_id.expect("a launch");
            open_entry(tx, &first);
            let second = top_up(tx, launch, &launcher()).expect("top up").expect("a second run");
            open_entry(tx, &second);

            let paused = automation_stop::pause_launch(tx, launch).expect("pause");
            assert_eq!(paused.launch.status, crate::model::AutomationLaunchStatus::Paused);
            assert_eq!(paused.runs.len(), 2);
            assert!(paused.runs.iter().all(|run| run.pause_requested), "each run is asked");
            assert!(launches_to_top_up(tx.conn(), &[launch]).expect("ask").is_empty(), "no run is added");
            assert!(launched_or_refused(tx, &automation).is_err(), "a press does not join a paused launch");
            assert!(
                automation_stop::cancel_launch(tx, launch).is_err(),
                "its runs are still running, so it is not canceled",
            );

            let resumed =
                automation_stop::resume_launch(tx, launch, None, nothing_asked(), None).expect("pick it up");
            assert_eq!(resumed.launch.status, crate::model::AutomationLaunchStatus::Running);
            assert!(resumed.runs.iter().all(|run| !run.pause_requested), "the asks are taken back");

            let stopped = automation_stop::stop_launch(tx, launch).expect("stop");
            assert_eq!(stopped.launch.status, crate::model::AutomationLaunchStatus::Ended);
            assert!(stopped.runs.iter().all(|run| run.status == AutomationRunStatus::Canceled));
            assert!(read::automation_launch_run_ids_under_way(tx.conn(), launch).expect("runs").is_empty());
            let taken: Vec<TaskStatus> = tasks.iter().map(|t| status(tx, *t)).collect();
            assert!(taken.iter().all(|s| *s == TaskStatus::Todo), "every task is handed back: {taken:?}");
            assert!(automation_stop::pause_launch(tx, launch).is_err(), "an ended launch is not paused");
        });
    }

    fn launched_or_refused(tx: &WriteTx<'_>, automation: &Automation) -> crate::error::Result<AutomationRun> {
        let claude = ["claude".to_string()];
        let by = Launcher {
            default_agent: None,
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        launch(tx, automation.id, &by)
    }
}
