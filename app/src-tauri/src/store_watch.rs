//! Let the kernel watch the store directory and wake us only when something is written. This is the
//! only place that touches the OS-specific machinery (macOS: FSEvents / Linux: inotify / Windows:
//! ReadDirectoryChangesW), so it stays **free of tauri**: on a wake-up it just calls the closure the
//! caller handed in, which lets `tests/store_watch.rs` exercise the real OS behaviour on all three.
//! **Never use kernel watching on a network FS**: on NFS the inotify watch *succeeds* yet reports
//! only changes made through our own mount, silently dropping writes from other hosts — so the
//! choice is made up front from the FS type (`is_network_dir`, one cfg'd impl per OS), not from
//! whether the watch could be installed. Emitting to the GUI and deciding whether anything *really*
//! changed belong to the caller (`commands::watch_store`).

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use notify::{Config, PollWatcher, RecursiveMode, Watcher};

/// Coalescing window. A single write makes the OS fire several events (main file, WAL, SHM,
/// rename), so wait for this much quiet after the last one and notify once.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

/// Interval of the stat polling used where a kernel watch cannot be had (containers, NFS, or a
/// store that does not exist yet).
const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(1500);

/// The channel that carries a wake-up. It carries nothing: a watcher says no more than "something
/// moved".
type Wake = ();

/// Have the kernel watch the directory (`None` if no watch can be installed).
///
/// **Watch the directory, not the file**: SQLite checkpoints and `stage_and_swap` replace files, so
/// an inode-level watch loses its target the moment a file is swapped. Watching the directory
/// (non-recursively) catches the main file, the WAL and the ledger alike.
fn spawn_native_watcher(dir: &Path, tx: Sender<Wake>) -> Option<Box<dyn Watcher + Send>> {
    notify::recommended_watcher(handler(tx))
        .ok()
        .and_then(|mut w| w.watch(dir, RecursiveMode::NonRecursive).ok().map(|()| w))
        .map(|w| Box::new(w) as Box<dyn Watcher + Send>)
}

/// The fallback for filesystems where kernel watching does not work (containers, NFS): the same
/// `notify` API, backed by stat polling.
fn spawn_poll_watcher(dir: &Path, tx: Sender<Wake>) -> Option<Box<dyn Watcher + Send>> {
    let config = Config::default().with_poll_interval(WATCH_POLL_INTERVAL);
    PollWatcher::new(handler(tx), config)
        .ok()
        .and_then(|mut w| w.watch(dir, RecursiveMode::NonRecursive).ok().map(|()| w))
        .map(|w| Box::new(w) as Box<dyn Watcher + Send>)
}

/// Which watcher observes the store directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchMode {
    /// Let the kernel wake us (local FS).
    Kernel,
    /// Go and look ourselves, by stat polling (network FS).
    Poll,
}

/// Choose polling up front on a network FS. **Whether the watch installs tells us nothing**: on NFS
/// `inotify_add_watch` *succeeds* (reporting only changes made through our own mount, never the
/// ones the server sees from other hosts), so `or_else` never fires and remote writes are lost
/// forever. A self-probe cannot expose it either, because our own byte does arrive. The FS type is
/// the only thing that can, so decide from it here.
fn watch_mode(dir: &Path) -> WatchMode {
    if is_network_dir(dir) {
        WatchMode::Poll
    } else {
        WatchMode::Kernel
    }
}

/// Whether `dir` sits on a filesystem reached over the network (when that cannot be told, call it
/// local: try the kernel watch and fall back to polling if it will not install).
#[cfg(target_os = "linux")]
fn is_network_dir(dir: &Path) -> bool {
    /// inotify only sees events raised on the local FS, so list the filesystems whose other-host
    /// writes never arrive by their `statfs(2)` magic (compared on the low 32 bits, since `f_type`
    /// is arch-dependent in width): NFS `0x6969`; the old smbfs `0x517B`; the legacy CIFS magic
    /// `0xFF53_4D42` (today's kernels report smb2 unless SMB1 is asked for); SMB2 `0xFE53_4D42`
    /// (where a current `mount -t cifs` lands); AFS `0x5346_414F` and kafs `0x6B41_4653`; Ceph
    /// `0x00C3_6400`; 9P `0x0102_1997` (WSL's drvfs too, where host-side writes do not arrive). When
    /// in doubt, list it: mistaking a network FS for a local one silently drops other hosts' writes,
    /// while the opposite mistake costs no more than polling — the errors are not symmetric.
    const NETWORK_MAGICS: &[u32] = &[
        0x6969,
        0x517B,
        0xFF53_4D42,
        0xFE53_4D42,
        0x5346_414F,
        0x6B41_4653,
        0x00C3_6400,
        0x0102_1997,
    ];
    statfs_f_type(dir).is_some_and(|t| NETWORK_MAGICS.contains(&t))
}

/// The `f_type` reported by `statfs(2)` (`None` if it cannot be read).
#[cfg(target_os = "linux")]
fn statfs_f_type(dir: &Path) -> Option<u32> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a valid NUL-terminated pointer and `st` is writable storage for a `statfs`.
    (unsafe { libc::statfs(path.as_ptr(), &mut st) } == 0).then_some(st.f_type as u64 as u32)
}

/// On macOS the kernel already knows whether the volume is backed locally (`MNT_LOCAL`). FSEvents
/// does not see network volumes (the stream opens, but no event ever comes), so anything that is
/// not local gets polling (when `statfs` will not answer, treat it as local).
#[cfg(target_os = "macos")]
fn is_network_dir(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a valid NUL-terminated pointer and `st` is writable storage for a `statfs`.
    if unsafe { libc::statfs(path.as_ptr(), &mut st) } != 0 {
        return false;
    }
    st.f_flags & (libc::MNT_LOCAL as u32) == 0
}

/// Windows' `ReadDirectoryChangesW` leans on the server's change-notify, so on a remote drive there
/// is no guarantee an event ever arrives. Rather than drop writes, take the coarse but certain path
/// of polling.
#[cfg(target_os = "windows")]
fn is_network_dir(dir: &Path) -> bool {
    use std::path::{Component, Prefix};

    let Some(Component::Prefix(prefix)) = dir.components().next() else {
        return false; // A relative path lives under the process's cwd, which is local.
    };
    match prefix.kind() {
        Prefix::UNC(..) | Prefix::VerbatimUNC(..) => true, // \\server\share
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => is_remote_drive(letter),
        _ => false, // Device names and the like cannot be judged: treat as local.
    }
}

/// Whether the drive letter (`Z:`) is mapped to the network.
#[cfg(target_os = "windows")]
fn is_remote_drive(letter: u8) -> bool {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

    /// A return value of `GetDriveTypeW` (windows-sys 0.59 does not expose the constants; see the
    /// Windows SDK's `winbase.h`).
    const DRIVE_REMOTE: u32 = 4;

    // `GetDriveTypeW` wants the root directory (`Z:\`): with a bare `Z:` it means "the current
    // directory on that drive", and the answer changes.
    let root = [u16::from(letter), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: `root` is a NUL-terminated UTF-16 string.
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
}

/// Any other OS, where we have no way to tell: treat the directory as local and let the watch
/// install decide.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn is_network_dir(_dir: &Path) -> bool {
    false
}

/// The event payload is ignored: all we take from it is "something moved", and whether anything
/// actually changed is answered by the signature (saying *what* changed is not a watcher's job).
fn handler(tx: Sender<Wake>) -> impl Fn(notify::Result<notify::Event>) + Send + 'static {
    move |res| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    }
}

/// Watch the store directory (kernel first, falling back to stat polling where that does not work).
/// `None` if no watcher can be installed, in which case the caller drops to the stat loop in [`run`].
///
/// On a network FS the kernel watch is **not even attempted** ([`watch_mode`]): it would install yet
/// never report other hosts' writes, leaving us convinced it works while writes are lost.
pub fn spawn_store_watcher(dir: &Path, tx: Sender<Wake>) -> Option<Box<dyn Watcher + Send>> {
    match watch_mode(dir) {
        WatchMode::Kernel => {
            spawn_native_watcher(dir, tx.clone()).or_else(|| spawn_poll_watcher(dir, tx))
        }
        WatchMode::Poll => spawn_poll_watcher(dir, tx),
    }
}

/// Call `on_wake` every time something happens under `dir` (**never returns**: the caller runs this
/// on a thread of its own). **The kernel does the waking**, so we sleep outright while idle and come
/// up the moment a write lands. Where no watch can be installed (`dir` is `None`, the FS does not
/// support it, the watcher died) it degrades to polling, so the behaviour gets coarser but never
/// stops. `on_wake` means only "we woke up": whether anything **really** changed is for the caller
/// to confirm (`store_signature_string` = `PRAGMA data_version` plus file identity). A file watch
/// also fires on touches that mean nothing (a read updating the SHM, say), and that check is what
/// drops the pointless emits.
pub fn run(dir: Option<&Path>, mut on_wake: impl FnMut()) {
    let (tx, rx) = std::sync::mpsc::channel::<Wake>();
    // `_watcher` must be kept alive: dropping it tears the watch down.
    let _watcher = dir.and_then(|d| spawn_store_watcher(d, tx));

    if _watcher.is_some() {
        watch_loop(&rx, &mut on_wake);
    }

    // Fallback: neither the kernel watch nor the polling watcher could be installed (no store yet),
    // or the watcher died.
    loop {
        std::thread::sleep(WATCH_POLL_INTERVAL);
        on_wake();
    }
}

/// Take wake-ups and coalesce them (returns once the watcher dies, dropping the caller to its stat
/// loop).
fn watch_loop(rx: &Receiver<Wake>, on_wake: &mut impl FnMut()) {
    loop {
        if rx.recv().is_err() {
            return; // The watcher died (the sender is gone): degrade to polling.
        }
        // Coalesce the burst of events a single write fires into one notification (wait for quiet).
        while rx.recv_timeout(WATCH_DEBOUNCE).is_ok() {}
        on_wake();
    }
}

/// The way in for the tests (`tests/store_watch.rs`) to exercise the fallback path, the poll
/// interval and the FS-type check for real. Production code reaches them only through
/// [`spawn_store_watcher`].
#[doc(hidden)]
pub mod testonly {
    use super::*;

    pub const POLL_INTERVAL: Duration = WATCH_POLL_INTERVAL;

    /// The fallback path itself: where `spawn_store_watcher` lands on a filesystem that kernel
    /// watching does not work on.
    pub fn spawn_poll_watcher(dir: &Path, tx: Sender<Wake>) -> Option<Box<dyn Watcher + Send>> {
        super::spawn_poll_watcher(dir, tx)
    }

    /// Whether `spawn_store_watcher` picks polling for `dir` — that is, whether it read it as a
    /// network FS.
    pub fn picks_poll_watcher(dir: &Path) -> bool {
        super::watch_mode(dir) == WatchMode::Poll
    }
}
