//! The write path. **Mutations issue SQL straight against the truth source**: a write wrapper
//! ([`Store::add_task`] and friends) opens a [`crate::store_engine::WriteTx`], has `ops` do its
//! reads and writes inside it, and commits. **One logical operation = one transaction.** A surface
//! (CLI or GUI) that only wants to write the config calls [`Store::save_config`] instead.

use chrono::NaiveDate;

use crate::error::Result;
use crate::store_engine::WriteTx;

use super::write_reach::{self, WriteTarget};
use super::{ensure_dir, write_atomic, Store};

/// Take the next activity sequence number for a system event and mark it used **in the same
/// transaction**. A system event has no row in the DB (only a line in the ledger), so the next
/// `MAX(id)` would not see this id — without the high-water mark, two events in a row would be
/// handed the same id, breaking the `(at, source, id)` tie-break of the total order that merges the
/// ledger with the comment tables. The mark commits together with the mutation that caused the
/// event, so an id can never be handed out while the mutation rolls back (which would leave a gap).
fn mint_activity_id(tx: &WriteTx<'_>) -> Result<i64> {
    let id = crate::store_engine::read::next_activity_id(tx.conn())?;
    tx.set_meta(crate::store_engine::read::ACTIVITY_HIGH_WATER, Some(&id.to_string()))?;
    Ok(id)
}

// ── Plugin observation events (the outbox emit half of the write path) ─────────
//
// A write wrapper composes the semantic event it alone can name — it knows the operation intent (its
// own name says status / assigned / moved / accepted / rejected), the actor, and the new state — and
// appends it to the plugin outbox inside the mutation's own transaction, through `WriteTx::emit_event`.
// Generation is leak-free (a rollback drops the event with the write it described); delivery is a later,
// best-effort concern handled by the dispatcher. The store interprets none of the strings — it stores
// the row it is given. Emitting here, at the one seam both CLI and GUI funnel through, is what makes the
// same event fire whatever the surface. The rationale for a log kept separate from the change feed lives
// in the decision log.

/// **The one door every observation event goes through.** Each write wrapper's helper below composes the
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
    use crate::plugin_payload::name as ev;
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
    use crate::plugin_payload::name as ev;
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
    use crate::plugin_payload::name as ev;
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
        // placed ahead of the `DELETE` (`Store::remove_task_comment`).
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
fn emit_task_status(
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
        crate::model::TaskStatus::Done => (crate::plugin_payload::name::TASK_DONE, None),
        crate::model::TaskStatus::Rejected => (crate::plugin_payload::name::TASK_REJECTED, None),
        _ => (crate::plugin_payload::name::TASK_STATUS_CHANGED, Some(task.status.as_str())),
    };
    emit(tx, event, task.id, actor, &at, new_state)
}

/// `task.assigned`: a task gained or changed its assignee, carrying the new assignee facet as `new`.
/// Clearing the assignee emits nothing (v1 has no `task.unassigned`), and re-assigning the same facet is
/// not a change to observe.
fn emit_task_assigned(
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
        crate::plugin_payload::name::TASK_ASSIGNED,
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
fn emit_task_moved(
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
    emit(tx, crate::plugin_payload::name::TASK_MOVED, task.id, actor, &at, Some(&slug))
}

/// A decision verdict event (`decision.accepted` / `decision.rejected`). The name is the whole state, so
/// it carries no `new`. The caller emits it only on a real transition — the idempotent re-accept /
/// re-reject reports `changed = false`, and there is nothing to observe.
fn emit_decision_verdict(
    tx: &WriteTx<'_>,
    decision: &crate::model::Decision,
    event: &str,
    actor: crate::model::ActorKind,
) -> Result<()> {
    let at = decision.updated_at.to_rfc3339_z();
    emit(tx, event, decision.id, actor, &at, None)
}

/// `task.created`: a task was created (`AMB-D-367`). The name is the whole state, so it carries no
/// `new`, and it fires unconditionally — every `add_task` is a creation to observe. The actor is the
/// creator's facet (`NewTask::created_by_kind`), which defaults to human when the creator was unstated.
fn emit_task_created(
    tx: &WriteTx<'_>,
    task: &crate::model::Task,
    actor: crate::model::ActorKind,
) -> Result<()> {
    let at = task.created_at.to_rfc3339_z();
    emit(tx, crate::plugin_payload::name::TASK_CREATED, task.id, actor, &at, None)
}

/// `task.deleted`: a task was hard-deleted (`AMB-D-367`). The name is the whole state (no `new`) and
/// `id` is the task's own — the row is gone after the delete, but the outbox is a separate table, so the
/// event outlives it. This is the task's own event alone; the comments it takes down with it are observed
/// by [`emit_task_subtree_deleted`], which is what both delete paths actually call. The caller passes the
/// deletion's clock as `at`, shared with the ledger line.
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
    emit(tx, crate::plugin_payload::name::TASK_DELETED, id, actor, at, None)
}

/// `comment.added`: a comment was added to a task (`AMB-D-367`). `id` is the comment's own id and the
/// actor is its author; the name is the whole state, so no `new`. Fired for task comments only — a
/// decision comment is not a v1 event, so its write point does not call this.
fn emit_comment_added(
    tx: &WriteTx<'_>,
    comment: &crate::model::TaskComment,
    actor: crate::model::ActorKind,
) -> Result<()> {
    let at = comment.created_at.to_rfc3339_z();
    emit(tx, crate::plugin_payload::name::COMMENT_ADDED, comment.id, actor, &at, None)
}

/// `comment.removed`: a task comment was hard-deleted (`AMB-D-401`). `id` is the comment's own — the same
/// axis `comment.added` reports on, so a subscriber can pair the two — and the actor is whoever deleted
/// it, not the author who wrote it. The name is the whole state, so no `new`, and the caller passes the
/// deletion's clock as `at` (the row's own timestamps describe the writing, not the taking back).
///
/// **Call this while the comment is still there** — before the `DELETE`, inside the same transaction —
/// for the reason [`emit_task_deleted`] gives: the event is stamped with the project of the task the
/// comment hung on, and that is read off the comment row (`AMB-D-405`).
fn emit_comment_removed(
    tx: &WriteTx<'_>,
    id: i64,
    actor: crate::model::ActorKind,
    at: &str,
) -> Result<()> {
    emit(tx, crate::plugin_payload::name::COMMENT_REMOVED, id, actor, at, None)
}

/// Everything one task's removal is observed as: a `comment.removed` for each comment the cascade carries
/// off, and then the `task.deleted` itself (`AMB-D-401`). Children first, the order the delete op itself
/// unwinds them in — a subscriber mirroring the store can drop the comments and then the task they hung
/// on, never the reverse.
///
/// A comment swept up by a delete is as unrecoverable as one deleted on its own, and just as invisible to
/// a re-read, so leaving the cascade silent is the generation gap `AMB-D-367` does not allow. Both delete
/// paths go through here — a task deleted on its own ([`Store::delete_task`]) and a task carried off by
/// its project ([`Store::project_delete`], once per member task) — so the two cannot drift apart.
///
/// **Call this while the task is still there**, before the `DELETE` and inside the same transaction: the
/// comment ids are read off the live rows, and every event is stamped with the project they were in
/// (`AMB-D-405`).
fn emit_task_subtree_deleted(
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

impl Store {
    /// **One logical operation = one transaction.** Opens `BEGIN IMMEDIATE`, hands it to `op`, and
    /// commits if `op` succeeds. If `op` returns early via `?` the guard drops before the commit and
    /// **not one** of that operation's writes survives — a torn row is structurally impossible.
    /// `op` also does its reads inside the same `tx` (the `before` snapshot, the new id from `next_id`, a
    /// sibling's `order_key`, the CAS of a reservation): hoist a read out of the transaction and a
    /// concurrent writer will build on the same snapshot and the two will erase each other.
    /// `targets` declares **the entities this mutation touches** ([`write_reach`]). The reach guard
    /// stands in this one place, and a mutation that names an entity outside the binding from a
    /// closed reach (the AI facet) fails with `out_of_reach`. Because the declaration is an argument
    /// it **cannot be forgotten** — a new write wrapper necessarily goes through the guard. The check
    /// runs before `op`, so "written, then refused" cannot happen.
    fn write_one<T>(
        &mut self,
        targets: &[WriteTarget],
        op: impl FnOnce(&WriteTx<'_>) -> Result<T>,
    ) -> Result<T> {
        let reach = self.reach;
        let tx = self.engine.write()?;
        write_reach::guard(tx.conn(), reach, targets)?;
        let out = op(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Create a task (one operation = one transaction). Both the conversational number
    /// (`next_id`) and the placement sibling's `order_key` are read-then-write, and those reads
    /// happen inside this transaction. Once the task exists, if its project has a time axis
    /// (`role: time_axis`) it is assigned by default to **the period that contains today** — this is
    /// automation, not a requirement: when no period applies (no time axis, no window containing
    /// today, an inbox task) the task is created unassigned, and the user can override or clear it
    /// later with `dimension set` / `unset`. Both CLI and GUI funnel their creation paths through
    /// here, so this one place is where the default takes effect. The assignment rides in the **same
    /// transaction** as the task row, so a task that should have the default can never commit
    /// without it. To classify at creation, call [`Self::add_task_with_dimensions`] — this is that,
    /// with nothing named.
    pub fn add_task(&mut self, input: crate::ops::task::NewTask) -> Result<crate::model::Task> {
        self.add_task_with_dimensions(input, &[])
    }

    /// [`Self::add_task`], with classification the caller already decided on — the values ride in the
    /// **same transaction** as the task row, so a task filed under an axis can never commit without it,
    /// and a create that fails leaves no half-classified task behind. The ids are resolved by the
    /// surface (a value is named by the axis it lives on, and that resolution belongs where the names
    /// are); here they are applied in the order given.
    ///
    /// **What the caller names wins over the default.** The time-axis default fills the axis nobody
    /// named — assign it first and the explicit value would land as a *replacement*, writing a period
    /// the task never belonged to and then deleting it, inside the transaction anyone watching the store
    /// is reading. So the caller's values go on first, and the default is offered only to an axis they
    /// left alone.
    pub fn add_task_with_dimensions(
        &mut self,
        input: crate::ops::task::NewTask,
        value_ids: &[i64],
    ) -> Result<crate::model::Task> {
        let today = crate::time::today();
        // The creator's facet, read before `input` is moved into the op — the actor of `task.created`.
        let actor = input.created_by_kind.unwrap_or_default();
        let home = WriteTarget::NewIn(input.project_id);
        self.write_one(&[home], |tx| {
            let task = crate::ops::task::add(tx, input)?;
            // An axis is single-select, so naming one twice is last-wins rather than two rows; the
            // surface is where that is refused, with the names to say which axis was named twice.
            let mut named_axes: Vec<i64> = Vec::with_capacity(value_ids.len());
            for &value_id in value_ids {
                let (tv, _) = crate::ops::dimension::set(tx, task.id, value_id)?;
                named_axes.push(tv.dimension_id);
            }
            if let Some(project_id) = task.project_id {
                let value_id =
                    crate::store_engine::read::current_time_axis_value(tx.conn(), project_id, today)?;
                if let Some(value_id) = value_id {
                    let axis = crate::store_engine::read::dimension_id_of_value(tx.conn(), value_id)?;
                    if !axis.is_some_and(|axis| named_axes.contains(&axis)) {
                        crate::ops::dimension::set(tx, task.id, value_id)?;
                    }
                }
            }
            emit_task_created(tx, &task, actor)?;
            Ok(task)
        })
    }

    /// Update a task's fields (one operation = one transaction).
    pub fn update_task(
        &mut self,
        id: i64,
        patch: crate::ops::task::TaskPatch,
    ) -> Result<crate::model::Task> {
        self.write_one(&[WriteTarget::Task(id)], |tx| crate::ops::task::update(tx, id, patch))
    }

    /// Assign or unassign a task's assignee (one operation = one transaction). `actor` is who performed
    /// the assignment (the process facet), stamped onto the `task.assigned` observation event; the
    /// assignee that lands is `kind`, a separate thing.
    pub fn set_task_assignee(
        &mut self,
        id: i64,
        kind: Option<crate::model::ActorKind>,
        actor: crate::model::ActorKind,
    ) -> Result<crate::model::Task> {
        self.write_one(&[WriteTarget::Task(id)], |tx| {
            let before = crate::store_engine::read::task(tx.conn(), id)?.and_then(|t| t.assignee_kind);
            let task = crate::ops::task::set_assignee(tx, id, kind)?;
            emit_task_assigned(tx, &task, before, actor)?;
            Ok(task)
        })
    }

    /// Set a task's status (one operation = one transaction). The compare-and-swap that reserves a
    /// task (`todo → in_progress`) reads the truth source's current status inside this transaction.
    /// `actor` is the process facet, stamped onto the `task.status_changed` / `task.done` event.
    pub fn set_task_status(
        &mut self,
        id: i64,
        status: crate::model::TaskStatus,
        actor: crate::model::ActorKind,
    ) -> Result<crate::model::Task> {
        self.write_one(&[WriteTarget::Task(id)], |tx| {
            let before = crate::store_engine::read::task_status(tx.conn(), id)?;
            let task = crate::ops::task::set_status(tx, id, status)?;
            if let Some(before) = before {
                emit_task_status(tx, &task, before, actor)?;
            }
            Ok(task)
        })
    }

    /// Done / reopen — sugar over `set_task_status` (one operation = one transaction). `actor` is the
    /// process facet, stamped onto the emitted status event.
    pub fn set_task_completed(
        &mut self,
        id: i64,
        completed: bool,
        actor: crate::model::ActorKind,
    ) -> Result<crate::model::Task> {
        self.write_one(&[WriteTarget::Task(id)], |tx| {
            let before = crate::store_engine::read::task_status(tx.conn(), id)?;
            let task = crate::ops::task::set_completed(tx, id, completed)?;
            if let Some(before) = before {
                emit_task_status(tx, &task, before, actor)?;
            }
            Ok(task)
        })
    }

    /// Delete a task — a hard delete (one operation = one transaction). The task row and its
    /// dependency edges go in the same transaction (leave one behind and you have a dangling edge).
    /// Blobs the delete orphaned are reclaimed after the commit ([`Self::reclaim_after_delete`]). The
    /// delete leaves a line **only in the file ledger** ([`Self::log_deletion`]) — and since what was
    /// deleted (its title, its project) becomes unreadable the moment the row is gone, it is read
    /// **before** the delete, inside the same transaction.
    pub fn delete_task(&mut self, id: i64, actor: crate::model::ActorKind) -> Result<()> {
        let (orphaned, entry) = self.write_one(&[WriteTarget::Task(id)], |tx| {
            let title = crate::store_engine::read::task_title(tx.conn(), id)?;
            let project = crate::store_engine::read::task_project_id(tx.conn(), id)?;
            let activity_id = mint_activity_id(tx)?;
            // One clock for the deletion — the plugin event and the ledger line record the same instant.
            let now = crate::time::Timestamp::now();
            // Before the delete: the events are stamped with the task's project, which only a task that
            // is still there can say (`AMB-D-405`).
            emit_task_subtree_deleted(tx, id, actor, &now.to_rfc3339_z())?;
            let orphaned = crate::ops::task::delete(tx, id)?;
            let entry = crate::activity_log::Entry {
                id: activity_id,
                at: now,
                actor: Some(actor),
                project,
                task: Some(id),
                decision: None,
                event: crate::activity_log::event::task_deleted(title.as_deref()),
            };
            Ok((orphaned, entry))
        })?;
        self.log_deletion(entry);
        self.reclaim_after_delete(&orphaned);
        Ok(())
    }

    /// Append one line for a deletion to the file ledger. Call this **after the commit**. A failure
    /// is a warning and the deletion still stands (the ledger is not the system of record). Since a
    /// deletion takes its row with it, this line is the only thing that remembers what was deleted.
    fn log_deletion(&self, entry: crate::activity_log::Entry) {
        crate::activity_log::append(&self.paths.activity_file, &entry);
    }

    /// Reclaim the bytes of blobs a deletion let go of. Call this **after the commit**: erase bytes
    /// inside the transaction and a rollback restores the rows while the bytes are gone for good.
    /// Best-effort — a failure leaves the deletion successful, and anything missed is unreachable
    /// garbage with zero references that the full sweep in `gc_blobs` (project teardown,
    /// `doctor --fix`) picks up later. Every path passes `GC_MIN_AGE` so a concurrent attach's
    /// in-flight bytes are not swept.
    fn reclaim_after_delete(&self, orphaned: &[String]) {
        let _ = self.reclaim_blobs(orphaned, crate::blob::GC_MIN_AGE);
    }

    /// Re-home a task (change which project it belongs to) and reorder it (one operation = one
    /// transaction). Checking that it exists, reading its current home and reading its placement
    /// siblings all happen inside this transaction. A task is only ever placed in a project.
    pub fn move_task(
        &mut self,
        id: i64,
        target_project: Option<i64>,
        pos: crate::ops::Position,
        actor: crate::model::ActorKind,
    ) -> Result<crate::model::Task> {
        self.write_one(
            // Re-homing touches the destination too: both where it comes from and where it lands
            // have to be within reach.
            &[WriteTarget::Task(id), WriteTarget::NewIn(target_project)],
            |tx| {
                let before_project =
                    crate::store_engine::read::task(tx.conn(), id)?.and_then(|t| t.project_id);
                let task = crate::ops::task::move_to(tx, id, target_project, pos)?;
                emit_task_moved(tx, &task, before_project, actor)?;
                Ok(task)
            },
        )
    }

    /// Add a dependency edge (one operation = one transaction). Idempotent: if the edge already
    /// exists, `created=false`.
    pub fn depend_task(
        &mut self,
        id: i64,
        blocked_by_id: i64,
        created_by_kind: Option<crate::model::ActorKind>,
    ) -> Result<(crate::model::TaskDependency, bool)> {
        self.write_one(
            &[WriteTarget::Task(id), WriteTarget::Task(blocked_by_id)],
            |tx| crate::ops::dependency::add(tx, id, blocked_by_id, created_by_kind),
        )
    }

    /// Remove a dependency edge (one operation = one transaction). If there is none, it is a no-op
    /// and returns `false`.
    pub fn undepend_task(&mut self, id: i64, blocked_by_id: i64) -> Result<bool> {
        self.write_one(
            &[WriteTarget::Task(id), WriteTarget::Task(blocked_by_id)],
            |tx| crate::ops::dependency::remove(tx, id, blocked_by_id),
        )
    }

    /// Record a commit SHA on a task (one operation = one transaction). The SHA is validated and
    /// normalised at the ops door; idempotent — a SHA already on the task yields `created=false`.
    pub fn add_task_commit(
        &mut self,
        id: i64,
        sha: &str,
        created_by_kind: Option<crate::model::ActorKind>,
    ) -> Result<(crate::model::TaskCommit, bool)> {
        self.write_one(&[WriteTarget::Task(id)], |tx| crate::ops::commit::add(tx, id, sha, created_by_kind))
    }

    /// Forget a commit SHA on a task (one operation = one transaction). If it is not recorded, it is a
    /// no-op and returns `false`.
    pub fn remove_task_commit(&mut self, id: i64, sha: &str) -> Result<bool> {
        self.write_one(&[WriteTarget::Task(id)], |tx| crate::ops::commit::remove(tx, id, sha))
    }

    /// Set (`Some`) or clear (`None`) one plugin text field's value in a project (one operation = one
    /// transaction). Returns whether anything changed. The value is validated at the config write boundary
    /// ([`crate::plugin_config::set`]) before it reaches here; reach is guarded by `WriteTarget::Project`.
    pub fn set_plugin_config_value(
        &mut self,
        project_id: i64,
        plugin: &str,
        field_key: &str,
        value: Option<&str>,
    ) -> Result<bool> {
        self.write_one(&[WriteTarget::Project(project_id)], |tx| {
            crate::ops::plugin_config::set(tx, project_id, plugin, field_key, value)
        })
    }

    /// The secret twin of [`Self::set_plugin_config_value`] — the same operation against the table an
    /// `export` must leave (`AMB-D-434`). Which of the two a value goes to is the config write
    /// boundary's call, made from the author's `secret` flag alone.
    pub fn set_plugin_secret(
        &mut self,
        project_id: i64,
        plugin: &str,
        field_key: &str,
        value: Option<&str>,
    ) -> Result<bool> {
        self.write_one(&[WriteTarget::Project(project_id)], |tx| {
            crate::ops::plugin_secret::set(tx, project_id, plugin, field_key, value)
        })
    }

    /// Erase every project's settings for one plugin, device-wide (one operation = one transaction) —
    /// the store half of `plugin uninstall` (`AMB-D-357`). Returns how many rows went.
    ///
    /// **Deliberately unguarded by project reach**, the only write here that is: what it deletes is one
    /// plugin's residue, not any project's content, and an uninstall that stopped at the bound project
    /// would leave exactly the leftovers the decision forbids. The blast radius is fixed by the plugin
    /// name alone — no caller can aim this at a project's tasks, decisions or comments.
    pub fn forget_plugin_config(&mut self, plugin: &str) -> Result<usize> {
        self.write_one(&[], |tx| crate::ops::plugin_config::forget_plugin(tx, plugin))
    }

    /// The secret twin of [`Self::forget_plugin_config`], unguarded for the same reason — and the one
    /// purge an uninstall runs unconditionally (`AMB-D-357`: a secret must never outlive the plugin).
    pub fn forget_plugin_secrets(&mut self, plugin: &str) -> Result<usize> {
        self.write_one(&[], |tx| crate::ops::plugin_secret::forget_plugin(tx, plugin))
    }

    /// Erase every project's settings for one plugin under a key its manifest no longer declares (one
    /// operation = one transaction) — the store half of `plugin update` (`AMB-D-456`). Returns how many
    /// rows went.
    ///
    /// **Unguarded by project reach** for the reason [`Self::forget_plugin_config`] is: what it deletes is
    /// one plugin's residue in every project, not any project's content, and the blast radius is fixed by
    /// the plugin name and its own declaration — a caller cannot aim it at anything else.
    pub fn purge_undeclared_plugin_config(
        &mut self,
        plugin: &str,
        declared: &[&str],
    ) -> Result<usize> {
        self.write_one(&[], |tx| crate::ops::plugin_config::forget_undeclared(tx, plugin, declared))
    }

    /// The secret twin of [`Self::purge_undeclared_plugin_config`], unguarded for the same reason
    /// (`AMB-D-456`). `declared` is what the manifest declares as secrets.
    pub fn purge_undeclared_plugin_secrets(
        &mut self,
        plugin: &str,
        declared: &[&str],
    ) -> Result<usize> {
        self.write_one(&[], |tx| crate::ops::plugin_secret::forget_undeclared(tx, plugin, declared))
    }

    /// Put a plugin's gate in this project into `on` (one operation = one transaction): `true` writes the
    /// row that says "enabled here", `false` deletes it (`AMB-D-434` — the row *is* the answer). Returns
    /// whether anything changed. Written through the trust boundary ([`crate::plugin_trust`]), which is
    /// where the fail-closed `required` check lives; reach is guarded by `WriteTarget::Project`.
    pub fn set_plugin_enabled_in_project(
        &mut self,
        project_id: i64,
        plugin: &str,
        on: bool,
    ) -> Result<bool> {
        self.write_one(&[WriteTarget::Project(project_id)], |tx| {
            crate::ops::plugin_enable::set(tx, project_id, plugin, on)
        })
    }

    /// Erase every per-project gate answer of one plugin, device-wide (one operation = one transaction) —
    /// the store half of `plugin uninstall` beside [`Self::forget_plugin_config`] (`AMB-D-357`). Returns
    /// how many rows went. **Deliberately unguarded by project reach**, for the reason its config twin is:
    /// what it deletes is one plugin's residue, not any project's content.
    pub fn forget_plugin_enable(&mut self, plugin: &str) -> Result<usize> {
        self.write_one(&[], |tx| crate::ops::plugin_enable::forget_plugin(tx, plugin))
    }

    /// Create a project (one operation = one transaction). The ordering sibling's `order_key` is read
    /// inside this transaction.
    pub fn project_add(
        &mut self,
        input: crate::ops::project::NewProject,
    ) -> Result<crate::model::Project> {
        self.write_one(&[WriteTarget::NewProject], |tx| crate::ops::project::add(tx, input))
    }

    /// Update a project's fields (one operation = one transaction).
    pub fn project_update(
        &mut self,
        id: i64,
        patch: crate::ops::project::ProjectPatch,
    ) -> Result<crate::model::Project> {
        self.write_one(&[WriteTarget::Project(id)], |tx| crate::ops::project::update(tx, id, patch))
    }

    /// Reorder a project (one operation = one transaction).
    pub fn project_move(
        &mut self,
        id: i64,
        pos: crate::ops::Position,
    ) -> Result<crate::model::Project> {
        self.write_one(&[WriteTarget::Project(id)], |tx| crate::ops::project::move_to(tx, id, pos))
    }

    /// Archive or unarchive a project (one operation = one transaction).
    pub fn project_set_archived(
        &mut self,
        id: i64,
        archived: bool,
    ) -> Result<crate::model::Project> {
        self.write_one(&[WriteTarget::Project(id)], |tx| crate::ops::project::set_archived(tx, id, archived))
    }

    /// Delete a project — a hard delete (one operation = one transaction). The cascade that removes
    /// its tasks and their dependency edges rides along. Filesystem and ledger teardown belongs to
    /// the layer above (`project_teardown`). Blob reclamation nonetheless runs here as well: the
    /// teardown above is **bound up with releasing the bound folders**, and deletion paths that never
    /// call it (library users, tests that only hit project delete) must not leave bytes behind (the
    /// full sweep in `project_teardown` remains as the catch-all and picks up blobs that were skipped
    /// for being too young). The ledger gets one line per project ([`Self::log_deletion`]), and the
    /// tasks and decisions taken down with it are recorded **as counts only** — a line each would
    /// have a single delete bury thousands of lines in the ledger and wash every other story away.
    ///
    /// **The plugin outbox is the opposite**: it gets one `task.deleted` per task the cascade carried
    /// off, and one `comment.removed` per comment those tasks carried off with them
    /// ([`emit_task_subtree_deleted`]). A deletion is the one event a plugin cannot recover by re-reading
    /// (there is no row left to read), so a record that vanished inside a project delete has to be as
    /// observable as one deleted on its own — leaving it silent is a generation gap, which `AMB-D-367`
    /// does not allow. The count of events is bounded by what was deleted and they are appended in the
    /// deleting transaction; running them costs one runner per plugin however many there are
    /// (`AMB-D-399`).
    pub fn project_delete(&mut self, id: i64, actor: crate::model::ActorKind) -> Result<()> {
        let (orphaned, entry) = self.write_one(&[WriteTarget::Project(id)], |tx| {
            use crate::store_engine::read;
            let name = read::project_name(tx.conn(), id)?;
            // Read the cascade set **before** the delete: afterwards there is no row left to name, and
            // the tasks are what the plugin events are built from — not just what the ledger counts.
            let tasks = read::task_ids_in_project(tx.conn(), id)?;
            let decisions = read::decision_ids_in_project(tx.conn(), id)?.len();
            let activity_id = mint_activity_id(tx)?;
            // One clock for the whole deletion — every cascaded event and the ledger line share it.
            let now = crate::time::Timestamp::now();
            let at = now.to_rfc3339_z();
            // Before the cascade, for the reason the single delete emits before its own: a task carried
            // off by its project can no longer name the project it was in (`AMB-D-405`).
            for task_id in &tasks {
                emit_task_subtree_deleted(tx, *task_id, actor, &at)?;
            }
            let orphaned = crate::ops::project::delete(tx, id)?;
            let entry = crate::activity_log::Entry {
                id: activity_id,
                at: now,
                actor: Some(actor),
                project: Some(id),
                task: None,
                decision: None,
                event: crate::activity_log::event::project_deleted(
                    name.as_deref(),
                    tasks.len(),
                    decisions,
                ),
            };
            Ok((orphaned, entry))
        })?;
        self.log_deletion(entry);
        self.reclaim_after_delete(&orphaned);
        Ok(())
    }

    /// Create a dimension — an axis of classification (one operation = one transaction).
    pub fn dimension_add(
        &mut self,
        project_id: i64,
        new: crate::ops::dimension::NewDimension,
    ) -> Result<crate::model::Dimension> {
        self.write_one(&[WriteTarget::NewIn(Some(project_id))], |tx| crate::ops::dimension::add(tx, project_id, new))
    }

    /// Update a dimension's name, notes, whether its values are ordered, and its role (one operation
    /// = one transaction).
    pub fn dimension_update(
        &mut self,
        id: i64,
        name: Option<&str>,
        notes: Option<&str>,
        ordered: Option<bool>,
        role: Option<crate::model::DimensionRole>,
    ) -> Result<crate::model::Dimension> {
        self.write_one(&[WriteTarget::Dimension(id)], |tx| {
            crate::ops::dimension::update(tx, id, name, notes, ordered, role)
        })
    }

    /// Reorder a dimension (one operation = one transaction).
    pub fn dimension_move(
        &mut self,
        id: i64,
        pos: crate::ops::Position,
    ) -> Result<crate::model::Dimension> {
        self.write_one(&[WriteTarget::Dimension(id)], |tx| crate::ops::dimension::move_to(tx, id, pos))
    }

    /// Delete a dimension — a hard delete (one operation = one transaction). The cascade that removes
    /// its values and the task assignments to them rides along, so live values can never be left
    /// hanging off a dimension that is gone.
    pub fn dimension_delete(&mut self, id: i64) -> Result<()> {
        self.write_one(&[WriteTarget::Dimension(id)], |tx| crate::ops::dimension::delete(tx, id))
    }

    /// Add a value to a dimension (one operation = one transaction). Pass a `period` and the creation
    /// and the period both commit together (`Some((start, end))`; a `None` at either end leaves that
    /// end open).
    pub fn dimension_value_add(
        &mut self,
        dimension_id: i64,
        name: &str,
        period: Option<(Option<NaiveDate>, Option<NaiveDate>)>,
    ) -> Result<crate::model::DimensionValue> {
        self.write_one(&[WriteTarget::Dimension(dimension_id)], |tx| {
            let v = crate::ops::dimension::value_add(tx, dimension_id, name)?;
            match period {
                Some((start_on, end_on)) => {
                    crate::ops::dimension::value_set_dates(tx, v.id, start_on, end_on)
                }
                None => Ok(v),
            }
        })
    }

    /// Update a dimension value's name and period (one operation = one transaction). A `None`
    /// argument leaves that field as it is. The rename and the period are bundled into one
    /// transaction, so if the period is rejected for running backwards the rename does not survive
    /// either.
    pub fn dimension_value_update(
        &mut self,
        value_id: i64,
        name: Option<&str>,
        period: Option<(Option<NaiveDate>, Option<NaiveDate>)>,
    ) -> Result<crate::model::DimensionValue> {
        self.write_one(&[WriteTarget::DimensionValue(value_id)], |tx| {
            let mut v = match name {
                Some(name) => Some(crate::ops::dimension::value_rename(tx, value_id, name)?),
                None => None,
            };
            if let Some((start_on, end_on)) = period {
                v = Some(crate::ops::dimension::value_set_dates(tx, value_id, start_on, end_on)?);
            }
            match v {
                Some(v) => Ok(v),
                // Nothing was specified, so nothing is written — just return the current value.
                None => crate::store_engine::read::dimension_value(tx.conn(), value_id)?
                    .ok_or_else(|| {
                        crate::ops::dimension::VALUE_NOUN.not_found(value_id.to_string())
                    }),
            }
        })
    }

    /// Reorder a dimension value (one operation = one transaction). Refused on an unordered dimension.
    pub fn dimension_value_move(
        &mut self,
        value_id: i64,
        pos: crate::ops::Position,
    ) -> Result<crate::model::DimensionValue> {
        self.write_one(&[WriteTarget::DimensionValue(value_id)], |tx| crate::ops::dimension::value_move(tx, value_id, pos))
    }

    /// Delete a dimension value — a hard delete (one operation = one transaction). The task
    /// assignments to that value go with it.
    pub fn dimension_value_delete(&mut self, value_id: i64) -> Result<()> {
        self.write_one(&[WriteTarget::DimensionValue(value_id)], |tx| crate::ops::dimension::value_delete(tx, value_id))
    }

    /// Assign a dimension value to a task (one operation = one transaction). Deleting the old row
    /// (the existing assignment on the same axis) and inserting the new one ride together, so the
    /// invariant of one row per `(task, dimension)` is never broken in between.
    pub fn set_task_dimension_value(
        &mut self,
        task_id: i64,
        value_id: i64,
    ) -> Result<(crate::model::TaskDimensionValue, bool)> {
        self.write_one(
            &[WriteTarget::Task(task_id), WriteTarget::DimensionValue(value_id)],
            |tx| crate::ops::dimension::set(tx, task_id, value_id),
        )
    }

    /// Remove a dimension value assignment from a task (one operation = one transaction). If there is
    /// none, it is a no-op and returns `false`.
    pub fn unset_task_dimension_value(&mut self, task_id: i64, value_id: i64) -> Result<bool> {
        self.write_one(
            &[WriteTarget::Task(task_id), WriteTarget::DimensionValue(value_id)],
            |tx| crate::ops::dimension::unset(tx, task_id, value_id),
        )
    }

    /// Add one system event to a task's activity (one operation = one transaction). It **does not
    /// ride in the mutation's own transaction**: activity is not the system of record, and it is
    /// written **after** the commit succeeds (crash before the commit and no line appears; crash
    /// after it and the line is lost — we err towards losing a line, never towards duplicating one).
    /// The callers (CLI, GUI) invoke this after the mutation wrapper has committed, so that ordering
    /// holds. A failure is warned about by the caller and the mutation proceeds. The line goes only
    /// into the file ledger ([`crate::activity_log`]); all that stays in the DB is the sequence-number
    /// mark, and the event itself — who did what — is one line of JSONL. Same shape as the deletion
    /// events ([`Self::delete_task`] and friends): the only difference between this path and the
    /// deletion path is whether the target's row survives.
    pub fn add_system_event(
        &mut self,
        author_kind: crate::model::ActorKind,
        target_id: i64,
        event: serde_json::Value,
    ) -> Result<crate::activity_log::Entry> {
        let entry = self.write_one(&[WriteTarget::Task(target_id)], |tx| {
            Ok(crate::activity_log::Entry {
                id: mint_activity_id(tx)?,
                at: crate::time::Timestamp::now(),
                actor: Some(author_kind),
                // A ledger line carries its own project — a file cannot be joined against the DB.
                project: crate::store_engine::read::task_project_id(tx.conn(), target_id)?,
                task: Some(target_id),
                decision: None,
                event,
            })
        })?;
        crate::activity_log::append(&self.paths.activity_file, &entry);
        Ok(entry)
    }

    /// Add a comment to a task (one operation = one transaction).
    pub fn add_task_comment(
        &mut self,
        task_id: i64,
        author_kind: crate::model::ActorKind,
        text: &str,
    ) -> Result<crate::model::TaskComment> {
        self.write_one(&[WriteTarget::Task(task_id)], |tx| {
            let comment = crate::ops::comment::add_comment(tx, task_id, author_kind, text)?;
            emit_comment_added(tx, &comment, author_kind)?;
            Ok(comment)
        })
    }

    /// Hard-delete a task comment (one operation = one transaction). If there is none, it is a no-op
    /// and returns `false`. `actor` is the facet doing the deleting — the comment row says who *wrote*
    /// it, and that is a different person from whoever takes it back.
    pub fn remove_task_comment(&mut self, id: i64, actor: crate::model::ActorKind) -> Result<bool> {
        self.write_one(&[WriteTarget::TaskComment(id)], |tx| {
            // Ahead of the delete, and only for a comment that is really there: a no-op is not a change
            // to observe, and after the `DELETE` the event could no longer say which project it was in
            // (`emit_comment_removed`).
            if crate::store_engine::read::task_comment(tx.conn(), id)?.is_some() {
                let at = crate::time::Timestamp::now().to_rfc3339_z();
                emit_comment_removed(tx, id, actor, &at)?;
            }
            crate::ops::comment::remove_comment(tx, id)
        })
    }

    /// Edit the body of a task comment (one operation = one transaction). Not found if there is none.
    pub fn edit_task_comment(&mut self, id: i64, text: &str) -> Result<crate::model::TaskComment> {
        self.write_one(&[WriteTarget::TaskComment(id)], |tx| crate::ops::comment::edit_comment(tx, id, text))
    }

    /// Create a decision (one operation = one transaction). A decision's conversational number comes
    /// from `next_id` in a number space of its own, separate from tasks, and that read happens
    /// inside this transaction.
    pub fn add_decision(
        &mut self,
        input: crate::ops::decision::NewDecision,
    ) -> Result<crate::model::Decision> {
        self.write_one(&[WriteTarget::NewIn(Some(input.project_id))], |tx| {
            crate::ops::decision::add(tx, input)
        })
    }

    /// Edit the body of a decision that is still under discussion (one operation = one transaction).
    pub fn update_decision(
        &mut self,
        id: i64,
        patch: crate::ops::decision::DecisionPatch,
    ) -> Result<crate::model::Decision> {
        self.write_one(&[WriteTarget::Decision(id)], |tx| crate::ops::decision::update(tx, id, patch))
    }

    /// Accept a decision (one operation = one transaction). Returns `(decision, changed)`; `changed`
    /// is `false` on the idempotent noop (already accepted), so the caller does not report a fresh
    /// acceptance that never happened.
    pub fn accept_decision(
        &mut self,
        id: i64,
        decided_by: Option<String>,
        actor: crate::model::ActorKind,
    ) -> Result<(crate::model::Decision, bool)> {
        self.write_one(&[WriteTarget::Decision(id)], |tx| {
            let (decision, changed) = crate::ops::decision::accept(tx, id, decided_by)?;
            if changed {
                emit_decision_verdict(tx, &decision, crate::plugin_payload::name::DECISION_ACCEPTED, actor)?;
            }
            Ok((decision, changed))
        })
    }

    /// Reject a decision (one operation = one transaction). Returns `(decision, changed)`; `changed`
    /// is `false` on the idempotent noop (already rejected). `actor` is the process facet, stamped onto
    /// the `decision.rejected` event fired on a real transition.
    pub fn reject_decision(
        &mut self,
        id: i64,
        actor: crate::model::ActorKind,
    ) -> Result<(crate::model::Decision, bool)> {
        self.write_one(&[WriteTarget::Decision(id)], |tx| {
            let (decision, changed) = crate::ops::decision::reject(tx, id)?;
            if changed {
                emit_decision_verdict(tx, &decision, crate::plugin_payload::name::DECISION_REJECTED, actor)?;
            }
            Ok((decision, changed))
        })
    }

    /// Return an accepted decision to discussion (one operation = one transaction). Returns
    /// `(decision, changed)`; `changed` is `false` on the idempotent noop (already proposed).
    pub fn reopen_decision(&mut self, id: i64) -> Result<(crate::model::Decision, bool)> {
        self.write_one(&[WriteTarget::Decision(id)], |tx| crate::ops::decision::reopen(tx, id))
    }

    /// Supersede one decision with another (one operation = one transaction). Inserting the
    /// `supersedes` edge and promoting the new decision ride together; the old decision's row is left
    /// untouched, because whether it is current is derived from the edges. When the supersession
    /// promotes the new side `Proposed → Accepted`, that acceptance is a real verdict, so a
    /// `decision.accepted` event is emitted on the promotion (and only then — drawing the edge over an
    /// already-accepted side promotes nothing and observes nothing). `actor` is the process facet,
    /// stamped onto that event. Returns `(new_decision, changed)`.
    pub fn supersede_decision(
        &mut self,
        new_id: i64,
        old_id: i64,
        decided_by: Option<String>,
        actor: crate::model::ActorKind,
    ) -> Result<(crate::model::Decision, bool)> {
        self.write_one(
            &[WriteTarget::Decision(new_id), WriteTarget::Decision(old_id)],
            |tx| {
                let (decision, changed, promoted) =
                    crate::ops::decision::supersede(tx, new_id, old_id, decided_by)?;
                if promoted {
                    emit_decision_verdict(tx, &decision, crate::plugin_payload::name::DECISION_ACCEPTED, actor)?;
                }
                Ok((decision, changed))
            },
        )
    }

    /// Amend a decision in part — an `amends` edge (one operation = one transaction).
    pub fn amend_decision(&mut self, new_id: i64, old_id: i64) -> Result<crate::model::Decision> {
        self.write_one(
            &[WriteTarget::Decision(new_id), WriteTarget::Decision(old_id)],
            |tx| crate::ops::decision::amend(tx, new_id, old_id),
        )
    }

    /// Record that one decision rests on another — a `builds_on` edge (one operation = one
    /// transaction). Neither decision's row is touched: this is one edge row and nothing else.
    pub fn decision_builds_on(
        &mut self,
        new_id: i64,
        old_id: i64,
    ) -> Result<crate::model::Decision> {
        self.write_one(
            &[WriteTarget::Decision(new_id), WriteTarget::Decision(old_id)],
            |tx| crate::ops::decision::builds_on(tx, new_id, old_id),
        )
    }

    /// Remove one edge between decisions — all three edge types (one operation = one transaction).
    /// This is how mis-wired edges are corrected; if there is none, it returns `false`. Remove a
    /// `supersedes` edge and the target simply becomes current again, since currency is a derived
    /// projection — there is nothing else to clean up.
    pub fn unlink_decision_edge(&mut self, decision_id: i64, target_decision_id: i64) -> Result<bool> {
        self.write_one(
            &[WriteTarget::Decision(decision_id), WriteTarget::Decision(target_decision_id)],
            |tx| crate::ops::decision::unlink_edge(tx, decision_id, target_decision_id),
        )
    }

    /// Delete a decision — a hard delete (one operation = one transaction). The decision's row and
    /// its task links go in the same transaction (leave one behind and you have a dangling link).
    /// Blobs the delete orphaned are reclaimed after the commit. The ledger line points at the
    /// decision through its `decision` field ([`crate::activity_log::Entry`]).
    pub fn delete_decision(&mut self, id: i64, actor: crate::model::ActorKind) -> Result<()> {
        let (orphaned, entry) = self.write_one(&[WriteTarget::Decision(id)], |tx| {
            use crate::store_engine::read;
            let title = read::decision_title(tx.conn(), id)?;
            let project = read::decision_project_id(tx.conn(), id)?;
            let activity_id = mint_activity_id(tx)?;
            let orphaned = crate::ops::decision::delete(tx, id)?;
            let entry = crate::activity_log::Entry {
                id: activity_id,
                at: crate::time::Timestamp::now(),
                actor: Some(actor),
                project,
                task: None,
                decision: Some(id),
                event: crate::activity_log::event::decision_deleted(title.as_deref()),
            };
            Ok((orphaned, entry))
        })?;
        self.log_deletion(entry);
        self.reclaim_after_delete(&orphaned);
        Ok(())
    }

    /// Link a decision to a task (one operation = one transaction). Idempotent: if the link already
    /// exists, `created=false`.
    pub fn link_decision(
        &mut self,
        decision_id: i64,
        task_id: i64,
    ) -> Result<(crate::model::DecisionTaskLink, bool)> {
        self.write_one(
            &[WriteTarget::Decision(decision_id), WriteTarget::Task(task_id)],
            |tx| crate::ops::decision::link(tx, decision_id, task_id),
        )
    }

    /// Remove that link (one operation = one transaction). If there is none, it is a no-op and
    /// returns `false`.
    pub fn unlink_decision(&mut self, decision_id: i64, task_id: i64) -> Result<bool> {
        self.write_one(
            &[WriteTarget::Decision(decision_id), WriteTarget::Task(task_id)],
            |tx| crate::ops::decision::unlink(tx, decision_id, task_id),
        )
    }

    /// Add a comment to a decision (one operation = one transaction).
    pub fn add_decision_comment(
        &mut self,
        decision_id: i64,
        author_kind: crate::model::ActorKind,
        text: &str,
    ) -> Result<crate::model::DecisionComment> {
        self.write_one(&[WriteTarget::Decision(decision_id)], |tx| {
            crate::ops::decision::add_comment(tx, decision_id, author_kind, text)
        })
    }

    /// Hard-delete a decision comment (one operation = one transaction). If there is none, it is a
    /// no-op and returns `false`.
    pub fn remove_decision_comment(&mut self, id: i64) -> Result<bool> {
        self.write_one(&[WriteTarget::DecisionComment(id)], |tx| crate::ops::decision::remove_comment(tx, id))
    }

    /// Edit the body of a decision comment (one operation = one transaction). Not found if there is
    /// none.
    pub fn edit_decision_comment(
        &mut self,
        id: i64,
        text: &str,
    ) -> Result<crate::model::DecisionComment> {
        self.write_one(&[WriteTarget::DecisionComment(id)], |tx| crate::ops::decision::edit_comment(tx, id, text))
    }

    /// Create a `blob`-mode attachment (one operation = one transaction). The read of the trailing
    /// `order_key` happens inside this transaction. Ingesting the bytes themselves is the caller's
    /// job, out of band.
    #[allow(clippy::too_many_arguments)]
    pub fn attach_blob(
        &mut self,
        target_type: crate::model::AttachmentTarget,
        target_id: i64,
        blob_hash: &str,
        filename: &str,
        mime: Option<&str>,
        size_bytes: i64,
        created_by_kind: crate::model::ActorKind,
    ) -> Result<crate::model::Attachment> {
        self.write_one(&[WriteTarget::AttachTo(target_type, target_id)], |tx| {
            crate::ops::attachment::add_blob(
                tx, target_type, target_id, blob_hash, filename, mime, size_bytes,
                created_by_kind,
            )
        })
    }

    /// Create a `url`-mode attachment (one operation = one transaction).
    #[allow(clippy::too_many_arguments)]
    pub fn attach_url(
        &mut self,
        target_type: crate::model::AttachmentTarget,
        target_id: i64,
        url: &str,
        label: Option<&str>,
        created_by_kind: crate::model::ActorKind,
    ) -> Result<crate::model::Attachment> {
        self.write_one(&[WriteTarget::AttachTo(target_type, target_id)], |tx| {
            crate::ops::attachment::add_url(
                tx, target_type, target_id, url, label, created_by_kind,
            )
        })
    }

    /// Remove an attachment — a hard delete (one operation = one transaction). If nothing is
    /// attached, it is a no-op and returns `false`. For a blob attachment, the bytes are reclaimed
    /// too, once we have confirmed no other attachment points at them.
    pub fn remove_attachment(&mut self, attachment_id: i64) -> Result<bool> {
        let removed = self.write_one(&[WriteTarget::Attachment(attachment_id)], |tx| {
            crate::ops::attachment::remove(tx, attachment_id)
        })?;
        let orphaned: Vec<String> =
            removed.iter().filter_map(|r| r.blob_hash.clone()).collect();
        self.reclaim_after_delete(&orphaned);
        Ok(removed.is_some())
    }

    /// Drop from the binding registry the folder rows no live project claims
    /// ([`crate::binding::orphan_dirs`]) — this is what `doctor --fix` calls. It tidies an index, it
    /// does not destroy anything: it touches neither the folders' contents nor their `.amenbo`, so it
    /// is safe to run without asking for confirmation. Returns how many folders were forgotten.
    pub fn forget_orphan_dirs(&self) -> Result<usize> {
        let orphans = crate::binding::orphan_dirs(self);
        if orphans.is_empty() {
            return Ok(0);
        }
        let mut registry = self.bindings();
        for dir in &orphans {
            registry.forget_dir(dir);
        }
        self.save_bindings(&registry)?;
        Ok(orphans.len())
    }
}

impl Store {
    /// Write only the settings (`config.json`) to disk. The domain data has already been committed by
    /// the write wrappers, one transaction per operation, so this is what a surface (CLI, GUI) calls
    /// after it has changed `store.config`.
    pub fn save_config(&mut self) -> Result<()> {
        self.persist()
    }

    /// Write the settings to disk. The truth source for domain data is the engine (the write wrappers
    /// have already landed it, one transaction per operation), so all this is responsible for is
    /// persisting the config.
    pub(super) fn persist(&mut self) -> Result<()> {
        ensure_dir(&self.paths.base_dir)?;
        let cfg_json = serde_json::to_string_pretty(&self.config)?;
        write_atomic(&self.paths.config_file, cfg_json.as_bytes())?;
        Ok(())
    }
}
