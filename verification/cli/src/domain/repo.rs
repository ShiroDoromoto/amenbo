//! The `repo` domain: the folder the run works in, rather than anything in the store. The files a
//! person already has lying there, the git repository the lint hooks stand in front of, and the
//! two gates that read them.

use std::path::Path;
use std::process::Command;

use amenbo_scenario::{Args, Domain};

use crate::{req_bool, req_i64, req_str, unmapped, Driver, Outcome};

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
            // `init` leaves behind: a repository with no commit has no branch either, and the
            // official `worktree` plugin needs one to cut a task's checkout from.
            //
            // Which folder becomes one is `write-file`'s rule, said by `dir:` and never by a path:
            // the run's own folder, or one a `folder` step bound. A road reading what git says about
            // a bound folder needs the second — the colours are drawn on the face of the folder the
            // project is bound to, and a repository anywhere else leaves every row of it bare.
            "git-init" => {
                let at = match with.get("dir") {
                    Some(_) => self.folder(with)?,
                    None => self.session.cwd.clone(),
                };
                let git = |args: &[&str]| -> Result<(), String> {
                    let out = Command::new("git")
                        .args(args)
                        .current_dir(&at)
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
                let at = match with.get("dir") {
                    Some(_) => self.folder(with)?,
                    None => self.session.cwd.clone(),
                };
                let git = |args: &[&str]| -> Result<(), String> {
                    let out = Command::new("git")
                        .args(args)
                        .current_dir(&at)
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
                git(&["add", "-A"])?;
                // The identity is named here for `git-init`'s reason: a box with none set would fail,
                // and neither name belongs to anybody.
                git(&[
                    "-c", "user.name=verify",
                    "-c", "user.email=verify@example.invalid",
                    "commit", "--quiet",
                    "-m", "what the road put here before it started",
                ])?;
                Ok(Outcome::action(format!("recorded what was lying in {}", at.display())))
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

#[cfg(test)]
mod tests {
    use super::*;

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
