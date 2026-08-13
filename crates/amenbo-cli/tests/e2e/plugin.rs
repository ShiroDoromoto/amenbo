//! A plugin on this machine: installing one, the settings its author declared, the switch that opens
//! it on a single project, what the listing says about a build the catalog has moved past, and the
//! catalogs the listing answers from.

mod harness;

use amenbo_static_host::StaticHost;
use serde_json::Value;

use harness::*;

/// `plugin config set/get`: the author's schema decides the keys and their `secret` flag decides which
/// table the value is kept in — either way it is the bound project's, with no tier under it
/// (`AMB-D-434`). A secret never comes back out of `get`.
#[test]
fn plugin_config_writes_by_the_authors_schema_and_never_echoes_a_secret() {
    let cli = Cli::new();
    // A setting belongs to a project, and which one is never named on the command line: it is the
    // binding (an AI has no other route to one).
    cli.run(&["init", "--name", "tester"]);
    install_plugin(
        &cli,
        "slack",
        serde_json::json!([
            { "key": "events", "label": "イベント" },
            { "key": "webhook_url", "label": "Webhook URL", "secret": true, "required": true },
        ]),
    );

    // A text field lands in this project's row, and reads back from it.
    let set = cli.json(&["plugin", "config", "set", "slack", "events", "push,merge", "--json"]);
    assert_eq!(set["action"], "plugin.config.set");
    assert_eq!(set["secret"], false);
    assert!(set["project"].is_number(), "the write says which project it was for");
    let got = cli.json(&["plugin", "config", "get", "slack", "events", "--json"]);
    assert_eq!(got["value"], "push,merge");
    assert_eq!(got["set"], true);

    // An empty value clears rather than storing a blank — the same door for set and unset.
    cli.json(&["plugin", "config", "set", "slack", "events", "", "--json"]);
    let cleared = cli.json(&["plugin", "config", "get", "slack", "events", "--json"]);
    assert_eq!(cleared["set"], false);
    assert!(cleared["value"].is_null());

    // There is no tier to name any more.
    let (_, no_scope) =
        cli.run(&["plugin", "config", "set", "slack", "events", "x", "--scope", "project", "--json"]);
    assert_eq!(no_scope, 2, "--scope is gone");

    // A secret goes to the table of its own, and `-` keeps it off argv.
    let sec = cli.json_stdin(
        &["plugin", "config", "set", "slack", "webhook_url", "-", "--json"],
        "https://hooks.example.com/T0P-53CR3T\n",
    );
    assert_eq!(sec["secret"], true);
    let (out, err, code) = cli.run_both(&["plugin", "config", "get", "slack", "webhook_url", "--json"]);
    assert_eq!(code, 0);
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["set"], true, "the secret is stored");
    assert!(v.get("value").is_none(), "a secret's value never leaves through get: {out}");
    assert!(!out.contains("T0P-53CR3T") && !err.contains("T0P-53CR3T"), "the secret was echoed: {out}{err}");

    // A key the manifest does not declare has no storage rule, so it is refused — with the vocabulary.
    let (e, c) = cli.run_err(&["plugin", "config", "set", "slack", "typo", "x", "--json"]);
    assert_ne!(c, 0);
    assert!(e.contains("typo") && e.contains("events"), "the refusal names the declared keys: {e}");

    // A plugin that is not installed has no schema to read, so there is nothing to write.
    let (e2, c2) = cli.run_err(&["plugin", "config", "get", "nope", "events", "--json"]);
    assert_ne!(c2, 0);
    assert!(e2.contains("nope"), "{e2}");
}

/// A plugin's secrets go out with a backup and stay home on an export (`AMB-D-434`).
///
/// The two doors lead to different places. An export is one-way, out of amenbo and into another tool's
/// hands, and a credential that leaves that way is a credential in a file nobody here controls any more
/// — so the table does not travel. A backup leads back to the same person's own store, and dropping the
/// secrets there would mean typing every one of them in again after each restore.
///
/// What is checked is the table, not the value: the export carries no `plugin_secret` at all, which is
/// the shape that cannot be got wrong one row at a time.
#[test]
fn a_plugins_secret_rides_a_backup_and_never_an_export() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    install_plugin(
        &cli,
        "slack",
        serde_json::json!([
            { "key": "events", "label": "イベント" },
            { "key": "webhook_url", "label": "Webhook URL", "secret": true, "required": true },
        ]),
    );
    cli.json(&["plugin", "config", "set", "slack", "events", "push", "--json"]);
    cli.json_stdin(
        &["plugin", "config", "set", "slack", "webhook_url", "-", "--json"],
        "https://hooks.example.com/T0P-53CR3T\n",
    );

    // The export: the plain setting is there to prove the plugin's rows were reached at all, and the
    // secret's table is not in the document — no key, not even an empty array to read as "none set".
    let (dump, code) = cli.run(&["export"]);
    assert_eq!(code, 0, "{dump}");
    assert!(!dump.contains("T0P-53CR3T"), "the secret left the device in the export: {dump}");
    let doc: Value = serde_json::from_str(&dump).unwrap();
    assert!(doc["tables"].get("plugin_secret").is_none(), "the whole table stays home: {dump}");
    let settings = doc["tables"]["plugin_config"].as_array().unwrap();
    assert_eq!(settings.len(), 1, "the plugin's other rows do travel: {dump}");
    assert_eq!(settings[0]["field_key"], "events");

    // The backup: the same secret comes back through a restore, ready to use, with nothing to type in
    // again. The value never leaves through `get`, so what answers is that it is set.
    let archive = cli.home.join("backup.amenbo-backup");
    cli.json(&["backup", archive.to_str().unwrap(), "--json"]);
    cli.json(&["plugin", "config", "set", "slack", "webhook_url", "", "--json"]);
    let cleared = cli.json(&["plugin", "config", "get", "slack", "webhook_url", "--json"]);
    assert_eq!(cleared["set"], false, "cleared, so the restore has something to bring back");

    cli.json(&["restore", archive.to_str().unwrap(), "--yes", "--json"]);
    let back = cli.json(&["plugin", "config", "get", "slack", "webhook_url", "--json"]);
    assert_eq!(back["set"], true, "the secret rode the archive home");
}

/// A plugin declaring `scope: machine` keeps its gate, its settings and its secrets at the **device**
/// layer (`AMB-D-601`), and none of it mixes with a project's.
///
/// Still one switch: the layer is the author's declaration, not a level the face picks, so nothing here
/// names one. What is checked is that the two layers are two sets of rows — a device value is not the
/// bound project's, the project's own plugin does not read it, and the device rows survive a road out and
/// back (an export leaves every secret behind whichever layer wrote it; a backup brings them home).
#[test]
fn a_machine_scoped_plugins_rows_are_the_devices_and_never_a_projects() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let fields = serde_json::json!([
        { "key": "server", "label": "サーバ" },
        { "key": "token", "label": "トークン", "secret": true },
    ]);
    install_plugin_at(&cli, "carrier", fields.clone(), Some("machine"));
    install_plugin(&cli, "slack", fields);

    // The gate is the device's: the write names no project, and the listing has no project row to draw.
    let on = cli.json(&["plugin", "enable", "carrier", "--json"]);
    assert_eq!(on["enabled"], true);
    assert!(on["project"].is_null(), "a device gate belongs to no project: {on}");
    assert_eq!(on["scope"], "machine");
    assert!(on.get("level").is_none(), "no tier is named, because there is none: {on}");
    let listed = cli.json(&["plugin", "list", "--json"]);
    let carrier = listed["plugins"].as_array().unwrap().iter().find(|p| p["name"] == "carrier").unwrap();
    assert_eq!(
        carrier["enabled_projects"].as_array().unwrap().len(),
        0,
        "the one device gate is not any project's: {carrier}",
    );
    assert_eq!(carrier["scope"], "machine");
    assert_eq!(carrier["enabled_on_device"], true, "and the listing says where it is on: {carrier}");

    // The settings are the device's too, and the project's own plugin cannot see them.
    let set = cli.json(&["plugin", "config", "set", "carrier", "server", "wss://here", "--json"]);
    assert!(set["project"].is_null(), "the write says it was for no project: {set}");
    cli.json_stdin(
        &["plugin", "config", "set", "carrier", "token", "-", "--json"],
        "DEVICE-53CR3T\n",
    );
    cli.json(&["plugin", "config", "set", "slack", "server", "wss://theirs", "--json"]);
    assert_eq!(cli.json(&["plugin", "config", "get", "carrier", "server", "--json"])["value"], "wss://here");
    assert_eq!(
        cli.json(&["plugin", "config", "get", "slack", "server", "--json"])["value"],
        "wss://theirs",
        "one layer's value is not the other's",
    );

    // Out: the device's text value travels, and no secret does — the exclusion is the whole table, so the
    // layer it was written at never had to be asked (`AMB-D-434`).
    let (dump, code) = cli.run(&["export"]);
    assert_eq!(code, 0, "{dump}");
    assert!(!dump.contains("DEVICE-53CR3T"), "the device secret left in the export: {dump}");
    let doc: Value = serde_json::from_str(&dump).unwrap();
    assert!(doc["tables"].get("plugin_secret").is_none(), "the whole table stays home: {dump}");
    let device_rows: Vec<&Value> = doc["tables"]["plugin_config"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["plugin"] == "carrier")
        .collect();
    assert_eq!(device_rows.len(), 1, "the device's own setting travels: {dump}");
    assert!(device_rows[0]["project_id"].is_null(), "and travels as the device's: {dump}");

    // And back: a backup carries the whole file, so the device gate and its secret come home together.
    let archive = cli.home.join("backup.amenbo-backup");
    cli.json(&["backup", archive.to_str().unwrap(), "--json"]);
    cli.json(&["plugin", "disable", "carrier", "--json"]);
    cli.json(&["plugin", "config", "set", "carrier", "token", "", "--json"]);
    cli.json(&["restore", archive.to_str().unwrap(), "--yes", "--json"]);
    assert_eq!(cli.json(&["plugin", "config", "get", "carrier", "token", "--json"])["set"], true);
    let after = cli.json(&["plugin", "list", "--json"]);
    let carrier = after["plugins"].as_array().unwrap().iter().find(|p| p["name"] == "carrier").unwrap();
    assert_eq!(carrier["enabled_on_device"], true, "the device gate rode the archive home: {carrier}");
}

/// One plugin, one switch, and it is the bound project's (`AMB-D-434`): it turns on for the project you
/// are in and nowhere else, and the faces never ask which level, because there is no other.
#[test]
fn a_plugins_one_switch_is_the_bound_projects() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    install_plugin(&cli, "slack", serde_json::json!([]));
    install_plugin(&cli, "watcher", serde_json::json!([]));
    let here = bound_project_name(&cli);

    let on = cli.json(&["plugin", "enable", "slack", "--json"]);
    assert_eq!(on["enabled"], true);
    assert!(on.get("level").is_none(), "no tier is named, because there is none: {on}");
    let listed = cli.json(&["plugin", "list", "--json"]);
    let slack = listed["plugins"].as_array().unwrap().iter().find(|p| p["name"] == "slack").unwrap();
    assert_eq!(slack["enabled_projects"][0]["name"], here.as_str(), "on in this project");

    // There is no second switch to reach for.
    let (_, no_scope_flag) = cli.run(&["plugin", "enable", "slack", "--scope", "project", "--json"]);
    assert_eq!(no_scope_flag, 2, "--scope is gone");
    let (_, no_inherit) = cli.run(&["plugin", "inherit", "slack", "--json"]);
    assert_eq!(no_inherit, 2, "so is inherit — there is no tier to inherit from");

    // Disabling is the same one switch, and it leaves the neighbour alone.
    cli.json(&["plugin", "enable", "watcher", "--json"]);
    cli.json(&["plugin", "disable", "slack", "--json"]);
    let after = cli.json(&["plugin", "list", "--json"]);
    let slack = after["plugins"].as_array().unwrap().iter().find(|p| p["name"] == "slack").unwrap();
    let watcher = after["plugins"].as_array().unwrap().iter().find(|p| p["name"] == "watcher").unwrap();
    assert_eq!(slack["enabled_projects"].as_array().unwrap().len(), 0);
    assert_eq!(
        watcher["enabled_projects"][0]["name"],
        here.as_str(),
        "the neighbour is untouched"
    );
}

/// A row names **every** project holding the switch open (`AMB-D-412`), not the one the terminal happens
/// to stand in: a plugin left running in a project you are not looking at is exactly what a yes/no answer
/// hides. Off nowhere at all is an answer too, and the line says so rather than going quiet.
#[test]
fn a_listed_plugin_names_every_project_it_fires_in() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let here = bound_project_name(&cli);
    cli.json(&["project", "add", "--name", "外の仕事", "--json"]);
    install_plugin(&cli, "slack", serde_json::json!([]));

    cli.json(&["plugin", "enable", "slack", "--json"]);
    // The gate names the project it was moved in — "this project" would be the wrong sentence when
    // `--project` names a folder nobody is standing in.
    let (said, _) = cli.run(&["--project", "外の仕事", "plugin", "enable", "slack"]);
    assert!(said.contains("Enabled plugin: slack (外の仕事)"), "{said}");

    let listed = cli.json(&["plugin", "list", "--json"]);
    let slack = listed["plugins"].as_array().unwrap().iter().find(|p| p["name"] == "slack").unwrap();
    let on: Vec<&str> = slack["enabled_projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(on, vec![here.as_str(), "外の仕事"], "both of them, in the order projects are shown");
    assert!(slack["enabled_projects"][0]["ref"].as_str().unwrap().starts_with("AMB-P-"));

    let (out, code) = cli.run(&["plugin", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(&format!("on: {here}, 外の仕事")), "the line names them: {out}");

    // Closing both gates: an empty list is the answer "off everywhere", said out loud.
    cli.json(&["plugin", "disable", "slack", "--json"]);
    cli.json(&["--project", "外の仕事", "plugin", "disable", "slack", "--json"]);
    let (out, _) = cli.run(&["plugin", "list"]);
    assert!(out.contains("off everywhere"), "{out}");
}

/// An AI reads one project and never learns the others exist, and this listing is narrowed like every
/// other one. So it must not claim "everywhere" over a list it was not shown: with its own gate closed it
/// says which project that was, and a plugin firing next door stays out of its context entirely.
#[test]
fn an_ais_listing_names_its_own_project_and_claims_nothing_beyond_it() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let here = bound_project_name(&cli);
    cli.json(&["project", "add", "--name", "外の仕事", "--json"]);
    install_plugin(&cli, "slack", serde_json::json!([]));
    cli.json(&["--project", "外の仕事", "plugin", "enable", "slack", "--json"]);

    let listed = cli.json(&["--actor", "ai", "plugin", "list", "--json"]);
    let slack = listed["plugins"].as_array().unwrap().iter().find(|p| p["name"] == "slack").unwrap();
    assert_eq!(
        slack["enabled_projects"].as_array().unwrap().len(),
        0,
        "the neighbour's gate is not this AI's to read: {slack}"
    );

    let (out, code) = cli.run(&["--actor", "ai", "plugin", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("外の仕事"), "not even the other project's name: {out}");
    assert!(out.contains(&format!("off in {here}")), "it names what it was answered for: {out}");
    assert!(!out.contains("off everywhere"), "a claim it cannot make from here: {out}");

    // The human standing over both sees what the AI could not.
    let (out, _) = cli.run(&["plugin", "list"]);
    assert!(out.contains("on: 外の仕事"), "{out}");
}

/// The project this test's folder was bound to at `init` — named after the folder, so it is read back
/// rather than spelled out.
fn bound_project_name(cli: &Cli) -> String {
    cli.json(&["project", "list", "--json"])["projects"][0]["name"]
        .as_str()
        .expect("init made exactly one project")
        .to_string()
}

/// An open gate is not the same as a plugin that fires (`AMB-D-359`). amenbo updates underneath an
/// install, so a plugin enabled while it was compatible is later dropped at dispatch — and with only
/// "enabled" on screen, that silence is readable nowhere but the log. The listing carries the verdict.
#[test]
fn a_plugin_this_build_cannot_speak_to_is_named_in_the_listing() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    install_plugin(&cli, "slack", serde_json::json!([]));
    install_plugin(&cli, "watcher", serde_json::json!([]));
    cli.json(&["plugin", "enable", "slack", "--json"]);

    // The floor rises out of reach the way an update's manifest would raise it — after the enable, which
    // is precisely the state the gate column alone could not tell anyone about.
    let manifest_file = cli.home.join("plugins").join("slack").join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_file).unwrap()).unwrap();
    manifest["min_amenbo"] = serde_json::json!("99.0.0");
    std::fs::write(&manifest_file, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let listed = cli.json(&["plugin", "list", "--json"]);
    let rows = listed["plugins"].as_array().unwrap();
    let slack = rows.iter().find(|p| p["name"] == "slack").unwrap();
    assert_eq!(
        slack["enabled_projects"].as_array().unwrap().len(),
        1,
        "the gate is still open — that is the whole trap"
    );
    assert_eq!(slack["compatible"], false);
    let why = slack["incompatible_reason"].as_str().unwrap();
    assert!(why.contains("99.0.0"), "the mismatch is named, not just flagged: {why}");

    // The plugin next to it is untouched: the two fields answer the other way, and one incompatible
    // install does not colour the rest.
    let watcher = rows.iter().find(|p| p["name"] == "watcher").unwrap();
    assert_eq!(watcher["compatible"], true);
    assert!(watcher["incompatible_reason"].is_null());

    let (out, code) = cli.run(&["plugin", "list"]);
    assert_eq!(code, 0, "a listing reports; it is not a verdict on the run");
    assert!(out.contains("enabled, but nothing fires"), "the consequence, not the verdict: {out}");
    assert!(out.contains("99.0.0"), "{out}");
    assert_eq!(
        out.lines().filter(|l| l.trim_start().starts_with("enabled, but nothing fires")).count(),
        1,
        "only the install that cannot run gets the second line: {out}"
    );
}

/// The listing marks a plugin the catalog's list says something has moved about (`AMB-D-359`). It reads
/// the cached list alone, so no request is made at all — not for the list, and not for the detail
/// document that would say *what* moved (`AMB-D-386`): that is `plugin update --check`'s to fetch. A
/// plugin the catalog does not list is passed over, not marked. Applying stays the explicit
/// `plugin update <name>`; the listing only carries the fact, quietly.
#[test]
fn the_listing_marks_a_plugin_the_catalog_has_a_different_build_of() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    install_plugin(&cli, "worktree", serde_json::json!([]));
    // Installed, but the catalog below never lists it — a hand-installed or delisted plugin, which the
    // check passes over rather than marking.
    install_plugin(&cli, "watcher", serde_json::json!([]));

    // A catalog listing a *different* detail document for `worktree` — a moved digest against the one
    // the install recorded. Seeded straight into the registry cache, so the listing's mark is read from
    // it without ever reaching the (refused) catalog URL, and without the second document being fetched
    // at all: a listing marks the candidate, and `plugin update --check` is what goes and reads it.
    let registry = cli.home.join("plugins").join("registry");
    std::fs::create_dir_all(&registry).unwrap();
    let catalog = serde_json::json!({
        "catalog_v": 1,
        "generated_at": "2026-07-23T04:57:10Z",
        "plugins": [{
            "name": "worktree", "desc": "a plugin", "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo", "os": ["macos", "linux", "windows"],
            "category": "workflow",
            "detail_sum": format!("sha256:{}", "b".repeat(64)),
        }],
    });
    std::fs::write(registry.join("official.json"), serde_json::to_vec(&catalog).unwrap()).unwrap();

    let listed = cli.json(&["plugin", "list", "--json"]);
    let rows = listed["plugins"].as_array().unwrap();
    let worktree = rows.iter().find(|p| p["name"] == "worktree").unwrap();
    assert_eq!(worktree["update_available"], true, "the catalog holds a different build");
    let watcher = rows.iter().find(|p| p["name"] == "watcher").unwrap();
    assert_eq!(watcher["update_available"], false, "not listed in the catalog — passed over, not marked");

    let (out, code) = cli.run(&["plugin", "list"]);
    assert_eq!(code, 0);
    assert!(out.contains("[update available]"), "the human listing carries the badge: {out}");
    assert_eq!(
        out.lines().filter(|l| l.contains("[update available]")).count(),
        1,
        "only the install the catalog moved past gets the badge: {out}"
    );
    assert!(
        out.lines().find(|l| l.contains("worktree")).unwrap().contains("[update available]"),
        "the badge is on worktree's line: {out}"
    );
}

/// `plugin update --check` says **which catalog it answered from** (`AMB-D-359`).
///
/// The freshness window is what makes a check cheap, and it is also what makes "nothing has changed" and
/// "nothing had changed an hour ago" the same rows and the same `count`. That is the bug this pins: a
/// reader who has just published reads the first and goes looking for a broken comparison. `--fresh` is
/// the way past the window, and it is honest about failing — a fetch that was asked for and did not happen
/// still reports the cache.
#[test]
fn an_update_check_says_how_current_the_catalog_it_answered_from_is() {
    // Nothing answers here, so every arm below is decided by what is on disk rather than by the real
    // index — including the `--fresh` one, whose point is what it says when the fetch does not land.
    const UNREACHABLE: [(&str, &str); 1] =
        [("AMENBO_PLUGIN_CATALOG_URL", "http://127.0.0.1:1/catalog.json")];

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // Nothing installed: no catalog is read at all, and the report says as much rather than reporting a
    // catalog it never went for.
    let (out, code) = cli.run_env(&UNREACHABLE, &["plugin", "update", "--check"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("Catalog:"), "no catalog was read, so none is reported on: {out}");
    let empty: Value = serde_json::from_str(
        &cli.run_env(&UNREACHABLE, &["plugin", "update", "--check", "--json"]).0,
    )
    .unwrap();
    assert_eq!(empty["catalog"]["read"], "not_needed", "and `--json` says which of the two it is");

    install_plugin(&cli, "worktree", serde_json::json!([]));

    // With something installed and no catalog at all, the empty verdict is the absence of a reading —
    // said in as many words, because it is the one an empty list is most easily mistaken for.
    let (out, code) = cli.run_env(&UNREACHABLE, &["plugin", "update", "--check"]);
    assert_eq!(code, 0, "a check that could read nothing still reports: {out}");
    assert!(out.contains("none answered"), "{out}");
    let none: Value = serde_json::from_str(
        &cli.run_env(&UNREACHABLE, &["plugin", "update", "--check", "--json"]).0,
    )
    .unwrap();
    assert_eq!(none["catalog"]["read"], "unavailable");
    assert_eq!(none["count"], 0, "nothing was learned — which is not nothing having moved");

    // A cache written just now: inside the freshness window, so the check answers off it with no request.
    let registry = cli.home.join("plugins").join("registry");
    std::fs::create_dir_all(&registry).unwrap();
    let catalog = serde_json::json!({
        "catalog_v": 1,
        "generated_at": "2026-07-23T04:57:10Z",
        "plugins": [{
            "name": "worktree", "desc": "a plugin", "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo", "os": ["macos", "linux", "windows"],
            "category": "workflow",
            "detail_sum": format!("sha256:{}", "d".repeat(64)),
        }],
    });
    std::fs::write(registry.join("official.json"), serde_json::to_vec(&catalog).unwrap()).unwrap();

    let (out, code) = cli.run_env(&UNREACHABLE, &["plugin", "update", "--check"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("the copy cached"), "the answer is dated: {out}");
    assert!(out.contains("--fresh"), "and the way past the window is named: {out}");
    assert!(
        out.contains("Everything installed matches"),
        "the verdict is still there, now with something to read it inside: {out}"
    );

    let cached: Value = serde_json::from_str(
        &cli.run_env(&UNREACHABLE, &["plugin", "update", "--check", "--json"]).0,
    )
    .unwrap();
    assert_eq!(cached["catalog"]["read"], "cached");
    assert!(cached["catalog"]["age_seconds"].as_u64().is_some(), "how old, in seconds");
    assert_eq!(cached["count"], 0, "the digest matches what is installed");

    // Asked for the network and did not reach it. Reporting this as a fetch is the one answer that would
    // make the flag worse than not having it.
    let (out, code) = cli.run_env(&UNREACHABLE, &["plugin", "update", "--check", "--fresh"]);
    assert_eq!(code, 0, "a fetch that failed is not a failed command: {out}");
    assert!(out.contains("could not be reached"), "{out}");
    let asked: Value = serde_json::from_str(
        &cli.run_env(&UNREACHABLE, &["plugin", "update", "--check", "--fresh", "--json"]).0,
    )
    .unwrap();
    assert_eq!(asked["catalog"]["read"], "offline", "asking is not reaching");
    assert!(
        !out.contains("--fresh"),
        "and it does not send them to fetch again after a fetch just failed: {out}"
    );

    // Applying already fetches the current index every time, so there is nothing for the flag to turn on
    // there. Refused rather than ignored — a flag that quietly did nothing would read as one that worked.
    let (_, code) = cli.run_env(&UNREACHABLE, &["plugin", "update", "--all", "--fresh"]);
    assert_eq!(code, 2, "--fresh outside a check is a misuse");
}

/// `plugin validate --json` hands back what it read only when the manifest passes: a parse error read
/// nothing, and a manifest that broke a rule is refused at the door, so neither carries a document the
/// aggregator could publish. What the two documents hold when it does pass is the test below.
#[test]
fn plugin_validate_json_carries_the_two_documents_only_when_the_manifest_passes() {
    let cli = Cli::new();

    // A valid manifest, written where the command reads it by path (no store is opened).
    let good = cli.home.join("worktree.json");
    let manifest = serde_json::json!({
        "name": "worktree",
        "desc": "Isolate each task in its own git worktree",
        "author": "amenbo",
        "repo": "ShiroDoromoto/amenbo-plugin-worktree",
        "os": ["macos", "linux"],
        "category": "workflow",
        "url": "https://example.com/worktree-v1.tar.gz",
        "checksum": format!("sha256:{}", "a".repeat(64)),
        "scope": "machine",
        "events": ["task.done"],
        "min_amenbo": "1.8.0",
    });
    std::fs::write(&good, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let out = cli.json(&["plugin", "validate", good.to_str().unwrap(), "--json"]);
    assert_eq!(out["ok"], true, "a well-formed manifest passes");
    assert!(out["entry"].is_object() && out["detail"].is_object(), "and both documents ride back");

    // A manifest that parses but breaks a rule: the door refuses it, so there is nothing to hand back.
    let bad = cli.home.join("bad.json");
    let mut broken = manifest.clone();
    broken["checksum"] = serde_json::json!("nope");
    std::fs::write(&bad, serde_json::to_vec(&broken).unwrap()).unwrap();
    let (stdout, code) = cli.run(&["plugin", "validate", bad.to_str().unwrap(), "--json"]);
    assert_eq!(code, 1, "an invalid manifest exits non-zero");
    let out: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(out["ok"], false);
    assert!(out["count"].as_u64().unwrap() >= 1, "it names the problem");
    assert!(out.get("entry").is_none() && out.get("detail").is_none(), "and carries neither document");

    // A manifest that does not even parse: nothing was read, so likewise nothing to hand back.
    let junk = cli.home.join("junk.json");
    std::fs::write(&junk, b"{ not json").unwrap();
    let (stdout, code) = cli.run(&["plugin", "validate", junk.to_str().unwrap(), "--json"]);
    assert_eq!(code, 1);
    let out: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(out["ok"], false);
    assert!(out["parse_error"].is_string(), "a parse failure is reported as such");
    assert!(out.get("entry").is_none() && out.get("detail").is_none(), "nor either document");
}

/// What a passing manifest hands back: the two documents the catalog serves (`AMB-D-385`) — an `entry`
/// small enough that everyone can fetch every one of them to draw a list, and a `detail` fetched for the one
/// plugin being opened or installed. The split is amenbo's, so the aggregator holds no idea of which half a
/// field belongs in — an idea it could hold only by naming fields, and so fail to name. Between them they
/// carry the whole manifest, including the fields a hand-written copy list has dropped before (`events`,
/// `config`). The entry's `added_at` and `detail_sum` (`AMB-D-386`) come back as empty slots: neither can be
/// known from a manifest, so the catalog fills them.
#[test]
fn plugin_validate_json_splits_the_manifest_into_the_two_documents_the_catalog_serves() {
    let cli = Cli::new();

    let path = cli.home.join("worktree.json");
    let manifest = serde_json::json!({
        "name": "worktree",
        "desc": "Isolate each task in its own git worktree",
        "author": "amenbo",
        "repo": "alice/amenbo-plugin-worktree",
        "os": ["macos"],
        "category": "workflow",
        "url": "https://example.com/worktree-v1.tar.gz",
        "checksum": format!("sha256:{}", "a".repeat(64)),
        "official": true,
        "events": ["task.done"],
        "config": [{ "key": "base", "label": "Base branch" }],
        "min_amenbo": "1.8.0",
    });
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let out = cli.json(&["plugin", "validate", path.to_str().unwrap(), "--json"]);
    assert_eq!(out["ok"], true);
    let (entry, detail) = (&out["entry"], &out["detail"]);

    // What a list draws, and nothing an install needs.
    assert_eq!(entry["name"], "worktree");
    assert_eq!(entry["desc"], "Isolate each task in its own git worktree");
    assert_eq!(entry["os"], serde_json::json!(["macos"]));
    assert_eq!(entry["official"], true, "the badge is drawn in the list");
    for install_only in ["url", "checksum", "signature", "assets", "events", "config"] {
        assert!(entry.get(install_only).is_none(), "{install_only} does not ride in the list");
    }
    assert!(entry["added_at"].is_null(), "the catalog's slot, emitted empty");
    assert!(entry["detail_sum"].is_null(), "and the digest the catalog computes over the detail");

    // What an install needs, joined back to its entry by name.
    assert_eq!(detail["name"], "worktree", "the join between the two documents");
    assert_eq!(detail["url"], "https://example.com/worktree-v1.tar.gz");
    assert_eq!(detail["checksum"], format!("sha256:{}", "a".repeat(64)));
    assert_eq!(detail["events"], serde_json::json!(["task.done"]));
    assert_eq!(detail["config"][0]["key"], "base");
    assert_eq!(detail["min_amenbo"], "1.8.0");
    assert_eq!(detail["payload_v"], 1, "the contract version the author relied on is stated");
    for list_only in ["desc", "author", "repo", "os", "category", "official"] {
        assert!(detail.get(list_only).is_none(), "{list_only} is drawn from the list, not fetched again");
    }
    assert!(detail.get("assets").is_none(), "an absent optional field stays absent here too");
}

/// **What an author learns about the layer before they open a catalog PR** (`AMB-D-601`). `scope` is the
/// author's declaration of whether their plugin is a project's or the device's, and `plugin validate` is
/// the one place they can find out what amenbo will make of it: a declaration amenbo does not know is
/// refused here exactly as it would be at the install door, and one it does know comes back inside the
/// detail document the catalog will publish. Absent is the answer that matters most — the entries already
/// in the catalog write no `scope` at all, and they must keep passing unchanged.
#[test]
fn plugin_validate_reads_the_layer_an_author_declared_and_refuses_one_it_does_not_know() {
    let cli = Cli::new();

    let manifest = |scope: Option<&str>| {
        let mut m = serde_json::json!({
            "name": "carrier",
            "desc": "Carry this device's backlog out to a phone",
            "author": "alice",
            "repo": "alice/amenbo-plugin-carrier",
            "os": ["macos"],
            "category": "workflow",
            "url": "https://example.com/carrier-v1.tar.gz",
            "checksum": format!("sha256:{}", "a".repeat(64)),
        });
        if let Some(s) = scope {
            m["scope"] = serde_json::json!(s);
        }
        m
    };
    let validate = |value: &Value| {
        let path = cli.home.join("carrier.json");
        std::fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
        cli.run(&["plugin", "validate", path.to_str().unwrap(), "--json"])
    };

    // Declaring nothing is a project's plugin — the safe answer, and the one every published entry relies
    // on, so the detail the catalog would publish states it rather than leaving the reader to guess.
    let (stdout, code) = validate(&manifest(None));
    assert_eq!(code, 0, "a manifest that declares no layer still passes");
    let out: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(out["detail"]["scope"], "project", "and it is read as the project's");

    // Declaring the device is the author saying their plugin's work is the machine's.
    let (stdout, code) = validate(&manifest(Some("machine")));
    assert_eq!(code, 0);
    let out: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(out["detail"]["scope"], "machine", "the declaration rides to the install");
    assert!(out["entry"].get("scope").is_none(), "and not into the row a browse view draws");

    // A third layer is not a rule the validator weighs but a document it cannot read at all, which is the
    // same wall an install meets — so the author is told now, not after the PR is merged.
    for unknown in ["global", "workspace", "device"] {
        let (stdout, code) = validate(&manifest(Some(unknown)));
        assert_eq!(code, 1, "'{unknown}' is not a layer amenbo knows");
        let out: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(out["ok"], false);
        assert!(out["parse_error"].is_string(), "refused at the shape, before any rule is asked");
        assert!(out.get("detail").is_none(), "so there is no document to publish");
    }
}

/// **What an author learns about their translations before they open a catalog PR** (`AMB-D-621`). The
/// overlays sit beside the manifest as `<name>.<lang>.yaml`, and `plugin validate` is where the pair is
/// read together: every language is picked up off the directory, checked against the base it translates,
/// and — when the whole thing passes — handed back split across the faces the catalog publishes
/// (`AMB-D-622`), the list halves as one document per language and the detail halves bundled inside the
/// detail. This is also the one road that reads them as YAML, which is the form an author writes.
#[test]
fn plugin_validate_reads_the_translations_beside_a_manifest_and_publishes_them_by_face() {
    let cli = Cli::new();
    let dir = cli.home.join("plugins");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = dir.join("mail.yaml");
    std::fs::write(
        &manifest,
        format!(
            "name: mail\n\
             desc: Report what your AI did by email\n\
             author: alice\n\
             repo: alice/amenbo-plugin-mail\n\
             os: [macos]\n\
             category: workflow\n\
             url: https://example.com/mail-v1.tar.gz\n\
             checksum: sha256:{}\n\
             config:\n\
             \x20 - key: events\n\
             \x20   label: What to report\n\
             \x20   type: multi\n\
             \x20   options:\n\
             \x20     - value: task.done\n\
             \x20       label: Task done\n",
            "a".repeat(64)
        ),
    )
    .unwrap();
    let write = |name: &str, body: &str| std::fs::write(dir.join(name), body).unwrap();
    let validate = || cli.run(&["plugin", "validate", manifest.to_str().unwrap(), "--json"]);

    // Nothing beside it yet: a manifest nobody translated passes, and publishes no language document.
    let (stdout, code) = validate();
    assert_eq!(code, 0);
    let out: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(out["entry_i18n"], serde_json::json!({}), "no author wrote one, so none is published");
    assert!(out["detail"].get("i18n").is_none());

    // One language translating both faces, one translating only the form.
    write(
        "mail.ja.yaml",
        "desc: AI がやったことをメールで報告する\n\
         config:\n\
         \x20 events:\n\
         \x20   label: 何を報告するか\n\
         \x20   options:\n\
         \x20     task.done: タスクが完了した\n",
    );
    write("mail.de.yaml", "config:\n  events:\n    label: Was gemeldet wird\n");

    let (stdout, code) = validate();
    assert_eq!(code, 0, "overlays that line up with the base pass: {stdout}");
    let out: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        out["entry_i18n"],
        serde_json::json!({ "ja": { "desc": "AI がやったことをメールで報告する" } }),
        "the list half is one document per language, and de translated nothing there",
    );
    assert_eq!(out["detail"]["i18n"]["ja"]["config"]["events"]["label"], "何を報告するか");
    assert_eq!(out["detail"]["i18n"]["ja"]["config"]["events"]["options"]["task.done"], "タスクが完了した");
    assert_eq!(
        out["detail"]["i18n"]["de"]["config"]["events"]["label"], "Was gemeldet wird",
        "the detail half carries every language at once, since it is fetched one plugin at a time",
    );
    assert_eq!(out["entry"]["desc"], "Report what your AI did by email", "the base line is untouched");

    // A language amenbo is not read in, and an overlay naming what the base does not have: both are the
    // author's to fix here, and both are named at once rather than one per run.
    write("mail.xx.yaml", "desc: kein Wort\n");
    write("mail.fr.yaml", "author: Alice\nconfig:\n  smtp_host:\n    label: Serveur SMTP\n");
    let (stdout, code) = validate();
    assert_eq!(code, 1, "and the manifest does not pass while its translations do not");
    let out: Value = serde_json::from_str(&stdout).unwrap();
    let problems: Vec<(&str, &str)> = out["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["location"].as_str().unwrap(), p["code"].as_str().unwrap()))
        .collect();
    assert!(problems.contains(&("i18n[xx]", "unknown_language")), "{problems:?}");
    assert!(problems.contains(&("i18n[fr].author", "not_in_base")), "{problems:?}");
    assert!(problems.contains(&("i18n[fr].config[smtp_host]", "not_in_base")), "{problems:?}");
    assert!(out.get("entry_i18n").is_none(), "nothing is published while anything is refused");
}

/// `plugin catalog add/list/remove` registers third-party catalogs for the browsing view (`AMB-T-1980`):
/// the merged listing is the official catalog plus each registered source, and registration is idempotent
/// and reversible. A dead loopback URL registers, is marked unreachable, and does not cost the official
/// catalog its place — the official cache is seeded fresh so the merge reads it without the network.
#[test]
fn plugin_catalog_registers_lists_and_removes_a_third_party_source() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // Seed the official cache so the merge answers from disk (fresh) and never reaches the real URL.
    let registry = cli.home.join("plugins").join("registry");
    std::fs::create_dir_all(&registry).unwrap();
    let official = serde_json::json!({
        "catalog_v": 1,
        "generated_at": "2026-07-23T04:57:10Z",
        "plugins": [{
            "name": "worktree", "desc": "a plugin", "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo", "os": ["macos", "linux", "windows"],
            "category": "workflow", "url": "https://example.invalid/x.tar.gz",
            "checksum": format!("sha256:{}", "b".repeat(64)),
        }],
    });
    std::fs::write(registry.join("official.json"), serde_json::to_vec(&official).unwrap()).unwrap();

    // A loopback address nothing answers on — refuses fast, so registration does not wait on a timeout.
    let url = "http://127.0.0.1:1/third/catalog.json";
    let added = cli.json(&["plugin", "catalog", "add", url, "--json"]);
    assert_eq!(added["added"], true, "a new URL registers");
    assert_eq!(added["reachable"], false, "unreachable, but still registered");
    let again = cli.json(&["plugin", "catalog", "add", url, "--json"]);
    assert_eq!(again["added"], false, "registering the same URL again is a no-op");

    // The official catalog's own URL cannot be registered as a third-party source.
    let (_out, code) = cli.run(&[
        "plugin",
        "catalog",
        "add",
        "https://shirodoromoto.github.io/amenbo-plugins/catalog.json",
    ]);
    assert_ne!(code, 0, "the official catalog is not a third-party source");

    let listed = cli.json(&["plugin", "catalog", "list", "--json"]);
    assert_eq!(listed["plugins_total"], 1, "the official 'worktree' is in the merged view");
    let sources = listed["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 2, "official plus the one registered source");
    assert_eq!(sources[0]["official"], true, "official is first");
    assert_eq!(
        sources[0]["fingerprint"], "6272CBB782CB57A0",
        "the official catalog's key is the one amenbo ships (`AMB-D-371`)"
    );
    let third = sources.iter().find(|s| s["url"] == url).expect("the source is listed");
    assert_eq!(third["reachable"], false, "and marked unreachable");
    assert_eq!(third["name"], "127.0.0.1:1", "named after its host, having been given no name");
    assert!(third["fingerprint"].is_null(), "it published no key: browsable, installs nothing");

    let removed = cli.json(&["plugin", "catalog", "remove", url, "--json"]);
    assert_eq!(removed["removed"], true, "it was registered, so it is removed");
    let after = cli.json(&["plugin", "catalog", "list", "--json"]);
    assert_eq!(after["sources"].as_array().unwrap().len(), 1, "back to the official catalog alone");
}

/// Registering a catalog that publishes a key is a trust decision, not a bookmark (`AMB-D-389`): the
/// fingerprint is put in front of whoever is deciding, the key is pinned on their consent, and a catalog
/// that later publishes a *different* key is refused rather than re-pinned.
#[test]
fn plugin_catalog_pins_the_key_a_catalog_publishes_and_refuses_a_changed_one() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // Seed the official cache so the merge never reaches the real catalog over the network.
    let registry = cli.home.join("plugins").join("registry");
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::write(
        registry.join("official.json"),
        r#"{"catalog_v": 1, "generated_at": "2026-07-23T04:57:10Z", "plugins": []}"#,
    )
    .unwrap();

    // Two real minisign public keys — the catalog key amenbo ships, and the throwaway test key beside it
    // in `plugin_provenance`. Which key is which does not matter here; that they are two does.
    const KEY_A: &str = "RWSgV8uCt8tyYg74JbwBblWoE+g7bxSGvK8blkKW7gUo3EuBXaqy5oMR";
    const KEY_B: &str = "RWSw3wZ34b1PMyHu4KajlLhV0SdlMAgQGefo4pFIxv7MgRoWSVpCVXSE";
    let catalog = r#"{"catalog_v": 1, "generated_at": "2026-07-23T04:57:10Z", "plugins": []}"#;

    let host = StaticHost::serve([
        ("/works/catalog.json", catalog.to_string()),
        (
            "/works/catalog-key.pub",
            format!("untrusted comment: minisign public key 6272CBB782CB57A0\n{KEY_A}\n"),
        ),
    ]);
    let url = host.url("/works/catalog.json");

    // A --json run is non-interactive, so the consent has to be declared: without --yes it is refused.
    let (_out, code) = cli.run(&["plugin", "catalog", "add", &url, "--json"]);
    assert_ne!(code, 0, "pinning a key non-interactively takes --yes");
    let listed = cli.json(&["plugin", "catalog", "list", "--json"]);
    assert_eq!(listed["sources"].as_array().unwrap().len(), 1, "and nothing was registered");

    let added = cli.json(&["plugin", "catalog", "add", &url, "--name", "the works", "--yes", "--json"]);
    assert_eq!(added["added"], true);
    assert_eq!(added["fingerprint"], "6272CBB782CB57A0", "the fingerprint of what was pinned");
    assert_eq!(added["name"], "the works", "registered under the name it was given");

    let listed = cli.json(&["plugin", "catalog", "list", "--json"]);
    let source = listed["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["url"] == serde_json::json!(url))
        .expect("the source is listed")
        .clone();
    assert_eq!(source["fingerprint"], "6272CBB782CB57A0", "the pin is what the listing shows");
    assert_eq!(source["reachable"], true, "and the catalog itself answered");

    // The same key again is the ordinary case: no change, and no second question.
    let again = cli.json(&["plugin", "catalog", "add", &url, "--name", "the works", "--yes", "--json"]);
    assert_eq!(again["added"], false, "registering the same catalog again is a no-op");

    // The publisher rotates their key, same URL. amenbo will not take the new one on the old consent.
    host.set("/works/catalog-key.pub", &format!("{KEY_B}\n"));
    let (_out, code) = cli.run(&["plugin", "catalog", "add", &url, "--yes"]);
    assert_ne!(code, 0, "a different key is refused, not swallowed");

    let listed = cli.json(&["plugin", "catalog", "list", "--json"]);
    let still = listed["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["url"] == serde_json::json!(url))
        .expect("still registered")
        .clone();
    assert_eq!(still["fingerprint"], "6272CBB782CB57A0", "the pin taken at registration stands");

    // Unregistering is what lets go of a pin, so the new key can be consented to from scratch.
    cli.json(&["plugin", "catalog", "remove", &url, "--json"]);
    let re_added = cli.json(&["plugin", "catalog", "add", &url, "--yes", "--json"]);
    assert_eq!(re_added["added"], true, "registering again is the way to trust the new key");
    assert_ne!(re_added["fingerprint"], "6272CBB782CB57A0", "and it pins the key served now");
}

/// A plugin whose author declared a check, with a script that really answers — the two things
/// `install_plugin` cannot give it: the `settings` block, and an executable.
#[cfg(unix)]
fn install_plugin_that_checks(cli: &Cli, name: &str, script: &str) {
    use std::os::unix::fs::PermissionsExt;

    install_plugin(cli, name, serde_json::json!([{ "key": "smtp_user", "label": "User" }]));
    let dir = cli.home.join("plugins").join(name);
    let manifest_path = dir.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["settings"] = serde_json::json!({ "check": "config check" });
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let program = dir.join(name);
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `plugin enable` turned away by the plugin's own check (`AMB-D-664`).
///
/// What a terminal is told is that the check refused and which of the author's declared settings it spoke
/// about. What it is **not** told is what the author wrote about them: those sentences are the settings
/// screen's, where a person is reading, and this face's output is an AI's. The way to them is the hint —
/// the screen, and the execution log the run itself landed on.
#[cfg(unix)]
#[test]
fn plugin_enable_refused_by_the_check_names_the_settings_and_not_the_authors_words() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    install_plugin_that_checks(
        &cli,
        "mail",
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"v\":1,\"ok\":false,\"fields\":{\"smtp_user\":\"no such mailbox\"},\"message\":\"cannot sign in\"}'\n",
    );

    let (refusal, code) = cli.run_err(&["plugin", "enable", "mail"]);

    assert_ne!(code, 0, "a check that says no leaves the plugin off");
    assert!(refusal.contains("smtp_user"), "the setting the check named is said: {refusal}");
    for wrote in ["no such mailbox", "cannot sign in"] {
        assert!(!refusal.contains(wrote), "the author's own sentence reached a terminal: {refusal}");
    }
    assert!(refusal.contains("plugin log mail"), "the way to what it said is named: {refusal}");

    // The gate did not move: the listing names no project it fires in.
    let listed = cli.json(&["plugin", "list", "--json"]);
    let mail = listed["plugins"].as_array().unwrap().iter().find(|p| p["name"] == "mail").unwrap();
    assert_eq!(
        mail["enabled_projects"].as_array().unwrap().len(),
        0,
        "the refusal is fail-closed: {listed}"
    );

    // A check that says nothing readable costs the same, and says so in amenbo's own words.
    install_plugin_that_checks(&cli, "mail", "#!/bin/sh\ncat >/dev/null\nprintf 'looks fine'\n");
    let (silent, code) = cli.run_err(&["plugin", "enable", "mail"]);
    assert_ne!(code, 0);
    assert!(silent.contains("did not answer"), "a silence is not a yes: {silent}");

    // And the same plugin, once its check says yes, enables as any other does.
    install_plugin_that_checks(
        &cli,
        "mail",
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"v\":1,\"ok\":true}'\n",
    );
    let enabled = cli.json(&["plugin", "enable", "mail", "--json"]);
    assert_eq!(enabled["enabled"], true, "a yes opens the gate");
}
