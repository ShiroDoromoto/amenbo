//! The throwaway store a verification run works in, by the same three rules as the
//! workspace's `amenbo-scratch` — one parent, a name whose uniqueness does not rest on the
//! pid, and a sweep on the way *in*. Reimplemented here rather than
//! shared, so the verification workspace stays independent of the main one (it drives the
//! shipped binary as a black box).
//!
//! The isolation an amenbo run needs is two things, both required: `AMENBO_HOME` pointed at a
//! throwaway dir (the ONLY thing that keeps a run out of the real app-data tree — an isolated
//! CWD alone does not, since `init` with no `.amenbo` in sight creates a store under the real
//! root), and a CWD with no `.amenbo` ancestor. One [`session`] hands back both.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// How long a leftover is kept before the next run sweeps it — long enough to still be reading
/// yesterday's failure, short enough that nothing piles up.
const KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// The one parent every throwaway dir sits under, so the sweep can only ever reach dirs this
/// driver made — never a neighbour's files under `temp_dir()`.
fn root() -> PathBuf {
    std::env::temp_dir().join("amenbo-verify")
}

/// A fresh, isolated store for one run: an `AMENBO_HOME` and a `.amenbo`-free CWD, both under
/// one throwaway parent. The parent is created; the caller runs the binary with these two.
pub struct Session {
    pub home: PathBuf,
    pub cwd: PathBuf,
    keep: bool,
    base: PathBuf,
}

/// Create a fresh isolated session named `<tag>-<pid>-<nanos>-<n>`. Uniqueness rests on the
/// wall clock and the counter, not on the pid: the OS recycles ids, so a name leaning on one
/// hands two runs the same path — and a caller wiping such a path on its way in reaches a
/// *live* run's dir. The clock separates runs, the counter separates calls within a run, and
/// the pid only separates two runs starting in one nanosecond. `keep` leaves the base in place
/// for inspection.
pub fn session(tag: &str, keep: bool) -> std::io::Result<Session> {
    sweep_once();
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let base = root().join(format!("{tag}-{:x}-{nanos:x}-{n:x}", std::process::id()));
    let home = base.join("home");
    let cwd = base.join("cwd");
    std::fs::create_dir_all(&home)?;
    std::fs::create_dir_all(&cwd)?;
    Ok(Session { home, cwd, keep, base })
}

impl Session {
    /// A folder for a step that needs a **second** directory — one to bind, resync or unbind. It is
    /// created beside [`Session::cwd`] rather than inside it, on purpose: a `.amenbo` pointer is
    /// found by walking *up*, so a folder under the run's own bound CWD would read as bound before
    /// anything bound it, and would still read as bound after it was unbound.
    pub fn folder(&self, name: &str) -> std::io::Result<PathBuf> {
        let dir = self.base.join("folders").join(name);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Best-effort tidy on the way out; the start-of-run sweep is the real guarantee, so a
        // killed run still gets collected by whoever runs next.
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.base);
        }
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

    /// Back-to-back sessions never collide, each carries its own home + `.amenbo`-free cwd, and
    /// both sit under the one parent.
    #[test]
    fn each_session_is_unique_and_isolated() {
        let sessions: Vec<Session> = (0..16).map(|_| session("selftest", true).unwrap()).collect();
        let homes: std::collections::HashSet<&PathBuf> = sessions.iter().map(|s| &s.home).collect();
        assert_eq!(homes.len(), sessions.len(), "no two sessions share a home");
        for s in &sessions {
            assert!(s.home.is_dir() && s.cwd.is_dir(), "both dirs exist");
            assert_ne!(s.home, s.cwd, "home and cwd are separate");
            assert!(!s.cwd.join(".amenbo").exists(), "the cwd carries no .amenbo ancestor");
            assert_eq!(s.base.parent(), Some(root().as_path()), "under the one parent");
        }
        for s in sessions {
            let base = s.base.clone();
            drop(s); // keep=true, so the base stays for the human, but we tidy the selftest ones
            let _ = std::fs::remove_dir_all(base);
        }
    }

    /// A second folder lands beside the run's CWD, never under it — under it, the CWD's own pointer
    /// would answer for it and every binding assert would read true before anything was bound.
    #[test]
    fn a_named_folder_sits_beside_the_cwd_and_answers_to_its_name() {
        let s = session("selftest-folder", true).unwrap();
        let dir = s.folder("shared").unwrap();
        assert!(dir.is_dir(), "the folder is there to be bound");
        assert!(!dir.starts_with(&s.cwd), "outside the run's own bound CWD");
        assert!(dir.starts_with(&s.base), "under the session's own parent");
        assert_eq!(dir, s.folder("shared").unwrap(), "one name, one folder");
        assert_ne!(dir, s.folder("other").unwrap(), "two names, two folders");

        let base = s.base.clone();
        drop(s);
        let _ = std::fs::remove_dir_all(base);
    }

    /// Age decides, and nothing else. Pointed at a parent of its own so it never reaches another
    /// test process's live dir.
    #[test]
    fn the_sweep_takes_what_is_older_than_the_cutoff_and_leaves_the_rest() {
        let own = session("selftest-sweep", true).unwrap();
        let entry = own.base.join("leftover");
        std::fs::create_dir_all(&entry).unwrap();

        sweep_older_than(&own.base, SystemTime::now() - KEEP);
        assert!(entry.is_dir(), "a directory made just now is inside the keep window");

        sweep_older_than(&own.base, SystemTime::now() + Duration::from_secs(60));
        assert!(!entry.exists(), "past the cutoff it goes");

        let base = own.base.clone();
        drop(own);
        let _ = std::fs::remove_dir_all(base);
    }
}
