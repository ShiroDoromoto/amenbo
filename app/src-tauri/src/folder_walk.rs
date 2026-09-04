//! What is in a folder, read by walking it: the names directly inside one, and the folders under it
//! that a reader would call theirs.
//!
//! **What a build wrote is not what a person wrote.** A tree that lists `node_modules` buries the
//! second under the first, so a floor is pruned whether or not anything says to (`PRUNED`), and
//! above that floor the folder speaks for itself — what its `.gitignore` calls noise is noise here
//! too (`walker`). The tree still draws that noise and says that it is noise (`AMB-D-786`), which
//! is why a level is walked twice rather than the ignore rules read a second time.

use std::path::{Component, Path, PathBuf};

use crate::folder::SAVING;

/// The floor of the pruning: folders whose contents are the machine's rather than the person's,
/// pruned whether or not anything says to. A tree that lists them buries what somebody wrote under
/// what a build wrote, and the walk the watch is laid over would be nearly all build output —
/// a build touches thousands of files in seconds (`AMB-T-3566`).
///
/// Above this floor the folder speaks for itself: what its `.gitignore` calls noise is noise here
/// too ([`walker`]). A floor is still needed under that, because `.git` is not in anybody's ignore
/// file, and a folder that is not a repository has no ignore file at all.
const PRUNED: [&str; 4] = [".git", "node_modules", "target", "dist"];

/// How many names the walk behind the watch will look at before it stops. A folder someone points
/// the app at can be anything, and a set of watches is not worth an unbounded walk.
const VISIT_CAP: usize = 20_000;

/// What both walks below agree on: the floor is pruned outright, and a dotfile is not noise.
///
/// Hidden files are **not** skipped, which is where this parts company with the ignore crate's
/// default: a dotfile is a file somebody wrote, and `.amenbo` and `.env` are exactly the ones a
/// reader goes looking for after an agent has been at work.
///
/// The one name left out on top of the floor is this app's own half-written save ([`SAVING`]),
/// which is not a file anybody wrote and is gone again before a reader could act on it.
fn floor(root: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder.hidden(false).require_git(false).filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        !PRUNED.contains(&name.as_ref()) && !name.starts_with(SAVING)
    });
    builder
}

/// The walk the tree is drawn from: the floor, and nothing the repository has to say (`AMB-D-786`).
///
/// **`.gitignore` says what git does not record, not what a person may not look at.** The two were
/// the same walk until now, and the argument against that is in this module's own reasoning: the
/// dotfiles named above as the ones worth showing — `.amenbo`, `.env` — are the very files a
/// repository ignores, this one included.
///
/// What is left out is still left out. A build directory is the floor's business, and the floor is
/// what keeps a tree from burying what somebody wrote under what a build wrote.
pub(crate) fn shown(root: &Path) -> ignore::WalkBuilder {
    let mut builder = floor(root);
    builder
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false);
    builder
}

/// The walk the watch is laid over: the floor, plus the repository's own answer (`AMB-T-3604`).
///
/// `.gitignore`, the global one and the parents' are all read, so a build directory this project
/// happens to call `.next` or `__pycache__` drops out without anybody listing it here. A folder
/// that is no repository loses nothing — there is simply nothing to read, and the floor is all that
/// applies.
///
/// **Here the ignore file is doing work a tree does not need done.** A build rewrites thousands of
/// files a second, and a watch laid over every folder it writes in would wake the face without
/// pause — in exactly the folders people work in (`AMB-D-786`).
pub(crate) fn walker(root: &Path) -> ignore::WalkBuilder {
    let mut builder = floor(root);
    builder
        .ignore(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true);
    builder
}

/// The names directly inside the folder a walk is rooted at, each with whether it is a folder.
pub(crate) fn level(builder: &mut ignore::WalkBuilder) -> Vec<(String, bool)> {
    builder
        .max_depth(Some(1))
        .build()
        .filter_map(Result::ok)
        // The first entry of a walk is the folder it started in.
        .filter(|entry| entry.depth() > 0)
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.file_type().is_some_and(|t| t.is_dir()),
            )
        })
        .collect()
}

/// Whether a path a watch woke us with names something the walk above would never have shown.
///
/// The floor is the same [`PRUNED`] table, read off a path instead of walked: where one watch
/// covers a whole tree ([`crate::folder_watch`]) the kernel reports the build output nobody asked
/// to see, and it is dropped here rather than by declining to watch it. Only the part below `root`
/// is judged — a folder somebody registered is theirs whatever the folders above it are called.
///
/// **This runs on every event a build fires**, thousands a second, so it reads names and nothing
/// else: no filesystem call, no allocation. 50 µs each was the difference between 0.1% of events
/// missed and 69% of them on Windows (`AMB-T-3753`).
///
/// What slips past is not a wrong answer, only an extra walk: the scan behind it compares what
/// would be drawn and finds it unchanged. `cargo clean` is that case — it renames `target` to
/// `target<six letters>` before removing it, and 0.24% of a build's events arrive under the new
/// name (`AMB-T-3752`). The folder's own `.gitignore` is left out for the same reason: reading it
/// per event costs more than the walk it would save.
pub fn pruned(root: &Path, path: &Path) -> bool {
    let Ok(below) = path.strip_prefix(root) else {
        // Not under the root at all. Nothing here can say what it is, so it is not thrown away.
        return false;
    };
    below.components().any(|part| {
        matches!(part, Component::Normal(name) if PRUNED.iter().any(|floor| name == *floor))
    })
}

/// The folders under `root` that a reader would call theirs — the list a watch is installed over
/// (`crate::folder_watch`).
///
/// **The files are walked past and not carried.** Nothing reads them any more: the tree asks for
/// one level at a time as it is opened, and what has changed in the folder is git's answer rather
/// than a list of the newest names (`AMB-D-785`). What that leaves out is the `modified` of every
/// file in the tree — one `stat` per name, which is 26-44% of what this walk cost.
///
/// The walk is capped rather than trusted to end: a folder somebody points the app at can be
/// anything, and a set of watches is not worth an unbounded one. `capped` is true when the cap is
/// what stopped it, which is the one thing the caller cannot work out from the answer.
pub struct Scan {
    /// Every folder walked, `root` included — one watch each.
    pub dirs: Vec<PathBuf>,
    /// Whether the walk stopped at the cap rather than at the end of the tree.
    pub capped: bool,
}

pub fn scan(root: &Path) -> Scan {
    let mut dirs = Vec::new();
    let mut visited = 0usize;
    let mut capped = false;

    for entry in walker(root).build() {
        let Ok(entry) = entry else { continue };
        visited += 1;
        if visited > VISIT_CAP {
            capped = true;
            break;
        }
        // The files are still walked — they are what the cap counts, and a folder is only reached
        // by walking what is beside it — but nothing is kept of them.
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            dirs.push(entry.path().to_path_buf());
        }
    }

    Scan { dirs, capped }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The floor is pruned whether or not anything says so, and what the folder's own ignore file
    /// calls noise is noise here too — the point of taking ripgrep's walker rather than reading the
    /// directory (`AMB-T-3604`). A folder whose name starts with a dot is not noise: it is a folder
    /// somebody made.
    #[test]
    fn the_folder_says_what_of_it_is_the_machines() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("node_modules/left-pad")).expect("the machine's folder");
        std::fs::create_dir_all(root.join("build/out")).expect("this project's own");
        std::fs::create_dir_all(root.join("notes")).expect("somebody's folder");
        std::fs::create_dir_all(root.join(".github")).expect("a dot folder is somebody's too");
        std::fs::write(root.join(".gitignore"), "build/\n").expect("the ignore file");

        let found = scan(root);
        let kept: Vec<String> = found
            .dirs
            .iter()
            .filter_map(|d| d.strip_prefix(root).ok())
            .map(|d| d.to_string_lossy().into_owned())
            .collect();
        // The root is one of them: it is watched like every folder under it.
        assert!(found.dirs.contains(&root.to_path_buf()));
        assert!(kept.iter().any(|d| d == "notes"), "{kept:?}");
        assert!(kept.iter().any(|d| d == ".github"), "a dot folder is somebody's: {kept:?}");
        for gone in ["node_modules", "build"] {
            assert!(
                !kept.iter().any(|d| d.starts_with(gone)),
                "{gone} is the machine's: {kept:?}",
            );
        }
    }

    /// The tree and the walk the watch is laid over part company at the ignore file, and only
    /// there: what a repository ignores is a file somebody wrote, and the tree says so by drawing
    /// it as ignored rather than by leaving it out (`AMB-D-786`). The floor is under both.
    #[test]
    fn the_tree_shows_what_the_repository_ignores_and_says_that_it_does() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("node_modules")).expect("the machine's folder");
        std::fs::create_dir_all(root.join("build")).expect("this project's own");
        std::fs::write(root.join(".gitignore"), "build/
.env
").expect("the ignore file");
        std::fs::write(root.join("node_modules/x.js"), b"built").expect("a file");
        std::fs::write(root.join("build/out.js"), b"built").expect("a file");
        std::fs::write(root.join(".env"), b"SECRET=1").expect("a file");
        std::fs::write(root.join("notes.md"), b"mine").expect("a file");

        // The same two walks the row is built from, and the same difference between them.
        let kept: std::collections::HashSet<String> =
            level(&mut walker(root)).into_iter().map(|(name, _)| name).collect();
        let rows: Vec<(String, bool)> = level(&mut shown(root))
            .into_iter()
            .map(|(name, _)| {
                let ignored = !kept.contains(&name);
                (name, ignored)
            })
            .collect();
        let named = |want: &str| rows.iter().find(|(name, _)| name == want).map(|(_, i)| *i);

        assert_eq!(named("notes.md"), Some(false), "nothing ignores it: {rows:?}");
        assert_eq!(named(".env"), Some(true), "ignored, and on the list all the same: {rows:?}");
        assert_eq!(named("build"), Some(true));
        // The floor is not the ignore file's to overturn: it is off the tree either way.
        assert_eq!(named("node_modules"), None, "the floor is pruned outright: {rows:?}");

        // And the walk the watch is laid over has not moved: what is ignored is still out of it.
        let found = scan(root);
        for gone in ["build", "node_modules"] {
            assert!(
                !found.dirs.iter().any(|d| d.ends_with(gone)),
                "{gone} is not watched: {:?}",
                found.dirs,
            );
        }
    }

    /// The floor is read off a path the same way it is walked — and only below the root, since the
    /// folders above it are not the reader's choice to answer for.
    #[test]
    fn the_floor_reads_the_same_off_a_path() {
        let root = Path::new("/home/someone/target/thing");
        assert!(!pruned(root, root));
        assert!(!pruned(root, &root.join("src/lib.rs")));
        assert!(pruned(root, &root.join("target/debug/x.o")));
        assert!(pruned(root, &root.join("node_modules/left-pad/index.js")));
        assert!(pruned(root, &root.join(".git/index")));
        // A name that only starts the same way is somebody's own.
        assert!(!pruned(root, &root.join("targets/plan.md")));
        // Somewhere else entirely says nothing about this folder, so it is not thrown away.
        assert!(!pruned(root, Path::new("/elsewhere/target/x.o")));
    }

}
