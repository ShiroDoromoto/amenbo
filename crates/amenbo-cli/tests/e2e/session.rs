//! `session`, the surface layer (`AMB-D-749`): what it does inside a pane of the talk window, and what
//! it refuses everywhere else.
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
        vec!["session", "name", "the top fix"],
        vec!["session", "note", "reading the migration"],
        vec!["session", "waiting", "a decision is needed"],
        vec!["session", "finished", "it landed"],
        vec!["session", "point", "AMB-T-1", "--why", "here"],
        vec!["session"],
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
            stderr.contains("session_outside_surface"),
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
    let dir = amenbo_scratch::scratch("session-half");
    let path = dir.to_string_lossy().into_owned();
    for half in [
        vec![("AMENBO_SESSION", "pane-1")],
        vec![("AMENBO_SESSION_DIR", path.as_str())],
        vec![("AMENBO_SESSION", " "), ("AMENBO_SESSION_DIR", path.as_str())],
    ] {
        let (_, code) = cli.run_env(&half, &["session", "note", "half"]);
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
    let dir = amenbo_scratch::scratch("session-drop");
    let pane = in_a_pane(&dir);

    for (args, _) in [
        (vec!["session", "name", "the top fix"], ()),
        (vec!["session", "note", "reading the migration"], ()),
        (vec!["session", "waiting", "a decision is needed"], ()),
        (vec!["session", "point", "AMB-T-3592", "--why", "the vocabulary lands here"], ()),
        (vec!["session", "finished", "it landed"], ()),
    ] {
        let (stdout, code) = cli.run_env(&pane_env(&pane), &args);
        assert_eq!(code, 0, "{args:?} is accepted inside a pane: {stdout}");
    }

    let said = statements(&dir);
    let verbs: Vec<&str> = said.iter().map(|s| s["verb"].as_str().unwrap_or_default()).collect();
    assert_eq!(
        verbs,
        vec!["name", "note", "waiting", "point", "finished"],
        "the window reads them in the order they were said",
    );
    assert!(
        said.iter().all(|s| s["session"] == "pane-1"),
        "every statement says which pane it came from: {said:?}",
    );
    let point = said.iter().find(|s| s["verb"] == "point").expect("the point is among them");
    assert_eq!(point["target"], "AMB-T-3592");
    assert_eq!(point["why"], "the vocabulary lands here", "a point carries its reason, not just its target");
}

/// The reason for a person's turn is bounded, and a longer one is turned away rather than cut. The row
/// it goes on holds three things, so a reason that overran would push the other two into ellipses —
/// and cutting it here would lose the same words one step later, with the agent believing the whole of
/// it had been read (`AMB-T-3673`).
#[test]
fn a_reason_too_long_for_the_label_is_refused_at_the_door_and_leaves_nothing_behind() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("session-long-reason");
    let pane = in_a_pane(&dir);
    let limit = amenbo_core::session::WAITING_LIMIT;
    // Japanese, where the bound is half the characters it is columns: this is one past it, and the
    // refusal has to say so in columns or the two numbers in it do not compare.
    let past = "あ".repeat(limit / 2 + 1);

    let (stderr, code) = cli.run_env_err(&pane_env(&pane), &["session", "waiting", &past]);
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
        cli.run_env_err(&pane_env(&pane), &["session", "waiting", &past, "--json"]);
    assert_eq!(code, 1, "the machine face refuses it too: {stderr}");
    assert!(
        stderr.contains("session_reason_too_long"),
        "in a code a caller can branch on: {stderr}",
    );

    assert!(
        !dir.exists() || statements(&dir).is_empty(),
        "and neither refusal left a statement for the window",
    );

    // The bound itself is within it, and the other verbs are not held to it: what they say does not
    // share the row three ways.
    let (stdout, code) =
        cli.run_env(&pane_env(&pane), &["session", "waiting", &"あ".repeat(limit / 2)]);
    assert_eq!(code, 0, "a reason of exactly the bound is accepted: {stdout}");
    let (stdout, code) = cli.run_env(&pane_env(&pane), &["session", "note", &past]);
    assert_eq!(code, 0, "and a note of the same length is nobody's business but the row's: {stdout}");
    assert_eq!(statements(&dir).len(), 2, "the two that were accepted are the two that were left");
}

/// The layer needs no pointer, no project and no facet: it is run in whatever checkout an agent was put
/// to work in, and what it moves is the pane rather than the store. A folder Amenbo was never bound to
/// is exactly where this has to keep working.
#[test]
fn the_layer_answers_in_a_folder_amenbo_was_never_bound_to() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("session-unbound");
    let pane = in_a_pane(&dir);

    let (stdout, code) = cli.run_env(&pane_env(&pane), &["session", "waiting", "a decision is needed"]);
    assert_eq!(code, 0, "no pointer is needed to say what this terminal is doing: {stdout}");
    assert_eq!(statements(&dir).len(), 1, "and the statement was left for the window");
}

/// `session --json` is the layer's canon, and it is served inside the window alone — the one place a
/// reader can act on it. Nothing outside is taught a vocabulary it cannot run.
#[test]
fn the_canon_is_served_inside_the_window_and_names_what_is_owed() {
    let cli = Cli::new();
    let dir = amenbo_scratch::scratch("session-canon");
    let pane = in_a_pane(&dir);

    let (stdout, code) = cli.run_env(&pane_env(&pane), &["session", "--json"]);
    assert_eq!(code, 0, "the canon is served inside the window: {stdout}");
    let spec: serde_json::Value = serde_json::from_str(&stdout).expect("the canon is JSON");
    let owed = spec["owed"].as_array().expect("what is owed is a list").len();
    assert_eq!(owed, 2, "two statements are owed — waiting and finished — and no more: {spec}");
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
    let surface: Vec<&String> = indexed.iter().filter(|n| n.starts_with("session")).collect();
    assert!(surface.is_empty(), "the surface layer is indexed where it cannot be run: {surface:?}");
}
