//! The semantic-event layer, driven through the **public ops** (`Store::…`). These pin the property
//! the layer exists for: a committed write, whatever drove it, surfaces as a fired lifecycle event —
//! route-independent, because the layer reads the one change feed every write passes through. A sink
//! added tomorrow, or a mutation added tomorrow, inherits this without anyone wiring the two together.

use std::sync::{Arc, Mutex};

use amenbo_core::config::Paths;
use amenbo_core::model::ActorKind;
use amenbo_core::plugin_events::{name, Event, EventLayer, EventSink};
use amenbo_core::store_engine::read;
use amenbo_core::Store;

fn temp_store() -> Store {
    let base = amenbo_scratch::scratch("plugin-events");
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
    }
}

/// A sink that just records what it was handed, so a test can read back the fired events in order.
#[derive(Clone, Default)]
struct Spy(Arc<Mutex<Vec<Event>>>);

impl Spy {
    fn events(&self) -> Vec<Event> {
        self.0.lock().unwrap().clone()
    }
}

impl EventSink for Spy {
    fn emit(&self, event: &Event) {
        self.0.lock().unwrap().push(event.clone());
    }
}

/// The end-to-end path: a task created through the ops layer is fired as `task.created`, carrying the
/// created task's id — with nobody having emitted anything at the call site.
#[test]
fn a_committed_create_fires_task_created() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    let spy = Spy::default();
    let mut layer = EventLayer::new(head).with_sink(Box::new(spy.clone()));

    let task = store.add_task(new_task("タスク", project)).unwrap().id;
    let fired = layer.pump(store.read_model().conn(), 100).unwrap();

    assert_eq!(fired, 1, "one event for the one create");
    assert_eq!(spy.events(), vec![Event { name: name::TASK_CREATED, id: task }]);
}

/// A comment added to a task fires `comment.added` with the comment's id — the insert names it on its
/// own, no state-reading needed.
#[test]
fn a_committed_comment_fires_comment_added() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("タスク", project)).unwrap().id;

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    let spy = Spy::default();
    let mut layer = EventLayer::new(head).with_sink(Box::new(spy.clone()));

    let comment = store.add_task_comment(task, ActorKind::Ai, "コメント").unwrap().id;
    layer.pump(store.read_model().conn(), 100).unwrap();

    assert_eq!(spy.events(), vec![Event { name: name::COMMENT_ADDED, id: comment }]);
}

/// A status change is an `update`, which this skeleton does not yet name (it needs the new state) — so
/// the pump advances its cursor past it and fires nothing. The seam is here; the naming is layered on.
#[test]
fn an_update_is_seen_but_not_yet_named() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("タスク", project)).unwrap().id;

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    let spy = Spy::default();
    let mut layer = EventLayer::new(head).with_sink(Box::new(spy.clone()));

    store.set_task_status(task, amenbo_core::model::TaskStatus::InProgress).unwrap();
    let fired = layer.pump(store.read_model().conn(), 100).unwrap();

    assert_eq!(fired, 0, "an update names no v1 event here");
    assert!(spy.events().is_empty());
    assert!(layer.cursor() > head, "but the cursor moved past the change, so it will not be seen twice");
}

/// The pump does not re-fire a change it has already passed: a second pump with no new writes fires
/// nothing, and the cursor is stable.
#[test]
fn a_second_pump_with_no_writes_fires_nothing() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    let spy = Spy::default();
    let mut layer = EventLayer::new(head).with_sink(Box::new(spy.clone()));

    store.add_task(new_task("タスク", project)).unwrap();
    assert_eq!(layer.pump(store.read_model().conn(), 100).unwrap(), 1);
    let after = layer.cursor();
    assert_eq!(layer.pump(store.read_model().conn(), 100).unwrap(), 0, "nothing new to fire");
    assert_eq!(layer.cursor(), after, "and the cursor held");
    assert_eq!(spy.events().len(), 1);
}

/// A page smaller than the backlog still fires every event: the pump loops until the feed is caught up.
#[test]
fn a_small_page_still_drains_the_whole_backlog() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    let spy = Spy::default();
    let mut layer = EventLayer::new(head).with_sink(Box::new(spy.clone()));

    for i in 0..5 {
        store.add_task(new_task(&format!("タスク{i}"), project)).unwrap();
    }
    // A page of 1 forces the pump to loop across pages.
    let fired = layer.pump(store.read_model().conn(), 1).unwrap();
    assert_eq!(fired, 5, "every create fired, one page at a time");
    assert_eq!(spy.events().len(), 5);
}
