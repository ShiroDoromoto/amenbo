//! The real scenarios under `verification/scenarios/` must all lint, and the invalid
//! fixtures must each be rejected — the loader's job is to fail closed.

use std::path::{Path, PathBuf};

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn scenarios_dir() -> PathBuf {
    manifest_dir().parent().unwrap().join("scenarios")
}

fn yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yaml"))
        .collect();
    out.sort();
    out
}

#[test]
fn every_real_scenario_lints() {
    let files = yaml_files(&scenarios_dir());
    assert!(!files.is_empty(), "expected at least one scenario");
    for f in files {
        amenbo_scenario::lint_file(&f)
            .unwrap_or_else(|errs| panic!("{} should lint but failed:\n{}", f.display(), errs.join("\n")));
    }
}

/// The fixture check is the one that reads the disk, so it is worth naming what it says rather than
/// leaving it inside the sweep above: a road can name a file that is not there and be right about
/// everything else, which is exactly how one reached the tree.
#[test]
fn a_road_naming_a_fixture_nobody_put_there_is_rejected() {
    let f = manifest_dir().join("tests").join("fixtures").join("invalid-fixture-not-there.yaml");
    let errs = amenbo_scenario::lint_file(&f).expect_err("a fixture that is not there is refused");
    assert!(
        errs.iter().any(|e| e.contains("nobody-put-this-here.bin")),
        "the line names the file that is missing: {errs:?}"
    );
}

#[test]
fn every_invalid_fixture_is_rejected() {
    let dir = manifest_dir().join("tests").join("fixtures");
    let files = yaml_files(&dir);
    assert!(!files.is_empty(), "expected invalid fixtures");
    for f in files {
        assert!(
            amenbo_scenario::lint_file(&f).is_err(),
            "{} is an invalid fixture but linted clean",
            f.display()
        );
    }
}
