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

use serde_json::{json, Value};

use amenbo_core::model::{
    AutomationCfgKind, AutomationOwner, AutomationPortDirection, AutomationPortKind,
    DEFAULT_MAX_TIMES,
};
use amenbo_core::model::{
    AutomationRun, AutomationRunDef, AutomationRunStep, AutomationRunTask, AutomationRunValue,
};
use amenbo_core::model::AttachmentTarget;
use amenbo_core::ops::automation_stop::Ending;
use amenbo_core::model::AutomationPictureOwner;
use amenbo_core::ops::automation::{lines_back, EdgeTarget, NewAutomation, NewStep};
use amenbo_core::ops::automation_report::{Next, Produced};
use amenbo_core::ops::automation_run::Launcher;
use amenbo_core::ops::automation_stop::{Paused, Resumed};
use amenbo_core::ops::automation_view::{ActionView, AutomationView, PlacementView, StepView};
use amenbo_core::time::Timestamp;
use amenbo_core::Store;

use crate::cli::*;
use crate::cmd::arg::{body_arg, body_arg_opt};
use crate::cmd::labels::task_label;
use crate::cmd::place::project_or_bound;
use crate::cmd::task::resolve_task;
use crate::output::{confirm, human, print_json, write_envelope, CliError, Flags};

/// Where an edge or a wire leaves from, as one token: `<box>:<way out>`. `4` and `4:` are both the
/// unnamed way out, `4:*` the error one.
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
        hint: Some("Write it as <box>:<way out> — `4:` is the unnamed way out, `4:*` the error one.".to_string()),
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
/// the action the picture is inside, bare for its unnamed way out; the other two end the run.
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
        if !amenbo_core::store_engine::read::is_task_sort(sort) {
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
    // of a run, days later.
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

/// **Where one of these verbs may be typed** (`AMB-D-948`): inside the terminal a run opened for a
/// step, outside any run, or either side.
///
/// **It is declared per verb rather than read off the namespace they sit under.** What is being kept
/// apart — building a picture, against a running step reporting on itself — is not the axis the
/// namespaces divide on: both are Amenbo > Automation > a step, so both land in the same name.
enum Where {
    /// Inside a run and nowhere else: the three a step reports through. Outside one there is no step
    /// for them to speak for.
    InsideARun,
    /// Either side. The reading verbs are left with a step so it can see where it stands, and refusing
    /// them would buy nothing.
    EitherSide,
    /// Outside a run and nowhere else. A step that could start a run would let a run make runs; one
    /// that could rewrite a definition would change what the *next* run is built out of, with the run
    /// under way unmoved — a change made where the person who started it is not looking.
    OutsideARun,
}

/// Which side each verb belongs on, as a match over every one of them.
///
/// **There is no `_` arm, deliberately.** A verb added later does not compile until this file says
/// where it may be typed, which is what keeps the declaration from drifting behind the command list.
fn typed_in(sub: &AutomationCmd) -> Where {
    match sub {
        AutomationCmd::StepTake { .. } | AutomationCmd::StepOut { .. } | AutomationCmd::StepDone { .. } => Where::InsideARun,

        // `action-list` and `action-show` are on this side because the prompts are: a definition is
        // read back over two commands now, `show` for the automation and the placements on it and
        // `action-show` for the steps inside one, and an agent that could reach only the first would
        // be left unable to read the action it is carrying out a step of.
        AutomationCmd::List { .. }
        | AutomationCmd::Show { .. }
        | AutomationCmd::ActionList { .. }
        | AutomationCmd::ActionShow { .. }
        | AutomationCmd::RunList { .. }
        | AutomationCmd::RunShow { .. } => Where::EitherSide,

        AutomationCmd::Add { .. }
        | AutomationCmd::Update { .. }
        | AutomationCmd::Rm { .. }
        | AutomationCmd::EntrySet { .. }
        | AutomationCmd::PlaceAdd { .. }
        | AutomationCmd::PlaceRm { .. }
        | AutomationCmd::ActionAdd { .. }
        | AutomationCmd::ActionUpdate { .. }
        | AutomationCmd::ActionEntrySet { .. }
        | AutomationCmd::ActionScopeSet { .. }
        | AutomationCmd::ActionRm { .. }
        | AutomationCmd::StepAdd { .. }
        | AutomationCmd::StepUpdate { .. }
        | AutomationCmd::StepRm { .. }
        | AutomationCmd::ExitAdd { .. }
        | AutomationCmd::ExitRename { .. }
        | AutomationCmd::ExitRm { .. }
        | AutomationCmd::PortAdd { .. }
        | AutomationCmd::PortUpdate { .. }
        | AutomationCmd::PortRm { .. }
        | AutomationCmd::CfgAdd { .. }
        | AutomationCmd::CfgUpdate { .. }
        | AutomationCmd::CfgSet { .. }
        | AutomationCmd::AgentSet { .. }
        | AutomationCmd::CfgRm { .. }
        | AutomationCmd::EdgeAdd { .. }
        | AutomationCmd::EdgeUpdate { .. }
        | AutomationCmd::EdgeRm { .. }
        | AutomationCmd::WireAdd { .. }
        | AutomationCmd::WireRm { .. }
        | AutomationCmd::Start { .. }
        | AutomationCmd::Pause { .. }
        | AutomationCmd::Resume { .. }
        | AutomationCmd::Stop { .. } => Where::OutsideARun,
    }
}

pub(crate) fn automation(store: &mut Store, flags: &Flags, sub: AutomationCmd) -> Result<i32, CliError> {
    // The place is checked before anything is read or written, so a refusal here has changed nothing.
    match typed_in(&sub) {
        // `speaking_for` is where the other direction is refused, in the sentence it has for it — and
        // the arms below read the step off it again.
        Where::InsideARun => {
            speaking_for()?;
        }
        Where::OutsideARun => {
            if amenbo_core::env::automation_step().is_some() {
                return Err(CliError::automation_outside_only());
            }
        }
        Where::EitherSide => {}
    }
    match sub {
        AutomationCmd::Add { project, name, notes } => {
            let pid = project_or_bound(store, project)?;
            let new = NewAutomation { name, notes: body_arg(notes)? };
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
                    human(
                        flags,
                        format!(
                            "  {}  {}  {} placement(s){archived}",
                            card.automation.id, card.automation.name, card.placements
                        ),
                    );
                }
            }
        }
        AutomationCmd::Show { id } => {
            let view = store.automation_detail(id).map_err(CliError::from)?.ok_or_else(|| {
                CliError::from(amenbo_core::Error::not_found(format!(
                    "automation '{id}' not found"
                )))
            })?;
            if flags.json {
                print_json(&serde_json::to_value(&view).unwrap());
            } else {
                render_automation(flags, &view);
            }
        }
        AutomationCmd::Update { id, name, notes, archived } => {
            let notes = body_arg_opt(notes)?;
            let a = store
                .automation_update(id, name.as_deref(), notes.as_deref(), archived)
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
        AutomationCmd::EntrySet { id, placement, clear } => {
            if placement.is_none() && !clear {
                return Err(CliError {
                    code: "invalid_value",
                    message: "name the placement a run starts at with --placement, or --clear to leave none.".to_string(),
                    hint: None,
                    exit: 2,
                });
            }
            let a = store.automation_set_entry(id, placement).map_err(CliError::from)?;
            let line = match a.entry_placement_id {
                Some(p) => format!("✓ Automation {} starts at placement {p}", a.id),
                None => format!("✓ Automation {} starts nowhere", a.id),
            };
            write_envelope(flags, "automation.entry-set", "automation", serde_json::to_value(&a).unwrap(), Some(vec!["entry_placement_id".to_string()]), false, line);
        }
        AutomationCmd::PlaceAdd { automation, action } => {
            let p = store.automation_placement_add(automation, action).map_err(CliError::from)?;
            write_envelope(flags, "automation.place-add", "automation_placement", serde_json::to_value(&p).unwrap(), None, false, format!("✓ Placed action {action} on automation {automation} ({})", p.id));
        }
        AutomationCmd::PlaceRm { id } => {
            if !confirm(flags, "take placement off")? {
                return Ok(0);
            }
            store.automation_placement_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.place-rm", "automation_placement", json!({ "id": id, "deleted": true }), None, false, format!("✓ Took placement off: {id}"));
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
                    human(
                        flags,
                        format!(
                            "  {}  {}{shelf}  {} step(s)  used by {} automation(s)",
                            card.action.id, card.action.name, card.steps, card.used_by
                        ),
                    );
                }
            }
        }
        AutomationCmd::ActionShow { id } => {
            let view = store.automation_action_detail(id).map_err(CliError::from)?.ok_or_else(|| {
                CliError::from(amenbo_core::Error::not_found(format!("action '{id}' not found")))
            })?;
            if flags.json {
                print_json(&serde_json::to_value(&view).unwrap());
            } else {
                render_action(flags, &view);
            }
        }
        AutomationCmd::ActionUpdate { id, name, note } => {
            let note = body_arg_opt(note)?;
            let a =
                store.automation_action_update(id, name.as_deref(), note.as_deref()).map_err(CliError::from)?;
            write_envelope(flags, "automation.action-update", "automation_action", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Updated action: {} ({})", a.name, a.id));
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
            write_envelope(flags, "automation.action-entry-set", "automation_action", serde_json::to_value(&a).unwrap(), Some(vec!["entry_step_id".to_string()]), false, line);
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
        AutomationCmd::StepAdd { action, name, prompt, interactive, work_dir, report_to_task, no_history } => {
            let prompt = body_arg(prompt)?;
            let new = NewStep {
                name,
                prompt,
                interactive,
                work_dir_ref: work_dir,
                report_to_task,
                // The run's story so far is handed on unless somebody turns it off, so the flag that
                // takes it away is the one written.
                show_history: !no_history,
            };
            let s = store.automation_step_add(action, new).map_err(CliError::from)?;
            write_envelope(flags, "automation.step-add", "automation_step", serde_json::to_value(&s).unwrap(), None, false, format!("✓ Added step: {} ({})", s.name, s.id));
        }
        AutomationCmd::StepUpdate { id, name, prompt, interactive, work_dir, clear_work_dir, report_to_task, history } => {
            let prompt = body_arg_opt(prompt)?;
            let work_dir = match clear_work_dir {
                true => Some(None),
                false => work_dir.as_deref().map(Some),
            };
            let s = store
                .automation_step_update(id, name.as_deref(), prompt.as_deref(), interactive, work_dir, report_to_task, history)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.step-update", "automation_step", serde_json::to_value(&s).unwrap(), None, false, format!("✓ Updated step: {} ({})", s.name, s.id));
        }
        AutomationCmd::StepRm { id } => {
            if !confirm(flags, "delete step")? {
                return Ok(0);
            }
            store.automation_step_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.step-rm", "automation_step", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted step: {id}"));
        }
        AutomationCmd::ExitAdd { step, action, name } => {
            let (owner_kind, owner_id) = declarer_from_flags(step, action)?;
            let e = store.automation_exit_add(owner_kind, owner_id, Some(&name)).map_err(CliError::from)?;
            write_envelope(flags, "automation.exit-add", "automation_exit", serde_json::to_value(&e).unwrap(), None, false, format!("✓ Added way out: {} ({})", name, e.id));
        }
        AutomationCmd::ExitRename { id, name, clear } => {
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
        AutomationCmd::ExitRm { id } => {
            if !confirm(flags, "delete way out")? {
                return Ok(0);
            }
            store.automation_exit_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.exit-rm", "automation_exit", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted way out: {id}"));
        }
        AutomationCmd::PortAdd { step, action, exit, name, kind, required } => {
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
        AutomationCmd::PortUpdate { id, name, kind, required } => {
            let kind = kind.as_deref().map(parse_port_kind).transpose()?;
            let p = store.automation_port_update(id, name.as_deref(), kind, required).map_err(CliError::from)?;
            write_envelope(flags, "automation.port-update", "automation_port", serde_json::to_value(&p).unwrap(), None, false, format!("✓ Updated port: {} ({})", p.name, p.id));
        }
        AutomationCmd::PortRm { id } => {
            if !confirm(flags, "delete port")? {
                return Ok(0);
            }
            store.automation_port_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.port-rm", "automation_port", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted port: {id}"));
        }
        AutomationCmd::CfgAdd { action, name, kind, required, options } => {
            let kind = parse_cfg_kind(&kind)?;
            let c = store
                .automation_cfg_add(action, &name, kind, required, options.as_deref())
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.cfg-add", "automation_cfg", serde_json::to_value(&c).unwrap(), None, false, format!("✓ Declared setting: {} ({})", c.name, c.id));
        }
        AutomationCmd::CfgUpdate { id, name, kind, required, options, clear_options } => {
            let kind = kind.as_deref().map(parse_cfg_kind).transpose()?;
            let options = match clear_options {
                true => Some(None),
                false => options.as_deref().map(Some),
            };
            let c = store.automation_cfg_update(id, name.as_deref(), kind, required, options).map_err(CliError::from)?;
            write_envelope(flags, "automation.cfg-update", "automation_cfg", serde_json::to_value(&c).unwrap(), None, false, format!("✓ Updated setting: {} ({})", c.name, c.id));
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
            write_envelope(flags, "automation.cfg-set", "automation_cfg", serde_json::to_value(&c).unwrap(), Some(vec!["value".to_string()]), false, line);
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
                    write_envelope(flags, "automation.agent-set", "automation_placement_step", serde_json::to_value(&c).unwrap(), None, false, line);
                }
                None => {
                    store.automation_placement_step_clear(placement, step).map_err(CliError::from)?;
                    write_envelope(
                        flags,
                        "automation.agent-set",
                        "automation_placement_step",
                        json!({ "placement_id": placement, "step_id": step, "cleared": true }),
                        None,
                        false,
                        format!("✓ Left nobody chosen for step {step} at placement {placement}"),
                    );
                }
            }
        }
        AutomationCmd::CfgRm { id } => {
            if !confirm(flags, "delete setting")? {
                return Ok(0);
            }
            store.automation_cfg_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.cfg-rm", "automation_cfg", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted setting: {id}"));
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
            write_envelope(flags, "automation.edge-add", "automation_edge", serde_json::to_value(&e).unwrap(), None, false, format!("✓ Added edge: {} ({})", from, e.id));
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
            write_envelope(flags, "automation.edge-update", "automation_edge", serde_json::to_value(&e).unwrap(), None, false, format!("✓ Updated edge: {}", e.id));
        }
        AutomationCmd::EdgeRm { id } => {
            if !confirm(flags, "delete edge")? {
                return Ok(0);
            }
            store.automation_edge_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.edge-rm", "automation_edge", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted edge: {id}"));
        }
        AutomationCmd::WireAdd { in_action, from, from_port, to, to_port } => {
            let (from_id, exit_name) = parse_point(&from)?;
            let w = store
                .automation_wire_add(picture(in_action), from_id, exit_name.as_deref(), &from_port, to, &to_port)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.wire-add", "automation_wire", serde_json::to_value(&w).unwrap(), None, false, format!("✓ Added wire: {from}.{from_port} → {to}.{to_port} ({})", w.id));
        }
        AutomationCmd::WireRm { id } => {
            if !confirm(flags, "delete wire")? {
                return Ok(0);
            }
            store.automation_wire_delete(id).map_err(CliError::from)?;
            write_envelope(flags, "automation.wire-rm", "automation_wire", json!({ "id": id, "deleted": true }), None, false, format!("✓ Deleted wire: {id}"));
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

        AutomationCmd::Start { id } => {
            let known = startable(store);
            let by = Launcher {
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
            let r = store.automation_launch(id, &by).map_err(CliError::from)?;
            let line = format!("✓ Run {} started", r.id);
            write_envelope(flags, "automation.start", "automation_run", serde_json::to_value(&r).unwrap(), None, false, line);
        }
        AutomationCmd::Pause { run } => {
            let paused = store.automation_pause(run).map_err(CliError::from)?;
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
            let Resumed { run: r, next } = store.automation_resume(run).map_err(CliError::from)?;
            let value = json!({ "run": r.id, "state": "running", "step": next.id });
            let line = format!("✓ Run {} picks up at {} ({})", r.id, next.name, next.id);
            write_envelope(flags, "automation.resume", "automation_run", value, None, false, line);
        }
        AutomationCmd::Stop { run } => {
            let ended = store
                .automation_stop(run, Ending::Canceled)
                .map_err(CliError::from)?;
            write_envelope(flags, "automation.stop", "automation_run", serde_json::to_value(&ended.run).unwrap(), None, false, format!("✓ Run {} canceled", ended.run.id));
        }

        AutomationCmd::StepTake { task } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let t = store.automation_take(speaking_for()?, tid).map_err(CliError::from)?;
            write_envelope(flags, "automation.take", "task", serde_json::to_value(&t).unwrap(), None, false, format!("✓ Took {} — {}", task_label(t.id), t.title));
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
        message: "this is not a step of a run — `take`, `out` and `done` are typed by the agent a step opened, in the terminal the run opened for it.".to_string(),
        hint: Some("start a run with `automation start <id>`, and the step's own terminal carries what these commands need".to_string()),
        exit: 2,
    })
}

/// **The agent ids this machine was last seen able to start**, or `None` where it has never been
/// asked (`AMB-D-792`) — [`amenbo_core::wake::startable_ids`], which every launching surface asks
/// rather than each deriving it from the remembered commands.
///
/// `None` reaches the launch check as "not asked", which leaves the agent check unmade rather than
/// failing every step on a machine nobody has probed.
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
/// **The task the run is about does not come this way and is refused here**, with the command that does
/// take it. Reserving it and declaring it are one act, because two would leave a task `in_progress`
/// that nothing can hand back where the agent died in between — so there is no way to say it with
/// `out`, and being told that by the kind check would not say what to type instead.
fn hand_on(store: &mut Store, step: i64, one: &str) -> Result<AutomationRunValue, CliError> {
    let (port, text) = parse_produced(one)?;
    match store.automation_out_kind(step, port).map_err(CliError::from)? {
        Some(AutomationPortKind::TaskTake) => Err(CliError {
            code: "invalid_value",
            message: format!(
                "output {port} is the task this step takes, which is reserved and handed on in one act"
            ),
            hint: Some(format!(
                "take it with `{} automation step-take <task>`",
                amenbo_core::config::Paths::command_name()
            )),
            exit: 2,
        }),
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
fn render_automation(flags: &Flags, view: &AutomationView) {
    let a = &view.automation;
    human(flags, format!("Automation {}  {}", a.id, a.name));
    let entry = match a.entry_placement_id {
        Some(placement) => format!("starts at placement {placement}"),
        None => "starts nowhere".to_string(),
    };
    let archived = if a.archived { "  archived" } else { "" };
    human(
        flags,
        format!(
            "project {}  {}  {} placement(s){archived}",
            a.project_id,
            entry,
            view.placements.len()
        ),
    );
    write_body(flags, "notes", &a.notes);
    for placement in &view.placements {
        render_placement(flags, view, placement);
    }
}

/// One placement: which action stands there, what it takes, what it is set to, and what happens after
/// each way out.
fn render_placement(flags: &Flags, view: &AutomationView, placement: &PlacementView) {
    let row = &placement.placement;
    let boxes: Vec<i64> = view.placements.iter().map(|p| p.placement.id).collect();
    let back = lines_back(view.automation.entry_placement_id, &boxes, &view.edges);
    let named = match &placement.action {
        Some(action) => format!("action {} ({})", action.id, action.name),
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
        let who = match &one.chosen {
            Some(c) => match &c.model {
                Some(model) => format!("carried out by {} ({model})", c.agent),
                None => format!("carried out by {}", c.agent),
            },
            None => "nobody chosen to carry it out".to_string(),
        };
        human(flags, format!("    step {} {}  {who}", one.step.id, one.step.name));
    }
    for exit in &placement.exits {
        human(flags, format!("    way out {}  [{}]", one_exit(exit.exit.name.as_deref()), exit.exit.id));
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
    let entry = match a.entry_step_id {
        Some(step) => format!("opens step {step} first"),
        None => "opens nothing".to_string(),
    };
    human(
        flags,
        format!("{shelf}  {entry}  used by {} automation(s)", view.used_by),
    );
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
        human(flags, format!("way out {}", one_exit(exit.exit.name.as_deref())));
        for port in &exit.outputs {
            human(flags, format!("    hands on  {}", one_port(port)));
        }
    }
    for step in &view.steps {
        render_step(flags, view, step);
    }
}

/// One step inside an action: the prompt it runs on, what it takes, and what happens after each way
/// out.
fn render_step(flags: &Flags, view: &ActionView, step: &StepView) {
    let boxes: Vec<i64> = view.steps.iter().map(|s| s.step.id).collect();
    let back = lines_back(view.action.entry_step_id, &boxes, &view.edges);
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
        human(flags, format!("    way out {}  [{}]", one_exit(exit.exit.name.as_deref()), exit.exit.id));
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

fn one_exit(name: Option<&str>) -> String {
    match name {
        Some(amenbo_core::model::ERROR_EXIT) => "the error one".to_string(),
        Some(name) => format!("\"{name}\""),
        None => "the unnamed one".to_string(),
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

/// One port on one line: the name it is handed under, what it carries, and whether it may be missing.
fn one_port(port: &amenbo_core::model::AutomationPort) -> String {
    let required = if port.required { "required" } else { "optional" };
    format!("{}  {}  {required}", port.name, port.kind.as_str())
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
                Some(e) => match e.exit.name.as_deref() {
                    Some(name) => format!("out of the action by '{name}'"),
                    None => "out of the action by its unnamed way out".to_string(),
                },
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

/// The name of the way out an execution left by, read from the run's copy of the step — the live row
/// may have been renamed or deleted since. `None` is the unnamed one.
fn left_by(defs: &[AutomationRunDef], m: &AutomationRunStep) -> Option<String> {
    let def = defs.iter().find(|d| d.id == m.run_def_id)?;
    let exits: Vec<amenbo_core::model::RunDefExit> = serde_json::from_str(&def.exits).ok()?;
    exits.into_iter().find(|e| Some(e.id) == m.exit_id).and_then(|e| e.name)
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
