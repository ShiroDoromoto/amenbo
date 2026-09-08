//! The talk window's face while the app is up: where its panes are, which one is being worked in, and
//! what each is called.
//!
//! **None of it is kept** (`AMB-T-3687`). A frame is a place a terminal is drawn in, and the terminal
//! died with the last run — so a place that came back would be an empty box drawn exactly like the way
//! in beside it, and a named one would say that pressing carries on where the reader left off, which
//! nothing in the window can do. What outlives the run is what the person *set* rather than what they
//! opened: how each project's page is split, and which project they were on
//! ([`amenbo_core::frames::SavedLayout`], in the store's device row).
//!
//! **It is held here, and not in either window, because the face moves between them.** The board and
//! the window a terminal is split out into are two webviews of one process: the arrangement is written
//! by whichever is drawing the face and read by the other as it comes up, which is how the second
//! window arrives with the same places, the same names and the reader in the same pane
//! (`AMB-D-753`, `AMB-T-3664`). A window reload lands in the same place — what is here is this
//! process's, and it goes when the process does.

use std::sync::Mutex;

use amenbo_core::frames::{FrameName, FrameNames, NamedBy, Orient, SavedLayout, Split};

use crate::commands::{open_store, open_store_read};
use crate::dto::{FrameNameDto, TalkLayoutDto};
use crate::error::CmdError;

/// The face as this run has it: the arrangement both windows read, and the names on its frames.
///
/// Managed state, one for the whole app. Nothing here is written to the store — the parts of the
/// arrangement that are ([`SavedLayout`]) go out through [`save_talk_layout`] as they change.
#[derive(Default)]
pub struct TalkFace {
    /// What this run calls its frames.
    names: Mutex<FrameNames>,
    /// The arrangement as the window drawing the face last had it, or nothing before either window
    /// has laid one out in this run.
    layout: Mutex<Option<TalkLayoutDto>>,
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

/// The arrangement of the talk window, as this run has it — and where it has none yet, the splits and
/// the project this device left behind.
///
/// It is answered as a window comes up and is what the face is laid out from. Nothing in it is
/// started: a frame is a place to open a terminal in, and a person presses for the ones they want
/// (`AMB-T-3607`). After a run of the app there are no frames in it at all — what comes back then is
/// one empty place on the project the reader was looking at.
///
/// **`count` is the opening project's own split, and a count of nothing where it has none.** What a
/// project nobody has split opens at is the window's to say, not this side's: `restored` reads a
/// count it does not know as the one it lays a fresh project out at (`app/src/talk/layout.ts`).
#[tauri::command]
pub fn talk_layout(face: tauri::State<'_, TalkFace>) -> Result<Option<TalkLayoutDto>, CmdError> {
    if let Some(live) = face.layout.lock().expect("talk layout lock").clone() {
        return Ok(Some(live));
    }
    Ok(open_store_read()?.saved_layout()?.map(|kept| {
        // What the face opens at, which is the split of the project it opens on. A project with no
        // answer kept opens at whatever the window lays out for one, which is the window's to decide
        // (`app/src/talk/layout.ts`) — nothing here invents a count for it.
        let opening = kept.project.and_then(|project| kept.splits.get(&project)).copied();
        TalkLayoutDto {
            count: opening.map_or(0, |split| split.count),
            orient: Some(opening.unwrap_or_default().orient.into()),
            splits: kept.splits.iter().map(|(project, split)| (*project, (*split).into())).collect(),
            // The ids of a run that has ended name nothing here, so this one starts its own at the
            // first.
            next_id: 1,
            project: kept.project,
            frames: Vec::new(),
            split_out: None,
        }
    }))
}

/// Keep the arrangement of the talk window, as the window drawing the face has it now.
///
/// The whole of it is held for the other window to read; the splits and the project go on to the
/// store, which is the part a person gets back after the app has been closed. That write is made only
/// where one of the two has actually moved — the arrangement is kept on every press that changes the
/// face, and the pane being worked in changes far more often than a split does.
///
/// **That guard is what lets a half-written sentence ride along.** The arrangement is sent again on
/// every keystroke in a box under a pane ([`crate::dto::TalkFrameDto::written`]), and none of those
/// reach the disk: what a keystroke moved is not a split and not the project, so the whole of it
/// stops in the mutex above.
#[tauri::command]
pub fn save_talk_layout(
    face: tauri::State<'_, TalkFace>,
    layout: TalkLayoutDto,
) -> Result<(), CmdError> {
    let keep = SavedLayout { project: layout.project, splits: splits_of(&layout) };
    let moved = {
        let mut held = face.layout.lock().expect("talk layout lock");
        let moved = held.as_ref().map_or(true, |was| {
            splits_of(was) != keep.splits || was.project != keep.project
        });
        *held = Some(layout);
        moved
    };
    if moved {
        open_store()?.save_layout(&keep)?;
    }
    Ok(())
}

/// Which way the arrangement says a two-pane page sits. An arrangement that says nothing sits the way
/// every other count does — the face writes the answer only once there is one.
fn orient_of(layout: &TalkLayoutDto) -> Orient {
    layout.orient.map_or(Orient::default(), Into::into)
}

/// The splits an arrangement is keeping, whichever shape the window sent.
///
/// **A window that sends the set is answered by the set.** One that does not is a window telling the
/// host about the project it is showing and no other, so its one `count` is put back under that
/// project — which is exactly where it came from, and leaves every other project's answer alone.
fn splits_of(layout: &TalkLayoutDto) -> std::collections::BTreeMap<u32, Split> {
    if !layout.splits.is_empty() {
        return layout.splits.iter().map(|(project, split)| (*project, (*split).into())).collect();
    }
    layout
        .project
        .map(|project| {
            std::collections::BTreeMap::from([(
                project,
                Split { count: layout.count, orient: orient_of(layout) },
            )])
        })
        .unwrap_or_default()
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
                NamedBy::Session => "session",
                NamedBy::Person => "person",
            },
        })
        .collect()
}
