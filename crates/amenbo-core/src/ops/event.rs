//! Plugin observation events — the outbox emit half of the write path (`AMB-D-367`).
//!
//! The ops write function composes the semantic event it alone can name — it knows the operation intent
//! (its own name says status / assigned / moved / accepted / rejected), the actor, and the new state — and
//! appends it to the plugin outbox inside the mutation's own transaction, through `WriteTx::emit_event`.
//! Generation is leak-free (a rollback drops the event with the write it described); delivery is a later,
//! best-effort concern handled by the dispatcher. The store interprets none of the strings — it stores
//! the row it is given. Emitting in the write function itself, rather than in the wrapper that called it,
//! is what makes the same event fire whoever writes: the CLI and the GUI through `Store`, and an
//! automation run's own steps, which write through ops without a wrapper in between. The rationale for a
//! log kept separate from the change feed lives in the decision log.

use crate::error::Result;
use crate::store_engine::WriteTx;

/// **The one door every observation event goes through.** Each helper below composes the
/// event it alone can name and hands it here; here is where the row is finished — with the project the
/// event happened in ([`project_of`]) — and appended.
///
/// One door is the whole point (`AMB-D-405`): the project is a field no write point should have to
/// remember, and a dozen call sites each stamping their own is a dozen chances to forget one, which reads
/// downstream as "that plugin was not subscribed" rather than as a bug.
fn emit(
    tx: &WriteTx<'_>,
    event: &str,
    record_id: i64,
    actor: crate::model::ActorKind,
    at: &str,
    new_state: Option<&str>,
) -> Result<()> {
    tx.emit_event(&crate::store_engine::outbox::EventRow {
        event,
        record_id,
        actor: actor.as_str(),
        at,
        new_state,
        project: project_of(tx, event, record_id)?,
        record: gone_record(tx, event, record_id)?.as_deref(),
        parent: parent_of(tx, event, record_id)?,
    })?;
    Ok(())
}

/// The record the event is about, as JSON, for the events whose record is **gone** by the time anyone
/// reads them (`AMB-D-407`) — read here for the same reason [`project_of`] is: this is the last instant
/// the row exists, and the emit door is where no write point has to remember it.
///
/// `None` on every other event, and that is not a shortcoming: a record still there is read back by name,
/// by the plugin itself (`AMB-D-406`), so carrying it would be a copy that goes stale between the append
/// and the run. `None` again for a deletion whose row cannot be read or whose shape will not serialize —
/// an event that fires without the shape is better than a delete that fails because of a notification.
///
/// The scope is **one record**. A task that takes its comments down with it does not fold them in here:
/// each is its own deletion event, and folding would say the same thing twice in two shapes.
fn gone_record(tx: &WriteTx<'_>, event: &str, record_id: i64) -> Result<Option<String>> {
    use crate::lifecycle::name as ev;
    use crate::store_engine::read;
    let conn = tx.conn();
    let shape = match event {
        ev::TASK_DELETED => read::task(conn, record_id)?.as_ref().and_then(to_json),
        ev::COMMENT_REMOVED => read::task_comment(conn, record_id)?.as_ref().and_then(to_json),
        _ => None,
    };
    Ok(shape)
}

/// The record the event's record **hangs on**, by id (`AMB-D-407`) — the task a comment was posted to, on
/// both of the comment events. Read at the same door as [`project_of`], and for a removal for the same
/// reason: after the `DELETE` there is no row left to ask which task the comment was on.
///
/// A comment that was *added* is still there, and yet it needs the same field, because the read-back that
/// covers every other live record (`AMB-D-406`) has no door onto a comment: a timeline is asked for by
/// task (`comment list <task>`), so a subscriber holding a comment's id has no call that answers whose
/// comment it is. On the wire the two events state one fact — a comment's id does not say where the
/// comment is — so they state it in one field.
///
/// The task events name no parent: a task is read back by its own id, and what it belongs to comes with it.
fn parent_of(tx: &WriteTx<'_>, event: &str, record_id: i64) -> Result<Option<i64>> {
    use crate::lifecycle::name as ev;
    use crate::store_engine::read;
    match event {
        ev::COMMENT_ADDED | ev::COMMENT_REMOVED => {
            Ok(read::task_comment(tx.conn(), record_id)?.map(|c| c.task_id))
        }
        _ => Ok(None),
    }
}

/// One record as the JSON the payload carries, or `None` when it will not serialize. A shape that cannot
/// be written is dropped rather than raised: this is a notification riding along with a deletion, and the
/// deletion is the operation the caller asked for.
fn to_json<T: serde::Serialize>(record: &T) -> Option<String> {
    match serde_json::to_string(record) {
        Ok(json) => Some(json),
        Err(e) => {
            tracing::warn!(error = %e, "a deleted record could not be carried on its event");
            None
        }
    }
}

/// The project the event's record is in, read **inside the emitting transaction, before the operation
/// finishes** (`AMB-D-405`). A task's own, a decision's own, and for a comment the project of the task it
/// hangs on; an event that names no record kind we route on has none.
///
/// Reading it here rather than at delivery is what makes deletions routable at all — the row is still
/// there at this instant and gone by the time anyone delivers — and it is also what keeps a task that
/// moves in between from sending its older events to its new home. `None` is a real answer: a record in no
/// project has no project, and an event stamped `None` reaches only the plugins that are not scoped to one.
fn project_of(tx: &WriteTx<'_>, event: &str, record_id: i64) -> Result<Option<i64>> {
    use crate::lifecycle::name as ev;
    use crate::store_engine::read;
    let conn = tx.conn();
    match event {
        ev::TASK_CREATED
        | ev::TASK_STATUS_CHANGED
        | ev::TASK_DONE
        | ev::TASK_REJECTED
        | ev::TASK_ASSIGNED
        | ev::TASK_MOVED
        | ev::TASK_DELETED => Ok(read::task_project_id(conn, record_id)?),
        ev::DECISION_ACCEPTED | ev::DECISION_REJECTED => Ok(read::decision_project_id(conn, record_id)?),
        // A comment's project is its task's — the comment table holds no project of its own. For a
        // removal that read only answers while the comment is still there, which is why the emit is
        // placed ahead of the `DELETE` (`crate::ops::comment::remove_comment`).
        ev::COMMENT_ADDED | ev::COMMENT_REMOVED => match read::task_comment(conn, record_id)? {
            Some(comment) => Ok(read::task_project_id(conn, comment.task_id)?),
            None => Ok(None),
        },
        _ => Ok(None),
    }
}

/// `task.status_changed` / `task.done` / `task.rejected`: a task's status moved (`AMB-D-367`). Each
/// terminal is its own event — `task.done` for work carried out, `task.rejected` for work decided
/// against (`AMB-D-397`) — and their names are the whole state, so neither carries `new`; every other
/// transition is `task.status_changed` carrying the new status. Without the second specialization, an
/// author subscribing to "the task closed" would have to take `task.done` and then string-match the
/// catch-all for the other half. An idempotent re-set that did not move the status is not a change to
/// observe, so it emits nothing.
pub(super) fn emit_task_status(
    tx: &WriteTx<'_>,
    task: &crate::model::Task,
    before: crate::model::TaskStatus,
    actor: crate::model::ActorKind,
) -> Result<()> {
    if task.status == before {
        return Ok(());
    }
    let at = task.updated_at.to_rfc3339_z();
    let (event, new_state) = match task.status {
        crate::model::TaskStatus::Done => (crate::lifecycle::name::TASK_DONE, None),
        crate::model::TaskStatus::Rejected => (crate::lifecycle::name::TASK_REJECTED, None),
        _ => (crate::lifecycle::name::TASK_STATUS_CHANGED, Some(task.status.as_str())),
    };
    emit(tx, event, task.id, actor, &at, new_state)
}

/// `task.assigned`: a task gained or changed its assignee, carrying the new assignee facet as `new`.
/// Clearing the assignee emits nothing (v1 has no `task.unassigned`), and re-assigning the same facet is
/// not a change to observe.
pub(super) fn emit_task_assigned(
    tx: &WriteTx<'_>,
    task: &crate::model::Task,
    before: Option<crate::model::ActorKind>,
    actor: crate::model::ActorKind,
) -> Result<()> {
    let Some(kind) = task.assignee_kind else { return Ok(()) };
    if Some(kind) == before {
        return Ok(());
    }
    let at = task.updated_at.to_rfc3339_z();
    emit(
        tx,
        crate::lifecycle::name::TASK_ASSIGNED,
        task.id,
        actor,
        &at,
        Some(kind.as_str()),
    )
}

/// `task.moved`: a task changed which project it belongs to, carrying the destination project's slug as
/// `new`. A pure reorder within the same project is not a move and emits nothing. The destination is
/// always a real project (`move_to` refuses the inbox) and a project's slug is derived at creation, so
/// the slug is present — the fallback only keeps the read total.
pub(super) fn emit_task_moved(
    tx: &WriteTx<'_>,
    task: &crate::model::Task,
    before_project: Option<i64>,
    actor: crate::model::ActorKind,
) -> Result<()> {
    if task.project_id == before_project {
        return Ok(());
    }
    let Some(project_id) = task.project_id else { return Ok(()) };
    let slug = crate::store_engine::read::project(tx.conn(), project_id)?
        .and_then(|p| p.slug)
        .unwrap_or_default();
    let at = task.updated_at.to_rfc3339_z();
    emit(tx, crate::lifecycle::name::TASK_MOVED, task.id, actor, &at, Some(&slug))
}

/// A decision verdict event (`decision.accepted` / `decision.rejected`). The name is the whole state, so
/// it carries no `new`. The caller emits it only on a real transition — the idempotent re-accept /
/// re-reject reports `changed = false`, and there is nothing to observe.
pub(super) fn emit_decision_verdict(
    tx: &WriteTx<'_>,
    decision: &crate::model::Decision,
    event: &str,
    actor: crate::model::ActorKind,
) -> Result<()> {
    let at = decision.updated_at.to_rfc3339_z();
    emit(tx, event, decision.id, actor, &at, None)
}

/// `task.created`: a task was created (`AMB-D-367`). The name is the whole state, so it carries no `new`.
///
/// It fires when the **creation ends**, not when `add_task` returns (`AMB-D-557`): between the two the
/// task is on the board but nobody can reserve it, so a subscriber told about it then has nothing it can
/// do — and the firing point is a wire contract, which cannot be moved quietly later. `at` is therefore
/// the moment the creation closed, and the actor is whoever closed it.
///
/// A creation that had already ended is not a change to observe; [`crate::ops::task::finish_creating`]
/// returns before it reaches here for one.
pub(super) fn emit_task_created(
    tx: &WriteTx<'_>,
    task: &crate::model::Task,
    actor: crate::model::ActorKind,
) -> Result<()> {
    let at = task.updated_at.to_rfc3339_z();
    emit(tx, crate::lifecycle::name::TASK_CREATED, task.id, actor, &at, None)
}

/// `task.deleted`: a task was hard-deleted (`AMB-D-367`). The name is the whole state (no `new`) and
/// `id` is the task's own — the row is gone after the delete, but the outbox is a separate table, so the
/// event outlives it. This is the task's own event alone; the comments it takes down with it are observed
/// by [`emit_task_subtree_deleted`], which is what both delete paths actually call. The caller passes the
/// deletion's clock as `at`.
///
/// **Call this while the task is still there** — before the `DELETE`, inside the same transaction. The
/// event is stamped with the project the task was in (`AMB-D-405`), and a deleted task can no longer say
/// which that was; emitted after the row went, the deletion would reach no project-scoped plugin at all,
/// which is the very failure that decision exists to end.
fn emit_task_deleted(
    tx: &WriteTx<'_>,
    id: i64,
    actor: crate::model::ActorKind,
    at: &str,
) -> Result<()> {
    emit(tx, crate::lifecycle::name::TASK_DELETED, id, actor, at, None)
}

/// `comment.added`: a comment was added to a task (`AMB-D-367`). `id` is the comment's own id and the
/// actor is its author; the name is the whole state, so no `new`. Fired for task comments only — a
/// decision comment is not a v1 event, so its write point does not call this.
pub(super) fn emit_comment_added(
    tx: &WriteTx<'_>,
    comment: &crate::model::TaskComment,
    actor: crate::model::ActorKind,
) -> Result<()> {
    let at = comment.created_at.to_rfc3339_z();
    emit(tx, crate::lifecycle::name::COMMENT_ADDED, comment.id, actor, &at, None)
}

/// `comment.removed`: a task comment was hard-deleted (`AMB-D-401`). `id` is the comment's own — the same
/// axis `comment.added` reports on, so a subscriber can pair the two — and the actor is whoever deleted
/// it, not the author who wrote it. The name is the whole state, so no `new`, and the caller passes the
/// deletion's clock as `at` (the row's own timestamps describe the writing, not the taking back).
///
/// **Call this while the comment is still there** — before the `DELETE`, inside the same transaction —
/// for the reason [`emit_task_deleted`] gives: the event is stamped with the project of the task the
/// comment hung on, and that is read off the comment row (`AMB-D-405`).
pub(super) fn emit_comment_removed(
    tx: &WriteTx<'_>,
    id: i64,
    actor: crate::model::ActorKind,
    at: &str,
) -> Result<()> {
    emit(tx, crate::lifecycle::name::COMMENT_REMOVED, id, actor, at, None)
}

/// Everything one task's removal is observed as: a `comment.removed` for each comment the cascade carries
/// off, and then the `task.deleted` itself (`AMB-D-401`). Children first, the order the delete op itself
/// unwinds them in — a subscriber mirroring the store can drop the comments and then the task they hung
/// on, never the reverse.
///
/// A comment swept up by a delete is as unrecoverable as one deleted on its own, and just as invisible to
/// a re-read, so leaving the cascade silent is the generation gap `AMB-D-367` does not allow. Both delete
/// paths go through here — a task deleted on its own ([`crate::ops::task::delete`]) and a task carried off by
/// its project ([`crate::ops::project::delete`], once per member task) — so the two cannot drift apart.
///
/// **Call this while the task is still there**, before the `DELETE` and inside the same transaction: the
/// comment ids are read off the live rows, and every event is stamped with the project they were in
/// (`AMB-D-405`).
pub(super) fn emit_task_subtree_deleted(
    tx: &WriteTx<'_>,
    task_id: i64,
    actor: crate::model::ActorKind,
    at: &str,
) -> Result<()> {
    for comment_id in crate::store_engine::read::task_comment_ids(tx.conn(), task_id)? {
        emit_comment_removed(tx, comment_id, actor, at)?;
    }
    emit_task_deleted(tx, task_id, actor, at)
}
