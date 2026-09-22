//! The `automation` domain: the picture agents are walked along, and a run of it.
//!
//! **Most of what is here is a road's premise.** A road about a run needs a definition to start, and
//! a definition is six kinds of row that mean nothing apart — so the build verbs are mapped whole and
//! a road takes as many as its own goal asks for. What each of them answers with is the id the next
//! one names, which is why nearly all of them bind.
//!
//! **What a step of a run types is not here** (`automation step-take` / `step-out` / `step-done`). Those are refused
//! outside the terminal a run opened for a step, and a run started at the terminal opens none — there
//! is no window to draw one in. The road for them waits on a door that opens a step without a screen.
//! The same door is what the other side waits on: the build and drive verbs refuse *inside* a step
//! (`automation_outside_only`), and there is no step here to type them in.
//!
//! **A definition is read back here whole** (`automation show`), with the declarations each step runs
//! under already resolved — so a road that built one at the terminal proves it by reading it at the
//! terminal, rather than by the build commands not having refused it. What the picture draws is still
//! the screen's: boxes, lines and the marks along them have no terminal.

use amenbo_scenario::{Args, Domain};

use crate::judge::judge_found;
use crate::{opt_bool, req_i64, req_str, unmapped, Driver, Outcome};

impl Driver<'_> {
    pub(crate) fn automation_action(
        &mut self,
        op: &str,
        with: &Args,
        bind: Option<&str>,
    ) -> Result<Outcome, String> {
        match op {
            "create" => {
                let name = req_str(with, "name")?;
                let mut args: Vec<String> = vec!["automation".into(), "add".into()];
                if with.contains_key("project") {
                    args.push("--project".into());
                    args.push(self.resolve_key(with, "project")?.to_string());
                }
                args.push("--name".into());
                args.push(name.into());
                for key in ["notes", "preamble"] {
                    if let Some(v) = with.get(key).and_then(|v| v.as_str()) {
                        args.push(format!("--{key}"));
                        args.push(v.to_string());
                    }
                }
                args.push("--json".into());
                let id = self.bound_id(&args, "automation", bind)?;
                Ok(Outcome::action(format!("built automation {id} `{name}`")))
            }
            // The three fields the definition itself holds. Only what a step names is written, the
            // way the command reads it, so a road that renames one leaves its notes alone.
            "update" => {
                let automation = self.resolve(with)?;
                let mut args: Vec<String> =
                    vec!["automation".into(), "update".into(), automation.to_string()];
                for key in ["name", "notes"] {
                    if let Some(v) = with.get(key).and_then(|v| v.as_str()) {
                        args.push(format!("--{key}"));
                        args.push(v.to_string());
                    }
                }
                if let Some(archived) = opt_bool(with, "archived") {
                    args.push("--archived".into());
                    args.push(archived.to_string());
                }
                if args.len() == 3 {
                    return Err(
                        "`update` writes a name, notes or whether it is archived — a step naming \
                         none of the three would write nothing"
                            .to_string(),
                    );
                }
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("wrote {} on automation {automation}", written(with))))
            }
            // **Destructive, and confirmed by the command unless it is told otherwise** — so the
            // road says `--yes` rather than being left waiting at a prompt nothing will answer.
            "remove" => {
                let automation = self.resolve(with)?;
                self.run_json(&[
                    "automation",
                    "rm",
                    &automation.to_string(),
                    "--yes",
                    "--json",
                ])?;
                Ok(Outcome::action(format!(
                    "deleted automation {automation} with everything built into it"
                )))
            }
            "step-add" => {
                let automation = self.resolve(with)?;
                let name = req_str(with, "name")?;
                let mut args: Vec<String> =
                    vec!["automation".into(),
                    "step-add".into(), automation.to_string()];
                args.push("--name".into());
                args.push(name.into());
                args.push("--agent".into());
                args.push(req_str(with, "agent")?.into());
                // A step is made of its own prompt or of a library action, and the command takes one
                // of the two. Which it is, is what the road named.
                if with.contains_key("action") {
                    args.push("--action".into());
                    args.push(self.resolve_key(with, "action")?.to_string());
                } else {
                    args.push("--prompt".into());
                    args.push(req_str(with, "prompt")?.into());
                }
                for (key, flag) in [("model", "--model"), ("work_dir_ref", "--work-dir")] {
                    if let Some(v) = with.get(key).and_then(|v| v.as_str()) {
                        args.push(flag.into());
                        args.push(v.to_string());
                    }
                }
                args.push("--json".into());
                let id = self.bound_id(&args, "automation_step", bind)?;
                Ok(Outcome::action(format!("added step {id} `{name}` to automation {automation}")))
            }
            "exit-add" => {
                let (flag, owner, what) = self.declarer(with)?;
                let name = req_str(with, "name")?;
                let args = [
                    "automation".into(),
                    "exit-add".into(),
                    flag.into(),
                    owner.to_string(),
                    "--name".into(),
                    name.to_string(),
                    "--json".into(),
                ];
                let id = self.bound_id(&args, "automation_exit", bind)?;
                Ok(Outcome::action(format!("declared way out {id} `{name}` on {what} {owner}")))
            }
            "port-add" => {
                // What it hangs off says which direction it is: a way out hands on, while a step or
                // the action it runs is handed.
                let (owner_flag, owner, what) = match with.contains_key("step") || with.contains_key("action") {
                    true => self.declarer(with)?,
                    false => ("--exit", self.resolve(with)?, "way out"),
                };
                let name = req_str(with, "name")?;
                let kind = req_str(with, "kind")?;
                let mut args: Vec<String> = vec![
                    "automation".into(),
                    "port-add".into(),
                    owner_flag.into(),
                    owner.to_string(),
                    "--name".into(),
                    name.into(),
                    "--kind".into(),
                    kind.into(),
                ];
                if opt_bool(with, "required").unwrap_or(false) {
                    args.push("--required".into());
                }
                args.push("--json".into());
                let id = self.bound_id(&args, "automation_port", bind)?;
                Ok(Outcome::action(format!("declared {kind} `{name}` ({id}) on {what} {owner}")))
            }
            "edge-add" => {
                let from = self.way_out(with)?;
                let mut args: Vec<String> =
                    vec!["automation".into(),
                    "edge-add".into(), "--from".into(), from.clone()];
                let goes = match (with.contains_key("to"), opt_bool(with, "halt").unwrap_or(false)) {
                    (true, _) => {
                        args.push("--to".into());
                        let step = self.resolve_key(with, "to")?;
                        args.push(step.to_string());
                        format!("on to step {step}")
                    }
                    (false, true) => {
                        args.push("--halt".into());
                        "stopping the run for a person".to_string()
                    }
                    (false, false) => {
                        args.push("--done".into());
                        "closing the run".to_string()
                    }
                };
                if let Some(n) = with.get("max_times").and_then(serde_yaml::Value::as_i64) {
                    args.push("--max-times".into());
                    args.push(n.to_string());
                }
                args.push("--json".into());
                let id = self.bound_id(&args, "automation_edge", bind)?;
                Ok(Outcome::action(format!("after `{from}`, {goes} (edge {id})")))
            }
            "wire-add" => {
                let from = self.way_out(with)?;
                let to = self.resolve_key(with, "to")?;
                let from_port = req_str(with, "from_port")?;
                let to_port = req_str(with, "to_port")?;
                let args = [
                    "automation".into(),
                    "wire-add".into(),
                    "--from".into(),
                    from.clone(),
                    "--from-port".into(),
                    from_port.to_string(),
                    "--to".into(),
                    to.to_string(),
                    "--to-port".into(),
                    to_port.to_string(),
                    "--json".into(),
                ];
                let id = self.bound_id(&args, "automation_wire", bind)?;
                Ok(Outcome::action(format!(
                    "`{from}` hands `{from_port}` to step {to} as `{to_port}` (wire {id})"
                )))
            }
            "entry" => {
                let automation = self.resolve(with)?;
                let step = self.resolve_key(with, "step")?;
                self.run_json(&[
                    "automation",
                    "entry-set",
                    &automation.to_string(),
                    "--step",
                    &step.to_string(),
                    "--json",
                ])?;
                Ok(Outcome::action(format!("automation {automation} starts at step {step}")))
            }
            "cfg-add" => {
                let (flag, owner, what) = self.declarer(with)?;
                let name = req_str(with, "name")?;
                let kind = req_str(with, "kind")?;
                let mut args: Vec<String> = vec![
                    "automation".into(),
                    "cfg-add".into(),
                    flag.into(),
                    owner.to_string(),
                    "--name".into(),
                    name.into(),
                    "--kind".into(),
                    kind.into(),
                ];
                if opt_bool(with, "required").unwrap_or(false) {
                    args.push("--required".into());
                }
                if let Some(options) = with.get("options").and_then(|v| v.as_str()) {
                    args.push("--options".into());
                    args.push(options.to_string());
                }
                args.push("--json".into());
                let id = self.bound_id(&args, "automation_cfg", bind)?;
                Ok(Outcome::action(format!("declared setting {id} `{name}` ({kind}) on {what} {owner}")))
            }
            "cfg-set" => {
                let step = self.resolve(with)?;
                let name = req_str(with, "name")?;
                let mut args: Vec<String> = vec![
                    "automation".into(),
                    "cfg-set".into(),
                    step.to_string(),
                    "--name".into(),
                    name.into(),
                ];
                let said = match opt_bool(with, "clear").unwrap_or(false) {
                    true => {
                        args.push("--clear".into());
                        "left unanswered".to_string()
                    }
                    false => answer(with, &mut args)?,
                };
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("setting `{name}` on step {step} {said}")))
            }
            "note-add" => {
                let automation = self.resolve(with)?;
                let name = req_str(with, "name")?;
                let args = [
                    "automation".into(),
                    "note-add".into(),
                    automation.to_string(),
                    "--name".into(),
                    name.to_string(),
                    "--body".into(),
                    req_str(with, "body")?.to_string(),
                    "--json".into(),
                ];
                let id = self.bound_id(&args, "automation_note", bind)?;
                Ok(Outcome::action(format!("wrote shared document {id} `{name}` on automation {automation}")))
            }
            "note-link" => {
                let note = self.resolve(with)?;
                let step = self.resolve_key(with, "step")?;
                self.run_json(&[
                    "automation",
                    "note-link",
                    &step.to_string(),
                    &note.to_string(),
                    "--json",
                ])?;
                Ok(Outcome::action(format!("handed document {note} to step {step}")))
            }
            "action-add" => {
                let name = req_str(with, "name")?;
                let mut args: Vec<String> = vec!["automation".into(), "action-add".into()];
                if with.contains_key("project") {
                    args.push("--project".into());
                    args.push(self.resolve_key(with, "project")?.to_string());
                }
                args.push("--name".into());
                args.push(name.into());
                args.push("--prompt".into());
                args.push(req_str(with, "prompt")?.into());
                args.push("--json".into());
                let id = self.bound_id(&args, "automation_action", bind)?;
                Ok(Outcome::action(format!("put `{name}` ({id}) in the library")))
            }
            "action-update" => {
                let action = self.resolve(with)?;
                let args = [
                    "automation".into(),
                    "action-update".into(),
                    action.to_string(),
                    "--prompt".into(),
                    req_str(with, "prompt")?.to_string(),
                    "--json".into(),
                ];
                let id = self.bound_id(&args, "automation_action", bind)?;
                Ok(Outcome::action(format!("rewrote the prompt of library action {id}")))
            }
            "start" => {
                let automation = self.resolve(with)?;
                let args =
                    ["automation".into(), "start".into(), automation.to_string(), "--json".into()];
                let id = self.bound_id(&args, "automation_run", bind)?;
                Ok(Outcome::action(format!("started automation {automation} as run {id}")))
            }
            verb @ ("pause" | "resume" | "stop") => {
                let run = self.resolve(with)?;
                self.run_json(&["automation", verb, &run.to_string(), "--json"])?;
                Ok(Outcome::action(format!("{verb}d run {run}")))
            }
            _ => Err(unmapped(Domain::Automation, op)),
        }
    }

    pub(crate) fn automation_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "run" => {
                let run = self.resolve(with)?;
                let want = req_str(with, "status")?;
                let v = self.run_json(&["automation",
                    "run-show", &run.to_string(), "--json"])?;
                let row = &v["run"];
                let status = row["status"].as_str().unwrap_or("(none reported)");
                let mut pass = status == want;
                let mut said = format!("run {run} is `{status}` (expected `{want}`");
                // Which of the four stops it was, asked only where a road named one: a run that is
                // running carries none, and a step asking for one there is asking about nothing.
                if let Some(why) = with.get("stopped_reason").and_then(|v| v.as_str()) {
                    let got = row["stopped_reason"].as_str().unwrap_or("(none reported)");
                    pass = pass && got == why;
                    said.push_str(&format!(", stopped because `{got}`, expected `{why}`"));
                }
                said.push_str(if pass { ", as expected)" } else { ", MISMATCH)" });
                Ok(Outcome::assert(pass, said))
            }
            "runs-listed" => {
                let run = self.resolve(with)?;
                let present = opt_bool(with, "present").unwrap_or(true);
                // The two doors a run is reached by. There is no listing of every run, so a step here
                // names which of the two it is walking.
                let (flag, id, door) = match with.contains_key("task") {
                    true => ("--task", self.resolve_key(with, "task")?, "the task it worked"),
                    false => (
                        "--automation",
                        self.resolve_key(with, "automation")?,
                        "the automation it came from",
                    ),
                };
                let v = self.run_json(&["automation",
                    "run-list", flag, &id.to_string(), "--json"])?;
                let rows = v["runs"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                // Each row is the run and how many moves it made, so the run is a step inside the row rather
                // than the row itself.
                let found = rows.iter().any(|one| one["run"]["id"].as_i64() == Some(run));
                let pass = found == present;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "run {run} {} the runs reached from {door} {id} ({} listed, expected {}, {})",
                        if found { "is among" } else { "is not among" },
                        rows.len(),
                        if present { "listed" } else { "left out" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // The two listings, which the automations screen draws as rows and the terminal prints as
            // lines. `steps` is the second half of what a row is read for — what this is, and whether
            // it is built yet.
            "listed" => {
                let target = self.resolve(with)?;
                let present = opt_bool(with, "present").unwrap_or(true);
                let v = self.run_json(&["automation", "list", "--json"])?;
                let rows = v["automations"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let row = rows.iter().find(|one| one["automation"]["id"].as_i64() == Some(target));
                let mut pass = row.is_some() == present;
                let mut said = format!(
                    "automation {target} {} the {} listed (expected {}",
                    if row.is_some() { "is among" } else { "is not among" },
                    rows.len(),
                    if present { "listed" } else { "left out" },
                );
                if with.contains_key("steps") {
                    let want = req_i64(with, "steps")?;
                    let got = row.and_then(|one| one["steps"].as_i64());
                    pass = pass && got == Some(want);
                    said.push_str(&format!(
                        ", built out of {} steps, expected {want}",
                        match got {
                            Some(n) => n.to_string(),
                            None => "no".to_string(),
                        }
                    ));
                }
                if let Some(want) = with.get("name").and_then(|v| v.as_str()) {
                    let got = row.and_then(|one| one["automation"]["name"].as_str());
                    pass = pass && got == Some(want);
                    said.push_str(&format!(
                        ", called {}, expected `{want}`",
                        match got {
                            Some(name) => format!("`{name}`"),
                            None => "nothing".to_string(),
                        }
                    ));
                }
                if let Some(want) = opt_bool(with, "archived") {
                    let got = row.and_then(|one| one["automation"]["archived"].as_bool());
                    pass = pass && got == Some(want);
                    said.push_str(&format!(
                        ", {} archived, expected {}",
                        match got {
                            Some(true) => "is",
                            Some(false) => "is not",
                            None => "says nothing about being",
                        },
                        match want {
                            true => "archived",
                            false => "not archived",
                        }
                    ));
                }
                said.push_str(if pass { ", as expected)" } else { ", MISMATCH)" });
                Ok(Outcome::assert(pass, said))
            }
            // A library action's row. `used_by` counts automations rather than steps — what it is read
            // for is how far a rewrite of the prompt carries — and `reach` says which of the two
            // shelves it sits on, which is the column the listing carries rather than a second list.
            "action-listed" => {
                let target = self.resolve(with)?;
                let v = self.run_json(&["automation",
                    "action-list", "--json"])?;
                let rows = v["actions"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let Some(row) = rows.iter().find(|one| one["action"]["id"].as_i64() == Some(target))
                else {
                    return Ok(Outcome::assert(
                        false,
                        format!(
                            "library action {target} is not among the {} the library lists (MISMATCH)",
                            rows.len()
                        ),
                    ));
                };
                let mut pass = true;
                let mut said = format!("library action {target} is listed");
                if with.contains_key("used_by") {
                    let want = req_i64(with, "used_by")?;
                    let got = row["used_by"].as_i64();
                    pass = pass && got == Some(want);
                    said.push_str(&format!(
                        ", used by {} automations (expected {want})",
                        match got {
                            Some(n) => n.to_string(),
                            None => "(none reported)".to_string(),
                        }
                    ));
                }
                if let Some(want) = with.get("reach").and_then(|v| v.as_str()) {
                    if !matches!(want, "device" | "project") {
                        return Err(format!(
                            "`reach` does not know `{want}` — it is device / project"
                        ));
                    }
                    // Which shelf a row is from is the project it hangs off: the device's library is
                    // nobody's project, which is what every project on this machine reaching it means.
                    let got = match row["action"]["project_id"].is_null() {
                        true => "device",
                        false => "project",
                    };
                    pass = pass && got == want;
                    said.push_str(&format!(", on the {got} library (expected {want})"));
                }
                said.push_str(if pass { ", as expected" } else { ", MISMATCH" });
                Ok(Outcome::assert(pass, said))
            }
            // One step of a definition, read back off `automation show` with the library already read
            // in. What a road may ask for is the prompt it runs on, where that prompt came from, and
            // the three families it declares — and the three are asked for whole, so a build that grew
            // a way out nobody declared is a mismatch rather than something nobody looked at.
            "step-read" => {
                let automation = self.resolve(with)?;
                judge_step(automation, &self.definition(automation)?, with)
            }
            // Where leaving one way out takes the run. The pair an edge hangs on is the step and the
            // name it carries, which is the pair `edge-add` writes it under.
            "edge-read" => {
                let automation = self.resolve(with)?;
                judge_edge(automation, &self.definition(automation)?, with)
            }
            // A document the steps share, and which of them were handed it. The second is asked for
            // whole: a document nobody was handed and one everybody was are two definitions, and a
            // road naming one step would read the same against both.
            "note-read" => {
                let automation = self.resolve(with)?;
                judge_note(automation, &self.definition(automation)?, with)
            }
            "found" => {
                let target = self.resolve(with)?;
                let words = self.query(with)?;
                let hits = self.search(with)?;
                judge_found("automation", &format!("AMB-AUT-{target}"), &words, with, &hits["hits"])
            }
            _ => Err(unmapped(Domain::Automation, op)),
        }
    }

    /// One automation's definition, resolved — the one read every `*-read` assert stands on.
    fn definition(&self, automation: i64) -> Result<serde_json::Value, String> {
        self.run_json(&["automation", "show", &automation.to_string(), "--json"])
    }

    /// **Which of the two declares it** — the step, or the library action the step runs.
    ///
    /// A step made of an action declares nothing of its own: its ways out, its settings and its
    /// inputs are the action's, so that two automations running the same action are running the same
    /// thing. The binary refuses the other spelling by name, and a road says which it means by naming
    /// `step:` or `action:`.
    fn declarer(&self, with: &Args) -> Result<(&'static str, i64, &'static str), String> {
        match with.contains_key("action") {
            true => Ok(("--action", self.resolve_key(with, "action")?, "library action")),
            false => Ok(("--step", self.resolve_key(with, "step")?, "step")),
        }
    }

    /// The way out an edge or a wire leaves by, written the one way the command takes it:
    /// `<step>:<way out>`, where the bare `<step>:` is the unnamed one and `<step>:*` the error one.
    ///
    /// It is built here rather than in each caller because both of them name the same pair, and a
    /// road that spelled it itself would be writing an id no scenario can know.
    fn way_out(&self, with: &Args) -> Result<String, String> {
        let step = self.resolve(with)?;
        let exit = with.get("exit").and_then(|v| v.as_str()).unwrap_or("");
        Ok(format!("{step}:{exit}"))
    }
}

/// A setting's answer, put into the flags the command takes it behind, and said in a line.
///
/// **A task filter is never one string.** What names it is the parts (`assignee`, `status`, `ready`,
/// and the rest), each carrying what is any-of on that part — the same reading `--filter`'s
/// expression gives, and the same shape the screen's rows take. So a road writes the parts and this
/// spells them out one flag at a time, which is also what lets the command refuse a value nothing
/// accepts while the person who wrote it is still here.
///
/// The other four kinds take one answer apiece, and a road naming more than one of them is naming
/// two answers for one setting — which the command refuses, and which this lets it.
fn answer(with: &Args, args: &mut Vec<String>) -> Result<String, String> {
    let mut said: Vec<String> = Vec::new();
    for key in ["folder", "choice", "text", "number"] {
        let Some(value) = with.get(key) else { continue };
        let value = match key {
            "number" => value
                .as_i64()
                .map(|n| n.to_string())
                .ok_or_else(|| "`number` takes a number".to_string())?,
            _ => value
                .as_str()
                .ok_or_else(|| format!("`{key}` takes a string"))?
                .to_string(),
        };
        args.push(format!("--{key}"));
        args.push(value.clone());
        said.push(format!("{key} `{value}`"));
    }
    for key in ["status", "priority", "assignee", "dim", "ready", "done", "due"] {
        let Some(value) = with.get(key) else { continue };
        // One value or several, written either way round: a part answered with one thing is the
        // ordinary case and a road should not have to wrap it in a list to say so.
        let values: Vec<String> = match value {
            serde_yaml::Value::Sequence(seq) => seq
                .iter()
                .map(|v| {
                    v.as_str().map(str::to_string).ok_or_else(|| {
                        format!("`{key}` takes the words a filter is written in")
                    })
                })
                .collect::<Result<_, _>>()?,
            other => vec![other
                .as_str()
                .ok_or_else(|| format!("`{key}` takes the words a filter is written in"))?
                .to_string()],
        };
        for one in &values {
            args.push(format!("--{key}"));
            args.push(one.clone());
        }
        said.push(format!("{key} `{}`", values.join(",")));
    }
    if said.is_empty() {
        return Err(
            "a setting is answered in the shape its kind takes, or left unanswered with `clear`"
                .to_string(),
        );
    }
    Ok(format!("answered {}", said.join(", ")))
}

/// One step of a definition, judged against what a road said it was built as.
///
/// **The three families are read whole.** `exits`, `inputs` and `settings` each name all of what the
/// step declares rather than a sample of it: a build that grew a way out nobody wrote would pass
/// every question asked one at a time, and the whole point of reading a definition back is that what
/// is there is what was built.
fn judge_step(automation: i64, view: &serde_json::Value, with: &Args) -> Result<Outcome, String> {
    let name = req_str(with, "name")?;
    let present = opt_bool(with, "present").unwrap_or(true);
    let Some(step) = step_named(view, name) else {
        return Ok(Outcome::assert(
            !present,
            format!(
                "automation {automation} has no step `{name}` (expected {}, {})",
                if present { "one" } else { "none" },
                if present { "MISMATCH" } else { "as expected" }
            ),
        ));
    };
    if !present {
        return Ok(Outcome::assert(
            false,
            format!("automation {automation} still has a step `{name}` (MISMATCH)"),
        ));
    }
    let mut pass = true;
    let mut said = format!("step `{name}` of automation {automation} is defined");
    if let Some(want) = with.get("prompt").and_then(|v| v.as_str()) {
        let got = step["prompt"].as_str().unwrap_or("(none reported)");
        pass = pass && got == want;
        said.push_str(&format!(", running on `{got}` (expected `{want}`)"));
    }
    // The half the picture cannot say. It draws that a prompt came from the library; the terminal
    // names which action it was read off.
    if let Some(want) = with.get("from_action").and_then(|v| v.as_str()) {
        let got = step["action"]["name"].as_str().unwrap_or("(its own prompt)");
        pass = pass && got == want;
        said.push_str(&format!(", read off library action `{got}` (expected `{want}`)"));
    }
    if with.contains_key("exits") {
        let want = word_list(with, "exits")?;
        // A way out is named the way an edge names one: the unnamed one it is born with is the empty
        // name, and the error one is `*`.
        let got: Vec<String> = rows_of(step, "exits")
            .iter()
            .map(|one| one["exit"]["name"].as_str().unwrap_or("").to_string())
            .collect();
        pass = pass && got == want;
        said.push_str(&format!(", leaving by {got:?} (expected {want:?})"));
    }
    if with.contains_key("inputs") {
        let want = word_list(with, "inputs")?;
        let got: Vec<String> = rows_of(step, "inputs")
            .iter()
            .map(|one| one["name"].as_str().unwrap_or("").to_string())
            .collect();
        pass = pass && got == want;
        said.push_str(&format!(", taking in {got:?} (expected {want:?})"));
    }
    if with.contains_key("settings") {
        let (ok, note) = judge_settings(with, rows_of(step, "settings"))?;
        pass = pass && ok;
        said.push_str(&note);
    }
    said.push_str(if pass { ", as expected" } else { ", MISMATCH" });
    Ok(Outcome::assert(pass, said))
}

/// Where leaving one way out takes the run — on to a step (`to`), or to an end of its own (`ends`).
fn judge_edge(automation: i64, view: &serde_json::Value, with: &Args) -> Result<Outcome, String> {
    let from = req_str(with, "from")?;
    let exit = with.get("exit").and_then(|v| v.as_str()).unwrap_or("");
    let named = match exit {
        "" => "the unnamed way out".to_string(),
        other => format!("`{other}`"),
    };
    let Some(from_step) = step_named(view, from) else {
        return Ok(Outcome::assert(
            false,
            format!("automation {automation} has no step `{from}` to leave (MISMATCH)"),
        ));
    };
    let from_id = from_step["step"]["id"].as_i64();
    let Some(edge) = rows_of(view, "edges").iter().find(|one| {
        one["from_step_id"].as_i64() == from_id && one["exit_name"].as_str().unwrap_or("") == exit
    }) else {
        return Ok(Outcome::assert(
            false,
            format!("nothing happens after {named} of step `{from}` (MISMATCH)"),
        ));
    };
    let ends = edge["ends"].as_str().unwrap_or("(none reported)");
    let pass;
    let mut said = format!("after {named} of step `{from}`, the run ");
    match (with.get("to").and_then(|v| v.as_str()), with.get("ends").and_then(|v| v.as_str())) {
        (Some(_), Some(_)) => {
            return Err(
                "`to` names the step an edge goes on to and `ends` says it goes nowhere — name one of them"
                    .to_string(),
            )
        }
        (Some(want), None) => {
            let got = step_name_of(view, edge["to_step_id"].as_i64());
            pass = ends == "go" && got.as_deref() == Some(want);
            said.push_str(&format!(
                "goes to `{}` (`{ends}`, expected step `{want}`)",
                got.unwrap_or_else(|| "nowhere".to_string())
            ));
        }
        (None, Some(want)) => {
            pass = ends == want;
            said.push_str(&format!("ends `{ends}` (expected `{want}`)"));
        }
        (None, None) => {
            pass = true;
            said.push_str(&format!("ends `{ends}`"));
        }
    }
    said.push_str(if pass { ", as expected" } else { ", MISMATCH" });
    Ok(Outcome::assert(pass, said))
}

/// A document the steps share, and — read whole — which of them were handed it.
fn judge_note(automation: i64, view: &serde_json::Value, with: &Args) -> Result<Outcome, String> {
    let name = req_str(with, "name")?;
    let Some(note) =
        rows_of(view, "notes").iter().find(|one| one["note"]["name"].as_str() == Some(name))
    else {
        return Ok(Outcome::assert(
            false,
            format!("automation {automation} shares no document `{name}` (MISMATCH)"),
        ));
    };
    let mut pass = true;
    let mut said = format!("document `{name}` of automation {automation} is shared");
    if let Some(want) = with.get("body").and_then(|v| v.as_str()) {
        let got = note["note"]["body"].as_str().unwrap_or("(none reported)");
        pass = pass && got == want;
        said.push_str(&format!(", reading `{got}` (expected `{want}`)"));
    }
    if with.contains_key("handed_to") {
        let want = word_list(with, "handed_to")?;
        let got: Vec<String> = rows_of(note, "step_ids")
            .iter()
            .map(|one| {
                step_name_of(view, one.as_i64())
                    .unwrap_or_else(|| "(a step that is gone)".to_string())
            })
            .collect();
        pass = pass && got == want;
        said.push_str(&format!(", handed to {got:?} (expected {want:?})"));
    }
    said.push_str(if pass { ", as expected" } else { ", MISMATCH" });
    Ok(Outcome::assert(pass, said))
}

/// The rows under one key of a read, or none where the key carries no list. A key the output does not
/// have comes back empty rather than erroring, for the reason a `field` path that runs off one does:
/// what a road is asserting about is the shape of the shipped output as much as what is in it, and an
/// empty list reaches the comparison the road wrote.
fn rows_of<'a>(shown: &'a serde_json::Value, key: &str) -> &'a [serde_json::Value] {
    shown[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// One step of a definition, found by the name it was built under.
fn step_named<'a>(view: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    rows_of(view, "steps").iter().find(|one| one["step"]["name"].as_str() == Some(name))
}

/// The name a step id carries in this definition — what turns the ids an edge and a document link
/// hold back into the words a road wrote.
fn step_name_of(view: &serde_json::Value, id: Option<i64>) -> Option<String> {
    let id = id?;
    rows_of(view, "steps")
        .iter()
        .find(|one| one["step"]["id"].as_i64() == Some(id))
        .and_then(|one| one["step"]["name"].as_str())
        .map(str::to_string)
}

/// A list of names as a road writes one. Written as anything else it is refused here rather than read
/// as one name: a road that wrote a single word where a list belongs is asserting about a list of one,
/// and passing that quietly would be the assert answering a question nobody asked.
fn word_list(with: &Args, key: &str) -> Result<Vec<String>, String> {
    match with.get(key) {
        Some(serde_yaml::Value::Sequence(seq)) => seq
            .iter()
            .map(|one| {
                one.as_str().map(str::to_string).ok_or_else(|| format!("`{key}` is a list of names"))
            })
            .collect(),
        _ => Err(format!("`{key}` is a list of names")),
    }
}

/// The settings one step runs under, judged against the name → answer mapping a road wrote.
///
/// **Both halves are asked.** The names have to be the whole of what the step declares, so a
/// declaration nobody wrote is a mismatch rather than something the road never looked at; and each
/// answer is compared as the value it is, the stored JSON read back first. A road writes `~` for a
/// setting nobody has answered, which is a state of its own and not an empty answer.
fn judge_settings(with: &Args, rows: &[serde_json::Value]) -> Result<(bool, String), String> {
    let Some(serde_yaml::Value::Mapping(want)) = with.get("settings") else {
        return Err("`settings` is a mapping of each setting's name to its answer".to_string());
    };
    let mut wanted: Vec<(String, serde_json::Value)> = Vec::new();
    for (name, answer) in want {
        let name = name
            .as_str()
            .ok_or("`settings` names each setting by the name it was declared under")?;
        let answer = serde_json::to_value(answer)
            .map_err(|e| format!("the answer written for `{name}` is not a value: {e}"))?;
        wanted.push((name.to_string(), answer));
    }
    let declared: Vec<String> =
        rows.iter().map(|one| one["name"].as_str().unwrap_or("").to_string()).collect();
    let mut asked: Vec<String> = wanted.iter().map(|(name, _)| name.clone()).collect();
    let (mut sorted, mut pass) = (declared.clone(), true);
    sorted.sort();
    asked.sort();
    if sorted != asked {
        return Ok((false, format!(", set by {declared:?} (expected {asked:?}), MISMATCH")));
    }
    let mut said = String::new();
    for (name, answer) in &wanted {
        let row = rows.iter().find(|one| one["name"].as_str() == Some(name));
        // The answer is kept as JSON text, so it is read back before it is compared: a road writes
        // the number it answered with, never the spelling the column holds it in.
        let got = match row.and_then(|one| one["value"].as_str()) {
            Some(value) => serde_json::from_str(value).unwrap_or(serde_json::Value::Null),
            None => serde_json::Value::Null,
        };
        pass = pass && got == *answer;
        said.push_str(&format!(", `{name}` answered {got} (expected {answer})"));
    }
    Ok((pass, said))
}

/// Which of the three fields an `update` step named, in the words the report says it back in. The
/// driver has already refused a step that named none, so this never answers empty.
fn written(with: &Args) -> String {
    let mut said: Vec<String> = Vec::new();
    if with.get("name").is_some() {
        said.push("a new name".to_string());
    }
    if with.get("notes").is_some() {
        said.push("new notes".to_string());
    }
    if let Some(archived) = opt_bool(with, "archived") {
        said.push(match archived {
            true => "that it is archived".to_string(),
            false => "that it is not archived".to_string(),
        });
    }
    said.join(" and ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(yaml: &str) -> Args {
        serde_yaml::from_str(yaml).expect("the road's own words")
    }

    /// A task filter is the one answer that is not a single value, and the shape it goes in as is
    /// the parts that name it — each carrying what is any-of on that part. What this guards is that
    /// a part written as one thing and a part written as a list both reach the command, since a road
    /// answering one value should not have to wrap it in a list to say so.
    #[test]
    fn a_task_filter_is_answered_a_part_at_a_time() {
        let mut args: Vec<String> = Vec::new();
        let said = answer(&with("{ assignee: me-ai, status: [todo, in_progress] }"), &mut args)
            .expect("an answer");
        assert_eq!(
            args,
            [
                "--status", "todo", "--status", "in_progress", "--assignee", "me-ai",
            ]
            .map(String::from)
            .to_vec(),
            "each part is written out one flag at a time, the command's own way round",
        );
        assert!(said.contains("todo,in_progress"), "{said}");
    }

    /// The four kinds that take one answer apiece, each behind its own flag.
    #[test]
    fn the_other_kinds_take_one_answer_apiece() {
        let mut args: Vec<String> = Vec::new();
        answer(&with("{ number: 3 }"), &mut args).expect("an answer");
        assert_eq!(args, ["--number", "3"].map(String::from).to_vec());

        let mut args: Vec<String> = Vec::new();
        answer(&with("{ text: SCENARIO }"), &mut args).expect("an answer");
        assert_eq!(args, ["--text", "SCENARIO"].map(String::from).to_vec());
    }

    /// An answer nobody wrote is not an empty answer: a setting left unanswered is said with
    /// `clear`, and a step that named neither is a step with nothing to send.
    #[test]
    fn a_setting_with_no_answer_at_all_is_refused_here() {
        let mut args: Vec<String> = Vec::new();
        assert!(answer(&with("{}"), &mut args).is_err());
    }

    /// One definition as `automation show --json` prints it, cut down to what the read-back asserts
    /// look at: two steps, one carrying its own prompt and one made of a library action, the edges
    /// between them and the document they share.
    fn definition() -> serde_json::Value {
        serde_json::json!({
            "automation": { "id": 1, "name": "one task, three steps" },
            "steps": [
                {
                    "step": { "id": 1, "name": "take", "prompt": "take the next task" },
                    "action": null,
                    "prompt": "take the next task",
                    "exits": [
                        { "exit": { "id": 3, "name": null }, "outputs": [] },
                        { "exit": { "id": 4, "name": "*" }, "outputs": [] },
                        { "exit": { "id": 5, "name": "got one" }, "outputs": [] },
                    ],
                    "inputs": [{ "id": 2, "name": "brief", "kind": "value", "required": true }],
                    "settings": [
                        { "id": 1, "name": "how many", "kind": "number", "value": "3" },
                        { "id": 2, "name": "where to work", "kind": "folder", "value": null },
                    ],
                },
                {
                    "step": { "id": 2, "name": "review", "prompt": null },
                    "action": { "id": 1, "name": "read it back", "prompt": "read what was written" },
                    "prompt": "read what was written",
                    "exits": [
                        { "exit": { "id": 1, "name": null }, "outputs": [] },
                        { "exit": { "id": 2, "name": "*" }, "outputs": [] },
                    ],
                    "inputs": [],
                    "settings": [],
                },
            ],
            "edges": [
                { "id": 1, "from_step_id": 1, "exit_name": "got one", "to_step_id": 2, "ends": "go" },
                { "id": 2, "from_step_id": 2, "exit_name": null, "to_step_id": null, "ends": "done" },
            ],
            "wires": [],
            "notes": [
                { "note": { "id": 1, "name": "house style", "body": "written this way" }, "step_ids": [1] },
            ],
        })
    }

    /// A step read back as it was built: the prompt it runs on, the ways out it can leave by — the two
    /// it is born with and the one that was declared — what it takes in, and its settings with the
    /// answer written for one of them.
    #[test]
    fn a_step_is_read_back_as_it_was_built() {
        let read = judge_step(
            1,
            &definition(),
            &with(
                r#"{ name: take, prompt: take the next task, exits: ["", "*", got one],
                     inputs: [brief], settings: { how many: 3, where to work: ~ } }"#,
            ),
        )
        .expect("a verdict");
        assert!(read.pass, "{}", read.note);
    }

    /// And the step made of a library action reads its prompt and its ways out off the action, which
    /// is the half the picture cannot say: it draws that a prompt came from the library, never which
    /// action it came from.
    #[test]
    fn a_step_made_of_a_library_action_names_the_action_it_reads() {
        let read = judge_step(
            1,
            &definition(),
            &with(
                r#"{ name: review, prompt: read what was written, from_action: read it back,
                     exits: ["", "*"], inputs: [], settings: {} }"#,
            ),
        )
        .expect("a verdict");
        assert!(read.pass, "{}", read.note);
    }

    /// **The three families are read whole**, which is what a family read one name at a time would
    /// miss: a way out nobody wrote, an input nobody declared, a setting that appeared from somewhere.
    #[test]
    fn a_family_read_whole_catches_one_nobody_wrote() {
        for asked in [
            r#"{ name: take, exits: ["", "*"] }"#,
            r#"{ name: take, inputs: [] }"#,
            r#"{ name: take, settings: { how many: 3 } }"#,
        ] {
            let read = judge_step(1, &definition(), &with(asked)).expect("a verdict");
            assert!(!read.pass, "asking `{asked}` should have caught the rest: {}", read.note);
        }
    }

    /// A setting nobody answered is a state of its own rather than an empty answer, and a road says so
    /// by writing nothing for it. The answer that was written is compared as the number it is, not as
    /// the JSON the column holds it in.
    #[test]
    fn an_unanswered_setting_is_not_an_empty_answer() {
        let wrong = judge_step(
            1,
            &definition(),
            &with(r#"{ name: take, settings: { how many: 3, where to work: "" } }"#),
        )
        .expect("a verdict");
        assert!(!wrong.pass, "an empty answer is not the same as none: {}", wrong.note);

        let also_wrong = judge_step(
            1,
            &definition(),
            &with(r#"{ name: take, settings: { how many: "3", where to work: ~ } }"#),
        )
        .expect("a verdict");
        assert!(!also_wrong.pass, "the number was answered as a number: {}", also_wrong.note);
    }

    /// A step nobody built. Every read above would pass against a build that answered with whatever
    /// name it was handed, and this is the one that says they were answers.
    #[test]
    fn a_step_nobody_built_is_read_back_as_absent() {
        let absent =
            judge_step(1, &definition(), &with("{ name: publish, present: false }")).expect("a verdict");
        assert!(absent.pass, "{}", absent.note);

        let claimed =
            judge_step(1, &definition(), &with("{ name: take, present: false }")).expect("a verdict");
        assert!(!claimed.pass, "a step that is there is not absent: {}", claimed.note);
    }

    /// Where leaving one way out takes the run: on to a step by the name written along it, or to an
    /// end of the run's own.
    #[test]
    fn a_way_out_is_read_back_with_where_it_goes() {
        for asked in ["{ from: take, exit: got one, to: review }", "{ from: review, ends: done }"] {
            let read = judge_edge(1, &definition(), &with(asked)).expect("a verdict");
            assert!(read.pass, "{}", read.note);
        }

        // The way out is half of what an edge hangs on, so the unnamed one of a step whose edge leaves
        // by a named one is nothing at all.
        let elsewhere =
            judge_edge(1, &definition(), &with("{ from: take, to: review }")).expect("a verdict");
        assert!(!elsewhere.pass, "{}", elsewhere.note);
    }

    /// A document, and the steps it was handed to read as one list: handed to nobody and handed to
    /// everybody are two different definitions.
    #[test]
    fn a_shared_document_is_read_back_with_who_holds_it() {
        let read = judge_note(
            1,
            &definition(),
            &with("{ name: house style, body: written this way, handed_to: [take] }"),
        )
        .expect("a verdict");
        assert!(read.pass, "{}", read.note);

        let everybody = judge_note(
            1,
            &definition(),
            &with("{ name: house style, handed_to: [take, review] }"),
        )
        .expect("a verdict");
        assert!(!everybody.pass, "{}", everybody.note);
    }
}
