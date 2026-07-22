//! `verify-all` — the multi-scenario runner for pre-distribution verification.
//! It drives a whole set of scenarios through the CLI driver, one after another,
//! and rolls their verdicts into one: green only when every scenario is green, and the exit code
//! is that roll-up, so a release gate reads it directly.
//!
//! Each scenario runs in its own throwaway store (the same isolation `verify-cli` uses), so one
//! scenario never sees another's state. A scenario that fails to load or whose binary errors is
//! recorded as a red entry and the run continues — one broken scenario never hides the rest.
//!
//! Usage: `verify-all [<scenario-or-dir>...] [--bin <amenbo>] [--json] [--keep]`
//!   positional  scenario `.yaml` files and/or directories to scan (default: `scenarios/`)
//!   `--bin`     path to the amenbo binary to drive (default: `$AMENBO_BIN`, else `amenbo` on PATH)
//!   `--json`    emit a machine-readable aggregate instead of the human summary
//!   `--keep`    leave each throwaway store in place for inspection
//!
//! Exit code: 0 when every scenario is green, non-zero when any is red or errored.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use amenbo_verify_cli::{json_string, run_scenario, Report};

fn main() -> ExitCode {
    let opts = match Opts::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("verify-all: {msg}");
            eprintln!("usage: verify-all [<scenario-or-dir>...] [--bin <amenbo>] [--json] [--keep]");
            return ExitCode::from(2);
        }
    };

    let files = match collect_scenarios(&opts.inputs) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("verify-all: {msg}");
            return ExitCode::from(2);
        }
    };
    if files.is_empty() {
        eprintln!("verify-all: no scenario files found");
        return ExitCode::from(2);
    }

    let results: Vec<ScenarioResult> = files
        .iter()
        .map(|path| run_one(path, &opts.bin, opts.keep))
        .collect();

    let green = results.iter().all(|r| r.passed());
    if opts.json {
        println!("{}", aggregate_json(&results, green));
    } else {
        print_human(&results, green);
    }
    if green { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// One scenario's outcome: either it ran to a verdict, or it never got that far (would not
/// load, or its binary errored). Both count as failures for the roll-up.
enum ScenarioResult {
    Ran { report: Report },
    Errored { path: PathBuf, error: String },
}

impl ScenarioResult {
    fn passed(&self) -> bool {
        matches!(self, ScenarioResult::Ran { report, .. } if report.passed())
    }
}

/// Load and run one scenario, folding a load or execution failure into an `Errored` entry so the
/// runner carries on to the next.
fn run_one(path: &Path, bin: &Path, keep: bool) -> ScenarioResult {
    let scenario = match amenbo_scenario::lint_file(path) {
        Ok(s) => s,
        Err(errs) => {
            return ScenarioResult::Errored {
                path: path.to_path_buf(),
                error: format!("does not load/validate: {}", errs.join("; ")),
            };
        }
    };
    match run_scenario(&scenario, bin, keep) {
        Ok(report) => ScenarioResult::Ran { report },
        Err(error) => ScenarioResult::Errored { path: path.to_path_buf(), error },
    }
}

/// Expand the inputs (files and/or directories) into a sorted, de-duplicated list of scenario
/// files. With no inputs, scan the default `scenarios/` directory.
fn collect_scenarios(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if inputs.is_empty() {
        scan_dir(Path::new("scenarios"), &mut files)?;
    } else {
        for input in inputs {
            if input.is_dir() {
                scan_dir(input, &mut files)?;
            } else if input.is_file() {
                files.push(input.clone());
            } else {
                return Err(format!("no such file or directory: {}", input.display()));
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// Collect every `.yaml` / `.yml` file directly under `dir` (not recursive — the scenario set is
/// flat by design).
fn scan_dir(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read directory {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?.path();
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "yaml" || e == "yml");
        if path.is_file() && is_yaml {
            out.push(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

fn print_human(results: &[ScenarioResult], green: bool) {
    let mut passed = 0usize;
    for r in results {
        match r {
            ScenarioResult::Ran { report, .. } => {
                let mark = if report.passed() { "✓" } else { "✗" };
                if report.passed() {
                    passed += 1;
                }
                // The human line leans on the scenario id; the file path is kept for the JSON form.
                println!("{mark} {} — {}", report.scenario_id(), report.title());
            }
            ScenarioResult::Errored { path, error } => {
                println!("✗ {} — ERROR: {error}", path.display());
            }
        }
    }
    println!("---");
    println!(
        "{}/{} scenarios green — VERDICT: {}",
        passed,
        results.len(),
        if green { "green" } else { "red" }
    );
}

/// A machine-readable aggregate: the roll-up plus each scenario's own report (or its error).
fn aggregate_json(results: &[ScenarioResult], green: bool) -> String {
    let passed = results.iter().filter(|r| r.passed()).count();
    let items: Vec<String> = results
        .iter()
        .map(|r| match r {
            ScenarioResult::Ran { report, .. } => report.to_json(),
            ScenarioResult::Errored { path, error } => format!(
                "{{\"scenario\":null,\"path\":{},\"passed\":false,\"error\":{}}}",
                json_string(&path.display().to_string()),
                json_string(error)
            ),
        })
        .collect();
    format!(
        "{{\"total\":{},\"passed\":{},\"failed\":{},\"green\":{},\"scenarios\":[{}]}}",
        results.len(),
        passed,
        results.len() - passed,
        green,
        items.join(",")
    )
}

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

struct Opts {
    inputs: Vec<PathBuf>,
    bin: PathBuf,
    json: bool,
    keep: bool,
}

impl Opts {
    fn parse(args: impl Iterator<Item = String>) -> Result<Opts, String> {
        let mut inputs = Vec::new();
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
                _ => inputs.push(PathBuf::from(a)),
            }
        }
        Ok(Opts {
            inputs,
            bin: bin.unwrap_or_else(|| PathBuf::from("amenbo")),
            json,
            keep,
        })
    }
}
