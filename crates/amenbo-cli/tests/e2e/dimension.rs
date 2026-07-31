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
    let set = cli.json(&["dimension", "set", &tid, "エリア", "設計", "--json"]);
    assert_eq!(set["noop"], false);
    assert_eq!(id_str(&set["task_dimension_value"]["value_id"]), v1id);
    // Setting the same value from another process is an idempotent no-op: it persisted.
    let again = cli.json(&["dimension", "set", &tid, "エリア", "設計", "--json"]);
    assert_eq!(again["noop"], true, "a persisted assignment is a noop on re-set");
    // Single-select: setting another value replaces the one row rather than adding to it.
    let repl = cli.json(&["dimension", "set", &tid, "エリア", "実装", "--json"]);
    assert_eq!(repl["noop"], false);

    // unset clears the assignment, and a second unset is a no-op.
    assert_eq!(cli.json(&["dimension", "unset", &tid, "エリア", "実装", "--json"])["noop"], false);
    assert_eq!(cli.json(&["dimension", "unset", &tid, "エリア", "実装", "--json"])["noop"], true);

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
    cli.json(&["dimension", "unset", &tid, "時代", "現代", "--json"]);
    let cleared = cli.json(&["task", "list", "--filter", "time_axis:現代", "--json"]);
    assert_eq!(cleared["count"], 0, "the default can be cleared");

    // Overriding works too — the time axis is single-select, so it replaces.
    cli.json(&["dimension", "set", &tid, "時代", "黎明期", "--json"]);
    let overridden = cli.json(&["task", "list", "--filter", "time_axis:黎明期", "--json"]);
    assert_eq!(overridden["count"], 1, "it can be overridden to another era");
}
