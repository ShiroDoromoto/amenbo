//! The talk window's frames: what they are called, and the little of their arrangement that outlives
//! the run.
//!
//! A frame is the place a terminal is drawn in; a session is the process running in it. **The name
//! belongs to the frame.** Tied to the session it would come back as an old name on a new process the
//! moment anything restarted — a pane called "the migration" running something else entirely.
//!
//! **A frame does not outlive the app** (`AMB-T-3687`). What came back before was a place with
//! nothing in it: the session died with the last run, so a restored frame was an empty box drawn
//! exactly like the way in beside it, and a *named* one was worse — it said pressing would carry on
//! where the reader left off, which nothing in the window can do. So the places, their names and
//! which one was being worked in are this run's, and they live where the running state lives
//! (`app/src-tauri/src/frames.rs`): in the process, for as long as it is up, shared by the board and
//! the window a terminal is split out into.
//!
//! **What is kept is what a person set rather than what they opened** ([`SavedLayout`]): how many
//! panes to a page, and the project they were looking at. Both are one machine's answer — a wider
//! screen holds more panes — so they sit in the store's device row, like the read receipts and the
//! tick's day marks in [`crate::overview`], and not in `config.json`, which a restore does not carry
//! (`AMB-D-434`).
//!
//! **Three things name a frame, and they are ranked** ([`NamedBy`]). The first line a person types into
//! a new pane names it, so a pane is not called "3" until somebody gets round to it; `talk name`
//! from the agent running in it improves on that; and a person renaming it outranks both, for good —
//! an agent that says `talk name` afterwards does not take a person's word back off the frame.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::store_engine::StoreEngine;

/// The `store_meta` key the kept part of the arrangement lives under, as one JSON object.
const LAYOUT_META: &str = "talk.layout";

/// The `store_meta` key older builds kept the frame names under.
///
/// Nothing writes it any more, and nothing may read it: ids start again at "1" every run, so a name
/// kept against one would land on a place it was never given to. It is deleted wherever it is met
/// ([`save_layout`]) rather than left as a row nobody can account for.
const RETIRED_NAMES_META: &str = "talk.frame_names";

/// How long a frame's name may be, in characters.
///
/// **A name is a label and not a sentence.** All three of the things that name a frame can run long —
/// a first line typed at an agent is a request, and `talk name` is whatever the agent thought of —
/// and the row it is drawn on has the rest of what is happening to fit on it beside the name. The
/// bound is here rather than at the three doors because it is one rule about names, and the window
/// gives what is left of a long one an ellipsis rather than the room.
const NAME_LIMIT: usize = 80;

/// Who named a frame. The order of the variants is the order of their authority: a naming may replace
/// one of its own rank or lower, never a higher one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamedBy {
    /// The first line the person typed into the pane. It names a frame that has none and never
    /// replaces one — it is the *first* line, and a second is just more typing.
    Typed,
    /// The agent, through `talk name`. It knows what it is doing better than the first line did.
    Session,
    /// The person, saying so. The last word, and it stays the last word.
    Person,
}

/// A frame's name, and who put it there — which is what says whether the next naming may replace it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameName {
    /// What the frame is called.
    pub name: String,
    /// Who called it that.
    pub by: NamedBy,
}

/// Whether a naming by `by` may take the place of what is on the frame now.
///
/// The whole rule, in one place: nothing outranks a person; the first line typed only ever names a
/// frame that has no name at all; anything else replaces its own rank and below.
fn accepts(current: Option<&FrameName>, by: NamedBy) -> bool {
    match current {
        None => true,
        Some(_) if by == NamedBy::Typed => false,
        Some(current) => by >= current.by,
    }
}

/// What this run calls the talk window's frames.
///
/// Held in the process and written nowhere: a name is about a place that is gone as soon as the app
/// is (`AMB-T-3687`). It is one map for the whole app rather than one per window, because the face
/// moves between the two windows and a name belongs to the place wherever it is being drawn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameNames(BTreeMap<String, FrameName>);

impl FrameNames {
    /// Every frame that has a name, in frame order.
    pub fn all(&self) -> &BTreeMap<String, FrameName> {
        &self.0
    }

    /// Name `frame`, if `by` outranks whoever named it last, and answer with the names as they now
    /// stand.
    ///
    /// The answer is the whole map rather than what was written, so a caller whose naming was refused
    /// sees the name that stood instead of assuming its own took. A blank name is a frame being
    /// un-named, which is the person's to do and follows the same ranking.
    pub fn name(&mut self, frame: &str, name: &str, by: NamedBy) -> &BTreeMap<String, FrameName> {
        if !accepts(self.0.get(frame), by) {
            return &self.0;
        }
        match name.trim() {
            "" => {
                self.0.remove(frame);
            }
            name => {
                // Cut by characters and not by bytes: a name in Japanese is a third of the characters
                // a byte count would leave of it, and half a character is not a shorter name.
                let name: String = name.chars().take(NAME_LIMIT).collect();
                self.0.insert(frame.to_string(), FrameName { name, by });
            }
        }
        &self.0
    }
}

/// The part of the talk window's arrangement that outlives the run, as one machine left it.
///
/// **The frames are not in it** — see this module's head. What is here is what a person set rather
/// than what they opened: the split they chose, and the project they were looking at. Both are worth
/// coming back to because neither says anything about work that has ended.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLayout {
    /// How many panes to a page.
    pub count: u32,
    /// The project whose panes the face was showing. `None` is a machine where the face has not been
    /// told of one yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<u32>,
}

/// What this device kept of the arrangement, or nothing where it kept none.
///
/// A scalar that will not parse reads as nothing rather than as a failure: it is one machine's screen,
/// and a window that meets a broken one can be laid out again — refusing to open over it would cost
/// far more than the answer is worth. Anything in the row beyond these two fields is read straight
/// past, so a row that carries more than them still answers with what they say.
pub fn saved_layout(engine: &StoreEngine) -> Result<Option<SavedLayout>> {
    Ok(engine
        .get_meta(LAYOUT_META)?
        .and_then(|json| serde_json::from_str(&json).ok()))
}

/// Keep what outlives the run. It is written as the window is changed rather than as it closes: a
/// window that is killed, or a machine that loses power, is the case a person wants their split back
/// after.
pub fn save_layout(engine: &StoreEngine, layout: &SavedLayout) -> Result<()> {
    engine.set_meta(LAYOUT_META, Some(&serde_json::to_string(layout)?))?;
    // And the names left in RETIRED_NAMES_META go with the write that would otherwise leave them
    // sitting there for good.
    Ok(engine.set_meta(RETIRED_NAMES_META, None)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    use NamedBy::{Person, Session, Typed};

    /// The ranking, as the one question it answers: may this naming take the place of that one?
    #[test]
    fn the_first_line_names_a_frame_that_has_none_and_nothing_more() {
        assert!(accepts(None, Typed), "a pane with no name is what the first line is for");
        let typed = FrameName { name: "make verify".into(), by: Typed };
        assert!(!accepts(Some(&typed), Typed), "the second line typed is just more typing");
        assert!(accepts(Some(&typed), Session), "the agent knows better than the first line");
    }

    /// The ranking is what the map applies — not what the caller hoped. A refused naming answers with
    /// the name that stood, so nobody draws the one it wanted.
    #[test]
    fn a_refused_naming_answers_with_the_name_that_stood() {
        let mut names = FrameNames::default();
        assert!(names.all().is_empty(), "no frame has been named yet");

        names.name("1", "make verify", Typed);
        names.name("1", "cargo test", Typed);
        assert_eq!(names.all()["1"].name, "make verify", "the *first* line");

        names.name("1", "the migration", Session);
        names.name("1", "AMB-T-3597", Person);
        let after = names.name("1", "reading the store", Session).clone();
        assert_eq!(after["1"], FrameName { name: "AMB-T-3597".into(), by: Person });

        names.name("2", "the plugins", Session);
        assert_eq!(names.all().len(), 2, "one name per frame, not one per window");

        names.name("1", "  ", Person);
        assert!(!names.all().contains_key("1"), "a blank name un-names the frame");
    }

    /// A name is a label, so a long one is cut to a label's length — in characters, because half a
    /// character is not a shorter name.
    #[test]
    fn a_name_is_cut_to_what_a_row_can_carry() {
        let mut names = FrameNames::default();
        names.name("1", &"の".repeat(NAME_LIMIT * 2), Session);
        let kept = &names.all()["1"].name;
        assert_eq!(kept.chars().count(), NAME_LIMIT, "cut to the label's length");
        assert_eq!(kept, &"の".repeat(NAME_LIMIT), "and cut on a character");
    }

    /// What a person set comes back, and what they opened does not: the split and the project are
    /// kept, and there is nowhere in the row for a frame to be kept in.
    #[test]
    fn the_split_and_the_project_come_back_and_the_frames_do_not() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert_eq!(saved_layout(&engine).unwrap(), None, "nothing has been laid out yet");

        let kept = SavedLayout { count: 4, project: Some(1) };
        save_layout(&engine, &kept).unwrap();

        assert_eq!(saved_layout(&engine).unwrap(), Some(kept));
        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        assert!(!written.contains("frames"), "a place is not kept: {written}");
        assert!(!written.contains("nextId"), "nor an id to hand out after it: {written}");
        assert!(!written.contains("splitOut"), "nor which one was being worked in: {written}");
    }

    /// An arrangement an older build wrote still reads: the split and the project are where they were,
    /// and the frames beside them are read past rather than refused.
    #[test]
    fn an_older_arrangement_gives_up_its_split_and_its_project() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine
            .set_meta(
                LAYOUT_META,
                Some(
                    r#"{"count":4,"nextId":3,"project":2,
                        "frames":[{"id":"1","project":2,"folder":"/work/repo"}],"splitOut":"1"}"#,
                ),
            )
            .unwrap();
        assert_eq!(saved_layout(&engine).unwrap(), Some(SavedLayout { count: 4, project: Some(2) }));
    }

    /// The names an older build kept are cleared where they are met: ids start again at "1" every
    /// run, so a kept name would come back on a place nobody gave it to.
    #[test]
    fn the_names_an_older_build_kept_are_dropped() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine
            .set_meta(RETIRED_NAMES_META, Some(r#"{"1":{"name":"the migration","by":"person"}}"#))
            .unwrap();

        save_layout(&engine, &SavedLayout { count: 2, project: None }).unwrap();

        assert_eq!(engine.get_meta(RETIRED_NAMES_META).unwrap(), None);
    }

    /// A scalar nobody can read is no arrangement, not a failure to open the window over.
    #[test]
    fn an_unreadable_arrangement_is_no_arrangement() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine.set_meta(LAYOUT_META, Some("{ not json")).unwrap();
        assert_eq!(saved_layout(&engine).unwrap(), None);
    }

    #[test]
    fn a_person_outranks_the_agent_and_keeps_outranking_it() {
        let person = FrameName { name: "the migration".into(), by: Person };
        assert!(!accepts(Some(&person), Session), "`talk name` does not take a person's word back");
        assert!(!accepts(Some(&person), Typed));
        assert!(accepts(Some(&person), Person), "and a person may change their mind");
        let session = FrameName { name: "reading the store".into(), by: Session };
        assert!(accepts(Some(&session), Session), "an agent may say something newer than itself");
        assert!(accepts(Some(&session), Person));
    }
}
