//! `hard-erase`: physically removing content from the store (human-gated maintenance).

use amenbo_core::config::Paths;
use amenbo_core::{time, Store};

use crate::cli::*;
use crate::cmd::arg::read_body_input;
use crate::cmd::comment::{resolve_live_decision_comment, resolve_live_task_comment};
use crate::cmd::data::progress_fn;
use crate::cmd::decision::resolve_decision;
use crate::cmd::guard::guard_ai_hard_erase;
use crate::cmd::labels::decision_label;
use crate::output::{confirm, human, print_json, CliError, Flags};

/// `hard-erase --json`: the erase report's own fields, flattened, plus the safety net it stands on — the
/// archive that can put the store back, and the earlier ones that archive superseded.
#[derive(serde::Serialize)]
struct HardEraseJson<'a> {
    #[serde(flatten)]
    erase: &'a amenbo_core::store::HardEraseReport,
    #[serde(flatten)]
    safety: &'a amenbo_core::archive::PreEraseReport,
}

/// `hard-erase`: physically erase content from the truth source (plaintext SQLite) — a comment in full (its
/// attachments' bytes with it), from either comment table, or one accepted decision's body. An ordinary delete leaves the freed
/// pages readable in the file, and editing a body in place does too, so this is the deliberate, gated exception
/// (see the `HardErase` command doc + `store::hard_erase`). Destructive: resolve targets, confirm (unless
/// `--yes`), take a safety backup, then erase + VACUUM. The safety backup still holds the erased content, so
/// we tell the operator to delete it after verifying. Exit 0 on success, 1 on an interactive abort.
pub(crate) fn hard_erase(store: &mut Store, flags: &Flags, sub: HardEraseCmd) -> Result<i32, CliError> {
    use amenbo_core::archive;
    use amenbo_core::store::HardEraseTarget;
    // Human-gated: AI cannot physically destroy store content (E guardrail).
    guard_ai_hard_erase(flags)?;

    // Resolve targets and describe exactly what will be erased (for the confirmation prompt).
    let (targets, what): (Vec<HardEraseTarget>, String) = match sub {
        HardEraseCmd::Comment { ids } => {
            let mut targets = Vec::with_capacity(ids.len());
            for id in &ids {
                targets
                    .push(HardEraseTarget::TaskComment { id: resolve_live_task_comment(store, id)? });
            }
            let what = format!(
                "physically erase {} task comment(s) — and any files attached to them — from the store",
                targets.len()
            );
            (targets, what)
        }
        HardEraseCmd::DecisionComment { ids } => {
            let mut targets = Vec::with_capacity(ids.len());
            for id in &ids {
                targets.push(HardEraseTarget::DecisionComment {
                    id: resolve_live_decision_comment(store, id)?,
                });
            }
            let what = format!(
                "physically erase {} decision comment(s) — and any files attached to them — from the store",
                targets.len()
            );
            (targets, what)
        }
        HardEraseCmd::Decision { id, body, body_file } => {
            let did = resolve_decision(store, &id).map_err(CliError::from)?;
            let new_body = read_body_input(body, body_file)?;
            let what = format!("redact the body of decision {} in the store", decision_label(did));
            (vec![HardEraseTarget::DecisionBody { id: did, new_body }], what)
        }
    };

    // Human gate (machine callers must pass --yes).
    if !confirm(flags, &format!("{what} — this is irreversible"))? {
        return Ok(1);
    }

    // Safety net: a verified backup archive before the destructive step, so a botched erase is recoverable
    // — through the one restore path there is (`amenbo restore`). It is written to an auto-named path next
    // to the store and carries the attachment bytes, which an erase destroys too. It still holds the erased
    // content, so delete it once the erase is verified.
    let Some(source) = archive::enumerate_store() else {
        return Err(CliError {
            code: "backup_error",
            message: "found no store to back up before erasing".to_string(),
            hint: Some(format!("Create or bind a store first (`{} init`).", Paths::command_name())),
            exit: 1,
        });
    };
    let safety_stamp = time::Timestamp::now().0.format("%Y%m%dT%H%M%SZ").to_string();
    let mut progress = progress_fn(flags);
    let safety = archive::pre_erase_backup(&source, &store.paths.base_dir, &safety_stamp, &mut progress)
        .map_err(|e| CliError {
            code: "backup_error", message: e.to_string(), hint: None, exit: 1,
        })?;
    human(flags, format!(
        "↩ Safety backup written to {} ({} attachment(s)) — it still contains the erased content, so delete it once you have verified the erase (`{} restore` puts it back)",
        safety.backup.path, safety.backup.blobs, Paths::command_name()
    ));
    // One rewind point per kind, the newest. Say what went, so a deleted copy is never a silent one.
    if !safety.superseded.is_empty() {
        human(flags, format!(
            "  Removed {} earlier safety backup(s) this one supersedes (each still held the content an earlier erase destroyed).",
            safety.superseded.len()
        ));
    }

    let report = store.hard_erase(&targets).map_err(CliError::from)?;
    if flags.json {
        // The erase report, plus the rewind point it stands on: a machine caller learns the archive it can
        // put the store back from, and the older ones that archive swept (never a silent delete).
        print_json(&HardEraseJson { erase: &report, safety: &safety });
    } else {
        human(flags, format!(
            "✓ Hard-erased {} task comment(s) + {} decision comment(s) + {} decision body(ies); {} row(s) removed and VACUUMed",
            report.task_comments_erased.len(), report.decision_comments_erased.len(),
            report.decisions_redacted.len(), report.rows_removed
        ));
        if report.blobs_reclaimed > 0 {
            human(flags, format!(
                "  {} attached file(s) reclaimed ({} bytes) — nothing else pointed at those bytes",
                report.blobs_reclaimed, report.bytes_reclaimed
            ));
        }
        human(flags, "  Verify the content is gone, then delete the safety backup next to the store.");
    }
    Ok(0)
}
