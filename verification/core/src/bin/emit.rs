//! `emit` — load and validate ONE scenario file, then print it as JSON on stdout.
//!
//! The scenario source is YAML, but a shell consumer reads it through this crate rather
//! than reparsing YAML by hand — `emit` is the crate's JSON face over the *validated*
//! model, so `jq` can pick the one field a driver needs. Its caller is the Linux OCR
//! harness's host-side launcher (`make verify-gui-linux`): the linux-gui-e2e container
//! carries no toolchain, so the host resolves the scenario here and hands the derived card
//! title into the container.
//!
//! Usage: `emit <scenario.yaml>` (exactly one file). Non-zero exit on load, validation, or
//! serialization failure, so a `$(...)` capture never silently yields a half-scenario.

use std::process::ExitCode;

use amenbo_scenario::lint_file;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        eprintln!("usage: emit <scenario.yaml>  (exactly one file)");
        return ExitCode::FAILURE;
    };

    let scenario = match lint_file(path) {
        Ok(s) => s,
        Err(errs) => {
            eprintln!("FAIL  {path}");
            for e in errs {
                eprintln!("        {e}");
            }
            return ExitCode::FAILURE;
        }
    };

    match serde_json::to_string(&scenario) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("serialize failed: {e}");
            ExitCode::FAILURE
        }
    }
}
