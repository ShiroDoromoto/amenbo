//! The harness every `cli_e2e_*` suite runs on: a throwaway `AMENBO_HOME`, the built binary, and the
//! few readings of its output that every slice needs.
//!
//! One copy, pulled into each suite as a module, so how a test reaches the binary is written once
//! and changed once. Each suite uses the part of it that its own subject needs, which is what the
//! file-wide `dead_code` allow is for: an unused helper here means a suite that did not need it, not
//! a helper nobody uses.

#![allow(dead_code)]

use std::process::Command;

use serde_json::Value;

/// A fresh, isolated AMENBO_HOME for each test.
pub(crate) fn temp_home() -> std::path::PathBuf {
    amenbo_scratch::scratch("home")
}

/// The child's exit code — or a stop that names the signal that ended it.
///
/// A signalled child has no code at all, and folding that into a number (`-1`) makes it read as an
/// ordinary non-zero exit: the assertion that follows blames the command's behaviour, so whoever
/// pushed reads the red as their own change breaking something. It is a different fact — the run did
/// not fail, it was ended, usually after it had already written its answer — and it has only ever been
/// seen on CI's combined scale+e2e run, where re-running the same commit came back green
/// (`AMB-T-2103`). Say which signal, where the fact is still known.
pub(crate) fn exit_code(out: &std::process::Output) -> i32 {
    match out.status.code() {
        Some(code) => code,
        None => panic!(
            "the Amenbo child was ended by {}, not by a command that failed — no assertion was reached.\n\
             Re-run before suspecting the change: this has been seen on CI's combined scale+e2e run only.\n\
             stdout: {}\nstderr: {}",
            signal_name(&out.status),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        ),
    }
}

/// Name the signal that ended a child, since the number alone is what nobody remembers. The few that
/// can plausibly land here are named; anything else is reported as its number.
#[cfg(unix)]
pub(crate) fn signal_name(status: &std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.signal() {
        Some(sig) => {
            let known = match sig {
                6 => " (SIGABRT)",
                9 => " (SIGKILL — an out-of-memory kill arrives as this)",
                11 => " (SIGSEGV)",
                13 => " (SIGPIPE — a write to a pipe nobody is reading)",
                _ => "",
            };
            format!("signal {sig}{known}")
        }
        None => "no exit code and no signal".to_string(),
    }
}

/// Off Unix there is no signal to name: a child that ends without a code is all the platform says.
#[cfg(not(unix))]
pub(crate) fn signal_name(_status: &std::process::ExitStatus) -> String {
    "no exit code".to_string()
}

/// `args` with the inputs every call needs but almost no test is *about*, filled in.
///
/// The facet, declared on the command line — `--actor <facet>` appended, which is the one input Amenbo
/// is to take it by (`AMB-D-408`). A test that declares its own facet in `args` is left alone, so
/// `--actor ai` in a call still means what it says. The flag beats anything the environment carries, so
/// a run is the same whatever the shell the tests were started from had set.
///
/// And the folder `project add` links (`AMB-D-529`): a fresh one per call, so a suite whose subject is
/// tasks or decisions still creates the project it files them under without naming a folder. A call that
/// names its own `--dir` — the tests that *are* about the linking — is left alone.
pub(crate) fn with_defaults(args: &[&str], facet: &str) -> Vec<String> {
    let mut with: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    if !args.contains(&"--actor") {
        with.push("--actor".to_string());
        with.push(facet.to_string());
    }
    if args.first() == Some(&"project") && args.get(1) == Some(&"add") && !args.contains(&"--dir") {
        with.push("--dir".to_string());
        with.push(a_project_folder());
    }
    with
}

/// A folder for a project a test creates: a fresh one each call, cut **beside** the home rather than
/// inside it — a folder under one that `init` has bound reads as already managed, which is the refusal
/// `project add` owes and not the state a test about something else wants to be in.
fn a_project_folder() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    amenbo_scratch::scratch(&format!("project-dir-{n}")).to_string_lossy().into_owned()
}

pub(crate) struct Cli {
    pub(crate) home: std::path::PathBuf,
}

impl Cli {
    pub(crate) fn new() -> Cli {
        let home = temp_home();
        // Isolate the CWD too, so the .amenbo / AGENTS.md that init drops never land in the repo.
        std::fs::create_dir_all(&home).unwrap();
        Cli { home }
    }

    /// Run the binary and return (stdout, exit_code).
    pub(crate) fn run(&self, args: &[&str]) -> (String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            // No update check: the tests never reach GitHub and never touch the real OS cache (hermetic).
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&self.home)
            // A write with no facet from a non-interactive caller (the test runner has no TTY) is refused
            // with facet_required, so every call declares one; a test that names its own is left alone.
            .args(with_defaults(args, "human"))
            .output()
            .expect("failed to run the binary");
        (
            String::from_utf8_lossy(&out.stdout).to_string(),
            exit_code(&out),
        )
    }

    /// Run `--json` from a different CWD against the same `AMENBO_HOME`. Needed to exercise behaviour
    /// **outside** a bound folder — a folder you never run Amenbo in gets no automatic follow-up.
    pub(crate) fn json_from(&self, cwd: &std::path::Path, args: &[&str]) -> Value {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(cwd)
            .args(with_defaults(args, "human"))
            .output()
            .expect("failed to run the binary");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(exit_code(&out), 0, "command {args:?} exited non-zero: {stdout}");
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("failed to parse JSON {args:?}: {e}\n{stdout}"))
    }

    /// Run with `--json` and parse stdout as JSON.
    pub(crate) fn json(&self, args: &[&str]) -> Value {
        let (stdout, code) = self.run(args);
        assert_eq!(code, 0, "command {args:?} exited non-zero: {stdout}");
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("failed to parse JSON {args:?}: {e}\n{stdout}"))
    }

    /// Run with `--json`, piping `stdin` in — for the body options' `-`, whose whole point is text that
    /// never passes through the shell.
    pub(crate) fn json_stdin(&self, args: &[&str], stdin: &str) -> Value {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&self.home)
            .args(with_defaults(args, "human"))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to run the binary");
        child.stdin.as_mut().unwrap().write_all(stdin.as_bytes()).unwrap();
        drop(child.stdin.take());
        let out = child.wait_with_output().expect("failed to wait for the binary");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(exit_code(&out), 0, "command {args:?} exited non-zero: {stdout}");
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("failed to parse JSON {args:?}: {e}\n{stdout}"))
    }

    /// Run the binary and return (stdout, stderr, exit_code) — for a command that succeeds on stdout
    /// while also emitting an advisory on stderr (the two streams inspected together).
    pub(crate) fn run_both(&self, args: &[&str]) -> (String, String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&self.home)
            .args(with_defaults(args, "human"))
            .output()
            .expect("failed to run the binary");
        (
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
            exit_code(&out),
        )
    }

    /// Run the binary and return (stderr, exit_code); used for the error paths.
    pub(crate) fn run_err(&self, args: &[&str]) -> (String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            // No update check: the tests never reach GitHub and never touch the real OS cache (hermetic).
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&self.home)
            // The facet, declared the same way `run` declares it.
            .args(with_defaults(args, "human"))
            .output()
            .expect("failed to run the binary");
        (
            String::from_utf8_lossy(&out.stderr).to_string(),
            exit_code(&out),
        )
    }

    /// Run the binary with extra environment on top of the harness's, and return (stdout, exit_code).
    ///
    /// For the commands that reach outside the machine: the plugin catalog's URL has to be pinned at
    /// something that never answers, or the test spends the real index's availability on a question it
    /// already seeded the answer to on disk.
    pub(crate) fn run_env(&self, env: &[(&str, &str)], args: &[&str]) -> (String, i32) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_amenbo"));
        command
            .env("AMENBO_HOME", &self.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&self.home)
            .args(with_defaults(args, "human"));
        for (key, value) in env {
            command.env(key, value);
        }
        let out = command.output().expect("failed to run the binary");
        (String::from_utf8_lossy(&out.stdout).to_string(), exit_code(&out))
    }

    /// Test helper: create one project and return its id. `task add` always needs a project, so tests
    /// about assignment / status / the mailbox — where the home project is incidental — use this.
    pub(crate) fn a_project(&self) -> String {
        id_str(&self.json(&["project", "add", "--name", "P", "--json"])["project"]["id"])
    }

    /// Test helper: finish creating a task — the second stage of every creation (`AMB-D-554`). What
    /// `task add` leaves behind cannot be reserved, so a test that goes on to reserve one runs this the
    /// way a caller does; a test that only reads back the row `add` wrote does not need it.
    pub(crate) fn finish_creating(&self, id: &str) {
        self.json(&["task", "finish-creating", id, "--json"]);
    }

    /// Test helper: the project this CWD's `.amenbo` points at (the default project `init` made, first
    /// in the listing). AI-facet work is confined to the bound project, so **tests acting as the AI must
    /// target this one** — the separate project `a_project` creates is outside the AI's reach.
    pub(crate) fn bound_project(&self) -> String {
        id_str(&self.json(&["project", "list", "--json"])["projects"][0]["id"])
    }
}

/// Turn a JSON id into a string that can be handed back as a CLI argument. project / dimension /
/// dimension_value ids are **numbers** (decision and friends are strings), so `as_str()` won't do.
pub(crate) fn id_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => panic!("not an id JSON value: {other}"),
    }
}

/// Spell a task id as the ref `dimension set` / `unset` demand: a bare number is refused there, tasks
/// and decisions numbering independently (`AMB-D-781`).
pub(crate) fn task_ref(id: &str) -> String {
    format!("AMB-T-{id}")
}

/// The decision half of [`task_ref`].
#[allow(dead_code)]
pub(crate) fn decision_ref(id: &str) -> String {
    format!("AMB-D-{id}")
}

/// Plant an installed plugin under the test's app-data: the manifest (the install marker) plus the
/// executable named after it, which is the whole on-disk shape `plugin_installed::read` looks for.
pub(crate) fn install_plugin(cli: &Cli, name: &str, config: serde_json::Value) {
    install_plugin_at(cli, name, config, None);
}

/// The same install, with the author's layer declared (`AMB-D-601`): `scope` of `Some("machine")` is a
/// plugin whose gate, settings and secrets are the device's. `None` writes no `scope` key at all — the
/// undeclared manifest every plugin shipped before this, which must keep meaning `project`.
pub(crate) fn install_plugin_at(
    cli: &Cli,
    name: &str,
    config: serde_json::Value,
    scope: Option<&str>,
) {
    let dir = cli.home.join("plugins").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = serde_json::json!({
        "name": name,
        "desc": "テスト用",
        "author": "amenbo",
        "repo": "ShiroDoromoto/amenbo-plugin-test",
        "os": ["macos", "linux", "windows"],
        "category": "workflow",
        "url": "https://example.com/x.tar.gz",
        "checksum": "sha256:deadbeef",
        // What an install records of the detail document it was installed from (`AMB-D-386`) — the
        // value a later catalog fetch compares against to say the plugin has moved.
        "detail_sum": format!("sha256:{}", "d".repeat(64)),
        "config": config,
    });
    let mut manifest = manifest;
    if let Some(scope) = scope {
        manifest["scope"] = serde_json::json!(scope);
    }
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    std::fs::write(dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)), b"#!/bin/sh\n").unwrap();
}

/// Plant an installed plugin that subscribes to `events` — [`install_plugin`] with the manifest field the
/// dispatch resolver reads. The executable it lays down does nothing; a caller that wants the plugin to
/// *do* something overwrites it.
#[cfg(unix)]
pub(crate) fn install_subscribing_plugin(cli: &Cli, name: &str, events: &[&str]) {
    install_plugin(cli, name, serde_json::json!([]));
    let manifest_file = cli.home.join("plugins").join(name).join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_file).unwrap()).unwrap();
    manifest["events"] = serde_json::json!(events);
    std::fs::write(&manifest_file, serde_json::to_vec(&manifest).unwrap()).unwrap();
}

/// The JSON a **runner process** wrote at `path`, waited for (`AMB-T-2175`).
///
/// A runner is launched by the command that queued the event and outlives it, so what the plugin writes
/// lands *after* that command has returned — there is nothing for a caller to join any more. `want` picks
/// the value being waited for, which is what tells a second run from the one already on disk rather than
/// racing it.
#[cfg(unix)]
pub(crate) fn wrote_json(path: &std::path::Path, want: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..200 {
        if let Ok(v) = std::fs::read_to_string(path).map_err(|_| ()).and_then(|t| {
            serde_json::from_str::<Value>(&t).map_err(|_| ())
        }) {
            if want(&v) {
                return v;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("no runner wrote the payload waited for at {} within ten seconds", path.display());
}
