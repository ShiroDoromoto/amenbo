//! SQL read layer over the engine SQLite truth-source.
//!
//! The `task list` grammar is compiled to **indexed SQL** against the [`super::engine::StoreEngine`]
//! read-model tables, with `LIMIT`/`OFFSET` paging (bounded memory).
//!
//! The filter grammar is parsed by [`crate::query::Filter`]; this module only maps an already-parsed
//! [`Filter`] to a `WHERE` clause.
//!
//! Note on the word narrowing — the words go to the **word index** ([`super::search`]), never to the record's own
//! columns: a term matches a normalised copy of the title, the notes, or any live comment body, and the
//! record it belongs to is reached from there. So this module states *which faces belong to which
//! record*, and leaves what a term means — the trigram index above three characters, a scan of the copy
//! below it — to the index.

use std::collections::HashMap;

use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::Row;

use super::schema::col;
use super::search;
use super::search::HitFace;
use super::sql::{
    same, Col, Count, Exists, Expr, IdSet, Int, NotNull, Nullability, Nullable, Pred, Select, Slot, Sort,
    Sql, Text as SqlText, Union,
};
use super::{StoreEngineError, Result};
use crate::model::{ActorKind, DecisionStatus, Priority, TaskStatus};
use crate::query::{AssigneeFilter, DueFilter, Filter, StartFilter};
use crate::view::{ProjectRef, TaskCompact};

/// The task named by `status` has **ended**: it reached one of the two terminals. The single definition
/// of that question, shared with its complement [`still_open`] — between them they carry the `done:`
/// filter, the open-blocker half of `ready:`, [`reserve_blockers`], `blocked_by` / `blocks`, the open
/// counts and the due buckets — so no reading can drift into its own idea of what an ended task is.
///
/// **Closed is not the same question as done** (`AMB-D-397`). Work decided against is over, so it
/// releases what it was blocking and leaves the open counts; what it must never do is join the
/// *completed* ones, which is a separate derivation over `Done` alone ([`crate::model::Task::completed`],
/// the completed count, `completed_today`) and is deliberately not routed through here.
///
/// Bind-free, like [`unsettled_premise`]: the only literals are the store's own enum spellings, grammar
/// rather than data, which keeps the predicate usable as a select item as well as in a `WHERE`.
pub(crate) fn closed<E: Expr<Ty = SqlText>>(status: E) -> Pred {
    let terminals: Vec<String> =
        TaskStatus::CLOSED.iter().map(|s| format!("'{}'", s.as_str())).collect();
    Pred::plain(format!("{} IN ({})", status.to_sql(), terminals.join(", ")))
}

/// The complement of [`closed`] — the task is still work. The two are one definition, so a reading that
/// takes the negative can never come to disagree with the one that takes the positive.
pub(crate) fn still_open<E: Expr<Ty = SqlText>>(status: E) -> Pred {
    !closed(status)
}

/// The premise decision `dc` is **unsettled** — it is not `accepted`, or it is not current because a
/// live decision holds a `supersedes` edge at it (currency is derived from the edges, never a status).
/// The single definition every premise read shares — the `ready:` filter, `task_detail`, the task card
/// and [`reserve_blockers`] — so they cannot drift into disagreeing about what blocks a reserve. The
/// premise is named by whatever alias the caller gave it, so the sharing costs no assumption about the
/// query it lands in. It carries no bind values, on purpose: besides riding in a `WHERE`, this predicate
/// is *selected as a column* (the task card reads it back as a bool and splits its links in one pass),
/// and a select item has no placeholders to bind — the only literals in it are the store's own enum
/// spellings, grammar here rather than data, which is exactly what [`Pred::plain`] is for.
fn unsettled_premise(dc: col::decision::Cols) -> Pred {
    Pred::plain(format!(
        "{} <> '{}'",
        dc.status.to_sql(),
        crate::model::DecisionStatus::Accepted.as_str()
    ))
    .or(superseded(dc))
}

/// A live decision holds a `supersedes` edge at `dc` — the whole of what "not current" means (currency
/// is derived from the edges, never a status). Shared by [`unsettled_premise`] (which blocks a reserve
/// on it) and by the reads that project `current` as a column, so the two cannot come to disagree about
/// which decisions are still standing. Bind-free, for the same reason as its callers.
fn superseded(dc: col::decision::Cols) -> Pred {
    const E: col::decision_edge::Cols = col::decision_edge::of("e");
    const S: col::decision::Cols = col::decision::of("s");

    Exists::over(E.table)
        .join(S.table, same(S.id, E.decision_id))
        .filter(same(E.target_decision_id, dc.id))
        .filter(word(E.kind, crate::model::DecisionEdgeKind::Supersedes.as_str()))
        .pred()
}

/// [`superseded`], narrowed to a supersession drawn **after** `since` — the arm that lets the premise-change
/// judgement date a premise that lost currency under a holder (`AMB-D-373`). The edge's own intent column is
/// the only clock there is for it: the superseded row is never rewritten, so nothing on the premise side
/// moved when it stopped being current. Unlike its bind-free sibling this one carries a bind, so it belongs
/// in a `WHERE` and not in a select item.
///
/// An edge whose `drawn_at` is unset reads as older than any `since` (`NULL > ?` is not true) and so stays
/// quiet — the same safe side the rest of the judgement errs on for an undatable premise.
fn superseded_since(dc: col::decision::Cols, since: String) -> Pred {
    const E: col::decision_edge::Cols = col::decision_edge::of("e");
    const S: col::decision::Cols = col::decision::of("s");

    Exists::over(E.table)
        .join(S.table, same(S.id, E.decision_id))
        .filter(same(E.target_decision_id, dc.id))
        .filter(word(E.kind, crate::model::DecisionEdgeKind::Supersedes.as_str()))
        .filter(Pred::cmp(E.drawn_at, ">", since))
        .pred()
}

/// `<col> = '<word>'` — a column against one of the store's own enum spellings, written as a literal
/// rather than a bind: this is grammar, not data, and the places it is needed (a select item, a `JOIN`'s
/// `ON`) have no placeholder to bind. The word comes from the model's `as_str`, so it is not spelled
/// here either.
fn word(col: Col<SqlText, NotNull>, w: &str) -> Pred {
    Pred::plain(format!("{} = '{}'", col.to_sql(), w))
}

/// The first live decision holding a `supersedes` edge at `target`, as a scalar subquery — the reverse
/// view of the edge table (nothing on the premise itself records that it was overturned), and the relink
/// target a `not_ready` names. Shared by [`reserve_blockers`] and [`premise_successor`], so the two
/// cannot come to name different successors for the same premise. Bind-free for the same reason as
/// [`unsettled_premise`]: both callers put it in a place that takes no placeholders (a `LEFT JOIN`'s
/// `ON`, a select item).
fn first_superseder(target: Col<Int, NotNull>) -> String {
    const SE: col::decision_edge::Cols = col::decision_edge::of("se");
    const SD: col::decision::Cols = col::decision::of("sd");

    // A scalar subquery, not an `Exists`: it yields the successor's id, and both callers put it where a
    // value goes. The tables and columns still come from the registry, and the conditions are the same
    // ones `unsettled_premise` asks the edge for.
    let mut sel = Select::new();
    sel.col(SE.decision_id);
    let mut q = Sql::from(&sel, SE.table);
    q.join(SD.table, same(SD.id, SE.decision_id))
        .push_where(Some(
            &same(SE.target_decision_id, target)
                .and(word(SE.kind, crate::model::DecisionEdgeKind::Supersedes.as_str())),
        ))
        .order_by([Sort::by(SE.id)])
        // The `LIMIT` is a literal, not the bound one `Sql::limit` writes: this fragment is spliced
        // where no placeholder can go (a select item, a `LEFT JOIN`'s `ON`), which is the same reason the
        // predicates above are bind-free.
        .push(" LIMIT 1");
    format!("({})", q.text())
}

/// The successor of the premise an edge `e` points at — computed only for a `builds_on` row, so an
/// ordinary supersedes/amends edge costs no extra seek ([`decision_edges`]).
fn premise_successor(e: col::decision_edge::Cols) -> String {
    format!(
        "CASE WHEN {} = '{}' THEN {} END",
        e.kind.to_sql(),
        crate::model::DecisionEdgeKind::BuildsOn.as_str(),
        first_superseder(e.target_decision_id),
    )
}

/// `SELECT <id> FROM <its table> [WHERE …] ORDER BY <id>` — the shape nearly every id lookup here has
/// (an existence probe, a ref resolve, a table's live ids). The table is the id column's **own**
/// ([`Col::table`]), so the projection and the `FROM` cannot name different tables, and the row is read
/// through the slot the projection handed back rather than a counted-off index.
fn select_ids(conn: &Connection, id: Col<Int, NotNull>, pred: Option<&Pred>) -> Result<Vec<i64>> {
    let mut sel = Select::new();
    let slot = sel.col(id);
    let mut sql = Sql::from(&sel, id.table());
    sql.push_where(pred).order_by([Sort::by(id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| slot.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(StoreEngineError::from)?;
    Ok(ids)
}

/// The reference resolvers' arm: `reference` names a row by its **key** or by its **exact name**
/// (byte-exact). A reference that is not a decimal key can only hit the name arm — the key arm is
/// given no value to match, rather than a wildcard that would hand back the only row.
fn key_or_name_pred(
    kind: crate::idref::RefKind,
    id: Col<Int, NotNull>,
    name: Col<SqlText, NotNull>,
    reference: &str,
) -> Pred {
    let by_name = Pred::eq(name, reference);
    match crate::ops::parse_id_ref(kind, reference) {
        Some(n) => Pred::eq(id, n).or(by_name),
        None => by_name,
    }
}

/// The same, folding case on the name arm — what the axis references resolve with (`dim:<axis>=…`).
/// `LOWER()` in SQLite only folds ASCII, so the bound name is ASCII-folded the same way: folding it
/// with Rust's full-Unicode `to_lowercase` would compare against a column SQLite left alone.
fn key_or_folded_name_pred(
    kind: crate::idref::RefKind,
    id: Col<Int, NotNull>,
    name: Col<SqlText, NotNull>,
    reference: &str,
) -> Pred {
    let by_name = Pred::eq(name.lower(), reference.to_ascii_lowercase());
    match crate::ops::parse_id_ref(kind, reference) {
        Some(n) => Pred::eq(id, n).or(by_name),
        None => by_name,
    }
}

/// The first id the predicate matches, in `id` order, or `None` — the shape of every "the live edge /
/// assignment between this pair, if there is one" lookup a mutation makes idempotent. At most one row
/// exists (a UNIQUE index says so); the `LIMIT 1` on an id order pins the pick if a store ever carries
/// two, rather than letting the answer depend on the scan.
fn first_id(conn: &Connection, id: Col<Int, NotNull>, pred: &Pred) -> Result<Option<i64>> {
    let mut sel = Select::new();
    let slot = sel.col(id);
    let mut sql = Sql::from(&sel, id.table());
    sql.push_where(Some(pred)).order_by([Sort::by(id)]).limit(1);
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| slot.get(r))
        .optional()
        .map_err(StoreEngineError::from)
}

/// Whether one row exists — an `EXISTS` over the predicate, which SQLite answers off the index without
/// materialising the row.
fn exists_row(conn: &Connection, id: Col<Int, NotNull>, pred: &Pred) -> Result<bool> {
    let mut sel = Select::new();
    let slot = sel.pred(Exists::over(id.table()).filter(pred.clone()).pred());
    let sql = Sql::select(&sel);
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| slot.get(r))
        .map_err(StoreEngineError::from)
}

/// One column of one row, by primary key — `None` when no row carries the id (the column's own
/// nullability is the caller's `V`, straight from the registry).
fn scalar_by_id<C: super::sql::Read + super::sql::Expr + Copy>(
    conn: &Connection,
    id: Col<Int, NotNull>,
    column: C,
    row_id: i64,
) -> Result<Option<C::Out>> {
    let mut sel = Select::new();
    let slot = sel.col(column);
    let pred = Pred::eq(id, row_id);
    let mut sql = Sql::from(&sel, id.table());
    sql.push_where(Some(&pred));
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| slot.get(r))
        .optional()
        .map_err(StoreEngineError::from)
}

/// A `task list` query against the SQLite truth-source. Mirrors [`crate::query::ListParams`] plus
/// `LIMIT`/`OFFSET` — the page is cut by the query itself, not by the caller.
pub struct TaskQuery<'a> {
    /// How far this read may see. There is no default: every caller names it, so a surface that reads
    /// tasks straight off the engine — the GUI's task page does — cannot forget to declare its reach and
    /// be handed every project. A closed reach narrows the rows here too, so the guard does not depend on
    /// the caller having gone through [`crate::query::list`].
    pub reach: crate::reach::Reach,
    /// Restrict to tasks placed in this project (exact id).
    pub project_id: Option<i64>,
    /// Already-parsed filter (the grammar lives in [`crate::query::Filter`]).
    pub filter: &'a Filter,
    /// Sort key: `order|due|priority|created|title`, optional leading `-` for descending.
    pub sort: &'a str,
    /// Today (referent of the relative `due:` filters), supplied by the caller.
    pub today: NaiveDate,
    /// Page size; `None` = no limit (all rows from `offset`).
    pub limit: Option<usize>,
    /// Rows to skip; `None` = 0.
    pub offset: Option<usize>,
}

/// One page of a task-list query: the total match count (before paging) and the page of ids in
/// sort order.
pub struct TaskPage {
    pub total_matched: usize,
    pub ids: Vec<i64>,
}

/// Run a `task list` query against the read-model: filter → indexed `WHERE`, sort → `ORDER BY`,
/// page → `LIMIT`/`OFFSET`. Returns the ordered page of task ids plus the total match count.
/// How many tasks `q` matches, and the earliest start day among them — one row, no ids. Fed the caller's
/// own query with `ready:` dropped and `start:future` put in its place, this answers "what is my empty
/// mailbox not showing me, and when does the first of it arrive".
///
/// It is a separate read rather than a page the caller counts because the answer is two numbers: paging
/// would carry every waiting row back to compute a minimum, and the whole point is that this runs on the
/// empty-result path where nothing was worth carrying. `None` when nothing is waiting — a store with no
/// waiting task has no earliest day, and `Some(0)` would be a count with a date that does not exist.
pub fn waiting_on_start(conn: &Connection, q: &TaskQuery) -> Result<Option<(usize, NaiveDate)>> {
    let started = std::time::Instant::now();
    let scope = [q.reach.project(), q.project_id]
        .into_iter()
        .flatten()
        .map(|pid| Pred::eq(T.project_id, pid));
    let pred = Pred::all(scope.chain(filter_preds(q)));

    let mut sel = Select::new();
    let count = sel.count_all();
    // Aggregates over a filtered set: the registry cannot type them, and over no rows `MIN` answers NULL
    // rather than nothing — which is exactly the "nothing is waiting" case.
    let earliest = sel.expr::<Option<String>>(format!("MIN({})", T.start_on.name()));
    let mut sql = Sql::from(&sel, T.table);
    sql.push_where(pred.as_ref());

    let (count, earliest) = conn
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
            Ok((count.get(r)?, earliest.get(r)?))
        })
        .map_err(StoreEngineError::from)?;
    // A count-only read: it hands back two aggregates and no rows, so the complexity ratio a page is
    // judged by does not apply and the time budget is the whole judgement (as in `list_task_ids`).
    crate::perf::record_count_query("engine.waiting_on_start", count.max(0) as usize, started.elapsed());
    let Some(earliest) = earliest.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    else {
        return Ok(None);
    };
    Ok((count > 0).then_some((count as usize, earliest)))
}

pub fn list_task_ids(conn: &Connection, q: &TaskQuery) -> Result<TaskPage> {
    let started = std::time::Instant::now();

    // The reach comes first: a closed one (an AI) sees its bound project's rows and nothing else, whatever
    // scope the caller asked for. `query::list` already refuses an out-of-reach scope outright — this is
    // the floor under the surfaces that do not go through it.
    //
    // Scoping by the *params* project (not the filter) follows: the task's own placement must match
    // (placement is task-held). Mirrors `query::list`'s task-column pre-filter.
    //
    // Nothing is filtered out by default: a row that exists is live, so an unfiltered list carries no
    // `WHERE` at all rather than a tautological one — hence `Option`.
    let scope = [q.reach.project(), q.project_id]
        .into_iter()
        .flatten()
        .map(|pid| Pred::eq(T.project_id, pid));
    let pred = Pred::all(scope.chain(filter_preds(q)));

    let order_keys = order_by(q.sort)?;

    // The same predicate value serves both statements — the count and the page cannot come to disagree
    // about what they are asking, and neither can bind a value the other's fragment expects.
    let mut counted = Select::new();
    let matched = counted.count_all();
    let mut count = Sql::from(&counted, T.table);
    count.push_where(pred.as_ref());

    // Total before paging (parity: `query::list.total_matched`).
    let total: usize = conn
        .query_row(count.text(), rusqlite::params_from_iter(count.params()), |r| matched.get(r))
        .map_err(StoreEngineError::from)? as usize;

    // The page itself. `LIMIT -1` = no limit; `OFFSET` skips.
    let mut sel = Select::new();
    let id = sel.col(T.id);
    let mut page = Sql::from(&sel, T.table);
    page.push_where(pred.as_ref())
        .order_by(order_keys)
        .limit(q.limit.map(|n| n as i64).unwrap_or(-1))
        .offset(q.offset.unwrap_or(0) as i64);

    let mut stmt = conn.prepare(page.text()).map_err(StoreEngineError::from)?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(page.params()), |r| id.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(StoreEngineError::from)?;

    // perf: even for index-served SQL, a `total` (the match count before paging) far above the number of
    // rows the page hands back says the whole match set is being counted to produce one page — O(matched).
    // scanned = total_matched, returned = the page's row count. A `limit 0` read is the exception: it is a
    // **count-only read** (a badge count and the like) that wants nothing but the total, so returned = 0 is
    // the design. The complexity ratio is a detector of wasted paging and therefore meaningless for a count
    // — counting N rows always looks like a ratio of N, which would false-positive — so a count-only read
    // drops the ratio and is held to the time budget alone.
    if q.limit == Some(0) {
        crate::perf::record_count_query("engine.list_task_ids", total, started.elapsed());
    } else {
        crate::perf::record_query("engine.list_task_ids", total, ids.len(), started.elapsed());
    }
    Ok(TaskPage { total_matched: total, ids })
}

/// The task's columns as the task queries name them: `FROM task t`, so `T.status` **is** `t.status` —
/// generated from the schema registry, so a column that is not in the store is a name that does not
/// compile, and the column's type comes with it.
const T: col::task::Cols = col::task::of("t");
/// The task↔dimension-value link's columns, as [`dimension_pred`]'s subquery aliases them.
const TDV: col::task_dimension_value::Cols = col::task_dimension_value::of("tdv");
/// The change feed's columns, and the store's scalars — plain tables, named the same way.
const FEED: col::change_feed::Cols = col::change_feed::ALL;
const META: col::store_meta::Cols = col::store_meta::ALL;

/// The word index's copy, as the subqueries below alias it.
const SD: col::search_doc::Cols = col::search_doc::of("sd");

/// One term of a task's word filter: it matches when the term lands on **any** face of the task — its
/// own title or notes, the body of one of its live comments, a label it was placed on, or the name of
/// something attached to it or to one of those comments.
///
/// Which faces those are is [`search::FACES`]'s to say; whose they are is this function's. Two of them
/// are not held on the task at all and are reached by the edge that makes them the task's: a label
/// through the assignment (and, from the value, the axis it is a value of), an attachment through what
/// it hangs off.
///
/// Every arm reaches the task's id **from its own side** — through that side's index
/// (`task_comment_by_task`, `task_dimension_value_by_task`, `attachment_by_target`) and the copy's face
/// key (`search_doc_face`) — and the term is one predicate over the whole set rather than a question put
/// to each candidate task ([`Pred::is_in_any`]). Written the other way, as an `EXISTS` correlated to the
/// task, an arm costs the walk it does times every task considered, whatever order the tables are named
/// in: the arms of a face are not the outer row's to re-derive (`AMB-D-511`).
///
/// What the term itself costs is not the arms' to bound: a [`search::Term`] is a lookup correlated to
/// nothing and hands back membership of what it found (`AMB-D-507`), so the lookup happens once wherever
/// it is written. Which path it took inside — the trigram index or a scan of the copy — is
/// [`search::Term`]'s to decide, and does not change what matching means; so is whether the statement
/// writes the lookup into each of these six arms or made it once at its head and named it, which is the
/// caller's call because only the caller knows how many places ask.
fn task_text_term(term: search::Term<'_>) -> Pred {
    const TC: col::task_comment::Cols = col::task_comment::of("tc");
    let own = IdSet::of(SD.table, SD.owner_id)
        .filter(Pred::eq(SD.owner_kind, search::DATASET_TASK))
        .filter(term.pred(SD));
    let in_comment = IdSet::of(TC.table, TC.task_id)
        .join(SD.table, on_face(search::DATASET_TASK_COMMENT, TC.id))
        .filter(term.pred(SD));
    // The label the task was placed on, and the axis that label is a value of. The axis is reached
    // through the assignment as well, not on its own: an axis nobody placed this task on is not one of
    // the task's own words.
    let on_value = IdSet::of(TDV.table, TDV.task_id)
        .join(SD.table, on_face(search::DATASET_DIMENSION_VALUE, TDV.value_id))
        .filter(term.pred(SD));
    let on_axis = IdSet::of(TDV.table, TDV.task_id)
        .join(SD.table, on_face(search::DATASET_DIMENSION, TDV.dimension_id))
        .filter(term.pred(SD));
    let attached = attachment_ids(search::DATASET_TASK, term);
    // What hangs off a comment hangs off the task, by the same reading that puts the comment's own body
    // here: the timeline is the task's, and so is what was pinned to it.
    let attached_to_comment = IdSet::of(TC.table, TC.task_id)
        .join(A.table, hangs_off(search::DATASET_TASK_COMMENT, TC.id))
        .join(SD.table, on_face(search::DATASET_ATTACHMENT, A.id))
        .filter(term.pred(SD));
    Pred::is_in_any(T.id, [own, in_comment, on_value, on_axis, attached, attached_to_comment])
}

/// The decision's columns as the decision-side word queries name them: `FROM decision d`. The mirror of
/// [`T`], and the alias [`decision_text_term`] asks its membership of.
const DEC: col::decision::Cols = col::decision::of("d");

/// One term of a decision's word filter — the mirror of [`task_text_term`], reaching the decision's own
/// title and body, the body of one of its live comments, and the name of something attached to it or to
/// one of those comments. The labels are the one difference: only a task is placed on an axis.
fn decision_text_term(term: search::Term<'_>) -> Pred {
    const DC: col::decision_comment::Cols = col::decision_comment::of("dc");
    let own = IdSet::of(SD.table, SD.owner_id)
        .filter(Pred::eq(SD.owner_kind, search::DATASET_DECISION))
        .filter(term.pred(SD));
    let in_comment = IdSet::of(DC.table, DC.decision_id)
        .join(SD.table, on_face(search::DATASET_DECISION_COMMENT, DC.id))
        .filter(term.pred(SD));
    let attached = attachment_ids(search::DATASET_DECISION, term);
    let attached_to_comment = IdSet::of(DC.table, DC.decision_id)
        .join(A.table, hangs_off(search::DATASET_DECISION_COMMENT, DC.id))
        .join(SD.table, on_face(search::DATASET_ATTACHMENT, A.id))
        .filter(term.pred(SD));
    Pred::is_in_any(DEC.id, [own, in_comment, attached, attached_to_comment])
}

/// What is attached, as the arms above alias it.
const A: col::attachment::Cols = col::attachment::of("a");

/// The join that reaches the word index's copy of one record's face: `search_doc_face` is keyed on the
/// kind first and the owner second, so both halves belong in the `ON` — a kind alone leaves the seek with
/// every record of that kind to walk.
fn on_face<N: Nullability>(owner_kind: &str, owner: Col<Int, N>) -> Pred {
    Pred::eq(SD.owner_kind, owner_kind).and(same(SD.owner_id, owner))
}

/// The join that reaches what hangs off one record — `attachment_by_target`'s two columns, the same way.
/// Polymorphic, so the kind of the thing hung off is half of the condition.
fn hangs_off<N: Nullability>(target_type: &str, target: Col<Int, N>) -> Pred {
    Pred::eq(A.target_type, target_type).and(same(A.target_id, target))
}

/// The ids of the records of `target_type` something attached to them is named by — the filename a blob
/// came in under, or the address a link points at.
///
/// It drives from `attachment`, seeking `attachment_by_target`, and reaches the copy from there: driving
/// from the copy instead would leave the seek with only the face key's leading column.
fn attachment_ids(target_type: &str, term: search::Term<'_>) -> IdSet {
    IdSet::of(A.table, A.target_id)
        .join(SD.table, on_face(search::DATASET_ATTACHMENT, A.id))
        .filter(Pred::eq(A.target_type, target_type))
        .filter(term.pred(SD))
}

/// The predicates an already-parsed [`Filter`] stands for — one per filter term, each carrying its own
/// bind values ([`Pred`]), so the caller can `AND` them, negate them or hand them to two statements
/// without anything to keep in step.
fn filter_preds(q: &TaskQuery) -> Vec<Pred> {
    let f = q.filter;
    let mut preds: Vec<Pred> = Vec::new();

    if let Some(done) = f.done {
        // `done:` asks whether the task is **closed**, not whether it was carried out (`AMB-D-397`): a
        // task decided against is over, so it belongs with `done:true` and must stay out of `done:false`,
        // which is the shape of every mailbox query. What was actually finished is `status:done`.
        preds.push(closed(T.status).negated_if(!done));
    }
    if let Some(statuses) = &f.status {
        // `status:` is an allow-set (a comma-separated any-of): one value means the same as `= ?`, several
        // become an `IN (…)`.
        preds.push(Pred::is_in(T.status, statuses.iter().map(|s| s.as_str())));
    }
    if let Some(due) = &f.due {
        let today = q.today.to_string();
        preds.push(match due {
            DueFilter::Today => Pred::eq(T.due_on, today),
            DueFilter::Overdue => Pred::is_not_null(T.due_on)
                .and(Pred::ne(T.due_on, ""))
                .and(Pred::cmp(T.due_on, "<", today)),
            DueFilter::Week => Pred::cmp(T.due_on, ">=", today)
                .and(Pred::cmp(T.due_on, "<=", (q.today + Duration::days(7)).to_string())),
            DueFilter::None => Pred::is_blank(T.due_on),
            DueFilter::On(d) => Pred::eq(T.due_on, d.to_string()),
        });
    }
    if let Some(start) = &f.start {
        // The same reading of `start_on` the ready predicate makes (`view::not_started_until`), said in
        // SQL: declared and still ahead is the waiting queue, declared and arrived is startable, blank is
        // nothing declared. Dates compare lexicographically (`YYYY-MM-DD`), as everywhere a day column is
        // compared here.
        let today = q.today.to_string();
        let declared = !Pred::is_blank(T.start_on);
        preds.push(match start {
            StartFilter::Today => declared.and(Pred::cmp(T.start_on, "<=", today.as_str())),
            StartFilter::Future => declared.and(Pred::cmp(T.start_on, ">", today.as_str())),
            StartFilter::None => Pred::is_blank(T.start_on),
        });
    }
    if let Some(pri) = &f.priority {
        preds.push(match pri {
            None => Pred::is_blank(T.priority),
            Some(p_val) => Pred::eq(T.priority, p_val.as_str()),
        });
    }
    debug_assert!(f.project_ref.is_none(), "Filter::resolve was not run (`project:` is silently dropped)");
    if let Some(project_id) = f.project_id {
        // `project:` may be written as a key or as a name, but only a resolved id gets this far: a
        // reference that resolves to nothing is an error at the read's entry point (`query::list`), so it
        // can never quietly become an empty result — or every task.
        preds.push(Pred::eq(T.project_id, project_id));
    }
    if let Some(t) = &f.text {
        // The words match over the word index's normalised copy: the task's own faces (title, notes) and
        // the faces of its live comments. Every term must land somewhere on the task — the terms are
        // ANDed, each on any one face — so a two-word text is two predicates, not one.
        for term in search::terms(t) {
            preds.push(task_text_term(search::Term::Inline(&term)));
        }
    }
    if let Some(nf) = &f.number {
        // Conversational-number filter (`number:`/`ref:`). A `D-` typed value names a decision, so it
        // matches no task; a bare number / `#n` / `T-n` matches the task id (the id **is** the number).
        // Mirrors `NumberFilter::matches_task`.
        preds.push(if nf.require_decision == Some(true) {
            Pred::never()
        } else {
            Pred::eq(T.id, nf.number as i64)
        });
    }
    if let Some(assignee) = &f.assignee {
        preds.push(assignee_pred(assignee));
    }
    if let Some(ai) = f.ai {
        // `ai:true|false` is the AI-delegation dimension (`assignee_kind = ai`), independent of the
        // assignee one. Mirrors `Filter::matches`.
        preds.push(if ai {
            Pred::eq(T.assignee_kind, ActorKind::Ai.as_str())
        } else {
            Pred::is_null(T.assignee_kind).or(Pred::ne(T.assignee_kind, ActorKind::Ai.as_str()))
        });
    }
    if let Some(draft) = f.draft {
        // `draft:` — the fourth premise asked for on its own, the way `start:` asks for the third. It is
        // the one premise stored as a column, so it needs no join and no clock.
        preds.push(Pred::eq(T.draft, draft));
    }
    if let Some(ready) = f.ready {
        // This is `crate::view::is_ready` restated in SQL, because a filter cannot ask four
        // booleans of a row it has not read yet. A task is held back by an *open* blocker (a live
        // dependency edge to a live blocker that has not ended), by an *unsettled premise*
        // (`unsettled_premise`), by a *start day that has not arrived*, or by a *creation not yet
        // finished*. The reserve guard (`reserve_blockers`) reads the same derivations, so the filter
        // and the guard cannot drift apart, and
        // `a_start_day_still_ahead_holds_the_task_back_on_every_read` holds this restatement to the
        // predicate on every arm.
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        const DC: col::decision::Cols = col::decision::of("dc");
        const D: col::task_dependency::Cols = col::task_dependency::of("d");
        const B: col::task::Cols = col::task::of("b");

        let premise = Exists::over(L.table)
            .join(DC.table, same(DC.id, L.decision_id))
            .filter(same(L.task_id, T.id))
            .filter(unsettled_premise(DC))
            .pred();

        let open_blocker = Exists::over(D.table)
            .join(B.table, same(B.id, D.blocked_by_id))
            .filter(same(D.task_id, T.id))
            .filter(still_open(B.status))
            .pred();

        // A `start_on` that is set and still ahead of today. Blank means nothing was declared about
        // when to start, which holds nothing back. The dates compare lexicographically (`YYYY-MM-DD`),
        // as everywhere else the read model compares a day column.
        let not_started = (!Pred::is_blank(T.start_on))
            .and(Pred::cmp(T.start_on, ">", q.today.to_string().as_str()));

        // The fourth premise, read straight off the column — the creation has not been finished.
        let still_draft = Pred::eq(T.draft, true);

        preds.push(open_blocker.or(premise).or(not_started).or(still_draft).negated_if(ready));
    }
    if let Some(decision) = f.decision {
        // `decision:` — tasks a decision links to (live link, live decision), as an EXISTS so it seeks
        // the link index instead of scanning the links per task.
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        const DL: col::decision::Cols = col::decision::of("dl");

        preds.push(
            Exists::over(L.table)
                .join(DL.table, same(DL.id, L.decision_id))
                .filter(same(L.task_id, T.id))
                .filter(Pred::eq(L.decision_id, i64::from(decision)))
                .pred(),
        );
    }
    if let Some(sha) = &f.commit {
        // `commit:` — tasks recording this SHA (the reverse chain git → task), as an EXISTS so it seeks
        // the `task_commit_by_sha` index instead of scanning a task's commits per row (O(result)). The
        // SHA arrives already normalised (the filter parser folds case through the door's `normalize`),
        // so it matches by the bytes it was stored as.
        const TC: col::task_commit::Cols = col::task_commit::of("tc");

        preds.push(
            Exists::over(TC.table)
                .filter(same(TC.task_id, T.id))
                .filter(Pred::eq(TC.sha, sha.as_str()))
                .pred(),
        );
    }
    preds.extend(f.dimensions.iter().map(dimension_pred));
    preds
}

/// `dim:<axis>=<value>` / `time_axis:<value>` → an EXISTS over the task's own assignment rows. Seeks by
/// `task_dimension_value_by_task` and matches the axis/value by primary key, so the filter stays
/// O(result) instead of scanning the link table once per task. Only resolved ids reach here: the read
/// entry point turned the axis/value names into ids and refused the ones that name nothing, so a typo is
/// an error rather than a silent zero — or, on the `=none` arm, a silent *everything* (a `NOT EXISTS`
/// over an axis that does not exist is true of every task). The `dimension_value` join still matters
/// though the ids are live by construction: an assignment can point at a value that was deleted, and
/// that assignment must read as *unassigned*.
fn dimension_pred(f: &crate::query::DimensionFilter) -> Pred {
    debug_assert!(f.resolved.is_some(), "Filter::resolve was not run (`dim:` is silently dropped)");
    // An unresolved filter names nothing, so it selects nothing — never *everything* (the `=none` arm
    // is a `NOT EXISTS`, and dropping it would let the read answer as if the filter were not there).
    let Some(resolved) = &f.resolved else { return Pred::never() };

    // The axis (and, when the filter names values, the value) as an `IN` list — one placeholder per id,
    // each carrying its own bind. The `=none` arm names no values, so it constrains the axis alone.
    let mut inner = Pred::is_in(TDV.dimension_id, resolved.axis_ids.iter().copied());
    if let Some(ids) = &resolved.value_ids {
        inner = inner.and(Pred::is_in(TDV.value_id, ids.iter().copied()));
    }
    const DV: col::dimension_value::Cols = col::dimension_value::of("dv");
    let exists = Exists::over(TDV.table)
        .join(DV.table, same(DV.id, TDV.value_id))
        .filter(same(TDV.task_id, T.id))
        .filter(inner)
        .pred();
    // `=none` = *no* live value on that axis.
    exists.negated_if(resolved.value_ids.is_none())
}

/// [`filter_preds`]'s twin on the other side: `decision list`'s own keys, said in SQL so a search can be
/// narrowed by them (`AMB-D-563`). `decision list` itself matches them in Rust over a bounded page
/// ([`crate::query::DecisionFilter::matches`]) — it can, having read the rows already; a search cuts its
/// page in SQL, so it needs them as predicates. The two must answer alike, which is why each arm below
/// names the line of `matches` it restates.
///
/// `text` is not among them: words are not a filter key on either side (`AMB-D-449`), and where a search
/// is concerned they are the search itself.
fn decision_filter_preds(f: &crate::query::DecisionFilter) -> Vec<Pred> {
    let mut preds: Vec<Pred> = Vec::new();

    if let Some(status) = f.status {
        preds.push(Pred::eq(DEC.status, status.as_str()));
    }
    if let Some(want) = f.superseded {
        // Currency is the edges' to say, never a column (`AMB-D-410`) — the very predicate the reads that
        // project `current` stand on, so the two cannot disagree about which decisions are still standing.
        preds.push(superseded(DEC).negated_if(!want));
    }
    debug_assert!(
        f.project_ref.is_none(),
        "DecisionFilter::resolve was not run (`project:` is silently dropped)"
    );
    if let Some(project_id) = f.project_id {
        preds.push(Pred::eq(DEC.project_id, project_id));
    }
    if let Some(nf) = &f.number {
        // A `T-` typed value names a task, so it matches no decision; anything else matches the decision
        // id (the id **is** the number). Mirrors `NumberFilter::matches_decision`.
        preds.push(if nf.require_decision == Some(false) {
            Pred::never()
        } else {
            Pred::eq(DEC.id, nf.number as i64)
        });
    }
    if let Some(task) = f.task {
        // `task:` — the decisions a task rests on, walked through the link (live link, live task), as an
        // EXISTS so it seeks the link index rather than scanning the links per decision. The mirror of
        // `decision:` on the task side.
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        const TL: col::task::Cols = col::task::of("tl");

        preds.push(
            Exists::over(L.table)
                .join(TL.table, same(TL.id, L.task_id))
                .filter(same(L.decision_id, DEC.id))
                .filter(Pred::eq(L.task_id, i64::from(task)))
                .pred(),
        );
    }
    // Acceptance time, at day granularity with the named day included at each end. The stored instant is
    // UTC text of one fixed width, so the cut is lexicographic — and it has to be the instant the named
    // day begins *here* (`time::local_day_start_utc`), or the day would be UTC's and so the wrong hours
    // everywhere else. A decision never accepted carries no instant and matches neither direction, which
    // `is_not_null` says outright rather than leaving to how NULL compares.
    if let Some(d) = f.decided_after {
        preds.push(Pred::is_not_null(DEC.decided_at).and(Pred::cmp(
            DEC.decided_at,
            ">=",
            crate::time::local_day_start_utc(d).to_rfc3339_z(),
        )));
    }
    if let Some(d) = f.decided_before {
        preds.push(Pred::is_not_null(DEC.decided_at).and(Pred::cmp(
            DEC.decided_at,
            "<",
            crate::time::local_day_start_utc(d + Duration::days(1)).to_rfc3339_z(),
        )));
    }
    preds
}

fn assignee_pred(a: &AssigneeFilter) -> Pred {
    match a {
        AssigneeFilter::None => Pred::is_null(T.assignee_kind),
        AssigneeFilter::Me => Pred::eq(T.assignee_kind, ActorKind::Human.as_str()),
        AssigneeFilter::MeAi => Pred::eq(T.assignee_kind, ActorKind::Ai.as_str()),
    }
}

/// The `priority` rank as an `ORDER BY` term: high → medium → low → none, which is neither the column's
/// storage order nor a comparison the predicate layer has a shape for — a `CASE` over the enum's own
/// words is the one thing here that stays written out (the column it ranks still comes from the
/// registry). Mirrors `view::priority_rank` and [`status_bucket_ids`]'s twin.
fn priority_rank(priority: Col<SqlText, Nullable>) -> String {
    format!(
        "CASE {} WHEN 'high' THEN 0 WHEN 'medium' THEN 1 WHEN 'low' THEN 2 ELSE 255 END",
        priority.to_sql()
    )
}

/// Build the `ORDER BY` body for `sort`. Ties break on `id`; `-` reverses the whole ordering, so every
/// component (including the `id` tiebreak and any NULLs-grouping term) flips direction together. Every
/// term names its column through the registry, so a
/// sort key cannot outlive the column it sorts by, and the "not written" grouping terms are
/// [`Pred::is_blank`]'s own fragment — the same reading of an unwritten text column the `due:none` filter
/// uses, not a second spelling of it.
fn order_by(sort: &str) -> Result<Vec<Sort>> {
    let desc = sort.starts_with('-');
    let key = sort.trim_start_matches('-');
    let comps: Vec<String> = match key {
        // Placement is task-held: sort by the task's own order_key.
        "order" => vec![format!("COALESCE({}, '')", T.order_key.to_sql())],
        // NULL/empty due dates sort last (ascending); the grouping term flips with `-`.
        "due" => vec![Pred::is_blank(T.due_on).sql().to_string(), T.due_on.to_sql()],
        "priority" => vec![priority_rank(T.priority)],
        "created" => vec![T.created_at.to_sql()],
        // NULL/empty completed dates sort last (ascending); the grouping term flips with `-`.
        // Mirrors `due` and `crate::query::cmp_opt` (None last). Used by the GUI archive view via
        // `-completed` (most-recently-completed first).
        "completed" => vec![Pred::is_blank(T.completed_at).sql().to_string(), T.completed_at.to_sql()],
        "title" => vec![T.title.to_sql()],
        other => return Err(StoreEngineError::InvalidSort(other.to_string())),
    };
    // The direction is the caller's (`-due`), the keys are the registry's — and the `id` tiebreak takes
    // the same direction, so a reversed sort reverses whole.
    let mut keys: Vec<Sort> = comps.into_iter().map(|c| Sort::expr(c).dir(desc)).collect();
    keys.push(Sort::by(T.id).dir(desc));
    Ok(keys)
}

/// The subset of decision `ids` that exist in this read-model, by an indexed primary-key lookup — the
/// decision counterpart of [`present_task_ids`]. Order is unspecified: the caller re-orders by its input.
pub fn present_decision_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<i64>> {
    const D: col::decision::Cols = col::decision::ALL;
    select_ids(conn, D.id, Some(&Pred::is_in(D.id, ids.iter().copied())))
}

/// The subset of `ids` that exist in this read-model, by an indexed primary-key lookup. The card itself
/// is still hydrated by the caller from its source. Order is unspecified — the caller re-orders by its
/// input.
pub fn present_task_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<i64>> {
    const TA: col::task::Cols = col::task::ALL;
    select_ids(conn, TA.id, Some(&Pred::is_in(TA.id, ids.iter().copied())))
}

/// One row of the change feed, as a reader sees it: the cursor value, and the instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedRow {
    /// The feed's monotonic id — what a reader passes back as "everything after this".
    pub id: i64,
    /// The dataset's stable key (`task`, `decision`, …).
    pub dataset: String,
    /// The changed row's id (the conversational number, which is the primary key).
    pub row_id: i64,
    /// `insert` / `update` / `delete`.
    pub op: String,
}

/// What a reader gets back when it asks the feed for everything after its cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedSlice {
    /// The changes since the cursor, oldest first. `more` says the `limit` cut the page short, so the
    /// reader should come back with the last id it saw.
    Changes { rows: Vec<FeedRow>, more: bool },
    /// **The cursor is gone.** Truncation has removed rows the reader had not seen, so the feed can no
    /// longer say what changed — the reader must reconcile from the truth source (re-read what it shows)
    /// rather than conclude that nothing did. Saying this out loud is the whole reason truncation records
    /// a watermark: an empty answer would look exactly like "no changes" and freeze a stale screen.
    Gap,
}

/// The change feed after a cursor. `limit` bounds one read, so a reader that has been away drains the
/// feed in pages instead of materialising it; it returns [`FeedSlice::Gap`] when truncation has passed
/// the cursor, and a reader with no cursor yet (`after_id = 0`) is in the same position by definition
/// once anything has been trimmed, and is told so.
pub fn changes_since(conn: &Connection, after_id: i64, limit: i64) -> Result<FeedSlice> {
    // The store's scalars are text (`store_meta` is one key/value table for all of them), so the
    // watermark is read back as the integer it was written as. A store that has never truncated carries
    // no row at all — which is the same answer as `0`, said by the absence rather than by a `COALESCE`.
    let mut sel = Select::new();
    let mark = sel.expr::<i64>(format!("CAST({} AS INTEGER)", META.value.to_sql()));
    let mut sql = Sql::from(&sel, META.table);
    sql.push_where(Some(&Pred::eq(META.key, super::engine::META_FEED_TRUNCATED_THROUGH)));
    let truncated_through: i64 = conn
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| mark.get(r))
        .optional()
        .map_err(StoreEngineError::from)?
        .unwrap_or(0);
    if after_id < truncated_through {
        return Ok(FeedSlice::Gap);
    }
    // One row past the page, so "is there more?" costs no second query.
    let mut sel = Select::new();
    let (id, dataset, row_id, op) = (sel.col(FEED.id), sel.col(FEED.dataset), sel.col(FEED.row_id), sel.col(FEED.op));
    let mut page = Sql::from(&sel, FEED.table);
    page.push_where(Some(&Pred::cmp(FEED.id, ">", after_id)))
        .order_by([Sort::by(FEED.id)])
        .limit(limit.saturating_add(1));
    let mut stmt = conn.prepare(page.text()).map_err(StoreEngineError::from)?;
    let mut rows = stmt
        .query_map(rusqlite::params_from_iter(page.params()), |r| {
            Ok(FeedRow { id: id.get(r)?, dataset: dataset.get(r)?, row_id: row_id.get(r)?, op: op.get(r)? })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(StoreEngineError::from)?;
    let more = rows.len() as i64 > limit;
    rows.truncate(limit.max(0) as usize);
    Ok(FeedSlice::Changes { rows, more })
}

/// The feed's newest id — the cursor a reader starts from when it has just loaded the store from the
/// truth source and only wants what happens *next*. `0` on an empty feed.
pub fn change_feed_head(conn: &Connection) -> Result<i64> {
    let mut sel = Select::new();
    // An aggregate over no rows is `NULL`, and an empty feed's head is `0` — the registry types the
    // column, not what `MAX` does to it.
    let head = sel.expr::<i64>(format!("COALESCE(MAX({}), 0)", FEED.id.to_sql()));
    let sql = Sql::from(&sel, FEED.table);
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| head.get(r))
        .map_err(StoreEngineError::from)
}

/// Whether any live `blob` attachment still points at these bytes — the single-hash question the delete
/// path asks instead of materialising the whole root set. Content-addressing means two attachments can
/// share one blob, so "the attachment that held it is gone" is *not* "the bytes are garbage": only this
/// answers that.
pub fn is_blob_referenced(conn: &Connection, hash: &str) -> Result<bool> {
    exists_row(conn, ATT.id, &holds_blob(hash))
}

/// The attachment's columns, unaliased — the table names itself.
const ATT: col::attachment::Cols = col::attachment::ALL;

/// An attachment in `blob` mode holding exactly these bytes. `kind` is what makes the read ignore the
/// `url`-mode rows, whose `blob_hash` is `NULL` and could never match anyway — saying it keeps the
/// question ("is this blob referenced") the same one the refcount answers.
fn holds_blob(hash: &str) -> Pred {
    Pred::eq(ATT.kind, "blob").and(Pred::eq(ATT.blob_hash, hash))
}

/// Blob-mode attachments that carry a hash at all — the set both hash readers project.
fn any_blob() -> Pred {
    Pred::eq(ATT.kind, "blob").and(Pred::is_not_null(ATT.blob_hash))
}

/// Distinct blob hashes over the attachments the predicate selects. The column is nullable in the
/// registry (a `url`-mode row carries none), so it reads back as an `Option` and the `None`s fall out
/// here rather than being claimed away in the projection.
fn blob_hashes<C: FromIterator<String>>(conn: &Connection, pred: &Pred) -> Result<C> {
    let mut sel = Select::new();
    sel.distinct();
    let hash = sel.col(ATT.blob_hash);
    let mut sql = Sql::from(&sel, ATT.table);
    sql.push_where(Some(pred));
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| hash.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<Option<String>>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows.into_iter().flatten().collect())
}

/// The blob hashes of the attachments hanging off one polymorphic target — read *before* the rows go, so
/// a delete can hand its caller the exact set of blobs it may have orphaned.
pub fn blob_hashes_for_target(
    conn: &Connection,
    target_type: &str,
    target_id: i64,
) -> Result<Vec<String>> {
    blob_hashes(conn, &any_blob().and(on_target(target_type, target_id)))
}

/// Every blob hash the store's attachments reference — the root set of the blob GC.
pub fn referenced_blob_hashes(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    blob_hashes(conn, &any_blob())
}

/// How many live `blob` attachments reference `hash` — its refcount. Zero means the bytes are
/// collectible. Counts the stored metadata rows, independent of whether the bytes are present locally.
pub fn blob_refcount(conn: &Connection, hash: &str) -> Result<i64> {
    let mut sel = Select::new();
    let count = sel.count_all();
    let mut sql = Sql::from(&sel, ATT.table);
    sql.push_where(Some(&holds_blob(hash)));
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| count.get(r))
        .map_err(StoreEngineError::from)
}

/// The attachments of one polymorphic target. The target is a `(type, id)` pair rather than a foreign
/// key — no `REFERENCES` can branch on a sibling column — so the two halves are asked for together, and
/// an id is never matched without the type that says what it names.
fn on_target(target_type: &str, target_id: i64) -> Pred {
    Pred::eq(ATT.target_type, target_type).and(Pred::eq(ATT.target_id, target_id))
}

/// One live attachment's metadata, as the store holds it. The blob bytes are
/// out-of-band, so this is purely the metadata the GUI viewer needs to dispatch on `mime` and to
/// build a stream URL (`blob_hash`); `url`-mode rows carry `url` instead. `created_by_kind` is the
/// stored facet text (`human`/`ai`) or `None` when there is none.
#[derive(Debug, Clone)]
pub struct AttachmentRow {
    pub id: i64,
    pub kind: String,
    pub blob_hash: Option<String>,
    pub filename: Option<String>,
    pub mime: Option<String>,
    pub size_bytes: Option<i64>,
    pub url: Option<String>,
    pub created_by_kind: Option<String>,
}

/// Live attachments on one target (task/decision), in attach order — the read-model query behind the
/// GUI viewer. Mirrors the CLI `attach ls` set (live only, ordered by `order_key`), read off the indexed
/// read-model so the GUI read path stays O(result).
pub fn attachments_for_target(
    conn: &Connection,
    target_type: &str,
    target_id: i64,
) -> Result<Vec<AttachmentRow>> {
    let mut sel = Select::new();
    let (id, kind, blob_hash, filename) =
        (sel.col(ATT.id), sel.col(ATT.kind), sel.col(ATT.blob_hash), sel.col(ATT.filename));
    let (mime, size_bytes, url, created_by_kind) =
        (sel.col(ATT.mime), sel.col(ATT.size_bytes), sel.col(ATT.url), sel.col(ATT.created_by_kind));
    let mut sql = Sql::from(&sel, ATT.table);
    sql.push_where(Some(&on_target(target_type, target_id)))
        .order_by([Sort::by(ATT.order_key), Sort::by(ATT.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(AttachmentRow {
                id: id.get(r)?,
                kind: kind.get(r)?,
                blob_hash: blob_hash.get(r)?,
                filename: filename.get(r)?,
                mime: mime.get(r)?,
                size_bytes: size_bytes.get(r)?,
                url: url.get(r)?,
                created_by_kind: created_by_kind.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// Every live task id in the read-model (one indexed pass over `task`). The GUI's device-state GC
/// (`gc_device_state`) unions this across stores to know which read-receipt / inbox-archive ids are
/// still real; it needs the full live set, not a page.
pub fn live_task_ids(conn: &Connection) -> Result<Vec<i64>> {
    select_ids(conn, col::task::ALL.id, None)
}

/// Resolve a task's current title from the read-model, or `None` when no live task carries that id
/// (a non-task row, or a deleted/absent task). Used to label rows that are keyed by a raw id
/// (e.g. a task id) without the caller touching SQL.
pub fn task_title(conn: &Connection, id: i64) -> Result<Option<String>> {
    const TA: col::task::Cols = col::task::ALL;
    scalar_by_id(conn, TA.id, TA.title, id)
}

/// Live task ids carrying friendly `number` (the `AMB-T-n` / `#n` / `T-n` resolve). Numbers are device-global, so
/// this is context-free — one number names at most one live task. Mirrors the predicate of
/// [`crate::ops::task`]'s `resolve_number`.
pub fn task_ids_by_number(conn: &Connection, number: u32) -> Result<Vec<i64>> {
    const TA: col::task::Cols = col::task::ALL;
    select_ids(conn, TA.id, Some(&Pred::eq(TA.id, i64::from(number))))
}

/// The task-comment id `reference` names, or none. A comment carries no conversational number, so its
/// key **is** its whole handle — the reference is the decimal key, exactly. Returns a `Vec` (0 or 1) so
/// the caller collapses it with the same `pick_id` every other resolver uses.
pub fn resolve_task_comment(conn: &Connection, reference: &str) -> Result<Vec<i64>> {
    resolve_by_key(conn, crate::idref::RefKind::TaskComment, col::task_comment::ALL.id, reference)
}

/// The attachment id `reference` names — behind the CLI's `attach show` / `open` / `rm`, which address
/// an attachment by its key. A removed attachment is not addressable: its row is gone.
pub fn resolve_attachment(conn: &Connection, reference: &str) -> Result<Vec<i64>> {
    resolve_by_key(conn, crate::idref::RefKind::Attachment, col::attachment::ALL.id, reference)
}

/// The live decision-comment id `reference` names — the decision counterpart of
/// [`resolve_task_comment`]. The two comment tables number independently, so a caller may query both
/// and treat any hit as unambiguous within its own table.
pub fn resolve_decision_comment(conn: &Connection, reference: &str) -> Result<Vec<i64>> {
    resolve_by_key(conn, crate::idref::RefKind::DecisionComment, col::decision_comment::ALL.id, reference)
}

/// Resolve a reference that is nothing but a key, against the table `id` belongs to. A reference that
/// is not a decimal key (empty, blank, a word) matches nothing — it must fail to resolve, never
/// silently hit the only row.
fn resolve_by_key(
    conn: &Connection,
    kind: crate::idref::RefKind,
    id: Col<Int, NotNull>,
    reference: &str,
) -> Result<Vec<i64>> {
    let Some(n) = crate::ops::parse_id_ref(kind, reference) else {
        return Ok(Vec::new());
    };
    select_ids(conn, id, Some(&Pred::eq(id, n)))
}


/// Live decision ids carrying friendly `number` (the `AMB-D-n` / `D-n` / `#n` resolve). Decisions number in their
/// own device-global space, so this scans only `decision` and needs no project context. Mirrors
/// [`crate::ops::decision`]'s `resolve_number`.
pub fn decision_ids_by_number(conn: &Connection, number: u32) -> Result<Vec<i64>> {
    const D: col::decision::Cols = col::decision::ALL;
    select_ids(conn, D.id, Some(&Pred::eq(D.id, i64::from(number))))
}

/// Live project ids resolved by **key OR exact name**. Collapsed by `pick_id`; `id`/`name` overlap
/// cannot double-count since `SELECT id` runs over distinct rows. The name arm is why
/// `--project '<full name>'` resolves, not just the key.
pub fn resolve_project(conn: &Connection, reference: &str) -> Result<Vec<i64>> {
    const P: col::project::Cols = col::project::ALL;
    select_ids(conn, P.id, Some(&key_or_name_pred(crate::idref::RefKind::Project, P.id, P.name, reference)))
}

/// The greatest id ever **issued** for `tables` — not the greatest currently present. The distinction
/// matters because deletes are physical: `MAX(id)` alone would hand the number of a deleted row straight
/// to the next record, and a conversational number is a **name** that must keep meaning the record it was
/// written for. So the record tables are `INTEGER PRIMARY KEY AUTOINCREMENT` (see [`super::schema`]) and
/// SQLite keeps the high-water mark in `sqlite_sequence`, which a delete does not lower. `MAX(id)` is
/// still taken alongside it: a store whose tables carry no `AUTOINCREMENT` (or an engine opened unchecked
/// in a test) has no sequence row, and the live maximum is the best it can offer. Raw by necessity: the
/// tables come in **as names**, and the two tables read here — `sqlite_master`, `sqlite_sequence` — are
/// SQLite's own bookkeeping, which the registry does not declare and `col::` therefore cannot name.
fn high_water(conn: &Connection, tables: &[&str]) -> Result<i64> {
    let has_seq: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'sqlite_sequence')",
            [],
            |r| r.get(0),
        )
        .map_err(StoreEngineError::from)?;
    let mut mark = 0i64;
    for table in tables {
        let present: i64 = conn
            .query_row(&format!("SELECT COALESCE(MAX(id), 0) FROM \"{table}\""), [], |r| r.get(0))
            .map_err(StoreEngineError::from)?;
        mark = mark.max(present);
        if has_seq {
            let issued: i64 = conn
                .query_row(
                    "SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name = ?1), 0)",
                    [table],
                    |r| r.get(0),
                )
                .map_err(StoreEngineError::from)?;
            mark = mark.max(issued);
        }
    }
    Ok(mark)
}

/// The next id of the **activity sequence** — one counter shared by the file ledger's system events and
/// the `task_comment` table. Those two streams pull from one sequence so that `id` alone separates them:
/// with a counter each, both would hold an id `1`, and the timeline's key would tie within one second.
/// The numbers are sparse in the table (an event consumes one the next comment will not take), which costs
/// nothing because these ids are internal, unlike `task.id`/`decision.id` (the conversational numbers).
/// Only one of the two streams leaves a row behind — a system event lives in the file alone, so there is no
/// `MAX(id)` for the next call to see, and two events in a row would be handed the same id; every event
/// therefore parks its id in [`ACTIVITY_HIGH_WATER`], inside the transaction that caused it, and this is
/// where the two marks meet.
///
/// It is **not** the timeline's whole key. `decision_comment` is a third source on that timeline and
/// numbers its rows against its own table, so the key carries which sequence a row came from
/// ([`crate::activity::Seq`]) — this counter is what makes the tie-break work *within* the pair above,
/// not across every source. A new source is free to number itself, as long as the key can name it.
pub fn next_activity_id(conn: &Connection) -> Result<i64> {
    let rows = high_water(conn, &["task_comment"])?;
    Ok(rows.max(activity_high_water(conn)?) + 1)
}

/// `PRAGMA data_version` — SQLite's own answer to *"has another connection committed?"*. The number
/// itself means nothing; only a **change of it, read from the same connection, means something**: it moves
/// when any *other* connection commits, and never for this connection's own writes. That is exactly the
/// question a watcher asks, and the one a file's timestamps only guess at — in WAL mode an external
/// writer's commit lands in `store.sqlite-wal` and leaves the main file's mtime untouched. Two
/// consequences the caller has to hold on to: keep the connection, because values from two different
/// connections are not comparable (SQLite says so), so a poller that opens a fresh connection each round
/// learns nothing; and a file swapped out from under it is a separate question — a rewrite that replaces
/// the file (`fold`, `restore`, a migration) leaves the old connection reading the old inode, where
/// nothing will ever commit again, so the caller watches the file's identity for that and reopens rather
/// than expecting this to report it.
pub fn data_version(conn: &Connection) -> Result<i64> {
    conn.query_row("PRAGMA data_version", [], |r| r.get(0)).map_err(StoreEngineError::from)
}

/// `store_meta` key holding the highest activity id handed to a **row-less** event (a system event, which
/// lives only in the file ledger), so the sequence keeps climbing across events the DB does not remember.
pub const ACTIVITY_HIGH_WATER: &str = "activity_high_water";

/// The mark [`ACTIVITY_HIGH_WATER`] carries — 0 when no row-less event was ever recorded, and 0 again if
/// the scalar is somehow unreadable: a lost mark can only mean re-using an id the ledger already passed,
/// never skipping one, so the sequence degrades the same way the ledger does.
fn activity_high_water(conn: &Connection) -> Result<i64> {
    Ok(crate::store_engine::engine::read_meta(conn, ACTIVITY_HIGH_WATER)
        .map_err(StoreEngineError::from)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0))
}

/// The next dense INTEGER `id` for `table` — one above the highest ever issued ([`high_water`]), or 1 on
/// an empty table (rowids start at 1). The ops layer central-allocates it here: this is the read half of
/// a create's read-then-write, taken under the write lock the create's `BEGIN IMMEDIATE` already holds,
/// so two concurrent writers cannot mint the same id. For `task` / `decision` this id **is** the
/// conversational number; for every other record table it is an internal dense key. `table` is a
/// `'static` registry literal at each call site, never user input, so the interpolation is not an
/// injection vector.
pub fn next_id(conn: &Connection, table: &str) -> Result<i64> {
    Ok(high_water(conn, &[table])? + 1)
}

/// The greatest `order_key` among the live attachments of one target, or `None` when it has none. A new
/// attachment goes after it, so the read must sit inside the write's transaction — two concurrent
/// attachments to the same target would otherwise read the same maximum and take the same key.
pub fn max_attachment_order_key(
    conn: &Connection,
    target_type: &str,
    target_id: i64,
) -> Result<Option<String>> {
    const A: col::attachment::Cols = col::attachment::ALL;
    let mut sel = Select::new();
    // MAX over a column: an aggregate, so the registry cannot type it — an empty target has no row to
    // take the maximum of, and the aggregate answers NULL rather than nothing.
    let max = sel.expr::<Option<String>>(format!("MAX({})", A.order_key.name()));
    let pred = Pred::eq(A.target_type, target_type).and(Pred::eq(A.target_id, target_id));
    let mut sql = Sql::from(&sel, A.table);
    sql.push_where(Some(&pred));
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| max.get(r))
        .map_err(StoreEngineError::from)
}

/// The placement siblings `(task_id, order_key)` for a task home in `project_id`, restricted to live
/// tasks, ordered by `order_key` ASC (BINARY collation agrees with Rust's byte-wise `String` ordering
/// that [`crate::ops::place`] assumes). `exclude_task` drops one task id (the one being moved); the
/// caller feeds the result to `ops::place` unchanged.
pub fn placement_siblings(
    conn: &Connection,
    project_id: i64,
    exclude_task: Option<i64>,
) -> Result<Vec<(i64, String)>> {
    // Placement is task-held: read the task's own project/order_key.
    let mut sel = Select::new();
    let (id, order_key) = (sel.col(T.id), sel.col(T.order_key));
    let pred = Pred::is_not_null(T.order_key).and(Pred::eq(T.project_id, project_id));
    let mut sql = Sql::from(&sel, T.table);
    sql.push_where(Some(&pred)).order_by([Sort::by(T.order_key)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            // `order_key` is nullable in the registry, and the predicate above is what makes it present.
            Ok((id.get(r)?, order_key.get(r)?.unwrap_or_default()))
        })
        .map_err(StoreEngineError::from)?;
    let mut out = Vec::new();
    for row in rows {
        let (tid, order_key) = row.map_err(StoreEngineError::from)?;
        if exclude_task == Some(tid) {
            continue;
        }
        out.push((tid, order_key));
    }
    Ok(out)
}


/// The task's current `status`, or `None` when there is no such row — the read half of the
/// reserve guard's compare-and-swap: `todo → in_progress` may only succeed from `todo`, and the
/// transition it guards against is *another process* having reserved the task. Only the truth source can
/// answer that, so the write path reads the status here, inside its own `BEGIN IMMEDIATE` transaction,
/// where no second writer can slip between the read and the UPDATE. An unparseable status is treated as
/// `None`: a row the reserve guard cannot reason about must not be reservable.
pub fn task_status(conn: &Connection, id: i64) -> Result<Option<crate::model::TaskStatus>> {
    const TA: col::task::Cols = col::task::ALL;
    Ok(scalar_by_id(conn, TA.id, TA.status, id)?
        .as_deref()
        .and_then(crate::model::TaskStatus::parse))
}

/// The project this task is filed under, or `None` when it is unfiled (inbox) or gone. Read inside the
/// system event's transaction, because the activity ledger's line carries the project itself — the file
/// cannot join against the DB.
pub fn task_project_id(conn: &Connection, id: i64) -> Result<Option<i64>> {
    const TA: col::task::Cols = col::task::ALL;
    Ok(scalar_by_id(conn, TA.id, TA.project_id, id)?.flatten())
}

/// What holds `ready` down for this task ([`crate::view::ReserveBlocker`]), read inside the reserving
/// transaction so the guard sees the truth source rather than the caller's snapshot (the same reason the
/// CAS reads [`task_status`] here). Open blockers first, then unsettled premises, each in edge-`id`
/// order, then a start day that has not arrived; the premise query resolves the superseding decision in
/// the same pass, so the error can name the relink target. The three arms are the three premises of
/// [`crate::view::is_ready`], so the guard refuses exactly what the reads call not ready — `today` is the
/// caller's reference day, for the same reason the reads take one.
pub fn reserve_blockers(
    conn: &Connection,
    task_id: i64,
    today: NaiveDate,
) -> Result<Vec<crate::view::ReserveBlocker>> {
    use crate::view::ReserveBlocker;

    let mut out = Vec::new();
    {
        const B: col::task::Cols = col::task::of("b");
        const DEP: col::task_dependency::Cols = col::task_dependency::of("d");
        // `id` **is** the conversational number, and it is never null.
        let mut sel = Select::new();
        let number = sel.col(B.id);
        let pred = Pred::eq(DEP.task_id, task_id).and(still_open(B.status));
        let mut sql = Sql::from(&sel, DEP.table);
        sql.join(B.table, same(B.id, DEP.blocked_by_id))
            .push_where(Some(&pred))
            .order_by([Sort::by(DEP.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok(ReserveBlocker::OpenBlocker { label: crate::idref::task(number.get(r)?) })
            })
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreEngineError::from)?;
        out.extend(rows);
    }
    {
        // An `id` **is** the conversational number (`D-<id>`); the premise's is never null, the
        // successor's is (the `LEFT JOIN` finds no row when nothing supersedes it) — which is why the
        // successor is named as an expression rather than the column, whose registry type says NOT NULL.
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        const DC: col::decision::Cols = col::decision::of("dc");
        const SUCC: col::decision::Cols = col::decision::of("succ");
        let mut sel = Select::new();
        let (number, status) = (sel.col(DC.id), sel.col(DC.status));
        // The successor is reached through a `LEFT JOIN`, so its `NOT NULL` id comes back NULL when
        // nothing supersedes the premise — the optionality is the join's, which the registry cannot say
        // (see `Col::nullable`).
        let successor_number = sel.col(SUCC.id.nullable());

        // Which premises block is `unsettled_premise`, shared with the `ready:` filter and the card
        // reads; who supersedes one is `first_superseder`, shared with `premise_successor` — so the
        // guard cannot come to disagree with the reads about either half.
        let mut sql = Sql::from(&sel, L.table);
        sql.join(DC.table, same(DC.id, L.decision_id))
            // The successor is the scalar subquery's answer, not a column of a table this query names —
            // the only `ON` here that is not a column-to-column `same`.
            .left_join(
                SUCC.table,
                Pred::plain(format!("{} = {}", SUCC.id.to_sql(), first_superseder(DC.id))),
            )
            .push_where(Some(&Pred::eq(L.task_id, task_id).and(unsettled_premise(DC))))
            .order_by([Sort::by(L.id)]);

        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((number.get(r)?, status.get(r)?, successor_number.get(r)?))
            })
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreEngineError::from)?;
        for (number, status, successor_number) in rows {
            let Some(status) = crate::model::DecisionStatus::parse(&status) else { continue };
            out.push(ReserveBlocker::UnsettledPremise {
                label: crate::idref::decision(number),
                status,
                superseded_by: successor_number.map(crate::idref::decision),
            });
        }
    }
    {
        // The last two premises, both read off the row this guard is about, from the same transaction as
        // the two above — a `start_on` the caller edited a moment ago, or a creation they finished a
        // moment ago, is already in view.
        const TA: col::task::Cols = col::task::ALL;
        let stored = scalar_by_id(conn, TA.id, TA.start_on, task_id)?.flatten();
        // A date that will not parse is raised, not shrugged off: letting a reservation through on an
        // unreadable declaration is the one answer that cannot be right.
        if let Some(start_on) = parse_card_date(stored).map_err(StoreEngineError::from)? {
            if start_on > today {
                out.push(ReserveBlocker::NotStartedYet { start_on });
            }
        }
        // Last, because it is the reason a reader acts on last: the other three name work elsewhere, and
        // this one is settled on the task in front of them (`AMB-D-553`).
        if scalar_by_id(conn, TA.id, TA.draft, task_id)?.unwrap_or(false) {
            out.push(ReserveBlocker::StillDraft);
        }
    }
    Ok(out)
}

/// EXISTS a task with this id (a row exists ⇒ it is live) — the liveness guard the write path runs
/// before a move ([`crate::ops::task::move_to`]).
pub fn task_live(conn: &Connection, id: i64) -> Result<bool> {
    const TA: col::task::Cols = col::task::ALL;
    exists_row(conn, TA.id, &Pred::eq(TA.id, id))
}

/// The project a task belongs to — the one column a reach check needs ([`crate::reach::Reach`]).
/// `None` = the task is unplaced (no project) **or** there is no such row; both are "not inside any
/// project", which is what the caller asks about. O(1) on the primary key.
pub fn task_project(conn: &Connection, id: i64) -> Result<Option<i64>> {
    const TA: col::task::Cols = col::task::ALL;
    Ok(scalar_by_id(conn, TA.id, TA.project_id, id)?.flatten())
}

/// The project a decision belongs to — the decision twin of [`task_project`].
pub fn decision_project(conn: &Connection, id: i64) -> Result<Option<i64>> {
    const D: col::decision::Cols = col::decision::ALL;
    // A decision's project is NOT NULL in the registry, so the only `None` here is "no such row".
    scalar_by_id(conn, D.id, D.project_id, id)
}

/// Live `(id, order_key)` rows from `table`, optionally scoped by a `(column, value)` owner filter,
/// dropping `exclude`, ascending by `order_key` (BINARY collation agrees with Rust's byte-wise `String`
/// ordering, which [`crate::ops::place`] assumes when it computes a between-key). Shared by the project
/// / dimension / dimension value sibling twins.
fn order_siblings(
    conn: &Connection,
    id: Col<Int, NotNull>,
    order_key: Col<SqlText, NotNull>,
    scope: Option<Pred>,
    exclude: Option<i64>,
) -> Result<Vec<(i64, String)>> {
    let mut sel = Select::new();
    let (id_slot, key_slot) = (sel.col(id), sel.col(order_key));
    let mut sql = Sql::from(&sel, id.table());
    sql.push_where(scope.as_ref()).order_by([Sort::by(order_key)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok((id_slot.get(r)?, key_slot.get(r)?))
        })
        .map_err(StoreEngineError::from)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, order_key) = row.map_err(StoreEngineError::from)?;
        if exclude == Some(id) {
            continue;
        }
        out.push((id, order_key));
    }
    Ok(out)
}

/// Live project placement siblings, in `order_key` order.
pub fn project_siblings(conn: &Connection, exclude: Option<i64>) -> Result<Vec<(i64, String)>> {
    const P: col::project::Cols = col::project::ALL;
    order_siblings(conn, P.id, P.order_key, None, exclude)
}

/// Live dimension siblings within one project. Read inside the operation's transaction, because
/// `order_key` placement is a read-then-write: two writers that scan the same siblings both land on the
/// same key.
pub fn dimension_siblings(
    conn: &Connection,
    project_id: i64,
    exclude: Option<i64>,
) -> Result<Vec<(i64, String)>> {
    const D: col::dimension::Cols = col::dimension::ALL;
    order_siblings(conn, D.id, D.order_key, Some(Pred::eq(D.project_id, project_id)), exclude)
}

/// Live value siblings within one dimension. Same read-then-write reason as [`dimension_siblings`].
pub fn dimension_value_siblings(
    conn: &Connection,
    dimension_id: i64,
    exclude: Option<i64>,
) -> Result<Vec<(i64, String)>> {
    const V: col::dimension_value::Cols = col::dimension_value::ALL;
    order_siblings(conn, V.id, V.order_key, Some(Pred::eq(V.dimension_id, dimension_id)), exclude)
}

/// Every project slug already in use. The read half of `ops::project`'s slug derivation, which is a
/// read-then-write: two writers that scan the same set both derive `amenbo-2`, and the
/// `project_by_slug` unique index would then fail the second commit. Read it inside the writer's
/// transaction.
pub fn taken_project_slugs(conn: &Connection) -> Result<std::collections::HashSet<String>> {
    const P: col::project::Cols = col::project::ALL;
    let mut sel = Select::new();
    let slug = sel.col(P.slug);
    // A slug that was never written reads as absent whether it is `NULL` or `''` — the store's one
    // reading of "not written" (`Pred::is_blank`), so what is *taken* is its negation.
    let mut sql = Sql::from(&sel, P.table);
    sql.push_where(Some(&!Pred::is_blank(P.slug)));
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| slug.get(r))
        .map_err(StoreEngineError::from)?;
    let mut out = std::collections::HashSet::new();
    for row in rows {
        // The predicate has already dropped the unwritten ones, so every row carries a slug.
        if let Some(slug) = row.map_err(StoreEngineError::from)? {
            out.insert(slug);
        }
    }
    Ok(out)
}

/// Whether a record with this id exists in `dataset`'s table — the SQL twin of
/// scanning a `Database` collection for the id. Used where a mutation validates its targets against the
/// truth source before touching anything (`Store::hard_erase`). An unknown dataset name is a code defect,
/// not a missing row, so it errors rather than answering `false`.
pub fn record_exists(conn: &Connection, dataset: &str, id: i64) -> Result<bool> {
    // Raw by necessity: the table is chosen at **runtime** from the dataset name, so there is no
    // `col::` constant to name it with — the registry lookup below is what makes it a known table at all.
    let ds = super::schema::dataset(dataset).ok_or(StoreEngineError::UnknownDataset(dataset.to_string()))?;
    conn.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {} WHERE id = ?1)", ds.table),
        [id],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n != 0)
    .map_err(StoreEngineError::from)
}

/// One comment row for a task's `comment list`, read from the persistent `task_comment` read-model.
/// `created_at` is the stored `to_rfc3339_z` form; the query layer parses it back to a `Timestamp`.
pub struct CommentRow {
    pub id: i64,
    /// Author facet (`human` / `ai`; None when there is none) — the display name comes from `config`
    /// keyed by this facet at the presentation layer.
    pub author_kind: Option<String>,
    pub text: String,
    /// `created_at` as stored (`to_rfc3339_z` form, fixed width — a faithful `ORDER BY` key).
    pub created_at: String,
    /// When the body was rewritten in place, if it ever was — the "edited" mark. `None` on a comment
    /// nobody has touched since posting.
    pub edited_at: Option<String>,
}

/// Live comments for one task, **oldest first** (`created_at ASC, id ASC` — ids are handed out in
/// creation order, so the id tiebreak agrees with `created_at`). Served from the read-model
/// `task_comment` table.
pub fn comment_list(conn: &Connection, task_id: i64) -> Result<Vec<CommentRow>> {
    const C: col::task_comment::Cols = col::task_comment::of("c");
    let mut sel = Select::new();
    let (id, author_kind, text, created_at, edited_at) =
        (sel.col(C.id), sel.col(C.author_kind), sel.col(C.text), sel.col(C.created_at), sel.col(C.edited_at));
    let mut sql = Sql::from(&sel, C.table);
    sql.push_where(Some(&Pred::eq(C.task_id, task_id)))
        .order_by([Sort::by(C.created_at), Sort::by(C.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(CommentRow {
                id: id.get(r)?,
                author_kind: author_kind.get(r)?,
                text: text.get(r)?,
                created_at: created_at.get(r)?,
                edited_at: edited_at.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<CommentRow>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// Resolve a decision's current title from the read-model, or `None` when no live decision carries
/// that id. The decision analogue of [`task_title`] — lets a caller label a decision-keyed row (e.g.
/// `decision_comment_list`) without touching SQL.
pub fn decision_title(conn: &Connection, id: i64) -> Result<Option<String>> {
    const D: col::decision::Cols = col::decision::ALL;
    scalar_by_id(conn, D.id, D.title, id)
}

/// The project this decision belongs to, or `None` when it is gone. The decision analogue of
/// [`task_project_id`], and read for the same reason: the ledger's line carries the project itself,
/// because the file cannot join against the DB. The `None` is the decision's absence, not an unfiled
/// decision: `decision.project_id` is `NOT NULL` in the registry, so there is no inbox here the way there
/// is for a task.
pub fn decision_project_id(conn: &Connection, id: i64) -> Result<Option<i64>> {
    const D: col::decision::Cols = col::decision::ALL;
    scalar_by_id(conn, D.id, D.project_id, id)
}

/// Live comments for one decision, **oldest first** (`created_at ASC, id ASC`), served from the
/// read-model `decision_comment` table (mirrors [`comment_list`] for tasks). Seeks the decision's own
/// comments via the `decision_comment_by_decision` index.
pub fn decision_comment_list(conn: &Connection, decision_id: i64) -> Result<Vec<CommentRow>> {
    const C: col::decision_comment::Cols = col::decision_comment::of("c");
    let mut sel = Select::new();
    let (id, author_kind, text, created_at, edited_at) =
        (sel.col(C.id), sel.col(C.author_kind), sel.col(C.text), sel.col(C.created_at), sel.col(C.edited_at));
    let mut sql = Sql::from(&sel, C.table);
    sql.push_where(Some(&Pred::eq(C.decision_id, decision_id)))
        .order_by([Sort::by(C.created_at), Sort::by(C.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(CommentRow {
                id: id.get(r)?,
                author_kind: author_kind.get(r)?,
                text: text.get(r)?,
                created_at: created_at.get(r)?,
                edited_at: edited_at.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<CommentRow>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// The decisions every one of `terms` lands on — the whole of `decision list`'s word narrowing, read
/// **once** and folded into the in-memory match as a set membership, so the listing costs one extra
/// query rather than a per-decision scan. A term may land on any of the decision's faces: its title, its
/// body, the body of one of its live comments, or the name of something attached to it or to one of
/// those comments — the same reach the task side has, bar the labels, which only a task carries.
///
/// No terms is no constraint — an all-whitespace text asks nothing, so every decision comes back rather
/// than none. Ids come back in id order.
pub fn decisions_matching_text(conn: &Connection, terms: &[String]) -> Result<Vec<i64>> {
    let mut sel = Select::new();
    let id = sel.col(DEC.id);
    let mut sql = Sql::from(&sel, DEC.table);
    sql.push_where(Pred::all(terms.iter().map(|t| decision_text_term(search::Term::Inline(t)))).as_ref())
        .order_by([Sort::by(DEC.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| id.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(StoreEngineError::from)?;
    Ok(ids)
}

/// One hit of a word search as SQL hands it over: the face it landed on, whose face it is, when it is
/// dated, and the text it landed in — from which the query layer cuts the snippet
/// ([`search::snippet`]). One row is one place a word is written, so a record with the word in its title
/// and in two of its comments is three rows.
pub struct SearchHitRow {
    pub face: HitFace,
    /// Which side the owner is: [`search::DATASET_TASK`] or [`search::DATASET_DECISION`]. Every hit
    /// belongs to one of the two, including the faces that are not held on the record itself — a label is
    /// the task's by the placement, an attachment by what it hangs off.
    pub owner_kind: String,
    pub owner_id: i64,
    pub owner_title: String,
    /// The comment the words are in, or the one the attachment hangs off — the timeline entry the reader
    /// needs to open to find them. `None` for a hit on the record's own faces.
    pub comment_id: Option<i64>,
    /// The hit's **own** instant (`AMB-D-449`): a comment is dated by when it was posted, anything else by
    /// when the text it sits in was last written.
    pub at: String,
    /// The face's text **as it was written** — the fold is the index's copy, never what comes back here.
    pub text: String,
}

/// One page of hits: the total before paging, and the page itself.
pub struct SearchPage {
    pub total_matched: usize,
    pub hits: Vec<SearchHitRow>,
}

/// The row shape every arm of the hit query projects, held to by the compiler ([`Union`]).
type HitSlots =
    (Slot<i64>, Slot<String>, Slot<i64>, Slot<String>, Slot<Option<i64>>, Slot<String>, Slot<String>);

/// Project one arm's row — the seam that keeps twelve arms saying the same thing in the same order.
///
/// The face travels as its rank ([`HitFace::tier`]) rather than as a name, because the rank is what the
/// compound query orders by and the mapping back is total. `text` arrives as an **expression** rather
/// than a column: one face is a `COALESCE` of two (an attachment carries a filename or a url, never
/// both), and the rest hand in their own registry column through [`Expr::to_sql`].
#[allow(clippy::too_many_arguments)]
fn hit_slots(
    sel: &mut Select,
    face: HitFace,
    owner_kind: &str,
    owner_id: Col<Int>,
    owner_title: Col<SqlText>,
    comment_id: Option<Col<Int>>,
    at: Col<SqlText>,
    text: String,
) -> HitSlots {
    let tier = sel.expr::<i64>(face.tier().to_string());
    // The kind is grammar here, not data: the two spellings are this module's own constants.
    let kind = sel.expr::<String>(format!("'{owner_kind}'"));
    let id = sel.col(owner_id);
    let title = sel.col(owner_title);
    let comment = match comment_id {
        Some(c) => sel.col(c.nullable()),
        None => sel.expr::<Option<i64>>("NULL"),
    };
    let at = sel.col(at);
    let text = sel.expr::<String>(text);
    (tier, kind, id, title, comment, at, text)
}

/// Does this record's own copy of `columns` carry any of the terms — the question that makes one arm's
/// row a hit. Any term, not every one: what has to carry them all is the **record**, and that is the
/// caller's other predicate ([`task_text_term`] / [`decision_text_term`]).
///
/// Membership of a set, not an `EXISTS` correlated to the arm's row, for the reason the record-level
/// predicate is one too (`AMB-D-511`): the term lookup inside a correlated subquery is a lookup the
/// statement can be made to repeat once per candidate row.
fn face_hit(dataset: &str, id: Col<Int>, columns: &[&str], terms: &[search::Term<'_>]) -> Pred {
    let any_column = Pred::any(columns.iter().map(|c| Pred::eq(SD.field, *c)));
    let any_term = Pred::any(terms.iter().map(|t| t.pred(SD)));
    let mut carriers = IdSet::of(SD.table, SD.owner_id)
        .filter(Pred::eq(SD.owner_kind, dataset))
        .filter(any_term.unwrap_or_else(Pred::never));
    if let Some(p) = any_column {
        carriers = carriers.filter(p);
    }
    Pred::is_in_any(id, [carriers])
}

/// One arm's `WHERE`: this face carries a term, the record it belongs to answers everything asked of
/// that side (the terms, the reach, the structural filter), and the arm is one the two axes left standing.
fn hit_where(face: Pred, side: &Option<Pred>, gate: &Option<Pred>) -> Option<Pred> {
    Pred::all([Some(face), side.clone(), gate.clone()].into_iter().flatten())
}

/// Is this arm one the caller's two axes keep (`AMB-D-562`)? `kind` says **which record** the words are
/// on and `face` **which face of it**, and they are judged apart and ANDed — an axis left unnamed keeps
/// everything on it. Because they are a product rather than one mixed narrowing, "the remarks on
/// decisions" is a thing a caller can ask for; a single four-valued `--kind` could not express it.
fn kept_by_axes(
    kind: Option<crate::query::SearchKind>,
    want: Option<HitFace>,
    owner_kind: &str,
    face: HitFace,
) -> bool {
    let by_kind = match kind {
        None => true,
        Some(crate::query::SearchKind::Task) => owner_kind == search::DATASET_TASK,
        Some(crate::query::SearchKind::Decision) => owner_kind == search::DATASET_DECISION,
    };
    by_kind && want.is_none_or(|w| w == face)
}

/// What a word search is asked for. The shape [`TaskQuery`] has, for the same reason: everything the read
/// needs is named in one place, and `reach` among them, so a caller cannot forget to declare its scope.
pub struct SearchQuery<'a> {
    pub reach: crate::reach::Reach,
    /// The words, already folded ([`search::terms`]). No terms is no search — not "no constraint".
    pub terms: &'a [String],
    /// The one project to look in — the same slot [`TaskQuery`] carries, and read the same way: the
    /// reach is folded into it here, so an empty slot under a closed reach is that reach's project and
    /// never everything. It narrows **both** sides, because a project is an axis a task and a decision
    /// carry alike (`AMB-D-564`).
    pub project_id: Option<i64>,
    /// The structural narrowing, already read in the grammar of the side it was asked of
    /// ([`crate::query::SearchNarrowing`], `AMB-D-563`) — which is why it narrows one side and says
    /// nothing about the other. The other side is taken out by `kind`, not by this: the caller had to
    /// name the side to be given a vocabulary at all, so the arms are already gated when this is here.
    pub filter: Option<&'a crate::query::SearchNarrowing>,
    /// Today, for the filter's day-relative keys (`due:today` and friends).
    pub today: NaiveDate,
    /// Which record the words are on, and which face of it — the two axes, kept apart ([`kept_by_axes`]).
    pub kind: Option<crate::query::SearchKind>,
    pub face: Option<HitFace>,
    pub sort: crate::query::SearchSort,
    pub limit: Option<usize>,
    pub offset: usize,
}

/// Every place `terms` are written, as hits (`AMB-D-449`) — the read behind `search`.
///
/// **What a hit is.** One row per (record, face): the task's own title or notes, the decision's title or
/// body, one comment's text on either, a label the task was placed on, or the name of something attached
/// to either — or to one of those comments. The faces are [`search::FACES`]'s to list; this is where each
/// one is said to be a task's or a decision's, which the index itself does not know.
///
/// **Where the AND sits.** Every term has to land on the **record**, on any of its faces, which is the
/// same rule the list filters match by ([`task_text_term`] / [`decision_text_term`]) — so the two mouths
/// can never disagree about whether a word reaches a record (`AMB-D-449`). A face is then shown when it
/// carries *any* of them: a search for two words returns the places each is written, rather than only the
/// places both happen to sit together.
///
/// **What narrows it.** The reach, first and always — folded into `project_id` before it gets here. Then
/// the caller's own: `project_id` again where the caller named one, which narrows both sides alike; a
/// structural `filter`, which lands on the side whose grammar it was written in and is the very predicate
/// that side's own listing narrows by ([`filter_preds`] / [`decision_filter_preds`]); and the two axes —
/// `kind`, which record the words are on, and `face`, which face of it — which between them keep the arms
/// that were asked for ([`kept_by_axes`]).
///
/// **The order arrives with the rows.** The page is cut in SQL, so the sort cannot be something the
/// reader applies to what it was handed. The compound query names its order by position, which is the
/// projection's to say ([`Slot::ordinal`]).
pub fn search_hits(conn: &Connection, q: &SearchQuery) -> Result<SearchPage> {
    // No words is no search — and, unlike a filter, not "no constraint": every record's every face is not
    // an answer to "where is this written".
    let terms = q.terms;
    if terms.is_empty() {
        return Ok(SearchPage { total_matched: 0, hits: Vec::new() });
    }
    // The scope the arms narrow by: what the caller asked for, with the reach folded in here as well as
    // where it was asked. The fold is idempotent, and doing it at the engine is the same containment
    // `list_task_ids` keeps — a caller that leaves the slot empty gets its bound project, never
    // everything.
    let project_id = q.reach.narrow(q.project_id).map_err(StoreEngineError::OutOfReach)?;
    let started = std::time::Instant::now();

    const TC: col::task_comment::Cols = col::task_comment::of("tc");
    const DC: col::decision_comment::Cols = col::decision_comment::of("dc");
    const DIM: col::dimension::Cols = col::dimension::of("dim");
    const DV: col::dimension_value::Cols = col::dimension_value::of("dv");
    const TASK: &str = search::DATASET_TASK;
    const DECISION: &str = search::DATASET_DECISION;

    // The words are looked up **once for the statement**, at its head, and every arm asks membership of
    // that (`AMB-D-511`). Twelve arms put the same words to the same copy, and the statement is built
    // twice over — the count and the page — so a lookup written where it is asked is a walk of the copy
    // per arm, whichever path the term's length takes it down. What each arm means is untouched.
    let head = search::terms_head(terms);
    let asked: Vec<search::Term<'_>> = (0..terms.len()).map(search::Term::Named).collect();
    let asked = asked.as_slice();

    // Everything asked of one side, folded once and cloned into every arm that owns that side: the
    // record-level AND, the reach, and the structural narrowing, which lands on the side whose grammar
    // it was written in and is the very predicate that side's own listing narrows by (`filter_preds` /
    // `decision_filter_preds`), so the two reads cannot come to read one expression differently.
    let task_filter = match q.filter {
        Some(crate::query::SearchNarrowing::Task(f)) => Some(filter_preds(&TaskQuery {
            reach: q.reach,
            project_id,
            filter: f,
            sort: "",
            today: q.today,
            limit: None,
            offset: None,
        })),
        _ => None,
    };
    let task_side = Pred::all(
        [
            Pred::all(asked.iter().map(|t| task_text_term(*t))),
            project_id.map(|pid| Pred::eq(T.project_id, pid)),
            task_filter.and_then(Pred::all),
        ]
        .into_iter()
        .flatten(),
    );
    let decision_filter = match q.filter {
        Some(crate::query::SearchNarrowing::Decision(f)) => Some(decision_filter_preds(f)),
        _ => None,
    };
    let decision_side = Pred::all(
        [
            Pred::all(asked.iter().map(|t| decision_text_term(*t))),
            project_id.map(|pid| Pred::eq(DEC.project_id, pid)),
            decision_filter.and_then(Pred::all),
        ]
        .into_iter()
        .flatten(),
    );
    let gate = |face: HitFace, owner_kind: &str| {
        (!kept_by_axes(q.kind, q.face, owner_kind, face)).then(Pred::never)
    };
    let (slots, mut sql) = Union::all(|sel| {
        // A task's title.
        let slots =
            hit_slots(sel, HitFace::Title, TASK, T.id, T.title, None, T.updated_at, T.title.to_sql());
        let mut tail = Sql::from_table(T.table);
        tail.push_where(
            hit_where(face_hit(TASK, T.id, &["title"], asked), &task_side, &gate(HitFace::Title, TASK),).as_ref(),
        );
        (slots, tail)
    })
    .arm(|sel| {
        // A task's notes.
        let slots =
            hit_slots(sel, HitFace::Body, TASK, T.id, T.title, None, T.updated_at, T.notes.to_sql());
        let mut tail = Sql::from_table(T.table);
        tail.push_where(
            hit_where(face_hit(TASK, T.id, &["notes"], asked), &task_side, &gate(HitFace::Body, TASK),).as_ref(),
        );
        (slots, tail)
    })
    .arm(|sel| {
        // A decision's title.
        let slots = hit_slots(
            sel,
            HitFace::Title,
            DECISION,
            DEC.id,
            DEC.title,
            None,
            DEC.updated_at,
            DEC.title.to_sql(),
        );
        let mut tail = Sql::from_table(DEC.table);
        tail.push_where(
            hit_where(face_hit(DECISION, DEC.id, &["title"], asked), &decision_side, &gate(HitFace::Title, DECISION),)
                .as_ref(),
        );
        (slots, tail)
    })
    .arm(|sel| {
        // A decision's body.
        let slots = hit_slots(
            sel,
            HitFace::Body,
            DECISION,
            DEC.id,
            DEC.title,
            None,
            DEC.updated_at,
            DEC.body.to_sql(),
        );
        let mut tail = Sql::from_table(DEC.table);
        tail.push_where(
            hit_where(face_hit(DECISION, DEC.id, &["body"], asked), &decision_side, &gate(HitFace::Body, DECISION),)
                .as_ref(),
        );
        (slots, tail)
    })
    .arm(|sel| {
        // A comment on a task — dated by when it was posted.
        let slots = hit_slots(
            sel,
            HitFace::Comment,
            TASK,
            T.id,
            T.title,
            Some(TC.id),
            TC.created_at,
            TC.text.to_sql(),
        );
        let mut tail = Sql::from_table(TC.table);
        tail.join(T.table, same(T.id, TC.task_id)).push_where(
            hit_where(face_hit(search::DATASET_TASK_COMMENT, TC.id, &["text"], asked), &task_side, &gate(HitFace::Comment, TASK),)
                .as_ref(),
        );
        (slots, tail)
    })
    .arm(|sel| {
        // A comment on a decision.
        let slots = hit_slots(
            sel,
            HitFace::Comment,
            DECISION,
            DEC.id,
            DEC.title,
            Some(DC.id),
            DC.created_at,
            DC.text.to_sql(),
        );
        let mut tail = Sql::from_table(DC.table);
        tail.join(DEC.table, same(DEC.id, DC.decision_id)).push_where(
            hit_where(
                face_hit(search::DATASET_DECISION_COMMENT, DC.id, &["text"], asked),
                &decision_side,
                &gate(HitFace::Comment, DECISION),
            )
            .as_ref(),
        );
        (slots, tail)
    })
    .arm(|sel| {
        // A label the task was placed on — the value's name.
        let slots =
            hit_slots(sel, HitFace::Label, TASK, T.id, T.title, None, DV.updated_at, DV.name.to_sql());
        let mut tail = Sql::from_table(TDV.table);
        tail.join(T.table, same(T.id, TDV.task_id))
            .join(DV.table, same(DV.id, TDV.value_id))
            .push_where(
                hit_where(
                    face_hit(search::DATASET_DIMENSION_VALUE, DV.id, &["name"], asked),
                    &task_side,
                    &gate(HitFace::Label, TASK),
                )
                .as_ref(),
            );
        (slots, tail)
    })
    .arm(|sel| {
        // The axis that label is a value of, reached through the same placement.
        let slots =
            hit_slots(sel, HitFace::Label, TASK, T.id, T.title, None, DIM.updated_at, DIM.name.to_sql());
        let mut tail = Sql::from_table(TDV.table);
        tail.join(T.table, same(T.id, TDV.task_id))
            .join(DIM.table, same(DIM.id, TDV.dimension_id))
            .push_where(
                hit_where(
                    face_hit(search::DATASET_DIMENSION, DIM.id, &["name"], asked),
                    &task_side,
                    &gate(HitFace::Label, TASK),
                )
                .as_ref(),
            );
        (slots, tail)
    })
    .arm(|sel| {
        // Something attached to a task.
        let slots = hit_slots(
            sel,
            HitFace::Attachment,
            TASK,
            T.id,
            T.title,
            None,
            A.updated_at,
            attachment_name(A),
        );
        let mut tail = Sql::from_table(A.table);
        tail.join(T.table, hangs_off(TASK, T.id)).push_where(
            hit_where(
                face_hit(search::DATASET_ATTACHMENT, A.id, &["filename", "url"], asked),
                &task_side,
                &gate(HitFace::Attachment, TASK),
            )
            .as_ref(),
        );
        (slots, tail)
    })
    .arm(|sel| {
        // Something attached to a decision.
        let slots = hit_slots(
            sel,
            HitFace::Attachment,
            DECISION,
            DEC.id,
            DEC.title,
            None,
            A.updated_at,
            attachment_name(A),
        );
        let mut tail = Sql::from_table(A.table);
        tail.join(DEC.table, hangs_off(DECISION, DEC.id)).push_where(
            hit_where(
                face_hit(search::DATASET_ATTACHMENT, A.id, &["filename", "url"], asked),
                &decision_side,
                &gate(HitFace::Attachment, DECISION),
            )
            .as_ref(),
        );
        (slots, tail)
    })
    .arm(|sel| {
        // Something pinned to a comment on a task: the task's, by the same reading that puts the
        // comment's own body here.
        let slots = hit_slots(
            sel,
            HitFace::Attachment,
            TASK,
            T.id,
            T.title,
            Some(TC.id),
            A.updated_at,
            attachment_name(A),
        );
        let mut tail = Sql::from_table(A.table);
        tail.join(TC.table, hangs_off(search::DATASET_TASK_COMMENT, TC.id))
            .join(T.table, same(T.id, TC.task_id))
            .push_where(
                hit_where(
                    face_hit(search::DATASET_ATTACHMENT, A.id, &["filename", "url"], asked),
                    &task_side,
                    &gate(HitFace::Attachment, TASK),
                )
                .as_ref(),
            );
        (slots, tail)
    })
    .arm(|sel| {
        // Something pinned to a comment on a decision.
        let slots = hit_slots(
            sel,
            HitFace::Attachment,
            DECISION,
            DEC.id,
            DEC.title,
            Some(DC.id),
            A.updated_at,
            attachment_name(A),
        );
        let mut tail = Sql::from_table(A.table);
        tail.join(DC.table, hangs_off(search::DATASET_DECISION_COMMENT, DC.id))
            .join(DEC.table, same(DEC.id, DC.decision_id))
            .push_where(
                hit_where(
                    face_hit(search::DATASET_ATTACHMENT, A.id, &["filename", "url"], asked),
                    &decision_side,
                    &gate(HitFace::Attachment, DECISION),
                )
                .as_ref(),
            );
        (slots, tail)
    })
    .into_parts();

    // The total is counted over the arms as they stand, before the page is cut from them. The head goes
    // on each statement rather than inside the arms: the lookups are the whole statement's, and the count
    // reads the arms through a derived table, which the head reaches into just the same.
    let mut counted = Select::new();
    let matched = counted.count_all();
    let mut count = Sql::from_sub(&counted, &sql, "hit");
    if let Some(head) = &head {
        count.with_head(head);
    }
    let total = conn
        .query_row(count.text(), rusqlite::params_from_iter(count.params()), |r| matched.get(r))
        .map_err(StoreEngineError::from)? as usize;

    let (tier, kind, owner_id, owner_title, comment_id, at, text) = slots;
    let (tier_n, at_n, id_n) = (tier.ordinal(), at.ordinal(), owner_id.ordinal());
    // Newest first within whatever leads: the current context is on the new side of a store that only
    // accumulates. The id breaks the remaining ties so a page boundary never wobbles between two reads.
    let order = match q.sort {
        crate::query::SearchSort::Face => format!(" ORDER BY {tier_n}, {at_n} DESC, {id_n} DESC"),
        crate::query::SearchSort::Newest => format!(" ORDER BY {at_n} DESC, {tier_n}, {id_n} DESC"),
        crate::query::SearchSort::Oldest => format!(" ORDER BY {at_n}, {tier_n}, {id_n}"),
    };
    sql.push(order).limit(q.limit.map(|n| n as i64).unwrap_or(-1)).offset(q.offset as i64);
    if let Some(head) = &head {
        sql.with_head(head);
    }

    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let hits = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(SearchHitRow {
                // The rank was written by this query, so there is no other face it could name.
                face: HitFace::from_tier(tier.get(r)?).expect("the tier this query projects"),
                owner_kind: kind.get(r)?,
                owner_id: owner_id.get(r)?,
                owner_title: owner_title.get(r)?,
                comment_id: comment_id.get(r)?,
                at: at.get(r)?,
                text: text.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<SearchHitRow>>>()
        .map_err(StoreEngineError::from)?;

    crate::perf::record_query("engine.search_hits", total, hits.len(), started.elapsed());
    Ok(SearchPage { total_matched: total, hits })
}

/// Where the records a page of hits points at stand: a task's status, priority and classification, and a
/// decision's status.
///
/// Keyed by record id, not by hit — a record the words reach on three faces is three hits and one entry,
/// which is also why this is not a column of [`search_hits`]: folding it in would read the same task once
/// per face, and the classification is one-to-many on top of that.
pub struct HitStandingRows {
    pub tasks: HashMap<i64, (TaskStatus, Option<Priority>)>,
    /// `(axis, value)` names per task, in axis order — a task sits on as many axes as it was placed on,
    /// and on none at all just as often, so an id absent here is a task with no classification.
    pub labels: HashMap<i64, Vec<(String, String)>>,
    pub decisions: HashMap<i64, DecisionStatus>,
}

/// Read that standing for the records a page names, **after** the page has been cut (`AMB-D-567`).
///
/// Three statements, each over an id set the caller already holds, so what is read is bounded by the page
/// and not by the store: a page of twenty reads twenty tasks' rows however many hits the words have in
/// all. That is the whole reason this is a second read rather than more columns on the first — the hit
/// query's own rows are the places a word is written, and there are more of those than there are records.
///
/// The reach rides on the id sets as a predicate, as it does in [`hydrate_task_cards`]: the ids arrive
/// from a scoped query, and this is still the floor under them. An id that hydrates nothing — gone
/// between the two reads, or outside the reach — is simply absent, which is how a face reads "nothing to
/// say about this one" rather than an invented default.
pub fn hit_standings(
    conn: &Connection,
    reach: crate::reach::Reach,
    task_ids: &[i64],
    decision_ids: &[i64],
) -> Result<HitStandingRows> {
    let started = std::time::Instant::now();
    let mut out =
        HitStandingRows { tasks: HashMap::new(), labels: HashMap::new(), decisions: HashMap::new() };

    if !task_ids.is_empty() {
        const TA: col::task::Cols = col::task::ALL;
        let mut sel = Select::new();
        let (id, status, priority) = (sel.col(TA.id), sel.col(TA.status), sel.col(TA.priority));
        let mut pred = Pred::is_in(TA.id, task_ids.iter().copied());
        if let Some(pid) = reach.project() {
            pred = pred.and(Pred::eq(TA.project_id, pid));
        }
        let mut sql = Sql::from(&sel, TA.table);
        sql.push_where(Some(&pred));
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((
                    id.get(r)?,
                    card_enum_req(TA.status, status.get(r)?, TaskStatus::parse)?,
                    card_enum_opt(TA.priority, priority.get(r)?, Priority::parse)?,
                ))
            })
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let (id, status, priority) = row.map_err(StoreEngineError::from)?;
            out.tasks.insert(id, (status, priority));
        }
    }

    // Only the tasks that answered above are asked for their labels: an id the reach turned away is one
    // this join must not reach either, and keying off the hydrated set says so once instead of repeating
    // the scope in a second predicate.
    let placed: Vec<i64> = task_ids.iter().copied().filter(|id| out.tasks.contains_key(id)).collect();
    if !placed.is_empty() {
        const TV: col::task_dimension_value::Cols = col::task_dimension_value::of("tv");
        const D: col::dimension::Cols = col::dimension::of("d");
        const V: col::dimension_value::Cols = col::dimension_value::of("v");
        let mut sel = Select::new();
        let (task, axis, value) = (sel.col(TV.task_id), sel.col(D.name), sel.col(V.name));
        let mut sql = Sql::from(&sel, TV.table);
        // Both joins are inner, as in `task_classification`: an assignment whose axis or value is gone
        // names nothing to show. The order is the axis's, so two tasks read down the same columns.
        sql.join(D.table, same(D.id, TV.dimension_id))
            .join(V.table, same(V.id, TV.value_id))
            .push_where(Some(&Pred::is_in(TV.task_id, placed.iter().copied())))
            .order_by([Sort::by(D.order_key), Sort::by(D.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((task.get(r)?, axis.get(r)?, value.get(r)?))
            })
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let (task, axis, value): (i64, String, String) = row.map_err(StoreEngineError::from)?;
            out.labels.entry(task).or_default().push((axis, value));
        }
    }

    if !decision_ids.is_empty() {
        const DE: col::decision::Cols = col::decision::ALL;
        let mut sel = Select::new();
        let (id, status) = (sel.col(DE.id), sel.col(DE.status));
        let mut pred = Pred::is_in(DE.id, decision_ids.iter().copied());
        if let Some(pid) = reach.project() {
            pred = pred.and(Pred::eq(DE.project_id, pid));
        }
        let mut sql = Sql::from(&sel, DE.table);
        sql.push_where(Some(&pred));
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((id.get(r)?, card_enum_req(DE.status, status.get(r)?, DecisionStatus::parse)?))
            })
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let (id, status) = row.map_err(StoreEngineError::from)?;
            out.decisions.insert(id, status);
        }
    }

    let read = out.tasks.len() + out.decisions.len();
    crate::perf::record_query("engine.hit_standings", read, read, started.elapsed());
    Ok(out)
}

/// The line a record is named by: its title, when it was last written, and where it sits. What a search
/// pins to the top when one of the words was a ref rather than a word ([`crate::query::search`]) — the
/// record itself, which is not something the word index can answer for, since nothing about a record
/// carries the ref it is called by.
pub struct Headline {
    pub title: String,
    pub at: String,
    /// The project the record is in, for the reach to judge. A task may be unplaced; a decision never is.
    pub project_id: Option<i64>,
}

/// The [`Headline`] of one live record — a task or a decision, by the dataset name the search speaks in
/// ([`search::DATASET_TASK`] / [`search::DATASET_DECISION`]). `None` when no live row carries the id, which
/// is how a ref for a record that is gone reads: a word that pins nothing.
pub fn record_headline(conn: &Connection, dataset: &str, id: i64) -> Result<Option<Headline>> {
    let is_task = dataset == search::DATASET_TASK;
    let mut sel = Select::new();
    let sql = if is_task {
        let (title, at, project) = (sel.col(T.title), sel.col(T.updated_at), sel.col(T.project_id));
        let mut sql = Sql::from(&sel, T.table);
        sql.push_where(Some(&Pred::eq(T.id, id)));
        (sql, title, at, project)
    } else {
        let (title, at) = (sel.col(DEC.title), sel.col(DEC.updated_at));
        // A decision's project is `NOT NULL` in the registry; read as optional so both sides of this
        // branch hand back the same row shape.
        let project = sel.col(DEC.project_id.nullable());
        let mut sql = Sql::from(&sel, DEC.table);
        sql.push_where(Some(&Pred::eq(DEC.id, id)));
        (sql, title, at, project)
    };
    let (sql, title, at, project) = sql;
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    stmt.query_row(rusqlite::params_from_iter(sql.params()), |r| {
        Ok(Headline { title: title.get(r)?, at: at.get(r)?, project_id: project.get(r)? })
    })
    .optional()
    .map_err(StoreEngineError::from)
}

/// What an attachment is called, as one expression: the filename it came in under, or the address the
/// link points at. Exactly one of the two is present on any row, so the `COALESCE` is a choice between a
/// value and an absence rather than between two values; the empty string is the third arm only so the
/// column can be read as text whatever the row holds.
fn attachment_name(a: col::attachment::Cols) -> String {
    format!("COALESCE({}, {}, '')", a.filename.to_sql(), a.url.to_sql())
}

/// One decision linked to a task (the reverse of decision→tasks): its conversational number, title and
/// status. Lets `task show` surface the "why" records inline so an agent reads them alongside notes and
/// comments instead of running a separate `decision list` pass.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LinkedDecisionRow {
    /// The decision id — which is the conversational number `D-<id>` itself.
    pub id: i64,
    pub title: String,
    /// Lifecycle status as stored (`proposed` / `accepted` / `rejected` / `superseded`).
    pub status: String,
}

/// Live decisions linked to a task, ordered by decision id (the reverse of the decision→tasks links
/// `decision_list` counts). Served from the `decision_task_link` read-model so `task show` can present
/// the linked "why" records without a separate pass.
pub fn decisions_for_task(conn: &Connection, task_id: i64) -> Result<Vec<LinkedDecisionRow>> {
    const D: col::decision::Cols = col::decision::of("d");
    const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
    let mut sel = Select::new();
    let (id, title, status) = (sel.col(D.id), sel.col(D.title), sel.col(D.status));
    let mut sql = Sql::from(&sel, D.table);
    sql.join(L.table, same(L.decision_id, D.id))
        .push_where(Some(&Pred::eq(L.task_id, task_id)))
        .order_by([Sort::by(D.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(LinkedDecisionRow { id: id.get(r)?, title: title.get(r)?, status: status.get(r)? })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<LinkedDecisionRow>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// One mailbox-D task with the **incoming** comments that put it in the inbox: the AI facet's comments
/// on a task the human facet is carrying. The device-local `read_receipts` then derives each task's
/// `unread` flag from these.
pub struct MailboxTask {
    pub task_id: i64,
    /// Incoming comments, oldest first: `(author_facet, author_is_human, created_at)`.
    pub comments: Vec<(String, bool, String)>,
}

/// Tasks for the mailbox's D bucket — live, assigned to my **human** facet, still outstanding, and carrying at
/// least one incoming comment — each bundled with its incoming comments so the caller derives `unread`
/// from `read_receipts` without a second pass. One indexed pass joins those tasks
/// to their **incoming** comments; rows arrive grouped by task (`task_id`) then
/// oldest-first (`created_at ASC, id ASC`), so a task with no incoming comment yields no rows and is
/// correctly absent. "Incoming" = the comment's author is the AI facet (`author_kind = 'ai'`).
/// A closed `reach` bundles only the bound project's tasks.
pub fn mailbox_comment_tasks(conn: &Connection, reach: crate::reach::Reach) -> Result<Vec<MailboxTask>> {
    let started = std::time::Instant::now();
    // The bucket is addressed, not just authored: it holds AI comments (`author_kind = 'ai'`) on tasks
    // whose assignee is the human facet. A task the AI itself carries is one the AI reports on, and a
    // report is read by pulling the task's timeline — it is not an inbox item, so it never rings here.
    // Handing the task back (assignee → human) or blocking it is how the AI asks for a human's move, and
    // that is bucket C.
    const C: col::task_comment::Cols = col::task_comment::of("c");
    let mut sel = Select::new();
    let (task_id, author_kind, created_at) = (sel.col(C.task_id), sel.col(C.author_kind), sel.col(C.created_at));
    // The reach narrows the same predicate the mailbox is made of, rather than being appended as a
    // fragment whose `?N` had to be counted against the others.
    let scope = reach.project().map(|pid| Pred::eq(T.project_id, pid));
    let pred = Pred::eq(T.assignee_kind, ActorKind::Human.as_str())
        .and(still_open(T.status))
        .and(Pred::eq(C.author_kind, ActorKind::Ai.as_str()));
    let pred = Pred::all(scope.into_iter().chain([pred])).expect("at least one predicate");
    let mut sql = Sql::from(&sel, C.table);
    sql.join(T.table, same(T.id, C.task_id))
        .push_where(Some(&pred))
        .order_by([Sort::by(C.task_id), Sort::by(C.created_at), Sort::by(C.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            let task_id = task_id.get(r)?;
            let author_kind = author_kind.get(r)?;
            let created_at = created_at.get(r)?;
            let facet = author_kind.clone().unwrap_or_default();
            let is_human = author_kind.as_deref() != Some("ai");
            Ok((task_id, facet, is_human, created_at))
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(i64, String, bool, String)>>>()
        .map_err(StoreEngineError::from)?;

    // Fold the (already task-ordered) rows into one MailboxTask per task id.
    let mut out: Vec<MailboxTask> = Vec::new();
    for (task_id, facet, is_human, created_at) in rows {
        match out.last_mut() {
            Some(last) if last.task_id == task_id => {
                last.comments.push((facet, is_human, created_at));
            }
            _ => out.push(MailboxTask {
                task_id,
                comments: vec![(facet, is_human, created_at)],
            }),
        }
    }
    crate::perf::record_query("engine.mailbox_comment_tasks", out.len(), out.len(), started.elapsed());
    Ok(out)
}

/// One page of a project's decision list: the total live count (before paging) and the page of ids
/// in newest-created order. The GUI `decision_page` command hydrates the `DecisionDto`s from these ids.
pub struct DecisionPage {
    pub total_matched: usize,
    pub ids: Vec<i64>,
}

/// List a project's live decision ids, newest-created first, with paging + total count — served from
/// the persistent read-model `decision` table. Scoped to one project. Order is
/// `created_at DESC, id DESC` (ids are handed out in creation order, so the id tiebreak agrees with
/// `created_at` — the `-created` sort). The GUI re-applies its own status/search/sort on the
/// bounded page, so this only fixes the count and the paged window. `limit` None = all rows from `offset`.
/// The page is asked for by project, so a closed `reach` cannot narrow it — it refuses a project outside
/// itself instead (`out_of_reach`, not an empty page).
pub fn decision_page(
    conn: &Connection,
    reach: crate::reach::Reach,
    project_id: i64,
    limit: Option<usize>,
    offset: usize,
) -> Result<DecisionPage> {
    reach
        .check(&crate::idref::project(project_id), Some(project_id))
        .map_err(StoreEngineError::OutOfReach)?;
    let started = std::time::Instant::now();
    const D: col::decision::Cols = col::decision::ALL;
    // One predicate for both statements: the count and the page cannot come to ask different questions.
    let pred = Pred::eq(D.project_id, project_id);

    let mut counted = Select::new();
    let matched = counted.count_all();
    let mut count = Sql::from(&counted, D.table);
    count.push_where(Some(&pred));
    let total: usize = conn
        .query_row(count.text(), rusqlite::params_from_iter(count.params()), |r| matched.get(r))
        .map_err(StoreEngineError::from)? as usize;

    // `LIMIT -1` = no limit; `OFFSET` skips.
    let mut sel = Select::new();
    let id = sel.col(D.id);
    let mut page = Sql::from(&sel, D.table);
    page.push_where(Some(&pred))
        .order_by([Sort::by(D.created_at).desc(), Sort::by(D.id).desc()])
        .limit(limit.map(|n| n as i64).unwrap_or(-1))
        .offset(offset as i64);
    let mut stmt = conn.prepare(page.text()).map_err(StoreEngineError::from)?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(page.params()), |r| id.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(StoreEngineError::from)?;

    // `limit 0` is a count-only read (returned = 0 is the design), so the complexity ratio is dropped and
    // only the time budget is measured — the same reason as in `list_task_ids`.
    if limit == Some(0) {
        crate::perf::record_count_query("engine.decision_page", total, started.elapsed());
    } else {
        crate::perf::record_query("engine.decision_page", total, ids.len(), started.elapsed());
    }
    Ok(DecisionPage { total_matched: total, ids })
}

/// One live decision for the unbounded `decision list` read, with the project name resolved (LEFT
/// JOIN, live project only) and the linked-task count pre-computed. The query layer rebuilds a
/// partial `Decision` from these fields to reuse the query layer's own filter and sort, then maps to
/// a `DecisionCompact` via [`crate::view::decision_compact_with`]. `number`/`decided_at` are nullable;
/// `decided_at`/`created_at` are the stored `to_rfc3339_z` form, parsed back to a `Timestamp` by the
/// caller. `body` is carried only because the word narrowing reads it (title OR body).
pub struct DecisionRow {
    pub id: i64,
    pub project_id: i64,
    /// Live project name, or `None` when there is no such project (the card renders no
    /// `project` ref in that case).
    pub project_name: Option<String>,
    pub title: String,
    pub body: String,
    /// `DecisionStatus` as snake_case wire text (proposed/accepted/rejected).
    pub status: String,
    /// The live decisions that superseded this one, in the order the edges were drawn — the reverse view
    /// `decision show` carries, brought down onto the list row. Whether it was replaced is read off this
    /// alone (`AMB-D-410`), so there is no second field to come to disagree with it.
    pub superseded_by: Vec<i64>,
    pub decided_at: Option<String>,
    pub created_at: String,
    /// Links to tasks (live links to live tasks).
    pub linked_task_count: usize,
}

/// Every live supersession, keyed by the decision it overturned — `target id → successor ids`, in the
/// order the edges were drawn. Read once for a whole listing, which is what keeps [`decision_list`] free
/// of a seek per row; the same reverse view [`decision_reverse_edges`] serves one decision at a time.
/// `project_id` narrows it to the rows the listing itself carries, so the map never outgrows them, and
/// liveness is the source join exactly as in [`superseded`] — a dangling edge is no supersession here
/// either, because its target is not a row.
fn supersessions(conn: &Connection, project_id: Option<i64>) -> Result<HashMap<i64, Vec<i64>>> {
    const E: col::decision_edge::Cols = col::decision_edge::of("e");
    const S: col::decision::Cols = col::decision::of("s");
    const TD: col::decision::Cols = col::decision::of("t");
    let mut sel = Select::new();
    let (target, source) = (sel.col(E.target_decision_id), sel.col(S.id));
    let mut wheres = vec![word(E.kind, crate::model::DecisionEdgeKind::Supersedes.as_str())];
    if let Some(pid) = project_id {
        wheres.push(Pred::eq(TD.project_id, pid));
    }
    let pred = Pred::all(wheres);
    let mut sql = Sql::from(&sel, E.table);
    sql.join(S.table, same(S.id, E.decision_id))
        .join(TD.table, same(TD.id, E.target_decision_id))
        .push_where(pred.as_ref())
        .order_by([Sort::by(E.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok((target.get(r)?, source.get(r)?))
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(i64, i64)>>>()
        .map_err(StoreEngineError::from)?;
    let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
    for (target, source) in rows {
        out.entry(target).or_default().push(source);
    }
    Ok(out)
}

/// The decisions (optionally scoped to one project) for `decision list`, served from the read-model
/// `decision` table — project name and linked-task count fold in by SQL rather than costing a pass
/// per decision. Rows are returned unordered: the query layer applies the status/text/project filter
/// and the sort over the bounded set. A closed `reach` fills the scope when the caller named none, and
/// refuses another project.
pub fn decision_list(
    conn: &Connection,
    reach: crate::reach::Reach,
    project_id: Option<i64>,
) -> Result<Vec<DecisionRow>> {
    let project_id = reach.narrow(project_id).map_err(StoreEngineError::OutOfReach)?;
    const D: col::decision::Cols = col::decision::of("d");
    const P: col::project::Cols = col::project::of("p");
    const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
    let mut sel = Select::new();
    let (id, pid, title, body) = (sel.col(D.id), sel.col(D.project_id), sel.col(D.title), sel.col(D.body));
    // `p.name` is `NOT NULL` in the registry, but this is a `LEFT JOIN`: a decision whose project is
    // gone yields no project row at all, and the column comes back `NULL`. The outer join is what makes
    // it optional, not the column — so it is the registry's column, widened (`Col::nullable`).
    let project_name = sel.col(P.name.nullable());
    let status = sel.col(D.status);
    let (decided_at, created_at) = (sel.col(D.decided_at), sel.col(D.created_at));
    let linked_task_count = sel.count_of(
        Count::over(L.table)
            .join(T.table, same(T.id, L.task_id))
            .filter(same(L.decision_id, D.id)),
    );
    // Who overturned each row, read in one pass beside the rows themselves.
    let successors = supersessions(conn, project_id)?;
    let scope = project_id.map(|pid| Pred::eq(D.project_id, pid));
    let mut sql = Sql::from(&sel, D.table);
    // Returned in `id` order: the caller's sort is stable, so this is what breaks its ties.
    sql.left_join(P.table, same(P.id, D.project_id))
        .push_where(scope.as_ref())
        .order_by([Sort::by(D.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let map_row = |r: &Row| {
        let id = id.get(r)?;
        Ok(DecisionRow {
            id,
            project_id: pid.get(r)?,
            project_name: project_name.get(r)?,
            title: title.get(r)?,
            body: body.get(r)?,
            status: status.get(r)?,
            superseded_by: successors.get(&id).cloned().unwrap_or_default(),
            decided_at: decided_at.get(r)?,
            created_at: created_at.get(r)?,
            linked_task_count: linked_task_count.get(r)? as usize,
        })
    };
    // The scope's bind travels with its fragment, so the statement needs no arm per "does it have a
    // `WHERE`" — the values are whatever the predicate carried.
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), map_row)
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<DecisionRow>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// Full detail for one decision (`decision show`), served from the read-model `decision` table. The
/// query layer ([`crate::query::decision_detail`]) assembles a `view::DecisionDetail` from this row: the
/// decision is fetched by id (a row exists ⇒ it is live);
/// `project_name` is live-only (LEFT JOIN); `edges` carries both directions of the decision→decision
/// edges ([`decision_edges`]) — forward for any liveness (a `None` title = a dangling target, left for the
/// face to fill in), reverse over live decisions only; `decided_by_name` carries the decider token for any
/// liveness; and `linked_tasks` are live links to live tasks, ordered by link id, each with its title and
/// status. `None` when no row carries this id.
pub struct DecisionDetailRow {
    pub id: i64,
    pub project_id: i64,
    pub project_name: Option<String>,
    pub title: String,
    pub body: String,
    pub status: String,
    /// The decision→decision edges, both directions.
    pub edges: DecisionEdges,
    pub decided_at: Option<String>,
    pub decided_by_id: Option<String>,
    /// The same facet token as [`DecisionDetailRow::decided_by_id`], read from the same column: there is
    /// nobody to look up, so this is set exactly when the id is, and `None` only for a decision no one has
    /// decided yet.
    pub decided_by_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Live links to live tasks, ordered by link id.
    pub linked_tasks: Vec<LinkedTaskRow>,
}

/// One task a decision produced, as the read surfaces list it: the link target plus its status, so "is
/// the work this decision generated still open?" is answerable from the decision alone.
pub struct LinkedTaskRow {
    pub id: i64,
    pub title: String,
    /// The task's stored status (`todo` / `in_progress` / `done` / `blocked`), parsed by the caller.
    pub status: String,
}

pub fn decision_detail(conn: &Connection, decision_id: i64) -> Result<Option<DecisionDetailRow>> {
    let started = std::time::Instant::now();
    const D: col::decision::Cols = col::decision::of("d");
    const P: col::project::Cols = col::project::of("p");
    let mut sel = Select::new();
    let (id, pid) = (sel.col(D.id), sel.col(D.project_id));
    // `NOT NULL` in the registry, `NULL` through this `LEFT JOIN` when the project is gone — see
    // `decision_list`.
    let project_name = sel.col(P.name.nullable());
    let (title, body, status) = (sel.col(D.title), sel.col(D.body), sel.col(D.status));
    // `decided_by` is TEXT (a name string, not an fk), read into both fields.
    let (decided_at, decided_by) = (sel.col(D.decided_at), sel.col(D.decided_by));
    let (created_at, updated_at) = (sel.col(D.created_at), sel.col(D.updated_at));
    let mut sql = Sql::from(&sel, D.table);
    sql.left_join(P.table, same(P.id, D.project_id))
        .push_where(Some(&Pred::eq(D.id, decision_id)));
    let row = conn
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
            Ok(DecisionDetailRow {
                id: id.get(r)?,
                project_id: pid.get(r)?,
                project_name: project_name.get(r)?,
                title: title.get(r)?,
                body: body.get(r)?,
                status: status.get(r)?,
                // The edges are two indexed seeks of their own (`decision_edges`), filled below.
                edges: DecisionEdges::default(),
                decided_at: decided_at.get(r)?,
                decided_by_id: decided_by.get(r)?,
                decided_by_name: decided_by.get(r)?,
                created_at: created_at.get(r)?,
                updated_at: updated_at.get(r)?,
                linked_tasks: Vec::new(),
            })
        })
        .optional()
        .map_err(StoreEngineError::from)?;
    let mut row = match row {
        Some(r) => r,
        None => return Ok(None),
    };
    const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
    const T: col::task::Cols = col::task::of("t");
    let mut linked = Select::new();
    let (lid, ltitle, lstatus) = (linked.col(T.id), linked.col(T.title), linked.col(T.status));
    let mut sql = Sql::from(&linked, L.table);
    sql.join(T.table, same(T.id, L.task_id))
        .push_where(Some(&Pred::eq(L.decision_id, decision_id)))
        .order_by([Sort::by(L.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    row.linked_tasks = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(LinkedTaskRow { id: lid.get(r)?, title: ltitle.get(r)?, status: lstatus.get(r)? })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<LinkedTaskRow>>>()
        .map_err(StoreEngineError::from)?;
    row.edges = decision_edges(conn, decision_id)?;
    crate::perf::record_query("engine.decision_detail", 1, row.linked_tasks.len(), started.elapsed());
    Ok(Some(row))
}

/// Where a task sits, as carried by [`TaskDetailRow`]. `project_name` is live-only (LEFT JOIN; the
/// caller renders a missing name as "").
pub struct PlacementRow {
    pub project_id: i64,
    pub project_name: Option<String>,
    pub order_key: String,
}

/// The task's placement, with the live-only project name. Placement is task-held, so this yields nothing
/// for an unplaced (inbox) task and one row otherwise — the shape a detail row and a card both carry.
fn placement_of(conn: &Connection, task_id: i64) -> Result<Option<PlacementRow>> {
    const T: col::task::Cols = col::task::of("t");
    const P: col::project::Cols = col::project::of("p");
    let mut sel = Select::new();
    // Neither optionality here is the column's own: `project_id` is nullable in the registry and the
    // `WHERE` below is what makes it present, while `p.name` is `NOT NULL` there and the `LEFT JOIN` is
    // what can leave it absent. Both are the registry's columns, restated by the query that knows.
    let project_id = sel.col(T.project_id.required());
    let project_name = sel.col(P.name.nullable());
    let order_key = sel.col(T.order_key);
    let pred = Pred::eq(T.id, task_id).and(Pred::is_not_null(T.project_id));
    let mut sql = Sql::from(&sel, T.table);
    sql.left_join(P.table, same(P.id, T.project_id)).push_where(Some(&pred));
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(PlacementRow {
                project_id: project_id.get(r)?,
                project_name: project_name.get(r)?,
                order_key: order_key.get(r)?.unwrap_or_default(),
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<PlacementRow>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows.into_iter().next())
}

/// Open blockers: live dependency → a blocker that has not ended, in dependency-`id` order, with title. `ready` is
/// this being empty (and no unsettled premise) — the same predicate the `ready:` filter applies.
fn open_blockers(conn: &Connection, task_id: i64) -> Result<Vec<(i64, String)>> {
    const D: col::task_dependency::Cols = col::task_dependency::of("d");
    const B: col::task::Cols = col::task::of("b");
    let mut sel = Select::new();
    let (bid, btitle) = (sel.col(B.id), sel.col(B.title));
    let pred = Pred::eq(D.task_id, task_id).and(still_open(B.status));
    let mut sql = Sql::from(&sel, D.table);
    sql.join(B.table, same(B.id, D.blocked_by_id))
        .push_where(Some(&pred))
        .order_by([Sort::by(D.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| Ok((bid.get(r)?, btitle.get(r)?)))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// What premises a task acquired **after its current status began** — the read behind `AMB-D-366`'s
/// "surface". Each field lists the premises that moved since [`crate::model::Task::status_changed_at`]
/// *and* still bear on readiness: blockers that have not ended, and linked decisions that are unsettled.
/// That conjunction is deliberate — `D-366` scopes a "premise change" to exactly what **silently drops
/// `ready`**, so an edge onto an already-done task, or a link onto a settled decision, is not one (it never
/// moved `ready`). The same [`still_open`] and [`unsettled_premise`] predicates the `ready:` filter and
/// [`task_detail`] apply are reused here, so the detection cannot drift from what actually blocks.
///
/// **Two ways a premise moves**, and both are read (`AMB-D-373`): one is *drawn* at the task after the
/// status began (the two `added_*` fields), the other was there all along and *stopped being settled* under
/// the holder — a decision reopened out of `accepted`, which drops `ready` just as silently as a new link
/// does. Detaching a premise is neither: it is a hard delete ([`crate::ops::dependency::remove`],
/// [`crate::ops::decision::unlink`]) that leaves no row to date — and a removal lifts `ready`, never drops
/// it, so it falls outside the scope above regardless.
///
/// **What dates a premise is an intent column, never a record one** (`AMB-D-372`): the edge's
/// [`established_at`](crate::model::TaskDependency::established_at), the link's
/// [`linked_at`](crate::model::DecisionTaskLink::linked_at), and the decision edge's
/// [`drawn_at`](crate::model::DecisionEdge::drawn_at), each fixed when the row was written. Their
/// `created_at` holds the same instant, but a record column is one an out-of-band batch, migration or
/// restore may rewrite — and the moment it moved, this judgement would misfire on rows nobody touched.
pub struct PremiseChangeRow {
    /// Not-done blockers whose edge was added after the status began (blocker id + title), in edge-`id`
    /// order.
    pub added_blockers: Vec<(i64, String)>,
    /// Unsettled decisions linked after the status began (decision id + title), in link-`id` order.
    pub added_decisions: Vec<(i64, String)>,
    /// Decisions linked to this task that **stopped being settled** after the status began — reopened out
    /// of `accepted` (dated by the decision's own status clock) or superseded (dated by the `supersedes`
    /// edge's) — as decision id + title, in link-`id` order. Disjoint from `added_decisions`: a decision
    /// that arrived after the status began is reported once, as the link it is.
    pub reopened_decisions: Vec<(i64, String)>,
}

impl PremiseChangeRow {
    /// Whether any premise moved after the status began — the bare "has it changed?" bit `D-366` asks for,
    /// leaving *how strongly to react* to the caller.
    pub fn any(&self) -> bool {
        !self.added_blockers.is_empty()
            || !self.added_decisions.is_empty()
            || !self.reopened_decisions.is_empty()
    }
}

/// Detect premises acquired after a task's current status began (`AMB-D-366`). `None` when the task does
/// not exist; an empty [`PremiseChangeRow`] when it does but was never stamped (a store predating the
/// `status_changed_at` column — nothing to compare against). Read-only: the reaction is the caller's.
///
/// A premise whose own intent column is unset is quiet for the same reason, from the other side: `NULL > ?`
/// is not true, so an edge, a link, or a decision whose clock no write and no backfill ever set reads as
/// predating the task's. Erring quiet on an undatable premise is the safe side — the loud side would warn on
/// every row of a store restored from a snapshot old enough to lack the column.
pub fn premise_change_since(conn: &Connection, task_id: i64) -> Result<Option<PremiseChangeRow>> {
    let started = std::time::Instant::now();
    let Some(t) = task(conn, task_id)? else {
        return Ok(None);
    };
    let Some(since) = t.status_changed_at else {
        // No status clock (older store) — there is no instant to compare a premise's age against.
        return Ok(Some(PremiseChangeRow {
            added_blockers: Vec::new(),
            added_decisions: Vec::new(),
            reopened_decisions: Vec::new(),
        }));
    };
    // Columns store the `to_rfc3339_z` form (fixed-width UTC), so a lexicographic `>` is chronological.
    let since = since.to_rfc3339_z();

    // Blockers added after the status began that have not ended.
    let added_blockers = {
        const D: col::task_dependency::Cols = col::task_dependency::of("d");
        const B: col::task::Cols = col::task::of("b");
        let mut sel = Select::new();
        let (bid, btitle) = (sel.col(B.id), sel.col(B.title));
        let pred = Pred::eq(D.task_id, task_id)
            .and(still_open(B.status))
            .and(Pred::cmp(D.established_at, ">", since.clone()));
        let mut sql = Sql::from(&sel, D.table);
        sql.join(B.table, same(B.id, D.blocked_by_id)).push_where(Some(&pred)).order_by([Sort::by(D.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| Ok((bid.get(r)?, btitle.get(r)?)))
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<(i64, String)>>>()
            .map_err(StoreEngineError::from)?;
        rows
    };

    // Decisions linked after the status began that are unsettled.
    let added_decisions = {
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        const DC: col::decision::Cols = col::decision::of("dc");
        let mut sel = Select::new();
        let (did, dtitle) = (sel.col(DC.id), sel.col(DC.title));
        let pred = Pred::eq(L.task_id, task_id)
            .and(unsettled_premise(DC))
            .and(Pred::cmp(L.linked_at, ">", since.clone()));
        let mut sql = Sql::from(&sel, L.table);
        sql.join(DC.table, same(DC.id, L.decision_id)).push_where(Some(&pred)).order_by([Sort::by(L.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| Ok((did.get(r)?, dtitle.get(r)?)))
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<(i64, String)>>>()
            .map_err(StoreEngineError::from)?;
        rows
    };

    // Linked decisions that stopped being settled after the status began (`AMB-D-373`). The link may be
    // older than the reservation — what is dated here is the premise's *own* clock, not the link's. A
    // decision the query above already named is dropped: the link is the earlier, more informative fact,
    // and one premise deserves one line.
    //
    // Two ways a premise stops being settled, and `unsettled_premise` holds for both, so both are dated
    // here or the axis is half deaf. A **reopen** out of `accepted` moves the decision's status clock, the
    // mirror of the task's own. A **supersession** moves nothing on the premise: the old row is never
    // rewritten, because being superseded is an edge and not a status — so it is dated by the edge's own
    // intent column instead, the third of the three (`AMB-D-372`).
    let reopened_decisions = {
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        const DC: col::decision::Cols = col::decision::of("dc");
        let mut sel = Select::new();
        let (did, dtitle) = (sel.col(DC.id), sel.col(DC.title));
        let pred = Pred::eq(L.task_id, task_id).and(unsettled_premise(DC)).and(
            Pred::cmp(DC.status_changed_at, ">", since.clone()).or(superseded_since(DC, since)),
        );
        let mut sql = Sql::from(&sel, L.table);
        sql.join(DC.table, same(DC.id, L.decision_id)).push_where(Some(&pred)).order_by([Sort::by(L.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| Ok((did.get(r)?, dtitle.get(r)?)))
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<(i64, String)>>>()
            .map_err(StoreEngineError::from)?;
        rows.into_iter().filter(|(id, _)| !added_decisions.iter().any(|(a, _)| a == id)).collect::<Vec<_>>()
    };

    let n = added_blockers.len() + added_decisions.len() + reopened_decisions.len();
    crate::perf::record_query("engine.premise_change_since", n, n, started.elapsed());
    Ok(Some(PremiseChangeRow { added_blockers, added_decisions, reopened_decisions }))
}

/// How many comments the task carries — the count a detail row and a card both show.
fn comment_count(conn: &Connection, task_id: i64) -> Result<usize> {
    const C: col::task_comment::Cols = col::task_comment::ALL;
    let mut sel = Select::new();
    let count = sel.count_all();
    let mut sql = Sql::from(&sel, C.table);
    sql.push_where(Some(&Pred::eq(C.task_id, task_id)));
    let n: i64 = conn
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| count.get(r))
        .map_err(StoreEngineError::from)?;
    Ok(n as usize)
}

/// Full detail for one task (`task show`), served from the read-model with one indexed query per part
/// instead of a scan per part. The query layer ([`crate::query::task_detail`]) parses the text fields and
/// assembles a `view::TaskDetail`; what this row promises is that the task is fetched by id (a row
/// exists ⇒ it is live, so a deleted task is `None` here and `show` reports "no such task"), that
/// `placement` is where the task sits (absent when it is unplaced) with the live-only project name, that `blocked_by` are the live blockers that have not ended, in dependency-`id` order
/// with their titles and `blocked_by_decisions` the live, unsettled linked decisions (`ready` is derived
/// as both being empty), and that `num_comments` is the live comment count.
pub struct TaskDetailRow {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub subtype: String,
    pub status: String,
    pub completed_at: Option<String>,
    pub created_by_kind: Option<String>,
    pub assignee_kind: Option<String>,
    pub start_on: Option<String>,
    pub due_on: Option<String>,
    pub priority: Option<String>,
    /// Whether the creation is still unfinished — the fourth premise of `ready` (`AMB-D-553`), read
    /// straight off the row like `status`.
    pub draft: bool,
    pub created_at: String,
    pub updated_at: String,
    pub placement: Option<PlacementRow>,
    /// `(id, title)` of live blockers that have not ended, in dependency-`id` order.
    pub blocked_by: Vec<(i64, String)>,
    /// `(id, title)` of live dependents that have not ended (this task is their blocker), in dependency-`id`
    /// order — the reverse of `blocked_by` (what finishing this task would unblock).
    pub blocks: Vec<(i64, String)>,
    /// `(id, title)` of the unsettled decisions linked to this task, in link-`id` order — a task cannot
    /// be reserved while one of them is unsettled.
    pub blocked_by_decisions: Vec<(i64, String)>,
    pub num_comments: usize,
}

pub fn task_detail(conn: &Connection, task_id: i64) -> Result<Option<TaskDetailRow>> {
    let started = std::time::Instant::now();
    const T: col::task::Cols = col::task::of("t");
    let mut sel = Select::new();
    let (id, title, notes) = (sel.col(T.id), sel.col(T.title), sel.col(T.notes));
    let (subtype, status, completed_at) = (sel.col(T.subtype), sel.col(T.status), sel.col(T.completed_at));
    let (created_by_kind, assignee_kind) = (sel.col(T.created_by_kind), sel.col(T.assignee_kind));
    let (start_on, due_on, priority) = (sel.col(T.start_on), sel.col(T.due_on), sel.col(T.priority));
    let draft = sel.col(T.draft);
    let (created_at, updated_at) = (sel.col(T.created_at), sel.col(T.updated_at));
    let mut sql = Sql::from(&sel, T.table);
    sql.push_where(Some(&Pred::eq(T.id, task_id)));
    let row = conn
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
            Ok(TaskDetailRow {
                id: id.get(r)?,
                title: title.get(r)?,
                notes: notes.get(r)?,
                subtype: subtype.get(r)?,
                status: status.get(r)?,
                completed_at: completed_at.get(r)?,
                created_by_kind: created_by_kind.get(r)?,
                assignee_kind: assignee_kind.get(r)?,
                start_on: start_on.get(r)?,
                due_on: due_on.get(r)?,
                priority: priority.get(r)?,
                draft: draft.get(r)?,
                created_at: created_at.get(r)?,
                updated_at: updated_at.get(r)?,
                placement: None,
                blocked_by: Vec::new(),
                blocks: Vec::new(),
                blocked_by_decisions: Vec::new(),
                num_comments: 0,
            })
        })
        .optional()
        .map_err(StoreEngineError::from)?;
    let mut row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    row.placement = placement_of(conn, task_id)?;
    row.blocked_by = open_blockers(conn, task_id)?;

    // Dependents (reverse of `blocked_by`): live dependency → a live task that has not ended and has this task as
    // its blocker, in dependency-`id` order, with title (what finishing this task would unblock).
    {
        const D: col::task_dependency::Cols = col::task_dependency::of("d");
        let mut sel = Select::new();
        let (tid, ttitle) = (sel.col(T.id), sel.col(T.title));
        let pred = Pred::eq(D.blocked_by_id, task_id).and(still_open(T.status));
        let mut sql = Sql::from(&sel, D.table);
        sql.join(T.table, same(T.id, D.task_id))
            .push_where(Some(&pred))
            .order_by([Sort::by(D.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        row.blocks = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((tid.get(r)?, ttitle.get(r)?))
            })
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<(i64, String)>>>()
            .map_err(StoreEngineError::from)?;
    }

    // Unsettled premises: live link → live, `unsettled_premise` decision, in link-`id` order, with
    // title. `ready` requires this to be empty as well as `blocked_by`.
    {
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        const DC: col::decision::Cols = col::decision::of("dc");
        let mut sel = Select::new();
        let (did, dtitle) = (sel.col(DC.id), sel.col(DC.title));
        let pred = Pred::eq(L.task_id, task_id).and(unsettled_premise(DC));
        let mut sql = Sql::from(&sel, L.table);
        sql.join(DC.table, same(DC.id, L.decision_id))
            .push_where(Some(&pred))
            .order_by([Sort::by(L.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        row.blocked_by_decisions = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((did.get(r)?, dtitle.get(r)?))
            })
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<(i64, String)>>>()
            .map_err(StoreEngineError::from)?;
    }

    row.num_comments = comment_count(conn, task_id)?;

    crate::perf::record_query("engine.task_detail", 1, usize::from(row.placement.is_some()), started.elapsed());
    Ok(Some(row))
}

/// One actor (assignee / creator) on a GUI card. An actor **is** its facet, so `kind` is the stored facet
/// string (`human`/`ai`) and `name` is that facet's default label ([`facet_display`]), which the GUI
/// overrides from `config`.
pub struct CardActor {
    /// The actor facet (`human`/`ai`; None when there is none).
    pub kind: Option<String>,
    pub name: String,
}

/// The default display label for an actor facet string (`human`/`ai`). The read layer emits the facet's
/// default label; the GUI overrides it from `config.human_name`/`ai_name`.
fn facet_display(kind: Option<&str>) -> String {
    match kind {
        Some("ai") => crate::config::default_ai_name(None),
        _ => crate::config::default_human_name(None),
    }
}

/// Everything a GUI task card (`TaskCardDto`) needs, read per id off the indexed read-model so a page
/// of cards costs O(result) instead of a full-store pass per tick. What one row carries: the task's
/// placement (absent when it is unplaced), open blockers (id + title), the live comment count, and the
/// actors with fallback-to-id names. The command layer derives the top-level project
/// id, `is_mine`/`is_my_other_session`, and `ready` (= `blocked_by` and `blocked_by_decisions` both
/// empty).
pub struct TaskCardRow {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub status: String,
    pub priority: Option<String>,
    pub due_on: Option<String>,
    /// The declared start day, raw. It rides along unread, the way `due_on` does: whether the day has
    /// come is [`crate::view::not_started_until`]'s to say, and the surface asks it against its own
    /// `today` rather than the row freezing an answer.
    pub start_on: Option<String>,
    /// Whether the creation is still unfinished — the fourth premise of `ready` (`AMB-D-553`). Unlike
    /// `start_on` it needs no reading against a clock: the stage a creation is at is stored, so the row
    /// carries the answer itself.
    pub draft: bool,
    pub completed_at: Option<String>,
    pub assignee: Option<CardActor>,
    pub created_by: Option<CardActor>,
    /// Where the task sits; absent when it is unplaced (inbox).
    pub placement: Option<PlacementRow>,
    /// `(id, title)` of live blockers that have not ended, in dependency-`id` order.
    pub blocked_by: Vec<(i64, String)>,
    pub num_comments: usize,
    /// Live links to live decisions that motivate this task, in link-`id` order (`ref` = `AMB-D-n`) — the
    /// reverse of [`DecisionCardRow::linked_tasks`].
    pub linked_decisions: Vec<DecisionCardRef>,
    /// The subset of `linked_decisions` that is unsettled. Together with `blocked_by` this decides
    /// `ready` (both empty ⇒ ready), mirroring [`reserve_blockers`].
    pub blocked_by_decisions: Vec<DecisionCardRef>,
}

/// Hydrate one GUI task card from the read-model (see [`TaskCardRow`]); `None` when no row carries
/// this id. Indexed SQL per id, so hydrating a page costs O(result) — nothing walks the whole store.
pub fn task_card_row(conn: &Connection, task_id: i64) -> Result<Option<TaskCardRow>> {
    let started = std::time::Instant::now();
    const T: col::task::Cols = col::task::of("t");
    let mut sel = Select::new();
    let (id, title, notes, status) = (sel.col(T.id), sel.col(T.title), sel.col(T.notes), sel.col(T.status));
    let (priority, due_on, completed_at) = (sel.col(T.priority), sel.col(T.due_on), sel.col(T.completed_at));
    let (start_on, draft) = (sel.col(T.start_on), sel.col(T.draft));
    let (assignee, creator) = (sel.col(T.assignee_kind), sel.col(T.created_by_kind));
    let mut sql = Sql::from(&sel, T.table);
    sql.push_where(Some(&Pred::eq(T.id, task_id)));
    let row = conn
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
            let assignee_kind = assignee.get(r)?;
            let created_by_kind = creator.get(r)?;
            Ok(TaskCardRow {
                id: id.get(r)?,
                title: title.get(r)?,
                notes: notes.get(r)?,
                status: status.get(r)?,
                priority: priority.get(r)?,
                due_on: due_on.get(r)?,
                start_on: start_on.get(r)?,
                draft: draft.get(r)?,
                completed_at: completed_at.get(r)?,
                assignee: assignee_kind
                    .map(|k| CardActor { name: facet_display(Some(&k)), kind: Some(k) }),
                created_by: created_by_kind
                    .map(|k| CardActor { name: facet_display(Some(&k)), kind: Some(k) }),
                placement: None,
                blocked_by: Vec::new(),
                num_comments: 0,
                linked_decisions: Vec::new(),
                blocked_by_decisions: Vec::new(),
            })
        })
        .optional()
        .map_err(StoreEngineError::from)?;
    let mut row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    row.placement = placement_of(conn, task_id)?;
    row.blocked_by = open_blockers(conn, task_id)?;
    row.num_comments = comment_count(conn, task_id)?;

    // Live links → live decisions, in link-`id` order, each with its `D-n` ref (the reverse of
    // `decision_card_row`'s linked_tasks).
    // The `unsettled_premise` ones are also the premises that hold `ready` down, so that predicate
    // rides along as a column and splits the list in one pass — it is *not* "status is not accepted":
    // a decision another one supersedes is accepted and still blocks.
    {
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        const DC: col::decision::Cols = col::decision::of("dc");
        let mut sel = Select::new();
        let (did, dtitle) = (sel.col(DC.id), sel.col(DC.title));
        // The shared predicate rides along as a selected column, carrying whatever binds it has
        // (`Select::pred`) — the card splits its links into blocked/linked in one pass.
        let unsettled_col = sel.pred(unsettled_premise(DC));
        let mut sql = Sql::from(&sel, L.table);
        sql.join(DC.table, same(DC.id, L.decision_id))
            .push_where(Some(&Pred::eq(L.task_id, task_id)))
            .order_by([Sort::by(L.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let linked = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                let id = did.get(r)?;
                let name = dtitle.get(r)?;
                let unsettled = unsettled_col.get(r)?;
                let display_ref = Some(crate::idref::decision(id));
                // Live decision (`DC.title` off the join), so the title is always there.
                Ok((DecisionCardRef { id, name: Some(name), display_ref }, unsettled))
            })
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<(DecisionCardRef, bool)>>>()
            .map_err(StoreEngineError::from)?;
        for (d, unsettled) in linked {
            if unsettled {
                row.blocked_by_decisions.push(DecisionCardRef {
                    id: d.id,
                    name: d.name.clone(),
                    display_ref: d.display_ref.clone(),
                });
            }
            row.linked_decisions.push(d);
        }
    }

    crate::perf::record_query("engine.task_card_row", 1, usize::from(row.placement.is_some()), started.elapsed());
    Ok(Some(row))
}

/// A cross-entity reference on a GUI decision card: the target's id, display name, and conversational
/// ref (`AMB-D-n` for a decision, `AMB-T-n` for a task; `None` for projects/users — they have no such ref).
/// The number each ref is composed from is looked up by id, so a card costs no scan per target.
pub struct DecisionCardRef {
    pub id: i64,
    /// `None` when a forward edge dangles (a supersedes / amends / builds_on target no longer live), so
    /// its title cannot be read. The face composes the placeholder; core holds no display string.
    pub name: Option<String>,
    pub display_ref: Option<String>,
}

/// One premise on a GUI decision card: the `builds_on` target, plus the successor that has overturned it
/// (`AMB-D-n`), or `None` while the premise still holds. Currency is not carried separately — it *is* the
/// emptiness of `superseded_by`, a derived projection and never a stored flag.
pub struct DecisionCardPremise {
    pub decision: DecisionCardRef,
    pub superseded_by: Option<String>,
}

/// Everything a GUI decision card (`DecisionDto`) needs, read per id off the indexed read-model — the
/// decision counterpart of [`TaskCardRow`], so a page of cards costs O(result). What one row carries: the
/// live-only project name, the supersedes target (any liveness, `name` left `None` when it dangles) and
/// the live superseded-by reverse link, the decider's token, and the live
/// links to live tasks — each cross-ref carrying its `AMB-D-n`/`AMB-T-n` ref. `decided_at`/`created_at` are the
/// stored rfc3339 strings, which the caller re-normalizes through `Timestamp` before display.
pub struct DecisionCardRow {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub status: String,
    pub decided_at: Option<String>,
    pub created_at: String,
    /// The owning project; `None` when the project is not live.
    pub project: Option<ProjectRef>,
    /// The decisions this one replaced (any liveness; `ref` = `AMB-D-n`). A set, not a single target: one
    /// decision may supersede several.
    pub supersedes: Vec<DecisionCardRef>,
    /// The live decisions that replaced this one (reverse edges; `ref` = `AMB-D-n`).
    pub superseded_by: Vec<DecisionCardRef>,
    /// The decisions this one amends (any liveness; `ref` = `AMB-D-n`). Unlike `supersedes`, an amend
    /// draws no claim that the target was replaced — read the two together.
    pub amends: Vec<DecisionCardRef>,
    /// The live decisions that amend this one (reverse edges; `ref` = `AMB-D-n`).
    pub amended_by: Vec<DecisionCardRef>,
    /// The premises this decision stands on (`builds_on`; any liveness, `ref` = `AMB-D-n`) — read these
    /// first. Each carries the successor that overturned it, if any, so the GUI can surface a decision
    /// standing on a rotten premise.
    pub builds_on: Vec<DecisionCardPremise>,
    /// The live decisions that stand on this one (reverse edges; `ref` = `AMB-D-n`) — its impact radius:
    /// what a supersede of this decision puts up for review.
    pub built_on_by: Vec<DecisionCardRef>,
    /// The decider — an opaque token, not an entity key, so it is a plain [`crate::view::Ref`] and not a
    /// [`DecisionCardRef`]. `None` when none was recorded.
    pub decided_by: Option<crate::view::Ref>,
    /// Live links to live tasks, in link-`id` order (`ref` = `AMB-T-n`), each with its status.
    pub linked_tasks: Vec<LinkedTaskCardRef>,
}

/// One task a decision produced, on a GUI decision card: the `AMB-T-n` reference plus the task's status, so
/// the card says whether the work it generated is still open.
pub struct LinkedTaskCardRef {
    pub task: DecisionCardRef,
    pub status: String,
}

/// Hydrate one GUI decision card from the read-model (see [`DecisionCardRow`]); `None` when no row
/// carries this id. Reads indexed SQL per id, so a page of cards costs O(result).
pub fn decision_card_row(conn: &Connection, decision_id: i64) -> Result<Option<DecisionCardRow>> {
    let started = std::time::Instant::now();
    // One row pulls the decision plus its project (live-only) and decider (any liveness); the
    // decision→decision edges are two indexed seeks of their own (`decision_edges`).
    const D: col::decision::Cols = col::decision::of("d");
    const P: col::project::Cols = col::project::of("p");
    let mut sel = Select::new();
    let (id, title, body, status) = (sel.col(D.id), sel.col(D.title), sel.col(D.body), sel.col(D.status));
    let (decided_at, created_at) = (sel.col(D.decided_at), sel.col(D.created_at));
    let pid = sel.col(D.project_id);
    // `NOT NULL` in the registry, `NULL` through this `LEFT JOIN` when the project is gone — see
    // `decision_list`.
    let pname = sel.col(P.name.nullable());
    // `decided_by` is TEXT (a name string, not an fk), read into both fields.
    let decided_by = sel.col(D.decided_by);
    let mut sql = Sql::from(&sel, D.table);
    sql.left_join(P.table, same(P.id, D.project_id)).push_where(Some(&Pred::eq(D.id, decision_id)));
    let row = conn
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
            let project_id = pid.get(r)?;
            let project_name = pname.get(r)?;
            let decided_by_id = decided_by.get(r)?;
            let decided_by_name = decided_by.get(r)?;
            Ok(DecisionCardRow {
                id: id.get(r)?,
                title: title.get(r)?,
                body: body.get(r)?,
                status: status.get(r)?,
                decided_at: decided_at.get(r)?,
                created_at: created_at.get(r)?,
                // Project: present only when live.
                project: project_name.map(|name| ProjectRef { id: project_id, name }),
                // The edge sets are filled below.
                supersedes: Vec::new(),
                superseded_by: Vec::new(),
                amends: Vec::new(),
                amended_by: Vec::new(),
                builds_on: Vec::new(),
                built_on_by: Vec::new(),
                // Decider: an opaque TEXT token read into both id and name, so the name is present
                // whenever the id is — the `unwrap_or_default` never fires. No display placeholder.
                decided_by: decided_by_id
                    .map(|id| crate::view::Ref { id, name: decided_by_name.unwrap_or_default() }),
                linked_tasks: Vec::new(),
            })
        })
        .optional()
        .map_err(StoreEngineError::from)?;
    let mut row = match row {
        Some(r) => r,
        None => return Ok(None),
    };
    // Live links → live tasks, in link-`id` order, each with its `#n` ref (mirrors
    // `tasks_for_decision` + `view::display_ref`).
    const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
    const T: col::task::Cols = col::task::of("t");
    let mut linked = Select::new();
    let (tid, ttitle, tstatus) = (linked.col(T.id), linked.col(T.title), linked.col(T.status));
    let mut sql = Sql::from(&linked, L.table);
    sql.join(T.table, same(T.id, L.task_id))
        .push_where(Some(&Pred::eq(L.decision_id, decision_id)))
        .order_by([Sort::by(L.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    row.linked_tasks = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            let id = tid.get(r)?;
            let name = ttitle.get(r)?;
            let display_ref = Some(crate::idref::task(id));
            Ok(LinkedTaskCardRef {
                // Live task (`T.title` off the join), so the title is always there.
                task: DecisionCardRef { id, name: Some(name), display_ref },
                status: tstatus.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<LinkedTaskCardRef>>>()
        .map_err(StoreEngineError::from)?;
    // The edges, both directions. A forward target is resolved for any liveness and leaves `name` as
    // `None` when it dangles (the face composes the placeholder); a reverse edge is live-only, so its
    // title is always there (wrapped in `Some`). The id *is* the number, so the `D-n` ref is the id.
    let edges = decision_edges(conn, decision_id)?;
    let forward = |(id, title): (i64, Option<String>)| DecisionCardRef {
        display_ref: Some(crate::idref::decision(id)),
        name: title,
        id,
    };
    let reverse = |(id, title): (i64, String)| DecisionCardRef {
        display_ref: Some(crate::idref::decision(id)),
        name: Some(title),
        id,
    };
    row.supersedes = edges.supersedes.into_iter().map(forward).collect();
    row.superseded_by = edges.superseded_by.into_iter().map(reverse).collect();
    row.amends = edges.amends.into_iter().map(forward).collect();
    row.amended_by = edges.amended_by.into_iter().map(reverse).collect();
    // A premise is a forward target that carries what replaced it: the successor is rendered as a `D-n`
    // ref so the card can say *which* decision overturned the ground it stands on.
    row.builds_on = edges
        .builds_on
        .into_iter()
        .map(|p| DecisionCardPremise {
            decision: forward((p.id, p.title)),
            superseded_by: p.superseded_by.map(crate::idref::decision),
        })
        .collect();
    row.built_on_by = edges.built_on_by.into_iter().map(reverse).collect();
    crate::perf::record_query("engine.decision_card_row", 1, row.linked_tasks.len(), started.elapsed());
    Ok(Some(row))
}

/// One dimension value of a project overview (in `order_key` order). `start_on`/`end_on` are the
/// period of a `role: time_axis` value — carried for every value, meaningful only on a time_axis axis.
pub struct DimensionValueRow {
    pub id: i64,
    pub name: String,
    pub start_on: Option<NaiveDate>,
    pub end_on: Option<NaiveDate>,
}

/// One dimension (classification axis) of a project overview: id/name/notes plus its live values (in
/// `order_key` order) and the flags the GUI needs to render/operate it. `role` is the snake_case wire
/// text (`none`/`time_axis`); `ordered` says whether its values carry an order.
pub struct DimensionRow {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub role: String,
    pub ordered: bool,
    pub values: Vec<DimensionValueRow>,
}

/// One project in the snapshot overview, read from the read-model. Carries only what the snapshot's
/// `ProjectDto` needs: identity/colour/view and its live dimensions (with values).
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    /// `default_view` as stored (`list`/`board`/`calendar`/`timeline`) — passes straight to the DTO.
    pub default_view: String,
    /// not-yet-done task count (todo/in_progress/blocked) — the sidebar's per-project badge.
    pub open_count: usize,
    /// proposed (under-discussion) decision count — a `proposed`, still-current decision; a superseded
    /// proposal is no longer under discussion, and accepted/rejected are settled. Feeds the sidebar/header
    /// under-discussion badge.
    pub proposed_decision_count: usize,
    pub dimensions: Vec<DimensionRow>,
}

/// The store's live, non-archived projects with their dimensions and values — served from the read-model
/// in a handful of grouped queries. Projects and their dimensions/values are ordered by `order_key`. A
/// closed `reach` sees one project, its own; everything downstream (counts, dimensions) is keyed off the
/// rows this first query returns, so narrowing here narrows the whole overview.
pub fn project_overview(conn: &Connection, reach: crate::reach::Reach) -> Result<Vec<ProjectRow>> {
    const P: col::project::Cols = col::project::ALL;
    const D: col::dimension::Cols = col::dimension::of("d");
    const V: col::dimension_value::Cols = col::dimension_value::of("v");
    const TA: col::task::Cols = col::task::ALL;
    // The reach narrows each query through the column that query reaches the project by — the scope and
    // its value travel together, rather than as a fragment each caller had to count `?1` against. An open
    // reach adds no predicate at all.
    fn scoped<N: Nullability>(reach: Option<i64>, c: Col<Int, N>) -> Option<Pred> {
        reach.map(|pid| Pred::eq(c, pid))
    }
    let reach = reach.project();

    // Projects (non-archived) in order_key order.
    let mut projects: Vec<ProjectRow> = {
        let mut sel = Select::new();
        let (id, name, color, default_view) =
            (sel.col(P.id), sel.col(P.name), sel.col(P.color), sel.col(P.default_view));
        let pred = Pred::all([not_archived(P)].into_iter().chain(scoped(reach, P.id)));
        let mut sql = Sql::from(&sel, P.table);
        sql.push_where(pred.as_ref())
            .order_by([Sort::by(P.order_key), Sort::by(P.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok(ProjectRow {
                    id: id.get(r)?,
                    name: name.get(r)?,
                    color: color.get(r)?,
                    default_view: default_view.get(r)?,
                    open_count: 0,
                    proposed_decision_count: 0,
                    dimensions: Vec::new(),
                })
            })
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<ProjectRow>>>()
            .map_err(StoreEngineError::from)?;
        rows
    };

    // Dimensions of those projects with their live values. Two grouped queries (dimensions, then
    // values) keyed by project/dimension; order_key order preserved on both.
    let mut dimensions_by_project: HashMap<i64, Vec<DimensionRow>> = HashMap::new();
    {
        let mut sel = Select::new();
        let (project_id, id, name) = (sel.col(D.project_id), sel.col(D.id), sel.col(D.name));
        let (notes, role, ordered) = (sel.col(D.notes), sel.col(D.role), sel.col(D.ordered));
        // The project is joined to keep a dimension whose project is gone out of the overview, not for a
        // column of its own — so it is named here and nowhere else.
        let mut sql = Sql::from(&sel, D.table);
        sql.join(P.table, same(P.id, D.project_id))
            .push_where(scoped(reach, D.project_id).as_ref())
            .order_by([Sort::by(D.order_key), Sort::by(D.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((
                    project_id.get(r)?,
                    DimensionRow {
                        id: id.get(r)?,
                        name: name.get(r)?,
                        notes: notes.get(r)?,
                        role: role.get(r)?,
                        ordered: ordered.get(r)?,
                        values: Vec::new(),
                    },
                ))
            })
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let (pid, dim) = row.map_err(StoreEngineError::from)?;
            dimensions_by_project.entry(pid).or_default().push(dim);
        }
    }
    let mut values_by_dimension: HashMap<i64, Vec<DimensionValueRow>> = HashMap::new();
    {
        let mut sel = Select::new();
        let (dimension_id, id, name) = (sel.col(V.dimension_id), sel.col(V.id), sel.col(V.name));
        let (start_on, end_on) = (sel.col(V.start_on), sel.col(V.end_on));
        let mut sql = Sql::from(&sel, V.table);
        sql.join(D.table, same(D.id, V.dimension_id))
            .push_where(scoped(reach, D.project_id).as_ref())
            .order_by([Sort::by(V.order_key), Sort::by(V.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((
                    dimension_id.get(r)?,
                    DimensionValueRow {
                        id: id.get(r)?,
                        name: name.get(r)?,
                        start_on: parse_card_date(start_on.get(r)?)?,
                        end_on: parse_card_date(end_on.get(r)?)?,
                    },
                ))
            })
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let (did, val) = row.map_err(StoreEngineError::from)?;
            values_by_dimension.entry(did).or_default().push(val);
        }
    }

    // Open (not-yet-done) task count per project, in one grouped pass over live tasks. Placement is
    // task-held (`task.project_id`); "open" = status other than `done`, matching the sidebar
    // smart-view badges.
    let mut open_count_by_project: HashMap<i64, usize> = HashMap::new();
    {
        let mut sel = Select::new();
        // `project_id` is nullable in the registry, and the `WHERE` below is what makes it present here.
        let project_id = sel.col(TA.project_id.required());
        let count = sel.count_all();
        let pred = Pred::all(
            [still_open(TA.status), Pred::is_not_null(TA.project_id)]
                .into_iter()
                .chain(scoped(reach, TA.project_id)),
        );
        let mut sql = Sql::from(&sel, TA.table);
        sql.push_where(pred.as_ref()).group_by([TA.project_id.to_sql()]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((project_id.get(r)?, count.get(r)? as usize))
            })
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let (pid, count) = row.map_err(StoreEngineError::from)?;
            open_count_by_project.insert(pid, count);
        }
    }

    // Proposed (under-discussion) decision count per project, in one grouped pass. "Proposed" is the
    // stored status; a superseded proposal is excluded because currency is derived from the edges, not the
    // status — accepted/rejected fall out by the status filter alone.
    let mut proposed_count_by_project: HashMap<i64, usize> = HashMap::new();
    {
        const DC: col::decision::Cols = col::decision::ALL;
        let mut sel = Select::new();
        let project_id = sel.col(DC.project_id);
        let count = sel.count_all();
        let pred = Pred::all(
            [Pred::eq(DC.status, crate::model::DecisionStatus::Proposed.as_str()), !superseded(DC)]
                .into_iter()
                .chain(scoped(reach, DC.project_id)),
        );
        let mut sql = Sql::from(&sel, DC.table);
        sql.push_where(pred.as_ref()).group_by([DC.project_id.to_sql()]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((project_id.get(r)?, count.get(r)? as usize))
            })
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let (pid, count) = row.map_err(StoreEngineError::from)?;
            proposed_count_by_project.insert(pid, count);
        }
    }

    for proj in &mut projects {
        let mut dimensions = dimensions_by_project.remove(&proj.id).unwrap_or_default();
        for dim in &mut dimensions {
            dim.values = values_by_dimension.remove(&dim.id).unwrap_or_default();
        }
        proj.dimensions = dimensions;
        proj.open_count = open_count_by_project.remove(&proj.id).unwrap_or(0);
        proj.proposed_decision_count = proposed_count_by_project.remove(&proj.id).unwrap_or(0);
    }

    Ok(projects)
}

/// A project the store has **not** archived. The flag is a `bool` a row need not carry, so it can read as
/// `NULL`, and absent means not archived — the same reading `COALESCE(archived, 0)` gives, said in
/// predicates instead of in a column expression.
fn not_archived(p: col::project::Cols) -> Pred {
    Pred::is_null(p.archived).or(Pred::eq(p.archived, false))
}

/// Its complement: a project the store has archived. The two partition the projects, which is what keeps
/// the sidebar's active and archived sections from ever both holding one.
fn archived(p: col::project::Cols) -> Pred {
    Pred::eq(p.archived, true)
}

/// The `(dimension_id, value_id)` assignments a single task carries, one row per dimension it is placed
/// on — served from the read-model so the GUI detail pane can reflect a task's current axis values. Live
/// task↔value links to live values only, in `task_dimension_value`-`id` order.
pub fn task_dimension_assignments(conn: &Connection, task_id: i64) -> Result<Vec<(i64, i64)>> {
    const TV: col::task_dimension_value::Cols = col::task_dimension_value::of("tv");
    let mut sel = Select::new();
    const V: col::dimension_value::Cols = col::dimension_value::of("v");
    let (dimension, value) = (sel.col(TV.dimension_id), sel.col(TV.value_id));
    // The value is joined to drop an assignment whose value is gone, not for a column of its own.
    let mut sql = Sql::from(&sel, TV.table);
    sql.join(V.table, same(V.id, TV.value_id))
        .push_where(Some(&Pred::eq(TV.task_id, task_id)))
        .order_by([Sort::by(TV.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok((dimension.get(r)?, value.get(r)?))
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(i64, i64)>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// What one task is classified as, in words: `(dimension, value)` names, one pair per axis it sits on.
///
/// The same rows [`task_dimension_assignments`] serves by id, resolved here instead — for a face that
/// prints rather than joins. The GUI already holds every axis and value in hand and needs only the ids;
/// a CLI showing one task does not, and asking it to fetch the project's whole classification to name
/// two values would be the join done the long way round.
///
/// Ordered by the **axis**, not by when the assignment was made, so two tasks classified in a different
/// order still read down the same columns.
pub fn task_classification(conn: &Connection, task_id: i64) -> Result<Vec<(String, String)>> {
    const TV: col::task_dimension_value::Cols = col::task_dimension_value::of("tv");
    const D: col::dimension::Cols = col::dimension::of("d");
    const V: col::dimension_value::Cols = col::dimension_value::of("v");
    let mut sel = Select::new();
    let (dimension, value) = (sel.col(D.name), sel.col(V.name));
    let mut sql = Sql::from(&sel, TV.table);
    // Both joins are inner: an assignment whose axis or value is gone names nothing to print.
    sql.join(D.table, same(D.id, TV.dimension_id))
        .join(V.table, same(V.id, TV.value_id))
        .push_where(Some(&Pred::eq(TV.task_id, task_id)))
        .order_by([Sort::by(D.order_key), Sort::by(D.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok((dimension.get(r)?, value.get(r)?))
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(String, String)>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// The `(task_id, value_id)` assignments for one dimension across a project — lets the GUI board group a
/// project's tasks by a chosen dimension's values in one query. Live links to live values only, scoped to
/// live tasks of the project.
pub fn project_dimension_assignments(
    conn: &Connection,
    project_id: i64,
    dimension_id: i64,
) -> Result<Vec<(i64, i64)>> {
    const TV: col::task_dimension_value::Cols = col::task_dimension_value::of("tv");
    const T: col::task::Cols = col::task::of("t");
    let mut sel = Select::new();
    const V: col::dimension_value::Cols = col::dimension_value::of("v");
    let (task, value) = (sel.col(TV.task_id), sel.col(TV.value_id));
    let mut sql = Sql::from(&sel, TV.table);
    sql.join(T.table, same(T.id, TV.task_id))
        .join(V.table, same(V.id, TV.value_id))
        .push_where(Some(
            &Pred::eq(TV.dimension_id, dimension_id).and(Pred::eq(T.project_id, project_id)),
        ))
        .order_by([Sort::by(TV.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| Ok((task.get(r)?, value.get(r)?)))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(i64, i64)>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// The live name of a project, or `None` if it does not exist / is deleted. Lets read paths surface a
/// not-found error without hydrating the whole `project` Vec.
pub fn project_name(conn: &Connection, project_id: i64) -> Result<Option<String>> {
    const P: col::project::Cols = col::project::ALL;
    scalar_by_id(conn, P.id, P.name, project_id)
}

/// The editable fields of a single project (name/notes/colour/view/archived) — served from the
/// read-model so the GUI settings screen can prefill its form without hydrating the whole `project` Vec.
/// Includes archived projects (the settings screen is the unarchive path); excludes only the deleted.
/// `None` when the project is absent/deleted.
pub struct ProjectSettingsRow {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub color: Option<String>,
    pub default_view: String,
    pub archived: bool,
}

pub fn project_settings(conn: &Connection, project_id: i64) -> Result<Option<ProjectSettingsRow>> {
    const P: col::project::Cols = col::project::ALL;
    let mut sel = Select::new();
    let (id, name, notes) = (sel.col(P.id), sel.col(P.name), sel.col(P.notes));
    let (color, default_view) = (sel.col(P.color), sel.col(P.default_view));
    // `COALESCE`d: a row that carries no value for the column reads as NULL.
    let archived = sel.expr::<bool>(format!("COALESCE({}, 0)", P.archived.to_sql()));
    let mut sql = Sql::from(&sel, P.table);
    sql.push_where(Some(&Pred::eq(P.id, project_id)));
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
        Ok(ProjectSettingsRow {
            id: id.get(r)?,
            name: name.get(r)?,
            notes: notes.get(r)?,
            color: color.get(r)?,
            default_view: default_view.get(r)?,
            archived: archived.get(r)?,
        })
    })
    .optional()
    .map_err(StoreEngineError::from)
}

/// A single archived project for the GUI's "Archived (N)" sidebar section — minimal fields, no
/// dimensions. [`project_overview`] deliberately excludes archived projects to keep the active sidebar
/// clean; this is the separate read path that supplies them for listing + restore.
pub struct ArchivedProjectRow {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    /// Last-write timestamp (roughly "when archived") — the section orders on it, newest first.
    pub updated_at: String,
}

/// The store's archived (but not deleted) projects for the sidebar's "Archived" section, served from the
/// read-model. The complement of [`project_overview`]'s `archived = 0` filter, so the two paths partition
/// the live projects and never both surface the same one. Ordered newest-updated first with a stable `id`
/// tiebreak.
pub fn archived_projects(conn: &Connection) -> Result<Vec<ArchivedProjectRow>> {
    const P: col::project::Cols = col::project::ALL;
    let mut sel = Select::new();
    let (id, name, color) = (sel.col(P.id), sel.col(P.name), sel.col(P.color));
    // `COALESCE`d: a row that carries no value for the column reads as NULL.
    let updated_at = sel.expr::<String>(format!("COALESCE({}, '')", P.updated_at.to_sql()));
    let mut sql = Sql::from(&sel, P.table);
    sql.push_where(Some(&archived(P)))
        .order_by([Sort::by(P.updated_at).desc(), Sort::by(P.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(ArchivedProjectRow {
                id: id.get(r)?,
                name: name.get(r)?,
                color: color.get(r)?,
                updated_at: updated_at.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<ArchivedProjectRow>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

// ───────────────────────── card hydration ─────────────────────────

/// Box a parse failure into the `rusqlite` conversion error (mirrors `hydrate::bad`), so a bad
/// enum/date in the read-model surfaces as a normal read error rather than a panic.
fn card_bad(msg: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, msg.into())
}

/// A nullable `%Y-%m-%d` day, parsed from the value a [`Select`] slot read (same vocabulary as
/// `hydrate::date_opt`) — the column has been named once, by the projection, so there is no second name
/// to spell here.
fn parse_card_date(stored: Option<String>) -> rusqlite::Result<Option<NaiveDate>> {
    match stored {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map(Some)
            .map_err(|e| card_bad(format!("bad date {s:?}: {e}"))),
        None => Ok(None),
    }
}

/// A required enum-as-text value, parsed through the model's `parse` (the same vocabulary the forward
/// projection writes), so the card path cannot drift from what was written. The column comes along
/// to name itself in the error — it is the identifier the projection already carries, not a second
/// spelling of the column.
fn card_enum_req<T, N: Nullability>(
    col: Col<SqlText, N>,
    stored: String,
    f: impl Fn(&str) -> Option<T>,
) -> rusqlite::Result<T> {
    f(&stored).ok_or_else(|| card_bad(format!("unknown {} value {stored:?}", col.name())))
}

/// The same, for a nullable enum-as-text column: absent stays absent, present must parse.
fn card_enum_opt<T, N: Nullability>(
    col: Col<SqlText, N>,
    stored: Option<String>,
    f: impl Fn(&str) -> Option<T>,
) -> rusqlite::Result<Option<T>> {
    match stored {
        Some(s) => card_enum_req(col, s, f).map(Some),
        None => Ok(None),
    }
}

/// Task base fields a [`TaskCompact`] card needs from the `task` table (everything not resolved by
/// a join: placement context, assignee name and open-blocker derivation are filled separately).
struct CardBase {
    id: i64,
    title: String,
    status: TaskStatus,
    due_on: Option<NaiveDate>,
    start_on: Option<NaiveDate>,
    priority: Option<Priority>,
    assignee_kind: Option<ActorKind>,
    draft: bool,
}

/// The placement context of a task: the project ref resolved from its placement,
/// `None` when there is no such referent.
struct PrimaryCtx {
    project: Option<ProjectRef>,
}

/// Hydrate a set of task ids into [`TaskCompact`] cards **directly from the SQL read-model**: rather
/// than indexing every record, it joins only the page's tasks to their placement / assignee /
/// open blockers. Cards come back in the input order; an id with no *live* task is skipped. A card takes
/// the project it is placed in as its project, gates the project name on liveness, and carries the
/// live blockers that have not ended, in dependency-`id` order. The ids come from somewhere — a query, a feed, a
/// caller's own list — so the reach is declared here too: under a closed one, an id outside the bound
/// project hydrates no card, exactly as a non-live id does. Hydration is the last step before content
/// reaches a face, so this is the floor that holds even when the ids arrived without passing a scoped
/// query.
pub fn hydrate_task_cards(
    conn: &Connection,
    reach: crate::reach::Reach,
    ids: &[i64],
    today: NaiveDate,
) -> Result<Vec<TaskCompact>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // 1) Base task rows (live only, in reach only). The reach rides on the id set as one predicate, so
    //    the scope cannot be appended without its value.
    const TA: col::task::Cols = col::task::ALL;
    let mut base_by_id: HashMap<i64, CardBase> = HashMap::new();
    {
        let mut sel = Select::new();
        let (id, title, status) = (sel.col(TA.id), sel.col(TA.title), sel.col(TA.status));
        let (due_on, start_on) = (sel.col(TA.due_on), sel.col(TA.start_on));
        let (priority, assignee_kind) = (sel.col(TA.priority), sel.col(TA.assignee_kind));
        let draft = sel.col(TA.draft);
        let mut pred = Pred::is_in(TA.id, ids.iter().copied());
        if let Some(pid) = reach.project() {
            pred = pred.and(Pred::eq(TA.project_id, pid));
        }
        let mut sql = Sql::from(&sel, TA.table);
        sql.push_where(Some(&pred));
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok(CardBase {
                    id: id.get(r)?,
                    title: title.get(r)?,
                    status: card_enum_req(TA.status, status.get(r)?, TaskStatus::parse)?,
                    due_on: parse_card_date(due_on.get(r)?)?,
                    start_on: parse_card_date(start_on.get(r)?)?,
                    priority: card_enum_opt(TA.priority, priority.get(r)?, Priority::parse)?,
                    assignee_kind: card_enum_opt(
                        TA.assignee_kind,
                        assignee_kind.get(r)?,
                        ActorKind::parse,
                    )?,
                    draft: draft.get(r)?,
                })
            })
            .map_err(StoreEngineError::from)?;
        for row in rows {
            let b = row.map_err(StoreEngineError::from)?;
            base_by_id.insert(b.id, b);
        }
    }

    // The ids that hydrated a base row — an id that is not live, or (under a closed reach) not in the
    // bound project, is gone. The joins below key off these, so nothing more of an out-of-reach task is
    // read at all.
    let hydrated: Vec<i64> = ids.iter().copied().filter(|id| base_by_id.contains_key(id)).collect();
    if hydrated.is_empty() {
        return Ok(Vec::new());
    }

    // 2) Placement (task-held) + project name. The LEFT JOIN gates the ref on the referent being live,
    //    exactly as the per-id `placement_of` does.
    let mut ctx_by_task: HashMap<i64, PrimaryCtx> = HashMap::new();
    {
        const T: col::task::Cols = col::task::of("t");
        const P: col::project::Cols = col::project::of("p");
        let mut sel = Select::new();
        let task = sel.col(T.id);
        // Present by the WHERE, and absent through the LEFT JOIN when the project is gone — neither
        // optionality is the column's own (see `placement_of`).
        let project_id = sel.col(T.project_id.required());
        let pname = sel.col(P.name.nullable());
        let pred =
            Pred::is_in(T.id, hydrated.iter().copied()).and(Pred::is_not_null(T.project_id));
        let mut sql = Sql::from(&sel, T.table);
        sql.left_join(P.table, same(P.id, T.project_id)).push_where(Some(&pred));
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let mut rows =
            stmt.query(rusqlite::params_from_iter(sql.params())).map_err(StoreEngineError::from)?;
        while let Some(r) = rows.next().map_err(StoreEngineError::from)? {
            let task_id = task.get(r)?;
            if ctx_by_task.contains_key(&task_id) {
                continue;
            }
            let project = match pname.get(r)? {
                Some(name) => Some(ProjectRef { id: project_id.get(r)?, name }),
                None => None,
            };
            ctx_by_task.insert(task_id, PrimaryCtx { project });
        }
    }

    // 3) Open blockers: live dependency → a blocker that has not ended, in dependency-id order (the same
    //    `still_open` predicate the `ready:` filter and `open_blockers` apply).
    let mut blocked_by_task: HashMap<i64, Vec<i64>> = HashMap::new();
    {
        const D: col::task_dependency::Cols = col::task_dependency::of("d");
        const B: col::task::Cols = col::task::of("b");
        let mut sel = Select::new();
        let (task, blocker) = (sel.col(D.task_id), sel.col(D.blocked_by_id));
        let pred =
            Pred::is_in(D.task_id, hydrated.iter().copied()).and(still_open(B.status));
        let mut sql = Sql::from(&sel, D.table);
        sql.join(B.table, same(B.id, D.blocked_by_id))
            .push_where(Some(&pred))
            .order_by([Sort::by(D.task_id), Sort::by(D.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let mut rows =
            stmt.query(rusqlite::params_from_iter(sql.params())).map_err(StoreEngineError::from)?;
        while let Some(r) = rows.next().map_err(StoreEngineError::from)? {
            blocked_by_task.entry(task.get(r)?).or_default().push(blocker.get(r)?);
        }
    }

    // 4) Unsettled premises: decision link → `unsettled_premise` decision, in link-id order (mirrors
    //    `view::blocked_by_decisions` / the `ready:` filter, which share the same predicate).
    let mut blocked_by_decision: HashMap<i64, Vec<i64>> = HashMap::new();
    {
        const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
        let mut sel = Select::new();
        let (task, decision) = (sel.col(L.task_id), sel.col(L.decision_id));
        const DC: col::decision::Cols = col::decision::of("dc");
        let pred =
            Pred::is_in(L.task_id, hydrated.iter().copied()).and(unsettled_premise(DC));
        let mut sql = Sql::from(&sel, L.table);
        sql.join(DC.table, same(DC.id, L.decision_id))
            .push_where(Some(&pred))
            .order_by([Sort::by(L.task_id), Sort::by(L.id)]);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let mut rows =
            stmt.query(rusqlite::params_from_iter(sql.params())).map_err(StoreEngineError::from)?;
        while let Some(r) = rows.next().map_err(StoreEngineError::from)? {
            blocked_by_decision.entry(task.get(r)?).or_default().push(decision.get(r)?);
        }
    }

    // Assemble cards in input-id order; skip ids with no live task (parity with list).
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(b) = base_by_id.get(id) else { continue };
        let ctx = ctx_by_task.get(id);
        let blocked_by_open = blocked_by_task.get(id).cloned().unwrap_or_default();
        let blocked_by_decisions = blocked_by_decision.get(id).cloned().unwrap_or_default();
        let ready = crate::view::is_ready(
            !blocked_by_open.is_empty(),
            !blocked_by_decisions.is_empty(),
            b.start_on,
            today,
            b.draft,
        );
        let not_started_until = crate::view::not_started_until(b.start_on, today);
        out.push(TaskCompact {
            id: b.id,
            title: b.title.clone(),
            completed: b.status == TaskStatus::Done,
            status: b.status,
            due_on: b.due_on,
            start_on: b.start_on,
            priority: b.priority,
            draft: b.draft,
            r#ref: crate::idref::task(b.id),
            project: ctx.and_then(|c| c.project.clone()),
            assignee_kind: b.assignee_kind,
            blocked_by_open,
            blocked_by_decisions,
            not_started_until,
            ready,
        });
    }
    Ok(out)
}

/// The `status` view's four task buckets as ordered id lists, plus the three summary counts that are
/// not a bucket length (the other three counts equal `overdue`/`due_today`/`in_progress` lengths).
/// Fed to [`hydrate_task_cards`] so `status` serves O(result) cards from SQL — the id-list counterpart
/// of `query`'s `StatusBuckets`.
pub struct StatusBucketIds {
    /// Most-overdue first (`due_on` asc), then priority, then id — matching `query::status`.
    pub overdue: Vec<i64>,
    /// `due_on == today`, ordered by priority then id.
    pub due_today: Vec<i64>,
    /// `today <= due_on <= today+7` (includes today), ordered by `due_on` then id.
    pub due_week: Vec<i64>,
    /// Reserved — `status == in_progress` — in id order. The only bucket read off the status field
    /// rather than off a day: what someone has their hands on is what they declared, not what the
    /// calendar implies.
    pub in_progress: Vec<i64>,
    /// Not done with `today < due_on <= today+7` (excludes today; distinct from `due_week`).
    pub upcoming_7d: usize,
    /// Not done with no due date.
    pub no_due: usize,
    /// Done with `completed_at` on `today` (UTC, matching `Timestamp`'s storage).
    pub completed_today: usize,
}

/// Compute the [`StatusBucketIds`] from the read-model. Every bucket tiebreaks on `id`. Dates compare
/// lexicographically — the read-model stores `due_on`/`start_on` as `YYYY-MM-DD` and `completed_at` as a
/// UTC RFC3339 whose first ten chars are its date, so string comparison against `today` is chronological.
/// `reach` scopes every bucket and every count (an AI reaches only the project its `.amenbo` names, so
/// its `status` must not count every project); [`crate::reach::Reach::All`] counts everything, which is
/// what a human gets.
pub fn status_bucket_ids(
    conn: &Connection,
    today: NaiveDate,
    reach: crate::reach::Reach,
) -> Result<StatusBucketIds> {
    const TA: col::task::Cols = col::task::ALL;
    let project_id = reach.project();
    let today_s = today.to_string();
    let week_end_s = (today + Duration::days(7)).to_string();

    // Every bucket asks its question of the tasks in reach, so the scope is `AND`-ed onto the bucket's
    // own predicate — carrying its bind with it, rather than being appended as a fragment whose `?N` each
    // caller had to count off against its own params.
    let scoped = |p: Pred| match project_id {
        Some(pid) => p.and(Pred::eq(TA.project_id, pid)),
        None => p,
    };
    let open = || still_open(TA.status);
    // "Has a day on it" — the negation of the store's not-written reading, so the buckets and the
    // `due:none` filter cannot come to disagree about what an unwritten day is.
    let has_due = || !Pred::is_blank(TA.due_on);

    let collect = |pred: Pred, order: Vec<Sort>| -> Result<Vec<i64>> {
        let mut sel = Select::new();
        let id = sel.col(TA.id);
        let mut sql = Sql::from(&sel, TA.table);
        sql.push_where(Some(&scoped(pred))).order_by(order);
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let ids = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| id.get(r))
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<i64>>>()
            .map_err(StoreEngineError::from)?;
        Ok(ids)
    };
    let count = |pred: Pred| -> Result<usize> {
        let mut sel = Select::new();
        let matched = sel.count_all();
        let mut sql = Sql::from(&sel, TA.table);
        sql.push_where(Some(&scoped(pred)));
        let n: i64 = conn
            .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| matched.get(r))
            .map_err(StoreEngineError::from)?;
        Ok(n as usize)
    };

    // Every bucket tiebreaks on `id`; `priority` ranks high→none (`priority_rank`, mirroring
    // `view::priority_rank`) — a `CASE` over the enum's own words, which the registry cannot type.
    let due = || Sort::by(TA.due_on);
    let id = || Sort::by(TA.id);
    let prank = || Sort::expr(priority_rank(TA.priority));

    let overdue = collect(
        open().and(has_due()).and(Pred::cmp(TA.due_on, "<", today_s.as_str())),
        vec![due(), prank(), id()],
    )?;

    let due_today =
        collect(open().and(Pred::eq(TA.due_on, today_s.as_str())), vec![prank(), id()])?;

    let due_week = collect(
        open()
            .and(has_due())
            .and(Pred::cmp(TA.due_on, ">=", today_s.as_str()))
            .and(Pred::cmp(TA.due_on, "<=", week_end_s.as_str())),
        vec![due(), id()],
    )?;

    // Under way is a thing a person declares by reserving the task (`AMB-D-118`), so this bucket asks the
    // status field and no day at all. A start day that has come says the work *may* begin, which is a
    // different fact — reading it as this one filled the section with work nobody had picked up.
    let in_progress = collect(Pred::eq(TA.status, "in_progress"), vec![id()])?;

    let upcoming_7d = count(
        open()
            .and(has_due())
            .and(Pred::cmp(TA.due_on, ">", today_s.as_str()))
            .and(Pred::cmp(TA.due_on, "<=", week_end_s.as_str())),
    )?;

    let no_due = count(open().and(Pred::is_blank(TA.due_on)))?;

    // The instant's day is its first ten characters (a fixed-width UTC RFC3339) — a derivation over the
    // column, which the layer names (`Col::day`), so the comparison is an ordinary typed one.
    let completed_today =
        count(Pred::eq(TA.status, "done").and(Pred::eq(TA.completed_at.day(), today_s.as_str())))?;

    Ok(StatusBucketIds {
        overdue,
        due_today,
        due_week,
        in_progress,
        upcoming_7d,
        no_due,
        completed_today,
    })
}

// ───────────────────── write-side set and graph twins ─────────────────────
//
// The set and graph reads a mutation needs: the ref resolvers, the cascade sets a
// delete fans out to, and the reachability that guards a dependency cycle. Each answers its
// question **from the truth source**, inside the operation's `BEGIN IMMEDIATE` transaction
// (`super::WriteTx::conn`) — a set another writer may already have changed must not be read outside
// the transaction that acts on it. Liveness of a single task is `task_live`; the `before` snapshot a
// field diff needs is a single-record loader (below).
//
// Every one of these answers "what is there now", which is the only question a row can answer: a
// delete takes the row out, so there is nothing to filter for liveness. Ids come back ascending, which
// is the order a caller collapsing a hit set (`ops::pick_id`) or fanning a cascade out relies on.

/// The dimension's columns, and its values' — unaliased, the tables name themselves.
const DIM: col::dimension::Cols = col::dimension::ALL;
const DVAL: col::dimension_value::Cols = col::dimension_value::ALL;

/// Live dimension ids matching `reference` as a **key** or an **exact name**, scoped to
/// `project_id` when given (pass `None` to resolve across the store). The caller collapses the hit set
/// with `ops::pick_id` (0 = not-found / 1 = resolved / many = ambiguous).
pub fn resolve_dimension_in(
    conn: &Connection,
    project_id: Option<i64>,
    reference: &str,
) -> Result<Vec<i64>> {
    let mut pred = key_or_name_pred(crate::idref::RefKind::Dimension, DIM.id, DIM.name, reference);
    if let Some(pid) = project_id {
        pred = pred.and(Pred::eq(DIM.project_id, pid));
    }
    select_ids(conn, DIM.id, Some(&pred))
}

/// Dimension ids matching `reference` as a key or a **case-insensitive** name ([`key_or_folded_name_pred`]),
/// across the store — what `dim:<axis>=…` resolves its axis with. Names are per-project, so a name shared
/// by two projects resolves to both, and the filter ORs them (see `query::ResolvedDimension`); that is why
/// this returns every hit rather than collapsing with `ops::pick_id` the way [`resolve_dimension_in`]
/// does. An empty result is the caller's error to raise: an unresolvable axis must not read as
/// "nothing matched".
pub fn resolve_dimension_by_ref(conn: &Connection, reference: &str) -> Result<Vec<i64>> {
    select_ids(conn, DIM.id, Some(&key_or_folded_name_pred(crate::idref::RefKind::Dimension, DIM.id, DIM.name, reference)))
}

/// The dimensions designated as the time axis (`role: time_axis`) — what `time_axis:` resolves to, since
/// it names the *role*, not an axis (a store carries one per project).
pub fn time_axis_dimensions(conn: &Connection) -> Result<Vec<i64>> {
    select_ids(conn, DIM.id, Some(&Pred::eq(DIM.role, "time_axis")))
}

/// Value ids of the given dimensions matching `reference` as a key or a case-insensitive name — the
/// value half of [`resolve_dimension_by_ref`], scoped to the axes that reference already resolved to.
/// No axis is no value: an empty axis set matches nothing, rather than widening to every value.
pub fn resolve_dimension_value_by_ref(
    conn: &Connection,
    dimension_ids: &[i64],
    reference: &str,
) -> Result<Vec<i64>> {
    let pred = key_or_folded_name_pred(crate::idref::RefKind::DimensionValue, DVAL.id, DVAL.name, reference)
        .and(Pred::is_in(DVAL.dimension_id, dimension_ids.iter().copied()));
    select_ids(conn, DVAL.id, Some(&pred))
}

/// Live value ids of one dimension matching `reference` as a key or an exact name. Collapsed by the
/// caller with `ops::pick_id`, like [`resolve_dimension_in`].
pub fn resolve_dimension_value_in(
    conn: &Connection,
    dimension_id: i64,
    reference: &str,
) -> Result<Vec<i64>> {
    let pred = key_or_name_pred(crate::idref::RefKind::DimensionValue, DVAL.id, DVAL.name, reference)
        .and(Pred::eq(DVAL.dimension_id, dimension_id));
    select_ids(conn, DVAL.id, Some(&pred))
}

/// The dimension a value belongs to, or `None` when there is no such value — the
/// starting point of every value operation (a value carries its axis, so the axis need not be passed
/// around).
pub fn dimension_id_of_value(conn: &Connection, value_id: i64) -> Result<Option<i64>> {
    scalar_by_id(conn, DVAL.id, DVAL.dimension_id, value_id)
}

/// The value of the project's time axis whose period covers `date`, or `None` — the "current era" a new
/// task is placed on by default. Only a `role: time_axis` dimension has periods that mean anything, and
/// only a value with at least one endpoint set covers any date at all (`DimensionValue::covers`: both
/// ends open ⇒ covers nothing); an open end is unbounded on that side. Ties — overlapping windows, or a
/// store with more than one time axis — resolve on the first axis in dimension order, then the axis's own
/// value order (`order_key` when ordered, id when not). [`crate::store::Store::add_task`] reads this
/// **inside** its write transaction, so the era it assigns and the task row it assigns it to commit
/// together.
pub fn current_time_axis_value(
    conn: &Connection,
    project_id: i64,
    date: NaiveDate,
) -> Result<Option<i64>> {
    const D: col::dimension::Cols = col::dimension::of("d");
    const V: col::dimension_value::Cols = col::dimension_value::of("v");
    let day = date.format("%Y-%m-%d").to_string();
    let day = day.as_str();
    // A value with both ends open covers nothing (`DimensionValue::covers`), and an open end is
    // unbounded on its side — said in the store's own reading of an unwritten day (`is_blank`), so this
    // and the `due:` filters cannot come to disagree about what "no day" is.
    let has_a_period = (!Pred::is_blank(V.start_on)).or(!Pred::is_blank(V.end_on));
    let covers_day = Pred::is_blank(V.start_on)
        .or(Pred::cmp(V.start_on, "<=", day))
        .and(Pred::is_blank(V.end_on).or(Pred::cmp(V.end_on, ">=", day)));
    let pred = Pred::eq(D.project_id, project_id)
        .and(Pred::eq(D.role, "time_axis"))
        .and(has_a_period)
        .and(covers_day);

    let mut sel = Select::new();
    let id = sel.col(V.id);
    let mut sql = Sql::from(&sel, V.table);
    sql.join(D.table, same(D.id, V.dimension_id));
    // The tiebreak is the first axis in dimension order, then the axis's own
    // value order (`order_key` when ordered, id when not) — a `CASE` over the columns, which is a
    // derivation rather than a comparison, so it is written out. It binds nothing.
    sql.push_where(Some(&pred))
        .order_by([
            Sort::by(D.order_key),
            Sort::expr(format!(
                "CASE WHEN {} THEN {} ELSE {} END",
                D.ordered.to_sql(),
                V.order_key.to_sql(),
                V.id.to_sql()
            )),
            Sort::by(V.id),
        ])
        .limit(1);
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| id.get(r))
        .optional()
        .map_err(StoreEngineError::from)
}

/// The live values of one dimension — the subtree [`crate::ops::dimension::delete`] deletes
/// child-first, each value taking the task assignments that name it.
pub fn dimension_value_ids(conn: &Connection, dimension_id: i64) -> Result<Vec<i64>> {
    select_ids(conn, DVAL.id, Some(&Pred::eq(DVAL.dimension_id, dimension_id)))
}

/// The live assignment of `task_id` on `dimension_id`, whatever value it names. Every dimension is
/// single-select, so `ops::dimension::set` reads this to delete the outgoing assignment in the same
/// transaction as the incoming one — the `(task, dimension)` one-row invariant is only an invariant if
/// the two writes commit together. It is a set, not a row, because the invariant is what keeps it a row:
/// a store that broke it must not silently keep the extra assignment.
pub fn assignment_ids_on_axis(
    conn: &Connection,
    task_id: i64,
    dimension_id: i64,
) -> Result<Vec<i64>> {
    const TV: col::task_dimension_value::Cols = col::task_dimension_value::ALL;
    let pred = Pred::eq(TV.task_id, task_id).and(Pred::eq(TV.dimension_id, dimension_id));
    select_ids(conn, TV.id, Some(&pred))
}

/// The live assignment of `task_id` to exactly `value_id`, or `None` — what makes
/// `ops::dimension::set` idempotent (re-assigning the same value is a noop) and `unset` a lookup.
pub fn assignment_id(
    conn: &Connection,
    task_id: i64,
    value_id: i64,
) -> Result<Option<i64>> {
    const TV: col::task_dimension_value::Cols = col::task_dimension_value::ALL;
    first_id(conn, TV.id, &Pred::eq(TV.task_id, task_id).and(Pred::eq(TV.value_id, value_id)))
}

/// Can `from` reach `target` by walking live dependency edges (`task_id → blocked_by_id`)? A recursive
/// CTE whose `UNION` dedups the frontier, so an already-cyclic store terminates instead of looping; the
/// seed is `from` itself, so `reaches(x, x)` is `true`. This is the cycle guard of
/// [`crate::ops::dependency::add`]: an edge `a → b` closes a cycle iff `b` already reaches `a`. Reading
/// it inside the write transaction is the point — two concurrent adds that each saw an acyclic graph can
/// commit a cycle between them.
pub fn dependency_reaches(conn: &Connection, from: i64, target: i64) -> Result<bool> {
    const D: col::task_dependency::Cols = col::task_dependency::of("d");
    // The frontier `reach` is the statement's own table — it exists for the length of the query and the
    // registry has never heard of it, so its column is the one thing here named in text. The edges it
    // walks are the registry's, and the two ids go in through the statement's bind seam.
    let mut sql = Sql::new("WITH RECURSIVE reach(id) AS ( SELECT ");
    sql.bind(from).push(format!(
        " UNION SELECT {} FROM task_dependency d JOIN reach r ON {} = r.id ) \
         SELECT EXISTS (SELECT 1 FROM reach WHERE id = ",
        D.blocked_by_id.to_sql(),
        D.task_id.to_sql()
    ));
    sql.bind(target).push(")");
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| r.get::<_, bool>(0))
        .map_err(StoreEngineError::from)
}

/// The live dependency edge `task_id → blocked_by_id`, or `None` — what makes `add` idempotent and
/// `remove` a lookup.
pub fn dependency_id(
    conn: &Connection,
    task_id: i64,
    blocked_by_id: i64,
) -> Result<Option<i64>> {
    const D: col::task_dependency::Cols = col::task_dependency::ALL;
    first_id(conn, D.id, &Pred::eq(D.task_id, task_id).and(Pred::eq(D.blocked_by_id, blocked_by_id)))
}


/// The projects on the far side of every edge this task has — its dependencies (in both directions), the
/// decisions linked to it, and the axes it is classified on. `None` is a peer sitting in the inbox (no
/// project of its own). This is what [`crate::ops::task::move_to`] asks before re-homing a task: an edge
/// that was legal when it was drawn (both ends in one project) becomes a project-crossing one the moment
/// one end moves, so the no-crossing invariant has to be re-checked against the destination rather than
/// only at creation. Costs the task's own edges, never the tables.
pub fn edge_peer_projects(conn: &Connection, task_id: i64) -> Result<Vec<Option<i64>>> {
    const D: col::task_dependency::Cols = col::task_dependency::of("d");
    const T: col::task::Cols = col::task::of("t");
    const L: col::decision_task_link::Cols = col::decision_task_link::of("l");
    const K: col::decision::Cols = col::decision::of("k");
    const V: col::task_dimension_value::Cols = col::task_dimension_value::of("v");
    const M: col::dimension::Cols = col::dimension::of("m");

    // Four arms, one column: the project the peer sits in. Each arm builds its own projection, and the
    // slots they hand back are the same type — which is the layer holding the arms to one row shape.
    let (project, sql) = Union::all(|sel| {
        let project = sel.col(T.project_id);
        let mut tail = Sql::from_table(D.table);
        tail.join(T.table, same(T.id, D.blocked_by_id))
            .push_where(Some(&Pred::eq(D.task_id, task_id)));
        (project, tail)
    })
    .arm(|sel| {
        let project = sel.col(T.project_id);
        let mut tail = Sql::from_table(D.table);
        tail.join(T.table, same(T.id, D.task_id))
            .push_where(Some(&Pred::eq(D.blocked_by_id, task_id)));
        (project, tail)
    })
    .arm(|sel| {
        // A decision always sits in a project; a task in the inbox does not — and one union has one row
        // shape, so the arm that cannot be absent is the one that widens.
        let project = sel.col(K.project_id.nullable());
        let mut tail = Sql::from_table(L.table);
        tail.join(K.table, same(K.id, L.decision_id))
            .push_where(Some(&Pred::eq(L.task_id, task_id)));
        (project, tail)
    })
    .arm(|sel| {
        // The axis a classification puts the task on. An assignment names a value by its axis, and reading
        // that axis is reading another project's vocabulary — so it is a peer like any other. The
        // assignment carries the axis id itself, so the value table need not be walked. An axis always
        // sits in a project, and widens like the decision arm.
        let project = sel.col(M.project_id.nullable());
        let mut tail = Sql::from_table(V.table);
        tail.join(M.table, same(M.id, V.dimension_id))
            .push_where(Some(&Pred::eq(V.task_id, task_id)));
        (project, tail)
    })
    .into_parts();

    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| project.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<Option<i64>>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// The live tasks of one project — part of the subtree [`crate::ops::project::delete`] deletes
/// child-first: deleting a project deletes its tasks rather than orphaning them, and `task.project_id`
/// is `RESTRICT`, so the tasks must go first or the delete fails.
pub fn task_ids_in_project(conn: &Connection, project_id: i64) -> Result<Vec<i64>> {
    const TA: col::task::Cols = col::task::ALL;
    select_ids(conn, TA.id, Some(&Pred::eq(TA.project_id, project_id)))
}

/// The live decisions of one project — the rest of that subtree (`decision.project_id` is `RESTRICT`
/// too, so `project delete` deletes them itself; each takes its own comments, edges and links along).
pub fn decision_ids_in_project(conn: &Connection, project_id: i64) -> Result<Vec<i64>> {
    const D: col::decision::Cols = col::decision::ALL;
    select_ids(conn, D.id, Some(&Pred::eq(D.project_id, project_id)))
}

/// The live dimensions of one project — likewise `RESTRICT`, and deleting one takes its values and the
/// task assignments on them.
pub fn dimension_ids_in_project(conn: &Connection, project_id: i64) -> Result<Vec<i64>> {
    select_ids(conn, DIM.id, Some(&Pred::eq(DIM.project_id, project_id)))
}

// ── The child sets a delete op sweeps itself (`AMB-D-403`) ──
// A row that stands for a concept is deleted by the op, never by a constraint: the op reads the
// children's ids and deletes them one by one, ahead of the parent. Reading the ids is the point —
// it is what puts the deletion in reach of the code, so what goes can be told and not merely happen.
// Each set is read **inside the deleting transaction**, or a child another writer added in between
// is missed.

/// The live comments of one task — deleted by the task's delete op, which also needs each id to sweep
/// the **attachments** hanging off that comment (polymorphic, so no constraint can carry them).
pub fn task_comment_ids(conn: &Connection, task_id: i64) -> Result<Vec<i64>> {
    const C: col::task_comment::Cols = col::task_comment::ALL;
    select_ids(conn, C.id, Some(&Pred::eq(C.task_id, task_id)))
}

/// The live comments of one decision — the `decision` twin of [`task_comment_ids`], for the same
/// delete and the same polymorphic-attachment sweep.
pub fn decision_comment_ids(conn: &Connection, decision_id: i64) -> Result<Vec<i64>> {
    const C: col::decision_comment::Cols = col::decision_comment::ALL;
    select_ids(conn, C.id, Some(&Pred::eq(C.decision_id, decision_id)))
}

/// The live dependency edges one task is an endpoint of — **both** ends. An edge names a blocker and
/// the task it blocks, and deleting either end leaves the edge pointing at nothing, so a task's delete
/// op takes the edges into it as well as the edges out of it.
pub fn task_dependency_ids(conn: &Connection, task_id: i64) -> Result<Vec<i64>> {
    const D: col::task_dependency::Cols = col::task_dependency::ALL;
    let pred = Pred::eq(D.task_id, task_id).or(Pred::eq(D.blocked_by_id, task_id));
    select_ids(conn, D.id, Some(&pred))
}

/// The live commit anchors of one task — the SHAs it records, deleted with it.
pub fn task_commit_ids(conn: &Connection, task_id: i64) -> Result<Vec<i64>> {
    const TC: col::task_commit::Cols = col::task_commit::ALL;
    select_ids(conn, TC.id, Some(&Pred::eq(TC.task_id, task_id)))
}

/// The live dimension assignments of one task — what the task is classified as, on every axis at once.
pub fn assignment_ids_of_task(conn: &Connection, task_id: i64) -> Result<Vec<i64>> {
    const TV: col::task_dimension_value::Cols = col::task_dimension_value::ALL;
    select_ids(conn, TV.id, Some(&Pred::eq(TV.task_id, task_id)))
}

/// The live assignments naming one dimension value — the tasks classified as it. Sweeping a dimension
/// value by value covers its axis as a whole: an assignment always names a value of the axis it names.
pub fn assignment_ids_of_value(conn: &Connection, value_id: i64) -> Result<Vec<i64>> {
    const TV: col::task_dimension_value::Cols = col::task_dimension_value::ALL;
    select_ids(conn, TV.id, Some(&Pred::eq(TV.value_id, value_id)))
}

/// The live decision⇄task links of one task — the decisions it rests on, from the task's side.
pub fn decision_task_link_ids_of_task(conn: &Connection, task_id: i64) -> Result<Vec<i64>> {
    const L: col::decision_task_link::Cols = col::decision_task_link::ALL;
    select_ids(conn, L.id, Some(&Pred::eq(L.task_id, task_id)))
}

/// The live decision⇄task links of one decision — the same edges from the decision's side.
pub fn decision_task_link_ids_of_decision(
    conn: &Connection,
    decision_id: i64,
) -> Result<Vec<i64>> {
    const L: col::decision_task_link::Cols = col::decision_task_link::ALL;
    select_ids(conn, L.id, Some(&Pred::eq(L.decision_id, decision_id)))
}

/// The live decision→decision edges one decision is an endpoint of — **both** ends, so the edges it
/// drew and the edges naming it go together and nothing is left pointing at a decision that is gone.
pub fn decision_edge_ids(conn: &Connection, decision_id: i64) -> Result<Vec<i64>> {
    const E: col::decision_edge::Cols = col::decision_edge::ALL;
    let pred =
        Pred::eq(E.decision_id, decision_id).or(Pred::eq(E.target_decision_id, decision_id));
    select_ids(conn, E.id, Some(&pred))
}

/// The live `(decision, task)` link, or `None` — what makes `ops::decision::link` idempotent and
/// `unlink` a lookup. The `(decision_id, task_id)` twin of [`dependency_id`].
pub fn decision_task_link_id(
    conn: &Connection,
    decision_id: i64,
    task_id: i64,
) -> Result<Option<i64>> {
    const L: col::decision_task_link::Cols = col::decision_task_link::ALL;
    first_id(conn, L.id, &Pred::eq(L.decision_id, decision_id).and(Pred::eq(L.task_id, task_id)))
}

/// The live tasks that depend directly on `blocker_id` and are **now ready** — the unblock signal the CLI
/// emits after marking a blocker done. Ready is [`reserve_blockers`] being empty, the same predicate the
/// reserve guard uses, so the signal and the guard cannot drift apart. This must be read **after** the
/// blocker's `done` has committed: a dependent is only newly ready because that write landed.
pub fn newly_ready_by(conn: &Connection, blocker_id: i64) -> Result<Vec<i64>> {
    const D: col::task_dependency::Cols = col::task_dependency::of("d");
    let mut sel = Select::new();
    sel.distinct();
    let dependent = sel.col(D.task_id);
    // The task is joined to keep a dependent that is gone out of the signal, not for a column of its own.
    let mut sql = Sql::from(&sel, D.table);
    sql.join(T.table, same(T.id, D.task_id))
        .push_where(Some(&Pred::eq(D.blocked_by_id, blocker_id)))
        .order_by([Sort::by(D.task_id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let dependents = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| dependent.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(StoreEngineError::from)?;
    let mut ready = Vec::new();
    // The reference day the guard would judge against, taken once for the whole sweep so every dependent
    // is measured against the same "today".
    let today = crate::time::today();
    for task_id in dependents {
        if reserve_blockers(conn, task_id, today)?.is_empty() {
            ready.push(task_id);
        }
    }
    Ok(ready)
}


// ───────────────────────── single-record loaders ─────────────────────────
//
// One record, read by id, straight from the truth source. These are what a mutation needs before it
// writes: the `before` snapshot its field diff is computed against, and the record it is about to
// change. Reading `before` inside the operation's `BEGIN IMMEDIATE` transaction
// (`super::WriteTx::conn`) is what lets a command hold no lock of its own.
//
// A row exists ⇒ it is live, so none of these filters for liveness and none needs to: the delete is
// physical. What makes a `before` snapshot readable at all is the transaction, not a retained row —
// the operation reads the record before its own DELETE, and once that commits there is nothing left
// to read.

/// The `task` record with this id. `None` when no such row exists.
pub fn task(conn: &Connection, id: i64) -> Result<Option<crate::model::Task>> {
    super::hydrate::row_by_id(conn, "task", id, super::hydrate::task_row)
}

/// The `project` record with this id.
pub fn project(conn: &Connection, id: i64) -> Result<Option<crate::model::Project>> {
    super::hydrate::row_by_id(conn, "project", id, super::hydrate::project_row)
}

/// The `decision` record with this id.
pub fn decision(conn: &Connection, id: i64) -> Result<Option<crate::model::Decision>> {
    super::hydrate::row_by_id(conn, "decision", id, super::hydrate::decision_row)
}

/// The `dimension` record with this id.
pub fn dimension(conn: &Connection, id: i64) -> Result<Option<crate::model::Dimension>> {
    super::hydrate::row_by_id(conn, "dimension", id, super::hydrate::dimension_row)
}

/// The `dimension_value` record with this id.
pub fn dimension_value(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::model::DimensionValue>> {
    super::hydrate::row_by_id(conn, "dimension_value", id, super::hydrate::dimension_value_row)
}

/// The `task_dimension_value` assignment with this id.
pub fn task_dimension_value(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::model::TaskDimensionValue>> {
    super::hydrate::row_by_id(
        conn,
        "task_dimension_value",
        id,
        super::hydrate::task_dimension_value_row,
    )
}

/// The `task_dependency` edge with this id.
pub fn task_dependency(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::model::TaskDependency>> {
    super::hydrate::row_by_id(conn, "task_dependency", id, super::hydrate::task_dependency_row)
}

/// The live `task_commit` row for `(task_id, sha)`, or `None` — what makes `ops::commit::add`
/// idempotent and `remove` a lookup. The `sha` is expected already normalised (lower-case, full
/// length); the UNIQUE index `task_commit_task_sha` guarantees at most one row.
pub fn task_commit_id(conn: &Connection, task_id: i64, sha: &str) -> Result<Option<i64>> {
    const C: col::task_commit::Cols = col::task_commit::ALL;
    first_id(conn, C.id, &Pred::eq(C.task_id, task_id).and(Pred::eq(C.sha, sha)))
}

/// The `task_commit` with this id.
pub fn task_commit(conn: &Connection, id: i64) -> Result<Option<crate::model::TaskCommit>> {
    super::hydrate::row_by_id(conn, "task_commit", id, super::hydrate::task_commit_row)
}

/// The commit SHAs recorded on one task, **oldest first** (`created_at ASC, id ASC` — ids are handed
/// out in creation order, so the id tiebreak agrees with `created_at`). Seeks the task's own rows via
/// the `task_commit_by_task` index. Whole records, so the mapping is `hydrate::task_commit_row` (one
/// row→model definition shared with the by-id load and the reverse projection) over a `SELECT *` — the
/// same raw shape [`super::hydrate::rows`] uses for the same reason: the mapping reads each column by
/// name, so the column list is not enumerated here.
pub fn task_commits(conn: &Connection, task_id: i64) -> Result<Vec<crate::model::TaskCommit>> {
    let mut stmt = conn
        .prepare("SELECT * FROM task_commit WHERE task_id = ?1 ORDER BY created_at, id")
        .map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map([task_id], super::hydrate::task_commit_row)
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// The live `plugin_config` row id for `(project_id, plugin, field_key)`, or `None` — what makes
/// the config write boundary an upsert (find-then-update) and a clear a lookup. The `plugin_config_triple`
/// UNIQUE index guarantees at most one row.
pub fn plugin_config_row_id(
    conn: &Connection,
    project_id: i64,
    plugin: &str,
    field_key: &str,
) -> Result<Option<i64>> {
    const C: col::plugin_config::Cols = col::plugin_config::ALL;
    first_id(
        conn,
        C.id,
        &Pred::eq(C.project_id, project_id)
            .and(Pred::eq(C.plugin, plugin))
            .and(Pred::eq(C.field_key, field_key)),
    )
}

/// Every live `plugin_config` row belonging to one plugin, **across every project** — what an
/// `uninstall` erases in one pass (`AMB-D-357`). The store is a single device-wide database, so a plugin's
/// per-project settings are one predicate away rather than a walk over projects.
pub fn plugin_config_row_ids(conn: &Connection, plugin: &str) -> Result<Vec<i64>> {
    const C: col::plugin_config::Cols = col::plugin_config::ALL;
    select_ids(conn, C.id, Some(&Pred::eq(C.plugin, plugin)))
}

/// The `plugin_config` row with this id.
pub fn plugin_config_row_by_id(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::model::PluginConfigValue>> {
    super::hydrate::row_by_id(conn, "plugin_config", id, super::hydrate::plugin_config_row)
}

/// One plugin text field's value in this project, or `None` when it is unset (`AMB-D-434`).
pub fn plugin_config_value(
    conn: &Connection,
    project_id: i64,
    plugin: &str,
    field_key: &str,
) -> Result<Option<String>> {
    match plugin_config_row_id(conn, project_id, plugin, field_key)? {
        Some(id) => Ok(plugin_config_row_by_id(conn, id)?.map(|r| r.value)),
        None => Ok(None),
    }
}

/// The `plugin_secret` twins of the three above — same address, same upsert-and-clear shape; the table is
/// separate so `export` can leave it out wholesale (`AMB-D-434`).
pub fn plugin_secret_row_id(
    conn: &Connection,
    project_id: i64,
    plugin: &str,
    field_key: &str,
) -> Result<Option<i64>> {
    const C: col::plugin_secret::Cols = col::plugin_secret::ALL;
    first_id(
        conn,
        C.id,
        &Pred::eq(C.project_id, project_id)
            .and(Pred::eq(C.plugin, plugin))
            .and(Pred::eq(C.field_key, field_key)),
    )
}

/// Every live `plugin_secret` row belonging to one plugin, across every project — what an `uninstall`
/// purges (`AMB-D-357`: a secret is the one thing that must never survive a removal).
pub fn plugin_secret_row_ids(conn: &Connection, plugin: &str) -> Result<Vec<i64>> {
    const C: col::plugin_secret::Cols = col::plugin_secret::ALL;
    select_ids(conn, C.id, Some(&Pred::eq(C.plugin, plugin)))
}

/// The `plugin_secret` row with this id.
pub fn plugin_secret_row_by_id(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::model::PluginSecret>> {
    super::hydrate::row_by_id(conn, "plugin_secret", id, super::hydrate::plugin_secret_row)
}

/// One plugin secret field's value in this project, or `None` when it is unset (`AMB-D-434`).
pub fn plugin_secret_value(
    conn: &Connection,
    project_id: i64,
    plugin: &str,
    field_key: &str,
) -> Result<Option<String>> {
    match plugin_secret_row_id(conn, project_id, plugin, field_key)? {
        Some(id) => Ok(plugin_secret_row_by_id(conn, id)?.map(|r| r.value)),
        None => Ok(None),
    }
}

/// The live `plugin_enable` row id for `(project_id, plugin)`, or `None` — the lookup behind both writes,
/// since enabling is "ensure the row" and disabling is "delete it" (`AMB-D-434`). The `plugin_enable_pair`
/// UNIQUE index guarantees at most one row.
pub fn plugin_enable_row_id(
    conn: &Connection,
    project_id: i64,
    plugin: &str,
) -> Result<Option<i64>> {
    const C: col::plugin_enable::Cols = col::plugin_enable::ALL;
    first_id(
        conn,
        C.id,
        &Pred::eq(C.project_id, project_id).and(Pred::eq(C.plugin, plugin)),
    )
}

/// Every live `plugin_enable` row belonging to one plugin, **across every project** — what an
/// `uninstall` erases in one pass (`AMB-D-357`), the gate twin of [`plugin_config_row_ids`].
pub fn plugin_enable_row_ids(conn: &Connection, plugin: &str) -> Result<Vec<i64>> {
    const C: col::plugin_enable::Cols = col::plugin_enable::ALL;
    select_ids(conn, C.id, Some(&Pred::eq(C.plugin, plugin)))
}

/// The `plugin_enable` row with this id.
pub fn plugin_enable_row_by_id(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::model::PluginEnabledProject>> {
    super::hydrate::row_by_id(conn, "plugin_enable", id, super::hydrate::plugin_enable_row)
}

/// Whether this project holds the gate open for one plugin — the row's presence, and nothing else
/// (`AMB-D-434`).
pub fn plugin_enabled_in_project(
    conn: &Connection,
    project_id: i64,
    plugin: &str,
) -> Result<bool> {
    Ok(plugin_enable_row_id(conn, project_id, plugin)?.is_some())
}

/// Both directions of one decision's edges, as the read surfaces render them. Forward (`supersedes` /
/// `amends` / `builds_on`) is what this decision drew, and the target is resolved for **any** liveness —
/// a title of `None` means the edge dangles, which the caller renders as the unknown-name placeholder.
/// Reverse
/// (`superseded_by` / `amended_by` / `built_on_by`) is derived by looking this decision up as a target,
/// and only live decisions count: the reverse view is a projection of the forward edges, never a stored
/// flag. Each list is in edge-`id` order — the order the edges were drawn.
#[derive(Default)]
pub struct DecisionEdges {
    pub supersedes: Vec<(i64, Option<String>)>,
    pub superseded_by: Vec<(i64, String)>,
    pub amends: Vec<(i64, Option<String>)>,
    pub amended_by: Vec<(i64, String)>,
    /// The decisions this one is built on. Each carries the successor that has *overturned* the premise,
    /// if one has — the caller surfaces a decision standing on a rotten premise with it.
    pub builds_on: Vec<Premise>,
    pub built_on_by: Vec<(i64, String)>,
}

/// One premise of a decision: the target of a `builds_on` edge, plus whether it still holds.
/// `superseded_by` is the first live decision holding a `supersedes` edge at the premise — `None` when
/// the premise is current, which is the ordinary case.
pub struct Premise {
    pub id: i64,
    pub title: Option<String>,
    pub superseded_by: Option<i64>,
}

/// Read one decision's edges in both directions. Two indexed seeks (`decision_edge_pair` forward,
/// `decision_edge_by_target` reverse), so a decision with no edges costs nothing.
pub fn decision_edges(conn: &Connection, decision_id: i64) -> Result<DecisionEdges> {
    const E: col::decision_edge::Cols = col::decision_edge::of("e");
    const TD: col::decision::Cols = col::decision::of("t");

    let mut out = DecisionEdges::default();
    {
        let mut sel = Select::new();
        let (kind, target) = (sel.col(E.kind), sel.col(E.target_decision_id));
        // The target's title comes through a `LEFT JOIN`, so its `NOT NULL` column reads back optional
        // (`Col::nullable`); the successor is a correlated subquery (`premise_successor`), which the
        // registry cannot type at all.
        let title = sel.col(TD.title.nullable());
        let successor = sel.expr::<Option<i64>>(premise_successor(E));

        let mut sql = Sql::from(&sel, E.table);
        sql.left_join(TD.table, same(TD.id, E.target_decision_id))
            .push_where(Some(&Pred::eq(E.decision_id, decision_id)))
            .order_by([Sort::by(E.id)]);

        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| {
                Ok((kind.get(r)?, target.get(r)?, title.get(r)?, successor.get(r)?))
            })
            .map_err(StoreEngineError::from)?
            .collect::<rusqlite::Result<Vec<(String, i64, Option<String>, Option<i64>)>>>()
            .map_err(StoreEngineError::from)?;
        for (kind, id, title, successor) in rows {
            match crate::model::DecisionEdgeKind::parse(&kind) {
                Some(crate::model::DecisionEdgeKind::Supersedes) => out.supersedes.push((id, title)),
                Some(crate::model::DecisionEdgeKind::Amends) => out.amends.push((id, title)),
                Some(crate::model::DecisionEdgeKind::BuildsOn) => {
                    out.builds_on.push(Premise { id, title, superseded_by: successor })
                }
                None => continue,
            }
        }
    }
    for edge in decision_reverse_edges(conn, decision_id)? {
        match edge.kind {
            crate::model::DecisionEdgeKind::Supersedes => {
                out.superseded_by.push((edge.id, edge.title))
            }
            crate::model::DecisionEdgeKind::Amends => out.amended_by.push((edge.id, edge.title)),
            crate::model::DecisionEdgeKind::BuildsOn => out.built_on_by.push((edge.id, edge.title)),
        }
    }
    Ok(out)
}

/// A live decision pointing at the one asked about, and how.
pub struct ReverseEdge {
    pub id: i64,
    pub title: String,
    pub kind: crate::model::DecisionEdgeKind,
}

/// Every live decision holding an edge at this one, of **all three kinds** — the **impact radius**: what
/// a `supersede` / `reject` / `delete` of this decision puts up for review. One indexed seek
/// (`decision_edge_by_target`), in the order the edges were drawn. Direct edges only — 1 hop, never
/// chased, because nothing here changes another decision's currency; it only tells a human/AI what to
/// re-examine. All three kinds count: a decision that superseded or amended this one stands on it just as
/// a `builds_on` does.
pub fn decision_reverse_edges(conn: &Connection, decision_id: i64) -> Result<Vec<ReverseEdge>> {
    const E: col::decision_edge::Cols = col::decision_edge::of("e");
    const O: col::decision::Cols = col::decision::of("o");
    let mut sel = Select::new();
    let (kind, id, title) = (sel.col(E.kind), sel.col(O.id), sel.col(O.title));
    let mut sql = Sql::from(&sel, E.table);
    sql.join(O.table, same(O.id, E.decision_id))
        .push_where(Some(&Pred::eq(E.target_decision_id, decision_id)))
        .order_by([Sort::by(E.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok((kind.get(r)?, id.get(r)?, title.get(r)?))
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(String, i64, String)>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows
        .into_iter()
        .filter_map(|(kind, id, title)| {
            crate::model::DecisionEdgeKind::parse(&kind).map(|kind| ReverseEdge { id, title, kind })
        })
        .collect())
}

/// The `decision_edge` with this id.
pub fn decision_edge(conn: &Connection, id: i64) -> Result<Option<crate::model::DecisionEdge>> {
    super::hydrate::row_by_id(conn, "decision_edge", id, super::hydrate::decision_edge_row)
}

/// The live edge between this ordered pair of decisions, or `None`. At most one exists
/// (`decision_edge_pair` is UNIQUE over the live rows), which is what lets `supersede`/`amend` re-draw
/// an existing edge instead of stacking a contradicting second one on the same pair.
pub fn decision_edge_id(
    conn: &Connection,
    decision_id: i64,
    target_decision_id: i64,
) -> Result<Option<i64>> {
    const E: col::decision_edge::Cols = col::decision_edge::ALL;
    let pred = Pred::eq(E.decision_id, decision_id)
        .and(Pred::eq(E.target_decision_id, target_decision_id));
    first_id(conn, E.id, &pred)
}


/// The `decision_task_link` with this id.
pub fn decision_task_link(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::model::DecisionTaskLink>> {
    super::hydrate::row_by_id(
        conn,
        "decision_task_link",
        id,
        super::hydrate::decision_task_link_row,
    )
}

/// The `task_comment` with this id.
pub fn task_comment(conn: &Connection, id: i64) -> Result<Option<crate::model::TaskComment>> {
    super::hydrate::row_by_id(conn, "task_comment", id, super::hydrate::task_comment_row)
}

/// The `decision_comment` with this id.
pub fn decision_comment(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::model::DecisionComment>> {
    super::hydrate::row_by_id(conn, "decision_comment", id, super::hydrate::decision_comment_row)
}

/// The `attachment` with this id.
pub fn attachment(conn: &Connection, id: i64) -> Result<Option<crate::model::Attachment>> {
    super::hydrate::row_by_id(conn, "attachment", id, super::hydrate::attachment_row)
}

/// Live attachment ids on one target (task/decision), in attach order (`order_key`, then `id`) —
/// the id list behind the CLI's `attach ls`. [`attachments_for_target`] answers the same question
/// with the GUI's projection row; this one hands back ids so the caller can load whole records.
pub fn live_attachment_ids_for_target(
    conn: &Connection,
    target_type: &str,
    target_id: i64,
) -> Result<Vec<i64>> {
    let mut sel = Select::new();
    let id = sel.col(ATT.id);
    let mut sql = Sql::from(&sel, ATT.table);
    sql.push_where(Some(&on_target(target_type, target_id)))
        .order_by([Sort::by(ATT.order_key), Sort::by(ATT.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| id.get(r))
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(StoreEngineError::from)?;
    Ok(ids)
}

// ───────────────────────── whole-table loaders ─────────────────────────
//
/// Every `task_comment` of the store, `id` ascending — a whole read-model table as
/// model records. What `ops::task`'s delete asserts against: that removing a task leaves no orphaned
/// comment.
pub fn all_task_comments(conn: &Connection) -> Result<Vec<crate::model::TaskComment>> {
    super::hydrate::rows(conn, "task_comment", super::hydrate::task_comment_row)
}

/// One row of the project index (`project list`), carrying the two counts the listing shows.
pub struct ProjectListRow {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    /// `default_view` as stored (`list`/`board`/`calendar`/`timeline`).
    pub default_view: String,
    pub archived: bool,
    pub order_key: String,
    /// Live classification axes declared on this project.
    pub num_dimensions: usize,
    /// Live tasks placed in this project (placement is task-held).
    pub num_tasks: usize,
}

/// The store's live projects in `order_key` order, with their dimension and task counts folded in by
/// SQL (a correlated subquery each, not a pass per project). `include_archived` widens the set to
/// archived projects. Ties on `order_key` break on `id`.
pub fn project_list(conn: &Connection, include_archived: bool) -> Result<Vec<ProjectListRow>> {
    const P: col::project::Cols = col::project::of("p");
    let mut sel = Select::new();
    let (id, name, color) = (sel.col(P.id), sel.col(P.name), sel.col(P.color));
    let default_view = sel.col(P.default_view);
    // `COALESCE`d: a row that carries no value for the column reads as NULL.
    let archived_flag = sel.expr::<bool>(format!("COALESCE({}, 0)", P.archived.to_sql()));
    let order_key = sel.col(P.order_key);
    const DI: col::dimension::Cols = col::dimension::of("d");
    let num_dimensions = sel.count_of(Count::over(DI.table).filter(same(DI.project_id, P.id)));
    // `task.project_id` is nullable — an unplaced task is in no project — so the correlation is the
    // equality, which no `NULL` satisfies: the inbox is counted into nobody's project.
    let num_tasks = sel.count_of(Count::over(T.table).filter(same(T.project_id, P.id)));
    // Widening the set is dropping the filter, not binding a flag into it: an unfiltered read carries no
    // `WHERE` at all.
    let pred = (!include_archived).then(|| not_archived(P));
    let mut sql = Sql::from(&sel, P.table);
    sql.push_where(pred.as_ref())
        .order_by([Sort::by(P.order_key), Sort::by(P.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(ProjectListRow {
                id: id.get(r)?,
                name: name.get(r)?,
                color: color.get(r)?,
                default_view: default_view.get(r)?,
                archived: archived_flag.get(r)?,
                order_key: order_key.get(r)?,
                num_dimensions: num_dimensions.get(r)? as usize,
                num_tasks: num_tasks.get(r)? as usize,
            })
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<ProjectListRow>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

/// The task-count summary of one project (`project show`), all six buckets in a single aggregate.
pub struct ProjectTaskCountsRow {
    pub total: usize,
    pub completed: usize,
    pub overdue: usize,
    pub due_today: usize,
    pub no_due: usize,
}

/// Count one project's live tasks by completion and due date, in one aggregate pass. `today` is the
/// caller's clock, so every reader buckets `overdue` / `due_today` against the same date.
///
/// `completed` is what was **carried out** (`Done` alone), while every due bucket asks whether the task is
/// still work ([`still_open`], which drops both terminals). Keeping the two apart is the whole of
/// `AMB-D-397` on this surface: a task decided against is in neither. An empty
/// `due_on` is no due date, not a date that sorts before every other — [`status_bucket_ids`] guards it
/// the same way.
pub fn project_task_counts(
    conn: &Connection,
    project_id: i64,
    today: NaiveDate,
) -> Result<ProjectTaskCountsRow> {
    const T: col::task::Cols = col::task::of("t");
    // Each bucket is a condition *inside* the aggregate, and a condition carries its value: `today` binds
    // where it is compared (`Select::count_if`), like it would in a `WHERE`. Nothing about the day is
    // spliced into the SQL text, and no one-row derived table has to smuggle it in.
    let today = today.to_string();
    let open = || still_open(T.status);
    let has_due = || Pred::is_not_null(T.due_on).and(Pred::ne(T.due_on, ""));

    let mut sel = Select::new();
    let total = sel.count_all();
    let completed = sel.count_if(Pred::eq(T.status, "done"));
    let overdue =
        sel.count_if(open().and(has_due()).and(Pred::cmp(T.due_on, "<", today.as_str())));
    let due_today = sel.count_if(open().and(Pred::eq(T.due_on, today.as_str())));
    let no_due = sel.count_if(open().and(Pred::is_blank(T.due_on)));

    let mut sql = Sql::from(&sel, T.table);
    sql.push_where(Some(&Pred::eq(T.project_id, project_id)));
    conn.query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| {
        Ok(ProjectTaskCountsRow {
            total: total.get(r)? as usize,
            completed: completed.get(r)? as usize,
            overdue: overdue.get(r)? as usize,
            due_today: due_today.get(r)? as usize,
            no_due: no_due.get(r)? as usize,
        })
    })
    .map_err(StoreEngineError::from)
}

/// Every live task's id and title, `id` ascending — the whole input of [`crate::validate::validate`],
/// which is a per-task field check with no joins. `project` narrows the input to one project (what a
/// closed reach may see), `None` takes them all.
pub fn live_task_titles(conn: &Connection, project: Option<i64>) -> Result<Vec<(String, String)>> {
    const TA: col::task::Cols = col::task::ALL;
    let mut sel = Select::new();
    let (id, title) = (sel.col(TA.id), sel.col(TA.title));
    // Narrowing the input is adding the filter, not binding a project into a tautology: taking them all
    // carries no `WHERE` at all, rather than a `?1 IS NULL OR …` that binds the same value twice.
    let pred = project.map(|p| Pred::eq(TA.project_id, p));
    let mut sql = Sql::from(&sel, TA.table);
    sql.push_where(pred.as_ref()).order_by([Sort::by(TA.id)]);
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok((id.get(r)?.to_string(), title.get(r)?))
        })
        .map_err(StoreEngineError::from)?
        .collect::<rusqlite::Result<Vec<(String, String)>>>()
        .map_err(StoreEngineError::from)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::StoreEngine;
    use rusqlite::types::Value;

    /// A field value for a record these tests write.
    fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }

    /// What a watcher leans on: `data_version` moves when **another** connection commits — including a
    /// commit that lands only in the WAL, where the main file's mtime never moves — and stays put for
    /// the connection's own writes, which is what lets the GUI tell an external write apart from its own.
    #[test]
    fn data_version_moves_for_another_connections_commit_and_not_for_ones_own() {
        let dir = amenbo_scratch::scratch("dataversion");
        let path = dir.join("store.sqlite");

        let watcher = StoreEngine::open(&path).unwrap();
        let writer = StoreEngine::open(&path).unwrap();
        let seen = data_version(watcher.conn()).unwrap();

        // The watcher's own write is not news to the watcher.
        let tx = watcher.write().unwrap();
        tx.put_record("task", 1, &[("title", text("自分の書き込み"))]).unwrap();
        tx.commit().unwrap();
        assert_eq!(data_version(watcher.conn()).unwrap(), seen, "one's own commit is not a change to detect");

        // Another connection's — the CLI, the AI, another window — is.
        let tx = writer.write().unwrap();
        tx.put_record("task", 2, &[("title", text("外からの書き込み"))]).unwrap();
        tx.commit().unwrap();
        assert_ne!(
            data_version(watcher.conn()).unwrap(),
            seen,
            "an external commit moves it, WAL or not"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `task_title` resolves a live task's projected title and yields `None` for an unknown id.
    #[test]
    fn task_title_resolves_live_task_and_none_for_unknown() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("task", 1, &[("title", text("Buy milk"))]).unwrap();
        let conn = e.conn();
        assert_eq!(task_title(conn, 1).unwrap().as_deref(), Some("Buy milk"));
        assert_eq!(task_title(conn, 999).unwrap(), None);
    }

    /// `decision_title` resolves a decision's projected title (None for unknown), and
    /// `decision_comment_list` returns its comments oldest-first, without other decisions' comments.
    #[test]
    fn decision_title_and_comment_list_from_read_model() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("decision", 1, &[("title", text("RDB を真実源にする"))]).unwrap();
        e.put_record("decision", 2, &[("title", text("別の決定"))]).unwrap();
        let comment = |id: i64, decision_id: &str, body: &str, at: &str| {
            let cols = vec![
                ("decision_id", text(decision_id)),
                ("author_kind", text("ai")),
                ("text", text(body)),
                ("created_at", text(at)),
            ];
            e.put_record("decision_comment", id, &cols).unwrap();
        };
        // d1: two comments, inserted out of order; d2: one comment (must not leak).
        comment(2, "1", "二番目", "2026-07-02T00:00:00Z");
        comment(1, "1", "一番目", "2026-07-01T00:00:00Z");
        comment(4, "2", "別決定のコメント", "2026-07-01T00:00:00Z");

        let conn = e.conn();
        assert_eq!(decision_title(conn, 1).unwrap().as_deref(), Some("RDB を真実源にする"));
        assert_eq!(decision_title(conn, 999).unwrap(), None);

        let rows = decision_comment_list(conn, 1).unwrap();
        let texts: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, vec!["一番目", "二番目"], "oldest-first, no d2 leak");
    }

    /// Mailbox D: `mailbox_comment_tasks` returns only live, still-outstanding tasks assigned to my human facet
    /// that carry an *incoming* comment (my own human-facet comment does not count), bundling just those
    /// incoming comments. A task the AI is carrying stays out however loud its AI comments are — that is
    /// the AI reporting on its own work, not a move being asked of me. (The other half of the trigger —
    /// the newest `task.assigned` event — is read off the file ledger, so it is tested there:
    /// [`crate::activity::mailbox_triggered_at`].)
    #[test]
    fn mailbox_d_from_read_model() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        let task = |id: i64, status: &str, assignee_kind: Option<&str>| {
            let mut cols = vec![("title", text(&id.to_string())), ("status", text(status))];
            if let Some(k) = assignee_kind {
                cols.push(("assignee_kind", text(k)));
            }
            e.put_record("task", id, &cols).unwrap();
        };
        let comment = |id: i64, task_id: &str, kind: &str, at: &str| {
            e.put_record(
                "task_comment",
                id,
                &[
                    ("task_id", text(task_id)),
                    ("author_kind", text(kind)),
                    ("created_at", text(at)),
                ],
            )
            .unwrap();
        };

        task(1, "todo", Some("human")); // assigned, not done, has an incoming (AI) comment → in the inbox
        task(2, "done", Some("human")); // assigned but done → out
        task(3, "todo", Some("human")); // assigned but only a human comment → nothing incoming → out
        task(4, "todo", None); // unassigned → out (incoming or not, an unassigned task never enters)
        task(5, "todo", Some("ai")); // the AI's own task → out, its comments are a report, not a move for me
        comment(1, "1", "human", "2026-06-01T00:00:00Z"); // one's own human comment (not incoming)
        comment(2, "1", "ai", "2026-06-02T00:00:00Z"); // an AI-facet comment (incoming)
        comment(3, "3", "human", "2026-06-01T00:00:00Z"); // not incoming
        comment(4, "4", "ai", "2026-06-01T00:00:00Z"); // incoming, but task 4 is unassigned
        comment(5, "5", "ai", "2026-06-01T00:00:00Z"); // AI comment on an AI-assigned task → not addressed to me

        let conn = e.conn();
        let mb = mailbox_comment_tasks(conn, crate::reach::Reach::All).unwrap();
        assert_eq!(mb.len(), 1, "only task 1 qualifies");
        assert_eq!(mb[0].task_id, 1);
        assert_eq!(
            mb[0].comments,
            vec![("ai".to_string(), false, "2026-06-02T00:00:00Z".to_string())],
            "only the incoming AI-facet comment is bundled (is_human=false); my human comment is filtered"
        );
    }

    /// `project_overview` carries per-project open-task counts — live tasks that have not ended
    /// (todo/in_progress/blocked count), grouped by the task's own `project_id`. **Both** terminals are
    /// excluded, `rejected` as much as `done` (`AMB-D-397`): a badge counting work decided against would
    /// show a project as busy over tasks nobody is going to do. Deleted tasks and tasks with no project
    /// are excluded too, and a project with no open tasks reports 0.
    #[test]
    fn project_overview_open_counts() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("project", 1, &[("name", text("Alpha")), ("order_key", text("a"))]).unwrap();
        e.put_record("project", 2, &[("name", text("Beta")), ("order_key", text("b"))]).unwrap();
        e.put_record("project", 3, &[("name", text("Gamma")), ("order_key", text("c"))]).unwrap();
        let task = |id: i64, project_id: Option<&str>, status: &str| {
            let mut cols = vec![("title", text(&id.to_string())), ("status", text(status))];
            if let Some(pid) = project_id {
                cols.push(("project_id", text(pid)));
            }
            e.put_record("task", id, &cols).unwrap();
        };
        // project 1: todo + in_progress open; a done and a rejected one are both excluded => 2.
        task(1, Some("1"), "todo");
        task(2, Some("1"), "in_progress");
        task(3, Some("1"), "done");
        task(4, Some("1"), "rejected");
        // project 2: a single blocked task still counts as open => 1.
        task(5, Some("2"), "blocked");
        // project 3: only a done task => 0 (badge hidden). A project-less task must not leak into any count.
        task(6, Some("3"), "done");
        task(7, None, "todo");

        let conn = e.conn();
        let by: HashMap<i64, usize> =
            project_overview(conn, crate::reach::Reach::All)
                .unwrap()
                .into_iter()
                .map(|r| (r.id, r.open_count))
                .collect();
        assert_eq!(by.get(&1).copied(), Some(2), "todo+in_progress open; done and rejected both excluded");
        assert_eq!(by.get(&2).copied(), Some(1), "blocked counts as open");
        assert_eq!(by.get(&3).copied(), Some(0), "only a done task => 0");
    }

    /// `project_task_counts` keeps the **two readings of an ended task apart** (`AMB-D-397`).
    /// `completed` counts what was carried out — `done` alone — while every bucket built on "still work"
    /// (overdue / due today / no due day) drops both terminals. So a task decided against belongs to
    /// neither: counting it as completed would claim work that never happened, and leaving it in the due
    /// buckets would keep nagging about a day nobody is going to meet.
    #[test]
    fn project_task_counts_keep_carried_out_and_closed_apart() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("project", 1, &[("name", text("Alpha")), ("order_key", text("a"))]).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        // The day column admits a day or nothing at all, so "no due day" is the column left unwritten.
        let task = |id: i64, status: &str, due: Option<&str>| {
            let mut cols = vec![
                ("title", text(&id.to_string())),
                ("status", text(status)),
                ("project_id", text("1")),
            ];
            if let Some(day) = due {
                cols.push(("due_on", text(day)));
            }
            e.put_record("task", id, &cols).unwrap();
        };
        task(1, "todo", Some("2026-07-01")); // overdue, and still work
        task(2, "done", Some("2026-07-01")); // carried out
        task(3, "rejected", Some("2026-07-01")); // over, and never carried out
        task(4, "rejected", None); // …with no day on it either
        task(5, "todo", None);

        let c = project_task_counts(e.conn(), 1, today).unwrap();

        assert_eq!(c.total, 5, "every live task of the project is in the total, however it ended");
        assert_eq!(c.completed, 1, "only the one that was carried out");
        assert_eq!(c.overdue, 1, "the rejected task stops being late the moment it is over");
        assert_eq!(c.no_due, 1, "…and stops being an undated piece of work too");
    }

    /// `project_overview` carries per-project proposed (under-discussion) decision counts: `proposed`
    /// decisions grouped by their `project_id`. accepted/rejected are excluded by status, and a proposed
    /// decision that a `supersedes` edge points at is excluded as no longer current. A project with none
    /// reports 0.
    #[test]
    fn project_overview_proposed_decision_counts() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("project", 1, &[("name", text("Alpha")), ("order_key", text("a"))]).unwrap();
        e.put_record("project", 2, &[("name", text("Beta")), ("order_key", text("b"))]).unwrap();
        e.put_record("project", 3, &[("name", text("Gamma")), ("order_key", text("c"))]).unwrap();
        let decision = |id: i64, project_id: &str, status: &str| {
            let cols = vec![
                ("project_id", text(project_id)),
                ("title", text(&id.to_string())),
                ("body", text("b")),
                ("status", text(status)),
            ];
            e.put_record("decision", id, &cols).unwrap();
        };
        // project 1: two proposed, plus an accepted and a rejected that must not count => 2.
        decision(1, "1", "proposed");
        decision(2, "1", "proposed");
        decision(3, "1", "accepted");
        decision(4, "1", "rejected");
        // project 2: one proposed, but it is superseded by decision 6 => 0 (no longer under discussion).
        decision(5, "2", "proposed");
        decision(6, "2", "accepted");
        e.put_record(
            "decision_edge",
            1,
            &[("decision_id", text("6")), ("target_decision_id", text("5")), ("kind", text("supersedes"))],
        )
        .unwrap();
        // project 3: only an accepted decision => 0.
        decision(7, "3", "accepted");

        let conn = e.conn();
        let by: HashMap<i64, usize> = project_overview(conn, crate::reach::Reach::All)
            .unwrap()
            .into_iter()
            .map(|r| (r.id, r.proposed_decision_count))
            .collect();
        assert_eq!(by.get(&1).copied(), Some(2), "two proposed; accepted/rejected excluded");
        assert_eq!(by.get(&2).copied(), Some(0), "the one proposal is superseded => not current");
        assert_eq!(by.get(&3).copied(), Some(0), "no proposed decision => 0");
    }

    /// The dimension ref resolvers answer from the truth source — key or exact name, optionally scoped
    /// to one project. Ambiguity (a name two projects share) comes back as the whole hit set (ascending)
    /// for the caller's `pick_id` to reject.
    #[test]
    fn resolve_dimension_matches_key_or_name_within_a_project() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        let dim = |id: i64, project_id: &str, name: &str| {
            let cols = vec![("project_id", text(project_id)), ("name", text(name))];
            e.put_record("dimension", id, &cols).unwrap();
        };
        dim(11, "1", "カテゴリー");
        dim(12, "1", "領域");
        dim(21, "2", "カテゴリー"); // same name, other project

        let conn = e.conn();
        // The key resolves exactly — a leading digit of it names nothing (no prefix matching).
        assert_eq!(resolve_dimension_in(conn, None, "11").unwrap(), vec![11]);
        assert!(resolve_dimension_in(conn, None, "1").unwrap().is_empty());
        // Exact name, unscoped → both projects' axes; scoped → one.
        assert_eq!(resolve_dimension_in(conn, None, "カテゴリー").unwrap(), vec![11, 21]);
        assert_eq!(resolve_dimension_in(conn, Some(1), "カテゴリー").unwrap(), vec![11]);
        // A partial name is not a name match either — both arms are exact.
        assert!(resolve_dimension_in(conn, None, "カテゴ").unwrap().is_empty());
        // A deleted axis has no row, so nothing resolves to it.
        assert!(resolve_dimension_in(conn, None, "13").unwrap().is_empty());
    }

    /// A reference that is not a key resolves to nothing — never to the whole table, and never to a row
    /// whose key merely *starts* with it. Both halves matter: `""` must not hand `pick_id` the only row
    /// of a one-row store (which reads as "resolved" and lets `amenbo task done ""` write it), and `7`
    /// must not reach `77`, keys being decimal — a leading digit is a different number, not an
    /// abbreviation of one.
    #[test]
    fn a_non_key_reference_matches_nothing_not_everything() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("project", 77, &[("name", text("唯一のPJ"))]).unwrap();
        e.put_record("dimension", 77, &[("project_id", text("77")), ("name", text("唯一の軸"))]).unwrap();
        e.put_record("task_comment", 77, &[("text", text("唯一のコメント"))]).unwrap();

        let conn = e.conn();
        for r in ["", " ", "\t", "7"] {
            assert!(resolve_project(conn, r).unwrap().is_empty(), "project: {r:?}");
            assert!(resolve_dimension_in(conn, None, r).unwrap().is_empty(), "dimension: {r:?}");
            assert!(resolve_task_comment(conn, r).unwrap().is_empty(), "comment: {r:?}");
        }
        // The key itself does hit — which is the proof that the misses above are not just an empty table.
        assert_eq!(resolve_project(conn, "77").unwrap(), vec![77]);
        assert_eq!(resolve_dimension_in(conn, None, "77").unwrap(), vec![77]);
        assert_eq!(resolve_task_comment(conn, "77").unwrap(), vec![77]);
    }

    /// The value resolver is scoped to its axis, and `dimension_id_of_value` names that axis (a value
    /// that is not there has no axis to operate on).
    #[test]
    fn resolve_dimension_value_is_scoped_to_its_axis() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        let value = |id: i64, dimension_id: &str, name: &str| {
            let cols = vec![("dimension_id", text(dimension_id)), ("name", text(name))];
            e.put_record("dimension_value", id, &cols).unwrap();
        };
        value(1, "1", "バグ");
        value(2, "2", "バグ"); // same name on another axis

        let conn = e.conn();
        assert_eq!(resolve_dimension_value_in(conn, 1, "バグ").unwrap(), vec![1]);
        assert_eq!(resolve_dimension_value_in(conn, 1, "1").unwrap(), vec![1]);
        assert!(resolve_dimension_value_in(conn, 9, "バグ").unwrap().is_empty(), "no axis, no hit");

        assert_eq!(dimension_id_of_value(conn, 2).unwrap(), Some(2));
        assert_eq!(dimension_id_of_value(conn, 999).unwrap(), None, "a value that is not there has no axis");
    }

    /// The cascade sets a dimension delete fans out to (its values, its assignments), the single-select
    /// `(task, dimension)` scan `set` replaces, and the `(task, value)` lookup that makes `set` idempotent.
    #[test]
    fn dimension_cascade_and_assignment_scans() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("dimension_value", 1, &[("dimension_id", text("1")), ("name", text("A"))])
            .unwrap();
        let assign = |id: i64, task: &str, dim: &str, value: &str| {
            let cols =
                vec![("task_id", text(task)), ("dimension_id", text(dim)), ("value_id", text(value))];
            e.put_record("task_dimension_value", id, &cols).unwrap();
        };
        assign(1, "1", "1", "1");
        assign(2, "2", "1", "1");
        assign(3, "1", "2", "8"); // another axis on the same task

        let conn = e.conn();
        assert_eq!(dimension_value_ids(conn, 1).unwrap(), vec![1]);
        // The one-row invariant per (task, dimension).
        assert_eq!(assignment_ids_on_axis(conn, 1, 1).unwrap(), vec![1]);
        assert_eq!(assignment_ids_on_axis(conn, 1, 2).unwrap(), vec![3], "axes independent");
        assert_eq!(assignment_id(conn, 1, 1).unwrap(), Some(1));
        assert_eq!(assignment_id(conn, 1, 9).unwrap(), None, "an assignment that is not there");
    }

    /// The cycle guard: `dependency_reaches` walks the edges from a seed that is itself in the set and
    /// terminates on an already-cyclic graph. Plus the natural-key edge lookup. (The id set a task delete
    /// reads is [`task_dependency_ids`], covered with the rest of the delete's children in `ops`.)
    #[test]
    fn dependency_reachability_and_edge_sets() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        let edge = |id: i64, task: &str, blocker: &str| {
            let cols = vec![("task_id", text(task)), ("blocked_by_id", text(blocker))];
            e.put_record("dependency", id, &cols).unwrap();
        };
        // Tasks a..f are 1..6: 1 → 2 → 3, and an unrelated 5 → 6.
        edge(1, "1", "2");
        edge(2, "2", "3");
        edge(4, "5", "6");

        let conn = e.conn();
        assert!(dependency_reaches(conn, 1, 2).unwrap(), "direct");
        assert!(dependency_reaches(conn, 1, 3).unwrap(), "transitive");
        assert!(dependency_reaches(conn, 1, 1).unwrap(), "the seed reaches itself");
        assert!(!dependency_reaches(conn, 3, 1).unwrap(), "edges are directed");
        assert!(!dependency_reaches(conn, 1, 6).unwrap(), "disjoint component");

        // A cycle in the store must terminate, not loop.
        edge(5, "3", "1");
        assert!(dependency_reaches(conn, 1, 1).unwrap());
        assert!(dependency_reaches(conn, 2, 1).unwrap(), "2 → 3 → 1");

        assert_eq!(dependency_id(conn, 1, 2).unwrap(), Some(1));
        assert_eq!(dependency_id(conn, 2, 1).unwrap(), None, "direction matters");
        assert_eq!(dependency_id(conn, 3, 4).unwrap(), None, "an edge that is not there");
    }

    /// The top of the subtree `ops::project::delete` deletes child-first: a project's tasks, decisions
    /// and dimensions. Each of those then reads its own children (links, comments, values, assignments)
    /// through the id sets beside these, and deletes them in turn.
    #[test]
    fn project_subtree_sets() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        let task = |id: i64, project_id: Option<&str>| {
            let mut cols = vec![("title", text(&id.to_string()))];
            if let Some(pid) = project_id {
                cols.push(("project_id", text(pid)));
            }
            e.put_record("task", id, &cols).unwrap();
        };
        task(1, Some("1"));
        task(3, Some("2"));
        task(4, None);

        e.put_record("decision", 1, &[("project_id", text("1")), ("title", text("D1"))]).unwrap();
        e.put_record("decision", 2, &[("project_id", text("2")), ("title", text("D2"))]).unwrap();
        e.put_record("dimension", 1, &[("project_id", text("1")), ("name", text("軸"))]).unwrap();
        e.put_record("dimension", 2, &[("project_id", text("2")), ("name", text("別軸"))]).unwrap();
        // The comment ids a delete op needs, to sweep the attachments hanging off each comment.
        e.put_record("task_comment", 1, &[("task_id", text("1")), ("text", text("x"))]).unwrap();
        e.put_record("decision_comment", 1, &[("decision_id", text("1")), ("text", text("y"))])
            .unwrap();

        let conn = e.conn();
        assert_eq!(task_ids_in_project(conn, 1).unwrap(), vec![1], "no cross-project leak");
        assert!(task_ids_in_project(conn, 9).unwrap().is_empty(), "no project, no tasks");
        assert_eq!(decision_ids_in_project(conn, 1).unwrap(), vec![1_i64], "no cross-project leak");
        assert_eq!(dimension_ids_in_project(conn, 1).unwrap(), vec![1], "no cross-project leak");
        assert_eq!(task_comment_ids(conn, 1).unwrap(), vec![1]);
        assert!(task_comment_ids(conn, 3).unwrap().is_empty());
        assert_eq!(decision_comment_ids(conn, 1).unwrap(), vec![1_i64]);
        assert!(decision_comment_ids(conn, 2).unwrap().is_empty());
    }

    /// The engine reads that carry content are the floor of the containment — a closed reach hands back
    /// its bound project and nothing else, whatever ids or scope the caller asked for. This is the guard
    /// the facade cannot give: a surface that reads the engine straight (the GUI does) declares its reach
    /// in the signature, so a new one cannot forget and be handed every project.
    #[test]
    fn a_closed_reach_holds_every_engine_read_to_its_bound_project() {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("project", 1, &[("name", text("Alpha")), ("order_key", text("a"))]).unwrap();
        e.put_record("project", 2, &[("name", text("Beta")), ("order_key", text("b"))]).unwrap();
        let task = |id: i64, project_id: &str| {
            e.put_record(
                "task",
                id,
                &[
                    ("title", text(&format!("task {id}"))),
                    ("status", text("todo")),
                    ("project_id", text(project_id)),
                    ("assignee_kind", text("human")),
                ],
            )
            .unwrap();
        };
        task(1, "1");
        task(2, "2");
        let ai_comment = |id: i64, task_id: &str| {
            e.put_record(
                "task_comment",
                id,
                &[
                    ("task_id", text(task_id)),
                    ("author_kind", text("ai")),
                    ("text", text("incoming")),
                    ("created_at", text("2026-07-13T00:00:00Z")),
                ],
            )
            .unwrap();
        };
        ai_comment(1, "1");
        ai_comment(2, "2");
        e.put_record("decision", 1, &[("project_id", text("1")), ("title", text("D1"))]).unwrap();
        e.put_record("decision", 2, &[("project_id", text("2")), ("title", text("D2"))]).unwrap();

        let conn = e.conn();
        let bound = crate::reach::Reach::binding(1);

        // Cards: hydration is the last step before content reaches a face, so an out-of-reach id yields
        // no card — it drops out exactly as a non-live id does.
        let cards = hydrate_task_cards(conn, bound, &[1, 2], crate::time::today()).unwrap();
        assert_eq!(cards.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1], "no card from project 2");

        // The overview shows the one project, with its dimensions and counts and no sight of the other.
        let overview = project_overview(conn, bound).unwrap();
        assert_eq!(overview.iter().map(|p| p.id).collect::<Vec<_>>(), vec![1]);

        // The mailbox bundles only the bound project's tasks.
        let mb = mailbox_comment_tasks(conn, bound).unwrap();
        assert_eq!(mb.iter().map(|m| m.task_id).collect::<Vec<_>>(), vec![1]);

        // Decisions: an unscoped list is filled with the bound project…
        let rows = decision_list(conn, bound, None).unwrap();
        assert_eq!(rows.iter().map(|d| d.id).collect::<Vec<_>>(), vec![1]);
        // …and naming another project is refused rather than answered with an empty list (which would be a
        // denial that it exists).
        let refused = |e: StoreEngineError| crate::error::Error::from(e).code().to_string();
        assert_eq!(
            decision_list(conn, bound, Some(2)).err().map(refused).as_deref(),
            Some("out_of_reach")
        );
        assert_eq!(
            decision_page(conn, bound, 2, None, 0).err().map(refused).as_deref(),
            Some("out_of_reach")
        );
        // The bound project's own page still reads.
        assert_eq!(decision_page(conn, bound, 1, None, 0).unwrap().ids, vec![1]);
    }

    /// `premise_change_since` (`AMB-D-366`): a missing task is `None`; an added blocker/decision that
    /// still bears on readiness and postdates the status clock is flagged; a done blocker, a settled
    /// decision, and any premise that predates the clock are not; and a task never stamped reports no
    /// change rather than erroring. Timestamps are stamped to fixed instants so second-resolution
    /// wall-clock ties cannot make the ordering flaky.
    #[test]
    fn premise_change_since_flags_post_status_readiness_relevant_additions_only() {
        use crate::model::TaskStatus;
        use crate::ops::{decision, dependency, task};
        use crate::ops::test_support::{mk_project, mk_task_in};

        let engine = StoreEngine::open_in_memory().unwrap();
        let tx = engine.write().unwrap();
        let pid = mk_project(&tx, "amenbo 開発");
        let held = mk_task_in(&tx, "held", Some(pid));

        // A task that does not exist has no answer.
        assert!(premise_change_since(tx.conn(), 9999).unwrap().is_none());
        // Nothing pinned on yet → no change.
        assert!(!premise_change_since(tx.conn(), held).unwrap().unwrap().any());

        // Anchor the status clock in the past so every premise added below is unambiguously after it.
        tx.set_field("task", held, "status_changed_at", text("2020-01-01T00:00:00Z")).unwrap();

        // An open blocker added after — a premise that silently dropped ready. Flagged.
        let blk = mk_task_in(&tx, "blocker", Some(pid));
        dependency::add(&tx, held, blk, None).unwrap();
        // A blocker added after but already done — never blocked, so not a premise change.
        let done_blk = mk_task_in(&tx, "done blocker", Some(pid));
        dependency::add(&tx, held, done_blk, None).unwrap();
        task::set_status(&tx, done_blk, TaskStatus::Done).unwrap();

        // A proposed (unsettled) decision linked after — flagged.
        let d_open =
            decision::add(&tx, decision::NewDecision { title: "未採択".into(), body: String::new(), project_id: pid }).unwrap();
        decision::link(&tx, d_open.id, held).unwrap();
        // An accepted decision linked after — a settled ground never blocks, so not a premise change.
        let d_settled =
            decision::add(&tx, decision::NewDecision { title: "採択済み".into(), body: String::new(), project_id: pid }).unwrap();
        decision::accept(&tx, d_settled.id, None).unwrap();
        decision::link(&tx, d_settled.id, held).unwrap();

        let got = premise_change_since(tx.conn(), held).unwrap().unwrap();
        assert!(got.any());
        assert_eq!(got.added_blockers.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![blk]);
        assert_eq!(got.added_decisions.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![d_open.id]);
        // `d_open`'s own status clock also postdates the reservation, but a decision that arrived after it
        // is the link it is — reported once, on the axis that saw it first.
        assert!(got.reopened_decisions.is_empty());

        // Move the clock past every premise → the same additions now predate it → nothing flagged.
        tx.set_field("task", held, "status_changed_at", text("2999-01-01T00:00:00Z")).unwrap();
        assert!(!premise_change_since(tx.conn(), held).unwrap().unwrap().any());

        // A store predating the column (no stamp) has no instant to compare against → no change, not an error.
        tx.set_field("task", held, "status_changed_at", Value::Null).unwrap();
        assert!(!premise_change_since(tx.conn(), held).unwrap().unwrap().any());
    }

    /// The reopen axis (`AMB-D-373`): a ground that was settled when the task was reserved, and stopped
    /// being settled since, is a premise change even though its link is older than the reservation — the
    /// comparison is between two intent columns, the decision's status clock and the task's. A decision
    /// whose settlement went away *before* the reservation is not one: the holder took the task knowing it.
    #[test]
    fn premise_change_since_flags_a_ground_that_stopped_being_settled_under_the_holder() {
        use crate::ops::decision;
        use crate::ops::test_support::{mk_project, mk_task_in};

        let engine = StoreEngine::open_in_memory().unwrap();
        let tx = engine.write().unwrap();
        let pid = mk_project(&tx, "amenbo 開発");
        let held = mk_task_in(&tx, "held", Some(pid));

        // Two accepted grounds, both linked long before the reservation.
        let ground = |title: &str| {
            let d = decision::add(
                &tx,
                decision::NewDecision { title: title.into(), body: String::new(), project_id: pid },
            )
            .unwrap();
            decision::accept(&tx, d.id, None).unwrap();
            let link = decision::link(&tx, d.id, held).unwrap().0;
            tx.set_field("decision_task_link", link.id, "linked_at", text("2019-01-01T00:00:00Z")).unwrap();
            tx.set_field("decision", d.id, "status_changed_at", text("2019-01-01T00:00:00Z")).unwrap();
            d.id
        };
        let reopened = ground("着手後に開き直る");
        let stayed = ground("採択のまま");

        // The reservation comes after both were settled → nothing has moved.
        tx.set_field("task", held, "status_changed_at", text("2020-01-01T00:00:00Z")).unwrap();
        assert!(!premise_change_since(tx.conn(), held).unwrap().unwrap().any());

        // One is reopened under the holder: the link is untouched, the settlement is what went away.
        decision::reopen(&tx, reopened).unwrap();
        let got = premise_change_since(tx.conn(), held).unwrap().unwrap();
        assert!(got.any());
        assert!(got.added_blockers.is_empty() && got.added_decisions.is_empty(), "no premise was drawn");
        assert_eq!(got.reopened_decisions.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![reopened]);

        // A ground reopened *before* the reservation is a premise the holder took the task with.
        tx.set_field("decision", stayed, "status", text("proposed")).unwrap();
        tx.set_field("decision", stayed, "status_changed_at", text("2019-06-01T00:00:00Z")).unwrap();
        let got = premise_change_since(tx.conn(), held).unwrap().unwrap();
        assert_eq!(got.reopened_decisions.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![reopened]);
    }

    /// The other way a ground stops being settled: it is **superseded** under the holder. Nothing on the
    /// premise's own row moves for it — the old side is never rewritten, and being superseded is an edge
    /// and not a status — so the `supersedes` edge's own intent column is what dates it, and the same axis
    /// reports it. A supersession drawn *before* the reservation is not a change, for the reason a reopen
    /// before it is not: the holder took the task with it.
    #[test]
    fn premise_change_since_flags_a_ground_superseded_under_the_holder() {
        use crate::ops::decision;
        use crate::ops::test_support::{mk_project, mk_task_in};

        let engine = StoreEngine::open_in_memory().unwrap();
        let tx = engine.write().unwrap();
        let pid = mk_project(&tx, "amenbo 開発");
        let held = mk_task_in(&tx, "held", Some(pid));

        let mk = |title: &str| {
            decision::add(
                &tx,
                decision::NewDecision { title: title.into(), body: String::new(), project_id: pid },
            )
            .unwrap()
            .id
        };
        // An accepted ground, linked and settled long before the reservation — so neither the link's clock
        // nor the decision's own can be what flags it below.
        let ground = mk("置き換えられる前提");
        decision::accept(&tx, ground, None).unwrap();
        let link = decision::link(&tx, ground, held).unwrap().0;
        tx.set_field("decision_task_link", link.id, "linked_at", text("2019-01-01T00:00:00Z")).unwrap();
        tx.set_field("decision", ground, "status_changed_at", text("2019-01-01T00:00:00Z")).unwrap();
        tx.set_field("task", held, "status_changed_at", text("2020-01-01T00:00:00Z")).unwrap();
        assert!(!premise_change_since(tx.conn(), held).unwrap().unwrap().any());

        // Superseded under the holder: `ready` drops, and the axis says why.
        let newer = mk("置き換える方");
        decision::supersede(&tx, newer, ground, None).unwrap();
        let got = premise_change_since(tx.conn(), held).unwrap().unwrap();
        assert!(got.added_blockers.is_empty() && got.added_decisions.is_empty(), "no premise was drawn");
        assert_eq!(got.reopened_decisions.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![ground]);
        assert_eq!(
            decision(tx.conn(), ground).unwrap().unwrap().status_changed_at.map(|t| t.to_rfc3339_z()),
            Some("2019-01-01T00:00:00Z".to_string()),
            "the premise's own clock did not move — the edge is the only thing that dates this",
        );

        // The same edge, backdated to before the reservation: a premise the holder took the task with.
        let edge = decision_edge_id(tx.conn(), newer, ground).unwrap().unwrap();
        tx.set_field("decision_edge", edge, "drawn_at", text("2019-06-01T00:00:00Z")).unwrap();
        assert!(!premise_change_since(tx.conn(), held).unwrap().unwrap().any());

        // An edge from a store that predates the column reads as older than any reservation, and stays quiet
        // rather than warning on every superseded ground at once.
        tx.set_field("decision_edge", edge, "drawn_at", Value::Null).unwrap();
        assert!(!premise_change_since(tx.conn(), held).unwrap().unwrap().any());
    }

    /// Several terms are ANDed, and each is free to land on a **different** face of the same decision
    /// (`AMB-D-450`) — the reading a search box needs, since the filter grammar splits on whitespace and
    /// cannot carry more than one word.
    #[test]
    fn every_term_must_land_on_the_decision_but_not_on_one_face() {
        let e = StoreEngine::open_in_memory().unwrap();
        // One transaction, because a record is created field by field: its reference columns hold their
        // default until the field that fills them lands, and only a transaction defers the check that long.
        let tx = e.transaction().unwrap();
        e.put_record("project", 1, &[("name", text("PJ"))]).unwrap();
        let mk = |id: i64, title: &str, body: &str| {
            e.put_record(
                "decision",
                id,
                &[("project_id", Value::Integer(1)), ("title", text(title)), ("body", text(body))],
            )
            .unwrap();
        };
        mk(1, "索引の設計", "走査の経路も持つ");
        mk(2, "索引の設計", "別の話");
        mk(3, "索引の設計", "無関係");
        e.put_record("decision_comment", 7, &[("decision_id", Value::Integer(2)), ("text", text("走査の経路"))])
            .unwrap();
        tx.commit().unwrap();

        let ids = |q: &str| decisions_matching_text(e.conn(), &search::terms(q)).unwrap();
        assert_eq!(ids("索引"), vec![1, 2, 3], "one term, one face");
        assert_eq!(ids("索引 走査"), vec![1, 2], "the second term may land on the body or on a comment");
        assert_eq!(ids("索引 走査 別の話"), vec![2], "three terms, three faces of one decision");
        assert!(ids("索引 存在しない語").is_empty());
        assert_eq!(ids("   "), vec![1, 2, 3], "no term is no constraint");
    }

    /// When the fixture's decision was accepted. Named rather than inlined because the day-granularity
    /// filters are asserted against the day *this instant falls on where the test runs*, which is the
    /// same reading the filter makes — an instant compared against a hard-coded day would pass in one
    /// zone and fail in another.
    const DECIDED_AT: &str = "2026-07-05T00:00:00Z";

    /// A store the hit-level search reads: one word written on every face there is — a task's title and
    /// notes, a comment on it, a decision's title, a label another task was placed on, and the name of a
    /// file attached to that task — plus one task in another project, for the reach.
    fn search_store() -> StoreEngine {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.transaction().unwrap();
        let at = |t: &str| ("updated_at", text(t));
        // The status is written out because the filter reads it: a record raised field by field holds the
        // column's default until something fills it.
        let todo = || ("status", text("todo"));
        e.put_record("project", 1, &[("name", text("PJ"))]).unwrap();
        e.put_record("project", 2, &[("name", text("別 PJ"))]).unwrap();
        e.put_record(
            "task",
            1,
            &[
                ("project_id", Value::Integer(1)),
                ("title", text("全文検索の索引")),
                todo(),
                ("notes", text("索引は走査の経路も持つ")),
                at("2026-07-01T00:00:00Z"),
            ],
        )
        .unwrap();
        e.put_record(
            "task",
            2,
            &[("project_id", Value::Integer(1)), ("title", text("名前で引く")),
                todo(), at("2026-07-02T00:00:00Z")],
        )
        .unwrap();
        e.put_record(
            "task",
            3,
            &[("project_id", Value::Integer(2)), ("title", text("索引を別 PJ で")),
                todo(), at("2026-07-09T00:00:00Z")],
        )
        .unwrap();
        e.put_record(
            "task_comment",
            5,
            &[
                ("task_id", Value::Integer(1)),
                ("text", text("索引の話をここでする")),
                ("created_at", text("2026-07-03T00:00:00Z")),
            ],
        )
        .unwrap();
        // Status and acceptance instant are written out for the reason the tasks' status is: the decision
        // grammar reads them, and a record raised field by field holds the column's default until
        // something fills it.
        e.put_record(
            "decision",
            1,
            &[
                ("project_id", Value::Integer(1)),
                ("title", text("索引を退役させる")),
                ("body", text("読み手がいない")),
                ("status", text("accepted")),
                ("decided_at", text(DECIDED_AT)),
                at("2026-07-05T00:00:00Z"),
            ],
        )
        .unwrap();
        e.put_record(
            "dimension",
            1,
            &[("project_id", Value::Integer(1)), ("name", text("フェーズ")), at("2026-07-04T00:00:00Z")],
        )
        .unwrap();
        e.put_record(
            "dimension_value",
            1,
            &[("dimension_id", Value::Integer(1)), ("name", text("索引の期")), at("2026-07-04T00:00:00Z")],
        )
        .unwrap();
        e.put_record(
            "task_dimension_value",
            1,
            &[
                ("task_id", Value::Integer(2)),
                ("dimension_id", Value::Integer(1)),
                ("value_id", Value::Integer(1)),
            ],
        )
        .unwrap();
        e.put_record(
            "attachment",
            1,
            &[
                ("target_type", text("task")),
                ("target_id", Value::Integer(2)),
                ("kind", text("blob")),
                ("filename", text("索引ログ.md")),
                at("2026-07-06T00:00:00Z"),
            ],
        )
        .unwrap();
        tx.commit().unwrap();
        e
    }

    /// One hit, as these tests read it: the face, whose it is, and the comment it sits on if any.
    fn line(h: &SearchHitRow) -> String {
        let owner = if h.owner_kind == search::DATASET_TASK {
            crate::idref::task(h.owner_id)
        } else {
            crate::idref::decision(h.owner_id)
        };
        match h.comment_id {
            Some(c) => format!("{:?} {owner} #{c}", h.face),
            None => format!("{:?} {owner}", h.face),
        }
    }

    /// A search of the fixture, inside the bound project, with everything but the words left at its
    /// default — the base every case below narrows from (`SearchQuery { .., ..ask(&terms) }`).
    fn ask(terms: &[String]) -> SearchQuery<'_> {
        SearchQuery {
            reach: crate::reach::Reach::binding(1),
            terms,
            project_id: None,
            filter: None,
            today: crate::time::today(),
            kind: None,
            face: None,
            sort: crate::query::SearchSort::Face,
            limit: None,
            offset: 0,
        }
    }

    /// A narrowing read in one grammar or the other — what the entry point hands the read once the
    /// caller has named the side (`AMB-D-563`). Neither expression here carries a `project:`, so neither
    /// needs the resolve step the entry point runs.
    fn task_narrowing(expr: &str, today: NaiveDate) -> crate::query::SearchNarrowing {
        crate::query::SearchNarrowing::Task(Filter::parse(expr, today).unwrap())
    }

    fn decision_narrowing(expr: &str, today: NaiveDate) -> crate::query::SearchNarrowing {
        crate::query::SearchNarrowing::Decision(
            crate::query::DecisionFilter::parse(expr, today).unwrap(),
        )
    }

    /// The total, and the hits in the order they come back.
    fn found(e: &StoreEngine, q: &SearchQuery) -> (usize, Vec<String>) {
        let page = search_hits(e.conn(), q).unwrap();
        (page.total_matched, page.hits.iter().map(line).collect())
    }

    /// A hit is one **place** a word is written, and every face the index carries is one: the record's own
    /// title and body, a comment on it, a label the task was placed on, and the name of something attached
    /// to it. The default order is the face first, and the newest first inside a face (`AMB-D-449`).
    #[test]
    fn a_hit_names_the_face_it_landed_on_and_whose_it_is() {
        let e = search_store();
        let t = search::terms("索引");
        let (total, hits) = found(&e, &ask(&t));
        assert_eq!(
            hits,
            vec![
                // Two titles, the newer first.
                "Title AMB-D-1",
                "Title AMB-T-1",
                "Body AMB-T-1",
                "Comment AMB-T-1 #5",
                "Label AMB-T-2",
                "Attachment AMB-T-2",
            ]
        );
        assert_eq!(total, hits.len(), "the total counts the same hits when nothing is paged off");
    }

    /// The hit carries the text it landed in, as it was written — the snippet is cut from this, and the
    /// index's folded copy never comes back out.
    #[test]
    fn a_hit_carries_the_text_the_person_wrote() {
        let e = search_store();
        let t = search::terms("ＡＩ 索引");
        assert!(
            search_hits(e.conn(), &ask(&t)).unwrap().hits.is_empty(),
            "a word nobody wrote holds the record back"
        );

        let t = search::terms("索引ログ");
        let page = search_hits(e.conn(), &ask(&t)).unwrap();
        assert_eq!(page.hits[0].text, "索引ログ.md", "the attachment's own name, whichever column holds it");
    }

    /// Every term has to land on the **record** — the rule the list filters match by, so the two mouths
    /// cannot disagree about whether a word reaches a record — while a face is shown for carrying *any* of
    /// them, which is what makes the answer "here is where each word is written".
    #[test]
    fn the_terms_are_anded_over_the_record_and_a_face_shows_any_of_them() {
        let e = search_store();
        let t = search::terms("索引 走査");
        assert_eq!(
            found(&e, &ask(&t)).1,
            vec!["Title AMB-T-1", "Body AMB-T-1", "Comment AMB-T-1 #5"],
            "the decision carries only one of the two words, so none of its faces is a place"
        );
        let t = search::terms("索引 存在しない語");
        assert!(found(&e, &ask(&t)).1.is_empty(), "a word nothing carries empties the answer");
    }

    /// `--sort` takes the face's weight off and reads the plain timeline, by the hit's **own** instant: a
    /// comment's posting time, and for anything else when the text it sits in was last written.
    #[test]
    fn the_sort_can_drop_the_faces_weight_for_the_plain_timeline() {
        let e = search_store();
        let t = search::terms("索引");
        let (_, newest) = found(&e, &SearchQuery { sort: crate::query::SearchSort::Newest, ..ask(&t) });
        assert_eq!(
            newest,
            vec![
                "Attachment AMB-T-2",
                "Title AMB-D-1",
                "Label AMB-T-2",
                "Comment AMB-T-1 #5",
                "Title AMB-T-1",
                "Body AMB-T-1",
            ]
        );
        let (_, oldest) = found(&e, &SearchQuery { sort: crate::query::SearchSort::Oldest, ..ask(&t) });
        assert_eq!(oldest.first().unwrap(), "Title AMB-T-1", "the other end of the same line");
        assert_eq!(oldest.len(), newest.len());
    }

    /// The page is cut in SQL, and the total counts what it was cut from — which is what tells a reader
    /// that a default ceiling left something behind.
    #[test]
    fn the_page_is_cut_from_the_hits_and_the_total_counts_them_all() {
        let e = search_store();
        let t = search::terms("索引");
        let (total, page) = found(&e, &SearchQuery { limit: Some(2), offset: 1, ..ask(&t) });
        assert_eq!(total, 6, "the total is of the hits, not of the page");
        assert_eq!(page, vec!["Title AMB-T-1", "Body AMB-T-1"]);
        assert!(found(&e, &SearchQuery { limit: Some(2), offset: 99, ..ask(&t) }).1.is_empty());
    }

    /// The reach is the search's too: a bound project does not answer with another's records, and no
    /// binding reaches everything.
    #[test]
    fn the_reach_narrows_the_hits_to_the_project_it_is_bound_to() {
        let e = search_store();
        let t = search::terms("索引");
        assert!(
            !found(&e, &ask(&t)).1.contains(&"Title AMB-T-3".to_string()),
            "the other project's task is out of a bound reach"
        );
        let all = found(&e, &SearchQuery { reach: crate::reach::Reach::All, ..ask(&t) });
        assert!(all.1.contains(&"Title AMB-T-3".to_string()));
    }

    /// The scope slot narrows on its own, without a reach to carry it — that is what `--project` is, and
    /// it reaches both sides at once (`AMB-D-564`), unlike a filter, which speaks of tasks alone.
    #[test]
    fn the_scope_slot_narrows_both_sides_under_a_reach_that_holds_everything() {
        let e = search_store();
        let t = search::terms("索引");
        let open = SearchQuery { reach: crate::reach::Reach::All, ..ask(&t) };
        let (_, everywhere) = found(&e, &open);
        assert!(everywhere.contains(&"Title AMB-D-1".to_string()));
        assert!(everywhere.contains(&"Title AMB-T-3".to_string()));

        let (_, scoped) = found(&e, &SearchQuery { project_id: Some(1), ..open });
        assert!(!scoped.contains(&"Title AMB-T-3".to_string()), "the other project's task is out");
        assert!(scoped.contains(&"Title AMB-D-1".to_string()), "and this project's decision is in");
    }

    /// `--kind` narrows by which record the words are on, and nothing else: every face of that side is
    /// still in, because where on it the words sit is the other axis's to say.
    #[test]
    fn the_kind_keeps_one_side_whole() {
        let e = search_store();
        let t = search::terms("索引");
        let only = |k| found(&e, &SearchQuery { kind: Some(k), ..ask(&t) }).1;
        assert_eq!(only(crate::query::SearchKind::Decision), vec!["Title AMB-D-1"]);
        assert_eq!(
            only(crate::query::SearchKind::Task),
            vec!["Title AMB-T-1", "Body AMB-T-1", "Comment AMB-T-1 #5", "Label AMB-T-2", "Attachment AMB-T-2"],
            "a task's own faces, its timeline, its labels and what is attached to it"
        );
    }

    /// The face narrows on its own axis, across both sides — and the two ANDed are the product a mixed
    /// `--kind` could not express: the remarks on one side alone (`AMB-D-562`).
    #[test]
    fn the_face_narrows_across_both_sides_and_meets_the_kind_as_a_product() {
        let e = search_store();
        let t = search::terms("索引");
        let asked = |kind, face| found(&e, &SearchQuery { kind, face, ..ask(&t) }).1;
        assert_eq!(
            asked(None, Some(HitFace::Title)),
            vec!["Title AMB-D-1", "Title AMB-T-1"],
            "a face with no side named crosses to the decision, which `--kind comment` never could"
        );
        assert_eq!(asked(None, Some(HitFace::Comment)), vec!["Comment AMB-T-1 #5"]);
        assert_eq!(
            asked(Some(crate::query::SearchKind::Task), Some(HitFace::Comment)),
            vec!["Comment AMB-T-1 #5"],
            "the product of the two: the remarks on tasks"
        );
        assert!(
            asked(Some(crate::query::SearchKind::Decision), Some(HitFace::Comment)).is_empty(),
            "the same product on the other side — askable now, and empty in this fixture"
        );
    }

    /// A narrowing written in the task grammar lands on the task side. What takes the decisions out is
    /// the `kind` the caller had to name to be given that grammar at all (`AMB-D-563`) — never the filter
    /// quietly standing in for one.
    #[test]
    fn a_task_narrowing_lands_on_the_task_side() {
        let e = search_store();
        let t = search::terms("索引");
        let today = crate::time::today();
        let ask_with = |f: &crate::query::SearchNarrowing| {
            found(
                &e,
                &SearchQuery {
                    filter: Some(f),
                    kind: Some(crate::query::SearchKind::Task),
                    ..ask(&t)
                },
            )
            .1
        };
        let none = task_narrowing("status:in_progress", today);
        assert!(
            ask_with(&none).is_empty(),
            "no task is in progress in the fixture, and the decision does not stand in for one"
        );
        let all = task_narrowing("status:todo", today);
        assert_eq!(
            ask_with(&all),
            vec!["Title AMB-T-1", "Body AMB-T-1", "Comment AMB-T-1 #5", "Label AMB-T-2", "Attachment AMB-T-2"],
        );
    }

    /// The other grammar, on the other side: `decision list`'s keys narrow the decision arms, which is
    /// what a search of decisions could not be asked before (`AMB-D-563`). The day-granularity arms are
    /// asserted against the day the fixture's instant falls on *here*, since that is the reading the
    /// filter makes.
    #[test]
    fn a_decision_narrowing_lands_on_the_decision_side() {
        let e = search_store();
        let t = search::terms("索引");
        let today = crate::time::today();
        let ask_with = |expr: &str| {
            let f = decision_narrowing(expr, today);
            found(
                &e,
                &SearchQuery {
                    filter: Some(&f),
                    kind: Some(crate::query::SearchKind::Decision),
                    ..ask(&t)
                },
            )
            .1
        };
        assert_eq!(ask_with("status:accepted"), vec!["Title AMB-D-1"]);
        assert!(ask_with("status:proposed").is_empty(), "the fixture's decision was accepted");
        assert!(ask_with("superseded:yes").is_empty(), "nothing overturned it");
        assert_eq!(ask_with("superseded:no"), vec!["Title AMB-D-1"]);
        assert_eq!(ask_with("number:1"), vec!["Title AMB-D-1"], "the id is the number");
        assert!(ask_with("number:T-1").is_empty(), "a task-typed number names no decision");
        assert!(ask_with("task:1").is_empty(), "no task links to it in the fixture");

        let decided = crate::time::Timestamp::parse_rfc3339(DECIDED_AT).unwrap().local_date();
        let day = |d: chrono::NaiveDate| d.to_string();
        assert_eq!(ask_with(&format!("decided_after:{}", day(decided))), vec!["Title AMB-D-1"]);
        assert_eq!(ask_with(&format!("decided_before:{}", day(decided))), vec!["Title AMB-D-1"]);
        assert!(
            ask_with(&format!("decided_after:{}", day(decided + Duration::days(1)))).is_empty(),
            "the day it was accepted on is included at each end, and the day after is past it"
        );
        assert!(ask_with(&format!("decided_before:{}", day(decided - Duration::days(1)))).is_empty());
    }

    /// No words is not "no constraint" here, the way it is in a filter: every face of every record is not
    /// an answer to where a word is written.
    #[test]
    fn a_search_with_no_words_is_no_search() {
        let e = search_store();
        let t = search::terms("   ");
        let (total, hits) = found(&e, &ask(&t));
        assert_eq!((total, hits.len()), (0, 0));
    }
}
