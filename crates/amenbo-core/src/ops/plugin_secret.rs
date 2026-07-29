//! A plugin's **secret** config value in one project (`AMB-D-434`).
//!
//! The twin of the text sibling beside it ([`crate::ops::plugin_config`]) — same address, same
//! upsert-and-clear shape, same `WriteTarget::Project` reach guard one level up. What makes it a layer of
//! its own is the table underneath: a secret lives in `plugin_secret`, and a table is what
//! `export` can be told to walk past once and for all. A row-by-row "is this field secret" test would put
//! the judgement on every path that reads config, including the ones written later by someone who did not
//! know to ask.
//!
//! `backup`/`restore` carry these rows — a snapshot of the whole file — because that road leads back to the
//! same person's machine, and dropping the secrets there would mean typing every credential in again after
//! a restore. An `export` must leave them: that road ends in somebody else's hands.
//!
//! amenbo does not judge what is secret: the author declares it field by field
//! ([`crate::plugin_manifest::ConfigField::secret`]) and the config write boundary
//! ([`crate::plugin_config`]) routes by that flag alone.

use crate::error::Result;
use crate::model::PluginSecret;
use crate::ops::{emit_create, emit_update};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Set (`Some`) or clear (`None`) one plugin secret field's value in one project, inside the caller's
/// transaction. Idempotent upsert on the `(project_id, plugin, field_key)` triple, exactly as its text
/// sibling is. Returns whether anything changed.
pub fn set(
    tx: &WriteTx<'_>,
    project_id: i64,
    plugin: &str,
    field_key: &str,
    value: Option<&str>,
) -> Result<bool> {
    let existing_id = read::plugin_secret_row_id(tx.conn(), project_id, plugin, field_key)?;
    match (existing_id, value) {
        (Some(id), Some(v)) => {
            let before = read::plugin_secret_row_by_id(tx.conn(), id)?
                .expect("the row id was just read from the same transaction");
            if before.value == v {
                return Ok(false);
            }
            let after = PluginSecret {
                value: v.to_string(),
                updated_at: Timestamp::now(),
                ..before.clone()
            };
            emit_update(tx, record::plugin_secret(&before), record::plugin_secret(&after))?;
            Ok(true)
        }
        (Some(id), None) => {
            tx.delete_record("plugin_secret", id)?;
            Ok(true)
        }
        (None, Some(v)) => {
            let now = Timestamp::now();
            let row = PluginSecret {
                id: read::next_id(tx.conn(), "plugin_secret")?,
                project_id,
                plugin: plugin.to_string(),
                field_key: field_key.to_string(),
                value: v.to_string(),
                created_at: now,
                updated_at: now,
            };
            emit_create(tx, record::plugin_secret(&row))?;
            Ok(true)
        }
        (None, None) => Ok(false),
    }
}

/// Delete **every** secret this plugin holds, in every project, inside the caller's transaction — the
/// purge `uninstall` performs unconditionally (`AMB-D-357`: a secret is the one thing that must never
/// survive a removal). Returns how many rows went. It crosses projects for the reason its text sibling
/// does: what goes is one plugin's residue, not one project's content.
pub fn forget_plugin(tx: &WriteTx<'_>, plugin: &str) -> Result<usize> {
    let ids = read::plugin_secret_row_ids(tx.conn(), plugin)?;
    for id in &ids {
        tx.delete_record("plugin_secret", *id)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, with_tx};
    use crate::store_engine::read;

    #[test]
    fn set_then_read_back_the_secret() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, p, "slack", "token", Some("s3cret")).unwrap());
            assert_eq!(
                read::plugin_secret_value(tx.conn(), p, "slack", "token").unwrap().as_deref(),
                Some("s3cret"),
            );
        });
    }

    /// The two tables share an address but not a row: a text value and a secret under the same
    /// `(project, plugin, key)` are two different settings, and neither read sees the other's.
    #[test]
    fn a_secret_and_a_text_value_under_the_same_key_do_not_collide() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            crate::ops::plugin_config::set(tx, p, "slack", "token", Some("plain")).unwrap();
            set(tx, p, "slack", "token", Some("s3cret")).unwrap();

            assert_eq!(
                read::plugin_config_value(tx.conn(), p, "slack", "token").unwrap().as_deref(),
                Some("plain"),
            );
            assert_eq!(
                read::plugin_secret_value(tx.conn(), p, "slack", "token").unwrap().as_deref(),
                Some("s3cret"),
            );
        });
    }

    #[test]
    fn clearing_deletes_the_row_and_leaves_the_field_unset() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", "token", Some("s3cret")).unwrap();
            assert!(set(tx, p, "slack", "token", None).unwrap());
            assert_eq!(read::plugin_secret_value(tx.conn(), p, "slack", "token").unwrap(), None);
            assert!(!set(tx, p, "slack", "token", None).unwrap(), "clearing an absent secret is a no-op");
        });
    }

    #[test]
    fn upsert_keeps_one_row_per_triple() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", "token", Some("a")).unwrap();
            set(tx, p, "slack", "token", Some("b")).unwrap();
            let n: i64 = tx
                .conn()
                .query_row(
                    "SELECT count(*) FROM plugin_secret WHERE project_id=?1 AND plugin='slack' AND field_key='token'",
                    [p],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "the triple is unique — an update reuses the row, never appends");
        });
    }

    /// The purge an uninstall runs: every project's secrets for that plugin, and only that plugin's.
    #[test]
    fn forgetting_a_plugin_erases_every_project_and_only_that_plugin() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, a, "slack", "token", Some("for-a")).unwrap();
            set(tx, b, "slack", "token", Some("for-b")).unwrap();
            set(tx, b, "worktree", "token", Some("kept")).unwrap();

            assert_eq!(forget_plugin(tx, "slack").unwrap(), 2);
            assert_eq!(read::plugin_secret_value(tx.conn(), a, "slack", "token").unwrap(), None);
            assert_eq!(read::plugin_secret_value(tx.conn(), b, "slack", "token").unwrap(), None);
            assert_eq!(
                read::plugin_secret_value(tx.conn(), b, "worktree", "token").unwrap().as_deref(),
                Some("kept"),
                "another plugin's secret is untouched",
            );
        });
    }

    #[test]
    fn deleting_the_project_cascades_its_secrets() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", "token", Some("s3cret")).unwrap();
            crate::ops::project::delete(tx, p).unwrap();
            let n: i64 = tx
                .conn()
                .query_row("SELECT count(*) FROM plugin_secret WHERE project_id=?1", [p], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "the secret cascaded with the project (ON DELETE CASCADE)");
        });
    }
}
