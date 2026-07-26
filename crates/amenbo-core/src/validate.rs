//! `validate` (a side-effect-free shape check) and `doctor` (a data-integrity check).
//!
//! Both follow the `--json` contract: the DTO serialization in [`crate::view`] and
//! [`crate::error::Error::code`].
//!
//! **An issue carries no sentence.** As with doctor, the core returns a template id ([`IssueRule`]) and the
//! difference (target / field / got / expected); the sentence a person reads is assembled by the surface.
//! `validate` has only one surface — the CLI — so there is one wording, in English (`validate_text` in
//! `amenbo-cli`). That does not change the shape an AI relies on: `--json`'s `fix_hint` is still something
//! it can run, and the CLI is what puts that English sentence there.

use std::collections::HashSet;

use rusqlite::{Connection, Row};
use serde::{Serialize, Serializer};

use crate::idref::RefKind;
use crate::reach::Reach;


use crate::store_engine::schema::col;
use crate::store_engine::sql::{same, Expr, Pred, Select, Slot, Sort, Sql};
use crate::store_engine::{StoreEngineError, Result as StoreEngineResult};

/// A rule that was broken. It is the **template id for the sentence a surface assembles**, and it is also
/// part of the machine contract that appears in `--json` — the same pattern as
/// [`crate::doctor::DoctorIssueKind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueRule {
    /// A required field is empty.
    Required,
}

impl IssueRule {
    /// The contractual set. A CLI-side test checks that the surface's templates cover all of it.
    pub const ALL: &'static [IssueRule] = &[Self::Required];

    /// The one and only place a rule string is written — `Serialize` goes through here too.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
        }
    }
}

impl Serialize for IssueRule {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// One issue raised by validate. **It has no sentence** — a surface assembles one by filling target / field
/// into the [`IssueRule`] template.
#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    pub target: String,
    pub field: String,
    pub rule: IssueRule,
    pub severity: String,
    pub got: String,
    pub expected: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub error: usize,
    pub warning: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ValidateResult {
    pub ok: bool,
    pub checked: usize,
    pub issues: Vec<Issue>,
    pub summary: Summary,
}

/// The SQL path behind `validate`. It pulls only the `(id, title)` of live tasks from the read-model
/// ([`crate::store_engine::read::live_task_titles`]) instead of hydrating the whole store; the checks are
/// per-task field checks with no joins. Issues come back ordered by ascending `id`.
///
/// When `reach` is closed, only the bound project's tasks are checked — neither `checked` nor any issue
/// reaches beyond it. Diagnostics look like a surface that merely asks "is anything broken?", but what they
/// return is the id and title of tasks; opening them up would let another project into the AI's context.
pub fn validate(conn: &Connection, target_ids: &[String], reach: Reach) -> StoreEngineResult<ValidateResult> {
    let tasks: Vec<(String, String)> = crate::store_engine::read::live_task_titles(conn, reach.project())?
        .into_iter()
        .filter(|(id, _)| {
            target_ids.is_empty()
                || target_ids.iter().any(|r| crate::ops::parse_id_ref(crate::idref::RefKind::Task, r).map(|n| n.to_string()).as_ref() == Some(id))
        })
        .collect();

    let mut issues = Vec::new();
    for (id, title) in &tasks {
        // title is required
        if title.trim().is_empty() {
            issues.push(Issue {
                target: format!("task:{id}"),
                field: "title".to_string(),
                rule: IssueRule::Required,
                severity: "error".to_string(),
                got: title.clone(),
                expected: "non-empty string".to_string(),
            });
        }
    }

    let error = issues.iter().filter(|i| i.severity == "error").count();
    let warning = issues.iter().filter(|i| i.severity == "warning").count();
    Ok(ValidateResult {
        ok: error == 0,
        checked: tasks.len(),
        issues,
        summary: Summary { error, warning },
    })
}

// ───────────────────────── doctor ─────────────────────────

// The issue types (`DoctorIssue` / `DoctorIssueKind`) live in `crate::doctor`, which owns the doctor
// surface. This module — the in-store checks — is a producer of issues in that same type, shared with the
// environment checks, and it holds no sentence for a surface to read out.
pub use crate::doctor::{DoctorIssue, DoctorIssueKind};

#[derive(Clone, Debug, Serialize)]
pub struct DoctorResult {
    pub ok: bool,
    pub issues: Vec<DoctorIssue>,
    pub summary: Summary,
}

// ─────────────────────── doctor ───────────────────────

/// Runs the integrity checks as **SQL straight against the read-model**: no full hydration of the store —
/// self-referencing edges come out of a join, and `duplicate_order_key` out of a SQL aggregate per sibling
/// set (folding the duplicates down to one issue). Every judgement about integrity goes through this one
/// function: the read-side open (`open_read_at`, `compute_startup_health`) and the write path behind
/// `doctor --fix` alike.
///
/// Deletion is physical, so an orphan means the referenced record does **not exist at all** — every check
/// asks only whether the row it points at is there.
///
/// When `reach` is closed, only breakage inside the bound project is looked at: an issue's `target` and
/// `params` carry task ids and project ids, so opening it up would let another project into the AI's
/// context.
pub fn doctor(conn: &Connection, reach: Reach) -> StoreEngineResult<DoctorResult> {
    let mut issues = Vec::new();

    // Self-referencing dependency edges. An edge never crosses projects, so its project is unambiguously
    // read off the task side.
    const D: col::task_dependency::Cols = col::task_dependency::of("d");
    const T: col::task::Cols = col::task::of("t");
    let mut sel = Select::new();
    let dep_id = sel.col(D.id);
    // Both the tables and the join condition come from the registry: the direction of the edge
    // (`t.id = d.task_id`) is an equality between two typed columns, so it cannot be swapped for a
    // neighbouring column and quietly walk a different edge.
    let mut sql = Sql::from(&sel, D.table);
    sql.join(T.table, same(T.id, D.task_id));
    sql.push_where(
        Pred::all(
            [Some(same(D.task_id, D.blocked_by_id)), reach_pred(reach, T)].into_iter().flatten(),
        )
        .as_ref(),
    );
    issues.extend(query_issues(conn, &sql, |r| {
        let id = dep_id.get(r)?;
        Ok(DoctorIssue::new(
            DoctorIssueKind::SelfDependency,
            format!("task_dependency:{id}"),
            &[("dep", &id.to_string())],
        ))
    })?);
    // Orphans — rows whose referent is gone — are not checked for at all. `task_comment.task_id`, both ends
    // of `task_dependency` and `task.project_id` all carry FK declarations, and a store in an older layout
    // without those constraints is refused at open: the only stores that open are ones the fold has stripped
    // of orphans and passed through `foreign_key_check`. Every write after that runs under
    // `PRAGMA foreign_keys = ON`, so a row with no referent cannot be inserted. Deleting a parent is the
    // other side of it: every reference that stands for a concept is RESTRICT (`AMB-D-403`), so a delete op
    // takes its children first — `ops::project::delete` deletes a project's tasks and only then the project —
    // and the database refuses the parent outright while one remains. So a row that has lost what it hangs
    // on cannot exist either, whichever end you come at it from. What is left is exactly what this layer can
    // still find: self-reference (a perfectly valid reference, so no FK stops it) and duplicate order_keys
    // (nothing to do with FKs).

    // Broken ordering: a duplicate order_key within one sibling set. The sibling set is the project — a task
    // is placed nowhere else. The inner query narrows to the duplicated order_keys; the outer one takes a
    // `MIN` per set, yielding one issue per set carrying the lowest duplicated key.
    //
    // What the outer query reads are the columns of a derived table (the inner `GROUP BY`), with names and
    // types drawn from the registry. It is the inner `WHERE` that makes `project_id` non-NULL, so
    // `Col::required` says so; `MIN(order_key)` is an aggregate rather than a column and is therefore the one
    // term the registry has no type for (`Select::expr`).
    const A: col::task::Cols = col::task::ALL;
    const G: col::task::Cols = col::task::of("g");

    // Inner: keep only the (project_id, order_key) pairs that occur more than once. GROUP BY / HAVING are
    // aggregation grammar and carry no bind values; the columns grouped on come from the registry, so
    // grouping by a column the table does not have is not expressible.
    let mut inner_sel = Select::new();
    inner_sel.col(A.project_id);
    inner_sel.col(A.order_key);
    let mut inner = Sql::from(&inner_sel, A.table);
    inner
        .push_where(
            Pred::all(
                [
                    Some(Pred::is_not_null(A.project_id)),
                    Some(Pred::is_not_null(A.order_key)),
                    reach_pred(reach, A),
                ]
                .into_iter()
                .flatten(),
            )
            .as_ref(),
        )
        .group_by([A.project_id.to_sql(), A.order_key.to_sql()])
        .having(&Pred::plain("COUNT(*) > 1"));

    // Outer: fold each set down to a single issue. `Sql::from_sub` is what inherits the derived table, and it
    // carries the inner query's bind values along with it.
    let mut sel = Select::new();
    let dup_project = sel.col(G.project_id.required());
    let dup_key = sel.expr::<String>(format!("MIN({})", G.order_key.to_sql()));
    let mut sql = Sql::from_sub(&sel, &inner, "g");
    sql.group_by([G.project_id.to_sql()]);

    issues.extend(query_issues(conn, &sql, |r| {
        let project_id = dup_project.get(r)?;
        let order_key = dup_key.get(r)?;
        Ok(DoctorIssue::new(
            DoctorIssueKind::DuplicateOrderKey,
            format!("project:{project_id}"),
            &[("project", &project_id.to_string()), ("order_key", &order_key)],
        ))
    })?);

    // A task declared to start after the day it is due. Neither declaration is wrong on its own, so
    // nothing on the write path refuses the pair — but together they hide the task: it stays out of the
    // mailbox until a start day that falls after the day it was already due. Raising it here is what was
    // chosen over an exception in the ready predicate ("it is due, so ignore the start day"), which would
    // have left nobody able to read which of the two declarations won.
    //
    // Tasks that have ended are left out, whichever way they ended: the contradiction is only worth a
    // sentence while the work is outstanding.
    // Both columns are stored as `YYYY-MM-DD`, so this is the lexicographic comparison the read model uses
    // everywhere for a day column.
    const S: col::task::Cols = col::task::ALL;
    let mut sel = Select::new();
    let (bad_id, bad_start, bad_due) = (sel.col(S.id), sel.col(S.start_on), sel.col(S.due_on));
    let mut sql = Sql::from(&sel, S.table);
    sql.push_where(
        Pred::all(
            [
                Some(!Pred::is_blank(S.start_on)),
                Some(!Pred::is_blank(S.due_on)),
                Some(Pred::plain(format!("{} > {}", S.start_on.to_sql(), S.due_on.to_sql()))),
                Some(crate::store_engine::read::still_open(S.status)),
                reach_pred(reach, S),
            ]
            .into_iter()
            .flatten(),
        )
        .as_ref(),
    )
    .order_by([Sort::by(S.id)]);

    issues.extend(query_issues(conn, &sql, |r| {
        let id = bad_id.get(r)?;
        Ok(DoctorIssue::new(
            DoctorIssueKind::StartAfterDue,
            format!("task:{id}"),
            &[
                ("task", &crate::idref::task(id)),
                ("start_on", &bad_start.get(r)?.unwrap_or_default()),
                ("due_on", &bad_due.get(r)?.unwrap_or_default()),
            ],
        ))
    })?);

    let error = issues.iter().filter(|i| i.severity == "error").count();
    let warning = issues.iter().filter(|i| i.severity == "warning").count();
    Ok(DoctorResult {
        ok: error == 0,
        issues,
        summary: Summary { error, warning },
    })
}

/// Bodies whose prose points at refs that resolve to nothing.
///
/// **Not part of [`doctor`], deliberately** — [`crate::doctor::report`] chains it in alongside the
/// environment checks instead. `doctor` is the cheap always-on half: it runs at every write open
/// (`compute_startup_health`) and, in the GUI, on every store-changed tick. This check reads every body on
/// the device and parses each as Markdown, which is the same reason the environment's filesystem walk is
/// kept out of that path. It also answers a different question: `doctor`'s checks say a row is broken, and
/// this one says a *sentence* has rotted while every row around it is intact.
///
/// Unlike its neighbours it is not a predicate SQL can hold: whether `AMB-D-79` in a body is a pointer or a
/// specimen is a question about Markdown ([`crate::refscan`]), so the bodies are read out and scanned here.
/// Four surfaces carry prose — a task's notes, a decision's body, and the comments on each — and each is
/// folded to **one issue per body**: a body is what a person opens and edits, so three dead refs in one note
/// are one thing to go and fix, not three. (The same folding `duplicate_order_key` does, for the same
/// reason.)
///
/// **Reach narrows what is read, never what resolves.** Which bodies are scanned is the binding's business,
/// as everywhere else. But a ref is dead when the number exists *nowhere*, so the liveness sets are the whole
/// device: narrowing them would make every ref into a neighbouring project read as broken, and send a reader
/// hunting for a task that is alive and simply not theirs. Nothing about the other project escapes — a ref
/// that resolves out of reach raises no issue at all, so the existence is read and then spent on silence.
pub fn dead_ref_issues(conn: &Connection, reach: Reach) -> StoreEngineResult<Vec<DoctorIssue>> {
    const T: col::task::Cols = col::task::ALL;
    const DEC: col::decision::Cols = col::decision::ALL;

    let live_tasks = live_ids(conn, T.table, T.id)?;
    let live_decisions = live_ids(conn, DEC.table, DEC.id)?;

    let mut issues = Vec::new();

    // A task's notes. `project_id` is nullable, so a closed reach leaves the inbox out — the same reading
    // every other check here gives it.
    let mut sel = Select::new();
    let (id, body) = (sel.col(T.id), sel.col(T.notes));
    let mut sql = Sql::from(&sel, T.table);
    sql.push_where(reach_pred(reach, T).as_ref());
    issues.extend(scan_bodies(conn, &sql, "task", &id, &body, &live_tasks, &live_decisions)?);

    // A comment on a task. It carries no project of its own, so the reach is read off the task it hangs on.
    const TC: col::task_comment::Cols = col::task_comment::of("c");
    const CT: col::task::Cols = col::task::of("t");
    let mut sel = Select::new();
    let (id, body) = (sel.col(TC.id), sel.col(TC.text));
    let mut sql = Sql::from(&sel, TC.table);
    sql.join(CT.table, same(CT.id, TC.task_id));
    sql.push_where(reach_pred(reach, CT).as_ref());
    issues.extend(scan_bodies(conn, &sql, "task_comment", &id, &body, &live_tasks, &live_decisions)?);

    // A decision's body. `decision.project_id` is NOT NULL, so a closed reach narrows it outright.
    let mut sel = Select::new();
    let (id, body) = (sel.col(DEC.id), sel.col(DEC.body));
    let mut sql = Sql::from(&sel, DEC.table);
    sql.push_where(reach.project().map(|p| Pred::eq(DEC.project_id, p)).as_ref());
    issues.extend(scan_bodies(conn, &sql, "decision", &id, &body, &live_tasks, &live_decisions)?);

    // A comment on a decision — the mirror of a task comment, reached through its decision.
    const DC: col::decision_comment::Cols = col::decision_comment::of("dc");
    const CD: col::decision::Cols = col::decision::of("d");
    let mut sel = Select::new();
    let (id, body) = (sel.col(DC.id), sel.col(DC.text));
    let mut sql = Sql::from(&sel, DC.table);
    sql.join(CD.table, same(CD.id, DC.decision_id));
    sql.push_where(reach.project().map(|p| Pred::eq(CD.project_id, p)).as_ref());
    issues.extend(scan_bodies(conn, &sql, "decision_comment", &id, &body, &live_tasks, &live_decisions)?);

    Ok(issues)
}

/// Every id a table holds, device-wide — the set a ref is resolved against. Deletion is physical, so
/// membership *is* existence: there is no tombstone to tell "deleted" from "never issued", and the two are
/// not worth telling apart — a reader sent to either comes back with nothing.
fn live_ids(
    conn: &Connection,
    table: crate::store_engine::sql::Table,
    id: crate::store_engine::sql::Col<crate::store_engine::sql::Int>,
) -> StoreEngineResult<HashSet<i64>> {
    let mut sel = Select::new();
    let slot = sel.col(id);
    let sql = Sql::from(&sel, table);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| slot.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<HashSet<i64>>>()
        .map_err(StoreEngineError::from)?;
    Ok(ids)
}

/// Run `sql` — a statement yielding `(id, body)` — and raise one issue per body that points at something
/// gone. The refs are listed in the order the body writes them, deduplicated: a note repeating a dead ref is
/// still one broken pointer, and the reader is being told which one, not how many times it was typed.
#[allow(clippy::too_many_arguments)]
fn scan_bodies(
    conn: &Connection,
    sql: &Sql,
    kind: &str,
    id: &Slot<i64>,
    body: &Slot<String>,
    live_tasks: &HashSet<i64>,
    live_decisions: &HashSet<i64>,
) -> StoreEngineResult<Vec<DoctorIssue>> {
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok((id.get(r)?, body.get(r)?))
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(StoreEngineError::from)?;

    let mut issues = Vec::new();
    for (row_id, text) in rows {
        let mut dead: Vec<String> = Vec::new();
        for r in crate::refscan::refs_in_prose(&text) {
            let live = match r.kind {
                RefKind::Task => live_tasks.contains(&r.id),
                _ => live_decisions.contains(&r.id),
            };
            if !live && !dead.iter().any(|seen| seen.eq_ignore_ascii_case(&r.raw)) {
                dead.push(r.raw);
            }
        }
        if dead.is_empty() {
            continue;
        }
        let at = format!("{kind}:{row_id}");
        issues.push(DoctorIssue::new(
            DoctorIssueKind::DeadRef,
            at.clone(),
            &[("at", &at), ("refs", &dead.join(", "))],
        ));
    }
    Ok(issues)
}

/// The reach, as a predicate on the task columns a check happens to have in hand: a closed reach narrows
/// to its project, an open one narrows to nothing at all. One derivation, handed to every check — a check
/// that has no project to narrow on carries no clause, and the reach's meaning is stated once.
fn reach_pred(reach: Reach, t: col::task::Cols) -> Option<Pred> {
    reach.project().map(|p| Pred::eq(t.project_id, p))
}

/// Run `sql` over the read-model and map every row to a [`DoctorIssue`] via `f`. A small helper so
/// each [`doctor`] check reads as its statement + its row→issue mapping — and the statement carries its
/// own bind values ([`Sql`]), so a check cannot come to disagree with what it binds.
fn query_issues<F>(conn: &Connection, sql: &Sql, f: F) -> StoreEngineResult<Vec<DoctorIssue>>
where
    F: Fn(&Row) -> rusqlite::Result<DoctorIssue>,
{
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| f(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<DoctorIssue>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::StoreEngine;
    use rusqlite::types::Value;

    fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }

    /// Plants one of each breakage doctor can find in each of two projects: a self-referencing edge (on task
    /// 1 and on task 3) and a duplicate order_key (tasks 1 and 2 in project 7, tasks 3 and 4 in project 8).
    fn broken() -> StoreEngine {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        for (pid, name) in [(7, "Alpha"), (8, "Beta")] {
            e.put_record("project", pid, &[("name", text(name)), ("order_key", text("a"))]).unwrap();
        }
        for (id, pid, key) in [(1, 7, "m"), (2, 7, "m"), (3, 8, "z"), (4, 8, "z")] {
            e.put_record(
                "task",
                id,
                &[("title", text("t")), ("project_id", Value::Integer(pid)), ("order_key", text(key))],
            )
            .unwrap();
        }
        for (edge, task) in [(10, 1), (11, 3)] {
            e.put_record(
                "dependency",
                edge,
                &[("task_id", Value::Integer(task)), ("blocked_by_id", Value::Integer(task))],
            )
            .unwrap();
        }
        e
    }

    /// An open reach (a human, the GUI) sees the whole machine — breakage in both projects is raised. The
    /// aggregate fold holds too: a duplicate order_key yields **one issue per project**, not one per
    /// duplicated row.
    #[test]
    fn an_open_reach_sees_every_project() {
        let e = broken();

        let r = doctor(e.conn(), Reach::All).unwrap();

        let mut targets: Vec<&str> = r.issues.iter().map(|i| i.target.as_str()).collect();
        targets.sort_unstable();
        assert_eq!(
            targets,
            vec!["project:7", "project:8", "task_dependency:10", "task_dependency:11"],
            "the self-reference in each of the two projects, plus the duplicate order_key folded to one per project"
        );
        assert!(!r.ok, "a single error is enough for ok to be false");
    }

    /// A closed reach (an AI) sees only breakage in its bound project — no other project's task ids or
    /// project id enter its context.
    #[test]
    fn a_closed_reach_sees_only_its_own_project() {
        let e = broken();

        let r = doctor(e.conn(), Reach::Project(7)).unwrap();

        let mut targets: Vec<&str> = r.issues.iter().map(|i| i.target.as_str()).collect();
        targets.sort_unstable();
        assert_eq!(targets, vec!["project:7", "task_dependency:10"]);
    }

    /// A sound store raises nothing — in particular, the duplicate check built on the derived table does not
    /// pick up sets that hold no duplicate.
    #[test]
    fn a_sound_store_raises_nothing() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("project", 7, &[("name", text("Alpha")), ("order_key", text("a"))]).unwrap();
        for (id, key) in [(1, "m"), (2, "n")] {
            e.put_record(
                "task",
                id,
                &[("title", text("t")), ("project_id", Value::Integer(7)), ("order_key", text(key))],
            )
            .unwrap();
        }
        e.put_record(
            "dependency",
            10,
            &[("task_id", Value::Integer(1)), ("blocked_by_id", Value::Integer(2))],
        )
        .unwrap();

        let r = doctor(e.conn(), Reach::All).unwrap();

        assert!(r.issues.is_empty(), "{:?}", r.issues);
        assert!(r.ok);
    }

    // ─────────────────────── a start day past the deadline ───────────────────────

    /// A store with one task per shape of the start/due pair. Task 1 is the contradiction; the rest are the
    /// ways of being fine, including the done task that carries the same contradiction.
    fn dated_tasks() -> StoreEngine {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("project", 7, &[("name", text("Alpha")), ("order_key", text("a"))]).unwrap();
        // An unset day is NULL, not an empty string — the schema's CHECK only lets a real `YYYY-MM-DD`
        // through — so those columns are left off the record rather than written blank.
        for (id, start, due, status, key) in [
            (1, Some("2026-09-01"), Some("2026-08-01"), "todo", "a"), // starts a month after it was due
            (2, Some("2026-08-01"), Some("2026-09-01"), "todo", "b"), // the ordinary way round
            (3, Some("2026-08-01"), Some("2026-08-01"), "todo", "c"), // same day: not after, so no contradiction
            (4, None, Some("2026-08-01"), "todo", "d"),               // due only
            (5, Some("2026-09-01"), None, "todo", "e"),               // start only
            (6, Some("2026-09-01"), Some("2026-08-01"), "done", "f"), // the same contradiction, already finished
        ] {
            let mut cols: Vec<(&str, Value)> = vec![
                ("title", text("t")),
                ("project_id", Value::Integer(7)),
                ("order_key", text(key)),
                ("status", text(status)),
            ];
            if let Some(d) = start {
                cols.push(("start_on", text(d)));
            }
            if let Some(d) = due {
                cols.push(("due_on", text(d)));
            }
            e.put_record("task", id, &cols).unwrap();
        }
        e
    }

    #[test]
    fn a_start_day_after_the_due_day_is_raised_with_both_dates() {
        let r = doctor(dated_tasks().conn(), Reach::All).unwrap();

        let raised: Vec<&DoctorIssue> =
            r.issues.iter().filter(|i| i.kind == DoctorIssueKind::StartAfterDue).collect();
        assert_eq!(
            raised.iter().map(|i| i.target.as_str()).collect::<Vec<_>>(),
            vec!["task:1"],
            "only the outstanding contradiction — not the right way round, not the same day, not one date \
             alone, and not the one already done"
        );
        // The sentence a surface assembles needs both days, so the issue has to carry both.
        assert_eq!(raised[0].params.get("start_on").map(String::as_str), Some("2026-09-01"));
        assert_eq!(raised[0].params.get("due_on").map(String::as_str), Some("2026-08-01"));
        assert!(r.ok, "a contradiction between two declarations has broken nothing in the store");
    }

    // ─────────────────────── dead refs ───────────────────────

    /// A store with prose on all four body surfaces. Live: task 1 and 2, decision 5. Everything else a body
    /// names is a number nothing was ever issued under.
    fn bodies() -> StoreEngine {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        for (pid, name) in [(7, "Alpha"), (8, "Beta")] {
            e.put_record("project", pid, &[("name", text(name)), ("order_key", text("a"))]).unwrap();
        }
        for (id, pid, notes) in [
            (1, 7, "read AMB-D-9 first"),
            (2, 8, "read AMB-D-9 first"),
        ] {
            e.put_record(
                "task",
                id,
                &[
                    ("title", text("t")),
                    ("notes", text(notes)),
                    ("project_id", Value::Integer(pid)),
                    ("order_key", text("m")),
                ],
            )
            .unwrap();
        }
        e.put_record(
            "decision",
            5,
            &[
                ("project_id", Value::Integer(7)),
                ("title", text("d")),
                ("body", text("supersedes AMB-D-4")),
            ],
        )
        .unwrap();
        e.put_record(
            "task_comment",
            30,
            &[("task_id", Value::Integer(1)), ("text", text("see AMB-T-99"))],
        )
        .unwrap();
        e.put_record(
            "decision_comment",
            40,
            &[("decision_id", Value::Integer(5)), ("text", text("see AMB-T-99"))],
        )
        .unwrap();
        e
    }

    /// The check is reached directly: it is deliberately not part of `doctor` (see its doc), so a test that
    /// went through `doctor` would pass while the check ran nowhere.
    fn dead(e: &StoreEngine, reach: Reach) -> Vec<DoctorIssue> {
        dead_ref_issues(e.conn(), reach).unwrap()
    }

    fn dead_targets(issues: &[DoctorIssue]) -> Vec<&str> {
        let mut t: Vec<&str> = issues.iter().map(|i| i.target.as_str()).collect();
        t.sort_unstable();
        t
    }

    /// Every body surface is read, and a ref whose number was never issued is raised against the body that
    /// writes it.
    #[test]
    fn a_ref_into_nothing_is_raised_on_every_body_surface() {
        let issues = dead(&bodies(), Reach::All);

        assert_eq!(
            dead_targets(&issues),
            vec!["decision:5", "decision_comment:40", "task:1", "task:2", "task_comment:30"],
        );
        let issue = issues.iter().find(|i| i.target == "task:1").unwrap();
        assert_eq!(issue.params["refs"], "AMB-D-9");
        assert_eq!(issue.params["at"], "task:1");
        assert_eq!(issue.severity, "warning", "a rotted sentence has broken no row");
    }

    /// A live ref is not raised — that is the whole point of resolving rather than pattern-matching.
    #[test]
    fn a_ref_that_resolves_is_left_alone() {
        let e = bodies();
        e.put_record(
            "task",
            3,
            &[
                ("title", text("t")),
                ("notes", text("read AMB-D-5 and AMB-T-1 first")),
                ("project_id", Value::Integer(7)),
                ("order_key", text("z")),
            ],
        )
        .unwrap();

        let issues = dead(&e, Reach::All);

        assert!(!dead_targets(&issues).contains(&"task:3"), "{issues:?}");
    }

    /// The reach narrows which bodies are **read**: another project's note never reaches an AI's context,
    /// dead ref or not.
    #[test]
    fn a_closed_reach_reads_only_its_own_bodies() {
        let issues = dead(&bodies(), Reach::Project(7));

        assert_eq!(
            dead_targets(&issues),
            vec!["decision:5", "decision_comment:40", "task:1", "task_comment:30"],
            "task:2 is project 8's body, so it is not read at all",
        );
    }

    /// …but it does **not** narrow what resolves. A ref into a neighbouring project is alive, and calling it
    /// dead would send a reader hunting for a task that is simply not theirs. Nothing about project 8 leaks
    /// either way: the existence is read and spent on raising nothing.
    #[test]
    fn a_closed_reach_still_resolves_against_the_whole_device() {
        let e = bodies();
        e.put_record(
            "task",
            3,
            &[
                ("title", text("t")),
                ("notes", text("blocked on AMB-T-2")),
                ("project_id", Value::Integer(7)),
                ("order_key", text("z")),
            ],
        )
        .unwrap();

        let issues = dead(&e, Reach::Project(7));

        assert!(
            !dead_targets(&issues).contains(&"task:3"),
            "task 2 lives in project 8, out of reach but not gone: {issues:?}",
        );
    }

    /// One body is one thing to go and fix, so its dead refs are folded into a single issue — and a ref the
    /// body repeats is named once.
    #[test]
    fn a_body_is_one_issue_however_many_refs_died_in_it() {
        let e = bodies();
        e.put_record(
            "task",
            3,
            &[
                ("title", text("t")),
                ("notes", text("AMB-D-9, then AMB-T-99, then AMB-D-9 again")),
                ("project_id", Value::Integer(7)),
                ("order_key", text("z")),
            ],
        )
        .unwrap();

        let all = dead(&e, Reach::All);

        let issues: Vec<_> = all.iter().filter(|i| i.target == "task:3").collect();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].params["refs"], "AMB-D-9, AMB-T-99", "in the order the body writes them");
    }

    /// The line `refscan` draws, held from this side too: a body showing the *form* of a ref is this
    /// repository's own convention, and reading it as a pointer would report that convention as breakage.
    #[test]
    fn a_ref_shown_as_a_specimen_is_not_a_dead_pointer() {
        let e = bodies();
        e.put_record(
            "task",
            3,
            &[
                ("title", text("t")),
                ("notes", text("the form is `AMB-D-9`\n\n```\nAMB-T-99\n```")),
                ("project_id", Value::Integer(7)),
                ("order_key", text("z")),
            ],
        )
        .unwrap();

        let issues = dead(&e, Reach::All);

        assert!(!dead_targets(&issues).contains(&"task:3"), "{issues:?}");
    }

    /// It is not part of `doctor`, and that is load-bearing: `doctor` runs at every write open and on every
    /// GUI tick, and this check reads and parses every body on the device.
    #[test]
    fn the_always_on_check_never_pays_for_it() {
        let r = doctor(bodies().conn(), Reach::All).unwrap();

        assert!(
            !r.issues.iter().any(|i| i.kind == DoctorIssueKind::DeadRef),
            "doctor raised a dead ref, so every command now scans every body: {:?}",
            r.issues,
        );
    }
}
