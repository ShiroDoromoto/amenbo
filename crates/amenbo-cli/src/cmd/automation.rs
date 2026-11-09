//! `automation`: the library of actions, and the pictures built out of them — placements, the ways out
//! of each one, what runs after each way out is taken, and what is handed along.
//!
//! **Three layers, one word each** (`AMB-D-949`): an automation places actions, an action holds steps,
//! one step is one terminal. Both pictures — the automation's, drawn between placements, and the
//! action's, drawn between its steps — are built with the same `edge` and `wire` verbs, told apart by
//! `--in-action`.
//!
//! **Only the building side is here** — what a run does is written elsewhere. Nothing in this file
//! refuses an unfinished automation, for the reason [`amenbo_core::ops::automation`] gives: the launch
//! check is where a person is about to be let down by one.
//!
//! **The parts are named by id.** They carry no conversational ref: a step is named by the action it
//! sits in, not by a number anybody types back, so every `add` here prints the id the next command
//! takes.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use amenbo_core::model::{
    AutomationCfgKind, AutomationCfgOwner, AutomationOwner, AutomationPortDirection, AutomationPortKind,
    AutomationPortOwner, DEFAULT_MAX_TIMES,
};
use amenbo_core::model::{
    AutomationRun, AutomationRunDef, AutomationRunStatus, AutomationRunStep, AutomationRunTask, AutomationRunValue,
};
use amenbo_core::model::AttachmentTarget;
use amenbo_core::ops::automation_stop::Ending;
use amenbo_core::model::AutomationPictureOwner;
use amenbo_core::model::{AutomationRole, FailuresTo};
use amenbo_core::ops::automation::{lines_back, EdgeTarget, NewAutomation, NewRole, NewScript, NewStep, RoleChange};
use amenbo_core::ops::automation_report::{Next, Produced};
use amenbo_core::ops::automation_run::{
    Carrier, ChosenAt, ForRoom, HandedAtLaunch, Launcher, ReadsFrom, RoleHeadcount, Started,
};
use amenbo_core::ops::automation_stop::{Paused, Resumed};
use amenbo_core::ops::automation_view::{
    ActionView, AutomationView, PlacementView, SavedVersion, Showing, StepView,
};
use amenbo_core::store_engine::read;
use amenbo_core::time::Timestamp;
use amenbo_core::Store;

use crate::cli::*;
use crate::cmd::arg::{body_arg, body_arg_opt};
use crate::cmd::labels::task_label;
use crate::cmd::place::project_or_bound;
use crate::cmd::task::resolve_task;
use crate::output::{confirm, human, print_json, write_envelope, write_envelope_with, CliError, Flags};

/// Where an edge or a wire leaves from, as one token: `<box>:<way out>`. `4` and `4:` are both the
/// done way out, `4:*` the error one.
///
/// **Written as one token because the two halves are one place.** A way out is named against whichever
/// of the box and the action standing on it declares it, so a name without the box it is read on names
/// nothing — and splitting them across two flags lets a caller pass a name that belongs to some other
/// box's list.
fn parse_point(s: &str) -> Result<(i64, Option<String>), CliError> {
    let (whose, exit) = match s.split_once(':') {
        Some((whose, exit)) => (whose, (!exit.is_empty()).then(|| exit.to_string())),
        None => (s, None),
    };
    let whose: i64 = whose.trim().parse().map_err(|_| CliError {
        code: "invalid_value",
        message: format!("'{s}' does not name a box and a way out."),
        hint: Some("Write it as <box>:<way out> — `4:` is the done way out, `4:*` the error one.".to_string()),
        exit: 2,
    })?;
    Ok((whose, exit))
}

/// Which picture a line is drawn on: an automation's, between placements, or one action's, between its
/// steps.
fn picture(in_action: bool) -> AutomationPictureOwner {
    match in_action {
        true => AutomationPictureOwner::Action,
        false => AutomationPictureOwner::Automation,
    }
}

/// **Whose draft a rewrite went into.** A rewrite of a definition lands in its draft, and a launch uses
/// only what is saved — so every rewrite says which draft it touched and the command that saves it.
#[derive(Clone, Copy)]
enum Draft {
    Automation(i64),
    Action(i64),
}

impl Draft {
    fn save_command(self) -> String {
        match self {
            Draft::Automation(id) => format!("amenbo automation save {id}"),
            Draft::Action(id) => format!("amenbo automation action-save {id}"),
        }
    }

    fn json(self) -> Value {
        let (kind, id) = match self {
            Draft::Automation(id) => ("automation", id),
            Draft::Action(id) => ("action", id),
        };
        json!({ "kind": kind, "id": id, "save": self.save_command() })
    }

    fn line(self) -> String {
        let whose = match self {
            Draft::Automation(id) => format!("automation {id}"),
            Draft::Action(id) => format!("action {id}"),
        };
        format!("  in the unsaved draft of {whose} — a launch uses only what is saved: `{}`", self.save_command())
    }

    fn of_picture(owner_kind: AutomationPictureOwner, owner_id: i64) -> Draft {
        match owner_kind {
            AutomationPictureOwner::Automation => Draft::Automation(owner_id),
            AutomationPictureOwner::Action => Draft::Action(owner_id),
        }
    }

    // The lookups below read the rows as they stand, past the reach check: what they find is printed only
    // after the rewrite itself has passed it. A row that is not there leaves the line off, and the
    // rewrite answers `not_found` on its own.

    fn of_placement(store: &Store, id: i64) -> Option<Draft> {
        let p = read::automation_placement(store.read_model().conn(), id).ok().flatten()?;
        Some(Draft::Automation(p.automation_id))
    }

    fn of_role(store: &Store, id: i64) -> Option<Draft> {
        let r = read::automation_role(store.read_model().conn(), id).ok().flatten()?;
        Some(Draft::Automation(r.automation_id))
    }

    fn of_step(store: &Store, id: i64) -> Option<Draft> {
        let s = read::automation_action_step(store.read_model().conn(), id).ok().flatten()?;
        Some(Draft::Action(s.action_id))
    }

    fn of_declarer(store: &Store, owner_kind: AutomationOwner, owner_id: i64) -> Option<Draft> {
        match owner_kind {
            AutomationOwner::Step => Draft::of_step(store, owner_id),
            AutomationOwner::Action => Some(Draft::Action(owner_id)),
        }
    }

    fn of_exit(store: &Store, id: i64) -> Option<Draft> {
        let e = read::automation_exit(store.read_model().conn(), id).ok().flatten()?;
        Draft::of_declarer(store, e.owner_kind, e.owner_id)
    }

    fn of_port_owner(store: &Store, owner_kind: AutomationPortOwner, owner_id: i64) -> Option<Draft> {
        match owner_kind.declarer() {
            Some(declarer) => Draft::of_declarer(store, declarer, owner_id),
            None => Draft::of_exit(store, owner_id),
        }
    }

    fn of_port(store: &Store, id: i64) -> Option<Draft> {
        let p = read::automation_port(store.read_model().conn(), id).ok().flatten()?;
        Draft::of_port_owner(store, p.owner_kind, p.owner_id)
    }

    fn of_cfg_owner(store: &Store, owner_kind: AutomationCfgOwner, owner_id: i64) -> Option<Draft> {
        match owner_kind {
            AutomationCfgOwner::Action => Some(Draft::Action(owner_id)),
            AutomationCfgOwner::Placement => Draft::of_placement(store, owner_id),
        }
    }

    fn of_cfg(store: &Store, id: i64) -> Option<Draft> {
        let c = read::automation_cfg(store.read_model().conn(), id).ok().flatten()?;
        Draft::of_cfg_owner(store, c.owner_kind, c.owner_id)
    }

    fn of_edge(store: &Store, id: i64) -> Option<Draft> {
        let e = read::automation_edge(store.read_model().conn(), id).ok().flatten()?;
        Some(Draft::of_picture(e.owner_kind, e.owner_id))
    }

    fn of_wire(store: &Store, id: i64) -> Option<Draft> {
        let w = read::automation_wire(store.read_model().conn(), id).ok().flatten()?;
        Some(Draft::of_picture(w.owner_kind, w.owner_id))
    }
}

/// [`write_envelope`] for a rewrite of a definition: the envelope, then whose draft it went into — a
/// `draft` key in the JSON, a second line for a person.
fn write_draft(
    flags: &Flags,
    action: &str,
    resource_key: &str,
    resource: Value,
    changed: Option<Vec<String>>,
    human_line: impl AsRef<str>,
    draft: Option<Draft>,
) {
    let extra: Vec<(&str, Value)> = draft.map(|d| ("draft", d.json())).into_iter().collect();
    write_envelope_with(flags, action, resource_key, resource, changed, false, human_line, &extra);
    if let (false, Some(d)) = (flags.json, draft) {
        human(flags, d.line());
    }
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
            hint: Some("A way out and a port are a step's or an action's; a setting is the action's alone.".to_string()),
            exit: 2,
        }),
    }
}

/// Where an edge goes, from the four flags that say so. `--to` names the next box; `--exit-to` leaves
/// the action the picture is inside, bare for its done way out; the other two end the run.
fn edge_target(to: Option<i64>, exit_to: Option<String>, done: bool, halt: bool) -> Result<EdgeTarget, CliError> {
    match (to, exit_to, done, halt) {
        (Some(to), None, false, false) => Ok(EdgeTarget::Go(to)),
        (None, Some(name), false, false) => Ok(EdgeTarget::Exit((!name.is_empty()).then_some(name))),
        (None, None, true, false) => Ok(EdgeTarget::Done),
        (None, None, false, true) => Ok(EdgeTarget::Halt),
        _ => Err(CliError {
            code: "invalid_value",
            message: "say what happens after this way out — one of --to <box>, --exit-to [name], --done or --halt.".to_string(),
            hint: None,
            exit: 2,
        }),
    }
}

/// The answer to a setting, in the shape its kind takes. **A task filter is never one string**: it is
/// built from the options that name each part, where the same option twice is any-of and two different
/// options are both — which is the same reading `--filter` gives a `key:a,b key2:c` expression, and it
/// is validated by parsing that expression before the answer is written. The order its tasks are taken
/// in (`--sort`) rides beside the parts, checked against the keys `task list --sort` takes.
///
/// What the JSON's own type says is which kind answered it: an object for a task filter, a number for a
/// number, a string for the other three. Nothing here reads the declaration — no command lists one — so
/// an answer in the wrong shape is caught when it is written (`ops::automation::answer_misfit`), where
/// the declaration is at hand.
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
        if !single.is_empty() || !filtered.is_empty() || o.sort.is_some() {
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
    if o.sort.is_some() && filtered.is_empty() {
        // An order is the order of a task filter's tasks, so it goes with the parts that say which
        // tasks — alone it narrows nothing, and beside another kind's answer it means nothing.
        return Err(CliError {
            code: "invalid_value",
            message: "--sort orders a task filter, so it takes the task-filter options beside it.".to_string(),
            hint: Some("Pass --status, --priority, --assignee, --dim, --ready, --done or --due with --sort.".to_string()),
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
    let mut map = serde_json::Map::new();
    for (k, v) in filtered {
        map.insert(k.to_string(), json!(v));
    }
    if let Some(sort) = &o.sort {
        if !read::is_task_sort(sort) {
            return Err(CliError {
                code: "invalid_value",
                message: format!("'{sort}' is not an order `task list --sort` takes."),
                hint: Some("Specify one of: order | due | priority | created | completed | title (- for descending).".to_string()),
                exit: 2,
            });
        }
        map.insert(amenbo_core::ops::automation_step::TASKFILTER_SORT_KEY.to_string(), json!(sort));
    }
    let value = Value::Object(map);
    // Read as the filter it will be run as — the same expression the step's prompt spells — so a value
    // nothing accepts is refused while the person who wrote it is still here, rather than at the launch
    // of a run, days later. Whether the axes and values it names are there is looked up when it is
    // written (`ops::automation::cfg_set`), where the store is at hand.
    let expr = amenbo_core::ops::automation_step::taskfilter_expr(&value.to_string()).unwrap_or_default();
    amenbo_core::query::Filter::parse(&expr, amenbo_core::time::today()).map_err(CliError::from)?;
    Ok(Some(value))
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
    sort: Option<String>,
}

pub(crate) fn automation(store: &mut Store, flags: &Flags, sub: AutomationCmd) -> Result<i32, CliError> {
    // A step's terminal types every command as it is typed outside a run (`AMB-D-1011`). A step's own
    // verb typed where there is no step is refused by `speaking_for`, in the arms that read the step
    // off it.
    match sub {
        AutomationCmd::Add { project, name, notes, failures_to } => {
            let pid = project_or_bound(store, project)?;
            let failures_to = failures_to.as_deref().map(failures_to_arg).transpose()?;
            let new = NewAutomation { name, notes: body_arg(notes)?, failures_to };
            let a = store.automation_add(pid, new).map_err(CliError::from)?;
            write_envelope(flags, "automation.add", "automation", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Created automation: {} ({})", a.name, a.id));
        }
        AutomationCmd::List { project } => {
            let pid = project_or_bound(store, project)?;
            let cards = store.automations(pid).map_err(CliError::from)?;
            if flags.json {
                print_json(&json!({
                    "count": cards.len(),
                    "project_id": pid,
                    "automations": serde_json::to_value(&cards).unwrap(),
                }));
            } else {
                human(flags, format!("{} automation(s) — project {pid}", cards.len()));
                for card in &cards {
                    let archived = if card.automation.archived { "  archived" } else { "" };
                    let export_only = if card.automation.export_only { "  export only" } else { "" };
                    let stopped = if card.automation.stopped { "  stopped" } else { "" };
                    human(
                        flags,
                        format!(
                            "  {}  {}  {} placement(s){export_only}{stopped}{archived}",
                            card.automation.id, card.automation.name, card.placements
                        ),
                    );
                }
            }
        }
        AutomationCmd::Show { id, saved } => {
            let view = if saved { store.automation_saved_detail(id) } else { store.automation_detail(id) };
            let view = view.map_err(CliError::from)?.ok_or_else(|| {
                CliError::from(amenbo_core::Error::not_found(format!(
                    "automation '{id}' not found"
                )))
            })?;
            if flags.json {
                print_json(&serde_json::to_value(&view).unwrap());
            } else {
                // Who carries each step out, as a launch would choose it: the step's own choice, else
                // its role's default, else Amenbo's.
                let reads = if saved { ReadsFrom::Saved } else { ReadsFrom::Draft };
                let default_agent = store.automation_default_agent(id).map_err(CliError::from)?;
                let carriers = amenbo_core::ops::automation_run::carriers(
                    store.read_model().conn(),
                    id,
                    reads,
                    default_agent.as_deref(),
                )
                .map_err(CliError::from)?;
                render_automation(flags, &view, &carriers);
            }
        }
        AutomationCmd::Export { id, out, force } => {
            let dest = std::path::PathBuf::from(&out);
            if dest.exists() && !force {
                return Err(CliError {
                    code: "file_exists",
                    message: format!("{} already exists", dest.display()),
                    hint: Some("pass --force to overwrite".to_string()),
                    exit: 1,
                });
            }
            let file = store.automation_export(id).map_err(CliError::from)?;
            let body = serde_json::to_string_pretty(&file).unwrap();
            std::fs::write(&dest, body + "\n").map_err(|e: std::io::Error| CliError {
                code: "io_error",
                message: format!("cannot write {}: {e}", dest.display()),
                hint: None,
                exit: 1,
            })?;
            write_envelope(
                flags,
                "automation.export",
                "automation",
                json!({ "id": id, "name": file.name, "format": file.format, "out": dest.display().to_string() }),
                None,
                false,
                format!("✓ Wrote automation {} ({id}) → {}", file.name, dest.display()),
            );
        }
        AutomationCmd::Import { file, project } => {
            let pid = project_or_bound(store, project)?;
            let src = std::path::PathBuf::from(&file);
            let text = std::fs::read_to_string(&src).map_err(|e: std::io::Error| CliError {
                code: "io_error",
                message: format!("cannot read {}: {e}", src.display()),
                hint: None,
                exit: 1,
            })?;
            let parsed = amenbo_core::ops::automation_import::parse(&text).map_err(CliError::from)?;
            let a = store.automation_import(pid, &parsed).map_err(CliError::from)?;
            write_envelope(
                flags,
                "automation.import",
                "automation",
                serde_json::to_value(&a).unwrap(),
                None,
                false,
                format!("✓ Imported automation: {} ({}) from {}", a.name, a.id, src.display()),
            );
        }
        AutomationCmd::Update {
            id,
            name,
            notes,
            archived,
            failures_to,
            no_failures_to,
        } => {
            let notes = body_arg_opt(notes)?;
            let failures_to = match (no_failures_to, failures_to.as_deref()) {
                (true, _) => Some(None),
                (false, Some(to)) => Some(Some(failures_to_arg(to)?)),
                (false, None) => None,
            };
            let a = store
                .automation_update(id, name.as_deref(), notes.as_deref(), archived, failures_to, None)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.update", "automation", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Updated automation: {} ({})", a.name, a.id));
        }
        AutomationCmd::Trigger {
            id,
            trigger: trigger_id,
            every,
            when_idle,
            if_tasks,
            fewer_than,
            if_tasks_role,
            if_script,
            script_args,
            script_timeout_minutes,
            title,
            notes,
            task_from_file,
            task_from_script,
            task_script_args,
            task_script_timeout_minutes,
            name,
            role,
            rm,
            on,
            off,
        } => {
            use amenbo_core::model::{TriggerCondition, TriggerWhen};
            use amenbo_core::ops::automation::{NewTrigger, NewTriggerTask};
            // clap takes the automation or --trigger, never both, and --rm, --on and --off only with
            // --trigger.
            let target = match (id, trigger_id) {
                (_, Some(trigger_id)) => trigger_id,
                (Some(automation_id), None) => automation_id,
                (None, None) => unreachable!("clap asks for the automation or --trigger"),
            };
            if rm {
                if !confirm(flags, "take trigger away")? {
                    return Ok(0);
                }
                store.automation_trigger_delete(target).map_err(CliError::from)?;
                write_envelope(flags, "automation.trigger", "automation_trigger", json!({ "id": target, "deleted": true }), None, false, format!("✓ Took trigger away: {target}"));
                return Ok(0);
            }
            if on || off {
                let t = store.automation_trigger_set_enabled(target, on).map_err(CliError::from)?;
                let line = format!("✓ Switched a trigger of automation {} — {}", t.automation_id, trigger_line(&t));
                write_envelope(flags, "automation.trigger", "automation_trigger", serde_json::to_value(&t).unwrap(), None, false, line);
                return Ok(0);
            }
            let condition = match (if_tasks, fewer_than) {
                (Some(filter), Some(below)) => Some(TriggerCondition { filter, role_id: if_tasks_role, below }),
                // clap asks for both or neither.
                _ => None,
            };
            let script = if_script.map(|program| NewScript {
                program,
                args: script_args,
                timeout_minutes: script_timeout_minutes,
            });
            let when = match (every, when_idle, &condition, &script) {
                (Some(minutes), _, _, _) => TriggerWhen::Every { minutes },
                (None, true, _, _) => TriggerWhen::Idle,
                (None, false, Some(_), _) => TriggerWhen::Tasks,
                (None, false, None, Some(_)) => TriggerWhen::Script,
                (None, false, None, None) => unreachable!("clap asks for a when unless --rm, --on or --off"),
            };
            let task = match (title, task_from_file, task_from_script) {
                (Some(title), _, _) => NewTriggerTask::Fixed { title, notes: body_arg_opt(notes)?.unwrap_or_default() },
                (None, Some(path), _) => NewTriggerTask::File { path },
                (None, None, Some(program)) => NewTriggerTask::Script(NewScript {
                    program,
                    args: task_script_args,
                    timeout_minutes: task_script_timeout_minutes,
                }),
                (None, None, None) => unreachable!("clap asks for the task unless --rm, --on or --off"),
            };
            let new = NewTrigger { name: name.unwrap_or_default(), role_id: role, when, condition, script, task };
            let (t, done) = match trigger_id {
                Some(trigger_id) => (store.automation_trigger_update(trigger_id, new).map_err(CliError::from)?, "Rewrote a trigger of"),
                None => (store.automation_trigger_add(target, new).map_err(CliError::from)?, "Added a trigger to"),
            };
            let line = format!("✓ {done} automation {} — {}", t.automation_id, trigger_line(&t));
            write_envelope(flags, "automation.trigger", "automation_trigger", serde_json::to_value(&t).unwrap(), None, false, line);
        }
        AutomationCmd::Stop { id, off } => {
            let done = store.automation_set_stopped(id, !off).map_err(CliError::from)?;
            let a = &done.automation;
            let line = match (a.stopped, done.paused.is_empty()) {
                (false, _) => format!("✓ Automation {} ({}) starts new rounds again", a.name, a.id),
                (true, true) => format!("✓ Stopped automation {} ({}) — it starts no new round", a.name, a.id),
                (true, false) => format!(
                    "✓ Stopped automation {} ({}) — it starts no new round; paused run(s) waiting for room to take a task: {}",
                    a.name,
                    a.id,
                    done.paused.iter().map(|r| r.id.to_string()).collect::<Vec<_>>().join(", ")
                ),
            };
            let paused: Vec<i64> = done.paused.iter().map(|r| r.id).collect();
            let mut resource = serde_json::to_value(a).unwrap();
            resource["paused_runs"] = json!(paused);
            write_envelope(flags, "automation.stop", "automation", resource, None, false, line);
        }
        AutomationCmd::Rm { id } => {
            if !confirm(flags, "delete automation")? {
                return Ok(0);
            }
            store.automation_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.rm", "automation", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted automation: {id}"));
        }
        AutomationCmd::Save { id } => {
            // Core hands the newest version straight back when nothing was written since it, so whether
            // this call saved anything is asked before it.
            let unsaved = store.automation_unsaved(id).map_err(CliError::from)?;
            let v = store.automation_save(id).map_err(CliError::from)?;
            let line = match unsaved {
                true => format!("✓ Saved automation {id} as version {}", v.version),
                false => format!("✓ Automation {id} has nothing unsaved — version {} stands", v.version),
            };
            let resource = json!({ "id": v.id, "automation_id": v.automation_id, "version": v.version });
            write_envelope(flags, "automation.save", "automation_version", resource, None, !unsaved, line);
        }
        AutomationCmd::Discard { id } => {
            if !confirm(flags, "throw away what is unsaved on automation")? {
                return Ok(0);
            }
            // Asked before, for the reason `save` asks it.
            let unsaved = store.automation_unsaved(id).map_err(CliError::from)?;
            let v = store.automation_discard(id).map_err(CliError::from)?;
            let line = match unsaved {
                true => format!("✓ Put automation {id} back to version {}", v.version),
                false => format!("✓ Automation {id} has nothing unsaved — version {} stands", v.version),
            };
            let resource = json!({ "id": v.id, "automation_id": v.automation_id, "version": v.version });
            write_envelope(flags, "automation.discard", "automation_version", resource, None, !unsaved, line);
        }
        AutomationCmd::PlaceAdd { automation, action, builtin, axis } => {
            // clap holds exactly one of the two: `--action` is required unless `--builtin` is given.
            let (p, what) = match (action, builtin) {
                (_, Some(key)) => {
                    let axis = match axis {
                        Some(axis) => Some(store.resolve_dimension(None, &axis).map_err(CliError::from)?),
                        None => None,
                    };
                    let p = store.automation_builtin_place(automation, &key, axis).map_err(CliError::from)?;
                    (p, format!("the built-in '{key}'"))
                }
                (Some(action), None) => {
                    // clap cannot hold this one: it skips `requires` for an argument whose counterpart
                    // conflicts with one that is there, so `--axis` would be dropped without a word.
                    if axis.is_some() {
                        return Err(CliError::from(amenbo_core::Error::invalid(
                            "--axis names the axis the built-in 'split_by_dim' splits by — a library action takes none",
                        )));
                    }
                    (store.automation_placement_add(automation, action).map_err(CliError::from)?, format!("action {action}"))
                }
                (None, None) => unreachable!("clap requires --action or --builtin"),
            };
            write_draft(flags, "automation.place-add", "automation_placement", serde_json::to_value(&p).unwrap(), None, format!("✓ Placed {what} on automation {automation} ({})", p.id), Some(Draft::Automation(p.automation_id)));
        }
        AutomationCmd::PlaceRm { id } => {
            if !confirm(flags, "take placement off")? {
                return Ok(0);
            }
            let draft = Draft::of_placement(store, id);
            store.automation_placement_delete(id).map_err(CliError::from)?;
            write_draft(flags, "automation.place-rm", "automation_placement", json!({ "id": id, "deleted": true }), None, format!("✓ Took placement off: {id}"), draft);
        }
        AutomationCmd::PlaceVersion { placement, version } => {
            let p = store.automation_placement_version_set(placement, version).map_err(CliError::from)?;
            let line = format!("✓ Placement {} stands on version {version} of action {}", p.id, p.action_id);
            write_draft(flags, "automation.place-version", "automation_placement", serde_json::to_value(&p).unwrap(), Some(vec!["version".to_string()]), line, Some(Draft::Automation(p.automation_id)));
        }

        AutomationCmd::ActionAdd { project, global, name, note } => {
            // The device's library is reached by every project on this machine, so it is nobody's
            // project to put something in: `None` is what core reads as that shelf, and an AI bound to
            // a project is turned away from it there.
            let pid = match global {
                true => None,
                false => Some(project_or_bound(store, project)?),
            };
            let note = body_arg(note)?;
            let a = store.automation_action_add(pid, &name, &note).map_err(CliError::from)?;
            write_envelope(flags, "automation.action-add", "automation_action", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Added action: {} ({})", a.name, a.id));
        }
        AutomationCmd::ActionList { project, global } => {
            // The device's shelf is reached from every project, so `--global` is a narrowing rather
            // than another place to look: without it the two shelves answer as the one list a step
            // here could be pointed at.
            let pid = match global {
                true => None,
                false => Some(project_or_bound(store, project)?),
            };
            let cards = store.automation_actions(pid).map_err(CliError::from)?;
            if flags.json {
                print_json(&json!({
                    "count": cards.len(),
                    "project_id": pid,
                    "actions": serde_json::to_value(&cards).unwrap(),
                }));
            } else {
                let about = match pid {
                    Some(pid) => format!("the device's library and project {pid}"),
                    None => "the device's library".to_string(),
                };
                human(flags, format!("{} action(s) — {about}", cards.len()));
                for card in &cards {
                    let shelf = if card.action.project_id.is_none() { "  [device]" } else { "" };
                    let draft = if card.action.draft { "  [being created]" } else { "" };
                    human(
                        flags,
                        format!(
                            "  {}  {}{shelf}{draft}  {} step(s)  used by {} automation(s)",
                            card.action.id, card.action.name, card.steps, card.used_by
                        ),
                    );
                }
            }
        }
        AutomationCmd::ActionShow { id, saved } => {
            let view = if saved {
                store.automation_action_saved_detail(id)
            } else {
                store.automation_action_detail(id)
            };
            let view = view.map_err(CliError::from)?.ok_or_else(|| {
                CliError::from(amenbo_core::Error::not_found(format!("action '{id}' not found")))
            })?;
            if flags.json {
                print_json(&serde_json::to_value(&view).unwrap());
            } else {
                render_action(flags, &view);
            }
        }
        AutomationCmd::BuiltinList => {
            let builtins = amenbo_core::ops::automation_builtin::all();
            if flags.json {
                print_json(&json!({
                    "count": builtins.len(),
                    "builtins": serde_json::to_value(builtins).unwrap(),
                }));
            } else {
                human(flags, format!("{} built-in(s)", builtins.len()));
                for one in builtins {
                    human(flags, format!("  {}  {} — {}", one.key, one.name, one.does));
                }
            }
        }
        AutomationCmd::ActionUpdate { id, name, note, one_at_a_time } => {
            let note = body_arg_opt(note)?;
            let one_at_a_time = one_at_a_time.map(|v| v == "on");
            // A name and a note hold at once; whether it takes one run at a time is kept in a version,
            // so that one goes into the draft and the answer says so. Two writes, the flag's last.
            if one_at_a_time.is_none() || name.is_some() || note.is_some() {
                let a = store
                    .automation_action_update(id, name.as_deref(), note.as_deref())
                    .map_err(CliError::from)?;
                if one_at_a_time.is_none() {
                    write_envelope(flags, "automation.action-update", "automation_action", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Updated action: {} ({})", a.name, a.id));
                }
            }
            if let Some(on) = one_at_a_time {
                let a = store.automation_action_set_one_at_a_time(id, on).map_err(CliError::from)?;
                let line = match a.one_at_a_time {
                    true => format!("✓ Action {} takes one run at a time", a.id),
                    false => format!("✓ Action {} lets runs in side by side", a.id),
                };
                write_draft(flags, "automation.action-update", "automation_action", serde_json::to_value(&a).unwrap(), Some(vec!["one_at_a_time".to_string()]), line, Some(Draft::Action(a.id)));
            }
        }
        AutomationCmd::ActionEntrySet { id, step, clear } => {
            if step.is_none() && !clear {
                return Err(CliError {
                    code: "invalid_value",
                    message: "name the step this action opens first with --step, or --clear to leave none.".to_string(),
                    hint: None,
                    exit: 2,
                });
            }
            let a = store.automation_action_set_entry(id, step).map_err(CliError::from)?;
            let line = match a.entry_step_id {
                Some(s) => format!("✓ Action {} opens step {s} first", a.id),
                None => format!("✓ Action {} opens nothing", a.id),
            };
            write_draft(flags, "automation.action-entry-set", "automation_action", serde_json::to_value(&a).unwrap(), Some(vec!["entry_step_id".to_string()]), line, Some(Draft::Action(a.id)));
        }
        AutomationCmd::ActionScopeSet { id, project, global } => {
            // The same two shelves `action-add` puts an action on, read the same way. Core declares
            // both ends, so an AI bound to a project is turned away whichever way the action moves.
            let pid = match global {
                true => None,
                false => Some(project_or_bound(store, project)?),
            };
            let a = store.automation_action_set_scope(id, pid).map_err(CliError::from)?;
            let line = match a.project_id {
                Some(p) => format!("✓ Action {} is in the library of project {p}", a.id),
                None => format!("✓ Action {} is in the device's library", a.id),
            };
            write_envelope(flags, "automation.action-scope-set", "automation_action", serde_json::to_value(&a).unwrap(), Some(vec!["project_id".to_string()]), false, line);
        }
        AutomationCmd::ActionRm { id } => {
            if !confirm(flags, "delete library action")? {
                return Ok(0);
            }
            store.automation_action_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.action-rm", "automation_action", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted action: {id}"));
        }
        AutomationCmd::ActionSave { id } => {
            // Asked before, for the reason `save` asks it — and whether it is still being created, since
            // core finishes that on the newest version even with nothing to save.
            let saved = store.automation_action_unsaved(id).map_err(CliError::from)?;
            let was_draft = store
                .automation_action_detail(id)
                .map_err(CliError::from)?
                .is_some_and(|view| view.action.draft);
            let v = store.automation_action_save(id).map_err(CliError::from)?;
            let line = match (saved, was_draft) {
                (true, true) => format!("✓ Saved action {id} as version {} and finished creating it", v.version),
                (true, false) => format!("✓ Saved action {id} as version {}", v.version),
                (false, true) => format!("✓ Finished creating action {id} on version {}", v.version),
                (false, false) => format!("✓ Action {id} has nothing unsaved — version {} stands", v.version),
            };
            let resource = json!({ "id": v.id, "action_id": v.action_id, "version": v.version });
            write_envelope(flags, "automation.action-save", "automation_action_version", resource, None, !saved && !was_draft, line);
        }
        AutomationCmd::ActionAbandon { id } => {
            if !confirm(flags, "give up this action and the placements standing on it")? {
                return Ok(0);
            }
            store.automation_action_abandon(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.action-abandon", "automation_action", json!({ "id": id, "abandoned": true }), None, false, format!("✓ Gave up action: {id}"));
        }
        AutomationCmd::ActionDiscard { id } => {
            if !confirm(flags, "throw away what is unsaved in action")? {
                return Ok(0);
            }
            // Asked before, for the reason `save` asks it.
            let unsaved = store.automation_action_unsaved(id).map_err(CliError::from)?;
            let v = store.automation_action_discard(id).map_err(CliError::from)?;
            let line = match unsaved {
                true => format!("✓ Put action {id} back to version {}", v.version),
                false => format!("✓ Action {id} has nothing unsaved — version {} stands", v.version),
            };
            let resource = json!({ "id": v.id, "action_id": v.action_id, "version": v.version });
            write_envelope(flags, "automation.action-discard", "automation_action_version", resource, None, !unsaved, line);
        }
        AutomationCmd::StepAdd { action, name, prompt, program, args, timeout_minutes, interactive, work_dir, report_to_task, no_history, no_task_notes, no_task_decisions, no_task_comments, expected_minutes } => {
            // A script step runs its program, not a prompt, so it may be written without one.
            let prompt = body_arg_opt(prompt)?.unwrap_or_default();
            let new = NewStep {
                name,
                prompt,
                interactive,
                work_dir_ref: work_dir,
                report_to_task,
                // The run's story so far is handed on unless somebody turns it off, so the flag that
                // takes it away is the one written.
                show_history: !no_history,
                // So is the task the run is on — its notes, its decisions and its comments, each on its own.
                show_notes: !no_task_notes,
                show_decisions: !no_task_decisions,
                show_comments: !no_task_comments,
                expected_minutes,
                script: program.map(|program| NewScript { program, args, timeout_minutes }),
            };
            let s = store.automation_step_add(action, new).map_err(CliError::from)?;
            write_draft(flags, "automation.step-add", "automation_step", serde_json::to_value(&s).unwrap(), None, format!("✓ Added step: {} ({})", s.name, s.id), Some(Draft::Action(s.action_id)));
        }
        AutomationCmd::StepUpdate { id, name, prompt, interactive, work_dir, clear_work_dir, report_to_task, history, task_notes, task_decisions, task_comments, program, args, timeout_minutes, clear_script, expected_minutes, clear_expected_minutes } => {
            let prompt = body_arg_opt(prompt)?;
            let work_dir = match clear_work_dir {
                true => Some(None),
                false => work_dir.as_deref().map(Some),
            };
            // Core writes a script whole, so `--program` carries the arguments and the timeout with it.
            let script = match clear_script {
                true => Some(None),
                false => program.map(|program| Some(NewScript { program, args, timeout_minutes })),
            };
            let expected_minutes = match clear_expected_minutes {
                true => Some(None),
                false => expected_minutes.map(Some),
            };
            let s = store
                .automation_step_update(id, name.as_deref(), prompt.as_deref(), interactive, work_dir, report_to_task, history, task_notes, task_decisions, task_comments, script, expected_minutes)
                .map_err(CliError::from)?;
            write_draft(flags, "automation.step-update", "automation_step", serde_json::to_value(&s).unwrap(), None, format!("✓ Updated step: {} ({})", s.name, s.id), Some(Draft::Action(s.action_id)));
        }
        AutomationCmd::StepRm { id } => {
            if !confirm(flags, "delete step")? {
                return Ok(0);
            }
            let draft = Draft::of_step(store, id);
            store.automation_step_delete(id).map_err(CliError::from)?;
            write_draft(flags, "automation.step-rm", "automation_step", json!({ "id": id, "deleted": true }), None, format!("✓ Deleted step: {id}"), draft);
        }
        AutomationCmd::ExitAdd { step, action, name } => {
            let (owner_kind, owner_id) = declarer_from_flags(step, action)?;
            let e = store.automation_exit_add(owner_kind, owner_id, Some(&name)).map_err(CliError::from)?;
            write_draft(flags, "automation.exit-add", "automation_exit", serde_json::to_value(&e).unwrap(), None, format!("✓ Added way out: {} ({})", name, e.id), Draft::of_declarer(store, e.owner_kind, e.owner_id));
        }
        AutomationCmd::ExitRename { id, name } => {
            let e = store.automation_exit_rename(id, Some(&name)).map_err(CliError::from)?;
            write_draft(flags, "automation.exit-rename", "automation_exit", serde_json::to_value(&e).unwrap(), Some(vec!["name".to_string()]), format!("✓ Renamed way out: {name} ({})", e.id), Draft::of_declarer(store, e.owner_kind, e.owner_id));
        }
        AutomationCmd::ExitRm { id } => {
            if !confirm(flags, "delete way out")? {
                return Ok(0);
            }
            let draft = Draft::of_exit(store, id);
            store.automation_exit_delete(id).map_err(CliError::from)?;
            write_draft(flags, "automation.exit-rm", "automation_exit", json!({ "id": id, "deleted": true }), None, format!("✓ Deleted way out: {id}"), draft);
        }
        AutomationCmd::PortAdd { step, action, exit, name, kind, required } => {
            let kind = parse_port_kind(&kind)?;
            // The direction falls out of the owner and is never asked for: an input belongs to the step
            // or the action that reads it, an output to the way out that produced it, and neither is
            // sayable on the other's.
            let (owner_kind, owner_id, direction) = match (step, action, exit) {
                (Some(id), None, None) => (AutomationPortOwner::Step, id, AutomationPortDirection::In),
                (None, Some(id), None) => (AutomationPortOwner::Action, id, AutomationPortDirection::In),
                (None, None, Some(id)) => (AutomationPortOwner::Exit, id, AutomationPortDirection::Out),
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
            write_draft(flags, "automation.port-add", "automation_port", serde_json::to_value(&p).unwrap(), None, format!("✓ Added {} port: {} ({})", direction.as_str(), p.name, p.id), Draft::of_port_owner(store, p.owner_kind, p.owner_id));
        }
        AutomationCmd::PortUpdate { id, name, kind, required } => {
            let kind = kind.as_deref().map(parse_port_kind).transpose()?;
            let p = store.automation_port_update(id, name.as_deref(), kind, required).map_err(CliError::from)?;
            write_draft(flags, "automation.port-update", "automation_port", serde_json::to_value(&p).unwrap(), None, format!("✓ Updated port: {} ({})", p.name, p.id), Draft::of_port_owner(store, p.owner_kind, p.owner_id));
        }
        AutomationCmd::PortRm { id } => {
            if !confirm(flags, "delete port")? {
                return Ok(0);
            }
            let draft = Draft::of_port(store, id);
            store.automation_port_delete(id).map_err(CliError::from)?;
            write_draft(flags, "automation.port-rm", "automation_port", json!({ "id": id, "deleted": true }), None, format!("✓ Deleted port: {id}"), draft);
        }
        AutomationCmd::CfgAdd { action, name, kind, required, options } => {
            let kind = parse_cfg_kind(&kind)?;
            let c = store
                .automation_cfg_add(action, &name, kind, required, options.as_deref())
                .map_err(CliError::from)?;
            write_draft(flags, "automation.cfg-add", "automation_cfg", serde_json::to_value(&c).unwrap(), None, format!("✓ Declared setting: {} ({})", c.name, c.id), Draft::of_cfg_owner(store, c.owner_kind, c.owner_id));
        }
        AutomationCmd::CfgUpdate { id, name, kind, required, options, clear_options } => {
            let kind = kind.as_deref().map(parse_cfg_kind).transpose()?;
            let options = match clear_options {
                true => Some(None),
                false => options.as_deref().map(Some),
            };
            let c = store.automation_cfg_update(id, name.as_deref(), kind, required, options).map_err(CliError::from)?;
            write_draft(flags, "automation.cfg-update", "automation_cfg", serde_json::to_value(&c).unwrap(), None, format!("✓ Updated setting: {} ({})", c.name, c.id), Draft::of_cfg_owner(store, c.owner_kind, c.owner_id));
        }
        AutomationCmd::CfgSet { placement, name, clear, folder, choice, number, text, filter } => {
            let TaskFilterArgs { status, priority, assignee, dim, ready, done, due, sort } = *filter;
            let answer = CfgAnswer { clear, folder, choice, number, text, status, priority, assignee, dim, ready, done, due, sort };
            let value = cfg_value(&answer)?;
            let value = value.map(|v| v.to_string());
            let c = store.automation_cfg_set(placement, &name, value.as_deref()).map_err(CliError::from)?;
            let line = match &c.value {
                Some(v) => format!("✓ Answered setting: {} = {v}", c.name),
                None => format!("✓ Left setting unanswered: {}", c.name),
            };
            write_draft(flags, "automation.cfg-set", "automation_cfg", serde_json::to_value(&c).unwrap(), Some(vec!["value".to_string()]), line, Draft::of_placement(store, placement));
        }
        AutomationCmd::AgentSet { placement, step, agent, model, clear } => {
            // clap holds `--agent` to be there unless `--clear` is, and the two apart.
            match agent.filter(|_| !clear) {
                Some(agent) => {
                    let c = store
                        .automation_placement_step_set(placement, step, &agent, model.as_deref())
                        .map_err(CliError::from)?;
                    let line = match &c.model {
                        Some(model) => format!("✓ Chose {} ({model}) for step {step} at placement {placement}", c.agent),
                        None => format!("✓ Chose {} for step {step} at placement {placement}", c.agent),
                    };
                    write_draft(flags, "automation.agent-set", "automation_placement_step", serde_json::to_value(&c).unwrap(), None, line, Draft::of_placement(store, placement));
                }
                None => {
                    store.automation_placement_step_clear(placement, step).map_err(CliError::from)?;
                    write_draft(
                        flags,
                        "automation.agent-set",
                        "automation_placement_step",
                        json!({ "placement_id": placement, "step_id": step, "cleared": true }),
                        None,
                        format!("✓ Left nobody chosen for step {step} at placement {placement}"),
                        Draft::of_placement(store, placement),
                    );
                }
            }
        }
        AutomationCmd::RoleAdd { automation, name, headcount, icon, entry, agent, model } => {
            let new = NewRole { name, headcount, take_order: None, icon, entry_placement_id: entry, agent, model };
            let r = store.automation_role_add(automation, new).map_err(CliError::from)?;
            let line = format!("✓ Defined role {} in automation {automation} ({})", r.name, r.id);
            write_draft(flags, "automation.role-add", "automation_role", serde_json::to_value(&r).unwrap(), None, line, Some(Draft::Automation(r.automation_id)));
        }
        AutomationCmd::RoleList { automation } => {
            let view = store.automation_detail(automation).map_err(CliError::from)?.ok_or_else(|| {
                CliError::from(amenbo_core::Error::not_found(format!("automation '{automation}' not found")))
            })?;
            if flags.json {
                print_json(&json!({
                    "count": view.roles.len(),
                    "automation_id": automation,
                    "roles": serde_json::to_value(&view.roles).unwrap(),
                }));
            } else {
                human(flags, format!("{} role(s) — automation {automation}", view.roles.len()));
                for role in &view.roles {
                    human(flags, format!("  {}", one_role(role)));
                }
            }
        }
        AutomationCmd::RoleUpdate {
            id,
            name,
            headcount,
            icon,
            clear_icon,
            entry,
            clear_entry,
            agent,
            clear_agent,
            model,
            clear_model,
        } => {
            // clap holds each value and its `--clear-…` apart, so at most one of the two is there.
            let change = RoleChange {
                name,
                headcount,
                take_order: None,
                icon: if clear_icon { Some(None) } else { icon.map(Some) },
                entry_placement_id: if clear_entry { Some(None) } else { entry.map(Some) },
                agent: if clear_agent { Some(None) } else { agent.map(Some) },
                model: if clear_model { Some(None) } else { model.map(Some) },
            };
            let r = store.automation_role_update(id, change).map_err(CliError::from)?;
            let line = format!("✓ Updated role: {} ({})", r.name, r.id);
            write_draft(flags, "automation.role-update", "automation_role", serde_json::to_value(&r).unwrap(), None, line, Some(Draft::Automation(r.automation_id)));
        }
        AutomationCmd::RoleRm { id } => {
            if !confirm(flags, "take role out")? {
                return Ok(0);
            }
            let draft = Draft::of_role(store, id);
            store.automation_role_delete(id).map_err(CliError::from)?;
            write_draft(flags, "automation.role-rm", "automation_role", json!({ "id": id, "deleted": true }), None, format!("✓ Took role out: {id}"), draft);
        }
        AutomationCmd::CfgRm { id } => {
            if !confirm(flags, "delete setting")? {
                return Ok(0);
            }
            let draft = Draft::of_cfg(store, id);
            store.automation_cfg_delete(id).map_err(CliError::from)?;
            write_draft(flags, "automation.cfg-rm", "automation_cfg", json!({ "id": id, "deleted": true }), None, format!("✓ Deleted setting: {id}"), draft);
        }
        AutomationCmd::EdgeAdd { in_action, from, to, exit_to, done, halt, max_times, no_max } => {
            let (from_id, exit_name) = parse_point(&from)?;
            let target = edge_target(to, exit_to, done, halt)?;
            // An edge into a box is capped unless somebody says otherwise: what the limit guards
            // against is a loop that never converges, and a caller who never thought about it is the
            // one that loop happens to. An edge that leaves the action, closes or stops the run is taken
            // once, so it carries no limit at all — core refuses one there.
            let max_times = match target {
                EdgeTarget::Go(_) if no_max => None,
                EdgeTarget::Go(_) => Some(max_times.unwrap_or(DEFAULT_MAX_TIMES)),
                _ => max_times,
            };
            let e = store
                .automation_edge_add(picture(in_action), from_id, exit_name.as_deref(), target, max_times)
                .map_err(CliError::from)?;
            write_draft(flags, "automation.edge-add", "automation_edge", serde_json::to_value(&e).unwrap(), None, format!("✓ Added edge: {} ({})", from, e.id), Some(Draft::of_picture(e.owner_kind, e.owner_id)));
        }
        AutomationCmd::EdgeUpdate { id, to, exit_to, done, halt, max_times, no_max } => {
            let target = match (to, &exit_to, done, halt) {
                (None, None, false, false) => None,
                _ => Some(edge_target(to, exit_to, done, halt)?),
            };
            let max_times = match (max_times, no_max) {
                (_, true) => Some(None),
                (Some(n), false) => Some(Some(n)),
                (None, false) => None,
            };
            let e = store.automation_edge_update(id, target, max_times).map_err(CliError::from)?;
            write_draft(flags, "automation.edge-update", "automation_edge", serde_json::to_value(&e).unwrap(), None, format!("✓ Updated edge: {}", e.id), Some(Draft::of_picture(e.owner_kind, e.owner_id)));
        }
        AutomationCmd::EdgeRm { id } => {
            if !confirm(flags, "delete edge")? {
                return Ok(0);
            }
            let draft = Draft::of_edge(store, id);
            store.automation_edge_delete(id).map_err(CliError::from)?;
            write_draft(flags, "automation.edge-rm", "automation_edge", json!({ "id": id, "deleted": true }), None, format!("✓ Deleted edge: {id}"), draft);
        }
        AutomationCmd::WireAdd { in_action, from, from_port, to, to_port } => {
            let (from_id, exit_name) = parse_point(&from)?;
            let (w, drawn) = store
                .automation_wire_add(picture(in_action), from_id, exit_name.as_deref(), &from_port, to, &to_port)
                .map_err(CliError::from)?;
            // The same wire drawn again is answered as it stands, and said to be no change.
            if drawn {
                write_draft(flags, "automation.wire-add", "automation_wire", serde_json::to_value(&w).unwrap(), None, format!("✓ Added wire: {from}.{from_port} → {to}.{to_port} ({})", w.id), Some(Draft::of_picture(w.owner_kind, w.owner_id)));
            } else {
                write_envelope(flags, "automation.wire-add", "automation_wire", serde_json::to_value(&w).unwrap(), Some(vec![]), true, format!("• Wire {from}.{from_port} → {to}.{to_port} ({}) is already drawn — no change.", w.id));
            }
        }
        AutomationCmd::WireRm { id } => {
            if !confirm(flags, "delete wire")? {
                return Ok(0);
            }
            let draft = Draft::of_wire(store, id);
            store.automation_wire_delete(id).map_err(CliError::from)?;
            write_draft(flags, "automation.wire-rm", "automation_wire", json!({ "id": id, "deleted": true }), None, format!("✓ Deleted wire: {id}"), draft);
        }
        AutomationCmd::RunList { task, automation, limit } => {
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
        AutomationCmd::RunShow { id } => {
            let run = store
                .automation_run(id)
                .map_err(CliError::from)?
                .ok_or_else(|| {
                    CliError::from(amenbo_core::Error::not_found(format!("run '{id}' not found")))
                })?;
            let defs = store.automation_run_defs(id).map_err(CliError::from)?;
            let stretches = store.automation_run_tasks(id).map_err(CliError::from)?;
            let moves = store.automation_run_steps(id).map_err(CliError::from)?;
            let room = store.automation_run_waits_for_room(id).map_err(CliError::from)?;
            if flags.json {
                let mut walked = Vec::with_capacity(moves.len());
                for m in &moves {
                    walked.push(json!({
                        "step": serde_json::to_value(m).unwrap(),
                        "name": named_step(&defs, m),
                        "automation_version": version_of(&defs, m),
                        "values": serde_json::to_value(
                            store.automation_run_values(m.id).map_err(CliError::from)?,
                        )
                        .unwrap(),
                    }));
                }
                print_json(&json!({
                    "run": serde_json::to_value(&run).unwrap(),
                    "waits_for_room": serde_json::to_value(room).unwrap(),
                    "versions": taken_up(&defs)
                        .iter()
                        .map(|(version, at)| json!({ "version": version, "taken_at": at.to_rfc3339_z() }))
                        .collect::<Vec<_>>(),
                    "tasks": serde_json::to_value(&stretches).unwrap(),
                    "steps": walked,
                }));
            } else {
                render_run(store, flags, &run, room, &defs, &stretches, &moves)?;
            }
        }

        AutomationCmd::Start { id } => {
            // Asked before anything is read, so a refusal leaves nothing behind (`AMB-D-995`).
            let paths = amenbo_core::config::Paths::resolve().map_err(CliError::from)?;
            if !amenbo_core::app_running::is_running(&paths).map_err(CliError::from)? {
                return Err(CliError::app_not_running());
            }
            // A launch reads the newest saved version, and core falls back to the draft for an
            // automation never saved. That fallback is refused here.
            let saved = store
                .automation_detail(id)
                .map_err(CliError::from)?
                .ok_or_else(|| CliError::from(amenbo_core::Error::not_found(format!("automation '{id}' not found"))))?
                .saved
                .ok_or_else(|| CliError {
                    code: "conflict",
                    message: format!("automation '{id}' has never been saved, and a run starts from its newest saved version. No run was started."),
                    hint: Some(format!(
                        "Save it first with `{cmd} automation save {id}`, or try the draft without starting anything with `{cmd} automation test-run {id}`.",
                        cmd = amenbo_core::config::Paths::command_name()
                    )),
                    exit: 1,
                })?;
            let known = startable(store);
            let by = Launcher {
                default_agent: None,
                startable: known.as_deref(),
                // Nothing is claimed about the models either: asking a provider what it offers is a
                // login shell and that provider starting up, and the answers the app keeps are in the
                // app's own process (`amenbo_core::agent_models`). So a step naming a model this
                // machine does not have is caught when the pane comes up, rather than here.
                models: amenbo_core::ops::automation_run::nothing_asked(),
                // Nothing is claimed about the window: a terminal cannot see what is on screen, and a
                // `false` written here would refuse a launch the reader can see perfectly well
                // (`amenbo_core::ops::automation_run::Launcher`).
                workspace_open: None,
                by: Some(flags.facet()?),
            };
            match store.automation_launch(id, &by).map_err(CliError::from)? {
                Started::Run(r) => {
                    let line = format!("✓ Run {} started from version {}", r.id, saved.version);
                    write_envelope(flags, "automation.start", "automation_run", serde_json::to_value(&r).unwrap(), None, false, line);
                }
                // Every role's start would have waited for a task with none to take: no run is made, and
                // the app starts one in the launch once a task turns up (`AMB-D-1126`).
                Started::Waits(a) => {
                    let line = format!(
                        "✓ Automation {} ({}) has no task to take — the app starts a run once one turns up",
                        a.name, a.id
                    );
                    write_envelope(flags, "automation.start", "automation", serde_json::to_value(&a).unwrap(), None, false, line);
                }
            }
        }
        AutomationCmd::StopWaiting { id } => {
            // An automation no longer waits for a task as a whole (`AMB-D-1126`), so there is nothing to
            // stop: it is answered as it stands.
            let a = store
                .automation_detail(id)
                .map_err(CliError::from)?
                .ok_or_else(|| CliError::from(amenbo_core::Error::not_found(format!("automation '{id}' not found"))))?
                .automation;
            let line = format!("✓ Automation {} ({}) is not waiting for a task", a.name, a.id);
            write_envelope(flags, "automation.stop-waiting", "automation", serde_json::to_value(&a).unwrap(), None, false, line);
        }
        AutomationCmd::TestRun { id, title, notes, dim } => {
            // Nothing is started, so the app need not be running, and nothing is kept, so there is no
            // file to ingest: a test run is handed text only.
            let notes = crate::cmd::arg::body_arg_opt(notes)?;
            let handed = HandedAtLaunch { title, notes, classification: classification_of(&dim)?, ..Default::default() };
            let known = startable(store);
            let by = Launcher {
                default_agent: None,
                startable: known.as_deref(),
                models: amenbo_core::ops::automation_run::nothing_asked(),
                workspace_open: None,
                by: Some(flags.facet()?),
            };
            let walked = store.automation_rehearse(id, &by, &handed).map_err(CliError::from)?;
            if flags.json {
                // A launch reads the newest saved version; a test run reads the draft, and says so.
                print_json(&json!({ "test_run": serde_json::to_value(&walked).unwrap(), "reads": "draft" }));
            } else {
                render_rehearsal(flags, &walked);
            }
        }
        AutomationCmd::Pause { run, before_next_task } => {
            let verb = if before_next_task { "pause --before-next-task" } else { "pause" };
            not_from_its_own_step(store, run, verb)?;
            // `--before-next-task` stops at the next take_task rather than the end of the action
            // (`AMB-D-1019`); a run waiting there for room comes back already paused.
            let paused = match before_next_task {
                true => store.automation_pause_run_before_next_task(run),
                false => store.automation_pause(run),
            }
            .map_err(CliError::from)?;
            let (value, line) = match &paused {
                Paused::Asked(r) => (
                    json!({ "run": r.id, "state": "asked" }),
                    match before_next_task {
                        true => format!("✓ Run {} pauses before it takes its next task", r.id),
                        false => format!("✓ Run {} pauses at the end of the action under way", r.id),
                    },
                ),
                Paused::Now(ended) => (
                    json!({ "run": ended.run.id, "state": "paused" }),
                    format!("✓ Run {} is paused", ended.run.id),
                ),
            };
            write_envelope(flags, "automation.pause", "automation_run", value, None, false, line);
        }
        AutomationCmd::Resume { run } => {
            // Asked as a launch asks them: a run paused before its next task is checked afresh.
            let known = startable(store);
            let models = amenbo_core::ops::automation_run::nothing_asked();
            let Resumed { run: r, next } =
                store.automation_resume(run, known.as_deref(), models).map_err(CliError::from)?;
            let value = json!({ "run": r.id, "state": "running", "step": next.id });
            let line = format!("✓ Run {} picks up at {} ({})", r.id, next.name, next.id);
            write_envelope(flags, "automation.resume", "automation_run", value, None, false, line);
        }
        AutomationCmd::Cancel { run, force } => {
            // Only a paused run is canceled; `--force` ends one still going where it stands (`AMB-D-1002`).
            if force {
                not_from_its_own_step(store, run, "cancel --force")?;
            }
            let ended = match force {
                true => store.automation_stop(run, Ending::Canceled),
                false => store.automation_cancel(run),
            }
            .map_err(CliError::from)?;
            write_envelope(flags, "automation.cancel", "automation_run", serde_json::to_value(&ended.run).unwrap(), None, false, format!("✓ Run {} canceled", ended.run.id));
        }
        AutomationCmd::Acknowledge { run } => {
            let r = store
                .automation_acknowledge(run, flags.facet()?)
                .map_err(CliError::from)?;
            let line = format!("✓ Run {} seen — it is on the history now", r.id);
            write_envelope(flags, "automation.acknowledge", "automation_run", serde_json::to_value(&r).unwrap(), None, false, line);
        }

        AutomationCmd::LaunchShow { launch } => {
            let roles = store.automation_launch_headcounts(launch).map_err(CliError::from)?;
            if flags.json {
                print_json(&json!({
                    "launch_id": launch,
                    "count": roles.len(),
                    "roles": serde_json::to_value(&roles).unwrap(),
                }));
            } else {
                human(flags, format!("Launch {launch} — {} role(s)", roles.len()));
                for role in &roles {
                    human(flags, format!("  {}", one_launch_role(role)));
                }
            }
        }
        AutomationCmd::LaunchHeadcountSet { launch, role, headcount } => {
            // Refusing none is the core's (`AMB-D-1124`); the way to stop a role is named beside it.
            let row = store.automation_launch_headcount_set(launch, role, headcount).map_err(|e| {
                let mut err = CliError::from(e);
                if headcount == 0 {
                    err.hint = Some(format!(
                        "To stop the role taking tasks, `amenbo automation launch-role-hold {launch} --role {role}`."
                    ));
                }
                err
            })?;
            let line = format!("✓ Role {} has {} in launch {} — the definition is left as it is", row.role_id, row.headcount, row.launch_id);
            write_envelope(flags, "automation.launch-headcount-set", "automation_launch_role", serde_json::to_value(&row).unwrap(), None, false, line);
        }
        AutomationCmd::LaunchHeadcountReset { launch, role } => {
            let row = store.automation_launch_headcount_reset(launch, role).map_err(CliError::from)?;
            let line = format!("✓ Role {} is back at its definition's {} in launch {}", row.role_id, row.headcount, row.launch_id);
            write_envelope(flags, "automation.launch-headcount-reset", "automation_launch_role", serde_json::to_value(&row).unwrap(), None, false, line);
        }
        AutomationCmd::LaunchRoleHold { launch, role } => {
            // The role's own stop, apart from a run's `pause --before-next-task` (`AMB-D-1019`). It is not
            // refused from a step: its own run finishes the task it is on first (`AMB-D-1020`).
            let held = store.automation_launch_role_hold(launch, role).map_err(CliError::from)?;
            let ended: Vec<i64> = held.runs.iter().map(|r| r.id).collect();
            let line = format!("✓ Role {} in launch {} stops once its task is done", held.role.role_id, held.role.launch_id);
            let value = serde_json::to_value(&held.role).unwrap();
            write_envelope_with(flags, "automation.launch-role-hold", "automation_launch_role", value, None, false, line, &[("runs_ended", json!(ended))]);
        }
        AutomationCmd::LaunchRoleRelease { launch, role } => {
            let row = store.automation_launch_role_release(launch, role).map_err(CliError::from)?;
            let line = format!("✓ Role {} in launch {} takes tasks again, up to {}", row.role_id, row.launch_id, row.headcount);
            write_envelope(flags, "automation.launch-role-release", "automation_launch_role", serde_json::to_value(&row).unwrap(), None, false, line);
        }

        AutomationCmd::StepOut { value, file } => {
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
                    let port = port_id(value.trim(), &value)?;
                    store.automation_out(step, port, Produced::File(a.id)).map_err(CliError::from)?
                }
                None => hand_on(store, step, &value)?,
            };
            write_envelope(flags, "automation.out", "automation_run_value", serde_json::to_value(&v).unwrap(), None, false, format!("✓ Handed on: output {}", v.port_id));
        }
        AutomationCmd::StepDone { report, exit, outs } => {
            let step = speaking_for()?;
            // Everything produced goes down before the way out is stamped: `done` refuses a missing
            // required output, and a value written after that refusal would arrive at a step that has
            // already been told it is not finished.
            for one in &outs {
                hand_on(store, step, one)?;
            }
            let report = body_arg(report)?;
            let next = store
                .automation_done(step, exit, &report)
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
        message: "this is not a step of a run — `step-out` and `step-done` are typed by the agent a step opened, in the terminal the run opened for it.".to_string(),
        hint: Some("start a run with `automation start <id>`, and the step's own terminal carries what these commands need".to_string()),
        exit: 2,
    })
}

/// **A step does not stop the run it belongs to** (`AMB-D-1020`). Pausing it stops the step that asked,
/// and a forced cancel closes that step's terminal with it, so either way the step cannot finish its
/// work. Outside a step, or aimed at another run, the command goes through.
fn not_from_its_own_step(store: &Store, run: i64, verb: &str) -> Result<(), CliError> {
    let Some(step) = amenbo_core::env::automation_step() else {
        return Ok(());
    };
    let own = store.automation_run_steps(run).map_err(CliError::from)?.iter().any(|s| s.id == step);
    if !own {
        return Ok(());
    }
    Err(CliError {
        code: "invalid_value",
        message: format!(
            "`automation {verb}` on run {run} is refused inside a step of run {run} — stopping its own run would leave this step unable to finish its work."
        ),
        hint: Some(format!(
            "finish the step with `automation step-done`; a person can pause or cancel run {run} from its pane"
        )),
        exit: 2,
    })
}

/// **The agent ids this machine was last seen able to start**, or `None` where it has never been
/// asked (`AMB-D-792`) — [`amenbo_core::wake::startable_ids`], which every launching surface asks
/// rather than each deriving it from the remembered commands.
///
/// `None` reaches the launch check as "not asked", which leaves the agent check unmade rather than
/// failing every step on a machine nobody has probed.
/// `--dim <axis>=<value>`, split here and looked up by the launch, which classifies the task it files
/// before the run with them (`amenbo_core::ops::automation_builtin_make::launch_values`).
fn classification_of(dim: &[String]) -> Result<Vec<(String, String)>, CliError> {
    dim.iter()
        .map(|pair| match pair.split_once('=') {
            Some((axis, value)) => Ok((axis.trim().to_string(), value.trim().to_string())),
            None => Err(CliError::from(amenbo_core::Error::invalid(format!(
                "--dim takes <axis>=<value> (e.g. --dim \"Category=bug\"), got `{pair}`"
            )))),
        })
        .collect()
}

/// **A test run, as a terminal reads it**: the first line says it tried the draft, then each step it
/// opened, the way out it was taken to leave by, and — for an agent's step — the whole prompt, indented
/// under it. The last line says how it ended.
fn render_rehearsal(flags: &Flags, walked: &amenbo_core::ops::automation_rehearse::Rehearsal) {
    use amenbo_core::ops::automation_rehearse::Cut;
    human(flags, "Test run of the draft, unsaved changes included — no agent was started, and nothing was kept");
    for (n, step) in walked.steps.iter().enumerate() {
        let who = match (&step.builtin, &step.agent) {
            (Some(key), _) => format!("built-in {key}"),
            (None, Some(agent)) => match &step.model {
                Some(model) => format!("{agent} {model}"),
                None => agent.clone(),
            },
            (None, None) => String::new(),
        };
        human(flags, format!("\n{}. {} [{who}] → {}", n + 1, step.name, step.exit));
        if let Some(folder) = &step.folder {
            human(flags, format!("   in {folder}"));
        }
        if let Some(prompt) = &step.prompt {
            for line in prompt.lines() {
                human(flags, format!("   │ {line}"));
            }
        }
    }
    let ended = match (walked.cut, walked.missing.is_empty(), &walked.no_agent, walked.stopped_reason) {
        (Some(Cut::Looped), ..) => "cut short: a step was reached again with no way out left to try".to_string(),
        (Some(Cut::TooLong), ..) => "cut short: too many steps".to_string(),
        (None, false, ..) => format!("stopped: nothing is wired into {}", walked.missing.join(", ")),
        (None, true, Some(agent), _) => format!("stopped: this machine cannot start {agent}"),
        (None, true, None, Some(reason)) => format!("failed: {}", reason.as_str()),
        (None, true, None, None) => walked.status.as_str().to_string(),
    };
    human(flags, format!("\n✓ Test run ended — {ended}"));
}

fn startable(store: &Store) -> Option<Vec<String>> {
    amenbo_core::wake::startable_ids(&store.config)
}

/// `<name>=<value>`, as `out` and `done --out` take it.
/// **Put one `<name>=<what>` down**, in whatever shape the name was declared to take.
///
/// The declaration is read first because the same words mean two things: on a `value` port the text is
/// the answer, and on a `task_make` port it names a task this step raised, which is written as the task
/// rather than as its spelling ([`amenbo_core::ops::automation_report::out_kind`]).
///
/// **The task the run is about does not come this way and is refused here**, saying whose it is. A
/// built-in takes it (`AMB-D-964`), reserving and declaring it in one act, and a step has no command
/// that does — being told so by the kind check would not say why.
fn hand_on(store: &mut Store, step: i64, one: &str) -> Result<AutomationRunValue, CliError> {
    let (port, text) = parse_produced(one)?;
    match store.automation_out_kind(step, port).map_err(CliError::from)? {
        Some(AutomationPortKind::TaskTake) => Err(CliError {
            code: "invalid_value",
            message: format!(
                "output {port} is the task the run works, which a built-in takes — a step does not hand it on"
            ),
            hint: Some(
                "put `take_task` or `make_task` on the picture before this step, and leave this one by a way out that does not declare the task".to_string()
            ),
            exit: 2,
        }),
        // A task of another project than the run's is refused by core, so a screen is held to it too.
        Some(AutomationPortKind::TaskMake) => {
            let task = resolve_task(store, text.trim()).map_err(CliError::from)?;
            store.automation_out(step, port, Produced::Task(task)).map_err(CliError::from)
        }
        // An id the step declares no output of is refused by core, naming the ones it does.
        _ => store.automation_out(step, port, Produced::Value(&text)).map_err(CliError::from),
    }
}

/// `<id>=<value>`, split — the id being an output's, as the step's text lists it (`AMB-D-961`).
fn parse_produced(one: &str) -> Result<(i64, String), CliError> {
    match one.split_once('=') {
        Some((id, value)) => Ok((port_id(id.trim(), one)?, value.to_string())),
        None => Err(not_an_output(one)),
    }
}

/// The id of an output, as `step-out` takes it; `whole` is what was typed, for the refusal.
fn port_id(id: &str, whole: &str) -> Result<i64, CliError> {
    id.parse::<i64>().map_err(|_| not_an_output(whole))
}

fn not_an_output(one: &str) -> CliError {
    CliError {
        code: "invalid_value",
        message: format!("'{one}' is not `<id>=<value>`"),
        hint: Some(
            "write what the step hands on as `12=the answer`, the id being an output's from the step's text"
                .to_string(),
        ),
        exit: 2,
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
        Next::Halted(_) => "✓ Step done — the run failed and is waiting for a person".to_string(),
        Next::Paused(_) => "✓ Step done — the run is paused".to_string(),
    }
}


// ───────────────────────── what was built ─────────────────────────

/// One automation in full: the placements on it, and what each runs under.
fn render_automation(
    flags: &Flags,
    view: &AutomationView,
    carriers: &BTreeMap<(i64, i64), Carrier>,
) {
    let a = &view.automation;
    human(flags, format!("Automation {}  {}", a.id, a.name));
    let archived = if a.archived { "  archived" } else { "" };
    let stopped = if a.stopped { "  stopped" } else { "" };
    human(
        flags,
        format!(
            "project {}  {} placement(s){stopped}{archived}",
            a.project_id,
            view.placements.len()
        ),
    );
    if a.export_only {
        human(flags, export_only_line(a.id));
    } else {
        human(flags, failures_to_line(a.failures_to, &view.roles));
    }
    for trigger in &a.triggers {
        human(flags, trigger_line(trigger));
    }
    human(flags, saved_line(view.saved.as_ref(), view.unsaved));
    if let Some(line) = showing_line(view.showing, view.saved.as_ref()) {
        human(flags, line);
    }
    write_body(flags, "notes", &a.notes);
    for role in &view.roles {
        human(flags, format!("role {}", one_role(role)));
    }
    for placement in &view.placements {
        render_placement(flags, view, placement, carriers);
    }
}

/// One of an automation's triggers: its id, whether it is on, what it is called and does, the tasks it
/// looks at and the program it asks first, the task it files (or the file or program that says it) and
/// the role it files it under, when it is next asked (for an `idle`, a `tasks` or a `script`, the
/// soonest it may be — and nothing, for one that is off, as it is asked no more), and when it last
/// filed a task.
fn trigger_line(trigger: &amenbo_core::model::AutomationTrigger) -> String {
    use amenbo_core::model::{TriggerTask, TriggerWhen};
    let fewer = trigger.condition.as_ref().map(|c| {
        let of_role = c.role_id.map(|role| format!(" under role {role}")).unwrap_or_default();
        match c.filter.as_str() {
            "" => format!("fewer than {} task(s){of_role} are in its project", c.below),
            filter => format!("fewer than {} task(s){of_role} match \"{filter}\"", c.below),
        }
    });
    let says = trigger.script.as_ref().map(|s| format!("\"{}\" says so", s.program));
    let (when, next) = match trigger.when {
        TriggerWhen::Every { minutes } => (format!("every {minutes} minute(s)"), "next at"),
        TriggerWhen::Idle => ("whenever no seat of its role is taken".to_string(), "not before"),
        TriggerWhen::Tasks | TriggerWhen::Script => ("whenever".to_string(), "not before"),
    };
    let first: Vec<String> = fewer.into_iter().chain(says).collect();
    let when = match (trigger.when, first.join(" and ")) {
        (_, first) if first.is_empty() => when,
        (TriggerWhen::Tasks | TriggerWhen::Script, first) => format!("{when} {first}"),
        (_, first) => format!("{when} while {first}"),
    };
    let task = match &trigger.task {
        TriggerTask::Fixed { title, .. } => format!("\"{title}\""),
        TriggerTask::File { path } => format!("the task \"{path}\" says"),
        TriggerTask::Script { script } => format!("the task \"{}\" writes", script.program),
    };
    let called = match trigger.name.as_str() {
        "" => String::new(),
        name => format!(" as \"{name}\""),
    };
    let under = trigger.role_id.map(|role| format!(" under role {role}")).unwrap_or_default();
    let (state, next) = if trigger.enabled {
        ("on", format!("{next} {}", trigger.next_at.to_rfc3339_z()))
    } else {
        ("off", "asked no more until --on".to_string())
    };
    let last = match &trigger.last_filed_at {
        Some(at) => format!("last filed one at {}", at.to_rfc3339_z()),
        None => "has filed none yet".to_string(),
    };
    format!(
        "trigger {} {state}: starts a run on its own{called} {when}, filing {task}{under} — {next}; {last}",
        trigger.id
    )
}

/// An automation from before automations had roles, which is kept only to be written out, and what to
/// do with it instead. Where its failed rounds would go is not said: it is never started.
fn export_only_line(id: i64) -> String {
    format!(
        "written before automations had roles — kept only to be exported: `automation export {id} --out <file>`, \
         rebuild it and `automation import` it as a new automation"
    )
}

/// Where a failed round is filed, on a line of its own: a person, a role by its id and name, or nobody
/// yet — which a start is refused over.
fn failures_to_line(to: Option<FailuresTo>, roles: &[AutomationRole]) -> String {
    match to {
        None => "failed rounds go to nobody yet — choose with `automation update --failures-to`".to_string(),
        Some(FailuresTo::Human) => "failed rounds go to a person".to_string(),
        Some(FailuresTo::Role { role_id }) => match roles.iter().find(|r| r.id == role_id) {
            Some(role) => format!("failed rounds go to role {role_id}  {}", role.name),
            None => format!("failed rounds go to role {role_id}"),
        },
    }
}

/// `--failures-to` as typed: `human` for a person, or a role's id.
fn failures_to_arg(to: &str) -> Result<FailuresTo, CliError> {
    let to = to.trim();
    if to == "human" {
        return Ok(FailuresTo::Human);
    }
    to.parse::<i64>().map(|role_id| FailuresTo::Role { role_id }).map_err(|_| {
        CliError::from(amenbo_core::Error::invalid(format!(
            "--failures-to takes `human` or the id of one of the automation's roles, not '{to}'"
        )))
    })
}

/// One role on one line: its id and name, how many are in it, its icon, where its round starts, and the
/// agent and model its steps fall back to.
fn one_role(role: &AutomationRole) -> String {
    let mut line = format!("{}  {}  {} in it", role.id, role.name, role.headcount);
    if let Some(icon) = &role.icon {
        line.push_str(&format!("  icon {icon}"));
    }
    match role.entry_placement_id {
        Some(placement) => line.push_str(&format!("  starts at placement {placement}")),
        None => line.push_str("  starts nowhere"),
    }
    match (&role.agent, &role.model) {
        (Some(agent), Some(model)) => line.push_str(&format!("  agent {agent} ({model})")),
        (Some(agent), None) => line.push_str(&format!("  agent {agent}")),
        (None, _) => line.push_str("  no agent"),
    }
    line
}

/// One role of a launch: the headcount its definition gives it beside the one this launch holds, the
/// runs under way, the seats still to go, and whether it stops once its task is done.
fn one_launch_role(role: &RoleHeadcount) -> String {
    let mut line = format!(
        "role {}  defined {}  this launch {}  {} under way",
        role.role_id,
        role.defined,
        role.headcount,
        role.under_way.len()
    );
    if role.to_let_go > 0 {
        line.push_str(&format!("  {} to let go", role.to_let_go));
    }
    if role.held {
        line.push_str("  stops once its task is done");
    }
    line
}

/// One placement: which action stands there, what it takes, what it is set to, and what happens after
/// each way out.
fn render_placement(
    flags: &Flags,
    view: &AutomationView,
    placement: &PlacementView,
    carriers: &BTreeMap<(i64, i64), Carrier>,
) {
    let row = &placement.placement;
    let boxes: Vec<i64> = view.placements.iter().map(|p| p.placement.id).collect();
    let starts: Vec<i64> = view.roles.iter().filter_map(|r| r.entry_placement_id).collect();
    let back = lines_back(&starts, &boxes, &view.edges);
    let named = match &placement.action {
        Some(action) => format!(
            "action {} ({}) {}",
            action.id,
            action.name,
            placed_version(placement.version, placement.latest_version)
        ),
        None => "no action".to_string(),
    };
    human(flags, format!("\nplacement {} — {named}", row.id));
    for port in &placement.inputs {
        human(flags, format!("    takes  {}", one_port(port)));
    }
    for cfg in &placement.settings {
        human(flags, format!("    set  {}", one_cfg(cfg)));
    }
    for one in &placement.steps {
        // Nobody is chosen for a built-in or a script, and nobody ever will be: Amenbo carries a
        // built-in out itself (`AMB-D-964`) and a script runs its own program (`AMB-D-1016`), so
        // naming an agent or "nobody chosen" there misreads what runs it.
        let who = match (&one.step.builtin, &one.step.script, carriers.get(&(row.id, one.step.id))) {
            (Some(key), _, _) => format!("carried out by Amenbo (built-in {key})"),
            (None, Some(script), _) => {
                let mut line = format!("carried out by a script: {}", script.program);
                for arg in &script.args {
                    line.push(' ');
                    line.push_str(arg);
                }
                line
            }
            (None, None, Some(c)) => {
                let by = match &c.model {
                    Some(model) => format!("carried out by {} ({model})", c.agent),
                    None => format!("carried out by {}", c.agent),
                };
                match &c.at {
                    ChosenAt::Step => by,
                    ChosenAt::Role { role_id } => {
                        let role = view.roles.iter().find(|r| r.id == *role_id).map_or("?", |r| r.name.as_str());
                        format!("{by}  — role {role_id} {role}'s default")
                    }
                    ChosenAt::Amenbo => format!("{by}  — Amenbo's default"),
                }
            }
            (None, None, None) => "nobody chosen to carry it out".to_string(),
        };
        human(flags, format!("    step {} {}  {who}", one.step.id, one.step.name));
    }
    for exit in &placement.exits {
        human(flags, format!("    way out {}  [{}]", one_exit(&exit.exit.name), exit.exit.id));
        for port in &exit.outputs {
            human(flags, format!("        hands on  {}", one_port(port)));
        }
        for edge in view.edges.iter().filter(|e| e.from_id == row.id && e.exit_id == exit.exit.id) {
            human(flags, format!("        then  {}", one_edge(edge, &[], &back)));
        }
        for wire in
            view.wires.iter().filter(|w| w.from_id == row.id && w.from_exit_id == Some(exit.exit.id))
        {
            human(
                flags,
                format!(
                    "        wire  {} → placement {} . {}",
                    port_named(view.port_name(wire.from_port_id), wire.from_port_id),
                    wire.to_id,
                    port_named(view.port_name(wire.to_port_id), wire.to_port_id)
                ),
            );
        }
    }
    // An edge keyed to a way out the action no longer declares is the reason a picture stops walking,
    // so it is written out rather than left off the account. Deleting a way out takes its edges with it
    // (`AMB-D-961`); what is left to show here is an edge a store carried in from before that.
    let declared: Vec<i64> = placement.exits.iter().map(|e| e.exit.id).collect();
    for edge in view.edges.iter().filter(|e| e.from_id == row.id && !declared.contains(&e.exit_id)) {
        human(
            flags,
            format!(
                "    way out [{}] — no longer declared\n        then  {}",
                edge.exit_id,
                one_edge(edge, &[], &back)
            ),
        );
    }
}

/// One library action: the steps inside it, the picture they are drawn into, and what it declares to
/// every placement of it.
fn render_action(flags: &Flags, view: &ActionView) {
    let a = &view.action;
    let shelf = match a.project_id {
        Some(project_id) => format!("the library of project {project_id}"),
        None => "the device's library".to_string(),
    };
    human(flags, format!("Action {}  {}", a.id, a.name));
    if a.draft {
        human(
            flags,
            format!(
                "still being created — keep it with `automation action-save {}`, or give it up \
                 with `automation action-abandon {}`",
                a.id, a.id
            ),
        );
    }
    let entry = match a.entry_step_id {
        Some(step) => format!("opens step {step} first"),
        None => "opens nothing".to_string(),
    };
    human(
        flags,
        format!("{shelf}  {entry}  used by {} automation(s)", view.used_by),
    );
    if a.one_at_a_time {
        human(flags, "one run at a time — a run reaching it waits while another run is inside it");
    }
    match &a.builtin {
        Some(key) => {
            let latest = amenbo_core::ops::automation_builtin::find(key).map(|b| b.version);
            human(flags, format!("built-in {}", placed_version(a.builtin_version, latest)));
        }
        None => {
            human(flags, saved_line(view.saved.as_ref(), view.unsaved));
            if let Some(line) = showing_line(view.showing, view.saved.as_ref()) {
                human(flags, line);
            }
        }
    }
    if !view.placed_at.is_empty() {
        let mut places: Vec<String> = Vec::new();
        for one in &view.placed_at {
            let place = match one.version {
                Some(version) => format!("automation {} version {version}", one.automation_id),
                None => format!("automation {} no version", one.automation_id),
            };
            if !places.contains(&place) {
                places.push(place);
            }
        }
        human(flags, format!("placed at: {}", places.join(", ")));
    }
    write_body(flags, "note", &a.note);
    for port in &view.inputs {
        human(flags, format!("takes  {}", one_port(port)));
        // What the action takes in is handed on from the boundary, so the wires out of it sit here
        // rather than under a step.
        for wire in view
            .wires
            .iter()
            .filter(|w| w.from_id == amenbo_core::model::ACTION_BOUNDARY && w.from_port_id == port.id)
        {
            human(
                flags,
                format!(
                    "    wire  {} → step {} . {}",
                    port.name,
                    wire.to_id,
                    port_named(view.port_name(wire.to_port_id), wire.to_port_id)
                ),
            );
        }
    }
    for cfg in &view.settings {
        human(flags, format!("declares  {}", one_cfg(cfg)));
    }
    for exit in &view.exits {
        human(flags, format!("way out {}  [{}]", one_exit(&exit.exit.name), exit.exit.id));
        for port in &exit.outputs {
            human(flags, format!("    hands on  {}", one_port(port)));
        }
    }
    for step in &view.steps {
        render_step(flags, view, step);
    }
}

/// The version saved last, and whether the draft holds more.
fn saved_line(saved: Option<&SavedVersion>, unsaved: bool) -> String {
    let saved = match saved {
        Some(one) => format!("version {} saved {}", one.version, one.saved_at.to_rfc3339_z()),
        None => "not saved yet".to_string(),
    };
    let draft = if unsaved { "the draft has unsaved changes" } else { "no unsaved changes" };
    format!("{saved} · {draft}")
}

/// Which of the two a show is reading, where there is a saved version to tell it from.
fn showing_line(showing: Showing, saved: Option<&SavedVersion>) -> Option<String> {
    let version = saved?.version;
    Some(match showing {
        Showing::Draft => format!("showing the draft; --saved shows version {version}"),
        Showing::Saved => format!("showing version {version}; without --saved shows the draft"),
    })
}

/// The version of its action a placement stands on, beside the newest there is.
fn placed_version(version: Option<i64>, latest: Option<i64>) -> String {
    match (version, latest) {
        (Some(v), Some(l)) if v == l => format!("version {v} (latest)"),
        (Some(v), Some(l)) => format!("version {v} (latest {l})"),
        (Some(v), None) => format!("version {v}"),
        (None, Some(l)) => format!("no version (latest {l})"),
        (None, None) => "no saved version".to_string(),
    }
}

/// One step inside an action: the prompt it runs on, what it takes, and what happens after each way
/// out.
fn render_step(flags: &Flags, view: &ActionView, step: &StepView) {
    let boxes: Vec<i64> = view.steps.iter().map(|s| s.step.id).collect();
    let back = lines_back(view.action.entry_step_id.as_slice(), &boxes, &view.edges);
    let row = &step.step;
    let mut marks = Vec::new();
    if row.interactive {
        marks.push("interactive".to_string());
    }
    if row.report_to_task {
        marks.push("reports to the task".to_string());
    }
    if let Some(name) = &row.work_dir_ref {
        marks.push(format!("runs in \"{name}\""));
    }
    if !row.show_history {
        marks.push("no history".to_string());
    }
    if !row.show_notes {
        marks.push("no task notes".to_string());
    }
    if !row.show_decisions {
        marks.push("no task decisions".to_string());
    }
    if !row.show_comments {
        marks.push("no task comments".to_string());
    }
    // Only a time the step writes is shown: one that says nothing takes the default.
    if let Some(minutes) = row.expected_minutes {
        marks.push(format!("expected {minutes} min"));
    }
    // A step that says none of these is written bare — who carries it out is the placement's to say.
    let marks = match marks.is_empty() {
        true => String::new(),
        false => format!("  [{}]", marks.join(" · ")),
    };
    human(flags, format!("\nstep {} — {}{marks}", row.id, row.name));
    for line in row.prompt.lines() {
        human(flags, format!("      | {line}"));
    }
    for port in &step.inputs {
        human(flags, format!("    takes  {}", one_port(port)));
    }
    for exit in &step.exits {
        human(flags, format!("    way out {}  [{}]", one_exit(&exit.exit.name), exit.exit.id));
        for port in &exit.outputs {
            human(flags, format!("        hands on  {}", one_port(port)));
        }
        for edge in view.edges.iter().filter(|e| e.from_id == row.id && e.exit_id == exit.exit.id) {
            human(flags, format!("        then  {}", one_edge(edge, &view.exits, &back)));
        }
        for wire in
            view.wires.iter().filter(|w| w.from_id == row.id && w.from_exit_id == Some(exit.exit.id))
        {
            let into = match wire.to_id {
                amenbo_core::model::ACTION_BOUNDARY => "the action".to_string(),
                step => format!("step {step}"),
            };
            human(
                flags,
                format!(
                    "        wire  {} → {into} . {}",
                    port_named(view.port_name(wire.from_port_id), wire.from_port_id),
                    port_named(view.port_name(wire.to_port_id), wire.to_port_id)
                ),
            );
        }
    }
}

/// How a way out is named where it labels a block rather than sits in a sentence — short, so the two
/// every declarer is born with do not read as the longer phrase a report uses.
/// A wire's end as a reader reads it: the port's name now, or its id where nothing here declares it.
fn port_named(name: Option<&str>, id: i64) -> String {
    name.map(str::to_string).unwrap_or_else(|| format!("port {id}"))
}

fn one_exit(name: &str) -> String {
    match name {
        amenbo_core::model::ERROR_EXIT => "the error one".to_string(),
        name => format!("\"{name}\""),
    }
}

/// A Markdown field under its own name, or nothing at all where it is empty — a heading with no body
/// under it says there is something to read.
fn write_body(flags: &Flags, what: &str, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    human(flags, format!("{what}:"));
    for line in body.lines() {
        human(flags, format!("  | {line}"));
    }
}

/// One port on one line: the name it is handed under, what it carries, whether it may be missing, and
/// the id `port-update` and `port-rm` take.
fn one_port(port: &amenbo_core::model::AutomationPort) -> String {
    let required = if port.required { "required" } else { "optional" };
    format!("{}  {}  {required}  [{}]", port.name, port.kind.as_str(), port.id)
}

/// One setting on one line, with the answer written while building where there is one.
fn one_cfg(cfg: &amenbo_core::model::AutomationCfg) -> String {
    let required = if cfg.required { "required" } else { "optional" };
    let answer = match &cfg.value {
        Some(value) => format!(" = {value}"),
        None => " — unanswered".to_string(),
    };
    let options = match &cfg.options {
        Some(options) => format!("  of {options}"),
        None => String::new(),
    };
    format!("{}  {}  {required}{options}{answer}", cfg.name, cfg.kind.as_str())
}

/// What happens after a way out is taken, as one phrase. `action_exits` are the ways out of the action
/// the picture is inside, which a line leaving the action is read against — empty on an automation's
/// picture, where no line leaves anything. `back` are the picture's lines that go back: the limit is
/// written on those alone, since a line going down carries one that is never counted.
fn one_edge(
    edge: &amenbo_core::model::AutomationEdge,
    action_exits: &[amenbo_core::ops::automation_view::ExitView],
    back: &std::collections::BTreeSet<i64>,
) -> String {
    let where_to = match (edge.ends, edge.to_id) {
        (amenbo_core::model::AutomationEnds::Go, Some(next)) => format!("box {next}"),
        (amenbo_core::model::AutomationEnds::Go, None) => "nowhere".to_string(),
        (amenbo_core::model::AutomationEnds::Exit, _) => {
            match action_exits.iter().find(|e| Some(e.exit.id) == edge.exit_to_id) {
                Some(e) => format!("out of the action by '{}'", e.exit.name),
                None => "out of the action by a way out it no longer declares".to_string(),
            }
        }
        (amenbo_core::model::AutomationEnds::Done, _) => "the run is done".to_string(),
        (amenbo_core::model::AutomationEnds::Halt, _) => "the run stops for a person".to_string(),
    };
    match edge.max_times.filter(|_| back.contains(&edge.id)) {
        Some(times) => format!("{where_to}  (at most {times} time(s) per task)"),
        None => where_to,
    }
}

// ───────────────────────── what ran ─────────────────────────

/// **A run is reached, never searched for.** What a later session asks is how one task was handled, or
/// what an automation has done — both of which start from a record that is already in hand. So there is
/// no listing of every run, and the words a run wrote are not on the word index: the report of a step
/// is reached from the task it was about (`AMB-T-5250`).
/// A run's whole story on the terminal: the run's own line, then each task it worked with the steps it
/// spent on that task under it. The order is the order it happened in, which is the only order a run
/// reads in.
///
/// **A run waiting for room says so** before an action that takes one run at a time, naming the action
/// and the run it waits behind. Its stretch is still going: the task it took is not done with yet.
///
/// No run waits for a task: one that would ends, and its seat is filled once a task turns up
/// (`AMB-D-1126`).
fn render_run(
    store: &mut Store,
    flags: &Flags,
    run: &AutomationRun,
    room: Option<ForRoom>,
    defs: &[AutomationRunDef],
    stretches: &[AutomationRunTask],
    moves: &[AutomationRunStep],
) -> Result<(), CliError> {
    let launch = run.launch_id.map(|l| format!("  launch {l}")).unwrap_or_default();
    human(flags, format!("Run {}  automation {}{launch}", run.id, run.automation_id));
    let stopped = run
        .stopped_reason
        .map(|r| format!(" ({})", r.as_str()))
        .unwrap_or_default();
    human(
        flags,
        format!("status: {}{stopped}  {}", run.status.as_str(), span(run.started_at, run.ended_at)),
    );
    if let Some(detail) = &run.stopped_detail {
        human(flags, "did not pass:");
        for line in detail.lines().filter(|l| !l.trim().is_empty()) {
            human(flags, format!("  | {line}"));
        }
    }
    // Each version the run took its copy from, in turn: a run taken up at its entry, or picked up after
    // a pause before its next task, goes on from a newer one (`AMB-D-1015`).
    let versions = taken_up(defs);
    if let Some(((first, _), rest)) = versions.split_first() {
        let mut line = format!("copied from: {}", version_label(*first));
        for (version, at) in rest {
            line.push_str(&format!(" → {} (taken up at {})", version_label(*version), at.to_rfc3339_z()));
        }
        human(flags, line);
    }
    // A failure is waiting on somebody until it is seen, so a failed run says which it is — and a seen
    // one, by whom (`AMB-D-989`).
    if run.status == AutomationRunStatus::Failed {
        let seen = match (run.acknowledged_at, run.acknowledged_by_kind) {
            (Some(at), Some(by)) => format!("seen by {} at {}", by.as_str(), at.to_rfc3339_z()),
            (Some(at), None) => format!("seen at {}", at.to_rfc3339_z()),
            (None, _) => "not seen yet — `automation acknowledge` once it has been".to_string(),
        };
        human(flags, format!("acknowledged: {seen}"));
    }
    if let Some(room) = room {
        human(
            flags,
            format!(
                "waiting: for room — action {} takes one run at a time; behind run {}",
                room.action_id, room.behind
            ),
        );
    }
    let switched = switches(defs, moves);
    for stretch in stretches {
        let about = match stretch.task_id {
            Some(task_id) => task_label(task_id),
            None => "no task".to_string(),
        };
        let stood = span(stretch.started_at, stretch.ended_at);
        human(flags, format!("\ntask {} — {about}  {stood}", stretch.seq));
        // A round that failed was filed as a task, and the round's task waits on it.
        if let Some(filed) = stretch.incident_task_id {
            human(flags, format!("  failure filed as {}", task_label(filed)));
        }
        for m in moves.iter().filter(|m| m.run_task_id == Some(stretch.id)) {
            render_move(store, flags, defs, &switched, m)?;
        }
    }
    // A step that went looking for a task and found none belongs to no stretch, and is the whole of
    // what the run did — so it is written out rather than left off the account.
    let loose: Vec<&AutomationRunStep> = moves.iter().filter(|m| m.run_task_id.is_none()).collect();
    if !loose.is_empty() {
        human(flags, "\nno task");
        for m in loose {
            render_move(store, flags, defs, &switched, m)?;
        }
    }
    Ok(())
}

/// One step execution: which step it was, how it left, how long it stood, what it carried, and the
/// whole of what it said. The report is written out in full — a run read back months later is read for
/// exactly this, and a snippet would send the reader somewhere else to finish the sentence. A script
/// step's last output follows, as much of it as was kept.
fn render_move(
    store: &mut Store,
    flags: &Flags,
    defs: &[AutomationRunDef],
    switched: &[(i64, Option<i64>, Option<i64>)],
    m: &AutomationRunStep,
) -> Result<(), CliError> {
    if let Some((_, from, to)) = switched.iter().find(|(id, _, _)| *id == m.id) {
        human(flags, format!("  ({} → {} from here)", version_label(*from), version_label(*to)));
    }
    human(
        flags,
        format!(
            "  {}. {}  left through {}  {}  [{}]",
            m.seq,
            named_step(defs, m),
            named(left_by(defs, m).as_deref()),
            span(m.started_at, m.ended_at),
            m.status.as_str(),
        ),
    );
    let def = defs.iter().find(|d| d.id == m.run_def_id);
    for v in store.automation_run_values(m.id).map_err(CliError::from)? {
        human(flags, format!("      {}", one_run_value(def, &v)));
    }
    for line in m.report.lines().filter(|l| !l.trim().is_empty()) {
        human(flags, format!("      | {line}"));
    }
    // Built to carry the report onto the task, which was closed by then (`AMB-D-963`).
    if m.report_withheld {
        human(flags, "      (not left on the task: it was closed)");
    }
    for line in tail_lines(&m.output_tail) {
        human(flags, line);
    }
    Ok(())
}

/// The end of what a script step printed to stdout and stderr (`AMB-D-1016`), or the screen an agent's
/// step had as it failed its run, under a heading, so a failed run can be read from the terminal.
/// Nothing where nothing was kept, which is every other step.
fn tail_lines(tail: &str) -> Vec<String> {
    if tail.trim().is_empty() {
        return Vec::new();
    }
    std::iter::once("      output (the end of it):".to_string())
        .chain(tail.lines().map(|line| format!("      > {line}")))
        .collect()
}

/// How a way out is spoken of in a sentence: by its name, and the error one as what it is. A step
/// still running has taken none yet, which is a thing of its own and reads as such.
fn named(exit: Option<&str>) -> String {
    match exit {
        Some(amenbo_core::model::ERROR_EXIT) => "the error way out".to_string(),
        Some(name) => format!("\"{name}\""),
        None => "no way out yet".to_string(),
    }
}

/// The name the step was launched under, or that the step it ran is gone.
fn named_step(defs: &[AutomationRunDef], m: &AutomationRunStep) -> String {
    defs.iter()
        .find(|d| d.id == m.run_def_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "a step".to_string())
}

/// The saved version of the automation the step's copy was taken from — `None` where it had none saved
/// then, or the copy is gone.
fn version_of(defs: &[AutomationRunDef], m: &AutomationRunStep) -> Option<i64> {
    defs.iter().find(|d| d.id == m.run_def_id).and_then(|d| d.automation_version)
}

/// The versions the run's copies were taken from, in the order they were taken, each with when — a
/// copy taken again from the version before it is no change, and is not listed.
fn taken_up(defs: &[AutomationRunDef]) -> Vec<(Option<i64>, Timestamp)> {
    let mut versions: Vec<(Option<i64>, Timestamp)> = Vec::new();
    for d in defs {
        if versions.last().map(|(v, _)| *v) != Some(d.automation_version) {
            versions.push((d.automation_version, d.created_at));
        }
    }
    versions
}

/// The step executions that ran on another version than the one before them, by the order they
/// happened in: each with the version before and its own.
fn switches(defs: &[AutomationRunDef], moves: &[AutomationRunStep]) -> Vec<(i64, Option<i64>, Option<i64>)> {
    let mut found = Vec::new();
    let mut before = None;
    for m in moves {
        let now = version_of(defs, m);
        if let Some(from) = before {
            if from != now {
                found.push((m.id, from, now));
            }
        }
        before = Some(now);
    }
    found
}

/// A saved version by its number, and a copy taken while nothing was saved as that.
fn version_label(version: Option<i64>) -> String {
    match version {
        Some(n) => format!("version {n}"),
        None => "no saved version".to_string(),
    }
}

/// The name of the way out an execution left by, read from the run's copy of the step — the live row
/// may have been renamed or deleted since. `None` where it has left by none, or by one the copy does not
/// hold.
fn left_by(defs: &[AutomationRunDef], m: &AutomationRunStep) -> Option<String> {
    let def = defs.iter().find(|d| d.id == m.run_def_id)?;
    let exits: Vec<amenbo_core::model::RunDefExit> = serde_json::from_str(&def.exits).ok()?;
    exits.into_iter().find(|e| Some(e.id) == m.exit_id).map(|e| e.name)
}

/// One value on one line, said from the side it was on: what came in, and what went out.
fn one_run_value(def: Option<&AutomationRunDef>, v: &AutomationRunValue) -> String {
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
    // The name the port had when the run launched, which is what the step was told and what it answered
    // to — the live port may since have been renamed or deleted.
    let name = def.and_then(|d| d.port_name(v.port_id)).unwrap_or_else(|| format!("output {}", v.port_id));
    format!("{way} {name} = {what}{from}")
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

#[cfg(test)]
mod tests {
    use super::tail_lines;

    #[test]
    fn a_tail_is_written_under_a_heading_and_nothing_is_written_for_none() {
        assert_eq!(
            tail_lines("error: no tests to run\nexit 4\n"),
            ["      output (the end of it):", "      > error: no tests to run", "      > exit 4"],
        );
        assert!(tail_lines("").is_empty());
        assert!(tail_lines("\n  \n").is_empty());
    }
}
