//! The shared timeline: the system events that reach it, the comments that ride alongside them, and
//! the cursor a reader pages it with.

mod harness;

use harness::*;

#[test]
fn activity_records_system_events_and_comments() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // An AI cannot pass --project; the binding fills in where the task goes.
    let t = id_str(&cli.json(&["task", "add", "--title", "do it", "--actor", "ai", "--json"])["task"]["id"]);
    cli.finish_creating(&t);
    // reserve via status (todo→in_progress).
    cli.run(&["task", "status", &t, "in_progress", "--actor", "ai"]);
    cli.run(&["comment", "add", &t, "--actor", "ai", "--text", "starting"]);
    cli.run(&["task", "status", &t, "done", "--actor", "ai"]);

    let act = cli.json(&["activity", "--task", &t, "--json"]);
    let kinds: Vec<String> = act["items"].as_array().unwrap().iter().map(|i| {
        if i["type"] == "comment" { "comment".to_string() }
        else { i["event"]["kind"].as_str().unwrap().to_string() }
    }).collect();
    // All four are recorded, all on the ai facet (created + 2× status_changed + comment).
    assert_eq!(act["count"], 4);
    assert!(kinds.contains(&"task.created".to_string()));
    assert!(kinds.contains(&"comment".to_string()));
    assert!(kinds.contains(&"task.status_changed".to_string()));
    assert!(act["items"].as_array().unwrap().iter().all(|i| i["author"]["kind"] == "ai"));

    // --kind system keeps the system events only, dropping comments.
    let sys = cli.json(&["activity", "--task", &t, "--kind", "system", "--json"]);
    assert_eq!(sys["count"], 3);
    assert!(sys["items"].as_array().unwrap().iter().all(|i| i["type"] == "system"));

    // --by human shows none of the ai events.
    let human = cli.json(&["activity", "--by", "human", "--json"]);
    assert_eq!(human["count"], 0);

    // Paging: --limit/--offset cut a newest-first window for walking back through history.
    let all = cli.json(&["activity", "--task", &t, "--json"]);
    let ids: Vec<String> = all["items"].as_array().unwrap().iter()
        .map(|i| id_str(&i["id"])).collect();
    let p0 = cli.json(&["activity", "--task", &t, "--limit", "2", "--json"]);
    assert_eq!(p0["count"], 2);
    let p1 = cli.json(&["activity", "--task", &t, "--limit", "2", "--offset", "2", "--json"]);
    assert_eq!(p1["count"], 2);
    // offset=2 continues with items 3 and 4 — no overlap, still newest-first.
    assert_eq!(id_str(&p1["items"][0]["id"]), ids[2]);
    assert_eq!(id_str(&p1["items"][1]["id"]), ids[3]);
    // An offset past the end is empty.
    let p_end = cli.json(&["activity", "--task", &t, "--offset", "99", "--json"]);
    assert_eq!(p_end["count"], 0);
}

/// The timeline names the facet and stops there. Two AI sessions are one facet, so every row an AI
/// wrote reads the same on it — and the ledger says nothing else about who wrote one, in particular
/// nothing about the pane it was written in. That belongs to a place that is emptied when the window
/// closes (`AMB-D-758`, `crates/amenbo-cli/tests/e2e/talk.rs`), because a session id means nothing
/// once its window has gone, and a permanent row is exactly where it must not be kept.
#[test]
fn the_timeline_names_the_facet_and_says_nothing_about_the_pane_a_write_came_from() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let t = id_str(&cli.json(&["task", "add", "--title", "first", "--actor", "ai", "--json"])["task"]["id"]);
    cli.finish_creating(&t);
    let (stdout, code) = cli.run_env(
        &[("AMENBO_SESSION", "pane-a")],
        &["task", "status", &t, "in_progress", "--actor", "ai"],
    );
    assert_eq!(code, 0, "reserving inside a pane: {stdout}");
    cli.run(&["comment", "add", &t, "--actor", "ai", "--text", "on it"]);
    cli.run(&["task", "status", &t, "blocked", "--actor", "ai"]);

    let rows = cli.json(&["activity", "--task", &t, "--json"]);
    let rows = rows["items"].as_array().unwrap();
    assert!(
        rows.iter().all(|i| i["author"]["kind"] == "ai"),
        "the facet says the same of every one of them: {rows:?}",
    );
    assert!(
        rows.iter().all(|i| i["author"].get("session").is_none()),
        "and no row names a pane, the one made inside one included: {rows:?}",
    );
}

/// The ledger self-compacts at 8 MiB, so the very lines that carry a vanished subject's **name**
/// (task.created / task.deleted) can age out — as can a name that falls outside the lookback budget. Core
/// then returns an empty title, and piping that straight to a human leaves nothing after the "—", so the
/// human line has to say the subject is gone. `--json` is the machine's face and stays empty.
#[test]
fn a_subject_whose_name_the_ledger_no_longer_carries_reads_as_deleted() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();

    // Reproduce a ledger whose compaction dropped the naming lines: only a nameless line
    // (task.status_changed) is left, and its subject exists nowhere else. Append the raw line by hand — a
    // real deletion appends a `task.deleted` line carrying the name, so the name stays recoverable and it
    // cannot stage this break; only compaction ageing the naming lines out gets there, and that is not
    // something a test can ask for.
    let ledger = cli.home.join("activity.jsonl");
    let line = serde_json::json!({
        "v": 2,
        "id": 999_999,
        "at": "2099-01-01T00:00:00Z",
        "actor": "ai",
        "project": pid.parse::<i64>().unwrap(),
        "task": 999_999,
        "decision": null,
        "event": {"kind": "task.status_changed", "new": "done"},
    });
    let mut body = std::fs::read_to_string(&ledger).unwrap_or_default();
    body.push_str(&format!("{line}\n"));
    std::fs::write(&ledger, body).unwrap();

    let (out, code) = cli.run(&["activity"]);
    assert_eq!(code, 0);
    assert!(out.contains("task.status_changed"), "a row with no name is still shown: {out}");
    assert!(out.contains("— (deleted)"), "a subject whose name cannot be recovered says (deleted): {out}");

    // JSON stays raw (empty title): the machine gets the fact, not a paraphrase.
    let js = cli.json(&["activity", "--json"]);
    assert_eq!(js["items"][0]["target"]["title"], "");
}

/// Some lines still carry a name whose subject is gone — the **past** lines of a deleted task. Printing
/// the bare name makes it look alive, and the reader wastes a `task show` on it. Even the one-line human
/// form says an unreachable subject is unreachable; `--json` paraphrases nothing and passes `live` raw.
#[test]
fn a_past_line_of_a_deleted_subject_says_the_subject_is_gone() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let add = |title: &str| -> String {
        let id = id_str(&cli.json(&["task", "add", "--title", title, "--project", &pid, "--json"])["task"]["id"]);
        cli.finish_creating(&id);
        id
    };

    let t = add("消されるタスク");
    // Lay down a nameless past line (task.status_changed), then delete the task. The ledger keeps that line
    // and the task.deleted line that carries the name — so core can name a subject that is no longer there.
    cli.run(&["task", "status", &t, "in_progress"]);
    cli.run(&["task", "delete", &t, "--yes"]);

    let (out, code) = cli.run(&["activity"]);
    assert_eq!(code, 0);
    assert!(out.contains("task.status_changed"), "past rows of a deleted subject remain in the ledger: {out}");
    assert!(
        !out.contains("— 消されるタスク\n"),
        "it does not print just the name to look like a live target: {out}"
    );
    assert!(out.contains("— 消されるタスク (deleted)"), "an untraceable target says so: {out}");

    // A live subject gets no extra mark: the mark means "gone", it is not decoration.
    let live = add("生きているタスク");
    cli.run(&["task", "status", &live, "in_progress"]);
    let (out, _) = cli.run(&["activity"]);
    assert!(out.contains("— 生きているタスク\n"), "a live target keeps its plain name: {out}");

    // JSON does not paraphrase — the machine reads `live`.
    let js = cli.json(&["activity", "--json"]);
    let items = js["items"].as_array().unwrap();
    let gone = items.iter().find(|i| i["target"]["title"] == "消されるタスク").unwrap();
    assert_eq!(gone["target"]["live"], false);
    let alive = items.iter().find(|i| i["target"]["title"] == "生きているタスク").unwrap();
    assert_eq!(alive["target"]["live"], true);
}

/// A decision record's comments are on the same timeline as a task's — `activity` is one stream, so a
/// discussion held on a decision is not invisible there. `--kind comment` takes both tables; the two
/// filters that ask about a task a decision comment does not hang on (`--task`, `--for`) leave it out.
#[test]
fn activity_carries_a_decision_records_comments_too() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let task = id_str(
        &cli.json(&["task", "add", "--title", "ai task", "--to", "tester", "--ai", "--actor", "ai", "--json"])
            ["task"]["id"],
    );
    let dec = id_str(
        &cli.json(&["decision", "add", "--title", "決定ひとつ", "--body", "本文", "--actor", "ai", "--json"])
            ["decision"]["id"],
    );
    cli.run(&["comment", "add", &task, "--actor", "ai", "--text", "task said"]);
    cli.run(&["decision", "comment", "add", &dec, "--actor", "ai", "--text", "decision said"]);

    let comments = cli.json(&["activity", "--kind", "comment", "--json"]);
    let items = comments["items"].as_array().unwrap().clone();
    let on_decision = items
        .iter()
        .find(|i| i["target"]["type"] == "decision")
        .expect("the decision's comment is on the timeline");
    assert_eq!(on_decision["text"], "decision said");
    assert_eq!(on_decision["target"]["id"].as_i64().unwrap().to_string(), dec);
    assert_eq!(on_decision["target"]["title"], "決定ひとつ", "named by the decision it hangs on");
    assert_eq!(on_decision["target"]["live"], true, "a comment cannot outlive what it hangs on");
    assert!(items.iter().any(|i| i["target"]["type"] == "task"), "and the task's comment is still there");

    for narrowed in [
        cli.json(&["activity", "--task", &task, "--json"]),
        cli.json(&["activity", "--for", "ai", "--json"]),
    ] {
        assert!(
            narrowed["items"].as_array().unwrap().iter().all(|i| i["target"]["type"] != "decision"),
            "a decision comment has no task to be filtered by, so it narrows away"
        );
    }
}

/// Built for agents: the opaque `--since <cursor>` returns only what is strictly newer than the last read,
/// oldest-first, and `--for me` narrows to events on tasks assigned to my facet.
#[test]
fn activity_incremental_cursor_and_for_me_scope() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    // One task the ai picks up, one assigned to the human.
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let mine = id_str(&cli.json(&["task", "add", "--title", "ai task", "--to", "tester", "--ai", "--actor", "ai", "--json"])["task"]["id"]);
    let theirs = id_str(&cli.json(&["task", "add", "--title", "human task", "--project", &pid, "--json"])["task"]["id"]);
    cli.finish_creating(&mine);
    cli.finish_creating(&theirs);
    cli.run(&["task", "status", &theirs, "in_progress"]); // the human facet reserves it via status → an event on the human side

    // Read once to get the cursor to resume from; history responses carry one too.
    let base = cli.json(&["activity", "--json"]);
    let cursor = base["cursor"].as_str().expect("history response carries an opaque cursor").to_string();
    assert!(cursor.starts_with("cur2_"), "opaque cursor prefix");

    // Nothing is strictly newer yet, and the position holds.
    let empty = cli.json(&["activity", "--since", &cursor, "--json"]);
    assert_eq!(empty["count"], 0, "nothing strictly newer yet");
    assert_eq!(empty["has_more"], false);

    // Make some movement on my own task after that point (status → comment → status).
    cli.run(&["task", "status", &mine, "in_progress", "--actor", "ai"]);
    cli.run(&["comment", "add", &mine, "--actor", "ai", "--text", "picked up"]);
    cli.run(&["task", "status", &mine, "done", "--actor", "ai"]);

    // Incremental: only what is newer than the cursor, oldest-first (status_changed → comment → status_changed).
    let inc = cli.json(&["activity", "--since", &cursor, "--json"]);
    assert!(inc["count"].as_u64().unwrap() >= 3, "the three new events since the cursor");
    let ats: Vec<String> = inc["items"].as_array().unwrap().iter().map(|i| i["at"].as_str().unwrap().to_string()).collect();
    let mut sorted = ats.clone();
    sorted.sort();
    assert_eq!(ats, sorted, "oldest-first (time-forward) for incremental consumption");
    // The advanced cursor differs from the old one and can be handed straight back next time.
    assert_ne!(inc["cursor"].as_str().unwrap(), cursor);

    // --for me (acting as the ai): human-assigned tasks drop out, leaving only my own.
    let for_me = cli.json(&["activity", "--for", "me", "--actor", "ai", "--json"]);
    assert!(for_me["count"].as_u64().unwrap() >= 1);
    assert!(
        for_me["items"].as_array().unwrap().iter().all(|i| id_str(&i["target"]["id"]) == mine),
        "--for me keeps only activity on tasks assigned to my facet"
    );

    // --for human is the mirror image: only human-assigned tasks.
    let for_human = cli.json(&["activity", "--for", "human", "--json"]);
    assert!(
        for_human["items"].as_array().unwrap().iter().all(|i| id_str(&i["target"]["id"]) == theirs),
        "--for human keeps only activity on human-assigned tasks"
    );

    // A malformed cursor fails loud instead of silently falling back to a date.
    let (_stderr, code) = cli.run_err(&["activity", "--since", "cur1_@@@broken@@@", "--json"]);
    assert_eq!(code, 2, "malformed cursor is fail-loud, not silently treated as a date");
}
