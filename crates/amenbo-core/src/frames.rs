//! The names the talk window's frames carry, and who gets to set one.
//!
//! A frame is the place a terminal is drawn in; a session is the process running in it. **The name
//! belongs to the frame.** Tied to the session it would come back as an old name on a new process the
//! moment anything restarted — a pane called "the migration" running something else entirely.
//!
//! Device-local, like the read receipts and the tick's day marks beside it in [`crate::overview`]:
//! frames are an arrangement of one machine's screen, and a second machine on the same data has its
//! own. They live in one `store_meta` scalar rather than a table of their own, because what is kept is
//! a handful of short strings and the window reads all of them at once or none.
//!
//! **The arrangement is kept beside the names** ([`SavedLayout`]), and for the same reason: the frames
//! a person laid out are their work, and a window that came back with one empty pane would have
//! thrown it away (`AMB-D-434`). What is kept is the shape only — how many panes, in what order, and
//! which folder each was working in. **What was running is not kept**: a session died with the last
//! run, and a pane drawn as though it were still there would be the window telling the reader
//! something untrue. The frames come back as places to open a terminal, and nothing is started until
//! somebody asks (`AMB-T-3607`).
//!
//! **Three things name a frame, and they are ranked** ([`NamedBy`]). The first line a person types into
//! a new pane names it, so a pane is not called "3" until somebody gets round to it; `session name`
//! from the agent running in it improves on that; and a person renaming it outranks both, for good —
//! an agent that says `session name` afterwards does not take a person's word back off the frame.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::store_engine::StoreEngine;

/// The `store_meta` key the frame names live under, as one JSON object keyed by frame.
const FRAME_NAMES_META: &str = "talk.frame_names";

/// The `store_meta` key the arrangement lives under, as one JSON object.
const LAYOUT_META: &str = "talk.layout";

/// Who named a frame. The order of the variants is the order of their authority: a naming may replace
/// one of its own rank or lower, never a higher one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamedBy {
    /// The first line the person typed into the pane. It names a frame that has none and never
    /// replaces one — it is the *first* line, and a second is just more typing.
    Typed,
    /// The agent, through `session name`. It knows what it is doing better than the first line did.
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

/// Every frame this device has a name for.
///
/// A scalar that will not parse reads as no names rather than as a failure: it is one machine's screen
/// arrangement, and the window that meets it can name its frames again — refusing to open the talk
/// window over it would be paying far more than the answer is worth.
pub fn frame_names(engine: &StoreEngine) -> Result<BTreeMap<String, FrameName>> {
    Ok(engine
        .get_meta(FRAME_NAMES_META)?
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

/// Name `frame`, if `by` outranks whoever named it last, and answer with the names as they now stand.
///
/// The answer is the whole map rather than what was written, so a caller whose naming was refused sees
/// the name that stood instead of assuming its own took. A blank name is a frame being un-named, which
/// is the person's to do and follows the same ranking.
pub fn name_frame(
    engine: &StoreEngine,
    frame: &str,
    name: &str,
    by: NamedBy,
) -> Result<BTreeMap<String, FrameName>> {
    let mut names = frame_names(engine)?;
    if !accepts(names.get(frame), by) {
        return Ok(names);
    }
    match name.trim() {
        "" => {
            names.remove(frame);
        }
        name => {
            names.insert(frame.to_string(), FrameName { name: name.to_string(), by });
        }
    }
    engine.set_meta(FRAME_NAMES_META, Some(&serde_json::to_string(&names)?))?;
    Ok(names)
}

/// The talk window's arrangement, as one machine left it.
///
/// The frames are in the order they were opened, and `count` is what that order is read against: how
/// many panes a page draws at most. Neither is worth anything without the other, so they are kept and answered
/// together. `next_id` travels because ids are never reused — a frame that comes back keeps the name
/// the person gave it, and a fresh frame must not be handed an id that already has one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLayout {
    /// How many panes to a page.
    pub count: u32,
    /// The next frame id to hand out.
    pub next_id: u32,
    /// The project whose panes the face was showing.
    ///
    /// It is here for the window that has no rail: the terminal split out into one of its own draws
    /// a single pane and nobody asked it on its way in, so the project it belongs to is the one the
    /// board was on when it was split out (`app/src/talk/agent.ts`). `None` is an arrangement
    /// written before that was kept, and a face that has not been told of a project yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<u32>,
    /// The frames, in slot order.
    pub frames: Vec<SavedFrame>,
}

/// One frame, as far as it survives a run: whose project it was, where it was, and what it was
/// working on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFrame {
    /// The id the name is kept against ([`frame_names`]).
    pub id: String,
    /// The project the pane is one of. `None` is an arrangement written before panes belonged to a
    /// project: what to do with one of those is the window's, since only it knows where the person
    /// is looking.
    #[serde(default)]
    pub project: Option<u32>,
    /// The folder its terminal was working in, where it had one.
    pub folder: Option<String>,
}

/// The arrangement this device left behind, or nothing where it left none.
///
/// A scalar that will not parse reads as nothing, for the same reason the names do: it is one
/// machine's screen, and a window that meets a broken one can be laid out again — refusing to open
/// over it would cost far more than the answer is worth.
pub fn saved_layout(engine: &StoreEngine) -> Result<Option<SavedLayout>> {
    Ok(engine
        .get_meta(LAYOUT_META)?
        .and_then(|json| serde_json::from_str(&json).ok()))
}

/// Keep the arrangement. It is written as the window is changed rather than as it closes: a window
/// that is killed, or a machine that loses power, is exactly the case a person wants their panes back
/// after.
pub fn save_layout(engine: &StoreEngine, layout: &SavedLayout) -> Result<()> {
    Ok(engine.set_meta(LAYOUT_META, Some(&serde_json::to_string(layout)?))?)
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

    /// The names survive the run, and the ranking is what the store applies — not what the caller
    /// hoped. A refused naming answers with the name that stood, so nobody draws the one it wanted.
    #[test]
    fn a_frame_keeps_its_name_across_the_run_and_a_refused_naming_says_so() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert!(frame_names(&engine).unwrap().is_empty(), "no frame has been named yet");

        name_frame(&engine, "1", "make verify", Typed).unwrap();
        name_frame(&engine, "1", "cargo test", Typed).unwrap();
        assert_eq!(frame_names(&engine).unwrap()["1"].name, "make verify", "the *first* line");

        name_frame(&engine, "1", "the migration", Session).unwrap();
        name_frame(&engine, "1", "AMB-T-3597", Person).unwrap();
        let after = name_frame(&engine, "1", "reading the store", Session).unwrap();
        assert_eq!(after["1"], FrameName { name: "AMB-T-3597".into(), by: Person });
        assert_eq!(frame_names(&engine).unwrap(), after, "and that is what was kept");

        name_frame(&engine, "2", "the plugins", Session).unwrap();
        assert_eq!(frame_names(&engine).unwrap().len(), 2, "one name per frame, not one per window");

        name_frame(&engine, "1", "  ", Person).unwrap();
        assert!(!frame_names(&engine).unwrap().contains_key("1"), "a blank name un-names the frame");
    }

    /// The shape survives the run and what was running does not: the frames come back as places, and
    /// the names they were given come back on them.
    #[test]
    fn the_arrangement_comes_back_and_the_sessions_do_not() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert_eq!(saved_layout(&engine).unwrap(), None, "nothing has been laid out yet");

        let laid = SavedLayout {
            count: 4,
            next_id: 3,
            project: Some(1),
            frames: vec![
                SavedFrame { id: "1".into(), project: Some(1), folder: Some("/work/repo".into()) },
                SavedFrame { id: "2".into(), project: Some(2), folder: None },
            ],
        };
        save_layout(&engine, &laid).unwrap();
        name_frame(&engine, "1", "the migration", Person).unwrap();

        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back, laid);
        assert_eq!(frame_names(&engine).unwrap()["1"].name, "the migration");
    }

    /// An arrangement written before panes belonged to a project still parses: what to do with a
    /// pane whose project nothing records is the window's, and refusing to read the row would take
    /// away the folder it could have answered with.
    #[test]
    fn a_frame_kept_without_a_project_still_comes_back() {
        let engine = StoreEngine::open_in_memory().unwrap();
        engine
            .set_meta(
                LAYOUT_META,
                Some(r#"{"count":2,"nextId":2,"frames":[{"id":"1","folder":"/work/repo"}]}"#),
            )
            .unwrap();
        let back = saved_layout(&engine).unwrap().expect("the arrangement");
        assert_eq!(back.project, None, "nothing said which project the face was on");
        assert_eq!(back.frames[0].project, None, "nothing said which project it was");
        assert_eq!(back.frames[0].folder.as_deref(), Some("/work/repo"));
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
        assert!(!accepts(Some(&person), Session), "`session name` does not take a person's word back");
        assert!(!accepts(Some(&person), Typed));
        assert!(accepts(Some(&person), Person), "and a person may change their mind");
        let session = FrameName { name: "reading the store".into(), by: Session };
        assert!(accepts(Some(&session), Session), "an agent may say something newer than itself");
        assert!(accepts(Some(&session), Person));
    }
}
