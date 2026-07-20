//! The read-side DTOs. The `--json` output contract *is* the serialization of these types — nothing
//! else defines it.
//!
//! What we hand back is not the store's raw records but a projection, with references resolved and
//! counts rolled up. The projections are assembled by the read model's indexed SQL
//! (`store_engine::read`).
//!
//! Fields that have no value yet (assignee_kind, num_comments) still emit their key, with an empty value,
//! so an AI consumer never has to reason about which keys a given build happens to include.

use chrono::NaiveDate;
use serde::Serialize;

use crate::model::{ActorKind, Decision, DecisionStatus, Priority, Subtype, Task, TaskStatus};
use crate::time::Timestamp;

/// **The** ready predicate — every read that projects `ready` derives it here, so the task card, the
/// task detail and the reserve guard cannot come to disagree about what "this can be started" means.
/// Three premises, all of them declared by the user rather than by amenbo: no blocker still open, no
/// linked decision still unsettled, and the declared start day arrived. No `start_on` means nothing was
/// declared about when to start, which is not a reason to hold the task back.
///
/// `today` is the caller's reference day ([`crate::time::today`]), the same one the `status` view's
/// buckets are cut against — a task the status view calls started must not read as not-yet-started here.
/// The `ready:` filter says the same thing in SQL, where the three premises are predicates rather than
/// booleans; that is the one restatement, and it is held to this one by test.
#[must_use]
pub fn is_ready(
    has_open_blocker: bool,
    has_unsettled_premise: bool,
    start_on: Option<NaiveDate>,
    today: NaiveDate,
) -> bool {
    !has_open_blocker && !has_unsettled_premise && start_on.is_none_or(|start| start <= today)
}

#[derive(Clone, Debug, Serialize)]
pub struct Ref {
    pub id: String,
    pub name: String,
}

/// A reference to a project (id + display name). Its id is an integer key, so it gets its own type
/// rather than reusing [`Ref`], which carries opaque tokens that are not entity keys (`decided_by` and
/// the like).
#[derive(Clone, Debug, Serialize)]
pub struct ProjectRef {
    pub id: i64,
    pub name: String,
}

/// A reference to a task (id + title). The id is an integer key.
#[derive(Clone, Debug, Serialize)]
pub struct TaskRef {
    pub id: i64,
    pub name: String,
}

/// A reference to a task a decision spawned (id + title + status). It is not a plain [`TaskRef`] because
/// it has to carry **whether that work is still outstanding**: a bare list of titles never answers "is the
/// work this decision created finished?", and the reader can sink the done ones only if the status comes
/// along.
#[derive(Clone, Debug, Serialize)]
pub struct LinkedTaskRef {
    pub id: i64,
    pub name: String,
    pub status: TaskStatus,
}

/// A reference to a decision (id + title). The id is an integer key. `name` is `None` when the target
/// dangles — a forward edge (supersedes / amends) pointing at a decision that is no longer live, so its
/// title cannot be read. The face composes the placeholder; core does not hold a display string.
#[derive(Clone, Debug, Serialize)]
pub struct DecisionRef {
    pub id: i64,
    pub name: Option<String>,
}

/// A reference to a premise decision — the far end of a `builds_on` edge. It is not a plain
/// [`DecisionRef`] because it has to carry **whether the premise is still live**: surfacing a decision
/// that stands on a premise since overturned is the entire reason this type exists. `superseded_by` names
/// the successor, and is `None` while the premise is current.
#[derive(Clone, Debug, Serialize)]
pub struct PremiseRef {
    pub id: i64,
    /// `None` when the premise target dangles (a `builds_on` edge onto a decision no longer live). The
    /// face composes the placeholder; core does not hold a display string.
    pub name: Option<String>,
    /// Is the premise current (i.e. `superseded_by` is empty)?
    pub current: bool,
    /// Conversational ref of the decision that overturned the premise — where to re-point, or what to
    /// revisit.
    pub superseded_by: Option<String>,
}

/// The minimal shape of a task, as returned by `status`, `task list` and friends.
#[derive(Clone, Debug, Serialize)]
pub struct TaskCompact {
    pub id: i64,
    pub title: String,
    pub completed: bool,
    pub status: TaskStatus,
    pub due_on: Option<NaiveDate>,
    pub start_on: Option<NaiveDate>,
    pub priority: Option<Priority>,
    /// The conversational ref, i.e. the displayed `AMB-T-<n>`. The primary key *is* the conversational
    /// number, so this is rendered straight from `id`.
    pub r#ref: String,
    pub project: Option<ProjectRef>,
    /// The assignee's facet (human / ai). The display name is the caller's to look up in config.
    pub assignee_kind: Option<ActorKind>,
    /// Derived from dependencies: the task_ids of blockers that are not done yet. Empty means nothing is
    /// in the way.
    pub blocked_by_open: Vec<i64>,
    /// Derived from premises: the decision_ids of linked decisions that are not live grounds. Empty means
    /// the premises hold.
    pub blocked_by_decisions: Vec<i64>,
    /// Derived: no open blockers, no unsettled grounds, and the declared start day arrived — i.e. this
    /// can be started ([`is_ready`]).
    pub ready: bool,
}

/// Where a task sits: the project it is placed in, and its order within that project.
#[derive(Clone, Debug, Serialize)]
pub struct PlacementView {
    pub project: ProjectRef,
    pub order_key: String,
}

/// The full-field shape of a task, as returned by `task show`.
#[derive(Clone, Debug, Serialize)]
pub struct TaskDetail {
    pub id: i64,
    pub resource_type: &'static str,
    pub title: String,
    pub notes: String,
    pub subtype: Subtype,
    pub completed: bool,
    pub completed_at: Option<Timestamp>,
    pub status: TaskStatus,
    /// Creator and assignee are facets and nothing more. The display name is the caller's to look up in
    /// config.
    pub created_by_kind: Option<ActorKind>,
    pub assignee_kind: Option<ActorKind>,
    pub start_on: Option<NaiveDate>,
    pub due_on: Option<NaiveDate>,
    pub priority: Option<Priority>,
    /// The conversational ref, i.e. the displayed `AMB-T-<n>` (rendered from `id`).
    pub r#ref: String,
    /// Where the task sits; absent when it is unplaced (inbox).
    pub placement: Option<PlacementView>,
    /// Dependencies: blockers that are not done yet (id + title). Empty means nothing is in the way.
    pub blocked_by: Vec<TaskRef>,
    /// Premises: linked decisions that are not live grounds (id + title). Empty means the premises hold.
    pub blocked_by_decisions: Vec<DecisionRef>,
    /// Derived: no open blockers, no unsettled grounds, and the declared start day arrived — i.e. this
    /// can be started ([`is_ready`]).
    pub ready: bool,
    /// The reverse of `blocked_by`: the not-yet-done tasks that hold this one as a blocker — what
    /// finishing this task unblocks (empty means nothing waits on it).
    pub blocks: Vec<TaskRef>,
    pub num_comments: usize,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A task's conversational ref: `AMB-T-<n>`. The primary key *is* the conversational number, so this is
/// just how `id` is displayed — not a second number, and there is no such state as "not numbered yet". The
/// spelling belongs to [`crate::idref`].
pub fn display_ref(task: &Task) -> String {
    crate::idref::task(task.id)
}

// ───────────────────────── decision (read layer) ─────────────────────────

/// The minimal shape of a decision, as returned by `decision list` and friends.
#[derive(Clone, Debug, Serialize)]
pub struct DecisionCompact {
    pub id: i64,
    pub title: String,
    /// The decision's body. Omitted by default, to keep listings cheap. `decision list --with-body` adds
    /// it as a column on the filtered/paged result — a bounded read for spotting semantic contradictions,
    /// and for proposing only. `None` is dropped by `serde`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub status: DecisionStatus,
    /// The conversational ref: `AMB-D-<n>` — a number space of its own, separate from a task's. Rendered from
    /// `id`.
    pub r#ref: String,
    pub project: Option<ProjectRef>,
    /// Is this current, i.e. is it not pointed at by a live `supersedes` edge? A derived projection, never
    /// a stored flag.
    pub current: bool,
    pub decided_at: Option<Timestamp>,
    pub created_at: Timestamp,
    /// How many live tasks are linked to it.
    pub linked_task_count: usize,
}

/// The full-field shape of a decision, as returned by `decision show`.
#[derive(Clone, Debug, Serialize)]
pub struct DecisionDetail {
    pub id: i64,
    pub resource_type: &'static str,
    pub project: Option<ProjectRef>,
    pub r#ref: String,
    pub title: String,
    pub body: String,
    pub status: DecisionStatus,
    /// Is this current, i.e. is `superseded_by` empty? A derived projection; it never shows up in `status`.
    pub current: bool,
    /// The decisions this one supersedes (id + title). One decision can supersede several — the edges
    /// form a DAG.
    pub supersedes: Vec<DecisionRef>,
    /// The decisions that superseded this one (the reverse lookup — a derived view over the edges).
    pub superseded_by: Vec<DecisionRef>,
    /// The decisions this one partially amends (the target stays current; id + title).
    pub amends: Vec<DecisionRef>,
    /// The decisions that partially amend this one (the reverse lookup).
    pub amended_by: Vec<DecisionRef>,
    /// The decisions this one **builds on** — read these first. A `builds_on` edge changes neither the
    /// target's currency (it is not greyed out) nor how it is read, but the currency rides along so that a
    /// **rotten premise** — a decision standing on ground that has since been overturned — is visible.
    pub builds_on: Vec<PremiseRef>,
    /// The decisions that build on this one (the reverse lookup) — part of the blast radius: overturn this
    /// one and these need revisiting.
    pub built_on_by: Vec<DecisionRef>,
    pub decided_at: Option<Timestamp>,
    /// Who accepted it: a free-text decider token, not an entity key. The token is read into both `id`
    /// and `name`, so the two always carry the same string.
    pub decided_by: Option<Ref>,
    /// The live tasks linked to it (id + title + status).
    pub linked_tasks: Vec<LinkedTaskRef>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A decision's conversational ref: `AMB-D-<n>`. Decisions live in a number space of their own, separate
/// from tasks (`AMB-T-<n>`), and the kind code is what keeps the two unambiguous. This is just how `id` is
/// displayed; the spelling belongs to [`crate::idref`].
pub fn decision_display_ref(d: &Decision) -> String {
    crate::idref::decision(d.id)
}

/// Assemble a decision card without reaching into the store. The caller supplies `project` (the project's
/// `Ref`) and `linked_task_count` (how many live tasks are linked), which is what lets the engine's
/// indexed read-model SQL ([`crate::query::decision_list`]) build the same card without a per-decision
/// scan.
pub fn decision_compact_with(
    d: &Decision,
    project: Option<ProjectRef>,
    linked_task_count: usize,
    current: bool,
) -> DecisionCompact {
    DecisionCompact {
        id: d.id,
        title: d.title.clone(),
        // The body is omitted by default; the `decision list --with-body` path fills in `d.body` later.
        body: None,
        status: d.status,
        r#ref: decision_display_ref(d),
        project,
        current,
        decided_at: d.decided_at,
        created_at: d.created_at,
        linked_task_count,
    }
}

/// Sort rank for a priority (`None` sorts last).
pub fn priority_rank(p: Option<Priority>) -> u8 {
    p.map(|p| p.rank()).unwrap_or(u8::MAX)
}

// ───────────────────────── dependencies (derived state) ─────────────────────────

/// One reason a reservation (`todo → in_progress`) is refused.
///
/// This is the evidence for `ready` being false, kept structured so the message can name the actual
/// reason ([`crate::ops::task::set_status`] composes the `not_ready` text from it).
///
/// **Invariant**: [`crate::store_engine::read::reserve_blockers`] is empty ⇔ `ready`. To keep the
/// reservation guard and the mailbox's `ready:` filter from drifting onto different predicates, both look
/// at the same two derivations, and nothing else — open blockers (a live dependency edge whose far end is
/// not done) and unsettled grounds (a linked decision that is not accepted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReserveBlocker {
    /// A predecessor task that is not done yet. `label` is the conversational ref (`AMB-T-12`).
    OpenBlocker { label: String },
    /// A linked decision that is not live ground (`proposed` / `rejected` / `superseded`).
    /// `superseded_by` is the successor resolved by reverse lookup — where to re-point the link.
    UnsettledPremise {
        label: String,
        status: DecisionStatus,
        superseded_by: Option<String>,
    },
}
