//! Reverse projection: reconstruct an in-memory [`Database`] from the engine SQLite read-model.
//!
//! This is the inverse of the [`super::record`] projection: the engine is the store's truth
//! source, and this reads every read-model table back into the model shapes.
//!
//! **Nothing on a hot path calls this**: reads issue indexed SQL straight at the read-model, and
//! this is O(rows). Its callers are the ones that genuinely need the whole model at once — backup and
//! restore prove a snapshot hydrates before adopting it ([`mod@crate::archive`]), and the round-trip
//! oracle below reads a `Database` out of the same rows the SQL path serves.
//!
//! Faithfulness: every dataset in [`super::schema`] round-trips field-for-field (the forward
//! projection and this reverse read share the same column vocabulary). The store-level singleton
//! scalar `Database::schema_version` has **no per-record dataset** (it is a store-level scalar, not a per-record field);
//! it lives in the `store_meta` KV table and is read back via [`crate::store_engine::read_meta`].
//!
//! Row order is canonical: the `SELECT … ORDER BY id` of every table sorts each collection by `id`
//! ascending, so a hydrated `Database` is byte-identical (export/JSON) regardless of write order.
//!
//! The row→model mapping lives in one function per table (`task_row`, `project_row`, …), shared by
//! the whole-table read here and the single-row loaders in [`super::read`]. One row, one mapping: a
//! `Task` read by id and a `Task` read by `hydrate_database` cannot drift apart.

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, Row};

use super::schema::col;
use super::sql::{Col, ColType, NotNull, Nullability, Nullable, Read, Text};
use super::Result;
use crate::model::{
    ActorKind, Attachment, AttachmentKind, AttachmentTarget, Database,
    Decision, DecisionComment, DecisionEdge, DecisionEdgeKind, DecisionStatus, DecisionTaskLink,
    Dimension, DimensionCardinality,
    DimensionRole, DimensionValue, PluginConfigValue, PluginEnabledProject, PluginSecret, Priority,
    Project, Subtype, Task, TaskComment, TaskCommit, TaskDependency,
    TaskDimensionValue, TaskStatus, View,
};
use crate::time::Timestamp;

// ───────────────────────── value readers ─────────────────────────

/// Box a message into the `rusqlite` conversion-failure error (parse failures surface as a normal
/// read error rather than a panic).
fn bad(msg: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, msg.into())
}

fn parse_ts(s: &str) -> rusqlite::Result<Timestamp> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| Timestamp(d.with_timezone(&Utc)))
        .map_err(|e| bad(format!("bad timestamp {s:?}: {e}")))
}

/// Read a column by the identifier the registry generated for it ([`col`]) — the column's name and the
/// Rust shape it reads back as (its SQLite type and whether it admits `NULL`) both come off the same
/// declaration that emits its DDL. A column the store does not have is a name that does not compile, and
/// a nullable one cannot be read as a bare `String`. The read is by name, not by position: these
/// mappings run over a `SELECT *`, so there is no list to keep in step with (which is why they need no
/// [`super::sql::Select`]).
fn get<T: ColType, N: Nullability>(r: &Row, c: Col<T, N>) -> rusqlite::Result<<Col<T, N> as Read>::Out>
where
    Col<T, N>: Read,
{
    r.get(c.name())
}

fn ts(r: &Row, c: Col<Text, NotNull>) -> rusqlite::Result<Timestamp> {
    parse_ts(&get(r, c)?)
}

fn ts_opt(r: &Row, c: Col<Text, Nullable>) -> rusqlite::Result<Option<Timestamp>> {
    get(r, c)?.as_deref().map(parse_ts).transpose()
}

fn date_opt(r: &Row, c: Col<Text, Nullable>) -> rusqlite::Result<Option<NaiveDate>> {
    match get(r, c)? {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map(Some)
            .map_err(|e| bad(format!("bad date {s:?}: {e}"))),
        None => Ok(None),
    }
}

fn enum_req<T>(r: &Row, c: Col<Text, NotNull>, f: impl Fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    let s = get(r, c)?;
    f(&s).ok_or_else(|| bad(format!("unknown {} value {s:?}", c.name())))
}

fn enum_opt<T>(
    r: &Row,
    c: Col<Text, Nullable>,
    f: impl Fn(&str) -> Option<T>,
) -> rusqlite::Result<Option<T>> {
    match get(r, c)? {
        Some(s) => f(&s).map(Some).ok_or_else(|| bad(format!("unknown {} value {s:?}", c.name()))),
        None => Ok(None),
    }
}

/// `(created_at, updated_at)` — the audit pair every record carries, named through its own table's
/// columns (they are the same two `Col`s on every table, but the table they come from is the caller's).
fn audit(r: &Row, created_at: Col<Text, NotNull>, updated_at: Col<Text, NotNull>) -> rusqlite::Result<(Timestamp, Timestamp)> {
    Ok((ts(r, created_at)?, ts(r, updated_at)?))
}

// Key columns (`id`, an `fk!` reference, the polymorphic `target_id`) are all INTEGER, so they read back
// as a `Col<Int>` and `get` answers them as they are — none of them needs a reader of its own.
//
// The reverse of an enum's wire string is defined once on the model itself (`View::parse` and friends),
// so it shares its vocabulary with the forward `as_str` the projection writes. Nothing is copied out
// locally here.

// ───────────────────────── row readers ─────────────────────────
//
// One function per table: the single definition of "this row is this record". `hydrate_database`
// maps a whole table through it; `super::read`'s single-row loaders map one row through the same
// function, so the two cannot disagree about a column.
//
// Every column is named through the registry's typed identifier (`col::<table>::ALL`), so the columns
// these mappings read are the columns the store has — and a rename in the registry lands here as a
// compile error rather than as a silently absent value.

pub(super) fn project_row(r: &Row) -> rusqlite::Result<Project> {
    const C: col::project::Cols = col::project::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(Project {
        id: get(r, C.id)?,
        name: get(r, C.name)?,
        notes: get(r, C.notes)?,
        color: get(r, C.color)?,
        default_view: enum_req(r, C.default_view, View::parse)?,
        archived: get(r, C.archived)?,
        order_key: get(r, C.order_key)?,
        // A store written by an older binary predating the column reads null here (no slug), which is
        // faithful.
        slug: get(r, C.slug)?,
        created_at,
        updated_at,
    })
}

pub(super) fn task_row(r: &Row) -> rusqlite::Result<Task> {
    const C: col::task::Cols = col::task::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(Task {
        id: get(r, C.id)?,
        title: get(r, C.title)?,
        notes: get(r, C.notes)?,
        subtype: enum_req(r, C.subtype, Subtype::parse)?,
        completed_at: ts_opt(r, C.completed_at)?,
        status: enum_req(r, C.status, TaskStatus::parse)?,
        status_changed_at: ts_opt(r, C.status_changed_at)?,
        draft: get(r, C.draft)?,
        created_by_kind: enum_opt(r, C.created_by_kind, ActorKind::parse)?,
        assignee_kind: enum_opt(r, C.assignee_kind, ActorKind::parse)?,
        start_on: date_opt(r, C.start_on)?,
        due_on: date_opt(r, C.due_on)?,
        priority: enum_opt(r, C.priority, Priority::parse)?,
        project_id: get(r, C.project_id)?,
        order_key: get(r, C.order_key)?,
        at_binding_id: get(r, C.at_binding_id)?,
        created_at,
        updated_at,
    })
}

pub(super) fn task_dependency_row(r: &Row) -> rusqlite::Result<TaskDependency> {
    const C: col::task_dependency::Cols = col::task_dependency::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(TaskDependency {
        id: get(r, C.id)?,
        task_id: get(r, C.task_id)?,
        blocked_by_id: get(r, C.blocked_by_id)?,
        created_by_kind: enum_opt(r, C.created_by_kind, ActorKind::parse)?,
        established_at: ts_opt(r, C.established_at)?,
        created_at,
        updated_at,
    })
}

pub(super) fn task_commit_row(r: &Row) -> rusqlite::Result<TaskCommit> {
    const C: col::task_commit::Cols = col::task_commit::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(TaskCommit {
        id: get(r, C.id)?,
        task_id: get(r, C.task_id)?,
        sha: get(r, C.sha)?,
        created_by_kind: enum_opt(r, C.created_by_kind, ActorKind::parse)?,
        created_at,
        updated_at,
    })
}

pub(super) fn plugin_config_row(r: &Row) -> rusqlite::Result<PluginConfigValue> {
    const C: col::plugin_config::Cols = col::plugin_config::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(PluginConfigValue {
        id: get(r, C.id)?,
        project_id: get(r, C.project_id)?,
        plugin: get(r, C.plugin)?,
        field_key: get(r, C.field_key)?,
        value: get(r, C.value)?,
        created_at,
        updated_at,
    })
}

pub(super) fn plugin_secret_row(r: &Row) -> rusqlite::Result<PluginSecret> {
    const C: col::plugin_secret::Cols = col::plugin_secret::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(PluginSecret {
        id: get(r, C.id)?,
        project_id: get(r, C.project_id)?,
        plugin: get(r, C.plugin)?,
        field_key: get(r, C.field_key)?,
        value: get(r, C.value)?,
        created_at,
        updated_at,
    })
}

pub(super) fn plugin_enable_row(r: &Row) -> rusqlite::Result<PluginEnabledProject> {
    const C: col::plugin_enable::Cols = col::plugin_enable::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(PluginEnabledProject {
        id: get(r, C.id)?,
        project_id: get(r, C.project_id)?,
        plugin: get(r, C.plugin)?,
        created_at,
        updated_at,
    })
}

pub(super) fn decision_row(r: &Row) -> rusqlite::Result<Decision> {
    const C: col::decision::Cols = col::decision::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(Decision {
        id: get(r, C.id)?,
        project_id: get(r, C.project_id)?,
        title: get(r, C.title)?,
        body: get(r, C.body)?,
        status: enum_req(r, C.status, DecisionStatus::parse)?,
        status_changed_at: ts_opt(r, C.status_changed_at)?,
        decided_at: ts_opt(r, C.decided_at)?,
        decided_by: get(r, C.decided_by)?,
        created_at,
        updated_at,
    })
}

pub(super) fn decision_edge_row(r: &Row) -> rusqlite::Result<DecisionEdge> {
    const C: col::decision_edge::Cols = col::decision_edge::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(DecisionEdge {
        id: get(r, C.id)?,
        decision_id: get(r, C.decision_id)?,
        target_decision_id: get(r, C.target_decision_id)?,
        kind: enum_req(r, C.kind, DecisionEdgeKind::parse)?,
        drawn_at: ts_opt(r, C.drawn_at)?,
        created_at,
        updated_at,
    })
}

pub(super) fn decision_task_link_row(r: &Row) -> rusqlite::Result<DecisionTaskLink> {
    const C: col::decision_task_link::Cols = col::decision_task_link::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(DecisionTaskLink {
        id: get(r, C.id)?,
        decision_id: get(r, C.decision_id)?,
        task_id: get(r, C.task_id)?,
        linked_at: ts_opt(r, C.linked_at)?,
        created_at,
        updated_at,
    })
}

pub(super) fn dimension_row(r: &Row) -> rusqlite::Result<Dimension> {
    const C: col::dimension::Cols = col::dimension::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(Dimension {
        id: get(r, C.id)?,
        project_id: get(r, C.project_id)?,
        name: get(r, C.name)?,
        notes: get(r, C.notes)?,
        cardinality: enum_req(r, C.cardinality, DimensionCardinality::parse)?,
        ordered: get(r, C.ordered)?,
        role: enum_req(r, C.role, DimensionRole::parse)?,
        show_on_card: get(r, C.show_on_card)?,
        required: get(r, C.required)?,
        order_key: get(r, C.order_key)?,
        created_at,
        updated_at,
    })
}

pub(super) fn dimension_value_row(r: &Row) -> rusqlite::Result<DimensionValue> {
    const C: col::dimension_value::Cols = col::dimension_value::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(DimensionValue {
        id: get(r, C.id)?,
        dimension_id: get(r, C.dimension_id)?,
        name: get(r, C.name)?,
        order_key: get(r, C.order_key)?,
        start_on: date_opt(r, C.start_on)?,
        end_on: date_opt(r, C.end_on)?,
        created_at,
        updated_at,
    })
}

pub(super) fn task_dimension_value_row(r: &Row) -> rusqlite::Result<TaskDimensionValue> {
    const C: col::task_dimension_value::Cols = col::task_dimension_value::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(TaskDimensionValue {
        id: get(r, C.id)?,
        task_id: get(r, C.task_id)?,
        dimension_id: get(r, C.dimension_id)?,
        value_id: get(r, C.value_id)?,
        created_at,
        updated_at,
    })
}

pub(super) fn task_comment_row(r: &Row) -> rusqlite::Result<TaskComment> {
    const C: col::task_comment::Cols = col::task_comment::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(TaskComment {
        id: get(r, C.id)?,
        task_id: get(r, C.task_id)?,
        author_kind: enum_opt(r, C.author_kind, ActorKind::parse)?,
        text: get(r, C.text)?,
        created_at,
        updated_at,
        edited_at: ts_opt(r, C.edited_at)?,
    })
}

pub(super) fn decision_comment_row(r: &Row) -> rusqlite::Result<DecisionComment> {
    const C: col::decision_comment::Cols = col::decision_comment::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(DecisionComment {
        id: get(r, C.id)?,
        decision_id: get(r, C.decision_id)?,
        author_kind: enum_opt(r, C.author_kind, ActorKind::parse)?,
        text: get(r, C.text)?,
        created_at,
        updated_at,
        edited_at: ts_opt(r, C.edited_at)?,
    })
}

pub(super) fn attachment_row(r: &Row) -> rusqlite::Result<Attachment> {
    const C: col::attachment::Cols = col::attachment::ALL;
    let (created_at, updated_at) = audit(r, C.created_at, C.updated_at)?;
    Ok(Attachment {
        id: get(r, C.id)?,
        target_type: enum_req(r, C.target_type, AttachmentTarget::parse)?,
        // Polymorphic (an unconstrained key, not `fk!`): which table it names is `target_type`'s to say.
        target_id: get(r, C.target_id)?,
        kind: enum_req(r, C.kind, AttachmentKind::parse)?,
        blob_hash: get(r, C.blob_hash)?,
        filename: get(r, C.filename)?,
        mime: get(r, C.mime)?,
        size_bytes: get(r, C.size_bytes)?,
        url: get(r, C.url)?,
        created_by_kind: enum_opt(r, C.created_by_kind, ActorKind::parse)?,
        order_key: get(r, C.order_key)?,
        created_at,
        updated_at,
    })
}

// ───────────────────────── table readers ─────────────────────────

/// Read a whole read-model table into model records, `id` ascending (matches the `by_id` hydrate).
/// Every row is a live record: a delete removes the row, so there is nothing here to filter. Raw by
/// necessity: the **table comes in as a name** — the caller is the reverse projection, which walks every
/// table the registry declares — so there is no `col::` constant to name it with, and `SELECT *` has no
/// list to count off while the mapping (`f`) reads each column **by name** through the registry's typed
/// identifiers.
pub(super) fn rows<T>(conn: &Connection, table: &str, f: impl Fn(&Row) -> rusqlite::Result<T>) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table} ORDER BY id"))?;
    let out = stmt.query_map([], |r| f(r))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

/// Read one record by `id`, or `None` when no such row exists — which, since a delete removes the row,
/// is also what a deleted record reads as. Raw for the same reason as [`rows`]: the table is a name the
/// caller brings, and the row is read by name rather than by position.
pub(super) fn row_by_id<T>(
    conn: &Connection,
    table: &str,
    id: i64,
    f: impl Fn(&Row) -> rusqlite::Result<T>,
) -> Result<Option<T>> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table} WHERE id = ?1"))?;
    let mut hits = stmt.query_map([id], |r| f(r))?;
    match hits.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// Reconstruct an in-memory [`Database`] from the engine read-model tables. The per-record collections
/// come from their read-model tables; the store-level singleton scalar `schema_version` comes from the
/// `store_meta` KV table (see the module docs).
pub fn hydrate_database(conn: &Connection) -> Result<Database> {
    let projects = rows(conn, "project", project_row)?;
    let tasks = rows(conn, "task", task_row)?;
    let task_dependencies = rows(conn, "task_dependency", task_dependency_row)?;
    let task_commits = rows(conn, "task_commit", task_commit_row)?;
    let decisions = rows(conn, "decision", decision_row)?;
    let decision_edges = rows(conn, "decision_edge", decision_edge_row)?;
    let decision_task_links = rows(conn, "decision_task_link", decision_task_link_row)?;
    let dimensions = rows(conn, "dimension", dimension_row)?;
    let dimension_values = rows(conn, "dimension_value", dimension_value_row)?;
    let task_dimension_values = rows(conn, "task_dimension_value", task_dimension_value_row)?;
    let task_comments = rows(conn, "task_comment", task_comment_row)?;
    let decision_comments = rows(conn, "decision_comment", decision_comment_row)?;
    let attachments = rows(conn, "attachment", attachment_row)?;

    Ok(Database {
        // The store-level singleton scalar lives in the store_meta KV table (no per-record dataset).
        // An older store predating that table reads None → fall back to the current schema version
        // (the same default the genesis path then re-stamps on first save).
        schema_version: crate::store_engine::read_meta(conn, crate::store_engine::META_SCHEMA_VERSION)?
            .unwrap_or_else(|| crate::model::SCHEMA_VERSION.to_string()),
        projects,
        tasks,
        task_dependencies,
        task_commits,
        decisions,
        decision_edges,
        decision_task_links,
        dimensions,
        dimension_values,
        task_dimension_values,
        task_comments,
        decision_comments,
        attachments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::record;

    /// Flush the fixture `Database` into the truth source through **the production write mapping**
    /// ([`record::put_database`]).
    fn project_database(db: &Database, engine: &StoreEngine) -> crate::store_engine::Result<()> {
        let tx = engine.write()?;
        record::put_database(&tx, db)?;
        tx.commit()
    }
    use crate::store_engine::read;
    use crate::store_engine::StoreEngine;

    /// One record in every collection, every field non-empty (every `Option` is `Some`, every enum set).
    /// Shared by the round-trip oracle and the single-record loader parity test below.
    fn fixture_database() -> Database {
        let now = crate::time::Timestamp::now();

        let db = Database {
            // Store-level singleton scalar: round-tripped through the store_meta KV table.
            schema_version: crate::model::SCHEMA_VERSION.to_string(),
            projects: vec![Project {
                id: 1,
                name: "PJ".to_string(),
                notes: "notes".to_string(),
                color: Some("red".to_string()),
                default_view: View::Board,
                archived: true,
                order_key: "a0".to_string(),
                // The human-readable identifier — exercises the `slug` column.
                slug: Some("pj-1".to_string()),
                created_at: now,
                updated_at: now,
            }],
            tasks: vec![Task {
                id: 42,
                title: "round-trip".to_string(),
                notes: "with notes".to_string(),
                subtype: Subtype::Milestone,
                completed_at: Some(now),
                status: TaskStatus::Done,
                status_changed_at: Some(now),
                // Set, like every other field here: `false` is the default, so a dropped column would
                // round-trip clean against it.
                draft: true,
                created_by_kind: Some(ActorKind::Ai),
                assignee_kind: Some(ActorKind::Ai),
                start_on: Some("2026-07-01".parse().unwrap()),
                due_on: Some("2026-07-15".parse().unwrap()),
                priority: Some(Priority::High),
                project_id: Some(1),
                order_key: Some("a0".to_string()),
                // Set for the same reason `draft` is: `None` is the default, so a column that failed to
                // round-trip would read as the value this field is meant to be testing.
                at_binding_id: Some(7),
                created_at: now,
                updated_at: now,
            }],
            task_dependencies: vec![TaskDependency {
                id: 1,
                task_id: 42,
                blocked_by_id: 42,
                created_by_kind: Some(ActorKind::Ai),
                established_at: Some(now),
                created_at: now,
                updated_at: now,
            }],
            task_commits: vec![TaskCommit {
                id: 1,
                task_id: 42,
                sha: "0123456789abcdef0123456789abcdef01234567".to_string(),
                created_by_kind: Some(ActorKind::Ai),
                created_at: now,
                updated_at: now,
            }],
            decisions: vec![Decision {
                id: 43,
                project_id: 1,
                title: "RDB を真実源にする".to_string(),
                body: "結論と根拠".to_string(),
                status: DecisionStatus::Rejected,
                status_changed_at: Some(now),
                decided_at: Some(now),
                decided_by: Some("user-1".to_string()),
                created_at: now,
                updated_at: now,
            }],
            // The edge points the fixture's one decision at itself — the same self-reference
            // `task_dependency` uses above, so the row is a real `decision → decision` edge without a
            // second decision. `Amends` (not the enum's default) so a dropped column cannot pass.
            decision_edges: vec![DecisionEdge {
                id: 1,
                decision_id: 43,
                target_decision_id: 43,
                kind: DecisionEdgeKind::Amends,
                drawn_at: Some(now),
                created_at: now,
                updated_at: now,
            }],
            decision_task_links: vec![DecisionTaskLink {
                id: 1,
                decision_id: 43,
                task_id: 42,
                linked_at: Some(now),
                created_at: now,
                updated_at: now,
            }],
            dimensions: vec![Dimension {
                id: 1,
                project_id: 1,
                name: "フェーズ".to_string(),
                notes: "時間軸".to_string(),
                cardinality: DimensionCardinality::Single,
                ordered: true,
                role: DimensionRole::TimeAxis,
                // Set for the same reason `at_binding_id` is: `false` is the default, so a column that
                // failed to round-trip would read as the value this field is meant to be testing.
                show_on_card: true,
                required: true,
                order_key: "a0".to_string(),
                created_at: now,
                updated_at: now,
            }],
            dimension_values: vec![DimensionValue {
                id: 1,
                dimension_id: 1,
                name: "done".to_string(),
                order_key: "a0".to_string(),
                start_on: Some("2026-06-20".parse().unwrap()),
                end_on: Some("2026-07-07".parse().unwrap()),
                created_at: now,
                updated_at: now,
            }],
            task_dimension_values: vec![TaskDimensionValue {
                id: 1,
                task_id: 42,
                dimension_id: 1,
                value_id: 1,
                created_at: now,
                updated_at: now,
            }],
            task_comments: vec![TaskComment {
                id: 1,
                task_id: 42,
                author_kind: Some(ActorKind::Ai),
                text: "looks good".to_string(),
                created_at: now,
                updated_at: now,
                // The fact that it was edited round-trips too — a column the projection never wrote
                // would fail right here.
                edited_at: Some(now),
            }],
            decision_comments: vec![DecisionComment {
                id: 1,
                decision_id: 43,
                author_kind: Some(ActorKind::Ai),
                text: "この根拠に同意".to_string(),
                created_at: now,
                updated_at: now,
                edited_at: Some(now),
            }],
            attachments: vec![Attachment {
                id: 1,
                target_type: AttachmentTarget::Decision,
                target_id: 43,
                kind: AttachmentKind::Blob,
                blob_hash: Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string()),
                filename: Some("spec.pdf".to_string()),
                mime: Some("application/pdf".to_string()),
                size_bytes: Some(2048),
                url: None,
                created_by_kind: Some(ActorKind::Ai),
                order_key: "m".to_string(),
                created_at: now,
                updated_at: now,
            }],
        };

        // Guard: every collection is populated, so a new collection added to `Database` without a
        // fixture record here trips this (forcing the oracle to stay comprehensive).
        assert!(
            !db.projects.is_empty()
                && !db.tasks.is_empty()
                && !db.task_dependencies.is_empty()
                && !db.task_commits.is_empty()
                && !db.decisions.is_empty()
                && !db.decision_edges.is_empty()
                && !db.decision_task_links.is_empty()
                && !db.dimensions.is_empty()
                && !db.dimension_values.is_empty()
                && !db.task_dimension_values.is_empty()
                && !db.task_comments.is_empty()
                && !db.decision_comments.is_empty()
                && !db.attachments.is_empty(),
            "fixture must cover every collection"
        );
        db
    }

    #[test]
    fn project_then_hydrate_round_trips_every_field() {
        let db = fixture_database();

        // Forward (project) → reverse (hydrate). The input is the oracle.
        let engine = StoreEngine::open_in_memory().unwrap();
        project_database(&db, &engine).unwrap();
        let back = hydrate_database(engine.conn()).unwrap();

        let want = serde_json::to_value(&db).unwrap();
        let got = serde_json::to_value(&back).unwrap();
        assert_eq!(want, got, "model↔engine round-trip diverged");
    }

    /// A record read by id is the same record `hydrate_database` produces for that id — the
    /// two share one row→model function per table, and this pins that they cannot drift.
    #[test]
    fn a_record_read_by_id_matches_the_hydrated_one() {
        let engine = StoreEngine::open_in_memory().unwrap();
        project_database(&fixture_database(), &engine).unwrap();
        let conn = engine.conn();
        let db = hydrate_database(conn).unwrap();

        // Each loader against its collection's single record. `json!`-comparing keeps this honest
        // for models that do not derive `PartialEq`.
        let pairs: Vec<(&str, serde_json::Value, serde_json::Value)> = vec![
            ("task", json(&db.tasks[0]), json(&read::task(conn, 42).unwrap().unwrap())),
            ("project", json(&db.projects[0]), json(&read::project(conn, 1).unwrap().unwrap())),
            ("decision", json(&db.decisions[0]), json(&read::decision(conn, 43).unwrap().unwrap())),
            ("dimension", json(&db.dimensions[0]), json(&read::dimension(conn, 1).unwrap().unwrap())),
            ("dimension_value", json(&db.dimension_values[0]), json(&read::dimension_value(conn, 1).unwrap().unwrap())),
            ("task_dimension_value", json(&db.task_dimension_values[0]), json(&read::task_dimension_value(conn, 1).unwrap().unwrap())),
            ("task_dependency", json(&db.task_dependencies[0]), json(&read::task_dependency(conn, 1).unwrap().unwrap())),
            ("task_commit", json(&db.task_commits[0]), json(&read::task_commit(conn, 1).unwrap().unwrap())),
            ("decision_edge", json(&db.decision_edges[0]), json(&read::decision_edge(conn, 1).unwrap().unwrap())),
            ("decision_task_link", json(&db.decision_task_links[0]), json(&read::decision_task_link(conn, 1).unwrap().unwrap())),
            ("task_comment", json(&db.task_comments[0]), json(&read::task_comment(conn, 1).unwrap().unwrap())),
            ("decision_comment", json(&db.decision_comments[0]), json(&read::decision_comment(conn, 1).unwrap().unwrap())),
            ("attachment", json(&db.attachments[0]), json(&read::attachment(conn, 1).unwrap().unwrap())),
        ];
        for (table, hydrated, by_id) in pairs {
            assert_eq!(hydrated, by_id, "`{table}` read by id diverged from the hydrated record");
        }

        assert!(
            read::task(conn, 999).unwrap().is_none(),
            "an absent id loads to None, not an error"
        );
    }

    fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
        serde_json::to_value(v).unwrap()
    }
}
