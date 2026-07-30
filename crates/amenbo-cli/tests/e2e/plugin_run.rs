//! The boundary a plugin is called across: which flags stay amenbo's and which reach the plugin, the
//! facet a call carries over it, the events that fire one, and the log that says how each run
//! ended.

mod harness;

use std::process::Command;

use serde_json::Value;

use harness::*;

/// The read-back a plugin makes (`AMB-D-406`) is the one read that needs no facet: amenbo launched the
/// process and handed it the window (`AMENBO_PLUGIN_REACH`), so the reach is already fixed and `--actor`
/// would decide nothing. That is what the author's documentation shows — `amenbo task show <id> --json`,
/// no facet — and what `AMB-T-2460` found stopping with facet_required. A write from the same plugin still
/// stamps who acted, so it is still refused without one, and the window still bounds what can be read.
#[test]
fn a_plugin_reads_back_with_no_facet_and_still_declares_one_to_write() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let tid = id_str(&cli.json(&["task", "add", "--title", "seen", "--project", &pid, "--json"])["task"]["id"]);
    // A task the window does not cover: the bound project is a different one from the plugin's gate.
    let outside = cli.bound_project();
    let unseen = id_str(&cli.json(&["task", "add", "--title", "unseen", "--project", &outside, "--json"])["task"]["id"]);

    // A plugin's process: the store and the window named in the environment, and no facet anywhere — the
    // CWD is not the bound folder either, since a plugin's is whatever its launcher happened to be in.
    let plugin = |args: &[&str]| -> (String, String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("AMENBO_PLUGIN_REACH", amenbo_core::idref::project(pid.parse().unwrap()))
            .current_dir(&cli.home)
            .args(args)
            .output()
            .expect("run amenbo");
        (
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
            exit_code(&out),
        )
    };

    // The read-back, exactly as it is written for authors.
    let (stdout, stderr, code) = plugin(&["task", "show", &tid, "--json"]);
    assert_eq!(code, 0, "a plugin's read-back must pass with no facet: {stderr}");
    let shown: Value = serde_json::from_str(&stdout).expect("the read-back answers JSON");
    assert_eq!(shown["title"], "seen");

    // The window, not the facet, is what bounds it: the project it fires for is all it may read.
    let (_out, stderr, code) = plugin(&["task", "show", &unseen, "--json"]);
    assert_ne!(code, 0, "a task outside the window must not be readable");
    assert!(stderr.contains("out_of_reach"), "should be out_of_reach: {stderr}");

    // A write names an author, and the window supplies none — so this is still facet_required.
    let (_out, stderr, code) = plugin(&["comment", "add", &tid, "--text", "from a plugin", "--json"]);
    assert_eq!(code, 2, "a plugin's write must still declare a facet: {stderr}");
    assert!(stderr.contains("facet_required"), "should return facet_required: {stderr}");
    let (_out, stderr, code) = plugin(&["comment", "add", &tid, "--text", "from a plugin", "--actor", "ai", "--json"]);
    assert_eq!(code, 0, "with the facet declared the write goes through: {stderr}");
}

/// `plugin run` hands everything after the plugin's name to the plugin, dashes and all — so a facet
/// written where every other amenbo command takes it, on the end, never reaches amenbo. The failure is
/// `facet_required`, and on its own it says nothing about the `--actor ai` the person can see they typed.
/// The hint closes that gap, and closes it only there: a plugin may carry a flag of amenbo's spelling
/// for reasons of its own, so what fires this is a facet that was written and did not arrive.
#[test]
fn a_facet_written_after_plugin_run_is_named_where_the_call_failed() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // Nothing is added to these calls: what a person typed is the whole input.
    let spawn = |args: &[&str]| -> (String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&cli.home)
            .args(args)
            .output()
            .expect("run amenbo");
        (String::from_utf8_lossy(&out.stderr).to_string(), exit_code(&out))
    };

    // The habit every other command teaches: flags on the end. Here they are the plugin's.
    let (stderr, code) = spawn(&["plugin", "run", "worktree", "start", "1", "--actor", "ai"]);
    assert_eq!(code, 2, "the facet never reached amenbo, so the call stops: {stderr}");
    assert!(stderr.contains("facet is unspecified"), "it is still the same failure: {stderr}");
    assert!(stderr.contains("went to the plugin"), "the hint names where it went: {stderr}");
    assert!(
        stderr.contains("--actor ai plugin run worktree start 1"),
        "and hands back the same call with the flag where amenbo can see it: {stderr}"
    );

    // Nothing of amenbo's among what the plugin was handed: no facet was written anywhere, so the
    // plugin's argv is not the explanation and is not pointed at.
    let (stderr, code) = spawn(&["plugin", "run", "worktree", "start", "--branch", "main"]);
    assert_eq!(code, 2, "still refused, for the plain reason: {stderr}");
    assert!(!stderr.contains("went to the plugin"), "nothing to name here: {stderr}");

    // And with the facet where amenbo reads it, the call gets as far as the plugin — which is not
    // installed here, and that is a different failure entirely.
    let (stderr, code) = spawn(&["--actor", "ai", "plugin", "run", "worktree", "start", "1"]);
    assert_ne!(code, 0, "the plugin is not installed: {stderr}");
    assert!(!stderr.contains("facet is unspecified"), "the facet arrived: {stderr}");
}

/// `--help` after the plugin's name is the plugin's word like every other one (`AMB-D-346`) — and it is
/// the word that matters most there, because a plugin's usage is what its author puts behind it. amenbo
/// answering in its place would hide the very text the person asked for.
///
/// One form is still amenbo's: `plugin run --help` names no plugin, so there is nobody else to ask. It is
/// answered before the facet and the pointer are, the way every other help request is.
#[cfg(unix)]
#[test]
fn a_help_flag_reaches_the_plugin_and_only_the_nameless_form_is_amenbos() {
    use std::os::unix::fs::PermissionsExt;

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // A plugin whose command face prints its usage, which is all it takes to tell the two apart.
    install_plugin(&cli, "usage", serde_json::json!([]));
    let program = cli.home.join("plugins").join("usage").join("usage");
    std::fs::write(&program, "#!/bin/sh\ncat >/dev/null\necho \"the plugin's usage: $*\"\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli.json(&["plugin", "enable", "usage", "--json"]);

    for help in ["--help", "-h"] {
        // After the name: handed through, and what comes back is what the plugin printed.
        let (stdout, code) = cli.run(&["--actor", "human", "plugin", "run", "usage", help]);
        assert_eq!(code, 0, "the call reached the plugin: {stdout}");
        assert!(stdout.contains(&format!("the plugin's usage: {help}")), "the plugin answered: {stdout}");

        // Naming no plugin: amenbo's own help, and no facet declared anywhere — a help request never
        // needed one.
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(&cli.home)
            .args(["plugin", "run", help])
            .output()
            .expect("run amenbo");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(exit_code(&out), 0, "help is not a failure: {stdout}");
        assert!(
            stdout.contains("Usage: amenbo plugin run"),
            "it is this command's help, named the way it is typed: {stdout}"
        );
    }

    // A hyphen where the name goes is otherwise a flag written one word too late, and is told so rather
    // than sent to the catalog as a plugin nobody could have installed.
    let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&cli.home)
        .args(["plugin", "run", "--jsn", "usage"])
        .output()
        .expect("run amenbo");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(exit_code(&out), 2, "a bad argument stops at the door: {stderr}");
    assert!(stderr.contains("is a flag, not a plugin's name"), "it names what went wrong: {stderr}");
    assert!(stderr.contains("plugin run usage"), "and hands back where the flag belongs: {stderr}");
}

/// A flag amenbo happens to share a spelling with is the plugin's too, and the word right after the name
/// is where the sharing bites: amenbo's flags are global, so the parser would answer for one written
/// there and the plugin would never see it. An author is entitled to put `--json` on their own face.
///
/// The other side of the same line: written **ahead** of the name, those flags are amenbo's — that is the
/// place its own help names for them, and moving the boundary must not take that away.
#[cfg(unix)]
#[test]
fn amenbos_own_flags_are_the_plugins_from_the_name_onward() {
    use std::os::unix::fs::PermissionsExt;

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    install_plugin(&cli, "usage", serde_json::json!([]));
    let program = cli.home.join("plugins").join("usage").join("usage");
    std::fs::write(&program, "#!/bin/sh\ncat >/dev/null\necho \"handed: $*\"\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli.json(&["plugin", "enable", "usage", "--json"]);

    // One word past the name: the position amenbo's own flags reach in every other command.
    for flag in ["--json", "--yes", "-y", "--quiet", "--no-color"] {
        let (stdout, code) = cli.run(&["--actor", "human", "plugin", "run", "usage", flag]);
        assert_eq!(code, 0, "the call reached the plugin: {stdout}");
        assert!(stdout.contains(&format!("handed: {flag}")), "`{flag}` reached the plugin: {stdout}");
    }
    // The one that carries a value, and would otherwise take the word after it along.
    let (stdout, _) = cli.run(&["--actor", "human", "plugin", "run", "usage", "--actor", "ai"]);
    assert!(stdout.contains("handed: --actor ai"), "both words reached the plugin: {stdout}");

    // Ahead of the name they are still amenbo's: this one asks amenbo to answer in JSON, and the
    // plugin's return value rides inside that document rather than being printed raw.
    let (stdout, code) = cli.run(&["plugin", "run", "--json", "--actor", "human", "usage", "hello"]);
    assert_eq!(code, 0, "{stdout}");
    let doc: Value = serde_json::from_str(&stdout).expect("amenbo answered in JSON");
    assert!(
        doc["value"].as_str().unwrap_or_default().contains("handed: hello"),
        "the plugin's own words are inside amenbo's document: {stdout}"
    );
}

/// The JSON a **runner process** wrote at `path`, waited for (`AMB-T-2175`).
///
/// A runner is launched by the command that queued the event and outlives it, so what the plugin writes
/// lands *after* that command has returned — there is nothing for a caller to join any more. `want` picks
/// the value being waited for, which is what tells a second run from the one already on disk rather than
/// racing it.
#[cfg(unix)]
fn wrote_json(path: &std::path::Path, want: impl Fn(&Value) -> bool) -> Value {
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

/// The execution log once it holds `count` runs, waited for the same way and for the same reason: the run is
/// recorded by the runner process, not by the command that launched it (`AMB-T-2175`).
#[cfg(unix)]
fn logged_runs(cli: &Cli, count: i64) -> Value {
    for _ in 0..200 {
        let runs = cli.json(&["plugin", "log", "--json"]);
        if runs["count"] == count {
            return runs;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("the execution log did not reach {count} run(s) within ten seconds");
}

/// The mutating CLI drives the observation dispatcher over what is *installed and enabled*: a subscribed
/// plugin is actually run, with the event payload on its stdin (`AMB-D-367`). Its neighbours are left alone —
/// a plugin that subscribes to nothing never runs.
///
/// The run lands **after** the command returned, because the runner working the queue is a process of its own
/// (`AMB-T-2175`): the command launches it and exits, so this waits for the payload rather than expecting it
/// to be there already.
#[cfg(unix)]
#[test]
fn a_mutating_command_fires_the_enabled_plugin_that_subscribes_to_it() {
    use std::os::unix::fs::PermissionsExt;

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // Two installs, one subscription between them: only `logger` asks for `task.created`.
    let capture = cli.home.join("fired.json");
    install_subscribing_plugin(&cli, "logger", &["task.created"]);
    install_subscribing_plugin(&cli, "quiet", &[]);
    std::fs::write(
        cli.home.join("plugins").join("logger").join("logger"),
        format!("#!/bin/sh\ncat > '{}'\n", capture.display()),
    )
    .unwrap();
    std::fs::set_permissions(
        cli.home.join("plugins").join("logger").join("logger"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    cli.json(&["plugin", "enable", "logger", "--json"]);
    cli.json(&["plugin", "enable", "quiet", "--json"]);

    // A task add commits, the dispatcher fans what it appended onto the plugin's queue, and launches the
    // runner that works it. The command is gone by then, so the payload is waited for.
    let pid = cli.bound_project();
    let added = cli.json(&["task", "add", "--title", "発火の確認", "--project", &pid, "--json"]);
    let id = id_str(&added["task"]["id"]);

    let payload = wrote_json(&capture, |v| !v["id"].is_null());
    assert_eq!(payload["event"], "task.created");
    assert_eq!(id_str(&payload["id"]), id, "the payload names the task that was created");
    assert_eq!(payload["actor"], "human");

    // The cursor advanced with it: a second mutation delivers only its own event, never the first again.
    cli.json(&["task", "add", "--title", "二件目", "--project", &pid, "--json"]);
    let second = wrote_json(&capture, |v| v["id"] != payload["id"]);
    assert_ne!(second["id"], payload["id"], "the second run fired for the second task");
}

/// The execution log, read back (`AMB-D-361`). A hook is fire-and-forget — nobody waits on it and nothing
/// fails when it fails (`AMB-D-352`) — so a plugin that died said so to nobody. This is where that answer
/// lives, and the answer is the plugin's own stderr (`AMB-D-353`), which is why the human face carries it.
#[cfg(unix)]
#[test]
fn plugin_log_says_why_a_hook_did_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // Nothing has fired on this machine yet: an absent log is an empty log, not a failure.
    let empty = cli.json(&["plugin", "log", "--json"]);
    assert_eq!(empty["count"], 0);
    // And nothing has been *delivered* either, which is the other half of the same question (`AMB-D-380`):
    // an empty log alone cannot say whether the dispatcher ran and found no subscriber, or never ran.
    assert_eq!(empty["dispatch"]["cursor"], 0);
    assert!(empty["dispatch"]["cursor_face"].is_null(), "no face has advanced it: {empty}");
    let (nothing, code) = cli.run(&["plugin", "log"]);
    assert_eq!(code, 0, "an empty log is an answer, not an error");
    assert!(nothing.contains("No plugin runs recorded"), "{nothing}");
    assert!(nothing.contains("nothing has been delivered from this store yet"), "{nothing}");

    // A plugin that fails the way a real one does: a diagnosis on stderr, and a non-zero exit.
    install_subscribing_plugin(&cli, "logger", &["task.created"]);
    let program = cli.home.join("plugins").join("logger").join("logger");
    std::fs::write(&program, "#!/bin/sh\necho 'the webhook refused the delivery' >&2\nexit 3\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli.json(&["plugin", "enable", "logger", "--json"]);

    let pid = cli.bound_project();
    cli.json(&["task", "add", "--title", "発火の確認", "--project", &pid, "--json"]);

    // Waited for: the runner that fired it is a process of its own, so the line lands after `task add`
    // returned (`AMB-T-2175`).
    let runs = logged_runs(&cli, 1);
    let run = &runs["runs"][0];
    assert_eq!(run["plugin"], "logger");
    assert_eq!(run["event"], "task.created");
    assert_eq!(run["outcome"], "failed");
    assert_eq!(run["code"], 3);
    assert!(
        run["stderr"].as_str().unwrap().contains("the webhook refused"),
        "the author's diagnosis is what was kept: {run}"
    );

    // The human face puts that diagnosis under the run. A reader who has to reach for --json to learn
    // why has not been answered.
    let (out, code) = cli.run(&["plugin", "log"]);
    assert_eq!(code, 0, "reading the log is not a verdict on what it holds");
    assert!(out.contains("logger") && out.contains("task.created"), "{out}");
    assert!(out.contains("failed") && out.contains("exit 3"), "{out}");
    assert!(out.contains("the webhook refused the delivery"), "the stderr follows the run: {out}");

    // A name narrows it; one with nothing on file is an empty log rather than an error, because a run
    // outlives the install that made it.
    let named = cli.json(&["plugin", "log", "logger", "--json"]);
    assert_eq!(named["count"], 1);
    assert_eq!(named["plugin"], "logger");
    let (other, code) = cli.run(&["plugin", "log", "quiet"]);
    assert_eq!(code, 0);
    assert!(other.contains("No runs recorded for plugin 'quiet'"), "{other}");

    // The cursor moved with that delivery, and carries the face that moved it. `task add` is the CLI, so
    // that is what the stamp names — the fact a reader lines this log up against when chasing a double
    // fire (`AMB-D-380`). Reported for a narrowed listing too: the cursor is the store's, not a plugin's.
    let cursor = runs["dispatch"]["cursor"].as_i64().expect("the cursor is a number");
    assert!(cursor > 0, "an event was delivered, so the cursor left the floor: {runs}");
    assert_eq!(runs["dispatch"]["cursor_face"], "cli");
    assert_eq!(named["dispatch"]["cursor"], cursor);
    assert!(out.contains("last advanced by cli"), "{out}");
}

/// The read-back path, end to end (`AMB-D-406`): a plugin calls `amenbo` again to read what its payload only
/// named, and what it reads back is the project it observes — and nothing outside it.
///
/// The whole point is that neither half comes from where the plugin happens to be standing. It is handed the
/// store and its window in its environment, and the window is the gate its manifest declared: `scope:
/// project`, so one project. The callback declares no facet — it is a plain read — which under the old rule
/// would have made it a human seeing the whole device, so a refusal here can only have come from the window.
#[cfg(unix)]
#[test]
fn a_plugin_reads_its_own_project_back_and_no_other() {
    use std::os::unix::fs::PermissionsExt;

    let cli = Cli::new();
    cli.run(&["init", "--name", "観測されるPJ"]);
    let bound = cli.bound_project();
    let inside = cli.json(&["task", "add", "--title", "中の仕事", "--project", &bound, "--json"]);
    let inside_id = id_str(&inside["task"]["id"]);

    // A second project on the same device, holding the task the plugin must not be able to reach.
    let other = cli.json(&["project", "add", "--name", "別のPJ", "--json"]);
    let other_id = id_str(&other["project"]["id"]);
    let outside = cli.json(&["task", "add", "--title", "外の仕事", "--project", &other_id, "--json"]);
    let outside_id = id_str(&outside["task"]["id"]);

    // A project-scoped plugin whose whole body is two calls back into amenbo, each answer kept in a file so
    // the test reads exactly what the plugin saw. It declares its facet like any other caller (`AMB-D-408`)
    // — and declares `human`, the facet that normally reaches the whole device, so what narrows these two
    // calls can only be the window amenbo handed it.
    install_plugin(&cli, "reader", serde_json::json!([]));
    let answers = cli.home.join("answers");
    std::fs::create_dir_all(&answers).unwrap();
    let program = cli.home.join("plugins").join("reader").join("reader");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\ncat >/dev/null\n\
             '{bin}' task list --actor human --json > '{out}/list.json'\n\
             '{bin}' task show {outside_id} --actor human --json > '{out}/show.out' 2> '{out}/show.err'\n\
             printf '%s' \"$?\" > '{out}/show.code'\n",
            bin = env!("CARGO_BIN_EXE_amenbo"),
            out = answers.display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli.json(&["plugin", "enable", "reader", "--json"]);

    // Amenbo's flags all go ahead of the name — past it every word is the plugin's, the facet included.
    let run = cli.json(&["plugin", "run", "--json", "--actor", "human", "reader"]);
    assert_eq!(run["ok"], true, "the plugin ran: {run}");

    // It read its own project's work — the listing needed no `--project`, because the window filled the slot.
    let listed: Value =
        serde_json::from_str(&std::fs::read_to_string(answers.join("list.json")).unwrap()).unwrap();
    let ids: Vec<String> =
        listed["tasks"].as_array().unwrap().iter().map(|t| id_str(&t["id"])).collect();
    assert_eq!(ids, vec![inside_id], "the plugin sees its own project's tasks and only those: {listed}");

    // And the task one project over is out of reach — refused, not served, and not an empty result either.
    let code = std::fs::read_to_string(answers.join("show.code")).unwrap();
    assert_ne!(code, "0", "reading outside the window is refused");
    let refusal = std::fs::read_to_string(answers.join("show.err")).unwrap();
    assert!(refusal.contains("out_of_reach"), "the refusal says why: {refusal}");
}

/// Plant an installed plugin that subscribes to `events` — [`install_plugin`] with the manifest field the
/// dispatch resolver reads. The executable it lays down does nothing; a caller that wants the plugin to
/// *do* something overwrites it.
#[cfg(unix)]
fn install_subscribing_plugin(cli: &Cli, name: &str, events: &[&str]) {
    install_plugin(cli, name, serde_json::json!([]));
    let manifest_file = cli.home.join("plugins").join(name).join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_file).unwrap()).unwrap();
    manifest["events"] = serde_json::json!(events);
    std::fs::write(&manifest_file, serde_json::to_vec(&manifest).unwrap()).unwrap();
}
