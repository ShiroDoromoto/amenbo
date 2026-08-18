//! The **change signal** — `store.changed`, the one event amenbo fires from the ledger rather than from a
//! write point (`AMB-D-582`).
//!
//! What it is for is a reader outside the store that carries a whole copy of its window: it needs to know
//! *that* something changed, never what. So the signal is seamed on the change feed's drain, which is what
//! makes it reach the many writes no semantic event names — a notes edit, a due date, a classification put
//! on, an edge drawn, an attachment gone — and it carries the version and nothing else.
//!
//! These tests drive it through the public `Store` wrappers, the seam CLI and GUI share, and read the
//! outbox back. The semantic half has its own file (`plugin_outbox_emit.rs`); the two ride the same table
//! and are deliberately kept apart here, because what each one promises is different.

use amenbo_core::config::Paths;
use amenbo_core::model::ActorKind;
use amenbo_core::plugin_payload::{name, Payload};
use amenbo_core::store_engine::outbox::{events_since, outbox_head, OutboxRow, OutboxSlice};
use amenbo_core::Store;

fn temp_store() -> Store {
    let base = amenbo_scratch::scratch("change-signal");
    Store::open_at(Paths::at(base)).unwrap()
}

fn new_project(name: &str) -> amenbo_core::ops::project::NewProject {
    amenbo_core::ops::project::NewProject {
        name: name.to_string(),
        view: amenbo_core::model::View::Board,
        notes: String::new(),
        color: None,
    }
}

fn new_task(title: &str, project_id: i64) -> amenbo_core::ops::task::NewTask {
    amenbo_core::ops::task::NewTask {
        title: title.to_string(),
        project_id: Some(project_id),
        due_on: None,
        start_on: None,
        priority: None,
        notes: String::new(),
        created_by_kind: Some(ActorKind::Ai),
        at_binding_id: None,
    }
}

fn filed(store: &mut Store, input: amenbo_core::ops::task::NewTask) -> i64 {
    let id = store.add_task(input).unwrap().id;
    store.finish_task_creation(id, ActorKind::Ai).unwrap();
    id
}

fn head(store: &Store) -> i64 {
    outbox_head(store.read_model().conn()).unwrap()
}

/// The signals the outbox gained after `after` — the semantic events beside them are another file's.
fn signals(store: &Store, after: i64) -> Vec<OutboxRow> {
    match events_since(store.read_model().conn(), after, 10_000).unwrap() {
        OutboxSlice::Events { rows, .. } => {
            rows.into_iter().filter(|r| r.event == name::STORE_CHANGED).collect()
        }
        OutboxSlice::Gap => panic!("nothing was trimmed, so there is no gap"),
    }
}

/// The signal a write left, or a loud failure — one write, one signal.
fn only_signal(store: &Store, after: i64) -> OutboxRow {
    let mut rows = signals(store, after);
    assert_eq!(rows.len(), 1, "exactly one signal: {rows:?}");
    rows.pop().unwrap()
}

/// The plain case: a write leaves one signal, stamped with the project it happened in — which is what the
/// fan-out routes a project-scoped subscription on — and carrying the version that project is now at.
#[test]
fn a_write_leaves_one_signal_carrying_the_version() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let h = head(&store);
    let task = store.add_task(new_task("タスク", project)).unwrap().id;
    let signal = only_signal(&store, h);

    assert_eq!(signal.record_id, project, "the signal is about the project, not the task");
    assert_eq!(signal.project, Some(project), "and is stamped with it, so the fan-out can route it");
    assert!(signal.actor.is_empty(), "the ledger names no actor: {signal:?}");

    let rebuilt = Payload::from_outbox_row(&signal).unwrap();
    assert!(rebuilt.actor.is_none(), "and none is invented on the way to the wire");
    // The version the signal carried is the one the store now answers with, so a reader that acts on the
    // signal and one that polls the version reach the same conclusion.
    assert_eq!(rebuilt.version, Some(store_version(&store, project)));

    let _ = task;
}

/// The reason the signal is seamed on the ledger and not on the write points: it fires for the changes no
/// semantic event names. A notes edit, a due date, a classification — none of the eleven says any of them,
/// and every one of them makes a carried copy stale.
#[test]
fn it_fires_for_the_changes_no_semantic_event_names() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("タスク", project));

    for patch in [
        amenbo_core::ops::task::TaskPatch { notes: Some("書き直した".into()), ..Default::default() },
        amenbo_core::ops::task::TaskPatch {
            priority: Some(amenbo_core::model::Priority::High),
            ..Default::default()
        },
    ] {
        let h = head(&store);
        store.update_task(task, patch).unwrap();
        let signal = only_signal(&store, h);
        assert_eq!(
            Payload::from_outbox_row(&signal).unwrap().version,
            Some(store_version(&store, project)),
            "the signal carries the version the write moved the project to",
        );
    }
}

/// A write that changed nothing signals nothing. The signal rides the change feed's drain, so "no rows
/// moved" and "no signal" are the same fact — a reader is never sent re-reading by a no-op.
///
/// **The clock is moved on first, and that is the whole test.** A stamp is kept to the second, so a
/// no-op that composed the row anyway would come out identical to what is already stored — and pass
/// here — for as long as both calls land inside one second. Crossing the boundary is what asks the
/// question: with `updated_at` now different, anything that writes the row back moves a column, and one
/// column moved is a feed row, a project version and a signal. Left to chance it is a test that passes
/// on a fast machine and fails on a slow one, which is what it did.
#[test]
fn a_write_that_changed_nothing_signals_nothing() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("タスク", project));
    wait_out_the_second();

    let h = head(&store);
    store.finish_task_creation(task, ActorKind::Ai).unwrap();
    assert!(signals(&store, h).is_empty(), "a creation already over moved nothing");
}

/// Wait until the wall clock's second has ticked over. Under a second, and only in the one test whose
/// subject it is.
fn wait_out_the_second() {
    let second = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is past the epoch")
            .as_secs()
    };
    let start = second();
    while second() == start {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// One transaction, one signal per project — never one per row it touched. A task delete carries its
/// comments off with it and fires a semantic event for each; the ledger says once that the project moved,
/// because that is all a reader carrying a copy has to act on.
#[test]
fn one_transaction_signals_once_however_many_rows_it_moved() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("タスク", project));
    for text in ["ひとつ", "ふたつ", "みっつ"] {
        store.add_task_comment(task, ActorKind::Ai, text).unwrap();
    }

    let h = head(&store);
    store.delete_task(task, ActorKind::Ai).unwrap();
    let _ = only_signal(&store, h);
}

/// Re-homing moves two projects at once, and each is told on its own: a reader watching the project the
/// task left has as much to re-read as one watching where it landed.
#[test]
fn re_homing_signals_both_projects() {
    let mut store = temp_store();
    let from = store.project_add(new_project("元")).unwrap().id;
    let to = store.project_add(new_project("先")).unwrap().id;
    let task = filed(&mut store, new_task("引っ越す", from));

    let h = head(&store);
    store.move_task(task, Some(to), amenbo_core::ops::Position::Bottom, ActorKind::Ai).unwrap();

    let mut signalled: Vec<i64> = signals(&store, h).iter().filter_map(|r| r.project).collect();
    signalled.sort_unstable();
    assert_eq!(signalled, vec![from.min(to), from.max(to)], "both ends were told");
}

/// The version a project is at, read the way a plugin's window reads it.
fn store_version(store: &Store, project: i64) -> i64 {
    Store::open_at(store.paths.clone())
        .unwrap()
        .with_reach(amenbo_core::reach::Reach::window(project))
        .sync_version()
        .unwrap()
}
