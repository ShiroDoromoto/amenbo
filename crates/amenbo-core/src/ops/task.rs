//! Task operations.
//!
//! Ordering: a task's placement is `Task.order_key`, held on the task itself. Relations between tasks are
//! expressed by exactly one thing — the dependency edge (`ops/dependency.rs`).
//!
//! **Writes are SQL straight at the engine.** Each mutator takes the [`WriteTx`] (`BEGIN IMMEDIATE`) the
//! caller opened, and does its reads (the `before` snapshot, the new id from `next_id`, the siblings' `order_key`s,
//! the CAS for a reservation) and its writes **inside the same transaction**. Hoist a read out of it and two
//! writers will both write on top of the same snapshot, erasing each other. The transaction is opened by
//! [`crate::Store`]'s write wrappers (`add_task` / `set_task_status` / …), which are all the CLI and the GUI
//! ever call.

use chrono::NaiveDate;

use crate::error::{Error, Result};
use crate::model::{
    ActorKind, DecisionStatus, Priority, Subtype, Task, TaskStatus,
};
use crate::ops::{emit_create, emit_update, place, Noun, Position};
use crate::store_engine::{read, record, WriteTx};
use crate::view::ReserveBlocker;
use crate::time::Timestamp;

/// This entity's word (the English/Japanese pair for `not_found` messages).
pub(crate) const NOUN: Noun = Noun { en: "task", ja: "タスク" };

/// Which kind a type-prefixed reference names — the code in `AMB-T-<n>` / `AMB-D-<n>`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TypedKind {
    Task,
    Decision,
}

/// Split a type-prefixed reference — `AMB-T-<n>` / `AMB-D-<n>`, or the bare `T-<n>` / `D-<n>`, prefixes
/// case-insensitive — into `(kind, number)`. Tasks and decisions have separate number spaces, so the code
/// lets a number that `#<n>` alone would leave ambiguous name its type. `None` if the input does not fit the
/// form. Shared with decision reference resolution (hence `pub(crate)`).
///
/// `AMB-` is what amenbo *renders* ([`crate::idref`]), so accepting it is what makes a ref the user
/// copied off the screen paste back in. The bare form stays accepted because reading is the loose side: the
/// collision the namespace exists to prevent is in foreign text, and text a user hands amenbo directly is
/// not foreign.
pub(crate) fn parse_typed_ref(s: &str) -> Option<(TypedKind, u32)> {
    let (head, num) = crate::idref::strip_namespace(s).split_once('-')?;
    let kind = if head.eq_ignore_ascii_case(crate::idref::RefKind::Task.code()) {
        TypedKind::Task
    } else if head.eq_ignore_ascii_case(crate::idref::RefKind::Decision.code()) {
        TypedKind::Decision
    } else {
        return None;
    };
    Some((kind, num.parse().ok()?))
}

/// Read `#12` or `12` (decimal only) as a `number`; `None` if it does not fit. Shared with decisions
/// (`pub(crate)`).
pub(crate) fn parse_number_ref(s: &str) -> Option<u32> {
    let body = s.strip_prefix('#').unwrap_or(s);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    body.parse().ok()
}

pub struct NewTask {
    pub title: String,
    /// The already-resolved project id.
    pub project_id: Option<i64>,
    pub due_on: Option<NaiveDate>,
    pub start_on: Option<NaiveDate>,
    pub priority: Option<Priority>,
    pub notes: String,
    /// The creator's facet. Left unset, the creator is unknown (and treated as "not authored by the AI").
    pub created_by_kind: Option<ActorKind>,
}

/// Create a task. **There are two read-then-writes here** — the conversational sequence number
/// (`next_id`) and the siblings' `order_key`s for placement. Both are read inside the same
/// `BEGIN IMMEDIATE` transaction as the write: read them outside it and two concurrent writers see the same
/// high-water mark and **take the same number**.
pub fn add(tx: &WriteTx<'_>, input: NewTask) -> Result<Task> {
    if input.title.trim().is_empty() {
        return Err(Error::invalid("a task title cannot be empty", "タスクのタイトルは空にできません"));
    }
    let now = Timestamp::now();

    // Allocate the conversational sequence number. The id (INTEGER PK) *is* that number, and numbers are
    // **globally unique on the device** — so the mark is read without narrowing to a project (the id is
    // mandatory, so an inbox task gets one too). It is read inside the same `BEGIN IMMEDIATE` as the write
    // (`next_id`) — read it outside and a concurrent writer takes the same id.
    let id = read::next_id(tx.conn(), "task")?;

    // With a project, allocate an `order_key` and put it on the task's own columns; without one it is an
    // inbox task (all `None`). The sibling ordering is computed before the task is inserted (the new task
    // does not count as its own sibling).
    let (project_id, order_key) = match input.project_id {
        Some(proj) => {
            let sibs = read::placement_siblings(tx.conn(), proj, None)?;
            let key = place(&sibs, &Position::Bottom)?;
            (Some(proj), Some(key))
        }
        None => (None, None),
    };

    let task = Task {
        id,
        title: input.title,
        notes: input.notes,
        subtype: Subtype::Default,
        completed_at: None,
        status: TaskStatus::Todo,
        created_by_kind: input.created_by_kind,
        assignee_kind: None,
        start_on: input.start_on,
        due_on: input.due_on,
        priority: input.priority,
        project_id,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::task(&task))?;
    Ok(task)
}

#[derive(Default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub due_on: Option<NaiveDate>,
    pub start_on: Option<NaiveDate>,
    pub priority: Option<Priority>,
    pub clear_due: bool,
    pub clear_priority: bool,
    pub clear_start: bool,
}

/// Read a task's `before` snapshot **from this transaction**. not_found if it does not exist (every mutation
/// targets a live row and nothing else).
fn live_before(tx: &WriteTx<'_>, id: i64) -> Result<Task> {
    read::task(tx.conn(), id)?.ok_or_else(|| NOUN.not_found(id.to_string()))
}

pub fn update(tx: &WriteTx<'_>, id: i64, patch: TaskPatch) -> Result<Task> {
    let before = live_before(tx, id)?;
    let mut t = before.clone();
    if let Some(title) = patch.title {
        if title.trim().is_empty() {
            return Err(Error::invalid("a task title cannot be empty", "タスクのタイトルは空にできません"));
        }
        t.title = title;
    }
    if let Some(notes) = patch.notes {
        t.notes = notes;
    }
    if patch.clear_due {
        t.due_on = None;
    } else if let Some(d) = patch.due_on {
        t.due_on = Some(d);
    }
    if patch.clear_start {
        t.start_on = None;
    } else if let Some(d) = patch.start_on {
        t.start_on = Some(d);
    }
    if patch.clear_priority {
        t.priority = None;
    } else if let Some(p) = patch.priority {
        t.priority = Some(p);
    }
    t.updated_at = Timestamp::now();
    emit_update(tx, record::task(&before), record::task(&t))?;
    Ok(t)
}

/// Assign an owner, or take one away. In a single local store the assignee is a facet and nothing more:
/// `Some(Human)` means it is for the human, `Some(Ai)` means it is for "that person's AI" (me-ai), and
/// `None` means unassigned.
pub fn set_assignee(tx: &WriteTx<'_>, id: i64, kind: Option<ActorKind>) -> Result<Task> {
    let before = live_before(tx, id)?;
    let after = Task { assignee_kind: kind, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::task(&before), record::task(&after))?;
    Ok(after)
}

pub fn set_completed(tx: &WriteTx<'_>, id: i64, completed: bool) -> Result<Task> {
    set_status(tx, id, if completed { TaskStatus::Done } else { TaskStatus::Todo })
}

/// How an error body names its subject (the conversational reference `#12`). Read from the source of truth,
/// like everything else a guard judges on — if it cannot be read, quote the argument as given.
fn subject(tx: &WriteTx<'_>, id: i64) -> String {
    read::task(tx.conn(), id)
        .ok()
        .flatten()
        .as_ref()
        .map(crate::view::display_ref)
        .unwrap_or_else(|| format!("'{id}'"))
}

/// Build the body of a `not_ready` error: one code, with only the reason varying. For a premise that has been
/// overruled, the advice keys off **whether a successor exists**, not off status: currency lives in a derived
/// projection, so only the successor lets us say "relink it" about a decision that is still `accepted` yet no
/// longer current.
fn not_ready(subject: &str, blockers: &[ReserveBlocker]) -> Error {
    let mut en = Vec::new();
    let mut ja = Vec::new();
    for b in blockers {
        match b {
            ReserveBlocker::OpenBlocker { label } => {
                en.push(format!("blocker {label} is not done"));
                ja.push(format!("先行タスク {label} が未完了です"));
            }
            ReserveBlocker::UnsettledPremise { label, superseded_by: Some(succ), .. } => {
                en.push(format!("premise {label} was superseded by {succ} — relink it"));
                ja.push(format!("根拠 {label} は {succ} に置き換わっています。link を張り替えてください"));
            }
            ReserveBlocker::UnsettledPremise { label, status, superseded_by: None } => match status {
                DecisionStatus::Rejected => {
                    en.push(format!("premise {label} was rejected — the task needs rethinking"));
                    ja.push(format!("根拠 {label} は却下されています。タスクの見直しが必要です"));
                }
                // `proposed` (not settled). "It was superseded" is caught by the arm above (successor
                // present) — currency is a derived projection and never surfaces in status, so a premise that
                // is no longer current with no successor cannot reach here. Nor can `accepted`
                // (`reserve_blockers` judges on `unsettled_premise`, which lets an accepted premise
                // that nothing supersedes through).
                DecisionStatus::Proposed | DecisionStatus::Accepted => {
                    en.push(format!("premise {label} is not settled — wait for the ruling, or unlink it"));
                    ja.push(format!("根拠 {label} が未確定です。裁定を待つか link を外してください"));
                }
            },
        }
    }
    Error::not_ready(
        format!("cannot reserve task {subject}: {}", en.join("; ")),
        format!("タスク {subject} を予約できません: {}", ja.join("・")),
    )
}

/// Set the status. `status` is the single source of truth for completion, and whether a task is done is
/// derived from `status == Done` ([`Task::completed`]). `completed_at` carries a value only while done, and
/// is `None` otherwise. Reserving (`→ in_progress`) is a **compare-and-swap**: it succeeds only if the task
/// is currently `todo` (i.e. a conditional update with `WHERE status='todo'`). Reserving from anything else
/// — already `in_progress` (a second session starting the same task), or `blocked`/`done` — is rejected with
/// [`Error::AlreadyReserved`]. The only thing it judges on is whether the source state is `todo`; who the
/// reserver is never enters into it. A reservation always goes via `todo` (from `blocked`/`done` you return
/// to `todo` first). The other transitions (`→ todo` / `→ blocked` / `→ done`) are unconditional, and
/// idempotent re-setting is the caller's business. A reservation **also demands `ready`**: a task with an
/// unfinished blocker left on it, or with a decision linked as a premise that is not settled, is rejected
/// with [`Error::NotReady`]. That is evaluated after the CAS, so "another session holds it" and "its premises
/// are unmet" arrive as different errors. There is no `--force`, and no actor is exempt (the coherence of a
/// declaration does not depend on who declares it). The check fires **on the transition only** — a dependency
/// added to an already `in_progress` task does not strip its status. **Both reads come from the source of
/// truth**: [`read::task_status`] and [`read::reserve_blockers`] are read **inside the same transaction** as
/// the UPDATE, so no other writer can slip between them — a reservation another process holds, or a blocker
/// it has finished, is already visible to this judgement.
pub fn set_status(tx: &WriteTx<'_>, id: i64, status: TaskStatus) -> Result<Task> {
    let now = Timestamp::now();
    // The CAS guard for the reserve transition: nothing but `todo` may move to `in_progress`.
    let current =
        read::task_status(tx.conn(), id)?.ok_or_else(|| NOUN.not_found(id.to_string()))?;
    if status == TaskStatus::InProgress && current != TaskStatus::Todo {
        let (cur, subject) = (current.as_str(), subject(tx, id));
        return Err(Error::already_reserved(
            format!("cannot reserve task {subject}: it is '{cur}', not 'todo' (reserve is todo → in_progress; another session may hold it)"),
            format!("タスク {subject} を予約できません: 現在 '{cur}' です（予約は todo → in_progress のみ・別セッションが着手済みの可能性）"),
        ));
    }
    // A reservation only goes through once the declared premises hold: no open blocker, and every decision
    // linked as a premise settled.
    if status == TaskStatus::InProgress {
        let blockers = read::reserve_blockers(tx.conn(), id)?;
        if !blockers.is_empty() {
            return Err(not_ready(&subject(tx, id), &blockers));
        }
    }
    let before = live_before(tx, id)?;
    let after = Task {
        status,
        completed_at: (status == TaskStatus::Done).then_some(now),
        updated_at: now,
        ..before.clone()
    };
    emit_update(tx, record::task(&before), record::task(&after))?;
    Ok(after)
}

/// Hard-delete a task. Placement (project / order_key) lives on the task's own columns, so it goes with the row.
pub fn delete(tx: &WriteTx<'_>, id: i64) -> Result<Vec<String>> {
    let before = live_before(tx, id)?;
    delete_subtree(tx, before.id)
}

/// Hard-delete one task and its children (the id must already be known to exist). This is the body of
/// [`delete`], and [`crate::ops::project::delete`] uses it to remove a project's tasks, so deleting a project
/// runs the same sweep. The schema's `CASCADE` takes `task_comment`, `task_dependency` (both the `task_id`
/// and the `blocked_by_id` side), `task_dimension_value` and `decision_task_link`. All the delete op sweeps
/// itself is the polymorphic children no constraint can cover — the task's own attachments, plus the
/// attachments hanging off the comments that `CASCADE` is about to remove (swept before those comments,
/// because once the parent row is gone nobody can find them any more). Returns the blob hashes this subtree
/// let go of — the candidates for reclamation after commit.
pub(crate) fn delete_subtree(tx: &WriteTx<'_>, id: i64) -> Result<Vec<String>> {
    let mut orphaned = Vec::new();
    for comment_id in read::task_comment_ids(tx.conn(), id)? {
        orphaned.extend(crate::ops::sweep_polymorphic(tx, "task_comment", comment_id)?);
    }
    orphaned.extend(crate::ops::sweep_polymorphic(tx, "task", id)?);
    tx.delete_record("task", id)?;
    Ok(orphaned)
}

/// Rehome (change project) and reorder. **Read-then-write** (scan the siblings' `order_key`s, then write the
/// new key). All three reads — liveness, current project, siblings — happen in the same transaction as the
/// write. Placement is by project only, and is held on the task itself.
pub fn move_to(
    tx: &WriteTx<'_>,
    id: i64,
    target_project: Option<i64>,
    pos: Position,
) -> Result<Task> {
    let before = live_before(tx, id)?;

    let proj = target_project
        .or(before.project_id)
        .ok_or_else(|| Error::invalid("a target project is required (specify --project)", "配置先プロジェクトが必要です（--project を指定してください）"))?;

    // An edge that was legal within one project becomes a crossing as soon as one of its ends moves. A guard
    // at creation time alone cannot hold the invariant, so re-check the edges against the destination. If one
    // of them would cross, detaching it before the move is the caller's call — cutting it silently would make
    // dependencies and decision links disappear without a sound.
    if read::edge_peer_projects(tx.conn(), id)?
        .into_iter()
        .any(|peer| crate::ops::crosses_projects(Some(proj), peer))
    {
        return Err(Error::invalid(
            "moving this task there would leave an edge crossing projects — detach its dependencies / decision links first",
            "このタスクをそこへ移すと、プロジェクトを跨ぐエッジが残ります ── 先に依存／決定リンクを外してください",
        ));
    }

    let sibs = read::placement_siblings(tx.conn(), proj, Some(id))?;
    let key = place(&sibs, &pos)?;

    let after = Task {
        project_id: Some(proj),
        order_key: Some(key),
        updated_at: Timestamp::now(),
        ..before.clone()
    };
    emit_update(tx, record::task(&before), record::task(&after))?;
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, mk_task, mk_task_in, with_tx};
    use crate::store_engine::WriteTx;

    // --- Reserving (todo → in_progress): the compare-and-swap ---

    /// Prepare one inbox task to reserve, and hand it to `f`.
    fn with_task(f: impl FnOnce(&WriteTx<'_>, i64)) {
        with_tx(|tx| {
            let tid = mk_task(tx, "claim me");
            f(tx, tid);
        });
    }

    /// The current status, read from the source of truth.
    fn status_of(tx: &WriteTx<'_>, id: i64) -> TaskStatus {
        read::task_status(tx.conn(), id).unwrap().expect("the task is live")
    }

    #[test]
    fn reserving_a_todo_succeeds() {
        with_task(|tx, tid| {
            let t = set_status(tx, tid, TaskStatus::InProgress).unwrap();
            assert_eq!(t.status, TaskStatus::InProgress);
        });
    }

    #[test]
    fn double_reserve_is_rejected_as_already_reserved() {
        // Two reservations in a row on the same todo: the first goes through, the second is rejected as
        // AlreadyReserved (this is what stops two sessions starting the same task).
        with_task(|tx, tid| {
            set_status(tx, tid, TaskStatus::InProgress).unwrap();
            let err = set_status(tx, tid, TaskStatus::InProgress).unwrap_err();
            assert_eq!(err.code(), "already_reserved");
            // The state stays in_progress — a rejection never regresses it.
            assert_eq!(status_of(tx, tid), TaskStatus::InProgress);
        });
    }

    #[test]
    fn releasing_to_todo_then_reserving_again_succeeds() {
        // Letting go back to `todo`, then reserving again, goes through.
        with_task(|tx, tid| {
            set_status(tx, tid, TaskStatus::InProgress).unwrap();
            set_status(tx, tid, TaskStatus::Todo).unwrap();
            let t = set_status(tx, tid, TaskStatus::InProgress).unwrap();
            assert_eq!(t.status, TaskStatus::InProgress);
        });
    }

    #[test]
    fn reserving_from_blocked_or_done_is_rejected() {
        // A reservation always goes via `todo`. Reserving straight from `blocked`/`done` is rejected.
        for from in [TaskStatus::Blocked, TaskStatus::Done] {
            with_task(|tx, tid| {
                set_status(tx, tid, from).unwrap();
                let err = set_status(tx, tid, TaskStatus::InProgress).unwrap_err();
                assert_eq!(err.code(), "already_reserved", "reserve from {from:?} must be rejected");
                assert_eq!(status_of(tx, tid), from, "status unchanged on rejection");
            });
        }
    }

    #[test]
    fn non_reserve_transitions_are_unconditional() {
        // The other transitions (→ blocked / → done / → todo) are untouched by the CAS and never regress.
        with_task(|tx, tid| {
            assert_eq!(set_status(tx, tid, TaskStatus::Blocked).unwrap().status, TaskStatus::Blocked);
            assert_eq!(set_status(tx, tid, TaskStatus::Done).unwrap().status, TaskStatus::Done);
            assert!(set_status(tx, tid, TaskStatus::Done).unwrap().completed_at.is_some());
            assert_eq!(set_status(tx, tid, TaskStatus::Todo).unwrap().status, TaskStatus::Todo);
            // done → todo drops completed_at.
            assert!(read::task(tx.conn(), tid).unwrap().unwrap().completed_at.is_none());
        });
    }

    // --- A reservation demands ready (not_ready, and there is no force) ---

    /// Prepare one project with one numbered task in it (`#1`), and hand them to `f`.
    fn with_numbered_task(f: impl FnOnce(&WriteTx<'_>, i64, i64)) {
        with_tx(|tx| {
            let pid = mk_project(tx, "PJ");
            let tid = mk_task_in(tx, "reserve me", Some(pid));
            f(tx, pid, tid);
        });
    }

    fn new_decision(tx: &WriteTx<'_>, project_id: i64, title: &str) -> i64 {
        crate::ops::decision::add(
            tx,
            crate::ops::decision::NewDecision {
                title: title.to_string(),
                body: String::new(),
                project_id,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn reserving_a_task_with_an_open_blocker_is_rejected_as_not_ready() {
        with_numbered_task(|tx, pid, tid| {
            let blocker = mk_task_in(tx, "do me first", Some(pid));
            crate::ops::dependency::add(tx, tid, blocker, None).unwrap();

            let err = set_status(tx, tid, TaskStatus::InProgress).unwrap_err();
            assert_eq!(err.code(), "not_ready");
            assert!(err.message_en().contains("blocker AMB-T-2 is not done"), "{}", err.message_en());
            assert!(err.to_string().contains("先行タスク AMB-T-2 が未完了です"), "{err}");
            assert_eq!(status_of(tx, tid), TaskStatus::Todo, "rejected, and the status has not moved");

            // Once the blocker is done the premise holds, and the reservation goes through.
            set_status(tx, blocker, TaskStatus::Done).unwrap();
            assert_eq!(set_status(tx, tid, TaskStatus::InProgress).unwrap().status, TaskStatus::InProgress);
        });
    }

    #[test]
    fn reserving_a_task_whose_premise_is_unsettled_names_the_reason() {
        // A decision that is not alive as a premise points at a different way out for each state (under one
        // code).
        with_numbered_task(|tx, pid, tid| {
            // proposed: wait for the ruling, or unlink it.
            let proposed = new_decision(tx, pid, "まだ議論中");
            crate::ops::decision::link(tx, proposed, tid).unwrap();
            let err = set_status(tx, tid, TaskStatus::InProgress).unwrap_err();
            assert_eq!(err.code(), "not_ready");
            assert!(err.message_en().contains("premise AMB-D-1 is not settled"), "{}", err.message_en());
            assert!(err.to_string().contains("根拠 AMB-D-1 が未確定です"), "{err}");

            // accepted: the premise is alive, so the reservation goes through.
            crate::ops::decision::accept(tx, proposed, None).unwrap();
            set_status(tx, tid, TaskStatus::InProgress).unwrap();
            set_status(tx, tid, TaskStatus::Todo).unwrap();

            // superseded: tell them to relink to the successor.
            let successor = new_decision(tx, pid, "置き換える決定");
            crate::ops::decision::supersede(tx, successor, proposed, None).unwrap();
            let err = set_status(tx, tid, TaskStatus::InProgress).unwrap_err();
            assert_eq!(err.code(), "not_ready");
            assert!(err.message_en().contains("premise AMB-D-1 was superseded by AMB-D-2"), "{}", err.message_en());
            assert!(err.to_string().contains("根拠 AMB-D-1 は AMB-D-2 に置き換わっています"), "{err}");

            // rejected: the task itself needs rethinking.
            crate::ops::decision::unlink(tx, proposed, tid).unwrap();
            let rejected = new_decision(tx, pid, "却下された案");
            crate::ops::decision::reject(tx, rejected).unwrap();
            crate::ops::decision::link(tx, rejected, tid).unwrap();
            let err = set_status(tx, tid, TaskStatus::InProgress).unwrap_err();
            assert!(err.message_en().contains("premise AMB-D-3 was rejected"), "{}", err.message_en());
            assert!(err.to_string().contains("根拠 AMB-D-3 は却下されています"), "{err}");

            // Unlink the premise and the task is startable at once — that is the way out.
            crate::ops::decision::unlink(tx, rejected, tid).unwrap();
            assert_eq!(set_status(tx, tid, TaskStatus::InProgress).unwrap().status, TaskStatus::InProgress);
        });
    }

    #[test]
    fn the_cas_is_evaluated_before_the_ready_guard() {
        // "Another session holds it" and "its premises are unmet" are different failures. When both hold, it
        // is the CAS that fires.
        with_numbered_task(|tx, pid, tid| {
            set_status(tx, tid, TaskStatus::InProgress).unwrap();
            let blocker = mk_task_in(tx, "do me first", Some(pid));
            crate::ops::dependency::add(tx, tid, blocker, None).unwrap();

            let err = set_status(tx, tid, TaskStatus::InProgress).unwrap_err();
            assert_eq!(err.code(), "already_reserved", "re-reserving an in_progress task is what the CAS rejects");
        });
    }

    #[test]
    fn the_ready_guard_only_fires_on_the_reserve_transition() {
        // The check fires on the transition only. A dependency added to a task already under way does not
        // strip its reservation, and `→ todo` / `→ blocked` / `→ done` go through whatever the premises say
        // (the way to let a task go is never blocked).
        with_numbered_task(|tx, pid, tid| {
            set_status(tx, tid, TaskStatus::InProgress).unwrap();
            let blocker = mk_task_in(tx, "do me first", Some(pid));
            crate::ops::dependency::add(tx, tid, blocker, None).unwrap();

            assert_eq!(status_of(tx, tid), TaskStatus::InProgress, "a dependency added after the fact does not strip the reservation");
            assert_eq!(set_status(tx, tid, TaskStatus::Todo).unwrap().status, TaskStatus::Todo);
            assert_eq!(set_status(tx, tid, TaskStatus::Blocked).unwrap().status, TaskStatus::Blocked);
            assert_eq!(set_status(tx, tid, TaskStatus::Done).unwrap().status, TaskStatus::Done);
        });
    }

    #[test]
    fn the_reserve_guard_and_the_ready_filter_share_one_predicate() {
        // Don't let the reserve guard and the mailbox filter drift into an asymmetry by running on different
        // predicates. Both sides ask the source-of-truth SQL — the mailbox through its `ready:yes` filter,
        // the reserve guard through `reserve_blockers`.
        with_numbered_task(|tx, pid, tid| {
            let blocker = mk_task_in(tx, "do me first", Some(pid));
            let premise = new_decision(tx, pid, "まだ議論中");
            let ready_in_mailbox = |tx: &WriteTx<'_>| {
                crate::query::list(
                    tx.conn(),
                    crate::reach::Reach::All,
                    crate::query::ListParams {
                        project_id: None,
                        filter_expr: Some("ready:yes".to_string()),
                        sort: "created".to_string(),
                        limit: None,
                        offset: None,
                    },
                )
                .unwrap()
                .tasks
                .iter()
                .any(|t| t.id == tid)
            };
            let check = |tx: &WriteTx<'_>| {
                assert_eq!(
                    read::reserve_blockers(tx.conn(), tid).unwrap().is_empty(),
                    ready_in_mailbox(tx),
                    "reserve_blockers が空 ⇔ mailbox の ready:yes に出る"
                );
            };
            check(tx);
            crate::ops::dependency::add(tx, tid, blocker, None).unwrap();
            check(tx);
            crate::ops::decision::link(tx, premise, tid).unwrap();
            check(tx);
            set_status(tx, blocker, TaskStatus::Done).unwrap();
            check(tx);
            crate::ops::decision::accept(tx, premise, None).unwrap();
            check(tx);
            assert!(read::reserve_blockers(tx.conn(), tid).unwrap().is_empty());
        });
    }

    #[test]
    fn a_start_day_still_ahead_holds_the_task_back_on_every_read() {
        // The third premise: a task declared to start later is not ready yet. Every read that projects
        // `ready` has to say so — the task card, the task detail, and the `ready:` filter that SQL
        // restates — or the mailbox and `task show` would disagree about the same task.
        with_numbered_task(|tx, _pid, tid| {
            let today = crate::time::today();
            let set_start = |tx: &WriteTx<'_>, d: Option<NaiveDate>| {
                update(
                    tx,
                    tid,
                    TaskPatch { start_on: d, clear_start: d.is_none(), ..TaskPatch::default() },
                )
                .unwrap();
            };
            // The card path (`task list`), the detail path (`task show`), and the filter path.
            let card = |tx: &WriteTx<'_>| {
                crate::query::list(tx.conn(), crate::reach::Reach::All, list_params(None))
                    .unwrap()
                    .tasks
                    .into_iter()
                    .find(|t| t.id == tid)
                    .expect("the task is listed whatever its start day")
                    .ready
            };
            let detail = |tx: &WriteTx<'_>| crate::query::task_detail(tx.conn(), tid).unwrap().ready;
            let in_mailbox = |tx: &WriteTx<'_>| {
                crate::query::list(
                    tx.conn(),
                    crate::reach::Reach::All,
                    list_params(Some("ready:yes")),
                )
                .unwrap()
                .tasks
                .iter()
                .any(|t| t.id == tid)
            };

            for (start, want, what) in [
                (None, true, "no start day declared"),
                (Some(today - chrono::Duration::days(1)), true, "started yesterday"),
                (Some(today), true, "starts today"),
                (Some(today + chrono::Duration::days(1)), false, "starts tomorrow"),
            ] {
                set_start(tx, start);
                assert_eq!(card(tx), want, "card: {what}");
                assert_eq!(detail(tx), want, "detail: {what}");
                assert_eq!(in_mailbox(tx), want, "ready:yes filter: {what}");
            }
        });
    }

    #[test]
    fn a_task_held_back_by_its_start_day_carries_the_date_it_waits_for() {
        // A `ready: false` with nothing to point at is the failure this guards: a plain `task list` shows
        // every task, so the one the mailbox skips has to say *why* on the spot. The reason travels beside
        // `blocked_by_open` / `blocked_by_decisions` on both faces, and it is present exactly when the
        // start day is the thing holding the task back — never when the task is ready.
        with_numbered_task(|tx, _pid, tid| {
            let today = crate::time::today();
            let tomorrow = today + chrono::Duration::days(1);
            let card = |tx: &WriteTx<'_>| {
                crate::query::list(tx.conn(), crate::reach::Reach::All, list_params(None))
                    .unwrap()
                    .tasks
                    .into_iter()
                    .find(|t| t.id == tid)
                    .expect("the task is listed whatever its start day")
            };
            let detail = |tx: &WriteTx<'_>| crate::query::task_detail(tx.conn(), tid).unwrap();

            for (start, want, what) in [
                (None, None, "no start day declared"),
                (Some(today - chrono::Duration::days(1)), None, "started yesterday"),
                (Some(today), None, "starts today"),
                (Some(tomorrow), Some(tomorrow), "starts tomorrow"),
            ] {
                update(
                    tx,
                    tid,
                    TaskPatch {
                        start_on: start,
                        clear_start: start.is_none(),
                        ..TaskPatch::default()
                    },
                )
                .unwrap();
                let (card, detail) = (card(tx), detail(tx));
                assert_eq!(card.not_started_until, want, "card: {what}");
                assert_eq!(detail.not_started_until, want, "detail: {what}");
                // The reason and the verdict are one derivation: a reason present means not ready, and a
                // task that is not ready for want of a start day always has one.
                assert_eq!(card.ready, want.is_none(), "card ready: {what}");
                assert_eq!(detail.ready, want.is_none(), "detail ready: {what}");
            }
        });
    }

    /// `ListParams` for a whole-store read, optionally filtered — the three ready paths only differ in
    /// the filter, so the rest is spelled once.
    fn list_params(filter: Option<&str>) -> crate::query::ListParams {
        crate::query::ListParams {
            project_id: None,
            filter_expr: filter.map(str::to_string),
            sort: "created".to_string(),
            limit: None,
            offset: None,
        }
    }

    // --- The crossing guard on rehoming (the three arms of `read::edge_peer_projects`) ---

    /// Create one decision and link it to `task` (this builds the far side of the third arm — the decision
    /// link).
    fn link_decision(tx: &WriteTx<'_>, project_id: i64, task_id: i64) {
        let d = crate::ops::decision::add(
            tx,
            crate::ops::decision::NewDecision {
                title: "前提".to_string(),
                body: String::new(),
                project_id,
            },
        )
        .unwrap();
        crate::ops::decision::link(tx, d.id, task_id).unwrap();
    }

    /// A move that leaves the far side of an edge behind is refused. Dependencies are checked in **both
    /// directions** — whether the depending task moves or the depended-on one does, what is left behind is an
    /// edge across projects. Decision links are no different.
    #[test]
    fn moving_a_task_away_from_its_edges_is_refused() {
        with_tx(|tx| {
            let alpha = mk_project(tx, "alpha");
            let beta = mk_project(tx, "beta");
            let a = mk_task_in(tx, "a", Some(alpha));
            let b = mk_task_in(tx, "b", Some(alpha));
            let c = mk_task_in(tx, "c", Some(alpha));
            crate::ops::dependency::add(tx, a, b, None).unwrap(); // a depends on b
            link_decision(tx, alpha, c);

            for (id, why) in [(a, "the depending side"), (b, "the depended-on side"), (c, "the decision-link side")] {
                let err = move_to(tx, id, Some(beta), Position::Bottom).unwrap_err();
                assert_eq!(err.code(), "invalid_value", "moving {why} would leave an edge across projects");
                assert_eq!(
                    read::task(tx.conn(), id).unwrap().unwrap().project_id,
                    Some(alpha),
                    "a refused move does not change what it belongs to"
                );
            }
        });
    }

    /// A task with no edges moves — what the guard looks at is the far side of an edge, not the move itself.
    #[test]
    fn moving_a_task_with_no_edges_is_allowed() {
        with_tx(|tx| {
            let alpha = mk_project(tx, "alpha");
            let beta = mk_project(tx, "beta");
            let free = mk_task_in(tx, "free", Some(alpha));

            let moved = move_to(tx, free, Some(beta), Position::Bottom).unwrap();
            assert_eq!(moved.project_id, Some(beta));
        });
    }

    #[test]
    fn delete_takes_the_tasks_comments_with_it() {
        // Deletion is a hard delete. Comments go with the row via `ON DELETE CASCADE` on
        // `task_comment.task_id` — not one orphan is left behind.
        with_tx(|tx| {
            let tid = mk_task(tx, "消えるタスク");
            crate::ops::comment::add_comment(tx, tid, ActorKind::Ai, "コメント").unwrap();
            let survivor = mk_task(tx, "残るタスク");
            crate::ops::comment::add_comment(tx, survivor, ActorKind::Ai, "残るコメント").unwrap();

            delete(tx, tid).unwrap();

            assert!(read::task(tx.conn(), tid).unwrap().is_none(), "the task's row itself goes");
            assert!(read::comment_list(tx.conn(), tid).unwrap().is_empty(), "the comments go with it, by cascade");
            // The surviving task's comment is untouched, and no orphaned row is left (that one row is all the
            // table holds).
            assert_eq!(read::comment_list(tx.conn(), survivor).unwrap().len(), 1);
            assert_eq!(read::all_task_comments(tx.conn()).unwrap().len(), 1, "deletion leaves no orphans");
        });
    }

    // --- Conversational numbering (globally unique on the device) ---

    /// A task's conversational number — its id. `None` if the row is gone.
    fn number_of(tx: &WriteTx<'_>, task_id: i64) -> Option<i64> {
        read::task(tx.conn(), task_id).unwrap().map(|t| t.id)
    }

    #[test]
    fn numbering_is_dense_within_a_project() {
        with_tx(|tx| {
            let p = mk_project(tx, "amenbo 開発");
            let a = mk_task_in(tx, "1", Some(p));
            let b = mk_task_in(tx, "2", Some(p));
            let c = mk_task_in(tx, "3", Some(p));
            assert_eq!((number_of(tx, a), number_of(tx, b), number_of(tx, c)), (Some(1), Some(2), Some(3)));
        });
    }

    #[test]
    fn numbering_never_reuses_a_deleted_number() {
        with_tx(|tx| {
            let p = mk_project(tx, "amenbo 開発");
            let a = mk_task_in(tx, "1", Some(p)); // #1
            let _b = mk_task_in(tx, "2", Some(p)); // #2
            delete(tx, a).unwrap(); // delete #1
            // Deleted rows still count toward the max, so the next one is #3: #1 is never handed out again,
            // and the sequence is allowed to have holes.
            let c = mk_task_in(tx, "3", Some(p));
            assert_eq!(number_of(tx, c), Some(3));
        });
    }

    #[test]
    fn numbering_is_global_across_projects() {
        with_tx(|tx| {
            // Numbers come from one sequence per device; crossing a project does not restart it.
            let p1 = mk_project(tx, "alpha");
            let p2 = mk_project(tx, "beta");
            let x = mk_task_in(tx, "x", Some(p1));
            let y = mk_task_in(tx, "y", Some(p2));
            let z = mk_task_in(tx, "z", Some(p1));
            assert_eq!((number_of(tx, x), number_of(tx, y), number_of(tx, z)), (Some(1), Some(2), Some(3)));
        });
    }

    /// An inbox task is numbered like any other: the number *is* the key (`task.id`), a row without a
    /// key cannot exist, and the number comes from the one global sequence.
    #[test]
    fn inbox_task_is_numbered_like_any_other() {
        with_tx(|tx| {
            let id = mk_task(tx, "no project");
            let t = read::task(tx.conn(), id).unwrap().unwrap();
            assert_eq!(t.id, 1, "the inbox task takes #1");
            assert_eq!(t.id, id, "the number is the key");
            assert!(t.project_id.is_none(), "and it stays in the inbox all the same (belonging to no project)");
        });
    }

    // --- Reference resolution (T-n / D-n / #n / n) ---

    /// References are resolved through the same SQL path as in production
    /// ([`crate::query::resolve_task_ref`]).
    fn resolve_ref(tx: &WriteTx<'_>, input: &str) -> Result<i64> {
        crate::query::resolve_task_ref(tx.conn(), input)
    }


    /// `KEY-n` is not part of the reference grammar — silently dropping an arbitrary prefix would turn
    /// `ZZZ-1` into `#1`, so it is rejected.
    #[test]
    fn resolve_ref_refuses_a_prefixed_number() {
        with_tx(|tx| {
            let p = mk_project(tx, "amenbo 開発");
            let _t1 = mk_task_in(tx, "a", Some(p)); // #1
            assert!(resolve_ref(tx, "AMENBO-1").is_err());
            assert!(resolve_ref(tx, "ZZZ-1").is_err());
        });
    }

    #[test]
    fn resolve_ref_accepts_hash_and_bare_number() {
        with_tx(|tx| {
            let p = mk_project(tx, "amenbo 開発");
            let t = mk_task_in(tx, "a", Some(p)); // #1
            assert_eq!(resolve_ref(tx, "#1").unwrap(), t);
            assert_eq!(resolve_ref(tx, "1").unwrap(), t);
            // A number that does not exist is not_found.
            assert!(resolve_ref(tx, "#2").is_err());
        });
    }

    #[test]
    fn resolve_ref_bare_number_is_global() {
        with_tx(|tx| {
            // Numbers are unique across the whole store, so they resolve uniquely with no project context and
            // never turn ambiguous.
            let p1 = mk_project(tx, "alpha");
            let p2 = mk_project(tx, "beta");
            let a1 = mk_task_in(tx, "a", Some(p1)); // #1
            let a2 = mk_task_in(tx, "b", Some(p1)); // #2
            let b1 = mk_task_in(tx, "c", Some(p2)); // #3
            assert_eq!(resolve_ref(tx, "#1").unwrap(), a1);
            assert_eq!(resolve_ref(tx, "#2").unwrap(), a2);
            assert_eq!(resolve_ref(tx, "#3").unwrap(), b1);
        });
    }

    /// The bare key resolves. The number *is* the key, so this is the *same* input as `#1`.
    #[test]
    fn resolve_ref_takes_the_bare_key() {
        with_tx(|tx| {
            let tid = mk_task(tx, "claim me"); // an inbox task is numbered too
            assert_eq!(resolve_ref(tx, &tid.to_string()).unwrap(), tid);
            assert_eq!(resolve_ref(tx, &format!("#{tid}")).unwrap(), tid, "it points at the same row");
        });
    }

    #[test]
    fn resolve_ref_refuses_a_deleted_number() {
        with_tx(|tx| {
            let p = mk_project(tx, "amenbo 開発");
            let t = mk_task_in(tx, "a", Some(p)); // #1
            delete(tx, t).unwrap();
            // A deleted task's number no longer resolves.
            assert!(resolve_ref(tx, "#1").is_err());
        });
    }

    #[test]
    fn parse_helpers_discriminate_forms() {
        assert_eq!(parse_number_ref("#12"), Some(12));
        assert_eq!(parse_number_ref("12"), Some(12));
        assert_eq!(parse_number_ref("#"), None);
        assert_eq!(parse_number_ref("01KVM"), None); // letters mixed in: not a number
        // The type prefixes T-/D- (case-insensitive). Any other single-letter key is not a type prefix.
        assert_eq!(parse_typed_ref("T-12"), Some((TypedKind::Task, 12)));
        assert_eq!(parse_typed_ref("d-3"), Some((TypedKind::Decision, 3)));
        assert_eq!(parse_typed_ref("X-1"), None); // anything but T/D is not a type prefix
        assert_eq!(parse_typed_ref("AMENBO-1"), None); // a multi-character key is not a type prefix
        assert_eq!(parse_typed_ref("T-"), None); // no number
        // The namespaced form amenbo renders reads back, so a ref pastes in off the screen.
        assert_eq!(parse_typed_ref(&crate::idref::task(12)), Some((TypedKind::Task, 12)));
        assert_eq!(parse_typed_ref(&crate::idref::decision(3)), Some((TypedKind::Decision, 3)));
        assert_eq!(parse_typed_ref("amb-t-7"), Some((TypedKind::Task, 7))); // the prefix folds case
        assert_eq!(parse_typed_ref("AMB-X-1"), None); // the namespace does not make X a type prefix
        assert_eq!(parse_typed_ref("AMB-P-1"), None); // a project is not one of the two number spaces
    }

    #[test]
    fn resolve_ref_honors_type_prefix() {
        with_tx(|tx| {
            let p = mk_project(tx, "amenbo 開発");
            let t = mk_task_in(tx, "a", Some(p)); // task #1
            // `AMB-T-1` names the task explicitly, as does the bare `T-1`; `#1` and `1` still mean the task.
            assert_eq!(resolve_ref(tx, "AMB-T-1").unwrap(), t);
            assert_eq!(resolve_ref(tx, "T-1").unwrap(), t);
            assert_eq!(resolve_ref(tx, "#1").unwrap(), t);
            // `AMB-D-1` names a decision, so it is not found as a task.
            assert!(resolve_ref(tx, "AMB-D-1").is_err());
            assert!(resolve_ref(tx, "D-1").is_err());
        });
    }
}
