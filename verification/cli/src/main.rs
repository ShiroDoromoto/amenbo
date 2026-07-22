//! `verify-cli` — the CLI driver for pre-distribution verification (decision `AMB-D-345`).
//!
//! It reads one scenario (the single source of truth), maps each step to an invocation of the
//! **shipped / installed** `amenbo` binary, and judges the asserts from that binary's `--json`
//! output. It is a black-box driver: it knows the domain vocabulary, not the build under test.
//!
//! The run is isolated by `AMENBO_HOME` pointed at a throwaway store plus a `.amenbo`-free CWD
//! (see [`scratch`]); the real app-data is never touched. `AMENBO_UPDATE_CHECK=0` keeps it off
//! the network (its cache is not `AMENBO_HOME`-scoped, so a check would write into the real one).
//!
//! Usage: `verify-cli <scenario.yaml> [--bin <amenbo>] [--json] [--keep]`
//!   `--bin`  path to the amenbo binary to drive (default: `$AMENBO_BIN`, else `amenbo` on PATH)
//!   `--json` emit a machine-readable result instead of the human summary
//!   `--keep` leave the throwaway store in place for inspection
//!
//! Exit code is the machine signal: 0 when every assert passes, non-zero on any failed assert
//! or execution error — so a multi-scenario runner (a later task) reads it directly.

mod scratch;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use amenbo_scenario::{Args, Domain, Scenario, Step};

fn main() -> ExitCode {
    let opts = match Opts::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("verify-cli: {msg}");
            eprintln!("usage: verify-cli <scenario.yaml> [--bin <amenbo>] [--json] [--keep]");
            return ExitCode::from(2);
        }
    };

    match run(&opts) {
        Ok(report) => {
            if opts.json {
                println!("{}", report.to_json());
            } else {
                report.print_human();
            }
            if report.passed { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Err(msg) => {
            // An execution error (the scenario would not load, the binary would not run) is not
            // a scenario verdict — surface it plainly and fail.
            if opts.json {
                println!("{{\"scenario\":null,\"passed\":false,\"error\":{}}}", json_string(&msg));
            } else {
                eprintln!("verify-cli: {msg}");
            }
            ExitCode::FAILURE
        }
    }
}

/// Parsed command line.
struct Opts {
    scenario: PathBuf,
    bin: PathBuf,
    json: bool,
    keep: bool,
}

impl Opts {
    fn parse(args: impl Iterator<Item = String>) -> Result<Opts, String> {
        let mut scenario = None;
        let mut bin = std::env::var_os("AMENBO_BIN").map(PathBuf::from);
        let mut json = false;
        let mut keep = false;
        let mut it = args.peekable();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--json" => json = true,
                "--keep" => keep = true,
                "--bin" => bin = Some(PathBuf::from(it.next().ok_or("--bin needs a path")?)),
                s if s.starts_with("--") => return Err(format!("unknown flag `{s}`")),
                _ => {
                    if scenario.replace(PathBuf::from(a)).is_some() {
                        return Err("more than one scenario given".into());
                    }
                }
            }
        }
        Ok(Opts {
            scenario: scenario.ok_or("no scenario file given")?,
            bin: bin.unwrap_or_else(|| PathBuf::from("amenbo")),
            json,
            keep,
        })
    }
}

/// Load, isolate, execute, judge.
fn run(opts: &Opts) -> Result<Report, String> {
    let scenario = amenbo_scenario::lint_file(&opts.scenario)
        .map_err(|errs| format!("scenario does not load/validate:\n  {}", errs.join("\n  ")))?;

    let session = scratch::session(&scenario.id, opts.keep)
        .map_err(|e| format!("could not create a throwaway store: {e}"))?;
    let mut driver = Driver::new(&opts.bin, session)?;

    let mut report = Report::new(&scenario);
    for (i, step) in scenario.steps.iter().enumerate() {
        let outcome = driver.exec(step)?; // an execution error aborts the whole run
        report.push(i, step, outcome);
    }
    Ok(report)
}

/// Drives the shipped binary against one isolated store, remembering the ids that steps bind.
struct Driver {
    bin: PathBuf,
    session: scratch::Session,
    project_id: i64,
    bindings: HashMap<String, i64>,
}

impl Driver {
    /// Boot a fresh store: `init` creates it and hands back the project every `task add` needs.
    fn new(bin: &std::path::Path, session: scratch::Session) -> Result<Driver, String> {
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

struct Report {
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

    fn print_human(&self) {
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

    fn to_json(&self) -> String {
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
fn json_string(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}
