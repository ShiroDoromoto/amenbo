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
use std::path::{Path, PathBuf};
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
    /// What the last `plugin run` came back with. A command face's return value is its own stdout
    /// and is deliberately kept out of the execution log, so this is the only place a later step can
    /// read it from — which is why the assert that reads it has to follow its call.
    last_run: Option<serde_json::Value>,
    /// The files the `store` actions wrote, under the same names. A scenario has one binding
    /// namespace — the loader keeps it unique across both — and which of the two maps a name lands
    /// in follows from the op that bound it: nothing in the store is a path, and no archive is an id.
    artifacts: HashMap<String, std::path::PathBuf>,
    /// Set while a step that declared `refused:` is running. It is read where a failed invocation
    /// is judged, so the arm issuing the command never has to know it might be turned away —
    /// [`Driver::refused`] puts it up and takes it back down around the one call.
    refusal: Option<String>,
}

/// What an expected refusal travels back on. A refusal has to reach [`Driver::refused`] from
/// wherever the command was issued, and the way out of an arm that every one of them already has is
/// the `?` on its invocation — so it goes as an `Err`, and a byte no message of ours carries keeps
/// it apart from a real failure. The code that came back is spliced on after it.
const REFUSED: &str = "\u{1}refused:";

impl Driver {
    /// Boot a fresh store: `init` creates it and hands back the project every `task add` needs.
    fn new(bin: &Path, session: scratch::Session) -> Result<Driver, String> {
        let mut d = Driver {
            bin: bin.to_path_buf(),
            session,
            project_id: 0,
            bindings: HashMap::new(),
            last_run: None,
            artifacts: HashMap::new(),
            refusal: None,
        };
        let v = d.run_json(&["init", "--name", "verify", "--json"])?;
        d.project_id = v["identity"]["project_id"]
            .as_i64()
            .ok_or("init did not report a project_id")?;
        Ok(d)
    }

    /// Spawn the shipped binary in the isolated store, from a chosen folder. Every call goes
    /// through here, so the isolation is stated once and cannot be forgotten by an arm that builds
    /// its own command. Where the command stands is itself an input for anything to do with binding
    /// — the pointer that decides what a run reaches is found by walking up from the CWD — so those
    /// steps ask their question from inside the folder they are asking about.
    fn invoke_in(&self, cwd: &Path, args: &[&str]) -> Result<std::process::Output, String> {
        // The facet goes on the command line, which is the one input amenbo is to take it by; a call
        // that names its own is left alone.
        let mut with_facet = args.to_vec();
        if !args.contains(&"--actor") {
            with_facet.extend_from_slice(&["--actor", "human"]);
        }
        Command::new(&self.bin)
            .args(&with_facet)
            .current_dir(cwd)
            .env("AMENBO_HOME", &self.session.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("NO_COLOR", "1")
            .output()
            .map_err(|e| format!("could not run `{}`: {e}", self.bin.display()))
    }

    /// The same, from the run's own CWD — where every step that is not asking about a folder stands.
    fn invoke(&self, args: &[&str]) -> Result<std::process::Output, String> {
        self.invoke_in(&self.session.cwd, args)
    }

    /// Run `amenbo <args>` in the isolated store and parse its `--json` output. An `Err` is an
    /// execution failure (spawn failed, non-JSON output, non-zero exit, or an `error` object) —
    /// distinct from an assert that ran cleanly and came out false.
    fn run_json(&self, args: &[&str]) -> Result<serde_json::Value, String> {
        self.run_json_in(&self.session.cwd, args)
    }

    /// The same, from a chosen folder.
    fn run_json_in(&self, cwd: &Path, args: &[&str]) -> Result<serde_json::Value, String> {
        let out = self.invoke_in(cwd, args)?;
        if let Some(code) = self.refused_code(&out) {
            return Err(format!("{REFUSED}{code}"));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let v = parse_json(args, &stdout)?;
        if !out.status.success() || v.get("error").is_some() {
            return Err(format!("`amenbo {}` failed: {}", args.join(" "), stdout.trim()));
        }
        Ok(v)
    }

    /// The same, for a command whose exit code is its **verdict** rather than a report on whether
    /// it ran: `doctor` and `validate` come back non-zero when what they found is bad news, and
    /// that is a value to judge, not a driver failure. A command that could not run at all still
    /// says so in an `error` object, which is an `Err` here as everywhere.
    fn run_check(&self, args: &[&str]) -> Result<serde_json::Value, String> {
        let out = self.invoke(args)?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let v = parse_json(args, &stdout)?;
        if v.get("error").is_some() {
            return Err(format!("`amenbo {}` failed: {}", args.join(" "), stdout.trim()));
        }
        Ok(v)
    }

    /// Run for the file it leaves behind rather than for what it prints — `export --out` answers
    /// with a directory on disk and prose on stdout, so there is no JSON to read and the exit code
    /// is the whole signal. stderr carries the reason when it fails.
    fn run_bare(&self, args: &[&str]) -> Result<(), String> {
        let out = self.invoke(args)?;
        if let Some(code) = self.refused_code(&out) {
            return Err(format!("{REFUSED}{code}"));
        }
        if !out.status.success() {
            return Err(format!(
                "`amenbo {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    }

    /// Resolve a path a step named against the session's own folder, refusing anything that would
    /// reach outside it. A scenario writes and lints files in the throwaway CWD and nowhere else —
    /// this driver is handed real machines to run on, so an absolute path or a `..` in a scenario is
    /// refused rather than followed.
    fn in_session(&self, path: &str) -> Result<std::path::PathBuf, String> {
        let p = Path::new(path);
        if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(format!("`path: {path}` must stay inside the run's own folder"));
        }
        Ok(self.session.cwd.join(p))
    }

    /// The code a refusal came back with, but only while a step is expecting one. amenbo prints the
    /// error object on **stderr** and leaves stdout empty, so a refusal is read off the stream it is
    /// actually on rather than the one a result would come back on. `None` when no refusal is
    /// expected, when the command succeeded, or when the failure carries no error object at all —
    /// that last is a binary that could not run, and it stays an execution error.
    fn refused_code(&self, out: &std::process::Output) -> Option<String> {
        self.refusal.as_ref()?;
        if out.status.success() {
            return None;
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        let v: serde_json::Value = serde_json::from_str(stderr.trim()).ok()?;
        Some(v["error"]["code"].as_str()?.to_string())
    }

    /// Map one step to a binary call. Returns whether the step passed — an assert's verdict, or a
    /// `refused:` action's — plus a human note. An action with nothing to prove passes unless it
    /// errors.
    fn exec(&mut self, step: &Step) -> Result<Outcome, String> {
        match step {
            Step::Action { domain, op, with, bind } => match with.get("refused") {
                Some(code) => {
                    let want = code
                        .as_str()
                        .ok_or("`refused` must be the error code the operation is expected to be rejected with")?
                        .to_string();
                    self.refused(*domain, op, with, &want)
                }
                None => self.action(*domain, op, with, bind.as_deref()),
            },
            Step::Assert { domain, op, with } => self.assert(*domain, op, with),
        }
    }

    /// Run an operation the step says amenbo will turn away, and judge the refusal — the guard in
    /// front of an operation is only proven by meeting it, and a driver that reads every non-zero
    /// exit as its own failure can never write that line down.
    ///
    /// The op arms are left exactly as they are: the invocation recognises the refusal and unwinds
    /// the arm through the `?` it already has on its command, which lands back here. So the verdict
    /// is: refused with the code the step named → pass; refused with another code → fail, since the
    /// step is about *that* guard; went through → fail, which is the regression this exists to catch.
    fn refused(&mut self, domain: Domain, op: &str, with: &Args, want: &str) -> Result<Outcome, String> {
        self.refusal = Some(want.to_string());
        // A refused op binds nothing, so no binding is offered to the arm (the loader refuses `as:`
        // on one, and there would be no id to put under the name anyway).
        let outcome = self.action(domain, op, with, None);
        self.refusal = None;
        match outcome {
            Err(e) => match e.strip_prefix(REFUSED) {
                Some(code) => Ok(Outcome::assert(
                    code == want,
                    format!(
                        "`{op}` was refused with `{code}` ({})",
                        if code == want {
                            "as expected".to_string()
                        } else {
                            format!("expected `{want}`, MISMATCH")
                        }
                    ),
                )),
                None => Err(e), // it did not get as far as a refusal — an execution error as usual
            },
            Ok(done) => Ok(Outcome::assert(
                false,
                format!("`{op}` went through where `{want}` was expected to refuse it — {} (MISMATCH)", done.note),
            )),
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
                // execution error — so a scenario that walks the states out of order says so. A
                // step that means to meet the guard declares it with `refused:` and is judged on it.
                self.run_json(&["task", "status", &target.to_string(), status, "--json"])?;
                Ok(Outcome::action(format!("moved task {target} to {status}")))
            }
            (Domain::Task, "done") => {
                let target = self.resolve(with)?;
                self.run_json(&["task", "done", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("marked task {target} done")))
            }
            (Domain::Task, "reject") => {
                let target = self.resolve(with)?;
                let reason = req_str(with, "reason")?;
                self.run_json(&["task", "reject", &target.to_string(), "--reason", reason, "--json"])?;
                Ok(Outcome::action(format!("rejected task {target}: {reason}")))
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
            (Domain::Decision, "reject") => {
                let target = self.resolve(with)?;
                let id = target.to_string();
                let mut args: Vec<String> =
                    vec!["decision".into(), "reject".into(), id, "--yes".into(), "--json".into()];
                // The reason is not a field of its own: it lands on the decision's timeline, which is
                // where a later reader looks for why the proposal did not carry.
                if let Some(reason) = with.get("reason").and_then(|v| v.as_str()) {
                    args.push("--reason".into());
                    args.push(reason.to_string());
                }
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("rejected decision {target}")))
            }
            (Domain::Decision, "reopen") => {
                let target = self.resolve(with)?;
                self.run_json(&["decision", "reopen", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("returned decision {target} to discussion")))
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
            // Hanging bytes or a link on a record. The three owners differ only in the command that
            // takes them, so one arm carries all three and the domain says which.
            (Domain::Task | Domain::Decision | Domain::Comment, "attach") => {
                let target = self.resolve(with)?;
                let (noun, argv0): (&str, &[&str]) = match domain {
                    Domain::Task => ("task", &["task", "attach"]),
                    Domain::Decision => ("decision", &["decision", "attach"]),
                    _ => ("comment", &["comment", "attach"]),
                };
                let id = target.to_string();
                let mut args: Vec<String> = argv0.iter().map(|s| s.to_string()).collect();
                args.push(id);
                // A blob is ingested from a file the run wrote (`repo write-file`); a link is the
                // URL itself. One or the other — an attach that names neither has nothing to hang.
                match (with.get("file").and_then(|v| v.as_str()), with.get("url").and_then(|v| v.as_str())) {
                    (Some(file), None) => {
                        self.in_session(file)?; // refuse a path that reaches out of the run's folder
                        args.push(file.to_string());
                    }
                    (None, Some(url)) => {
                        args.push(url.to_string());
                        args.push("--url".into());
                    }
                    _ => return Err("`attach` names either a `file` or a `url`, and exactly one".to_string()),
                }
                if let Some(name) = with.get("name").and_then(|v| v.as_str()) {
                    args.push("--name".into());
                    args.push(name.to_string());
                }
                args.push("--json".into());
                let v = self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                let att = v["attachment"]["id"].as_i64().ok_or("attach did not report an id")?;
                if let Some(b) = bind {
                    self.bindings.insert(b.to_string(), att);
                }
                Ok(Outcome::action(format!("attached {att} to {noun} {target}")))
            }
            (Domain::Attachment, "rm") => {
                let target = self.resolve(with)?;
                self.run_json(&["attach", "rm", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("removed attachment {target}")))
            }
            // The folder the run works in. `write-file` is a person already having a file there —
            // what gets attached, and what the lint is pointed at.
            (Domain::Repo, "write-file") => {
                let path = req_str(with, "path")?;
                let content = req_str(with, "content")?;
                let full = self.in_session(path)?;
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                std::fs::write(&full, content).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Outcome::action(format!("wrote {path} ({} bytes)", content.len())))
            }
            // The same, for text a scenario cannot hold itself. A file under `fixtures/` is where the
            // reference form lives: this tree's prose rule keeps a bare ref out of every `.yaml`, and
            // the lint has nothing to find unless something really carries one.
            (Domain::Repo, "copy-fixture") => {
                let from = req_str(with, "from")?;
                let path = req_str(with, "path")?;
                if Path::new(from).is_absolute()
                    || Path::new(from).components().any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(format!("`from: {from}` must name a file under fixtures/"));
                }
                let src = fixtures_dir().join(from);
                let full = self.in_session(path)?;
                let bytes = std::fs::read(&src)
                    .map_err(|e| format!("could not read the fixture {}: {e}", src.display()))?;
                std::fs::write(&full, &bytes).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Outcome::action(format!("copied the fixture {from} to {path} ({} bytes)", bytes.len())))
            }
            // The hooks are written into a git repository, so the scenario has to stand one up first.
            // This is the one step that is not amenbo — everything it proves is about what amenbo
            // then does to a repository that is really there.
            //
            // It leaves a `main` with one commit on it, rather than the branchless state a bare
            // `init` leaves behind: a repository with no commit has no branch either, and the
            // official `worktree` plugin needs one to cut a task's checkout from.
            (Domain::Repo, "git-init") => {
                let git = |args: &[&str]| -> Result<(), String> {
                    let out = Command::new("git")
                        .args(args)
                        .current_dir(&self.session.cwd)
                        .output()
                        .map_err(|e| format!("could not run git: {e}"))?;
                    if !out.status.success() {
                        return Err(format!(
                            "`git {}` failed: {}",
                            args.join(" "),
                            String::from_utf8_lossy(&out.stderr).trim()
                        ));
                    }
                    Ok(())
                };
                git(&["init", "-q", "--initial-branch", "main"])?;
                // Named on the command line rather than left to the machine's git config: a box with
                // no identity set would fail here, and neither name belongs to anybody.
                git(&[
                    "-c", "user.name=verify",
                    "-c", "user.email=verify@example.invalid",
                    "commit", "--quiet", "--allow-empty",
                    "-m", "the branch a scenario cuts from",
                ])?;
                Ok(Outcome::action("made the run's folder a git repository on `main`".to_string()))
            }
            (Domain::Repo, verb @ ("hooks-install" | "hooks-uninstall")) => {
                let sub = verb.trim_start_matches("hooks-");
                self.run_json(&["hooks", sub, "--yes", "--json"])?;
                Ok(Outcome::action(format!("ran `hooks {sub}` on the run's repository")))
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
            (Domain::Store, "export") => {
                let out = self.artifact(bind, "export", "");
                // `--out` takes the attachment files along with the records, which is the shape a
                // move to another tool is actually made in; it answers with prose, not JSON.
                self.run_bare(&["export", "--out", path_str(&out)?])?;
                self.remember(bind, "export", out.clone());
                Ok(Outcome::action(format!("exported the store to {}", out.display())))
            }
            (Domain::Store, "backup") => {
                let path = self.artifact(bind, "backup", ".amenbo-backup");
                let v = self.run_json(&["backup", path_str(&path)?, "--json"])?;
                let bytes = v["bytes"].as_i64().unwrap_or(0);
                self.remember(bind, "backup", path.clone());
                Ok(Outcome::action(format!("wrote a {bytes}-byte snapshot to {}", path.display())))
            }
            (Domain::Store, "restore") => {
                let path = self.artifact_ref(with, "target")?;
                // A destructive replace asks first; the driver is unattended, so it answers up front.
                let v = self.run_json(&["restore", path_str(&path)?, "--yes", "--json"])?;
                let saved = v["previous_saved_to"].as_str().unwrap_or("(not reported)");
                Ok(Outcome::action(format!(
                    "restored the store from {} (what it replaced was set aside at {saved})",
                    path.display()
                )))
            }
            (Domain::Comment, "hard-erase") => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["hard-erase", "comment", &target.to_string(), "--yes", "--json"])?;
                let safety = v["backup"]["path"].as_str().unwrap_or("(none reported)");
                Ok(Outcome::action(format!(
                    "erased comment {target} from the truth source (safety archive: {safety})"
                )))
            }
            (Domain::Decision, "hard-erase") => {
                let target = self.resolve(with)?;
                let body = req_str(with, "body")?;
                self.run_json(&["hard-erase", "decision", &target.to_string(), "--body", body, "--yes", "--json"])?;
                Ok(Outcome::action(format!("redacted the body of decision {target}")))
            }
            (Domain::Decision, "unlink") => {
                let target = self.resolve(with)?;
                let other = self.resolve_key(with, "from")?;
                self.run_json(&["decision", "unlink", &target.to_string(), "--from", &other.to_string(), "--json"])?;
                Ok(Outcome::action(format!("decision {target} no longer points at decision {other}")))
            }
            // The repairing face. `--yes` because the driver is unattended, and the verdict it comes
            // back with is judged the way the reading face's is — a store still unsound after a
            // repair is a value, not a driver failure.
            (Domain::Store, "doctor-fix") => {
                let v = self.run_check(&["doctor", "--fix", "--yes", "--json"])?;
                let left = v["issues"].as_array().map_or(0, Vec::len);
                Ok(Outcome::action(format!("swept the store ({left} issue(s) still standing after it)")))
            }
            (Domain::Store, "config-set") => {
                let key = req_str(with, "key")?;
                let value = req_str(with, "value")?;
                self.run_json(&["config", "set", key, value, "--json"])?;
                Ok(Outcome::action(format!("set `{key}` to `{value}`")))
            }
            (Domain::Folder, "init") => {
                let dir = self.folder(with)?;
                // Run it *from inside* the folder: `init` binds where it stands, and the project it
                // raises is named after that folder.
                let v = self.run_json_in(&dir, &["init", "--json"])?;
                let id = v["identity"]["project_id"].as_i64().ok_or("init did not report a project_id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("initialised {} as project {id}", dir.display())))
            }
            (Domain::Folder, "bind") => {
                // Which project a folder is pointed at: this run's own unless the step names another.
                let pid = match with.get("project") {
                    Some(_) => self.resolve_key(with, "project")?,
                    None => self.project_id,
                };
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                self.run_json(&["bind", "--project", &pid.to_string(), "--dir", &path, "--json"])?;
                Ok(Outcome::action(format!("bound {path} to project {pid}")))
            }
            (Domain::Folder, "unbind") => {
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                // Removing a pointer asks first; the driver is unattended, so it answers up front.
                self.run_json(&["unbind", "--dir", &path, "--yes", "--json"])?;
                Ok(Outcome::action(format!("unbound {path}")))
            }
            // Leave the folder's pointer in the shape an older amenbo wrote: a `project_id` that is
            // not the integer key. Nothing amenbo ships writes one any more, and that is the point —
            // this is the state on disk that `doctor --fix` exists to put right, so the scenario has
            // to make it, exactly as `repo write-file` makes the file a person already had.
            //
            // It is written from outside the folder rather than by running anything in it: amenbo
            // heals the pointer of a folder it is run in, so a visit would undo this before the
            // repair under test ever saw it.
            (Domain::Folder, "legacy-pointer") => {
                let dir = self.folder(with)?;
                let pointer = dir.join(".amenbo");
                std::fs::write(&pointer, br#"{"v":1,"project_id":"a-name-not-a-key"}"#)
                    .map_err(|e| format!("could not write {}: {e}", pointer.display()))?;
                Ok(Outcome::action(format!("left {} pointing the way an older build did", dir.display())))
            }
            (Domain::Plugin, "install") => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["plugin", "install", name, "--json"])?;
                let bytes = v["program_bytes"].as_i64().unwrap_or(0);
                Ok(Outcome::action(format!("installed plugin `{name}` ({bytes} bytes of program)")))
            }
            (Domain::Plugin, "enable") => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["plugin", "enable", name, "--json"])?;
                let level = v["level"].as_str().unwrap_or("?").to_string();
                Ok(Outcome::action(format!("opened `{name}`'s gate ({level})")))
            }
            (Domain::Plugin, "disable") => {
                let name = req_str(with, "name")?;
                self.run_json(&["plugin", "disable", name, "--json"])?;
                Ok(Outcome::action(format!("closed `{name}`'s gate")))
            }
            (Domain::Plugin, "uninstall") => {
                let name = req_str(with, "name")?;
                // Removing a plugin takes its settings, its consent and its log rows with it, so it
                // asks first; the driver is unattended and answers up front.
                self.run_json(&["plugin", "uninstall", name, "--yes", "--json"])?;
                Ok(Outcome::action(format!("removed plugin `{name}`")))
            }
            (Domain::Plugin, "run") => {
                let name = req_str(with, "name")?.to_string();
                let command = req_str(with, "command")?.to_string();
                // Everything after `plugin run <name>` belongs to the plugin, so amenbo's own flags
                // have to be said before the subcommand — appended, they would reach the plugin as
                // arguments and amenbo would see no facet at all.
                let mut args: Vec<String> =
                    vec!["--actor".into(), "human".into(), "--json".into(), "plugin".into(), "run".into()];
                args.push(name.clone());
                args.push(command.clone());
                if with.contains_key("task") {
                    args.push(self.resolve_key(with, "task")?.to_string());
                }
                for extra in with.get("args").and_then(|v| v.as_sequence()).unwrap_or(&Vec::new()) {
                    let extra = extra.as_str().ok_or("every entry under `args` must be a string")?;
                    args.push(extra.to_string());
                }
                let v = self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                let value = v["value"].as_str().unwrap_or_default().len();
                self.last_run = Some(v);
                Ok(Outcome::action(format!(
                    "called `{name} {command}` — it returned {value} byte(s)"
                )))
            }
            (Domain::Folder, "sync-guide") => {
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                let v = self.run_json(&["sync-guide", "--dir", &path, "--json"])?;
                let rewritten = v["updated"].as_array().map_or(0, Vec::len);
                Ok(Outcome::action(format!("resynced the guidance in {path} ({rewritten} file(s) rewritten)")))
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
                judge_field(&format!("project {target}"), with, &v)
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
                judge_field(&format!("task {target}"), with, &v)
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
                judge_field(&format!("decision {target}"), with, &v)
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
            (Domain::Task, "exported") | (Domain::Decision, "exported") | (Domain::Comment, "exported") => {
                self.judge_exported(domain, with)
            }
            (Domain::Store, "snapshot") => {
                let path = self.artifact_ref(with, "target")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                // An archive is a file with bytes in it. Whether those bytes put a store back is
                // what `restore` answers — asking that here would only be guessing at the layout of
                // something this driver is meant to treat as a black box.
                let bytes = path.metadata().ok().filter(|m| m.is_file()).map(|m| m.len());
                let found = bytes.is_some_and(|n| n > 0);
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "{} {} (expected {}, {})",
                        path.display(),
                        match bytes {
                            Some(n) => format!("holds {n} bytes"),
                            None => "is not a file on disk".to_string(),
                        },
                        if present { "an archive" } else { "nothing" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Store, "doctor") => {
                let want = req_bool(with, "ok")?;
                let v = self.run_check(&["doctor", "--json"])?;
                // Naming a kind asks about the list rather than the verdict. Most of what doctor
                // raises is a warning, which leaves `ok` alone — so a problem appearing, and a repair
                // taking it away, are only visible from here.
                match with.get("issue").and_then(|v| v.as_str()) {
                    None => Ok(judge_check("doctor", want, &v)),
                    Some(kind) => {
                        let present = opt_bool(with, "present").unwrap_or(true);
                        let found = v["issues"]
                            .as_array()
                            .is_some_and(|rows| rows.iter().any(|i| i["kind"].as_str() == Some(kind)));
                        let pass = found == present && v["ok"].as_bool().unwrap_or(false) == want;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "`doctor` {} `{kind}` issue (expected {}, {})",
                                if found { "raises a" } else { "raises no" },
                                if present { "raised" } else { "gone" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            (Domain::Store, "validate") => {
                let want = req_bool(with, "ok")?;
                // With no target the whole store is checked, which is what a user typing it bare
                // gets; naming one narrows it to that object.
                let mut args: Vec<String> = vec!["validate".into()];
                if with.contains_key("target") {
                    args.push(self.resolve(with)?.to_string());
                }
                args.push("--json".into());
                let v = self.run_check(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(judge_check("validate", want, &v))
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
            (Domain::Store, "config") => {
                let v = self.run_json(&["config", "--json"])?;
                judge_field("this store's configuration", with, &v)
            }
            (Domain::Store, "identity") => {
                let v = self.run_json(&["whoami", "--json"])?;
                judge_field("this store's identity", with, &v)
            }
            (Domain::Store, "update") => {
                // `--print` is the face that opens nothing, which is the only one a scenario may
                // wear: a check is a read, and it must not launch a browser at whoever runs it.
                let v = self.run_json(&["update", "--print", "--json"])?;
                judge_field("the update check", with, &v)
            }
            (Domain::Folder, "bound") => {
                let dir = self.folder(with)?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let want = match with.get("project") {
                    Some(_) => Some(self.resolve_key(with, "project")?),
                    None => None,
                };
                // Asked from inside the folder, which is the question an AI launched there asks.
                let v = self.run_json_in(&dir, &["bind", "--json"])?;
                let at = v["binding"]["project_id"].as_i64();
                let found = match (at, want) {
                    (Some(id), Some(named)) => id == named,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "{} {} (expected {}, {})",
                        dir.display(),
                        match at {
                            Some(id) => format!("points at project {id}"),
                            None => "points at no project".to_string(),
                        },
                        match (present, want) {
                            (true, Some(named)) => format!("project {named}"),
                            (true, None) => "a binding".to_string(),
                            (false, _) => "none".to_string(),
                        },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Plugin, "listed") => {
                let name = req_str(with, "name")?;
                let v = self.run_json(&["plugin", "list", "--json"])?;
                let row = v["plugins"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|p| p["name"].as_str() == Some(name)));
                // With `enabled` the question is the gate, and `install ≠ enable` is exactly what a
                // reader gets wrong — so the two are asked apart, never rolled into one answer.
                match opt_bool(with, "enabled") {
                    Some(want) => {
                        let got = row.and_then(|r| r["enabled"].as_bool());
                        let pass = got == Some(want);
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "plugin `{name}` gate is {} (expected {want}, {})",
                                match got {
                                    Some(open) => open.to_string(),
                                    None => "not installed at all".to_string(),
                                },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                    None => {
                        let present = opt_bool(with, "present").unwrap_or(true);
                        let pass = row.is_some() == present;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "plugin `{name}` {} on this machine (expected {}, {})",
                                if row.is_some() { "is installed" } else { "is not installed" },
                                if present { "installed" } else { "gone" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            (Domain::Plugin, "returned") => {
                let want = req_str(with, "contains")?;
                let last = self
                    .last_run
                    .as_ref()
                    .ok_or("no `plugin run` has been called yet, so there is no return value to read")?;
                let value = last["value"].as_str().unwrap_or_default();
                let pass = value.contains(want);
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the call returned {:?} (expected it to carry `{want}`, {})",
                        value,
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Attachment, "field") => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["attach", "show", &target.to_string(), "--json"])?;
                judge_field(&format!("attachment {target}"), with, &v)
            }
            (Domain::Attachment, "listed") => {
                let target = self.resolve(with)?;
                let id = self.resolve_key(with, "owner")?;
                // Which list to ask is not something the id can say. Tasks and decisions number in
                // sibling spaces, so the same number can name one of each and a bare id is refused as
                // ambiguous — the owner is named in full. A comment is reached by a flag instead: the
                // two comment tables number apart, and there is no ref that says which one it is.
                let kind = req_str(with, "owner_kind")?;
                let owner = match kind {
                    "decision" => format!("AMB-D-{id}"),
                    "task" => format!("AMB-T-{id}"),
                    _ => id.to_string(),
                };
                let args: Vec<&str> = match kind {
                    "task" | "decision" => vec!["attach", "ls", &owner, "--json"],
                    "task-comment" => vec!["attach", "ls", "--task-comment", &owner, "--json"],
                    "decision-comment" => vec!["attach", "ls", "--decision-comment", &owner, "--json"],
                    other => {
                        return Err(format!(
                            "`owner_kind: {other}` is not task / decision / task-comment / decision-comment"
                        ))
                    }
                };
                let v = self.run_json(&args)?;
                let rows = v["attachments"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                judge_listing("attachment", target, &format!("the {kind}'s attachments"), rows, with)
            }
            (Domain::Attachment, "saved") => {
                let target = self.resolve(with)?;
                let want = req_str(with, "content")?;
                // Saving the bytes back out is the only thing that proves the ingest kept them: the
                // row says how many bytes there were, the file says which ones.
                let out = self.in_session(&format!("saved-{target}"))?;
                let out_arg = out.to_string_lossy().to_string();
                self.run_json(&[
                    "attach", "save", &target.to_string(), "--out", &out_arg, "--force", "--json",
                ])?;
                let got = std::fs::read_to_string(&out)
                    .map_err(|e| format!("could not read back {}: {e}", out.display()))?;
                let pass = got == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "attachment {target} saved {} byte(s), {}",
                        got.len(),
                        if pass { "the bytes that went in" } else { "MISMATCH against what went in" }
                    ),
                ))
            }
            (Domain::Repo, "lint") => {
                let path = req_str(with, "path")?;
                self.in_session(path)?;
                let want =
                    with.get("hits").and_then(|v| v.as_u64()).ok_or("arg `hits` must be a number")?;
                // Finding something is how the lint reports — exit code included — so a non-zero
                // exit here is its verdict rather than a failure to run.
                let v = self.run_check(&["lint", path, "--json"])?;
                let hits = v["hits"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                // A count alone would not say the report locates anything, and the ref itself cannot be
                // written into a scenario (this tree's prose rule keeps a bare one out of every
                // `.yaml`), so what a line asks for instead is the line number it was found on.
                let at = with.get("line").and_then(|v| v.as_u64());
                let located = match at {
                    Some(n) => hits.iter().any(|h| h["line"].as_u64() == Some(n)),
                    None => true,
                };
                let pass = hits.len() as u64 == want && located;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "lint reports {} ref(s) in {path}{} (expected {want}, {})",
                        hits.len(),
                        at.map(|n| format!(", one of them on line {n}")).unwrap_or_default(),
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Plugin, "ran") => {
                let name = req_str(with, "name")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["plugin", "runs", name, "--json"])?;
                // Newest first, so the run a step is asking about is the one at the head.
                let newest = v["runs"].as_array().and_then(|rows| rows.first());
                match with.get("outcome").and_then(|v| v.as_str()) {
                    Some(want) => {
                        let got = newest.and_then(|r| r["outcome"].as_str());
                        let pass = got == Some(want);
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "`{name}`'s last run ended {} (expected {want}, {})",
                                got.unwrap_or("— it has no runs at all"),
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                    None => {
                        let pass = newest.is_some() == present;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "the log holds {} run(s) for `{name}` (expected {}, {})",
                                v["count"].as_i64().unwrap_or(0),
                                if present { "at least one" } else { "none" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            (Domain::Folder, "resynced") => {
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                let changed = opt_bool(with, "changed").unwrap_or(false);
                // A resync writes only what actually differs, so "nothing left to write" is how a
                // folder says its block is already at this build's version — and it is the property
                // that makes the command safe to point at every folder on the device.
                let v = self.run_json(&["sync-guide", "--dir", &path, "--json"])?;
                let rewritten = v["updated"].as_array().map_or(0, Vec::len);
                let pass = (rewritten > 0) == changed;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "a resync of {path} rewrote {rewritten} file(s) (expected {}, {})",
                        if changed { "a rewrite" } else { "nothing to do" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Repo, "hooks") => {
                let hook = req_str(with, "hook")?;
                let want = req_str(with, "state")?;
                let v = self.run_json(&["hooks", "status", "--json"])?;
                let slot = v["hooks"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .find(|h| h["hook"].as_str() == Some(hook));
                let state = slot.and_then(|h| h["state"]["kind"].as_str()).unwrap_or("no slot");
                let pass = state == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "hook `{hook}` is {state} (expected {want}, {})",
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(domain, op)),
        }
    }

    /// Is the object an earlier step bound in the export written by another? The export is read off
    /// disk as the document it is, because that document is the whole promise of the capability:
    /// what another tool receives is this file, not what amenbo would say about it.
    fn judge_exported(&self, domain: Domain, with: &Args) -> Result<Outcome, String> {
        let target = self.resolve(with)?;
        let dir = self.artifact_ref(with, "from")?;
        let present = opt_bool(with, "present").unwrap_or(true);
        let file = dir.join("export.json");
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("could not read the export at {}: {e}", file.display()))?;
        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("the export at {} is not JSON: {e}", file.display()))?;
        // The tables an object of this domain lands in. A comment is looked for on both timelines:
        // a bound comment id is whichever of the two the step that posted it made.
        let (noun, tables): (&str, &[&str]) = match domain {
            Domain::Task => ("task", &["task"]),
            Domain::Decision => ("decision", &["decision"]),
            Domain::Comment => ("comment", &["task_comment", "decision_comment"]),
            other => return Err(format!("`exported` says nothing about domain `{other:?}`")),
        };
        let found = tables.iter().any(|t| {
            v["tables"][t]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r["id"].as_i64() == Some(target)))
        });
        let pass = found == present;
        Ok(Outcome::assert(
            pass,
            format!(
                "{noun} {target} {} the export at {} under {} (expected {}, {})",
                if found { "is in" } else { "is missing from" },
                file.display(),
                tables.join("/"),
                if present { "carried out" } else { "left behind" },
                if pass { "as expected" } else { "MISMATCH" }
            ),
        ))
    }

    /// Where a `store` action's file goes: under the session's own scratch space, named after the
    /// binding that will name it back. A step that binds nothing still gets a slot of its own, so
    /// two unnamed backups never collide on one path — which `backup` would refuse outright.
    fn artifact(&self, bind: Option<&str>, kind: &str, ext: &str) -> std::path::PathBuf {
        let stem = match bind {
            Some(name) => name.to_string(),
            None => format!("{kind}-{}", self.artifacts.len()),
        };
        self.session.artifacts.join(format!("{stem}{ext}"))
    }

    /// Record what an action wrote, under the name a later step will ask for it by.
    fn remember(&mut self, bind: Option<&str>, kind: &str, path: std::path::PathBuf) {
        let name = match bind {
            Some(name) => name.to_string(),
            None => format!("{kind}-{}", self.artifacts.len()),
        };
        self.artifacts.insert(name, path);
    }

    /// Resolve a name to the file an earlier `store` action wrote. The loader proved the name
    /// resolves to an earlier `as:`, so what is left to catch here is a name bound by an op that
    /// produced an object rather than a file.
    fn artifact_ref(&self, with: &Args, key: &str) -> Result<std::path::PathBuf, String> {
        let name = req_str(with, key)?;
        self.artifacts.get(name).cloned().ok_or_else(|| {
            format!("`{key}: {name}` names no file a `store` action wrote in this run")
        })
    }

    /// The folder a step names. A scenario says *which* folder, never where it is: the driver places
    /// it, because a binding is answered by where a folder sits and only the driver knows where its
    /// own isolated run lives.
    fn folder(&self, with: &Args) -> Result<PathBuf, String> {
        let name = req_str(with, "dir")?;
        self.session
            .folder(name)
            .map_err(|e| format!("could not make the folder `{name}`: {e}"))
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
    /// An assert's verdict — and an action's, when it declared `refused:` and is judged on whether
    /// it really was turned away. An ordinary action is `true` unless it errored out of `exec`.
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

/// Where the scenario fixtures live — `verification/fixtures/`, beside the scenarios that name them.
/// Resolved from this crate's own location rather than from the CWD, so `verify-all` finds them
/// wherever it is invoked from.
fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("fixtures")
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

/// Judge a `field` assert against a read of the thing it is about — an object's `show --json`, or
/// one of the reads the store answers about itself. `equals` is any scalar (string / bool / number /
/// null), compared structurally against the field's JSON value, so `status: todo` and `completed:
/// false` both work — and a field the output does not carry at all is a mismatch, not an error,
/// since a scenario naming one is asserting about the shipped output's shape too.
fn judge_field(subject: &str, with: &Args, shown: &serde_json::Value) -> Result<Outcome, String> {
    let field = req_str(with, "field")?;
    let expected = with.get("equals").ok_or("arg `equals` is required")?;
    let expected = serde_json::to_value(expected)
        .map_err(|e| format!("arg `equals` is not a valid value: {e}"))?;
    match dig(shown, field) {
        None => Ok(Outcome::assert(
            false,
            format!("{subject} has no field `{field}` in its output (MISMATCH)"),
        )),
        Some(actual) => {
            let pass = *actual == expected;
            Ok(Outcome::assert(
                pass,
                format!(
                    "{subject} field `{field}` = {actual} (expected {expected}, {})",
                    if pass { "as expected" } else { "MISMATCH" }
                ),
            ))
        }
    }
}

/// Read a binary's stdout as JSON, naming the call in the failure so a command that printed prose
/// (or nothing) is recognisable without re-running it by hand.
fn parse_json(args: &[&str], stdout: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(stdout.trim()).map_err(|e| {
        format!("`amenbo {}` did not print JSON ({e}); output was:\n{}", args.join(" "), stdout.trim())
    })
}

fn unmapped(domain: Domain, op: &str) -> String {
    format!(
        "op `{op}` for domain `{domain:?}` is in the scenario registry but not yet mapped in the CLI driver"
    )
}

/// Judge an integrity read: the check reports a verdict of its own, and the step says which one it
/// expects. The issue count rides along in the note, since a red one is only useful with its list.
fn judge_check(tool: &str, want: bool, v: &serde_json::Value) -> Outcome {
    let ok = v["ok"].as_bool().unwrap_or(false);
    let issues = v["issues"].as_array().map(Vec::len).unwrap_or(0);
    Outcome::assert(
        ok == want,
        format!(
            "`{tool}` reports {} over {issues} issue(s) (expected {}, {})",
            if ok { "sound" } else { "problems" },
            if want { "sound" } else { "problems" },
            if ok == want { "as expected" } else { "MISMATCH" }
        ),
    )
}

/// A path as the binary takes it. Non-UTF-8 never comes up — these paths are the driver's own —
/// but it is refused rather than mangled into one that names something else.
fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path {} is not valid UTF-8", path.display()))
}

fn req_str<'a>(with: &'a Args, key: &str) -> Result<&'a str, String> {
    with.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("arg `{key}` must be a string"))
}

fn req_bool(with: &Args, key: &str) -> Result<bool, String> {
    opt_bool(with, key).ok_or_else(|| format!("arg `{key}` must be a boolean"))
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
            // A step that declared `refused:` is an action in the schema, but it comes back with a
            // verdict the way an assert does. Naming it apart says which, so a red line under
            // `action` never reads as the driver having tripped over its own command.
            Step::Action { with, .. } if with.contains_key("refused") => "refused",
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
            // A step that carries no verdict of its own is marked apart, but a failure is a failure
            // whichever kind it came from — an action can fail too, now that one can be judged.
            let mark = if !l.pass {
                "✗"
            } else if l.kind == "action" {
                "·"
            } else {
                "✓"
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
