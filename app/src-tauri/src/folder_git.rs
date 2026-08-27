//! What git says about the folder the file face is showing, so a tree row can wear a colour.
//!
//! It is one `git status` per bound folder, asked for rather than kept up to date — the face asks
//! when it opens the panel — and read for display and nothing else: no pane is marked from it and
//! nobody is told their turn has come (`AMB-D-774`). A folder that is not a repository, and a
//! machine with no git that can be run without asking the reader to install a compiler, both answer
//! the same way — nothing.
//!
//! **Two folders of one repository are asked separately.** What `git status` costs is the amount of
//! tree it is asked about, so folding two bound folders into one call over their common root is
//! five times the work of asking each about itself (`AMB-T-3742` measured it).
//!
//! **What git answers with is not what the tree is drawn from.** Every path comes back relative to
//! the repository's own root, so a folder bound at `repo/app` is answered in `app/…` and not one row
//! of it lines up. Taking that front off is what [`crate::folder_git::repo_of`] is for, and it is
//! the whole reason a second git call exists here at all.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::dto::GitEntryDto;
use crate::error::CmdError;
use crate::folder_fence::root_of;

/// What git calls one bound folder — as much of it as anything here reads.
///
/// One `rev-parse` answers several questions about where a folder sits in the same call, and only
/// the ones something reads are asked for: a field carried for a reader that does not exist yet is
/// one nothing goes red about when it turns out to be wrong. The repository's own root is still not
/// among them.
#[derive(Clone)]
pub struct Repo {
    /// The path from the repository's root down to the bound folder, ending in `/`, and empty when
    /// the bound folder *is* the root. This is the front that comes off every path git names.
    pub prefix: String,
    /// The directory git keeps the repository in — where staging and committing write, and where
    /// nothing else does ([`crate::folder_watch`]).
    ///
    /// **Asked for rather than guessed at as `<folder>/.git`.** In a linked worktree that name is a
    /// file pointing elsewhere, and a watch laid on it would miss every commit made in the worktree
    /// (`AMB-T-3748` measured all three systems). It is the per-worktree directory that holds
    /// `index` and `HEAD`, which is why this is `--absolute-git-dir` and not `--git-common-dir`.
    pub git_dir: PathBuf,
}

/// The answer for each bound folder that had one, kept for the life of the process.
///
/// Only repositories are remembered. A folder that is not one is asked again on the next refresh,
/// which costs the one process `rev-parse` is (3ms measured, and flat in the size of the tree) and
/// buys `git init` being noticed without restarting the app. The other direction does not need the
/// same care: a repository stops being one only by being deleted, and the folder goes with it.
fn remembered() -> &'static Mutex<HashMap<PathBuf, Repo>> {
    static REPOS: OnceLock<Mutex<HashMap<PathBuf, Repo>>> = OnceLock::new();
    REPOS.get_or_init(Default::default)
}

/// Where `dir`'s repository is, or `None` when it is not in one — asked once and then remembered.
pub fn repo_of(dir: &Path) -> Option<Repo> {
    if let Some(known) = remembered().lock().ok()?.get(dir) {
        return Some(known.clone());
    }
    let out = run(dir, &["rev-parse", "--show-prefix", "--absolute-git-dir"])?;
    // One line each, in the order the options were written. At the root of a repository the prefix
    // is an empty line — an answer and not a failure — so both are taken by position rather than by
    // whether they say anything.
    let mut lines = out.lines();
    let repo = Repo {
        prefix: lines.next()?.to_string(),
        git_dir: PathBuf::from(lines.next()?),
    };
    if let Ok(mut all) = remembered().lock() {
        all.insert(dir.to_path_buf(), repo.clone());
    }
    Some(repo)
}

/// Everything git has to say about the folder `root` names, in the shape the file face draws rows
/// from. An empty answer is the honest one for every way this can come to nothing.
#[tauri::command]
pub fn folder_git_status(project_id: i64, root: String) -> Result<Vec<GitEntryDto>, CmdError> {
    let dir = root_of(project_id, &root)?;
    let Some(repo) = repo_of(&dir) else { return Ok(Vec::new()) };
    // `--no-optional-locks` sits before `status` because it is git's own option and not the
    // subcommand's; behind it git exits 129 without doing anything. What it buys is the index lock:
    // without it this call races the reader's own `git add` and breaks it — 92.8% of the time on
    // Linux, and never with it (`AMB-T-3742` measured all three systems).
    //
    // `-z` is what makes a name in any language come back as the bytes it really is; without it git
    // writes octal escapes instead. `-- .` holds the answer to this folder: git otherwise climbs to
    // the repository root and answers for the whole of it, at eight times the cost.
    let Some(out) = run(&dir, &["--no-optional-locks", "status", "--porcelain=v1", "-z", "--", "."])
    else {
        return Ok(Vec::new());
    };
    Ok(rows(&out, &repo.prefix))
}

/// Run git in `dir` and hand back its stdout, or `None` for every way it did not answer.
///
/// Whether it answered is read off the exit status alone. "not a repository" is 128 on all three
/// systems but says so in three different sentences, and one of them is translated (`AMB-T-3748`).
///
/// Its stderr goes nowhere on purpose. git warns there about what it stepped over and then answers
/// normally — a Windows branch past 260 characters is skipped with a `Filename too long` warning
/// and the rest of the tree comes back — and a warning about a path is not something to put on a
/// reader's screen when the thing they asked for arrived.
fn run(dir: &Path, args: &[&str]) -> Option<String> {
    let out = amenbo_core::sys::git()?
        .arg("-C")
        .arg(dir)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Read `--porcelain=v1 -z` into rows, with `prefix` taken off the front of every path.
///
/// A record is two status letters, a space, and a path, run together and ended by NUL. A rename or
/// a copy carries a second path after the first — where it came from — and that one is dropped:
/// the tree draws where a file is now, and where it was is a row that is not there any more. In
/// `-z` the current path comes first, which is the reverse of the arrow form git writes for people.
///
/// A path git ends in `/` is a folder it is answering for as a whole rather than naming what is
/// inside it, and the row says so: a folded folder is somewhere a colour still has to appear. When
/// nothing under the bound folder is tracked, the folder git names that way is the bound folder
/// itself, and the row for it carries no segments at all — zero segments *is* the bound folder, in
/// the same spelling every other row is measured from.
fn rows(out: &str, prefix: &str) -> Vec<GitEntryDto> {
    let mut rows = Vec::new();
    let mut fields = out.split('\0');
    while let Some(record) = fields.next() {
        // The trailing NUL leaves an empty field behind it, and a record shorter than the two
        // letters and the space is not one. The letters are ASCII, so the third byte is a boundary.
        if !record.is_char_boundary(3) {
            continue;
        }
        let (code, path) = record.split_at(3);
        let mut code = code.chars();
        let (Some(index), Some(worktree)) = (code.next(), code.next()) else { continue };
        if index == 'R' || index == 'C' || worktree == 'R' || worktree == 'C' {
            fields.next();
        }
        // Whether it is a folder is read off the whole path rather than off what is left of it: the
        // bound folder's own row is the prefix and nothing else, so taking the prefix off takes the
        // slash that says so with it.
        let is_dir = path.ends_with('/');
        // A path that is not under this folder is not a row the tree can draw. git is answering
        // about the folder, so that is the rare case — and a folder that is its repository's root
        // has an empty prefix, where everything passes.
        let Some(inside) = path.strip_prefix(prefix) else { continue };
        let inside = inside.trim_end_matches('/');
        // Splitting an empty path would make one empty segment out of no path at all, and those are
        // different rows: no segments is the bound folder, one empty segment is nothing.
        let segments: Vec<String> = if inside.is_empty() {
            Vec::new()
        } else {
            inside.split('/').map(str::to_owned).collect()
        };
        // An empty segment anywhere else is a record that did not come out of git.
        if segments.iter().any(String::is_empty) {
            continue;
        }
        rows.push(GitEntryDto {
            path: segments,
            index: index.to_string(),
            worktree: worktree.to_string(),
            is_dir,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build what git writes: the two letters, a space, the path, and the NUL that ends it.
    fn z(records: &[&str]) -> String {
        records.iter().map(|r| format!("{r}\0")).collect()
    }

    #[test]
    fn takes_the_repository_prefix_off_every_path() {
        let out = z(&["A  app/keep.txt", "?? app/untracked.txt"]);
        let rows = rows(&out, "app/");
        assert_eq!(
            rows.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
            vec![vec!["keep.txt".to_string()], vec!["untracked.txt".to_string()]]
        );
    }

    #[test]
    fn a_folder_bound_at_its_repositorys_root_has_nothing_to_take_off() {
        let rows = rows(&z(&[" M src/lib.rs"]), "");
        assert_eq!(rows[0].path, vec!["src".to_string(), "lib.rs".to_string()]);
        assert_eq!((rows[0].index.as_str(), rows[0].worktree.as_str()), (" ", "M"));
    }

    #[test]
    fn a_name_in_another_language_arrives_as_itself() {
        let rows = rows(&z(&["?? app/日本語.txt"]), "app/");
        assert_eq!(rows[0].path, vec!["日本語.txt".to_string()]);
    }

    /// The second path of a rename is where the file came from, and no row is drawn for it.
    #[test]
    fn a_rename_draws_one_row_at_the_name_it_has_now() {
        let out = format!("R  app/new.txt\0app/old.txt\0{}", z(&["?? app/other.txt"]));
        let rows = rows(&out, "app/");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, vec!["new.txt".to_string()]);
        assert_eq!(rows[1].path, vec!["other.txt".to_string()]);
    }

    /// git names an untracked folder rather than everything in it, and the row keeps that apart
    /// from a file of the same name.
    #[test]
    fn a_folder_git_answers_for_as_a_whole_says_it_is_one() {
        let rows = rows(&z(&["?? app/newdir/", "?? app/newdir.txt"]), "app/");
        assert_eq!((rows[0].path.clone(), rows[0].is_dir), (vec!["newdir".to_string()], true));
        assert_eq!((rows[1].path.clone(), rows[1].is_dir), (vec!["newdir.txt".to_string()], false));
    }

    /// Everything about a path that is not under the bound folder, on the way in rather than in
    /// front of a reader: git answers about the folder, so a record that is not in it is a record
    /// the tree has no row for.
    #[test]
    fn a_path_outside_the_folder_draws_no_row() {
        assert!(rows(&z(&["?? other/thing.txt"]), "app/").is_empty());
    }

    /// Nothing under the folder is tracked, so the folder git names is the bound folder — the one
    /// row whose path has no segments in it.
    #[test]
    fn a_wholly_untracked_folder_is_named_as_itself() {
        let rows = rows(&z(&["?? app/"]), "app/");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].path.is_empty(), "the bound folder is no segments, not one empty one");
        assert!(rows[0].is_dir);
    }

    #[test]
    fn nothing_to_say_is_no_rows_rather_than_one_empty_one() {
        assert!(rows("", "").is_empty());
        assert!(rows("\0", "").is_empty());
    }

    /// The whole of it against a real git: the options in the order git takes them, the front it
    /// puts on every path, and a name that is not ASCII surviving both. None of that can be pinned
    /// by handing the parser bytes somebody wrote by hand.
    #[test]
    fn a_folder_below_its_repositorys_root_reads_its_own_rows() {
        if amenbo_core::sys::git().is_none() {
            return; // Nothing to pin on a machine with no git: every road here answers nothing.
        }
        let repo = amenbo_scratch::scratch("app-foldergit-repo");
        let app = repo.join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(repo.join("outside.txt"), "o").unwrap();
        std::fs::write(app.join("keep.txt"), "k").unwrap();
        std::fs::write(app.join("日本語.txt"), "n").unwrap();
        run(&repo, &["init", "-q"]).expect("git init");
        // One path is tracked, so git names what is left one by one instead of answering for the
        // folder as a whole — which is what puts a path in front of the parser at all.
        run(&app, &["add", "keep.txt"]).expect("git add");

        let repo_of_app = repo_of(&app).expect("the folder is inside a repository");
        assert_eq!(repo_of_app.prefix, "app/", "git answers from its own root, not from the folder");

        let out = run(&app, &["--no-optional-locks", "status", "--porcelain=v1", "-z", "--", "."])
            .expect("git status");
        let mut named: Vec<String> =
            rows(&out, &repo_of_app.prefix).iter().map(|row| row.path.join("/")).collect();
        named.sort();
        assert_eq!(named, vec!["keep.txt".to_string(), "日本語.txt".to_string()]);
    }

    /// A folder that is not in a repository, which is most of them.
    #[test]
    fn a_folder_outside_any_repository_has_no_repository() {
        if amenbo_core::sys::git().is_none() {
            return;
        }
        let plain = amenbo_scratch::scratch("app-foldergit-plain");
        assert!(repo_of(&plain).is_none());
    }
}
