//! The `store` domain: this device's amenbo rather than anything filed in it — what comes out
//! (`export`), what is set aside (`backup`), what goes back in (`restore`), whether it is sound,
//! and the settings it keeps.

use std::path::Path;

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, path_str, req_bool, req_i64, req_str, unmapped, Driver, Outcome};
use crate::judge::judge_field;

impl Driver {
    pub(crate) fn store_action(&mut self, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            "export" => {
                let out = self.artifact(bind, "export", "");
                // `--out` takes the attachment files along with the records, which is the shape a
                // move to another tool is actually made in; it answers with prose, not JSON.
                self.run_bare(&["export", "--out", path_str(&out)?])?;
                self.remember(bind, "export", out.clone());
                Ok(Outcome::action(format!("exported the store to {}", out.display())))
            }
            "backup" => {
                let path = self.artifact(bind, "backup", ".amenbo-backup");
                let v = self.run_json(&["backup", path_str(&path)?, "--json"])?;
                let bytes = v["bytes"].as_i64().unwrap_or(0);
                self.remember(bind, "backup", path.clone());
                Ok(Outcome::action(format!("wrote a {bytes}-byte snapshot to {}", path.display())))
            }
            "restore" => {
                let path = self.artifact_ref(with, "target")?;
                // A destructive replace asks first; the driver is unattended, so it answers up front.
                let v = self.run_json(&["restore", path_str(&path)?, "--yes", "--json"])?;
                let saved = v["previous_saved_to"].as_str().unwrap_or("(not reported)");
                Ok(Outcome::action(format!(
                    "restored the store from {} (what it replaced was set aside at {saved})",
                    path.display()
                )))
            }
            // The repairing face. `--yes` because the driver is unattended, and the verdict it comes
            // back with is judged the way the reading face's is — a store still unsound after a
            // repair is a value, not a driver failure.
            "doctor-fix" => {
                let v = self.run_check(&["doctor", "--fix", "--yes", "--json"])?;
                let left = v["issues"].as_array().map_or(0, Vec::len);
                Ok(Outcome::action(format!("swept the store ({left} issue(s) still standing after it)")))
            }
            // Age every blob the store is holding past `GC_MIN_AGE`, so the sweep will take the ones
            // nothing references. A blob a run just wrote is always too young — removing its
            // attachment spares it for the hour that protects an attach in flight — which leaves the
            // reclaim unprovable from a scenario until its bytes are made old. The file's mtime is
            // what the sweep reads, so that is what moves here; no row is touched.
            "age-blobs" => {
                let dir = self.session.home.join("blobs");
                let aged = age_files_in(&dir)?;
                Ok(Outcome::action(format!("aged {aged} blob file(s) past the collection boundary")))
            }
            "config-set" => {
                let key = req_str(with, "key")?;
                let value = req_str(with, "value")?;
                self.run_json(&["config", "set", key, value, "--json"])?;
                Ok(Outcome::action(format!("set `{key}` to `{value}`")))
            }
            _ => Err(unmapped(Domain::Store, op)),
        }
    }
    pub(crate) fn store_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "snapshot" => {
                let path = self.artifact_ref(with, "target")?;
                // A word that must not be in what amenbo handed out. Read as bytes and searched for
                // verbatim: a value stored in the clear is in the clear whatever the layout around
                // it is, and that is the whole question — no claim about what the archive *does*
                // carry is made here.
                if let Some(needle) = with.get("absent").and_then(|v| v.as_str()) {
                    if needle.is_empty() {
                        return Err("`absent` names nothing to look for".to_string());
                    }
                    let (carried, read) = carries_text(&path, needle)?;
                    return Ok(Outcome::assert(
                        !carried,
                        format!(
                            "{} ({read} bytes) {} `{needle}` ({})",
                            path.display(),
                            if carried { "carries" } else { "does not carry" },
                            if carried { "MISMATCH" } else { "as expected" }
                        ),
                    ));
                }
                // The other half of the same reading. A backup is the road home, so what it must carry
                // is as much the question as what an export must not — and a value missing there is one
                // a person has to type in again after every restore.
                if let Some(needle) = with.get("contains").and_then(|v| v.as_str()) {
                    if needle.is_empty() {
                        return Err("`contains` names nothing to look for".to_string());
                    }
                    let (carried, read) = carries_text(&path, needle)?;
                    return Ok(Outcome::assert(
                        carried,
                        format!(
                            "{} ({read} bytes) {} `{needle}` ({})",
                            path.display(),
                            if carried { "carries" } else { "does not carry" },
                            if carried { "as expected" } else { "MISMATCH" }
                        ),
                    ));
                }
                let present = opt_bool(with, "present").unwrap_or(true);
                // An archive is a file with bytes in it. Whether those bytes put a store back is
                // what `restore` answers — asking that here would only be guessing at the layout of
                // something this driver is meant to treat as a black box.
                let bytes = path.metadata().ok().filter(|m| m.is_file()).map(|m| m.len());
                let found = bytes.is_some_and(|n| n > 0);
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "{} {} (expected {}, {})",
                        path.display(),
                        match bytes {
                            Some(n) => format!("holds {n} bytes"),
                            None => "is not a file on disk".to_string(),
                        },
                        if present { "an archive" } else { "nothing" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // What the sweep leaves behind. The reclaim raises no issue and says nothing a machine
            // reads, so the count of files is the only place it is observable.
            "blobs" => {
                let expected = req_i64(with, "count")?;
                let actual = count_files_in(&self.session.home.join("blobs"))? as i64;
                Ok(Outcome::assert(
                    actual == expected,
                    format!(
                        "the store holds {actual} blob file(s) (expected {expected}, {})",
                        if actual == expected { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "doctor" => {
                let want = req_bool(with, "ok")?;
                let v = self.run_check(&["doctor", "--json"])?;
                // Naming a kind asks about the list rather than the verdict. Most of what doctor
                // raises is a warning, which leaves `ok` alone — so a problem appearing, and a repair
                // taking it away, are only visible from here.
                match with.get("issue").and_then(|v| v.as_str()) {
                    None => Ok(judge_check("doctor", want, &v)),
                    Some(kind) => {
                        let present = opt_bool(with, "present").unwrap_or(true);
                        let found = v["issues"]
                            .as_array()
                            .is_some_and(|rows| rows.iter().any(|i| i["kind"].as_str() == Some(kind)));
                        let pass = found == present && v["ok"].as_bool().unwrap_or(false) == want;
                        Ok(Outcome::assert(
                            pass,
                            format!(
                                "`doctor` {} `{kind}` issue (expected {}, {})",
                                if found { "raises a" } else { "raises no" },
                                if present { "raised" } else { "gone" },
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        ))
                    }
                }
            }
            "validate" => {
                let want = req_bool(with, "ok")?;
                // With no target the whole store is checked, which is what a user typing it bare
                // gets; naming one narrows it to that object.
                let mut args: Vec<String> = vec!["validate".into()];
                if with.contains_key("target") {
                    args.push(self.resolve(with)?.to_string());
                }
                args.push("--json".into());
                let v = self.run_check(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(judge_check("validate", want, &v))
            }
            "config" => {
                let v = self.run_json(&["config", "--json"])?;
                judge_field("this store's configuration", with, &v)
            }
            "identity" => {
                let v = self.run_json(&["whoami", "--json"])?;
                judge_field("this store's identity", with, &v)
            }
            "update" => {
                // `--print` is the face that opens nothing, which is the only one a scenario may
                // wear: a check is a read, and it must not launch a browser at whoever runs it.
                let v = self.run_json(&["update", "--print", "--json"])?;
                judge_field("the update check", with, &v)
            }
            _ => Err(unmapped(Domain::Store, op)),
        }
    }

    /// Is the object an earlier step bound in the export written by another? The export is read off
    /// disk as the document it is, because that document is the whole promise of the capability:
    /// what another tool receives is this file, not what amenbo would say about it.
    pub(crate) fn judge_exported(&self, domain: Domain, with: &Args) -> Result<Outcome, String> {
        let target = self.resolve(with)?;
        let dir = self.artifact_ref(with, "from")?;
        let present = opt_bool(with, "present").unwrap_or(true);
        let file = dir.join("export.json");
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("could not read the export at {}: {e}", file.display()))?;
        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("the export at {} is not JSON: {e}", file.display()))?;
        // The tables an object of this domain lands in. A comment is looked for on both timelines:
        // a bound comment id is whichever of the two the step that posted it made.
        let (noun, tables): (&str, &[&str]) = match domain {
            Domain::Task => ("task", &["task"]),
            Domain::Decision => ("decision", &["decision"]),
            Domain::Comment => ("comment", &["task_comment", "decision_comment"]),
            other => return Err(format!("`exported` says nothing about domain `{other:?}`")),
        };
        let found = tables.iter().any(|t| {
            v["tables"][t]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r["id"].as_i64() == Some(target)))
        });
        let pass = found == present;
        Ok(Outcome::assert(
            pass,
            format!(
                "{noun} {target} {} the export at {} under {} (expected {}, {})",
                if found { "is in" } else { "is missing from" },
                file.display(),
                tables.join("/"),
                if present { "carried out" } else { "left behind" },
                if pass { "as expected" } else { "MISMATCH" }
            ),
        ))
    }
}

/// Judge an integrity read: the check reports a verdict of its own, and the step says which one it
/// expects. The issue count rides along in the note, since a red one is only useful with its list.
fn judge_check(tool: &str, want: bool, v: &serde_json::Value) -> Outcome {
    let ok = v["ok"].as_bool().unwrap_or(false);
    let issues = v["issues"].as_array().map(Vec::len).unwrap_or(0);
    Outcome::assert(
        ok == want,
        format!(
            "`{tool}` reports {} over {issues} issue(s) (expected {}, {})",
            if ok { "sound" } else { "problems" },
            if want { "sound" } else { "problems" },
            if ok == want { "as expected" } else { "MISMATCH" }
        ),
    )
}

/// Whether what amenbo wrote at `path` carries `needle` anywhere in its bytes, and how many bytes
/// were read to say so. What a `store` action hands out is one file for a backup and a whole folder
/// for an export, so both shapes are walked — a value that leaked into any one file in there leaked.
fn carries_text(path: &Path, needle: &str) -> Result<(bool, u64), String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if meta.is_file() {
        let bytes =
            std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let carried = bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
        return Ok((carried, bytes.len() as u64));
    }
    let entries = std::fs::read_dir(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let (mut carried, mut read) = (false, 0);
    for entry in entries {
        let entry = entry.map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let (found, n) = carries_text(&entry.path(), needle)?;
        carried |= found;
        read += n;
    }
    Ok((carried, read))
}

/// Every blob file the store is holding — the flat hash-named files, and nothing else. `tmp/` is the
/// staging area an ingest writes through, not stored bytes, so it is a directory and skipped by the
/// same rule that skips any directory.
fn blob_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // A store that has never held an attachment has no directory at all, which is zero blobs and
        // not a failure to read.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("could not read {}: {e}", dir.display())),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("could not read {}: {e}", dir.display()))?;
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            files.push(entry.path());
        }
    }
    Ok(files)
}

fn count_files_in(dir: &Path) -> Result<usize, String> {
    blob_files(dir).map(|f| f.len())
}

/// Backdate every blob file two hours, comfortably past the hour a young blob is spared for.
fn age_files_in(dir: &Path) -> Result<usize, String> {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
    let files = blob_files(dir)?;
    for path in &files {
        let f = std::fs::File::options()
            .write(true)
            .open(path)
            .map_err(|e| format!("could not open {}: {e}", path.display()))?;
        f.set_modified(old).map_err(|e| format!("could not age {}: {e}", path.display()))?;
    }
    Ok(files.len())
}
