//! A project and the folders bound to it: what `init` and `bind` leave behind, the pointer a command
//! walks up to, the managed block a folder carries, and what `doctor` says once either has gone
//! stale.

mod harness;

use std::process::Command;

use serde_json::Value;

use harness::*;

/// `config set default_view` decides the view a project **created without one** opens on. The setting
/// is otherwise unobservable — reading it back out of `config` only proves it was stored — so what is
/// asserted here is the project that came after it, which is the whole point of the key.
#[test]
fn the_configured_default_view_is_what_a_new_project_opens_on() {
    let cli = Cli::new();
    // The shipped default, on a project that names no view.
    let shipped = cli.json(&["project", "add", "--name", "既定のまま", "--json"]);
    assert_eq!(shipped["project"]["default_view"], "board");

    cli.run(&["config", "set", "default_view", "list"]);
    let configured = cli.json(&["project", "add", "--name", "設定に従う", "--json"]);
    assert_eq!(configured["project"]["default_view"], "list", "the key is what answers now");

    // And `--view` still wins: the setting is the answer when nobody gave one, not a ceiling.
    let named = cli.json(&["project", "add", "--name", "明示する", "--view", "timeline", "--json"]);
    assert_eq!(named["project"]["default_view"], "timeline");
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

/// A human standing in a bound folder does not have to name the project that folder already names: the
/// slot `task add` leaves empty is filled from `.amenbo`, exactly as `decision add` and `dimension add`
/// fill theirs. The binding is the answer for both facets, which is what keeps the one caller who *can*
/// name a project from being the only one who must.
#[test]
fn a_task_added_in_a_bound_folder_lands_there_without_naming_it() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let bound = cli.bound_project();
    // A second project, so "it landed in the bound one" is a choice rather than the only answer available.
    cli.json(&["project", "add", "--name", "Elsewhere", "--json"]);

    let added = cli.json(&["task", "add", "--title", "束縛先に入るタスク", "--json"]);
    assert_eq!(added["action"], "task.add");
    assert_eq!(
        id_str(&added["task"]["placement"]["project"]["id"]),
        bound,
        "the folder's own project took the slot",
    );

    // And the override still outranks it, from the same folder.
    let elsewhere = cli.json(&["task", "add", "--title", "名指しした先", "--project", "Elsewhere", "--json"]);
    assert_eq!(
        elsewhere["task"]["placement"]["project"]["name"],
        "Elsewhere",
        "naming a project still wins over the binding",
    );
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
        .current_dir(&dir)
        // Declare the facet too: this watches the human-side guard, and as `ai` the run would instead trip
        // the guard that cuts an AI off in an unbound CWD, which fires first.
        .args(["status", "--json", "--actor", "human"])
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
        .current_dir(&dir)
        // As above (an AI would be cut off for want of a binding — a different guard).
        .args(["status", "--json", "--actor", "human"])
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
            .env("AMENBO_UPDATE_CHECK", "0")
            .current_dir(dir)
            .args(with_defaults(args, "human"))
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
        assert!(body.contains("<!-- amenbo:begin (managed v3) -->"), "{f} has the versioned managed marker");
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
    assert!(body.contains("<!-- amenbo:begin (managed v3) -->") && body.contains("<!-- amenbo:end -->"), "the block is rewritten at the current version");
    assert!(body.contains("agent --json"), "the block is regenerated to the current version: {body}");
    assert!(!body.contains("stale block content carried in from a clone"), "stale content between the markers is replaced with the current version: {body}");
}

/// Write the block back to the old, unversioned `(managed)` marker: a block left stale on disk by an upgrade.
fn make_block_stale(path: &std::path::Path) -> String {
    let before = std::fs::read_to_string(path).unwrap();
    let stale = before.replace("<!-- amenbo:begin (managed v3) -->", "<!-- amenbo:begin (managed) -->");
    assert_ne!(before, stale, "the downgrade actually changes the markers");
    std::fs::write(path, &stale).unwrap();
    stale
}

/// A stale block in a bound folder catches up to the current version **just by running amenbo there**, no
/// matter who runs it (actor-independent). Afterwards doctor no longer calls that folder stale.
#[test]
fn running_amenbo_in_a_bound_folder_follows_its_stale_managed_block() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]); // places the current block and registers this folder as bound
    let claude = cli.home.join("CLAUDE.md");
    make_block_stale(&claude);

    // Run amenbo in this folder — any command will do, so long as it resolves `.amenbo`.
    cli.run(&["status"]);

    let after = std::fs::read_to_string(&claude).unwrap();
    assert!(after.contains("<!-- amenbo:begin (managed v3) -->"), "just launching follows to the current version: {after}");
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

    // Once the work has ended, that same body stops being an entrance: nobody is arriving through a finished
    // task's notes, and the number it names is history rather than a broken pointer. The body is untouched —
    // only the question doctor asks of it has gone away.
    cli.run(&["task", "done", &live]);
    assert!(
        dead(&cli.json(&["doctor", "--json"])).is_empty(),
        "a finished task's notes are frozen prose, so the ref that died in them is not raised",
    );
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
    let ext = amenbo_scratch::scratch("ext");
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

/// `project show` reverses the binding: every folder linked to the project — the one it was created
/// with, the CWD, and a `--dir` target, many-to-one — is listed under bound_folders, each with an
/// existence check.
#[test]
fn project_show_lists_bound_folders_with_existence() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // The folder a project is created with is already one of its folders (`AMB-D-529`), so this one is
    // named rather than left to the harness: the count below is about which folders are listed.
    let born = amenbo_scratch::scratch("bf-born");
    let born_str = born.to_string_lossy().to_string();
    let pid = id_str(
        &cli.json(&["project", "add", "--name", "逆引きPJ", "--dir", &born_str, "--json"])["project"]["id"],
    );

    // Bind the CWD (home) too, plus an external folder via `--dir` (many-to-one).
    cli.run(&["bind", "--project", &pid]);
    let ext = amenbo_scratch::scratch("bf");
    let ext_str = ext.to_string_lossy().to_string();
    cli.run(&["bind", "--project", &pid, "--dir", &ext_str]);
    // `--dir` is canonicalized (symlinks resolved) before it is recorded; match on that path later.
    let ext_canon = std::fs::canonicalize(&ext).unwrap().to_string_lossy().to_string();

    let shown = cli.json(&["project", "show", &pid, "--json"]);
    let folders = shown["bound_folders"].as_array().expect("bound_folders array");
    assert_eq!(folders.len(), 3, "all three bound folders are listed: {folders:?}");
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

    let ext = amenbo_scratch::scratch("mp");
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
            .current_dir(cwd)
            .args(with_defaults(&args, "human"))
            .output()
            .expect("run amenbo bind");
        (
            String::from_utf8_lossy(&out.stderr).to_string(),
            exit_code(&out),
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
