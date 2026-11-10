//! **The thread that keeps a run going**: it looks at what is running, and opens whatever step is
//! waiting to be opened (`AMB-D-945`).
//!
//! **A run is the subject, not a reaction.** Somebody presses start, or an automation's trigger files a
//! task and starts one, and from there the run carries itself — the same shape as pressing play. The
//! other road available was to hang the advance off the store changing, and that one inverts it: the
//! run would become an answer to somebody else's write, and every entrance into it would carry its own
//! copy of "and then open the next one".
//!
//! **It is not where the truth is.** What a run is doing is in the store, written by the ops that do
//! it; this thread only asks and opens. It dies with the app, and a run that was under way when that
//! happened is swept on the way back up (`AMB-T-5274`) — so nothing is kept here that a restart would
//! have to rebuild. So does the thread a script step's program runs on (`crate::automation`): a run
//! whose script was running is swept the same way, failed as a crash (`AMB-D-961`).
//!
//! **What one look costs.** One read of the runs that are running, and one read per run of what it is
//! waiting for — a few more where a step is under way, for whether it is a wait whose time has come
//! (`AMB-D-983`) and for the task it may have taken. A look is a handful of small reads on tables with
//! tens of rows in them, through a connection the thread keeps.
//!
//! **No run waits for a task here, and no automation as a whole either** (`AMB-D-1126`). A run whose
//! built-in would wait ends where it comes to it, and its seat is free again; the launch standing fills
//! it once a task of that role turns up (`top_up`, below). Nothing is written while there is none.
//!
//! **And how often.** `WHILE_GOING` while anything is running, because a step is a person-scale
//! thing and a second is under the noticing; `WHILE_IDLE` otherwise, so a machine with no
//! automations on it is not woken five times a second for nothing. A press inside the app skips the
//! wait ([`wake`]); a `start` typed in a terminal cannot reach this process, so that one waits out the
//! idle sleep.
//!
//! **One launch keeps its runs going side by side** (`top_up`). Each look asks every running launch
//! whether one of its automation's roles has a free seat under the headcount the launch holds for it,
//! and a task of its own to take, and starts one more run in that seat where it does — so a launch fills
//! its seats one run a look, a run that ends makes room for the next, and a headcount raised on the
//! launch takes hold on the next look. A role none of whose runs is about to take a
//! task is the only one given another, so two runs are not started for one task.
//!
//! **Triggers are asked here too** (`fire_triggers`): each look asks which are due before it looks at
//! the runs, so a run a trigger started is opened in the same look. A trigger keeps the time it is next
//! due in the store and nothing runs beside this thread for it (`AMB-D-983`'s shape): a look sleeps no
//! longer than until the soonest one. The app being closed is what stops them — a terminal starts no
//! run on its own (`AMB-D-995`). A trigger with a script — one that asks whether to file a task, or one
//! that writes what it says — has its programs run on a thread of its own, outside any write, because
//! they may run for hours; the look that finds their answer fires it.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use amenbo_core::ops::automation_run::{ScriptsSaid, TriggerScripts, Waiting};

/// How long between looks while something is running. A step takes minutes, so a second is under
/// what anybody notices — and it is a second rather than half of one because there is no reason to
/// wake a laptop twice as often for a number nobody can see move.
const WHILE_GOING: Duration = Duration::from_secs(1);

/// How long between looks while nothing is. Long enough that an idle machine is left alone, short
/// enough that a run started from a terminal — which cannot reach this process to say so — comes up
/// without anybody wondering whether it worked.
const WHILE_IDLE: Duration = Duration::from_secs(5);

/// How long a trigger or a launch whose run was refused is left before it is asked again
/// (`AMB-D-983`), rather than every look with a warning each time. A run is not held to this: it is
/// still looked at every `WHILE_GOING`.
const WHILE_WAITING: Duration = Duration::from_secs(30);

/// How long a trigger whose scripts have answered is left before they are asked again — the least a
/// trigger waits between two tasks it files. One that said no keeps its `next_at` (`AMB-D-1119`), so
/// without this its programs would be started again on every look.
const SCRIPT_ASKED_AGAIN: Duration = Duration::from_secs(60 * amenbo_core::model::MIN_TRIGGER_EVERY_MINUTES as u64);

/// **Where a trigger's scripts stand**, by trigger: running on a thread of their own, or answered and
/// left until they may be asked again.
enum Asking {
    /// Its programs are running; what they said comes down here when they end — `None` for no.
    Running(Receiver<Result<Option<ScriptsSaid>, String>>),
    /// It answered, and is not asked again until then.
    Answered(Instant),
}

/// What the sleeping thread is woken through. It carries no message: the answer to "is there anything
/// to do" is in the store, and a flag here could only ever be a second, staler copy of it.
static NUDGE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// **Look now rather than at the end of the wait.** Called where this process is the one that moved a
/// run — launching it, or opening a step of it — so what the run is waiting for next is picked up at
/// once instead of up to `WHILE_IDLE` later.
///
/// **A `start` typed in a terminal cannot reach here.** The CLI is another process, so a run launched
/// there is found by the next look rather than announced — which is what `WHILE_IDLE` is sized for.
///
/// It is a nudge and not an instruction: what the look finds is still read from the store, so a nudge
/// that arrives when there is nothing to do costs one look.
pub(crate) fn wake() {
    if let Ok(mut nudged) = NUDGE.0.lock() {
        *nudged = true;
        NUDGE.1.notify_all();
    }
}

/// The thread itself. It never returns: the app ending is what ends it.
pub fn watch(app: tauri::AppHandle) {
    // Before the first look, and before anything else in this process can act on a run: what a
    // previous launch left standing is stopped (`sweep`). Here rather than beside the other startup
    // work because the order matters and this is where it is guaranteed — a look taken first would
    // open a step for a run that is about to be told it crashed.
    sweep();
    // The triggers refused at launch, and when each is asked again. Kept here rather than in the store
    // because it is only a pace: a restart that loses it asks them once more, which is all.
    let mut held = HashMap::new();
    // The triggers whose script is running or has answered. Not cleared by a nudge, so a press in the
    // app does not start a program again; a restart that loses it asks them once more, which is all.
    let mut asking = HashMap::new();
    // The launches whose next run was refused, and when each is asked again. A pace, like `held`.
    let mut refused = HashMap::new();
    loop {
        // A failure is not fatal and not a reason to stop looking: the store may be mid-swap, or a
        // run may have been deleted between the two reads. The next look is a second away.
        let pause = match advance(&app, &mut held, &mut asking, &mut refused) {
            Ok(pause) => pause,
            Err(e) => {
                log::warn!("the automation watch could not look: {e}");
                WHILE_IDLE
            }
        };
        if sleep(pause) {
            held.clear();
            refused.clear();
        }
    }
}

/// **Stop what a previous launch left standing**, once, before this one acts on any run.
///
/// A run's steps are terminals and threads of the app, so a `running` row on the way up is a record of
/// what was true before rather than of anything going on now: left alone it holds a task nobody is
/// working.
///
/// **Nobody is told.** What a person needs to find is the task, and stopping a run hands that back
/// to `todo` with a comment saying how far it got — which is where they work, and it is there
/// whether or not they were looking at a banner when the app came up
/// (`amenbo_core::ops::automation_stop::ended`).
///
/// **The CLI does not do this.** Opening the store from a terminal says nothing about whether the app
/// is running, so a sweep there would stop the runs of an app that is up and watching them.
fn sweep() {
    let swept = crate::commands::open_store()
        .and_then(|mut store| store.automation_sweep().map_err(crate::error::CmdError::from));
    match swept {
        Ok(runs) if !runs.is_empty() => {
            log::info!("{} run(s) did not survive the last launch and were stopped", runs.len());
        }
        Ok(_) => {}
        // Not fatal and not worth stopping the watch over: what was not swept is swept next launch,
        // and until then it costs the task the run was holding.
        Err(e) => log::warn!("the runs left by the last launch were not swept: {}", e.message_en),
    }
}

/// Wait out one interval, or until somebody nudges — whichever comes first — and answer whether it was
/// a nudge. One that came while the last look was being taken counts too, rather than being lost to
/// a wait that had not started yet.
fn sleep(how_long: Duration) -> bool {
    let Ok(nudged) = NUDGE.0.lock() else { return false };
    let Ok((mut nudged, _)) = NUDGE.1.wait_timeout_while(nudged, how_long, |nudged| !*nudged) else {
        return false;
    };
    std::mem::take(&mut *nudged)
}

/// **One look**: move each running run on as far as it can go, and answer how long to wait before the
/// next one — `WHILE_GOING` while any run is running, and otherwise until the first trigger is due,
/// `WHILE_IDLE` at most.
///
/// A run answers one of three things ([`amenbo_core::ops::automation_run::Waiting`]) and each is acted
/// on here. A step waiting to be opened is opened. A run with a step under way is ended where it is a
/// wait whose time has come, and otherwise only checked for a change to the task that step holds
/// since it opened. A run with nowhere left to go is ended — it would otherwise be read again every second for the rest of
/// the session, holding a task nobody is working.
///
/// A run whose step could not be opened is left where it is and looked at again next time. The one
/// that stops a run — a required input with nothing in it — stops it inside the op that found it, so
/// this sees it as a run that is no longer running rather than as a failure to handle.
///
/// The triggers are fired first (`fire_triggers`), then the running launches are topped up (`top_up`),
/// and the wait is cut short to when the next trigger is due.
fn advance(
    app: &tauri::AppHandle,
    held: &mut HashMap<i64, Instant>,
    asking: &mut HashMap<i64, Asking>,
    refused: &mut HashMap<i64, Instant>,
) -> Result<Duration, crate::error::CmdError> {
    let next_trigger = fire_triggers(held, asking)?;
    top_up(refused)?;
    let running = {
        let store = crate::commands::open_store_read()?;
        amenbo_core::store_engine::read::automation_run_ids_running(store.read_model().conn())?
    };
    if let Err(e) = end_what_is_over(app, &running) {
        log::warn!("the terminals of runs that are over could not be ended: {}", e.message_en);
    }
    for run in &running {
        // Asked run by run rather than in one sweep, because opening one writes — and the answer for
        // the run after it is read after that write rather than from a list taken before it.
        let waiting = {
            let store = crate::commands::open_store_read()?;
            amenbo_core::ops::automation_run::next_def(store.read_model().conn(), *run)?
        };
        match waiting {
            // Before the entry is opened, what has been saved since the run's copy was taken is taken
            // up (`AMB-D-1015`): a run that came back to its entry starts from the newest saved
            // definition. A run that failed its check has nothing left to open.
            Waiting::Step(def) => {
                let def = match crate::automation::take_up_newer(app, *run, *def) {
                    Ok(Some(def)) => def,
                    Ok(None) => continue,
                    Err(e) => {
                        log::warn!("run {run} could not take up a newer saved version: {}", e.message_en);
                        continue;
                    }
                };
                if let Err(e) = crate::automation::automation_step_open(app.clone(), *run, Some(def.id)) {
                    log::warn!("run {run} could not open step {}: {}", def.id, e.message_en)
                }
            }
            // A step under way that the built-in that waits holds open ends here once its time has
            // come (`AMB-D-983`) — this look is the only thing that ends it.
            //
            // Otherwise the task it holds may have changed since it was opened — deleted out from
            // under it, say — and the pane was told before then (`crate::automation::retell_task`).
            Waiting::Nothing => {
                match crate::automation::time_up(app, *run) {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(e) => log::warn!("run {run} could not end the wait it holds: {}", e.message_en),
                }
                let store = crate::commands::open_store_read()?;
                if let Err(e) = crate::automation::retell_task(app, &store, *run) {
                    log::warn!("run {run} could not say which task its step holds: {}", e.message_en);
                }
            }
            // Ended here rather than by whatever changed the definition: what a run can still do is
            // read off the run's own copies, and an edit that leaves one of a dozen runs with nowhere
            // to go does not know which. A stop hands the task back with a comment saying how far it
            // got, which is where the reader finds it.
            Waiting::NoWayOn => {
                if let Err(e) = give_up(*run) {
                    log::warn!("run {run} has nowhere to go and could not be stopped: {}", e.message_en);
                }
            }
        }
    }
    if !running.is_empty() {
        return Ok(WHILE_GOING);
    }
    let first_due = held.values().min().copied().into_iter().chain(next_trigger).min();
    Ok(first_due
        .map_or(WHILE_IDLE, |due| due.saturating_duration_since(Instant::now()).min(WHILE_IDLE)))
}

/// **Fire every trigger that is due** ([`amenbo_core::ops::automation_run::fire`]), and answer when the
/// next one not yet due is, if any.
///
/// Each is fired in a write of its own, so one refused does not take the others with it. Nobody pressed
/// anything, so the launch says nothing about who did, cannot see a window, and has asked nobody which
/// agents this machine can start (`AMB-D-792`): no step is judged on its agent, and the run waits for
/// its first step to be opened as one started from a terminal does. The models are asked, as a press
/// in the app asks them.
///
/// A trigger whose launch was refused — its automation not saved, its entry not the built-in that files
/// a task, the file its task is read from not there — goes into `held` and is not asked again until
/// `WHILE_WAITING` has passed, rather than every second with a warning each time. Why is kept on the
/// trigger (`AMB-D-1136`), by the store as it answers.
///
/// A trigger due but for its scripts (`ask`) has its programs started on a thread of their own
/// ([`amenbo_core::ops::automation_run::ask_trigger`]), which wakes the look when they end; the look that
/// finds the answer fires it, handing it what they said. Whatever they said, they are not asked again
/// for `SCRIPT_ASKED_AGAIN`. One that said nothing — a program that fell over — is warned about, files
/// nothing, and keeps why on the trigger ([`keep_unanswered`]).
fn fire_triggers(
    held: &mut HashMap<i64, Instant>,
    asking: &mut HashMap<i64, Asking>,
) -> Result<Option<Instant>, crate::error::CmdError> {
    let found = {
        let store = crate::commands::open_store_read()?;
        amenbo_core::ops::automation_run::triggers_due(store.read_model().conn(), amenbo_core::time::Timestamp::now())?
    };
    let to_ask = |trigger: &i64| found.ask.iter().any(|(asked, _)| asked == trigger);
    held.retain(|trigger, due| (found.due.contains(trigger) || to_ask(trigger)) && Instant::now() < *due);
    asking.retain(|trigger, standing| match standing {
        Asking::Running(_) => true,
        Asking::Answered(until) => to_ask(trigger) && Instant::now() < *until,
    });
    let mut due: Vec<(i64, Option<ScriptsSaid>)> = found
        .due
        .into_iter()
        .filter(|trigger| !held.contains_key(trigger))
        .map(|trigger| (trigger, None))
        .collect();
    for (trigger, scripts) in &found.ask {
        if !held.contains_key(trigger) && !asking.contains_key(trigger) {
            asking.insert(*trigger, ask(*trigger, scripts.clone()));
        }
    }
    for (trigger, standing) in asking.iter_mut() {
        let Asking::Running(answer) = standing else { continue };
        let said = match answer.try_recv() {
            Ok(said) => Some(said),
            Err(TryRecvError::Empty) => continue,
            Err(TryRecvError::Disconnected) => None,
        };
        *standing = Asking::Answered(Instant::now() + SCRIPT_ASKED_AGAIN);
        match said {
            Some(Ok(Some(said))) => due.push((*trigger, Some(said))),
            Some(Ok(None)) => {}
            Some(Err(why)) => {
                log::warn!("trigger {trigger} had no answer from its script: {why}");
                keep_unanswered(*trigger, &why);
            }
            None => {
                log::warn!("trigger {trigger} lost its script before it answered");
                keep_unanswered(*trigger, "it was lost before it answered");
            }
        }
    }
    if !due.is_empty() {
        let offered = crate::agent_models::offered_here();
        let by = amenbo_core::ops::automation_run::Launcher {
            default_agent: None,
            startable: None,
            models: &offered,
            workspace_open: None,
            by: None,
        };
        for (trigger, said) in due {
            let fired =
                crate::commands::with_store_mut(|store| Ok(store.automation_trigger_fire(trigger, &by, said.as_ref())?));
            match fired {
                Ok(Some(run)) => log::info!("trigger {trigger} started run {}", run.id),
                Ok(None) => {}
                Err(e) => {
                    log::warn!("trigger {trigger} could not start a run: {}", e.message_en);
                    held.insert(trigger, Instant::now() + WHILE_WAITING);
                }
            }
        }
    }
    let now = amenbo_core::time::Timestamp::now();
    let next_at = found.next_at.map(|at| Instant::now() + (at.0 - now.0).to_std().unwrap_or(Duration::ZERO));
    let answered = asking.values().filter_map(|standing| match standing {
        Asking::Answered(until) => Some(*until),
        Asking::Running(_) => None,
    });
    Ok(next_at.into_iter().chain(answered).min())
}

/// **Keep on a trigger that its programs gave no answer, and why** (`AMB-D-1136`,
/// [`amenbo_core::ops::automation_run::unanswered`]). A store that cannot take it is warned about.
fn keep_unanswered(trigger: i64, why: &str) {
    let why = amenbo_core::ops::automation_run::unanswered(why);
    let kept = crate::commands::with_store_mut(|store| Ok(store.automation_trigger_refused(trigger, &why)?));
    if let Err(e) = kept {
        log::warn!("trigger {trigger} could not keep why it started nothing: {}", e.message_en);
    }
}

/// **Start a trigger's scripts** on a thread of their own, and wake the look once they have answered.
/// Each is stopped only at its timeout: nothing in the app waits on them.
fn ask(trigger: i64, scripts: TriggerScripts) -> Asking {
    let (tell, answer) = std::sync::mpsc::channel();
    let started = std::thread::Builder::new().name(format!("trigger-script-{trigger}")).spawn(move || {
        let said = amenbo_core::ops::automation_run::ask_trigger(&scripts, || false);
        let _ = tell.send(said);
        wake();
    });
    if let Err(e) = started {
        log::warn!("trigger {trigger} could not start a thread for its script: {e}");
    }
    Asking::Running(answer)
}

/// **Start one more run in each running launch where a role has a free seat and a task to take**
/// ([`amenbo_core::ops::automation_run::top_up`]).
///
/// Asked every look, as the runs are. A launch stays running with no run going until a person stops
/// it, so what is asked is kept small: a few small reads and one row of the tasks for each role. Each run is
/// started in a write of its own, launched as a trigger launches — no window is seen and no agent probe
/// is passed, and the models are asked; who it is started by is the launch's. A launch whose run was
/// refused goes into `refused` and is not asked again until `WHILE_WAITING` has passed, rather than
/// every second with a warning each time.
fn top_up(refused: &mut HashMap<i64, Instant>) -> Result<(), crate::error::CmdError> {
    let now = Instant::now();
    refused.retain(|_, due| now < *due);
    let due = {
        let store = crate::commands::open_store_read()?;
        let conn = store.read_model().conn();
        let running: Vec<i64> = amenbo_core::store_engine::read::automation_launches_running(conn)?
            .into_iter()
            .map(|launch| launch.id)
            .filter(|launch| !refused.contains_key(launch))
            .collect();
        amenbo_core::ops::automation_run::launches_to_top_up(conn, &running)?
    };
    if due.is_empty() {
        return Ok(());
    }
    let offered = crate::agent_models::offered_here();
    let by = amenbo_core::ops::automation_run::Launcher {
        default_agent: None,
        startable: None,
        models: &offered,
        workspace_open: None,
        by: None,
    };
    for launch in due {
        match crate::commands::with_store_mut(|store| Ok(store.automation_launch_top_up(launch, &by)?)) {
            Ok(Some(run)) => log::info!("launch {launch} had room and a task, and started run {}", run.id),
            Ok(None) => {}
            Err(e) => {
                log::warn!("launch {launch} could not start another run: {}", e.message_en);
                refused.insert(launch, Instant::now() + WHILE_WAITING);
            }
        }
    }
    Ok(())
}

/// **End the terminal of a run that is over** — completed, failed or canceled — and forget what was
/// kept of its step for a face coming up ([`crate::automation::StepsStanding`]).
///
/// A step's terminal is otherwise ended only as the next step opens ([`crate::pty::end_steps_of`]), so
/// the last one of a run would stand there after it: the agent waiting for input, still typing
/// commands, over a task the run has handed back. A run ends by many roads, one of
/// them a `step-done` typed in another process, and this look is the one place all of them pass. A
/// built-in or a script step a run stopped on was kept the same way, and without this a workspace put
/// up again stood the stopped run's pane once more.
///
/// **The screen of an agent's step that failed the run is kept before its terminal is ended**
/// (`crate::automation::keep_failed_screen`), so the round can be read from the task it was filed as
/// once the terminal is gone.
///
/// A paused run keeps its terminal and what was kept of it: it is not over, and the step it paused
/// after may be asked why. A script step's terminal whose program has ended is not one this looks at:
/// there is nothing in it to stop, and it stays for its pane to read (`crate::pty::runs_with_steps`).
/// The store is read only where a terminal or a kept step stands for a run that is not running, which
/// is none on most looks.
fn end_what_is_over(app: &tauri::AppHandle, running: &[i64]) -> Result<(), crate::error::CmdError> {
    let terminals: Vec<i64> =
        crate::pty::runs_with_steps(app).into_iter().filter(|run| !running.contains(run)).collect();
    let kept: Vec<i64> = crate::automation::runs_standing(app)
        .into_iter()
        .filter(|run| !running.contains(run))
        .collect();
    if terminals.is_empty() && kept.is_empty() {
        return Ok(());
    }
    let store = crate::commands::open_store_read()?;
    let conn = store.read_model().conn();
    let live = crate::automation::runs_still_standing(
        terminals.iter().chain(kept.iter()).copied(),
        |run| Ok(amenbo_core::store_engine::read::automation_run(conn, run)?.map(|it| it.status)),
    )?;
    drop(store);
    for run in terminals.into_iter().filter(|run| !live.contains(run)) {
        // An agent's step that failed the run by its own error way out is still on its terminal, and
        // the screen it stood at is kept before the terminal goes (`crate::automation::keep_failed_screen`).
        for (run_step, screen) in crate::pty::step_screens_of(app, run) {
            crate::automation::keep_failed_screen(run_step, &screen, None);
        }
        crate::pty::end_steps_of(app, run);
    }
    for run in kept.into_iter().filter(|run| !live.contains(run)) {
        crate::automation::forget_standing(app, run);
    }
    Ok(())
}

/// **End a run that cannot go on**/// **End a run that cannot go on**, with the reason that says so
/// ([`amenbo_core::model::AutomationStoppedReason::NoWayOn`]).
fn give_up(run: i64) -> Result<(), crate::error::CmdError> {
    let mut store = crate::commands::open_store()?;
    let ended =
        store.automation_stop(
            run,
            amenbo_core::ops::automation_stop::Ending::Failed(
                amenbo_core::model::AutomationStoppedReason::NoWayOn,
            ),
        )?;
    log::info!("run {} had nowhere left to go and was stopped", ended.run.id);
    Ok(())
}
