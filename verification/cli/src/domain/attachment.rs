//! The `attachment` domain: the one place Amenbo carries bytes. Hanging a file or a link on a
//! task, a decision or a comment — the three owners differ only in the command that takes them —
//! and reading it back as a row, in its owner's list, and as the bytes coming out again.

use amenbo_scenario::{Args, Domain};

use crate::{req_str, unmapped, Driver, Outcome};
use crate::judge::{judge_field, judge_listing};

impl Driver<'_> {
    pub(crate) fn attachment_action(&mut self, domain: Domain, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            // Hanging bytes or a link on a record. The three owners differ only in the command that
            // takes them, so one arm carries all three and the domain says which.
            "attach" => {
                let target = self.resolve(with)?;
                let (noun, argv0): (&str, &[&str]) = match domain {
                    Domain::Task => ("task", &["task", "attach"]),
                    Domain::Decision => ("decision", &["decision", "attach"]),
                    Domain::Comment => ("comment", &["comment", "attach"]),
                    // Nothing else carries attachments, and guessing one of the three would hang the
                    // bytes on a record the step never named.
                    other => return Err(unmapped(other, op)),
                };
                let id = target.to_string();
                let mut args: Vec<String> = argv0.iter().map(|s| s.to_string()).collect();
                args.push(id);
                // A blob is ingested from a file the run wrote (`repo write-file`); a link is the
                // URL itself. One or the other — an attach that names neither has nothing to hang.
                match (with.get("file").and_then(|v| v.as_str()), with.get("url").and_then(|v| v.as_str())) {
                    (Some(file), None) => {
                        self.in_session(file)?; // refuse a path that reaches out of the run's folder
                        args.push(file.to_string());
                    }
                    (None, Some(url)) => {
                        args.push(url.to_string());
                        args.push("--url".into());
                    }
                    _ => return Err("`attach` names either a `file` or a `url`, and exactly one".to_string()),
                }
                if let Some(name) = with.get("name").and_then(|v| v.as_str()) {
                    args.push("--name".into());
                    args.push(name.to_string());
                }
                args.push("--json".into());
                let v = self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                let att = v["attachment"]["id"].as_i64().ok_or("attach did not report an id")?;
                if let Some(b) = bind {
                    self.bindings.insert(b.to_string(), att);
                }
                Ok(Outcome::action(format!("attached {att} to {noun} {target}")))
            }
            "rm" => {
                let target = self.resolve(with)?;
                self.run_json(&["attach", "rm", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("removed attachment {target}")))
            }
            _ => Err(unmapped(Domain::Attachment, op)),
        }
    }
    pub(crate) fn attachment_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "field" => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["attach", "show", &target.to_string(), "--json"])?;
                judge_field(&format!("attachment {target}"), with, &v)
            }
            "listed" => {
                let target = self.resolve(with)?;
                let id = self.resolve_key(with, "owner")?;
                // Which list to ask is not something the id can say. Tasks and decisions number in
                // sibling spaces, so the same number can name one of each and a bare id is refused as
                // ambiguous — the owner is named in full. A comment is reached by a flag instead: the
                // two comment tables number apart, and there is no ref that says which one it is.
                let kind = req_str(with, "owner_kind")?;
                let owner = match kind {
                    "decision" => format!("AMB-D-{id}"),
                    "task" => format!("AMB-T-{id}"),
                    _ => id.to_string(),
                };
                let args: Vec<&str> = match kind {
                    "task" | "decision" => vec!["attach", "ls", &owner, "--json"],
                    "task-comment" => vec!["attach", "ls", "--task-comment", &owner, "--json"],
                    "decision-comment" => vec!["attach", "ls", "--decision-comment", &owner, "--json"],
                    other => {
                        return Err(format!(
                            "`owner_kind: {other}` is not task / decision / task-comment / decision-comment"
                        ))
                    }
                };
                let v = self.run_json(&args)?;
                let rows = v["attachments"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                judge_listing("attachment", target, &format!("the {kind}'s attachments"), rows, with)
            }
            "saved" => {
                let target = self.resolve(with)?;
                let want = req_str(with, "content")?;
                // Saving the bytes back out is the only thing that proves the ingest kept them: the
                // row says how many bytes there were, the file says which ones.
                let out = self.in_session(&format!("saved-{target}"))?;
                let out_arg = out.to_string_lossy().to_string();
                self.run_json(&[
                    "attach", "save", &target.to_string(), "--out", &out_arg, "--force", "--json",
                ])?;
                let got = std::fs::read_to_string(&out)
                    .map_err(|e| format!("could not read back {}: {e}", out.display()))?;
                let pass = got == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "attachment {target} saved {} byte(s), {}",
                        got.len(),
                        if pass { "the bytes that went in" } else { "MISMATCH against what went in" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Attachment, op)),
        }
    }
}
