//! The outbox emit half of `AMB-D-367`: a write point appends the semantic event it alone can name —
//! which of the nine v1 events happened, the actor that drove it, the new state — to the plugin outbox,
//! **inside the mutation's own transaction**. These tests drive the events through the public `Store`
//! wrappers (the one seam CLI and GUI share), read them back through `outbox::events_since`, and pin
//! three things: the right event fires with the right `new_state`, the actor is stamped from the caller
//! (only the write point holds it), and a change that did not happen — an idempotent re-set, a
//! re-accept, a same-project reorder — emits nothing.

use amenbo_core::config::Paths;
use amenbo_core::model::{ActorKind, TaskStatus};
use amenbo_core::ops::Position;
use amenbo_core::store_engine::outbox::{events_since, outbox_head, OutboxRow, OutboxSlice};
use amenbo_core::Store;

fn temp_store() -> Store {
    let base = amenbo_scratch::scratch("outbox-emit");
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

fn new_decision(title: &str, project_id: i64) -> amenbo_core::ops::decision::NewDecision {
    amenbo_core::ops::decision::NewDecision {
        title: title.to_string(),
        body: String::new(),
        project_id,
    }
}

/// The head — the cursor a reader starts from, so a test only sees what its own step emitted.
fn head(store: &Store) -> i64 {
    outbox_head(store.read_model().conn()).unwrap()
}

/// The events the outbox gained after `after`.
fn since(store: &Store, after: i64) -> Vec<OutboxRow> {
    match events_since(store.read_model().conn(), after, 10_000).unwrap() {
        OutboxSlice::Events { rows, .. } => rows,
        OutboxSlice::Gap => panic!("nothing was trimmed, so there is no gap"),
    }
}

/// The single event the last step emitted (fails loudly if it emitted none or several).
fn only(store: &Store, after: i64) -> OutboxRow {
    let mut rows = since(store, after);
    assert_eq!(rows.len(), 1, "exactly one event: {rows:?}");
    rows.pop().unwrap()
}

/// Creating a task fires `task.created`: id the task's own, actor the creator's facet from `NewTask`,
/// and no `new` — the name is the whole state.
#[test]
fn creating_a_task_fires_created_with_the_creator_facet() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let h = head(&store);
    let task = store.add_task(new_task("タスク", project)).unwrap().id;
    let ev = only(&store, h);
    assert_eq!(ev.event, "task.created");
    assert_eq!(ev.record_id, task);
    assert_eq!(ev.actor, "ai", "the actor is the creator facet the task was made with");
    assert_eq!(ev.new_state, None, "the name is the whole state");
}

/// Deleting a task fires `task.deleted`: id the task's own, actor the caller's, no `new`. The cascade
/// children the delete takes with it are not v1 events, so exactly one event fires.
#[test]
fn deleting_a_task_fires_deleted() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("タスク", project)).unwrap().id;

    let h = head(&store);
    store.delete_task(task, ActorKind::Human).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "task.deleted");
    assert_eq!(ev.record_id, task);
    assert_eq!(ev.actor, "human", "the actor is who deleted it");
    assert_eq!(ev.new_state, None);
}

/// Deleting a project fires one `task.deleted` per task the cascade carried off — a task that vanished
/// inside a project delete is as observable as one deleted on its own (there is no row left to re-read,
/// so a silent cascade would lose it for good). Only the tasks are observed: the decisions and the
/// comments that go with them are not v1 events. Another project's task is untouched and unobserved.
#[test]
fn deleting_a_project_fires_deleted_for_every_task_it_carried_off() {
    let mut store = temp_store();
    let doomed = store.project_add(new_project("消えるPJ")).unwrap().id;
    let other = store.project_add(new_project("残るPJ")).unwrap().id;
    let t1 = store.add_task(new_task("属タスク1", doomed)).unwrap().id;
    let t2 = store.add_task(new_task("属タスク2", doomed)).unwrap().id;
    let survivor = store.add_task(new_task("別PJのタスク", other)).unwrap().id;
    // A comment and a decision ride along in the cascade; neither is a v1 event.
    store.add_task_comment(t1, ActorKind::Ai, "コメント").unwrap();
    store.add_decision(new_decision("案", doomed)).unwrap();

    let h = head(&store);
    store.project_delete(doomed, ActorKind::Human).unwrap();
    let rows = since(&store, h);
    assert!(
        rows.iter().all(|r| r.event == "task.deleted" && r.actor == "human" && r.new_state.is_none()),
        "every event the cascade emits is a `task.deleted` stamped with who deleted the project: {rows:?}",
    );
    let mut deleted: Vec<i64> = rows.iter().map(|r| r.record_id).collect();
    deleted.sort_unstable();
    let mut expected = vec![t1, t2];
    expected.sort_unstable();
    assert_eq!(deleted, expected, "one event per member task, and nothing else");
    assert!(!deleted.contains(&survivor), "another project's task is not observed");
}

/// Adding a task comment fires `comment.added`: `id` is the comment's own (not the task's), actor its
/// author, no `new`.
#[test]
fn adding_a_task_comment_fires_comment_added() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("タスク", project)).unwrap().id;

    let h = head(&store);
    let comment = store.add_task_comment(task, ActorKind::Ai, "コメント").unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "comment.added");
    assert_eq!(ev.record_id, comment.id, "the id is the comment's own, not the task's");
    assert_eq!(ev.actor, "ai");
    assert_eq!(ev.new_state, None);
}

/// A decision comment is not a v1 event: its write point emits nothing (only a *task* comment does).
#[test]
fn a_decision_comment_is_not_observed() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let decision = store.add_decision(new_decision("案", project)).unwrap().id;

    let h = head(&store);
    store.add_decision_comment(decision, ActorKind::Ai, "コメント").unwrap();
    assert!(since(&store, h).is_empty(), "a decision comment fires no v1 event");
}

/// A status change fires `task.status_changed` carrying the new status; the specialisation to `done`
/// fires `task.done` with no `new`. Both stamp the actor the caller passed, not a default.
#[test]
fn a_status_change_and_a_done_carry_the_actor_and_new_state() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("タスク", project)).unwrap().id;

    let h = head(&store);
    store.set_task_status(task, TaskStatus::InProgress, ActorKind::Ai).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "task.status_changed");
    assert_eq!(ev.record_id, task);
    assert_eq!(ev.actor, "ai");
    assert_eq!(ev.new_state.as_deref(), Some("in_progress"));

    // `→ done` is its own event, and its name is the whole state (no `new`). The actor is whoever the
    // caller declared — here the human.
    let h = head(&store);
    store.set_task_completed(task, true, ActorKind::Human).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "task.done");
    assert_eq!(ev.actor, "human");
    assert_eq!(ev.new_state, None);
}

/// The other terminal is its own event too (`AMB-D-397`). Left in the catch-all, "the task closed" would
/// be `task.done` plus a string match on `task.status_changed` — the asymmetry a plugin author would have
/// to work around, and one the decision side never had (it names both `accepted` and `rejected`).
#[test]
fn rejecting_a_task_fires_its_own_event_and_not_the_catch_all() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("やらないと決めたタスク", project)).unwrap().id;

    let h = head(&store);
    store.set_task_status(task, TaskStatus::Rejected, ActorKind::Ai).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "task.rejected", "the terminal has a name of its own");
    assert_eq!(ev.record_id, task);
    assert_eq!(ev.actor, "ai");
    assert_eq!(ev.new_state, None, "the name is the whole state — and the reason rides `comment.added`");
}

/// A transition that does not move the status observes nothing — an idempotent re-set is not a change.
#[test]
fn an_idempotent_status_set_emits_nothing() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("タスク", project)).unwrap().id;

    // A fresh task is already `todo`; setting it to `todo` again writes `updated_at` but changes no
    // status, so no event fires.
    let h = head(&store);
    store.set_task_status(task, TaskStatus::Todo, ActorKind::Ai).unwrap();
    assert!(since(&store, h).is_empty(), "a no-op status set emits no event");
}

/// Assigning fires `task.assigned` with the new assignee facet as `new`. Clearing the assignee is not a
/// v1 event, and re-assigning the same facet is not a change — both emit nothing.
#[test]
fn assigning_emits_the_facet_but_clearing_and_re_assigning_do_not() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("タスク", project)).unwrap().id;

    let h = head(&store);
    store.set_task_assignee(task, Some(ActorKind::Ai), ActorKind::Human).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "task.assigned");
    assert_eq!(ev.actor, "human", "the actor is who assigned");
    assert_eq!(ev.new_state.as_deref(), Some("ai"), "the new state is the assignee facet");

    // Re-assigning the same facet changes nothing.
    let h = head(&store);
    store.set_task_assignee(task, Some(ActorKind::Ai), ActorKind::Human).unwrap();
    assert!(since(&store, h).is_empty(), "re-assigning the same facet emits nothing");

    // Clearing the assignee has no v1 event.
    let h = head(&store);
    store.set_task_assignee(task, None, ActorKind::Human).unwrap();
    assert!(since(&store, h).is_empty(), "clearing the assignee emits nothing");
}

/// Moving a task to another project fires `task.moved` carrying the destination's slug; a reorder that
/// keeps the project is not a move.
#[test]
fn moving_between_projects_carries_the_slug_but_a_reorder_does_not() {
    let mut store = temp_store();
    let alpha = store.project_add(new_project("Alpha")).unwrap().id;
    let beta = store.project_add(new_project("Beta")).unwrap();
    let task = store.add_task(new_task("タスク", alpha)).unwrap().id;

    let h = head(&store);
    store.move_task(task, Some(beta.id), Position::Bottom, ActorKind::Ai).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "task.moved");
    assert_eq!(ev.actor, "ai");
    assert_eq!(ev.new_state.as_deref(), beta.slug.as_deref(), "the new state is the destination slug");
    assert!(ev.new_state.is_some(), "a real project always has a slug");

    // A reorder that keeps the project (target `None` = stay put) is not a move.
    let h = head(&store);
    store.move_task(task, None, Position::Top, ActorKind::Ai).unwrap();
    assert!(since(&store, h).is_empty(), "a same-project reorder emits no move event");
}

/// Accepting and rejecting fire their verdict events once, on the real transition only — the idempotent
/// re-accept reports no change and observes nothing.
#[test]
fn decision_verdicts_fire_once_on_the_real_transition() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let accepted = store.add_decision(new_decision("採択する案", project)).unwrap().id;
    let h = head(&store);
    store.accept_decision(accepted, Some("user".to_string()), ActorKind::Human).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "decision.accepted");
    assert_eq!(ev.record_id, accepted);
    assert_eq!(ev.actor, "human");
    assert_eq!(ev.new_state, None, "the name is the whole state");

    // Re-accepting an already-accepted decision reports changed=false, so nothing fires.
    let h = head(&store);
    store.accept_decision(accepted, Some("user".to_string()), ActorKind::Human).unwrap();
    assert!(since(&store, h).is_empty(), "a re-accept observes nothing");

    let rejected = store.add_decision(new_decision("却下する案", project)).unwrap().id;
    let h = head(&store);
    store.reject_decision(rejected, ActorKind::Ai).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "decision.rejected");
    assert_eq!(ev.actor, "ai");
    assert_eq!(ev.new_state, None);
}

/// Superseding with a still-`Proposed` decision promotes it to `Accepted` on the way, and that promotion
/// is a real verdict — so `decision.accepted` fires once, stamped with the caller's actor. Drawing the
/// edge again over the now-accepted side promotes nothing and observes nothing.
#[test]
fn a_supersede_that_promotes_the_new_side_fires_decision_accepted_once() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let old = store.add_decision(new_decision("旧案", project)).unwrap().id;
    store.accept_decision(old, Some("user".to_string()), ActorKind::Human).unwrap();
    let new = store.add_decision(new_decision("新案", project)).unwrap().id;

    // The new side is still Proposed; superseding promotes it to Accepted, which observes as an acceptance.
    let h = head(&store);
    store.supersede_decision(new, old, Some("user".to_string()), ActorKind::Ai).unwrap();
    let ev = only(&store, h);
    assert_eq!(ev.event, "decision.accepted");
    assert_eq!(ev.record_id, new, "the promotion is the new side's acceptance");
    assert_eq!(ev.actor, "ai");
    assert_eq!(ev.new_state, None, "the name is the whole state");

    // Re-superseding the same pair promotes nothing (the new side is already accepted), so nothing fires.
    let h = head(&store);
    store.supersede_decision(new, old, Some("user".to_string()), ActorKind::Ai).unwrap();
    assert!(since(&store, h).is_empty(), "a re-supersede promotes nothing and observes nothing");
}

/// Superseding with a side that is *already* `Accepted` draws the edge but promotes nothing, so no
/// acceptance is observed — the edge is not a verdict.
#[test]
fn a_supersede_over_an_already_accepted_side_emits_nothing() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let old = store.add_decision(new_decision("旧案", project)).unwrap().id;
    let new = store.add_decision(new_decision("新案", project)).unwrap().id;
    // Accept the new side first, so the supersession only draws the edge — no promotion.
    store.accept_decision(new, Some("user".to_string()), ActorKind::Human).unwrap();

    let h = head(&store);
    store.supersede_decision(new, old, Some("user".to_string()), ActorKind::Human).unwrap();
    assert!(since(&store, h).is_empty(), "drawing the edge over an accepted side promotes nothing");
}
