//! **The thread that keeps a run going**: it looks at what is running, and opens whatever step is
//! waiting to be opened (`AMB-D-945`).
//!
//! **A run is the subject, not a reaction.** Somebody presses start, and from there the run carries
//! itself — the same shape as pressing play. The other road available was to hang the advance off the
//! store changing, and that one inverts it: the run would become an answer to somebody else's write,
//! and every entrance into it would carry its own copy of "and then open the next one".
//!
//! **It is not where the truth is.** What a run is doing is in the store, written by the ops that do
//! it; this thread only asks and opens. It dies with the app, and a run that was under way when that
//! happened is swept on the way back up (`AMB-T-5274`) — so nothing is kept here that a restart would
//! have to rebuild.
//!
//! **What one look costs.** One read of the runs that are running, and one read per run of what it is
//! waiting for. How many runs there can be is capped by the lanes
//! ([`amenbo_core::config::Config::automation_lanes`], three by default), so a look is four small
//! reads on tables with tens of rows in them, through a connection the thread keeps.
//!
//! **And how often.** `WHILE_GOING` while anything is running, because a step is a person-scale
//! thing and a second is under the noticing; `WHILE_IDLE` otherwise, so a machine with no
//! automations on it is not woken five times a second for nothing. A press inside the app skips the
//! wait ([`wake`]); a `start` typed in a terminal cannot reach this process, so that one waits out
//! the idle sleep.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// How long between looks while something is running. A step takes minutes, so a second is under
/// what anybody notices — and it is a second rather than half of one because there is no reason to
/// wake a laptop twice as often for a number nobody can see move.
const WHILE_GOING: Duration = Duration::from_secs(1);

/// How long between looks while nothing is. Long enough that an idle machine is left alone, short
/// enough that a run started from a terminal — which cannot reach this process to say so — comes up
/// without anybody wondering whether it worked.
const WHILE_IDLE: Duration = Duration::from_secs(5);

/// What the sleeping thread is woken through. It carries no message: the answer to "is there anything
/// to do" is in the store, and a flag here could only ever be a second, staler copy of it.
static NUDGE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// **Look now rather than at the end of the wait.** Called where this process is the one that moved a
/// run — opening a step of it — so what comes after that step is picked up at once instead of up to
/// `WHILE_IDLE` later.
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
    loop {
        // A failure is not fatal and not a reason to stop looking: the store may be mid-swap, or a
        // run may have been deleted between the two reads. The next look is a second away.
        let going = match advance(&app) {
            Ok(going) => going,
            Err(e) => {
                log::warn!("the automation watch could not look: {e}");
                false
            }
        };
        sleep(if going { WHILE_GOING } else { WHILE_IDLE });
    }
}

/// Wait out one interval, or until somebody nudges — whichever comes first.
fn sleep(how_long: Duration) {
    let Ok(nudged) = NUDGE.0.lock() else { return };
    let Ok((mut nudged, _)) = NUDGE.1.wait_timeout(nudged, how_long) else { return };
    *nudged = false;
}

/// **One look**: open whatever each running run is waiting for, and answer whether anything is
/// running at all — which is what decides how long to wait before the next one.
///
/// A run whose step could not be opened is left where it is and looked at again next time. The one
/// that stops a run — a required input with nothing in it — stops it inside the op that found it, so
/// this sees it as a run that is no longer running rather than as a failure to handle.
fn advance(app: &tauri::AppHandle) -> Result<bool, crate::error::CmdError> {
    let running = {
        let store = crate::commands::open_store_read()?;
        amenbo_core::store_engine::read::automation_run_ids_running(store.read_model().conn())?
    };
    for run in &running {
        // Asked run by run rather than in one sweep, because opening one writes — and the answer for
        // the run after it is read after that write rather than from a list taken before it.
        let waiting = {
            let store = crate::commands::open_store_read()?;
            amenbo_core::ops::automation_run::next_def(store.read_model().conn(), *run)?
        };
        let Some(def) = waiting else { continue };
        if let Err(e) = crate::automation::automation_step_open(app.clone(), *run, Some(def.id)) {
            log::warn!("run {run} could not open step {}: {}", def.id, e.message_en);
        }
    }
    Ok(!running.is_empty())
}
