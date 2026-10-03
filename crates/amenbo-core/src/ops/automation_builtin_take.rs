//! **The built-in that takes a task** — find the task a run works next and reserve it (`AMB-D-964`).
//!
//! What a prompt used to be asked to do with `task list` and `step-take`, done here in the one
//! transaction the step is opened in: list the tasks the filter matches, in the order it asks for, and
//! reserve them from the top until one goes through. A task somebody else reserved first is not a
//! failure — the next one is tried.
//!
//! **Not started and ready are always asked for.** Only a `todo` task whose premises all hold can be
//! reserved, so the filter's own `status:` and `ready:` are dropped and those two put in their place:
//! a setting that asked for anything else would be asking for a task the reservation refuses.
//!
//! **Left unanswered, the filter is the tasks given to the AI** (`assignee:me-ai`). A run that stops to
//! call a person hands its task to that person (`AMB-D-966`); a filter that did not ask for the AI's
//! would take it straight back.
//!
//! It leaves by [`TAKEN`] with the task it reserved, or by [`NONE_TO_TAKE`] with nothing —
//! the step that went looking and found none, which owes no report and keeps no stretch
//! ([`super::automation_report::done`]).
//!
//! **Or it waits for one** (`AMB-D-969`), where [`WHEN_NONE`] is answered [`WAIT`]. A run or a session
//! closing a task can make another one ready, and so can a new one being filed, so there is no point at
//! which none will ever turn up: it waits until a person pauses or stops the run. Left unanswered it
//! does not wait, so a run nobody chose to keep open does not stay open. What is keeping it waiting
//! is counted apart from that look (`AMB-D-999`): the tasks the filter matches that are not started
//! and not ready, by what stops them ([`held_back`]).

use rusqlite::Connection;

use crate::error::Result;
use crate::model::{AutomationCfgKind, AutomationPortKind, AutomationRun, RunDefCfg};
use crate::ops::automation_builtin::{
    answer, Builtin, BuiltinExit, BuiltinPort, BuiltinSetting, Carried, Carry, HeldBack, HeldByRecord,
    HeldByValue, Waits, Work,
};
use crate::ops::automation_report;
use crate::run_wording::builtin as say;
use crate::ops::automation_step::{taskfilter_expr, taskfilter_sort, TASKFILTER_SORT_DEFAULT};
use crate::query::{self, ListParams};
use crate::reach::Reach;
use crate::store_engine::read;

/// The way out it leaves by once it has reserved a task.
pub const TAKEN: &str = "着手した";
/// The way out it leaves by when no task the filter matches could be reserved.
pub const NONE_TO_TAKE: &str = "着手できるタスクが無い";
/// The setting that says which tasks it takes, and in what order.
pub const FILTER: &str = "絞り込み";
/// The output the reserved task is handed on through.
pub const TASK: &str = "タスク";
/// The setting that says what it does when there is no task to take.
pub const WHEN_NONE: &str = "着手できるタスクが無いとき";
/// The choice on [`WHEN_NONE`] that waits for one.
pub const WAIT: &str = "着手できるタスクが出るまで待つ";
/// The choice on [`WHEN_NONE`] that leaves by [`NONE_TO_TAKE`] — also what it does left unanswered.
pub const GO_ON: &str = "待たずに出口「着手できるタスクが無い」へ進む";

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
    version: 1,
    name: "タスクに着手する",
    does: "絞り込みに合い、着手できる未着手のタスクを並び順どおりに探し、先頭から予約して進行中にする",
    steps: &[
        "設定の絞り込みに合うタスクのうち、未着手で着手できるものを、並び順どおりに探す。絞り込みが空なら、AI が担当のタスクを探す",
        "先頭のタスクから予約して進行中にし、「着手した」から出てそのタスクを渡す。ほかで先に予約されたタスクは飛ばして、次を試す",
        "1件も予約できなければ「着手できるタスクが無い」から出る。設定で待つと決めてあれば、着手できるタスクが出るまで待つ",
    ],
    halts: &[],
    settings: &[
        BuiltinSetting { name: FILTER, kind: AutomationCfgKind::TaskFilter, required: false, options: None },
        BuiltinSetting {
            name: WHEN_NONE,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["待たずに出口「着手できるタスクが無い」へ進む","着手できるタスクが出るまで待つ"]"#),
        },
    ],
    ins: &[],
    exits: &[
        BuiltinExit {
            name: TAKEN,
            outs: &[BuiltinPort { name: TASK, kind: AutomationPortKind::TaskTake, required: true }],
        },
        BuiltinExit { name: NONE_TO_TAKE, outs: &[] },
    ],
    waits: Some(Waits {
        setting: WHEN_NONE,
        answer: WAIT,
        instead_of: NONE_TO_TAKE,
        turned_up,
        looks_for: |cfg| expression(answer(cfg, FILTER)),
        held_back,
    }),
    chooses: None,
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
    Ok(Carried { exit: NONE_TO_TAKE, report: say(carry.tx.language(), "noneToTake", &[("filter", &expr)]) })
}

/// **Whether there is a task to take now** — the one question asked while it waits. One row is read,
/// however many the filter matches, and none is counted or sorted.
fn turned_up(conn: &Connection, run: &AutomationRun, cfg: &[RunDefCfg]) -> Result<bool> {
    query::any(conn, Reach::binding(run.project_id), &expression(answer(cfg, FILTER)))
}

/// **What is keeping it waiting** (`AMB-D-999`): every task the filter matches that is not started and
/// not ready, counted by what stops it. The reasons are the five the list already carries on each row
/// (`view::is_ready`), so a reason counted here is one the reservation would refuse by.
fn held_back(conn: &Connection, run: &AutomationRun, cfg: &[RunDefCfg]) -> Result<HeldBack> {
    let expr = with_asked(narrowing(answer(cfg, FILTER)), HELD);
    let mut held = HeldBack::default();
    let (mut blockers, mut decisions): (Counts, Counts) = (Vec::new(), Vec::new());
    let mut offset = 0;
    loop {
        let page = query::list(
            conn,
            Reach::binding(run.project_id),
            ListParams {
                filter_expr: Some(expr.clone()),
                sort: TASKFILTER_SORT_DEFAULT.to_string(),
                limit: Some(COUNT_PAGE),
                offset: Some(offset),
                ..Default::default()
            },
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
            if let Some(day) = task.not_started_until {
                held.not_started += 1;
                held.first_start = Some(held.first_start.map_or(day, |first| first.min(day)));
            }
            if task.draft {
                held.drafts += 1;
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

/// One page of the tasks the filter answered with lists, in the order it asks for.
fn search(
    conn: &Connection,
    run: &AutomationRun,
    answer: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<query::TaskListResult> {
    let sort = answer.map(taskfilter_sort).unwrap_or_else(|| TASKFILTER_SORT_DEFAULT.to_string());
    query::list(
        conn,
        Reach::binding(run.project_id),
        ListParams {
            filter_expr: Some(expression(answer)),
            sort,
            limit: Some(limit),
            offset: Some(offset),
            ..Default::default()
        },
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
        AutomationRun, AutomationRunStatus, Priority, TaskStatus,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{
        check, held_back as held_back_of, is_waiting, launch, launch_past_the_task_checks, next_def,
        nothing_asked, Launcher, Unmet, Waiting,
    };
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::ops::automation_stop;
    use crate::ops::task::{self, TaskPatch};
    use crate::ops::test_support::{mk_out, mk_placed, mk_project, mk_task_in, way_out, with_tx};
    use crate::store_engine::{read, WriteTx};

    /// An automation whose entry is the built-in. What it takes goes on to an agent's step, since a run
    /// does not end with its task still in progress (`AMB-D-967`); nothing to take closes the run.
    fn picture(tx: &WriteTx<'_>, project: i64) -> (Automation, AutomationPlacement) {
        let automation =
            automation::add(tx, project, NewAutomation { name: "take".into(), ..Default::default() })
                .expect("automation");
        let written = action(tx, "take_task").expect("the built-in's action");
        let spot = automation::placement_add(tx, automation.id, written.id).expect("place it");
        let on = AutomationPictureOwner::Automation;
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        automation::edge_add(tx, on, spot.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("onward");
        automation::edge_add(tx, on, spot.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("closes");
        crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
        let automation = automation::set_entry(tx, automation.id, Some(spot.id)).expect("entry");
        (automation, spot)
    }

    /// Launch it and open the entry step, which the built-in carries out on the spot.
    fn carried(tx: &WriteTx<'_>, automation: &Automation) -> (AutomationRun, i64, Next) {
        let claude = ["claude".to_string()];
        let by = Launcher {
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        let run = launch(tx, automation.id, &by).expect("launch");
        let entry = read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.builtin.as_deref() == Some("take_task"))
            .expect("the built-in's copy");
        match open(tx, run.id, entry.id, Some(&[])).expect("open") {
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

            let low = for_ai(tx, "low", project, Some(Priority::Low));
            let high = for_ai(tx, "high", project, Some(Priority::High));
            let people = mk_task_in(tx, "a person's", Some(project));
            task::update(tx, people, TaskPatch { priority: Some(Priority::High), ..Default::default() })
                .expect("priority");
            let running = for_ai(tx, "running", project, Some(Priority::High));
            task::set_status(tx, running, TaskStatus::InProgress, crate::model::ActorKind::Ai).expect("reserve");
            let waiting = for_ai(tx, "waiting", project, Some(Priority::High));
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
            let running = for_ai(tx, "running", project, None);
            task::set_status(tx, running, TaskStatus::InProgress, crate::model::ActorKind::Ai).expect("reserve");
            let people = mk_task_in(tx, "a person's", Some(project));
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

    /// **Nothing to take leaves by [`NONE_TO_TAKE`]** — with no task touched and no stretch kept.
    #[test]
    fn with_nothing_to_take_it_leaves_by_the_way_out_that_says_so() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let (automation, _) = picture(tx, project);
            let people = mk_task_in(tx, "a person's", Some(project));

            let (run, run_step_id, next) = carried(tx, &automation);
            assert!(matches!(next, Next::Closed(_)), "the line after it closes the run: {next:?}");
            assert_eq!(status(tx, people), TaskStatus::Todo);
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, NONE_TO_TAKE));
            assert_eq!(ran.run_task_id, None, "it went looking and found none");
            let run = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(run.status, AutomationRunStatus::Completed);
        });
    }

    /// The same picture, set to wait: nothing follows [`NONE_TO_TAKE`], since it is never taken.
    fn waiting_picture(tx: &WriteTx<'_>, project: i64) -> Automation {
        let automation =
            automation::add(tx, project, NewAutomation { name: "wait".into(), ..Default::default() })
                .expect("automation");
        let written = action(tx, "take_task").expect("the built-in's action");
        let spot = automation::placement_add(tx, automation.id, written.id).expect("place it");
        let on = AutomationPictureOwner::Automation;
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        automation::edge_add(tx, on, spot.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("onward");
        crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
        automation::cfg_set(tx, spot.id, WHEN_NONE, Some(&format!("\"{WAIT}\""))).expect("wait");
        automation::set_entry(tx, automation.id, Some(spot.id)).expect("entry")
    }

    fn launched(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let claude = ["claude".to_string()];
        let by = Launcher {
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

    /// **Set to wait, with nothing to take, it writes nothing and the run stands before it** — opened
    /// again on the next look, and the task that has turned up since is the one it takes.
    #[test]
    fn set_to_wait_it_stands_until_a_task_turns_up_and_then_takes_it() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = waiting_picture(tx, project);
            let startable = ["claude".to_string()];
            let unmet = check(tx.conn(), automation.id, Some(&startable), nothing_asked()).expect("check");
            assert!(unmet.is_empty(), "nothing has to follow the way out it never takes: {unmet:?}");

            let run = launched(tx, &automation);
            for _ in 0..2 {
                match open_entry(tx, &run) {
                    Opened::Waiting { run: still } => assert_eq!(still.status, AutomationRunStatus::Running),
                    other => panic!("with nothing to take it waits, not {other:?}"),
                }
            }
            assert!(read::automation_run_steps_of(tx.conn(), run.id).expect("steps").is_empty(), "nothing written");
            assert!(read::automation_run_task_last(tx.conn(), run.id).expect("read").is_none(), "no stretch");
            assert!(is_waiting(tx.conn(), run.id).expect("waiting"));

            let turned_up = for_ai(tx, "new", project, None);
            let run_step_id = match open_entry(tx, &run) {
                Opened::Carried { run_step_id, .. } => run_step_id,
                other => panic!("the task that turned up is taken, not {other:?}"),
            };
            assert_eq!(status(tx, turned_up), TaskStatus::InProgress);
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, TAKEN));
            assert!(!is_waiting(tx.conn(), run.id).expect("waiting"));
        });
    }

    /// **While it waits, what keeps it waiting is counted** (`AMB-D-999`): the tasks the filter matches
    /// that cannot be taken, by what stops them — a task stopped twice counted under both and once in
    /// the whole. A run that is not waiting has nothing to say.
    #[test]
    fn while_it_waits_the_tasks_it_cannot_take_are_counted_by_what_stops_them() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = waiting_picture(tx, project);
            // A person's task, which the filter does not take, is what two of the AI's wait on.
            let first = mk_task_in(tx, "the part before", Some(project));
            let after_a = for_ai(tx, "after it, a", project, None);
            let after_b = for_ai(tx, "after it, b", project, None);
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
            let later = for_ai(tx, "later", project, None);
            let day = crate::time::today() + chrono::Days::new(30);
            task::update(tx, later, TaskPatch { start_on: Some(day), ..Default::default() }).expect("start");

            let run = launched(tx, &automation);
            assert!(matches!(open_entry(tx, &run), Opened::Waiting { .. }));
            let held = held_back_of(tx.conn(), run.id).expect("held back").expect("it is waiting");
            assert_eq!(held.tasks, 3, "each task once, however many things stop it");
            assert_eq!(
                held.blockers,
                vec![HeldByRecord { id: first, title: "the part before".into(), count: 2 }],
            );
            assert_eq!(
                held.decisions,
                vec![HeldByRecord { id: premise.id, title: "the premise".into(), count: 1 }],
            );
            assert_eq!((held.not_started, held.first_start), (1, Some(day)));
            assert!(held.values.is_empty());
            assert_eq!(held.drafts, 0);

            for_ai(tx, "one it can take", project, None);
            assert_eq!(held_back_of(tx.conn(), run.id).expect("held back"), None, "no longer waiting");
        });
    }

    /// **A pause takes hold while it waits**, since a waiting step never reports; picked up again, the
    /// run starts at its entry as though it had just been launched.
    #[test]
    fn a_pause_takes_hold_while_it_waits_and_a_resume_starts_it_again() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = waiting_picture(tx, project);
            let run = launched(tx, &automation);
            assert!(matches!(open_entry(tx, &run), Opened::Waiting { .. }));

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

            let resumed = automation_stop::resume(tx, run.id).expect("resume");
            assert_eq!(resumed.run.pause_kind, None, "a running run is stopped at neither");
            assert_eq!(resumed.next.builtin.as_deref(), Some("take_task"), "back at its entry");
            assert!(matches!(open_entry(tx, &run), Opened::Waiting { .. }));
        });
    }

    /// The picture a run goes round: take a task, work on it, close it, and back to take the next —
    /// nothing to take closes the run.
    fn round_picture(tx: &WriteTx<'_>, project: i64) -> Automation {
        let automation =
            automation::add(tx, project, NewAutomation { name: "round".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place take");
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        let close = automation::placement_add(tx, automation.id, action(tx, "close_task").expect("close").id)
            .expect("place close");
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        automation::edge_add(tx, on, work.id, None, EdgeTarget::Go(close.id), None).expect("work → close");
        automation::edge_add(tx, on, close.id, None, EdgeTarget::Go(take.id), None).expect("close → take");
        automation::set_entry(tx, automation.id, Some(take.id)).expect("entry")
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
    fn paused_after_the_first(tx: &WriteTx<'_>, project: i64, automation: &Automation) -> (AutomationRun, i64, Opened) {
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

        let asked = automation_stop::pause_before_next_task(tx, project).expect("ask");
        assert_eq!(asked.iter().map(|r| r.id).collect::<Vec<_>>(), vec![run.id]);
        assert!(asked[0].pause_before_next_task);
        assert_eq!(asked[0].status, AutomationRunStatus::Running, "the task under way is not cut off");
        assert!(
            automation_stop::pause_before_next_task(tx, project).expect("ask again").is_empty(),
            "a run already asked is not asked twice",
        );

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
            let a = for_ai(tx, "a", project, Some(Priority::High));
            let b = for_ai(tx, "b", project, Some(Priority::Low));

            let (run, first, opened) = paused_after_the_first(tx, project, &automation);
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
            assert!(!automation_stop::pauses_before_next_task(tx.conn(), &paused).expect("judge"));

            let resumed = automation_stop::resume(tx, run.id).expect("resume");
            assert_eq!(resumed.next.builtin.as_deref(), Some("take_task"), "back at the take");
            assert_eq!(resumed.run.pause_kind, None);
            assert!(matches!(
                open(tx, run.id, resumed.next.id, Some(&[])).expect("open"),
                Opened::Carried { next: Next::Step(_), .. }
            ));
            assert_eq!(status(tx, b), TaskStatus::InProgress, "and the next task is taken");
        });
    }

    /// **Canceled there, a run paused before its next task hands nothing back** — it holds no task.
    #[test]
    fn a_run_paused_before_its_next_task_is_canceled_with_nothing_to_hand_back() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = round_picture(tx, project);
            for_ai(tx, "a", project, Some(Priority::High));
            let b = for_ai(tx, "b", project, Some(Priority::Low));

            let (run, first, opened) = paused_after_the_first(tx, project, &automation);
            assert!(matches!(opened, Opened::Waiting { .. }));
            let ended = automation_stop::cancel(tx, run.id).expect("cancel");
            assert_eq!(ended.run.status, AutomationRunStatus::Canceled);
            assert_eq!(ended.run.pause_kind, None, "a run that is over is paused at neither");
            assert_eq!(status(tx, first), TaskStatus::Done, "the task it finished stays finished");
            assert_eq!(status(tx, b), TaskStatus::Todo, "and the next was never taken");
        });
    }

    /// **A run waiting for a task to turn up pauses as soon as it is asked** (`AMB-D-1009`): it holds
    /// no task, so nothing waits for its next look. Picked up again, it is back at the take.
    #[test]
    fn a_run_waiting_for_a_task_is_paused_as_soon_as_it_is_asked() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = waiting_picture(tx, project);
            let run = launched(tx, &automation);
            assert!(matches!(open_entry(tx, &run), Opened::Waiting { .. }));

            let asked = automation_stop::pause_before_next_task(tx, project).expect("ask");
            assert_eq!(asked.len(), 1);
            assert_eq!(asked[0].status, AutomationRunStatus::Paused, "paused there and then");
            let paused = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(paused.status, AutomationRunStatus::Paused);
            assert!(!paused.pause_before_next_task, "nothing is left asked");
            assert_eq!(paused.pause_kind, Some(crate::model::AutomationPauseKind::BeforeNextTask));
            assert!(read::automation_run_steps_of(tx.conn(), run.id).expect("steps").is_empty(), "nothing taken");

            let resumed = automation_stop::resume(tx, run.id).expect("resume");
            assert_eq!(resumed.next.builtin.as_deref(), Some("take_task"), "back at the take");
        });
    }

    /// **A run that takes no tasks is not asked** (`AMB-D-1009`): it runs on to its end, and so does a
    /// run of another project, or one already asked to pause at the end of its action.
    #[test]
    fn only_a_running_run_of_the_project_that_takes_tasks_is_asked() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let elsewhere = mk_project(tx, "other");
            let taskless =
                automation::add(tx, project, NewAutomation { name: "no tasks".into(), ..Default::default() })
                    .expect("automation");
            // A run has to start at a step that takes a task, so the entry here is one of a person's own
            // that says it may hand one on — launched past the checks that refuse such a step.
            let (work_action, work) = mk_placed(tx, &taskless, "work", "work on it", "claude");
            mk_out(tx, &work_action, None, "task", AutomationPortKind::TaskTake, false);
            automation::edge_add(tx, AutomationPictureOwner::Automation, work.id, None, EdgeTarget::Done, None)
                .expect("done");
            let taskless = automation::set_entry(tx, taskless.id, Some(work.id)).expect("entry");
            let claude = ["claude".to_string()];
            let by = Launcher {
                startable: Some(&claude),
                models: nothing_asked(),
                workspace_open: Some(true),
                by: Some(ActorKind::Ai),
            };
            let plain = launch_past_the_task_checks(tx, taskless.id, &by).expect("launch");
            let pausing = launched(tx, &waiting_picture(tx, project));
            automation_stop::pause(tx, pausing.id).expect("pause");
            let other = launched(tx, &waiting_picture(tx, elsewhere));

            assert!(automation_stop::pause_before_next_task(tx, project).expect("ask").is_empty());
            for run in [&plain, &pausing, &other] {
                let run = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
                assert!(!run.pause_before_next_task, "run {} is not asked", run.id);
            }

            let Waiting::Step(entry) = next_def(tx.conn(), plain.id).expect("next") else {
                panic!("the run stands before its entry");
            };
            let Opened::Ready(opening) = open(tx, plain.id, entry.id, Some(&claude)).expect("open") else {
                panic!("the work opens a terminal");
            };
            let Next::Closed(ended) = report_done(tx, &opening) else { panic!("the run ends") };
            assert_eq!(ended.run.status, AutomationRunStatus::Completed, "it ran to its end");
        });
    }

    /// **Left unanswered, or answered not to wait, it does not** — and then the way out that says
    /// there was nothing to take is one the launch check asks a line of. The reason carries the
    /// built-in's key, so a screen can write its words in its own language.
    #[test]
    fn not_set_to_wait_the_way_out_for_nothing_to_take_needs_a_line() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation = waiting_picture(tx, project);
            let spot = automation.entry_placement_id.expect("entry");
            let startable = ["claude".to_string()];
            for answer in [None, Some(format!("\"{GO_ON}\""))] {
                automation::cfg_set(tx, spot, WHEN_NONE, answer.as_deref()).expect("answer");
                let unmet = check(tx.conn(), automation.id, Some(&startable), nothing_asked()).expect("check");
                let open = unmet
                    .iter()
                    .find(|u| matches!(u, Unmet::OpenExit { exit: e, .. } if e == NONE_TO_TAKE))
                    .unwrap_or_else(|| panic!("{answer:?}: {unmet:?}"));
                assert!(
                    open.msg().fields().iter().any(|field| field == ("builtin", TAKE_TASK.key)),
                    "{open:?}",
                );
            }
        });
    }

    /// The choices written out on the setting are the two the code reads, the one it does unanswered
    /// first.
    #[test]
    fn the_choices_offered_are_the_ones_it_reads() {
        let options = TAKE_TASK.settings.iter().find(|s| s.name == WHEN_NONE).and_then(|s| s.options);
        let offered: Vec<String> = serde_json::from_str(options.expect("a choice list")).expect("JSON");
        assert_eq!(offered, vec![GO_ON, WAIT]);
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
}
