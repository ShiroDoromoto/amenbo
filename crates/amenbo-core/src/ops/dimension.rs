//! Operations on the unified dimension model.
//!
//! Every axis a task is classified along — phase, category, tag, or anything a user invents —
//! funnels into a single "dimension" mechanism. A dimension is a plain classification axis, and all
//! of them are user-editable alike: no built-in fixed axes, no locked values, no seeding (status
//! and priority are first-class task attributes instead).
//!
//! The operations are add/list/show/update/move/delete. Renaming is the `name` argument of
//! `update` (there is no dedicated `rename`). Values are handled by `value_*`, and assignment to a
//! task by set/unset.
//!
//! **Writes go straight to SQL through the engine.** Every mutator takes a [`WriteTx`]
//! (`BEGIN IMMEDIATE`) opened by the caller and does its read-then-write — the `order_key` of the
//! ordering siblings — **inside that same transaction**. The cascading deletes (dimension → value →
//! assignment) ride on one transaction too: apply them partially and you leave values hanging off a
//! dimension that is gone. The transaction is opened by the write wrappers on [`crate::Store`], and
//! that is all the CLI/GUI ever calls.

use chrono::NaiveDate;

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::model::{
    Dimension, DimensionCardinality, DimensionRole, DimensionValue,
    TaskDimensionValue,
};
use crate::ops::{emit_create, emit_update, place, Noun, Position};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// The noun for the dimension entity (the English/Japanese pair used in not_found messages).
pub(crate) const NOUN: Noun = Noun { en: "dimension", code: ErrorCode::NotFoundDimension };
/// The noun for the dimension-value entity.
pub(crate) const VALUE_NOUN: Noun = Noun { en: "dimension value", code: ErrorCode::NotFoundDimensionValue };

/// The specification of a new dimension. The defaults — single-select, unordered, no role — are the
/// bare shape of a user-defined axis. A time-axis phase is built by setting `role=TimeAxis`.
#[derive(Clone, Debug)]
pub struct NewDimension {
    pub name: String,
    pub notes: String,
    pub cardinality: DimensionCardinality,
    pub ordered: bool,
    pub role: DimensionRole,
}

impl Default for NewDimension {
    fn default() -> Self {
        NewDimension {
            name: String::new(),
            notes: String::new(),
            cardinality: DimensionCardinality::Single,
            ordered: false,
            role: DimensionRole::None,
        }
    }
}

// ───────────────────────────── Dimensions (the axis itself) ─────────────────────────────

pub fn add(tx: &WriteTx<'_>, project_id: i64, new: NewDimension) -> Result<Dimension> {
    if new.name.trim().is_empty() {
        return Err(Error::invalid("a dimension name cannot be empty"));
    }
    if read::project(tx.conn(), project_id)?.is_none() {
        return Err(crate::ops::project::NOUN.not_found(project_id.to_string()));
    }
    let sibs = read::dimension_siblings(tx.conn(), project_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let dimension = Dimension {
        id: read::next_id(tx.conn(), "dimension")?,
        project_id,
        name: new.name.trim().to_string(),
        notes: new.notes,
        cardinality: new.cardinality,
        ordered: new.ordered,
        role: new.role,
        order_key,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::dimension(&dimension))?;
    Ok(dimension)
}

/// Read a live dimension's `before` snapshot **from this transaction**.
fn live_before(tx: &WriteTx<'_>, id: i64) -> Result<Dimension> {
    read::dimension(tx.conn(), id)?
        
        .ok_or_else(|| NOUN.not_found(id.to_string()))
}

/// Read a live dimension value's `before` snapshot **from this transaction**.
fn live_value_before(tx: &WriteTx<'_>, id: i64) -> Result<DimensionValue> {
    read::dimension_value(tx.conn(), id)?
        
        .ok_or_else(|| VALUE_NOUN.not_found(id.to_string()))
}

/// Update a dimension's name, notes, whether its values are ordered (`ordered`) and its role
/// (`role`). Only the `Some` fields are written. The name is a display label, so it is free to
/// change. Flipping `ordered` false→true brings the values' `order_key` into play (making
/// `value_move` possible); true→false drops them back to a stable ascending-id order (`order_key`
/// is not cleared, so flipping it on again revives the old arrangement). `role` is a nomination:
/// set `TimeAxis` and that axis's values carry the periods and decide the "current era"; set it
/// back to `None` and the dates stay in their columns but stop meaning anything. **Uniqueness of
/// the nomination is not enforced**: even if several axes call themselves the time axis,
/// [`read::current_time_axis_value`] folds them deterministically down to one by dimension order
/// (`add`'s `--time-axis` is just as unchecked, so the same rule holds whichever door you came in
/// by).
pub fn update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    notes: Option<&str>,
    ordered: Option<bool>,
    role: Option<DimensionRole>,
) -> Result<Dimension> {
    if let Some(n) = name {
        if n.trim().is_empty() {
            return Err(Error::invalid("a dimension name cannot be empty"));
        }
    }
    let before = live_before(tx, id)?;
    let mut d = before.clone();
    if let Some(n) = name {
        d.name = n.trim().to_string();
    }
    if let Some(t) = notes {
        d.notes = t.to_string();
    }
    if let Some(o) = ordered {
        d.ordered = o;
    }
    if let Some(r) = role {
        d.role = r;
    }
    d.updated_at = Timestamp::now();
    emit_update(tx, record::dimension(&before), record::dimension(&d))?;
    Ok(d)
}

pub fn move_to(tx: &WriteTx<'_>, id: i64, pos: Position) -> Result<Dimension> {
    let before = live_before(tx, id)?;
    let sibs = read::dimension_siblings(tx.conn(), before.project_id, Some(id))?;
    let key = place(&sibs, &pos)?;
    let after = Dimension { order_key: key, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::dimension(&before), record::dimension(&after))?;
    Ok(after)
}

/// Hard-delete a dimension, its values (`dimension_value`) and the task assignments on them
/// (`task_dimension_value`). Dimensions are all user-editable alike, so deleting one is equally free.
pub fn delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let before = live_before(tx, id)?;
    delete_subtree(tx, before.id)
}

/// Hard-delete one dimension and its children (pass an id whose existence has already been checked).
/// This is the body of [`delete`], and [`crate::ops::project::delete`] uses it to clear out a project's
/// dimensions. The op deletes each child itself, child-first — an axis and the classification a task
/// carries on it are both things a person can point at, so what goes has to go through code
/// (`AMB-D-403`). Sweeping value by value covers the whole axis: an assignment names a value, and that
/// value's axis is this one.
pub(crate) fn delete_subtree(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    for value_id in read::dimension_value_ids(tx.conn(), id)? {
        delete_value_subtree(tx, value_id)?;
    }
    tx.delete_record("dimension", id)?;
    Ok(())
}

// ───────────────────────────── Dimension values (the choices on an axis) ─────────────────────────────

pub fn value_add(tx: &WriteTx<'_>, dimension_id: i64, name: &str) -> Result<DimensionValue> {
    if name.trim().is_empty() {
        return Err(Error::invalid("a dimension value name cannot be empty"));
    }
    live_before(tx, dimension_id)?;
    // Placed at the bottom whether or not the axis is ordered; when unordered it is merely carried
    // as a stable key (see the model).
    let sibs = read::dimension_value_siblings(tx.conn(), dimension_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let value = DimensionValue {
        id: read::next_id(tx.conn(), "dimension_value")?,
        dimension_id,
        name: name.trim().to_string(),
        order_key,
        // Periods are filled in later with `value_set_dates`; a fresh value has none.
        start_on: None,
        end_on: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::dimension_value(&value))?;
    Ok(value)
}

pub fn value_rename(tx: &WriteTx<'_>, value_id: i64, name: &str) -> Result<DimensionValue> {
    if name.trim().is_empty() {
        return Err(Error::invalid("a dimension value name cannot be empty"));
    }
    let before = live_value_before(tx, value_id)?;
    live_before(tx, before.dimension_id)?;
    let after =
        DimensionValue { name: name.trim().to_string(), updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::dimension_value(&before), record::dimension_value(&after))?;
    Ok(after)
}

/// Set a value's period `[start_on, end_on]`, both ends inclusive. `None` opens that end — i.e.
/// clears it, and `end_on: None` reads as "still running". `start_on <= end_on` is enforced only
/// when both ends are present. **The role gatekeeper does not live here**: whether to refuse dates
/// on a value of a non-time_axis axis is a policy for the layer above (CLI/GUI), and the data layer
/// writes the columns as asked (see the `DimensionValue` doc in the model).
pub fn value_set_dates(
    tx: &WriteTx<'_>,
    value_id: i64,
    start_on: Option<NaiveDate>,
    end_on: Option<NaiveDate>,
) -> Result<DimensionValue> {
    if matches!((start_on, end_on), (Some(s), Some(e)) if s > e) {
        return Err(Error::Invalid(
            Msg::new("a value's start date cannot be after its end date")
            .coded(ErrorCode::InvalidDimensionPeriodOrder),
        ));
    }
    let before = live_value_before(tx, value_id)?;
    live_before(tx, before.dimension_id)?;
    let after =
        DimensionValue { start_on, end_on, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::dimension_value(&before), record::dimension_value(&after))?;
    Ok(after)
}

/// Reorder a value on an ordered dimension. An unordered dimension has no arrangement, so this is
/// refused.
pub fn value_move(tx: &WriteTx<'_>, value_id: i64, pos: Position) -> Result<DimensionValue> {
    let before = live_value_before(tx, value_id)?;
    let dimension = live_before(tx, before.dimension_id)?;
    if !dimension.ordered {
        return Err(Error::Invalid(
            Msg::new("this dimension's values are unordered and cannot be reordered")
            .coded(ErrorCode::InvalidDimensionValuesUnordered),
        ));
    }
    let sibs = read::dimension_value_siblings(tx.conn(), dimension.id, Some(value_id))?;
    let key = place(&sibs, &pos)?;
    let after = DimensionValue { order_key: key, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::dimension_value(&before), record::dimension_value(&after))?;
    Ok(after)
}

/// Hard-delete a dimension value, and with it every task assignment naming it.
pub fn value_delete(tx: &WriteTx<'_>, value_id: i64) -> Result<()> {
    let before = live_value_before(tx, value_id)?;
    live_before(tx, before.dimension_id)?;
    delete_value_subtree(tx, before.id)
}

/// Hard-delete one dimension value and the assignments on it (pass an id already checked to exist) —
/// the body of [`value_delete`], and the per-value step [`delete_subtree`] repeats down an axis.
pub(crate) fn delete_value_subtree(tx: &WriteTx<'_>, value_id: i64) -> Result<()> {
    for assignment_id in read::assignment_ids_of_value(tx.conn(), value_id)? {
        tx.delete_record("task_dimension_value", assignment_id)?;
    }
    tx.delete_record("dimension_value", value_id)?;
    Ok(())
}

// ───────────────────────────── Assignment to tasks ─────────────────────────────

/// Assign a task to a dimension value. Every dimension is single-select, so `(task, dimension)` is
/// constrained to a single row: an existing assignment to a different value is deleted and
/// replaced. A noop when the same value is already assigned. Returns (row, created). The removal
/// and the insert ride on **the same transaction** — commit them separately and a crash in between
/// leaves zero or two rows for one task on one dimension, breaking the one-row invariant.
pub fn set(tx: &WriteTx<'_>, task_id: i64, value_id: i64) -> Result<(TaskDimensionValue, bool)> {
    let Some(task) = read::task(tx.conn(), task_id)? else {
        return Err(crate::ops::task::NOUN.not_found(task_id.to_string()));
    };
    let dimension_id = read::dimension_id_of_value(tx.conn(), value_id)?
        .ok_or_else(|| VALUE_NOUN.not_found(value_id.to_string()))?;
    let axis = read::dimension(tx.conn(), dimension_id)?
        .ok_or_else(|| NOUN.not_found(dimension_id.to_string()))?;
    // A classification is an edge like any other: it names an axis and a value that live in one project,
    // and following it is how that project's vocabulary — what its axes are called, what values they
    // offer — reaches this task's context.
    crate::ops::guard_same_project(
        task.project_id,
        Some(axis.project_id),
        "this classification",
    )?;

    // Idempotent noop when the same value is already assigned.
    if let Some(id) = read::assignment_id(tx.conn(), task_id, value_id)? {
        let existing = read::task_dimension_value(tx.conn(), id)?
            .expect("the assignment id was just read from the same transaction");
        return Ok((existing, false));
    }

    // Drop the existing assignment on this axis (a different value) first, keeping it to one row.
    let now = Timestamp::now();
    for id in read::assignment_ids_on_axis(tx.conn(), task_id, dimension_id)? {
        tx.delete_record("task_dimension_value", id)?;
    }

    let tv = TaskDimensionValue {
        id: read::next_id(tx.conn(), "task_dimension_value")?,
        task_id,
        dimension_id,
        value_id,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::task_dimension_value(&tv))?;
    Ok((tv, true))
}

/// Remove a task's assignment to a particular dimension value (a hard delete). A noop when it is
/// not assigned. Returns changed.
pub fn unset(tx: &WriteTx<'_>, task_id: i64, value_id: i64) -> Result<bool> {
    let Some(id) = read::assignment_id(tx.conn(), task_id, value_id)? else {
        return Ok(false);
    };
    tx.delete_record("task_dimension_value", id)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, mk_task_in, new_engine};

    /// A project in which to exercise the dimension ops in isolation (`project::add` seeds no
    /// dimensions).
    fn project_named(tx: &WriteTx<'_>, name: &str) -> i64 {
        mk_project(tx, name)
    }

    fn task_in(tx: &WriteTx<'_>, title: &str, project_id: i64) -> i64 {
        mk_task_in(tx, title, Some(project_id))
    }

    fn custom(name: &str) -> NewDimension {
        NewDimension { name: name.to_string(), ..NewDimension::default() }
    }

    /// One dimension (`None` if it has been deleted).
    fn dim_opt(tx: &WriteTx<'_>, id: i64) -> Option<Dimension> {
        read::dimension(tx.conn(), id).unwrap()
    }

    /// One dimension, assumed to exist.
    fn dim(tx: &WriteTx<'_>, id: i64) -> Dimension {
        dim_opt(tx, id).unwrap()
    }

    /// One dimension value (`None` if it has been deleted).
    fn val_opt(tx: &WriteTx<'_>, id: i64) -> Option<DimensionValue> {
        read::dimension_value(tx.conn(), id).unwrap()
    }

    /// One dimension value, assumed to exist.
    fn val(tx: &WriteTx<'_>, id: i64) -> DimensionValue {
        val_opt(tx, id).unwrap()
    }

    /// A project's live dimensions in display order (ascending `order_key`).
    fn dims(tx: &WriteTx<'_>, project_id: i64) -> Vec<Dimension> {
        read::dimension_siblings(tx.conn(), project_id, None)
            .unwrap()
            .into_iter()
            .map(|(id, _)| dim(tx, id))
            .collect()
    }

    /// A dimension's live values (ascending `order_key` when ordered, ascending id when not).
    fn vals(tx: &WriteTx<'_>, dimension_id: i64) -> Vec<DimensionValue> {
        let ordered = dim(tx, dimension_id).ordered;
        let mut v: Vec<DimensionValue> = read::dimension_value_ids(tx.conn(), dimension_id)
            .unwrap()
            .into_iter()
            .map(|id| val(tx, id))
            .collect();
        if ordered {
            v.sort_by(|a, b| a.order_key.cmp(&b.order_key));
        } else {
            v.sort_by_key(|a| a.id);
        }
        v
    }

    /// The time-axis value covering that day — the same read the default assignment does.
    fn current(tx: &WriteTx<'_>, project_id: i64, date: NaiveDate) -> Option<i64> {
        read::current_time_axis_value(tx.conn(), project_id, date).unwrap()
    }

    #[test]
    fn add_orders_and_lists_dimensions() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d1 = add(tx, p, custom("カテゴリー")).unwrap();
        let d2 = add(tx, p, custom("優先度")).unwrap();
        assert!(d1.order_key < d2.order_key, "a dimension added later sorts after");
                let names: Vec<String> = dims(tx, p).iter().map(|d| d.name.clone()).collect();
        assert_eq!(names, vec!["カテゴリー", "優先度"]);
    }

    #[test]
    fn add_rejects_empty_name_and_unknown_project() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        assert!(add(tx, p, custom("   ")).is_err());
        assert!(add(tx, 999_999, custom("軸")).is_err());
    }

    #[test]
    fn rename_move_and_resolve_dimensions() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d1 = add(tx, p, custom("D1")).unwrap();
        let d2 = add(tx, p, custom("D2")).unwrap();
        update(tx, d1.id, Some("分類"), None, None, None).unwrap();
        assert_eq!(dim(tx, d1.id).name, "分類");
        // Resolves by name or by id (exact match).
        assert_eq!(read::resolve_dimension_in(tx.conn(), None, "分類").unwrap(), vec![d1.id]);
        assert_eq!(read::resolve_dimension_in(tx.conn(), None, &d2.id.to_string()).unwrap(), vec![d2.id]);
        // D2 to the top.
        move_to(tx, d2.id, Position::Top).unwrap();
                let order: Vec<String> = dims(tx, p).iter().map(|d| d.name.clone()).collect();
        assert_eq!(order, vec!["D2", "分類"]);
    }

    #[test]
    fn any_dimension_can_be_deleted() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        // All dimensions are user-editable alike, so any of them can be deleted.
        let c = add(tx, p, custom("消える")).unwrap();
        delete(tx, c.id).unwrap();
        assert!(dim_opt(tx, c.id).is_none(), "the row goes away entirely");
    }

    #[test]
    fn value_ops_place_rename_and_reorder() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, NewDimension { name: "優先度".into(), ordered: true, ..NewDimension::default() }).unwrap();
        let v1 = value_add(tx, d.id, "低").unwrap();
        let v2 = value_add(tx, d.id, "高").unwrap();
        assert!(v1.order_key < v2.order_key);
        value_rename(tx, v1.id, "low").unwrap();
        assert_eq!(val(tx, v1.id).name, "low");
        // The second value to the top.
        value_move(tx, v2.id, Position::Top).unwrap();
                let order: Vec<String> = vals(tx, d.id).iter().map(|v| v.name.clone()).collect();
        assert_eq!(order, vec!["高", "low"]);
        // Values resolve by name or by id.
        assert_eq!(read::resolve_dimension_value_in(tx.conn(), d.id, "高").unwrap(), vec![v2.id]);
    }

    fn day(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    /// A value's period can be set and cleared, and start > end is refused. The role gatekeeper
    /// lives above, so the data layer writes the dates even on a non-time_axis axis.
    #[test]
    fn value_set_dates_writes_clears_and_rejects_reversed_period() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("フェーズ")).unwrap();
        let v = value_add(tx, d.id, "開発期").unwrap();
        // A fresh value has no period.
        assert_eq!((v.start_on, v.end_on), (None, None));

        let set = value_set_dates(tx, v.id, Some(day("2026-06-20")), Some(day("2026-07-07")))
            .unwrap();
        assert_eq!(set.start_on, Some(day("2026-06-20")));
        assert_eq!(set.end_on, Some(day("2026-07-07")));
        assert_eq!(val(tx, v.id).end_on, Some(day("2026-07-07")));

        // Opening the end means "still running".
        let open = value_set_dates(tx, v.id, Some(day("2026-07-08")), None).unwrap();
        assert_eq!(open.end_on, None);
        // Both ends can be cleared.
        let cleared = value_set_dates(tx, v.id, None, None).unwrap();
        assert_eq!((cleared.start_on, cleared.end_on), (None, None));

        // A reversed period is refused; with only one end present nothing is enforced.
        assert!(value_set_dates(tx, v.id, Some(day("2026-07-08")), Some(day("2026-07-07")))
            .is_err());
        assert!(value_set_dates(tx, v.id, None, Some(day("2026-01-01"))).is_ok());

        // An unknown or deleted value is refused.
        assert!(value_set_dates(tx, 999_999, None, None).is_err());
    }

    /// The "current era" test is an inclusive window. A value with no period covers no day at all,
    /// which is what keeps the window meaningful.
    #[test]
    fn covers_is_inclusive_and_open_ended_but_never_matches_an_unset_period() {
        let mut v = DimensionValue { start_on: Some(day("2026-06-20")), ..Default::default() };
        v.end_on = Some(day("2026-07-07"));
        assert!(v.covers(day("2026-06-20")), "start is inclusive");
        assert!(v.covers(day("2026-07-07")), "end is inclusive");
        assert!(!v.covers(day("2026-06-19")));
        assert!(!v.covers(day("2026-07-08")));

        // Still running: the end is open.
        v.end_on = None;
        assert!(v.covers(day("2099-01-01")));
        // The start is open.
        v.start_on = None;
        v.end_on = Some(day("2026-07-07"));
        assert!(v.covers(day("1970-01-01")));
        assert!(!v.covers(day("2026-07-08")));

        // With no period at all, nothing is covered.
        v.end_on = None;
        assert!(!v.covers(day("2026-07-08")));
    }

    /// The "current era" is decided by the windows on the time_axis axis alone. Dates sitting on a
    /// non-time_axis axis have no effect on it.
    #[test]
    fn current_value_resolves_only_through_the_time_axis() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");

        // No time axis yet, so nothing is assigned.
        assert!(current(tx, p, day("2026-07-09")).is_none());

        // Dates can be written on a non-time_axis axis (nothing here refuses them), but the
        // resolution reads the time_axis axis alone, so they contribute nothing to it.
        let other = add(tx, p, custom("カテゴリー")).unwrap();
        let ov = value_add(tx, other.id, "バグ").unwrap();
        value_set_dates(tx, ov.id, Some(day("2026-01-01")), None).unwrap();
        assert!(current(tx, p, day("2026-07-09")).is_none(), "a non-time_axis axis makes no current era");

        let axis = add(
            tx,
            p,
            NewDimension { role: DimensionRole::TimeAxis, ordered: true, ..custom("時代") },
        )
        .unwrap();
        let dev = value_add(tx, axis.id, "開発期").unwrap();
        value_set_dates(tx, dev.id, Some(day("2026-06-20")), Some(day("2026-07-07"))).unwrap();
        let ops1 = value_add(tx, axis.id, "運用第1期").unwrap();
        value_set_dates(tx, ops1.id, Some(day("2026-07-08")), None).unwrap();
        // A value with no period has no window, so it covers no day.
        value_add(tx, axis.id, "未設定").unwrap();

        assert_eq!(current(tx, p, day("2026-07-01")).unwrap(), dev.id);
        assert_eq!(current(tx, p, day("2026-07-07")).unwrap(), dev.id, "the end is inclusive");
        assert_eq!(current(tx, p, day("2026-07-08")).unwrap(), ops1.id);
        assert_eq!(current(tx, p, day("2099-01-01")).unwrap(), ops1.id, "the era still running");
        // A day that falls in no window resolves to nothing.
        assert!(current(tx, p, day("2026-06-19")).is_none());

        // Overlapping windows are folded to the first value in the axis's order (it is ordered, so
        // that means `order_key`).
        value_set_dates(tx, ops1.id, Some(day("2026-06-20")), None).unwrap();
        assert_eq!(current(tx, p, day("2026-07-01")).unwrap(), dev.id, "an overlap resolves uniquely by order");

        // Drop the axis and the era goes with it.
        delete(tx, axis.id).unwrap();
        assert!(current(tx, p, day("2026-07-01")).is_none());
    }

    /// An existing axis can be nominated as the time axis after the fact, and un-nominating it steps
    /// back out of era resolution — the dates stay put.
    #[test]
    fn update_names_and_unnames_the_time_axis() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("時代")).unwrap();
        let now = value_add(tx, d.id, "運用第1期").unwrap();
        value_set_dates(tx, now.id, Some(day("2026-07-08")), None).unwrap();
        // An axis with no role makes no current era, dates or not.
        assert!(current(tx, p, day("2026-07-09")).is_none());

        // Nominate it later and the very same dates start acting as windows. Name and notes are
        // left as they were.
        let named = update(tx, d.id, None, None, None, Some(DimensionRole::TimeAxis)).unwrap();
        assert_eq!(named.name, "時代");
        assert_eq!(current(tx, p, day("2026-07-09")).unwrap(), now.id);

        // Un-nominate it and it steps out of the resolution; the dates stay in their columns but
        // stop meaning anything.
        update(tx, d.id, None, None, None, Some(DimensionRole::None)).unwrap();
        assert!(current(tx, p, day("2026-07-09")).is_none());
        assert_eq!(val(tx, now.id).start_on, Some(day("2026-07-08")));
    }

    #[test]
    fn update_toggles_ordered_and_gates_value_move() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        // Created unordered, its values cannot be reordered.
        let d = add(tx, p, custom("カテゴリー")).unwrap();
        let a = value_add(tx, d.id, "A").unwrap();
        let b = value_add(tx, d.id, "B").unwrap();
        assert!(!dim(tx, d.id).ordered);
        assert!(value_move(tx, b.id, Position::Top).is_err(), "an unordered axis cannot be reordered");

        // Turn `ordered` on and `order_key` takes effect, so values can be reordered. Name and
        // notes are left as they were.
        let updated = update(tx, d.id, None, None, Some(true), None).unwrap();
        assert!(updated.ordered);
        assert_eq!(updated.name, "カテゴリー");
        value_move(tx, b.id, Position::Top).unwrap();
                let order: Vec<String> = vals(tx, d.id).iter().map(|v| v.name.clone()).collect();
        assert_eq!(order, vec!["B", "A"]);

        // Turn `ordered` back off and the values fall back to a stable ascending-id order rather
        // than `order_key`, so the reordering stops showing.
        update(tx, d.id, None, None, Some(false), None).unwrap();
        assert!(!dim(tx, d.id).ordered);
        let mut expected = [("A", &a.id), ("B", &b.id)];
        expected.sort_by(|x, y| x.1.cmp(y.1));
        let expected_names: Vec<&str> = expected.iter().map(|(n, _)| *n).collect();
                let order: Vec<String> = vals(tx, d.id).iter().map(|v| v.name.clone()).collect();
        assert_eq!(order, expected_names, "unordered means ascending id (independent of the reordering)");
    }

    #[test]
    fn unordered_values_cannot_be_reordered() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, NewDimension { name: "色".into(), ordered: false, ..NewDimension::default() }).unwrap();
        let v = value_add(tx, d.id, "赤").unwrap();
        assert!(value_move(tx, v.id, Position::Top).is_err());
    }

    #[test]
    fn value_ops_reject_a_missing_dimension() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        // A value op on a dimension that does not exist is not_found.
        assert!(value_add(tx, 999_999, "done").is_err());
    }

    /// Deleting a value removes the row, and the task assignments naming it go with it.
    #[test]
    fn value_delete_takes_the_assignments() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("カテゴリー")).unwrap();
        let v = value_add(tx, d.id, "バグ").unwrap();
        let t = task_in(tx, "task", p);
        set(tx, t, v.id).unwrap();
        value_delete(tx, v.id).unwrap();
        assert!(val_opt(tx, v.id).is_none());
        assert!(read::assignment_id(tx.conn(), t, v.id).unwrap().is_none(), "the assignments go too");
    }

    #[test]
    fn set_replaces_within_an_axis_and_axes_are_independent() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "task", p);

        // Every dimension is single-select: setting a different value drops the previous assignment
        // and keeps it to one row.
        let cat = add(tx, p, custom("カテゴリー")).unwrap();
        let a = value_add(tx, cat.id, "A").unwrap();
        let b = value_add(tx, cat.id, "B").unwrap();
        set(tx, t, a.id).unwrap();
        set(tx, t, b.id).unwrap();
        let live = read::assignment_ids_on_axis(tx.conn(), t, cat.id).unwrap();
        assert_eq!(live.len(), 1, "single-select means one row per (task,dimension)");
        let row = read::task_dimension_value(tx.conn(), live[0]).unwrap().unwrap();
        assert_eq!(row.value_id, b.id);

        // The one-row constraint is per axis; assignments on a different axis coexist.
        let area = add(tx, p, custom("領域")).unwrap();
        let core = value_add(tx, area.id, "core").unwrap();
        set(tx, t, core.id).unwrap();
        let n = read::task_dimension_assignments(tx.conn(), t).unwrap().len();
        assert_eq!(n, 2, "assignments on a different axis coexist");
    }

    #[test]
    fn set_is_idempotent_and_unset_removes() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "task", p);
        let d = add(tx, p, custom("カテゴリー")).unwrap();
        let v = value_add(tx, d.id, "A").unwrap();
        let (_, created1) = set(tx, t, v.id).unwrap();
        let (_, created2) = set(tx, t, v.id).unwrap();
        assert!(created1 && !created2, "setting the same value again is idempotent");
        assert!(unset(tx, t, v.id).unwrap());
        assert!(!unset(tx, t, v.id).unwrap(), "already unset is a noop");
    }

    #[test]
    fn set_rejects_unknown_task_and_value() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("カテゴリー")).unwrap();
        let v = value_add(tx, d.id, "A").unwrap();
        assert!(set(tx, 9999, v.id).is_err(), "assigning to a task that does not exist is refused");
        let t = task_in(tx, "task", p);
        assert!(set(tx, t, 999_999).is_err());
    }

    /// Deleting a dimension takes its values (dependent content) and assignments (links) with it.
    #[test]
    fn delete_dimension_takes_values_and_assignments() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("カテゴリー")).unwrap();
        let v = value_add(tx, d.id, "A").unwrap();
        let t = task_in(tx, "task", p);
        set(tx, t, v.id).unwrap();
        delete(tx, d.id).unwrap();
        assert!(dim_opt(tx, d.id).is_none());
        assert!(val_opt(tx, v.id).is_none(), "the values go too");
        assert!(read::assignment_id(tx.conn(), t, v.id).unwrap().is_none(), "and the assignments on them");
    }
}
