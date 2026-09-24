//! Cutting a task its own worktree, and folding it up again — the git half of `worktree start` /
//! `worktree finish` (`AMB-D-881`).
//!
//! Its sibling [`crate::worktree`] asks whether the folder Amenbo is being *run* in is a place to
//! drive a backlog from; this one makes the folder that is not. The two never meet: a worktree cut
//! here stands beside the project rather than inside it, which is the shape that guard leaves alone.
//!
//! **Where it goes is fixed and unasked** — `<the repository's parent>/<its name>-worktrees/<id>`, on
//! `task/<id>`. A setting for it would have to be right in every repository Amenbo is used in, and the
//! answer it would hold is one nobody has a reason to differ on.
//!
//! **Nothing here reads the backlog.** Whether the task is one this repository works is settled by the
//! caller, which has the store open; what is left is the git, and git is all this speaks.

use std::path::{Path, PathBuf};

/// Why a cut, or a fold, did not happen. Each variant is one sentence the CLI writes — the prose and
/// the code that names it live there, so the refusals read as Amenbo's rather than as git's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// This machine has no git that can be run without asking the person to install something
    /// ([`crate::sys::git`]).
    NoGit,
    /// The folder the command was typed in lies in no repository, with git's own reason.
    NotARepository(String),
    /// A worktree is already standing for this task. Whose it is cannot be read from disk.
    WorktreeExists(PathBuf),
    /// `task/<id>` is already a branch, with no worktree checked out on it.
    BranchExists(String),
    /// The fold was asked for where there is nothing to fold.
    NoWorktree(PathBuf),
    /// The checkout carries changes nobody has committed.
    Dirty(PathBuf),
    /// The branch carries commits whose changes `base` does not have.
    Unmerged { branch: String, base: String },
    /// A path that cannot be written into a `cd` line both shells read alike.
    Unquotable(PathBuf),
    /// git ran and refused, carrying its own reason.
    Git(String),
    /// The filesystem refused — creating the sibling directory, or sweeping the checkout away.
    Io(String),
}

/// The branch one task's worktree is checked out on.
pub fn branch_name(id: &str) -> String {
    format!("task/{id}")
}

/// Where one task's worktree stands, and what it is called — derived from the repository the command
/// was typed in, never from the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    /// The **main** worktree's root, which is where git is run from and where the branch lives.
    pub root: PathBuf,
    /// The sibling directory every task's worktree is cut inside.
    pub parent: PathBuf,
    /// This task's worktree.
    pub worktree: PathBuf,
    /// `task/<id>`.
    pub branch: String,
}

/// A git call that did not succeed: the call, and what git said about it. The two are kept apart
/// because the callers use them differently — most repeat both, and [`git_root`] wants git's reason
/// alone, the call it made being its own business rather than the reader's.
struct GitFailed {
    call: String,
    said: String,
}

impl From<GitFailed> for Refusal {
    fn from(f: GitFailed) -> Refusal {
        Refusal::Git(format!("{}: {}", f.call, f.said))
    }
}

/// Run one git command in `dir` and hand back its trimmed stdout. A failure carries git's own stderr,
/// so a refusal can say what git objected to instead of "exit status 128".
fn git(dir: &Path, args: &[&str]) -> Result<String, Refusal> {
    git_raw(dir, args).map_err(|f| if f.said == NO_GIT { Refusal::NoGit } else { Refusal::from(f) })
}

/// [`git`], with the failure left in its two parts.
fn git_raw(dir: &Path, args: &[&str]) -> Result<String, GitFailed> {
    let call = format!("git {}", args.join(" "));
    let Some(mut git) = crate::sys::git() else {
        // No git to run is not a git refusal, and nothing here can carry that: the caller meets it as
        // `NoGit` through `git`, which is the only door that builds a `Refusal`.
        return Err(GitFailed { call, said: NO_GIT.to_string() });
    };
    let out = match git.current_dir(dir).args(args).output() {
        Ok(out) => out,
        Err(e) => return Err(GitFailed { call, said: e.to_string() }),
    };
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let said = if said.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { said };
        return Err(GitFailed { call, said });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// What [`git_raw`] says when this machine has no git to run at all — the one reason that is not
/// git's own, and so the one [`git`] turns back into [`Refusal::NoGit`] rather than passing on.
const NO_GIT: &str = "no git on this machine";

/// The **main** worktree's root of the repository holding `dir` — not the linked worktree one might be
/// standing in. That distinction is what lets `finish` behave the same from either place:
/// `--show-toplevel` would answer with the task worktree itself, and the sibling layout would then
/// resolve one level deeper every time. `--git-common-dir` resolves to the main repository's `.git`,
/// whose parent is the root.
pub fn git_root(dir: &Path) -> Result<PathBuf, Refusal> {
    let common = git_raw(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).map_err(
        |f| if f.said == NO_GIT { Refusal::NoGit } else { Refusal::NotARepository(f.said) },
    )?;
    Path::new(&common)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| Refusal::NotARepository(format!("{common} has no parent to take as the root")))
}

/// Everything about where task `id`'s worktree goes, worked out from the folder the command was typed
/// in.
pub fn cut_for(id: &str, cwd: &Path) -> Result<Cut, Refusal> {
    Ok(layout(&git_root(cwd)?, id))
}

/// The layout itself, with the repository handed in: the sibling directory beside the root, and this
/// task's worktree inside it. Beside rather than within, because a worktree cut inside the project
/// folder inherits its `.amenbo` and could drive the real backlog from a throwaway checkout — which is
/// what [`crate::worktree::nested`] refuses, and what this placement is written never to make.
pub fn layout(root: &Path, id: &str) -> Cut {
    let name = root.file_name().unwrap_or(root.as_os_str()).to_string_lossy().into_owned();
    let parent = root.parent().unwrap_or(Path::new(".")).join(format!("{name}-worktrees"));
    Cut { worktree: parent.join(id), root: root.to_path_buf(), parent, branch: branch_name(id) }
}

/// The branch a worktree is cut from, and measured against when it is folded: the one `--base` names,
/// or the branch the repository is standing on.
///
/// The branch you are on is the answer, so there is nothing to fill in before the first `start` works.
/// A name held anywhere else would have to be right in every repository, and one whose trunk is called
/// something else would meet `invalid reference` instead of a worktree. A detached HEAD has no branch
/// to name, so it answers with the word git itself resolves — which cuts and measures from the same
/// commit, and reads as what it is wherever it is printed.
pub fn cut_from(root: &Path, flag: Option<&str>) -> String {
    if let Some(named) = flag.filter(|b| !b.is_empty()) {
        return named.to_string();
    }
    match git(root, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(name) if !name.is_empty() => name,
        _ => "HEAD".to_string(),
    }
}

/// Is `refs/heads/<branch>` there?
fn branch_exists(root: &Path, branch: &str) -> bool {
    git(root, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok()
}

/// Cut the worktree and its branch. `base` is what the branch is cut from; the name actually used comes
/// back, since it is what the account on screen names.
pub fn start(cut: &Cut, base: Option<&str>) -> Result<String, Refusal> {
    nothing_standing(cut)?;
    let from = cut_from(&cut.root, base);
    add(cut, &from, &[])?;
    Ok(from)
}

/// **Cut the worktree from the newest of the remote's default branch** — what an automation's built-in
/// cuts from (`AMB-D-964`). Whatever the repository is standing on is not asked: the main worktree may
/// be on any branch, and it is neither switched nor pulled.
///
/// The branch does not track `origin/<default>`. It is pushed under its own name, and one that tracked
/// the trunk would read as ahead of it for as long as it lives.
pub fn start_from_origin(cut: &Cut) -> Result<String, Refusal> {
    nothing_standing(cut)?;
    let from = origin_default(&cut.root)?;
    add(cut, &from, &["--no-track"])?;
    Ok(from)
}

/// **The remote's default branch, fetched fresh** — `origin/<name>`, read from `refs/remotes/origin/HEAD`
/// rather than from the forge, so no account and no forge's tool is needed to know it. A clone records
/// that ref; a repository that was given its remote by hand may not have, and git has the one command
/// that records it.
pub fn origin_default(root: &Path) -> Result<String, Refusal> {
    git(root, &["fetch", "--quiet", "origin"])?;
    git_raw(root, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]).map_err(|f| {
        if f.said == NO_GIT {
            return Refusal::NoGit;
        }
        Refusal::Git(format!(
            "origin's default branch is not recorded in {} — `git remote set-head origin --auto` records it ({})",
            root.display(),
            f.said,
        ))
    })
}

/// Refuse a cut where one is standing already: the worktree, or its branch on its own.
fn nothing_standing(cut: &Cut) -> Result<(), Refusal> {
    if cut.worktree.exists() {
        return Err(Refusal::WorktreeExists(cut.worktree.clone()));
    }
    if branch_exists(&cut.root, &cut.branch) {
        return Err(Refusal::BranchExists(cut.branch.clone()));
    }
    Ok(())
}

/// `git worktree add` itself, into the sibling directory made for it.
fn add(cut: &Cut, from: &str, flags: &[&str]) -> Result<(), Refusal> {
    std::fs::create_dir_all(&cut.parent)
        .map_err(|e| Refusal::Io(format!("{}: {e}", cut.parent.display())))?;
    let worktree = cut.worktree.to_string_lossy();
    let mut args = vec!["worktree", "add"];
    args.extend_from_slice(flags);
    args.extend_from_slice(&[&worktree, "-b", &cut.branch, from]);
    git(&cut.root, &args)?;
    Ok(())
}

/// Is every change on `branch` already in `base`, so that deleting it loses nothing?
///
/// The measure is the patch each commit carries rather than where the commit sits in the graph
/// (`AMB-D-699`). A squash and a rebase merge both land the branch's changes under commits of their
/// own, and lineage would call those unmerged forever — leaving `--force`, which also discards
/// uncommitted work, as the only way to fold a finished task down. `git cherry` marks a commit `-`
/// when base already holds an equivalent patch and `+` when it does not, and leaves merge commits out
/// of the comparison, so a branch that took base in along the way is not measured against what it took.
///
/// A patch that changed on its way in — a conflict resolved by hand — has no equivalent to find, and
/// the answer falls on the refusing side. That is the safe way to be wrong: the work is untouched, and
/// saying otherwise on purpose is what `--force` is for.
fn is_merged(root: &Path, branch: &str, base: &str) -> bool {
    match git(root, &["cherry", base, branch]) {
        Ok(cherry) => !cherry.lines().any(|line| line.starts_with('+')),
        Err(_) => false,
    }
}

/// Take the worktree and its branch away again. The base it was measured against comes back, for the
/// same reason [`start`] hands back what it cut from.
///
/// It refuses while there is anything left to lose: work nobody committed, or a branch carrying changes
/// base does not have. `force` overrides both, which is the only way to discard work on purpose.
pub fn finish(cut: &Cut, base: Option<&str>, force: bool) -> Result<String, Refusal> {
    if !cut.worktree.exists() {
        return Err(Refusal::NoWorktree(cut.worktree.clone()));
    }
    let into = cut_from(&cut.root, base);
    if !force {
        // `--no-optional-locks` sits before `status` because it is git's own option and not the
        // subcommand's; behind it git exits 129 without doing anything. What it buys is the index
        // lock: a bare `status` takes it to refresh the index, and while it holds it the reader's own
        // `git add` in the same worktree fails — 43.6% of the time on macOS and 46.9% on Linux, and
        // never with the option (`AMB-T-4901` measured this call against the other reads made here,
        // which take no lock either way).
        if !git(&cut.worktree, &["--no-optional-locks", "status", "--porcelain"])?.is_empty() {
            return Err(Refusal::Dirty(cut.worktree.clone()));
        }
        if !is_merged(&cut.root, &cut.branch, &into) {
            return Err(Refusal::Unmerged { branch: cut.branch.clone(), base: into });
        }
    }
    let path = cut.worktree.to_string_lossy().into_owned();
    let mut remove = vec!["worktree", "remove", &path];
    if force {
        remove.push("--force");
    }
    git(&cut.root, &remove)?;
    // The delete is not asking git whether anything is lost by it: reaching one at all means that
    // question is settled — either `is_merged` found the branch's changes in base, or the caller said
    // to discard them on purpose — and git's own answer is the weaker of the two, measuring lineage.
    git(&cut.root, &["branch", "-D", &cut.branch])?;
    // `git worktree remove` deletes the directory, but a leftover is swept anyway, and the sibling
    // directory is pruned once it is empty. A non-empty error there means another task's worktree still
    // lives in it, so it is left alone.
    std::fs::remove_dir_all(&cut.worktree)
        .or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) })
        .map_err(|e| Refusal::Io(format!("{}: {e}", cut.worktree.display())))?;
    let _ = std::fs::remove_dir(&cut.parent);
    Ok(into)
}

/// The one line `start` returns: `cd '<path>'`, single-quoted so it survives being fed to a shell —
/// `eval "$(…)"` in a POSIX one, `iex (…)` in PowerShell.
///
/// Single quotes are the form the two share: everything between them is literal in both, which is what
/// carries a Windows path's backslashes through unharmed (double quotes would not — POSIX reads a
/// backslash as an escape). What they do not share is how to put a single quote *inside*: POSIX ends
/// the string and re-opens it, PowerShell doubles the quote, and neither spelling is inert in the
/// other. So a path carrying one has no line that works in both, and this refuses rather than hand back
/// one that works where it was written and breaks elsewhere.
pub fn cd_line(path: &Path) -> Result<String, Refusal> {
    let text = path.to_string_lossy();
    if text.contains('\'') {
        return Err(Refusal::Unquotable(path.to_path_buf()));
    }
    Ok(format!("cd '{text}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The placement is derived from the repository's own name and sits beside it — never inside, which
    /// is the shape [`crate::worktree::nested`] refuses Amenbo from being run in.
    #[test]
    fn a_worktree_is_placed_beside_the_repository_it_was_cut_from() {
        let cut = layout(Path::new("/work/amenbo"), "4735");
        assert_eq!(cut.parent, Path::new("/work/amenbo-worktrees"));
        assert_eq!(cut.worktree, Path::new("/work/amenbo-worktrees/4735"));
        assert_eq!(cut.branch, "task/4735");
        assert!(!cut.worktree.starts_with(&cut.root), "it never lands inside the project folder");
    }

    /// The `cd` line is single-quoted, which is the one quoting both a POSIX shell and PowerShell read
    /// alike — and a path carrying a single quote has no such line, so it is refused rather than written
    /// in a form that works where it was baked and breaks elsewhere.
    #[test]
    fn the_way_in_is_quoted_for_both_shells_or_not_written_at_all() {
        assert_eq!(cd_line(Path::new("/work/amenbo-worktrees/4735")).unwrap(), "cd '/work/amenbo-worktrees/4735'");
        // A Windows path's backslashes are literal inside single quotes, so they cross unharmed.
        assert_eq!(cd_line(Path::new(r"C:\work\wt")).unwrap(), r"cd 'C:\work\wt'");
        assert_eq!(
            cd_line(Path::new("/work/it's/4735")),
            Err(Refusal::Unquotable(PathBuf::from("/work/it's/4735"))),
        );
    }

    /// `--base` names the branch when it is given; an empty one is not a name, and falls through to what
    /// the repository is standing on.
    #[test]
    fn a_named_base_wins_and_an_empty_one_is_not_a_name() {
        // A directory that is in no repository: the fallback cannot read a branch there, so it answers
        // with the word git itself resolves rather than inventing a trunk's name.
        let nowhere = amenbo_scratch::scratch("wt-cut-base");
        assert_eq!(cut_from(&nowhere, Some("release/26")), "release/26");
        assert_eq!(cut_from(&nowhere, Some("")), "HEAD");
        assert_eq!(cut_from(&nowhere, None), "HEAD");
    }
}
