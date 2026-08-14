//! The `repo` domain: the folder the run works in, rather than anything in the store. The files a
//! person already has lying there, the git repository the lint hooks stand in front of, and the
//! two gates that read them.

use std::path::Path;
use std::process::Command;

use amenbo_scenario::{Args, Domain};

use crate::{req_bool, req_str, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn repo_action(&mut self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // The folder the run works in. `write-file` is a person already having a file there —
            // what gets attached, and what the lint is pointed at.
            // Where it lands is the run's own folder, unless the step names one of the folders a
            // `folder` step binds: what a folder traces is read off its own contents, so a bound
            // folder that already carries a provider's settings can only be made by writing inside
            // it. Which is which is said by `dir:` and never by the path, so a path can no more
            // climb out of one folder than out of the other.
            "write-file" => {
                let path = req_str(with, "path")?;
                let content = req_str(with, "content")?;
                let full = match with.get("dir") {
                    Some(_) => self.folder(with)?.join(self.inside(path)?),
                    None => self.in_session(path)?,
                };
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                std::fs::write(&full, content).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Outcome::action(format!("wrote {} ({} bytes)", full.display(), content.len())))
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
            // The edit the handed-over text asks for. amenbo writes no settings file, so this stands
            // in for the AI the reader gives that text to — and it takes both halves of the answer
            // from the build under test: the configuration the request carries, and the file the
            // build says it belongs in. Writing either of them down here instead would leave the road
            // wired by the driver's own idea of the provider, which is the one thing this step must
            // not be the judge of. It is the `configuration` field and not the request, because the
            // request is prose: an AI reads it, a provider does not.
            "wire-ai" => {
                let tool = req_str(with, "tool")?;
                let v = self.run_json(&["agent-hook", "snippet", tool, "--json"])?;
                let into = v["paste_into"].as_str().ok_or("the text does not say where it goes")?;
                let configuration =
                    v["configuration"].as_str().ok_or("the text came with no configuration")?;
                // Which folder it lands in is `write-file`'s rule, said by `dir:` and never by the
                // path: the run's own folder, or one a `folder` step bound. A bound folder reads as
                // wired only from what is inside it, so a world that opens on one somebody already
                // wired is made here and nowhere else.
                let full = match with.get("dir") {
                    Some(_) => self.folder(with)?.join(self.inside(into)?),
                    None => self.in_session(into)?,
                };
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                std::fs::write(&full, configuration).map_err(|e| format!("could not write {into}: {e}"))?;
                Ok(Outcome::action(format!("made the edit {tool}'s text asks for, in {into}")))
            }
            // An app already reaching this folder over MCP. amenbo hands that entry over on screen, and
            // the one app it writes a file for takes a bundle a person opens — so there is nothing to
            // ask the build for here, and the shape below is the driver's own. What that costs is
            // drift, and it costs it the safe way round: an entry the build no longer reads leaves the
            // folder unreached, so a road that says the report about wiring went comes out red.
            //
            // Only an app that keeps its settings inside the folder is named. The rest keep one file
            // for the whole machine, which belongs to whoever is driving this run.
            "mcp-reach" => {
                let app = req_str(with, "app")?;
                let (place, servers) = match app {
                    "claude-code" => (".mcp.json", "mcpServers"),
                    "vscode" => (".vscode/mcp.json", "servers"),
                    other => {
                        return Err(format!(
                            "`app: {other}` keeps its MCP settings somewhere other than this folder — \
                             name one that keeps them inside it (claude-code, vscode)"
                        ))
                    }
                };
                // Where it lands is `write-file`'s rule, said by `dir:` and never by the path: the run's
                // own folder, or one a `folder` step bound.
                let folder = match with.get("dir") {
                    Some(_) => self.folder(with)?,
                    None => self.session.cwd.clone(),
                };
                let full = folder.join(self.inside(place)?);
                // The folder the entry binds the server to, canonical: that is the form amenbo records
                // a binding in, and the two have to be the same word for the entry to be read as this
                // folder's.
                let folder = std::fs::canonicalize(&folder).unwrap_or(folder);
                let entry = serde_json::json!({
                    "command": "amenbo",
                    "args": ["mcp", "--dir", folder.to_string_lossy()],
                });
                // Filed under the launch command's own name, which is what amenbo looks for. This
                // harness drives a release build and refuses anything else, so that name is `amenbo`
                // here — a dev build files its own, and is never what is under test.
                let mut entries = serde_json::Map::new();
                entries.insert("amenbo".to_string(), entry);
                let mut document = serde_json::Map::new();
                document.insert(servers.to_string(), serde_json::Value::Object(entries));
                let document = serde_json::Value::Object(document);
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                std::fs::write(&full, document.to_string())
                    .map_err(|e| format!("could not write {place}: {e}"))?;
                Ok(Outcome::action(format!("set {app} up to reach {} over MCP", folder.display())))
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
            // Whether this folder starts its AI on amenbo, read off the report amenbo carries on
            // every response until it does. There is no command that answers this on its own, and
            // that is the design: the answer travels on whatever the reader was already running, so
            // it reaches an AI that never thought to ask.
            "ai-launch" => {
                let wired = req_bool(with, "wired")?;
                let named = with.get("tool").and_then(|v| v.as_str());
                let v = self.run_json(&["task", "list", "--json"])?;
                let report = &v["setup_incomplete"]["agent_hook"];
                // `any_wired` is the answer while the report stands, and on this face it stands
                // until every tool in the catalog is wired — the reader here names its own, so one
                // provider wired leaves the rest still worth carrying. A report gone silent is that
                // same fact at its limit, and both readings say this folder starts its AI on amenbo.
                let is_wired = report.is_null() || report["any_wired"].as_bool() == Some(true);
                let points_at = named.is_none_or(|tool| {
                    report["unwired"]
                        .as_array()
                        .is_some_and(|all| all.iter().any(|one| one["tool"].as_str() == Some(tool)))
                });
                let pass = is_wired == wired && points_at;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "this folder {} its AI on amenbo{} (expected {}, {})",
                        if is_wired { "starts" } else { "does not start" },
                        match named {
                            Some(tool) if points_at => format!(", and {tool} is named as unwired"),
                            Some(tool) => format!(", and {tool} is not among the ones it names"),
                            None => String::new(),
                        },
                        if wired { "wired" } else { "unwired" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // The same report, read on one tool's own row. The reader of this face names itself, so a
            // provider the folder shows no trace of is still asking whether *it* is wired here — and
            // it is the one that gets no answer anywhere else: nothing in the folder points at it, so
            // no warning is printed and the shortlist above leaves it out.
            "ai-launch-tool" => {
                let tool = req_str(with, "tool")?;
                let wired = req_bool(with, "wired")?;
                let v = self.run_json(&["task", "list", "--json"])?;
                let report = &v["setup_incomplete"]["agent_hook"];
                let row = report["tools"]
                    .as_array()
                    .and_then(|all| all.iter().find(|one| one["tool"].as_str() == Some(tool)));
                let is_wired = match row {
                    Some(row) => row["wired"].as_bool().unwrap_or(false),
                    // A report gone silent is every tool in the catalog wired, this one included.
                    None if report.is_null() => true,
                    None => return Err(format!("the report carries no row for {tool}")),
                };
                // And where it is not, the row says how to fix it: a reader told it is unwired and
                // handed no way to the text knows exactly as much as one told nothing.
                let says_how = is_wired
                    || row.is_some_and(|row| {
                        row["fix"].as_str().is_some_and(|fix| fix.contains(tool))
                    });
                let pass = is_wired == wired && says_how;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the report says {tool} is {}{} (expected {}, {})",
                        if is_wired { "wired here" } else { "not wired here" },
                        if is_wired || says_how { String::new() } else { ", and does not say how to wire it".to_string() },
                        if wired { "wired" } else { "unwired" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // The text that closes the gap, read from the face that hands it over. The file it names
            // is checked beside what it carries, because the two only work together: a configuration
            // landing somewhere the provider does not read leaves the folder exactly as unwired as
            // before, and the reader with no way of telling.
            "ai-launch-text" => {
                let tool = req_str(with, "tool")?;
                let carries = req_str(with, "carries")?;
                let into = with.get("paste_into").and_then(|v| v.as_str());
                let v = self.run_json(&["agent-hook", "snippet", tool, "--json"])?;
                let request = v["request"].as_str().unwrap_or_default();
                let paste_into = v["paste_into"].as_str().unwrap_or_default();
                let carried = request.contains(carries);
                let placed = into.is_none_or(|want| want == paste_into);
                let pass = carried && placed;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the text for {tool} {} `{carries}` and goes in {paste_into}{} ({})",
                        if carried { "carries" } else { "does NOT carry" },
                        match into {
                            Some(want) if !placed => format!(" (expected {want})"),
                            _ => String::new(),
                        },
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
