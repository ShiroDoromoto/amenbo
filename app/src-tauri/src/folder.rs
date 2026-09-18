//! What a folder holds — the question [`crate::fileproto`] refuses, answered here.
//!
//! That door hands out the bytes of one file and says so plainly: a directory is not listed there,
//! because listing is not what a door that streams bytes is for. The file face asks the other half
//! of the question — what is in this folder, and what does this file say — and it asks over the
//! command seam, where an answer can be a list.
//!
//! **The three doors are all that is left here.** How far a name may reach is
//! [`crate::folder_fence`]'s, what is in a folder is [`crate::folder_walk`]'s, and what one file
//! says is [`crate::folder_bytes`]'s; each door below asks those three and returns what they say.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use crate::dto::{FolderEntryDto, FolderNameDto, FolderNamesDto};
use crate::error::CmdError;
use crate::folder_fence::{gone, plain, root_of, rooted, under};
use crate::folder_walk::{level, shown, walker};

/// The front of the name a save writes its bytes into before it puts them in place
/// (`crate::folder_save`).
///
/// **It is left out of both walks below.** The window it exists in is about a tenth of a
/// millisecond and the watch waits 400 ms for quiet before it looks, so it is rarely seen
/// (`AMB-T-3739`) — but a burst arriving around it draws a row in the tree under a name nobody
/// wrote, which reads as the app being broken rather than as the app working. The row would go on
/// standing there too: what takes it away is the next walk, and nothing is bound to happen next.
///
/// **It carries no part of the file's own name.** A name may be 255 bytes and no more, so one built
/// out of another name plus a front of its own is longer than the filesystem will take for exactly
/// the files whose names are longest — a refusal that would have nothing to do with what the reader
/// was trying to save.
pub const SAVING: &str = ".amenbo-saving-";

/// The names directly inside one folder — folders first, then files, each run in the order a person
/// reads them. Nothing recurses: a folded tree opens one level at a time (`AMB-T-3602`).
///
/// **Off the main thread.** A command with no `async` on it is run where the webview is drawn
/// ([`crate::agent_models`]), and this one walks the level twice on a filesystem nobody has promised
/// is fast. A project's folders are drawn side by side and each asks for itself, so what the reader
/// meets is not one walk but all of them one behind the other (`AMB-T-4897`).
#[tauri::command]
pub async fn folder_entries(
    project_id: i64,
    root: String,
    path: Vec<String>,
) -> Result<Vec<FolderEntryDto>, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<FolderEntryDto>, CmdError> {
        let (roots, base) = rooted(project_id, &root)?;
        let (_owner, dir) = under(&roots, base, &path).ok_or_else(gone)?;
        // Read off the name itself and not off what it leads to: a folder that is a link is not
        // walked, whatever is on the other side of it (`AMB-D-782`).
        if !std::fs::symlink_metadata(&dir).is_ok_and(|meta| meta.is_dir()) {
            return Err(gone());
        }
        // The level is walked twice, and the difference between the two walks is the mark: what the
        // repository ignores is drawn, and drawn as ignored (`AMB-D-786`). Asking the ignore rules
        // directly instead would be a second reading of them — global file, parents,
        // `.git/info/exclude` and all — and the one that could drift from the walk the watch is
        // actually laid over.
        let kept: std::collections::HashSet<String> =
            level(&mut walker(&dir)).into_iter().map(|(name, _)| name).collect();
        let mut rows: Vec<FolderEntryDto> = level(&mut shown(&dir))
            .into_iter()
            .map(|(name, is_dir)| FolderEntryDto {
                ignored: !kept.contains(&name),
                name,
                is_dir,
            })
            .collect();
        rows.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(rows)
    })
    .await
    .map_err(|e| -> CmdError { format!("reading this folder did not finish: {e}").into() })?
}

/// The most rows a name filter hands back. A tree is read by eye, and a thousand rows of it is not
/// something anybody reads — what a reader does with that many is type another letter.
const NAME_CAP: usize = 2_000;

/// How many names the filter looks at before it gives up on looking at the rest.
///
/// The walk itself is cheap — 344 ms over 177,752 names, all eighteen folders of a real store at
/// once (`AMB-T-4917`) — and it is made again on every letter typed. The cap is what keeps the one
/// folder that is nothing like the others from making the filter feel broken.
const NAME_VISITS: usize = 200_000;

/// How many threads walk at once, the number the same measurement found more of stops helping at.
const NAME_THREADS: usize = 10;

/// Every name under one bound folder that holds `query`, wherever it is.
///
/// **It looks past what is open.** The tree is opened one level at a time, and a reader filtering it
/// is doing it instead of opening folders to look — so a filter that only narrowed the rows already
/// on the screen would answer about the part of the folder they had already searched by hand.
///
/// **Names and nothing else.** No file is opened: what this costs is the walk, which is a fiftieth
/// of what reading the same tree costs (`AMB-T-4917`). Looking inside the files is the other search
/// and a screen of its own (`crate::folder_search`, `AMB-D-910`).
///
/// A row comes back marked `ignored` on the same terms the tree draws one: the two walks are made
/// and the difference between them is the mark (`AMB-D-786`). Both are made over the matches alone,
/// so what is held is the answer rather than the folder.
#[tauri::command]
pub async fn folder_names(
    project_id: i64,
    root: String,
    query: String,
) -> Result<FolderNamesDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<FolderNamesDto, CmdError> {
        let dir = root_of(project_id, &root)?;
        let wanted = query.as_str();
        if wanted.is_empty() {
            return Ok(FolderNamesDto { rows: Vec::new(), capped: false });
        }
        let (mut found, capped) = matches(&mut shown(&dir), &dir, wanted);
        let (kept, _) = matches(&mut walker(&dir), &dir, wanted);
        let kept: HashSet<Vec<String>> = kept.into_iter().map(|(path, _)| path).collect();

        found.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(FolderNamesDto {
            rows: found
                .into_iter()
                .map(|(path, is_dir)| FolderNameDto {
                    ignored: !kept.contains(&path),
                    path,
                    is_dir,
                })
                .collect(),
            capped,
        })
    })
    .await
    .map_err(|e| -> CmdError { format!("looking through these names did not finish: {e}").into() })?
}

/// The names one walk finds `wanted` in, and whether it stopped at a cap rather than at the end.
///
/// The match is on the name and not on the path: a reader narrowing a tree is looking for a file
/// they could name, and a folder's name matching would otherwise drag everything under it in.
fn matches(
    builder: &mut ignore::WalkBuilder,
    root: &Path,
    wanted: &str,
) -> (Vec<(Vec<String>, bool)>, bool) {
    // Folded here rather than by the caller: a reader typing into a filter is not thinking about
    // case, and a function that took the folding as already done would be one every caller could
    // forget.
    let wanted = wanted.to_lowercase();
    let found: Mutex<Vec<(Vec<String>, bool)>> = Mutex::new(Vec::new());
    let seen = std::sync::atomic::AtomicUsize::new(0);
    let capped = std::sync::atomic::AtomicBool::new(false);
    builder.threads(NAME_THREADS).build_parallel().run(|| {
        Box::new(|entry| {
            use std::sync::atomic::Ordering;
            if seen.fetch_add(1, Ordering::Relaxed) >= NAME_VISITS {
                capped.store(true, Ordering::Relaxed);
                return ignore::WalkState::Quit;
            }
            let Ok(entry) = entry else { return ignore::WalkState::Continue };
            // The first entry of a walk is the folder it started in, and it is not a row.
            if entry.depth() == 0 {
                return ignore::WalkState::Continue;
            }
            if !entry.file_name().to_string_lossy().to_lowercase().contains(&wanted) {
                return ignore::WalkState::Continue;
            }
            let Some(path) = under_root(root, entry.path()) else {
                return ignore::WalkState::Continue;
            };
            let mut held = found.lock().expect("the names found");
            if held.len() >= NAME_CAP {
                capped.store(true, Ordering::Relaxed);
                return ignore::WalkState::Quit;
            }
            held.push((path, entry.file_type().is_some_and(|t| t.is_dir())));
            ignore::WalkState::Continue
        })
    });
    (
        found.into_inner().expect("the names found"),
        capped.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// One row's path as the tree names one: the segments under the bound folder.
fn under_root(root: &Path, at: &Path) -> Option<Vec<String>> {
    Some(
        at.strip_prefix(root)
            .ok()?
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect(),
    )
}

/// Open one file the way the machine would open it — the reader's own applications, not ours.
///
/// The face has an editor of its own, and this is still worth having: what it opens a file in is
/// whatever the person already opens that kind of file with. The OS decides what that is, and Amenbo
/// does not keep an opinion about it.
///
/// The path goes out through [`plain`] because this is a door out of the process: past 260
/// characters the fence answers in Windows's internal spelling, and what is on the other side of
/// this call is the shell (`AMB-T-3749`).
#[tauri::command]
pub fn folder_open_file(project_id: i64, root: String, path: Vec<String>) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    tauri_plugin_opener::open_path(plain(&file).as_ref(), None::<&str>)
        .map_err(|e| CmdError::coded("folder.open", e.to_string(), serde_json::Value::Null))
}

/// Show one file where it lives, in the machine's file manager.
///
/// It is the other half of opening: what a person wants of a file is as often "where is this" as
/// "what is in it", and a panel that could only read would leave them hunting for a path they can
/// already see.
///
/// Spelled with [`plain`] for the same reason as [`folder_open_file`]: the file manager is outside
/// this process, and the plugin's own levelling stops at 260 characters where ours does not
/// (`dunce::simplified`, which it calls, keeps the verbatim front on past that).
#[tauri::command]
pub fn folder_reveal_file(project_id: i64, root: String, path: Vec<String>) -> Result<(), CmdError> {
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    tauri_plugin_opener::reveal_item_in_dir(plain(&file).as_ref())
        .map_err(|e| CmdError::coded("folder.reveal", e.to_string(), serde_json::Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with something to find at every depth, and the two kinds of name a tree draws
    /// differently.
    fn folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/deep/deeper")).expect("folders");
        std::fs::create_dir_all(root.join("build")).expect("this project's own");
        std::fs::create_dir_all(root.join("node_modules/left-pad")).expect("the machine's");
        std::fs::write(root.join(".gitignore"), "build/\n").expect("the ignore file");
        std::fs::write(root.join("needle.md"), b"x").expect("a file");
        std::fs::write(root.join("src/needle.rs"), b"x").expect("a file");
        std::fs::write(root.join("src/deep/deeper/needle.txt"), b"x").expect("a file");
        std::fs::write(root.join("src/other.rs"), b"x").expect("a file");
        std::fs::write(root.join("build/needle.js"), b"x").expect("a built file");
        std::fs::write(root.join("node_modules/left-pad/needle.js"), b"x").expect("the machine's");
        dir
    }

    fn found(root: &Path, wanted: &str) -> Vec<String> {
        let (rows, _) = matches(&mut shown(root), root, wanted);
        let mut names: Vec<String> = rows.into_iter().map(|(path, _)| path.join("/")).collect();
        names.sort();
        names
    }

    /// A reader filtering a tree is doing it instead of opening folders to look, so the filter
    /// looks past what is open — all the way down, and into what the repository ignores, which the
    /// tree draws too (`AMB-D-786`). What a build wrote is off the tree either way.
    #[test]
    fn it_finds_a_name_however_deep_it_is() {
        let dir = folder();
        let names = found(dir.path(), "needle");
        assert_eq!(
            names,
            vec![
                "build/needle.js",
                "needle.md",
                "src/deep/deeper/needle.txt",
                "src/needle.rs",
            ],
        );
    }

    /// Case is not something a reader typing into a filter is thinking about.
    #[test]
    fn it_does_not_ask_about_case() {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::write(dir.path().join("README.md"), b"x").expect("a file");
        assert_eq!(found(dir.path(), "readme"), vec!["README.md"]);
        assert_eq!(found(dir.path(), "READ"), vec!["README.md"]);
    }

    /// The match is on the name and not on the path: a folder's name matching would otherwise drag
    /// everything under it in, and a reader narrowing a tree is looking for a file they could name.
    #[test]
    fn it_matches_the_name_and_not_the_way_to_it() {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::create_dir_all(dir.path().join("needle")).expect("a folder");
        std::fs::write(dir.path().join("needle/plain.txt"), b"x").expect("a file");
        assert_eq!(found(dir.path(), "needle"), vec!["needle"]);
    }

    /// What the repository ignores is a row the tree draws and marks, so the filter has to answer
    /// for it the same way: the difference between the two walks is the mark.
    #[test]
    fn what_the_repository_ignores_comes_back_marked() {
        let dir = folder();
        let root = dir.path();
        let (kept, _) = matches(&mut walker(root), root, "needle");
        let kept: HashSet<Vec<String>> = kept.into_iter().map(|(path, _)| path).collect();
        assert!(kept.contains(&vec!["src".to_string(), "needle.rs".to_string()]));
        assert!(!kept.contains(&vec!["build".to_string(), "needle.js".to_string()]));
    }
}
