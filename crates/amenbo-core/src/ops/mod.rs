//! Domain operations (CRUD, reordering, completion, rehoming).
//!
//! **Deletes are hard deletes** (`DELETE`). Two rules govern them:
//!
//! 1. **A subtree of entities is removed explicitly by the delete op, children first** — never left to an
//!    implicit schema `CASCADE`. `project delete` removes that project's tasks, decisions and dimensions
//!    before the project row itself (the schema side is `RESTRICT`, so getting the order wrong fails loudly
//!    instead of silently orphaning rows).
//! 2. **Links and dependent content ride the schema's `CASCADE`** (dependency edges, comments, dimension
//!    values, assignments). The exception is the polymorphic `attachment` (a reference discriminated by
//!    `target_type`), which no constraint can cover — so the delete op sweeps its own with
//!    [`sweep_polymorphic`].
//!
//! **Mutations issue SQL straight at the source of truth (the read-model).** Every mutator takes only the
//! [`WriteTx`] (`BEGIN IMMEDIATE`) the caller opened, and reads both its `before` snapshot and any existence
//! check **inside that transaction**, through [`crate::store_engine::read`].

pub mod attachment;
pub mod commit;
pub mod decision;
pub mod dependency;
pub mod dimension;
pub mod comment;
pub mod plugin_config;
pub mod project;
pub mod task;
pub mod user;

use crate::error::{Error, Result};
use crate::order::key_between;
use crate::store_engine::{Record, WriteTx};

/// **A project boundary is a context boundary.** Rejects any edge that spans projects — decision↔decision
/// (supersedes / amends / builds_on), decision↔task, task↔task (dependency). Each project is worked by its
/// own AI agent, and an agent assembles its context by **following the references it can follow**. A
/// crossing edge is one of those references: the moment it exists, the contents of one project's decisions
/// and tasks have a path into the other's context. **The inbox (`project_id` of `None`) belongs to no
/// project**, so an edge with the inbox on either end is no crossing at all and goes through — only two
/// *real, distinct* projects break this invariant.
pub(crate) fn crosses_projects(a: Option<i64>, b: Option<i64>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x != y)
}

/// [`crosses_projects`] as a guard: rejects the edge, naming what it was.
pub(crate) fn guard_same_project(
    a: Option<i64>,
    b: Option<i64>,
    what: &str,
    what_ja: &str,
) -> Result<()> {
    if crosses_projects(a, b) {
        return Err(Error::invalid(
            format!("{what} would cross projects — a project's context must not leak into another"),
            format!("{what_ja}はプロジェクトを跨ぎます ── あるプロジェクトの文脈を別のプロジェクトへ流してはいけません"),
        ));
    }
    Ok(())
}

/// Emit every field of a newly created record to the source of truth (**one logical operation = one
/// transaction**). *What* gets written is decided solely by the projection in
/// [`crate::store_engine::record`]; all this function decides is the **destination** — this operation's
/// `BEGIN IMMEDIATE` transaction ([`WriteTx`]). A `?` on the way out drops the guard before commit and
/// **nothing lands**: a partially written row is structurally impossible.
pub(crate) fn emit_create(tx: &WriteTx<'_>, rec: Record) -> Result<()> {
    for (col, val) in rec.fields {
        tx.set_field(rec.dataset, rec.id, col, val)?;
    }
    Ok(())
}

/// Emit only the columns an update actually changed (same destination rule as [`emit_create`]). The `before`
/// snapshot and the existence check are read from the same `tx` ([`crate::store_engine::read`]), so the write
/// lands **on top of the state it was computed from** — take `before` outside the transaction and another
/// process can write the same row in between, making the diff read as "unchanged" and silently dropping the
/// write.
pub(crate) fn emit_update(tx: &WriteTx<'_>, before: Record, after: Record) -> Result<()> {
    for (col, val) in after.changed_from(&before) {
        tx.set_field(after.dataset, after.id, col, val)?;
    }
    Ok(())
}

/// Hard-delete the polymorphic children — `attachment` rows — of an entity being deleted. What
/// `attachment.target_id` points at is decided by its sibling column `target_type`, so it can carry no
/// `REFERENCES` clause and the schema's `ON DELETE` will not look after it — the delete op that removes the
/// parent sweeps them instead (leave them and the row outlives the entity it hung off, unreachable, while
/// its `blob_hash` stays in the GC root set and keeps the bytes alive for good). Returns the blob hashes the
/// deleted attachments pointed at: the bytes live out of band, so they do not go with the row — they are
/// reclaimed only once nothing references them, and this is the only moment the candidates are knowable
/// (with the rows gone there is nobody left to ask which blobs were orphaned). **Reclamation happens after
/// commit**, so all we do here is hand the candidates back.
pub(crate) fn sweep_polymorphic(
    tx: &WriteTx<'_>,
    target_type: &str,
    target_id: i64,
) -> Result<Vec<String>> {
    let orphaned = crate::store_engine::read::blob_hashes_for_target(tx.conn(), target_type, target_id)?;
    tx.delete_records_for_target(target_type, target_id)?;
    Ok(orphaned)
}

/// Scaffolding for the ops unit tests. Mutations issue SQL straight at the source of truth, so the tests open
/// an in-memory engine too and run **inside a single transaction** (reads through the same `tx` see the
/// writes even with no commit).
#[cfg(test)]
pub(crate) mod test_support {
    use crate::store_engine::{StoreEngine, WriteTx};

    /// Open an in-memory engine and hand one write transaction to `f` (never committed — every assertion
    /// reads back through the same `tx`).
    pub(crate) fn with_tx(f: impl FnOnce(&WriteTx<'_>)) {
        let engine = new_engine();
        let tx = engine.write().expect("write transaction");
        f(&tx);
    }

    /// An empty in-memory engine (`let tx = &e.write().unwrap();` opens a write transaction).
    pub(crate) fn new_engine() -> StoreEngine {
        StoreEngine::open_in_memory().expect("in-memory engine")
    }

    /// Create one inbox task (no project) and return its id.
    pub(crate) fn mk_task(tx: &WriteTx<'_>, title: &str) -> i64 {
        mk_task_in(tx, title, None)
    }

    /// Create one task in the given project (`None` for the inbox) and return its id.
    pub(crate) fn mk_task_in(tx: &WriteTx<'_>, title: &str, project_id: Option<i64>) -> i64 {
        super::task::add(
            tx,
            super::task::NewTask {
                title: title.to_string(),
                project_id,
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
            },
        )
        .expect("add task")
        .id
    }

    /// Create one project and return its id.
    pub(crate) fn mk_project(tx: &WriteTx<'_>, name: &str) -> i64 {
        super::project::add(
            tx,
            super::project::NewProject {
                name: name.to_string(),
                view: crate::model::View::List,
                notes: String::new(),
                color: None,
            },
        )
        .expect("add project")
        .id
    }

}

/// No edge may cross a project boundary — from any entry point.
#[cfg(test)]
mod cross_project_tests {
    use super::test_support::{mk_project, mk_task_in, with_tx};
    use crate::store_engine::WriteTx;

    fn mk_decision(tx: &WriteTx<'_>, project_id: i64, title: &str) -> i64 {
        super::decision::add(
            tx,
            super::decision::NewDecision { project_id, title: title.to_string(), body: String::new() },
        )
        .expect("add decision")
        .id
    }

    /// Decision↔decision (supersede / amend / builds_on all funnel into `put_edge`).
    #[test]
    fn a_decision_edge_cannot_cross_projects() {
        with_tx(|tx| {
            let (a, b) = (mk_project(tx, "A"), mk_project(tx, "B"));
            let (da, db) = (mk_decision(tx, a, "A の決定"), mk_decision(tx, b, "B の決定"));
            let same = mk_decision(tx, a, "A のもう 1 つ");

            for r in [
                super::decision::supersede(tx, db, da, None).err(),
                super::decision::amend(tx, db, da).err(),
                super::decision::builds_on(tx, db, da).err(),
            ] {
                let e = r.expect("a crossing must be rejected");
                assert_eq!(e.code(), "invalid_value", "{e}");
            }
            // Within one project it goes through (what we blocked is the crossing, not the edge itself).
            super::decision::builds_on(tx, same, da).expect("within one project it goes through");
        });
    }

    /// Decision↔task. An inbox task belongs to no project, so it constitutes no crossing and goes through.
    #[test]
    fn a_decision_task_link_cannot_cross_projects_but_the_inbox_is_not_a_project() {
        with_tx(|tx| {
            let (a, b) = (mk_project(tx, "A"), mk_project(tx, "B"));
            let da = mk_decision(tx, a, "A の決定");
            let tb = mk_task_in(tx, "B のタスク", Some(b));
            let ta = mk_task_in(tx, "A のタスク", Some(a));
            let inbox = mk_task_in(tx, "受信箱のタスク", None);

            let e = super::decision::link(tx, da, tb).expect_err("a crossing must be rejected");
            assert_eq!(e.code(), "invalid_value", "{e}");
            super::decision::link(tx, da, ta).expect("within one project it goes through");
            super::decision::link(tx, da, inbox).expect("the inbox has no project, so this is no crossing");
        });
    }

    /// Task↔task (dependency). A task in another project cannot be named as a blocker.
    #[test]
    fn a_dependency_cannot_cross_projects() {
        with_tx(|tx| {
            let (a, b) = (mk_project(tx, "A"), mk_project(tx, "B"));
            let ta = mk_task_in(tx, "A のタスク", Some(a));
            let ta2 = mk_task_in(tx, "A のもう 1 つ", Some(a));
            let tb = mk_task_in(tx, "B のタスク", Some(b));

            let e = super::dependency::add(tx, ta, tb, None).expect_err("a crossing must be rejected");
            assert_eq!(e.code(), "invalid_value", "{e}");
            super::dependency::add(tx, ta, ta2, None).expect("within one project it goes through");
        });
    }

    /// An edge that was legal when it was made becomes a crossing once one of its ends moves — so moves are
    /// checked too. A guard at creation time alone cannot hold the invariant.
    #[test]
    fn moving_a_task_cannot_leave_an_edge_crossing_projects() {
        with_tx(|tx| {
            let (a, b) = (mk_project(tx, "A"), mk_project(tx, "B"));
            let ta = mk_task_in(tx, "A のタスク", Some(a));
            let blocker = mk_task_in(tx, "A のブロッカー", Some(a));
            super::dependency::add(tx, ta, blocker, None).expect("a dependency within one project");

            let e = super::task::move_to(tx, ta, Some(b), super::Position::Bottom)
                .expect_err("a move that would leave a crossing must be rejected");
            assert_eq!(e.code(), "invalid_value", "{e}");

            // Detach the dependency and it moves (whether to detach is the caller's call — we never cut
            // edges silently).
            super::dependency::remove(tx, ta, blocker).expect("detach the dependency");
            super::task::move_to(tx, ta, Some(b), super::Position::Bottom).expect("once detached, it moves");
        });
    }
}

/// What a conversational reference points at. Tasks and decisions have **separate number spaces**, so the
/// type prefixes `T-47` / `D-47` let a reference name its type. A bare `#47` is looked up across both spaces
/// — if the same number exists in each, it is ambiguous (and the error asks for a prefix).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ref {
    Task(i64),
    Decision(i64),
}

impl Ref {
    /// The id referenced (both are INTEGER primary keys — the conversational number itself).
    pub fn id(&self) -> i64 {
        match self {
            Ref::Task(id) | Ref::Decision(id) => *id,
        }
    }
}

/// Read a reference string as an **id** — `AMB-P-<n>` as amenbo renders it, or the bare `<n>`. The id
/// *is* the conversational number, so this one parser covers the id arm of every resolver that takes either
/// a name or an id (project / dimension / dimension value). Empty, blank or non-decimal input gives `None` —
/// as an id it matches nothing.
///
/// `kind` scopes which namespaced form is accepted, so a ref from the wrong number space (`AMB-T-<n>` at a
/// project resolver) stays unreadable rather than resolving to that project.
pub(crate) fn parse_id_ref(kind: crate::idref::RefKind, reference: &str) -> Option<i64> {
    let s = crate::idref::strip(kind, reference.trim());
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

pub(crate) fn pick(mut hits: Vec<Ref>, input: &str) -> Result<Ref> {
    match hits.len() {
        0 => Err(Error::not_found(
            format!("'{input}' not found"),
            format!("'{input}' が見つかりません"),
        )),
        1 => Ok(hits.pop().unwrap()),
        _ => Err(Error::AmbiguousId {
            prefix: input.to_string(),
            candidates: hits.iter().map(|r| r.id().to_string()).collect(),
        }),
    }
}

pub(crate) fn pick_anywhere(hits: Vec<Ref>, number: u32) -> Result<Ref> {
    let (task, decision) = (crate::idref::task(number.into()), crate::idref::decision(number.into()));
    match hits.len() {
        0 => Err(Error::not_found(
            format!("'{number}' names neither {task} nor {decision}"),
            format!("'{number}' は {task} にも {decision} にも当たりません"),
        )),
        1 => Ok(hits.into_iter().next().unwrap()),
        // Numbers are device-global, so a bare number is ambiguous only across the two number spaces — a
        // task and a decision. Quote both refs: the kind code is exactly what disjoins them.
        _ => Err(Error::invalid(
            format!("{number} is both a task and a decision; use {task} or {decision}"),
            format!("{number} はタスクと決定の両方にあります。{task} か {decision} で指定してください"),
        )),
    }
}

/// An entity's name as an English/Japanese pair, so the `not_found` of `#N` / id resolution has a single
/// source. Each ops module holds the word for its own entity once, as a `const NOUN`, and leaves message
/// building to this.
#[derive(Clone, Copy)]
pub(crate) struct Noun {
    pub en: &'static str,
    pub ja: &'static str,
}

impl Noun {
    /// The bilingual error pair `<en> '<token>' not found`, with its Japanese counterpart.
    pub fn not_found(self, token: impl std::fmt::Display) -> Error {
        Error::not_found(
            format!("{} '{token}' not found", self.en),
            format!("{} '{token}' が見つかりません", self.ja),
        )
    }
}

/// How key / name / number resolution converges: no hit → `not_found()`, exactly one → resolved, several →
/// `AmbiguousId` listing the candidates. Each module keeps only its own predicate — which set it narrows and
/// how — and leaves the convergence here.
pub(crate) fn pick_id<T: ToString>(
    mut hits: Vec<T>,
    token: &str,
    not_found: impl FnOnce() -> Error,
) -> Result<T> {
    match hits.len() {
        0 => Err(not_found()),
        1 => Ok(hits.pop().unwrap()),
        _ => Err(Error::AmbiguousId {
            prefix: token.to_string(),
            candidates: hits.iter().map(ToString::to_string).collect(),
        }),
    }
}

/// Where a reorder puts the item (`--before` / `--after` / `--top` / `--bottom`).
#[derive(Clone, Debug)]
pub enum Position {
    Top,
    Bottom,
    /// Before this anchor (an id within the same ordering).
    Before(i64),
    /// After this anchor.
    After(i64),
}

impl Position {
    /// Build a `Position` from the CLI flags. More than one is an error; none means `Bottom`.
    pub fn from_flags(
        top: bool,
        bottom: bool,
        before: Option<i64>,
        after: Option<i64>,
    ) -> Result<Position> {
        let count = top as u8 + bottom as u8 + before.is_some() as u8 + after.is_some() as u8;
        if count > 1 {
            return Err(Error::invalid(
                "specify exactly one of --before / --after / --top / --bottom",
                "--before / --after / --top / --bottom はいずれか 1 つだけ指定してください",
            ));
        }
        Ok(match (top, bottom, before, after) {
            (_, _, Some(id), _) => Position::Before(id),
            (_, _, _, Some(id)) => Position::After(id),
            (true, _, _, _) => Position::Top,
            _ => Position::Bottom,
        })
    }
}

/// Compute the key for the given position, from an ascending list of `(id, order_key)` siblings (the item
/// being moved is already excluded).
///
/// The anchor of `Before`/`After` must be an id in that sibling set (not_found if it is not).
pub fn place(siblings: &[(i64, String)], pos: &Position) -> Result<String> {
    let key = match pos {
        Position::Top => key_between(None, siblings.first().map(|(_, k)| k.as_str())),
        Position::Bottom => key_between(siblings.last().map(|(_, k)| k.as_str()), None),
        Position::Before(id) => {
            let idx = anchor_index(siblings, *id)?;
            let prev = if idx == 0 {
                None
            } else {
                Some(siblings[idx - 1].1.as_str())
            };
            key_between(prev, Some(siblings[idx].1.as_str()))
        }
        Position::After(id) => {
            let idx = anchor_index(siblings, *id)?;
            let next = siblings.get(idx + 1).map(|(_, k)| k.as_str());
            key_between(Some(siblings[idx].1.as_str()), next)
        }
    };
    Ok(key)
}

fn anchor_index(siblings: &[(i64, String)], id: i64) -> Result<usize> {
    siblings
        .iter()
        .position(|(i, _)| *i == id)
        .ok_or_else(|| {
            Error::not_found(
                format!("reorder anchor '{id}' not found in the same ordering"),
                format!("並べ替えアンカー '{id}' が同じ並びに見つかりません"),
            )
        })
}

