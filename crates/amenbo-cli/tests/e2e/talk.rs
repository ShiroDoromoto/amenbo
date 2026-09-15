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

/// The store this harness's runs write to, opened here so a table no command reads back yet can still
/// be asserted on. `task show` and `decision show` grow that reading next (`AMB-D-897`).
fn store_of(cli: &Cli) -> amenbo_core::Store {
    amenbo_core::Store::open_at(amenbo_core::config::Paths::at(cli.home.clone())).expect("the store")
}

/// Name `pane` in the arrangement this device keeps, the way the window does as it draws one.
fn name_the_pane(cli: &Cli, pane: &str, name: &str) {
    let store = store_of(cli);
    store
        .save_layout(&amenbo_core::frames::SavedLayout {
            project: None,
            splits: Default::default(),
            panes: vec![amenbo_core::frames::SavedPane {
                id: pane.to_string(),
                project: 1,
                folder: None,
                agent: None,
                name: Some(amenbo_core::frames::FrameName {
                    name: name.to_string(),
                    by: amenbo_core::frames::NamedBy::Session,
                }),
                resume: None,
                model: None,
                compose_open: None,
            }],
        })
        .expect("the arrangement");
}

/// The two variables a pane's terminal carries, as `run_env` takes them.
fn made_in_env<'a>(pane: &'a str, resume: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![("AMENBO_PANE", pane), ("AMENBO_PANE_RESUME", resume)]
}

/// A task and a decision filed from a pane remember which one (`AMB-D-897`). Driven as a process
/// because the whole of it turns on what the run was launched with: the window sets the pane's id and
/// the way back into its conversation on the terminal, and an agent types `amenbo` several levels
/// deep inside that.
#[test]
fn a_record_filed_in_a_pane_remembers_the_session_it_was_made_in() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project();
    name_the_pane(&cli, "pane-a", "移行を書いている窓");

    let env = made_in_env("pane-a", "0f9c");
    let t = cli.json_env(&env, &["task", "add", "--title", "ペインのついたタスク", "--project", &pid, "--json"]);
    let tid: i64 = id_str(&t["task"]["id"]).parse().unwrap();
    let d = cli.json_env(&env, &["decision", "add", "--title", "ペインのついた決定", "--body", "結論", "--project", &pid, "--json"]);
    let did: i64 = id_str(&d["decision"]["id"]).parse().unwrap();

    let store = store_of(&cli);
    let made_in = store.task_made_in(tid).unwrap().expect("the task's pane");
    assert_eq!(made_in.pane, "pane-a");
    assert_eq!(made_in.pane_resume.as_deref(), Some("0f9c"), "the way back came off the environment");
    assert_eq!(
        made_in.pane_name.as_deref(),
        Some("移行を書いている窓"),
        "and the name off the pane's own row, which is the one place it is written",
    );
    let made_in = store.decision_made_in(did).unwrap().expect("the decision's pane");
    assert_eq!(made_in.pane, "pane-a");
    assert_eq!(made_in.pane_name.as_deref(), Some("移行を書いている窓"));
}

/// Outside a pane there is nothing to name, and a row naming a session nobody can reach is worse than
/// none: what is filed at a plain terminal carries no row at all (`AMB-D-897`). A pane whose row the
/// arrangement does not hold loses the name and keeps the rest — the id and the way back are what take
/// a reader back to the session.
#[test]
fn a_record_filed_outside_a_pane_carries_no_session_at_all() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project();

    let t = cli.json(&["task", "add", "--title", "窓の外で立てたタスク", "--project", &pid, "--json"]);
    let outside: i64 = id_str(&t["task"]["id"]).parse().unwrap();
    // A variable inherited set to nothing is not a pane either.
    let t = cli.json_env(&made_in_env("", ""), &["task", "add", "--title", "空の変数", "--project", &pid, "--json"]);
    let blank: i64 = id_str(&t["task"]["id"]).parse().unwrap();
    // A pane the arrangement has no row for: named it cannot be, reached it still can.
    let t = cli.json_env(&made_in_env("pane-z", "0f9c"), &["task", "add", "--title", "行の無いペイン", "--project", &pid, "--json"]);
    let unnamed: i64 = id_str(&t["task"]["id"]).parse().unwrap();

    let store = store_of(&cli);
    assert!(store.task_made_in(outside).unwrap().is_none(), "a plain terminal is no pane");
    assert!(store.task_made_in(blank).unwrap().is_none(), "and neither is a blank one");
    let made_in = store.task_made_in(unnamed).unwrap().expect("the pane is still named by its id");
    assert_eq!(made_in.pane, "pane-z");
    assert_eq!(made_in.pane_name, None, "there is no row to read a name off");
    assert_eq!(made_in.pane_resume.as_deref(), Some("0f9c"));
}

/// The other statement nobody speaks: a create leaves a note saying what it filed, so the band under
/// the pane can count this session's own work while it is still running (`AMB-D-897`).
///
/// **It is the command running that is counted**, which is the footing the briefed mark stands on and
/// the one `AMB-D-862` left the row on: an AI's own account of what it had done was taken off this
/// screen, and a count made of the same account would put it back.
#[test]
fn a_create_leaves_the_pane_a_note_of_what_it_filed() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project();
    let dir = amenbo_scratch::scratch("talk-made");
    let pane = in_a_pane(&dir);

    let t = cli.json_env(&pane_env(&pane), &["task", "add", "--title", "帯に出す件数", "--project", &pid, "--json"]);
    let task = id_str(&t["task"]["id"]);
    let d = cli.json_env(&pane_env(&pane), &["decision", "add", "--title", "帯に出す決定", "--body", "結論", "--project", &pid, "--json"]);
    let decision = id_str(&d["decision"]["id"]);
    // A decision raised out of a comment was filed from this pane as much as one typed outright.
    let c = cli.json_env(&pane_env(&pane), &["comment", "add", &task, "--text", "これは決定だ", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    let p = cli.json_env(&pane_env(&pane), &["decision", "promote", &cid, "--title", "昇格した決定", "--json"]);
    let promoted = id_str(&p["decision"]["id"]);

    let said = statements(&dir);
    let made: Vec<&serde_json::Value> = said.iter().filter(|s| s["verb"] == "made").collect();
    assert_eq!(
        made.iter()
            .map(|s| format!("{}:{}", s["kind"].as_str().unwrap_or_default(), s["id"]))
            .collect::<Vec<_>>(),
        vec![format!("task:{task}"), format!("decision:{decision}"), format!("decision:{promoted}")],
        "each create says which space it filed in and what number it got: {made:?}",
    );
    assert!(
        made.iter().all(|s| s["session"] == "pane-1" && s["text"].is_null()),
        "each says which pane it was typed in, and carries no line: {made:?}",
    );
}

/// Outside a pane there is nobody to tell, and a create is a create either way: `task add` answers as
/// it always did and leaves nothing anywhere. A comment is not a create, so it leaves nothing even
/// inside one — what the band counts is the records a session filed.
#[test]
fn a_create_outside_a_pane_leaves_nothing_and_a_comment_is_not_one() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project();
    let dir = amenbo_scratch::scratch("talk-made-outside");
    let pane = in_a_pane(&dir);

    let t = cli.json(&["task", "add", "--title", "窓の外で立てたタスク", "--project", &pid, "--json"]);
    assert!(
        !dir.exists() || statements(&dir).is_empty(),
        "a plain terminal is no pane, so there is no band to tell",
    );

    let id = id_str(&t["task"]["id"]);
    cli.json_env(&pane_env(&pane), &["comment", "add", &id, "--text", "話の続き", "--json"]);
    assert!(
        statements(&dir).iter().all(|s| s["verb"] != "made"),
        "and a comment files no record: {:?}",
        statements(&dir),
    );
}

/// `task show` and `decision show` say which session made the record — the reading that lets the
/// question be answered from a terminal, without opening the window (`AMB-D-897`).
///
/// The text line folds away where no pane made it, the way `folder:` does; `--json` carries the key
/// either way, so a reader parsing it never has to tell "made outside a pane" from "this build does
/// not say".
#[test]
fn the_pages_say_which_session_made_the_record() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project();
    name_the_pane(&cli, "pane-a", "移行を書いている窓");
    let env = made_in_env("pane-a", "0f9c");

    let tid = id_str(&cli.json_env(&env, &["task", "add", "--title", "ペインのついたタスク", "--project", &pid, "--json"])["task"]["id"]);
    let did = id_str(&cli.json_env(&env, &["decision", "add", "--title", "ペインのついた決定", "--body", "結論", "--project", &pid, "--json"])["decision"]["id"]);

    let (stdout, _) = cli.run(&["task", "show", &tid]);
    assert!(
        stdout.contains("made in: 移行を書いている窓 (pane-a)"),
        "the name leads and the id follows it: {stdout}",
    );
    let (stdout, _) = cli.run(&["decision", "show", &did]);
    assert!(stdout.contains("made in: 移行を書いている窓 (pane-a)"), "and the decision's page says it too: {stdout}");

    let made_in = &cli.json(&["task", "show", &tid, "--json"])["made_in"];
    assert_eq!(made_in["pane"], "pane-a");
    assert_eq!(made_in["pane_name"], "移行を書いている窓");
    assert_eq!(made_in["pane_resume"], "0f9c", "the handle is carried as it was written");
    assert_eq!(cli.json(&["decision", "show", &did, "--json"])["made_in"]["pane"], "pane-a");

    // Filed at a plain terminal: no line on the page, and a key that is there and null.
    let outside = id_str(&cli.json(&["task", "add", "--title", "窓の外で立てたタスク", "--project", &pid, "--json"])["task"]["id"]);
    let (stdout, _) = cli.run(&["task", "show", &outside]);
    assert!(!stdout.contains("made in:"), "no session made it, so the line is not there: {stdout}");
    let shown = cli.json(&["task", "show", &outside, "--json"]);
    assert!(shown.get("made_in").is_some(), "the key is written whatever the answer");
    assert!(shown["made_in"].is_null(), "and it is null: {shown}");
}
