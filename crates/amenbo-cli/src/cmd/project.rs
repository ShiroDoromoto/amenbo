//! `project`: the projects work is filed under.

use serde_json::json;

use amenbo_core::config::Paths;
use amenbo_core::{ops, Store};

use crate::cli::*;
use crate::cmd::arg::{parse_view, pos_from_keys};
use crate::cmd::binding::{bound_folders_json, place_binding, resolve_bind_target};
use crate::cmd::guard::guard_ai_project_ops;
use crate::output::{confirm, human, print_json, write_envelope, CliError, Flags};

pub(crate) fn project(store: &mut Store, flags: &Flags, sub: ProjectCmd) -> Result<i32, CliError> {
    match sub {
        ProjectCmd::Add { name, dir, view, notes, color } => {
            // The folder comes first, and everything that could refuse it is asked before the project
            // exists (`AMB-D-529`): a folder that is not there, one already linked, one inside a managed
            // tree. The nested-worktree guard has already answered for this same folder, ahead of any
            // dispatch.
            let dir = resolve_bind_target(Some(dir))?;
            if let Some(pointer) = amenbo_core::binding::read_pointer(&dir) {
                let pid = pointer
                    .project_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "(no project)".to_string());
                return Err(CliError::project_dir_bound(&dir.to_string_lossy(), &pid));
            }
            // The same "respect the tree that is already there" rule `bind` follows — and with no
            // `--force` here, since creating a project is not the moment to overrule it.
            if let Some((ancestor, _)) = amenbo_core::binding::find_upward_ancestor(&dir) {
                return Err(CliError::binding_nested_tree(&ancestor.to_string_lossy(), false));
            }
            // No `--view` is not "board": it is "whatever this store was configured to open a new
            // project on". The setting exists to be the answer here, so reading it anywhere else —
            // or defaulting past it — is what would leave it a value nothing acts on.
            let view = match view {
                Some(v) => parse_view(&v)?,
                None => store.config.default_view,
            };
            let p = store.project_add(ops::project::NewProject { name, view, notes, color }).map_err(CliError::from)?;
            // Linking is what the project was raised for, so a failure here takes the project with it:
            // leaving one behind that nothing points at is the state `--dir` was made required to prevent.
            if let Err(e) = place_binding(store, p.id, &dir) {
                let _ = store.project_delete(p.id, flags.facet()?);
                return Err(e);
            }
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            let mut value = serde_json::to_value(&detail).unwrap();
            if let Some(obj) = value.as_object_mut() {
                obj.insert("dir".to_string(), json!(dir.to_string_lossy()));
            }
            write_envelope(flags, "project.add", "project", value, None, false,
                format!("✓ Created project: {} ({}) — linked to {}", p.name, p.id, dir.to_string_lossy()));
        }
        ProjectCmd::List { archived } => {
            let result = store.project_list(archived).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, format!("{} project(s)", result.count));
                for p in &result.projects {
                    let a = if p.archived { " [archived]" } else { "" };
                    human(flags, format!("  {}  {} (tasks: {}){}", amenbo_core::idref::project(p.id), p.name, p.num_tasks, a));
                }
            }
        }
        ProjectCmd::Show { id } => {
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let detail = store.project_detail(pid).map_err(CliError::from)?;
            // The project→folders reverse lookup. The `.amenbo` files are scattered across the filesystem and
            // cannot be enumerated from app-data, so this comes from what bind recorded in the registry (the
            // binding table of the consolidated store). Absolute paths go stale when a folder is moved or
            // renamed, so every folder carries an `exists` check.
            let bound_folders = bound_folders_json(store, pid);
            if flags.json {
                let mut v = serde_json::to_value(&detail).unwrap();
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("bound_folders".to_string(), bound_folders.clone());
                }
                print_json(&v);
            } else {
                human(flags, format!("{}  {}", amenbo_core::idref::project(detail.id), detail.name));
                human(flags, format!("view: {} / tasks: {} (completed {})", detail.default_view.as_str(), detail.task_counts.total, detail.task_counts.completed));
                let folders = bound_folders.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                if folders.is_empty() {
                    human(flags, "folders: (none bound)");
                } else {
                    human(flags, "folders:");
                    for f in folders {
                        let path = f["path"].as_str().unwrap_or("");
                        // Say what the inspection found on the same line (same material as the JSON; the
                        // order runs strongest first, and "the folder is gone" outranks the rest).
                        let mark = if !f["exists"].as_bool().unwrap_or(false) {
                            "  (missing)".to_string()
                        } else if f["pointer_missing"].as_bool().unwrap_or(false) {
                            format!("  (no .amenbo — run `{} init` there to relink)", Paths::command_name())
                        } else if f["legacy"].as_bool().unwrap_or(false) {
                            "  (legacy .amenbo)".to_string()
                        } else if !f["mismatch"].is_null() {
                            "  (.amenbo points at another store)".to_string()
                        } else {
                            String::new()
                        };
                        human(flags, format!("  {path}{mark}"));
                    }
                }
            }
        }
        ProjectCmd::Update { id, name, notes, view, color } => {
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let mut changed = Vec::new();
            if name.is_some() { changed.push("name".to_string()); }
            if notes.is_some() { changed.push("notes".to_string()); }
            if view.is_some() { changed.push("default_view".to_string()); }
            if color.is_some() { changed.push("color".to_string()); }
            let view = match view { Some(v) => Some(parse_view(&v)?), None => None };
            let p = store.project_update(pid, ops::project::ProjectPatch { name, notes, view, color }).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.update", "project", serde_json::to_value(&detail).unwrap(), Some(changed), false, format!("✓ Updated project: {}", p.id));
        }
        ProjectCmd::Move { id, before, after, top, bottom } => {
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let before = before.map(|b| store.resolve_project_ref(&b)).transpose().map_err(CliError::from)?;
            let after = after.map(|a| store.resolve_project_ref(&a)).transpose().map_err(CliError::from)?;
            let pos = pos_from_keys(top, bottom, before, after)?;
            let p = store.project_move(pid, pos).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.move", "project", serde_json::to_value(&detail).unwrap(), Some(vec!["order_key".to_string()]), false, format!("✓ Moved project: {}", p.id));
        }
        ProjectCmd::Archive { id } => {
            guard_ai_project_ops(store, flags)?;
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let p = store.project_set_archived(pid, true).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.archive", "project", serde_json::to_value(&detail).unwrap(), Some(vec!["archived".to_string()]), false, format!("✓ Archived project: {}", p.id));
        }
        ProjectCmd::Unarchive { id } => {
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            let p = store.project_set_archived(pid, false).map_err(CliError::from)?;
            let detail = store.project_detail(p.id).map_err(CliError::from)?;
            write_envelope(flags, "project.unarchive", "project", serde_json::to_value(&detail).unwrap(), Some(vec!["archived".to_string()]), false, format!("✓ Unarchived project: {}", p.id));
        }
        ProjectCmd::Delete { id } => {
            guard_ai_project_ops(store, flags)?;
            let pid = store.resolve_project_ref(&id).map_err(CliError::from)?;
            if !confirm(flags, "delete project")? {
                return Ok(0);
            }
            store.project_delete(pid, flags.facet()?).map_err(CliError::from)?;
            // Delete is destructive (the same shape as the GUI's `project_delete`). Release the bindings of
            // the folders that still point at this project: their `.amenbo` pointers, managed blocks and
            // registry rows. The teardown is best-effort — a failure there does not fail the delete.
            let _ = amenbo_core::project_teardown::teardown_deleted_project(store, pid);
            write_envelope(flags, "project.delete", "project", json!({ "id": pid, "deleted": true }), None, false, format!("✓ Deleted project: {pid}"));
        }
    }
    Ok(0)
}
