//! The `folder` domain: a working directory and the project its `.amenbo` pointer names — what an
//! AI launched there may reach. Every question here is asked from inside the folder it is about,
//! since the pointer is found by walking up from the CWD.

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, req_i64, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn folder_action(&mut self, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            "init" => {
                let dir = self.folder(with)?;
                // Run it *from inside* the folder: `init` binds where it stands, and the project it
                // raises is named after that folder.
                let v = self.run_json_in(&dir, &["init", "--json"])?;
                let id = v["identity"]["project_id"].as_i64().ok_or("init did not report a project_id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("initialised {} as project {id}", dir.display())))
            }
            "bind" => {
                // Which project a folder is pointed at: this run's own unless the step names another.
                let pid = match with.get("project") {
                    Some(_) => self.resolve_key(with, "project")?,
                    None => self.project_id,
                };
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                self.run_json(&["bind", "--project", &pid.to_string(), "--dir", &path, "--json"])?;
                Ok(Outcome::action(format!("bound {path} to project {pid}")))
            }
            "unbind" => {
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                // Removing a pointer asks first; the driver is unattended, so it answers up front.
                let v = self.run_json(&["unbind", "--dir", &path, "--yes", "--json"])?;
                self.last_unbind = Some(v);
                Ok(Outcome::action(format!("unbound {path}")))
            }
            // Leave the folder's pointer in the shape an older amenbo wrote: a `project_id` that is
            // not the integer key. Nothing amenbo ships writes one any more, and that is the point —
            // this is the state on disk that `doctor --fix` exists to put right, so the scenario has
            // to make it, exactly as `repo write-file` makes the file a person already had.
            //
            // It is written from outside the folder rather than by running anything in it: amenbo
            // heals the pointer of a folder it is run in, so a visit would undo this before the
            // repair under test ever saw it.
            "legacy-pointer" => {
                let dir = self.folder(with)?;
                let pointer = dir.join(".amenbo");
                std::fs::write(&pointer, br#"{"v":1,"project_id":"a-name-not-a-key"}"#)
                    .map_err(|e| format!("could not write {}: {e}", pointer.display()))?;
                Ok(Outcome::action(format!("left {} pointing the way an older build did", dir.display())))
            }
            "sync-guide" => {
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                let v = self.run_json(&["sync-guide", "--dir", &path, "--json"])?;
                let rewritten = v["updated"].as_array().map_or(0, Vec::len);
                Ok(Outcome::action(format!("resynced the guidance in {path} ({rewritten} file(s) rewritten)")))
            }
            _ => Err(unmapped(Domain::Folder, op)),
        }
    }
    pub(crate) fn folder_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "bound" => {
                let dir = self.folder(with)?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let want = match with.get("project") {
                    Some(_) => Some(self.resolve_key(with, "project")?),
                    None => None,
                };
                // Asked from inside the folder, which is the question an AI launched there asks.
                let v = self.run_json_in(&dir, &["bind", "--json"])?;
                let at = v["binding"]["project_id"].as_i64();
                let found = match (at, want) {
                    (Some(id), Some(named)) => id == named,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "{} {} (expected {}, {})",
                        dir.display(),
                        match at {
                            Some(id) => format!("points at project {id}"),
                            None => "points at no project".to_string(),
                        },
                        match (present, want) {
                            (true, Some(named)) => format!("project {named}"),
                            (true, None) => "a binding".to_string(),
                            (false, _) => "none".to_string(),
                        },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // What the unbind just before this one answered about the project it took the folder from.
            // Read off that answer and not off the store: with the pointer gone there is only the
            // state left behind, which is the same whether the answer said a word about it or not.
            "folders-left" => {
                let want = req_i64(with, "left")?;
                let last = self
                    .last_unbind
                    .as_ref()
                    .ok_or("no folder has been unbound yet, so there is no answer to read the count off")?;
                let left = last["binding"]["project_folders_left"].as_i64();
                let pass = left == Some(want);
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the unbind answered {} (expected {want}, {})",
                        match left {
                            Some(n) => format!("{n} folder(s) left on the project"),
                            None => "nothing about how many folders are left".to_string(),
                        },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "resynced" => {
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                let changed = opt_bool(with, "changed").unwrap_or(false);
                // A resync writes only what actually differs, so "nothing left to write" is how a
                // folder says its block is already at this build's version — and it is the property
                // that makes the command safe to point at every folder on the device.
                let v = self.run_json(&["sync-guide", "--dir", &path, "--json"])?;
                let rewritten = v["updated"].as_array().map_or(0, Vec::len);
                let pass = (rewritten > 0) == changed;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "a resync of {path} rewrote {rewritten} file(s) (expected {}, {})",
                        if changed { "a rewrite" } else { "nothing to do" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Folder, op)),
        }
    }
}
