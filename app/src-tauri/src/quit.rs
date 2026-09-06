//! The way out of the app, and the one question asked on the way.
//!
//! **Ending the app ends every terminal in it, and none of them comes back.** A session is the
//! process's, not a window's or a pane's: quitting takes down the agents that were running, drops
//! whatever they had not written yet, and leaves the volatile area for the next launch's `sweep` to
//! empty (`crate::pty`). Closing a single pane has asked about this for a while
//! (`app/src/shell/HoldingAsk.tsx`); closing all of them at once had not been asked about at all.
//!
//! **The question is the board's, not this side's.** What has to be said names the tasks a session
//! reserved, in the reader's language, and both of those live in the webview: the dictionary is the
//! front end's (`AMB-D-396` carves out the menu bar and nothing else), and a task's ref is spelled
//! by the same code that spells it everywhere else. So this module decides *whether* to ask and
//! hands the asking over — the board raises the box, gathers what is held, and comes back through
//! [`app_quit`](crate::quit::app_quit) when the person has answered.
//!
//! **Silence when there is nothing to lose.** No terminal open means no question: the app ends on
//! the gesture that asked for it, with nothing in the way.

use tauri::{Emitter, Manager};

use crate::pty::Terminals;
use crate::windows::BOARD;

/// Menu id of the item that ends the app — the one `⌘Q` / `Ctrl+Q` reaches (`crate::menu`).
///
/// It is an item of this app's own rather than the platform's predefined quit, and that is the whole
/// reason it exists: the predefined one is wired to the OS's own terminate on macOS, which ends the
/// process without the run loop ever offering the embedder a say. An item with an id is a click this
/// side hears (`crate::run`'s `on_menu_event`), on all three operating systems, which is what makes
/// the question below possible at all.
pub const QUIT_ID: &str = "quit";

/// Told to the board when the app was asked to end and something is still running in a pane.
///
/// Carries nothing. What the box has to show is what is held *now*, and the board reads that itself
/// (`session_work`, the same call the way out of a single pane makes) rather than take a copy that
/// was true a moment ago.
pub const QUIT_ASKED_EVENT: &str = "quit://asked";

/// The app was asked to end. Ask about it first if there is anything to lose, and otherwise end.
///
/// The board is raised before the box is raised on it, because the gesture may well have been made
/// with the terminal window in front — a question drawn behind what the reader is looking at is a
/// window that has stopped responding as far as they can tell.
///
/// With no board to ask in, the app ends. That is the honest answer rather than a refusal: the
/// question has nowhere to be drawn, and a quit that silently did nothing would leave the reader
/// pressing it again.
pub fn requested(app: &tauri::AppHandle) {
    if app.state::<Terminals>().open() == 0 {
        app.exit(0);
        return;
    }
    match app.get_webview_window(BOARD) {
        Some(board) => {
            crate::windows::raise_window(&board);
            let _ = app.emit_to(BOARD, QUIT_ASKED_EVENT, ());
        }
        None => app.exit(0),
    }
}

/// Whether the close just pressed on `label` is the app ending, with terminals still open.
///
/// `true` means the caller holds the close off (`prevent_close`) — the question has been put to the
/// board, and [`app_quit`] is what closes it if the answer is yes.
///
/// **Only the board, and only when it is the last window.** Closing the board while the terminal
/// window is up is not the app ending — the process goes on and the sessions with it — so the close
/// stands. The talk window is never this: it folds back into the board and leaves every session
/// running (`crate::windows`).
pub fn ask_before_this_close(app: &tauri::AppHandle, label: &str) -> bool {
    if label != BOARD || app.webview_windows().len() > 1 {
        return false;
    }
    if app.state::<Terminals>().open() == 0 {
        return false;
    }
    let _ = app.emit_to(BOARD, QUIT_ASKED_EVENT, ());
    true
}

/// End the app, now that the person has said so (`app/src/shell/AppShell.tsx`).
///
/// Nothing is tidied up here. What the answer meant for the reservations a session was holding has
/// already happened on the other side — handed back, or deliberately left standing — and the
/// terminals themselves are ended by the process going away, which is what was being asked about.
#[tauri::command]
pub fn app_quit(app: tauri::AppHandle) {
    app.exit(0);
}
