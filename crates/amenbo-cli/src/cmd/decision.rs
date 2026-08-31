//! `decision` and `decision comment`: the append-only "why", its lifecycle, the edges between
//! decisions, and the timeline each one carries.

use serde_json::json;

use amenbo_core::config::Paths;
use amenbo_core::model::{AttachmentTarget, ClassifiedSide, TaskStatus};
use amenbo_core::{ops, query, Store};

use crate::cli::*;
use crate::cmd::arg::{body_arg, body_arg_opt};
use crate::cmd::attach::attach_add;
use crate::cmd::comment::{comment_line, comment_not_found, comment_section, resolve_live_decision_comment};
use crate::cmd::labels::{decision_comment_label, decision_label, task_comment_label, task_label};
use crate::cmd::place::{project_or_bound, resolve_dim_pairs};
use crate::cmd::premise::{attach_revisit, note_revisit, standing_on, warn_if_premise_added_to_reserved, warn_if_unsettled_under_reserved};
use crate::cmd::task::resolve_task;
use crate::output::{confirm, count_header, human, print_json, warn_body, write_envelope, CliError, Flags};

/// Resolve a decision reference (`AMB-D-n`, or the bare `D-n` / `#n` / `n`). The numbers are globally unique on the device — a
/// number space of their own, separate from tasks — so no project context is needed. The id **is** the
/// conversational number: nothing is abbreviated and nothing is prefix-matched.
pub(crate) fn resolve_decision(store: &Store, id: &str) -> amenbo_core::Result<i64> {
    store.resolve_decision_ref(id)
}

/// The display name of a decision reference. `None` means the target dangles — a forward edge onto a
/// decision no longer live, whose title cannot be read — and the CLI (English-fixed) composes the
/// placeholder the core deliberately withholds.
pub(crate) fn decision_ref_name(name: &Option<String>) -> &str {
    name.as_deref().unwrap_or("(unknown)")
}

pub(crate) fn decision(store: &mut Store, flags: &Flags, sub: DecisionCmd) -> Result<i32, CliError> {
    match sub {
        DecisionCmd::Add { title, body, project, dim } => {
            let body = body_arg(body)?;
            let project_id = project_or_bound(store, project)?;
            // Resolved before the create, the way `task add`'s are: a misspelled axis or value — or one
            // that does not classify decisions at all — is an error with no decision left behind to go
            // and classify by hand.
            let dimension_values = resolve_dim_pairs(store, project_id, &dim, ClassifiedSide::Decision)?;
            let d = store.add_decision_with_dimensions(ops::decision::NewDecision {
                title, body, project_id,
            }, &dimension_values).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            warn_body(&detail.body); // non-blocking readability hint on write (stderr)
            // A decision has no `task finish-creating` to be turned away at, so the one place it can be
            // told that a required axis is still blank is the response to the create. The demand itself
            // is read at `decision accept` (`AMB-D-790`), and the human who presses that is not the one
            // who wrote this — so saying nothing here leaves the writer no moment to notice. It names,
            // it does not refuse: the record is written either way.
            let unmet = store.unmet_required_decision_axes(d.id).map_err(CliError::from)?;
            let mut resource = serde_json::to_value(&detail).unwrap();
            if !unmet.is_empty() {
                if let Some(obj) = resource.as_object_mut() {
                    obj.insert("unmet_required_dimensions".to_string(), json!(unmet));
                }
            }
            write_envelope(flags, "decision.add", "decision", resource, None, false, format!("✓ Recorded decision: {} ({})", d.title, decision_label(d.id)));
            if !unmet.is_empty() {
                human(flags, format!(
                    "  still to classify: {} — pass --dim <axis>=<value> here, or fill it in with `{} dimension set {} <axis> <value>` (accepting it is refused until then)",
                    unmet.join(", "),
                    Paths::command_name(),
                    decision_label(d.id),
                ));
            }
        }
        DecisionCmd::List { project, filter, sort, limit, offset, with_body } => {
            let project_id = project.map(|p| store.resolve_project_ref(&p)).transpose().map_err(CliError::from)?;
            let result = store.decision_list(query::DecisionListParams {
                // `text` is the structural term, and the CLI never fills it in: words are `search`'s door,
                // not a listing's (`AMB-D-449`). The GUI, which has a search box and no grammar to spell it
                // in, is the caller that passes one.
                project_id, filter_expr: filter, text: None, sort, limit, offset, with_body,
            }).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, count_header(result.count, result.total_matched, "decision"));
                for d in &result.decisions {
                    // "Superseded" is not a status, and does not stand in place of one: a rejected decision
                    // that was later replaced is both, so the edge is said after the status, not instead of it.
                    let state = if d.superseded_by.is_empty() {
                        d.status.as_str().to_string()
                    } else {
                        format!("{}, superseded", d.status.as_str())
                    };
                    human(flags, format!("  {}  [{}] {} (tasks: {})", d.r#ref, state, d.title, d.linked_task_count));
                    // `--with-body`: follow with the body, indented — a body column on a narrowed page.
                    if let Some(body) = &d.body {
                        for line in body.lines() {
                            human(flags, format!("      {line}"));
                        }
                    }
                }
            }
        }
        DecisionCmd::Show { id } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let detail = store.decision_detail(did).map_err(CliError::from)?;
            // The timeline is read with the decision, the way a task's is (`AMB-D-448`). What accepting or
            // rejecting one gave as its reason is a comment (`decision accept --reason`), so a page that
            // did not carry the timeline would leave the ruling's own reasoning off the only page anyone
            // opens to read the ruling.
            let comments = store.decision_comment_list(did, None, None).map(|r| r.comments).unwrap_or_default();
            if flags.json {
                // DecisionDetail stays as it is; `comments` is added beside it, whole, under the name
                // `task show` gives its own.
                let mut v = serde_json::to_value(&detail).unwrap_or(json!({}));
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("comments".to_string(), serde_json::to_value(&comments).unwrap_or(json!([])));
                }
                print_json(&v);
            } else {
                human(flags, format!("{}  {}", detail.r#ref, detail.title));
                human(flags, format!("status: {}", detail.status.as_str()));
                // How fresh the record is — the one thing a reader cannot get from the body. `recorded`
                // is when it was written down, `decided` when it was settled (a proposed decision has no
                // such moment, and a reopen clears it again). `last changed` moves on any write, an
                // accept included, so it is said only where it is news: not when it merely repeats the
                // instant the decision was recorded or settled at.
                human(flags, format!("recorded: {}", detail.created_at.to_rfc3339_z()));
                if let Some(at) = detail.decided_at {
                    // Who settled it rides on the same line as when, because the two are one fact about
                    // the ruling — and a reader who is deciding whether to trust it wants both at once
                    // (`AMB-D-788`, now that an AI may accept as well as a person).
                    let by = detail.decided_by.as_ref().map(|r| format!(" by {}", decider_name(&r.id))).unwrap_or_default();
                    human(flags, format!("decided: {}{by}", at.to_rfc3339_z()));
                }
                if detail.updated_at != detail.created_at && Some(detail.updated_at) != detail.decided_at {
                    human(flags, format!("last changed: {}", detail.updated_at.to_rfc3339_z()));
                }
                // Each edge kind is a set — one decision may supersede or amend several others — so every
                // edge gets its own line.
                for (label, edges) in [
                    ("supersedes", &detail.supersedes),
                    ("superseded by", &detail.superseded_by),
                    ("amends", &detail.amends),
                    ("amended by", &detail.amended_by),
                ] {
                    for s in edges.iter() {
                        human(flags, format!("{label}: {} {}", decision_label(s.id), decision_ref_name(&s.name)));
                    }
                }
                // The premises this decision stands on — read them first. A premise that has been overturned
                // is called out on its own line: this decision stands on rotten ground and wants revisiting
                // (the reason the edge type exists at all).
                for p in detail.builds_on.iter() {
                    let rot = match p.superseded_by.as_deref() {
                        Some(by) => format!("  ⚠ premise superseded by {by} — revisit this decision"),
                        None => String::new(),
                    };
                    human(flags, format!("builds on: {} {}{rot}", decision_label(p.id), decision_ref_name(&p.name)));
                }
                // The reverse edge: the decisions that would want revisiting if this one were overturned —
                // its blast radius, one hop out.
                for s in detail.built_on_by.iter() {
                    human(flags, format!("built on by: {} {}", decision_label(s.id), decision_ref_name(&s.name)));
                }
                // Mark both the body and the linked tasks even when they are empty. An empty body means a
                // draft whose conclusion was never written, and printing nothing would leave the reader
                // unable to tell that from having simply missed it.
                if detail.body.is_empty() {
                    human(flags, "body: (none)");
                } else {
                    human(flags, format!("\n{}", detail.body));
                }
                // What it is filed under, on one line, the way `task show` puts a task's (`AMB-D-781`).
                // Said only when there is something to say: a project that declares no axis would
                // otherwise carry an empty line on every decision it holds.
                if !detail.dimensions.is_empty() {
                    let classified = detail.dimensions.iter()
                        .map(|c| format!("{}={}", c.dimension, c.value))
                        .collect::<Vec<_>>()
                        .join(", ");
                    human(flags, format!("dimensions: {classified}"));
                }
                if detail.linked_tasks.is_empty() {
                    human(flags, "linked tasks: (none)");
                } else {
                    // Let the decision show whether the work it spawned is still outstanding. What has
                    // ended recedes behind an `[x]` — carried out or decided against, it is off the list
                    // either way — and everything but `todo`, the default, names its state, so a task that
                    // receded still says which of the two ways it went.
                    human(flags, "linked tasks:");
                    for t in detail.linked_tasks.iter() {
                        let check = if t.status.is_closed() { "x" } else { " " };
                        let state = match t.status {
                            TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::Rejected => {
                                format!(" ({})", t.status.as_str())
                            }
                            TaskStatus::Todo | TaskStatus::Done => String::new(),
                        };
                        human(flags, format!("  [{check}] {} {}{state}", task_label(t.id), t.name));
                    }
                }
                for line in comment_section(&comments, &format!("{} decision comment list {}", Paths::command_name(), decision_label(did))) {
                    human(flags, line);
                }
            }
        }
        DecisionCmd::Edit { id, title, body } => {
            let body = body_arg_opt(body)?;
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let mut changed = Vec::new();
            if title.is_some() { changed.push("title".to_string()); }
            if body.is_some() { changed.push("body".to_string()); }
            let d = store.update_decision(did, ops::decision::DecisionPatch { title, body }).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            write_envelope(flags, "decision.edit", "decision", serde_json::to_value(&detail).unwrap(), Some(changed), false, format!("✓ Edited decision: {}", decision_label(d.id)));
        }
        DecisionCmd::Accept { id, reason } => {
            let reason = body_arg_opt(reason)?;
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let by = flags.facet()?.as_str().to_string();
            let (d, changed) = store.accept_decision(did, Some(by), flags.facet()?).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            if changed {
                // `--reason` is thin sugar for adding one comment with the reason (the same shape as
                // `task block --reason`). It gets no field of its own. Only on a real acceptance —
                // re-accepting an already-settled decision changes nothing, so a reason must not pile up.
                add_reason_comment(store, flags, did, reason)?;
                write_envelope(flags, "decision.accept", "decision", serde_json::to_value(&detail).unwrap(), Some(vec!["status".to_string()]), false, format!("✓ Accepted decision: {}", decision_label(d.id)));
            } else {
                // Already accepted: say so plainly instead of a bare "✓" that reads as "just now settled".
                // The facet that accepted it is frozen; `reopen` is the sanctioned route to change it.
                write_envelope(flags, "decision.accept", "decision", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("• Decision {} is already accepted{} — no change. To change who accepted it, `reopen` then `accept` again.", decision_label(d.id), accepted_by_suffix(&d)));
            }
        }
        DecisionCmd::Reject { id, reason } => {
            let reason = body_arg_opt(reason)?;
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            // Read the blast radius (one hop) before rejecting. A reject leaves the edges in place, but
            // keeping the order the same gives all three verbs the same shape.
            let standing = standing_on(store, did);
            let (d, changed) = store.reject_decision(did, flags.facet()?).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            let mut resource = serde_json::to_value(&detail).unwrap();
            attach_revisit(&mut resource, &standing);
            if changed {
                // Only attach the reason on a real rejection; a re-reject changes nothing.
                add_reason_comment(store, flags, did, reason)?;
                write_envelope(flags, "decision.reject", "decision", resource, Some(vec!["status".to_string()]), false, format!("✓ Rejected decision: {}", decision_label(d.id)));
                note_revisit(flags, did, &standing);
            } else {
                write_envelope(flags, "decision.reject", "decision", resource, Some(vec![]), true, format!("• Decision {} is already rejected — no change.", decision_label(d.id)));
            }
        }
        DecisionCmd::Reopen { id } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let (d, changed) = store.reopen_decision(did).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            if changed {
                warn_if_unsettled_under_reserved(d.id, &detail, "reopening it");
                write_envelope(flags, "decision.reopen", "decision", serde_json::to_value(&detail).unwrap(), Some(vec!["status".to_string()]), false, format!("✓ Reopened decision: {}", decision_label(d.id)));
            } else {
                // Already proposed: reopening changes nothing, so say so plainly instead of a bare "✓"
                // that reads as "just now reopened" — the same two-branch shape as accept/reject.
                write_envelope(flags, "decision.reopen", "decision", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("• Decision {} is already proposed — no change.", decision_label(d.id)));
            }
        }
        DecisionCmd::Delete { id } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            // Take the label before the delete — the row will not be there afterwards — and read the blast
            // radius up front for the same reason.
            let label = decision_label(did);
            let standing = standing_on(store, did);
            if !confirm(flags, "delete decision")? {
                return Ok(0);
            }
            store.delete_decision(did, flags.facet()?).map_err(CliError::from)?;
            let mut resource = json!({ "id": did, "deleted": true });
            attach_revisit(&mut resource, &standing);
            write_envelope(flags, "decision.delete", "decision", resource, None, false, format!("✓ Deleted decision: {label}"));
            note_revisit(flags, did, &standing);
        }
        DecisionCmd::Supersede { decision: new_ref, replaces } => {
            let new_id = resolve_decision(store, &new_ref).map_err(CliError::from)?;
            let old_id = resolve_decision(store, &replaces).map_err(CliError::from)?;
            // Read the blast radius before drawing the edge: read it afterwards and the supersedes edge just
            // drawn (new_id itself) turns up among the decisions said to want revisiting.
            let standing = standing_on(store, old_id);
            let by = flags.facet()?.as_str().to_string();
            let (d, changed) = store.supersede_decision(new_id, old_id, Some(by), flags.facet()?).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            let mut resource = serde_json::to_value(&detail).unwrap();
            attach_revisit(&mut resource, &standing);
            if changed {
                // The old side is what stopped being current, so its card — not the new decision's — holds
                // the reservations whose ground just moved. Read only when the edge actually landed.
                match store.decision_detail(old_id) {
                    Ok(old) => warn_if_unsettled_under_reserved(old_id, &old, "superseding it"),
                    Err(e) => eprintln!(
                        "warning: could not check what rests on {}: {e}",
                        decision_label(old_id)
                    ),
                }
                write_envelope(flags, "decision.supersede", "decision", resource, Some(vec!["status".to_string(), "supersedes".to_string()]), false, format!("✓ {} supersedes {}", decision_label(new_id), decision_label(old_id)));
                note_revisit(flags, old_id, &standing);
            } else {
                // The edge was already there and the new side already settled: nothing to draw.
                write_envelope(flags, "decision.supersede", "decision", resource, Some(vec![]), true, format!("• {} already supersedes {} — no change.", decision_label(new_id), decision_label(old_id)));
            }
        }
        DecisionCmd::Amend { decision: new_ref, amends } => {
            let new_id = resolve_decision(store, &new_ref).map_err(CliError::from)?;
            let old_id = resolve_decision(store, &amends).map_err(CliError::from)?;
            let d = store.amend_decision(new_id, old_id).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            write_envelope(flags, "decision.amend", "decision", serde_json::to_value(&detail).unwrap(), Some(vec!["amends".to_string()]), false, format!("✓ {} amends {}", decision_label(new_id), decision_label(old_id)));
        }
        DecisionCmd::BuildsOn { decision: new_ref, on: premise_ref } => {
            let new_id = resolve_decision(store, &new_ref).map_err(CliError::from)?;
            let old_id = resolve_decision(store, &premise_ref).map_err(CliError::from)?;
            let d = store.decision_builds_on(new_id, old_id).map_err(CliError::from)?;
            let detail = store.decision_detail(d.id).map_err(CliError::from)?;
            write_envelope(flags, "decision.builds_on", "decision", serde_json::to_value(&detail).unwrap(), Some(vec!["builds_on".to_string()]), false, format!("✓ {} builds on {}", decision_label(new_id), decision_label(old_id)));
        }
        DecisionCmd::Unlink { decision: from_ref, from: to_ref } => {
            let decision_id = resolve_decision(store, &from_ref).map_err(CliError::from)?;
            let target_id = resolve_decision(store, &to_ref).map_err(CliError::from)?;
            let removed = store.unlink_decision_edge(decision_id, target_id).map_err(CliError::from)?;
            write_envelope(flags, "decision.unlink_edge", "decision_edge", json!({ "decision_id": decision_id, "target_decision_id": target_id, "unlinked": removed }), None, !removed, format!("✓ Unlinked {} → {}", decision_label(decision_id), decision_label(target_id)));
        }
        DecisionCmd::Link { decision: d_ref, task, unlink } => {
            let did = resolve_decision(store, &d_ref).map_err(CliError::from)?;
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            if unlink {
                let changed = store.unlink_decision(did, tid).map_err(CliError::from)?;
                write_envelope(flags, "decision.unlink", "decision_task_link", json!({ "decision_id": did, "task_id": tid, "unlinked": changed }), None, !changed, format!("✓ Unlinked {} ⇄ {}", decision_label(did), task_label(tid)));
            } else {
                let (l, created) = store.link_decision(did, tid).map_err(CliError::from)?;
                if created {
                    warn_if_premise_added_to_reserved(store, tid, "you linked a decision it now rests on as a premise");
                }
                write_envelope(flags, "decision.link", "decision_task_link", serde_json::to_value(&l).unwrap(), None, !created, format!("✓ Linked {} ⇄ {}", decision_label(did), task_label(tid)));
            }
        }
        DecisionCmd::Promote { comment, title, project } => {
            let from_task = store.resolve_task_comment(&comment).map_err(CliError::from)?.first().copied();
            let from_decision = store.resolve_decision_comment(&comment).map_err(CliError::from)?.first().copied();
            let (did, source) = match (from_task, from_decision) {
                (Some(a), Some(b)) => return Err(ambiguous_comment(&comment, a, b)),
                (Some(cid), None) => (promote_task_comment(store, cid, title, project)?, task_comment_label(cid)),
                (None, Some(cid)) => (promote_decision_comment(store, cid, title, project)?, decision_comment_label(cid)),
                (None, None) => return Err(comment_not_found(&comment)),
            };
            let detail = store.decision_detail(did).map_err(CliError::from)?;
            write_envelope(flags, "decision.promote", "decision", serde_json::to_value(&detail).unwrap(), None, false, format!("✓ Promoted {source} to decision: {} ({})", detail.title, decision_label(did)));
        }
        DecisionCmd::Comment { sub } => return decision_comment(store, flags, sub),
        DecisionCmd::Attach { id, source, url, name } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            return attach_add(store, flags, AttachmentTarget::Decision, did, &source, url, name);
        }
    }
    Ok(0)
}

/// The task-comment side of `decision promote`: the comment's text becomes the body, its task's project
/// becomes the home, and the new decision is linked back to that task — the decision is that task's
/// premise, which is exactly what the edge says.
fn promote_task_comment(store: &mut Store, cid: i64, title: String, project: Option<String>) -> Result<i64, CliError> {
    let c = store.task_comment(cid).map_err(CliError::from)?.ok_or_else(|| comment_not_found(&task_comment_label(cid)))?;
    let task_id = c.task_id;
    let body = c.text.clone();
    let project_id = match project {
        Some(p) => store.resolve_project_ref(&p).map_err(CliError::from)?,
        None => store.task(task_id).map_err(CliError::from)?
            .and_then(|t| t.project_id)
            .ok_or_else(|| CliError { code: "invalid_value", message: "the comment's task has no project; pass --project".to_string(), hint: None, exit: 2 })?,
    };
    let d = store.add_decision(ops::decision::NewDecision { title, body, project_id }).map_err(CliError::from)?;
    store.link_decision(d.id, task_id).map_err(CliError::from)?;
    Ok(d.id)
}

/// The decision-comment side of `decision promote`: the text becomes the body and the comment's decision
/// gives the home, but **no edge is drawn back to it**. A record raised out of a decision's comment thread
/// is a question that turned into its own, and an automatic link would claim a relation promote cannot
/// know. Where one does hold, its author names it — `builds-on`, `amend`, `supersede`.
fn promote_decision_comment(store: &mut Store, cid: i64, title: String, project: Option<String>) -> Result<i64, CliError> {
    let c = store.decision_comment(cid).map_err(CliError::from)?.ok_or_else(|| comment_not_found(&decision_comment_label(cid)))?;
    let body = c.text.clone();
    let project_id = match project {
        Some(p) => store.resolve_project_ref(&p).map_err(CliError::from)?,
        None => store.decision_detail(c.decision_id).map_err(CliError::from)?
            .project.map(|p| p.id)
            .ok_or_else(|| CliError { code: "invalid_value", message: "the comment's decision has no project; pass --project".to_string(), hint: None, exit: 2 })?,
    };
    let d = store.add_decision(ops::decision::NewDecision { title, body, project_id }).map_err(CliError::from)?;
    Ok(d.id)
}

/// A bare `<n>` handed to `decision promote` when both comment tables hold that key. They number
/// independently, so the number alone names a row in each and the kind code is what disjoins them — the
/// same shape as a bare number that is both a task and a decision. Refused, never guessed at.
fn ambiguous_comment(reference: &str, task_comment_id: i64, decision_comment_id: i64) -> CliError {
    CliError {
        code: "invalid_value",
        message: format!(
            "'{reference}' names both {} and {}; say which",
            task_comment_label(task_comment_id),
            decision_comment_label(decision_comment_id)
        ),
        hint: None,
        exit: 2,
    }
}

/// The name to print for whoever settled a decision. `decided_by` holds a free-text decider token, so
/// the two facets are named the way the rest of the page names them ([`query::facet_label`], the same
/// call `task show` makes for an assignee) and any other token a store carries is printed as written.
fn decider_name(token: &str) -> String {
    match amenbo_core::model::ActorKind::parse(token) {
        Some(kind) => query::facet_label(Some(kind)),
        None => token.to_string(),
    }
}

/// A `" (by <facet>, <utc>)"` suffix naming who settled an already-accepted decision, empty when the
/// stamps are missing. Shown on the idempotent re-accept so `reopen` is an informed choice, not a guess.
fn accepted_by_suffix(d: &amenbo_core::model::Decision) -> String {
    match (&d.decided_by, &d.decided_at) {
        (Some(who), Some(at)) => format!(" (by {}, {})", decider_name(who), at.to_rfc3339_z()),
        (Some(who), None) => format!(" (by {})", decider_name(who)),
        (None, Some(at)) => format!(" (at {})", at.to_rfc3339_z()),
        (None, None) => String::new(),
    }
}

/// Record the reason a decision was accepted or rejected as a comment (the same shape as
/// `task block --reason`). An empty or whitespace-only reason is ignored.
fn add_reason_comment(store: &mut Store, flags: &Flags, decision_id: i64, reason: Option<String>) -> Result<(), CliError> {
    if let Some(r) = reason.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        // The author is our own facet; the author argument is the trace string for the audit log.
        store.add_decision_comment(decision_id, flags.facet()?, r).map_err(CliError::from)?;
    }
    Ok(())
}

/// `decision comment add/list` — mirrors [`comment`](crate::cmd::comment::comment) on the task side.
pub(crate) fn decision_comment(store: &mut Store, flags: &Flags, sub: DecisionCommentCmd) -> Result<i32, CliError> {
    match sub {
        DecisionCommentCmd::Add { decision, text } => {
            let text = body_arg(text)?;
            let did = resolve_decision(store, &decision).map_err(CliError::from)?;
            // The author is our own facet; add_comment's author argument is the trace string for the audit log.
            let c = store.add_decision_comment(did, flags.facet()?, &text).map_err(CliError::from)?;
            warn_body(&text); // non-blocking readability hint on write (stderr)
            write_envelope(flags, "decision.comment.add", "comment", serde_json::to_value(&c).unwrap(), None, false, format!("✓ Added comment: {}", decision_label(did)));
        }
        DecisionCommentCmd::List { decision, limit, offset } => {
            let did = resolve_decision(store, &decision).map_err(CliError::from)?;
            let result = store.decision_comment_list(did, offset, limit).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, format!("{} — {}", count_header(result.count, result.total_matched, "comment"), decision_ref_name(&result.decision.name)));
                for c in &result.comments {
                    human(flags, comment_line(amenbo_core::idref::RefKind::DecisionComment, c));
                }
            }
        }
        DecisionCommentCmd::Rm { comment } => {
            let cid = resolve_live_decision_comment(store, &comment)?;
            if !confirm(flags, "delete comment")? {
                return Ok(0);
            }
            let changed = store.remove_decision_comment(cid).map_err(CliError::from)?;
            write_envelope(flags, "decision.comment.rm", "comment", json!({ "id": cid, "deleted": true }), None, !changed, format!("✓ Deleted comment: {}", decision_comment_label(cid)));
        }
        DecisionCommentCmd::Edit { comment, text } => {
            let text = body_arg(text)?;
            let cid = resolve_live_decision_comment(store, &comment)?;
            let c = store.edit_decision_comment(cid, &text).map_err(CliError::from)?;
            warn_body(&text);
            write_envelope(flags, "decision.comment.edit", "comment", serde_json::to_value(&c).unwrap(), Some(vec!["text".to_string()]), false, format!("✓ Edited comment: {}", decision_comment_label(cid)));
        }
        DecisionCommentCmd::Attach { comment, source, url, name } => {
            // Look only in the decision-comment table (symmetric with the task side of `comment attach`).
            let cid = resolve_live_decision_comment(store, &comment)?;
            return attach_add(store, flags, AttachmentTarget::DecisionComment, cid, &source, url, name);
        }
    }
    Ok(0)
}
