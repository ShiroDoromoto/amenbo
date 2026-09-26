//! **Which project an entity belongs to.** Every reach check, on the read side as much as the write
//! side, starts by asking "which project does this id belong to?" — and this is the only place that
//! answers it.
//!
//! The lookup differs per entity: tasks and decisions carry their project themselves, but a comment's
//! project comes from its parent (task or decision), an attachment's from its target (task, decision, a
//! comment on either, or one execution of one step of an automation run), and a dimension value's from
//! its dimension. If the write side
//! ([`super::write_reach`]) and the read side ([`super::read`]) each kept their own copy of how to walk
//! that, we would eventually fix one and leave the other wide open.
//!
//! **An id nothing answers is [`Owner::Missing`], not a row in no project.** The two used to share `None`,
//! and a narrowed reach turned a missing id away as `out_of_reach` — telling the reader the row sat in
//! another project when there was no row at all. A missing id now passes the reach check and the
//! operation itself answers `not_found`, as it does for a human (`AMB-D-986`). What the reach keeps out is
//! a row's content (`AMB-D-222`); whether an id is taken is not content, and a task or decision ref
//! already tells the two apart.

use rusqlite::Connection;

use crate::error::Result;
use crate::model::{
    AttachmentTarget, AutomationCfgOwner, AutomationOwner, AutomationPictureOwner,
};
use crate::reach::Reach;
use crate::store_engine::read;

/// What an id answers when asked which project it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Owner {
    /// Nothing answers to the id.
    Missing,
    /// A row answers, in this project — or in none (`None`): an unplaced task, an action on the
    /// device's own shelf.
    Row(Option<i64>),
}

impl Owner {
    fn of(project: Option<Option<i64>>) -> Owner {
        match project {
            Some(p) => Owner::Row(p),
            None => Owner::Missing,
        }
    }

    /// The project the row is in; `None` for a row in no project and for an id nothing answers alike.
    pub(super) fn project(self) -> Option<i64> {
        match self {
            Owner::Row(p) => p,
            Owner::Missing => None,
        }
    }

    /// Is the row within reach? An id nothing answers passes, so the operation that goes on to look it
    /// up answers `not_found` rather than this answering `out_of_reach` (`AMB-D-986`).
    pub(super) fn check(self, reach: Reach, what: &str) -> Result<()> {
        match self {
            Owner::Missing => Ok(()),
            Owner::Row(p) => reach.check(what, p),
        }
    }
}

pub(super) fn task(conn: &Connection, id: i64) -> Result<Owner> {
    Ok(Owner::of(read::task_project(conn, id).map_err(crate::error::engine_on(conn))?))
}

pub(super) fn decision(conn: &Connection, id: i64) -> Result<Owner> {
    Ok(Owner::of(
        read::decision_project(conn, id).map_err(crate::error::engine_on(conn))?.map(Some),
    ))
}

pub(super) fn task_comment(conn: &Connection, id: i64) -> Result<Owner> {
    match read::task_comment(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(c) => task(conn, c.task_id),
        None => Ok(Owner::Missing),
    }
}

pub(super) fn decision_comment(conn: &Connection, id: i64) -> Result<Owner> {
    match read::decision_comment(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(c) => decision(conn, c.decision_id),
        None => Ok(Owner::Missing),
    }
}

pub(super) fn dimension(conn: &Connection, id: i64) -> Result<Owner> {
    Ok(Owner::of(
        read::dimension(conn, id).map_err(crate::error::engine_on(conn))?.map(|d| Some(d.project_id)),
    ))
}

pub(super) fn dimension_value(conn: &Connection, value_id: i64) -> Result<Owner> {
    match read::dimension_id_of_value(conn, value_id).map_err(crate::error::engine_on(conn))? {
        Some(dimension_id) => dimension(conn, dimension_id),
        None => Ok(Owner::Missing),
    }
}

/// The project an automation is built in.
pub(super) fn automation(conn: &Connection, id: i64) -> Result<Owner> {
    Ok(Owner::of(
        read::automation(conn, id).map_err(crate::error::engine_on(conn))?.map(|a| Some(a.project_id)),
    ))
}

/// The project a library action is in. A row in no project is the device's own library, outside a
/// narrowed reach for a write — an AI bound to one project writes that project's library, and the
/// device's is a human's to keep.
pub(super) fn automation_action(conn: &Connection, id: i64) -> Result<Owner> {
    Ok(Owner::of(
        read::automation_action(conn, id).map_err(crate::error::engine_on(conn))?.map(|a| a.project_id),
    ))
}

/// The project a step is in: the library action holding it says, the same way the action's own reach
/// does — so a step of a device-held action is nobody's project's.
pub(super) fn automation_step(conn: &Connection, id: i64) -> Result<Owner> {
    match read::automation_action_step(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(s) => automation_action(conn, s.action_id),
        None => Ok(Owner::Missing),
    }
}

/// The project a placement is on — its automation's.
pub(super) fn automation_placement(conn: &Connection, id: i64) -> Result<Owner> {
    match read::automation_placement(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(p) => automation(conn, p.automation_id),
        None => Ok(Owner::Missing),
    }
}

/// The project behind whichever of the two declared a way out or a port.
fn automation_declarer(
    conn: &Connection,
    owner_kind: AutomationOwner,
    owner_id: i64,
) -> Result<Owner> {
    match owner_kind {
        AutomationOwner::Step => automation_step(conn, owner_id),
        AutomationOwner::Action => automation_action(conn, owner_id),
    }
}

pub(super) fn automation_exit(conn: &Connection, id: i64) -> Result<Owner> {
    match read::automation_exit(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(x) => automation_declarer(conn, x.owner_kind, x.owner_id),
        None => Ok(Owner::Missing),
    }
}

pub(super) fn automation_port(conn: &Connection, id: i64) -> Result<Owner> {
    match read::automation_port(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(p) => match p.owner_kind.declarer() {
            Some(declarer) => automation_declarer(conn, declarer, p.owner_id),
            None => automation_exit(conn, p.owner_id),
        },
        None => Ok(Owner::Missing),
    }
}

/// The project behind whichever half of a setting this row is: the action's declaration, or one
/// placement's answer to it.
fn automation_cfg_owner(
    conn: &Connection,
    owner_kind: AutomationCfgOwner,
    owner_id: i64,
) -> Result<Owner> {
    match owner_kind {
        AutomationCfgOwner::Action => automation_action(conn, owner_id),
        AutomationCfgOwner::Placement => automation_placement(conn, owner_id),
    }
}

/// The project a picture is drawn in: an automation's own, or the one holding the library action.
fn automation_picture(
    conn: &Connection,
    owner_kind: AutomationPictureOwner,
    owner_id: i64,
) -> Result<Owner> {
    match owner_kind {
        AutomationPictureOwner::Automation => automation(conn, owner_id),
        AutomationPictureOwner::Action => automation_action(conn, owner_id),
    }
}

pub(super) fn automation_cfg(conn: &Connection, id: i64) -> Result<Owner> {
    match read::automation_cfg(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(c) => automation_cfg_owner(conn, c.owner_kind, c.owner_id),
        None => Ok(Owner::Missing),
    }
}

pub(super) fn automation_edge(conn: &Connection, id: i64) -> Result<Owner> {
    match read::automation_edge(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(e) => automation_picture(conn, e.owner_kind, e.owner_id),
        None => Ok(Owner::Missing),
    }
}

pub(super) fn automation_wire(conn: &Connection, id: i64) -> Result<Owner> {
    match read::automation_wire(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(w) => automation_picture(conn, w.owner_kind, w.owner_id),
        None => Ok(Owner::Missing),
    }
}

/// The project a run was launched from. It is carried on the run's own row rather than walked back
/// through the automation, so a run is still reachable after the automation it came from is archived.
pub(super) fn automation_run(conn: &Connection, id: i64) -> Result<Owner> {
    Ok(Owner::of(
        read::automation_run(conn, id).map_err(crate::error::engine_on(conn))?.map(|r| Some(r.project_id)),
    ))
}

/// The project a step execution is filed under — the run's, since a run is filed under a project of
/// its own and not only under the automation it was launched from.
pub(super) fn automation_run_step(conn: &Connection, id: i64) -> Result<Owner> {
    match read::automation_run_step_run(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(run_id) => automation_run(conn, run_id),
        None => Ok(Owner::Missing),
    }
}

pub(super) fn attachment(conn: &Connection, id: i64) -> Result<Owner> {
    match read::attachment(conn, id).map_err(crate::error::engine_on(conn))? {
        Some(a) => attach_target(conn, a.target_type, a.target_id),
        None => Ok(Owner::Missing),
    }
}

/// The project owning an attachment's target (the polymorphic `target_type` + id).
pub(super) fn attach_target(
    conn: &Connection,
    kind: AttachmentTarget,
    id: i64,
) -> Result<Owner> {
    match kind {
        AttachmentTarget::Task => task(conn, id),
        AttachmentTarget::Decision => decision(conn, id),
        AttachmentTarget::TaskComment => task_comment(conn, id),
        AttachmentTarget::DecisionComment => decision_comment(conn, id),
        AttachmentTarget::AutomationRunStep => automation_run_step(conn, id),
        AttachmentTarget::AutomationRun => automation_run(conn, id),
    }
}

/// Render an attachment's target as the display ref an error message can quote. The mapping from the
/// polymorphic pair to a ref space belongs to the target itself ([`AttachmentTarget::ref_kind`]), so the
/// cases are written once and every reader that has to name a target quotes the same ref.
pub(super) fn attach_target_ref(kind: AttachmentTarget, id: i64) -> String {
    kind.target_ref(id)
}
