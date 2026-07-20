//! End-to-end integration tests for the CLI: the built binary is run against a throwaway AMENBO_HOME.

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Once;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// The one parent every throwaway directory in this file sits under, so the sweep below can only ever
/// reach leftovers of these tests — never a neighbour's files under `temp_dir()`.
fn scratch_root() -> std::path::PathBuf {
    std::env::temp_dir().join("amenbo-test")
}

/// A throwaway directory nobody else holds, named `<tag>-<pid>-<nanos>-<n>`.
///
/// Uniqueness must not rest on the pid alone: the OS recycles ids, so two runs were handed the same path
/// and one could open the other's leftovers — or wipe a live run's working directory on its way in. The
/// wall clock separates runs, the counter separates calls within a run, and the pid separates two runs
/// that start in the same nanosecond. The path returned is new, so there is nothing to wipe first.
fn scratch(tag: &str) -> std::path::PathBuf {
    sweep_old_scratch();
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    scratch_root().join(format!("{tag}-{:x}-{nanos:x}-{n:x}", std::process::id()))
}

/// Sweep on the way in, not on the way out. A `Drop` guard never runs when nextest kills a hung test (nor
/// under Ctrl-C or `process::exit`), and it would take the wreckage of a failure with it — which is the
/// thing one wants to read afterwards. Leaving the job to whoever runs next survives both, and caps what
/// accumulates at a day's worth. Only entries older than that go, so a parallel run is never touched.
fn sweep_old_scratch() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let cutoff = SystemTime::now() - Duration::from_secs(24 * 60 * 60);
        let Ok(entries) = std::fs::read_dir(scratch_root()) else { return };
        for entry in entries.flatten() {
            let stale = entry.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < cutoff);
            if stale {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    });
}

/// A fresh, isolated AMENBO_HOME for each test.
fn temp_home() -> std::path::PathBuf {
    scratch("home")
}

struct Cli {
    home: std::path::PathBuf,
}

impl Cli {
    fn new() -> Cli {
        let home = temp_home();
        // Isolate the CWD too, so the .amenbo / AGENTS.md that init drops never land in the repo.
        std::fs::create_dir_all(&home).unwrap();
        Cli { home }
    }

    /// Run the binary and return (stdout, exit_code).
    fn run(&self, args: &[&str]) -> (String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            // A write with no facet from a non-interactive caller (the test runner has no TTY) is refused with
            // facet_required, so declare one here; tests that pass --actor ai override it themselves.
            .env("AMENBO_ACTOR", "human")
            // No update check: the tests never reach GitHub and never touch the real OS cache (hermetic).
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&self.home)
            .args(args)
            .output()
            .expect("failed to run the binary");
        (
            String::from_utf8_lossy(&out.stdout).to_string(),
            out.status.code().unwrap_or(-1),
        )
    }

    /// Run `--json` from a different CWD against the same `AMENBO_HOME`. Needed to exercise behaviour
    /// **outside** a bound folder — a folder you never run amenbo in gets no automatic follow-up.
    fn json_from(&self, cwd: &std::path::Path, args: &[&str]) -> Value {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            .env("AMENBO_ACTOR", "human")
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("failed to run the binary");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(out.status.code(), Some(0), "command {args:?} exited non-zero: {stdout}");
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("failed to parse JSON {args:?}: {e}\n{stdout}"))
    }

    /// Run with `--json` and parse stdout as JSON.
    fn json(&self, args: &[&str]) -> Value {
        let (stdout, code) = self.run(args);
        assert_eq!(code, 0, "command {args:?} exited non-zero: {stdout}");
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("failed to parse JSON {args:?}: {e}\n{stdout}"))
    }

    /// Run with `--json`, piping `stdin` in — for the body options' `-`, whose whole point is text that
    /// never passes through the shell.
    fn json_stdin(&self, args: &[&str], stdin: &str) -> Value {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            .env("AMENBO_ACTOR", "human")
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&self.home)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to run the binary");
        child.stdin.as_mut().unwrap().write_all(stdin.as_bytes()).unwrap();
        drop(child.stdin.take());
        let out = child.wait_with_output().expect("failed to wait for the binary");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(out.status.code(), Some(0), "command {args:?} exited non-zero: {stdout}");
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("failed to parse JSON {args:?}: {e}\n{stdout}"))
    }

    /// Run the binary and return (stderr, exit_code); used for the error paths.
    fn run_err(&self, args: &[&str]) -> (String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &self.home)
            // A write with no facet from a non-interactive caller (the test runner has no TTY) is refused with
            // facet_required, so declare one here; tests that pass --actor ai override it themselves.
            .env("AMENBO_ACTOR", "human")
            // No update check: the tests never reach GitHub and never touch the real OS cache (hermetic).
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&self.home)
            .args(args)
            .output()
            .expect("failed to run the binary");
        (
            String::from_utf8_lossy(&out.stderr).to_string(),
            out.status.code().unwrap_or(-1),
        )
    }

    /// Test helper: create one project and return its id. `task add` always needs a project, so tests
    /// about assignment / status / the mailbox — where the home project is incidental — use this.
    fn a_project(&self) -> String {
        id_str(&self.json(&["project", "add", "--name", "P", "--json"])["project"]["id"])
    }

    /// Test helper: the project this CWD's `.amenbo` points at (the default project `init` made, first
    /// in the listing). AI-facet work is confined to the bound project, so **tests acting as the AI must
    /// target this one** — the separate project `a_project` creates is outside the AI's reach.
    fn bound_project(&self) -> String {
        id_str(&self.json(&["project", "list", "--json"])["projects"][0]["id"])
    }
}

/// Turn a JSON id into a string that can be handed back as a CLI argument. project / dimension /
/// dimension_value ids are **numbers** (decision and friends are strings), so `as_str()` won't do.
fn id_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => panic!("not an id JSON value: {other}"),
    }
}

#[test]
fn full_task_lifecycle() {
    let cli = Cli::new();

    // Project → task (classification lives on dimensions; the task's place is the project itself).
    let p = cli.json(&["project", "add", "--name", "サイト刷新", "--view", "board", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    assert_eq!(p["action"], "project.add");

    let t = cli.json(&[
        "task", "add", "--title", "ワイヤー作成", "--project", &pid,
        "--due", "2026-06-30", "--priority", "high", "--json",
    ]);
    let tid = id_str(&t["task"]["id"]);
    assert_eq!(t["task"]["due_on"], "2026-06-30");
    assert_eq!(t["task"]["priority"], "high");
    assert_eq!(id_str(&t["task"]["placement"]["project"]["id"]), pid);

    // Breaking work down means another task plus a dependency edge.
    let t2 = cli.json(&[
        "task", "add", "--title", "配色決め", "--project", &pid, "--json",
    ]);
    let t2id = id_str(&t2["task"]["id"]);
    // The wireframe depends on the palette (the palette must be done first).
    let dep = cli.json(&["task", "depend", &tid, "--on", &t2id, "--json"]);
    assert_eq!(id_str(&dep["task"]["blocked_by"][0]["id"]), t2id);

    // Listing: both tasks sit at top level — there is no parent/child folding.
    let list = cli.json(&["task", "list", "--project", &pid, "--json"]);
    assert_eq!(list["count"], 2);

    // Completion is idempotent.
    let done = cli.json(&["task", "done", &tid, "--json"]);
    assert_eq!(done["task"]["completed"], true);
    assert_eq!(done["noop"], false);
    let again = cli.json(&["task", "done", &tid, "--json"]);
    assert_eq!(again["noop"], true);
}

#[test]
fn task_dependencies_drive_ready_and_unblock() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "依存PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let a = cli.json(&["task", "add", "--title", "土台", "--project", &pid, "--json"]);
    let aid = id_str(&a["task"]["id"]);
    let b = cli.json(&["task", "add", "--title", "上物", "--project", &pid, "--json"]);
    let bid = id_str(&b["task"]["id"]);

    // b depends on a (a must be done first).
    let dep = cli.json(&["task", "depend", &bid, "--on", &aid, "--json"]);
    assert_eq!(dep["action"], "task.depend");
    assert_eq!(dep["task"]["ready"], false);
    assert_eq!(id_str(&dep["task"]["blocked_by"][0]["id"]), aid);

    // Self-reference and cycles are refused (a→b would close the loop with b→a).
    let (_e, code) = cli.run_err(&["task", "depend", &bid, "--on", &bid, "--json"]);
    assert_ne!(code, 0);
    let (_e2, code2) = cli.run_err(&["task", "depend", &aid, "--on", &bid, "--json"]);
    assert_ne!(code2, 0);

    // A ready:yes mailbox holds only a; b is blocked.
    let ready = cli.json(&["task", "list", "--project", &pid, "--filter", "ready:yes", "--json"]);
    let ready_ids: Vec<String> = ready["tasks"].as_array().unwrap().iter().map(|t| id_str(&t["id"])).collect();
    assert!(ready_ids.contains(&aid) && !ready_ids.contains(&bid));
    // ready:no (blocked) holds only b.
    let blocked = cli.json(&["task", "list", "--project", &pid, "--filter", "ready:no", "--json"]);
    let blocked_ids: Vec<String> = blocked["tasks"].as_array().unwrap().iter().map(|t| id_str(&t["id"])).collect();
    assert_eq!(blocked_ids, vec![bid.clone()]);

    // Completing a makes b ready and records task.unblocked on b.
    cli.json(&["task", "done", &aid, "--json"]);
    let show_b = cli.json(&["task", "show", &bid, "--json"]);
    assert_eq!(show_b["ready"], true);
    assert!(show_b["blocked_by"].as_array().unwrap().is_empty());
    let acts = cli.json(&["activity", "--task", &bid, "--json"]);
    let has_unblock = acts["items"].as_array().unwrap().iter()
        .any(|i| i["event"]["kind"] == "task.unblocked");
    assert!(has_unblock, "task.unblocked was not recorded: {acts}");
}

#[test]
fn config_onboarded_flag_roundtrips() {
    let cli = Cli::new();
    // The flag starts false; the GUI's first-run setup keys off config.onboarded.
    let c0 = cli.json(&["config", "--json"]);
    assert_eq!(c0["settings"]["onboarded"], false);
    // An explicit set persists true into config.
    cli.run(&["config", "set", "onboarded", "true"]);
    let c1 = cli.json(&["config", "--json"]);
    assert_eq!(c1["settings"]["onboarded"], true);
    // Bogus values are rejected.
    let (_e, code) = cli.run_err(&["config", "set", "onboarded", "maybe"]);
    assert_ne!(code, 0);
}

/// `amenbo export` streams the whole single-DB store out as portable JSON (a thin wrapper over core's
/// `export_json`). There is exactly one shape — no excerpts, no markdown/csv. The envelope carries an
/// `amenbo_export` header, and its reader lives outside amenbo: nothing reads it back in.
#[test]
fn whole_device_export_streams_the_tables() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "資料", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    for title in ["下書き", "推敲"] {
        cli.json(&["task", "add", "--title", title, "--project", &pid, "--json"]);
    }

    // export (whole-device by default): the envelope goes to stdout.
    let (dump, code) = cli.run(&["export"]);
    assert_eq!(code, 0);
    let doc: Value = serde_json::from_str(&dump).unwrap();
    assert!(doc.get("amenbo_export").is_some(), "has the whole-device envelope: {dump}");
    // One DB means no per-store envelope — the tables sit directly in the document.
    assert!(doc["stores"].is_null(), "no per-store envelope: {dump}");
    let titles: Vec<&str> = doc["tables"]["task"].as_array().unwrap().iter()
        .map(|t| t["title"].as_str().unwrap()).collect();
    assert!(["下書き", "推敲"].iter().all(|t| titles.contains(t)), "lists the task rows plainly: {titles:?}");
}

/// There is no ingest surface: `amenbo import` **does not exist as a command** (usage error). No path
/// for adding rows from the outside quietly survives.
#[test]
fn import_is_retired() {
    let cli = Cli::new();
    let (err, code) = cli.run_err(&["import", "--from", "whatever.json"]);
    assert_eq!(code, 2, "unknown subcommand = usage error: {err}");
}

/// `--out <dir>` writes the whole device into an **export directory** — `export.json` (the same content
/// as the stdout stream) plus an `attachments/` tree holding the attachment **bytes**. With nothing to
/// read a dump back in, an export only counts as taking your data with you once the bytes come too. A
/// row names where its bytes landed (`export_path`), and that path names what they hung off.
#[test]
fn whole_device_export_writes_a_directory_with_the_attachment_bytes() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "束", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "章立て", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    let src = cli.home.join("見取り図.txt");
    std::fs::write(&src, b"the bytes").unwrap();
    cli.json(&["task", "attach", &tid, src.to_str().unwrap(), "--json"]);

    let out = cli.home.join("dump");
    let res = cli.json(&["export", "--out", out.to_str().unwrap(), "--json"]);
    assert_eq!(res["ok"], true);
    assert_eq!(res["action"], "export");
    assert_eq!(res["attachments"], 1);
    assert_eq!(res["missing"], 0);

    let body = std::fs::read_to_string(out.join("export.json")).unwrap();
    let doc: Value = serde_json::from_str(&body).unwrap();
    assert!(doc.get("amenbo_export").is_some());
    assert!(!doc["tables"]["task"].as_array().unwrap().is_empty());

    let row = &doc["tables"]["attachment"].as_array().unwrap()[0];
    let rel = row["export_path"].as_str().expect("the row names where its own bytes went");
    assert!(rel.contains(&format!("/task-{tid}/")), "the path reads back where it was attached: {rel}");
    assert_eq!(std::fs::read(out.join(rel)).unwrap(), b"the bytes");
}

/// A non-empty destination is refused; the dump is never mixed into someone else's files.
#[test]
fn export_refuses_an_occupied_destination() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "束", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    cli.json(&["task", "add", "--title", "章立て", "--project", &pid, "--json"]);

    let out = cli.home.join("occupied");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("mine.txt"), b"keep me").unwrap();

    let (err, code) = cli.run_err(&["export", "--out", out.to_str().unwrap()]);
    assert_ne!(code, 0, "does not write over something that already exists: {err}");
    assert!(out.join("mine.txt").is_file(), "a refusal touches nothing");
}


/// `amenbo backup <path>` bundles **everything on this device** — the store on this device plus the root
/// overview store — into one verified `.amenbo-backup` archive, and `amenbo restore <path>` destructively
/// replaces those stores from it. Under AMENBO_HOME that is the single base store; the round-trip
/// preserves the base store's task set — the archive replaces whatever the store later held. Plaintext.
#[test]
fn whole_device_backup_restore_round_trips() {
    // Source store: one project and a few tasks.
    let src = Cli::new();
    let p = src.json(&["project", "add", "--name", "資料", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    for title in ["下書き", "推敲", "清書"] {
        src.json(&["task", "add", "--title", title, "--project", &pid, "--json"]);
    }

    // backup: the whole device into a single .amenbo-backup archive (the base store at minimum).
    let archive = src.home.join("backup.amenbo-backup");
    let report = src.json(&["backup", archive.to_str().unwrap(), "--json"]);
    assert!(report["bytes"].as_u64().unwrap() > 0);
    assert!(archive.exists());

    // Add one more task after the archive was taken, so restore has something to replace.
    src.json(&["task", "add", "--title", "消えるタスク", "--project", &pid, "--json"]);

    // restore: bring the whole device back from the archive — the base store rewinds to three tasks.
    let restored = src.json(&["restore", archive.to_str().unwrap(), "--yes", "--json"]);
    assert!(
        restored["previous_saved_to"].as_str().is_some(),
        "the replaced store's previous truth source is set aside"
    );

    // After the restore only the original three are served; the post-archive task was replaced away.
    let in_project = src.json(&["task", "list", "--project", &pid, "--json"]);
    assert_eq!(in_project["count"], 3);
    let all = src.json(&["task", "list", "--json"]);
    assert_eq!(all["count"], 3, "the archive fully replaced the base store's truth source");
    let titles: Vec<&str> = all["tasks"].as_array().unwrap().iter()
        .map(|t| t["title"].as_str().unwrap()).collect();
    assert!(["下書き", "推敲", "清書"].iter().all(|t| titles.contains(t)));
    assert!(!titles.contains(&"消えるタスク"), "the post-archive task is gone");
}

/// Whole-device backup needs an explicit destination path (the archive is a self-placed
/// disaster-recovery file, not a managed rotation). With no path and no `--store`, it fails loudly
/// (exit 2) rather than guessing a location.
#[test]
fn whole_device_backup_requires_a_path() {
    let src = Cli::new();
    let (err, code) = src.run_err(&["backup", "--json"]);
    assert_eq!(code, 2, "missing archive path is a usage error: {err}");
    assert!(err.contains("destination path"), "hint names the missing path: {err}");
}

/// Every task belongs to a project: `task add` without --project is refused (no unnumbered
/// orphan/inbox task), and the error lists existing projects to pick from.
#[test]
fn task_add_requires_project() {
    let cli = Cli::new();
    // Refused even before any project exists (exit 1).
    let (_e0, code0) = cli.run_err(&["task", "add", "--title", "宙ぶらりん", "--json"]);
    assert_eq!(code0, 1, "without --project it is rejected (exit 1)");

    // With projects around, the error lists them by name so one can be picked.
    let p = cli.json(&["project", "add", "--name", "受け皿", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let (err, code) = cli.run_err(&["task", "add", "--title", "宙ぶらりん", "--json"]);
    assert_eq!(code, 1, "even with an existing project, no --project is rejected");
    assert!(err.contains("受け皿"), "the error names the existing project: {err}");

    // With --project it goes through and is numbered inside its project.
    let ok = cli.json(&["task", "add", "--title", "所属あり", "--project", &pid, "--json"]);
    assert_eq!(ok["task"]["title"], "所属あり");
    assert_eq!(id_str(&ok["task"]["placement"]["project"]["id"]), pid);
    // Not a single project-less task was created.
    let all = cli.json(&["task", "list", "--json"]);
    assert_eq!(all["count"], 1, "only the one task with a project was created");
}

#[test]
fn errors_and_exit_codes() {
    let cli = Cli::new();
    // Unknown command → exit 2.
    let (_, code) = cli.run(&["tsak"]);
    assert_eq!(code, 2);
    // Missing id → exit 1.
    let (_, code) = cli.run(&["task", "show", "01ZZZZZZZZZZZZZZZZZZZZZZZZZ"]);
    assert_eq!(code, 1);
    // A destructive op with --json and no --yes → confirmation_required (exit 1).
    let p = cli.json(&["project", "add", "--name", "x", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let (out, code) = cli.run(&["project", "delete", &pid, "--json"]);
    assert_eq!(code, 1);
    assert!(out.is_empty(), "the confirmation error is expected on stderr");
}

#[test]
fn comment_assign_lifecycle() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    // Assign to myself (human): the assignee is a facet — one local store means the only recipient is me.
    let uid = "human";
    let assigned = cli.json(&["task", "assign", &tid, "--to", uid, "--json"]);
    assert_eq!(assigned["task"]["assignee_kind"], "human");
    // The assignee filter narrows by facet token.
    let mine = cli.json(&["task", "list", "--filter", &format!("assignee:{uid}"), "--json"]);
    assert_eq!(mine["count"], 1);
    // `task list --json` carries assignee_kind too, not just `task show`.
    assert_eq!(mine["tasks"][0]["assignee_kind"], "human");
    // unassign clears it.
    let un = cli.json(&["task", "unassign", &tid, "--json"]);
    assert!(un["task"]["assignee_kind"].is_null());

    // comment add shows up in num_comments and in comment list.
    cli.json(&["comment", "add", &tid, "--text", "先方確認待ち", "--json"]);
    let shown = cli.json(&["task", "show", &tid, "--json"]);
    assert_eq!(shown["num_comments"], 1);
    let comments = cli.json(&["comment", "list", &tid, "--json"]);
    assert_eq!(comments["count"], 1);
    assert_eq!(comments["comments"][0]["text"], "先方確認待ち");
    // The author is the current human facet; its name is config.human_name, unset here so the language default `Human` stands.
    assert_eq!(comments["comments"][0]["author"]["name"], "Human");
}

#[test]
fn task_add_delegates_in_one_step() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "委任PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    // One local store: the only delegate is me — my human facet or my AI.
    let uid = "human";

    // --to assigns at creation time (kind defaults to human).
    let t = cli.json(&["task", "add", "--title", "下調べ", "--project", &pid, "--to", uid, "--json"]);
    assert_eq!(t["task"]["assignee_kind"], "human");

    // --to --ai delegates to my AI (kind=ai), folding create→assign into one command.
    let t2 = cli.json(&["task", "add", "--title", "ログ調査", "--project", &pid, "--to", uid, "--ai", "--json"]);
    assert_eq!(t2["task"]["assignee_kind"], "ai");

    // The assignee filter splits by facet (human vs ai).
    let human_mine = cli.json(&["task", "list", "--filter", "assignee:human", "--json"]);
    assert_eq!(human_mine["count"], 1, "one addressed to the human facet");
    let ai_mine = cli.json(&["task", "list", "--filter", "assignee:me-ai", "--json"]);
    assert_eq!(ai_mine["count"], 1, "one addressed to the AI facet");

    // --ai without --to is rejected.
    let (_e, code) = cli.run_err(&["task", "add", "--title", "x", "--project", &pid, "--ai", "--json"]);
    assert_ne!(code, 0, "--ai requires --to");

    // An unresolvable recipient is refused *before* the task is created — no orphan is left behind.
    let before = cli.json(&["task", "list", "--project", &pid, "--json"])["count"].as_i64().unwrap();
    let (_e2, code2) = cli.run_err(&["task", "add", "--title", "孤児", "--project", &pid, "--to", "居ない人", "--json"]);
    assert_ne!(code2, 0);
    let after = cli.json(&["task", "list", "--project", &pid, "--json"])["count"].as_i64().unwrap();
    assert_eq!(before, after, "no task must be created when resolution fails");
}

/// A task lives in exactly one project, and `task move` rehomes it: placement names the new one.
#[test]
fn task_move_rehomes_single_placement() {
    let cli = Cli::new();
    let pa = cli.json(&["project", "add", "--name", "PJ-A", "--json"]);
    let pa_id = id_str(&pa["project"]["id"]);
    let pb = cli.json(&["project", "add", "--name", "PJ-B", "--json"]);
    let pb_id = id_str(&pb["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "横断タスク", "--project", &pa_id, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    // Freshly created, it belongs to PJ-A.
    assert_eq!(id_str(&t["task"]["placement"]["project"]["id"]), pa_id);
    assert_eq!(cli.json(&["task", "list", "--project", &pa_id, "--json"])["count"], 1);

    // After the move it belongs to PJ-B alone: gone from A's listing, present in B's.
    let moved = cli.json(&["task", "move", &tid, "--project", &pb_id, "--json"]);
    assert_eq!(moved["action"], "task.move");
    assert_eq!(id_str(&moved["task"]["placement"]["project"]["id"]), pb_id);
    assert_eq!(cli.json(&["task", "list", "--project", &pa_id, "--json"])["count"], 0);
    assert_eq!(cli.json(&["task", "list", "--project", &pb_id, "--json"])["count"], 1);
    let f = cli.json(&["task", "list", "--filter", &format!("project:{pb_id}"), "--json"]);
    assert_eq!(f["count"], 1);

    // `addto`/`removefrom` do not exist (unknown subcommand → clap exit 2).
    let (_, code) = cli.run(&["task", "addto", &tid, "--project", &pa_id, "--json"]);
    assert_ne!(code, 0, "addto has been removed");
    let (_, code) = cli.run(&["task", "removefrom", &tid, "--project", &pb_id, "--json"]);
    assert_ne!(code, 0, "removefrom has been removed");

    // doctor finds no orphans.
    assert_eq!(cli.json(&["doctor", "--json"])["ok"], true);
}

/// Eight processes `task add` into the same store at once and every write lands. The only serialisation
/// is **SQLite's writer lock** (`BEGIN IMMEDIATE` + `busy_timeout`) — there is no file lock. Waits are
/// per-transaction (milliseconds), so none of the eight is turned away with `store_busy`: assert that
/// **every process exits successfully**, since a count alone reads a dropped writer as one that never wrote.
#[test]
fn concurrent_writers_all_land_without_a_file_lock() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "P", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let mut handles = Vec::new();
    for i in 0..8 {
        let home = cli.home.clone();
        let pid = pid.clone();
        handles.push(std::thread::spawn(move || {
            let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
                .env("AMENBO_HOME", &home)
                .env("AMENBO_ACTOR", "human") // non-interactive writes declare a facet
                .env("AMENBO_UPDATE_CHECK", "0") // no update check (hermetic)
                .args(["task", "add", "--title", &format!("t{i}"), "--project", &pid])
                .output()
                .expect("task add");
            assert!(
                out.status.success(),
                "concurrent writer {i} must not be turned away: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let list = cli.json(&["task", "list", "--project", &pid, "--json"]);
    assert_eq!(list["count"], 8, "concurrent writers must not lose updates");
}

/// Copying a store to another machine (a different AMENBO_HW_ID) is caught as a clone: bound_hw is
/// rebound to the current machine and stderr says so. Only the hardware binding is rewritten, and the
/// rebind persists, so reopening on that machine never reports again.
#[test]
fn r6_clone_to_new_machine_rebinds_hardware() {
    let home_a = temp_home();
    let home_b = temp_home();
    // The runs below pin the CWD to home, so home_a must exist before the first one.
    std::fs::create_dir_all(&home_a).unwrap();
    let run = |home: &std::path::Path, hw: &str, args: &[&str]| -> (Value, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", home)
            .env("AMENBO_HW_ID", hw)
            .env("AMENBO_ACTOR", "human") // writes such as init declare a facet
            .env("AMENBO_UPDATE_CHECK", "0") // no update check (hermetic)
            // Isolate the CWD into the temp home as well (as the other helpers do). Without it, init writes
            // `.amenbo`/AGENTS.md/CLAUDE.md into the test binary's CWD (crates/amenbo-cli) and dirties both the
            // source tree and the real app-data.
            .current_dir(home)
            .args(args)
            .output()
            .expect("run amenbo");
        (serde_json::from_slice(&out.stdout).unwrap_or(Value::Null), String::from_utf8_lossy(&out.stderr).into_owned())
    };
    run(&home_a, "machine-1", &["init", "--name", "Alice"]);
    let (who_a, _) = run(&home_a, "machine-1", &["whoami", "--json"]);
    // whoami carries no account/replica dimension (`replica_id`/`user_id`); the hardware binding remains.
    assert!(who_a.get("replica_id").is_none(), "replica_id is retired");
    assert!(who_a.get("user_id").is_none(), "user_id is retired");
    assert_eq!(who_a["bound_hw"].as_str().unwrap(), "machine-1", "bound to the first machine");
    assert_eq!(who_a["hw_mismatch"], false, "same machine — no mismatch");

    // Clone it, as a restore onto another machine would: the identity file sits plainly in the base directory and travels along.
    std::fs::create_dir_all(&home_b).unwrap();
    for f in ["store.sqlite", "config.json", "identity.json"] {
        let src = home_a.join(f);
        if src.exists() {
            std::fs::copy(&src, home_b.join(f)).unwrap();
        }
    }

    // The first open on the other machine detects the clone, rebinds bound_hw to machine-2 and reports it.
    let (who_b, stderr_b) = run(&home_b, "machine-2", &["whoami", "--json"]);
    assert!(stderr_b.contains("copied to a different machine"), "the clone is detected and reported: {stderr_b}");
    assert_eq!(who_b["bound_hw"].as_str().unwrap(), "machine-2", "bound_hw is rebound to the new machine");
    assert!(who_b.get("replica_id").is_none(), "still no replica dimension after the clone");
    // Reopening there is already rebound, so it never reports again — the rebind persisted.
    let (_who_b2, stderr_b2) = run(&home_b, "machine-2", &["whoami", "--json"]);
    assert!(!stderr_b2.contains("copied to a different machine"), "no re-report on the same machine: {stderr_b2}");
}

/// `--json` on `version` / `doctor` / `agent` surfaces the store's `format_version` and the highest format
/// this build can open. There is no per-surface `last_*_version`. Update availability comes only from the
/// upstream `latest.json`, which the e2e harness disables, so it never fires here.
#[test]
fn version_and_format_state_are_visible() {
    let cli = Cli::new();
    let v = cli.json(&["version", "--json"]);
    assert!(v["format_version"].as_i64().unwrap() >= 1, "after the guard, format is v1 or higher: {v}");
    assert!(v["max_supported_format"].as_i64().unwrap() >= 1, "the max supported format is shown: {v}");
    assert!(v.get("last_cli_version").is_none(), "the per-surface versions are gone: {v}");
    assert!(v.get("last_gui_version").is_none(), "the per-surface versions are gone: {v}");
    assert_eq!(v["update_available"], false, "update check disabled = no update: {v}");

    // doctor --json carries version_status too, and does not count it as an issue.
    let d = cli.json(&["doctor", "--json"]);
    assert_eq!(d["ok"], true, "a plain store has zero orphans: {d}");
    assert!(d["version_status"]["format_version"].as_i64().unwrap() >= 1, "doctor surfaces the format version: {d}");
    assert!(d["version_status"].get("last_cli_version").is_none(), "doctor: no per-surface versions: {d}");

    // agent --json exposes the same state under store_status.
    let a = cli.json(&["agent", "--json"]);
    assert!(a["store_status"]["format_version"].as_i64().unwrap() >= 1, "agent surfaces the format version: {a}");
    assert!(a["store_status"].get("last_gui_version").is_none(), "agent: no per-surface versions: {a}");
}

#[test]
fn agent_json_is_self_describing() {
    let cli = Cli::new();
    let a = cli.json(&["agent", "--json"]);
    assert_eq!(a["version"], env!("CARGO_PKG_VERSION"));
    assert!(a["commands"].as_array().unwrap().len() >= 20);
    // amenbo is one local store: the spec is single-shaped (personal mode), and no sharing / sync / key /
    // multi-device surface appears anywhere.
    assert_eq!(a["mode"], "personal");
    assert!(!a["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "sync" || c == "peer list" || c == "join"));
    // The core task commands are still there.
    assert!(a["commands"].as_array().unwrap().iter().any(|c| c == "task add"));
    // No networking or sharing roadmap (iroh/DHT/relay/key revocation) leaks into the spec.
    assert!(!serde_json::to_string(&a).unwrap().to_lowercase().contains("iroh"));
    // capabilities are intent-shaped, and decision records show up as one of them.
    let caps = a["capabilities"].as_array().expect("capabilities is an array");
    assert!(!caps.is_empty());
    assert!(caps
        .iter()
        .any(|c| c["commands"].as_array().is_some_and(|cs| cs.iter().any(|n| n == "decision add"))));
    // Every command a capability points at exists in commands — catches typos and omissions.
    let known: std::collections::HashSet<&str> =
        a["commands"].as_array().unwrap().iter().filter_map(|c| c.as_str()).collect();
    for c in caps {
        for n in c["commands"].as_array().expect("capability.commands is an array") {
            assert!(known.contains(n.as_str().unwrap_or("")), "capability references unknown command: {n}");
        }
    }
}

/// `--store` — neither the global override nor `bind --store` — exists **anywhere on the command
/// surface**: there is no store-id space at all. Working from outside a folder, or with none, is what
/// `--project` is for: naming a project on this one device.
#[test]
fn the_store_axis_is_gone_from_the_binding_surface() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    for args in [
        vec!["bind", "--store", "01SOMEID", "--json"],
        vec!["--store", "01SOMEID", "task", "list", "--json"],
    ] {
        let (err, code) = cli.run_err(&args);
        assert_eq!(code, 2, "{args:?} should exit 2 as an unknown flag: {err}");
        assert!(err.contains("unknown_command"), "{args:?} is unknown_command: {err}");
    }

    // Many-to-one binding still lives on `bind --project`: several folders may point at one project.
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let bound = cli.json(&["bind", "--project", &pid, "--json"]);
    assert_eq!(id_str(&bound["binding"]["project_id"]), pid, "bind --project still works");
}

/// `amenbo --project <name|id> …` (before the subcommand, like `git -C`) outranks the `.amenbo` binding and
/// replaces the effective project context. Numbers are globally unique on a device, so it does not change
/// ref resolution, but it does pick the default project for `decision add` — both are checked here.
#[test]
fn global_project_override_drives_defaults() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let aid = id_str(&cli.json(&["project", "add", "--name", "Alpha", "--json"])["project"]["id"]);
    cli.json(&["project", "add", "--name", "Beta", "--json"]);
    // Numbering runs 1.. across the store (alpha-1 → #1, beta-1 → #2). Exercise id and name resolution both.
    cli.json(&["task", "add", "--title", "alpha-1", "--project", &aid, "--json"]);
    cli.json(&["task", "add", "--title", "beta-1", "--project", "Beta", "--json"]);

    // `#n` is globally unique: the same number names the same task whatever the --project context.
    for ctx in ["Alpha", "Beta"] {
        assert_eq!(cli.json(&["--project", ctx, "task", "show", "1", "--json"])["title"], "alpha-1");
        assert_eq!(cli.json(&["--project", ctx, "task", "show", "2", "--json"])["title"], "beta-1");
    }

    // The default project of decision add follows --project, so a decision can be filed with no folder at
    // all. Read the new decision back and check it landed in Beta — proof the default took.
    let d = cli.json(&["--project", "Beta", "decision", "add", "--title", "beta-dec", "--json"]);
    assert_eq!(d["action"], "decision.add");
    let did = id_str(&d["decision"]["id"]);
    assert_eq!(cli.json(&["decision", "show", &did, "--json"])["project"]["name"], "Beta");

    // An unknown project fails loud rather than being guessed at.
    let (err, code) = cli.run_err(&["--project", "NoSuch", "decision", "add", "--title", "x", "--json"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("not_found"), "an unknown project is not_found: {err}");
}

#[test]
fn personal_mode_has_no_sharing_commands() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // Sharing commands (peer/device/key) do not exist for a single local store: clap turns them away as
    // unknown subcommands (exit 2, unknown_command) rather than refusing them at run time.
    for args in [
        vec!["peer", "list", "--json"],
        vec!["device", "list", "--json"],
        vec!["key", "status", "--json"],
    ] {
        let (stderr, code) = cli.run_err(&args);
        assert_eq!(code, 2, "{args:?} should exit 2 as an unknown subcommand: {stderr}");
        assert!(
            stderr.contains("unknown_command"),
            "{args:?} should return unknown_command: {stderr}"
        );
    }
    // The core personal-mode operation — tasks — works as usual.
    let pid = cli.a_project();
    let t = cli.json(&["task", "add", "--title", "personal-ok", "--project", &pid, "--json"]);
    assert!(t["task"]["id"].is_number(), "id is an integer key");
}

/// Strict execution guard: in a bare directory with no `.amenbo` pointer and no AMENBO_HOME /
/// AMENBO_PROJECT_DIR, the CLI does not quietly create a default root store — it stops and tells you to
/// init. It covers every surface that opens a store (here, status); version and update open none, an
/// exception pinned by `version_and_update_answer_without_a_pointer`. app-data is never touched.
#[test]
fn execution_guard_requires_pointer_when_unbound() {
    let dir = temp_home();
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env_remove("AMENBO_HOME")
        .env_remove("AMENBO_PROJECT_DIR")
        .env("AMENBO_UPDATE_CHECK", "0") // no update check (hermetic)
        // Declare the facet too: this watches the human-side guard. An inherited AMENBO_ACTOR=ai would instead
        // trip the guard that cuts an AI off in an unbound CWD, which fires first.
        .env("AMENBO_ACTOR", "human")
        .current_dir(&dir)
        .args(["status", "--json"])
        .output()
        .expect("failed to run the binary");
    assert_eq!(out.status.code(), Some(1), "an unbound plain dir exits 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "no_pointer", "stops with no_pointer: {stderr}");
    // The hint offers two branches — init a new project, or bind an existing one — and none for a store.
    let hint = v["error"]["hint"].as_str().unwrap_or("");
    assert!(hint.contains("amenbo init --name"), "hint offers new-project branch: {hint}");
    assert!(hint.contains("amenbo bind --project"), "hint offers link-project branch: {hint}");
    assert!(!hint.contains("--store"), "the retired store axis is not offered: {hint}");

    // Exception: an explicit AMENBO_HOME names the store to open, so a bare dir goes through. The temp
    // home keeps app-data clean.
    let home = temp_home();
    std::fs::create_dir_all(&home).unwrap();
    let out2 = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &home)
        .env_remove("AMENBO_PROJECT_DIR")
        .env("AMENBO_UPDATE_CHECK", "0") // no update check (hermetic)
        .env("AMENBO_ACTOR", "human") // as above (an AI would be cut off for want of a binding — a different guard)
        .current_dir(&dir)
        .args(["status", "--json"])
        .output()
        .expect("failed to run the binary");
    assert_eq!(out2.status.code(), Some(0), "an explicit AMENBO_HOME passes the guard exception");
}

/// Nested-worktree guard: a git worktree cut **inside** a bound folder inherits its `.amenbo` by the upward
/// walk, so a throwaway checkout could drive the real backlog — the store it writes to lives in app-data and
/// outlives the worktree. amenbo refuses there, and says where to operate instead without ever offering
/// `bind`, which is neither a way out (restoring the binding in the throwaway is the accident itself) nor a
/// way through: binding is refused in a nested worktree as squarely as operating is, `--force` and all. The
/// one command that stays open is `unbind`, the way out. The shapes that merely resemble the hazard — an
/// ordinary subdirectory, a submodule — go through untouched.
#[test]
fn nested_worktree_is_refused_but_a_subdirectory_and_a_submodule_are_not() {
    let cli = Cli::new();
    // Bind the folder: `init` drops the `.amenbo` the nested checkout would inherit.
    cli.run(&["init", "--name", "Alice"]);

    let run_args_in = |dir: &std::path::Path, args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env("AMENBO_ACTOR", "human")
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("failed to run the binary");
        (out.status.code(), String::from_utf8_lossy(&out.stderr).to_string())
    };
    let run_in = |dir: &std::path::Path| run_args_in(dir, &["status", "--json"]);

    // A worktree nested under the bound folder: refused.
    let wt = cli.home.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    std::fs::write(wt.join(".git"), format!("gitdir: {}/.git/worktrees/wt\n", cli.home.display())).unwrap();
    let (code, stderr) = run_in(&wt);
    assert_eq!(code, Some(1), "a nested worktree exits 1: {stderr}");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "nested_worktree", "refused as nested_worktree: {stderr}");
    // The refusal names where to operate instead, and never offers to bind the throwaway.
    let hint = v["error"]["hint"].as_str().unwrap_or("");
    assert!(hint.contains(&cli.home.to_string_lossy().to_string()), "the hint names the project folder: {hint}");
    assert!(!hint.contains("amenbo bind"), "restoring the binding here is not offered: {hint}");

    // The guard fires from inside the worktree too, not only at its root.
    let deep = wt.join("crates");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(run_in(&deep).0, Some(1), "a subdirectory of the worktree is refused as well");

    // An ordinary subdirectory of the bound folder is ordinary work.
    let sub = cli.home.join("crates");
    std::fs::create_dir_all(&sub).unwrap();
    assert_eq!(run_in(&sub).0, Some(0), "a plain subdirectory goes through");

    // A submodule shares the "`.git` is a file" shape, and must not be caught by it.
    let sub_mod = cli.home.join("vendor").join("lib");
    std::fs::create_dir_all(&sub_mod).unwrap();
    std::fs::write(sub_mod.join(".git"), "gitdir: ../../.git/modules/vendor/lib\n").unwrap();
    let (code, stderr) = run_in(&sub_mod);
    assert_eq!(code, Some(0), "a submodule is not a throwaway worktree: {stderr}");

    // `bind --force` does not write the throwaway a pointer of its own: binding is held to the guard as much
    // as operating is, and `--force` — which means "overwrite the pointer already there" — is not a passport
    // to it. A pointer here could only ever be meaningless, since every command that would read it is refused.
    let project_id = cli.json(&["project", "list", "--json"])["projects"][0]["id"].to_string();
    let (code, stderr) = run_args_in(&wt, &["bind", "--project", &project_id, "--force", "--json"]);
    assert_eq!(code, Some(1), "binding a nested worktree is refused: {stderr}");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "nested_worktree", "and refused as the same thing: {stderr}");
    assert!(!wt.join(".amenbo").is_file(), "no pointer was left behind");
    assert!(!wt.join("CLAUDE.md").exists(), "and no managed block was written into the checkout");

    // The guard judges the folder that would receive the pointer, not the one the command was typed in — so
    // reaching into the worktree with `--dir` from the project folder is refused just the same.
    let (code, stderr) = run_args_in(
        &cli.home,
        &["bind", "--project", &project_id, "--dir", &wt.to_string_lossy(), "--force", "--json"],
    );
    assert_eq!(code, Some(1), "binding it from outside by --dir is refused too: {stderr}");
    assert!(!wt.join(".amenbo").is_file(), "and still leaves no pointer");

    // `init --force` is refused before it runs, and that is the sharpest edge of the three: it raises a
    // project in the store, which lives in app-data and would outlast the checkout that asked for it — no
    // `git worktree remove` takes that back.
    let before = cli.json(&["project", "list", "--json"])["projects"].as_array().unwrap().len();
    let (code, stderr) = run_args_in(&wt, &["init", "--name", "Bob", "--force", "--json"]);
    assert_eq!(code, Some(1), "init in a nested worktree is refused: {stderr}");
    let after = cli.json(&["project", "list", "--json"])["projects"].as_array().unwrap().len();
    assert_eq!(after, before, "and raised no project in the store");

    // `unbind` is the way out, so it stays open: refusing it would strand a pointer an older build wrote with
    // nothing but a text editor to remove it. There is none to remove here, and saying so is itself the proof
    // that it ran rather than being turned away at the guard.
    std::fs::write(wt.join(".amenbo"), "{}").unwrap();
    let (code, stderr) = run_args_in(&wt, &["unbind", "--yes", "--json"]);
    assert_eq!(code, Some(0), "unbind is not held to the guard: {stderr}");
    assert!(!wt.join(".amenbo").is_file(), "and it removed the pointer");
}

/// The agent spec advises a worktree per task — advice that means nothing to someone tracking no VCS, so it
/// never reaches them: with the bound folder outside any git checkout the `worktree` cycle is absent from
/// the output entirely, not present with a caveat. Nothing else in the spec moves either way.
#[test]
fn the_worktree_cycle_reaches_only_a_bound_folder_under_git() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "Alice"]);

    let cycles_of = |v: &Value| v["cycles"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();

    // No git anywhere above the bound folder: the cycle is gone, and so is the byte it would have cost.
    let plain = cli.json(&["agent", "--json"]);
    let without = cycles_of(&plain);
    assert!(!without.contains(&"worktree".to_string()), "no git, no worktree cycle: {without:?}");
    assert!(without.contains(&"commit".to_string()), "the rest of the spec is untouched: {without:?}");

    // Put the folder under git, and the advice arrives — including the step that says to cut one per task.
    std::fs::create_dir_all(cli.home.join(".git")).unwrap();
    let under_git = cli.json(&["agent", "--json"]);
    let with = cycles_of(&under_git);
    assert!(with.contains(&"worktree".to_string()), "under git, the cycle is served: {with:?}");
    let backbone = under_git["cycles"]["worktree"]["backbone"].as_array().unwrap();
    assert_eq!(backbone.len(), 3, "all three steps come through: {backbone:?}");
    assert_eq!(
        without.len() + 1,
        with.len(),
        "and the gate moves this one cycle only: {without:?} vs {with:?}"
    );
}

/// `version` and `update` answer even in a bare, unbound dir — neither ever reads a store (version is a
/// fact about this build; update turns the published latest.json into an installer URL for this OS). With
/// no self-update, `update` is the only road for someone who wants a newer build, and answering "init
/// first" would shut out exactly the people stuck on an old one or fresh off an install. Reach already
/// confines the AI, so these two need no second gate by location — hence the ai facet here.
#[test]
fn version_and_update_answer_without_a_pointer() {
    let dir = temp_home();
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env_remove("AMENBO_HOME")
            .env_remove("AMENBO_PROJECT_DIR")
            .env("AMENBO_UPDATE_CHECK", "0") // no upstream lookup (hermetic)
            .env("AMENBO_ACTOR", "ai")
            .current_dir(&dir)
            .args(args)
            .output()
            .expect("failed to run the binary");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        (out.status.code(), stdout)
    };

    let (code, stdout) = run(&["version", "--json"]);
    assert_eq!(code, Some(0), "version answers even without a binding: {stdout}");
    let v: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|_| panic!("version JSON: {stdout}"));
    assert!(v["version"].is_string(), "it names this build's version: {stdout}");
    assert!(v["max_supported_format"].is_number(), "the max openable format version is a fact of the build: {stdout}");
    // Proof no store was opened: a number here would mean the bare dir opened — that is, silently created — one.
    assert!(v["format_version"].is_null(), "it does not name store-derived facts: {stdout}");

    let (code, stdout) = run(&["update", "--print", "--json"]);
    assert_eq!(code, Some(0), "update answers even without a binding: {stdout}");
    let v: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|_| panic!("update JSON: {stdout}"));
    assert!(v["url"].as_str().is_some_and(|u| u.starts_with("https://")), "returns the installer link: {stdout}");
    assert_eq!(v["opened"], false, "--print does not open, it only prints the URL: {stdout}");
}

/// Clobber guard: re-initing a folder that already carries `.amenbo` would create a new store and
/// overwrite the pointer unasked, orphaning the old one. Refused by default; only `--force` allows it.
#[test]
fn init_refuses_to_clobber_existing_pointer() {
    let cli = Cli::new();
    // 1) The first init succeeds and drops .amenbo.
    let (_out, code) = cli.run(&["init", "--name", "山田"]);
    assert_eq!(code, 0, "the first init succeeds");
    assert!(cli.home.join(".amenbo").exists(), ".amenbo is placed");

    // 2) A second init in the same folder is refused with init_pointer_exists.
    let (stderr, ecode) = cli.run_err(&["init", "--name", "佐藤", "--json"]);
    assert_eq!(ecode, 1, "init over an existing pointer exits 1");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "init_pointer_exists", "stops at the clobber guard: {stderr}");

    // 3) --force says it out loud and slips past the clobber guard: no init_pointer_exists.
    //    In the real multi-store app-data layout that creates a new store and overwrites the pointer; under
    //    the test's single-store AMENBO_HOME there is only ever one store at the root, so it reports
    //    "already initialised". Either way the guard itself was passed, and that is what is checked.
    let (stderr3, _fcode) = cli.run_err(&["init", "--name", "佐藤", "--force", "--json"]);
    assert!(
        !stderr3.contains("init_pointer_exists"),
        "--force bypasses the clobber guard: {stderr3}"
    );
}

#[test]
fn assign_is_plain_reassignment() {
    // Assignment is plain reassignment: no special state, no mandatory-reason gate.
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let t = id_str(&cli.json(&["task", "add", "--title", "asg", "--project", &pid, "--json"])["task"]["id"]);

    // The human delegates to the AI → assignee_kind=ai.
    let a = cli.json(&["task", "assign", &t, "--to", "tester", "--ai", "--json"]);
    assert_eq!(a["task"]["assignee_kind"], "ai");

    // The AI hands it back (ai→human) with no reason required.
    let h = cli.json(&["task", "assign", &t, "--to", "tester", "--actor", "ai", "--json"]);
    assert_eq!(h["task"]["assignee_kind"], "human");

    // Reassigning to the same assignee/kind is an idempotent no-op.
    let again = cli.json(&["task", "assign", &t, "--to", "tester", "--actor", "ai", "--json"]);
    assert_eq!(again["noop"], true, "an identical assignment is a noop");
}

#[test]
fn status_transitions_and_completed_stays_in_sync() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let t = cli.json(&["task", "add", "--title", "S3", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    // A new task starts as todo.
    assert_eq!(cli.json(&["task", "show", &tid, "--json"])["status"], "todo");

    // in_progress leaves completed false.
    let ip = cli.json(&["task", "status", &tid, "in_progress", "--json"]);
    assert_eq!(ip["task"]["status"], "in_progress");
    assert_eq!(ip["task"]["completed"], false);

    // The done sugar sets status=done and completed=true.
    let done = cli.json(&["task", "done", &tid, "--json"]);
    assert_eq!(done["task"]["status"], "done");
    assert_eq!(done["task"]["completed"], true);

    // The reopen sugar sets status=todo and completed=false.
    let re = cli.json(&["task", "reopen", &tid, "--json"]);
    assert_eq!(re["task"]["status"], "todo");
    assert_eq!(re["task"]["completed"], false);

    // block --reason records the reason as a comment.
    let blk = cli.json(&["task", "block", &tid, "--reason", "先方確認待ち", "--json"]);
    assert_eq!(blk["task"]["status"], "blocked");
    let comments = cli.json(&["comment", "list", &tid, "--json"]);
    assert!(comments["comments"].as_array().unwrap().iter().any(|c| c["text"] == "先方確認待ち"));

    // Setting the status it already holds is an idempotent no-op.
    let noop = cli.json(&["task", "status", &tid, "blocked", "--json"]);
    assert_eq!(noop["noop"], true);

    // An invalid status is invalid_value (exit 2).
    let (_o, code) = cli.run(&["task", "status", &tid, "frozen"]);
    assert_eq!(code, 2);
}

#[test]
fn init_places_marker_files_and_bind_links_project() {
    let cli = Cli::new();
    // init drops .amenbo plus the managed blocks of AGENTS.md and CLAUDE.md into the CWD (the isolated home).
    let init = cli.json(&["init", "--name", "tester", "--json"]);
    // The display name lives under the config facet key — one key name, and `whoami` reports the same value.
    assert!(init["identity"].get("user_name").is_none(), "user_name was renamed to human_name");
    assert_eq!(init["identity"]["human_name"], "tester", "init's display name is config.human_name");
    let placed: Vec<String> = init["identity"]["placed"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(placed.contains(&".amenbo".to_string()));
    assert!(placed.contains(&"AGENTS.md".to_string()));
    assert!(placed.contains(&"CLAUDE.md".to_string()));
    assert!(cli.home.join(".amenbo").is_file());
    for f in ["AGENTS.md", "CLAUDE.md"] {
        let body = std::fs::read_to_string(cli.home.join(f)).unwrap();
        assert!(body.contains("<!-- amenbo:begin (managed v2) -->"), "{f} has the versioned managed marker");
        assert!(body.contains("<!-- amenbo:end -->"), "{f} has the end marker");
        assert!(body.contains("agent --json"), "{f} points at agent --json");
    }

    // init creates an initial project named after the folder and binds the folder to it, so a project_id is
    // there from the start — the same shape as the GUI's "new project from this folder".
    let before = cli.json(&["bind", "--json"]);
    assert!(before["binding"]["project_id"].is_number(), "init auto-creates and binds a project: {before}");
    assert!(
        before["binding"]["project_name"].as_str().is_some_and(|n| !n.is_empty()),
        "the auto-created project is named after the folder: {before}"
    );

    // The same folder can be re-bound to another project: the ancestor search excludes self, so re-binding
    // your own folder does not trip the nested guard.
    let pid = id_str(&cli.json(&["project", "add", "--name", "サイト刷新", "--json"])["project"]["id"]);
    let bound = cli.json(&["bind", "--project", &pid, "--json"]);
    assert_eq!(id_str(&bound["binding"]["project_id"]), pid);

    // The display shows the project it was rebound to.
    let after = cli.json(&["bind", "--json"]);
    assert_eq!(id_str(&after["binding"]["project_id"]), pid);
    assert_eq!(after["binding"]["project_name"], "サイト刷新");
}

/// Init in a folder that has no `.amenbo` but does carry amenbo's managed block in CLAUDE.md is **not
/// hard-blocked by the marker alone**. When no living store claims that cwd — a clone, a copy, a leftover
/// stale marker — init proceeds: it writes `.amenbo`, regenerates the block idempotently and preserves
/// everything outside the markers (under a single AMENBO_HOME that claim set is always empty).
#[test]
fn init_proceeds_despite_a_stale_managed_block_without_a_living_owner() {
    let cli = Cli::new();
    let claude = cli.home.join("CLAUDE.md");
    // Simulate a clone: no `.amenbo`, but a managed block already there, plus user prose outside the markers.
    let planted = "# CLAUDE.md\n\nUser guidance that must survive (Class P).\n\n<!-- amenbo:begin (managed v2) -->\nstale block content carried in from a clone\n<!-- amenbo:end -->\n";
    std::fs::write(&claude, planted).unwrap();
    assert!(!cli.home.join(".amenbo").is_file(), "premise: there is no `.amenbo` yet");

    let init = cli.json(&["init", "--name", "tester", "--json"]);
    let placed: Vec<String> = init["identity"]["placed"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(placed.contains(&".amenbo".to_string()), "init proceeds and places `.amenbo`: {init}");
    assert!(cli.home.join(".amenbo").is_file(), "`.amenbo` is restored");

    // The block is regenerated at the current version (agent pointer and all) and the prose outside survives.
    let body = std::fs::read_to_string(&claude).unwrap();
    assert!(body.contains("User guidance that must survive (Class P)."), "preserves Class P outside the markers: {body}");
    assert!(body.contains("<!-- amenbo:begin (managed v2) -->") && body.contains("<!-- amenbo:end -->"), "the block markers remain");
    assert!(body.contains("agent --json"), "the block is regenerated to the current version: {body}");
    assert!(!body.contains("stale block content carried in from a clone"), "stale content between the markers is replaced with the current version: {body}");
}

/// Write the block back to the old, unversioned `(managed)` marker: a block left stale on disk by an upgrade.
fn make_block_stale(path: &std::path::Path) -> String {
    let before = std::fs::read_to_string(path).unwrap();
    let stale = before.replace("<!-- amenbo:begin (managed v2) -->", "<!-- amenbo:begin (managed) -->");
    assert_ne!(before, stale, "the downgrade actually changes the markers");
    std::fs::write(path, &stale).unwrap();
    stale
}

/// A stale block in a bound folder catches up to the current version **just by running amenbo there**, no
/// matter who runs it (actor-independent). Afterwards doctor no longer calls that folder stale.
#[test]
fn running_amenbo_in_a_bound_folder_follows_its_stale_managed_block() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]); // places the current (v2) block and registers this folder as bound
    let claude = cli.home.join("CLAUDE.md");
    make_block_stale(&claude);

    // Run amenbo in this folder — any command will do, so long as it resolves `.amenbo`.
    cli.run(&["status"]);

    let after = std::fs::read_to_string(&claude).unwrap();
    assert!(after.contains("<!-- amenbo:begin (managed v2) -->"), "just launching follows to the current version: {after}");
    assert!(!after.contains("(managed) -->"), "the old markers do not remain: {after}");
    // Having caught up, the folder is not reported as stale.
    let doctor = cli.json(&["doctor", "--json"]);
    assert!(
        !doctor["issues"].as_array().unwrap().iter().any(|i| i["kind"] == "stale_managed_block"),
        "a followed folder is not flagged stale: {doctor}",
    );
}

/// A body that tells the next reader to go and read something that is not there. The whole reason the
/// check exists is that a dead pointer fails **silently**: an agent's pre-flight reads the notes, follows
/// the ref, finds nothing, and cannot tell "deleted" from "I looked in the wrong place". Deletion is
/// physical, so nothing else in the store remembers the number ever meant anything.
#[test]
fn doctor_reports_a_body_pointing_at_a_ref_that_resolves_to_nothing() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let dead = |v: &Value| -> Vec<Value> {
        v["issues"].as_array().unwrap().iter().filter(|i| i["kind"] == "dead_ref").cloned().collect()
    };
    assert!(dead(&cli.json(&["doctor", "--json"])).is_empty(), "an empty store points at nothing");

    // Two tasks, and then one of them is deleted — leaving the other's notes pointing at a number that no
    // longer names anything.
    let pid = id_str(&cli.json(&["project", "add", "--name", "Alpha", "--json"])["project"]["id"]);
    let live = id_str(&cli.json(&["task", "add", "--title", "Live", "--project", &pid, "--json"])["task"]["id"]);
    let doomed =
        id_str(&cli.json(&["task", "add", "--title", "Doomed", "--project", &pid, "--json"])["task"]["id"]);
    cli.run(&[
        "task",
        "update",
        &live,
        "--notes",
        &format!("blocked on AMB-T-{doomed}; the spelling is `AMB-T-{doomed}`"),
    ]);
    assert!(dead(&cli.json(&["doctor", "--json"])).is_empty(), "while it resolves, nothing is raised");

    cli.run(&["task", "delete", &doomed, "--yes"]);
    let issues = dead(&cli.json(&["doctor", "--json"]));
    assert_eq!(issues.len(), 1, "one issue for the one body that points at it: {issues:?}");
    let issue = &issues[0];
    assert_eq!(issue["target"], format!("task:{live}"));
    assert_eq!(issue["severity"], "warning", "the row is intact; a sentence in it has rotted");
    assert_eq!(
        issue["params"]["refs"],
        format!("AMB-T-{doomed}"),
        "the code span shows the form and points at nothing, so it is not named twice: {issue}",
    );
    assert!(
        issue["message"].as_str().unwrap().contains(&format!("AMB-T-{doomed}")),
        "the sentence quotes the ref that died: {issue}",
    );
    // It is a report, not a gate: nothing was rewritten and the store's verdict stands.
    assert!(
        cli.json(&["task", "show", &live, "--json"])["notes"]
            .as_str()
            .unwrap()
            .contains(&format!("`AMB-T-{doomed}`")),
        "the body is left exactly as it was",
    );
    assert_eq!(cli.json(&["doctor", "--json"])["ok"], true, "a dead ref does not fail the store");
}

/// What doctor covers is the folders automatic follow-up never reaches — bound folders you are not in —
/// where it detects a stale block **without rewriting it** and says how to fix it.
#[test]
fn doctor_flags_a_stale_block_in_a_folder_you_are_not_in_without_rewriting_it() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let claude = cli.home.join("CLAUDE.md");

    // A current block raises no stale_managed_block.
    let has_stale = |v: &Value| {
        v["issues"].as_array().unwrap().iter().any(|i| i["kind"] == "stale_managed_block")
    };
    assert!(!has_stale(&cli.json(&["doctor", "--json"])), "does not flag a current-version block");

    let stale = make_block_stale(&claude);

    // Run doctor from **outside** the bound folder: follow-up never runs there, and that gap is doctor's job.
    let outside = temp_home();
    std::fs::create_dir_all(&outside).unwrap();
    let flagged = cli.json_from(&outside, &["doctor", "--json"]);
    let issue = flagged["issues"].as_array().unwrap().iter()
        .find(|i| i["kind"] == "stale_managed_block")
        .unwrap_or_else(|| panic!("should detect stale_managed_block: {flagged}"));
    assert_eq!(issue["severity"], "warning");
    assert!(issue["target"].as_str().unwrap().ends_with("CLAUDE.md"), "the target is CLAUDE.md: {issue}");
    // Two ways out are offered: run amenbo in that folder, or sync-guide every folder.
    assert!(issue["fix_hint"].as_str().unwrap().contains("sync-guide"), "points at the re-sync command: {issue}");
    // It is a warning and not an error, so doctor stays ok.
    assert_eq!(flagged["summary"]["error"], 0, "a stale block is a warning, not an error: {flagged}");

    // **doctor never rewrites** — detection has no side effects. Only follow-up and `sync-guide` write.
    assert_eq!(std::fs::read_to_string(&claude).unwrap(), stale, "doctor does not rewrite the file");
}

/// Once bound, the human output of `status`/`whoami` opens with "Project: <name>  (this folder: <path>)",
/// quietly restating where you are on every command. An unbound folder prints no such header.
#[test]
fn status_and_whoami_begin_with_project_location_header() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = id_str(&cli.json(&["project", "add", "--name", "サイト刷新", "--json"])["project"]["id"]);
    cli.run(&["bind", "--project", &pid]);

    // The child's getcwd resolves symlinks (on macOS /var → /private/var), so canonicalize the expected
    // path to match.
    let home = std::fs::canonicalize(&cli.home).unwrap().to_string_lossy().to_string();
    let expected = format!("Project: サイト刷新  (this folder: {home})");

    let (status_out, code) = cli.run(&["status"]);
    assert_eq!(code, 0);
    assert!(status_out.lines().next() == Some(expected.as_str()),
        "status begins with the location header: {status_out}");

    let (whoami_out, code) = cli.run(&["whoami"]);
    assert_eq!(code, 0);
    assert!(whoami_out.lines().next() == Some(expected.as_str()),
        "whoami begins with the location header: {whoami_out}");
}

/// `bind --dir <path>` places `.amenbo` in the named existing folder rather than the CWD, so a project can
/// be linked from elsewhere. A `--dir` that does not exist is rejected.
#[test]
fn bind_dir_places_pointer_in_external_folder() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = id_str(&cli.json(&["project", "add", "--name", "外部PJ", "--json"])["project"]["id"]);

    // Make an empty folder outside home and bind it from a different CWD.
    let ext = scratch("ext");
    std::fs::create_dir_all(&ext).unwrap();
    let ext_str = ext.to_string_lossy().to_string();

    let bound = cli.json(&["bind", "--project", &pid, "--dir", &ext_str, "--json"]);
    assert_eq!(id_str(&bound["binding"]["project_id"]), pid);
    // The pointer lands in the --dir target.
    assert!(ext.join(".amenbo").is_file(), "pointer placed in the --dir target");

    // A nonexistent --dir is rejected with io_error — there is nowhere to put `.amenbo`.
    let (err, code) = cli.run_err(&["bind", "--project", &pid, "--dir", "/no/such/amenbo-dir", "--json"]);
    assert_eq!(code, 1, "missing --dir target is rejected: {err}");

    let _ = std::fs::remove_dir_all(&ext);
}

/// `project show` reverses the binding: every folder linked to the project — CWD-bound and `--dir`-bound
/// alike, many-to-one — is listed under bound_folders, each with an existence check.
#[test]
fn project_show_lists_bound_folders_with_existence() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = id_str(&cli.json(&["project", "add", "--name", "逆引きPJ", "--json"])["project"]["id"]);

    // Bind the CWD (home) as the main folder, plus an external folder via `--dir` (many-to-one).
    cli.run(&["bind", "--project", &pid]);
    let ext = scratch("bf");
    std::fs::create_dir_all(&ext).unwrap();
    let ext_str = ext.to_string_lossy().to_string();
    cli.run(&["bind", "--project", &pid, "--dir", &ext_str]);
    // `--dir` is canonicalized (symlinks resolved) before it is recorded; match on that path later.
    let ext_canon = std::fs::canonicalize(&ext).unwrap().to_string_lossy().to_string();

    let shown = cli.json(&["project", "show", &pid, "--json"]);
    let folders = shown["bound_folders"].as_array().expect("bound_folders array");
    assert_eq!(folders.len(), 2, "both bound folders are listed: {folders:?}");
    // Both live folders report exists=true.
    assert!(folders.iter().all(|f| f["exists"] == true), "live folders exist: {folders:?}");

    // Removing the external folder shows up as exists=false — surfaced, never cleaned up.
    std::fs::remove_dir_all(&ext).unwrap();
    let after = cli.json(&["project", "show", &pid, "--json"]);
    let ext_entry = after["bound_folders"].as_array().unwrap().iter()
        .find(|f| f["path"].as_str() == Some(ext_canon.as_str()))
        .expect("external folder still recorded");
    assert_eq!(ext_entry["exists"], false, "removed folder is flagged stale");
}

/// When a bound folder is still there but its `.amenbo` is gone, the CLI says so: `doctor` lists it as a
/// `missing_pointer` warning and `project show` carries the same mark on `bound_folders` (mirroring the
/// GUI). `init` only repairs the folder you run it in, so folders you never enter are covered only here.
#[test]
fn doctor_and_project_show_flag_a_bound_folder_whose_pointer_vanished() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = id_str(&cli.json(&["project", "add", "--name", "ポインタ消失PJ", "--json"])["project"]["id"]);

    let ext = scratch("mp");
    std::fs::create_dir_all(&ext).unwrap();
    let ext_str = ext.to_string_lossy().to_string();
    cli.run(&["bind", "--project", &pid, "--dir", &ext_str]);
    let ext_canon = std::fs::canonicalize(&ext).unwrap().to_string_lossy().to_string();

    let has_missing = |v: &Value| {
        v["issues"].as_array().unwrap().iter().any(|i| i["kind"] == "missing_pointer")
    };
    let folder_entry = |v: &Value| -> Value {
        v["bound_folders"].as_array().unwrap().iter()
            .find(|f| f["path"].as_str() == Some(ext_canon.as_str()))
            .expect("bound folder is listed")
            .clone()
    };

    // While the pointer is there, nothing is reported.
    assert!(!has_missing(&cli.json(&["doctor", "--json"])), "does not flag when the pointer is present");
    let bound = folder_entry(&cli.json(&["project", "show", &pid, "--json"]));
    assert_eq!(bound["pointer_missing"], false, "a just-bound folder has a pointer: {bound}");

    // Delete `.amenbo` alone; the folder stays, so it is not stale.
    std::fs::remove_file(ext.join(".amenbo")).unwrap();

    let flagged = cli.json(&["doctor", "--json"]);
    let issue = flagged["issues"].as_array().unwrap().iter()
        .find(|i| i["kind"] == "missing_pointer")
        .unwrap_or_else(|| panic!("should detect missing_pointer: {flagged}"));
    assert_eq!(issue["severity"], "warning");
    assert_eq!(issue["target"].as_str(), Some(ext_canon.as_str()), "the target is that folder: {issue}");
    assert!(issue["fix_hint"].as_str().unwrap().contains("init"), "points out that init there fixes it: {issue}");
    assert_eq!(flagged["summary"]["error"], 0, "a missing pointer is a warning, not an error: {flagged}");

    // `project show` carries the same mark — the folder exists, so it is not stale.
    let lost = folder_entry(&cli.json(&["project", "show", &pid, "--json"]));
    assert_eq!(lost["exists"], true, "the folder itself exists: {lost}");
    assert_eq!(lost["pointer_missing"], true, "reports the loss of `.amenbo`: {lost}");
    assert_eq!(lost["legacy"], false, "a missing pointer is not the legacy form: {lost}");

    // Re-binding silences it: the hint really does fix it.
    cli.run(&["bind", "--project", &pid, "--dir", &ext_str]);
    assert!(!has_missing(&cli.json(&["doctor", "--json"])), "re-linking clears it");
    let relinked = folder_entry(&cli.json(&["project", "show", &pid, "--json"]));
    assert_eq!(relinked["pointer_missing"], false, "a re-linked folder returns to AI-operable: {relinked}");

    std::fs::remove_dir_all(&ext).unwrap();
}

/// The human success output of init/bind states a capability and a next step rather than reporting
/// machinery; the files placed are a light parenthetical, and the JSON envelope — the contract — is unchanged.
#[test]
fn init_and_bind_success_output_states_capability_and_next_step() {
    let cli = Cli::new();
    // init (human): states the capability (your AI can operate amenbo) and the next step (amenbo status).
    let (out, code) = cli.run(&["init", "--name", "tester"]);
    assert_eq!(code, 0);
    assert!(out.contains("can now operate amenbo"), "init states the capability: {out}");
    assert!(out.contains("amenbo status"), "init points at the next step: {out}");
    assert!(out.contains("(placed"), "placed files are a light parenthetical: {out}");

    // bind --project (human): the capability (this project is now operable) plus the next step.
    let pid = id_str(&cli.json(&["project", "add", "--name", "認知PJ", "--json"])["project"]["id"]);
    let (bout, bcode) = cli.run(&["bind", "--project", &pid]);
    assert_eq!(bcode, 0);
    assert!(bout.contains("Linked this folder to project '認知PJ'"), "bind names the project: {bout}");
    assert!(bout.contains("can now operate"), "bind states the capability: {bout}");
    assert!(bout.contains("amenbo status"), "bind points at the next step: {bout}");

    // The JSON envelope is unchanged — it is a contract the GUI depends on.
    let jinit = cli.json(&["bind", "--json"]);
    assert_eq!(id_str(&jinit["binding"]["project_id"]), pid, "JSON envelope unchanged");
}

#[test]
fn activity_records_system_events_and_comments() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // An AI cannot pass --project; the binding fills in where the task goes.
    let t = id_str(&cli.json(&["task", "add", "--title", "do it", "--actor", "ai", "--json"])["task"]["id"]);
    // reserve via status (todo→in_progress).
    cli.run(&["task", "status", &t, "in_progress", "--actor", "ai"]);
    cli.run(&["comment", "add", &t, "--actor", "ai", "--text", "starting"]);
    cli.run(&["task", "status", &t, "done", "--actor", "ai"]);

    let act = cli.json(&["activity", "--task", &t, "--json"]);
    let kinds: Vec<String> = act["items"].as_array().unwrap().iter().map(|i| {
        if i["type"] == "comment" { "comment".to_string() }
        else { i["event"]["kind"].as_str().unwrap().to_string() }
    }).collect();
    // All four are recorded, all on the ai facet (created + 2× status_changed + comment).
    assert_eq!(act["count"], 4);
    assert!(kinds.contains(&"task.created".to_string()));
    assert!(kinds.contains(&"comment".to_string()));
    assert!(kinds.contains(&"task.status_changed".to_string()));
    assert!(act["items"].as_array().unwrap().iter().all(|i| i["author"]["kind"] == "ai"));

    // --kind system keeps the system events only, dropping comments.
    let sys = cli.json(&["activity", "--task", &t, "--kind", "system", "--json"]);
    assert_eq!(sys["count"], 3);
    assert!(sys["items"].as_array().unwrap().iter().all(|i| i["type"] == "system"));

    // --by human shows none of the ai events.
    let human = cli.json(&["activity", "--by", "human", "--json"]);
    assert_eq!(human["count"], 0);

    // Paging: --limit/--offset cut a newest-first window for walking back through history.
    let all = cli.json(&["activity", "--task", &t, "--json"]);
    let ids: Vec<String> = all["items"].as_array().unwrap().iter()
        .map(|i| id_str(&i["id"])).collect();
    let p0 = cli.json(&["activity", "--task", &t, "--limit", "2", "--json"]);
    assert_eq!(p0["count"], 2);
    let p1 = cli.json(&["activity", "--task", &t, "--limit", "2", "--offset", "2", "--json"]);
    assert_eq!(p1["count"], 2);
    // offset=2 continues with items 3 and 4 — no overlap, still newest-first.
    assert_eq!(id_str(&p1["items"][0]["id"]), ids[2]);
    assert_eq!(id_str(&p1["items"][1]["id"]), ids[3]);
    // An offset past the end is empty.
    let p_end = cli.json(&["activity", "--task", &t, "--offset", "99", "--json"]);
    assert_eq!(p_end["count"], 0);
}

/// The ledger self-compacts at 8 MiB, so the very lines that carry a vanished subject's **name**
/// (task.created / task.deleted) can age out — as can a name that falls outside the lookback budget. Core
/// then returns an empty title, and piping that straight to a human leaves nothing after the "—", so the
/// human line has to say the subject is gone. `--json` is the machine's face and stays empty.
#[test]
fn a_subject_whose_name_the_ledger_no_longer_carries_reads_as_deleted() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();

    // Reproduce a ledger whose compaction dropped the naming lines: only a nameless line
    // (task.status_changed) is left, and its subject exists nowhere else. Append the raw line by hand — a
    // real deletion appends a `task.deleted` line carrying the name, so the name stays recoverable and it
    // cannot stage this break; only compaction ageing the naming lines out gets there, and that is not
    // something a test can ask for.
    let ledger = cli.home.join("activity.jsonl");
    let line = serde_json::json!({
        "v": 2,
        "id": 999_999,
        "at": "2099-01-01T00:00:00Z",
        "actor": "ai",
        "project": pid.parse::<i64>().unwrap(),
        "task": 999_999,
        "decision": null,
        "event": {"kind": "task.status_changed", "new": "done"},
    });
    let mut body = std::fs::read_to_string(&ledger).unwrap_or_default();
    body.push_str(&format!("{line}\n"));
    std::fs::write(&ledger, body).unwrap();

    let (out, code) = cli.run(&["activity"]);
    assert_eq!(code, 0);
    assert!(out.contains("task.status_changed"), "a row with no name is still shown: {out}");
    assert!(out.contains("— (deleted)"), "a subject whose name cannot be recovered says (deleted): {out}");

    // JSON stays raw (empty title): the machine gets the fact, not a paraphrase.
    let js = cli.json(&["activity", "--json"]);
    assert_eq!(js["items"][0]["target"]["title"], "");
}

/// Some lines still carry a name whose subject is gone — the **past** lines of a deleted task. Printing
/// the bare name makes it look alive, and the reader wastes a `task show` on it. Even the one-line human
/// form says an unreachable subject is unreachable; `--json` paraphrases nothing and passes `live` raw.
#[test]
fn a_past_line_of_a_deleted_subject_says_the_subject_is_gone() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let add = |title: &str| -> String {
        id_str(&cli.json(&["task", "add", "--title", title, "--project", &pid, "--json"])["task"]["id"])
    };

    let t = add("消されるタスク");
    // Lay down a nameless past line (task.status_changed), then delete the task. The ledger keeps that line
    // and the task.deleted line that carries the name — so core can name a subject that is no longer there.
    cli.run(&["task", "status", &t, "in_progress"]);
    cli.run(&["task", "delete", &t, "--yes"]);

    let (out, code) = cli.run(&["activity"]);
    assert_eq!(code, 0);
    assert!(out.contains("task.status_changed"), "past rows of a deleted subject remain in the ledger: {out}");
    assert!(
        !out.contains("— 消されるタスク\n"),
        "it does not print just the name to look like a live target: {out}"
    );
    assert!(out.contains("— 消されるタスク (deleted)"), "an untraceable target says so: {out}");

    // A live subject gets no extra mark: the mark means "gone", it is not decoration.
    let live = add("生きているタスク");
    cli.run(&["task", "status", &live, "in_progress"]);
    let (out, _) = cli.run(&["activity"]);
    assert!(out.contains("— 生きているタスク\n"), "a live target keeps its plain name: {out}");

    // JSON does not paraphrase — the machine reads `live`.
    let js = cli.json(&["activity", "--json"]);
    let items = js["items"].as_array().unwrap();
    let gone = items.iter().find(|i| i["target"]["title"] == "消されるタスク").unwrap();
    assert_eq!(gone["target"]["live"], false);
    let alive = items.iter().find(|i| i["target"]["title"] == "生きているタスク").unwrap();
    assert_eq!(alive["target"]["live"], true);
}

/// Built for agents: the opaque `--since <cursor>` returns only what is strictly newer than the last read,
/// oldest-first, and `--for me` narrows to events on tasks assigned to my facet.
#[test]
fn activity_incremental_cursor_and_for_me_scope() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // One task the ai picks up, one assigned to the human.
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let mine = id_str(&cli.json(&["task", "add", "--title", "ai task", "--to", "tester", "--ai", "--actor", "ai", "--json"])["task"]["id"]);
    let theirs = id_str(&cli.json(&["task", "add", "--title", "human task", "--project", &pid, "--json"])["task"]["id"]);
    cli.run(&["task", "status", &theirs, "in_progress"]); // the human facet reserves it via status → an event on the human side

    // Read once to get the cursor to resume from; history responses carry one too.
    let base = cli.json(&["activity", "--json"]);
    let cursor = base["cursor"].as_str().expect("history response carries an opaque cursor").to_string();
    assert!(cursor.starts_with("cur1_"), "opaque cursor prefix");

    // Nothing is strictly newer yet, and the position holds.
    let empty = cli.json(&["activity", "--since", &cursor, "--json"]);
    assert_eq!(empty["count"], 0, "nothing strictly newer yet");
    assert_eq!(empty["has_more"], false);

    // Make some movement on my own task after that point (status → comment → status).
    cli.run(&["task", "status", &mine, "in_progress", "--actor", "ai"]);
    cli.run(&["comment", "add", &mine, "--actor", "ai", "--text", "picked up"]);
    cli.run(&["task", "status", &mine, "done", "--actor", "ai"]);

    // Incremental: only what is newer than the cursor, oldest-first (status_changed → comment → status_changed).
    let inc = cli.json(&["activity", "--since", &cursor, "--json"]);
    assert!(inc["count"].as_u64().unwrap() >= 3, "the three new events since the cursor");
    let ats: Vec<String> = inc["items"].as_array().unwrap().iter().map(|i| i["at"].as_str().unwrap().to_string()).collect();
    let mut sorted = ats.clone();
    sorted.sort();
    assert_eq!(ats, sorted, "oldest-first (time-forward) for incremental consumption");
    // The advanced cursor differs from the old one and can be handed straight back next time.
    assert_ne!(inc["cursor"].as_str().unwrap(), cursor);

    // --for me (acting as the ai): human-assigned tasks drop out, leaving only my own.
    let for_me = cli.json(&["activity", "--for", "me", "--actor", "ai", "--json"]);
    assert!(for_me["count"].as_u64().unwrap() >= 1);
    assert!(
        for_me["items"].as_array().unwrap().iter().all(|i| id_str(&i["target"]["id"]) == mine),
        "--for me keeps only activity on tasks assigned to my facet"
    );

    // --for human is the mirror image: only human-assigned tasks.
    let for_human = cli.json(&["activity", "--for", "human", "--json"]);
    assert!(
        for_human["items"].as_array().unwrap().iter().all(|i| id_str(&i["target"]["id"]) == theirs),
        "--for human keeps only activity on human-assigned tasks"
    );

    // A malformed cursor fails loud instead of silently falling back to a date.
    let (_stderr, code) = cli.run_err(&["activity", "--since", "cur1_@@@broken@@@", "--json"]);
    assert_eq!(code, 2, "malformed cursor is fail-loud, not silently treated as a date");
}

#[test]
fn assign_facet_and_mailbox_filters() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // Assignees are referenced by facet token (human).
    let me = "human";
    let pid = cli.bound_project(); // what the AI touches lives in the bound project

    // Assigned to my AI, not started.
    let t1 = id_str(&cli.json(&["task", "add", "--title", "ai-todo", "--project", &pid, "--json"])["task"]["id"]);
    cli.run(&["task", "assign", &t1, "--to", me, "--ai"]);
    // Assigned to me (human).
    let t2 = id_str(&cli.json(&["task", "add", "--title", "human-mine", "--project", &pid, "--json"])["task"]["id"]);
    cli.run(&["task", "assign", &t2, "--to", me]);
    // Assigned to my AI and under way — reservation is status alone (in_progress).
    let t3 = id_str(&cli.json(&["task", "add", "--title", "ai-inprogress", "--project", &pid, "--json"])["task"]["id"]);
    cli.run(&["task", "assign", &t3, "--to", me, "--ai"]);
    cli.run(&["task", "status", &t3, "in_progress", "--actor", "ai"]);

    // assignee_kind is stamped.
    assert_eq!(cli.json(&["task", "show", &t1, "--json"])["assignee_kind"], "ai");
    assert_eq!(cli.json(&["task", "show", &t2, "--json"])["assignee_kind"], "human");

    let titles = |filter: &str| -> Vec<String> {
        cli.json(&["task", "list", "--filter", filter, "--json"])["tasks"]
            .as_array().unwrap().iter()
            .map(|t| t["title"].as_str().unwrap().to_string()).collect()
    };

    // The mailbox is AI-assigned and unstarted: t3 is under way and falls out of status:todo, so it is never double-booked.
    assert_eq!(titles("assignee:me-ai status:todo ready:yes"), vec!["ai-todo"]);
    // Only human-assigned, never me-ai.
    assert_eq!(titles("assignee:me"), vec!["human-mine"]);
    // The status filter picks out what is under way.
    assert_eq!(titles("status:in_progress"), vec!["ai-inprogress"]);
}

/// An assignee reference resolves to a **facet** (human / ai) — the only two subjects a single local store
/// has. It is looked up by reserved word (me/self/human, me-ai/ai) or by the display name in config, and a
/// token that matches neither fails to resolve (exit 1).
#[test]
fn assign_resolves_by_facet_token() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // whoami carries no account dimension (user_id); the display name comes from the config facet key.
    let who = cli.json(&["whoami", "--json"]);
    assert!(who.get("user_id").is_none(), "user_id is removed");
    assert!(who.get("user_name").is_none(), "user_name was renamed to human_name");
    assert_eq!(who["human_name"], "tester", "the human's display name is config.human_name");

    let pid = cli.a_project();
    let t = id_str(&cli.json(&["task", "add", "--title", "t", "--project", &pid, "--json"])["task"]["id"]);
    // Assign to the human facet by display name.
    cli.run(&["task", "assign", &t, "--to", "tester"]);
    assert_eq!(cli.json(&["task", "show", &t, "--json"])["assignee_kind"], "human", "the display name tester resolves to the human facet");
    // The reserved word me-ai lands on the AI facet.
    cli.run(&["task", "assign", &t, "--to", "me-ai"]);
    assert_eq!(cli.json(&["task", "show", &t, "--json"])["assignee_kind"], "ai", "me-ai resolves to the AI facet");

    // A token that matches no facet does not resolve (exit 1).
    let t2 = id_str(&cli.json(&["task", "add", "--title", "t2", "--project", &pid, "--json"])["task"]["id"]);
    let (stderr, code) = cli.run_err(&["task", "assign", &t2, "--to", "nobody"]);
    assert_eq!(code, 1, "an unknown token does not resolve to a facet: {stderr}");
}

#[test]
fn created_by_is_stamped_and_e_guardrail_limits_ai() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // A task the human creates.
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let h = cli.json(&["task", "add", "--title", "human task", "--project", &pid, "--json"]);
    let htid = id_str(&h["task"]["id"]);
    assert_eq!(h["task"]["created_by_kind"], "human");

    // A task the ai creates.
    let a = cli.json(&["task", "add", "--title", "ai task", "--actor", "ai", "--json"]);
    let atid = id_str(&a["task"]["id"]);
    assert_eq!(a["task"]["created_by_kind"], "ai");

    // The AI cannot delete a task the human created (ai_guardrail).
    let (_o, code) = cli.run(&["task", "delete", &htid, "--actor", "ai", "-y"]);
    assert_eq!(code, 1);
    // The AI can delete what it created itself.
    let (_o2, code2) = cli.run(&["task", "delete", &atid, "--actor", "ai", "-y"]);
    assert_eq!(code2, 0);

    // Reversible project ops (add/update/move/unarchive) are open to the AI by default, but only on the
    // **bound project** — naming any other is out_of_reach before ai_guardrail is even consulted.
    let (_ou, codeu) = cli.run(&["project", "update", &pid, "--name", "bound pj v2", "--actor", "ai"]);
    assert_eq!(codeu, 0);

    // The AI cannot create a project: a new one is by definition outside the binding, so it would leave
    // behind something the AI made yet cannot reach. Reversible ops are allowed, inside the binding alone.
    let (add_err, add_code) = cli.run_err(&["project", "add", "--name", "ai pj", "--actor", "ai", "--json"]);
    assert_eq!(add_code, 1, "a bound AI cannot create a new project");
    assert!(add_err.contains("out_of_reach"), "{add_err}");

    // Destructive or hiding ops (archive/delete) are refused to the AI by default.
    let (_o3, code3) = cli.run(&["project", "archive", &pid, "--actor", "ai"]);
    assert_eq!(code3, 1);
    let (_od, coded) = cli.run(&["project", "delete", &pid, "--actor", "ai", "-y"]);
    assert_eq!(coded, 1);
    // A local policy can grant them.
    cli.run(&["config", "set", "ai_allow_project_ops", "true"]);
    let (_o4, code4) = cli.run(&["project", "archive", &pid, "--actor", "ai"]);
    assert_eq!(code4, 0);

    // The human is unconstrained: add and delete alike.
    let p = cli.json(&["project", "add", "--name", "human pj", "--json"]);
    assert_eq!(p["action"], "project.add");
}

/// Reservation is `task status <id> in_progress` and handing it back is `task status <id> todo`, and
/// reserving is compare-and-swap — a re-reserve of an already-`in_progress` task is rejected
/// (`already_reserved`), not a no-op, so two sessions never double-book. Every other same-status set
/// stays idempotent.
#[test]
fn status_reserves_and_hands_back_and_reserve_is_compare_and_swap() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let t = cli.json(&["task", "add", "--title", "reservable", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    // reserve: todo → in_progress.
    let c = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(c["task"]["status"], "in_progress");

    // re-reserving an already-in_progress task is a CAS conflict, not a no-op.
    let (stderr, code) = cli.run_err(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "re-reserve must fail: {stderr}");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "already_reserved", "re-reserve → already_reserved: {stderr}");
    // The refusal leaves the status at in_progress — no regression.
    let show = cli.json(&["task", "show", &tid, "--json"]);
    assert_eq!(show["status"], "in_progress");

    // hand back: in_progress → todo.
    let r = cli.json(&["task", "status", &tid, "todo", "--actor", "ai", "--json"]);
    assert_eq!(r["task"]["status"], "todo");

    // Once handed back, it can be reserved again (todo → in_progress).
    let re = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(re["task"]["status"], "in_progress");

    // Re-setting any other status to the one it holds stays an idempotent no-op.
    cli.json(&["task", "status", &tid, "todo", "--actor", "ai", "--json"]);
    let r2 = cli.json(&["task", "status", &tid, "todo", "--actor", "ai", "--json"]);
    assert_eq!(r2["noop"], true);
}

/// The reserve also requires `ready`, and the CLI surfaces the refusal the same way
/// it surfaces `already_reserved` — a distinct error code on a non-zero exit, with the hint naming
/// the way out. The two failures must stay distinguishable: `already_reserved` sends you to the next
/// task, `not_ready` sends you to resolve your own declaration. Both premises are exercised (an open
/// blocker and an unsettled linked decision), and each is shown to release the reserve once resolved.
#[test]
fn reserving_a_not_ready_task_is_refused_with_a_way_out() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let blocker = cli.json(&["task", "add", "--title", "先行", "--project", &pid, "--json"]);
    let bid = id_str(&blocker["task"]["id"]);
    let t = cli.json(&["task", "add", "--title", "後続", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    cli.json(&["task", "depend", &tid, "--on", &bid, "--json"]);

    // An open blocker: naming the task by number gets you no reserve — the guard is in the write path, not the filter.
    let (stderr, code) = cli.run_err(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "reserve with an open blocker must fail: {stderr}");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "not_ready", "open blocker → not_ready: {stderr}");
    assert!(v["error"]["hint"].as_str().is_some_and(|h| h.contains("undepend")), "hint names the way out: {stderr}");
    let show = cli.json(&["task", "show", &tid, "--json"]);
    assert_eq!(show["status"], "todo", "a rejected reservation does not move the status");

    // Finishing the blocker lets the same reserve through.
    cli.json(&["task", "done", &bid, "--actor", "ai", "--json"]);
    let ok = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(ok["task"]["status"], "in_progress");
    cli.json(&["task", "status", &tid, "todo", "--actor", "ai", "--json"]);

    // An unsettled premise — a linked decision still proposed — is refused with the same code.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "この形にした理由", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    cli.json(&["decision", "link", &did, &tid, "--json"]);
    let (stderr, code) = cli.run_err(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "reserve on an unsettled premise must fail: {stderr}");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "not_ready", "unsettled premise → not_ready: {stderr}");

    // Settling it clears the way: accept satisfies the premise, and there is no --force.
    cli.json(&["decision", "accept", &did, "--json"]);
    let ok = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(ok["task"]["status"], "in_progress");
}

#[test]
fn actor_facet_is_stamped_from_flag_and_defaults_to_human() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let t = cli.json(&["task", "add", "--title", "facet", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    // --actor ai stamps author_kind=ai on the comment, and the write echoes the effective facet (acted_facet).
    let ai = cli.json(&["comment", "add", &tid, "--text", "from ai", "--actor", "ai", "--json"]);
    assert_eq!(ai["comment"]["author_kind"], "ai");
    assert_eq!(ai["acted_facet"], "ai");

    // The default — the harness declares AMENBO_ACTOR=human — is human.
    let human = cli.json(&["comment", "add", &tid, "--text", "from human", "--json"]);
    assert_eq!(human["comment"]["author_kind"], "human");
    assert_eq!(human["acted_facet"], "human");

    // status, the reserve, carries acted_facet too.
    let st = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(st["acted_facet"], "ai");

    // An invalid actor is invalid_value (exit 2).
    let (_out, code) = cli.run(&["task", "list", "--actor", "robot"]);
    assert_eq!(code, 2);
}

/// A **write** with no facet from a machine context (--json, no TTY) does not quietly become a human write:
/// it stops with facet_required (exit 2). Pure reads (version/list) record no facet, so they go through
/// without one — a machine's reads are never blocked for nothing.
#[test]
fn facet_required_fails_loud_on_unspecified_machine_writes_only() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project(); // somewhere for the explicit-facet task add below to land

    // A machine call that declares no facet at all (the test process has no TTY); AMENBO_ACTOR is removed.
    let spawn = |args: &[&str]| -> (String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env_remove("AMENBO_ACTOR")
            .current_dir(&cli.home)
            .args(args)
            .output()
            .expect("run amenbo");
        (String::from_utf8_lossy(&out.stderr).to_string(), out.status.code().unwrap_or(-1))
    };

    // The write (task add) stops with facet_required.
    let (stderr, code) = spawn(&["task", "add", "--title", "x", "--json"]);
    assert_eq!(code, 2, "a write with no facet specified must stop: {stderr}");
    assert!(stderr.contains("facet_required"), "should return facet_required: {stderr}");

    // Reads (version / task list) record no facet, so they pass without one.
    let (_e, code) = spawn(&["version", "--json"]);
    assert_eq!(code, 0, "version is a read, so it passes without a facet");
    let (_e, code) = spawn(&["task", "list", "--json"]);
    assert_eq!(code, 0, "task list is a read, so it passes without a facet");

    // With the facet spelled out (--actor human) the write goes through.
    let (_e, code) = spawn(&["task", "add", "--title", "y", "--project", &pid, "--actor", "human", "--json"]);
    assert_eq!(code, 0, "a write with an explicit facet passes");
}

/// One lap around the dimension model on the CLI: add an axis, value-add, list/show by name, set/unset on a
/// task (single-select replacement, and a cross-process no-op proving it persisted), rename, cascading rm.
#[test]
fn dimension_lifecycle_axis_values_and_assignment() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "次元PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // A single-select, ordered axis.
    let d = cli.json(&["dimension", "add", "--project", &pid, "--name", "エリア", "--ordered", "--json"]);
    let did = id_str(&d["dimension"]["id"]);
    assert_eq!(d["dimension"]["cardinality"], "single");
    assert_eq!(d["dimension"]["ordered"], true);

    // Two values: the first resolved by axis id, the second by axis name.
    let v1 = cli.json(&["dimension", "value-add", &did, "--name", "設計", "--json"]);
    let v1id = id_str(&v1["dimension_value"]["id"]);
    cli.json(&["dimension", "value-add", "エリア", "--name", "実装", "--json"]);

    // list returns the axis with its values — from another process, so through persistence.
    let list = cli.json(&["dimension", "list", "--project", &pid, "--json"]);
    assert_eq!(list["count"], 1);
    assert_eq!(list["dimensions"][0]["values"].as_array().unwrap().len(), 2);
    // show accepts the name too.
    let show = cli.json(&["dimension", "show", "エリア", "--json"]);
    assert_eq!(id_str(&show["dimension"]["id"]), did);

    // Assign to a task, resolving the value by name within the axis; the task ref needs no project context.
    let t = cli.json(&["task", "add", "--title", "T", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let set = cli.json(&["dimension", "set", &tid, "エリア", "設計", "--json"]);
    assert_eq!(set["noop"], false);
    assert_eq!(id_str(&set["task_dimension_value"]["value_id"]), v1id);
    // Setting the same value from another process is an idempotent no-op: it persisted.
    let again = cli.json(&["dimension", "set", &tid, "エリア", "設計", "--json"]);
    assert_eq!(again["noop"], true, "a persisted assignment is a noop on re-set");
    // Single-select: setting another value replaces the one row rather than adding to it.
    let repl = cli.json(&["dimension", "set", &tid, "エリア", "実装", "--json"]);
    assert_eq!(repl["noop"], false);

    // unset clears the assignment, and a second unset is a no-op.
    assert_eq!(cli.json(&["dimension", "unset", &tid, "エリア", "実装", "--json"])["noop"], false);
    assert_eq!(cli.json(&["dimension", "unset", &tid, "エリア", "実装", "--json"])["noop"], true);

    // Rename the axis.
    let rn = cli.json(&["dimension", "rename", &did, "--name", "領域", "--json"]);
    assert_eq!(rn["dimension"]["name"], "領域");

    // rm cascades over axis and values, and the listing returns to empty.
    cli.json(&["dimension", "rm", &did, "--yes", "--json"]);
    assert_eq!(cli.json(&["dimension", "list", "--project", &pid, "--json"])["count"], 0);
}

/// Only values on a time axis (role: time_axis) carry a period `[start_on, end_on]`. value-add /
/// value-update write it, list / show print it for humans, and dates on any other axis are turned away by
/// the CLI gatekeeper (core just writes the columns) — all across processes, so persistence is included.
#[test]
fn time_axis_values_carry_a_period_and_other_axes_reject_dates() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "期間PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "時代", "--ordered", "--time-axis", "--json"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "エリア", "--json"]);

    // One closed period, and one with an open end — still running.
    let closed = cli.json(&["dimension", "value-add", "時代", "--name", "開発期", "--start", "2026-06-20", "--end", "2026-07-07", "--json"]);
    assert_eq!(closed["dimension_value"]["start_on"], "2026-06-20");
    assert_eq!(closed["dimension_value"]["end_on"], "2026-07-07");
    let open = cli.json(&["dimension", "value-add", "時代", "--name", "運用第1期", "--start", "2026-07-08", "--json"]);
    assert_eq!(open["dimension_value"]["end_on"], Value::Null, "omit the end and it is ongoing");

    // A value with no period carries no dates; that is the value-add default.
    let plain = cli.json(&["dimension", "value-add", "エリア", "--name", "設計", "--json"]);
    assert_eq!(plain["dimension_value"]["start_on"], Value::Null);

    // Human output: only values with a period print one, and an open end reads as ongoing.
    let (shown, code) = cli.run(&["dimension", "show", "時代"]);
    assert_eq!(code, 0, "{shown}");
    assert!(shown.contains("[2026-06-20 → 2026-07-07]"), "shows a closed period: {shown}");
    assert!(shown.contains("[2026-07-08 → ongoing]"), "an open end shows ongoing: {shown}");
    let (listed, _) = cli.run(&["dimension", "list", "--project", &pid]);
    assert!(listed.contains("[2026-06-20 → 2026-07-07]"), "list also shows the period: {listed}");
    assert!(!listed.contains("設計  ["), "a value with no period shows no brackets: {listed}");

    // value-update touches only the fields it is given — a rename keeps the period.
    let renamed = cli.json(&["dimension", "value-update", "時代", "開発期", "--name", "黎明期", "--json"]);
    assert_eq!(renamed["dimension_value"]["name"], "黎明期");
    assert_eq!(renamed["dimension_value"]["end_on"], "2026-07-07", "renaming does not clear the period");
    assert_eq!(renamed["changed"], serde_json::json!(["name"]));

    // Close the end, then open it again.
    let closed_now = cli.json(&["dimension", "value-update", "時代", "運用第1期", "--end", "2026-12-31", "--json"]);
    assert_eq!(closed_now["dimension_value"]["end_on"], "2026-12-31");
    let reopened = cli.json(&["dimension", "value-update", "時代", "運用第1期", "--clear-end", "--json"]);
    assert_eq!(reopened["dimension_value"]["end_on"], Value::Null, "--clear-end makes it ongoing again");
    assert_eq!(reopened["dimension_value"]["start_on"], "2026-07-08", "opening the end keeps the start");

    // An inverted period is refused by core.
    let (err, code) = cli.run_err(&["dimension", "value-update", "時代", "黎明期", "--start", "2026-08-01", "--json"]);
    assert_ne!(code, 0, "start > end is rejected: {err}");

    // Dates on a non-time axis are turned away by the CLI gatekeeper, on value-add and value-update alike.
    let (err, code) = cli.run_err(&["dimension", "value-add", "エリア", "--name", "実装", "--start", "2026-07-08", "--json"]);
    assert_ne!(code, 0, "value-add on a non-time-axis rejects dates: {err}");
    let (err, code) = cli.run_err(&["dimension", "value-update", "エリア", "設計", "--clear-end", "--json"]);
    assert_ne!(code, 0, "value-update on a non-time-axis rejects period ops: {err}");
    // The refusal added no value: the gatekeeper stands before the write.
    let vals = cli.json(&["dimension", "show", "エリア", "--json"]);
    assert_eq!(vals["values"].as_array().unwrap().len(), 1);
}

/// An existing axis can be named the time axis after the fact, and unnamed again (`dimension update
/// --time-axis`). That naming *is* the date gatekeeper: the axis refuses dates, then takes them, then refuses again.
#[test]
fn dimension_update_names_and_unnames_the_time_axis() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "指名PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    // An axis created with no role refuses dates.
    cli.json(&["dimension", "add", "--project", &pid, "--name", "時代", "--ordered", "--json"]);
    let (err, code) = cli.run_err(&["dimension", "value-add", "時代", "--name", "開発期", "--start", "2026-06-20", "--json"]);
    assert_ne!(code, 0, "before designation, dates are rejected: {err}");

    // Once named it takes periods, and the current era is settled — new tasks default to it.
    let named = cli.json(&["dimension", "update", "時代", "--time-axis", "true", "--json"]);
    assert_eq!(named["dimension"]["role"], "time_axis");
    assert_eq!(named["changed"], serde_json::json!(["role"]));
    cli.json(&["dimension", "value-add", "時代", "--name", "運用第1期", "--start", "2026-07-08", "--json"]);
    let (shown, _) = cli.run(&["dimension", "show", "時代"]);
    assert!(shown.contains("[2026-07-08 → ongoing]"), "after designation, the period is shown: {shown}");

    // Unnaming it makes dates refused again; the dates already stored stay, but mean nothing.
    let unnamed = cli.json(&["dimension", "update", "時代", "--time-axis", "false", "--json"]);
    assert_eq!(unnamed["dimension"]["role"], "none");
    let (err, code) = cli.run_err(&["dimension", "value-update", "時代", "運用第1期", "--clear-start", "--json"]);
    assert_ne!(code, 0, "clear the designation and dates are rejected: {err}");
    let vals = cli.json(&["dimension", "show", "時代", "--json"]);
    assert_eq!(vals["values"][0]["start_on"], "2026-07-08", "the dates remain in the physical columns");
}

/// A new task defaults to the era on its project's time axis that **covers today** — automation, not a
/// requirement. With no era over today it is created unassigned, and the default can be cleared or overridden.
#[test]
fn task_add_defaults_to_the_time_axis_value_covering_today() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "時代PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "時代", "--ordered", "--time-axis", "--json"]);

    // While only a past window exists, no default is applied — creation is not refused.
    cli.json(&["dimension", "value-add", "時代", "--name", "黎明期", "--start", "2000-01-01", "--end", "2000-12-31", "--json"]);
    cli.json(&["task", "add", "--title", "窓の外", "--project", &pid, "--json"]);
    let outside = cli.json(&["task", "list", "--filter", "time_axis:黎明期", "--json"]);
    assert_eq!(outside["count"], 0, "with no era covering today, nothing is assigned");

    // Add an ongoing era (an open end) and new tasks pick it up by default.
    cli.json(&["dimension", "value-add", "時代", "--name", "現代", "--start", "2000-01-01", "--json"]);
    let t = cli.json(&["task", "add", "--title", "既定つき", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let current = cli.json(&["task", "list", "--filter", "time_axis:現代", "--json"]);
    assert_eq!(current["count"], 1, "the current era is assigned by default at creation");
    assert_eq!(id_str(&current["tasks"][0]["id"]), tid);

    // The default is not mandatory: it can be cleared.
    cli.json(&["dimension", "unset", &tid, "時代", "現代", "--json"]);
    let cleared = cli.json(&["task", "list", "--filter", "time_axis:現代", "--json"]);
    assert_eq!(cleared["count"], 0, "the default can be cleared");

    // Overriding works too — the time axis is single-select, so it replaces.
    cli.json(&["dimension", "set", &tid, "時代", "黎明期", "--json"]);
    let overridden = cli.json(&["task", "list", "--filter", "time_axis:黎明期", "--json"]);
    assert_eq!(overridden["count"], 1, "it can be overridden to another era");
}

/// Nested-binding guard: `bind --project` inside a managed tree (an ancestor holds `.amenbo`) would shadow
/// the parent and scatter `.amenbo`/AGENTS.md/CLAUDE.md through the source tree. As with init's clobber
/// guard, an existing tree is respected: refused without `--force`, allowed with it for a deliberate bind.
#[test]
fn bind_refuses_nested_subdirectory_without_force() {
    let cli = Cli::new();
    // Establish the managed root: init drops `.amenbo` into home (which is both AMENBO_HOME and cwd).
    let (_o, code) = cli.run(&["init", "--name", "tester"]);
    assert_eq!(code, 0, "init succeeds at the managed root");
    // A project to bind to (`bind --project` resolves by id prefix, so pass the id).
    let pid = id_str(&cli.json(&["project", "add", "--name", "P", "--json"])["project"]["id"]);

    // A subdirectory inside the managed tree, standing in for a crate directory in a source tree.
    let subdir = cli.home.join("crates").join("amenbo-cli");
    std::fs::create_dir_all(&subdir).unwrap();

    let bind = |cwd: &std::path::Path, extra: &[&str]| {
        let mut args: Vec<&str> = vec!["bind", "--project", &pid];
        args.extend_from_slice(extra);
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env("AMENBO_ACTOR", "human")
            .current_dir(cwd)
            .args(&args)
            .output()
            .expect("run amenbo bind");
        (
            String::from_utf8_lossy(&out.stderr).to_string(),
            out.status.code().unwrap_or(-1),
        )
    };

    // 1) In the subdir without --force: refused, and neither pointer nor managed block is written.
    let (stderr, code) = bind(&subdir, &[]);
    assert_eq!(code, 1, "nested bind is refused without --force: {stderr}");
    assert!(stderr.contains("managed tree"), "the error explains the nested tree: {stderr}");
    assert!(!subdir.join(".amenbo").exists(), "no pointer is written into the source subdirectory");
    assert!(!subdir.join("CLAUDE.md").exists(), "no managed CLAUDE.md is scattered into the subdirectory");
    assert!(!subdir.join("AGENTS.md").exists(), "no managed AGENTS.md is scattered into the subdirectory");

    // 2) With --force a deliberate subdir bind goes through — the escape hatch.
    let (stderr2, code2) = bind(&subdir, &["--force"]);
    assert_eq!(code2, 0, "--force overrides the nested guard: {stderr2}");
    assert!(subdir.join(".amenbo").exists(), "--force writes the pointer");
}

/// `task show` bundles the four things an agent must read before starting — body, notes, the
/// linked decisions (the "why"), and the latest comments — in one command, so none is missed by
/// reading notes alone. The JSON carries `linked_decisions` and `recent_comments` additively.
#[test]
fn task_show_bundles_notes_comments_and_linked_decisions() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "サンプル", "--project", &pid, "--notes", "着手前に読む前提", "--json"]);
    let tid = id_str(&t["task"]["id"]);

    cli.json(&["comment", "add", &tid, "--text", "古いコメント", "--json"]);
    cli.json(&["comment", "add", &tid, "--text", "最新の但し書き", "--json"]);

    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "この形にした理由", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    cli.json(&["decision", "link", &did, &tid, "--json"]);

    let shown = cli.json(&["task", "show", &tid, "--json"]);
    // notes keeps its existing key, as part of the body.
    assert_eq!(shown["notes"], "着手前に読む前提");
    // Of the four: the linked decisions come back by reverse lookup.
    let decisions = shown["linked_decisions"].as_array().expect("linked_decisions array");
    assert_eq!(decisions.len(), 1, "linked decision surfaced inline");
    assert_eq!(decisions[0]["id"], 1, "the id is the decision number");
    assert_eq!(decisions[0]["title"], "この形にした理由");
    // Of the four: the comment bodies come too, not just the count.
    let comments = shown["recent_comments"].as_array().expect("recent_comments array");
    assert_eq!(comments.len(), 2);
    let texts: Vec<&str> = comments.iter().map(|c| c["text"].as_str().unwrap()).collect();
    assert!(texts.contains(&"最新の但し書き") && texts.contains(&"古いコメント"));
}

/// Decisions take `comment add`/`list`, and `accept`/`reject --reason` is thin sugar that appends one
/// reason comment — there is no dedicated field. An empty or whitespace-only reason is ignored.
#[test]
fn decision_comment_add_list_and_accept_reject_reason() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // comment add shows up in list, oldest first.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    cli.json(&["decision", "comment", "add", &did, "--text", "初回コメント", "--json"]);
    let listed = cli.json(&["decision", "comment", "list", &did, "--json"]);
    assert_eq!(listed["count"], 1);
    assert_eq!(id_str(&listed["decision"]["id"]), did);
    assert_eq!(listed["comments"][0]["text"], "初回コメント");

    // accept --reason appends one comment: the reason lands on the timeline, not in the body.
    cli.json(&["decision", "accept", &did, "--reason", "レビュー後に合意", "--json"]);
    let after_accept = cli.json(&["decision", "comment", "list", &did, "--json"]);
    assert_eq!(after_accept["count"], 2, "one reason comment is added");
    assert_eq!(after_accept["comments"][1]["text"], "レビュー後に合意");
    // The decision itself becomes accepted — the sugar does not get in the way of the transition.
    assert_eq!(cli.json(&["decision", "show", &did, "--json"])["status"], "accepted");

    // reject --reason behaves the same way.
    let d2 = cli.json(&["decision", "add", "--project", &pid, "--title", "却下される案", "--json"]);
    let did2 = id_str(&d2["decision"]["id"]);
    cli.json(&["decision", "reject", &did2, "--reason", "D-1 で代替", "--json"]);
    let rej = cli.json(&["decision", "comment", "list", &did2, "--json"]);
    assert_eq!(rej["count"], 1);
    assert_eq!(rej["comments"][0]["text"], "D-1 で代替");

    // A whitespace-only reason is ignored, leaving no empty comment behind.
    let d3 = cli.json(&["decision", "add", "--project", &pid, "--title", "理由なし", "--json"]);
    let did3 = id_str(&d3["decision"]["id"]);
    cli.json(&["decision", "accept", &did3, "--reason", "   ", "--json"]);
    assert_eq!(cli.json(&["decision", "comment", "list", &did3, "--json"])["count"], 0);
}

/// Re-accepting an already-accepted decision is an idempotent noop that **says so** instead of a bare
/// "✓" that reads as a fresh acceptance: `noop` is true, `changed` is empty, the facet that first
/// settled it is never silently overwritten (that is `reopen`'s job), and a `--reason` on the noop
/// does not pile a comment. `reject` / `supersede` are the same shape.
#[test]
fn re_settling_a_decision_is_a_reported_noop_and_does_not_overwrite_or_pile_a_reason() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // First accept settles it; the facet is recorded.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "採択の名義", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let first = cli.json(&["decision", "accept", &did, "--json"]);
    assert_eq!(first["noop"], false);
    assert_eq!(first["decision"]["decided_by"]["name"], "human");

    // Re-accepting reports a noop with nothing changed, keeps the recorded facet (re-stamping is
    // `reopen`'s route), and the `--reason` does not become a comment.
    let again = cli.json(&["decision", "accept", &did, "--reason", "名義を直したい", "--json"]);
    assert_eq!(again["noop"], true, "re-accepting is a reported noop");
    assert_eq!(again["changed"].as_array().unwrap().len(), 0, "nothing changed");
    assert_eq!(again["decision"]["decided_by"]["name"], "human", "the recorded facet is untouched");
    assert_eq!(
        cli.json(&["decision", "comment", "list", &did, "--json"])["count"], 0,
        "a reason on a noop re-accept must not pile a comment"
    );

    // reject: re-rejecting an already-rejected decision is a reported noop too.
    let dr = cli.json(&["decision", "add", "--project", &pid, "--title", "却下の冪等", "--json"]);
    let didr = id_str(&dr["decision"]["id"]);
    assert_eq!(cli.json(&["decision", "reject", &didr, "--json"])["noop"], false);
    let rej_again = cli.json(&["decision", "reject", &didr, "--reason", "二度目", "--json"]);
    assert_eq!(rej_again["noop"], true);
    assert_eq!(
        cli.json(&["decision", "comment", "list", &didr, "--json"])["count"], 0,
        "a reason on a noop re-reject must not pile a comment"
    );

    // supersede: re-superseding an already-superseded pair is a reported noop.
    let old = cli.json(&["decision", "add", "--project", &pid, "--title", "旧", "--json"]);
    let oldid = id_str(&old["decision"]["id"]);
    cli.json(&["decision", "accept", &oldid, "--json"]);
    let new = cli.json(&["decision", "add", "--project", &pid, "--title", "新", "--json"]);
    let newid = id_str(&new["decision"]["id"]);
    assert_eq!(cli.json(&["decision", "supersede", &newid, "--replaces", &oldid, "--json"])["noop"], false);
    assert_eq!(
        cli.json(&["decision", "supersede", &newid, "--replaces", &oldid, "--json"])["noop"], true,
        "re-superseding an already-superseded pair is a noop"
    );
}

/// `task show` surfaces dependents (`blocks`) — the reverse of `blocked_by` — so an agent
/// can see what finishing this task would unblock. The category is always signposted: the human output
/// prints `blocks: (none)` when empty (never silently omitted, so the agent cannot mistake "no
/// dependents" for "this category does not exist"), and the JSON carries `blocks` additively.
#[test]
fn task_show_surfaces_dependents_blocks() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let a = cli.json(&["task", "add", "--title", "後続A", "--project", &pid, "--json"]);
    let aid = id_str(&a["task"]["id"]);
    let b = cli.json(&["task", "add", "--title", "先行B", "--project", &pid, "--json"]);
    let bid = id_str(&b["task"]["id"]);
    // A depends on B ⇒ finishing B unblocks A ⇒ B `blocks` A.
    cli.json(&["task", "depend", &aid, "--on", &bid, "--json"]);

    // JSON: B lists A in `blocks`; A has no dependents.
    let b_json = cli.json(&["task", "show", &bid, "--json"]);
    let blocks = b_json["blocks"].as_array().expect("blocks array");
    assert_eq!(blocks.len(), 1, "B blocks exactly A");
    assert_eq!(blocks[0]["name"], "後続A");
    let a_json = cli.json(&["task", "show", &aid, "--json"]);
    assert_eq!(a_json["blocks"].as_array().expect("blocks array").len(), 0, "nothing depends on A");

    // Human: B names the dependent; A signposts the empty category rather than hiding it.
    let (b_human, code) = cli.run(&["task", "show", &bid]);
    assert_eq!(code, 0, "{b_human}");
    assert!(
        b_human.contains("blocks (1):") && b_human.contains("後続A"),
        "B's human output lists the dependent: {b_human}"
    );
    let (a_human, _) = cli.run(&["task", "show", &aid]);
    assert!(
        a_human.contains("blocks: (none)"),
        "A's human output signposts the empty category: {a_human}"
    );
}

/// Every information category in `task show` is always signposted — a bare task with no notes,
/// no blockers, no dependents and no linked decisions still prints each label with `(none)` rather
/// than omitting the line, so an agent cannot mistake "empty" for "this category does not exist".
#[test]
fn task_show_signposts_empty_categories() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "素のタスク", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let (out, code) = cli.run(&["task", "show", &tid]);
    assert_eq!(code, 0, "{out}");
    for marker in ["blocked by: (none)", "blocks: (none)", "notes: (none)", "decisions: (none)"] {
        assert!(out.contains(marker), "missing signpost `{marker}` in:\n{out}");
    }
}

/// The `attach` surface end-to-end — a file ingests as a `blob` (metadata recorded, bytes in
/// the content-addressed store), an external link attaches as `url`, both list/show, and `rm` deletes
/// them so the listing drops back to empty.
#[test]
fn attach_blob_and_url_lifecycle() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "添付PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "資料つきタスク", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "決めた理由", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    // blob: ingest a file. The mime is guessed from the extension (.md → text/markdown), size is the byte length.
    let file = cli.home.join("report.md");
    std::fs::write(&file, "# title\nbody\n").unwrap();
    let add = cli.json(&["task", "attach", &tid, file.to_str().unwrap(), "--json"]);
    assert_eq!(add["action"], "attach.add");
    assert_eq!(add["attachment"]["kind"], "blob");
    assert_eq!(add["attachment"]["mime"], "text/markdown");
    assert_eq!(add["attachment"]["filename"], "report.md");
    let blob_id = id_str(&add["attachment"]["id"]);

    // url: hang an external link off a decision (--url); nothing is ingested.
    let url = cli.json(&["decision", "attach", &did, "https://example.com/spec", "--url", "--name", "spec", "--json"]);
    assert_eq!(url["attachment"]["kind"], "url");
    assert_eq!(url["attachment"]["url"], "https://example.com/spec");

    // Non-web schemes are turned away at the door — what is never stored can never reach the OS opener.
    let (_, code) = cli.run_err(&["task", "attach", &tid, "file:///etc/passwd", "--url", "--json"]);
    assert_ne!(code, 0, "a file: url attachment is not accepted");

    // ls: one blob on the task, one url on the decision. Ids are conversational numbers, so a bare `1` reads
    // as task `#1` or decision `D-1` alike: name the type (an ambiguous ref is rejected as `ambiguous_id`).
    let ls_task = cli.json(&["attach", "ls", &format!("T-{tid}"), "--json"]);
    assert_eq!(ls_task["count"], 1);
    assert_eq!(ls_task["attachments"][0]["kind"], "blob");
    let ls_dec = cli.json(&["attach", "ls", &format!("D-{did}"), "--json"]);
    assert_eq!(ls_dec["count"], 1);
    assert_eq!(ls_dec["attachments"][0]["kind"], "url");

    // show: fetch one attachment's metadata by id.
    let show = cli.json(&["attach", "show", &blob_id, "--json"]);
    assert_eq!(id_str(&show["id"]), blob_id);
    assert_eq!(show["size_bytes"], 13);

    // Without -y, rm is refused in a non-interactive context and the attachment survives.
    let (_, refused) = cli.run(&["attach", "rm", &blob_id, "--json"]);
    assert_eq!(refused, 1, "an rm that did not skip confirmation is rejected");
    assert_eq!(cli.json(&["attach", "ls", &format!("T-{tid}"), "--json"])["count"], 1, "a rejected rm deletes nothing");

    // rm removes it, a fresh ls no longer shows it, and a second rm is a no-op.
    let rm = cli.json(&["attach", "rm", &blob_id, "--yes", "--json"]);
    assert_eq!(rm["action"], "attach.rm");
    assert_eq!(rm["noop"], false);
    let ls_after = cli.json(&["attach", "ls", &format!("T-{tid}"), "--json"]);
    assert_eq!(ls_after["count"], 0);

    // A missing id is not_found (non-zero exit).
    let (_, code) = cli.run(&["attach", "show", "01NOPENOPENOPENOPENOPENOPE"]);
    assert_ne!(code, 0);
}

/// `attach save` writes a blob's bytes back out to a file — the CLI counterpart of the GUI download.
/// A file path is written verbatim; a directory saves under the attachment's own filename. It refuses
/// to clobber an existing destination without `--force`, and refuses a URL attachment (no bytes to save).
#[test]
fn attach_save_writes_a_blob_to_a_file() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "保存PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "保存タスク", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "理由", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    let body = "# title\nbody\n";
    let src = cli.home.join("report.md");
    std::fs::write(&src, body).unwrap();
    let blob_id = id_str(&cli.json(&["task", "attach", &tid, src.to_str().unwrap(), "--json"])["attachment"]["id"]);

    // Save to an explicit file path — the bytes round-trip exactly.
    let dst = cli.home.join("out").join("copy.md");
    std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
    let saved = cli.json(&["attach", "save", &blob_id, "--out", dst.to_str().unwrap(), "--json"]);
    assert_eq!(saved["action"], "attach.save");
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), body);

    // Save into a directory — the file lands under the attachment's own filename.
    let dir = cli.home.join("into");
    std::fs::create_dir_all(&dir).unwrap();
    cli.json(&["attach", "save", &blob_id, "--out", dir.to_str().unwrap(), "--json"]);
    assert_eq!(std::fs::read_to_string(dir.join("report.md")).unwrap(), body);

    // An existing destination is not clobbered without --force; --force overwrites it.
    let (_, refused) = cli.run(&["attach", "save", &blob_id, "--out", dst.to_str().unwrap(), "--json"]);
    assert_ne!(refused, 0, "saving over an existing file without --force is refused");
    let forced = cli.json(&["attach", "save", &blob_id, "--out", dst.to_str().unwrap(), "--force", "--json"]);
    assert_eq!(forced["action"], "attach.save");

    // A URL attachment has no bytes to save.
    let url_id = id_str(&cli.json(&["decision", "attach", &did, "https://example.com/spec", "--url", "--json"])["attachment"]["id"]);
    let (_, code) = cli.run(&["attach", "save", &url_id, "--out", cli.home.join("nope").to_str().unwrap(), "--json"]);
    assert_ne!(code, 0, "a url attachment cannot be saved");

    // A missing id is not_found.
    let (_, code) = cli.run(&["attach", "save", "01NOPENOPENOPENOPENOPENOPE", "--json"]);
    assert_ne!(code, 0);
}

/// How many blob files actually sit in a `blobs/` directory under `<home>`.
fn blob_count(home: &std::path::Path) -> usize {
    fn walk(dir: &std::path::Path, inside_blobs: bool, n: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, inside_blobs || p.file_name() == Some("blobs".as_ref()), n);
            } else if inside_blobs {
                *n += 1;
            }
        }
    }
    let mut n = 0;
    walk(home, false, &mut n);
    n
}

/// Ordering invariant: `attach` ingests the bytes only **after** the target resolves. The other order lets
/// a failed attach leave behind a pinned blob with zero references. Blobs are reclaimed on the delete paths
/// (`attach rm`, deleting a task or a decision), and each reclaims only what it orphaned — so an orphan
/// from an attach that never happened is on no delete path, and only a `doctor --fix` sweep picks it up.
/// Until then every failure fattens `blobs/`, and `backup` ships that directory whole.
#[test]
fn failed_attach_ingests_nothing() {
    let cli = Cli::new();
    let pid = cli.a_project();
    let tid = id_str(&cli.json(&["task", "add", "--title", "資料つきタスク", "--project", &pid, "--json"])["task"]["id"]);
    let file = cli.home.join("payload.txt");
    std::fs::write(&file, "payload\n").unwrap();
    let path = file.to_str().unwrap();

    // An unresolvable target exits non-zero and leaves no blob behind — tasks, decisions and comments alike.
    for target in ["#99999", "T-99999", "01NOPENOPENOPENOPENOPENOPE"] {
        let (_, code) = cli.run(&["task", "attach", target, path]);
        assert_ne!(code, 0, "an unresolvable attach target '{target}' should fail");
        let (_, code) = cli.run(&["decision", "attach", target, path]);
        assert_ne!(code, 0, "an unresolvable decision '{target}' should fail");
        let (_, code) = cli.run(&["comment", "attach", target, path]);
        assert_ne!(code, 0, "an unresolvable comment '{target}' should fail");
        assert_eq!(blob_count(&cli.home), 0, "a failed attach left a blob (target '{target}')");
    }

    // An unreadable file ingests nothing either: metadata and the per-file limit are checked before ingest.
    let (_, code) = cli.run(&["task", "attach", &tid, "no-such-file.txt"]);
    assert_ne!(code, 0, "attaching a missing file should fail");
    assert_eq!(blob_count(&cli.home), 0, "a failed attach left a blob (missing file)");

    // A successful attach does leave one blob, which proves the zeros above are not a miscount.
    cli.json(&["task", "attach", &tid, path, "--json"]);
    assert_eq!(blob_count(&cli.home), 1, "a successful attach leaves one blob");
}

/// An empty reference points at nothing: it fails to resolve, and it is not a wildcard. In a store with a
/// single live candidate an empty prefix would otherwise match it, `pick_id` would read that as a unique
/// hit, and `amenbo task done ""` would rewrite the one row it found without asking.
#[test]
fn an_empty_ref_resolves_to_nothing() {
    let cli = Cli::new();
    let pid = cli.a_project();
    let tid = id_str(&cli.json(&["task", "add", "--title", "唯一のタスク", "--project", &pid, "--json"])["task"]["id"]);
    cli.json(&["decision", "add", "--project", &pid, "--title", "唯一の決定", "--json"]);

    // Even with exactly one live task and one live decision, an empty ref does not resolve (non-zero exit).
    for empty in ["", " "] {
        let (_, code) = cli.run(&["task", "done", empty]);
        assert_ne!(code, 0, "an empty task ref {empty:?} resolved");
        let (_, code) = cli.run(&["task", "show", empty]);
        assert_ne!(code, 0, "an empty task ref {empty:?} resolved");
        let (_, code) = cli.run(&["decision", "show", empty]);
        assert_ne!(code, 0, "an empty decision ref {empty:?} resolved");
    }

    // The task was not touched — it did not quietly become done.
    assert_eq!(cli.json(&["task", "show", &tid, "--json"])["status"], "todo");
}

/// The two comment tables number independently, so the same decimal id can stand in both. **The command
/// says which table**: `comment attach` means a task comment, `decision comment attach` a decision comment,
/// and `attach ls` picks the table with a flag. Comment ids carry no type sigil, unlike `T-n` / `D-n`.
#[test]
fn a_comment_id_in_both_tables_is_disjoined_by_the_command() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    // The tables number independently, so drive the decision side up until it collides with the task side's
    // id — that collision is exactly why the table has to be named.
    let tc = id_str(&cli.json(&["comment", "add", &tid, "--text", "タスクのコメント", "--json"])["comment"]["id"]);
    let dc = loop {
        let id = id_str(&cli.json(&["decision", "comment", "add", &did, "--text", "決定のコメント", "--json"])["comment"]["id"]);
        assert!(id.parse::<i64>().unwrap() <= tc.parse::<i64>().unwrap(), "the decision numbering overtook the task numbering");
        if id == tc {
            break id;
        }
    };

    cli.json(&["comment", "attach", &tc, "https://example.com/task", "--url", "--json"]);
    cli.json(&["decision", "comment", "attach", &dc, "https://example.com/decision", "--url", "--json"]);

    // The same id reaches the right table, because the command and its flag choose the table.
    let on_task = cli.json(&["attach", "ls", "--task-comment", &tc, "--json"]);
    assert_eq!(on_task["count"], 1);
    assert_eq!(on_task["attachments"][0]["url"], "https://example.com/task");
    let on_decision = cli.json(&["attach", "ls", "--decision-comment", &dc, "--json"]);
    assert_eq!(on_decision["count"], 1);
    assert_eq!(on_decision["attachments"][0]["url"], "https://example.com/decision");

    // There is no way to hand `attach ls` a bare comment id: it is read as a task/decision ref and fails.
    let (_, code) = cli.run(&["attach", "ls", "--task-comment", "9999", "--json"]);
    assert_eq!(code, 1, "a nonexistent comment is not_found");
}

/// A misposted comment is taken back with `comment rm` — a hard delete, attachments and all. Decision comments mirror it.
#[test]
fn comment_rm_deletes_the_comment_and_its_attachment() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    let c = cli.json(&["comment", "add", &tid, "--text", "誤投稿", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    cli.json(&["comment", "add", &tid, "--text", "残すコメント", "--json"]);
    cli.json(&["comment", "attach", &cid, "https://example.com/", "--url", "--json"]);

    let removed = cli.json(&["comment", "rm", &cid, "--yes", "--json"]);
    assert_eq!(removed["comment"]["deleted"], true);

    let listed = cli.json(&["comment", "list", &tid, "--json"]);
    assert_eq!(listed["count"], 1, "only the deleted comment drops out");
    assert_eq!(listed["comments"][0]["text"], "残すコメント");
    // The attachments hanging off the comment go with it, and with their target gone `attach ls` can no
    // longer resolve that id.
    let (_, code) = cli.run(&["attach", "ls", "--task-comment", &cid, "--json"]);
    assert_eq!(code, 1, "a deleted comment cannot resolve as an attach target");
    // A second rm is not_found: the row is gone.
    let (_, again) = cli.run(&["comment", "rm", &cid, "--yes", "--json"]);
    assert_eq!(again, 1);

    // Decision comments delete the same way.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let dc = cli.json(&["decision", "comment", "add", &did, "--text", "誤投稿", "--json"]);
    let dcid = id_str(&dc["comment"]["id"]);
    cli.json(&["decision", "comment", "rm", &dcid, "--yes", "--json"]);
    assert_eq!(cli.json(&["decision", "comment", "list", &did, "--json"])["count"], 0);
}

/// A post you only want to reword is rewritten in place by `comment edit`: id, position in the thread and
/// attachments all survive, unlike delete-and-repost. Decision comments mirror it, even under a frozen decision.
#[test]
fn comment_edit_rewrites_the_body_and_keeps_the_id_and_its_attachment() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    let c = cli.json(&["comment", "add", &tid, "--text", "誤字のある投稿", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    cli.json(&["comment", "add", &tid, "--text", "後の投稿", "--json"]);
    cli.json(&["comment", "attach", &cid, "https://example.com/", "--url", "--json"]);

    let edited = cli.json(&["comment", "edit", &cid, "--text", "直した投稿", "--json"]);
    assert_eq!(id_str(&edited["comment"]["id"]), cid, "the id does not change (not a new post)");

    let listed = cli.json(&["comment", "list", &tid, "--json"]);
    assert_eq!(listed["count"], 2, "the count neither grows nor shrinks");
    assert_eq!(listed["comments"][0]["text"], "直した投稿", "still first in oldest-first order = its position does not move");
    assert_eq!(listed["comments"][1]["text"], "後の投稿");
    assert_eq!(cli.json(&["attach", "ls", "--task-comment", &cid, "--json"])["count"], 1, "the attachment that was there remains");

    // An empty body is refused, and so is editing a comment that is not there — unlike delete, it is no no-op.
    let (_, empty) = cli.run(&["comment", "edit", &cid, "--text", "  ", "--json"]);
    assert_eq!(empty, 1);
    let (_, gone) = cli.run(&["comment", "edit", "9999", "--text", "x", "--json"]);
    assert_eq!(gone, 1);

    // Decision comments take the same shape, and stay editable under an accepted decision: what freezes is the decision's body.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let dc = cli.json(&["decision", "comment", "add", &did, "--text", "誤字のある投稿", "--json"]);
    let dcid = id_str(&dc["comment"]["id"]);
    cli.json(&["decision", "accept", &did, "--json"]);
    cli.json(&["decision", "comment", "edit", &dcid, "--text", "直した投稿", "--json"]);
    let dlisted = cli.json(&["decision", "comment", "list", &did, "--json"]);
    assert_eq!(dlisted["count"], 1);
    assert_eq!(dlisted["comments"][0]["text"], "直した投稿");
}

/// An edited post says so. No revision history is kept, so `edited_at` is the only clue a reader gets that
/// the body is no longer the one they read — which counts for most when the writer is an AI. An untouched
/// post stays quiet: the mark appears only where there is a fact to report.
#[test]
fn an_edited_comment_says_so_and_an_untouched_one_stays_quiet() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    let c = cli.json(&["comment", "add", &tid, "--text", "誤字のある投稿", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    cli.json(&["comment", "add", &tid, "--text", "触らない投稿", "--json"]);

    // Before any edit nothing is marked: updated_at equals created_at on insert, which is not "edited".
    let before = cli.json(&["comment", "list", &tid, "--json"]);
    assert!(
        before["comments"].as_array().unwrap().iter().all(|c| c["edited_at"].is_null()),
        "a merely-posted comment shows no edited mark: {before}"
    );

    cli.json(&["comment", "edit", &cid, "--text", "直した投稿", "--json"]);
    let after = cli.json(&["comment", "list", &tid, "--json"]);
    assert!(after["comments"][0]["edited_at"].is_string(), "the edited post has an edited-at time: {after}");
    assert!(after["comments"][1]["edited_at"].is_null(), "an untouched post stays quiet: {after}");

    // The human output says the same thing, and only on the line that was edited.
    let (human, code) = cli.run(&["comment", "list", &tid]);
    assert_eq!(code, 0);
    let edited_line = human.lines().find(|l| l.contains("直した投稿")).expect("the edited row exists");
    let quiet_line = human.lines().find(|l| l.contains("触らない投稿")).expect("the untouched row exists");
    assert!(edited_line.contains("edited"), "the edited row says edited: {edited_line}");
    assert!(!quiet_line.contains("edited"), "the untouched row does not: {quiet_line}");

    // Decision comments mirror it.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let dc = cli.json(&["decision", "comment", "add", &did, "--text", "誤字のある投稿", "--json"]);
    let dcid = id_str(&dc["comment"]["id"]);
    assert!(cli.json(&["decision", "comment", "list", &did, "--json"])["comments"][0]["edited_at"].is_null());
    cli.json(&["decision", "comment", "edit", &dcid, "--text", "直した投稿", "--json"]);
    let dlisted = cli.json(&["decision", "comment", "list", &did, "--json"]);
    assert!(dlisted["comments"][0]["edited_at"].is_string(), "a decision comment also says edited: {dlisted}");
}

/// The edited mark shows on `activity` too, the main way a timeline is read. It exists so a human notices
/// that an AI rewrote its own post — staying quiet on the surface both of them use most would defeat it.
#[test]
fn the_timeline_says_a_comment_was_edited_too() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let c = cli.json(&["comment", "add", &tid, "--text", "誤字のある投稿", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    cli.json(&["comment", "add", &tid, "--text", "触らない投稿", "--json"]);

    let comment_rows = |v: &Value| -> Vec<Value> {
        v["items"].as_array().unwrap().iter().filter(|i| i["type"] == "comment").cloned().collect()
    };
    let before = cli.json(&["activity", "--task", &tid, "--json"]);
    assert!(
        comment_rows(&before).iter().all(|i| i["edited_at"].is_null()),
        "a merely-posted row shows no edited mark: {before}"
    );

    cli.json(&["comment", "edit", &cid, "--text", "直した投稿", "--json"]);
    let after = cli.json(&["activity", "--task", &tid, "--json"]);
    let rows = comment_rows(&after);
    let edited = rows.iter().find(|i| i["text"] == "直した投稿").expect("the edited row exists");
    let quiet = rows.iter().find(|i| i["text"] == "触らない投稿").expect("the untouched row exists");
    assert!(edited["edited_at"].is_string(), "the edited row has an edited-at time: {after}");
    assert!(quiet["edited_at"].is_null(), "the untouched row stays quiet: {after}");

    // The human timeline line says the same thing.
    let (human, code) = cli.run(&["activity", "--task", &tid]);
    assert_eq!(code, 0);
    let edited_line = human.lines().find(|l| l.contains("直した投稿")).expect("the edited row exists");
    let quiet_line = human.lines().find(|l| l.contains("触らない投稿")).expect("the untouched row exists");
    assert!(edited_line.contains("edited"), "the edited row says edited: {edited_line}");
    assert!(!quiet_line.contains("edited"), "the untouched row does not: {quiet_line}");
}

/// A decision says whether the work it spawned is **still standing**: the linked tasks of `decision show`
/// carry a status beside id and title (`linked_tasks[].status` in `--json`), a finished task sinks to `[x]`
/// in the human output, and only what is moving or stuck names its status — `todo` is the default and stays quiet.
#[test]
fn a_decision_says_which_of_the_tasks_it_created_are_still_standing() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let did = id_str(
        &cli.json(&["decision", "add", "--project", &pid, "--title", "畳み込みは 1 回で束ねる", "--json"])
            ["decision"]["id"],
    );
    // Tasks under an unsettled decision cannot be started, so accept it before moving any status.
    cli.json(&["decision", "accept", &did, "--json"]);

    let a_task = |title: &str| -> String {
        id_str(&cli.json(&["task", "add", "--project", &pid, "--title", title, "--json"])["task"]["id"])
    };
    let todo = a_task("まだ手を付けていない");
    let doing = a_task("いま進めている");
    let done = a_task("終わった");
    let blocked = a_task("外の事情で止まっている");
    for tid in [&todo, &doing, &done, &blocked] {
        cli.json(&["decision", "link", &did, tid, "--json"]);
    }
    cli.json(&["task", "status", &doing, "in_progress", "--json"]);
    cli.json(&["task", "done", &done, "--json"]);
    cli.json(&["task", "status", &blocked, "blocked", "--json"]);

    let shown = cli.json(&["decision", "show", &did, "--json"]);
    let status_of = |tid: &str| -> String {
        shown["linked_tasks"]
            .as_array()
            .expect("linked_tasks is an array")
            .iter()
            .find(|t| id_str(&t["id"]) == tid)
            .unwrap_or_else(|| panic!("the linked task {tid} is missing: {shown}"))["status"]
            .as_str()
            .expect("status is a string")
            .to_string()
    };
    assert_eq!(status_of(&todo), "todo");
    assert_eq!(status_of(&doing), "in_progress");
    assert_eq!(status_of(&done), "done");
    assert_eq!(status_of(&blocked), "blocked");

    let (human, code) = cli.run(&["decision", "show", &did]);
    assert_eq!(code, 0);
    let line = |title: &str| -> String {
        human
            .lines()
            .find(|l| l.contains(title))
            .unwrap_or_else(|| panic!("the row for {title} exists: {human}"))
            .to_string()
    };
    assert!(line("終わった").contains("[x]"), "completed sinks: {}", line("終わった"));
    assert!(line("いま進めている").contains("(in_progress)"), "work in motion names itself");
    assert!(line("外の事情で止まっている").contains("(blocked)"), "stalled work also names itself");
    let untouched = line("まだ手を付けていない");
    assert!(untouched.contains("[ ]"), "incomplete is unchecked: {untouched}");
    assert!(!untouched.contains('('), "a default todo does not name its status: {untouched}");
}

/// `task list --filter commit:<sha>` walks the reverse chain **git → task**: a public commit carries no
/// store-local ref, so the only face back is the SHA recorded on the task. The
/// SHA folds to the bytes the door stored, the same commit on two tasks finds both, and a SHA nobody
/// recorded — a short one included, since the door admits full hex only — is an empty result, not an
/// error (a SHA is a free value, not a name the store knows); only an empty value is refused.
#[test]
fn commit_filter_walks_git_back_to_the_task() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();

    let a_task = |title: &str| -> String {
        id_str(&cli.json(&["task", "add", "--project", &pid, "--title", title, "--json"])["task"]["id"])
    };
    let t1 = a_task("one");
    let t2 = a_task("two");
    let _t3 = a_task("three");

    let sha_a = "a".repeat(40); // SHA-1 form
    let sha_b = "b".repeat(64); // SHA-256 form
    cli.json(&["task", "commit", "add", &t1, &sha_a, "--json"]);
    cli.json(&["task", "commit", "add", &t2, &sha_a, "--json"]); // the same commit on two tasks
    cli.json(&["task", "commit", "add", &t2, &sha_b, "--json"]);

    // Sorted task ids a `commit:` filter returns.
    let ids_for = |sha: &str| -> Vec<String> {
        let mut ids: Vec<String> = cli.json(&["task", "list", "--project", &pid, "--filter", &format!("commit:{sha}"), "--json"])
            ["tasks"]
            .as_array()
            .expect("tasks is an array")
            .iter()
            .map(|t| id_str(&t["id"]))
            .collect();
        ids.sort();
        ids
    };

    let mut both = vec![t1.clone(), t2.clone()];
    both.sort();
    assert_eq!(ids_for(&sha_a), both, "both tasks that recorded the commit come back");
    assert_eq!(ids_for(&sha_b), vec![t2.clone()], "the SHA-256 form finds its one task");
    assert_eq!(ids_for(&sha_a.to_uppercase()), both, "an upper-case SHA folds to the stored lower-case bytes");
    assert!(ids_for(&"c".repeat(40)).is_empty(), "a full SHA nobody recorded is an empty result, not an error");
    assert!(ids_for("abc1234").is_empty(), "a short SHA is never stored, so it simply matches nothing (not rejected)");

    // Only an empty value is no SHA at all — refused (a non-zero exit), unlike an unknown SHA.
    let (_out, code) = cli.run(&["task", "list", "--project", &pid, "--filter", "commit:", "--json"]);
    assert_ne!(code, 0, "an empty commit value is refused, not treated as match-nothing");
}

/// `builds_on` hands a machine two things: read the premise first, and revisit when the premise is
/// overturned. Three surfaces carry it — the premise list of `decision show`, the note on an overturned
/// premise, and the blast radius named when one is superseded, rejected or deleted. It names (one hop, not transitive); it never blocks.
#[test]
fn a_premise_is_read_first_and_its_overturn_names_what_to_revisit() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();

    let add = |title: &str| -> String {
        id_str(&cli.json(&["decision", "add", "--project", &pid, "--title", title, "--json"])["decision"]["id"])
    };
    let premise = add("同期は撤去する");
    let standing = add("削除は物理削除にする");
    cli.json(&["decision", "accept", &premise, "--json"]);
    cli.json(&["decision", "accept", &standing, "--json"]);

    // Draw the premise edge: neither decision moves, and the premise stays current.
    let built = cli.json(&["decision", "builds-on", &standing, "--on", &premise, "--json"]);
    assert_eq!(built["action"], "decision.builds_on");
    let shown = cli.json(&["decision", "show", &standing, "--json"]);
    assert_eq!(id_str(&shown["builds_on"][0]["id"]), premise, "premise = the decision to read first");
    assert_eq!(shown["builds_on"][0]["current"], true, "the premise stays current (not greyed out)");
    // The reverse lookup is the blast radius — what needs revisiting if this decision is overturned.
    let from_premise = cli.json(&["decision", "show", &premise, "--json"]);
    assert_eq!(id_str(&from_premise["built_on_by"][0]["id"]), standing);

    // Overturning the premise names what must be revisited, and lets the operation through.
    let successor = add("同期をやり直す");
    let sup = cli.json(&["decision", "supersede", &successor, "--replaces", &premise, "--json"]);
    assert_eq!(sup["ok"], true, "it only surfaces = supersede succeeds");
    assert_eq!(id_str(&sup["decision"]["revisit"][0]["id"]), standing, "names the decision standing on the rotted premise");

    // Open the standing decision and the overturned premise is right there.
    let after = cli.json(&["decision", "show", &standing, "--json"]);
    assert_eq!(after["builds_on"][0]["current"], false, "the premise is no longer current");
    assert_eq!(after["builds_on"][0]["superseded_by"], amenbo_core::idref::decision(successor.parse().unwrap()), "names the successor");
}

/// `.amenbo` does not say which store to open — it **is** the AI's reach. Rows outside the binding are never
/// shown to the AI: seeing them pulls their content into the session context, from where it bleeds into
/// summaries, memory and commit messages. Humans are not confined; the overview is theirs.
#[test]
fn an_ai_reads_only_the_project_its_folder_is_bound_to() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = id_str(&cli.json(&["project", "list", "--json"])["projects"][0]["id"]);
    let other = id_str(&cli.json(&["project", "add", "--name", "Other", "--json"])["project"]["id"]);

    let mine = id_str(&cli.json(&["task", "add", "--title", "mine", "--project", &bound, "--json"])["task"]["id"]);
    let theirs = id_str(&cli.json(&["task", "add", "--title", "theirs", "--project", &other, "--json"])["task"]["id"]);
    cli.json(&["decision", "add", "--title", "mine-dec", "--project", &bound, "--json"]);
    let their_dec = id_str(
        &cli.json(&["decision", "add", "--title", "their-dec", "--project", &other, "--json"])["decision"]["id"],
    );

    // The human sees the whole device — only the AI facet is confined.
    let all = cli.json(&["task", "list", "--json"]);
    assert_eq!(all["count"], 2, "the human sees tasks from both projects");
    assert_eq!(cli.json(&["decision", "list", "--json"])["count"], 2, "decisions likewise");
    assert_eq!(cli.json(&["project", "list", "--json"])["count"], 2);

    // The AI sees the bound project alone: listings, timeline, status and the project list all stop at the binding.
    let ai_tasks = cli.json(&["task", "list", "--actor", "ai", "--json"]);
    assert_eq!(ai_tasks["count"], 1, "the AI sees only the bound project's tasks");
    assert_eq!(ai_tasks["tasks"][0]["title"], "mine");
    let ai_decisions = cli.json(&["decision", "list", "--actor", "ai", "--json"]);
    assert_eq!(ai_decisions["count"], 1, "decisions too, only the bound project");
    assert_eq!(ai_decisions["decisions"][0]["title"], "mine-dec");
    let ai_projects = cli.json(&["project", "list", "--actor", "ai", "--json"]);
    assert_eq!(ai_projects["count"], 1, "not even another project's name enters the context");
    assert_eq!(id_str(&ai_projects["projects"][0]["id"]), bound, "what is visible is the bound project itself");
    let ai_activity = cli.json(&["activity", "--actor", "ai", "--json"]);
    assert!(
        ai_activity["items"].as_array().unwrap().iter().all(|i| i["title"] != "theirs"),
        "out-of-binding events do not flow into the timeline either: {ai_activity}"
    );
    let ai_status = cli.json(&["status", "--actor", "ai", "--json"]);
    assert_eq!(ai_status["counts"]["no_due"], 1, "status counts stay within the bound project too");

    // Naming something outside by id is out_of_reach, not not_found: its existence is not denied, its reach is.
    for args in [
        vec!["task", "show", theirs.as_str(), "--actor", "ai", "--json"],
        vec!["comment", "list", theirs.as_str(), "--actor", "ai", "--json"],
        vec!["decision", "show", their_dec.as_str(), "--actor", "ai", "--json"],
        vec!["project", "show", other.as_str(), "--actor", "ai", "--json"],
    ] {
        let (err, code) = cli.run_err(&args);
        assert_ne!(code, 0, "{args:?} is out of the binding and must not pass");
        assert!(err.contains("out_of_reach"), "{args:?} is out_of_reach: {err}");
        assert!(!err.contains("not_found"), "{args:?} does not lie that it does not exist: {err}");
    }

    // Ids inside the bound project read normally for the AI — only the outside is closed.
    assert_eq!(cli.json(&["task", "show", &mine, "--actor", "ai", "--json"])["title"], "mine");

    // Neither `--project` nor `project:` widens the reach, so the binding never decays into decoration.
    for args in [
        vec!["--project", other.as_str(), "task", "list", "--actor", "ai", "--json"],
        vec!["task", "list", "--filter", "project:Other", "--actor", "ai", "--json"],
    ] {
        let (err, code) = cli.run_err(&args);
        assert_ne!(code, 0, "{args:?} cannot widen the reach");
        assert!(err.contains("out_of_reach"), "{args:?} is out_of_reach: {err}");
    }
}

/// Diagnostics (`validate` / `doctor`) ask whether anything is broken, but what they hand back is the ids and
/// titles of tasks and the absolute folder paths of other projects — read them and those projects are in the
/// AI's context, so they narrow to the binding. Export is **not** closed off: only the path that streams
/// content into the context (no destination, i.e. stdout) is diverted to a file, rather than refused.
#[test]
fn an_ais_diagnostics_and_export_stay_inside_the_project_it_is_bound_to() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = cli.bound_project();
    let other = id_str(&cli.json(&["project", "add", "--name", "Other", "--json"])["project"]["id"]);
    cli.json(&["task", "add", "--title", "mine", "--project", &bound, "--json"]);
    let theirs = id_str(&cli.json(&["task", "add", "--title", "theirs", "--project", &other, "--json"])["task"]["id"]);

    // validate: the human checks the whole device, the AI only the bound project — `checked` is the reach itself.
    assert_eq!(cli.json(&["validate", "--json"])["checked"], 2, "the human validates both projects");
    assert_eq!(
        cli.json(&["validate", "--actor", "ai", "--json"])["checked"],
        1,
        "the AI validates only the bound project's tasks"
    );

    // Checking something outside is out_of_reach, not an empty result: never "it does not exist".
    let (err, code) = cli.run_err(&["validate", &theirs, "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "a task out of the binding cannot be validated");
    assert!(err.contains("out_of_reach"), "returns out_of_reach: {err}");

    // doctor: no other project's folder path, and no project id, reaches the AI's surface. Register a folder
    // outside the binding and remove its pointer, so doctor lists it as a bound folder whose `.amenbo` vanished.
    // It must sit outside this CWD — no `.amenbo` may stand above a bound folder (bind's ancestor guard).
    let stray = scratch("stray");
    std::fs::create_dir_all(&stray).unwrap();
    let stray_path = stray.display().to_string();
    cli.json(&["bind", "--project", &other, "--dir", &stray_path, "--json"]);
    std::fs::remove_file(stray.join(".amenbo")).unwrap();

    let human_doctor = cli.json(&["doctor", "--json"]).to_string();
    assert!(human_doctor.contains("stray"), "the human sees this machine's bound folders: {human_doctor}");
    let ai_doctor = cli.json(&["doctor", "--actor", "ai", "--json"]).to_string();
    assert!(
        !ai_doctor.contains("stray"),
        "an out-of-binding project's folder does not enter the AI's context: {ai_doctor}"
    );

    // export: the AI can take the whole device out too — no lock-in, not even for an agent. Only the stdout
    // path is closed: with the destination omitted it writes a file and returns just the path and the counts.
    let streamed = cli.json(&["export"]).to_string();
    assert!(streamed.contains("theirs"), "the human's export streams the whole machine to stdout");

    let ai_export = cli.json(&["export", "--actor", "ai", "--json"]);
    assert_eq!(ai_export["action"], "export", "the AI's export passes (not refused)");
    let out = ai_export["out"].as_str().expect("returns the output destination").to_string();
    assert!(
        !ai_export.to_string().contains("theirs"),
        "only the path and counts are returned = the content does not flow into the context: {ai_export}"
    );
    // The destination is relative to the CLI's CWD, not the test process's.
    let dumped = std::fs::read_to_string(cli.home.join(&out).join("export.json")).unwrap();
    assert!(dumped.contains("theirs"), "the exported file holds the whole machine (unfiltered)");
}

/// **An AI does not pick a project** — the binding does. `--project` and the `project:` filter are human
/// vocabulary: an AI passing either is an error, even when it names the bound project itself (no silent
/// ignore, no silent fallback). The flip side: an AI's `task add` needs no `--project` and lands in the binding.
#[test]
fn an_ai_does_not_pick_a_project_the_binding_does() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = cli.bound_project();
    let bound_name = cli.json(&["project", "list", "--json"])["projects"][0]["name"]
        .as_str()
        .unwrap()
        .to_string();

    // Even naming the bound project is refused: the AI is left no vocabulary for choosing, reach aside.
    for args in [
        vec!["task", "add", "--title", "x", "--project", bound.as_str(), "--json"],
        vec!["task", "list", "--project", bound.as_str(), "--json"],
        vec!["decision", "add", "--title", "x", "--project", bound.as_str(), "--json"],
        vec!["decision", "list", "--project", bound.as_str(), "--json"],
        vec!["activity", "--project", bound.as_str(), "--json"],
        vec!["task", "list", "--filter", &format!("project:{bound_name}"), "--json"],
    ] {
        let (err, code) = cli.run_err(&[args.clone(), vec!["--actor", "ai"]].concat());
        assert_ne!(code, 0, "{args:?} does not pass from the AI (the binding decides)");
        assert!(err.contains("out_of_reach"), "{args:?} is out_of_reach: {err}");
    }

    // The human is not confined; the overview is theirs.
    assert_eq!(cli.json(&["task", "list", "--project", &bound, "--json"])["count"], 0);

    // The AI writes with no `--project` and the binding fills the place in — a project-less task stays impossible.
    let t = cli.json(&["task", "add", "--title", "束縛が決める", "--actor", "ai", "--json"]);
    assert_eq!(id_str(&t["task"]["placement"]["project"]["id"]), bound);
    let d = cli.json(&["decision", "add", "--title", "束縛が決める", "--actor", "ai", "--json"]);
    assert_eq!(id_str(&d["decision"]["project"]["id"]), bound);
    assert_eq!(cli.json(&["task", "list", "--actor", "ai", "--json"])["count"], 1);
}

/// **Writes** stay inside the bound project too. Paths that resolve a ref before mutating (`task status`,
/// `decision amend`, …) are already closed by the read-side resolver, so what is watched here is the mutations
/// that bypass it: raw comment and attachment ids, and creating new entities. The guard stands **before** the mutation.
#[test]
fn an_ai_writes_only_inside_the_project_its_folder_is_bound_to() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = cli.bound_project();
    let other = id_str(&cli.json(&["project", "add", "--name", "Other", "--json"])["project"]["id"]);

    // What the human sets up outside the binding: a task, a decision, comments on both, and an attachment.
    let theirs =
        id_str(&cli.json(&["task", "add", "--title", "theirs", "--project", &other, "--json"])["task"]["id"]);
    let their_dec = id_str(
        &cli.json(&["decision", "add", "--title", "their-dec", "--project", &other, "--json"])["decision"]["id"],
    );
    let their_comment =
        id_str(&cli.json(&["comment", "add", &theirs, "--text", "彼らの投稿", "--json"])["comment"]["id"]);
    let their_dec_comment = id_str(
        &cli.json(&["decision", "comment", "add", &their_dec, "--text", "彼らの投稿", "--json"])["comment"]["id"],
    );
    let their_attachment = id_str(
        &cli.json(&["task", "attach", &theirs, "https://example.com/spec", "--url", "--json"])["attachment"]["id"],
    );

    // Comment ids and attachment ids are not conversational refs, so they pass through no `resolve_*_ref` —
    // and still nothing outside the binding may be mutated.
    for args in [
        vec!["comment", "edit", their_comment.as_str(), "--text", "書き換え", "--json"],
        vec!["comment", "rm", their_comment.as_str(), "--yes", "--json"],
        vec!["decision", "comment", "edit", their_dec_comment.as_str(), "--text", "書き換え", "--json"],
        vec!["decision", "comment", "rm", their_dec_comment.as_str(), "--yes", "--json"],
        vec!["attach", "rm", their_attachment.as_str(), "--yes", "--json"],
    ] {
        let (err, code) = cli.run_err(&[args.clone(), vec!["--actor", "ai"]].concat());
        assert_ne!(code, 0, "{args:?} is an out-of-binding mutation and must not pass");
        assert!(err.contains("out_of_reach"), "{args:?} is out_of_reach: {err}");
    }
    // The refusal comes before the mutation: nothing outside the binding changed.
    assert_eq!(cli.json(&["comment", "list", &theirs, "--json"])["count"], 1);
    assert_eq!(cli.json(&["decision", "comment", "list", &their_dec, "--json"])["count"], 1);
    assert_eq!(cli.json(&["attach", "ls", &format!("T-{theirs}"), "--json"])["count"], 1);

    // A new entity has no id yet, so the check is on **where it would land**: not in a project outside the
    // binding, and not in a brand-new project its own creator could never reach.
    for args in [
        vec!["task", "add", "--title", "潜り込み", "--project", other.as_str(), "--json"],
        vec!["decision", "add", "--title", "潜り込み", "--project", other.as_str(), "--json"],
        vec!["project", "add", "--name", "ai pj", "--json"],
    ] {
        let (err, code) = cli.run_err(&[args.clone(), vec!["--actor", "ai"]].concat());
        assert_ne!(code, 0, "{args:?} creates an entity outside the binding");
        assert!(err.contains("out_of_reach"), "{args:?} is out_of_reach: {err}");
    }
    assert_eq!(cli.json(&["project", "list", "--json"])["count"], 2, "no project was added");

    // Inside the binding, writing works as usual, and the binding fills in where things land.
    let created = cli.json(&["task", "add", "--title", "mine", "--actor", "ai", "--json"]);
    assert_eq!(
        id_str(&created["task"]["placement"]["project"]["id"]),
        bound,
        "a task the AI creates lands in the bound project"
    );
    let mine = id_str(&created["task"]["id"]);
    let mine_comment =
        id_str(&cli.json(&["comment", "add", &mine, "--text", "進捗", "--actor", "ai", "--json"])["comment"]["id"]);
    let edited = cli.json(&["comment", "edit", &mine_comment, "--text", "進捗（直した）", "--actor", "ai", "--json"]);
    assert_eq!(edited["comment"]["text"], "進捗（直した）");
}

/// An AI in an unbound folder is given nothing — **neither reads nor writes**. Reach is drawn from the
/// binding alone, so with no binding the reach is empty; falling back to All would mean an unconfined AI
/// seeing everything, and `.amenbo` decaying into decoration. The human is not confined: an unbound CWD is the overview.
#[test]
fn an_ai_in_an_unbound_folder_reaches_nothing() {
    // No `init`, so the CWD carries no `.amenbo`. The store still opens through `AMENBO_HOME`, so the
    // no_pointer execution guard is passed — what is closed here is everything beyond it.
    let cli = Cli::new();
    let pid = cli.a_project();
    cli.json(&["task", "add", "--title", "human の仕事", "--project", &pid, "--json"]);

    // The human sees the store; the overview is theirs.
    assert_eq!(cli.json(&["task", "list", "--json"])["count"], 1);

    // The AI can neither read nor write nor take stock, and hears "unreachable", not "does not exist".
    for args in [
        vec!["task", "list", "--json"],
        vec!["decision", "list", "--json"],
        vec!["project", "list", "--json"],
        vec!["activity", "--json"],
        vec!["status", "--json"],
        // Writes are no different: the AI cannot pass `--project`, so the place comes from a binding it lacks.
        vec!["task", "add", "--title", "潜り込み", "--json"],
    ] {
        let (err, code) = cli.run_err(&[args.clone(), vec!["--actor", "ai"]].concat());
        assert_ne!(code, 0, "{args:?} is the AI of an unbound folder and must not pass");
        assert!(err.contains("out_of_reach"), "{args:?} is out_of_reach: {err}");
        assert!(!err.contains("not_found"), "{args:?} does not lie that it does not exist: {err}");
    }
    assert_eq!(cli.json(&["task", "list", "--json"])["count"], 1, "the refusal is before the mutation = the store is unchanged");

    // The one exception: `init` is the operation that creates a binding, so an AI may run it — and then works normally inside it.
    let fresh = Cli::new();
    let (_out, code) = fresh.run(&["init", "--name", "tester", "--actor", "ai"]);
    assert_eq!(code, 0, "init is the very act of creating a binding = it passes even for the AI");
    let t = fresh.json(&["task", "add", "--title", "束縛の中", "--actor", "ai", "--json"]);
    assert_eq!(t["task"]["title"], "束縛の中");
    assert_eq!(
        id_str(&t["task"]["placement"]["project"]["id"]),
        fresh.bound_project(),
        "the home is filled from the binding"
    );
}

/// **Reads that name a raw id**, bypassing the conversational refs, must not show anything outside the
/// binding either: attachment ids in `attach show/open/ls`, comment ids in `attach ls --task-comment` and
/// `decision promote`, dimension ids and names in `dimension show`. None goes through `resolve_*_ref`, so
/// the resolver's guard alone does not close them — and filenames, URLs and comment bodies are the content.
#[test]
fn an_ai_cannot_read_an_out_of_reach_entity_by_its_raw_id() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = cli.bound_project();
    let other = id_str(&cli.json(&["project", "add", "--name", "Other", "--json"])["project"]["id"]);

    // What the human sets up outside the binding: a task, its attachment, a comment, and a dimension.
    let theirs =
        id_str(&cli.json(&["task", "add", "--title", "theirs", "--project", &other, "--json"])["task"]["id"]);
    let their_attachment = id_str(
        &cli.json(&["task", "attach", &theirs, "https://example.com/their-spec", "--url", "--json"])
            ["attachment"]["id"],
    );
    let their_comment =
        id_str(&cli.json(&["comment", "add", &theirs, "--text", "彼らの投稿", "--json"])["comment"]["id"]);
    cli.json(&["dimension", "add", "--project", &other, "--name", "their-axis", "--json"]);

    // Even holding the raw id, the AI cannot reach them — "unreachable", not "does not exist".
    for args in [
        vec!["attach", "show", their_attachment.as_str(), "--json"],
        vec!["attach", "open", their_attachment.as_str(), "--json"],
        vec!["attach", "ls", "--task-comment", their_comment.as_str(), "--json"],
        vec!["decision", "promote", their_comment.as_str(), "--title", "横取り", "--json"],
        vec!["dimension", "show", "their-axis", "--json"],
    ] {
        let (err, code) = cli.run_err(&[args.clone(), vec!["--actor", "ai"]].concat());
        assert_ne!(code, 0, "{args:?} must not let out-of-binding content be read");
        assert!(err.contains("out_of_reach"), "{args:?} is out_of_reach: {err}");
        assert!(!err.contains("not_found"), "{args:?} does not lie that it does not exist: {err}");
        assert!(!err.contains("their-spec"), "{args:?} does not leak the content: {err}");
    }

    // The same shapes inside the binding read normally for the AI: only the outside is closed.
    let mine = id_str(&cli.json(&["task", "add", "--title", "mine", "--project", &bound, "--json"])["task"]["id"]);
    let my_attachment = id_str(
        &cli.json(&["task", "attach", &mine, "https://example.com/my-spec", "--url", "--json"])["attachment"]["id"],
    );
    let my_comment =
        id_str(&cli.json(&["comment", "add", &mine, "--text", "私の投稿", "--json"])["comment"]["id"]);
    cli.json(&["dimension", "add", "--project", &bound, "--name", "my-axis", "--json"]);

    let shown = cli.json(&["attach", "show", &my_attachment, "--actor", "ai", "--json"]);
    assert_eq!(shown["url"], "https://example.com/my-spec");
    assert_eq!(cli.json(&["attach", "ls", "--task-comment", &my_comment, "--actor", "ai", "--json"])["count"], 0);
    assert_eq!(cli.json(&["dimension", "show", "my-axis", "--actor", "ai", "--json"])["dimension"]["name"], "my-axis");
    // The listing shows the bound project's axes alone, where the human sees both.
    assert_eq!(cli.json(&["dimension", "list", "--actor", "ai", "--json"])["count"], 1);
}

// ───────────────────────── lint ─────────────────────────

/// Run `lint` and return (stdout, stderr, exit code), optionally piping `stdin` in.
///
/// It takes no `Cli`, on purpose: `lint` must open no store, so these tests name none. `AMENBO_HOME`
/// points at a directory that does not exist and each test asserts it still does not afterwards — which
/// keeps the run hermetic (a regression that opened a store would create it there, never in the real
/// app-data tree) *and* is itself the evidence that no store was opened.
fn lint(cwd: &std::path::Path, home: &std::path::Path, args: &[&str], stdin: Option<&str>) -> (String, String, i32) {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", home)
        .env_remove("AMENBO_ACTOR") // a read stamps no facet, so `--json` here must not want one
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
        out.status.code().unwrap_or(-1),
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
/// Fresh is what [`scratch`] hands back, and this function needs it to be: when a recycled pid once named
/// this repository, it already existed, already had `a.rs` at exactly this content, and already had the
/// `base` commit. `git add -A` then staged nothing and `git commit -qm base` exited non-zero saying
/// "nothing to commit" on stdout, which `-q` swallowed — a rare, silent, empty-stderr failure of an
/// unrelated test.
fn a_repo() -> std::path::PathBuf {
    let dir = scratch("repo");
    std::fs::create_dir_all(&dir).unwrap();
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
    let home = temp_home();
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
    let home = temp_home();
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
    let home = temp_home();

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
    let dir = scratch("plain");
    std::fs::create_dir_all(&dir).unwrap();
    let home = temp_home();

    let (_, err, code) = lint(&dir, &home, &["--json"], None);
    assert_ne!(code, 0);
    assert!(err.contains("Not a git repository"), "says what is wrong: {err}");
    assert!(err.lines().count() < 12, "and does not reprint git's manual: {err}");

    // The text faces need no repository at all: they read what they are handed.
    let (_, _, code) = lint(&dir, &home, &["--stdin"], Some("clean\n"));
    assert_eq!(code, 0, "lint needs no repository to read piped text");
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
            .env("AMENBO_ACTOR", "human")
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("PATH", &shim_dir)
            .current_dir(&cli.home)
            .args(args)
            .output()
            .expect("failed to run the binary");
        assert_eq!(out.status.code(), Some(0), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        std::fs::read_to_string(&log).map(|s| s.lines().count()).unwrap_or(0)
    };

    assert_eq!(spawns(&["task", "list", "--json"]), 1, "an ordinary command probes the hooks once");
    assert_eq!(spawns(&["hooks", "status", "--json"]), 1, "and the hooks' own faces do not probe twice");

    Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_ACTOR", "human")
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&cli.home)
        .args(["hooks", "install"])
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

/// A reader that walks away ends the run quietly, rather than crashing it.
///
/// The Rust runtime ignores SIGPIPE on startup, and with it ignored every printing macro panics on a
/// write it cannot make — so `amenbo task list | head` used to end in a panic message and exit 101.
/// `main` hands the signal back, which is what this pins.
///
/// `agent --full` is the fixture because it is the one command that overflows a pipe buffer off a
/// bare `init`: the writer is still writing when the reader goes, which is the only way to reach the
/// failing write at all. A smaller output lands whole in the buffer and the write always succeeds.
#[cfg(unix)]
#[test]
fn a_closed_pipe_ends_the_run_without_a_panic() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::Stdio;

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let mut child = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_ACTOR", "human")
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&cli.home)
        .args(["agent", "--full"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run the binary");

    // What `head` does after its last line: the read end goes, and the next write finds no reader.
    drop(child.stdout.take().expect("stdout was piped"));

    let out = child.wait_with_output().expect("failed to wait for the binary");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(
        out.status.signal(),
        Some(libc::SIGPIPE),
        "expected the kernel's SIGPIPE to end the run, got {:?} (stderr: {stderr})",
        out.status
    );
    assert!(!stderr.contains("panicked"), "the run should say nothing on its way out (stderr: {stderr})");
}

/// Every body option takes `-` as "the body comes in on stdin". Bodies here are Markdown thick with
/// code spans, and a shell eats those out of a quoted argument by command substitution — silently,
/// taking the word with it — so the text has to be able to arrive without word expansion at all.
/// The value is passed through byte for byte, and a value that is not `-` is still the text itself.
#[test]
fn a_body_option_reads_stdin_on_dash() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "本文PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    // A body of the kind actually written here: code spans, which a shell would eat.
    const BODY: &str = "`--text` に直書きすると消える\n\n- `a` と `b`\n";

    let t = cli.json_stdin(&["task", "add", "--title", "本文", "--project", &pid, "--notes", "-", "--json"], BODY);
    let tid = id_str(&t["task"]["id"]);
    assert_eq!(t["task"]["notes"], BODY, "task add --notes - takes the body off stdin");

    let e = cli.json_stdin(&["task", "update", &tid, "--notes", "-", "--json"], "書き換え `x`");
    assert_eq!(e["task"]["notes"], "書き換え `x`", "task update --notes - too");

    let c = cli.json_stdin(&["comment", "add", &tid, "--text", "-", "--json"], BODY);
    assert_eq!(c["comment"]["text"], BODY, "comment add --text - too");

    let d = cli.json_stdin(&["decision", "add", "--title", "決定", "--project", &pid, "--body", "-", "--json"], BODY);
    let did = id_str(&d["decision"]["id"]);
    assert_eq!(d["decision"]["body"], BODY, "decision add --body - too");

    let dc = cli.json_stdin(&["decision", "comment", "add", &did, "--text", "-", "--json"], BODY);
    assert_eq!(dc["comment"]["text"], BODY, "decision comment add --text - too");

    // The ordinary path is untouched: a value that is not `-` is the body, stdin unread.
    let plain = cli.json(&["comment", "add", &tid, "--text", "そのまま", "--json"]);
    assert_eq!(plain["comment"]["text"], "そのまま", "a non-dash value is still the text");
}
