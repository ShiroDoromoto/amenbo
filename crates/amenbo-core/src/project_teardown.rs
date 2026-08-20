//! The destructive teardown that follows a project deletion. Called **after** the rows are physically gone
//! ([`crate::ops::project::delete`]) and saved, it disposes of what is left outside the engine: the folders
//! that were bound to the deleted project (`.amenbo`, the managed block, the bindings registry) and the
//! attachment bytes (blobs) nothing references any more.
//!
//! Archiving keeps a project and hides it; deleting destroys it. To hold that line, a delete leaves behind
//! no trace of ownership.
//!
//! The teardown is best-effort: a filesystem operation that fails does not stop the rest.
//!
//! The store is a single SQLite database, so there is no "the store for this project" to unlink — the rows
//! go with a `DELETE … WHERE project_id`. What is left outside the engine is this module's business.

use std::path::Path;

use crate::binding::Registry;
use crate::store::Store;
use crate::Result;

/// What the teardown did (for the caller to log).
#[derive(Debug, Default, Clone, Copy)]
pub struct TeardownReport {
    /// Folders whose binding was released (`.amenbo` removed, managed block taken out, bindings entry
    /// forgotten).
    pub folders_released: usize,
    /// Folders left standing because they name **another project** now — only the deleted project's own
    /// record of them was forgotten.
    pub folders_kept: usize,
    /// Blobs reclaimed — the bytes of attachments the delete dropped to zero references.
    pub blobs_reclaimed: u64,
    /// Bytes reclaimed.
    pub bytes_freed: u64,
}

/// The destructive teardown of a deleted project:
///
/// 1. Release the binding of **every folder** that pointed at it — remove `.amenbo`, take the managed block
///    out, forget the folder in the bindings registry. Folders bound to other, living projects are left
///    alone: what is released is decided per folder by [`folder_still_ours`], and the registry record that
///    goes is the deleted project's own pair, never the folder's other owners'.
/// 2. Reclaim the blobs that lost their last reference ([`Store::gc_blobs`]). The delete itself has already
///    reclaimed the blobs it orphaned (`Store::reclaim_blobs`), so this full sweep only picks up **what got
///    past that**: blobs still too young to reclaim at the time (`GC_MIN_AGE`), or bytes stranded by an
///    interrupted delete or restore. A project deletion is a good moment to sweep the whole store, so the
///    catch-all lives here — the same cleaning `doctor --fix` does.
///
/// The binding registry is a table in the store, so it is read and written through the open `store`.
pub fn teardown_deleted_project(store: &Store, project_id: i64) -> Result<TeardownReport> {
    let mut registry = store.bindings();

    // Collect into `String`s first, for ownership: `forget_project_ref` borrows the registry mutably.
    let dirs: Vec<String> =
        registry.dirs_for_project(project_id).into_iter().map(str::to_string).collect();

    let mut report = TeardownReport::default();
    for dir in &dirs {
        if release_folder(store, dir, project_id, &mut registry) {
            report.folders_released += 1;
        } else {
            report.folders_kept += 1;
        }
    }
    // Save the registry: releasing has changed its indexes, and the unbinding has to stick rather than
    // being best-effort.
    let _ = store.save_bindings(&registry);

    // Reclaiming blobs *is* best-effort: if it fails the delete still succeeded, and all that is left over
    // is unreclaimed bytes for the next sweep to pick up. A blob that is still too young is skipped
    // (`GC_MIN_AGE`) — it may belong to an attach another process is in the middle of.
    if let Ok(gc) = store.gc_blobs(crate::blob::GC_MIN_AGE) {
        report.blobs_reclaimed = gc.removed;
        report.bytes_freed = gc.freed_bytes;
    }
    Ok(report)
}

/// Release one folder: remove its `.amenbo`, take the managed block out, and forget the folder in the
/// bindings. Returns whether the folder was released **on disk** — a folder that names another project now
/// keeps its pointer and its managed block, and only loses the deleted project's record of it.
///
/// The registered string is the path as canonicalised at bind time, so — in case the path handed in here is
/// not the canonical one — forget the canonical form too (the same shape `unbind` has). What is forgotten is
/// the `(project, dir)` pair, not the folder: `forget_dir` is `unbind`'s move, where the folder itself is
/// leaving Amenbo, and using it here would unbind the folder from every project that holds it.
fn release_folder(store: &Store, dir: &str, project_id: i64, registry: &mut Registry) -> bool {
    let path = Path::new(dir);
    let ours = folder_still_ours(store, path, project_id);
    if ours {
        let marker = path.join(".amenbo");
        if marker.is_file() {
            let _ = std::fs::remove_file(&marker);
        }
        // Take the managed block out of AGENTS.md / CLAUDE.md. The user's own content outside the markers is
        // kept; a file that was nothing but the block is removed.
        let _ = crate::agents::remove_from_dir(path);
    }
    registry.forget_project_ref(project_id, dir);
    if let Ok(canon) = crate::binding::canonical_dir(path) {
        let canon_str = canon.to_string_lossy().to_string();
        if canon_str != dir {
            registry.forget_project_ref(project_id, &canon_str);
        }
    }
    ours
}

/// Is this folder still the deleted project's to clear?
///
/// A `.amenbo` names **one** project, so wherever it reads, the disk itself has the answer: the folder is
/// ours only while its pointer still names us. A folder re-pointed at another project is that project's
/// live binding, and clearing it would unbind a project nobody asked about. Re-pointing now retracts the
/// old pair as it goes ([`crate::binding::Registry::claim_project_ref`]), so the deleted project usually
/// has no record of such a folder to begin with; the check answers for the ones an index written before
/// that still carries.
///
/// With no id to read (no `.amenbo`, or one too old to name a project by id) the registry decides instead:
/// another **living** project recording this folder makes it theirs. The deleted project's own rows are
/// already gone by the time the teardown runs, so it cannot count itself among them.
fn folder_still_ours(store: &Store, dir: &Path, project_id: i64) -> bool {
    match crate::binding::read_pointer(dir).and_then(|b| b.project_id) {
        Some(named) => named == project_id,
        None => crate::binding::live_projects_claiming(store, dir).is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::DirBinding;

    /// Bring up one store in an isolated app-data tree and add `n` projects, returning their ids.
    fn store_with_projects(tag: &str, n: usize) -> (Store, Vec<i64>) {
        let home = amenbo_scratch::scratch(&format!("teardown-home-{tag}"));
        let mut store = Store::open_at(crate::config::Paths::at(home)).unwrap();
        let ids = (0..n)
            .map(|i| {
                store
                    .project_add(crate::ops::project::NewProject {
                        name: format!("案件{i}"),
                        view: crate::model::View::Board,
                        notes: String::new(),
                        color: None,
                    })
                    .unwrap()
                    .id
            })
            .collect();
        (store, ids)
    }

    /// Releasing folder bindings is the work the teardown is left with; this is one folder's worth of it —
    /// `.amenbo` removed, and the folder forgotten in the bindings index.
    #[test]
    fn release_folder_removes_pointer_marker_and_forgets_binding() {
        let (store, ids) = store_with_projects("release", 1);
        let pid = ids[0];
        let tmp = amenbo_scratch::scratch("teardown");
        let dir = tmp.to_string_lossy().to_string();

        // Set up the `.amenbo` pointer and the bindings entry.
        DirBinding::new(Some(pid), Some("proj-x".into())).write(&tmp).unwrap();
        let mut reg = Registry::default();
        reg.record_project_ref(pid, dir.as_str());
        assert!(!reg.projects_for_dir(&dir).is_empty());

        assert!(release_folder(&store, &dir, pid, &mut reg), "the folder still names this project");

        assert!(!tmp.join(".amenbo").is_file(), "the `.amenbo` marker is gone");
        assert!(reg.projects_for_dir(&dir).is_empty(), "the folder drops out of the project reverse index");
        assert!(reg.dirs_for_project(pid).is_empty(), "and the project is left with no folder");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A folder **re-pointed at another project** before the delete keeps everything: the doomed project
    /// still lists the folder here — an index written before re-pointing retracted the old pair, which is
    /// the state this builds by hand — but the pointer on disk names the live project and that is the one
    /// the folder belongs to. Deleting the old project may take its own record of the folder and nothing
    /// else.
    #[test]
    fn a_folder_re_pointed_at_another_project_survives_the_delete() {
        let (mut store, ids) = store_with_projects("repointed", 2);
        let (doomed, live) = (ids[0], ids[1]);
        let tmp = amenbo_scratch::scratch("teardown-repointed");
        let dir = tmp.to_string_lossy().to_string();

        // The folder was bound to `doomed`, then re-pointed at `live`: the pointer names `live` while both
        // projects hold a record of the folder.
        DirBinding::new(Some(live), None).write(&tmp).unwrap();
        std::fs::write(
            tmp.join("CLAUDE.md"),
            format!(
                "# 手引き\n\n<!-- amenbo:begin (managed) -->\n{}\n<!-- amenbo:end -->\n",
                crate::agents::managed_block_body("Japanese", "amenbo")
            ),
        )
        .unwrap();
        let mut reg = store.bindings();
        reg.record_project_ref(doomed, dir.as_str());
        reg.record_project_ref(live, dir.as_str());
        store.save_bindings(&reg).unwrap();

        store.project_delete(doomed, crate::model::ActorKind::Human).unwrap();
        let report = teardown_deleted_project(&store, doomed).unwrap();

        assert_eq!(report.folders_released, 0, "nothing on disk was the deleted project's to clear");
        assert_eq!(report.folders_kept, 1, "the folder is counted as kept, not released");
        assert_eq!(
            crate::binding::read_pointer(&tmp).and_then(|b| b.project_id),
            Some(live),
            "the pointer still names the live project",
        );
        assert!(
            std::fs::read_to_string(tmp.join("CLAUDE.md")).unwrap().contains("amenbo:begin"),
            "and its managed block is still there",
        );
        let reg = store.bindings();
        assert_eq!(reg.dirs_for_project(live), vec![dir.as_str()], "the live project keeps its record");
        assert!(reg.dirs_for_project(doomed).is_empty(), "only the deleted project's own record goes");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// With no pointer to read, the registry decides. A folder no living project claims is the deleted
    /// project's to clear — the managed block goes with it.
    #[test]
    fn a_folder_without_a_pointer_is_released_when_no_live_project_claims_it() {
        let (mut store, ids) = store_with_projects("pointerless", 1);
        let doomed = ids[0];
        let tmp = amenbo_scratch::scratch("teardown-pointerless");
        let dir = tmp.to_string_lossy().to_string();

        // The `.amenbo` is already gone (deleted by hand, a folder restored from a backup): only the
        // registry records that this folder was the doomed project's.
        std::fs::write(
            tmp.join("AGENTS.md"),
            format!(
                "<!-- amenbo:begin (managed) -->\n{}\n<!-- amenbo:end -->\n",
                crate::agents::managed_block_body("Japanese", "amenbo")
            ),
        )
        .unwrap();
        let mut reg = store.bindings();
        reg.record_project_ref(doomed, dir.as_str());
        store.save_bindings(&reg).unwrap();

        store.project_delete(doomed, crate::model::ActorKind::Human).unwrap();
        let report = teardown_deleted_project(&store, doomed).unwrap();

        assert_eq!(report.folders_released, 1, "no other owner, so the folder is released");
        assert_eq!(report.folders_kept, 0);
        assert!(!tmp.join("AGENTS.md").is_file(), "a file that was nothing but the block is removed");
        assert!(store.bindings().dirs_for_project(doomed).is_empty(), "the record is forgotten");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
