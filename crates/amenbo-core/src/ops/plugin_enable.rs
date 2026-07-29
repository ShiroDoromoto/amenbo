//! The projects a plugin is **enabled in** (`AMB-D-434`).
//!
//! A set, not a pair of answers: a plugin has one switch and it is a project's, with no tier beneath it to
//! inherit from or veto. The row **is** the answer — present means "on here", absent means off — which is
//! why [`set`] takes a plain `bool` and turning a plugin off deletes rather than stores a `false`.
//! Like the sibling [`crate::ops::plugin_config`] this is a real record, carried by `export`/`backup`: a
//! restore that dropped it would silently switch a project's plugins off.
//!
//! **The row is the whole of it.** Turning a plugin on is itself the permission to run its code
//! (`AMB-D-434`), so nothing else has to travel beside these rows for them to mean the same thing wherever
//! they land.
//!
//! One row per `(project_id, plugin)` — the `plugin_enable_pair` UNIQUE index is what makes [`set`]
//! idempotent rather than an append. Reach is guarded one level up, by the `Store` wrapper
//! (`WriteTarget::Project`), so an AI cannot write a project outside its binding.

use crate::error::Result;
use crate::model::PluginEnabledProject;
use crate::ops::emit_create;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Turn one plugin on (`true`) or off (`false`) in one project, inside the caller's transaction — write
/// the row, or delete it (`AMB-D-434`: the row is the answer, so there is no second state to update in
/// place). Idempotent on the `(project_id, plugin)` pair in both directions; returns whether anything
/// changed.
pub fn set(tx: &WriteTx<'_>, project_id: i64, plugin: &str, on: bool) -> Result<bool> {
    let existing_id = read::plugin_enable_row_id(tx.conn(), project_id, plugin)?;
    match (existing_id, on) {
        (Some(_), true) | (None, false) => Ok(false),
        (Some(id), false) => {
            tx.delete_record("plugin_enable", id)?;
            Ok(true)
        }
        (None, true) => {
            let now = Timestamp::now();
            let row = PluginEnabledProject {
                id: read::next_id(tx.conn(), "plugin_enable")?,
                project_id,
                plugin: plugin.to_string(),
                created_at: now,
                updated_at: now,
            };
            emit_create(tx, record::plugin_enable(&row))?;
            Ok(true)
        }
    }
}

/// Delete **every** row this plugin holds, in every project, inside the caller's transaction —
/// the store half of `uninstall` beside [`crate::ops::plugin_config::forget_plugin`] (`AMB-D-357`: nothing
/// is left behind, and a re-install starts clean). Returns how many rows went.
///
/// It crosses projects on purpose, for the reason its config twin does: a plugin is installed machine-wide
/// (`AMB-D-350`), so its gate rows are one plugin's residue and not one project's content.
pub fn forget_plugin(tx: &WriteTx<'_>, plugin: &str) -> Result<usize> {
    let ids = read::plugin_enable_row_ids(tx.conn(), plugin)?;
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
    fn a_row_is_the_answer() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(!read::plugin_enabled_in_project(tx.conn(), p, "slack").unwrap());
            assert!(set(tx, p, "slack", true).unwrap());
            assert!(read::plugin_enabled_in_project(tx.conn(), p, "slack").unwrap());
        });
    }

    /// Turning it off deletes the row rather than storing a `false`: there is no tier under it, so "off
    /// here" and "nothing here" are the same state (`AMB-D-434`).
    #[test]
    fn turning_it_off_deletes_the_row() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", true).unwrap();
            assert!(set(tx, p, "slack", false).unwrap());
            assert!(!read::plugin_enabled_in_project(tx.conn(), p, "slack").unwrap());
            let n: i64 = tx
                .conn()
                .query_row(
                    "SELECT count(*) FROM plugin_enable WHERE project_id=?1 AND plugin='slack'",
                    [p],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 0, "the row is gone, not rewritten");
        });
    }

    #[test]
    fn setting_what_is_already_set_changes_nothing() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, p, "slack", true).unwrap());
            assert!(!set(tx, p, "slack", true).unwrap(), "already on");
            assert!(set(tx, p, "slack", false).unwrap());
            assert!(!set(tx, p, "slack", false).unwrap(), "already off");
        });
    }

    #[test]
    fn one_row_per_pair_however_often_it_is_toggled() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            for on in [true, false, true] {
                set(tx, p, "slack", on).unwrap();
            }
            let n: i64 = tx
                .conn()
                .query_row(
                    "SELECT count(*) FROM plugin_enable WHERE project_id=?1 AND plugin='slack'",
                    [p],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "the pair is unique — a re-enable never appends");
        });
    }

    #[test]
    fn projects_answer_for_themselves() {
        with_tx(|tx| {
            let a = mk_project(tx, "a");
            let b = mk_project(tx, "b");
            set(tx, a, "slack", true).unwrap();
            assert!(read::plugin_enabled_in_project(tx.conn(), a, "slack").unwrap());
            assert!(
                !read::plugin_enabled_in_project(tx.conn(), b, "slack").unwrap(),
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
            set(tx, a, "slack", true).unwrap();
            set(tx, b, "slack", true).unwrap();
            set(tx, b, "worktree", true).unwrap();

            assert_eq!(forget_plugin(tx, "slack").unwrap(), 2);
            assert!(!read::plugin_enabled_in_project(tx.conn(), a, "slack").unwrap());
            assert!(!read::plugin_enabled_in_project(tx.conn(), b, "slack").unwrap());
            assert!(
                read::plugin_enabled_in_project(tx.conn(), b, "worktree").unwrap(),
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
    fn deleting_the_project_cascades_its_rows() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, p, "slack", true).unwrap();
            crate::ops::project::delete(tx, p).unwrap();
            let n: i64 = tx
                .conn()
                .query_row("SELECT count(*) FROM plugin_enable WHERE project_id=?1", [p], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "the row cascaded with the project (ON DELETE CASCADE)");
        });
    }
}
