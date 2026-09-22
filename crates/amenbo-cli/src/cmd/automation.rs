//! `automation`: the library of prompts, and the pictures built out of them — steps, the ways out of
//! each one, what runs after each way out is taken, and what is handed along.
//!
//! **Only the building side is here** — what a run does is written elsewhere. Nothing in this file
//! refuses an unfinished automation, for the reason [`amenbo_core::ops::automation`] gives: the launch
//! check is where a person is about to be let down by one.
//!
//! **The parts are named by id.** They carry no conversational ref: a step is named by the automation
//! it sits in, not by a number anybody types back, so every `add` here prints the id the next command
//! takes.

use serde_json::{json, Value};

use amenbo_core::model::{
    AutomationCfgKind, AutomationOwner, AutomationPortDirection, AutomationPortKind,
    DEFAULT_MAX_TIMES,
};
use amenbo_core::model::{
    AutomationRun, AutomationRunDef, AutomationRunStep, AutomationRunTask, AutomationRunValue,
};
use amenbo_core::model::{AttachmentTarget, AutomationStoppedReason};
use amenbo_core::ops::automation::{EdgeTarget, NewAutomation, NewStep, StepSource};
use amenbo_core::ops::automation_report::{Next, Produced};
use amenbo_core::ops::automation_run::Launcher;
use amenbo_core::ops::automation_stop::{Paused, Resumed};
use amenbo_core::time::Timestamp;
use amenbo_core::Store;

use crate::cli::*;
use crate::cmd::arg::{body_arg, body_arg_opt};
use crate::cmd::labels::task_label;
use crate::cmd::place::project_or_bound;
use crate::cmd::task::resolve_task;
use crate::output::{confirm, human, print_json, write_envelope, CliError, Flags};

/// Where an edge or a wire leaves from, as one token: `<step>:<way out>`. `4` and `4:` are both the
/// unnamed way out, `4:*` the error one.
///
/// **Written as one token because the two halves are one place.** A way out is named against whichever
/// of the step and its library action declares it, so a name without the step it is read on names
/// nothing — and splitting them across two flags lets a caller pass a name that belongs to some other
/// step's list.
fn parse_point(s: &str) -> Result<(i64, Option<String>), CliError> {
    let (step, exit) = match s.split_once(':') {
        Some((step, exit)) => (step, (!exit.is_empty()).then(|| exit.to_string())),
        None => (s, None),
    };
    let step: i64 = step.trim().parse().map_err(|_| CliError {
        code: "invalid_value",
        message: format!("'{s}' does not name a step and a way out."),
        hint: Some("Write it as <step>:<way out> — `4:` is the unnamed way out, `4:*` the error one.".to_string()),
        exit: 2,
    })?;
    Ok((step, exit))
}

/// What a port carries. Spelled out here rather than left to the model's own parse so the refusal names
/// the flag and lists what it takes, the way `--priority`'s does.
fn parse_port_kind(s: &str) -> Result<AutomationPortKind, CliError> {
    AutomationPortKind::parse(s).ok_or_else(|| CliError {
        code: "invalid_value",
        message: format!("--kind value '{s}' is invalid."),
        hint: Some("Specify one of: value | file | task_take | task_make.".to_string()),
        exit: 2,
    })
}

/// What kind of answer a setting takes.
fn parse_cfg_kind(s: &str) -> Result<AutomationCfgKind, CliError> {
    AutomationCfgKind::parse(s).ok_or_else(|| CliError {
        code: "invalid_value",
        message: format!("--kind value '{s}' is invalid."),
        hint: Some("Specify one of: taskfilter | folder | choice | number | text.".to_string()),
        exit: 2,
    })
}

/// Which of the two declared a way out or a setting, from the pair of flags that name one.
fn declarer_from_flags(step: Option<i64>, action: Option<i64>) -> Result<(AutomationOwner, i64), CliError> {
    match (step, action) {
        (Some(id), None) => Ok((AutomationOwner::Step, id)),
        (None, Some(id)) => Ok((AutomationOwner::Action, id)),
        _ => Err(CliError {
            code: "invalid_value",
            message: "name what declares this — one of --step or --action.".to_string(),
            hint: Some("A step that runs a library action declares nothing of its own; write it on the action.".to_string()),
            exit: 2,
        }),
    }
}

/// Where an edge goes, from the three flags that say so. `--to` names the next step; the other two end
/// the run.
fn edge_target(to: Option<i64>, done: bool, halt: bool) -> Result<EdgeTarget, CliError> {
    match (to, done, halt) {
        (Some(to), false, false) => Ok(EdgeTarget::Go(to)),
        (None, true, false) => Ok(EdgeTarget::Done),
        (None, false, true) => Ok(EdgeTarget::Halt),
        _ => Err(CliError {
            code: "invalid_value",
            message: "say what happens after this way out — one of --to <step>, --done or --halt.".to_string(),
            hint: None,
            exit: 2,
        }),
    }
}

/// The answer to a setting, in the shape its kind takes. **A task filter is never one string**: it is
/// built from the options that name each part, where the same option twice is any-of and two different
/// options are both — which is the same reading `--filter` gives a `key:a,b key2:c` expression, and it
/// is validated by parsing that expression before the answer is written.
///
/// What the JSON's own type says is which kind answered it: an object for a task filter, a number for a
/// number, a string for the other three. Nothing here reads the declaration — no command lists one — so
/// an answer in the wrong shape is caught when the run reads it, not here.
fn cfg_value(o: &CfgAnswer) -> Result<Option<Value>, CliError> {
    let filter: Vec<(&str, &Vec<String>)> = vec![
        ("status", &o.status),
        ("priority", &o.priority),
        ("assignee", &o.assignee),
        ("dim", &o.dim),
        ("ready", &o.ready),
        ("done", &o.done),
        ("due", &o.due),
    ];
    let filtered: Vec<(&str, &Vec<String>)> =
        filter.into_iter().filter(|(_, v)| !v.is_empty()).collect();
    let singles = [
        o.folder.as_ref().map(|v| json!(v)),
        o.choice.as_ref().map(|v| json!(v)),
        o.number.map(|v| json!(v)),
        o.text.as_ref().map(|v| json!(v)),
    ];
    let single: Vec<Value> = singles.into_iter().flatten().collect();
    if o.clear {
        if !single.is_empty() || !filtered.is_empty() {
            return Err(CliError {
                code: "invalid_value",
                message: "--clear leaves the setting unanswered, so it takes no answer beside it.".to_string(),
                hint: None,
                exit: 2,
            });
        }
        return Ok(None);
    }
    if single.len() > 1 || (!single.is_empty() && !filtered.is_empty()) {
        return Err(CliError {
            code: "invalid_value",
            message: "a setting takes one answer, in the shape its kind takes.".to_string(),
            hint: Some("Pass the options of one kind: --folder, --choice, --number, --text, or the task-filter options together.".to_string()),
            exit: 2,
        });
    }
    if let Some(v) = single.into_iter().next() {
        return Ok(Some(v));
    }
    if filtered.is_empty() {
        return Err(CliError {
            code: "invalid_value",
            message: "no answer given.".to_string(),
            hint: Some("Pass --folder, --choice, --number, --text, the task-filter options, or --clear.".to_string()),
            exit: 2,
        });
    }
    // Read as the filter it will be run as, so a value nothing accepts is refused while the person who
    // wrote it is still here — rather than at the launch of a run, days later.
    let expr = filtered
        .iter()
        .map(|(k, v)| format!("{k}:{}", v.join(",")))
        .collect::<Vec<_>>()
        .join(" ");
    amenbo_core::query::Filter::parse(&expr, amenbo_core::time::today()).map_err(CliError::from)?;
    let mut map = serde_json::Map::new();
    for (k, v) in filtered {
        map.insert(k.to_string(), json!(v));
    }
    Ok(Some(Value::Object(map)))
}

/// The options `cfg set` answers a setting with, gathered off the parsed command.
struct CfgAnswer {
    clear: bool,
    folder: Option<String>,
    choice: Option<String>,
    number: Option<i64>,
    text: Option<String>,
    status: Vec<String>,
    priority: Vec<String>,
    assignee: Vec<String>,
    dim: Vec<String>,
    ready: Vec<String>,
    done: Vec<String>,
    due: Vec<String>,
}

pub(crate) fn automation(store: &mut Store, flags: &Flags, sub: AutomationCmd) -> Result<i32, CliError> {
    match sub {
        AutomationCmd::Add { project, name, notes, preamble } => {
            let pid = project_or_bound(store, project)?;
            let new = NewAutomation {
                name,
                notes: body_arg(notes)?,
                // Left out, the standing operating rules go in: what every step is told before its own
                // prompt is the same for every automation until somebody writes otherwise, and an empty
                // one is a thing to pass rather than a thing to fall into.
                preamble: body_arg_opt(preamble)?
                    .unwrap_or_else(|| amenbo_core::agents::DEFAULT_PREAMBLE.to_string()),
            };
            let a = store.automation_add(pid, new).map_err(CliError::from)?;
            write_envelope(flags, "automation.add", "automation", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Created automation: {} ({})", a.name, a.id));
        }
        AutomationCmd::Update { id, name, notes, preamble, archived } => {
            let notes = body_arg_opt(notes)?;
            let preamble = body_arg_opt(preamble)?;
            let a = store
                .automation_update(id, name.as_deref(), notes.as_deref(), preamble.as_deref(), archived)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.update", "automation", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Updated automation: {} ({})", a.name, a.id));
        }
        AutomationCmd::Rm { id } => {
            if !confirm(flags, "delete automation")? {
                return Ok(0);
            }
            store.automation_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.rm", "automation", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted automation: {id}"));
        }
        AutomationCmd::Entry { sub: AutomationEntryCmd::Set { id, step, clear } } => {
            if step.is_none() && !clear {
                return Err(CliError {
                    code: "invalid_value",
                    message: "name the step a run starts at with --step, or --clear to leave none.".to_string(),
                    hint: None,
                    exit: 2,
                });
            }
            let a = store.automation_set_entry(id, step).map_err(CliError::from)?;
            let line = match a.entry_step_id {
                Some(s) => format!("✓ Automation {} starts at step {s}", a.id),
                None => format!("✓ Automation {} starts nowhere", a.id),
            };
            write_envelope(flags, "automation.entry-set", "automation", serde_json::to_value(&a).unwrap(), Some(vec!["entry_step_id".to_string()]), false, line);
        }

        AutomationCmd::Action { sub } => return action(store, flags, sub),
        AutomationCmd::Step { sub } => return step(store, flags, sub),
        AutomationCmd::Exit { sub } => return exit(store, flags, sub),
        AutomationCmd::Port { sub } => return port(store, flags, sub),
        AutomationCmd::Cfg { sub } => return cfg(store, flags, sub),
        AutomationCmd::Edge { sub } => return edge(store, flags, sub),
        AutomationCmd::Wire { sub } => return wire(store, flags, sub),
        AutomationCmd::Note { sub } => return note(store, flags, sub),
        AutomationCmd::Run { sub } => return run(store, flags, sub),

        AutomationCmd::Start { id } => {
            let known = startable(store);
            let by = Launcher {
                startable: known.as_deref(),
                // Nothing is claimed about the models either: asking a provider what it offers is a
                // login shell and that provider starting up, and the answers the app keeps are in the
                // app's own process (`amenbo_core::agent_models`). So a step naming a model this
                // machine does not have is caught when the pane comes up, rather than here.
                models: amenbo_core::ops::automation_run::nothing_asked(),
                lanes: store.config.automation_lanes,
                // Nothing is claimed about the window: a terminal cannot see what is on screen, and a
                // `false` written here would refuse a launch the reader can see perfectly well
                // (`amenbo_core::ops::automation_run::Launcher`).
                workspace_open: None,
                by: Some(flags.facet()?),
            };
            let r = store.automation_launch(id, &by).map_err(CliError::from)?;
            let line = match r.status {
                amenbo_core::model::AutomationRunStatus::Queued => {
                    format!("✓ Run {} is waiting for a lane", r.id)
                }
                _ => format!("✓ Run {} started", r.id),
            };
            write_envelope(flags, "automation.start", "automation_run", serde_json::to_value(&r).unwrap(), None, false, line);
        }
        AutomationCmd::Pause { run } => {
            let paused = store
                .automation_pause(run, store.config.automation_lanes)
                .map_err(CliError::from)?;
            let (value, line) = match &paused {
                Paused::Asked(r) => (
                    json!({ "run": r.id, "state": "asked" }),
                    format!("✓ Run {} pauses at the end of the step under way", r.id),
                ),
                Paused::Now(ended) => (
                    json!({ "run": ended.run.id, "state": "paused" }),
                    format!("✓ Run {} is paused", ended.run.id),
                ),
            };
            write_envelope(flags, "automation.pause", "automation_run", value, None, false, line);
        }
        AutomationCmd::Resume { run } => {
            let resumed = store
                .automation_resume(run, store.config.automation_lanes)
                .map_err(CliError::from)?;
            let (value, line) = match &resumed {
                Resumed::Step { run: r, next } => (
                    json!({ "run": r.id, "state": "running", "step": next.id }),
                    format!("✓ Run {} picks up at {} ({})", r.id, next.name, next.id),
                ),
                Resumed::Queued(r) => (
                    json!({ "run": r.id, "state": "queued" }),
                    format!("✓ Run {} is waiting for a lane", r.id),
                ),
            };
            write_envelope(flags, "automation.resume", "automation_run", value, None, false, line);
        }
        AutomationCmd::Stop { run } => {
            let ended = store
                .automation_stop(run, AutomationStoppedReason::ByHuman, store.config.automation_lanes)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.stop", "automation_run", serde_json::to_value(&ended.run).unwrap(), None, false, format!("✓ Run {} stopped", ended.run.id));
        }

        AutomationCmd::Take { task } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let t = store.automation_take(speaking_for()?, tid).map_err(CliError::from)?;
            write_envelope(flags, "automation.take", "task", serde_json::to_value(&t).unwrap(), None, false, format!("✓ Took {} — {}", task_label(t.id), t.title));
        }
        AutomationCmd::Out { value, file } => {
            let step = speaking_for()?;
            let v = match file {
                // A file is put down as an attachment on this step execution first: what the port
                // carries is the row, so there is nothing to name until the bytes are in.
                Some(path) => {
                    let a = crate::cmd::attach::attach_file(
                        store,
                        flags,
                        AttachmentTarget::AutomationRunStep,
                        step,
                        &path,
                        None,
                    )?;
                    store.automation_out(step, value.trim(), Produced::File(a.id)).map_err(CliError::from)?
                }
                None => {
                    let (name, text) = parse_produced(&value)?;
                    store.automation_out(step, &name, Produced::Value(&text)).map_err(CliError::from)?
                }
            };
            write_envelope(flags, "automation.out", "automation_run_value", serde_json::to_value(&v).unwrap(), None, false, format!("✓ Handed on: {}", v.name));
        }
        AutomationCmd::Done { report, exit, outs } => {
            let step = speaking_for()?;
            // Everything produced goes down before the way out is stamped: `done` refuses a missing
            // required output, and a value written after that refusal would arrive at a step that has
            // already been told it is not finished.
            for one in &outs {
                let (name, text) = parse_produced(one)?;
                store.automation_out(step, &name, Produced::Value(&text)).map_err(CliError::from)?;
            }
            let report = body_arg(report)?;
            let next = store
                .automation_done(step, exit.as_deref(), &report, store.config.automation_lanes)
                .map_err(CliError::from)?;
            let value = json!({ "step": step, "next": next_word(&next) });
            write_envelope(flags, "automation.done", "automation_run_step", value, None, false, next_line(&next));
        }
    }
    Ok(0)
}
// ───────────────────────────── running one ─────────────────────────────

/// **The step execution this command is speaking for**, read off the environment the window opened
/// this terminal with ([`amenbo_core::session::STEP_VAR`]).
///
/// The agent carrying a step out is told what to do and nothing about where it sits, so nothing in
/// its prompt names the step and nothing it types could. The window opened the terminal and knows,
/// and the environment is the one road that reaches an `amenbo` several processes deep.
///
/// **Outside a step it refuses rather than guessing.** There is no "the current step" to fall back on:
/// several runs go at once, and picking the newest would put one step's report on another's record.
fn speaking_for() -> Result<i64, CliError> {
    amenbo_core::env::automation_step().ok_or_else(|| CliError {
        code: "invalid_value",
        message: "this is not a step of a run — `take`, `out` and `done` are typed by the agent a step opened, in the terminal the run opened for it.".to_string(),
        hint: Some("start a run with `automation start <id>`, and the step's own terminal carries what these commands need".to_string()),
        exit: 2,
    })
}

/// **The agent ids this machine was last seen able to start**, or `None` where it has never been
/// asked (`AMB-D-792`).
///
/// It is the remembered answer and not a fresh probe: probing starts a login shell and reads a
/// profile, which is arbitrary code that can wait on a network, and a launch is not the moment to pay
/// that. `None` reaches the launch check as "not asked", which leaves the agent check unmade rather
/// than failing every step on a machine nobody has probed.
fn startable(store: &Store) -> Option<Vec<String>> {
    let config = &store.config;
    let known = config.installed_agents()?;
    let candidates = amenbo_core::wake::candidates(&[], &config.custom_agents, |command| {
        known.iter().any(|one| one == command)
    });
    Some(
        amenbo_core::wake::startable(&candidates)
            .into_iter()
            .map(|one| one.id.clone())
            .collect(),
    )
}

/// `<name>=<value>`, as `out` and `done --out` take it.
fn parse_produced(one: &str) -> Result<(String, String), CliError> {
    match one.split_once('=') {
        Some((name, value)) if !name.trim().is_empty() => {
            Ok((name.trim().to_string(), value.to_string()))
        }
        _ => Err(CliError {
            code: "invalid_value",
            message: format!("'{one}' is not `<name>=<value>`"),
            hint: Some("write what the step hands on as `note=the answer`, naming an output the step declared".to_string()),
            exit: 2,
        }),
    }
}

/// What a run did next, in the one word a caller reads it by.
fn next_word(next: &Next) -> &'static str {
    match next {
        Next::Step(_) => "step",
        Next::Closed(_) => "closed",
        Next::Halted(_) => "halted",
        Next::Paused(_) => "paused",
    }
}

/// The sentence each of those is said in.
fn next_line(next: &Next) -> String {
    match next {
        Next::Step(def) => format!("✓ Step done — next is {} ({})", def.name, def.id),
        Next::Closed(_) => "✓ Step done — the run is over".to_string(),
        Next::Halted(_) => "✓ Step done — the run stopped and is waiting for a person".to_string(),
        Next::Paused(_) => "✓ Step done — the run is paused".to_string(),
    }
}


fn action(store: &mut Store, flags: &Flags, sub: AutomationActionCmd) -> Result<i32, CliError> {
    match sub {
        AutomationActionCmd::Add { project, global, name, prompt } => {
            // The device's library is reached by every project on this machine, so it is nobody's
            // project to put something in: `None` is what core reads as that shelf, and an AI bound to
            // a project is turned away from it there.
            let pid = match global {
                true => None,
                false => Some(project_or_bound(store, project)?),
            };
            let prompt = body_arg(prompt)?;
            let a = store.automation_action_add(pid, &name, &prompt).map_err(CliError::from)?;
            write_envelope(flags, "automation.action-add", "automation_action", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Added action: {} ({})", a.name, a.id));
        }
        AutomationActionCmd::Update { id, name, prompt } => {
            let prompt = body_arg_opt(prompt)?;
            let a = store
                .automation_action_update(id, name.as_deref(), prompt.as_deref())
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.action-update", "automation_action", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Updated action: {} ({})", a.name, a.id));
        }
        AutomationActionCmd::Rm { id } => {
            if !confirm(flags, "delete library action")? {
                return Ok(0);
            }
            store.automation_action_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.action-rm", "automation_action", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted action: {id}"));
        }
    }
    Ok(0)
}

fn step(store: &mut Store, flags: &Flags, sub: AutomationStepCmd) -> Result<i32, CliError> {
    match sub {
        AutomationStepCmd::Add { automation, name, action, prompt, agent, model, interactive, work_dir, report_to_task, no_history } => {
            let prompt = body_arg_opt(prompt)?;
            let source = match (action, prompt) {
                (Some(id), None) => StepSource::Action(id),
                (None, Some(p)) => StepSource::Prompt(p),
                _ => {
                    return Err(CliError {
                        code: "invalid_value",
                        message: "say where this step's prompt comes from — one of --action <id> or --prompt <text>.".to_string(),
                        hint: None,
                        exit: 2,
                    })
                }
            };
            let new = NewStep {
                name,
                source,
                agent,
                model,
                interactive,
                work_dir_ref: work_dir,
                report_to_task,
                // The run's story so far is handed on unless somebody turns it off, so the flag that
                // takes it away is the one written.
                show_history: !no_history,
            };
            let s = store.automation_step_add(automation, new).map_err(CliError::from)?;
            write_envelope(flags, "automation.step-add", "automation_step", serde_json::to_value(&s).unwrap(), None, false, format!("✓ Added step: {} ({})", s.name, s.id));
        }
        AutomationStepCmd::Update { id, name, action, prompt, agent, model, clear_model, interactive, work_dir, clear_work_dir, report_to_task, history } => {
            let prompt = body_arg_opt(prompt)?;
            let source = match (action, prompt) {
                (Some(id), None) => Some(StepSource::Action(id)),
                (None, Some(p)) => Some(StepSource::Prompt(p)),
                _ => None,
            };
            let model = match clear_model {
                true => Some(None),
                false => model.as_deref().map(Some),
            };
            let work_dir = match clear_work_dir {
                true => Some(None),
                false => work_dir.as_deref().map(Some),
            };
            let s = store
                .automation_step_update(id, name.as_deref(), source, agent.as_deref(), model, interactive, work_dir, report_to_task, history)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.step-update", "automation_step", serde_json::to_value(&s).unwrap(), None, false, format!("✓ Updated step: {} ({})", s.name, s.id));
        }
        AutomationStepCmd::Rm { id } => {
            if !confirm(flags, "delete step")? {
                return Ok(0);
            }
            store.automation_step_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.step-rm", "automation_step", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted step: {id}"));
        }
    }
    Ok(0)
}

fn exit(store: &mut Store, flags: &Flags, sub: AutomationExitCmd) -> Result<i32, CliError> {
    match sub {
        AutomationExitCmd::Add { step, action, name } => {
            let (owner_kind, owner_id) = declarer_from_flags(step, action)?;
            let e = store.automation_exit_add(owner_kind, owner_id, Some(&name)).map_err(CliError::from)?;
            write_envelope(flags, "automation.exit-add", "automation_exit", serde_json::to_value(&e).unwrap(), None, false, format!("✓ Added way out: {} ({})", name, e.id));
        }
        AutomationExitCmd::Rename { id, name, clear } => {
            if name.is_none() && !clear {
                return Err(CliError {
                    code: "invalid_value",
                    message: "give the new name with --name, or --clear to make it the unnamed way out.".to_string(),
                    hint: None,
                    exit: 2,
                });
            }
            let e = store.automation_exit_rename(id, name.as_deref()).map_err(CliError::from)?;
            let shown = e.name.clone().unwrap_or_else(|| "(unnamed)".to_string());
            write_envelope(flags, "automation.exit-rename", "automation_exit", serde_json::to_value(&e).unwrap(), Some(vec!["name".to_string()]), false, format!("✓ Renamed way out: {shown} ({})", e.id));
        }
        AutomationExitCmd::Rm { id } => {
            if !confirm(flags, "delete way out")? {
                return Ok(0);
            }
            store.automation_exit_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.exit-rm", "automation_exit", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted way out: {id}"));
        }
    }
    Ok(0)
}

fn port(store: &mut Store, flags: &Flags, sub: AutomationPortCmd) -> Result<i32, CliError> {
    match sub {
        AutomationPortCmd::Add { step, action, exit, name, kind, required } => {
            let kind = parse_port_kind(&kind)?;
            // The direction falls out of the owner and is never asked for: an input belongs to the step
            // or the action that reads it, an output to the way out that produced it, and neither is
            // sayable on the other's.
            let (owner_kind, owner_id, direction) = match (step, action, exit) {
                (Some(id), None, None) => (amenbo_core::model::AutomationPortOwner::Step, id, AutomationPortDirection::In),
                (None, Some(id), None) => (amenbo_core::model::AutomationPortOwner::Action, id, AutomationPortDirection::In),
                (None, None, Some(id)) => (amenbo_core::model::AutomationPortOwner::Exit, id, AutomationPortDirection::Out),
                _ => {
                    return Err(CliError {
                        code: "invalid_value",
                        message: "name what this port hangs off — one of --step, --action or --exit.".to_string(),
                        hint: Some("--step / --action declare what is taken in; --exit declares what that way out hands on.".to_string()),
                        exit: 2,
                    })
                }
            };
            let p = store
                .automation_port_add(owner_kind, owner_id, direction, &name, kind, required)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.port-add", "automation_port", serde_json::to_value(&p).unwrap(), None, false, format!("✓ Added {} port: {} ({})", direction.as_str(), p.name, p.id));
        }
        AutomationPortCmd::Update { id, name, kind, required } => {
            let kind = kind.as_deref().map(parse_port_kind).transpose()?;
            let p = store.automation_port_update(id, name.as_deref(), kind, required).map_err(CliError::from)?;
            write_envelope(flags, "automation.port-update", "automation_port", serde_json::to_value(&p).unwrap(), None, false, format!("✓ Updated port: {} ({})", p.name, p.id));
        }
        AutomationPortCmd::Rm { id } => {
            if !confirm(flags, "delete port")? {
                return Ok(0);
            }
            store.automation_port_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.port-rm", "automation_port", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted port: {id}"));
        }
    }
    Ok(0)
}

fn cfg(store: &mut Store, flags: &Flags, sub: AutomationCfgCmd) -> Result<i32, CliError> {
    match sub {
        AutomationCfgCmd::Add { step, action, name, kind, required, options } => {
            let (owner_kind, owner_id) = declarer_from_flags(step, action)?;
            let kind = parse_cfg_kind(&kind)?;
            let c = store
                .automation_cfg_add(owner_kind, owner_id, &name, kind, required, options.as_deref())
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.cfg-add", "automation_cfg", serde_json::to_value(&c).unwrap(), None, false, format!("✓ Declared setting: {} ({})", c.name, c.id));
        }
        AutomationCfgCmd::Update { id, name, kind, required, options, clear_options } => {
            let kind = kind.as_deref().map(parse_cfg_kind).transpose()?;
            let options = match clear_options {
                true => Some(None),
                false => options.as_deref().map(Some),
            };
            let c = store.automation_cfg_update(id, name.as_deref(), kind, required, options).map_err(CliError::from)?;
            write_envelope(flags, "automation.cfg-update", "automation_cfg", serde_json::to_value(&c).unwrap(), None, false, format!("✓ Updated setting: {} ({})", c.name, c.id));
        }
        AutomationCfgCmd::Set { step, name, clear, folder, choice, number, text, status, priority, assignee, dim, ready, done, due } => {
            let answer = CfgAnswer { clear, folder, choice, number, text, status, priority, assignee, dim, ready, done, due };
            let value = cfg_value(&answer)?;
            let value = value.map(|v| v.to_string());
            let c = store.automation_cfg_set(step, &name, value.as_deref()).map_err(CliError::from)?;
            let line = match &c.value {
                Some(v) => format!("✓ Answered setting: {} = {v}", c.name),
                None => format!("✓ Left setting unanswered: {}", c.name),
            };
            write_envelope(flags, "automation.cfg-set", "automation_cfg", serde_json::to_value(&c).unwrap(), Some(vec!["value".to_string()]), false, line);
        }
        AutomationCfgCmd::Rm { id } => {
            if !confirm(flags, "delete setting")? {
                return Ok(0);
            }
            store.automation_cfg_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.cfg-rm", "automation_cfg", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted setting: {id}"));
        }
    }
    Ok(0)
}

fn edge(store: &mut Store, flags: &Flags, sub: AutomationEdgeCmd) -> Result<i32, CliError> {
    match sub {
        AutomationEdgeCmd::Add { from, to, done, halt, max_times, no_max } => {
            let (from_step, exit_name) = parse_point(&from)?;
            let target = edge_target(to, done, halt)?;
            // An edge into a step is capped unless somebody says otherwise: what the limit guards
            // against is a loop that never converges, and a caller who never thought about it is the
            // one that loop happens to. An edge that closes or stops the run is taken once, so it
            // carries no limit at all — core refuses one there.
            let max_times = match target {
                EdgeTarget::Go(_) if no_max => None,
                EdgeTarget::Go(_) => Some(max_times.unwrap_or(DEFAULT_MAX_TIMES)),
                _ => max_times,
            };
            let e = store
                .automation_edge_add(from_step, exit_name.as_deref(), target, max_times)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.edge-add", "automation_edge", serde_json::to_value(&e).unwrap(), None, false, format!("✓ Added edge: {} ({})", from, e.id));
        }
        AutomationEdgeCmd::Update { id, to, done, halt, max_times, no_max } => {
            let target = match (to, done, halt) {
                (None, false, false) => None,
                _ => Some(edge_target(to, done, halt)?),
            };
            let max_times = match (max_times, no_max) {
                (_, true) => Some(None),
                (Some(n), false) => Some(Some(n)),
                (None, false) => None,
            };
            let e = store.automation_edge_update(id, target, max_times).map_err(CliError::from)?;
            write_envelope(flags, "automation.edge-update", "automation_edge", serde_json::to_value(&e).unwrap(), None, false, format!("✓ Updated edge: {}", e.id));
        }
        AutomationEdgeCmd::Rm { id } => {
            if !confirm(flags, "delete edge")? {
                return Ok(0);
            }
            store.automation_edge_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.edge-rm", "automation_edge", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted edge: {id}"));
        }
    }
    Ok(0)
}

fn wire(store: &mut Store, flags: &Flags, sub: AutomationWireCmd) -> Result<i32, CliError> {
    match sub {
        AutomationWireCmd::Add { from, from_port, to, to_port } => {
            let (from_step, exit_name) = parse_point(&from)?;
            let w = store
                .automation_wire_add(from_step, exit_name.as_deref(), &from_port, to, &to_port)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.wire-add", "automation_wire", serde_json::to_value(&w).unwrap(), None, false, format!("✓ Added wire: {from}.{from_port} → {to}.{to_port} ({})", w.id));
        }
        AutomationWireCmd::Rm { id } => {
            if !confirm(flags, "delete wire")? {
                return Ok(0);
            }
            store.automation_wire_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.wire-rm", "automation_wire", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted wire: {id}"));
        }
    }
    Ok(0)
}

fn note(store: &mut Store, flags: &Flags, sub: AutomationNoteCmd) -> Result<i32, CliError> {
    match sub {
        AutomationNoteCmd::Add { automation, name, body } => {
            let body = body_arg(body)?;
            let n = store.automation_note_add(automation, &name, &body).map_err(CliError::from)?;
            write_envelope(flags, "automation.note-add", "automation_note", serde_json::to_value(&n).unwrap(), None, false, format!("✓ Added shared document: {} ({})", n.name, n.id));
        }
        AutomationNoteCmd::Update { id, name, body } => {
            let body = body_arg_opt(body)?;
            let n = store.automation_note_update(id, name.as_deref(), body.as_deref()).map_err(CliError::from)?;
            write_envelope(flags, "automation.note-update", "automation_note", serde_json::to_value(&n).unwrap(), None, false, format!("✓ Updated shared document: {} ({})", n.name, n.id));
        }
        AutomationNoteCmd::Rm { id } => {
            if !confirm(flags, "delete shared document")? {
                return Ok(0);
            }
            store.automation_note_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.note-rm", "automation_note", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted shared document: {id}"));
        }
        AutomationNoteCmd::Link { step, note } => {
            let l = store.automation_note_link(step, note).map_err(CliError::from)?;
            write_envelope(flags, "automation.note-link", "automation_step_note", serde_json::to_value(&l).unwrap(), None, false, format!("✓ Step {step} is handed document {note}"));
        }
        AutomationNoteCmd::Unlink { step, note } => {
            let took = store.automation_note_unlink(step, note).map_err(CliError::from)?;
            write_envelope(flags, "automation.note-unlink", "automation_step_note", json!({ "step_id": step, "note_id": note, "unlinked": took }), None, !took, format!("✓ Step {step} is no longer handed document {note}"));
        }
    }
    Ok(0)
}

// ───────────────────────── what ran ─────────────────────────

/// **A run is reached, never searched for.** What a later session asks is how one task was handled, or
/// what an automation has done — both of which start from a record that is already in hand. So there is
/// no listing of every run, and the words a run wrote are not on the word index: the report of a step
/// is reached from the task it was about (`AMB-T-5250`).
fn run(store: &mut Store, flags: &Flags, sub: AutomationRunCmd) -> Result<i32, CliError> {
    match sub {
        AutomationRunCmd::List { task, automation, limit } => {
            let (runs, about) = match (task, automation) {
                (Some(task), None) => {
                    let tid = resolve_task(store, &task).map_err(CliError::from)?;
                    (store.automation_runs_for_task(tid).map_err(CliError::from)?, task_label(tid))
                }
                (None, Some(automation)) => (
                    store.automation_runs_of(automation).map_err(CliError::from)?,
                    format!("automation {automation}"),
                ),
                _ => {
                    return Err(CliError {
                        code: "invalid_value",
                        message: "say which runs — one of --task <id> or --automation <id>.".to_string(),
                        hint: Some("A run is reached from the task it worked or the automation it came from; there is no listing of all of them.".to_string()),
                        exit: 2,
                    })
                }
            };
            let shown: Vec<&AutomationRun> =
                runs.iter().take(limit.unwrap_or(runs.len())).collect();
            if flags.json {
                let mut out = Vec::with_capacity(shown.len());
                for r in &shown {
                    out.push(json!({
                        "run": serde_json::to_value(r).unwrap(),
                        "steps": store.automation_run_steps(r.id).map_err(CliError::from)?.len(),
                    }));
                }
                print_json(&json!({ "count": out.len(), "about": about, "runs": out }));
            } else {
                human(flags, format!("{} run(s) — {about}", shown.len()));
                for r in &shown {
                    let moves = store.automation_run_steps(r.id).map_err(CliError::from)?.len();
                    human(
                        flags,
                        format!(
                            "  run {}  {}  {}  {moves} step(s)",
                            r.id,
                            r.status.as_str(),
                            span(r.started_at, r.ended_at),
                        ),
                    );
                }
            }
        }
        AutomationRunCmd::Show { id } => {
            let run = store
                .automation_run(id)
                .map_err(CliError::from)?
                .ok_or_else(|| {
                    CliError::from(amenbo_core::Error::not_found(format!("run '{id}' not found")))
                })?;
            let defs = store.automation_run_defs(id).map_err(CliError::from)?;
            let stretches = store.automation_run_tasks(id).map_err(CliError::from)?;
            let moves = store.automation_run_steps(id).map_err(CliError::from)?;
            if flags.json {
                let mut walked = Vec::with_capacity(moves.len());
                for m in &moves {
                    walked.push(json!({
                        "step": serde_json::to_value(m).unwrap(),
                        "name": named_step(&defs, m),
                        "values": serde_json::to_value(
                            store.automation_run_values(m.id).map_err(CliError::from)?,
                        )
                        .unwrap(),
                    }));
                }
                print_json(&json!({
                    "run": serde_json::to_value(&run).unwrap(),
                    "tasks": serde_json::to_value(&stretches).unwrap(),
                    "steps": walked,
                }));
            } else {
                render_run(store, flags, &run, &defs, &stretches, &moves)?;
            }
        }
    }
    Ok(0)
}

/// A run's whole story on the terminal: the run's own line, then each task it worked with the steps it
/// spent on that task under it. The order is the order it happened in, which is the only order a run
/// reads in.
fn render_run(
    store: &mut Store,
    flags: &Flags,
    run: &AutomationRun,
    defs: &[AutomationRunDef],
    stretches: &[AutomationRunTask],
    moves: &[AutomationRunStep],
) -> Result<(), CliError> {
    human(flags, format!("Run {}  automation {}", run.id, run.automation_id));
    let stopped = run
        .stopped_reason
        .map(|r| format!(" ({})", r.as_str()))
        .unwrap_or_default();
    human(
        flags,
        format!("status: {}{stopped}  {}", run.status.as_str(), span(run.started_at, run.ended_at)),
    );
    for stretch in stretches {
        let about = match stretch.task_id {
            Some(task_id) => task_label(task_id),
            None => "no task".to_string(),
        };
        human(
            flags,
            format!("\ntask {} — {about}  {}", stretch.seq, span(stretch.started_at, stretch.ended_at)),
        );
        for m in moves.iter().filter(|m| m.run_task_id == Some(stretch.id)) {
            render_move(store, flags, defs, m)?;
        }
    }
    // A step that went looking for a task and found none belongs to no stretch, and is the whole of
    // what the run did — so it is written out rather than left off the account.
    let loose: Vec<&AutomationRunStep> = moves.iter().filter(|m| m.run_task_id.is_none()).collect();
    if !loose.is_empty() {
        human(flags, "\nno task");
        for m in loose {
            render_move(store, flags, defs, m)?;
        }
    }
    Ok(())
}

/// One step execution: which step it was, how it left, how long it stood, what it carried, and the
/// whole of what it said. The report is written out in full — a run read back months later is read for
/// exactly this, and a snippet would send the reader somewhere else to finish the sentence.
fn render_move(
    store: &mut Store,
    flags: &Flags,
    defs: &[AutomationRunDef],
    m: &AutomationRunStep,
) -> Result<(), CliError> {
    human(
        flags,
        format!(
            "  {}. {}  left through {}  {}  [{}]",
            m.seq,
            named_step(defs, m),
            named(m.exit_name.as_deref()),
            span(m.started_at, m.ended_at),
            m.status.as_str(),
        ),
    );
    for v in store.automation_run_values(m.id).map_err(CliError::from)? {
        human(flags, format!("      {}", one_run_value(&v)));
    }
    for line in m.report.lines().filter(|l| !l.trim().is_empty()) {
        human(flags, format!("      | {line}"));
    }
    Ok(())
}

/// How a way out is spoken of in a sentence: by its name, or as the unnamed one. A step still running
/// has taken none yet, which is a third thing and reads as such.
fn named(exit: Option<&str>) -> String {
    match exit {
        Some(amenbo_core::model::ERROR_EXIT) => "the error way out".to_string(),
        Some(name) => format!("\"{name}\""),
        None => "the unnamed way out".to_string(),
    }
}

/// The name the step was launched under, or that the step it ran is gone.
fn named_step(defs: &[AutomationRunDef], m: &AutomationRunStep) -> String {
    defs.iter()
        .find(|d| d.id == m.run_def_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "a step".to_string())
}

/// One value on one line, said from the side it was on: what came in, and what went out.
fn one_run_value(v: &AutomationRunValue) -> String {
    let way = match v.direction {
        amenbo_core::model::AutomationPortDirection::In => "in ",
        amenbo_core::model::AutomationPortDirection::Out => "out",
    };
    let what = match (v.value.as_deref(), v.attachment_id, v.task_id) {
        (Some(text), _, _) => text.to_string(),
        (_, Some(id), _) => format!("AMB-ATT-{id}"),
        (_, _, Some(id)) => task_label(id),
        _ => String::new(),
    };
    let from = v.from_run_step_id.map(|_| " (handed on)").unwrap_or_default();
    format!("{way} {} = {what}{from}", v.name)
}

/// How long something stood, as the two instants it stood between. An end that has not come reads as
/// still standing rather than as a blank: a run under way and a run that ended are different facts, and
/// an empty column says neither.
fn span(from: Option<Timestamp>, to: Option<Timestamp>) -> String {
    match (from, to) {
        (Some(from), Some(to)) => format!("{} → {}", from.to_rfc3339_z(), to.to_rfc3339_z()),
        (Some(from), None) => format!("{} → still going", from.to_rfc3339_z()),
        (None, _) => "not started".to_string(),
    }
}
