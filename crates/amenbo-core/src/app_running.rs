//! Whether the app is up on this store — the question a CLI command asks before it hands the app
//! something only the app can carry out.
//!
//! `automation start` is the one that asks today (`AMB-D-995`): a step's terminal is opened by the
//! app, so a run started from the CLI while the app is closed stands `running` with nothing moving,
//! and the next launch's sweep ends it `crashed` with no reason the person who typed it can see.
//!
//! The mark is an **advisory lock** the app holds on `<base>/app.running.lock` for as long as it
//! runs, not a file whose presence is the answer. A file alone says only that an app once started:
//! an app that crashed, or was killed, never gets to remove it. A process id written into it would
//! need a liveness probe that is different on every OS and that a reused id fools. The lock is
//! released by the OS the moment the process is gone, however it went, so the answer is read off the
//! lock ([`is_running`]) and the file is only where the lock lives.
//!
//! One file per store directory, beside the store, for the same reason the swap lock is
//! (`crate::swap_lock`): a development build and a store named by `AMENBO_HOME` each have a base of
//! their own, so an app up on one is never taken for an app up on another.
//!
//! The descriptor does not reach the processes the app starts. Rust opens files close-on-exec, so a
//! terminal's shell or an agent in a pane holds no copy of it, and a pane left running after the app
//! is gone does not keep the lock taken.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::PathBuf;

use crate::config::Paths;
use crate::error::Result;

/// Fixed name of the mark, directly under the store's base directory.
pub const FILE_NAME: &str = "app.running.lock";

/// Where the mark for the store at `paths` lives.
pub fn mark_path(paths: &Paths) -> PathBuf {
    paths.base_dir.join(FILE_NAME)
}

/// The app's claim on the mark, held for the life of the process. [`release`](Self::release) is how
/// it is let go on an orderly exit; a process that ends any other way lets the OS release it.
#[derive(Debug)]
#[must_use = "dropping the claim releases the mark, and the app then reads as not running"]
pub struct Presence {
    file: File,
    path: PathBuf,
}

impl Presence {
    /// Let the mark go and take the file away, so an orderly exit leaves nothing behind. The unlock
    /// is asked for rather than left to the close, for the reason `crate::swap_lock` gives: on macOS
    /// a lock outlives its close by up to a few hundred microseconds. The removal is best-effort —
    /// a file left behind unlocked still reads as not running.
    pub fn release(self) {
        let _ = self.file.unlock();
        drop(self.file);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Take the mark for the store at `paths`. `Ok(None)` when another process already holds it — two
/// apps on one store are kept apart elsewhere (`single_instance` in the app), so the one that got
/// here second has nothing to add: the store already reads as running.
pub fn claim(paths: &Paths) -> Result<Option<Presence>> {
    let path = mark_path(paths);
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(Presence { file, path })),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e.into()),
    }
}

/// Whether an app is up on the store at `paths` right now.
///
/// No file is no app. A file whose lock can be taken is an app that ended without removing it —
/// crashed or killed — and is no app either. Only a lock someone else holds is an app. The probe
/// takes the lock shared and lets it go at once, so two CLIs asking together do not turn each other
/// away, and it writes nothing: a store with no app never gains the file from being asked.
pub fn is_running(paths: &Paths) -> Result<bool> {
    let file = match File::open(mark_path(paths)) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            Ok(false)
        }
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(e)) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(tag: &str) -> Paths {
        Paths::at(amenbo_scratch::scratch(&format!("app-running-{tag}")))
    }

    /// A store no app has ever opened reads as not running, and asking leaves no file behind.
    #[test]
    fn no_mark_is_not_running() {
        let paths = paths("no-mark");
        assert!(!is_running(&paths).unwrap());
        assert!(!mark_path(&paths).exists());
    }

    /// While the claim is held the store reads as running; once released it does not, and the file
    /// is gone.
    #[test]
    fn a_held_claim_is_running_until_released() {
        let paths = paths("held");
        let presence = claim(&paths).unwrap().expect("the first claim takes the mark");
        assert!(is_running(&paths).unwrap());
        presence.release();
        assert!(!is_running(&paths).unwrap());
        assert!(!mark_path(&paths).exists());
    }

    /// An app that ended without releasing — crashed or killed — leaves the file but not the lock,
    /// and that reads as not running. Dropping the claim without `release` is that ending: the
    /// descriptor closes and the lock goes with it, the way it does when the process dies.
    #[test]
    fn a_mark_left_behind_is_not_running() {
        let paths = paths("left-behind");
        drop(claim(&paths).unwrap().expect("the first claim takes the mark"));
        assert!(mark_path(&paths).exists());
        assert!(!is_running(&paths).unwrap());
    }

    /// A second claim on a store already claimed comes back empty rather than failing, and does not
    /// disturb the first.
    #[test]
    fn a_second_claim_finds_the_mark_taken() {
        let paths = paths("second");
        let first = claim(&paths).unwrap().expect("the first claim takes the mark");
        assert!(claim(&paths).unwrap().is_none());
        assert!(is_running(&paths).unwrap());
        first.release();
    }
}
