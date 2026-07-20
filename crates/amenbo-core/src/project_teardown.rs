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
    /// Blobs reclaimed — the bytes of attachments the delete dropped to zero references.
    pub blobs_reclaimed: u64,
    /// Bytes reclaimed.
    pub bytes_freed: u64,
}

/// The destructive teardown of a deleted project:
///
/// 1. Release the binding of **every folder** that pointed at it — remove `.amenbo`, take the managed block
///    out, forget the folder in the bindings registry. Folders bound to other, living projects are left
///    alone.
/// 2. Reclaim the blobs that lost their last reference ([`Store::gc_blobs`]). The delete itself has already
///    reclaimed the blobs it orphaned (`Store::reclaim_blobs`), so this full sweep only picks up **what got
///    past that**: blobs still too young to reclaim at the time (`GC_MIN_AGE`), or bytes stranded by an
///    interrupted delete or restore. A project deletion is a good moment to sweep the whole store, so the
///    catch-all lives here — the same cleaning `doctor --fix` does.
///
/// The binding registry is a table in the store, so it is read and written through the open `store`.
pub fn teardown_deleted_project(store: &Store, project_id: i64) -> Result<TeardownReport> {
    let mut registry = store.bindings();

    // Collect into `String`s first, for ownership: `forget_dir` borrows the registry mutably.
    let dirs: Vec<String> =
        registry.dirs_for_project(project_id).into_iter().map(str::to_string).collect();

    let mut report = TeardownReport::default();
    for dir in &dirs {
        release_folder(dir, &mut registry);
        report.folders_released += 1;
    }
    // Save the registry: `forget_dir` has changed its indexes, and the unbinding has to stick rather than
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
/// bindings. The registered string is the path as canonicalised at bind time, so — in case the path handed
/// in here is not the canonical one — forget the canonical form too (the same shape `unbind` has).
fn release_folder(dir: &str, registry: &mut Registry) {
    let path = Path::new(dir);
    let marker = path.join(".amenbo");
    if marker.is_file() {
        let _ = std::fs::remove_file(&marker);
    }
    // Take the managed block out of AGENTS.md / CLAUDE.md. The user's own content outside the markers is
    // kept; a file that was nothing but the block is removed.
    let _ = crate::agents::remove_from_dir(path);
    registry.forget_dir(dir);
    if let Ok(canon) = std::fs::canonicalize(path) {
        let canon_str = canon.to_string_lossy().to_string();
        if canon_str != dir {
            registry.forget_dir(&canon_str);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::DirBinding;

    /// Releasing folder bindings is the work the teardown is left with; this is one folder's worth of it —
    /// `.amenbo` removed, and the folder forgotten in the bindings indexes.
    #[test]
    fn release_folder_removes_pointer_marker_and_forgets_binding() {
        let tmp = amenbo_scratch::scratch("teardown");
        let dir = tmp.to_string_lossy().to_string();

        // Set up the `.amenbo` pointer and the bindings entry.
        DirBinding::new(Some(7), Some("proj-x".into())).write(&tmp).unwrap();
        let mut reg = Registry::default();
        reg.record_project_ref(7, dir.as_str());
        reg.set(7, dir.as_str());
        assert!(!reg.projects_for_dir(&dir).is_empty());

        release_folder(&dir, &mut reg);

        assert!(!tmp.join(".amenbo").is_file(), "the `.amenbo` marker is gone");
        assert!(reg.projects_for_dir(&dir).is_empty(), "the folder drops out of the project reverse index");
        assert_eq!(reg.get(7), None, "and out of the primary directory registration too");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
