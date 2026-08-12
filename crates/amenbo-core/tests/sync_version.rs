//! The sync version: the one number a reader carrying a copy of this store out asks to decide whether to
//! re-send its window (`AMB-D-582`). Two properties are what these pin — **it moves on a write** and
//! **it stays put without one** — and with them the question that makes it worth stamping at all:
//! *whose* write moves it. A project's version is left alone by every write in another project, so churn
//! next door sends nobody re-reading 13MB.
//!
//! The third property, that it never rewinds, is not tested here because nothing in the code could make
//! it: the number is a stored `change_feed` id, `AUTOINCREMENT` hands none out twice, and the feed's
//! truncation only ever removes rows *below* it. Deriving the version from the feed instead is what would
//! put a rewind within reach — which is the reason it is stamped.
//!
//! Like the change-feed tests beside them, these go through the **public ops** (`Store::…`) rather than
//! poking the engine: a mutation added tomorrow goes through the same write door, so it inherits the
//! version without anyone remembering to stamp it.

use amenbo_core::config::Paths;
use amenbo_core::model::ActorKind;
use amenbo_core::reach::Reach;
use amenbo_core::Store;

fn temp_store() -> Store {
    let base = amenbo_scratch::scratch("sync-version");
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

/// File a task and finish creating it — both stages (`AMB-D-554`), the way the surfaces do.
fn filed(store: &mut Store, input: amenbo_core::ops::task::NewTask) -> i64 {
    let id = store.add_task(input).unwrap().id;
    store.finish_task_creation(id, ActorKind::Human).unwrap();
    id
}

/// One project's version, asked the way a plugin's window asks it — through a closed reach, which is the
/// only way to ask for a project rather than for the whole device.
fn version_of(store: &Store, project: i64) -> i64 {
    // `with_reach` consumes, so this reads through a clone of the open rather than narrowing the caller's.
    Store::open_at(store.paths.clone())
        .unwrap()
        .with_reach(Reach::window(project))
        .sync_version()
        .unwrap()
}

/// The plain case: a write inside the project moves its version, and a read that changes nothing leaves
/// it exactly where it was.
#[test]
fn a_write_moves_the_version_and_nothing_else_does() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let after_create = version_of(&store, project);
    let task = filed(&mut store, new_task("タスク", project));
    let after_task = version_of(&store, project);
    assert!(after_task > after_create, "filing a task moved it: {after_create} → {after_task}");

    // A read is not a write.
    store.task_detail(task).unwrap();
    assert_eq!(version_of(&store, project), after_task, "a read left it where it was");

    // The writes no observation event carries move it too (`AMB-D-582`) — a notes edit, a priority.
    // Whether a subscriber would hear about a change is a different question from whether its copy of
    // the project went stale, and this number answers the second one.
    let mut seen = after_task;
    for patch in [
        amenbo_core::ops::task::TaskPatch { notes: Some("書き直した".into()), ..Default::default() },
        amenbo_core::ops::task::TaskPatch {
            priority: Some(amenbo_core::model::Priority::High),
            ..Default::default()
        },
    ] {
        store.update_task(task, patch).unwrap();
        let now = version_of(&store, project);
        assert!(now > seen, "the update moved it: {seen} → {now}");
        seen = now;
    }
}

/// The property that makes it worth stamping rather than deriving: a project is left alone by writes
/// next door. Everything a busy project does — creating, updating, deleting — passes the quiet one by.
#[test]
fn a_project_is_left_alone_by_writes_in_another() {
    let mut store = temp_store();
    let quiet = store.project_add(new_project("静")).unwrap().id;
    let busy = store.project_add(new_project("動")).unwrap().id;
    let _ = filed(&mut store, new_task("既にある", quiet));

    let held = version_of(&store, quiet);

    let churn = filed(&mut store, new_task("よそのタスク", busy));
    store.set_task_status(churn, amenbo_core::model::TaskStatus::InProgress, ActorKind::Ai).unwrap();
    store.add_task_comment(churn, ActorKind::Ai, "よそのコメント").unwrap();
    store.delete_task(churn, ActorKind::Ai).unwrap();

    assert_eq!(version_of(&store, quiet), held, "the quiet project did not move");
    assert!(version_of(&store, busy) > held, "the busy one did");
}

/// A deletion moves the version of the project the row was **in** — which is the one thing a version
/// derived from the feed after the fact could never say, because by then the row cannot be asked where
/// it lived.
#[test]
fn a_deletion_moves_the_version_of_the_project_it_left() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("消す", project));

    let before = version_of(&store, project);
    store.delete_task(task, ActorKind::Ai).unwrap();

    assert!(version_of(&store, project) > before, "the project the task left moved");
}

/// Re-homing is the other case the declaration exists for: the task is in two projects across one
/// transaction, and **both** ends have to move — the one that lost it as much as the one that gained it.
#[test]
fn re_homing_moves_both_ends() {
    let mut store = temp_store();
    let from = store.project_add(new_project("元")).unwrap().id;
    let to = store.project_add(new_project("先")).unwrap().id;
    let task = filed(&mut store, new_task("引っ越す", from));

    let (from_before, to_before) = (version_of(&store, from), version_of(&store, to));
    store.move_task(task, Some(to), amenbo_core::ops::Position::Bottom, ActorKind::Ai).unwrap();

    assert!(version_of(&store, from) > from_before, "the project it left moved");
    assert!(version_of(&store, to) > to_before, "the project it landed in moved");
}

/// The erase that goes around the ordinary write door still moves the version. A comment erased for good
/// (`hard-erase`) but left at the same version would live on in every copy carried out of this store —
/// the exact opposite of what that capability is for.
#[test]
fn a_hard_erase_moves_the_version() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("タスク", project));
    let comment = store.add_task_comment(task, ActorKind::Human, "消える言葉").unwrap().id;

    let before = version_of(&store, project);
    store
        .hard_erase(&[amenbo_core::store::HardEraseTarget::TaskComment { id: comment }])
        .unwrap();

    assert!(version_of(&store, project) > before, "the erase moved it");
}

/// Read through the whole device — the human's reach, and the GUI's — the version is the store's own,
/// and it moves with **any** committed write. A project's version is an id from that same feed, so it
/// never runs ahead of it.
#[test]
fn the_whole_device_has_a_version_of_its_own_that_no_project_outruns() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let before = store.sync_version().unwrap();
    let _ = filed(&mut store, new_task("タスク", project));
    let after = store.sync_version().unwrap();

    assert!(after > before, "the store's own version moved: {before} → {after}");
    assert!(version_of(&store, project) <= after, "a project never runs ahead of the store");
}

/// A project nothing has written reads `0` — below every id the feed will hand out next, so the first
/// write after that carries it forward and whoever is watching sends the whole thing once.
#[test]
fn a_project_nothing_has_written_starts_below_every_id_to_come() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    assert_eq!(version_of(&store, project), 0, "creating it stamped nothing");

    let _ = filed(&mut store, new_task("最初の1件", project));
    assert!(version_of(&store, project) > 0, "the first write carried it forward");
}
