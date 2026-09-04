//! The `comment` domain: what was said on a task's timeline, and what becomes of a line once it
//! is there — edited, deleted, promoted into a decision, or erased from the truth source outright.

use amenbo_scenario::{Args, Domain};

use crate::{req_str, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn comment_action(&mut self, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            "edit" => {
                let target = self.resolve(with)?;
                let text = req_str(with, "text")?;
                self.run_json(&["comment", "edit", &target.to_string(), "--text", text, "--json"])?;
                Ok(Outcome::action(format!("rewrote comment {target}")))
            }
            "rm" => {
                let target = self.resolve(with)?;
                self.run_json(&["comment", "rm", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("deleted comment {target}")))
            }
            "promote" => {
                let target = self.resolve(with)?;
                let title = req_str(with, "title")?;
                // The two comment tables number independently, so a store holding both can have this id
                // twice and a bare number is refused. This domain is the task side; say so in the ref.
                let target_ref = format!("AMB-TC-{target}");
                // What comes out is a decision, so the axes a project requires are read here as they
                // are at `decision add` — and answered the same way, on the way in.
                let mut args: Vec<String> =
                    vec!["decision".into(), "promote".into(), target_ref, "--title".into(), title.into(), "--json".into()];
                if let Some(dim) = with.get("dimension").and_then(|v| v.as_str()) {
                    args.push("--dim".into());
                    args.push(format!("{dim}={}", req_str(with, "value")?));
                }
                let v = self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                let id = v["decision"]["id"].as_i64().ok_or("decision promote did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("promoted comment {target} into decision {id}")))
            }
            "hard-erase" => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["hard-erase", "comment", &target.to_string(), "--yes", "--json"])?;
                let safety = v["backup"]["path"].as_str().unwrap_or("(none reported)");
                Ok(Outcome::action(format!(
                    "erased comment {target} from the truth source (safety archive: {safety})"
                )))
            }
            _ => Err(unmapped(Domain::Comment, op)),
        }
    }
}
