//! Being told what changed in the folder, rather than going to look (`AMB-T-3604`).
//!
//! The file face asked a walk whenever it could think of a reason to (`crate::folder`), which is a
//! guess about when a person is looking and a walk they pay for whether or not anything moved. Here
//! the kernel does the waking, and the face is told.
//!
//! **Four things make this different from watching a store directory** ([`crate::store_watch`]),
//! which watches one folder that holds a handful of files:
//!
//! 1. **How many watches a folder takes is the OS's answer, not this module's** (`AMB-D-779`).
//!    macOS and Windows can cover a whole tree with one, so a folder gets exactly one; Linux has no
//!    such thing — asking for a recursive watch there walks the tree and adds one per folder anyway
//!    — so the pruned list of folders is watched one by one, as it always was. Neither side is a
//!    preference. FSEvents stops at 4,096 paths per stream and `notify` 8.2 answers `Ok` past it
//!    without ever firing again, and one repository of 3,760 folders is already inside the last few
//!    hundred of that (`AMB-T-3752` measured where the silence starts). Recursion on Linux
//!    instead costs 76 times as many inotify watches as the pruned list does, out of a supply
//!    counted per user and shared with whatever editor the reader has open.
//! 2. **What the events say is read for where, never for what.** macOS calls an append a `Create`,
//!    so new, changed and gone cannot be told apart from the event kind — and none of the three
//!    would be trustworthy anyway on a burst that arrived out of order. A wake-up still means only
//!    "something moved", and what changed is worked out by scanning again and comparing with what
//!    was held. The path is the exception: where one watch covers the tree, the kernel reports the
//!    build output the walk prunes away — 100% of what a 47-second build fired (`AMB-T-3752`) — and
//!    pruning it is now something this does on arrival rather than by declining to watch it
//!    ([`crate::folder::pruned`]). ⚠ That has to stay cheap: 50 µs an event took Windows from 0.1%
//!    of events missed to 69% (`AMB-T-3753`).
//! 3. **A kernel that dropped events says so, and that is not a burst to settle.** FSEvents'
//!    `MustScanSubDirs` and inotify's `Q_OVERFLOW` do not mean "something moved" but "what moved is
//!    not known", so they are not held back for the quiet that follows: nothing arriving after can
//!    make the answer any less unknown. Windows sends no such signal at all, which is why nothing
//!    here waits for one.
//! 4. **A watch that could not be installed is said out loud.** The kernel's watch limit is per
//!    user (inotify's `max_user_watches`), and hitting it does not stop the ones already installed:
//!    the answer is a real but partial watch, and a face that drew it as a whole one would be
//!    telling the reader that nothing has changed in the half nobody is looking at.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tauri::Emitter as _;

use crate::dto::FolderChangesDto;
use crate::error::CmdError;

/// How long to wait for quiet before scanning again. One write fires three or four events (the
/// file, its directory, an editor's temporary file and the rename over it), and a formatter or a
/// build fires them in bursts — so the scan is what the burst costs, once.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// How often the thread looks up from waiting to ask whether it is still wanted. Nothing depends on
/// this being short: it is how long the thread outlives the face that asked for it, not how long a
/// change takes to arrive.
const HEARTBEAT: Duration = Duration::from_millis(500);

/// The event the webview listens for. It carries the whole list rather than what moved: the face
/// draws a list, and a delta it had to apply would be a second copy of the truth to keep in step.
const CHANGED_EVENT: &str = "folder://changed";

/// Whether one watch covers everything under the folder it is put on. The line four other products
/// draw, in the spelling Zed writes it in (`AMB-T-3752`, `AMB-D-779`); the reasoning is at the top.
const RECURSIVE: bool = cfg!(any(target_os = "windows", target_os = "macos"));

/// What a wake-up carries. Not which file — that is still never read (see the note at the top) —
/// only whether what moved is known to be somewhere under the root, or is not known at all.
enum Wake {
    /// Something under the root moved. Part of a burst, worth waiting out.
    Moved,
    /// The kernel dropped events and is saying so. Not a burst: acted on where it lands.
    Rescan,
}

/// The watch this app has running, if any. One per app, because one face draws it: asking for a
/// second root replaces the first rather than adding to it.
#[derive(Default)]
pub struct FolderWatches(Mutex<Option<Live>>);

/// A watch that is up — or rather the one thing holding it up: the flag its thread reads to learn
/// that it is not. Dropping this is how a watch is taken down, which is what makes replacing the
/// entry in the registry enough.
struct Live {
    stop: Arc<AtomicBool>,
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Start watching a project's folder and answer with what is in it now.
///
/// Asking again for the same root is not a second watch: the one that is up is taken down first, so
/// a face that remounts (a language change rebuilds the interface) leaves nothing behind it.
#[tauri::command]
pub fn folder_watch(
    app: tauri::AppHandle,
    watches: tauri::State<'_, FolderWatches>,
    project_id: i64,
    root: String,
) -> Result<FolderChangesDto, CmdError> {
    let root = crate::folder::root_of(project_id, &root)?;
    let scan = crate::folder::scan(&root);
    let first = FolderChangesDto {
        changed: crate::folder::recent(&scan),
        // Nothing has been installed yet, so what is reported here is only what the walk itself hit.
        partial: scan.capped,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let live = Live { stop: Arc::clone(&stop) };
    // The old watch comes down as its `Live` is dropped, before the new one goes up.
    *watches.0.lock().expect("the watch registry") = Some(live);

    let held = first.clone();
    std::thread::spawn(move || run(&app, &root, held, &stop));
    Ok(first)
}

/// Stop watching. The face calls this when it goes away; nothing else has to, since asking for a
/// different root replaces the watch on its own.
#[tauri::command]
pub fn folder_unwatch(watches: tauri::State<'_, FolderWatches>) {
    *watches.0.lock().expect("the watch registry") = None;
}

/// The thread behind one watch: install, wait, scan, tell — until the flag says the face has gone.
fn run(app: &tauri::AppHandle, root: &Path, mut held: FolderChangesDto, stop: &AtomicBool) {
    let (tx, rx) = std::sync::mpsc::channel::<Wake>();
    let Ok(mut watcher) = notify::recommended_watcher(handler(tx, root.to_path_buf())) else {
        // No watcher at all. The face keeps the list it was answered with, which is true of the
        // moment it asked — and is told that this is all it will get.
        told(app, &FolderChangesDto { partial: true, ..held });
        return;
    };

    // Where one watch covers the tree, the root is the whole of what gets watched however many
    // folders the walk found under it.
    let whole = [root.to_path_buf()];
    let mut scan = crate::folder::scan(root);
    let mut watched = HashSet::new();
    let mut partial = install(&mut watcher, laid_over(&whole, &scan), &mut watched) || scan.capped;
    if partial != held.partial {
        held.partial = partial;
        told(app, &held);
    }

    while !stop.load(Ordering::Relaxed) {
        if !woke(&rx, stop) {
            continue;
        }
        scan = crate::folder::scan(root);
        // Where a folder needs a watch of its own, one made while the watch was up gets it here,
        // and one that is gone takes its watch with it — the walk is the only place either fact is
        // learned, since what the events said was never read. Where one watch covers the tree there
        // is nothing to keep up with: the list is the root, and the kernel is already inside it.
        watched.retain(|dir: &PathBuf| {
            let kept = scan.dirs.contains(dir);
            if !kept {
                let _ = watcher.unwatch(dir);
            }
            kept
        });
        partial = install(&mut watcher, laid_over(&whole, &scan), &mut watched) || scan.capped;

        let fresh = FolderChangesDto { changed: crate::folder::recent(&scan), partial };
        // A watch fires on touches that mean nothing to a reader — a file read that updated an
        // access time, a build writing inside a folder nobody is shown. Comparing what would be
        // drawn is what drops those, and it is the same reasoning `store_watch`'s signature check
        // is built on.
        if fresh != held {
            held = fresh;
            told(app, &held);
        }
    }
}

/// Wait for a wake-up and let the burst behind it settle. False when nothing came — the thread then
/// gets its chance to notice it is no longer wanted.
///
/// A dropped-events signal is not settled for. Waiting is what turns a burst into one scan, and
/// there is no burst here to fold: the signal already says the kernel has stopped accounting for
/// what moved, so the answer is the same whatever arrives next.
fn woke(rx: &Receiver<Wake>, stop: &AtomicBool) -> bool {
    match rx.recv_timeout(HEARTBEAT) {
        Err(_) => return false,
        Ok(Wake::Rescan) => return true,
        Ok(Wake::Moved) => {}
    }
    while let Ok(wake) = rx.recv_timeout(DEBOUNCE) {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        if matches!(wake, Wake::Rescan) {
            return true;
        }
    }
    true
}

/// The folders the watch is laid over: the root alone where one watch covers what is under it, and
/// every folder the walk found where it does not.
fn laid_over<'a>(whole: &'a [PathBuf], scan: &'a crate::folder::Scan) -> &'a [PathBuf] {
    if RECURSIVE { whole } else { &scan.dirs }
}

/// Put a watch on every folder in `dirs` that has not got one. True when at least one could not be
/// installed — the kernel's per-user limit, most often, which leaves the watches already up working
/// and is the whole reason this is answered rather than swallowed.
fn install(
    watcher: &mut impl Watcher,
    dirs: &[PathBuf],
    watched: &mut HashSet<PathBuf>,
) -> bool {
    let mut refused = 0usize;
    for dir in dirs {
        if watched.contains(dir) {
            continue;
        }
        let mode = if RECURSIVE {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        match watcher.watch(dir, mode) {
            Ok(()) => {
                watched.insert(dir.clone());
            }
            Err(_) => refused += 1,
        }
    }
    if refused > 0 {
        // The path is not logged: a folder somebody chose is theirs, and the diagnostic log is not
        // where it belongs (`crate::diag`). How many were refused is enough to read the report by.
        log::warn!("folder watch: {refused} folder(s) could not be watched (the kernel's limit)");
    }
    refused > 0
}

/// What is made of an event, on the kernel's own thread: at most which of the two things happened.
/// Whether anything really changed is still answered by scanning, not by the event — the kinds
/// cannot be trusted to say (see the note at the top).
///
/// **Everything dropped is dropped here**, before the channel. There is no mask to ask `notify` 8.2
/// for, and a Linux folder walked while `git status` runs fires 37,000 events a second
/// (`AMB-T-3753`): sending those on and dropping them downstream is the same thing as not dropping
/// them.
fn handler(
    tx: Sender<Wake>,
    root: PathBuf,
) -> impl Fn(notify::Result<notify::Event>) + Send + 'static {
    move |res| {
        let Ok(event) = res else { return };
        if event.need_rescan() {
            let _ = tx.send(Wake::Rescan);
            return;
        }
        // inotify wakes on a file being *opened*, and reading is what a machine with an agent on it
        // does most: one `git status` is 1,029 of them, and the walk this module answers with would
        // wake itself in a loop (`AMB-T-3753`). Nothing a reader sees is a read, so nowhere loses
        // anything by this.
        if matches!(event.kind, notify::EventKind::Access(_)) {
            return;
        }
        // A rename carries both names; either one being a name a reader would see is reason enough.
        if !event.paths.is_empty()
            && event.paths.iter().all(|path| crate::folder::pruned(&root, path))
        {
            return;
        }
        let _ = tx.send(Wake::Moved);
    }
}

/// Tell every window. The face is drawn in the board's window today, and a second one drawing it
/// would be looking at the same folder.
fn told(app: &tauri::AppHandle, changes: &FolderChangesDto) {
    let _ = app.emit(CHANGED_EVENT, changes);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Installing is once per folder: a scan that names the same folders again asks the kernel for
    /// nothing, which is what keeps a watch that is up from being rebuilt on every wake-up.
    #[test]
    fn a_folder_is_watched_once() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().to_path_buf();
        let (tx, _rx) = std::sync::mpsc::channel::<Wake>();
        let mut watcher =
            notify::recommended_watcher(handler(tx, root.clone())).expect("a watcher");
        let mut watched = HashSet::new();

        assert!(!install(&mut watcher, std::slice::from_ref(&root), &mut watched));
        assert_eq!(watched.len(), 1);
        assert!(!install(&mut watcher, std::slice::from_ref(&root), &mut watched));
        assert_eq!(watched.len(), 1);
    }

    /// A folder that is not there cannot be watched, and that is the shape the kernel's limit
    /// arrives in too: the call fails, the watches already up keep working, and the answer says so.
    #[test]
    fn a_watch_that_will_not_install_is_reported() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let (tx, _rx) = std::sync::mpsc::channel::<Wake>();
        let mut watcher = notify::recommended_watcher(handler(tx, dir.path().to_path_buf()))
            .expect("a watcher");
        let mut watched = HashSet::new();

        assert!(install(&mut watcher, &[dir.path().join("never-made")], &mut watched));
        assert!(watched.is_empty());
    }

    /// One wake-up per event that a reader could see, and none at all for the ones a build fires
    /// into the folders the tree prunes away — which is where a recursive watch spends nearly all
    /// of its noise.
    #[test]
    fn a_build_writing_where_nobody_looks_does_not_wake_anybody() {
        let root = PathBuf::from("/projects/thing");
        let (tx, rx) = std::sync::mpsc::channel::<Wake>();
        let wake = handler(tx, root.clone());

        for quiet in ["target/debug/x.o", ".git/index", "node_modules/left-pad/index.js"] {
            wake(Ok(notify::Event::new(notify::EventKind::Modify(
                notify::event::ModifyKind::Any,
            ))
            .add_path(root.join(quiet))));
        }
        assert!(rx.try_recv().is_err(), "the machine's folders are not the reader's");

        wake(Ok(notify::Event::new(notify::EventKind::Modify(
            notify::event::ModifyKind::Any,
        ))
        .add_path(root.join("src/lib.rs"))));
        assert!(matches!(rx.try_recv(), Ok(Wake::Moved)));
    }

    /// A file being opened is not a change, and on Linux it is most of what arrives: `git status`
    /// alone fires a thousand of them, and the scan this module answers with fires thousands more.
    #[test]
    fn reading_a_file_is_not_a_change() {
        let root = PathBuf::from("/projects/thing");
        let (tx, rx) = std::sync::mpsc::channel::<Wake>();
        let wake = handler(tx, root.clone());

        wake(Ok(notify::Event::new(notify::EventKind::Access(
            notify::event::AccessKind::Open(notify::event::AccessMode::Read),
        ))
        .add_path(root.join("src/lib.rs"))));
        assert!(rx.try_recv().is_err());
    }

    /// "What moved is not known" is not a change under a path, so neither the prune nor the wait
    /// for quiet applies to it: it arrives having already said everything it is going to say.
    #[test]
    fn a_kernel_that_lost_track_is_heard_out_at_once() {
        let root = PathBuf::from("/projects/thing");
        let (tx, rx) = std::sync::mpsc::channel::<Wake>();
        let wake = handler(tx, root.clone());

        wake(Ok(notify::Event::new(notify::EventKind::Any)
            .add_path(root.join("target/debug/x.o"))
            .set_flag(notify::event::Flag::Rescan)));
        assert!(matches!(rx.try_recv(), Ok(Wake::Rescan)));

        let (tx, rx) = std::sync::mpsc::channel::<Wake>();
        tx.send(Wake::Rescan).expect("the channel");
        let stop = AtomicBool::new(false);
        // Nothing follows it, and it does not wait for anything to.
        assert!(woke(&rx, &stop));
    }

    /// How many watches a folder takes is the OS's answer, and the two answers are laid over
    /// different things: the root alone, or every folder the walk kept.
    #[test]
    fn what_is_watched_follows_the_os() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/deep")).expect("a folder");
        let whole = [root.to_path_buf()];
        let scan = crate::folder::scan(root);
        assert!(scan.dirs.len() > 1);

        let over = laid_over(&whole, &scan);
        if RECURSIVE {
            assert_eq!(over, whole, "one watch already covers what is under it");
        } else {
            assert_eq!(over.len(), scan.dirs.len(), "every folder needs its own");
        }
    }
}
