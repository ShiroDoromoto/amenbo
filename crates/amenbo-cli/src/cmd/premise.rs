//! What a write says about the premises it moved — the decisions a record stands on, and the
//! readiness a change to them shifted.

use serde_json::json;

use amenbo_core::Store;
use amenbo_core::config::Paths;
use amenbo_core::model::TaskStatus;

use crate::cmd::decision::decision_ref_name;
use crate::cmd::labels::{decision_label, task_label};
use crate::output::{human, Flags};

/// Warn the changer when a premise (a blocker, or a linked decision) is newly placed on a task that is
/// already reserved (`in_progress`). Such a task silently drops to `ready:no` — its reservation is not
/// revoked, and its holder gets no interrupt, so they only notice on their next command (`AMB-D-366`, the
/// changer side; surfacing it to the holder is a separate task). This does not forbid the edge, only breaks
/// the silence. `todo` / `blocked` say nothing; a `done` target is reopen's business, not a premise change,
/// so it is left alone here. A failure to read the status never fails the caller — like [`emit_event`](crate::cmd::outbox::emit_event), it
/// warns and moves on.
pub(crate) fn warn_if_premise_added_to_reserved(store: &Store, id: i64, what: &str) {
    match store.task(id) {
        Ok(Some(t)) if t.status == TaskStatus::InProgress => eprintln!(
            "⚠ {task} is reserved (in progress) — {what}. Its holder is not notified now; they will see it on their next amenbo command.",
            task = task_label(id),
        ),
        Ok(_) => {}
        Err(e) => eprintln!("warning: could not check whether {} is reserved: {e}", task_label(id)),
    }
}

/// Warn the changer when unsettling a decision takes the ground out from under a task that is already
/// reserved (`in_progress`). It is the other direction of the same silence
/// [`warn_if_premise_added_to_reserved`] breaks: there a premise is newly placed on a running task, here a
/// premise the task already rests on stops being settled (`AMB-D-373`, the changer side). Either way the
/// task drops to `ready:no` without losing its reservation and its holder gets no interrupt.
///
/// Two acts reach this, and `act` names which one is speaking: a `reopen` (the decision goes back to
/// proposed) and a `supersede` (it stays accepted but stops being current). Both leave the premise
/// unsettled, which is what `ready` reads — so both are made audible, and neither is forbidden. An
/// idempotent one settles nothing anew and so says nothing, which is why every caller warns only when its
/// write reported a change. `detail` is the **unsettled** decision's card — the old side of a supersede,
/// not the new one — since those are the tasks whose ground moved.
pub(crate) fn warn_if_unsettled_under_reserved(did: i64, detail: &amenbo_core::view::DecisionDetail, act: &str) {
    for t in detail.linked_tasks.iter().filter(|t| t.status == TaskStatus::InProgress) {
        eprintln!(
            "⚠ {task} is reserved (in progress) and rests on {decision} — {act} leaves that premise unsettled. Its holder is not notified now; they will see it on their next amenbo command.",
            task = task_label(t.id),
            decision = decision_label(did),
        );
    }
}

/// The blast radius: the decisions standing on this one (the reverse lookup of all three edge kinds, one hop
/// only). It exists to name the decisions that want revisiting when this one is superseded, rejected or
/// deleted; it never blocks the operation. Currency is not cascaded — the non-transitive rule stands, and a
/// longer chain can be walked but is never followed automatically. All three edge kinds count because
/// `supersedes` and `amends` imply `builds_on`: whatever corrects a decision necessarily stands on it. If the
/// detail cannot be read (the decision is already deleted, say), the result is empty — the suggestion simply
/// disappears, and the operation goes ahead.
pub(crate) fn standing_on(store: &Store, id: i64) -> Vec<amenbo_core::view::DecisionRef> {
    let Ok(d) = store.decision_detail(id) else {
        return Vec::new();
    };
    let mut out = d.superseded_by;
    out.extend(d.amended_by);
    out.extend(d.built_on_by);
    out
}

/// Carry the blast radius to a machine reader (`--json`): add a `revisit` field to the resource that was
/// operated on. Nothing is added when it is empty — an operation on a decision nothing stands on should not
/// be dressed up with "0 to revisit".
pub(crate) fn attach_revisit(resource: &mut serde_json::Value, standing: &[amenbo_core::view::DecisionRef]) {
    if !standing.is_empty() {
        resource["revisit"] = serde_json::to_value(standing).unwrap_or_else(|_| json!([]));
    }
}

/// Show the blast radius to a human, after the success line. A suggestion only — nothing is stopped here.
pub(crate) fn note_revisit(flags: &Flags, target: i64, standing: &[amenbo_core::view::DecisionRef]) {
    if standing.is_empty() {
        return;
    }
    human(flags, format!("note: these decisions stand on {} — revisit them:", decision_label(target)));
    for s in standing {
        human(flags, format!("  {} {}", decision_label(s.id), decision_ref_name(&s.name)));
    }
}

/// The holder-side surface of `AMB-D-366`: premises a task acquired **after it was reserved** — a blocker or
/// an unsettled decision pinned on since `in_progress` began, silently dropping `ready`. Read-only; the
/// reaction is the caller's (a quiet note on `task show`, a firm warn at completion). Only the reservation
/// holder is at risk, so callers gate on `status == in_progress`. A read error yields "nothing changed" —
/// this is additive context, never a reason to fail the command.
pub(crate) fn premise_change(store: &Store, tid: i64) -> amenbo_core::view::PremiseChange {
    store.premise_change_since(tid).unwrap_or_else(|_| no_premise_change())
}

/// The empty change — what a site reports when it did not look, or could not.
fn no_premise_change() -> amenbo_core::view::PremiseChange {
    amenbo_core::view::PremiseChange {
        added_blockers: Vec::new(),
        added_decisions: Vec::new(),
        reopened_decisions: Vec::new(),
    }
}

/// `premise_change` when `applies`, an empty change otherwise — so a safety-net site reads the premises only
/// on the transition that matters (leaving `in_progress`) and skips the query on every other status change.
pub(crate) fn premise_change_when(store: &Store, tid: i64, applies: bool) -> amenbo_core::view::PremiseChange {
    if applies {
        premise_change(store, tid)
    } else {
        no_premise_change()
    }
}

/// The premise-change lines, one per added premise, shared by the quiet and the firm surface.
pub(crate) fn premise_change_lines(pc: &amenbo_core::view::PremiseChange) -> Vec<String> {
    let mut out = Vec::new();
    for b in &pc.added_blockers {
        out.push(format!("  blocker {} {}", task_label(b.id), b.name));
    }
    for d in &pc.added_decisions {
        out.push(format!("  decision {} {} (not settled)", decision_label(d.id), decision_ref_name(&d.name)));
    }
    // The reopen axis (`AMB-D-373`): the link is not new, the decision's settlement is what went away.
    for d in &pc.reopened_decisions {
        out.push(format!("  decision {} {} (no longer settled)", decision_label(d.id), decision_ref_name(&d.name)));
    }
    out
}

/// The **firm** surface — the safety net that must not be missed: on a status change out of `in_progress`
/// (completing, blocking), warn that the reservation's premises shifted underneath. On stderr, so it reaches
/// both a human and a `--json` caller without touching stdout; `attach_premise_change` folds the same fact
/// into the JSON envelope. It never blocks the transition — `D-366` surfaces the change, it does not forbid
/// finishing (the holder may still ship the part that stands on its own).
pub(crate) fn warn_premise_change(pc: &amenbo_core::view::PremiseChange) {
    if !pc.any() {
        return;
    }
    eprintln!("⚠ Premises changed after you reserved this task — readiness was silently withdrawn (AMB-D-366):");
    for line in premise_change_lines(pc) {
        eprintln!("{line}");
    }
    eprintln!("  Finish only the part that stands on its own, or hand it back with `{} task status <id> todo`.", Paths::command_name());
}

/// Fold the premise change into a write command's JSON resource, so a `--json` caller sees it structurally
/// (not only as a stderr line). Absent when nothing changed, so the key appears exactly when it matters.
pub(crate) fn attach_premise_change(resource: &mut serde_json::Value, pc: &amenbo_core::view::PremiseChange) {
    if pc.any() {
        if let Some(obj) = resource.as_object_mut() {
            obj.insert("premise_change".to_string(), serde_json::to_value(pc).unwrap_or(json!(null)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The premise-change fold (`AMB-D-366`): the `premise_change` key appears in a write's JSON envelope
    /// exactly when a premise shifted, so a `--json` caller reads it structurally and an unchanged
    /// reservation carries no noise key.
    #[test]
    fn attach_premise_change_only_adds_the_key_when_something_changed() {
        use amenbo_core::view::{PremiseChange, TaskRef};

        // Empty change: the key is absent.
        let mut v = json!({ "id": 1 });
        attach_premise_change(&mut v, &no_premise_change());
        assert!(v.get("premise_change").is_none());

        // A pinned-on blocker: the key carries it.
        let pc = PremiseChange {
            added_blockers: vec![TaskRef { id: 7, name: "後付け".to_string() }],
            ..no_premise_change()
        };
        attach_premise_change(&mut v, &pc);
        assert_eq!(v["premise_change"]["added_blockers"][0]["id"], 7);
    }
}
