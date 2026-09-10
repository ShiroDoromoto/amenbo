//! The talk window's face while the app is up: where its panes are, which one is being worked in, and
//! what each is called.
//!
//! **The places come back and the sessions do not** (`AMB-D-869`). What outlives a run is a row a
//! pane — where it works, what was started in it, what it is called and the handle it is resumed
//! from — kept in the store's device row ([`amenbo_core::frames::SavedLayout`]). The process that
//! was drawing into the pane died with the run, so the row is a way back in rather than a picture of
//! one, and nothing here starts anything: which of the panes is woken, and when, is the window's
//! (`AMB-T-4641`).
//!
//! **What is this run's alone stays here**: the half-written sentence under each pane, and which pane
//! is being worked in. Both are about a person's place on a screen that is up, and an older write
//! must not move either.
//!
//! **It is held here, and not in either window, because the face moves between them.** The board and
//! the window a terminal is split out into are two webviews of one process: the arrangement is written
//! by whichever is drawing the face and read by the other as it comes up, which is how the second
//! window arrives with the same places, the same names and the reader in the same pane
//! (`AMB-D-753`, `AMB-T-3664`). A window reload lands in the same place — what is here is this
//! process's, and it goes when the process does.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use amenbo_core::frames::{FrameName, FrameNames, NamedBy, Orient, SavedLayout, SavedPane, Split};

use crate::commands::{open_store, open_store_read};
use crate::dto::{FrameNameDto, TalkFrameDto, TalkLayoutDto};
use crate::error::CmdError;

/// The face as this run has it: the arrangement both windows read, the names on its frames, and the
/// handles the sessions in them are resumed from.
///
/// Managed state, one for the whole app. What of it is kept goes out through [`keep`] as it changes,
/// assembled from all three: the window sends the places, and the two maps beside them are what the
/// window never holds.
#[derive(Default)]
pub struct TalkFace {
    /// What this run calls its frames — read back out of the store's panes as the first window of a
    /// run comes up ([`talk_layout`]).
    names: Mutex<FrameNames>,
    /// The handle each pane's provider is resumed from, by frame — a session id for most of them, and
    /// the path of a home of its own for `codex` (`AMB-D-869`).
    ///
    /// **It is held beside the arrangement rather than in it** for the reason the names are: it is
    /// the host that issues one, as it starts the session (`AMB-T-4639`, `AMB-T-4640`), and a value
    /// that made the round trip through the window would be one a window could write. What is in here
    /// now is what came back from the store, which is what keeps a handle from being dropped by the
    /// next write of the arrangement.
    hints: Mutex<BTreeMap<String, String>>,
    /// The arrangement as the window drawing the face last had it, or nothing before either window
    /// has laid one out in this run.
    layout: Mutex<Option<TalkLayoutDto>>,
    /// What was last written to the store, so a write is made only where something it holds has
    /// actually moved.
    kept: Mutex<Option<SavedLayout>>,
    /// Whether the panes the store kept have been read back into the two maps above. Once, per run:
    /// both windows read the arrangement as they come up, and a second reading would put a name back
    /// over one a person had changed in between.
    seeded: AtomicBool,
}

impl TalkFace {
    /// The handle this frame comes back on, where one was written down **for this provider**.
    ///
    /// It is what a pane is opened on when the frame already had a session — the one the person left
    /// there, whether a run ago or a moment ago (`AMB-D-869`).
    ///
    /// **The provider is asked about because a handle is only a handle to the one that issued it.**
    /// A frame the person opened Claude Code in and is now opening OpenCode in still holds Claude's
    /// uuid, and OpenCode handed that would refuse to start. Which provider the handle belongs to is
    /// the agent on the row it came back on — the row this frame was last written down as.
    pub fn comes_back_on(&self, frame: &str, agent: &str) -> Option<String> {
        let kept = self.kept.lock().expect("kept layout lock");
        let pane = kept.as_ref()?.panes.iter().find(|pane| pane.id == frame)?;
        if pane.agent.as_deref() != Some(agent) {
            return None;
        }
        self.hints.lock().expect("resume hints lock").get(frame).cloned()
    }

    /// Every handle written down in this run — what a pane reading one back out of a provider's own
    /// list has to pick around ([`amenbo_core::agent_sessions::newest_in`]).
    pub fn resume_hints(&self) -> Vec<String> {
        self.hints.lock().expect("resume hints lock").values().cloned().collect()
    }

    /// Write down the handle a pane's provider is resumed from, as the pane is started
    /// (`crate::pty::pty_open`).
    ///
    /// **The host writes it and no window carries it**, which is why it is a door of its own rather
    /// than a field of the arrangement: a value that made the round trip through a webview would be
    /// one a webview could write, and what this holds is the way back into somebody's conversation.
    ///
    /// **The row is written here rather than left to the next arrangement.** For a handle settled as
    /// the pane starts, the pane's own opening would bring one about — but one provider names its
    /// own handle and is asked for it seconds later (`crate::agent_sessions`), by which time the
    /// window has sent its arrangement and has no reason to send another. Where no arrangement has
    /// been sent yet there is no row to write onto, and the window's first one carries it.
    pub fn resumed_from(&self, frame: &str, handle: String) {
        self.hints.lock().expect("resume hints lock").insert(frame.to_string(), handle);
        let Some(layout) = self.layout.lock().expect("talk layout lock").clone() else {
            return;
        };
        if let Err(e) = keep(self, &layout) {
            log::warn!("could not write down the way back into frame {frame}: {e:?}");
        }
    }

    /// Take back the way into a frame, where what was written down leads nowhere
    /// (`crate::pty::pty_open`).
    ///
    /// **A handle earns its place by a session running under it, and one is written before the
    /// program starts** — that is what keeps a quit in between from costing the pane its way back
    /// ([`resumed_from`](Self::resumed_from)). The other side of writing early is a program that
    /// never came up: the handle then names a session nothing ever made, and the next run opens the
    /// pane on it and is refused in the same breath. Nothing gets better on the run after that, so
    /// the row is cleared and the pane comes up on a session of its own instead.
    pub fn gave_up(&self, frame: &str) {
        if self.hints.lock().expect("resume hints lock").remove(frame).is_none() {
            return;
        }
        let Some(layout) = self.layout.lock().expect("talk layout lock").clone() else {
            return;
        };
        if let Err(e) = keep(self, &layout) {
            log::warn!("could not take back the way into frame {frame}: {e:?}");
        }
    }
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
    let now = named(face.names.lock().expect("frame names lock").name(&frame, &name, by));
    // And the name goes down on that pane's row, where it is read back from on the next run. The
    // write is made from here because a naming moves nothing else: the window sends the arrangement
    // as the *shape* of the face changes, and what a pane is called is not part of that shape.
    if let Some(layout) = face.layout.lock().expect("talk layout lock").clone() {
        let _ = keep(&face, &layout);
    }
    now
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
        seed(&face, &kept);
        TalkLayoutDto {
            count: opening.map_or(0, |split| split.count),
            orient: Some(opening.unwrap_or_default().orient.into()),
            splits: kept.splits.iter().map(|(project, split)| (*project, (*split).into())).collect(),
            next_id: kept.next_id,
            project: kept.project,
            // The places, with nothing running in any of them: a session died with the run that
            // started it, and what the window draws is the offer to carry on (`AMB-D-869`).
            frames: kept
                .panes
                .iter()
                .map(|pane| TalkFrameDto {
                    id: pane.id.clone(),
                    project: Some(pane.project),
                    folder: pane.folder.clone(),
                    agent: pane.agent.clone(),
                    written: None,
                    // The one thing this run knows about the place that the window cannot work out:
                    // whether the last run left a way into what was running in it. The window opens
                    // those without being pressed (`AMB-T-4641`).
                    resumes: pane.resume.is_some(),
                })
                .collect(),
            // Which pane was being worked in is this run's: it is where a reader is looking, and the
            // last run has nothing to say about that.
            split_out: None,
        }
    }))
}

/// Keep the arrangement of the talk window, as the window drawing the face has it now.
///
/// The whole of it is held for the other window to read, and what outlives the run goes on to the
/// store ([`keep`]).
#[tauri::command]
pub fn save_talk_layout(
    face: tauri::State<'_, TalkFace>,
    layout: TalkLayoutDto,
) -> Result<(), CmdError> {
    *face.layout.lock().expect("talk layout lock") = Some(layout.clone());
    keep(&face, &layout)
}

/// Take the halves of a pane's row no window holds — what it is called, and the way back into what
/// was running in it — out of what the store kept.
///
/// **Once a run.** Both windows read the arrangement as they come up, and a second reading would put
/// a name back over one a person had changed in between. What was read is remembered as written as
/// well, so the window's first arrangement — the same row, come back around — is not written out
/// again.
fn seed(face: &TalkFace, kept: &SavedLayout) {
    if face.seeded.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut names = face.names.lock().expect("frame names lock");
    let mut hints = face.hints.lock().expect("resume hints lock");
    for pane in &kept.panes {
        if let Some(name) = &pane.name {
            names.name(&pane.id, &name.name, name.by);
        }
        if let Some(resume) = &pane.resume {
            hints.insert(pane.id.clone(), resume.clone());
        }
    }
    *face.kept.lock().expect("kept layout lock") = Some(kept.clone());
}

/// Write down what outlives the run, where anything in it has moved.
///
/// **The guard is what lets a half-written sentence ride along.** The arrangement is sent again on
/// every keystroke in a box under a pane ([`crate::dto::TalkFrameDto::written`]), and none of those
/// reach the disk: what a keystroke moved is not part of what is kept, so the row comes out
/// identical to the one already written and the write is not made.
///
/// What was written is remembered only once the store has taken it, so a write that failed is made
/// again by the next change rather than counted as done.
fn keep(face: &TalkFace, layout: &TalkLayoutDto) -> Result<(), CmdError> {
    forget_dropped(face, layout);
    let keeping = SavedLayout {
        project: layout.project,
        splits: splits_of(layout),
        panes: panes_of(face, layout),
        next_id: layout.next_id,
    };
    if face.kept.lock().expect("kept layout lock").as_ref() == Some(&keeping) {
        return Ok(());
    }
    open_store()?.save_layout(&keeping)?;
    *face.kept.lock().expect("kept layout lock") = Some(keeping);
    Ok(())
}

/// Let go of the handles of panes the arrangement no longer has, and of whatever they were holding
/// open.
///
/// A pane that is closed is closed for good: its row goes with it ([`panes_of`] keeps only the places
/// the window sent), so a handle left behind here would be one nothing could ever hand back. For most
/// providers letting go is the whole of it — the handle is a session id, and what it names is the
/// provider's to keep or forget. Where it is a directory Amenbo made, that comes away too rather
/// than being left to pile up on the machine (`crate::pane_home::forget`) — the pane's own home,
/// and with it the conversation nothing can reach any more.
fn forget_dropped(face: &TalkFace, layout: &TalkLayoutDto) {
    let here: std::collections::BTreeSet<&str> =
        layout.frames.iter().map(|frame| frame.id.as_str()).collect();
    face.hints.lock().expect("resume hints lock").retain(|frame, handle| {
        if here.contains(frame.as_str()) {
            return true;
        }
        crate::pane_home::forget(std::path::Path::new(handle));
        false
    });
}

/// The panes as they are written down: what the window sent about each place, and beside it the two
/// halves the window never holds — what the frame is called, and the handle it is resumed from.
///
/// A frame the window sends with no project is let go rather than kept under a guess. A pane belongs
/// to a project and is drawn on that project's page, so one with nowhere to be put back is one no
/// window could draw again.
fn panes_of(face: &TalkFace, layout: &TalkLayoutDto) -> Vec<SavedPane> {
    let names = face.names.lock().expect("frame names lock");
    let hints = face.hints.lock().expect("resume hints lock");
    layout
        .frames
        .iter()
        .filter_map(|frame| {
            Some(SavedPane {
                id: frame.id.clone(),
                project: frame.project?,
                folder: frame.folder.clone(),
                agent: frame.agent.clone(),
                name: names.all().get(&frame.id).cloned(),
                resume: hints.get(&frame.id).cloned(),
            })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A place as the window sends one over.
    fn frame(id: &str, agent: Option<&str>) -> TalkFrameDto {
        TalkFrameDto {
            id: id.to_string(),
            project: Some(1),
            folder: Some("/work/repo".to_string()),
            agent: agent.map(str::to_string),
            written: Some("half a sentence".to_string()),
            resumes: false,
        }
    }

    fn layout(frames: Vec<TalkFrameDto>) -> TalkLayoutDto {
        TalkLayoutDto {
            count: 2,
            orient: None,
            splits: std::collections::BTreeMap::new(),
            next_id: 3,
            project: Some(1),
            frames,
            split_out: Some("1".to_string()),
        }
    }

    /// A row is the window's places joined to what only the host has: the name on the frame, and the
    /// handle the session in it is resumed from.
    #[test]
    fn a_row_is_the_window_and_the_host_together() {
        let face = TalkFace::default();
        face.names.lock().unwrap().name("1", "the migration", NamedBy::Person);
        face.hints.lock().unwrap().insert("1".to_string(), "0f9c".to_string());

        let panes = panes_of(&face, &layout(vec![frame("1", Some("claude")), frame("2", None)]));

        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].agent.as_deref(), Some("claude"));
        assert_eq!(panes[0].name.as_ref().map(|named| named.name.as_str()), Some("the migration"));
        assert_eq!(panes[0].name.as_ref().map(|named| named.by), Some(NamedBy::Person));
        assert_eq!(panes[0].resume.as_deref(), Some("0f9c"));
        // And a pane the host has nothing to say about is a row of what the window sent.
        assert_eq!(panes[1].name, None);
        assert_eq!(panes[1].resume, None);
    }

    /// A pane the arrangement no longer has is a pane whose way back goes with it: the handle is let
    /// go of rather than held for a frame nothing will draw again.
    #[test]
    fn a_pane_that_is_gone_lets_go_of_its_handle() {
        let face = TalkFace::default();
        face.resumed_from("1", "0f9c".to_string());
        face.resumed_from("2", "7b2e".to_string());

        forget_dropped(&face, &layout(vec![frame("1", Some("claude"))]));

        let hints = face.hints.lock().unwrap();
        assert_eq!(hints.get("1").map(String::as_str), Some("0f9c"));
        assert_eq!(hints.get("2"), None);
    }

    /// A handle whose program never came up is taken back off the frame, and the frames beside it
    /// keep theirs.
    #[test]
    fn a_way_back_that_leads_nowhere_is_taken_back() {
        let face = TalkFace::default();
        face.resumed_from("1", "0f9c".to_string());
        face.resumed_from("2", "7b2e".to_string());

        face.gave_up("1");

        let hints = face.hints.lock().unwrap();
        assert_eq!(hints.get("1"), None);
        assert_eq!(hints.get("2").map(String::as_str), Some("7b2e"));
    }

    /// A place the window sends with no project is let go rather than kept under a guess: a pane is
    /// drawn on its project's page, so one with nowhere to be put back is one nothing could draw.
    #[test]
    fn a_place_with_no_project_is_not_kept() {
        let face = TalkFace::default();
        let mut orphan = frame("1", None);
        orphan.project = None;

        assert!(panes_of(&face, &layout(vec![orphan])).is_empty());
    }

    /// A handle is a handle to the provider that issued it, and the row says which that was.
    ///
    /// The frame that has been opened on two providers is the case: a uuid Claude Code was given is
    /// nothing OpenCode could start on, and handing it over would open the pane on a refusal.
    #[test]
    fn a_handle_comes_back_only_for_the_provider_it_was_issued_for() {
        let face = TalkFace::default();
        seed(&face, &SavedLayout {
            project: Some(1),
            splits: std::collections::BTreeMap::new(),
            panes: vec![SavedPane {
                id: "1".to_string(),
                project: 1,
                folder: Some("/work/repo".to_string()),
                agent: Some("claude-code".to_string()),
                name: None,
                resume: Some("0f9c".to_string()),
            }],
            next_id: 2,
        });

        assert_eq!(face.comes_back_on("1", "claude-code").as_deref(), Some("0f9c"));
        assert_eq!(face.comes_back_on("1", "opencode"), None);
        // And a frame no row came back for has nothing to come back to.
        assert_eq!(face.comes_back_on("2", "claude-code"), None);

        // A handle written down now is what that frame answers with — the store is not reached for,
        // there being no arrangement yet to write it onto.
        face.resumed_from("1", "aa11".to_string());
        assert_eq!(face.comes_back_on("1", "claude-code").as_deref(), Some("aa11"));
        assert_eq!(face.resume_hints(), vec!["aa11".to_string()]);
    }

    /// What the store kept comes back into the two maps the window does not hold — with the rank the
    /// name was given at, so an agent's `talk name` does not take a person's word back off a frame
    /// that has just come back.
    #[test]
    fn the_panes_that_came_back_name_their_frames_again() {
        let face = TalkFace::default();
        let kept = SavedLayout {
            project: Some(1),
            splits: std::collections::BTreeMap::new(),
            panes: vec![SavedPane {
                id: "1".to_string(),
                project: 1,
                folder: Some("/work/repo".to_string()),
                agent: Some("claude".to_string()),
                name: Some(FrameName { name: "the migration".to_string(), by: NamedBy::Person }),
                resume: Some("0f9c".to_string()),
            }],
            next_id: 2,
        };

        seed(&face, &kept);
        assert_eq!(face.names.lock().unwrap().all()["1"].by, NamedBy::Person);
        assert_eq!(face.hints.lock().unwrap()["1"], "0f9c");
        // What was read stands as what is written, so the window's first arrangement is not written
        // straight back out.
        assert_eq!(face.kept.lock().unwrap().as_ref(), Some(&kept));

        // And the second window of the run takes nothing: a name a person changed in between is
        // theirs, and a re-reading would put the row's back over it.
        face.names.lock().unwrap().name("1", "reading the store", NamedBy::Person);
        seed(&face, &kept);
        assert_eq!(face.names.lock().unwrap().all()["1"].name, "reading the store");
    }
}
