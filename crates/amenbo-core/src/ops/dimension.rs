//! Operations on the unified dimension model.
//!
//! Every axis a task is classified along — phase, category, tag, or anything a user invents —
//! funnels into a single "dimension" mechanism. A dimension is a plain classification axis, and all
//! of them are user-editable alike: no built-in fixed axes, no locked values, no seeding (status
//! and priority are first-class task attributes instead).
//!
//! The operations are add/list/show/update/move/delete. Renaming is the `name` argument of
//! `update` (there is no dedicated `rename`). Values are handled by `value_*`, assignment to a
//! task by set/unset, and assignment to a decision by set_on_decision/unset_on_decision — one axis,
//! one set of values, two kinds of thing classified by it (`AMB-D-781`).
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
    DecisionDimensionValue, Dimension, DimensionAppliesTo, DimensionCardinality, DimensionRole,
    DimensionValue, TaskDimensionValue,
};
use crate::ops::{emit_create, emit_update, place, Noun, Position};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// The noun for the dimension entity (the English/Japanese pair used in not_found messages).
pub(crate) const NOUN: Noun = Noun { en: "dimension", code: ErrorCode::NotFoundDimension };
/// The noun for the dimension-value entity.
pub(crate) const VALUE_NOUN: Noun = Noun { en: "dimension value", code: ErrorCode::NotFoundDimensionValue };

/// The specification of a new dimension. The defaults — single-select, unordered, no role, off the
/// card, not required, classifying both entities — are the bare shape of a user-defined axis. A
/// time-axis phase is built by setting `role=TimeAxis`.
#[derive(Clone, Debug)]
pub struct NewDimension {
    pub name: String,
    pub notes: String,
    pub cardinality: DimensionCardinality,
    pub ordered: bool,
    pub role: DimensionRole,
    /// Whether a task's value on this axis goes on its card (`AMB-D-651`). New axes start `false`, so
    /// an axis reaches the cards only once somebody says it should (`AMB-D-650`).
    pub show_on_card: bool,
    /// Whether a creation can be finished without a value on this axis (`AMB-D-734`). A new axis has no
    /// values yet, so raising it here is refused for the same reason [`update`] refuses it on an empty
    /// axis: the flag would be unsatisfiable from the moment it was written.
    pub required: bool,
    /// Which entity this axis classifies (`AMB-D-789`). Unlike the flags above it starts on the wide
    /// side — `Both` — because an axis nobody said anything about is one whose raiser expects it where
    /// they raised it. Narrowing is [`update`]'s to do, and takes no assignment away.
    pub applies_to: DimensionAppliesTo,
    /// The readable key this axis is to be known by outside Amenbo (`AMB-D-735`). `None` takes the
    /// id-derived default, which is what nearly every axis keeps; naming one here is for the axis whose
    /// slug somebody outside has to type.
    pub slug: Option<String>,
}

impl Default for NewDimension {
    fn default() -> Self {
        NewDimension {
            name: String::new(),
            notes: String::new(),
            cardinality: DimensionCardinality::Single,
            ordered: false,
            role: DimensionRole::None,
            show_on_card: false,
            required: false,
            applies_to: DimensionAppliesTo::Both,
            slug: None,
        }
    }
}

/// The refusal both doors to `required` share: an axis offering no values cannot demand one
/// (`AMB-D-734`). Written once so the sentence a caller sees does not depend on which door it came in
/// by — `add`, where the axis is new and so necessarily empty, or `update` on an axis whose values were
/// never added.
fn unsatisfiable_required(name: &str) -> Error {
    Error::Invalid(
        Msg::new(format!(
            "'{name}' offers no values, so it cannot be required — add a value to it first"
        ))
        .coded(ErrorCode::InvalidDimensionRequiredWithoutValues)
        .with("name", name),
    )
}

/// The refusal both doors to the pair share: a time axis is the mechanism that resolves one "current
/// era" and writes it onto a new record ([`read::current_time_axis_value`]), so an axis that admits
/// several values cannot be the one doing it — what to write, and what belonging to two eras at once
/// means, would both be undefined (`AMB-D-826`). Written once so the sentence does not depend on which
/// half moved: `add`, where the two arrive together, and `update`, where either can be flipped onto the
/// other.
fn time_axis_holds_one(name: &str) -> Error {
    Error::Invalid(
        Msg::new(format!(
            "'{name}' is the time axis, and the time axis holds one value at a time — take the \
             time-axis role off it, or leave it single-select"
        ))
        .coded(ErrorCode::InvalidDimensionMultiTimeAxis)
        .with("name", name),
    )
}

/// The refusal a demotion meets: going back to single-select would throw away every value but one, on
/// every record answering with several (`AMB-D-826`). Raising the flag adds nothing and so is free; this
/// is the direction that loses data, and it names the count so the caller knows the size of what it is
/// being asked to decide — the shape `value_delete` takes when it demands `--reassign-to`.
fn demotion_drops_values(name: &str, holders: usize) -> Error {
    Error::Invalid(
        Msg::new(format!(
            "{holders} record(s) answer '{name}' with more than one value, so it cannot go back to \
             single-select — clear the extra values off them first"
        ))
        .coded(ErrorCode::InvalidDimensionDemoteHolders)
        .with("name", name)
        .with("count", holders.to_string()),
    )
}

/// How many records answer this axis with more than one value — what a demotion would have to throw
/// away. Both sides are asked: the axis is one mechanism serving tasks and decisions alike
/// (`AMB-D-781`), so a decision filed under three values is as much a holder as a task is.
///
/// Read through the project-wide assignment readers and counted here rather than in SQL: a demotion is
/// a deliberate, rare act on one axis, and the rows it reads are that axis's alone.
fn holders_of_several(tx: &WriteTx<'_>, axis: &Dimension) -> Result<usize> {
    fn several(rows: Vec<(i64, i64)>) -> usize {
        let mut per_holder: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
        for (holder, _) in rows {
            *per_holder.entry(holder).or_default() += 1;
        }
        per_holder.into_values().filter(|n| *n > 1).count()
    }
    let tasks = read::project_dimension_assignments(tx.conn(), axis.project_id, axis.id)?;
    let decisions =
        read::project_decision_dimension_assignments(tx.conn(), axis.project_id, axis.id)?;
    Ok(several(tasks) + several(decisions))
}

/// The longest a slug may be. Long enough to read as a word, short enough for the places a slug is
/// going — a branch name, a directory, an element of a D-Bus well-known name (`AMB-D-735`).
const SLUG_MAX: usize = 24;

/// The slug a row is born with when the caller names none: its own id, behind a letter.
///
/// **Derived from the id and never from the name.** `crate::slug::base` keeps runs of ASCII
/// alphanumerics and drops the rest, so a Japanese name yields nothing and every axis in this store
/// would be born under the same fallback word. The id is the one thing every row already has that is
/// unique, so the default is unique for free. The leading letter is not decoration: it is what makes
/// the result satisfy [`checked_slug`], and what keeps a slug usable as a D-Bus name element, which may
/// not begin with a digit (`AMB-D-733`).
///
/// **A default is escaped, not refused.** Unique for free holds against other defaults, not against an
/// edit: somebody may already have moved `d7` onto another axis by hand, and the row now being created
/// asked for nothing and has no way to ask for anything else — its slug can only be edited once it
/// exists. So a taken default takes a numeric suffix, the way a project's derived slug does
/// (`crate::slug::unique`). A slug the caller *named* is refused instead, because there the caller can
/// name another.
fn settled_default<F>(prefix: char, id: i64, taken: F) -> Result<String>
where
    F: Fn(&str) -> Result<bool>,
{
    let base = format!("{prefix}{id}");
    if !taken(&base)? {
        return Ok(base);
    }
    for n in 2u32.. {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate)? {
            return Ok(candidate);
        }
    }
    unreachable!("the suffix range is unbounded")
}

/// The shape a named slug has to have: lower-case ASCII letters, digits and hyphens, opening with a
/// letter, [`SLUG_MAX`] characters at most (`AMB-D-735`).
///
/// **Upper case is refused rather than folded.** SQLite compares text byte-wise, so the store would
/// hold `Foo` and `foo` as two axes, while the file names and the D-Bus names a slug is carried into
/// treat them as one — a disagreement that only shows up on the machine that has to resolve it. The
/// door takes the narrow set and says so.
fn checked_slug(slug: &str) -> Result<String> {
    let s = slug.trim();
    let shaped = s.len() <= SLUG_MAX
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !shaped {
        return Err(Error::Invalid(
            Msg::new(format!(
                "'{s}' cannot be a slug — use at most {SLUG_MAX} lower-case ASCII letters, digits and \
                 hyphens, starting with a letter"
            ))
            .coded(ErrorCode::InvalidDimensionSlugShape)
            .with("slug", s)
            .with("max", SLUG_MAX),
        ));
    }
    Ok(s.to_string())
}

/// The refusal both slug doors share: somebody within the same reach already answers to it. Raised in
/// place of the table's own `UNIQUE`, so the caller is told which row holds the slug instead of reading
/// a constraint violation — and so the refusal arrives before anything in the transaction is written.
///
/// The holder is named the way this crate names any row it cannot hand back — by its key, in the bare
/// form (`ops::Noun::not_found` writes the same), leaving the reference label to whichever surface has
/// one. It rides as a field too, so that surface need not read the sentence to find it.
fn slug_taken(what: &str, slug: &str, holder: i64) -> Error {
    Error::Invalid(
        Msg::new(format!("'{slug}' is already the key of {what} {holder}"))
            .coded(ErrorCode::InvalidDimensionSlugTaken)
            .with("slug", slug)
            .with("holder", holder),
    )
}

/// The shape a name has to have: something once trimmed, and no whitespace left inside it
/// (`AMB-D-819`). `what` is the noun the empty case is reported with, so the sentence names the axis or
/// the value the caller was actually adding.
///
/// **Whitespace is refused because a name holding it cannot be filtered on.** `crate::query` cuts a
/// filter on whitespace before it cuts it on `:`, so `dim:<axis>=<name>` cannot be written for such a
/// name at all — the id and the slug still reach it, but the thing a person actually remembers stops
/// working, and nothing on the screen says why.
///
/// **Every Unicode space, not the ASCII one alone.** The cut is `str::split_whitespace`, which parts on
/// U+3000 as readily as on U+0020 — a name typed with a full-width space on a Japanese keyboard breaks
/// in exactly the same way, and this store is named in Japanese throughout.
fn checked_name(what: &str, name: &str) -> Result<String> {
    let s = name.trim();
    if s.is_empty() {
        return Err(Error::invalid(format!("a {what} name cannot be empty")));
    }
    if s.chars().any(char::is_whitespace) {
        return Err(Error::Invalid(
            Msg::new(format!(
                "'{s}' cannot be a name — leave the whitespace out, so a filter can name it after `dim:`"
            ))
            .coded(ErrorCode::InvalidDimensionNameWhitespace)
            .with("name", s),
        ));
    }
    Ok(s.to_string())
}

// ───────────────────────────── Dimensions (the axis itself) ─────────────────────────────

pub fn add(tx: &WriteTx<'_>, project_id: i64, new: NewDimension) -> Result<Dimension> {
    let name = checked_name("dimension", &new.name)?;
    if read::project(tx.conn(), project_id)?.is_none() {
        return Err(crate::ops::project::NOUN.not_found(project_id.to_string()));
    }
    // An axis is born with no values, so "required" here could never be met by anybody.
    if new.required {
        return Err(unsatisfiable_required(&name));
    }
    // The one door where the pair arrives already made; `update` is the other.
    if new.role == DimensionRole::TimeAxis && new.cardinality == DimensionCardinality::Multi {
        return Err(time_axis_holds_one(&name));
    }
    let sibs = read::dimension_siblings(tx.conn(), project_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    // The id is settled before the record is emitted, which is what lets the default be derived from it.
    let id = read::next_id(tx.conn(), "dimension")?;
    let slug = match new.slug.as_deref() {
        Some(named) => {
            let named = checked_slug(named)?;
            if let Some(holder) = read::dimension_id_by_slug(tx.conn(), project_id, &named)? {
                return Err(slug_taken("category", &named, holder));
            }
            named
        }
        None => settled_default('d', id, |candidate| {
            Ok(read::dimension_id_by_slug(tx.conn(), project_id, candidate)?.is_some())
        })?,
    };
    let dimension = Dimension {
        id,
        project_id,
        name,
        notes: new.notes,
        cardinality: new.cardinality,
        ordered: new.ordered,
        role: new.role,
        show_on_card: new.show_on_card,
        required: new.required,
        applies_to: new.applies_to,
        slug: Some(slug),
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

/// Update a dimension's name, notes, how many of its values one record may hold (`cardinality`),
/// whether its values are ordered (`ordered`), its role (`role`),
/// whether it belongs on the task card (`show_on_card`), whether it refuses to be left empty
/// (`required`) and which entity it classifies (`applies_to`). Only the `Some` fields are written. The
/// name is a display label, so it is free to change. Flipping `ordered` false→true brings the
/// values' `order_key` into play (making `value_move` possible); true→false drops them back to a
/// stable ascending-id order (`order_key` is not cleared, so flipping it on again revives the old
/// arrangement). `role` is a nomination:
/// set `TimeAxis` and that axis's values carry the periods and decide the "current era"; set it
/// back to `None` and the dates stay in their columns but stop meaning anything. **Uniqueness of
/// the nomination is not enforced**: even if several axes call themselves the time axis,
/// [`read::current_time_axis_value`] folds them deterministically down to one by dimension order
/// (`add`'s `--time-axis` is just as unchecked, so the same rule holds whichever door you came in
/// by). `show_on_card` is the axis's own answer to whether a task's value on it goes on the card, so
/// flipping it here moves every face at once and no number of axes flipped on is refused — the
/// crowding that invites is the raiser's to judge (`AMB-D-650`), not this op's to cap.
///
/// `required` is the one flag with a precondition: an axis offering no values is refused, because a
/// flag nobody can satisfy would leave every creation on this project stuck at the door (`AMB-D-734`).
/// Lowering it is free, and so is leaving it where it is. Raising it does not touch a task that has
/// already finished its creation — the premise bites at [`crate::ops::task::finish_creating`] and
/// nowhere else — but a draft left open on this project will now be held there until it carries a
/// value, which the decision names as the cost it accepts.
///
/// `cardinality` says how many of the axis's values one record may hold (`AMB-D-826`), and it is the
/// one flag with a precondition in **each** direction. Widening single→multi takes nothing away and is
/// free. Demoting multi→single would throw every value but one off every record answering with several,
/// so it is refused while any such record stands, and the refusal names how many there are — the shape
/// `value_delete` takes when it demands `--reassign-to`: what to keep is the caller's to decide, not
/// this op's to guess. Either direction is refused on the time axis, whichever half moved: the era
/// resolution reads one value, so `--time-axis` on a multi axis and `--cardinality multi` on the time
/// axis are one refusal met from two sides.
///
/// `applies_to` narrows or widens which entity the axis classifies (`AMB-D-789`). Narrowing "both →
/// one side" is free and **takes nothing away**: the assignments already made on the side that just
/// stopped counting stay in their table and simply stop meaning anything — the same shape `role` takes
/// when a time axis goes back to `None` and its values keep their dates. Widening is free too, and
/// there is no precondition on either direction: an axis with no values narrows as readily as one with
/// a hundred, since this flag says where the axis is offered, not whether it can be answered.
///
/// `slug` renames the axis's readable key (`AMB-D-735`). It is checked for shape and for a collision
/// inside the project before anything is written, so the refusal names the axis already holding it
/// rather than surfacing the table's `UNIQUE`. Passing `None` leaves the slug where it is; there is no
/// way through this door to clear one, because a saved row is never without one.
///
/// The arity is the axis's own: every flag it carries is settable through this one door, and splitting
/// them into per-flag ops would make "change two things at once" two transactions instead of one.
#[allow(clippy::too_many_arguments)]
pub fn update(
    tx: &WriteTx<'_>,
    id: i64,
    name: Option<&str>,
    notes: Option<&str>,
    cardinality: Option<DimensionCardinality>,
    ordered: Option<bool>,
    role: Option<DimensionRole>,
    show_on_card: Option<bool>,
    required: Option<bool>,
    applies_to: Option<DimensionAppliesTo>,
    slug: Option<&str>,
) -> Result<Dimension> {
    let name = match name {
        Some(n) => Some(checked_name("dimension", n)?),
        None => None,
    };
    let before = live_before(tx, id)?;
    let mut d = before.clone();
    if let Some(n) = name {
        d.name = n;
    }
    if let Some(t) = notes {
        d.notes = t.to_string();
    }
    if let Some(c) = cardinality {
        d.cardinality = c;
    }
    if let Some(o) = ordered {
        d.ordered = o;
    }
    if let Some(r) = role {
        d.role = r;
    }
    if let Some(c) = show_on_card {
        d.show_on_card = c;
    }
    if let Some(r) = required {
        if r && read::dimension_value_ids(tx.conn(), id)?.is_empty() {
            return Err(unsatisfiable_required(&d.name));
        }
        d.required = r;
    }
    if let Some(a) = applies_to {
        d.applies_to = a;
    }
    // Both halves of the pair are read off the axis as it *would* stand, so whichever of them this call
    // moved meets the same refusal (`AMB-D-826`).
    if d.role == DimensionRole::TimeAxis && d.cardinality == DimensionCardinality::Multi {
        return Err(time_axis_holds_one(&d.name));
    }
    if before.cardinality == DimensionCardinality::Multi
        && d.cardinality == DimensionCardinality::Single
    {
        let holders = holders_of_several(tx, &before)?;
        if holders > 0 {
            return Err(demotion_drops_values(&d.name, holders));
        }
    }
    if let Some(named) = slug {
        let named = checked_slug(named)?;
        match read::dimension_id_by_slug(tx.conn(), d.project_id, &named)? {
            Some(holder) if holder != d.id => {
                return Err(slug_taken("category", &named, holder))
            }
            _ => d.slug = Some(named),
        }
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

/// Hard-delete a dimension, its values (`dimension_value`) and the assignments on them — on tasks
/// (`task_dimension_value`) and on decisions (`decision_dimension_value`) alike. Dimensions are all
/// user-editable alike, so deleting one is equally free.
pub fn delete(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    let before = live_before(tx, id)?;
    delete_subtree(tx, before.id)
}

/// Hard-delete one dimension and its children (pass an id whose existence has already been checked).
/// This is the body of [`delete`], and [`crate::ops::project::delete`] uses it to clear out a project's
/// dimensions. The op deletes each child itself, child-first — an axis and the classification a task
/// carries on it are both things a person can point at, so what goes has to go through code
/// (`AMB-D-403`). Sweeping value by value covers the whole axis, and both sides of it: an assignment —
/// a task's or a decision's — names a value, and that value's axis is this one.
pub(crate) fn delete_subtree(tx: &WriteTx<'_>, id: i64) -> Result<()> {
    for value_id in read::dimension_value_ids(tx.conn(), id)? {
        delete_value_subtree(tx, value_id)?;
    }
    tx.delete_record("dimension", id)?;
    Ok(())
}

// ───────────────────────────── Dimension values (the choices on an axis) ─────────────────────────────

/// Add a value to an axis. `slug` is the readable key the value answers to outside Amenbo
/// (`AMB-D-735`); `None` takes the id-derived default, which is what a value keeps unless somebody
/// outside has to type it.
pub fn value_add(
    tx: &WriteTx<'_>,
    dimension_id: i64,
    name: &str,
    slug: Option<&str>,
) -> Result<DimensionValue> {
    let name = checked_name("dimension value", name)?;
    live_before(tx, dimension_id)?;
    // Placed at the bottom whether or not the axis is ordered; when unordered it is merely carried
    // as a stable key (see the model).
    let sibs = read::dimension_value_siblings(tx.conn(), dimension_id, None)?;
    let order_key = place(&sibs, &Position::Bottom)?;
    let now = Timestamp::now();
    let id = read::next_id(tx.conn(), "dimension_value")?;
    let slug = match slug {
        Some(named) => {
            let named = checked_slug(named)?;
            if let Some(holder) =
                read::dimension_value_id_by_slug(tx.conn(), dimension_id, &named)?
            {
                return Err(slug_taken("value", &named, holder));
            }
            named
        }
        None => settled_default('v', id, |candidate| {
            Ok(read::dimension_value_id_by_slug(tx.conn(), dimension_id, candidate)?.is_some())
        })?,
    };
    let value = DimensionValue {
        id,
        dimension_id,
        name,
        slug: Some(slug),
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
    let name = checked_name("dimension value", name)?;
    let before = live_value_before(tx, value_id)?;
    live_before(tx, before.dimension_id)?;
    let after = DimensionValue { name, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::dimension_value(&before), record::dimension_value(&after))?;
    Ok(after)
}

/// Rename a value's readable key (`AMB-D-735`) — [`update`]'s `slug` arm, one value wide, and a door of
/// its own for the same reason [`value_set_dates`] is one: a value's fields are set one concern at a
/// time. Checked for shape and for a collision **within the axis**, which is as far as a value's slug is
/// unique.
pub fn value_set_slug(tx: &WriteTx<'_>, value_id: i64, slug: &str) -> Result<DimensionValue> {
    let slug = checked_slug(slug)?;
    let before = live_value_before(tx, value_id)?;
    live_before(tx, before.dimension_id)?;
    if let Some(holder) = read::dimension_value_id_by_slug(tx.conn(), before.dimension_id, &slug)? {
        if holder != before.id {
            return Err(slug_taken("value", &slug, holder));
        }
    }
    let after =
        DimensionValue { slug: Some(slug), updated_at: Timestamp::now(), ..before.clone() };
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

/// Hard-delete a dimension value, and with it every task assignment naming it. `reassign_to` names
/// another value **on the same axis** to move those assignments to first, so the tasks keep an answer
/// instead of losing one.
///
/// **A required axis refuses to empty tasks out** (`AMB-D-734`). Two ways to that state pass through
/// here, and both are shut:
///
/// - **The last value of a required axis** leaves a flag nobody can satisfy — every creation on the
///   project would then stop at the door, which is the same state [`update`] already refuses to create
///   from the other side. Lower the requirement first; that is free, and then the values are ordinary
///   again.
/// - **Any value a task is classified as** takes that classification with it, dropping a task that had
///   already finished its creation back to no value at all — the one state [`unset`] refuses one task
///   at a time, reached here for every task on the value at once, behind the creation premise's back.
///   So a required axis demands `reassign_to`: the delete says where the tasks go, or it does not
///   happen (`AMB-D-751` had to close this by hand every time a theme ended).
///
/// An axis that is not required carries no such demand — its assignments go with the value as before —
/// but `reassign_to` is honoured there too: naming a destination means the same thing on any axis.
///
/// Deleting the axis itself is not this door: [`delete_subtree`] sweeps its values through
/// [`delete_value_subtree`], which carries no guard, because an axis that is gone demands nothing.
pub fn value_delete(tx: &WriteTx<'_>, value_id: i64, reassign_to: Option<i64>) -> Result<()> {
    let before = live_value_before(tx, value_id)?;
    let axis = live_before(tx, before.dimension_id)?;
    if axis.required && read::dimension_value_ids(tx.conn(), axis.id)?.len() <= 1 {
        return Err(Error::Invalid(
            Msg::new(format!(
                "'{}' is a required category and this is its last value, so removing it would leave \
                 a demand nobody can meet — lower the requirement first",
                axis.name
            ))
            .with("name", &axis.name),
        ));
    }
    let assignments = read::assignment_ids_of_value(tx.conn(), before.id)?;
    match reassign_to {
        Some(target_id) => {
            let target = live_value_before(tx, target_id)?;
            if target.id == before.id {
                return Err(Error::invalid(format!(
                    "'{}' is the value being removed, so the tasks on it cannot move there — name \
                     another value of '{}'",
                    before.name, axis.name
                )));
            }
            if target.dimension_id != axis.id {
                return Err(Error::invalid(format!(
                    "'{}' is not a value of '{}', so the tasks on '{}' cannot move to it — a task's \
                     classification names a value of the axis it answers",
                    target.name, axis.name, before.name
                )));
            }
            move_assignments(tx, &assignments, target.id)?;
            let on_decisions = read::decision_assignment_ids_of_value(tx.conn(), before.id)?;
            move_decision_assignments(tx, &on_decisions, target.id)?;
        }
        None if axis.required && !assignments.is_empty() => {
            return Err(Error::Invalid(
                Msg::new(format!(
                    "'{}' is a required category and {} task(s) are classified as '{}', so removing \
                     it would leave them with no value at all — say which value they move to instead",
                    axis.name,
                    assignments.len(),
                    before.name
                ))
                .with("name", &axis.name),
            ));
        }
        None => {}
    }
    delete_value_subtree(tx, before.id)
}

/// Re-point assignments at another value of the same axis (checked by the caller). The row keeps its
/// id rather than being deleted and re-created: it is the same classification of the same task, moved,
/// and a carrier reading the change feed sees one update instead of a disappearance and an arrival.
/// One row per `(task, axis)` still holds — the tasks here all answered the axis with the value being
/// removed, so none of them has a second row on it to collide with.
fn move_assignments(tx: &WriteTx<'_>, assignment_ids: &[i64], target_id: i64) -> Result<()> {
    let now = Timestamp::now();
    for &id in assignment_ids {
        let cur = read::task_dimension_value(tx.conn(), id)?
            .expect("the assignment id was just read from the same transaction");
        let moved =
            TaskDimensionValue { value_id: target_id, updated_at: now, ..cur.clone() };
        emit_update(tx, record::task_dimension_value(&cur), record::task_dimension_value(&moved))?;
    }
    Ok(())
}

/// The same move for the decisions on the value (`AMB-D-781`). A destination named on the door means the
/// same thing on either side — say where what carried this value goes, and the decisions go there too
/// rather than being quietly un-classified while the tasks travel. Unnamed, they leave with the value
/// ([`delete_value_subtree`]); a decision itself is never deleted by either road.
fn move_decision_assignments(
    tx: &WriteTx<'_>,
    assignment_ids: &[i64],
    target_id: i64,
) -> Result<()> {
    let now = Timestamp::now();
    for &id in assignment_ids {
        let cur = read::decision_dimension_value(tx.conn(), id)?
            .expect("the assignment id was just read from the same transaction");
        let moved = DecisionDimensionValue { value_id: target_id, updated_at: now, ..cur.clone() };
        emit_update(
            tx,
            record::decision_dimension_value(&cur),
            record::decision_dimension_value(&moved),
        )?;
    }
    Ok(())
}

/// Hard-delete one dimension value and the assignments on it, on both sides (pass an id already checked
/// to exist) —
/// the body of [`value_delete`], and the per-value step [`delete_subtree`] repeats down an axis.
pub(crate) fn delete_value_subtree(tx: &WriteTx<'_>, value_id: i64) -> Result<()> {
    for assignment_id in read::assignment_ids_of_value(tx.conn(), value_id)? {
        tx.delete_record("task_dimension_value", assignment_id)?;
    }
    // The decision side of the same value (`AMB-D-781`). What goes here is the **assignment**, never the
    // decision: removing a value un-classifies what carried it, and a decision is not deleted by a
    // classification going away. This is also the only sweep the decision side needs — an axis is deleted
    // value by value by delete_subtree, so its decision assignments leave with the values they name.
    for assignment_id in read::decision_assignment_ids_of_value(tx.conn(), value_id)? {
        tx.delete_record("decision_dimension_value", assignment_id)?;
    }
    tx.delete_record("dimension_value", value_id)?;
    Ok(())
}

// ───────────────────────────── Assignment to tasks ─────────────────────────────

/// Assign a task to a dimension value. What happens to what was there is the axis's own answer
/// (`AMB-D-826`): on a single-select axis `(task, dimension)` is constrained to one row, so an existing
/// assignment to a different value is deleted and replaced; on a multi-select one the task simply gains
/// a row and keeps the values it had — taking one off is [`unset`], and nothing else. A noop when the
/// same value is already assigned, either way. Returns (row, created). The removal and the insert ride
/// on **the same transaction** — commit them separately and a crash in between leaves zero or two rows
/// for one task on a single-select axis, breaking the one-row invariant.
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

    // Drop the existing assignment on this axis (a different value) first, keeping it to one row — on a
    // single-select axis. A multi-select one is meant to accumulate, so nothing is dropped.
    let now = Timestamp::now();
    if axis.cardinality == DimensionCardinality::Single {
        for id in read::assignment_ids_on_axis(tx.conn(), task_id, dimension_id)? {
            tx.delete_record("task_dimension_value", id)?;
        }
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
///
/// **A required axis has no way back to empty** (`AMB-D-734`). On a single-select axis, moving the task
/// to another value is `set`, which replaces the assignment rather than clearing it; what this would do
/// is leave the axis blank, which is the one state the flag exists to forbid. Refusing here rather than
/// at the next `finish_creating` keeps the task from being emptied out behind the premise's back — the
/// premise is read once, at the door, and a task already through it would never be asked again.
///
/// **What the flag demands is one value, not this value** (`AMB-D-826`). So it is the *last* one that
/// is held: a task answering a multi-select axis with three values gives two of them up freely, and the
/// refusal arrives when the axis would go blank — the same state, reached by the same door, whatever
/// the axis admits.
pub fn unset(tx: &WriteTx<'_>, task_id: i64, value_id: i64) -> Result<bool> {
    let Some(id) = read::assignment_id(tx.conn(), task_id, value_id)? else {
        return Ok(false);
    };
    let dimension_id = read::dimension_id_of_value(tx.conn(), value_id)?
        .ok_or_else(|| VALUE_NOUN.not_found(value_id.to_string()))?;
    let axis = read::dimension(tx.conn(), dimension_id)?
        .ok_or_else(|| NOUN.not_found(dimension_id.to_string()))?;
    if axis.required && read::assignment_ids_on_axis(tx.conn(), task_id, dimension_id)?.len() <= 1 {
        return Err(Error::Invalid(
            Msg::new(format!(
                "'{}' is a required category, so a task's value on it cannot be cleared — assign \
                 another value instead",
                axis.name
            ))
            .coded(ErrorCode::InvalidDimensionRequiredUnset)
            .with("name", &axis.name),
        ));
    }
    tx.delete_record("task_dimension_value", id)?;
    Ok(true)
}

// ───────────────────────────── Assignment to decisions ─────────────────────────────

/// Assign a decision to a dimension value — [`set`]'s twin on the decision side (`AMB-D-781`), reading
/// the axis's `cardinality` the same way (`AMB-D-826`): one row per `(decision, dimension)` on a
/// single-select axis, replaced in one transaction, and an added row on a multi-select one. A noop when
/// the same value is already assigned. Returns (row, created).
///
/// **No `required` here.** The flag bites at the two doors a record passes through once —
/// `task::finish_creating` and `decision::accept` (`AMB-D-790`) — and this is neither. So an axis being
/// required does not demand a value at the moment of assigning, and clearing one is left unconditional
/// ([`unset_on_decision`]): what a required axis holds is the acceptance, which asks again on its way
/// through.
pub fn set_on_decision(
    tx: &WriteTx<'_>,
    decision_id: i64,
    value_id: i64,
) -> Result<(DecisionDimensionValue, bool)> {
    let Some(decision) = read::decision(tx.conn(), decision_id)? else {
        return Err(crate::ops::decision::NOUN.not_found(decision_id.to_string()));
    };
    let dimension_id = read::dimension_id_of_value(tx.conn(), value_id)?
        .ok_or_else(|| VALUE_NOUN.not_found(value_id.to_string()))?;
    let axis = read::dimension(tx.conn(), dimension_id)?
        .ok_or_else(|| NOUN.not_found(dimension_id.to_string()))?;
    // A classification is an edge, and following it reads the other end's project vocabulary — the same
    // reason the task side guards. A decision always sits in a project, so neither end is ever the inbox.
    crate::ops::guard_same_project(
        Some(decision.project_id),
        Some(axis.project_id),
        "this classification",
    )?;

    // Idempotent noop when the same value is already assigned.
    if let Some(id) = read::decision_assignment_id(tx.conn(), decision_id, value_id)? {
        let existing = read::decision_dimension_value(tx.conn(), id)?
            .expect("the assignment id was just read from the same transaction");
        return Ok((existing, false));
    }

    // Drop the existing assignment on this axis (a different value) first, keeping it to one row — on a
    // single-select axis, for the reason the task side gives.
    let now = Timestamp::now();
    if axis.cardinality == DimensionCardinality::Single {
        for id in read::decision_assignment_ids_on_axis(tx.conn(), decision_id, dimension_id)? {
            tx.delete_record("decision_dimension_value", id)?;
        }
    }

    let dv = DecisionDimensionValue {
        id: read::next_id(tx.conn(), "decision_dimension_value")?,
        decision_id,
        dimension_id,
        value_id,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::decision_dimension_value(&dv))?;
    Ok((dv, true))
}

/// Remove a decision's assignment to a particular dimension value (a hard delete). A noop when it is not
/// assigned. Returns changed. Unconditional, for the reason [`set_on_decision`] gives: what a required
/// axis holds on this side is the acceptance, not the assignment — so a settled decision can be stripped
/// of a required value here, and only a fresh acceptance would ask for one again. The task side refuses
/// the same move ([`unset`]) because `finish_creating` is one-directional and would never ask twice.
pub fn unset_on_decision(tx: &WriteTx<'_>, decision_id: i64, value_id: i64) -> Result<bool> {
    let Some(id) = read::decision_assignment_id(tx.conn(), decision_id, value_id)? else {
        return Ok(false);
    };
    tx.delete_record("decision_dimension_value", id)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_decision_in, mk_project, mk_task_in, new_engine};

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

    /// The default every row is born with, and where it comes from (`AMB-D-735`): the id, never the name
    /// — two axes named in Japanese would otherwise be handed the same slug.
    #[test]
    fn a_new_axis_and_a_new_value_are_born_with_a_slug_off_their_id() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d1 = add(tx, p, custom("フェーズ")).unwrap();
        let d2 = add(tx, p, custom("製品")).unwrap();
        assert_eq!(d1.slug.as_deref(), Some(format!("d{}", d1.id).as_str()));
        assert_ne!(d1.slug, d2.slug, "two Japanese names do not collapse onto one slug");
        let v = value_add(tx, d1.id, "運用第2期", None).unwrap();
        assert_eq!(v.slug.as_deref(), Some(format!("v{}", v.id).as_str()));
        // Named at the door instead, and it is the name that is kept.
        let named = add(tx, p, NewDimension { slug: Some("release".into()), ..custom("リリース") })
            .unwrap();
        assert_eq!(named.slug.as_deref(), Some("release"));
    }

    /// A default nobody asked for cannot be refused — the row being created has no way to name another,
    /// since a slug is only editable once the row exists. So it steps past the holder (`AMB-D-735`).
    #[test]
    fn a_default_slug_steps_past_one_an_edit_already_took() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let squatter = add(tx, p, custom("先客")).unwrap();
        // Take the slug the *next* axis would otherwise be born with.
        let next = squatter.id + 1;
        update(tx, squatter.id, None, None, None, None, None, None, None, None, Some(&format!("d{next}")))
            .unwrap();
        let born = add(tx, p, custom("あと")).unwrap();
        assert_eq!(born.id, next);
        assert_eq!(born.slug.as_deref(), Some(format!("d{next}-2").as_str()));

        let v = value_add(tx, born.id, "値", None).unwrap();
        let after = v.id + 1;
        value_set_slug(tx, v.id, &format!("v{after}")).unwrap();
        let born_value = value_add(tx, born.id, "つぎの値", None).unwrap();
        assert_eq!(born_value.id, after);
        assert_eq!(born_value.slug.as_deref(), Some(format!("v{after}-2").as_str()));
    }

    /// Whitespace inside a name is refused at all four doors, and trimmed off the edges (`AMB-D-819`).
    /// A name is what a person filters by, and `dim:<axis>=<name>` is cut on whitespace before it is cut
    /// on `:` — so a name holding any is one the filter can never reach.
    #[test]
    fn a_name_carries_no_whitespace_inside_it() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("軸")).unwrap();
        let v = value_add(tx, d.id, "値", None).unwrap();
        // Every kind of space the filter parts on, the full-width one included — this store is named in
        // Japanese, where U+3000 is a keystroke away.
        for bad in ["日本語の 表記", "日本語の\u{3000}表記", "a\tb", "a\nb"] {
            let code = ErrorCode::InvalidDimensionNameWhitespace.as_str();
            let err = add(tx, p, custom(bad)).unwrap_err();
            assert_eq!(err.code(), code, "{bad:?} is refused on add");
            let err = update(tx, d.id, Some(bad), None, None, None, None, None, None, None, None).unwrap_err();
            assert_eq!(err.code(), code, "{bad:?} is refused on rename");
            let err = value_add(tx, d.id, bad, None).unwrap_err();
            assert_eq!(err.code(), code, "{bad:?} is refused on value-add");
            let err = value_rename(tx, v.id, bad).unwrap_err();
            assert_eq!(err.code(), code, "{bad:?} is refused on value-rename");
        }
        // The edges are the caller's slip, not their intent: they come off, and what is left is kept.
        assert_eq!(add(tx, p, custom("  リリース\u{3000}")).unwrap().name, "リリース");
        assert_eq!(value_add(tx, d.id, " 運用第2期 ", None).unwrap().name, "運用第2期");
    }

    /// The shape the door takes, and the shapes it does not (`AMB-D-735`).
    #[test]
    fn a_slug_is_lower_case_ascii_opening_with_a_letter() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("軸")).unwrap();
        for good in ["release", "run-2", "a", &"a".repeat(SLUG_MAX)] {
            update(tx, d.id, None, None, None, None, None, None, None, None, Some(good)).unwrap();
            assert_eq!(dim(tx, d.id).slug.as_deref(), Some(good));
        }
        for bad in [
            "",              // nothing to read
            "2026",          // opens with a digit — a D-Bus name element may not
            "-release",      // nor with a hyphen
            "Release",       // upper case: the store would keep two, the filesystem one
            "リリース",      // not ASCII
            "re_lease",      // underscore is not in the set
            "re lease",      // nor is a space
            &"a".repeat(SLUG_MAX + 1),
        ] {
            let err = update(tx, d.id, None, None, None, None, None, None, None, None, Some(bad)).unwrap_err();
            assert_eq!(err.code(), ErrorCode::InvalidDimensionSlugShape.as_str(), "{bad:?} is refused");
        }
    }

    /// How far a slug has to be unique, and how far it does not (`AMB-D-735`) — the reach the table's
    /// own constraint holds, said at the door so the refusal names the holder.
    #[test]
    fn a_slug_is_refused_where_it_is_already_taken_and_free_where_it_is_not() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let q = project_named(tx, "QJ");
        let d1 = add(tx, p, NewDimension { slug: Some("phase".into()), ..custom("フェーズ") }).unwrap();
        let d2 = add(tx, p, custom("製品")).unwrap();
        let err = update(tx, d2.id, None, None, None, None, None, None, None, None, Some("phase")).unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidDimensionSlugTaken.as_str());
        // Another project is another reach.
        add(tx, q, NewDimension { slug: Some("phase".into()), ..custom("フェーズ") }).unwrap();
        // Naming an axis the slug it already holds is not a collision with itself.
        update(tx, d1.id, None, None, None, None, None, None, None, None, Some("phase")).unwrap();

        let v1 = value_add(tx, d1.id, "運用第2期", Some("ops2")).unwrap();
        let v2 = value_add(tx, d1.id, "運用第1期", None).unwrap();
        assert_eq!(
            value_set_slug(tx, v2.id, "ops2").unwrap_err().code(),
            ErrorCode::InvalidDimensionSlugTaken.as_str()
        );
        // Another axis is another reach for a value, too.
        value_add(tx, d2.id, "本体", Some("ops2")).unwrap();
        assert_eq!(val(tx, v1.id).slug.as_deref(), Some("ops2"));
    }

    /// **id → slug → name, in that order** (`AMB-D-735`). The tiers are asked one at a time, so a slug
    /// that happens to spell another axis's name does not make either reference ambiguous.
    #[test]
    fn a_reference_resolves_by_key_then_slug_then_name() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let phase = add(tx, p, NewDimension { slug: Some("phase".into()), ..custom("フェーズ") }).unwrap();
        // Named exactly what the other one's slug says, which is what makes the order visible.
        let decoy = add(tx, p, custom("phase")).unwrap();
        assert_eq!(read::resolve_dimension_in(tx.conn(), None, "phase").unwrap(), vec![phase.id]);
        assert_eq!(
            read::resolve_dimension_in(tx.conn(), None, &decoy.id.to_string()).unwrap(),
            vec![decoy.id],
            "the key still wins over both",
        );
        assert_eq!(
            read::resolve_dimension_in(tx.conn(), None, "フェーズ").unwrap(),
            vec![phase.id],
            "a name nobody's slug spells still resolves",
        );

        let ops2 = value_add(tx, phase.id, "運用第2期", Some("ops2")).unwrap();
        let value_decoy = value_add(tx, phase.id, "ops2", None).unwrap();
        assert_eq!(
            read::resolve_dimension_value_in(tx.conn(), phase.id, "ops2").unwrap(),
            vec![ops2.id],
        );
        assert_eq!(
            read::resolve_dimension_value_in(tx.conn(), phase.id, "運用第2期").unwrap(),
            vec![ops2.id],
        );
        assert_eq!(
            read::resolve_dimension_value_in(tx.conn(), phase.id, &value_decoy.id.to_string())
                .unwrap(),
            vec![value_decoy.id],
        );
    }

    #[test]
    fn rename_move_and_resolve_dimensions() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d1 = add(tx, p, custom("D1")).unwrap();
        let d2 = add(tx, p, custom("D2")).unwrap();
        update(tx, d1.id, Some("分類"), None, None, None, None, None, None, None, None).unwrap();
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
        let v1 = value_add(tx, d.id, "低", None).unwrap();
        let v2 = value_add(tx, d.id, "高", None).unwrap();
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
        let v = value_add(tx, d.id, "開発期", None).unwrap();
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
        let ov = value_add(tx, other.id, "バグ", None).unwrap();
        value_set_dates(tx, ov.id, Some(day("2026-01-01")), None).unwrap();
        assert!(current(tx, p, day("2026-07-09")).is_none(), "a non-time_axis axis makes no current era");

        let axis = add(
            tx,
            p,
            NewDimension { role: DimensionRole::TimeAxis, ordered: true, ..custom("時代") },
        )
        .unwrap();
        let dev = value_add(tx, axis.id, "開発期", None).unwrap();
        value_set_dates(tx, dev.id, Some(day("2026-06-20")), Some(day("2026-07-07"))).unwrap();
        let ops1 = value_add(tx, axis.id, "運用第1期", None).unwrap();
        value_set_dates(tx, ops1.id, Some(day("2026-07-08")), None).unwrap();
        // A value with no period has no window, so it covers no day.
        value_add(tx, axis.id, "未設定", None).unwrap();

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
        let now = value_add(tx, d.id, "運用第1期", None).unwrap();
        value_set_dates(tx, now.id, Some(day("2026-07-08")), None).unwrap();
        // An axis with no role makes no current era, dates or not.
        assert!(current(tx, p, day("2026-07-09")).is_none());

        // Nominate it later and the very same dates start acting as windows. Name and notes are
        // left as they were.
        let named = update(tx, d.id, None, None, None, None, Some(DimensionRole::TimeAxis), None, None, None, None).unwrap();
        assert_eq!(named.name, "時代");
        assert_eq!(current(tx, p, day("2026-07-09")).unwrap(), now.id);

        // Un-nominate it and it steps out of the resolution; the dates stay in their columns but
        // stop meaning anything.
        update(tx, d.id, None, None, None, None, Some(DimensionRole::None), None, None, None, None).unwrap();
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
        let a = value_add(tx, d.id, "A", None).unwrap();
        let b = value_add(tx, d.id, "B", None).unwrap();
        assert!(!dim(tx, d.id).ordered);
        assert!(value_move(tx, b.id, Position::Top).is_err(), "an unordered axis cannot be reordered");

        // Turn `ordered` on and `order_key` takes effect, so values can be reordered. Name and
        // notes are left as they were.
        let updated = update(tx, d.id, None, None, None, Some(true), None, None, None, None, None).unwrap();
        assert!(updated.ordered);
        assert_eq!(updated.name, "カテゴリー");
        value_move(tx, b.id, Position::Top).unwrap();
                let order: Vec<String> = vals(tx, d.id).iter().map(|v| v.name.clone()).collect();
        assert_eq!(order, vec!["B", "A"]);

        // Turn `ordered` back off and the values fall back to a stable ascending-id order rather
        // than `order_key`, so the reordering stops showing.
        update(tx, d.id, None, None, None, Some(false), None, None, None, None, None).unwrap();
        assert!(!dim(tx, d.id).ordered);
        let mut expected = [("A", &a.id), ("B", &b.id)];
        expected.sort_by(|x, y| x.1.cmp(y.1));
        let expected_names: Vec<&str> = expected.iter().map(|(n, _)| *n).collect();
                let order: Vec<String> = vals(tx, d.id).iter().map(|v| v.name.clone()).collect();
        assert_eq!(order, expected_names, "unordered means ascending id (independent of the reordering)");
    }

    /// The card flag is the axis's own (`AMB-D-651`), so it starts down, goes up and comes back down
    /// through the same door every face uses, and nothing else on the axis moves with it.
    #[test]
    fn update_toggles_show_on_card_and_leaves_the_rest_alone() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        // Raised in its plain form, an axis is off the card: `AMB-D-40`'s surface is what the cards
        // keep until somebody names an axis to widen it.
        let d = add(tx, p, NewDimension { ordered: true, ..custom("カテゴリー") }).unwrap();
        assert!(!dim(tx, d.id).show_on_card, "a new axis starts off the card");

        // Raise the flag and it is the axis that carries it — name, notes, order and role are not
        // touched on the way through.
        let raised = update(tx, d.id, None, None, None, None, None, Some(true), None, None, None).unwrap();
        assert!(raised.show_on_card);
        assert_eq!(raised.name, "カテゴリー");
        assert!(raised.ordered);
        assert_eq!(raised.role, DimensionRole::None);
        assert!(dim(tx, d.id).show_on_card, "and it is what was written, not just what came back");

        // Lower it again. Nothing about the axis remembers it was ever up.
        update(tx, d.id, None, None, None, None, None, Some(false), None, None, None).unwrap();
        assert!(!dim(tx, d.id).show_on_card);

        // An update that says nothing about the flag leaves it where it stands.
        update(tx, d.id, None, None, None, None, None, Some(true), None, None, None).unwrap();
        update(tx, d.id, Some("区分"), None, None, None, None, None, None, None, None).unwrap();
        let after = dim(tx, d.id);
        assert_eq!(after.name, "区分");
        assert!(after.show_on_card, "an unmentioned flag is not cleared");
    }

    /// `applies_to` is the flag that starts on the *wide* side (`AMB-D-789`): an axis nobody narrowed
    /// classifies both entities, which is what an existing store's axes already did. Narrowing and
    /// widening are both free and neither has a precondition — the flag says where the axis is offered,
    /// not whether it can be answered.
    #[test]
    fn an_axis_is_born_classifying_both_and_narrows_either_way() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("占有")).unwrap();
        assert_eq!(d.applies_to, DimensionAppliesTo::Both, "a new axis classifies both");
        assert!(d.applies_to.on_task() && d.applies_to.on_decision());

        // Narrow it to the work side. The axis offers no values, and that is no obstacle — the one
        // thing `required` would refuse here, this flag has no opinion about.
        let narrowed =
            update(tx, d.id, None, None, None, None, None, None, None, Some(DimensionAppliesTo::Task), None)
                .unwrap();
        assert_eq!(narrowed.applies_to, DimensionAppliesTo::Task);
        assert!(!narrowed.applies_to.on_decision());
        assert_eq!(dim(tx, d.id).applies_to, DimensionAppliesTo::Task, "and it is what was written");

        // Moving it to the other side is just as free; nothing checks what is already assigned.
        update(tx, d.id, None, None, None, None, None, None, None, Some(DimensionAppliesTo::Decision), None)
            .unwrap();
        assert_eq!(dim(tx, d.id).applies_to, DimensionAppliesTo::Decision);

        // An update that says nothing about it leaves it where it stands.
        update(tx, d.id, Some("排他レーン"), None, None, None, None, None, None, None, None).unwrap();
        let after = dim(tx, d.id);
        assert_eq!(after.name, "排他レーン");
        assert_eq!(after.applies_to, DimensionAppliesTo::Decision, "an unmentioned flag is not cleared");
    }

    /// Narrowing takes nothing away (`AMB-D-789`) — the shape `role` takes when a time axis goes back
    /// to `None` and its values keep their dates. The assignment on the side that just stopped counting
    /// stays in its table; it only stops meaning anything.
    #[test]
    fn narrowing_an_axis_leaves_the_assignments_it_no_longer_means_anything_to() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("占有")).unwrap();
        let v = value_add(tx, d.id, "iOS", None).unwrap();
        let t = task_in(tx, "実機で試す", p);
        set(tx, t, v.id).unwrap();

        update(tx, d.id, None, None, None, None, None, None, None, Some(DimensionAppliesTo::Decision), None)
            .unwrap();

        let live = read::assignment_ids_on_axis(tx.conn(), t, d.id).unwrap();
        assert_eq!(live.len(), 1, "the task keeps the value it was given");
        assert_eq!(read::task_dimension_value(tx.conn(), live[0]).unwrap().unwrap().value_id, v.id);
    }

    /// `required` is the one flag with a precondition (`AMB-D-734`): an axis nobody can answer cannot
    /// demand an answer. Both doors refuse it — the axis's birth, where it is necessarily empty, and an
    /// `update` on one whose values were never added — and the guard lifts the moment a value exists.
    #[test]
    fn required_is_refused_until_the_axis_has_a_value() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        // A new axis has no values, so it cannot be born required.
        assert!(
            add(tx, p, NewDimension { required: true, ..custom("プロダクト") }).is_err(),
            "an axis is empty at birth, so requiring it there could never be satisfied"
        );

        let d = add(tx, p, custom("プロダクト")).unwrap();
        assert!(!dim(tx, d.id).required, "a new axis demands nothing");
        assert!(
            update(tx, d.id, None, None, None, None, None, None, Some(true), None, None).is_err(),
            "an axis offering no values cannot be required"
        );

        // Give it something to answer with and the same call goes through.
        value_add(tx, d.id, "Amenbo本体", None).unwrap();
        let raised = update(tx, d.id, None, None, None, None, None, None, Some(true), None, None).unwrap();
        assert!(raised.required);
        assert_eq!(raised.name, "プロダクト", "nothing else on the axis moved with the flag");
        assert!(dim(tx, d.id).required, "and it is what was written, not just what came back");

        // Lowering it is free, and an update that says nothing about it leaves it standing.
        update(tx, d.id, None, None, None, None, None, None, Some(false), None, None).unwrap();
        assert!(!dim(tx, d.id).required);
        update(tx, d.id, None, None, None, None, None, None, Some(true), None, None).unwrap();
        update(tx, d.id, Some("製品"), None, None, None, None, None, None, None, None).unwrap();
        assert!(dim(tx, d.id).required, "an unmentioned flag is not cleared");
    }

    /// A required axis has no route back to empty (`AMB-D-734`): `set` moves the task to another value,
    /// and `unset` — the one call that would leave the axis blank — is refused.
    #[test]
    fn a_required_axis_cannot_be_cleared_off_a_task() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "a task", p);
        let axis = add(tx, p, custom("プロダクト")).unwrap();
        let core = value_add(tx, axis.id, "Amenbo本体", None).unwrap();
        let site = value_add(tx, axis.id, "Amenboサイト", None).unwrap();
        set(tx, t, core.id).unwrap();

        // While the axis demands nothing, clearing the assignment is an ordinary removal.
        assert!(unset(tx, t, core.id).unwrap());
        set(tx, t, core.id).unwrap();

        update(tx, axis.id, None, None, None, None, None, None, Some(true), None, None).unwrap();
        assert!(unset(tx, t, core.id).is_err(), "a required axis cannot be emptied");
        // Moving to another value on the same axis is `set`, and it still works — the axis stays answered.
        set(tx, t, site.id).unwrap();
        assert_eq!(read::task_dimension_assignments(tx.conn(), t).unwrap(), vec![(axis.id, site.id)]);
        // An axis that demands nothing is unaffected by the one that does.
        let free = add(tx, p, custom("フェーズ")).unwrap();
        let stage = value_add(tx, free.id, "運用第2期", None).unwrap();
        set(tx, t, stage.id).unwrap();
        assert!(unset(tx, t, stage.id).unwrap());
    }

    /// The other route back to empty (`AMB-D-734`): deleting the values themselves. A required axis
    /// keeps its last one, because losing it would leave a demand nobody can meet.
    #[test]
    fn a_required_axis_keeps_its_last_value() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "a task", p);
        let axis = add(tx, p, custom("プロダクト")).unwrap();
        let core = value_add(tx, axis.id, "Amenbo本体", None).unwrap();
        let site = value_add(tx, axis.id, "Amenboサイト", None).unwrap();
        set(tx, t, core.id).unwrap();
        update(tx, axis.id, None, None, None, None, None, None, Some(true), None, None).unwrap();

        // Down to the last one is fine — the axis can still be answered. Nobody answers with this one,
        // so it goes without anywhere to send anyone.
        value_delete(tx, site.id, None).unwrap();
        assert!(val_opt(tx, site.id).is_none());

        assert!(value_delete(tx, core.id, None).is_err(), "the last value of a required axis stays");
        assert!(val_opt(tx, core.id).is_some(), "and the refusal wrote nothing");
        assert_eq!(
            read::task_dimension_assignments(tx.conn(), t).unwrap(),
            vec![(axis.id, core.id)],
            "so the task that answered it is still answering",
        );

        // Lowering the flag is the way out, and then the value is ordinary again.
        update(tx, axis.id, None, None, None, None, None, None, Some(false), None, None).unwrap();
        value_delete(tx, core.id, None).unwrap();
        assert!(val_opt(tx, core.id).is_none());
    }

    /// The same refusal, one step earlier (`AMB-D-734`, `AMB-D-751`): a required axis will not let a
    /// value go while tasks answer with it, because they would be emptied out behind the creation
    /// premise's back — the state `unset` refuses one task at a time. `reassign_to` is what says where
    /// they go, and then the value goes.
    #[test]
    fn a_required_axis_wants_somewhere_for_its_tasks_to_go() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "a task", p);
        let axis = add(tx, p, custom("テーマ")).unwrap();
        let main = value_add(tx, axis.id, "メイン", None).unwrap();
        let theme = value_add(tx, axis.id, "検索の作り直し", None).unwrap();
        let other = add(tx, p, custom("プロダクト")).unwrap();
        let core = value_add(tx, other.id, "Amenbo本体", None).unwrap();
        set(tx, t, theme.id).unwrap();
        update(tx, axis.id, None, None, None, None, None, None, Some(true), None, None).unwrap();

        let assignment = read::assignment_id(tx.conn(), t, theme.id).unwrap().unwrap();

        assert!(value_delete(tx, theme.id, None).is_err(), "a task answers with it, so it stays");
        assert!(val_opt(tx, theme.id).is_some(), "and the refusal wrote nothing");

        // A destination has to be somewhere the classification could actually land.
        assert!(value_delete(tx, theme.id, Some(core.id)).is_err(), "not a value of this axis");
        assert!(value_delete(tx, theme.id, Some(theme.id)).is_err(), "not the value being removed");
        assert_eq!(
            read::task_dimension_assignments(tx.conn(), t).unwrap(),
            vec![(axis.id, theme.id)],
            "neither refusal moved anybody",
        );

        value_delete(tx, theme.id, Some(main.id)).unwrap();
        assert!(val_opt(tx, theme.id).is_none(), "and now it goes");
        assert_eq!(
            read::task_dimension_assignments(tx.conn(), t).unwrap(),
            vec![(axis.id, main.id)],
            "with the task carried over rather than emptied",
        );
        assert_eq!(
            read::assignment_id(tx.conn(), t, main.id).unwrap(),
            Some(assignment),
            "the same classification, moved — not a new row",
        );
    }

    /// An axis that demands nothing is unchanged: its assignments still go with the value. Naming a
    /// destination there is honoured all the same — it means the same thing on any axis.
    #[test]
    fn an_optional_axis_still_lets_its_assignments_go() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let dropped = task_in(tx, "dropped", p);
        let moved = task_in(tx, "moved", p);
        let axis = add(tx, p, custom("区分")).unwrap();
        let bug = value_add(tx, axis.id, "バグ", None).unwrap();
        let chore = value_add(tx, axis.id, "雑務", None).unwrap();
        set(tx, dropped, bug.id).unwrap();
        set(tx, moved, chore.id).unwrap();

        value_delete(tx, bug.id, None).unwrap();
        assert!(read::task_dimension_assignments(tx.conn(), dropped).unwrap().is_empty());

        let main = value_add(tx, axis.id, "その他", None).unwrap();
        value_delete(tx, chore.id, Some(main.id)).unwrap();
        assert_eq!(
            read::task_dimension_assignments(tx.conn(), moved).unwrap(),
            vec![(axis.id, main.id)],
        );
    }

    /// Deleting the axis is not that door: an axis that is gone demands nothing, so its values go with
    /// it however the flag stood.
    #[test]
    fn deleting_a_required_axis_takes_its_last_value_with_it() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let axis = add(tx, p, custom("プロダクト")).unwrap();
        let core = value_add(tx, axis.id, "Amenbo本体", None).unwrap();
        update(tx, axis.id, None, None, None, None, None, None, Some(true), None, None).unwrap();
        delete(tx, axis.id).unwrap();
        assert!(val_opt(tx, core.id).is_none());
    }

    #[test]
    fn unordered_values_cannot_be_reordered() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, NewDimension { name: "色".into(), ordered: false, ..NewDimension::default() }).unwrap();
        let v = value_add(tx, d.id, "赤", None).unwrap();
        assert!(value_move(tx, v.id, Position::Top).is_err());
    }

    #[test]
    fn value_ops_reject_a_missing_dimension() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        // A value op on a dimension that does not exist is not_found.
        assert!(value_add(tx, 999_999, "done", None).is_err());
    }

    /// Deleting a value removes the row, and the task assignments naming it go with it.
    #[test]
    fn value_delete_takes_the_assignments() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("カテゴリー")).unwrap();
        let v = value_add(tx, d.id, "バグ", None).unwrap();
        let t = task_in(tx, "task", p);
        set(tx, t, v.id).unwrap();
        value_delete(tx, v.id, None).unwrap();
        assert!(val_opt(tx, v.id).is_none());
        assert!(read::assignment_id(tx.conn(), t, v.id).unwrap().is_none(), "the assignments go too");
    }

    #[test]
    fn set_replaces_within_an_axis_and_axes_are_independent() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "task", p);

        // A dimension is single-select unless its raiser says otherwise: setting a different value drops
        // the previous assignment and keeps it to one row.
        let cat = add(tx, p, custom("カテゴリー")).unwrap();
        let a = value_add(tx, cat.id, "A", None).unwrap();
        let b = value_add(tx, cat.id, "B", None).unwrap();
        set(tx, t, a.id).unwrap();
        set(tx, t, b.id).unwrap();
        let live = read::assignment_ids_on_axis(tx.conn(), t, cat.id).unwrap();
        assert_eq!(live.len(), 1, "single-select means one row per (task,dimension)");
        let row = read::task_dimension_value(tx.conn(), live[0]).unwrap().unwrap();
        assert_eq!(row.value_id, b.id);

        // The one-row constraint is per axis; assignments on a different axis coexist.
        let area = add(tx, p, custom("領域")).unwrap();
        let core = value_add(tx, area.id, "core", None).unwrap();
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
        let v = value_add(tx, d.id, "A", None).unwrap();
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
        let v = value_add(tx, d.id, "A", None).unwrap();
        assert!(set(tx, 9999, v.id).is_err(), "assigning to a task that does not exist is refused");
        let t = task_in(tx, "task", p);
        assert!(set(tx, t, 999_999).is_err());
    }

    /// The decision side of the axis (`AMB-D-781`): single-select per `(decision, axis)`, idempotent, and
    /// independent of the task assignments that name the same value.
    #[test]
    fn a_decision_carries_one_value_per_axis_and_shares_the_axis_with_tasks() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let k = mk_decision_in(tx, "決定", p);
        let cat = add(tx, p, custom("カテゴリー")).unwrap();
        let a = value_add(tx, cat.id, "A", None).unwrap();
        let b = value_add(tx, cat.id, "B", None).unwrap();

        let (_, created1) = set_on_decision(tx, k, a.id).unwrap();
        let (_, created2) = set_on_decision(tx, k, a.id).unwrap();
        assert!(created1 && !created2, "setting the same value again is idempotent");

        set_on_decision(tx, k, b.id).unwrap();
        let live = read::decision_assignment_ids_on_axis(tx.conn(), k, cat.id).unwrap();
        assert_eq!(live.len(), 1, "single-select means one row per (decision,axis)");
        let row = read::decision_dimension_value(tx.conn(), live[0]).unwrap().unwrap();
        assert_eq!(row.value_id, b.id);

        // The values are the axis's own: a task on the same value is a separate row, and neither side
        // reads the other's.
        let t = task_in(tx, "task", p);
        set(tx, t, a.id).unwrap();
        assert_eq!(read::task_dimension_assignments(tx.conn(), t).unwrap(), vec![(cat.id, a.id)]);
        assert_eq!(read::decision_dimension_assignments(tx.conn(), k).unwrap(), vec![(cat.id, b.id)]);
        assert_eq!(
            read::decision_classification(tx.conn(), k).unwrap(),
            vec![("カテゴリー".to_string(), "B".to_string())]
        );

        assert!(unset_on_decision(tx, k, b.id).unwrap());
        assert!(!unset_on_decision(tx, k, b.id).unwrap(), "already unset is a noop");
    }

    /// What a required axis holds on the decision side is the acceptance (`AMB-D-790`), not the
    /// assignment — so a decision's value on one still clears, and removing the value itself still
    /// counts tasks only. The task side keeps refusing both, its door being one-directional.
    #[test]
    fn a_required_axis_still_lets_a_decisions_value_be_cleared() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let k = mk_decision_in(tx, "決定", p);
        // An axis is born with no values, so the flag is raised once it has some.
        let axis = add(tx, p, custom("カテゴリー")).unwrap();
        let a = value_add(tx, axis.id, "A", None).unwrap();
        let b = value_add(tx, axis.id, "B", None).unwrap();
        update(tx, axis.id, None, None, None, None, None, None, Some(true), None, None).unwrap();
        set_on_decision(tx, k, a.id).unwrap();
        assert!(unset_on_decision(tx, k, a.id).unwrap(), "a decision's value on a required axis clears");

        // And the required-axis guard on `value_delete` still counts tasks only: a value carrying nothing
        // but decisions is removable without naming a destination.
        set_on_decision(tx, k, b.id).unwrap();
        value_delete(tx, b.id, None).unwrap();
        assert!(val_opt(tx, b.id).is_none());
        assert!(
            read::decision_assignment_id(tx.conn(), k, b.id).unwrap().is_none(),
            "the decision keeps existing, unclassified"
        );
        assert!(read::decision(tx.conn(), k).unwrap().is_some(), "removing a value deletes no decision");
    }

    /// Removing a value un-classifies both sides; naming a destination moves both, so a decision is never
    /// quietly left behind while the tasks travel.
    #[test]
    fn value_delete_carries_the_decisions_with_the_tasks() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let axis = add(tx, p, custom("カテゴリー")).unwrap();
        let going = value_add(tx, axis.id, "旧", None).unwrap();
        let staying = value_add(tx, axis.id, "新", None).unwrap();
        let t = task_in(tx, "task", p);
        let k = mk_decision_in(tx, "決定", p);
        set(tx, t, going.id).unwrap();
        set_on_decision(tx, k, going.id).unwrap();

        value_delete(tx, going.id, Some(staying.id)).unwrap();
        assert_eq!(read::task_dimension_assignments(tx.conn(), t).unwrap(), vec![(axis.id, staying.id)]);
        assert_eq!(
            read::decision_dimension_assignments(tx.conn(), k).unwrap(),
            vec![(axis.id, staying.id)],
            "the decisions move where the tasks moved"
        );

        // Unnamed, both sides leave with the value.
        value_delete(tx, staying.id, None).unwrap();
        assert!(read::assignment_id(tx.conn(), t, staying.id).unwrap().is_none());
        assert!(read::decision_assignment_id(tx.conn(), k, staying.id).unwrap().is_none());
    }

    /// Deleting a dimension takes its values (dependent content) and assignments (links) with it.
    #[test]
    fn delete_dimension_takes_values_and_assignments() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let d = add(tx, p, custom("カテゴリー")).unwrap();
        let v = value_add(tx, d.id, "A", None).unwrap();
        let t = task_in(tx, "task", p);
        set(tx, t, v.id).unwrap();
        let k = mk_decision_in(tx, "決定", p);
        set_on_decision(tx, k, v.id).unwrap();
        delete(tx, d.id).unwrap();
        assert!(dim_opt(tx, d.id).is_none());
        assert!(val_opt(tx, v.id).is_none(), "the values go too");
        assert!(read::assignment_id(tx.conn(), t, v.id).unwrap().is_none(), "and the assignments on them");
        assert!(
            read::decision_assignment_id(tx.conn(), k, v.id).unwrap().is_none(),
            "on either side — an axis is swept value by value, so the decision rows leave with them"
        );
    }

    /// The axis's own answer to how many of its values a record may hold (`AMB-D-826`): single replaces,
    /// multi accumulates, and taking one off is `unset` either way. Both sides of the axis read the same
    /// flag (`AMB-D-781`).
    #[test]
    fn a_multi_axis_gathers_values_where_a_single_one_replaces() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "task", p);
        let k = mk_decision_in(tx, "決定", p);
        let axis = add(tx, p, custom("プロダクト")).unwrap();
        let core = value_add(tx, axis.id, "Amenbo本体", None).unwrap();
        let site = value_add(tx, axis.id, "Amenboサイト", None).unwrap();

        // Single-select while nobody has said otherwise: the second value takes the first one's place.
        set(tx, t, core.id).unwrap();
        set(tx, t, site.id).unwrap();
        assert_eq!(read::task_dimension_assignments(tx.conn(), t).unwrap(), vec![(axis.id, site.id)]);

        update(tx, axis.id, None, None, Some(DimensionCardinality::Multi), None, None, None, None, None, None)
            .unwrap();

        // From here the axis gathers: the value that was there stays, and the new one joins it.
        set(tx, t, core.id).unwrap();
        assert_eq!(
            read::task_dimension_assignments(tx.conn(), t).unwrap(),
            vec![(axis.id, site.id), (axis.id, core.id)],
            "a multi axis keeps what it had",
        );
        let (_, created) = set(tx, t, core.id).unwrap();
        assert!(!created, "setting the same value again is still idempotent");

        // The decision side reads the same flag.
        set_on_decision(tx, k, core.id).unwrap();
        set_on_decision(tx, k, site.id).unwrap();
        assert_eq!(
            read::decision_dimension_assignments(tx.conn(), k).unwrap(),
            vec![(axis.id, core.id), (axis.id, site.id)],
        );

        // And what is printed lines the values of one axis up, ordered by the axis's own order.
        assert_eq!(
            read::task_classification(tx.conn(), t).unwrap(),
            vec![
                ("プロダクト".to_string(), "Amenbo本体".to_string()),
                ("プロダクト".to_string(), "Amenboサイト".to_string()),
            ],
        );

        // Taking one value off leaves the rest — the removal is `unset`, and nothing else.
        assert!(unset(tx, t, site.id).unwrap());
        assert_eq!(read::task_dimension_assignments(tx.conn(), t).unwrap(), vec![(axis.id, core.id)]);
    }

    /// `required` is met by one value or more (`AMB-D-826`, `AMB-D-734`): a task answering a multi axis
    /// with three gives two of them up freely, and it is the last one the flag holds.
    #[test]
    fn a_required_multi_axis_holds_only_the_last_value() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "task", p);
        let axis = add(tx, p, custom("プロダクト")).unwrap();
        let core = value_add(tx, axis.id, "Amenbo本体", None).unwrap();
        let site = value_add(tx, axis.id, "Amenboサイト", None).unwrap();
        update(
            tx,
            axis.id,
            None,
            None,
            Some(DimensionCardinality::Multi),
            None,
            None,
            None,
            Some(true),
            None,
            None,
        )
        .unwrap();
        set(tx, t, core.id).unwrap();
        set(tx, t, site.id).unwrap();

        assert!(unset(tx, t, site.id).unwrap(), "one of several goes freely");
        assert!(unset(tx, t, core.id).is_err(), "the last one is what a required axis holds");
        assert_eq!(read::task_dimension_assignments(tx.conn(), t).unwrap(), vec![(axis.id, core.id)]);

        // And the creation premise reads the same "one or more": the task is answered, so it passes.
        assert!(crate::ops::task::finish_creating(tx, t).is_ok());
    }

    /// The time axis holds one value at a time (`AMB-D-826`), and the refusal is the same whichever half
    /// of the pair moved — `add` where they arrive together, `update` from either side.
    #[test]
    fn the_time_axis_refuses_to_admit_several_values() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");

        let born = NewDimension {
            role: DimensionRole::TimeAxis,
            cardinality: DimensionCardinality::Multi,
            ..custom("時代")
        };
        assert!(add(tx, p, born).is_err(), "the pair is refused where it arrives already made");

        // Multi first, then nominated as the time axis.
        let wide = add(
            tx,
            p,
            NewDimension { cardinality: DimensionCardinality::Multi, ..custom("プロダクト") },
        )
        .unwrap();
        assert!(
            update(tx, wide.id, None, None, None, None, Some(DimensionRole::TimeAxis), None, None, None, None)
                .is_err(),
            "and refused from the role's side",
        );

        // Time axis first, then widened.
        let era = add(tx, p, NewDimension { role: DimensionRole::TimeAxis, ..custom("フェーズ") }).unwrap();
        assert!(
            update(tx, era.id, None, None, Some(DimensionCardinality::Multi), None, None, None, None, None, None)
                .is_err(),
            "and from the cardinality's side",
        );
        assert_eq!(dim(tx, era.id).cardinality, DimensionCardinality::Single, "the refusal wrote nothing");
    }

    /// A demotion throws values away, so it asks first and names how many records it would take them
    /// from (`AMB-D-826`) — both sides of the axis counted, since both answer it.
    #[test]
    fn demoting_a_multi_axis_is_refused_while_records_answer_with_several() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = project_named(tx, "PJ");
        let t = task_in(tx, "task", p);
        let k = mk_decision_in(tx, "決定", p);
        let axis = add(
            tx,
            p,
            NewDimension { cardinality: DimensionCardinality::Multi, ..custom("プロダクト") },
        )
        .unwrap();
        let core = value_add(tx, axis.id, "Amenbo本体", None).unwrap();
        let site = value_add(tx, axis.id, "Amenboサイト", None).unwrap();

        // Nobody answers with several yet, so the demotion is an ordinary edit — and widening back is free.
        set(tx, t, core.id).unwrap();
        update(tx, axis.id, None, None, Some(DimensionCardinality::Single), None, None, None, None, None, None)
            .unwrap();
        update(tx, axis.id, None, None, Some(DimensionCardinality::Multi), None, None, None, None, None, None)
            .unwrap();

        set(tx, t, site.id).unwrap();
        set_on_decision(tx, k, core.id).unwrap();
        set_on_decision(tx, k, site.id).unwrap();
        let refused = update(
            tx,
            axis.id,
            None,
            None,
            Some(DimensionCardinality::Single),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(refused.to_string().contains('2'), "the refusal names the count: {refused}");
        assert_eq!(dim(tx, axis.id).cardinality, DimensionCardinality::Multi, "and wrote nothing");

        // Clearing the extra values off both is what opens the way.
        assert!(unset(tx, t, site.id).unwrap());
        assert!(unset_on_decision(tx, k, site.id).unwrap());
        update(tx, axis.id, None, None, Some(DimensionCardinality::Single), None, None, None, None, None, None)
            .unwrap();
        assert_eq!(dim(tx, axis.id).cardinality, DimensionCardinality::Single);
    }
}
