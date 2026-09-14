//! `worktree start` / `worktree finish`: a task's own checkout, cut and folded (`AMB-D-881`).
//!
//! The git is [`amenbo_core::worktree_cut`]'s. What is here is the two things it deliberately does not
//! know — whether this is the repository the task is worked in, which only the store can say
//! (`AMB-D-649`), and what each refusal reads like, which is the CLI's for the reason every other
//! refusal about a folder is.

use std::path::Path;

use amenbo_core::Store;
use amenbo_core::config::Paths;
use amenbo_core::worktree_cut::{self, Cut, Refusal};
use serde_json::json;

use crate::cli::WorktreeCmd;
use crate::cmd::task::resolve_task;
use crate::output::{CliError, CliErrorCode, Flags, print_json};

pub(crate) fn worktree(store: &Store, flags: &Flags, sub: WorktreeCmd) -> Result<i32, CliError> {
    match sub {
        WorktreeCmd::Start { id, base } => start(store, flags, &id, base.as_deref()),
        WorktreeCmd::Finish { id, base, force } => finish(store, flags, &id, base.as_deref(), force),
    }?;
    Ok(0)
}

/// What a person reads while `start` is answering: it goes to stderr, because stdout is the one `cd`
/// line and nothing else. `--json` and `--quiet` silence it, the way [`crate::output::human`] does for
/// every other command's account.
fn aside(flags: &Flags, line: impl AsRef<str>) {
    if !flags.json && !flags.quiet {
        eprintln!("{}", line.as_ref());
    }
}

fn cut_for(id: i64) -> Result<Cut, CliError> {
    let cwd = std::env::current_dir().map_err(|e| refusal(Refusal::Io(e.to_string())))?;
    worktree_cut::cut_for(&id.to_string(), &cwd).map_err(refusal)
}

fn start(store: &Store, flags: &Flags, id: &str, base: Option<&str>) -> Result<(), CliError> {
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let cut = cut_for(tid)?;
    elsewhere(store, flags, tid, &cut)?;
    // Settled before anything is created: the way in is the whole return value, and a checkout on disk
    // that no returned line can enter is worse than a refusal. `--json` hands the path back as a value
    // rather than as a line for a shell, so it has nothing to quote and nothing to refuse over.
    let cd = if flags.json { None } else { Some(worktree_cut::cd_line(&cut.worktree).map_err(refusal)?) };

    let from = worktree_cut::start(&cut, base).map_err(refusal)?;

    aside(flags, format!(
        "✓ task {tid} has a worktree: {}  (branch {}, cut from {from})",
        cut.worktree.display(),
        cut.branch,
    ));
    aside(flags, "  code, build and test there — it is a development environment, not a bound folder");
    aside(flags, format!(
        "  task moves (comment / done) stay with Amenbo, run from {}",
        cut.root.display(),
    ));
    match cd {
        Some(line) => println!("{line}"),
        None => print_json(&json!({ "path": cut.worktree, "branch": cut.branch })),
    }
    Ok(())
}

fn finish(store: &Store, flags: &Flags, id: &str, base: Option<&str>, force: bool) -> Result<(), CliError> {
    let tid = resolve_task(store, id).map_err(CliError::from)?;
    let cut = cut_for(tid)?;
    let into = worktree_cut::finish(&cut, base, force).map_err(refusal)?;

    if flags.json {
        print_json(&json!({ "path": cut.worktree, "branch": cut.branch, "base": into }));
    } else {
        crate::output::human(flags, format!("✓ task {tid} folded up (worktree and branch {} removed)", cut.branch));
        let cmd = Paths::command_name();
        crate::output::human(flags, format!(
            "  the task is untouched — close it with `{cmd} task done {tid}`, or hand it back with `{cmd} task status {tid} todo`",
        ));
    }
    Ok(())
}

/// Refuse a `start` typed in a repository the task is not worked in (`AMB-D-649`).
///
/// A worktree takes its whole identity from where the command was typed: the placement is derived from
/// the current directory and never from the task, so the same `start` typed in the wrong repository
/// succeeds and hands back a checkout of a different project. A project whose work is spread over
/// several repositories is where that happens, and it is also where Amenbo holds the answer — a task
/// can name the folder it is worked in (`AMB-D-648`).
///
/// Everything short of a plain contradiction passes through. A task naming no folder claims nothing,
/// and a folder that lies in no repository leaves nothing to compare: being unable to cut a worktree is
/// the worse failure of the two, so only a folder naming another repository outright is refused. The
/// fold is held to none of this — it takes away a worktree that is already there, and where that stands
/// is not in question.
fn elsewhere(store: &Store, flags: &Flags, tid: i64, cut: &Cut) -> Result<(), CliError> {
    let Ok(detail) = store.task_detail(tid) else { return Ok(()) };
    let Some(at) = detail.at else { return Ok(()) };
    let home = match worktree_cut::git_root(Path::new(&at.dir)) {
        Ok(home) => home,
        Err(_) => {
            aside(flags, format!(
                "worktree: cutting here — task {tid} is worked in {}, which is in no repository",
                at.dir,
            ));
            return Ok(());
        }
    };
    // Both sides are git's own answer for their directory, so one repository reached by two spellings —
    // a symlink, a path leading through a linked worktree — resolves to one string.
    if home == cut.root {
        return Ok(());
    }
    let cmd = Paths::command_name();
    Err(CliError {
        code: CliErrorCode::WorktreeElsewhere.as_str(),
        message: format!(
            "Task {tid} is worked in {}, and this is {}. A worktree is cut from the repository the command is typed in, never from the task, so one cut here would be a checkout of the wrong project.",
            at.dir,
            cut.root.display(),
        ),
        hint: Some(format!("Start it where the task is:\n  cd {}\n  eval \"$({cmd} worktree start {tid})\"", at.dir)),
        exit: 1,
    })
}

/// Every refusal the git half can raise, as the sentence and the code this face answers with. The match
/// is exhaustive, so a refusal added there cannot reach a caller unnamed.
fn refusal(r: Refusal) -> CliError {
    let cmd = Paths::command_name();
    let (code, message, hint) = match r {
        Refusal::NoGit => (
            CliErrorCode::WorktreeNoGit,
            "This machine has no git Amenbo can run, so there is no worktree to cut.".to_string(),
            Some("Install git — on a Mac that is the Command Line Tools (`xcode-select --install`) or a git of your own — and run this again.".to_string()),
        ),
        Refusal::NotARepository(said) => (
            CliErrorCode::WorktreeNotARepository,
            format!("This folder is in no git repository, so there is nothing to cut a worktree from. git said: {said}"),
            Some("Run this in the repository the task is worked in.".to_string()),
        ),
        Refusal::WorktreeExists(path) => (
            CliErrorCode::WorktreeExists,
            format!("{} already exists.", path.display()),
            Some(format!(
                "ANOTHER SESSION MAY BE WORKING THERE. Do not look inside it, do not judge whether it is stale, do not remove it. Ask Amenbo first (`{cmd} task show <id>`) — in_progress means someone is on it, and the answer is to take a different task. Only if you know the worktree is your own leftover: `{cmd} worktree finish <id>`."
            )),
        ),
        Refusal::BranchExists(branch) => (
            CliErrorCode::WorktreeBranchExists,
            format!("Branch {branch} already exists, with no worktree checked out on it."),
            Some(format!("Fold the task's worktree (`{cmd} worktree finish <id>`), or delete the branch, before starting again.")),
        ),
        Refusal::NoWorktree(path) => (
            CliErrorCode::WorktreeMissing,
            format!("There is no worktree to fold ({} is missing).", path.display()),
            None,
        ),
        Refusal::Dirty(path) => (
            CliErrorCode::WorktreeDirty,
            format!("The worktree {} carries changes nobody has committed.", path.display()),
            Some("Commit them, or pass --force to discard them.".to_string()),
        ),
        Refusal::Unmerged { branch, base } => (
            CliErrorCode::WorktreeUnmerged,
            format!("Branch {branch} carries changes {base} does not have."),
            Some(format!("Merge it, or pass --force to discard them. A change that landed differently than it left — a conflict resolved by hand — reads as unmerged here: check with `git diff {base}...{branch}` before forcing.")),
        ),
        Refusal::Unquotable(path) => (
            CliErrorCode::WorktreeUnquotablePath,
            format!("The path {} contains a single quote, which cannot be quoted in a way both a POSIX shell and PowerShell read alike.", path.display()),
            Some("Move the repository somewhere without one, and the `cd` line works in either. `--json` hands the path back as a value and needs no quoting at all.".to_string()),
        ),
        Refusal::Git(said) => (CliErrorCode::WorktreeGit, format!("git refused: {said}"), None),
        Refusal::Io(said) => (CliErrorCode::WorktreeIo, format!("The filesystem refused: {said}"), None),
    };
    CliError { code: code.as_str(), message, hint, exit: 1 }
}
