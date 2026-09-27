//! The mark a CLI reads to know the app is up on this store (`amenbo_core::app_running`,
//! `AMB-D-995`): taken as the app comes up, and let go as it ends.
//!
//! What this side owns is the occasion. The claim is held in the app's state for the life of the
//! process, and released on [`tauri::RunEvent::Exit`] so an orderly quit leaves no file behind. A
//! process that ends any other way leaves the file, and the lock goes with the process — the reading
//! side takes that as not running.

use std::sync::Mutex;

use amenbo_core::app_running::{self, Presence};
use tauri::Manager;

/// The claim, while the app holds it. `None` when it could not be taken — no base directory to put
/// it in, or another process on the same store got there first.
#[derive(Default)]
pub struct Held(Mutex<Option<Presence>>);

/// Take the mark and keep it in the app's state. A mark that cannot be taken is written to the log
/// and nothing more: the app runs the same without it, and what goes missing is only a CLI's
/// ability to see it.
pub fn claim(app: &tauri::AppHandle) {
    let presence = match amenbo_core::config::Paths::resolve().and_then(|paths| app_running::claim(&paths)) {
        Ok(Some(presence)) => Some(presence),
        Ok(None) => {
            log::warn!("app running mark: already held by another process on this store");
            None
        }
        Err(e) => {
            log::warn!("app running mark: not taken ({e})");
            None
        }
    };
    app.manage(Held(Mutex::new(presence)));
}

/// Let the mark go, on the way out.
pub fn release(app: &tauri::AppHandle) {
    if let Some(held) = app.try_state::<Held>() {
        if let Some(presence) = held.0.lock().ok().and_then(|mut slot| slot.take()) {
            presence.release();
        }
    }
}
