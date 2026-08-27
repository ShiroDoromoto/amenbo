//! **Which project an entity belongs to.** Every reach check, on the read side as much as the write
//! side, starts by asking "which project does this id belong to?" — and this is the only place that
//! answers it.
//!
//! The lookup differs per entity: tasks and decisions carry their project themselves, but a comment's
//! project comes from its parent (task or decision), an attachment's from its target (task, decision, or
//! a comment on either), and a dimension value's from its dimension. If the write side
//! ([`super::write_reach`]) and the read side ([`super::read`]) each kept their own copy of how to walk
//! that, we would eventually fix one and leave the other wide open.
//!
//! **An id that does not exist has no owner** (`None`), so a narrowed reach cannot touch it. That is the
//! same discipline that keeps out-of-reach from degrading into `not_found`: a missing id also answers
//! "you cannot reach this", neither confirming nor denying that it exists.

use rusqlite::Connection;

use crate::error::Result;
use crate::model::AttachmentTarget;
use crate::store_engine::read;

pub(super) fn task(conn: &Connection, id: i64) -> Result<Option<i64>> {
    read::task_project(conn, id).map_err(crate::error::engine_on(conn))
}

pub(super) fn decision(conn: &Connection, id: i64) -> Result<Option<i64>> {
    read::decision_project(conn, id).map_err(crate::error::engine_on(conn))
}

pub(super) fn task_comment(conn: &Connection, id: i64) -> Result<Option<i64>> {
    match read::task_comment(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(c) => task(conn, c.task_id),
        None => Ok(None),
    }
}

pub(super) fn decision_comment(conn: &Connection, id: i64) -> Result<Option<i64>> {
    match read::decision_comment(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(c) => decision(conn, c.decision_id),
        None => Ok(None),
    }
}

pub(super) fn dimension(conn: &Connection, id: i64) -> Result<Option<i64>> {
    Ok(read::dimension(conn, id)
        .map_err(crate::error::engine_on(conn))?
        .map(|d| d.project_id))
}

pub(super) fn dimension_value(conn: &Connection, value_id: i64) -> Result<Option<i64>> {
    match read::dimension_id_of_value(conn, value_id).map_err(crate::error::engine_on(conn))? {
        Some(dimension_id) => dimension(conn, dimension_id),
        None => Ok(None),
    }
}

pub(super) fn attachment(conn: &Connection, id: i64) -> Result<Option<i64>> {
    match read::attachment(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(a) => attach_target(conn, a.target_type, a.target_id),
        None => Ok(None),
    }
}

/// The project owning an attachment's target (the polymorphic `target_type` + id).
pub(super) fn attach_target(
    conn: &Connection,
    kind: AttachmentTarget,
    id: i64,
) -> Result<Option<i64>> {
    match kind {
        AttachmentTarget::Task => task(conn, id),
        AttachmentTarget::Decision => decision(conn, id),
        AttachmentTarget::TaskComment => task_comment(conn, id),
        AttachmentTarget::DecisionComment => decision_comment(conn, id),
    }
}

/// Render an attachment's target as the display ref an error message can quote. The mapping from the
/// polymorphic pair to a ref space belongs to the target itself ([`AttachmentTarget::ref_kind`]), so the
/// four cases are written once and every reader that has to name a target quotes the same ref.
pub(super) fn attach_target_ref(kind: AttachmentTarget, id: i64) -> String {
    kind.target_ref(id)
}
