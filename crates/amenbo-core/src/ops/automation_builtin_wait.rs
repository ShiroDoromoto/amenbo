//! **The built-in that waits** — the time it is set to, and then on by [`DONE_EXIT`] (`AMB-D-983`).
//!
//! **Its step is under way for as long as it waits** ([`Work::Holds`]). Opening it writes the execution
//! and carries nothing out, so the run stands on a running step, as it does while an agent works one.
//! When the wait ends is read off that execution — the moment it was opened, plus the time the settings
//! add up to ([`super::automation_builtin::held_until`]) — rather than kept anywhere of its own: both are
//! written once and never change, so no timer or thread is started for it either.
//!
//! **What ends it is the thread that keeps runs going**, which asks whether the time has come
//! ([`super::automation_builtin::due`]) on the look it already takes at a run with a step under way, and
//! reports the step done ([`super::automation_builtin::time_up`]). From that report on it is the road
//! an agent's report takes: a pause asked for while it waited takes hold as it ends, a stop ends it at
//! once, and a run caught `running` when the app closed is stopped at the next start
//! ([`super::automation_stop::sweep`]).
//!
//! **Not the waiting a built-in set to wait does** ([`super::automation_builtin::Waits`]). A step that
//! waits there has not been opened, so a pause takes hold at once — this one would pause unlike any
//! other step.

use chrono::Duration;

use crate::error::{Error, Result};
use crate::model::{AutomationCfgKind, RunDefCfg, DONE_EXIT};
use crate::ops::automation_builtin::{answer, Builtin, BuiltinExit, BuiltinSetting, Work};

/// The settings that add up to how long it waits. Each left unanswered counts as none.
pub const HOURS: &str = "時間";
pub const MINUTES: &str = "分";
pub const SECONDS: &str = "秒";

pub(super) const WAIT: Builtin = Builtin {
    key: "wait",
    name: "待つ",
    does: "設定した時間だけ待ってから、「完了」から出る",
    settings: &[
        BuiltinSetting { name: HOURS, kind: AutomationCfgKind::Number, required: false, options: None },
        BuiltinSetting { name: MINUTES, kind: AutomationCfgKind::Number, required: false, options: None },
        BuiltinSetting { name: SECONDS, kind: AutomationCfgKind::Number, required: false, options: None },
    ],
    ins: &[],
    exits: &[BuiltinExit { name: DONE_EXIT, outs: &[] }],
    waits: None,
    chooses: None,
    work: Work::Holds(how_long),
};

/// **How long it waits**, from the three settings. An answer that is not a whole number of zero or more
/// is refused, and the step leaves by the error way out rather than wait for a time nobody set.
fn how_long(cfg: &[RunDefCfg]) -> Result<Duration> {
    let mut total = Duration::zero();
    for (name, per) in [(HOURS, 3600), (MINUTES, 60), (SECONDS, 1)] {
        let n = count(name, answer(cfg, name))?;
        total += Duration::seconds(n * per);
    }
    Ok(total)
}

/// One setting's answer as a whole number — JSON, a number or the digits of one in a string.
fn count(name: &str, value: Option<&str>) -> Result<i64> {
    let Some(value) = value else { return Ok(0) };
    let read = match serde_json::from_str::<serde_json::Value>(value) {
        Ok(serde_json::Value::Null) => return Ok(0),
        Ok(serde_json::Value::Number(n)) => n.as_i64(),
        Ok(serde_json::Value::String(s)) if s.trim().is_empty() => return Ok(0),
        Ok(serde_json::Value::String(s)) => s.trim().parse::<i64>().ok(),
        _ => None,
    };
    // A year of seconds is far past any wait a run is left for, and keeps the sum from overflowing.
    match read {
        Some(n) if (0..=366 * 24 * 3600).contains(&n) => Ok(n),
        _ => Err(Error::invalid(format!(
            "the setting '{name}' says how long to wait, and {value} is not a whole number of zero or more"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ActorKind, Automation, AutomationPictureOwner, AutomationRun, AutomationRunStatus,
        AutomationRunStepStatus,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::{action, due, held_until, time_up, time_up_at};
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{launch_past_the_task_checks as launch, nothing_asked, Launcher};
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::ops::test_support::{mk_project, mk_task_in, with_tx};
    use crate::store_engine::{read, record, WriteTx};
    use crate::time::Timestamp;

    fn cfg(answers: &[(&str, &str)]) -> Vec<RunDefCfg> {
        answers
            .iter()
            .map(|(name, value)| RunDefCfg {
                name: name.to_string(),
                kind: AutomationCfgKind::Number,
                required: false,
                options: None,
                value: Some(value.to_string()),
            })
            .collect()
    }

    /// **The three settings add up, and what is left unanswered counts as none.** A number written as a
    /// string is read the same; anything else is refused.
    #[test]
    fn how_long_adds_the_settings_up() {
        assert_eq!(how_long(&[]).expect("none"), Duration::zero());
        let set = cfg(&[(HOURS, "1"), (MINUTES, "\"2\""), (SECONDS, "3")]);
        assert_eq!(how_long(&set).expect("set"), Duration::seconds(3723));
        for wrong in ["-1", "1.5", "\"soon\"", "[1]"] {
            assert!(how_long(&cfg(&[(SECONDS, wrong)])).is_err(), "{wrong} is refused");
        }
    }

    /// Take a task, wait, and close it — the run ends on the done way out of the close, or where there
    /// was nothing to take.
    fn picture(tx: &WriteTx<'_>, project: i64) -> (Automation, i64) {
        let automation =
            automation::add(tx, project, NewAutomation { name: "wait".into(), ..Default::default() })
                .expect("automation");
        let on = AutomationPictureOwner::Automation;
        let place = |key: &str| {
            automation::placement_add(tx, automation.id, action(tx, key).expect("the built-in").id).expect("place")
        };
        let (take, wait, close) = (place("take_task"), place("wait"), place("close_task"));
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(wait.id), None).expect("on");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("closes");
        automation::edge_add(tx, on, wait.id, None, EdgeTarget::Go(close.id), None).expect("on");
        automation::edge_add(tx, on, close.id, None, EdgeTarget::Done, None).expect("closes");
        automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");
        (automation, wait.id)
    }

    fn launched(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let by = Launcher { startable: None, models: nothing_asked(), workspace_open: Some(true), by: Some(ActorKind::Ai) };
        launch(tx, automation.id, &by).expect("launch")
    }

    /// Open the entry, which takes the task, and open the wait after it — answering its seconds first.
    fn opened_wait(tx: &WriteTx<'_>, seconds: &str) -> (AutomationRun, i64) {
        let project = mk_project(tx, "amenbo");
        let task = mk_task_in(tx, "one", Some(project));
        crate::ops::task::set_assignee(tx, task, Some(ActorKind::Ai)).expect("the AI's");
        let (automation, wait) = picture(tx, project);
        automation::cfg_set(tx, wait, SECONDS, Some(seconds)).expect("answer");
        let run = launched(tx, &automation);
        let entry = read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.builtin.as_deref() == Some("take_task"))
            .expect("the entry");
        let Opened::Carried { next: Next::Step(wait_def), .. } = open(tx, run.id, entry.id, None).expect("take")
        else {
            panic!("the take goes on to the wait");
        };
        let Opened::Holding { run_step_id } = open(tx, run.id, wait_def.id, None).expect("wait") else {
            panic!("the wait holds its step open");
        };
        (run, run_step_id)
    }

    /// **Opening it carries nothing out**: its step stands under way, and the run is running on it —
    /// until the time the settings say has passed since it was opened, when it is due and reports done.
    #[test]
    fn a_wait_holds_its_step_until_its_time_comes() {
        with_tx(|tx| {
            let (run, run_step_id) = opened_wait(tx, "90");
            let step = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(step.status, AutomationRunStepStatus::Running);
            assert_eq!(step.exit_id, None, "nothing reported yet");
            let until = held_until(tx.conn(), &step).expect("until").expect("a wait holds");
            let started = step.started_at.expect("started");
            assert_eq!(until.0 - started.0, Duration::seconds(90));

            assert_eq!(due(tx.conn(), run.id, started).expect("due"), None, "not yet");
            assert!(time_up(tx, run_step_id).is_err(), "and ending it now is refused");
            let later = Timestamp(until.0 + Duration::seconds(1));
            assert_eq!(due(tx.conn(), run.id, later).expect("due"), Some(run_step_id));

            // Ended as if its time had come: the run walks on from the done way out as after an agent's report.
            let Next::Step(close) = time_up_at(tx, run_step_id, later).expect("end") else {
                panic!("the done way out goes on to the close");
            };
            let step = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(step.report, "waited 0h 1m 30s");
            assert_eq!(step.exit_id, crate::ops::test_support::way_out(tx, run_step_id, DONE_EXIT));
            assert!(matches!(open(tx, run.id, close.id, None).expect("close"), Opened::Carried { next: Next::Closed(_), .. }));
            let run = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(run.status, AutomationRunStatus::Completed);
        });
    }

    /// **A wait whose time has come reports done**, and a pause asked for while it waited takes hold
    /// then, as it does when an agent's step reports.
    #[test]
    fn a_pause_takes_hold_when_the_wait_ends() {
        with_tx(|tx| {
            let (run, run_step_id) = opened_wait(tx, "0");
            crate::ops::automation_stop::pause(tx, run.id).expect("pause");
            let still = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(still.status, AutomationRunStatus::Running, "the wait is under way, so the pause waits for it");
            assert_eq!(due(tx.conn(), run.id, Timestamp::now()).expect("due"), Some(run_step_id));
            let ended = time_up(tx, run_step_id).expect("time up");
            assert!(matches!(ended, Next::Paused(_)), "the pause takes hold now: {ended:?}");
        });
    }

    /// **A stop ends the wait at once**, as it ends an agent's step: nothing is left under way, and
    /// nothing is due.
    #[test]
    fn a_stop_ends_the_wait() {
        with_tx(|tx| {
            let (run, run_step_id) = opened_wait(tx, "3600");
            crate::ops::automation_stop::stop(tx, run.id, crate::ops::automation_stop::Ending::Canceled).expect("stop");
            let far = Timestamp(Timestamp::now().0 + Duration::days(1));
            assert_eq!(due(tx.conn(), run.id, far).expect("due"), None);
            assert!(time_up(tx, run_step_id).is_err());
        });
    }

    /// **A setting nobody could wait on is not waited on.** It is refused as it is answered; one that got
    /// onto a run all the same — written before that was checked — leaves by the error way out as the
    /// step is opened, saying why.
    #[test]
    fn a_wait_set_wrong_is_refused_and_leaves_by_the_error_way_out() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let task = mk_task_in(tx, "one", Some(project));
            crate::ops::task::set_assignee(tx, task, Some(ActorKind::Ai)).expect("the AI's");
            let (automation, wait) = picture(tx, project);
            for wrong in ["\"soon\"", "-5", "1.5"] {
                let refused = automation::cfg_set(tx, wait, SECONDS, Some(wrong));
                assert!(refused.is_err(), "{wrong} is not a number of seconds to wait");
            }
            let run = launched(tx, &automation);
            let entry = read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.builtin.as_deref() == Some("take_task"))
                .expect("the entry");
            let Opened::Carried { next: Next::Step(wait_def), .. } = open(tx, run.id, entry.id, None).expect("take")
            else {
                panic!("the take goes on to the wait");
            };
            let mut written = wait_def.clone();
            written.cfg = serde_json::to_string(&[RunDefCfg {
                name: SECONDS.to_string(),
                kind: AutomationCfgKind::Number,
                required: false,
                options: None,
                value: Some("\"soon\"".to_string()),
            }])
            .expect("json");
            crate::ops::emit_update(tx, record::automation_run_def(&wait_def), record::automation_run_def(&written))
                .expect("an answer past the checks");
            let Opened::Carried { run_step_id, next } = open(tx, run.id, wait_def.id, None).expect("wait") else {
                panic!("a wait set wrong is not held");
            };
            assert!(matches!(next, Next::Halted(_)), "the error way out halts: {next:?}");
            let step = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert!(step.report.contains("soon"), "{}", step.report);
        });
    }
}
