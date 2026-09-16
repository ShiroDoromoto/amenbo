//! The `repo` domain: the folder the run works in, rather than anything in the store. The files a
//! person already has lying there, the git repository the lint hooks stand in front of, and the
//! two gates that read them.

use std::path::Path;
use std::process::Command;

use amenbo_scenario::{Args, Domain};
use amenbo_static_host::{Reply, StaticHost};

use crate::{opt_bool, req_bool, req_i64, req_str, unmapped, Driver, Outcome};

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
            // The same, for what a scenario cannot hold itself. A file under `fixtures/` is where the
            // reference form lives: this tree's prose rule keeps a bare ref out of every `.yaml`, and
            // the lint has nothing to find unless something really carries one. Bytes that are not text
            // at all come off the same shelf — an image a screen road has an operator choose in a
            // picker — which is why the copy is made of the bytes rather than of a string, and why the
            // line it leaves names where the file landed: on a screen road that path is what the
            // operator hunts the picker with, the instructions being rendered from the YAML alone.
            //
            // `dir` says where it lands, the way `write-file`'s does and for the same reason: what a
            // face reads off a folder is read off that folder, so a file that has to be in one can
            // only be put there.
            "copy-fixture" => {
                let from = req_str(with, "from")?;
                let path = req_str(with, "path")?;
                if Path::new(from).is_absolute()
                    || Path::new(from).components().any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(format!("`from: {from}` must name a file under fixtures/"));
                }
                let src = self.fixtures.join(from);
                let full = match with.get("dir") {
                    Some(_) => self.folder(with)?.join(self.inside(path)?),
                    None => self.in_session(path)?,
                };
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                let bytes = std::fs::read(&src)
                    .map_err(|e| format!("could not read the fixture {}: {e}", src.display()))?;
                std::fs::write(&full, &bytes).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Outcome::action(format!(
                    "copied the fixture {from} to {} ({} bytes)",
                    full.display(),
                    bytes.len()
                )))
            }
            // A picture of a named size, drawn here rather than kept on the fixtures shelf. What a
            // road asks for is the size — the provider behaviour under test turns on it, and the
            // smallest that walks is over four megabytes, which is not a thing to add to a tree
            // everyone clones.
            //
            // **It is noise, and it is stored rather than compressed.** A drawing would come out a
            // few kilobytes however many pixels it had, and the file has to *be* the size it was
            // asked for — what reads it is a provider deciding whether to take the picture in.
            // `megabytes` counts 1024 × 1024, so a road that named the size in the smaller
            // megabyte gets at least what it asked for and never less.
            //
            // `dir` says where it lands, the way `write-file`'s does and for the same reason.
            "write-picture" => {
                let path = req_str(with, "path")?;
                let megabytes = req_i64(with, "megabytes")?;
                if megabytes < 1 {
                    return Err(format!("`megabytes: {megabytes}` — a picture is at least one"));
                }
                let full = match with.get("dir") {
                    Some(_) => self.folder(with)?.join(self.inside(path)?),
                    None => self.in_session(path)?,
                };
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                let png = noise_png(megabytes as u64 * 1024 * 1024);
                std::fs::write(&full, &png).map_err(|e| format!("could not write {path}: {e}"))?;
                Ok(Outcome::action(format!(
                    "drew {} ({} bytes, {megabytes}MiB of noise)",
                    full.display(),
                    png.len()
                )))
            }
            // A name in that folder that is a link rather than a file. `path` is the name and `to`
            // is what it points at, read in the run's own folder — outside every folder a `folder`
            // step binds, which is the shape a person really has: one file kept in one place, and a
            // project pointing at it.
            //
            // Nothing has to be lying at `to`. A link is what its own name is and never what it
            // leads to, and a face that refuses to follow one refuses it either way — so a road
            // about the refusal is not made to stand up the far side of the link as well.
            "symlink" => {
                let path = req_str(with, "path")?;
                let to = req_str(with, "to")?;
                let target = self.in_session(to)?;
                let full = match with.get("dir") {
                    Some(_) => self.folder(with)?.join(self.inside(path)?),
                    None => self.in_session(path)?,
                };
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                symlink(&target, &full)?;
                Ok(Outcome::action(format!("linked {} at {path}", target.display())))
            }
            // The hooks are written into a git repository, so the scenario has to stand one up first.
            // This is the one step that is not Amenbo — everything it proves is about what Amenbo
            // then does to a repository that is really there.
            //
            // It leaves a `main` with one commit on it, rather than the branchless state a bare
            // `init` leaves behind: a repository with no commit has no branch either, and
            // `worktree start` needs one to cut a task's checkout from.
            //
            // Which folder becomes one is `write-file`'s rule, said by `dir:` and never by a path:
            // the run's own folder, or one a `folder` step bound. A road reading what git says about
            // a bound folder needs the second — the colours are drawn on the face of the folder the
            // project is bound to, and a repository anywhere else leaves every row of it bare.
            "git-init" => {
                let at = self.repo_dir(with)?;
                git_in(&at, &["init", "-q", "--initial-branch", "main"])?;
                git_in(&at, &["commit", "--quiet", "--allow-empty", "-m", "the branch a scenario cuts from"])?;
                Ok(Outcome::action(format!("made {} a git repository on `main`", at.display())))
            }
            // What is lying in the folder, recorded — the one way a road can stand up a folder git
            // has nothing to say about while a file inside it is new. Until something is committed,
            // git names the whole top folder and never the paths under it, and a face reading what a
            // folded folder holds is reading a state that cannot arise.
            //
            // Everything is staged rather than a named path: what a road puts in the folder is
            // whatever its premises wrote, and a step that had to list them again would be a second
            // place to keep the same list. An empty commit is refused by git and left to fail — a
            // road committing nothing has a premise that did not do what it said.
            "git-commit" => {
                let at = self.repo_dir(with)?;
                git_in(&at, &["add", "-A"])?;
                git_in(&at, &["commit", "--quiet", "-m", "what the road put here before it started"])?;
                Ok(Outcome::action(format!("recorded what was lying in {}", at.display())))
            }
            // Somewhere for that repository to send to, and the branch measured against it. What
            // stands on the other side is a bare repository beside the folder in the run's own
            // throwaway space, reached by a path: a path asks for no key and no account, so this
            // walks with nothing on the network and the two numbers are git's own arithmetic.
            //
            // The branch is sent as this step goes, because `push` against a branch git has never
            // been told where to send is refused with its own sentence about `--set-upstream`. That
            // sentence is a true answer and the wrong one to open a road about the numbers on: what
            // a road wants from here is a branch level with the other side.
            "git-remote" => {
                let at = self.repo_dir(with)?;
                let bare = share_from(&at)?;
                Ok(Outcome::action(format!("{} now sends to {}", at.display(), bare.display())))
            }
            // The other side moving on, which is the only way `behind` ever comes to be a number.
            // The driver stands in for somebody else's checkout the way `write-file` stands in for a
            // file a person already had: it clones the shared repository somewhere of its own,
            // records the file there and sends it, and takes the clone away again.
            //
            // The clone is thrown away rather than kept between steps. A road that moved the other
            // side twice would otherwise be reaching back into a checkout an earlier step left, and
            // what it is standing in for is a person on another machine, not a second folder of this
            // road's own.
            "git-remote-move" => {
                let at = self.repo_dir(with)?;
                let path = req_str(with, "path")?;
                let content = req_str(with, "content")?;
                // The path is bent through the same gate every other one in this domain is: a step
                // may name something inside the folder and nothing above it.
                let bare = move_share_on(&at, self.inside(path)?, content)?;
                Ok(Outcome::action(format!("somebody else sent {path} to {}", bare.display())))
            }
            // The other side that asks who is sending. It is a loopback host turning every request
            // away with a `401`, which is the least that sends git looking for a credential — so the
            // question the window then puts up is git's own, asked the way a repository nobody is
            // logged in to asks it.
            //
            // The host is held on the driver for as long as the world stands. A premise that let go
            // of it would take the port with it, and git would come back saying the connection was
            // refused instead of asking anything.
            "git-remote-asking" => {
                let at = self.repo_dir(with)?;
                let (host, url) = share_that_asks(&at)?;
                self.asking = Some(host);
                Ok(Outcome::action(format!("{} now sends to {url}, which asks who is sending", at.display())))
            }
            // Which branch the folder is standing on when the road opens. The branch is cut here
            // where there is none by that name and stepped onto where there is, because what a
            // premise declares is where the reader finds the folder — never which of the two ways it
            // came to be there. A road that had to say "make this one, move onto that one" would be
            // saying the same thing twice for branches it made itself a few lines earlier.
            //
            // A branch cannot be cut where nothing has been recorded, so this follows `git-init`,
            // whose own commit is what there is to cut from.
            "git-branch" => {
                let at = self.repo_dir(with)?;
                let name = req_str(with, "name")?;
                let there = git_asks(&at, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{name}")])?;
                match there {
                    true => git_in(&at, &["checkout", "-q", name])?,
                    false => git_in(&at, &["checkout", "-q", "-b", name])?,
                }
                Ok(Outcome::action(format!("{} is standing on `{name}`", at.display())))
            }
            // A folder whose files of some shape are kept by Git LFS rather than by git — the state
            // `git lfs install` leaves a machine in, written into this repository's own
            // configuration instead.
            //
            // **The reader's own `~/.gitconfig` is not the premise's to write to**, and this is the
            // one place the difference shows: git reads the four keys from wherever they are set, so
            // a repository carrying them behaves as the reader's machine would without the run
            // having touched anything outside its own folder.
            //
            // **Which paths go through it is `.gitattributes`**, written by `write-file` like any
            // other file a road puts in the folder. The two halves are separate because they are
            // separate for a reader too: one is the machine's, the other is the repository's, and a
            // road that means to stand on both has to say both.
            //
            // **Nothing here needs `git-lfs` to be installed**, and a road standing on this premise
            // is usually one about what happens where it is not.
            "uses-lfs" => {
                let at = self.repo_dir(with)?;
                for (key, value) in [
                    ("filter.lfs.clean", "git-lfs clean -- %f"),
                    ("filter.lfs.smudge", "git-lfs smudge -- %f"),
                    ("filter.lfs.process", "git-lfs filter-process"),
                    ("filter.lfs.required", "true"),
                ] {
                    git_in(&at, &["config", key, value])?;
                }
                Ok(Outcome::action(format!("{} keeps its big files in LFS", at.display())))
            }
            // The edit the handed-over text asks for. Amenbo writes no settings file, so this stands
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
            // An app already reaching this folder over MCP. Amenbo hands that entry over on screen, and
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
                // The folder the entry binds the server to, canonical: that is the form Amenbo records
                // a binding in, and the two have to be the same word for the entry to be read as this
                // folder's.
                let folder = std::fs::canonicalize(&folder).unwrap_or(folder);
                let entry = serde_json::json!({
                    "command": "amenbo",
                    "args": ["mcp", "--dir", folder.to_string_lossy()],
                });
                // Filed under the launch command's own name, which is what Amenbo looks for. This
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
            // A checkout of the task's own, cut and folded. What a person working a
            // task types, in the order they type it — and the refusals in between, which is where
            // most of the value is: a second `start` is what keeps two sessions off one checkout,
            // and a fold that refuses is what keeps work nobody recorded from going with it.
            //
            // The return value goes where a later step can read it. `start`'s whole answer is one
            // `cd` line on stdout, which is not a state anything can be asked about afterwards.
            "worktree-start" => {
                let at = self.repo_dir(with)?;
                let id = self.resolve(with)?;
                let id = id.to_string();
                let mut args = vec!["worktree", "start", &id];
                // A road asking how this refuses reads the machine face; one asking what it hands
                // back reads the line a shell takes. The two are the same command and cannot be the
                // same call: a refusal names its code in an `error` object on stderr, which is only
                // written under `--json`, and `--json` is also what replaces the `cd` line with a
                // document. Which question the step is asking is the step's own `refused:`.
                if self.refusing() {
                    args.push("--json");
                }
                let out = self.run_stdout_in(&at, &args)?;
                self.last_worktree = Some(String::from_utf8_lossy(&out).into_owned());
                Ok(Outcome::action(format!("cut task {id} a checkout of its own in {}", at.display())))
            }
            // The other end of it. `force` is the only way to discard work on purpose, so a road that
            // wants the guard to give way has to say so — the same word the person types.
            "worktree-finish" => {
                let at = self.repo_dir(with)?;
                let id = self.resolve(with)?;
                let id = id.to_string();
                let mut args = vec!["worktree", "finish", &id];
                if opt_bool(with, "force").unwrap_or(false) {
                    args.push("--force");
                }
                // `worktree-start`'s reason, and the fold has no return value to lose by it.
                if self.refusing() {
                    args.push("--json");
                }
                self.run_stdout_in(&at, &args)?;
                Ok(Outcome::action(format!("folded task {id}'s checkout away")))
            }
            // Work nobody has recorded, left in a task's checkout — the one state no other op here can
            // reach. A checkout stands beside the run's folder rather than inside it, and every path a
            // road may write is closed to what lies outside; without this the fold's own guard is a
            // guard no road walks.
            "worktree-write-file" => {
                let at = self.repo_dir(with)?;
                let id = self.resolve(with)?;
                let path = self.inside(req_str(with, "path")?)?;
                let full = worktree_of(&at, id)?.join(path);
                if let Some(dir) = full.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                }
                std::fs::write(&full, req_str(with, "content")?)
                    .map_err(|e| format!("could not write {}: {e}", full.display()))?;
                Ok(Outcome::action(format!("left work nobody recorded in task {id}'s checkout")))
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
            // What the entry point recommends here. The cycles are named by id, and a step inside one
            // by its own — read across both halves of a cycle, because which half a line sits in is
            // how strongly it is put and not whether it was handed over.
            //
            // A cycle nobody dropped is a key in `cycles`; a dropped one is not there at all, which
            // is the same nothing a reader gets. So both readings are the same lookup, and `present`
            // is which of them the road expects.
            "agent-cycle" => {
                let cycle = req_str(with, "cycle")?;
                let step = with.get("step").and_then(serde_yaml::Value::as_str);
                let want = req_bool(with, "present")?;
                // Run where the reader is standing. What gates the advice is the folder holding the
                // pointer this invocation resolved, so a run made anywhere else is answering about
                // somewhere else — and a run made where nothing is bound is answering about nothing
                // being bound, which is a third state and not this reading.
                let at = self.folder(with)?;
                let v = self.run_json_in(&at, &["agent", "--json"])?;
                let held = &v["cycles"][cycle];
                let found = match step {
                    None => !held.is_null(),
                    Some(one) => ["backbone", "optional"].iter().any(|half| {
                        held[half]
                            .as_array()
                            .map(Vec::as_slice)
                            .unwrap_or(&[])
                            .iter()
                            .any(|row| row["id"].as_str() == Some(one))
                    }),
                };
                let named = match step {
                    None => format!("cycle `{cycle}`"),
                    Some(one) => format!("step `{one}` of cycle `{cycle}`"),
                };
                Ok(Outcome::assert(
                    found == want,
                    format!(
                        "{named} is {} in what `agent --json` hands a reader in that folder (expected {}, {})",
                        if found { "there" } else { "not there" },
                        if want { "there" } else { "not there" },
                        if found == want { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // Whether this folder starts its AI on Amenbo, read off the report Amenbo carries on
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
                // same fact at its limit, and both readings say this folder starts its AI on Amenbo.
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
                        "this folder {} its AI on Amenbo{} (expected {}, {})",
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
            // Whether a task's checkout is standing. Both halves are read, because half of either is
            // not a state a fold may leave behind: a directory whose branch is gone turns the next
            // `start` away with the wrong reason, and a branch whose directory is gone leaves work
            // that nothing points at.
            "worktree" => {
                let at = self.repo_dir(with)?;
                let id = self.resolve(with)?;
                let want = req_bool(with, "present")?;
                let checkout = worktree_of(&at, id)?;
                let standing = checkout.is_dir();
                let branch = format!("task/{id}");
                let held = Command::new("git")
                    .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])
                    .current_dir(&at)
                    .status()
                    .map_err(|e| format!("could not run git: {e}"))?
                    .success();
                let pass = standing == want && held == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "task {id}: the checkout is {}, branch `{branch}` is {} (expected both {}, {})",
                        if standing { "there" } else { "gone" },
                        if held { "there" } else { "gone" },
                        if want { "there" } else { "gone" },
                        if pass { "as expected" } else { "MISMATCH" },
                    ),
                ))
            }
            // The way in `worktree start` handed back: one `cd` line and nothing else. It is the whole
            // of that command's return value — a caller is meant to run it rather than read it — so
            // what is judged is the exact text, not that a path appears somewhere in it.
            "worktree-way-in" => {
                let at = self.repo_dir(with)?;
                let id = self.resolve(with)?;
                let said = self
                    .last_worktree
                    .as_deref()
                    .ok_or("no `worktree start` has run on this road for this to read")?;
                let want = format!("cd '{}'\n", worktree_of(&at, id)?.display());
                let pass = said == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "`worktree start` wrote {said:?} (expected {want:?}, {})",
                        if pass { "as expected" } else { "MISMATCH" },
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Repo, op)),
        }
    }
}

/// Make `at` a link pointing at `target`.
///
/// The two platforms name the call differently — and on Windows a file link is a privilege rather
/// than an ordinary write, so what comes back when the machine has not granted it is said here
/// rather than left as a bare error number.
#[cfg(unix)]
fn symlink(target: &Path, at: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, at)
        .map_err(|e| format!("could not link {} at {}: {e}", target.display(), at.display()))
}

#[cfg(windows)]
fn symlink(target: &Path, at: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_file(target, at).map_err(|e| {
        format!(
            "could not link {} at {}: {e} — making one here needs the privilege developer mode grants",
            target.display(),
            at.display(),
        )
    })
}

/// A valid PNG of at least `bytes`, filled with noise.
///
/// **Written out by hand because what is wanted is the size.** Every encoder's job is to make a
/// picture small, and the roads this serves turn on a file being large — a provider reads the size
/// off the bytes and decides whether to take the picture in. So the pixels are noise, which nothing
/// can shrink, and the deflate stream is made of *stored* blocks, which shrink nothing by
/// definition: the file comes out a little over the asked-for size rather than a few kilobytes
/// under it, whatever library is on the machine.
///
/// The picture itself is 1024 pixels wide and as tall as the size asks for, in eight-bit RGB.
/// Nothing reads what it depicts.
fn noise_png(bytes: u64) -> Vec<u8> {
    const WIDTH: u32 = 1024;
    let row = 1 + WIDTH as usize * 3; // the filter byte, then the pixels
    let height = (bytes as usize).div_ceil(row).max(1) as u32;

    // The rows, each opening with the byte that says it was not filtered — which is the nought it
    // already holds — and the rest of it noise.
    let mut raw = vec![0u8; height as usize * row];
    let mut seed: u32 = 0x9e37_79b9;
    for line in raw.chunks_mut(row) {
        for byte in line.iter_mut().skip(1) {
            // A plain linear congruential step — the numbers only have to be unalike, and it is
            // written out rather than pulled in so the harness keeps the dependencies it has.
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *byte = (seed >> 16) as u8;
        }
    }

    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend(WIDTH.to_be_bytes());
    ihdr.extend(height.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]); // eight bits a channel, truecolour, and no interlacing
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &stored_zlib(&raw));
    chunk(&mut png, b"IEND", &[]);
    png
}

/// One PNG chunk: its length, its name, its bytes, and the check over the last two.
fn chunk(png: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    png.extend((data.len() as u32).to_be_bytes());
    png.extend(name);
    png.extend(data);
    let mut crc = Vec::with_capacity(4 + data.len());
    crc.extend(name);
    crc.extend(data);
    png.extend(crc32(&crc).to_be_bytes());
}

/// `raw` as a zlib stream whose deflate blocks are all *stored* — the one encoding that is allowed
/// to make nothing smaller, which is the whole reason it is used here.
fn stored_zlib(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // deflate, a 32KiB window, no preset dictionary
    let mut rest = raw;
    loop {
        let take = rest.len().min(u16::MAX as usize);
        let (block, after) = rest.split_at(take);
        out.push(u8::from(after.is_empty())); // the last block says so; the type is stored
        out.extend((take as u16).to_le_bytes());
        out.extend((!(take as u16)).to_le_bytes());
        out.extend(block);
        rest = after;
        if rest.is_empty() {
            break;
        }
    }
    out.extend(adler32(raw).to_be_bytes());
    out
}

/// The check PNG puts on a chunk.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (!(crc & 1)).wrapping_add(1));
        }
    }
    !crc
}

/// The check zlib puts on the stream it carried.
fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in bytes {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}


/// Where a step's repository is: the folder its `dir:` names, or the run's own. `git-init`'s rule,
/// spelled once because every op in this domain follows it.
impl Driver<'_> {
    fn repo_dir(&self, with: &Args) -> Result<std::path::PathBuf, String> {
        match with.get("dir") {
            Some(_) => self.folder(with),
            None => Ok(self.session.cwd.clone()),
        }
    }
}

/// One git command in one folder, with the identity named on the command line rather than left to
/// the machine's git config: a box with none set is refused by `commit`, and neither name belongs to
/// anybody. The two flags are harmless on the commands that never read them, which is what lets
/// every call in this domain go through one door.
fn git_in(at: &Path, args: &[&str]) -> Result<(), String> {
    let out = git_run(at, args)?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` in {} failed: {}",
            args.join(" "),
            at.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// Whether git answers yes — a question a premise puts before it picks which road to take, where
/// the no is an answer and not a failure. `git_in`'s door, read for its status instead of its
/// refusal.
fn git_asks(at: &Path, args: &[&str]) -> Result<bool, String> {
    Ok(git_run(at, args)?.status.success())
}

/// The one place this domain's git is built, so that what it does **not** read is settled once.
///
/// **None of the machine's git configuration is.** What a premise declares is the repository it
/// builds, and a reader's own `~/.gitconfig` is no part of that: `git lfs install` in it would send
/// the commits made here through a filter no road asked for, and `core.autocrlf`, `commit.gpgsign`
/// or `core.hooksPath` would each change what is recorded or where. Left inherited, a scenario is
/// green on the box it was written on and red on the next one, which is the one thing a release
/// gate must not be.
///
/// `/dev/null` is a configuration file with nothing in it, which is what "read none of the
/// reader's" comes to; Windows spells that file `NUL`. The system one is turned off by a variable
/// of its own, there being no path to point at it with.
fn git_run(at: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    git_run_with(at, args, &[])
}

/// The same git, with something more in its environment. It is separate so that the isolation above
/// is written once: a caller that needed it and built its own command would be a second place for
/// the reader's configuration to leak back in.
fn git_run_with(
    at: &Path,
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<std::process::Output, String> {
    let mut git = Command::new("git");
    git.args(["-c", "user.name=verify", "-c", "user.email=verify@example.invalid"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .current_dir(at);
    for (name, value) in env {
        git.env(name, value);
    }
    git.output().map_err(|e| format!("could not run git: {e}"))
}

/// Where the repository a folder sends to stands, and where somebody else's checkout of it is made:
/// beside that folder, under its own name. Both are derived rather than said, for the reason every
/// other placement in this domain is — a scenario names folders and never paths, so the one place
/// the layout is written down is here.
///
/// **Beside and never inside.** What git says about a folder is drawn from that folder's own rows,
/// so a second repository under one of them would put rows on the panel no road ever put there.
fn beside(at: &Path, suffix: &str) -> Result<std::path::PathBuf, String> {
    let name = at
        .file_name()
        .ok_or_else(|| format!("{} has no name to put a shared repository beside", at.display()))?
        .to_string_lossy()
        .into_owned();
    Ok(at.with_file_name(format!("{name}{suffix}")))
}

/// Give a repository somewhere to send to, and leave its branch measured against it.
///
/// The other side is a bare repository reached by a path, which is what lets every road about the
/// two counts walk with nothing on the network: a path asks for no key and no account. The branch is
/// sent as this goes, because `push` against a branch git has never been told where to send is
/// refused with its own sentence about `--set-upstream` — true, and the wrong answer to open a road
/// about the counts on.
fn share_from(at: &Path) -> Result<std::path::PathBuf, String> {
    let bare = beside(at, "-shared.git")?;
    // git runs in a folder, so the one it is about has to be there before it is started — the only
    // placement in this domain that is not a folder somebody's premise already stood up.
    std::fs::create_dir_all(&bare).map_err(|e| format!("could not make {}: {e}", bare.display()))?;
    git_in(&bare, &["init", "-q", "--bare", "--initial-branch", "main"])?;
    git_in(at, &["remote", "add", "origin", &bare.to_string_lossy()])?;
    git_in(at, &["push", "--quiet", "--set-upstream", "origin", "main"])?;
    Ok(bare)
}

/// Give a repository somewhere to send to that stops and asks who is sending, and hand back the
/// host answering there beside the URL git was pointed at.
///
/// **What answers is a `401` and nothing else.** git goes looking for a credential when it is turned
/// away that way, and the header naming what was wanted is what says the refusal is about who is
/// asking rather than about what was asked for. Nothing behind it is a repository and nothing needs
/// to be: git asks before it has been let in once, so a road about the asking never reaches the
/// point where there would be something to send.
///
/// Every path answers alike, both services named, because which of the two git reaches for is its
/// own business — a road pressing the control that reads the other side and a road pressing the one
/// that sends must meet the same door.
///
/// The host has to be **held**: it answers while it is alive and stops when it is dropped
/// ([`amenbo_static_host::StaticHost`]), and a port nothing is listening at would have git come back
/// saying the connection was refused instead of asking anything.
fn share_that_asks(at: &Path) -> Result<(StaticHost, String), String> {
    let name = at
        .file_name()
        .ok_or_else(|| format!("{} has no name to send under", at.display()))?
        .to_string_lossy()
        .into_owned();
    let path = format!("/{name}.git");
    let host = StaticHost::serve(Vec::<(String, String)>::new());
    for service in ["git-upload-pack", "git-receive-pack"] {
        host.set_reply(
            &format!("{path}/info/refs?service={service}"),
            Reply::status(401, "").and_header("WWW-Authenticate", "Basic realm=\"amenbo\""),
        );
    }
    let url = host.url(&path);
    git_in(at, &["remote", "add", "origin", &url])?;
    Ok((host, url))
}

/// Record one file on the other side, as somebody working from their own checkout of it would.
///
/// The checkout is cloned here and taken away again rather than kept between steps. What it stands
/// in for is a person on another machine, so a road that moved the other side twice must not be
/// reaching back into a folder an earlier step left lying beside this one.
fn move_share_on(at: &Path, path: &Path, content: &str) -> Result<std::path::PathBuf, String> {
    let bare = beside(at, "-shared.git")?;
    if !bare.exists() {
        return Err(format!(
            "{} has nowhere to send to — a road that moves the other side stands one up first (`git-remote`)",
            at.display()
        ));
    }
    let theirs = beside(at, "-elsewhere")?;
    // Left behind by a step that failed halfway, it would be cloned over. Taking it away first makes
    // this say the same thing however the step before it ended.
    let _ = std::fs::remove_dir_all(&theirs);
    git_in(at, &["clone", "--quiet", &bare.to_string_lossy(), &theirs.to_string_lossy()])?;
    let full = theirs.join(path);
    if let Some(dir) = full.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    }
    std::fs::write(&full, content)
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    git_in(&theirs, &["add", "-A"])?;
    git_in(&theirs, &["commit", "--quiet", "-m", "what somebody else recorded"])?;
    git_in(&theirs, &["push", "--quiet", "origin", "main"])?;
    std::fs::remove_dir_all(&theirs)
        .map_err(|e| format!("could not take {} away again: {e}", theirs.display()))?;
    Ok(bare)
}

/// Where a task's checkout stands, by the layout Amenbo fixes and nobody is asked about:
/// `<the repository's parent>/<its name>-worktrees/<id>`.
///
/// The repository is resolved first. Amenbo derives the placement from what git answers, which is
/// the resolved path — and a run whose throwaway folder is reached through a symlink (`/tmp` on a
/// Mac) would otherwise be comparing two spellings of one directory.
fn worktree_of(root: &Path, id: i64) -> Result<std::path::PathBuf, String> {
    let root = std::fs::canonicalize(root)
        .map_err(|e| format!("could not resolve {}: {e}", root.display()))?;
    let name = root
        .file_name()
        .ok_or_else(|| format!("{} has no name to cut a sibling beside", root.display()))?
        .to_string_lossy()
        .into_owned();
    let parent = root
        .parent()
        .ok_or_else(|| format!("{} has no parent to cut a sibling in", root.display()))?;
    Ok(parent.join(format!("{name}-worktrees")).join(id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one question `git-branch` asks before it decides which road to take. A premise names the
    /// branch it wants the folder standing on and never says whether that branch is there yet, so a
    /// wrong answer here is either a cut that fails on a name already taken or a move to a branch
    /// nobody made.
    #[test]
    fn a_branch_is_cut_where_there_is_none_by_that_name_and_stepped_onto_where_there_is() {
        let session = crate::scratch::session("repo-git-branch", false).unwrap();
        let at = session.cwd.join("orchard");
        std::fs::create_dir_all(&at).unwrap();
        git_in(&at, &["init", "-q", "--initial-branch", "main"]).unwrap();
        git_in(&at, &["commit", "--quiet", "--allow-empty", "-m", "cut from"]).unwrap();

        let asks = |name: &str| {
            git_asks(&at, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{name}")])
                .unwrap()
        };
        assert!(asks("main"), "the branch `git-init` left is there");
        assert!(!asks("trays"), "and one nobody made is not");

        git_in(&at, &["checkout", "-q", "-b", "trays"]).unwrap();
        assert!(asks("trays"), "once cut, the same question answers the other way");
    }

    /// The layout a task's checkout is placed by — beside the repository, never inside it, which is
    /// the shape Amenbo refuses to be run in. The name is the repository's own, so a road that cuts
    /// in a bound folder and one that cuts in the run's own folder each look beside the right thing.
    #[test]
    fn a_checkout_is_looked_for_beside_the_repository_it_was_cut_from() {
        let session = crate::scratch::session("repo-worktree-of", false).unwrap();
        let base = session.cwd.clone();
        let repo = base.join("orchard");
        std::fs::create_dir_all(&repo).unwrap();

        let at = worktree_of(&repo, 4739).unwrap();
        // The base is resolved, because git answers with the resolved path and a Mac reaches the
        // throwaway folder through a symlink.
        let resolved = std::fs::canonicalize(&base).unwrap();
        assert_eq!(at, resolved.join("orchard-worktrees").join("4739"));
        assert!(!at.starts_with(&repo), "it is never looked for inside the repository");
    }

    /// What git answers about a branch, in the shape the panel reads it in: the first line of
    /// `status --branch`, which is the one read both counts come off.
    fn standing(at: &Path) -> String {
        let out = Command::new("git")
            .args(["--no-optional-locks", "status", "--porcelain=v1", "--branch"])
            .current_dir(at)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).lines().next().unwrap().to_string()
    }

    /// The whole road the two counts are walked on, stood up the way the driver stands it up.
    ///
    /// **It is written here because nothing else ever runs these two.** The road they were made for
    /// is the screen's, and the screen harness only drives a shipped build — so a gate that ran on
    /// every change would never reach them, and a premise that had quietly stopped working would
    /// first be met by whoever sat down to walk the road by hand.
    ///
    /// What is asserted is git's own arithmetic rather than either function's return: level after
    /// the share is made, ahead by what was recorded here, and behind by what the other side
    /// recorded — but only once this side has asked, which is the fact the road turns on.
    #[test]
    fn a_folder_measures_itself_against_the_repository_it_shares_with() {
        let session = crate::scratch::session("repo-share-from", false).unwrap();
        let at = session.cwd.join("greenhouse-beds");
        // A bound folder is stood up by the session before any step reaches it; this stands in for
        // that, since what is under test starts at the repository being there.
        std::fs::create_dir_all(&at).unwrap();
        git_in(&at, &["init", "-q", "--initial-branch", "main"]).unwrap();
        git_in(&at, &["commit", "--quiet", "--allow-empty", "-m", "the branch a scenario cuts from"])
            .unwrap();

        let bare = share_from(&at).unwrap();
        assert!(bare.join("HEAD").is_file(), "{} is no bare repository", bare.display());
        assert_eq!(standing(&at), "## main...origin/main", "not level with what it shares with");

        // Recorded here and nowhere else.
        std::fs::write(at.join("watering.md"), "SCENARIO the seedlings are watered").unwrap();
        git_in(&at, &["add", "-A"]).unwrap();
        git_in(&at, &["commit", "--quiet", "-m", "the watering note goes in"]).unwrap();
        assert_eq!(standing(&at), "## main...origin/main [ahead 1]");

        git_in(&at, &["push", "--quiet"]).unwrap();
        assert_eq!(standing(&at), "## main...origin/main", "sending did not empty the count");

        // And somebody else records something on the other side.
        move_share_on(&at, Path::new("sieving.md"), "SCENARIO the loam is sieved").unwrap();
        assert!(
            !at.with_file_name("greenhouse-beds-elsewhere").exists(),
            "their checkout was left lying beside the folder"
        );
        // Nothing here knows yet, which is the step the road reads before it asks.
        assert_eq!(standing(&at), "## main...origin/main", "it knew before anybody asked");

        git_in(&at, &["fetch", "--quiet"]).unwrap();
        assert_eq!(standing(&at), "## main...origin/main [behind 1]");

        git_in(&at, &["pull", "--quiet"]).unwrap();
        assert_eq!(standing(&at), "## main...origin/main", "bringing it in did not empty the count");
        assert!(at.join("sieving.md").is_file(), "the count moved and nothing came in");
    }

    /// The door that asks, walked with the real git — as far as a test may walk it.
    ///
    /// **It is written here for the reason the road above is.** What this premise stands up is only
    /// ever asked for by a screen road, and the screen harness drives a shipped build, so no gate
    /// that runs on a change reaches it. What it has to be right about is one thing: that git gets
    /// as far as wanting a credential. A door that refused the connection, or answered `404`, would
    /// fail earlier and the road would never see a question at all.
    ///
    /// That is asserted by git's own sentence, with the terminal shut off so the call ends instead of
    /// waiting on somebody — which is the one thing this test does differently from the road, where
    /// the whole point is that the question goes to the window.
    #[test]
    fn a_folder_sending_to_a_door_that_asks_makes_git_want_a_credential() {
        let session = crate::scratch::session("repo-share-asking", false).unwrap();
        let at = session.cwd.join("greenhouse-beds");
        std::fs::create_dir_all(&at).unwrap();
        git_in(&at, &["init", "-q", "--initial-branch", "main"]).unwrap();
        git_in(&at, &["commit", "--quiet", "--allow-empty", "-m", "the branch a scenario cuts from"])
            .unwrap();

        let (host, url) = share_that_asks(&at).unwrap();
        assert!(url.starts_with("http://127.0.0.1:"), "it is reached on the loopback: {url}");
        assert!(url.ends_with("/greenhouse-beds.git"), "it sends under the folder's own name: {url}");

        // Nothing is standing to be asked in a test, so git is told to give up where it would
        // otherwise wait — and to ask nothing of the reader's own askpass on the way there, a
        // machine with one set being a machine where this would answer itself. The sentence git
        // gives up with is what names how far it got. It goes through the same door every other call
        // in this file does, so the reader's configuration is no more readable here than there.
        let out = git_run_with(
            &at,
            &["fetch", "origin"],
            &[("GIT_TERMINAL_PROMPT", "0"), ("GIT_ASKPASS", "")],
        )
        .unwrap();
        let said = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success(), "the door let git in: {said}");
        assert!(
            said.contains("could not read Username for"),
            "git never got as far as wanting a credential: {said}"
        );
        assert_eq!(host.heard().len(), 1, "git asked once and was turned away once");
    }

    /// Moving the other side of a folder that shares with nothing says which folder, rather than
    /// failing inside git with a path nobody wrote.
    #[test]
    fn moving_a_side_that_was_never_stood_up_says_so() {
        let session = crate::scratch::session("repo-share-missing", false).unwrap();
        let at = session.cwd.join("greenhouse-beds");
        std::fs::create_dir_all(&at).unwrap();
        let said = move_share_on(&at, Path::new("sieving.md"), "SCENARIO").unwrap_err();
        assert!(said.contains("greenhouse-beds"), "the refusal names the folder: {said}");
        assert!(said.contains("git-remote"), "the refusal says what stands one up: {said}");
    }

    /// A folder that is not there cannot be measured from, and the reason says which one — a road
    /// naming a `dir:` nothing stood up would otherwise fail with a path nobody recognises.
    #[test]
    fn a_repository_that_is_not_there_is_named_in_the_refusal() {
        let session = crate::scratch::session("repo-worktree-of-missing", false).unwrap();
        let missing = session.cwd.join("nowhere");
        let said = worktree_of(&missing, 1).unwrap_err();
        assert!(said.contains("nowhere"), "the refusal names the folder: {said}");
    }

    /// Walk a PNG chunk by chunk, answering with each one's name — and failing on the first whose
    /// check does not match what is in it.
    ///
    /// It is written here rather than reached for because the point is to read the file the way a
    /// stranger would: a decoder of our own making that agreed with a writer of our own making
    /// would prove only that the two agree. What this asks is the format's own question — does the
    /// length say where the next chunk starts, and does the check match the bytes.
    fn chunks(png: &[u8]) -> Vec<String> {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG at all");
        let mut names = Vec::new();
        let mut at = 8;
        while at < png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let name = String::from_utf8(png[at + 4..at + 8].to_vec()).unwrap();
            let end = at + 8 + len;
            let said = u32::from_be_bytes(png[end..end + 4].try_into().unwrap());
            assert_eq!(crc32(&png[at + 4..end]), said, "the check on {name} does not match it");
            names.push(name);
            at = end + 4;
        }
        assert_eq!(at, png.len(), "the last chunk ends somewhere other than the file does");
        names
    }

    /// A picture asked for by size is a real PNG, and it is at least the size it was asked for.
    ///
    /// **The size is the whole reason this op exists**: what reads these files is a provider
    /// deciding whether a picture is large enough to take in, and one that came out a few kilobytes
    /// would walk every road green while proving nothing.
    #[test]
    fn a_picture_asked_for_by_size_is_a_png_and_is_at_least_that_large() {
        let asked = 4 * 1024 * 1024;
        let png = noise_png(asked);

        assert_eq!(chunks(&png), ["IHDR", "IDAT", "IEND"]);
        assert!(png.len() as u64 >= asked, "{} bytes for {asked}", png.len());
        // And not wildly over it either: what is paid for the stored blocks and the chunk headers
        // is a fraction of a per cent, and a road asking for twenty megabytes gets twenty.
        assert!(png.len() as u64 <= asked + asked / 100, "{} bytes for {asked}", png.len());
    }

    /// And it is noise, which is what keeps it that large: a drawing of one colour would come back
    /// from any re-encoding as a few kilobytes, and the size is what the roads turn on.
    #[test]
    fn what_is_drawn_is_noise_rather_than_a_picture_that_would_compress_away() {
        let png = noise_png(1024 * 1024);
        let pixels = &png[png.len() / 3..png.len() / 3 + 4096];
        let distinct: std::collections::BTreeSet<u8> = pixels.iter().copied().collect();
        assert!(distinct.len() > 200, "only {} distinct bytes in a run of noise", distinct.len());
    }

    /// The smallest size a road may ask for still comes out a picture with a row in it.
    #[test]
    fn the_smallest_picture_a_road_may_ask_for_is_still_a_png() {
        let png = noise_png(1);
        assert_eq!(chunks(&png), ["IHDR", "IDAT", "IEND"]);
        // The signature, the chunk's length and name, then the width — so the height starts here.
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 1, "fewer than one row");
    }
}
