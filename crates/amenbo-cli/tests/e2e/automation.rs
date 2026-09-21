//! `automation …`, end to end: the building side of an automation — the library, the steps, the ways
//! out, what runs after each one, and what is handed along.
//!
//! Driven as a process because what is being read here is the **command surface**: which flags stand
//! for what, what a refusal says, and what each `add` hands back for the next command to take. The
//! writes themselves are core's, and tested there.

mod harness;

use harness::*;

use serde_json::Value;

/// The id a write handed back, as a string to pass to the next command.
fn id_of(v: &Value, key: &str) -> String {
    id_str(&v[key]["id"])
}

/// An automation with one step carrying its own prompt — the shape most of these tests start from.
/// The project is named rather than left to the folder: the harness runs in a home nothing has bound.
fn an_automation(cli: &Cli) -> (String, String) {
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]), "automation");
    let s = id_of(
        &cli.json(&["automation", "step", "add", &a, "--name", "one", "--prompt", "do it", "--agent", "claude", "--json"]),
        "automation_step",
    );
    (a, s)
}

/// The whole picture, built the way it is meant to be built: a prompt in the library, an automation,
/// two steps, a second way out of one of them, what happens after each, and a file handed across. Each
/// command is worth anything only if the next one takes what it handed back, so the run is one test
/// rather than seven.
#[test]
fn a_picture_is_built_from_the_ids_each_command_hands_back() {
    let cli = Cli::new();

    let p = cli.a_project();
    let action = id_of(
        &cli.json(&["automation", "action", "add", "--project", &p, "--name", "Review", "--prompt", "review it", "--json"]),
        "automation_action",
    );
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "Review and fix", "--json"]), "automation");

    let review = id_of(
        &cli.json(&["automation", "step", "add", &a, "--name", "review", "--action", &action, "--agent", "claude", "--json"]),
        "automation_step",
    );
    let fix = id_of(
        &cli.json(&["automation", "step", "add", &a, "--name", "fix", "--prompt", "fix it", "--agent", "claude", "--json"]),
        "automation_step",
    );

    // The way out is declared on the action, since the step that runs one declares nothing of its own.
    let found = id_of(
        &cli.json(&["automation", "exit", "add", "--action", &action, "--name", "something to fix", "--json"]),
        "automation_exit",
    );
    // What that way out hands on, and what the later step takes in.
    cli.json(&["automation", "port", "add", "--exit", &found, "--name", "report", "--kind", "file", "--json"]);
    cli.json(&["automation", "port", "add", "--step", &fix, "--name", "report", "--kind", "file", "--required", "--json"]);

    cli.json(&["automation", "entry", "set", &a, "--step", &review, "--json"]);

    let onward = cli.json(&["automation", "edge", "add", "--from", &format!("{review}:something to fix"), "--to", &fix, "--json"]);
    assert_eq!(onward["automation_edge"]["ends"].as_str(), Some("go"));
    assert_eq!(onward["automation_edge"]["to_step_id"], serde_json::json!(fix.parse::<i64>().unwrap()));
    let closes = cli.json(&["automation", "edge", "add", "--from", &format!("{review}:"), "--done", "--json"]);
    assert_eq!(closes["automation_edge"]["ends"].as_str(), Some("done"));

    let wire = cli.json(&[
        "automation", "wire", "add",
        "--from", &format!("{review}:something to fix"), "--from-port", "report",
        "--to", &fix, "--to-port", "report", "--json",
    ]);
    assert_eq!(wire["automation_wire"]["from_port_name"].as_str(), Some("report"));

    let note = id_of(&cli.json(&["automation", "note", "add", &a, "--name", "House style", "--body", "short lines", "--json"]), "automation_note");
    cli.json(&["automation", "note", "link", &review, &note, "--json"]);
    let unlinked = cli.json(&["automation", "note", "unlink", &review, &note, "--json"]);
    assert_eq!(unlinked["automation_step_note"]["unlinked"], serde_json::json!(true));
}

/// What every step is told before its own prompt is the same for every automation until somebody
/// writes otherwise, so leaving `--preamble` out puts the standing rules there. Carrying none is a
/// thing to ask for, not a thing to fall into.
#[test]
fn the_standing_rules_go_in_unless_a_preamble_is_given() {
    let cli = Cli::new();

    let p = cli.a_project();
    let stood = cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]);
    let preamble = stood["automation"]["preamble"].as_str().unwrap_or_default();
    assert!(!preamble.is_empty(), "a new automation carries the standing rules: {preamble:?}");
    assert!(preamble.contains("one step of an automation run"), "{preamble}");

    let bare = cli.json(&["automation", "add", "--project", &p, "--name", "B", "--preamble", "", "--json"]);
    assert_eq!(bare["automation"]["preamble"].as_str(), Some(""));

    let own = cli.json(&["automation", "add", "--project", &p, "--name", "C", "--preamble", "keep it short", "--json"]);
    assert_eq!(own["automation"]["preamble"].as_str(), Some("keep it short"));
}

/// `--from` is one token, `<step>:<way out>`, because a way out is named against whichever of the step
/// and its action declares it — a name without its step names nothing. The colon with nothing after it
/// is the unnamed way out, and `*` the error one.
#[test]
fn an_edge_names_its_way_out_on_the_step_it_leaves_from() {
    let cli = Cli::new();
    let (_, step) = an_automation(&cli);

    let unnamed = cli.json(&["automation", "edge", "add", "--from", &format!("{step}:"), "--halt", "--json"]);
    assert_eq!(unnamed["automation_edge"]["exit_name"], Value::Null);
    assert_eq!(unnamed["automation_edge"]["ends"].as_str(), Some("halt"));

    let errored = cli.json(&["automation", "edge", "add", "--from", &format!("{step}:*"), "--done", "--json"]);
    assert_eq!(errored["automation_edge"]["exit_name"].as_str(), Some("*"));

    // No colon at all reads as the unnamed way out too — and that one already says what happens.
    let (again, code) = cli.run_err(&["automation", "edge", "add", "--from", &step, "--done", "--json"]);
    assert_ne!(code, 0, "the unnamed way out already says what happens after it");
    assert!(again.contains("unnamed"), "{again}");

    let (refused, code) = cli.run_err(&["automation", "edge", "add", "--from", "not-a-step", "--done", "--json"]);
    assert_eq!(code, 2, "{refused}");
    assert!(refused.contains("<step>:<way out>"), "the refusal says how to write it: {refused}");
}

/// The cap guards a loop that never converges, and a caller who never thought about it is the one that
/// loop happens to — so an edge into a step carries it unless somebody takes it off. An edge that
/// closes or stops the run is taken once and carries none at all.
#[test]
fn an_edge_into_a_step_is_capped_unless_the_cap_is_taken_off() {
    let cli = Cli::new();
    let (a, one) = an_automation(&cli);
    let two = id_of(
        &cli.json(&["automation", "step", "add", &a, "--name", "two", "--prompt", "again", "--agent", "claude", "--json"]),
        "automation_step",
    );

    let capped = cli.json(&["automation", "edge", "add", "--from", &format!("{one}:"), "--to", &two, "--json"]);
    assert_eq!(capped["automation_edge"]["max_times"], serde_json::json!(10));

    let open = cli.json(&["automation", "edge", "add", "--from", &format!("{two}:"), "--to", &one, "--no-max", "--json"]);
    assert_eq!(open["automation_edge"]["max_times"], Value::Null);

    let edge = id_of(&open, "automation_edge");
    let again = cli.json(&["automation", "edge", "update", &edge, "--max-times", "3", "--json"]);
    assert_eq!(again["automation_edge"]["max_times"], serde_json::json!(3));

    let closes = cli.json(&["automation", "edge", "add", "--from", &format!("{one}:*"), "--done", "--json"]);
    assert_eq!(closes["automation_edge"]["max_times"], Value::Null, "an edge that closes the run counts nothing");
}

/// A task filter is written in parts, never as one string: the same option twice is any-of and two
/// different options are both. What the parts build is read as the filter it will be run as, so a value
/// nothing accepts is refused while the person who wrote it is still here.
#[test]
fn a_task_filter_is_answered_in_parts_and_read_before_it_is_written() {
    let cli = Cli::new();
    let (_, step) = an_automation(&cli);
    cli.json(&["automation", "cfg", "add", "--step", &step, "--name", "queue", "--kind", "taskfilter", "--json"]);

    let set = cli.json(&[
        "automation", "cfg", "set", &step, "--name", "queue",
        "--status", "todo", "--status", "in_progress", "--priority", "high", "--json",
    ]);
    let written: Value = serde_json::from_str(set["automation_cfg"]["value"].as_str().unwrap()).unwrap();
    assert_eq!(written["status"], serde_json::json!(["todo", "in_progress"]));
    assert_eq!(written["priority"], serde_json::json!(["high"]));

    let (refused, code) = cli.run_err(&["automation", "cfg", "set", &step, "--name", "queue", "--status", "sideways", "--json"]);
    assert_ne!(code, 0, "a status nothing accepts is refused here: {refused}");
}

/// A setting takes one answer, in the shape its kind takes. Two shapes at once is a caller who has not
/// decided which setting they are answering; none at all is one who said nothing.
#[test]
fn a_setting_takes_one_answer_or_none_at_all() {
    let cli = Cli::new();
    let (_, step) = an_automation(&cli);
    cli.json(&["automation", "cfg", "add", "--step", &step, "--name", "depth", "--kind", "text", "--json"]);

    let (both, code) = cli.run_err(&["automation", "cfg", "set", &step, "--name", "depth", "--text", "deep", "--number", "3", "--json"]);
    assert_eq!(code, 2, "{both}");

    let (silent, code) = cli.run_err(&["automation", "cfg", "set", &step, "--name", "depth", "--json"]);
    assert_eq!(code, 2, "{silent}");

    cli.json(&["automation", "cfg", "set", &step, "--name", "depth", "--text", "deep", "--json"]);
    let cleared = cli.json(&["automation", "cfg", "set", &step, "--name", "depth", "--clear", "--json"]);
    assert_eq!(cleared["automation_cfg"]["value"], Value::Null);
}

/// The direction is never asked for: an input belongs to the step or the action that reads it, an
/// output to the way out that produced it. Naming none of the three is the one mistake the flags leave
/// open, and it is refused with the three to pick from.
#[test]
fn a_port_takes_its_direction_from_what_it_hangs_off() {
    let cli = Cli::new();
    let (_, step) = an_automation(&cli);
    let exit = id_of(
        &cli.json(&["automation", "exit", "add", "--step", &step, "--name", "found", "--json"]),
        "automation_exit",
    );

    let takes = cli.json(&["automation", "port", "add", "--step", &step, "--name", "in", "--kind", "value", "--json"]);
    assert_eq!(takes["automation_port"]["direction"].as_str(), Some("in"));

    let hands = cli.json(&["automation", "port", "add", "--exit", &exit, "--name", "out", "--kind", "file", "--json"]);
    assert_eq!(hands["automation_port"]["direction"].as_str(), Some("out"));

    let (refused, code) = cli.run_err(&["automation", "port", "add", "--name", "x", "--kind", "value", "--json"]);
    assert_eq!(code, 2, "{refused}");
    assert!(refused.contains("--step"), "the refusal names the three: {refused}");
}

/// Deleting an automation takes every step, way out, edge and wire with it, so it is confirmed like
/// every other destructive command — and `--json` has nobody to ask.
#[test]
fn deleting_an_automation_is_confirmed() {
    let cli = Cli::new();
    let (a, _) = an_automation(&cli);

    let (refused, code) = cli.run_err(&["automation", "rm", &a, "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("confirmation"), "{refused}");

    let gone = cli.json(&["automation", "rm", &a, "--yes", "--json"]);
    assert_eq!(gone["automation"]["deleted"], serde_json::json!(true));
}

/// A run is reached from a record already in hand — the task it worked, or the automation it came
/// from. Naming neither is the one mistake the flags leave open, and it is refused with both roads
/// rather than with a listing of every run there has ever been.
#[test]
fn a_run_is_reached_from_a_task_or_from_an_automation_and_never_listed_whole() {
    let cli = Cli::new();
    let (a, _) = an_automation(&cli);

    let (refused, code) = cli.run_err(&["automation", "run", "list", "--json"]);
    assert_eq!(code, 2, "{refused}");
    assert!(refused.contains("--task"), "the refusal names both roads: {refused}");
    assert!(refused.contains("--automation"), "{refused}");

    let none = cli.json(&["automation", "run", "list", "--automation", &a, "--json"]);
    assert_eq!(none["count"], serde_json::json!(0));
    assert_eq!(none["about"].as_str(), Some(format!("automation {a}").as_str()));
}

/// The task side of the same road: a task nothing has run reads as none, rather than as an error.
#[test]
fn a_task_no_run_has_worked_reads_as_none() {
    let cli = Cli::new();
    let p = cli.a_project();
    let task = id_str(&cli.json(&["task", "add", "--project", &p, "--title", "T", "--json"])["task"]["id"]);

    let none = cli.json(&["automation", "run", "list", "--task", &task_ref(&task), "--json"]);
    assert_eq!(none["count"], serde_json::json!(0));
    assert_eq!(none["about"].as_str(), Some(task_ref(&task).as_str()));
}

/// A run that was never launched is not found, said as a refusal rather than as an empty account.
#[test]
fn a_run_that_does_not_exist_is_said_to_be_missing() {
    let cli = Cli::new();
    let (refused, code) = cli.run_err(&["automation", "run", "show", "404", "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("404"), "{refused}");
}
