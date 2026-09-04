//! The `folder` domain: a working directory and the project its `.amenbo` pointer names — what an
//! AI launched there may reach. Every question here is asked from inside the folder it is about,
//! since the pointer is found by walking up from the CWD.

use std::path::{Path, PathBuf};

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, req_i64, req_str, unmapped, Driver, Outcome};

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
                    None => self.standing_project()?,
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
            // Leave the folder's pointer in the shape an older Amenbo wrote: a `project_id` that is
            // not the integer key. Nothing Amenbo ships writes one any more, and that is the point —
            // this is the state on disk that `doctor --fix` exists to put right, so the scenario has
            // to make it, exactly as `repo write-file` makes the file a person already had.
            //
            // It is written from outside the folder rather than by running anything in it: Amenbo
            // heals the pointer of a folder it is run in, so a visit would undo this before the
            // repair under test ever saw it.
            "legacy-pointer" => {
                let dir = self.folder(with)?;
                let pointer = dir.join(".amenbo");
                std::fs::write(&pointer, br#"{"v":1,"project_id":"a-name-not-a-key"}"#)
                    .map_err(|e| format!("could not write {}: {e}", pointer.display()))?;
                Ok(Outcome::action(format!("left {} pointing the way an older build did", dir.display())))
            }
            // Leave the folder claimed by another store: this run's own pointer, with a different
            // store's name stamped on it. Copied rather than hand-written, and that is the whole
            // fixture — every other field then *agrees*, which is the state a store name exists to
            // catch. The id is a live key in this store and the slug beside it is that project's, so
            // the cross-check that would otherwise notice a stray pointer says nothing, and a build
            // reading it would go quietly to work under a number that means something here too.
            //
            // Written from outside the folder, the way `legacy-pointer` is: a build run inside brings
            // a pointer forward to its own shape, so a visit would take the claim off before the guard
            // under test ever met it.
            "foreign-pointer" => {
                let dir = self.folder(with)?;
                let by = req_str(with, "store")?;
                let ours = self.session.cwd.join(".amenbo");
                let text = std::fs::read_to_string(&ours)
                    .map_err(|e| format!("could not read the run's own pointer at {}: {e}", ours.display()))?;
                let claimed = dir.join(".amenbo");
                std::fs::write(&claimed, claimed_by(&text, by)?)
                    .map_err(|e| format!("could not write {}: {e}", claimed.display()))?;
                Ok(Outcome::action(format!("left {} claimed by {by}, everything but the name agreeing", dir.display())))
            }
            // Leave the folder naming a project this store does not have: the run's own pointer with
            // the number moved out of range. Copied rather than written from parts, for the reason
            // `foreign-pointer` is copied — the shape and the store's name are this build's, so a
            // build that turned the folder away over its version, or picked it up as another store's,
            // would be answering something this road is not about.
            //
            // Written from outside the folder, the way its two neighbours are: a build run in there
            // answers for the pointer it finds, and this is a pointer a road wants answered for on
            // the way in rather than before anyone got there.
            "lost-pointer" => {
                let dir = self.folder(with)?;
                let ours = self.session.cwd.join(".amenbo");
                let text = std::fs::read_to_string(&ours)
                    .map_err(|e| format!("could not read the run's own pointer at {}: {e}", ours.display()))?;
                let lost = dir.join(".amenbo");
                std::fs::write(&lost, pointing_nowhere(&text)?)
                    .map_err(|e| format!("could not write {}: {e}", lost.display()))?;
                Ok(Outcome::action(format!(
                    "left {} naming project {NO_SUCH_PROJECT}, which this store does not have",
                    dir.display()
                )))
            }
            "sync-guide" => {
                let dir = self.folder(with)?;
                let path = dir.to_string_lossy().into_owned();
                let v = self.run_json(&["sync-guide", "--dir", &path, "--json"])?;
                let rewritten = v["updated"].as_array().map_or(0, Vec::len);
                Ok(Outcome::action(format!("resynced the guidance in {path} ({rewritten} file(s) rewritten)")))
            }
            // The folder goes and stands somewhere else, with everything lying in it — its pointer
            // above all, which is what makes the new place the same folder rather than a bare
            // directory. Nothing here touches Amenbo: to the registry a rename, a move and a restore
            // beside the original are one event, and it is one it is never told about.
            //
            // Where it stood is remembered canonically and *before* it goes: the registry records the
            // canonical spelling, so that is what every later answer has to be matched against — and
            // once the folder is away there is nothing left on disk to canonicalize.
            "move" => {
                let from = self.folder(with)?;
                let to = self.folder_named(req_str(with, "to")?)?;
                let was = std::fs::canonicalize(&from).unwrap_or_else(|_| from.clone());
                move_folder(&from, &to)?;
                self.moved.insert(req_str(with, "dir")?.to_string(), was.clone());
                Ok(Outcome::action(format!(
                    "moved {} to {} — the path its binding holds leads nowhere now",
                    was.display(),
                    to.display()
                )))
            }
            // The binding that folder had, brought onto where it stands now. The id is not the road's
            // to know, so it is read the way its reader reads it: from the answer `bind` gives in a
            // folder whose project has one gone, which lines the vanished bindings up by id. That is
            // also why the step names the folder that moved rather than a number.
            "rebind" => {
                let dir = self.folder(with)?;
                let gone = self.moved_path(with, "moved")?;
                let pid = match with.get("project") {
                    Some(_) => self.resolve_key(with, "project")?,
                    None => self.standing_project()?,
                };
                let id = self.vanished_id(&dir, &gone)?;
                let path = dir.to_string_lossy().into_owned();
                let v = self.run_json(&[
                    "bind", "--project", &pid.to_string(), "--rebind", &id.to_string(), "--dir", &path, "--json",
                ])?;
                self.last_rebind = Some(v);
                Ok(Outcome::action(format!("re-pointed binding {id} at {path}, keeping its id")))
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
            // Asked of the project rather than of the folder: the list its settings screen is built
            // from. A folder re-pointed elsewhere has to leave this list — the pointer it now holds
            // names one project, so a second one going on offering the folder is a way in that leads
            // somewhere else.
            "listed" => {
                let dir = self.folder(with)?;
                let pid = self.resolve_key(with, "project")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                // The store records the folder canonicalized (symlinks resolved), which is the spelling
                // that comes back here; the run's own path may still carry the symlinked parent.
                let want = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
                let v = self.run_json(&["project", "show", &pid.to_string(), "--json"])?;
                let listed = lists_folder(&v, &want);
                let pass = listed == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "project {pid} {} {} (expected {}, {})",
                        if listed { "lists" } else { "does not list" },
                        want.display(),
                        if present { "listed" } else { "not listed" },
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
            // Amenbo does not go quietly to work in whatever is left of a project one of whose folders
            // has vanished: the read stops, and the answer lines the gone bindings up by id beside the
            // command that re-points them. Asked from a folder that is still there — the moved one's
            // new home is one, since the pointer travelled with it.
            //
            // A refusal that never came, one that came for another reason, and one that came without
            // the id in it are all the same verdict here: the door this road walks through is shut.
            "vanished" => {
                let dir = self.folder(with)?;
                let gone = self.moved_path(with, "gone")?;
                Ok(match self.vanished_id(&dir, &gone) {
                    Ok(id) => Outcome::assert(
                        true,
                        format!(
                            "the read in {} stopped and listed binding {id}, still pointing at {} (as expected)",
                            dir.display(),
                            gone.display()
                        ),
                    ),
                    Err(why) => Outcome::assert(
                        false,
                        format!("{} is not offered for re-pointing by id (MISMATCH): {why}", gone.display()),
                    ),
                })
            }
            // What the re-point just before this one answered about the binding it moved: the id it
            // kept, the folder it names now, and the path it named before. All three off that one
            // answer — afterwards there is only the state it left, which carries no id at all.
            "repointed" => {
                let dir = self.folder(with)?;
                let was = self.moved_path(with, "previously")?;
                let last = self
                    .last_rebind
                    .as_ref()
                    .ok_or("no binding has been re-pointed yet, so there is no answer to read one off")?;
                let id = last["binding"]["binding_id"].as_i64();
                let now = last["binding"]["dir"].as_str().map(PathBuf::from);
                let before = last["binding"]["previous_dir"].as_str().map(PathBuf::from);
                // The store records the folder canonicalized, which is the spelling that comes back.
                let want = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
                let pass = id.is_some() && now.as_deref() == Some(want.as_path()) && before.as_deref() == Some(was.as_path());
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the re-point answered {} now naming {} (was {}) — expected the one that pointed at {} to name {}, {}",
                        match id {
                            Some(n) => format!("binding {n}"),
                            None => "no binding id".to_string(),
                        },
                        show(now.as_deref()),
                        show(before.as_deref()),
                        was.display(),
                        want.display(),
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // Work in a folder another store claimed. `status` is what a reader types when they walk
            // in and ask what there is to do — an ordinary read, which is the point: the guard stands
            // in front of everything that would resolve the pointer, so an extraordinary command would
            // be the weaker witness.
            //
            // Three ways this comes out red, and they are one verdict: the command went through (the
            // regression this exists to catch), it stopped for some other reason, or it stopped
            // without saying whose folder this is — a reader turned away with no store named has
            // nothing to act on.
            "claimed" => {
                let dir = self.folder(with)?;
                let by = req_str(with, "store")?;
                Ok(match self.refusal_in(&dir, &["status", "--json"]) {
                    Ok(err) => {
                        let code = err["code"].as_str().unwrap_or_default();
                        let said = err["message"].as_str().unwrap_or_default();
                        let pass = code == "pointer_other_store" && said.contains(by);
                        Outcome::assert(
                            pass,
                            format!(
                                "work in {} stopped with `{code}`, saying: {said} (expected the pointer guard, naming {by}, {})",
                                dir.display(),
                                if pass { "as expected" } else { "MISMATCH" }
                            ),
                        )
                    }
                    Err(why) => Outcome::assert(
                        false,
                        format!("work in {} was not turned away (MISMATCH): {why}", dir.display()),
                    ),
                })
            }
            _ => Err(unmapped(Domain::Folder, op)),
        }
    }

    /// Where a folder a step moved used to stand. It is looked up rather than asked of the session,
    /// because asking places it: a path put back is a path that leads somewhere, and a folder that
    /// leads nowhere is the whole state this road is about. A name no `move` bound is the road
    /// written out of order, which is what the message says.
    fn moved_path(&self, with: &Args, key: &str) -> Result<PathBuf, String> {
        let name = req_str(with, key)?;
        self.moved.get(name).cloned().ok_or_else(|| {
            format!("`{key}: {name}` names no folder this run moved — a binding is re-pointed after its folder goes, not before")
        })
    }

    /// The id of the binding still pointing at `gone`, read the way the reader reading it does: from
    /// what `bind` answers in a folder whose project has one that vanished. Asking is the only way —
    /// a binding's id is in no read Amenbo offers — which is exactly why the answer carries it.
    fn vanished_id(&self, ask_from: &Path, gone: &Path) -> Result<i64, String> {
        let err = self.refusal_in(ask_from, &["bind", "--json"])?;
        let code = err["code"].as_str().unwrap_or_default();
        if code != "binding_stale" {
            return Err(format!("the read stopped with `{code}` rather than on the folder that vanished"));
        }
        let hint = err["hint"].as_str().unwrap_or_default();
        vanished_bindings(hint)
            .into_iter()
            .find(|(_, dir)| Path::new(dir) == gone)
            .map(|(id, _)| id)
            .ok_or_else(|| format!("no line of the answer offers it by id: {hint}"))
    }
}

/// A folder and everything lying in it come to stand at `to`, and the path it was at is taken away.
/// Entry by entry rather than one rename of the folder itself, because both names are folders the
/// session has already placed — a scenario names folders and the driver puts them somewhere, so the
/// destination exists before anything is moved into it.
fn move_folder(from: &Path, to: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(from).map_err(|e| format!("could not read {}: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("could not read {}: {e}", from.display()))?;
        let dest = to.join(entry.file_name());
        std::fs::rename(entry.path(), &dest)
            .map_err(|e| format!("could not move {} to {}: {e}", entry.path().display(), dest.display()))?;
    }
    std::fs::remove_dir_all(from).map_err(|e| format!("could not take {} away: {e}", from.display()))
}

/// The bindings a `binding_stale` answer offers for re-pointing, as the id and the path each one is
/// still holding. One line per vanished binding, each written as the command that moves it with the
/// path it points at beside — so the reading is taken on the two things a road needs and not on the
/// sentence they are written into: a line that names no `--rebind` is prose about them, not one of
/// them.
fn vanished_bindings(hint: &str) -> Vec<(i64, String)> {
    hint.lines()
        .filter_map(|line| {
            let id: i64 = line.split("--rebind ").nth(1)?.split_whitespace().next()?.parse().ok()?;
            let dir = line.rsplit_once('(')?.1.trim_end().strip_suffix(')')?;
            Some((id, dir.to_string()))
        })
        .collect()
}

/// The same pointer, claimed by another store. Only the one field is written: the fixture is worth
/// making precisely because everything else stays as the build under test wrote it, so a pointer
/// rebuilt from parts here — a version this build does not write, a field it has since added — would
/// be turned away for a reason the road is not about.
fn claimed_by(pointer: &str, store: &str) -> Result<String, String> {
    let mut v: serde_json::Value =
        serde_json::from_str(pointer).map_err(|e| format!("the run's own pointer is not JSON ({e}): {pointer}"))?;
    v["store"] = serde_json::Value::String(store.to_string());
    Ok(v.to_string())
}

/// The number a lost pointer names. It is written down rather than read off the store, because a
/// store's ids are its own to hand out: a road that asked for the highest one and added to it would
/// be racing the writes it is standing on, and one that named a small number would collide with the
/// project the run itself raised. Far enough out that nothing a run creates reaches it.
const NO_SUCH_PROJECT: i64 = 9_000_000;

/// The same pointer, naming a project nothing in this store answers for. Only the number is moved,
/// for the reason `claimed_by` moves only the name: everything a build reads before it resolves that
/// number stays as this build wrote it, so what the folder is met with on the way in is about the
/// number alone.
fn pointing_nowhere(pointer: &str) -> Result<String, String> {
    let mut v: serde_json::Value =
        serde_json::from_str(pointer).map_err(|e| format!("the run's own pointer is not JSON ({e}): {pointer}"))?;
    v["project_id"] = serde_json::Value::from(NO_SUCH_PROJECT);
    Ok(v.to_string())
}

/// A path as a note names it, and what to write where there is none to name.
fn show(path: Option<&Path>) -> String {
    path.map_or_else(|| "nothing".to_string(), |p| p.display().to_string())
}

/// Does what `project show` answered list this folder? The rows carry the path as a string, so the
/// comparison is made on `Path` rather than on the text: two spellings of one folder are one folder,
/// and a road that asked about a folder it made would otherwise read a miss as an answer.
fn lists_folder(shown: &serde_json::Value, want: &std::path::Path) -> bool {
    shown["bound_folders"].as_array().is_some_and(|folders| {
        folders.iter().any(|f| f["path"].as_str().is_some_and(|p| std::path::Path::new(p) == want))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reading `listed` is taken on: a project's own answer about its folders, matched as a path.
    /// A project that lists other folders is a miss and not an error — that is the very state
    /// `present: false` asks for, and the two have to be told apart.
    #[test]
    fn a_projects_folder_list_is_read_as_paths_and_a_project_holding_none_is_a_miss() {
        let shown = serde_json::json!({
            "bound_folders": [
                { "path": "/work/one", "exists": true },
                { "path": "/work/two/", "exists": true },
            ]
        });
        assert!(lists_folder(&shown, Path::new("/work/one")));
        // A trailing slash is the same folder: `Path` compares component by component.
        assert!(lists_folder(&shown, Path::new("/work/two")));
        assert!(!lists_folder(&shown, Path::new("/work/three")));
        // A folder that only prefixes a listed one is not listed.
        assert!(!lists_folder(&shown, Path::new("/work/on")));

        // A project with no folder at all answers with an empty list, and one whose answer carries no
        // list at all is read the same way rather than blowing up mid-road.
        assert!(!lists_folder(&serde_json::json!({ "bound_folders": [] }), Path::new("/work/one")));
        assert!(!lists_folder(&serde_json::json!({}), Path::new("/work/one")));
    }

    /// The one door to a binding's id: the answer `bind` gives where a folder has vanished. Every
    /// line that offers one is read, and the prose around them — which names the command too, without
    /// an id — is not, since a road that read it would re-point whatever the sentence happened to
    /// mention.
    #[test]
    fn the_bindings_offered_for_re_pointing_are_read_off_the_answer_and_the_prose_around_them_is_not() {
        let hint = "The folder moved? Re-point that binding from its new home, so whatever points at it follows:\n  \
                    • amenbo bind --project 4 --rebind 7   (/work/gone)\n  \
                    • amenbo bind --project 4 --rebind 12   (/work/also gone)\n\
                    Binding it again with `amenbo bind --project 4` instead records a new binding, leaving anything filed at the old one naming nothing.";
        assert_eq!(
            vanished_bindings(hint),
            vec![(7, "/work/gone".to_string()), (12, "/work/also gone".to_string())],
        );

        // A build that stopped offering them leaves nothing to read, which is a road that stops
        // rather than one that re-points a number it guessed.
        assert!(vanished_bindings("the linked project directory was not found: /work/gone").is_empty());
    }

    /// A pointer another store claimed: the one this build wrote, with one name changed and nothing
    /// else touched. What makes it the fixture is what it keeps — the id is a live key here and the
    /// slug is that project's, so every check but the name agrees — and a build that wrote no name at
    /// all is claimed all the same, since the field is added rather than replaced.
    #[test]
    fn a_claimed_pointer_keeps_everything_this_build_wrote_but_the_store_it_names() {
        let ours = r#"{"v":2,"project_id":4,"slug":"greenhouse","store":"amenbo"}"#;
        let claimed: serde_json::Value = serde_json::from_str(&claimed_by(ours, "amenbo-dev").unwrap()).unwrap();
        assert_eq!(claimed["store"], "amenbo-dev");
        assert_eq!(claimed["project_id"], 4, "the id is a live key in the store that is refusing");
        assert_eq!(claimed["slug"], "greenhouse", "and the cross-check beside it agrees too");
        assert_eq!(claimed["v"], 2, "written in the shape this build reads");

        let older = claimed_by(r#"{"v":1,"project_id":4}"#, "amenbo-dev").unwrap();
        assert!(older.contains("amenbo-dev"), "a pointer that named no store is claimed by adding one");

        // A pointer that could not be read is the run's own gone wrong, which is a failure to report
        // rather than a folder to leave half claimed.
        assert!(claimed_by("not json", "amenbo-dev").is_err());
    }

    /// A pointer that leads nowhere: the one this build wrote, with the number moved past anything a
    /// run hands out and nothing else touched. What makes it the fixture is what it keeps — the shape
    /// and the store's name are this build's, so the folder reads as its own right up to the number,
    /// which is the whole of what is being met on the way in.
    #[test]
    fn a_lost_pointer_keeps_everything_this_build_wrote_but_the_project_it_names() {
        let ours = r#"{"v":2,"project_id":4,"slug":"greenhouse","store":"amenbo"}"#;
        let lost: serde_json::Value = serde_json::from_str(&pointing_nowhere(ours).unwrap()).unwrap();
        assert_eq!(lost["project_id"], NO_SUCH_PROJECT, "the number leads nowhere");
        assert_eq!(lost["store"], "amenbo", "and the build that wrote it is still this one");
        assert_eq!(lost["v"], 2, "written in the shape this build reads");
        assert_eq!(lost["slug"], "greenhouse", "the cross-check beside it is left as it stood");

        // The run's own pointer gone wrong is a failure to report, not a folder to leave half written.
        assert!(pointing_nowhere("not json").is_err());
    }

    /// A folder that moved is the same folder: what was lying in it — the pointer above all — is
    /// standing in the new place, and the path it was at leads nowhere.
    #[test]
    fn a_moved_folder_arrives_with_what_was_in_it_and_leaves_nothing_behind() {
        // Through the session, by the same rules a run's own folders are placed under: one parent, and
        // a name two tests running at once cannot collide on.
        let session = crate::scratch::session("selftest-move", false).unwrap();
        let from = session.folder("was").unwrap();
        let to = session.folder("now").unwrap();
        std::fs::write(from.join(".amenbo"), br#"{"v":1,"project_id":4}"#).unwrap();
        std::fs::write(from.join("AGENTS.md"), b"managed block").unwrap();

        move_folder(&from, &to).unwrap();

        assert!(!from.exists(), "the path the binding holds leads nowhere now");
        assert!(to.join(".amenbo").is_file(), "the pointer travelled with the folder");
        assert!(to.join("AGENTS.md").is_file(), "and so did everything else lying in it");
    }
}
