//! The labels of the windows this app can open, which of them anything that raises a window means,
//! and the door the second one is opened and closed through.
//!
//! Amenbo has two faces (`AMB-D-733`'s theme): the **board**, where the ledger is read and written,
//! and the **terminal**, where the agents run. What they are *not* is two windows by default. The
//! app comes up as one window showing the board, and the two faces are switched between inside it;
//! only someone who wants them side by side splits the terminal out into a window of its own, and
//! can fold it back (`AMB-D-753`). Two windows opening unasked leave a first-time user wondering
//! what happened, and leave someone on a single laptop screen with nowhere to put the second.
//!
//! So the talk window is not in `tauri.conf.json`: a window declared there is a window that opens at
//! launch, and this one opens when it is asked for. [`talk_open`](crate::windows::talk_open) builds it, [`talk_close`](crate::windows::talk_close) takes it
//! away, and what is in it — the terminal — is not restarted by either, because a session belongs to
//! the process rather than to a window (`crate::pty`).
//!
//! Everything that raises a window from outside the webview — a second launch turned away, a
//! notification clicked — means the **board**. What sends those is an arrival in the inbox or the
//! user asking for the app they already have open, and both of those are the board's subject. So
//! [`BOARD`](crate::windows::BOARD) is what they name, spelled once here rather than as a string in three files, and it is
//! the one window that is always there to name.
//!
//! A launch at login is the ordinary launch (`autostart`), so it comes up exactly as opening the app
//! does: the board, in whichever shape this machine was last used in.

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::dto::RefTargetDto;
use crate::error::CmdError;

/// The window the ledger is read in — the app as it was before the second one existed, which is why
/// its label is still `main`: the label is what `tauri.conf.json` and every `get_webview_window`
/// call agree on, and renaming it would buy nothing.
pub const BOARD: &str = "main";

/// The window the agents run in, once the terminal has been split out of the board.
pub const TALK: &str = "talk";

/// The page the talk window is built on, and the second entry `app/vite.config.ts` emits for it.
const TALK_URL: &str = "talk.html";

/// Told to the board when the talk window has gone, so it can put the terminal back on a face of its
/// own and go back to being one window.
///
/// It is raised from the window's own end rather than from the button that folds the app back,
/// because the button is not the only way out: the title bar's close is one too, and an app that
/// only noticed the tidy exit would sit there believing it was still two windows — its "terminal"
/// would keep pointing at a window that is not there, and the terminal itself would be unreachable
/// while still running.
pub const TALK_CLOSED_EVENT: &str = "talk://closed";

/// The talk window's size, in logical pixels: the board's own, so the two look like one app split in
/// half rather than a window and a dialogue.
const TALK_SIZE: (f64, f64) = (1280.0, 820.0);
const TALK_MIN_SIZE: (f64, f64) = (960.0, 640.0);

/// How far down and to the right of the board a newly split-out window is put.
///
/// Left to itself the second window comes up in the same place at the same size, exactly covering
/// the first — two windows nobody can tell apart or find the edge of (`AMB-T-3588` measured it).
/// macOS can cycle them with ⌘`, and Windows and Linux offer no such promise. An offset is the whole
/// fix: the user can see there are two, and drag the top one wherever they meant to put it.
const TALK_OFFSET: f64 = 48.0;

/// Turn a window that could not be built or closed into the refusal the webview is given. There is
/// nothing for the caller to do about it beyond saying so, so what it says is what tauri said.
fn failed(e: tauri::Error) -> CmdError {
    let reason = e.to_string();
    CmdError::coded(
        "window_failed",
        format!("That window could not be opened: {reason}"),
        serde_json::json!({ "reason": reason }),
    )
}

/// Make sure the talk window exists, and — with `raise` — bring it to the front.
///
/// Both are this one door because a window that is not there and a window that is behind other work
/// look the same to the person asking for the terminal, and answering either by opening a *second*
/// window would answer a request to see something by making another of it.
///
/// `raise` is the difference between the user asking and the app arranging itself. Pressing
/// "terminal" is asking, and wants the window in front. Coming up in the shape this machine was last
/// used in is arranging, and must not take the front away from the board the user is looking at.
///
/// Raising is spelled out the way a notification click spells it (`crate::macos_notify`) —
/// unminimize, show, focus — because a window that is merely unfocused and a window that is
/// minimized both look, to the user, like the terminal not being there.
#[tauri::command]
pub fn talk_open(app: tauri::AppHandle, raise: bool) -> Result<(), CmdError> {
    if let Some(win) = app.get_webview_window(TALK) {
        if raise {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
        return Ok(());
    }
    let mut builder = WebviewWindowBuilder::new(&app, TALK, WebviewUrl::App(TALK_URL.into()))
        // The product name, which is what shows for the moment before the page names itself in the
        // reader's own language (`app/src/talk.ts`).
        .title("Amenbo")
        .inner_size(TALK_SIZE.0, TALK_SIZE.1)
        .min_inner_size(TALK_MIN_SIZE.0, TALK_MIN_SIZE.1)
        .resizable(true)
        // A window that was not asked for does not take the front. A launch restoring the shape this
        // machine was last used in builds this one, and the board is what the user is looking at.
        .focused(raise)
        // As the board has it (`tauri.conf.json`): a file dropped on the window is the page's to
        // refuse, and the OS handler would otherwise navigate the webview to whatever was dropped.
        .disable_drag_drop_handler();
    if let Some((x, y)) = beside_the_board(&app) {
        builder = builder.position(x, y);
    }
    let win = builder.build().map_err(failed)?;
    // A window is put in front of the others as it is made, whether or not it was given the
    // keyboard — so the board is raised back over it. Without this, restoring the shape at launch
    // buries the ledger under a terminal nobody asked to look at.
    if !raise {
        if let Some(board) = app.get_webview_window(BOARD) {
            let _ = board.set_focus();
        }
    }
    let raised = app.clone();
    win.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            let _ = raised.emit_to(BOARD, TALK_CLOSED_EVENT, ());
        }
    });
    Ok(())
}

/// Take the talk window away. Opening it again is [`talk_open`], and the terminal that was in it is
/// untouched either way — a pane is a drawing of a session, not the session (`crate::pty`).
///
/// Nothing happens where there is no talk window: folding an app that is already one window back
/// into one window is what the user asked for, and it is already so.
#[tauri::command]
pub fn talk_close(app: tauri::AppHandle) -> Result<(), CmdError> {
    match app.get_webview_window(TALK) {
        Some(win) => win.destroy().map_err(failed),
        None => Ok(()),
    }
}

/// Where to put a window being split out: down and to the right of the board, in logical pixels.
///
/// `None` where the board cannot be asked (it has been closed, or the platform would not say), and
/// the window then comes up wherever the system puts it — which is worse than the offset and better
/// than not opening.
fn beside_the_board(app: &tauri::AppHandle) -> Option<(f64, f64)> {
    let board = app.get_webview_window(BOARD)?;
    let scale = board.scale_factor().ok()?;
    let at = board.outer_position().ok()?.to_logical::<f64>(scale);
    Some((at.x + TALK_OFFSET, at.y + TALK_OFFSET))
}

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
