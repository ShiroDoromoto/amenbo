//! **The built-in that files a task** — one task, from the title and the notes it is handed (`AMB-D-971`).
//!
//! What a person hands a run and what a run fetches become a task before anything goes on
//! (`AMB-D-970`), so the rest of the run is the same road a taken task walks. Filing it here rather than
//! in a prompt keeps it to the one transaction the step is opened in, the way taking one is
//! ([`super::automation_builtin_take`]).
//!
//! **It can take the task it filed** ([`WHAT_THEN`] answered [`TAKE_IT`]): in progress from the moment it
//! exists, and the task this run works from here on. Filed and reserved as two acts, the task would stand
//! `todo` between them, and a run waiting for a task to take looks once a second.
//!
//! **Everything the task is filed with is decided where it is placed**: what it depends on — the task
//! this run works, and tasks named by their number — how it is classified, who it is given to, how
//! urgent it is, the decisions it is linked to and the folder it is worked in. A task that is not to be picked up yet says so by
//! those, never by `blocked` — that is for a task nobody can move (`AMB-D-966`).
//!
//! **What only reading the task can decide is the step before it's to choose** — a classification like
//! the trade that is to work it. Where it is placed says which axes that step may choose on
//! ([`AI_AXES`]), the step hands its choice in ([`CHOSEN`]), and a value off those axes is refused, the
//! way a way out nobody declared is. Everything else the task is filed with is fixed where it is
//! placed, and the step before it cannot change it: an axis already answered in [`CLASSIFY`] is not
//! one it may choose on.
//!
//! **Placed as the entry, it is handed its title, notes and classification at launch** (`AMB-D-970`) —
//! there is no step before it to hand them on, so the person launching the run does ([`read_at_launch`]).
//! The files handed along with them hang off the run until it files the task, and are moved on to that
//! task then (`AMB-D-981`), so what comes after reads them as the task's attachments.
//! That person chooses on the axes the step before it would have, and also gives a value on every axis
//! the project requires and [`CLASSIFY`] does not fix: nobody else is there to. The launch checks all of
//! it before a run is made ([`handed_at_launch`]).
//!
//! **Its creation is finished here.** A task left being created is one nobody can reserve.
//!
//! **A setting it cannot follow files nothing.** A built-in that falls over leaves by the error way out
//! inside the transaction that opened its step, and nothing takes back what it wrote before that — so
//! every refusal the writes would raise is asked first ([`refusal`]): a task, a decision or a folder it
//! cannot find in this run's project, a classification it cannot find or may not use, a required axis
//! left empty, a task to depend on that would keep the new one from being taken.

use crate::error::{Error, Result};
use crate::model::{ActorKind, AttachmentTarget, AutomationCfgKind, AutomationPortKind, Priority};
use crate::ops::automation_builtin::{
    Builtin, BuiltinExit, BuiltinPort, BuiltinSetting, Carried, Carry, Chooses, Work,
};
use crate::ops::automation_report::{self, Produced};
use crate::ops::task::{self, parse_number_ref, parse_typed_ref, NewTask, TypedKind};
use crate::store_engine::read;

/// The built-in's key.
pub const KEY: &str = "make_task";
/// The way out it leaves by once it has filed a task and left it not started.
pub const MADE: &str = "起票した";
/// The way out it leaves by once it has filed a task and taken it.
pub const MADE_AND_TAKEN: &str = "起票して着手した";
/// The output the task is handed on through, on either way out.
pub const TASK: &str = "タスク";
/// The input the task's title is handed in through.
pub const TITLE: &str = "タイトル";
/// The input the task's notes are handed in through.
pub const NOTES: &str = "本文";
/// The input the step before it hands its choice of classification in through: one `axis=value` a
/// line, each on an axis [`AI_AXES`] names.
pub const CHOSEN: &str = "選んだ分類";
/// The setting that says whether it takes the task it filed.
pub const WHAT_THEN: &str = "起票したタスク";
/// The choice on [`WHAT_THEN`] that leaves the task not started — also what it does left unanswered.
pub const LEAVE_IT: &str = "未着手のまま、出口「起票した」へ進む";
/// The choice on [`WHAT_THEN`] that takes it.
pub const TAKE_IT: &str = "進行中にして、出口「起票して着手した」へ進む";
/// The setting that says what the task depends on.
pub const DEPENDS_ON: &str = "依存させる相手";
/// The choice on [`DEPENDS_ON`] that makes it depend on the task this run works.
pub const THE_RUNS_TASK: &str = "この run が扱っているタスク";
/// The setting that classifies it: one `axis=value` a line.
pub const CLASSIFY: &str = "分類";
/// The setting that names the axes the step before it chooses a value on: one axis a line. The step
/// is shown their open values ([`choosable`]).
pub const AI_AXES: &str = "AI に選ばせる軸";
/// The setting that names tasks for it to depend on, one a line.
pub const DEPENDS_ON_TASKS: &str = "依存させる既存のタスク";
/// The setting that names decisions to link it to, one a line.
pub const DECISIONS: &str = "リンクする決定";
/// The setting that names the folder it is worked in — one of this project's linked folders.
pub const FOLDER: &str = "作業フォルダ";
/// The setting that says who it is given to.
pub const ASSIGNEE: &str = "担当";
/// The setting that says how urgent it is.
pub const PRIORITY: &str = "優先度";
/// The choice on every choice setting here that sets nothing — also what it does left unanswered.
pub const NONE: &str = "なし";
const HUMAN: &str = "人";
const AI: &str = "AI";
const HIGH: &str = "高";
const MEDIUM: &str = "中";
const LOW: &str = "低";

pub(super) const MAKE_TASK: Builtin = Builtin {
    key: KEY,
    name: "タスクを起票する",
    does: "受け取ったタイトルと本文で、タスクを1件起票する。設定で、起票と同時に進行中にし、この run で扱える",
    settings: &[
        BuiltinSetting {
            name: WHAT_THEN,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["未着手のまま、出口「起票した」へ進む","進行中にして、出口「起票して着手した」へ進む"]"#),
        },
        BuiltinSetting {
            name: DEPENDS_ON,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["なし","この run が扱っているタスク"]"#),
        },
        BuiltinSetting { name: DEPENDS_ON_TASKS, kind: AutomationCfgKind::Text, required: false, options: None },
        BuiltinSetting { name: CLASSIFY, kind: AutomationCfgKind::Text, required: false, options: None },
        BuiltinSetting { name: AI_AXES, kind: AutomationCfgKind::Text, required: false, options: None },
        BuiltinSetting {
            name: ASSIGNEE,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["なし","人","AI"]"#),
        },
        BuiltinSetting {
            name: PRIORITY,
            kind: AutomationCfgKind::Choice,
            required: false,
            options: Some(r#"["なし","高","中","低"]"#),
        },
        BuiltinSetting { name: DECISIONS, kind: AutomationCfgKind::Text, required: false, options: None },
        BuiltinSetting { name: FOLDER, kind: AutomationCfgKind::Folder, required: false, options: None },
    ],
    ins: &[
        BuiltinPort { name: TITLE, kind: AutomationPortKind::Value, required: true },
        BuiltinPort { name: NOTES, kind: AutomationPortKind::Value, required: false },
        BuiltinPort { name: CHOSEN, kind: AutomationPortKind::Value, required: false },
    ],
    exits: &[
        BuiltinExit {
            name: MADE,
            outs: &[BuiltinPort { name: TASK, kind: AutomationPortKind::TaskMake, required: true }],
        },
        BuiltinExit {
            name: MADE_AND_TAKEN,
            outs: &[BuiltinPort { name: TASK, kind: AutomationPortKind::TaskTake, required: true }],
        },
    ],
    waits: None,
    chooses: Some(Chooses { setting: WHAT_THEN, answer: TAKE_IT, chosen: MADE_AND_TAKEN, otherwise: MADE }),
    work: Work::InStore(make),
};

fn make(carry: &Carry<'_, '_>) -> Result<Carried> {
    let takes = choice(carry, WHAT_THEN)?.as_deref() == Some(TAKE_IT);
    let title = carry.input(TITLE).map(str::trim).unwrap_or_default();
    // Read before anything is written, so a setting that cannot be followed files nothing.
    let mut depends_on = match choice(carry, DEPENDS_ON)?.as_deref() {
        Some(THE_RUNS_TASK) => vec![the_runs_task(carry, takes)?],
        _ => Vec::new(),
    };
    depends_on.extend(named(carry, DEPENDS_ON_TASKS, TypedKind::Task)?);
    let decisions = named(carry, DECISIONS, TypedKind::Decision)?;
    let at_binding_id = folder(carry)?;
    let conn = carry.tx.conn();
    let project_id = carry.run.project_id;
    let mut values = fixed(conn, project_id, choice(carry, CLASSIFY)?.as_deref())?;
    let at_launch = at_launch(carry)?;
    if let Some(written) = carry.input(CHOSEN).filter(|w| !w.trim().is_empty()) {
        let ai_axes = choice(carry, AI_AXES)?;
        let by = match at_launch {
            true => Chooser::Launcher,
            false => Chooser::StepBefore,
        };
        values.extend(chosen(conn, project_id, ai_axes.as_deref(), written, &values, by)?);
    }
    refusal(carry, &values, if takes { &depends_on } else { &[] })?;
    let assignee = match choice(carry, ASSIGNEE)?.as_deref() {
        Some(HUMAN) => Some(ActorKind::Human),
        Some(AI) => Some(ActorKind::Ai),
        _ => None,
    };
    let priority = match choice(carry, PRIORITY)?.as_deref() {
        Some(HIGH) => Some(Priority::High),
        Some(MEDIUM) => Some(Priority::Medium),
        Some(LOW) => Some(Priority::Low),
        _ => None,
    };

    let tx = carry.tx;
    let filed = task::add(
        tx,
        NewTask {
            title: title.to_string(),
            project_id: Some(carry.run.project_id),
            due_on: None,
            start_on: None,
            priority,
            notes: carry.input(NOTES).unwrap_or_default().to_string(),
            created_by_kind: Some(ActorKind::Ai),
            at_binding_id,
            made_in: None,
        },
    )?;
    for (_, value) in values {
        crate::ops::dimension::set(tx, filed.id, value)?;
    }
    if assignee.is_some() {
        task::set_assignee(tx, filed.id, assignee)?;
    }
    for blocker in depends_on {
        crate::ops::dependency::add(tx, filed.id, blocker, Some(ActorKind::Ai))?;
    }
    for decision in decisions {
        crate::ops::decision::link(tx, decision, filed.id)?;
    }
    if at_launch {
        attach_handed(carry, filed.id)?;
    }
    let filed = task::finish_creating(tx, filed.id)?;

    if takes {
        let taken = automation_report::take(tx, carry.run_step.id, filed.id)?;
        return Ok(Carried {
            exit: MADE_AND_TAKEN,
            report: format!("filed and took AMB-T-{} {}", taken.id, taken.title),
        });
    }
    carry.put(MADE, TASK, Produced::Task(filed.id))?;
    Ok(Carried { exit: MADE, report: format!("filed AMB-T-{} {}", filed.id, filed.title) })
}

/// **The files handed over at launch, moved from the run on to the task it filed** (`AMB-D-981`), in the
/// order they were handed and each still saying who handed it. Moved rather than copied: the run was
/// only holding them until there was a task, and nothing reads them off the run.
fn attach_handed(carry: &Carry<'_, '_>, task_id: i64) -> Result<()> {
    let tx = carry.tx;
    let held = read::attachments_for_target(tx.conn(), AttachmentTarget::AutomationRun, carry.run.id)?;
    for row in held {
        let Some(file) = read::attachment(tx.conn(), row.id)? else { continue };
        let (Some(hash), Some(who)) = (file.blob_hash.as_deref(), file.created_by_kind) else { continue };
        crate::ops::attachment::add_blob(
            tx,
            AttachmentTarget::Task,
            task_id,
            hash,
            file.filename.as_deref().unwrap_or_default(),
            file.mime.as_deref(),
            file.size_bytes.unwrap_or_default(),
            who,
        )?;
        crate::ops::attachment::remove(tx, file.id)?;
    }
    Ok(())
}

/// The choice written for one setting, or `None` where nobody answered it.
fn choice(carry: &Carry<'_, '_>, name: &str) -> Result<Option<String>> {
    carry.setting(name).map(|value| serde_json::from_str::<String>(value).map_err(Error::from)).transpose()
}

/// **The task this run works**, for the new one to depend on. Taking the new one opens a stretch of its
/// own, so the task meant is the one the stretch before it worked — closed by then, since a run takes no
/// task with another still open (`AMB-D-967`).
fn the_runs_task(carry: &Carry<'_, '_>, takes: bool) -> Result<i64> {
    let found = match takes {
        false => carry.task_id,
        true => read::automation_run_tasks_of(carry.tx.conn(), carry.run.id)?
            .into_iter()
            .rev()
            .filter(|stretch| Some(stretch.id) != carry.run_step.run_task_id)
            .find_map(|stretch| stretch.task_id),
    };
    found.ok_or_else(|| {
        Error::invalid(format!(
            "'{DEPENDS_ON}' is answered '{THE_RUNS_TASK}', and this run works no task yet"
        ))
    })
}

/// **What the writes would refuse, asked before any of them is made** (the module's last paragraph): a
/// closed value, a required axis with no value, and — where the new task is taken — a task it depends on
/// that is not closed yet, which would leave it not ready to take.
fn refusal(carry: &Carry<'_, '_>, values: &[(i64, i64)], taken_after: &[i64]) -> Result<()> {
    let conn = carry.tx.conn();
    unfilable(conn, carry.run.project_id, values)?;
    for &blocker in taken_after {
        let open = read::task(conn, blocker)?.is_some_and(|task| !task.status.is_closed());
        if open {
            return Err(Error::invalid(format!(
                "the task to take would depend on AMB-T-{blocker}, which is not closed, and could not be taken"
            )));
        }
    }
    Ok(())
}

/// **What a classification would refuse the task over**: a closed value, and an axis the project
/// requires with no value.
fn unfilable(conn: &rusqlite::Connection, project_id: i64, values: &[(i64, i64)]) -> Result<()> {
    for &(_, value_id) in values {
        let value = read::dimension_value(conn, value_id)?
            .ok_or_else(|| crate::ops::dimension::VALUE_NOUN.not_found(value_id.to_string()))?;
        if value.closed {
            return Err(Error::invalid(format!("'{CLASSIFY}' names the value '{}', which is closed", value.name)));
        }
    }
    let empty: Vec<String> =
        read::required_dimensions(conn, project_id, crate::model::ClassifiedSide::Task)?
            .into_iter()
            .filter(|(axis_id, _)| !values.iter().any(|(on, _)| on == axis_id))
            .map(|(_, name)| name)
            .collect();
    if !empty.is_empty() {
        return Err(Error::invalid(format!(
            "the task would carry no value on {}, which this project requires",
            empty.join(", ")
        )));
    }
    Ok(())
}

/// **The tasks or the decisions one setting names**, one a line — `AMB-T-12`, `T-12`, `#12` or `12` for
/// a task, the same with `D` for a decision. Each has to be one of this run's project's: a task filed in
/// one project and tied to another's is the context of that other project leaking in.
fn named(carry: &Carry<'_, '_>, setting: &str, kind: TypedKind) -> Result<Vec<i64>> {
    let Some(written) = choice(carry, setting)? else {
        return Ok(Vec::new());
    };
    let conn = carry.tx.conn();
    let mut ids = Vec::new();
    for line in written.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let number = match parse_typed_ref(line) {
            Some((named, n)) if named == kind => Some(n),
            Some(_) => None,
            None => parse_number_ref(line),
        };
        let id = i64::from(number.ok_or_else(|| {
            Error::invalid(format!("'{setting}' reads one number a line, and '{line}' is not one"))
        })?);
        let project = match kind {
            TypedKind::Task => read::task(conn, id)?.map(|task| task.project_id),
            TypedKind::Decision => read::decision(conn, id)?.map(|decision| Some(decision.project_id)),
        };
        if project != Some(Some(carry.run.project_id)) {
            return Err(Error::invalid(format!("'{setting}' names '{line}', which is not in this project")));
        }
        ids.push(id);
    }
    Ok(ids)
}

/// **The folder [`FOLDER`] names**, as one of this run's project's linked folders — by its path as
/// recorded, or as it resolves on this machine. A task's folder is one of its own project's
/// (`AMB-D-648`).
fn folder(carry: &Carry<'_, '_>) -> Result<Option<i64>> {
    let Some(written) = choice(carry, FOLDER)? else {
        return Ok(None);
    };
    let folders: Vec<_> = crate::overview::bound_folders(carry.tx.conn())?
        .into_iter()
        .filter(|f| f.project_id == carry.run.project_id)
        .collect();
    let canonical = crate::binding::canonical_dir(&written).ok().map(|p| p.to_string_lossy().to_string());
    folders
        .iter()
        .find(|f| f.dir == written || Some(&f.dir) == canonical.as_ref())
        .map(|f| Some(f.id))
        .ok_or_else(|| {
            Error::invalid(format!("'{FOLDER}' names '{written}', which is not one of this project's linked folders"))
        })
}

/// **The values [`CLASSIFY`] names**, one `axis=value` a line, each looked up among this run's project's
/// axes, as (axis, value). A line that names no axis or no value there is refused rather than skipped: a
/// task filed without the classification it was meant to carry is one the filters that should find it
/// pass over.
fn fixed(conn: &rusqlite::Connection, project_id: i64, written: Option<&str>) -> Result<Vec<(i64, i64)>> {
    let Some(written) = written else {
        return Ok(Vec::new());
    };
    let mut values = Vec::new();
    for line in written.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((axis, value)) = line.split_once('=') else {
            return Err(Error::invalid(format!("'{CLASSIFY}' reads one `axis=value` a line, and '{line}' is not one")));
        };
        let (axis, value) = (axis.trim(), value.trim());
        let axis_id = crate::ops::pick_id(
            read::resolve_dimension_in(conn, Some(project_id), axis)?,
            axis,
            || crate::ops::dimension::NOUN.not_found(axis),
        )?;
        let value_id = crate::ops::pick_id(read::resolve_dimension_value_in(conn, axis_id, value)?, value, || {
            crate::ops::dimension::VALUE_NOUN.not_found(format!("{axis}={value}"))
        })?;
        values.push((axis_id, value_id));
    }
    Ok(values)
}

/// **The axes [`AI_AXES`] names**, each looked up among the project's axes the way [`CLASSIFY`]'s are,
/// in the order they are written. An axis that is not there is refused rather than skipped: the step
/// before it was shown nothing to choose on it, and a task filed without it is one the filters pass over.
pub(crate) fn ai_axes(conn: &rusqlite::Connection, project_id: i64, written: &str) -> Result<Vec<i64>> {
    let mut axes = Vec::new();
    for axis in written.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let id = crate::ops::pick_id(read::resolve_dimension_in(conn, Some(project_id), axis)?, axis, || {
            crate::ops::dimension::NOUN.not_found(axis)
        })?;
        if !axes.contains(&id) {
            axes.push(id);
        }
    }
    Ok(axes)
}

/// **Who chose the values handed in through [`CHOSEN`]** — which says what they may choose on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chooser {
    /// The step before it, on the axes [`AI_AXES`] names.
    StepBefore,
    /// The person launching a run whose entry this is: on those axes too, and on every axis the project
    /// requires that [`CLASSIFY`] leaves open — there is nobody else to give it a value.
    Launcher,
}

impl Chooser {
    fn what(self) -> String {
        match self {
            Chooser::StepBefore => format!("'{CHOSEN}'"),
            Chooser::Launcher => "the classification handed over at launch".to_string(),
        }
    }
}

/// **The values chosen** (`written`, one `axis=value` a line), as (axis, value), refused where they step
/// off what was offered: an axis nobody offered `by` ([`Chooser`]), an axis already fixed where the
/// built-in is placed (`fixed`), a value that axis does not have, and more than one value on one axis —
/// one was asked for.
fn chosen(
    conn: &rusqlite::Connection,
    project_id: i64,
    ai_axes_written: Option<&str>,
    written: &str,
    fixed: &[(i64, i64)],
    by: Chooser,
) -> Result<Vec<(i64, i64)>> {
    let mut offered = match ai_axes_written {
        Some(axes) => ai_axes(conn, project_id, axes)?,
        None => Vec::new(),
    };
    if by == Chooser::Launcher {
        offered.extend(
            read::required_dimensions(conn, project_id, crate::model::ClassifiedSide::Task)?
                .into_iter()
                .map(|(axis_id, _)| axis_id),
        );
    }
    let what = by.what();
    let mut values: Vec<(i64, i64)> = Vec::new();
    for line in written.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((axis, value)) = line.split_once('=') else {
            return Err(Error::invalid(format!("{what} reads one `axis=value` a line, and '{line}' is not one")));
        };
        let (axis, value) = (axis.trim(), value.trim());
        let axis_id = read::resolve_dimension_in(conn, Some(project_id), axis)?
            .into_iter()
            .find(|id| offered.contains(id))
            .ok_or_else(|| match by {
                Chooser::StepBefore => {
                    Error::invalid(format!("{what} names the axis '{axis}', which '{AI_AXES}' does not offer"))
                }
                Chooser::Launcher => Error::invalid(format!(
                    "{what} names the axis '{axis}', which '{AI_AXES}' does not offer and this project does not require"
                )),
            })?;
        if fixed.iter().any(|(on, _)| *on == axis_id) {
            return Err(Error::invalid(format!(
                "{what} names the axis '{axis}', which '{CLASSIFY}' already fixes where this is placed"
            )));
        }
        if values.iter().any(|(on, _)| *on == axis_id) {
            return Err(Error::invalid(format!("{what} names more than one value on the axis '{axis}'")));
        }
        let value_id = crate::ops::pick_id(read::resolve_dimension_value_in(conn, axis_id, value)?, value, || {
            crate::ops::dimension::VALUE_NOUN.not_found(format!("{axis}={value}"))
        })?;
        values.push((axis_id, value_id));
    }
    Ok(values)
}

/// **The inputs it reads from what was handed over at launch**, placed as the entry — `builtin` being
/// the key of the built-in placed there, if one is.
pub fn read_at_launch(builtin: Option<&str>, port: &str) -> bool {
    builtin == Some(KEY) && [TITLE, NOTES, CHOSEN].contains(&port)
}

/// **The classification a person handed over to launch a run whose entry this is**, checked the way it
/// will be when the task is filed — so a launch that would only fall over at its first step is refused
/// before a run is made — and written as [`CHOSEN`] reads it. `None` where nothing was handed.
///
/// Asked of where it is placed as the entry (`entry`), since the axes offered and the axes fixed are
/// answered there. The title is the caller's to ask for.
pub(crate) fn handed_at_launch(
    conn: &rusqlite::Connection,
    entry: &crate::model::AutomationPlacement,
    project_id: i64,
    classification: &[(String, String)],
) -> Result<Option<String>> {
    let settings = crate::ops::automation_run::settings_of(conn, entry)?;
    let answer = |name: &str| answered(&settings, name);
    let mut values = fixed(conn, project_id, answer(CLASSIFY)?.as_deref())?;
    let written = (!classification.is_empty()).then(|| {
        classification.iter().map(|(axis, value)| format!("{axis}={value}")).collect::<Vec<_>>().join("\n")
    });
    if let Some(written) = &written {
        let ai_axes = answer(AI_AXES)?;
        values.extend(chosen(conn, project_id, ai_axes.as_deref(), written, &values, Chooser::Launcher)?);
    }
    unfilable(conn, project_id, &values)?;
    Ok(written)
}

/// **One setting's answer where it is placed** (`settings`), as the text it was answered with.
fn answered(settings: &[crate::model::AutomationCfg], name: &str) -> Result<Option<String>> {
    settings
        .iter()
        .find(|cfg| cfg.name == name)
        .and_then(|cfg| cfg.value.as_deref())
        .map(|value| serde_json::from_str::<String>(value).map_err(Error::from))
        .transpose()
}

/// **An axis the person launching a run whose entry this is gives a value on**, with the values open
/// on it in the order the axis lists them. `required` is the project's: a launch that leaves it
/// without a value is refused ([`handed_at_launch`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchAxis {
    pub name: String,
    pub values: Vec<String>,
    pub required: bool,
}

/// **The axes a person launching a run whose entry this is may give a value on** — the ones
/// [`handed_at_launch`] accepts: those [`AI_AXES`] names, then those the project requires, each once,
/// less those [`CLASSIFY`] already fixes where it is placed (`entry`).
///
/// A line on either setting that names no axis is left out rather than refused: this is what a launch
/// dialog draws, and the launch itself refuses what it cannot file from, in its own words.
pub(crate) fn launch_axes(
    conn: &rusqlite::Connection,
    entry: &crate::model::AutomationPlacement,
    project_id: i64,
) -> Result<Vec<LaunchAxis>> {
    let settings = crate::ops::automation_run::settings_of(conn, entry)?;
    let fixed = fixed(conn, project_id, answered(&settings, CLASSIFY)?.as_deref()).unwrap_or_default();
    let required: Vec<i64> = read::required_dimensions(conn, project_id, crate::model::ClassifiedSide::Task)?
        .into_iter()
        .map(|(axis_id, _)| axis_id)
        .collect();
    let written = answered(&settings, AI_AXES)?.unwrap_or_default();
    let named = written.lines().filter_map(|line| ai_axes(conn, project_id, line).ok()).flatten();
    let mut offered: Vec<i64> = Vec::new();
    for axis_id in named.chain(required.iter().copied()) {
        if !offered.contains(&axis_id) && !fixed.iter().any(|(on, _)| *on == axis_id) {
            offered.push(axis_id);
        }
    }
    let mut axes = Vec::new();
    for axis_id in offered {
        let Some(dimension) = read::dimension(conn, axis_id)? else { continue };
        axes.push(LaunchAxis {
            name: dimension.name,
            values: open_values(conn, axis_id)?,
            required: required.contains(&axis_id),
        });
    }
    Ok(axes)
}

/// The names of the values open on an axis, in the order the axis lists them.
fn open_values(conn: &rusqlite::Connection, axis_id: i64) -> Result<Vec<String>> {
    let mut values = Vec::new();
    for id in read::open_dimension_value_ids(conn, axis_id)? {
        if let Some(value) = read::dimension_value(conn, id)? {
            values.push(value);
        }
    }
    values.sort_by(|a, b| a.order_key.cmp(&b.order_key).then(a.id.cmp(&b.id)));
    Ok(values.into_iter().map(|v| v.name).collect())
}

/// **Whether this execution is the entry, handed its inputs at launch** — the first step of a run that
/// was handed a task to file ([`crate::ops::automation_run::HandedTask`]).
fn at_launch(carry: &Carry<'_, '_>) -> Result<bool> {
    if carry.run.handed_task.is_none() {
        return Ok(false);
    }
    let first = read::automation_run_steps_of(carry.tx.conn(), carry.run.id)?.into_iter().next();
    Ok(first.is_some_and(|step| step.id == carry.run_step.id))
}

/// **What the step before it may choose**, as the lines its prompt shows: for each axis [`AI_AXES`]
/// names, the axis and its open values in the order the axis lists them. `written` is that setting's
/// answer as it is kept (JSON). An axis that cannot be found is left out here — the built-in refuses it
/// when it is carried out, and a prompt is no place to fail.
pub(crate) fn choosable(conn: &rusqlite::Connection, project_id: i64, written: &str) -> Result<Vec<String>> {
    let Ok(written) = serde_json::from_str::<String>(written) else {
        return Ok(Vec::new());
    };
    let mut lines = Vec::new();
    for axis in written.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Ok(axes) = ai_axes(conn, project_id, axis) else { continue };
        for axis_id in axes {
            let Some(dimension) = read::dimension(conn, axis_id)? else { continue };
            lines.push(format!("{}: {}", dimension.name, open_values(conn, axis_id)?.join(", ")));
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Automation, AutomationPictureOwner, AutomationPlacement, AutomationRun, AutomationRunStatus,
        TaskStatus,
    };
    use crate::ops::automation::{self, EdgeTarget, NewAutomation};
    use crate::ops::automation_builtin::action;
    use crate::ops::automation_report::Next;
    use crate::ops::automation_run::{
        check, launch_handing, launch_past_the_task_checks as launch, nothing_asked, HandedAtLaunch,
        Launcher, Unmet,
    };
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::{mk_out, mk_placed, mk_project, open, way_out, with_tx};
    use crate::store_engine::WriteTx;

    /// An agent's step that takes a task and hands on a title and notes, the built-in after it, and an
    /// agent's step after that on either way out. Nothing on the picture closes the first step's task —
    /// each test closes it by hand where it has to be — so it is launched past that check.
    struct Picture {
        automation: Automation,
        project: i64,
        first: AutomationPlacement,
        make: AutomationPlacement,
    }

    fn picture(tx: &WriteTx<'_>) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation =
            automation::add(tx, project, NewAutomation { name: "file".into(), ..Default::default() })
                .expect("automation");
        let (first_action, first) = mk_placed(tx, &automation, "Take", "take one", "claude");
        crate::ops::test_support::mk_exit(tx, &first_action, "found");
        mk_out(tx, &first_action, Some("found"), "task", AutomationPortKind::TaskTake, true);
        mk_out(tx, &first_action, Some("found"), "title", AutomationPortKind::Value, true);
        mk_out(tx, &first_action, Some("found"), "notes", AutomationPortKind::Value, true);
        mk_out(tx, &first_action, Some("found"), "trade", AutomationPortKind::Value, false);
        let written = action(tx, "make_task").expect("the built-in's action");
        let make = automation::placement_add(tx, automation.id, written.id).expect("place it");
        let on = AutomationPictureOwner::Automation;
        automation::wire_add(tx, on, first.id, Some("found"), "title", make.id, TITLE).expect("title");
        automation::wire_add(tx, on, first.id, Some("found"), "notes", make.id, NOTES).expect("notes");
        automation::wire_add(tx, on, first.id, Some("found"), "trade", make.id, CHOSEN).expect("chosen");
        automation::edge_add(tx, on, first.id, Some("found"), EdgeTarget::Go(make.id), None).expect("on");
        automation::edge_add(tx, on, first.id, None, EdgeTarget::Done, None).expect("closes");
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        for exit in [MADE, MADE_AND_TAKEN] {
            automation::edge_add(tx, on, make.id, Some(exit), EdgeTarget::Go(work.id), None).expect("onward");
        }
        crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
        let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, project, first, make }
    }

    fn answer(tx: &WriteTx<'_>, p: &Picture, setting: &str, value: &str) {
        automation::cfg_set(tx, p.make.id, setting, Some(&serde_json::to_string(value).expect("json")))
            .expect("answer");
    }

    fn launched(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let claude = ["claude".to_string()];
        let by = Launcher {
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        launch(tx, automation.id, &by).expect("launch")
    }

    /// Walk the first step — take a task, hand on a title and notes — close that task, and carry the
    /// built-in out. What comes back is the task the first step took, the built-in's execution and what
    /// the run does next.
    fn carried(tx: &WriteTx<'_>, p: &Picture, close_first: bool) -> (AutomationRun, i64, i64, Next) {
        carried_choosing(tx, p, close_first, None)
    }

    /// [`carried`], with the first step handing a classification on to [`CHOSEN`] as well.
    fn carried_choosing(
        tx: &WriteTx<'_>,
        p: &Picture,
        close_first: bool,
        chose: Option<&str>,
    ) -> (AutomationRun, i64, i64, Next) {
        let run = launched(tx, &p.automation);
        let claude = ["claude".to_string()];
        let def_of = |placement: i64| {
            read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.placement_id == Some(placement))
                .expect("the spot's copy")
        };
        let opening = match open(tx, run.id, def_of(p.first.id).id, Some(&claude)).expect("open") {
            Opened::Ready(opening) => *opening,
            other => panic!("the agent's step opens a terminal, not {other:?}"),
        };
        let first = crate::ops::test_support::mk_task_in(tx, "the first", Some(p.project));
        automation_report::take(tx, opening.run_step.id, first).expect("take");
        let handed = [("title", Some("  a follow-up  ")), ("notes", Some("what to do")), ("trade", chose)];
        for (port, value) in handed {
            let Some(value) = value else { continue };
            let id = crate::ops::test_support::out_port(tx, opening.run_step.id, None, port);
            automation_report::out(tx, opening.run_step.id, id, Produced::Value(value)).expect("out");
        }
        let found = way_out(tx, opening.run_step.id, "found");
        automation_report::done(tx, opening.run_step.id, found, "took one").expect("done");
        if close_first {
            task::set_status(tx, first, TaskStatus::Done).expect("close the first");
        }
        match open(tx, run.id, def_of(p.make.id).id, Some(&claude)).expect("open") {
            Opened::Carried { run_step_id, next } => (run, first, run_step_id, next),
            other => panic!("a built-in is carried out, not {other:?}"),
        }
    }

    /// The task the built-in handed on, and the way out it left by.
    fn handed(tx: &WriteTx<'_>, run_step_id: i64) -> (crate::model::Task, Option<i64>) {
        let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
        let id = read::automation_run_values_of(tx.conn(), run_step_id)
            .expect("values")
            .into_iter()
            .find_map(|v| v.task_id)
            .unwrap_or_else(|| panic!("no task handed on: {}", ran.report));
        (read::task(tx.conn(), id).expect("read").expect("task"), ran.exit_id)
    }

    /// **Left unanswered, it files one task and leaves it not started** — titled and noted from what it
    /// was handed, its creation finished, nothing else set — and the run stays on the task it had.
    #[test]
    fn unanswered_it_files_one_task_and_leaves_it_not_started() {
        with_tx(|tx| {
            let p = picture(tx);
            let (run, first, run_step_id, _) = carried(tx, &p, false);
            let (filed, exit) = handed(tx, run_step_id);
            assert_eq!(exit, way_out(tx, run_step_id, MADE));
            assert_eq!(filed.title, "a follow-up");
            assert_eq!(filed.notes, "what to do");
            assert_eq!(filed.status, TaskStatus::Todo);
            assert!(!filed.draft, "its creation is finished");
            assert_eq!(filed.project_id, Some(p.project));
            assert_eq!(filed.assignee_kind, None);
            assert_eq!(filed.priority, None);
            let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
            assert_eq!(stretch.task_id, Some(first), "the run is still on the task it took");
        });
    }

    /// **Set to take it, the task it files is in progress and is what the run works** — a stretch of
    /// its own, handed on as the task taken.
    #[test]
    fn set_to_take_it_the_task_it_files_is_the_one_the_run_works() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, WHAT_THEN, TAKE_IT);
            let (run, first, run_step_id, next) = carried(tx, &p, true);
            let (filed, exit) = handed(tx, run_step_id);
            assert_eq!(exit, way_out(tx, run_step_id, MADE_AND_TAKEN));
            assert_eq!(filed.status, TaskStatus::InProgress, "reserved in the act of filing");
            assert!(matches!(next, Next::Step(_)), "the run goes on: {next:?}");
            let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
            assert_eq!(stretch.task_id, Some(filed.id));
            assert_ne!(stretch.task_id, Some(first));
        });
    }

    /// **Taking it with the task before still open is refused**, as for any step that takes one
    /// (`AMB-D-967`) — nothing is filed.
    #[test]
    fn taking_it_with_the_task_before_open_is_refused() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, WHAT_THEN, TAKE_IT);
            let run = launched(tx, &p.automation);
            let claude = ["claude".to_string()];
            let defs = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            let def = |placement: i64| defs.iter().find(|d| d.placement_id == Some(placement)).expect("copy").id;
            let opening = match open(tx, run.id, def(p.first.id), Some(&claude)).expect("open") {
                Opened::Ready(opening) => *opening,
                other => panic!("{other:?}"),
            };
            let first = crate::ops::test_support::mk_task_in(tx, "the first", Some(p.project));
            automation_report::take(tx, opening.run_step.id, first).expect("take");
            let before = read::automation_run_steps_of(tx.conn(), run.id).expect("steps").len();
            match open(tx, run.id, def(p.make.id), Some(&claude)).expect("open") {
                Opened::LeftTaskOpen { run } => assert_eq!(run.status, AutomationRunStatus::Failed),
                other => panic!("the task before is still in progress, so {other:?} is wrong"),
            }
            assert_eq!(read::automation_run_steps_of(tx.conn(), run.id).expect("steps").len(), before);
        });
    }

    /// **What it is set with is on the task it files**: the classification, who it is given to, how
    /// urgent it is and the run's task as what it depends on.
    #[test]
    fn what_it_is_set_with_is_on_the_task_it_files() {
        with_tx(|tx| {
            let p = picture(tx);
            let axis = crate::ops::dimension::add(
                tx,
                p.project,
                crate::ops::dimension::NewDimension { name: "職能".into(), ..Default::default() },
            )
            .expect("axis");
            let value = crate::ops::dimension::value_add(tx, axis.id, "実装", None).expect("value");
            answer(tx, &p, CLASSIFY, "職能=実装\n");
            answer(tx, &p, ASSIGNEE, AI);
            answer(tx, &p, PRIORITY, HIGH);
            answer(tx, &p, DEPENDS_ON, THE_RUNS_TASK);
            let (_, first, run_step_id, _) = carried(tx, &p, false);
            let (filed, _) = handed(tx, run_step_id);
            assert_eq!(filed.assignee_kind, Some(ActorKind::Ai));
            assert_eq!(filed.priority, Some(Priority::High));
            assert!(read::assignment_id(tx.conn(), filed.id, value.id).expect("read").is_some());
            assert!(read::dependency_id(tx.conn(), filed.id, first).expect("read").is_some());
        });
    }

    /// **Tasks and decisions named by their number, and a linked folder, are on the task it files** —
    /// alongside the run's task, which it depends on too.
    #[test]
    fn named_tasks_decisions_and_a_folder_are_on_the_task_it_files() {
        with_tx(|tx| {
            let p = picture(tx);
            let earlier = crate::ops::test_support::mk_task_in(tx, "earlier", Some(p.project));
            let decision = crate::ops::test_support::mk_decision_in(tx, "why", p.project);
            let mut reg = crate::binding::Registry::default();
            reg.project_dirs.entry(p.project).or_default().insert("/work/here".to_string());
            crate::overview::write_bindings(tx, &reg).expect("bind");
            let binding = crate::overview::bound_folders(tx.conn()).expect("folders")[0].id;
            answer(tx, &p, DEPENDS_ON, THE_RUNS_TASK);
            answer(tx, &p, DEPENDS_ON_TASKS, &format!("AMB-T-{earlier}\n"));
            answer(tx, &p, DECISIONS, &format!("D-{decision}"));
            answer(tx, &p, FOLDER, "/work/here");
            let (_, first, run_step_id, _) = carried(tx, &p, false);
            let (filed, _) = handed(tx, run_step_id);
            for blocker in [first, earlier] {
                assert!(read::dependency_id(tx.conn(), filed.id, blocker).expect("read").is_some(), "{blocker}");
            }
            assert!(read::decision_task_link_id(tx.conn(), decision, filed.id).expect("read").is_some());
            assert_eq!(filed.at_binding_id, Some(binding));
        });
    }

    /// **A number that is not this project's, or a folder it has not linked, files nothing.**
    #[test]
    fn a_number_or_a_folder_it_cannot_find_here_files_nothing() {
        for (setting, written) in [
            (DEPENDS_ON_TASKS, "AMB-T-999"),
            (DEPENDS_ON_TASKS, "AMB-D-1"),
            (DECISIONS, "D-999"),
            (FOLDER, "/nowhere"),
        ] {
            with_tx(|tx| {
                let p = picture(tx);
                answer(tx, &p, setting, written);
                let (_, first, run_step_id, next) = carried(tx, &p, false);
                assert!(matches!(next, Next::Halted(_)), "{setting} {written}: {next:?}");
                let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
                assert!(ran.report.contains(setting), "{}", ran.report);
                assert!(read::task(tx.conn(), first + 1).expect("read").is_none(), "nothing filed");
            });
        }
    }

    /// **Taking it and depending on the run's task means the task the stretch before worked.**
    #[test]
    fn taking_it_the_runs_task_is_the_one_before() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, WHAT_THEN, TAKE_IT);
            answer(tx, &p, DEPENDS_ON, THE_RUNS_TASK);
            let (_, first, run_step_id, _) = carried(tx, &p, true);
            let (filed, exit) = handed(tx, run_step_id);
            assert_eq!(exit, way_out(tx, run_step_id, MADE_AND_TAKEN));
            assert!(read::dependency_id(tx.conn(), filed.id, first).expect("read").is_some());
        });
    }

    /// **A classification it cannot find files nothing** and leaves by the error way out, naming it.
    #[test]
    fn a_classification_it_cannot_find_files_nothing() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, CLASSIFY, "職能=実装");
            let (_, first, run_step_id, next) = carried(tx, &p, false);
            assert!(matches!(next, Next::Halted(_)), "{next:?}");
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert!(ran.report.contains("職能"), "{}", ran.report);
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, crate::model::ERROR_EXIT));
            let filed = read::automation_run_values_of(tx.conn(), run_step_id).expect("values");
            assert!(filed.iter().all(|v| v.task_id.is_none()), "nothing handed on");
            assert!(read::task(tx.conn(), first + 1).expect("read").is_none(), "and nothing filed");
        });
    }

    /// An axis with two values, the one the step before it may choose on.
    fn trades(tx: &WriteTx<'_>, p: &Picture) -> (i64, i64) {
        let axis = crate::ops::dimension::add(
            tx,
            p.project,
            crate::ops::dimension::NewDimension { name: "職能".into(), ..Default::default() },
        )
        .expect("axis");
        let build = crate::ops::dimension::value_add(tx, axis.id, "実装", None).expect("value");
        crate::ops::dimension::value_add(tx, axis.id, "設計", None).expect("value");
        (axis.id, build.id)
    }

    /// Carry the built-in out with `chose` handed in, and say it fell over naming `what`, filing nothing.
    fn refused(tx: &WriteTx<'_>, p: &Picture, chose: &str, what: &str) {
        let (_, first, run_step_id, next) = carried_choosing(tx, p, false, Some(chose));
        assert!(matches!(next, Next::Halted(_)), "{next:?}");
        let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
        assert!(ran.report.contains(what), "{}", ran.report);
        assert_eq!(ran.exit_id, way_out(tx, run_step_id, crate::model::ERROR_EXIT));
        assert!(read::task(tx.conn(), first + 1).expect("read").is_none(), "nothing filed");
    }

    /// **A value the step before it chose, on an axis it was offered, is on the task it files**
    /// (`AMB-D-971`).
    #[test]
    fn a_value_chosen_on_an_offered_axis_is_filed_with_the_task() {
        with_tx(|tx| {
            let p = picture(tx);
            let (_, build) = trades(tx, &p);
            answer(tx, &p, AI_AXES, "職能");
            let (_, _, run_step_id, _) = carried_choosing(tx, &p, false, Some("職能=実装\n"));
            let (filed, exit) = handed(tx, run_step_id);
            assert_eq!(exit, way_out(tx, run_step_id, MADE));
            assert!(read::assignment_id(tx.conn(), filed.id, build).expect("read").is_some());
        });
    }

    /// **What steps off the offer is refused**, and nothing is filed: an axis not offered, a value the
    /// axis does not have, two values on one axis, and an axis fixed where it is placed.
    #[test]
    fn a_choice_off_what_was_offered_files_nothing() {
        with_tx(|tx| {
            let p = picture(tx);
            trades(tx, &p);
            refused(tx, &p, "職能=実装", AI_AXES);
        });
        with_tx(|tx| {
            let p = picture(tx);
            trades(tx, &p);
            answer(tx, &p, AI_AXES, "職能");
            refused(tx, &p, "職能=営業", "営業");
        });
        with_tx(|tx| {
            let p = picture(tx);
            trades(tx, &p);
            answer(tx, &p, AI_AXES, "職能");
            refused(tx, &p, "職能=実装\n職能=設計", "more than one");
        });
        with_tx(|tx| {
            let p = picture(tx);
            trades(tx, &p);
            answer(tx, &p, AI_AXES, "職能");
            answer(tx, &p, CLASSIFY, "職能=設計");
            refused(tx, &p, "職能=実装", CLASSIFY);
        });
    }

    /// **The step before it is shown what it may choose**, under the output wired to [`CHOSEN`]: each
    /// offered axis and its open values, in the axis's order.
    #[test]
    fn the_step_before_it_is_shown_what_it_may_choose() {
        with_tx(|tx| {
            let p = picture(tx);
            trades(tx, &p);
            answer(tx, &p, AI_AXES, "職能");
            let run = launched(tx, &p.automation);
            let claude = ["claude".to_string()];
            let def = read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.placement_id == Some(p.first.id))
                .expect("the first spot's copy");
            let opening = match open(tx, run.id, def.id, Some(&claude)).expect("open") {
                Opened::Ready(opening) => *opening,
                other => panic!("{other:?}"),
            };
            assert!(opening.text.contains("trade takes one `axis=value` a line"), "{}", opening.text);
            assert!(opening.text.contains("    - 職能: 実装, 設計"), "{}", opening.text);
        });
    }

    /// **The launch check reads the setting**: the way out it will not leave by needs no line, and only
    /// set to take the task it files can it be the entry.
    #[test]
    fn the_launch_check_asks_only_the_way_out_it_is_set_to_leave_by() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "entry".into(), ..Default::default() })
                    .expect("automation");
            let written = action(tx, "make_task").expect("the built-in's action");
            let make = automation::placement_add(tx, automation.id, written.id).expect("place it");
            let on = AutomationPictureOwner::Automation;
            let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
            automation::edge_add(tx, on, make.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(work.id), None)
                .expect("onward");
            crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
            let automation = automation::set_entry(tx, automation.id, Some(make.id)).expect("entry");
            let startable = ["claude".to_string()];
            let unmet = |tx: &WriteTx<'_>| check(tx.conn(), automation.id, Some(&startable), nothing_asked()).expect("check");

            let left = unmet(tx);
            assert!(left.iter().any(|u| matches!(u, Unmet::EntryTakesNoTask { .. })), "{left:?}");
            assert!(left.iter().any(|u| matches!(u, Unmet::OpenExit { exit, .. } if exit == MADE)), "{left:?}");

            automation::cfg_set(tx, make.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
                .expect("take it");
            // Nothing is wired into the title: as the entry, it is handed one at launch.
            let taken = unmet(tx);
            assert!(taken.is_empty(), "{taken:?}");
        });
    }

    /// The built-in placed as the entry, set to take what it files, with a step after it that closes
    /// the task — launched with what a person hands over.
    fn entry(tx: &WriteTx<'_>) -> (Automation, AutomationPlacement, i64) {
        let project = mk_project(tx, "amenbo");
        let automation =
            automation::add(tx, project, NewAutomation { name: "entry".into(), ..Default::default() })
                .expect("automation");
        let written = action(tx, "make_task").expect("the built-in's action");
        let make = automation::placement_add(tx, automation.id, written.id).expect("place it");
        automation::cfg_set(tx, make.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
            .expect("take it");
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        let on = AutomationPictureOwner::Automation;
        automation::edge_add(tx, on, make.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(work.id), None)
            .expect("onward");
        crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
        let automation = automation::set_entry(tx, automation.id, Some(make.id)).expect("entry");
        (automation, make, project)
    }

    fn launch_with(tx: &WriteTx<'_>, automation: &Automation, handed: &HandedAtLaunch) -> Result<AutomationRun> {
        let claude = ["claude".to_string()];
        let by = Launcher {
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Human),
        };
        launch_handing(tx, automation.id, &by, handed)
    }

    fn titled(title: &str) -> HandedAtLaunch {
        HandedAtLaunch { title: Some(title.into()), ..Default::default() }
    }

    fn with_values(title: &str, values: &[(&str, &str)]) -> HandedAtLaunch {
        HandedAtLaunch {
            classification: values.iter().map(|(a, v)| (a.to_string(), v.to_string())).collect(),
            ..titled(title)
        }
    }

    /// Open the run's first step, which is the built-in, and say what it filed and took.
    fn filed_first(tx: &WriteTx<'_>, run: &AutomationRun, make: &AutomationPlacement) -> crate::model::Task {
        let def = read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.placement_id == Some(make.id))
            .expect("the entry's copy");
        let claude = ["claude".to_string()];
        match open(tx, run.id, def.id, Some(&claude)).expect("open") {
            Opened::Carried { run_step_id, next } => {
                assert!(matches!(next, Next::Step(_)), "the run goes on: {next:?}");
                let (filed, exit) = handed(tx, run_step_id);
                assert_eq!(exit, way_out(tx, run_step_id, MADE_AND_TAKEN));
                filed
            }
            other => panic!("the entry is carried out, not {other:?}"),
        }
    }

    /// **Placed as the entry, it files the task from what was handed over at launch** — the title, the
    /// notes and a value on an offered axis — and the run works that task (`AMB-D-970`).
    #[test]
    fn as_the_entry_it_files_what_the_launch_handed_over() {
        with_tx(|tx| {
            let (automation, make, project) = entry(tx);
            let p = Picture { automation: automation.clone(), project, first: make.clone(), make: make.clone() };
            let (_, build) = trades(tx, &p);
            answer(tx, &p, AI_AXES, "職能");
            let handed = HandedAtLaunch {
                notes: Some("what to do".into()),
                ..with_values("  an issue  ", &[("職能", "実装")])
            };
            let run = launch_with(tx, &automation, &handed).expect("launch");

            let filed = filed_first(tx, &run, &make);
            assert_eq!(filed.title, "an issue");
            assert_eq!(filed.notes, "what to do");
            assert_eq!(filed.status, TaskStatus::InProgress);
            assert!(read::assignment_id(tx.conn(), filed.id, build).expect("read").is_some());
            let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
            assert_eq!(stretch.task_id, Some(filed.id));
        });
    }

    /// **The files handed over at launch are moved on to the task it files** (`AMB-D-981`) — in the order
    /// they were handed, each still saying who handed it, and none left on the run.
    #[test]
    fn as_the_entry_it_attaches_the_files_handed_over_to_the_task_it_files() {
        use crate::ops::automation_run::HandedFile;
        with_tx(|tx| {
            let (automation, make, _) = entry(tx);
            let file = |name: &str| HandedFile {
                blob_hash: "a".repeat(64),
                filename: name.to_string(),
                mime: Some("text/markdown".to_string()),
                size_bytes: 12,
            };
            let handed = HandedAtLaunch { files: vec![file("issue.md"), file("log.txt")], ..titled("an issue") };
            // A file has to say who handed it over, so a launcher that says nothing about itself cannot.
            let claude = ["claude".to_string()];
            let nobody =
                Launcher { startable: Some(&claude), models: nothing_asked(), workspace_open: Some(true), by: None };
            assert!(launch_handing(tx, automation.id, &nobody, &handed).is_err());
            assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty());

            let run = launch_with(tx, &automation, &handed).expect("launch");
            let on_the_run = |tx: &WriteTx<'_>| {
                read::attachments_for_target(tx.conn(), AttachmentTarget::AutomationRun, run.id).expect("files")
            };
            assert_eq!(on_the_run(tx).len(), 2, "held on the run until there is a task");

            let filed = filed_first(tx, &run, &make);
            let files = read::attachments_for_target(tx.conn(), AttachmentTarget::Task, filed.id).expect("files");
            let names: Vec<_> = files.iter().map(|a| a.filename.clone().unwrap_or_default()).collect();
            assert_eq!(names, ["issue.md", "log.txt"]);
            assert!(files.iter().all(|a| a.created_by_kind.as_deref() == Some("human")));
            assert!(on_the_run(tx).is_empty(), "moved, not copied");
        });
    }

    /// **The person launching gives a value on every axis the project requires** and where it is placed
    /// leaves open, offered or not — nobody else is there to. Left out, the launch is refused.
    #[test]
    fn a_required_axis_left_open_is_the_launchers_to_answer() {
        with_tx(|tx| {
            let (automation, make, project) = entry(tx);
            let axis = crate::ops::dimension::add(
                tx,
                project,
                crate::ops::dimension::NewDimension { name: "職能".into(), ..Default::default() },
            )
            .expect("axis");
            let build = crate::ops::dimension::value_add(tx, axis.id, "実装", None).expect("value");
            // An axis is required only once it offers a value.
            crate::ops::dimension::update(tx, axis.id, None, None, None, None, None, None, Some(true), None, None)
                .expect("required");

            let err = launch_with(tx, &automation, &titled("an issue")).expect_err("no value on it");
            assert!(err.to_string().contains("職能"), "{err}");
            assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty());

            let run = launch_with(tx, &automation, &with_values("an issue", &[("職能", "実装")])).expect("launch");
            let filed = filed_first(tx, &run, &make);
            assert!(read::assignment_id(tx.conn(), filed.id, build.id).expect("read").is_some());
        });
    }

    /// **What the entry cannot file from is refused before a run is made**: no title, an axis nobody
    /// offered, one fixed where it is placed, a value the axis does not have.
    #[test]
    fn a_launch_it_could_not_file_from_makes_no_run() {
        with_tx(|tx| {
            let (automation, make, project) = entry(tx);
            let p = Picture { automation: automation.clone(), project, first: make.clone(), make: make.clone() };
            trades(tx, &p);
            let refused = |tx: &WriteTx<'_>, handed: HandedAtLaunch, what: &str| {
                let err = launch_with(tx, &automation, &handed).expect_err(what);
                assert!(err.to_string().contains(what), "{err}");
                assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty());
            };
            refused(tx, HandedAtLaunch::default(), "no title");
            refused(tx, titled("  "), "no title");
            refused(tx, with_values("an issue", &[("職能", "実装")]), AI_AXES);
            answer(tx, &p, AI_AXES, "職能");
            refused(tx, with_values("an issue", &[("職能", "営業")]), "職能=営業");
            answer(tx, &p, CLASSIFY, "職能=設計");
            refused(tx, with_values("an issue", &[("職能", "実装")]), "already fixes");
        });
    }

    /// **A launch asks for the axes it accepts a value on** — those offered to choose on, then those the
    /// project requires, less those fixed where it is placed — each with its open values; any other
    /// entry, an agent's step or a built-in that takes a task, asks for nothing (`AMB-D-981`).
    #[test]
    fn a_launch_asks_for_the_axes_it_accepts_a_value_on() {
        use crate::ops::automation_run::{launch_asks, LaunchAsks};
        with_tx(|tx| {
            let (automation, make, project) = entry(tx);
            let p = Picture { automation: automation.clone(), project, first: make.clone(), make: make.clone() };
            let asked = |tx: &WriteTx<'_>| match launch_asks(tx.conn(), automation.id).expect("asks") {
                LaunchAsks::Task { axes } => axes,
                other => panic!("the entry files a task: {other:?}"),
            };
            assert_eq!(asked(tx), Vec::new());

            trades(tx, &p);
            answer(tx, &p, AI_AXES, "職能\nどこにも無い軸");
            let kind = crate::ops::dimension::add(
                tx,
                project,
                crate::ops::dimension::NewDimension { name: "種別".into(), ..Default::default() },
            )
            .expect("axis");
            crate::ops::dimension::value_add(tx, kind.id, "不具合", None).expect("value");
            crate::ops::dimension::update(tx, kind.id, None, None, None, None, None, None, Some(true), None, None)
                .expect("required");
            let axis = |name: &str, values: &[&str], required: bool| LaunchAxis {
                name: name.into(),
                values: values.iter().map(|v| v.to_string()).collect(),
                required,
            };
            assert_eq!(asked(tx), vec![axis("職能", &["実装", "設計"], false), axis("種別", &["不具合"], true)]);

            answer(tx, &p, CLASSIFY, "職能=設計");
            assert_eq!(asked(tx), vec![axis("種別", &["不具合"], true)]);

            let another = |tx: &WriteTx<'_>, name: &str| {
                automation::add(tx, project, NewAutomation { name: name.into(), ..Default::default() })
                    .expect("automation")
            };
            let agent = another(tx, "agent");
            let (_, work) = mk_placed(tx, &agent, "work", "work on it", "claude");
            automation::set_entry(tx, agent.id, Some(work.id)).expect("entry");
            assert_eq!(launch_asks(tx.conn(), agent.id).expect("asks"), LaunchAsks::Nothing);

            let take = another(tx, "take");
            let placed = automation::placement_add(tx, take.id, action(tx, "take_task").expect("action").id)
                .expect("place it");
            assert_eq!(launch_asks(tx.conn(), take.id).expect("asks"), LaunchAsks::Nothing);
            automation::set_entry(tx, take.id, Some(placed.id)).expect("entry");
            assert_eq!(launch_asks(tx.conn(), take.id).expect("asks"), LaunchAsks::Nothing);
        });
    }

    /// The choices written out on the setting are the ones the code reads, the one it does unanswered
    /// first.
    #[test]
    fn the_choices_offered_are_the_ones_it_reads() {
        let offered = |name: &str| -> Vec<String> {
            let options = MAKE_TASK.settings.iter().find(|s| s.name == name).and_then(|s| s.options);
            serde_json::from_str(options.expect("a choice list")).expect("JSON")
        };
        assert_eq!(offered(WHAT_THEN), vec![LEAVE_IT, TAKE_IT]);
        assert_eq!(offered(DEPENDS_ON), vec![NONE, THE_RUNS_TASK]);
        assert_eq!(offered(ASSIGNEE), vec![NONE, HUMAN, AI]);
        assert_eq!(offered(PRIORITY), vec![NONE, HIGH, MEDIUM, LOW]);
        let kinds: Vec<_> = MAKE_TASK.settings.iter().map(|s| (s.name, s.kind)).collect();
        assert!(kinds.contains(&(FOLDER, AutomationCfgKind::Folder)));
    }
}
