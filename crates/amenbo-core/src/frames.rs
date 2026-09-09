//! The talk window's frames: what they are called, and what of them outlives the run.
//!
//! A frame is the place a terminal is drawn in; a session is the process running in it. **The name
//! belongs to the frame.** Tied to the session it would come back as an old name on a new process the
//! moment anything restarted — a pane called "the migration" running something else entirely.
//!
//! **A frame outlives the app, and comes back as a row rather than as a screen** ([`SavedPane`],
//! `AMB-D-869`). One row a pane: where it works, what was started in it, what it is called, and the
//! handle its provider is resumed from. A place that came back with none of that was the reason they
//! were dropped once (`AMB-T-3687`) — an empty box drawn exactly like the way in beside it, and a
//! named one saying that pressing would carry on where the reader left off, which nothing in the
//! window could then do. The handle is what changed: every provider the window opens can be told to
//! carry on, so the row is a way back into the session rather than a picture of one.
//!
//! **What is written down is one machine's answer** — a wider screen holds more panes — so all of it
//! sits in the store's device row, like the read receipts and the tick's day marks in
//! [`crate::overview`], and not in `config.json`, which a restore does not carry (`AMB-D-434`).
//!
//! **What is not kept is the session.** A process died with the run, and the pane comes back with
//! nothing running in it until something starts one from the handle the row carries. Which pane was
//! being worked in is this run's as well: it is where the reader is looking, and an older write must
//! not move them.
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
//! Both the name and who gave it are on the pane's row: a name that came back without its rank would
//! be a person's word the next `talk name` could take off.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::store_engine::StoreEngine;

/// The `store_meta` key the kept part of the arrangement lives under, as one JSON object.
const LAYOUT_META: &str = "talk.layout";

/// The `store_meta` key older builds kept the frame names under.
///
/// Nothing writes it any more, and nothing may read it. A name is kept again ([`SavedPane::name`]),
/// but on the row of the pane it names — while what is in here was held against ids that a build
/// handed out from "1" on every run, so a name taken out of it would land on a place it was never
/// given to. It is deleted wherever it is met ([`save_layout`]) rather than left as a row nobody can
/// account for.
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
/// Held in the process, and written down with the panes rather than from here: what a frame is called
/// goes on that frame's row as the arrangement is kept ([`SavedPane::name`]), and comes back into
/// this map as the first window of a run reads one (`app/src-tauri/src/frames.rs`). It is one map for
/// the whole app rather than one per window, because the face moves between the two windows and a
/// name belongs to the place wherever it is being drawn.
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

/// The talk window's arrangement as one machine left it: what a person set, and what they opened.
///
/// **A project nobody has split is absent rather than written at a default.** What is kept is an
/// answer somebody gave, and a row that carried every project the store has would grow with the
/// store while saying nothing about most of them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLayout {
    /// The project whose panes the face was showing. `None` is a machine where the face has not been
    /// told of one yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<u32>,
    /// How each project's page is split, by project.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub splits: BTreeMap<u32, Split>,
    /// The panes, in the order they were opened ([`SavedPane`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panes: Vec<SavedPane>,
    /// The id to hand out to the next pane opened.
    ///
    /// **It is kept because the ids are.** A pane's row is found again by its id, and a run that
    /// began handing them out from the first would give a fresh pane the id of one that was closed
    /// before the app went down — along with whatever is still lying about under that id
    /// (`AMB-T-4640`). So the count goes up across runs and never back.
    #[serde(default = "first_id")]
    pub next_id: u32,
}

/// The id a store that has never opened a pane hands out first, and what a row written before the
/// ids were kept reads as: there is nothing behind it to collide with.
fn first_id() -> u32 {
    1
}

/// A machine that has laid nothing out: no project, no answer about a split, no places — and the
/// first id still to hand out, because an id is a count of what has been opened rather than a field
/// that starts empty.
impl Default for SavedLayout {
    fn default() -> Self {
        Self {
            project: None,
            splits: BTreeMap::new(),
            panes: Vec::new(),
            next_id: first_id(),
        }
    }
}

/// One pane's row: where it works, what was started in it, what it is called, and the way back into
/// what it was talking to.
///
/// **It is a row and not a screen.** What a pane had on it went with the process that printed it, so
/// nothing here draws: this is what a window needs to put the place back and offer to carry on in it
/// (`AMB-D-869`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPane {
    /// The id this pane was handed when it was opened, which is what its name and its handle are held
    /// against ([`SavedLayout::next_id`]).
    pub id: String,
    /// The project it is one of. A pane is a project's from the moment it is made and never moves
    /// between them, so it comes back under the same one.
    pub project: u32,
    /// The folder its terminal works in — one of the folders that project is bound to. `None` for a
    /// pane that took up a terminal somebody else started and had not yet been told where it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// The id the agent in it was started as — a row of [`crate::harness::LAUNCHES`], or a command
    /// the reader registered — and `None` for a plain prompt, which is a pane to come back to with
    /// nothing to resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// What the pane is called, and who called it that ([`FrameName`]). Absent for a pane nobody has
    /// named, which the window draws by the folder it works in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<FrameName>,
    /// The handle this pane's provider is resumed from — a session id for most of them, and the path
    /// of a home of its own for `codex` (`AMB-D-869`).
    ///
    /// **What shape it takes is the provider's, so it is kept as the word to hand back and nothing
    /// more.** Which flag carries it is the launch's to say (`AMB-T-4639`, `AMB-T-4640`); a row that
    /// has none is a pane there is no way back into, and it comes back as a place to start something
    /// in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
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
///
/// **The panes an older build wrote are read straight past.** It kept them under `frames`, a shape
/// with no name, no agent and no handle on it, numbered by a run that began again at "1" — so what
/// there is to take from one is a place with nothing to say and an id another pane may already be
/// holding. A store written by such a build comes back with its splits and no panes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    #[serde(default)]
    project: Option<u32>,
    #[serde(default)]
    splits: BTreeMap<u32, Split>,
    #[serde(default)]
    panes: Vec<SavedPane>,
    #[serde(default = "first_id")]
    next_id: u32,
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
    // Whatever the row says, the next id clears every pane in it: an id handed out twice would put one
    // pane's name, and the way back into another one's session, on a place neither belongs to.
    let next_id = row
        .panes
        .iter()
        .filter_map(|pane| pane.id.parse::<u32>().ok())
        .fold(row.next_id, |next, id| next.max(id + 1));
    Ok(Some(SavedLayout { project: row.project, splits, panes: row.panes, next_id }))
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
            ..SavedLayout::default()
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

    /// What a person set comes back, and so do the places they opened — with nothing running in any
    /// of them, and no word about which one they were working in.
    #[test]
    fn the_split_the_project_and_the_panes_come_back() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert_eq!(saved_layout(&engine).unwrap(), None, "nothing has been laid out yet");

        let kept = SavedLayout {
            project: Some(1),
            splits: BTreeMap::from([(1, Split { count: 4, orient: Orient::Across })]),
            panes: vec![SavedPane {
                id: "2".into(),
                project: 1,
                folder: Some("/work/repo".into()),
                agent: Some("claude".into()),
                name: Some(FrameName { name: "the migration".into(), by: Person }),
                resume: Some("0f9c-…".into()),
            }],
            next_id: 3,
        };
        save_layout(&engine, &kept).unwrap();

        assert_eq!(saved_layout(&engine).unwrap(), Some(kept));
        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        assert!(!written.contains("session"), "a process is not kept: {written}");
        assert!(!written.contains("splitOut"), "nor which one was being worked in: {written}");
    }

    /// A pane nobody has named, opened at a plain prompt, is a row of what there is to say and no
    /// more — the absent halves are absent rather than written empty.
    #[test]
    fn a_pane_with_nothing_to_say_writes_nothing() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let kept = SavedLayout {
            project: Some(1),
            splits: BTreeMap::new(),
            panes: vec![SavedPane {
                id: "1".into(),
                project: 1,
                folder: Some("/work/repo".into()),
                agent: None,
                name: None,
                resume: None,
            }],
            next_id: 2,
        };
        save_layout(&engine, &kept).unwrap();

        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        assert!(!written.contains("agent"), "nothing was started in it: {written}");
        assert!(!written.contains("name"), "nobody has named it: {written}");
        assert!(!written.contains("resume"), "and there is no way back into it: {written}");
        assert_eq!(saved_layout(&engine).unwrap(), Some(kept));
    }

    /// The id handed out next clears every pane in the row, whichever of the two says the higher
    /// number: a reused id would put one pane's name, and the way back into another's session, on a
    /// place neither belongs to.
    #[test]
    fn the_next_id_clears_every_pane_that_came_back() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let pane = |id: &str| SavedPane {
            id: id.into(),
            project: 1,
            folder: None,
            agent: None,
            name: None,
            resume: None,
        };
        engine
            .set_meta(
                LAYOUT_META,
                Some(r#"{"project":1,"nextId":2,"panes":[{"id":"5","project":1}]}"#),
            )
            .unwrap();
        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back.next_id, 6, "a row that undercounts its own panes is answered past");

        save_layout(
            &engine,
            &SavedLayout {
                project: Some(1),
                splits: BTreeMap::new(),
                panes: vec![pane("2")],
                next_id: 9,
            },
        )
        .unwrap();
        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back.next_id, 9, "and one that counts past them keeps its count");
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
                // Its places are read past: what it kept of one is a folder under an id its own run
                // began handing out from the first, with nothing to say about what was in it.
                panes: Vec::new(),
                next_id: 3,
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
            Some(SavedLayout { project: None, splits: BTreeMap::new(), panes: Vec::new(), next_id: 1 })
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
            ..SavedLayout::default()
        };
        save_layout(&engine, &across).unwrap();
        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        assert!(!written.contains("orient"), "nothing was asked: {written}");

        let kept = SavedLayout {
            project: Some(1),
            splits: BTreeMap::from([(1, Split { count: 2, orient: Orient::Down })]),
            ..SavedLayout::default()
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
