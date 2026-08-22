//! Domain operations (CRUD, reordering, completion, rehoming).
//!
//! **Deletes are hard deletes** (`DELETE`). One rule governs them:
//!
//! **A subtree is removed explicitly by the delete op, children first** — never left to an implicit schema
//! `CASCADE`. That covers the entities (`project delete` removes that project's tasks, decisions and
//! dimensions before the project row itself) and equally the rows hanging off them: comments, dependency
//! edges, commit anchors, dimension values, assignments, decision links. Each of those is a row a person
//! can point at, and a row deleted by a constraint is deleted where no code can see it — so what goes must
//! go through an op that read its id first (`AMB-D-403`). Only Amenbo's own per-project settings
//! (`plugin_config` / `plugin_enable`) ride the schema, having nothing to tell.
//!
//! The polymorphic `attachment` (a reference discriminated by `target_type`) is the one child no constraint
//! *could* cover; the delete op sweeps its own with [`sweep_polymorphic`], ahead of the row it hangs off.
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
pub mod plugin_enable;
pub mod plugin_secret;
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
) -> Result<()> {
    if crosses_projects(a, b) {
        return Err(Error::invalid(
            format!("{what} would cross projects — a project's context must not leak into another"),
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

    /// Create one task in the given project (`None` for the inbox) and return its id. **Both stages of the
    /// creation** run here (`AMB-D-554`), so what the fixture hands back is a task there is nothing left to
    /// write on — which is what every test that reserves it, lists it or ends it is asking for. A test about
    /// the first stage calls `task::add` itself and stops there.
    pub(crate) fn mk_task_in(tx: &WriteTx<'_>, title: &str, project_id: Option<i64>) -> i64 {
        let id = super::task::add(
            tx,
            super::task::NewTask {
                title: title.to_string(),
                project_id,
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
                at_binding_id: None,
            },
        )
        .expect("add task")
        .id;
        super::task::finish_creating(tx, id).expect("finish creating the task");
        id
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

    /// One axis carrying one value, in the given project. Returns the value's id — what a classification
    /// names.
    fn mk_value(tx: &WriteTx<'_>, project_id: i64, axis: &str, value: &str) -> i64 {
        let dimension = super::dimension::add(
            tx,
            project_id,
            super::dimension::NewDimension {
                name: axis.to_string(),
                notes: String::new(),
                cardinality: crate::model::DimensionCardinality::Single,
                ordered: false,
                role: crate::model::DimensionRole::None,
                show_on_card: false,
                required: false,
                slug: None,
            },
        )
        .expect("add dimension")
        .id;
        super::dimension::value_add(tx, dimension, value, None).expect("add value").id
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

    /// Task↔dimension value (a classification). Naming another project's axis means reading what that
    /// project calls its axes and what values they offer — the same crossing as any other edge. An inbox
    /// task belongs to no project, so classifying it is no crossing.
    #[test]
    fn a_classification_cannot_cross_projects_but_the_inbox_is_not_a_project() {
        with_tx(|tx| {
            let (a, b) = (mk_project(tx, "A"), mk_project(tx, "B"));
            let va = mk_value(tx, a, "分類", "バグ");
            let vb = mk_value(tx, b, "分類", "バグ");
            let ta = mk_task_in(tx, "A のタスク", Some(a));
            let inbox = mk_task_in(tx, "受信箱のタスク", None);

            let e = super::dimension::set(tx, ta, vb).expect_err("a crossing must be rejected");
            assert_eq!(e.code(), "invalid_value", "{e}");
            super::dimension::set(tx, ta, va).expect("within one project it goes through");
            super::dimension::set(tx, inbox, vb).expect("the inbox has no project, so this is no crossing");
        });
    }

    /// The same re-check, for a classification: a task filed on this project's axis cannot be re-homed
    /// while it is still filed there.
    #[test]
    fn moving_a_task_cannot_leave_a_classification_crossing_projects() {
        with_tx(|tx| {
            let (a, b) = (mk_project(tx, "A"), mk_project(tx, "B"));
            let ta = mk_task_in(tx, "A のタスク", Some(a));
            let value = mk_value(tx, a, "分類", "バグ");
            super::dimension::set(tx, ta, value).expect("a classification within one project");

            let e = super::task::move_to(tx, ta, Some(b), super::Position::Bottom)
                .expect_err("a move that would leave a crossing must be rejected");
            assert_eq!(e.code(), "invalid_value", "{e}");

            // Clear the classification and it moves — the same "we never cut edges silently" rule the
            // dependency case follows.
            super::dimension::unset(tx, ta, value).expect("clear the classification");
            super::task::move_to(tx, ta, Some(b), super::Position::Bottom).expect("once cleared, it moves");
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

/// A delete op takes its own children — every row that stands for a concept goes through code, and the
/// database is left nothing to sweep behind it (`AMB-D-403`).
#[cfg(test)]
mod delete_children_tests {
    use super::test_support::{mk_project, mk_task_in};
    use crate::model::{ActorKind, DimensionCardinality, DimensionRole};
    use crate::store_engine::{StoreEngine, WriteTx};

    /// An engine with the `REFERENCES` unenforced, which is what keeps these assertions about the *op*.
    /// With no constraint acting, a child that is gone is a child the op deleted, and a child the op forgot
    /// is a row left behind here — rather than a parent `DELETE` the database refused, which is what the
    /// declarations do (`AMB-D-403`) and what the constraints' own tests are for.
    fn unenforced_engine() -> StoreEngine {
        StoreEngine::open_in_memory_unchecked().expect("in-memory engine")
    }

    /// Run `f` against one unenforced write transaction (never committed, as with `with_tx`).
    fn with_unenforced_tx(f: impl FnOnce(&WriteTx<'_>)) {
        let engine = unenforced_engine();
        let tx = engine.write().expect("write transaction");
        f(&tx);
    }

    /// One integer, straight off the source of truth — the tables are being counted for emptiness, which
    /// is precisely what the read layer (built for live graphs) does not offer.
    fn scalar(tx: &WriteTx<'_>, sql: &str) -> i64 {
        tx.conn().query_row(sql, [], |r| r.get(0)).expect("count rows")
    }

    fn mk_decision(tx: &WriteTx<'_>, project_id: i64, title: &str) -> i64 {
        super::decision::add(
            tx,
            super::decision::NewDecision { project_id, title: title.to_string(), body: String::new() },
        )
        .expect("add decision")
        .id
    }

    fn mk_dimension(tx: &WriteTx<'_>, project_id: i64, name: &str) -> i64 {
        super::dimension::add(
            tx,
            project_id,
            super::dimension::NewDimension {
                name: name.to_string(),
                notes: String::new(),
                cardinality: DimensionCardinality::Single,
                ordered: false,
                role: DimensionRole::None,
                show_on_card: false,
                required: false,
                slug: None,
            },
        )
        .expect("add dimension")
        .id
    }

    /// A project with everything hanging off it: two tasks (one blocking the other), a comment, a commit
    /// anchor and a classification on each, a decision linked to both, and a second decision naming the
    /// first. Returns `(project, task, other_task, decision, dimension, value)`.
    fn seed(tx: &WriteTx<'_>) -> (i64, i64, i64, i64, i64, i64) {
        let p = mk_project(tx, "消えるPJ");
        let t = mk_task_in(tx, "消えるタスク", Some(p));
        let other = mk_task_in(tx, "並びのタスク", Some(p));
        let d = mk_decision(tx, p, "この実装の根拠");
        let d2 = mk_decision(tx, p, "その上に立つ決定");
        let dim = mk_dimension(tx, p, "分類");
        let value = super::dimension::value_add(tx, dim, "バグ", None).expect("add value").id;

        super::comment::add_comment(tx, t, ActorKind::Ai, "作業メモ").expect("comment on the task");
        super::decision::add_comment(tx, d, ActorKind::Ai, "裁定の理由").expect("comment on the decision");
        super::dependency::add(tx, t, other, None).expect("a dependency edge");
        super::commit::add(tx, t, &"a".repeat(40), None).expect("a commit anchor");
        super::dimension::set(tx, t, value).expect("classify the task");
        super::decision::link(tx, d, t).expect("link the decision to the task");
        super::decision::link(tx, d, other).expect("and to the other task");
        super::decision::builds_on(tx, d2, d).expect("an edge naming the decision");

        (p, t, other, d, dim, value)
    }

    /// Deleting a task takes its comments, the edges at both of its ends, its commit anchors, its
    /// classifications and its decision links — and touches nothing that belongs to another task.
    #[test]
    fn deleting_a_task_takes_its_own_children_only() {
        with_unenforced_tx(|tx| {
            let (_, t, other, _, _, _) = seed(tx);

            super::task::delete(tx, t).expect("delete the task");

            let left = scalar(tx, &format!(
                "SELECT (SELECT COUNT(*) FROM task_comment WHERE task_id = {t})
                      + (SELECT COUNT(*) FROM task_dependency WHERE task_id = {t} OR blocked_by_id = {t})
                      + (SELECT COUNT(*) FROM task_commit WHERE task_id = {t})
                      + (SELECT COUNT(*) FROM task_dimension_value WHERE task_id = {t})
                      + (SELECT COUNT(*) FROM decision_task_link WHERE task_id = {t})"
            ));
            assert_eq!(left, 0, "no child of the deleted task is left behind");
            assert_eq!(
                scalar(tx, &format!("SELECT COUNT(*) FROM decision_task_link WHERE task_id = {other}")),
                1,
                "the other task's link to the same decision survives",
            );
        });
    }

    /// Deleting a decision takes its comments, the edges at both of its ends, and its task links.
    #[test]
    fn deleting_a_decision_takes_its_own_children() {
        with_unenforced_tx(|tx| {
            let (_, _, _, d, _, _) = seed(tx);

            super::decision::delete(tx, d).expect("delete the decision");

            let left = scalar(tx, &format!(
                "SELECT (SELECT COUNT(*) FROM decision_comment WHERE decision_id = {d})
                      + (SELECT COUNT(*) FROM decision_edge
                         WHERE decision_id = {d} OR target_decision_id = {d})
                      + (SELECT COUNT(*) FROM decision_task_link WHERE decision_id = {d})"
            ));
            assert_eq!(left, 0, "no child of the deleted decision is left behind");
        });
    }

    /// Deleting a dimension takes its values, and each value the assignments naming it. Deleting one
    /// value alone takes the same assignments — the axis-wide sweep is that step repeated.
    #[test]
    fn deleting_a_dimension_takes_its_values_and_their_assignments() {
        with_unenforced_tx(|tx| {
            let (_, _, _, _, dim, value) = seed(tx);

            super::dimension::delete(tx, dim).expect("delete the dimension");

            assert_eq!(
                scalar(tx, &format!("SELECT COUNT(*) FROM dimension_value WHERE dimension_id = {dim}")),
                0,
                "the axis's values go with it",
            );
            assert_eq!(
                scalar(tx, &format!("SELECT COUNT(*) FROM task_dimension_value WHERE value_id = {value}")),
                0,
                "and the classifications on them",
            );
        });
    }

    /// Deleting a project runs every one of those sweeps: nothing anywhere in the store still names a row
    /// the delete took.
    #[test]
    fn deleting_a_project_leaves_no_child_row_anywhere() {
        with_unenforced_tx(|tx| {
            let (p, ..) = seed(tx);

            super::project::delete(tx, p).expect("delete the project");

            for table in [
                "task",
                "task_comment",
                "task_dependency",
                "task_commit",
                "task_dimension_value",
                "decision",
                "decision_comment",
                "decision_edge",
                "decision_task_link",
                "dimension",
                "dimension_value",
            ] {
                assert_eq!(
                    scalar(tx, &format!("SELECT COUNT(*) FROM {table}")),
                    0,
                    "{table} is empty once the project that owned every row is deleted",
                );
            }
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

/// Read a reference string as an **id** — `AMB-P-<n>` as Amenbo renders it, or the bare `<n>`. The id
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
        0 => Err(Error::not_found(format!("'{input}' not found"))),
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
        0 => Err(Error::not_found(format!("'{number}' names neither {task} nor {decision}"))),
        1 => Ok(hits.into_iter().next().unwrap()),
        // Numbers are device-global, so a bare number is ambiguous only across the two number spaces — a
        // task and a decision. Quote both refs: the kind code is exactly what disjoins them.
        _ => Err(Error::invalid(format!("{number} is both a task and a decision; use {task} or {decision}"))),
    }
}

/// An entity's name, so the `not_found` of `#N` / id resolution has a single source. Each ops module holds
/// the word for its own entity once, as a `const NOUN`, and leaves message building to this.
///
/// The `code` is the same word again, in the form the GUI writes the sentence from (`AMB-D-413`). It sits
/// here rather than at the call sites because the noun *is* the sentence: nothing of "task X was not found"
/// survives into "dimension X was not found" except the id, so there is one code per entity and each one
/// gets its own template.
#[derive(Clone, Copy)]
pub(crate) struct Noun {
    pub en: &'static str,
    pub code: crate::error::ErrorCode,
}

impl Noun {
    /// The refusal `<en> '<token>' not found` — and, for the side that writes it in another language,
    /// this entity's code and the token it could not find.
    pub fn not_found(self, token: impl std::fmt::Display) -> Error {
        Error::NotFound(
            crate::error::Msg::new(format!("{} '{token}' not found", self.en))
            .coded(self.code)
            .with("ref", token),
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
            return Err(Error::invalid("specify exactly one of --before / --after / --top / --bottom"));
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
            Error::not_found(format!("reorder anchor '{id}' not found in the same ordering"))
        })
}

