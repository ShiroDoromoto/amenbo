//! **One automation read in from a file** ([`crate::ops::automation_export`]) — always as new, never
//! matched against what is already here (`AMB-D-1033`). Names are not unique, and a file may come from
//! another person's Amenbo, so nothing in it is taken to be anything already in this store.
//!
//! **What the file names by place and by name is given new ids here.** A placement, an action and a
//! step are the place they stand in their list, and a way out, a port and a setting their name on the
//! box that declares them, so each is looked up in what this import has just written.
//!
//! **The actions are saved, the automation is not.** Each action a person wrote comes in as its version
//! 1, without the check a save asks (`AMB-D-1005`), and the placements stand on it; the automation is
//! left with no saved version, for whoever imported it to look over and save. A model, a path or a
//! script this machine does not have is taken in as written — the launch check is what finds it
//! (`AMB-D-987`).
//!
//! **A built-in is not read in.** The file names its key and the version it was placed at, and the
//! placement stands on this build's version of it (`AMB-D-1000`): a file written by an older build is
//! brought up to this one, and one written by a newer build is refused. The built-in that splits by an
//! axis takes the axis of the same name in the project imported into (`AMB-D-973`). A line or an answer
//! on a built-in naming a way out, a port or a setting this build's version does not declare is left
//! out, for the person to draw again.
//!
//! **The picture is put down as it was drawn.** The first placement is not held to the built-ins a run
//! starts at, nothing is added after a filed task, and the entry is the one the file names
//! (`AMB-D-977`).

use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::model::{
    Automation, AutomationAction, AutomationEnds, AutomationOwner, AutomationPictureOwner,
    AutomationPortDirection, AutomationPortOwner, ACTION_BOUNDARY, DONE_EXIT,
};
use crate::ops::automation::{self, ActionDef, EdgeTarget, NewAutomation, NewScript, NewStep};
use crate::ops::automation_builtin;
use crate::ops::automation_builtin_split::SPLIT_BY_DIM;
use crate::ops::automation_export::{
    ActionFile, AutomationFile, BuiltinFile, EdgeFile, ExitFile, PortFile, StandsOn, FORMAT,
};
use crate::store_engine::{read, WriteTx};

/// **Read a file's text** as an automation written out, refusing one that is not JSON of that shape.
/// Which format it says it is, is [`import`]'s to ask.
pub fn parse(text: &str) -> Result<AutomationFile> {
    serde_json::from_str(text)
        .map_err(|e| Error::invalid(format!("the file is not an automation written out by Amenbo: {e}")))
}

/// **Take one automation in as new**, into `project_id`: the actions a person wrote, then the
/// automation with its placements, the answers and choices written on them, and the lines between them.
///
/// Refused for a file of another format, for a built-in this build does not have or has at an older
/// version than the file, for an axis the project has no axis of that name for, and for a file whose
/// places and names do not hold together. The caller's transaction takes the refusal back whole.
pub fn import(tx: &WriteTx<'_>, project_id: i64, file: &AutomationFile) -> Result<Automation> {
    if file.format != FORMAT {
        return Err(Error::invalid(format!(
            "the file is '{}', and this Amenbo reads '{FORMAT}'",
            file.format
        )));
    }
    let actions = file
        .actions
        .iter()
        .map(|action| action_in(tx, project_id, action))
        .collect::<Result<Vec<_>>>()?;

    let automation =
        automation::add(tx, project_id, NewAutomation { name: file.name.clone(), notes: file.notes.clone() })?;
    let mut placed: Vec<Placed> = Vec::new();
    for placement in &file.placements {
        let (action_id, steps, builtin) = match &placement.stands_on {
            StandsOn::Action(at) => {
                let action = actions.get(*at).ok_or_else(|| no_such("action", *at))?;
                (action.id, action.steps.as_slice(), false)
            }
            StandsOn::Builtin(b) => (builtin_action(tx, project_id, b)?.id, &[][..], true),
        };
        let row = automation::placement_add_by_hand(tx, automation.id, action_id)?;
        let def = ActionDef::placed(tx.conn(), &row)?;
        for answer in &placement.settings {
            if builtin && !def.cfgs.iter().any(|c| c.name == answer.name) {
                continue;
            }
            let value = answer.value.as_ref().map(serde_json::to_string).transpose()?;
            automation::cfg_set(tx, row.id, &answer.name, value.as_deref())?;
        }
        // A built-in's step is carried out by Amenbo itself, so nobody is chosen for it.
        if !builtin {
            for chosen in &placement.agents {
                let step = *steps.get(chosen.step).ok_or_else(|| no_such("step", chosen.step))?;
                automation::placement_step_set(tx, row.id, step, &chosen.agent, chosen.model.as_deref())?;
            }
        }
        placed.push(Placed { id: row.id, def, builtin });
    }
    if let Some(entry) = file.entry {
        let entry = placed.get(entry).ok_or_else(|| no_such("placement", entry))?;
        automation::set_entry(tx, automation.id, Some(entry.id))?;
    }

    let at = |place: usize| placed.get(place).ok_or_else(|| no_such("placement", place));
    let on = AutomationPictureOwner::Automation;
    for edge in &file.edges {
        let from = at(edge.from)?;
        if from.builtin && from.exit(&edge.exit).is_none() {
            continue;
        }
        let target = target(edge, |to| Ok(at(to)?.id))?;
        automation::edge_add(tx, on, from.id, Some(&edge.exit), target, edge.max_times)?;
    }
    for wire in &file.wires {
        let (from, to) = (at(placement_end(wire.from)?)?, at(placement_end(wire.to)?)?);
        let from_exit = wire.from_exit.as_deref().unwrap_or(DONE_EXIT);
        let hands_on = from.exit(from_exit).is_some_and(|exit| {
            from.def.port_by_name(AutomationPortOwner::Exit, exit, AutomationPortDirection::Out, &wire.from_port).is_some()
        });
        let takes_in = to.def.port_by_name(
            AutomationPortOwner::Action,
            to.def.action_id,
            AutomationPortDirection::In,
            &wire.to_port,
        );
        if (from.builtin && !hands_on) || (to.builtin && takes_in.is_none()) {
            continue;
        }
        automation::wire_add(tx, on, from.id, Some(from_exit), &wire.from_port, to.id, &wire.to_port)?;
    }
    read::automation(tx.conn(), automation.id)?
        .ok_or_else(|| Error::not_found(format!("automation '{}' not found", automation.id)))
}

/// One placement this import has put down, with what it stands on as it stands on it.
struct Placed {
    id: i64,
    def: ActionDef,
    builtin: bool,
}

impl Placed {
    /// The id of the way out of the action standing here under this name.
    fn exit(&self, name: &str) -> Option<i64> {
        self.def.exit_by_name(AutomationOwner::Action, self.def.action_id, Some(name)).map(|x| x.id)
    }
}

/// An action this import has written, with its steps' ids in the file's order.
struct Written {
    id: i64,
    steps: Vec<i64>,
}

/// **Write one action a person wrote, and save it as its version 1** — without the check a save asks,
/// so that one naming what this machine does not have still comes in (`AMB-D-987`).
fn action_in(tx: &WriteTx<'_>, project_id: i64, file: &ActionFile) -> Result<Written> {
    let shelf = if file.global { None } else { Some(project_id) };
    let action = automation::action_add(tx, shelf, &file.name, &file.note)?;
    for setting in &file.settings {
        let options = setting.options.as_ref().map(serde_json::to_string).transpose()?;
        automation::cfg_add(tx, action.id, &setting.name, setting.kind, setting.required, options.as_deref())?;
    }
    inputs(tx, AutomationPortOwner::Action, action.id, &file.inputs)?;
    exits(tx, AutomationOwner::Action, action.id, &file.exits)?;

    let mut steps = Vec::new();
    for step in &file.steps {
        let written = automation::step_add(
            tx,
            action.id,
            NewStep {
                name: step.name.clone(),
                prompt: step.prompt.clone(),
                interactive: step.interactive,
                work_dir_ref: step.work_dir_ref.clone(),
                report_to_task: step.report_to_task,
                show_history: step.show_history,
                show_notes: step.show_notes,
                show_decisions: step.show_decisions,
                show_comments: step.show_comments,
                script: step.script.clone().map(|s| NewScript {
                    program: s.program,
                    args: s.args,
                    timeout_minutes: Some(s.timeout_minutes),
                }),
            },
        )?;
        inputs(tx, AutomationPortOwner::Step, written.id, &step.inputs)?;
        exits(tx, AutomationOwner::Step, written.id, &step.exits)?;
        steps.push(written.id);
    }
    let entry = file.entry_step.map(|at| steps.get(at).copied().ok_or_else(|| no_such("step", at))).transpose()?;
    automation::action_set_entry(tx, action.id, entry)?;
    if file.one_at_a_time {
        automation::action_set_one_at_a_time(tx, action.id, true)?;
    }

    let step_at = |place: usize| steps.get(place).copied().ok_or_else(|| no_such("step", place));
    let on = AutomationPictureOwner::Action;
    for edge in &file.edges {
        automation::edge_add(tx, on, step_at(edge.from)?, Some(&edge.exit), target(edge, step_at)?, edge.max_times)?;
    }
    for wire in &file.wires {
        let end = |place: Option<usize>| place.map_or(Ok(ACTION_BOUNDARY), step_at);
        automation::wire_add(
            tx,
            on,
            end(wire.from)?,
            wire.from_exit.as_deref(),
            &wire.from_port,
            end(wire.to)?,
            &wire.to_port,
        )?;
    }
    automation::action_version_add(tx, action.id)?;
    Ok(Written { id: action.id, steps })
}

/// Declare the inputs one step or the action itself takes.
fn inputs(tx: &WriteTx<'_>, owner: AutomationPortOwner, owner_id: i64, ports: &[PortFile]) -> Result<()> {
    for port in ports {
        automation::port_add(tx, owner, owner_id, AutomationPortDirection::In, &port.name, port.kind, port.required)?;
    }
    Ok(())
}

/// **Declare the ways out of one step or the action itself**, with what each hands on. The two every
/// box is born with are there already: [`DONE_EXIT`] is taken away where the file leaves it out, and
/// the error one, which nobody can take away, is always there.
fn exits(tx: &WriteTx<'_>, owner: AutomationOwner, owner_id: i64, exits: &[ExitFile]) -> Result<()> {
    if !exits.iter().any(|x| x.name == DONE_EXIT) {
        if let Some(done) = read::automation_exit_by_name(tx.conn(), owner, owner_id, Some(DONE_EXIT))? {
            automation::exit_delete(tx, done.id)?;
        }
    }
    for exit in exits {
        let row = match read::automation_exit_by_name(tx.conn(), owner, owner_id, Some(&exit.name))? {
            Some(born) => born,
            None => automation::exit_add(tx, owner, owner_id, Some(&exit.name))?,
        };
        for port in &exit.outputs {
            let out = AutomationPortDirection::Out;
            automation::port_add(tx, AutomationPortOwner::Exit, row.id, out, &port.name, port.kind, port.required)?;
        }
    }
    Ok(())
}

/// What an edge does once its way out is taken, its `to` given an id by `box_at`.
fn target(edge: &EdgeFile, box_at: impl Fn(usize) -> Result<i64>) -> Result<EdgeTarget> {
    Ok(match edge.ends {
        AutomationEnds::Go => {
            let to = edge.to.ok_or_else(|| Error::invalid(format!("the line out of '{}' goes nowhere", edge.exit)))?;
            EdgeTarget::Go(box_at(to)?)
        }
        AutomationEnds::Exit => EdgeTarget::Exit(edge.exit_to.clone()),
        AutomationEnds::Done => EdgeTarget::Done,
        AutomationEnds::Halt => EdgeTarget::Halt,
    })
}

/// A wire on an automation's picture joins two placements: there is no action around it to reach.
fn placement_end(place: Option<usize>) -> Result<usize> {
    place.ok_or_else(|| {
        Error::invalid("a wire on an automation joins two placements — the file names an end that is none")
    })
}

/// **The action this build has for a built-in the file names**, refused where it has none or has it at
/// an older version than the file was written at. The one that splits by an axis takes the project's
/// axis of the name the file gives.
fn builtin_action(tx: &WriteTx<'_>, project_id: i64, file: &BuiltinFile) -> Result<AutomationAction> {
    let here = automation_builtin::find(&file.key)
        .ok_or_else(|| Error::invalid(format!("this Amenbo has no built-in '{}'", file.key)))?;
    if file.version > here.version {
        return Err(Error::invalid(format!(
            "the built-in '{}' is at version {} in the file, newer than this Amenbo's {} — update Amenbo \
             before importing it",
            file.key, file.version, here.version
        )));
    }
    if file.key != SPLIT_BY_DIM.key {
        return automation_builtin::action(tx, &file.key);
    }
    let name = file.dimension.as_deref().ok_or_else(|| {
        Error::invalid(format!("the built-in '{}' in the file names no axis to split by", file.key))
    })?;
    let axis = axis_named(tx.conn(), project_id, name)?.ok_or_else(|| {
        Error::invalid(format!(
            "the built-in '{}' splits by '{name}', and this project has no axis called that",
            file.key
        ))
    })?;
    automation_builtin::action_on(tx, &file.key, Some(axis))
}

/// The project's axis of this name.
fn axis_named(conn: &Connection, project_id: i64, name: &str) -> Result<Option<i64>> {
    for id in read::dimension_ids_in_project(conn, project_id)? {
        if read::dimension(conn, id)?.is_some_and(|d| d.name == name) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// A place in one of the file's lists that the list does not have.
fn no_such(what: &str, place: usize) -> Error {
    Error::invalid(format!("the file names {what} {place}, and holds no {what} there"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AutomationCfgKind, AutomationPortKind, DimensionAppliesTo, DimensionCardinality, DimensionRole};
    use crate::ops::automation_export::export;
    use crate::ops::dimension::{self, NewDimension};
    use crate::ops::test_support::{exit_id, mk_project, with_tx};

    /// An automation written out: it takes a task, looks at it twice with an action a person wrote
    /// handing a value on, and splits by an axis.
    fn written_out(tx: &WriteTx<'_>, project: i64) -> AutomationFile {
        axis(tx, project, "職能", &["技術"]);
        let automation =
            automation::add(tx, project, NewAutomation { name: "1件やりきる".into(), notes: "メモ".into() })
                .expect("add automation");
        let take = automation_builtin::action(tx, "take_task").expect("built-in");
        let entry = automation::placement_add(tx, automation.id, take.id).expect("place the built-in");
        let action =
            automation::action_from_prompt(tx, Some(project), NewStep::new("点検", "見る"), &["直す".to_string()], &[])
                .expect("write the action");
        let (into, out_of, value) =
            (AutomationPortDirection::In, AutomationPortDirection::Out, AutomationPortKind::Value);
        let done = exit_id(tx, AutomationOwner::Action, action.id, None);
        automation::port_add(tx, AutomationPortOwner::Exit, done, out_of, "結果", value, false).expect("output");
        automation::port_add(tx, AutomationPortOwner::Action, action.id, into, "前から", value, false).expect("input");
        automation::cfg_add(tx, action.id, "深さ", AutomationCfgKind::Number, false, None).expect("setting");
        automation::action_set_one_at_a_time(tx, action.id, true).expect("one run at a time");
        automation::action_version_add(tx, action.id).expect("save the action");

        let first = automation::placement_add(tx, automation.id, action.id).expect("place it");
        let second = automation::placement_add(tx, automation.id, action.id).expect("place it again");
        let axis_id = axis_named(tx.conn(), project, "職能").expect("read").expect("axis");
        let split = automation_builtin::action_on(tx, SPLIT_BY_DIM.key, Some(axis_id)).expect("split");
        let third = automation::placement_add(tx, automation.id, split.id).expect("place the split");
        let on = AutomationPictureOwner::Automation;
        let took = automation_builtin::find("take_task").expect("take_task").exits[0].name;
        automation::edge_add(tx, on, entry.id, Some(took), EdgeTarget::Go(first.id), None).expect("edge");
        automation::edge_add(tx, on, first.id, None, EdgeTarget::Go(second.id), Some(3)).expect("edge");
        automation::edge_add(tx, on, second.id, None, EdgeTarget::Go(third.id), None).expect("edge");
        automation::edge_add(tx, on, third.id, Some("技術"), EdgeTarget::Done, None).expect("edge");
        automation::wire_add(tx, on, first.id, None, "結果", second.id, "前から").expect("wire");
        automation::cfg_set(tx, first.id, "深さ", Some("3")).expect("answer");
        let step = action.entry_step_id.expect("an entry step");
        automation::placement_step_set(tx, first.id, step, "claude", Some("opus")).expect("choose");
        automation::version_add(tx, automation.id).expect("save the automation");
        export(tx.conn(), automation.id).expect("export")
    }

    fn axis(tx: &WriteTx<'_>, project: i64, name: &str, values: &[&str]) -> i64 {
        let axis = dimension::add(
            tx,
            project,
            NewDimension {
                name: name.to_string(),
                notes: String::new(),
                cardinality: DimensionCardinality::Single,
                ordered: true,
                role: DimensionRole::None,
                show_on_card: false,
                required: false,
                applies_to: DimensionAppliesTo::Both,
                sequential: false,
                slug: None,
            },
        )
        .expect("axis")
        .id;
        for value in values {
            dimension::value_add(tx, axis, value, None).expect("value");
        }
        axis
    }

    fn count(tx: &WriteTx<'_>, table: &str) -> i64 {
        tx.conn().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).expect("count")
    }

    /// **What is read in is new, and writes back out as the file it came from**: the actions saved and
    /// the automation not, in another project with an axis of the same name.
    #[test]
    fn a_file_comes_in_as_new_and_writes_back_out_the_same() {
        with_tx(|tx| {
            let from = mk_project(tx, "amenbo");
            let file = written_out(tx, from);
            let text = serde_json::to_string(&file).expect("serialize");

            let into = mk_project(tx, "別のPJ");
            axis(tx, into, "職能", &["技術"]);
            let actions_before = count(tx, "automation_action");
            let imported = import(tx, into, &parse(&text).expect("parse")).expect("import");
            assert_eq!((imported.project_id, imported.name.as_str()), (into, "1件やりきる"));
            assert!(read::automation_version_latest(tx.conn(), imported.id).expect("read").is_none(), "not saved");
            assert!(automation::unsaved(tx.conn(), imported.id).expect("unsaved"), "left being written");

            let placements = read::automation_placement_ids(tx.conn(), imported.id).expect("placements");
            let written = read::automation_placement(tx.conn(), placements[1]).expect("read").expect("row");
            let action = read::automation_action(tx.conn(), written.action_id).expect("read").expect("action");
            assert_eq!((action.project_id, action.draft, written.version), (Some(into), false, Some(1)));
            let saved = read::automation_action_version(tx.conn(), action.id, 1).expect("read").expect("version 1");
            assert!(action.one_at_a_time && saved.one_at_a_time, "one run at a time comes in, saved");
            assert_eq!(count(tx, "automation_action"), actions_before + 2, "one written, one split on the new axis");

            automation::version_add(tx, imported.id).expect("save it");
            let back = export(tx.conn(), imported.id).expect("export again");
            assert_eq!(
                serde_json::to_value(&back).expect("json"),
                serde_json::to_value(&file).expect("json"),
                "the same file"
            );
        });
    }

    /// **A file of another format, a built-in newer than this build, and an axis the project lacks are
    /// refused** — and the caller's transaction takes back what was written before the refusal.
    #[test]
    fn what_cannot_be_read_in_is_refused() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let file = written_out(tx, project);
            assert!(matches!(parse("{"), Err(Error::Invalid(_))), "not JSON");

            let mut older = serde_json::to_value(&file).expect("json");
            older["actions"][0].as_object_mut().expect("an action").remove("one_at_a_time");
            let older = parse(&older.to_string()).expect("a file from before one_at_a_time");
            assert!(!older.actions[0].one_at_a_time, "an older file reads as off");

            let other = AutomationFile { format: "amenbo-automation/v0".into(), ..file.clone() };
            assert!(matches!(import(tx, project, &other), Err(Error::Invalid(_))), "another format");

            let mut newer = file.clone();
            if let StandsOn::Builtin(b) = &mut newer.placements[0].stands_on {
                b.version += 1;
            }
            assert!(matches!(import(tx, project, &newer), Err(Error::Invalid(_))), "a newer built-in");

            let elsewhere = mk_project(tx, "軸の無いPJ");
            assert!(matches!(import(tx, elsewhere, &file), Err(Error::Invalid(_))), "no axis of that name");
        });
    }

    /// **A built-in placed at an older version stands on this build's**, and a line on a way out that
    /// version does not declare is left out.
    #[test]
    fn an_older_built_in_stands_on_this_builds_version() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let mut file = written_out(tx, project);
            if let StandsOn::Builtin(b) = &mut file.placements[0].stands_on {
                b.version = 0;
            }
            file.edges[3].exit = "昔の出口".into();
            let imported = import(tx, project, &file).expect("import");
            let entry = imported.entry_placement_id.expect("an entry");
            let placed = read::automation_placement(tx.conn(), entry).expect("read").expect("row");
            let take = automation_builtin::action(tx, "take_task").expect("built-in");
            assert_eq!(placed.action_id, take.id, "this build's version");
            let edges = read::automation_edges_of(tx.conn(), AutomationPictureOwner::Automation, imported.id)
                .expect("edges");
            assert_eq!(edges.len(), 3, "the line on a way out the split does not have is left out");
        });
    }
}
