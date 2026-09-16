//! What git says about the folder the file face is showing: the colour on a tree row, where the
//! branch stands, the commits behind both, and the branches and stashes a reader is offered to
//! choose from.
//!
//! It is one `git status` per bound folder, asked for rather than kept up to date — the face asks
//! when it opens the panel — and read for display and nothing else: no pane is marked from it and
//! nobody is told their turn has come (`AMB-D-774`). A folder that is not a repository, and a
//! machine with no git that can be run without asking the reader to install a compiler, both answer
//! the same way — nothing.
//!
//! **Everything here reads; nothing here writes** — the other half is
//! [`crate::folder_git_write`] (`AMB-D-906`). The line between the two modules is the answer a road
//! gives when it comes to nothing: here it is an empty hand, because a folder that is no repository
//! is the ordinary case and not a failure to report, and there it is git's own refusal, word for
//! word.
//!
//! **What a commit costs is what is asked of it, and the asking is split up.** The history list is
//! one call and carries no file names, because putting them on it takes one call from 19ms to
//! between 53 and 254ms; the names come when a commit is opened, and the patch when one of its
//! files is (`AMB-T-4899`). What the branch stands at rides on the status call rather than being
//! counted by one of its own.
//!
//! **A commit is read as the difference from its first parent**, never as `show` (`AMB-T-4919`).
//! `show` answers for a merge with a summary of a couple of hundred bytes, so a road built on it
//! has to ask whether each commit is one — and the first-parent form gives the same answer for an
//! ordinary commit while needing no such question.
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

use crate::dto::{FolderGitDto, GitBranchDto, GitCommitDto, GitEntryDto, GitFileDto, GitStashDto};
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
///
/// **Off the main thread.** A command with no `async` on it is run where the webview is drawn
/// ([`crate::agent_models`]), and what this one waits on is git starting up and reading an index
/// whose size is the repository's, not ours. A project's folders each ask for themselves, so the
/// waits queue up and the window stands still for the sum — 337 ms over six folders on Windows,
/// against 70 ms for one (`AMB-T-4897`).
#[tauri::command]
pub async fn folder_git_status(project_id: i64, root: String) -> Result<FolderGitDto, CmdError> {
    off_thread(move || {
        let dir = root_of(project_id, &root)?;
        let Some(repo) = repo_of(&dir) else { return Ok(FolderGitDto::default()) };
        // `--no-optional-locks` sits before `status` because it is git's own option and not the
        // subcommand's; behind it git exits 129 without doing anything. What it buys is the index
        // lock: without it this call races the reader's own `git add` and breaks it — 92.8% of the
        // time on Linux, and never with it (`AMB-T-3742` measured all three systems).
        //
        // `-z` is what makes a name in any language come back as the bytes it really is; without it
        // git writes octal escapes instead. `-- .` holds the answer to this folder: git otherwise
        // climbs to the repository root and answers for the whole of it, at eight times the cost.
        //
        // `--branch` puts one more line at the front and costs nothing to ask for. Counting the
        // same thing with `rev-list --count` would be a second process, which is 14ms of a call
        // that is 20ms whole (`AMB-T-4899`).
        let args = ["--no-optional-locks", "status", "--porcelain=v1", "-z", "--branch", "--", "."];
        let Some(out) = run(&dir, &args) else {
            return Ok(FolderGitDto { prefix: repo.prefix, ..Default::default() });
        };
        // The branch line is the first record and `--branch` always writes one, so what follows the
        // first NUL is the rows — which is also what keeps the `##` out of the row parser, where it
        // would read as a path wearing two status letters.
        let (head, named) = out.split_once('\0').unwrap_or((out.as_str(), ""));
        let rows = rows(named, &repo.prefix);
        // Whether a merge is underway, asked of the file git itself asks — and free, because the
        // directory it is in was read once and kept (`repo_of`). A second process to ask git the
        // same question would be 14ms of a call that is 20ms whole (`AMB-T-4899`), and the watch
        // the rail lays covers this directory, so the file appearing and going is already a reason
        // to read again (`crate::folder_watch`).
        let merging = repo.git_dir.join("MERGE_HEAD").exists();
        Ok(FolderGitDto { prefix: repo.prefix, branch: branch_of(head), rows, merging })
    })
    .await
}

/// Run one git road where waiting on it costs nobody the window.
///
/// Every command that goes to git waits on it starting up and reading an index whose size is the
/// repository's, not ours — and a command with no `async` on it is run where the webview is drawn
/// ([`crate::agent_models`]). What the thread is given back for is the same on all of them, so the
/// words for a road that did not finish are written once here rather than at each of them, and
/// [`crate::folder_git_write`] takes the same road out.
pub(crate) async fn off_thread<T, F>(work: F) -> Result<T, CmdError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CmdError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| -> CmdError {
            format!("asking git about this folder did not finish: {e}").into()
        })?
}

/// The `--format` the history is read back by: the fields a row is drawn from, in one record each.
///
/// The unit separator is what stands between the fields, because it is the one byte none of them
/// can hold — a subject is a line of a commit message and a name is a name, and both can hold
/// anything a person can type. The records themselves are ended by NUL, which `-z` asks for.
const LOG_FIELDS: &str = "--format=%H%x1f%h%x1f%P%x1f%an%x1f%aI%x1f%s";

/// The commits behind the folder `root` names, newest first — the whole repository's, or one path's
/// where `path` names one.
///
/// `path` is spelled from the bound folder, the way every row that offers a file history spells
/// its own paths — a row of the tree, and a changed path in the rail's git half. git is run in that
/// folder and measures a pathspec from there, so it is read as written.
///
/// **It is not the spelling a commit's own files come back in** (`folder_git_show`), which is the
/// repository's. Each road takes what the rows that reach it already hold, rather than making one
/// of them do arithmetic on a path to ask about it.
///
/// **A hundred and no more.** Reading 30 rather than 100 saves nothing that can be measured — what
/// the call costs is git starting up (`AMB-T-4899`) — and a hundred is the length of a scroll
/// somebody actually reads to the end of.
#[tauri::command]
pub async fn folder_git_log(
    project_id: i64,
    root: String,
    path: Option<String>,
) -> Result<Vec<GitCommitDto>, CmdError> {
    off_thread(move || {
        let dir = root_of(project_id, &root)?;
        if repo_of(&dir).is_none() {
            return Ok(Vec::new());
        }
        let mut args = vec!["--no-optional-locks", "log", "--max-count=100", "-z", LOG_FIELDS];
        // After `--`, so a path that begins like an option is read as the path it is.
        let spec = path.as_deref().map(here);
        if let Some(spec) = spec.as_deref() {
            args.push("--");
            args.push(spec);
        }
        let Some(out) = run(&dir, &args) else { return Ok(Vec::new()) };
        Ok(commits(&out))
    })
    .await
}

/// What one commit touched, as the rows the layer under it draws.
///
/// The counts are `--numstat` rather than the `--stat` a person reads: the two carry the same
/// answer, and the one written for people truncates a long path with an ellipsis, which is a name
/// nothing can be opened by.
#[tauri::command]
pub async fn folder_git_show(
    project_id: i64,
    root: String,
    sha: String,
) -> Result<Vec<GitFileDto>, CmdError> {
    off_thread(move || {
        let dir = root_of(project_id, &root)?;
        if repo_of(&dir).is_none() || !is_sha(&sha) {
            return Ok(Vec::new());
        }
        let Some(out) = from_first_parent(&dir, &sha, &["--numstat", "-z"], &[]) else {
            return Ok(Vec::new());
        };
        Ok(files(&out))
    })
    .await
}

/// The patch for one path of one commit, as git wrote it.
///
/// `path` is the repository's own spelling, which is what a commit's files come back as
/// (`folder_git_show`) — see [`whole`] for why that has to be said to git rather than assumed.
///
/// It is handed over as git's own text rather than read into rows here: what a diff means is one
/// answer, and a face that draws it and a face that puts it in front of an editor are reading the
/// same bytes.
#[tauri::command]
pub async fn folder_git_diff(
    project_id: i64,
    root: String,
    sha: String,
    path: String,
) -> Result<String, CmdError> {
    off_thread(move || {
        let dir = root_of(project_id, &root)?;
        if repo_of(&dir).is_none() || !is_sha(&sha) {
            return Ok(String::new());
        }
        let spec = whole(&path);
        Ok(from_first_parent(&dir, &sha, &["--patch"], &["--", &spec]).unwrap_or_default())
    })
    .await
}

/// The `--format` the branch list is read back by: the name, what it is measured against, and how
/// it stands against it.
///
/// A unit separator stands between the fields and a plain newline between the records, with no `-z`
/// asked for: a ref name can hold neither a space nor a control character, and git refuses to make
/// one that does (`check-ref-format`). That is the one field here somebody chose the bytes of.
const BRANCH_FIELDS: &str = "--format=%(refname:short)%1f%(upstream:short)%1f%(upstream:track)";

/// The `--format` the stash list is read back by. `%gd` is where the stash sits (`stash@{0}`), `%gs`
/// the line git wrote on it, and `%aI` when it was made — and `-z` ends each record, because `%gs`
/// is a message and holds whatever was typed into it.
const STASH_FIELDS: &str = "--format=%gd%x1f%gs%x1f%aI";

/// Every branch of the folder's repository, with where each one stands against its upstream.
///
/// It is one `for-each-ref` rather than a `status` per branch: the counts ride on the same call as
/// the names, the same way the checked-out branch's ride on [`folder_git_status`]
/// (`AMB-T-4899` measured what a second process costs).
///
/// **Which branch is the one checked out is not carried.** [`folder_git_status`] already answers
/// that, and a second answer to the same question is one that can disagree with the first.
#[tauri::command]
pub async fn folder_git_branches(
    project_id: i64,
    root: String,
) -> Result<Vec<GitBranchDto>, CmdError> {
    off_thread(move || {
        let dir = root_of(project_id, &root)?;
        if repo_of(&dir).is_none() {
            return Ok(Vec::new());
        }
        let args = ["--no-optional-locks", "for-each-ref", BRANCH_FIELDS, "refs/heads"];
        Ok(run(&dir, &args).map(|out| branches(&out)).unwrap_or_default())
    })
    .await
}

/// What has been stashed in the folder's repository, newest first — which is the order git keeps
/// them in, `stash@{0}` being the last one made.
#[tauri::command]
pub async fn folder_git_stashes(
    project_id: i64,
    root: String,
) -> Result<Vec<GitStashDto>, CmdError> {
    off_thread(move || {
        let dir = root_of(project_id, &root)?;
        if repo_of(&dir).is_none() {
            return Ok(Vec::new());
        }
        let args = ["--no-optional-locks", "stash", "list", "-z", STASH_FIELDS];
        Ok(run(&dir, &args).map(|out| stashes(&out)).unwrap_or_default())
    })
    .await
}

/// One path of the bound folder, spelled so git reads it as a path and not as a pattern.
///
/// A file called `a[1].txt` otherwise also matches `a1.txt`, which is one file's history drawn
/// under another's name. Where git measures it from is where git is run, which is that folder.
fn here(path: &str) -> String {
    format!(":(literal){path}")
}

/// One repository-relative path, spelled so git reads it as that path and from that root.
///
/// **`top` is what makes the root the repository's.** git measures a pathspec from where it is run,
/// which here is the bound folder — so a folder bound at `repo/app` asked about `app/main.rs` would
/// be asked about `repo/app/app/main.rs`, and answer with nothing. What reaches this is a path a
/// commit named, and a commit names the repository's.
///
/// **`literal` is what makes it a path rather than a pattern**, for the reason [`here`] gives.
fn whole(path: &str) -> String {
    format!(":(top,literal){path}")
}

/// Whether a commit is named by something git will read as a commit and not as an option.
///
/// Every sha here came out of [`folder_git_log`], so this turns nothing away that a reader could
/// have asked for — and a name arriving from anywhere else is one nothing vouches for. A word
/// beginning with `-` would be read as an option by every command above, which is the whole of what
/// this is in the way of.
fn is_sha(sha: &str) -> bool {
    !sha.is_empty() && sha.len() <= 40 && sha.chars().all(|c| c.is_ascii_hexdigit())
}

/// One commit read as what it changed against its first parent, with `how` saying in what shape.
///
/// **The first parent and not `show`**, for every commit and with no question asked about which
/// kind it is: `show --patch` answers for a merge with a couple of hundred bytes of summary, and
/// the difference from the first parent is what a reader means by "what this commit did" either way
/// (`AMB-T-4919`, and VS Code reads a merge the same way).
///
/// The first commit of a repository has no parent to measure from, and that is the one case `show`
/// is asked instead — where it is also exactly right, since everything in that commit is new.
fn from_first_parent(dir: &Path, sha: &str, how: &[&str], about: &[&str]) -> Option<String> {
    let parent = format!("{sha}^");
    let mut args = vec!["--no-optional-locks", "diff"];
    args.extend_from_slice(how);
    args.push(&parent);
    args.push(sha);
    args.extend_from_slice(about);
    if let Some(out) = run(dir, &args) {
        return Some(out);
    }
    let mut args = vec!["--no-optional-locks", "show", "--format="];
    args.extend_from_slice(how);
    args.push(sha);
    args.extend_from_slice(about);
    run(dir, &args)
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

/// Read the `--branch` line at the front of `status` into where the branch stands.
///
/// git writes one line however the checkout stands, and the three shapes it has are three different
/// answers rather than degrees of one: a branch measured against another, a branch measured against
/// nothing, and a checkout that is not on a branch at all.
///
/// `[ahead 1, behind 2]` is the only bracket on that line and a ref cannot hold a space, so the
/// counts are found by the space before it and nothing has to be escaped or counted through.
fn branch_of(line: &str) -> Option<GitBranchDto> {
    let line = line.strip_prefix("## ")?;
    // A repository nobody has committed in yet. git names the branch the first commit would land
    // on, and there is nothing yet to measure it by.
    if let Some(name) = line.strip_prefix("No commits yet on ") {
        return Some(GitBranchDto { name: Some(name.to_string()), ..Default::default() });
    }
    // A checkout made at a commit rather than at a branch. There is no name to carry: what git
    // writes here is the words, in English, whatever the reader's language is.
    if line == "HEAD (no branch)" {
        return Some(GitBranchDto::default());
    }
    let (head, counts) = match line.rsplit_once(" [") {
        Some((head, counts)) => (head, counts.trim_end_matches(']')),
        None => (line, ""),
    };
    // Three dots, which is the one thing a ref cannot hold: git refuses a name with two dots
    // running together, so this cannot be part of either side.
    let (name, upstream) = match head.split_once("...") {
        Some((name, upstream)) => (name, Some(upstream.to_string())),
        None => (head, None),
    };
    Some(GitBranchDto {
        name: Some(name.to_string()),
        // `[gone]` is git saying the upstream it was told to measure by is not there any more, and
        // it leaves both counts at nothing — which is the same hand as having none.
        upstream: upstream.filter(|_| counts != "gone"),
        ahead: counted(counts, "ahead"),
        behind: counted(counts, "behind"),
    })
}

/// One of the two counts out of what stood in the brackets, and nothing where it was not there.
fn counted(counts: &str, which: &str) -> u32 {
    counts
        .split(", ")
        .find_map(|one| one.strip_prefix(which)?.trim().parse().ok())
        .unwrap_or(0)
}

/// Read `log -z` with [`LOG_FIELDS`] into rows, one record per commit.
///
/// A record short of its fields is dropped rather than half-read: every field is written by the
/// same format string, so a record missing one did not come out of git.
fn commits(out: &str) -> Vec<GitCommitDto> {
    out.split('\0')
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut field = record.split('\u{1f}');
            Some(GitCommitDto {
                sha: field.next()?.to_string(),
                short: field.next()?.to_string(),
                // Space-separated, and none at all for the first commit there was.
                parents: field.next()?.split_whitespace().map(str::to_owned).collect(),
                author: field.next()?.to_string(),
                at: field.next()?.to_string(),
                subject: field.next()?.to_string(),
            })
        })
        .collect()
}

/// Read [`BRANCH_FIELDS`] into one row per branch, in the order git wrote them — by name, which is
/// what `for-each-ref` sorts by when it is told nothing else.
///
/// **How a branch stands is the same brackets `status --branch` writes**, so it is read by the same
/// hand ([`counted`]): `[ahead 1, behind 2]`, `[gone]`, or nothing at all where there is nothing to
/// measure against or nothing between them.
fn branches(out: &str) -> Vec<GitBranchDto> {
    out.lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut field = line.split('\u{1f}');
            let name = field.next()?.to_string();
            let upstream = field.next()?.to_string();
            let track = field.next()?.trim_start_matches('[').trim_end_matches(']');
            Some(GitBranchDto {
                name: Some(name),
                // A branch measured against nothing writes the field empty; one measured against a
                // branch that is gone writes the name and then says so. Both are the same hand to
                // a reader — there is nothing to count against — and `branch_of` reads the
                // checked-out branch's the same way.
                upstream: Some(upstream).filter(|it| !it.is_empty() && track != "gone"),
                ahead: counted(track, "ahead"),
                behind: counted(track, "behind"),
            })
        })
        .collect()
}

/// Read [`STASH_FIELDS`] into one row per stash, newest first.
///
/// A record short of its fields is dropped rather than half-read, for the reason [`commits`] drops
/// one: every field is written by the same format string.
fn stashes(out: &str) -> Vec<GitStashDto> {
    out.split('\0')
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut field = record.split('\u{1f}');
            Some(GitStashDto {
                name: field.next()?.to_string(),
                message: field.next()?.to_string(),
                at: field.next()?.to_string(),
            })
        })
        .collect()
}

/// Read `--numstat -z` into the rows one commit's layer draws.
///
/// A record is the two counts and the path, tabs between them and a NUL at the end. A file git
/// reads as bytes has `-` for both counts, which is git saying it counts no lines there rather than
/// that it counted none.
///
/// **A rename writes its path nowhere and its two names next.** The path field comes back empty and
/// the record is followed by two more — where the file was, then where it is — which is the reverse
/// of the arrow form git writes for people to read.
fn files(out: &str) -> Vec<GitFileDto> {
    let mut files = Vec::new();
    let mut fields = out.split('\0');
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut part = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (part.next(), part.next(), part.next())
        else {
            continue;
        };
        let (from, path) = if path.is_empty() {
            let (Some(was), Some(now)) = (fields.next(), fields.next()) else { continue };
            (Some(was.to_string()), now.to_string())
        } else {
            (None, path.to_string())
        };
        let (added, removed) = (added.parse().ok(), removed.parse().ok());
        files.push(GitFileDto { path, from, added, removed });
    }
    files
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

    // ── where the branch stands ──────────────────────────────────────────────────────────────

    #[test]
    fn a_branch_measured_against_another_carries_both_counts() {
        let on = branch_of("## main...origin/main [ahead 1, behind 2]").unwrap();
        assert_eq!(on.name.as_deref(), Some("main"));
        assert_eq!(on.upstream.as_deref(), Some("origin/main"));
        assert_eq!((on.ahead, on.behind), (1, 2));
    }

    /// git writes only the count there is one of, so the other is read off a line that never
    /// mentions it.
    #[test]
    fn one_count_alone_leaves_the_other_at_nothing() {
        let ahead = branch_of("## main...origin/main [ahead 3]").unwrap();
        assert_eq!((ahead.ahead, ahead.behind), (3, 0));
        let behind = branch_of("## main...origin/main [behind 4]").unwrap();
        assert_eq!((behind.ahead, behind.behind), (0, 4));
    }

    #[test]
    fn a_branch_level_with_its_upstream_has_no_brackets_and_no_counts() {
        let on = branch_of("## task/4927...origin/task/4927").unwrap();
        assert_eq!(on.upstream.as_deref(), Some("origin/task/4927"));
        assert_eq!((on.ahead, on.behind), (0, 0));
    }

    /// A branch nobody has pushed. There is a name and nothing to measure it by, which is a
    /// different answer from being level with something.
    #[test]
    fn a_branch_with_no_upstream_is_named_and_measured_against_nothing() {
        let on = branch_of("## wip").unwrap();
        assert_eq!(on.name.as_deref(), Some("wip"));
        assert_eq!(on.upstream, None);
    }

    /// The upstream it was told to measure by is not there any more. Nothing can be counted against
    /// it, so it is handed over as no upstream rather than as a name that answers nothing.
    #[test]
    fn an_upstream_that_is_gone_is_handed_over_as_none() {
        let on = branch_of("## wip...origin/wip [gone]").unwrap();
        assert_eq!(on.name.as_deref(), Some("wip"));
        assert_eq!(on.upstream, None);
        assert_eq!((on.ahead, on.behind), (0, 0));
    }

    /// A checkout made at a commit rather than at a branch. What git writes there is words and not
    /// a name, so no name is carried.
    #[test]
    fn a_checkout_that_is_on_no_branch_carries_no_name() {
        let on = branch_of("## HEAD (no branch)").unwrap();
        assert_eq!(on.name, None);
        assert_eq!(on.upstream, None);
    }

    /// A repository nobody has written in yet. git names the branch the first record would land on.
    #[test]
    fn a_repository_with_nothing_in_it_names_the_branch_it_would_make() {
        let on = branch_of("## No commits yet on main").unwrap();
        assert_eq!(on.name.as_deref(), Some("main"));
        assert_eq!(on.upstream, None);
    }

    /// Anything that is not the line — which is what a row of the status is, and what an answer
    /// with nothing in it is.
    #[test]
    fn a_line_that_is_not_the_branch_line_is_no_branch() {
        assert!(branch_of(" M src/lib.rs").is_none());
        assert!(branch_of("").is_none());
    }

    // ── the history ──────────────────────────────────────────────────────────────────────────

    /// Build what `log -z` writes with [`LOG_FIELDS`]: the fields with a unit separator between
    /// them, and a NUL after each record.
    fn logged(records: &[[&str; 6]]) -> String {
        records.iter().map(|one| format!("{}\0", one.join("\u{1f}"))).collect()
    }

    #[test]
    fn a_record_is_read_into_the_fields_a_row_is_drawn_from() {
        let out = logged(&[[
            "b92495cedd4ec1600395d13b8a60c5f3e4b01d0b",
            "b92495ce",
            "50fa3f83f27fe6fb6af437602f88e0dcc8287e8e",
            "Alice",
            "2026-09-16T21:59:56+09:00",
            "feat(gui): draw one folder in the rail",
        ]]);
        let read = commits(&out);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].short, "b92495ce");
        assert_eq!(read[0].parents, vec!["50fa3f83f27fe6fb6af437602f88e0dcc8287e8e".to_string()]);
        assert_eq!(read[0].author, "Alice");
        assert_eq!(read[0].at, "2026-09-16T21:59:56+09:00");
        assert_eq!(read[0].subject, "feat(gui): draw one folder in the rail");
    }

    /// The two ends of the shape the face draws the lines of: a merge is made on top of two, and
    /// the first one there was is made on top of none.
    #[test]
    fn a_merge_carries_two_parents_and_the_first_one_carries_none() {
        let a = "a".repeat(40);
        let d = "d".repeat(40);
        let out = logged(&[
            [&a, "aaaaaaaa", "bbbb cccc", "Alice", "2026-01-01T00:00:00Z", "Merge"],
            [&d, "dddddddd", "", "Bob", "2025-01-01T00:00:00Z", "first"],
        ]);
        let read = commits(&out);
        assert_eq!(read[0].parents.len(), 2);
        assert!(read[1].parents.is_empty(), "the first one was made on top of nothing");
    }

    /// A subject is a line somebody typed, so it holds whatever a reader types — and the separator
    /// between the fields is the one byte it cannot.
    #[test]
    fn a_subject_holding_its_own_punctuation_arrives_whole() {
        let a = "a".repeat(40);
        let subject = "fix: 「変わったもの」を消す — a\ttab, and a ...b";
        let out = logged(&[[&a, "aaaaaaaa", "", "Alice", "2026-01-01T00:00:00Z", subject]]);
        assert_eq!(commits(&out)[0].subject, subject);
    }

    #[test]
    fn a_history_with_nothing_in_it_is_no_rows_rather_than_one_empty_one() {
        assert!(commits("").is_empty());
        assert!(commits("\0").is_empty());
    }

    // ── what one record touched ──────────────────────────────────────────────────────────────

    #[test]
    fn a_touched_file_carries_what_it_gained_and_lost() {
        let read = files("18\t4\tapp/src/shell/TerminalFace.tsx\0");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].path, "app/src/shell/TerminalFace.tsx");
        assert_eq!((read[0].added, read[0].removed), (Some(18), Some(4)));
        assert_eq!(read[0].from, None);
    }

    /// git writes `-` for both where it read the file as bytes. That is git counting no lines
    /// there, which is not the same as counting none.
    #[test]
    fn a_file_git_counts_no_lines_in_carries_no_counts() {
        let read = files("-\t-\tapp/icon.png\0");
        assert_eq!((read[0].added, read[0].removed), (None, None));
        assert_eq!(read[0].path, "app/icon.png");
    }

    /// A rename leaves the path empty and writes the two names as records of their own — where it
    /// was, then where it is. The row is drawn at the name it has now.
    #[test]
    fn a_rename_is_drawn_at_the_name_it_has_now_and_says_where_it_was() {
        let read = files(concat!("2\t2\t\0old/name.rs\0new/name.rs\0", "3\t1\tother.rs\0"));
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].path, "new/name.rs");
        assert_eq!(read[0].from.as_deref(), Some("old/name.rs"));
        assert_eq!(read[1].path, "other.rs");
        assert_eq!(read[1].from, None);
    }

    #[test]
    fn one_that_touched_nothing_is_no_rows() {
        assert!(files("").is_empty());
        assert!(files("\0").is_empty());
    }

    // ── the branches, and what has been stashed ──────────────────────────────────────────────

    /// Build what `for-each-ref` writes with [`BRANCH_FIELDS`]: the fields with a unit separator
    /// between them, one line each.
    fn listed(records: &[[&str; 3]]) -> String {
        records.iter().map(|one| format!("{}\n", one.join("\u{1f}"))).collect()
    }

    #[test]
    fn a_branch_row_carries_its_name_its_upstream_and_both_counts() {
        let read = branches(&listed(&[["main", "origin/main", "[ahead 1, behind 2]"]]));
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name.as_deref(), Some("main"));
        assert_eq!(read[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!((read[0].ahead, read[0].behind), (1, 2));
    }

    /// The two ways a branch has nothing to be measured against: it was told nothing to measure by,
    /// and the one it was told is not there any more. A reader can do the same thing with either,
    /// which is nothing, so both arrive as no upstream.
    #[test]
    fn a_branch_with_nothing_to_measure_against_carries_no_upstream() {
        let read = branches(&listed(&[["wip", "", ""], ["old", "origin/old", "[gone]"]]));
        assert_eq!((read[0].upstream.clone(), read[0].ahead, read[0].behind), (None, 0, 0));
        assert_eq!((read[1].upstream.clone(), read[1].ahead, read[1].behind), (None, 0, 0));
    }

    /// Level with its upstream. git writes the field empty rather than writing two zeroes, and the
    /// row says there is an upstream and nothing between them.
    #[test]
    fn a_branch_level_with_its_upstream_keeps_the_upstream_and_counts_nothing() {
        let read = branches(&listed(&[["main", "origin/main", ""]]));
        assert_eq!(read[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!((read[0].ahead, read[0].behind), (0, 0));
    }

    #[test]
    fn a_repository_with_no_branches_is_no_rows_rather_than_one_empty_one() {
        assert!(branches("").is_empty());
        assert!(branches("\n").is_empty());
    }

    #[test]
    fn a_stash_row_carries_where_it_sits_what_git_wrote_on_it_and_when() {
        let read = stashes("stash@{0}\u{1f}On main: 書きかけ\u{1f}2026-09-16T22:58:06+09:00\0");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "stash@{0}");
        assert_eq!(read[0].message, "On main: 書きかけ");
        assert_eq!(read[0].at, "2026-09-16T22:58:06+09:00");
    }

    #[test]
    fn nothing_stashed_is_no_rows_rather_than_one_empty_one() {
        assert!(stashes("").is_empty());
        assert!(stashes("\0").is_empty());
    }

    // ── the names a record may be asked by ───────────────────────────────────────────────────

    /// Every sha reaching the roads below came out of the history, and a word that did not is one
    /// git would read as an option instead.
    #[test]
    fn only_a_name_git_reads_as_a_point_in_history_is_asked_by() {
        assert!(is_sha("b92495ce"));
        assert!(is_sha(&"a".repeat(40)));
        assert!(!is_sha(""));
        assert!(!is_sha("--all"));
        assert!(!is_sha("HEAD"));
        assert!(!is_sha(&"a".repeat(41)));
    }

    /// The whole of the history road against a real git: the format git takes, the order it answers
    /// in, and what one record changed read as the difference from its first parent. None of that
    /// can be pinned by handing the parser bytes somebody wrote by hand.
    #[test]
    fn a_repository_reads_its_own_history_back() {
        if amenbo_core::sys::git().is_none() {
            return; // Nothing to pin on a machine with no git: every road here answers nothing.
        }
        let repo = amenbo_scratch::scratch("app-foldergit-log");
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]).expect("git init");
        // Who wrote them is named on each call rather than on the machine, so a git with no name
        // set still walks this road and nothing outside the scratch folder is written to.
        const WHO: [&str; 4] = ["-c", "user.name=Alice", "-c", "user.email=alice@example.com"];
        let record = |message: &'static str| {
            let mut args = WHO.to_vec();
            args.extend_from_slice(&["commit", "-q", "-m", message]);
            args
        };

        std::fs::write(repo.join("first.txt"), "one\n").unwrap();
        run(&repo, &["add", "first.txt"]).expect("git add");
        run(&repo, &record("first")).expect("the first record");
        std::fs::write(repo.join("first.txt"), "one\ntwo\n").unwrap();
        run(&repo, &["add", "first.txt"]).expect("git add");
        run(&repo, &record("second")).expect("the second record");

        let out = run(&repo, &["--no-optional-locks", "log", "--max-count=100", "-z", LOG_FIELDS])
            .expect("git log");
        let read = commits(&out);
        assert_eq!(
            read.iter().map(|one| one.subject.clone()).collect::<Vec<_>>(),
            vec!["second".to_string(), "first".to_string()],
            "newest first, which is the order a list is read in"
        );
        assert!(read[1].parents.is_empty(), "the first one was made on top of nothing");
        assert_eq!(read[0].parents, vec![read[1].sha.clone()]);
        assert_eq!(read[0].author, "Alice");
        assert!(
            read[0].sha.starts_with(&read[0].short),
            "the short name is the front of the whole one"
        );

        // And read as what it changed, which is the road every one of them is opened by.
        let touched = files(
            &from_first_parent(&repo, &read[0].sha, &["--numstat", "-z"], &[]).expect("git diff"),
        );
        assert_eq!(touched.len(), 1);
        assert_eq!((touched[0].path.as_str(), touched[0].added), ("first.txt", Some(1)));

        // The first one has no parent to measure from, and everything in it is new — which is the
        // one place the other spelling is asked for.
        let made = files(
            &from_first_parent(&repo, &read[1].sha, &["--numstat", "-z"], &[]).expect("git show"),
        );
        assert_eq!(made.len(), 1);
        assert_eq!((made[0].path.as_str(), made[0].added), ("first.txt", Some(1)));

        // The patch for one path of it, handed over as git's own text.
        let patch =
            from_first_parent(&repo, &read[0].sha, &["--patch"], &["--", "first.txt"]).unwrap();
        assert!(patch.contains("+two"), "the line that was added is in the patch git wrote");
    }

    /// git measures a pathspec from where it is run, which is the bound folder — so a path the
    /// repository named has to say so, or a folder bound below the root asks about a path that is
    /// not there.
    #[test]
    fn a_path_the_repository_named_is_asked_for_as_the_repositorys_own() {
        assert_eq!(whole("app/src/main.rs"), ":(top,literal)app/src/main.rs");
        // And the folder's own spelling is measured from where git is run, which is that folder.
        assert_eq!(here("src/main.rs"), ":(literal)src/main.rs");
    }

    /// The whole of it against a real git: a folder bound below its repository's root, asked about
    /// a path spelled from that root. Without the magic word git measures it from the folder and
    /// answers about nothing, which is the shape this exists to stop.
    #[test]
    fn a_folder_below_its_root_reads_the_history_of_a_path_the_repository_named() {
        if amenbo_core::sys::git().is_none() {
            return;
        }
        let repo = amenbo_scratch::scratch("app-foldergit-whole");
        let app = repo.join("app");
        std::fs::create_dir_all(&app).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]).expect("git init");
        const WHO: [&str; 4] = ["-c", "user.name=Alice", "-c", "user.email=alice@example.com"];
        std::fs::write(app.join("main.rs"), "fn main() {}\n").unwrap();
        run(&repo, &["add", "-A"]).expect("git add");
        let mut args = WHO.to_vec();
        args.extend_from_slice(&["commit", "-q", "-m", "the only record"]);
        run(&repo, &args).expect("the record");

        // The history of one path, asked for from the bound folder in that folder's own spelling.
        let mut asked =
            vec!["--no-optional-locks", "log", "--max-count=100", "-z", LOG_FIELDS, "--"];
        let mine = here("main.rs");
        asked.push(&mine);
        let read = commits(&run(&app, &asked).expect("git log"));
        assert_eq!(read.len(), 1, "the record that touched it, found from a folder below the root");
        assert_eq!(read[0].subject, "the only record");

        // And the patch for the same file, asked for in the spelling a commit hands back — which
        // is the repository's, and would name nothing measured from the bound folder.
        let named = whole("app/main.rs");
        let patch = from_first_parent(&app, &read[0].sha, &["--patch"], &["--", &named]).unwrap();
        assert!(patch.contains("+fn main()"), "the line the record added");
        let missed = from_first_parent(&app, &read[0].sha, &["--patch"], &["--", &here("app/main.rs")])
            .unwrap();
        assert!(missed.is_empty(), "the repository's spelling is not the folder's");
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
