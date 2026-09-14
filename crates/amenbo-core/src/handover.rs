//! **What the plugins became**, for the surfaces to say once (`AMB-D-884`).
//!
//! The migration that took mail, slack, viewer and worktree into the body
//! ([`crate::store_engine::migrate`]'s v42) leaves one `store_meta` row behind saying which of them this
//! device had and how much came across. It is not a log: a person who installed a plugin and found it
//! gone deserves to be told where it went, and the row is what lets each surface tell them without the
//! other having to.
//!
//! **Each surface is marked told on its own.** The CLI and the app are two places a person meets this,
//! and whichever of them speaks first must not silence the other — so the row records who has said it
//! rather than being taken away by the first reader. It stays afterwards, being a few dozen bytes and a
//! true account of what the migration did.
//!
//! **Nothing here is a road out.** The row names plugins and counts; no address and no credential is in
//! it, which is why it rides in `store_meta` beside the version stamp rather than in the table no road
//! out walks.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::store_engine::StoreEngine;

/// Where the account sits. Written by the migration step, in frozen text on that side.
const KEY: &str = "plugins_carried_in";

/// The surface a person meets this on. A name rather than an enum on the wire, so a surface added later
/// reads a row an older build wrote without the row having to change.
pub mod surface {
    /// The terminal.
    pub const CLI: &str = "cli";
    /// The app's window.
    pub const GUI: &str = "gui";
}

/// What the handover carried, as the migration wrote it down.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handover {
    /// Which of the four were installed on this device, in the order the step takes them.
    #[serde(default)]
    pub plugins: Vec<String>,
    /// How many connections landed on the device's notification shelf.
    #[serde(default)]
    pub targets: usize,
    /// How many projects came away with notification settings of their own.
    #[serde(default)]
    pub projects: usize,
    /// Whether anything of the Viewer was taken in — its keys, or the switch.
    #[serde(default)]
    pub viewer: bool,
    /// The surfaces that have already said it.
    #[serde(default)]
    told: Vec<String>,
}

impl Handover {
    /// Has `surface` said it yet?
    fn told(&self, surface: &str) -> bool {
        self.told.iter().any(|s| s == surface)
    }

    /// Did any of the four notifiers come across? The Viewer and worktree are said differently, so a
    /// surface asks this before writing a sentence about targets nobody has.
    pub fn carried_notifications(&self) -> bool {
        self.targets > 0
    }
}

/// What this surface still owes the person, or `None` when there is nothing to say — no handover ran on
/// this device, or this surface has already said it.
///
/// A row that will not parse reads as nothing to say. The account is a courtesy; refusing to start over
/// one that a later build reshaped would be the worse of the two outcomes.
pub fn waiting(engine: &StoreEngine, surface: &str) -> Result<Option<Handover>> {
    let Some(raw) = engine.get_meta(KEY)? else { return Ok(None) };
    let Ok(held) = serde_json::from_str::<Handover>(&raw) else { return Ok(None) };
    Ok((!held.told(surface)).then_some(held))
}

/// Write down that `surface` has said it. Idempotent, and it leaves everything else in the row where it
/// was — the other surface's turn is still owed.
pub fn mark_told(engine: &StoreEngine, surface: &str) -> Result<()> {
    let Some(raw) = engine.get_meta(KEY)? else { return Ok(()) };
    let Ok(mut held) = serde_json::from_str::<Handover>(&raw) else { return Ok(()) };
    if held.told(surface) {
        return Ok(());
    }
    held.told.push(surface.to_string());
    let Ok(text) = serde_json::to_string(&held) else { return Ok(()) };
    engine.set_meta(KEY, Some(&text))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_store() -> StoreEngine {
        let dir = amenbo_scratch::scratch("handover");
        StoreEngine::open(&dir.join("store.sqlite")).unwrap()
    }

    fn write(engine: &StoreEngine, raw: &str) {
        engine.set_meta(KEY, Some(raw)).unwrap();
    }

    /// Both surfaces owe the same sentence, and neither saying it lets the other off — a person who
    /// upgraded from the terminal still opens the app and finds the plugin gone.
    #[test]
    fn each_surface_owes_it_until_that_surface_has_said_it() {
        let engine = a_store();
        write(&engine, r#"{"plugins":["slack"],"targets":1,"projects":2,"viewer":false,"told":[]}"#);

        let owed = waiting(&engine, surface::CLI).unwrap().expect("the terminal owes it");
        assert_eq!(owed.plugins, vec!["slack".to_string()]);
        assert_eq!(owed.targets, 1);
        assert_eq!(owed.projects, 2);
        assert!(owed.carried_notifications());

        mark_told(&engine, surface::CLI).unwrap();
        assert_eq!(waiting(&engine, surface::CLI).unwrap(), None, "said once is said");
        assert!(waiting(&engine, surface::GUI).unwrap().is_some(), "the window has still not said it");

        mark_told(&engine, surface::GUI).unwrap();
        assert_eq!(waiting(&engine, surface::GUI).unwrap(), None);
        // Marking twice changes nothing, and the account itself stays readable.
        mark_told(&engine, surface::GUI).unwrap();
        assert!(engine.get_meta(KEY).unwrap().is_some(), "the row is the account, not a flag to burn");
    }

    /// A device no handover ran on owes nothing, and neither does one whose row a later build wrote in a
    /// shape this one cannot read — an account nobody can parse is not worth refusing to start over.
    #[test]
    fn nothing_to_say_is_the_answer_for_an_absent_or_unreadable_account() {
        let engine = a_store();
        assert_eq!(waiting(&engine, surface::CLI).unwrap(), None);

        write(&engine, "not json at all");
        assert_eq!(waiting(&engine, surface::CLI).unwrap(), None);
        mark_told(&engine, surface::CLI).unwrap();
        assert_eq!(engine.get_meta(KEY).unwrap().as_deref(), Some("not json at all"), "left as it lies");
    }

    /// A device that had only `worktree` carried no notification settings, and the sentence it is owed
    /// says so rather than naming a shelf with nothing on it.
    #[test]
    fn a_device_that_carried_no_connection_says_so() {
        let engine = a_store();
        write(&engine, r#"{"plugins":["worktree"],"targets":0,"projects":0,"viewer":false,"told":[]}"#);
        let owed = waiting(&engine, surface::CLI).unwrap().unwrap();
        assert!(!owed.carried_notifications());
        assert!(!owed.viewer);
    }
}
