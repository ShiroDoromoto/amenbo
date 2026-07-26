//! The CLI driver's reusable core: drive the **shipped / installed**
//! `amenbo` binary against one scenario in an isolated throwaway store, and judge the asserts
//! from that binary's `--json` output. A black-box driver — it knows the domain vocabulary, not
//! the build under test.
//!
//! Two bins sit on top of this: `verify-cli` runs one scenario, `verify-all` runs a whole set
//! and aggregates. Both call [`run_scenario`] and read a [`Report`]; the isolation, the driver
//! and the reporting live here so the two share one contract.

/// The throwaway store an amenbo run is given. Public because every bin in this crate needs it:
/// asking the shipped binary anything at all means giving it a home that is not the user's.
pub mod scratch;

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use amenbo_scenario::{Args, Domain, Scenario, Step};

/// Load, isolate, execute, judge — one scenario against one binary. `Err` is an execution error
/// (the scenario would not load, the binary would not run); a scenario that ran but had a failing
/// assert comes back as an `Ok(Report)` with `passed == false`.
pub fn run_scenario(scenario: &Scenario, bin: &Path, keep: bool) -> Result<Report, String> {
    let session = scratch::session(&scenario.id, keep)
        .map_err(|e| format!("could not create a throwaway store: {e}"))?;
    let mut driver = Driver::new(bin, session)?;

    let mut report = Report::new(scenario);
    for (i, step) in scenario.steps.iter().enumerate() {
        let outcome = driver.exec(step)?; // an execution error aborts this scenario's run
        report.push(i, step, outcome);
    }
    Ok(report)
}

/// Drives the shipped binary against one isolated store, remembering the ids that steps bind.
struct Driver {
    bin: std::path::PathBuf,
    session: scratch::Session,
    project_id: i64,
    bindings: HashMap<String, i64>,
}

impl Driver {
    /// Boot a fresh store: `init` creates it and hands back the project every `task add` needs.
    fn new(bin: &Path, session: scratch::Session) -> Result<Driver, String> {
        let mut d = Driver { bin: bin.to_path_buf(), session, project_id: 0, bindings: HashMap::new() };
        let v = d.run_json(&["init", "--name", "verify", "--json"])?;
        d.project_id = v["identity"]["project_id"]
            .as_i64()
            .ok_or("init did not report a project_id")?;
        Ok(d)
    }

    /// Run `amenbo <args>` in the isolated store and parse its `--json` output. An `Err` is an
    /// execution failure (spawn failed, non-JSON output, non-zero exit, or an `error` object) —
    /// distinct from an assert that ran cleanly and came out false.
    fn run_json(&self, args: &[&str]) -> Result<serde_json::Value, String> {
        // The facet goes on the command line, which is the one input amenbo is to take it by; a call
        // that names its own is left alone.
        let mut with_facet = args.to_vec();
        if !args.contains(&"--actor") {
            with_facet.extend_from_slice(&["--actor", "human"]);
        }
        let out = Command::new(&self.bin)
            .args(&with_facet)
            .current_dir(&self.session.cwd)
            .env("AMENBO_HOME", &self.session.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("NO_COLOR", "1")
            .output()
            .map_err(|e| format!("could not run `{}`: {e}", self.bin.display()))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let v: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|e| {
            format!("`amenbo {}` did not print JSON ({e}); output was:\n{}", args.join(" "), stdout.trim())
        })?;
        if !out.status.success() || v.get("error").is_some() {
            return Err(format!("`amenbo {}` failed: {}", args.join(" "), stdout.trim()));
        }
        Ok(v)
    }

    /// Map one step to a binary call. Returns whether an assert passed (an action always passes
    /// unless it errors), plus a human note.
    fn exec(&mut self, step: &Step) -> Result<Outcome, String> {
        match step {
            Step::Action { domain, op, with, bind } => {
                self.action(*domain, op, with, bind.as_deref())
            }
            Step::Assert { domain, op, with } => self.assert(*domain, op, with),
        }
    }

    fn action(&mut self, domain: Domain, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match (domain, op) {
            (Domain::Task, "create") => {
                let title = req_str(with, "title")?;
                let pid = self.project_id.to_string();
                let v = self.run_json(&["task", "add", "--title", title, "--project", &pid, "--json"])?;
                let id = v["task"]["id"].as_i64().ok_or("task add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("created task {id} `{title}`")))
            }
            (Domain::Task, "assign") => {
                let target = self.resolve(with)?;
                let assignee = req_str(with, "assignee")?;
                self.run_json(&["task", "assign", &target.to_string(), "--to", assignee, "--json"])?;
                Ok(Outcome::action(format!("assigned task {target} to {assignee}")))
            }
            (Domain::Task, "comment") => {
                let target = self.resolve(with)?;
                let text = req_str(with, "text")?;
                let v = self.run_json(&["comment", "add", &target.to_string(), "--text", text, "--json"])?;
                let id = v["comment"]["id"].as_i64().ok_or("comment add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("commented on task {target}")))
            }
            (Domain::Comment, "edit") => {
                let target = self.resolve(with)?;
                let text = req_str(with, "text")?;
                self.run_json(&["comment", "edit", &target.to_string(), "--text", text, "--json"])?;
                Ok(Outcome::action(format!("rewrote comment {target}")))
            }
            (Domain::Comment, "rm") => {
                let target = self.resolve(with)?;
                self.run_json(&["comment", "rm", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("deleted comment {target}")))
            }
            (Domain::Comment, "promote") => {
                let target = self.resolve(with)?;
                let title = req_str(with, "title")?;
                let v = self.run_json(&["decision", "promote", &target.to_string(), "--title", title, "--json"])?;
                let id = v["decision"]["id"].as_i64().ok_or("decision promote did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("promoted comment {target} into decision {id}")))
            }
            (Domain::Task, "status") => {
                let target = self.resolve(with)?;
                let status = req_str(with, "status")?;
                // The move is refused rather than silently ignored (a reserve that is not from todo
                // comes back `already_reserved`), and `run_json` reads that non-zero exit as an
                // execution error — so a scenario that walks the states out of order says so.
                self.run_json(&["task", "status", &target.to_string(), status, "--json"])?;
                Ok(Outcome::action(format!("moved task {target} to {status}")))
            }
            (Domain::Task, "done") => {
                let target = self.resolve(with)?;
                self.run_json(&["task", "done", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("marked task {target} done")))
            }
            (Domain::Task, "reopen") => {
                let target = self.resolve(with)?;
                self.run_json(&["task", "reopen", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("reopened task {target}")))
            }
            (Domain::Task, "block") => {
                let target = self.resolve(with)?;
                let reason = req_str(with, "reason")?;
                self.run_json(&["task", "block", &target.to_string(), "--reason", reason, "--json"])?;
                Ok(Outcome::action(format!("blocked task {target}: {reason}")))
            }
            (Domain::Task, "update") => {
                let target = self.resolve(with)?;
                let id = target.to_string();
                let mut args: Vec<String> = vec!["task".into(), "update".into(), id];
                // Only the fields the step names are sent, so one op covers "set a due date" and
                // "retitle and reprioritise" without a scenario spelling out the command line.
                for key in ["title", "notes", "due", "start", "priority"] {
                    if let Some(v) = with.get(key) {
                        let v = v.as_str().ok_or_else(|| format!("arg `{key}` must be a string"))?;
                        args.push(format!("--{key}"));
                        args.push(v.to_string());
                    }
                }
                if args.len() == 3 {
                    return Err("`update` names no field to set".to_string());
                }
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("updated task {target}")))
            }
            (Domain::Task, "clear") => {
                let target = self.resolve(with)?;
                let field = req_str(with, "field")?;
                let flag = format!("--clear-{field}");
                self.run_json(&["task", "update", &target.to_string(), &flag, "--json"])?;
                Ok(Outcome::action(format!("cleared `{field}` on task {target}")))
            }
            (Domain::Task, "move") => {
                let target = self.resolve(with)?;
                let id = target.to_string();
                let mut args: Vec<String> = vec!["task".into(), "move".into(), id];
                if with.contains_key("project") {
                    args.push("--project".into());
                    args.push(self.resolve_key(with, "project")?.to_string());
                }
                // Where in the project it lands: the same command carries the re-home and the order.
                if let Some(pos) = with.get("position").and_then(|v| v.as_str()) {
                    match pos {
                        "top" | "bottom" => args.push(format!("--{pos}")),
                        other => return Err(format!("`position: {other}` is not top or bottom")),
                    }
                }
                if args.len() == 3 {
                    return Err("`move` names neither a project nor a position".to_string());
                }
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("moved task {target}")))
            }
            (Domain::Project, "create") => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["project", "add", "--name", name, "--json"])?;
                let id = v["project"]["id"].as_i64().ok_or("project add did not report an id")?;
                if let Some(b) = bind {
                    self.bindings.insert(b.to_string(), id);
                }
                Ok(Outcome::action(format!("created project {id} `{name}`")))
            }
            (Domain::Project, "update") => {
                let target = self.resolve(with)?;
                let id = target.to_string();
                let mut args: Vec<String> = vec!["project".into(), "update".into(), id];
                for key in ["name", "notes", "view"] {
                    if let Some(v) = with.get(key) {
                        let v = v.as_str().ok_or_else(|| format!("arg `{key}` must be a string"))?;
                        args.push(format!("--{key}"));
                        args.push(v.to_string());
                    }
                }
                if args.len() == 3 {
                    return Err("`update` names no field to set".to_string());
                }
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("updated project {target}")))
            }
            (Domain::Project, "move") => {
                let target = self.resolve(with)?;
                let pos = req_str(with, "position")?;
                let flag = match pos {
                    "top" | "bottom" => format!("--{pos}"),
                    other => return Err(format!("`position: {other}` is not top or bottom")),
                };
                self.run_json(&["project", "move", &target.to_string(), &flag, "--json"])?;
                Ok(Outcome::action(format!("moved project {target} to the {pos}")))
            }
            (Domain::Project, verb @ ("archive" | "unarchive")) => {
                let target = self.resolve(with)?;
                self.run_json(&["project", verb, &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("{verb}d project {target}")))
            }
            (Domain::Dimension, "create") => {
                let name = req_str(with, "name")?;
                let pid = self.project_id.to_string();
                self.run_json(&["dimension", "add", "--name", name, "--project", &pid, "--json"])?;
                Ok(Outcome::action(format!("defined the axis `{name}`")))
            }
            (Domain::Dimension, "value-add") => {
                let dimension = req_str(with, "dimension")?;
                let value = req_str(with, "value")?;
                self.run_json(&["dimension", "value-add", dimension, "--name", value, "--json"])?;
                Ok(Outcome::action(format!("added the value `{value}` to `{dimension}`")))
            }
            // Filing a task under an axis and taking it back off. The axis and value go by name, which
            // is what the command takes — a bare number there would be read as a name, not an id.
            (Domain::Dimension, verb @ ("set" | "unset")) => {
                let target = self.resolve(with)?;
                let dimension = req_str(with, "dimension")?;
                let value = req_str(with, "value")?;
                self.run_json(&["dimension", verb, &target.to_string(), dimension, value, "--json"])?;
                let note = match verb {
                    "set" => format!("filed task {target} under `{dimension}` = `{value}`"),
                    _ => format!("took task {target} out of `{dimension}` = `{value}`"),
                };
                Ok(Outcome::action(note))
            }
            (Domain::Task, "depend") => {
                let target = self.resolve(with)?;
                let on = self.resolve_key(with, "on")?;
                self.run_json(&["task", "depend", &target.to_string(), "--on", &on.to_string(), "--json"])?;
                Ok(Outcome::action(format!("task {target} now waits on task {on}")))
            }
            (Domain::Task, "undepend") => {
                let target = self.resolve(with)?;
                let on = self.resolve_key(with, "on")?;
                self.run_json(&["task", "undepend", &target.to_string(), "--on", &on.to_string(), "--json"])?;
                Ok(Outcome::action(format!("task {target} no longer waits on task {on}")))
            }
            (Domain::Task, "commit-add") => {
                let target = self.resolve(with)?;
                let sha = req_str(with, "sha")?;
                self.run_json(&["task", "commit", "add", &target.to_string(), sha, "--json"])?;
                Ok(Outcome::action(format!("recorded commit {sha} on task {target}")))
            }
            (Domain::Task, "commit-rm") => {
                let target = self.resolve(with)?;
                let sha = req_str(with, "sha")?;
                // A hard delete asks first; the driver is unattended, so it answers up front.
                self.run_json(&["task", "commit", "rm", &target.to_string(), sha, "--yes", "--json"])?;
                Ok(Outcome::action(format!("forgot commit {sha} on task {target}")))
            }
            (Domain::Decision, "create") => {
                let title = req_str(with, "title")?;
                let pid = self.project_id.to_string();
                let v = self.run_json(&["decision", "add", "--title", title, "--project", &pid, "--json"])?;
                let id = v["decision"]["id"].as_i64().ok_or("decision add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("created decision {id} `{title}`")))
            }
            (Domain::Decision, "edit") => {
                let target = self.resolve(with)?;
                let body = req_str(with, "body")?;
                self.run_json(&["decision", "edit", &target.to_string(), "--body", body, "--json"])?;
                Ok(Outcome::action(format!("edited the body of decision {target}")))
            }
            (Domain::Decision, "accept") => {
                let target = self.resolve(with)?;
                self.run_json(&["decision", "accept", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("accepted decision {target}")))
            }
            (Domain::Decision, "comment") => {
                let target = self.resolve(with)?;
                let text = req_str(with, "text")?;
                let v = self.run_json(&["decision", "comment", "add", &target.to_string(), "--text", text, "--json"])?;
                let id = v["comment"]["id"].as_i64().ok_or("decision comment add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("commented on decision {target}")))
            }
            (Domain::Decision, "comment-edit") => {
                let target = self.resolve(with)?;
                let text = req_str(with, "text")?;
                self.run_json(&["decision", "comment", "edit", &target.to_string(), "--text", text, "--json"])?;
                Ok(Outcome::action(format!("rewrote decision comment {target}")))
            }
            (Domain::Decision, "comment-rm") => {
                let target = self.resolve(with)?;
                self.run_json(&["decision", "comment", "rm", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("deleted decision comment {target}")))
            }
            (Domain::Decision, "link") => {
                let target = self.resolve(with)?;
                let task = self.resolve_key(with, "task")?;
                self.run_json(&["decision", "link", &target.to_string(), &task.to_string(), "--json"])?;
                Ok(Outcome::action(format!("linked decision {target} to task {task}")))
            }
            (Domain::Decision, "supersede") => {
                let target = self.resolve(with)?;
                let old = self.resolve_key(with, "replaces")?;
                // The command accepts the new decision on the way, so a scenario that supersedes
                // does not accept it first — it would be refused as already settled.
                self.run_json(&["decision", "supersede", &target.to_string(), "--replaces", &old.to_string(), "--json"])?;
                Ok(Outcome::action(format!("decision {target} replaces decision {old}")))
            }
            (Domain::Decision, "builds-on") => {
                let target = self.resolve(with)?;
                let base = self.resolve_key(with, "on")?;
                self.run_json(&["decision", "builds-on", &target.to_string(), "--on", &base.to_string(), "--json"])?;
                Ok(Outcome::action(format!("decision {target} stands on decision {base}")))
            }
            (Domain::Decision, "unlink") => {
                let target = self.resolve(with)?;
                let other = self.resolve_key(with, "from")?;
                self.run_json(&["decision", "unlink", &target.to_string(), "--from", &other.to_string(), "--json"])?;
                Ok(Outcome::action(format!("decision {target} no longer points at decision {other}")))
            }
            _ => Err(unmapped(domain, op)),
        }
    }

    fn assert(&self, domain: Domain, op: &str, with: &Args) -> Result<Outcome, String> {
        match (domain, op) {
            (Domain::Task, "listed") => {
                let target = self.resolve(with)?;
                let filter = req_str(with, "filter")?;
                let v = self.run_json(&["task", "list", "--filter", filter, "--json"])?;
                let rows = v["tasks"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                judge_listing("task", target, &format!("`{filter}`"), rows, with)
            }
            (Domain::Project, "listed") => {
                let target = self.resolve(with)?;
                // The archived ones are a listing of their own: they leave the everyday list, which is
                // what archiving is for, so proving one went means asking both.
                let archived = opt_bool(with, "archived").unwrap_or(false);
                let mut args = vec!["project", "list"];
                if archived {
                    args.push("--archived");
                }
                args.push("--json");
                let v = self.run_json(&args)?;
                let rows = v["projects"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let listing =
                    if archived { "the listing that carries archived projects" } else { "the project listing" };
                judge_listing("project", target, listing, rows, with)
            }
            (Domain::Project, "field") => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["project", "show", &target.to_string(), "--json"])?;
                judge_field("project", target, with, &v)
            }
            (Domain::Dimension, "listed") => {
                let dimension = req_str(with, "dimension")?;
                let value = with.get("value").and_then(|v| v.as_str());
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["dimension", "list", "--json"])?;
                let axis = v["dimensions"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .find(|d| d["dimension"]["name"].as_str() == Some(dimension));
                // Without a `value` the question is whether the axis is defined at all; with one it is
                // whether the axis carries that value, since a value is only ever read through its axis.
                let found = match (axis, value) {
                    (None, _) => false,
                    (Some(_), None) => true,
                    (Some(a), Some(want)) => a["values"]
                        .as_array()
                        .map(|vs| vs.iter().any(|v| v["name"].as_str() == Some(want)))
                        .unwrap_or(false),
                };
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "axis `{dimension}`{} {} defined (expected {}, {})",
                        value.map(|v| format!(" value `{v}`")).unwrap_or_default(),
                        if found { "is" } else { "is not" },
                        if present { "defined" } else { "gone" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Task, "status-bucket") => {
                let target = self.resolve(with)?;
                let bucket = req_str(with, "bucket")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["status", "--json"])?;
                // A bucket the view does not print is a scenario bug, not an empty bucket: answering
                // "absent" there would pass a line that asks about nothing.
                let Some(rows) = v.get(bucket).and_then(|b| b.as_array()) else {
                    return Err(format!("`bucket: {bucket}` is not a list the status view prints"));
                };
                let found = rows.iter().any(|t| t["id"].as_i64() == Some(target));
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "task {target} {} the `{bucket}` bucket of the status view ({} there, expected {}, {})",
                        if found { "is in" } else { "is not in" },
                        rows.len(),
                        if present { "in it" } else { "out of it" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Task, "field") => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["task", "show", &target.to_string(), "--json"])?;
                judge_field("task", target, with, &v)
            }
            (Domain::Task, "commit") => {
                let target = self.resolve(with)?;
                let sha = req_str(with, "sha")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["task", "commit", "list", &target.to_string(), "--json"])?;
                let found = v["commits"]
                    .as_array()
                    .map(|a| a.iter().any(|c| c["sha"].as_str() == Some(sha)))
                    .unwrap_or(false);
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "task {target} {} commit {sha} (expected {}, {})",
                        if found { "records" } else { "does not record" },
                        if present { "recorded" } else { "not recorded" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Task, "commented") => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["comment", "list", &target.to_string(), "--json"])?;
                judge_timeline("task", target, with, &v["comments"])
            }
            (Domain::Decision, "commented") => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["decision", "comment", "list", &target.to_string(), "--json"])?;
                judge_timeline("decision", target, with, &v["comments"])
            }
            (Domain::Task, "activity") => {
                let target = self.resolve(with)?;
                let id = target.to_string();
                let mut args: Vec<String> =
                    vec!["activity".into(), "--task".into(), id, "--json".into()];
                // `kind` narrows the stream the way a reader does — the system events one way, what
                // people wrote the other — so a scenario can ask which side an entry landed on.
                if let Some(kind) = with.get("kind").and_then(|v| v.as_str()) {
                    args.push("--kind".into());
                    args.push(kind.to_string());
                }
                let v = self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                judge_timeline("task", target, with, &v["items"])
            }
            (Domain::Decision, "field") => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["decision", "show", &target.to_string(), "--json"])?;
                judge_field("decision", target, with, &v)
            }
            (Domain::Decision, "listed") => {
                let target = self.resolve(with)?;
                let filter = req_str(with, "filter")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["decision", "list", "--filter", filter, "--json"])?;
                let found = v["decisions"]
                    .as_array()
                    .map(|rows| rows.iter().any(|d| d["id"].as_i64() == Some(target)))
                    .unwrap_or(false);
                let pass = found == present;
                let word = if present { "present in" } else { "absent from" };
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "decision {target} {} `{filter}` (expected {word}, {})",
                        if found { "is present in" } else { "is absent from" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Decision, "edge") => {
                let target = self.resolve(with)?;
                let other = self.resolve_key(with, "other")?;
                let kind = req_str(with, "kind")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["decision", "show", &target.to_string(), "--json"])?;
                // A named side that the output does not carry is a mismatch rather than an error,
                // for the reason a `field` path that runs off it is: the scenario is asserting about
                // the shape of the shipped output as much as about what it holds.
                let Some(rows) = v[kind].as_array() else {
                    return Ok(Outcome::assert(
                        false,
                        format!("decision {target} has no `{kind}` side in its show output (MISMATCH)"),
                    ));
                };
                let found = rows.iter().any(|d| d["id"].as_i64() == Some(other));
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "decision {target} {} decision {other} under `{kind}` (expected {}, {})",
                        if found { "points at" } else { "does not point at" },
                        if present { "an edge" } else { "none" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(domain, op)),
        }
    }

    /// Resolve a step's `target:` to the id an earlier action bound. The loader already proved
    /// the name resolves to an earlier `as:`, so a miss here is an internal error, not user input.
    fn resolve(&self, with: &Args) -> Result<i64, String> {
        self.resolve_key(with, "target")
    }

    /// The same, for an op that names a second object under its own key (`decision link`'s `task`).
    fn resolve_key(&self, with: &Args, key: &str) -> Result<i64, String> {
        let name = req_str(with, key)?;
        self.bindings
            .get(name)
            .copied()
            .ok_or_else(|| format!("internal: binding `{name}` was never produced"))
    }
}

/// The outcome of one step.
struct Outcome {
    /// An assert's verdict; an action is always `true` unless it errored out of `exec`.
    pass: bool,
    note: String,
}

impl Outcome {
    fn action(note: String) -> Outcome {
        Outcome { pass: true, note }
    }
    fn assert(pass: bool, note: String) -> Outcome {
        Outcome { pass, note }
    }
}

/// Judge a listing assert: is the row in this listing, and — when the step names a `position` — is it
/// where the order says it is. `position` is the only place a reorder shows: order is the store's to
/// keep and the key it keeps it by is opaque, so where a row sits is all a reader can ask about.
/// `listing` names the listing in the note, since a row can be absent from one and present in another
/// (the archived projects being exactly that).
fn judge_listing(
    noun: &str,
    id: i64,
    listing: &str,
    rows: &[serde_json::Value],
    with: &Args,
) -> Result<Outcome, String> {
    let at = rows.iter().position(|r| r["id"].as_i64() == Some(id));
    let found = at.is_some();
    if let Some(want) = with.get("position").and_then(|v| v.as_str()) {
        let last = rows.len().saturating_sub(1);
        let (pass, seen) = match (want, at) {
            ("first", Some(i)) => (i == 0, i),
            ("last", Some(i)) => (i == last, i),
            (other, Some(_)) => return Err(format!("`position: {other}` is not first or last")),
            (_, None) => (false, 0),
        };
        return Ok(Outcome::assert(
            pass,
            format!(
                "{noun} {id} sits at {} of {} in {listing} (expected {want}, {})",
                if found { seen.to_string() } else { "nowhere".to_string() },
                rows.len(),
                if pass { "as expected" } else { "MISMATCH" }
            ),
        ));
    }
    let present = opt_bool(with, "present").unwrap_or(true);
    let pass = found == present;
    Ok(Outcome::assert(
        pass,
        format!(
            "{noun} {id} {} {listing} (expected {}, {})",
            if found { "is present in" } else { "is absent from" },
            if present { "present" } else { "absent" },
            if pass { "as expected" } else { "MISMATCH" }
        ),
    ))
}

/// Judge a timeline assert: does this stream of entries carry the wording the step names? With no
/// `text` the question is only whether the stream has anything in it at all, which is what a
/// narrowed activity read (a `kind`) is asked. `present: false` is the same question in reverse —
/// the proof a comment that was deleted is really gone.
fn judge_timeline(noun: &str, id: i64, with: &Args, entries: &serde_json::Value) -> Result<Outcome, String> {
    let want = with.get("text").and_then(|v| v.as_str());
    let present = opt_bool(with, "present").unwrap_or(true);
    let rows = entries.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let found = match want {
        Some(text) => rows.iter().any(|c| c["text"].as_str() == Some(text)),
        None => !rows.is_empty(),
    };
    let pass = found == present;
    Ok(Outcome::assert(
        pass,
        format!(
            "{noun} {id} timeline {} {} ({} entries, expected {}, {})",
            if found { "carries" } else { "does not carry" },
            want.map(|t| format!("`{t}`")).unwrap_or_else(|| "an entry".to_string()),
            rows.len(),
            if present { "carried" } else { "gone" },
            if pass { "as expected" } else { "MISMATCH" }
        ),
    ))
}

/// Read a field out of a `show --json` object, following a dotted path into what the output nests:
/// `placement.project.name` walks two objects, `blocked_by.0.name` indexes an array on the way. A
/// path that runs off the output is `None`, which the caller reports as a mismatch — a scenario
/// naming a path is asserting about the shape of the shipped output as much as about the value.
fn dig<'a>(shown: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut here = shown;
    for step in path.split('.') {
        here = match step.parse::<usize>() {
            Ok(i) => here.get(i)?,
            Err(_) => here.get(step)?,
        };
    }
    Some(here)
}

/// Judge a `field` assert against the object's own `show --json`. `equals` is any scalar (string /
/// bool / number / null), compared structurally against the field's JSON value, so `status: todo`
/// and `completed: false` both work — and a field the output does not carry at all is a mismatch,
/// not an error, since a scenario naming one is asserting about the shipped output's shape too.
fn judge_field(noun: &str, id: i64, with: &Args, shown: &serde_json::Value) -> Result<Outcome, String> {
    let field = req_str(with, "field")?;
    let expected = with.get("equals").ok_or("arg `equals` is required")?;
    let expected = serde_json::to_value(expected)
        .map_err(|e| format!("arg `equals` is not a valid value: {e}"))?;
    match dig(shown, field) {
        None => Ok(Outcome::assert(
            false,
            format!("{noun} {id} has no field `{field}` in its show output (MISMATCH)"),
        )),
        Some(actual) => {
            let pass = *actual == expected;
            Ok(Outcome::assert(
                pass,
                format!(
                    "{noun} {id} field `{field}` = {actual} (expected {expected}, {})",
                    if pass { "as expected" } else { "MISMATCH" }
                ),
            ))
        }
    }
}

fn unmapped(domain: Domain, op: &str) -> String {
    format!(
        "op `{op}` for domain `{domain:?}` is in the scenario registry but not yet mapped in the CLI driver"
    )
}

fn req_str<'a>(with: &'a Args, key: &str) -> Result<&'a str, String> {
    with.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("arg `{key}` must be a string"))
}

fn opt_bool(with: &Args, key: &str) -> Option<bool> {
    with.get(key).and_then(|v| v.as_bool())
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

/// The verdict of one scenario run: the ordered per-step lines plus the roll-up. `passed` is the
/// AND of every step, so a runner reads it directly to aggregate across scenarios.
pub struct Report {
    scenario_id: String,
    title: String,
    steps: Vec<Line>,
    passed: bool,
}

struct Line {
    index: usize,
    kind: &'static str,
    pass: bool,
    note: String,
}

impl Report {
    fn new(s: &Scenario) -> Report {
        Report { scenario_id: s.id.clone(), title: s.title.clone(), steps: Vec::new(), passed: true }
    }

    fn push(&mut self, index: usize, step: &Step, outcome: Outcome) {
        let kind = match step {
            Step::Action { .. } => "action",
            Step::Assert { .. } => "assert",
        };
        if !outcome.pass {
            self.passed = false;
        }
        self.steps.push(Line { index, kind, pass: outcome.pass, note: outcome.note });
    }

    /// Did every step pass?
    pub fn passed(&self) -> bool {
        self.passed
    }

    /// The scenario's stable id.
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    /// The scenario's human title.
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn print_human(&self) {
        println!("scenario: {} — {}", self.scenario_id, self.title);
        for l in &self.steps {
            let mark = if l.kind == "action" {
                "·"
            } else if l.pass {
                "✓"
            } else {
                "✗"
            };
            println!("  {mark} step {} [{}] {}", l.index + 1, l.kind, l.note);
        }
        println!("VERDICT: {}", if self.passed { "green" } else { "red" });
    }

    pub fn to_json(&self) -> String {
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|l| {
                format!(
                    "{{\"step\":{},\"kind\":{},\"pass\":{},\"note\":{}}}",
                    l.index + 1,
                    json_string(l.kind),
                    l.pass,
                    json_string(&l.note)
                )
            })
            .collect();
        format!(
            "{{\"scenario\":{},\"title\":{},\"passed\":{},\"steps\":[{}]}}",
            json_string(&self.scenario_id),
            json_string(&self.title),
            self.passed,
            steps.join(",")
        )
    }
}

/// Encode a string as a JSON string literal via serde_json (correct escaping without a dep on
/// a bespoke struct just for output).
pub fn json_string(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_dotted_path_walks_objects_and_indexes_arrays() {
        let v = json!({
            "placement": { "project": { "name": "verify" } },
            "blocked_by": [{ "name": "the blocker" }],
            "status": "todo",
        });
        assert_eq!(dig(&v, "status"), Some(&json!("todo")));
        assert_eq!(dig(&v, "placement.project.name"), Some(&json!("verify")));
        assert_eq!(dig(&v, "blocked_by.0.name"), Some(&json!("the blocker")));
    }

    /// A path that runs off the output comes back empty rather than landing on something else —
    /// the caller reports that as a mismatch, which is what a scenario naming a stale path deserves.
    #[test]
    fn a_path_that_does_not_exist_is_none() {
        let v = json!({ "blocked_by": [] });
        assert_eq!(dig(&v, "blocked_by.0.name"), None);
        assert_eq!(dig(&v, "placement.project.name"), None);
    }
}
