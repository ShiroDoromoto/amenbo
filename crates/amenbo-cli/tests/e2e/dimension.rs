//! Classification axes: a dimension's lifecycle, filing a task under the axes it names, and the one
//! axis that carries time.

mod harness;

use serde_json::Value;

use harness::*;

/// One lap around the dimension model on the CLI: add an axis, value-add, list/show by name, set/unset on a
/// task (single-select replacement, and a cross-process no-op proving it persisted), rename, cascading rm.
#[test]
fn dimension_lifecycle_axis_values_and_assignment() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "次元PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // A single-select, ordered axis.
    let d = cli.json(&["dimension", "add", "--project", &pid, "--name", "エリア", "--ordered", "--json"]);
    let did = id_str(&d["dimension"]["id"]);
    assert_eq!(d["dimension"]["cardinality"], "single");
    assert_eq!(d["dimension"]["ordered"], true);

    // Two values: the first resolved by axis id, the second by axis name.
    let v1 = cli.json(&["dimension", "value-add", &did, "--name", "設計", "--json"]);
    let v1id = id_str(&v1["dimension_value"]["id"]);
    cli.json(&["dimension", "value-add", "エリア", "--name", "実装", "--json"]);

    // list returns the axis with its values — from another process, so through persistence.
    let list = cli.json(&["dimension", "list", "--project", &pid, "--json"]);
    assert_eq!(list["count"], 1);
    assert_eq!(list["dimensions"][0]["values"].as_array().unwrap().len(), 2);
    // show accepts the name too.
    let show = cli.json(&["dimension", "show", "エリア", "--json"]);
    assert_eq!(id_str(&show["dimension"]["id"]), did);

    // Assign to a task, resolving the value by name within the axis; the task ref needs no project context.
    let t = cli.json(&["task", "add", "--title", "T", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let set = cli.json(&["dimension", "set", &task_ref(&tid), "エリア", "設計", "--json"]);
    assert_eq!(set["noop"], false);
    assert_eq!(id_str(&set["task_dimension_value"]["value_id"]), v1id);
    // Setting the same value from another process is an idempotent no-op: it persisted.
    let again = cli.json(&["dimension", "set", &task_ref(&tid), "エリア", "設計", "--json"]);
    assert_eq!(again["noop"], true, "a persisted assignment is a noop on re-set");
    // Single-select: setting another value replaces the one row rather than adding to it.
    let repl = cli.json(&["dimension", "set", &task_ref(&tid), "エリア", "実装", "--json"]);
    assert_eq!(repl["noop"], false);

    // unset clears the assignment, and a second unset is a no-op.
    assert_eq!(cli.json(&["dimension", "unset", &task_ref(&tid), "エリア", "実装", "--json"])["noop"], false);
    assert_eq!(cli.json(&["dimension", "unset", &task_ref(&tid), "エリア", "実装", "--json"])["noop"], true);

    // Rename the axis: one verb updates it, and a name on its own is a rename.
    let rn = cli.json(&["dimension", "update", &did, "--name", "領域", "--json"]);
    assert_eq!(rn["dimension"]["name"], "領域");
    assert_eq!(rn["changed"], serde_json::json!(["name"]));

    // rm cascades over axis and values, and the listing returns to empty.
    cli.json(&["dimension", "rm", &did, "--yes", "--json"]);
    assert_eq!(cli.json(&["dimension", "list", "--project", &pid, "--json"])["count"], 0);
}

/// `task add --dim <axis>=<value>` files the task under an axis as it is created, saving the
/// create→`dimension set` round trip that is easy to walk away from half-done. What it refuses is refused
/// **before the task exists**: a mistyped name or an axis named twice leaves nothing behind to go and
/// classify by hand.
#[test]
fn task_add_files_the_new_task_under_the_axes_it_names() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "分類PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "区分", "--json"]);
    cli.json(&["dimension", "value-add", "区分", "--name", "バグ", "--json"]);
    cli.json(&["dimension", "value-add", "区分", "--name", "設計", "--json"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "エリア", "--json"]);
    cli.json(&["dimension", "value-add", "エリア", "--name", "コア", "--json"]);

    // Two axes at once. The filter reads it back from another process, so the assignment persisted.
    let t = cli.json(&[
        "task", "add", "--title", "分類つき", "--project", &pid,
        "--dim", "区分=バグ", "--dim", "エリア=コア", "--json",
    ]);
    let tid = id_str(&t["task"]["id"]);
    let filed = cli.json(&["task", "list", "--project", &pid, "--filter", "dim:区分=バグ dim:エリア=コア", "--json"]);
    assert_eq!(filed["count"], 1, "both axes were filed at creation: {filed}");
    assert_eq!(id_str(&filed["tasks"][0]["id"]), tid);

    // An axis holds one value, so naming it twice is refused rather than quietly taking the last —
    // and the task is not created.
    let (out, code) = cli.run(&["task", "add", "--title", "二重", "--project", &pid, "--dim", "区分=バグ", "--dim", "区分=設計", "--json"]);
    assert_ne!(code, 0, "the same axis twice is refused: {out}");
    // A value that names nothing is refused the same way, at the same moment.
    let (bad, bad_code) = cli.run(&["task", "add", "--title", "誤記", "--project", &pid, "--dim", "区分=無い値", "--json"]);
    assert_ne!(bad_code, 0, "an unresolvable value is refused: {bad}");
    let (shape, shape_code) = cli.run(&["task", "add", "--title", "形", "--project", &pid, "--dim", "区分", "--json"]);
    assert_ne!(shape_code, 0, "`<axis>=<value>` is the shape: {shape}");

    let all = cli.json(&["task", "list", "--project", &pid, "--json"]);
    assert_eq!(all["count"], 1, "a refused create leaves no unclassified task behind: {all}");
}

/// `decision add --dim <axis>=<value>` is the same flag on the other classified side (`AMB-D-781`): a
/// decision is classified as it is recorded, so a required axis is filled before `decision accept` reads
/// it and turns the acceptance away. The refusals are the task side's, plus the one this side has of its
/// own — an axis narrowed off decisions (`applies_to`, `AMB-D-789`) classifies nothing here, so it is
/// refused rather than written as a row that means nothing.
#[test]
fn decision_add_files_the_new_decision_under_the_axes_it_names() {
    let cli = Cli::new();
    let pid = id_str(&cli.json(&["project", "add", "--name", "決定分類PJ", "--json"])["project"]["id"]);
    // An axis is born with nothing to answer with, so the demand is raised once it has values.
    cli.json(&["dimension", "add", "--project", &pid, "--name", "テーマ", "--json"]);
    cli.json(&["dimension", "value-add", "テーマ", "--name", "対話ウィンドウ", "--json"]);
    cli.json(&["dimension", "value-add", "テーマ", "--name", "メイン", "--json"]);
    cli.json(&["dimension", "update", "テーマ", "--required", "true", "--json"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "影響半径", "--json"]);
    cli.json(&["dimension", "value-add", "影響半径", "--name", "広い", "--json"]);
    // The axis that runs on tasks alone — what this side has to turn away.
    cli.json(&["dimension", "add", "--project", &pid, "--name", "占有", "--applies-to", "task", "--json"]);
    cli.json(&["dimension", "value-add", "占有", "--name", "iOS", "--json"]);

    // Two axes at once, read back from another process through the decision-side filter.
    let did = id_str(&cli.json(&[
        "decision", "add", "--project", &pid, "--title", "窓をどう建てるか", "--body", "根拠",
        "--dim", "テーマ=対話ウィンドウ", "--dim", "影響半径=広い", "--json",
    ])["decision"]["id"]);
    let filed = cli.json(&["decision", "list", "--project", &pid, "--filter", "dim:テーマ=対話ウィンドウ dim:影響半径=広い", "--json"]);
    assert_eq!(filed["count"], 1, "both axes were filed at creation: {filed}");
    assert_eq!(id_str(&filed["decisions"][0]["id"]), did);

    // The required axis is filled, so the acceptance the empty one would have been turned away for goes
    // through — and the response said nothing was left to fill in.
    let (accepted, code) = cli.run(&["decision", "accept", &decision_ref(&did), "--json"]);
    assert_eq!(code, 0, "a decision classified at creation accepts straight away: {accepted}");

    // Recorded with the required axis left blank: written all the same, and the response names the axis
    // rather than leaving the writer to find out when somebody else presses accept.
    let blank = cli.json(&["decision", "add", "--project", &pid, "--title", "分類なし", "--body", "根拠", "--json"]);
    assert_eq!(
        blank["decision"]["unmet_required_dimensions"],
        serde_json::json!(["テーマ"]),
        "the create names what is still blank: {blank}"
    );
    let (said, code) = cli.run(&["decision", "add", "--project", &pid, "--title", "分類なし2", "--body", "根拠"]);
    assert_eq!(code, 0, "naming it is not refusing it: {said}");
    assert!(said.contains("テーマ"), "the human face names the axis too: {said}");
    assert!(said.contains("--dim"), "and the way to fill it in: {said}");

    // Nothing left blank, nothing said — the field is absent rather than an empty list, so a reader
    // testing for it is testing for something to do.
    assert!(
        cli.json(&["decision", "add", "--project", &pid, "--title", "分類あり", "--body", "根拠", "--dim", "テーマ=メイン", "--json"])
            ["decision"]["unmet_required_dimensions"]
            .is_null(),
        "a decision with every demand answered is told nothing",
    );

    // An axis narrowed off decisions: refused, naming the axis and the side rather than the value.
    let (err, code) = cli.run_err(&["decision", "add", "--project", &pid, "--title", "レーン", "--body", "根拠", "--dim", "占有=iOS"]);
    assert_ne!(code, 0, "an axis that does not classify decisions is refused: {err}");
    assert!(err.contains("占有"), "the refusal names the axis: {err}");
    assert!(err.contains("decisions"), "and the side it does not classify: {err}");

    // The task side's refusals, on this side too — an axis named twice, an unresolvable value, a bare axis.
    let before = cli.json(&["decision", "list", "--project", &pid, "--json"])["count"].clone();
    for bad in [
        vec!["--dim", "テーマ=対話ウィンドウ", "--dim", "テーマ=メイン"],
        vec!["--dim", "テーマ=無い値"],
        vec!["--dim", "テーマ"],
    ] {
        let mut argv = vec!["decision", "add", "--project", &pid, "--title", "断られる", "--body", "根拠", "--json"];
        argv.extend(bad.iter().copied());
        let (out, code) = cli.run(&argv);
        assert_ne!(code, 0, "refused before the decision exists: {out}");
    }
    let all = cli.json(&["decision", "list", "--project", &pid, "--json"]);
    assert_eq!(all["count"], before, "a refused create leaves no unclassified decision behind: {all}");
}

/// The other door that records a decision. `decision promote` raises one out of a comment, and it
/// leaves the same gap `decision add` did: the demand is read where the decision is settled, by
/// somebody else. So the response names the axes still blank here too — and names one way in rather
/// than two, since a promotion carries no `--dim` to send anyone to.
#[test]
fn promoting_a_comment_names_the_required_axes_too() {
    let cli = Cli::new();
    let pid = id_str(&cli.json(&["project", "add", "--name", "昇格PJ", "--json"])["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "テーマ", "--json"]);
    cli.json(&["dimension", "value-add", "テーマ", "--name", "メイン", "--json"]);
    cli.json(&["dimension", "update", "テーマ", "--required", "true", "--json"]);

    let tid = id_str(&cli.json(&["task", "add", "--project", &pid, "--title", "土台", "--json"])["task"]["id"]);
    let cid = id_str(&cli.json(&["comment", "add", &task_ref(&tid), "--text", "UTC で保存する", "--json"])["comment"]["id"]);

    let response = cli.json(&[
        "decision", "promote", &format!("AMB-TC-{cid}"), "--title", "保存はUTC", "--json",
    ]);
    let promoted = &response["decision"];
    assert_eq!(
        promoted["unmet_required_dimensions"],
        serde_json::json!(["テーマ"]),
        "the promotion names what is still blank: {promoted}"
    );

    // The human face says it too, and points at the one command that fills it in — a promotion has no
    // flag of its own to offer.
    let (said, code) = cli.run(&["decision", "promote", &format!("AMB-TC-{cid}"), "--title", "保存はUTC2"]);
    assert_eq!(code, 0, "naming it is not refusing it: {said}");
    assert!(said.contains("テーマ"), "the axis is named: {said}");
    assert!(said.contains("dimension set"), "and the way to fill it in: {said}");
    assert!(!said.contains("--dim"), "but not a flag this command does not have: {said}");
}

/// Only values on a time axis (role: time_axis) carry a period `[start_on, end_on]`. value-add /
/// value-update write it, list / show print it for humans, and dates on any other axis are turned away by
/// the CLI gatekeeper (core just writes the columns) — all across processes, so persistence is included.
#[test]
fn time_axis_values_carry_a_period_and_other_axes_reject_dates() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "期間PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "時代", "--ordered", "--time-axis", "--json"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "エリア", "--json"]);

    // One closed period, and one with an open end — still running.
    let closed = cli.json(&["dimension", "value-add", "時代", "--name", "開発期", "--start", "2026-06-20", "--end", "2026-07-07", "--json"]);
    assert_eq!(closed["dimension_value"]["start_on"], "2026-06-20");
    assert_eq!(closed["dimension_value"]["end_on"], "2026-07-07");
    let open = cli.json(&["dimension", "value-add", "時代", "--name", "運用第1期", "--start", "2026-07-08", "--json"]);
    assert_eq!(open["dimension_value"]["end_on"], Value::Null, "omit the end and it is ongoing");

    // A value with no period carries no dates; that is the value-add default.
    let plain = cli.json(&["dimension", "value-add", "エリア", "--name", "設計", "--json"]);
    assert_eq!(plain["dimension_value"]["start_on"], Value::Null);

    // Human output: only values with a period print one, and an open end reads as ongoing.
    let (shown, code) = cli.run(&["dimension", "show", "時代"]);
    assert_eq!(code, 0, "{shown}");
    assert!(shown.contains("[2026-06-20 → 2026-07-07]"), "shows a closed period: {shown}");
    assert!(shown.contains("[2026-07-08 → ongoing]"), "an open end shows ongoing: {shown}");
    let (listed, _) = cli.run(&["dimension", "list", "--project", &pid]);
    assert!(listed.contains("[2026-06-20 → 2026-07-07]"), "list also shows the period: {listed}");
    assert!(!listed.contains("設計  ["), "a value with no period shows no brackets: {listed}");

    // value-update touches only the fields it is given — a rename keeps the period.
    let renamed = cli.json(&["dimension", "value-update", "時代", "開発期", "--name", "黎明期", "--json"]);
    assert_eq!(renamed["dimension_value"]["name"], "黎明期");
    assert_eq!(renamed["dimension_value"]["end_on"], "2026-07-07", "renaming does not clear the period");
    assert_eq!(renamed["changed"], serde_json::json!(["name"]));

    // Close the end, then open it again.
    let closed_now = cli.json(&["dimension", "value-update", "時代", "運用第1期", "--end", "2026-12-31", "--json"]);
    assert_eq!(closed_now["dimension_value"]["end_on"], "2026-12-31");
    let reopened = cli.json(&["dimension", "value-update", "時代", "運用第1期", "--clear-end", "--json"]);
    assert_eq!(reopened["dimension_value"]["end_on"], Value::Null, "--clear-end makes it ongoing again");
    assert_eq!(reopened["dimension_value"]["start_on"], "2026-07-08", "opening the end keeps the start");

    // An inverted period is refused by core.
    let (err, code) = cli.run_err(&["dimension", "value-update", "時代", "黎明期", "--start", "2026-08-01", "--json"]);
    assert_ne!(code, 0, "start > end is rejected: {err}");

    // Dates on a non-time axis are turned away by the CLI gatekeeper, on value-add and value-update alike.
    let (err, code) = cli.run_err(&["dimension", "value-add", "エリア", "--name", "実装", "--start", "2026-07-08", "--json"]);
    assert_ne!(code, 0, "value-add on a non-time-axis rejects dates: {err}");
    let (err, code) = cli.run_err(&["dimension", "value-update", "エリア", "設計", "--clear-end", "--json"]);
    assert_ne!(code, 0, "value-update on a non-time-axis rejects period ops: {err}");
    // The refusal added no value: the gatekeeper stands before the write.
    let vals = cli.json(&["dimension", "show", "エリア", "--json"]);
    assert_eq!(vals["values"].as_array().unwrap().len(), 1);
}

/// An existing axis can be named the time axis after the fact, and unnamed again (`dimension update
/// --time-axis`). That naming *is* the date gatekeeper: the axis refuses dates, then takes them, then refuses again.
#[test]
fn dimension_update_names_and_unnames_the_time_axis() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "指名PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    // An axis created with no role refuses dates.
    cli.json(&["dimension", "add", "--project", &pid, "--name", "時代", "--ordered", "--json"]);
    let (err, code) = cli.run_err(&["dimension", "value-add", "時代", "--name", "開発期", "--start", "2026-06-20", "--json"]);
    assert_ne!(code, 0, "before designation, dates are rejected: {err}");

    // Once named it takes periods, and the current era is settled — new tasks default to it.
    let named = cli.json(&["dimension", "update", "時代", "--time-axis", "true", "--json"]);
    assert_eq!(named["dimension"]["role"], "time_axis");
    assert_eq!(named["changed"], serde_json::json!(["role"]));
    cli.json(&["dimension", "value-add", "時代", "--name", "運用第1期", "--start", "2026-07-08", "--json"]);
    let (shown, _) = cli.run(&["dimension", "show", "時代"]);
    assert!(shown.contains("[2026-07-08 → ongoing]"), "after designation, the period is shown: {shown}");

    // Unnaming it makes dates refused again; the dates already stored stay, but mean nothing.
    let unnamed = cli.json(&["dimension", "update", "時代", "--time-axis", "false", "--json"]);
    assert_eq!(unnamed["dimension"]["role"], "none");
    let (err, code) = cli.run_err(&["dimension", "value-update", "時代", "運用第1期", "--clear-start", "--json"]);
    assert_ne!(code, 0, "clear the designation and dates are rejected: {err}");
    let vals = cli.json(&["dimension", "show", "時代", "--json"]);
    assert_eq!(vals["values"][0]["start_on"], "2026-07-08", "the dates remain in the physical columns");
}

/// Whether an axis goes on the task card is the axis's own answer (`AMB-D-651`), so this face raises it
/// at creation or afterwards, lowers it again, and says so wherever an axis is read.
#[test]
fn dimension_marks_and_unmarks_an_axis_for_the_task_card() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "カードPJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    // Raised at creation. An axis raised with nothing said about it starts down.
    let up = cli.json(&["dimension", "add", "--project", &pid, "--name", "エリア", "--show-on-card", "--json"]);
    assert_eq!(up["dimension"]["show_on_card"], true);
    let down = cli.json(&["dimension", "add", "--project", &pid, "--name", "種別", "--json"]);
    assert_eq!(down["dimension"]["show_on_card"], false);

    // Raised after the fact, and the envelope names the field that moved.
    let marked = cli.json(&["dimension", "update", "種別", "--show-on-card", "true", "--json"]);
    assert_eq!(marked["dimension"]["show_on_card"], true);
    assert_eq!(marked["changed"], serde_json::json!(["show_on_card"]));

    // Both faces that read an axis say which way it stands.
    let (shown, _) = cli.run(&["dimension", "show", "種別"]);
    assert!(shown.contains("show-on-card"), "show says the axis is marked: {shown}");
    let (listed, _) = cli.run(&["dimension", "list", "--project", &pid]);
    assert!(listed.contains("エリア (d1) [single, show-on-card]"), "list says it too: {listed}");

    // Lowered again, and nothing else on the axis moved with it.
    let cleared = cli.json(&["dimension", "update", "種別", "--show-on-card", "false", "--json"]);
    assert_eq!(cleared["dimension"]["show_on_card"], false);
    assert_eq!(cleared["dimension"]["name"], "種別");
    let (after, _) = cli.run(&["dimension", "show", "種別"]);
    assert!(!after.contains("show-on-card"), "and it stops saying so: {after}");
}

/// Which side an axis classifies is the axis's own answer (`AMB-D-789`), and this face sets it at
/// creation or afterwards, refuses a value it does not know, and says so wherever an axis is read —
/// from the wide side, so only a narrowed axis is written about.
#[test]
fn dimension_narrows_which_side_of_the_store_an_axis_classifies() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "対象PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    // An axis raised with nothing said about it classifies both — the opposite of the flags that
    // start down.
    let wide = cli.json(&["dimension", "add", "--project", &pid, "--name", "顧客", "--json"]);
    assert_eq!(wide["dimension"]["applies_to"], "both");
    // Narrowed at creation.
    let narrow = cli.json(&[
        "dimension", "add", "--project", &pid, "--name", "占有", "--applies-to", "task", "--json",
    ]);
    assert_eq!(narrow["dimension"]["applies_to"], "task");

    // Narrowed after the fact, and the envelope names the field that moved.
    let moved = cli.json(&["dimension", "update", "顧客", "--applies-to", "decision", "--json"]);
    assert_eq!(moved["dimension"]["applies_to"], "decision");
    assert_eq!(moved["changed"], serde_json::json!(["applies_to"]));

    // Both faces that read an axis say which side it is offered on, and say nothing about the wide one.
    let (shown, _) = cli.run(&["dimension", "show", "占有"]);
    assert!(shown.contains("tasks only"), "show names the side: {shown}");
    let (listed, _) = cli.run(&["dimension", "list", "--project", &pid]);
    assert!(listed.contains("顧客 (d1) [single, decisions only]"), "list says it too: {listed}");
    cli.json(&["dimension", "update", "顧客", "--applies-to", "both", "--json"]);
    let (back, _) = cli.run(&["dimension", "show", "顧客"]);
    assert!(!back.contains("only"), "widened again, it stops saying anything: {back}");

    // A side nobody defined is refused by name, with what the flag takes.
    let (err, code) = cli.run_err(&["dimension", "update", "顧客", "--applies-to", "task,decision"]);
    assert_ne!(code, 0);
    assert!(err.contains("task | decision | both"), "the refusal lists what it takes: {err}");
}

/// An axis can demand an answer (`AMB-D-734`), and this face raises it, says so wherever an axis is read,
/// and reports the one refusal it causes: a creation that cannot be finished while the axis is blank.
#[test]
fn dimension_demands_an_answer_and_the_creation_is_held_until_it_gets_one() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "必須PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // An axis is born with no values, so it cannot be born demanding one.
    let (err, code) = cli.run_err(&["dimension", "add", "--project", &pid, "--name", "プロダクト", "--required"]);
    assert_ne!(code, 0, "a new axis has nothing to answer with: {err}");

    let axis = cli.json(&["dimension", "add", "--project", &pid, "--name", "プロダクト", "--json"]);
    assert_eq!(axis["dimension"]["required"], false, "an axis raised plainly demands nothing");
    cli.json(&["dimension", "value-add", "プロダクト", "--name", "本体", "--json"]);

    // Raised after the fact, and the envelope names the field that moved.
    let raised = cli.json(&["dimension", "update", "プロダクト", "--required", "true", "--json"]);
    assert_eq!(raised["dimension"]["required"], true);
    assert_eq!(raised["changed"], serde_json::json!(["required"]));

    // Both faces that read an axis say which way it stands.
    let (shown, _) = cli.run(&["dimension", "show", "プロダクト"]);
    assert!(shown.contains("required"), "show says the axis demands an answer: {shown}");
    let (listed, _) = cli.run(&["dimension", "list", "--project", &pid]);
    assert!(listed.contains("プロダクト (d1) [single, required]"), "list says it too: {listed}");

    // A task filed with the axis blank cannot finish its creation, and the refusal names the axis.
    let t = cli.json(&["task", "add", "--project", &pid, "--title", "分類のないタスク", "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let (err, code) = cli.run_err(&["task", "finish-creating", &tid]);
    assert_ne!(code, 0, "the creation is held: {err}");
    assert!(err.contains("プロダクト"), "and the axis is named: {err}");
    assert!(err.contains("dimension set"), "and the hint says how to answer it: {err}");

    // The same refusal in --json carries the code a caller can branch on.
    let (refused, code) = cli.run_err(&["task", "finish-creating", &tid, "--json"]);
    assert_ne!(code, 0);
    let refused: serde_json::Value = serde_json::from_str(&refused).expect("the refusal is JSON");
    assert_eq!(refused["error"]["code"], "invalid_task_required_dimension");

    // Answer the axis and the creation goes through.
    cli.json(&["dimension", "set", &task_ref(&tid), "プロダクト", "本体", "--json"]);
    let done = cli.json(&["task", "finish-creating", &tid, "--json"]);
    assert_eq!(done["task"]["draft"], false);

    // And a required axis cannot be blanked back out.
    let (err, code) = cli.run_err(&["dimension", "unset", &task_ref(&tid), "プロダクト", "本体"]);
    assert_ne!(code, 0, "a required axis cannot be cleared: {err}");

    // Lowering the flag reopens both doors.
    cli.json(&["dimension", "update", "プロダクト", "--required", "false", "--json"]);
    let (after, _) = cli.run(&["dimension", "show", "プロダクト"]);
    assert!(!after.contains("required"), "and it stops saying so: {after}");
}

/// The decision side of the required-classification door (`AMB-D-790`): a decision cannot be settled
/// while an axis the project requires of decisions is blank, `supersede` reads the same door because it
/// settles too, and which side a required axis holds is the axis's own `applies_to` to say
/// (`AMB-D-789`) — so a task-only one lets an acceptance through and a decision-only one lets a
/// creation through.
#[test]
fn a_required_axis_holds_an_acceptance_on_the_side_it_classifies() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "決定必須PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    let axis = cli.json(&["dimension", "add", "--project", &pid, "--name", "影響半径", "--json"]);
    cli.json(&["dimension", "value-add", "影響半径", "--name", "この一箇所", "--json"]);
    cli.json(&["dimension", "update", "影響半径", "--required", "true", "--json"]);
    assert_eq!(axis["dimension"]["applies_to"], "both", "raised plainly, it classifies both sides");

    // A decision recorded with the axis blank cannot be settled, and the refusal names the axis and
    // the way to answer it.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "分類のない決定", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let (err, code) = cli.run_err(&["decision", "accept", &did]);
    assert_ne!(code, 0, "the acceptance is held: {err}");
    assert!(err.contains("影響半径"), "and the axis is named: {err}");
    assert!(err.contains("dimension set"), "and the hint says how to answer it: {err}");

    // The same refusal in --json carries the code a caller can branch on, and nothing was settled.
    let (refused, code) = cli.run_err(&["decision", "accept", &did, "--json"]);
    assert_ne!(code, 0);
    let refused: serde_json::Value = serde_json::from_str(&refused).expect("the refusal is JSON");
    assert_eq!(refused["error"]["code"], "invalid_decision_required_dimension");
    assert_eq!(cli.json(&["decision", "show", &did, "--json"])["status"], "proposed");

    // `supersede` settles the new side too, so it meets the same door.
    let old = cli.json(&["decision", "add", "--project", &pid, "--title", "覆される決定", "--json"]);
    let old_id = id_str(&old["decision"]["id"]);
    cli.json(&["dimension", "set", &decision_ref(&old_id), "影響半径", "この一箇所", "--json"]);
    cli.json(&["decision", "accept", &old_id, "--json"]);
    let (err, code) = cli.run_err(&["decision", "supersede", &did, "--replaces", &old_id]);
    assert_ne!(code, 0, "promoting through supersede is still settling: {err}");
    assert!(err.contains("影響半径"), "and the same axis is named: {err}");

    // Answer the axis and both roads go through.
    cli.json(&["dimension", "set", &decision_ref(&did), "影響半径", "この一箇所", "--json"]);
    assert_eq!(cli.json(&["decision", "accept", &did, "--json"])["decision"]["status"], "accepted");

    // Which side the flag holds is the axis's own to say. A task-only required axis asks nothing of a
    // decision...
    let task_only =
        cli.json(&["dimension", "add", "--project", &pid, "--name", "占有", "--applies-to", "task", "--json"]);
    let task_only_id = id_str(&task_only["dimension"]["id"]);
    cli.json(&["dimension", "value-add", "占有", "--name", "iOS", "--json"]);
    cli.json(&["dimension", "update", "占有", "--required", "true", "--json"]);
    let free = cli.json(&["decision", "add", "--project", &pid, "--title", "レーンを問われない決定", "--json"]);
    let free_id = id_str(&free["decision"]["id"]);
    cli.json(&["dimension", "set", &decision_ref(&free_id), "影響半径", "この一箇所", "--json"]);
    assert_eq!(
        cli.json(&["decision", "accept", &free_id, "--json"])["decision"]["status"],
        "accepted",
        "an axis that classifies only tasks holds no acceptance",
    );

    // ...and a decision-only one asks nothing of a task.
    cli.json(&["dimension", "update", &task_only_id, "--applies-to", "decision", "--json"]);
    let t = cli.json(&["task", "add", "--project", &pid, "--title", "レーンを問われないタスク", "--json"]);
    let tid = id_str(&t["task"]["id"]);
    cli.json(&["dimension", "set", &task_ref(&tid), "影響半径", "この一箇所", "--json"]);
    assert_eq!(
        cli.json(&["task", "finish-creating", &tid, "--json"])["task"]["draft"],
        false,
        "an axis that classifies only decisions holds no creation",
    );
}

/// The readable key an axis and its values are named by outside Amenbo (`AMB-D-735`): what a row is
/// born with when nobody names one, what `--slug` puts there instead, where both faces show it, and
/// that a reference resolves by it — including the one case that says which tier wins, where a slug
/// spells another axis's name.
#[test]
fn a_slug_is_the_readable_key_an_axis_and_its_values_answer_to() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "鍵PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    // Nobody names one, so the key comes off the id — the display name is Japanese and yields nothing.
    let plain = cli.json(&["dimension", "add", "--project", &pid, "--name", "製品", "--json"]);
    let plain_id = id_str(&plain["dimension"]["id"]);
    assert_eq!(plain["dimension"]["slug"], format!("d{plain_id}"));

    // Named at the door instead, on the axis and on a value.
    let axis = cli.json(&["dimension", "add", "--project", &pid, "--name", "フェーズ", "--slug", "phase", "--json"]);
    let did = id_str(&axis["dimension"]["id"]);
    assert_eq!(axis["dimension"]["slug"], "phase");
    let v = cli.json(&["dimension", "value-add", "phase", "--name", "運用第2期", "--slug", "ops2", "--json"]);
    let vid = id_str(&v["dimension_value"]["id"]);
    assert_eq!(v["dimension_value"]["slug"], "ops2");
    let derived = cli.json(&["dimension", "value-add", "phase", "--name", "運用第3期", "--json"]);
    assert_eq!(derived["dimension_value"]["slug"], format!("v{}", id_str(&derived["dimension_value"]["id"])));

    // Both faces put the key beside the name, so what a person would type is on the line they read.
    let (shown, _) = cli.run(&["dimension", "show", "phase"]);
    assert!(shown.contains("フェーズ (phase)"), "show names the axis by both: {shown}");
    assert!(shown.contains("運用第2期 (ops2)"), "and its values too: {shown}");
    let (listed, _) = cli.run(&["dimension", "list", "--project", &pid]);
    assert!(listed.contains("フェーズ (phase)"), "list names the axis by both: {listed}");
    assert!(listed.contains("運用第2期 (ops2)"), "and its values too: {listed}");

    // Renaming a key is one field of the same door, and the envelope says which field moved.
    let renamed = cli.json(&["dimension", "update", "phase", "--slug", "era", "--json"]);
    assert_eq!(renamed["dimension"]["slug"], "era");
    assert_eq!(renamed["changed"], serde_json::json!(["slug"]));
    let value_renamed = cli.json(&["dimension", "value-update", "era", "ops2", "--slug", "ops-2", "--json"]);
    assert_eq!(value_renamed["dimension_value"]["slug"], "ops-2");
    assert_eq!(value_renamed["changed"], serde_json::json!(["slug"]));

    // A key the door will not take, and one somebody else already answers to.
    let (refused, code) = cli.run_err(&["dimension", "update", "era", "--slug", "Bad", "--json"]);
    assert_ne!(code, 0);
    let refused: Value = serde_json::from_str(&refused).expect("the refusal is JSON");
    assert_eq!(refused["error"]["code"], "invalid_dimension_slug_shape");
    let (taken, code) = cli.run_err(&["dimension", "update", &plain_id, "--slug", "era", "--json"]);
    assert_ne!(code, 0);
    let taken: Value = serde_json::from_str(&taken).expect("the refusal is JSON");
    assert_eq!(taken["error"]["code"], "invalid_dimension_slug_taken");

    // **id → slug → name.** This axis is *named* what the other one's key says, and the key still wins;
    // the id wins over both.
    let decoy = cli.json(&["dimension", "add", "--project", &pid, "--name", "era", "--json"]);
    let decoy_id = id_str(&decoy["dimension"]["id"]);
    assert_eq!(id_str(&cli.json(&["dimension", "show", "era", "--json"])["dimension"]["id"]), did);
    assert_eq!(id_str(&cli.json(&["dimension", "show", &decoy_id, "--json"])["dimension"]["id"]), decoy_id);
    // The same order one axis down: a value named what another value's key says.
    cli.json(&["dimension", "value-add", "era", "--name", "ops-2", "--json"]);
    let filed = id_str(&cli.json(&["task", "add", "--title", "T", "--project", &pid, "--json"])["task"]["id"]);
    let hit = cli.json(&["dimension", "set", &task_ref(&filed), "era", "ops-2", "--json"]);
    assert_eq!(id_str(&hit["task_dimension_value"]["value_id"]), vid);
}

/// A name a filter can reach (`AMB-D-819`). `--filter` is cut on whitespace before it is cut on `:`, so
/// `dim:<axis>=<name>` can never be written for a name holding any — the id and the key would still
/// reach it, but the one thing a person remembers would not. The door refuses it at all four places a
/// name is written, and what gets through is a name the filter finds.
#[test]
fn a_name_holding_whitespace_is_refused_so_a_filter_can_still_reach_it() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "空白PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);

    let refused = |args: &[&str]| {
        let (out, code) = cli.run_err(args);
        assert_ne!(code, 0, "{args:?} is refused");
        let out: Value = serde_json::from_str(&out).expect("the refusal is JSON");
        assert_eq!(out["error"]["code"], "invalid_dimension_name_whitespace", "{args:?}");
    };

    // The axis, at creation and at rename. The second is the full-width space a Japanese keyboard
    // reaches for, which `split_whitespace` parts on exactly as it parts on U+0020.
    refused(&["dimension", "add", "--project", &pid, "--name", "リリース 分類", "--json"]);
    let axis = cli.json(&["dimension", "add", "--project", &pid, "--name", "リリース", "--json"]);
    let did = id_str(&axis["dimension"]["id"]);
    refused(&["dimension", "update", &did, "--name", "リリース\u{3000}分類", "--json"]);

    // And the value, at both of its doors.
    refused(&["dimension", "value-add", &did, "--name", "日本語の 表記を揃える", "--json"]);
    cli.json(&["dimension", "value-add", &did, "--name", "日本語の表記を揃える", "--json"]);
    refused(&["dimension", "value-update", &did, "日本語の表記を揃える", "--name", "日本語の 表記", "--json"]);

    // Which is what the door is for: the name that got through is one `dim:` can name.
    cli.json(&[
        "task", "add", "--title", "T", "--project", &pid,
        "--dim", "リリース=日本語の表記を揃える", "--json",
    ]);
    let listed = cli.json(&["task", "list", "--filter", "dim:リリース=日本語の表記を揃える", "--json"]);
    assert_eq!(listed["count"], 1, "the name reaches its tasks through the filter");
}

/// A new task defaults to the era on its project's time axis that **covers today** — automation, not a
/// requirement. With no era over today it is created unassigned, and the default can be cleared or overridden.
#[test]
fn task_add_defaults_to_the_time_axis_value_covering_today() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "時代PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "時代", "--ordered", "--time-axis", "--json"]);

    // While only a past window exists, no default is applied — creation is not refused.
    cli.json(&["dimension", "value-add", "時代", "--name", "黎明期", "--start", "2000-01-01", "--end", "2000-12-31", "--json"]);
    cli.json(&["task", "add", "--title", "窓の外", "--project", &pid, "--json"]);
    let outside = cli.json(&["task", "list", "--filter", "time_axis:黎明期", "--json"]);
    assert_eq!(outside["count"], 0, "with no era covering today, nothing is assigned");

    // Add an ongoing era (an open end) and new tasks pick it up by default.
    cli.json(&["dimension", "value-add", "時代", "--name", "現代", "--start", "2000-01-01", "--json"]);
    let t = cli.json(&["task", "add", "--title", "既定つき", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let current = cli.json(&["task", "list", "--filter", "time_axis:現代", "--json"]);
    assert_eq!(current["count"], 1, "the current era is assigned by default at creation");
    assert_eq!(id_str(&current["tasks"][0]["id"]), tid);

    // The default is not mandatory: it can be cleared.
    cli.json(&["dimension", "unset", &task_ref(&tid), "時代", "現代", "--json"]);
    let cleared = cli.json(&["task", "list", "--filter", "time_axis:現代", "--json"]);
    assert_eq!(cleared["count"], 0, "the default can be cleared");

    // Overriding works too — the time axis is single-select, so it replaces.
    cli.json(&["dimension", "set", &task_ref(&tid), "時代", "黎明期", "--json"]);
    let overridden = cli.json(&["task", "list", "--filter", "time_axis:黎明期", "--json"]);
    assert_eq!(overridden["count"], 1, "it can be overridden to another era");
}

/// A decision is filed on the very same axes a task is, with the very same values (`AMB-D-781`) — and the
/// target has to say which of the two it is, because they number independently.
#[test]
fn a_decision_is_filed_on_the_same_axes_and_the_target_names_its_kind() {
    let cli = Cli::new();
    let pid = id_str(&cli.json(&["project", "add", "--name", "分類PJ", "--json"])["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "テーマ", "--json"]);
    cli.json(&["dimension", "value-add", "テーマ", "--name", "メイン", "--json"]);
    cli.json(&["dimension", "value-add", "テーマ", "--name", "対話", "--json"]);

    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "真実源を RDB にする", "--body", "根拠", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    // Filed by ref, and the row that lands is the decision's own.
    let set = cli.json(&["dimension", "set", &decision_ref(&did), "テーマ", "メイン", "--json"]);
    assert_eq!(set["noop"], false);
    assert_eq!(id_str(&set["decision_dimension_value"]["decision_id"]), did);
    // From another process — so it persisted — the same value again is a no-op, and a different value on
    // the same axis replaces rather than adds (single-select holds on this side too).
    assert_eq!(cli.json(&["dimension", "set", &decision_ref(&did), "テーマ", "メイン", "--json"])["noop"], true);
    cli.json(&["dimension", "set", &decision_ref(&did), "テーマ", "対話", "--json"]);

    // The decision's own page says what it is filed under, on both faces.
    let shown = cli.json(&["decision", "show", &did, "--json"]);
    let dims = shown["dimensions"].as_array().expect("the page carries the classification");
    assert_eq!(dims.len(), 1, "single-select: one value per axis, the replacement included");
    assert_eq!(dims[0]["dimension"], "テーマ");
    assert_eq!(dims[0]["value"], "対話");
    let (human, _) = cli.run(&["decision", "show", &did]);
    assert!(human.contains("dimensions: テーマ=対話"), "and the human page says it too: {human}");

    // A task filed on the same value is a row of its own — one axis, two kinds of thing on it.
    let tid = id_str(&cli.json(&["task", "add", "--title", "実装", "--project", &pid, "--json"])["task"]["id"]);
    cli.json(&["dimension", "set", &task_ref(&tid), "テーマ", "対話", "--json"]);
    assert_eq!(cli.json(&["task", "show", &tid, "--json"])["dimensions"][0]["value"], "対話");
    assert_eq!(cli.json(&["decision", "show", &did, "--json"])["dimensions"][0]["value"], "対話");

    // unset clears the decision's value and is a no-op the second time.
    assert_eq!(cli.json(&["dimension", "unset", &decision_ref(&did), "テーマ", "対話", "--json"])["noop"], false);
    assert_eq!(cli.json(&["dimension", "unset", &decision_ref(&did), "テーマ", "対話", "--json"])["noop"], true);
}

/// The number alone does not say which kind it names, so it is refused rather than guessed at — and the
/// refusal spells both readings out.
#[test]
fn a_bare_number_is_refused_where_it_could_mean_either_kind() {
    let cli = Cli::new();
    let pid = id_str(&cli.json(&["project", "add", "--name", "分類PJ", "--json"])["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "テーマ", "--json"]);
    cli.json(&["dimension", "value-add", "テーマ", "--name", "メイン", "--json"]);
    let tid = id_str(&cli.json(&["task", "add", "--title", "T", "--project", &pid, "--json"])["task"]["id"]);

    let (err, code) = cli.run_err(&["dimension", "set", &tid, "テーマ", "メイン"]);
    assert_ne!(code, 0, "a bare number is refused: {err}");
    assert!(err.contains(&format!("AMB-T-{tid}")), "the refusal spells the task reading: {err}");
    assert!(err.contains(&format!("AMB-D-{tid}")), "and the decision reading: {err}");
    // The same on the way back out.
    let (err, code) = cli.run_err(&["dimension", "unset", &tid, "テーマ", "メイン"]);
    assert_ne!(code, 0, "unset asks the same question: {err}");
}

/// What a required axis holds on the decision side is the **acceptance** (`AMB-D-790`), not the
/// assignment — so a decision's value on one still clears, where a task's is refused. The two differ
/// because the task's door is one-directional and would never ask again, while a decision stripped of a
/// required value is simply one that has to answer for it the next time it is settled.
#[test]
fn a_required_axis_still_lets_a_decisions_value_be_cleared() {
    let cli = Cli::new();
    let pid = id_str(&cli.json(&["project", "add", "--name", "分類PJ", "--json"])["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "プロダクト", "--json"]);
    cli.json(&["dimension", "value-add", "プロダクト", "--name", "本体", "--json"]);
    cli.json(&["dimension", "update", "プロダクト", "--required", "true", "--json"]);

    let did = id_str(&cli.json(&["decision", "add", "--project", &pid, "--title", "決定", "--body", "根拠", "--json"])["decision"]["id"]);
    cli.json(&["dimension", "set", &decision_ref(&did), "プロダクト", "本体", "--json"]);
    assert_eq!(
        cli.json(&["dimension", "unset", &decision_ref(&did), "プロダクト", "本体", "--json"])["noop"],
        false,
        "a decision's value on a required axis clears"
    );
}

/// The names of the classification are a way **in** to a decision, not only a way to narrow a listing:
/// `search` reaches the value a decision is filed under and the axis behind it, exactly as it does on the
/// task side, and the row it comes back on says what the record is filed under.
///
/// One value, worn by a task and by a decision, is what makes the two halves separable here: a search
/// narrowed to decisions must answer with the decision alone, and the same search narrowed to tasks with
/// the task alone — a single set of arms answering for both would show it by answering twice.
#[test]
fn search_reaches_the_classification_a_decision_is_filed_under() {
    let cli = Cli::new();
    let pid = id_str(&cli.json(&["project", "add", "--name", "分類検索PJ", "--json"])["project"]["id"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "テーマ", "--json"]);
    cli.json(&["dimension", "value-add", "テーマ", "--name", "対話ウィンドウ", "--json"]);

    let tid = id_str(
        &cli.json(&["task", "add", "--project", &pid, "--title", "窓の設計", "--dim", "テーマ=対話ウィンドウ", "--json"])
            ["task"]["id"],
    );
    let did = id_str(
        &cli.json(&["decision", "add", "--project", &pid, "--title", "窓をどう閉じるか", "--body", "根拠", "--json"])
            ["decision"]["id"],
    );
    cli.json(&["dimension", "set", &decision_ref(&did), "テーマ", "対話ウィンドウ", "--json"]);

    // The refs a word reaches on the label face, on the side asked for.
    let refs_for = |word: &str, kind: &str| -> Vec<String> {
        cli.json(&["search", word, "--kind", kind, "--face", "label", "--limit", "100", "--json"])["hits"]
            .as_array()
            .expect("hits is an array")
            .iter()
            .map(|h| h["ref"].as_str().expect("a hit names its record").to_string())
            .collect()
    };

    assert_eq!(refs_for("対話ウィンドウ", "decision"), vec![format!("AMB-D-{did}")], "the value's name");
    assert_eq!(refs_for("テーマ", "decision"), vec![format!("AMB-D-{did}")], "the axis behind it");
    assert_eq!(
        refs_for("対話ウィンドウ", "task"),
        vec![format!("AMB-T-{tid}")],
        "and the same value on the other side stays the task's"
    );

    // The row says what the record it points at is filed under, the way the filter takes it back.
    let hit = &cli.json(&["search", "対話ウィンドウ", "--kind", "decision", "--json"])["hits"][0];
    assert_eq!(hit["standing"]["labels"][0]["axis"], "テーマ");
    assert_eq!(hit["standing"]["labels"][0]["value"], "対話ウィンドウ");
}

/// The refusal `AMB-D-789` asks for, read off the binary rather than off the resolver: an axis narrowed
/// to one side narrows there and is turned away on the other, in every face that takes a `dim:`. The
/// empty page it would otherwise answer with says two things at once — "nothing carries that value" and
/// "that axis does not run here" — and only the second is a question worth correcting.
///
/// `time_axis:` walks it too. The role names an axis; it does not exempt one, so the sugar meets the
/// same guard as the name it stands for.
#[test]
fn a_dim_on_the_side_its_axis_does_not_classify_is_refused_by_every_face() {
    let cli = Cli::new();
    let pid = id_str(&cli.json(&["project", "add", "--name", "対象PJ", "--json"])["project"]["id"]);

    // A task-only axis, and a both-sided time axis to measure the refusal against.
    cli.json(&["dimension", "add", "--project", &pid, "--name", "占有", "--applies-to", "task", "--json"]);
    cli.json(&["dimension", "value-add", "占有", "--name", "iOS", "--json"]);
    cli.json(&["dimension", "add", "--project", &pid, "--name", "時代", "--ordered", "--json"]);
    cli.json(&["dimension", "update", "時代", "--time-axis", "true", "--json"]);
    cli.json(&["dimension", "value-add", "時代", "--name", "運用第1期", "--start", "2026-07-08", "--json"]);

    let tid = id_str(
        &cli.json(&["task", "add", "--project", &pid, "--title", "レーンの上", "--dim", "占有=iOS", "--json"])["task"]["id"],
    );
    let did = id_str(
        &cli.json(&["decision", "add", "--project", &pid, "--title", "どのレーンで焼くか", "--body", "根拠", "--json"])
            ["decision"]["id"],
    );
    cli.json(&["dimension", "set", &decision_ref(&did), "時代", "運用第1期", "--json"]);

    // The side the axis classifies is untouched — the half the refusal must not cost.
    let listed = cli.json(&["task", "list", "--filter", "dim:占有=iOS", "--json"]);
    assert_eq!(listed["count"], 1);
    assert_eq!(id_str(&listed["tasks"][0]["id"]), tid);

    // The side it does not classify: refused, naming the axis and the side rather than the value.
    let (err, code) = cli.run_err(&["decision", "list", "--filter", "dim:占有=iOS"]);
    assert_ne!(code, 0, "the decision face is turned away: {err}");
    assert!(err.contains("占有"), "the refusal names the axis: {err}");
    assert!(err.contains("decisions"), "and the side it does not classify: {err}");
    // `=none` is the arm where the empty page would have been *every* row instead.
    let (err, code) = cli.run_err(&["decision", "list", "--filter", "dim:占有=none"]);
    assert_ne!(code, 0, "`=none` is refused the same way: {err}");

    // The third face that takes the grammar. `search --kind` says which side it is asking about, so it
    // meets the same guard — a word plus a `dim:` the side does not carry is still the wrong question.
    let (err, code) = cli.run_err(&["search", "レーン", "--kind", "decision", "--filter", "dim:占有=iOS"]);
    assert_ne!(code, 0, "search narrowed to decisions is turned away too: {err}");
    assert!(err.contains("占有"), "and names the axis: {err}");
    // Narrowed to the side the axis does run on, the same search goes through.
    assert!(cli.json(&["search", "レーン", "--kind", "task", "--filter", "dim:占有=iOS", "--json"])["hits"]
        .as_array()
        .is_some_and(|h| !h.is_empty()));

    // `time_axis:` is the axis by its role, and the role is no exemption: narrow the axis off tasks and
    // the sugar is refused there while the decision side it still classifies keeps answering.
    cli.json(&["dimension", "update", "時代", "--applies-to", "decision", "--json"]);
    let (err, code) = cli.run_err(&["task", "list", "--filter", "time_axis:運用第1期"]);
    assert_ne!(code, 0, "the sugar meets the same guard: {err}");
    assert!(err.contains("time axis"), "the refusal names the role it was asked by: {err}");
    assert!(err.contains("tasks"), "and the side it does not classify: {err}");
    let by_role = cli.json(&["decision", "list", "--filter", "time_axis:運用第1期", "--json"]);
    assert_eq!(by_role["count"], 1, "the side it does classify still answers by role");
    assert_eq!(id_str(&by_role["decisions"][0]["id"]), did);
}
