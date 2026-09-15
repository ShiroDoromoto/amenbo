//! **The v1 lifecycle-event catalog** — the names Amenbo fires when something happens (`AMB-D-367`).
//!
//! One vocabulary, three sides of it. An ops write point composes an event from what it already holds and
//! appends it to the [outbox](crate::store_engine::outbox) inside its own transaction; the
//! [drive](crate::outbox_drive) walks those rows and hands each on to whatever observes a write; a reader
//! — a [notification](crate::notify_dispatch), say — dispatches on the name. None of them owns the list, which is why it sits here rather than with any one of them: a name
//! added on one side and not the others is exactly the drift this placement rules out.
//!
//! The names themselves are strings, not an enum. They cross a process boundary as JSON and are stored in
//! the outbox as text, so the string *is* the value; [`V1_EVENTS`] is the closed set a stored one is
//! recognised against, and a row naming anything else is skipped rather than guessed at.

/// The v1 event names — the one source of truth for the strings a reader dispatches on, shared with the
/// points that emit them. Eleven are semantic: three the events an `update` alone cannot tell apart
/// (named alongside the new state that does), and eight that name themselves outright — a creation, a
/// deletion, a terminal, a comment posted or taken back. Two more are the **due warnings**, which are
/// semantic in the same way but are not writes at all: nobody acted, a day arrived, and the hourly tick
/// noticed ([`crate::due`]). The last ([`name::STORE_CHANGED`]) is of no family: it is the ledger saying
/// only that something moved. Together they are the v1 catalog ([`V1_EVENTS`]).
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
    /// **A task's due day has come or gone** — today's, or a day already past (`AMB-D-708`). No `new` and
    /// **no `actor`**: nobody acted, a day arrived, and the only thing that fired it is the hourly tick
    /// ([`crate::tick`]) noticing. It fires once per calendar day per task, for as long as the day stays
    /// past and the task stays open, so a task nobody closes is named again tomorrow.
    pub const TASK_DUE: &str = "task.due";
    /// **A task's due day is tomorrow** — the warning step before [`TASK_DUE`], and the same cut the screen
    /// draws in the colour before its own (`app/src/core/due.ts`). The two are separate names rather than
    /// one carrying which step, because a subscription is by name: someone who wants only the day itself
    /// says so by not subscribing to this. No `new`, and no `actor`, for the same reason as [`TASK_DUE`].
    pub const TASK_DUE_TOMORROW: &str = "task.due_tomorrow";
    /// **Something in this project changed** — the one signal that says only that, and is the odd one out
    /// of this catalog on purpose (`AMB-D-582`). The thirteen above all say what happened — eleven composed
    /// at an ops write point, which alone knows which of the six an `update` was and who drove it, and two
    /// composed by the tick for a day that came ([`crate::due`]). This one is composed at
    /// the **ledger seam** — the change feed's drain, inside the very transaction that wrote — so it
    /// reaches every write, including the ones no name above covers: a notes edit, a due date, a
    /// classification put on or taken off, an edge drawn, a decision settled, an attachment gone.
    ///
    /// What it carries is what that seam knows and no more. `id` is the project, `version` is the number
    /// that project is now at (the scalar the row carries), and there is **no `actor`**: the feed
    /// records which rows moved, never who moved them. A subscriber that wants to know *what* changed does not read it
    /// out of this — it re-reads its window, which is what it would do anyway (`AMB-D-582`).
    ///
    /// It is a signal, not a record: it may be missed, and nothing is built on its arrival. Whoever
    /// carries a copy out also checks the version at startup and on a timer, so a dropped signal costs a
    /// delay and never a divergence.
    pub const STORE_CHANGED: &str = "store.changed";
}

/// The complete v1 event catalog — every name in [`name`]. A stored event name is recognised against this
/// set, and a project's reporting is chosen out of it. Thirteen of them say what happened — eleven at a
/// write point, and two the hourly tick fires for a day that came; the last ([`name::STORE_CHANGED`]) is
/// the ledger's own signal and says nothing about what happened.
pub const V1_EVENTS: [&str; 14] = [
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
    name::TASK_DUE,
    name::TASK_DUE_TOMORROW,
    name::STORE_CHANGED,
];

/// Pin a stored event name to its catalog entry, or `None` for one this build does not know.
///
/// The `'static` name is what comes back rather than a `bool`, so a reader that recognises a row carries
/// the catalog's own string from there on: a payload built from it is byte-identical to one built by name,
/// and a comparison downstream is a pointer's worth of work rather than another string's.
pub fn recognised(event: &str) -> Option<&'static str> {
    V1_EVENTS.iter().copied().find(|name| *name == event)
}
