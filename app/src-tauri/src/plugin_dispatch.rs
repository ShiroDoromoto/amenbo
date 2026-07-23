//! The GUI's mount of the single observation-hook dispatcher — the **long-lived face** (`AMB-D-367`).
//!
//! Core's [`deliver`](amenbo_core::plugin_dispatch::deliver) holds no cursor: it drains the outbox past
//! the one it is given and returns the one to keep. The CLI persists that cursor in the store between its
//! short runs; this process outlives every write it makes, so it keeps the cursor **in memory** for the
//! whole session and never stores one. Three consequences shape this module:
//!
//! - **The session starts at the outbox head.** A GUI launched today is not an observer of what happened
//!   last week, so [`crate::plugin_dispatch::prime`] sets the cursor to the head rather than to zero —
//!   otherwise the first write after launch would re-fire the entire backlog.
//! - **The drive rides the write seam.** Commands open the store per action
//!   (`commands::with_store_mut`), so there is no long-lived store to hang a loop off. The dispatcher runs
//!   once after each mutating command committed, on that command's own open store — the same shape the
//!   CLI has at its write seam, minus the persistence.
//! - **The hooks are dropped, not joined.** Dropping the handles is true fire-and-forget
//!   (`AMB-D-352`): this process is not about to exit, so a hook it launched runs to its own end. Joining
//!   would make every write wait on a subprocess.
//!
//! Nothing here fails a command. A store that will not answer, a plugins directory that will not read: the
//! mutation is already committed, so the dispatcher warns and the next write tries again.

use std::sync::Mutex;

use amenbo_core::plugin_subscribe::EnabledSubscribers;
use amenbo_core::{plugin_installed, Store};

/// This session's dispatch cursor: the id of the last outbox event delivered, `None` until the session
/// start is known. It is process state, not store state — the store's own cursor row belongs to the CLI
/// face (`plugin_drive::CURSOR_META`), and a GUI that wrote there would drag the two consumers onto one
/// cursor.
static CURSOR: Mutex<Option<i64>> = Mutex::new(None);

/// Set this session's starting cursor to the outbox head, if it is not set yet — so the session observes
/// what fires *next* (see the module docs). Idempotent: a second call leaves an already-started session
/// where it is, which is what lets the write seam call it defensively.
pub fn prime(store: &Store) {
    prime_in(&CURSOR, store);
}

/// Drive the dispatcher once over everything committed since this session's cursor, and keep where it
/// advanced to. Call it after a mutating command committed, on that command's still-open store.
pub fn drive(store: &Store) {
    drive_in(&CURSOR, store);
}

/// Open the store read-only and [`prime`] from it — the launch-time half, off the main thread because it
/// touches the disk. Called once the migration gate is through (`lib::start_store_threads`); a failure is
/// only a warning, since the first write primes the cursor as well.
pub fn prime_at_startup() {
    match amenbo_core::config::Paths::resolve().map_err(|e| e.to_string()).and_then(|paths| {
        Store::open_read_at(paths).map_err(|e| e.to_string())
    }) {
        Ok(store) => prime(&store),
        Err(e) => log::warn!("could not open the store to start the plugin dispatcher: {e}"),
    }
}

/// [`prime`] against a caller-held cursor cell, so a test can drive one of its own instead of the
/// process-wide session.
fn prime_in(cursor: &Mutex<Option<i64>>, store: &Store) {
    let mut cursor = lock(cursor);
    if cursor.is_some() {
        return;
    }
    match store.plugin_outbox_head() {
        Ok(head) => *cursor = Some(head),
        Err(e) => log::warn!("could not read the plugin outbox head; the dispatcher stays unstarted: {e}"),
    }
}

/// [`drive`] against a caller-held cursor cell (see [`prime_in`]).
fn drive_in(cursor: &Mutex<Option<i64>>, store: &Store) {
    let mut cursor = lock(cursor);
    let Some(from) = *cursor else {
        // No start was ever read, so there is no honest place to drain from: draining from zero would
        // re-fire the whole backlog. Leave it for the next write, which primes before it mutates.
        return;
    };
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
    let subscribers = EnabledSubscribers::new(&installed, &store.config, store);
    match store.deliver_plugins(from, &subscribers) {
        // `delivered` is dropped here, and with it the hooks' handles: fire-and-forget (`AMB-D-352`).
        Ok(delivered) => *cursor = Some(delivered.cursor),
        Err(e) => log::warn!("could not dispatch the plugin observation hooks: {e}"),
    }
}

/// Take the cursor lock, taking a poisoned one as it stands. A panic in another thread says nothing about
/// an integer cursor's validity, and refusing to dispatch for the rest of the session would be a worse
/// answer than carrying on.
fn lock(cursor: &Mutex<Option<i64>>) -> std::sync::MutexGuard<'_, Option<i64>> {
    cursor.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::config::Paths;
    use amenbo_core::model::{ActorKind, View};

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

    /// A session starts at the head, so the backlog from before launch is never delivered — and priming
    /// again does not move a session that has already started.
    #[test]
    fn priming_starts_at_the_head_and_only_once() {
        let mut store = temp_store("dispatch-prime");
        let project = a_project(&mut store);
        add_task(&mut store, project, "起動前");
        let head = store.plugin_outbox_head().unwrap();
        assert!(head > 0, "the backlog is not empty");

        let cursor = Mutex::new(None);
        prime_in(&cursor, &store);
        assert_eq!(*lock(&cursor), Some(head), "the session starts at what is already there");

        add_task(&mut store, project, "起動後");
        prime_in(&cursor, &store);
        assert_eq!(*lock(&cursor), Some(head), "a started session is left where it is");
    }

    /// The drive walks the cursor to the head of what committed since, so a second drive over the same
    /// events delivers nothing again. Nothing is installed here, so nothing fires — the walk is the point.
    #[test]
    fn driving_advances_the_session_cursor() {
        let mut store = temp_store("dispatch-drive");
        let project = a_project(&mut store);
        let cursor = Mutex::new(None);
        prime_in(&cursor, &store);

        add_task(&mut store, project, "発火対象");
        drive_in(&cursor, &store);
        let after = *lock(&cursor);
        assert_eq!(after, Some(store.plugin_outbox_head().unwrap()));

        drive_in(&cursor, &store);
        assert_eq!(*lock(&cursor), after, "a second drive over the same events moves nothing");
    }

    /// An unstarted session drains nothing: draining from zero would re-fire the whole backlog, so the
    /// drive leaves the cursor unset for a later prime.
    #[test]
    fn an_unstarted_session_drives_nothing() {
        let mut store = temp_store("dispatch-unstarted");
        let project = a_project(&mut store);
        add_task(&mut store, project, "配送されない");

        let cursor = Mutex::new(None);
        drive_in(&cursor, &store);
        assert_eq!(*lock(&cursor), None);
    }
}
