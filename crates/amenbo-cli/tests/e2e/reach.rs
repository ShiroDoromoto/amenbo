//! The facet a call declares, and the project an AI reaches from the folder it was started in: what
//! `agent --json` says about itself, what a missing `--actor` stops, and where `out_of_reach` lands
//! instead of an answer.

mod harness;

use std::process::Command;

use harness::*;

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

#[test]
fn actor_facet_is_stamped_from_flag_and_defaults_to_human() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let t = cli.json(&["task", "add", "--title", "facet", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    cli.finish_creating(&tid);

    // --actor ai stamps author_kind=ai on the comment, and the write echoes the effective facet (acted_facet).
    let ai = cli.json(&["comment", "add", &tid, "--text", "from ai", "--actor", "ai", "--json"]);
    assert_eq!(ai["comment"]["author_kind"], "ai");
    assert_eq!(ai["acted_facet"], "ai");

    // The harness declares `--actor human`, so an undeclared call lands as human.
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

/// An operation that **uses** the facet and does not declare one stops with facet_required (exit 2), and it
/// stops whatever the call looks like — `--json` or not, TTY or not (`AMB-D-408`). That covers the writes
/// that stamp the facet *and* the reads that draw an AI's reach from it, so `task list` is refused for the
/// same reason `task add` is. Only the faces that never touch a facet (version …) pass without one.
#[test]
fn facet_required_stops_every_operation_that_uses_the_facet() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project(); // somewhere for the explicit-facet task add below to land

    // A call that declares no facet at all. Nothing is stripped from the environment: the facet has no
    // entry point there to inherit one from.
    let spawn = |args: &[&str]| -> (String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .current_dir(&cli.home)
            .args(args)
            .output()
            .expect("run amenbo");
        (String::from_utf8_lossy(&out.stderr).to_string(), exit_code(&out))
    };

    // The write (task add) stops with facet_required.
    let (stderr, code) = spawn(&["task", "add", "--title", "x", "--json"]);
    assert_eq!(code, 2, "a write with no facet specified must stop: {stderr}");
    assert!(stderr.contains("facet_required"), "should return facet_required: {stderr}");

    // So does a read that surfaces store content, and dropping `--json` changes nothing — the context of
    // the call is not what decides.
    let (stderr, code) = spawn(&["task", "list", "--json"]);
    assert_eq!(code, 2, "a read that draws the reach must stop too: {stderr}");
    assert!(stderr.contains("facet_required"), "should return facet_required: {stderr}");
    let (stderr, code) = spawn(&["task", "list"]);
    assert_eq!(code, 2, "and it stops without --json as well: {stderr}");

    // A face that never touches a facet passes without one.
    let (_e, code) = spawn(&["version", "--json"]);
    assert_eq!(code, 0, "version uses no facet, so it passes without one");

    // With the facet spelled out (--actor human) both go through.
    let (_e, code) = spawn(&["task", "add", "--title", "y", "--project", &pid, "--actor", "human", "--json"]);
    assert_eq!(code, 0, "a write with an explicit facet passes");
    let (_e, code) = spawn(&["task", "list", "--actor", "human", "--json"]);
    assert_eq!(code, 0, "a read with an explicit facet passes");
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
    let stray = amenbo_scratch::scratch("stray");
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

/// The commands that take the whole device — the export a binding keeps (above), the archive beside it,
/// and the restore that writes one back — are refused to a **window**, the reach a plugin is launched
/// with (`AMB-D-406`). Binding and window are the same distance apart on every other call, and part
/// company only here: what `AMB-D-224` let through was the user taking their own data out and the
/// recovery their agent runs, and a plugin is neither. It moves to no other tool, recovers nothing, and
/// the project it observes was chosen by the runner before its code ran — so every project at once is not
/// a wider reading of that window, it is the way around it.
#[test]
fn a_plugins_window_is_refused_the_whole_device() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = cli.bound_project();
    cli.json(&["task", "add", "--title", "observed", "--project", &bound, "--json"]);

    // An archive the human took, so the restore below is refused on a real one rather than failing for
    // want of a file — and taken *now*, before the second project exists, so a restore that went through
    // would be visible afterwards as that project's work missing.
    cli.json(&["backup", "kept.amenbo-backup", "--json"]);

    let other = id_str(&cli.json(&["project", "add", "--name", "Other", "--json"])["project"]["id"]);
    cli.json(&["task", "add", "--title", "theirs", "--project", &other, "--json"]);

    // A plugin's process: the store and the window in the environment, no facet, and a CWD that is
    // whatever its launcher happened to be in (never the bound folder).
    let plugin = |args: &[&str]| -> (String, String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("AMENBO_PLUGIN_REACH", amenbo_core::idref::project(bound.parse().unwrap()))
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

    // Every shape is refused. Export streams every project into the plugin's stdout, or leaves the same
    // bytes on disk for it to read back; the archive is that second one under another name; and the
    // restore does not read the device at all — it overwrites it.
    for args in [
        vec!["export", "--json"],
        vec!["export", "--out", "taken", "--json"],
        vec!["backup", "taken.amenbo-backup", "--json"],
        vec!["restore", "kept.amenbo-backup", "--yes", "--json"],
    ] {
        let (stdout, stderr, code) = plugin(&args);
        assert_ne!(code, 0, "{args:?} must not hand a plugin the device: {stdout}");
        assert!(stderr.contains("out_of_reach"), "{args:?} is out_of_reach: {stderr}");
        assert!(!stdout.contains("theirs"), "{args:?} leaked another project: {stdout}");
        assert!(stderr.contains("plugin"), "the refusal is in the plugin's terms: {stderr}");
        assert!(!stderr.contains(".amenbo"), "a window is not a binding: {stderr}");
    }
    assert!(!cli.home.join("taken").exists(), "the refused export wrote nothing");
    assert!(!cli.home.join("taken.amenbo-backup").exists(), "the refused backup wrote nothing");

    // And the refused restore left the device standing: the work filed after the archive was taken is
    // still here, which it would not be had the store been replaced by that archive's.
    let after = cli.json(&["task", "list", "--json"]).to_string();
    assert!(after.contains("theirs"), "the refused restore replaced the device anyway: {after}");

    // What the window does open is unchanged: the project it fires for still reads back.
    let (stdout, stderr, code) = plugin(&["task", "list", "--json"]);
    assert_eq!(code, 0, "the read-back the window exists for still passes: {stderr}");
    assert!(stdout.contains("observed") && !stdout.contains("theirs"), "got: {stdout}");
}

/// The carrier's road is the other side of the test above: `sync` is **open** to a window where the
/// whole-device commands are refused, because it *is* that window answered rather than a way past it
/// (`AMB-D-581`). Three things have to hold at once for a carrier plugin to work at all — it is launched
/// with no facet, it must be told when to send, and what it sends must be its own project.
#[test]
fn a_carriers_road_is_open_to_the_window_the_whole_device_is_refused_to() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = cli.bound_project();
    cli.json(&["task", "add", "--title", "observed", "--project", &bound, "--json"]);
    let other = id_str(&cli.json(&["project", "add", "--name", "Other", "--json"])["project"]["id"]);
    let next_door =
        id_str(&cli.json(&["task", "add", "--title", "theirs", "--project", &other, "--json"])["task"]["id"]);

    // A plugin's process: the store and the window in the environment, and **no facet** — a read decides
    // nothing by one, and a carrier has none to declare.
    let plugin = |args: &[&str]| -> (String, String, i32) {
        let out = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .env("AMENBO_HOME", &cli.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("AMENBO_PLUGIN_REACH", amenbo_core::idref::project(bound.parse().unwrap()))
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
    let version = || -> i64 {
        let (stdout, stderr, code) = plugin(&["sync", "version", "--json"]);
        assert_eq!(code, 0, "a window asks its own version with no facet: {stderr}");
        let v: serde_json::Value = serde_json::from_str(&stdout).expect("json");
        assert_eq!(id_str(&v["project_id"]), bound, "the answer names the window it is for: {stdout}");
        v["version"].as_i64().expect("a number")
    };

    // What the carrier sends is its own project, from one document — and never the other's.
    let (stdout, stderr, code) = plugin(&["sync", "snapshot"]);
    assert_eq!(code, 0, "a window takes its own snapshot with no facet: {stderr}");
    assert!(stdout.contains("observed"), "the window's own work is in it: {stdout}");
    assert!(!stdout.contains("theirs"), "another project's work is not: {stdout}");
    assert!(!stdout.contains("Other"), "nor is the other project named at all: {stdout}");

    // The version is what tells a carrier whether to send that at all: it moves on a write inside the
    // window, and does not move for one next door — which is the whole of why it is asked per window and
    // not per device.
    let before = version();
    cli.json(&["task", "add", "--title", "more", "--project", &other, "--json"]);
    assert_eq!(version(), before, "a write in another project sent this carrier re-reading");
    cli.json(&["task", "add", "--title", "mine too", "--project", &bound, "--json"]);
    assert_ne!(version(), before, "a write inside the window left the carrier holding a stale copy");

    // And the third road: once it holds a copy, the carrier reads on from a cursor instead of sending the
    // window again. The changes it is handed are its own window's — the churn next door above is what
    // makes that a real question rather than a restatement of the snapshot's.
    let page = |since: &str| -> (serde_json::Value, i32) {
        let (stdout, stderr, code) = plugin(&["sync", "changes", "--since", since, "--json"]);
        assert_eq!(code, 0, "a window reads on with no facet: {stderr}");
        (serde_json::from_str(&stdout).expect("json"), code)
    };
    let (all, _) = page("0");
    assert_eq!(id_str(&all["project_id"]), bound, "the page names the window it is of");
    let datasets_and_ids: Vec<(String, i64)> = all["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .map(|c| (c["dataset"].as_str().unwrap().to_string(), c["record_id"].as_i64().unwrap()))
        .collect();
    assert!(!datasets_and_ids.is_empty(), "the window's own writes are named: {all}");
    // What the window may not see is not named, not counted, and not left as a hole: every task the page
    // names reads back through this same window.
    for (dataset, record_id) in &datasets_and_ids {
        if dataset != "task" {
            continue;
        }
        let (stdout, _, code) = plugin(&["task", "show", &format!("AMB-T-{record_id}"), "--json"]);
        assert_eq!(code, 0, "a task the page named is out of the window it was handed to: {stdout}");
    }

    // The cursor it hands back is where it stands: read again from there and there is nothing new, which
    // is the cheap answer a carrier gets on almost every pass.
    let cursor = all["cursor"].as_i64().expect("a cursor");
    let (nothing, _) = page(&cursor.to_string());
    assert_eq!(nothing["changes"].as_array().unwrap().len(), 0, "nothing has happened since: {nothing}");
    assert_eq!(nothing["cursor"].as_i64(), Some(cursor), "an empty page hands the cursor straight back");

    // A gap is not that. Both the exit code and the payload have to part company with the empty page
    // above, or a carrier reads "your copy is unusable" as "you are up to date" and sits stale forever.
    // `--since=-1` rather than `--since -1`: a bare leading minus is an option to the parser, so the
    // attached form is how a negative reaches the road at all.
    for bad in ["--since=900000001", "--since=-1"] {
        let (stdout, stderr, code) = plugin(&["sync", "changes", bad, "--json"]);
        assert_ne!(code, 0, "a cursor the ledger cannot speak for is not a success: {stdout}");
        assert!(stderr.contains("sync_gap"), "and it says which condition it is: {stderr}");
        assert!(stderr.contains("sync snapshot"), "the way on is in the answer: {stderr}");
        assert!(stdout.is_empty(), "nothing on stdout for a carrier to mistake for a page: {stdout}");
    }

    // The fourth road, and the one that makes the third worth walking: the page named records and never
    // what they hold, so the carrier reads those rows back by id. Through the same window — an id from
    // next door is one it could really be handed (a copy of its own state, a bug upstream), and what it
    // gets is nothing rather than the row.
    let mine_task = datasets_and_ids
        .iter()
        .find(|(dataset, _)| dataset == "task")
        .map(|(_, id)| *id)
        .expect("the page named a task of this window");
    let (stdout, stderr, code) =
        plugin(&["sync", "records", "--dataset", "task", "--ids", &mine_task.to_string()]);
    assert_eq!(code, 0, "a window reads its own records back with no facet: {stderr}");
    assert!(stdout.contains("observed"), "the row the page named comes back: {stdout}");

    let (stdout, _, code) = plugin(&["sync", "records", "--dataset", "task", "--ids", &next_door]);
    assert_eq!(code, 0, "an id outside the window is not an error — it is simply absent");
    assert!(!stdout.contains("theirs"), "a window read back the project next door: {stdout}");

    // And a dataset no road out carries is refused rather than answered empty: an empty answer reads as
    // "those records are gone", and a carrier that believed it would delete what it holds.
    let (stdout, stderr, code) =
        plugin(&["sync", "records", "--dataset", "plugin_secret", "--ids", "1", "--json"]);
    assert_ne!(code, 0, "the secrets are on no road out: {stdout}");
    assert!(stderr.contains("sync_error"), "and it says so in a code: {stderr}");
    assert!(stdout.is_empty(), "nothing on stdout for a carrier to mistake for an answer: {stdout}");
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

/// An axis **name a second project also uses** must resolve for the AI, not collapse into `ambiguous`. Names
/// are per-project, so only one of the two was ever reachable — and the `ambiguous` error names the other's
/// id, which is exactly the out-of-binding content the reach exists to keep out of the answer. The narrowing
/// is on both sides of the door: the reach drops what the facet cannot see, and `dimension set` / `unset`
/// resolve inside the task's own project, so the human — who reaches both — is not asked to disambiguate
/// something the task already pins.
#[test]
fn an_axis_name_two_projects_share_resolves_by_reach_and_by_the_task() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = cli.bound_project();
    let other = id_str(&cli.json(&["project", "add", "--name", "Other", "--json"])["project"]["id"]);

    // The same axis name, carrying the same value name, in both projects.
    let mut axis = Vec::new();
    for project in [bound.as_str(), other.as_str()] {
        let d = id_str(
            &cli.json(&["dimension", "add", "--project", project, "--name", "フェーズ", "--json"])["dimension"]["id"],
        );
        cli.json(&["dimension", "value-add", &d, "--name", "運用第2期", "--json"]);
        axis.push(d);
    }
    let (mine, theirs) = (axis[0].clone(), axis[1].clone());
    let task = id_str(&cli.json(&["task", "add", "--title", "分類する", "--project", &bound, "--json"])["task"]["id"]);

    // The AI names the axis by name and it lands: the other project's row never enters the candidate set.
    let set = cli.json(&["dimension", "set", &task, "フェーズ", "運用第2期", "--actor", "ai", "--json"]);
    assert_eq!(set["noop"], false);
    assert_eq!(id_str(&set["task_dimension_value"]["dimension_id"]), mine);
    assert_eq!(cli.json(&["dimension", "unset", &task, "フェーズ", "運用第2期", "--actor", "ai", "--json"])["noop"], false);

    // Reading it by that name is the bound project's axis, not an ambiguity listing the other's id.
    let shown = cli.json(&["dimension", "show", "フェーズ", "--actor", "ai", "--json"]);
    assert_eq!(id_str(&shown["dimension"]["id"]), mine);

    // Naming the other project's axis **by its id** stays out_of_reach: an id names one row on this machine,
    // so we do not answer that it does not exist.
    let (err, code) = cli.run_err(&["dimension", "show", &theirs, "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "the other project's axis is not readable by id");
    assert!(err.contains("out_of_reach"), "out_of_reach, not not_found: {err}");
    assert!(!err.contains("not_found"), "existence is not denied: {err}");

    // `dimension set` narrows by the task's project rather than by the reach, and that narrowing must not
    // swallow the distinction either: the other project's axis is unreachable, not absent.
    let (err, code) = cli.run_err(&["dimension", "set", &task, &theirs, "運用第2期", "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "an axis outside the task's project is not assignable");
    assert!(err.contains("out_of_reach"), "out_of_reach, not not_found: {err}");

    // The human reaches both, so the bare name genuinely names two axes — that truth is unchanged.
    let (err, code) = cli.run_err(&["dimension", "show", "フェーズ", "--json"]);
    assert_ne!(code, 0, "for the human the name really is ambiguous");
    assert!(err.contains("ambiguous_id"), "{err}");

    // …but `dimension set` is not ambiguous even for the human: the task names the project, and an
    // assignment never crosses one.
    let set = cli.json(&["dimension", "set", &task, "フェーズ", "運用第2期", "--json"]);
    assert_eq!(id_str(&set["task_dimension_value"]["dimension_id"]), mine);
}

// ───────────────────────── lint ─────────────────────────
