//! The labels of the two windows this app opens, and which of them anything that raises a window
//! means.
//!
//! Amenbo is one app with two faces (`AMB-D-733`'s theme): the **board**, where the ledger is read
//! and written, and the **talk** window, where the agents run. They are separate windows rather than
//! two views of one so they can sit on separate displays, and one process still backs both — there
//! is one store, one watcher, one guard.
//!
//! Everything that raises a window from outside the webview — a second launch turned away, a
//! notification clicked — means the **board**. What sends those is an arrival in the inbox or the
//! user asking for the app they already have open, and both of those are the board's subject. So
//! [`BOARD`](crate::windows::BOARD) is what they name, spelled once here rather than as a string in
//! three files.
//!
//! A launch at login is the ordinary launch (`autostart`), so both windows come up there exactly as
//! they do when the user opens the app: the board in front, the talk window behind it (`focus:
//! false` in `tauri.conf.json`). Nothing decides between them at startup, because a window that came
//! up is one the user can move to another display and leave there.

/// The window the ledger is read in — the app as it was before the second one existed, which is why
/// its label is still `main`: the label is what `tauri.conf.json` and every `get_webview_window`
/// call agree on, and renaming it would buy nothing.
pub const BOARD: &str = "main";

/// The window the agents run in.
pub const TALK: &str = "talk";

use tauri::{Emitter as _, Manager as _};

use crate::dto::RefTargetDto;

/// The event the board navigates on when a record is asked for from outside its own webview.
const SHOW_REF_EVENT: &str = "ref-activated";

/// Which of the two spaces a ref clicked in a pane names.
///
/// An enum and not a string: only a task and a decision are destinations — `amenbo_core::idref::url`
/// gives an address to no other space — so a word this does not know is turned away by the
/// deserializer, before anything here runs. A string argument would have needed a branch of its own,
/// and a sentence for a case the pane cannot produce.
#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefSpace {
    Task,
    Decision,
}

impl RefSpace {
    /// The word the front end branches on, as [`RefTargetDto`] spells it.
    fn as_str(&self) -> &'static str {
        match self {
            RefSpace::Task => "task",
            RefSpace::Decision => "decision",
        }
    }
}

/// Show a task or a decision on the board, asked for from inside a pane.
///
/// A ref clicked in a terminal names a record, and records are read on the board — so the answer
/// spans both windows, and this is the seam. What happens here is the half a webview cannot do: a
/// window cannot raise its sibling, so the board is brought forward from the process that owns them
/// both. What happens on the board is the half this must not do: which screen a record is read on,
/// and what a selection does to whatever was open there, is the front end's routing, and resolving
/// it out here would be a second copy of it, free to go stale on its own.
#[tauri::command]
pub fn show_ref(app: tauri::AppHandle, kind: RefSpace, id: i64) {
    if let Some(win) = app.get_webview_window(BOARD) {
        // Spelled out the way a notification click spells it (`crate::macos_notify`): a window that
        // was minimized or hidden is not brought back by focus alone.
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    // Logged rather than returned, as the notifier does with the same call. The window is already in
    // front by now, so there is nothing left for the pane to do about it, and a refusal handed back
    // there would reach a click that has no way to report one.
    if let Err(e) = app.emit(SHOW_REF_EVENT, RefTargetDto { kind: kind.as_str().into(), id }) {
        log::warn!("failed to emit {SHOW_REF_EVENT}: {e}");
    }
}
