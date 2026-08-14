//! The AI guardrails: what `--actor ai` is refused, and the sentence that says who to ask.

use amenbo_core::Store;
use amenbo_core::model::ActorKind;

use crate::output::{CliError, Flags};

// These prevent accidents on this device. They assume an honest actor — the facet is self-declared and can
// be spoofed — so they are not a security boundary. actor=human is unconstrained.

/// An AI may not run a project's destructive or hiding ops (project archive/delete). Off by default, and a
/// config setting can allow it. The point is to stop irreversible destruction and the scrambling of a human's
/// structure, so it covers exactly archive (hides from the default view) and delete (destroys). The
/// reversible, constructive and restoring directions (add / update / move / unarchive) are not gated —
/// the same asymmetry as gating delete but allowing add, gating hard-erase but never the way back, and
/// letting an AI delete only tasks it created: the safe direction is always open.
pub(crate) fn guard_ai_project_ops(store: &Store, flags: &Flags) -> Result<(), CliError> {
    if flags.actor == Some(ActorKind::Ai) && !store.config.ai_allow_project_ops {
        return Err(CliError::ai_guardrail(
            "AI cannot archive or delete projects (destructive/hiding project ops) (E guardrail).",
        ));
    }
    Ok(())
}

/// An AI may not hard-erase (physically destroy append-only content). It is an unrecoverable, destructive
/// maintenance op, so it is human-gated — with no config setting to open it: the refusal is unconditional.
pub(crate) fn guard_ai_hard_erase(flags: &Flags) -> Result<(), CliError> {
    if flags.actor == Some(ActorKind::Ai) {
        return Err(CliError::ai_guardrail(
            "AI cannot hard-erase content: physically destroying store content is a human-gated maintenance op (E guardrail).",
        ));
    }
    Ok(())
}

/// An AI may delete only the tasks it created as the AI facet; human-created tasks and legacy rows are
/// refused. The facet is the only notion of actor there is, so "did the AI make this?" is exactly
/// `created_by_kind == Ai`.
pub(crate) fn guard_ai_task_delete(store: &Store, flags: &Flags, task_id: i64) -> Result<(), CliError> {
    if flags.actor != Some(ActorKind::Ai) {
        return Ok(());
    }
    let self_made = store
        .task(task_id)
        .ok()
        .flatten()
        .map(|t| t.created_by_kind == Some(ActorKind::Ai))
        .unwrap_or(false);
    if self_made {
        Ok(())
    } else {
        Err(CliError::ai_guardrail(
            "AI can only delete tasks it created as AI (deleting tasks created by others is not allowed; E guardrail).",
        ))
    }
}
