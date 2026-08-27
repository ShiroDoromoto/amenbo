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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use tauri::Emitter as _;

use crate::dto::{FolderChangedDto, FolderChangesDto};
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
/// only which of three things happened.
enum Wake {
    /// Something under the root moved. Part of a burst, worth waiting out.
    Moved,
    /// Something moved in the repository's own directory: the reader staged something, or
    /// committed. Part of a burst too, and told apart from the rest because **nothing under the
    /// root moved with it** — staging and committing write inside `.git` and touch not one byte of
    /// the working tree (`AMB-T-3748` measured it) — so the answer the face is holding is still
    /// true, and it still has to be told (`AMB-D-774`).
    Git,
    /// The kernel dropped events and is saying so. Not a burst: acted on where it lands.
    Rescan,
}

/// The watches this app has running, one per folder it was asked to watch (`AMB-D-778`).
///
/// **The key is the folder as the caller named it, not what the filesystem calls it.** Taking a
/// watch down has to work for a folder that has since been removed, and a canonical spelling is
/// something only a folder that is still there has. The fence is not weakened by that: what is
/// watched is what [`crate::folder::root_of`] answered for the same name.
#[derive(Default)]
pub struct FolderWatches(Mutex<HashMap<String, Live>>);

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

/// Start watching one of a project's folders and answer with what is in it now.
///
/// Asking again for the same folder is not a second watch: the one that is up is taken down first,
/// so a face that remounts (a language change rebuilds the interface) leaves nothing behind it.
/// **Asking for a different folder adds one**, since a project's folders are drawn side by side and
/// each of them moves on its own (`AMB-D-778`).
///
/// A folder that is not there is refused here rather than answered for: what a face draws for one
/// it cannot find is a question about a folder it already knows the project is bound to. A folder
/// that goes away *while it is watched* is a different matter — that is `gone`, and it is what the
/// watch is there to notice.
#[tauri::command]
pub fn folder_watch(
    app: tauri::AppHandle,
    watches: tauri::State<'_, FolderWatches>,
    project_id: i64,
    root: String,
) -> Result<FolderChangesDto, CmdError> {
    let dir = crate::folder::root_of(project_id, &root)?;
    let scan = crate::folder::scan(&dir);
    let first = FolderChangesDto {
        root: root.clone(),
        changed: crate::folder::recent(&scan),
        // Nothing has been installed yet, so what is reported here is only what the walk itself hit.
        partial: scan.capped,
        gone: false,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let live = Live { stop: Arc::clone(&stop) };
    // This folder's old watch comes down as its `Live` is dropped, and no other folder's is touched.
    watches.0.lock().expect("the watch registry").insert(root, live);

    let held = first.clone();
    std::thread::spawn(move || run(&dir, held, &stop, &|changes| told(&app, changes)));
    Ok(first)
}

/// Stop watching one folder. The face calls this for each folder it drew as it goes away; the
/// others keep running, and asking for the same folder again replaces its watch rather than this.
#[tauri::command]
pub fn folder_unwatch(watches: tauri::State<'_, FolderWatches>, root: String) {
    watches.0.lock().expect("the watch registry").remove(&root);
}

/// The thread behind one watch: install, wait, scan, tell — until the flag says the face has gone.
///
/// Where what it has to say goes is the caller's to give, and every window is where that is in the
/// app ([`told`]). Taking it as an argument is what lets the loop be run against a folder in a test
/// rather than only against a running window.
fn run(
    root: &Path,
    mut held: FolderChangesDto,
    stop: &AtomicBool,
    tell: &dyn Fn(&FolderChangesDto),
) {
    // Asked before the watcher is built, because what the handler lets through depends on it — and
    // asked once: a folder that becomes a repository while it is being watched is one the face
    // learns about when it next mounts, which is the same moment this thread starts again.
    let repo = crate::folder_git::repo_of(root).map(|repo| repo.git_dir);

    let (tx, rx) = std::sync::mpsc::channel::<Wake>();
    let Ok(mut watcher) = notify::recommended_watcher(handler(tx, root.to_path_buf(), repo.clone()))
    else {
        // No watcher at all. The face keeps the list it was answered with, which is true of the
        // moment it asked — and is told that this is all it will get.
        tell(&FolderChangesDto { partial: true, ..held });
        return;
    };

    // Where one watch covers the tree, the root is the whole of what gets watched however many
    // folders the walk found under it.
    let whole = [root.to_path_buf()];
    let mut scan = crate::folder::scan(root);
    let mut watched = HashSet::new();
    let mut partial = install(&mut watcher, laid_over(&whole, &scan), &mut watched) || scan.capped;
    watch_repo(&mut watcher, repo.as_deref());
    if partial != held.partial {
        held.partial = partial;
        tell(&held);
    }

    // Whether the folder is where it was. A watch does not follow it: on Windows and Linux the
    // kernel holds the folder itself, so one that is moved away keeps firing under a name that is
    // now somebody else's, and one made again where it was gets no watch at all (`AMB-T-3753`
    // measured both). So the walk is asked, not the events — and asked on every heartbeat, since a
    // folder that is not there is also a folder nothing arrives from.
    let mut present = !held.gone;

    while !stop.load(Ordering::Relaxed) {
        let awake = woke(&rx, stop);
        let here = root.is_dir();
        if awake.is_none() && here == present {
            continue;
        }
        // A folder that went away took its watches with it, the repository's among them.
        if here && !present {
            watch_repo(&mut watcher, repo.as_deref());
        }
        present = here;
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
        // A folder that is not there is not a folder that is half watched: the walk found nothing
        // because there is nothing, and saying "some of this is unwatched" of it would send a
        // reader looking for the half that is.
        partial = present
            && (install(&mut watcher, laid_over(&whole, &scan), &mut watched) || scan.capped);

        let fresh = FolderChangesDto {
            changed: crate::folder::recent(&scan),
            partial,
            gone: !present,
            ..held.clone()
        };
        // A watch fires on touches that mean nothing to a reader — a file read that updated an
        // access time, a build writing inside a folder nobody is shown. Comparing what would be
        // drawn is what drops those, and it is the same reasoning `store_watch`'s signature check
        // is built on.
        //
        // **Except when the repository is what moved**, where the comparison would drop the one
        // wake-up worth having: staging changes what git says about a file and nothing about the
        // file, so the rows are equal and the face still has to go and ask again (`AMB-D-774`).
        //
        // **And what this app itself just saved**, which is a change a reader asked for and is
        // already looking at: the row belongs on the list, and the panel being redrawn under them
        // for their own save is the noise (`our_own` below). The answer is kept either way, so the
        // saved file is on the list the next time anything is said.
        let news = fresh != held && !our_own(root, &held, &fresh);
        held = fresh;
        if news || matches!(awake, Some(Wake::Git)) {
            tell(&held);
        }
    }
}

/// What this process has written lately: the path, and the time the file came out with.
///
/// **The kernel does not say who wrote.** macOS answers `None` for the process behind an event and
/// the other two carry no such field at all (`AMB-T-3739` measured it), and `notify` sets no
/// ignore-self flag — so the only way to tell a save made here from one made by the agent in the
/// pane is to have written down what was saved.
///
/// The time is what makes it a note about *this* write rather than about the file: it is spelled
/// the way a scan spells it ([`crate::folder::recent`]), so a row read back off the filesystem
/// either is the one that was written or is somebody else's. ⚠ The one filesystem that blurs this
/// is FAT, whose times move in two-second steps (`AMB-T-3739`) — a write from elsewhere landing in
/// the same step as ours is read as ours, and told about at the next thing that moves.
static WRITTEN: Mutex<Vec<(PathBuf, String, Instant)>> = Mutex::new(Vec::new());

/// How long a note is kept: long enough to cover the wait for quiet and the walk behind it, short
/// enough that the file goes back to being anybody's the moment nothing more is coming.
const REMEMBERED: Duration = Duration::from_secs(5);

/// Write down that this process has just put bytes into `path` (`crate::folder_save`).
///
/// A file whose time cannot be read is not written down. That is the safe way round: the save is
/// then treated as somebody else's and the face is told, which is a redraw rather than a silence.
pub fn wrote(path: &Path) {
    let Ok(stamped) = path.symlink_metadata().and_then(|meta| meta.modified()) else {
        return;
    };
    let stamped = chrono::DateTime::<chrono::Utc>::from(stamped).to_rfc3339();
    let Ok(mut written) = WRITTEN.lock() else { return };
    let now = Instant::now();
    written.retain(|(_, _, at)| now.duration_since(*at) < REMEMBERED);
    written.push((path.to_path_buf(), stamped, now));
}

/// Whether the only rows that moved between these two answers are ones this process wrote itself.
///
/// A row that **arrived** has to be one of ours, and there has to be at least one — two answers
/// that differ in nothing gained are not a save.
///
/// A row that **left** is allowed only where a row arriving explains it: the same file under the
/// time it carried before, or the oldest of a full list pushed off the end by the new one. Anything
/// else — a file deleted while the save was going through — is news, and is told. ⚠ The one it
/// cannot tell apart is a deletion of the very oldest row of a full list, which is the thirtieth
/// entry of "what changed lately" and reappears correct at the next wake-up.
fn our_own(root: &Path, held: &FolderChangesDto, fresh: &FolderChangesDto) -> bool {
    if held.partial != fresh.partial || held.gone != fresh.gone {
        return false;
    }
    let Ok(written) = WRITTEN.lock() else { return false };
    let now = Instant::now();
    let ours = |row: &FolderChangedDto| {
        let path = root.join(row.path.iter().collect::<PathBuf>());
        written.iter().any(|(wrote, stamped, at)| {
            *wrote == path && *stamped == row.modified && now.duration_since(*at) < REMEMBERED
        })
    };

    let before: HashSet<(&[String], &str)> = held.changed.iter().map(named).collect();
    let mut gained = fresh.changed.iter().filter(|row| !before.contains(&named(row))).peekable();
    if gained.peek().is_none() || !gained.all(ours) {
        return false;
    }

    let after: HashSet<(&[String], &str)> = fresh.changed.iter().map(named).collect();
    held.changed
        .iter()
        .filter(|row| !after.contains(&named(row)))
        .all(|row| {
            fresh.changed.iter().any(|now| now.path == row.path)
                || fresh.changed.len() == crate::folder::RECENT
        })
}

/// One row as the pair that makes it the same row: where it is, and when it was written.
fn named(row: &FolderChangedDto) -> (&[String], &str) {
    (&row.path, &row.modified)
}

/// Wait for a wake-up and let the burst behind it settle. False when nothing came — the thread then
/// gets its chance to notice it is no longer wanted.
///
/// A dropped-events signal is not settled for. Waiting is what turns a burst into one scan, and
/// there is no burst here to fold: the signal already says the kernel has stopped accounting for
/// what moved, so the answer is the same whatever arrives next.
fn woke(rx: &Receiver<Wake>, stop: &AtomicBool) -> Option<Wake> {
    let mut held = match rx.recv_timeout(HEARTBEAT) {
        Err(_) => return None,
        Ok(Wake::Rescan) => return Some(Wake::Rescan),
        Ok(wake) => wake,
    };
    while let Ok(wake) = rx.recv_timeout(DEBOUNCE) {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        match wake {
            Wake::Rescan => return Some(Wake::Rescan),
            // One repository wake-up in the burst is what the burst is: a commit writes several
            // names in there and moves nothing outside, and it is the outside the comparison reads.
            Wake::Git => held = Wake::Git,
            Wake::Moved => {}
        }
    }
    Some(held)
}

/// Put the second watch on: the directory the repository keeps itself in, where staging and
/// committing write (`AMB-D-774`). Nothing is said when there is none — a folder that is not a
/// repository is not a lesser answer, it is the ordinary one.
///
/// **Not recursive, whatever the OS would allow.** What a commit does to `.git/objects` is
/// thousands of names nobody is shown; what a reader's screen turns on is `index` and `HEAD`, which
/// are directly inside.
///
/// **One watch per bound folder, even where two of them are the same repository.** Folding those
/// into one would leave the other folder's face unwoken unless something fanned the wake-up back
/// out — two watches on one directory is the cheaper of the two, now that a bound folder costs one
/// watch rather than one per folder under it (`AMB-D-779`).
fn watch_repo(watcher: &mut impl Watcher, repo: Option<&Path>) {
    let Some(dir) = repo else { return };
    if watcher.watch(dir, RecursiveMode::NonRecursive).is_err() {
        // The folder is a repository whose own directory could not be watched — the reader's
        // staging will not turn anything on, and everything else about the folder still will.
        log::warn!("folder watch: the repository's own directory could not be watched");
    }
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
    repo: Option<PathBuf>,
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
        // `.git` is on the floor the tree is pruned by, so the one directory this is here to hear
        // from is the one the prune would throw away. What is let back through is what is *directly*
        // inside it — `index`, `HEAD`, the refs a commit moves — and not the thousands of names a
        // commit writes under `objects`, which are as much nobody's business as a build's output.
        let staged = |path: &Path| {
            repo.as_deref().is_some_and(|dir| path.parent() == Some(dir))
        };
        if event.paths.iter().any(|path| staged(path)) {
            let _ = tx.send(Wake::Git);
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

    /// One row, as a scan would have read it off the filesystem.
    fn row(root: &Path, name: &str) -> FolderChangedDto {
        let stamped = root
            .join(name)
            .symlink_metadata()
            .and_then(|meta| meta.modified())
            .expect("the file");
        FolderChangedDto {
            path: vec![name.to_string()],
            modified: chrono::DateTime::<chrono::Utc>::from(stamped).to_rfc3339(),
        }
    }

    fn changes(rows: Vec<FolderChangedDto>) -> FolderChangesDto {
        FolderChangesDto { root: String::new(), changed: rows, partial: false, gone: false }
    }

    /// A save made here is a change the reader asked for and is already looking at, so the wake-up
    /// it causes is not passed on — while a write from anywhere else is, which is the whole point of
    /// writing anything down (`AMB-T-3739`: the events themselves never say who wrote).
    #[test]
    fn this_app_s_own_save_is_not_news_and_anybody_else_s_is() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        let before = changes(vec![]);

        std::fs::write(root.join("ours.md"), b"saved from the panel").expect("a file");
        wrote(&root.join("ours.md"));
        assert!(our_own(root, &before, &changes(vec![row(root, "ours.md")])));

        // The same file, written again by something else: the time is not the one written down.
        std::fs::write(root.join("theirs.md"), b"the agent in the pane").expect("a file");
        assert!(!our_own(root, &before, &changes(vec![row(root, "theirs.md")])));

        // And two answers that gained nothing are not a save either — they are a folder where
        // something was taken away, which is news.
        assert!(!our_own(root, &changes(vec![row(root, "ours.md")]), &before));
    }

    /// Installing is once per folder: a scan that names the same folders again asks the kernel for
    /// nothing, which is what keeps a watch that is up from being rebuilt on every wake-up.
    #[test]
    fn a_folder_is_watched_once() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().to_path_buf();
        let (tx, _rx) = std::sync::mpsc::channel::<Wake>();
        let mut watcher =
            notify::recommended_watcher(handler(tx, root.clone(), None)).expect("a watcher");
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
        let mut watcher = notify::recommended_watcher(handler(tx, dir.path().to_path_buf(), None))
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
        let wake = handler(tx, root.clone(), None);

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
        let wake = handler(tx, root.clone(), None);

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
        let wake = handler(tx, root.clone(), None);

        wake(Ok(notify::Event::new(notify::EventKind::Any)
            .add_path(root.join("target/debug/x.o"))
            .set_flag(notify::event::Flag::Rescan)));
        assert!(matches!(rx.try_recv(), Ok(Wake::Rescan)));

        let (tx, rx) = std::sync::mpsc::channel::<Wake>();
        tx.send(Wake::Rescan).expect("the channel");
        let stop = AtomicBool::new(false);
        // Nothing follows it, and it does not wait for anything to.
        assert!(woke(&rx, &stop).is_some());
    }

    /// What the mode is for: a file written in a folder that never got a watch of its own still
    /// wakes somebody. Where every folder needs one this passes because the walk found that folder;
    /// where one covers the tree it passes because the kernel is already inside it — and that is
    /// the whole claim, since nothing here installs a watch on the folder the write lands in.
    #[test]
    fn a_write_deep_in_the_tree_wakes_somebody() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("src/deep")).expect("a folder");

        let (tx, rx) = std::sync::mpsc::channel::<Wake>();
        let mut watcher =
            notify::recommended_watcher(handler(tx, root.clone(), None)).expect("a watcher");
        let scan = crate::folder::scan(&root);
        let whole = [root.clone()];
        let mut watched = HashSet::new();
        assert!(!install(&mut watcher, laid_over(&whole, &scan), &mut watched));

        std::fs::write(root.join("src/deep/note.md"), b"mine").expect("a file");
        let stop = AtomicBool::new(false);
        // One wait is a heartbeat long, which is how often the thread looks up — not how long a
        // kernel has to get round to it. A machine with something else on it takes longer than one.
        let woken = (0..20).any(|_| woke(&rx, &stop).is_some());
        assert!(woken, "a write under the root is what the watch is for");
    }

    /// Taking one folder's watch down is that folder's own business: what stops is its thread, and
    /// the others go on. Dropping the entry is the whole of the mechanism, which is why the
    /// registry holds one per folder rather than one for the app.
    #[test]
    fn one_folder_stops_and_the_others_do_not() {
        let watches = FolderWatches::default();
        let mut flags = HashMap::new();
        for root in ["/work/repo", "/work/plugins"] {
            let stop = Arc::new(AtomicBool::new(false));
            flags.insert(root, Arc::clone(&stop));
            watches.0.lock().expect("the registry").insert(root.to_string(), Live { stop });
        }

        watches.0.lock().expect("the registry").remove("/work/repo");
        assert!(flags["/work/repo"].load(Ordering::Relaxed), "the one taken down stops");
        assert!(!flags["/work/plugins"].load(Ordering::Relaxed), "the other one does not");
        assert_eq!(watches.0.lock().expect("the registry").len(), 1);
    }

    /// Asking again for a folder that is already watched replaces its watch instead of laying a
    /// second one over it — the face that remounts is the same face looking at the same folder.
    #[test]
    fn asking_again_for_a_folder_replaces_its_watch() {
        let watches = FolderWatches::default();
        let first = Arc::new(AtomicBool::new(false));
        let second = Arc::new(AtomicBool::new(false));
        let mut registry = watches.0.lock().expect("the registry");
        registry.insert("/work/repo".to_string(), Live { stop: Arc::clone(&first) });
        registry.insert("/work/repo".to_string(), Live { stop: Arc::clone(&second) });

        assert!(first.load(Ordering::Relaxed), "the one that was up is told to stop");
        assert!(!second.load(Ordering::Relaxed));
        assert_eq!(registry.len(), 1);
    }

    /// A folder that is removed is not a folder with nothing in it, and the difference is the
    /// whole of what a reader can act on. Nothing arrives from a folder that is not there — the
    /// walk is what answers, and it keeps answering, so a folder made again where this one was is
    /// watched again without anybody asking.
    #[test]
    fn a_folder_that_goes_away_says_so_and_says_when_it_is_back() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("watched");
        std::fs::create_dir(&root).expect("the folder");

        let (tx, rx) = std::sync::mpsc::channel::<FolderChangesDto>();
        let stop = Arc::new(AtomicBool::new(false));
        let held = FolderChangesDto {
            root: root.to_string_lossy().into_owned(),
            changed: Vec::new(),
            partial: false,
            gone: false,
        };
        let thread = {
            let (root, stop) = (root.clone(), Arc::clone(&stop));
            std::thread::spawn(move || {
                run(&root, held, &stop, &|changes| {
                    let _ = tx.send(changes.clone());
                });
            })
        };

        std::fs::remove_dir_all(&root).expect("take the folder away");
        let told = next_saying(&rx, |changes| changes.gone);
        assert!(told.gone, "a folder that is not there is said to be gone");
        assert!(!told.partial, "and not drawn as a folder that is half watched");

        std::fs::create_dir(&root).expect("put it back");
        let told = next_saying(&rx, |changes| !changes.gone);
        assert!(!told.gone, "a folder made again where it was is watched again");

        stop.store(true, Ordering::Relaxed);
        thread.join().expect("the watch thread");
    }

    /// The first answer that satisfies `wanted`, or a failure rather than a hang. Long enough for a
    /// machine with something else on it: what is waited for is a heartbeat, not a kernel.
    fn next_saying(
        rx: &Receiver<FolderChangesDto>,
        wanted: impl Fn(&FolderChangesDto) -> bool,
    ) -> FolderChangesDto {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(changes) if wanted(&changes) => return changes,
                Ok(_) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(other) => panic!("the watch thread went away: {other}"),
            }
        }
        panic!("nothing said what was waited for");
    }

    /// Staging is the wake-up nothing else here would notice: `git add` writes inside the
    /// repository's own directory and touches not one byte of the working tree, so the rows the
    /// face is holding are still right and it still has to be told — the colour beside them is not
    /// (`AMB-D-774`).
    #[test]
    fn staging_something_wakes_the_face_although_nothing_moved() {
        let Some(_) = amenbo_core::sys::git() else {
            // A machine with no git to run. What is under test is git writing in its own directory.
            return;
        };
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().to_path_buf();
        for args in [&["init"][..], &["config", "user.email", "a@b"], &["config", "user.name", "A"]]
        {
            let done = amenbo_core::sys::git()
                .expect("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .expect("git ran");
            assert!(done.status.success(), "{args:?}");
        }
        // Written before the watch is up, so the only thing that moves afterwards is inside `.git`.
        std::fs::write(root.join("note.md"), b"mine").expect("a file");

        let (tx, rx) = std::sync::mpsc::channel::<FolderChangesDto>();
        let stop = Arc::new(AtomicBool::new(false));
        let held = FolderChangesDto {
            root: root.to_string_lossy().into_owned(),
            changed: Vec::new(),
            partial: false,
            gone: false,
        };
        let thread = {
            let (root, stop) = (root.clone(), Arc::clone(&stop));
            std::thread::spawn(move || {
                run(&root, held, &stop, &|changes| {
                    let _ = tx.send(changes.clone());
                });
            })
        };
        // Whatever the first walk had to say, said and done with, so what is waited for below can
        // only be the staging.
        while rx.recv_timeout(Duration::from_secs(2)).is_ok() {}

        let staged = amenbo_core::sys::git()
            .expect("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "note.md"])
            .output()
            .expect("git ran");
        assert!(staged.status.success());

        let told = rx.recv_timeout(Duration::from_secs(20)).expect("the face is told");
        assert!(!told.gone);
        // And what it is told is the same list it was holding: the rows did not move, which is the
        // whole reason the comparison would have swallowed this one.
        assert!(told.changed.iter().any(|row| row.path == ["note.md"]));

        stop.store(true, Ordering::Relaxed);
        thread.join().expect("the watch thread");
    }

    /// The same, in a linked worktree — where `.git` is a *file* and the directory that holds the
    /// index is somewhere else entirely. A watch laid on `<folder>/.git` would be watching a file
    /// nothing writes, and every commit made in the worktree would go unnoticed.
    #[test]
    fn staging_in_a_linked_worktree_wakes_it_too() {
        let Some(_) = amenbo_core::sys::git() else { return };
        let dir = tempfile::tempdir().expect("a temp dir");
        let main = dir.path().join("main");
        std::fs::create_dir(&main).expect("the repository");
        let linked = dir.path().join("linked");
        std::fs::write(main.join("first.md"), b"a commit to branch from").expect("a file");
        for args in [
            &["init"][..],
            &["config", "user.email", "a@b"],
            &["config", "user.name", "A"],
            &["add", "first.md"],
            &["commit", "-m", "first"],
            &["worktree", "add", linked.to_str().expect("a path"), "-b", "side"],
        ] {
            let done = amenbo_core::sys::git()
                .expect("git")
                .arg("-C")
                .arg(&main)
                .args(args)
                .output()
                .expect("git ran");
            assert!(done.status.success(), "{args:?}: {}", String::from_utf8_lossy(&done.stderr));
        }
        assert!(linked.join(".git").is_file(), "a linked worktree keeps a file there, not a folder");
        std::fs::write(linked.join("note.md"), b"mine").expect("a file");

        let (tx, rx) = std::sync::mpsc::channel::<FolderChangesDto>();
        let stop = Arc::new(AtomicBool::new(false));
        let held = FolderChangesDto {
            root: linked.to_string_lossy().into_owned(),
            changed: Vec::new(),
            partial: false,
            gone: false,
        };
        let thread = {
            let (root, stop) = (linked.clone(), Arc::clone(&stop));
            std::thread::spawn(move || {
                run(&root, held, &stop, &|changes| {
                    let _ = tx.send(changes.clone());
                });
            })
        };
        while rx.recv_timeout(Duration::from_secs(2)).is_ok() {}

        let staged = amenbo_core::sys::git()
            .expect("git")
            .arg("-C")
            .arg(&linked)
            .args(["add", "note.md"])
            .output()
            .expect("git ran");
        assert!(staged.status.success());

        rx.recv_timeout(Duration::from_secs(20)).expect("the face is told");
        stop.store(true, Ordering::Relaxed);
        thread.join().expect("the watch thread");
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
