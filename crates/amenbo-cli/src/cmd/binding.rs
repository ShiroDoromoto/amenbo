//! `init` / `bind` / `unbind` / `sync-guide` / `whoami`: the folder's pointer to a project — what
//! places it, what removes it, what repairs it, and who is acting through it.

use serde_json::json;

use amenbo_core::Store;
use amenbo_core::config::Paths;

use crate::cmd::place::{location_header, project_name, slug_mismatch_warning};
use crate::output::{confirm, human, print_json, write_envelope, CliError, Flags};

pub(crate) fn whoami(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    let id = &store.identity;
    let live = amenbo_core::identity::live_hw();
    let mismatch = id.hw_mismatch();
    // The facet is the only actor there is, so the display name comes from config (`human_name`).
    if flags.json {
        print_json(&json!({
            "human_name": store.config.human_display_name(),
            "bound_hw": id.bound_hw,
            "live_hw": live,
            "hw_mismatch": mismatch,
        }));
    } else {
        if let Some(loc) = location_header(store) {
            human(flags, loc);
        }
        human(flags, format!("human: {}", store.config.human_display_name()));
        human(flags, format!("hardware check: {}", if mismatch { "⚠ mismatch (suspected copy to another machine)" } else { "ok" }));
    }
    Ok(0)
}

/// The guidance for AI agents that `amenbo init` leaves in the current folder.
/// Idempotently upserts the managed block of generic amenbo guidance (Class A) into **both** `AGENTS.md` and
/// `CLAUDE.md` under `dir`. Only what lies between the markers is ours; the user's own Class P content is
/// preserved (no file → create, markers present → replace the block, markers absent → append at the end).
/// The command reference is not duplicated here — `{CMD} agent --json` is its single source of truth, which
/// keeps this from rotting and keeps it out of commit diffs. `{CMD}` follows `Paths::command_name` (`amenbo` in
/// production, `amenbo-dev` on the dev channel) so a dev build never points people at production. With no
/// `lang_code`, English. Returns only the files whose content actually changed (created or updated), so
/// calling it again is a no-op.
pub(crate) fn upsert_agent_guidance(dir: &std::path::Path, lang_code: Option<&str>) -> Vec<&'static str> {
    // Picking the language — the fallback, and keeping an existing block's language — is `upsert_into_dir`'s.
    amenbo_core::agents::upsert_into_dir(dir, lang_code, Paths::command_name())
}

/// When a new binary raises the managed-block template version, folders bound earlier keep the old one. A
/// folder somebody walks into repairs itself — running amenbo there brings it forward
/// ([`amenbo_core::binding::resolve_upward`]). This command is for the folders nobody walks into, and for
/// blocks that could not be rewritten at the time (the leftovers `doctor` keeps flagging as
/// `stale_managed_block`): it resyncs the `CLAUDE.md` / `AGENTS.md` of every bound folder to the current
/// version in one go. `upsert_into_dir` writes only when the content actually changes, so this causes no
/// churn, and the language label is kept from each folder's existing block (`lang_code = None`) rather than
/// degraded. Without `--dir` it covers every bound folder on this device, matching what `doctor` looks at.
/// Paths that were deleted or renamed are skipped silently — `doctor` surfaces those as stale.
pub(crate) fn sync_guide(store: &Store, flags: &Flags, dir: Option<String>) -> Result<i32, CliError> {
    use amenbo_core::agents::{resync_bound_blocks, MANAGED_BLOCK_VERSION};
    // The resync itself is core's shared path (`resync_bound_blocks`, the same one the GUI uses); all that
    // happens here is formatting it for the terminal or for JSON. The bound folders come from the binding
    // table in the consolidated store.
    let report = resync_bound_blocks(&store.bindings(), dir.as_deref(), Paths::command_name());
    let mut updated: Vec<serde_json::Value> = Vec::new();
    for (d, f) in &report.updated {
        human(flags, format!("✓ {d}: {f} resynced to the current version (v{MANAGED_BLOCK_VERSION})"));
        updated.push(json!({ "dir": d, "file": f }));
    }
    if report.updated.is_empty() {
        human(
            flags,
            format!(
                "every managed block is at the current version (v{MANAGED_BLOCK_VERSION}) — {} folder(s) checked, nothing changed.",
                report.scanned
            ),
        );
    }
    if flags.json {
        print_json(&json!({
            "ok": true,
            "action": "sync_guide",
            "version": MANAGED_BLOCK_VERSION,
            "scanned": report.scanned,
            "updated": updated,
        }));
    }
    Ok(0)
}

pub(crate) fn init_cmd(flags: &Flags, name: Option<String>, language: Option<String>, force: bool) -> Result<i32, CliError> {
    let paths = Paths::resolve().map_err(CliError::from)?;
    // There is one store. `init` does not raise a new one — it creates a new project in this folder and
    // places the pointer — and only doubles as genesis when no store exists yet. Whether one exists is
    // answered by peeking at the truth-source file directly: merely opening with `Store::open_at` writes
    // genesis and stamps `format_version`, i.e. it would touch a store we have not decided to touch.
    let store_exists = amenbo_core::store_engine::probe_is_populated(&paths.store_file);
    // Clobber guard: if this folder (or one above it) is already bound to another project by a `.amenbo`,
    // do not create a new project and overwrite the pointer without asking. Same "respect what is already
    // there" rule that `if !agents.exists()` gives AGENTS.md, applied to `.amenbo`. Only `--force` overrides.
    if !force {
        if let Ok(cwd) = std::env::current_dir() {
            if let Some((_, binding)) = amenbo_core::binding::find_upward(&cwd) {
                // If the store already holds data (tasks/projects), refuse all the harder and spell out how
                // to recover.
                let has_data = !amenbo_core::store::store_file_is_content_empty(&paths.store_file)
                    .unwrap_or(true);
                let pid = binding
                    .project_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "(no project)".to_string());
                return Err(CliError::init_pointer_exists(
                    &cwd.to_string_lossy(),
                    &pid,
                    has_data,
                ));
            }
            // No pointer (we did not return above), but this CWD already has a CLAUDE.md/AGENTS.md carrying
            // an amenbo managed block. A marker alone is not grounds to hard-block: it is a thin, borrowed
            // surface that a clone, a copy or a sync carries along, so it proves nothing about ownership.
            // Ownership lives in amenbo's own artifacts (`.amenbo` plus the bindings registry), so branch on
            // what the registry's reverse lookup says: which live projects claim this cwd.
            if store_exists && amenbo_core::agents::dir_has_managed_block(&cwd) {
                let store = Store::open_at(paths.clone()).map_err(CliError::from)?;
                let owners = amenbo_core::binding::live_projects_claiming(&store, &cwd);
                match owners.as_slice() {
                    // (a) Exactly one live project: recover the missing/broken `.amenbo` (a bind, in effect —
                    // never a silent init).
                    [project_id] => {
                        return recover_lost_pointer(flags, &cwd, *project_id, &store);
                    }
                    // (c) Several claim it: which lost pointer to recover is not determined, so stop and
                    // offer the candidates.
                    many if many.len() > 1 => {
                        let ids: Vec<String> = many.iter().map(|pid| pid.to_string()).collect();
                        return Err(CliError::init_ambiguous_owners(&cwd.to_string_lossy(), &ids));
                    }
                    // (b) None: a stale marker left by a clone, a copy, or a leftover. Do not hard-block —
                    // carry on with init. A new project is created below and `upsert_agent_guidance`
                    // regenerates the managed block idempotently (anything outside the markers is kept).
                    _ => {}
                }
            }
        }
    }
    // Get the store (the single DB) ready: open it if it is already there (`Store::init` rejects an
    // initialized store as a conflict), otherwise raise it with genesis. Neither the store nor any secret
    // goes in the project directory — that keeps them out of iCloud-synced trees, which corrupt them, and
    // out of the AI's project context.
    let mut store = if store_exists {
        if name.is_some() {
            human(flags, format!("  (--name is ignored: this device already holds an amenbo store; change your display name with `{} config set human_name <name>`)", Paths::command_name()));
        }
        Store::open_at(paths).map_err(CliError::from)?
    } else {
        Store::init(paths, name.as_deref()).map_err(CliError::from)?
    };
    // `--language`, if given, sets the global user language (stored in the user-layer config).
    if let Some(lang) = &language {
        store.config.set("language", lang).map_err(CliError::from)?;
    }
    // The language embedded in AGENTS.md is the global setting — what `--language` just set, or what was
    // already there.
    let lang_code = store.config.language.clone();
    // init mirrors the GUI's "new project in a folder" (`provision_project_store`): create one initial
    // project named after the folder (the CWD basename; a language-specific default when that is empty or
    // unusable) and stamp its project_id into `.amenbo`. A folder init'd from the CLI then shows up in the
    // GUI sidebar (which renders the project list) on its own, and `task add` works there right away (the
    // required project comes from the pointer's project_id). Project creation happens on this CLI path only:
    // `Store::init` is shared with the GUI's provisioning, so putting it there would create it twice.
    let cwd = std::env::current_dir().ok();
    let project_name = cwd
        .as_deref()
        .and_then(|c| c.file_name())
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| amenbo_core::config::default_project_name(lang_code.as_deref()));
    let project = store
        .project_add(amenbo_core::ops::project::NewProject {
            name: project_name,
            view: store.config.default_view,
            notes: String::new(),
            color: None,
        })
        .map_err(CliError::from)?;
    let project_id = project.id;
    let project_name = project.name.clone();
    // The project was already committed by the write seam. All that is flushed here is the config that
    // `--language` touched.
    store.save_config().map_err(CliError::from)?;
    // Place `.amenbo` (the dir→project pointer) and the AI guidance (AGENTS.md) in the current folder.
    let mut placed: Vec<&str> = Vec::new();
    if let Some(cwd) = cwd {
        // The pointer names a project and nothing else — `.amenbo` plays no part in choosing the store.
        let pointer = amenbo_core::binding::pointer_for(&store, project_id);
        if pointer.write(&cwd).is_ok() {
            placed.push(".amenbo");
            // Record the project→folder reverse lookup (what the settings screen lists, and the index a lost
            // pointer is recovered from), claiming the folder for the project the pointer just written names
            // — any record another project held for it is retracted. Best-effort: a failure to record does
            // not fail init, same as the pointer write.
            let mut registry = store.bindings();
            registry.claim_project_ref(project_id, cwd.to_string_lossy());
            let _ = store.save_bindings(&registry);
        }
        // Idempotently upsert the generic amenbo guidance (the managed block) into both AGENTS.md and
        // CLAUDE.md. Only the space between the markers is ours; the user's Class P content is preserved.
        // English base plus a directive naming the user's language.
        placed.extend(upsert_agent_guidance(&cwd, lang_code.as_deref()));
    }
    let human_name = store.config.human_display_name();
    // Success reports what you can now do and what to do next, not what machinery ran. The `.amenbo` and
    // AGENTS.md that were placed get a light mention in parentheses.
    if flags.json {
        write_envelope(flags, "init", "identity",
            json!({ "human_name": human_name, "project_id": project_id, "project_name": project_name, "placed": placed }),
            None, false, "");
    } else {
        human(flags, format!("✓ Ready — project '{}' is set up; an AI launched in this folder can now operate amenbo (you are {}).", project_name, human_name));
        human(flags, format!("  Next: {} status", Paths::command_name()));
        if !placed.is_empty() {
            human(flags, format!("  (placed {})", placed.join(", ")));
        }
        // init writes the managed block mid-session, and a block written mid-session does not bind the
        // session that is running — it takes effect at the next one. So tell the AI that just ran init to run
        // `agent` right now, and close that gap. The command name follows the channel (amenbo / amenbo-dev).
        human(flags, format!(
            "  AI agents: run `{} agent --json` now and follow it — the managed guidance just written to CLAUDE.md/AGENTS.md does not take effect until your next session.",
            Paths::command_name(),
        ));
    }
    Ok(0)
}

/// When the `.amenbo` is gone but the bindings registry's reverse lookup names exactly one live project as
/// this folder's owner, recover the pointer instead of creating a new project — no silent init. It amounts
/// to a `bind --project`: rewrite the pointer and the bindings index, and idempotently upsert the managed
/// block (anything outside the markers is kept).
fn recover_lost_pointer(
    flags: &Flags,
    cwd: &std::path::Path,
    project_id: i64,
    store: &Store,
) -> Result<i32, CliError> {
    // Rewrite `.amenbo` (this is a bind, in effect).
    amenbo_core::binding::pointer_for(store, project_id).write(cwd).map_err(CliError::from)?;
    // Update the bindings index idempotently (the many-to-one reverse lookup), claiming the folder for the
    // project the recovered pointer names.
    {
        let mut reg = store.bindings();
        reg.claim_project_ref(project_id, cwd.to_string_lossy());
        let _ = store.save_bindings(&reg);
    }
    // Regenerate the managed block idempotently (outside the markers is kept). The upsert keeps an existing
    // block's language label.
    upsert_agent_guidance(cwd, store.config.language.as_deref());

    // Answer with the same facet keys as the init envelope — the same display name must not come back under
    // two different key names.
    let human_name = store.config.human_display_name();
    let project_name = project_name(store, Some(project_id))?;
    if flags.json {
        write_envelope(flags, "init", "identity",
            json!({ "human_name": human_name, "project_id": project_id, "project_name": project_name, "recovered": true }),
            None, false, "");
    } else {
        human(flags, format!(
            "✓ Recovered — this folder was already linked to project '{}' but its .amenbo pointer was missing; rewrote it (you are {}).",
            project_name.clone().unwrap_or_else(|| project_id.to_string()), human_name,
        ));
        human(flags, format!("  Next: {} status", Paths::command_name()));
    }
    Ok(0)
}

/// The project→folders reverse lookup, as a JSON array. Lists the folders recorded in the registry (the
/// binding table of the consolidated store) in ascending order, each with its absolute path and what
/// inspecting it found. An absolute path goes stale when the folder is moved or renamed, so `exists=false`
/// is a cleanup candidate. The inspection carries the same three findings as the GUI's folder list
/// (`BoundFolderDto`): `legacy` (an old-format pointer), `pointer_missing` (the folder is there but the
/// `.amenbo` is not), and `mismatch` (the pointer's slug disagrees with the store — it belongs to another
/// one). Every one of those judgements goes through core's shared path.
pub(crate) fn bound_folders_json(store: &Store, project_id: i64) -> serde_json::Value {
    use amenbo_core::binding;
    let folders: Vec<serde_json::Value> = store
        .bindings()
        .dirs_for_project(project_id)
        .into_iter()
        .map(|dir| {
            let path = std::path::Path::new(dir);
            let mismatch = binding::read_pointer(path)
                .and_then(|b| binding::slug_mismatch(store, &b))
                .map(|m| json!({ "project_id": m.project_id, "recorded": m.recorded, "actual": m.actual }));
            json!({
                "path": dir,
                "exists": path.is_dir(),
                "legacy": binding::is_legacy_pointer(path),
                "pointer_missing": binding::is_pointer_missing(path),
                "mismatch": mismatch,
            })
        })
        .collect();
    serde_json::Value::Array(folders)
}

/// Resolve the folder `bind` acts on: the directory `--dir <path>` names, or the CWD when it is omitted.
/// bind places a `.amenbo` inside that folder, so anything that is not an existing directory — a file, or
/// nothing at all — is refused.
pub(crate) fn resolve_bind_target(dir: Option<String>) -> Result<std::path::PathBuf, CliError> {
    match dir {
        Some(d) => {
            let p = std::path::PathBuf::from(&d);
            if !p.is_dir() {
                return Err(CliError {
                    code: "io_error",
                    message: format!("--dir target does not exist or is not a directory: {d}"),
                    hint: Some("Pass an existing directory to place its `.amenbo` pointer in.".to_string()),
                    exit: 1,
                });
            }
            // Registrations made from the CWD are absolute paths. Canonicalize so a relative `--dir` does not
            // put a different string in the registry (it exists — checked above — so this cannot fail).
            Ok(std::fs::canonicalize(&p).unwrap_or(p))
        }
        None => std::env::current_dir()
            .map_err(|e| CliError { code: "io_error", message: e.to_string(), hint: None, exit: 1 }),
    }
}

/// Put a project's binding in `dir`: the `.amenbo` pointer, the registry's project→folder record (the
/// many-to-one reverse lookup the settings screen lists), and the managed guidance block in that folder's
/// AGENTS.md / CLAUDE.md (whatever is outside the markers is kept). This is the whole of what linking a
/// folder means, which is why `bind --project` and `project add --dir` both go through it rather than each
/// remembering the three steps.
///
/// Recording the folder is a **claim**: the pointer just written names one project, so the records other
/// projects hold for this folder are retracted with it (`Registry::claim_project_ref`). A re-point that
/// left the old pair standing would leave two live projects claiming one folder — the old project would go
/// on listing a folder that no longer names it, and a lost pointer there could no longer be recovered.
///
/// Every step is required to succeed. A caller that raised the project for this folder has to undo that on
/// failure — a project nothing is linked to is what `AMB-D-528` refuses to create.
pub(crate) fn place_binding(store: &Store, project_id: i64, dir: &std::path::Path) -> Result<(), CliError> {
    amenbo_core::binding::pointer_for(store, project_id).write(dir).map_err(CliError::from)?;
    let mut registry = store.bindings();
    registry.claim_project_ref(project_id, dir.to_string_lossy());
    store.save_bindings(&registry).map_err(CliError::from)?;
    upsert_agent_guidance(dir, store.config.language.as_deref());
    Ok(())
}

/// `amenbo bind --project <p> --rebind <binding-id>`: the binding already recorded under `binding_id`
/// comes to name this folder, **keeping its id**. That is the whole difference from binding the folder
/// again: a plain bind records a new row, and whatever pointed at the old one — a task filed at that
/// folder (`AMB-D-648`) — is left naming a row nobody points at. A folder that was moved, renamed or
/// restored somewhere else is the same folder, and this is how it is said.
///
/// The row has to be there. A number nobody bound (or one an unbind retired) is a typo, and the answer
/// lines up the ids there are rather than quietly recording a new binding under the number given.
/// Otherwise this writes exactly what a bind writes — the `.amenbo` pointer and the folder's guidance
/// block — so the new folder is bound in every sense; only the registry side is an id-keyed re-point
/// instead of a claim.
fn rebind_cmd(store: &Store, flags: &Flags, pid: i64, cwd: &std::path::Path, binding_id: i64) -> Result<i32, CliError> {
    let known = store.bound_folders().map_err(CliError::from)?;
    if !known.iter().any(|b| b.id == binding_id) {
        return Err(CliError::binding_unknown(binding_id, pid, &known));
    }
    // The pointer first, as `place_binding` writes it: the folder is what the registry is an index of.
    amenbo_core::binding::pointer_for(store, pid).write(cwd).map_err(CliError::from)?;
    let dir = cwd.to_string_lossy().to_string();
    let done = store
        .repoint_binding(binding_id, pid, &dir)
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::binding_unknown(binding_id, pid, &known))?;
    upsert_agent_guidance(cwd, store.config.language.as_deref());
    let name = project_name(store, Some(pid))?.unwrap_or_default();
    if flags.json {
        write_envelope(flags, "bind.rebind", "binding",
            json!({ "binding_id": done.id, "project_id": pid, "project_name": name, "dir": dir,
                    "previous_dir": done.previous_dir, "previous_project_id": done.previous_project_id,
                    "retracted_bindings": done.retracted.iter().map(|b| b.id).collect::<Vec<_>>() }),
            None, false, "");
    } else {
        human(flags, format!("✓ Binding {} now points here — {} (was {}).", done.id, dir, done.previous_dir));
        human(flags, "  It kept its id, so whatever was filed at that folder followed it.");
        // The project is normally the one it already named — worth a line only when it is not.
        if done.previous_project_id != pid {
            human(flags, format!("  It came off project {}, and belongs to '{name}' now.", done.previous_project_id));
        }
    }
    Ok(0)
}

pub(crate) fn bind_cmd(store: &Store, flags: &Flags, project: Option<String>, dir: Option<String>, force: bool, rebind: Option<i64>) -> Result<i32, CliError> {
    use amenbo_core::binding::find_upward_ancestor;
    // With `--dir <path>`, the `.amenbo` goes in that folder rather than the CWD — binding from outside it.
    let cwd = resolve_bind_target(dir)?;

    if let Some(p) = project {
        // Nested-binding guard: binding inside a subdirectory of an already-managed tree (an ancestor holds a
        // `.amenbo`) would shadow the parent with the pointer placed here, and scatter `.amenbo`/AGENTS.md/
        // CLAUDE.md through the source tree. Same "respect the tree that is already there" rule as `init`'s
        // clobber guard. A deliberate subdir bind gets through with `--force`.
        if !force {
            if let Some((dir, _)) = find_upward_ancestor(&cwd) {
                return Err(CliError::binding_nested_tree(&dir.to_string_lossy(), true));
            }
        }
        // Bind: resolve the project in the store and place the `.amenbo` pointer (its project_id). Several
        // directories may point at the same project_id, which makes the relation many-to-one.
        let pid = store.resolve_project_ref(&p).map_err(CliError::from)?;
        // `--rebind <id>` moves a binding that is already recorded onto this folder instead of adding
        // one. Everything up to here is the same act — the guard, the project — and what differs is
        // only whether the registry gains a row or one of its rows moves.
        if let Some(binding_id) = rebind {
            return rebind_cmd(store, flags, pid, &cwd, binding_id);
        }
        place_binding(store, pid, &cwd)?;
        let name = project_name(store, Some(pid))?.unwrap_or_default();
        // For a human: what you can now do, and what to do next.
        if flags.json {
            write_envelope(flags, "bind", "binding",
                json!({ "project_id": pid, "project_name": name, "dir": cwd.to_string_lossy() }),
                None, false, "");
        } else {
            human(flags, format!("✓ Linked this folder to project '{name}' — an AI launched here can now operate this project."));
            human(flags, format!("  Next: {} status", Paths::command_name()));
        }
        return Ok(0);
    }

    // Display: search upward for `.amenbo` (a legacy pointer is read compatibly and rewritten lazily) and
    // check the registered path for staleness.
    match amenbo_core::binding::resolve_upward(store, &cwd) {
        Some((dir, b)) => {
            // A project→dir registration whose path has vanished is binding_stale. The folders bound to a
            // project stand alongside each other, so the question is put to each of them and the first
            // vanished one is what the error names — with every one of them listed by id beside it,
            // since re-pointing is what a vanished folder wants and an id is what that takes.
            if let Some(pid) = b.project_id {
                let gone: Vec<amenbo_core::binding::BoundFolder> = store
                    .bound_folders()
                    .map_err(CliError::from)?
                    .into_iter()
                    .filter(|f| f.project_id == pid && !f.exists())
                    .collect();
                if !gone.is_empty() {
                    return Err(CliError::binding_stale(pid, &gone));
                }
            }
            let name = project_name(store, b.project_id)?;
            // A recorded slug that disagrees with the store means this pointer likely came from another one.
            let mismatch = slug_mismatch_warning(store, &b);
            if flags.json {
                print_json(&json!({ "ok": true, "action": "bind.show", "binding": {
                    "found_in": dir.to_string_lossy(),
                    "project_id": b.project_id, "project_name": name,
                    "slug": b.slug, "slug_mismatch": mismatch.is_some() } }));
            } else {
                human(flags, format!("Binding: {} ({})", name.clone().unwrap_or_else(|| "(no project set)".to_string()), dir.to_string_lossy()));
                if let Some(warning) = &mismatch {
                    human(flags, warning);
                }
                human(flags, format!("To link a project, run `{} bind --project <name or ID>`.", Paths::command_name()));
            }
        }
        None => {
            if flags.json {
                print_json(&json!({ "ok": true, "action": "bind.show", "binding": null }));
            } else {
                human(flags, format!("No .amenbo found in this directory (or above). Link one with `{} bind --project <name or ID>`.", Paths::command_name()));
            }
        }
    }
    Ok(0)
}

/// `amenbo unbind`: undo this folder's `.amenbo` binding (the CWD by default, any folder with `--dir`). It
/// removes that folder's pointer and nothing else — the relation is many-to-one, so other folders pointing at
/// the same project are untouched — and it never deletes the store: this is the inverse of `bind`, not a
/// teardown. It also strips the managed block from the AGENTS.md / CLAUDE.md that bind/init wrote (the user's
/// Class P content is preserved) and forgets this folder in the registry, keeping the orphan-detection index
/// consistent. It never unbinds an ancestor: with no `.amenbo` directly in this folder, the tree above is not
/// dragged in — `unbind_no_binding` says where to run it instead.
///
/// **The last folder goes too** (`AMB-D-530`). There is no count here that refuses: peeling a folder off is a
/// deliberate act, and re-homing folders means taking them all off before putting them back, so a refusal
/// would only force an order rather than protect anything. What is closed instead is the creating end
/// (`AMB-D-528`). What is owed is the report: the answer says the project has no folder left and that
/// nothing can operate it until one is linked again, because a binding **is** an AI's reach (`AMB-D-222`),
/// and only the person doing it knows whether they are mid-reshuffle. `--json` carries the same fact as a
/// number (`project_folders_left`), so a machine reads it without parsing the sentence.
pub(crate) fn unbind_cmd(flags: &Flags, dir: Option<String>) -> Result<i32, CliError> {
    use amenbo_core::binding::{find_upward, DirBinding};
    let target = match dir {
        Some(d) => std::path::PathBuf::from(d),
        None => std::env::current_dir()
            .map_err(|e| CliError { code: "io_error", message: e.to_string(), hint: None, exit: 1 })?,
    };
    let marker = target.join(".amenbo");
    if !marker.is_file() {
        // Not here. If an ancestor is bound, say so — but do not silently unbind it.
        let ancestor = find_upward(&target).map(|(d, _)| d.to_string_lossy().to_string());
        return Err(CliError::unbind_no_binding(&target.to_string_lossy(), ancestor.as_deref()));
    }
    if !confirm(flags, &format!("unbind this folder ({}) from amenbo (removes .amenbo and amenbo's managed blocks; the store is kept)", target.to_string_lossy()))? {
        return Ok(0);
    }
    // Read the pointer before removing it, to report its project_id (best-effort).
    let pointer: Option<DirBinding> = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());
    // Delete `.amenbo`.
    std::fs::remove_file(&marker)
        .map_err(|e| CliError { code: "io_error", message: format!("could not remove {}: {e}", marker.display()), hint: None, exit: 1 })?;
    let mut removed: Vec<String> = vec![".amenbo".to_string()];
    // Strip the managed block from AGENTS.md / CLAUDE.md (Class P is preserved; a file that was nothing but
    // the block is deleted).
    removed.extend(amenbo_core::agents::remove_from_dir(&target).into_iter().map(String::from));
    // Forget this folder in the binding registry (the binding table of the consolidated store). Drop every
    // registration by folder rather than by the pointer's id (`forget_dir`), so the index is cleaned even
    // when the pointer was stale. If there is no store yet (no store file), there is no index either: remove
    // the pointer and the managed block and stop — never silently genesis a store, same rule as the exec
    // guard.
    let dir_str = target.to_string_lossy().to_string();
    let project_id = pointer.as_ref().and_then(|b| b.project_id);
    let mut forgot = 0usize;
    // How many folders the project has left, once this one is off. None where there is nothing to ask —
    // no store on this machine, or a pointer naming no project.
    let mut folders_left: Option<usize> = None;
    let paths = Paths::resolve().map_err(CliError::from)?;
    if amenbo_core::store_engine::probe_is_populated(&paths.store_file) {
        let mut store = Store::open_at(paths).map_err(CliError::from)?;
        let mut registry = store.bindings();
        forgot = registry.forget_dir(&dir_str);
        // The CWD is already canonical, but a non-canonical path passed to `--dir` can disagree with the
        // registered string (the CWD as it read at bind time). Try the canonical form too, if it differs.
        if let Ok(canon) = std::fs::canonicalize(&target) {
            let canon_str = canon.to_string_lossy().to_string();
            if canon_str != dir_str {
                forgot += registry.forget_dir(&canon_str);
            }
        }
        if forgot > 0 {
            store.save_bindings(&registry).map_err(CliError::from)?;
            // The folder is gone, so no task can be worked in it any more: whichever of this project's
            // tasks named it lose their place (`AMB-D-648`). Best-effort — the unbind itself has landed,
            // and a task still naming a folder nothing answers for already reads as naming none.
            if let Some(pid) = project_id {
                let _ = store.forget_gone_task_folders(pid);
            }
        }
        // Counted after the forgetting, so it is what is left rather than what there was.
        folders_left = project_id.map(|pid| registry.dirs_for_project(pid).len());
    }
    let mut binding = json!({ "dir": dir_str, "project_id": project_id, "removed": removed, "registry_entries_forgotten": forgot });
    if let Some(left) = folders_left {
        binding["project_folders_left"] = json!(left);
    }
    write_envelope(
        flags,
        "unbind",
        "binding",
        binding,
        None,
        false,
        format!("✓ Unbound {} (removed: {}). The project is kept.", dir_str, removed.join(", ")),
    );
    // Taking the last one is allowed, and saying so is what stands in for refusing it. A folder is how an
    // AI reaches a project, so with none left there is nobody to operate it — which is either the middle of
    // a reshuffle or a surprise, and only the person here knows which.
    if folders_left == Some(0) {
        human(
            flags,
            format!(
                "  This project now has no folder. An AI reaches a project through a folder, so nothing can operate it until one is linked again (`{} bind --project <name or ID>`).",
                Paths::command_name()
            ),
        );
    }
    Ok(0)
}
