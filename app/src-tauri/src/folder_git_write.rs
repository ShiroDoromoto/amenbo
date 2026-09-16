//! What the Git panel does to the folder it is showing: staging, committing, stashing, branches,
//! merging, and the three that go out to the network.
//!
//! It is the other half of [`crate::folder_git`], and the line between them is the answer a road
//! gives when it comes to nothing. A read answers with an empty hand — a folder that is no
//! repository is the ordinary case there. **A write answers with git's own refusal, word for
//! word** (`AMB-D-906`): not rewritten, not said again in Amenbo's words, and not given a code that
//! would owe a template in nineteen languages, because a template is a rewriting. What reaches the
//! reader is the sentence git wrote, which is also the sentence they would get in a terminal.
//!
//! **The agent in the pane is not guarded against** (`AMB-D-906`). Nothing here asks whether
//! something is running beside it, refuses on that ground, or puts a question in the way. What is
//! done instead is to close the four holes that make writing from here unsafe *in shape* rather
//! than in timing — measured on all three systems by `AMB-T-4901` and `AMB-T-4900`:
//!
//! | The hole | What closes it |
//! |---|---|
//! | Committing takes in whatever the pane happened to have staged — all three systems, every time | The pathspec every commit carries (`pathspecs`); 0 of 3,855 took the wrong thing with it |
//! | `index.lock` is held by the pane, and git neither waits nor tries again | Twenty tries, 50ms apart (`run`); macOS 32/32, Linux 310/310, Windows 25/26 |
//! | `ssh-keygen` / `ssh-add` read the reader's terminal — and there is none | The child's stdin is closed (`run_once`) |
//! | A call that needs a password has no terminal to ask in | The askpass helper shipped beside the app, which puts the question to the window instead (`crate::folder_git_askpass`) |
//!
//! **The last row is `AMB-D-913`, and it replaced what `AMB-D-906` had put there.** That was
//! `GIT_TERMINAL_PROMPT=0` and `ssh -o BatchMode=yes`, which turned `Device not configured` into
//! `terminal prompts disabled` — a failure a reader can at least read, with the road out of it
//! written nowhere. Every one of the six products measured in `AMB-T-4968` puts the question to the
//! person instead.
//!
//! **The two did not weigh the same, and only one of them had to go.** Measured on git 2.55.0:
//!
//! | | with an askpass set |
//! |---|---|
//! | `ssh -o BatchMode=yes` | ssh asks nothing at all, the helper included — so this had to go, or the helper would never be run |
//! | `GIT_TERMINAL_PROMPT=0` | git runs the helper anyway: it is consulted first, and this only words the fallback taken when the helper answers nothing |
//!
//! So what dropping the second one costs is a sentence: where nobody answers, the reader is back to
//! `Device not configured` rather than `terminal prompts disabled`. `AMB-D-913` drops both, and the
//! case it is worded for — nobody there to answer — is the one the dialog exists to end.
//!
//! **`--no-optional-locks` is not passed here**, though every read next door carries it. What it
//! turns off is the lock git takes for its own convenience while reading, and a write's lock is the
//! one it cannot do without.
//!
//! **Off the main thread, like the reads.** A command with no `async` on it runs where the webview
//! is drawn ([`crate::agent_models`]), and one call out to the network is 0.6 to 0.9 seconds
//! (`AMB-T-4900`) — the window would stand still for the whole of it.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use crate::error::CmdError;
use crate::folder_fence::{gone, names, root_of};
use crate::folder_git::off_thread;

/// How many times a call held off by `index.lock` is made again before the refusal is the answer.
///
/// Twenty at 50ms is a second of trying, which is what the pane's own `git add` was measured to
/// need: every call got through on macOS and Linux, and all but one of 26 on Windows
/// (`AMB-T-4901`). git has nothing of its own here — it does not wait for the lock and does not
/// queue behind whoever holds it — so this is the caller's to hold.
const LOCK_TRIES: u32 = 20;

/// The wait between two of [`LOCK_TRIES`].
const LOCK_WAIT: Duration = Duration::from_millis(50);

/// The name of the lock, as it appears in what git says about being unable to take it.
///
/// git writes the path of the file it could not make (`Unable to create '…/.git/index.lock': File
/// exists.`), so the name is in the sentence whatever language the sentence is in — which is what
/// makes this readable off git's own words rather than off an exit status shared by everything else
/// that goes wrong.
const LOCK: &str = "index.lock";

// ── staging, and what is written down ────────────────────────────────────────────────────────

/// Put `paths` into the index — `git add`.
#[tauri::command]
pub async fn folder_git_stage(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
) -> Result<String, CmdError> {
    asked(project_id, root, paths, |specs| {
        let mut args = vec!["add".into(), "--".into()];
        args.extend(specs);
        args
    })
    .await
}

/// Take `paths` back out of the index, leaving the working tree alone — `git reset`.
///
/// **`reset` and not `restore --staged`**, which is the spelling git's own advice now uses. In a
/// repository nobody has written a commit in yet there is no `HEAD` to restore from, and `restore`
/// says so and does nothing (`fatal: could not resolve 'HEAD'`, measured here); `reset` empties the
/// entry and is right in both repositories.
#[tauri::command]
pub async fn folder_git_unstage(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
) -> Result<String, CmdError> {
    asked(project_id, root, paths, |specs| {
        let mut args = vec!["reset".into(), "-q".into(), "--".into()];
        args.extend(specs);
        args
    })
    .await
}

/// Write `paths` down with `message` on them.
///
/// **The pathspec is the whole point of this road** (`AMB-D-906`, 3-2). Taking what the index holds
/// takes in whatever the agent in the pane had staged and then stopped in the middle of — on all
/// three systems, every single time (`AMB-T-4901`). Naming the paths made it 0 of 3,855, and what
/// the pane staged stays staged.
///
/// **`paths` empty is the other road**: what the index holds, whoever put it there. It is the
/// "everything" a reader asks for outright, and the screen that offers it is the one that owes them
/// the warning that the pane's work comes too (`AMB-D-906`, 3-2).
#[tauri::command]
pub async fn folder_git_commit(
    project_id: i64,
    root: String,
    message: String,
    paths: Vec<Vec<String>>,
) -> Result<String, CmdError> {
    asked(project_id, root, paths, move |specs| {
        let mut args = vec!["commit".into(), "-m".into(), message];
        if !specs.is_empty() {
            args.push("--".into());
            args.extend(specs);
        }
        args
    })
    .await
}

/// Throw away what the working tree has done to `paths` — `git checkout -- <paths>`.
///
/// **This is the one road here that cannot be walked back.** A commit, a stash and a branch switch
/// are all still in the reflog afterwards; changes never written down are nowhere once this
/// returns. Asking first is the face's (`AMB-D-777` settles how, in the same words a file going to
/// the wastebasket is asked about) — the host does what it is told.
#[tauri::command]
pub async fn folder_git_restore(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
) -> Result<String, CmdError> {
    asked(project_id, root, paths, |specs| {
        let mut args = vec!["checkout".into(), "--".into()];
        args.extend(specs);
        args
    })
    .await
}

/// Stop git following `paths`, leaving them on disk — `git rm --cached`.
///
/// It is a change to the index and nothing else, so writing it down puts it back: that is why the
/// face asks nothing before it, where [`folder_git_restore`] asks.
#[tauri::command]
pub async fn folder_git_untrack(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
) -> Result<String, CmdError> {
    asked(project_id, root, paths, |specs| {
        let mut args = vec!["rm".into(), "--cached".into(), "--".into()];
        args.extend(specs);
        args
    })
    .await
}

/// Write `paths` into the folder's own `.gitignore`, one line each.
///
/// **It runs no git at all**, and it lives here because of what it is for rather than what it does:
/// the item a reader presses stands beside the ones that do run git, and a road that answered
/// differently from its neighbours would be one they had to learn twice.
///
/// **The file the lines go in is the bound folder's own** — the one the window is on (`AMB-D-905`).
/// git reads an ignore file in every folder on the way down, so a line could be written nearer the
/// path it is about; what that buys is a rule the reader has to go looking for, in a file they did
/// not know was there.
///
/// **What is written is a path from that folder, led by `/`.** Without the slash git reads the line
/// as a name and ignores every file of that name anywhere below — so ignoring one `build/main.rs`
/// would quietly ignore all of them.
///
/// A path already on the list is left alone. Writing it twice would be a second rule saying what
/// the first says, and a reader who pressed the item on a file already ignored asked for the state
/// rather than for the line.
#[tauri::command]
pub async fn folder_git_ignore(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
) -> Result<String, CmdError> {
    off_thread(move || {
        let dir = root_of(project_id, &root)?;
        let lines: Vec<String> = paths
            .iter()
            .map(|path| Ok(format!("/{}", names(path).ok_or_else(gone)?.join("/"))))
            .collect::<Result<_, CmdError>>()?;
        let at = dir.join(".gitignore");
        // Nothing there is nothing to read, which is a file about to be made rather than a failure:
        // a repository need not have one, and pressing this is how the first one comes to exist.
        let had = std::fs::read_to_string(&at).unwrap_or_default();
        let Some(out) = ignoring(&had, &lines) else { return Ok(String::new()) };
        std::fs::write(&at, out).map_err(|e| -> CmdError {
            format!("writing .gitignore did not work: {e}").into()
        })?;
        Ok(String::new())
    })
    .await
}

// ── what is put aside ────────────────────────────────────────────────────────────────────────

/// Put `paths` aside — `git stash push`, with `message` written on it where there is one.
///
/// It carries a pathspec for the same reason writing one down does: without one it would take in
/// the pane's working tree along with the reader's.
#[tauri::command]
pub async fn folder_git_stash(
    project_id: i64,
    root: String,
    message: String,
    paths: Vec<Vec<String>>,
) -> Result<String, CmdError> {
    asked(project_id, root, paths, move |specs| {
        let mut args = vec!["stash".into(), "push".into()];
        if !message.is_empty() {
            args.extend(["-m".into(), message]);
        }
        if !specs.is_empty() {
            args.push("--".into());
            args.extend(specs);
        }
        args
    })
    .await
}

/// Take a stash back out and drop it — `git stash pop`.
///
/// `name` is one the list was just read with ([`crate::folder_git::folder_git_stashes`]), and it is
/// checked for being a word git reads as a stash rather than as an option, the way a commit's name
/// is next door.
#[tauri::command]
pub async fn folder_git_stash_pop(
    project_id: i64,
    root: String,
    name: String,
) -> Result<String, CmdError> {
    if !is_stash(&name) {
        return Err(not_a_name());
    }
    told(project_id, root, move || vec!["stash".into(), "pop".into(), name]).await
}

// ── the branch ───────────────────────────────────────────────────────────────────────────────

/// Move onto `branch` — `git checkout`.
///
/// **git's refusal is the answer here more often than the move is.** Editing a file the other
/// branch also changes makes this exit 1 with its own three-line account of which files stand in
/// the way (`AMB-T-4901` measured it on all three systems), and that account is what the reader is
/// shown: it is git's answer about their working tree, not a rule Amenbo laid down (`AMB-D-906`,
/// 3-4).
#[tauri::command]
pub async fn folder_git_switch(
    project_id: i64,
    root: String,
    branch: String,
) -> Result<String, CmdError> {
    if !is_name(&branch) {
        return Err(not_a_name());
    }
    told(project_id, root, move || vec!["checkout".into(), branch]).await
}

/// Make `name` where the reader is standing and move onto it — `git checkout -b`.
///
/// Whether the name is one git will have is git's to say, and it says so in its own words: the
/// check here is only for a word git would read as an option instead of as a name.
#[tauri::command]
pub async fn folder_git_branch_create(
    project_id: i64,
    root: String,
    name: String,
) -> Result<String, CmdError> {
    if !is_name(&name) {
        return Err(not_a_name());
    }
    told(project_id, root, move || vec!["checkout".into(), "-b".into(), name]).await
}

/// Bring `branch` into the one the reader is on — `git merge`.
#[tauri::command]
pub async fn folder_git_merge(
    project_id: i64,
    root: String,
    branch: String,
) -> Result<String, CmdError> {
    if !is_name(&branch) {
        return Err(not_a_name());
    }
    told(project_id, root, move || vec!["merge".into(), branch]).await
}

/// Finish a merge whose conflicts have been settled — `git merge --continue`.
#[tauri::command]
pub async fn folder_git_merge_continue(project_id: i64, root: String) -> Result<String, CmdError> {
    told(project_id, root, || vec!["merge".into(), "--continue".into()]).await
}

/// Put the tree back where it stood before the merge — `git merge --abort`.
#[tauri::command]
pub async fn folder_git_merge_abort(project_id: i64, root: String) -> Result<String, CmdError> {
    told(project_id, root, || vec!["merge".into(), "--abort".into()]).await
}

// ── out to the network ───────────────────────────────────────────────────────────────────────

/// Read the remote — `git fetch`.
///
/// **The window makes the call itself** rather than writing it into a pane for the agent to run
/// (`AMB-D-906`, 3-5). `SSH_AUTH_SOCK` is handed to every process launchd starts, so a window
/// opened from the Dock has the same agent a terminal does; HTTPS goes through the credential
/// helper without a keychain prompt; and neither git nor ssh can stop to ask for a passphrase,
/// because with no terminal they give up inside half a second (`AMB-T-4900`).
///
/// ⚠ **A repository using LFS falls over here and says something about `git-lfs`.** A window's
/// `PATH` is `/usr/bin:/bin:/usr/sbin:/sbin` and nothing else, and `filter.lfs.required` makes that
/// fatal (`AMB-T-4900`). What git says about it is what the reader gets, the same as any other
/// refusal.
#[tauri::command]
pub async fn folder_git_fetch(project_id: i64, root: String) -> Result<String, CmdError> {
    told(project_id, root, || vec!["fetch".into()]).await
}

/// Read the remote and bring it in — `git pull`, in whatever shape the reader's own config gives it
/// (merge or rebase; nothing here decides that for them).
#[tauri::command]
pub async fn folder_git_pull(project_id: i64, root: String) -> Result<String, CmdError> {
    told(project_id, root, || vec!["pull".into()]).await
}

/// Send the branch — `git push`.
///
/// A branch with no upstream is refused by git with its own sentence, the one naming
/// `--set-upstream`. That sentence is the answer: it tells the reader exactly what a terminal would.
#[tauri::command]
pub async fn folder_git_push(project_id: i64, root: String) -> Result<String, CmdError> {
    told(project_id, root, || vec!["push".into()]).await
}

// ── the roads under all of them ──────────────────────────────────────────────────────────────

/// Run in the folder `root` names what `build` makes of the pathspecs `paths` come to.
///
/// The paths are turned into pathspecs before git is started, so one this project may not name is
/// refused without a process being made for it.
async fn asked<F>(
    project_id: i64,
    root: String,
    paths: Vec<Vec<String>>,
    build: F,
) -> Result<String, CmdError>
where
    F: FnOnce(Vec<String>) -> Vec<String> + Send + 'static,
{
    off_thread(move || {
        let dir = root_of(project_id, &root)?;
        let args = build(pathspecs(&paths)?);
        run(&dir, &args.iter().map(String::as_str).collect::<Vec<_>>())
    })
    .await
}

/// The same road for one that names no paths: there are none to turn into pathspecs, so `build` is
/// handed nothing.
async fn told<F>(project_id: i64, root: String, build: F) -> Result<String, CmdError>
where
    F: FnOnce() -> Vec<String> + Send + 'static,
{
    asked(project_id, root, Vec::new(), |_| build()).await
}

/// What the ignore file says once `lines` are on it, or nothing where every one of them already is.
///
/// A line already there is left where it is: writing it twice would be a second rule saying what
/// the first says, and a reader who pressed the item on a path already ignored asked for the state
/// rather than for the line.
///
/// The newline in front of what is added is the one the file may be missing. A last line somebody
/// wrote without one would otherwise have the first of these run on to the end of it, and the two
/// would be one rule that matches neither path.
fn ignoring(had: &str, lines: &[String]) -> Option<String> {
    let already: Vec<&str> = had.lines().map(str::trim_end).collect();
    let adding: Vec<&String> = lines.iter().filter(|one| !already.contains(&one.as_str())).collect();
    if adding.is_empty() {
        return None;
    }
    let mut out = had.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for one in adding {
        out.push_str(one);
        out.push('\n');
    }
    Some(out)
}

/// The pathspecs `paths` come to, in the spelling git is run in — one ordinary name per segment,
/// and the whole of it read as a path rather than as a pattern.
///
/// **`:(literal)` is not decoration.** A pathspec is a pattern, so a file called `a[1].txt` also
/// matches `a1.txt`: staging the one row a reader pointed at stages the other file too, which was
/// measured here against a real git. The magic word turns the pattern off for that path alone.
///
/// **A path of no segments is the bound folder itself**, which is the row git writes for a folder
/// it answers for whole rather than naming what is inside it ([`crate::folder_git`]). `:(literal)`
/// with nothing after it is how git spells that same folder.
///
/// **The fence is [`names`] and not [`crate::folder_fence::under`].** What is built here is handed
/// to git, which matches it inside the repository — this process never opens it — so the folders
/// above it are not walked against the filesystem, and must not be: a path being staged is very
/// often one that is not there any more, and a deleted folder would turn away the staging of its
/// own deletion.
fn pathspecs(paths: &[Vec<String>]) -> Result<Vec<String>, CmdError> {
    paths
        .iter()
        .map(|path| Ok(format!(":(literal){}", names(path).ok_or_else(gone)?.join("/"))))
        .collect()
}

/// Run one git that writes, and hand back what it said — or what it said in refusing.
///
/// This is where the conditions in the module doc-comment are kept, so that no road above can be
/// written without them.
fn run(dir: &Path, args: &[&str]) -> Result<String, CmdError> {
    for _ in 1..LOCK_TRIES {
        match run_once(dir, args)? {
            Ok(said) => return Ok(said),
            Err(said) if said.contains(LOCK) => std::thread::sleep(LOCK_WAIT),
            Err(said) => return Err(said.into()),
        }
    }
    // The last of the twenty is not followed by a wait: what would be waited for is a try that is
    // not coming, and a second of holding the reader is long enough to have told them instead.
    run_once(dir, args)?.map_err(CmdError::from)
}

/// One run of git: `Ok` where it did what it was asked, `Err` where it refused — both being the
/// words git wrote, which is the whole of what this layer has to say (`AMB-D-906`, 3-4).
///
/// The outer failure is the other thing entirely: git not being on the machine, or not starting.
fn run_once(dir: &Path, args: &[&str]) -> Result<Result<String, String>, CmdError> {
    let mut git = amenbo_core::sys::git().ok_or_else(|| CmdError::from("git is not installed"))?;
    let out = git
        .arg("-C")
        .arg(dir)
        .args(args)
        // Closed, so that nothing git starts can go looking for the reader down it. `ssh-keygen`
        // and `ssh-add` read stdin when they find no terminal, and what they would find is the
        // window's own — which nobody is watching (`AMB-T-4900`).
        .stdin(Stdio::null())
        // Where the question goes instead of to a terminal: the helper shipped beside the app, and
        // the port it calls back on (`crate::folder_git_askpass`). `GIT_TERMINAL_PROMPT=0` and
        // `ssh -o BatchMode=yes` stood here until `AMB-D-913` — the module doc-comment has which of
        // the two would have stopped the helper being run at all. Nothing is set out of a build
        // tree, where there is no helper beside the binary to point at.
        .envs(crate::folder_git_askpass::env())
        // Nothing here has an editor to open, and a merge being finished asks for one. `true` takes
        // the message git already wrote, which is the one the editor would have opened on.
        .env("GIT_EDITOR", "true")
        .output()
        .map_err(|e| CmdError::from(format!("git could not be run: {e}")))?;
    let said = said(&out);
    Ok(match out.status.success() {
        true => Ok(said),
        false => Err(said),
    })
}

/// What git wrote, both streams together and in the order they were written.
///
/// **stderr comes first** because that is where git puts what it is doing — what it read from the
/// remote, and why it stopped — and stdout what it did afterwards. Reading only one of them would
/// mean deciding which a reader is shown, and neither answer is right for all of `push`, `pull` and
/// a commit.
fn said(out: &std::process::Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    let out = String::from_utf8_lossy(&out.stdout);
    [err.trim_end(), out.trim_end()]
        .into_iter()
        .filter(|said| !said.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether a word is one git reads as a name rather than as an option.
///
/// It is [`crate::folder_git`]'s `is_sha` for refs: everything else about a name — the characters
/// git will have in one, whether the branch is there at all — is git's own to answer, and it
/// answers in its own words. A word starting with `-` never gets that far, because git would have
/// read it as an option before looking at it.
fn is_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('-')
}

/// Whether a word is one git reads as a stash: `stash@{0}` and nothing else.
///
/// Every one of them came out of the list read a moment ago, so this turns away nothing a reader
/// could have pointed at.
fn is_stash(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("stash@{") else { return false };
    let Some(digits) = rest.strip_suffix('}') else { return false };
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// What a name git would have read as an option is answered with. It is the backstop under a face
/// that offers names rather than a sentence a reader is meant to meet, which is why it carries no
/// code of its own.
fn not_a_name() -> CmdError {
    CmdError::from("that is not a name git would read as a name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_spelled_so_that_git_reads_it_as_a_path_and_not_as_a_pattern() {
        let specs = pathspecs(&[vec!["src".into(), "a[1].txt".into()]]).unwrap();
        assert_eq!(specs, vec![":(literal)src/a[1].txt".to_string()]);
    }

    /// The row git writes for a folder it answers for whole. No segments is the bound folder, and
    /// `:(literal)` with nothing after it is git's own name for the folder it is run in.
    #[test]
    fn a_path_of_no_segments_is_the_folder_itself() {
        assert_eq!(pathspecs(&[vec![]]).unwrap(), vec![":(literal)".to_string()]);
    }

    /// Every way out of the folder, refused before a process is started. The pathspec is built from
    /// text alone, so this is the only thing standing between a caller and a path above the folder.
    #[test]
    fn nothing_that_climbs_out_of_the_folder_becomes_a_pathspec() {
        for path in [vec!["..".to_string()], vec!["a/b".to_string()], vec!["/etc".to_string()]] {
            assert!(pathspecs(std::slice::from_ref(&path)).is_err(), "{path:?}");
        }
    }

    // ── the ignore file ──────────────────────────────────────────────────────────────────────

    /// The line is led by a slash, which is what makes it a path from this folder rather than a
    /// name matched anywhere below it — and a repository need not have the file at all.
    #[test]
    fn a_first_line_makes_the_file_it_goes_in() {
        let out = ignoring("", &["/build/main.rs".to_string()]).unwrap();
        assert_eq!(out, "/build/main.rs\n");
    }

    /// A file somebody wrote without a last newline. Without the one put in front, the line added
    /// would run on to the end of theirs and the two would be one rule matching neither path.
    #[test]
    fn a_file_with_no_last_newline_gets_one_before_the_line() {
        let out = ignoring("target\n*.log", &["/note.md".to_string()]).unwrap();
        assert_eq!(out, "target\n*.log\n/note.md\n");
    }

    #[test]
    fn a_line_already_there_is_not_written_again() {
        assert!(ignoring("target\n/note.md\n", &["/note.md".to_string()]).is_none());
        // And the one that is not there still goes on, beside the one that was.
        let out = ignoring("/note.md\n", &["/note.md".to_string(), "/other.md".to_string()]).unwrap();
        assert_eq!(out, "/note.md\n/other.md\n");
    }

    /// The file's own last newline is what a line is judged against, not the spaces behind it: git
    /// reads a trailing space as part of the pattern only when it is escaped.
    #[test]
    fn a_line_is_matched_past_the_spaces_at_the_end_of_it() {
        assert!(ignoring("/note.md   \n", &["/note.md".to_string()]).is_none());
    }

    #[test]
    fn only_a_word_git_reads_as_a_name_is_asked_by() {
        assert!(is_name("main"));
        assert!(is_name("feature/日本語"));
        assert!(!is_name(""));
        assert!(!is_name("--all"));
    }

    #[test]
    fn only_a_word_git_reads_as_a_stash_is_asked_by() {
        assert!(is_stash("stash@{0}"));
        assert!(is_stash("stash@{12}"));
        assert!(!is_stash("stash@{}"));
        assert!(!is_stash("stash@{a}"));
        assert!(!is_stash("--all"));
        assert!(!is_stash("stash@{0"));
    }

    /// Both streams, in the order git wrote them, with nothing invented between them.
    #[test]
    fn what_git_said_is_both_streams_with_the_one_it_stopped_on_first() {
        let both = std::process::Output {
            status: Default::default(),
            stdout: b"Updating abc..def\n".to_vec(),
            stderr: b"From github.com:me/repo\n".to_vec(),
        };
        assert_eq!(said(&both), "From github.com:me/repo\nUpdating abc..def");

        let quiet = std::process::Output {
            status: Default::default(),
            stdout: Vec::new(),
            stderr: b"error: pathspec 'nope' did not match\n".to_vec(),
        };
        assert_eq!(said(&quiet), "error: pathspec 'nope' did not match");
    }

    /// The whole road against a real git: staging one path and not its neighbour, what is written
    /// down carrying only what it was pointed at although the index holds more, and the pane's own
    /// staging surviving it. None of that can be pinned by handing a parser bytes.
    #[test]
    fn what_is_written_down_is_what_it_was_pointed_at_and_the_rest_stays_staged() {
        if amenbo_core::sys::git().is_none() {
            return; // Nothing to pin on a machine with no git.
        }
        let repo = amenbo_scratch::scratch("app-foldergitwrite-paths");
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]).expect("git init");
        // Who wrote it is set on the repository rather than on the machine, so a git with no name
        // set still walks this road and nothing outside the scratch folder is touched.
        run(&repo, &["config", "user.name", "Alice"]).expect("a name");
        run(&repo, &["config", "user.email", "alice@example.com"]).expect("an address");

        std::fs::write(repo.join("mine.txt"), "mine\n").unwrap();
        std::fs::write(repo.join("theirs.txt"), "theirs\n").unwrap();
        // Both staged, which is the shape the agent in the pane leaves behind.
        run(&repo, &["add", "--", ":(literal)mine.txt", ":(literal)theirs.txt"]).expect("git add");

        run(&repo, &["commit", "-m", "mine", "--", ":(literal)mine.txt"]).expect("the record");

        let held = run(&repo, &["show", "--name-only", "--format=", "HEAD"]).expect("git show");
        assert_eq!(held.trim(), "mine.txt", "only the path it was pointed at is in it");
        let staged = run(&repo, &["diff", "--cached", "--name-only"]).expect("git diff --cached");
        assert_eq!(staged.trim(), "theirs.txt", "what the pane staged is still staged");
    }

    /// git refusing, carried back as the sentence git wrote. Nothing is put in front of it and
    /// nothing is taken off it (`AMB-D-906`, 3-4).
    #[test]
    fn a_refusal_is_the_sentence_git_wrote() {
        if amenbo_core::sys::git().is_none() {
            return;
        }
        let repo = amenbo_scratch::scratch("app-foldergitwrite-refusal");
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]).expect("git init");

        let refused =
            run(&repo, &["add", "--", ":(literal)nowhere.txt"]).expect_err("no such path");
        assert!(
            refused.message_en.contains("nowhere.txt"),
            "git's own words reach the reader: {}",
            refused.message_en
        );
        assert_eq!(refused.code, "error", "no code, because a code would owe a translation");
    }

    /// A lock somebody else is holding is waited out rather than reported. This is the hole
    /// `AMB-T-4901` measured the agent in the pane opening, and holding it open on purpose is what
    /// `AMB-D-906` chose over guarding against the pane.
    #[test]
    fn a_call_held_off_by_the_lock_is_made_again_until_the_lock_goes() {
        if amenbo_core::sys::git().is_none() {
            return;
        }
        let repo = amenbo_scratch::scratch("app-foldergitwrite-lock");
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]).expect("git init");
        std::fs::write(repo.join("one.txt"), "one\n").unwrap();

        let lock = repo.join(".git").join("index.lock");
        std::fs::write(&lock, "").unwrap();
        let holder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            std::fs::remove_file(&lock).unwrap();
        });

        run(&repo, &["add", "--", ":(literal)one.txt"]).expect("the lock went and the add landed");
        holder.join().unwrap();
    }
}
