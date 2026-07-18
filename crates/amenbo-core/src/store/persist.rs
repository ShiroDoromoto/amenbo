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
    /// without it.
    pub fn add_task(&mut self, input: crate::ops::task::NewTask) -> Result<crate::model::Task> {
        let today = crate::time::today();
        let home = WriteTarget::NewIn(input.project_id);
        self.write_one(&[home], |tx| {
            let task = crate::ops::task::add(tx, input)?;
            if let Some(project_id) = task.project_id {
                let value_id =
                    crate::store_engine::read::current_time_axis_value(tx.conn(), project_id, today)?;
                if let Some(value_id) = value_id {
                    crate::ops::dimension::set(tx, task.id, value_id)?;
                }
            }
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

    /// Assign or unassign a task's assignee (one operation = one transaction).
    pub fn set_task_assignee(
        &mut self,
        id: i64,
        kind: Option<crate::model::ActorKind>,
    ) -> Result<crate::model::Task> {
        self.write_one(&[WriteTarget::Task(id)], |tx| crate::ops::task::set_assignee(tx, id, kind))
    }

    /// Set a task's status (one operation = one transaction). The compare-and-swap that reserves a
    /// task (`todo → in_progress`) reads the truth source's current status inside this transaction.
    pub fn set_task_status(
        &mut self,
        id: i64,
        status: crate::model::TaskStatus,
    ) -> Result<crate::model::Task> {
        self.write_one(&[WriteTarget::Task(id)], |tx| crate::ops::task::set_status(tx, id, status))
    }

    /// Done / reopen — sugar over `set_task_status` (one operation = one transaction).
    pub fn set_task_completed(&mut self, id: i64, completed: bool) -> Result<crate::model::Task> {
        self.write_one(&[WriteTarget::Task(id)], |tx| crate::ops::task::set_completed(tx, id, completed))
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
            let orphaned = crate::ops::task::delete(tx, id)?;
            let entry = crate::activity_log::Entry {
                id: activity_id,
                at: crate::time::Timestamp::now(),
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
    ) -> Result<crate::model::Task> {
        self.write_one(
            // Re-homing touches the destination too: both where it comes from and where it lands
            // have to be within reach.
            &[WriteTarget::Task(id), WriteTarget::NewIn(target_project)],
            |tx| crate::ops::task::move_to(tx, id, target_project, pos),
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
    pub fn project_delete(&mut self, id: i64, actor: crate::model::ActorKind) -> Result<()> {
        let (orphaned, entry) = self.write_one(&[WriteTarget::Project(id)], |tx| {
            use crate::store_engine::read;
            let name = read::project_name(tx.conn(), id)?;
            let tasks = read::task_ids_in_project(tx.conn(), id)?.len();
            let decisions = read::decision_ids_in_project(tx.conn(), id)?.len();
            let activity_id = mint_activity_id(tx)?;
            let orphaned = crate::ops::project::delete(tx, id)?;
            let entry = crate::activity_log::Entry {
                id: activity_id,
                at: crate::time::Timestamp::now(),
                actor: Some(actor),
                project: Some(id),
                task: None,
                decision: None,
                event: crate::activity_log::event::project_deleted(name.as_deref(), tasks, decisions),
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
            crate::ops::comment::add_comment(tx, task_id, author_kind, text)
        })
    }

    /// Hard-delete a task comment (one operation = one transaction). If there is none, it is a no-op
    /// and returns `false`.
    pub fn remove_task_comment(&mut self, id: i64) -> Result<bool> {
        self.write_one(&[WriteTarget::TaskComment(id)], |tx| crate::ops::comment::remove_comment(tx, id))
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

    /// Accept a decision (one operation = one transaction).
    pub fn accept_decision(
        &mut self,
        id: i64,
        decided_by: Option<String>,
    ) -> Result<crate::model::Decision> {
        self.write_one(&[WriteTarget::Decision(id)], |tx| crate::ops::decision::accept(tx, id, decided_by))
    }

    /// Reject a decision (one operation = one transaction).
    pub fn reject_decision(&mut self, id: i64) -> Result<crate::model::Decision> {
        self.write_one(&[WriteTarget::Decision(id)], |tx| crate::ops::decision::reject(tx, id))
    }

    /// Return an accepted decision to discussion (one operation = one transaction).
    pub fn reopen_decision(&mut self, id: i64) -> Result<crate::model::Decision> {
        self.write_one(&[WriteTarget::Decision(id)], |tx| crate::ops::decision::reopen(tx, id))
    }

    /// Supersede one decision with another (one operation = one transaction). Inserting the
    /// `supersedes` edge and promoting the new decision ride together; the old decision's row is left
    /// untouched, because whether it is current is derived from the edges.
    pub fn supersede_decision(
        &mut self,
        new_id: i64,
        old_id: i64,
        decided_by: Option<String>,
    ) -> Result<crate::model::Decision> {
        self.write_one(
            &[WriteTarget::Decision(new_id), WriteTarget::Decision(old_id)],
            |tx| crate::ops::decision::supersede(tx, new_id, old_id, decided_by),
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
