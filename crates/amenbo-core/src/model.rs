//! The domain model — the logical schema.
//!
//! The structs in this module are, as written, **the format of the local store**. Every record carries
//! its audit metadata (id / created_at / updated_at). Deletion is physical.
//!

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

/// A project's default view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    List,
    Board,
    Calendar,
    Timeline,
}

impl View {
    pub fn as_str(&self) -> &'static str {
        match self {
            View::List => "list",
            View::Board => "board",
            View::Calendar => "calendar",
            View::Timeline => "timeline",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        match s {
            "list" => Some(View::List),
            "board" => Some(View::Board),
            "calendar" => Some(View::Calendar),
            "timeline" => Some(View::Timeline),
            _ => None,
        }
    }
}

/// A task's priority — a fixed enum, not a user-defined scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }

    pub fn parse(s: &str) -> Option<Priority> {
        match s {
            "high" => Some(Priority::High),
            "medium" => Some(Priority::Medium),
            "low" => Some(Priority::Low),
            _ => None,
        }
    }

    /// Sort weight — `high` comes first.
    pub fn rank(&self) -> u8 {
        match self {
            Priority::High => 0,
            Priority::Medium => 1,
            Priority::Low => 2,
        }
    }
}

/// A task's subtype. Classification belongs to a separate entity (the dimension), so all that is left
/// here is `default` / `milestone`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subtype {
    #[default]
    Default,
    Milestone,
}

impl Subtype {
    pub fn as_str(&self) -> &'static str {
        match self {
            Subtype::Default => "default",
            Subtype::Milestone => "milestone",
        }
    }

    pub fn parse(s: &str) -> Option<Subtype> {
        match s {
            "default" => Some(Subtype::Default),
            "milestone" => Some(Subtype::Milestone),
            _ => None,
        }
    }
}

/// A task's status. It is **the authority on completion**: being done is derived from `Done`
/// ([`Task::completed`]), never stored alongside it.
///
/// `Done` and `Rejected` are the two terminals, and the difference between them is whether the work was
/// *carried out* (`AMB-D-397`): a task nobody is going to do ends at `Rejected`, where recording it as
/// `Done` would make the history claim something that never happened and deleting it would take the
/// reasoning away with the row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Todo,
    InProgress,
    Done,
    Blocked,
    /// Considered, and decided against. A terminal that is not an achievement — so `completed_at`, the
    /// day the work was *finished*, stays unset, and when the rejection happened is what
    /// [`Task::status_changed_at`] holds.
    Rejected,
}

impl TaskStatus {
    /// Every status there is, in the order a board reads them. The one place they are enumerated, so a
    /// surface that lists them (the filter grammar an agent reads before it queries) cannot fall behind
    /// the enum.
    pub const ALL: [TaskStatus; 5] = [
        TaskStatus::Todo,
        TaskStatus::InProgress,
        TaskStatus::Done,
        TaskStatus::Blocked,
        TaskStatus::Rejected,
    ];

    /// The **terminals**: the statuses that mean the work is not coming back. Written once here so the
    /// two readings of an ended task cannot drift apart — *closed* is this set, *carried out* is `Done`
    /// alone ([`Task::completed`]).
    pub const CLOSED: [TaskStatus; 2] = [TaskStatus::Done, TaskStatus::Rejected];

    /// Is this task over, whichever way it ended? What a dependency asks before it releases what it was
    /// holding back, and what an "outstanding work" count asks before it counts a task. Not the same
    /// question as [`Task::completed`], which asks whether the work was *carried out*.
    pub fn is_closed(&self) -> bool {
        Self::CLOSED.contains(self)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Todo => "todo",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Done => "done",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<TaskStatus> {
        match s {
            "todo" => Some(TaskStatus::Todo),
            "in_progress" => Some(TaskStatus::InProgress),
            "done" => Some(TaskStatus::Done),
            "blocked" => Some(TaskStatus::Blocked),
            "rejected" => Some(TaskStatus::Rejected),
            _ => None,
        }
    }
}

/// The facet an action or an attribution belongs to: the human, or the human's AI.
/// It is **a label that assumes an honest actor** — a guardrail against accidents, not a security
/// boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    #[default]
    Human,
    Ai,
}

impl ActorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorKind::Human => "human",
            ActorKind::Ai => "ai",
        }
    }

    pub fn parse(s: &str) -> Option<ActorKind> {
        match s {
            "human" => Some(ActorKind::Human),
            "ai" => Some(ActorKind::Ai),
            _ => None,
        }
    }
}

/// The lifecycle state of a decision record: `Proposed` (under discussion) → `Accepted` / `Rejected`.
/// Decisions have no todo/in_progress workflow the way tasks do, and they never show up in a mailbox.
/// **"Superseded" is not a state.** It is a *relationship between decisions* — the `supersedes` edge —
/// and currency is a projection derived from it (`current` = not pointed at by a live `supersedes` edge).
/// Stored as a flag, deleting the superseding decision would strand its target at `superseded` forever: a
/// decision nothing overturns, drifting on as history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    /// Under discussion. Not settled yet, and still editable.
    #[default]
    Proposed,
    /// Accepted and settled. `decided_at` / `decided_by` are set.
    Accepted,
    /// Rejected — considered, and not adopted.
    Rejected,
}

impl DecisionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionStatus::Proposed => "proposed",
            DecisionStatus::Accepted => "accepted",
            DecisionStatus::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<DecisionStatus> {
        match s {
            "proposed" => Some(DecisionStatus::Proposed),
            "accepted" => Some(DecisionStatus::Accepted),
            "rejected" => Some(DecisionStatus::Rejected),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Project {
    /// Primary key (INTEGER). It stays an integer across every boundary — Rust, TS, `--json`.
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub color: Option<String>,
    pub default_view: View,
    pub archived: bool,
    pub order_key: String,
    /// A short human-readable identifier, unique on this device (e.g. "amenbo"). Derived from the name at
    /// creation and immutable thereafter. The primary key (`id`) is the real identifier; the slug is
    /// **corroborating evidence** — an `.amenbo` pointer carries both, and if the slug disagrees with the
    /// one on the project `id` names, we warn that the pointer belongs to a different store.
    #[serde(default)]
    pub slug: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Task {
    /// The primary key, and also the conversational number (displayed as `AMB-T-<n>`). They are one and the
    /// same number — there is no separate `number` field.
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub subtype: Subtype,
    /// When it was completed. Set only while `status == Done`; `None` otherwise. *Whether* a task is done
    /// is derived from `status` ([`Task::completed`]), so this field says only *when*.
    pub completed_at: Option<Timestamp>,
    /// The authority on status, and the single truth about completion. There is no independent `completed`
    /// boolean: being done is derived from `status == Done` ([`Task::completed`]).
    #[serde(default)]
    pub status: TaskStatus,
    /// When the current `status` began — updated **only** on a status change, never on an ordinary field
    /// write (`updated_at` moves on any write, so it cannot answer "when did this status begin"). Because
    /// `in_progress` is exclusive (reserving is a CAS from `todo`), while a task is reserved this holds the
    /// reservation instant, and a reopen re-stamps it to the latest reservation. It is the basis for
    /// judging whether a premise changed *after* a task was reserved (`AMB-D-366`). `None` for a task that
    /// predates the column (an older store never stamped it).
    #[serde(default)]
    pub status_changed_at: Option<Timestamp>,
    /// Is the task still being put together? Creation is two stages, and this is the one it is at
    /// (`AMB-D-552`): `true` while it is being assembled, `false` once the creation was finished. It is a
    /// premise of `ready` and not a sixth status (`AMB-D-553`) — a draft is on the board and in every
    /// listing, it just cannot be reserved (`AMB-D-555`). `false` is what a task from a store that
    /// predates the column means, and it is the truth about it: the build that wrote it had no second
    /// stage to leave it in.
    #[serde(default)]
    pub draft: bool,
    /// The creator's facet. `None` means unknown (older data), which reads as "not authored by the AI".
    #[serde(default)]
    pub created_by_kind: Option<ActorKind>,
    /// The assignee's facet. `ai` means "for this person's AI" (me-ai). In a single local store, an
    /// assignee is one of exactly two facets, human or ai; `None` means unassigned.
    #[serde(default)]
    pub assignee_kind: Option<ActorKind>,
    pub start_on: Option<NaiveDate>,
    pub due_on: Option<NaiveDate>,
    pub priority: Option<Priority>,
    /// The project it belongs to. A task lives in exactly one project — it never multi-homes — so
    /// placement is held on the task itself. `None` means unfiled (the inbox).
    #[serde(default)]
    pub project_id: Option<i64>,
    /// Where it sits within its project. `None` when unfiled — the inbox has no ordering.
    #[serde(default)]
    pub order_key: Option<String>,
    /// Which of the project's bound folders this task is worked in (`AMB-D-648`), as the id of the
    /// binding row — never a path, so the folder can be moved or renamed under it. `None` is a task that
    /// names no folder, which is every task unless someone said otherwise: nothing infers a place from
    /// where the task was filed. Having one refuses nothing on its own — no reservation and no worktree
    /// is stopped for it here; it is expressiveness the surfaces around Amenbo may act on.
    #[serde(default)]
    pub at_binding_id: Option<i64>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Task {
    /// Is this task done? `status` is the sole source of truth for completion — there is no independent
    /// `completed` boolean, only this derivation of `done ⟺ completed`.
    pub fn completed(&self) -> bool {
        self.status == TaskStatus::Done
    }
}

/// A dependency between two tasks, as an edge object — one edge, one record. `task_id` depends on
/// `blocked_by_id`, i.e. the latter should be done first. The reverse direction (`blocks`) is not stored;
/// it is derived at query time. Removing a dependency deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskDependency {
    pub id: i64,
    /// The blocked side — this depends on …
    pub task_id: i64,
    /// The blocker — … must be done first.
    pub blocked_by_id: i64,
    /// The creator's facet.
    #[serde(default)]
    pub created_by_kind: Option<ActorKind>,
    /// When this edge was established — what the premise-change judgement dates it by (`AMB-D-372`).
    /// Stamped once when the edge is drawn and never rewritten; `created_at` records the same instant but
    /// is a record column, so it takes no part in the judgement. `None` only on a row that predates the
    /// column and was never backfilled — read as "not after the status clock", i.e. no premise change.
    #[serde(default)]
    pub established_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A git commit SHA recorded against a task — one row, one commit (a task carries many). Amenbo keeps the
/// SHA as an opaque string: it never reads git, verifies the commit exists, or knows which forge it lives
/// on. It is the anchor from history back to a task: a public commit carries no store-local reference, so
/// the chain can only be drawn on the task side. `sha` is the full-length lower-case hex the ops layer
/// admits at the door (40 hex = SHA-1, 64 = SHA-256); short forms and refs are refused before they land.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskCommit {
    pub id: i64,
    /// The task this commit belongs to.
    pub task_id: i64,
    /// The full commit SHA, lower-case hex.
    pub sha: String,
    /// The creator's facet. `None` reads as human (older data).
    #[serde(default)]
    pub created_by_kind: Option<ActorKind>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A **plugin's text (non-secret) config value at one layer** (`AMB-D-434` / `AMB-D-601` / `AMB-D-356`).
/// One row per `(layer, plugin, field)`, and that row is the whole answer: the author's `scope` declaration
/// picks the single layer the plugin lives at, so there is no tier under this for the value to fall back to.
/// A `secret` field is never one of these — it is a
/// [`PluginSecret`], the table an `export` must leave behind. `plugin` is the plugin's manifest name (plugins
/// live on disk, not in the store, so there is no id for it) and `field_key` the config field's key.
/// Unlike `hook_optout` this is a real record, carried by `export`/`backup`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PluginConfigValue {
    pub id: i64,
    /// The project this value belongs to, or `None` for the device row a `scope: machine` plugin holds
    /// (`AMB-D-601`).
    pub project_id: Option<i64>,
    /// The plugin's manifest name.
    pub plugin: String,
    /// The config field's key (spelled out because `key` is a SQLite keyword).
    pub field_key: String,
    /// The value.
    pub value: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A **plugin's secret config value at one layer** (`AMB-D-434` / `AMB-D-601`): the same shape and address as
/// [`PluginConfigValue`], in a table of its own because its rows may travel to fewer places. A backup
/// carries it — the road back to one's own machine, where dropping the secrets would mean typing every
/// credential in again — and an export must not, that being the one-way door out to another tool. The
/// separation is a whole table rather than a flag on a row so that the exclusion holds for a path nobody
/// remembered to teach.
///
/// The value is never handed back to a face; it is read at the moment a plugin runs and injected as an
/// environment variable ([`crate::plugin_inject`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PluginSecret {
    pub id: i64,
    /// The project this secret belongs to, or `None` for the device row (`AMB-D-601`).
    pub project_id: Option<i64>,
    /// The plugin's manifest name.
    pub plugin: String,
    /// The config field's key (spelled out because `key` is a SQLite keyword).
    pub field_key: String,
    /// The secret value, in plaintext — at-rest secrecy is the OS's full-disk encryption, the same
    /// delegation the truth source itself makes.
    pub value: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A **plugin's enable gate at one layer** (`AMB-D-434` / `AMB-D-601`). One row per `(layer, plugin)`: this
/// project — or this device — has the plugin **on**. The row is the whole answer: there is no other tier to
/// inherit from or veto, so absence is simply off, and turning it off deletes the row rather than storing a
/// `false`.
///
/// The row is also the whole of it: turning a plugin on is itself the permission to run its code
/// (`AMB-D-434`), so nothing sits beside this to be carried separately. Like [`PluginConfigValue`] it
/// is a real record, carried by `export`/`backup` — a restore that dropped it would silently switch a
/// project's plugins off, and one that keeps it brings the plugin back on where it was.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PluginEnabledProject {
    pub id: i64,
    /// The project the plugin is enabled in, or `None` for the device gate a `scope: machine` plugin opens
    /// once for the whole machine (`AMB-D-601`).
    pub project_id: Option<i64>,
    /// The plugin's manifest name.
    pub plugin: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A decision record — a decision, and *why* we made it — as a first-class entity that sits beside Task,
/// under Project. **Append-only**: you do not edit a decision, you write a new one that `supersedes` it.
/// Decisions have no status workflow and take no part in the mailbox, so they never clutter a task list.
/// Their numbers are a global sequence in a space of their own, separate from tasks.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Decision {
    /// The primary key, and also the decision number (displayed as `AMB-D-<n>` — a number space separate from
    /// tasks').
    pub id: i64,
    /// The project it lives under. Decisions do not multi-home: one decision, one project.
    pub project_id: i64,
    pub title: String,
    /// The decision itself: the conclusion and the grounds for it. Distil rather than transcribe — the raw
    /// discussion does not belong here, and watch for PII on the way in.
    pub body: String,
    /// Lifecycle state.
    #[serde(default)]
    pub status: DecisionStatus,
    /// When the current `status` began — updated **only** on a status transition (propose / accept / reject
    /// / reopen, and the promotion `supersede` performs), never on an ordinary edit: a body rewritten in
    /// place moves `updated_at` and leaves this still answering "when did this decision last change what it
    /// is". The symmetric twin of [`Task::status_changed_at`], and the far side of the comparison that says
    /// a premise re-opened *after* a task was reserved (`AMB-D-373`).
    ///
    /// Not `decided_at`: that one is the moment of *settling* and is cleared by a reopen, so it cannot
    /// answer for a decision that is back under discussion — which is the one state the reopen axis judges.
    /// `None` only for a decision no migration reached (every existing row was backfilled).
    #[serde(default)]
    pub status_changed_at: Option<Timestamp>,
    /// When it was accepted (set on `Accepted`).
    #[serde(default)]
    pub decided_at: Option<Timestamp>,
    /// The decider token, for display.
    #[serde(default)]
    pub decided_by: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The kind of an edge between decisions. The variants are cut by **behaviour** — what this edge tells
/// you to do with the older decision from now on — not by taxonomy. All three sit on one axis, "how to
/// read it": don't read it any more (`Supersedes`), read it together with this one (`Amends`), read it
/// first (`BuildsOn`). A generic `related` edge would change nothing about how the target is read, so it
/// has no place on that axis — which is why there isn't one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionEdgeKind {
    /// Consigns the target to history: it stops being current, and is greyed out.
    #[default]
    Supersedes,
    /// Partially revises the target. The target **stays current**, and the two are read together.
    Amends,
    /// **Builds on** the target. It changes neither the target's currency nor how it is read; it supplies
    /// only the **reading order** (read that one first) and the **blast radius of overturning it** (the
    /// reverse lookup: the decisions that need revisiting if this one falls). `Supersedes` and `Amends`
    /// both imply it, so it is never laid on top of either for the same pair.
    BuildsOn,
}

impl DecisionEdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionEdgeKind::Supersedes => "supersedes",
            DecisionEdgeKind::Amends => "amends",
            DecisionEdgeKind::BuildsOn => "builds_on",
        }
    }

    pub fn parse(s: &str) -> Option<DecisionEdgeKind> {
        match s {
            "supersedes" => Some(DecisionEdgeKind::Supersedes),
            "amends" => Some(DecisionEdgeKind::Amends),
            "builds_on" => Some(DecisionEdgeKind::BuildsOn),
            _ => None,
        }
    }
}

/// A decision → decision edge. One edge, one record (the same shape as `TaskDependency`), so a single
/// decision can point at **several** older ones, per kind — the edges form a DAG. The direction is always
/// new → old: `decision_id` is the side that drew the edge, `target_decision_id` the older side it points
/// at, and the older row is never rewritten (the reverse lookup, "who overturned me?", is derived from the
/// index on `target_decision_id`). A pair can carry only one kind (`decision_edge_pair` UNIQUE):
/// supersedes and amends contradict each
/// other, and builds_on is implied by both, so stacking it adds nothing. Removing an edge deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionEdge {
    pub id: i64,
    /// The side that drew the edge — the newer decision.
    pub decision_id: i64,
    /// The side pointed at — the older decision.
    pub target_decision_id: i64,
    pub kind: DecisionEdgeKind,
    /// When the edge came to carry its current `kind` — the third premise-change intent column, beside
    /// [`TaskDependency::established_at`] and [`DecisionTaskLink::linked_at`] (`AMB-D-372`). What dates a
    /// supersession, the target's row being left alone by it (`AMB-D-373`).
    #[serde(default)]
    pub drawn_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A bidirectional link between a decision and a task. One link, one record (the same shape as
/// `TaskDependency`). It is many-to-many and cheap to traverse either way — the implementation tasks a
/// decision spawned, and the decision that motivated a task — because the SQLite truth source can just
/// join. Removing a link deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionTaskLink {
    pub id: i64,
    pub decision_id: i64,
    pub task_id: i64,
    /// When the link was drawn — the twin of [`TaskDependency::established_at`], and what dates a linked
    /// decision for the premise-change judgement (`AMB-D-372`).
    #[serde(default)]
    pub linked_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

// ───────────────────────── the unified dimension model ─────────────────────────
// Every axis a task can be classified along — phase, category, whatever the user invents — goes through
// one mechanism: the Dimension.

/// How many values of a dimension a task may hold. Single-select only: `(task, dimension)` is constrained
/// to one row. A one-variant enum is kept so the physical column `dimension.cardinality` keeps its meaning
/// in the type system — bringing multi-select back is then a matter of adding a variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionCardinality {
    #[default]
    Single,
}

impl DimensionCardinality {
    pub fn as_str(&self) -> &'static str {
        match self {
            DimensionCardinality::Single => "single",
        }
    }

    pub fn parse(s: &str) -> Option<DimensionCardinality> {
        match s {
            "single" => Some(DimensionCardinality::Single),
            _ => None,
        }
    }
}

/// A dimension's role. `TimeAxis` nominates this dimension as the project's axis of time, which is what
/// earns it special treatment in the views' filter affordances. The user is free to call that axis
/// "phase", or "sprint", or anything else — the role is the engine's vocabulary, the name is data. The
/// mechanism is identical to every other axis; only this flag is added.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionRole {
    #[default]
    None,
    TimeAxis,
}

impl DimensionRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            DimensionRole::None => "none",
            DimensionRole::TimeAxis => "time_axis",
        }
    }

    pub fn parse(s: &str) -> Option<DimensionRole> {
        match s {
            "none" => Some(DimensionRole::None),
            "time_axis" => Some(DimensionRole::TimeAxis),
            _ => None,
        }
    }
}

/// The classification axis itself — one "column". Scoped to a project; its set of values lives in
/// [`DimensionValue`], its assignments to tasks in [`TaskDimensionValue`] and its assignments to
/// decisions in [`DecisionDimensionValue`]. Categories, phases and any
/// axis a user invents all fold into this one mechanism. Every dimension is a plain, user-editable
/// classification axis: there are no built-in fixed axes and no locked values (status and priority are
/// first-class task attributes instead, not dimensions). `order_key` is where the dimension itself sits in
/// the display order; `ordered` says whether its **values** have an order (if they do, their `order_key`
/// is what sorts them).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Dimension {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    /// Free-form description (Markdown).
    #[serde(default)]
    pub notes: String,
    pub cardinality: DimensionCardinality,
    /// Do the values have an order? If so they sort by their `order_key`; if not, they are an unordered
    /// set.
    pub ordered: bool,
    pub role: DimensionRole,
    /// Does a task's value on this axis belong on its card? The axis holds the answer, not the device
    /// looking at it (`AMB-D-651`): which axes a board shows is a reading of the project that whoever
    /// raised the axis settles, so every face and every machine gets the same one. `false` is where an
    /// axis starts and where every axis an upgrade brings in stays, so the cards keep the surface
    /// `AMB-D-40` drew until somebody names an axis to widen it (`AMB-D-650`).
    #[serde(default)]
    pub show_on_card: bool,
    /// Does this axis refuse to be left empty? A task carrying no value here cannot have its creation
    /// finished (`AMB-D-734`): a task that was never classified is one nobody can hand to a release
    /// later, so the axis's raiser gets to say the axis has to be answered. It is the axis's own answer,
    /// like `show_on_card`, and it bites at one point only — `ops::task::finish_creating`. It is
    /// deliberately **not** a `ready` premise: raising the flag would otherwise drop every task already
    /// filed to `ready:no` in one stroke, and a flag that can be raised at any time must not be able to
    /// stop the whole backlog. `false` is where an axis starts and where every axis an upgrade brings in
    /// stays.
    #[serde(default)]
    pub required: bool,
    /// The axis's readable, stable key — what names it **outside** Amenbo (`AMB-D-735`). The id is the
    /// real identifier; the slug is the one that can be read and typed where the id cannot be and the
    /// display name (Japanese, spaces and all) may not go. Unique within the project. `None` is only
    /// what a store written before the column says, and what a row holds for the instant between being
    /// created and being filled: every saved row carries one, and it is `ops::dimension` that holds
    /// that true, not the table.
    #[serde(default)]
    pub slug: Option<String>,
    /// Where the dimension itself sits in the display order.
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// One value a dimension can take (say, "bug" on a category axis). On an ordered dimension `order_key` is
/// what sorts them; on an unordered one it is carried only as a stable key. `start_on` / `end_on` are the
/// **payload of the `DimensionRole::TimeAxis` role** — when the period this
/// value names begins and ends. The physical columns exist on every value, but their meaning, their
/// editability, and the resolution of "the current period" apply only to values on a time_axis dimension;
/// the layers above (ops / CLI / GUI) are the gatekeepers. They are held independently of `cardinality`
/// and `ordered`, so overlapping time axes (campaign-style) can reuse the same columns. This is a
/// different layer from [`Dimension::role`], which is the nomination flag — do not conflate the two.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DimensionValue {
    pub id: i64,
    pub dimension_id: i64,
    pub name: String,
    /// The value's readable, stable key — [`Dimension::slug`]'s counterpart, unique within the axis
    /// (`AMB-D-735`). It is what a caller outside Amenbo names this value by: a theme's branch, a
    /// release's folder, anything that has to spell a classification and cannot spell `AMB-DIMV-46`.
    #[serde(default)]
    pub slug: Option<String>,
    pub order_key: String,
    /// First day of the period, inclusive. `None` means the start is open.
    #[serde(default)]
    pub start_on: Option<NaiveDate>,
    /// Last day of the period, inclusive. `None` means "ongoing" — an open end.
    #[serde(default)]
    pub end_on: Option<NaiveDate>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl DimensionValue {
    /// Does `date` fall inside this value's period `[start_on, end_on]`, both ends inclusive? An open end
    /// is unbounded — a `None` side is always satisfied. Both ends `None` means the period was never set,
    /// and that covers **no** date at all: "the current period" has to resolve to exactly one of the values
    /// that actually drew a window, and letting the window-less ones match would empty the window of
    /// meaning.
    pub fn covers(&self, date: NaiveDate) -> bool {
        if self.start_on.is_none() && self.end_on.is_none() {
            return false;
        }
        self.start_on.is_none_or(|s| s <= date) && self.end_on.is_none_or(|e| date <= e)
    }
}

/// The assignment of a dimension value to a task — the join record. `dimension_id` is denormalised onto
/// it so that ops and reads can enforce the single-select `(task, dimension)` one-row constraint, and
/// filter on an axis directly, without joining through to the value. Removing an assignment deletes the
/// row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskDimensionValue {
    pub id: i64,
    pub task_id: i64,
    pub dimension_id: i64,
    pub value_id: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The assignment of a dimension value to a **decision** — the task join record's twin (`AMB-D-781`).
/// Same shape and same denormalised `dimension_id`, and the values it names are the axis's own: a
/// decision is classified along the axes the project already has, never along a set raised for
/// decisions. A separate table rather than a `target_type` column on [`TaskDimensionValue`], for the
/// reason `decision_comment` is separate from `task_comment` — each end keeps a real foreign key.
/// Removing an assignment deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionDimensionValue {
    pub id: i64,
    pub decision_id: i64,
    pub dimension_id: i64,
    pub value_id: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A durable comment. It is durable, human-authored data, so it gets a table of its own, `task_comment`
/// (the `task_` prefix is what distinguishes it from `decision_comment` and the like). A comment is always
/// addressed to a task and always has a body. The other half of the timeline — the system events — exists
/// as no row at all: those live only in the ledger file ([`crate::activity_log`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskComment {
    pub id: i64,
    pub task_id: i64,
    /// The author's facet. `None` reads as human (older data).
    #[serde(default)]
    pub author_kind: Option<ActorKind>,
    pub text: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// When the body was edited in place afterwards; `None` if it never was. `updated_at` cannot stand in
    /// for it: instants are second-precision, so an edit made within the same second leaves `updated_at`
    /// equal to `created_at`. We keep no revision history by design, which makes this the only clue a
    /// reader has that the text is not the text they read a moment ago.
    #[serde(default)]
    pub edited_at: Option<Timestamp>,
}

/// A durable comment on a decision record. Its own table, `decision_comment`, mirroring `TaskComment`
/// rather than sharing one polymorphic table: every comment holds a real FK to its parent
/// (`decision_id → decision.id`), and anything decision-specific that shows up later can grow here
/// without littering `task_comment` with nullable columns.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionComment {
    pub id: i64,
    pub decision_id: i64,
    /// The author's facet. `None` reads as human (older data).
    #[serde(default)]
    pub author_kind: Option<ActorKind>,
    pub text: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// When the body was edited in place afterwards; `None` if it never was. `updated_at` cannot stand in
    /// for it: instants are second-precision, so an edit made within the same second leaves `updated_at`
    /// equal to `created_at`. We keep no revision history by design, which makes this the only clue a
    /// reader has that the text is not the text they read a moment ago.
    #[serde(default)]
    pub edited_at: Option<Timestamp>,
}

/// How an attachment was taken in. `Blob` is the default — the file is ingested into the store,
/// content-addressed, and the bytes `blob_hash` points at live out-of-band rather than in the engine.
/// `Url` is an external link, which we do not manage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    #[default]
    Blob,
    Url,
}

impl AttachmentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttachmentKind::Blob => "blob",
            AttachmentKind::Url => "url",
        }
    }

    pub fn parse(s: &str) -> Option<AttachmentKind> {
        match s {
            "blob" => Some(AttachmentKind::Blob),
            "url" => Some(AttachmentKind::Url),
            _ => None,
        }
    }
}

/// What an attachment hangs off. Tasks and decision records themselves, and the comments on either
/// (`task_comment` / `decision_comment`). A comment's attachments are kept **separately** from the parent
/// record's, which is what preserves the chronology of which comment a file arrived with. The target is
/// polymorphic — `target_type` names the table, and SQL cannot enforce a reference across it. The
/// `target_type` column is a string, so adding a variant is purely additive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentTarget {
    #[default]
    Task,
    Decision,
    /// Attached to a durable comment on a task ([`TaskComment`]).
    TaskComment,
    /// Attached to a durable comment on a decision record ([`DecisionComment`]).
    DecisionComment,
}

impl AttachmentTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttachmentTarget::Task => "task",
            AttachmentTarget::Decision => "decision",
            AttachmentTarget::TaskComment => "task_comment",
            AttachmentTarget::DecisionComment => "decision_comment",
        }
    }

    pub fn parse(s: &str) -> Option<AttachmentTarget> {
        match s {
            "task" => Some(AttachmentTarget::Task),
            "decision" => Some(AttachmentTarget::Decision),
            "task_comment" => Some(AttachmentTarget::TaskComment),
            "decision_comment" => Some(AttachmentTarget::DecisionComment),
            _ => None,
        }
    }

    /// The ref space the target is numbered in — what turns `(target_type, target_id)` back into the
    /// ref a reader knows it by (`AMB-T-12`, `AMB-TC-12`). The pair is polymorphic, so this mapping is
    /// the one place the column's four values line up with [`crate::idref::RefKind`]; everything that
    /// has to name a target quotes it through here rather than spelling the four cases again.
    pub const fn ref_kind(self) -> crate::idref::RefKind {
        match self {
            AttachmentTarget::Task => crate::idref::RefKind::Task,
            AttachmentTarget::Decision => crate::idref::RefKind::Decision,
            AttachmentTarget::TaskComment => crate::idref::RefKind::TaskComment,
            AttachmentTarget::DecisionComment => crate::idref::RefKind::DecisionComment,
        }
    }

    /// The target rendered as the ref a reader quotes it by.
    pub fn target_ref(self, id: i64) -> String {
        crate::idref::render(self.ref_kind(), id)
    }
}

/// An attachment on a task or a decision record. Two modes: `blob` (the default — ingested into the store,
/// content-addressed) and `url` (an external link we do not manage). A blob's bytes are not held in the
/// engine but out-of-band in the content-addressed blob store, and all the truth source keeps here is the
/// metadata: `blob_hash` / `filename` / `mime` / `size_bytes`. In url mode the external link sits in `url`
/// and the blob metadata columns are empty.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i64,
    /// What it hangs off (task / decision / task_comment / decision_comment).
    pub target_type: AttachmentTarget,
    /// The id (INTEGER primary key) of the record it hangs off. Which table that is, `target_type` says.
    pub target_id: i64,
    pub kind: AttachmentKind,
    /// The content-address in blob mode — a BLAKE3 fingerprint of the bytes. It is what gives us dedup,
    /// tamper detection, and identity across devices. `None` in url mode.
    #[serde(default)]
    pub blob_hash: Option<String>,
    /// What the attachment is called: the file's own name (blob) or the display label (url). A blob
    /// renamed at attach time carries the label with the file's suffix kept on it, because this one
    /// field is read as a name again past the ingest — `attach save` writes under it, and `attach open`
    /// gives the OS a copy carrying its extension. What the bytes *are* is [`Attachment::mime`]'s, read
    /// off the file itself and never off this.
    #[serde(default)]
    pub filename: Option<String>,
    /// MIME type — the GUI picks its viewer by it.
    #[serde(default)]
    pub mime: Option<String>,
    /// The blob's byte length, for keeping an eye on how much space attachments take. `None` in url mode.
    #[serde(default)]
    pub size_bytes: Option<i64>,
    /// The external link in url mode. `None` in blob mode.
    #[serde(default)]
    pub url: Option<String>,
    /// The provenance facet (human / ai). `None` for placeholders and older data.
    #[serde(default)]
    pub created_by_kind: Option<ActorKind>,
    /// Where it sits among the attachments on the same target.
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A serde-shaped vessel holding every record of one store at once. It is **not the store's contents**:
/// the truth source is SQLite, and [`crate::store::Store`] does not hold one of these. The shape exists
/// for the two places that need the records handed over **as a single lump**: verifying a backup or a
/// restore (that this vessel can be raised from a snapshot is the proof that the snapshot has not just the
/// structure but the contents — [`mod@crate::archive`]), and the projection-parity tests (raise the vessel
/// from the truth source, project it back onto the engine, and compare for fidelity). It is not the
/// substrate of export: export streams rows straight out of the read model and never
/// hydrates into this ([`crate::export`]). Nor is there any way to load rows into a store through it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Database {
    pub schema_version: String,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub task_dependencies: Vec<TaskDependency>,
    /// A task's recorded commit SHAs. Hydration tolerates their absence — a store predating the table
    /// yields an empty vec.
    #[serde(default)]
    pub task_commits: Vec<TaskCommit>,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    /// Edges between decisions. Hydration tolerates their absence — a store without them yields an empty
    /// vec.
    #[serde(default)]
    pub decision_edges: Vec<DecisionEdge>,
    #[serde(default)]
    pub decision_task_links: Vec<DecisionTaskLink>,
    /// The unified dimension model. Hydration tolerates its absence — a store without it yields an empty
    /// vec.
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
    #[serde(default)]
    pub dimension_values: Vec<DimensionValue>,
    #[serde(default)]
    pub task_dimension_values: Vec<TaskDimensionValue>,
    /// The decision side of the dimension model. Hydration tolerates its absence — a store predating
    /// the table yields an empty vec.
    #[serde(default)]
    pub decision_dimension_values: Vec<DecisionDimensionValue>,
    #[serde(default)]
    pub task_comments: Vec<TaskComment>,
    /// Comments on decision records. Hydration tolerates their absence — a store without them yields an
    /// empty vec.
    #[serde(default)]
    pub decision_comments: Vec<DecisionComment>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// The current schema version.
pub const SCHEMA_VERSION: &str = "1";

/// The store **format version** this binary reads and writes — a monotonically increasing scalar, and a
/// different thing entirely from the frozen `SCHEMA_VERSION` (`"1"`). It is bumped only by a **breaking
/// migration**: one that drops or renames a column or table an older reader's SQL needs. Every write-path
/// open stamps this version into `store_meta.format_version`. The forward-migration gate compares the
/// `format_version` a store recorded against this constant, and if `store > FORMAT_VERSION` it fails with
/// a clear error: this store has been updated by a newer Amenbo, so update to the latest one
/// (`amenbo update`). **The chain decides the version.** This constant *is* the end of the version chain
/// ([`crate::store_engine::migrate::LATEST_VERSION`]), and **a number is never written here**: add one
/// step and the version goes up; without a step it cannot. The thing that carries a breaking migration is
/// that numbered step, not the open. A store with no `format_version` key **reads as v0**
/// ([`crate::store_engine::read_format_version`]).
/// The chain's baseline ([`crate::store_engine::migrate::BASELINE_VERSION`]) is the oldest store this
/// build can open; there is no path back to anything older.
pub const FORMAT_VERSION: i64 = crate::store_engine::migrate::LATEST_VERSION;

impl Default for Database {
    fn default() -> Self {
        Database {
            schema_version: SCHEMA_VERSION.to_string(),
            projects: Vec::new(),
            tasks: Vec::new(),
            task_dependencies: Vec::new(),
            task_commits: Vec::new(),
            decisions: Vec::new(),
            decision_edges: Vec::new(),
            decision_task_links: Vec::new(),
            dimensions: Vec::new(),
            dimension_values: Vec::new(),
            task_dimension_values: Vec::new(),
            decision_dimension_values: Vec::new(),
            task_comments: Vec::new(),
            decision_comments: Vec::new(),
            attachments: Vec::new(),
        }
    }
}

