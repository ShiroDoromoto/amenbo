//! **The built-ins** — steps Amenbo carries out itself rather than an agent in a terminal (`AMB-D-964`).
//!
//! Finding a task and reserving it, closing it, cutting a worktree and folding it away are all things
//! the core already does, and does the same way every time. Written into a prompt, each of them costs a
//! terminal, an agent's turns and the chance of a mistyped filter or a task left open. So they are a
//! kind of step: a run opens one where it would open a terminal, carries it out in place
//! ([`carry_out`]), and walks on from the way out it left by, the same road a step's report takes
//! ([`super::automation_report::done`]).
//!
//! **The definition is the code's.** What a built-in does, the settings it reads, the inputs it takes
//! and the ways out it leaves by are written here ([`Builtin`]) and nowhere else. The way out a built-in
//! leaves by is named by the code that runs it, so a name a person could change would be a way out the
//! code never takes.
//!
//! **Its rows are still rows.** The pictures key a way out and a port by the row that declares it
//! (`AMB-D-961`), and a run copies those ids at launch. So a built-in placed on a picture is written out
//! as the rows any step has — from the definition, the first time its action is asked for ([`action`])
//! — and carries its key so that nothing edits them afterwards
//! ([`super::automation`]'s `not_built_in`). The library's built-in action is written once per store
//! and found again by its key.
//!
//! **Nobody is chosen to carry one out.** A built-in step names no agent, so the launch check asks no
//! agent of it (`AMB-D-960`) and a placement refuses one for it.
//!
//! **One can be set to wait** ([`Waits`], `AMB-D-969`). Until what it waits for turns up, opening it
//! writes nothing, and the run stands before it as `running`; the watch opens it again on every look.
//!
//! **What drives git is done before the transaction, not in it** ([`Work::Outside`]). Opening a step is
//! one write transaction, and a fetch from a slow remote held inside it would keep every other writer to
//! the store waiting — the CLI, the GUI and the other runs. So a built-in that works outside the store
//! does its work first ([`work_outside`]), reading the store only to find its way, and the transaction
//! that opens the step writes down what came of it.

use std::borrow::Cow;

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::{
    AutomationAction, AutomationCfgKind, AutomationOwner, AutomationPictureOwner,
    AutomationPortDirection, AutomationPortKind, AutomationPortOwner, AutomationRun, AutomationRunDef,
    AutomationRunStatus, AutomationRunStep, RunDefCfg, RunDefExit, ACTION_BOUNDARY, DONE_EXIT,
    ERROR_EXIT,
};
use crate::ops::automation::{self, EdgeTarget, NewStep};
use crate::ops::automation_builtin_close::CLOSE_TASK;
use crate::ops::automation_builtin_cut::CUT_WORKTREE;
use crate::ops::automation_builtin_fetch::FETCH;
use crate::ops::automation_builtin_fold::FOLD_WORKTREE;
use crate::ops::automation_builtin_make::MAKE_TASK;
use crate::ops::automation_builtin_split::SPLIT_BY_DIM;
use crate::ops::automation_builtin_take::TAKE_TASK;
use crate::ops::automation_builtin_wait::WAIT;
use crate::ops::automation_report::{self, Next, Produced};
use crate::ops::emit_update;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// **One built-in, as Amenbo defines it.** Read by the CLI and the GUI to show what one does, and
/// written out as rows wherever one is placed.
#[derive(Serialize)]
pub struct Builtin {
    /// What names it — on a step's row, on the library's action, and on a run's copy.
    pub key: &'static str,
    /// The name its step and its library action are written with.
    pub name: &'static str,
    /// What it does, in a sentence a person building with it reads.
    pub does: &'static str,
    /// The settings it reads, answered where it is placed.
    pub settings: &'static [BuiltinSetting],
    /// What it takes in.
    pub ins: &'static [BuiltinPort],
    /// The ways out it leaves by, each with what it hands on. The error way out every step carries is
    /// not listed: a built-in that could not finish leaves by it, as any step does.
    pub exits: &'static [BuiltinExit],
    /// How it waits, for one that can be set to ([`Waits`]).
    #[serde(skip)]
    pub waits: Option<Waits>,
    /// Which of two ways out it leaves by, for one whose setting chooses ([`Chooses`]).
    #[serde(skip)]
    pub chooses: Option<Chooses>,
    /// The work itself, and where it is done ([`Work`]).
    #[serde(skip)]
    pub work: Work,
}

/// **Where a built-in's work is done.**
pub enum Work {
    /// Inside the transaction that opens its step ([`Carry`]) — work that reads and writes the store and
    /// nothing else.
    InStore(fn(&Carry<'_, '_>) -> Result<Carried>),
    /// **Before that transaction** ([`Outside`]) — work that drives git, which can take as long as the
    /// remote does. It reads the store to find its way and writes nothing; what it hands back
    /// ([`Worked`]) is written down when the step is opened.
    ///
    /// A built-in of this kind takes no input, never waits and takes no task: those are what the
    /// opening decides after this work is done, so none of them could hold it back.
    Outside(fn(&Outside<'_>) -> Result<Worked>),
    /// Inside that transaction too, but **leaving by a way out the data names** rather than one the code
    /// holds ([`Named`]) — the ways out of the built-in that splits by an axis are that axis's values
    /// (`AMB-D-972`), written onto its action from the axis rather than from [`Builtin::exits`].
    Named(fn(&Carry<'_, '_>) -> Result<Named>),
    /// **Nothing carried out: its step is held open** for as long as the answers where it was placed
    /// say ([`hold`]), and ended by the watch once that time has passed ([`time_up`]) — the built-in that
    /// waits (`AMB-D-983`). The step stands under way in the meantime, as an agent's does while it works,
    /// so a pause or a stop acts on it as on any other.
    Holds(fn(&[RunDefCfg]) -> Result<chrono::Duration>),
}

/// **How a built-in leaving by a way out the data names finished** — [`Carried`], with a name that is
/// not the code's.
#[derive(Debug)]
pub struct Named {
    pub exit: String,
    pub report: String,
}

/// **What a built-in working outside the store is handed**: the store to read, the run, and the task the
/// stretch under way is about.
pub struct Outside<'a> {
    pub conn: &'a Connection,
    pub run: &'a AutomationRun,
    /// The task the stretch under way is about, or `None` before one is taken.
    pub task_id: Option<i64>,
    /// The answers written where the step was placed.
    pub cfg: &'a [RunDefCfg],
    /// The language the report is written in ([`crate::run_wording::builtin`], `AMB-D-976`) — there is
    /// no transaction to read it off yet.
    pub language: &'a str,
}

/// **How a built-in working outside the store finished** — [`Carried`], with what it hands on through
/// the way out it leaves by, since it had no store to put that down in.
#[derive(Debug)]
pub struct Worked {
    pub exit: &'static str,
    pub report: String,
    /// Each output of that way out it fills, and the value.
    pub hands: Vec<(&'static str, String)>,
}

/// **The work a built-in did outside the store**, done before the transaction that opens its step and
/// handed to it ([`super::automation_step::open`]).
pub struct DoneOutside {
    /// The copy of the step it was done for — a result is not written down on another.
    run_def_id: i64,
    worked: Result<Worked>,
}

/// **Do a built-in's work outside the store**, for the step about to be opened — `None` where that
/// step is not a built-in working there, or its run is no longer running and will open nothing.
///
/// A work that went wrong is not an error here: it is handed on, and the step leaves by the error way
/// out, as it would have for a failure inside.
///
/// **The store may move before the step is opened**, since no transaction holds it. Where the run was
/// stopped in between, the opening refuses and what the work did stays done without a record — a
/// worktree cut and not written anywhere, which the next cut for that task is refused on.
pub fn work_outside(
    conn: &Connection,
    language: &str,
    run_id: i64,
    run_def_id: i64,
) -> Result<Option<DoneOutside>> {
    let Some(def) = read::automation_run_def(conn, run_def_id)? else {
        return Ok(None);
    };
    let Some(Work::Outside(work)) = def.builtin.as_deref().and_then(find).map(|b| &b.work) else {
        return Ok(None);
    };
    let Some(run) = read::automation_run(conn, run_id)?
        .filter(|run| run.id == def.run_id && run.status == AutomationRunStatus::Running)
    else {
        return Ok(None);
    };
    // The stretch under way, as the opening reads it: a built-in working here takes no task, so it
    // joins that one and opens none.
    let task_id = read::automation_run_task_last(conn, run_id)?.and_then(|s| s.task_id);
    let cfg: Vec<RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    let worked = work(&Outside { conn, run: &run, task_id, cfg: &cfg, language });
    Ok(Some(DoneOutside { run_def_id, worked }))
}

/// A setting a built-in reads.
#[derive(Debug, Serialize)]
pub struct BuiltinSetting {
    pub name: &'static str,
    pub kind: AutomationCfgKind,
    pub required: bool,
    /// The choice list as JSON, for a `choice` alone.
    pub options: Option<&'static str>,
}

/// An input a built-in takes, or an output it hands on.
#[derive(Debug, Serialize)]
pub struct BuiltinPort {
    pub name: &'static str,
    pub kind: AutomationPortKind,
    pub required: bool,
}

/// A way out a built-in leaves by, and what it hands on through it. [`DONE_EXIT`] is the one every
/// owner is born with; any other is declared beside it.
#[derive(Debug, Serialize)]
pub struct BuiltinExit {
    pub name: &'static str,
    pub outs: &'static [BuiltinPort],
}

/// **How a built-in waits** where it is set to, rather than leave by the way out that says there was
/// nothing to do.
///
/// **Waiting writes nothing.** The step is not opened until what it waits for has turned up, so the
/// run stands before it — `running`, with no execution under way — and every look of the watch asks
/// again ([`waiting`]). A pause or a stop acts on that run as on any other.
///
/// **What is asked is only whether one has turned up** ([`Waits::turned_up`]): it is asked once a
/// second, and a count or a list of every candidate would be asked of every task there is.
pub struct Waits {
    /// The setting that says whether it waits, and the choice on it that means it does.
    pub setting: &'static str,
    pub answer: &'static str,
    /// The way out it would otherwise leave by. Set to wait, it never does, so the launch check asks
    /// no line of it.
    pub instead_of: &'static str,
    /// Whether what it waits for is there now, read from the answers where it was placed.
    pub turned_up: fn(&Connection, &AutomationRun, &[RunDefCfg]) -> Result<bool>,
    /// What it looks for, as a person reads it beside the run while it waits — read from the same
    /// answers. What it would find is not listed or counted: that is asked of every task there is.
    pub looks_for: fn(&[RunDefCfg]) -> String,
}

impl Waits {
    /// Whether the answer written for [`Waits::setting`] is the one that means it waits. Left
    /// unanswered, it does not.
    pub fn chosen(&self, answer: Option<&str>) -> bool {
        answer.and_then(|value| serde_json::from_str::<String>(value).ok()).as_deref() == Some(self.answer)
    }
}

/// **Which of two ways out a built-in leaves by, as one setting chooses.** Every placement of it leaves
/// by one of the pair and never by the other, so the other is asked of nothing: the launch check asks no
/// line of it, and a task it would hand on through it neither makes the placement an entry nor opens a
/// stretch of the run ([`never_leaves_by`]).
pub struct Chooses {
    /// The setting that chooses, and the choice on it that means [`Chooses::chosen`].
    pub setting: &'static str,
    pub answer: &'static str,
    /// The way out it leaves by where that choice is made.
    pub chosen: &'static str,
    /// The way out it leaves by otherwise — also where the setting is left unanswered.
    pub otherwise: &'static str,
}

/// The answer written for one setting where the step was placed — JSON, as `automation_cfg.value`
/// holds it — or `None` where nobody answered it.
pub fn answer<'c>(cfg: &'c [RunDefCfg], name: &str) -> Option<&'c str> {
    cfg.iter().find(|c| c.name == name).and_then(|c| c.value.as_deref())
}

/// **Whether this copy of a step waits** rather than be opened now: a built-in set to wait, with
/// nothing yet for it to act on ([`Waits`]). Anything else — an agent's step, a built-in that never
/// waits, a key this build does not know — is opened as it always is.
pub fn waiting(conn: &Connection, run: &AutomationRun, def: &AutomationRunDef) -> Result<bool> {
    let Some(waits) = def.builtin.as_deref().and_then(find).and_then(|b| b.waits.as_ref()) else {
        return Ok(false);
    };
    let cfg: Vec<RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    if !waits.chosen(answer(&cfg, waits.setting)) {
        return Ok(false);
    }
    Ok(!(waits.turned_up)(conn, run, &cfg)?)
}

/// **What this copy of a step looks for while it waits** ([`Waits::looks_for`]), or `None` for a step
/// that does not wait.
pub fn looks_for(def: &AutomationRunDef) -> Result<Option<String>> {
    let Some(waits) = def.builtin.as_deref().and_then(find).and_then(|b| b.waits.as_ref()) else {
        return Ok(None);
    };
    let cfg: Vec<RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    Ok(Some((waits.looks_for)(&cfg)))
}

/// **The way out a placed built-in never leaves by**, as it is set there — the launch check asks no
/// line of it. `answer` reads the placement's answer to one setting.
pub fn never_leaves_by<'a>(key: &str, answer: impl Fn(&str) -> Option<&'a str>) -> Option<&'static str> {
    let builtin = find(key)?;
    if let Some(waits) = &builtin.waits {
        if waits.chosen(answer(waits.setting)) {
            return Some(waits.instead_of);
        }
    }
    let chooses = builtin.chooses.as_ref()?;
    let chosen = answer(chooses.setting).and_then(|value| serde_json::from_str::<String>(value).ok());
    Some(match chosen.as_deref() == Some(chooses.answer) {
        true => chooses.otherwise,
        false => chooses.chosen,
    })
}

/// [`never_leaves_by`], for a run's copy of a step, read from the answers copied with it. `None` for an
/// agent's step.
pub fn never_leaves_by_def(def: &AutomationRunDef) -> Result<Option<&'static str>> {
    let Some(key) = def.builtin.as_deref() else {
        return Ok(None);
    };
    let cfg: Vec<RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    Ok(never_leaves_by(key, |setting| answer(&cfg, setting)))
}

/// **What a built-in is handed while it runs**: the store, the run and the execution it is carried
/// out under, the task the stretch is about, and the answers it reads.
pub struct Carry<'a, 't> {
    pub tx: &'a WriteTx<'t>,
    pub run: &'a AutomationRun,
    pub run_step: &'a AutomationRunStep,
    /// The task the stretch under way is about, or `None` before one is taken.
    pub task_id: Option<i64>,
    pub exits: &'a [RunDefExit],
    ins: &'a [(String, Option<String>)],
    cfg: &'a [RunDefCfg],
}

impl Carry<'_, '_> {
    /// The value handed to one input, where something was.
    pub fn input(&self, name: &str) -> Option<&str> {
        self.ins.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.as_deref())
    }

    /// The answer written for one setting where the step was placed — JSON, as `automation_cfg.value`
    /// holds it — or `None` where nobody answered it.
    pub fn setting(&self, name: &str) -> Option<&str> {
        answer(self.cfg, name)
    }

    /// **Put down one thing this built-in hands on**, on the output of that name on the way out it is
    /// about to leave by — the door an agent's step puts it down through ([`automation_report::out`]).
    pub fn put(&self, exit: &str, port: &str, produced: Produced<'_>) -> Result<()> {
        let declared = self
            .exits
            .iter()
            .find(|e| e.name == exit)
            .and_then(|e| e.outs.iter().find(|p| p.name == port))
            .ok_or_else(|| {
                Error::invalid(format!("the built-in hands on no '{port}' through that way out"))
            })?;
        automation_report::out(self.tx, self.run_step.id, declared.id, produced)?;
        Ok(())
    }
}

/// **How a built-in finished**: the way out it leaves by, and the report the run keeps of it.
#[derive(Debug)]
pub struct Carried {
    pub exit: &'static str,
    pub report: String,
}

/// **Every built-in this build carries.** Each is its own module, holding its definition and the work
/// it does.
#[cfg(not(test))]
const BUILTINS: &[Builtin] =
    &[TAKE_TASK, MAKE_TASK, CUT_WORKTREE, FOLD_WORKTREE, CLOSE_TASK, FETCH, SPLIT_BY_DIM, WAIT];
#[cfg(test)]
const BUILTINS: &[Builtin] = &[
    TAKE_TASK,
    MAKE_TASK,
    CUT_WORKTREE,
    FOLD_WORKTREE,
    CLOSE_TASK,
    FETCH,
    SPLIT_BY_DIM,
    WAIT,
    tests::STAMP,
    tests::FALLS,
];

/// Every built-in, in the order a library lists them.
pub fn all() -> &'static [Builtin] {
    BUILTINS
}

/// **The built-ins a run can start at before it holds a task** (`AMB-D-970`): the one that fetches,
/// whose work needs no task and whose way on is to file one. The launch lets one stand as the entry where
/// every line out of it reaches a step that takes a task before any other step
/// ([`super::automation_run::check`]).
const BEFORE_A_TASK: &[&str] = &[FETCH.key];

/// Whether the built-in of this key works before the run holds a task ([`BEFORE_A_TASK`]).
pub fn works_before_a_task(key: &str) -> bool {
    BEFORE_A_TASK.contains(&key)
}

/// **The built-ins a run can start at** (`AMB-D-977`), in the order a picture with nothing on it offers
/// them: the one that takes a task, the one a person hands one to, and the one that fetches. The first
/// thing put on a picture has to be one of these, and it is the entry from then on
/// ([`super::automation::placement_add`]).
const ENTRIES: &[&str] = &[TAKE_TASK.key, MAKE_TASK.key, FETCH.key];

/// The keys of the built-ins a run can start at ([`ENTRIES`]).
pub fn entries() -> &'static [&'static str] {
    ENTRIES
}

/// Whether a run can start at the built-in of this key ([`ENTRIES`]).
pub fn starts_a_run(key: &str) -> bool {
    ENTRIES.contains(&key)
}

/// The built-in of this key, or `None` where this build carries none.
pub fn find(key: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.key == key)
}

/// The built-in of this key, or a refusal naming the ones there are.
fn known(key: &str) -> Result<&'static Builtin> {
    find(key).ok_or_else(|| {
        let there = BUILTINS.iter().map(|b| b.key).collect::<Vec<_>>();
        Error::not_found(match there.is_empty() {
            true => format!("Amenbo has no built-in '{key}' — this build carries none yet"),
            false => format!("Amenbo has no built-in '{key}' — the ones it has: {}", there.join(", ")),
        })
    })
}

// ───────────────────────── written out as rows ─────────────────────────

/// **The one step of a built-in's library action**, written from its definition: its ways out and
/// ports, and the settings it reads declared on the action it stands in. That action is the only place
/// a built-in is a step (`AMB-D-969`): nobody puts one inside an action they wrote.
fn step_of(tx: &WriteTx<'_>, action_id: i64, builtin: &Builtin) -> Result<crate::model::AutomationStep> {
    for setting in builtin.settings {
        automation::cfg_add(tx, action_id, setting.name, setting.kind, setting.required, setting.options)?;
    }
    let step = automation::step_add(tx, action_id, NewStep::new(builtin.name, ""))?;
    unborn_unless_declared(tx, builtin, AutomationOwner::Step, step.id)?;
    for exit in builtin.exits {
        if exit.name != DONE_EXIT {
            automation::exit_add(tx, AutomationOwner::Step, step.id, Some(exit.name))?;
        }
        let owner =
            read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, step.id, Some(exit.name))?
            .ok_or_else(|| Error::invalid("the way out was not written"))?;
        for out in exit.outs {
            automation::declare_port(
                tx,
                AutomationPortOwner::Exit,
                owner.id,
                AutomationPortDirection::Out,
                out.name,
                out.kind,
                out.required,
            )?;
        }
    }
    for input in builtin.ins {
        automation::port_add(
            tx,
            AutomationPortOwner::Step,
            step.id,
            AutomationPortDirection::In,
            input.name,
            input.kind,
            input.required,
        )?;
    }
    // Last, so every write above got past the guard that refuses a built-in's rows.
    let mut marked = step.clone();
    marked.builtin = Some(builtin.key.to_string());
    marked.updated_at = Timestamp::now();
    emit_update(tx, record::automation_action_step(&step), record::automation_action_step(&marked))?;
    Ok(marked)
}

/// **The library's action for a built-in**, written the first time it is asked for and found by its key
/// after that. It is the device's (`project_id` `None`), so every project's automations reach it.
///
/// It is the built-in as one step, joined to the action's edge the way a whole action written from one
/// prompt is ([`automation::action_from_prompt`]): each way out of the step returns by the action's way
/// out of the same name, what it hands on is carried out through it, and each input the action takes is
/// wired in.
pub fn action(tx: &WriteTx<'_>, key: &str) -> Result<AutomationAction> {
    let builtin = known(key)?;
    if builtin.key == SPLIT_BY_DIM.key {
        return Err(Error::invalid(format!(
            "the built-in '{key}' splits by an axis, and there is one of it per axis — name the axis"
        )));
    }
    if let Some(written) = read::automation_action_builtin(tx.conn(), key)? {
        return Ok(written);
    }
    write_action(tx, builtin, None, None)
}

/// **The library action for a built-in, on an axis where it splits by one** — [`action`] for any other,
/// and for the built-in that splits by an axis, the one written for that axis
/// ([`crate::ops::automation_builtin_split::action`]). An axis named for any other is refused rather
/// than dropped.
pub fn action_on(tx: &WriteTx<'_>, key: &str, axis: Option<i64>) -> Result<AutomationAction> {
    match (key == SPLIT_BY_DIM.key, axis) {
        (true, Some(axis)) => crate::ops::automation_builtin_split::action(tx, axis),
        (false, Some(_)) => Err(Error::invalid(format!(
            "the built-in '{key}' does not split by an axis, so it takes none"
        ))),
        (_, None) => action(tx, key),
    }
}

/// **Write a built-in's library action** from its definition — in the device's library, or in the
/// project's where it splits by that project's axis (`AMB-D-972`), marked with the axis too.
pub(crate) fn write_action(
    tx: &WriteTx<'_>,
    builtin: &Builtin,
    project_id: Option<i64>,
    axis: Option<i64>,
) -> Result<AutomationAction> {
    let action = automation::action_add(tx, project_id, builtin.name, builtin.does)?;
    unborn_unless_declared(tx, builtin, AutomationOwner::Action, action.id)?;
    let step = step_of(tx, action.id, builtin)?;
    for exit in builtin.exits {
        if exit.name != DONE_EXIT {
            automation::exit_add(tx, AutomationOwner::Action, action.id, Some(exit.name))?;
        }
        let owner =
            read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, action.id, Some(exit.name))?
                .ok_or_else(|| Error::invalid("the way out was not written"))?;
        for out in exit.outs {
            automation::declare_port(
                tx,
                AutomationPortOwner::Exit,
                owner.id,
                AutomationPortDirection::Out,
                out.name,
                out.kind,
                out.required,
            )?;
        }
    }
    let every_exit = builtin.exits.iter().map(|e| e.name).chain([ERROR_EXIT]);
    for name in every_exit {
        automation::edge_add(
            tx,
            AutomationPictureOwner::Action,
            step.id,
            Some(name),
            EdgeTarget::Exit(Some(name.to_string())),
            None,
        )?;
    }
    for exit in builtin.exits {
        for out in exit.outs {
            automation::wire_add(
                tx,
                AutomationPictureOwner::Action,
                step.id,
                Some(exit.name),
                out.name,
                ACTION_BOUNDARY,
                out.name,
            )?;
        }
    }
    for input in builtin.ins {
        automation::port_add(
            tx,
            AutomationPortOwner::Action,
            action.id,
            AutomationPortDirection::In,
            input.name,
            input.kind,
            input.required,
        )?;
        automation::wire_add(
            tx,
            AutomationPictureOwner::Action,
            ACTION_BOUNDARY,
            None,
            input.name,
            step.id,
            input.name,
        )?;
    }
    let entered = automation::action_set_entry(tx, action.id, Some(step.id))?;
    let mut marked = entered.clone();
    marked.builtin = Some(builtin.key.to_string());
    marked.builtin_dimension_id = axis;
    marked.updated_at = Timestamp::now();
    emit_update(tx, record::automation_action(&entered), record::automation_action(&marked))?;
    Ok(marked)
}

/// **Every owner is born with [`DONE_EXIT`]; a built-in keeps it only where it leaves by it.** Left
/// standing with nothing after it, it would be a way out the launch check asks a line for and the code
/// never takes.
fn unborn_unless_declared(
    tx: &WriteTx<'_>,
    builtin: &Builtin,
    owner: AutomationOwner,
    owner_id: i64,
) -> Result<()> {
    if builtin.exits.iter().any(|e| e.name == DONE_EXIT) {
        return Ok(());
    }
    if let Some(done) = read::automation_exit_by_name(tx.conn(), owner, owner_id, Some(DONE_EXIT))? {
        automation::exit_delete(tx, done.id)?;
    }
    Ok(())
}

// ───────────────────────── carried out ─────────────────────────

/// **Carry a built-in step out**, where [`super::automation_step::open`] would have opened a terminal on
/// it, and report it done the way an agent's step reports — so what the run does next is read off the
/// same way out, by the same door.
///
/// **A built-in that could not finish leaves by the error way out**, with what went wrong as its report,
/// and the picture decides what follows (left alone, a person is called). So does a copy whose key this
/// build does not know — a store carried back to an older Amenbo — and a way out the code named that
/// the copy does not declare.
///
/// **A built-in working outside the store is not worked here** ([`Work::Outside`]): what it did is
/// `outside`, done before this transaction, and only written down. Handed nothing, it leaves by the
/// error way out rather than drive git with every other writer waiting.
#[allow(clippy::too_many_arguments)]
pub(crate) fn carry_out(
    tx: &WriteTx<'_>,
    run: &AutomationRun,
    run_step: &AutomationRunStep,
    def: &AutomationRunDef,
    exits: &[RunDefExit],
    ins: &[(String, Option<String>)],
    task_id: Option<i64>,
    outside: Option<DoneOutside>,
) -> Result<Next> {
    let cfg: Vec<RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    let key = def.builtin.as_deref().unwrap_or_default();
    let carried = known(key).and_then(|builtin| {
        let carry = Carry { tx, run, run_step, task_id, exits, ins, cfg: &cfg };
        match &builtin.work {
            Work::InStore(work) => work(&carry).map(|c| (Cow::Borrowed(c.exit), c.report)),
            Work::Named(work) => work(&carry).map(|n| (Cow::Owned(n.exit), n.report)),
            Work::Outside(_) => match outside.filter(|done| done.run_def_id == def.id) {
                Some(done) => done.worked.and_then(|worked| {
                    for (port, value) in &worked.hands {
                        carry.put(worked.exit, port, Produced::Value(value))?;
                    }
                    Ok((Cow::Borrowed(worked.exit), worked.report))
                }),
                None => Err(Error::invalid(format!(
                    "the built-in '{key}' works outside the store, and that was not done before its step was opened"
                ))),
            },
            Work::Holds(_) => Err(Error::invalid(format!(
                "the built-in '{key}' holds its step open, and is not carried out"
            ))),
        }
    });
    let (exit, report) = match carried {
        Ok(carried) => carried,
        Err(e) => (Cow::Borrowed(ERROR_EXIT), e.to_string()),
    };
    let leaves_by = |name: &str| exits.iter().find(|e| e.name == name).map(|e| e.id);
    let Some(exit_id) = leaves_by(&exit) else {
        return fell_over(tx, run_step, exits, &format!("the built-in '{key}' left by a way out this step does not declare"));
    };
    match automation_report::done(tx, run_step.id, Some(exit_id), &report) {
        Ok(next) => Ok(next),
        // What the code named would not finish the step — a required output it did not put down, or a
        // report it owed. That is the built-in falling over, and it is said as such.
        Err(e) if exit != ERROR_EXIT => fell_over(tx, run_step, exits, &e.to_string()),
        Err(e) => Err(e),
    }
}

// ───────────────────────── held open ─────────────────────────

/// **How opening a built-in that holds its step went** ([`Work::Holds`]).
pub(crate) enum Held {
    /// The step stands under way until its time has passed.
    Holding,
    /// What it was set to could not be waited on, so it left by the error way out, and this is what
    /// that way out leads to.
    FellOver(Next),
}

/// How long this copy of a step is held open, or `None` for one that is not ([`Work::Holds`]).
fn holds_for(def: &AutomationRunDef) -> Option<Result<chrono::Duration>> {
    let Work::Holds(how_long) = def.builtin.as_deref().and_then(find)?.work else {
        return None;
    };
    Some(serde_json::from_str::<Vec<RunDefCfg>>(&def.cfg).map_err(Error::from).and_then(|cfg| how_long(&cfg)))
}

/// **Hold a built-in's step open** where [`carry_out`] would carry it out — `None` for a step that is not
/// held. Nothing is written: the execution already stands under way, and when it ends is read off it
/// ([`held_until`]). Answers that say no length of time leave by the error way out at once, saying why.
pub(crate) fn hold(
    tx: &WriteTx<'_>,
    run_step: &AutomationRunStep,
    def: &AutomationRunDef,
    exits: &[RunDefExit],
) -> Result<Option<Held>> {
    match holds_for(def) {
        None => Ok(None),
        Some(Ok(_)) => Ok(Some(Held::Holding)),
        Some(Err(e)) => Ok(Some(Held::FellOver(fell_over(tx, run_step, exits, &e.to_string())?))),
    }
}

/// **When a held step's time comes** — the moment it was opened, plus how long the answers where it
/// was placed say. Both are written once and never change, so this is read rather than kept. `None` for
/// a step that is not held.
pub fn held_until(conn: &Connection, run_step: &AutomationRunStep) -> Result<Option<Timestamp>> {
    let Some(def) = read::automation_run_def(conn, run_step.run_def_id)? else {
        return Ok(None);
    };
    let Some(how_long) = holds_for(&def) else {
        return Ok(None);
    };
    let started = run_step.started_at.unwrap_or(run_step.created_at);
    Ok(Some(Timestamp(started.0 + how_long?)))
}

/// **The held step of this run whose time has come by `now`**, or `None` — asked by the thread that
/// keeps runs going, on the look it takes at a run with a step under way. Only a run still running is
/// asked about: a run that is over closed its held step as it ended, and a stop is the end of the wait.
pub fn due(conn: &Connection, run_id: i64, now: Timestamp) -> Result<Option<i64>> {
    let running = read::automation_run(conn, run_id)?.is_some_and(|run| run.status == AutomationRunStatus::Running);
    if !running {
        return Ok(None);
    }
    let Some(last) = read::automation_run_steps_of(conn, run_id)?.pop() else {
        return Ok(None);
    };
    if last.status != crate::model::AutomationRunStepStatus::Running {
        return Ok(None);
    }
    Ok(match held_until(conn, &last) {
        Ok(Some(until)) if until.0 <= now.0 => Some(last.id),
        _ => None,
    })
}

/// **End a held step whose time has come**: it leaves by [`DONE_EXIT`] with how long it waited as its
/// report, and the run walks on from there as after an agent's report — a pause asked for meanwhile
/// takes hold here. A step that is not held, or whose time has not come ([`due`]), is refused.
pub fn time_up(tx: &WriteTx<'_>, run_step_id: i64) -> Result<Next> {
    time_up_at(tx, run_step_id, Timestamp::now())
}

pub(crate) fn time_up_at(tx: &WriteTx<'_>, run_step_id: i64, now: Timestamp) -> Result<Next> {
    let run_step = read::automation_run_step(tx.conn(), run_step_id)?
        .ok_or_else(|| Error::not_found(format!("step execution '{run_step_id}' not found")))?;
    if due(tx.conn(), run_step.run_id, now)? != Some(run_step_id) {
        return Err(Error::invalid(format!(
            "step execution '{run_step_id}' is not a wait whose time has come"
        )));
    }
    let def = read::automation_run_def(tx.conn(), run_step.run_def_id)?
        .ok_or_else(|| Error::not_found(format!("step of a run '{}' not found", run_step.run_def_id)))?;
    let how_long = holds_for(&def).ok_or_else(|| Error::invalid("the step is not held"))??;
    let exits: Vec<RunDefExit> = serde_json::from_str(&def.exits).map_err(Error::from)?;
    let done = exits
        .iter()
        .find(|e| e.name == DONE_EXIT)
        .ok_or_else(|| Error::invalid("the step carries no done way out"))?;
    automation_report::done(tx, run_step_id, Some(done.id), &waited(tx.language(), how_long))
}

/// **The report a held step leaves with** — how long it waited, as it was set.
fn waited(language: &str, how_long: chrono::Duration) -> String {
    let s = how_long.num_seconds();
    let [hours, minutes, seconds] = [s / 3600, s % 3600 / 60, s % 60].map(|n| n.to_string());
    crate::run_wording::builtin(
        language,
        "waited",
        &[("hours", &hours), ("minutes", &minutes), ("seconds", &seconds)],
    )
}

/// Leave by the error way out, saying why.
fn fell_over(
    tx: &WriteTx<'_>,
    run_step: &AutomationRunStep,
    exits: &[RunDefExit],
    why: &str,
) -> Result<Next> {
    let error = exits
        .iter()
        .find(|e| e.name == ERROR_EXIT)
        .ok_or_else(|| Error::invalid("the step carries no error way out"))?;
    automation_report::done(tx, run_step.id, Some(error.id), why)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A built-in that puts down the value it was handed, stamped — and leaves by "stamped".
    pub(super) const STAMP: Builtin = Builtin {
        key: "test_stamp",
        name: "Stamp",
        does: "hands on what it was given, stamped",
        settings: &[BuiltinSetting {
            name: "stamp",
            kind: AutomationCfgKind::Text,
            required: false,
            options: None,
        }],
        ins: &[BuiltinPort { name: "note", kind: AutomationPortKind::Value, required: false }],
        exits: &[
            BuiltinExit {
                name: "stamped",
                outs: &[BuiltinPort { name: "stamped", kind: AutomationPortKind::Value, required: true }],
            },
            BuiltinExit { name: DONE_EXIT, outs: &[] },
        ],
        waits: None,
        chooses: None,
        work: Work::InStore(|carry| {
            let stamp = carry.setting("stamp").unwrap_or("\"ok\"");
            let note = carry.input("note").unwrap_or("nothing");
            let stamped = format!("{note} {stamp}");
            carry.put("stamped", "stamped", Produced::Value(&stamped))?;
            Ok(Carried { exit: "stamped", report: format!("stamped {note}") })
        }),
    };

    /// A built-in that falls over every time.
    pub(super) const FALLS: Builtin = Builtin {
        key: "test_falls",
        name: "Falls",
        does: "never finishes",
        settings: &[],
        ins: &[],
        exits: &[BuiltinExit { name: DONE_EXIT, outs: &[] }],
        waits: None,
        chooses: None,
        work: Work::InStore(|_| Err(Error::invalid("the floor gave way"))),
    };

    use crate::model::{ActorKind, Automation, AutomationPlacement, AutomationRunStatus, AutomationStep};
    use crate::ops::automation::NewAutomation;
    use crate::ops::automation_run::{check, launch_past_the_task_checks as launch, nothing_asked, Launcher, Unmet};
    use crate::ops::automation_step::Opened;
    use crate::ops::test_support::open;
    use crate::ops::test_support::{mk_out, mk_placed, mk_project, mk_task_in, with_tx};

    /// An agent's step that takes a task and hands on a note through "found", followed by the built-in
    /// `key` placed on its own, fed the note. Every way out of both is decided.
    struct Picture {
        automation: Automation,
        project: i64,
        first: AutomationPlacement,
        builtin: AutomationPlacement,
    }

    fn picture(tx: &WriteTx<'_>, key: &str) -> Picture {
        let project = mk_project(tx, "amenbo");
        let automation =
            automation::add(tx, project, NewAutomation { name: "loop".into(), ..Default::default() })
                .expect("automation");
        let (first_action, first) = mk_placed(tx, &automation, "Take", "take one", "claude");
        crate::ops::test_support::mk_exit(tx, &first_action, "found");
        mk_out(tx, &first_action, Some("found"), "task", AutomationPortKind::TaskTake, true);
        mk_out(tx, &first_action, Some("found"), "note", AutomationPortKind::Value, true);
        let builtin_action = action(tx, key).expect("the built-in's action");
        let builtin = automation::placement_add(tx, automation.id, builtin_action.id).expect("place it");
        let on = AutomationPictureOwner::Automation;
        if find(key).expect("known").ins.iter().any(|i| i.name == "note") {
            automation::wire_add(tx, on, first.id, Some("found"), "note", builtin.id, "note").expect("wire");
        }
        automation::edge_add(tx, on, first.id, Some("found"), EdgeTarget::Go(builtin.id), None).expect("on");
        automation::edge_add(tx, on, first.id, None, EdgeTarget::Done, None).expect("closes");
        for exit in find(key).expect("known").exits {
            automation::edge_add(tx, on, builtin.id, Some(exit.name), EdgeTarget::Done, None).expect("closes");
        }
        let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        Picture { automation, project, first, builtin }
    }

    fn launched(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        let startable = vec!["claude".to_string()];
        let by = Launcher {
            startable: Some(&startable),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        };
        launch(tx, automation.id, &by).expect("launch")
    }

    fn def_of(tx: &WriteTx<'_>, run: &AutomationRun, placement: &AutomationPlacement) -> AutomationRunDef {
        read::automation_run_defs_of(tx.conn(), run.id)
            .expect("defs")
            .into_iter()
            .find(|d| d.placement_id == Some(placement.id))
            .expect("the spot's copy")
    }

    /// Walk the first step: take a task, hand on the note, leave by "found". What comes back is the
    /// copy the run opens next.
    fn past_the_first(tx: &WriteTx<'_>, p: &Picture, run: &AutomationRun) -> AutomationRunDef {
        let startable = vec!["claude".to_string()];
        let opening = match open(tx, run.id, def_of(tx, run, &p.first).id, Some(&startable)).expect("open") {
            Opened::Ready(opening) => *opening,
            other => panic!("the agent's step opens a terminal, not {other:?}"),
        };
        let task = mk_task_in(tx, "one", Some(p.project));
        automation_report::take(tx, opening.run_step.id, task).expect("take");
        let note = crate::ops::test_support::out_port(tx, opening.run_step.id, None, "note");
        automation_report::out(tx, opening.run_step.id, note, Produced::Value("hello")).expect("note");
        let found = crate::ops::test_support::way_out(tx, opening.run_step.id, "found");
        match automation_report::done(tx, opening.run_step.id, found, "took one").expect("done") {
            Next::Step(def) => *def,
            other => panic!("the run goes on to the built-in, not {other:?}"),
        }
    }

    /// **A built-in working outside the store takes no input, never waits and takes no task** — the
    /// opening decides those after its work is done, so none of them may hold that work back.
    #[test]
    fn a_built_in_working_outside_is_held_back_by_nothing_the_opening_decides() {
        for builtin in all().iter().filter(|b| matches!(b.work, Work::Outside(_))) {
            assert!(builtin.ins.is_empty(), "{} takes an input", builtin.key);
            assert!(builtin.waits.is_none(), "{} can wait", builtin.key);
            let takes = builtin.exits.iter().flat_map(|e| e.outs).any(|p| p.kind == AutomationPortKind::TaskTake);
            assert!(!takes, "{} takes a task", builtin.key);
        }
    }

    /// **A built-in's action is written once, from the definition, and found again after that** — one
    /// step marked with the key, carrying exactly the ways out and outputs the definition names, and
    /// joined to the action's edge so a placement of it can be wired and walked.
    #[test]
    fn a_builtin_action_is_written_from_its_definition_once() {
        with_tx(|tx| {
            let written = action(tx, "test_stamp").expect("write");
            assert_eq!(written.builtin.as_deref(), Some("test_stamp"));
            assert_eq!(written.project_id, None, "the device's library, reached from every project");
            assert_eq!(action(tx, "test_stamp").expect("again").id, written.id, "one per key");

            let steps = read::automation_action_steps_of(tx.conn(), written.id).expect("steps");
            let [step]: [AutomationStep; 1] = steps.try_into().expect("one step");
            assert_eq!(step.builtin.as_deref(), Some("test_stamp"));
            assert_eq!(written.entry_step_id, Some(step.id));
            let names = |owner, id| -> Vec<String> {
                read::automation_exits_of(tx.conn(), owner, id)
                    .expect("exits")
                    .into_iter()
                    .map(|e| e.name)
                    .collect()
            };
            let expected =
                vec![DONE_EXIT.to_string(), ERROR_EXIT.to_string(), "stamped".to_string()];
            assert_eq!(names(AutomationOwner::Step, step.id), expected);
            assert_eq!(names(AutomationOwner::Action, written.id), expected);

            // A built-in that leaves by the done way out alone keeps the pair every owner is born with.
            let falls = action(tx, "test_falls").expect("write");
            let falls_step = written_step(tx, falls.id);
            assert_eq!(
                names(AutomationOwner::Step, falls_step.id),
                vec![DONE_EXIT.to_string(), ERROR_EXIT.to_string()],
                "the one it declares is 完了",
            );

            assert!(action(tx, "no_such").is_err(), "a key this build does not carry is refused");
        });
    }

    fn written_step(tx: &WriteTx<'_>, action_id: i64) -> AutomationStep {
        read::automation_action_steps_of(tx.conn(), action_id).expect("steps").remove(0)
    }

    /// **Nothing a built-in declares is edited by hand** — not its action, not its step, not a way out
    /// or an output of either — and nobody is chosen to carry one out.
    #[test]
    fn a_builtin_is_not_edited_and_names_no_agent() {
        with_tx(|tx| {
            let p = picture(tx, "test_stamp");
            let written = action(tx, "test_stamp").expect("found");
            let step = written_step(tx, written.id);
            let refused = |r: Result<()>, what: &str| {
                let e = r.expect_err(what);
                assert!(e.to_string().contains("built into Amenbo"), "{what}: {e}");
            };
            refused(automation::action_update(tx, written.id, Some("mine"), None).map(drop), "rename the action");
            refused(automation::action_delete(tx, written.id), "delete the action");
            refused(
                automation::step_update(tx, step.id, None, Some("do more"), None, None, None, None, None, None, None).map(drop),
                "rewrite the step",
            );
            refused(
                automation::exit_add(tx, AutomationOwner::Step, step.id, Some("another")).map(drop),
                "add a way out to the step",
            );
            let stamped = read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, step.id, Some("stamped"))
                .expect("read")
                .expect("stamped");
            refused(automation::exit_rename(tx, stamped.id, Some("done")).map(drop), "rename its way out");
            refused(automation::step_delete(tx, step.id), "take the step out of the action");
            refused(
                automation::cfg_add(tx, written.id, "more", AutomationCfgKind::Text, false, None).map(drop),
                "declare another setting",
            );

            let chosen = automation::placement_step_set(tx, p.builtin.id, step.id, "claude", None);
            assert!(chosen.is_err(), "nobody is chosen for a built-in");

            // The placement's own answers are the automation's, not the built-in's.
            automation::cfg_set(tx, p.builtin.id, "stamp", Some("\"seen\"")).expect("answer the setting");
        });
    }

    /// **The launch asks no agent of a built-in, and opening it carries it out** — no terminal: it reads
    /// what it was handed and its setting, puts its output down, leaves by the way out the code named,
    /// and the run walks on from there exactly as after an agent's report.
    #[test]
    fn a_builtin_step_is_carried_out_where_it_is_opened() {
        with_tx(|tx| {
            let p = picture(tx, "test_stamp");
            let startable = vec!["claude".to_string()];
            let unmet = check(tx.conn(), p.automation.id, Some(&startable), nothing_asked()).expect("check");
            // The picture ends with the task open, and takes it at a step of its own rather than a
            // built-in — each its own reason, and neither the one asked here.
            let gaps: Vec<_> = unmet
                .iter()
                .filter(|u| !matches!(u, Unmet::LeavesTaskOpen { .. } | Unmet::HandsOnTaskTaken { .. }))
                .collect();
            assert!(gaps.is_empty(), "nobody chosen for the built-in is not a gap: {gaps:?}");

            automation::cfg_set(tx, p.builtin.id, "stamp", Some("\"seen\"")).expect("answer");
            let run = launched(tx, &p.automation);
            let next = past_the_first(tx, &p, &run);
            assert_eq!(next.builtin.as_deref(), Some("test_stamp"));
            assert_eq!(next.agent, "", "its copy names nobody");
            assert_eq!(next.prompt, None, "and carries no prompt");
            // The picture ends after the built-in, and a run does not complete with its task still in
            // progress (`AMB-D-967`), so the task is closed as a step would have closed it.
            let task = read::automation_run_task_last(tx.conn(), run.id)
                .expect("read")
                .and_then(|s| s.task_id)
                .expect("the task the first step took");
            crate::ops::task::set_status(tx, task, crate::model::TaskStatus::Done).expect("close");

            // Only "claude" can be started here, and the built-in is opened all the same.
            let (run_step_id, next) = match open(tx, run.id, next.id, Some(&startable)).expect("open") {
                Opened::Carried { run_step_id, next } => (run_step_id, next),
                other => panic!("a built-in is carried out, not {other:?}"),
            };
            assert!(matches!(next, Next::Closed(_)), "\"stamped\" closes the run: {next:?}");

            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert_eq!(ran.report, "stamped hello");
            let stamped = crate::ops::test_support::way_out(tx, run_step_id, "stamped");
            assert_eq!(ran.exit_id, stamped);
            let outs: Vec<_> = read::automation_run_values_of(tx.conn(), run_step_id)
                .expect("values")
                .into_iter()
                .filter(|v| v.direction == AutomationPortDirection::Out)
                .collect();
            assert_eq!(outs.len(), 1);
            assert_eq!(outs[0].value.as_deref(), Some("hello \"seen\""));
            let run = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(run.status, AutomationRunStatus::Completed);
        });
    }

    /// **A built-in that falls over leaves by the error way out**, with what went wrong as its report —
    /// and with no line on it the run halts and calls a person, as it would for an agent's step.
    #[test]
    fn a_builtin_that_falls_over_leaves_by_the_error_way_out() {
        with_tx(|tx| {
            let p = picture(tx, "test_falls");
            let run = launched(tx, &p.automation);
            let next = past_the_first(tx, &p, &run);
            let (run_step_id, next) = match open(tx, run.id, next.id, None).expect("open") {
                Opened::Carried { run_step_id, next } => (run_step_id, next),
                other => panic!("a built-in is carried out, not {other:?}"),
            };
            assert!(matches!(next, Next::Halted(_)), "the error way out halts: {next:?}");
            let ran = read::automation_run_step(tx.conn(), run_step_id).expect("read").expect("row");
            assert!(ran.report.contains("the floor gave way"), "{}", ran.report);
            assert_eq!(ran.exit_id, crate::ops::test_support::way_out(tx, run_step_id, ERROR_EXIT));
            let run = read::automation_run(tx.conn(), run.id).expect("read").expect("run");
            assert_eq!(run.status, AutomationRunStatus::Failed);
        });
    }

    /// **The screen's table of a built-in's words is this definition's.** The GUI draws
    /// a built-in's words in the screen's language by looking the store's word up among the Japanese
    /// dictionary's `auto.bi.<built-in>.*` entries (`app/src/core/i18n/builtinKeys.ts`). A word renamed
    /// here and not there would be drawn untranslated, and one left there would translate nothing — so
    /// both sides are held to the same set, a built-in at a time.
    #[test]
    fn the_japanese_dictionary_holds_every_word_of_every_builtin() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/core/i18n/locales/ja.ts");
        let ja = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for builtin in all().iter().filter(|one| !one.key.starts_with("test_")) {
            // `take_task` is written `takeTask` in the dictionary's keys.
            let section: String = builtin
                .key
                .split('_')
                .enumerate()
                .map(|(nth, part)| match nth {
                    0 => part.to_string(),
                    _ => part[..1].to_uppercase() + &part[1..],
                })
                .collect();
            let head = format!("\"auto.bi.{section}.");
            let written: std::collections::BTreeSet<String> = ja
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with(&head))
                .map(|line| {
                    let value = line.split_once(": ").expect("a `key: value` line").1.trim_end_matches(',');
                    serde_json::from_str::<String>(value).expect("a quoted value")
                })
                .collect();
            let mut words: std::collections::BTreeSet<String> =
                [builtin.name, builtin.does].into_iter().map(str::to_string).collect();
            for setting in builtin.settings {
                words.insert(setting.name.to_string());
                if let Some(options) = setting.options {
                    words.extend(serde_json::from_str::<Vec<String>>(options).expect("options are a JSON list"));
                }
            }
            words.extend(builtin.ins.iter().map(|port| port.name.to_string()));
            for exit in builtin.exits {
                words.insert(exit.name.to_string());
                words.extend(exit.outs.iter().map(|port| port.name.to_string()));
            }
            assert_eq!(written, words, "`{}`'s words against the dictionary's `auto.bi.{section}.*`", builtin.key);
        }
    }
}
