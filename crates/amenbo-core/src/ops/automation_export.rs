//! **One automation written out as one file** — its newest saved version, with the actions it places,
//! so that the file stands on its own on another store.
//!
//! **No id is written.** Ids are this store's; on another one they name nothing, or the wrong thing. A
//! placement, an action and a step are named by where they stand in their list, and a way out, a port
//! and a setting by their name, which is unique on the box that declares it.
//!
//! **What is saved, not what is being written.** The automation is read off its newest saved version, and
//! each action a person wrote off the saved version its placement stands on. An automation nobody has
//! saved, and a placement on an action nobody has saved, are refused.
//!
//! **A built-in is named, not written out.** Its rows are Amenbo's own definition, so the file carries its
//! key and the version it was placed at — and, for the one that splits by an axis, the axis's name.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::{
    AutomationCfgKind, AutomationEnds, AutomationExit, AutomationOwner, AutomationPort,
    AutomationPortDirection, AutomationPortKind, AutomationPortOwner, StepScript, ACTION_BOUNDARY,
};
use crate::ops::automation::{ActionDef, Picture};
use crate::store_engine::read;

/// What the file says it is, and which shape of it — the number moves when the shape does.
pub const FORMAT: &str = "amenbo-automation/v1";

/// **One automation, written out.**
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationFile {
    /// [`FORMAT`].
    pub format: String,
    /// The Amenbo that wrote it.
    pub amenbo_version: String,
    pub name: String,
    pub notes: String,
    /// The placement a run opens first, by its place in `placements`.
    pub entry: Option<usize>,
    pub placements: Vec<PlacementFile>,
    pub edges: Vec<EdgeFile>,
    pub wires: Vec<WireFile>,
    /// The actions a person wrote that the placements stand on, each once per version placed.
    pub actions: Vec<ActionFile>,
}

/// **One placement**: what it stands on, the answers written for its settings and who carries out each
/// of its steps.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlacementFile {
    pub stands_on: StandsOn,
    pub settings: Vec<AnswerFile>,
    pub agents: Vec<AgentFile>,
}

/// What a placement stands on.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandsOn {
    /// An action in the file, by its place in `actions`.
    Action(usize),
    Builtin(BuiltinFile),
}

/// **A built-in, by its key and the version it was placed at.**
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuiltinFile {
    pub key: String,
    pub version: i64,
    /// The axis it splits by, by name, for the built-in whose ways out are an axis's values; `None` on
    /// every other one, and on one whose axis has been deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
}

/// The answer written for one setting where it is placed, as JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnswerFile {
    pub name: String,
    pub value: Option<serde_json::Value>,
}

/// Who carries out one step of a placement, the step by its place in its action's `steps`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentFile {
    pub step: usize,
    pub agent: String,
    pub model: Option<String>,
}

/// **What happens after one box leaves by one way out.** `from` and `to` are places in the picture's
/// list — the placements on an automation, the steps inside an action.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeFile {
    pub from: usize,
    pub exit: String,
    pub ends: AutomationEnds,
    /// Where it goes, for `ends = go`.
    pub to: Option<usize>,
    /// The way out of the action the run leaves by, for `ends = exit`.
    pub exit_to: Option<String>,
    pub max_times: Option<i64>,
}

/// **What is handed from one box to the next.** Inside an action, `None` at either end is the action
/// itself: out of it comes an input it declares, into it goes an output on its way out.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WireFile {
    pub from: Option<usize>,
    /// The way out the wire leaves by — `None` out of the action itself.
    pub from_exit: Option<String>,
    pub from_port: String,
    pub to: Option<usize>,
    pub to_port: String,
}

/// **One action a person wrote**, as the version placed holds it. Its name and note are the action's
/// own, which no version holds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionFile {
    pub name: String,
    pub note: String,
    /// Held by the device and reachable from every project, rather than by the automation's project.
    pub global: bool,
    /// The step it opens first, by its place in `steps`.
    pub entry_step: Option<usize>,
    pub settings: Vec<SettingFile>,
    pub inputs: Vec<PortFile>,
    pub exits: Vec<ExitFile>,
    pub steps: Vec<StepFile>,
    pub edges: Vec<EdgeFile>,
    pub wires: Vec<WireFile>,
}

/// A setting an action declares.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingFile {
    pub name: String,
    pub kind: AutomationCfgKind,
    pub required: bool,
    /// The choices, for `kind = choice`.
    pub options: Option<serde_json::Value>,
}

/// An input, or an output on a way out.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortFile {
    pub name: String,
    pub kind: AutomationPortKind,
    pub required: bool,
}

/// A way out, with what it hands on.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExitFile {
    pub name: String,
    pub outputs: Vec<PortFile>,
}

/// One step inside an action.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepFile {
    pub name: String,
    pub prompt: String,
    pub script: Option<StepScript>,
    pub interactive: bool,
    pub work_dir_ref: Option<String>,
    pub report_to_task: bool,
    pub show_history: bool,
    pub show_notes: bool,
    pub show_decisions: bool,
    pub show_comments: bool,
    pub inputs: Vec<PortFile>,
    pub exits: Vec<ExitFile>,
}

/// **Write out the newest saved version of one automation**, with the actions it places.
///
/// Refused as not found for an automation that is not there, and as invalid for one nobody has saved and
/// for one whose saved version places an action a person wrote that nobody has saved.
pub fn export(conn: &Connection, automation_id: i64) -> Result<AutomationFile> {
    let automation = read::automation(conn, automation_id)?
        .ok_or_else(|| Error::not_found(format!("automation '{automation_id}' not found")))?;
    let saved = read::automation_version_latest(conn, automation_id)?.ok_or_else(|| {
        Error::invalid(format!(
            "automation '{}' has never been saved — only a saved version is written out",
            automation.name
        ))
    })?;
    let picture = Picture::saved(&saved)?;

    let mut actions: Vec<ActionFile> = Vec::new();
    let mut action_at: BTreeMap<(i64, i64), usize> = BTreeMap::new();
    let mut defs = Vec::new();
    let mut placements = Vec::new();
    for placement in &picture.placements {
        let action = read::automation_action(conn, placement.action_id)?.ok_or_else(|| {
            Error::not_found(format!(
                "placement '{}' stands on action '{}', which has been deleted",
                placement.id, placement.action_id
            ))
        })?;
        let def = ActionDef::placed(conn, placement)?;
        let stands_on = match (&action.builtin, placement.version) {
            (Some(key), _) => StandsOn::Builtin(BuiltinFile {
                key: key.clone(),
                version: action.builtin_version.ok_or_else(|| {
                    Error::invalid(format!("built-in action '{}' carries no version", action.name))
                })?,
                dimension: match action.builtin_dimension_id {
                    Some(id) => read::dimension(conn, id)?.map(|d| d.name),
                    None => None,
                },
            }),
            (None, None) => {
                return Err(Error::invalid(format!(
                    "action '{}' has never been saved — only a saved version is written out",
                    action.name
                )))
            }
            (None, Some(version)) => {
                let at = *action_at.entry((action.id, version)).or_insert_with(|| actions.len());
                if at == actions.len() {
                    actions.push(action_file(&action.name, &action.note, action.project_id.is_none(), &def)?);
                }
                StandsOn::Action(at)
            }
        };
        let settings = picture
            .placement_cfgs(placement.id)
            .map(|c| Ok(AnswerFile { name: c.name.clone(), value: json(c.value.as_deref())? }))
            .collect::<Result<Vec<_>>>()?;
        let agents = picture
            .placement_steps_of(placement.id)
            .filter_map(|c| {
                let step = def.steps.iter().position(|s| s.id == c.step_id)?;
                Some(AgentFile { step, agent: c.agent.clone(), model: c.model.clone() })
            })
            .collect();
        placements.push(PlacementFile { stands_on, settings, agents });
        defs.push(def);
    }

    let at = |id: i64| picture.placements.iter().position(|p| p.id == id);
    let mut edges = Vec::new();
    for edge in picture.edges() {
        let Some(from) = at(edge.from_id) else { continue };
        edges.push(EdgeFile {
            from,
            exit: exit_name(&defs[from].exits, edge.exit_id)?,
            ends: edge.ends,
            to: edge.to_id.and_then(at),
            exit_to: None,
            max_times: edge.max_times,
        });
    }
    let mut wires = Vec::new();
    for wire in picture.wires() {
        let (Some(from), Some(to)) = (at(wire.from_id), at(wire.to_id)) else { continue };
        wires.push(WireFile {
            from: Some(from),
            from_exit: wire.from_exit_id.map(|id| exit_name(&defs[from].exits, id)).transpose()?,
            from_port: port_name(&defs[from].ports, wire.from_port_id)?,
            to: Some(to),
            to_port: port_name(&defs[to].ports, wire.to_port_id)?,
        });
    }

    Ok(AutomationFile {
        format: FORMAT.to_string(),
        amenbo_version: crate::export::APP_VERSION.to_string(),
        name: automation.name,
        notes: automation.notes,
        entry: picture.entry_placement_id.and_then(at),
        placements,
        edges,
        wires,
        actions,
    })
}

/// One action a person wrote, as `def` holds it.
fn action_file(name: &str, note: &str, global: bool, def: &ActionDef) -> Result<ActionFile> {
    let at = |id: i64| def.steps.iter().position(|s| s.id == id);
    let (into, out_of) = (AutomationPortDirection::In, AutomationPortDirection::Out);
    let exits_of = |owner_kind: AutomationOwner, owner_id: i64| -> Vec<ExitFile> {
        def.exits_of(owner_kind, owner_id)
            .into_iter()
            .map(|x| ExitFile {
                outputs: ports(&def.ports_of(AutomationPortOwner::Exit, x.id, out_of)),
                name: x.name,
            })
            .collect()
    };
    let steps = def
        .steps
        .iter()
        .map(|s| StepFile {
            name: s.name.clone(),
            prompt: s.prompt.clone(),
            script: s.script.clone(),
            interactive: s.interactive,
            work_dir_ref: s.work_dir_ref.clone(),
            report_to_task: s.report_to_task,
            show_history: s.show_history,
            show_notes: s.show_notes,
            show_decisions: s.show_decisions,
            show_comments: s.show_comments,
            inputs: ports(&def.ports_of(AutomationPortOwner::Step, s.id, into)),
            exits: exits_of(AutomationOwner::Step, s.id),
        })
        .collect();
    let settings = def
        .cfgs
        .iter()
        .map(|c| {
            Ok(SettingFile {
                name: c.name.clone(),
                kind: c.kind,
                required: c.required,
                options: json(c.options.as_deref())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut edges = Vec::new();
    for edge in &def.edges {
        let Some(from) = at(edge.from_id) else { continue };
        edges.push(EdgeFile {
            from,
            exit: exit_name(&def.exits, edge.exit_id)?,
            ends: edge.ends,
            to: edge.to_id.and_then(at),
            exit_to: edge.exit_to_id.map(|id| exit_name(&def.exits, id)).transpose()?,
            max_times: edge.max_times,
        });
    }
    let mut wires = Vec::new();
    for wire in &def.wires {
        // An end at the action itself is `None`; an end at a step that is not there drops the wire.
        let end = |id: i64| if id == ACTION_BOUNDARY { Some(None) } else { at(id).map(Some) };
        let (Some(from), Some(to)) = (end(wire.from_id), end(wire.to_id)) else { continue };
        wires.push(WireFile {
            from,
            from_exit: wire.from_exit_id.map(|id| exit_name(&def.exits, id)).transpose()?,
            from_port: port_name(&def.ports, wire.from_port_id)?,
            to,
            to_port: port_name(&def.ports, wire.to_port_id)?,
        });
    }
    Ok(ActionFile {
        name: name.to_string(),
        note: note.to_string(),
        global,
        entry_step: def.entry_step_id.and_then(at),
        settings,
        inputs: ports(&def.action_inputs()),
        exits: exits_of(AutomationOwner::Action, def.action_id),
        steps,
        edges,
        wires,
    })
}

fn ports(ports: &[AutomationPort]) -> Vec<PortFile> {
    ports.iter().map(|p| PortFile { name: p.name.clone(), kind: p.kind, required: p.required }).collect()
}

/// The name of the way out under this id — one the saved rows hold, so a miss is a broken version.
fn exit_name(exits: &[AutomationExit], id: i64) -> Result<String> {
    exits
        .iter()
        .find(|x| x.id == id)
        .map(|x| x.name.clone())
        .ok_or_else(|| Error::not_found(format!("way out '{id}' not found in the saved version")))
}

/// The name of the port under this id, as [`exit_name`] is.
fn port_name(ports: &[AutomationPort], id: i64) -> Result<String> {
    ports
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .ok_or_else(|| Error::not_found(format!("port '{id}' not found in the saved version")))
}

/// A JSON text the store holds, read back as JSON so that the file carries it as a value.
fn json(text: Option<&str>) -> Result<Option<serde_json::Value>> {
    text.map(|t| serde_json::from_str(t).map_err(Error::from)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AutomationPictureOwner;
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewStep};
    use crate::ops::automation_builtin;
    use crate::ops::test_support::{exit_id, mk_project, with_tx};
    use crate::store_engine::WriteTx;

    /// An automation that opens at the built-in that takes a task, with nothing else on it yet.
    fn mk_automation(tx: &WriteTx<'_>) -> (crate::model::Automation, crate::model::AutomationPlacement) {
        let project = mk_project(tx, "amenbo");
        let automation = automation::add(tx, project, NewAutomation { name: "1件やりきる".into(), ..Default::default() })
            .expect("add automation");
        let take = automation_builtin::action(tx, "take_task").expect("built-in");
        let entry = automation::placement_add(tx, automation.id, take.id).expect("place the built-in");
        (automation, entry)
    }

    /// **The saved version is written out with no id in it**: the built-in by its key and version, the
    /// action a person wrote once however often it is placed, and every line by places and names.
    /// What is written on the action after its save is not in the file.
    #[test]
    fn the_saved_version_is_written_out_by_places_and_names() {
        with_tx(|tx| {
            let (automation, entry) = mk_automation(tx);
            let action = automation::action_from_prompt(
                tx,
                Some(automation.project_id),
                NewStep::new("点検", "見る"),
                &[],
                &[],
            )
            .expect("write the action");
            let (into, out_of, value) =
                (AutomationPortDirection::In, AutomationPortDirection::Out, AutomationPortKind::Value);
            let done = exit_id(tx, AutomationOwner::Action, action.id, None);
            automation::port_add(tx, AutomationPortOwner::Exit, done, out_of, "結果", value, false).expect("an output");
            automation::port_add(tx, AutomationPortOwner::Action, action.id, into, "前から", value, false)
                .expect("an input");
            automation::cfg_add(tx, action.id, "深さ", AutomationCfgKind::Number, false, None).expect("a setting");
            automation::action_version_add(tx, action.id).expect("save the action");
            automation::exit_add(tx, AutomationOwner::Action, action.id, Some("直す")).expect("written since");

            let first = automation::placement_add(tx, automation.id, action.id).expect("place it");
            let second = automation::placement_add(tx, automation.id, action.id).expect("place it again");
            let take = automation_builtin::find("take_task").expect("take_task");
            let on = AutomationPictureOwner::Automation;
            automation::edge_add(tx, on, entry.id, Some(take.exits[0].name), EdgeTarget::Go(first.id), None)
                .expect("take, then look");
            automation::edge_add(tx, on, first.id, None, EdgeTarget::Go(second.id), Some(3)).expect("look again");
            automation::wire_add(tx, on, first.id, None, "結果", second.id, "前から").expect("hand it on");
            automation::cfg_set(tx, first.id, "深さ", Some("3")).expect("answer");
            let step = action.entry_step_id.expect("an entry step");
            automation::placement_step_set(tx, first.id, step, "claude", Some("opus")).expect("choose");
            automation::version_add(tx, automation.id).expect("save the automation");

            let file = export(tx.conn(), automation.id).expect("export");
            assert_eq!(file.format, FORMAT);
            assert_eq!(file.amenbo_version, crate::export::APP_VERSION);
            assert_eq!(file.name, "1件やりきる");
            assert_eq!(file.entry, Some(0));
            match &file.placements[0].stands_on {
                StandsOn::Builtin(b) => assert_eq!((b.key.as_str(), b.version), ("take_task", take.version)),
                other => panic!("the entry is the built-in, not {other:?}"),
            }
            assert!(matches!(file.placements[1].stands_on, StandsOn::Action(0)));
            assert!(matches!(file.placements[2].stands_on, StandsOn::Action(0)), "one action, placed twice");
            assert_eq!(file.actions.len(), 1);

            let looked = &file.placements[1];
            assert_eq!(looked.settings[0].name, "深さ");
            assert_eq!(looked.settings[0].value, Some(serde_json::json!(3)));
            assert_eq!(looked.agents.len(), 1);
            assert_eq!((looked.agents[0].step, looked.agents[0].agent.as_str()), (0, "claude"));
            assert_eq!(looked.agents[0].model.as_deref(), Some("opus"));

            let written = &file.actions[0];
            assert_eq!((written.name.as_str(), written.global, written.entry_step), ("点検", false, Some(0)));
            assert_eq!(written.steps[0].prompt, "見る");
            assert_eq!(written.inputs[0].name, "前から");
            assert_eq!(written.settings[0].kind, AutomationCfgKind::Number);
            let exits: Vec<&str> = written.exits.iter().map(|x| x.name.as_str()).collect();
            assert!(!exits.contains(&"直す"), "written after the save: {exits:?}");
            let done = written.exits.iter().find(|x| x.name == crate::model::DONE_EXIT).expect("done");
            assert_eq!(done.outputs[0].name, "結果");

            assert_eq!(file.edges.len(), 2);
            assert_eq!((file.edges[0].from, file.edges[0].exit.as_str()), (0, take.exits[0].name));
            assert_eq!((file.edges[1].from, file.edges[1].to, file.edges[1].max_times), (1, Some(2), Some(3)));
            assert_eq!(file.wires.len(), 1);
            let wire = &file.wires[0];
            assert_eq!((wire.from, wire.to), (Some(1), Some(2)));
            assert_eq!((wire.from_port.as_str(), wire.to_port.as_str()), ("結果", "前から"));

            let text = serde_json::to_string(&file).expect("serialize");
            assert!(!text.contains("\"id\""), "no id is written: {text}");
            let back: AutomationFile = serde_json::from_str(&text).expect("read back");
            assert_eq!(back.placements.len(), 3);
        });
    }

    /// **Only what is saved is written out**: an automation nobody has saved, and one standing on an
    /// action nobody has saved, are refused.
    #[test]
    fn what_nobody_has_saved_is_not_written_out() {
        with_tx(|tx| {
            let (automation, _) = mk_automation(tx);
            assert!(matches!(export(tx.conn(), automation.id), Err(Error::Invalid(_))), "never saved");

            let action = automation::action_from_prompt(
                tx,
                Some(automation.project_id),
                NewStep::new("点検", "見る"),
                &[],
                &[],
            )
            .expect("write the action");
            automation::placement_add(tx, automation.id, action.id).expect("place it unsaved");
            automation::version_add(tx, automation.id).expect("save the automation");
            assert!(matches!(export(tx.conn(), automation.id), Err(Error::Invalid(_))), "the action is unsaved");
            assert!(matches!(export(tx.conn(), 9999), Err(Error::NotFound(_))));
        });
    }
}
