//! The `project` domain: the one bucket a task belongs to — its fields, its place in the order,
//! whether it is still in play, and the deletion that lets go of the folders bound to it.

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, req_str, unmapped, Driver, Outcome};
use crate::judge::{judge_field, judge_listing};

impl Driver<'_> {
    pub(crate) fn project_action(&mut self, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            "create" => {
                let name = req_str(with, "name")?;
                // Creating a project links a folder to it, so there is always one. A step that is
                // about the linking names it (`dir:`, the same scratch folder every folder step
                // takes); one that is only after somewhere to file work leaves it unnamed and gets a
                // folder named after the project, which keeps two projects of one session out of each
                // other's way.
                let dir = match with.get("dir") {
                    Some(_) => self.folder(with)?,
                    None => self
                        .session
                        .folder(&folder_name(name))
                        .map_err(|e| format!("could not make a folder for `{name}`: {e}"))?,
                };
                let path = dir.to_string_lossy().into_owned();
                let v = self.run_json(&["project", "add", "--name", name, "--dir", &path, "--json"])?;
                let id = v["project"]["id"].as_i64().ok_or("project add did not report an id")?;
                if let Some(b) = bind {
                    self.bindings.insert(b.to_string(), id);
                }
                Ok(Outcome::action(format!("created project {id} `{name}`, linked to {path}")))
            }
            "update" => {
                let target = self.resolve(with)?;
                let id = target.to_string();
                let mut args: Vec<String> = vec!["project".into(), "update".into(), id];
                for key in ["name", "notes", "view"] {
                    if let Some(v) = with.get(key) {
                        let v = v.as_str().ok_or_else(|| format!("arg `{key}` must be a string"))?;
                        args.push(format!("--{key}"));
                        args.push(v.to_string());
                    }
                }
                if args.len() == 3 {
                    return Err("`update` names no field to set".to_string());
                }
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("updated project {target}")))
            }
            "move" => {
                let target = self.resolve(with)?;
                let pos = req_str(with, "position")?;
                let flag = match pos {
                    "top" | "bottom" => format!("--{pos}"),
                    other => return Err(format!("`position: {other}` is not top or bottom")),
                };
                self.run_json(&["project", "move", &target.to_string(), &flag, "--json"])?;
                Ok(Outcome::action(format!("moved project {target} to the {pos}")))
            }
            verb @ ("archive" | "unarchive") => {
                let target = self.resolve(with)?;
                self.run_json(&["project", verb, &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("{verb}d project {target}")))
            }
            "delete" => {
                let target = self.resolve(with)?;
                self.delete_project(target)?;
                Ok(Outcome::action(format!("deleted project {target}")))
            }
            _ => Err(unmapped(Domain::Project, op)),
        }
    }
    /// Take one project off the store. Destructive, and the run has nobody to ask: `--yes` is how a
    /// non-interactive caller declares the confirmation the command would otherwise stop for.
    ///
    /// Read for its exit code rather than for JSON: the delete face reports nothing on its way out,
    /// so a caller waiting for an object waits for one that never comes. Shared with the premise
    /// that empties the store (`store nothing-raised`), so the shape of the call is settled once.
    pub(crate) fn delete_project(&self, id: i64) -> Result<(), String> {
        self.run_bare(&["project", "delete", &id.to_string(), "--yes"])
    }

    pub(crate) fn project_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "listed" => {
                let target = self.resolve(with)?;
                // The archived ones are a listing of their own: they leave the everyday list, which is
                // what archiving is for, so proving one went means asking both.
                let archived = opt_bool(with, "archived").unwrap_or(false);
                let mut args = vec!["project", "list"];
                if archived {
                    args.push("--archived");
                }
                args.push("--json");
                let v = self.run_json(&args)?;
                let rows = v["projects"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let listing =
                    if archived { "the listing that carries archived projects" } else { "the project listing" };
                judge_listing("project", target, listing, rows, with)
            }
            "field" => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["project", "show", &target.to_string(), "--json"])?;
                judge_field(&format!("project {target}"), with, &v)
            }
            _ => Err(unmapped(Domain::Project, op)),
        }
    }
}

/// A folder name to hold `name`'s project: the letters, digits, `.`, `_` and `-` of the project's
/// name, with every run of anything else collapsed to a single `-`. Project names are written for
/// people — spaces, dashes of several widths, Japanese — and this is only the scratch folder the run
/// links, so what it owes is a path that is legible in a failure message and distinct between two
/// projects of one session.
///
/// It is also the way back: a later step that names a project is looking for the folder bound to it,
/// and the name is all it has to find one by ([`Driver::project_folder`]). So the mapping is a
/// function of the project's name and nothing else — no counter, no order of creation.
pub(crate) fn folder_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() { "project".to_string() } else { trimmed.to_string() }
}
