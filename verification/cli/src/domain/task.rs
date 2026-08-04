//! The `task` domain: the object the tracker is for. Creating one, moving it through its
//! progress states, editing its fields, ordering it against another, anchoring it to the commits
//! that carried it out — and reading every one of those back.

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, req_str, unmapped, Driver, Outcome};
use crate::judge::{judge_field, judge_found, judge_listing, judge_timeline};

impl Driver<'_> {
    pub(crate) fn task_action(&mut self, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            "create" => {
                let title = req_str(with, "title")?;
                // Which board it lands on: this run's own unless the step names another, the same
                // way `folder bind` picks the project it points a folder at.
                let pid = match with.get("project") {
                    Some(_) => self.resolve_key(with, "project")?,
                    None => self.project_id,
                }
                .to_string();
                let v = self.run_json(&["task", "add", "--title", title, "--project", &pid, "--json"])?;
                let id = v["task"]["id"].as_i64().ok_or("task add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("created task {id} `{title}` in project {pid}")))
            }
            // The second stage of the creation, run as the caller runs it — as its own command, once
            // the task is written. Idempotent on the binary's side, so a road that walks it twice is
            // reporting a no-op rather than failing.
            "finish-creating" => {
                let target = self.resolve(with)?;
                self.run_json(&["task", "finish-creating", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("finished creating task {target}")))
            }
            "assign" => {
                let target = self.resolve(with)?;
                let assignee = req_str(with, "assignee")?;
                self.run_json(&["task", "assign", &target.to_string(), "--to", assignee, "--json"])?;
                Ok(Outcome::action(format!("assigned task {target} to {assignee}")))
            }
            "comment" => {
                let target = self.resolve(with)?;
                let text = req_str(with, "text")?;
                let v = self.run_json(&["comment", "add", &target.to_string(), "--text", text, "--json"])?;
                let id = v["comment"]["id"].as_i64().ok_or("comment add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("commented on task {target}")))
            }
            "status" => {
                let target = self.resolve(with)?;
                let status = req_str(with, "status")?;
                // The move is refused rather than silently ignored (a reserve that is not from todo
                // comes back `already_reserved`), and `run_json` reads that non-zero exit as an
                // execution error — so a scenario that walks the states out of order says so. A
                // step that means to meet the guard declares it with `refused:` and is judged on it.
                self.run_json(&["task", "status", &target.to_string(), status, "--json"])?;
                Ok(Outcome::action(format!("moved task {target} to {status}")))
            }
            "done" => {
                let target = self.resolve(with)?;
                self.run_json(&["task", "done", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("marked task {target} done")))
            }
            "reject" => {
                let target = self.resolve(with)?;
                let reason = req_str(with, "reason")?;
                self.run_json(&["task", "reject", &target.to_string(), "--reason", reason, "--json"])?;
                Ok(Outcome::action(format!("rejected task {target}: {reason}")))
            }
            "reopen" => {
                let target = self.resolve(with)?;
                self.run_json(&["task", "reopen", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("reopened task {target}")))
            }
            "block" => {
                let target = self.resolve(with)?;
                let reason = req_str(with, "reason")?;
                self.run_json(&["task", "block", &target.to_string(), "--reason", reason, "--json"])?;
                Ok(Outcome::action(format!("blocked task {target}: {reason}")))
            }
            "update" => {
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
            "clear" => {
                let target = self.resolve(with)?;
                let field = req_str(with, "field")?;
                let flag = format!("--clear-{field}");
                self.run_json(&["task", "update", &target.to_string(), &flag, "--json"])?;
                Ok(Outcome::action(format!("cleared `{field}` on task {target}")))
            }
            "move" => {
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
            "depend" => {
                let target = self.resolve(with)?;
                let on = self.resolve_key(with, "on")?;
                self.run_json(&["task", "depend", &target.to_string(), "--on", &on.to_string(), "--json"])?;
                Ok(Outcome::action(format!("task {target} now waits on task {on}")))
            }
            "undepend" => {
                let target = self.resolve(with)?;
                let on = self.resolve_key(with, "on")?;
                self.run_json(&["task", "undepend", &target.to_string(), "--on", &on.to_string(), "--json"])?;
                Ok(Outcome::action(format!("task {target} no longer waits on task {on}")))
            }
            "commit-add" => {
                let target = self.resolve(with)?;
                let sha = req_str(with, "sha")?;
                self.run_json(&["task", "commit", "add", &target.to_string(), sha, "--json"])?;
                Ok(Outcome::action(format!("recorded commit {sha} on task {target}")))
            }
            "commit-rm" => {
                let target = self.resolve(with)?;
                let sha = req_str(with, "sha")?;
                // A hard delete asks first; the driver is unattended, so it answers up front.
                self.run_json(&["task", "commit", "rm", &target.to_string(), sha, "--yes", "--json"])?;
                Ok(Outcome::action(format!("forgot commit {sha} on task {target}")))
            }
            _ => Err(unmapped(Domain::Task, op)),
        }
    }
    pub(crate) fn task_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "listed" => {
                let target = self.resolve(with)?;
                let filter = req_str(with, "filter")?;
                let v = self.run_json(&["task", "list", "--filter", filter, "--json"])?;
                let rows = v["tasks"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                judge_listing("task", target, &format!("`{filter}`"), rows, with)
            }
            "found" => {
                let target = self.resolve(with)?;
                let hits = self.search(with)?;
                judge_found("task", &format!("AMB-T-{target}"), req_str(with, "words")?, with, &hits["hits"])
            }
            "status-bucket" => {
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
            "field" => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["task", "show", &target.to_string(), "--json"])?;
                judge_field(&format!("task {target}"), with, &v)
            }
            "commit" => {
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
            "commented" => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["comment", "list", &target.to_string(), "--json"])?;
                judge_timeline("task", target, with, &v["comments"])
            }
            "activity" => {
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
            _ => Err(unmapped(Domain::Task, op)),
        }
    }
}
