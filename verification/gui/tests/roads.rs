//! Every screen road in `verification/scenarios/` must render in this harness.
//!
//! The registry is closed and the mapping fails closed, so an op written into a `steps_gui` road but
//! never mapped here is caught — but only when someone runs that scenario, which is once before a
//! release, in front of a screen, by hand. This asks the same question without any of that.

use std::path::{Path, PathBuf};

use amenbo_scenario::Driver;

fn scenarios_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("verification/")
        .join("scenarios")
}

#[test]
fn every_screen_road_renders_into_instructions() {
    let mut roads = 0;
    let mut files: Vec<PathBuf> = std::fs::read_dir(scenarios_dir())
        .expect("scenarios dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yaml"))
        .collect();
    files.sort();

    for f in files {
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
