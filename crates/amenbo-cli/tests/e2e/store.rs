//! The store as a whole: the config it carries, the version and format it reports, taking every
//! table out (export) and a snapshot back in (backup / restore), and updating the binary that opens
//! it.

mod harness;

use std::process::Command;

use serde_json::Value;

use harness::*;

/// `amenbo export` streams the whole single-DB store out as portable JSON (a thin wrapper over core's
/// `export_json`). There is exactly one shape — no excerpts, no markdown/csv. The envelope carries an
/// `amenbo_export` header, and its reader lives outside Amenbo: nothing reads it back in.
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

/// The recovery has to work on the store it exists for. There is no downgrade, so the only way back from a
/// store a newer Amenbo carried past this build is the pre-migration backup — which means `restore` has to
/// run on exactly the store every other command refuses (`format_ahead`). It replaces the truth source
/// wholesale, so it never reads the one it replaces, and it therefore sits ahead of the open.
#[test]
fn restore_replaces_a_store_this_build_cannot_open() {
    use amenbo_core::store_engine::{StoreEngine, META_FORMAT_VERSION, META_FORMAT_VERSION_SET_BY};

    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "退避", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    for title in ["移行前A", "移行前B"] {
        cli.json(&["task", "add", "--title", title, "--project", &pid, "--json"]);
    }
    let archive = cli.home.join("pre-migrate.amenbo-backup");
    cli.json(&["backup", archive.to_str().unwrap(), "--json"]);

    // Put the live store one generation past what this build opens — what a newer Amenbo's migration
    // leaves behind on a device whose other copy is still the old one.
    {
        let engine = StoreEngine::open(&cli.home.join("store.sqlite")).unwrap();
        let ahead = (amenbo_core::model::FORMAT_VERSION + 1).to_string();
        engine.set_meta(META_FORMAT_VERSION, Some(&ahead)).unwrap();
        engine.set_meta(META_FORMAT_VERSION_SET_BY, Some("99.0.0")).unwrap();
    }

    // The gate is real: an ordinary command is refused, and it names the version to use.
    let (err, code) = cli.run_err(&["task", "list", "--json"]);
    assert_ne!(code, 0, "a too-new store is not opened: {err}");
    assert!(err.contains("99.0.0"), "the refusal names the version that wrote the store: {err}");

    // Restore goes through anyway — and the store it hands back is openable.
    let restored = cli.json(&["restore", archive.to_str().unwrap(), "--yes", "--json"]);
    assert!(
        restored["previous_saved_to"].as_str().is_some(),
        "the store that could not be opened is still set aside, not discarded: {restored}"
    );
    let all = cli.json(&["task", "list", "--json"]);
    assert_eq!(all["count"], 2, "the archive's store is back and readable");
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
                .env("AMENBO_UPDATE_CHECK", "0") // no update check (hermetic)
                // A non-interactive write declares a facet.
                .args(["task", "add", "--title", &format!("t{i}"), "--project", &pid, "--actor", "human"])
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
            .env("AMENBO_UPDATE_CHECK", "0") // no update check (hermetic)
            // Isolate the CWD into the temp home as well (as the other helpers do). Without it, init writes
            // `.amenbo`/AGENTS.md/CLAUDE.md into the test binary's CWD (crates/amenbo-cli) and dirties both the
            // source tree and the real app-data.
            .current_dir(home)
            // Writes such as init declare a facet.
            .args(with_defaults(args, "human"))
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

/// The `version` face says whether this binary came out of the release workflow. Pre-distribution
/// verification reads that answer to decide whether what is in front of it may be driven at all, and
/// nothing else about a running Amenbo tells the two apart — the version number is the released one
/// on both sides of a release. A binary built for a test is not a release artifact, so the honest
/// answer here is `false`; what it must never be is absent, since a missing answer is a driver that
/// cannot tell a shipped build from somebody's working tree.
#[test]
fn the_version_face_says_which_build_it_is() {
    let cli = Cli::new();
    let v = cli.json(&["version", "--json"]);
    assert_eq!(v["release_build"], false, "a build the release workflow did not produce says so: {v}");
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

/// `version` and `update` answer even in a bare, unbound dir — neither ever reads a store (version is a
/// fact about this build; update turns the published latest.json into an installer URL for this OS). With
/// no self-update, `update` is the only road for someone who wants a newer build, and answering "init
/// first" would shut out exactly the people stuck on an old one or fresh off an install. Reach already
/// confines the AI, so these two need no second gate by location — hence the ai facet here.
///
/// The manifest URL is overridden because a test binary carries no release stamp, and an unstamped
/// build declines `update` outright rather than naming an installer. The override says "ask here
/// instead", which is what puts this run back on the road a shipped build takes; the kill switch is
/// still what keeps it off the network, so nothing is fetched from the address either.
#[test]
fn version_and_update_answer_without_a_pointer() {
    let dir = temp_home();
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env_remove("AMENBO_HOME")
            .env_remove("AMENBO_PROJECT_DIR")
            .env("AMENBO_UPDATE_CHECK", "0") // no upstream lookup (hermetic)
            .env("AMENBO_UPDATE_JSON_URL", "http://127.0.0.1:1/latest.json") // a shipped build's road, without the endpoint
            .current_dir(&dir)
            .args(with_defaults(args, "ai"))
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

/// `update --apply` (CLI self-update) is wired the same store-free way as `update`, and stays graceful
/// when it cannot proceed. Two cheap, network-free checks of the wiring: `--apply` and `--print` are
/// mutually exclusive (clap turns them away), and with the upstream lookup disabled `--apply` cannot
/// reach the manifest and fails with a plain error pointing at the installer — never a self-replace on
/// nothing. The download-and-swap path is covered by `amenbo-core`'s `self_update` unit tests, which
/// exercise the version gate, the `.app` guard, and archive extraction without touching a real binary.
#[test]
fn update_apply_declines_gracefully_without_manifest() {
    let dir = temp_home();
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str], check_off: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_amenbo"));
        cmd.env_remove("AMENBO_HOME")
            .env_remove("AMENBO_PROJECT_DIR")
            // A test binary is unstamped, which is its own refusal; the override puts it on the road a
            // shipped build takes, so what is exercised here is the unreachable manifest and nothing else.
            .env("AMENBO_UPDATE_JSON_URL", "http://127.0.0.1:1/latest.json")
            .current_dir(&dir)
            .args(with_defaults(args, "ai"));
        if check_off {
            cmd.env("AMENBO_UPDATE_CHECK", "0"); // no upstream lookup — the manifest is unreachable
        }
        let out = cmd.output().expect("failed to run the binary");
        (out.status.code(), String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string())
    };

    // Mutually exclusive with --print: clap refuses the pair before anything runs.
    let (code, _out, err) = run(&["update", "--apply", "--print"], true);
    assert_eq!(code, Some(2), "clap turns away conflicting flags: {err}");

    // With the upstream lookup disabled the manifest cannot be read, so --apply fails plainly (exit 1)
    // rather than attempting a swap, and names io_error with the installer as the way out.
    let (code, _out, err) = run(&["update", "--apply", "--json"], true);
    assert_eq!(code, Some(1), "no manifest, no self-update: {err}");
    let v: Value = serde_json::from_str(err.trim()).unwrap_or_else(|_| panic!("error JSON: {err}"));
    assert_eq!(v["error"]["code"], "io_error", "a plain io error, not a panic: {err}");
}

/// **A build the release workflow did not stamp asks nothing, with nothing set to stop it.** This is
/// the case every other test in the tree writes `AMENBO_UPDATE_CHECK=0` for; here that env var is
/// deliberately absent, along with the URL override, so what answers is the rule itself rather than a
/// kill switch somebody remembered. A forgotten spawn now falls to *not asking* instead of reaching
/// the production endpoint — and both faces say which build it is rather than claiming to be current.
#[test]
fn an_unstamped_build_declines_to_check_for_updates() {
    let dir = temp_home();
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env_remove("AMENBO_HOME")
            .env_remove("AMENBO_PROJECT_DIR")
            // Nothing is set here on purpose: no kill switch, no override.
            .env_remove("AMENBO_UPDATE_CHECK")
            .env_remove("AMENBO_UPDATE_JSON_URL")
            .current_dir(&dir)
            .args(with_defaults(args, "ai"))
            .output()
            .expect("failed to run the binary");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    };

    let (code, out, err) = run(&["update", "--print", "--json"]);
    assert_eq!(code, Some(0), "declining is an answer, not a failure: {err}");
    let v: Value = serde_json::from_str(out.trim()).unwrap_or_else(|_| panic!("update JSON: {out}"));
    assert_eq!(v["reason"], "unstamped_build", "it names the build, not the network: {out}");
    assert_eq!(v["update_available"], false, "nothing was asked, so nothing is available: {out}");
    // Nothing was queried, so nothing upstream is claimed — not a version, not an installer.
    assert!(v["latest_version"].is_null(), "no version is claimed: {out}");
    assert!(v["url"].is_null(), "no installer is named: {out}");
    assert_eq!(v["opened"], false, "and nothing is opened: {out}");

    // `--apply` declines the same way, and plainly (exit 0) — there was never a manifest to fail on.
    let (code, out, err) = run(&["update", "--apply", "--json"]);
    assert_eq!(code, Some(0), "a build that cannot be measured is not an error: {err}");
    let v: Value = serde_json::from_str(out.trim()).unwrap_or_else(|_| panic!("apply JSON: {out}"));
    assert_eq!(v["action"], "self_update");
    assert_eq!(v["updated"], false, "no swap happened: {out}");
    assert_eq!(v["reason"], "unstamped_build", "for the same named reason: {out}");
}

/// `amenbo update --rollback` undoes the last `--apply` offline. Two network-free checks of the wiring:
/// `--rollback` is mutually exclusive with `--apply` and `--print` (clap turns them away), and with no
/// previous binary retained beside the running one it declines plainly (`no_backup`, exit 0) rather than
/// swapping onto nothing. The actual restore is covered by `amenbo-core`'s `self_update` unit tests; here
/// the test binary has no sibling `.bak`, so the `NoBackup` guard fires before any swap could touch it.
#[test]
fn update_rollback_declines_gracefully_without_a_retained_binary() {
    let dir = temp_home();
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env_remove("AMENBO_HOME")
            .env_remove("AMENBO_PROJECT_DIR")
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&dir)
            .args(with_defaults(args, "ai"))
            .output()
            .expect("failed to run the binary");
        (out.status.code(), String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string())
    };

    // Mutually exclusive with --apply and --print: clap refuses the pair before anything runs.
    let (code, _out, err) = run(&["update", "--rollback", "--apply"]);
    assert_eq!(code, Some(2), "clap turns away --rollback with --apply: {err}");
    let (code, _out, err) = run(&["update", "--rollback", "--print"]);
    assert_eq!(code, Some(2), "clap turns away --rollback with --print: {err}");

    // Nothing retained: a plain, zero-exit decline naming no_backup — never a swap onto nothing.
    let (code, out, err) = run(&["update", "--rollback", "--json"]);
    assert_eq!(code, Some(0), "no backup is a decline, not an error: {err}");
    let v: Value = serde_json::from_str(out.trim()).unwrap_or_else(|_| panic!("rollback JSON: {out}"));
    assert_eq!(v["action"], "self_rollback");
    assert_eq!(v["rolled_back"], false);
    assert_eq!(v["reason"], "no_backup");
}

/// A version check somebody typed goes and asks, rather than answering from the detection cache
/// (`AMB-D-463`). Two runs back to back, well inside the cache's hour: the second must report what the
/// upstream says *now*, not what the first run put in the cache — which is the whole of the change, and
/// is invisible to any test that only runs the command once.
///
/// The cache lives beside the OS's other caches rather than under `AMENBO_HOME`, so the whole
/// environment that locates it is pointed into the throwaway home too — a test that writes the real one
/// would leave this machine announcing a version nobody published.
#[test]
fn a_typed_update_check_asks_upstream_rather_than_the_cache() {
    let cli = Cli::new();
    let manifest = |version: &str| {
        format!(r#"{{"version": "{version}", "assets": {{}}}}"#)
    };
    let host = amenbo_static_host::StaticHost::serve([("/latest.json", manifest("9.9.9"))]);
    let home = cli.home.to_str().expect("a utf-8 home").to_string();
    let url = host.url("/latest.json");
    let env: Vec<(&str, &str)> = vec![
        ("AMENBO_UPDATE_CHECK", "1"),
        ("AMENBO_UPDATE_JSON_URL", url.as_str()),
        // Where `directories` looks for a cache, on each of the three platforms.
        ("HOME", home.as_str()),
        ("XDG_CACHE_HOME", home.as_str()),
        ("LOCALAPPDATA", home.as_str()),
    ];
    let reported = || -> String {
        let (out, code) = cli.run_env(&env, &["update", "--print", "--json"]);
        assert_eq!(code, 0, "update answers: {out}");
        let v: Value = serde_json::from_str(out.trim()).unwrap_or_else(|_| panic!("update JSON: {out}"));
        v["latest_version"].as_str().unwrap_or_default().to_string()
    };

    assert_eq!(reported(), "9.9.9", "the first run reads what is published");

    // A second version published a moment later. Inside the hour, so a cached answer would still be the
    // first one — and would be what a person is told when they ask again.
    host.set("/latest.json", &manifest("9.9.10"));
    assert_eq!(reported(), "9.9.10", "asked again, it says what is published now");
}
