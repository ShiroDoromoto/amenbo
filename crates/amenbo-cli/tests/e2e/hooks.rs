//! What amenbo puts around a repository: the lint that reads a staged diff or a commit message, the
//! git hook that runs it on every commit, and the paste an AI's session start is handed.

mod harness;

use std::process::Command;

use serde_json::Value;

use harness::*;

/// Run `lint` and return (stdout, stderr, exit code), optionally piping `stdin` in.
///
/// It takes no `Cli`, on purpose: `lint` must open no store, so these tests name none. `AMENBO_HOME`
/// points at a directory that does not exist and each test asserts it still does not afterwards — which
/// keeps the run hermetic (a regression that opened a store would create it there, never in the real
/// app-data tree) *and* is itself the evidence that no store was opened.
/// An `AMENBO_HOME` that does not exist — a name inside a scratch directory, one level down, which
/// nothing creates. [`lint`] asserts it is still missing afterwards, so it has to start out missing.
fn unopened_home() -> std::path::PathBuf {
    amenbo_scratch::scratch("lint-home").join("home")
}

fn lint(cwd: &std::path::Path, home: &std::path::Path, args: &[&str], stdin: Option<&str>) -> (String, String, i32) {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", home)
        // `lint` touches no facet, so it must run with none declared — and none is declared here.
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(cwd)
        .arg("lint")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run the binary");
    if let Some(text) = stdin {
        child.stdin.as_mut().unwrap().write_all(text.as_bytes()).unwrap();
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert!(!home.exists(), "lint opened a store: {}", home.display());
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        exit_code(&out),
    )
}

/// Run git, and on failure say what git actually objected to — the exit code and **both** streams.
///
/// stderr alone is not enough, and that is not a hypothetical: `git commit -q` writes "nothing to commit"
/// to stdout, so a commit that fails that way reports an empty message and a bare non-zero code, which is
/// exactly how the flake this helper now describes managed to stay unreadable.
fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git").current_dir(dir).args(args).output().expect("failed to run git");
    assert!(
        out.status.success(),
        "git {args:?} exited {code}\nstdout: {stdout}\nstderr: {stderr}",
        code = out.status.code().map_or_else(|| "by signal".to_string(), |c| c.to_string()),
        stdout = String::from_utf8_lossy(&out.stdout),
        stderr = String::from_utf8_lossy(&out.stderr),
    );
}

/// A repository with one commit behind it, at a fresh path.
///
/// Fresh is what [`amenbo_scratch::scratch`] hands back, and this function needs it to be: when a recycled pid once named
/// this repository, it already existed, already had `a.rs` at exactly this content, and already had the
/// `base` commit. `git add -A` then staged nothing and `git commit -qm base` exited non-zero saying
/// "nothing to commit" on stdout, which `-q` swallowed — a rare, silent, empty-stderr failure of an
/// unrelated test.
fn a_repo() -> std::path::PathBuf {
    let dir = amenbo_scratch::scratch("repo");
    git(&dir, &["init", "-q", "."]);
    // A commit needs an identity, and the machine's own must not decide a test's outcome.
    git(&dir, &["config", "user.email", "alice@example.com"]);
    git(&dir, &["config", "user.name", "Alice"]);
    std::fs::write(dir.join("a.rs"), "fn main() {}\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    dir
}

/// The default input: what the commit is about to record. The report names the new file at the line the
/// ref lands on, and the exit code is the verdict a hook and CI both read.
#[test]
fn lint_reads_the_staged_diff_by_default() {
    let repo = a_repo();
    let home = unopened_home();
    std::fs::write(repo.join("a.rs"), "fn main() {}\n// as decided in AMB-D-272\n").unwrap();
    git(&repo, &["add", "-A"]);

    let (out, _, code) = lint(&repo, &home, &["--json"], None);
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(code, 1, "a leak exits non-zero: {out}");
    assert_eq!(v["ok"], false);
    assert_eq!(v["count"], 1);
    assert_eq!(v["hits"][0]["path"], "a.rs");
    assert_eq!(v["hits"][0]["line"], 2);
    assert_eq!(v["hits"][0]["ref"], "AMB-D-272");

    // Committing it is not what clears the lint — what the commit *adds* is. With the leak gone from the
    // staged text, the same call is clean.
    std::fs::write(repo.join("a.rs"), "fn main() {}\n// as decided in the ref-namespace decision\n").unwrap();
    git(&repo, &["add", "-A"]);
    let (out, _, code) = lint(&repo, &home, &["--json"], None);
    assert_eq!(code, 0, "clean text exits zero: {out}");
    assert_eq!(serde_json::from_str::<Value>(&out).unwrap()["ok"], true);
}

/// The commit message, handed over the way git hands it to a `commit-msg` hook: as a file.
#[test]
fn lint_reads_a_commit_message_file() {
    let repo = a_repo();
    let home = unopened_home();
    let msg = repo.join("COMMIT_EDITMSG");
    std::fs::write(&msg, "feat(lint): add it\n\ncloses AMB-T-1655\n").unwrap();

    let (out, _, code) = lint(&repo, &home, &[msg.to_str().unwrap()], None);
    assert_eq!(code, 1, "a ref in the message exits non-zero: {out}");
    assert!(out.contains("COMMIT_EDITMSG:3: AMB-T-1655"), "reported at path:line: {out}");
}

/// Piped text, and the boundary the namespace buys: a bare `#12` is a GitHub issue and a `T-45` may be
/// another tracker's, so neither is ours to flag.
#[test]
fn lint_reads_stdin_and_leaves_foreign_refs_alone() {
    let repo = a_repo();
    let home = unopened_home();

    let (out, _, code) = lint(&repo, &home, &["--stdin"], Some("fixes #12, part of PROJ-9 and T-45\n"));
    assert_eq!(code, 0, "no amenbo ref, so nothing to report: {out}");

    let (out, _, code) = lint(&repo, &home, &["--stdin", "--json"], Some("see AMB-T-1\n"));
    assert_eq!(code, 1);
    assert_eq!(serde_json::from_str::<Value>(&out).unwrap()["hits"][0]["ref"], "AMB-T-1");
}

/// Outside a repository there is no staged diff — said in one line, with what to do instead. (git answers
/// that case with its whole usage text, which is no use to anyone.)
#[test]
fn lint_outside_a_repository_says_so_plainly() {
    let dir = amenbo_scratch::scratch("plain");
    let home = unopened_home();

    let (_, err, code) = lint(&dir, &home, &["--json"], None);
    assert_ne!(code, 0);
    assert!(err.contains("Not a git repository"), "says what is wrong: {err}");
    assert!(err.lines().count() < 12, "and does not reprint git's manual: {err}");

    // The text faces need no repository at all: they read what they are handed.
    let (_, _, code) = lint(&dir, &home, &["--stdin"], Some("clean\n"));
    assert_eq!(code, 0, "lint needs no repository to read piped text");
}

/// `agent-hook snippet` hands over a request for the reader's own AI, and stdout carries nothing but it:
/// the text is meant to reach that AI through a pipe or a clipboard, and one courtesy line landing in with
/// it would read as part of what is being asked for. amenbo's own voice — where the text is going, and that
/// amenbo wired nothing itself — is on stderr (`AMB-D-440`).
#[test]
fn agent_hook_snippet_gives_stdout_to_the_request_and_says_where_it_goes_on_stderr() {
    let cli = Cli::new();

    let (out, err, code) = cli.run_both(&["agent-hook", "snippet", "claude-code"]);
    assert_eq!(code, 0, "{err}");
    // A request and not a settings file: what it asks for is a merge into one, which is the whole reason
    // it is prose. Handed to a provider as-is it would not parse, and nothing should read as though it could.
    assert!(
        serde_json::from_str::<Value>(&out).is_err(),
        "stdout reads as a settings file rather than a request: {out}"
    );
    assert!(out.contains("Merge"), "the request does not ask for a merge: {out}");
    assert!(out.contains(".claude/settings.json"), "the request names no file: {out}");
    assert!(out.contains("\"SessionStart\""), "the request carries no configuration: {out}");
    assert!(out.contains(" agent --json"), "the request does not launch the entry point: {out}");
    assert!(err.contains(".claude/settings.json"), "stderr does not name the file it edits: {err}");
    assert!(
        err.contains("amenbo writes nothing"),
        "stderr does not say the writing is not amenbo's: {err}"
    );

    // A tool nobody lists is refused where the argument is read, naming what it takes — so the answer to
    // "which ones are there" is the refusal itself, and no branch further in has to hold a second list.
    let (err, code) = cli.run_err(&["agent-hook", "snippet", "my-editor"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("claude-code") && err.contains("gemini-cli"), "the refusal lists no tools: {err}");
}

/// A folder whose AI is not started on amenbo says so on every response until it is — and under `--json`
/// it says so in a field, which is the one surface an AI is sure to read (`AMB-D-440`). The provider it
/// names is the one the folder shows a trace of, and the fix is the command that prints the text.
#[test]
fn an_unwired_folder_reports_the_tool_it_traces_and_how_to_get_its_text() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);
    // A folder that uses Claude Code, with nothing wired in it.
    std::fs::create_dir_all(cli.home.join(".claude")).unwrap();

    let doc = cli.json(&["task", "list", "--json"]);
    let report = &doc["setup_incomplete"]["agent_hook"];
    assert_eq!(report["unwired"][0]["tool"], "claude-code");
    assert_eq!(report["unwired"][0]["label"], "Claude Code");
    assert!(
        report["unwired"][0]["fix"].as_str().is_some_and(|fix| fix.contains("agent-hook snippet claude-code")),
        "the report does not say how to get the text: {report}"
    );
    assert_eq!(report["any_wired"], false);
    // The catalog rides along, because a harness that left no trace still knows which one it is.
    assert!(report["tools"].as_array().is_some_and(|all| all.len() >= 5), "{report}");

    // The text face says it on stderr, and the command the user actually ran still succeeds.
    let (out, err, code) = cli.run_both(&["task", "list"]);
    assert_eq!(code, 0, "the report is a warning, not a refusal: {err}");
    assert!(err.contains("agent-hook snippet claude-code"), "stderr does not name the way out: {err}");
    assert!(!out.contains("agent-hook"), "the report leaked into stdout: {out}");
}

/// A folder that shows no AI tool of its own is told nothing **as a person** — a warning naming no tool is
/// one nobody can act on, and it would arrive on every command — while the `--json` face still carries it,
/// because the reader there is the harness and knows which one it is (`AMB-D-440`).
#[test]
fn a_folder_that_traces_no_tool_says_it_only_where_the_reader_can_name_its_own() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);

    let (_, err, code) = cli.run_both(&["task", "list"]);
    assert_eq!(code, 0);
    assert!(!err.contains("agent-hook"), "a person was warned about a tool amenbo cannot see: {err}");

    let doc = cli.json(&["task", "list", "--json"]);
    let report = &doc["setup_incomplete"]["agent_hook"];
    assert_eq!(report["any_wired"], false, "the AI is told what the folder is missing: {doc}");
    assert!(report["unwired"].as_array().is_some_and(|named| named.is_empty()), "{report}");
    assert!(report["tools"].as_array().is_some_and(|all| all.len() >= 5), "{report}");
}

/// Once the wiring has landed the report stops — and only then. It is the file that ends it, since amenbo
/// never writes that file itself.
///
/// What lands here is the `configuration` the request carries, written by the hand the request is
/// addressed to: the request itself is prose and would not parse as settings, which is exactly why the
/// `--json` face carries the two apart.
#[test]
fn a_wired_folder_is_told_nothing() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);
    std::fs::create_dir_all(cli.home.join(".claude")).unwrap();

    let doc = cli.json(&["agent-hook", "snippet", "claude-code", "--json"]);
    let configuration = doc["configuration"].as_str().expect("the document carries no configuration");
    std::fs::write(cli.home.join(".claude/settings.json"), configuration).unwrap();

    let doc = cli.json(&["task", "list", "--json"]);
    assert!(
        doc.get("setup_incomplete").is_none(),
        "a folder that starts its AI on amenbo has nothing left to finish: {doc}"
    );
}

/// The question amenbo cannot put on the `--json` face is closed from the outside: the report names the
/// command that writes the answer back, an AI puts the question to the human, and recording their answer
/// takes the question off the report (`AMB-D-440`). The wiring is a separate fact, so the report itself
/// stands — nothing has been wired.
#[test]
fn an_ai_records_the_humans_answer_and_the_question_stops_being_carried() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);

    let doc = cli.json(&["task", "list", "--json", "--actor", "ai"]);
    let asked = doc["setup_incomplete"]["agent_hook"]["record_answer"].as_str();
    assert!(
        asked.is_some_and(|how| how.contains("agent-hook answer")),
        "the open question does not name the way back: {doc}"
    );

    let done = cli.json(&["agent-hook", "answer", "yes", "--json", "--actor", "ai"]);
    assert_eq!(done["allowed"], true);
    // A yes is an answer, not a wiring: what is still owed is the edit, and the document says so.
    assert!(
        done["next"].as_str().is_some_and(|next| next.contains("agent-hook snippet")),
        "a yes leaves the wiring owed, and the document does not say it: {done}"
    );

    let doc = cli.json(&["task", "list", "--json", "--actor", "ai"]);
    let report = &doc["setup_incomplete"]["agent_hook"];
    assert!(report["record_answer"].is_null(), "an answered question is still being asked: {report}");
    assert_eq!(report["any_wired"], false, "consent is not wiring: {report}");
}

/// A `no` is "stop asking", and it stops the report with it — but it forbids nothing: the text is still
/// handed over to whoever asks for it.
#[test]
fn a_recorded_no_ends_the_report_and_bars_nothing() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);
    std::fs::create_dir_all(cli.home.join(".claude")).unwrap();

    let (out, code) = cli.run(&["agent-hook", "answer", "no", "--actor", "ai"]);
    assert_eq!(code, 0, "{out}");

    let doc = cli.json(&["task", "list", "--json", "--actor", "ai"]);
    assert!(doc.get("setup_incomplete").is_none(), "a reader who said no is still being told: {doc}");
    let (_, err, code) = cli.run_both(&["task", "list"]);
    assert_eq!(code, 0);
    assert!(!err.contains("agent-hook"), "a reader who said no is still being warned: {err}");

    let (request, code) = cli.run(&["agent-hook", "snippet", "claude-code"]);
    assert_eq!(code, 0, "a no closed the door, not just the question");
    assert!(request.contains(" agent --json"), "the text is still handed over: {request}");
}

/// The `--json` face carries the request plus the file it names, which is what lets an AI hand both to the
/// human in one message — and the configuration on its own beside it, for a caller that is doing the edit
/// rather than passing the request on. `copied` says which route the text took.
#[test]
fn agent_hook_snippet_json_carries_the_request_the_configuration_and_its_destination() {
    let cli = Cli::new();

    let doc = cli.json(&["agent-hook", "snippet", "cursor", "--json"]);
    assert_eq!(doc["tool"], "cursor");
    assert_eq!(doc["label"], "Cursor");
    assert_eq!(doc["paste_into"], ".cursor/hooks.json");
    assert_eq!(doc["copied"], false);
    let request = doc["request"].as_str().expect("the document carries no request");
    let configuration = doc["configuration"].as_str().expect("the document carries no configuration");
    assert!(request.contains(configuration), "the request does not carry the configuration: {doc}");
    assert!(request.contains(".cursor/hooks.json"), "the request names no file: {doc}");
    // The configuration alone is a settings file; the request around it is not, and the two fields are
    // apart precisely so a caller never has to guess which of those it is holding.
    serde_json::from_str::<Value>(configuration).expect("the configuration is not a settings file");
    assert!(configuration.contains(" agent --json"), "{doc}");
}

/// The lint-hook probe asks git where the hooks live, and that one spawn rides every amenbo command. What
/// is held here is the **count**, counted by putting a `git` on `PATH` that logs each call and delegates to
/// the real one — not a wall-clock number, because the cost is process startup, which says more about the
/// machine (a `/usr/bin/git` that is an xcrun shim costs double a real one) than about this code, while
/// "how many times is git spawned" is the same fact everywhere. The `hooks` faces are not routed through
/// the standing setup check that runs ahead of every other command, which is what keeps them to the one
/// probe they came for, and keeps `hooks status` read-only as its spec says: that check can pull the record
/// to `yes` when it finds our hook on disk, and a read must not do that behind a reader's back.
#[test]
fn the_hook_probe_spawns_git_once_per_command_and_never_for_hooks_itself() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);
    let real_git = String::from_utf8(Command::new("/usr/bin/env").args(["which", "git"]).output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    Command::new(&real_git).current_dir(&cli.home).args(["init", "-q"]).output().unwrap();

    let shim_dir = cli.home.join("shim");
    std::fs::create_dir_all(&shim_dir).unwrap();
    let log = cli.home.join("git-calls.log");
    std::fs::write(
        shim_dir.join("git"),
        format!("#!/bin/sh\necho \"$@\" >> {}\nexec {real_git} \"$@\"\n", log.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(shim_dir.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let spawns = |args: &[&str]| -> usize {
        let _ = std::fs::remove_file(&log);
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("PATH", &shim_dir)
            .current_dir(&cli.home)
            .args(with_defaults(args, "human"))
            .output()
            .expect("failed to run the binary");
        assert_eq!(exit_code(&out), 0, "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        std::fs::read_to_string(&log).map(|s| s.lines().count()).unwrap_or(0)
    };

    assert_eq!(spawns(&["task", "list", "--json"]), 1, "an ordinary command probes the hooks once");
    assert_eq!(spawns(&["hooks", "status", "--json"]), 1, "and the hooks' own faces do not probe twice");

    Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&cli.home)
        .args(["hooks", "install", "--actor", "human"])
        .output()
        .unwrap();
    let consent_before = cli.json(&["hooks", "status", "--json"])["consent"].clone();
    assert_eq!(consent_before, serde_json::json!("yes"), "the explicit install recorded the answer");
}

/// Resolve the real `git` on this machine, the way the probe test does.
#[cfg(unix)]
fn real_git_path() -> String {
    String::from_utf8(Command::new("/usr/bin/env").args(["which", "git"]).output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_string()
}

/// Build a `bin` dir holding a `git` symlink — so a fully-controlled `PATH` can still run git — and, when
/// `with_amenbo`, an `amenbo` symlink to this build. The hook's `command -v amenbo` has to resolve to the
/// binary under test, not whatever is installed on the machine running the suite; and "uninstalled" has to
/// mean a `PATH` that holds no amenbo at all, which the machine's own `PATH` cannot promise. Returns the dir
/// as the whole `PATH` value.
#[cfg(unix)]
fn hook_path(bin: &std::path::Path, with_amenbo: bool) -> String {
    std::fs::create_dir_all(bin).unwrap();
    let git = bin.join("git");
    let _ = std::fs::remove_file(&git);
    std::os::unix::fs::symlink(real_git_path(), &git).unwrap();
    if with_amenbo {
        let link = bin.join("amenbo");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_amenbo"), &link).unwrap();
    }
    bin.display().to_string()
}

/// The whole life of an installed hook, driven through real `git commit`: a ref in the staged diff is
/// refused, a ref in the message is refused, a clean commit goes through, an amenbo gone from `PATH` never
/// traps a commit, and `uninstall` stops the gate-keeping. The unit tests see the install/strip logic and
/// the generated shell each in isolation; only this runs git the way a user does — and both shipped bugs (a
/// ref slipping through, an uninstalled amenbo trapping every commit) lived past where those tests looked.
#[cfg(unix)]
#[test]
fn the_installed_hook_lives_its_whole_life_under_real_git() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);
    let repo = a_repo();

    let with_amenbo = hook_path(&cli.home.join("bin"), true);
    let no_amenbo = hook_path(&cli.home.join("gitonly"), false);

    let installed = cli.json_from(&repo, &["hooks", "install", "--json"]);
    assert_eq!(installed["ok"], serde_json::json!(true), "install: {installed}");

    // Write `content` to `file`, stage everything, and attempt a commit with `message` on `path`.
    let commit = |file: &str, content: &str, message: &str, path: &str| -> std::process::Output {
        std::fs::write(repo.join(file), content).unwrap();
        git(&repo, &["add", "-A"]);
        Command::new("git")
            .current_dir(&repo)
            .env("PATH", path)
            .env("AMENBO_UPDATE_CHECK", "0")
            .args(["commit", "-m", message])
            .output()
            .unwrap()
    };

    // A ref in the staged diff is refused — the pre-commit slot lints what the commit adds.
    let diff = commit("leak.txt", "adds AMB-T-1656 here\n", "chore: a clean message", &with_amenbo);
    assert!(!diff.status.success(), "a ref in the staged diff must be refused: {}", String::from_utf8_lossy(&diff.stderr));
    std::fs::remove_file(repo.join("leak.txt")).unwrap(); // so the next `add -A` stages it away

    // A ref in the message is refused — the commit-msg slot lints the message file git hands it.
    let msg = commit("one.txt", "one\n", "chore: closes AMB-T-1655", &with_amenbo);
    assert!(!msg.status.success(), "a ref in the message must be refused: {}", String::from_utf8_lossy(&msg.stderr));

    // The same change with a clean message goes through.
    let clean = commit("one.txt", "one\n", "chore: tidy up", &with_amenbo);
    assert!(clean.status.success(), "a clean commit must go through: {}", String::from_utf8_lossy(&clean.stderr));

    // amenbo gone from PATH: even a ref in the message must not trap the commit — `command -v` fails, the
    // block's `|| true` clears it, and a standalone hook lets the commit through.
    let gone = commit("two.txt", "two\n", "chore: closes AMB-T-1655, amenbo gone", &no_amenbo);
    assert!(gone.status.success(), "an uninstalled amenbo must not trap a commit: {}", String::from_utf8_lossy(&gone.stderr));

    // uninstall stops the gate-keeping: a ref in the message now goes through.
    let removed = cli.json_from(&repo, &["hooks", "uninstall", "--json"]);
    assert_eq!(removed["ok"], serde_json::json!(true), "uninstall: {removed}");
    let after = commit("three.txt", "three\n", "chore: closes AMB-T-1655 after uninstall", &with_amenbo);
    assert!(after.status.success(), "uninstall must remove the gate: {}", String::from_utf8_lossy(&after.stderr));
}

/// Installed beside another tool's hook, amenbo guards without disturbing it, and `uninstall` takes only
/// amenbo's block — the other tool's hook keeps running. amenbo's block sits after the shebang and runs
/// first, and when it passes, control falls through to the foreign body.
#[cfg(unix)]
#[test]
fn the_hook_coexists_with_a_foreign_hook_and_leaves_it_on_uninstall() {
    use std::os::unix::fs::PermissionsExt;
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);
    let repo = a_repo();
    let with_amenbo = hook_path(&cli.home.join("bin"), true);

    // A foreign commit-msg hook that leaves a mark when it runs, so we can see it still fires.
    let hooks_dir = repo.join(".git").join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    // The body marks a file with only shell built-ins (`: > file`), so it needs nothing on the tightly
    // controlled PATH these commits run under — the mark appearing is proof the foreign body actually ran.
    let mark = repo.join("foreign-ran");
    std::fs::write(hooks_dir.join("commit-msg"), format!("#!/bin/sh\n: > {}\nexit 0\n", mark.display())).unwrap();
    std::fs::set_permissions(hooks_dir.join("commit-msg"), std::fs::Permissions::from_mode(0o755)).unwrap();

    let installed = cli.json_from(&repo, &["hooks", "install", "--json"]);
    assert_eq!(installed["ok"], serde_json::json!(true), "install: {installed}");

    let commit = |file: &str, message: &str| -> std::process::Output {
        std::fs::write(repo.join(file), format!("{file}\n")).unwrap();
        git(&repo, &["add", "-A"]);
        Command::new("git")
            .current_dir(&repo)
            .env("PATH", &with_amenbo)
            .env("AMENBO_UPDATE_CHECK", "0")
            .args(["commit", "-m", message])
            .output()
            .unwrap()
    };

    // A ref in the message is still refused — amenbo's block runs before the foreign body ever gets control.
    let blocked = commit("a.txt", "chore: closes AMB-T-1655");
    assert!(!blocked.status.success(), "the coexisting block must still refuse a ref");
    assert!(!mark.exists(), "a refused commit must not have reached the foreign body");

    // A clean commit passes and the foreign hook runs (control fell through to it).
    let _ = std::fs::remove_file(&mark);
    let clean = commit("a.txt", "chore: tidy up");
    assert!(clean.status.success(), "a clean commit must pass: {}", String::from_utf8_lossy(&clean.stderr));
    assert!(mark.exists(), "the foreign hook must still run when amenbo's block falls through");

    // uninstall takes only amenbo's block; the foreign body stays and keeps running.
    let removed = cli.json_from(&repo, &["hooks", "uninstall", "--json"]);
    assert_eq!(removed["ok"], serde_json::json!(true), "uninstall: {removed}");
    let body = std::fs::read_to_string(hooks_dir.join("commit-msg")).unwrap();
    assert!(!body.contains("amenbo:hook"), "uninstall left amenbo's managed block behind: {body}");
    assert!(body.contains(": >"), "uninstall must leave the foreign hook intact: {body}");
    let _ = std::fs::remove_file(&mark);
    let still = commit("b.txt", "chore: still here");
    assert!(still.status.success() && mark.exists(), "the foreign hook must survive uninstall");
}
