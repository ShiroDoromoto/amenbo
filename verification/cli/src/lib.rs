//! The CLI driver's reusable core: drive the **shipped / installed**
//! `amenbo` binary against one scenario in an isolated throwaway store, and judge the asserts
//! from that binary's `--json` output. A black-box driver — it knows the domain vocabulary, not
//! the build under test.
//!
//! Two bins sit on top of this: `verify-cli` runs one scenario, `verify-all` runs a whole set
//! and aggregates. Both call [`run_scenario`] and read a [`Report`]; the isolation, the driver
//! and the reporting live here so the two share one contract.

mod scratch;

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
        let out = Command::new(&self.bin)
            .args(args)
            .current_dir(&self.session.cwd)
            .env("AMENBO_HOME", &self.session.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("AMENBO_ACTOR", "human")
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
                self.run_json(&["comment", "add", &target.to_string(), "--text", text, "--json"])?;
                Ok(Outcome::action(format!("commented on task {target}")))
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
            _ => Err(unmapped(domain, op)),
        }
    }

    fn assert(&self, domain: Domain, op: &str, with: &Args) -> Result<Outcome, String> {
        match (domain, op) {
            (Domain::Task, "listed") => {
                let target = self.resolve(with)?;
                let filter = req_str(with, "filter")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["task", "list", "--filter", filter, "--json"])?;
                let found = v["tasks"]
                    .as_array()
                    .map(|a| a.iter().any(|t| t["id"].as_i64() == Some(target)))
                    .unwrap_or(false);
                let pass = found == present;
                let word = if present { "present in" } else { "absent from" };
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "task {target} {} `{filter}` (expected {word}, {})",
                        if found { "is present in" } else { "is absent from" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            (Domain::Task, "field") => {
                let target = self.resolve(with)?;
                let field = req_str(with, "field")?;
                // `equals` is any scalar (string / bool / number / null); compare it structurally
                // against the field's JSON value, so `status: todo` and `completed: false` both work.
                let expected = with.get("equals").ok_or("arg `equals` is required")?;
                let expected = serde_json::to_value(expected)
                    .map_err(|e| format!("arg `equals` is not a valid value: {e}"))?;
                let v = self.run_json(&["task", "show", &target.to_string(), "--json"])?;
                match v.get(field) {
                    None => Ok(Outcome::assert(
                        false,
                        format!("task {target} has no field `{field}` in its show output (MISMATCH)"),
                    )),
                    Some(actual) => {
                        let pass = *actual == expected;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "task {target} field `{field}` = {actual} (expected {expected}, {})",
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            _ => Err(unmapped(domain, op)),
        }
    }

    /// Resolve a step's `target:` to the id an earlier action bound. The loader already proved
    /// the name resolves to an earlier `as:`, so a miss here is an internal error, not user input.
    fn resolve(&self, with: &Args) -> Result<i64, String> {
        let name = req_str(with, "target")?;
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
