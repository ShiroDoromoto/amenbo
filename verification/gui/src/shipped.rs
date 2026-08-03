//! Which bundle a run is allowed to launch.
//!
//! Pre-distribution verification is how a release earns its promotion, so the screen it shoots has
//! to be the one the shipped bytes draw. A window says nothing about that: the app under test and
//! a build made here draw the same screen, wear the same version number, and — apart from the dev
//! channel's own name — are one app in two places on disk.
//!
//! The bundle is asked instead, through the CLI it ships. One installer carries both faces and one
//! build produces them together (`app/scripts/prepare-cli-sidecar.mjs` stages the CLI into the
//! bundle as the app is built), so the sidecar's stamp is the bundle's provenance — and a stamp is
//! something a binary can be asked for, which a window is not. What it answers is `release_build`,
//! from the `version` face's `--json`.
//!
//! There is no flag to say "I know, launch it anyway": an exception would put shots of a shipped
//! build and shots of somebody's working tree in one evidence directory, under one manifest,
//! telling one story — and a promotion resting on the second is a promotion resting on nothing.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::scratch;

/// Refuse a bundle the release workflow did not produce. `Ok(())` is the only way past — the answer
/// comes from the bundle's own CLI, so a bundle that cannot be asked is refused with what went
/// wrong rather than launched anyway.
pub fn ensure_release_build(bundle: &Path) -> Result<(), String> {
    let cli = sidecar(bundle)?;
    if release_build(&cli)? {
        return Ok(());
    }
    Err(format!(
        "`{}` did not come out of the release workflow, and verification shoots what ships. Point \
         `--app` at an installed amenbo.app, or at the bundle the workflow built.",
        bundle.display()
    ))
}

/// The CLI inside a mac app bundle. Named rather than searched for: the installer puts one CLI in
/// one place, and a harness that went looking would be reading whichever amenbo it found first.
///
/// It answers two questions with one path. Which build this is, above — and, once that is settled,
/// what the run stands the scenario's world up with: the world a road starts from is raised by the
/// bundle under test, never by whichever amenbo the operator has on `PATH`.
pub fn sidecar(bundle: &Path) -> Result<PathBuf, String> {
    let cli = bundle.join("Contents/MacOS/amenbo");
    if !cli.is_file() {
        return Err(format!(
            "`{}` ships no CLI at {} — every installed amenbo carries one, so this is not a bundle \
             a run can ask about",
            bundle.display(),
            cli.display()
        ));
    }
    Ok(cli)
}

/// Ask that CLI which build it is. `Err` is "the question could not be answered" — it would not
/// run, said nothing a reader could parse, or predates the stamp being reported at all.
///
/// It is asked in a store of the run's own, and not the one the app is about to be launched
/// against: the `version` face opens a store where it can, and a probe that opened *that* one would
/// hand the app a store somebody had already been in — which is the one thing a road through the
/// setup screens is written to walk out of.
fn release_build(cli: &Path) -> Result<bool, String> {
    let probe = scratch::session("build-stamp", false)
        .map_err(|e| format!("could not create a throwaway store to ask which build it is: {e}"))?;
    let out = Command::new(cli)
        .args(["version", "--json"])
        .current_dir(&probe.cwd)
        .env("AMENBO_HOME", &probe.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .output()
        .map_err(|e| format!("could not run `{}` to ask which build it is: {e}", cli.display()))?;
    if !out.status.success() {
        return Err(format!(
            "`{} version --json` failed ({}): {}",
            cli.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let said: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("`{} version --json` did not answer in JSON: {e}", cli.display()))?;
    said["release_build"].as_bool().ok_or_else(|| {
        format!(
            "`{} version --json` does not say which build it is — no `release_build` in its answer",
            cli.display()
        )
    })
}

// The subject here is a bundle being asked a question, so the stand-ins are bundle-shaped
// directories carrying a script that answers it. Unix shell, which is where this harness runs.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A bundle in the shape this reads: a CLI where the installer puts one, answering `version
    /// --json` with `body`.
    fn bundle(dir: &Path, body: &str) -> PathBuf {
        let app = dir.join("stand-in.app");
        let macos = app.join("Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        let cli = macos.join("amenbo");
        std::fs::write(&cli, format!("#!/bin/sh\ncat <<'EOF'\n{body}\nEOF\n")).unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        app
    }

    /// The stamped bundle is the one that gets launched.
    #[test]
    fn a_release_bundle_is_launched() {
        let d = scratch::session("selftest-shipped-yes", false).unwrap();
        let app = bundle(&d.cwd, r#"{"version":"5.3.0","release_build":true}"#);
        assert!(ensure_release_build(&app).is_ok());
    }

    /// The bundle somebody built themselves is turned away, and the refusal names it — a machine
    /// carries several amenbos, and which one was refused is the answer the operator needs.
    #[test]
    fn a_local_bundle_is_refused_by_name() {
        let d = scratch::session("selftest-shipped-no", false).unwrap();
        let app = bundle(&d.cwd, r#"{"version":"5.3.0","release_build":false}"#);
        let err = ensure_release_build(&app).unwrap_err();
        assert!(err.contains("stand-in.app"), "the refusal names the bundle it was handed: {err}");
        assert!(err.contains("release workflow"), "and says what it is missing: {err}");
    }

    /// A bundle that cannot answer is refused too, and so is a directory carrying no CLI at all: a
    /// question left unanswered is not a yes.
    #[test]
    fn a_bundle_that_will_not_answer_is_refused_with_why() {
        let d = scratch::session("selftest-shipped-mute", false).unwrap();
        let older = bundle(&d.cwd, r#"{"version":"5.3.0"}"#);
        let err = ensure_release_build(&older).unwrap_err();
        assert!(err.contains("release_build"), "the field that was missing is named: {err}");

        let empty = d.cwd.join("empty.app");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(ensure_release_build(&empty).unwrap_err().contains("ships no CLI"));
    }
}
