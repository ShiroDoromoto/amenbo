//! The short-lived exclusion that guards a truth-source **file swap**.
//!
//! Write serialisation is delegated to SQLite's own writer lock, which is held on the database
//! **inode** for the length of a transaction. That is the right call for writes — but a swap
//! (`restore`, and a migration) does not write *through* a connection, it **replaces the file
//! underneath**, and an inode lock cannot reach an operation that swaps the inode out. So the swap
//! gets its own exclusion, scoped to the swap window (a few hundred ms) rather than to the whole
//! command:
//!
//! - The **swapping** side ([`hold_for_swap`]) takes the lock **exclusive** for the length of the
//!   replace, so no open reads a store mid-swap.
//! - The **opening** side ([`guard_write_open`] / [`guard_read_open`]) takes it **shared**, only for the
//!   length of the open, and fails fast with `store_busy` (rather than reading a half-replaced store) if a
//!   swap holds it exclusive. It does **not** hold the lock for the store's lifetime — an open that
//!   succeeds and *then* races a swap lands its write on the old inode or the aside, never on an empty DB
//!   (a single atomic rename pairs with this so no absence window exists; see
//!   [`crate::archive::replace_truth_source`]). What the lock guarantees is that nothing is *corrupted*,
//!   not that nothing is *lost*.
//!
//! The lock is a zero-byte sidecar `store.swap.lock` beside the truth source, one per store directory, so
//! it covers whichever of `store.sqlite` / `oplog.sqlite` is live. A [`SwapGuard`] **unlocks explicitly**
//! before it closes the fd — see its `Drop`.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Fixed name of the swap-lock sidecar, kept beside the truth source in the store's directory.
pub const SWAP_LOCK_NAME: &str = "store.swap.lock";

/// The swap-lock path for the store whose truth source is `db_path` (its directory's `store.swap.lock`).
/// Directory-scoped, so it names the same lock whichever truth-source filename is live
/// (`store.sqlite` or `oplog.sqlite`).
pub fn lock_path(db_path: &Path) -> PathBuf {
    db_path.with_file_name(SWAP_LOCK_NAME)
}

/// An advisory file lock held for the length of a swap or an open. Dropping it releases the lock.
#[derive(Debug)]
#[must_use = "dropping the guard immediately releases the swap lock"]
pub struct SwapGuard {
    file: File,
}

/// Release the lock **by asking**, not by closing the fd.
///
/// Closing does release it, but not in time: on macOS a `try_lock` issued right after the close still sees
/// the lock for tens to hundreds of microseconds (measured 65–400 µs on a loaded machine). That window is
/// enough for the open that follows a restore — same thread, the next statement — to come back
/// `store_busy` when no swap is underway at all. An explicit unlock is a synchronous syscall, so the lock
/// is gone the moment the guard drops and a spurious `store_busy` cannot be minted.
///
/// A failure here is not worth surfacing: it means the fd is already gone, which is the state this is
/// trying to reach.
impl Drop for SwapGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// The bilingual `store_busy` a contended open surfaces: the store is mid-swap, so retrying in a
/// moment succeeds. Its stable code (`store_busy`) is in the retryable set.
fn busy() -> Error {
    Error::store_busy(
        "the store is being restored or migrated; try again in a moment",
        "ストアの復元／移行の最中です。少し待ってから再実行してください",
    )
}

/// Translate a non-blocking lock attempt's failure: real contention (someone holds the lock) becomes the
/// bilingual `store_busy`; a genuine I/O fault propagates as itself. `std::fs`'s advisory file locking
/// distinguishes the two in [`TryLockError`], so this needs no platform-specific errno comparison.
fn try_shared(file: File) -> Result<SwapGuard> {
    match file.try_lock_shared() {
        Ok(()) => Ok(SwapGuard { file }),
        Err(TryLockError::WouldBlock) => Err(busy()),
        Err(TryLockError::Error(e)) => Err(Error::from(e)),
    }
}

/// Take the store's swap lock **exclusively**, for the file-replacing side (`restore`, and the
/// migration's chain and rollback). Creates the lock sidecar if absent (the swapping side always may
/// write). Blocks until any in-flight opens — which hold the lock shared only for the length of their
/// open — release, then keeps every new open out until the returned guard drops.
pub fn hold_for_swap(db_path: &Path) -> Result<SwapGuard> {
    hold_exclusive(&lock_path(db_path))
}

/// Take an exclusive advisory lock on `path` (creating the file if absent), **blocking** until it is
/// free. The primitive under [`hold_for_swap`], and under the migration's own sidecar
/// ([`crate::migrate::migration_lock_path`]) — which must be a *different* file: the migration holds its
/// lock across a run that itself takes the swap lock, and an flock is per open file description, so the
/// same process nesting one inside the other on one file would deadlock against itself.
pub(crate) fn hold_exclusive(path: &Path) -> Result<SwapGuard> {
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
    file.lock()?;
    Ok(SwapGuard { file })
}

/// Take the store's swap lock **shared**, for the write-open side (which is allowed to create the lock
/// sidecar). Returns `store_busy` — without opening the store — when a swap holds it exclusive. Meant to
/// be held only for the length of the open (bind it to a local that drops when the open returns).
pub fn guard_write_open(db_path: &Path) -> Result<SwapGuard> {
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path(db_path))?;
    try_shared(file)
}

/// Take the store's swap lock **shared**, for the read-open side, which writes nothing to disk — so it
/// never *creates* the sidecar. An absent sidecar means no swap can be underway (the swapping side
/// creates it before it locks), so the open proceeds unguarded (`Ok(None)`). When the sidecar exists,
/// this behaves like [`guard_write_open`]: `Ok(Some(guard))`, or `store_busy` if a swap holds it.
pub fn guard_read_open(db_path: &Path) -> Result<Option<SwapGuard>> {
    match OpenOptions::new().read(true).open(lock_path(db_path)) {
        Ok(file) => try_shared(file).map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::from(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = amenbo_scratch::scratch(&format!("swaplock-{tag}"));
        dir
    }

    /// The lock sits beside the truth source, directory-scoped, under the fixed name.
    #[test]
    fn lock_path_is_the_sidecar_beside_the_store() {
        let db = Path::new("/data/stores/x/store.sqlite");
        assert_eq!(lock_path(db), Path::new("/data/stores/x/store.swap.lock"));
        // Same lock whichever truth-source filename is live.
        let legacy = Path::new("/data/stores/x/oplog.sqlite");
        assert_eq!(lock_path(legacy), lock_path(db));
    }

    /// A read open never creates the sidecar: absent ⇒ `None` (proceed unguarded), and nothing is written.
    #[test]
    fn read_open_does_not_create_the_lock_file() {
        let dir = scratch("read-nocreate");
        let db = dir.join("store.sqlite");
        assert!(guard_read_open(&db).unwrap().is_none());
        assert!(!lock_path(&db).exists(), "the read path wrote nothing to disk");
    }

    /// A held exclusive swap lock turns every concurrent open into `store_busy` (both open flavours),
    /// and releasing it lets opens back in.
    #[test]
    fn a_swap_locks_opens_out_then_lets_them_back() {
        let dir = scratch("busy");
        let db = dir.join("store.sqlite");

        let held = hold_for_swap(&db).unwrap();
        assert_eq!(guard_write_open(&db).unwrap_err().code(), "store_busy");
        // The sidecar now exists (the swap created it), so the read path sees it and also refuses.
        assert_eq!(guard_read_open(&db).unwrap_err().code(), "store_busy");

        drop(held);
        // Uncontended now: both opens succeed.
        let _reopened = guard_write_open(&db).expect("write open after the swap released");
        assert!(guard_read_open(&db).unwrap().is_some(), "read open after the swap released");
    }

    /// The release is **prompt**, not eventual: the open that follows a swap — same thread, the next
    /// statement, which is exactly what a restore's caller does — succeeds on its first try, every time.
    /// A lock left to the fd's close instead of released by asking lingers for microseconds after the
    /// guard is gone, and an open landing in that window is told the store is busy while nothing is
    /// swapping it. The repetition is what gives such a lag a chance to show.
    #[test]
    fn an_open_right_after_a_swap_released_is_never_told_the_store_is_busy() {
        let dir = scratch("prompt-release");
        let db = dir.join("store.sqlite");
        for i in 0..500 {
            drop(hold_for_swap(&db).unwrap());
            let opened = guard_write_open(&db);
            assert!(opened.is_ok(), "open {i}, right after the swap released: {opened:?}");
        }
    }

    /// Two opens can hold the shared lock at once — readers never block each other, only a swap blocks them.
    #[test]
    fn shared_opens_do_not_exclude_each_other() {
        let dir = scratch("shared");
        let db = dir.join("store.sqlite");
        let a = guard_write_open(&db).unwrap();
        let b = guard_write_open(&db).expect("a second shared open coexists with the first");
        drop((a, b));
    }
}
