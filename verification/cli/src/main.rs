//! `verify-cli` — the CLI driver for pre-distribution verification.
//!
//! It reads one scenario (the single source of truth), maps each step to an invocation of the
//! **shipped / installed** `amenbo` binary, and judges the asserts from that binary's `--json`
//! output. It is a black-box driver: it knows the domain vocabulary, not the build under test.
//! The driver, the throwaway-store isolation and the reporting all live in the crate library; a
//! whole set of scenarios is run and aggregated by the sibling bin `verify-all`.
//!
//! Usage: `verify-cli <scenario.yaml> [--bin <amenbo>] [--json] [--keep]`
//!   `--bin`  path to the amenbo binary to drive (default: `$AMENBO_BIN`, else `amenbo` on PATH)
//!   `--json` emit a machine-readable result instead of the human summary
//!   `--keep` leave the throwaway store in place for inspection
//!
//! Exit code is the machine signal: 0 when every assert passes, non-zero on any failed assert
//! or execution error — so a multi-scenario runner reads it directly.

use std::path::PathBuf;
use std::process::ExitCode;

use amenbo_verify_cli::{json_string, run_scenario};

fn main() -> ExitCode {
    let opts = match Opts::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("verify-cli: {msg}");
            eprintln!("usage: verify-cli <scenario.yaml> [--bin <amenbo>] [--json] [--keep]");
            return ExitCode::from(2);
        }
    };

    let loaded = amenbo_scenario::lint_file(&opts.scenario)
        .map_err(|errs| format!("scenario does not load/validate:\n  {}", errs.join("\n  ")));

    let result = loaded.and_then(|scenario| run_scenario(&scenario, &opts.bin, opts.keep));

    match result {
        Ok(report) => {
            if opts.json {
                println!("{}", report.to_json());
            } else {
                report.print_human();
            }
            if report.passed() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
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
