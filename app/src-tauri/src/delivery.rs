//! The GUI's mount of the outbox drive — the **long-lived face** (`AMB-D-367`).
//!
//! The cursor is not this module's to hold. Core's [`outbox_drive`](amenbo_core::outbox_drive) reads it
//! from the store, carries out what committed since, and stores where it advanced to; both faces drive that
//! same one (`AMB-D-380`). A session cursor kept here instead left the events this process carried still
//! standing in the outbox, so the next `amenbo` command on the command line carried them a second time —
//! one GUI action, two notifications. What is left here is what is genuinely the GUI's:
//!
//! - **The drive rides the write seam.** Commands open the store per action
//!   (`commands::with_store_mut`), so there is no long-lived store to hang a loop off. The drive runs once
//!   after each mutating command committed, on that command's own open store — the same shape the CLI has
//!   at its write seam.
//! - **And once as the app comes up** ([`resume`](crate::delivery::resume), `AMB-D-399`), for what a previous run left half carried.
//!   That is the one thing the write seam cannot reach: a session spent reading makes no write for it to
//!   ride.
//! - **The sender and the carrier are processes, and nothing here waits for one** (`AMB-D-885`,
//!   `AMB-D-884`). Each is this same executable, re-run through its own flag — so it finishes whether the
//!   app is still up or the user has quit it, and a write never waits on a relay (`AMB-D-352`).
//!
//! There is no session start to set, either: whatever sits past the stored cursor is uncarried, and a GUI
//! launched today carries it if no CLI run already did — one cursor is the only answer to how far this
//! store has been carried out (`AMB-D-380`). The startup kick is not a session position; it is the same
//! drive from that same cursor, made at the moment a leftover would otherwise wait for a write.
//!
//! Nothing here fails a command. A store that will not answer: the mutation is already committed, so this
//! warns and the next write tries again.

use amenbo_core::outbox_drive::Face;
use amenbo_core::Store;

/// The door a **notification sender** is launched through (`AMB-D-885`): this executable, re-run to post
/// one drive's worth of messages and exit. The app is not a command-line tool, so what it answers is a
/// flag, checked by its `main` ahead of everything else, rather than a subcommand.
///
/// It is deliberately not something a user would land on: an app started this way puts up no window, and
/// the only caller is Amenbo itself.
pub const SENDER_FLAG: &str = "--notify-sender";

/// The argv prefix core re-runs this executable through to send. Core follows it with [`SENDER_ARGS`] of
/// its own — the store's base directory, and nothing else: the messages come over stdin.
pub const SENDER_ARGV: &[&str] = &[SENDER_FLAG];

/// The one argument core appends after [`SENDER_ARGV`] — the store's base directory.
pub const SENDER_ARGS: usize = 1;

/// The same door, for a **Viewer carrier** (`AMB-D-884`): this executable, re-run to take one turn of the
/// send and exit. An app started this way puts up no window either, and the only caller is Amenbo itself.
pub const CARRIER_FLAG: &str = "--viewer-carrier";

/// The argv prefix core re-runs this executable through to carry. Core follows it with [`CARRIER_ARGS`] of
/// its own — the store's base directory, and nothing else: what a carrier carries is in that store.
pub const CARRIER_ARGV: &[&str] = &[CARRIER_FLAG];

/// The one argument core appends after [`CARRIER_ARGV`] — the store's base directory.
pub const CARRIER_ARGS: usize = 1;

/// Carry a write out once over everything committed since the store's cursor, and store where it advanced
/// to. Call it after a mutating command committed, on that command's still-open store.
pub fn drive(store: &Store) {
    // The Viewer is set off beside the drive and not through it: what a carrier carries is the backlog, so
    // which record moved decides nothing here — a write happened, and the phone is now behind
    // (`AMB-D-884`). It is a process, so this waits for none of it.
    store.set_the_viewer_off(CARRIER_ARGV);
    // The returned `Walked` is dropped here: the sender it started is a process of its own, and the cursor
    // it advanced to is already stored.
    if let Err(e) = store.drive_delivery(Face::Gui, SENDER_ARGV) {
        log::warn!("could not carry this write out: {e}");
    }
}

/// Pick up what a previous run left half carried, once, as the app comes up (`AMB-D-399`).
///
/// The write seam above cannot answer for those rows: it rides a mutation, and a session spent reading —
/// the ordinary way this app is used — never makes one. So the app asks as it starts, and a walk cut short
/// last time resumes here instead of waiting for somebody to type something.
///
/// It opens a store of its own because there is no long-lived one to borrow (`commands::with_store_mut`
/// opens per action), which is also why this belongs on a background thread: neither the open nor the drive
/// is the window's to wait for. A store that will not open is no reason to hold the app up — the rows keep,
/// and the next write carries them.
pub fn resume() {
    let store = match Store::open() {
        Ok(store) => store,
        Err(e) => {
            log::warn!("could not open the store to resume carrying writes out: {e}");
            return;
        }
    };
    if let Err(e) = store.resume_delivery(Face::Gui, SENDER_ARGV) {
        log::warn!("could not carry out what a previous run left standing: {e}");
    }
    // The Viewer's half of the same kick (`AMB-D-884`). A carrier that died between reading the backlog
    // out and placing it leaves a queue, and only a write sets one off — so a session spent reading, which
    // is the ordinary way this app is used, would never reach those rows. What it asks before starting
    // anything is core's, and on a device with an empty queue it is one count.
    store.carry_what_was_left_behind(CARRIER_ARGV);
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::config::Paths;
    use amenbo_core::model::{ActorKind, View};
    use amenbo_core::outbox_drive::persisted_cursor;

    fn temp_store(tag: &str) -> Store {
        Store::open_at(Paths::at(amenbo_scratch::scratch(tag))).unwrap()
    }

    /// File a task and end its creation, so the outbox gains one `task.created` event — the second stage
    /// is what fires it (`AMB-D-557`), the first announcing nothing.
    fn add_task(store: &mut Store, project: i64, title: &str) {
        let id = store
            .add_task(amenbo_core::ops::task::NewTask {
                title: title.to_string(),
                project_id: Some(project),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(ActorKind::Human),
                at_binding_id: None,
            })
            .unwrap()
            .id;
        store.finish_task_creation(id, ActorKind::Human).unwrap();
    }

    fn a_project(store: &mut Store) -> i64 {
        store
            .project_add(amenbo_core::ops::project::NewProject {
                name: "PJ".to_string(),
                view: View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id
    }

    /// The GUI's drive walks the **stored** cursor, so what it delivered is behind the cursor the next CLI
    /// run reads — the second fire `AMB-D-380` closes. Nothing is installed here, so nothing fires; the walk
    /// and where it is kept are the point.
    #[test]
    fn driving_advances_the_stored_cursor() {
        let mut store = temp_store("dispatch-drive");
        let project = a_project(&mut store);

        add_task(&mut store, project, "発火対象");
        drive(&store);
        let after = persisted_cursor(store.read_model()).unwrap();
        assert!(after > 0, "the drive walked the cursor past the event it delivered");

        drive(&store);
        assert_eq!(
            persisted_cursor(store.read_model()).unwrap(),
            after,
            "a second drive over the same events moves nothing"
        );
    }
}
