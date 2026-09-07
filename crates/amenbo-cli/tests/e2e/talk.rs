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
        let (_, code) = cli.run_env(&half, &["talk", "name", "half"]);
        assert_eq!(code, 1, "half a window is not a window: {half:?}");
    }
    assert!(
        !dir.exists() || std::fs::read_dir(&dir).into_iter().flatten().count() == 0,
        "and none of the three left a statement behind",
    );
}

/// Inside a pane the verb is accepted, and what it said is left whole for the window to read — in the
/// order it was said, with the pane it belongs to on every statement.
#[test]
fn inside_a_pane_each_statement_is_left_whole_for_the_window() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-drop");
    let pane = in_a_pane(&dir);

    for args in [
        vec!["talk", "name", "the top fix"],
        vec!["talk", "name", "the second fix"],
    ] {
        let (stdout, code) = cli.run_env(&pane_env(&pane), &args);
        assert_eq!(code, 0, "{args:?} is accepted inside a pane: {stdout}");
    }

    let said = statements(&dir);
    let texts: Vec<&str> = said.iter().map(|s| s["text"].as_str().unwrap_or_default()).collect();
    assert_eq!(
        texts,
        vec!["the top fix", "the second fix"],
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

/// The layer needs no pointer, no project and no facet: it is run in whatever checkout an agent was put
/// to work in, and what it moves is the pane rather than the store. A folder Amenbo was never bound to
/// is exactly where this has to keep working.
#[test]
fn the_layer_answers_in_a_folder_amenbo_was_never_bound_to() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-unbound");
    let pane = in_a_pane(&dir);

    let (stdout, code) = cli.run_env(&pane_env(&pane), &["talk", "name", "the top fix"]);
    assert_eq!(code, 0, "no pointer is needed to say what this terminal is called: {stdout}");
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
    assert_eq!(owed, 1, "one statement is owed — the name — and no more: {spec}");
    assert!(
        spec["offered"].as_array().is_some_and(|o| o.is_empty()),
        "and nothing here is the speaker's to leave out: {spec}",
    );
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
/// (`AMB-D-757`). So the layer answers to its two verbs and to nothing else: a word that is not one of
/// them fails at the door, where the mistake is still visible. `talk note` and `talk finished` are two
/// such words, and an agent carrying an older habit types them — so they are walked here beside the
/// bare one, to pin that the refusal is the same.
#[test]
fn a_bare_word_under_talk_is_refused_rather_than_taken_as_something_to_say() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("talk-bare-word");
    let pane = in_a_pane(&dir);

    for stray in [
        vec!["talk", "carry on then"],
        vec!["talk", "note", "reading the migration"],
        vec!["talk", "finished", "it landed"],
    ] {
        let (stderr, code) = cli.run_env_err(&pane_env(&pane), &stray);
        assert_ne!(code, 0, "{stray:?} is not a verb of the layer: {stderr}");
    }
    assert!(
        !dir.exists() || statements(&dir).is_empty(),
        "and nothing was left for the window to read",
    );
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
