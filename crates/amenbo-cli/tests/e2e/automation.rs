//! `automation …`, end to end: the building side of an automation — the library, the steps inside each
//! action, the ways out, what runs after each one, and what is handed along.
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

/// One library action holding one step, opened first — the unit an automation places. Answers the
/// action's id and the step's.
fn an_action(cli: &Cli, project: &str, name: &str, prompt: &str) -> (String, String) {
    let action = id_of(
        &cli.json(&["automation", "action-add", "--project", project, "--name", name, "--json"]),
        "automation_action",
    );
    let step = id_of(
        &cli.json(&[
            "automation", "step-add", &action, "--name", name, "--prompt", prompt, "--json",
        ]),
        "automation_step",
    );
    cli.json(&["automation", "action-entry-set", &action, "--step", &step, "--json"]);
    (action, step)
}

/// Put the built-in of `key` on an automation — the first thing placed is where a run starts, and it
/// has to be one of the built-ins a run can start at (`AMB-D-977`). Answers the placement.
fn an_entry(cli: &Cli, automation: &str, key: &str) -> String {
    id_of(
        &cli.json(&["automation", "place-add", automation, "--builtin", key, "--json"]),
        "automation_placement",
    )
}

/// An automation with one action placed on it, after the built-in it starts at — the shape most of
/// these tests start from. Answers the project it is in, the automation, the action and the placement
/// of the action. The project is named rather than left to the folder: the harness runs in a home
/// nothing has bound.
fn an_automation(cli: &Cli) -> (String, String, String, String) {
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]), "automation");
    an_entry(cli, &a, "take_task");
    let (action, _) = an_action(cli, &p, "one", "do it");
    let placement = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]),
        "automation_placement",
    );
    (p, a, action, placement)
}

/// The whole picture, built the way it is meant to be built: two actions in the library, an automation
/// they are placed on, a second way out of one of them, what happens after each, and a file handed
/// across. Each command is worth anything only if the next one takes what it handed back, so the run is
/// one test rather than seven.
#[test]
fn a_picture_is_built_from_the_ids_each_command_hands_back() {
    let cli = Cli::new();

    let p = cli.a_project();
    let (review_action, _) = an_action(&cli, &p, "Review", "review it");
    let (fix_action, _) = an_action(&cli, &p, "Fix", "fix it");
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "Review and fix", "--json"]), "automation");

    // The first thing placed is where a run starts (`AMB-D-977`).
    let take = an_entry(&cli, &a, "take_task");
    let shown = cli.json(&["automation", "show", &a, "--json"]);
    assert_eq!(shown["automation"]["entry_placement_id"].to_string(), take);
    let review = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &review_action, "--json"]),
        "automation_placement",
    );
    let fix = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &fix_action, "--json"]),
        "automation_placement",
    );

    // The way out is declared on the action: a placement carries nothing of its own but the answers.
    let found = id_of(
        &cli.json(&["automation", "exit-add", "--action", &review_action, "--name", "something to fix", "--json"]),
        "automation_exit",
    );
    // What that way out hands on, and what the later action takes in.
    let hands_on = id_of(
        &cli.json(&["automation", "port-add", "--exit", &found, "--name", "report", "--kind", "file", "--json"]),
        "automation_port",
    );
    cli.json(&["automation", "port-add", "--action", &fix_action, "--name", "report", "--kind", "file", "--required", "--json"]);

    cli.json(&["automation", "edge-add", "--from", &format!("{take}:着手した"), "--to", &review, "--json"]);
    let onward = cli.json(&["automation", "edge-add", "--from", &format!("{review}:something to fix"), "--to", &fix, "--json"]);
    assert_eq!(onward["automation_edge"]["ends"].as_str(), Some("go"));
    assert_eq!(onward["automation_edge"]["to_id"], serde_json::json!(fix.parse::<i64>().unwrap()));
    let closes = cli.json(&["automation", "edge-add", "--from", &format!("{review}:"), "--done", "--json"]);
    assert_eq!(closes["automation_edge"]["ends"].as_str(), Some("done"));

    let wire = cli.json(&[
        "automation", "wire-add",
        "--from", &format!("{review}:something to fix"), "--from-port", "report",
        "--to", &fix, "--to-port", "report", "--json",
    ]);
    // The wire is written by the names a person types and keys the port rows they name (`AMB-D-961`).
    assert_eq!(wire["automation_wire"]["from_port_id"], serde_json::json!(hands_on.parse::<i64>().unwrap()));
    assert_eq!(wire["noop"], serde_json::json!(false), "{wire}");

    // Drawn again, it is the one wire already there, and the answer says nothing was added (`AMB-T-5660`).
    let again = cli.json(&[
        "automation", "wire-add",
        "--from", &format!("{review}:something to fix"), "--from-port", "report",
        "--to", &fix, "--to-port", "report", "--json",
    ]);
    assert_eq!(again["automation_wire"]["id"], wire["automation_wire"]["id"], "{again}");
    assert_eq!(again["noop"], serde_json::json!(true), "the same wire drawn again was said to be added: {again}");
}

/// What every step is told before its own prompt is Amenbo's own and the same on every automation
/// (`AMB-D-952`), so a definition carries no field for it and the door offers no way to write one.
#[test]
fn an_automation_carries_no_preamble_of_its_own() {
    let cli = Cli::new();

    let p = cli.a_project();
    let stood = cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]);
    assert!(stood["automation"]["preamble"].is_null(), "{stood}");

    let (said, code) = cli.run_err(&["automation", "add", "--project", &p, "--name", "B", "--preamble", "mine"]);
    assert_ne!(code, 0, "--preamble is not a flag any more: {said}");
}

/// `--from` is one token, `<box>:<way out>`, because a way out is named against whichever of the box
/// and the action standing on it declares it — a name without its box names nothing. The colon with
/// nothing after it is the done way out every box is born with, and `*` the error one. What the edge keeps is the way
/// out's row (`AMB-D-961`), so two names give two keys.
#[test]
fn an_edge_names_its_way_out_on_the_box_it_leaves_from() {
    let cli = Cli::new();
    let (_, _, _, placement) = an_automation(&cli);

    let done = cli.json(&["automation", "edge-add", "--from", &format!("{placement}:"), "--halt", "--json"]);
    let done_exit = done["automation_edge"]["exit_id"].as_i64().expect("keyed");
    assert_eq!(done["automation_edge"]["ends"].as_str(), Some("halt"));

    let errored = cli.json(&["automation", "edge-add", "--from", &format!("{placement}:*"), "--done", "--json"]);
    let errored_exit = errored["automation_edge"]["exit_id"].as_i64().expect("keyed");
    assert_ne!(done_exit, errored_exit, "the done way out and the error one are two rows");

    // No colon at all reads as the done way out too — and that one already says what happens.
    let (again, code) = cli.run_err(&["automation", "edge-add", "--from", &placement, "--done", "--json"]);
    assert_ne!(code, 0, "the done way out already says what happens after it");
    assert!(again.contains("already says what happens"), "{again}");

    let (refused, code) = cli.run_err(&["automation", "edge-add", "--from", "not-a-box", "--done", "--json"]);
    assert_eq!(code, 2, "{refused}");
    assert!(refused.contains("<box>:<way out>"), "the refusal says how to write it: {refused}");
}

/// Both pictures are drawn with the same verb, and `--in-action` is what says which: an automation's
/// boxes are placements, an action's are the steps inside it.
#[test]
fn a_line_is_drawn_on_an_automation_or_inside_an_action() {
    let cli = Cli::new();
    let p = cli.a_project();
    let (action, first) = an_action(&cli, &p, "Review", "review it");
    let second = id_of(
        &cli.json(&["automation", "step-add", &action, "--name", "again", "--prompt", "look again", "--json"]),
        "automation_step",
    );

    let inside = cli.json(&[
        "automation", "edge-add", "--in-action", "--from", &format!("{first}:"), "--to", &second,
        "--json",
    ]);
    assert_eq!(inside["automation_edge"]["owner_kind"].as_str(), Some("action"));
    assert_eq!(inside["automation_edge"]["owner_id"], serde_json::json!(action.parse::<i64>().unwrap()));

    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]), "automation");
    an_entry(&cli, &a, "take_task");
    let placement = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]),
        "automation_placement",
    );
    let outside = cli.json(&["automation", "edge-add", "--from", &format!("{placement}:"), "--done", "--json"]);
    assert_eq!(outside["automation_edge"]["owner_kind"].as_str(), Some("automation"));
    assert_eq!(outside["automation_edge"]["owner_id"], serde_json::json!(a.parse::<i64>().unwrap()));
}

/// Inside an action, a step's way out can leave the action by one the action declares (`--exit-to`,
/// bare for its done one), and `0` on a wire is the action itself: what it takes in is handed to a
/// step, and what a step hands on fills the way out of the action it returns to. Driven as the AI in its
/// bound project, since the boundary is no step and must not be read as one outside its reach.
#[test]
fn inside_an_action_a_step_returns_to_the_action_and_wires_reach_the_action_itself() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let p = cli.bound_project();
    let ai = |args: &[&str]| {
        let mut with = args.to_vec();
        with.extend(["--actor", "ai", "--json"]);
        cli.json(&with)
    };
    let action = id_of(&ai(&["automation", "action-add", "--project", &p, "--name", "Review"]), "automation_action");
    let step = id_of(
        &ai(&["automation", "step-add", &action, "--name", "check", "--prompt", "check it"]),
        "automation_step",
    );
    let step_exit = id_of(&ai(&["automation", "exit-add", "--step", &step, "--name", "approved"]), "automation_exit");
    let action_exit = id_of(&ai(&["automation", "exit-add", "--action", &action, "--name", "approved"]), "automation_exit");
    ai(&["automation", "port-add", "--action", &action, "--name", "task", "--kind", "value"]);
    ai(&["automation", "port-add", "--step", &step, "--name", "task", "--kind", "value"]);
    ai(&["automation", "port-add", "--exit", &step_exit, "--name", "report", "--kind", "file"]);
    ai(&["automation", "port-add", "--exit", &action_exit, "--name", "report", "--kind", "file"]);

    let named = ai(&["automation", "edge-add", "--in-action", "--from", &format!("{step}:approved"), "--exit-to", "approved"]);
    assert_eq!(named["automation_edge"]["ends"].as_str(), Some("exit"));
    assert_eq!(named["automation_edge"]["exit_to_id"].as_i64(), action_exit.parse().ok());
    assert_eq!(named["automation_edge"]["max_times"], Value::Null, "leaving the action counts nothing");

    let bare = ai(&["automation", "edge-add", "--in-action", "--from", &format!("{step}:"), "--exit-to"]);
    assert_eq!(bare["automation_edge"]["ends"].as_str(), Some("exit"));
    let done = bare["automation_edge"]["exit_to_id"].as_i64().expect("keyed");
    assert_ne!(Some(done), action_exit.parse().ok(), "bare is the action's done way out");
    let edge = id_of(&bare, "automation_edge");
    let moved = ai(&["automation", "edge-update", &edge, "--exit-to", "approved"]);
    assert_eq!(moved["automation_edge"]["exit_to_id"].as_i64(), action_exit.parse().ok());

    let into = ai(&[
        "automation", "wire-add", "--in-action", "--from", "0", "--from-port", "task", "--to", &step,
        "--to-port", "task",
    ]);
    assert_eq!(into["automation_wire"]["from_id"], serde_json::json!(0));
    let out = ai(&[
        "automation", "wire-add", "--in-action", "--from", &format!("{step}:approved"), "--from-port",
        "report", "--to", "0", "--to-port", "report",
    ]);
    assert_eq!(out["automation_wire"]["to_id"], serde_json::json!(0));

    let (refused, code) = cli.run_err(&[
        "automation", "edge-add", "--in-action", "--from", &format!("{step}:*"), "--exit-to", "--done",
    ]);
    assert_eq!(code, 2, "one edge says one thing: {refused}");
}

/// The cap guards a loop that never converges, and a caller who never thought about it is the one that
/// loop happens to — so an edge into a box carries it unless somebody takes it off. An edge that
/// closes or stops the run is taken once and carries none at all.
#[test]
fn an_edge_into_a_box_is_capped_unless_the_cap_is_taken_off() {
    let cli = Cli::new();
    let (p, a, _, one) = an_automation(&cli);
    let (other, _) = an_action(&cli, &p, "two", "again");
    let two = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &other, "--json"]),
        "automation_placement",
    );

    let capped = cli.json(&["automation", "edge-add", "--from", &format!("{one}:"), "--to", &two, "--json"]);
    assert_eq!(capped["automation_edge"]["max_times"], serde_json::json!(10));

    let open = cli.json(&["automation", "edge-add", "--from", &format!("{two}:"), "--to", &one, "--no-max", "--json"]);
    assert_eq!(open["automation_edge"]["max_times"], Value::Null);

    let edge = id_of(&open, "automation_edge");
    let again = cli.json(&["automation", "edge-update", &edge, "--max-times", "3", "--json"]);
    assert_eq!(again["automation_edge"]["max_times"], serde_json::json!(3));

    let closes = cli.json(&["automation", "edge-add", "--from", &format!("{one}:*"), "--done", "--json"]);
    assert_eq!(closes["automation_edge"]["max_times"], Value::Null, "an edge that closes the run counts nothing");
}

/// A task filter is written in parts, never as one string: the same option twice is any-of and two
/// different options are both. What the parts build is read as the filter it will be run as, so a value
/// nothing accepts is refused while the person who wrote it is still here.
#[test]
fn a_task_filter_is_answered_in_parts_and_read_before_it_is_written() {
    let cli = Cli::new();
    let (_, _, action, placement) = an_automation(&cli);
    cli.json(&["automation", "cfg-add", "--action", &action, "--name", "queue", "--kind", "taskfilter", "--json"]);

    let set = cli.json(&[
        "automation", "cfg-set", &placement, "--name", "queue",
        "--status", "todo", "--status", "in_progress", "--priority", "high", "--json",
    ]);
    let written: Value = serde_json::from_str(set["automation_cfg"]["value"].as_str().unwrap()).unwrap();
    assert_eq!(written["status"], serde_json::json!(["todo", "in_progress"]));
    assert_eq!(written["priority"], serde_json::json!(["high"]));

    let (refused, code) = cli.run_err(&["automation", "cfg-set", &placement, "--name", "queue", "--status", "sideways", "--json"]);
    assert_ne!(code, 0, "a status nothing accepts is refused here: {refused}");
}

/// A task filter's order rides beside its parts, in the keys `task list --sort` takes. A key the list
/// has no order for is refused before it is written, and so is an order with no parts to put in it.
#[test]
fn a_task_filter_names_the_order_its_tasks_are_taken_in() {
    let cli = Cli::new();
    let (_, _, action, placement) = an_automation(&cli);
    cli.json(&["automation", "cfg-add", "--action", &action, "--name", "queue", "--kind", "taskfilter", "--json"]);

    let set = cli.json(&[
        "automation", "cfg-set", &placement, "--name", "queue", "--status", "todo", "--sort", "-due", "--json",
    ]);
    let written: Value = serde_json::from_str(set["automation_cfg"]["value"].as_str().unwrap()).unwrap();
    assert_eq!(written["status"], serde_json::json!(["todo"]));
    assert_eq!(written["sort"], serde_json::json!("-due"));

    let (unknown, code) = cli.run_err(&[
        "automation", "cfg-set", &placement, "--name", "queue", "--status", "todo", "--sort", "sideways", "--json",
    ]);
    assert_eq!(code, 2, "an order `task list` does not have is refused: {unknown}");

    let (alone, code) = cli.run_err(&["automation", "cfg-set", &placement, "--name", "queue", "--sort", "due", "--json"]);
    assert_eq!(code, 2, "an order with no parts narrows nothing: {alone}");
}

/// A setting takes one answer, in the shape its kind takes. Two shapes at once is a caller who has not
/// decided which setting they are answering; none at all is one who said nothing.
#[test]
fn a_setting_takes_one_answer_or_none_at_all() {
    let cli = Cli::new();
    let (_, _, action, placement) = an_automation(&cli);
    cli.json(&["automation", "cfg-add", "--action", &action, "--name", "depth", "--kind", "text", "--json"]);

    let (both, code) = cli.run_err(&["automation", "cfg-set", &placement, "--name", "depth", "--text", "deep", "--number", "3", "--json"]);
    assert_eq!(code, 2, "{both}");

    let (silent, code) = cli.run_err(&["automation", "cfg-set", &placement, "--name", "depth", "--json"]);
    assert_eq!(code, 2, "{silent}");

    cli.json(&["automation", "cfg-set", &placement, "--name", "depth", "--text", "deep", "--json"]);
    let cleared = cli.json(&["automation", "cfg-set", &placement, "--name", "depth", "--clear", "--json"]);
    assert_eq!(cleared["automation_cfg"]["value"], Value::Null);
}

/// **An answer in another kind's shape is refused when it is written** (`AMB-T-5648`): a task filter
/// answered with a choice was read as no narrowing, and the run took a person's task. So is a choice
/// that is not listed, a number below zero, and a choice list that is not one.
#[test]
fn an_answer_in_another_kinds_shape_is_refused() {
    let cli = Cli::new();
    let (_, _, action, placement) = an_automation(&cli);
    cli.json(&["automation", "cfg-add", "--action", &action, "--name", "queue", "--kind", "taskfilter", "--json"]);
    cli.json(&["automation", "cfg-add", "--action", &action, "--name", "wait", "--kind", "number", "--json"]);
    cli.json(&[
        "automation", "cfg-add", "--action", &action, "--name", "which", "--kind", "choice",
        "--options", r#"["a","b"]"#, "--json",
    ]);

    for wrong in [
        vec!["--name", "queue", "--choice", "x"],
        vec!["--name", "wait", "--number=-5"],
        vec!["--name", "which", "--choice", "c"],
    ] {
        let mut args = vec!["automation", "cfg-set", placement.as_str()];
        args.extend(wrong.iter().copied());
        args.push("--json");
        let (refused, code) = cli.run_err(&args);
        assert_ne!(code, 0, "{wrong:?} is refused: {refused}");
    }
    cli.json(&["automation", "cfg-set", &placement, "--name", "which", "--choice", "b", "--json"]);

    for broken in ["notjson", "[]", r#"["a","a"]"#] {
        let (refused, code) = cli.run_err(&[
            "automation", "cfg-add", "--action", &action, "--name", "other", "--kind", "choice",
            "--options", broken, "--json",
        ]);
        assert_ne!(code, 0, "{broken} is not a list of choices: {refused}");
    }
}

/// The direction is never asked for: an input belongs to the step or the action that reads it, an
/// output to the way out that produced it. Naming none of the three is the one mistake the flags leave
/// open, and it is refused with the three to pick from.
#[test]
fn a_port_takes_its_direction_from_what_it_hangs_off() {
    let cli = Cli::new();
    let p = cli.a_project();
    let (_, step) = an_action(&cli, &p, "one", "do it");
    let exit = id_of(
        &cli.json(&["automation", "exit-add", "--step", &step, "--name", "found", "--json"]),
        "automation_exit",
    );

    let takes = cli.json(&["automation", "port-add", "--step", &step, "--name", "in", "--kind", "value", "--json"]);
    assert_eq!(takes["automation_port"]["direction"].as_str(), Some("in"));

    let hands = cli.json(&["automation", "port-add", "--exit", &exit, "--name", "out", "--kind", "file", "--json"]);
    assert_eq!(hands["automation_port"]["direction"].as_str(), Some("out"));

    let (refused, code) = cli.run_err(&["automation", "port-add", "--name", "x", "--kind", "value", "--json"]);
    assert_eq!(code, 2, "{refused}");
    assert!(refused.contains("--step"), "the refusal names the three: {refused}");
}

/// **A way out does not hand on the task the run works** — only a built-in takes it, so the refusal
/// points at the two that do. Reading that task is an input, and that is still declared.
#[test]
fn a_way_out_is_refused_the_task_the_run_works() {
    let cli = Cli::new();
    let p = cli.a_project();
    let (_, step) = an_action(&cli, &p, "one", "do it");
    let exit = id_of(
        &cli.json(&["automation", "exit-add", "--step", &step, "--name", "found", "--json"]),
        "automation_exit",
    );

    let (refused, code) =
        cli.run_err(&["automation", "port-add", "--exit", &exit, "--name", "task", "--kind", "task_take", "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("take_task") && refused.contains("make_task"), "{refused}");

    let reads = cli.json(&["automation", "port-add", "--step", &step, "--name", "task", "--kind", "task_take", "--json"]);
    assert_eq!(reads["automation_port"]["direction"].as_str(), Some("in"));
}

/// Deleting an automation takes every placement, edge and wire with it, so it is confirmed like every
/// other destructive command — and `--json` has nobody to ask.
#[test]
fn deleting_an_automation_is_confirmed() {
    let cli = Cli::new();
    let (_, a, _, _) = an_automation(&cli);

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
    let (_, a, _, _) = an_automation(&cli);

    let (refused, code) = cli.run_err(&["automation", "run-list", "--json"]);
    assert_eq!(code, 2, "{refused}");
    assert!(refused.contains("--task"), "the refusal names both roads: {refused}");
    assert!(refused.contains("--automation"), "{refused}");

    let none = cli.json(&["automation", "run-list", "--automation", &a, "--json"]);
    assert_eq!(none["count"], serde_json::json!(0));
    assert_eq!(none["about"].as_str(), Some(format!("automation {a}").as_str()));
}

/// The task side of the same road: a task nothing has run reads as none, rather than as an error.
#[test]
fn a_task_no_run_has_worked_reads_as_none() {
    let cli = Cli::new();
    let p = cli.a_project();
    let task = id_str(&cli.json(&["task", "add", "--project", &p, "--title", "T", "--json"])["task"]["id"]);

    let none = cli.json(&["automation", "run-list", "--task", &task_ref(&task), "--json"]);
    assert_eq!(none["count"], serde_json::json!(0));
    assert_eq!(none["about"].as_str(), Some(task_ref(&task).as_str()));
}

/// A run that was never launched is not found, said as a refusal rather than as an empty account.
#[test]
fn a_run_that_does_not_exist_is_said_to_be_missing() {
    let cli = Cli::new();
    let (refused, code) = cli.run_err(&["automation", "run-show", "404", "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("404"), "{refused}");
}

// ───────────────────────────── reading one back ─────────────────────────────

/// The whole definition comes back off one command, with each placement's declarations resolved to the
/// action standing on it. Building it and reading it back is one test: what `show` is for is saying
/// what the `add`s just built, so anything asserted against a hand-written fixture would pass while the
/// two sides disagreed.
#[test]
fn a_definition_is_read_back_whole_with_each_placement_resolved() {
    let cli = Cli::new();
    let p = cli.a_project();
    let (review_action, _) = an_action(&cli, &p, "Review", "review it");
    let (fix_action, _) = an_action(&cli, &p, "Fix", "fix it");
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "Review and fix", "--json"]), "automation");
    an_entry(&cli, &a, "take_task");
    let review = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &review_action, "--json"]),
        "automation_placement",
    );
    let fix = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &fix_action, "--json"]),
        "automation_placement",
    );
    let found = id_of(
        &cli.json(&["automation", "exit-add", "--action", &review_action, "--name", "something to fix", "--json"]),
        "automation_exit",
    );
    cli.json(&["automation", "port-add", "--exit", &found, "--name", "report", "--kind", "file", "--json"]);
    cli.json(&["automation", "port-add", "--action", &fix_action, "--name", "report", "--kind", "file", "--required", "--json"]);
    cli.json(&["automation", "cfg-add", "--action", &review_action, "--name", "depth", "--kind", "number", "--json"]);
    cli.json(&["automation", "cfg-set", &review, "--name", "depth", "--number", "3", "--json"]);
    cli.json(&["automation", "edge-add", "--from", &format!("{review}:something to fix"), "--to", &fix, "--json"]);
    cli.json(&["automation", "wire-add", "--from", &format!("{review}:something to fix"), "--from-port", "report", "--to", &fix, "--to-port", "report", "--json"]);

    let shown = cli.json(&["automation", "show", &a, "--json"]);
    assert_eq!(shown["automation"]["name"].as_str(), Some("Review and fix"));
    assert_eq!(shown["placements"].as_array().map(|s| s.len()), Some(3), "the entry and the two");

    // The ways out a spot can be left by are the action's — resolved here rather than left for the
    // reader to go and look up.
    let first = &shown["placements"][1];
    assert_eq!(first["action"]["name"].as_str(), Some("Review"));
    // Every declarer is born carrying the done way out and the error one, so the one built here
    // is found by name rather than by where it sits.
    let found = first["exits"]
        .as_array()
        .expect("the ways out")
        .iter()
        .find(|x| x["exit"]["name"].as_str() == Some("something to fix"))
        .expect("the way out declared on the action");
    assert_eq!(found["outputs"][0]["name"].as_str(), Some("report"));
    // An action declares and the placement answers, and the two rows come back as one.
    assert_eq!(first["settings"][0]["name"].as_str(), Some("depth"));
    assert_eq!(first["settings"][0]["value"].as_str(), Some("3"));

    assert_eq!(shown["placements"][2]["inputs"][0]["name"].as_str(), Some("report"));
    assert_eq!(shown["edges"][0]["to_id"], serde_json::json!(fix.parse::<i64>().unwrap()));
    assert_eq!(shown["wires"][0]["to_port_id"], shown["placements"][2]["inputs"][0]["id"]);

    // And the prompts are inside the action, which is the picture `action show` reads.
    let inside = cli.json(&["automation", "action-show", &review_action, "--json"]);
    assert_eq!(inside["steps"].as_array().map(|s| s.len()), Some(1));
    assert_eq!(inside["steps"][0]["step"]["prompt"].as_str(), Some("review it"));
}

/// The listing counts what is placed and keeps an archived automation on it: archiving puts one out of
/// the way rather than removing it, and a listing that hid them would leave an id nothing explains.
#[test]
fn the_listing_counts_the_placements_and_keeps_an_archived_one() {
    let cli = Cli::new();
    let (p, a, _, _) = an_automation(&cli);

    let listed = cli.json(&["automation", "list", "--project", &p, "--json"]);
    assert_eq!(listed["count"], serde_json::json!(1));
    assert_eq!(listed["automations"][0]["placements"], serde_json::json!(2), "the entry and the action");
    assert_eq!(listed["automations"][0]["automation"]["archived"], serde_json::json!(false));

    cli.json(&["automation", "update", &a, "--archived", "true", "--json"]);
    let after = cli.json(&["automation", "list", "--project", &p, "--json"]);
    assert_eq!(after["count"], serde_json::json!(1));
    assert_eq!(after["automations"][0]["automation"]["archived"], serde_json::json!(true));
}

/// The library answers as the one list an automation could place, and `--global` narrows it to the
/// device's shelf rather than opening a second place to look.
#[test]
fn the_library_is_one_list_and_global_narrows_it() {
    let cli = Cli::new();
    let p = cli.a_project();
    let (action, _) = an_action(&cli, &p, "Review", "review it");

    let listed = cli.json(&["automation", "action-list", "--project", &p, "--json"]);
    assert_eq!(listed["count"], serde_json::json!(1));
    assert_eq!(listed["actions"][0]["action"]["name"].as_str(), Some("Review"));
    assert_eq!(listed["actions"][0]["steps"], serde_json::json!(1));
    assert_eq!(listed["actions"][0]["used_by"], serde_json::json!(0));

    let device = cli.json(&["automation", "action-list", "--global", "--json"]);
    assert_eq!(device["count"], serde_json::json!(0));

    // One action placed twice on one automation is one automation whose runs change when the prompt
    // is rewritten, which is what the count is about.
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]), "automation");
    an_entry(&cli, &a, "take_task");
    for _ in 0..2 {
        cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]);
    }
    let again = cli.json(&["automation", "action-list", "--project", &p, "--json"]);
    let id = serde_json::json!(action.parse::<i64>().unwrap());
    let card = again["actions"].as_array().expect("the actions").iter().find(|card| card["action"]["id"] == id);
    assert_eq!(card.expect("the action is listed")["used_by"], serde_json::json!(1), "{again}");

    let shown = cli.json(&["automation", "action-show", &action, "--json"]);
    assert_eq!(shown["steps"][0]["step"]["prompt"].as_str(), Some("review it"));
    assert_eq!(shown["used_by"], serde_json::json!(1));
    // Every declarer is born carrying the done way out and the error one.
    assert_eq!(shown["exits"].as_array().map(|x| x.len()), Some(2));
}

/// **What an action is for is written on the action** (`AMB-D-952`), and it is the build screen's
/// alone: a launch carries the preamble and the step's own prompt, and never this. An action written
/// without one carries the empty string rather than nothing, which is what lets it be rewritten later
/// without a second shape to read.
#[test]
fn an_action_says_what_it_is_for_and_a_rewrite_reaches_it() {
    let cli = Cli::new();
    let p = cli.a_project();

    let bare = cli.json(&["automation", "action-add", "--project", &p, "--name", "Review", "--json"]);
    assert_eq!(bare["automation_action"]["note"].as_str(), Some(""));

    let told = cli.json(&[
        "automation", "action-add", "--project", &p, "--name", "Fix",
        "--note", "点検で見つかった分だけ直す", "--json",
    ]);
    assert_eq!(told["automation_action"]["note"].as_str(), Some("点検で見つかった分だけ直す"));

    let id = id_of(&told, "automation_action");
    let rewritten =
        cli.json(&["automation", "action-update", &id, "--note", "直すのは一度に1件", "--json"]);
    assert_eq!(rewritten["automation_action"]["note"].as_str(), Some("直すのは一度に1件"));
    assert_eq!(
        rewritten["automation_action"]["name"].as_str(),
        Some("Fix"),
        "only the fields given change",
    );
}

/// **An action moves between the two libraries** (`AMB-D-954`): out to the device's is never refused,
/// and into a project is refused while an automation of another project places it — the refusal names
/// that automation, and the action stays where it was.
#[test]
fn an_action_moves_between_libraries_and_is_refused_where_another_project_places_it() {
    let cli = Cli::new();
    let (p, _, action, _) = an_automation(&cli);

    let out = cli.json(&["automation", "action-scope-set", &action, "--global", "--json"]);
    assert_eq!(out["automation_action"]["project_id"], Value::Null);
    let device = cli.json(&["automation", "action-list", "--global", "--json"]);
    let id = serde_json::json!(action.parse::<i64>().unwrap());
    let on_the_device = device["actions"].as_array().expect("the actions").iter().any(|card| card["action"]["id"] == id);
    assert!(on_the_device, "it is on the device's shelf now: {device}");

    let other = cli.a_project();
    let (refused, code) = cli.run_err(&["automation", "action-scope-set", &action, "--project", &other, "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("'A'"), "it names the automation that places it: {refused}");
    let still = cli.json(&["automation", "action-show", &action, "--json"]);
    assert_eq!(still["action"]["project_id"], Value::Null, "and nothing moved");

    let back = cli.json(&["automation", "action-scope-set", &action, "--project", &p, "--json"]);
    assert_eq!(back["automation_action"]["project_id"].to_string(), p, "every placement is in this project");
}

/// `--axis` belongs to the built-in that splits by one. Passed with a library action it is refused
/// rather than dropped, and nothing is placed.
#[test]
fn an_axis_given_with_a_library_action_is_refused() {
    let cli = Cli::new();
    let (p, a, action, _) = an_automation(&cli);
    let before = cli.json(&["automation", "show", &a, "--json"]);
    let (refused, code) = cli.run_err(&["automation", "place-add", &a, "--action", &action, "--axis", "x", "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("--axis"), "{refused}");
    let after = cli.json(&["automation", "show", &a, "--json"]);
    assert_eq!(before, after, "nothing was placed in project {p}");
}

/// An id naming nothing is a refusal, not an empty account — the same road `run show` takes.
#[test]
fn a_definition_that_does_not_exist_is_said_to_be_missing() {
    let cli = Cli::new();
    let both: [&[&str]; 2] = [
        &["automation", "show", "404", "--json"],
        &["automation", "action-show", "404", "--json"],
    ];
    for args in both {
        let (refused, code) = cli.run_err(args);
        assert_ne!(code, 0, "{refused}");
        assert!(refused.contains("404"), "{refused}");
    }
}

// ───────────────────────────── saving one ─────────────────────────────

/// An automation is saved once the launch check passes, and a save with nothing written since is said
/// to be one. What is written after it is thrown away by `discard`, which confirms first.
#[test]
fn an_automation_is_saved_and_what_is_written_after_is_thrown_away() {
    let cli = Cli::new();
    let (half, _, _, _) = an_automation(&cli);
    let (err, code) = cli.run_err(&["automation", "save", &half, "--json"]);
    assert_ne!(code, 0, "a half-drawn automation is not saved: {err}");
    assert!(err.contains("not_ready"), "{err}");
    let (err, code) = cli.run_err(&["automation", "discard", &half, "--yes", "--json"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("never been saved"), "{err}");

    let (a, _, _) = a_launchable(&cli);
    let saved = cli.json(&["automation", "save", &a, "--json"]);
    assert_eq!(saved["automation_version"]["version"].as_i64(), Some(1), "{saved}");
    assert_eq!(saved["noop"], serde_json::json!(false));
    let again = cli.json(&["automation", "save", &a, "--json"]);
    assert_eq!(again["automation_version"]["version"].as_i64(), Some(1), "{again}");
    assert_eq!(again["noop"], serde_json::json!(true), "nothing was written since");

    cli.json(&["automation", "place-add", &a, "--builtin", "close_task", "--json"]);
    let (refused, code) = cli.run_err(&["automation", "discard", &a, "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("confirmation"), "{refused}");
    let discarded = cli.json(&["automation", "discard", &a, "--yes", "--json"]);
    assert_eq!(discarded["automation_version"]["version"].as_i64(), Some(1), "{discarded}");
    assert_eq!(discarded["noop"], serde_json::json!(false));
    let shown = cli.json(&["automation", "show", &a, "--json"]);
    assert_eq!(shown["placements"].as_array().map(Vec::len), Some(3), "the placement added since went: {shown}");
}

/// An action is saved as versions of its own, and a placement keeps the one it stands on until
/// `place-version` moves it. What is written inside it after a save is thrown away by `action-discard`.
#[test]
fn an_action_is_saved_and_a_placement_is_moved_onto_its_version() {
    let cli = Cli::new();
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]), "automation");
    an_entry(&cli, &a, "take_task");
    let (action, step) = an_action(&cli, &p, "one", "do it");
    let placement = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]),
        "automation_placement",
    );
    let (err, code) = cli.run_err(&["automation", "action-discard", &action, "--yes", "--json"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("never been saved"), "{err}");
    let (err, code) = cli.run_err(&["automation", "action-save", &action, "--json"]);
    assert_ne!(code, 0, "the step inside leaves by a way out with nothing after it: {err}");
    assert!(err.contains("not_ready"), "{err}");

    cli.json(&["automation", "edge-add", "--in-action", "--from", &format!("{step}:"), "--exit-to", "--json"]);
    let first = cli.json(&["automation", "action-save", &action, "--json"]);
    assert_eq!(first["automation_action_version"]["version"].as_i64(), Some(1), "{first}");
    assert_eq!(first["noop"], serde_json::json!(false));
    let again = cli.json(&["automation", "action-save", &action, "--json"]);
    assert_eq!(again["noop"], serde_json::json!(true), "nothing was written since: {again}");

    cli.json(&["automation", "step-update", &step, "--prompt", "do it again", "--json"]);
    let second = cli.json(&["automation", "action-save", &action, "--json"]);
    assert_eq!(second["automation_action_version"]["version"].as_i64(), Some(2), "{second}");

    let moved = cli.json(&["automation", "place-version", &placement, "2", "--json"]);
    assert_eq!(moved["automation_placement"]["version"].as_i64(), Some(2), "{moved}");
    let back = cli.json(&["automation", "place-version", &placement, "1", "--json"]);
    assert_eq!(back["automation_placement"]["version"].as_i64(), Some(1), "{back}");
    let (err, code) = cli.run_err(&["automation", "place-version", &placement, "3", "--json"]);
    assert_ne!(code, 0, "a version the action does not have: {err}");
    assert!(err.contains("not_found"), "{err}");

    cli.json(&["automation", "step-update", &step, "--prompt", "never mind", "--json"]);
    let (refused, code) = cli.run_err(&["automation", "action-discard", &action, "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("confirmation"), "{refused}");
    let discarded = cli.json(&["automation", "action-discard", &action, "--yes", "--json"]);
    assert_eq!(discarded["automation_action_version"]["version"].as_i64(), Some(2), "{discarded}");
    assert_eq!(discarded["noop"], serde_json::json!(false));
    let after = cli.json(&["automation", "action-save", &action, "--json"]);
    assert_eq!(after["noop"], serde_json::json!(true), "the draft is version 2 again: {after}");
}

/// A built-in's action is Amenbo's, one version already, so its placement is not moved and the
/// action is neither saved nor thrown back by hand.
#[test]
fn a_built_in_is_not_saved_or_moved_by_hand() {
    let cli = Cli::new();
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]), "automation");
    let take = an_entry(&cli, &a, "take_task");
    let (err, code) = cli.run_err(&["automation", "place-version", &take, "1", "--json"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("built into Amenbo"), "{err}");

    let builtin = id_str(&cli.json(&["automation", "show", &a, "--json"])["placements"][0]["placement"]["action_id"]);
    let (err, code) = cli.run_err(&["automation", "action-save", &builtin, "--json"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("built into Amenbo"), "{err}");
    let (err, code) = cli.run_err(&["automation", "action-discard", &builtin, "--yes", "--json"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("built into Amenbo"), "{err}");
}

// ───────────────────────────── running one ─────────────────────────────

/// An automation that launches as it stands: the built-in that takes a task, with one there for it to
/// take, then one action of one step that works on it, then the built-in that closes it — every way
/// out decided and an agent chosen for the step where it is placed. Answers the automation's id, the
/// action's placement and the step's.
fn a_launchable(cli: &Cli) -> (String, String, String) {
    let (a, placement, step, _) = a_picture(cli, true);
    (a, placement, step)
}

/// [`a_launchable`], with the placement of its entry handed back too, and a task for it to take only
/// where one is asked for.
fn a_picture(cli: &Cli, with_task: bool) -> (String, String, String, String) {
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "Do one", "--json"]), "automation");
    if with_task {
        let t = id_str(&cli.json(&["task", "add", "--title", "one", "--project", &p, "--json"])["task"]["id"]);
        cli.finish_creating(&t);
    }
    // The first thing placed is where a run starts (`AMB-D-977`), set to take any task that is ready.
    let take = an_entry(cli, &a, "take_task");
    cli.json(&["automation", "cfg-set", &take, "--name", "絞り込み", "--status", "todo", "--json"]);
    let (action, step) = an_action(cli, &p, "work", "work on it");
    // The step inside leaves the action by its done way out — the launch check asks the picture
    // inside an action the same question it asks the automation's.
    cli.json(&["automation", "edge-add", "--in-action", "--from", &format!("{step}:"), "--exit-to", "--json"]);
    let placement = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]),
        "automation_placement",
    );
    cli.json(&["automation", "agent-set", &placement, "--step", &step, "--agent", "claude", "--json"]);
    // Every way out of a reachable spot is answered for, and the task taken is closed before the run
    // ends (`AMB-D-967`) — the two things the launch check asks about the picture.
    let close = id_of(
        &cli.json(&["automation", "place-add", &a, "--builtin", "close_task", "--json"]),
        "automation_placement",
    );
    cli.json(&["automation", "edge-add", "--from", &format!("{take}:着手した"), "--to", &placement, "--json"]);
    cli.json(&["automation", "edge-add", "--from", &format!("{take}:着手できるタスクが無い"), "--done", "--json"]);
    cli.json(&["automation", "edge-add", "--from", &format!("{placement}:"), "--to", &close, "--json"]);
    cli.json(&["automation", "edge-add", "--from", &format!("{close}:"), "--done", "--json"]);
    (a, placement, step, take)
}

/// A launch makes a run, and the run is the record every later command names.
///
/// **Nothing is claimed about the workspace.** A terminal cannot see what is on screen, so the launch
/// says nothing about it rather than refusing a launch the reader can see perfectly well — and the run
/// waits for whatever opens its first step.
#[test]
fn a_launch_makes_a_run_and_the_run_is_what_pause_and_cancel_name() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let (a, _, _) = a_launchable(&cli);

    let started = cli.json(&["automation", "start", &a, "--json"]);
    assert_eq!(started["automation_run"]["status"].as_str(), Some("running"));
    let run = id_of(&started, "automation_run");

    // A running run pauses at the end of the action under way, so what comes back is the asking rather
    // than the pause.
    let paused = cli.json(&["automation", "pause", &run, "--json"]);
    assert_eq!(paused["automation_run"]["state"].as_str(), Some("asked"));

    // Still going, so a plain cancel is refused (`AMB-D-1002`), naming both ways on.
    let (refused, code) = cli.run_err(&["automation", "cancel", &run, "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("only a paused run is canceled"), "{refused}");
    assert!(refused.contains(&format!("automation pause {run}")), "{refused}");
    assert!(refused.contains(&format!("automation cancel {run} --force")), "{refused}");

    let canceled = cli.json(&["automation", "cancel", &run, "--force", "--json"]);
    assert_eq!(canceled["automation_run"]["status"].as_str(), Some("canceled"));
    assert!(canceled["automation_run"]["stopped_reason"].is_null(), "a cancel carries no reason");
}

/// **A run is asked to pause before its next task** (`AMB-D-1019`), by its id. Asked again, it is
/// answered as it stands, still asked, and a whole project is no longer something to ask.
#[test]
fn pause_before_next_task_asks_the_run() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let (a, _, _) = a_launchable(&cli);
    let run = id_of(&cli.json(&["automation", "start", &a, "--json"]), "automation_run");
    let project = cli.json(&["automation", "run-show", &run, "--json"])["run"]["project_id"].to_string();

    let asked = cli.json(&["automation", "pause", &run, "--before-next-task", "--json"]);
    assert_eq!(asked["automation_run"]["run"].to_string(), run, "{asked}");
    assert_eq!(asked["automation_run"]["state"].as_str(), Some("asked"), "{asked}");
    let shown = cli.json(&["automation", "run-show", &run, "--json"]);
    assert_eq!(shown["run"]["status"].as_str(), Some("running"), "{shown}");
    assert_eq!(shown["run"]["pause_before_next_task"].as_bool(), Some(true), "{shown}");
    assert_eq!(shown["run"]["pause_requested"].as_bool(), Some(false), "{shown}");

    let again = cli.json(&["automation", "pause", &run, "--before-next-task", "--json"]);
    assert_eq!(again["automation_run"]["state"].as_str(), Some("asked"), "{again}");

    let (refused, code) = cli.run_err(&["automation", "pause", "--before-next-task", "--project", &project, "--json"]);
    assert_ne!(code, 0, "{refused}");

    cli.json(&["automation", "cancel", &run, "--force", "--json"]);
}

/// **A step does not pause or force-cancel its own run** (`AMB-D-1020`): typed in a step's terminal, both
/// are refused, naming why and that a person can stop the run from its pane. Aimed at another run, they
/// go through as they would outside a step.
#[test]
fn inside_a_step_its_own_run_is_not_paused_or_force_canceled() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let (a, _, _) = a_launchable(&cli);
    let own = id_of(&cli.json(&["automation", "start", &a, "--json"]), "automation_run");
    let other = id_of(&cli.json(&["automation", "start", &a, "--json"]), "automation_run");
    // No watch opens a step here, so the run's first one — the built-in that takes a task — is opened
    // on the store, as the app's watch would.
    let step = {
        let run: i64 = own.parse().unwrap();
        let paths = amenbo_core::config::Paths::at(cli.home.clone());
        let mut store = amenbo_core::Store::open_at(paths).expect("the store");
        let entry = store.automation_run_defs(run).expect("defs").into_iter().find(|d| d.entry).expect("an entry");
        match store.automation_step_open(run, entry.id, None).expect("open") {
            amenbo_core::ops::automation_step::Opened::Carried { run_step_id, .. } => run_step_id.to_string(),
            _ => panic!("the entry takes a task and is carried out"),
        }
    };
    let inside = [("AMENBO_AUTOMATION_STEP", step.as_str())];

    for args in [
        vec!["automation", "pause", &own, "--json"],
        vec!["automation", "pause", &own, "--before-next-task", "--json"],
        vec!["automation", "cancel", &own, "--force", "--json"],
    ] {
        let (refused, code) = cli.run_env_err(&inside, &args);
        assert_eq!(code, 2, "{args:?}: {refused}");
        assert!(refused.contains("unable to finish its work"), "{args:?}: {refused}");
        assert!(refused.contains("from its pane"), "{args:?}: {refused}");
    }
    let shown = cli.json(&["automation", "run-show", &own, "--json"]);
    assert_eq!(shown["run"]["status"].as_str(), Some("running"), "{shown}");
    assert_eq!(shown["run"]["pause_requested"].as_bool(), Some(false), "{shown}");
    assert_eq!(shown["run"]["pause_before_next_task"].as_bool(), Some(false), "{shown}");

    for args in [
        vec!["automation", "pause", &other, "--json"],
        vec!["automation", "pause", &other, "--before-next-task", "--json"],
        vec!["automation", "cancel", &other, "--force", "--json"],
    ] {
        let (out, code) = cli.run_env(&inside, &args);
        assert_eq!(code, 0, "{args:?}: {out}");
    }

    cli.json(&["automation", "cancel", &own, "--force", "--json"]);
}

/// **A run waiting for a task is paused by the time the asking returns** (`AMB-D-1019`) — with no
/// watch looking on to open its step again.
#[test]
fn pause_before_next_task_pauses_a_run_waiting_for_a_task_at_once() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let (a, _, _, take) = a_picture(&cli, false);
    cli.json(&[
        "automation", "cfg-set", &take, "--name", "着手できるタスクが無いとき",
        "--choice", "着手できるタスクが出るまで待つ", "--json",
    ]);
    let run = id_of(&cli.json(&["automation", "start", &a, "--json"]), "automation_run");
    assert_eq!(cli.json(&["automation", "run-show", &run, "--json"])["waiting"].as_bool(), Some(true));

    let asked = cli.json(&["automation", "pause", &run, "--before-next-task", "--json"]);
    assert_eq!(asked["automation_run"]["state"].as_str(), Some("paused"), "{asked}");

    let shown = cli.json(&["automation", "run-show", &run, "--json"]);
    assert_eq!(shown["run"]["status"].as_str(), Some("paused"), "{shown}");
    assert_eq!(shown["run"]["pause_before_next_task"].as_bool(), Some(false), "{shown}");

    let canceled = cli.json(&["automation", "cancel", &run, "--json"]);
    assert_eq!(canceled["automation_run"]["status"].as_str(), Some("canceled"), "{canceled}");
}

/// **Only a failure is waiting to be seen** (`AMB-D-989`): a run a person stopped needs nobody, so
/// saying it has been seen is refused, and its account says nothing about being seen. That a failure
/// takes the mark, with whoever set it, is held by the core's own tests — a terminal has no way to make
/// a run fail on purpose.
#[test]
fn only_a_failed_run_is_acknowledged() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let (a, _, _) = a_launchable(&cli);
    let run = id_of(&cli.json(&["automation", "start", &a, "--json"]), "automation_run");
    cli.json(&["automation", "cancel", &run, "--force", "--json"]);

    let (refused, code) = cli.run_err(&["automation", "acknowledge", &run, "--json"]);
    assert_ne!(code, 0, "{refused}");
    assert!(refused.contains("only a failed run"), "{refused}");

    let (shown, _) = cli.run(&["automation", "run-show", &run]);
    assert!(shown.contains("status: canceled"), "{shown}");
    assert!(!shown.contains("acknowledged:"), "a cancel is not waiting on anyone: {shown}");
}

/// **A run waiting for a task says so** on `run-show`, in the same answer its pane is drawn from: an
/// entry set to wait, with nothing to take, leaves the run running and waiting. A run that is no longer
/// running waits for nothing.
#[test]
fn run_show_says_when_a_run_is_waiting_for_a_task() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let (a, _, _, take) = a_picture(&cli, false);
    cli.json(&[
        "automation", "cfg-set", &take, "--name", "着手できるタスクが無いとき",
        "--choice", "着手できるタスクが出るまで待つ", "--json",
    ]);
    let run = id_of(&cli.json(&["automation", "start", &a, "--json"]), "automation_run");

    let shown = cli.json(&["automation", "run-show", &run, "--json"]);
    assert_eq!(shown["run"]["status"].as_str(), Some("running"), "{shown}");
    assert_eq!(shown["waiting"].as_bool(), Some(true), "{shown}");
    let (text, _) = cli.run(&["automation", "run-show", &run]);
    assert!(text.contains("waiting: for a task"), "{text}");

    cli.json(&["automation", "cancel", &run, "--force", "--json"]);
    let shown = cli.json(&["automation", "run-show", &run, "--json"]);
    assert_eq!(shown["waiting"].as_bool(), Some(false), "{shown}");
    let (text, _) = cli.run(&["automation", "run-show", &run]);
    assert!(!text.contains("waiting:"), "{text}");
}

/// **What a person hands over comes in on the launch itself** (`AMB-D-981`): for a run that starts by
/// filing a task, its title, its notes and the files to attach to it. A file that cannot be read is
/// refused before anything is started, so no run is left holding half of what it was handed, and a
/// text on its own is not a thing the launch takes at all.
#[test]
fn a_launch_takes_what_its_entry_reads_and_refuses_the_rest() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "File one", "--json"]), "automation");
    let make = an_entry(&cli, &a, "make_task");
    cli.json(&[
        "automation", "cfg-set", &make, "--name", "起票したタスク",
        "--choice", "進行中にして、出口「起票して着手した」へ進む", "--json",
    ]);
    let close = id_of(
        &cli.json(&["automation", "place-add", &a, "--builtin", "close_task", "--json"]),
        "automation_placement",
    );
    cli.json(&["automation", "edge-add", "--from", &format!("{make}:起票して着手した"), "--to", &close, "--json"]);
    cli.json(&["automation", "edge-add", "--from", &format!("{close}:"), "--done", "--json"]);

    let brief = cli.home.join("brief.md");
    std::fs::write(&brief, "the login page loses the password field\n").expect("write the brief");
    let started = cli.json(&[
        "automation", "start", &a, "--title", "file this", "--notes", "# a draft",
        "--file", brief.to_str().unwrap(), "--json",
    ]);
    let run = id_of(&started, "automation_run");

    let (err, code) = cli.run_err(&["automation", "start", &a, "--title", "again", "--text", "for a step", "--json"]);
    assert_ne!(code, 0, "--text is gone: {err}");
    let missing = cli.home.join("not-there.md");
    let (err, code) = cli.run_err(&[
        "automation", "start", &a, "--title", "again", "--file", missing.to_str().unwrap(), "--json",
    ]);
    assert_ne!(code, 0, "an unreadable file is refused: {err}");
    assert!(err.contains("not_found"), "{err}");
    assert!(err.contains("pass a readable file path"), "the hint says how to fix it: {err}");
    assert!(!err.contains("--url"), "and names no flag this command does not have: {err}");
    let runs = cli.json(&["automation", "run-list", "--automation", &a, "--json"]);
    assert_eq!(runs["count"].as_u64(), Some(1), "only the first launch made a run: {runs}");
    assert!(runs.to_string().contains(&run), "{runs}");
}

/// **With no app up on the store, nothing is started** (`AMB-D-995`). A step's terminal is opened by the
/// app, so a run accepted now would stand `running` with nothing moving until the next launch ended it
/// `crashed`. The refusal says to start the app, and the same launch goes through once it is up.
#[test]
fn a_launch_is_refused_while_the_app_is_not_running() {
    let cli = Cli::new();
    let (a, _, _) = a_launchable(&cli);

    let (err, code) = cli.run_err(&["automation", "start", &a, "--json"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("app_not_running"), "{err}");
    assert!(err.contains("Start the app"), "the hint says what to do: {err}");
    let runs = cli.json(&["automation", "run-list", "--automation", &a, "--json"]);
    assert_eq!(runs["count"].as_u64(), Some(0), "no run was made: {runs}");

    let app = cli.the_app_up();
    cli.json(&["automation", "start", &a, "--json"]);
    app.release();
    let (err, _) = cli.run_err(&["automation", "start", &a, "--json"]);
    assert!(err.contains("app_not_running"), "an app that quit is no app: {err}");
}

/// The launch check refuses an unfinished automation and names what is missing. Nothing on the
/// building side ever did: a picture is half-built for as long as somebody is drawing it, and this is
/// the moment a person is about to be let down by one.
#[test]
fn a_launch_is_refused_while_a_way_out_has_nothing_after_it() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "Half drawn", "--json"]), "automation");
    an_entry(&cli, &a, "take_task");

    let (err, code) = cli.run_err(&["automation", "start", &a, "--json"]);
    assert_ne!(code, 0, "an unfinished automation does not launch: {err}");
    assert!(err.contains("not_ready"), "{err}");
    assert!(err.contains("nothing is set to happen after"), "it names what is missing: {err}");
}

/// Who carries a step out is chosen where its action is placed (`AMB-D-960`): with nobody chosen the
/// launch is refused and says so, `show` lists the choice at each spot, and `--clear` takes it back.
#[test]
fn a_step_is_carried_out_by_whoever_is_chosen_where_it_is_placed() {
    let cli = Cli::new();
    let _app = cli.the_app_up();
    let (a, placement, step) = a_launchable(&cli);

    let chosen = cli.json(&[
        "automation", "agent-set", &placement, "--step", &step, "--agent", "codex", "--model", "gpt-5",
        "--json",
    ]);
    assert_eq!(chosen["automation_placement_step"]["agent"].as_str(), Some("codex"));
    assert_eq!(chosen["automation_placement_step"]["model"].as_str(), Some("gpt-5"));
    let (shown, _) = cli.run(&["automation", "show", &a]);
    assert!(shown.contains("carried out by codex (gpt-5)"), "{shown}");
    // The built-in that closes the task is Amenbo's own, so nobody is left unchosen for it
    // (`AMB-D-964`).
    assert!(shown.contains("carried out by Amenbo (built-in close_task)"), "{shown}");
    assert!(!shown.contains("nobody chosen"), "{shown}");

    cli.json(&["automation", "agent-set", &placement, "--step", &step, "--clear", "--json"]);
    let (shown, _) = cli.run(&["automation", "show", &a]);
    assert_eq!(shown.matches("nobody chosen to carry it out").count(), 1, "only the agent's step: {shown}");
    let (err, code) = cli.run_err(&["automation", "start", &a, "--json"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("nobody is chosen to carry out"), "{err}");

    let (err, code) = cli.run_err(&["automation", "agent-set", &placement, "--step", &step, "--json"]);
    assert_eq!(code, 2, "an agent or --clear has to be said: {err}");
}

/// A script step runs its own program (`AMB-D-1016`), so `show` names the program and its arguments
/// where an agent's step names the agent — never the agent chosen by default where it was placed.
#[test]
fn a_script_step_is_shown_carried_out_by_its_program() {
    let cli = Cli::new();
    let (p, a, _, _) = an_automation(&cli);
    let action = id_of(
        &cli.json(&["automation", "action-add", "--project", &p, "--name", "run it", "--json"]),
        "automation_action",
    );
    let step = id_of(
        &cli.json(&[
            "automation", "step-add", &action, "--name", "run it", "--program", "/bin/echo", "--arg",
            "--verbose", "--arg", "hello", "--json",
        ]),
        "automation_step",
    );
    cli.json(&["automation", "action-entry-set", &action, "--step", &step, "--json"]);
    cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]);

    let (shown, _) = cli.run(&["automation", "show", &a]);
    assert!(shown.contains("run it  carried out by a script: /bin/echo --verbose hello"), "{shown}");
    assert!(!shown.contains("run it  carried out by claude"), "{shown}");
}

/// The two verbs a step's own agent types refuse outside a step, and say why.
///
/// **There is no "the current step" to fall back on.** Several runs go at once, so a command that
/// guessed would put one step's report on another's record — and the guess would look like it worked.
#[test]
fn the_verbs_a_step_types_refuse_outside_a_step() {
    let cli = Cli::new();

    for args in [
        vec!["automation", "step-out", "12=done", "--json"],
        vec!["automation", "step-done", "--report", "did it", "--json"],
    ] {
        let (err, code) = cli.run_err(&args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(err.contains("this is not a step of a run"), "{args:?}: {err}");
    }
}

/// A step execution named by the environment is read from there and nowhere else — and one that names
/// no row is refused rather than falling back to whatever is newest.
#[test]
fn a_step_execution_that_does_not_exist_is_refused_rather_than_guessed_at() {
    let cli = Cli::new();
    let (err, code) = cli.run_env_err(
        &[("AMENBO_AUTOMATION_STEP", "9999")],
        &["automation", "step-done", "--report", "did it", "--json"],
    );
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("9999"), "it names the row it was pointed at: {err}");
}

/// **Inside a step, every command is typed as it is outside a run** (`AMB-D-1011`) — what moves a
/// task, what writes a decision, and what builds an automation alike. Nothing is refused for being
/// typed in a step's terminal.
#[test]
fn inside_a_step_what_was_once_refused_goes_through() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // What the AI reaches is the bound project, so the task is filed there.
    let p = cli.bound_project();
    let t = id_str(&cli.json(&["task", "add", "--title", "one", "--project", &p, "--json"])["task"]["id"]);
    cli.finish_creating(&t);

    for args in [
        vec!["--actor", "ai", "task", "status", &t, "in_progress", "--json"],
        vec!["--actor", "ai", "decision", "add", "--title", "why", "--json"],
        vec!["automation", "add", "--project", &p, "--name", "one", "--json"],
    ] {
        let (out, code) = cli.run_env(&[("AMENBO_AUTOMATION_STEP", "1")], &args);
        assert_eq!(code, 0, "{args:?}: {out}");
    }
    let shown = cli.json(&["task", "show", &t, "--json"]);
    assert_eq!(shown["status"], "in_progress", "{shown}");
}

/// The reading verbs answer inside a step, so the agent carrying it out can see where it stands — and
/// the action layer is among them, because the prompts moved there. `show` alone would hand back the
/// placements and nothing of what stands at one.
#[test]
fn inside_a_step_the_reading_verbs_still_answer() {
    let cli = Cli::new();
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "one", "--json"]), "automation");
    let (action, _) = an_action(&cli, &p, "Review", "review it");

    for args in [
        vec!["automation", "list", "--project", &p, "--json"],
        vec!["automation", "show", &a, "--json"],
        vec!["automation", "action-list", "--project", &p, "--json"],
        vec!["automation", "action-show", &action, "--json"],
        vec!["automation", "run-list", "--automation", &a, "--json"],
    ] {
        let (out, code) = cli.run_env(&[("AMENBO_AUTOMATION_STEP", "1")], &args);
        assert_eq!(code, 0, "{args:?}: {out}");
    }
}

/// **Inside a step, a task's edges and fields can be changed** (`AMB-D-1011`), on any task in the
/// project as outside a run — a step that files several tasks orders them and hangs them on their
/// premise.
#[test]
fn inside_a_step_a_tasks_edges_and_fields_can_be_changed() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // What the AI reaches is the bound project, so the tasks are filed there.
    let p = cli.bound_project();
    let t = id_str(&cli.json(&["task", "add", "--title", "one", "--project", &p, "--json"])["task"]["id"]);
    let u = id_str(&cli.json(&["task", "add", "--title", "two", "--project", &p, "--json"])["task"]["id"]);
    let d = id_str(&cli.json(&["decision", "add", "--project", &p, "--title", "why", "--json"])["decision"]["id"]);
    cli.json(&["dimension", "add", "--project", &p, "--name", "Area", "--json"]);
    cli.json(&["dimension", "value-add", "Area", "--name", "core", "--json"]);
    let (tr, ur) = (task_ref(&t), task_ref(&u));

    for args in [
        vec!["--actor", "ai", "task", "depend", &tr, "--on", &ur, "--json"],
        vec!["--actor", "ai", "task", "undepend", &tr, "--on", &ur, "--json"],
        vec!["--actor", "ai", "decision", "link", &d, &tr, "--json"],
        vec!["--actor", "ai", "decision", "link", &d, &tr, "--unlink", "--json"],
        vec!["--actor", "ai", "dimension", "set", &tr, "Area", "core", "--json"],
        vec!["--actor", "ai", "dimension", "unset", &tr, "Area", "core", "--json"],
        vec!["--actor", "ai", "task", "update", &tr, "--priority", "high", "--json"],
    ] {
        let (err, code) = cli.run_env_err(&[("AMENBO_AUTOMATION_STEP", "1")], &args);
        assert_eq!(code, 0, "{args:?}: {err}");
    }
    let shown = cli.json(&["task", "show", &t, "--json"]);
    assert_eq!(shown["priority"], "high", "{shown}");
}

/// **Inside a step, `agent --json` is the step's own entry** (`AMB-T-5385`): the folder sends every
/// step's fresh session there first, and the whole entry is about a mailbox a step does not work. The
/// two verbs that hand the work back come in full; `--full` still answers with everything.
#[test]
fn inside_a_step_the_entry_is_the_steps_own() {
    let cli = Cli::new();
    let (out, code) = cli.run_env(&[("AMENBO_AUTOMATION_STEP", "1")], &["agent", "--json"]);
    assert_eq!(code, 0, "{out}");
    let entry: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(entry["mode"], "step", "{out}");
    assert!(entry.get("agentCycle").is_none(), "none of the mailbox comes with it: {out}");
    let names: Vec<&str> =
        entry["commands"].as_array().expect("commands").iter().filter_map(|c| c["name"].as_str()).collect();
    assert_eq!(names, ["automation step-out", "automation step-done"]);

    let (out, code) = cli.run_env(&[("AMENBO_AUTOMATION_STEP", "1")], &["agent", "--json", "--full"]);
    assert_eq!(code, 0, "{out}");
    let full: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(full["mode"], "personal", "--full is asked for on purpose");

    let (out, _) = cli.run_env(&[], &["agent", "--json"]);
    let outside: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert!(outside.get("agentCycle").is_some(), "outside a step the entry is the whole one");
}
