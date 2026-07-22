//! `lint` — validate one or more scenario files.
//!
//! Usage: `lint <file.yaml> [more.yaml ...]`, or `lint` with no args to lint every
//! `*.yaml` under `verification/scenarios/`. Prints one line per file and exits non-zero
//! if any file fails to parse or validate, so it drops cleanly into a make target or CI.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use amenbo_scenario::lint_file;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let files = if args.is_empty() {
        default_scenarios()
    } else {
        args.into_iter().map(PathBuf::from).collect()
    };

    if files.is_empty() {
        eprintln!("no scenarios to lint (looked under verification/scenarios/)");
        return ExitCode::FAILURE;
    }

    let mut failed = 0usize;
    for path in &files {
        match lint_file(path) {
            Ok(s) => println!("ok    {}  ({} step(s): {})", path.display(), s.steps.len(), s.id),
            Err(errs) => {
                failed += 1;
                println!("FAIL  {}", path.display());
                for e in errs {
                    println!("        {e}");
                }
            }
        }
    }

    println!("\n{} file(s), {} failed", files.len(), failed);
    if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Every `*.yaml` under `verification/scenarios/`, resolved relative to this crate so the
/// binary lints the real source tree whatever the CWD.
fn default_scenarios() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // verification/
        .map(|p| p.join("scenarios"))
        .unwrap_or_else(|| PathBuf::from("scenarios"));
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("yaml") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}
