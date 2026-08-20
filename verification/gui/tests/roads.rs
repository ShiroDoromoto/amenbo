//! Every screen road in `verification/scenarios/` must render in this harness.
//!
//! The registry is closed and the mapping fails closed, so an op written into a `steps_gui` road but
//! never mapped here is caught — but only when someone runs that scenario, which is once before a
//! release, in front of a screen, by hand. This asks the same question without any of that.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use amenbo_scenario::{Domain, Driver, Scenario, Step};

fn scenarios_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("verification/")
        .join("scenarios")
}

#[test]
fn every_screen_road_renders_into_instructions() {
    let mut roads = 0;
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f)
            .unwrap_or_else(|errs| panic!("{} does not lint: {}", f.display(), errs.join("\n")));
        if !scenario.runs_on(Driver::Gui) {
            continue;
        }
        roads += 1;
        amenbo_verify_gui::instructions(&scenario).unwrap_or_else(|e| {
            panic!("{} carries a screen road this harness cannot render: {e}", f.display())
        });
    }
    assert!(roads > 0, "expected at least one screen road");
}

/// A screen road that says "open the project X" is telling an operator to find X in a list, so X has
/// to be there. Nothing else checks it: the name travels as a word rather than as a binding, and a
/// step naming a project nobody raised renders, lints and walks — the operator is the one who finds
/// out, mid-run, hunting a list for a name that was never in it.
///
/// What may be named is a project the world stood up, or the one a run finds itself in already
/// (Amenbo raises a project for the folder the run works in, and calls it after that folder).
#[test]
fn every_project_a_screen_road_opens_is_a_project_that_exists() {
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f).expect("lints");
        if !scenario.runs_on(Driver::Gui) {
            continue;
        }
        let standing = projects_standing(&scenario);
        for step in scenario.steps(Driver::Gui) {
            let Step::Action { domain, op, with, .. } = step else { continue };
            if (*domain, op.as_str()) != (Domain::Project, "open") {
                continue;
            }
            let named = with.get("project").and_then(|v| v.as_str()).unwrap_or_default();
            assert!(
                standing.contains(named),
                "{} opens the project `{named}`, which nothing stands up — the world raises {:?}, \
                 and a run is already in `{}`",
                f.display(),
                standing,
                amenbo_verify_cli::scratch::CWD_DIR
            );
        }
    }
}

/// The projects an operator could find in the list when a road opens: whatever the world raised, and
/// the one the run is standing in before it raised anything.
fn projects_standing(scenario: &Scenario) -> HashSet<String> {
    let mut names: HashSet<String> = HashSet::new();
    names.insert(amenbo_verify_cli::scratch::CWD_DIR.to_string());
    for step in &scenario.given {
        if let Step::Action { domain, op, with, .. } = step {
            if (*domain, op.as_str()) == (Domain::Project, "create") {
                if let Some(name) = with.get("name").and_then(|v| v.as_str()) {
                    names.insert(name.to_string());
                }
            }
        }
    }
    names
}

fn scenario_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(scenarios_dir())
        .expect("scenarios dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yaml"))
        .collect();
    files.sort();
    files
}
