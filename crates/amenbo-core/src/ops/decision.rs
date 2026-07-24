//! Decision-record operations.
//!
//! A decision records *why*. Its body is **edited in place**, whatever its status: a proposed
//! decision while it is still under discussion, and an accepted one for a later correction or
//! refinement (`AMB-D-363`). Editing is not re-deciding — an edit leaves the `decided_*` stamps
//! standing — and there is no revision history. What edit does *not* do is overturn: to replace an
//! accepted decision with a different conclusion, `supersede` it (the old one stays readable in the
//! chain); to pull a too-hastily accepted one back into debate, `reopen` it to `Proposed`. A rejected
//! decision is terminal. Decisions have no mailbox workflow the way tasks do.
//! They are numbered and resolved in a namespace of their own, separate from tasks, and displayed
//! as `D-N`.
//!
//! Every mutator takes a [`WriteTx`] (`BEGIN IMMEDIATE`) opened by the caller (the write wrappers
//! on [`crate::Store`]) and does all of its reading **inside that same transaction**: the `before`
//! snapshot, the existence checks, and the read-then-write behind [`add`]'s `next_id`. Read the
//! number outside the transaction and two writers will take the same one.

use crate::error::{Error, Result};
use crate::model::{
    ActorKind, Decision, DecisionComment, DecisionEdge, DecisionEdgeKind, DecisionStatus,
    DecisionTaskLink,
};
use crate::ops::{emit_create, emit_update, Noun};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// This entity's noun (the English/Japanese pair used in not_found messages).
pub(crate) const NOUN: Noun = Noun { en: "decision", ja: "決定" };

/// Append a comment to a decision record (written to its own `decision_comment` table).
/// `decision_id` is a decision id the caller has already resolved. The empty-body check is shared
/// with task comments through [`crate::ops::comment::prepare_comment`].
pub fn add_comment(
    tx: &WriteTx<'_>,
    decision_id: i64,
    author_kind: ActorKind,
    text: &str,
) -> Result<DecisionComment> {
    live_before(tx, decision_id)?;
    let now = crate::ops::comment::prepare_comment(text)?;
    let comment = DecisionComment {
        id: read::next_id(tx.conn(), "decision_comment")?,
        decision_id,
        author_kind: Some(author_kind),
        text: text.to_string(),
        created_at: now,
        updated_at: now,
        edited_at: None,
    };
    emit_create(tx, record::decision_comment(&comment))?;
    Ok(comment)
}

/// Hard-delete a decision comment (the mirror of [`crate::ops::comment::remove_comment`] on the
/// task side). A noop (`false`) if it is already gone. It works even under an accepted decision:
/// what freezes is the decision's *body*, and we never promised to keep a misposted remark from
/// the discussion held underneath it.
pub fn remove_comment(tx: &WriteTx<'_>, id: i64) -> Result<bool> {
    if read::decision_comment(tx.conn(), id)?.is_none() {
        return Ok(false);
    }
    crate::ops::sweep_polymorphic(tx, "decision_comment", id)?;
    tx.delete_record("decision_comment", id)?;
    Ok(true)
}

/// Rewrite the body of a decision comment (the mirror of [`crate::ops::comment::edit_comment`] on
/// the task side). not_found if it is gone. It works even under an accepted decision: what freezes
/// is the decision's body, not the comments below it.
pub fn edit_comment(tx: &WriteTx<'_>, id: i64, text: &str) -> Result<DecisionComment> {
    let before = read::decision_comment(tx.conn(), id)?
        
        .ok_or_else(|| crate::ops::comment::COMMENT_NOUN.not_found(id.to_string()))?;
    let now = crate::ops::comment::prepare_comment(text)?;
    let after = DecisionComment {
        text: text.to_string(),
        updated_at: now,
        edited_at: Some(now),
        ..before.clone()
    };
    emit_update(tx, record::decision_comment(&before), record::decision_comment(&after))?;
    Ok(after)
}

pub struct NewDecision {
    pub title: String,
    pub body: String,
    /// The resolved project id: a decision always lives under a project and never multi-homes.
    pub project_id: i64,
}

/// Create a decision as `Proposed` (under discussion), assigning the conversational number (`D-N`)
/// that is its id. Tasks and decisions live in **separate number spaces**, so the next number comes
/// from looking at decisions alone. `next_id` is read inside the same `BEGIN IMMEDIATE` as the
/// write — read it outside and two concurrent writers take the same number.
pub fn add(tx: &WriteTx<'_>, input: NewDecision) -> Result<Decision> {
    if input.title.trim().is_empty() {
        return Err(Error::invalid(
            "a decision title cannot be empty",
            "決定のタイトルは空にできません",
        ));
    }
    if read::project(tx.conn(), input.project_id)?.is_none() {
        return Err(crate::ops::project::NOUN.not_found(input.project_id.to_string()));
    }
    let now = Timestamp::now();
    // Numbers are **globally unique on this machine**: the mark is read without narrowing by project.
    let id = read::next_id(tx.conn(), "decision")?;
    let decision = Decision {
        id,
        project_id: input.project_id,
        title: input.title,
        body: input.body,
        status: DecisionStatus::Proposed,
        // Proposing *is* the first status transition, so the status clock starts here (`AMB-D-373`).
        status_changed_at: Some(now),
        decided_at: None,
        decided_by: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::decision(&decision))?;
    Ok(decision)
}


#[derive(Default)]
pub struct DecisionPatch {
    pub title: Option<String>,
    pub body: Option<String>,
}

/// Edit a decision's title/body in place. Both a `Proposed` decision (edit-while-proposed) and an
/// `Accepted` one edit directly — accepting no longer freezes the body (`AMB-D-363`). Editing is not
/// re-deciding, so `decided_at`/`decided_by` are left untouched, and there is no versioning: to
/// overturn an accepted decision rather than refine it, `supersede` it. A `Rejected` decision is
/// terminal and cannot be edited.
pub fn update(tx: &WriteTx<'_>, id: i64, patch: DecisionPatch) -> Result<Decision> {
    let before = live_before(tx, id)?;
    if before.status == DecisionStatus::Rejected {
        return Err(Error::invalid(
            format!("decision '{id}' is rejected and cannot be edited"),
            format!("決定 '{id}' は却下済みのため編集できません"),
        ));
    }
    let mut d = before.clone();
    if let Some(title) = patch.title {
        if title.trim().is_empty() {
            return Err(Error::invalid(
                "a decision title cannot be empty",
                "決定のタイトルは空にできません",
            ));
        }
        d.title = title;
    }
    if let Some(body) = patch.body {
        d.body = body;
    }
    d.updated_at = Timestamp::now();
    emit_update(tx, record::decision(&before), record::decision(&d))?;
    Ok(d)
}

/// Accept a decision (`Proposed` → `Accepted`), stamping `decided_at`/`decided_by`. Idempotent when
/// it is already `Accepted` (re-accepting is a noop). Accepting a `Rejected` decision is an error.
///
/// Returns `(decision, changed)`. `changed` is `false` on the idempotent noop (already `Accepted`) so
/// the caller can tell "settled it" from "nothing happened" and not report a fresh acceptance that
/// never occurred — the freeze on `decided_by`/`decided_at` still holds, and re-stamping a different
/// facet is [`reopen`]'s business, not a silent overwrite here.
pub fn accept(tx: &WriteTx<'_>, id: i64, decided_by: Option<String>) -> Result<(Decision, bool)> {
    let now = Timestamp::now();
    let decided_by = decided_by.map(|t| t.trim().to_string());
    let before = live_before(tx, id)?;
    match before.status {
        DecisionStatus::Accepted => return Ok((before, false)), // idempotent: already settled, nothing changed
        DecisionStatus::Proposed => {}
        other => {
            return Err(Error::invalid(
                format!("decision '{id}' is {} and cannot be accepted", other.as_str()),
                format!("決定 '{id}' は {} のため採択できません", other.as_str()),
            ))
        }
    }
    // Every arm above either returned or left `Proposed`, so reaching here *is* the transition — the
    // idempotent re-accept never gets this far, and so never moves the clock.
    let after = Decision {
        status: DecisionStatus::Accepted,
        status_changed_at: Some(now),
        decided_at: Some(now),
        decided_by,
        updated_at: now,
        ..before.clone()
    };
    emit_update(tx, record::decision(&before), record::decision(&after))?;
    Ok((after, true))
}

/// Reject a decision (`Proposed` → `Rejected`). Idempotent when it is already `Rejected`.
/// Rejecting an `Accepted` decision is an error — a settled decision is replaced by superseding it.
///
/// Returns `(decision, changed)`. `changed` is `false` on the idempotent noop (already `Rejected`), so
/// the caller does not report a fresh rejection that never happened.
pub fn reject(tx: &WriteTx<'_>, id: i64) -> Result<(Decision, bool)> {
    let before = live_before(tx, id)?;
    match before.status {
        DecisionStatus::Rejected => return Ok((before, false)), // idempotent: already rejected, nothing changed
        DecisionStatus::Proposed => {}
        other => {
            return Err(Error::invalid(
                format!("decision '{id}' is {} and cannot be rejected", other.as_str()),
                format!("決定 '{id}' は {} のため却下できません", other.as_str()),
            ))
        }
    }
    let now = Timestamp::now();
    let after = Decision {
        status: DecisionStatus::Rejected,
        status_changed_at: Some(now),
        updated_at: now,
        ..before.clone()
    };
    emit_update(tx, record::decision(&before), record::decision(&after))?;
    Ok((after, true))
}

/// Return an accepted decision to discussion (`Accepted` → `Proposed`), un-settling it. It clears
/// `decided_at`/`decided_by` and, as a side effect unique to this route, sends the tasks that rest on
/// it back to `ready:no`. Use it to pull a too-hastily accepted decision back into debate — a use
/// neither `reject` (a negative verdict) nor `supersede` (a replacement) expresses. It is **not** a
/// precondition for editing: an accepted decision edits in place ([`update`], `AMB-D-363`), so reopen
/// is only for un-deciding. Idempotent (a noop) when it is already `Proposed`. A `Rejected` decision
/// cannot be reopened — reject has no inverse. A superseded decision *can* be, because being
/// superseded is not a status but a projection of the edges: drop the edge behind an erroneous
/// supersede and the target is current again. Reopen is non-destructive and reversible, so it is open
/// to AI actors too. Returns `(decision, changed)` like its sibling verdicts, so a caller can tell the
/// noop from a premise it has just un-settled.
pub fn reopen(tx: &WriteTx<'_>, id: i64) -> Result<(Decision, bool)> {
    let before = live_before(tx, id)?;
    match before.status {
        DecisionStatus::Proposed => return Ok((before, false)), // idempotent (already editable)
        DecisionStatus::Accepted => {}
        other => {
            return Err(Error::invalid(
                format!("decision '{id}' is {} and cannot be reopened", other.as_str()),
                format!("決定 '{id}' は {} のため議論中に戻せません", other.as_str()),
            ))
        }
    }
    let now = Timestamp::now();
    let after = Decision {
        status: DecisionStatus::Proposed,
        // The one stamp the reopen axis is built on: a decision that re-opened *after* a task was reserved
        // is a premise that moved under it (`AMB-D-373`). `decided_at` is cleared on this very route, which
        // is why the axis cannot be read off it.
        status_changed_at: Some(now),
        decided_at: None,
        decided_by: None,
        updated_at: now,
        ..before.clone()
    };
    emit_update(tx, record::decision(&before), record::decision(&after))?;
    Ok((after, true))
}

/// Replace decision `old_id` with decision `new_id` (the supersession chain — the heart of
/// append-only). The replacement is recorded as **a single edge row**: the `new_id → old_id`
/// `supersedes` edge (see [`put_edge`]). **The old row is left alone** — "has been replaced" is not
/// a status but the far end of an edge, i.e. currency is derived from the edges. The new decision
/// ought to be settled, so if it is still `Proposed` it is promoted to `Accepted` in the same
/// breath. Edges are rows, so **one decision can supersede several old ones** (a DAG).
/// Self-reference is rejected.
///
/// Returns `(new_decision, changed, promoted)`. `changed` is `false` only when the supersedes edge was
/// already there and the new side was already `Accepted` — i.e. re-running it did nothing — so the
/// caller does not announce a fresh supersession that never happened. `promoted` is the narrower fact:
/// `true` only when this call moved the new side `Proposed → Accepted`, so a caller that observes
/// acceptances (`decision.accepted`) fires on the promotion alone, not on merely drawing the edge over
/// an already-accepted side.
pub fn supersede(
    tx: &WriteTx<'_>,
    new_id: i64,
    old_id: i64,
    decided_by: Option<String>,
) -> Result<(Decision, bool, bool)> {
    if new_id == old_id {
        return Err(Error::invalid(
            "a decision cannot supersede itself",
            "決定は自分自身を置き換えられません",
        ));
    }
    // Only check that the old side is alive; its row is never rewritten.
    live_before(tx, old_id)?;
    let new_before = live_before(tx, new_id)?;
    let now = Timestamp::now();
    let decided_by = decided_by.map(|t| t.trim().to_string());

    let edge_changed = put_edge(tx, new_id, old_id, DecisionEdgeKind::Supersedes)?;

    // New side: promote Proposed to Accepted (the edge itself is put_edge's business).
    let mut promoted = false;
    if new_before.status == DecisionStatus::Proposed {
        let new_after = Decision {
            status: DecisionStatus::Accepted,
            // The promotion is a status transition like any other; the old side's row is not rewritten
            // here, and being superseded is an edge rather than a status, so its clock stays where it is.
            status_changed_at: Some(now),
            decided_at: Some(now),
            decided_by,
            updated_at: now,
            ..new_before.clone()
        };
        emit_update(tx, record::decision(&new_before), record::decision(&new_after))?;
        promoted = true;
    }
    Ok((live_before(tx, new_id)?, edge_changed || promoted, promoted))
}

/// Amend part of decision `old_id` with decision `new_id` (an `amends` edge). It draws the
/// `new_id → old_id` edge but, unlike supersede, **leaves `old_id` current**: the target is not
/// greyed out, and the two are read together. Amend confines itself to recording the revision link
/// and **does not move the new side's status either**: you can amend
/// while still `Proposed`. Accepting is left to a separate operation so that the new side is not
/// settled before a human has ruled on it. Edges are rows, so **one decision can
/// amend several old ones** (a DAG). Self-reference is rejected.
pub fn amend(tx: &WriteTx<'_>, new_id: i64, old_id: i64) -> Result<Decision> {
    if new_id == old_id {
        return Err(Error::invalid(
            "a decision cannot amend itself",
            "決定は自分自身を改訂できません",
        ));
    }
    // The target (the old side) must be alive.
    live_before(tx, old_id)?;
    let after = live_before(tx, new_id)?;
    put_edge(tx, new_id, old_id, DecisionEdgeKind::Amends)?;
    // Neither row is touched (accepting is a separate operation): one edge row, INSERTed.
    Ok(after)
}

/// Record that decision `new_id` **rests on** decision `old_id` (a `builds_on` edge). Unlike
/// supersede/amend, **neither row is touched** — the target stays current and is read no
/// differently — so this is one edge row, INSERTed. It buys exactly two things: a **reading order**
/// ("read that one first") and, read backwards, the **blast radius of overturning it** (the
/// decisions that need revisiting if this premise falls). Self-reference is rejected.
///
/// If a `supersedes` / `amends` edge already joins the pair, this **does nothing** (idempotent):
/// whoever corrects a decision is by definition standing on it, so builds_on is already implied,
/// and rewriting the edge would throw away the stronger behavior ("stop reading this" / "read both
/// together"). One kind per pair — `decision_edge_pair` is UNIQUE.
pub fn builds_on(tx: &WriteTx<'_>, new_id: i64, old_id: i64) -> Result<Decision> {
    if new_id == old_id {
        return Err(Error::invalid(
            "a decision cannot build on itself",
            "決定は自分自身を前提にできません",
        ));
    }
    // The premise (the old side) must be alive.
    live_before(tx, old_id)?;
    let after = live_before(tx, new_id)?;
    if let Some(id) = read::decision_edge_id(tx.conn(), new_id, old_id)? {
        let existing = read::decision_edge(tx.conn(), id)?
            .expect("the edge id was just read from the same transaction");
        if existing.kind != DecisionEdgeKind::BuildsOn {
            return Ok(after); // supersedes / amends already implies it
        }
    }
    put_edge(tx, new_id, old_id, DecisionEdgeKind::BuildsOn)?;
    Ok(after)
}

/// Drop one decision-to-decision edge (the same call for all three kinds). Name the pair in the
/// direction it was drawn, new → old, and the row is **hard-deleted** (`decision_edge_pair` is
/// UNIQUE, so a pair pins exactly one edge and the kind never has to be named). A noop (`false`) if
/// there is none. Overturning a decision is a change of mind that supersede records as history — but
/// an edge is not the decision, it is **wiring**, and a misspoken wire is corrected rather than kept
/// as history. Currency is a derived projection (is this
/// decision pointed at by a live `supersedes` edge?), so removing a `supersedes` edge **makes the
/// target current again on its own**. There is nothing to clean up afterwards.
pub fn unlink_edge(tx: &WriteTx<'_>, decision_id: i64, target_decision_id: i64) -> Result<bool> {
    let Some(id) = read::decision_edge_id(tx.conn(), decision_id, target_decision_id)? else {
        return Ok(false);
    };
    tx.delete_record("decision_edge", id)?;
    Ok(true)
}

/// Draw one decision-to-decision edge. The direction is always new → old. If a live edge already
/// joins the pair, **its kind is rewritten** in place (no second INSERT): `decision_edge_pair` is
/// UNIQUE, and it would be a contradiction for supersedes (target becomes history) and amends
/// (target stays current) to hold over the same pair at once. Redrawing the same kind is idempotent
/// (a noop). Rewriting builds_on into supersedes/amends is a **promotion** — a weak implication
/// gives way to a strong behavior — and goes straight through. Guarding against the reverse, a
/// demotion, is [`builds_on`]'s job: it does nothing when the edge already implies it.
///
/// Returns `changed`: `false` when the same-kind edge was already there (the idempotent noop), so a
/// caller like [`supersede`] can tell whether it actually drew anything.
fn put_edge(
    tx: &WriteTx<'_>,
    decision_id: i64,
    target_decision_id: i64,
    kind: DecisionEdgeKind,
) -> Result<bool> {
    // Every decision-to-decision edge funnels through here (supersede / amend / builds_on), so the
    // invariant is enforced at the confluence — put it at each call site instead and the next edge
    // kind someone adds will slip past.
    crate::ops::guard_same_project(
        Some(live_before(tx, decision_id)?.project_id),
        Some(live_before(tx, target_decision_id)?.project_id),
        "this decision edge",
        "この決定間エッジ",
    )?;
    let now = Timestamp::now();
    if let Some(id) = read::decision_edge_id(tx.conn(), decision_id, target_decision_id)? {
        let before = read::decision_edge(tx.conn(), id)?
            .expect("the edge id was just read from the same transaction");
        if before.kind == kind {
            return Ok(false); // idempotent: same-kind edge already present
        }
        // The intent column moves with the kind: a `builds_on` promoted to `supersedes` began superseding
        // here, not when it was first drawn, and dating the promotion by the original insert would put the
        // supersession before a reservation it in fact came after (`AMB-D-373`).
        let after = DecisionEdge { kind, drawn_at: Some(now), updated_at: now, ..before.clone() };
        emit_update(tx, record::decision_edge(&before), record::decision_edge(&after))?;
        return Ok(true);
    }
    let edge = DecisionEdge {
        id: read::next_id(tx.conn(), "decision_edge")?,
        decision_id,
        target_decision_id,
        kind,
        drawn_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::decision_edge(&edge))?;
    Ok(true)
}

/// Hard-delete a decision, `Accepted` ones included: deleting retires a record outright, a different
/// act from editing its body or superseding it — if all you want is to replace an accepted decision
/// while keeping it readable, use `supersede`.
pub fn delete(tx: &WriteTx<'_>, id: i64) -> Result<Vec<String>> {
    let before = live_before(tx, id)?;
    delete_subtree(tx, before.id)
}

/// Hard-delete one decision and its children (pass an id whose existence has already been checked).
/// This is the body of [`delete`], and [`crate::ops::project::delete`] also uses it to clear out a
/// project's decisions. The schema's `CASCADE` takes `decision_comment`, `decision_task_link` and
/// `decision_edge` along (both endpoints cascade, so the edges this decision drew and the edges
/// pointing at it go with it — nothing dangles). All the delete op has to sweep is the polymorphic
/// children: the decision's own attachments, plus the attachments hanging off the comments that
/// `CASCADE` removes. Returns the blob hashes this subtree let go of (candidates for collection
/// after commit).
pub(crate) fn delete_subtree(tx: &WriteTx<'_>, id: i64) -> Result<Vec<String>> {
    let mut orphaned = Vec::new();
    for comment_id in read::decision_comment_ids(tx.conn(), id)? {
        orphaned.extend(crate::ops::sweep_polymorphic(tx, "decision_comment", comment_id)?);
    }
    orphaned.extend(crate::ops::sweep_polymorphic(tx, "decision", id)?);
    tx.delete_record("decision", id)?;
    Ok(orphaned)
}

// ───────────────────────── Decision ⇄ task links ─────────────────────────

/// Link a decision to a task (an edge), so that the implementation tasks a decision spawned and the
/// decision that motivated a task are reachable from either side. Idempotent when a live link for
/// the same `(decision, task)` already exists (the existing row is returned). Returns
/// `(link, created)`.
pub fn link(
    tx: &WriteTx<'_>,
    decision_id: i64,
    task_id: i64,
) -> Result<(DecisionTaskLink, bool)> {
    let decision = live_before(tx, decision_id)?;
    let Some(task) = read::task(tx.conn(), task_id)? else {
        return Err(crate::ops::task::NOUN.not_found(task_id.to_string()));
    };
    // An implementation task a decision spawned lives in that decision's project. Inbox tasks
    // belong to no project at all, so they pass — and if moving one into a project would straddle
    // two, `task::move_to` is what stops it.
    crate::ops::guard_same_project(
        Some(decision.project_id),
        task.project_id,
        "this decision-task link",
        "この決定とタスクのリンク",
    )?;
    if let Some(id) = read::decision_task_link_id(tx.conn(), decision_id, task_id)? {
        let existing = read::decision_task_link(tx.conn(), id)?
            .expect("the link id was just read from the same transaction");
        return Ok((existing, false));
    }
    let now = Timestamp::now();
    let l = DecisionTaskLink {
        id: read::next_id(tx.conn(), "decision_task_link")?,
        decision_id,
        task_id,
        // The instant the link was drawn — the intent column the premise-change judgement reads
        // (`AMB-D-372`). Stamped here, once, and never rewritten.
        linked_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::decision_task_link(&l))?;
    Ok((l, true))
}

/// Unlink (delete the row). Does nothing when there is no such link (idempotent). Returns `changed`.
pub fn unlink(tx: &WriteTx<'_>, decision_id: i64, task_id: i64) -> Result<bool> {
    let Some(link_id) = read::decision_task_link_id(tx.conn(), decision_id, task_id)? else {
        return Ok(false);
    };
    tx.delete_record("decision_task_link", link_id)?;
    Ok(true)
}

/// Read a live decision's `before` snapshot **from this transaction**. not_found if it is gone.
fn live_before(tx: &WriteTx<'_>, id: i64) -> Result<Decision> {
    read::decision(tx.conn(), id)?.ok_or_else(|| NOUN.not_found(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, mk_task_in, new_engine};

    /// This decision's detail row (`None` when the row itself is gone).
    fn detail(tx: &WriteTx<'_>, id: i64) -> Option<read::DecisionDetailRow> {
        read::decision_detail(tx.conn(), id).unwrap()
    }

    /// The ids this decision points at with `kind`, in the order the edges were drawn — the forward
    /// read of the edge table.
    fn targets(tx: &WriteTx<'_>, decision_id: i64, kind: DecisionEdgeKind) -> Vec<i64> {
        let Some(d) = detail(tx, decision_id) else { return Vec::new() };
        let ids = |v: Vec<(i64, Option<String>)>| v.into_iter().map(|(id, _)| id).collect();
        match kind {
            DecisionEdgeKind::Supersedes => ids(d.edges.supersedes),
            DecisionEdgeKind::Amends => ids(d.edges.amends),
            DecisionEdgeKind::BuildsOn => d.edges.builds_on.into_iter().map(|p| p.id).collect(),
        }
    }

    /// How many edges touch this decision, in either direction.
    fn edge_count(tx: &WriteTx<'_>, decision_id: i64) -> usize {
        let Some(d) = detail(tx, decision_id) else { return 0 };
        let e = d.edges;
        e.supersedes.len()
            + e.amends.len()
            + e.builds_on.len()
            + e.superseded_by.len()
            + e.amended_by.len()
            + e.built_on_by.len()
    }

    /// Is this decision current — i.e. not pointed at by a live `supersedes` edge?
    fn is_current(tx: &WriteTx<'_>, decision_id: i64) -> bool {
        read::decision_card_row(tx.conn(), decision_id).unwrap().expect("live decision").current
    }

    /// How many live comments this decision has.
    fn num_comments(tx: &WriteTx<'_>, decision_id: i64) -> usize {
        read::decision_comment_list(tx.conn(), decision_id).unwrap().len()
    }

    /// The live task ids linked to this decision (live links to live tasks only).
    fn tasks_for_decision(tx: &WriteTx<'_>, decision_id: i64) -> Vec<i64> {
        detail(tx, decision_id)
            .map(|d| d.linked_tasks.into_iter().map(|t| t.id).collect())
            .unwrap_or_default()
    }

    /// The live decision ids that motivated this task.
    fn decisions_for_task(tx: &WriteTx<'_>, task_id: i64) -> Vec<i64> {
        read::decisions_for_task(tx.conn(), task_id).unwrap().into_iter().map(|d| d.id).collect()
    }

    fn new_decision(tx: &WriteTx<'_>, pid: i64, title: &str) -> Decision {
        add(
            tx,
            NewDecision {
                title: title.to_string(),
                body: "結論と根拠".to_string(),
                project_id: pid,
            },
        )
        .unwrap()
    }

    #[test]
    fn add_creates_a_proposed_decision() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "RDB を真実源にする");
        assert_eq!(d.status, DecisionStatus::Proposed);
        assert_eq!(d.project_id, pid);
        assert_eq!(d.id, 1, "the first decision is D-1 (a space separate from tasks)");
        assert!(d.decided_at.is_none());
        assert!(edge_count(tx, d.id) == 0);
        assert_eq!(crate::view::decision_display_ref(&d), "AMB-D-1");
    }

    #[test]
    fn add_comment_appends_a_live_decision_comment() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "RDB を真実源にする");
        let c = add_comment(tx, d.id, ActorKind::Ai, "この根拠に同意").unwrap();
        assert_eq!(c.decision_id, d.id);
        assert_eq!(c.author_kind, Some(ActorKind::Ai));
        assert_eq!(c.text, "この根拠に同意");
        assert_eq!(num_comments(tx, d.id), 1);
        // Nothing is ever written to task_comment: the tables are separate.
        let task_comments: i64 =
            tx.conn().query_row("SELECT count(*) FROM task_comment", [], |r| r.get(0)).unwrap();
        assert_eq!(task_comments, 0, "a decision comment never pollutes task_comment");
    }

    #[test]
    fn add_comment_rejects_empty_body() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "件名");
        assert!(add_comment(tx, d.id, ActorKind::Human, "   ").is_err());
        assert_eq!(num_comments(tx, d.id), 0);
    }

    #[test]
    fn add_comment_rejects_unknown_decision() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let _pid = mk_project(tx, "amenbo 開発");
        assert!(add_comment(tx, 9999, ActorKind::Ai, "x").is_err());
    }

    #[test]
    fn num_comments_counts_only_the_targeted_live_decision() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d1 = new_decision(tx, pid, "A");
        let d2 = new_decision(tx, pid, "B");
        add_comment(tx, d1.id, ActorKind::Ai, "1").unwrap();
        add_comment(tx, d1.id, ActorKind::Ai, "2").unwrap();
        add_comment(tx, d2.id, ActorKind::Ai, "3").unwrap();
        assert_eq!(num_comments(tx, d1.id), 2);
        assert_eq!(num_comments(tx, d2.id), 1);
    }

    #[test]
    fn remove_comment_takes_its_attachments_with_it() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "件名");
        let c = add_comment(tx, d.id, ActorKind::Ai, "誤投稿").unwrap();
        crate::ops::attachment::add_url(
            tx,
            crate::model::AttachmentTarget::DecisionComment,
            c.id,
            "https://example.com/",
            None,
            ActorKind::Ai,
        )
        .unwrap();

        assert!(remove_comment(tx, c.id).unwrap());
        assert_eq!(num_comments(tx, d.id), 0);
        assert!(read::decision_comment(tx.conn(), c.id).unwrap().is_none(), "the row goes away entirely (it is not a tombstone)");
        assert!(
            read::attachments_for_target(tx.conn(), "decision_comment", c.id).unwrap().is_empty(),
            "a polymorphic attachment is swept by the delete op (no FK can be drawn)"
        );
    }

    #[test]
    fn remove_comment_is_a_noop_when_it_is_gone() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        assert!(!remove_comment(tx, 9999).unwrap());
    }

    /// A comment can be fixed in place even under an accepted decision — accepting settles the
    /// decision, it does not lock the discussion held underneath it.
    #[test]
    fn edit_comment_works_even_under_an_accepted_decision() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "件名");
        let c = add_comment(tx, d.id, ActorKind::Ai, "誤字のある投稿").unwrap();
        accept(tx, d.id, None).unwrap();

        let edited = edit_comment(tx, c.id, "直した投稿").unwrap();
        assert_eq!(edited.id, c.id, "the id does not change (this is not a new post)");
        assert_eq!(read::decision_comment(tx.conn(), c.id).unwrap().unwrap().text, "直した投稿");
        assert!(edit_comment(tx, c.id, " ").is_err(), "an empty body is refused");
        assert!(edit_comment(tx, 9999, "x").is_err(), "editing a comment that is not there is not_found");
    }

    /// Create one numbered task and return its id.
    fn add_task_in(tx: &WriteTx<'_>, pid: i64, title: &str) -> i64 {
        mk_task_in(tx, title, Some(pid))
    }

    #[test]
    fn tasks_and_decisions_have_separate_number_spaces() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        // Tasks and decisions each start at 1 in their own space, so #1 and #1 coexist.
        let t1 = add_task_in(tx, pid, "t1");
        assert_eq!(t1, 1);
        assert_eq!(new_decision(tx, pid, "d1").id, 1);
        let t2 = add_task_in(tx, pid, "t2");
        assert_eq!(t2, 2);
        assert_eq!(new_decision(tx, pid, "d2").id, 2);
        let mut tnums = read::live_task_ids(tx.conn()).unwrap();
        let mut dnums: Vec<i64> =
            read::decision_list(tx.conn(), crate::reach::Reach::All, None)
                .unwrap()
                .into_iter()
                .map(|d| d.id)
                .collect();
        tnums.sort_unstable();
        dnums.sort_unstable();
        assert_eq!(tnums, vec![1_i64, 2]);
        assert_eq!(dnums, vec![1_i64, 2]);
    }

    #[test]
    fn resolve_ref_resolves_decisions_by_number_and_id() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let _t = add_task_in(tx, pid, "t1"); // task #1
        let d = new_decision(tx, pid, "d1"); // decision #1 (a separate space)
        // Look a decision up by `D-1`, `#1` or its bare id; numbers are globally unique, so no project
        // context is needed.
        assert_eq!(crate::query::resolve_decision_ref(tx.conn(), "D-1").unwrap(), d.id);
        assert_eq!(crate::query::resolve_decision_ref(tx.conn(), "#1").unwrap(), d.id);
        assert_eq!(crate::query::resolve_decision_ref(tx.conn(), &d.id.to_string()).unwrap(), d.id);
        assert!(crate::query::resolve_decision_ref(tx.conn(), "AMENBO-1").is_err());
        // `T-1` names a task explicitly, so it is not found as a decision.
        assert!(crate::query::resolve_decision_ref(tx.conn(), "T-1").is_err());
        // A number with no decision behind it is not_found.
        assert!(crate::query::resolve_decision_ref(tx.conn(), "D-2").is_err());
    }

    #[test]
    fn resolve_any_dispatches_by_type_prefix() {
        use crate::ops::Ref;
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let t = add_task_in(tx, pid, "t1"); // task #1
        let d = new_decision(tx, pid, "d1"); // decision #1 (a separate space)
        // The type prefix pins the type.
        assert_eq!(crate::query::resolve_any(tx.conn(), "T-1").unwrap(), Ref::Task(t));
        assert_eq!(crate::query::resolve_any(tx.conn(), "D-1").unwrap(), Ref::Decision(d.id));
        // Bare #1 exists in both spaces, so it is ambiguous and we ask for a prefix.
        assert!(crate::query::resolve_any(tx.conn(), "#1").is_err());
    }


    #[test]
    fn link_is_bidirectional_idempotent_and_unlinkable() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "RDB を真実源");
        let t1 = add_task_in(tx, pid, "task1");
        let t2 = add_task_in(tx, pid, "task2");
        // One decision → many tasks (many-to-many).
        assert!(link(tx, d.id, t1).unwrap().1, "a new link");
        assert!(link(tx, d.id, t2).unwrap().1);
        // Idempotent: the same link is never created twice.
        assert!(!link(tx, d.id, t1).unwrap().1, "an existing link is idempotent");
        // Reachable from both sides.
        let mut tasks = tasks_for_decision(tx, d.id);
        tasks.sort();
        let mut want = vec![t1, t2];
        want.sort();
        assert_eq!(tasks, want, "decision → tasks");
        assert_eq!(decisions_for_task(tx, t1), vec![d.id], "task → the decision that motivated it");
        // Unlinking drops it from both directions (idempotent).
        assert!(unlink(tx, d.id, t1).unwrap());
        assert!(!unlink(tx, d.id, t1).unwrap(), "the second time is a noop");
        assert_eq!(tasks_for_decision(tx, d.id), vec![t2]);
        assert!(decisions_for_task(tx, t1).is_empty());
    }

    #[test]
    fn link_rejects_missing_decision_or_task() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "決定");
        let t = add_task_in(tx, pid, "task");
        assert!(link(tx, 9999, t).is_err());
        assert!(link(tx, d.id, 9999).is_err(), "linking to a task that does not exist is refused");
    }

    #[test]
    fn queries_ignore_deleted_endpoints() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "決定");
        let t = add_task_in(tx, pid, "task");
        link(tx, d.id, t).unwrap();
        // Delete the task and it drops out of the decision → task view: the endpoint is gone.
        crate::ops::task::delete(tx, t).unwrap();
        assert!(tasks_for_decision(tx, d.id).is_empty(), "a dead task is not returned");
        // Delete the decision and it drops out of the task → decision view as well.
        let t2 = add_task_in(tx, pid, "task2");
        link(tx, d.id, t2).unwrap();
        delete(tx, d.id).unwrap();
        assert!(decisions_for_task(tx, t2).is_empty(), "a dead decision is not returned");
    }

    /// Decision ⇄ task links are **queryable from both sides** (`decision list --filter task:` /
    /// `task list --filter decision:`). Tasks and decisions number in separate spaces, so a prefix
    /// from the wrong side is an error rather than an empty result — a silent zero would read as
    /// "there are no links".
    #[test]
    fn decision_list_filters_by_the_task_a_decision_rests_on() {
        use crate::query::{decision_list, DecisionFilter, DecisionListParams, Filter};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "リンクを辿れる面を作る");
        let _other = new_decision(tx, pid, "無関係な決定");
        let t = add_task_in(tx, pid, "リンクされたタスク");
        let t_unlinked = add_task_in(tx, pid, "リンクを外したタスク");
        link(tx, d.id, t).unwrap();
        link(tx, d.id, t_unlinked).unwrap();
        unlink(tx, d.id, t_unlinked).unwrap();

        let list = |expr: &str| {
            decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
                project_id: Some(pid),
                filter_expr: Some(expr.to_string()),
                sort: "created".to_string(),
                ..Default::default()
            })
            .unwrap()
        };
        let r = list(&format!("task:{t}"));
        assert_eq!(r.decisions.iter().map(|d| d.id).collect::<Vec<_>>(), vec![d.id]);
        // An unlinked pair is gone, row and all, so it cannot be read back.
        assert_eq!(list(&format!("task:{t_unlinked}")).count, 0);
        assert_eq!(list("task:999").count, 0);

        // A prefix from the wrong side is an error (`task:D-1` / `decision:T-1`).
        assert!(DecisionFilter::parse(&format!("task:D-{}", d.id), crate::time::today()).is_err());
        assert!(Filter::parse(&format!("decision:T-{t}"), crate::time::today()).is_err());
    }

    /// `project:` takes an id or a name (the same intake as on the task side). A reference that does
    /// not resolve is an error.
    #[test]
    fn decision_list_project_filter_takes_a_name_and_refuses_an_unknown_one() {
        use crate::query::{decision_list, DecisionListParams};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "アルファ");
        let other = mk_project(tx, "ベータ");
        let d = new_decision(tx, pid, "アルファの決定");
        let _ = new_decision(tx, other, "ベータの決定");

        let list = |expr: &str| {
            decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
                project_id: None,
                filter_expr: Some(expr.to_string()),
                sort: "created".to_string(),
                ..Default::default()
            })
        };
        assert_eq!(list(&format!("project:{pid}")).unwrap().decisions.iter().map(|x| x.id).collect::<Vec<_>>(), vec![d.id]);
        assert_eq!(list("project:アルファ").unwrap().decisions.iter().map(|x| x.id).collect::<Vec<_>>(), vec![d.id], "the name gives the same single row");
        let err = list("project:存在しないPJ").unwrap_err();
        assert!(err.to_string().contains("存在しないPJ"), "a reference that does not resolve is an error: {err}");
    }

    #[test]
    fn decision_list_filters_by_status_text_and_sorts() {
        use crate::query::{decision_list, DecisionListParams};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d1 = add(tx, NewDecision {
            title: "RDB を真実源にする".to_string(),
            body: "engine+HLC で同期".to_string(),
            project_id: pid,
        }).unwrap();
        accept(tx, d1.id, None).unwrap();
        let _d2 = add(tx, NewDecision {
            title: "OSS は英語表記".to_string(),
            body: "README とコミットは英語".to_string(),
            project_id: pid,
        }).unwrap(); // still Proposed

        // status:accepted matches d1 alone.
        let r = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: Some("status:accepted".to_string()),
            sort: "-created".to_string(),
            ..Default::default()
        }).unwrap();
        assert_eq!(r.count, 1);
        assert_eq!(r.decisions[0].id, d1.id);

        // `text:` searches the body as well as the title (this term appears only in d2's body).
        let r = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: Some("text:英語".to_string()),
            sort: "-created".to_string(),
            ..Default::default()
        }).unwrap();
        assert_eq!(r.count, 1, "a body match hits");

        // With no filter, both rows come back, and the compact form carries the ref/number.
        let r = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: None,
            sort: "number".to_string(),
            ..Default::default()
        }).unwrap();
        assert_eq!(r.count, 2);
        assert_eq!(r.decisions[0].r#ref, "AMB-D-1");
    }

    #[test]
    fn decision_list_text_reaches_comment_bodies() {
        use crate::query::{decision_list, DecisionListParams};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        // The term appears in neither title nor body — only in a comment. Before the comment arm this
        // matched nothing; now it hits, mirroring the task side's `text:` over comment bodies.
        let d = add(tx, NewDecision {
            title: "RDB を真実源にする".to_string(),
            body: "engine+HLC で同期".to_string(),
            project_id: pid,
        }).unwrap();
        add_comment(tx, d.id, ActorKind::Ai, "計測してから設計する方針で合意").unwrap();
        // A second decision with the term nowhere, to prove the filter still narrows.
        add(tx, NewDecision {
            title: "OSS は英語表記".to_string(),
            body: "README とコミットは英語".to_string(),
            project_id: pid,
        }).unwrap();

        let r = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: Some("text:計測".to_string()),
            sort: "-created".to_string(),
            ..Default::default()
        }).unwrap();
        assert_eq!(r.count, 1, "a comment-body match hits");
        assert_eq!(r.decisions[0].id, d.id);

        // Case-insensitive, same as the title/body arms.
        let r = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: Some("text:合意".to_string()),
            sort: "-created".to_string(),
            ..Default::default()
        }).unwrap();
        assert_eq!(r.count, 1, "a later term in the same comment also hits");
    }

    /// The structural `text` term (`DecisionListParams::text`) is the same search as the grammar's `text:`,
    /// for the callers that cannot spell one — a search box hands over a phrase, and the grammar splits on
    /// whitespace, so an expression would silently drop everything after the first word. Given both, the
    /// structural one is the term.
    #[test]
    fn the_structural_text_term_carries_a_phrase_the_grammar_cannot() {
        use crate::query::{decision_list, DecisionListParams};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = add(tx, NewDecision {
            title: "the store is the truth".to_string(),
            body: String::new(),
            project_id: pid,
        }).unwrap();
        add_comment(tx, d.id, ActorKind::Ai, "measured first, designed after").unwrap();
        add(tx, NewDecision {
            title: "commits are English".to_string(),
            body: String::new(),
            project_id: pid,
        }).unwrap();

        let list = |params: DecisionListParams| {
            decision_list(tx.conn(), crate::reach::Reach::All, params).unwrap()
        };

        // A phrase with a space, matched against a comment body — the whole point of the structural term.
        let r = list(DecisionListParams {
            project_id: Some(pid),
            text: Some("measured first".to_string()),
            sort: "-created".to_string(),
            ..Default::default()
        });
        assert_eq!(r.count, 1, "the phrase reaches the comment arm whole");
        assert_eq!(r.decisions[0].id, d.id);

        // The same phrase through the grammar cannot be said at all: whitespace ends the value.
        let via_grammar = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: Some("text:measured first".to_string()),
            sort: "-created".to_string(),
            ..Default::default()
        });
        assert!(via_grammar.is_err(), "the grammar refuses the bare word rather than searching for it");

        // Given both, the structural one is the term.
        let r = list(DecisionListParams {
            project_id: Some(pid),
            filter_expr: Some("text:English".to_string()),
            text: Some("measured".to_string()),
            sort: "-created".to_string(),
            ..Default::default()
        });
        assert_eq!(r.count, 1);
        assert_eq!(r.decisions[0].id, d.id, "the structural term won, not the expression's");
    }

    #[test]
    fn decision_list_with_body_carries_body_only_when_requested() {
        use crate::query::{decision_list, DecisionListParams};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        add(tx, NewDecision {
            title: "RDB を真実源にする".to_string(),
            body: "engine+HLC で同期".to_string(),
            project_id: pid,
        })
        .unwrap();

        // The body is left out by default, keeping the list light.
        let r = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            sort: "-created".to_string(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.count, 1);
        assert_eq!(r.decisions[0].body, None, "without with_body the body is left out");

        // `--with-body` adds the body column to an already-narrowed result — a bounded read, e.g.
        // to spot contradictions in substance.
        let r = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            sort: "-created".to_string(),
            with_body: true,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(r.decisions[0].body.as_deref(), Some("engine+HLC で同期"));
    }

    #[test]
    fn decision_list_does_not_return_a_deleted_decision() {
        use crate::query::{decision_list, DecisionListParams};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "消す決定");
        delete(tx, d.id).unwrap();
        let r = decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: None,
            sort: "-created".to_string(),
            ..Default::default()
        }).unwrap();
        assert_eq!(r.count, 0, "a deleted decision is not returned");
    }


    #[test]
    fn add_rejects_empty_title_and_missing_project() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        assert!(add(tx, NewDecision {
            title: "  ".to_string(),
            body: String::new(),
            project_id: pid,
        }).is_err());
        assert!(add(tx, NewDecision {
            title: "x".to_string(),
            body: String::new(),
            project_id: 999_999,
        }).is_err());
    }

    #[test]
    fn update_edits_proposed_and_accepted_but_not_rejected() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "old title");
        // While proposed, edit freely.
        let edited = update(tx, d.id, DecisionPatch {
            title: Some("new title".to_string()),
            body: Some("詳しい根拠".to_string()),
        })
        .unwrap();
        assert_eq!(edited.title, "new title");
        assert_eq!(edited.body, "詳しい根拠");
        // Accepted edits in place too (`AMB-D-363`), and editing does not re-decide: the decided_* stamps stand.
        let (accepted, _) = accept(tx, d.id, Some("user-1".to_string())).unwrap();
        let decided_at = accepted.decided_at;
        let reedited = update(tx, d.id, DecisionPatch { body: Some("採択後に直した本文".to_string()), ..Default::default() }).unwrap();
        assert_eq!(reedited.status, DecisionStatus::Accepted, "editing an accepted decision does not un-settle it");
        assert_eq!(reedited.body, "採択後に直した本文");
        assert_eq!(reedited.decided_by.as_deref(), Some("user-1"), "edit leaves decided_by untouched");
        assert_eq!(reedited.decided_at, decided_at, "edit leaves decided_at untouched");
        // A rejected decision is terminal: it cannot be edited.
        let r = new_decision(tx, pid, "却下する案");
        reject(tx, r.id).unwrap();
        assert!(update(tx, r.id, DecisionPatch { body: Some("x".to_string()), ..Default::default() }).is_err());
    }

    #[test]
    fn accept_sets_decided_fields_and_is_idempotent() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "決定");
        let (a, changed) = accept(tx, d.id, Some("user-1".to_string())).unwrap();
        assert!(changed, "a fresh acceptance reports changed");
        assert_eq!(a.status, DecisionStatus::Accepted);
        assert!(a.decided_at.is_some());
        assert_eq!(a.decided_by.as_deref(), Some("user-1"));
        // Idempotent: re-accepting is a noop, reports unchanged, and does not overwrite decided_by.
        let (a2, changed2) = accept(tx, d.id, Some("user-2".to_string())).unwrap();
        assert!(!changed2, "re-accepting an already-accepted decision reports unchanged");
        assert_eq!(a2.decided_by.as_deref(), Some("user-1"));
    }

    #[test]
    fn reject_only_from_proposed() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "却下する案");
        let (r, changed) = reject(tx, d.id).unwrap();
        assert!(changed, "a fresh rejection reports changed");
        assert_eq!(r.status, DecisionStatus::Rejected);
        // Idempotent: re-rejecting reports unchanged.
        let (r2, changed2) = reject(tx, d.id).unwrap();
        assert!(!changed2, "re-rejecting an already-rejected decision reports unchanged");
        assert_eq!(r2.status, DecisionStatus::Rejected);
        // An accepted decision cannot be rejected.
        let d2 = new_decision(tx, pid, "採択済み");
        accept(tx, d2.id, None).unwrap();
        assert!(reject(tx, d2.id).is_err());
    }

    #[test]
    fn reopen_un_settles_an_accepted_decision_and_is_auditable() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "早すぎた採択を議論へ戻す決定");
        accept(tx, d.id, Some("user-1".to_string())).unwrap();
        // reopen un-settles it back to discussion, clearing decided_* — this is its whole job now
        // (editing does not need it: an accepted decision edits in place, see the update test).
        let (re, changed) = reopen(tx, d.id).unwrap();
        assert!(changed, "an accepted decision really reopens");
        assert_eq!(re.status, DecisionStatus::Proposed);
        assert!(re.decided_at.is_none(), "decided_at is cleared");
        assert!(re.decided_by.is_none(), "decided_by is cleared");
        // Re-accepting settles it again — a real transition.
        let (reaccepted, changed) = accept(tx, d.id, Some("user-1".to_string())).unwrap();
        assert!(changed, "accepting again after a reopen is a real transition");
        assert_eq!(reaccepted.status, DecisionStatus::Accepted);
        assert!(reaccepted.decided_at.is_some());
    }

    /// The status clock moves on a status transition and on nothing else. Instants are recorded to the
    /// second, so a fresh stamp cannot be told apart from the one it replaced by comparing the two — plant
    /// a distinctly old instant instead, and watch which operations displace it.
    #[test]
    fn status_changed_at_moves_on_transitions_only() {
        use rusqlite::types::Value;
        const PLANTED: &str = "2020-01-01T00:00:00Z";

        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let clock =
            |id: i64| read::decision(tx.conn(), id).unwrap().expect("live decision").status_changed_at;
        let plant = |id: i64| {
            tx.set_field("decision", id, "status_changed_at", Value::Text(PLANTED.to_string())).unwrap();
        };
        let planted = Timestamp::parse_rfc3339(PLANTED);
        assert!(planted.is_some(), "the planted instant is one the column admits");

        let d = new_decision(tx, pid, "決定");
        assert!(clock(d.id).is_some(), "proposing is the first transition, so it starts the clock");

        // An edit is not a status change: `update` moves the body and `updated_at`, and leaves this alone.
        plant(d.id);
        update(tx, d.id, DecisionPatch { body: Some("直した根拠".to_string()), ..Default::default() })
            .unwrap();
        assert_eq!(clock(d.id), planted, "editing the body leaves the status clock where it was");

        accept(tx, d.id, None).unwrap();
        assert_ne!(clock(d.id), planted, "accepting stamps the clock");

        // The idempotent re-accept never reaches the write, so it cannot re-stamp.
        plant(d.id);
        accept(tx, d.id, Some("user-2".to_string())).unwrap();
        assert_eq!(clock(d.id), planted, "re-accepting an accepted decision leaves the clock alone");

        // Reopen is the transition the whole axis is built on (`AMB-D-373`).
        reopen(tx, d.id).unwrap();
        assert_ne!(clock(d.id), planted, "reopening stamps the clock");

        // Superseding promotes the *new* side (a transition) and never rewrites the old row — being
        // superseded is an edge, not a status, so the old side's clock stands.
        let old = new_decision(tx, pid, "旧: 置き換えられる");
        accept(tx, old.id, None).unwrap();
        let newer = new_decision(tx, pid, "新: 置き換える");
        plant(old.id);
        plant(newer.id);
        supersede(tx, newer.id, old.id, None).unwrap();
        assert_ne!(clock(newer.id), planted, "the promotion a supersede performs stamps the new side");
        assert_eq!(clock(old.id), planted, "the superseded side's status did not change, nor its clock");
    }

    #[test]
    fn reopen_is_idempotent_on_proposed_and_rejects_terminal_states() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        // Proposed is idempotent: it is already un-settled.
        let d = new_decision(tx, pid, "議論中の決定");
        let (again, changed) = reopen(tx, d.id).unwrap();
        assert_eq!(again.status, DecisionStatus::Proposed);
        assert!(!changed, "reopening a proposed decision changes nothing");
        // Rejected cannot be reopened: reject has no inverse.
        let r = new_decision(tx, pid, "却下した決定");
        reject(tx, r.id).unwrap();
        assert!(reopen(tx, r.id).is_err(), "Rejected cannot be reopened");
        // A superseded decision stays accepted (being superseded is a derived projection, not a status), so reopen works.
        let old = new_decision(tx, pid, "旧: 置き換えられる");
        accept(tx, old.id, None).unwrap();
        let newer = new_decision(tx, pid, "新: 置き換える");
        supersede(tx, newer.id, old.id, None).unwrap();
        assert_eq!(reopen(tx, old.id).unwrap().0.status, DecisionStatus::Proposed);
    }

    #[test]
    fn supersede_draws_the_edge_and_leaves_the_old_row_alone() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let old = new_decision(tx, pid, "v1: engine を真実源");
        accept(tx, old.id, None).unwrap();
        let new = new_decision(tx, pid, "v2: RDB を真実源");
        // new (Proposed) supersedes old, and is promoted to Accepted on the way.
        let (res, changed, promoted) = supersede(tx, new.id, old.id, Some("user-1".to_string())).unwrap();
        assert!(changed, "drawing the edge and promoting the new side is a real change");
        assert!(promoted, "the new side moved Proposed → Accepted");
        assert_eq!(res.status, DecisionStatus::Accepted);
        assert_eq!(targets(tx, new.id, DecisionEdgeKind::Supersedes), vec![old.id]);
        assert!(res.decided_at.is_some());
        // Re-running it changes nothing: the edge is already there and the new side already settled.
        let (_, changed2, promoted2) = supersede(tx, new.id, old.id, Some("user-2".to_string())).unwrap();
        assert!(!changed2, "re-superseding an already-superseded pair reports unchanged");
        assert!(!promoted2, "re-superseding promotes nothing — the new side was already accepted");
        // The old row is untouched: its status stays accepted, and being superseded shows up in the
        // derived currency instead.
        assert_eq!(read::decision(tx.conn(), old.id).unwrap().unwrap().status, DecisionStatus::Accepted, "the old side's status is unchanged");
        assert!(!is_current(tx, old.id), "superseded means not current (derived from the edge)");
        assert!(is_current(tx, new.id), "the superseding side is current");
        // Self-reference is rejected.
        assert!(supersede(tx, new.id, new.id, None).is_err());
    }

    /// Wiring drawn by mistake can be undrawn: the edge alone is dropped, both decisions stay, and
    /// because currency is a derived projection the target **becomes current again on its own**. A
    /// pair pins one edge (`decision_edge_pair` UNIQUE), so the kind need not be named — any of the
    /// three comes off with the same single call. Unlinking a pair that has no edge is a noop.
    #[test]
    fn an_edge_drawn_by_mistake_can_be_undrawn_and_the_target_becomes_current_again() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let old = new_decision(tx, pid, "旧");
        accept(tx, old.id, None).unwrap();
        let new = new_decision(tx, pid, "誤って覆した決定");
        supersede(tx, new.id, old.id, None).unwrap();
        assert!(!is_current(tx, old.id), "precondition: it has been superseded");

        assert!(unlink_edge(tx, new.id, old.id).unwrap(), "the edge came off");
        assert!(is_current(tx, old.id), "with supersedes gone the target becomes current again (no cleanup needed)");
        assert!(read::decision(tx.conn(), new.id).unwrap().is_some(), "the decision itself stays — only the wiring came off");
        assert!(targets(tx, new.id, DecisionEdgeKind::Supersedes).is_empty());

        assert!(!unlink_edge(tx, new.id, old.id).unwrap(), "unlinking a pair that is already gone is a noop");

        // builds_on comes off with that same single call — no kind to name.
        builds_on(tx, new.id, old.id).unwrap();
        assert_eq!(targets(tx, new.id, DecisionEdgeKind::BuildsOn), vec![old.id]);
        assert!(unlink_edge(tx, new.id, old.id).unwrap());
        assert!(targets(tx, new.id, DecisionEdgeKind::BuildsOn).is_empty());
    }

    /// Currency is derived from the edges, so retiring the superseding decision folds the edge away
    /// and the decision it had superseded **becomes current again on its own** — the recovery route
    /// for an erroneous supersede.
    #[test]
    fn currency_follows_the_edge_so_retiring_the_successor_restores_the_target() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let old = new_decision(tx, pid, "旧");
        accept(tx, old.id, None).unwrap();
        let new = new_decision(tx, pid, "誤って覆した決定");
        supersede(tx, new.id, old.id, None).unwrap();
        assert!(!is_current(tx, old.id));

        delete(tx, new.id).unwrap();
        assert!(is_current(tx, old.id), "retiring the superseding side restores currency (no orphaned superseded is left behind)");
        assert_eq!(read::decision(tx.conn(), old.id).unwrap().unwrap().status, DecisionStatus::Accepted);
    }

    /// A superseded decision is queryable through the edges (`current:`).
    #[test]
    fn decision_list_filters_by_current() {
        use crate::query::{decision_list, DecisionListParams};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let old = new_decision(tx, pid, "覆される決定");
        accept(tx, old.id, None).unwrap();
        let new = new_decision(tx, pid, "覆す決定");
        supersede(tx, new.id, old.id, None).unwrap();

        let list = |filter: &str| -> Vec<i64> {
            decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
                project_id: Some(pid),
                filter_expr: Some(filter.to_string()),
                sort: "number".to_string(),
                ..Default::default()
            })
            .unwrap()
            .decisions
            .into_iter()
            .map(|d| d.id)
            .collect()
        };
        assert_eq!(list("current:no"), vec![old.id], "the superseded decision");
        assert_eq!(list("current:yes"), vec![new.id], "the current decision");
        // status has only three values, so a superseded decision still comes back as accepted.
        assert_eq!(list("status:accepted").len(), 2);
        // `status:superseded` is refused: being superseded is not a status.
        assert!(decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: Some("status:superseded".to_string()),
            sort: "number".to_string(),
            ..Default::default()
        })
        .is_err());
    }

    /// Filtering by the day a decision was accepted is an ordinary `decision list` filter term;
    /// there is no dedicated as-of mode. "The policy that was settled by T" comes from composing it
    /// with `current:`, so this pins down that the two compose.
    #[test]
    fn decision_list_filters_by_the_day_it_was_decided() {
        use crate::query::{decision_list, DecisionListParams};
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let settled = new_decision(tx, pid, "今日採択した決定");
        accept(tx, settled.id, None).unwrap();
        let open = new_decision(tx, pid, "まだ採択していない決定");

        let list = |filter: &str| -> Vec<i64> {
            decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
                project_id: Some(pid),
                filter_expr: Some(filter.to_string()),
                sort: "number".to_string(),
                ..Default::default()
            })
            .unwrap()
            .decisions
            .into_iter()
            .map(|d| d.id)
            .collect()
        };
        // The named day is inclusive: something accepted today falls in both "up to today" and
        // "from today on".
        assert_eq!(list("decided_before:today"), vec![settled.id]);
        assert_eq!(list("decided_after:today"), vec![settled.id]);
        // Outside the window it drops out.
        assert!(list("decided_before:-1d").is_empty(), "it was not decided by yesterday");
        assert!(list("decided_after:+1d").is_empty(), "nor was it decided tomorrow or later");
        // An unaccepted decision has no such day, so it matches neither direction — though it is
        // still queryable as proposed.
        assert!(!list("decided_before:+1d").contains(&open.id));
        assert!(!list("decided_after:-1d").contains(&open.id));
        assert_eq!(list("status:proposed"), vec![open.id]);
        // Compose with the currency filter to get the policy that was live as of T.
        assert_eq!(list("decided_before:today current:yes"), vec![settled.id]);
        // A range, with both ends inclusive.
        assert_eq!(list("decided_after:-1d decided_before:+1d"), vec![settled.id]);
        // A value that is not a date is refused.
        assert!(decision_list(tx.conn(), crate::reach::Reach::All, DecisionListParams {
            project_id: Some(pid),
            filter_expr: Some("decided_before:soon".to_string()),
            sort: "number".to_string(),
            ..Default::default()
        })
        .is_err());
    }

    /// One decision can draw edges to **several** old ones: supersede and amend alike take as many
    /// edges of a kind as you draw.
    #[test]
    fn a_decision_can_supersede_and_amend_many_others() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let a = new_decision(tx, pid, "旧 A");
        let b = new_decision(tx, pid, "旧 B");
        let c = new_decision(tx, pid, "旧 C");
        for d in [&a, &b, &c] {
            accept(tx, d.id, None).unwrap();
        }
        let newer = new_decision(tx, pid, "新: A と B を置き換え C を改訂");
        supersede(tx, newer.id, a.id, None).unwrap();
        supersede(tx, newer.id, b.id, None).unwrap();
        amend(tx, newer.id, c.id).unwrap();

        assert_eq!(
            targets(tx, newer.id, DecisionEdgeKind::Supersedes),
            vec![a.id, b.id],
            "two supersedes edges stand side by side (the later one does not erase the earlier)"
        );
        assert_eq!(targets(tx, newer.id, DecisionEdgeKind::Amends), vec![c.id]);
        // Only the two superseded decisions stop being current; the amended one stays current.
        assert!(!is_current(tx, a.id));
        assert!(!is_current(tx, b.id));
        assert!(is_current(tx, c.id), "amend does not historicize its target");
        assert_eq!(read::decision(tx.conn(), c.id).unwrap().unwrap().status, DecisionStatus::Accepted);
    }

    /// A pair never carries two kinds (`decision_edge_pair` UNIQUE): the kind drawn later
    /// **rewrites** the earlier one.
    #[test]
    fn a_pair_carries_one_kind_and_redrawing_rewrites_it() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let old = new_decision(tx, pid, "旧");
        accept(tx, old.id, None).unwrap();
        let new = new_decision(tx, pid, "新");

        amend(tx, new.id, old.id).unwrap();
        assert_eq!(targets(tx, new.id, DecisionEdgeKind::Amends), vec![old.id]);
        // Redraw the same pair as supersede and no amends is left behind: the contradiction is
        // structurally impossible.
        supersede(tx, new.id, old.id, None).unwrap();
        assert_eq!(targets(tx, new.id, DecisionEdgeKind::Supersedes), vec![old.id]);
        assert!(targets(tx, new.id, DecisionEdgeKind::Amends).is_empty(), "the kind is rewritten");
        assert_eq!(edge_count(tx, new.id), 1, "still a single edge");
        // Redrawing the same kind is idempotent.
        supersede(tx, new.id, old.id, None).unwrap();
        assert_eq!(edge_count(tx, new.id), 1);
    }

    /// builds_on says "read that one first" — it names a premise. It moves neither decision's row
    /// and draws nothing but a single edge.
    #[test]
    fn builds_on_links_the_premise_without_touching_either_side() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let premise = new_decision(tx, pid, "同期基盤を撤去");
        accept(tx, premise.id, None).unwrap();
        let standing = new_decision(tx, pid, "その上に立つ決定");

        let res = builds_on(tx, standing.id, premise.id).unwrap();
        assert_eq!(targets(tx, standing.id, DecisionEdgeKind::BuildsOn), vec![premise.id]);
        assert_eq!(res.status, DecisionStatus::Proposed, "the drawing side's status is not moved");
        assert_eq!(read::decision(tx.conn(), premise.id).unwrap().unwrap().status, DecisionStatus::Accepted);
        assert!(is_current(tx, premise.id), "the premise stays current (it is not greyed out)");
        // Redrawing the same premise adds nothing (idempotent). Self-reference is rejected.
        builds_on(tx, standing.id, premise.id).unwrap();
        assert_eq!(edge_count(tx, standing.id), 1);
        assert!(builds_on(tx, standing.id, standing.id).is_err());
    }

    /// supersedes / amends **imply** builds_on — correcting a decision means standing on it — so
    /// drawing builds_on over the same pair does nothing: rewriting would throw away the stronger
    /// behavior ("stop reading this" / "read both together"). The other direction
    /// (builds_on → supersedes) is a promotion, and goes through.
    #[test]
    fn builds_on_never_downgrades_a_supersedes_or_amends_edge() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let old = new_decision(tx, pid, "旧");
        accept(tx, old.id, None).unwrap();
        let superseding = new_decision(tx, pid, "覆す側");
        let amending = new_decision(tx, pid, "改訂する側");
        supersede(tx, superseding.id, old.id, None).unwrap();
        amend(tx, amending.id, old.id).unwrap();

        builds_on(tx, superseding.id, old.id).unwrap();
        builds_on(tx, amending.id, old.id).unwrap();
        assert_eq!(
            targets(tx, superseding.id, DecisionEdgeKind::Supersedes),
            vec![old.id],
            "supersedes is not downgraded to builds_on"
        );
        assert_eq!(targets(tx, amending.id, DecisionEdgeKind::Amends), vec![old.id]);
        assert!(targets(tx, superseding.id, DecisionEdgeKind::BuildsOn).is_empty());
        assert!(targets(tx, amending.id, DecisionEdgeKind::BuildsOn).is_empty());

        // Drawn as a premise, then overturned after all: that is a promotion (weak implication →
        // strong behavior).
        let later = new_decision(tx, pid, "後から覆す側");
        builds_on(tx, later.id, old.id).unwrap();
        supersede(tx, later.id, old.id, None).unwrap();
        assert_eq!(targets(tx, later.id, DecisionEdgeKind::Supersedes), vec![old.id]);
        assert!(targets(tx, later.id, DecisionEdgeKind::BuildsOn).is_empty());
        assert_eq!(edge_count(tx, later.id), 1, "still a single edge");
    }

    /// Retiring a decision folds away both the edges it drew and the edges pointing at it, so
    /// nothing dangles.
    #[test]
    fn delete_folds_away_the_edges_on_both_sides() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let old = new_decision(tx, pid, "旧");
        accept(tx, old.id, None).unwrap();
        let new = new_decision(tx, pid, "新");
        supersede(tx, new.id, old.id, None).unwrap();

        delete(tx, new.id).unwrap();
        assert!(edge_count(tx, new.id) == 0, "retiring the drawing side folds its edges away");
        assert!(edge_count(tx, old.id) == 0, "it goes away from the side that was pointed at as well");
    }

    #[test]
    fn amend_links_forward_but_keeps_the_target_current() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let old = new_decision(tx, pid, "軸を統一");
        accept(tx, old.id, None).unwrap();
        let new = new_decision(tx, pid, "タグ行だけ改訂");
        amend(tx, new.id, old.id).unwrap();
        assert_eq!(targets(tx, new.id, DecisionEdgeKind::Amends), vec![old.id]);
        assert!(
            targets(tx, new.id, DecisionEdgeKind::Supersedes).is_empty(),
            "amend does not supersede"
        );
        // The target's row is left alone: its status stays Accepted. Amend's real difference from
        // supersede is currency, not status, and that is pinned in
        // `a_decision_can_supersede_and_amend_many_others`.
        assert_eq!(
            read::decision(tx.conn(), old.id).unwrap().unwrap().status,
            DecisionStatus::Accepted,
            "the amended target stays Accepted (it is not made Superseded)"
        );
        // Self-reference is rejected.
        assert!(amend(tx, new.id, new.id).is_err());
    }

    #[test]
    fn amend_does_not_promote_the_amending_side_out_of_proposed() {
        // amend records the revision link and nothing more: it must not move the new side's status.
        // Flipping it to Accepted before a human explicitly accepts would freeze the body, which
        // does real damage.
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let target = new_decision(tx, pid, "通信ゼロ");
        accept(tx, target.id, None).unwrap();
        let amending = new_decision(tx, pid, "更新チェックを解禁");
        assert_eq!(amending.status, DecisionStatus::Proposed);

        let res = amend(tx, amending.id, target.id).unwrap();
        assert_eq!(
            res.status,
            DecisionStatus::Proposed,
            "amending leaves the new side Proposed (no side-effect accept)"
        );
        assert!(res.decided_at.is_none(), "decided_at is not set");
        assert!(res.decided_by.is_none(), "decided_by is not set");
        assert_eq!(targets(tx, amending.id, DecisionEdgeKind::Amends), vec![target.id]);
        // accept still works as a separate operation, and leaves the edge as it is.
        let (accepted, _) = accept(tx, amending.id, None).unwrap();
        assert_eq!(accepted.status, DecisionStatus::Accepted);
        assert_eq!(targets(tx, amending.id, DecisionEdgeKind::Amends), vec![target.id]);
    }

    #[test]
    fn delete_removes_the_row_and_resolve_stops_finding_it() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        let d = new_decision(tx, pid, "消す決定");
        assert_eq!(crate::query::resolve_decision_ref(tx.conn(), &d.id.to_string()).unwrap(), d.id);
        delete(tx, d.id).unwrap();
        assert!(read::decision(tx.conn(), d.id).unwrap().is_none(), "the row goes away entirely");
        assert!(crate::query::resolve_decision_ref(tx.conn(), &d.id.to_string()).is_err(), "a deleted decision no longer resolves");
    }

    #[test]
    fn delete_works_on_accepted_and_takes_its_links() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pid = mk_project(tx, "amenbo 開発");
        // Even an accepted decision can be deleted: retiring a record outright is a different act from
        // editing or superseding it.
        let d = new_decision(tx, pid, "退役させる商用決定");
        accept(tx, d.id, None).unwrap();
        let t = add_task_in(tx, pid, "linked task");
        link(tx, d.id, t).unwrap();
        assert!(read::decision_task_link_id(tx.conn(), d.id, t).unwrap().is_some());

        delete(tx, d.id).unwrap();
        assert!(read::decision(tx.conn(), d.id).unwrap().is_none(), "even an accepted decision goes away, row and all");
        // The link goes with it through the schema's `ON DELETE CASCADE`, so nothing dangles.
        assert!(
            read::decision_task_link_id(tx.conn(), d.id, t).unwrap().is_none(),
            "the link cascades away with it"
        );
        // It drops out of the reverse query as well.
        assert!(decisions_for_task(tx, t).is_empty());
    }

    #[test]
    fn delete_missing_decision_is_not_found() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let _pid = mk_project(tx, "amenbo 開発");
        assert!(delete(tx, 9999).is_err());
    }
}
