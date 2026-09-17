//! The `decision` domain: the append-only "why we chose X", the two stages its writing takes, the
//! edges between decisions, the link that makes one a task's premise, and its own timeline.

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, req_str, unmapped, Driver, Outcome};
use crate::judge::{judge_field, judge_found, judge_timeline};

/// Judge one entry on the shared activity stream: is there a row for this decision naming this event.
/// Both halves of the pair are asked, because either alone would pass over a build that draws one word
/// for everything — a row for the right record saying the wrong thing, or the right word said of some
/// other record.
fn judge_activity(
    id: i64,
    event: &str,
    with: &Args,
    items: &serde_json::Value,
) -> Result<Outcome, String> {
    let rows = items.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let found = rows.iter().any(|it| {
        it["target"]["type"].as_str() == Some("decision")
            && it["target"]["id"].as_i64() == Some(id)
            && it["event"]["kind"].as_str() == Some(event)
    });
    let present = opt_bool(with, "present").unwrap_or(true);
    let pass = found == present;
    Ok(Outcome::assert(
        pass,
        format!(
            "the stream {} `{event}` for decision {id} ({} entries, expected {}, {})",
            if found { "carries" } else { "does not carry" },
            rows.len(),
            if present { "carried" } else { "gone" },
            if pass { "as expected" } else { "MISMATCH" }
        ),
    ))
}

impl Driver<'_> {
    pub(crate) fn decision_action(&mut self, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        match op {
            "create" => {
                let title = req_str(with, "title")?;
                // A step that names no project files it where everything else in the run goes.
                let pid = match with.get("project") {
                    Some(_) => self.resolve_key(with, "project")?.to_string(),
                    None => self.standing_project()?.to_string(),
                };
                // The decision side of the same flag, and the side where it also answers a demand: an
                // axis the project requires is read where the writing ends, so filling it here keeps
                // the refusal off somebody else's press.
                let mut args: Vec<String> =
                    vec!["decision".into(), "add".into(), "--title".into(), title.into(), "--project".into(), pid.clone(), "--json".into()];
                if let Some(dim) = with.get("dimension").and_then(|v| v.as_str()) {
                    args.push("--dim".into());
                    args.push(format!("{dim}={}", req_str(with, "value")?));
                }
                let v = self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                let id = v["decision"]["id"].as_i64().ok_or("decision add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("created decision {id} `{title}`")))
            }
            // The same create, typed inside a pane. What parts it from the one above is the
            // environment and nothing else: a window puts the pane's id and the way back into its
            // conversation on every terminal it opens, and a create reads them there — so a road that
            // means to read the row afterwards has to say the line was run in one.
            //
            // `shows` is a handle rather than anything on a screen. There is no screen here, so the
            // driver stands a pane up under those words and mints what the window would have set.
            "create-in-pane" => {
                let title = req_str(with, "title")?;
                let pane = self.pane(req_str(with, "shows")?);
                let pid = match with.get("project") {
                    Some(_) => self.resolve_key(with, "project")?.to_string(),
                    None => self.standing_project()?.to_string(),
                };
                let v = self.typed_in(pane, |d| {
                    d.run_json(&["decision", "add", "--title", title, "--project", &pid, "--json"])
                })?;
                let id = v["decision"]["id"].as_i64().ok_or("decision add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("recorded decision {id} `{title}` in a pane")))
            }
            "edit" => {
                let target = self.resolve(with)?;
                let body = req_str(with, "body")?;
                self.run_json(&["decision", "edit", &target.to_string(), "--body", body, "--json"])?;
                Ok(Outcome::action(format!("edited the body of decision {target}")))
            }
            "finish-writing" => {
                let target = self.resolve(with)?;
                self.run_json(&["decision", "finish-writing", &target.to_string(), "--json"])?;
                Ok(Outcome::action(format!("finished writing decision {target}")))
            }
            "reject" => {
                let target = self.resolve(with)?;
                let id = target.to_string();
                let mut args: Vec<String> =
                    vec!["decision".into(), "reject".into(), id, "--yes".into(), "--json".into()];
                // The reason is not a field of its own: it lands on the decision's timeline, which is
                // where a later reader looks for why it was not taken.
                if let Some(reason) = with.get("reason").and_then(|v| v.as_str()) {
                    args.push("--reason".into());
                    args.push(reason.to_string());
                }
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("rejected decision {target}")))
            }
            "reopen" => {
                let target = self.resolve(with)?;
                self.run_json(&["decision", "reopen", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("returned decision {target} to discussion")))
            }
            "comment" => {
                let target = self.resolve(with)?;
                let text = req_str(with, "text")?;
                let v = self.run_json(&["decision", "comment", "add", &target.to_string(), "--text", text, "--json"])?;
                let id = v["comment"]["id"].as_i64().ok_or("decision comment add did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("commented on decision {target}")))
            }
            "comment-edit" => {
                let target = self.resolve(with)?;
                let text = req_str(with, "text")?;
                self.run_json(&["decision", "comment", "edit", &target.to_string(), "--text", text, "--json"])?;
                Ok(Outcome::action(format!("rewrote decision comment {target}")))
            }
            "comment-rm" => {
                let target = self.resolve(with)?;
                self.run_json(&["decision", "comment", "rm", &target.to_string(), "--yes", "--json"])?;
                Ok(Outcome::action(format!("deleted decision comment {target}")))
            }
            "comment-promote" => {
                let target = self.resolve(with)?;
                let title = req_str(with, "title")?;
                // The two comment tables number independently, so a store holding both can have this
                // id twice and a bare number is refused. This is the decision side; say so in the ref.
                let target_ref = format!("AMB-DC-{target}");
                let v = self.run_json(&["decision", "promote", &target_ref, "--title", title, "--json"])?;
                let id = v["decision"]["id"].as_i64().ok_or("decision promote did not report an id")?;
                if let Some(name) = bind {
                    self.bindings.insert(name.to_string(), id);
                }
                Ok(Outcome::action(format!("raised decision comment {target} into decision {id}")))
            }
            "link" => {
                let target = self.resolve(with)?;
                let task = self.resolve_key(with, "task")?;
                self.run_json(&["decision", "link", &target.to_string(), &task.to_string(), "--json"])?;
                Ok(Outcome::action(format!("linked decision {target} to task {task}")))
            }
            "supersede" => {
                let target = self.resolve(with)?;
                let old = self.resolve_key(with, "replaces")?;
                self.run_json(&["decision", "supersede", &target.to_string(), "--replaces", &old.to_string(), "--json"])?;
                Ok(Outcome::action(format!("decision {target} replaces decision {old}")))
            }
            "builds-on" => {
                let target = self.resolve(with)?;
                let base = self.resolve_key(with, "on")?;
                self.run_json(&["decision", "builds-on", &target.to_string(), "--on", &base.to_string(), "--json"])?;
                Ok(Outcome::action(format!("decision {target} stands on decision {base}")))
            }
            "hard-erase" => {
                let target = self.resolve(with)?;
                let body = req_str(with, "body")?;
                self.run_json(&["hard-erase", "decision", &target.to_string(), "--body", body, "--yes", "--json"])?;
                Ok(Outcome::action(format!("redacted the body of decision {target}")))
            }
            "unlink" => {
                let target = self.resolve(with)?;
                let other = self.resolve_key(with, "from")?;
                self.run_json(&["decision", "unlink", &target.to_string(), "--from", &other.to_string(), "--json"])?;
                Ok(Outcome::action(format!("decision {target} no longer points at decision {other}")))
            }
            _ => Err(unmapped(Domain::Decision, op)),
        }
    }
    pub(crate) fn decision_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "commented" => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["decision", "comment", "list", &target.to_string(), "--json"])?;
                judge_timeline("decision", target, with, &v["comments"])
            }
            "found" => {
                let target = self.resolve(with)?;
                let words = self.query(with)?;
                let hits = self.search(with)?;
                judge_found("decision", &format!("AMB-D-{target}"), &words, with, &hits["hits"])
            }
            "field" => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["decision", "show", &target.to_string(), "--json"])?;
                judge_field(&format!("decision {target}"), with, &v)
            }
            // The decision's entries on the shared stream. There is no narrowing to one decision —
            // `--task` is the task side's and has no twin here — so the whole system half is read and
            // the row is picked out by the record it points at and the event it names. Reading it
            // unnarrowed is also what a person does: the stream is one stream, and a decision's own
            // page does not carry it.
            "activity" => {
                let target = self.resolve(with)?;
                let event = req_str(with, "event")?;
                let v = self.run_json(&["activity", "--kind", "system", "--json"])?;
                judge_activity(target, event, with, &v["items"])
            }
            // Which session the record says it was made in. What is asked here is identity and not a
            // name: what a pane is called is kept on the pane's own row, which the window writes and a
            // terminal never does, so the line prints the id alone — and the id is the run's, which no
            // road can spell. `pane` names the handle the create was typed under, and the driver holds
            // both ends of that.
            //
            // The way back is read beside it. It is written down for one purpose — opening the
            // conversation again — so a row that kept the pane and lost the handle would read as a way
            // back that is not one.
            "made-in" => {
                let target = self.resolve(with)?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["decision", "show", &target.to_string(), "--json"])?;
                let made = v.get("made_in").filter(|m| !m.is_null());
                let Some(made) = made else {
                    return Ok(Outcome::assert(
                        !present,
                        format!(
                            "decision {target} names no session (expected {}, {})",
                            if present { "one" } else { "none" },
                            if present { "MISMATCH" } else { "as expected" }
                        ),
                    ));
                };
                if !present {
                    return Ok(Outcome::assert(
                        false,
                        format!(
                            "decision {target} names the session `{}` where it had to name none (MISMATCH)",
                            made["pane"].as_str().unwrap_or("?")
                        ),
                    ));
                }
                let pane = self.known_pane(req_str(with, "pane")?)?;
                let same = made["pane"].as_str() == Some(pane.id.as_str());
                let way_back = made["pane_resume"].as_str() == Some(pane.resume.as_str());
                Ok(Outcome::assert(
                    same && way_back,
                    format!(
                        "decision {target} names the session `{}` with the way back `{}` (expected `{}` / `{}`, {})",
                        made["pane"].as_str().unwrap_or("?"),
                        made["pane_resume"].as_str().unwrap_or("?"),
                        pane.id,
                        pane.resume,
                        if same && way_back { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            "listed" => {
                let target = self.resolve(with)?;
                let filter = req_str(with, "filter")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["decision", "list", "--filter", filter, "--json"])?;
                let found = v["decisions"]
                    .as_array()
                    .map(|rows| rows.iter().any(|d| d["id"].as_i64() == Some(target)))
                    .unwrap_or(false);
                let pass = found == present;
                let word = if present { "present in" } else { "absent from" };
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "decision {target} {} `{filter}` (expected {word}, {})",
                        if found { "is present in" } else { "is absent from" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // And turned away rather than answered — the side of an axis narrowed off this face.
            "filter-refused" => {
                let filter = req_str(with, "filter")?;
                let want = req_str(with, "code")?;
                self.refused_read(&["decision", "list", "--filter", filter, "--json"], want)
            }
            "edge" => {
                let target = self.resolve(with)?;
                let other = self.resolve_key(with, "other")?;
                let kind = req_str(with, "kind")?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["decision", "show", &target.to_string(), "--json"])?;
                // A named side that the output does not carry is a mismatch rather than an error,
                // for the reason a `field` path that runs off it is: the scenario is asserting about
                // the shape of the shipped output as much as about what it holds.
                let Some(rows) = v[kind].as_array() else {
                    return Ok(Outcome::assert(
                        false,
                        format!("decision {target} has no `{kind}` side in its show output (MISMATCH)"),
                    ));
                };
                let found = rows.iter().any(|d| d["id"].as_i64() == Some(other));
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "decision {target} {} decision {other} under `{kind}` (expected {}, {})",
                        if found { "points at" } else { "does not point at" },
                        if present { "an edge" } else { "none" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Decision, op)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The row is picked out by all three of what it points at, which side of the store that is, and
    /// what it says. A build that matched on fewer would answer for the wrong record — the two sides
    /// number independently, so a task and a decision really can share an id.
    #[test]
    fn a_row_is_the_one_asked_for_only_when_record_side_and_event_all_agree() {
        let stream = json!([
            { "target": { "type": "task", "id": 7 }, "event": { "kind": "decision.decided" } },
            { "target": { "type": "decision", "id": 9 }, "event": { "kind": "decision.decided" } },
            { "target": { "type": "decision", "id": 7 }, "event": { "kind": "decision.proposed" } },
        ]);
        let asked = |id, event| {
            judge_activity(id, event, &Args::new(), &stream).expect("the judge answers").pass
        };
        assert!(asked(7, "decision.proposed"), "the row for this decision, saying this");
        assert!(!asked(7, "decision.decided"), "the same word, on the task side and on another decision");
        assert!(!asked(9, "decision.rejected"), "a word nothing on the stream says");
    }

    /// `present: false` is the other half of the same question, and the one a road writes to say an
    /// end never arrived — so it has to pass on an empty stream rather than erroring on one.
    #[test]
    fn an_absence_is_asked_for_the_same_way_and_holds_on_an_empty_stream() {
        let mut with = Args::new();
        with.insert("present".to_string(), serde_yaml::Value::Bool(false));
        let empty = json!([]);
        assert!(judge_activity(7, "decision.decided", &with, &empty).expect("the judge answers").pass);
    }
}
