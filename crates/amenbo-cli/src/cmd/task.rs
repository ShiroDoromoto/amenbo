//! `task`: the backlog's own commands — filing, editing, ordering, the status transitions that
//! reserve and end a task, and the commits anchored to it.

use serde_json::json;

use amenbo_core::config::Paths;
use amenbo_core::model::{ActorKind, AttachmentTarget, TaskStatus};
use amenbo_core::{activity_log, ops, query, time, Store};

use crate::cli::*;
use crate::cmd::arg::{body_arg, body_arg_opt, parse_date_opt, parse_priority, pos_from_keys};
use crate::cmd::attach::attach_add;
use crate::cmd::comment::comment_section;
use crate::cmd::decision::decision_ref_name;
use crate::cmd::guard::guard_ai_task_delete;
use crate::cmd::labels::{decision_label, task_label};
use crate::cmd::outbox::{emit_event, emit_unblocks, newly_ready_or_warn};
use crate::cmd::place::{project_or_bound, resolve_bound_folder, resolve_dim_pairs};
use crate::cmd::premise::{attach_premise_change, premise_change, premise_change_lines, premise_change_when, warn_if_premise_added_to_reserved, warn_premise_change};
use crate::output::{confirm, count_header, human, print_json, warn_body, write_envelope, CliError, Flags};

/// Resolve a task reference (`AMB-T-n`, or the bare `T-n` / `#n` / `n`). The numbers are globally unique on the device, so no
/// project context is needed. The id **is** the conversational number: nothing is abbreviated and nothing is
/// prefix-matched. The return type mirrors core's `resolve`, so `.map_err(CliError::from)?` works as is.
pub(crate) fn resolve_task(store: &Store, id: &str) -> amenbo_core::Result<i64> {
    store.resolve_task_ref(id)
}

pub(crate) fn task(store: &mut Store, flags: &Flags, sub: TaskCmd) -> Result<i32, CliError> {
    match sub {
        TaskCmd::Add { title, project, due, start, priority, notes, to, ai, dim, at } => {
            if ai && to.is_none() {
                return Err(CliError::from(amenbo_core::Error::invalid("--ai requires --to")));
            }
            // After the argument checks: a rejected invocation should not have drained the pipe first.
            let notes = body_arg(notes)?;
            // Resolve `--to` to a facet first, so an unknown assignee is refused before the task exists — an
            // error after creation would leave an orphan behind. The assignee is a facet and nothing else:
            // `--ai` means the AI facet, otherwise the token is resolved to one.
            let assignee_kind = match to {
                Some(ref to) => Some(if ai { ActorKind::Ai } else { store.resolve_assignee_facet(to).map_err(CliError::from)? }),
                None => None,
            };
            // Every task belongs to a project. Refuse a project-less create — an unnumbered orphan/inbox
            // task has low discoverability and breaks per-project numbering — and point at the existing
            // projects so the caller can pick one. Enforced here at the CLI write boundary, not in core
            // add_task: backup/migrate must still reconstruct legacy project-less rows (project_id: None),
            // so it is a write policy, not a data invariant.
            //
            // Where the slot comes from is `project_or_bound`'s answer, the same one `decision add` and
            // `dimension add` take: what `--project` named, else the folder's own binding. Not the *reach*,
            // which answers for an AI alone — a reach is what closes an AI to one project, and a human's is
            // the whole device, so reading it would make a human name the project their folder already
            // names. An AI cannot pass `--project`, so for it this is the binding either way; without a
            // binding there is nothing to fill the slot with, and the create is refused.
            let project_id = project_or_bound(store, project)?;
            let due_on = parse_date_opt(&due)?;
            let start_on = parse_date_opt(&start)?;
            let priority = match priority { Some(p) => Some(parse_priority(&p)?), None => None };
            // Resolved before the create, like `--to` above: a misspelled axis or value is an error with
            // no task left behind to go and classify by hand.
            let dimension_values = resolve_dim_pairs(store, project_id, &dim)?;
            // Resolved before the create for the same reason `--to` and `--dim` are: a folder name that
            // answers to nothing is an error with no task left behind to go and correct.
            let at_binding_id = match at {
                Some(ref folder) => Some(resolve_bound_folder(store, project_id, folder)?),
                None => None,
            };
            let t = store.add_task_with_dimensions(ops::task::NewTask {
                title, project_id: Some(project_id), due_on, start_on, priority, notes,
                created_by_kind: Some(flags.facet()?), at_binding_id,
            }, &dimension_values).map_err(CliError::from)?;
            emit_event(store, flags, t.id, activity_log::event::task_created(&t.title));
            // With `--to`, hand it over here as well, folding create→assign into one command. They are two
            // logical operations and therefore two transactions, so the add survives a failing assign.
            if let Some(kind) = assignee_kind {
                store.set_task_assignee(t.id, Some(kind), flags.facet()?).map_err(CliError::from)?;
                emit_event(store, flags, t.id, activity_log::event::task_assigned(Some(kind.as_str())));
            }
            let detail = store.task_detail(t.id).map_err(CliError::from)?;
            warn_body(&detail.notes); // non-blocking readability hint on write (stderr)
            // The creation is two stages, and this response is where the one who just typed it learns so
            // (`AMB-D-556`): the id it was given, that it is still being created, and the command that
            // ends the creation. The wording is the next move to make, not a question to answer
            // (`AMB-D-558`) — there is nobody to ask, the creator being the one who ends it.
            let finish = format!("{} task finish-creating {}", Paths::command_name(), task_label(t.id));
            let mut resource = serde_json::to_value(&detail).unwrap();
            // Read off the task rather than assumed: what the response says about the stage it is at is
            // whatever the store put there.
            if detail.draft {
                if let Some(obj) = resource.as_object_mut() {
                    obj.insert("next".to_string(), json!(finish));
                }
            }
            let created = format!("✓ Created task: {} ({})", t.title, task_label(t.id));
            let human_line = if detail.draft { format!("{created} — still being created") } else { created };
            write_envelope(flags, "task.add", "task", resource, None, false, human_line);
            if detail.draft {
                human(flags, format!("  finish creating it: {finish}"));
            }
        }
        TaskCmd::List { project, filter, sort, limit, offset } => {
            let project_id = project.map(|p| store.resolve_project_ref(&p)).transpose().map_err(CliError::from)?;
            let result = store.list_tasks(query::ListParams {
                project_id, filter_expr: filter, sort,
                limit, offset,
                // The structural term is for a caller with a phrase in hand and no expression to put it in
                // (a search box). The CLI never has one here: words are `search`'s door (`AMB-D-449`).
                text: None,
                // Reach belongs to the store — the surface does not declare it here; `Store`'s read supply
                // applies it.
            }).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, count_header(result.count, result.total_matched, "task"));
                // An empty mailbox that means "not yet" has to say so on the spot. A start day mistyped far
                // into the future hides a task in the one way nothing else catches — the list is empty and
                // reads as finished — so the count and the first day arrive with the emptiness.
                if let Some(w) = &result.waiting_on_start {
                    human(
                        flags,
                        format!(
                            "  ({} waiting on a start day — earliest {})",
                            w.count,
                            time::date_to_string(w.earliest)
                        ),
                    );
                }
                for t in &result.tasks {
                    let check = if t.completed { "x" } else { " " };
                    let due = t.due_on.map(|d| format!(" due:{}", time::date_to_string(d))).unwrap_or_default();
                    let pri = t.priority.map(|p| format!(" [{}]", p.as_str())).unwrap_or_default();
                    // Why this row is not in the mailbox, said on the row itself. A plain `task list` shows
                    // everything, so a task held back by a start day still ahead has to carry its reason
                    // here or it reads as ordinary work that the mailbox inexplicably skips. Written only
                    // when it is a reason — like `due:`, and unlike the marked-when-empty lines of `task
                    // show`, a listing row states what is so, and a date column of `-` on every other row
                    // buys nothing.
                    let waiting = t.not_started_until
                        .map(|d| format!(" waiting-until:{}", time::date_to_string(d)))
                        .unwrap_or_default();
                    // The same thing said about the fourth premise: a task still being created is listed
                    // like any other (`AMB-D-555`), so the row is where it says why the mailbox skips it.
                    let draft = if t.draft { " draft" } else { "" };
                    human(flags, format!("  [{check}] {}  {}{}{}{}{}", task_label(t.id), t.title, due, waiting, draft, pri));
                }
            }
        }
        TaskCmd::Show { id } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            // Close off, structurally, the mistake of reading only the notes and starting work without seeing
            // the comments or the decisions. A single `task show` must put all four in front of the reader —
            // the task, its notes, its linked decisions, its latest comments — so the links and comments are
            // fetched here too. Shipping them in one command's output is more reliable than prompting anyone
            // to go and look.
            let decisions = store.decisions_for_task(tid);
            let comments = store.comment_list(tid, None, None).map(|r| r.comments).unwrap_or_default();
            if flags.json {
                // TaskDetail stays as it is (task at the top level); linked_decisions / comments are added
                // beside it. `comments` carries the whole timeline, under the name `decision show` gives
                // its own (`AMB-D-448`) — the text output is where the reading is cut short, never here.
                let mut v = serde_json::to_value(&detail).unwrap_or(json!({}));
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("linked_decisions".to_string(), serde_json::to_value(&decisions).unwrap_or(json!([])));
                    obj.insert("comments".to_string(), serde_json::to_value(&comments).unwrap_or(json!([])));
                }
                // The holder-side surface of `AMB-D-366`: only the reservation holder (in_progress) is at risk
                // of a premise silently pinned on after they reserved, so gate on it and fold in what changed.
                if detail.status == TaskStatus::InProgress {
                    attach_premise_change(&mut v, &premise_change(store, tid));
                }
                print_json(&v);
            } else {
                // One name only: the ref. The id is the conversational number, so there is no second
                // identifier to print alongside it.
                human(flags, format!("{}  {}", detail.r#ref, detail.title));
                let due = detail.due_on.map(time::date_to_string).unwrap_or_else(|| "-".to_string());
                // All five states, not the two-valued reading of them: whether a task is reserved
                // (`in_progress`) or free to take (`todo`) is what a reader opening this page acts on,
                // and both terminals (`AMB-D-397`) name themselves rather than collapsing into one
                // "closed". Nothing is lost — closed is derivable from the status — and `--json`
                // carries both fields for anything filtering on the flag.
                human(flags, format!("status: {} / due: {} / priority: {}", detail.status.as_str(), due, detail.priority.map(|p| p.as_str()).unwrap_or("-")));
                let assignee = match detail.assignee_kind {
                    Some(k) => query::facet_label(Some(k)),
                    None => "-".to_string(),
                };
                human(flags, format!("assignee: {} / comments: {}", assignee, detail.num_comments));
                // Always mark whether the task is placed; never omit the line when empty. Being unplaced
                // is a meaningful state — a task belonging to no project — so say `(none)` out loud.
                match &detail.placement {
                    None => human(flags, "project: (none)"),
                    Some(p) => human(flags, format!("project: {}", p.project.name)),
                }
                // The folder this task is worked in (`AMB-D-648`). It folds away when there is none, the way
                // `dimensions` does and unlike the premise lines below: naming a folder is something a
                // person opts into, and a task that names none is held back by nothing — so a
                // `folder: (none)` on every task in every store that never uses one would say nothing at
                // all. The path is what is printed, since that is what a reader reads; `--json` carries the
                // binding's id beside it for whatever re-points or re-reads the folder later.
                if let Some(at) = &detail.at {
                    human(flags, format!("folder: {}", at.dir));
                }
                // What the task is classified as (`AMB-D-101`). Unlike the lines around it this one folds
                // away when there is nothing to say: an axis is something a store opts into, so a
                // `dimensions: (none)` would be printed forever in every store that classifies nothing,
                // and an unclassified task is not a state anyone is stopped by. Written the way the filter
                // takes it (`axis=value`), so a line read here pastes into `--filter "dim:…"`.
                if !detail.dimensions.is_empty() {
                    let classified = detail.dimensions.iter()
                        .map(|d| format!("{}={}", d.dimension, d.value))
                        .collect::<Vec<_>>().join(", ");
                    human(flags, format!("dimensions: {classified}"));
                }
                // Empty means nothing blocks this task and it can be started; say `(none)`. Omitting the line
                // would leave the reader unable to tell "no dependencies" from "dependencies not checked".
                if detail.blocked_by.is_empty() {
                    human(flags, "blocked by: (none)");
                } else {
                    let waiting = detail.blocked_by.iter()
                        .map(|b| format!("{} {}", task_label(b.id), b.name))
                        .collect::<Vec<_>>().join(", ");
                    human(flags, format!("blocked by: {waiting} (cannot start)"));
                }
                // A premise that is not settled stops the work too. Mark it even when empty, or the reader
                // cannot tell "the premises are settled" from "the premises were never checked".
                if detail.blocked_by_decisions.is_empty() {
                    human(flags, "blocked by decisions: (none)");
                } else {
                    let premises = detail.blocked_by_decisions.iter()
                        .map(|d| format!("{} {}", decision_label(d.id), decision_ref_name(&d.name)))
                        .collect::<Vec<_>>().join(", ");
                    human(flags, format!("blocked by decisions: {premises} (not settled — cannot start)"));
                }
                // The third reason a task is not ready: a start day that has not arrived. Marked even when
                // empty, like the two above — a reader who cannot tell "startable today" from "the start day
                // was never looked at" is back to guessing why the task is not in the mailbox.
                match detail.not_started_until {
                    None => human(flags, "not started until: (none)"),
                    Some(d) => human(
                        flags,
                        format!("not started until: {} (cannot start yet)", time::date_to_string(d)),
                    ),
                }
                // The fourth reason (`AMB-D-553`). Marked even when empty, like the three above: a task
                // still being created is on this page like any other (`AMB-D-555`), so the page is where
                // a reader finds out that is what is holding it — and the reader is the one who clears it.
                human(
                    flags,
                    if detail.draft {
                        "creation: not finished (cannot start yet)"
                    } else {
                        "creation: finished"
                    },
                );
                // The quiet early-warning surface of `AMB-D-366`: if this task is reserved (in_progress) and a
                // premise was pinned on after the reservation — silently dropping `ready` — say so here, on
                // an ordinary read, so the holder notices long before they try to complete it. Only printed
                // when something actually shifted (nothing to say otherwise).
                if detail.status == TaskStatus::InProgress {
                    let pc = premise_change(store, tid);
                    if pc.any() {
                        human(flags, "premises changed since reserved (readiness withdrawn — AMB-D-366):");
                        for line in premise_change_lines(&pc) {
                            human(flags, line);
                        }
                    }
                }
                // The dependents — what becomes startable once this task is done. Always mark it, printing
                // `blocks: (none)` when empty; leaving the line out reads to an AI as "nothing follows".
                if detail.blocks.is_empty() {
                    human(flags, "blocks: (none)");
                } else {
                    let blocks = detail.blocks.iter()
                        .map(|b| format!("{} {}", task_label(b.id), b.name))
                        .collect::<Vec<_>>().join(", ");
                    human(flags, format!("blocks ({}): {blocks}", detail.blocks.len()));
                }
                // The rest of what must be read before starting, all in this one command: notes, linked
                // decisions, latest comments. Each is marked even when empty (`notes: (none)` and so on), so
                // an absent note is never mistaken for one that went unread.
                if detail.notes.trim().is_empty() {
                    human(flags, "notes: (none)");
                } else {
                    human(flags, format!("notes:\n{}", detail.notes));
                }
                if decisions.is_empty() {
                    human(flags, "decisions: (none)");
                } else {
                    human(flags, format!("decisions ({}):", decisions.len()));
                    for d in &decisions {
                        let r = decision_label(d.id);
                        human(flags, format!("  {r} [{}] {}", d.status, d.title));
                    }
                }
                // The timeline's tail, in the shape `decision show` prints it too (`AMB-D-448`).
                for line in comment_section(&comments, &format!("{} comment list {}", Paths::command_name(), task_label(tid))) {
                    human(flags, line);
                }
            }
        }
        TaskCmd::Update { id, title, notes, due, start, priority, clear_due, clear_start, clear_priority, at, clear_at } => {
            let notes = body_arg_opt(notes)?;
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let mut changed = Vec::new();
            if title.is_some() { changed.push("title".to_string()); }
            if notes.is_some() { changed.push("notes".to_string()); }
            if due.is_some() || clear_due { changed.push("due_on".to_string()); }
            if start.is_some() || clear_start { changed.push("start_on".to_string()); }
            if priority.is_some() || clear_priority { changed.push("priority".to_string()); }
            if at.is_some() || clear_at { changed.push("at_binding_id".to_string()); }
            let due_on = parse_date_opt(&due)?;
            let start_on = parse_date_opt(&start)?;
            let priority = match priority { Some(p) => Some(parse_priority(&p)?), None => None };
            // The folder is named among the task's **own project's** — which is where a task's folder comes
            // from, so an unplaced task has none to name and is refused rather than pointed anywhere.
            let at_binding_id = match at {
                Some(ref folder) => {
                    let project_id = store.task_detail(tid).map_err(CliError::from)?
                        .placement.map(|p| p.project.id)
                        .ok_or_else(|| CliError::from(amenbo_core::Error::invalid(
                            "--at names one of the project's linked folders, and this task is in no project",
                        )))?;
                    Some(resolve_bound_folder(store, project_id, folder)?)
                }
                None => None,
            };
            let t = store.update_task(tid, ops::task::TaskPatch {
                title, notes, due_on, start_on, priority, clear_due, clear_priority, clear_start,
                at_binding_id, clear_at,
            }).map_err(CliError::from)?;
            let detail = store.task_detail(t.id).map_err(CliError::from)?;
            // Hint only when notes were actually written — an update that touches just the title should not
            // drag the existing notes back up.
            if changed.iter().any(|c| c == "notes") {
                warn_body(&detail.notes);
            }
            write_envelope(flags, "task.update", "task", serde_json::to_value(&detail).unwrap(), Some(changed), false, format!("✓ Updated task: {}", task_label(t.id)));
        }
        TaskCmd::FinishCreating { id } => return task_finish_creating(store, flags, &id),
        TaskCmd::Done { id } => return task_complete(store, flags, &id, true),
        TaskCmd::Reopen { id } => return task_complete(store, flags, &id, false),
        TaskCmd::Status { id, status } => return task_set_status(store, flags, &id, &status),
        TaskCmd::Block { id, reason } => return task_block(store, flags, &id, body_arg_opt(reason)?),
        TaskCmd::Reject { id, reason } => return task_reject(store, flags, &id, body_arg(reason)?),
        TaskCmd::Move { id, project, before, after, top, bottom } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let project_id = project.map(|p| store.resolve_project_ref(&p)).transpose().map_err(CliError::from)?;
            let before = before.map(|b| resolve_task(store, &b)).transpose().map_err(CliError::from)?;
            let after = after.map(|a| resolve_task(store, &a)).transpose().map_err(CliError::from)?;
            let pos = pos_from_keys(top, bottom, before, after)?;
            let project_for_event = project_id.map(|pid| pid.to_string());
            let t = store.move_task(tid, project_id, pos, flags.facet()?).map_err(CliError::from)?;
            emit_event(store, flags, tid, activity_log::event::task_moved(project_for_event.as_deref()));
            let detail = store.task_detail(t.id).map_err(CliError::from)?;
            write_envelope(flags, "task.move", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["placement".to_string()]), false, format!("✓ Moved task: {}", task_label(t.id)));
        }
        TaskCmd::Depend { id, on } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let bid = resolve_task(store, &on).map_err(CliError::from)?;
            let (_edge, created) = store.depend_task(tid, bid, Some(flags.facet()?)).map_err(CliError::from)?;
            if created {
                warn_if_premise_added_to_reserved(store, tid, "you added a blocker it must be done after");
            }
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            write_envelope(flags, "task.depend", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["blocked_by".to_string()]), !created, format!("✓ Added dependency ({} waits on {}): {}", task_label(tid), task_label(bid), task_label(tid)));
        }
        TaskCmd::Undepend { id, on } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let bid = resolve_task(store, &on).map_err(CliError::from)?;
            let changed = store.undepend_task(tid, bid).map_err(CliError::from)?;
            // If dropping the blocker made this task ready, emit the unblock signal.
            if changed && newly_ready_or_warn(store, bid).contains(&tid) {
                emit_event(store, flags, tid, activity_log::event::task_unblocked(&bid.to_string()));
            }
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            write_envelope(flags, "task.undepend", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["blocked_by".to_string()]), !changed, format!("✓ Removed dependency: {}", task_label(tid)));
        }
        TaskCmd::Attach { id, source, url, name } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            return attach_add(store, flags, AttachmentTarget::Task, tid, &source, url, name);
        }
        TaskCmd::Commit { sub } => return task_commit(store, flags, sub),
        TaskCmd::Assign { id, to, ai } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            // The assignee is a facet and nothing else: `--ai` means the AI facet, otherwise the token is
            // resolved to one.
            let kind = if ai { ActorKind::Ai } else { store.resolve_assignee_facet(&to).map_err(CliError::from)? };
            // Assigning the same facet again is an idempotent no-op — skip the write entirely.
            // `set_task_assignee` commits its own transaction, so calling it would move updated_at even when
            // the value does not change.
            let noop = store.task(tid).map_err(CliError::from)?
                .is_some_and(|t| t.assignee_kind == Some(kind));
            if !noop {
                store.set_task_assignee(tid, Some(kind), flags.facet()?).map_err(CliError::from)?;
                emit_event(store, flags, tid, activity_log::event::task_assigned(Some(kind.as_str())));
            }
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            let to_label = if ai { " (to that person's AI)" } else { "" };
            let msg = format!("✓ Assigned{to_label}: {}", task_label(tid));
            write_envelope(flags, "task.assign", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["assignee".to_string()]), noop, msg);
        }
        TaskCmd::Unassign { id } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            let noop = store.task(tid).map_err(CliError::from)?.is_some_and(|t| t.assignee_kind.is_none());
            if !noop {
                store.set_task_assignee(tid, None, flags.facet()?).map_err(CliError::from)?;
                emit_event(store, flags, tid, activity_log::event::task_assigned(None));
            }
            let detail = store.task_detail(tid).map_err(CliError::from)?;
            write_envelope(flags, "task.unassign", "task", serde_json::to_value(&detail).unwrap(), Some(vec!["assignee".to_string()]), noop, format!("✓ Unassigned: {}", task_label(tid)));
        }
        TaskCmd::Delete { id } => {
            let tid = resolve_task(store, &id).map_err(CliError::from)?;
            guard_ai_task_delete(store, flags, tid)?;
            if !confirm(flags, "delete task")? {
                return Ok(0);
            }
            // Take the label before the delete — the row will not be there afterwards.
            let label = task_label(tid);
            store.delete_task(tid, flags.facet()?).map_err(CliError::from)?;
            write_envelope(flags, "task.delete", "task", json!({ "id": tid, "deleted": true }), None, false, format!("✓ Deleted task: {label}"));
        }
    }
    Ok(0)
}

// ───────────────────────── task commit ─────────────────────────

/// A task's git commit SHAs: record / list / forget. amenbo stores each SHA opaquely — the ops door
/// admits only full-length lower-case hex and folds case, and the `(task_id, sha)` index makes a
/// re-record idempotent. The chain runs history to task, since a public commit carries no store-local
/// reference.
fn task_commit(store: &mut Store, flags: &Flags, sub: TaskCommitCmd) -> Result<i32, CliError> {
    match sub {
        TaskCommitCmd::Add { task, sha } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let (row, created) = store.add_task_commit(tid, &sha, Some(flags.facet()?)).map_err(CliError::from)?;
            let msg = format!("✓ Recorded commit {} on {}", row.sha, task_label(tid));
            write_envelope(flags, "task.commit.add", "task_commit", serde_json::to_value(&row).unwrap(), None, !created, msg);
        }
        TaskCommitCmd::List { task } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let commits = store.task_commits(tid).map_err(CliError::from)?;
            if flags.json {
                print_json(&json!({ "task": tid, "count": commits.len(), "commits": commits }));
            } else {
                human(flags, format!("{} commit(s) — {}", commits.len(), task_label(tid)));
                for c in &commits {
                    human(flags, format!("  {}  [{}]", c.sha, c.created_at.to_rfc3339_z()));
                }
            }
        }
        TaskCommitCmd::Rm { task, sha } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            if !confirm(flags, "forget commit")? {
                return Ok(0);
            }
            let changed = store.remove_task_commit(tid, &sha).map_err(CliError::from)?;
            let data = json!({ "task": tid, "sha": sha, "deleted": changed });
            write_envelope(flags, "task.commit.rm", "task_commit", data, None, !changed, format!("✓ Forgot commit on {}", task_label(tid)));
        }
    }
    Ok(0)
}

/// `task finish-creating <id>`: end the second stage of a creation (`AMB-D-554`). Until this lands the task
/// is held out of the mailbox and refused a reservation; after it, it is ordinary work. Finishing a creation
/// that is already finished is a reported no-op rather than an error — the caller wanted the task creatable,
/// and it is.
fn task_finish_creating(store: &mut Store, flags: &Flags, id: &str) -> Result<i32, CliError> {
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let still_being_created = store.task(tid).map_err(CliError::from)?.is_some_and(|t| t.draft);
    if !still_being_created {
        let detail = store.task_detail(tid).map_err(CliError::from)?;
        say_creation_finished(flags, &detail, vec![], true, format!("(no change) {}", task_label(tid)));
        return Ok(0);
    }
    let t = store.finish_task_creation(tid, flags.facet()?).map_err(CliError::from)?;
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let changed = vec!["draft".to_string(), "ready".to_string()];
    say_creation_finished(flags, &detail, changed, false, format!("✓ Finished creating: {}", task_label(t.id)));
    Ok(0)
}

/// Report a finished creation, with the unassigned nudge folded in where one is owed
/// ([`unassigned_hint`]). Both of `finish-creating`'s endings come through here — the real transition and
/// the no-op — because the state the hint reads is the same in each: a task nobody is going to pick up says
/// so whether this call is what finished it or the one before was.
///
/// A `--json` caller reads the hint as a key on the task and never as a line on stderr; a person reads the
/// line. One or the other, since a program told the same thing twice has two places to look for it.
fn say_creation_finished(
    flags: &Flags,
    detail: &amenbo_core::view::TaskDetail,
    changed: Vec<String>,
    noop: bool,
    human_line: String,
) {
    let hint = unassigned_hint(flags, detail);
    let mut resource = serde_json::to_value(detail).unwrap();
    if let (Some(h), Some(obj)) = (&hint, resource.as_object_mut()) {
        obj.insert("hint".to_string(), json!(h));
    }
    write_envelope(flags, "task.finish-creating", "task", resource, Some(changed), noop, human_line);
    if let Some(h) = hint.filter(|_| !flags.json) {
        eprintln!("hint: {h}");
    }
}

/// The one thing worth saying as a creation ends: **nobody has this task**.
///
/// The cycle's first step already says to file AI work with `--to me-ai`, and it gets read past. What that
/// leaves is the one state no mailbox shows — a finished task with no assignee matches nobody's
/// `assignee:`, so it sits in the backlog looking filed and is never taken. Guidance did not close that, so
/// it is said here instead, at the hand that just moved, as the next line to type rather than as a question
/// (`AMB-D-558`).
///
/// **Only the AI facet.** A person filing work for whoever picks it up is doing an ordinary thing, and has
/// the board to see it on besides. **And only here, never at `task add`**: a task is still being created
/// there (`AMB-D-554`), so having no assignee yet is not a state to name — it is the middle of writing one.
fn unassigned_hint(flags: &Flags, detail: &amenbo_core::view::TaskDetail) -> Option<String> {
    if flags.actor != Some(ActorKind::Ai) || detail.assignee_kind.is_some() {
        return None;
    }
    Some(format!(
        "担当者が未割当です。{} task assign {} --to me-ai",
        Paths::command_name(),
        amenbo_core::idref::task(detail.id),
    ))
}

fn task_complete(store: &mut Store, flags: &Flags, id: &str, completed: bool) -> Result<i32, CliError> {
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let before = store.task(tid).map_err(CliError::from)?;
    let old = before.map(|t| t.status).unwrap_or_default();
    // "Already in the target state" reads differently in each direction, because the two terminals are not
    // one (`AMB-D-397`): `done` has arrived only when the work was carried out, while `reopen` has arrived
    // whenever the task has not ended at all. A task decided against *has* ended, so reopen is the way back
    // from it — reading `completed` here would answer false and make the command silently do nothing.
    let already_there = if completed { old == TaskStatus::Done } else { !old.is_closed() };
    let action = if completed { "task.done" } else { "task.reopen" };
    if already_there {
        // Idempotent: already in the target state. Report success, as a no-op.
        let detail = store.task_detail(tid).map_err(CliError::from)?;
        write_envelope(flags, action, "task", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("(no change) {}", task_label(tid)));
        return Ok(0);
    }
    // Safety net (`AMB-D-366`): completing a reserved task is the moment not to miss — read the premises pinned on
    // after the reservation *before* the transition retires the in_progress clock they are measured against.
    let pc = premise_change_when(store, tid, completed && old == TaskStatus::InProgress);
    let t = store.set_task_completed(tid, completed, flags.facet()?).map_err(CliError::from)?;
    emit_event(store, flags, tid, activity_log::event::task_status_changed(old.as_str(), t.status.as_str()));
    // Ending the task — carried out or decided against — may have made dependents ready; emit the
    // unblock signal if so.
    if t.status.is_closed() {
        emit_unblocks(store, flags, tid);
    }
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let msg = if completed { "✓ Marked done" } else { "✓ Reopened" };
    let mut resource = serde_json::to_value(&detail).unwrap();
    attach_premise_change(&mut resource, &pc);
    write_envelope(flags, action, "task", resource, Some(vec!["completed".to_string(), "status".to_string()]), false, format!("{msg}: {}", task_label(t.id)));
    warn_premise_change(&pc);
    Ok(0)
}

/// `task status <id> <status>`: set the status explicitly (done keeps `completed` in step). This is the only
/// guard against two people starting the same task: `in_progress` reserves it, `todo` gives it back.
fn task_set_status(store: &mut Store, flags: &Flags, id: &str, status: &str) -> Result<i32, CliError> {
    let new_status = TaskStatus::parse(status).ok_or_else(|| CliError {
        code: "invalid_value",
        message: format!("status '{status}' is invalid (todo / in_progress / done / blocked / rejected)"),
        hint: None,
        exit: 2,
    })?;
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let current = store.task(tid).map_err(CliError::from)?.map(|t| t.status);
    // Setting the same status again is a no-op success — except for `in_progress → in_progress`, which must
    // not short-circuit: doing so would defeat the reservation's compare-and-set and let a second session
    // start a task someone is already on. Let that one fall through to `set_status` and come back as
    // `AlreadyReserved`.
    if current == Some(new_status) && new_status != TaskStatus::InProgress {
        // Idempotent: already at the target status. Report success, as a no-op.
        let detail = store.task_detail(tid).map_err(CliError::from)?;
        write_envelope(flags, "task.status", "task", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("(no change) {}", task_label(tid)));
        return Ok(0);
    }
    let old = current.unwrap_or_default();
    // Safety net (`AMB-D-366`): leaving in_progress to complete or block is the not-to-miss moment; read the
    // premises acquired since the reservation before the transition retires the clock. Handing it back to
    // todo needs no warn — the holder is stepping off anyway.
    let pc = premise_change_when(
        store,
        tid,
        old == TaskStatus::InProgress && (new_status.is_closed() || new_status == TaskStatus::Blocked),
    );
    let t = store.set_task_status(tid, new_status, flags.facet()?).map_err(CliError::from)?;
    emit_event(store, flags, tid, activity_log::event::task_status_changed(old.as_str(), new_status.as_str()));
    // Ending the task — carried out or decided against — may have made dependents ready; emit the
    // unblock signal if so.
    if t.status.is_closed() {
        emit_unblocks(store, flags, tid);
    }
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let mut resource = serde_json::to_value(&detail).unwrap();
    attach_premise_change(&mut resource, &pc);
    write_envelope(flags, "task.status", "task", resource, Some(vec!["status".to_string(), "completed".to_string()]), false, format!("✓ Set status to {}: {}", new_status.as_str(), task_label(t.id)));
    warn_premise_change(&pc);
    Ok(0)
}

/// `task block <id> [--reason]`: set the task to blocked, recording the reason as a comment.
fn task_block(store: &mut Store, flags: &Flags, id: &str, reason: Option<String>) -> Result<i32, CliError> {
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let old = store.task(tid).map_err(CliError::from)?.map(|t| t.status).unwrap_or_default();
    // Safety net (`AMB-D-366`): interrupting a reserved task — read the premises acquired since the reservation
    // before the transition retires the in_progress clock.
    let pc = premise_change_when(store, tid, old == TaskStatus::InProgress);
    let t = store.set_task_status(tid, TaskStatus::Blocked, flags.facet()?).map_err(CliError::from)?;
    if old != TaskStatus::Blocked {
        emit_event(store, flags, tid, activity_log::event::task_status_changed(old.as_str(), "blocked"));
    }
    // Keep the reason as a comment when there is one (under our own facet; the author argument is the trace
    // string for the audit log).
    if let Some(r) = reason.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        store.add_task_comment(tid, flags.facet()?, r).map_err(CliError::from)?;
    }
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let mut resource = serde_json::to_value(&detail).unwrap();
    attach_premise_change(&mut resource, &pc);
    write_envelope(flags, "task.block", "task", resource, Some(vec!["status".to_string()]), false, format!("✓ Set to blocked: {}", task_label(t.id)));
    warn_premise_change(&pc);
    Ok(0)
}

/// `task reject <id> --reason <why>`: end a task that will not be done (`AMB-D-397`). The sibling of
/// `task done` — both are terminals, and the difference is only whether the work was carried out.
///
/// The reason is **required**, and it is why the command exists at all: `task status <id> rejected` can
/// reach the same state, but nothing there asks for the reasoning, which is the part worth keeping when a
/// task is closed unfinished. It lands as a comment rather than a column of its own — the same sugar as
/// `task block --reason` and `decision reject --reason`, so free text keeps its one home on the timeline.
fn task_reject(store: &mut Store, flags: &Flags, id: &str, reason: String) -> Result<i32, CliError> {
    // An empty `--reason` passes clap (it is a value) but not the point of the flag: a rejection with no
    // reasoning is the `done`-borrowing this command was added to end.
    let reason = reason.trim().to_string();
    if reason.is_empty() {
        return Err(CliError {
            code: "invalid_value",
            message: "--reason is empty — say why the task will not be done".to_string(),
            hint: Some("A rejection is kept for its reasoning; pass the text, or `-` to read it from stdin.".to_string()),
            exit: 2,
        });
    }
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let old = store.task(tid).map_err(CliError::from)?.map(|t| t.status).unwrap_or_default();
    if old == TaskStatus::Rejected {
        // Idempotent: already decided against. Report success as a no-op, and do **not** pile the reason
        // on — a re-reject changes nothing, so it has nothing new to explain (`decision reject` likewise).
        let detail = store.task_detail(tid).map_err(CliError::from)?;
        write_envelope(flags, "task.reject", "task", serde_json::to_value(&detail).unwrap(), Some(vec![]), true, format!("(no change) {}", task_label(tid)));
        return Ok(0);
    }
    // Safety net (`AMB-D-366`): ending a reserved task is the moment not to miss — read the premises pinned
    // on after the reservation before the transition retires the in_progress clock they are measured against.
    let pc = premise_change_when(store, tid, old == TaskStatus::InProgress);
    let t = store.set_task_status(tid, TaskStatus::Rejected, flags.facet()?).map_err(CliError::from)?;
    emit_event(store, flags, tid, activity_log::event::task_status_changed(old.as_str(), t.status.as_str()));
    // A blocker decided against is a blocker no longer — dependents may have just become ready.
    emit_unblocks(store, flags, tid);
    store.add_task_comment(tid, flags.facet()?, &reason).map_err(CliError::from)?;
    let detail = store.task_detail(t.id).map_err(CliError::from)?;
    let mut resource = serde_json::to_value(&detail).unwrap();
    attach_premise_change(&mut resource, &pc);
    write_envelope(flags, "task.reject", "task", resource, Some(vec!["status".to_string()]), false, format!("✓ Rejected: {}", task_label(t.id)));
    warn_premise_change(&pc);
    Ok(0)
}
