//! The v1 plugin payload: the JSON document a fired event hands to a plugin.
//!
//! Events are appended to the plugin observation outbox at the ops write points (see
//! [`outbox`](crate::store_engine::outbox) and `AMB-D-367`); this layer defines the *shape* of what a
//! plugin receives for each named event: the four fields every event carries, plus, where the event name
//! alone does not say it, the record's **new state**.
//!
//! ```json
//! { "v": 1, "event": "task.status_changed", "id": 42, "actor": "ai", "at": "2026-07-22T09:00:00Z", "new": "in_progress" }
//! { "v": 1, "event": "task.deleted", "id": 42, "actor": "ai", "at": "2026-07-22T09:01:00Z", "record": { "id": 42, "title": "…" } }
//! ```
//!
//! - **Common four** (`event`, `id`, `actor`, `at`) — present on every event. `event` is the namespace
//!   name a plugin dispatches on; `id` the affected record; `actor` who drove the write; `at` when.
//! - **New state** (`new`) — the record's state *after* the change, for the events an `update`
//!   disambiguates (a status change, an assignment, a move). Absent on events whose name already is the
//!   whole state (a creation, a deletion, a done, an accept/reject, a comment). No *before* value is
//!   carried for a record that still exists (`AMB-D-348`).
//! - **The vanished record** (`record`) — the deleted record's own shape, on the events where there is
//!   nothing left to read (`AMB-D-407`). A live record is read back by the plugin itself (`AMB-D-406`), so
//!   only the unreadable half travels on the wire. This is `AMB-D-348`'s foreseen additive extension: a
//!   *before* captured at the ops write point, for the events that need one.
//! - **The parent** (`parent`) — the task a comment hangs on, by id (`AMB-D-407`), on both of the comment
//!   events. A removed comment names it because the deletion took the relation with it; a posted one names
//!   it because no read answers for a comment by its own id, which is where the read-back of a live record
//!   (`AMB-D-406`) stops short.
//! - **Version** `v` — a single integer for the whole contract, `1` today. Adding a field does **not**
//!   bump it: a consumer ignores keys it does not know, so new fields are additive and silent. `v`
//!   rises only on a breaking change to an existing field's meaning (see `AMB-D-349`).
//!
//! This module is the single source of truth for the payload *type* and the [`eleven v1 event
//! names`](name). It does not read the store or launch anything: a caller on the write path (which alone
//! knows the actor) builds a [`Payload`] with the values it already holds and hands it to the hook runner.

use serde::Serialize;

use crate::model::{ActorKind, TaskStatus};
use crate::time::Timestamp;

/// The payload contract version. A single integer for the whole contract, bumped only on a breaking
/// change to an existing field — additive fields do not touch it (`AMB-D-349`).
pub const VERSION: u32 = 1;

/// The eleven v1 event names — the one source of truth for the strings a plugin dispatches on, shared
/// with the ops write points that emit them. Three are the events an `update` alone cannot tell apart
/// (named alongside the new state that does); the other eight name themselves outright — a creation, a
/// deletion, a terminal, a comment posted or taken back. Together they are the v1 catalog
/// ([`V1_EVENTS`]).
pub mod name {
    /// A task was created. No `new` — the name is the whole state. It fires when the **creation ends**
    /// (`task finish-creating`), not when `task add` returns (`AMB-D-557`): between the two nobody can
    /// reserve the task, so a subscriber hearing about it then has nothing it can act on.
    pub const TASK_CREATED: &str = "task.created";
    /// A task's status changed (to something other than a terminal; see [`TASK_DONE`] and
    /// [`TASK_REJECTED`]). Carries `new`.
    pub const TASK_STATUS_CHANGED: &str = "task.status_changed";
    /// A task was completed — the `status → done` specialization of a status change. No `new`.
    pub const TASK_DONE: &str = "task.done";
    /// A task was decided against — the `status → rejected` specialization, and the sibling of
    /// [`TASK_DONE`]: the two terminals differ only in whether the work was carried out (`AMB-D-397`).
    /// No `new`, and no reason either — that lands as a comment, so `comment.added` carries it.
    pub const TASK_REJECTED: &str = "task.rejected";
    /// A task was assigned (or reassigned). Carries `new`: the assignee facet.
    pub const TASK_ASSIGNED: &str = "task.assigned";
    /// A task moved to another project. Carries `new`: the destination project's slug.
    pub const TASK_MOVED: &str = "task.moved";
    /// A task was deleted. No `new` — the name is the whole state.
    pub const TASK_DELETED: &str = "task.deleted";
    /// A decision was accepted. No `new` — the name is the whole state.
    pub const DECISION_ACCEPTED: &str = "decision.accepted";
    /// A decision was rejected. No `new` — the name is the whole state.
    pub const DECISION_REJECTED: &str = "decision.rejected";
    /// A comment was added to a task. No `new` — the name is the whole state.
    pub const COMMENT_ADDED: &str = "comment.added";
    /// A comment was taken back — hard-deleted — from a task, and the pair of [`COMMENT_ADDED`]
    /// (`AMB-D-401`). A deletion is the one change a subscriber cannot catch up on by re-reading (there
    /// is nothing left to read), so without this event a mirror keeps a comment that is gone. No `new` —
    /// the name is the whole state.
    pub const COMMENT_REMOVED: &str = "comment.removed";
}

/// The complete v1 event catalog — all eleven names in [`name`]. A plugin's subscription is checked
/// against this set.
pub const V1_EVENTS: [&str; 11] = [
    name::TASK_CREATED,
    name::TASK_STATUS_CHANGED,
    name::TASK_DONE,
    name::TASK_REJECTED,
    name::TASK_ASSIGNED,
    name::TASK_MOVED,
    name::TASK_DELETED,
    name::DECISION_ACCEPTED,
    name::DECISION_REJECTED,
    name::COMMENT_ADDED,
    name::COMMENT_REMOVED,
];

/// One fired event, ready to serialize to the JSON a plugin receives.
///
/// Build one with the per-event constructor ([`task_status_changed`](Self::task_status_changed) and its
/// kin) — each fills `event` with the right name and `new` with the right state, so the eleven events are
/// constructed by name and cannot be mismatched. Field order is the wire order: `v` leads, as `AMB-D-349`
/// asks.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Payload {
    /// The contract version, always [`VERSION`]. First on the wire.
    pub v: u32,
    /// The event's namespace name — one of [`V1_EVENTS`].
    pub event: &'static str,
    /// The affected record's id — the conversational number a reader knows it by.
    pub id: i64,
    /// Who drove the write, human or the human's AI.
    pub actor: ActorKind,
    /// When the event fired, as `2026-07-22T09:00:00Z`.
    pub at: Timestamp,
    /// The record's new state, for the events an `update` disambiguates; absent on the rest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
    /// The **vanished record**, for the events whose record is gone (`AMB-D-407`): the row as it stood the
    /// instant before it went, one record's worth, its children not folded in — each child fires its own
    /// deletion. Absent on every other event, where the record is still there and a plugin reads it back
    /// by name (`AMB-D-406`), and absent on a deletion an older store appended without one.
    ///
    /// Carried whole rather than reduced to a display name: what a subscriber needs out of a deleted
    /// record is the subscriber's to decide, and the publisher choosing for it is the choice that cannot
    /// be undone later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<serde_json::Value>,
    /// The record the event's record **hangs on**, by id (`AMB-D-407`): the task a comment was posted to,
    /// on the comment events. Absent on everything else, and absent on a comment event an older store
    /// appended without one.
    ///
    /// `id` names the record the event is about and nothing else, so a subscriber that hears "comment 5"
    /// cannot say where it is. For a removal the relation cannot be looked up afterwards either, because
    /// looking it up is exactly what the deletion removed; for an addition it cannot be looked up at all,
    /// since a comment is read as part of a task's timeline and never by its own id. Named on its own rather
    /// than left inside `record` so routing does not depend on knowing amenbo's field names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<i64>,
}

impl Payload {
    /// The shared shell — the common four plus `v`, with no new state. The named constructors below add
    /// `new` where the event carries one.
    fn base(event: &'static str, id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self { v: VERSION, event, id, actor, at, new: None, record: None, parent: None }
    }

    /// `task.created` — a task was created, which on this wire means its creation ended and it can be
    /// picked up (`AMB-D-557`). `actor` is whoever ended it and `at` is when.
    pub fn task_created(id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self::base(name::TASK_CREATED, id, actor, at)
    }

    /// `task.status_changed` — a task's status moved to `new` (a `done` transition is
    /// [`task_done`](Self::task_done) instead).
    pub fn task_status_changed(id: i64, actor: ActorKind, at: Timestamp, new: TaskStatus) -> Self {
        Self { new: Some(new.as_str().to_string()), ..Self::base(name::TASK_STATUS_CHANGED, id, actor, at) }
    }

    /// `task.done` — a task was completed.
    pub fn task_done(id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self::base(name::TASK_DONE, id, actor, at)
    }

    /// `task.rejected` — a task was decided against.
    pub fn task_rejected(id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self::base(name::TASK_REJECTED, id, actor, at)
    }

    /// `task.assigned` — a task was assigned to `new` (the assignee facet).
    pub fn task_assigned(id: i64, actor: ActorKind, at: Timestamp, new: ActorKind) -> Self {
        Self { new: Some(new.as_str().to_string()), ..Self::base(name::TASK_ASSIGNED, id, actor, at) }
    }

    /// `task.moved` — a task moved to the project whose slug is `new`.
    pub fn task_moved(id: i64, actor: ActorKind, at: Timestamp, new: impl Into<String>) -> Self {
        Self { new: Some(new.into()), ..Self::base(name::TASK_MOVED, id, actor, at) }
    }

    /// `task.deleted` — a task was deleted. The vanished task rides on `record` when the emit point
    /// captured one (`AMB-D-407`); this constructor builds the event without it, which is also the shape a
    /// row appended before the capture existed rebuilds to.
    pub fn task_deleted(id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self::base(name::TASK_DELETED, id, actor, at)
    }

    /// `decision.accepted` — a decision was accepted.
    pub fn decision_accepted(id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self::base(name::DECISION_ACCEPTED, id, actor, at)
    }

    /// `decision.rejected` — a decision was rejected.
    pub fn decision_rejected(id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self::base(name::DECISION_REJECTED, id, actor, at)
    }

    /// `comment.added` — a comment was added; `id` is the comment's id. The task it was posted to rides on
    /// `parent` when the emit point read one (`AMB-D-407`); this constructor builds the event without it,
    /// which is also the shape a row appended before the capture existed rebuilds to.
    pub fn comment_added(id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self::base(name::COMMENT_ADDED, id, actor, at)
    }

    /// `comment.removed` — a comment was taken back; `id` is the comment's id, the same axis
    /// [`comment_added`](Self::comment_added) reports on, so a subscriber can pair the two. The task it
    /// hung on rides on `parent` and the comment itself on `record` when the emit point captured them
    /// (`AMB-D-407`); this constructor builds the event without either.
    pub fn comment_removed(id: i64, actor: ActorKind, at: Timestamp) -> Self {
        Self::base(name::COMMENT_REMOVED, id, actor, at)
    }

    /// Rebuild the payload a fired event carries from its stored outbox row (`AMB-D-367`). The store
    /// classifies nothing — an [`OutboxRow`](crate::store_engine::OutboxRow) holds the wire fields as
    /// opaque strings — so this is the mapping half that turns one back into the typed payload the
    /// dispatcher serializes and hands a plugin (`AMB-D-348`: the thin mapping layer sits with the payload
    /// type). The `event` string is pinned to its `'static` catalog name so the wire form is byte-identical
    /// to a payload built by the named constructors, and `new` is carried through verbatim — the row
    /// already holds the new state the emit side composed.
    ///
    /// Returns `None` for a row this amenbo does not recognise — an `event` outside [`V1_EVENTS`], or an
    /// `actor` / `at` that does not parse — so the dispatcher can warn and skip rather than fire a payload
    /// it cannot faithfully build.
    pub fn from_outbox_row(row: &crate::store_engine::OutboxRow) -> Option<Self> {
        Self::from_wire(
            &row.event,
            row.record_id,
            &row.actor,
            &row.at,
            row.new_state.as_deref(),
            row.record.as_deref(),
            row.parent,
        )
    }

    /// The same rebuild, from a row on a plugin's queue (`AMB-D-399`). A queued row is an outbox row the
    /// fan-out addressed to one plugin, so the wire fields — and what makes them unrecognisable — are the
    /// same; only the reader differs (the runner of one queue, rather than the drain of the outbox).
    pub fn from_queue_row(row: &crate::store_engine::QueueRow) -> Option<Self> {
        Self::from_wire(
            &row.event,
            row.record_id,
            &row.actor,
            &row.at,
            row.new_state.as_deref(),
            row.record.as_deref(),
            row.parent,
        )
    }

    /// The mapping both stored forms share: opaque strings in, the typed payload out, `None` for anything
    /// this build cannot faithfully rebuild.
    fn from_wire(
        event: &str,
        record_id: i64,
        actor: &str,
        at: &str,
        new: Option<&str>,
        record: Option<&str>,
        parent: Option<i64>,
    ) -> Option<Self> {
        let event = V1_EVENTS.iter().copied().find(|name| *name == event)?;
        let actor = ActorKind::parse(actor)?;
        let at = Timestamp::parse_rfc3339(at)?;
        Some(Self {
            v: VERSION,
            event,
            id: record_id,
            actor,
            at,
            new: new.map(str::to_string),
            // A shape that will not parse is dropped rather than passed on as text: the field is a JSON
            // object on the wire, and half an answer in the shape of one is worse than the absence a
            // subscriber already has to handle (an older store's deletion carries none).
            record: record.and_then(|raw| serde_json::from_str(raw).ok()),
            parent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> Timestamp {
        Timestamp::parse_rfc3339("2026-07-22T09:00:00Z").unwrap()
    }

    #[test]
    fn the_catalog_holds_eleven_distinct_names() {
        let mut seen = V1_EVENTS.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 11, "the v1 catalog is eleven distinct events");
    }

    #[test]
    fn every_constructor_names_a_catalog_event() {
        let all = [
            Payload::task_created(1, ActorKind::Human, at()),
            Payload::task_status_changed(1, ActorKind::Human, at(), TaskStatus::InProgress),
            Payload::task_done(1, ActorKind::Human, at()),
            Payload::task_rejected(1, ActorKind::Human, at()),
            Payload::task_assigned(1, ActorKind::Human, at(), ActorKind::Ai),
            Payload::task_moved(1, ActorKind::Human, at(), "amenbo"),
            Payload::task_deleted(1, ActorKind::Human, at()),
            Payload::decision_accepted(1, ActorKind::Human, at()),
            Payload::decision_rejected(1, ActorKind::Human, at()),
            Payload::comment_added(1, ActorKind::Human, at()),
            Payload::comment_removed(1, ActorKind::Human, at()),
        ];
        for p in &all {
            assert!(V1_EVENTS.contains(&p.event), "{} is in the catalog", p.event);
            assert_eq!(p.v, 1, "every payload is v1");
        }
    }

    #[test]
    fn version_leads_the_wire_and_the_common_four_are_present() {
        let json = serde_json::to_string(&Payload::comment_added(42, ActorKind::Ai, at())).unwrap();
        assert!(json.starts_with(r#"{"v":1,"#), "v is first on the wire: {json}");
        assert_eq!(
            json,
            r#"{"v":1,"event":"comment.added","id":42,"actor":"ai","at":"2026-07-22T09:00:00Z"}"#
        );
    }

    #[test]
    fn a_status_change_carries_its_new_state() {
        let json = serde_json::to_value(Payload::task_status_changed(
            7,
            ActorKind::Ai,
            at(),
            TaskStatus::InProgress,
        ))
        .unwrap();
        assert_eq!(json["event"], "task.status_changed");
        assert_eq!(json["new"], "in_progress");
        assert_eq!(json["actor"], "ai");
    }

    #[test]
    fn an_assignment_carries_the_new_assignee_facet() {
        let json = serde_json::to_value(Payload::task_assigned(7, ActorKind::Human, at(), ActorKind::Ai)).unwrap();
        assert_eq!(json["event"], "task.assigned");
        assert_eq!(json["new"], "ai");
    }

    #[test]
    fn a_move_carries_the_destination_slug() {
        let json = serde_json::to_value(Payload::task_moved(7, ActorKind::Human, at(), "amenbo")).unwrap();
        assert_eq!(json["event"], "task.moved");
        assert_eq!(json["new"], "amenbo");
    }

    #[test]
    fn from_outbox_row_rebuilds_the_wire_payload() {
        use crate::store_engine::OutboxRow;
        // A status change round-trips through the outbox row into the same wire bytes a named constructor
        // would produce.
        let row = OutboxRow {
            id: 9,
            event: "task.status_changed".to_string(),
            record_id: 42,
            actor: "ai".to_string(),
            at: "2026-07-22T09:00:00Z".to_string(),
            new_state: Some("in_progress".to_string()),
            project: Some(1),
            record: None,
            parent: None,
        };
        let rebuilt = Payload::from_outbox_row(&row).unwrap();
        assert_eq!(
            serde_json::to_string(&rebuilt).unwrap(),
            serde_json::to_string(&Payload::task_status_changed(
                42,
                ActorKind::Ai,
                at(),
                TaskStatus::InProgress,
            ))
            .unwrap(),
            "the row rebuilds byte-for-byte what the named constructor emits",
        );
    }

    /// A deletion's row rebuilds with the vanished record parsed back into the payload's own JSON
    /// (`AMB-D-407`) — the wire carries an object, so the column's text becomes one.
    #[test]
    fn a_deletions_row_rebuilds_with_the_record_that_is_gone() {
        use crate::store_engine::OutboxRow;
        let row = OutboxRow {
            id: 9,
            event: "task.deleted".to_string(),
            record_id: 42,
            actor: "ai".to_string(),
            at: "2026-07-22T09:00:00Z".to_string(),
            new_state: None,
            project: Some(1),
            record: Some(r#"{"id":42,"title":"消えたタスク"}"#.to_string()),
            parent: None,
        };
        let rebuilt = Payload::from_outbox_row(&row).unwrap();
        assert_eq!(rebuilt.record.as_ref().unwrap()["title"], "消えたタスク");
        assert_eq!(rebuilt.parent, None, "a task hangs on no record this payload names");
        assert!(
            serde_json::to_string(&rebuilt).unwrap().contains(r#""record":{"#),
            "and it goes out as an object, not as a string holding JSON",
        );

        // A shape that will not parse is dropped, not passed on as text: the absence is a case every
        // subscriber already handles, and half an answer in the shape of an object is not.
        let broken = OutboxRow { record: Some("{not json".to_string()), ..row };
        assert_eq!(Payload::from_outbox_row(&broken).unwrap().record, None);
    }

    /// A child's deletion rebuilds naming what it hung on (`AMB-D-407`), beside the shape that went with
    /// it — the two halves of what the removal took away.
    #[test]
    fn a_childs_deletion_rebuilds_naming_what_it_hung_on() {
        use crate::store_engine::OutboxRow;
        let row = OutboxRow {
            id: 9,
            event: "comment.removed".to_string(),
            record_id: 5,
            actor: "human".to_string(),
            at: "2026-07-22T09:00:00Z".to_string(),
            new_state: None,
            project: Some(1),
            record: Some(r#"{"id":5,"task_id":42,"text":"誤投稿"}"#.to_string()),
            parent: Some(42),
        };
        let rebuilt = Payload::from_outbox_row(&row).unwrap();
        assert_eq!(rebuilt.parent, Some(42));
        let wire = serde_json::to_string(&rebuilt).unwrap();
        assert!(wire.contains(r#""parent":42"#), "it goes out as its own field: {wire}");
    }

    /// A posted comment rebuilds naming the task it is on (`AMB-T-2467`) — the same field a removal uses,
    /// for the reason that outlives the removal: nothing reads a comment by its own id, so the id alone
    /// leaves a subscriber unable to say what the comment is about.
    #[test]
    fn a_posted_comment_rebuilds_naming_the_task_it_is_on() {
        use crate::store_engine::OutboxRow;
        let row = OutboxRow {
            id: 9,
            event: "comment.added".to_string(),
            record_id: 5,
            actor: "ai".to_string(),
            at: "2026-07-22T09:00:00Z".to_string(),
            new_state: None,
            project: Some(1),
            record: None,
            parent: Some(42),
        };
        let rebuilt = Payload::from_outbox_row(&row).unwrap();
        assert_eq!(rebuilt.parent, Some(42));
        let wire = serde_json::to_string(&rebuilt).unwrap();
        assert_eq!(
            wire,
            r#"{"v":1,"event":"comment.added","id":5,"actor":"ai","at":"2026-07-22T09:00:00Z","parent":42}"#
        );
    }

    #[test]
    fn from_outbox_row_rejects_a_row_it_cannot_faithfully_build() {
        use crate::store_engine::OutboxRow;
        let ok = OutboxRow {
            id: 1,
            event: "task.created".to_string(),
            record_id: 7,
            actor: "human".to_string(),
            at: "2026-07-22T09:00:00Z".to_string(),
            new_state: None,
            project: None,
            record: None,
            parent: None,
        };
        assert!(Payload::from_outbox_row(&ok).is_some());
        // An event outside the v1 catalog, an unparseable actor, and a bad timestamp each yield None.
        assert!(Payload::from_outbox_row(&OutboxRow { event: "task.exploded".to_string(), ..ok.clone() }).is_none());
        assert!(Payload::from_outbox_row(&OutboxRow { actor: "robot".to_string(), ..ok.clone() }).is_none());
        assert!(Payload::from_outbox_row(&OutboxRow { at: "not-a-time".to_string(), ..ok.clone() }).is_none());
    }

    #[test]
    fn events_whose_name_is_the_whole_state_omit_new() {
        for p in [
            Payload::task_created(1, ActorKind::Human, at()),
            Payload::task_done(1, ActorKind::Human, at()),
            Payload::task_deleted(1, ActorKind::Human, at()),
            Payload::decision_accepted(1, ActorKind::Human, at()),
            Payload::decision_rejected(1, ActorKind::Human, at()),
            Payload::comment_added(1, ActorKind::Human, at()),
            Payload::comment_removed(1, ActorKind::Human, at()),
        ] {
            let json = serde_json::to_value(&p).unwrap();
            assert!(json.get("new").is_none(), "{} carries no new state", p.event);
        }
    }
}
