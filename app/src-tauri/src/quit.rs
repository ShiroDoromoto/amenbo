//! The way out of the app, and the one question asked on the way.
//!
//! **Ending the app ends every terminal in it, and what does not come back is the running.** A
//! session is the process's, not a window's or a pane's: quitting takes down the agents that were
//! running and drops whatever they had not written yet (`crate::pty`). The talk itself is a
//! different thing and it does come back — each place is opened again on the session it was left in
//! (`AMB-D-869`, `crate::frames`) — so what the question is about is the work in flight, and the
//! sentence says so (`app/src/core/i18n`). Closing a single pane has asked about this for a while
//! (`app/src/shell/TerminalPane.tsx`); closing all of them at once had not been asked about at all.
//!
//! **What is asked is whether a terminal is going, and never what one was doing** (`AMB-D-858`).
//! Tying a pane to a task went through a key the world could rewrite behind the pane, so a question
//! naming what was about to be lost named as often work somebody had already finished elsewhere.
//! A reservation left standing is on the ledger, where `amenbo task list` finds it.
//!
//! **The question is the board's, not this side's.** The sentence is in the reader's language, and
//! the dictionary is the front end's (`AMB-D-396` carves out the menu bar and nothing else). So this
//! module decides *whether* to ask and hands the asking over — the board raises the question and
//! comes back through [`app_quit`](crate::quit::app_quit) when the person has answered.
//!
//! **Silence when there is nothing to lose.** No terminal open means no question: the app ends on
//! the gesture that asked for it, with nothing in the way.
//!
//! **Quiet is not the same as immediate.** A page that keeps what is typed on its own writes it a
//! moment after the typing settles, so at any instant there can be a sentence that is on the screen
//! and not yet in the store (`app/src/files/MemoPage.tsx`). `exit` takes the process with whatever
//! that moment was about to do, and no page is unloaded on the way — so the last thing this module
//! does before ending is ask every window for what it has not written, and wait to be answered.
//! The wait is a backstop, not a pause: the app ends the moment the last window answers.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

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
/// Carries nothing. Whether there is anything to lose was settled on this side before it was sent,
/// and what the question says beyond that is the same sentence every time.
pub const QUIT_ASKED_EVENT: &str = "quit://asked";

/// Told to every window once the app is ending for certain: write what is unwritten and say so.
///
/// Carries nothing, and is not a question — whether to end has been settled by the time it goes out.
/// A window answers it with [`quit_written`], whether or not it had anything to write.
pub const GOING_EVENT: &str = "quit://going";

/// How long the windows are given to answer [`GOING_EVENT`] before the app ends without them.
///
/// It is a backstop rather than a wait: the app ends the moment the last window has answered, and a
/// window that cannot answer — one whose page has not drawn yet, or has stopped — must not be able
/// to hold the quit open. Writing a page is 23ms of store open and 0.2ms of write (`AMB-T-4461`),
/// so a window with something to write is back long inside this.
const WRITE_GRACE: Duration = Duration::from_millis(500);

/// How many windows still owe an answer to [`GOING_EVENT`]. Only ever above zero while ending.
static OWED: AtomicUsize = AtomicUsize::new(0);

/// The app is ending: ask every window for what it has not written, and end once they have answered.
///
/// With no window to ask, and where the ask cannot be sent, the app ends now — there is nothing that
/// could be holding a sentence, and a quit that waited on an answer nobody can give would be a quit
/// that never happened.
fn ending(app: &tauri::AppHandle) {
    let windows = app.webview_windows().len();
    if windows == 0 {
        app.exit(0);
        return;
    }
    // Counted before the ask goes out, and not after: a window that answers straight away — which is
    // every window with nothing to write — would otherwise have its answer overwritten by the count,
    // and the quit would sit out the whole backstop for nothing.
    OWED.store(windows, Ordering::SeqCst);
    if app.emit(GOING_EVENT, ()).is_err() {
        app.exit(0);
        return;
    }
    let waited = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(WRITE_GRACE);
        let ending = waited.clone();
        let _ = waited.run_on_main_thread(move || ending.exit(0));
    });
}

/// A window has written what it had (`app/src/core/unwritten.ts`). The app ends on the last answer.
///
/// Answers beyond the ones asked for are ignored rather than counted: this is a door the webview can
/// press at any time, and one pressed outside an ending is not a window that has just gone quiet.
#[tauri::command]
pub fn quit_written(app: tauri::AppHandle) {
    let last = OWED.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |owed| owed.checked_sub(1)) == Ok(1);
    if last {
        app.exit(0);
    }
}

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
        ending(app);
        return;
    }
    match app.get_webview_window(BOARD) {
        Some(board) => {
            crate::windows::raise_window(&board);
            let _ = app.emit_to(BOARD, QUIT_ASKED_EVENT, ());
        }
        None => ending(app),
    }
}

/// Whether the close just pressed on `label` is the app ending rather than a window going.
///
/// `true` means the caller holds the close off (`prevent_close`) and takes the way out
/// ([`requested`]) instead of letting the window go: the same way out the menu's quit takes, which
/// asks where there is something to lose and, either way, lets the windows write what is unwritten
/// before the process goes. Letting the close stand would end the app the moment the last window
/// went, with neither of those.
///
/// **Only the board, and only when it is the last window.** Closing the board while the terminal
/// window is up is not the app ending — the process goes on and the sessions with it — so the close
/// stands. The talk window is never this: it folds back into the board and leaves every session
/// running (`crate::windows`).
pub fn is_the_app_ending(app: &tauri::AppHandle, label: &str) -> bool {
    label == BOARD && app.webview_windows().len() == 1
}

/// End the app, now that the person has said so (`app/src/shell/AppShell.tsx`).
///
/// Nothing is tidied up here beyond the one thing that cannot be redone afterwards. What the answer
/// meant for the reservations a session was holding has already happened on the other side — handed
/// back, or deliberately left standing — and the terminals themselves are ended by the process going
/// away, which is what was being asked about. What is left is the sentence a page has typed and not
/// yet written, which nothing else would carry, so the windows are asked for it first ([`ending`]).
#[tauri::command]
pub fn app_quit(app: tauri::AppHandle) {
    ending(&app);
}
