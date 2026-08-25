//! The talk window's face while the app is up: where its panes are, which one is being worked in, and
//! what each is called.
//!
//! **None of it is kept** (`AMB-T-3687`). A frame is a place a terminal is drawn in, and the terminal
//! died with the last run — so a place that came back would be an empty box drawn exactly like the way
//! in beside it, and a named one would say that pressing carries on where the reader left off, which
//! nothing in the window can do. What outlives the run is what the person *set* rather than what they
//! opened: the split and the project ([`amenbo_core::frames::SavedLayout`], in the store's device row).
//!
//! **Which place each running terminal is drawn in is held here too**
//! ([`panes_drawn`](crate::frames::panes_drawn)), and for the
//! same reason: it pairs a session, which belongs to the process, with a place, which belongs to the
//! arrangement — and the one thing that can see both is whichever window is drawing the face. It is
//! what lets a task on the ledger name the pane it is being worked in
//! ([`task_pane`](crate::frames::task_pane), `AMB-D-758`).
//!
//! **It is held here, and not in either window, because the face moves between them.** The board and
//! the window a terminal is split out into are two webviews of one process: the arrangement is written
//! by whichever is drawing the face and read by the other as it comes up, which is how the second
//! window arrives with the same places, the same names and the reader in the same pane
//! (`AMB-D-753`, `AMB-T-3664`). A window reload lands in the same place — what is here is this
//! process's, and it goes when the process does.

use std::sync::Mutex;

use tauri::Emitter;

use amenbo_core::frames::{FrameName, FrameNames, NamedBy, SavedLayout};

use crate::commands::{open_store, open_store_read};
use crate::dto::{FrameNameDto, PaneDrawnDto, TalkLayoutDto, TaskPaneDto};
use crate::error::CmdError;

/// The face as this run has it: the arrangement both windows read, and the names on its frames.
///
/// Managed state, one for the whole app. Nothing here is written to the store — the two fields of the
/// arrangement that are ([`SavedLayout`]) go out through [`save_talk_layout`] as they change.
#[derive(Default)]
pub struct TalkFace {
    /// What this run calls its frames.
    names: Mutex<FrameNames>,
    /// The arrangement as the window drawing the face last had it, or nothing before either window
    /// has laid one out in this run.
    layout: Mutex<Option<TalkLayoutDto>>,
    /// Which place each running session is drawn in, as the face last said ([`panes_drawn`]). It is
    /// held apart from the arrangement above rather than inside it, because the two are different
    /// kinds of thing: the arrangement is a shape the windows hand between themselves, and a shape
    /// with a session written into it would say a terminal is running in a place the other window has
    /// not drawn yet (`app/src/talk/layout.ts`).
    panes: Mutex<Vec<PaneDrawnDto>>,
}

/// What this run calls the talk window's frames — the whole of it, since the window draws every frame
/// it has at once.
#[tauri::command]
pub fn frame_names(face: tauri::State<'_, TalkFace>) -> Vec<FrameNameDto> {
    named(face.names.lock().expect("frame names lock").all())
}

/// Name one frame, and answer with the names as they now stand.
///
/// The answer is the whole set rather than an acknowledgement, because a naming can be refused: a
/// person's name for a frame outranks the agent's and stays put (`amenbo_core::frames`). A caller that
/// drew what it asked for would show a name that is not the frame's.
#[tauri::command]
pub fn name_frame(
    face: tauri::State<'_, TalkFace>,
    frame: String,
    name: String,
    by: NamedBy,
) -> Vec<FrameNameDto> {
    named(face.names.lock().expect("frame names lock").name(&frame, &name, by))
}

/// The arrangement of the talk window, as this run has it — and where it has none yet, the split and
/// the project this device left behind.
///
/// It is answered as a window comes up and is what the face is laid out from. Nothing in it is
/// started: a frame is a place to open a terminal in, and a person presses for the ones they want
/// (`AMB-T-3607`). After a run of the app there are no frames in it at all — what comes back then is
/// one empty place on the project the reader was looking at.
#[tauri::command]
pub fn talk_layout(face: tauri::State<'_, TalkFace>) -> Result<Option<TalkLayoutDto>, CmdError> {
    if let Some(live) = face.layout.lock().expect("talk layout lock").clone() {
        return Ok(Some(live));
    }
    Ok(open_store_read()?.saved_layout()?.map(|kept| TalkLayoutDto {
        count: kept.count,
        // The ids of a run that has ended name nothing here, so this one starts its own at the first.
        next_id: 1,
        project: kept.project,
        frames: Vec::new(),
        split_out: None,
    }))
}

/// Keep the arrangement of the talk window, as the window drawing the face has it now.
///
/// The whole of it is held for the other window to read; the split and the project go on to the store,
/// which is the part a person gets back after the app has been closed. That write is made only where
/// one of the two has actually moved — the arrangement is kept on every press that changes the face,
/// and the pane being worked in changes far more often than the split does.
#[tauri::command]
pub fn save_talk_layout(
    face: tauri::State<'_, TalkFace>,
    layout: TalkLayoutDto,
) -> Result<(), CmdError> {
    let keep = SavedLayout { count: layout.count, project: layout.project };
    let moved = {
        let mut held = face.layout.lock().expect("talk layout lock");
        let moved = held
            .as_ref()
            .map_or(true, |was| was.count != keep.count || was.project != keep.project);
        *held = Some(layout);
        moved
    };
    if moved {
        open_store()?.save_layout(&keep)?;
    }
    Ok(())
}

/// Which place each running session is drawn in, as the face has it now.
///
/// **The whole set, every time**, the way the arrangement is written ([`save_talk_layout`]): what the
/// face is saying is where its terminals are, and a set that arrived one pairing at a time would leave
/// the host holding a pane that closed while nobody was listening.
///
/// It is the half of the way back from the ledger that neither end can supply alone. The volatile area
/// knows which session is holding a task and nothing about where it is drawn; the face knows where
/// every session is drawn and nothing about what it is holding (`AMB-D-758`). They meet in
/// [`task_pane`].
///
/// The board is told whenever this moves, because what it draws from it — the row on a task naming the
/// pane the work is happening in — has no other way to hear that a pane has closed.
#[tauri::command]
pub fn panes_drawn(app: tauri::AppHandle, face: tauri::State<'_, TalkFace>, panes: Vec<PaneDrawnDto>) {
    *face.panes.lock().expect("panes lock") = panes;
    if let Err(e) = app.emit_to(crate::windows::BOARD, PANES_EVENT, ()) {
        log::warn!("failed to emit {PANES_EVENT}: {e}");
    }
}

/// Told to the board when the panes the face is drawing have changed, so a task showing which pane it
/// is being worked in can ask again ([`task_pane`]).
///
/// It carries nothing. What changed is the whole set, and the reader wants the answer about **its own**
/// task rather than the set — so the payload would be thrown away by everyone who received it.
pub const PANES_EVENT: &str = "panes-changed";

/// The pane a task is being worked in, or nothing where it is being worked in none.
///
/// **Both halves have to speak** (`AMB-D-758`): the volatile area names the session holding the task,
/// and the face names the place that session is drawn in. Neither is guessed and neither is inferred
/// from the other — a task reserved from somebody's own terminal has no session at all, and a session
/// whose pane has closed has nowhere to send a reader. Both come back as no row on the task, which is
/// the honest answer: **"no pane is working on this here", never "nobody is working on this"**.
#[tauri::command]
pub fn task_pane(face: tauri::State<'_, TalkFace>, task: i64) -> Result<Option<TaskPaneDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("task_pane");
    let paths = amenbo_core::config::Paths::resolve()?;
    let Some(session) = amenbo_core::session_work::holder(&paths.sessions_dir, task) else {
        return Ok(None);
    };
    Ok(face
        .panes
        .lock()
        .expect("panes lock")
        .iter()
        .find(|pane| pane.session == session)
        .map(|pane| TaskPaneDto { session, label: pane.label.clone() }))
}

/// The frame names in the shape the webview reads them: a list, in frame order, rather than a map —
/// the window draws them in a row, and a map's order is the caller's to rebuild.
fn named(names: &std::collections::BTreeMap<String, FrameName>) -> Vec<FrameNameDto> {
    names
        .iter()
        .map(|(frame, named)| FrameNameDto {
            frame: frame.clone(),
            name: named.name.clone(),
            by: match named.by {
                NamedBy::Typed => "typed",
                NamedBy::Session => "session",
                NamedBy::Person => "person",
            },
        })
        .collect()
}
