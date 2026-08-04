//! Tasks, end to end: the lifecycle a task walks, the premises that hold a reservation back, the
//! reservation itself, who it is assigned to, where it is filed, and what `task show` gathers up.

mod harness;

use serde_json::Value;

use harness::*;

#[test]
fn full_task_lifecycle() {
    let cli = Cli::new();

    // Project → task (classification lives on dimensions; the task's place is the project itself).
    let p = cli.json(&["project", "add", "--name", "サイト刷新", "--view", "board", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    assert_eq!(p["action"], "project.add");

    let t = cli.json(&[
        "task", "add", "--title", "ワイヤー作成", "--project", &pid,
        "--due", "2026-06-30", "--priority", "high", "--json",
    ]);
    let tid = id_str(&t["task"]["id"]);
    assert_eq!(t["task"]["due_on"], "2026-06-30");
    assert_eq!(t["task"]["priority"], "high");
    assert_eq!(id_str(&t["task"]["placement"]["project"]["id"]), pid);

    // Breaking work down means another task plus a dependency edge.
    let t2 = cli.json(&[
        "task", "add", "--title", "配色決め", "--project", &pid, "--json",
    ]);
    let t2id = id_str(&t2["task"]["id"]);
    // The wireframe depends on the palette (the palette must be done first).
    let dep = cli.json(&["task", "depend", &tid, "--on", &t2id, "--json"]);
    assert_eq!(id_str(&dep["task"]["blocked_by"][0]["id"]), t2id);

    // Listing: both tasks sit at top level — there is no parent/child folding.
    let list = cli.json(&["task", "list", "--project", &pid, "--json"]);
    assert_eq!(list["count"], 2);

    // Completion is idempotent.
    let done = cli.json(&["task", "done", &tid, "--json"]);
    assert_eq!(done["task"]["completed"], true);
    assert_eq!(done["noop"], false);
    let again = cli.json(&["task", "done", &tid, "--json"]);
    assert_eq!(again["noop"], true);
}

/// Placing a premise (a blocker, or a linked decision) on a reserved (`in_progress`) task silently drops it
/// to `ready:no`. It stays allowed, but the changer is warned on stderr so it is not silent (`AMB-D-366`,
/// changer side). A `todo` / `blocked` / `done` target says nothing.
#[test]
fn adding_a_premise_to_a_reserved_task_warns_the_changer() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "警告PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let a = cli.json(&["task", "add", "--title", "予約先", "--project", &pid, "--json"]);
    let aid = id_str(&a["task"]["id"]);
    let b = cli.json(&["task", "add", "--title", "前提", "--project", &pid, "--json"]);
    let bid = id_str(&b["task"]["id"]);
    let a_ref = a["task"]["ref"].as_str().unwrap();
    cli.finish_creating(&aid);
    cli.finish_creating(&bid);

    // A todo target is silent — no advisory on stderr.
    let (_o0, e0, c0) = cli.run_both(&["task", "depend", &aid, "--on", &bid, "--json"]);
    assert_eq!(c0, 0);
    assert!(!e0.contains('⚠'), "a todo target must not warn: {e0}");
    // Idempotent-undo so the same edge can be re-added once A is reserved.
    let _ = cli.json(&["task", "undepend", &aid, "--on", &bid, "--json"]);

    // Reserve A (todo→in_progress), then re-add the blocker: now the holder must be told.
    cli.json(&["task", "status", &aid, "in_progress", "--json"]);
    let (o1, e1, c1) = cli.run_both(&["task", "depend", &aid, "--on", &bid, "--json"]);
    assert_eq!(c1, 0, "the warn does not fail the command: {e1}");
    assert!(o1.contains("\"action\""), "stdout still carries the JSON envelope: {o1}");
    assert!(e1.contains('⚠') && e1.contains("reserved"), "a reserved target warns: {e1}");
    assert!(e1.contains(a_ref), "the warn names the reserved task {a_ref}: {e1}");

    // Re-running the same edge is an idempotent no-op — nothing was added, so no second warn.
    let (_o2, e2, _c2) = cli.run_both(&["task", "depend", &aid, "--on", &bid, "--json"]);
    assert!(!e2.contains('⚠'), "a no-op edge must not warn: {e2}");

    // Linking a decision as a premise to the reserved task warns on the same footing.
    let d = cli.json(&["decision", "add", "--title", "根拠", "--body", "x", "--project", &pid, "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let (_o3, e3, c3) = cli.run_both(&["decision", "link", &did, &aid, "--json"]);
    assert_eq!(c3, 0);
    assert!(e3.contains('⚠') && e3.contains(a_ref), "linking a premise to a reserved task warns: {e3}");
}

#[test]
fn reopening_a_decision_under_a_reserved_task_warns_the_changer() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "再開警告PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let d = cli.json(&["decision", "add", "--title", "根拠", "--body", "x", "--project", &pid, "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let d_ref = d["decision"]["ref"].as_str().unwrap().to_string();
    let t = cli.json(&["task", "add", "--title", "根拠に立つ作業", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let t_ref = t["task"]["ref"].as_str().unwrap().to_string();
    cli.finish_creating(&tid);

    // A premise must be accepted before the task it holds can be reserved, so accept, then link.
    cli.json(&["decision", "accept", &did, "--json"]);
    cli.json(&["decision", "link", &did, &tid, "--json"]);

    // Nobody holds the task yet: reopening takes no ground out from under anyone.
    let (_o0, e0, c0) = cli.run_both(&["decision", "reopen", &did, "--json"]);
    assert_eq!(c0, 0);
    assert!(!e0.contains('⚠'), "a todo linked task must not warn: {e0}");

    cli.json(&["decision", "accept", &did, "--json"]);
    cli.json(&["task", "status", &tid, "in_progress", "--json"]);

    // Reopening pulls the settled ground out from under the reservation: the changer is told.
    let (o1, e1, c1) = cli.run_both(&["decision", "reopen", &did, "--json"]);
    assert_eq!(c1, 0, "the warn does not fail the command: {e1}");
    assert!(o1.contains("\"action\""), "stdout still carries the JSON envelope: {o1}");
    // A real reopen is not a no-op — the envelope says so, matching accept/reject.
    let v1: Value = serde_json::from_str(&o1).unwrap();
    assert_eq!(v1["noop"], Value::Bool(false), "a real reopen is not a no-op: {o1}");
    assert!(e1.contains('⚠') && e1.contains(&t_ref), "the warn names the reserved task {t_ref}: {e1}");
    assert!(e1.contains(&d_ref), "the warn names the decision {d_ref}: {e1}");

    // Reopening what is already proposed settles nothing anew — no second warn, and the envelope
    // flags it as a no-op instead of reporting "✓ Reopened" as if it just changed.
    let (o2, e2, c2) = cli.run_both(&["decision", "reopen", &did, "--json"]);
    assert_eq!(c2, 0);
    assert!(!e2.contains('⚠'), "an idempotent reopen must not warn: {e2}");
    let v2: Value = serde_json::from_str(&o2).unwrap();
    assert_eq!(v2["noop"], Value::Bool(true), "an idempotent reopen is a no-op: {o2}");

    // The human line says so too, distinct from the "✓ Reopened" of a real change.
    let (h, _he, hc) = cli.run_both(&["decision", "reopen", &did]);
    assert_eq!(hc, 0);
    assert!(h.contains("already proposed") && h.contains("no change"), "idempotent reopen reports no change: {h}");
}

/// The other act that unsettles a premise: superseding leaves the old decision accepted but no longer
/// current, which reads the same way `ready` does — so the changer hears about the reservations standing
/// on it, exactly as with a reopen (`AMB-D-373`).
#[test]
fn superseding_a_decision_under_a_reserved_task_warns_the_changer() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "差替警告PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // A premise plus the task standing on it, accepted and linked — the shape both halves need. A
    // superseded decision cannot be put back (accepting it again settles nothing, the edge stands), so
    // the held and the unheld case each get their own pair rather than reusing one.
    let premise_and_task = |title: &str| {
        let d = cli.json(&["decision", "add", "--title", title, "--body", "x", "--project", &pid, "--json"]);
        let did = id_str(&d["decision"]["id"]);
        let t = cli.json(&["task", "add", "--title", title, "--project", &pid, "--json"]);
        let tid = id_str(&t["task"]["id"]);
        cli.finish_creating(&tid);
        cli.json(&["decision", "accept", &did, "--json"]);
        cli.json(&["decision", "link", &did, &tid, "--json"]);
        let d_ref = d["decision"]["ref"].as_str().unwrap().to_string();
        let t_ref = t["task"]["ref"].as_str().unwrap().to_string();
        (did, d_ref, tid, t_ref)
    };
    let successor = |title: &str| {
        let d = cli.json(&["decision", "add", "--title", title, "--body", "y", "--project", &pid, "--json"]);
        id_str(&d["decision"]["id"])
    };

    // Nobody holds the task yet: superseding takes no ground out from under anyone.
    let (unheld, _, _, _) = premise_and_task("誰も予約していない根拠");
    let (_o0, e0, c0) =
        cli.run_both(&["decision", "supersede", &successor("新根拠1"), "--replaces", &unheld, "--json"]);
    assert_eq!(c0, 0);
    assert!(!e0.contains('⚠'), "a todo linked task must not warn: {e0}");

    // Now the other pair, with the task reserved: superseding pulls the settled ground out from under it.
    let (held, held_ref, tid, t_ref) = premise_and_task("予約中の作業が立つ根拠");
    cli.json(&["task", "status", &tid, "in_progress", "--json"]);
    let new_id = successor("新根拠2");
    let (o1, e1, c1) = cli.run_both(&["decision", "supersede", &new_id, "--replaces", &held, "--json"]);
    assert_eq!(c1, 0, "the warn does not fail the command: {e1}");
    assert!(o1.contains("\"action\""), "stdout still carries the JSON envelope: {o1}");
    assert!(e1.contains('⚠') && e1.contains(&t_ref), "the warn names the reserved task {t_ref}: {e1}");
    assert!(e1.contains(&held_ref), "the warn names the superseded decision {held_ref}: {e1}");

    // Drawing the same edge again changes nothing, so it unsettles nothing anew — no second warn.
    let (_o2, e2, c2) = cli.run_both(&["decision", "supersede", &new_id, "--replaces", &held, "--json"]);
    assert_eq!(c2, 0);
    assert!(!e2.contains('⚠'), "an idempotent supersede must not warn: {e2}");
}

#[test]
fn task_dependencies_drive_ready_and_unblock() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "依存PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let a = cli.json(&["task", "add", "--title", "土台", "--project", &pid, "--json"]);
    let aid = id_str(&a["task"]["id"]);
    let b = cli.json(&["task", "add", "--title", "上物", "--project", &pid, "--json"]);
    let bid = id_str(&b["task"]["id"]);
    cli.finish_creating(&aid);
    cli.finish_creating(&bid);

    // b depends on a (a must be done first).
    let dep = cli.json(&["task", "depend", &bid, "--on", &aid, "--json"]);
    assert_eq!(dep["action"], "task.depend");
    assert_eq!(dep["task"]["ready"], false);
    assert_eq!(id_str(&dep["task"]["blocked_by"][0]["id"]), aid);

    // Self-reference and cycles are refused (a→b would close the loop with b→a).
    let (_e, code) = cli.run_err(&["task", "depend", &bid, "--on", &bid, "--json"]);
    assert_ne!(code, 0);
    let (_e2, code2) = cli.run_err(&["task", "depend", &aid, "--on", &bid, "--json"]);
    assert_ne!(code2, 0);

    // A ready:yes mailbox holds only a; b is blocked.
    let ready = cli.json(&["task", "list", "--project", &pid, "--filter", "ready:yes", "--json"]);
    let ready_ids: Vec<String> = ready["tasks"].as_array().unwrap().iter().map(|t| id_str(&t["id"])).collect();
    assert!(ready_ids.contains(&aid) && !ready_ids.contains(&bid));
    // ready:no (blocked) holds only b.
    let blocked = cli.json(&["task", "list", "--project", &pid, "--filter", "ready:no", "--json"]);
    let blocked_ids: Vec<String> = blocked["tasks"].as_array().unwrap().iter().map(|t| id_str(&t["id"])).collect();
    assert_eq!(blocked_ids, vec![bid.clone()]);

    // Completing a makes b ready and records task.unblocked on b.
    cli.json(&["task", "done", &aid, "--json"]);
    let show_b = cli.json(&["task", "show", &bid, "--json"]);
    assert_eq!(show_b["ready"], true);
    assert!(show_b["blocked_by"].as_array().unwrap().is_empty());
    let acts = cli.json(&["activity", "--task", &bid, "--json"]);
    let has_unblock = acts["items"].as_array().unwrap().iter()
        .any(|i| i["event"]["kind"] == "task.unblocked");
    assert!(has_unblock, "task.unblocked was not recorded: {acts}");
}

/// A blocker that is **decided against** releases what it was holding back, exactly as a finished one
/// does (`AMB-D-397`). The two terminals differ only in whether the work was carried out; leaving
/// `rejected` out of the open-blocker reading would strand every dependent behind a task nobody is going
/// to do, with `undepend` the only way out — and nothing on any screen to say so.
///
/// The unblock signal has to follow the same reading: readiness is derived on every query either way, so
/// what would be lost is the line that says *when* it happened.
#[test]
fn rejecting_a_blocker_releases_its_dependents() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "却下PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let a = cli.json(&["task", "add", "--title", "やらないと決めた土台", "--project", &pid, "--json"]);
    let aid = id_str(&a["task"]["id"]);
    let b = cli.json(&["task", "add", "--title", "上物", "--project", &pid, "--json"]);
    let bid = id_str(&b["task"]["id"]);
    cli.finish_creating(&aid);
    cli.finish_creating(&bid);
    cli.json(&["task", "depend", &bid, "--on", &aid, "--json"]);
    assert_eq!(cli.json(&["task", "show", &bid, "--json"])["ready"], false);

    cli.json(&["task", "status", &aid, "rejected", "--json"]);

    let show_b = cli.json(&["task", "show", &bid, "--json"]);
    assert_eq!(show_b["ready"], true, "a blocker decided against is a blocker no longer");
    assert!(show_b["blocked_by"].as_array().unwrap().is_empty());
    let acts = cli.json(&["activity", "--task", &bid, "--json"]);
    let has_unblock = acts["items"].as_array().unwrap().iter()
        .any(|i| i["event"]["kind"] == "task.unblocked");
    assert!(has_unblock, "the unblock was not announced: {acts}");
    // The rejected task itself is not a completed one — it is out of the open counts and out of the
    // finished ones both.
    let show_a = cli.json(&["task", "show", &aid, "--json"]);
    assert_eq!(show_a["status"], "rejected");
    assert_eq!(show_a["completed"], false, "decided against is not carried out");
}

/// `done:` asks whether a task is **closed**, and `status:` asks which state it is in — two questions
/// with two answers (`AMB-D-397`). A task decided against belongs with `done:true` and stays out of
/// `done:false`, which is the shape of every mailbox query; what was carried out is `status:done`, and
/// `rejected` is an any-of element there like any other value.
#[test]
fn done_asks_whether_a_task_is_closed_and_status_asks_which_way() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "終端PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let ids: Vec<String> = ["残っている", "やり遂げた", "やらないと決めた"]
        .iter()
        .map(|t| id_str(&cli.json(&["task", "add", "--title", t, "--project", &pid, "--json"])["task"]["id"]))
        .collect();
    cli.json(&["task", "done", &ids[1], "--json"]);
    cli.json(&["task", "status", &ids[2], "rejected", "--json"]);

    let listed = |filter: &str| -> Vec<String> {
        cli.json(&["task", "list", "--project", &pid, "--filter", filter, "--json"])["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| id_str(&t["id"]))
            .collect()
    };

    assert_eq!(listed("done:false"), vec![ids[0].clone()], "only what is still work");
    let mut closed = listed("done:true");
    closed.sort();
    let mut both_terminals = vec![ids[1].clone(), ids[2].clone()];
    both_terminals.sort();
    assert_eq!(closed, both_terminals, "both ways of ending are closed");
    assert_eq!(listed("status:done"), vec![ids[1].clone()], "carried out is its own question");
    assert_eq!(listed("status:rejected"), vec![ids[2].clone()], "and so is decided against");
    let mut any_of = listed("status:todo,rejected");
    any_of.sort();
    let mut expected = vec![ids[0].clone(), ids[2].clone()];
    expected.sort();
    assert_eq!(any_of, expected, "the new value is an any-of element like the others");

    let (err, code) = cli.run_err(&["task", "list", "--filter", "status:shipped", "--json"]);
    assert_ne!(code, 0);
    assert!(err.contains("rejected"), "the refusal names every value it would take: {err}");
}

/// Every task belongs to a project: `task add` without --project is refused (no unnumbered
/// orphan/inbox task), and the error lists existing projects to pick from.
#[test]
fn task_add_requires_project() {
    let cli = Cli::new();
    // Refused even before any project exists (exit 1).
    let (_e0, code0) = cli.run_err(&["task", "add", "--title", "宙ぶらりん", "--json"]);
    assert_eq!(code0, 1, "without --project it is rejected (exit 1)");

    // With projects around, the error lists them by name so one can be picked.
    let p = cli.json(&["project", "add", "--name", "受け皿", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let (err, code) = cli.run_err(&["task", "add", "--title", "宙ぶらりん", "--json"]);
    assert_eq!(code, 1, "even with an existing project, no --project is rejected");
    assert!(err.contains("受け皿"), "the error names the existing project: {err}");

    // With --project it goes through and is numbered inside its project.
    let ok = cli.json(&["task", "add", "--title", "所属あり", "--project", &pid, "--json"]);
    assert_eq!(ok["task"]["title"], "所属あり");
    assert_eq!(id_str(&ok["task"]["placement"]["project"]["id"]), pid);
    // Not a single project-less task was created.
    let all = cli.json(&["task", "list", "--json"]);
    assert_eq!(all["count"], 1, "only the one task with a project was created");
}

#[test]
fn comment_assign_lifecycle() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    // Assign to myself (human): the assignee is a facet — one local store means the only recipient is me.
    let uid = "human";
    let assigned = cli.json(&["task", "assign", &tid, "--to", uid, "--json"]);
    assert_eq!(assigned["task"]["assignee_kind"], "human");
    // The assignee filter narrows by facet token.
    let mine = cli.json(&["task", "list", "--filter", &format!("assignee:{uid}"), "--json"]);
    assert_eq!(mine["count"], 1);
    // `task list --json` carries assignee_kind too, not just `task show`.
    assert_eq!(mine["tasks"][0]["assignee_kind"], "human");
    // unassign clears it.
    let un = cli.json(&["task", "unassign", &tid, "--json"]);
    assert!(un["task"]["assignee_kind"].is_null());

    // comment add shows up in num_comments and in comment list.
    cli.json(&["comment", "add", &tid, "--text", "先方確認待ち", "--json"]);
    let shown = cli.json(&["task", "show", &tid, "--json"]);
    assert_eq!(shown["num_comments"], 1);
    let comments = cli.json(&["comment", "list", &tid, "--json"]);
    assert_eq!(comments["count"], 1);
    assert_eq!(comments["comments"][0]["text"], "先方確認待ち");
    // The author is the current human facet; its name is config.human_name, unset here so the language default `Human` stands.
    assert_eq!(comments["comments"][0]["author"]["name"], "Human");
}

#[test]
fn task_add_delegates_in_one_step() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "委任PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    // One local store: the only delegate is me — my human facet or my AI.
    let uid = "human";

    // --to assigns at creation time (kind defaults to human).
    let t = cli.json(&["task", "add", "--title", "下調べ", "--project", &pid, "--to", uid, "--json"]);
    assert_eq!(t["task"]["assignee_kind"], "human");

    // --to --ai delegates to my AI (kind=ai), folding create→assign into one command.
    let t2 = cli.json(&["task", "add", "--title", "ログ調査", "--project", &pid, "--to", uid, "--ai", "--json"]);
    assert_eq!(t2["task"]["assignee_kind"], "ai");

    // The assignee filter splits by facet (human vs ai).
    let human_mine = cli.json(&["task", "list", "--filter", "assignee:human", "--json"]);
    assert_eq!(human_mine["count"], 1, "one addressed to the human facet");
    let ai_mine = cli.json(&["task", "list", "--filter", "assignee:me-ai", "--json"]);
    assert_eq!(ai_mine["count"], 1, "one addressed to the AI facet");

    // --ai without --to is rejected.
    let (_e, code) = cli.run_err(&["task", "add", "--title", "x", "--project", &pid, "--ai", "--json"]);
    assert_ne!(code, 0, "--ai requires --to");

    // An unresolvable recipient is refused *before* the task is created — no orphan is left behind.
    let before = cli.json(&["task", "list", "--project", &pid, "--json"])["count"].as_i64().unwrap();
    let (_e2, code2) = cli.run_err(&["task", "add", "--title", "孤児", "--project", &pid, "--to", "居ない人", "--json"]);
    assert_ne!(code2, 0);
    let after = cli.json(&["task", "list", "--project", &pid, "--json"])["count"].as_i64().unwrap();
    assert_eq!(before, after, "no task must be created when resolution fails");
}

/// A task lives in exactly one project, and `task move` rehomes it: placement names the new one.
#[test]
fn task_move_rehomes_single_placement() {
    let cli = Cli::new();
    let pa = cli.json(&["project", "add", "--name", "PJ-A", "--json"]);
    let pa_id = id_str(&pa["project"]["id"]);
    let pb = cli.json(&["project", "add", "--name", "PJ-B", "--json"]);
    let pb_id = id_str(&pb["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "横断タスク", "--project", &pa_id, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    // Freshly created, it belongs to PJ-A.
    assert_eq!(id_str(&t["task"]["placement"]["project"]["id"]), pa_id);
    assert_eq!(cli.json(&["task", "list", "--project", &pa_id, "--json"])["count"], 1);

    // After the move it belongs to PJ-B alone: gone from A's listing, present in B's.
    let moved = cli.json(&["task", "move", &tid, "--project", &pb_id, "--json"]);
    assert_eq!(moved["action"], "task.move");
    assert_eq!(id_str(&moved["task"]["placement"]["project"]["id"]), pb_id);
    assert_eq!(cli.json(&["task", "list", "--project", &pa_id, "--json"])["count"], 0);
    assert_eq!(cli.json(&["task", "list", "--project", &pb_id, "--json"])["count"], 1);
    let f = cli.json(&["task", "list", "--filter", &format!("project:{pb_id}"), "--json"]);
    assert_eq!(f["count"], 1);

    // `addto`/`removefrom` do not exist (unknown subcommand → clap exit 2).
    let (_, code) = cli.run(&["task", "addto", &tid, "--project", &pa_id, "--json"]);
    assert_ne!(code, 0, "addto has been removed");
    let (_, code) = cli.run(&["task", "removefrom", &tid, "--project", &pb_id, "--json"]);
    assert_ne!(code, 0, "removefrom has been removed");

    // doctor finds no orphans.
    assert_eq!(cli.json(&["doctor", "--json"])["ok"], true);
}

/// The `status` view's in-progress section is the reservation, and nothing else (`AMB-D-118`). A start day
/// that has come says the work *may* begin — one more thing than the calendar can say about who is on it —
/// so reading the day for this section listed work nobody had picked up, and the count said the same. What
/// the section answers is "what do I have my hands on", which only the status field knows.
#[test]
fn status_lists_the_reserved_as_in_progress_and_not_the_merely_startable() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project();

    // Its day has come and nobody has taken it: startable, not started.
    let startable = id_str(
        &cli.json(&[
            "task", "add", "--title", "開始日だけ来ている", "--project", &pid, "--start", "today",
            "--json",
        ])["task"]["id"],
    );
    // Reserved, and carrying no day at all — the case the old reading could not see.
    let reserved = id_str(
        &cli.json(&["task", "add", "--title", "予約済み", "--project", &pid, "--json"])["task"]
            ["id"],
    );
    cli.finish_creating(&startable);
    cli.finish_creating(&reserved);
    cli.json(&["task", "status", &reserved, "in_progress", "--json"]);

    let s = cli.json(&["status", "--json"]);
    let listed: Vec<String> =
        s["in_progress"].as_array().unwrap().iter().map(|t| id_str(&t["id"])).collect();
    assert_eq!(listed, vec![reserved], "only the reserved task is under way: {s}");
    assert!(!listed.contains(&startable), "a day that has come is not a pair of hands: {s}");
    assert_eq!(s["counts"]["in_progress"], 1, "the count reads the section, not a second rule: {s}");
}

#[test]
fn assign_is_plain_reassignment() {
    // Assignment is plain reassignment: no special state, no mandatory-reason gate.
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let t = id_str(&cli.json(&["task", "add", "--title", "asg", "--project", &pid, "--json"])["task"]["id"]);

    // The human delegates to the AI → assignee_kind=ai.
    let a = cli.json(&["task", "assign", &t, "--to", "tester", "--ai", "--json"]);
    assert_eq!(a["task"]["assignee_kind"], "ai");

    // The AI hands it back (ai→human) with no reason required.
    let h = cli.json(&["task", "assign", &t, "--to", "tester", "--actor", "ai", "--json"]);
    assert_eq!(h["task"]["assignee_kind"], "human");

    // Reassigning to the same assignee/kind is an idempotent no-op.
    let again = cli.json(&["task", "assign", &t, "--to", "tester", "--actor", "ai", "--json"]);
    assert_eq!(again["noop"], true, "an identical assignment is a noop");
}

#[test]
fn status_transitions_and_completed_stays_in_sync() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let t = cli.json(&["task", "add", "--title", "S3", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    cli.finish_creating(&tid);
    // A new task starts as todo.
    assert_eq!(cli.json(&["task", "show", &tid, "--json"])["status"], "todo");

    // in_progress leaves completed false.
    let ip = cli.json(&["task", "status", &tid, "in_progress", "--json"]);
    assert_eq!(ip["task"]["status"], "in_progress");
    assert_eq!(ip["task"]["completed"], false);

    // The done sugar sets status=done and completed=true.
    let done = cli.json(&["task", "done", &tid, "--json"]);
    assert_eq!(done["task"]["status"], "done");
    assert_eq!(done["task"]["completed"], true);

    // The reopen sugar sets status=todo and completed=false.
    let re = cli.json(&["task", "reopen", &tid, "--json"]);
    assert_eq!(re["task"]["status"], "todo");
    assert_eq!(re["task"]["completed"], false);

    // block --reason records the reason as a comment.
    let blk = cli.json(&["task", "block", &tid, "--reason", "先方確認待ち", "--json"]);
    assert_eq!(blk["task"]["status"], "blocked");
    let comments = cli.json(&["comment", "list", &tid, "--json"]);
    assert!(comments["comments"].as_array().unwrap().iter().any(|c| c["text"] == "先方確認待ち"));

    // Setting the status it already holds is an idempotent no-op.
    let noop = cli.json(&["task", "status", &tid, "blocked", "--json"]);
    assert_eq!(noop["noop"], true);

    // An invalid status is invalid_value (exit 2).
    let (_o, code) = cli.run(&["task", "status", &tid, "frozen"]);
    assert_eq!(code, 2);
}

/// `task reject` is the write port for the terminal that is not an achievement (`AMB-D-397`): the reason
/// is what the row is kept for, so it cannot be skipped, and it lands on the timeline rather than in a
/// field of its own. Reopen is the way back — reading `completed` there would answer "not done" about a
/// task that has very much ended, and silently do nothing.
#[test]
fn rejecting_a_task_demands_a_reason_and_keeps_it_on_the_timeline() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let tid = id_str(&cli.json(&["task", "add", "--title", "弱ハードの実測", "--project", &pid, "--json"])["task"]["id"]);

    // No --reason at all: clap refuses the invocation (exit 2) — the flag is required, not optional.
    let (_o, code) = cli.run(&["task", "reject", &tid]);
    assert_eq!(code, 2, "a rejection with no reasoning is what this command exists to prevent");
    // An empty one gets past clap but not past the command.
    let (err, code) = cli.run_err(&["task", "reject", &tid, "--reason", "   ", "--json"]);
    assert_eq!(code, 2, "an empty reason is refused too: {err}");
    assert_eq!(cli.json(&["task", "show", &tid, "--json"])["status"], "todo", "a refused rejection moves nothing");

    // With a reason: the task ends as rejected — closed, but not carried out.
    let rj = cli.json(&["task", "reject", &tid, "--reason", "測っても分岐が痩せていて何も変わらない", "--json"]);
    assert_eq!(rj["task"]["status"], "rejected");
    assert_eq!(rj["task"]["completed"], false, "decided against is not carried out");
    let comments = cli.json(&["comment", "list", &tid, "--json"]);
    assert!(
        comments["comments"].as_array().unwrap().iter().any(|c| c["text"] == "測っても分岐が痩せていて何も変わらない"),
        "the reason is kept as a comment: {comments}"
    );

    // Re-rejecting is an idempotent no-op, and does not pile a second reason on.
    let again = cli.json(&["task", "reject", &tid, "--reason", "二度目の理由", "--json"]);
    assert_eq!(again["noop"], true);
    let comments = cli.json(&["comment", "list", &tid, "--json"]);
    assert_eq!(comments["comments"].as_array().unwrap().len(), 1, "a re-reject explains nothing new: {comments}");

    // Reopen brings it back from the rejected terminal, the same as it does from done.
    let re = cli.json(&["task", "reopen", &tid, "--json"]);
    assert_eq!(re["noop"], false, "reopening a rejected task is a real change, not a no-op");
    assert_eq!(re["task"]["status"], "todo");
}

/// The holder-side surface of `AMB-D-366`: after a task is reserved (`in_progress`), a premise pinned on
/// afterwards silently drops its readiness. An ordinary `task show` (the early warning) and completing it
/// (the safety net) both surface the added premise; a fresh reservation and a plain `todo` task show
/// nothing. Timestamps are whole seconds, so the test waits past the reservation second before adding the
/// edge — mirroring the real gap between reserving and a later change.
#[test]
fn a_premise_pinned_on_after_reservation_surfaces_on_show_and_completion() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let a = id_str(&cli.json(&["task", "add", "--title", "保持タスク", "--project", &pid, "--json"])["task"]["id"]);
    let b = id_str(&cli.json(&["task", "add", "--title", "後付けブロッカー", "--project", &pid, "--json"])["task"]["id"]);
    cli.finish_creating(&a);
    cli.finish_creating(&b);

    // A plain todo task surfaces nothing — the surface is scoped to the reservation holder.
    assert!(cli.json(&["task", "show", &a, "--json"]).get("premise_change").is_none());

    // Reserve A. Its status clock is stamped now; nothing is pinned on yet, so there is still nothing to show.
    assert_eq!(cli.json(&["task", "status", &a, "in_progress", "--json"])["task"]["status"], "in_progress");
    assert!(cli.json(&["task", "show", &a, "--json"]).get("premise_change").is_none());

    // Wait past the reservation second so the new edge is unambiguously "after" the status clock.
    std::thread::sleep(std::time::Duration::from_millis(2000));

    // A blocker is pinned on after the reservation — silently dropping A's readiness under the holder.
    cli.json(&["task", "depend", &a, "--on", &b, "--json"]);

    // Early warning: an ordinary `task show` now surfaces the added blocker.
    let show = cli.json(&["task", "show", &a, "--json"]);
    let blockers = show["premise_change"]["added_blockers"].as_array().expect("premise_change surfaced on show");
    assert_eq!(id_str(&blockers[0]["id"]), b);

    // Safety net: completing the reserved task folds the same change into the envelope (under the task
    // resource, the write-envelope shape), and never blocks it.
    let done = cli.json(&["task", "done", &a, "--json"]);
    assert_eq!(done["task"]["completed"], true);
    assert_eq!(id_str(&done["task"]["premise_change"]["added_blockers"][0]["id"]), b);
}

#[test]
fn assign_facet_and_mailbox_filters() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // Assignees are referenced by facet token (human).
    let me = "human";
    let pid = cli.bound_project(); // what the AI touches lives in the bound project

    // Assigned to my AI, not started.
    let t1 = id_str(&cli.json(&["task", "add", "--title", "ai-todo", "--project", &pid, "--json"])["task"]["id"]);
    cli.run(&["task", "assign", &t1, "--to", me, "--ai"]);
    // Assigned to me (human).
    let t2 = id_str(&cli.json(&["task", "add", "--title", "human-mine", "--project", &pid, "--json"])["task"]["id"]);
    cli.run(&["task", "assign", &t2, "--to", me]);
    // Assigned to my AI and under way — reservation is status alone (in_progress).
    let t3 = id_str(&cli.json(&["task", "add", "--title", "ai-inprogress", "--project", &pid, "--json"])["task"]["id"]);
    cli.finish_creating(&t1);
    cli.finish_creating(&t2);
    cli.finish_creating(&t3);
    cli.run(&["task", "assign", &t3, "--to", me, "--ai"]);
    cli.run(&["task", "status", &t3, "in_progress", "--actor", "ai"]);

    // assignee_kind is stamped.
    assert_eq!(cli.json(&["task", "show", &t1, "--json"])["assignee_kind"], "ai");
    assert_eq!(cli.json(&["task", "show", &t2, "--json"])["assignee_kind"], "human");

    let titles = |filter: &str| -> Vec<String> {
        cli.json(&["task", "list", "--filter", filter, "--json"])["tasks"]
            .as_array().unwrap().iter()
            .map(|t| t["title"].as_str().unwrap().to_string()).collect()
    };

    // The mailbox is AI-assigned and unstarted: t3 is under way and falls out of status:todo, so it is never double-booked.
    assert_eq!(titles("assignee:me-ai status:todo ready:yes"), vec!["ai-todo"]);
    // Only human-assigned, never me-ai.
    assert_eq!(titles("assignee:me"), vec!["human-mine"]);
    // The status filter picks out what is under way.
    assert_eq!(titles("status:in_progress"), vec!["ai-inprogress"]);
}

/// An assignee reference resolves to a **facet** (human / ai) — the only two subjects a single local store
/// has. It is looked up by reserved word (me/self/human, me-ai/ai) or by the display name in config, and a
/// token that matches neither fails to resolve (exit 1).
#[test]
fn assign_resolves_by_facet_token() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    // whoami carries no account dimension (user_id); the display name comes from the config facet key.
    let who = cli.json(&["whoami", "--json"]);
    assert!(who.get("user_id").is_none(), "user_id is removed");
    assert!(who.get("user_name").is_none(), "user_name was renamed to human_name");
    assert_eq!(who["human_name"], "tester", "the human's display name is config.human_name");

    let pid = cli.a_project();
    let t = id_str(&cli.json(&["task", "add", "--title", "t", "--project", &pid, "--json"])["task"]["id"]);
    // Assign to the human facet by display name.
    cli.run(&["task", "assign", &t, "--to", "tester"]);
    assert_eq!(cli.json(&["task", "show", &t, "--json"])["assignee_kind"], "human", "the display name tester resolves to the human facet");
    // The reserved word me-ai lands on the AI facet.
    cli.run(&["task", "assign", &t, "--to", "me-ai"]);
    assert_eq!(cli.json(&["task", "show", &t, "--json"])["assignee_kind"], "ai", "me-ai resolves to the AI facet");

    // A token that matches no facet does not resolve (exit 1).
    let t2 = id_str(&cli.json(&["task", "add", "--title", "t2", "--project", &pid, "--json"])["task"]["id"]);
    let (stderr, code) = cli.run_err(&["task", "assign", &t2, "--to", "nobody"]);
    assert_eq!(code, 1, "an unknown token does not resolve to a facet: {stderr}");
}

/// Reservation is `task status <id> in_progress` and handing it back is `task status <id> todo`, and
/// reserving is compare-and-swap — a re-reserve of an already-`in_progress` task is rejected
/// (`already_reserved`), not a no-op, so two sessions never double-book. Every other same-status set
/// stays idempotent.
#[test]
fn status_reserves_and_hands_back_and_reserve_is_compare_and_swap() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let t = cli.json(&["task", "add", "--title", "reservable", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    cli.finish_creating(&tid);

    // reserve: todo → in_progress.
    let c = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(c["task"]["status"], "in_progress");

    // re-reserving an already-in_progress task is a CAS conflict, not a no-op.
    let (stderr, code) = cli.run_err(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "re-reserve must fail: {stderr}");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "already_reserved", "re-reserve → already_reserved: {stderr}");
    // The refusal leaves the status at in_progress — no regression.
    let show = cli.json(&["task", "show", &tid, "--json"]);
    assert_eq!(show["status"], "in_progress");

    // hand back: in_progress → todo.
    let r = cli.json(&["task", "status", &tid, "todo", "--actor", "ai", "--json"]);
    assert_eq!(r["task"]["status"], "todo");

    // Once handed back, it can be reserved again (todo → in_progress).
    let re = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(re["task"]["status"], "in_progress");

    // Re-setting any other status to the one it holds stays an idempotent no-op.
    cli.json(&["task", "status", &tid, "todo", "--actor", "ai", "--json"]);
    let r2 = cli.json(&["task", "status", &tid, "todo", "--actor", "ai", "--json"]);
    assert_eq!(r2["noop"], true);
}

/// The reserve also requires `ready`, and the CLI surfaces the refusal the same way
/// it surfaces `already_reserved` — a distinct error code on a non-zero exit, with the hint naming
/// the way out. The two failures must stay distinguishable: `already_reserved` sends you to the next
/// task, `not_ready` sends you to resolve your own declaration. Both premises are exercised (an open
/// blocker and an unsettled linked decision), and each is shown to release the reserve once resolved.
#[test]
fn reserving_a_not_ready_task_is_refused_with_a_way_out() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.bound_project(); // what the AI touches lives in the bound project
    let blocker = cli.json(&["task", "add", "--title", "先行", "--project", &pid, "--json"]);
    let bid = id_str(&blocker["task"]["id"]);
    let t = cli.json(&["task", "add", "--title", "後続", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    cli.finish_creating(&bid);
    cli.finish_creating(&tid);
    cli.json(&["task", "depend", &tid, "--on", &bid, "--json"]);

    // An open blocker: naming the task by number gets you no reserve — the guard is in the write path, not the filter.
    let (stderr, code) = cli.run_err(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "reserve with an open blocker must fail: {stderr}");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "not_ready", "open blocker → not_ready: {stderr}");
    assert!(v["error"]["hint"].as_str().is_some_and(|h| h.contains("undepend")), "hint names the way out: {stderr}");
    let show = cli.json(&["task", "show", &tid, "--json"]);
    assert_eq!(show["status"], "todo", "a rejected reservation does not move the status");

    // Finishing the blocker lets the same reserve through.
    cli.json(&["task", "done", &bid, "--actor", "ai", "--json"]);
    let ok = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(ok["task"]["status"], "in_progress");
    cli.json(&["task", "status", &tid, "todo", "--actor", "ai", "--json"]);

    // An unsettled premise — a linked decision still proposed — is refused with the same code.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "この形にした理由", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    cli.json(&["decision", "link", &did, &tid, "--json"]);
    let (stderr, code) = cli.run_err(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_ne!(code, 0, "reserve on an unsettled premise must fail: {stderr}");
    let v: Value = serde_json::from_str(stderr.trim()).unwrap_or_else(|_| panic!("error JSON: {stderr}"));
    assert_eq!(v["error"]["code"], "not_ready", "unsettled premise → not_ready: {stderr}");

    // Settling it clears the way: accept satisfies the premise, and there is no --force.
    cli.json(&["decision", "accept", &did, "--json"]);
    let ok = cli.json(&["task", "status", &tid, "in_progress", "--actor", "ai", "--json"]);
    assert_eq!(ok["task"]["status"], "in_progress");
}

/// `task show` bundles the four things an agent must read before starting — body, notes, the
/// linked decisions (the "why"), and the timeline — in one command, so none is missed by
/// reading notes alone. The JSON carries `linked_decisions` and `comments` additively.
#[test]
fn task_show_bundles_notes_comments_and_linked_decisions() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "サンプル", "--project", &pid, "--notes", "着手前に読む前提", "--json"]);
    let tid = id_str(&t["task"]["id"]);

    cli.json(&["comment", "add", &tid, "--text", "古いコメント", "--json"]);
    cli.json(&["comment", "add", &tid, "--text", "最新の但し書き", "--json"]);

    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "この形にした理由", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    cli.json(&["decision", "link", &did, &tid, "--json"]);

    let shown = cli.json(&["task", "show", &tid, "--json"]);
    // notes keeps its existing key, as part of the body.
    assert_eq!(shown["notes"], "着手前に読む前提");
    // Of the four: the linked decisions come back by reverse lookup.
    let decisions = shown["linked_decisions"].as_array().expect("linked_decisions array");
    assert_eq!(decisions.len(), 1, "linked decision surfaced inline");
    assert_eq!(decisions[0]["id"], 1, "the id is the decision number");
    assert_eq!(decisions[0]["title"], "この形にした理由");
    // Of the four: the comment bodies come too, not just the count.
    let comments = shown["comments"].as_array().expect("comments array");
    assert_eq!(comments.len(), 2);
    let texts: Vec<&str> = comments.iter().map(|c| c["text"].as_str().unwrap()).collect();
    assert!(texts.contains(&"最新の但し書き") && texts.contains(&"古いコメント"));
}

/// `task show` surfaces dependents (`blocks`) — the reverse of `blocked_by` — so an agent
/// can see what finishing this task would unblock. The category is always signposted: the human output
/// prints `blocks: (none)` when empty (never silently omitted, so the agent cannot mistake "no
/// dependents" for "this category does not exist"), and the JSON carries `blocks` additively.
#[test]
fn task_show_surfaces_dependents_blocks() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let a = cli.json(&["task", "add", "--title", "後続A", "--project", &pid, "--json"]);
    let aid = id_str(&a["task"]["id"]);
    let b = cli.json(&["task", "add", "--title", "先行B", "--project", &pid, "--json"]);
    let bid = id_str(&b["task"]["id"]);
    // A depends on B ⇒ finishing B unblocks A ⇒ B `blocks` A.
    cli.json(&["task", "depend", &aid, "--on", &bid, "--json"]);

    // JSON: B lists A in `blocks`; A has no dependents.
    let b_json = cli.json(&["task", "show", &bid, "--json"]);
    let blocks = b_json["blocks"].as_array().expect("blocks array");
    assert_eq!(blocks.len(), 1, "B blocks exactly A");
    assert_eq!(blocks[0]["name"], "後続A");
    let a_json = cli.json(&["task", "show", &aid, "--json"]);
    assert_eq!(a_json["blocks"].as_array().expect("blocks array").len(), 0, "nothing depends on A");

    // Human: B names the dependent; A signposts the empty category rather than hiding it.
    let (b_human, code) = cli.run(&["task", "show", &bid]);
    assert_eq!(code, 0, "{b_human}");
    assert!(
        b_human.contains("blocks (1):") && b_human.contains("後続A"),
        "B's human output lists the dependent: {b_human}"
    );
    let (a_human, _) = cli.run(&["task", "show", &aid]);
    assert!(
        a_human.contains("blocks: (none)"),
        "A's human output signposts the empty category: {a_human}"
    );
}

/// Every information category in `task show` is always signposted — a bare task with no notes,
/// no blockers, no dependents and no linked decisions still prints each label with `(none)` rather
/// than omitting the line, so an agent cannot mistake "empty" for "this category does not exist".
#[test]
fn task_show_signposts_empty_categories() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "素のタスク", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let (out, code) = cli.run(&["task", "show", &tid]);
    assert_eq!(code, 0, "{out}");
    for marker in ["blocked by: (none)", "blocks: (none)", "notes: (none)", "decisions: (none)"] {
        assert!(out.contains(marker), "missing signpost `{marker}` in:\n{out}");
    }
}

/// `task list --filter commit:<sha>` walks the reverse chain **git → task**: a public commit carries no
/// store-local ref, so the only face back is the SHA recorded on the task. The
/// SHA folds to the bytes the door stored, the same commit on two tasks finds both, and a SHA nobody
/// recorded — a short one included, since the door admits full hex only — is an empty result, not an
/// error (a SHA is a free value, not a name the store knows); only an empty value is refused.
#[test]
fn commit_filter_walks_git_back_to_the_task() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();

    let a_task = |title: &str| -> String {
        id_str(&cli.json(&["task", "add", "--project", &pid, "--title", title, "--json"])["task"]["id"])
    };
    let t1 = a_task("one");
    let t2 = a_task("two");
    let _t3 = a_task("three");

    let sha_a = "a".repeat(40); // SHA-1 form
    let sha_b = "b".repeat(64); // SHA-256 form
    cli.json(&["task", "commit", "add", &t1, &sha_a, "--json"]);
    cli.json(&["task", "commit", "add", &t2, &sha_a, "--json"]); // the same commit on two tasks
    cli.json(&["task", "commit", "add", &t2, &sha_b, "--json"]);

    // Sorted task ids a `commit:` filter returns.
    let ids_for = |sha: &str| -> Vec<String> {
        let mut ids: Vec<String> = cli.json(&["task", "list", "--project", &pid, "--filter", &format!("commit:{sha}"), "--json"])
            ["tasks"]
            .as_array()
            .expect("tasks is an array")
            .iter()
            .map(|t| id_str(&t["id"]))
            .collect();
        ids.sort();
        ids
    };

    let mut both = vec![t1.clone(), t2.clone()];
    both.sort();
    assert_eq!(ids_for(&sha_a), both, "both tasks that recorded the commit come back");
    assert_eq!(ids_for(&sha_b), vec![t2.clone()], "the SHA-256 form finds its one task");
    assert_eq!(ids_for(&sha_a.to_uppercase()), both, "an upper-case SHA folds to the stored lower-case bytes");
    assert!(ids_for(&"c".repeat(40)).is_empty(), "a full SHA nobody recorded is an empty result, not an error");
    assert!(ids_for("abc1234").is_empty(), "a short SHA is never stored, so it simply matches nothing (not rejected)");

    // Only an empty value is no SHA at all — refused (a non-zero exit), unlike an unknown SHA.
    let (_out, code) = cli.run(&["task", "list", "--project", &pid, "--filter", "commit:", "--json"]);
    assert_ne!(code, 0, "an empty commit value is refused, not treated as match-nothing");
}

/// `search` over the word index (`AMB-D-450`), end to end: it reaches every face of a task — its title,
/// its notes, the body of a comment on it — and it folds width, case and kana on both sides, so a word
/// typed one way finds the same word written another.
///
/// Words are `search`'s alone (`AMB-D-449`): the filter grammar carries none, and `--filter` here is the
/// structural narrowing beside them. What is read back is which tasks the words reached, so the subject
/// stays the index rather than the shape of a hit.
///
/// Both paths are exercised on purpose: a term of three characters or more is answered by the trigram
/// index, a shorter one by a scan of the same normalised copy, and neither is allowed to mean something
/// the other does not.
#[test]
fn search_reaches_every_face_of_a_task_and_folds_the_spellings() {
    let cli = Cli::new();
    let pid = cli.a_project();

    let a_task = |title: &str, notes: &str| -> String {
        id_str(&cli.json(&[
            "task", "add", "--project", &pid, "--title", title, "--notes", notes, "--json",
        ])["task"]["id"])
    };
    let titled = a_task("全文検索の索引を張る", "");
    let noted = a_task("別件", "ＡＩ が引く経路の Search");
    let commented = a_task("三件目", "");
    cli.json(&["comment", "add", &commented, "--text", "さーばの設定を直す", "--json"]);

    // The tasks the word reached, read off `search`'s hits: a word may land on several faces of the same
    // task, so the refs are folded back to one id apiece.
    let ids_for = |term: &str| -> Vec<String> {
        let mut ids: Vec<String> = cli.json(&[
            "search", term, "--filter", &format!("project:{pid}"), "--limit", "100", "--json",
        ])["hits"]
            .as_array()
            .expect("hits is an array")
            .iter()
            .map(|h| h["ref"].as_str().expect("a hit names its record").trim_start_matches("AMB-T-").to_string())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };

    // The three faces, each found on its own.
    assert_eq!(ids_for("全文検索"), vec![titled.clone()], "the title");
    assert_eq!(ids_for("引く経路"), vec![noted.clone()], "the notes");
    assert_eq!(ids_for("設定を直す"), vec![commented.clone()], "a comment body");

    // The foldings, on the index path and on the scan path alike.
    assert_eq!(ids_for("SEARCH"), vec![noted.clone()], "case");
    assert_eq!(ids_for("サーバ"), vec![commented.clone()], "kana");
    assert_eq!(ids_for("ai"), vec![noted.clone()], "width and case, on a two-character term");
    assert_eq!(ids_for("検索"), vec![titled.clone()], "a two-character term the index cannot hold");

    // A word nobody wrote is an empty result, and the index is a substring match rather than a bag of
    // triples: the same characters in another order do not match.
    assert!(ids_for("全文一致").is_empty());
    assert!(ids_for("索引全文").is_empty());

    // A rewrite is followed, and so is a deletion.
    cli.json(&["task", "update", &titled, "--title", "番号で引く", "--json"]);
    assert!(ids_for("全文検索").is_empty(), "the old title is no longer a word the task holds");
    assert_eq!(ids_for("番号で引く"), vec![titled.clone()]);
}

/// The word face reaches past the bodies (`AMB-D-450`): a label a person put on the task, and the name
/// of what is attached to it — including what is attached to a comment on it, which is the task's
/// timeline as much as the comment's own words are.
///
/// Neither is text held on the task, so what is under test is the join that makes it the task's: a label
/// nobody placed on this task, and an attachment hanging off another one, must stay out.
#[test]
fn search_reaches_the_labels_and_what_is_attached() {
    let cli = Cli::new();
    let pid = cli.a_project();

    let a_task = |title: &str| -> String {
        id_str(&cli.json(&["task", "add", "--project", &pid, "--title", title, "--json"])["task"]["id"])
    };
    let labelled = a_task("SCENARIO — the one with a label");
    let attached = a_task("SCENARIO — the one with a file");
    let bystander = a_task("SCENARIO — the one with neither");

    cli.json(&["dimension", "add", "--project", &pid, "--name", "エリア", "--json"]);
    cli.json(&["dimension", "value-add", "エリア", "--name", "配色", "--json"]);
    cli.json(&["dimension", "value-add", "エリア", "--name", "実装", "--json"]);
    cli.json(&["dimension", "set", &labelled, "エリア", "配色", "--json"]);

    let file = cli.home.join("計測ログ.md");
    std::fs::write(&file, "# body\n").unwrap();
    cli.json(&["task", "attach", &attached, file.to_str().unwrap(), "--json"]);

    let cid = id_str(&cli.json(&["comment", "add", &attached, "--text", "資料を付けた", "--json"])["comment"]["id"]);
    cli.json(&["comment", "attach", &cid, "https://example.com/benchmark-run", "--url", "--json"]);

    // The tasks the word reached, read off `search`'s hits: a word may land on several faces of the same
    // task, so the refs are folded back to one id apiece.
    let ids_for = |term: &str| -> Vec<String> {
        let mut ids: Vec<String> = cli.json(&[
            "search", term, "--filter", &format!("project:{pid}"), "--limit", "100", "--json",
        ])["hits"]
            .as_array()
            .expect("hits is an array")
            .iter()
            .map(|h| h["ref"].as_str().expect("a hit names its record").trim_start_matches("AMB-T-").to_string())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };

    // The value the task was placed on, and the axis that value belongs to.
    assert_eq!(ids_for("配色"), vec![labelled.clone()], "the label itself");
    assert_eq!(ids_for("エリア"), vec![labelled.clone()], "the axis it is a value of");
    assert!(ids_for("実装").is_empty(), "a value on that axis nobody placed this task on");

    // What is attached: the filename a blob came in under, and the address a link points at.
    assert_eq!(ids_for("計測ログ"), vec![attached.clone()], "an attachment's filename");
    assert_eq!(ids_for("benchmark-run"), vec![attached.clone()], "a link's address, through a comment");

    // The task that carries neither is reached by none of them.
    assert!(!ids_for("配色").contains(&bystander));
    assert!(!ids_for("計測ログ").contains(&bystander));

    // Taking the label off, and the file with it, takes the words away too.
    cli.json(&["dimension", "unset", &labelled, "エリア", "配色", "--json"]);
    assert!(ids_for("配色").is_empty(), "an unset label is no longer one of the task's words");
}
