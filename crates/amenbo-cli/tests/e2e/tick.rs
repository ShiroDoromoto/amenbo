//! `tick run`, the face the scheduler calls: what a wake-up with nothing owed still does, what it carries
//! out of the outbox, and the device where there is nothing to wake for at all.
//!
//! Driven as a process because that is the whole of what a scheduler starts: it resolves no folder,
//! declares no facet, and answers with an exit code nothing else reads.

mod harness;

use harness::*;

/// The ordinary round: every purpose takes its turn, and the day is marked so the next hour's wake-up
/// takes none. A store with nothing due warns about nothing, which is a turn taken and not a turn skipped.
/// It takes no facet and needs no pointer: the scheduler that starts it is neither a person nor their AI,
/// and stands wherever it happens to stand.
#[test]
fn a_tick_takes_each_purposes_turn_once_a_day_and_exits_clean() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let out = cli.json(&["tick", "run", "--json"]);
    assert_eq!(out["action"], "tick.run");
    assert_eq!(out["ok"], true);
    assert_eq!(out["ran"], serde_json::json!(["due"]), "the day's turn was taken: {out}");
    assert_eq!(out["failed"].as_array().unwrap().len(), 0);
    assert_eq!(out["carried"], 0, "nothing is due, so there was nothing to carry out");

    // The next hour of the same day: the turn is already taken, so nothing runs again.
    let again = cli.json(&["tick", "run", "--json"]);
    assert_eq!(again["ran"].as_array().unwrap().len(), 0, "{again}");
    assert_eq!(again["already_done"], serde_json::json!(["due"]), "{again}");

    // The same round without a facet on the command line: a scheduler has none to pass.
    let out = amenbo_scratch::command(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&cli.home)
        .args(["tick", "run"])
        .output()
        .expect("failed to run the binary");
    assert_eq!(exit_code(&out), 0, "stderr: {}", String::from_utf8_lossy(&out.stderr));
}

/// A device where Amenbo has never been used. The scheduler's registration outlives an uninstall and a
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

/// The whole road, walked once: a day comes, the tick warns about it, and the warning is carried out of
/// the outbox in the same round — with no app open, no facet on the command line, and nothing resident
/// (`AMB-D-706`).
///
/// The tick is the one mount that posts in the process it was woken in, rather than handing the messages
/// to a sender it would not outlive, so what it says it carried is what actually left the outbox.
///
/// The two steps are named apart on the wire so a project can report one and leave the other, and the
/// event carries no actor at all: nobody acted, a day arrived (`AMB-D-708`).
#[test]
fn a_day_that_has_come_is_carried_out_through_the_tick() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // Somewhere for the warning to go. The connection is never written, so the post is refused — which is
    // the trace that says the message was built, addressed and handed over.
    cli.json(&["notify", "target", "add", "--kind", "slack", "unreachable", "--json"]);
    cli.json(&["notify", "on", "--json"]);
    cli.json(&["notify", "use", "1", "--json"]);
    cli.json(&["notify", "event", "task.due", "--json"]);

    let pid = cli.bound_project();
    let today =
        cli.json(&["task", "add", "--title", "今日が期日", "--project", &pid, "--due", "today", "--json"]);
    cli.finish_creating(&id_str(&today["task"]["id"]));

    // The creations above were carried out by the write seam each command makes, so what this round finds
    // on the outbox is the day's own warning and nothing else.
    let out = cli.json(&["tick", "run", "--json"]);
    assert_eq!(out["ran"], serde_json::json!(["due"]), "{out}");
    assert!(out["carried"].as_i64().unwrap() >= 1, "the warning was carried out: {out}");

    // Woken again the same day: the turn is taken and the outbox is empty behind the round above.
    let again = cli.json(&["tick", "run", "--json"]);
    assert_eq!(again["carried"], 0, "{again}");
}
