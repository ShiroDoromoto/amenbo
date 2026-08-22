//! `dimension`: the classification axes a project declares, their values, and placing tasks on them.

use chrono::NaiveDate;
use serde_json::json;

use amenbo_core::Store;

use crate::cli::*;
use crate::cmd::arg::{parse_date_opt, pos_from_keys};
use crate::cmd::labels::{dimension_label, dimension_value_label, task_label};
use crate::cmd::place::project_or_bound;
use crate::cmd::task::resolve_task;
use crate::output::{confirm, human, print_json, write_envelope, CliError, Flags};

/// The CLI surface of the unified dimension model. The axes themselves (purely user-defined), their values,
/// and their assignment to tasks are all delegated to `ops::dimension`. An axis resolves by id prefix or by
/// name (`resolve_in`); a value resolves within the dimension it belongs to (`resolve_value_in`), because a
/// value's name is only unique inside its own axis.
pub(crate) fn dimension(store: &mut Store, flags: &Flags, sub: DimensionCmd) -> Result<i32, CliError> {
    use amenbo_core::model::{DimensionCardinality, DimensionRole};
    use amenbo_core::ops::dimension::NewDimension;
    // A dimension's kind on one human-readable line (single, ordered, time-axis, show-on-card,
    // required).
    fn kind_line(
        cardinality: DimensionCardinality,
        ordered: bool,
        role: DimensionRole,
        show_on_card: bool,
        required: bool,
    ) -> String {
        let mut s = cardinality.as_str().to_string();
        if ordered {
            s.push_str(", ordered");
        }
        if matches!(role, DimensionRole::TimeAxis) {
            s.push_str(", time-axis");
        }
        if show_on_card {
            s.push_str(", show-on-card");
        }
        if required {
            s.push_str(", required");
        }
        s
    }
    /// A value's period `[start_on, end_on]` (both ends inclusive) on one human-readable line. An open end
    /// reads `…` at the start and `ongoing` at the finish. With neither end set there is no period at all,
    /// and nothing is shown.
    fn period_line(v: &amenbo_core::model::DimensionValue) -> Option<String> {
        let (s, e) = (v.start_on, v.end_on);
        if s.is_none() && e.is_none() {
            return None;
        }
        let fmt = |d: Option<NaiveDate>, open: &str| d.map(|d| d.to_string()).unwrap_or_else(|| open.to_string());
        Some(format!("[{} → {}]", fmt(s, "…"), fmt(e, "ongoing")))
    }
    /// A period is the payload of the time_axis role, not a general feature of every axis. Core writes the
    /// physical columns as told, so the CLI surface is what guards the role.
    fn ensure_time_axis(store: &Store, dimension_id: i64) -> Result<(), CliError> {
        let role = store.dimension(dimension_id).map_err(CliError::from)?.map(|d| d.role);
        if matches!(role, Some(DimensionRole::TimeAxis)) {
            return Ok(());
        }
        Err(CliError::from(amenbo_core::Error::invalid(
            "only a time-axis dimension's values carry a period; mark the axis with --time-axis first",
        )))
    }
    /// Build a value's new period from `--start`/`--end` (a new end) and `--clear-*` (open an end). An end
    /// given by neither keeps its current value, which makes this a partial update.
    fn merged_period(
        cur: &amenbo_core::model::DimensionValue,
        start: Option<NaiveDate>,
        end: Option<NaiveDate>,
        clear_start: bool,
        clear_end: bool,
    ) -> (Option<NaiveDate>, Option<NaiveDate>) {
        let s = if clear_start { None } else { start.or(cur.start_on) };
        let e = if clear_end { None } else { end.or(cur.end_on) };
        (s, e)
    }
    match sub {
        DimensionCmd::Add { project, name, notes, ordered, time_axis, show_on_card, required } => {
            let pid = project_or_bound(store, project)?;
            let new = NewDimension {
                name,
                notes,
                // A classification axis is single-select, always.
                cardinality: DimensionCardinality::Single,
                ordered,
                role: if time_axis { DimensionRole::TimeAxis } else { DimensionRole::None },
                show_on_card,
                // Core refuses this while the axis has no values, which a new one never does. The flag
                // is here because `AMB-D-734` names this door as one of the two: passing it is how the
                // refusal — which says to add a value first — reaches the person who tried.
                required,
            };
            let d = store.dimension_add(pid, new).map_err(CliError::from)?;
            write_envelope(flags, "dimension.add", "dimension", serde_json::to_value(&d).unwrap(), None, false, format!("✓ Created dimension: {} ({})", d.name, dimension_label(d.id)));
        }
        DimensionCmd::List { project } => {
            let pid = project_or_bound(store, project)?;
            let dims: Vec<_> = store.dimensions(pid).map_err(CliError::from)?;
            if flags.json {
                let mut out: Vec<serde_json::Value> = Vec::with_capacity(dims.len());
                for d in &dims {
                    let values = store.dimension_values(d.id).map_err(CliError::from)?;
                    out.push(json!({ "dimension": serde_json::to_value(d).unwrap(), "values": values }));
                }
                print_json(&json!({ "count": out.len(), "dimensions": out }));
            } else {
                human(flags, format!("{} dimension(s)", dims.len()));
                for d in &dims {
                    let vals = store.dimension_values(d.id).map_err(CliError::from)?;
                    human(flags, format!("  {}  {} [{}]  {} value(s)", dimension_label(d.id), d.name, kind_line(d.cardinality, d.ordered, d.role, d.show_on_card, d.required), vals.len()));
                    for v in &vals {
                        let period = period_line(v).map(|p| format!("  {p}")).unwrap_or_default();
                        human(flags, format!("      {}  {}{}", dimension_value_label(v.id), v.name, period));
                    }
                }
            }
        }
        DimensionCmd::Show { id } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            let d = store
                .dimension(did)
                .map_err(CliError::from)?
                
                .ok_or_else(|| { let r = dimension_label(did); CliError::from(amenbo_core::Error::not_found(format!("dimension '{r}' not found"))) })?;
            let vals: Vec<_> = store.dimension_values(did).map_err(CliError::from)?;
            if flags.json {
                print_json(&json!({ "dimension": serde_json::to_value(&d).unwrap(), "values": serde_json::to_value(&vals).unwrap() }));
            } else {
                human(flags, format!("{}  {}", dimension_label(d.id), d.name));
                human(flags, format!("kind: {}", kind_line(d.cardinality, d.ordered, d.role, d.show_on_card, d.required)));
                if d.notes.trim().is_empty() {
                    human(flags, "notes: (none)");
                } else {
                    human(flags, format!("notes:\n{}", d.notes));
                }
                human(flags, format!("{} value(s)", vals.len()));
                for v in &vals {
                    let period = period_line(v).map(|p| format!("  {p}")).unwrap_or_default();
                    human(flags, format!("  {}  {}{}", dimension_value_label(v.id), v.name, period));
                }
            }
        }
        DimensionCmd::Update { id, name, notes, ordered, time_axis, show_on_card, required } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            let mut changed = Vec::new();
            if name.is_some() {
                changed.push("name".to_string());
            }
            if notes.is_some() {
                changed.push("notes".to_string());
            }
            if ordered.is_some() {
                changed.push("ordered".to_string());
            }
            if time_axis.is_some() {
                changed.push("role".to_string());
            }
            if show_on_card.is_some() {
                changed.push("show_on_card".to_string());
            }
            if required.is_some() {
                changed.push("required".to_string());
            }
            let role = time_axis
                .map(|on| if on { DimensionRole::TimeAxis } else { DimensionRole::None });
            let d = store.dimension_update(did, name.as_deref(), notes.as_deref(), ordered, role, show_on_card, required).map_err(CliError::from)?;
            write_envelope(flags, "dimension.update", "dimension", serde_json::to_value(&d).unwrap(), Some(changed), false, format!("✓ Updated dimension: {}", dimension_label(d.id)));
        }
        DimensionCmd::Move { id, before, after, top, bottom } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            let before = before.map(|b| store.resolve_dimension(None, &b)).transpose().map_err(CliError::from)?;
            let after = after.map(|a| store.resolve_dimension(None, &a)).transpose().map_err(CliError::from)?;
            let pos = pos_from_keys(top, bottom, before, after)?;
            let d = store.dimension_move(did, pos).map_err(CliError::from)?;
            write_envelope(flags, "dimension.move", "dimension", serde_json::to_value(&d).unwrap(), Some(vec!["order_key".to_string()]), false, format!("✓ Moved dimension: {}", dimension_label(d.id)));
        }
        DimensionCmd::Rm { id } => {
            let did = store.resolve_dimension(None, &id).map_err(CliError::from)?;
            if !confirm(flags, "delete dimension")? {
                return Ok(0);
            }
            store.dimension_delete(did).map_err(CliError::from)?;
            write_envelope(flags, "dimension.rm", "dimension", json!({ "id": did, "deleted": true }), None, false, format!("✓ Deleted dimension: {}", dimension_label(did)));
        }
        DimensionCmd::ValueAdd { dimension, name, start, end } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let start_on = parse_date_opt(&start)?;
            let end_on = parse_date_opt(&end)?;
            let dated = start_on.is_some() || end_on.is_some();
            if dated {
                ensure_time_axis(store, did)?;
            }
            let period = dated.then_some((start_on, end_on));
            let v = store.dimension_value_add(did, &name, period).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-add", "dimension_value", serde_json::to_value(&v).unwrap(), None, false, format!("✓ Added value: {} ({})", v.name, dimension_value_label(v.id)));
        }
        DimensionCmd::ValueUpdate { dimension, value, name, start, end, clear_start, clear_end } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let start_on = parse_date_opt(&start)?;
            let end_on = parse_date_opt(&end)?;
            let mut changed = Vec::new();
            if name.is_some() {
                changed.push("name".to_string());
            }
            if start.is_some() || clear_start {
                changed.push("start_on".to_string());
            }
            if end.is_some() || clear_end {
                changed.push("end_on".to_string());
            }
            let touches_period = start.is_some() || end.is_some() || clear_start || clear_end;
            if touches_period {
                ensure_time_axis(store, did)?;
            }
            let cur = store.dimension_value(vid).map_err(CliError::from)?.ok_or_else(|| {
                CliError::from(amenbo_core::Error::not_found(format!("dimension value '{vid}' not found")))
            })?;
            let period = touches_period
                .then(|| merged_period(&cur, start_on, end_on, clear_start, clear_end));
            let v = store.dimension_value_update(vid, name.as_deref(), period).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-update", "dimension_value", serde_json::to_value(&v).unwrap(), Some(changed), false, format!("✓ Updated value: {}", dimension_value_label(v.id)));
        }
        DimensionCmd::ValueMove { dimension, value, before, after, top, bottom } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let before = before.map(|b| store.resolve_dimension_value(did, &b)).transpose().map_err(CliError::from)?;
            let after = after.map(|a| store.resolve_dimension_value(did, &a)).transpose().map_err(CliError::from)?;
            let pos = pos_from_keys(top, bottom, before, after)?;
            let v = store.dimension_value_move(vid, pos).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-move", "dimension_value", serde_json::to_value(&v).unwrap(), Some(vec!["order_key".to_string()]), false, format!("✓ Moved value: {}", dimension_value_label(v.id)));
        }
        DimensionCmd::ValueRm { dimension, value } => {
            let did = store.resolve_dimension(None, &dimension).map_err(CliError::from)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            if !confirm(flags, "delete dimension value")? {
                return Ok(0);
            }
            store.dimension_value_delete(vid).map_err(CliError::from)?;
            write_envelope(flags, "dimension.value-rm", "dimension_value", json!({ "id": vid, "deleted": true }), None, false, format!("✓ Deleted value: {}", dimension_value_label(vid)));
        }
        DimensionCmd::Set { task, dimension, value } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let did = resolve_axis_of_task(store, tid, &dimension)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let (tv, changed) = store.set_task_dimension_value(tid, vid).map_err(CliError::from)?;
            write_envelope(flags, "dimension.set", "task_dimension_value", serde_json::to_value(&tv).unwrap(), None, !changed, format!("✓ Set value on task {}", task_label(tid)));
        }
        DimensionCmd::Unset { task, dimension, value } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let did = resolve_axis_of_task(store, tid, &dimension)?;
            let vid = store.resolve_dimension_value(did, &value).map_err(CliError::from)?;
            let removed = store.unset_task_dimension_value(tid, vid).map_err(CliError::from)?;
            write_envelope(flags, "dimension.unset", "task_dimension_value", json!({ "task_id": tid, "value_id": vid, "removed": removed }), None, !removed, format!("✓ Cleared value on task {}", task_label(tid)));
        }
    }
    Ok(0)
}

/// Resolve the axis named by `dimension set` / `unset` **inside the task's own project**. An axis belongs to
/// one project and an assignment never crosses projects, so the task — resolved a line above — is what says
/// which axis a bare name means; without it, a name a second project happens to use as well reads as
/// `ambiguous` when only one of the two could ever have been assigned here. Same narrowing `task add --dim`
/// already does ([`resolve_dim_pairs`](crate::cmd::place::resolve_dim_pairs)). An unfiled task has no project to narrow by, so its axis resolves
/// across the store as before.
fn resolve_axis_of_task(store: &Store, task_id: i64, reference: &str) -> Result<i64, CliError> {
    let project_id = store.task(task_id).map_err(CliError::from)?.and_then(|t| t.project_id);
    store.resolve_dimension(project_id, reference).map_err(CliError::from)
}
