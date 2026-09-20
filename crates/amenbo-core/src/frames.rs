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
//! **How much of a page a pane takes is the pane's own answer** ([`PaneSize`], `AMB-D-939`). How
//! much room a piece of work wants is about that piece of work — an agent and the shell it is
//! watched from are not the same size of thing — so one answer held for a whole project made every
//! pane on it change together. Where each pane is drawn is not written down at all: the window works
//! it out from the order every time it draws (`app/src/talk/layout.ts`).
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

    /// Let go of the names of the frames the window no longer has, keeping only those `here` says are
    /// still on the screen.
    ///
    /// A frame that is closed is closed for good, and the name it was called by goes with it — the
    /// same way as everything else the run is holding for that frame
    /// (`app/src-tauri/src/frames.rs`). What is written down is unaffected either way: a name is kept
    /// on its frame's row, and a row the window no longer sends is not written at all.
    pub fn retain(&mut self, here: impl Fn(&str) -> bool) {
        self.0.retain(|frame, _| here(frame.as_str()));
    }
}

/// How much of a page one pane takes (`AMB-D-939`).
///
/// **Six sizes, and every one of them divides the page exactly**, so one grid draws all of them —
/// twelve cells across and two down, of which an eighth is three and a sixth is four
/// (`app/src/talk/layout.ts`). Where the small end stops is settled by columns of text: what an
/// agent's TUI wants is eighty columns, and a twelfth would put a pane under eighty on every screen
/// there is.
///
/// **A half comes two ways round.** Side by side it halves the columns, which is under the eighty a
/// TUI wants on a window with a column beside it; laid down the page it leaves the columns whole and
/// takes the lines instead. They are two sizes rather than one size with an answer on it, which is
/// what lets one pane sit the way its work wants while the pane beside it sits the other way.
///
/// A row written without one is a pane nobody sized, and it comes back at the whole page — which is
/// what one pane on its own fills.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaneSize {
    /// The whole page.
    #[default]
    Whole,
    /// Half of it, side by side — six cells across and both rows.
    Half,
    /// Half of it, one above the other — the whole width and one row.
    HalfDown,
    /// A quarter: six cells across and one row.
    Quarter,
    /// A sixth: four cells across and one row.
    Sixth,
    /// An eighth: three cells across and one row.
    Eighth,
}

/// The talk window's arrangement as one machine left it: what a person set, and what they opened.
///
/// **The order is the whole of the arrangement.** A pane carries how much of a page it takes and
/// nothing about where it sits: the window lays the panes down in order and the pages fall out of
/// that, so there is no place here for a row to be put back in the wrong one (`AMB-D-939`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLayout {
    /// The project whose panes the face was showing. `None` is a machine where the face has not been
    /// told of one yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<u32>,
    /// The panes, in the order they were opened ([`SavedPane`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panes: Vec<SavedPane>,
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
    /// against — a version 4 UUID, drawn by the window that opened the pane
    /// (`app/src/talk/layout.ts`).
    ///
    /// **It is drawn and not counted** (`AMB-D-897`). A counted id is only ever as unique as the
    /// count it came from, and that count was kept in this same row: a row that would not parse was
    /// dropped whole, and the run after it began at the first id again — onto ids a pane's name, the
    /// way back into its session and the home still lying about under it were already held against
    /// (`AMB-T-4640`). A drawn id collides with none of that, whatever becomes of the row.
    pub id: String,
    /// The project it is one of. A pane is a project's from the moment it is made and never moves
    /// between them, so it comes back under the same one.
    pub project: u32,
    /// How much of a page it takes ([`PaneSize`]). A row written before sizes were kept has none, and
    /// comes back at the whole page.
    #[serde(default)]
    pub size: PaneSize,
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
    /// The model this pane was last known to be answering on — the spelling the provider takes, and
    /// `None` for a pane opened on whatever the provider's own settings had.
    ///
    /// **It is the pane's and not the agent's** (`AMB-T-4698`). What model an agent comes up on is
    /// kept against the agent ([`crate::config::Config::model_for`]), which is the right answer for
    /// a pane about to be opened and the wrong one for a pane coming back: choosing another model in
    /// one pane would otherwise decide what every other pane of that provider resumes on. So the
    /// name that went on this pane's line goes down here, beside the handle it comes back by, and it
    /// is that name the next run hands back to the one provider that does not restore its own
    /// ([`crate::harness::Launch::model_on_the_way_back`]).
    ///
    /// **What is written in the pane is not in it.** A person who types the provider's own model
    /// command into the terminal has changed something Amenbo never sees, and reading it back off
    /// the screen is the one thing a pane exists not to do (`AMB-D-747`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Whether the box under this pane is open (`AMB-D-890`).
    ///
    /// **It is the pane's, and the habit it starts from is the machine's.** What a pane *about to be
    /// opened* comes up as is the answer this machine last gave, kept beside the theme rather than
    /// here (`app/src/core/composeStartsOpen.ts`, `AMB-D-889`). What a pane already made is stands
    /// here, so one put in a window of its own, moved to another page or come back to after a run is
    /// the pane the reader left — and not the one their habit would make now.
    ///
    /// `None` is a row written before this was kept. The window opens such a pane on the habit, which
    /// is the only answer there is for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_open: Option<bool>,
}

/// What this device kept of the arrangement, or nothing where it kept none.
///
/// A scalar that will not parse reads as nothing rather than as a failure: it is one machine's screen,
/// and a window that meets a broken one can be laid out again — refusing to open over it would cost
/// far more than the answer is worth. Anything in the row beyond the fields read here is read straight
/// past, so a row that carries more than them still answers with what they say.
///
/// **The shapes older builds wrote are converted once, on the way in** — the split a whole project
/// was held at becomes a size on each of that project's panes (`AMB-D-939`, migration v49). So there
/// is one shape to read here, and nothing in this module knows what a count was.
pub fn saved_layout(engine: &StoreEngine) -> Result<Option<SavedLayout>> {
    Ok(engine
        .get_meta(LAYOUT_META)?
        .and_then(|json| serde_json::from_str::<SavedLayout>(&json).ok()))
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

    /// One pane's row, with nothing on it but where it is and how much of a page it takes.
    fn pane(id: &str, project: u32, size: PaneSize) -> SavedPane {
        SavedPane {
            id: id.into(),
            project,
            size,
            folder: None,
            agent: None,
            name: None,
            resume: None,
            model: None,
            compose_open: None,
        }
    }

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

    /// A size on each pane, kept apart: how much room one piece of work wants says nothing about
    /// what the pane beside it wants (`AMB-D-939`).
    #[test]
    fn each_pane_keeps_its_own_size() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let kept = SavedLayout {
            project: Some(1),
            panes: vec![pane("a", 1, PaneSize::Quarter), pane("b", 1, PaneSize::HalfDown)],
        };
        save_layout(&engine, &kept).unwrap();

        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back, kept);
        assert_eq!(back.panes[0].size, PaneSize::Quarter);
        assert_eq!(back.panes[1].size, PaneSize::HalfDown);
    }

    /// The six sizes go over as the window spells them, which is what lets the two sides be read
    /// against each other (`app/src/talk/layout.ts`).
    #[test]
    fn a_size_is_written_the_way_the_window_spells_it() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let all = [
            PaneSize::Whole,
            PaneSize::Half,
            PaneSize::HalfDown,
            PaneSize::Quarter,
            PaneSize::Sixth,
            PaneSize::Eighth,
        ];
        let kept = SavedLayout {
            project: Some(1),
            panes: all.iter().enumerate().map(|(at, size)| pane(&at.to_string(), 1, *size)).collect(),
        };
        save_layout(&engine, &kept).unwrap();

        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        for spelling in ["whole", "half", "half-down", "quarter", "sixth", "eighth"] {
            assert!(written.contains(spelling), "{spelling} is not in it: {written}");
        }
        assert_eq!(saved_layout(&engine).unwrap(), Some(kept));
    }

    /// A row from before sizes were kept comes back at the whole page, which is what one pane on its
    /// own fills. The conversion of an older store's splits is the migration's (v49), so nothing
    /// here has to know what a count was.
    #[test]
    fn a_pane_written_without_a_size_comes_back_at_the_whole_page() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine
            .set_meta(LAYOUT_META, Some(r#"{"project":1,"panes":[{"id":"5","project":1}]}"#))
            .unwrap();
        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back.panes[0].size, PaneSize::Whole);
    }

    /// What a person set comes back, and so do the places they opened — with nothing running in any
    /// of them, and no word about which one they were working in.
    #[test]
    fn the_project_and_the_panes_come_back() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert_eq!(saved_layout(&engine).unwrap(), None, "nothing has been laid out yet");

        let kept = SavedLayout {
            project: Some(1),
            panes: vec![SavedPane {
                id: "7b3f0c1e-2d4a-4c88-9a51-6e0d2f83b114".into(),
                project: 1,
                size: PaneSize::Quarter,
                folder: Some("/work/repo".into()),
                agent: Some("claude".into()),
                name: Some(FrameName { name: "the migration".into(), by: Person }),
                resume: Some("0f9c-…".into()),
                model: Some("opus".into()),
                compose_open: Some(true),
            }],
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
            panes: vec![SavedPane {
                id: "1f0b6d92-8c47-4a10-b3e5-5d9a7c204e6b".into(),
                project: 1,
                size: PaneSize::Whole,
                folder: Some("/work/repo".into()),
                agent: None,
                name: None,
                resume: None,
                model: None,
                compose_open: None,
            }],
        };
        save_layout(&engine, &kept).unwrap();

        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        assert!(!written.contains("agent"), "nothing was started in it: {written}");
        assert!(!written.contains("name"), "nobody has named it: {written}");
        assert!(!written.contains("resume"), "and there is no way back into it: {written}");
        assert!(!written.contains("model"), "nor a model it was put on: {written}");
        assert!(
            !written.contains("composeOpen"),
            "nor which way the box under it was left, on a row that never said: {written}"
        );
        assert_eq!(saved_layout(&engine).unwrap(), Some(kept));
    }

    /// The count an older build kept beside the panes is read straight past, and goes with the next
    /// write: ids are drawn now, so a number saying which one to hand out next names nothing.
    #[test]
    fn the_count_an_older_build_kept_is_read_past_and_written_out() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine
            .set_meta(
                LAYOUT_META,
                Some(r#"{"project":1,"nextId":9,"panes":[{"id":"5","project":1}]}"#),
            )
            .unwrap();
        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back.panes.len(), 1, "the pane it kept comes back");

        save_layout(&engine, &back).unwrap();
        let written = engine.get_meta(LAYOUT_META).unwrap().expect("the arrangement");
        assert!(!written.contains("nextId"), "and the count is not written again: {written}");
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
