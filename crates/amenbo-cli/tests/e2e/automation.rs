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
            "automation", "step-add", &action, "--name", name, "--prompt", prompt, "--agent",
            "claude", "--json",
        ]),
        "automation_step",
    );
    cli.json(&["automation", "action-entry-set", &action, "--step", &step, "--json"]);
    (action, step)
}

/// An automation with one action placed on it — the shape most of these tests start from. Answers the
/// project it is in, the automation, the action and the placement. The project is named rather than
/// left to the folder: the harness runs in a home nothing has bound.
fn an_automation(cli: &Cli) -> (String, String, String, String) {
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]), "automation");
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
    cli.json(&["automation", "port-add", "--exit", &found, "--name", "report", "--kind", "file", "--json"]);
    cli.json(&["automation", "port-add", "--action", &fix_action, "--name", "report", "--kind", "file", "--required", "--json"]);

    cli.json(&["automation", "entry-set", &a, "--placement", &review, "--json"]);

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
    assert_eq!(wire["automation_wire"]["from_port_name"].as_str(), Some("report"));

    let note = id_of(&cli.json(&["automation", "note-add", &a, "--name", "House style", "--body", "short lines", "--json"]), "automation_note");
    cli.json(&["automation", "note-link", &review, &note, "--json"]);
    let unlinked = cli.json(&["automation", "note-unlink", &review, &note, "--json"]);
    assert_eq!(unlinked["automation_placement_note"]["unlinked"], serde_json::json!(true));
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

/// `--from` is one token, `<box>:<way out>`, because a way out is named against whichever of the box
/// and the action standing on it declares it — a name without its box names nothing. The colon with
/// nothing after it is the unnamed way out, and `*` the error one.
#[test]
fn an_edge_names_its_way_out_on_the_box_it_leaves_from() {
    let cli = Cli::new();
    let (_, _, _, placement) = an_automation(&cli);

    let unnamed = cli.json(&["automation", "edge-add", "--from", &format!("{placement}:"), "--halt", "--json"]);
    assert_eq!(unnamed["automation_edge"]["exit_name"], Value::Null);
    assert_eq!(unnamed["automation_edge"]["ends"].as_str(), Some("halt"));

    let errored = cli.json(&["automation", "edge-add", "--from", &format!("{placement}:*"), "--done", "--json"]);
    assert_eq!(errored["automation_edge"]["exit_name"].as_str(), Some("*"));

    // No colon at all reads as the unnamed way out too — and that one already says what happens.
    let (again, code) = cli.run_err(&["automation", "edge-add", "--from", &placement, "--done", "--json"]);
    assert_ne!(code, 0, "the unnamed way out already says what happens after it");
    assert!(again.contains("unnamed"), "{again}");

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
        &cli.json(&["automation", "step-add", &action, "--name", "again", "--prompt", "look again", "--agent", "claude", "--json"]),
        "automation_step",
    );

    let inside = cli.json(&[
        "automation", "edge-add", "--in-action", "--from", &format!("{first}:"), "--to", &second,
        "--json",
    ]);
    assert_eq!(inside["automation_edge"]["owner_kind"].as_str(), Some("action"));
    assert_eq!(inside["automation_edge"]["owner_id"], serde_json::json!(action.parse::<i64>().unwrap()));

    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "A", "--json"]), "automation");
    let placement = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]),
        "automation_placement",
    );
    let outside = cli.json(&["automation", "edge-add", "--from", &format!("{placement}:"), "--done", "--json"]);
    assert_eq!(outside["automation_edge"]["owner_kind"].as_str(), Some("automation"));
    assert_eq!(outside["automation_edge"]["owner_id"], serde_json::json!(a.parse::<i64>().unwrap()));
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
    cli.json(&["automation", "entry-set", &a, "--placement", &review, "--json"]);
    cli.json(&["automation", "edge-add", "--from", &format!("{review}:something to fix"), "--to", &fix, "--json"]);
    cli.json(&["automation", "wire-add", "--from", &format!("{review}:something to fix"), "--from-port", "report", "--to", &fix, "--to-port", "report", "--json"]);
    let note = id_of(&cli.json(&["automation", "note-add", &a, "--name", "House style", "--body", "short lines", "--json"]), "automation_note");
    cli.json(&["automation", "note-link", &review, &note, "--json"]);

    let shown = cli.json(&["automation", "show", &a, "--json"]);
    assert_eq!(shown["automation"]["name"].as_str(), Some("Review and fix"));
    assert_eq!(shown["placements"].as_array().map(|s| s.len()), Some(2));

    // The ways out a spot can be left by are the action's — resolved here rather than left for the
    // reader to go and look up.
    let first = &shown["placements"][0];
    assert_eq!(first["action"]["name"].as_str(), Some("Review"));
    // Every declarer is born carrying the unnamed way out and the error one, so the one built here
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

    assert_eq!(shown["placements"][1]["inputs"][0]["name"].as_str(), Some("report"));
    assert_eq!(shown["edges"][0]["to_id"], serde_json::json!(fix.parse::<i64>().unwrap()));
    assert_eq!(shown["wires"][0]["to_port_name"].as_str(), Some("report"));
    assert_eq!(shown["notes"][0]["note"]["name"].as_str(), Some("House style"));
    assert_eq!(shown["notes"][0]["placement_ids"][0], serde_json::json!(review.parse::<i64>().unwrap()));

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
    assert_eq!(listed["automations"][0]["placements"], serde_json::json!(1));
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
    for _ in 0..2 {
        cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]);
    }
    let again = cli.json(&["automation", "action-list", "--project", &p, "--json"]);
    assert_eq!(again["actions"][0]["used_by"], serde_json::json!(1));

    let shown = cli.json(&["automation", "action-show", &action, "--json"]);
    assert_eq!(shown["steps"][0]["step"]["prompt"].as_str(), Some("review it"));
    assert_eq!(shown["used_by"], serde_json::json!(1));
    // Every declarer is born carrying the unnamed way out and the error one.
    assert_eq!(shown["exits"].as_array().map(|x| x.len()), Some(2));
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

// ───────────────────────────── running one ─────────────────────────────

/// An automation that launches as it stands: one action of one step that takes a task and closes the
/// run, with every way out of it decided. Answers the automation's id.
fn a_launchable(cli: &Cli) -> String {
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "Do one", "--json"]), "automation");
    let (action, _) = an_action(cli, &p, "take one", "take one");
    let placement = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]),
        "automation_placement",
    );
    // The way out the task comes out on, which is what makes this spot usable as an entry.
    let took = id_of(
        &cli.json(&["automation", "exit-add", "--action", &action, "--name", "took one", "--json"]),
        "automation_exit",
    );
    cli.json(&["automation", "port-add", "--exit", &took, "--name", "task", "--kind", "task_take", "--required", "--json"]);
    cli.json(&["automation", "entry-set", &a, "--placement", &placement, "--json"]);
    // Every way out of a reachable spot is answered for, which is the whole of what the launch check
    // asks about the picture.
    cli.json(&["automation", "edge-add", "--from", &format!("{placement}:took one"), "--done", "--json"]);
    cli.json(&["automation", "edge-add", "--from", &format!("{placement}:"), "--done", "--json"]);
    a
}

/// A launch makes a run, and the run is the record every later command names.
///
/// **Nothing is claimed about the workspace.** A terminal cannot see what is on screen, so the launch
/// says nothing about it rather than refusing a launch the reader can see perfectly well — and the run
/// waits for whatever opens its first step.
#[test]
fn a_launch_makes_a_run_and_the_run_is_what_pause_and_stop_name() {
    let cli = Cli::new();
    let a = a_launchable(&cli);

    let started = cli.json(&["automation", "start", &a, "--json"]);
    assert_eq!(started["automation_run"]["status"].as_str(), Some("running"));
    let run = id_of(&started, "automation_run");

    // A running run pauses at the end of the step under way, so what comes back is the asking rather
    // than the pause.
    let paused = cli.json(&["automation", "pause", &run, "--json"]);
    assert_eq!(paused["automation_run"]["state"].as_str(), Some("asked"));

    let stopped = cli.json(&["automation", "stop", &run, "--json"]);
    assert_eq!(stopped["automation_run"]["status"].as_str(), Some("stopped"));
    assert_eq!(stopped["automation_run"]["stopped_reason"].as_str(), Some("by_human"));
}

/// The launch check refuses an unfinished automation and names what is missing. Nothing on the
/// building side ever did: a picture is half-built for as long as somebody is drawing it, and this is
/// the moment a person is about to be let down by one.
#[test]
fn a_launch_is_refused_while_a_way_out_has_nothing_after_it() {
    let cli = Cli::new();
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "Half drawn", "--json"]), "automation");
    let (action, _) = an_action(&cli, &p, "one", "do it");
    let placement = id_of(
        &cli.json(&["automation", "place-add", &a, "--action", &action, "--json"]),
        "automation_placement",
    );
    cli.json(&["automation", "entry-set", &a, "--placement", &placement, "--json"]);

    let (err, code) = cli.run_err(&["automation", "start", &a, "--json"]);
    assert_ne!(code, 0, "an unfinished automation does not launch: {err}");
    assert!(err.contains("not_ready"), "{err}");
    assert!(err.contains("nothing is set to happen after"), "it names what is missing: {err}");
}

/// The three verbs a step's own agent types refuse outside a step, and say why.
///
/// **There is no "the current step" to fall back on.** Several runs go at once, so a command that
/// guessed would put one step's report on another's record — and the guess would look like it worked.
#[test]
fn the_verbs_a_step_types_refuse_outside_a_step() {
    let cli = Cli::new();
    let p = cli.a_project();
    let t = id_str(&cli.json(&["task", "add", "--title", "one", "--project", &p, "--json"])["task"]["id"]);
    cli.finish_creating(&t);

    for args in [
        vec!["automation", "step-take", &t, "--json"],
        vec!["automation", "step-out", "note=done", "--json"],
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

/// Inside a step's terminal, the verbs that build an automation or drive a run are refused — and the
/// refusal says which side they are typed on (`AMB-D-948`).
///
/// **A run that could start a run, or rewrite the definition the next run is copied from, changes what
/// is about to happen where the person who started it is not looking.**
#[test]
fn inside_a_step_the_building_and_driving_verbs_are_refused() {
    let cli = Cli::new();
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "one", "--json"]), "automation");

    for args in [
        vec!["automation", "start", &a, "--json"],
        vec!["automation", "add", "--project", &p, "--name", "another", "--json"],
        vec!["automation", "action-add", "--project", &p, "--name", "one", "--json"],
        vec!["automation", "rm", &a, "--yes", "--json"],
    ] {
        let (err, code) = cli.run_env_err(&[("AMENBO_AUTOMATION_STEP", "1")], &args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(err.contains("automation_outside_only"), "{args:?}: {err}");
        assert!(err.contains("step-take"), "it names what does reach from there: {args:?}: {err}");
    }
}

/// The reading verbs are left with a step, so the agent carrying it out can see where it stands.
#[test]
fn inside_a_step_the_reading_verbs_still_answer() {
    let cli = Cli::new();
    let p = cli.a_project();
    let a = id_of(&cli.json(&["automation", "add", "--project", &p, "--name", "one", "--json"]), "automation");

    for args in [
        vec!["automation", "list", "--project", &p, "--json"],
        vec!["automation", "show", &a, "--json"],
        vec!["automation", "run-list", "--automation", &a, "--json"],
    ] {
        let (out, code) = cli.run_env(&[("AMENBO_AUTOMATION_STEP", "1")], &args);
        assert_eq!(code, 0, "{args:?}: {out}");
    }
}
