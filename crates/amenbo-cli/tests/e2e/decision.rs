//! Decisions, end to end: the timeline one carries, settling the same one twice, promoting a
//! comment into a record of its own, and the edges a decision draws to its tasks and to its
//! neighbours.

mod harness;

use serde_json::Value;

use harness::*;

/// Decisions take `comment add`/`list`, and `accept`/`reject --reason` is thin sugar that appends one
/// reason comment — there is no dedicated field. An empty or whitespace-only reason is ignored.
#[test]
fn decision_comment_add_list_and_accept_reject_reason() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // comment add shows up in list, oldest first.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    cli.json(&["decision", "comment", "add", &did, "--text", "初回コメント", "--json"]);
    let listed = cli.json(&["decision", "comment", "list", &did, "--json"]);
    assert_eq!(listed["count"], 1);
    assert_eq!(id_str(&listed["decision"]["id"]), did);
    assert_eq!(listed["comments"][0]["text"], "初回コメント");

    // accept --reason appends one comment: the reason lands on the timeline, not in the body.
    cli.json(&["decision", "accept", &did, "--reason", "レビュー後に合意", "--json"]);
    let after_accept = cli.json(&["decision", "comment", "list", &did, "--json"]);
    assert_eq!(after_accept["count"], 2, "one reason comment is added");
    assert_eq!(after_accept["comments"][1]["text"], "レビュー後に合意");
    // The decision itself becomes accepted — the sugar does not get in the way of the transition.
    assert_eq!(cli.json(&["decision", "show", &did, "--json"])["status"], "accepted");

    // reject --reason behaves the same way.
    let d2 = cli.json(&["decision", "add", "--project", &pid, "--title", "却下される案", "--json"]);
    let did2 = id_str(&d2["decision"]["id"]);
    cli.json(&["decision", "reject", &did2, "--reason", "D-1 で代替", "--json"]);
    let rej = cli.json(&["decision", "comment", "list", &did2, "--json"]);
    assert_eq!(rej["count"], 1);
    assert_eq!(rej["comments"][0]["text"], "D-1 で代替");

    // A whitespace-only reason is ignored, leaving no empty comment behind.
    let d3 = cli.json(&["decision", "add", "--project", &pid, "--title", "理由なし", "--json"]);
    let did3 = id_str(&d3["decision"]["id"]);
    cli.json(&["decision", "accept", &did3, "--reason", "   ", "--json"]);
    assert_eq!(cli.json(&["decision", "comment", "list", &did3, "--json"])["count"], 0);
}

/// Re-accepting an already-accepted decision is an idempotent noop that **says so** instead of a bare
/// "✓" that reads as a fresh acceptance: `noop` is true, `changed` is empty, the facet that first
/// settled it is never silently overwritten (that is `reopen`'s job), and a `--reason` on the noop
/// does not pile a comment. `reject` / `supersede` are the same shape.
#[test]
fn re_settling_a_decision_is_a_reported_noop_and_does_not_overwrite_or_pile_a_reason() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // First accept settles it; the facet is recorded.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "採択の名義", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let first = cli.json(&["decision", "accept", &did, "--json"]);
    assert_eq!(first["noop"], false);
    assert_eq!(first["decision"]["decided_by"]["name"], "human");

    // Re-accepting reports a noop with nothing changed, keeps the recorded facet (re-stamping is
    // `reopen`'s route), and the `--reason` does not become a comment.
    let again = cli.json(&["decision", "accept", &did, "--reason", "名義を直したい", "--json"]);
    assert_eq!(again["noop"], true, "re-accepting is a reported noop");
    assert_eq!(again["changed"].as_array().unwrap().len(), 0, "nothing changed");
    assert_eq!(again["decision"]["decided_by"]["name"], "human", "the recorded facet is untouched");
    assert_eq!(
        cli.json(&["decision", "comment", "list", &did, "--json"])["count"], 0,
        "a reason on a noop re-accept must not pile a comment"
    );

    // reject: re-rejecting an already-rejected decision is a reported noop too.
    let dr = cli.json(&["decision", "add", "--project", &pid, "--title", "却下の冪等", "--json"]);
    let didr = id_str(&dr["decision"]["id"]);
    assert_eq!(cli.json(&["decision", "reject", &didr, "--json"])["noop"], false);
    let rej_again = cli.json(&["decision", "reject", &didr, "--reason", "二度目", "--json"]);
    assert_eq!(rej_again["noop"], true);
    assert_eq!(
        cli.json(&["decision", "comment", "list", &didr, "--json"])["count"], 0,
        "a reason on a noop re-reject must not pile a comment"
    );

    // supersede: re-superseding an already-superseded pair is a reported noop.
    let old = cli.json(&["decision", "add", "--project", &pid, "--title", "旧", "--json"]);
    let oldid = id_str(&old["decision"]["id"]);
    cli.json(&["decision", "accept", &oldid, "--json"]);
    let new = cli.json(&["decision", "add", "--project", &pid, "--title", "新", "--json"]);
    let newid = id_str(&new["decision"]["id"]);
    assert_eq!(cli.json(&["decision", "supersede", &newid, "--replaces", &oldid, "--json"])["noop"], false);
    assert_eq!(
        cli.json(&["decision", "supersede", &newid, "--replaces", &oldid, "--json"])["noop"], true,
        "re-superseding an already-superseded pair is a noop"
    );
}

/// A decision's own thread raises records too, so `decision promote` is one door for both comment kinds
/// and the ref says which table it came from. What differs is what is drawn afterwards: a task comment
/// links the new record back to its task, a decision comment draws nothing at all — a question raised out
/// of a decision's thread became its own, and an automatic edge would claim a relation nobody chose. A
/// bare `<n>` naming a row in each table is refused, the way a bare number that is both a task and a
/// decision is.
#[test]
fn a_decision_comment_promotes_into_a_record_that_stands_alone() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "昇格PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "作業", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    let post_task = |text: &str| id_str(&cli.json(&["comment", "add", &tid, "--text", text, "--json"])["comment"]["id"]);
    let post_decision =
        |text: &str| id_str(&cli.json(&["decision", "comment", "add", &did, "--text", text, "--json"])["comment"]["id"]);
    // The two tables number apart, so one key names a row in each — whichever side is behind posts until
    // they meet there. That collision is what makes a bare number ambiguous, and the spelling the way through.
    let mut tcid = post_task("タスク側");
    let mut dcid = post_decision("表示の桁は別問題だ");
    while tcid.parse::<i64>().unwrap() < dcid.parse::<i64>().unwrap() {
        tcid = post_task("タスク側");
    }
    while dcid.parse::<i64>().unwrap() < tcid.parse::<i64>().unwrap() {
        dcid = post_decision("表示の桁は別問題だ");
    }
    assert_eq!(tcid, dcid);

    let (err, code) = cli.run_err(&["decision", "promote", &dcid, "--title", "桁を決める", "--json"]);
    assert_eq!(code, 2, "a bare number naming a row in each table is refused: {err}");
    assert!(err.contains(&format!("AMB-TC-{tcid}")) && err.contains(&format!("AMB-DC-{dcid}")), "it names both: {err}");

    let (out, _, _) = cli.run_both(&["decision", "promote", &format!("AMB-DC-{dcid}"), "--title", "桁を決める"]);
    assert!(out.contains(&format!("AMB-DC-{dcid}")), "the line says which comment was raised: {out}");

    let promoted =
        cli.json(&["decision", "promote", &format!("AMB-DC-{dcid}"), "--title", "桁を決める2", "--json"])["decision"]
            .clone();
    assert_eq!(promoted["body"], "表示の桁は別問題だ", "the comment's text is the new body");
    assert_eq!(promoted["status"], "proposed", "it is a proposal, not a settled decision");
    assert_eq!(id_str(&promoted["project"]["id"]), pid, "the home comes from the comment's decision");
    for edge in ["linked_tasks", "builds_on", "supersedes", "amends"] {
        assert!(promoted[edge].as_array().unwrap().is_empty(), "the new record draws no {edge}: {promoted}");
    }

    // …and the decision it was raised out of is untouched: nothing points back at it either.
    let source = cli.json(&["decision", "show", &did, "--json"]);
    for edge in ["built_on_by", "superseded_by", "amended_by"] {
        assert!(source[edge].as_array().unwrap().is_empty(), "the source gained no {edge}: {source}");
    }

    // The task side is unchanged: its comment still promotes, and still links the decision to its task.
    let from_task =
        cli.json(&["decision", "promote", &format!("AMB-TC-{tcid}"), "--title", "作業の前提", "--json"])["decision"].clone();
    assert_eq!(from_task["body"], "タスク側");
    assert_eq!(from_task["linked_tasks"][0]["id"].to_string(), tid, "a task comment's decision is that task's premise");
}

/// A decision's page carries its timeline (`AMB-D-448`), in the shape `task show` gives its own: the count
/// is marked even at zero, the newest three are previewed on one line each, and the way to the full text is
/// named. What `accept --reason` wrote is a comment, so a page without them would leave the ruling's own
/// reasoning off the only page anyone opens to read the ruling.
#[test]
fn a_decision_page_says_how_much_was_said_on_it_and_previews_the_latest() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let did = id_str(
        &cli.json(&["decision", "add", "--project", &pid, "--title", "交点を1行にする", "--json"])
            ["decision"]["id"],
    );

    // Nothing said yet: the category is marked rather than left out (`AMB-D-75`).
    let (empty, code) = cli.run(&["decision", "show", &did]);
    assert_eq!(code, 0);
    assert!(empty.contains("comments: (none)"), "an empty timeline says so: {empty}");
    assert_eq!(
        cli.json(&["decision", "show", &did, "--json"])["comments"].as_array().map(|c| c.len()),
        Some(0),
        "the JSON carries the key whether or not anything was said",
    );

    // The reason an acceptance was given lands on the timeline, and is what the page has to carry.
    cli.json(&["decision", "accept", &did, "--reason", "この形で行く", "--json"]);
    let long = "あ".repeat(80);
    cli.json(&["decision", "comment", "add", &did, "--text", &long, "--json"]);

    let (human, code) = cli.run(&["decision", "show", &did]);
    assert_eq!(code, 0);
    assert!(human.contains("comments (2, newest first):"), "the count is every comment: {human}");
    assert!(human.contains("この形で行く"), "the acceptance's reason is on the page: {human}");
    let previewed = human
        .lines()
        .find(|l| l.contains('あ'))
        .unwrap_or_else(|| panic!("the long comment is previewed: {human}"));
    assert!(previewed.ends_with('…'), "a long comment is cut, not printed whole: {previewed}");
    // Named with this build's own command, not the production spelling — on the dev channel the
    // hardcoded one points at something that is not installed.
    assert!(
        human.contains(&format!(
            "full text: {} decision comment list AMB-D-{did}",
            amenbo_core::config::Paths::command_name()
        )),
        "the way to what the preview cut is named: {human}",
    );

    // The JSON is where nothing is cut: the whole timeline, under the name `task show` gives its own.
    let shown = cli.json(&["decision", "show", &did, "--json"]);
    let comments = shown["comments"].as_array().expect("comments array");
    assert_eq!(comments.len(), 2);
    assert!(
        comments.iter().any(|c| c["text"].as_str() == Some(long.as_str())),
        "the JSON carries the text whole: {shown}",
    );
}

/// A decision says whether the work it spawned is **still standing**: the linked tasks of `decision show`
/// carry a status beside id and title (`linked_tasks[].status` in `--json`), a finished task sinks to `[x]`
/// in the human output, and only what is moving or stuck names its status — `todo` is the default and stays quiet.
#[test]
fn a_decision_says_which_of_the_tasks_it_created_are_still_standing() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let did = id_str(
        &cli.json(&["decision", "add", "--project", &pid, "--title", "畳み込みは 1 回で束ねる", "--json"])
            ["decision"]["id"],
    );
    // Tasks under an unsettled decision cannot be started, so accept it before moving any status.
    cli.json(&["decision", "accept", &did, "--json"]);

    let a_task = |title: &str| -> String {
        let id = id_str(&cli.json(&["task", "add", "--project", &pid, "--title", title, "--json"])["task"]["id"]);
        cli.finish_creating(&id);
        id
    };
    let todo = a_task("まだ手を付けていない");
    let doing = a_task("いま進めている");
    let done = a_task("終わった");
    let blocked = a_task("外の事情で止まっている");
    for tid in [&todo, &doing, &done, &blocked] {
        cli.json(&["decision", "link", &did, tid, "--json"]);
    }
    cli.json(&["task", "status", &doing, "in_progress", "--json"]);
    cli.json(&["task", "done", &done, "--json"]);
    cli.json(&["task", "status", &blocked, "blocked", "--json"]);

    let shown = cli.json(&["decision", "show", &did, "--json"]);
    let status_of = |tid: &str| -> String {
        shown["linked_tasks"]
            .as_array()
            .expect("linked_tasks is an array")
            .iter()
            .find(|t| id_str(&t["id"]) == tid)
            .unwrap_or_else(|| panic!("the linked task {tid} is missing: {shown}"))["status"]
            .as_str()
            .expect("status is a string")
            .to_string()
    };
    assert_eq!(status_of(&todo), "todo");
    assert_eq!(status_of(&doing), "in_progress");
    assert_eq!(status_of(&done), "done");
    assert_eq!(status_of(&blocked), "blocked");

    let (human, code) = cli.run(&["decision", "show", &did]);
    assert_eq!(code, 0);
    let line = |title: &str| -> String {
        human
            .lines()
            .find(|l| l.contains(title))
            .unwrap_or_else(|| panic!("the row for {title} exists: {human}"))
            .to_string()
    };
    assert!(line("終わった").contains("[x]"), "completed sinks: {}", line("終わった"));
    assert!(line("いま進めている").contains("(in_progress)"), "work in motion names itself");
    assert!(line("外の事情で止まっている").contains("(blocked)"), "stalled work also names itself");
    let untouched = line("まだ手を付けていない");
    assert!(untouched.contains("[ ]"), "incomplete is unchecked: {untouched}");
    assert!(!untouched.contains('('), "a default todo does not name its status: {untouched}");
}

/// `builds_on` hands a machine two things: read the premise first, and revisit when the premise is
/// overturned. Three surfaces carry it — the premise list of `decision show`, the note on an overturned
/// premise, and the blast radius named when one is superseded, rejected or deleted. It names (one hop, not transitive); it never blocks.
#[test]
fn a_premise_is_read_first_and_its_overturn_names_what_to_revisit() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();

    let add = |title: &str| -> String {
        id_str(&cli.json(&["decision", "add", "--project", &pid, "--title", title, "--json"])["decision"]["id"])
    };
    let premise = add("同期は撤去する");
    let standing = add("削除は物理削除にする");
    cli.json(&["decision", "accept", &premise, "--json"]);
    cli.json(&["decision", "accept", &standing, "--json"]);

    // Draw the premise edge: neither decision moves, and nothing is drawn at the premise.
    let built = cli.json(&["decision", "builds-on", &standing, "--on", &premise, "--json"]);
    assert_eq!(built["action"], "decision.builds_on");
    let shown = cli.json(&["decision", "show", &standing, "--json"]);
    assert_eq!(id_str(&shown["builds_on"][0]["id"]), premise, "premise = the decision to read first");
    assert!(shown["builds_on"][0]["superseded_by"].is_null(), "nothing has replaced the premise");
    // The reverse lookup is the blast radius — what needs revisiting if this decision is overturned.
    let from_premise = cli.json(&["decision", "show", &premise, "--json"]);
    assert_eq!(id_str(&from_premise["built_on_by"][0]["id"]), standing);

    // Overturning the premise names what must be revisited, and lets the operation through.
    let successor = add("同期をやり直す");
    let sup = cli.json(&["decision", "supersede", &successor, "--replaces", &premise, "--json"]);
    assert_eq!(sup["ok"], true, "it only surfaces = supersede succeeds");
    assert_eq!(id_str(&sup["decision"]["revisit"][0]["id"]), standing, "names the decision standing on the rotted premise");

    // Open the standing decision and the overturned premise is right there.
    let after = cli.json(&["decision", "show", &standing, "--json"]);
    assert_eq!(after["builds_on"][0]["superseded_by"], amenbo_core::idref::decision(successor.parse().unwrap()), "names the successor");
}

/// A listing row says **who** overturned a decision, not merely that something did. `superseded_by` is
/// the edges themselves, so a reader who finds a row replaced can go straight to the decision that
/// replaced it, from the listing and without opening anything. It is also the whole of what the row says
/// on the subject (`AMB-D-410`), so there is no second field for it to disagree with.
#[test]
fn a_decision_listing_row_names_what_superseded_it() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();

    let add = |title: &str| -> String {
        id_str(&cli.json(&["decision", "add", "--project", &pid, "--title", title, "--json"])["decision"]["id"])
    };
    let old = add("旧: 目録は同梱する");
    let newer = add("新: 目録は取りに行く");
    let untouched = add("無関係: 削除は物理削除にする");

    let row = |list: &Value, id: &str| -> Value {
        list["decisions"]
            .as_array()
            .expect("the listing carries its rows")
            .iter()
            .find(|d| id_str(&d["id"]) == id)
            .unwrap_or_else(|| panic!("no row for decision {id}"))
            .clone()
    };
    let named = |r: &Value| -> Vec<String> {
        r["superseded_by"].as_array().expect("always an array").iter().map(id_str).collect()
    };

    // With no edge drawn, every row names nobody — the field is present, not omitted.
    let before = cli.json(&["decision", "list", "--json"]);
    for id in [&old, &newer, &untouched] {
        assert!(named(&row(&before, id)).is_empty(), "no row names a successor yet");
    }

    cli.json(&["decision", "supersede", &newer, "--replaces", &old, "--json"]);

    let after = cli.json(&["decision", "list", "--json"]);
    let overturned = row(&after, &old);
    assert_eq!(
        named(&overturned),
        vec![amenbo_core::idref::decision(newer.parse().unwrap())],
        "and the row names the decision that replaced it, as its conversational ref"
    );
    // The successor, and a decision no edge touches, stay empty.
    for id in [&newer, &untouched] {
        assert!(named(&row(&after, id)).is_empty());
    }
}

/// `search --kind decision` runs over the same word index the task side does (`AMB-D-450`): it reaches a
/// decision's title, its body and the body of a comment on it, folded the same way. Words are the one
/// command's, on this side as on the other (`AMB-D-449`) — `decision list` narrows by status and edges,
/// never by a word.
#[test]
fn search_reaches_every_face_of_a_decision_and_folds_the_spellings() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "検索PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    let a_decision = |title: &str, body: &str| -> String {
        id_str(&cli.json(&[
            "decision", "add", "--project", &pid, "--title", title, "--body", body, "--json",
        ])["decision"]["id"])
    };
    let titled = a_decision("全文検索を索引に載せる", "本文はそのまま持つ");
    let bodied = a_decision("別の決定", "ＡＩ が読む Search の面");
    let commented = a_decision("三件目", "本文には無い");
    cli.json(&["decision", "comment", "add", &commented, "--text", "さーばの側で寄せる", "--json"]);

    // The decisions the word reached, read off `search`'s hits: a word may land on several faces of the
    // same decision, so the refs are folded back to one id apiece.
    let ids_for = |term: &str| -> Vec<String> {
        let mut ids: Vec<String> = cli.json(&[
            "search", term, "--kind", "decision", "--limit", "100", "--json",
        ])["hits"]
            .as_array()
            .expect("hits is an array")
            .iter()
            .map(|h| h["ref"].as_str().expect("a hit names its record").trim_start_matches("AMB-D-").to_string())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };

    assert_eq!(ids_for("全文検索"), vec![titled.clone()], "the title");
    assert_eq!(ids_for("読む"), vec![bodied.clone()], "the body");
    assert_eq!(ids_for("寄せる"), vec![commented.clone()], "a comment body");
    assert_eq!(ids_for("SEARCH"), vec![bodied.clone()], "case, on the index path");
    assert_eq!(ids_for("サーバ"), vec![commented.clone()], "kana");
    assert_eq!(ids_for("ai"), vec![bodied.clone()], "a two-character term takes the scan path");
    assert!(ids_for("全文一致").is_empty());
}

/// A decision's word faces reach what is attached to it, and to its comments (`AMB-D-450`) — the same
/// join the task side makes, bar the labels, which only a task carries.
#[test]
fn search_reaches_what_is_attached_to_a_decision() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "添付検索PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    let a_decision = |title: &str| -> String {
        id_str(&cli.json(&["decision", "add", "--project", &pid, "--title", title, "--json"])["decision"]["id"])
    };
    let carrying = a_decision("SCENARIO — the one with the evidence");
    let bystander = a_decision("SCENARIO — the one with none");

    cli.json(&["decision", "attach", &carrying, "https://example.com/latency-profile", "--url", "--json"]);
    let cid = id_str(
        &cli.json(&["decision", "comment", "add", &carrying, "--text", "測ったものを付ける", "--json"])["comment"]["id"],
    );
    let file = cli.home.join("実測メモ.md");
    std::fs::write(&file, "# numbers\n").unwrap();
    cli.json(&["decision", "comment", "attach", &cid, file.to_str().unwrap(), "--json"]);

    // The decisions the word reached, read off `search`'s hits: a word may land on several faces of the
    // same decision, so the refs are folded back to one id apiece.
    let ids_for = |term: &str| -> Vec<String> {
        let mut ids: Vec<String> = cli.json(&[
            "search", term, "--kind", "decision", "--limit", "100", "--json",
        ])["hits"]
            .as_array()
            .expect("hits is an array")
            .iter()
            .map(|h| h["ref"].as_str().expect("a hit names its record").trim_start_matches("AMB-D-").to_string())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };

    assert_eq!(ids_for("latency-profile"), vec![carrying.clone()], "a link hanging off the decision");
    assert_eq!(ids_for("実測メモ"), vec![carrying.clone()], "a file hanging off one of its comments");
    assert!(!ids_for("latency-profile").contains(&bystander));
}

/// The terminal's two axes (`AMB-D-562`): `--kind` says which record the words are on, `--face` which
/// face of it, and they are judged apart — so the pair asks for the product neither one alone can, the
/// remarks on decisions. The same word is written on a remark on each side here, which is what makes
/// the narrowing visible: a `--face` that did nothing would answer with both.
#[test]
fn search_narrows_by_face_as_an_axis_apart_from_the_side() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "面で絞るPJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    let did = id_str(
        &cli.json(&["decision", "add", "--project", &pid, "--title", "掃引の決定", "--json"])["decision"]["id"],
    );
    cli.json(&["decision", "comment", "add", &did, "--text", "掃引は夜に回す", "--json"]);
    let tid = id_str(
        &cli.json(&["task", "add", "--project", &pid, "--title", "掃引を書く", "--json"])["task"]["id"],
    );
    cli.finish_creating(&tid);
    cli.json(&["comment", "add", &tid, "--text", "掃引はここで走る", "--json"]);

    let asked = |narrowing: &[&str]| -> Vec<String> {
        let mut args = vec!["search", "掃引"];
        args.extend_from_slice(narrowing);
        args.extend_from_slice(&["--limit", "100", "--json"]);
        let mut said: Vec<String> = cli.json(&args)["hits"]
            .as_array()
            .expect("hits is an array")
            .iter()
            .map(|h| format!("{} {}", h["face"].as_str().expect("a hit names its face"), h["ref"].as_str().unwrap()))
            .collect();
        said.sort();
        said
    };

    assert_eq!(
        asked(&["--face", "comment"]),
        vec![format!("comment AMB-D-{did}"), format!("comment AMB-T-{tid}")],
        "the face alone keeps both sides' remarks"
    );
    assert_eq!(
        asked(&["--kind", "decision", "--face", "comment"]),
        vec![format!("comment AMB-D-{did}")],
        "the product: the remarks on decisions, which neither axis alone asks for"
    );
    assert_eq!(
        asked(&["--kind", "decision", "--face", "title"]),
        vec![format!("title AMB-D-{did}")],
        "the other face of the same side"
    );

    // The narrowing is echoed back apart from the first axis, so a caller can see which of the two it
    // actually asked for.
    let echo = cli.json(&["search", "掃引", "--kind", "decision", "--face", "comment", "--json"])["query"].clone();
    assert_eq!(echo["kind"], "decision");
    assert_eq!(echo["face"], "comment");

    // A value the axis does not have is refused, rather than read as "no narrowing" and answered with
    // everything.
    let (err, code) = cli.run_err(&["search", "掃引", "--face", "notes", "--json"]);
    assert_eq!(code, 1, "an unknown face is an error: {err}");
    assert!(err.contains("notes"), "the message names what was asked for: {err}");
}
