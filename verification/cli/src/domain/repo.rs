//! The `repo` domain: the folder the run works in, rather than anything in the store. The files a
//! person already has lying there, the git repository the lint hooks stand in front of, and the
//! two gates that read them.

use std::path::Path;
use std::process::Command;

use amenbo_scenario::{Args, Domain};

use crate::{req_str, unmapped, Driver, Outcome};

impl Driver {
    pub(crate) fn repo_action(&mut self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // The folder the run works in. `write-file` is a person already having a file there —
            // what gets attached, and what the lint is pointed at.
            "write-file" => {
                let path = req_str(with, "path")?;
                let content = req_str(with, "content")?;
                let full = self.in_session(path)?;
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                std::fs::write(&full, content).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Outcome::action(format!("wrote {path} ({} bytes)", content.len())))
            }
            // The same, for text a scenario cannot hold itself. A file under `fixtures/` is where the
            // reference form lives: this tree's prose rule keeps a bare ref out of every `.yaml`, and
            // the lint has nothing to find unless something really carries one.
            "copy-fixture" => {
                let from = req_str(with, "from")?;
                let path = req_str(with, "path")?;
                if Path::new(from).is_absolute()
                    || Path::new(from).components().any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(format!("`from: {from}` must name a file under fixtures/"));
                }
                let src = fixtures_dir().join(from);
                let full = self.in_session(path)?;
                let bytes = std::fs::read(&src)
                    .map_err(|e| format!("could not read the fixture {}: {e}", src.display()))?;
                std::fs::write(&full, &bytes).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Outcome::action(format!("copied the fixture {from} to {path} ({} bytes)", bytes.len())))
            }
            // The hooks are written into a git repository, so the scenario has to stand one up first.
            // This is the one step that is not amenbo — everything it proves is about what amenbo
            // then does to a repository that is really there.
            //
            // It leaves a `main` with one commit on it, rather than the branchless state a bare
            // `init` leaves behind: a repository with no commit has no branch either, and the
            // official `worktree` plugin needs one to cut a task's checkout from.
            "git-init" => {
                let git = |args: &[&str]| -> Result<(), String> {
                    let out = Command::new("git")
                        .args(args)
                        .current_dir(&self.session.cwd)
                        .output()
                        .map_err(|e| format!("could not run git: {e}"))?;
                    if !out.status.success() {
                        return Err(format!(
                            "`git {}` failed: {}",
                            args.join(" "),
                            String::from_utf8_lossy(&out.stderr).trim()
                        ));
                    }
                    Ok(())
                };
                git(&["init", "-q", "--initial-branch", "main"])?;
                // Named on the command line rather than left to the machine's git config: a box with
                // no identity set would fail here, and neither name belongs to anybody.
                git(&[
                    "-c", "user.name=verify",
                    "-c", "user.email=verify@example.invalid",
                    "commit", "--quiet", "--allow-empty",
                    "-m", "the branch a scenario cuts from",
                ])?;
                Ok(Outcome::action("made the run's folder a git repository on `main`".to_string()))
            }
            verb @ ("hooks-install" | "hooks-uninstall") => {
                let sub = verb.trim_start_matches("hooks-");
                self.run_json(&["hooks", sub, "--yes", "--json"])?;
                Ok(Outcome::action(format!("ran `hooks {sub}` on the run's repository")))
            }
            _ => Err(unmapped(Domain::Repo, op)),
        }
    }
    pub(crate) fn repo_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "lint" => {
                let path = req_str(with, "path")?;
                self.in_session(path)?;
                let want =
                    with.get("hits").and_then(|v| v.as_u64()).ok_or("arg `hits` must be a number")?;
                // Finding something is how the lint reports — exit code included — so a non-zero
                // exit here is its verdict rather than a failure to run.
                let v = self.run_check(&["lint", path, "--json"])?;
                let hits = v["hits"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                // A count alone would not say the report locates anything, and the ref itself cannot be
                // written into a scenario (this tree's prose rule keeps a bare one out of every
                // `.yaml`), so what a line asks for instead is the line number it was found on.
                let at = with.get("line").and_then(|v| v.as_u64());
                let located = match at {
                    Some(n) => hits.iter().any(|h| h["line"].as_u64() == Some(n)),
                    None => true,
                };
                let pass = hits.len() as u64 == want && located;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "lint reports {} ref(s) in {path}{} (expected {want}, {})",
                        hits.len(),
                        at.map(|n| format!(", one of them on line {n}")).unwrap_or_default(),
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "hooks" => {
                let hook = req_str(with, "hook")?;
                let want = req_str(with, "state")?;
                let v = self.run_json(&["hooks", "status", "--json"])?;
                let slot = v["hooks"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .find(|h| h["hook"].as_str() == Some(hook));
                let state = slot.and_then(|h| h["state"]["kind"].as_str()).unwrap_or("no slot");
                let pass = state == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "hook `{hook}` is {state} (expected {want}, {})",
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Repo, op)),
        }
    }
}

/// Where the scenario fixtures live — `verification/fixtures/`, beside the scenarios that name them.
/// Resolved from this crate's own location rather than from the CWD, so `verify-all` finds them
/// wherever it is invoked from.
fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("fixtures")
}
