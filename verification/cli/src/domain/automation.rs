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
