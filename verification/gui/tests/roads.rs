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
        let scenario = amenbo_scenario::lint_file(&f, None)
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

/// The two steps that name a project by word rather than by binding: opening one on the ledger, and
/// going to one on the terminal face's rail. Both tell an operator to find that name in a list, so it
/// has to be in one. Nothing else checks it — a step naming a project nobody raised renders, lints and
/// walks, and the operator is the one who finds out, mid-run, hunting a list for a name that was never
/// in it.
///
/// What may be named is a project the world stood up, or the one a run finds itself in already
/// (Amenbo raises a project for the folder the run works in, and calls it after that folder).
#[test]
fn every_project_a_screen_road_opens_is_a_project_that_exists() {
    let by_name = [(Domain::Project, "open"), (Domain::Terminal, "go-project")];
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f, None).expect("lints");
        if !scenario.runs_on(Driver::Gui) {
            continue;
        }
        let standing = projects_standing(&scenario);
        for step in scenario.steps(Driver::Gui) {
            let Step::Action { domain, op, with, .. } = step else { continue };
            if !by_name.contains(&(*domain, op.as_str())) {
                continue;
            }
            let named = with.get("project").and_then(|v| v.as_str()).unwrap_or_default();
            assert!(
                standing.contains(named),
                "{} goes to the project `{named}`, which nothing stands up — the world raises {:?}, \
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
///
/// Walked in order, because a premise can take them all away again: `store nothing-raised` leaves a
/// device with none, and a road opening on that names no project at all.
fn projects_standing(scenario: &Scenario) -> HashSet<String> {
    let mut names: HashSet<String> = HashSet::new();
    names.insert(amenbo_verify_cli::scratch::CWD_DIR.to_string());
    for step in &scenario.given {
        if let Step::Action { domain, op, with, .. } = step {
            match (*domain, op.as_str()) {
                (Domain::Project, "create") => {
                    if let Some(name) = with.get("name").and_then(|v| v.as_str()) {
                        names.insert(name.to_string());
                    }
                }
                (Domain::Store, "nothing-raised") => names.clear(),
                _ => {}
            }
        }
    }
    names
}

/// A premise that takes the device back to nothing raised on it leaves no project to name — the
/// boot's own included. Held here rather than left to the scan above, because no scenario walks that
/// world yet: the roads it exists for are written against ops that are still being added, and until
/// one of them lands the rule would be a line nothing ever reaches.
#[test]
fn a_premise_that_empties_the_device_leaves_no_project_to_name() {
    let emptied = amenbo_scenario::load_str(
        r#"
id: x
title: y
given:
  - type: action
    domain: project
    op: create
    with: { name: Greenhouse }
  - type: action
    domain: store
    op: nothing-raised
steps_gui:
  - type: action
    domain: project
    op: create
    with: { name: Seedbed }
"#,
    )
    .expect("loads");
    assert!(
        projects_standing(&emptied).is_empty(),
        "nothing stands on a device the premise emptied — not what it raised, and not the boot's own"
    );

    let raised = amenbo_scenario::load_str(
        r#"
id: x
title: y
given:
  - type: action
    domain: store
    op: nothing-raised
  - type: action
    domain: project
    op: create
    with: { name: Greenhouse }
steps_gui:
  - type: action
    domain: project
    op: create
    with: { name: Seedbed }
"#,
    )
    .expect("loads");
    assert_eq!(
        projects_standing(&raised),
        ["Greenhouse".to_string()].into_iter().collect::<HashSet<String>>(),
        "and what the premise raises after it does stand — the emptying is a moment, not a mode"
    );
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
