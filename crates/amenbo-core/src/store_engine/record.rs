//! The field projection of one record.
//!
//! This is the only place that decides **what** a mutation writes to the truth source; where it goes —
//! which transaction it is flushed through — is [`crate::ops::emit_create`] /
//! [`crate::ops::emit_update`]'s call. A domain operation ([`crate::ops`]) projects the record it wants
//! to write into a [`Record`] here.
//!
//! There are only two ways to record something:
//! - **Create**: project every field of the record (the [`Record`] itself).
//! - **Update**: compare the `before` and `after` projections and write only the columns whose value
//!   changed ([`Record::changed_from`] — the one record the op touched, never a scan of the database).
//!   Structurally, this cannot miss a field the way writing columns out by hand can.
//!
//! The stored form of a value (Text / Integer / Null) and the dataset and column names are decided
//! **here and nowhere else**. Two mappings means one of them goes stale, so even fixtures write through
//! this one ([`put_database`]). The stored spelling of an enum is defined once on the model itself
//! (`as_str` / `parse`), so the forward projection (here) and the reverse one ([`super::hydrate`]) share
//! a vocabulary rather than each keeping a local copy of it; round-trip tests watch that the two agree.

use chrono::NaiveDate;
use rusqlite::types::Value;

use crate::model::{
    ActorKind, Attachment, Database, Decision, DecisionComment, DecisionEdge, DecisionTaskLink,
    Dimension, DimensionValue, Project, Task, TaskComment, TaskCommit, TaskDependency,
    TaskDimensionValue,
};
use crate::time::Timestamp;

/// One record projected onto its dataset, its row id and all of its fields.
#[derive(Clone, Debug)]
pub struct Record {
    pub dataset: &'static str,
    pub id: i64,
    pub fields: Vec<(&'static str, Value)>,
}

impl Record {
    fn new(dataset: &'static str, id: i64, fields: Vec<(&'static str, Value)>) -> Self {
        Self { dataset, id, fields }
    }

    /// The columns an update has to write: exactly those whose value changed from `before`. Both field
    /// lists come from the same projection function, so their column order matches — which is what makes
    /// the zip comparison sound.
    pub fn changed_from(&self, before: &Record) -> Vec<(&'static str, Value)> {
        debug_assert_eq!(self.dataset, before.dataset);
        debug_assert_eq!(self.id, before.id);
        before
            .fields
            .iter()
            .zip(&self.fields)
            .filter(|((_, b), (_, a))| b != a)
            .map(|(_, (col, a))| (*col, a.clone()))
            .collect()
    }
}

// ───────────────────────── value helpers ─────────────────────────

fn tv(s: &str) -> Value {
    Value::Text(s.to_string())
}
fn ov(s: &Option<String>) -> Value {
    s.as_ref().map(|s| tv(s)).unwrap_or(Value::Null)
}
fn bv(v: bool) -> Value {
    Value::Integer(v as i64)
}
/// A genuine integer value (`size_bytes`) — not a key.
fn iv(n: i64) -> Value {
    Value::Integer(n)
}
/// The stored value of a key (`id`, a reference column). The physical column is INTEGER, so it is written
/// as an integer.
fn kv(n: i64) -> Value {
    Value::Integer(n)
}
fn kv_opt(n: &Option<i64>) -> Value {
    n.map(Value::Integer).unwrap_or(Value::Null)
}
fn tsv(t: &Timestamp) -> Value {
    tv(&t.to_rfc3339_z())
}
fn tsov(t: &Option<Timestamp>) -> Value {
    t.as_ref().map(tsv).unwrap_or(Value::Null)
}
fn dov(d: &Option<NaiveDate>) -> Value {
    d.map(|d| tv(&d.to_string())).unwrap_or(Value::Null)
}
fn kov(k: &Option<ActorKind>) -> Value {
    k.map(|k| tv(k.as_str())).unwrap_or(Value::Null)
}

/// Append the two audit columns to a field list.
fn with_audit(
    mut f: Vec<(&'static str, Value)>,
    c: &Timestamp,
    u: &Timestamp,
) -> Vec<(&'static str, Value)> {
    f.push(("created_at", tsv(c)));
    f.push(("updated_at", tsv(u)));
    f
}

// ───────────────────────── per-record field projection ─────────────────────────
// The column order only has to agree between `before` and `after` — that is what `changed_from`'s zip
// comparison rests on.

pub fn project(p: &Project) -> Record {
    Record::new(
        "project",
        p.id,
        with_audit(
            vec![
                ("name", tv(&p.name)),
                ("notes", tv(&p.notes)),
                ("color", ov(&p.color)),
                ("default_view", tv(p.default_view.as_str())),
                ("archived", bv(p.archived)),
                ("order_key", tv(&p.order_key)),
                ("slug", ov(&p.slug)),
            ],
            &p.created_at,
            &p.updated_at,
        ),
    )
}

pub fn task(t: &Task) -> Record {
    Record::new(
        "task",
        t.id,
        with_audit(
            vec![
                ("title", tv(&t.title)),
                ("notes", tv(&t.notes)),
                ("subtype", tv(t.subtype.as_str())),
                ("completed_at", tsov(&t.completed_at)),
                ("status", tv(t.status.as_str())),
                ("created_by_kind", kov(&t.created_by_kind)),
                ("assignee_kind", kov(&t.assignee_kind)),
                ("start_on", dov(&t.start_on)),
                ("due_on", dov(&t.due_on)),
                ("priority", t.priority.map(|p| tv(p.as_str())).unwrap_or(Value::Null)),
                ("project_id", kv_opt(&t.project_id)),
                ("order_key", ov(&t.order_key)),
            ],
            &t.created_at,
            &t.updated_at,
        ),
    )
}

pub fn dependency(d: &TaskDependency) -> Record {
    Record::new(
        "dependency",
        d.id,
        with_audit(
            vec![
                ("task_id", kv(d.task_id)),
                ("blocked_by_id", kv(d.blocked_by_id)),
                ("created_by_kind", kov(&d.created_by_kind)),
            ],
            &d.created_at,
            &d.updated_at,
        ),
    )
}

pub fn task_commit(c: &TaskCommit) -> Record {
    Record::new(
        "task_commit",
        c.id,
        with_audit(
            vec![
                ("task_id", kv(c.task_id)),
                ("sha", tv(&c.sha)),
                ("created_by_kind", kov(&c.created_by_kind)),
            ],
            &c.created_at,
            &c.updated_at,
        ),
    )
}

pub fn decision(d: &Decision) -> Record {
    Record::new(
        "decision",
        d.id,
        with_audit(
            vec![
                ("project_id", kv(d.project_id)),
                ("title", tv(&d.title)),
                ("body", tv(&d.body)),
                ("status", tv(d.status.as_str())),
                ("decided_at", tsov(&d.decided_at)),
                ("decided_by", ov(&d.decided_by)),
            ],
            &d.created_at,
            &d.updated_at,
        ),
    )
}

pub fn decision_edge(e: &DecisionEdge) -> Record {
    Record::new(
        "decision_edge",
        e.id,
        with_audit(
            vec![
                ("decision_id", kv(e.decision_id)),
                ("target_decision_id", kv(e.target_decision_id)),
                ("kind", tv(e.kind.as_str())),
            ],
            &e.created_at,
            &e.updated_at,
        ),
    )
}

pub fn decision_task_link(l: &DecisionTaskLink) -> Record {
    Record::new(
        "decision_task_link",
        l.id,
        with_audit(
            vec![("decision_id", kv(l.decision_id)), ("task_id", kv(l.task_id))],
            &l.created_at,
            &l.updated_at,
        ),
    )
}

pub fn dimension(d: &Dimension) -> Record {
    Record::new(
        "dimension",
        d.id,
        with_audit(
            vec![
                ("project_id", kv(d.project_id)),
                ("name", tv(&d.name)),
                ("notes", tv(&d.notes)),
                ("cardinality", tv(d.cardinality.as_str())),
                ("ordered", bv(d.ordered)),
                ("role", tv(d.role.as_str())),
                ("order_key", tv(&d.order_key)),
            ],
            &d.created_at,
            &d.updated_at,
        ),
    )
}

pub fn dimension_value(v: &DimensionValue) -> Record {
    Record::new(
        "dimension_value",
        v.id,
        with_audit(
            vec![
                ("dimension_id", kv(v.dimension_id)),
                ("name", tv(&v.name)),
                ("order_key", tv(&v.order_key)),
                ("start_on", dov(&v.start_on)),
                ("end_on", dov(&v.end_on)),
            ],
            &v.created_at,
            &v.updated_at,
        ),
    )
}

pub fn task_dimension_value(v: &TaskDimensionValue) -> Record {
    Record::new(
        "task_dimension_value",
        v.id,
        with_audit(
            vec![
                ("task_id", kv(v.task_id)),
                ("dimension_id", kv(v.dimension_id)),
                ("value_id", kv(v.value_id)),
            ],
            &v.created_at,
            &v.updated_at,
        ),
    )
}

pub fn task_comment(c: &TaskComment) -> Record {
    Record::new(
        "task_comment",
        c.id,
        with_audit(
            vec![
                ("task_id", kv(c.task_id)),
                ("author_kind", kov(&c.author_kind)),
                ("text", tv(&c.text)),
                ("edited_at", tsov(&c.edited_at)),
            ],
            &c.created_at,
            &c.updated_at,
        ),
    )
}

pub fn decision_comment(c: &DecisionComment) -> Record {
    Record::new(
        "decision_comment",
        c.id,
        with_audit(
            vec![
                ("decision_id", kv(c.decision_id)),
                ("author_kind", kov(&c.author_kind)),
                ("text", tv(&c.text)),
                ("edited_at", tsov(&c.edited_at)),
            ],
            &c.created_at,
            &c.updated_at,
        ),
    )
}

pub fn attachment(a: &Attachment) -> Record {
    Record::new(
        "attachment",
        a.id,
        with_audit(
            vec![
                ("target_type", tv(a.target_type.as_str())),
                ("target_id", kv(a.target_id)),
                ("kind", tv(a.kind.as_str())),
                ("blob_hash", ov(&a.blob_hash)),
                ("filename", ov(&a.filename)),
                ("mime", ov(&a.mime)),
                ("size_bytes", a.size_bytes.map(iv).unwrap_or(Value::Null)),
                ("url", ov(&a.url)),
                ("created_by_kind", kov(&a.created_by_kind)),
                ("order_key", tv(&a.order_key)),
            ],
            &a.created_at,
            &a.updated_at,
        ),
    )
}

/// Flush a whole [`Database`] into the truth source — **a fixture-only entry point**. The mapping is
/// still the one `Record` projection above: all this does is list *which collections* to flush, and it
/// knows nothing of columns or values. So **a test writes through the same mapping production does**,
/// and a fixture cannot drift away from it. Production writes do not come through here (an op writes one
/// record at a time, through [`crate::ops`]'s `emit_create` / `emit_update`). What needs this is the case
/// where **the model is built first and the reads are then tried against it**: `hydrate`'s round-trip
/// oracle, and the scale seeding (`tests/common`) that would be O(N²) if each row went through `ops`.
/// The device-local overview tables (bindings, read receipts, …) are not part of a `Database`, so they
/// are not written — which is why the resulting DB **must never be swapped in as a live truth source**.
pub fn put_database(tx: &super::WriteTx<'_>, db: &Database) -> super::Result<()> {
    fn put(tx: &super::WriteTx<'_>, r: Record) -> super::Result<()> {
        tx.put_record(r.dataset, r.id, &r.fields)
    }
    for x in &db.projects {
        put(tx, project(x))?;
    }
    for x in &db.tasks {
        put(tx, task(x))?;
    }
    for x in &db.task_dependencies {
        put(tx, dependency(x))?;
    }
    for x in &db.task_commits {
        put(tx, task_commit(x))?;
    }
    for x in &db.decisions {
        put(tx, decision(x))?;
    }
    for x in &db.decision_edges {
        put(tx, decision_edge(x))?;
    }
    for x in &db.decision_task_links {
        put(tx, decision_task_link(x))?;
    }
    for x in &db.dimensions {
        put(tx, dimension(x))?;
    }
    for x in &db.dimension_values {
        put(tx, dimension_value(x))?;
    }
    for x in &db.task_dimension_values {
        put(tx, task_dimension_value(x))?;
    }
    for x in &db.task_comments {
        put(tx, task_comment(x))?;
    }
    for x in &db.decision_comments {
        put(tx, decision_comment(x))?;
    }
    for x in &db.attachments {
        put(tx, attachment(x))?;
    }
    // Store-level scalars have no per-record dataset, so they go to the store_meta KV table.
    tx.set_meta(super::META_SCHEMA_VERSION, Some(&db.schema_version))?;
    Ok(())
}
