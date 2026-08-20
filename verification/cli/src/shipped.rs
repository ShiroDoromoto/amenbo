//! Which binary a run is allowed to drive.
//!
//! Pre-distribution verification is how a release earns its promotion, so what it walks has to be
//! the bytes that ship. Nothing about a binary's path says that: `amenbo` on `PATH` is whatever was
//! installed last, a debug build carries the released version number, and a build made here answers
//! to the production channel unless it was built for the dev one. The one fact that separates them
//! is the release workflow's stamp, and the binary reports it — `release_build` in what the `version`
//! face answers with `--json`.
//!
//! So the driver asks, before it drives. There is no flag to say "I know, run it anyway": an
//! exception would put evidence gathered from a shipped build and evidence gathered from somebody's
//! working tree in the same directory, under the same manifest, telling the same story — and a
//! promotion resting on the second is a promotion resting on nothing.
//!
//! Asking costs one invocation, and it is asked in a store of the run's own like every other: the
//! `version` face opens one where it can, and a probe that let it open the operator's would be
//! reading a real backlog to find out what build it is talking to.

use std::path::Path;
use std::process::Command;

use crate::scratch;

/// Refuse a binary the release workflow did not produce. `Ok(())` is the only way past — the answer
/// is read from the binary itself, so a build that cannot be asked is refused with what went wrong
/// rather than waved through.
pub fn ensure_release_build(bin: &Path) -> Result<(), String> {
    if release_build(bin)? {
        return Ok(());
    }
    Err(format!(
        "`{}` did not come out of the release workflow, and verification drives what ships. Point \
         `--bin` at an installed Amenbo, or at the artifact the workflow built.",
        bin.display()
    ))
}

/// Ask the binary which build it is. `Err` is "the question could not be answered" — the binary
/// would not run, said nothing a reader could parse, or predates the stamp being reported at all.
fn release_build(bin: &Path) -> Result<bool, String> {
    let probe = scratch::session("build-stamp", false)
        .map_err(|e| format!("could not create a throwaway store to ask which build it is: {e}"))?;
    let out = Command::new(bin)
        .args(["version", "--json"])
        .current_dir(&probe.cwd)
        .env("AMENBO_HOME", &probe.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .output()
        .map_err(|e| format!("could not run `{}` to ask which build it is: {e}", bin.display()))?;
    if !out.status.success() {
        return Err(format!(
            "`{} version --json` failed ({}): {}",
            bin.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let said: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("`{} version --json` did not answer in JSON: {e}", bin.display()))?;
    said["release_build"].as_bool().ok_or_else(|| {
        format!(
            "`{} version --json` does not say which build it is — no `release_build` in its answer",
            bin.display()
        )
    })
}

// The subject here is a binary being asked a question, so the stand-ins are scripts that answer it.
// They are unix shell, which is where this harness runs: a mac by hand, and ubuntu on CI.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// A stand-in binary that answers `version --json` with `body` on stdout, under `code`.
    fn stand_in(dir: &Path, body: &str, code: i32) -> PathBuf {
        let bin = dir.join("amenbo");
        std::fs::write(&bin, format!("#!/bin/sh\ncat <<'EOF'\n{body}\nEOF\nexit {code}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    /// A place to put a stand-in that is not the repository the harness is run from.
    fn dir(tag: &str) -> scratch::Session {
        scratch::session(tag, false).unwrap()
    }

    /// The stamped build is the one that gets through.
    #[test]
    fn a_release_build_is_driven() {
        let d = dir("selftest-shipped-yes");
        let bin = stand_in(&d.cwd, r#"{"version":"5.3.0","release_build":true}"#, 0);
        assert!(ensure_release_build(&bin).is_ok());
    }

    /// The build somebody made themselves is turned away, and the refusal names it — the operator is
    /// holding several amenbos and has to be told which one was refused.
    #[test]
    fn a_local_build_is_refused_by_name() {
        let d = dir("selftest-shipped-no");
        let bin = stand_in(&d.cwd, r#"{"version":"5.3.0","release_build":false}"#, 0);
        let err = ensure_release_build(&bin).unwrap_err();
        assert!(err.contains("amenbo"), "the refusal names the binary it was handed: {err}");
        assert!(err.contains("release workflow"), "and says what it is missing: {err}");
    }

    /// A binary that cannot answer the question is refused too. An unreadable answer is not a `false`
    /// — it is a driver that does not know what it is about to walk, which is the thing to say.
    #[test]
    fn a_binary_that_will_not_answer_is_refused_with_why() {
        let d = dir("selftest-shipped-mute");
        let older = stand_in(&d.cwd, r#"{"version":"5.3.0"}"#, 0);
        let err = ensure_release_build(&older).unwrap_err();
        assert!(err.contains("release_build"), "the field that was missing is named: {err}");

        let broken = stand_in(&d.cwd, "not json at all", 0);
        assert!(ensure_release_build(&broken).unwrap_err().contains("JSON"));

        let dead = stand_in(&d.cwd, "", 1);
        assert!(ensure_release_build(&dead).unwrap_err().contains("failed"));

        assert!(ensure_release_build(&d.cwd.join("nothing-here")).unwrap_err().contains("could not run"));
    }
}
