//! **Launching an automation** — what refuses to start one, and the run the launch writes.
//!
//! Nothing on the definition side refuses an unfinished automation ([`crate::ops::automation`]): a step
//! with no way onward, an automation with no entry, a required setting nobody answered — each of them
//! saves, because building happens in whatever order the author likes. **This is where they are
//! refused**, which is the moment a person is actually about to be let down by them.
//!
//! **There are four entrances and one launch** — the CLI, a workspace's empty frame, a task's own
//! screen, and the automation tab. Putting the check and the copy in one place is what keeps them
//! from behaving differently depending on which was pressed.
//!
//! **The check ([`check`]) and the refusal ([`launch`]) are separate doors on purpose.** A build screen
//! draws the list while nobody has pressed anything, so it asks for the list; a launch that cannot go
//! ahead raises it as one refusal carrying a part per reason ([`Msg::part`]), the way a reservation
//! does. Neither of them holds a code of its own yet — the codes are split off a family where a screen
//! puts the refusal in front of a person (`app/src/core/errorCodes.ts`), and the screen that will draw
//! these is not built.
//!
//! **Three things the store cannot answer are handed in** ([`Launcher`]): which agents this machine can
//! actually start, which models each of them offers, and whether the workspace is open. The first two
//! are the reader's own login shell ([`crate::wake`], [`crate::agent_models`]) and the third a window on
//! their screen. Read here, they would be read from whatever process happened to be running — which is
//! the reason [`crate::ops::MadeIn`] is handed in too.
//!
//! **A run stops reading the definition the moment it starts.** Every step inside every placed action
//! is copied into `automation_run_def` at launch — one column of them, each saying which placement it
//! was opened from, with the wires joined to each input resolved into it — so editing the automation
//! afterwards cannot change what a run already under way is doing, and a run stays readable months
//! later when the automation it came from has moved on.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::model::{
    ActorKind, AttachmentTarget, Automation, AutomationCfg, AutomationCfgOwner, AutomationEnds, AutomationExit,
    AutomationOwner, AutomationPictureOwner, AutomationPlacement, AutomationPlacementStep,
    AutomationPortDirection,
    AutomationPortKind, AutomationPortOwner, AutomationRun, AutomationRunDef, AutomationRunStatus,
    AutomationRunStepStatus, AutomationStep, AutomationEdge,
    RunDefCfg, RunDefExit, RunDefIn, RunDefLine, RunDefPort, RunDefSource, ACTION_BOUNDARY, ERROR_EXIT,
};
use crate::ops::{automation, emit_create};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// **One thing the launch check found missing.** Ten of them, and every one is something a person can
/// go and fix in the build screen — which is why each names where it is rather than only what it is.
///
/// They are a type rather than ten sentences because both doors need them: the refusal writes them out
/// as English, and the build screen draws them as a list beside the step each belongs to.
///
/// **`placement` is the placement on the automation's picture the reason is about** — the one standing
/// where the gap is, or holding the action whose picture has it. A name alone cannot say which: the same
/// action placed twice is two boxes with one name, and a screen that opens the box a reason is about has
/// to know which of the two. So the same gap in an action placed twice is two reasons, one per placement.
/// The two reasons about the automation as a whole ([`Unmet::NoSteps`], [`Unmet::NoEntry`]) name none.
///
/// **`builtin` is the key of the built-in a name came from** (`AMB-D-964`), and `to_builtin` the same
/// for `to`. A built-in's step, ways out, inputs and settings are named in the store's Japanese, so a
/// screen drawn in another language turns them into its own — and only a name carrying a key is
/// turned, since a person's action may use the same word for something of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unmet {
    /// Nothing is placed on the automation at all. Nothing else is worth saying about it.
    NoSteps,
    /// No placement is named as the entry, so there is nowhere for a run to start and nothing is
    /// reachable. The first placement is the entry (`AMB-D-977`), so only a store written before that
    /// holds one of these, and replacing the entry gives it one.
    NoEntry,
    /// An action standing on the picture has no step to open — none written yet, or none named as its
    /// entry. A run reaching that spot would have no terminal to put up.
    ActionEmpty { action: String, placement: i64 },
    /// The entry declares no `task_take` output, so no step of the run would ever come to hold a task
    /// and every step after it would be about nothing.
    EntryTakesNoTask { step: String, builtin: Option<String>, placement: i64 },
    /// A way out with nothing set to happen after it. The run would reach it and stop. The error way
    /// out is not one of these — it is carried from birth and halts unless somebody says otherwise.
    OpenExit { step: String, exit: String, builtin: Option<String>, placement: i64 },
    /// A required input with nothing reaching it — no wire at all, or none whose far end is both
    /// declared and reachable from the entry.
    UnwiredInput { step: String, port: String, builtin: Option<String>, placement: i64 },
    /// A required setting nobody answered while building.
    UnansweredCfg { step: String, cfg: String, builtin: Option<String>, placement: i64 },
    /// A step nobody has been chosen to carry out where its action is placed (`AMB-D-960`). A pane
    /// opened on it would have no agent to start.
    AgentUnchosen { step: String, placement: i64 },
    /// A step asking for an agent this machine cannot start. A pane opened on it would come up on
    /// `command not found`.
    AgentMissing { step: String, agent: String, placement: i64 },
    /// A step naming a model its agent does not offer here. The pane would come up, and the agent would
    /// turn the model down inside it — which is a refusal the reader only meets once the run is away.
    ///
    /// Only raised for an agent that has already been asked what it offers, and that answered with a
    /// list ([`ModelsHere`]).
    ModelMissing { step: String, agent: String, model: String, placement: i64 },
    /// A line a run could walk while it holds the task it took, and that goes on to take another one
    /// (`to` naming where) or ends the run (`to` `None`) without the task being closed or a person being
    /// called on the way (`AMB-D-967`). The task would be left in progress with no run holding it.
    ///
    /// `step` and `exit` name the way out the line leaves by. Lines inside one task — a review sending
    /// the work back, say — are not asked about: only the ones leading out of it.
    LeavesTaskOpen {
        step: String,
        exit: String,
        to: Option<String>,
        builtin: Option<String>,
        to_builtin: Option<String>,
        placement: i64,
    },
}

impl Unmet {
    /// The English sentence, which is what the CLI prints and what any surface holding no dictionary
    /// falls back to.
    pub fn say(&self) -> String {
        match self {
            Unmet::NoSteps => "no action is placed on it".to_string(),
            Unmet::NoEntry => {
                "no placement is named as the entry — choose what a run starts at with `automation entry-replace`"
                    .to_string()
            }
            Unmet::ActionEmpty { action, .. } => {
                format!("the action '{action}' placed on it has no step to start at")
            }
            Unmet::EntryTakesNoTask { step, .. } => {
                format!(
                    "the entry '{step}' takes no task — draw every line out of it on to a step that takes one, \
                     or change what a run starts at with `automation entry-replace`"
                )
            }
            Unmet::OpenExit { step, exit, .. } => {
                format!("nothing is set to happen after {} of '{step}'", named(exit))
            }
            Unmet::UnwiredInput { step, port, .. } => {
                format!("the required input '{port}' of '{step}' has nothing reaching it")
            }
            Unmet::UnansweredCfg { step, cfg, .. } => {
                format!("the required setting '{cfg}' of '{step}' is unanswered")
            }
            Unmet::AgentUnchosen { step, .. } => {
                format!("nobody is chosen to carry out '{step}' where its action is placed")
            }
            Unmet::AgentMissing { step, agent, .. } => {
                format!("'{step}' asks for '{agent}', which this machine cannot start")
            }
            Unmet::ModelMissing { step, agent, model, .. } => {
                format!("'{step}' asks for the model '{model}', which '{agent}' here does not offer")
            }
            Unmet::LeavesTaskOpen { step, exit, to, .. } => {
                let from = format!("{} of '{step}'", named(exit));
                match to {
                    Some(to) => format!(
                        "{from} goes on to '{to}', which takes another task, with the task taken before \
                         still open — close it or call a person on the way"
                    ),
                    None => format!(
                        "{from} ends the run with the task it took still open — close it or call a \
                         person on the way"
                    ),
                }
            }
        }
    }
}

impl Unmet {
    /// The code naming **this** reason, so a screen can write it in the reader's language
    /// ([`crate::ErrorCode`], `AMB-D-413`). [`Unmet::say`] is the English the CLI prints and the
    /// fallback for a surface holding no dictionary; this is the other half of the same sentence.
    pub fn code(&self) -> ErrorCode {
        match self {
            Unmet::NoSteps => ErrorCode::NotReadyAutomationNoSteps,
            Unmet::NoEntry => ErrorCode::NotReadyAutomationNoEntry,
            Unmet::ActionEmpty { .. } => ErrorCode::NotReadyAutomationActionEmpty,
            Unmet::EntryTakesNoTask { .. } => ErrorCode::NotReadyAutomationEntryTakesNoTask,
            Unmet::OpenExit { .. } => ErrorCode::NotReadyAutomationOpenExit,
            Unmet::UnwiredInput { .. } => ErrorCode::NotReadyAutomationUnwiredInput,
            Unmet::UnansweredCfg { .. } => ErrorCode::NotReadyAutomationUnansweredCfg,
            Unmet::AgentUnchosen { .. } => ErrorCode::NotReadyAutomationAgentUnchosen,
            Unmet::AgentMissing { .. } => ErrorCode::NotReadyAutomationAgentMissing,
            Unmet::ModelMissing { .. } => ErrorCode::NotReadyAutomationModelMissing,
            Unmet::LeavesTaskOpen { to: Some(_), .. } => ErrorCode::NotReadyAutomationTaskLeftOpen,
            Unmet::LeavesTaskOpen { to: None, .. } => ErrorCode::NotReadyAutomationTaskLeftOpenAtEnd,
        }
    }

    /// This reason as a sentence that carries its values apart from its words: the code above, the
    /// English underneath, and every name the template interpolates as a field of its own.
    ///
    /// Public because both doors hand the same thing over — the refusal raises these as its parts, and
    /// the check's own list is drawn from them (`app/src-tauri/src/automation.rs`). Read twice, the two
    /// lists came apart (`AMB-T-5287`).
    pub fn msg(&self) -> Msg {
        let msg = Msg::new(self.say()).coded(self.code());
        let msg = match self.builtin() {
            Some(key) => msg.with("builtin", key),
            None => msg,
        };
        let msg = match self.placement() {
            Some(id) => msg.with("placement", id),
            None => msg,
        };
        match self {
            Unmet::NoSteps | Unmet::NoEntry => msg,
            Unmet::ActionEmpty { action, .. } => msg.with("action", action),
            Unmet::EntryTakesNoTask { step, .. } => msg.with("step", step),
            Unmet::OpenExit { step, exit, .. } => msg.with("step", step).with("exit", exit),
            Unmet::UnwiredInput { step, port, .. } => msg.with("step", step).with("port", port),
            Unmet::UnansweredCfg { step, cfg, .. } => msg.with("step", step).with("cfg", cfg),
            Unmet::AgentUnchosen { step, .. } => msg.with("step", step),
            Unmet::AgentMissing { step, agent, .. } => msg.with("step", step).with("agent", agent),
            Unmet::ModelMissing { step, model, .. } => msg.with("step", step).with("model", model),
            Unmet::LeavesTaskOpen { step, exit, to, to_builtin, .. } => {
                let msg = msg.with("step", step).with("exit", exit);
                let msg = match to {
                    Some(to) => msg.with("to", to),
                    None => msg,
                };
                match to_builtin {
                    Some(key) => msg.with("to_builtin", key),
                    None => msg,
                }
            }
        }
    }

    /// The placement this reason is about, or `None` for one about the automation as a whole.
    pub fn placement(&self) -> Option<i64> {
        match self {
            Unmet::NoSteps | Unmet::NoEntry => None,
            Unmet::ActionEmpty { placement, .. }
            | Unmet::EntryTakesNoTask { placement, .. }
            | Unmet::OpenExit { placement, .. }
            | Unmet::UnwiredInput { placement, .. }
            | Unmet::UnansweredCfg { placement, .. }
            | Unmet::AgentUnchosen { placement, .. }
            | Unmet::AgentMissing { placement, .. }
            | Unmet::ModelMissing { placement, .. }
            | Unmet::LeavesTaskOpen { placement, .. } => Some(*placement),
        }
    }

    /// The key of the built-in `step` came from, or `None` for a person's own action or step.
    fn builtin(&self) -> Option<&str> {
        match self {
            Unmet::EntryTakesNoTask { builtin, .. }
            | Unmet::OpenExit { builtin, .. }
            | Unmet::UnwiredInput { builtin, .. }
            | Unmet::UnansweredCfg { builtin, .. }
            | Unmet::LeavesTaskOpen { builtin, .. } => builtin.as_deref(),
            _ => None,
        }
    }
}

/// How a way out is spoken of in a sentence: by its name.
fn named(exit: &str) -> String {
    format!("the way out '{exit}'")
}

/// **What the store cannot answer**, handed to [`launch`] by whoever pressed it.
pub struct Launcher<'a> {
    /// The agent ids a pane can actually be opened on ([`crate::wake::startable`]), or **`None` for a
    /// machine that has not been asked**.
    ///
    /// `None` is not an empty list and must not be read as one (`AMB-D-792`): the probe starts a login
    /// shell and can be abandoned on a deadline, and an unanswered probe drawn as an answer would tell
    /// a reader with four agents installed that they have none. Where it is `None` the agent check is
    /// not made, rather than made and failed.
    pub startable: Option<&'a [String]>,
    /// The models each agent offers on this machine ([`ModelsHere`]) — empty from a caller that has
    /// asked nobody, which is every caller outside the app.
    pub models: &'a ModelsHere,
    /// Whether the talk window is open — `Some(false)` refuses, and **`None` is a caller that cannot
    /// see** (`AMB-D-792`'s discipline, the same one [`Launcher::startable`] takes).
    ///
    /// A run's steps are drawn in the window's panes, so a launch made with it closed has nowhere to
    /// put them, and opening it is the workspace's own act rather than this one's. But only a caller
    /// inside the app can answer: a terminal somewhere else knows nothing about what is on screen, and
    /// a `false` written there would refuse a launch the reader could see perfectly well. So it says
    /// nothing instead, and the run waits for whatever opens its first step.
    pub workspace_open: Option<bool>,
    /// Who pressed launch, or `None` from a caller that says nothing about itself.
    pub by: Option<ActorKind>,
}

/// **What a person hands over when launching a run** — the entrance where a person hands something
/// over (`AMB-D-970`). The first step the run opens is told it ([`crate::ops::automation_step`]).
///
/// **Only the built-in that files a task reads anything** (`AMB-D-981`): the title, the notes and the
/// classification of the task it files, and the files, which it attaches to that task. A launch handing
/// what its entry does not read is refused rather than left lying on the run where nothing reads it
/// ([`entry_reads`]).
///
/// Nothing handed is the default, and a launch that hands nothing starts as it always has.
#[derive(Clone, Debug, Default)]
pub struct HandedAtLaunch {
    /// A text on its own. **No entry reads one**, so a launch handing one is refused — words for the task
    /// go in its notes. Blank text is the same as none.
    pub text: Option<String>,
    /// The files, already ingested into the blob store by the caller ([`crate::blob::BlobStore`]), in
    /// the order they were handed over. Each is attached to the run in the launch's own transaction,
    /// and moved from there on to the task the entry files
    /// ([`crate::ops::automation_builtin_make`]).
    pub files: Vec<HandedFile>,
    /// The title of the task an entry that files one files. Blank is the same as none.
    pub title: Option<String>,
    /// Its notes. Blank is the same as none.
    pub notes: Option<String>,
    /// Its classification, as (axis, value) by name, in the order handed.
    pub classification: Vec<(String, String)>,
}

impl HandedAtLaunch {
    fn text(&self) -> Option<&str> {
        self.text.as_deref().filter(|t| !t.trim().is_empty())
    }

    fn title(&self) -> Option<&str> {
        self.title.as_deref().map(str::trim).filter(|t| !t.is_empty())
    }

    fn notes(&self) -> Option<&str> {
        self.notes.as_deref().filter(|t| !t.trim().is_empty())
    }

    /// What was handed for a task to be filed from: a title, notes, a classification or a file.
    fn for_a_task(&self) -> bool {
        self.title().is_some() || self.notes().is_some() || !self.classification.is_empty() || !self.files.is_empty()
    }
}

/// **The task a run starts by filing, as handed over at launch** — kept on the run as JSON
/// (`automation_run.handed_task`) and read by the built-in that files it, as the inputs it would
/// otherwise be wired
/// ([`crate::ops::automation_builtin_make`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandedTask {
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    /// One `axis=value` a line — the shape the built-in's input for a chosen classification reads.
    #[serde(default)]
    pub classification: Option<String>,
}

impl HandedTask {
    /// The one kept on `run`, or `None` where it was handed none.
    pub fn of(run: &AutomationRun) -> Result<Option<HandedTask>> {
        run.handed_task.as_deref().map(|json| serde_json::from_str(json).map_err(Error::from)).transpose()
    }
}

/// One file handed over at launch, as [`crate::ops::attachment::add_blob`] takes it.
#[derive(Clone, Debug)]
pub struct HandedFile {
    pub blob_hash: String,
    pub filename: String,
    pub mime: Option<String>,
    pub size_bytes: i64,
}

/// **The models each agent offers here**, by agent id ([`crate::agent_models`]) — and only the agents
/// that have already been asked and answered with a list.
///
/// An agent **absent from the map is one this machine has nothing to say about**, and its steps' models
/// are left unjudged rather than judged and failed — the discipline [`Launcher::startable`] takes for the
/// same reason (`AMB-D-792`). Absent covers two cases that a screen cannot tell apart and need not:
/// nobody has asked it yet, and it was asked and could offer nothing (not signed in, no such flag, an
/// answer in a shape nothing reads). Both are "no list", never "no models".
///
/// Asking is what keeps it out of the map by default: one ask is a login shell plus a provider starting
/// up, and a check that put that behind every draw of a build screen would charge a reader seconds for
/// opening a picture (`AMB-D-865`).
pub type ModelsHere = BTreeMap<String, Vec<String>>;

/// **A caller that has asked no provider anything** — the map every [`ModelsHere`] slot takes where the
/// question cannot be put at all.
///
/// Handed back by reference rather than made at each call so a `Launcher` can hold it: a terminal is
/// outside the process that keeps the answers ([`crate::agent_models`]), and putting the question there
/// would start a provider per launch for a check the app has already made.
pub fn nothing_asked() -> &'static ModelsHere {
    static EMPTY: std::sync::OnceLock<ModelsHere> = std::sync::OnceLock::new();
    EMPTY.get_or_init(ModelsHere::new)
}

/// `<what> '<id>' not found`, the uncoded refusal the automation entities take
/// ([`crate::ops::automation`] says why).
fn not_found(what: &str, id: i64) -> Error {
    Error::not_found(format!("{what} '{id}' not found"))
}

/// **Is this automation ready to be launched?** An empty answer is yes.
///
/// The list is walked in display order, so a person reading it walks their own picture. Two of the ten
/// answer alone: an automation with nothing placed on it has nothing else to say about it, and one with
/// no entry has nothing reachable to say it about — every other check is asked of the placements a run
/// would actually walk, and with no entry that is none of them.
///
/// **What is asked of a placement is asked of the action standing on it**, and then of every step the
/// action could open ([`steps_opened_by`]): a way out inside with nothing after it and a required input
/// inside that nothing reaches stop a run just as surely as the same gaps on the automation's picture
/// ([`inside`]). The agent and the model are asked of each step as it is placed here — they are chosen
/// where the action is placed, step by step (`AMB-D-960`).
///
/// `startable` is [`Launcher::startable`], and `None` leaves the agent check unmade. `models` is
/// [`Launcher::models`], and an agent it says nothing about leaves that step's model check unmade.
pub fn check(
    conn: &Connection,
    automation_id: i64,
    startable: Option<&[String]>,
    models: &ModelsHere,
) -> Result<Vec<Unmet>> {
    let automation = read::automation(conn, automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    let placements = read::automation_placements_of(conn, automation_id)?;
    if placements.is_empty() {
        return Ok(vec![Unmet::NoSteps]);
    }
    let Some(entry_id) = automation.entry_placement_id else {
        return Ok(vec![Unmet::NoEntry]);
    };
    let by_id: BTreeMap<i64, &AutomationPlacement> = placements.iter().map(|p| (p.id, p)).collect();
    let live = reachable(conn, entry_id, &by_id)?;

    let mut unmet = Vec::new();
    if let Some(entry) = by_id.get(&entry_id) {
        if !takes_a_task(conn, entry)? && !takes_one_first(conn, entry, &by_id)? {
            unmet.push(Unmet::EntryTakesNoTask {
                step: action_name(conn, entry.action_id)?,
                builtin: action_builtin(conn, entry.action_id)?,
                placement: entry.id,
            });
        }
    }
    for placement in placements.iter().filter(|p| live.contains(&p.id)) {
        let name = action_name(conn, placement.action_id)?;
        let builtin = action_builtin(conn, placement.action_id)?;
        // A built-in set to wait never leaves by the way out it waits instead of (`AMB-D-969`), and one
        // whose setting chooses its way out never by the other, so nothing has to follow it.
        let never_taken = never_taken(conn, placement)?;
        let settings = settings_of(conn, placement)?;
        for exit in read::automation_exits_of(conn, AutomationOwner::Action, placement.action_id)? {
            if never_taken.is_some() && Some(exit.name.as_str()) == never_taken {
                continue;
            }
            // The error way out is the one nobody has to answer for. Every step and every action is
            // born carrying it (crate::ops::automation), so asking for an edge on each of them
            // would put one more thing to write on every action somebody places — for the case that
            // is already handled. Left alone it halts the run and calls a person, and an edge on it
            // is how somebody says otherwise.
            if exit.name == ERROR_EXIT {
                continue;
            }
            if !decided(conn, placement.id, exit.id, &by_id)? {
                unmet.push(Unmet::OpenExit {
                    step: name.clone(),
                    exit: exit.name.clone(),
                    builtin: builtin.clone(),
                    placement: placement.id,
                });
            }
        }
        for port in read::automation_ports_of(
            conn,
            AutomationPortOwner::Action,
            placement.action_id,
            AutomationPortDirection::In,
        )? {
            if port.required && !fed(conn, placement, port.id, entry_id, &live, &by_id)? {
                unmet.push(Unmet::UnwiredInput {
                    step: name.clone(),
                    port: port.name,
                    builtin: builtin.clone(),
                    placement: placement.id,
                });
            }
        }
        for cfg in settings {
            if cfg.required && cfg.value.is_none() {
                unmet.push(Unmet::UnansweredCfg {
                    step: name.clone(),
                    cfg: cfg.name,
                    builtin: builtin.clone(),
                    placement: placement.id,
                });
            }
        }
        let steps = steps_opened_by(conn, placement.action_id)?;
        if steps.is_empty() {
            unmet.push(Unmet::ActionEmpty { action: name.clone(), placement: placement.id });
            continue;
        }
        push_new(&mut unmet, inside(conn, placement, &steps, entry_id, &live, &by_id)?);
        // A built-in names no agent and no model: Amenbo carries it out itself (`AMB-D-964`).
        for step in steps.iter().filter(|step| step.builtin.is_none()) {
            let mut found = Vec::new();
            let Some(chosen) = read::automation_placement_step_for(conn, placement.id, step.id)? else {
                found.push(Unmet::AgentUnchosen { step: step.name.clone(), placement: placement.id });
                push_new(&mut unmet, found);
                continue;
            };
            if let Some(startable) = startable {
                if !startable.iter().any(|id| id == &chosen.agent) {
                    found.push(Unmet::AgentMissing {
                        step: step.name.clone(),
                        agent: chosen.agent.clone(),
                        placement: placement.id,
                    });
                }
            }
            // Asked of the chosen model, and only where the agent offered a list (`ModelsHere`). A
            // choice naming no model is on whatever the provider's own settings have, which is not a
            // name this could judge.
            if let (Some(model), Some(offered)) = (chosen.model.as_deref(), models.get(&chosen.agent)) {
                if !offered.iter().any(|id| id == model) {
                    found.push(Unmet::ModelMissing {
                        step: step.name.clone(),
                        agent: chosen.agent.clone(),
                        model: model.to_string(),
                        placement: placement.id,
                    });
                }
            }
            push_new(&mut unmet, found);
        }
    }
    push_new(&mut unmet, leaves_task_open(conn, &live, &by_id)?);
    Ok(unmet)
}

/// **The lines that leave a taken task open** (`AMB-D-967`), walked from every way out that hands a
/// task on.
///
/// While a run holds the task it took, the walk goes on through the placements it reaches and stops at
/// the two that settle the task: the built-in that closes it, and a line that stops the run and calls a
/// person (the error way out with nothing after it is one — it halts). Reaching the run's end, or a
/// placement that takes a task, before either is the line refused. A placement already walked is not
/// walked again, so a line back within the task — a review sending the work back — is followed once
/// and asked nothing.
///
/// **The picture inside an action is opened too.** A line in there can end the run on its own, and
/// that is the same task left open. What takes a task and what closes one are asked of the action as a
/// whole: taking is declared on the action's ways out, and the built-in that closes stands on a
/// placement of its own (`AMB-D-969`).
fn leaves_task_open(
    conn: &Connection,
    live: &BTreeSet<i64>,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<Vec<Unmet>> {
    let mut unmet = Vec::new();
    for &start in live {
        let Some(placement) = by_id.get(&start) else { continue };
        let mut lines = Vec::new();
        for exit in read::automation_exits_of(conn, AutomationOwner::Action, placement.action_id)? {
            if Some(exit.name.as_str()) == never_taken(conn, placement)? {
                continue;
            }
            if outs_of(conn, &exit)?.iter().any(|p| p.kind == AutomationPortKind::TaskTake) {
                lines.push((*placement, exit));
            }
        }
        let mut walked = BTreeSet::new();
        while let Some((from, exit)) = lines.pop() {
            let Some(edge) =
                read::automation_edge_for_exit(conn, AutomationPictureOwner::Automation, from.id, exit.id)?
            else {
                // Nothing drawn: the error way out halts, and any other is refused as an open way out.
                continue;
            };
            let to = match edge.ends {
                AutomationEnds::Halt | AutomationEnds::Exit => continue,
                AutomationEnds::Done => {
                    push_new(
                        &mut unmet,
                        vec![Unmet::LeavesTaskOpen {
                            step: action_name(conn, from.action_id)?,
                            exit: exit.name.clone(),
                            to: None,
                            builtin: action_builtin(conn, from.action_id)?,
                            to_builtin: None,
                            placement: from.id,
                        }],
                    );
                    continue;
                }
                AutomationEnds::Go => match edge.to_id.and_then(|id| by_id.get(&id)) {
                    Some(to) => *to,
                    None => continue,
                },
            };
            if closes_the_task(conn, to.action_id)? {
                continue;
            }
            if takes_a_task(conn, to)? {
                push_new(
                    &mut unmet,
                    vec![Unmet::LeavesTaskOpen {
                        step: action_name(conn, from.action_id)?,
                        exit: exit.name.clone(),
                        to: Some(action_name(conn, to.action_id)?),
                        builtin: action_builtin(conn, from.action_id)?,
                        to_builtin: action_builtin(conn, to.action_id)?,
                        placement: from.id,
                    }],
                );
                continue;
            }
            if !walked.insert(to.id) {
                continue;
            }
            push_new(&mut unmet, ends_inside(conn, to)?);
            for exit in read::automation_exits_of(conn, AutomationOwner::Action, to.action_id)? {
                lines.push((to, exit));
            }
        }
    }
    Ok(unmet)
}

/// Whether a placement of this action closes the task the run holds — the built-in that does
/// (`AMB-D-964`).
fn closes_the_task(conn: &Connection, action_id: i64) -> Result<bool> {
    Ok(action_builtin(conn, action_id)?.as_deref() == Some(crate::ops::automation_builtin_close::CLOSE_TASK.key))
}

/// The lines inside the action standing on one placement that end the run, each named by the step and
/// the way out it leaves by.
fn ends_inside(conn: &Connection, placement: &AutomationPlacement) -> Result<Vec<Unmet>> {
    let mut found = Vec::new();
    for step in steps_opened_by(conn, placement.action_id)? {
        for exit in read::automation_exits_of(conn, AutomationOwner::Step, step.id)? {
            let edge =
                read::automation_edge_for_exit(conn, AutomationPictureOwner::Action, step.id, exit.id)?;
            if edge.is_some_and(|e| e.ends == AutomationEnds::Done) {
                found.push(Unmet::LeavesTaskOpen {
                    step: step.name.clone(),
                    exit: exit.name,
                    to: None,
                    builtin: step.builtin.clone(),
                    to_builtin: None,
                    placement: placement.id,
                });
            }
        }
    }
    Ok(found)
}

/// **The way out a line is keyed to**, where that row is still one the given owner declares — `None`
/// for a line keyed to nothing, or to a row that is gone or belongs elsewhere.
fn declared_exit(
    conn: &Connection,
    exit_id: Option<i64>,
    owner_kind: AutomationOwner,
    owner_id: i64,
) -> Result<Option<AutomationExit>> {
    let Some(id) = exit_id else { return Ok(None) };
    Ok(read::automation_exit(conn, id)?
        .filter(|exit| exit.owner_kind == owner_kind && exit.owner_id == owner_id))
}

/// **What is missing inside the action standing on one placement** — the same two questions the
/// placement is asked, put to every step a run could open inside it ([`steps_opened_by`]).
///
/// **A way out of a step is decided** where a line inside the action goes on from it: to another of its
/// steps, to closing or halting the run, or back to one of the action's ways out — and that last only
/// where the action really declares the way out it names. What follows the action's way out is the
/// automation's to say, and asked of the placement above. The error way out is left alone here for the
/// reason it is left alone there.
///
/// **A required input of a step is reached** by a wire inside the action from a step the run could
/// open, whose way out hands on a port of that name, or by a wire from the action itself
/// ([`ACTION_BOUNDARY`]) — which counts only where the action declares that input and something on the
/// automation's picture reaches it on this placement ([`fed`]). It is the same walk a run makes when it
/// hands a step its values ([`crate::ops::automation_step`]).
fn inside(
    conn: &Connection,
    placement: &AutomationPlacement,
    steps: &[AutomationStep],
    entry_id: i64,
    live: &BTreeSet<i64>,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<Vec<Unmet>> {
    let opened: BTreeSet<i64> = steps.iter().map(|s| s.id).collect();
    let wires = read::automation_wires_of(conn, AutomationPictureOwner::Action, placement.action_id)?;
    let mut unmet = Vec::new();
    for step in steps {
        for exit in read::automation_exits_of(conn, AutomationOwner::Step, step.id)? {
            if exit.name == ERROR_EXIT {
                continue;
            }
            let edge =
                read::automation_edge_for_exit(conn, AutomationPictureOwner::Action, step.id, exit.id)?;
            let decided = match edge {
                None => false,
                Some(edge) => match edge.ends {
                    AutomationEnds::Done | AutomationEnds::Halt => true,
                    AutomationEnds::Go => edge.to_id.is_some_and(|to| opened.contains(&to)),
                    AutomationEnds::Exit => {
                        declared_exit(conn, edge.exit_to_id, AutomationOwner::Action, placement.action_id)?
                            .is_some()
                    }
                },
            };
            if !decided {
                unmet.push(Unmet::OpenExit {
                    step: step.name.clone(),
                    exit: exit.name.clone(),
                    builtin: step.builtin.clone(),
                    placement: placement.id,
                });
            }
        }
        for port in read::automation_ports_of(
            conn,
            AutomationPortOwner::Step,
            step.id,
            AutomationPortDirection::In,
        )? {
            if !port.required {
                continue;
            }
            let mut reached = false;
            for wire in wires.iter().filter(|w| w.to_id == step.id && w.to_port_id == port.id) {
                reached = if wire.from_id == ACTION_BOUNDARY {
                    let declared = read::automation_ports_of(
                        conn,
                        AutomationPortOwner::Action,
                        placement.action_id,
                        AutomationPortDirection::In,
                    )?
                    .iter()
                    .any(|p| p.id == wire.from_port_id);
                    declared && fed(conn, placement, wire.from_port_id, entry_id, live, by_id)?
                } else if opened.contains(&wire.from_id) {
                    let exit = declared_exit(conn, wire.from_exit_id, AutomationOwner::Step, wire.from_id)?;
                    match exit {
                        Some(exit) => {
                            outs_of(conn, &exit)?.iter().any(|p| p.id == wire.from_port_id)
                        }
                        None => false,
                    }
                } else {
                    false
                };
                if reached {
                    break;
                }
            }
            if !reached {
                unmet.push(Unmet::UnwiredInput {
                    step: step.name.clone(),
                    port: port.name,
                    builtin: step.builtin.clone(),
                    placement: placement.id,
                });
            }
        }
    }
    Ok(unmet)
}

/// Add what one placement is missing, leaving out what is already said. A reason names its placement, so
/// the same gap in an action placed twice is kept once per placement — what this leaves out is the same
/// line found again, as the walk for lines that leave a task open does from several starts.
fn push_new(unmet: &mut Vec<Unmet>, found: Vec<Unmet>) {
    for one in found {
        if !unmet.contains(&one) {
            unmet.push(one);
        }
    }
}

/// The key of the built-in standing on a placement, or `None` for an action somebody wrote — what lets
/// a screen turn [`action_name`] into its own language (`AMB-D-964`).
fn action_builtin(conn: &Connection, action_id: i64) -> Result<Option<String>> {
    Ok(read::automation_action(conn, action_id)?.and_then(|a| a.builtin))
}

/// What a placement is called in a refusal: the name of the action standing on it.
fn action_name(conn: &Connection, action_id: i64) -> Result<String> {
    Ok(read::automation_action(conn, action_id)?
        .map(|a| a.name)
        .unwrap_or_else(|| format!("action '{action_id}'")))
}

/// **The steps a run opens when it reaches one action** — the one it starts at, and every step the
/// picture inside leads on to from there. An action with no entry opens none of them, which is what
/// [`Unmet::ActionEmpty`] is raised on.
///
/// Which of them one run walks is decided by the ways out taken while it goes; this is every one it
/// could walk, in display order, and that is what the agent and the model are asked of (`AMB-D-960`)
/// and what a launch copies into the run ([`snapshot`]).
/// A step nothing inside leads to is left out for the reason [`reachable`] leaves a placement out: no
/// pane ever comes up on it, so refusing the launch over the agent it names would hold a run back for
/// a box still being drawn.
fn steps_opened_by(conn: &Connection, action_id: i64) -> Result<Vec<crate::model::AutomationStep>> {
    let Some(entry) = read::automation_action(conn, action_id)?.and_then(|a| a.entry_step_id) else {
        return Ok(Vec::new());
    };
    let steps = read::automation_action_steps_of(conn, action_id)?;
    let ids: BTreeSet<i64> = steps.iter().map(|s| s.id).collect();
    let mut seen = BTreeSet::new();
    let mut todo = vec![entry];
    while let Some(id) = todo.pop() {
        if !ids.contains(&id) || !seen.insert(id) {
            continue;
        }
        for edge in read::automation_edges_from(conn, AutomationPictureOwner::Action, id)? {
            if let Some(next) = edge.to_id {
                todo.push(next);
            }
        }
    }
    Ok(steps.into_iter().filter(|step| seen.contains(&step.id)).collect())
}

/// The placements a run could actually reach, walked from the entry along the edges that go on to
/// another placement. A placement nothing reaches is not checked: it cannot stop a run, and refusing to
/// launch over one would make an automation undeletable-in-practice while its author was still drawing
/// it.
fn reachable(
    conn: &Connection,
    entry_id: i64,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<BTreeSet<i64>> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![entry_id];
    while let Some(id) = todo.pop() {
        if !by_id.contains_key(&id) || !seen.insert(id) {
            continue;
        }
        let placement = by_id[&id];
        for exit in read::automation_exits_of(conn, AutomationOwner::Action, placement.action_id)? {
            let edge =
                read::automation_edge_for_exit(conn, AutomationPictureOwner::Automation, id, exit.id)?;
            if let Some(next) = edge.and_then(|e| e.to_id) {
                todo.push(next);
            }
        }
    }
    Ok(seen)
}

/// Whether a way out has something set to happen after it. An edge that goes on to a placement of some
/// other automation — or to none — decides nothing, so it counts as undecided rather than as an edge.
fn decided(
    conn: &Connection,
    placement_id: i64,
    exit_id: i64,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<bool> {
    let Some(edge) =
        read::automation_edge_for_exit(conn, AutomationPictureOwner::Automation, placement_id, exit_id)?
    else {
        return Ok(false);
    };
    Ok(match edge.to_id {
        Some(to) => by_id.contains_key(&to),
        None => true,
    })
}

/// Whether the action on a placement declares a `task_take` output on any of the ways out it can leave by
/// there — the thing that makes the placement usable as an entry, since the task it comes out holding is
/// what the run is about from there on.
fn takes_a_task(conn: &Connection, placement: &AutomationPlacement) -> Result<bool> {
    let never = never_taken(conn, placement)?;
    for exit in read::automation_exits_of(conn, AutomationOwner::Action, placement.action_id)? {
        if Some(exit.name.as_str()) == never {
            continue;
        }
        if outs_of(conn, &exit)?.iter().any(|p| p.kind == AutomationPortKind::TaskTake) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// **Whether a run started here holds a task before any step works on one** — for an entry that takes
/// none itself (`AMB-D-970`). It has to be a built-in that works before a task
/// ([`crate::ops::automation_builtin::works_before_a_task`]), and every line out of it has to end the
/// run or reach a placement that takes a task, passing only through more of those built-ins on the way.
/// A line nothing is drawn after is not asked here: that is [`Unmet::OpenExit`], said on its own.
fn takes_one_first(
    conn: &Connection,
    entry: &AutomationPlacement,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<bool> {
    let before_a_task = |placement: &AutomationPlacement| -> Result<bool> {
        Ok(action_builtin(conn, placement.action_id)?
            .is_some_and(|key| crate::ops::automation_builtin::works_before_a_task(&key)))
    };
    if !before_a_task(entry)? {
        return Ok(false);
    }
    let mut walked = BTreeSet::new();
    let mut todo = vec![entry.id];
    while let Some(id) = todo.pop() {
        if !walked.insert(id) {
            continue;
        }
        let Some(placement) = by_id.get(&id) else { continue };
        let never = never_taken(conn, placement)?;
        for exit in read::automation_exits_of(conn, AutomationOwner::Action, placement.action_id)? {
            if Some(exit.name.as_str()) == never {
                continue;
            }
            let Some(edge) =
                read::automation_edge_for_exit(conn, AutomationPictureOwner::Automation, id, exit.id)?
            else {
                continue;
            };
            let Some(to) = edge.to_id.and_then(|to| by_id.get(&to)) else { continue };
            if takes_a_task(conn, to)? {
                continue;
            }
            if !before_a_task(to)? {
                return Ok(false);
            }
            todo.push(to.id);
        }
    }
    Ok(true)
}

/// What one way out hands on. An output belongs to the way out that produced it, so this is the only
/// owner it is ever asked of.
fn outs_of(conn: &Connection, exit: &AutomationExit) -> Result<Vec<crate::model::AutomationPort>> {
    Ok(read::automation_ports_of(
        conn,
        AutomationPortOwner::Exit,
        exit.id,
        AutomationPortDirection::Out,
    )?)
}

/// Whether anything actually reaches one input — the action's input port `port_id`, on this placement.
/// A wire counts only where **both** halves hold: its far end is declared — that placement's way out
/// really hands on the port it keys — and that placement is reachable from the entry. A wire from a
/// placement no run reaches would never carry anything, so it feeds no input.
///
/// **An input the entry reads at launch is fed there** (`entry_id`): the built-in that files a task,
/// placed as the entry, is handed its title, notes and classification by the person launching the run
/// ([`crate::ops::automation_builtin_make::read_at_launch`]), and the launch refuses one that hands no
/// title.
fn fed(
    conn: &Connection,
    placement: &AutomationPlacement,
    port_id: i64,
    entry_id: i64,
    live: &BTreeSet<i64>,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
) -> Result<bool> {
    if placement.id == entry_id {
        let builtin = action_builtin(conn, placement.action_id)?;
        let port = read::automation_ports_of(
            conn,
            AutomationPortOwner::Action,
            placement.action_id,
            AutomationPortDirection::In,
        )?
        .into_iter()
        .find(|p| p.id == port_id);
        if port.is_some_and(|p| {
            crate::ops::automation_builtin_make::read_at_launch(builtin.as_deref(), &p.name)
        }) {
            return Ok(true);
        }
    }
    for wire in read::automation_wires_to_port(
        conn,
        AutomationPictureOwner::Automation,
        placement.id,
        port_id,
    )? {
        if !live.contains(&wire.from_id) {
            continue;
        }
        let Some(from) = by_id.get(&wire.from_id) else { continue };
        let exit = declared_exit(conn, wire.from_exit_id, AutomationOwner::Action, from.action_id)?;
        let Some(exit) = exit else { continue };
        if outs_of(conn, &exit)?.iter().any(|p| p.id == wire.from_port_id) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// One placement's settings, **declaration and answer together**. Public because the build screen draws
/// the same pair and must not put them back together a second way. The action declares and carries no
/// answer; the placement answers on a row of its own under the same name
/// ([`crate::ops::automation::cfg_set`]), so the two have to be put back together here.
/// **The way out a placement never leaves by**, as it is set there
/// ([`crate::ops::automation_builtin::never_leaves_by`]) — `None` for an action somebody wrote.
fn never_taken(conn: &Connection, placement: &AutomationPlacement) -> Result<Option<&'static str>> {
    let Some(key) = action_builtin(conn, placement.action_id)? else {
        return Ok(None);
    };
    let settings = settings_of(conn, placement)?;
    Ok(crate::ops::automation_builtin::never_leaves_by(&key, |setting| {
        settings.iter().find(|cfg| cfg.name == setting).and_then(|cfg| cfg.value.as_deref())
    }))
}

pub fn settings_of(conn: &Connection, placement: &AutomationPlacement) -> Result<Vec<AutomationCfg>> {
    let declared = read::automation_cfgs_of(conn, AutomationCfgOwner::Action, placement.action_id)?;
    let mut out = Vec::with_capacity(declared.len());
    for mut cfg in declared {
        cfg.value = read::automation_cfg_by_name(
            conn,
            AutomationCfgOwner::Placement,
            placement.id,
            &cfg.name,
        )?
        .and_then(|answer| answer.value);
        out.push(cfg);
    }
    Ok(out)
}

/// **Launch an automation**: check it, copy what is placed on it into a run, and start it.
///
/// Three things refuse, in this order.
///
/// - **Archived.** Archiving is what keeps an automation nobody launches any more out of the lists, and
///   launching one straight past that would make the word mean nothing.
/// - **The check** ([`check`]), as one `not_ready` refusal carrying a part per reason.
/// - **The workspace being closed**, last on purpose. What the check found is wrong with the automation
///   and stays wrong after a window is opened, so saying "open a window" first would send somebody to do
///   that and then tell them the automation was never going to run.
///
/// The run is born `running`: nothing caps how many may be under way at once, so a launch never waits
/// (`AMB-D-947`). `started_at` is the moment of the launch itself.
pub fn launch(tx: &WriteTx<'_>, automation_id: i64, by: &Launcher<'_>) -> Result<AutomationRun> {
    launch_handing(tx, automation_id, by, &HandedAtLaunch::default())
}

/// [`launch`], with what a person handed over along with it ([`HandedAtLaunch`]). The task to file is
/// kept on the run and the files hang off it, both written before any step is opened.
///
/// **A file needs somebody who handed it over.** An attachment says who put it there, and a launch
/// whose caller says nothing about itself has no one to name, so it is refused rather than guessed at.
pub fn launch_handing(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    handed: &HandedAtLaunch,
) -> Result<AutomationRun> {
    let who = match (handed.files.is_empty(), by.by) {
        (true, _) => None,
        (false, Some(who)) => Some(who),
        (false, None) => {
            return Err(Error::invalid(
                "a file handed over at launch needs the launcher to say who it is — pass who is launching",
            ))
        }
    };
    let run = launch_asking(tx, automation_id, by, handed, |_| true)?;
    if let Some(who) = who {
        for file in &handed.files {
            crate::ops::attachment::add_blob(
                tx,
                AttachmentTarget::AutomationRun,
                run.id,
                &file.blob_hash,
                &file.filename,
                file.mime.as_deref(),
                file.size_bytes,
                who,
            )?;
        }
    }
    Ok(run)
}

/// [`launch`], with the check's lines that leave a taken task open let through — for a test of how a
/// run walks, opens, reports or stops, where the picture's shape is not the subject, and for a test of
/// the safety net a run carries for a line the check missed (`AMB-D-967`).
#[cfg(test)]
pub(crate) fn launch_leaving_the_task_open(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
) -> Result<AutomationRun> {
    launch_asking(tx, automation_id, by, &HandedAtLaunch::default(), |unmet| {
        !matches!(unmet, Unmet::LeavesTaskOpen { .. })
    })
}

/// [`launch`], refusing only over what `counts` says counts, and keeping what was `handed` on the run.
/// The files are the caller's to attach once the run exists.
fn launch_asking(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    handed: &HandedAtLaunch,
    counts: impl Fn(&Unmet) -> bool,
) -> Result<AutomationRun> {
    let automation: Automation = read::automation(tx.conn(), automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    if automation.archived {
        return Err(Error::Invalid(
            Msg::new(format!(
                "automation '{}' is archived — bring it back before launching it",
                automation.name
            ))
            .coded(ErrorCode::InvalidAutomationArchived)
            .with("automation", &automation.name),
        ));
    }
    let unmet: Vec<Unmet> =
        check(tx.conn(), automation_id, by.startable, by.models)?.into_iter().filter(|u| counts(u)).collect();
    if !unmet.is_empty() {
        return Err(not_ready(&automation.name, &unmet));
    }
    // Only where somebody answered. A caller that cannot see the window says nothing rather than
    // `false`, and the run is made — a launch from a terminal is not a claim about what is on screen.
    if by.workspace_open == Some(false) {
        return Err(Error::Invalid(
            Msg::new(
                "the workspace is closed — a run draws its steps in its panes, so open it and launch again",
            )
            .coded(ErrorCode::InvalidAutomationWorkspaceClosed),
        ));
    }
    let handed_task = entry_reads(tx.conn(), &automation, handed)?
        .map(|task| serde_json::to_string(&task).map_err(Error::from))
        .transpose()?;
    let now = Timestamp::now();
    let run = AutomationRun {
        id: read::next_id(tx.conn(), "automation_run")?,
        automation_id,
        project_id: automation.project_id,
        status: AutomationRunStatus::Running,
        pause_requested: false,
        stopped_reason: None,
        started_by_kind: by.by,
        started_at: Some(now),
        ended_at: None,
        acknowledged_at: None,
        handed_task,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run(&run))?;
    for placement in read::automation_placements_of(tx.conn(), automation_id)? {
        let opens_first =
            read::automation_action(tx.conn(), placement.action_id)?.and_then(|a| a.entry_step_id);
        for step in steps_opened_by(tx.conn(), placement.action_id)? {
            let entry = automation.entry_placement_id == Some(placement.id) && opens_first == Some(step.id);
            if let Some(def) = snapshot(tx, run.id, &placement, &step, entry, now)? {
                emit_create(tx, record::automation_run_def(&def))?;
            }
        }
    }
    Ok(run)
}

/// **Whether the entry reads what was handed over at launch**, and the task to file where it files one.
///
/// Asked after the check, so the entry is there to be asked of (`AMB-D-981`):
///
/// - **The built-in that files a task** reads a title — required, since a task cannot be filed without
///   one — the notes, a classification and files. The classification is checked here the way the
///   built-in checks it when it files the task
///   ([`crate::ops::automation_builtin_make::handed_at_launch`]), so a launch that would only fall over
///   at its first step is refused before a run is made.
/// - **Any other entry** reads nothing: a built-in takes a task or fetches from what it was set with,
///   and an agent's step, which a new picture can no longer start at (`AMB-D-977`), is handed nothing
///   at launch either.
/// - **A text on its own** is read by none of them: words for the task go in its notes.
///
/// Something handed that the entry does not read is refused: nothing would ever read it, and the
/// person handing it would believe it went somewhere.
fn entry_reads(
    conn: &Connection,
    automation: &Automation,
    handed: &HandedAtLaunch,
) -> Result<Option<HandedTask>> {
    let Some(entry) = entry_of(conn, automation)? else {
        return Ok(None);
    };
    let step = action_name(conn, entry.action_id)?;
    let builtin = action_builtin(conn, entry.action_id)?;
    let files_a_task = builtin.as_deref() == Some(crate::ops::automation_builtin_make::KEY);
    let refused = |what: &str, reads: &str| {
        Err(Error::invalid(format!(
            "the entry '{step}' {reads}, and {what} was handed over at launch — it would never be read"
        )))
    };
    match files_a_task {
        true if handed.text().is_some() => refused(
            "a text",
            "files a task and reads its title, notes, classification and files — put the words in its notes",
        ),
        true => {
            let Some(title) = handed.title() else {
                return Err(Error::invalid(format!(
                    "the entry '{step}' files a task, and no title for it was handed over at launch"
                )));
            };
            let classification = crate::ops::automation_builtin_make::handed_at_launch(
                conn,
                &entry,
                automation.project_id,
                &handed.classification,
            )?;
            Ok(Some(HandedTask {
                title: title.to_string(),
                notes: handed.notes().map(str::to_string),
                classification,
            }))
        }
        false if handed.text().is_some() || handed.for_a_task() => {
            refused("something", "reads nothing handed over at launch")
        }
        false => Ok(None),
    }
}

/// The placement a run of `automation` opens first, where one is named.
fn entry_of(conn: &Connection, automation: &Automation) -> Result<Option<AutomationPlacement>> {
    match automation.entry_placement_id {
        Some(id) => Ok(read::automation_placement(conn, id)?),
        None => Ok(None),
    }
}

/// **What the entry asks the person launching a run for** (`AMB-D-970`) — the same split
/// [`entry_reads`] refuses by, answered before the press so a launch dialog asks for what the entry
/// reads and nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchAsks {
    /// The built-in that files a task: its title and notes, a value on each of these axes, and files to
    /// attach to it.
    Task { axes: Vec<crate::ops::automation_builtin_make::LaunchAxis> },
    /// Any other entry, or no entry yet: nothing is handed over.
    Nothing,
}

/// **What a launch of this automation asks for** ([`LaunchAsks`]).
pub fn launch_asks(conn: &Connection, automation_id: i64) -> Result<LaunchAsks> {
    let automation: Automation =
        read::automation(conn, automation_id)?.ok_or_else(|| not_found("automation", automation_id))?;
    let Some(entry) = entry_of(conn, &automation)? else {
        return Ok(LaunchAsks::Nothing);
    };
    Ok(match action_builtin(conn, entry.action_id)?.as_deref() {
        Some(crate::ops::automation_builtin_make::KEY) => LaunchAsks::Task {
            axes: crate::ops::automation_builtin_make::launch_axes(conn, &entry, automation.project_id)?,
        },
        _ => LaunchAsks::Nothing,
    })
}

/// Build the body of the `not_ready` refusal: one refusal over a list of reasons whose length is only
/// known here. Each reason rides as a part rather than being folded into the sentence, because joining
/// them is punctuation and punctuation belongs to the language doing the reading — the same shape a
/// reservation's refusal takes ([`crate::ops::task`]).
fn not_ready(name: &str, unmet: &[Unmet]) -> Error {
    let sentence = format!(
        "cannot launch '{name}': {}",
        unmet.iter().map(Unmet::say).collect::<Vec<_>>().join("; ")
    );
    let msg = unmet.iter().fold(
        Msg::new(sentence).coded(ErrorCode::NotReadyAutomation).with("automation", name),
        |msg, one| msg.part(one.msg()),
    );
    Error::NotReady(msg)
}

/// **One step of one placement, as it stands at this moment** — the copy a run reads from then on.
///
/// A placement is opened into **one row per step its action could open** ([`steps_opened_by`]), so the
/// run holds a single column of steps and never has to ask, while it moves, whether it is inside an
/// action. Each row says which placement it was opened from, since one action placed twice gives two
/// rows for every step inside it and only `placement_id` tells them apart.
///
/// The ways out and the inputs are **the step's own**: they are what the agent is told it may leave by
/// and what it is handed, and the action's declarations are reached through the lines drawn across its
/// edge ([`onward`], [`wired_into`]). Each input carries the outputs wired into it, resolved here, so a
/// run hands a value along the wires it launched with (`AMB-D-961`). The settings are the placement's —
/// the action's declarations with this spot's answers written in — because a step reads them by name and
/// the answer is given once, where the action is placed. The agent and the model are the placement's too
/// — chosen there, step by step (`AMB-D-960`). The prompt, the agent, the model and the three flags are
/// resolved here rather than kept as a pointer, so the step is not read halfway through the run as
/// whatever it had since been edited into.
///
/// **A step nobody is chosen for is not copied**, and `None` says so. The launch check refuses a run
/// that could open one ([`Unmet::AgentUnchosen`]), so the only such step left here stands on a
/// placement no run reaches — copied, it would need an agent it does not have. A built-in is copied
/// with nobody named and no prompt: Amenbo carries it out itself (`AMB-D-964`).
///
/// No cycle can be met here: an action places no action (`AMB-D-949`), so opening a placement goes one
/// level down and stops. A loop drawn inside an action is a way back between its steps, walked at run
/// time and held by `max_times`, not something this expands.
fn snapshot(
    tx: &WriteTx<'_>,
    run_id: i64,
    placement: &AutomationPlacement,
    step: &AutomationStep,
    entry: bool,
    now: Timestamp,
) -> Result<Option<AutomationRunDef>> {
    let conn = tx.conn();
    // A built-in is carried out by Amenbo, so nobody was chosen for it and its copy names nobody.
    let chosen = match &step.builtin {
        Some(_) => AutomationPlacementStep::default(),
        None => match read::automation_placement_step_for(conn, placement.id, step.id)? {
            Some(chosen) => chosen,
            None => return Ok(None),
        },
    };
    let mut exits = Vec::new();
    for exit in read::automation_exits_of(conn, AutomationOwner::Step, step.id)? {
        let outs = outs_of(conn, &exit)?
            .into_iter()
            .map(|p| RunDefPort { id: p.id, name: p.name, kind: p.kind, required: p.required })
            .collect();
        let (then, returns_to) = line_after(conn, placement, step.id, exit.id)?;
        exits.push(RunDefExit { id: exit.id, name: exit.name.clone(), outs, then, returns_to });
    }
    let mut ins: Vec<RunDefIn> = Vec::new();
    let declared =
        read::automation_ports_of(conn, AutomationPortOwner::Step, step.id, AutomationPortDirection::In)?;
    for p in declared {
        let from = wired_into(conn, placement, step.id, p.id)?;
        let port = RunDefPort { id: p.id, name: p.name, kind: p.kind, required: p.required };
        ins.push(RunDefIn { port, from });
    }
    let cfg: Vec<RunDefCfg> = settings_of(conn, placement)?
        .into_iter()
        .map(|c| RunDefCfg {
            name: c.name,
            kind: c.kind,
            required: c.required,
            options: c.options,
            value: c.value,
        })
        .collect();
    Ok(Some(AutomationRunDef {
        id: read::next_id(conn, "automation_run_def")?,
        run_id,
        placement_id: Some(placement.id),
        step_id: Some(step.id),
        name: step.name.clone(),
        prompt: step.builtin.is_none().then(|| step.prompt.clone()),
        builtin: step.builtin.clone(),
        agent: chosen.agent,
        model: chosen.model,
        interactive: step.interactive,
        work_dir_ref: step.work_dir_ref.clone(),
        report_to_task: step.report_to_task,
        show_history: step.show_history,
        show_notes: step.show_notes,
        show_decisions: step.show_decisions,
        show_comments: step.show_comments,
        exits: serde_json::to_string(&exits).map_err(Error::from)?,
        ins: serde_json::to_string(&ins).map_err(Error::from)?,
        cfg: serde_json::to_string(&cfg).map_err(Error::from)?,
        entry,
        created_at: now,
        updated_at: now,
    }))
}

/// **What follows one way out of one step of one placement**, read off the two pictures at launch —
/// the line the copy keeps ([`RunDefExit::then`]), and the action's way out it returns to where it
/// leaves the action ([`RunDefExit::returns_to`]).
///
/// The step's own action is asked first: the line drawn inside it from this way out either goes on to
/// another of its steps, ends or halts the run, or returns to one of the action's ways out
/// ([`AutomationEnds::Exit`]). Only in that last case is the automation's picture asked, from this
/// placement, on the action's way out that line keys — so a run crosses the action's edge exactly
/// where its author drew it, and the line kept is the one it walks.
fn line_after(
    conn: &Connection,
    placement: &AutomationPlacement,
    step_id: i64,
    exit_id: i64,
) -> Result<(Option<RunDefLine>, Option<i64>)> {
    let Some(inner) = read::automation_edge_for_exit(conn, AutomationPictureOwner::Action, step_id, exit_id)?
    else {
        return Ok((None, None));
    };
    if inner.ends != AutomationEnds::Exit {
        let step = match inner.ends {
            AutomationEnds::Go => inner.to_id,
            _ => None,
        };
        return Ok((Some(kept(conn, &inner, Some(placement.id).filter(|_| step.is_some()), step)?), None));
    }
    let Some(action_exit) = inner.exit_to_id else { return Ok((None, None)) };
    let outer =
        read::automation_edge_for_exit(conn, AutomationPictureOwner::Automation, placement.id, action_exit)?;
    let line = match outer {
        // An automation's picture has no edge of its own to return to, and the write side refuses one.
        None => None,
        Some(outer) if outer.ends == AutomationEnds::Exit => None,
        Some(outer) => {
            let (to, step) = match (outer.ends, outer.to_id) {
                (AutomationEnds::Go, Some(to)) => {
                    let opens = match read::automation_placement(conn, to)? {
                        Some(p) => read::automation_action(conn, p.action_id)?.and_then(|a| a.entry_step_id),
                        None => None,
                    };
                    (Some(to), opens)
                }
                _ => (None, None),
            };
            Some(kept(conn, &outer, to, step)?)
        }
    };
    Ok((line, Some(action_exit)))
}

/// One live line as the copy keeps it, going on to `placement_id` / `step_id` where it goes on at all.
///
/// **The limit is kept on a line that goes back, and on no other** ([`automation::lines_back_on`]). A
/// line going down carries the standing limit it was drawn with, and nobody is shown it — so counting
/// it would stop a loop drawn to go round twenty times at the ten its way down happened to carry.
fn kept(
    conn: &Connection,
    edge: &AutomationEdge,
    placement_id: Option<i64>,
    step_id: Option<i64>,
) -> Result<RunDefLine> {
    let goes_back = match edge.max_times {
        Some(_) => automation::lines_back_on(conn, edge.owner_kind, edge.owner_id)?.contains(&edge.id),
        None => false,
    };
    Ok(RunDefLine {
        edge_id: edge.id,
        picture: edge.owner_kind,
        from_id: edge.from_id,
        exit_id: edge.exit_id,
        ends: edge.ends,
        placement_id,
        step_id,
        max_times: edge.max_times.filter(|_| goes_back),
    })
}

/// **Every step output the wires join to one input of one step**, followed across the action's edge —
/// resolved once, at launch, into the copy's [`RunDefIn::from`].
///
/// A wire inside the action from another of its steps is a source as it stands. A wire from the
/// action itself ([`ACTION_BOUNDARY`]) hands on one of the action's inputs, so it is followed out to the
/// automation's picture: to the wires feeding that input on this placement, and from each of them back
/// into the action placed at the far end, to the wires that fill the output it keys. Those are drawn
/// into the boundary from a step's way out, and they count only where that way out returns to the very
/// way out of the action the automation's wire leaves by ([`returns_to`]) — the wire into the boundary
/// does not name one, and the line from the step's way out is what says which.
fn wired_into(
    conn: &Connection,
    placement: &AutomationPlacement,
    step_id: i64,
    port_id: i64,
) -> Result<Vec<RunDefSource>> {
    let mut out = Vec::new();
    for wire in read::automation_wires_of(conn, AutomationPictureOwner::Action, placement.action_id)? {
        if wire.to_id != step_id || wire.to_port_id != port_id {
            continue;
        }
        if wire.from_id != ACTION_BOUNDARY {
            out.push(RunDefSource {
                placement_id: placement.id,
                step_id: wire.from_id,
                exit_id: wire.from_exit_id,
                port_id: wire.from_port_id,
            });
            continue;
        }
        for outer in read::automation_wires_to_port(
            conn,
            AutomationPictureOwner::Automation,
            placement.id,
            wire.from_port_id,
        )? {
            let Some(far) = read::automation_placement(conn, outer.from_id)? else { continue };
            for inner in
                read::automation_wires_of(conn, AutomationPictureOwner::Action, far.action_id)?
            {
                if inner.to_id != ACTION_BOUNDARY || inner.to_port_id != outer.from_port_id {
                    continue;
                }
                let Some(inner_exit) = inner.from_exit_id else { continue };
                let leaves_by = returns_to(conn, inner.from_id, inner_exit)?;
                if leaves_by.is_some() && leaves_by == outer.from_exit_id {
                    out.push(RunDefSource {
                        placement_id: far.id,
                        step_id: inner.from_id,
                        exit_id: inner.from_exit_id,
                        port_id: inner.from_port_id,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// The copy a run took of one step of one placement.
fn copy_of(
    conn: &Connection,
    run_id: i64,
    placement_id: i64,
    step_id: i64,
) -> Result<Option<AutomationRunDef>> {
    Ok(read::automation_run_defs_of(conn, run_id)?
        .into_iter()
        .find(|def| def.placement_id == Some(placement_id) && def.step_id == Some(step_id)))
}

/// **The spot a run starts at** — the copy marked at launch as the step the automation's entry placement
/// opened first ([`AutomationRunDef::entry`]), or `None` where the run carries no such copy.
pub fn entry_def(conn: &Connection, run_id: i64) -> Result<Option<AutomationRunDef>> {
    if read::automation_run(conn, run_id)?.is_none() {
        return Err(not_found("run", run_id));
    }
    Ok(read::automation_run_defs_of(conn, run_id)?.into_iter().find(|def| def.entry))
}

/// **What follows one way out of one step**, as the step's copy says ([`RunDefExit::then`]) — resolved
/// at launch, so a run under way reads no picture but its own copies (`AMB-D-961`).
pub(crate) enum Onward {
    /// Open this copy next. `line` is the line that leads to it — the one inside the action for a step
    /// of the same placement, the automation's for the first step of another — which is what a limit on
    /// how often it may be taken is counted against ([`crate::ops::automation_report`]).
    Go { def: Box<AutomationRunDef>, line: RunDefLine },
    /// The way out closes the run.
    Done,
    /// The way out halts the run and calls a person — drawn so, or an error way out with no line after
    /// it.
    Halt,
    /// Nothing says what follows, or what it leads to was never copied into this run.
    Nowhere,
}

pub(crate) fn onward(conn: &Connection, def: &AutomationRunDef, exit: Option<i64>) -> Result<Onward> {
    let Some(exit) = exit else { return Ok(Onward::Nowhere) };
    let exits: Vec<RunDefExit> = serde_json::from_str(&def.exits).map_err(Error::from)?;
    let Some(taken) = exits.into_iter().find(|e| e.id == exit) else { return Ok(Onward::Nowhere) };
    let Some(line) = taken.then else {
        // **An error way out nobody drew a line from halts the run and calls a person** (`AMB-D-966`)
        // — which is why the launch check never asks for one. That holds for the step's own error way
        // out left without a line inside the action, and for one returned to an action's way out the
        // automation draws nothing after: the launch check lets that through only for the action's
        // error way out, and a run holds its pictures still, so no other can be left bare here.
        if taken.name == ERROR_EXIT || taken.returns_to.is_some() {
            return Ok(Onward::Halt);
        }
        return Ok(Onward::Nowhere);
    };
    Ok(match line.ends {
        AutomationEnds::Done => Onward::Done,
        AutomationEnds::Halt => Onward::Halt,
        AutomationEnds::Exit => Onward::Nowhere,
        AutomationEnds::Go => {
            let (Some(placement_id), Some(step_id)) = (line.placement_id, line.step_id) else {
                return Ok(Onward::Nowhere);
            };
            match copy_of(conn, def.run_id, placement_id, step_id)? {
                Some(next) => Onward::Go { def: Box::new(next), line },
                None => Onward::Nowhere,
            }
        }
    })
}

/// **Which of an action's ways out one way out of a step inside it returns to** — the action's way out
/// the line drawn from it inside the action keys, where that line is an [`AutomationEnds::Exit`].
/// `None` where it leaves the action by no way out. Read off the live action, which is what a launch
/// resolves its copies from; a run under way reads [`RunDefExit::returns_to`] instead.
pub(crate) fn returns_to(conn: &Connection, step_id: i64, exit: i64) -> Result<Option<i64>> {
    Ok(read::automation_edge_for_exit(conn, AutomationPictureOwner::Action, step_id, exit)?
        .filter(|edge| edge.ends == AutomationEnds::Exit)
        .and_then(|edge| edge.exit_to_id))
}

/// **What a run is waiting for**, in the three shapes a watcher has to tell apart.
///
/// The two that are not a step were one answer once, and a watcher that cannot tell them apart reads
/// a run it must leave alone and a run nobody will ever move again as the same thing — so the second
/// sits `running` for good, holding a task nobody is working.
#[derive(Debug, Clone)]
pub enum Waiting {
    /// This step is waiting to be opened. Boxed because the other two carry nothing, and a copy of a
    /// step is the whole of what one is.
    Step(Box<AutomationRunDef>),
    /// Nothing for anybody to do: a step is under way, or the run is not running at all.
    Nothing,
    /// **The run cannot go on.** Nothing is under way and nothing leads anywhere — its copies say
    /// nothing follows the way out it took, or mark no step to start at. It will never move again on
    /// its own, so it is ended rather than looked at every second.
    ///
    /// **Nobody can walk a run into this on purpose**, which is why it is held by the tests below and
    /// by no scenario (`AMB-T-5307`). A launch copies every line and marks where it starts
    /// (`AMB-D-961`); what is left is a store carried in from before it did, whose copies hold neither.
    NoWayOn,
}

/// **The step this run is waiting to have opened**, or why there is none ([`Waiting`]).
///
/// A run that is `running` is either carrying a step out or standing between two of them, and only the
/// second is anybody's to act on. So this answers a step in exactly two cases — a run that has just
/// been launched and has no execution yet, where the answer is the entry ([`entry_def`]); and a run
/// whose last execution has reported, where the answer is read off the way out it took.
///
/// **It derives rather than remembers**, because the report already wrote down everything it takes:
/// the execution carries the way out, and the step's copy says what follows one
/// ([`crate::ops::automation_report::done`] resolved the same line to decide whether the run goes on
/// at all). A second copy kept for the watcher's benefit would be a second thing to keep true.
///
/// **Deriving is also what puts a run in the way of `NoWayOn`**: a copy carried in from before a
/// launch copied the lines can say nothing follows the way out a run has already taken.
pub fn next_def(conn: &Connection, run_id: i64) -> Result<Waiting> {
    let Some(run) = read::automation_run(conn, run_id)? else {
        return Err(not_found("run", run_id));
    };
    if run.status != AutomationRunStatus::Running {
        return Ok(Waiting::Nothing);
    }
    let Some(last) = read::automation_run_steps_of(conn, run_id)?.pop() else {
        // Nothing has run yet, so what is waiting to be opened is where the run starts. A run whose
        // copies mark none cannot start at all.
        return Ok(entry_def(conn, run_id)?
            .map_or(Waiting::NoWayOn, |def| Waiting::Step(Box::new(def))));
    };
    if last.status == AutomationRunStepStatus::Running {
        return Ok(Waiting::Nothing);
    }
    // Everything below is a run standing between two steps. A way out that closed or halted the run
    // would have done so in the report that took it, so a run still `running` here is standing
    // towards the step the copy says comes next — or towards nothing, where the copy says nothing.
    let Some(from) = read::automation_run_def(conn, last.run_def_id)? else {
        return Ok(Waiting::NoWayOn);
    };
    Ok(match onward(conn, &from, last.exit_id)? {
        Onward::Go { def, .. } => Waiting::Step(def),
        Onward::Done | Onward::Halt | Onward::Nowhere => Waiting::NoWayOn,
    })
}

/// **Whether this run stands before a built-in that is waiting** for something to turn up
/// (`AMB-D-969`) — `running`, with nothing under way, and the step it would open next set to wait
/// with nothing yet for it. Asked by a face that says so beside the run.
pub fn is_waiting(conn: &Connection, run_id: i64) -> Result<bool> {
    let Waiting::Step(def) = next_def(conn, run_id)? else { return Ok(false) };
    let Some(run) = read::automation_run(conn, run_id)? else { return Ok(false) };
    crate::ops::automation_builtin::waiting(conn, &run, &def)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AutomationAction, AutomationEdge, AutomationPlacement};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewStep};
    use crate::ops::test_support::{exit_id, mk_exit, mk_in, mk_out, mk_placed, mk_project, only_step, with_tx};

    fn mk_automation(tx: &WriteTx<'_>, name: &str) -> Automation {
        let project = mk_project(tx, "amenbo");
        automation::add(tx, project, NewAutomation { name: name.into(), ..Default::default() })
            .expect("add automation")
    }

    /// What every test here starts from: one automation, one action of one step that takes a task, the
    /// built-in that closes it, and the end of the run — every way out decided. It launches as it stands,
    /// so each test can take one thing back off and watch the check find it.
    fn launchable(tx: &WriteTx<'_>) -> (Automation, AutomationAction, AutomationPlacement) {
        let automation = mk_automation(tx, "1件やりきる");
        let (action, placement) = mk_placed(tx, &automation, "取る", "take one", "claude");
        takes_task_on(tx, &action, None);
        crate::ops::test_support::mk_closed_after(tx, &automation, placement.id, None);
        // Nothing is written for the error way out: it is carried from birth and halts unless
        // somebody says otherwise, which is what `an_error_way_out_nobody_answered_for_is_not_open`
        // holds this to.
        let automation =
            automation::set_entry(tx, automation.id, Some(placement.id)).expect("entry");
        (automation, action, placement)
    }

    /// Declare a `task_take` output on one way out of an action — what makes a placement of it usable
    /// as an entry, and what the step inside it takes the task by.
    fn takes_task_on(tx: &WriteTx<'_>, action: &AutomationAction, exit_name: Option<&str>) {
        mk_out(tx, action, exit_name, "タスク", AutomationPortKind::TaskTake, true);
    }

    /// Name a model for the one step an action holds, where it is placed — the agent stays the one
    /// chosen there.
    fn names_model(
        tx: &WriteTx<'_>,
        placement: &AutomationPlacement,
        action: &AutomationAction,
        model: &str,
    ) {
        let step = only_step(tx, action);
        let chosen = read::automation_placement_step_for(tx.conn(), placement.id, step.id)
            .expect("read the choice")
            .expect("an agent chosen");
        automation::placement_step_set(tx, placement.id, step.id, &chosen.agent, Some(model))
            .expect("name a model");
    }

    /// The machine every test launches on: one that can start `claude`, with a window
    /// open.
    fn here<'a>(startable: &'a [String]) -> Launcher<'a> {
        Launcher {
            startable: Some(startable),
            models: nothing_asked(),
            workspace_open: Some(true),
            by: Some(ActorKind::Ai),
        }
    }

    fn claude() -> Vec<String> {
        vec!["claude".to_string()]
    }

    #[test]
    fn a_finished_automation_passes_the_check_and_launches_running() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
            );
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            assert_eq!(run.status, AutomationRunStatus::Running);
            assert!(run.started_at.is_some(), "a launch starts on the spot");
            assert_eq!(run.project_id, automation.project_id);
        });
    }

    #[test]
    fn an_automation_with_nothing_placed_on_it_says_only_that() {
        with_tx(|tx| {
            let automation = mk_automation(tx, "空");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::NoSteps],
                "nothing else is worth saying about it",
            );
        });
    }

    #[test]
    fn an_automation_with_no_entry_says_only_that() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            automation::set_entry(tx, automation.id, None).expect("clear entry");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::NoEntry],
                "with no entry nothing is reachable, so every other check is asked of nothing",
            );
        });
    }

    /// **An action with nothing to open is refused** (`AMB-D-949`). A placement of it is drawn on the
    /// picture like any other, and a run reaching it would have no terminal to put up.
    #[test]
    fn a_placement_standing_on_an_action_with_no_step_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            automation::action_set_entry(tx, action.id, None).expect("take the entry off");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::ActionEmpty { action: "取る".into(), placement: placement.id }],
            );
        });
    }

    #[test]
    fn an_entry_that_takes_no_task_is_refused() {
        with_tx(|tx| {
            let automation = mk_automation(tx, "1件やりきる");
            let (_, placement) = mk_placed(tx, &automation, "取る", "take one", "claude");
            automation::edge_add(
                tx,
                AutomationPictureOwner::Automation,
                placement.id,
                None,
                EdgeTarget::Done,
                None,
            )
            .expect("edge");
            automation::set_entry(tx, automation.id, Some(placement.id)).expect("entry");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::EntryTakesNoTask { step: "取る".into(), builtin: None, placement: placement.id }],
            );
        });
    }

    /// A task is taken by one placement and worked by the next; what follows the work is each test's to draw.
    fn taken_then_worked(tx: &WriteTx<'_>) -> (Automation, AutomationPlacement, AutomationAction, AutomationPlacement) {
        let automation = mk_automation(tx, "閉じ忘れ");
        let (take, taking) = mk_placed(tx, &automation, "取る", "take one", "claude");
        takes_task_on(tx, &take, None);
        let (work, working) = mk_placed(tx, &automation, "直す", "fix it", "claude");
        let on = AutomationPictureOwner::Automation;
        automation::edge_add(tx, on, taking.id, None, EdgeTarget::Go(working.id), None).expect("take → work");
        let automation = automation::set_entry(tx, automation.id, Some(taking.id)).expect("entry");
        (automation, taking, work, working)
    }

    fn checked(tx: &WriteTx<'_>, automation: &Automation) -> Vec<Unmet> {
        check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check")
    }

    /// **A line that ends the run with the task still open is refused** (`AMB-D-967`), named by the way
    /// out it leaves by.
    #[test]
    fn a_line_that_ends_the_run_with_the_task_open_is_refused() {
        with_tx(|tx| {
            let (automation, _, _, working) = taken_then_worked(tx);
            automation::edge_add(tx, AutomationPictureOwner::Automation, working.id, None, EdgeTarget::Done, None)
                .expect("work → end");
            assert_eq!(
                checked(tx, &automation),
                vec![Unmet::LeavesTaskOpen {
                    step: "直す".into(),
                    exit: crate::model::DONE_EXIT.into(),
                    to: None,
                    builtin: None,
                    to_builtin: None,
                    placement: working.id,
                }],
            );
        });
    }

    /// **A line that goes on to take another task with this one open is refused**, naming where it goes.
    #[test]
    fn a_line_that_takes_another_task_with_this_one_open_is_refused() {
        with_tx(|tx| {
            let (automation, taking, _, working) = taken_then_worked(tx);
            automation::edge_add(
                tx,
                AutomationPictureOwner::Automation,
                working.id,
                None,
                EdgeTarget::Go(taking.id),
                None,
            )
            .expect("work → take again");
            assert_eq!(
                checked(tx, &automation),
                vec![Unmet::LeavesTaskOpen {
                    step: "直す".into(),
                    exit: crate::model::DONE_EXIT.into(),
                    to: Some("取る".into()),
                    builtin: None,
                    to_builtin: None,
                    placement: working.id,
                }],
            );
        });
    }

    /// **Closing the task, or calling a person, settles it** — and from the close the run may go back
    /// for the next one.
    #[test]
    fn closing_the_task_or_calling_a_person_settles_it() {
        with_tx(|tx| {
            let (automation, taking, work, working) = taken_then_worked(tx);
            let stuck = automation::exit_add(tx, AutomationOwner::Action, work.id, Some("人に聞く"))
                .expect("exit");
            let on = AutomationPictureOwner::Automation;
            automation::edge_add(tx, on, working.id, Some(&stuck.name), EdgeTarget::Halt, None)
                .expect("call a person");
            let close = crate::ops::test_support::mk_closed_after(tx, &automation, working.id, None);
            // The close goes back to take the next one rather than on to the end.
            let closed = read::automation_edge_for_exit(
                tx.conn(),
                on,
                close.id,
                exit_id(tx, AutomationOwner::Action, close.action_id, None),
            )
            .expect("read")
            .expect("the close's line");
            automation::edge_delete(tx, closed.id).expect("unhook the end");
            automation::edge_add(tx, on, close.id, None, EdgeTarget::Go(taking.id), None).expect("close → take");
            assert_eq!(checked(tx, &automation), vec![]);
        });
    }

    /// **A line back within one task is not asked about** — a review sending the work back is walked
    /// once, and what counts is that every way out of the task closes it.
    #[test]
    fn a_line_back_within_the_task_is_not_asked_about() {
        with_tx(|tx| {
            let (automation, _, _, working) = taken_then_worked(tx);
            let (review, reviewing) = mk_placed(tx, &automation, "見る", "review it", "claude");
            mk_exit(tx, &review, "戻す");
            let on = AutomationPictureOwner::Automation;
            automation::edge_add(tx, on, working.id, None, EdgeTarget::Go(reviewing.id), None).expect("work → review");
            automation::edge_add(tx, on, reviewing.id, Some("戻す"), EdgeTarget::Go(working.id), None)
                .expect("review → back to work");
            crate::ops::test_support::mk_closed_after(tx, &automation, reviewing.id, None);
            assert_eq!(checked(tx, &automation), vec![]);
        });
    }

    #[test]
    fn a_way_out_with_nothing_after_it_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let exit =
                automation::exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある"))
                    .expect("exit");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::OpenExit {
                    step: "取る".into(),
                    exit: exit.name.clone(),
                    builtin: None,
                    placement: placement.id,
                }],
                "it saves while building, and is refused at launch",
            );
        });
    }

    #[test]
    fn an_error_way_out_nobody_answered_for_is_not_open() {
        with_tx(|tx| {
            // `launchable` writes no edge on the error way out, so a check that asked for one would
            // refuse the automation every other test here launches.
            let (automation, _, _) = launchable(tx);
            let unmet =
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check");
            assert_eq!(unmet, vec![], "the error way out is carried from birth, not written");
        });
    }

    #[test]
    fn a_required_input_nothing_reaches_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            automation::port_add(
                tx,
                AutomationPortOwner::Action,
                action.id,
                AutomationPortDirection::In,
                "下書き",
                AutomationPortKind::Value,
                true,
            )
            .expect("port");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "取る".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: placement.id,
                }],
            );
        });
    }

    #[test]
    fn a_wire_from_a_placement_no_run_reaches_does_not_feed_an_input() {
        with_tx(|tx| {
            let (automation, entry_action, entry) = launchable(tx);
            // A second placement, wired into the entry's input but reached by nothing: the run would
            // walk straight past it, so what it hands on never arrives.
            let (orphan_action, orphan) = mk_placed(tx, &automation, "書く", "write it", "claude");
            let exit = read::automation_exit_by_name(
                tx.conn(),
                AutomationOwner::Action,
                orphan_action.id,
                None,
            )
            .expect("read")
            .expect("way out");
            automation::port_add(
                tx,
                AutomationPortOwner::Exit,
                exit.id,
                AutomationPortDirection::Out,
                "下書き",
                AutomationPortKind::Value,
                false,
            )
            .expect("out");
            automation::port_add(
                tx,
                AutomationPortOwner::Action,
                entry_action.id,
                AutomationPortDirection::In,
                "下書き",
                AutomationPortKind::Value,
                true,
            )
            .expect("in");
            automation::wire_add(
                tx,
                AutomationPictureOwner::Automation,
                orphan.id,
                None,
                "下書き",
                entry.id,
                "下書き",
            )
            .expect("wire");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "取る".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: entry.id,
                }],
                "and the orphan's own ways out are not checked either — no run reaches them",
            );
        });
    }

    #[test]
    fn a_required_setting_nobody_answered_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            automation::cfg_add(
                tx,
                action.id,
                "作業フォルダ",
                crate::model::AutomationCfgKind::Folder,
                true,
                None,
            )
            .expect("cfg");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnansweredCfg {
                    step: "取る".into(),
                    cfg: "作業フォルダ".into(),
                    builtin: None,
                    placement: placement.id,
                }],
                "the declaration is the action's and the answer is the placement's",
            );
            automation::cfg_set(tx, placement.id, "作業フォルダ", Some("\"~/work\"")).expect("answer");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"), vec![]);
        });
    }

    #[test]
    fn an_agent_this_machine_cannot_start_is_refused_and_an_unasked_machine_is_not() {
        with_tx(|tx| {
            let (automation, _, placement) = launchable(tx);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&[]), nothing_asked()).expect("check"),
                vec![Unmet::AgentMissing { step: "取る".into(), agent: "claude".into(), placement: placement.id }],
            );
            assert_eq!(
                check(tx.conn(), automation.id, None, nothing_asked()).expect("check"),
                vec![],
                "a machine nobody asked is not a machine with nothing on it (AMB-D-792)",
            );
        });
    }

    /// **A second step inside an action**, put in on the line its first step leaves the action by — so
    /// the one it starts at goes on to this one, and this one is what leaves the action from here.
    /// `agent` is chosen for it at `placement`.
    fn goes_on_to(
        tx: &WriteTx<'_>,
        action: &AutomationAction,
        placement: &AutomationPlacement,
        name: &str,
        agent: &str,
    ) {
        let entry = only_step(tx, action);
        let leaves_by =
            read::automation_edge_for_exit(
                tx.conn(),
                AutomationPictureOwner::Action,
                entry.id,
                exit_id(tx, AutomationOwner::Step, entry.id, None),
            )
                .expect("read the line out of the action")
                .expect("the step an action is written with leaves it by its done way out");
        let step = automation::step_insert(tx, leaves_by.id, NewStep::new(name, "続ける"), &[], &[])
            .expect("the second step");
        automation::placement_step_set(tx, placement.id, step.id, agent, None)
            .expect("choose who carries it out");
    }

    /// **A step nobody is chosen for where it is placed is refused** (`AMB-D-960`), even on a machine
    /// nobody has asked what it can start — there is no agent to ask about at all.
    #[test]
    fn a_step_nobody_is_chosen_for_where_it_is_placed_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            automation::placement_step_clear(tx, placement.id, step.id).expect("take the choice back");
            assert_eq!(
                check(tx.conn(), automation.id, None, nothing_asked()).expect("check"),
                vec![Unmet::AgentUnchosen { step: "取る".into(), placement: placement.id }],
            );
            let err = launch(tx, automation.id, &here(&claude())).expect_err("refused");
            let Error::NotReady(msg) = err else { panic!("a launch that cannot go ahead is not_ready") };
            assert_eq!(
                msg.parts().iter().map(|p| p.code()).collect::<Vec<_>>(),
                vec![Some(ErrorCode::NotReadyAutomationAgentUnchosen)],
            );
        });
    }

    /// **The run carries out a step by whoever is chosen where it is placed** — the agent and the model
    /// are copied from the placement at launch, and choosing again afterwards leaves the run's copy as
    /// it was.
    #[test]
    fn a_run_copies_the_agent_and_the_model_chosen_where_the_step_is_placed() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            automation::placement_step_set(tx, placement.id, step.id, "claude", Some("opus"))
                .expect("choose");
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            crate::ops::automation_stop::stop(tx, run.id, crate::ops::automation_stop::Ending::Canceled)
                .expect("stop");
            automation::placement_step_set(tx, placement.id, step.id, "codex", None)
                .expect("choose again once the run is over");
            let defs = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert_eq!(defs[0].agent, "claude");
            assert_eq!(defs[0].model.as_deref(), Some("opus"));
        });
    }

    #[test]
    fn a_step_further_inside_an_action_is_asked_for_its_agent_too() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            goes_on_to(tx, &action, &placement, "書く", "codex");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::AgentMissing { step: "書く".into(), agent: "codex".into(), placement: placement.id }],
                "the check walks the picture inside the action, not its entry alone",
            );
        });
    }

    #[test]
    fn a_step_inside_an_action_that_nothing_leads_to_is_left_out() {
        with_tx(|tx| {
            let (automation, action, _) = launchable(tx);
            automation::step_add(tx, action.id, NewStep::new("書く", "続ける"))
                .expect("a step with no line into it");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
                "no pane comes up on it, so the agent it names cannot hold the launch back",
            );
        });
    }

    /// The models an agent said it can be started on, as the app remembers them.
    fn offering(agent: &str, models: &[&str]) -> ModelsHere {
        ModelsHere::from([(
            agent.to_string(),
            models.iter().map(|one| (*one).to_string()).collect::<Vec<String>>(),
        )])
    }

    #[test]
    fn a_model_the_agent_does_not_offer_here_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            names_model(tx, &placement, &action, "opus-9");
            assert_eq!(
                check(
                    tx.conn(),
                    automation.id,
                    Some(&claude()),
                    &offering("claude", &["sonnet", "haiku"]),
                )
                .expect("check"),
                vec![Unmet::ModelMissing {
                    step: "取る".into(),
                    agent: "claude".into(),
                    model: "opus-9".into(),
                    placement: placement.id,
                }],
            );
            assert_eq!(
                check(
                    tx.conn(),
                    automation.id,
                    Some(&claude()),
                    &offering("claude", &["sonnet", "opus-9"]),
                )
                .expect("check"),
                vec![],
                "a model the agent does offer is no reason at all",
            );
        });
    }

    #[test]
    fn a_model_is_judged_only_against_an_agent_that_answered() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            names_model(tx, &placement, &action, "opus-9");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
                "an agent nobody has asked says nothing about its models (AMB-D-865)",
            );
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), &offering("codex-cli", &["gpt"]))
                    .expect("check"),
                vec![],
                "another agent's list is not this one's",
            );
        });
    }

    #[test]
    fn a_step_naming_no_model_is_not_judged_on_one() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            assert_eq!(
                check(
                    tx.conn(),
                    automation.id,
                    Some(&claude()),
                    &offering("claude", &["sonnet"]),
                )
                .expect("check"),
                vec![],
                "no model named is the provider's own settings, which this cannot judge",
            );
        });
    }

    #[test]
    fn an_archived_automation_and_a_closed_workspace_each_refuse_the_launch() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let startable = claude();
            let closed = Launcher { workspace_open: Some(false), ..here(&startable) };
            assert!(launch(tx, automation.id, &closed).is_err());
            automation::update(tx, automation.id, None, None, Some(true)).expect("archive");
            assert!(launch(tx, automation.id, &here(&claude())).is_err());
        });
    }

    #[test]
    fn the_refusal_carries_one_part_per_reason() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            automation::exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある"))
                .expect("exit");
            let err = launch(tx, automation.id, &here(&[])).expect_err("refused");
            let Error::NotReady(msg) = err else { panic!("a launch that cannot go ahead is not_ready") };
            assert_eq!(msg.parts().len(), 2, "one open way out and one agent this machine has not");
            // Every sentence names itself, so a screen writes the whole refusal in the reader's own
            // language rather than the outer line in theirs and the reasons in English (`AMB-D-413`).
            assert_eq!(msg.code(), Some(ErrorCode::NotReadyAutomation));
            assert_eq!(
                msg.parts().iter().map(|p| p.code()).collect::<Vec<_>>(),
                vec![
                    Some(ErrorCode::NotReadyAutomationOpenExit),
                    Some(ErrorCode::NotReadyAutomationAgentMissing),
                ],
            );
            // And carries the values those sentences are built from, under the names the templates
            // interpolate them by — a part with a hole where the step's name goes reads as `{step}` —
            // and the placement it is about, which no template writes but a screen opens.
            let named: Vec<(&str, &str)> = msg.parts()[1].fields().iter().collect();
            let id = placement.id.to_string();
            assert_eq!(named, vec![("placement", id.as_str()), ("step", "取る"), ("agent", "claude")]);
        });
    }

    #[test]
    fn the_two_refusals_that_stand_alone_name_themselves_too() {
        with_tx(|tx| {
            let startable = claude();
            let (automation, _, _) = launchable(tx);
            automation::update(tx, automation.id, None, None, Some(true)).expect("archive");
            let err = launch(tx, automation.id, &here(&startable)).expect_err("archived");
            let Error::Invalid(msg) = err else { panic!("an archived automation is invalid") };
            assert_eq!(msg.code(), Some(ErrorCode::InvalidAutomationArchived));
            assert_eq!(
                msg.fields().iter().map(|(key, _)| key).collect::<Vec<_>>(),
                vec!["automation"],
            );

            automation::update(tx, automation.id, None, None, Some(false)).expect("bring back");
            let closed = Launcher { workspace_open: Some(false), ..here(&startable) };
            let err = launch(tx, automation.id, &closed).expect_err("closed");
            let Error::Invalid(msg) = err else { panic!("a closed workspace is invalid") };
            assert_eq!(msg.code(), Some(ErrorCode::InvalidAutomationWorkspaceClosed));
        });
    }

    /// Blank text is no text, and a launch that hands nothing over starts as it always has.
    #[test]
    fn blank_text_handed_at_launch_is_no_text() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let handed = HandedAtLaunch { text: Some("  \n".into()), ..Default::default() };
            launch_handing(tx, automation.id, &here(&claude()), &handed).expect("a blank text is not refused");
            launch(tx, automation.id, &here(&claude())).expect("launch");
        });
    }

    /// **An entry refuses what it does not read** (`AMB-D-981`): only the built-in that files a task
    /// reads anything, so an agent's step and a built-in that takes a task are handed nothing — a text,
    /// a file, or the title or classification of a task to file. Refused before any run is made.
    #[test]
    fn an_entry_refuses_what_it_does_not_read() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let startable = claude();
            let file = HandedFile {
                blob_hash: "a".repeat(64),
                filename: "issue.md".to_string(),
                mime: Some("text/markdown".to_string()),
                size_bytes: 12,
            };
            for handed in [
                HandedAtLaunch { text: Some("words".into()), ..Default::default() },
                HandedAtLaunch { files: vec![file], ..Default::default() },
                HandedAtLaunch { title: Some("an issue".into()), ..Default::default() },
                HandedAtLaunch { classification: vec![("職能".into(), "実装".into())], ..Default::default() },
            ] {
                let err = launch_handing(tx, automation.id, &here(&startable), &handed).expect_err("not read");
                assert!(err.to_string().contains("reads nothing handed over at launch"), "{err}");
            }
            assert!(read::automation_run_ids(tx.conn(), automation.id).expect("runs").is_empty());

            let take = mk_automation(tx, "取るだけ");
            let placed = crate::ops::automation::placement_add(
                tx,
                take.id,
                crate::ops::automation_builtin::action(tx, "take_task").expect("the built-in's action").id,
            )
            .expect("place it");
            let take = automation::set_entry(tx, take.id, Some(placed.id)).expect("entry");
            let words = HandedAtLaunch { text: Some("words".into()), ..Default::default() };
            let err = entry_reads(tx.conn(), &take, &words).expect_err("reads nothing");
            assert!(err.to_string().contains("reads nothing handed over at launch"), "{err}");
            assert_eq!(entry_reads(tx.conn(), &take, &HandedAtLaunch::default()).expect("nothing"), None);
        });
    }

    #[test]
    fn what_is_placed_is_copied_into_the_run_and_stops_following_the_definition() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            let copy_of_it = |tx: &WriteTx<'_>| {
                read::automation_run_defs_of(tx.conn(), run.id)
                    .expect("defs")
                    .into_iter()
                    .find(|d| d.placement_id == Some(placement.id))
                    .expect("the placement's copy")
            };
            assert_eq!(read::automation_run_defs_of(tx.conn(), run.id).expect("defs").len(), 2, "and the close's");
            let copy = copy_of_it(tx);
            assert_eq!(copy.name, "取る");
            assert_eq!(copy.prompt.as_deref(), Some("take one"));
            assert_eq!(copy.step_id, Some(step.id));
            let exits: Vec<RunDefExit> = serde_json::from_str(&copy.exits).expect("exits");
            assert_eq!(exits.len(), 2, "the done way out and the error one");
            assert_eq!(exits[0].outs[0].kind, AutomationPortKind::TaskTake);

            // The definition is held while the run is going (`AMB-D-961`), so it is ended first — the copy
            // is what the run's record reads from then on, whatever the definition becomes.
            crate::ops::automation_stop::stop(tx, run.id, crate::ops::automation_stop::Ending::Canceled)
                .expect("stop");
            automation::action_update(tx, action.id, Some("取り直す"), None)
                .expect("edit the definition once the run is over");
            automation::step_update(tx, step.id, None, Some("take another"), None, None, None, None, None, None, None)
            .expect("rewrite the prompt once the run is over");
            let copy = copy_of_it(tx);
            assert_eq!(copy.name, "取る", "the copy is what the run reads from here on");
            assert_eq!(copy.prompt.as_deref(), Some("take one"));
        });
    }

    /// What a run is waiting to have opened, at each of the three moments there is an answer to it.
    ///
    /// **It is derived and not remembered**, which is what this holds: the report writes the way out
    /// down, and the picture says what follows one — so a watcher asking later reads the same answer
    /// the report acted on, without a second copy being kept for it (`AMB-D-945`).
    #[test]
    fn what_a_run_is_waiting_to_have_opened_is_read_off_what_it_has_already_done() {
        with_tx(|tx| {
            let (automation, _, placement) = launchable(tx);
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");

            // Nothing has run yet, so what is waiting is where the run starts.
            let Waiting::Step(first) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the entry is what a fresh run waits for")
            };
            assert_eq!(first.placement_id, Some(placement.id));

            // A step under way is nobody's to open a second time.
            let opening = match crate::ops::test_support::open(tx, run.id, first.id, None)
                .expect("open")
            {
                crate::ops::automation_step::Opened::Ready(ready) => *ready,
                crate::ops::automation_step::Opened::Stopped { missing, .. } => {
                    panic!("stopped for {missing:?}")
                }
                crate::ops::automation_step::Opened::NoAgent { agent, .. } => {
                    panic!("cannot start {agent}")
                }
                crate::ops::automation_step::Opened::Carried { .. } | crate::ops::automation_step::Opened::Waiting { .. } => panic!("not a built-in"),
                crate::ops::automation_step::Opened::LeftTaskOpen { .. } => panic!("left a task open"),
            };
            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Nothing));

            // The entry hands a task on through that way out, so the task is taken before it can
            // report — the refusal that guards a step saying it is done with nothing to show.
            let task = crate::ops::test_support::mk_task_in(tx, "一件", Some(automation.project_id));
            crate::ops::automation_report::take(tx, opening.run_step.id, task).expect("take");

            // Once it has reported, the answer is read off the way out it took — here the done one,
            // which goes on to the built-in that closes the task.
            crate::ops::automation_report::done(tx, opening.run_step.id, None, "did it")
                .expect("report");
            let Waiting::Step(close) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the close is what it waits for next")
            };
            assert_eq!(close.builtin.as_deref(), Some("close_task"));

            // Carried out on the spot, it closes the task and the run with it: nothing is waiting, and
            // the run is no longer running.
            crate::ops::test_support::open(tx, run.id, close.id, None).expect("close");
            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Nothing));
            assert_eq!(
                read::automation_run(tx.conn(), run.id).expect("read").expect("the run").status,
                AutomationRunStatus::Completed,
            );
        });
    }

    /// **A run with nowhere to go says so**, rather than reading as a run somebody is about to move.
    ///
    /// The two are one answer to look at — nothing is open either way — and telling them apart is the
    /// whole of why there are three. A run reads where it starts off its copies (`AMB-D-961`), and a
    /// store carried in from before a launch marked it can hold a run whose copies mark none; left as
    /// "nothing to do" it holds its task for the rest of the session.
    #[test]
    fn a_run_whose_copies_mark_no_start_says_it_cannot_go_on() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            let Waiting::Step(entry) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the entry is what a fresh run waits for")
            };
            assert!(entry.entry, "the copy it starts at is marked at launch");

            let mut unmarked = (*entry).clone();
            unmarked.entry = false;
            crate::ops::emit_update(tx, record::automation_run_def(&entry), record::automation_run_def(&unmarked))
                .expect("unmark it");

            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::NoWayOn));
            assert_eq!(
                read::automation_run(tx.conn(), run.id).expect("read").expect("the run").status,
                AutomationRunStatus::Running,
                "reading it says nothing about it — ending it is the watch's",
            );
        });
    }

    /// A picture with somewhere to stand towards: the entry takes a task and leaves through its
    /// done way out into a second placement, which closes the run. What the tests about a run
    /// standing between two spots start from — [`launchable`]'s single placement closes the run on the
    /// spot and never stands anywhere.
    fn two_spots(tx: &WriteTx<'_>) -> (Automation, AutomationPlacement, AutomationEdge) {
        let automation = mk_automation(tx, "取って読む");
        let (first_action, first) = mk_placed(tx, &automation, "取る", "take one", "claude");
        takes_task_on(tx, &first_action, None);
        let (_, second) = mk_placed(tx, &automation, "読む", "read it back", "claude");
        automation::edge_add(
            tx,
            AutomationPictureOwner::Automation,
            second.id,
            None,
            EdgeTarget::Done,
            None,
        )
        .expect("edge");
        let onward = automation::edge_add(
            tx,
            AutomationPictureOwner::Automation,
            first.id,
            None,
            EdgeTarget::Go(second.id),
            None,
        )
        .expect("edge");
        automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
        (automation, first, onward)
    }

    /// Walk that picture as far as the gap between its two spots: the entry opened, a task taken, and
    /// a report that left through the way out leading on. The run is left `running` with nothing
    /// open, which is the one state [`Waiting`]'s three answers are told apart in.
    fn standing_between(tx: &WriteTx<'_>, automation: &Automation) -> AutomationRun {
        // Its second spot ends the run with the task open; where the run goes is what is asked here.
        let run = crate::ops::automation_run::launch_leaving_the_task_open(tx, automation.id, &here(&claude()))
            .expect("launch");
        let Waiting::Step(entry) = next_def(tx.conn(), run.id).expect("next") else {
            panic!("the entry is what a fresh run waits for")
        };
        let opening = match crate::ops::test_support::open(tx, run.id, entry.id, None).expect("open") {
            crate::ops::automation_step::Opened::Ready(ready) => *ready,
            crate::ops::automation_step::Opened::Stopped { missing, .. } => {
                panic!("stopped for {missing:?}")
            }
            crate::ops::automation_step::Opened::NoAgent { agent, .. } => {
                panic!("cannot start {agent}")
            }
            crate::ops::automation_step::Opened::Carried { .. } | crate::ops::automation_step::Opened::Waiting { .. } => panic!("not a built-in"),
            crate::ops::automation_step::Opened::LeftTaskOpen { .. } => panic!("left a task open"),
        };
        let task = crate::ops::test_support::mk_task_in(tx, "一件", Some(automation.project_id));
        crate::ops::automation_report::take(tx, opening.run_step.id, task).expect("take");
        crate::ops::automation_report::done(tx, opening.run_step.id, None, "did it")
            .expect("report");
        assert!(
            matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Step(_)),
            "with the picture as it stands, the second spot is what it waits for",
        );
        read::automation_run(tx.conn(), run.id).expect("read").expect("the run")
    }

    /// **A run goes on by its copies, whatever the picture has become** (`AMB-D-961`). What follows a
    /// way out was copied at launch, so a picture moved under the run — the spot it left taken off, the
    /// line out of it pointed at an ending, or at a placement added since — changes nothing about where
    /// it goes next. No op moves a definition a run is going on; a store written before that was held
    /// can, and the run still reads only what it launched with.
    #[test]
    fn a_run_goes_on_by_its_copies_whatever_the_picture_has_become() {
        with_tx(|tx| {
            let (automation, _, onward) = two_spots(tx);
            let run = standing_between(tx, &automation);
            let waits_for = |tx: &WriteTx<'_>| match next_def(tx.conn(), run.id).expect("next") {
                Waiting::Step(def) => def.name,
                other => panic!("the run should still stand towards its second spot: {other:?}"),
            };
            assert_eq!(waits_for(tx), "読む");

            automation::past_the_guard(|| automation::edge_update(tx, onward.id, Some(EdgeTarget::Halt), None))
                .expect("point it at an ending");
            assert_eq!(waits_for(tx), "読む", "the line it copied still leads on");

            let (_, late) = automation::past_the_guard(|| mk_placed(tx, &automation, "直す", "fix it", "claude"));
            automation::past_the_guard(|| automation::edge_update(tx, onward.id, Some(EdgeTarget::Go(late.id)), None))
                .expect("point it at a new placement");
            assert_eq!(waits_for(tx), "読む", "a placement added since is not in its copies");

            automation::past_the_guard(|| automation::edge_delete(tx, onward.id)).expect("delete the edge");
            // The entry comes off last (`AMB-D-977`), so the spot taken off is the one added since.
            automation::past_the_guard(|| automation::placement_delete(tx, late.id))
                .expect("take the placement off");
            assert_eq!(waits_for(tx), "読む", "nor is a line or a spot taken off");
        });
    }

    /// **A copy that says nothing follows its way out leaves the run nowhere to go**, and it says so. A
    /// launch always copies the line; a copy carried in from before one did, of a run that had ended by
    /// then, is the shape that holds none.
    #[test]
    fn a_run_whose_copy_says_nothing_follows_says_it_cannot_go_on() {
        with_tx(|tx| {
            let (automation, first, _) = two_spots(tx);
            let run = standing_between(tx, &automation);
            let copy = read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.placement_id == Some(first.id))
                .expect("the first spot's copy");
            let mut exits: Vec<RunDefExit> = serde_json::from_str(&copy.exits).expect("exits");
            for exit in exits.iter_mut() {
                exit.then = None;
            }
            let mut bare = copy.clone();
            bare.exits = serde_json::to_string(&exits).expect("json");
            crate::ops::emit_update(tx, record::automation_run_def(&copy), record::automation_run_def(&bare))
                .expect("strip the lines");

            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::NoWayOn));
        });
    }

    /// **A second launch does not wait for the first** (`AMB-D-947`). Nothing caps how many runs may be
    /// under way, so both are `running` from the moment they are made and both carry a `started_at`.
    #[test]
    fn a_second_launch_starts_beside_the_first_rather_than_behind_it() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let startable = claude();
            let first = launch(tx, automation.id, &here(&startable)).expect("launch");
            let second = launch(tx, automation.id, &here(&startable)).expect("launch");

            for run in [&first, &second] {
                assert_eq!(run.status, AutomationRunStatus::Running);
                assert!(run.started_at.is_some(), "a launch starts on the spot");
            }
            assert_eq!(
                read::automation_run_ids_running(tx.conn()).expect("running").len(),
                2,
                "both are going at once",
            );
        });
    }

    /// The action a placement of `launchable` stands on, given a second step its one step goes on to.
    fn a_second_step(
        tx: &WriteTx<'_>,
        action: &AutomationAction,
        placement: &AutomationPlacement,
    ) -> crate::model::AutomationStep {
        let first = only_step(tx, action);
        let second = automation::step_add(tx, action.id, NewStep::new("見直す", "review"))
            .expect("second step");
        automation::placement_step_set(tx, placement.id, second.id, "claude", None)
            .expect("choose who carries it out");
        let on = AutomationPictureOwner::Action;
        let done_exit = exit_id(tx, AutomationOwner::Step, first.id, None);
        let leaves = read::automation_edge_for_exit(tx.conn(), on, first.id, done_exit)
            .expect("read")
            .expect("the line out of the first step");
        automation::edge_update(tx, leaves.id, Some(EdgeTarget::Go(second.id)), None).expect("on");
        second
    }

    #[test]
    fn a_way_out_inside_an_action_with_nothing_after_it_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            a_second_step(tx, &action, &placement);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::OpenExit {
                    step: "見直す".into(),
                    exit: crate::model::DONE_EXIT.into(),
                    builtin: None,
                    placement: placement.id,
                }],
                "the second step's done way out leads nowhere inside the action",
            );
        });
    }

    /// **A line returning to a way out of the action stays on it when the way out is renamed, and goes
    /// with it when it is deleted** (`AMB-D-961`). The line keys the row, so a rename changes nothing
    /// the check reads; a delete takes the line, which leaves the step's way out leading nowhere.
    #[test]
    fn a_way_out_inside_follows_the_action_s_way_out_through_a_rename_and_goes_with_it() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let second = a_second_step(tx, &action, &placement);
            let mine = automation::exit_add(tx, AutomationOwner::Action, action.id, Some("差し戻し"))
                .expect("the action's way out");
            automation::edge_add(
                tx,
                AutomationPictureOwner::Automation,
                placement.id,
                Some("差し戻し"),
                EdgeTarget::Done,
                None,
            )
            .expect("the automation closes on it");
            automation::edge_add(
                tx,
                AutomationPictureOwner::Action,
                second.id,
                None,
                EdgeTarget::Exit(Some("差し戻し".into())),
                None,
            )
            .expect("return to it");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
            );

            automation::exit_rename(tx, mine.id, Some("戻す")).expect("rename");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
                "both lines stay on the way out they key",
            );

            automation::exit_delete(tx, mine.id).expect("delete");
            let unmet =
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check");
            assert!(
                unmet.contains(&Unmet::OpenExit {
                    step: "見直す".into(),
                    exit: crate::model::DONE_EXIT.into(),
                    builtin: None,
                    placement: placement.id,
                }),
                "the line returning to it went with it: {unmet:?}",
            );
        });
    }

    #[test]
    fn a_required_input_inside_an_action_nothing_reaches_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let second = a_second_step(tx, &action, &placement);
            let on = AutomationPictureOwner::Action;
            automation::edge_add(tx, on, second.id, None, EdgeTarget::Exit(None), None).expect("edge");
            automation::port_add(
                tx,
                AutomationPortOwner::Step,
                second.id,
                AutomationPortDirection::In,
                "下書き",
                AutomationPortKind::Value,
                true,
            )
            .expect("in");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "見直す".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: placement.id,
                }],
            );

            // Wired from the first step's way out, which hands on nothing of that name yet.
            let first = only_step(tx, &action);
            automation::wire_add(tx, on, first.id, None, "下書き", second.id, "下書き")
                .expect_err("a wire from an output nobody declared is refused while building");
            let exit = read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, first.id, None)
                .expect("read")
                .expect("way out");
            automation::port_add(
                tx,
                AutomationPortOwner::Exit,
                exit.id,
                AutomationPortDirection::Out,
                "下書き",
                AutomationPortKind::Value,
                false,
            )
            .expect("out");
            automation::wire_add(tx, on, first.id, None, "下書き", second.id, "下書き")
                .expect("wire");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![],
                "the first step inside hands it on",
            );
        });
    }

    #[test]
    fn an_input_inside_handed_on_by_the_action_counts_only_where_the_action_is_fed() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            // Optional on the action, so only the step inside asks for it.
            mk_in(tx, &action, "下書き", AutomationPortKind::Value, false);
            let step = only_step(tx, &action);
            let port = read::automation_ports_of(
                tx.conn(),
                AutomationPortOwner::Step,
                step.id,
                AutomationPortDirection::In,
            )
            .expect("read")
            .into_iter()
            .find(|p| p.name == "下書き")
            .expect("the step's input");
            automation::port_update(tx, port.id, None, None, Some(true)).expect("required inside");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "取る".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: placement.id,
                }],
                "the wire from the action carries nothing while nothing reaches the action",
            );
        });
    }

    /// **The same gap in an action placed twice is two reasons, one per placement** — a name cannot say
    /// which of the two boxes a reason is about, and the placement can. The one reason about the
    /// automation as a whole names none.
    #[test]
    fn the_same_gap_in_an_action_placed_twice_is_named_once_per_placement() {
        with_tx(|tx| {
            let automation = mk_automation(tx, "二度置く");
            let (action, first) = mk_placed(tx, &automation, "取る", "take one", "claude");
            takes_task_on(tx, &action, None);
            a_second_step(tx, &action, &first);
            let again = automation::placement_add(tx, automation.id, action.id).expect("place it again");
            automation::edge_add(
                tx,
                AutomationPictureOwner::Automation,
                first.id,
                None,
                EdgeTarget::Go(again.id),
                None,
            )
            .expect("on to the second placement");
            let automation = automation::set_entry(tx, automation.id, Some(first.id)).expect("entry");
            let unmet = check(tx.conn(), automation.id, Some(&claude()), nothing_asked()).expect("check");
            let about = |at: i64| Unmet::OpenExit {
                step: "見直す".into(),
                exit: crate::model::DONE_EXIT.into(),
                builtin: None,
                placement: at,
            };
            assert!(unmet.contains(&about(first.id)), "{unmet:?}");
            assert!(unmet.contains(&about(again.id)), "{unmet:?}");
            let fields = about(again.id).msg().fields().iter().map(|(k, v)| (k, v.to_string())).collect::<Vec<_>>();
            assert!(fields.contains(&("placement", again.id.to_string())), "{fields:?}");
            assert_eq!(Unmet::NoEntry.placement(), None);
        });
    }
}
