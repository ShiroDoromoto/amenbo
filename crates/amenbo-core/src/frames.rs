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
//! **What is kept is what a person set rather than what they opened** ([`SavedLayout`]): how each
//! project's page is split, and which project they were looking at. Both are one machine's answer — a
//! wider screen holds more panes — so they sit in the store's device row, like the read receipts and
//! the tick's day marks in [`crate::overview`], and not in `config.json`, which a restore does not
//! carry (`AMB-D-434`).
//!
//! **The split is one answer per project and not one for the face** ([`Split`]). How many panes a
//! project wants is about the work in it — one repository is watched in a pane and another is worked
//! in four — so a person moving between projects is not changing their mind about either. It is the
//! rule the columns beside the panes already read by (`AMB-D-835`).
//!
//! **Two things name a frame, and they are ranked** ([`NamedBy`]). `talk name` from the agent running
//! in the pane names it, and a person renaming it outranks that, for good — an agent that says
//! `talk name` afterwards does not take a person's word back off the frame. A frame neither has
//! named is drawn by the folder it works in, which the window decides and nothing here writes down.

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
/// **A name is a label and not a sentence.** Both of the things that name a frame can run long —
/// `talk name` is whatever the agent thought of, and a person may type anything — and the row it is
/// drawn on has the rest of what is happening to fit on it beside the name. The bound is here rather
/// than at the two doors because it is one rule about names, and the window gives what is left of a
/// long one an ellipsis rather than the room.
///
/// It is public because it is also the bound every provider's own rename has to clear: a name cut to
/// this length is one [`crate::harness::Rename`] can hand on without asking (`AMB-D-872`).
pub const NAME_LIMIT: usize = 80;

/// Who named a frame. The order of the variants is the order of their authority: a naming may replace
/// one of its own rank or lower, never a higher one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamedBy {
    /// The agent, through `talk name` — the only thing that names a frame nobody has named.
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
/// The whole rule, in one place: nothing outranks a person, and anything else replaces its own rank
/// and below.
fn accepts(current: Option<&FrameName>, by: NamedBy) -> bool {
    match current {
        None => true,
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

/// How one project's page is split.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Split {
    /// How many panes to a page.
    pub count: u32,
    /// Which way a two-pane page sits ([`Orient`]). It is kept whatever the split is: a person who
    /// went to four panes and back to two means the two they set up.
    #[serde(default, skip_serializing_if = "Orient::is_across")]
    pub orient: Orient,
}

/// The part of the talk window's arrangement that outlives the run, as one machine left it.
///
/// **The frames are not in it** — see this module's head. What is here is what a person set rather
/// than what they opened: how each project's page is split, and which project they were looking at.
/// Both are worth coming back to because neither says anything about work that has ended.
///
/// **A project nobody has split is absent rather than written at a default.** What is kept is an
/// answer somebody gave, and a row that carried every project the store has would grow with the
/// store while saying nothing about most of them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLayout {
    /// The project whose panes the face was showing. `None` is a machine where the face has not been
    /// told of one yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<u32>,
    /// How each project's page is split, by project.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub splits: BTreeMap<u32, Split>,
}

/// Which way the two panes of a two-pane page sit: side by side, or one above the other.
///
/// **It is asked about two panes and about nothing else.** Every count spends width before height —
/// a terminal runs short of columns long before it runs short of lines — and at four and above the
/// rows are already spent, so there is no arrangement left to choose between. Two is the count where
/// spending width first stops paying: half a window is under the eighty columns an agent's TUI wants,
/// while two down leaves the columns whole and takes the lines instead (`app/src/talk/layout.ts`).
///
/// An arrangement written before there was anything to ask reads as [`Across`](Orient::Across), which
/// is what two panes did then.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orient {
    /// Side by side, which is what every other count does.
    #[default]
    Across,
    /// One above the other.
    Down,
}

impl Orient {
    /// Whether this is the way a page sits when nobody has said otherwise — what lets the answer stay
    /// out of the row until it is one.
    pub fn is_across(&self) -> bool {
        matches!(self, Orient::Across)
    }
}

/// The row as it may be written, in either of the shapes this build can meet.
///
/// A build before the split was per project wrote one for the whole face. That answer belonged to
/// whichever project the face was on, and [`saved_layout`] is where it is put back under it — so a
/// person who set four panes and updated finds four panes on the project they set them on.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    #[serde(default)]
    project: Option<u32>,
    #[serde(default)]
    splits: BTreeMap<u32, Split>,
    /// The one split an older build wrote. `None` in anything this build has written.
    #[serde(default)]
    count: Option<u32>,
    /// And the way it sat, which only ever meant anything beside that count.
    #[serde(default)]
    orient: Orient,
}

/// What this device kept of the arrangement, or nothing where it kept none.
///
/// A scalar that will not parse reads as nothing rather than as a failure: it is one machine's screen,
/// and a window that meets a broken one can be laid out again — refusing to open over it would cost
/// far more than the answer is worth. Anything in the row beyond the fields read here is read straight
/// past, so a row that carries more than them still answers with what they say.
///
/// **An older build's one split is put back under the project it was set on.** Where the row names no
/// project there is nowhere to put it, and it is let go: a split with nothing to hold it against is
/// not an answer about anything.
pub fn saved_layout(engine: &StoreEngine) -> Result<Option<SavedLayout>> {
    let Some(row) = engine
        .get_meta(LAYOUT_META)?
        .and_then(|json| serde_json::from_str::<Row>(&json).ok())
    else {
        return Ok(None);
    };
    let mut splits = row.splits;
    if let (true, Some(count), Some(project)) = (splits.is_empty(), row.count, row.project) {
        splits.insert(project, Split { count, orient: row.orient });
    }
    Ok(Some(SavedLayout { project: row.project, splits }))
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

    use NamedBy::{Person, Session};

    /// The ranking, as the one question it answers: may this naming take the place of that one?
    #[test]
    fn a_frame_with_no_name_takes_whatever_names_it() {
        assert!(accepts(None, Session), "an agent may name a pane nobody has named");
        assert!(accepts(None, Person), "and so may a person");
    }

    /// The ranking is what the map applies — not what the caller hoped. A refused naming answers with
    /// the name that stood, so nobody draws the one it wanted.
    #[test]
    fn a_refused_naming_answers_with_the_name_that_stood() {
        let mut names = FrameNames::default();
        assert!(names.all().is_empty(), "no frame has been named yet");

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

    /// A split for each project, kept apart: how many panes one project wants says nothing about
    /// what another one wants, and moving between them is not changing your mind about either.
    #[test]
    fn each_project_keeps_its_own_split() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let kept = SavedLayout {
            project: Some(1),
            splits: BTreeMap::from([
                (1, Split { count: 4, orient: Orient::Across }),
                (2, Split { count: 2, orient: Orient::Down }),
            ]),
        };
        save_layout(&engine, &kept).unwrap();

        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back, kept);
        assert_eq!(back.splits[&1].count, 4);
        assert_eq!(back.splits[&2].orient, Orient::Down);
        // A project nobody has split is absent, not written at a default: what is kept is an answer
        // somebody gave.
        assert!(!back.splits.contains_key(&3));
    }

    /// What a person set comes back, and what they opened does not: the split and the project are
    /// kept, and there is nowhere in the row for a frame to be kept in.
    #[test]
    fn the_split_and_the_project_come_back_and_the_frames_do_not() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert_eq!(saved_layout(&engine).unwrap(), None, "nothing has been laid out yet");

        let kept = SavedLayout {
            project: Some(1),
            splits: BTreeMap::from([(1, Split { count: 4, orient: Orient::Across })]),
        };
        save_layout(&engine, &kept).unwrap();

        assert_eq!(saved_layout(&engine).unwrap(), Some(kept));
        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        assert!(!written.contains("frames"), "a place is not kept: {written}");
        assert!(!written.contains("nextId"), "nor an id to hand out after it: {written}");
        assert!(!written.contains("splitOut"), "nor which one was being worked in: {written}");
    }

    /// An arrangement an older build wrote still reads: the frames beside it are read past rather
    /// than refused, and its one split is put back under the project the face was on — which is the
    /// project it was set on.
    #[test]
    fn an_older_arrangement_gives_its_one_split_to_the_project_it_was_set_on() {
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
        assert_eq!(
            saved_layout(&engine).unwrap(),
            Some(SavedLayout {
                project: Some(2),
                splits: BTreeMap::from([(2, Split { count: 4, orient: Orient::Across })]),
            })
        );
    }

    /// And an older one that names no project has nowhere to put its split, so it lets it go: a
    /// count with nothing to hold it against is not an answer about anything.
    #[test]
    fn an_older_arrangement_naming_no_project_lets_its_split_go() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine.set_meta(LAYOUT_META, Some(r#"{"count":4}"#)).unwrap();
        assert_eq!(
            saved_layout(&engine).unwrap(),
            Some(SavedLayout { project: None, splits: BTreeMap::new() })
        );
    }

    /// The names an older build kept are cleared where they are met: ids start again at "1" every
    /// run, so a kept name would come back on a place nobody gave it to.
    #[test]
    fn the_names_an_older_build_kept_are_dropped() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine
            .set_meta(RETIRED_NAMES_META, Some(r#"{"1":{"name":"the migration","by":"person"}}"#))
            .unwrap();

        save_layout(&engine, &SavedLayout::default()).unwrap();

        assert_eq!(engine.get_meta(RETIRED_NAMES_META).unwrap(), None);
    }

    /// The orientation comes back the way the split does, and stays out of the row while it is the
    /// one every count has: a person who never asked has nothing of theirs to keep.
    #[test]
    fn the_way_two_panes_sit_is_kept_only_once_it_has_been_asked() {
        let engine = StoreEngine::open_in_memory().unwrap();

        let across = SavedLayout {
            project: Some(1),
            splits: BTreeMap::from([(1, Split { count: 2, orient: Orient::Across })]),
        };
        save_layout(&engine, &across).unwrap();
        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        assert!(!written.contains("orient"), "nothing was asked: {written}");

        let kept = SavedLayout {
            project: Some(1),
            splits: BTreeMap::from([(1, Split { count: 2, orient: Orient::Down })]),
        };
        save_layout(&engine, &kept).unwrap();
        assert_eq!(saved_layout(&engine).unwrap(), Some(kept));
    }

    /// An arrangement written before there was anything to ask is two panes side by side, which is
    /// what two panes were then.
    #[test]
    fn an_arrangement_with_no_orientation_in_it_is_side_by_side() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine.set_meta(LAYOUT_META, Some(r#"{"count":2,"project":1}"#)).unwrap();
        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back.splits[&1].orient, Orient::Across);
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
        assert!(accepts(Some(&person), Person), "and a person may change their mind");
        let session = FrameName { name: "reading the store".into(), by: Session };
        assert!(accepts(Some(&session), Session), "an agent may say something newer than itself");
        assert!(accepts(Some(&session), Person));
    }
}
