//! A plugin's **text (non-secret)** config value at one layer (`AMB-D-434` / `AMB-D-601` / `AMB-D-356`).
//!
//! One value per layer and nothing under it: the author's `scope` declaration picks the single layer a
//! plugin lives at, so a row here is the whole answer rather than an override of a machine-wide default. A
//! `secret` field never reaches this table — the config write boundary routes it to
//! [`crate::ops::plugin_secret`], the table an `export` must leave.
//! Unlike `hook_optout` this is a real record, carried by `export`/`backup`.
//!
//! One row per `(project_id, plugin, field_key)`, where `project_id` is the layer — a project's id, or
//! `None` for the device row. The `plugin_config_triple` and `plugin_config_device` UNIQUE indexes are what
//! make [`set`] an idempotent upsert rather than an append at either layer. The value is assumed **already
//! validated** at the config write boundary ([`crate::plugin_config`]) — the single enforcement point for
//! the safe floor (`AMB-D-354`); this layer only stores. Reach is guarded one level up, by the `Store`
//! wrapper, so an AI cannot write a project outside its binding.

use crate::error::Result;
use crate::model::PluginConfigValue;
use crate::ops::{emit_create, emit_update};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Set (`Some`) or clear (`None`) one plugin text field's value at one layer, inside the caller's
/// transaction. Idempotent upsert on the `(project_id, plugin, field_key)` triple: an existing row's value
/// is updated in place (a re-set to the same value is a no-op); a new one is inserted; a clear deletes the
/// row, leaving the field unset. Returns whether anything changed.
pub fn set(
    tx: &WriteTx<'_>,
    project_id: Option<i64>,
    plugin: &str,
    field_key: &str,
    value: Option<&str>,
) -> Result<bool> {
    let existing_id = read::plugin_config_row_id(tx.conn(), project_id, plugin, field_key)?;
    match (existing_id, value) {
        (Some(id), Some(v)) => {
            let before = read::plugin_config_row_by_id(tx.conn(), id)?
                .expect("the row id was just read from the same transaction");
            if before.value == v {
                return Ok(false);
            }
            let after = PluginConfigValue {
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
            let row = PluginConfigValue {
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

/// Delete **every** value this plugin holds, in every project, inside the caller's transaction —
/// the store half of `uninstall` (`AMB-D-357`: nothing is left behind, and a re-install starts clean).
/// Returns how many rows went.
///
/// It crosses projects on purpose, and does so in one pass rather than a walk: a plugin is installed
/// machine-wide (`AMB-D-350`), so its settings are one plugin's residue and not one project's content,
/// and the store is a single device-wide database. A reach-limited erase would be the very thing the
/// decision forbids — an uninstall that leaves other projects' rows behind.
pub fn forget_plugin(tx: &WriteTx<'_>, plugin: &str) -> Result<usize> {
    let ids = read::plugin_config_row_ids(tx.conn(), plugin)?;
    for id in &ids {
        tx.delete_record("plugin_config", *id)?;
    }
    Ok(ids.len())
}

/// Delete every value this plugin holds under a key `declared` does not name, in every project, inside
/// the caller's transaction — the purge an **update** runs once the new build is in place (`AMB-D-456`:
/// bytes do not outlive the declaration that asked for them). Returns how many rows went.
///
/// It is keyed on what is declared **now**, not on a diff of two manifests: the rule is that a value is
/// kept only while something asks for it, so a row nothing declares goes whether its key left in this
/// update or was never declared at all. That makes it idempotent — a second run over the same declaration
/// finds nothing — and self-healing, since a run that could not finish is simply redone by the next one.
///
/// It crosses projects for the reason [`forget_plugin`] does: the declaration is the plugin's, and it is
/// the same declaration in every project.
pub fn forget_undeclared(tx: &WriteTx<'_>, plugin: &str, declared: &[&str]) -> Result<usize> {
    let mut gone = 0;
    for id in read::plugin_config_row_ids(tx.conn(), plugin)? {
        let row = read::plugin_config_row_by_id(tx.conn(), id)?
            .expect("the row id was just read from the same transaction");
        if !declared.contains(&row.field_key.as_str()) {
            tx.delete_record("plugin_config", id)?;
            gone += 1;
        }
    }
    Ok(gone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, with_tx};
    use crate::store_engine::read;

    #[test]
    fn set_then_read_back_the_value() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, Some(p), "slack", "events", Some("push,merge")).unwrap());
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(p), "slack", "events").unwrap().as_deref(),
                Some("push,merge"),
            );
        });
    }

    #[test]
    fn re_setting_the_same_value_is_a_no_op() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, Some(p), "slack", "events", Some("x")).unwrap());
            assert!(!set(tx, Some(p), "slack", "events", Some("x")).unwrap(), "same value changes nothing");
            // A different value does change.
            assert!(set(tx, Some(p), "slack", "events", Some("y")).unwrap());
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(p), "slack", "events").unwrap().as_deref(),
                Some("y"),
            );
        });
    }

    #[test]
    fn upsert_keeps_one_row_per_triple() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), "slack", "events", Some("a")).unwrap();
            set(tx, Some(p), "slack", "events", Some("b")).unwrap();
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
    fn clearing_deletes_the_row_and_leaves_the_field_unset() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), "slack", "events", Some("a")).unwrap();
            assert!(set(tx, Some(p), "slack", "events", None).unwrap(), "clearing an existing value changes state");
            assert_eq!(read::plugin_config_value(tx.conn(), Some(p), "slack", "events").unwrap(), None);
            assert!(!set(tx, Some(p), "slack", "events", None).unwrap(), "clearing an absent value is a no-op");
        });
    }

    #[test]
    fn values_are_one_projects_own() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, Some(a), "slack", "events", Some("for-a")).unwrap();
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(a), "slack", "events").unwrap().as_deref(),
                Some("for-a"),
            );
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(b), "slack", "events").unwrap(),
                None,
                "one project's value does not leak into another",
            );
        });
    }

    /// `forget_plugin` takes every project's rows for that plugin in one pass, and only that plugin's.
    #[test]
    fn forgetting_a_plugin_erases_every_project_and_only_that_plugin() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, Some(a), "slack", "events", Some("for-a")).unwrap();
            set(tx, Some(a), "slack", "channel", Some("#a")).unwrap();
            set(tx, Some(b), "slack", "events", Some("for-b")).unwrap();
            set(tx, Some(b), "worktree", "base", Some("main")).unwrap();

            assert_eq!(forget_plugin(tx, "slack").unwrap(), 3);
            assert_eq!(read::plugin_config_value(tx.conn(), Some(a), "slack", "events").unwrap(), None);
            assert_eq!(read::plugin_config_value(tx.conn(), Some(a), "slack", "channel").unwrap(), None);
            assert_eq!(read::plugin_config_value(tx.conn(), Some(b), "slack", "events").unwrap(), None);
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(b), "worktree", "base").unwrap().as_deref(),
                Some("main"),
                "another plugin's rows are untouched",
            );
        });
    }

    /// The purge an update runs (`AMB-D-456`): a key the new manifest no longer declares loses its value
    /// in every project, and the keys it still declares keep theirs.
    #[test]
    fn forgetting_the_undeclared_keeps_what_the_declaration_still_names() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, Some(a), "slack", "channel", Some("#a")).unwrap();
            set(tx, Some(a), "slack", "legacy", Some("dropped")).unwrap();
            set(tx, Some(b), "slack", "legacy", Some("dropped-too")).unwrap();
            set(tx, Some(b), "worktree", "legacy", Some("kept")).unwrap();

            assert_eq!(forget_undeclared(tx, "slack", &["channel"]).unwrap(), 2);
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(a), "slack", "channel").unwrap().as_deref(),
                Some("#a"),
            );
            assert_eq!(read::plugin_config_value(tx.conn(), Some(a), "slack", "legacy").unwrap(), None);
            assert_eq!(read::plugin_config_value(tx.conn(), Some(b), "slack", "legacy").unwrap(), None);
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(b), "worktree", "legacy").unwrap().as_deref(),
                Some("kept"),
                "another plugin's key of the same name is not this plugin's residue",
            );
        });
    }

    /// Nothing to purge is the ordinary update: a declaration that still names every stored key takes
    /// nothing, however many keys it grew.
    #[test]
    fn forgetting_the_undeclared_takes_nothing_when_every_key_is_still_declared() {
        with_tx(|tx| {
            let p = mk_project(tx, "p");
            set(tx, Some(p), "slack", "channel", Some("#a")).unwrap();
            assert_eq!(forget_undeclared(tx, "slack", &["channel", "added"]).unwrap(), 0);
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(p), "slack", "channel").unwrap().as_deref(),
                Some("#a"),
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

    // ───────────────────────── the device layer (`AMB-D-601`) ─────────────────────────

    /// A device value and a project value under the same key are two settings, and neither read sees the
    /// other's — the same separation two projects already have, one layer up.
    #[test]
    fn a_device_value_and_a_projects_value_do_not_mix() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, None, "carrier", "server", Some("for-the-device")).unwrap();
            set(tx, Some(p), "carrier", "server", Some("for-the-project")).unwrap();

            assert_eq!(
                read::plugin_config_value(tx.conn(), None, "carrier", "server").unwrap().as_deref(),
                Some("for-the-device"),
            );
            assert_eq!(
                read::plugin_config_value(tx.conn(), Some(p), "carrier", "server").unwrap().as_deref(),
                Some("for-the-project"),
            );
        });
    }

    /// The device row upserts rather than appends, which the triple index cannot hold on its own: SQLite
    /// counts NULLs in an index as distinct, so `plugin_config_device` is the constraint here.
    #[test]
    fn upsert_keeps_one_device_row_per_key() {
        with_tx(|tx| {
            set(tx, None, "carrier", "server", Some("a")).unwrap();
            set(tx, None, "carrier", "server", Some("b")).unwrap();
            let n: i64 = tx
                .conn()
                .query_row(
                    "SELECT count(*) FROM plugin_config WHERE project_id IS NULL AND plugin='carrier'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "the device row is unique — an update reuses it, never appends");
            assert_eq!(
                read::plugin_config_value(tx.conn(), None, "carrier", "server").unwrap().as_deref(),
                Some("b"),
            );
        });
    }

    /// Deleting a project takes its own values and leaves the device's: the cascade follows the reference,
    /// and the device row holds none.
    #[test]
    fn deleting_the_project_leaves_the_device_value_standing() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), "carrier", "server", Some("theirs")).unwrap();
            set(tx, None, "carrier", "server", Some("the device's")).unwrap();

            crate::ops::project::delete(tx, p).unwrap();

            assert_eq!(read::plugin_config_value(tx.conn(), Some(p), "carrier", "server").unwrap(), None);
            assert_eq!(
                read::plugin_config_value(tx.conn(), None, "carrier", "server").unwrap().as_deref(),
                Some("the device's"),
                "the device value belongs to no project, so no cascade reaches it",
            );
        });
    }

    #[test]
    fn deleting_the_project_cascades_its_values() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), "slack", "events", Some("a")).unwrap();
            crate::ops::project::delete(tx, p).unwrap();
            let n: i64 = tx
                .conn()
                .query_row("SELECT count(*) FROM plugin_config WHERE project_id=?1", [p], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "the value cascaded with the project (ON DELETE CASCADE)");
        });
    }
}
