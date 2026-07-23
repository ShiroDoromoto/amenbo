//! Task dependencies. `task_id` depends on `blocked_by_id` (the latter should be done first).
//!
//! The relation is carried by a standalone `TaskDependency` edge, and detaching it is a hard delete. The
//! derived state (ready / blocked_by_open) is computed at read time, over in `view` — status is never
//! rewritten on its behalf.
//!
//! Writes ride the same seam as `task` ([`crate::ops::emit_create`] / [`crate::ops::emit_update`]). With
//! `ON DELETE CASCADE` on both ends, an edge dies in the **same statement** as the task it hangs off — there
//! is no gap in which a dangling edge could be created.
//!
//! Everything the degenerate-case checks judge on (both ends alive, an existing edge, a cycle) is read from
//! the source of truth **inside the same transaction**. Read the cycle check outside it and two concurrent
//! `add`s, each looking at an acyclic graph, can commit a cycle between them.

use crate::error::{Error, Result};
use crate::model::{ActorKind, TaskDependency};
use crate::ops::emit_create;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Create the edge "`task_id` depends on `blocked_by_id`" (the latter should be done first). The degenerate
/// cases — self-reference, cycles, dangling ends — are rejected. Idempotent: an edge that already exists
/// yields `created=false`.
pub fn add(
    tx: &WriteTx<'_>,
    task_id: i64,
    blocked_by_id: i64,
    created_by_kind: Option<ActorKind>,
) -> Result<(TaskDependency, bool)> {
    if task_id == blocked_by_id {
        return Err(Error::invalid("a task cannot depend on itself", "タスクは自分自身に依存できません"));
    }
    // Both ends must be live, existing tasks (this is what keeps edges from dangling).
    let Some(task) = read::task(tx.conn(), task_id)? else {
        return Err(Error::not_found(format!("task '{task_id}' not found"), format!("タスク '{task_id}' が見つかりません")));
    };
    let Some(blocker) = read::task(tx.conn(), blocked_by_id)? else {
        return Err(Error::not_found(format!("blocker '{blocked_by_id}' not found"), format!("ブロッカー '{blocked_by_id}' が見つかりません")));
    };
    // Naming a task in another project as a blocker means going off to read that task's notes and comments
    // — one project's context flowing into the other.
    crate::ops::guard_same_project(
        task.project_id,
        blocker.project_id,
        "this dependency",
        "この依存",
    )?;
    if let Some(id) = read::dependency_id(tx.conn(), task_id, blocked_by_id)? {
        let existing = read::task_dependency(tx.conn(), id)?
            .expect("the edge id was just read from the same transaction");
        return Ok((existing, false));
    }
    // Cycle check: if `task_id` is reachable by walking out from `blocked_by_id`, this edge closes a loop.
    if read::dependency_reaches(tx.conn(), blocked_by_id, task_id)? {
        return Err(Error::invalid(
            "this dependency would create a cycle (the blocker depends on this task directly or indirectly)",
            "この依存は循環を作ります（ブロッカーが直接/間接にこのタスクへ依存しています）",
        ));
    }
    let now = Timestamp::now();
    let edge = TaskDependency {
        // `next_id` takes the real table name; the `dependency` dataset maps to table `task_dependency`.
        id: read::next_id(tx.conn(), "task_dependency")?,
        task_id,
        blocked_by_id,
        created_by_kind,
        // The instant the edge was established — the intent column the premise-change judgement reads
        // (`AMB-D-372`). Stamped here, once, and never rewritten.
        established_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::dependency(&edge))?;
    Ok((edge, true))
}

/// Detach a dependency edge (a hard delete). An edge that is not there is a no-op (idempotent); the return
/// value is `changed`. There is no cleanup here for task deletion: both `task_id` and `blocked_by_id` are
/// `ON DELETE CASCADE`, so deleting a task lets the schema take the edge from either side.
pub fn remove(tx: &WriteTx<'_>, task_id: i64, blocked_by_id: i64) -> Result<bool> {
    let Some(id) = read::dependency_id(tx.conn(), task_id, blocked_by_id)? else {
        return Ok(false);
    };
    tx.delete_record("dependency", id)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_task, with_tx};
    use crate::store_engine::read;

    #[test]
    fn add_is_idempotent_and_rejects_self_reference() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            let b = mk_task(tx, "b");
            let (_e, created) = add(tx, a, b, None).unwrap();
            assert!(created);
            let (_e, created2) = add(tx, a, b, None).unwrap();
            assert!(!created2);
            assert!(read::dependency_id(tx.conn(), a, b).unwrap().is_some());
            // A self-reference is rejected.
            assert!(add(tx, a, a, None).is_err());
        });
    }

    #[test]
    fn add_rejects_cycles_direct_and_indirect() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            let b = mk_task(tx, "b");
            let c = mk_task(tx, "c");
            // a→b, b→c.
            add(tx, a, b, None).unwrap();
            add(tx, b, c, None).unwrap();
            // c→a would close the loop a→b→c→a — rejected.
            assert!(add(tx, c, a, None).is_err());
            // b→a would close a→b→a — rejected too.
            assert!(add(tx, b, a, None).is_err());
        });
    }

    #[test]
    fn remove_deletes_the_edge_and_is_idempotent() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            let b = mk_task(tx, "b");
            add(tx, a, b, None).unwrap();
            assert!(remove(tx, a, b).unwrap());
            assert!(!remove(tx, a, b).unwrap());
            // Once removed, the edge can be created again.
            let (_e, created) = add(tx, a, b, None).unwrap();
            assert!(created);
        });
    }

    #[test]
    fn add_rejects_dangling_blocker() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            assert!(add(tx, a, 9999, None).is_err());
        });
    }

    /// Delete a task and the schema's `ON DELETE CASCADE` (declared on both ends) takes the edges on either
    /// side of it — the ones it depends on and the ones that depend on it. The ops layer needs no cleanup
    /// code of its own.
    #[test]
    fn deleting_a_task_cascades_the_edges_on_both_ends() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            let b = mk_task(tx, "b");
            let c = mk_task(tx, "c");
            add(tx, a, b, None).unwrap(); // b blocks a
            add(tx, b, c, None).unwrap(); // c blocks b

            crate::ops::task::delete(tx, b).unwrap();
            assert!(read::dependency_id(tx.conn(), a, b).unwrap().is_none(), "the edge into b");
            assert!(read::dependency_id(tx.conn(), b, c).unwrap().is_none(), "the edge out of b");
        });
    }
}
