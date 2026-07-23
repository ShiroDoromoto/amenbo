//! The per-project override of a plugin's **text (non-secret)** config value (`AMB-D-356` / `AMB-D-350`).
//!
//! This is the store-side, upper tier of the two a text config field lives in: a row here overrides, for
//! one project, the machine default the field carries in `config.json`
//! ([`crate::config::Config::plugin_config`]). A `secret` field never reaches this layer — it is routed to
//! the user-area secret file ([`crate::plugin_secret`]) by the config write boundary, off the store and off
//! every backup. Unlike `hook_optout` this is a real record, carried by `export`/`backup`.
//!
//! One row per `(project_id, plugin, field_key)` — the `plugin_config_triple` UNIQUE index is what makes
//! [`set`] an idempotent upsert rather than an append. The value is assumed **already validated** at the
//! config write boundary ([`crate::plugin_config`]) — the single enforcement point for the safe floor
//! (`AMB-D-354`); this layer only stores. Reach is guarded one level up, by the `Store` wrapper
//! (`WriteTarget::Project`), so an AI cannot write a project outside its binding.

use crate::error::Result;
use crate::model::PluginConfigOverride;
use crate::ops::{emit_create, emit_update};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Set (`Some`) or clear (`None`) the per-project override of one plugin text field, inside the caller's
/// transaction. Idempotent upsert on the `(project_id, plugin, field_key)` triple: an existing row's value
/// is updated in place (a re-set to the same value is a no-op); a new one is inserted; a clear deletes the
/// row so the machine default stands again. Returns whether anything changed.
pub fn set(
    tx: &WriteTx<'_>,
    project_id: i64,
    plugin: &str,
    field_key: &str,
    value: Option<&str>,
) -> Result<bool> {
    let existing_id = read::plugin_config_override_id(tx.conn(), project_id, plugin, field_key)?;
    match (existing_id, value) {
        (Some(id), Some(v)) => {
            let before = read::plugin_config_override(tx.conn(), id)?
                .expect("the row id was just read from the same transaction");
            if before.value == v {
                return Ok(false);
            }
            let after = PluginConfigOverride {
                value: v.to_string(),
                updated_at: Timestamp::now(),
                ..before.clone()
            };
            emit_update(tx, record::plugin_config(&before), record::plugin_config(&after))?;
            Ok(true)
        }
        (Some(id), None) => {
            tx.delete_record("plugin_config", id)?;
            Ok(true)
        }
        (None, Some(v)) => {
            let now = Timestamp::now();
            let row = PluginConfigOverride {
                id: read::next_id(tx.conn(), "plugin_config")?,
                project_id,
                plugin: plugin.to_string(),
                field_key: field_key.to_string(),
                value: v.to_string(),
                created_at: now,
                updated_at: now,
            };
            emit_create(tx, record::plugin_config(&row))?;
            Ok(true)
        }
        (None, None) => Ok(false),
    }
}

/// Delete **every** override this plugin holds, in every project, inside the caller's transaction —
/// the store half of `uninstall` (`AMB-D-357`: nothing is left behind, and a re-install starts clean).
/// Returns how many rows went.
///
/// It crosses projects on purpose, and does so in one pass rather than a walk: a plugin is installed
/// machine-wide (`AMB-D-350`), so its settings are one plugin's residue and not one project's content,
/// and the store is a single device-wide database. A reach-limited erase would be the very thing the
/// decision forbids — an uninstall that leaves other projects' rows behind.
pub fn forget_plugin(tx: &WriteTx<'_>, plugin: &str) -> Result<usize> {
    let ids = read::plugin_config_override_ids(tx.conn(), plugin)?;
    for id in &ids {
        tx.delete_record("plugin_config", *id)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, with_tx};
    use crate::store_engine::read;

    #[test]
    fn set_then_read_back_the_override() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, p, "slack", "events", Some("push,merge")).unwrap());
            assert_eq!(
                read::plugin_config_value(tx.conn(), p, "slack", "events").unwrap().as_deref(),
                Some("push,merge"),
            );
        });
    }

    #[test]
    fn re_setting_the_same_value_is_a_no_op() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, p, "slack", "events", Some("x")).unwrap());
            assert!(!set(tx, p, "slack", "events", Some("x")).unwrap(), "same value changes nothing");
            // A different value does change.
            assert!(set(tx, p, "slack", "events", Some("y")).unwrap());
            assert_eq!(
                read::plugin_config_value(tx.conn(), p, "slack", "events").unwrap().as_deref(),
                Some("y"),
            );
        });
    }

    #[test]
    fn upsert_keeps_one_row_per_triple() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", "events", Some("a")).unwrap();
            set(tx, p, "slack", "events", Some("b")).unwrap();
            let n: i64 = tx
                .conn()
                .query_row(
                    "SELECT count(*) FROM plugin_config WHERE project_id=?1 AND plugin='slack' AND field_key='events'",
                    [p],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "the triple is unique — an update reuses the row, never appends");
        });
    }

    #[test]
    fn clearing_deletes_the_override_so_the_default_stands() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", "events", Some("a")).unwrap();
            assert!(set(tx, p, "slack", "events", None).unwrap(), "clearing an existing override changes state");
            assert_eq!(read::plugin_config_value(tx.conn(), p, "slack", "events").unwrap(), None);
            assert!(!set(tx, p, "slack", "events", None).unwrap(), "clearing an absent override is a no-op");
        });
    }

    #[test]
    fn overrides_are_scoped_per_project() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, a, "slack", "events", Some("for-a")).unwrap();
            assert_eq!(
                read::plugin_config_value(tx.conn(), a, "slack", "events").unwrap().as_deref(),
                Some("for-a"),
            );
            assert_eq!(
                read::plugin_config_value(tx.conn(), b, "slack", "events").unwrap(),
                None,
                "one project's override does not leak into another",
            );
        });
    }

    /// `forget_plugin` takes every project's rows for that plugin in one pass, and only that plugin's.
    #[test]
    fn forgetting_a_plugin_erases_every_project_and_only_that_plugin() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, a, "slack", "events", Some("for-a")).unwrap();
            set(tx, a, "slack", "channel", Some("#a")).unwrap();
            set(tx, b, "slack", "events", Some("for-b")).unwrap();
            set(tx, b, "worktree", "base", Some("main")).unwrap();

            assert_eq!(forget_plugin(tx, "slack").unwrap(), 3);
            assert_eq!(read::plugin_config_value(tx.conn(), a, "slack", "events").unwrap(), None);
            assert_eq!(read::plugin_config_value(tx.conn(), a, "slack", "channel").unwrap(), None);
            assert_eq!(read::plugin_config_value(tx.conn(), b, "slack", "events").unwrap(), None);
            assert_eq!(
                read::plugin_config_value(tx.conn(), b, "worktree", "base").unwrap().as_deref(),
                Some("main"),
                "another plugin's rows are untouched",
            );
        });
    }

    /// Forgetting a plugin that has no rows is a no-op — an uninstall does not need a settings row to
    /// exist before it can clean up.
    #[test]
    fn forgetting_a_plugin_with_no_rows_is_a_no_op() {
        with_tx(|tx| {
            assert_eq!(forget_plugin(tx, "never-configured").unwrap(), 0);
        });
    }

    #[test]
    fn deleting_the_project_cascades_its_overrides() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", "events", Some("a")).unwrap();
            crate::ops::project::delete(tx, p).unwrap();
            let n: i64 = tx
                .conn()
                .query_row("SELECT count(*) FROM plugin_config WHERE project_id=?1", [p], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "the override cascaded with the project (ON DELETE CASCADE)");
        });
    }
}
