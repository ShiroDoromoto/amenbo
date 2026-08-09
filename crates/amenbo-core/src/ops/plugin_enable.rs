//! Where a plugin is **enabled** (`AMB-D-434` / `AMB-D-601`).
//!
//! A set, not a pair of answers: a plugin has one switch and it sits at the layer its author declared, with
//! no tier beneath it to inherit from or veto. The row **is** the answer — present means "on here", absent
//! means off — which is why [`set`] takes a plain `bool` and turning a plugin off deletes rather than stores
//! a `false`. Like the sibling [`crate::ops::plugin_config`] this is a real record, carried by
//! `export`/`backup`: a restore that dropped it would silently switch a project's plugins off.
//!
//! **The row is the whole of it.** Turning a plugin on is itself the permission to run its code
//! (`AMB-D-434`), so nothing else has to travel beside these rows for them to mean the same thing wherever
//! they land.
//!
//! One row per `(project_id, plugin)`, where `project_id` is the layer: a project's id, or `None` for the
//! single device row a `scope: machine` plugin holds. The `plugin_enable_pair` and `plugin_enable_device`
//! UNIQUE indexes are what make [`set`] idempotent rather than an append at either layer. Reach is guarded
//! one level up, by the `Store` wrapper, so an AI cannot write a project outside its binding.

use crate::error::Result;
use crate::model::PluginEnabledProject;
use crate::ops::emit_create;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Turn one plugin on (`true`) or off (`false`) at one layer, inside the caller's transaction — write
/// the row, or delete it (`AMB-D-434`: the row is the answer, so there is no second state to update in
/// place). Idempotent on the `(project_id, plugin)` pair in both directions; returns whether anything
/// changed.
pub fn set(tx: &WriteTx<'_>, project_id: Option<i64>, plugin: &str, on: bool) -> Result<bool> {
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

/// Delete **every** row this plugin holds, at every layer, inside the caller's transaction —
/// the store half of `uninstall` beside [`crate::ops::plugin_config::forget_plugin`] (`AMB-D-357`: nothing
/// is left behind, and a re-install starts clean). Returns how many rows went.
///
/// It crosses layers on purpose, for the reason its config twin does: a plugin is installed machine-wide
/// (`AMB-D-350`), so its gate rows are one plugin's residue and not one project's content — and a plugin
/// whose declaration changed layer between installs would otherwise leave the old layer's row behind.
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
            assert!(!read::plugin_enabled_in_project(tx.conn(), Some(p), "slack").unwrap());
            assert!(set(tx, Some(p), "slack", true).unwrap());
            assert!(read::plugin_enabled_in_project(tx.conn(), Some(p), "slack").unwrap());
        });
    }

    /// Turning it off deletes the row rather than storing a `false`: there is no tier under it, so "off
    /// here" and "nothing here" are the same state (`AMB-D-434`).
    #[test]
    fn turning_it_off_deletes_the_row() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), "slack", true).unwrap();
            assert!(set(tx, Some(p), "slack", false).unwrap());
            assert!(!read::plugin_enabled_in_project(tx.conn(), Some(p), "slack").unwrap());
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
            assert!(set(tx, Some(p), "slack", true).unwrap());
            assert!(!set(tx, Some(p), "slack", true).unwrap(), "already on");
            assert!(set(tx, Some(p), "slack", false).unwrap());
            assert!(!set(tx, Some(p), "slack", false).unwrap(), "already off");
        });
    }

    #[test]
    fn one_row_per_pair_however_often_it_is_toggled() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            for on in [true, false, true] {
                set(tx, Some(p), "slack", on).unwrap();
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
            set(tx, Some(a), "slack", true).unwrap();
            assert!(read::plugin_enabled_in_project(tx.conn(), Some(a), "slack").unwrap());
            assert!(
                !read::plugin_enabled_in_project(tx.conn(), Some(b), "slack").unwrap(),
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
            set(tx, Some(a), "slack", true).unwrap();
            set(tx, Some(b), "slack", true).unwrap();
            set(tx, Some(b), "worktree", true).unwrap();

            assert_eq!(forget_plugin(tx, "slack").unwrap(), 2);
            assert!(!read::plugin_enabled_in_project(tx.conn(), Some(a), "slack").unwrap());
            assert!(!read::plugin_enabled_in_project(tx.conn(), Some(b), "slack").unwrap());
            assert!(
                read::plugin_enabled_in_project(tx.conn(), Some(b), "worktree").unwrap(),
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

    // ───────────────────────── the device layer (`AMB-D-601`) ─────────────────────────

    /// The two layers are two gates, not one read two ways: a plugin on for the device is not on in a
    /// project that never opened it, and neither answer leaks into the other.
    #[test]
    fn the_device_gate_and_a_projects_gate_are_separate_answers() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            assert!(set(tx, None, "carrier", true).unwrap());
            assert!(read::plugin_enabled_in_project(tx.conn(), None, "carrier").unwrap());
            assert!(
                !read::plugin_enabled_in_project(tx.conn(), Some(p), "carrier").unwrap(),
                "the device's answer is not the project's",
            );

            set(tx, Some(p), "carrier", true).unwrap();
            set(tx, None, "carrier", false).unwrap();
            assert!(
                read::plugin_enabled_in_project(tx.conn(), Some(p), "carrier").unwrap(),
                "closing one leaves the other standing",
            );
        });
    }

    /// The device row is one row however often it is toggled: SQLite counts NULLs in an index as distinct,
    /// so the pair index cannot hold this — `plugin_enable_device` is what does.
    #[test]
    fn one_device_row_however_often_it_is_toggled() {
        with_tx(|tx| {
            for on in [true, false, true, true] {
                set(tx, None, "carrier", on).unwrap();
            }
            let n: i64 = tx
                .conn()
                .query_row(
                    "SELECT count(*) FROM plugin_enable WHERE project_id IS NULL AND plugin='carrier'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "the device row is unique — a re-enable never appends");
        });
    }

    /// Deleting a project takes its own gate and leaves the device one: the cascade follows the reference,
    /// and the device row holds none (`AMB-D-601`).
    #[test]
    fn deleting_the_project_leaves_the_device_gate_standing() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), "carrier", true).unwrap();
            set(tx, None, "carrier", true).unwrap();

            crate::ops::project::delete(tx, p).unwrap();

            assert!(!read::plugin_enabled_in_project(tx.conn(), Some(p), "carrier").unwrap());
            assert!(
                read::plugin_enabled_in_project(tx.conn(), None, "carrier").unwrap(),
                "the device gate belongs to no project, so no cascade reaches it",
            );
        });
    }

    /// An uninstall leaves nothing behind at either layer (`AMB-D-357`) — including a plugin that has rows
    /// at both, which is what a declaration that changed layer between installs leaves.
    #[test]
    fn forgetting_a_plugin_takes_the_device_row_too() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), "carrier", true).unwrap();
            set(tx, None, "carrier", true).unwrap();

            assert_eq!(forget_plugin(tx, "carrier").unwrap(), 2);
            assert!(!read::plugin_enabled_in_project(tx.conn(), None, "carrier").unwrap());
            assert!(!read::plugin_enabled_in_project(tx.conn(), Some(p), "carrier").unwrap());
        });
    }

    #[test]
    fn deleting_the_project_cascades_its_rows() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), "slack", true).unwrap();
            crate::ops::project::delete(tx, p).unwrap();
            let n: i64 = tx
                .conn()
                .query_row("SELECT count(*) FROM plugin_enable WHERE project_id=?1", [p], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "the row cascaded with the project (ON DELETE CASCADE)");
        });
    }
}
