//! Being told what changed in the folder, rather than going to look (`AMB-T-3604`).
//!
//! The file face asked a walk whenever it could think of a reason to (`crate::folder`), which is a
//! guess about when a person is looking and a walk they pay for whether or not anything moved. Here
//! the kernel does the waking, and the face is told.
//!
//! **Three things make this different from watching a store directory** ([`crate::store_watch`]),
//! which watches one folder that holds a handful of files:
//!
//! 1. **The watch is a set of non-recursive watches, one per folder**, laid over the pruned tree.
//!    A recursive watch cannot be pruned, and the folders that are pruned are exactly the ones a
//!    build writes thousands of files into: on Linux a recursive watch of a repository reports
//!    8,645 events before anybody has typed anything, and a 4.6-second build on Windows fires 2,550
//!    (`AMB-T-3566`). A folder made while the watch is up gets one of its own on the next scan.
//! 2. **What the events say is not read.** macOS calls an append a `Create`, so new, changed and
//!    gone cannot be told apart from the event kind — and none of the three would be trustworthy
//!    anyway on a burst that arrived out of order. So a wake-up means only "something moved", and
//!    what changed is worked out by scanning again and comparing with what was held.
//! 3. **A watch that could not be installed is said out loud.** The kernel's watch limit is per
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
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let Ok(mut watcher) = notify::recommended_watcher(handler(tx)) else {
        // No watcher at all. The face keeps the list it was answered with, which is true of the
        // moment it asked — and is told that this is all it will get.
        told(app, &FolderChangesDto { partial: true, ..held });
        return;
    };

    let mut scan = crate::folder::scan(root);
    let mut watched = HashSet::new();
    let mut partial = install(&mut watcher, &scan.dirs, &mut watched) || scan.capped;
    if partial != held.partial {
        held.partial = partial;
        told(app, &held);
    }

    while !stop.load(Ordering::Relaxed) {
        if !woke(&rx, stop) {
            continue;
        }
        scan = crate::folder::scan(root);
        // A folder made while the watch was up gets a watch of its own here, and one that is gone
        // takes its watch with it — the walk is the only place either fact is learned, since what
        // the events said was never read.
        watched.retain(|dir: &PathBuf| {
            let kept = scan.dirs.contains(dir);
            if !kept {
                let _ = watcher.unwatch(dir);
            }
            kept
        });
        partial = install(&mut watcher, &scan.dirs, &mut watched) || scan.capped;

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
fn woke(rx: &Receiver<()>, stop: &AtomicBool) -> bool {
    if rx.recv_timeout(HEARTBEAT).is_err() {
        return false;
    }
    while rx.recv_timeout(DEBOUNCE).is_ok() {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
    }
    true
}

/// Put a non-recursive watch on every folder that has not got one. True when at least one could not
/// be installed — the kernel's per-user limit, most often, which leaves the watches already up
/// working and is the whole reason this is answered rather than swallowed.
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
        match watcher.watch(dir, RecursiveMode::NonRecursive) {
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

/// What a wake-up carries: nothing. Whether anything really changed is answered by scanning, not by
/// the event — the kinds cannot be trusted to say (see the note at the top).
fn handler(tx: Sender<()>) -> impl Fn(notify::Result<notify::Event>) + Send + 'static {
    move |res| {
        if res.is_ok() {
            let _ = tx.send(());
        }
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
        let (tx, _rx) = std::sync::mpsc::channel::<()>();
        let mut watcher = notify::recommended_watcher(handler(tx)).expect("a watcher");
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
        let (tx, _rx) = std::sync::mpsc::channel::<()>();
        let mut watcher = notify::recommended_watcher(handler(tx)).expect("a watcher");
        let mut watched = HashSet::new();

        assert!(install(&mut watcher, &[dir.path().join("never-made")], &mut watched));
        assert!(watched.is_empty());
    }
}
