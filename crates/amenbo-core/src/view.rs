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
/// Four premises, all of them the user's own rather than Amenbo's: no blocker still open, no linked
/// decision still unsettled, the declared start day arrived, and the task's creation finished. No
/// `start_on` means nothing was declared about when to start, which is not a reason to hold the task
/// back.
///
/// The fourth is `draft` (`AMB-D-553`): while a task is still being put together its other premises have
/// not been declared yet, so a `ready` taken before the creation finished would be saying "every premise
/// holds" about a task nobody has finished writing (`AMB-D-552`). It holds the task out of the mailbox
/// and out of a reservation, and out of nothing else — a draft is on the board and in every listing
/// (`AMB-D-555`).
///
/// `today` is the caller's reference day ([`crate::time::today`]), the same one the `status` view's
/// buckets are cut against — a task the status view calls started must not read as not-yet-started here.
/// The `ready:` filter says the same thing in SQL, where the premises are predicates rather than
/// booleans; that is the one restatement, and it is held to this one by test.
#[must_use]
pub fn is_ready(
    has_open_blocker: bool,
    has_unsettled_premise: bool,
    start_on: Option<NaiveDate>,
    today: NaiveDate,
    draft: bool,
) -> bool {
    !has_open_blocker
        && !has_unsettled_premise
        && not_started_until(start_on, today).is_none()
        && !draft
}

/// The third premise as a *reason* rather than a boolean: the declared start day, when it is still ahead
/// of `today` — i.e. the date this task is waiting for. `None` means the start day is no reason to hold
/// the task back, either because none was declared or because it has arrived.
///
/// A read projects this beside `blocked_by_open` / `blocked_by_decisions` so that every `ready: false` on
/// a face carries the reason for it: a task in a listing is never left saying "not ready" with nothing to
/// point at. [`is_ready`] derives its third premise from here, so the reason and the verdict are one
/// thing.
#[must_use]
pub fn not_started_until(start_on: Option<NaiveDate>, today: NaiveDate) -> Option<NaiveDate> {
    start_on.filter(|start| *start > today)
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
/// [`DecisionRef`] because it has to carry **what overturned the premise**: surfacing a decision that
/// stands on ground since replaced is the entire reason this type exists. `superseded_by` names the
/// successor, and is `None` when nothing replaced it.
#[derive(Clone, Debug, Serialize)]
pub struct PremiseRef {
    pub id: i64,
    /// `None` when the premise target dangles (a `builds_on` edge onto a decision no longer live). The
    /// face composes the placeholder; core does not hold a display string.
    pub name: Option<String>,
    /// Conversational ref of the decision that overturned the premise — where to re-point, or what to
    /// revisit. `None` is the whole of "nothing overturned it": there is no second field saying the
    /// same thing, and so no way for the two to disagree (`AMB-D-410`).
    pub superseded_by: Option<String>,
}

/// The premises a task acquired **after its current status began** (`AMB-D-366`) — what a caller surfaces
/// to a holder whose reservation may have been silently undercut. Each list is the added premises that
/// still bear on readiness (a blocker that has not ended, an unsettled decision); an added edge onto a
/// task that is already over, or a link onto a settled decision, is not here, because it never moved
/// `ready`. Read-only: *how strongly to
/// react* (a quiet note on a read, a firm warn at completion) is the caller's, not this type's.
#[derive(Clone, Debug, Serialize)]
pub struct PremiseChange {
    /// Not-done blockers whose dependency edge was added after the status began, in edge order.
    pub added_blockers: Vec<TaskRef>,
    /// Unsettled decisions linked after the status began, in link order.
    pub added_decisions: Vec<DecisionRef>,
    /// Decisions already linked that **stopped being settled** after the status began (`AMB-D-373`), in link
    /// order. Disjoint from `added_decisions`. Both ways of it are here: reopened or rejected under the
    /// holder, and superseded under the holder — the second dated by the `supersedes` edge, currency being
    /// an edge rather than a status.
    pub reopened_decisions: Vec<DecisionRef>,
}

impl PremiseChange {
    /// Whether any premise moved after the status began — the bare "did it change?" bit, leaving the
    /// reaction to the caller.
    pub fn any(&self) -> bool {
        !self.added_blockers.is_empty()
            || !self.added_decisions.is_empty()
            || !self.reopened_decisions.is_empty()
    }
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
    /// Derived from the declared start day: the date this task is waiting for, when that day is still
    /// ahead ([`not_started_until`]). The third reason a task is not ready, beside the two above — always
    /// serialized, `null` when the start day is no reason.
    pub not_started_until: Option<NaiveDate>,
    /// Still being put together: the fourth reason a task is not ready (`AMB-D-553`). Carried on the row
    /// rather than derived, because the stage a creation is at is stored and not computed — and carried
    /// at all because a draft **is** listed (`AMB-D-555`), so its row has to be able to say why it is not
    /// in the mailbox.
    pub draft: bool,
    /// Derived: no open blockers, no unsettled grounds, the declared start day arrived, and the creation
    /// finished — i.e. this can be started ([`is_ready`]).
    pub ready: bool,
}

/// Where a task sits: the project it is placed in, and its order within that project.
#[derive(Clone, Debug, Serialize)]
pub struct PlacementView {
    pub project: ProjectRef,
    pub order_key: String,
}

/// The bound folder a task is worked in (`AMB-D-648`): the binding row's id, and the path it records.
/// Both, because the two answer different readers — a person reads the path, and whatever re-points or
/// re-reads the folder later needs the id, the path being the half that changes.
#[derive(Clone, Debug, Serialize)]
pub struct FolderView {
    pub binding_id: i64,
    pub dir: String,
}

/// One axis a task is classified on, and the value it holds there (`AMB-D-101`) — both by name, because
/// this is what a reader is shown. There is nothing else to carry: an axis is single-select, so one axis
/// is one value.
#[derive(Clone, Debug, Serialize)]
pub struct ClassifiedAs {
    /// The axis's name, as the dimension is called.
    pub dimension: String,
    /// The value it holds on that axis.
    pub value: String,
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
    /// The bound folder this task is worked in (`AMB-D-648`), or `null` for a task that names none —
    /// which is every task unless somebody said so. It is resolved against the folders the task's **own
    /// project** offers, so a task carrying an id that project no longer has (its folder unbound, or the
    /// task moved to another project) reads as naming no folder rather than pointing outside.
    pub at: Option<FolderView>,
    /// What the task is classified as, axis by axis (`AMB-D-101`) — the words, not the ids, since this is
    /// the shape a face shows. An axis the task holds no value on is not in the list, so an empty one
    /// means unclassified rather than "no axes exist".
    pub dimensions: Vec<ClassifiedAs>,
    /// Dependencies: blockers that are not done yet (id + title). Empty means nothing is in the way.
    pub blocked_by: Vec<TaskRef>,
    /// Premises: linked decisions that are not live grounds (id + title). Empty means the premises hold.
    pub blocked_by_decisions: Vec<DecisionRef>,
    /// The declared start day, when it is still ahead ([`not_started_until`]) — the third reason this task
    /// is not ready, beside the two above. Always serialized, `null` when the start day is no reason.
    pub not_started_until: Option<NaiveDate>,
    /// Still being put together — the fourth reason this task is not ready (`AMB-D-553`). The one premise
    /// the reader of a detail page can settle on the spot: finishing the creation is what clears it.
    pub draft: bool,
    /// Derived: no open blockers, no unsettled grounds, the declared start day arrived, and the creation
    /// finished — i.e. this can be started ([`is_ready`]).
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
    /// The decisions that superseded this one, as conversational refs (`AMB-D-<n>`), in the order the
    /// edges were drawn. This is the whole of what the row says about being replaced (`AMB-D-410`): the
    /// edges are the author's own, and a reader goes to *which* decision overturned it rather than to a
    /// word this row invented. Refs rather than [`DecisionRef`]s because a listing resolves no titles:
    /// the row says where to look, and `decision show` is where it is read.
    pub superseded_by: Vec<String>,
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
    /// The decisions this one supersedes (id + title). One decision can supersede several — the edges
    /// form a DAG.
    pub supersedes: Vec<DecisionRef>,
    /// The decisions that superseded this one (the reverse lookup — a derived view over the edges).
    pub superseded_by: Vec<DecisionRef>,
    /// The decisions this one partially amends (the target stays current; id + title).
    pub amends: Vec<DecisionRef>,
    /// The decisions that partially amend this one (the reverse lookup).
    pub amended_by: Vec<DecisionRef>,
    /// The decisions this one **builds on** — read these first. A `builds_on` edge changes nothing about
    /// its target, but each premise carries what replaced it, so that a **rotten premise** — a decision
    /// standing on ground that has since been overturned — is visible.
    pub builds_on: Vec<PremiseRef>,
    /// The decisions that build on this one (the reverse lookup) — part of the blast radius: overturn this
    /// one and these need revisiting.
    pub built_on_by: Vec<DecisionRef>,
    /// What this decision is filed under, axis by axis (`AMB-D-781`) — the same shape a task's page
    /// carries, over the same axes and the same values. Empty where nothing was filed, and in a project
    /// that declares no axis at all.
    pub dimensions: Vec<ClassifiedAs>,
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
/// `Ref`), `linked_task_count` (how many live tasks are linked) and `superseded_by` (the ids of the
/// decisions that overturned it), which is what lets the engine's indexed read-model SQL
/// ([`crate::query::decision_list`]) build the same card without a per-decision scan. The successors
/// arrive as ids and are spelled into refs here, so no caller can spell them its own way.
pub fn decision_compact_with(
    d: &Decision,
    project: Option<ProjectRef>,
    linked_task_count: usize,
    superseded_by: &[i64],
) -> DecisionCompact {
    DecisionCompact {
        id: d.id,
        title: d.title.clone(),
        // The body is omitted by default; the `decision list --with-body` path fills in `d.body` later.
        body: None,
        status: d.status,
        r#ref: decision_display_ref(d),
        project,
        superseded_by: superseded_by.iter().map(|id| crate::idref::decision(*id)).collect(),
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
/// at the same derivations, and nothing else — open blockers (a live dependency edge whose far end is
/// not done), unsettled grounds (a linked decision that is not accepted), a start day still ahead, and a
/// creation not finished.
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
    /// The task declares a start day that has not arrived. Unlike the two above, this one clears itself:
    /// the day comes and the task is ready, with nothing to do. `start_on` is the declared day, so the
    /// error can say when the wait ends.
    NotStartedYet { start_on: NaiveDate },
    /// The task is still being put together (`AMB-D-553`). It carries no value: the whole of the reason
    /// is that the creation has not been finished, and what clears it is finishing it — there is nothing
    /// further for the refusal to name.
    StillDraft,
}
