//! `talk`, the surface layer (`AMB-D-749`), named for the window it reaches (`AMB-D-757`): what it does
//! inside a pane of the talk window, and what it refuses everywhere else.
//!
//! Driven as a process because the whole of the layer's line is drawn by what the process was launched
//! with — the window names a session in the environment, and an agent runs `amenbo` several levels deep
//! inside it. Nothing about that is visible to a call made in-process.

mod harness;

use harness::*;

/// The pane a window opened a terminal in, as the environment carries it: a session id, and the
/// throwaway directory statements are left in. The directory's path is borrowed by the environment the
/// call is made with, so it is handed back for the caller to hold.
fn in_a_pane(dir: &std::path::Path) -> String {
    dir.to_string_lossy().into_owned()
}

/// The two variables, in the shape `run_env` takes them.
fn pane_env(path: &str) -> Vec<(&str, &str)> {
    vec![("AMENBO_SESSION", "pane-1"), ("AMENBO_SESSION_DIR", path)]
}

/// Every statement left in the drop box, oldest first.
fn statements(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .expect("the drop box exists")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).expect("a whole statement"))
        .collect()
}

/// Outside the window every verb fails, loudly and with a code of its own. This is the point of the
/// layer rather than a limit of it: a quiet success would leave the agent believing it had spoken while
/// the person's screen never changed (`AMB-D-749`).
#[test]
fn outside_the_talk_window_every_verb_is_refused_rather_than_quietly_accepted() {
    let cli = Cli::new();
    for verb in [
        vec!["talk", "name", "the top fix"],
        vec!["talk", "note", "reading the migration"],
        vec!["talk", "waiting", "a decision is needed"],
        vec!["talk", "finished", "it landed"],
        vec!["talk"],
    ] {
        let (stderr, code) = cli.run_err(&verb);
        assert_eq!(code, 1, "{verb:?} exits non-zero outside the window: {stderr}");
        assert!(
            stderr.contains("Nothing was recorded"),
            "{verb:?} says outright that nothing happened: {stderr}",
        );

        let mut machine = verb.clone();
        machine.push("--json");
        let (stderr, code) = cli.run_err(&machine);
        assert_eq!(code, 1, "{machine:?} exits non-zero too: {stderr}");
        assert!(
            stderr.contains("talk_outside_surface"),
            "{machine:?} says which refusal it is, in a code a caller can branch on: {stderr}",
        );
    }
}

/// Half an environment is not half a window. A session named with nowhere to leave a statement, or a
/// directory with no session to file one under, is refused exactly as the bare terminal is — the
/// statement would otherwise be written where nothing is watching, which reads as success.
#[test]
fn a_session_named_without_a_drop_box_is_still_outside_the_window() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-half");
    let path = dir.to_string_lossy().into_owned();
    for half in [
        vec![("AMENBO_SESSION", "pane-1")],
        vec![("AMENBO_SESSION_DIR", path.as_str())],
        vec![("AMENBO_SESSION", " "), ("AMENBO_SESSION_DIR", path.as_str())],
    ] {
        let (_, code) = cli.run_env(&half, &["talk", "note", "half"]);
        assert_eq!(code, 1, "half a window is not a window: {half:?}");
    }
    assert!(
        !dir.exists() || std::fs::read_dir(&dir).into_iter().flatten().count() == 0,
        "and none of the three left a statement behind",
    );
}

/// Inside a pane the verbs are accepted, and what each one said is left whole for the window to read —
/// in the order it was said, with the pane it belongs to on every statement.
#[test]
fn inside_a_pane_each_statement_is_left_whole_for_the_window() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-drop");
    let pane = in_a_pane(&dir);

    for (args, _) in [
        (vec!["talk", "name", "the top fix"], ()),
        (vec!["talk", "note", "reading the migration"], ()),
        (vec!["talk", "waiting", "a decision is needed"], ()),
        (vec!["talk", "finished", "it landed"], ()),
    ] {
        let (stdout, code) = cli.run_env(&pane_env(&pane), &args);
        assert_eq!(code, 0, "{args:?} is accepted inside a pane: {stdout}");
    }

    let said = statements(&dir);
    let verbs: Vec<&str> = said.iter().map(|s| s["verb"].as_str().unwrap_or_default()).collect();
    assert_eq!(
        verbs,
        vec!["name", "note", "waiting", "finished"],
        "the window reads them in the order they were said",
    );
    assert!(
        said.iter().all(|s| s["session"] == "pane-1"),
        "every statement says which pane it came from: {said:?}",
    );
    assert!(
        said.iter().all(|s| s["text"].is_string()),
        "and what was said in it, which is the whole of a statement's own body: {said:?}",
    );
}

/// The reason for a person's turn is bounded, and a longer one is turned away rather than cut. The row
/// it goes on holds three things, so a reason that overran would push the other two into ellipses —
/// and cutting it here would lose the same words one step later, with the agent believing the whole of
/// it had been read (`AMB-T-3673`).
#[test]
fn a_reason_too_long_for_the_label_is_refused_at_the_door_and_leaves_nothing_behind() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-long-reason");
    let pane = in_a_pane(&dir);
    let limit = amenbo_core::session::WAITING_LIMIT;
    // Japanese, where the bound is half the characters it is columns: this is one past it, and the
    // refusal has to say so in columns or the two numbers in it do not compare.
    let past = "あ".repeat(limit / 2 + 1);

    let (stderr, code) = cli.run_env_err(&pane_env(&pane), &["talk", "waiting", &past]);
    assert_eq!(code, 1, "a reason past the bound exits non-zero: {stderr}");
    assert!(
        stderr.contains(&(limit + 2).to_string()) && stderr.contains(&limit.to_string()),
        "and says how much room it took against how much it may take: {stderr}",
    );
    assert!(
        stderr.contains("Nothing was recorded"),
        "and says outright that nothing happened: {stderr}",
    );

    let (stderr, code) =
        cli.run_env_err(&pane_env(&pane), &["talk", "waiting", &past, "--json"]);
    assert_eq!(code, 1, "the machine face refuses it too: {stderr}");
    assert!(
        stderr.contains("talk_reason_too_long"),
        "in a code a caller can branch on: {stderr}",
    );

    assert!(
        !dir.exists() || statements(&dir).is_empty(),
        "and neither refusal left a statement for the window",
    );

    // The bound itself is within it, and the other verbs are not held to it: what they say does not
    // share the row three ways.
    let (stdout, code) =
        cli.run_env(&pane_env(&pane), &["talk", "waiting", &"あ".repeat(limit / 2)]);
    assert_eq!(code, 0, "a reason of exactly the bound is accepted: {stdout}");
    let (stdout, code) = cli.run_env(&pane_env(&pane), &["talk", "note", &past]);
    assert_eq!(code, 0, "and a note of the same length is nobody's business but the row's: {stdout}");
    assert_eq!(statements(&dir).len(), 2, "the two that were accepted are the two that were left");
}

/// The layer needs no pointer, no project and no facet: it is run in whatever checkout an agent was put
/// to work in, and what it moves is the pane rather than the store. A folder Amenbo was never bound to
/// is exactly where this has to keep working.
#[test]
fn the_layer_answers_in_a_folder_amenbo_was_never_bound_to() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-unbound");
    let pane = in_a_pane(&dir);

    let (stdout, code) = cli.run_env(&pane_env(&pane), &["talk", "waiting", "a decision is needed"]);
    assert_eq!(code, 0, "no pointer is needed to say what this terminal is doing: {stdout}");
    assert_eq!(statements(&dir).len(), 1, "and the statement was left for the window");
}

/// `talk --json` is the layer's canon, and it is served inside the window alone — the one place a
/// reader can act on it. Nothing outside is taught a vocabulary it cannot run.
#[test]
fn the_canon_is_served_inside_the_window_and_names_what_is_owed() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-canon");
    let pane = in_a_pane(&dir);

    let (stdout, code) = cli.run_env(&pane_env(&pane), &["talk", "--json"]);
    assert_eq!(code, 0, "the canon is served inside the window: {stdout}");
    let spec: serde_json::Value = serde_json::from_str(&stdout).expect("the canon is JSON");
    let owed = spec["owed"].as_array().expect("what is owed is a list").len();
    assert_eq!(owed, 3, "three statements are owed — name, waiting and finished — and no more: {spec}");
    assert!(
        statements(&dir).is_empty(),
        "reading the canon says nothing about the session, so nothing is left for the window",
    );
}

/// The layer is absent from `agent --json`, which is read in every terminal Amenbo runs in. Teaching a
/// vocabulary there that almost nowhere can run would invite the silent failure the layer exists to
/// prevent (`AMB-D-749`).
#[test]
fn the_agent_entry_point_does_not_teach_a_vocabulary_most_readers_cannot_run() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let spec = cli.json(&["agent", "--json"]);

    let indexed: Vec<String> = spec["commands"]
        .as_array()
        .expect("the entry point indexes its commands")
        .iter()
        .map(|c| c["command"].as_str().unwrap_or_default().to_string())
        .collect();
    let surface: Vec<&String> = indexed.iter().filter(|n| n.starts_with("talk")).collect();
    assert!(surface.is_empty(), "the surface layer is indexed where it cannot be run: {surface:?}");
}

/// A bare word under `talk` is not something to say — it is refused, and nothing is left for the window.
///
/// The risk the name carries is that `talk <text>` reads as a mouth that talks to the agent, and an AI
/// that believed it had one would send a person's answer into a drop box nobody speaks from
/// (`AMB-D-757`). So the layer answers to its four verbs and to nothing else: a word that is not one of
/// them fails at the door, where the mistake is still visible.
#[test]
fn a_bare_word_under_talk_is_refused_rather_than_taken_as_something_to_say() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-bare-word");
    let pane = in_a_pane(&dir);

    let (stderr, code) = cli.run_env_err(&pane_env(&pane), &["talk", "carry on then"]);
    assert_ne!(code, 0, "a bare word is not a verb of the layer: {stderr}");
    assert!(
        !dir.exists() || statements(&dir).is_empty(),
        "and nothing was left for the window to read",
    );
}

/// The other thing a pane's environment reaches, and it is not the surface layer: a status move made
/// inside a pane is recorded under that pane in the **volatile area** beside the store, which is what
/// puts a task on the pane's label (`AMB-D-758`).
///
/// Driven as a process for the same reason every test here is — a session id arrives on the
/// environment and nowhere else — and read off disk, because the one reader is the window and there is
/// no command that answers for it.
///
/// **The two halves are one claim.** A move made at somebody's own terminal has no session to be
/// recorded under, so nothing is written and nothing may be guessed; a move made in a pane is written
/// under it, so the label reads rather than infers.
#[test]
fn a_status_move_made_inside_a_pane_is_recorded_under_it_and_one_made_outside_is_not() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let area = cli.home.join(amenbo_core::session_work::DIR_NAME);

    let mut ids = Vec::new();
    for title in ["first", "second"] {
        let t = id_str(&cli.json(&["task", "add", "--title", title, "--actor", "ai", "--json"])["task"]["id"]);
        cli.finish_creating(&t);
        ids.push(t);
    }

    // One task reserved in a pane, and one at a terminal Amenbo cannot see.
    let (stdout, code) = cli.run_env(
        &[("AMENBO_SESSION", "pane-a")],
        &["task", "status", &ids[0], "in_progress", "--actor", "ai"],
    );
    assert_eq!(code, 0, "reserving inside a pane: {stdout}");
    cli.run(&["task", "status", &ids[1], "in_progress", "--actor", "ai"]);

    let work = amenbo_core::session_work::work(&area, "pane-a");
    assert_eq!(
        work.holding,
        vec![ids[0].parse::<i64>().unwrap()],
        "the pane holds what it reserved, and nothing it did not: {work:?}",
    );

    // Ended in the same pane: off its hands, and still its own doing.
    cli.run_env(&[("AMENBO_SESSION", "pane-a")], &["task", "done", &ids[0], "--actor", "ai"]);
    let work = amenbo_core::session_work::work(&area, "pane-a");
    assert!(work.holding.is_empty(), "nothing is left on its hands: {work:?}");
    assert_eq!(work.finished, vec![ids[0].parse::<i64>().unwrap()]);

    // And the area knows of no other pane, because the second reservation named none.
    let files: Vec<String> = std::fs::read_dir(&area)
        .expect("the area exists once something has been recorded in it")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, vec!["pane-a.jsonl".to_string()], "a move from outside a pane writes nothing: {files:?}");
}

/// The one statement nobody speaks: `amenbo agent` leaves it to say it was run here (`AMB-D-805`).
///
/// **Whether the first word reached the AI in a pane is settled by this fact rather than by reading the
/// screen it was typed into.** The screen is the provider's and changes with their releases; this
/// command is Amenbo's own. So it is left on every route through `agent` — the entry index and the
/// drill-down into one command alike — because each of them is the AI having reached the canon.
#[test]
fn running_the_canon_inside_a_pane_leaves_the_mark_that_it_was_read() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let dir = amenbo_scratch::scratch("talk-briefed");
    let pane = in_a_pane(&dir);

    for args in [vec!["agent", "--json"], vec!["agent", "--command", "task add"]] {
        let (stdout, code) = cli.run_env(&pane_env(&pane), &args);
        assert_eq!(code, 0, "{args:?} answers as it always did: {stdout}");
    }

    let said = statements(&dir);
    let verbs: Vec<&str> = said.iter().map(|s| s["verb"].as_str().unwrap_or_default()).collect();
    assert_eq!(verbs, vec!["briefed", "briefed"], "one mark per run, on either route: {said:?}");
    assert!(
        said.iter().all(|s| s["session"] == "pane-1" && s["text"].is_null()),
        "each says which pane it was run in, and carries no line: {said:?}",
    );
}

/// Outside a pane there is nobody to tell, and `agent` is a read of this build either way. It answers
/// exactly as it does inside one, and leaves nothing anywhere.
#[test]
fn running_the_canon_outside_a_pane_leaves_nothing_and_still_answers() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let dir = amenbo_scratch::scratch("talk-briefed-outside");

    let (stdout, code) = cli.run(&["agent", "--json"]);
    assert_eq!(code, 0, "the canon is answered at a plain terminal: {stdout}");

    // Half an environment is no pane either: a session with nowhere to leave the mark names a box
    // nothing is watching, and writing there would read as a mark that arrived.
    let (_, code) = cli.run_env(&[("AMENBO_SESSION", "pane-1")], &["agent", "--json"]);
    assert_eq!(code, 0, "and half a window changes nothing about the answer");

    assert!(
        !dir.exists() || std::fs::read_dir(&dir).into_iter().flatten().count() == 0,
        "no mark was left behind",
    );
}

/// A drop box that cannot be written to is a mark that does not arrive, which is not a reason to fail a
/// read: `agent` answers about this build and touches no store, and leaving the mark must not be the
/// thing that changes that.
#[test]
fn a_mark_that_cannot_be_left_does_not_fail_the_read() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // A file where the drop box would be: `create_dir_all` cannot make a directory over it.
    let blocked = amenbo_scratch::scratch("talk-briefed-blocked").join("not-a-directory");
    std::fs::write(&blocked, "in the way").expect("the obstruction is written");
    let path = blocked.to_string_lossy().into_owned();

    let (stdout, code) = cli.run_env(&pane_env(&path), &["agent", "--json"]);
    assert_eq!(code, 0, "the canon is still answered: {stdout}");
    assert!(stdout.contains("agentCycle"), "and it is the whole answer: {stdout}");
}
