//! `tick run`, the face the scheduler calls: what a wake-up with nothing owed still does, what it picks
//! up that a previous run left standing, the queue it leaves to the runner already on it, and the device
//! where there is nothing to wake for at all.
//!
//! Driven as a process because that is the whole of what a scheduler starts: it resolves no folder,
//! declares no facet, and answers with an exit code nothing else reads.

mod harness;

use harness::*;

/// The ordinary round. Nothing is declared to be done on a calendar day yet, so a tick has nothing to
/// carry out — and it is still a clean exit, not a refusal. It takes no facet and needs no pointer: the
/// scheduler that starts it is neither a person nor their AI, and stands wherever it happens to stand.
#[test]
fn a_tick_with_nothing_owed_works_the_queues_and_exits_clean() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let out = cli.json(&["tick", "run", "--json"]);
    assert_eq!(out["action"], "tick.run");
    assert_eq!(out["ok"], true);
    assert_eq!(out["ran"].as_array().unwrap().len(), 0, "nothing is owed on this build: {out}");
    assert_eq!(out["failed"].as_array().unwrap().len(), 0);
    assert_eq!(out["delivered"], 0, "and there is nothing waiting to deliver");
    assert_eq!(out["queues"].as_array().unwrap().len(), 0);

    // The same round without a facet on the command line: a scheduler has none to pass.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&cli.home)
        .args(["tick", "run"])
        .output()
        .expect("failed to run the binary");
    assert_eq!(exit_code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

/// A device where amenbo has never been used. The scheduler's registration outlives an uninstall and a
/// store that was moved away, so the tick has to meet "there is nothing here" — and meet it by doing
/// nothing, rather than by raising a store on a schedule nobody is watching.
#[test]
fn a_tick_on_a_device_with_no_store_raises_none() {
    let cli = Cli::new();
    let empty = cli.home.join("never-used");

    let (out, code) =
        cli.run_env(&[("AMENBO_HOME", empty.to_str().unwrap())], &["tick", "run", "--json"]);
    assert_eq!(code, 0, "nothing to do is not a failure: {out}");
    assert!(out.trim().is_empty(), "and nothing to report either: {out}");
    assert!(!empty.exists(), "a tick does not bring a store into being");
}

/// What the tick is woken for on the days it has nothing of its own to say (`AMB-D-706`). A fan-out that
/// could not resolve anybody leaves its event standing, and — with writes this feature makes a day apart —
/// the next wake-up is what carries it rather than the next time somebody happens to type something.
#[cfg(unix)]
#[test]
fn a_tick_carries_the_delivery_a_previous_run_left_standing() {
    use std::os::unix::fs::PermissionsExt;

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let capture = cli.home.join("fired.json");
    install_subscribing_plugin(&cli, "logger", &["task.created"]);
    let program = cli.home.join("plugins").join("logger").join("logger");
    std::fs::write(&program, format!("#!/bin/sh\ncat > '{}'\n", capture.display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli.json(&["plugin", "enable", "logger", "--json"]);

    // Shut the installs away, so the write below fans out to nobody it can resolve and leaves the event
    // where it is. This stands in for the ways a delivery really is left half-done — a runner killed, a
    // machine that went down mid-fan-out — none of which a test can stage.
    let plugins = cli.home.join("plugins");
    std::fs::set_permissions(&plugins, std::fs::Permissions::from_mode(0o000)).unwrap();
    let pid = cli.bound_project();
    let added = cli.json(&["task", "add", "--title", "取り残された配送", "--project", &pid, "--json"]);
    cli.json(&["task", "finish-creating", &id_str(&added["task"]["id"]), "--json"]);
    std::fs::set_permissions(&plugins, std::fs::Permissions::from_mode(0o755)).unwrap();

    let out = cli.json(&["tick", "run", "--json"]);
    assert_eq!(out["delivered"], 1, "the tick carried what was standing: {out}");
    assert_eq!(out["queues"].as_array().unwrap().len(), 0, "and left nothing owed: {out}");
    let payload = wrote_json(&capture, |v| !v["event"].is_null());
    assert_eq!(payload["event"], "task.created");

    // Woken again with the queues empty: the same clean round as a fresh store's.
    let again = cli.json(&["tick", "run", "--json"]);
    assert_eq!(again["delivered"], 0);
}

/// One queue is worked by one runner, and the lease is what says so (`AMB-D-399`). The GUI's own drive may
/// well have a runner on a queue when the hour comes round, and a tick that took the rows out from under it
/// would deliver them twice — so it names the queue and leaves it alone.
#[cfg(unix)]
#[test]
fn a_tick_leaves_the_queue_a_runner_is_already_on() {
    use std::os::unix::fs::PermissionsExt;

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    install_subscribing_plugin(&cli, "slow", &["task.created"]);
    let program = cli.home.join("plugins").join("slow").join("slow");
    std::fs::write(&program, "#!/bin/sh\nsleep 10\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    cli.json(&["plugin", "enable", "slow", "--json"]);

    // The write takes the lease and launches the runner before it returns, so there is a live runner on
    // the queue by the time the tick below is woken.
    let pid = cli.bound_project();
    let added = cli.json(&["task", "add", "--title", "走行役の居るキュー", "--project", &pid, "--json"]);
    cli.json(&["task", "finish-creating", &id_str(&added["task"]["id"]), "--json"]);

    let out = cli.json(&["tick", "run", "--json"]);
    assert_eq!(out["delivered"], 0, "nothing was taken from under the runner: {out}");
    let queues = out["queues"].as_array().unwrap();
    assert_eq!(queues.len(), 1, "the queue is named rather than passed over in silence: {out}");
    assert_eq!(queues[0]["plugin"], "slow");
    assert_eq!(queues[0]["waiting"], 1);
    assert_eq!(queues[0]["running"], true, "and named as one somebody is on: {out}");
}
