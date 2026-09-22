//! The `automation` domain: the picture agents are walked along, and a run of it.
//!
//! **Most of what is here is a road's premise.** A road about a run needs a definition to start, and
//! a definition is six kinds of row that mean nothing apart — so the build verbs are mapped whole and
//! a road takes as many as its own goal asks for. What each of them answers with is the id the next
//! one names, which is why nearly all of them bind.
//!
//! **What a step of a run types is not here** (`automation take` / `out` / `done`). Those are refused
//! outside the terminal a run opened for a step, and a run started at the terminal opens none — there
//! is no window to draw one in. The road for them waits on a door that opens a step without a screen.
//!
//! **A definition cannot be read back at this face.** The binary has no `automation show`: what reads
//! one whole is the build screen, through a door of the host's. So the asserts here are about runs and
//! about the search, and a build step is judged by the command not refusing it.

use amenbo_scenario::{Args, Domain};

use crate::judge::judge_found;
use crate::{opt_bool, req_str, unmapped, Driver, Outcome};

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
            "step-add" => {
                let automation = self.resolve(with)?;
                let name = req_str(with, "name")?;
                let mut args: Vec<String> =
                    vec!["automation".into(), "step".into(), "add".into(), automation.to_string()];
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
                    "exit".into(),
                    "add".into(),
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
                    "port".into(),
                    "add".into(),
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
                    vec!["automation".into(), "edge".into(), "add".into(), "--from".into(), from.clone()];
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
                    "wire".into(),
                    "add".into(),
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
                    "entry",
                    "set",
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
                    "cfg".into(),
                    "add".into(),
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
                    "cfg".into(),
                    "set".into(),
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
                    "note".into(),
                    "add".into(),
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
                    "note",
                    "link",
                    &step.to_string(),
                    &note.to_string(),
                    "--json",
                ])?;
                Ok(Outcome::action(format!("handed document {note} to step {step}")))
            }
            "action-add" => {
                let name = req_str(with, "name")?;
                let mut args: Vec<String> = vec!["automation".into(), "action".into(), "add".into()];
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
                    "action".into(),
                    "update".into(),
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
                let v = self.run_json(&["automation", "run", "show", &run.to_string(), "--json"])?;
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
                let v = self.run_json(&["automation", "run", "list", flag, &id.to_string(), "--json"])?;
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
            "found" => {
                let target = self.resolve(with)?;
                let words = self.query(with)?;
                let hits = self.search(with)?;
                judge_found("automation", &format!("AMB-AUT-{target}"), &words, with, &hits["hits"])
            }
            _ => Err(unmapped(Domain::Automation, op)),
        }
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
}
