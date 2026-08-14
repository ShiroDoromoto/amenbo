//! The project a terminal road names has to be one the run is standing in a folder for.
//!
//! A setting is held per crossing, and down this pipe the crossing is answered by where the command
//! is typed — so a step naming one is asking the driver to stand in that project's folder. The name
//! travels as a word rather than as a binding: a misspelt one lints, and a road that left it out
//! altogether lints too, and both walk. The driver catches it when it runs, which is once before a
//! release with a store and a shipped binary standing; this asks the same question in a second.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use amenbo_scenario::{Domain, Driver, Step};

fn scenarios_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("verification/").join("scenarios")
}

fn scenario_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(scenarios_dir())
        .expect("verification/scenarios is readable")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "expected scenarios to read");
    files
}

/// The settings steps that name the crossing they are held at — the two ops that take a `project:`
/// down this road.
fn names_a_crossing(step: &Step) -> Option<&str> {
    let (domain, op, with) = match step {
        Step::Action { domain, op, with, .. } => (domain, op, with),
        Step::Assert { domain, op, with } => (domain, op, with),
    };
    if *domain != Domain::Plugin || !matches!(op.as_str(), "config-set" | "config") {
        return None;
    }
    with.get("project").and_then(|v| v.as_str())
}

#[test]
fn every_crossing_a_settings_step_names_is_a_project_that_stands() {
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f)
            .unwrap_or_else(|errs| panic!("{} does not lint: {}", f.display(), errs.join("\n")));
        // The project a run finds itself in before anything is raised: amenbo raises one for the
        // folder the run works in, and calls it after that folder.
        let mut standing: HashSet<&str> =
            [amenbo_verify_cli::scratch::CWD_DIR].into_iter().collect();
        // The premise first and the road after it, in the order a run walks them — a project is
        // somewhere to stand only once something has raised it.
        let walked = scenario.given.iter().chain(scenario.steps(Driver::Cli).iter());
        for step in walked {
            if let Some(named) = names_a_crossing(step) {
                assert!(
                    standing.contains(named),
                    "{} holds a setting at the crossing `{named}`, which nothing has raised — \
                     by here the run is standing in {:?}",
                    f.display(),
                    standing
                );
            }
            if let Step::Action { domain, op, with, .. } = step {
                if (*domain, op.as_str()) == (Domain::Project, "create") {
                    if let Some(name) = with.get("name").and_then(|v| v.as_str()) {
                        standing.insert(name);
                    }
                }
            }
        }
    }
}
