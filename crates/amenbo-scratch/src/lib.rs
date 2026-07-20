//! The throwaway directory a test works in.
//!
//! Every crate's tests reach the same three rules through this one function: one parent, a name whose
//! uniqueness does not rest on the pid, and a sweep on the way *in*. It is a dev-dependency everywhere and
//! is never linked into a shipped binary.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Once;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// How long a leftover is kept before the next run sweeps it. A day is long enough to still be reading
/// yesterday's failure, and short enough that nothing piles up.
const KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// The one parent every throwaway directory sits under, so the sweep can only ever reach directories the
/// tests made — never a neighbour's files under `temp_dir()`.
pub fn root() -> PathBuf {
    std::env::temp_dir().join("amenbo-test")
}

/// A fresh, empty directory nobody else holds, named `<tag>-<pid>-<nanos>-<n>`. The directory is created;
/// `tag` only says which test it belongs to, so a human reading the parent can tell them apart.
///
/// Uniqueness rests on the wall clock and the counter, not on the pid: the OS recycles ids, so a name
/// that leans on one hands two runs the same path — and a caller that wipes such a path on its way in
/// reaches a *live* run's working directory rather than any wreckage. The clock separates runs, the
/// counter separates calls within a run, and the pid only separates two runs starting in one nanosecond.
pub fn scratch(tag: &str) -> PathBuf {
    sweep_once();
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = root().join(format!("{tag}-{:x}-{nanos:x}-{n:x}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

/// Sweep on the way in, not on the way out, and once per process.
///
/// A `Drop` guard never runs when nextest kills a hung test (nor under Ctrl-C or `process::exit`), and one
/// that did run would take the wreckage of a failure with it — which is the thing one wants to read
/// afterwards. Leaving the job to whoever runs next survives both and caps what accumulates at [`KEEP`].
fn sweep_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| sweep_older_than(&root(), SystemTime::now() - KEEP));
}

/// Drop every entry under `dir` last modified before `cutoff`. Age is the whole rule: a run happening
/// right now is inside the window, so a parallel run is never touched.
fn sweep_older_than(dir: &Path, cutoff: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let stale = entry.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < cutoff);
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Back-to-back calls never collide, and each lands under the one parent.
    #[test]
    fn each_call_gets_its_own_directory_under_the_root() {
        let dirs: Vec<PathBuf> = (0..32).map(|_| scratch("selftest")).collect();
        let unique: std::collections::HashSet<&PathBuf> = dirs.iter().collect();
        assert_eq!(unique.len(), dirs.len(), "no two calls share a path");
        assert!(dirs.iter().all(|d| d.is_dir()), "each path is a directory that exists");
        assert!(dirs.iter().all(|d| d.parent() == Some(root().as_path())), "all under one parent");
    }

    /// Age decides, and nothing else. The sweep runs against a parent of its own here: pointed at the real
    /// root with a cutoff in the future it would take another test process's live directory with it.
    #[test]
    fn the_sweep_takes_what_is_older_than_the_cutoff_and_leaves_the_rest() {
        let own_root = scratch("selftest-sweep");
        let entry = own_root.join("leftover");
        std::fs::create_dir_all(&entry).unwrap();

        sweep_older_than(&own_root, SystemTime::now() - KEEP);
        assert!(entry.is_dir(), "a directory made just now is inside the keep window");

        sweep_older_than(&own_root, SystemTime::now() + Duration::from_secs(60));
        assert!(!entry.exists(), "past the cutoff it goes");
    }
}
