//! **The built-in that files a task** — one task, from the title and the notes it is handed (`AMB-D-971`).
//!
//! What a person hands a run and what a run fetches become a task before anything goes on
//! (`AMB-D-970`), so the rest of the run is the same road a taken task walks. Filing it here rather than
//! in a prompt keeps it to the one transaction the step is opened in, the way taking one is
//! ([`super::automation_builtin_take`]).
//!
//! **It can take the task it filed** ([`WHAT_THEN`] answered [`TAKE_IT`]): in progress from the moment it
//! exists, and the task this run works from here on. Filed and reserved as two acts, the task would stand
//! `todo` between them, and another run's take could reserve it there.
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
//! **It reads nothing handed over at launch** (`AMB-D-1123`): what a person or a trigger hands a run is
//! filed as a task with a role before the run starts ([`crate::ops::automation_run::HandedAtLaunch`]),
//! classified as [`launch_values`] reads it, so its title, notes and classification come to it along a
//! wire like any other step's.
//!
//! **It can file the task under another role** ([`ROLE`]), by the name of one of the automation's roles.
//! Where the round it runs in is worked under a role and files the task under a different one, that is
//! a task passed from one role to the other, and it is recorded on the run
//! ([`crate::model::AutomationRunPass`]) — what the run's pane draws a line between two roles by.
//!
//! **Its creation is finished here.** A task left being created is one nobody can reserve.
//!
//! **A setting it cannot follow files nothing.** A built-in that falls over leaves by the error way out
//! inside the transaction that opened its step, and nothing takes back what it wrote before that — so
//! every refusal the writes would raise is asked first ([`refusal`]): a task, a decision or a folder it
//! cannot find in this run's project, a role its automation does not have exactly one of, the folder
//! left empty with several linked to fill it from, a classification it cannot find or may not use, a
//! required axis left empty, a task to depend on that would keep the new one from being taken.

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::model::{ActorKind, AutomationCfgKind, AutomationPortKind, Priority};
use crate::ops::automation_builtin::{
    Builtin, BuiltinExit, BuiltinPort, BuiltinSetting, Carried, Carry, Chooses, Work,
};
use crate::ops::automation_report::{self, Produced};
use crate::run_wording::builtin as say;
use crate::ops::task::{self, parse_number_ref, parse_typed_ref, NewTask, TypedKind};
use crate::store_engine::{read, record};
use crate::time::Timestamp;

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
pub const THE_RUNS_TASK: &str = "この実行が扱っているタスク";
/// The setting that classifies it: one `axis=value` a line.
pub const CLASSIFY: &str = "分類";
/// The setting that names the axes the step before it chooses a value on: one axis a line. The step
/// is shown their open values ([`choosable`]).
pub const AI_AXES: &str = "AI に選ばせる軸";
/// The setting that names tasks for it to depend on, one a line.
pub const DEPENDS_ON_TASKS: &str = "依存させる既存のタスク";
/// The setting that names decisions to link it to, one a line.
pub const DECISIONS: &str = "リンクする決定";
/// The setting that names the folder it is worked in — one of this project's linked folders. Left
/// empty, it is the project's one folder, if it has just one ([`folder`]).
pub const FOLDER: &str = "作業フォルダ";
/// The setting that says who it is given to. Left at its "none", a task a trigger files is the AI's all the
/// same: nobody asked for it by hand, and it is worked in a round of this automation.
pub const ASSIGNEE: &str = "担当";
/// The setting that says how urgent it is.
pub const PRIORITY: &str = "優先度";
/// The setting that names the role the task is worked under — one of the automation's roles, by its
/// name. Left empty, it is filed under no role ([`role`]).
pub const ROLE: &str = "役割";
/// The choice on every choice setting here that sets nothing — also what it does left unanswered.
pub const NONE: &str = "なし";
const HUMAN: &str = "人";
const AI: &str = "AI";
const HIGH: &str = "高";
const MEDIUM: &str = "中";
const LOW: &str = "低";

pub(super) const MAKE_TASK: Builtin = Builtin {
    key: KEY,
    version: 2,
    name: "タスクを起票する",
    does: "受け取ったタイトルと本文で、タスクを1件起票する。設定で、起票と同時に進行中にし、この実行で扱える",
    steps: &[
        "設定と入力をすべて読み、依存させる相手・分類・作業フォルダ・リンクする決定がこのプロジェクトにあるかを確かめる。1つでも従えなければ、何も起票しない",
        "受け取ったタイトルと本文でタスクを1件起票し、分類・担当・優先度・役割・依存・決定を付ける。いまの周の役割と違う役割で起票したら、その役割へ渡したと記録する。最初のステップに置いたときは、起動のときに渡されたファイルをタスクに移す",
        "設定「起票したタスク」で進行中にすると決めてあれば、進行中にして「起票して着手した」から出る。そうでなければ、未着手のまま「起票した」から出てタスクを渡す",
    ],
    halts: &[
        "依存させる相手が「この実行が扱っているタスク」なのに、この実行がまだタスクを扱っていない",
        "設定に書いたタスク・決定・作業フォルダが、このプロジェクトに無い",
        "分類が「軸=値」の形で書かれていないか、その軸か値が無い。または、閉じた値を指している",
        "AI に選ばせる軸に無い軸か、分類で決めてある軸の値を受け取った",
        "このプロジェクトが必須にしている軸に、値が付かない",
        "進行中にするのに、依存させる相手がまだ閉じていない",
        "設定に書いた役割が、このオートメーションの役割の1つに決まらない",
    ],
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
            options: Some(r#"["なし","この実行が扱っているタスク"]"#),
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
        BuiltinSetting { name: ROLE, kind: AutomationCfgKind::Text, required: false, options: None },
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
    let role_id = role(carry)?;
    let conn = carry.tx.conn();
    let project_id = carry.run.project_id;
    let mut values = fixed(conn, project_id, choice(carry, CLASSIFY)?.as_deref())?;
    if let Some(written) = carry.input(CHOSEN).filter(|w| !w.trim().is_empty()) {
        let ai_axes = choice(carry, AI_AXES)?;
        values.extend(chosen(conn, project_id, ai_axes.as_deref(), written, &values)?);
    }
    with_time_axis(conn, project_id, &mut values)?;
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
            due: None,
            start: None,
            priority,
            notes: carry.input(NOTES).unwrap_or_default().to_string(),
            created_by_kind: Some(ActorKind::Ai),
            at_binding_id,
            made_in: None,
            role_id,
            made_in_run_step_id: Some(carry.run_step.id),
        },
    )?;
    for (_, value) in values {
        crate::ops::dimension::set(tx, filed.id, value)?;
    }
    if assignee.is_some() {
        task::set_assignee(tx, filed.id, assignee, ActorKind::Ai)?;
    }
    for blocker in depends_on {
        crate::ops::dependency::add(tx, filed.id, blocker, Some(ActorKind::Ai))?;
    }
    for decision in decisions {
        crate::ops::decision::link(tx, decision, filed.id)?;
    }
    let filed = task::finish_creating(tx, filed.id, ActorKind::Ai)?;
    if let (Some(to_role_id), false) = (role_id, takes) {
        record_pass(carry, to_role_id, filed.id)?;
    }

    if takes {
        let taken = automation_report::take(tx, carry.run_step.id, filed.id)?;
        return Ok(Carried {
            exit: MADE_AND_TAKEN,
            report: say(
                tx.language(),
                "filedAndTook",
                &[("task", &format!("AMB-T-{}", taken.id)), ("title", &taken.title)],
            ),
        });
    }
    carry.put(MADE, TASK, Produced::Task(filed.id))?;
    let task = format!("AMB-T-{}", filed.id);
    Ok(Carried { exit: MADE, report: say(tx.language(), "filed", &[("task", &task), ("title", &filed.title)]) })
}

/// **The role [`ROLE`] names**, by its name among the roles of the run's automation, or `None` where it is
/// left empty. A name no role has, or one two roles share, is refused: a role's name is not held unique,
/// and filing the task under whichever came first would be the built-in choosing for the person.
fn role(carry: &Carry<'_, '_>) -> Result<Option<i64>> {
    let Some(written) = choice(carry, ROLE)? else { return Ok(None) };
    let name = written.trim();
    if name.is_empty() {
        return Ok(None);
    }
    let roles = read::automation_roles_of(carry.tx.conn(), carry.run.automation_id)?;
    match role_named(&roles, name) {
        Some(id) => Ok(Some(id)),
        None => Err(Error::Invalid(
            Msg::new(format!("'{ROLE}' names '{name}', which is not exactly one of this automation's roles"))
                .coded(ErrorCode::InvalidMakeTaskRoleUnknown)
                .with("role", name),
        )),
    }
}

/// **The one role of `roles` called `name`**, or `None` where no role or more than one is ([`role`]).
pub(crate) fn role_named(roles: &[crate::model::AutomationRole], name: &str) -> Option<i64> {
    match roles.iter().filter(|r| r.name == name).collect::<Vec<_>>().as_slice() {
        [one] => Some(one.id),
        _ => None,
    }
}

/// **The role [`ROLE`] is answered with, among `roles`** — `None` where it is left empty, `Some(None)`
/// where the name is not exactly one of them. Read by the launch check ([`unfound_role`]) and by the
/// build screen, which draws the role a placement passes its task to
/// ([`crate::ops::automation_run::passes_by_role`]).
pub(crate) fn answered_role(
    settings: &[crate::model::AutomationCfg],
    roles: &[crate::model::AutomationRole],
) -> Option<(String, Option<i64>)> {
    let written = answered(settings, ROLE).ok().flatten()?;
    let name = written.trim();
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), role_named(roles, name)))
}

/// **Whether the task it files is taken by the round that files it** ([`WHAT_THEN`] answered [`TAKE_IT`]).
/// The round goes on to work it under the role it is filed under, so that is the same round turning to
/// another role, not a task passed to another role's round — the built-in records no pass for it
/// ([`record_pass`]), and the build screen draws none ([`crate::ops::automation_run::passes_by_role`]).
pub(crate) fn answered_takes(settings: &[crate::model::AutomationCfg]) -> bool {
    answered(settings, WHAT_THEN).ok().flatten().as_deref() == Some(TAKE_IT)
}

/// **Whether the task it files is set to depend on another** — on the task this run works
/// ([`DEPENDS_ON`]), or on a task named ([`DEPENDS_ON_TASKS`]). What the build screen marks a task a
/// role passes on with ([`crate::ops::automation_run::passes_by_role`]).
pub(crate) fn answered_depends(settings: &[crate::model::AutomationCfg]) -> bool {
    let written = |name: &str| answered(settings, name).ok().flatten();
    written(DEPENDS_ON).as_deref() == Some(THE_RUNS_TASK)
        || written(DEPENDS_ON_TASKS).is_some_and(|tasks| !tasks.trim().is_empty())
}

/// **The name [`ROLE`] is answered with, where it is not exactly one of `roles`** — the launch check's
/// counterpart of [`unfound`] for the role, which the built-in would refuse when a run reached it.
pub(crate) fn unfound_role(
    settings: &[crate::model::AutomationCfg],
    roles: &[crate::model::AutomationRole],
) -> Option<(&'static str, String)> {
    match answered_role(settings, roles)? {
        (name, None) => Some((ROLE, name)),
        (_, Some(_)) => None,
    }
}

/// **Record the task as passed on**, where the round this step runs in is worked under a role and the
/// task was filed under another. A round worked under no role passes nothing: there is no role it
/// came from. Neither does a task filed under the round's own role.
fn record_pass(carry: &Carry<'_, '_>, to_role_id: i64, task_id: i64) -> Result<()> {
    let tx = carry.tx;
    let Some(round_id) = carry.run_step.run_task_id else { return Ok(()) };
    let Some(round) = read::automation_run_task(tx.conn(), round_id)? else { return Ok(()) };
    if round.role_id.is_none_or(|from| from == to_role_id) {
        return Ok(());
    }
    let placement_id =
        read::automation_run_def(tx.conn(), carry.run_step.run_def_id)?.and_then(|def| def.placement_id);
    let now = Timestamp::now();
    let pass = crate::model::AutomationRunPass {
        id: read::next_id(tx.conn(), "automation_run_pass")?,
        run_id: carry.run.id,
        run_task_id: round.id,
        to_role_id: Some(to_role_id),
        task_id: Some(task_id),
        placement_id,
        created_at: now,
        updated_at: now,
    };
    crate::ops::emit_create(tx, record::automation_run_pass(&pass))
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
        refused(
            ErrorCode::InvalidMakeTaskNoRunsTask,
            format!("'{DEPENDS_ON}' is answered '{THE_RUNS_TASK}', and this run works no task yet"),
        )
    })
}

/// A refusal of this built-in's that names no value: its code, and the English the CLI prints.
fn refused(code: ErrorCode, en: String) -> Error {
    Error::Invalid(Msg::new(en).coded(code))
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
            let blocked = format!("AMB-T-{blocker}");
            return Err(Error::Invalid(
                Msg::new(format!(
                    "the task to take would depend on {blocked}, which is not closed, and could not be taken"
                ))
                .coded(ErrorCode::InvalidMakeTaskBlockerOpen)
                .with("ref", blocked),
            ));
        }
    }
    Ok(())
}

/// **The classification, with the time-axis value the task gets without anyone naming it** (`AMB-D-147`)
/// — the same default `task add` fills ([`task::time_axis_default`]), added before [`unfilable`] reads
/// the axes the project requires, so a required time axis is filled rather than refused. An axis the
/// classification already answers keeps its answer.
fn with_time_axis(conn: &rusqlite::Connection, project_id: i64, values: &mut Vec<(i64, i64)>) -> Result<()> {
    let named: Vec<i64> = values.iter().map(|&(axis, _)| axis).collect();
    values.extend(task::time_axis_default(conn, project_id, crate::time::today(), &named)?);
    Ok(())
}

/// **What a classification would refuse the task over**: a closed value, and an axis the project
/// requires with no value.
fn unfilable(conn: &rusqlite::Connection, project_id: i64, values: &[(i64, i64)]) -> Result<()> {
    for &(_, value_id) in values {
        let value = read::dimension_value(conn, value_id)?
            .ok_or_else(|| crate::ops::dimension::VALUE_NOUN.not_found(value_id.to_string()))?;
        if value.closed {
            return Err(Error::Invalid(
                Msg::new(format!("'{CLASSIFY}' names the value '{}', which is closed", value.name))
                    .coded(ErrorCode::InvalidMakeTaskValueClosed)
                    .with("value", &value.name),
            ));
        }
    }
    let empty: Vec<String> =
        read::required_dimensions(conn, project_id, crate::model::ClassifiedSide::Task)?
            .into_iter()
            .filter(|(axis_id, _)| !values.iter().any(|(on, _)| on == axis_id))
            .map(|(_, name)| name)
            .collect();
    if !empty.is_empty() {
        let axes = empty.join(", ");
        return Err(Error::Invalid(
            Msg::new(format!("the task would carry no value on {axes}, which this project requires"))
                .coded(ErrorCode::InvalidMakeTaskRequiredEmpty)
                .with("axes", axes),
        ));
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
        ids.push(named_one(conn, carry.run.project_id, setting, kind, line)?);
    }
    Ok(ids)
}

/// **The task or the decision one line of a setting names** ([`named`]), refused where it is not one of
/// `project_id`'s.
fn named_one(
    conn: &rusqlite::Connection,
    project_id: i64,
    setting: &str,
    kind: TypedKind,
    line: &str,
) -> Result<i64> {
    let number = match parse_typed_ref(line) {
        Some((named, n)) if named == kind => Some(n),
        Some(_) => None,
        None => parse_number_ref(line),
    };
    let (not_a_number, elsewhere) = match kind {
        TypedKind::Task => (ErrorCode::InvalidMakeTaskTaskNotANumber, ErrorCode::InvalidMakeTaskTaskNotInProject),
        TypedKind::Decision => {
            (ErrorCode::InvalidMakeTaskDecisionNotANumber, ErrorCode::InvalidMakeTaskDecisionNotInProject)
        }
    };
    let on_line = |code, en: String| Error::Invalid(Msg::new(en).coded(code).with("line", line));
    let id = i64::from(number.ok_or_else(|| {
        on_line(not_a_number, format!("'{setting}' reads one number a line, and '{line}' is not one"))
    })?);
    let project = match kind {
        TypedKind::Task => read::task(conn, id)?.map(|task| task.project_id),
        TypedKind::Decision => read::decision(conn, id)?.map(|decision| Some(decision.project_id)),
    };
    if project != Some(Some(project_id)) {
        return Err(on_line(elsewhere, format!("'{setting}' names '{line}', which is not in this project")));
    }
    Ok(id)
}

/// **The folder [`FOLDER`] names**, as one of this run's project's linked folders — by its path as
/// recorded, or as it resolves on this machine. A task's folder is one of its own project's
/// (`AMB-D-648`).
///
/// **Left empty, it is the project's one folder** (`AMB-D-1012`): a run files from no folder of its own,
/// so there is nowhere it was filed to fill it from. With no folder linked the task has none; with
/// several, which one is not ours to pick (`AMB-D-531`), so it files nothing ([`folder_unwritten`]).
fn folder(carry: &Carry<'_, '_>) -> Result<Option<i64>> {
    let conn = carry.tx.conn();
    match choice(carry, FOLDER)? {
        Some(written) => folder_named(conn, carry.run.project_id, &written).map(Some),
        None => folder_unwritten(conn, carry.run.project_id),
    }
}

/// **The folder of `project_id`'s a task is filed in when [`FOLDER`] is left empty** ([`folder`]).
fn folder_unwritten(conn: &rusqlite::Connection, project_id: i64) -> Result<Option<i64>> {
    match linked_folders(conn, project_id)?.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one.id)),
        several => Err(Error::Invalid(
            Msg::new(format!(
                "'{FOLDER}' is left empty, and this project has {} linked folders — write which one the task \
                 is worked in",
                several.len()
            ))
            .coded(ErrorCode::InvalidMakeTaskFolderUnchosen)
            .with("count", several.len()),
        )),
    }
}

/// **The folders linked to `project_id`.**
fn linked_folders(conn: &rusqlite::Connection, project_id: i64) -> Result<Vec<crate::binding::BoundFolder>> {
    Ok(crate::overview::bound_folders(conn)?.into_iter().filter(|f| f.project_id == project_id).collect())
}

/// **The linked folder of `project_id`'s that `written` names** ([`folder`]).
fn folder_named(conn: &rusqlite::Connection, project_id: i64, written: &str) -> Result<i64> {
    let folders = linked_folders(conn, project_id)?;
    let canonical = crate::binding::canonical_dir(written).ok().map(|p| p.to_string_lossy().to_string());
    folders
        .iter()
        .find(|f| f.dir == written || Some(&f.dir) == canonical.as_ref())
        .map(|f| f.id)
        .ok_or_else(|| {
            Error::Invalid(
                Msg::new(format!("'{FOLDER}' names '{written}', which is not one of this project's linked folders"))
                    .coded(ErrorCode::InvalidMakeTaskFolderUnknown)
                    .with("folder", written),
            )
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
        values.push(fixed_one(conn, project_id, line)?);
    }
    Ok(values)
}

/// **The axis and the value one line of [`CLASSIFY`] names** ([`fixed`]).
fn fixed_one(conn: &rusqlite::Connection, project_id: i64, line: &str) -> Result<(i64, i64)> {
    let Some((axis, value)) = line.split_once('=') else {
        return Err(Error::Invalid(
            Msg::new(format!("'{CLASSIFY}' reads one `axis=value` a line, and '{line}' is not one"))
                .coded(ErrorCode::InvalidMakeTaskClassifyNotAxisValue)
                .with("line", line),
        ));
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
    Ok((axis_id, value_id))
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

/// **The values the step before it chose** (`written`, one `axis=value` a line), as (axis, value),
/// refused where they step off what was offered: an axis [`AI_AXES`] does not offer, an axis already
/// fixed where the built-in is placed (`fixed`), a value that axis does not have, and more than one
/// value on one axis — one was asked for. The refusal is left on the task in the reader's language.
fn chosen(
    conn: &rusqlite::Connection,
    project_id: i64,
    ai_axes_written: Option<&str>,
    written: &str,
    fixed: &[(i64, i64)],
) -> Result<Vec<(i64, i64)>> {
    let offered = match ai_axes_written {
        Some(axes) => ai_axes(conn, project_id, axes)?,
        None => Vec::new(),
    };
    let what = format!("'{CHOSEN}'");
    let said = |code: ErrorCode, en: String, field: (&'static str, &str)| {
        Error::Invalid(Msg::new(en).coded(code).with(field.0, field.1))
    };
    let mut values: Vec<(i64, i64)> = Vec::new();
    for line in written.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((axis, value)) = line.split_once('=') else {
            return Err(said(
                ErrorCode::InvalidMakeTaskChosenNotAxisValue,
                format!("{what} reads one `axis=value` a line, and '{line}' is not one"),
                ("line", line),
            ));
        };
        let (axis, value) = (axis.trim(), value.trim());
        let axis_id = read::resolve_dimension_in(conn, Some(project_id), axis)?
            .into_iter()
            .find(|id| offered.contains(id))
            .ok_or_else(|| {
                said(
                    ErrorCode::InvalidMakeTaskAxisNotOffered,
                    format!("{what} names the axis '{axis}', which '{AI_AXES}' does not offer"),
                    ("axis", axis),
                )
            })?;
        if fixed.iter().any(|(on, _)| *on == axis_id) {
            return Err(said(
                ErrorCode::InvalidMakeTaskAxisFixed,
                format!("{what} names the axis '{axis}', which '{CLASSIFY}' already fixes where this is placed"),
                ("axis", axis),
            ));
        }
        if values.iter().any(|(on, _)| *on == axis_id) {
            return Err(said(
                ErrorCode::InvalidMakeTaskAxisTwice,
                format!("{what} names more than one value on the axis '{axis}'"),
                ("axis", axis),
            ));
        }
        let value_id = crate::ops::pick_id(read::resolve_dimension_value_in(conn, axis_id, value)?, value, || {
            crate::ops::dimension::VALUE_NOUN.not_found(format!("{axis}={value}"))
        })?;
        values.push((axis_id, value_id));
    }
    Ok(values)
}

/// **The classification handed over with a task filed at launch** ([`crate::ops::automation_run::HandedAtLaunch`]),
/// as (axis, value) by name, looked up among `project_id`'s axes — with the time-axis value the task
/// gets without anyone naming it ([`with_time_axis`]). Refused the way the built-in refuses what it
/// files: an axis or a value the project does not have, more than one value on one axis, a closed
/// value, and an axis the project requires left without one ([`unfilable`]). Asked before the task is
/// filed, so a launch it refuses files nothing.
pub(crate) fn launch_values(
    conn: &rusqlite::Connection,
    project_id: i64,
    classification: &[(String, String)],
) -> Result<Vec<(i64, i64)>> {
    let mut values: Vec<(i64, i64)> = Vec::new();
    for (axis, value) in classification {
        let (axis_id, value_id) = fixed_one(conn, project_id, &format!("{axis}={value}"))?;
        if values.iter().any(|(on, _)| *on == axis_id) {
            return Err(Error::invalid(format!(
                "the classification handed over at launch names more than one value on the axis '{axis}'"
            )));
        }
        values.push((axis_id, value_id));
    }
    with_time_axis(conn, project_id, &mut values)?;
    unfilable(conn, project_id, &values)?;
    Ok(values)
}

/// **The folder a task filed at launch is worked in** — `project_id`'s one linked folder, or none where
/// it has none or several: there is nowhere it was filed from to choose by (`AMB-D-1012`).
pub(crate) fn sole_folder(conn: &rusqlite::Connection, project_id: i64) -> Result<Option<i64>> {
    Ok(match linked_folders(conn, project_id)?.as_slice() {
        [one] => Some(one.id),
        _ => None,
    })
}

/// **The lines of its settings that name what `project_id` does not have**, as they are answered where
/// it is placed (`settings`) — as (setting, line), in the order the settings are declared (`AMB-D-987`).
///
/// Asked by the launch check ([`crate::ops::automation_run::check`]) of the settings that name something by
/// its number, its path or its name: a task to depend on, a decision to link, the folder, an axis and a
/// value to classify with, and an axis the step before it chooses on. Each line is looked up the way the
/// built-in looks it up when it is carried out, so a line refused here is one the run would halt on. The
/// built-in asks again when it is carried out: what is found at launch can be gone by then.
///
/// A line that is not written as that setting reads it — a word where a number goes, a classification with
/// no `=` — is one of these too, since nothing can be found from it.
pub(crate) fn unfound(
    conn: &rusqlite::Connection,
    settings: &[crate::model::AutomationCfg],
    project_id: i64,
) -> Result<Vec<(&'static str, String)>> {
    let mut found = Vec::new();
    for setting in [DEPENDS_ON_TASKS, DECISIONS, FOLDER, CLASSIFY, AI_AXES] {
        // An answer that is not the JSON text it is kept as is the settings check's to refuse, not this one's.
        let Ok(Some(written)) = answered(settings, setting) else { continue };
        let lines: Vec<&str> = match setting {
            FOLDER => vec![written.as_str()],
            _ => written.lines().map(str::trim).filter(|line| !line.is_empty()).collect(),
        };
        for line in lines {
            let looked_up = match setting {
                DEPENDS_ON_TASKS => named_one(conn, project_id, setting, TypedKind::Task, line).map(drop),
                DECISIONS => named_one(conn, project_id, setting, TypedKind::Decision, line).map(drop),
                FOLDER => folder_named(conn, project_id, line).map(drop),
                CLASSIFY => fixed_one(conn, project_id, line).map(drop),
                _ => ai_axes(conn, project_id, line).map(drop),
            };
            if not_there(looked_up)? {
                found.push((setting, line.to_string()));
            }
        }
    }
    Ok(found)
}

/// **The lines of [`CLASSIFY`] that name a value this project has but has closed** (`AMB-D-987`). A task
/// filed with one is refused when a run reaches it ([`unfilable`]), so the launch check names it first. A
/// line naming what is not there at all is [`unfound`]'s.
pub(crate) fn closed_values(
    conn: &rusqlite::Connection,
    settings: &[crate::model::AutomationCfg],
    project_id: i64,
) -> Result<Vec<String>> {
    let Ok(Some(written)) = answered(settings, CLASSIFY) else { return Ok(Vec::new()) };
    let mut closed = Vec::new();
    for line in written.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let value_id = match fixed_one(conn, project_id, line) {
            Ok((_, value_id)) => value_id,
            Err(Error::NotFound(_) | Error::Invalid(_) | Error::AmbiguousId { .. }) => continue,
            Err(other) => return Err(other),
        };
        if read::dimension_value(conn, value_id)?.is_some_and(|value| value.closed) {
            closed.push(line.to_string());
        }
    }
    Ok(closed)
}

/// **How many folders `project_id` has linked, when [`FOLDER`] is left empty where it is placed
/// (`settings`) and there are too many to fill it from** (`AMB-D-1012`, `AMB-D-987`). The built-in
/// files nothing then ([`folder_unwritten`]), so the launch check names it first. It is asked again when
/// the built-in is carried out: a folder can be linked or unlinked in between.
pub(crate) fn folder_unchosen(
    conn: &rusqlite::Connection,
    settings: &[crate::model::AutomationCfg],
    project_id: i64,
) -> Result<Option<usize>> {
    // An answer that is not the JSON text it is kept as is the settings check's to refuse, not this one's.
    if !matches!(answered(settings, FOLDER), Ok(None)) {
        return Ok(None);
    }
    let count = linked_folders(conn, project_id)?.len();
    Ok((count > 1).then_some(count))
}

/// Whether a lookup came back saying there is nothing there — not found, not written as one, or naming
/// more than one. Anything else it fails on is the store's own failure and is passed on.
fn not_there(looked_up: Result<()>) -> Result<bool> {
    match looked_up {
        Ok(()) => Ok(false),
        Err(Error::NotFound(_) | Error::Invalid(_) | Error::AmbiguousId { .. }) => Ok(true),
        Err(other) => Err(other),
    }
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
        check, launch_handing, launch_past_the_setting_checks as launch, nothing_asked, HandedAtLaunch,
        Launcher, ReadsFrom, Unmet,
    };
    use crate::ops::automation_step::Opened;
    use crate::lifecycle::name::{TASK_ASSIGNED, TASK_CREATED, TASK_DONE, TASK_STATUS_CHANGED};
    use crate::ops::test_support::{by_ai, events_after, outbox_head};
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
            automation::add(tx, project, NewAutomation { name: "file".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
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
            default_agent: None,
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
        carried_under(tx, p, close_first, chose, None)
    }

    /// [`carried_choosing`], with the round the built-in runs in worked under `role` — written on the
    /// round by hand, as the take of a task under a role whose round starts at the entry would.
    fn carried_under(
        tx: &WriteTx<'_>,
        p: &Picture,
        close_first: bool,
        chose: Option<&str>,
        role: Option<i64>,
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
        if let Some(role) = role {
            let before = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("round");
            let after = crate::model::AutomationRunTask { role_id: Some(role), ..before.clone() };
            crate::ops::emit_update(tx, record::automation_run_task(&before), record::automation_run_task(&after))
                .expect("the round's role");
        }
        let handed = [("title", Some("  a follow-up  ")), ("notes", Some("what to do")), ("trade", chose)];
        for (port, value) in handed {
            let Some(value) = value else { continue };
            let id = crate::ops::test_support::out_port(tx, opening.run_step.id, None, port);
            automation_report::out(tx, opening.run_step.id, id, Produced::Value(value)).expect("out");
        }
        let found = way_out(tx, opening.run_step.id, "found");
        automation_report::done(tx, opening.run_step.id, found, "took one").expect("done");
        if close_first {
            task::set_status(tx, first, TaskStatus::Done, crate::model::ActorKind::Ai).expect("close the first");
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

    /// **The task it files is announced as the AI's** (`AMB-D-367`): given to someone, then born — the
    /// end of its creation is when a task is announced (`AMB-D-557`) — and, set to take it, taken. Each
    /// event is stamped `ai`, the one actor a notification sends for (`AMB-D-473`).
    #[test]
    fn the_task_it_files_and_takes_is_announced_as_the_ai_s() {
        with_tx(|tx| {
            let p = picture(tx);
            answer(tx, &p, ASSIGNEE, AI);
            answer(tx, &p, WHAT_THEN, TAKE_IT);
            let head = outbox_head(tx);
            carried(tx, &p, true);
            assert_eq!(
                events_after(tx, head),
                vec![
                    // The first step's task: filed by the test, taken, and closed by hand.
                    by_ai(TASK_CREATED, None),
                    by_ai(TASK_STATUS_CHANGED, Some("in_progress")),
                    by_ai(TASK_DONE, None),
                    // The one the built-in files.
                    by_ai(TASK_ASSIGNED, Some("ai")),
                    by_ai(TASK_CREATED, None),
                    by_ai(TASK_STATUS_CHANGED, Some("in_progress")),
                ]
            );
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
                // Said in the reader's language, so the setting goes by its translated name; what was
                // written in it is nobody's to translate.
                assert!(ran.report.contains(written), "{}", ran.report);
                nothing_filed(tx, run_step_id, first, "nothing filed");
            });
        }
    }

    /// **Nothing was filed after `first` but the failed round itself** — the task the run's stop files
    /// for whoever the automation sends a failed round to, which is not this built-in's doing.
    fn nothing_filed(tx: &WriteTx<'_>, run_step_id: i64, first: i64, said: &str) {
        let run_id = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row").run_id;
        let round = read::automation_run_task_last(tx.conn(), run_id).expect("read");
        let mut next = first + 1;
        if round.and_then(|r| r.incident_task_id) == Some(next) {
            next += 1;
        }
        assert!(read::task(tx.conn(), next).expect("read").is_none(), "{said}");
    }

    /// Links `dirs` to the picture's project, as its folders.
    fn linked(tx: &WriteTx<'_>, p: &Picture, dirs: &[&str]) {
        let mut reg = crate::binding::Registry::default();
        reg.project_dirs.entry(p.project).or_default().extend(dirs.iter().map(|dir| dir.to_string()));
        crate::overview::write_bindings(tx, &reg).expect("bind");
    }

    /// **With the folder left empty, the task is filed in the project's one folder** (`AMB-D-1012`), and
    /// in none when the project has none.
    #[test]
    fn a_folder_left_empty_is_the_projects_one_folder_or_none() {
        for dirs in [&["/work/here"][..], &[][..]] {
            with_tx(|tx| {
                let p = picture(tx);
                linked(tx, &p, dirs);
                let binding = crate::overview::bound_folders(tx.conn()).expect("folders").first().map(|f| f.id);
                let (_, _, run_step_id, _) = carried(tx, &p, false);
                let (filed, _) = handed(tx, run_step_id);
                assert_eq!(filed.at_binding_id, binding, "{dirs:?}");
            });
        }
    }

    /// **With the folder left empty and several folders linked, it files nothing** (`AMB-D-1012`): which
    /// one is not its to pick.
    #[test]
    fn a_folder_left_empty_among_several_files_nothing() {
        with_tx(|tx| {
            let p = picture(tx);
            linked(tx, &p, &["/work/here", "/work/there"]);
            let (_, first, run_step_id, next) = carried(tx, &p, false);
            assert!(matches!(next, Next::Halted(_)), "{next:?}");
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.exit_id, way_out(tx, run_step_id, crate::model::ERROR_EXIT));
            nothing_filed(tx, run_step_id, first, "nothing filed");
        });
    }

    /// **The launch check names a folder left empty among several** (`AMB-D-987`) — and not one written,
    /// nor one left empty with a single folder to fill it from.
    #[test]
    fn the_launch_check_names_a_folder_left_empty_among_several() {
        let startable = ["claude".to_string()];
        let unchosen = |tx: &WriteTx<'_>, p: &Picture| -> Vec<(String, String, usize, Option<String>, i64)> {
            check(tx.conn(), p.automation.id, Some(&startable), nothing_asked(), None)
                .expect("check")
                .into_iter()
                .filter_map(|unmet| match unmet {
                    Unmet::FolderUnchosen { step, cfg, count, builtin, placement } => {
                        Some((step, cfg, count, builtin, placement))
                    }
                    _ => None,
                })
                .collect()
        };
        with_tx(|tx| {
            let p = picture(tx);
            linked(tx, &p, &["/work/here"]);
            assert!(unchosen(tx, &p).is_empty(), "one folder fills it");
            linked(tx, &p, &["/work/here", "/work/there"]);
            let named = ("タスクを起票する".to_string(), FOLDER.to_string(), 2, Some(KEY.to_string()), p.make.id);
            assert_eq!(unchosen(tx, &p), vec![named]);
            answer(tx, &p, FOLDER, "/work/there");
            assert!(unchosen(tx, &p).is_empty(), "a folder written is the one");
        });
    }

    /// **The launch check names each line of a setting that names what this project does not have**
    /// (`AMB-D-987`) — a task or a decision by a number, a folder, an axis or a value — and nothing about
    /// the lines it does have.
    #[test]
    fn the_launch_check_names_each_line_a_setting_cannot_find_here() {
        with_tx(|tx| {
            let p = picture(tx);
            let earlier = crate::ops::test_support::mk_task_in(tx, "earlier", Some(p.project));
            let other = mk_project(tx, "other");
            let elsewhere = crate::ops::test_support::mk_task_in(tx, "elsewhere", Some(other));
            trades(tx, &p);
            let tasks = format!("AMB-T-{earlier}\nAMB-T-99999\nAMB-T-{elsewhere}\nsoon");
            answer(tx, &p, DEPENDS_ON_TASKS, &tasks);
            answer(tx, &p, DECISIONS, "D-999");
            answer(tx, &p, FOLDER, "/nowhere");
            answer(tx, &p, CLASSIFY, "職能=実装\n職能=営業\nnosuch=x");
            answer(tx, &p, AI_AXES, "職能\n種別");
            let startable = ["claude".to_string()];
            let unmet = check(tx.conn(), p.automation.id, Some(&startable), nothing_asked(), None).expect("check");
            let found: Vec<(String, String)> = unmet
                .into_iter()
                .filter_map(|unmet| match unmet {
                    Unmet::CfgNotFound { step, cfg, line, builtin, placement } => {
                        let named = (step.as_str(), builtin.as_deref(), placement);
                        assert_eq!(named, ("タスクを起票する", Some(KEY), p.make.id));
                        Some((cfg, line))
                    }
                    _ => None,
                })
                .collect();
            let expected = [
                (DEPENDS_ON_TASKS, "AMB-T-99999".to_string()),
                (DEPENDS_ON_TASKS, format!("AMB-T-{elsewhere}")),
                (DEPENDS_ON_TASKS, "soon".to_string()),
                (DECISIONS, "D-999".to_string()),
                (FOLDER, "/nowhere".to_string()),
                (CLASSIFY, "職能=営業".to_string()),
                (CLASSIFY, "nosuch=x".to_string()),
                (AI_AXES, "種別".to_string()),
            ]
            .map(|(cfg, line)| (cfg.to_string(), line))
            .to_vec();
            assert_eq!(found, expected);
        });
    }

    /// **Filed under another role than its round's, the task is recorded as passed to that role** — from
    /// the round's role, to the role [`ROLE`] names, which the task is worked under. Filed under the
    /// round's own role, or from a round worked under none, it is worked under the role named and
    /// nothing is recorded.
    #[test]
    fn a_task_filed_under_another_role_is_recorded_as_passed_to_it() {
        use crate::ops::automation_run::{passes_of_run, RunPass};
        let new_role = |name: &str| automation::NewRole { name: name.into(), ..Default::default() };
        with_tx(|tx| {
            let p = picture(tx);
            let alice = automation::role_add(tx, p.automation.id, new_role("Alice")).expect("Alice");
            let bob = automation::role_add(tx, p.automation.id, new_role("Bob")).expect("Bob");
            answer(tx, &p, ROLE, " Bob ");
            let (run, _, run_step_id, _) = carried_under(tx, &p, false, None, Some(alice.id));
            let (filed, exit) = handed(tx, run_step_id);
            assert_eq!(exit, way_out(tx, run_step_id, MADE));
            assert_eq!(filed.role_id, Some(bob.id), "worked under the role named");
            let round = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("round");
            let passed = passes_of_run(tx.conn(), run.id).expect("passes");
            let expected = RunPass {
                run_task_id: round.id,
                from_role_id: Some(alice.id),
                to_role_id: Some(bob.id),
                task_id: Some(filed.id),
            };
            assert_eq!(passed, vec![expected]);
            let kept = read::automation_run_passes_of(tx.conn(), run.id).expect("passes");
            let spots: Vec<Option<i64>> = kept.iter().map(|k| k.placement_id).collect();
            assert_eq!(spots, vec![Some(p.make.id)], "the spot that filed it");
        });
        for (round_role, why) in [(true, "filed under the round's own role"), (false, "from a round under no role")] {
            with_tx(|tx| {
                let p = picture(tx);
                let bob = automation::role_add(tx, p.automation.id, new_role("Bob")).expect("Bob");
                answer(tx, &p, ROLE, "Bob");
                let (run, _, run_step_id, _) =
                    carried_under(tx, &p, false, None, round_role.then_some(bob.id));
                let (filed, _) = handed(tx, run_step_id);
                assert_eq!(filed.role_id, Some(bob.id), "{why}: worked under the role named");
                assert!(passes_of_run(tx.conn(), run.id).expect("passes").is_empty(), "{why}: nothing passed");
            });
        }
    }

    /// **The build screen reads what each role's round passes on** off the definition: the placement of
    /// the built-in inside one role's round, answered with another role, whether the task it files
    /// depends on another, and the task a run last passed on from there. Answered with the round's own
    /// role, or set to take the task it files, it passes nothing — as a run records nothing there.
    #[test]
    fn what_a_roles_round_passes_on_is_read_off_the_definition() {
        use crate::ops::automation_run::{passes_by_role, RolePass};
        with_tx(|tx| {
            let p = picture(tx);
            let starting = |name: &str, at: Option<i64>| automation::NewRole {
                name: name.into(),
                entry_placement_id: at,
                ..Default::default()
            };
            let alice = automation::role_add(tx, p.automation.id, starting("Alice", Some(p.first.id))).expect("Alice");
            let bob = automation::role_add(tx, p.automation.id, starting("Bob", None)).expect("Bob");
            let passes = || passes_by_role(tx.conn(), p.automation.id, ReadsFrom::Draft).expect("passes");
            assert!(passes().is_empty(), "left unanswered, it passes nothing");

            answer(tx, &p, ROLE, "Bob");
            let expected = RolePass {
                from_role_id: alice.id,
                to_role_id: bob.id,
                placement_id: p.make.id,
                depends: false,
                last_task_id: None,
            };
            assert_eq!(passes(), vec![expected.clone()]);

            answer(tx, &p, WHAT_THEN, TAKE_IT);
            assert!(passes().is_empty(), "taking the task it files, it passes nothing");
            answer(tx, &p, WHAT_THEN, LEAVE_IT);

            let (_, _, run_step_id, _) = carried_under(tx, &p, false, None, Some(alice.id));
            let (filed, _) = handed(tx, run_step_id);
            assert_eq!(
                passes(),
                vec![RolePass { last_task_id: Some(filed.id), ..expected.clone() }],
                "the task a run last passed on from here"
            );
            let expected = RolePass { last_task_id: Some(filed.id), ..expected };

            answer(tx, &p, DEPENDS_ON, THE_RUNS_TASK);
            assert_eq!(passes(), vec![RolePass { depends: true, ..expected.clone() }], "on the run's task");
            answer(tx, &p, DEPENDS_ON, NONE);
            answer(tx, &p, DEPENDS_ON_TASKS, "AMB-T-1");
            assert_eq!(passes(), vec![RolePass { depends: true, ..expected }], "on a task named");

            answer(tx, &p, ROLE, "Alice");
            assert!(passes().is_empty(), "filed under its own round's role, it passes nothing");
        });
    }

    /// **A role its automation does not have exactly one of is refused** — a name no role has, or one two
    /// roles share — by the launch check first, and by the built-in where a run reaches it anyway, which
    /// files nothing.
    #[test]
    fn a_role_named_by_no_role_or_by_two_is_refused() {
        let new_role = |name: &str| automation::NewRole { name: name.into(), ..Default::default() };
        for (roles, named) in [(vec!["Alice"], "Carol"), (vec!["Alice", "Alice"], "Alice")] {
            with_tx(|tx| {
                let p = picture(tx);
                for name in &roles {
                    automation::role_add(tx, p.automation.id, new_role(name)).expect("a role");
                }
                answer(tx, &p, ROLE, named);
                let startable = ["claude".to_string()];
                let unmet = check(tx.conn(), p.automation.id, Some(&startable), nothing_asked(), None).expect("check");
                let found: Vec<(String, String)> = unmet
                    .into_iter()
                    .filter_map(|unmet| match unmet {
                        Unmet::CfgNotFound { cfg, line, placement, .. } if placement == p.make.id => Some((cfg, line)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(found, vec![(ROLE.to_string(), named.to_string())], "{roles:?}");

                let (_, first, run_step_id, next) = carried(tx, &p, false);
                assert!(matches!(next, Next::Halted(_)), "{roles:?}: {next:?}");
                let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
                assert!(ran.report.contains(named), "the refusal names the role: {}", ran.report);
                nothing_filed(tx, run_step_id, first, "nothing filed");
            });
        }
    }

    /// **The launch check names a classification on a value that is closed** (`AMB-T-5689`), apart from
    /// one this project does not have: the value is there, and the built-in would still file no task
    /// under it and halt the run.
    #[test]
    fn the_launch_check_names_a_classification_on_a_closed_value() {
        with_tx(|tx| {
            let p = picture(tx);
            // `trades`, on an axis whose values can be closed.
            let axis = crate::ops::dimension::add(
                tx,
                p.project,
                crate::ops::dimension::NewDimension {
                    name: "職能".into(),
                    role: crate::model::DimensionRole::Closable,
                    ..Default::default()
                },
            )
            .expect("axis");
            let build = crate::ops::dimension::value_add(tx, axis.id, "実装", None).expect("value");
            crate::ops::dimension::value_add(tx, axis.id, "設計", None).expect("value");
            crate::ops::dimension::value_set_closed(tx, build.id, true).expect("close it");
            answer(tx, &p, CLASSIFY, "職能=実装\n職能=設計\n職能=営業");
            let startable = ["claude".to_string()];
            let unmet = check(tx.conn(), p.automation.id, Some(&startable), nothing_asked(), None).expect("check");
            let closed: Vec<(String, String)> = unmet
                .iter()
                .filter_map(|unmet| match unmet {
                    Unmet::CfgValueClosed { step, cfg, line, builtin, placement } => {
                        let named = (step.as_str(), builtin.as_deref(), *placement);
                        assert_eq!(named, ("タスクを起票する", Some(KEY), p.make.id));
                        Some((cfg.clone(), line.clone()))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(closed, vec![(CLASSIFY.to_string(), "職能=実装".to_string())]);
            let missing: Vec<&str> = unmet
                .iter()
                .filter_map(|unmet| match unmet {
                    Unmet::CfgNotFound { line, .. } => Some(line.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(missing, vec!["職能=営業"], "a closed value was also said to be missing");
        });
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
            nothing_filed(tx, run_step_id, first, "and nothing filed");
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
        nothing_filed(tx, run_step_id, first, "nothing filed");
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
            refused(tx, &p, "職能=実装", "“Axes the AI chooses on” does not offer");
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
            refused(tx, &p, "職能=実装", "“Classification” already fixes");
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
    /// set to take the task it files can it be the entry. Nothing is handed to it at launch any more
    /// (`AMB-D-1123`), so as the entry nothing reaches its title.
    #[test]
    fn the_launch_check_asks_only_the_way_out_it_is_set_to_leave_by() {
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let automation =
                automation::add(tx, project, NewAutomation { name: "entry".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                    .expect("automation");
            let written = action(tx, "make_task").expect("the built-in's action");
            let make = automation::placement_add(tx, automation.id, written.id).expect("place it");
            // Placed first it comes set to take the task it files; a person can set it back.
            automation::cfg_set(tx, make.id, WHAT_THEN, None).expect("leave it unanswered");
            let on = AutomationPictureOwner::Automation;
            let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
            automation::edge_add(tx, on, make.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(work.id), None)
                .expect("onward");
            crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
            let automation = automation::set_entry(tx, automation.id, Some(make.id)).expect("entry");
            let startable = ["claude".to_string()];
            let unmet = |tx: &WriteTx<'_>| check(tx.conn(), automation.id, Some(&startable), nothing_asked(), None).expect("check");

            let left = unmet(tx);
            assert!(left.iter().any(|u| matches!(u, Unmet::EntryTakesNoTask { .. })), "{left:?}");
            assert!(left.iter().any(|u| matches!(u, Unmet::OpenExit { exit, .. } if exit == MADE)), "{left:?}");

            automation::cfg_set(tx, make.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
                .expect("take it");
            let taken = unmet(tx);
            assert!(!taken.iter().any(|u| matches!(u, Unmet::EntryTakesNoTask { .. })), "{taken:?}");
            assert!(!taken.iter().any(|u| matches!(u, Unmet::OpenExit { .. })), "{taken:?}");
            assert!(
                taken.iter().any(|u| matches!(u, Unmet::UnwiredInput { port, .. } if port == TITLE)),
                "nothing reaches the title: {taken:?}",
            );
        });
    }

    /// The built-in that takes a task placed as the entry, a step after it and the built-in that closes
    /// the task, the run ending where there is none to take — what a task filed at launch is worked by.
    fn entry(tx: &WriteTx<'_>) -> (Automation, AutomationPlacement, i64) {
        use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
        let project = mk_project(tx, "amenbo");
        let automation =
            automation::add(tx, project, NewAutomation { name: "entry".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
                .expect("automation");
        let take = automation::placement_add(tx, automation.id, action(tx, "take_task").expect("take").id)
            .expect("place it");
        let (_, work) = mk_placed(tx, &automation, "work", "work on it", "claude");
        let on = AutomationPictureOwner::Automation;
        automation::edge_add(tx, on, take.id, Some(TAKEN), EdgeTarget::Go(work.id), None).expect("take → work");
        automation::edge_add(tx, on, take.id, Some(NONE_TO_TAKE), EdgeTarget::Done, None).expect("none");
        crate::ops::test_support::mk_closed_after(tx, &automation, work.id, None);
        let automation = automation::set_entry(tx, automation.id, Some(take.id)).expect("entry");
        (automation, take, project)
    }

    fn launch_with(tx: &WriteTx<'_>, automation: &Automation, handed: &HandedAtLaunch) -> Result<AutomationRun> {
        let claude = ["claude".to_string()];
        let by = Launcher {
            default_agent: None,
            startable: Some(&claude),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Human),
        };
        launch_handing(tx, automation.id, &by, handed, ReadsFrom::Saved)
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

    /// Open the run's first step, the built-in that takes a task, and say which task it took.
    fn taken_first(tx: &WriteTx<'_>, run: &AutomationRun, take: &AutomationPlacement) -> crate::model::Task {
        let def = read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.placement_id == Some(take.id))
            .expect("the entry's copy");
        let claude = ["claude".to_string()];
        match open(tx, run.id, def.id, Some(&claude)).expect("open") {
            Opened::Carried { next, .. } => assert!(matches!(next, Next::Step(_)), "the run goes on: {next:?}"),
            other => panic!("the entry is carried out, not {other:?}"),
        }
        let stretch = read::automation_run_task_last(tx.conn(), run.id).expect("read").expect("stretch");
        let taken = stretch.task_id.expect("a task taken");
        read::task(tx.conn(), taken).expect("read").expect("the task")
    }

    /// **What a launch is handed is filed as a task, and the run takes it** (`AMB-D-1123`) — the title,
    /// the notes and a value on an axis, filed before the run starts and given to the AI.
    #[test]
    fn a_launch_files_what_it_was_handed_and_the_run_takes_it() {
        with_tx(|tx| {
            let (automation, take, project) = entry(tx);
            let axis = crate::ops::dimension::add(
                tx,
                project,
                crate::ops::dimension::NewDimension { name: "職能".into(), ..Default::default() },
            )
            .expect("axis");
            let build = crate::ops::dimension::value_add(tx, axis.id, "実装", None).expect("value");
            let handed = HandedAtLaunch {
                notes: Some("what to do".into()),
                ..with_values("  an issue  ", &[("職能", "実装")])
            };
            let run = launch_with(tx, &automation, &handed).expect("launch");

            let filed = taken_first(tx, &run, &take);
            assert_eq!(filed.title, "an issue");
            assert_eq!(filed.notes, "what to do");
            assert_eq!(filed.status, TaskStatus::InProgress);
            assert_eq!(filed.assignee_kind, Some(ActorKind::Ai));
            assert_eq!(filed.created_by_kind, Some(ActorKind::Human), "filed by whoever launched");
            assert!(read::assignment_id(tx.conn(), filed.id, build.id).expect("read").is_some());
        });
    }

    /// **A trigger files its task and starts a run on it once it is due** — the title and notes written
    /// on it, filed before the run as a person's would be, and taken by it. Before it is due, and while
    /// every seat of its role is taken, it decides not to, and leaves nothing behind: no run, and the
    /// time it is next asked where it was.
    #[test]
    fn a_trigger_files_its_task_and_starts_a_run_once_due() {
        use crate::model::TriggerWhen;
        use crate::ops::automation_run::{fire, triggers_due, TriggersDue};
        use crate::time::Timestamp;
        with_tx(|tx| {
            let (automation, take, _) = entry(tx);
            let every = TriggerWhen::Every { minutes: 60 };
            let trigger = automation::NewTrigger {
                name: String::new(),
                role_id: None,
                when: every,
                condition: None,
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "受信箱を見る".into(), notes: "what to do".into() },
            };
            let trigger_id = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers[0].id;
            let next_at = |tx: &WriteTx<'_>| {
                read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers.into_iter().next().expect("kept").next_at
            };
            let due = next_at(tx);
            let claude = ["claude".to_string()];
            let by = Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };
            let under_way = |tx: &WriteTx<'_>| read::automation_run_ids_under_way(tx.conn(), automation.id).expect("read");

            let early = Timestamp(due.0 - chrono::Duration::seconds(1));
            assert_eq!(triggers_due(tx.conn(), early).expect("ask"), TriggersDue { due: vec![], ask: vec![], next_at: Some(due) });
            assert!(fire(tx, trigger_id, &by, early, None).expect("fire").is_none(), "not before it is due");
            assert!(under_way(tx).is_empty(), "and no run");
            assert_eq!(next_at(tx), due);

            assert_eq!(triggers_due(tx.conn(), due).expect("ask").due, vec![trigger_id]);
            let run = fire(tx, trigger_id, &by, due, None).expect("fire").expect("a run");
            assert_eq!(run.started_by_kind, None, "nobody pressed start");
            let filed = taken_first(tx, &run, &take);
            assert_eq!(filed.title, "受信箱を見る");
            assert_eq!(filed.notes, "what to do");
            let then = every.after(due);
            assert_eq!(next_at(tx), then, "counted on from when it filed");
            let filed_at = read::automation_trigger(tx.conn(), trigger_id).expect("read").expect("there").last_filed_at;
            assert_eq!(filed_at, Some(due), "and it keeps when it filed");

            // The run is still under way, in the role's one seat.
            assert!(triggers_due(tx.conn(), then).expect("ask").due.is_empty(), "not due while it is full");
            assert!(fire(tx, trigger_id, &by, then, None).expect("fire").is_none(), "no task with no run to go to");
            assert_eq!(under_way(tx), vec![run.id]);
            assert_eq!(next_at(tx), then, "a time it decides no leaves nothing");
        });
    }

    /// **Each trigger of an automation is asked on its own** — the one that files a task moves only its
    /// own `next_at` and `last_filed_at`, the other is left where it was, and one not enabled is neither
    /// due nor fired.
    #[test]
    fn each_trigger_of_an_automation_is_asked_on_its_own() {
        use crate::model::{AutomationTrigger, TriggerWhen};
        use crate::ops::automation_run::{fire, triggers_due};
        with_tx(|tx| {
            let (automation, _, _) = entry(tx);
            crate::ops::test_support::room_for(tx, automation.id, 4);
            let trigger = automation::NewTrigger {
                name: "毎時".into(),
                role_id: None,
                when: TriggerWhen::Every { minutes: 60 },
                condition: None,
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "受信箱を見る".into(), notes: String::new() },
            };
            let first = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers.remove(0);
            let second = AutomationTrigger {
                id: read::next_id(tx.conn(), "automation_trigger").expect("id"),
                name: "毎日".into(),
                when: TriggerWhen::Every { minutes: 24 * 60 },
                order_key: crate::ops::place(&[(first.id, first.order_key.clone())], &crate::ops::Position::Bottom)
                    .expect("after it"),
                ..first.clone()
            };
            crate::ops::emit_create(tx, crate::store_engine::record::automation_trigger(&second)).expect("a second");
            let kept = read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers;
            assert_eq!(kept, vec![first.clone(), second.clone()], "both, in their order");

            let due = first.next_at;
            assert_eq!(triggers_due(tx.conn(), due).expect("ask").due, vec![first.id, second.id], "both due at once");
            let claude = ["claude".to_string()];
            let by = Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };
            assert!(fire(tx, first.id, &by, due, None).expect("fire").is_some(), "the first files its task");
            let read_one = |id: i64| read::automation_trigger(tx.conn(), id).expect("read").expect("there");
            let moved = read_one(first.id);
            assert_eq!((moved.next_at, moved.last_filed_at), (TriggerWhen::Every { minutes: 60 }.after(due), Some(due)));
            assert_eq!(read_one(second.id), second, "the other is left where it was");

            let off = AutomationTrigger { enabled: false, ..second.clone() };
            crate::ops::emit_update(
                tx,
                crate::store_engine::record::automation_trigger(&second),
                crate::store_engine::record::automation_trigger(&off),
            )
            .expect("turn it off");
            assert!(triggers_due(tx.conn(), due).expect("ask").due.is_empty(), "the one turned off is not asked");
            assert!(fire(tx, second.id, &by, due, None).expect("fire").is_none(), "nor fired");
            assert_eq!(read_one(second.id), off, "and keeps its next_at");

            automation::trigger_set(tx, automation.id, None).expect("take them away");
            assert!(read::automation_triggers_of(tx.conn(), automation.id).expect("read").is_empty(), "every one goes");
        });
    }

    /// **A trigger files its task under its role, and the task's timeline names it** — one line beside
    /// `task.created`, written only when it files one: a time it decides not to leaves none. A role of
    /// another automation is refused, and a role that goes is taken off the trigger.
    #[test]
    fn a_trigger_files_its_task_under_its_role_and_names_itself_in_the_timeline() {
        use crate::model::TriggerWhen;
        use crate::ops::automation_run::fire;
        use crate::time::Timestamp;
        with_tx(|tx| {
            let (automation, take, project) = entry(tx);
            let new_role = |name: &str| automation::NewRole { name: name.into(), ..Default::default() };
            let role = automation::role_add(tx, automation.id, new_role("さくら")).expect("a role");
            let trigger = |role_id: Option<i64>| automation::NewTrigger {
                name: "  毎時の見回り  ".into(),
                role_id,
                when: TriggerWhen::Every { minutes: 60 },
                condition: None,
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "受信箱を見る".into(), notes: String::new() },
            };

            let other = automation::add(
                tx,
                project,
                NewAutomation { name: "other".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() },
            )
            .expect("another automation");
            let theirs = automation::role_add(tx, other.id, new_role("佐藤")).expect("their role");
            let refused = automation::trigger_set(tx, automation.id, Some(trigger(Some(theirs.id))));
            assert!(matches!(refused, Err(Error::Invalid(_))), "another automation's role is refused: {refused:?}");

            let set = automation::trigger_set(tx, automation.id, Some(trigger(Some(role.id)))).expect("trigger");
            let trigger_id = set.triggers[0].id;
            let kept = read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers.into_iter().next().expect("kept");
            assert_eq!(set.triggers.first(), Some(&kept), "read back as written");
            assert_eq!((kept.name.as_str(), kept.role_id), ("毎時の見回り", Some(role.id)), "the name trimmed");

            let claude = ["claude".to_string()];
            let by = Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };
            let named = |tx: &WriteTx<'_>| {
                tx.queued_activity().into_iter().filter(|l| l.event["kind"] == "task.filed_by_trigger").collect::<Vec<_>>()
            };
            let early = Timestamp(kept.next_at.0 - chrono::Duration::seconds(1));
            assert!(fire(tx, trigger_id, &by, early, None).expect("fire").is_none(), "not before it is due");
            assert!(named(tx).is_empty(), "a time it decides no leaves no line");

            let run = fire(tx, trigger_id, &by, kept.next_at, None).expect("fire").expect("a run");
            let filed = taken_first(tx, &run, &take);
            assert_eq!(filed.role_id, Some(role.id), "filed under the trigger's role");
            assert_eq!(filed.assignee_kind, Some(ActorKind::Ai), "the AI's");
            let lines = named(tx);
            assert_eq!(lines.len(), 1, "one line: {lines:?}");
            assert_eq!(lines[0].task, Some(filed.id));
            assert_eq!(lines[0].event, serde_json::json!({ "kind": "task.filed_by_trigger", "name": "毎時の見回り" }));

            automation::role_delete(tx, role.id).expect("the role goes");
            let after = read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers.into_iter().next().expect("kept");
            assert_eq!(after.role_id, None, "the trigger lets go of it");
            assert_eq!(after.name, "毎時の見回り");
        });
    }

    /// **An idle trigger files only once no seat of its role is taken** — even with room for more beside
    /// it — and a minute after the task it last filed at the soonest. A time it says no
    /// leaves `next_at` where it was.
    #[test]
    fn an_idle_trigger_files_once_no_run_is_under_way() {
        use crate::model::TriggerWhen;
        use crate::ops::automation_run::{fire, triggers_due};
        use crate::ops::automation_stop::{stop, Ending};
        use crate::time::Timestamp;
        with_tx(|tx| {
            let (automation, take, _) = entry(tx);
            crate::ops::test_support::room_for(tx, automation.id, 2);
            let trigger = automation::NewTrigger {
                name: String::new(),
                role_id: None,
                when: TriggerWhen::Idle,
                condition: None,
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "手が空いたら見る".into(), notes: String::new() },
            };
            let written = Timestamp::now();
            let trigger_id = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers[0].id;
            let next_at = |tx: &WriteTx<'_>| {
                read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers.into_iter().next().expect("kept").next_at
            };
            let due = next_at(tx);
            assert!(due > written, "not on the spot, but counted from the write");
            let claude = ["claude".to_string()];
            let by = Launcher {
                startable: Some(&claude),
                models: nothing_asked(),
                default_agent: None,
                workspace_open: None,
                by: None,
            };
            let early = Timestamp(due.0 - chrono::Duration::seconds(1));
            assert!(fire(tx, trigger_id, &by, early, None).expect("fire").is_none(), "not before it is due");

            assert_eq!(triggers_due(tx.conn(), due).expect("ask").due, vec![trigger_id], "idle and due");
            let run = fire(tx, trigger_id, &by, due, None).expect("fire").expect("a run");
            assert_eq!(taken_first(tx, &run, &take).title, "手が空いたら見る");
            let then = TriggerWhen::Idle.after(due);
            assert_eq!(next_at(tx), then, "a minute on from when it filed");

            // One run under way, with room for another: an idle trigger still waits.
            let found = triggers_due(tx.conn(), then).expect("ask");
            assert!(found.due.is_empty() && found.next_at.is_none(), "not due while a run is going: {found:?}");
            assert!(fire(tx, trigger_id, &by, then, None).expect("fire").is_none(), "nothing filed beside it");
            assert_eq!(next_at(tx), then, "a time it decides no leaves nothing");

            stop(tx, run.id, Ending::Canceled).expect("end it");
            assert_eq!(triggers_due(tx.conn(), then).expect("ask").due, vec![trigger_id], "idle again");
            assert!(fire(tx, trigger_id, &by, then, None).expect("fire").is_some(), "so it files the next one");
        });
    }

    /// **A trigger with a condition files only while fewer tasks than it says match its filter** — on its
    /// own (`tasks`), or laid on an `every`. A time it says no leaves `next_at` where it was, and once
    /// the tasks go below it again it files the next one.
    #[test]
    fn a_trigger_with_a_condition_files_only_while_few_enough_tasks_match() {
        use crate::model::{TriggerCondition, TriggerWhen};
        use crate::ops::automation_run::{fire, triggers_due};
        use crate::ops::test_support::mk_task_in;
        with_tx(|tx| {
            let (automation, _, project) = entry(tx);
            crate::ops::test_support::room_for(tx, automation.id, 2);
            let trigger = automation::NewTrigger {
                name: String::new(),
                role_id: None,
                when: TriggerWhen::Tasks,
                condition: Some(TriggerCondition { filter: "status:todo".into(), role_id: None, below: 2 }),
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "足りなければ起こす".into(), notes: String::new() },
            };
            let trigger_id = automation::trigger_set(tx, automation.id, Some(trigger.clone())).expect("trigger").triggers[0].id;
            let next_at = |tx: &WriteTx<'_>| {
                read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers.into_iter().next().expect("kept").next_at
            };
            let due = next_at(tx);
            let claude = ["claude".to_string()];
            let by = Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };

            let waiting = mk_task_in(tx, "待っている", Some(project));
            assert_eq!(triggers_due(tx.conn(), due).expect("ask").due, vec![trigger_id], "one todo, fewer than two");
            assert!(fire(tx, trigger_id, &by, due, None).expect("fire").is_some(), "so it files one");
            let then = TriggerWhen::Tasks.after(due);
            assert_eq!(next_at(tx), then, "a minute on from when it filed");

            // Room for one more run, but two todo now: it waits.
            let another = mk_task_in(tx, "もう一つ", Some(project));
            assert!(triggers_due(tx.conn(), then).expect("ask").due.is_empty(), "not due with two todo");
            assert!(fire(tx, trigger_id, &by, then, None).expect("fire").is_none(), "nothing filed");
            assert_eq!(next_at(tx), then, "a time it decides no leaves nothing");

            for id in [waiting, another] {
                task::set_status(tx, id, TaskStatus::Done, ActorKind::Human).expect("done");
            }
            assert_eq!(triggers_due(tx.conn(), then).expect("ask").due, vec![trigger_id], "fewer again");
            assert!(fire(tx, trigger_id, &by, then, None).expect("fire").is_some(), "so it files the next one");

            // Laid on an `every`, it holds the clock back too.
            let hourly = automation::NewTrigger { when: TriggerWhen::Every { minutes: 60 }, ..trigger };
            crate::ops::test_support::room_for(tx, automation.id, 4);
            let trigger_id = automation::trigger_set(tx, automation.id, Some(hourly)).expect("an every with a condition").triggers[0].id;
            let due = next_at(tx);
            mk_task_in(tx, "三つ目", Some(project));
            mk_task_in(tx, "四つ目", Some(project));
            assert!(triggers_due(tx.conn(), due).expect("ask").due.is_empty(), "due on the clock, but two todo");
            assert!(fire(tx, trigger_id, &by, due, None).expect("fire").is_none());
            assert_eq!(next_at(tx), due);
        });
    }

    /// A task of `project` worked under `role_id`, its creation finished.
    fn mk_task_under(tx: &WriteTx<'_>, title: &str, project: i64, role_id: i64) -> i64 {
        let new = NewTask {
            title: title.into(),
            project_id: Some(project),
            due: None,
            start: None,
            priority: None,
            notes: String::new(),
            created_by_kind: None,
            at_binding_id: None,
            role_id: Some(role_id),
            made_in_run_step_id: None,
            made_in: None,
        };
        let added = task::add(tx, new).expect("a task");
        task::finish_creating(tx, added.id, ActorKind::Ai).expect("finished").id
    }

    /// **A condition that names a role counts only the tasks worked under it** — "fewer than 2 todo
    /// under this role" holds with one there, however many another role has. A role of another automation
    /// is refused, and once the role goes, the condition counts every role's tasks again.
    #[test]
    fn a_condition_naming_a_role_counts_only_the_tasks_worked_under_it() {
        use crate::model::{TriggerCondition, TriggerWhen};
        use crate::ops::automation_run::{trigger_check, ConditionCount};
        with_tx(|tx| {
            let (automation, _, project) = entry(tx);
            let new_role = |name: &str| automation::NewRole { name: name.into(), ..Default::default() };
            let research = automation::role_add(tx, automation.id, new_role("調査係")).expect("a role");
            let design = automation::role_add(tx, automation.id, new_role("設計係")).expect("another");
            mk_task_under(tx, "調べる", project, research.id);
            mk_task_under(tx, "描く", project, design.id);
            mk_task_under(tx, "もう一枚描く", project, design.id);
            let trigger = |counted: Option<i64>| automation::NewTrigger {
                name: "調査の残りを補う".into(),
                role_id: Some(research.id),
                when: TriggerWhen::Tasks,
                condition: Some(TriggerCondition { filter: "status:todo".into(), role_id: counted, below: 2 }),
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "調査を起票する".into(), notes: String::new() },
            };

            let other = automation::add(
                tx,
                project,
                NewAutomation { name: "other".into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() },
            )
            .expect("another automation");
            let theirs = automation::role_add(tx, other.id, new_role("保守係")).expect("their role");
            let refused = automation::trigger_set(tx, automation.id, Some(trigger(Some(theirs.id))));
            assert!(matches!(refused, Err(Error::Invalid(_))), "another automation's role is refused: {refused:?}");

            let set = automation::trigger_set(tx, automation.id, Some(trigger(Some(research.id)))).expect("trigger");
            let written = set.triggers[0].clone();
            let kept = read::automation_trigger(tx.conn(), written.id).expect("read").expect("there");
            assert_eq!(kept.condition.as_ref().and_then(|c| c.role_id), Some(research.id), "read back as written");
            let counted = |tx: &WriteTx<'_>| trigger_check(tx.conn(), written.id, kept.next_at).expect("check").condition;
            assert_eq!(counted(tx), Some(ConditionCount { matched: 1, below: 2 }), "only 調査係's todo");

            let everyone = automation::trigger_set(tx, automation.id, Some(trigger(None))).expect("every role");
            let check = trigger_check(tx.conn(), everyone.triggers[0].id, everyone.triggers[0].next_at).expect("check");
            assert_eq!(check.condition, Some(ConditionCount { matched: 3, below: 2 }), "every role's todo");

            let set = automation::trigger_set(tx, automation.id, Some(trigger(Some(research.id)))).expect("trigger");
            let id = set.triggers[0].id;
            automation::role_delete(tx, research.id).expect("the role goes");
            let after = read::automation_trigger(tx.conn(), id).expect("read").expect("there");
            assert_eq!(after.role_id, None, "it files its task under no role");
            assert_eq!(after.condition.as_ref().map(|c| c.role_id), Some(None), "and counts every role's tasks");
            assert_eq!(after.condition.as_ref().map(|c| c.below), Some(2), "below the same count");
        });
    }

    /// **A trigger is checked and tried without anything written** — the check counts its condition and
    /// says what holds it back now: not due yet, then no room while a run is under way, and nothing once
    /// it is due with room; a trigger not enabled is held back by that first. Trying it says what the
    /// task would say, or that the condition does not hold, and files nothing.
    #[test]
    fn a_trigger_is_checked_and_tried_without_anything_written() {
        use crate::model::{AutomationTrigger, TriggerCondition, TriggerWhen};
        use crate::ops::automation_run::{trigger_check, trigger_try, ConditionCount, TriggerHeld, TriggerTried};
        use crate::ops::test_support::mk_task_in;
        use crate::time::Timestamp;
        with_tx(|tx| {
            let (automation, _, project) = entry(tx);
            let trigger = automation::NewTrigger {
                name: "調査の残りを補う".into(),
                role_id: None,
                when: TriggerWhen::Idle,
                condition: Some(TriggerCondition { filter: "status:todo".into(), role_id: None, below: 2 }),
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "調査を起票する".into(), notes: "3 件まで".into() },
            };
            let written = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers[0].clone();
            mk_task_in(tx, "待っている", Some(project));
            let check = |tx: &WriteTx<'_>, at: Timestamp| trigger_check(tx.conn(), written.id, at).expect("check");
            let early = Timestamp(written.next_at.0 - chrono::Duration::seconds(1));

            let now = check(tx, early);
            assert_eq!(now.condition, Some(ConditionCount { matched: 1, below: 2 }));
            assert_eq!(now.held, Some(TriggerHeld::NotYet { next_at: written.next_at }), "written a moment ago");
            assert!(!now.runs_programs);
            let tried = trigger_try(&written, &now, || false);
            assert_eq!(
                tried,
                TriggerTried::Files { title: "調査を起票する".into(), notes: "3 件まで".into() },
                "tried before it is first due",
            );
            assert!(read::automation_run_ids_under_way(tx.conn(), automation.id).expect("read").is_empty(), "no run");

            assert_eq!(check(tx, written.next_at).held, None, "due, with no run under way");
            let run = launch_with(tx, &automation, &HandedAtLaunch::default()).expect("a run");
            assert_eq!(
                check(tx, written.next_at).held,
                Some(TriggerHeld::NoRoom { under_way: 1, max: 1 }),
                "an idle waits for no seat of its role to be taken",
            );
            assert_eq!(read::automation_run_ids_under_way(tx.conn(), automation.id).expect("read"), vec![run.id]);

            mk_task_in(tx, "もう一つ", Some(project));
            let full = check(tx, written.next_at);
            assert_eq!(full.condition.map(|c| c.holds()), Some(false), "two todo, not below 2");
            assert_eq!(full.held, Some(TriggerHeld::Condition), "the condition before the room");
            assert_eq!(trigger_try(&written, &full, || false), TriggerTried::Condition);

            let off = AutomationTrigger { enabled: false, ..written.clone() };
            crate::ops::emit_update(
                tx,
                crate::store_engine::record::automation_trigger(&written),
                crate::store_engine::record::automation_trigger(&off),
            )
            .expect("switched off");
            assert_eq!(check(tx, written.next_at).held, Some(TriggerHeld::Disabled));
        });
    }

    /// **A trigger with a script files only once that script said yes** — the look puts it in `ask`
    /// rather than `due`, and `fire` files nothing unless it is handed the very script the trigger has:
    /// none at all, or one rewritten since it was asked, files nothing and leaves `next_at` where it was.
    #[test]
    fn a_trigger_with_a_script_files_only_once_its_script_said_yes() {
        use crate::model::{StepScript, TriggerWhen};
        use crate::ops::automation_run::{fire, triggers_due, ScriptsSaid, TriggerScripts};
        with_tx(|tx| {
            let (automation, _, _) = entry(tx);
            // Named by its full path, as a program is written; it is never run here.
            let judge = std::env::temp_dir().join("judge").to_string_lossy().into_owned();
            let trigger = automation::NewTrigger {
                name: String::new(),
                role_id: None,
                when: TriggerWhen::Script,
                condition: None,
                script: Some(automation::NewScript {
                    program: judge.clone(),
                    args: Vec::new(),
                    timeout_minutes: Some(5),
                }),
                task: automation::NewTriggerTask::Fixed { title: "判定で起こす".into(), notes: String::new() },
            };
            let trigger_id = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers[0].id;
            let next_at = |tx: &WriteTx<'_>| {
                read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers.into_iter().next().expect("kept").next_at
            };
            let due = next_at(tx);
            let claude = ["claude".to_string()];
            let by = Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };

            let script = StepScript { program: judge, args: Vec::new(), timeout_minutes: 5 };
            let scripts = TriggerScripts { ask: Some(script.clone()), write: None };
            let found = triggers_due(tx.conn(), due).expect("ask");
            assert!(found.due.is_empty(), "not fired before its script is asked");
            assert_eq!(found.ask, vec![(trigger_id, scripts.clone())]);

            let other = TriggerScripts { ask: Some(StepScript { timeout_minutes: 6, ..script }), write: None };
            let said = |scripts: &TriggerScripts| ScriptsSaid { scripts: scripts.clone(), wrote: None };
            assert!(fire(tx, trigger_id, &by, due, None).expect("fire").is_none(), "no answer, no task");
            assert!(fire(tx, trigger_id, &by, due, Some(&said(&other))).expect("fire").is_none(), "another script's yes");
            assert_eq!(next_at(tx), due, "a time it decides no leaves nothing");

            assert!(fire(tx, trigger_id, &by, due, Some(&said(&scripts))).expect("fire").is_some(), "its own script's yes");
            assert_eq!(next_at(tx), TriggerWhen::Script.after(due), "a minute on from when it filed");
        });
    }

    /// **A trigger's task may be read from a file**, as the file stands when it fires: its first line is
    /// the title of the task filed and the rest its notes. A file it cannot read files nothing — an
    /// error, as a launch refused is — and leaves `next_at` where it was.
    #[test]
    fn a_trigger_files_the_task_its_file_says() {
        use crate::model::TriggerWhen;
        use crate::ops::automation_run::{fire, triggers_due};
        with_tx(|tx| {
            let (automation, take, _) = entry(tx);
            let dir = std::env::temp_dir().join(format!("amenbo-trigger-file-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("dir");
            let path = dir.join("task.md");
            let trigger = automation::NewTrigger {
                name: String::new(),
                role_id: None,
                when: TriggerWhen::Every { minutes: 5 },
                condition: None,
                script: None,
                task: automation::NewTriggerTask::File { path: path.to_string_lossy().into_owned() },
            };
            let trigger_id = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers[0].id;
            let next_at = |tx: &WriteTx<'_>| {
                read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers.into_iter().next().expect("kept").next_at
            };
            let due = next_at(tx);
            let claude = ["claude".to_string()];
            let by = Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };

            assert_eq!(triggers_due(tx.conn(), due).expect("ask").due, vec![trigger_id], "nothing to run first");
            assert!(fire(tx, trigger_id, &by, due, None).is_err(), "no file there to read");
            assert_eq!(next_at(tx), due, "and nothing moves on");
            assert!(read::automation_run_ids_under_way(tx.conn(), automation.id).expect("read").is_empty());

            std::fs::write(&path, "ファイルで起こす\n\n本文").expect("write");
            let run = fire(tx, trigger_id, &by, due, None).expect("fire").expect("a run");
            let filed = taken_first(tx, &run, &take);
            assert_eq!(filed.title, "ファイルで起こす");
            assert_eq!(filed.notes, "本文");
            std::fs::remove_dir_all(&dir).ok();
        });
    }

    /// **A trigger's task may be written by a program** — run outside the write with the trigger's
    /// other scripts, so the look puts it in `ask`, and `fire` files the title and notes it wrote only
    /// where they came from that very program: none at all, or one rewritten since, files nothing.
    #[test]
    fn a_trigger_files_the_task_its_program_wrote() {
        use crate::model::{StepScript, TriggerWhen};
        use crate::ops::automation_run::{fire, triggers_due, ScriptsSaid, TriggerScripts};
        with_tx(|tx| {
            let (automation, take, _) = entry(tx);
            // Named by its full path, as a program is written; it is never run here.
            let writer = std::env::temp_dir().join("write-task").to_string_lossy().into_owned();
            let trigger = automation::NewTrigger {
                name: String::new(),
                role_id: None,
                when: TriggerWhen::Idle,
                condition: None,
                script: None,
                task: automation::NewTriggerTask::Script(automation::NewScript {
                    program: writer.clone(),
                    args: Vec::new(),
                    timeout_minutes: None,
                }),
            };
            let trigger_id = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers[0].id;
            let next_at = |tx: &WriteTx<'_>| {
                read::automation(tx.conn(), automation.id).expect("read").expect("there").triggers.into_iter().next().expect("kept").next_at
            };
            let due = next_at(tx);
            let claude = ["claude".to_string()];
            let by = Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: None, by: None };

            let script = StepScript {
                program: writer,
                args: Vec::new(),
                timeout_minutes: crate::model::DEFAULT_SCRIPT_TIMEOUT_MINUTES,
            };
            let scripts = TriggerScripts { ask: None, write: Some(script.clone()) };
            let found = triggers_due(tx.conn(), due).expect("ask");
            assert!(found.due.is_empty(), "not fired before its program has written the task");
            assert_eq!(found.ask, vec![(trigger_id, scripts.clone())]);

            let wrote = Some(("書いてもらう".to_string(), "本文".to_string()));
            let other = TriggerScripts { ask: None, write: Some(StepScript { timeout_minutes: 6, ..script }) };
            assert!(fire(tx, trigger_id, &by, due, None).expect("fire").is_none(), "nothing written, no task");
            let rewritten = ScriptsSaid { scripts: other, wrote: wrote.clone() };
            assert!(fire(tx, trigger_id, &by, due, Some(&rewritten)).expect("fire").is_none(), "another program's");
            assert_eq!(next_at(tx), due, "a time it decides no leaves nothing");

            let said = ScriptsSaid { scripts, wrote };
            let run = fire(tx, trigger_id, &by, due, Some(&said)).expect("fire").expect("a run");
            let filed = taken_first(tx, &run, &take);
            assert_eq!(filed.title, "書いてもらう");
            assert_eq!(filed.notes, "本文");
            assert_eq!(next_at(tx), TriggerWhen::Idle.after(due));
        });
    }

    /// **A stopped automation's trigger files nothing**: it is not due, it decides no, and the time it
    /// is next asked stays where it was. Started again, it files its task.
    #[test]
    fn a_stopped_automations_trigger_files_nothing() {
        use crate::model::TriggerWhen;
        use crate::ops::automation_run::{fire, triggers_due, TriggersDue};
        with_tx(|tx| {
            let (automation, _, _) = entry(tx);
            let trigger = automation::NewTrigger {
                name: String::new(),
                role_id: None,
                when: TriggerWhen::Every { minutes: 5 },
                condition: None,
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "見る".into(), notes: String::new() },
            };
            let trigger_id = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers[0].id;
            automation::stop(tx, automation.id, true).expect("stop");
            let before = read::automation(tx.conn(), automation.id).expect("read").expect("there");
            let due = before.triggers[0].next_at;
            let claude = ["claude".to_string()];
            let by = Launcher {
                startable: Some(&claude),
                models: nothing_asked(),
                default_agent: None,
                workspace_open: None,
                by: None,
            };

            assert_eq!(triggers_due(tx.conn(), due).expect("ask"), TriggersDue::default(), "not asked at all");
            assert!(fire(tx, trigger_id, &by, due, None).expect("fire").is_none(), "it decides no");
            let after = read::automation(tx.conn(), automation.id).expect("read").expect("there");
            assert_eq!(after.triggers, before.triggers, "next_at is where it was");
            assert!(read::automation_run_ids_under_way(tx.conn(), automation.id).expect("read").is_empty());

            automation::stop(tx, automation.id, false).expect("start it again");
            assert_eq!(triggers_due(tx.conn(), due).expect("ask").due, vec![trigger_id]);
            assert!(fire(tx, trigger_id, &by, due, None).expect("fire").is_some(), "and it files its task");
        });
    }

    /// **A trigger whose run the launch refuses files nothing**: the refusal comes back, and the time it
    /// is next asked stays where it was.
    #[test]
    fn a_trigger_refused_at_launch_files_nothing() {
        use crate::model::TriggerWhen;
        use crate::ops::automation_run::fire;
        with_tx(|tx| {
            let (automation, _, project) = entry(tx);
            let trigger = automation::NewTrigger {
                name: String::new(),
                role_id: None,
                when: TriggerWhen::Every { minutes: 5 },
                condition: None,
                script: None,
                task: automation::NewTriggerTask::Fixed { title: "見る".into(), notes: String::new() },
            };
            let trigger_id = automation::trigger_set(tx, automation.id, Some(trigger)).expect("trigger").triggers[0].id;
            let before = read::automation(tx.conn(), automation.id).expect("read").expect("there");
            let due = before.triggers[0].next_at;
            // Nobody here can start the agent its step names, which the launch check refuses.
            let nothing: [String; 0] = [];
            let by = Launcher { default_agent: None, startable: Some(&nothing), models: nothing_asked(), workspace_open: None, by: None };

            assert!(fire(tx, trigger_id, &by, due, None).is_err(), "the launch check refuses it");
            let after = read::automation(tx.conn(), automation.id).expect("read").expect("there");
            assert_eq!(after.triggers, before.triggers, "next_at is where it was");
            assert_eq!(after.updated_at, before.updated_at);
            assert!(read::automation_run_ids_under_way(tx.conn(), automation.id).expect("read").is_empty());
            assert!(read::task_ids_in_project(tx.conn(), project).expect("tasks").is_empty(), "no task left behind");
        });
    }

    /// **The files handed over at launch are attached to the task filed** (`AMB-D-981`) — in the order
    /// they were handed, each saying who handed it, and none on the run.
    #[test]
    fn a_launch_attaches_the_files_handed_over_to_the_task_it_files() {
        use crate::model::AttachmentTarget;
        use crate::ops::automation_run::HandedFile;
        with_tx(|tx| {
            let (automation, take, _) = entry(tx);
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
                Launcher { default_agent: None, startable: Some(&claude), models: nothing_asked(), workspace_open: Some(true), by: None };
            assert!(launch_handing(tx, automation.id, &nobody, &handed, ReadsFrom::Saved).is_err());
            assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty());

            let run = launch_with(tx, &automation, &handed).expect("launch");
            let on_the_run = read::attachments_for_target(tx.conn(), AttachmentTarget::AutomationRun, run.id).expect("files");
            assert!(on_the_run.is_empty(), "nothing is held on the run");

            let filed = taken_first(tx, &run, &take);
            let files = read::attachments_for_target(tx.conn(), AttachmentTarget::Task, filed.id).expect("files");
            let names: Vec<_> = files.iter().map(|a| a.filename.clone().unwrap_or_default()).collect();
            assert_eq!(names, ["issue.md", "log.txt"]);
            assert!(files.iter().all(|a| a.created_by_kind.as_deref() == Some("human")));
        });
    }

    /// **The person launching gives a value on every axis the project requires** — nobody else is there
    /// to. Left out, the launch is refused and nothing is filed.
    #[test]
    fn a_required_axis_left_open_is_the_launchers_to_answer() {
        with_tx(|tx| {
            let (automation, take, project) = entry(tx);
            let axis = crate::ops::dimension::add(
                tx,
                project,
                crate::ops::dimension::NewDimension { name: "職能".into(), ..Default::default() },
            )
            .expect("axis");
            let build = crate::ops::dimension::value_add(tx, axis.id, "実装", None).expect("value");
            // An axis is required only once it offers a value.
            crate::ops::dimension::update(tx, axis.id, None, None, None, None, None, None, Some(true), None, None, None)
                .expect("required");

            let err = launch_with(tx, &automation, &titled("an issue")).expect_err("no value on it");
            assert!(err.to_string().contains("職能"), "{err}");
            assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty());

            let run = launch_with(tx, &automation, &with_values("an issue", &[("職能", "実装")])).expect("launch");
            let filed = taken_first(tx, &run, &take);
            assert!(read::assignment_id(tx.conn(), filed.id, build.id).expect("read").is_some());
        });
    }

    /// **A required time axis is filled, not demanded** (`AMB-D-147`): the period that contains today goes
    /// on the task filed at launch, as `task add` puts it on.
    #[test]
    fn a_required_time_axis_is_filled_from_the_period_containing_today() {
        with_tx(|tx| {
            let (automation, take, project) = entry(tx);
            let era = crate::ops::dimension::add(
                tx,
                project,
                crate::ops::dimension::NewDimension {
                    name: "フェーズ".into(),
                    role: crate::model::DimensionRole::TimeAxis,
                    ..Default::default()
                },
            )
            .expect("axis");
            let now = crate::ops::dimension::value_add(tx, era.id, "運用期", None).expect("value");
            let today = crate::time::today();
            crate::ops::dimension::value_set_dates(tx, now.id, Some(today), None).expect("period");
            crate::ops::dimension::update(tx, era.id, None, None, None, None, None, None, Some(true), None, None, None)
                .expect("required");

            let run = launch_with(tx, &automation, &titled("an issue")).expect("launch");
            let filed = taken_first(tx, &run, &take);
            assert!(read::assignment_id(tx.conn(), filed.id, now.id).expect("read").is_some());

            // Narrowed off tasks, the axis gives a task nothing (`AMB-D-789`).
            let decisions_only = Some(crate::model::DimensionAppliesTo::Decision);
            crate::ops::dimension::update(tx, era.id, None, None, None, None, None, None, None, decisions_only, None, None)
                .expect("narrowed");
            assert_eq!(task::time_axis_default(tx.conn(), project, today, &[]).expect("read"), None);
        });
    }

    /// **What a task cannot be filed from is refused before a run is made**: no title, an axis or a
    /// value the project does not have, more than one value on one axis.
    #[test]
    fn a_launch_it_could_not_file_from_makes_no_run() {
        with_tx(|tx| {
            let (automation, _, project) = entry(tx);
            let axis = crate::ops::dimension::add(
                tx,
                project,
                crate::ops::dimension::NewDimension { name: "職能".into(), ..Default::default() },
            )
            .expect("axis");
            crate::ops::dimension::value_add(tx, axis.id, "実装", None).expect("value");
            crate::ops::dimension::value_add(tx, axis.id, "設計", None).expect("value");
            let refused = |tx: &WriteTx<'_>, handed: HandedAtLaunch, what: &str| {
                let err = launch_with(tx, &automation, &handed).expect_err(what);
                assert!(err.to_string().contains(what), "{err}");
                assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty());
                assert!(read::task_ids_in_project(tx.conn(), project).expect("tasks").is_empty());
            };
            refused(tx, HandedAtLaunch { notes: Some("what to do".into()), ..Default::default() }, "needs a title");
            refused(tx, with_values("an issue", &[("職能", "営業")]), "職能=営業");
            refused(tx, with_values("an issue", &[("職能", "実装"), ("職能", "設計")]), "more than one value");
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
