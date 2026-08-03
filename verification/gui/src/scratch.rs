//! The throwaway store the GUI under test is launched against.
//!
//! A screen road creates projects, tasks and bindings, so a run pointed at the store the operator
//! actually uses writes verification leftovers into their own backlog, and the tidying is left to
//! whoever remembers. Pointing the app at a store of the run's own settles that at the launch
//! rather than in a note: there is nothing to remember, because there is nothing left behind.
//!
//! Isolation is two things, and the app is given both: `AMENBO_HOME` at a directory this run made,
//! which is what decides where the store goes, and a working directory of the run's own, since a
//! child inherits the harness's and the harness is run from the repository — a folder that carries
//! a pointer to a real project.
//!
//! The three rules are the workspace's own — one parent, a name whose uniqueness does not rest on
//! the pid, and a sweep on the way *in*. They are written out here rather than borrowed from the
//! CLI driver beside it: that one hands its runs an artifacts directory and named folders to bind,
//! which a screen road reaches through the screen, and a harness that pulled in the whole CLI
//! driver for a temp directory would carry its every dependency to get one.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// How long a leftover is kept before the next run sweeps it — long enough to still be reading
/// yesterday's failure, short enough that nothing piles up.
const KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// The one parent every throwaway store sits under, so the sweep can only ever reach directories
/// this harness made — never a neighbour's files under `temp_dir()`. It sits beside the evidence
/// dirs rather than among them: evidence is what a run leaves to be read, a store is what it leaves
/// to be dropped.
fn root() -> PathBuf {
    std::env::temp_dir().join("amenbo-verify-gui").join("stores")
}

/// A store no user owns, and a directory to launch the app from: the two the shipped build is given
/// so that a run reaches neither the real app-data nor the project the harness itself stands in.
#[derive(Debug)]
pub struct Store {
    /// What `AMENBO_HOME` is pointed at — the whole user layer of the app under test.
    pub home: PathBuf,
    /// What the app is launched from, carrying no pointer to any project.
    pub cwd: PathBuf,
    base: PathBuf,
}

/// Create a fresh store named `<tag>-<pid>-<nanos>-<n>`. Uniqueness rests on the wall clock and the
/// counter, not on the pid: the OS recycles ids, so a name leaning on one hands two runs the same
/// path — and a caller wiping such a path on its way in reaches a *live* run's store. The clock
/// separates runs, the counter separates calls within a run, and the pid only separates two runs
/// starting in one nanosecond.
pub fn store(tag: &str) -> std::io::Result<Store> {
    sweep_once();
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let base = root().join(format!("{tag}-{:x}-{nanos:x}-{n:x}", std::process::id()));
    let home = base.join("home");
    let cwd = base.join("cwd");
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(&cwd)?;
    Ok(Store { home, cwd, base })
}

impl Drop for Store {
    fn drop(&mut self) {
        // Best-effort tidy on the way out; the start-of-run sweep is the real guarantee, so a
        // killed run still gets collected by whoever runs next.
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Sweep on the way in, not on the way out, and once per process: a `Drop` never runs when the
/// process is killed, and one that did would take the wreckage of a failure with it.
fn sweep_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| sweep_older_than(&root(), SystemTime::now() - KEEP));
}

/// Drop every entry under `dir` last modified before `cutoff`. Age is the whole rule: a run
/// happening right now is inside the window, so a parallel run is never touched.
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

    /// Back-to-back stores never collide, each carries its own home and its own launch directory,
    /// and all of them sit under the one parent.
    #[test]
    fn each_store_is_unique_and_isolated() {
        let stores: Vec<Store> = (0..16).map(|_| store("selftest").unwrap()).collect();
        let homes: std::collections::HashSet<&PathBuf> = stores.iter().map(|s| &s.home).collect();
        assert_eq!(homes.len(), stores.len(), "no two runs share a home");
        for s in &stores {
            assert!(s.home.is_dir() && s.cwd.is_dir(), "both dirs exist for the app to be given");
            assert_ne!(s.home, s.cwd, "the store and the directory it is launched from are separate");
            assert!(!s.cwd.join(".amenbo").exists(), "nothing points the launch dir at a project");
            assert_eq!(s.base.parent(), Some(root().as_path()), "under the one parent");
        }
    }

    /// The store goes out with the run that made it — what the app wrote is not something to leave
    /// on the machine.
    #[test]
    fn the_store_goes_when_the_run_does() {
        let s = store("selftest-drop").unwrap();
        let base = s.base.clone();
        std::fs::write(s.home.join("something"), "the app wrote this").unwrap();
        drop(s);
        assert!(!base.exists(), "the whole store is gone");
    }

    /// Age decides, and nothing else. Pointed at a parent of its own so it never reaches another
    /// test process's live store.
    #[test]
    fn the_sweep_takes_what_is_older_than_the_cutoff_and_leaves_the_rest() {
        let own = store("selftest-sweep").unwrap();
        let entry = own.base.join("leftover");
        std::fs::create_dir_all(&entry).unwrap();

        sweep_older_than(&own.base, SystemTime::now() - KEEP);
        assert!(entry.is_dir(), "a directory made just now is inside the keep window");

        sweep_older_than(&own.base, SystemTime::now() + Duration::from_secs(60));
        assert!(!entry.exists(), "past the cutoff it goes");
    }
}
