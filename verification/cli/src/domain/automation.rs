//! The `automation` domain: the picture agents are walked along, and a run of it.
//!
//! **Most of what is here is a road's premise.** A road about a run needs a definition to start, and
//! a definition is three layers of rows that mean nothing apart: the automation places
//! library actions, an action holds steps, and a step is one terminal. So the build verbs are mapped
//! whole and a road takes as many as its own goal asks for. What each of them answers with is the id
//! the next one names, which is why nearly all of them bind.
//!
//! **What a step of a run types is not here** (`automation step-take` / `step-out` / `step-done`). Those are refused
//! outside the terminal a run opened for a step, and a run started at the terminal opens none — there
//! is no window to draw one in. The road for them waits on a door that opens a step without a screen.
//! The same door is what the other side waits on: the build and drive verbs refuse *inside* a step
//! (`automation_outside_only`), and there is no step here to type them in.
//!
//! **A definition is read back here in the two layers it is built in** — the placements on an
//! automation (`automation show`) and the steps inside an action (`automation action-show`) — so a
//! road that built one at the terminal proves it by reading it at the terminal, rather than by the
//! build commands not having refused it. What the picture draws is still the screen's: boxes, lines
//! and the marks along them have no terminal.

use amenbo_scenario::{Args, Domain};

use crate::{opt_bool, req_i64, req_str, unmapped, Driver, Outcome};

/// The name of the way out every step and every action is born with — core's `DONE_EXIT`, spelled
/// here because this crate reads the product from outside. A road that leaves the way out unsaid means
/// this one, as `4:` does on the command line.
const DONE_EXIT: &str = "完了";

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
                for key in ["notes"] {
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
            // **The library.** An action is born empty; what it holds is written with `step-add`.
            "action-add" => {
                let name = req_str(with, "name")?;
                let mut args: Vec<String> = vec!["automation".into(), "action-add".into()];
                if with.contains_key("project") {
                    args.push("--project".into());
                    args.push(self.resolve_key(with, "project")?.to_string());
                }
                args.push("--name".into());
                args.push(name.into());
                args.push("--json".into());
                let id = self.bound_id(&args, "automation_action", bind)?;
                Ok(Outcome::action(format!("put `{name}` ({id}) in the library")))
            }
            // Only what a step names is written, the way the command reads it.
            "action-update" => {
                let action = self.resolve(with)?;
                let mut args: Vec<String> =
                    vec!["automation".into(), "action-update".into(), action.to_string()];
                for key in ["name", "note"] {
                    if let Some(v) = with.get(key).and_then(|v| v.as_str()) {
                        args.push(format!("--{key}"));
                        args.push(v.to_string());
                    }
                }
                if args.len() == 3 {
                    return Err(
                        "`action-update` writes a name or a note — a step naming neither would write nothing"
                            .to_string(),
                    );
                }
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("rewrote library action {action}")))
            }
            // Into the other library, by the command a person types for it. Refused into a project
            // another project's automation still places it in — the step that says so carries
            // `refused: invalid`, and what the refusal names is `scope-refusal-names`'.
            "action-scope" => {
                let action = self.resolve(with)?;
                let args = self.scope_args(action, with)?;
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!(
                    "moved library action {action} to the {} library",
                    req_str(with, "reach")?
                )))
            }
            // A step inside an action, carrying its own prompt — the one layer that is a terminal.
            "step-add" => {
                let action = self.resolve(with)?;
                let name = req_str(with, "name")?;
                let mut args: Vec<String> = vec![
                    "automation".into(),
                    "step-add".into(),
                    action.to_string(),
                    "--name".into(),
                    name.into(),
                    "--prompt".into(),
                    req_str(with, "prompt")?.into(),
                ];
                if let Some(v) = with.get("work_dir_ref").and_then(|v| v.as_str()) {
                    args.push("--work-dir".into());
                    args.push(v.to_string());
                }
                // Each is handed on unless a flag says not to, so a road's `true` is the flag left off.
                for (key, flag) in [
                    ("task_notes", "--no-task-notes"),
                    ("task_decisions", "--no-task-decisions"),
                    ("task_comments", "--no-task-comments"),
                    ("history", "--no-history"),
                ] {
                    if opt_bool(with, key) == Some(false) {
                        args.push(flag.into());
                    }
                }
                args.push("--json".into());
                let id = self.bound_id(&args, "automation_step", bind)?;
                Ok(Outcome::action(format!("added step {id} `{name}` to library action {action}")))
            }
            // Turning what a step is handed back on or off, the rest of the step left as it is.
            "step-update" => {
                let step = self.resolve(with)?;
                let mut args: Vec<String> = vec!["automation".into(), "step-update".into(), step.to_string()];
                let mut said = Vec::new();
                for (key, flag, what) in [
                    ("task_notes", "--task-notes", "the task's notes"),
                    ("task_decisions", "--task-decisions", "the decisions linked to the task"),
                    ("task_comments", "--task-comments", "the comments on the task"),
                    ("history", "--history", "the run's story so far"),
                ] {
                    if let Some(on) = opt_bool(with, key) {
                        args.push(flag.into());
                        args.push(on.to_string());
                        said.push(format!("{} {what}", if on { "handed" } else { "not handed" }));
                    }
                }
                if said.is_empty() {
                    return Err(
                        "`step-update` turns `task_notes`, `task_decisions`, `task_comments` or `history` — a step naming none of them would write nothing"
                            .to_string(),
                    );
                }
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("step {step} is now {}", said.join(" and "))))
            }
            "action-entry" => {
                let action = self.resolve(with)?;
                let step = self.resolve_key(with, "step")?;
                self.run_json(&[
                    "automation",
                    "action-entry-set",
                    &action.to_string(),
                    "--step",
                    &step.to_string(),
                    "--json",
                ])?;
                Ok(Outcome::action(format!("a placement of library action {action} opens step {step} first")))
            }
            // **The picture.** What stands on it is a placement of an action, never a prompt — a
            // library action, or one of Amenbo's built-ins named by its key.
            "place-add" => {
                let automation = self.resolve(with)?;
                let (flag, placed, what) = match with.get("builtin").and_then(|v| v.as_str()) {
                    Some(key) => ("--builtin", key.to_string(), "built-in"),
                    None => ("--action", self.resolve_key(with, "action")?.to_string(), "library action"),
                };
                let args = [
                    "automation".into(),
                    "place-add".into(),
                    automation.to_string(),
                    flag.into(),
                    placed.clone(),
                    "--json".into(),
                ];
                let id = self.bound_id(&args, "automation_placement", bind)?;
                Ok(Outcome::action(format!(
                    "placed {what} {placed} on automation {automation} (placement {id})"
                )))
            }
            // Who carries one step out at one placement.
            "agent-set" => {
                let placement = self.resolve(with)?;
                let step = self.resolve_key(with, "step")?;
                let agent = req_str(with, "agent")?;
                let mut args: Vec<String> = vec![
                    "automation".into(),
                    "agent-set".into(),
                    placement.to_string(),
                    "--step".into(),
                    step.to_string(),
                    "--agent".into(),
                    agent.into(),
                ];
                if let Some(model) = with.get("model").and_then(|v| v.as_str()) {
                    args.push("--model".into());
                    args.push(model.to_string());
                }
                args.push("--json".into());
                self.run_json(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
                Ok(Outcome::action(format!("step {step} is carried out by {agent} at placement {placement}")))
            }
            "entry" => {
                let automation = self.resolve(with)?;
                let placement = self.resolve_key(with, "placement")?;
                self.run_json(&[
                    "automation",
                    "entry-set",
                    &automation.to_string(),
                    "--placement",
                    &placement.to_string(),
                    "--json",
                ])?;
                Ok(Outcome::action(format!("automation {automation} starts at placement {placement}")))
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
                // an action takes in.
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
                let inside = opt_bool(with, "in_action").unwrap_or(false);
                let from = self.way_out(with)?;
                let mut args: Vec<String> = vec!["automation".into(), "edge-add".into()];
                if inside {
                    args.push("--in-action".into());
                }
                args.push("--from".into());
                args.push(from.clone());
                let halt = opt_bool(with, "halt").unwrap_or(false);
                let exit_to = with.get("exit_to").and_then(|v| v.as_str());
                let goes = match (with.contains_key("to"), exit_to, halt) {
                    (true, None, false) => {
                        args.push("--to".into());
                        let to = self.resolve_key(with, "to")?;
                        args.push(to.to_string());
                        format!("on to box {to}")
                    }
                    // Carried out to the action's own way out. The flag's value is optional on the
                    // command — bare, the unnamed one — so the empty name is left off rather than
                    // passed as an empty word.
                    (false, Some(name), false) => {
                        if !inside {
                            return Err(
                                "`exit_to` leaves an action by its own way out — an automation's picture has nothing outside it, so it goes with `in_action: true`"
                                    .to_string(),
                            );
                        }
                        args.push("--exit-to".into());
                        if !name.is_empty() {
                            args.push(name.into());
                        }
                        match name {
                            "" => "out by the action's unnamed way out".to_string(),
                            named => format!("out by the action's way out `{named}`"),
                        }
                    }
                    (false, None, true) => {
                        args.push("--halt".into());
                        "stopping the run for a person".to_string()
                    }
                    (false, None, false) => {
                        args.push("--done".into());
                        "closing the run".to_string()
                    }
                    _ => {
                        return Err(
                            "an edge goes on to a box (`to`), out of the action (`exit_to`), or stops the run (`halt`) — name one of them, or none to close the run"
                                .to_string(),
                        )
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
            // **Inside an action, either end may be the action itself** — the box `0` on the command.
            // A road says so by leaving that end out, there being no binding to name it by.
            "wire-add" => {
                let inside = opt_bool(with, "in_action").unwrap_or(false);
                let both_named = with.contains_key("target") && with.contains_key("to");
                if !(inside || both_named) {
                    return Err(
                        "a wire on an automation joins two placements, and both are named — only inside an action (`in_action: true`) is an end left out, for the action itself"
                            .to_string(),
                    );
                }
                let from = match with.contains_key("target") {
                    true => self.way_out(with)?,
                    false => "0".to_string(),
                };
                let to = match with.contains_key("to") {
                    true => self.resolve_key(with, "to")?.to_string(),
                    false => "0".to_string(),
                };
                let from_port = req_str(with, "from_port")?;
                let to_port = req_str(with, "to_port")?;
                let mut args: Vec<String> = vec!["automation".into(), "wire-add".into()];
                if inside {
                    args.push("--in-action".into());
                }
                args.extend([
                    "--from".into(),
                    from.clone(),
                    "--from-port".into(),
                    from_port.to_string(),
                    "--to".into(),
                    to.clone(),
                    "--to-port".into(),
                    to_port.to_string(),
                    "--json".into(),
                ]);
                let id = self.bound_id(&args, "automation_wire", bind)?;
                Ok(Outcome::action(format!(
                    "`{from}` hands `{from_port}` to box {to} as `{to_port}` (wire {id})"
                )))
            }
            // A setting is only ever the action's to declare.
            "cfg-add" => {
                let action = self.resolve_key(with, "action")?;
                let name = req_str(with, "name")?;
                let kind = req_str(with, "kind")?;
                let mut args: Vec<String> = vec![
                    "automation".into(),
                    "cfg-add".into(),
                    "--action".into(),
                    action.to_string(),
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
                Ok(Outcome::action(format!(
                    "declared setting {id} `{name}` ({kind}) on library action {action}"
                )))
            }
            // And answered on one placement of it.
            "cfg-set" => {
                let placement = self.resolve(with)?;
                let name = req_str(with, "name")?;
                let mut args: Vec<String> = vec![
                    "automation".into(),
                    "cfg-set".into(),
                    placement.to_string(),
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
                Ok(Outcome::action(format!("setting `{name}` on placement {placement} {said}")))
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
                // Whether a pause has been asked for and not yet settled. Pausing waits for the step
                // under way to report, so a run pressed pause on is still `running`; this is the one
                // thing that says the press landed.
                if let Some(want) = opt_bool(with, "pause_requested") {
                    let got = row["pause_requested"].as_bool();
                    pass = pass && got == Some(want);
                    let got = got.map_or("(none reported)".to_string(), |b| b.to_string());
                    said.push_str(&format!(", pause asked `{got}`, expected `{want}`"));
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
            // lines. `placements` is the second half of what a row is read for — what this is, and
            // whether anything is built onto it yet.
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
                if with.contains_key("placements") {
                    let want = req_i64(with, "placements")?;
                    let got = row.and_then(|one| one["placements"].as_i64());
                    pass = pass && got == Some(want);
                    said.push_str(&format!(
                        ", with {} actions placed on it, expected {want}",
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
            // A library action's row. `used_by` counts automations rather than placements — what it is
            // read for is how far a rewrite of what it holds carries — `steps` how many steps it holds,
            // and `reach` says which of the two shelves it sits on, which is the column the listing
            // carries rather than a second list.
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
                if with.contains_key("steps") {
                    let want = req_i64(with, "steps")?;
                    let got = row["steps"].as_i64();
                    pass = pass && got == Some(want);
                    said.push_str(&format!(
                        ", holding {} steps (expected {want})",
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
                // The row carries the note's first line that is not blank, and no more — so that is
                // what is compared, and the listing's whole note is cut the way the screen cuts it.
                if let Some(want) = with.get("note").and_then(|v| v.as_str()) {
                    let got = first_line(row["action"]["note"].as_str().unwrap_or(""));
                    pass = pass && got == want;
                    said.push_str(&format!(", reading \"{got}\" under its name (expected \"{want}\")"));
                }
                said.push_str(if pass { ", as expected" } else { ", MISMATCH" });
                Ok(Outcome::assert(pass, said))
            }
            // The move is asked again and the error it is turned away with is read — a refusal writes
            // nothing, so the second ask leaves the store as the first did. Core names each
            // automation standing in the way as `'<name>' (<id>)`, so the id in brackets is what is
            // looked for: the name is a road's to change, the id is not.
            "scope-refusal-names" => {
                let action = self.resolve(with)?;
                let named = self.resolve_key(with, "names")?;
                let args = self.scope_args(action, with)?;
                let error = self.refusal_in(
                    &self.session.cwd,
                    &args.iter().map(String::as_str).collect::<Vec<_>>(),
                )?;
                let message = error["message"].as_str().unwrap_or("");
                let pass = message.contains(&format!("({named})"));
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "moving library action {action} was refused saying \"{message}\" — {} automation {named}{}",
                        if pass { "naming" } else { "without naming" },
                        if pass { ", as expected" } else { " (MISMATCH)" }
                    ),
                ))
            }
            // One placement on an automation, read back off `automation show`: what it runs under,
            // with the answers written on this placement. The families are asked for whole, so a
            // build that grew a way out nobody declared is a mismatch rather than something nobody
            // looked at.
            "placement-read" => {
                let automation = self.resolve(with)?;
                judge_placement(automation, &self.definition(automation)?, with)
            }
            // One step inside a library action, read back off `automation action-show`.
            "step-read" => {
                let action = self.resolve(with)?;
                judge_step(action, &self.action_definition(action)?, with)
            }
            // Where leaving one way out takes the run, on either picture. The pair an edge hangs on is
            // the box and the name it carries, which is the pair `edge-add` writes it under.
            "edge-read" => {
                let target = self.resolve(with)?;
                let view = match opt_bool(with, "in_action").unwrap_or(false) {
                    true => Picture::Action(self.action_definition(target)?),
                    false => Picture::Automation(self.definition(target)?),
                };
                judge_edge(target, &view, with)
            }
            _ => Err(unmapped(Domain::Automation, op)),
        }
    }

    /// One automation's definition, resolved — the placements on it and what joins them.
    /// The command that moves `action` to the reach a step names: `--global` for the device's
    /// library, `--project <id>` for a named project's, and no flag for the project the run stands
    /// in — which is what the command does with neither.
    fn scope_args(&self, action: i64, with: &Args) -> Result<Vec<String>, String> {
        let mut args: Vec<String> =
            vec!["automation".into(), "action-scope-set".into(), action.to_string()];
        match req_str(with, "reach")? {
            "device" => args.push("--global".into()),
            "project" => {
                if with.contains_key("project") {
                    args.push("--project".into());
                    args.push(self.resolve_key(with, "project")?.to_string());
                }
            }
            other => return Err(format!("`reach` does not know `{other}` — it is device / project")),
        }
        args.push("--json".into());
        Ok(args)
    }

    fn definition(&self, automation: i64) -> Result<serde_json::Value, String> {
        self.run_json(&["automation", "show", &automation.to_string(), "--json"])
    }

    /// One library action's definition — the steps inside it and what joins them.
    fn action_definition(&self, action: i64) -> Result<serde_json::Value, String> {
        self.run_json(&["automation", "action-show", &action.to_string(), "--json"])
    }

    /// **Which of the two declares it** — a step, or a library action.
    ///
    /// Both declare their own ways out and inputs: a step's are what a line inside the
    /// action leaves from and what a wire inside it fills, and an action's are what its placements
    /// leave by and take in. A road says which it means by naming `step:` or `action:`.
    fn declarer(&self, with: &Args) -> Result<(&'static str, i64, &'static str), String> {
        match with.contains_key("action") {
            true => Ok(("--action", self.resolve_key(with, "action")?, "library action")),
            false => Ok(("--step", self.resolve_key(with, "step")?, "step")),
        }
    }

    /// The way out an edge or a wire leaves by, written the one way the command takes it:
    /// `<box>:<way out>`, where the bare `<box>:` is the unnamed one and `<box>:*` the error one. The
    /// box is a placement, or inside an action a step — the command reads which off `--in-action`.
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
    // The order the filter takes its tasks in. One word, and only beside the parts: the command
    // refuses it alone, and that refusal is what a road leaving the parts out is there to read.
    if let Some(value) = with.get("sort") {
        let sort = value.as_str().ok_or("`sort` takes a key `task list --sort` takes")?;
        args.push("--sort".into());
        args.push(sort.to_string());
        said.push(format!("sort `{sort}`"));
    }
    if said.is_empty() {
        return Err(
            "a setting is answered in the shape its kind takes, or left unanswered with `clear`"
                .to_string(),
        );
    }
    Ok(format!("answered {}", said.join(", ")))
}

/// One placement on an automation, judged against what a road said it runs under.
///
/// **The three families are read whole.** `exits`, `inputs` and `settings` each name all of what the
/// placement runs under rather than a sample of it: a build that grew a way out nobody wrote would
/// pass every question asked one at a time, and the whole point of reading a definition back is that
/// what is there is what was built. The first two are the action's, the answers in the third this
/// placement's own.
fn judge_placement(automation: i64, view: &serde_json::Value, with: &Args) -> Result<Outcome, String> {
    let name = req_str(with, "name")?;
    let present = opt_bool(with, "present").unwrap_or(true);
    let placed = placements_named(view, name);
    let one = match (placed.as_slice(), present) {
        ([], _) => {
            return Ok(Outcome::assert(
                !present,
                format!(
                    "automation {automation} has no placement of `{name}` (expected {}, {})",
                    if present { "one" } else { "none" },
                    if present { "MISMATCH" } else { "as expected" }
                ),
            ))
        }
        (_, false) => {
            return Ok(Outcome::assert(
                false,
                format!("automation {automation} still has a placement of `{name}` (MISMATCH)"),
            ))
        }
        ([one], true) => *one,
        (many, true) => {
            return Err(format!(
                "automation {automation} places `{name}` {} times — a road reading one of them gives its actions names that tell them apart",
                many.len()
            ))
        }
    };
    let mut pass = true;
    let mut said = format!("the placement of `{name}` on automation {automation} is there");
    if with.contains_key("exits") {
        let (ok, note) = judge_names(with, "exits", &exit_names(one), "leaving by")?;
        pass = pass && ok;
        said.push_str(&note);
    }
    if with.contains_key("inputs") {
        let (ok, note) = judge_names(with, "inputs", &port_names(one), "taking in")?;
        pass = pass && ok;
        said.push_str(&note);
    }
    if with.contains_key("settings") {
        let (ok, note) = judge_settings(with, rows_of(one, "settings"))?;
        pass = pass && ok;
        said.push_str(&note);
    }
    said.push_str(if pass { ", as expected" } else { ", MISMATCH" });
    Ok(Outcome::assert(pass, said))
}

/// One step inside a library action, judged against what a road said it was built as — the prompt it
/// runs on, and its ways out and inputs read whole, for the reason a placement's are.
fn judge_step(action: i64, view: &serde_json::Value, with: &Args) -> Result<Outcome, String> {
    let name = req_str(with, "name")?;
    let present = opt_bool(with, "present").unwrap_or(true);
    let Some(step) = step_named(view, name) else {
        return Ok(Outcome::assert(
            !present,
            format!(
                "library action {action} has no step `{name}` (expected {}, {})",
                if present { "one" } else { "none" },
                if present { "MISMATCH" } else { "as expected" }
            ),
        ));
    };
    if !present {
        return Ok(Outcome::assert(
            false,
            format!("library action {action} still has a step `{name}` (MISMATCH)"),
        ));
    }
    let mut pass = true;
    let mut said = format!("step `{name}` of library action {action} is defined");
    if let Some(want) = with.get("prompt").and_then(|v| v.as_str()) {
        let got = step["step"]["prompt"].as_str().unwrap_or("(none reported)");
        pass = pass && got == want;
        said.push_str(&format!(", running on `{got}` (expected `{want}`)"));
    }
    if with.contains_key("exits") {
        let (ok, note) = judge_names(with, "exits", &exit_names(step), "leaving by")?;
        pass = pass && ok;
        said.push_str(&note);
    }
    if with.contains_key("inputs") {
        let (ok, note) = judge_names(with, "inputs", &port_names(step), "taking in")?;
        pass = pass && ok;
        said.push_str(&note);
    }
    for (key, column, what) in [
        ("task_notes", "show_notes", "the task's notes"),
        ("task_decisions", "show_decisions", "the decisions linked to the task"),
        ("task_comments", "show_comments", "the comments on the task"),
        ("history", "show_history", "the run's story so far"),
    ] {
        if let Some(want) = opt_bool(with, key) {
            let got = step["step"][column].as_bool();
            pass = pass && got == Some(want);
            said.push_str(&format!(
                ", {} {what} (expected {})",
                match got {
                    Some(true) => "handed",
                    Some(false) => "not handed",
                    None => "(none reported for)",
                },
                if want { "handed" } else { "not handed" }
            ));
        }
    }
    said.push_str(if pass { ", as expected" } else { ", MISMATCH" });
    Ok(Outcome::assert(pass, said))
}

/// The picture an edge is read off: an automation's, whose boxes are placements named by the action
/// standing on each, or an action's, whose boxes are its steps.
enum Picture {
    Automation(serde_json::Value),
    Action(serde_json::Value),
}

impl Picture {
    fn view(&self) -> &serde_json::Value {
        match self {
            Picture::Automation(view) | Picture::Action(view) => view,
        }
    }

    /// The id of the box a road names. On an automation's picture that is the placement the named
    /// action stands on — refused where it stands on two, since which of them the road meant is not
    /// something a name can say.
    fn box_named(&self, name: &str) -> Result<Option<i64>, String> {
        match self {
            Picture::Action(view) => Ok(step_named(view, name).and_then(|one| one["step"]["id"].as_i64())),
            Picture::Automation(view) => match placements_named(view, name).as_slice() {
                [] => Ok(None),
                [one] => Ok(one["placement"]["id"].as_i64()),
                many => Err(format!(
                    "`{name}` is placed {} times — a road reading an edge gives its actions names that tell them apart",
                    many.len()
                )),
            },
        }
    }

    /// The name a box id carries on this picture — what turns the ids an edge holds back into the
    /// words a road wrote.
    fn name_of(&self, id: Option<i64>) -> Option<String> {
        let id = id?;
        let (key, rows, name) = match self {
            Picture::Action(view) => ("step", rows_of(view, "steps"), "step"),
            Picture::Automation(view) => ("placement", rows_of(view, "placements"), "action"),
        };
        rows.iter()
            .find(|one| one[key]["id"].as_i64() == Some(id))
            .and_then(|one| one[name]["name"].as_str())
            .map(str::to_string)
    }
}

/// Where leaving one way out takes the run — on to a box (`to`), out by one of the action's own ways
/// out (`exit_to`), or to an end of its own (`ends`).
fn judge_edge(owner: i64, picture: &Picture, with: &Args) -> Result<Outcome, String> {
    let from = req_str(with, "from")?;
    let exit = with.get("exit").and_then(|v| v.as_str()).filter(|e| !e.is_empty()).unwrap_or(DONE_EXIT);
    let named = format!("`{exit}`");
    let Some(from_id) = picture.box_named(from)? else {
        return Ok(Outcome::assert(false, format!("{owner} has no box `{from}` to leave (MISMATCH)")));
    };
    let view = picture.view();
    let Some(edge) = rows_of(view, "edges").iter().find(|one| {
        one["from_id"].as_i64() == Some(from_id)
            && exit_named(view, one["exit_id"].as_i64()).as_deref() == Some(exit)
    }) else {
        return Ok(Outcome::assert(false, format!("nothing happens after {named} of `{from}` (MISMATCH)")));
    };
    let ends = edge["ends"].as_str().unwrap_or("(none reported)");
    let pass;
    let mut said = format!("after {named} of `{from}`, the run ");
    let asked = (
        with.get("to").and_then(|v| v.as_str()),
        with.get("exit_to").and_then(|v| v.as_str()),
        with.get("ends").and_then(|v| v.as_str()),
    );
    match asked {
        (Some(want), None, None) => {
            let got = picture.name_of(edge["to_id"].as_i64());
            pass = ends == "go" && got.as_deref() == Some(want);
            said.push_str(&format!(
                "goes to `{}` (`{ends}`, expected `{want}`)",
                got.unwrap_or_else(|| "nowhere".to_string())
            ));
        }
        (None, Some(want), None) => {
            let want = if want.is_empty() { DONE_EXIT } else { want };
            let got = exit_named(view, edge["exit_to_id"].as_i64()).unwrap_or_default();
            pass = ends == "exit" && got == want;
            said.push_str(&format!(
                "leaves the action by `{got}` (`{ends}`, expected the action's way out `{want}`)"
            ));
        }
        (None, None, Some(want)) => {
            pass = ends == want;
            said.push_str(&format!("ends `{ends}` (expected `{want}`)"));
        }
        (None, None, None) => {
            pass = true;
            said.push_str(&format!("ends `{ends}`"));
        }
        _ => {
            return Err(
                "an edge goes on to a box (`to`), out of the action (`exit_to`) or to an end (`ends`) — name one of them"
                    .to_string(),
            )
        }
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

/// One step of an action, found by the name it was built under.
fn step_named<'a>(view: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    rows_of(view, "steps").iter().find(|one| one["step"]["name"].as_str() == Some(name))
}

/// The placements on an automation of the action carrying this name — the name its box is drawn
/// under. Usually one; an action placed twice is two.
fn placements_named<'a>(view: &'a serde_json::Value, name: &str) -> Vec<&'a serde_json::Value> {
    rows_of(view, "placements")
        .iter()
        .filter(|one| one["action"]["name"].as_str() == Some(name))
        .collect()
}

/// **The name of the way out a line keys**, found among every way out the read declares — a
/// placement's, a step's, the action's own — and named the way an edge names one: the unnamed one is
/// the empty name. A line keys the row rather than naming it, and a road reads it by
/// the name the way out carries now. `None` where no way out in the read carries that id.
fn exit_named(view: &serde_json::Value, id: Option<i64>) -> Option<String> {
    let id = id?;
    let boxes = rows_of(view, "placements").iter().chain(rows_of(view, "steps"));
    boxes
        .flat_map(|one| rows_of(one, "exits"))
        .chain(rows_of(view, "exits"))
        .find(|one| one["exit"]["id"].as_i64() == Some(id))
        .map(|one| one["exit"]["name"].as_str().unwrap_or("").to_string())
}

/// The ways out a box declares, named the way an edge names one: the error one is `*`. A way out
/// with no name — which no build from v71 on writes — reads as the empty name.
fn exit_names(declarer: &serde_json::Value) -> Vec<String> {
    rows_of(declarer, "exits")
        .iter()
        .map(|one| one["exit"]["name"].as_str().unwrap_or("").to_string())
        .collect()
}

/// The inputs a box declares, by name.
fn port_names(declarer: &serde_json::Value) -> Vec<String> {
    rows_of(declarer, "inputs")
        .iter()
        .map(|one| one["name"].as_str().unwrap_or("").to_string())
        .collect()
}

/// One family of names, compared whole against the list a road wrote.
fn judge_names(with: &Args, key: &str, got: &[String], reading: &str) -> Result<(bool, String), String> {
    let want = word_list(with, key)?;
    Ok((got == want.as_slice(), format!(", {reading} {got:?} (expected {want:?})")))
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

/// The line a library action's row draws under its name: the first line of its note that is not
/// blank, trimmed — `AutomationActionsTab`'s `firstLine`, read the same way off the listing.
fn first_line(note: &str) -> &str {
    note.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("")
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

    /// The order rides after the parts, as the one `--sort` the command takes beside them.
    #[test]
    fn a_task_filter_takes_its_order_after_the_parts() {
        let mut args: Vec<String> = Vec::new();
        let said = answer(&with("{ status: todo, sort: \"-due\" }"), &mut args).expect("an answer");
        assert_eq!(args, ["--status", "todo", "--sort", "-due"].map(String::from).to_vec());
        assert!(said.contains("sort `-due`"), "{said}");
    }

    /// What a library action's row is read for is the line the screen draws under the name, so the
    /// listing's note is cut the way the screen cuts it: leading blank lines skipped, the rest dropped.
    #[test]
    fn a_rows_note_is_its_first_line_that_is_not_blank() {
        assert_eq!(first_line("\n  sorts the inbox  \nby hand, once a day"), "sorts the inbox");
        assert_eq!(first_line(" \n\n"), "");
        assert_eq!(first_line(""), "");
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

    /// One automation as `automation show --json` prints it, cut down to what the read-back asserts
    /// look at: two placements, the first answering one of the two settings its action declares, and
    /// the edges between them.
    fn automation() -> serde_json::Value {
        serde_json::json!({
            "automation": { "id": 1, "name": "one task, two actions" },
            "placements": [
                {
                    "placement": { "id": 1, "action_id": 1 },
                    "action": { "id": 1, "name": "take" },
                    "exits": [
                        { "exit": { "id": 1, "name": "完了" }, "outputs": [] },
                        { "exit": { "id": 2, "name": "*" }, "outputs": [] },
                        { "exit": { "id": 5, "name": "got one" }, "outputs": [] },
                    ],
                    "inputs": [{ "id": 1, "name": "brief", "kind": "value", "required": true }],
                    "settings": [
                        { "id": 1, "name": "how many", "kind": "number", "value": "3" },
                        { "id": 2, "name": "where to work", "kind": "folder", "value": null },
                    ],
                },
                {
                    "placement": { "id": 2, "action_id": 2 },
                    "action": { "id": 2, "name": "review" },
                    "exits": [
                        { "exit": { "id": 3, "name": "完了" }, "outputs": [] },
                        { "exit": { "id": 4, "name": "*" }, "outputs": [] },
                    ],
                    "inputs": [],
                    "settings": [],
                },
            ],
            "edges": [
                { "id": 1, "from_id": 1, "exit_id": 5, "to_id": 2, "exit_to_id": null, "ends": "go" },
                { "id": 2, "from_id": 2, "exit_id": 3, "to_id": null, "exit_to_id": null, "ends": "done" },
            ],
            "wires": [],
        })
    }

    /// One library action as `automation action-show --json` prints it, cut the same way: two steps,
    /// the line between them, and the line that carries the second out by the action's own way out.
    fn action() -> serde_json::Value {
        serde_json::json!({
            "action": { "id": 1, "name": "take" },
            "steps": [
                {
                    "step": { "id": 1, "name": "look", "prompt": "look for the next task" },
                    "exits": [
                        { "exit": { "id": 6, "name": "完了" }, "outputs": [] },
                        { "exit": { "id": 7, "name": "*" }, "outputs": [] },
                        { "exit": { "id": 8, "name": "found" }, "outputs": [] },
                    ],
                    "inputs": [{ "id": 2, "name": "brief", "kind": "value", "required": false }],
                },
                {
                    "step": { "id": 2, "name": "claim", "prompt": "take it" },
                    "exits": [
                        { "exit": { "id": 9, "name": "完了" }, "outputs": [] },
                        { "exit": { "id": 10, "name": "*" }, "outputs": [] },
                    ],
                    "inputs": [],
                },
            ],
            "edges": [
                { "id": 3, "from_id": 1, "exit_id": 8, "to_id": 2, "exit_to_id": null, "ends": "go" },
                { "id": 4, "from_id": 2, "exit_id": 9, "to_id": null, "exit_to_id": 13, "ends": "exit" },
            ],
            "wires": [],
            "exits": [
                { "exit": { "id": 11, "name": "完了" }, "outputs": [] },
                { "exit": { "id": 12, "name": "*" }, "outputs": [] },
                { "exit": { "id": 13, "name": "got one" }, "outputs": [] },
            ],
        })
    }

    /// A placement read back as it was built: the ways out its action declares — the two it is born
    /// with and the one that was declared — what it takes in, and its settings with the answer this
    /// placement wrote for one of them.
    #[test]
    fn a_placement_is_read_back_as_it_was_built() {
        let read = judge_placement(
            1,
            &automation(),
            &with(
                r#"{ name: take, exits: [完了, "*", got one], inputs: [brief],
                     settings: { how many: 3, where to work: ~ } }"#,
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
            r#"{ name: take, exits: [完了, "*"] }"#,
            r#"{ name: take, inputs: [] }"#,
            r#"{ name: take, settings: { how many: 3 } }"#,
        ] {
            let read = judge_placement(1, &automation(), &with(asked)).expect("a verdict");
            assert!(!read.pass, "asking `{asked}` should have caught the rest: {}", read.note);
        }
        let read = judge_step(1, &action(), &with(r#"{ name: look, exits: [完了, "*"] }"#))
            .expect("a verdict");
        assert!(!read.pass, "a step's ways out are read whole too: {}", read.note);
    }

    /// A setting nobody answered is a state of its own rather than an empty answer, and a road says so
    /// by writing nothing for it. The answer that was written is compared as the number it is, not as
    /// the JSON the column holds it in.
    #[test]
    fn an_unanswered_setting_is_not_an_empty_answer() {
        let wrong = judge_placement(
            1,
            &automation(),
            &with(r#"{ name: take, settings: { how many: 3, where to work: "" } }"#),
        )
        .expect("a verdict");
        assert!(!wrong.pass, "an empty answer is not the same as none: {}", wrong.note);

        let also_wrong = judge_placement(
            1,
            &automation(),
            &with(r#"{ name: take, settings: { how many: "3", where to work: ~ } }"#),
        )
        .expect("a verdict");
        assert!(!also_wrong.pass, "the number was answered as a number: {}", also_wrong.note);
    }

    /// A step inside the action, read back with the prompt it carries and what it declares of its own
    /// — which is not what the action declares to the automations placing it.
    #[test]
    fn a_step_is_read_back_as_it_was_built() {
        let read = judge_step(
            1,
            &action(),
            &with(r#"{ name: look, prompt: look for the next task, exits: [完了, "*", found], inputs: [brief] }"#),
        )
        .expect("a verdict");
        assert!(read.pass, "{}", read.note);
    }

    /// A box nobody built. Every read above would pass against a build that answered with whatever
    /// name it was handed, and this is the one that says they were answers.
    #[test]
    fn a_box_nobody_built_is_read_back_as_absent() {
        let absent = judge_placement(1, &automation(), &with("{ name: publish, present: false }"))
            .expect("a verdict");
        assert!(absent.pass, "{}", absent.note);
        let claimed = judge_placement(1, &automation(), &with("{ name: take, present: false }"))
            .expect("a verdict");
        assert!(!claimed.pass, "a placement that is there is not absent: {}", claimed.note);

        let absent = judge_step(1, &action(), &with("{ name: publish, present: false }")).expect("a verdict");
        assert!(absent.pass, "{}", absent.note);
        let claimed = judge_step(1, &action(), &with("{ name: look, present: false }")).expect("a verdict");
        assert!(!claimed.pass, "a step that is there is not absent: {}", claimed.note);
    }

    /// Where leaving one way out takes the run on an automation's picture: on to a box named by the
    /// action standing on it, or to an end of the run's own.
    #[test]
    fn a_way_out_is_read_back_with_where_it_goes() {
        let picture = Picture::Automation(automation());
        for asked in ["{ from: take, exit: got one, to: review }", "{ from: review, ends: done }"] {
            let read = judge_edge(1, &picture, &with(asked)).expect("a verdict");
            assert!(read.pass, "{}", read.note);
        }

        // The way out is half of what an edge hangs on, so the done one of a box whose edge leaves
        // by a named one is nothing at all.
        let elsewhere = judge_edge(1, &picture, &with("{ from: take, to: review }")).expect("a verdict");
        assert!(!elsewhere.pass, "{}", elsewhere.note);
    }

    /// And inside an action, where a step's way out may be carried out by one of the action's own —
    /// the line that makes what a step does reach the placement around it.
    #[test]
    fn a_way_out_inside_an_action_is_read_back_with_where_it_goes() {
        let picture = Picture::Action(action());
        for asked in ["{ from: look, exit: found, to: claim }", "{ from: claim, exit_to: got one }"] {
            let read = judge_edge(1, &picture, &with(asked)).expect("a verdict");
            assert!(read.pass, "{}", read.note);
        }
        let other = judge_edge(1, &picture, &with("{ from: claim, exit_to: '' }")).expect("a verdict");
        assert!(!other.pass, "carried out by a different way out of the action: {}", other.note);
    }

    /// An action placed twice is two boxes under one name, and a read naming it cannot say which it
    /// meant — so it is refused rather than answered off whichever came first.
    #[test]
    fn a_name_placed_twice_is_refused_rather_than_guessed() {
        let mut twice = automation();
        twice["placements"][1]["action"]["name"] = serde_json::json!("take");
        assert!(judge_placement(1, &twice, &with("{ name: take }")).is_err());
        assert!(judge_edge(1, &Picture::Automation(twice), &with("{ from: take, to: take }")).is_err());
    }
}
