//! The `comment` domain: what was said on a task's timeline, and what becomes of a line once it
//! is there — edited, deleted, promoted into a decision, or erased from the truth source outright.

use amenbo_scenario::{Args, Domain};

use crate::{req_str, unmapped, Driver, Outcome};

impl Driver {
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
                let v = self.run_json(&["decision", "promote", &target_ref, "--title", title, "--json"])?;
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
