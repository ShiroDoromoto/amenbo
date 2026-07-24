//! The GUI's mount of the single observation-hook dispatcher — the **long-lived face** (`AMB-D-367`).
//!
//! The cursor is not this module's to hold. Core's [`drive_persisted`](amenbo_core::plugin_drive) reads it
//! from the store, delivers what committed since, and stores where it advanced to; both faces drive that
//! same one (`AMB-D-380`). A session cursor kept here instead left the events this process delivered still
//! standing in the outbox, so the next `amenbo` command on the command line fired them a second time — one
//! GUI action, two notifications. What is left here is what is genuinely the GUI's:
//!
//! - **The drive rides the write seam.** Commands open the store per action
//!   (`commands::with_store_mut`), so there is no long-lived store to hang a loop off. The dispatcher runs
//!   once after each mutating command committed, on that command's own open store — the same shape the
//!   CLI has at its write seam.
//! - **The runners are dropped, not waited on.** Dropping the handles is true fire-and-forget
//!   (`AMB-D-352`): this process is not about to exit, so a runner it started works its plugin's queue to
//!   the end on its own. Waiting would make every write wait on a subprocess.
//!
//! There is no session start to set, either: whatever sits past the stored cursor is undelivered, and a GUI
//! launched today delivers it if no CLI run already did — one cursor is the only answer to how far this
//! store has been delivered (`AMB-D-380`).
//!
//! Nothing here fails a command. A store that will not answer, a plugins directory that will not read: the
//! mutation is already committed, so the dispatcher warns and the next write tries again.

use amenbo_core::plugin_drive::Face;
use amenbo_core::plugin_subscribe::EnabledSubscribers;
use amenbo_core::{plugin_installed, Store};

/// Drive the dispatcher once over everything committed since the store's cursor, and store where it
/// advanced to. Call it after a mutating command committed, on that command's still-open store.
pub fn drive(store: &Store) {
    // Who is installed is read off disk once per drive and handed to the resolver, which stays a pure
    // function of the state it is given. A directory that will not read is not "nothing is installed":
    // leave the events in the outbox and the cursor where it is, rather than walk past events no
    // subscriber was ever offered.
    let installed = match plugin_installed::installed(&store.paths) {
        Ok(installed) => installed,
        Err(e) => {
            log::warn!("could not read the installed plugins, so none was dispatched: {e}");
            return;
        }
    };
    let subscribers = EnabledSubscribers::new(&installed, store);
    // The returned `Delivered` is dropped here, and with it the runners' handles: fire-and-forget
    // (`AMB-D-352`). The cursor it advanced to is already stored.
    if let Err(e) = store.drive_plugins_persisted(Face::Gui, &subscribers) {
        log::warn!("could not dispatch the plugin observation hooks: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::config::Paths;
    use amenbo_core::model::{ActorKind, View};
    use amenbo_core::plugin_drive::persisted_cursor;

    fn temp_store(tag: &str) -> Store {
        Store::open_at(Paths::at(amenbo_scratch::scratch(tag))).unwrap()
    }

    /// Add a task, so the outbox gains one `task.created` event.
    fn add_task(store: &mut Store, project: i64, title: &str) {
        store
            .add_task(amenbo_core::ops::task::NewTask {
                title: title.to_string(),
                project_id: Some(project),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(ActorKind::Human),
            })
            .unwrap();
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
