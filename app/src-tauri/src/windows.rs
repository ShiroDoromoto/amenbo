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
//! **The window speaks twice, and both times about itself.** It says it drew
//! ([`talk_ready`](crate::windows::talk_ready)), which is what finishes the open, and it says it has gone
//! ([`TALK_CLOSED_EVENT`](crate::windows::TALK_CLOSED_EVENT)), which is what folds the app back. Between
//! them the board never has to guess at a window it cannot see, and that guessing is what left a reader
//! with a blank window and no way back to the terminal (`AMB-T-3701`, `AMB-T-3702`).
//!
//! **What is split out is the face and not a pane of it.** The window holds what the board's own
//! terminal face holds — the rail, the pages, the split, the files beside them — because the point
//! of the second window is to put the terminal on another display, and a face that lost its rail on
//! the way there would be a person carrying one pane out rather than moving the work
//! (`AMB-D-753`, `app/src/talk.tsx`).
//!
//! Everything that raises a window from outside the webview — a second launch turned away, a
//! notification clicked — means the **board**. What sends those is an arrival in the inbox or the
//! user asking for the app they already have open, and both of those are the board's subject. So
//! [`BOARD`](crate::windows::BOARD) is what they name, spelled once here rather than as a string in three files, and it is
//! the one window that is always there to name.
//!
//! A launch at login is the ordinary launch (`autostart`), so it comes up exactly as opening the app
//! does: the board, in whichever shape this machine was last used in.

use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::dto::RefTargetDto;
use crate::error::CmdError;

/// The window the ledger is read in — the app as it was before the second one existed, which is why
/// its label is still `main`: the label is what `tauri.conf.json` and every `get_webview_window`
/// call agree on, and renaming it would buy nothing.
pub const BOARD: &str = "main";

/// The window the agents run in, once the terminal has been split out of the board. What it draws is
/// the terminal face whole, so nothing about the arrangement is this side's to know.
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

/// How long a newly built talk window is given to say it drew its face before it is taken away
/// again.
///
/// It is the wait for a page to load rather than for anybody to do anything, so it is long by the
/// standards of the thing it measures and short by the standards of the person watching: a webview
/// coming up cold on a slow machine takes a second or two, and ten leaves room for several of those
/// without stranding a reader in front of an empty window for as long as it takes to wonder whether
/// the app is broken.
const TALK_DRAW_GRACE: Duration = Duration::from_secs(10);

/// The talk window's promise that it drew, kept for as long as the promise is outstanding.
///
/// **A window is not a face.** [`WebviewWindowBuilder::build`] answers when the OS has a window, and
/// says nothing about the page in it: a webview that never ran a line still leaves a window on the
/// screen, with the platform's own furniture on it and nothing else. Answering the board with `Ok`
/// there is how one blank window turned into an app the reader could not get the terminal back out
/// of — the board went on believing it was two windows, and every press meant for the face went to a
/// window that had nothing to show (`AMB-T-3701`).
///
/// So the window says so itself ([`talk_ready`], called by `app/src/talk.tsx`), and this is where the
/// half-built open waits to hear it. One sender, put here before the window is built and taken by
/// whichever comes first — the page saying it drew, or the wait running out.
#[derive(Default)]
pub struct TalkDrawn(Mutex<Option<SyncSender<()>>>);

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

/// Answer the clipboard permission a pane's paste raises, and answer nothing else.
///
/// **Linux is the one place a pane reads the clipboard itself.** WebKitGTK leaves the paste event's
/// `clipboardData` empty — measured on 2.50.4, 2.52.5 and 2.52.6 alike — so the image is fetched
/// with `navigator.clipboard.read()` instead, where the other two operating systems get it as a
/// `File` off the event (`AMB-D-854`, `AMB-T-4427`). That read is not something a page simply has:
/// the engine asks the embedder first, and an ask nobody is listening for is a refusal. Without
/// this the read never returns an image, on every window and every version.
///
/// **What is allowed is the clipboard alone.** The same signal carries geolocation, the camera, the
/// microphone and desktop notifications, so a handler that said yes to whatever it was handed would
/// give the page all of them. `webkit2gtk` 2.0.2 has no `ClipboardPermissionRequest` type to match
/// on, so the one request that is answered is picked out by the GType name the engine gives it;
/// everything else is left alone, and left alone is the refusal WebKit already defaults to.
///
/// Called for each window as it becomes one — the board in `setup` (`crate::run`), the talk window
/// where it is built ([`talk_open`]) — because the signal belongs to a `WebKitWebView` rather than
/// to the application, and the file panel and its panes are drawn in both.
#[cfg(target_os = "linux")]
pub fn allow_clipboard_read(win: &WebviewWindow) {
    // The handler is put on the raw view, which is reached on the thread the view lives on; a
    // failure to get there is a window whose panes will not paste an image, and nothing this side
    // can retry, so it is written down rather than returned.
    if let Err(e) = win.with_webview(|webview| {
        use webkit2gtk::glib::prelude::ObjectExt;
        use webkit2gtk::{PermissionRequestExt, WebViewExt};

        webview.inner().connect_permission_request(|_, req| {
            if req.type_().name() == "WebKitClipboardPermissionRequest" {
                req.allow();
                return true;
            }
            false
        });
    }) {
        log::warn!("clipboard permission goes unanswered in this window: {e}");
    }
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
/// **Nothing is handed over with it.** What the window draws is the whole terminal face, which reads
/// the arrangement this run holds (`crate::frames`) and takes up the terminals that are still
/// running — the same two questions it answers when the app folds back into one window. A split that
/// also passed a pane along would be a second, shorter-lived copy of an answer the process already
/// has.
///
/// **It is `async` because building a window on Windows cannot be done anywhere else.** A
/// synchronous command runs on the thread the event loop is on, and building a webview there waits
/// for a loop that is inside this call and so cannot answer — WebView2 hangs, and the app hangs with
/// it: the window appears with nothing drawn in it and nothing in the app responds again, including
/// the board behind it (`AMB-T-3701` measured it; tauri says so at
/// `WebviewWindowBuilder::new`, from <https://github.com/tauri-apps/wry/issues/583>). An `async`
/// command is run off that thread, which leaves the loop free to answer. macOS does not hang, so
/// nothing here reads as broken until it is run on Windows — the reason the word `async` is worth a
/// paragraph.
///
/// **What it answers `Ok` to is a face, not a window.** A window built around a page that never ran
/// is the failure the caller most needs told about and the one the builder cannot report, so this
/// waits for the page to say it drew ([`TalkDrawn`]) and takes the window away again if it does not.
/// The refusal then goes back the way a refused build does, and the board folds itself into one
/// window on the path it already had for that (`app/src/shell/AppShell.tsx`).
#[tauri::command]
pub async fn talk_open(
    app: tauri::AppHandle,
    drawn: tauri::State<'_, TalkDrawn>,
    raise: bool,
) -> Result<(), CmdError> {
    if let Some(win) = app.get_webview_window(TALK) {
        // A window that is there has already been through the wait below, so there is nothing left
        // to establish about it.
        if raise {
            raise_window(&win);
        }
        return Ok(());
    }
    // Put the ear out before there is anything that could speak into it: the page is built by the
    // line below, so nothing can announce itself earlier than this.
    let (tx, rx) = sync_channel::<()>(1);
    *drawn.0.lock().expect("talk drawn lock") = Some(tx);
    let mut builder = WebviewWindowBuilder::new(&app, TALK, WebviewUrl::App(TALK_URL.into()))
        // The product name, which is what shows for the moment before the page names itself in the
        // reader's own language (`app/src/talk.tsx`).
        .title("Amenbo")
        .inner_size(TALK_SIZE.0, TALK_SIZE.1)
        .min_inner_size(TALK_MIN_SIZE.0, TALK_MIN_SIZE.1)
        .resizable(true)
        // A window that was not asked for does not take the front. A launch restoring the shape this
        // machine was last used in builds this one, and the board is what the user is looking at.
        .focused(raise);
    // Nothing turns the OS drag handler off, which is what makes a file dragged in from the desktop
    // the application's rather than the page's (`crate::dropped`, `AMB-D-775`). The board says the
    // same thing in its own words (`dragDropEnabled` in `tauri.conf.json`), and the two must agree:
    // the file panel is drawn in both windows, and a panel that answered a drop in one of them and
    // not the other would be the same panel behaving differently for no reason a reader could see.
    if let Some((x, y)) = beside_the_board(&app) {
        builder = builder.position(x, y);
    }
    let win = builder.build().map_err(failed)?;
    // The panes drawn in here read the clipboard the same way the board's do, so this window is
    // given the same answer to the same ask.
    #[cfg(target_os = "linux")]
    allow_clipboard_read(&win);
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
    // Waited for off this thread. The wait is seconds long where it is a wait at all, and the runtime
    // this command was given has other commands to answer in the meantime — including the ones the
    // page being waited for makes as it comes up.
    let heard = tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(TALK_DRAW_GRACE).is_ok())
        .await
        .unwrap_or(false);
    if !heard {
        // Nothing was drawn, so there is nothing for the reader in this window — and leaving it up
        // would leave the board pressing "terminal" at it forever. Taking it away is also what tells
        // the board, which is listening for exactly that (`TALK_CLOSED_EVENT`).
        let _ = talk_close(app);
        return Err(blank());
    }
    Ok(())
}

/// The talk window saying it drew its face, which is the only thing that makes it a face rather than
/// a window (`app/src/talk.tsx` calls it once the page has something on it).
///
/// Nothing happens when nobody is waiting — a reload of a window that is already up says it again,
/// and the second saying has no open to finish.
#[tauri::command]
pub fn talk_ready(drawn: tauri::State<'_, TalkDrawn>) {
    if let Some(tx) = drawn.0.lock().expect("talk drawn lock").take() {
        let _ = tx.try_send(());
    }
}

/// Bring the talk window forward, and say whether there was one to bring.
///
/// The board asks this instead of [`talk_open`] when it already believes it is two windows: what it
/// wants then is the window it thinks it has, and building a second one behind a belief that turned
/// out to be wrong would open a window nobody pressed for. `false` is the board's cue to fold itself
/// back and put the terminal on a face of its own, which is where the reader was trying to get.
#[tauri::command]
pub fn talk_raise(app: tauri::AppHandle) -> bool {
    match app.get_webview_window(TALK) {
        Some(win) => {
            raise_window(&win);
            true
        }
        None => false,
    }
}

/// Bring a window forward, spelled the way a notification click spells it (`crate::macos_notify`):
/// a window that is merely unfocused and a window that is minimized both look, to the user, like the
/// terminal not being there.
fn raise_window(win: &WebviewWindow) {
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
}

/// A window that was built and then drew nothing, turned into the refusal the board is given.
///
/// It is a code of its own rather than a [`failed`] with a reason in it, because it is not the
/// platform refusing anything: everything worked and the result is still an empty window, and the
/// sentence a reader gets has to say that rather than quote an error nobody produced.
fn blank() -> CmdError {
    CmdError::coded(
        "talk_blank",
        "That window opened but never drew anything, so the terminal was put back in this one."
            .to_string(),
        serde_json::json!({}),
    )
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

/// The event the terminal face goes to a pane on, when one is asked for from the ledger.
const SHOW_PANE_EVENT: &str = "pane-activated";

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

/// Go to the pane a task is being worked in, asked for from the ledger.
///
/// **It is [`show_ref`] the other way round**, and it is here for the same reason: the two faces can be
/// in two windows, and a window cannot raise its sibling. So the road out of one and into the other
/// runs through the process that owns them both.
///
/// Which window is told is which window has the face. Split out, that is the talk window, and it is
/// brought forward — the reader asked to see the pane, and a window behind other work is a pane they
/// cannot see. In one window the board already has the keyboard: what has to change there is the face,
/// which is the board's own to change (`app/src/shell/AppShell.tsx`).
///
/// **What travels is the session and nothing else.** Where that session is drawn is the face's
/// (`crate::frames::panes_drawn`), and a place named out here would be a second answer free to go stale
/// against the one the face is drawing from.
#[tauri::command]
pub fn show_pane(app: tauri::AppHandle, session: String) {
    let face = match app.get_webview_window(TALK) {
        Some(win) => {
            raise_window(&win);
            TALK
        }
        None => BOARD,
    };
    // Logged rather than returned, as `show_ref` does: by now the window is already in front, and a
    // refusal handed back would reach a press with no way to report one.
    if let Err(e) = app.emit_to(face, SHOW_PANE_EVENT, session) {
        log::warn!("failed to emit {SHOW_PANE_EVENT}: {e}");
    }
}
