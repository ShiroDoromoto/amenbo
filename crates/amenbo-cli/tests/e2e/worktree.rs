//! `worktree start` / `worktree finish`, end to end (`AMB-D-881`): where a task's checkout is cut, the
//! one line `start` returns, and every refusal that keeps a cut — or a fold — from happening.
//!
//! Driven as a process against a real repository, because git is the whole of what these two move.

mod harness;

use std::path::{Path, PathBuf};

use serde_json::Value;

use harness::*;

/// Run git, and on failure say what git objected to — the exit code and **both** streams. A quiet
/// commit writes "nothing to commit" to stdout, so stderr alone can leave a failure unreadable.
fn git(dir: &Path, args: &[&str]) {
    let out = amenbo_scratch::command("git").current_dir(dir).args(args).output().expect("failed to run git");
    assert!(
        out.status.success(),
        "git {args:?} exited {code}\nstdout: {stdout}\nstderr: {stderr}",
        code = out.status.code().map_or_else(|| "by signal".to_string(), |c| c.to_string()),
        stdout = String::from_utf8_lossy(&out.stdout),
        stderr = String::from_utf8_lossy(&out.stderr),
    );
}

/// git's own answer, for the assertions that ask the repository rather than the filesystem.
fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = amenbo_scratch::command("git").current_dir(dir).args(args).output().expect("failed to run git");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Record the working tree as one revision on the branch it is standing on.
fn record(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", message]);
}

/// A repository with one revision behind it, at a fresh path. The branch is named outright so the test
/// does not depend on what this machine's git calls a first branch.
fn a_repo(tag: &str) -> PathBuf {
    let dir = amenbo_scratch::scratch(tag);
    git(&dir, &["init", "-q", "-b", "main", "."]);
    git(&dir, &["config", "user.email", "alice@example.com"]);
    git(&dir, &["config", "user.name", "Alice"]);
    std::fs::write(dir.join("a.txt"), "base\n").unwrap();
    record(&dir, "base");
    dir
}

/// Run the binary in `cwd` and hand back (stdout, stderr, exit code). The worktree commands are about
/// the folder they are typed in, so every call names one — which the harness's own runners do not.
fn run_in(cli: &Cli, cwd: &Path, args: &[&str]) -> (String, String, i32) {
    let out = amenbo_scratch::command(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(cwd)
        .args(args)
        .arg("--actor")
        .arg("human")
        .output()
        .expect("failed to run the binary");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        exit_code(&out),
    )
}

/// The refusal a `--json` run reports. It is written to **stderr**, like every other command's, and it
/// is not the only thing there — an advisory can stand ahead of it — so the envelope is taken from the
/// first brace on.
fn refusal(stderr: &str) -> Value {
    let at = stderr.find('{').unwrap_or_else(|| panic!("no refusal envelope on stderr:\n{stderr}"));
    serde_json::from_str(&stderr[at..]).unwrap_or_else(|e| panic!("not JSON: {e}\n{stderr}"))
}

/// A project whose linked folder is `repo`, with one task ready to be worked in it. The id comes back.
fn a_task_in(cli: &Cli, repo: &Path) -> String {
    let dir = repo.to_string_lossy().into_owned();
    let p = cli.json(&["project", "add", "--name", "P", "--dir", &dir, "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "作業", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    cli.finish_creating(&tid);
    tid
}

/// Where a task's worktree is cut, by the layout nobody is asked about: beside the repository, never
/// inside it.
fn worktree_of(repo: &Path, tid: &str) -> PathBuf {
    // git answers with the resolved path, and on macOS the scratch directory is reached through a
    // symlink (/tmp → /private/tmp) — so the test levels its own spelling rather than the command's.
    let repo = std::fs::canonicalize(repo).expect("the repository is on disk");
    let name = repo.file_name().unwrap().to_string_lossy().into_owned();
    repo.parent().unwrap().join(format!("{name}-worktrees")).join(tid)
}

/// The whole round trip: cut, work, merge, fold. `start`'s stdout is the one `cd` line and nothing
/// else — the account of what happened is on stderr beside it — and `finish` leaves neither the
/// checkout nor the branch behind.
#[test]
fn a_task_is_cut_a_worktree_worked_in_and_folded_up_again() {
    let cli = Cli::new();
    let repo = a_repo("wt-round-trip");
    let tid = a_task_in(&cli, &repo);
    let wt = worktree_of(&repo, &tid);

    let (out, err, code) = run_in(&cli, &repo, &["worktree", "start", &tid]);
    assert_eq!(code, 0, "start failed: {err}");
    assert_eq!(out, format!("cd '{}'\n", wt.display()), "stdout is the way in and nothing else");
    assert!(err.contains(&format!("branch task/{tid}")), "the account names the branch: {err}");
    assert!(err.contains("cut from main"), "and what it was cut from: {err}");
    assert!(wt.is_dir(), "the checkout is on disk");
    assert_eq!(git_out(&wt, &["rev-parse", "--abbrev-ref", "HEAD"]), format!("task/{tid}"));
    assert!(!wt.starts_with(&repo), "it is cut beside the project folder, never inside it");

    // Work landed on the branch and merged into main: nothing is left to lose, so the fold goes through.
    std::fs::write(wt.join("a.txt"), "worked\n").unwrap();
    record(&wt, "work");
    git(&repo, &["merge", "-q", "--no-ff", "-m", "merged", &format!("task/{tid}")]);

    let (out, err, code) = run_in(&cli, &repo, &["worktree", "finish", &tid]);
    assert_eq!(code, 0, "finish failed: {err}");
    assert!(out.contains("folded up"), "the account is on stdout, there being no value to return: {out}");
    assert!(!wt.exists(), "the checkout is gone");
    assert_eq!(git_out(&repo, &["branch", "--list", &format!("task/{tid}")]), "", "and so is the branch");
}

/// A worktree already standing is not the caller's to enter, and the refusal says so rather than
/// guessing whose it is. The second `start` creates nothing.
#[test]
fn a_second_start_is_refused_and_points_at_the_backlog() {
    let cli = Cli::new();
    let repo = a_repo("wt-second-start");
    let tid = a_task_in(&cli, &repo);

    let (_, err, code) = run_in(&cli, &repo, &["worktree", "start", &tid]);
    assert_eq!(code, 0, "{err}");

    let (_, err, code) = run_in(&cli, &repo, &["worktree", "start", &tid, "--json"]);
    assert_eq!(code, 1);
    let refused = refusal(&err);
    assert_eq!(refused["error"]["code"], "worktree_exists");
    assert!(
        refused["error"]["hint"].as_str().unwrap().contains("ANOTHER SESSION MAY BE WORKING THERE"),
        "the hint names what it may be: {refused}",
    );
}

/// The fold refuses while there is anything left to lose, and each refusal is its own code: work
/// nobody recorded, then revisions `main` does not have. `--force` is what discards them on purpose.
#[test]
fn folding_refuses_over_uncommitted_work_and_over_unmerged_revisions() {
    let cli = Cli::new();
    let repo = a_repo("wt-refuse-fold");
    let tid = a_task_in(&cli, &repo);
    let wt = worktree_of(&repo, &tid);
    run_in(&cli, &repo, &["worktree", "start", &tid]);

    std::fs::write(wt.join("a.txt"), "half-done\n").unwrap();
    let (_, err, code) = run_in(&cli, &repo, &["worktree", "finish", &tid, "--json"]);
    assert_eq!(code, 1);
    assert_eq!(refusal(&err)["error"]["code"], "worktree_dirty");
    assert!(wt.is_dir(), "nothing was torn down");

    // Recorded, and still nowhere near main: the other half of the guard.
    record(&wt, "work");
    let (_, err, code) = run_in(&cli, &repo, &["worktree", "finish", &tid, "--json"]);
    assert_eq!(code, 1);
    assert_eq!(refusal(&err)["error"]["code"], "worktree_unmerged");

    let (_, err, code) = run_in(&cli, &repo, &["worktree", "finish", &tid, "--force"]);
    assert_eq!(code, 0, "--force discards both: {err}");
    assert!(!wt.exists());
}

/// A squash merge rewrites the branch's revisions under one of main's own, so lineage would call the
/// branch unmerged for ever — leaving `--force`, which also discards uncommitted work, as the only way
/// to fold a finished task down. The patch each revision carries is what is measured instead
/// (`AMB-D-699`), and it reads a squash as what it is.
#[test]
fn a_squash_merge_reads_as_merged() {
    let cli = Cli::new();
    let repo = a_repo("wt-squash");
    let tid = a_task_in(&cli, &repo);
    let wt = worktree_of(&repo, &tid);
    run_in(&cli, &repo, &["worktree", "start", &tid]);

    std::fs::write(wt.join("a.txt"), "worked\n").unwrap();
    record(&wt, "work");
    git(&repo, &["merge", "-q", "--squash", &format!("task/{tid}")]);
    // Only what the squash staged: the repository also holds the untracked files linking it to a
    // project left behind, and sweeping those in would change the patch being measured.
    git(&repo, &["commit", "-qm", "squashed"]);
    // Lineage says no: the branch's own revision is on no ancestor path of main.
    assert!(
        !amenbo_scratch::command("git")
            .current_dir(&repo)
            .args(["merge-base", "--is-ancestor", &format!("task/{tid}"), "main"])
            .status()
            .unwrap()
            .success(),
        "the premise of this test is that lineage calls a squash unmerged",
    );

    let (_, err, code) = run_in(&cli, &repo, &["worktree", "finish", &tid]);
    assert_eq!(code, 0, "the patch is what is measured, so no --force is needed: {err}");
    assert!(!wt.exists());
}

/// A worktree is cut from the repository the command is typed in, never from the task — so a `start`
/// typed in the wrong one would hand back a checkout of a different project. Where the task names the
/// folder it is worked in, that contradiction is refused, with the repository to type it in
/// (`AMB-D-649`).
#[test]
fn a_start_typed_in_another_repository_is_refused() {
    let cli = Cli::new();
    let home_repo = a_repo("wt-elsewhere-home");
    let other = a_repo("wt-elsewhere-other");
    let tid = a_task_in(&cli, &home_repo);

    // A second folder of the same project, and the task is worked in the first.
    let other_dir = other.to_string_lossy().into_owned();
    let (_, err, code) = run_in(&cli, &other, &["bind", "--project", "P", "--dir", &other_dir]);
    assert_eq!(code, 0, "the second folder is linked: {err}");
    cli.json(&["task", "update", &tid, "--at", &home_repo.to_string_lossy(), "--json"]);

    let (_, err, code) = run_in(&cli, &other, &["worktree", "start", &tid, "--json"]);
    assert_eq!(code, 1);
    assert_eq!(refusal(&err)["error"]["code"], "worktree_elsewhere");
    assert!(!worktree_of(&other, &tid).exists(), "nothing was cut in the wrong repository");

    // And in the repository it names, the same call goes through.
    let (_, err, code) = run_in(&cli, &home_repo, &["worktree", "start", &tid]);
    assert_eq!(code, 0, "{err}");
    assert!(worktree_of(&home_repo, &tid).is_dir());
}

/// Typed outside any repository there is nothing to cut from, and the refusal says that rather than
/// git's exit code.
#[test]
fn a_start_outside_a_repository_is_refused() {
    let cli = Cli::new();
    let plain = amenbo_scratch::scratch("wt-not-a-repo");
    let dir = plain.to_string_lossy().into_owned();
    let p = cli.json(&["project", "add", "--name", "P", "--dir", &dir, "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "作業", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    cli.finish_creating(&tid);

    let (_, err, code) = run_in(&cli, &plain, &["worktree", "start", &tid, "--json"]);
    assert_eq!(code, 1);
    assert_eq!(refusal(&err)["error"]["code"], "worktree_not_a_repository");
}

/// `--json` answers with the path and the branch instead of the `cd` line, for a caller that is not a
/// shell.
#[test]
fn json_answers_with_the_path_and_the_branch() {
    let cli = Cli::new();
    let repo = a_repo("wt-json");
    let tid = a_task_in(&cli, &repo);

    let (out, _, code) = run_in(&cli, &repo, &["worktree", "start", &tid, "--json"]);
    assert_eq!(code, 0, "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["path"].as_str().unwrap(), worktree_of(&repo, &tid).to_string_lossy());
    assert_eq!(v["branch"], format!("task/{tid}"));
}
