//! The per-project override of a plugin's **enable gate** (`AMB-D-350`, the upper tier).
//!
//! This is the store-side tier of the two the gate lives in: a row here answers, for one project, over the
//! machine-global gate in `config.json` ([`crate::config::Config::plugin_enabled`]). Absence is the third
//! state — no row means the project inherits the machine answer — so a clear is a delete, not a `false`.
//! Like the sibling [`crate::ops::plugin_config`] and unlike `hook_optout` this is a real record, carried
//! by `export`/`backup`: a restore that dropped it would reopen a gate the user closed in that project.
//!
//! **The gate only, never the consent.** Consent to run a plugin's code is machine-local and stays in
//! `config.json` (`AMB-D-351`); this tier cannot grant it, and the resolution that reads both
//! ([`crate::plugin_trust::effective_enabled`]) is what keeps a row carried onto another device from
//! firing anything there.
//!
//! One row per `(project_id, plugin)` — the `plugin_enable_pair` UNIQUE index is what makes [`set`] an
//! idempotent upsert rather than an append. Reach is guarded one level up, by the `Store` wrapper
//! (`WriteTarget::Project`), so an AI cannot write a project outside its binding.

use crate::error::Result;
use crate::model::PluginEnableOverride;
use crate::ops::{emit_create, emit_update};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Set (`Some`) or clear (`None`) this project's answer for one plugin's gate, inside the caller's
/// transaction. Idempotent upsert on the `(project_id, plugin)` pair: an existing row's answer is updated
/// in place (a re-set to the same answer is a no-op); a new one is inserted; a clear deletes the row so the
/// machine-global gate stands again. Returns whether anything changed.
pub fn set(
    tx: &WriteTx<'_>,
    project_id: i64,
    plugin: &str,
    enabled: Option<bool>,
) -> Result<bool> {
    let existing_id = read::plugin_enable_override_id(tx.conn(), project_id, plugin)?;
    match (existing_id, enabled) {
        (Some(id), Some(on)) => {
            let before = read::plugin_enable_override(tx.conn(), id)?
                .expect("the row id was just read from the same transaction");
            if before.enabled == on {
                return Ok(false);
            }
            let after =
                PluginEnableOverride { enabled: on, updated_at: Timestamp::now(), ..before.clone() };
            emit_update(tx, record::plugin_enable(&before), record::plugin_enable(&after))?;
            Ok(true)
        }
        (Some(id), None) => {
            tx.delete_record("plugin_enable", id)?;
            Ok(true)
        }
        (None, Some(on)) => {
            let now = Timestamp::now();
            let row = PluginEnableOverride {
                id: read::next_id(tx.conn(), "plugin_enable")?,
                project_id,
                plugin: plugin.to_string(),
                enabled: on,
                created_at: now,
                updated_at: now,
            };
            emit_create(tx, record::plugin_enable(&row))?;
            Ok(true)
        }
        (None, None) => Ok(false),
    }
}

/// Delete **every** gate override this plugin holds, in every project, inside the caller's transaction —
/// the store half of `uninstall` beside [`crate::ops::plugin_config::forget_plugin`] (`AMB-D-357`: nothing
/// is left behind, and a re-install starts clean). Returns how many rows went.
///
/// It crosses projects on purpose, for the reason its config twin does: a plugin is installed machine-wide
/// (`AMB-D-350`), so its gate answers are one plugin's residue and not one project's content.
pub fn forget_plugin(tx: &WriteTx<'_>, plugin: &str) -> Result<usize> {
    let ids = read::plugin_enable_override_ids(tx.conn(), plugin)?;
    for id in &ids {
        tx.delete_record("plugin_enable", *id)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, with_tx};
    use crate::store_engine::read;

    #[test]
    fn set_then_read_back_the_answer() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, p, "slack", Some(true)).unwrap());
            assert_eq!(read::plugin_enable_value(tx.conn(), p, "slack").unwrap(), Some(true));
        });
    }

    /// Both answers are storable: the tier must be able to say "off here" over an open machine gate as
    /// well as "on here" over a closed one.
    #[test]
    fn the_override_stores_either_answer() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", Some(false)).unwrap();
            assert_eq!(read::plugin_enable_value(tx.conn(), p, "slack").unwrap(), Some(false));
            set(tx, p, "slack", Some(true)).unwrap();
            assert_eq!(read::plugin_enable_value(tx.conn(), p, "slack").unwrap(), Some(true));
        });
    }

    #[test]
    fn re_setting_the_same_answer_is_a_no_op() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, p, "slack", Some(true)).unwrap());
            assert!(!set(tx, p, "slack", Some(true)).unwrap(), "the same answer changes nothing");
            assert!(set(tx, p, "slack", Some(false)).unwrap(), "the other answer does");
        });
    }

    #[test]
    fn upsert_keeps_one_row_per_pair() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", Some(true)).unwrap();
            set(tx, p, "slack", Some(false)).unwrap();
            let n: i64 = tx
                .conn()
                .query_row(
                    "SELECT count(*) FROM plugin_enable WHERE project_id=?1 AND plugin='slack'",
                    [p],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "the pair is unique — an update reuses the row, never appends");
        });
    }

    /// Clearing is a delete, not a stored `false`: the project goes back to inheriting the machine answer,
    /// which is a different state from "off here".
    #[test]
    fn clearing_deletes_the_row_so_the_machine_answer_stands() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", Some(false)).unwrap();
            assert!(set(tx, p, "slack", None).unwrap(), "clearing an existing override changes state");
            assert_eq!(read::plugin_enable_value(tx.conn(), p, "slack").unwrap(), None);
            assert!(!set(tx, p, "slack", None).unwrap(), "clearing an absent override is a no-op");
        });
    }

    #[test]
    fn overrides_are_scoped_per_project() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, a, "slack", Some(true)).unwrap();
            assert_eq!(read::plugin_enable_value(tx.conn(), a, "slack").unwrap(), Some(true));
            assert_eq!(
                read::plugin_enable_value(tx.conn(), b, "slack").unwrap(),
                None,
                "one project's answer does not leak into another",
            );
        });
    }

    /// `forget_plugin` takes every project's rows for that plugin in one pass, and only that plugin's.
    #[test]
    fn forgetting_a_plugin_erases_every_project_and_only_that_plugin() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, a, "slack", Some(true)).unwrap();
            set(tx, b, "slack", Some(false)).unwrap();
            set(tx, b, "worktree", Some(true)).unwrap();

            assert_eq!(forget_plugin(tx, "slack").unwrap(), 2);
            assert_eq!(read::plugin_enable_value(tx.conn(), a, "slack").unwrap(), None);
            assert_eq!(read::plugin_enable_value(tx.conn(), b, "slack").unwrap(), None);
            assert_eq!(
                read::plugin_enable_value(tx.conn(), b, "worktree").unwrap(),
                Some(true),
                "another plugin's rows are untouched",
            );
        });
    }

    #[test]
    fn forgetting_a_plugin_with_no_rows_is_a_no_op() {
        with_tx(|tx| {
            assert_eq!(forget_plugin(tx, "never-enabled").unwrap(), 0);
        });
    }

    #[test]
    fn deleting_the_project_cascades_its_overrides() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", Some(true)).unwrap();
            crate::ops::project::delete(tx, p).unwrap();
            let n: i64 = tx
                .conn()
                .query_row("SELECT count(*) FROM plugin_enable WHERE project_id=?1", [p], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "the override cascaded with the project (ON DELETE CASCADE)");
        });
    }
}
