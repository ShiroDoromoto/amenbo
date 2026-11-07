//! **Launching an automation** — what refuses to start one, and the run the launch writes.
//!
//! Nothing on the definition side refuses an unfinished automation ([`crate::ops::automation`]): a step
//! with no way onward, an automation with no entry, a required setting nobody answered — each of them
//! saves, because building happens in whatever order the author likes. **This is where they are
//! refused**, which is the moment a person is actually about to be let down by them.
//!
//! **There are two entrances and one launch** — the CLI and the automations screen. Putting the check
//! and the copy in one place is what keeps them from behaving differently depending on which was
//! pressed.
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
//! **A run stops reading the definition the moment it starts.** Every step inside every placed action,
//! as the version the placement stands on holds it, is copied into `automation_run_def` at launch — one
//! column of them, each saying which placement it was opened from, which action that placement stood on
//! and whether that action takes one run at a time, with the wires joined to each input resolved into
//! it — so editing the automation afterwards cannot change what a run already under way is
//! doing, and a run stays readable months later when the automation it came from has moved on. The
//! exceptions are a run with no task in hand: one paused before its next task, which picking up copies
//! the automation down onto again ([`copy_down_again`]), and one standing at its entry, copied down again
//! where a newer version has been saved ([`take_up_newer`]) (`AMB-D-1015`).
//!
//! **A launched run goes round once per task.** Each round takes one task at the entry and walks the
//! picture to its end, and the run then goes round again at its entry ([`goes_round`]), until the entry
//! finds no task to take. A round is one stretch of the run ([`crate::model::AutomationRunTask`]), and
//! a definition saved while one is under way is taken up as the next round starts — never in the middle
//! of one.
//!
//! **Where a round goes after its task is taken is the task's role.** An automation's roles each name
//! the placement their round starts at, and a launch copies that onto the run ([`AutomationRunDef::roles`]);
//! the way out that hands on the task taken then goes on to where the task's role starts rather than
//! along a line ([`by_role`]), and only a task of one of those roles is taken ([`roles_started`]). A task
//! with no role is not taken at all. An automation with no role naming where its round starts walks its
//! picture as it is drawn.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::model::{
    ActorKind, AttachmentTarget, Automation, AutomationCfg, AutomationCfgOwner, AutomationEnds, AutomationExit,
    AutomationOwner, AutomationPictureOwner, AutomationPlacement, AutomationRole,
    AutomationPortDirection,
    AutomationPortKind, AutomationPortOwner, AutomationRun, AutomationRunDef, AutomationRunStatus,
    AutomationRunStepStatus, AutomationStep, AutomationStoppedReason, AutomationEdge,
    RunDefCfg, RunDefExit, RunDefIn, RunDefLine, RunDefPort, RunDefSource, StepScript, ACTION_BOUNDARY,
    DEFAULT_EXPECTED_MINUTES, ERROR_EXIT,
};
use crate::ops::automation::{ActionDef, Picture};
use crate::ops::automation_script;
use crate::ops::automation_builtin_split::SPLIT_BY_DIM;
use crate::ops::automation_builtin_take::TAKE_TASK;
use crate::ops::emit_create;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// **One thing the launch check found missing.** Every one is something a person can
/// go and fix in the build screen — which is why each names where it is rather than only what it is.
///
/// They are a type rather than sentences because both doors need them: the refusal writes them out
/// as English, and the build screen draws them as a list beside the step each belongs to.
///
/// **`placement` is the placement on the automation's picture the reason is about** — the one standing
/// where the gap is, or holding the action whose picture has it. A name alone cannot say which: the same
/// action placed twice is two boxes with one name, and a screen that opens the box a reason is about has
/// to know which of the two. So the same gap in an action placed twice is two reasons, one per placement.
/// The reasons about the automation as a whole ([`Unmet::ExportOnly`], [`Unmet::NoSteps`],
/// [`Unmet::NoEntry`], [`Unmet::NoFailuresTo`]) name none,
/// and nor do the ones the action's own check finds ([`check_action`]): it is asked with the action on
/// no picture.
///
/// **`builtin` is the key of the built-in a name came from** (`AMB-D-964`), and `to_builtin` the same
/// for `to`. A built-in's step, ways out, inputs and settings are named in the store's Japanese, so a
/// screen drawn in another language turns them into its own — and only a name carrying a key is
/// turned, since a person's action may use the same word for something of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unmet {
    /// The automation is a definition from before automations had roles, kept only to be written out
    /// ([`crate::model::Automation::export_only`]). It is not mended in place: it is exported, rebuilt by
    /// hand and imported as a new automation. Nothing else is worth saying about it.
    ExportOnly,
    /// Nothing is placed on the automation at all. Nothing else is worth saying about it.
    NoSteps,
    /// No placement is named as the entry, so there is nowhere for a run to start and nothing is
    /// reachable. The first placement is the entry (`AMB-D-977`), so only a store written before that
    /// holds one of these, and replacing the entry gives it one.
    NoEntry,
    /// Nobody is chosen to file a failed round to — neither a person nor one of the automation's roles
    /// ([`crate::model::Automation::failures_to`]). A round that fails would have nobody to go to.
    NoFailuresTo,
    /// An action standing on the picture has no step to open — none written yet, or none named as its
    /// entry. A run reaching that spot would have no terminal to put up. Asked of the action alone
    /// ([`check_action`]), it names no placement.
    ActionEmpty { action: String, placement: Option<i64> },
    /// An action standing on the picture that is still being written (`AMB-D-1005`) — made on the spot
    /// where it was placed, and neither finished nor given up. Its author has not said it is what they
    /// meant, so a run is not sent through it. Raised wherever it stands, reached from the entry or not:
    /// it is a decision left open, not a gap a run would fall into.
    ActionDraft { action: String, placement: i64 },
    /// The entry declares no `task_take` output, so no step of the run would ever come to hold a task
    /// and every step after it would be about nothing.
    EntryTakesNoTask { step: String, builtin: Option<String>, placement: i64 },
    /// A line a run could walk back to an entry that reads what was handed over at launch — the
    /// built-in that files a task ([`crate::ops::automation_builtin_make::reads_at_launch`]). It is
    /// handed the title only the first time a run opens it, so coming back to it the run would fail
    /// there on no input.
    ///
    /// `step` and `exit` name the way out the line leaves by, and `to` the entry it goes back to.
    BackToEntry {
        step: String,
        exit: String,
        to: String,
        builtin: Option<String>,
        to_builtin: Option<String>,
        placement: i64,
    },
    /// A way out with nothing set to happen after it. The run would reach it and stop. The error way
    /// out is not one of these — it is carried from birth and halts unless somebody says otherwise.
    ///
    /// `inside` is the step inside the placed action whose way out it is, or `None` for the action's
    /// own way out on the automation's picture. The two read alike, and are mended in different places:
    /// the one inside only on the action's own picture.
    OpenExit {
        step: String,
        exit: String,
        builtin: Option<String>,
        placement: Option<i64>,
        inside: Option<i64>,
    },
    /// A required input with nothing reaching it — no wire at all, or none whose far end is both
    /// declared and reachable from the entry before this placement is. `inside` as for
    /// [`Unmet::OpenExit`].
    UnwiredInput {
        step: String,
        port: String,
        builtin: Option<String>,
        placement: Option<i64>,
        inside: Option<i64>,
    },
    /// A required setting nobody answered while building.
    UnansweredCfg { step: String, cfg: String, builtin: Option<String>, placement: i64 },
    /// A setting answered with something its kind does not take — a task filter that is not its parts, a
    /// number below zero, a choice that is not listed ([`crate::ops::automation::answer_misfit`]). The
    /// answer is checked when it is written; one written before that, or left behind when the
    /// declaration changed, would otherwise be read at run time as something nobody wrote. `why` is the
    /// English reason.
    MisansweredCfg { step: String, cfg: String, why: String, builtin: Option<String>, placement: i64 },
    /// A setting whose answer names something this automation's project does not have — `line` being
    /// the line of it that does (`AMB-D-987`): a task or a decision by a number, a folder, an axis or a
    /// value, or a role by a name that is not exactly one of the automation's roles. The built-in would
    /// refuse it when a run reached it and halt the run there.
    CfgNotFound { step: String, cfg: String, line: String, builtin: Option<String>, placement: i64 },
    /// A setting whose answer names a value this automation's project has but has closed — `line` being
    /// the line of it that does (`AMB-D-987`). The built-in files no task under a closed value, and would
    /// halt the run there.
    CfgValueClosed { step: String, cfg: String, line: String, builtin: Option<String>, placement: i64 },
    /// The folder setting `cfg` of the built-in that files a task left empty, in a project with `count`
    /// linked folders (`AMB-D-1012`). Which one the task is worked in is not the built-in's to pick, so it
    /// would file nothing and halt the run there.
    FolderUnchosen { step: String, cfg: String, count: usize, builtin: Option<String>, placement: i64 },
    /// A step nobody carries out: no agent is chosen for it where its action is placed, the role whose
    /// round reaches it names none, and Amenbo has no default either ([`carrier`]). A pane opened on it
    /// would have no agent to start.
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
    /// A script step whose program has no file at its path (`AMB-D-1016`). The path is checked for
    /// being a full one when it is saved; whether a file stands there is asked here, since it can be
    /// moved after that, or the automation launched on another machine.
    ScriptMissing { step: String, program: String, placement: i64 },
    /// A script step whose program is a file this machine will not run — on macOS and Linux, one with
    /// no permission to execute. The step would fail to start once the run is away.
    ScriptNotExecutable { step: String, program: String, placement: i64 },
    /// A line a run could walk while it holds the task it took, and that goes on to take another one
    /// (`to` naming where) or ends the run (`to` `None`) without the task being closed or a person being
    /// called on the way (`AMB-D-967`). The task would be left in progress with no run holding it.
    ///
    /// `step` and `exit` name the way out the line leaves by. Lines inside one task — a review sending
    /// the work back, say — are not asked about: only the ones leading out of it. `inside` as for
    /// [`Unmet::OpenExit`]: the step inside the action whose line ends the run there.
    LeavesTaskOpen {
        step: String,
        exit: String,
        to: Option<String>,
        builtin: Option<String>,
        to_builtin: Option<String>,
        placement: i64,
        inside: Option<i64>,
    },
    /// A way out of a person's own action — or of a step inside it — that declares a `task_take` output
    /// (`AMB-D-964`). Only a built-in takes the task a run works, and a step has no command to hand it
    /// on, so a step leaving by a way out that requires one is refused at `step-done` and the run
    /// stands still. Declaring one is refused while building ([`crate::ops::automation::port_add`]);
    /// a store written before that can still hold one.
    ///
    /// `step` names the action, or the step inside it, whose way out `exit` is.
    HandsOnTaskTaken { step: String, exit: String, placement: i64 },
    /// The built-in that splits by an axis, placed while its axis was there, whose axis has been deleted
    /// since (`AMB-D-987`). A run reaching it would have nothing to split by and halt, so the launch is
    /// refused here instead. `step` is the action's name, which every split shares, so the placement is
    /// named in the sentence too.
    SplitAxisGone { step: String, placement: i64 },
}

impl Unmet {
    /// The English sentence, which is what the CLI prints and what any surface holding no dictionary
    /// falls back to.
    pub fn say(&self) -> String {
        match self {
            Unmet::ExportOnly => {
                "it was written before automations had roles and is kept only to be written out — export it \
                 with `automation export`, rebuild it and import it as a new automation"
                    .to_string()
            }
            Unmet::NoSteps => "no action is placed on it".to_string(),
            Unmet::NoEntry => {
                "no placement is named as the entry — choose what a run starts at with `automation entry-replace`"
                    .to_string()
            }
            Unmet::NoFailuresTo => {
                "nobody is chosen to file a failed round to — choose a person or one of its roles with \
                 `automation update --failures-to`"
                    .to_string()
            }
            Unmet::ActionEmpty { action, placement: Some(_) } => {
                format!("the action '{action}' placed on it has no step to start at")
            }
            Unmet::ActionEmpty { action, placement: None } => {
                format!("the action '{action}' has no step to start at")
            }
            Unmet::ActionDraft { action, .. } => {
                format!(
                    "the action '{action}' placed on it is still being written — finish creating it, or give \
                     it up"
                )
            }
            Unmet::EntryTakesNoTask { step, .. } => {
                format!(
                    "the entry '{step}' takes no task — draw every line out of it on to a step that takes one, \
                     or change what a run starts at with `automation entry-replace`"
                )
            }
            Unmet::BackToEntry { step, exit, to, .. } => {
                format!(
                    "{} of '{step}' goes back to the entry '{to}', which is handed what to file only when a \
                     run starts — end the run there, or go on to a step that takes the next task",
                    named(exit)
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
            Unmet::MisansweredCfg { step, cfg, why, .. } => {
                format!("the setting '{cfg}' of '{step}' is answered with something it does not take: {why}")
            }
            Unmet::CfgNotFound { step, cfg, line, .. } => {
                format!("the setting '{cfg}' of '{step}' names '{line}', which this project does not have")
            }
            Unmet::CfgValueClosed { step, cfg, line, .. } => {
                format!("the setting '{cfg}' of '{step}' names '{line}', a value that is closed")
            }
            Unmet::FolderUnchosen { step, cfg, count, .. } => {
                format!(
                    "the setting '{cfg}' of '{step}' is left empty, and this project has {count} linked folders \
                     — write which one the task is worked in"
                )
            }
            Unmet::AgentUnchosen { step, .. } => {
                format!(
                    "nobody is chosen to carry out '{step}' — not where its action is placed, not by its role, \
                     and Amenbo has no default agent"
                )
            }
            Unmet::AgentMissing { step, agent, .. } => {
                format!("'{step}' asks for '{agent}', which this machine cannot start")
            }
            Unmet::ModelMissing { step, agent, model, .. } => {
                format!("'{step}' asks for the model '{model}', which '{agent}' here does not offer")
            }
            Unmet::ScriptMissing { step, program, .. } => {
                format!("'{step}' runs '{program}', and there is no file there")
            }
            Unmet::ScriptNotExecutable { step, program, .. } => {
                format!(
                    "'{step}' runs '{program}', which is not allowed to be executed — give it permission \
                     (chmod +x)"
                )
            }
            Unmet::LeavesTaskOpen { step, exit, to, .. } => {
                let from = format!("{} of '{step}'", named(exit));
                match to {
                    Some(to) => format!(
                        "{from} goes on to '{to}', which takes another task, with the task taken before \
                         still open — close it, hand it back to a person, put it off, or call a person on \
                         the way"
                    ),
                    None => format!(
                        "{from} ends the run with the task it took still open — close it, hand it back \
                         to a person, put it off, or call a person on the way"
                    ),
                }
            }
            Unmet::HandsOnTaskTaken { step, exit, .. } => {
                format!(
                    "{} of '{step}' hands on the task the run works (task_take) — only a built-in takes it. \
                     Put the built-in 'take_task' or 'make_task' before it, and take that output off",
                    named(exit)
                )
            }
            Unmet::SplitAxisGone { step, placement } => {
                format!(
                    "'{step}' on placement {placement} splits by an axis that has been deleted — take the \
                     placement off and place the split again on an axis that is there"
                )
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
            Unmet::ExportOnly => ErrorCode::NotReadyAutomationExportOnly,
            Unmet::NoSteps => ErrorCode::NotReadyAutomationNoSteps,
            Unmet::NoEntry => ErrorCode::NotReadyAutomationNoEntry,
            Unmet::NoFailuresTo => ErrorCode::NotReadyAutomationNoFailuresTo,
            Unmet::ActionEmpty { placement: Some(_), .. } => ErrorCode::NotReadyAutomationActionEmpty,
            Unmet::ActionEmpty { placement: None, .. } => ErrorCode::NotReadyActionSaveEmpty,
            Unmet::ActionDraft { .. } => ErrorCode::NotReadyAutomationActionDraft,
            Unmet::EntryTakesNoTask { .. } => ErrorCode::NotReadyAutomationEntryTakesNoTask,
            Unmet::BackToEntry { .. } => ErrorCode::NotReadyAutomationBackToEntry,
            Unmet::OpenExit { .. } => ErrorCode::NotReadyAutomationOpenExit,
            Unmet::UnwiredInput { .. } => ErrorCode::NotReadyAutomationUnwiredInput,
            Unmet::UnansweredCfg { .. } => ErrorCode::NotReadyAutomationUnansweredCfg,
            Unmet::MisansweredCfg { .. } => ErrorCode::NotReadyAutomationMisansweredCfg,
            Unmet::CfgNotFound { .. } => ErrorCode::NotReadyAutomationCfgNotFound,
            Unmet::CfgValueClosed { .. } => ErrorCode::NotReadyAutomationCfgValueClosed,
            Unmet::FolderUnchosen { .. } => ErrorCode::NotReadyAutomationFolderUnchosen,
            Unmet::AgentUnchosen { .. } => ErrorCode::NotReadyAutomationAgentUnchosen,
            Unmet::AgentMissing { .. } => ErrorCode::NotReadyAutomationAgentMissing,
            Unmet::ModelMissing { .. } => ErrorCode::NotReadyAutomationModelMissing,
            Unmet::ScriptMissing { .. } => ErrorCode::NotReadyAutomationScriptMissing,
            Unmet::ScriptNotExecutable { .. } => ErrorCode::NotReadyAutomationScriptNotExecutable,
            Unmet::LeavesTaskOpen { to: Some(_), .. } => ErrorCode::NotReadyAutomationTaskLeftOpen,
            Unmet::LeavesTaskOpen { to: None, .. } => ErrorCode::NotReadyAutomationTaskLeftOpenAtEnd,
            Unmet::HandsOnTaskTaken { .. } => ErrorCode::NotReadyAutomationHandsOnTaskTaken,
            Unmet::SplitAxisGone { .. } => ErrorCode::NotReadyAutomationSplitAxisGone,
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
        let msg = match self.inside() {
            Some(id) => msg.with("inside_step", id),
            None => msg,
        };
        match self {
            Unmet::ExportOnly | Unmet::NoSteps | Unmet::NoEntry | Unmet::NoFailuresTo => msg,
            Unmet::ActionEmpty { action, .. } | Unmet::ActionDraft { action, .. } => msg.with("action", action),
            Unmet::EntryTakesNoTask { step, .. } => msg.with("step", step),
            Unmet::BackToEntry { step, exit, to, to_builtin, .. } => {
                let msg = msg.with("step", step).with("exit", exit).with("to", to);
                match to_builtin {
                    Some(key) => msg.with("to_builtin", key),
                    None => msg,
                }
            }
            Unmet::OpenExit { step, exit, .. } => msg.with("step", step).with("exit", exit),
            Unmet::UnwiredInput { step, port, .. } => msg.with("step", step).with("port", port),
            Unmet::UnansweredCfg { step, cfg, .. } | Unmet::MisansweredCfg { step, cfg, .. } => {
                msg.with("step", step).with("cfg", cfg)
            }
            Unmet::CfgNotFound { step, cfg, line, .. } | Unmet::CfgValueClosed { step, cfg, line, .. } => {
                msg.with("step", step).with("cfg", cfg).with("line", line)
            }
            Unmet::FolderUnchosen { step, cfg, count, .. } => {
                msg.with("step", step).with("cfg", cfg).with("count", count)
            }
            Unmet::AgentUnchosen { step, .. } => msg.with("step", step),
            Unmet::AgentMissing { step, agent, .. } => msg.with("step", step).with("agent", agent),
            Unmet::ModelMissing { step, model, .. } => msg.with("step", step).with("model", model),
            Unmet::ScriptMissing { step, program, .. } | Unmet::ScriptNotExecutable { step, program, .. } => {
                msg.with("step", step).with("program", program)
            }
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
            Unmet::HandsOnTaskTaken { step, exit, .. } => msg.with("step", step).with("exit", exit),
            Unmet::SplitAxisGone { step, .. } => msg.with("step", step),
        }
    }

    /// The placement this reason is about, or `None` for one about the automation as a whole or found by
    /// the action's own check.
    pub fn placement(&self) -> Option<i64> {
        match self {
            Unmet::ExportOnly | Unmet::NoSteps | Unmet::NoEntry | Unmet::NoFailuresTo => None,
            Unmet::ActionEmpty { placement, .. }
            | Unmet::OpenExit { placement, .. }
            | Unmet::UnwiredInput { placement, .. } => *placement,
            Unmet::ActionDraft { placement, .. }
            | Unmet::EntryTakesNoTask { placement, .. }
            | Unmet::BackToEntry { placement, .. }
            | Unmet::UnansweredCfg { placement, .. }
            | Unmet::MisansweredCfg { placement, .. }
            | Unmet::CfgNotFound { placement, .. }
            | Unmet::CfgValueClosed { placement, .. }
            | Unmet::FolderUnchosen { placement, .. }
            | Unmet::AgentUnchosen { placement, .. }
            | Unmet::AgentMissing { placement, .. }
            | Unmet::ModelMissing { placement, .. }
            | Unmet::ScriptMissing { placement, .. }
            | Unmet::ScriptNotExecutable { placement, .. }
            | Unmet::LeavesTaskOpen { placement, .. }
            | Unmet::HandsOnTaskTaken { placement, .. }
            | Unmet::SplitAxisGone { placement, .. } => Some(*placement),
        }
    }

    /// The step inside the placed action this reason is about, or `None` for one the automation's own
    /// picture answers — what lets a screen open the action on that step rather than the box.
    pub fn inside(&self) -> Option<i64> {
        match self {
            Unmet::OpenExit { inside, .. }
            | Unmet::UnwiredInput { inside, .. }
            | Unmet::LeavesTaskOpen { inside, .. } => *inside,
            _ => None,
        }
    }

    /// The key of the built-in `step` came from, or `None` for a person's own action or step.
    fn builtin(&self) -> Option<&str> {
        match self {
            Unmet::EntryTakesNoTask { builtin, .. }
            | Unmet::BackToEntry { builtin, .. }
            | Unmet::OpenExit { builtin, .. }
            | Unmet::UnwiredInput { builtin, .. }
            | Unmet::UnansweredCfg { builtin, .. }
            | Unmet::MisansweredCfg { builtin, .. }
            | Unmet::CfgNotFound { builtin, .. }
            | Unmet::CfgValueClosed { builtin, .. }
            | Unmet::FolderUnchosen { builtin, .. }
            | Unmet::LeavesTaskOpen { builtin, .. } => builtin.as_deref(),
            Unmet::SplitAxisGone { .. } => Some(SPLIT_BY_DIM.key),
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
    /// **Amenbo's own default agent** — the last of the three a step's agent is chosen from, after the
    /// step at its placement and the role whose round reaches it ([`carrier`]). It is the answer a pane
    /// would open with for the automation's project ([`crate::wake::step_agent`]), and names no model.
    ///
    /// The store fills it from its configuration on every launch it carries
    /// ([`crate::store::Store::automation_launch`]), so a caller going through the store writes `None`.
    pub default_agent: Option<&'a str>,
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
/// A trigger hands its task over the same way, as the title and notes it says, with the role the task is
/// worked under and its own name for the line the task's timeline gets ([`fire`]).
///
/// Nothing handed is the default, and a launch that hands nothing starts as it always has.
#[derive(Clone, Debug, Default)]
pub struct HandedAtLaunch {
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
    /// The role it is worked under, one of the automation's own — handed by a trigger
    /// ([`crate::model::AutomationTrigger::role_id`]). `None` for none.
    pub role_id: Option<i64>,
    /// The name of the trigger that hands it, which the task's timeline names it by once it is filed
    /// ([`crate::activity_log::event::task_filed_by_trigger`]). `None` where a person launched the run.
    pub trigger: Option<String>,
}

impl HandedAtLaunch {
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
    /// The role the task is worked under ([`HandedAtLaunch::role_id`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_id: Option<i64>,
    /// The name of the trigger that handed it ([`HandedAtLaunch::trigger`]), or `None` for a person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
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
/// The list is walked in display order, so a person reading it walks their own picture. An automation
/// kept only to be written out ([`Unmet::ExportOnly`]) has that one reason and no other: its picture is
/// not mended in place, so nothing found in it is worth saying. Otherwise the list opens with the one
/// reason that is about the automation's own settings rather than its picture: nobody chosen to
/// file a failed round to ([`Unmet::NoFailuresTo`]). Two of the rest answer alone, beside that one: an
/// automation with nothing placed on it has nothing else to say about it, and one with no entry has
/// nothing reachable to say it about — every other check is asked of the placements a run would
/// actually walk, and with no entry that is none of them.
///
/// **What is asked of a placement is asked of the action standing on it**, and then of every step the
/// action could open ([`steps_opened_by`]): a way out inside with nothing after it and a required input
/// inside that nothing reaches stop a run just as surely as the same gaps on the automation's picture
/// ([`inside`]). The agent and the model are asked of each step as it is placed here — the step's own
/// choice, else the default of the role whose round reaches it, else Amenbo's ([`carrier`]). A script
/// step is asked instead whether its program is a file this machine can run (`AMB-D-1016`).
///
/// `startable` is [`Launcher::startable`], and `None` leaves the agent check unmade. `models` is
/// [`Launcher::models`], and an agent it says nothing about leaves that step's model check unmade.
/// `default_agent` is [`Launcher::default_agent`], Amenbo's own default.
///
/// Asked of the draft — the automation's picture as it is written now. A launch asks it of the
/// picture it is about to copy down ([`check_picture`]).
pub fn check(
    conn: &Connection,
    automation_id: i64,
    startable: Option<&[String]>,
    models: &ModelsHere,
    default_agent: Option<&str>,
) -> Result<Vec<Unmet>> {
    let automation = read::automation(conn, automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    check_picture(conn, &automation, &Picture::read(conn, &automation)?, startable, models, default_agent)
}

/// **Where the agent carrying a step out was chosen** — the first of three that names one: the step at
/// its placement, the role whose round reaches that placement, and Amenbo as a whole ([`carrier`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChosenAt {
    /// Chosen for this step where its action is placed ([`crate::model::AutomationPlacementStep`]).
    Step,
    /// The default of this role ([`AutomationRole`]).
    Role { role_id: i64 },
    /// Amenbo's own default ([`Launcher::default_agent`]).
    Amenbo,
}

/// **Who carries one step out at one placement**, and where that was chosen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Carrier {
    pub agent: String,
    /// `None` leaves the agent's own default model.
    pub model: Option<String>,
    pub at: ChosenAt,
}

/// **The role each placement falls back to** — for every role, in the order they are defined in, the
/// placements its round reaches ([`round_of`]). A placement more than one round reaches falls back to the
/// first of those roles. A role that names no agent is passed by: it has nothing for a step to fall back
/// to.
fn roles_by_placement<'p>(
    pic: &'p Picture,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
    defs: &Defs,
) -> BTreeMap<i64, &'p AutomationRole> {
    first_round_reaching(pic, by_id, defs, pic.roles().iter().filter(|role| role.agent.is_some()))
}

/// **The first of `roles` whose round reaches each placement** — `roles` walked in the order given, a
/// placement more than one round reaches kept with the first ([`round_of`]). Every role's start still
/// stops a round, whether or not it is among `roles`.
fn first_round_reaching<'p>(
    pic: &'p Picture,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
    defs: &Defs,
    roles: impl Iterator<Item = &'p AutomationRole>,
) -> BTreeMap<i64, &'p AutomationRole> {
    let starts: BTreeSet<i64> =
        pic.roles().iter().filter_map(|role| role.entry_placement_id).chain(pic.entry_placement_id).collect();
    let mut found = BTreeMap::new();
    for role in roles {
        let Some(entry) = role.entry_placement_id else { continue };
        for id in round_of(entry, pic, by_id, defs, &starts) {
            found.entry(id).or_insert(role);
        }
    }
    found
}

/// **The role whose round reaches each placement**, as placement id → role id, for the definition
/// `reads` names. Every role is walked in the order they are defined in, a role naming no agent
/// included: this is where a round goes, which a picture draws whether or not the role has an agent for
/// its steps to fall back to ([`roles_by_placement`] is that narrower one). A placement more than one
/// round reaches is the first role's; a placement no round reaches is left out.
pub fn rounds_by_placement(conn: &Connection, automation_id: i64, reads: ReadsFrom) -> Result<BTreeMap<i64, i64>> {
    let automation = read::automation(conn, automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    let (pic, _) = definition(conn, &automation, reads)?;
    let by_id: BTreeMap<i64, &AutomationPlacement> = pic.placements.iter().map(|p| (p.id, p)).collect();
    let defs = defs_of(conn, &pic.placements)?;
    Ok(first_round_reaching(&pic, &by_id, &defs, pic.roles().iter())
        .into_iter()
        .map(|(placement_id, role)| (placement_id, role.id))
        .collect())
}

/// **One task a role's round files for another role**, as the build screen draws it beside the role:
/// the placement of the built-in that files it ([`crate::ops::automation_builtin_make`]), inside the
/// round of `from_role_id`, answered with `to_role_id` as the role the task is worked under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RolePass {
    pub from_role_id: i64,
    pub to_role_id: i64,
    pub placement_id: i64,
    /// Whether the task it files is set to depend on another
    /// ([`crate::ops::automation_builtin_make::answered_depends`]).
    pub depends: bool,
    /// **The task this placement last passed to `to_role_id`**, by any run of the automation
    /// ([`crate::model::AutomationRunPass::placement_id`]) — what the build screen names the task it
    /// passes on by, the title being written only when a run files it. `None` where no run has passed one
    /// from here yet, or the ones it did have since been deleted.
    pub last_task_id: Option<i64>,
}

/// **What each role's round passes to another role**, for the definition `reads` names, in the order
/// the placements are — every placement of the built-in that files a task whose round is one role's
/// ([`rounds_by_placement`]) and whose role setting names exactly one other role. A placement filing
/// under its own round's role, or under a role its name does not settle, passes nothing. Neither does
/// one that takes the task it files ([`crate::ops::automation_builtin_make::answered_takes`]): the run
/// records no pass there, and the build screen says only what a run's pane can draw a line for.
pub fn passes_by_role(conn: &Connection, automation_id: i64, reads: ReadsFrom) -> Result<Vec<RolePass>> {
    let automation = read::automation(conn, automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    let (pic, _) = definition(conn, &automation, reads)?;
    let by_id: BTreeMap<i64, &AutomationPlacement> = pic.placements.iter().map(|p| (p.id, p)).collect();
    let defs = defs_of(conn, &pic.placements)?;
    let rounds = first_round_reaching(&pic, &by_id, &defs, pic.roles().iter());
    let mut passes = Vec::new();
    for placement in &pic.placements {
        if action_builtin(conn, placement.action_id)?.as_deref() != Some(crate::ops::automation_builtin_make::KEY) {
            continue;
        }
        let Some(from) = rounds.get(&placement.id) else { continue };
        let settings = answers(&pic, placement, &defs[&placement.id]);
        let Some((_, Some(to))) = crate::ops::automation_builtin_make::answered_role(&settings, pic.roles()) else {
            continue;
        };
        if to != from.id && !crate::ops::automation_builtin_make::answered_takes(&settings) {
            let depends = crate::ops::automation_builtin_make::answered_depends(&settings);
            passes.push(RolePass {
                from_role_id: from.id,
                to_role_id: to,
                placement_id: placement.id,
                depends,
                last_task_id: None,
            });
        }
    }
    let spots: Vec<i64> = passes.iter().map(|p| p.placement_id).collect();
    let filed = read::automation_run_passes_at(conn, &spots)?;
    for pass in &mut passes {
        pass.last_task_id = filed
            .iter()
            .filter(|f| f.placement_id == Some(pass.placement_id) && f.to_role_id == Some(pass.to_role_id))
            .find_map(|f| f.task_id);
    }
    Ok(passes)
}

/// **One task a round of a run passed to another role** ([`crate::model::AutomationRunPass`]), with the
/// role it was passed from read off the round — what the run's pane draws a line between two roles by.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RunPass {
    /// The round that passed it.
    pub run_task_id: i64,
    /// The round's role. `None` where that role has since been taken out.
    pub from_role_id: Option<i64>,
    /// The role it was filed under. `None` where that role has since been taken out.
    pub to_role_id: Option<i64>,
    /// The task. `None` where it has since been deleted.
    pub task_id: Option<i64>,
}

/// **Every task one run's rounds passed to another role**, in the order they were filed.
pub fn passes_of_run(conn: &Connection, run_id: i64) -> Result<Vec<RunPass>> {
    let rounds: BTreeMap<i64, Option<i64>> =
        read::automation_run_tasks_of(conn, run_id)?.into_iter().map(|r| (r.id, r.role_id)).collect();
    Ok(read::automation_run_passes_of(conn, run_id)?
        .into_iter()
        .map(|pass| RunPass {
            run_task_id: pass.run_task_id,
            from_role_id: rounds.get(&pass.run_task_id).copied().flatten(),
            to_role_id: pass.to_role_id,
            task_id: pass.task_id,
        })
        .collect())
}

/// **The placements one round reaches** — walked from where it starts, as [`reachable`] walks, but not
/// on into a placement in `starts` other than its own: that is where the automation's entry or another
/// role's round begins, and a line going there has left this round.
fn round_of(
    entry: i64,
    pic: &Picture,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
    defs: &Defs,
    starts: &BTreeSet<i64>,
) -> BTreeSet<i64> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![entry];
    while let Some(id) = todo.pop() {
        if (id != entry && starts.contains(&id)) || !by_id.contains_key(&id) || !seen.insert(id) {
            continue;
        }
        for exit in defs[&id].action_exits() {
            if let Some(next) = pic.edge_for_exit(id, exit.id).and_then(|e| e.to_id) {
                todo.push(next);
            }
        }
    }
    seen
}

/// **Who carries one step out at one placement**: the agent chosen for the step there, else the default
/// of the role the placement falls back to ([`roles_by_placement`]), else Amenbo's own default. `None`
/// where none of the three names one.
///
/// **The model comes from where the agent did.** It is the agent's model and is named only beside it
/// (`AMB-D-865`), so a step's agent is never put with a role's model. Amenbo's default names no model,
/// which is the agent's own default.
fn carrier(
    pic: &Picture,
    roles: &BTreeMap<i64, &AutomationRole>,
    placement_id: i64,
    step_id: i64,
    default_agent: Option<&str>,
) -> Option<Carrier> {
    if let Some(chosen) = pic.chosen(placement_id, step_id) {
        return Some(Carrier { agent: chosen.agent.clone(), model: chosen.model.clone(), at: ChosenAt::Step });
    }
    if let Some(role) = roles.get(&placement_id) {
        if let Some(agent) = &role.agent {
            let at = ChosenAt::Role { role_id: role.id };
            return Some(Carrier { agent: agent.clone(), model: role.model.clone(), at });
        }
    }
    default_agent.map(|agent| Carrier { agent: agent.to_string(), model: None, at: ChosenAt::Amenbo })
}

/// **Who carries each step out**, keyed by (placement, step), for the definition `reads` names — every
/// step an agent carries out, wherever one of the three names somebody ([`carrier`]). A built-in and a
/// script are left out: Amenbo carries the one out and the other is its own program.
pub fn carriers(
    conn: &Connection,
    automation_id: i64,
    reads: ReadsFrom,
    default_agent: Option<&str>,
) -> Result<BTreeMap<(i64, i64), Carrier>> {
    let automation = read::automation(conn, automation_id)?
        .ok_or_else(|| not_found("automation", automation_id))?;
    let (pic, _) = definition(conn, &automation, reads)?;
    let by_id: BTreeMap<i64, &AutomationPlacement> = pic.placements.iter().map(|p| (p.id, p)).collect();
    let defs = defs_of(conn, &pic.placements)?;
    let roles = roles_by_placement(&pic, &by_id, &defs);
    let mut found = BTreeMap::new();
    for placement in &pic.placements {
        for step in &defs[&placement.id].steps {
            if step.builtin.is_some() || step.script.is_some() {
                continue;
            }
            if let Some(carried) = carrier(&pic, &roles, placement.id, step.id, default_agent) {
                found.insert((placement.id, step.id), carried);
            }
        }
    }
    Ok(found)
}

/// [`check`], asked of `pic` — the draft or a saved version of `automation` ([`ReadsFrom`]).
fn check_picture(
    conn: &Connection,
    automation: &Automation,
    pic: &Picture,
    startable: Option<&[String]>,
    models: &ModelsHere,
    default_agent: Option<&str>,
) -> Result<Vec<Unmet>> {
    if automation.export_only {
        return Ok(vec![Unmet::ExportOnly]);
    }
    // Read off the automation, not off `pic`: where a failed round goes is not part of a saved version,
    // so a launch of one asks the automation as it stands now.
    let mut unmet = Vec::new();
    if automation.failures_to.is_none() {
        unmet.push(Unmet::NoFailuresTo);
    }
    let placements = &pic.placements;
    if placements.is_empty() {
        unmet.push(Unmet::NoSteps);
        return Ok(unmet);
    }
    let Some(entry_id) = pic.entry_placement_id else {
        unmet.push(Unmet::NoEntry);
        return Ok(unmet);
    };
    let by_id: BTreeMap<i64, &AutomationPlacement> = placements.iter().map(|p| (p.id, p)).collect();
    let defs = defs_of(conn, placements)?;
    let live = reachable(entry_id, pic, &by_id, &defs);
    let routed = !role_entries(pic, &by_id).is_empty();
    let roles = roles_by_placement(pic, &by_id, &defs);

    for placement in placements {
        if let Some(action) = read::automation_action(conn, placement.action_id)?.filter(|a| a.draft) {
            unmet.push(Unmet::ActionDraft { action: action.name, placement: placement.id });
        }
    }
    if let Some(entry) = by_id.get(&entry_id) {
        if !takes_a_task(conn, pic, entry, &defs[&entry.id])? && !takes_one_first(conn, pic, entry, &by_id, &defs)? {
            unmet.push(Unmet::EntryTakesNoTask {
                step: action_name(conn, entry.action_id)?,
                builtin: action_builtin(conn, entry.action_id)?,
                placement: entry.id,
            });
        }
        unmet.extend(back_to_entry(conn, pic, entry, &live, &defs)?);
    }
    for placement in placements.iter().filter(|p| live.contains(&p.id)) {
        let def = &defs[&placement.id];
        let name = action_name(conn, placement.action_id)?;
        let builtin = action_builtin(conn, placement.action_id)?;
        // A built-in set to wait never leaves by the way out it waits instead of (`AMB-D-969`), and one
        // whose setting chooses its way out never by the other, so nothing has to follow it.
        let never_taken = never_taken(conn, pic, placement, def)?;
        let settings = answers(pic, placement, def);
        for exit in def.action_exits() {
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
            // A way out that hands on the task taken goes where the task's role starts, in an automation
            // whose roles say where that is (`by_role`) — no line drawn from it is walked.
            if routed && goes_by_role(def, exit.id) {
                continue;
            }
            if !decided(pic, placement.id, exit.id, &by_id) {
                unmet.push(Unmet::OpenExit {
                    step: name.clone(),
                    exit: exit.name.clone(),
                    builtin: builtin.clone(),
                    placement: Some(placement.id),
                    inside: None,
                });
            }
        }
        for port in def.action_inputs() {
            if port.required && !fed(conn, pic, placement, port.id, entry_id, &live, &by_id, &defs)? {
                unmet.push(Unmet::UnwiredInput {
                    step: name.clone(),
                    port: port.name,
                    builtin: builtin.clone(),
                    placement: Some(placement.id),
                    inside: None,
                });
            }
        }
        for cfg in &settings {
            match cfg.value.as_deref() {
                None if cfg.required => unmet.push(Unmet::UnansweredCfg {
                    step: name.clone(),
                    cfg: cfg.name.clone(),
                    builtin: builtin.clone(),
                    placement: placement.id,
                }),
                None => {}
                Some(value) => {
                    let why = crate::ops::automation::answer_misfit(cfg.kind, cfg.options.as_deref(), value);
                    if let Some(why) = why {
                        unmet.push(Unmet::MisansweredCfg {
                            step: name.clone(),
                            cfg: cfg.name.clone(),
                            why,
                            builtin: builtin.clone(),
                            placement: placement.id,
                        });
                    }
                }
            }
        }
        if builtin.as_deref() == Some(crate::ops::automation_builtin_make::KEY) {
            let unfound = crate::ops::automation_builtin_make::unfound(conn, &settings, automation.project_id)?;
            let role = crate::ops::automation_builtin_make::unfound_role(&settings, pic.roles());
            for (cfg, line) in unfound.into_iter().chain(role) {
                unmet.push(Unmet::CfgNotFound {
                    step: name.clone(),
                    cfg: cfg.to_string(),
                    line,
                    builtin: builtin.clone(),
                    placement: placement.id,
                });
            }
            let classify = crate::ops::automation_builtin_make::CLASSIFY;
            for line in crate::ops::automation_builtin_make::closed_values(conn, &settings, automation.project_id)? {
                unmet.push(Unmet::CfgValueClosed {
                    step: name.clone(),
                    cfg: classify.to_string(),
                    line,
                    builtin: builtin.clone(),
                    placement: placement.id,
                });
            }
            let folder = crate::ops::automation_builtin_make::FOLDER;
            if let Some(count) =
                crate::ops::automation_builtin_make::folder_unchosen(conn, &settings, automation.project_id)?
            {
                unmet.push(Unmet::FolderUnchosen {
                    step: name.clone(),
                    cfg: folder.to_string(),
                    count,
                    builtin: builtin.clone(),
                    placement: placement.id,
                });
            }
        }
        if crate::ops::automation_builtin_split::lost_its_axis(conn, placement.action_id)? {
            unmet.push(Unmet::SplitAxisGone { step: name.clone(), placement: placement.id });
        }
        let steps = steps_opened_by(def);
        if builtin.is_none() {
            push_new(&mut unmet, hands_on_the_task(placement, def, &name, &steps));
        }
        if steps.is_empty() {
            unmet.push(Unmet::ActionEmpty { action: name.clone(), placement: Some(placement.id) });
            continue;
        }
        let fed_here = |port| fed(conn, pic, placement, port, entry_id, &live, &by_id, &defs);
        push_new(&mut unmet, inside(def, Some(placement.id), &steps, fed_here)?);
        // A built-in names no agent and no model: Amenbo carries it out itself (`AMB-D-964`).
        for step in steps.iter().filter(|step| step.builtin.is_none()) {
            // Nor does a script: its program is what is started, so that is what is asked about.
            if let Some(script) = &step.script {
                push_new(&mut unmet, unrunnable(&script.program, &step.name, placement.id).into_iter().collect());
                continue;
            }
            let mut found = Vec::new();
            let Some(chosen) = carrier(pic, &roles, placement.id, step.id, default_agent) else {
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
    push_new(&mut unmet, leaves_task_open(conn, pic, &live, &by_id, &defs)?);
    Ok(unmet)
}

/// **The action each placement of one automation stands on**, by placement id — read once, as the
/// version the placement points at holds it ([`ActionDef::placed`]), and asked by every check and copy
/// below rather than the action's rows as they stand now.
type Defs = BTreeMap<i64, ActionDef>;

fn defs_of(conn: &Connection, placements: &[AutomationPlacement]) -> Result<Defs> {
    placements.iter().map(|p| Ok((p.id, ActionDef::placed(conn, p)?))).collect()
}

/// **Is what is inside this action ready to be saved?** An empty answer is yes.
///
/// Only what the action answers for on its own is asked: that it has a step to open
/// ([`Unmet::ActionEmpty`]), and of every step it could open, the two questions [`inside`] puts — a way
/// out with nothing after it, and a required input nothing inside reaches. An input wired in from the
/// action itself counts where the action declares it; whether something reaches that input is asked of
/// each placement, at the automation's launch check. So is everything chosen or answered where the action
/// is placed — the agents, the settings — and the lines out of the action.
pub fn check_action(conn: &Connection, action_id: i64) -> Result<Vec<Unmet>> {
    let action = read::automation_action(conn, action_id)?.ok_or_else(|| not_found("action", action_id))?;
    let def = ActionDef::live(conn, action_id)?;
    let steps = steps_opened_by(&def);
    if steps.is_empty() {
        return Ok(vec![Unmet::ActionEmpty { action: action.name, placement: None }]);
    }
    inside(&def, None, &steps, |_| Ok(true))
}

/// **Why the program of a script step could not be started here**, or `None` when it could
/// (`AMB-D-1016`): no file at the path, or — on macOS and Linux — a file with no execute permission.
/// Windows runs a file by what it is rather than by a permission, so there a file standing there is
/// enough.
fn unrunnable(program: &str, step: &str, placement: i64) -> Option<Unmet> {
    let (step, program) = (step.to_string(), program.to_string());
    let meta = match std::fs::metadata(&program) {
        Ok(meta) if meta.is_file() => meta,
        _ => return Some(Unmet::ScriptMissing { step, program, placement }),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return Some(Unmet::ScriptNotExecutable { step, program, placement });
        }
    }
    #[cfg(not(unix))]
    let _ = meta;
    None
}

/// **The lines that leave a taken task open** (`AMB-D-967`), walked from every way out that hands a
/// task on.
///
/// While a run holds the task it took, the walk goes on through the placements it reaches and stops at
/// the ones that settle the task: the built-ins that close it, hand it back to a person or put it off,
/// and a line that stops the run and calls a person (the error way out with nothing after it is one — it
/// halts). Reaching the run's end, or a placement that takes a task, before any of them is the line
/// refused. A placement already walked is not walked again, so a line back within the task — a review
/// sending the work back — is followed once and asked nothing.
///
/// **The picture inside an action is opened too.** A line in there can end the run on its own, and
/// that is the same task left open. What takes a task and what closes one are asked of the action as a
/// whole: taking is declared on the action's ways out, and the built-in that closes stands on a
/// placement of its own (`AMB-D-969`).
fn leaves_task_open(
    conn: &Connection,
    pic: &Picture,
    live: &BTreeSet<i64>,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
    defs: &Defs,
) -> Result<Vec<Unmet>> {
    let routed = role_entries(pic, by_id);
    let mut unmet = Vec::new();
    for &start in live {
        let Some(placement) = by_id.get(&start) else { continue };
        let def = &defs[&placement.id];
        let mut lines = Vec::new();
        for exit in def.action_exits() {
            if Some(exit.name.as_str()) == never_taken(conn, pic, placement, def)? {
                continue;
            }
            if hands_on_taken(def, exit.id) {
                lines.push((*placement, exit));
            }
        }
        let mut walked = BTreeSet::new();
        while let Some((from, exit)) = lines.pop() {
            // The way out that hands on the task taken goes on to where each role starts (`by_role`);
            // any other goes where its line is drawn to.
            let tos: Vec<&AutomationPlacement> = if !routed.is_empty() && goes_by_role(&defs[&from.id], exit.id) {
                routed.iter().filter_map(|id| by_id.get(id).copied()).collect()
            } else {
                let Some(edge) = pic.edge_for_exit(from.id, exit.id) else {
                    // Nothing drawn: the error way out halts, and any other is refused as an open way out.
                    continue;
                };
                match edge.ends {
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
                                inside: None,
                            }],
                        );
                        continue;
                    }
                    AutomationEnds::Go => match edge.to_id.and_then(|id| by_id.get(&id)) {
                        Some(to) => vec![*to],
                        None => continue,
                    },
                }
            };
            for to in tos {
                if lets_go_of_the_task(conn, to.action_id)? {
                    continue;
                }
                if takes_a_task(conn, pic, to, &defs[&to.id])? {
                    push_new(
                        &mut unmet,
                        vec![Unmet::LeavesTaskOpen {
                            step: action_name(conn, from.action_id)?,
                            exit: exit.name.clone(),
                            to: Some(action_name(conn, to.action_id)?),
                            builtin: action_builtin(conn, from.action_id)?,
                            to_builtin: action_builtin(conn, to.action_id)?,
                            placement: from.id,
                            inside: None,
                        }],
                    );
                    continue;
                }
                if !walked.insert(to.id) {
                    continue;
                }
                push_new(&mut unmet, ends_inside(to, &defs[&to.id]));
                for exit in defs[&to.id].action_exits() {
                    lines.push((to, exit));
                }
            }
        }
    }
    Ok(unmet)
}

/// **The ways out of a person's own action that hand on the task a run works** ([`Unmet::HandsOnTaskTaken`]),
/// asked of the action's own ways out and of every step inside it the action could open. A step's way
/// out mirrored on the action's under the same name is said once.
fn hands_on_the_task(
    placement: &AutomationPlacement,
    def: &ActionDef,
    action: &str,
    steps: &[AutomationStep],
) -> Vec<Unmet> {
    let mut owners = vec![(AutomationOwner::Action, def.action_id, action.to_string())];
    owners.extend(steps.iter().map(|step| (AutomationOwner::Step, step.id, step.name.clone())));
    let mut found = Vec::new();
    for (owner, owner_id, name) in owners {
        for exit in def.exits_of(owner, owner_id) {
            if hands_on_taken(def, exit.id) {
                push_new(
                    &mut found,
                    vec![Unmet::HandsOnTaskTaken { step: name.clone(), exit: exit.name, placement: placement.id }],
                );
            }
        }
    }
    found
}

/// Whether a placement of this action lets go of the task the run holds — the built-in that closes it
/// (`AMB-D-964`), the one that hands it back to a person and goes on, or the one that puts it off and
/// goes on.
fn lets_go_of_the_task(conn: &Connection, action_id: i64) -> Result<bool> {
    let lets_go = [
        crate::ops::automation_builtin_close::CLOSE_TASK.key,
        crate::ops::automation_builtin_hand_back::HAND_BACK_TASK.key,
        crate::ops::automation_builtin_defer::DEFER_TASK.key,
    ];
    Ok(action_builtin(conn, action_id)?.is_some_and(|key| lets_go.contains(&key.as_str())))
}

/// The lines inside the action standing on one placement that end the run, each named by the step and
/// the way out it leaves by.
fn ends_inside(placement: &AutomationPlacement, def: &ActionDef) -> Vec<Unmet> {
    let mut found = Vec::new();
    for step in steps_opened_by(def) {
        for exit in def.exits_of(AutomationOwner::Step, step.id) {
            if def.edge_for_exit(step.id, exit.id).is_some_and(|e| e.ends == AutomationEnds::Done) {
                found.push(Unmet::LeavesTaskOpen {
                    step: step.name.clone(),
                    exit: exit.name,
                    to: None,
                    builtin: step.builtin.clone(),
                    to_builtin: None,
                    placement: placement.id,
                    inside: Some(step.id),
                });
            }
        }
    }
    found
}

/// **The way out a line is keyed to**, where that row is one the given owner declares in `def` — `None`
/// for a line keyed to nothing, or to a row that is not there or belongs elsewhere.
fn declared_exit(
    def: &ActionDef,
    exit_id: Option<i64>,
    owner_kind: AutomationOwner,
    owner_id: i64,
) -> Option<&AutomationExit> {
    def.exit(exit_id?).filter(|exit| exit.owner_kind == owner_kind && exit.owner_id == owner_id)
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
/// ([`ACTION_BOUNDARY`]) — which counts only where the action declares that input and `fed` says
/// something reaches it: on a placement, something on the automation's picture ([`fed`]); asked of the
/// action alone ([`check_action`]), the declaration is all there is to ask. It is the same walk a run
/// makes when it hands a step its values ([`crate::ops::automation_step`]).
///
/// `placement` is the one the reasons name, `None` when the action is asked on no picture.
fn inside(
    def: &ActionDef,
    placement: Option<i64>,
    steps: &[AutomationStep],
    mut fed: impl FnMut(i64) -> Result<bool>,
) -> Result<Vec<Unmet>> {
    let opened: BTreeSet<i64> = steps.iter().map(|s| s.id).collect();
    let mut unmet = Vec::new();
    for step in steps {
        for exit in def.exits_of(AutomationOwner::Step, step.id) {
            if exit.name == ERROR_EXIT {
                continue;
            }
            let decided = match def.edge_for_exit(step.id, exit.id) {
                None => false,
                Some(edge) => match edge.ends {
                    AutomationEnds::Done | AutomationEnds::Halt => true,
                    AutomationEnds::Go => edge.to_id.is_some_and(|to| opened.contains(&to)),
                    AutomationEnds::Exit => {
                        declared_exit(def, edge.exit_to_id, AutomationOwner::Action, def.action_id).is_some()
                    }
                },
            };
            if !decided {
                unmet.push(Unmet::OpenExit {
                    step: step.name.clone(),
                    exit: exit.name.clone(),
                    builtin: step.builtin.clone(),
                    placement,
                    inside: Some(step.id),
                });
            }
        }
        for port in def.ports_of(AutomationPortOwner::Step, step.id, AutomationPortDirection::In) {
            if !port.required {
                continue;
            }
            let mut reached = false;
            let mut before = None;
            for wire in def.wires.iter().filter(|w| w.to_id == step.id && w.to_port_id == port.id) {
                reached = if wire.from_id == ACTION_BOUNDARY {
                    let declared = def.action_inputs().iter().any(|p| p.id == wire.from_port_id);
                    declared && fed(wire.from_port_id)?
                } else if opened.contains(&wire.from_id) {
                    // Only a step a run opens before this one hands anything on to its first opening:
                    // its own way out, or that of a step only reached through it, leaves the input empty
                    // then (`AMB-T-5641`), as on the automation's picture (`fed`).
                    if before.is_none() {
                        before = Some(steps_reached(def, steps, Some(step.id)));
                    }
                    if !before.as_ref().is_some_and(|b| b.contains(&wire.from_id)) {
                        continue;
                    }
                    match declared_exit(def, wire.from_exit_id, AutomationOwner::Step, wire.from_id) {
                        Some(exit) => def.outs_of(exit.id).iter().any(|p| p.id == wire.from_port_id),
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
                    placement,
                    inside: Some(step.id),
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
/// could walk, in display order, and that is what the agent and the model are asked of ([`carrier`])
/// and what a launch copies into the run ([`snapshot`]).
/// A step nothing inside leads to is left out for the reason [`reachable`] leaves a placement out: no
/// pane ever comes up on it, so refusing the launch over the agent it names would hold a run back for
/// a box still being drawn.
fn steps_opened_by(def: &ActionDef) -> Vec<AutomationStep> {
    let seen = steps_reached(def, &def.steps, None);
    def.steps.iter().filter(|step| seen.contains(&step.id)).cloned().collect()
}

/// **The steps of one action walked from its entry**, without passing through `avoiding` — with it,
/// what a run can open before it first comes to that step, the step itself not among them. The walk
/// [`reachable_without`] takes over the placements, taken over the picture inside.
fn steps_reached(def: &ActionDef, steps: &[AutomationStep], avoiding: Option<i64>) -> BTreeSet<i64> {
    let Some(entry) = def.entry_step_id else {
        return BTreeSet::new();
    };
    let ids: BTreeSet<i64> = steps.iter().map(|s| s.id).collect();
    let mut seen = BTreeSet::new();
    let mut todo = vec![entry];
    while let Some(id) = todo.pop() {
        if Some(id) == avoiding || !ids.contains(&id) || !seen.insert(id) {
            continue;
        }
        for edge in def.edges_from(id) {
            if let Some(next) = edge.to_id {
                todo.push(next);
            }
        }
    }
    seen
}

/// The placements a run could actually reach, walked from the entry along the edges that go on to
/// another placement — and, from a way out that hands on the task taken, on to where each role starts
/// ([`role_entries`]). A placement nothing reaches is not checked: it cannot stop a run, and refusing to
/// launch over one would make an automation undeletable-in-practice while its author was still drawing
/// it.
fn reachable(
    entry_id: i64,
    pic: &Picture,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
    defs: &Defs,
) -> BTreeSet<i64> {
    reachable_without(entry_id, pic, by_id, defs, None)
}

/// [`reachable`], walked without passing through `avoiding` — **what a run can reach before it first
/// comes to that placement**. The placement itself is not among them.
fn reachable_without(
    entry_id: i64,
    pic: &Picture,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
    defs: &Defs,
    avoiding: Option<i64>,
) -> BTreeSet<i64> {
    let routed = role_entries(pic, by_id);
    let mut seen = BTreeSet::new();
    let mut todo = vec![entry_id];
    while let Some(id) = todo.pop() {
        if Some(id) == avoiding || !by_id.contains_key(&id) || !seen.insert(id) {
            continue;
        }
        for exit in defs[&id].action_exits() {
            if !routed.is_empty() && goes_by_role(&defs[&id], exit.id) {
                todo.extend(routed.iter().copied());
                continue;
            }
            if let Some(next) = pic.edge_for_exit(id, exit.id).and_then(|e| e.to_id) {
                todo.push(next);
            }
        }
    }
    seen
}

/// **The placements the automation's roles start their rounds at** (`AutomationRole::entry_placement_id`),
/// in the order the roles are defined in, each once — the ones on the picture. Empty where it defines no
/// role, or none of them names where its round starts: such a picture is walked as it is drawn.
///
/// Where there are any, **a way out that hands on the task taken goes on to the one its task's role
/// starts at** ([`by_role`]), so every one of them is where such a way out leads.
fn role_entries(pic: &Picture, by_id: &BTreeMap<i64, &AutomationPlacement>) -> Vec<i64> {
    let mut entries = Vec::new();
    for id in pic.roles().iter().filter_map(|r| r.entry_placement_id) {
        if by_id.contains_key(&id) && !entries.contains(&id) {
            entries.push(id);
        }
    }
    entries
}

/// Whether this way out of a placed action hands on the task a run works — the way out that takes one.
fn hands_on_taken(def: &ActionDef, exit_id: i64) -> bool {
    def.outs_of(exit_id).iter().any(|p| p.kind == AutomationPortKind::TaskTake)
}

/// **Whether this way out goes where the task's role starts** rather than along a line, where the
/// automation's roles say where that is ([`by_role`]): the way out of the built-in that takes a task
/// which hands it on. A task the run files itself and takes walks the line drawn, even one a trigger
/// files under a role.
fn goes_by_role(def: &ActionDef, exit_id: i64) -> bool {
    def.steps.iter().any(|s| s.builtin.as_deref() == Some(TAKE_TASK.key)) && hands_on_taken(def, exit_id)
}

/// **The lines a run could walk back to an entry that reads what was handed over at launch**
/// ([`Unmet::BackToEntry`]) — one for each way out of a placement in `live` that goes to `entry`, the
/// entry's own included. A way out its setting says it never leaves by is not asked about.
fn back_to_entry(
    conn: &Connection,
    pic: &Picture,
    entry: &AutomationPlacement,
    live: &BTreeSet<i64>,
    defs: &Defs,
) -> Result<Vec<Unmet>> {
    let to_builtin = action_builtin(conn, entry.action_id)?;
    if !crate::ops::automation_builtin_make::reads_at_launch(to_builtin.as_deref()) {
        return Ok(Vec::new());
    }
    let to = action_name(conn, entry.action_id)?;
    let mut found = Vec::new();
    for placement in pic.placements.iter().filter(|p| live.contains(&p.id)) {
        let def = &defs[&placement.id];
        let never = never_taken(conn, pic, placement, def)?;
        for exit in def.action_exits() {
            if Some(exit.name.as_str()) == never {
                continue;
            }
            if pic.edge_for_exit(placement.id, exit.id).and_then(|e| e.to_id) == Some(entry.id) {
                found.push(Unmet::BackToEntry {
                    step: action_name(conn, placement.action_id)?,
                    exit: exit.name,
                    to: to.clone(),
                    builtin: action_builtin(conn, placement.action_id)?,
                    to_builtin: to_builtin.clone(),
                    placement: placement.id,
                });
            }
        }
    }
    Ok(found)
}

/// Whether a way out has something set to happen after it. An edge that goes on to a placement of some
/// other automation — or to none — decides nothing, so it counts as undecided rather than as an edge.
fn decided(pic: &Picture, placement_id: i64, exit_id: i64, by_id: &BTreeMap<i64, &AutomationPlacement>) -> bool {
    let Some(edge) = pic.edge_for_exit(placement_id, exit_id) else {
        return false;
    };
    match edge.to_id {
        Some(to) => by_id.contains_key(&to),
        None => true,
    }
}

/// Whether the action on a placement declares a `task_take` output on any of the ways out it can leave by
/// there — the thing that makes the placement usable as an entry, since the task it comes out holding is
/// what the run is about from there on.
fn takes_a_task(
    conn: &Connection,
    pic: &Picture,
    placement: &AutomationPlacement,
    def: &ActionDef,
) -> Result<bool> {
    let never = never_taken(conn, pic, placement, def)?;
    for exit in def.action_exits() {
        if Some(exit.name.as_str()) == never {
            continue;
        }
        if hands_on_taken(def, exit.id) {
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
    pic: &Picture,
    entry: &AutomationPlacement,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
    defs: &Defs,
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
        let def = &defs[&placement.id];
        let never = never_taken(conn, pic, placement, def)?;
        for exit in def.action_exits() {
            if Some(exit.name.as_str()) == never {
                continue;
            }
            let Some(edge) = pic.edge_for_exit(id, exit.id) else {
                continue;
            };
            let Some(to) = edge.to_id.and_then(|to| by_id.get(&to)) else { continue };
            if takes_a_task(conn, pic, to, &defs[&to.id])? {
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

/// Whether anything actually reaches one input — the action's input port `port_id`, on this placement.
/// A wire counts only where **both** halves hold: its far end is declared — that placement's way out
/// really hands on the port it keys — and that placement is reachable from the entry **without passing
/// through this one**. A wire from a placement no run reaches would never carry anything, and one from
/// this placement's own way out, or from a placement only reached after it, carries nothing the first
/// time a run arrives — either way it feeds no input.
///
/// **An input the entry reads at launch is fed there** (`entry_id`): the built-in that files a task,
/// placed as the entry, is handed its title, notes and classification by the person launching the run
/// ([`crate::ops::automation_builtin_make::read_at_launch`]), and the launch refuses one that hands no
/// title. That holds only the first time a run opens it, and a line back to it is refused on its own
/// ([`Unmet::BackToEntry`]).
#[allow(clippy::too_many_arguments)]
fn fed(
    conn: &Connection,
    pic: &Picture,
    placement: &AutomationPlacement,
    port_id: i64,
    entry_id: i64,
    live: &BTreeSet<i64>,
    by_id: &BTreeMap<i64, &AutomationPlacement>,
    defs: &Defs,
) -> Result<bool> {
    if placement.id == entry_id {
        let builtin = action_builtin(conn, placement.action_id)?;
        let port = defs[&placement.id].action_inputs().into_iter().find(|p| p.id == port_id);
        if port.is_some_and(|p| {
            crate::ops::automation_builtin_make::read_at_launch(builtin.as_deref(), &p.name)
        }) {
            return Ok(true);
        }
    }
    let mut before = None;
    for wire in pic.wires_to_port(placement.id, port_id) {
        if !live.contains(&wire.from_id) {
            continue;
        }
        // Only a placement a run can come to before this one hands anything on to its first visit
        // (`AMB-T-5641`): the placement's own way out, or one of a placement only reached through it,
        // leaves the input empty the first time the run arrives, and the run fails there on no_input.
        if before.is_none() {
            before = Some(reachable_without(entry_id, pic, by_id, defs, Some(placement.id)));
        }
        if !before.as_ref().is_some_and(|b| b.contains(&wire.from_id)) {
            continue;
        }
        let Some(from) = by_id.get(&wire.from_id) else { continue };
        let far = &defs[&from.id];
        let Some(exit) = declared_exit(far, wire.from_exit_id, AutomationOwner::Action, far.action_id) else {
            continue;
        };
        if far.outs_of(exit.id).iter().any(|p| p.id == wire.from_port_id) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// **The way out a placement never leaves by**, as it is set there
/// ([`crate::ops::automation_builtin::never_leaves_by`]) — `None` for an action somebody wrote.
fn never_taken(
    conn: &Connection,
    pic: &Picture,
    placement: &AutomationPlacement,
    def: &ActionDef,
) -> Result<Option<&'static str>> {
    let Some(key) = action_builtin(conn, placement.action_id)? else {
        return Ok(None);
    };
    let settings = answers(pic, placement, def);
    Ok(crate::ops::automation_builtin::never_leaves_by(&key, |setting| {
        settings.iter().find(|cfg| cfg.name == setting).and_then(|cfg| cfg.value.as_deref())
    }))
}

/// One placement's settings, **declaration and answer together**. Public because the build screen draws
/// the same pair and must not put them back together a second way. The action declares and carries no
/// answer; the placement answers on a row of its own under the same name
/// ([`crate::ops::automation::cfg_set`]), so the two have to be put back together here. What is declared
/// is read off the version the placement stands on ([`ActionDef::placed`]).
pub fn settings_of(conn: &Connection, placement: &AutomationPlacement) -> Result<Vec<AutomationCfg>> {
    answered(conn, placement, &ActionDef::placed(conn, placement)?)
}

/// [`settings_of`], with the action the placement stands on already read.
pub(crate) fn answered(
    conn: &Connection,
    placement: &AutomationPlacement,
    def: &ActionDef,
) -> Result<Vec<AutomationCfg>> {
    let mut out = Vec::with_capacity(def.cfgs.len());
    for mut cfg in def.cfgs.iter().cloned() {
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

/// One setting as a run's copy keeps it.
fn run_def_cfg(c: AutomationCfg) -> RunDefCfg {
    RunDefCfg { name: c.name, kind: c.kind, required: c.required, options: c.options, value: c.value }
}

/// [`answered`], with the answers read off `pic` — the draft or a saved version — rather than the store.
fn answers(pic: &Picture, placement: &AutomationPlacement, def: &ActionDef) -> Vec<AutomationCfg> {
    def.cfgs
        .iter()
        .cloned()
        .map(|mut cfg| {
            cfg.value = pic.answer(placement.id, &cfg.name);
            cfg
        })
        .collect()
}

/// **Launch an automation**: check it, copy what is placed on it into a run, and start it.
///
/// Five things refuse, in this order.
///
/// - **Archived.** Archiving is what keeps an automation nobody launches any more out of the lists, and
///   launching one straight past that would make the word mean nothing.
/// - **Stopped.** A person stopped it so that it starts no new round ([`Automation::stopped`]); a launch
///   is one.
/// - **The check** ([`check`]), as one `not_ready` refusal carrying a part per reason.
/// - **The workspace being closed**, last on purpose. What the check found is wrong with the automation
///   and stays wrong after a window is opened, so saying "open a window" first would send somebody to do
///   that and then tell them the automation was never going to run.
/// - **Its most runs under way.** A launch is refused once `max_concurrent_runs` of the automation's
///   runs are `running` or `paused` — one, unless somebody raised it. A paused one can be picked up
///   again, so it holds a place as much as a running one. It is counted in the same transaction the run
///   is written in. Last, since it is the only one that goes away by waiting.
///
/// The run is born `running`, so a launch never waits (`AMB-D-947`): it is either refused or under
/// way. `started_at` is the moment of the launch itself.
///
/// It reads the saved definition ([`ReadsFrom::Saved`]). It makes the run even where the entry would
/// wait for a task, which the run's entry then finds ([`wait_for_a_task`]); a person's start asks
/// that first and makes no run ([`start`]).
pub fn launch(tx: &WriteTx<'_>, automation_id: i64, by: &Launcher<'_>) -> Result<AutomationRun> {
    launch_handing(tx, automation_id, by, &HandedAtLaunch::default(), ReadsFrom::Saved)
}

/// **What a start of an automation came to** ([`start`]).
#[derive(Clone, Debug)]
pub enum Started {
    /// A run was launched.
    Run(AutomationRun),
    /// **Its entry would wait for a task with none to take**, so no run was made and the automation
    /// waits for one instead ([`Automation::waits_for_task_since`]).
    Waits(Box<Automation>),
}

impl Started {
    /// The run, for a launch that was never let wait.
    fn run(self) -> Result<AutomationRun> {
        match self {
            Started::Run(run) => Ok(run),
            Started::Waits(automation) => Err(Error::invalid(format!(
                "automation '{}' was launched to wait for a task, and only a start waits",
                automation.name
            ))),
        }
    }
}

/// **Start an automation as a person does** — [`launch_handing`] of the newest saved version, refused
/// over everything a launch is refused over, and in that order.
///
/// **Where the entry would wait for a task and none can be taken now, no run is made**
/// (`AMB-D-969`): the automation is marked as waiting for one instead ([`Started::Waits`]), and the
/// app's watch starts a run once one turns up ([`start_waiting`]). A run made then would only stand
/// at its entry, counted among the runs under way and doing nothing.
pub fn start(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    handed: &HandedAtLaunch,
) -> Result<Started> {
    hand(tx, automation_id, by, handed, ReadsFrom::Saved, true, true)
}

/// **Which definition of an automation a launch reads** — what is checked and copied down onto the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadsFrom {
    /// Its newest saved version, so what is being written on the automation is not what runs. An
    /// automation nobody has saved has only its draft, and that is read. A launch and a run picked up
    /// before its next task read this.
    Saved,
    /// The draft — its picture as it is written now, saved or not. A test run reads this, to try what is
    /// being written before it is saved.
    Draft,
}

/// **The picture `reads` names, and the saved version it is** — `None` for the draft.
fn definition(conn: &Connection, automation: &Automation, reads: ReadsFrom) -> Result<(Picture, Option<i64>)> {
    let saved = match reads {
        ReadsFrom::Saved => read::automation_version_latest(conn, automation.id)?,
        ReadsFrom::Draft => None,
    };
    match saved {
        Some(saved) => Ok((Picture::saved(&saved)?, Some(saved.version))),
        None => Ok((Picture::read(conn, automation)?, None)),
    }
}

/// [`launch`], with what a person handed over along with it ([`HandedAtLaunch`]), reading the definition
/// `reads` names. The task to file is kept on the run and the files hang off it, both written before any
/// step is opened.
///
/// **A file needs somebody who handed it over.** An attachment says who put it there, and a launch
/// whose caller says nothing about itself has no one to name, so it is refused rather than guessed at.
pub fn launch_handing(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    handed: &HandedAtLaunch,
    reads: ReadsFrom,
) -> Result<AutomationRun> {
    hand(tx, automation_id, by, handed, reads, true, false)?.run()
}

/// [`launch_handing`], not refused over the automation's most runs under way. For a test run, whose run
/// is thrown away with its transaction and so never stands beside the others.
pub fn launch_handing_past_the_limit(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    handed: &HandedAtLaunch,
    reads: ReadsFrom,
) -> Result<AutomationRun> {
    hand(tx, automation_id, by, handed, reads, false, false)?.run()
}

/// **What the triggers say on one look** ([`triggers_due`]): the automations whose trigger is due now,
/// the ones due but for what their scripts say, and when the soonest of the rest is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TriggersDue {
    /// The automations to [`fire`], oldest first.
    pub due: Vec<i64>,
    /// The automations whose trigger is due but for its scripts, oldest first, each with them: run them
    /// outside any write ([`ask_trigger`]), and [`fire`] the automation only where they said to file a
    /// task, handing it what they said.
    pub ask: Vec<(i64, TriggerScripts)>,
    /// When the next trigger not yet due is, or `None` where there is none — so a look can sleep no
    /// longer than that.
    pub next_at: Option<Timestamp>,
}

/// **Which triggers are due at `now`** — the first part of each, asked of every automation that may be
/// started by one ([`read::automations_triggered`]), which leaves out a stopped one. One whose runs
/// under way say no ([`has_room`]), or whose condition does not hold ([`condition_holds`]), is not
/// due: it would decide not to file a task, so a look need not open a write to hear it say so. One
/// with a script — one that asks whether to file a task, or one that writes what it says — is put in
/// `ask` rather than `due`: its program may run for hours, so it is not run here. It writes nothing:
/// [`fire`] asks all of it again, in the transaction it writes in.
pub fn triggers_due(conn: &Connection, now: Timestamp) -> Result<TriggersDue> {
    let mut found = TriggersDue::default();
    for automation in read::automations_triggered(conn)? {
        let Some(trigger) = &automation.trigger else { continue };
        if trigger.next_at <= now {
            let under_way = read::automation_run_ids_under_way(conn, automation.id)?;
            // A condition that no longer reads (an axis it names gone since) is left for `fire` to
            // refuse, as a launch refused is, rather than stopping every other trigger on this look.
            if has_room(&trigger.when, under_way.len(), automation.max_concurrent_runs)
                && condition_holds(conn, automation.project_id, trigger).unwrap_or(true)
            {
                match TriggerScripts::of(trigger) {
                    Some(scripts) => found.ask.push((automation.id, scripts)),
                    None => found.due.push(automation.id),
                }
            }
        } else if found.next_at.is_none_or(|soonest| trigger.next_at < soonest) {
            found.next_at = Some(trigger.next_at);
        }
    }
    Ok(found)
}

/// **Fire an automation's trigger** at `now`: decide whether to file a task, write what it says, and
/// start a run of the automation on it — `Some(run)` when it did, `None` when it decided not to.
///
/// 1. **Whether to file one.** Not before the trigger's `next_at`, and not while its runs under way say
///    no ([`has_room`]): an `every` or a `tasks` waits while the automation has its most runs under way
///    (`max_concurrent_runs`), where a task filed would have no run to go to, and an `idle` waits until
///    it has none at all. Nor while its condition does not hold ([`condition_holds`]). One with a
///    script files one only where `said` was heard from those very scripts — the ones [`ask_trigger`]
///    ran outside this write — so one rewritten since they were asked files nothing. An archived
///    automation, a stopped one, one kept for export only and one with no trigger file none either.
/// 2. **What it says** ([`crate::model::TriggerTask`]): the title and notes written on the trigger, the
///    ones read from its file now ([`task_from_file`]), or the ones its program wrote in `said`. A file
///    that says nothing it can file is an error, as a launch refused is.
/// 3. **The run.** A launch like any other ([`launch_handing`]), of the newest saved version, the task
///    handed to its entry as a person's would be — so it is the entry, the built-in that files a task,
///    that files it. What `by` says is passed on as it stands. The task is handed with the trigger's
///    role and name too: the entry files it under that role, and leaves a line in its timeline naming
///    the trigger ([`crate::activity_log::event::task_filed_by_trigger`]).
///
/// **A time it decides no leaves nothing**: no task, no run, and `next_at` where it was, so the next
/// look asks again. A launch refused by its checks is an error, and the transaction it is written in
/// takes nothing with it either. Only a task filed moves `next_at` on, counted from `now` rather than
/// from when it was due, so an app that was closed for a day files one task when it comes back rather
/// than a day's worth.
pub fn fire(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    now: Timestamp,
    said: Option<&ScriptsSaid>,
) -> Result<Option<AutomationRun>> {
    let before = read::automation(tx.conn(), automation_id)?.ok_or_else(|| not_found("automation", automation_id))?;
    let Some(trigger) = before.trigger.clone() else { return Ok(None) };
    if before.archived || before.stopped || before.export_only || now < trigger.next_at {
        return Ok(None);
    }
    let under_way = read::automation_run_ids_under_way(tx.conn(), automation_id)?;
    if !has_room(&trigger.when, under_way.len(), before.max_concurrent_runs)
        || !condition_holds(tx.conn(), before.project_id, &trigger)?
        || TriggerScripts::of(&trigger).is_some_and(|scripts| said.map(|said| &said.scripts) != Some(&scripts))
    {
        return Ok(None);
    }
    let (title, notes) = match &trigger.task {
        crate::model::TriggerTask::Fixed { title, notes } => (title.clone(), notes.clone()),
        crate::model::TriggerTask::File { path } => task_from_file(path).map_err(Error::invalid)?,
        crate::model::TriggerTask::Script { .. } => match said.and_then(|said| said.wrote.clone()) {
            Some(wrote) => wrote,
            None => return Ok(None),
        },
    };
    let handed = HandedAtLaunch {
        title: Some(title),
        notes: Some(notes),
        role_id: trigger.role_id,
        trigger: Some(trigger.name.clone()),
        ..Default::default()
    };
    let run = launch_handing(tx, automation_id, by, &handed, ReadsFrom::Saved)?;
    let mut after = before.clone();
    after.trigger = Some(crate::model::AutomationTrigger { next_at: trigger.when.after(now), ..trigger });
    crate::ops::emit_update(tx, record::automation(&before), record::automation(&after))?;
    Ok(Some(run))
}

/// **The entry of `pic`**: the key of the built-in it stands on (`None` for an agent's step or a
/// script), with the answers written for it there as a run's copy would keep them — or `None` where
/// `pic` names no entry.
fn entry_builtin(conn: &Connection, pic: &Picture) -> Result<Option<EntryBuiltin>> {
    let Some(entry) = pic.entry_placement_id.and_then(|id| pic.placement(id)) else { return Ok(None) };
    let cfg = answers(pic, entry, &ActionDef::placed(conn, entry)?).into_iter().map(run_def_cfg).collect();
    Ok(Some(EntryBuiltin { builtin: action_builtin(conn, entry.action_id)?, cfg, roles: roles_on(pic) }))
}

/// **The entry of a picture as a waiting built-in reads it** ([`entry_builtin`]): the built-in placed
/// there, the answers written for it, and the roles a run of the picture would start rounds of.
struct EntryBuiltin {
    builtin: Option<String>,
    cfg: Vec<RunDefCfg>,
    roles: Vec<i64>,
}

/// **The roles a run launched from `pic` would start rounds of** — [`roles_started`] read before any
/// run is: the picture's roles that name a placement on it as where their round starts, each once.
fn roles_on(pic: &Picture) -> Vec<i64> {
    pic.roles()
        .iter()
        .filter(|r| r.entry_placement_id.is_some_and(|id| pic.placement(id).is_some()))
        .map(|r| r.id)
        .collect()
}

/// **Mark an automation as waiting for a task** ([`Automation::waits_for_task_since`]) — from now, or
/// from when it already was.
fn mark_waiting(tx: &WriteTx<'_>, before: Automation) -> Result<Automation> {
    if before.waits_for_task_since.is_some() {
        return Ok(before);
    }
    let after = Automation { waits_for_task_since: Some(Timestamp::now()), ..before.clone() };
    crate::ops::emit_update(tx, record::automation(&before), record::automation(&after))?;
    Ok(after)
}

/// **End a run whose built-in would wait for a task, and have its automation wait instead**
/// (`AMB-D-969`) — where the run came back to its entry with no task to take.
///
/// **The run completes** there, its step never opened: it found nothing to do, as one leaving by the
/// way out for nothing to take does. It holds no task, so there is none to hand back. **The automation
/// is marked as waiting**, and the app's watch starts a run of it once a task turns up
/// ([`start_waiting`]). So waiting leaves no run standing, counted among those under way, and no step or
/// stretch.
pub(crate) fn wait_for_a_task(tx: &WriteTx<'_>, run: AutomationRun) -> Result<AutomationRun> {
    let automation_id = run.automation_id;
    let ended = crate::ops::automation_stop::ended(tx, run, crate::ops::automation_stop::Ending::Completed)?;
    let automation =
        read::automation(tx.conn(), automation_id)?.ok_or_else(|| not_found("automation", automation_id))?;
    mark_waiting(tx, automation)?;
    Ok(ended.run)
}

/// **Which of these waiting automations a run may be started of now** ([`start_waiting`]) — those a
/// task has turned up for, with room under `max_concurrent_runs`, and those whose newest saved entry no
/// longer waits for one, whose wait [`start_waiting`] ends.
///
/// Only whether a task has turned up is asked, one row of the tasks each, never a count
/// (`AMB-D-969`). It writes nothing: [`start_waiting`] asks all of it again, in the transaction it
/// writes in. The caller says which to ask, so it can leave each one alone for a while after asking
/// (`AMB-D-983`).
pub fn tasks_turned_up(conn: &Connection, automations: &[i64]) -> Result<Vec<i64>> {
    let mut due = Vec::new();
    for &automation_id in automations {
        let Some(automation) = read::automation(conn, automation_id)? else { continue };
        if automation.waits_for_task_since.is_none() || automation.archived || automation.stopped || automation.export_only {
            continue;
        }
        if read::automation_run_ids_under_way(conn, automation_id)?.len() >= automation.max_concurrent_runs as usize {
            continue;
        }
        let (pic, _) = definition(conn, &automation, ReadsFrom::Saved)?;
        let turned_up = match entry_builtin(conn, &pic)? {
            Some(e) => crate::ops::automation_builtin::turned_up(
                conn,
                automation.project_id,
                &e.roles,
                e.builtin.as_deref(),
                &e.cfg,
            )?,
            None => None,
        };
        if turned_up != Some(false) {
            due.push(automation_id);
        }
    }
    Ok(due)
}

/// **Start a run of an automation waiting for a task, now that one has turned up** — `Some(run)` where
/// it did, `None` where there was nothing to start.
///
/// It starts none where the automation is not waiting, is archived, stopped or kept for export only,
/// has its most runs under way, or no task has turned up after all: the wait goes on, and nothing is
/// written.
/// Where its newest saved entry no longer waits for a task, the wait ends and no run is made — a run
/// started then would start for no task.
///
/// The run is a launch like any other ([`launch_handing`]) of the newest saved version, with nothing
/// handed to it, and the wait ends in the same transaction. A launch its checks refuse is an error, and
/// the transaction takes nothing with it: the automation goes on waiting.
pub fn start_waiting(tx: &WriteTx<'_>, automation_id: i64, by: &Launcher<'_>) -> Result<Option<AutomationRun>> {
    let before = read::automation(tx.conn(), automation_id)?.ok_or_else(|| not_found("automation", automation_id))?;
    if before.waits_for_task_since.is_none() || before.archived || before.stopped || before.export_only {
        return Ok(None);
    }
    if read::automation_run_ids_under_way(tx.conn(), automation_id)?.len() >= before.max_concurrent_runs as usize {
        return Ok(None);
    }
    let (pic, _) = definition(tx.conn(), &before, ReadsFrom::Saved)?;
    let turned_up = match entry_builtin(tx.conn(), &pic)? {
        Some(e) => crate::ops::automation_builtin::turned_up(
            tx.conn(),
            before.project_id,
            &e.roles,
            e.builtin.as_deref(),
            &e.cfg,
        )?,
        None => None,
    };
    match turned_up {
        Some(false) => return Ok(None),
        None => {
            stop_waiting(tx, automation_id)?;
            return Ok(None);
        }
        Some(true) => {}
    }
    let run = launch_handing(tx, automation_id, by, &HandedAtLaunch::default(), ReadsFrom::Saved)?;
    stop_waiting(tx, automation_id)?;
    Ok(Some(run))
}

/// **Stop an automation waiting for a task** — a person saying not to start a run once one turns up, or
/// [`start_waiting`] having started one. One that is not waiting is answered as it stands, with nothing
/// written.
pub fn stop_waiting(tx: &WriteTx<'_>, automation_id: i64) -> Result<Automation> {
    let before = read::automation(tx.conn(), automation_id)?.ok_or_else(|| not_found("automation", automation_id))?;
    if before.waits_for_task_since.is_none() {
        return Ok(before);
    }
    let after = Automation { waits_for_task_since: None, ..before.clone() };
    crate::ops::emit_update(tx, record::automation(&before), record::automation(&after))?;
    Ok(after)
}

/// **An automation waiting for a task**, as a person reads it: since when, and what its newest saved
/// entry looks for (`None` where that entry no longer waits for anything).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskWait {
    pub since: Timestamp,
    pub looks_for: Option<String>,
}

/// **Whether this automation is waiting for a task** ([`TaskWait`]), or `None` where it is not.
pub fn task_wait(conn: &Connection, automation_id: i64) -> Result<Option<TaskWait>> {
    let automation = read::automation(conn, automation_id)?.ok_or_else(|| not_found("automation", automation_id))?;
    let Some(since) = automation.waits_for_task_since else { return Ok(None) };
    let (pic, _) = definition(conn, &automation, ReadsFrom::Saved)?;
    let looks_for = entry_builtin(conn, &pic)?
        .and_then(|e| crate::ops::automation_builtin::looks_for(e.builtin.as_deref(), &e.cfg));
    Ok(Some(TaskWait { since, looks_for }))
}

/// **What is keeping this automation waiting** (`AMB-D-999`) — the tasks its newest saved entry looks
/// for that cannot be taken, by what stops them. `None` for an automation that is not waiting. Read
/// when a person is about to look, never by the watch's look.
pub fn task_wait_held_back(
    conn: &Connection,
    automation_id: i64,
) -> Result<Option<crate::ops::automation_builtin::HeldBack>> {
    let automation = read::automation(conn, automation_id)?.ok_or_else(|| not_found("automation", automation_id))?;
    if automation.waits_for_task_since.is_none() {
        return Ok(None);
    }
    let (pic, _) = definition(conn, &automation, ReadsFrom::Saved)?;
    let Some(e) = entry_builtin(conn, &pic)? else { return Ok(None) };
    crate::ops::automation_builtin::held_back(conn, automation.project_id, &e.roles, e.builtin.as_deref(), &e.cfg)
}

/// **Whether the runs under way leave a trigger room to file a task** — `under_way` being the
/// automation's runs `running` or `paused`, and `max` its `max_concurrent_runs`. An `every`, a `tasks`
/// or a `script` files one while there is room for one more run; an `idle` only once there is none
/// under way.
fn has_room(when: &crate::model::TriggerWhen, under_way: usize, max: u32) -> bool {
    match when {
        crate::model::TriggerWhen::Every { .. }
        | crate::model::TriggerWhen::Tasks
        | crate::model::TriggerWhen::Script => under_way < max as usize,
        crate::model::TriggerWhen::Idle => under_way == 0,
    }
}

/// **Whether a trigger's condition lets it file a task** — fewer than `below` tasks of the automation's
/// project (`project_id`) matching its filter. One with no condition always does.
fn condition_holds(conn: &Connection, project_id: i64, trigger: &crate::model::AutomationTrigger) -> Result<bool> {
    let Some(condition) = &trigger.condition else { return Ok(true) };
    let matched = crate::query::count(conn, crate::reach::Reach::binding(project_id), &condition.filter)?;
    Ok(matched < condition.below as usize)
}

/// The version of the `output.json` a trigger's script writes.
pub const TRIGGER_OUTPUT_VERSION: u64 = 1;

/// **The programs a trigger runs outside any write** ([`TriggersDue::ask`]): the one it asks whether
/// to file a task ([`crate::model::AutomationTrigger::script`]), and the one that writes what the task
/// says ([`crate::model::TriggerTask::Script`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerScripts {
    pub ask: Option<StepScript>,
    pub write: Option<StepScript>,
}

impl TriggerScripts {
    /// The programs `trigger` runs — `None` where it runs none.
    pub fn of(trigger: &crate::model::AutomationTrigger) -> Option<Self> {
        let scripts = TriggerScripts { ask: trigger.script.clone(), write: trigger.task.script().cloned() };
        (scripts.ask.is_some() || scripts.write.is_some()).then_some(scripts)
    }
}

/// **What a trigger's programs said** ([`ask_trigger`]) — that it is to file a task, and handed to
/// [`fire`] with the programs that said so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptsSaid {
    /// The programs that were run, as they stood when they were.
    pub scripts: TriggerScripts,
    /// The title and notes the program that writes the task wrote — `None` where there is none.
    pub wrote: Option<(String, String)>,
}

/// **Run a trigger's programs** — first the one that asks whether to file a task ([`ask_script`]),
/// then the one that writes what it says ([`write_script`]), each stopped at its timeout or once
/// `stop` answers `true`. `Ok(None)` is the first saying no, which leaves the second unrun. Like
/// [`ask_script`], it writes nothing to the store and may run for hours, so it is run outside any
/// write, and what it said is handed to [`fire`]. `Err` is why one of them said nothing, in a
/// sentence; the trigger files nothing then.
pub fn ask_trigger(
    scripts: &TriggerScripts,
    mut stop: impl FnMut() -> bool,
) -> std::result::Result<Option<ScriptsSaid>, String> {
    if let Some(script) = &scripts.ask {
        if !ask_script(script, &mut stop)? {
            return Ok(None);
        }
    }
    let wrote = scripts.write.as_ref().map(|script| write_script(script, &mut stop)).transpose()?;
    Ok(Some(ScriptsSaid { scripts: scripts.clone(), wrote }))
}

/// **Ask a trigger's script whether to file a task** — run it once ([`automation_script::run`]), with
/// no inputs, stopping it at its timeout or once `stop` answers `true`, and read its answer. It writes
/// nothing to the store and may run for hours, so it is run outside any write, and what it said is
/// handed to [`fire`].
///
/// It answers in `output.json`, `{"version":1,"file":true}` to file one and `"file":false` not to —
/// never in its exit code, which says only whether it ran to the end (`AMB-D-1016`): a script that
/// fell over is not taken for one that said no. `Err` is why it said neither, in a sentence: it could
/// not be started, ran out of time, was stopped, ended other than with 0, or left no `output.json` or
/// one not in that shape. The trigger files nothing then either.
pub fn ask_script(script: &StepScript, stop: impl FnMut() -> bool) -> std::result::Result<bool, String> {
    script_said(script, &automation_script::run(script, &[], stop).ended)
}

/// **Have the program of a [`crate::model::TriggerTask::Script`] write what the task says** — run as
/// [`ask_script`] runs a trigger's own script, with no inputs, and answered the same way, in
/// `output.json` alone: `{"version":1,"title":"…","notes":"…"}`, the notes left out where there are
/// none. The title and the notes are handed back in that order. `Err` is why it wrote nothing that
/// can be filed — a blank title included.
pub fn write_script(script: &StepScript, stop: impl FnMut() -> bool) -> std::result::Result<(String, String), String> {
    script_wrote(script, &automation_script::run(script, &[], stop).ended)
}

/// What a trigger's script said, from how its run `ended` ([`ask_script`]).
fn script_said(script: &StepScript, ended: &automation_script::Ended) -> std::result::Result<bool, String> {
    let output = script_output(script, ended)?;
    let unshaped = |what: &str| format!("output.json is not in the shape {{\"version\":1,\"file\":true|false}}: {what}");
    if output.get("version").and_then(|v| v.as_u64()) != Some(TRIGGER_OUTPUT_VERSION) {
        return Err(unshaped("\"version\" is not 1"));
    }
    output.get("file").and_then(|v| v.as_bool()).ok_or_else(|| unshaped("\"file\" is not true or false"))
}

/// What the program of a trigger's task wrote, from how its run `ended` ([`write_script`]).
fn script_wrote(script: &StepScript, ended: &automation_script::Ended) -> std::result::Result<(String, String), String> {
    let output = script_output(script, ended)?;
    let unshaped =
        |what: &str| format!("output.json is not in the shape {{\"version\":1,\"title\":\"…\",\"notes\":\"…\"}}: {what}");
    if output.get("version").and_then(|v| v.as_u64()) != Some(TRIGGER_OUTPUT_VERSION) {
        return Err(unshaped("\"version\" is not 1"));
    }
    let title = match output.get("title").and_then(|v| v.as_str()).map(str::trim) {
        Some(title) if !title.is_empty() => title.to_string(),
        Some(_) => return Err(unshaped("\"title\" is blank")),
        None => return Err(unshaped("\"title\" is not a string")),
    };
    let notes = match output.get("notes") {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(notes) => notes.as_str().ok_or_else(|| unshaped("\"notes\" is not a string"))?.to_string(),
    };
    Ok((title, notes))
}

/// The `output.json` a trigger's program wrote, from how its run `ended` — or why there is none.
fn script_output<'a>(
    script: &StepScript,
    ended: &'a automation_script::Ended,
) -> std::result::Result<&'a serde_json::Value, String> {
    use automation_script::Ended;
    match ended {
        Ended::NotStarted(why) => Err(format!("the program could not be started: {why}")),
        Ended::TimedOut => Err(format!(
            "the program was still running after {} minutes, and was stopped",
            script.timeout_minutes
        )),
        Ended::Stopped => Err("the program was stopped before it ended".to_string()),
        Ended::Failed(status) => Err(match status.code() {
            Some(code) => format!("the program ended with exit code {code}"),
            None => format!("the program ended without an exit code ({status})"),
        }),
        Ended::NoOutput => Err("the program ended with exit code 0 and wrote no output.json".to_string()),
        Ended::NotJson(why) => Err(format!("output.json is not JSON: {why}")),
        Ended::Wrote { output, .. } => Ok(output),
    }
}

/// **Read what a [`crate::model::TriggerTask::File`] says** — the file at `path`, as it stands now: its
/// first line that is not blank is the title, and what follows it the notes, blank lines around them
/// left out. `Err` is why it says nothing that can be filed: it cannot be read as UTF-8 text, or has
/// no line that is not blank.
pub fn task_from_file(path: &str) -> std::result::Result<(String, String), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("the file '{path}' could not be read: {e}"))?;
    let text = text.trim_start();
    if text.is_empty() {
        return Err(format!("the file '{path}' is blank, so it has no title"));
    }
    let (title, mut notes) = text.split_once('\n').unwrap_or((text, ""));
    // Blank lines before the notes are left out whole, so notes that open indented keep their indent.
    while let Some((line, rest)) = notes.split_once('\n') {
        if !line.trim().is_empty() {
            break;
        }
        notes = rest;
    }
    Ok((title.trim().to_string(), notes.trim_end().to_string()))
}

/// [`launch_handing`], counting the runs under way only where `limited`, and making no run where the
/// entry would wait for a task only where `may_wait` ([`start`]).
fn hand(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    handed: &HandedAtLaunch,
    reads: ReadsFrom,
    limited: bool,
    may_wait: bool,
) -> Result<Started> {
    let who = match (handed.files.is_empty(), by.by) {
        (true, _) => None,
        (false, Some(who)) => Some(who),
        (false, None) => {
            return Err(Error::invalid(
                "a file handed over at launch needs the launcher to say who it is — pass who is launching",
            ))
        }
    };
    let run = match launch_asking(tx, automation_id, by, handed, reads, limited, may_wait, |_| true)? {
        Started::Run(run) => run,
        waits @ Started::Waits(_) => return Ok(waits),
    };
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
    Ok(Started::Run(run))
}

/// [`launch`], with the check's two reasons about how a task is held let through — the lines that
/// leave a taken task open, and a person's own action handing one on. For a test of how a run walks,
/// opens, reports or stops, where the picture's shape is not the subject, and for a test of the safety
/// net a run carries for a line the check missed (`AMB-D-967`).
///
/// A step of a person's own that hands on the task is how those tests draw a run coming to hold one
/// without a built-in in front of it; the refusal of such a way out is asked in this module's tests.
#[cfg(test)]
pub(crate) fn launch_past_the_task_checks(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
) -> Result<AutomationRun> {
    launch_asking(tx, automation_id, by, &HandedAtLaunch::default(), ReadsFrom::Saved, true, false, |unmet| {
        !matches!(unmet, Unmet::LeavesTaskOpen { .. } | Unmet::HandsOnTaskTaken { .. })
    })?
    .run()
}

/// [`launch_past_the_task_checks`], with a setting that names what the project does not have let through
/// as well. For a test of what a built-in does when a run reaches such a setting: the check refuses it at
/// launch, and a run can still meet one, since what was there at launch can be gone by the time the run
/// gets to it (`AMB-D-987`).
#[cfg(test)]
pub(crate) fn launch_past_the_setting_checks(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
) -> Result<AutomationRun> {
    launch_asking(tx, automation_id, by, &HandedAtLaunch::default(), ReadsFrom::Saved, true, false, |unmet| {
        !matches!(
            unmet,
            Unmet::LeavesTaskOpen { .. }
                | Unmet::HandsOnTaskTaken { .. }
                | Unmet::CfgNotFound { .. }
                | Unmet::CfgValueClosed { .. }
                | Unmet::FolderUnchosen { .. }
        )
    })?
    .run()
}

/// [`launch`] of the definition `reads` names, refusing only over what `counts` says counts — and over
/// the runs under way only where `limited` — and keeping what was `handed` on the run. The files are the
/// caller's to attach once the run exists.
///
/// Where `may_wait`, an entry that would wait for a task with none to take makes no run, and the
/// automation waits instead ([`start`]). Asked last, once nothing refuses the launch: an automation
/// that could not be launched is not one to wait on.
#[allow(clippy::too_many_arguments)]
fn launch_asking(
    tx: &WriteTx<'_>,
    automation_id: i64,
    by: &Launcher<'_>,
    handed: &HandedAtLaunch,
    reads: ReadsFrom,
    limited: bool,
    may_wait: bool,
    counts: impl Fn(&Unmet) -> bool,
) -> Result<Started> {
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
    if automation.stopped {
        return Err(Error::invalid(format!(
            "automation '{}' is stopped — start it again (`automation stop {} --off`) before launching it",
            automation.name, automation.id
        )));
    }
    let (pic, version) = definition(tx.conn(), &automation, reads)?;
    let unmet: Vec<Unmet> = check_picture(tx.conn(), &automation, &pic, by.startable, by.models, by.default_agent)?
        .into_iter()
        .filter(|u| counts(u))
        .collect();
    if !unmet.is_empty() {
        return Err(not_ready("launch", &automation.name, &unmet));
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
    // What was handed is checked before the room left: a launch handed the wrong thing would not go
    // through with room to spare either, and what to fix is the person's to see first.
    let handed_task = entry_reads(tx.conn(), &automation, &pic, handed)?
        .map(|task| serde_json::to_string(&task).map_err(Error::from))
        .transpose()?;
    if limited {
        let max = automation.max_concurrent_runs;
        let mut runs = read::automation_run_ids_under_way(tx.conn(), automation_id)?;
        if runs.len() >= max as usize {
            runs.sort_unstable();
            let runs = runs.iter().map(i64::to_string).collect::<Vec<_>>().join(", ");
            return Err(Error::Invalid(
                Msg::new(format!(
                    "automation '{}' already has {max} run(s) under way, the most it may have at once \
                     ({runs}) — wait for one to end before launching it again",
                    automation.name
                ))
                .coded(ErrorCode::InvalidAutomationRunLimit)
                .with("automation", &automation.name)
                .with("max", max)
                .with("runs", runs),
            ));
        }
    }
    if may_wait {
        if let Some(e) = entry_builtin(tx.conn(), &pic)? {
            let waits = crate::ops::automation_builtin::waits_now(
                tx.conn(),
                automation.project_id,
                &e.roles,
                e.builtin.as_deref(),
                &e.cfg,
            )?;
            if waits {
                return Ok(Started::Waits(Box::new(mark_waiting(tx, automation)?)));
            }
        }
    }
    let now = Timestamp::now();
    let run = AutomationRun {
        id: read::next_id(tx.conn(), "automation_run")?,
        automation_id,
        project_id: automation.project_id,
        status: AutomationRunStatus::Running,
        pause_requested: false,
        pause_before_next_task: false,
        pause_kind: None,
        stopped_reason: None,
        started_by_kind: by.by,
        started_at: Some(now),
        ended_at: None,
        acknowledged_at: None,
        handed_task,
        acknowledged_by_kind: None,
        stopped_detail: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::automation_run(&run))?;
    copy_down(tx, run.id, &pic, version, by.default_agent, now)?;
    Ok(Started::Run(run))
}

/// **Copy every step of `automation` down onto the run** — [`snapshot`] for each step of each
/// placement, at launch and again where a run paused before its next task is picked up (`AMB-D-1015`).
///
/// **The entry's copy is written first**, so a run's copies fall into one run of rows per time they
/// were taken, each starting at its entry ([`current_defs`]). The rows of an earlier copy are kept:
/// the executions opened from them still point at them.
///
/// What is copied is `pic`, the definition the launch read ([`ReadsFrom`]), and every row of one copy
/// records the saved version it is — `version`, `None` for the draft. **What is copied of each placement
/// is the version of the action it stands on** ([`ActionDef::placed`]), not the action as it has since
/// been written on.
fn copy_down(
    tx: &WriteTx<'_>,
    run_id: i64,
    pic: &Picture,
    version: Option<i64>,
    default_agent: Option<&str>,
    now: Timestamp,
) -> Result<()> {
    let defs = defs_of(tx.conn(), &pic.placements)?;
    let by_id: BTreeMap<i64, &AutomationPlacement> = pic.placements.iter().map(|p| (p.id, p)).collect();
    let roles = roles_by_placement(pic, &by_id, &defs);
    let mut steps = Vec::new();
    for placement in &pic.placements {
        let def = &defs[&placement.id];
        for step in steps_opened_by(def) {
            let entry = pic.entry_placement_id == Some(placement.id) && def.entry_step_id == Some(step.id);
            steps.push((placement, step, entry));
        }
    }
    steps.sort_by_key(|(_, _, entry)| !entry);
    for (placement, step, entry) in steps {
        let carried = carrier(pic, &roles, placement.id, step.id, default_agent);
        if let Some(def) = snapshot(tx, run_id, version, pic, placement, &defs, &step, carried, entry, now)? {
            emit_create(tx, record::automation_run_def(&def))?;
        }
    }
    Ok(())
}

/// **Copy the automation down onto a run paused before its next task, afresh** (`AMB-D-1015`), and
/// answer the copy of its entry — where the run picks up.
///
/// It reads the saved definition, as a launch does ([`ReadsFrom::Saved`]). The new copy is checked
/// first, as a launch checks it ([`checked_saved`]): an archived automation, a stopped one, or one
/// [`check`] finds anything unmet in, is refused, and nothing is written.
pub(crate) fn copy_down_again(
    tx: &WriteTx<'_>,
    run: &AutomationRun,
    startable: Option<&[String]>,
    models: &ModelsHere,
    default_agent: Option<&str>,
) -> Result<AutomationRunDef> {
    let (pic, version) = checked_saved(tx.conn(), run, startable, models, default_agent)?;
    copy_down(tx, run.id, &pic, version, default_agent, Timestamp::now())?;
    entry_def(tx.conn(), run.id)?.ok_or_else(|| {
        Error::invalid(format!("run '{}' was copied down with no step to start at", run.id))
    })
}

/// **The automation's newest saved definition, checked as a launch checks it** ([`launch_asking`]) — for
/// a run about to be copied down afresh. An archived automation, a stopped one, or one [`check`] finds
/// anything unmet in, is refused.
fn checked_saved(
    conn: &Connection,
    run: &AutomationRun,
    startable: Option<&[String]>,
    models: &ModelsHere,
    default_agent: Option<&str>,
) -> Result<(Picture, Option<i64>)> {
    let automation: Automation =
        read::automation(conn, run.automation_id)?.ok_or_else(|| not_found("automation", run.automation_id))?;
    if automation.archived {
        return Err(Error::Invalid(
            Msg::new(format!(
                "automation '{}' is archived — bring it back before picking up run '{}'",
                automation.name, run.id
            ))
            .coded(ErrorCode::InvalidAutomationArchived)
            .with("automation", &automation.name),
        ));
    }
    if automation.stopped {
        return Err(Error::invalid(format!(
            "automation '{}' is stopped, and picking up run '{}' would start a new round — start the \
             automation again (`automation stop {} --off`) first",
            automation.name, run.id, automation.id
        )));
    }
    let (pic, version) = definition(conn, &automation, ReadsFrom::Saved)?;
    let unmet = check_picture(conn, &automation, &pic, startable, models, default_agent)?;
    if !unmet.is_empty() {
        return Err(not_ready("launch", &automation.name, &unmet));
    }
    Ok((pic, version))
}

/// **What [`take_up_newer`] did to a run.**
#[derive(Clone, Debug)]
pub enum TakenUp {
    /// Nothing: the run is not standing at its entry, or nothing newer than its copy has been saved.
    Same,
    /// The run was copied down afresh from the newer version, and starts at this, the new copy's entry.
    /// What was waiting to open the old entry is stale.
    Copied(Box<AutomationRunDef>),
    /// The newer version did not pass, so the run failed with
    /// [`AutomationStoppedReason::FailedCheck`]; this is the run as it ended.
    Failed(AutomationRun),
}

/// **Copy a run standing at its entry down afresh, where the automation has been saved since its copy
/// was taken** (`AMB-D-1015`) — asked before the entry is opened, so a run starting its next round
/// ([`goes_round`]), one a line brings back to its entry, or one waiting there for a task to turn up,
/// starts from the newest saved definition.
///
/// - **Standing at its entry** is a `running` run with nothing under way whose next step is the copy of
///   the entry ([`next_def`]). Anywhere else the run may be partway through a task, and is left alone.
/// - **Newer** is a saved version later than the one its entry's copy was taken from. A copy taken from
///   the draft (`None`) is older than any saved version.
/// - **The newer version is checked as a launch checks it** ([`checked_saved`]), and it may not start
///   by filing a task: only a launch is handed what to file ([`entry_reads`]). Either refusal fails the
///   run, with what it did not pass kept on it ([`AutomationRun::stopped_detail`]), and nothing is
///   copied. The run holds no task here, so there is nothing to hand back (`AMB-D-967`).
///
/// The rows of the old copy are kept ([`copy_down`]).
pub fn take_up_newer(
    tx: &WriteTx<'_>,
    run_id: i64,
    startable: Option<&[String]>,
    models: &ModelsHere,
    default_agent: Option<&str>,
) -> Result<TakenUp> {
    let run = read::automation_run(tx.conn(), run_id)?.ok_or_else(|| not_found("run", run_id))?;
    let Waiting::Step(standing) = next_def(tx.conn(), run_id)? else { return Ok(TakenUp::Same) };
    if !standing.entry {
        return Ok(TakenUp::Same);
    }
    let Some(saved) = read::automation_version_latest(tx.conn(), run.automation_id)? else {
        return Ok(TakenUp::Same);
    };
    if standing.automation_version.is_some_and(|copied| copied >= saved.version) {
        return Ok(TakenUp::Same);
    }
    let refused = match checked_saved(tx.conn(), &run, startable, models, default_agent) {
        Ok((pic, version)) => match files_a_task(tx.conn(), &pic)? {
            Some(step) => format!(
                "cannot pick up run '{}' at version {}: the entry '{step}' files a task, and only a launch \
                 is handed what to file",
                run.id, saved.version
            ),
            None => {
                copy_down(tx, run.id, &pic, version, default_agent, Timestamp::now())?;
                let entry = entry_def(tx.conn(), run.id)?.ok_or_else(|| {
                    Error::invalid(format!("run '{}' was copied down with no step to start at", run.id))
                })?;
                return Ok(TakenUp::Copied(Box::new(entry)));
            }
        },
        Err(refusal @ (Error::Invalid(_) | Error::NotReady(_))) => refusal.to_string(),
        Err(other) => return Err(other),
    };
    let mut detailed = run.clone();
    detailed.stopped_detail = Some(refused);
    crate::ops::emit_update(tx, record::automation_run(&run), record::automation_run(&detailed))?;
    let ended = crate::ops::automation_stop::ended(
        tx,
        detailed,
        crate::ops::automation_stop::Ending::Failed(AutomationStoppedReason::FailedCheck),
    )?;
    Ok(TakenUp::Failed(ended.run))
}

/// The name of the entry's action where it is the built-in that files a task, `None` where it is not.
fn files_a_task(conn: &Connection, pic: &Picture) -> Result<Option<String>> {
    let Some(entry) = pic.entry_placement_id.and_then(|id| pic.placement(id)) else { return Ok(None) };
    Ok(match action_builtin(conn, entry.action_id)?.as_deref() {
        Some(crate::ops::automation_builtin_make::KEY) => Some(action_name(conn, entry.action_id)?),
        _ => None,
    })
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
///
/// Something handed that the entry does not read is refused: nothing would ever read it, and the
/// person handing it would believe it went somewhere.
fn entry_reads(
    conn: &Connection,
    automation: &Automation,
    pic: &Picture,
    handed: &HandedAtLaunch,
) -> Result<Option<HandedTask>> {
    let Some(entry) = pic.entry_placement_id.and_then(|id| pic.placement(id)) else {
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
        true => {
            let Some(title) = handed.title() else {
                return Err(Error::invalid(format!(
                    "the entry '{step}' files a task, and no title for it was handed over at launch"
                )));
            };
            let settings = answers(pic, entry, &ActionDef::placed(conn, entry)?);
            let classification = crate::ops::automation_builtin_make::handed_at_launch(
                conn,
                &settings,
                automation.project_id,
                &handed.classification,
            )?;
            Ok(Some(HandedTask {
                title: title.to_string(),
                notes: handed.notes().map(str::to_string),
                classification,
                role_id: handed.role_id,
                trigger: handed.trigger.clone(),
            }))
        }
        false if handed.for_a_task() => {
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
/// reservation's refusal takes ([`crate::ops::task`]). `doing` is what was refused — a launch, or a save
/// ([`crate::ops::automation::save`]), which asks the same check.
pub(crate) fn not_ready(doing: &str, name: &str, unmet: &[Unmet]) -> Error {
    refused_over(format!("cannot {doing} '{name}'"), ErrorCode::NotReadyAutomation, "automation", name, unmet)
}

/// The `not_ready` refusal of a save of an action ([`crate::ops::automation::action_save`]), over
/// [`check_action`]'s reasons. It names its own code: what was pressed is a save, not a launch.
pub(crate) fn not_ready_to_save_action(name: &str, unmet: &[Unmet]) -> Error {
    refused_over(format!("cannot save '{name}'"), ErrorCode::NotReadyActionSave, "action", name, unmet)
}

/// One `not_ready` refusal headed by `head`, named by `code` with `name` under `field`, over `unmet`.
fn refused_over(head: String, code: ErrorCode, field: &'static str, name: &str, unmet: &[Unmet]) -> Error {
    let sentence = format!("{head}: {}", unmet.iter().map(Unmet::say).collect::<Vec<_>>().join("; "));
    let msg = unmet
        .iter()
        .fold(Msg::new(sentence).coded(code).with(field, name), |msg, one| msg.part(one.msg()));
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
/// the answer is given once, where the action is placed. The agent and the model are `carried` — the
/// step's own choice at the placement, else its role's default, else Amenbo's ([`carrier`]). The prompt,
/// the agent, the model and the three flags are resolved here rather than kept as a pointer, so the
/// step is not read halfway through the run as whatever it had since been edited into.
///
/// **A step none of the three names an agent for is not copied**, and `None` says so. The launch
/// check refuses a run that could open one ([`Unmet::AgentUnchosen`]), so the only such step left here
/// stands on a placement no run reaches — copied, it would need an agent it does not have. A built-in is copied
/// with nobody named and no prompt: Amenbo carries it out itself (`AMB-D-964`). A script is copied with
/// nobody named too: its program is what is started (`AMB-D-1016`).
///
/// No cycle can be met here: an action places no action (`AMB-D-949`), so opening a placement goes one
/// level down and stops. A loop drawn inside an action is a way back between its steps, walked at run
/// time and held by `max_times`, not something this expands.
#[allow(clippy::too_many_arguments)]
fn snapshot(
    tx: &WriteTx<'_>,
    run_id: i64,
    automation_version: Option<i64>,
    pic: &Picture,
    placement: &AutomationPlacement,
    defs: &Defs,
    step: &AutomationStep,
    carried: Option<Carrier>,
    entry: bool,
    now: Timestamp,
) -> Result<Option<AutomationRunDef>> {
    let conn = tx.conn();
    let def = &defs[&placement.id];
    // A built-in is carried out by Amenbo and a script by its program, so nobody was chosen for either
    // and its copy names nobody.
    let (agent, model) = if step.builtin.is_some() || step.script.is_some() {
        (String::new(), None)
    } else {
        match carried {
            Some(carried) => (carried.agent, carried.model),
            None => return Ok(None),
        }
    };
    let mut exits = Vec::new();
    for exit in def.exits_of(AutomationOwner::Step, step.id) {
        let outs = def
            .outs_of(exit.id)
            .into_iter()
            .map(|p| RunDefPort { id: p.id, name: p.name, kind: p.kind, required: p.required })
            .collect();
        let (then, returns_to) = line_after(pic, placement, defs, step.id, exit.id);
        exits.push(RunDefExit { id: exit.id, name: exit.name.clone(), outs, then, returns_to });
    }
    let mut ins: Vec<RunDefIn> = Vec::new();
    for p in def.ports_of(AutomationPortOwner::Step, step.id, AutomationPortDirection::In) {
        let from = wired_into(pic, placement, defs, step.id, p.id);
        let port = RunDefPort { id: p.id, name: p.name, kind: p.kind, required: p.required };
        ins.push(RunDefIn { port, from });
    }
    // The version of the built-in the placed action was written from (`AMB-D-1000`) — the one whose ways
    // out were just copied, and so the one its code is to behave as.
    let builtin_version = match &step.builtin {
        Some(_) => read::automation_action(conn, placement.action_id)?.and_then(|a| a.builtin_version),
        None => None,
    };
    let cfg: Vec<RunDefCfg> = answers(pic, placement, def).into_iter().map(run_def_cfg).collect();
    // The roles whose round starts here: those naming this placement, on the copy of its first step.
    let roles: Vec<i64> = match def.entry_step_id == Some(step.id) {
        true => pic.roles().iter().filter(|r| r.entry_placement_id == Some(placement.id)).map(|r| r.id).collect(),
        false => Vec::new(),
    };
    Ok(Some(AutomationRunDef {
        id: read::next_id(conn, "automation_run_def")?,
        run_id,
        placement_id: Some(placement.id),
        action_id: Some(def.action_id),
        one_at_a_time: def.one_at_a_time,
        step_id: Some(step.id),
        name: step.name.clone(),
        prompt: step.builtin.is_none().then(|| step.prompt.clone()),
        builtin: step.builtin.clone(),
        builtin_version,
        automation_version,
        script: step.script.clone(),
        agent,
        model,
        interactive: step.interactive,
        work_dir_ref: step.work_dir_ref.clone(),
        report_to_task: step.report_to_task,
        show_history: step.show_history,
        show_notes: step.show_notes,
        show_decisions: step.show_decisions,
        show_comments: step.show_comments,
        // Copied with the default already put in, so the copy says what was expected at launch even
        // where the default moves on afterwards.
        expected_minutes: Some(step.expected_minutes.unwrap_or(DEFAULT_EXPECTED_MINUTES)),
        exits: serde_json::to_string(&exits).map_err(Error::from)?,
        ins: serde_json::to_string(&ins).map_err(Error::from)?,
        cfg: serde_json::to_string(&cfg).map_err(Error::from)?,
        entry,
        roles: serde_json::to_string(&roles).map_err(Error::from)?,
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
    pic: &Picture,
    placement: &AutomationPlacement,
    defs: &Defs,
    step_id: i64,
    exit_id: i64,
) -> (Option<RunDefLine>, Option<i64>) {
    let def = &defs[&placement.id];
    let Some(inner) = def.edge_for_exit(step_id, exit_id) else {
        return (None, None);
    };
    if inner.ends != AutomationEnds::Exit {
        let step = match inner.ends {
            AutomationEnds::Go => inner.to_id,
            _ => None,
        };
        let line = kept(pic, def, inner, Some(placement.id).filter(|_| step.is_some()), step);
        return (Some(line), None);
    }
    let Some(action_exit) = inner.exit_to_id else { return (None, None) };
    let line = match pic.edge_for_exit(placement.id, action_exit) {
        // An automation's picture has no edge of its own to return to, and the write side refuses one.
        None => None,
        Some(outer) if outer.ends == AutomationEnds::Exit => None,
        Some(outer) => {
            let (to, step) = match (outer.ends, outer.to_id) {
                (AutomationEnds::Go, Some(to)) => (Some(to), defs.get(&to).and_then(|d| d.entry_step_id)),
                _ => (None, None),
            };
            Some(kept(pic, def, outer, to, step))
        }
    };
    (line, Some(action_exit))
}

/// One live line as the copy keeps it, going on to `placement_id` / `step_id` where it goes on at all.
///
/// **The limit is kept on a line that goes back, and on no other**
/// ([`crate::ops::automation::lines_back`]). A line going down carries the standing limit it was drawn
/// with, and nobody is shown it — so counting it would stop a loop drawn to go round twenty times at the
/// ten its way down happened to carry.
/// A line inside the action is asked of `def`, the version the placement stands on, and one on the
/// automation's picture of `pic`, the definition the launch read.
fn kept(
    pic: &Picture,
    def: &ActionDef,
    edge: &AutomationEdge,
    placement_id: Option<i64>,
    step_id: Option<i64>,
) -> RunDefLine {
    let goes_back = match (edge.max_times, edge.owner_kind) {
        (None, _) => false,
        (Some(_), AutomationPictureOwner::Action) => def.lines_back().contains(&edge.id),
        (Some(_), AutomationPictureOwner::Automation) => pic.lines_back().contains(&edge.id),
    };
    RunDefLine {
        edge_id: edge.id,
        picture: edge.owner_kind,
        from_id: edge.from_id,
        exit_id: edge.exit_id,
        ends: edge.ends,
        placement_id,
        step_id,
        max_times: edge.max_times.filter(|_| goes_back),
    }
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
    pic: &Picture,
    placement: &AutomationPlacement,
    defs: &Defs,
    step_id: i64,
    port_id: i64,
) -> Vec<RunDefSource> {
    let mut out = Vec::new();
    for wire in &defs[&placement.id].wires {
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
        for outer in pic.wires_to_port(placement.id, wire.from_port_id) {
            let Some(far) = defs.get(&outer.from_id) else { continue };
            for inner in &far.wires {
                if inner.to_id != ACTION_BOUNDARY || inner.to_port_id != outer.from_port_id {
                    continue;
                }
                let Some(inner_exit) = inner.from_exit_id else { continue };
                let leaves_by = returns_to(far, inner.from_id, inner_exit);
                if leaves_by.is_some() && leaves_by == outer.from_exit_id {
                    out.push(RunDefSource {
                        placement_id: outer.from_id,
                        step_id: inner.from_id,
                        exit_id: inner.from_exit_id,
                        port_id: inner.from_port_id,
                    });
                }
            }
        }
    }
    out
}

/// **The copies a run reads from now** — the last time the automation was copied down onto it
/// ([`copy_down`]): from its last entry copy on.
///
/// A run carrying a single entry copy reads all of its copies. That is every run copied down once, and
/// it covers one launched before the entry's copy was written first.
pub(crate) fn current_defs(conn: &Connection, run_id: i64) -> Result<Vec<AutomationRunDef>> {
    let mut defs = read::automation_run_defs_of(conn, run_id)?;
    if defs.iter().filter(|def| def.entry).count() > 1 {
        if let Some(from) = defs.iter().rposition(|def| def.entry) {
            defs.drain(..from);
        }
    }
    Ok(defs)
}

/// The copy a run took of one step of one placement, the last time it was copied down.
fn copy_of(
    conn: &Connection,
    run_id: i64,
    placement_id: i64,
    step_id: i64,
) -> Result<Option<AutomationRunDef>> {
    Ok(current_defs(conn, run_id)?
        .into_iter()
        .find(|def| def.placement_id == Some(placement_id) && def.step_id == Some(step_id)))
}

/// **The spot a run starts at** — the copy marked as the step the automation's entry placement opened
/// first ([`AutomationRunDef::entry`]), the last time it was copied down, or `None` where the run
/// carries no such copy.
pub fn entry_def(conn: &Connection, run_id: i64) -> Result<Option<AutomationRunDef>> {
    if read::automation_run(conn, run_id)?.is_none() {
        return Err(not_found("run", run_id));
    }
    Ok(current_defs(conn, run_id)?.into_iter().find(|def| def.entry))
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
    if let Some(routed) = by_role(conn, def, &taken)? {
        return Ok(routed);
    }
    let Some(line) = taken.then else {
        // **An error way out nobody drew a line from halts the run**, and the round is filed as a failure
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

/// **Where the built-in that takes a task goes once it has taken one, in a run whose copies start roles'
/// rounds** — on to the copy the round's role starts at ([`AutomationRunDef::roles`]), whatever line is
/// drawn from it. `None` for any other way out, and for every way out of a run whose automation defines
/// no role that names where its round starts: that run walks its picture as it is drawn.
///
/// The role is the one the round was taken under ([`crate::model::AutomationRunTask::role_id`]). Only a
/// task of a role the copies start a round of is taken ([`roles_started`]), so a round with none has
/// nowhere to go ([`Onward::Nowhere`]) — which a task whose role was changed under the run as it was being
/// taken is the one way to reach.
///
/// No line is drawn for it, so the line it hands back is made up: on the automation's picture, from the
/// placement it leaves, with no edge (`edge_id` 0) and no limit on how often it is taken. That it is on the
/// automation's picture is what lets a pause asked for take hold on it, as on any line between two
/// placements.
fn by_role(conn: &Connection, def: &AutomationRunDef, taken: &RunDefExit) -> Result<Option<Onward>> {
    if def.builtin.as_deref() != Some(TAKE_TASK.key)
        || !taken.outs.iter().any(|p| p.kind == AutomationPortKind::TaskTake)
    {
        return Ok(None);
    }
    let defs = current_defs(conn, def.run_id)?;
    if defs.iter().all(|d| d.role_ids().is_empty()) {
        return Ok(None);
    }
    let role = read::automation_run_task_last(conn, def.run_id)?.and_then(|stretch| stretch.role_id);
    let Some(next) = role.and_then(|role| defs.into_iter().find(|d| d.role_ids().contains(&role))) else {
        return Ok(Some(Onward::Nowhere));
    };
    let line = RunDefLine {
        edge_id: 0,
        picture: AutomationPictureOwner::Automation,
        from_id: def.placement_id.unwrap_or_default(),
        exit_id: taken.returns_to.unwrap_or(taken.id),
        ends: AutomationEnds::Go,
        placement_id: next.placement_id,
        step_id: next.step_id,
        max_times: None,
    };
    Ok(Some(Onward::Go { def: Box::new(next), line }))
}

/// **The roles a run's copies start rounds of** ([`AutomationRunDef::roles`]), as it was last copied down,
/// each once. Empty for a run whose automation defines no role that names where its round starts.
///
/// Where there are any, the built-in that takes a task takes only a task of one of them, so a task with
/// no role, or one of a role no copy starts, is not taken.
pub(crate) fn roles_started(conn: &Connection, run_id: i64) -> Result<Vec<i64>> {
    let mut roles = Vec::new();
    for role in current_defs(conn, run_id)?.iter().flat_map(|d| d.role_ids()) {
        if !roles.contains(&role) {
            roles.push(role);
        }
    }
    Ok(roles)
}

/// **Which of an action's ways out one way out of a step inside it returns to** — the action's way out
/// the line drawn from it inside the action keys, where that line is an [`AutomationEnds::Exit`].
/// `None` where it leaves the action by no way out. Read off the version a placement stands on, which is
/// what a launch resolves its copies from; a run under way reads [`RunDefExit::returns_to`] instead.
fn returns_to(def: &ActionDef, step_id: i64, exit: i64) -> Option<i64> {
    def.edge_for_exit(step_id, exit)
        .filter(|edge| edge.ends == AutomationEnds::Exit)
        .and_then(|edge| edge.exit_to_id)
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
/// been launched and has no execution yet, or was copied down again as it was picked up and has none
/// from the new copy, where the answer is the entry ([`entry_def`]); and a run whose last execution has
/// reported, where the answer is read off the way out it took — the entry again where that way out
/// ended one round ([`goes_round`]).
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
    // A step opened from an earlier copy is behind a run copied down again as it was picked up
    // (`AMB-D-1015`), which starts afresh at the new copy's entry.
    if current_defs(conn, run_id)?.first().is_some_and(|first| last.run_def_id < first.id) {
        return Ok(entry_def(conn, run_id)?
            .map_or(Waiting::NoWayOn, |def| Waiting::Step(Box::new(def))));
    }
    // Everything below is a run standing between two steps. A way out that closed or halted the run
    // would have done so in the report that took it, so a run still `running` here is standing
    // towards the step the copy says comes next — or towards nothing, where the copy says nothing.
    let Some(from) = read::automation_run_def(conn, last.run_def_id)? else {
        return Ok(Waiting::NoWayOn);
    };
    Ok(match onward(conn, &from, last.exit_id)? {
        Onward::Go { def, .. } => Waiting::Step(def),
        // A round that came to the end of the picture is over, and the run goes round again at its
        // entry (`goes_round`) — the report that took the way out left it `running` for that.
        Onward::Done if goes_round(conn, run_id, &last)? => {
            entry_def(conn, run_id)?.map_or(Waiting::NoWayOn, |def| Waiting::Step(Box::new(def)))
        }
        Onward::Done | Onward::Halt | Onward::Nowhere => Waiting::NoWayOn,
    })
}

/// **Whether the end of the picture is the end of one round rather than of the run** — one task a
/// round, each round walking the picture from its entry to its end.
///
/// It is, where the run's entry, as its newest copy marks it, is the built-in that takes a task, and
/// the step that came to the end belongs to a stretch that held one ([`AutomationRunTask`]): the entry
/// then takes the next task, and the round after starts there. **A round never goes without a task.**
/// A step that went looking for one and found none keeps no stretch, so a run whose entry finds nothing
/// to take comes to its end; and a run whose entry files a task, or is anything else, walks the picture
/// once — what it was handed at launch is one task's (`AMB-D-971`).
///
/// **Only a round whose task was closed goes round.** A task put off or handed back is `todo` again,
/// and the entry could take the very same one straight back, round after round; so a round that let
/// its task go without closing it ends the run at the end of the picture, as before.
///
/// [`AutomationRunTask`]: crate::model::AutomationRunTask
pub(crate) fn goes_round(
    conn: &Connection,
    run_id: i64,
    ended: &crate::model::AutomationRunStep,
) -> Result<bool> {
    let Some(stretch) = ended.run_task_id else { return Ok(false) };
    let Some(task_id) = read::automation_run_task(conn, stretch)?.and_then(|s| s.task_id) else {
        return Ok(false);
    };
    if !read::task_status(conn, task_id)?.is_some_and(|s| s.is_closed()) {
        return Ok(false);
    }
    Ok(entry_def(conn, run_id)?.is_some_and(|entry| crate::ops::automation_stop::takes_a_task(&entry)))
}

/// **Whether this run stands before a built-in that would wait** for something to turn up
/// (`AMB-D-969`) — `running`, with nothing under way, and the step it would open next set to wait with
/// nothing yet for it. Such a run is ended the next time that step is opened, and its automation waits
/// instead ([`wait_for_a_task`]); until then a pause before its next task takes hold at once.
pub fn is_waiting(conn: &Connection, run_id: i64) -> Result<bool> {
    let Waiting::Step(def) = next_def(conn, run_id)? else { return Ok(false) };
    let Some(run) = read::automation_run(conn, run_id)? else { return Ok(false) };
    crate::ops::automation_builtin::waiting(conn, &run, &def)
}

/// **What is keeping a run waiting** (`AMB-D-999`) — `None` for a run that is not standing before a
/// built-in that would wait ([`is_waiting`]).
pub fn held_back(conn: &Connection, run_id: i64) -> Result<Option<crate::ops::automation_builtin::HeldBack>> {
    let Waiting::Step(def) = next_def(conn, run_id)? else { return Ok(None) };
    let Some(run) = read::automation_run(conn, run_id)? else { return Ok(None) };
    if !crate::ops::automation_builtin::waiting(conn, &run, &def)? {
        return Ok(None);
    }
    let cfg: Vec<RunDefCfg> = serde_json::from_str(&def.cfg).map_err(Error::from)?;
    let roles = roles_started(conn, run.id)?;
    crate::ops::automation_builtin::held_back(conn, run.project_id, &roles, def.builtin.as_deref(), &cfg)
}

/// **A run kept out of an action that takes one run at a time** ([`waits_for_room`]): the action, and
/// the run it waits behind — the one inside it, or, where none is, the one that came to it first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ForRoom {
    pub action_id: i64,
    pub behind: i64,
}

/// **Whether this run stands before an action that takes one run at a time, waiting for room** — and
/// whom it waits behind. `None` for a run that is not `running`, that has a step under way, or that
/// stands before anything else; and for one waiting on a built-in first ([`is_waiting`]), which is
/// that wait and not this one. Asked by a face that says so beside the run.
pub fn waits_for_room(conn: &Connection, run_id: i64) -> Result<Option<ForRoom>> {
    let Waiting::Step(def) = next_def(conn, run_id)? else { return Ok(None) };
    let Some(run) = read::automation_run(conn, run_id)? else { return Ok(None) };
    if crate::ops::automation_builtin::waiting(conn, &run, &def)? {
        return Ok(None);
    }
    room_for(conn, &run, &def)
}

/// **Whether opening `def` would put `run` inside an action somebody else is in, or ahead of a run
/// that came to it first** — answered as the run whom it waits behind, or `None` where it goes in now.
///
/// Read off the run's own copy (`AMB-D-961`): only a copy that says its action takes one run at a
/// time waits, and only on its way into that action — a step after another step of the same spot is
/// already inside. Nothing is remembered for it. **Who is inside** is read off each other run's last
/// step: one under way in the action, or one that left by a line that stays inside it. A run paused
/// there still holds the action, since picking it up goes on inside; one paused before its next task
/// does not, since picking it up starts afresh at its entry. Whether that other run's copy takes one
/// run at a time does not matter: it is in the same action.
///
/// **Arrival order** is when each run came to stand before the action ([`arrived`]). A run waiting on
/// a built-in first ([`crate::ops::automation_builtin::waiting`]) is not in the line: it is not ready
/// to go in, and holding the others back for it would leave the action empty. Ties go to the lower
/// run id.
pub(crate) fn room_for(conn: &Connection, run: &AutomationRun, def: &AutomationRunDef) -> Result<Option<ForRoom>> {
    let (true, Some(action_id)) = (def.one_at_a_time, def.action_id) else { return Ok(None) };
    if action_held_by(conn, run.id)?.is_some() {
        return Ok(None);
    }
    let mine = (arrived(conn, run)?, run.id);
    let mut ahead: Option<(Timestamp, i64)> = None;
    for other in read::automation_runs_live(conn)? {
        if other.id == run.id {
            continue;
        }
        let holds = match other.status {
            AutomationRunStatus::Running => true,
            AutomationRunStatus::Paused => {
                other.pause_kind != Some(crate::model::AutomationPauseKind::BeforeNextTask)
            }
            _ => false,
        };
        if !holds {
            continue;
        }
        if let Some(at) = action_held_by(conn, other.id)? {
            if at.action_id == Some(action_id) {
                return Ok(Some(ForRoom { action_id, behind: other.id }));
            }
            continue;
        }
        if other.status != AutomationRunStatus::Running {
            continue;
        }
        let Waiting::Step(theirs) = next_def(conn, other.id)? else { continue };
        if !theirs.one_at_a_time || theirs.action_id != Some(action_id) {
            continue;
        }
        if crate::ops::automation_builtin::waiting(conn, &other, &theirs)? {
            continue;
        }
        let place = (arrived(conn, &other)?, other.id);
        if place < mine && ahead.is_none_or(|first| place < first) {
            ahead = Some(place);
        }
    }
    Ok(ahead.map(|(_, behind)| ForRoom { action_id, behind }))
}

/// **The copy of the step whose action this run is inside**, or `None` where it stands between two
/// actions. Inside is its last step still under way, or left by a line that stays inside the action
/// — the run is going on to another of the action's steps. A step from before the run was copied down
/// again is behind it: the run starts afresh at its entry.
fn action_held_by(conn: &Connection, run_id: i64) -> Result<Option<AutomationRunDef>> {
    let Some(last) = read::automation_run_steps_of(conn, run_id)?.pop() else { return Ok(None) };
    if current_defs(conn, run_id)?.first().is_some_and(|first| last.run_def_id < first.id) {
        return Ok(None);
    }
    let Some(def) = read::automation_run_def(conn, last.run_def_id)? else { return Ok(None) };
    let stays = match last.status {
        AutomationRunStepStatus::Running => true,
        _ => matches!(
            onward(conn, &def, last.exit_id)?,
            Onward::Go { line, .. } if line.picture == AutomationPictureOwner::Action
        ),
    };
    Ok(stays.then_some(def))
}

/// **When a run came to stand where it stands now**: when its last step ended, or, with none since
/// it was last copied down, when that copy was taken — the launch, or the pick-up after a pause before
/// its next task. A pause at the end of an action moves nothing, so a run picked up from one keeps the
/// place it had.
fn arrived(conn: &Connection, run: &AutomationRun) -> Result<Timestamp> {
    let defs = current_defs(conn, run.id)?;
    let last = read::automation_run_steps_of(conn, run.id)?.pop();
    Ok(match (last, defs.first()) {
        (Some(last), first) if first.is_none_or(|first| last.run_def_id >= first.id) => {
            last.ended_at.unwrap_or(last.updated_at)
        }
        (_, Some(first)) => first.created_at,
        (_, None) => run.started_at.unwrap_or(run.created_at),
    })
}

/// **The way a run has come on the task it is working** — the spots of the automation's picture it
/// passed, in the order it passed them, the spot it stands at now, and the steps it passed inside
/// that spot and the one it stands at ([`trail`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Trail {
    pub passed: Vec<Passed>,
    /// The placement whose step is under way, or the one the run is about to open or waiting on.
    /// `None` for a run that is not `running`, and where the copy names no spot.
    pub at: Option<i64>,
    /// The steps the run passed inside the spot it is in, in the order it passed them — empty once it
    /// has left that spot, and before it opens one there. A step passed twice is two of these.
    pub steps: Vec<PassedStep>,
    /// The step under way, or the one the run is about to open or waiting on — the first step of the
    /// spot for a run that has not opened one there yet. `None` where [`Trail::at`] is `None`, and where the copy
    /// names no step.
    pub step: Option<i64>,
}

/// **One pass through one step inside a spot**, keyed the way the action's picture keys its boxes
/// and lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PassedStep {
    /// `None` where the run's copy names no step.
    pub step_id: Option<i64>,
    pub status: AutomationRunStepStatus,
    /// The line on the action's picture the run walked from there to the next step. `None` while the
    /// step is under way, and where the run left the spot from it.
    pub edge_id: Option<i64>,
}

/// **One pass through one spot.** A spot passed twice — sent back to it by a review — is two of these,
/// in the order the run came to them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Passed {
    /// `None` where the run's copy names no spot.
    pub placement_id: Option<i64>,
    /// How the last step the run opened there stands.
    pub status: AutomationRunStepStatus,
    /// The action's way out the run left the spot by — the id the automation's picture keys its lines
    /// by. `None` while the run is still inside the spot, and where it left by no way out of the action.
    pub exit_id: Option<i64>,
    /// The line on the automation's picture the run walked from there — one going on to another spot,
    /// or one ending the run. `None` while the run is still inside the spot, and where nothing was drawn
    /// after the way out it left by.
    pub edge_id: Option<i64>,
}

/// **The spots a run has passed on the task it is working, and where it is now** — what the picture
/// on a run's pane lights up.
///
/// Only the stretch the run is in counts ([`read::automation_run_task_last`]): the step that takes the
/// next task opens a new one, so what lights up starts again with each task. The steps inside one
/// spot fold into one pass, so the picture — which draws spots, not steps — reads one box per pass;
/// the steps of the spot the run is in are kept beside them, for the action's picture inside it.
/// Read off the run's copies, the same as the run goes on by them (`AMB-D-961`).
pub fn trail(conn: &Connection, run_id: i64) -> Result<Trail> {
    if read::automation_run(conn, run_id)?.is_none() {
        return Err(not_found("run", run_id));
    }
    let defs: BTreeMap<i64, AutomationRunDef> =
        read::automation_run_defs_of(conn, run_id)?.into_iter().map(|def| (def.id, def)).collect();
    let mut passed: Vec<Passed> = Vec::new();
    let mut steps: Vec<PassedStep> = Vec::new();
    // Whether the run is still in the spot of the last pass — its step is under way, or went on to
    // another of the action's.
    let mut open = false;
    if let Some(stretch) = read::automation_run_task_last(conn, run_id)? {
        // Whether the last pass is still inside its spot — its step went on to another of the action's.
        let mut inside = false;
        for step in read::automation_run_steps_of_task(conn, stretch.id)? {
            let Some(def) = defs.get(&step.run_def_id) else { continue };
            let taken = match step.exit_id {
                Some(id) => {
                    let exits: Vec<RunDefExit> = serde_json::from_str(&def.exits).map_err(Error::from)?;
                    exits.into_iter().find(|exit| exit.id == id)
                }
                None => None,
            };
            let then = taken.as_ref().and_then(|exit| exit.then.as_ref());
            let stays = then.is_some_and(|line| {
                line.picture == AutomationPictureOwner::Action && line.ends == AutomationEnds::Go
            });
            let pass = Passed {
                placement_id: def.placement_id,
                status: step.status,
                exit_id: taken.as_ref().filter(|_| !stays).and_then(|exit| exit.returns_to),
                edge_id: then
                    .filter(|line| line.picture == AutomationPictureOwner::Automation)
                    .map(|line| line.edge_id),
            };
            let walked = PassedStep {
                step_id: def.step_id,
                status: step.status,
                edge_id: then.filter(|_| stays).map(|line| line.edge_id),
            };
            match passed.last_mut() {
                Some(last) if inside && last.placement_id == pass.placement_id => *last = pass,
                _ => {
                    passed.push(pass);
                    steps.clear();
                }
            }
            steps.push(walked);
            inside = stays;
            open = stays || step.status == AutomationRunStepStatus::Running;
        }
    }
    if !open {
        steps.clear();
    }
    let (at, step) = match read::automation_run_steps_of(conn, run_id)?.pop() {
        Some(last) if last.status == AutomationRunStepStatus::Running => defs
            .get(&last.run_def_id)
            .map_or((None, None), |def| (def.placement_id, def.step_id)),
        _ => match next_def(conn, run_id)? {
            Waiting::Step(def) => (def.placement_id, def.step_id),
            Waiting::Nothing | Waiting::NoWayOn => (None, None),
        },
    };
    Ok(Trail { passed, at, steps, step })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AutomationAction, AutomationEdge, AutomationPlacement};
    use crate::ops::automation::{self, EdgeTarget, NewAutomation, NewRole, NewScript, NewStep};
    use crate::ops::automation_builtin_take::{NONE_TO_TAKE, TAKEN};
    use crate::ops::test_support::{exit_id, mk_exit, mk_in, mk_out, mk_placed, mk_project, only_step, with_tx};

    fn mk_automation(tx: &WriteTx<'_>, name: &str) -> Automation {
        let project = mk_project(tx, "amenbo");
        automation::add(tx, project, NewAutomation { name: name.into(), failures_to: Some(crate::model::FailuresTo::Human), ..Default::default() })
            .expect("add automation")
    }

    /// What every test here starts from: one automation whose entry is the built-in that takes a task,
    /// one action of one step that works on it, the built-in that closes it, and the end of the run —
    /// every way out decided. It launches as it stands, so each test can take one thing back off and
    /// watch the check find it. What is handed back is the action that works, and its placement.
    fn launchable(tx: &WriteTx<'_>) -> (Automation, AutomationAction, AutomationPlacement) {
        let automation = mk_automation(tx, "1件やりきる");
        let taking = takes_first(tx, &automation);
        let (action, placement) = mk_placed(tx, &automation, "直す", "fix it", "claude");
        automation::edge_add(
            tx,
            AutomationPictureOwner::Automation,
            taking.id,
            Some(TAKEN),
            EdgeTarget::Go(placement.id),
            None,
        )
        .expect("take → work");
        crate::ops::test_support::mk_closed_after(tx, &automation, placement.id, None);
        // Nothing is written for the error way out: it is carried from birth and halts unless
        // somebody says otherwise, which is what `an_error_way_out_nobody_answered_for_is_not_open`
        // holds this to.
        let automation =
            automation::set_entry(tx, automation.id, Some(taking.id)).expect("entry");
        (automation, action, placement)
    }

    /// **The built-in that takes a task, placed on `automation`** — the only thing that takes one
    /// (`AMB-D-964`). With nothing to take, the run ends; what follows a task taken is the caller's to
    /// draw.
    fn takes_first(tx: &WriteTx<'_>, automation: &Automation) -> AutomationPlacement {
        let take = crate::ops::automation_builtin::action(tx, "take_task").expect("the built-in's action");
        let taking = automation::placement_add(tx, automation.id, take.id).expect("place it");
        automation::edge_add(
            tx,
            AutomationPictureOwner::Automation,
            taking.id,
            Some(NONE_TO_TAKE),
            EdgeTarget::Done,
            None,
        )
        .expect("nothing to take ends the run");
        taking
    }

    /// Declare a `task_take` output on one way out of a person's own action — what a store written
    /// before that was refused can hold (`AMB-D-964`), past the refusal the way the tests of the run
    /// side draw one ([`mk_out`]).
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
            default_agent: None,
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::NoEntry],
                "with no entry nothing is reachable, so every other check is asked of nothing",
            );
        });
    }

    /// **Nobody chosen to file a failed round to is refused** — by the check, by a launch and by a save —
    /// and said beside the two reasons that otherwise answer alone. Choosing a person, or one of the
    /// automation's own roles, is what lets it go.
    #[test]
    fn an_automation_with_nobody_to_file_a_failed_round_to_is_refused() {
        use crate::model::FailuresTo;
        with_tx(|tx| {
            let project = mk_project(tx, "amenbo");
            let empty = automation::add(tx, project, NewAutomation { name: "空".into(), ..Default::default() })
                .expect("add automation");
            assert_eq!(empty.failures_to, None, "born with nobody unless somebody is named");
            assert_eq!(
                check(tx.conn(), empty.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::NoFailuresTo, Unmet::NoSteps],
            );

            let (automation, _, _) = launchable(tx);
            automation::update(tx, automation.id, None, None, None, None, Some(None)).expect("leave it unsaid");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::NoFailuresTo],
            );
            let refused = launch(tx, automation.id, &here(&claude())).expect_err("nobody to file to");
            assert_eq!(refused.error_code(), ErrorCode::NotReadyAutomation, "{refused}");
            assert!(refused.to_string().contains("failed round"), "{refused}");
            let refused = automation::save(tx, automation.id, None).expect_err("a save asks the same");
            assert!(matches!(refused, Error::NotReady(_)), "{refused}");
            assert_eq!(Unmet::NoFailuresTo.placement(), None);
            assert_eq!(Unmet::NoFailuresTo.code(), ErrorCode::NotReadyAutomationNoFailuresTo);

            let role = automation::role_add(
                tx,
                automation.id,
                automation::NewRole { name: "見る".into(), ..Default::default() },
            )
            .expect("a role");
            let to_role = Some(Some(FailuresTo::Role { role_id: role.id }));
            automation::update(tx, automation.id, None, None, None, None, to_role).expect("to the role");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"), vec![]);
            launch(tx, automation.id, &here(&claude())).expect("launch");
        });
    }

    /// **An automation kept only to be written out is refused**, with that one reason and no other, by
    /// the check and by a launch — however ready its picture is.
    #[test]
    fn an_automation_kept_only_to_be_written_out_is_refused() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let old = crate::model::Automation { export_only: true, failures_to: None, ..automation.clone() };
            crate::ops::emit_update(
                tx,
                crate::store_engine::record::automation(&automation),
                crate::store_engine::record::automation(&old),
            )
            .expect("written before automations had roles");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::ExportOnly],
                "nothing else is worth saying about it",
            );
            let refused = launch(tx, automation.id, &here(&claude())).expect_err("kept only to be written out");
            assert_eq!(refused.error_code(), ErrorCode::NotReadyAutomation, "{refused}");
            assert!(refused.to_string().contains("automation export"), "{refused}");
            assert_eq!(Unmet::ExportOnly.placement(), None);
            assert_eq!(Unmet::ExportOnly.code(), ErrorCode::NotReadyAutomationExportOnly);
        });
    }

    /// **An action still being written is refused** (`AMB-D-1005`), and saving it, which finishes it, is
    /// what lets the automation go.
    #[test]
    fn a_placement_standing_on_an_action_still_being_written_is_refused_until_it_is_finished() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let before = crate::store_engine::read::automation_action(tx.conn(), action.id)
                .expect("read it")
                .expect("there");
            let draft = crate::model::AutomationAction { draft: true, ..before.clone() };
            crate::ops::emit_update(
                tx,
                crate::store_engine::record::automation_action(&before),
                crate::store_engine::record::automation_action(&draft),
            )
            .expect("mark it as still being written");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::ActionDraft { action: "直す".into(), placement: placement.id }],
            );

            automation::action_save(tx, action.id).expect("save it");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![],
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::ActionEmpty { action: "直す".into(), placement: Some(placement.id) }],
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::EntryTakesNoTask { step: "取る".into(), builtin: None, placement: placement.id }],
            );
        });
    }

    /// A task is taken by the built-in and worked by the next placement; what follows the work is each
    /// test's to draw.
    fn taken_then_worked(tx: &WriteTx<'_>) -> (Automation, AutomationPlacement, AutomationAction, AutomationPlacement) {
        let automation = mk_automation(tx, "閉じ忘れ");
        let taking = takes_first(tx, &automation);
        let (work, working) = mk_placed(tx, &automation, "直す", "fix it", "claude");
        let on = AutomationPictureOwner::Automation;
        automation::edge_add(tx, on, taking.id, Some(TAKEN), EdgeTarget::Go(working.id), None).expect("take → work");
        let automation = automation::set_entry(tx, automation.id, Some(taking.id)).expect("entry");
        (automation, taking, work, working)
    }

    fn checked(tx: &WriteTx<'_>, automation: &Automation) -> Vec<Unmet> {
        check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check")
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
                    inside: None,
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
            let take = read::automation_action(tx.conn(), taking.action_id).expect("read").expect("the built-in's action");
            assert_eq!(
                checked(tx, &automation),
                vec![Unmet::LeavesTaskOpen {
                    step: "直す".into(),
                    exit: crate::model::DONE_EXIT.into(),
                    to: Some(take.name),
                    builtin: None,
                    to_builtin: Some("take_task".into()),
                    placement: working.id,
                    inside: None,
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

    /// **A line back to the entry that files a task is refused**, even closed on the way: it is handed
    /// its title only when a run starts, so coming back to it the run would fail there on no input.
    #[test]
    fn a_line_back_to_the_entry_that_files_a_task_is_refused() {
        with_tx(|tx| {
            use crate::ops::automation_builtin_make::{MADE_AND_TAKEN, TAKE_IT, WHAT_THEN};
            let automation = mk_automation(tx, "起票してやりきる");
            let make = crate::ops::automation_builtin::action(tx, "make_task").expect("the built-in's action");
            let making = automation::placement_add(tx, automation.id, make.id).expect("place it");
            automation::cfg_set(tx, making.id, WHAT_THEN, Some(&serde_json::to_string(TAKE_IT).expect("json")))
                .expect("take it");
            let (_, working) = mk_placed(tx, &automation, "直す", "fix it", "claude");
            let on = AutomationPictureOwner::Automation;
            automation::edge_add(tx, on, making.id, Some(MADE_AND_TAKEN), EdgeTarget::Go(working.id), None)
                .expect("file → work");
            let close = crate::ops::test_support::mk_closed_after(tx, &automation, working.id, None);
            let automation = automation::set_entry(tx, automation.id, Some(making.id)).expect("entry");
            assert_eq!(checked(tx, &automation), vec![]);

            let closed = read::automation_edge_for_exit(
                tx.conn(),
                on,
                close.id,
                exit_id(tx, AutomationOwner::Action, close.action_id, None),
            )
            .expect("read")
            .expect("the close's line");
            automation::edge_delete(tx, closed.id).expect("unhook the end");
            automation::edge_add(tx, on, close.id, None, EdgeTarget::Go(making.id), None).expect("close → file");
            let closing = read::automation_action(tx.conn(), close.action_id).expect("read").expect("the close");
            assert_eq!(
                checked(tx, &automation),
                vec![Unmet::BackToEntry {
                    step: closing.name,
                    exit: crate::model::DONE_EXIT.into(),
                    to: make.name,
                    builtin: Some("close_task".into()),
                    to_builtin: Some("make_task".into()),
                    placement: close.id,
                }],
            );
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

    /// **A person's own action handing on the task a run works is refused** (`AMB-D-964`), named by the
    /// way out that declares it. Only a built-in takes that task, and declaring one is refused while
    /// building — a store written before that can still hold one, and a run would stand still at it.
    ///
    /// The step inside declares it too, under the same name, and that is said once.
    #[test]
    fn a_way_out_of_a_person_s_own_action_handing_on_the_task_is_refused() {
        with_tx(|tx| {
            let automation = mk_automation(tx, "自分で取る");
            let (action, placement) = mk_placed(tx, &automation, "取る", "take one", "claude");
            takes_task_on(tx, &action, None);
            crate::ops::test_support::mk_closed_after(tx, &automation, placement.id, None);
            let automation = automation::set_entry(tx, automation.id, Some(placement.id)).expect("entry");
            let refused = Unmet::HandsOnTaskTaken {
                step: "取る".into(),
                exit: crate::model::DONE_EXIT.into(),
                placement: placement.id,
            };
            assert_eq!(checked(tx, &automation), vec![refused.clone()]);
            assert!(refused.say().contains("take_task"), "{}", refused.say());

            let err = launch(tx, automation.id, &here(&claude())).expect_err("refused");
            let Error::NotReady(msg) = err else { panic!("a launch that cannot go ahead is not_ready") };
            assert_eq!(
                msg.parts().iter().map(|p| p.code()).collect::<Vec<_>>(),
                vec![Some(ErrorCode::NotReadyAutomationHandsOnTaskTaken)],
            );
            let named: Vec<(&str, &str)> = msg.parts()[0].fields().iter().collect();
            let id = placement.id.to_string();
            assert_eq!(named, vec![("placement", id.as_str()), ("step", "取る"), ("exit", crate::model::DONE_EXIT)]);
        });
    }

    /// **A step inside the action handing it on is refused on its own**, where the action's way out
    /// declares nothing — the step is what could never leave by it.
    #[test]
    fn a_step_inside_a_person_s_own_action_handing_on_the_task_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            let exit = exit_id(tx, AutomationOwner::Step, step.id, None);
            crate::ops::automation::declare_port(
                tx,
                AutomationPortOwner::Exit,
                exit,
                AutomationPortDirection::Out,
                "タスク",
                AutomationPortKind::TaskTake,
                true,
            )
            .expect("declared past the refusal");
            assert_eq!(
                checked(tx, &automation),
                vec![Unmet::HandsOnTaskTaken {
                    step: "直す".into(),
                    exit: crate::model::DONE_EXIT.into(),
                    placement: placement.id,
                }],
            );
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::OpenExit {
                    step: "直す".into(),
                    exit: exit.name.clone(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: None,
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check");
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "直す".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: None,
                }],
            );
        });
    }

    /// **A wire from the placement's own way out does not feed its input** (`AMB-T-5641`): the first
    /// time a run arrives nothing has left by it yet, so the input is empty and the run fails there on
    /// no_input. Reached from the entry is not enough — the far end has to be reached before this one.
    #[test]
    fn a_wire_from_the_placements_own_way_out_does_not_feed_its_input() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let exit = read::automation_exit_by_name(tx.conn(), AutomationOwner::Action, action.id, None)
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
                action.id,
                AutomationPortDirection::In,
                "下書き",
                AutomationPortKind::Value,
                true,
            )
            .expect("in");
            automation::wire_add(
                tx,
                AutomationPictureOwner::Automation,
                placement.id,
                None,
                "下書き",
                placement.id,
                "下書き",
            )
            .expect("wire back into itself");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "直す".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: None,
                }],
                "the only wire into it comes back from its own way out",
            );
        });
    }

    #[test]
    fn a_wire_from_a_placement_no_run_reaches_does_not_feed_an_input() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            // A second placement, wired into the worker's input but reached by nothing: the run would
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
                action.id,
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
                placement.id,
                "下書き",
            )
            .expect("wire");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "直す".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: None,
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::UnansweredCfg {
                    step: "直す".into(),
                    cfg: "作業フォルダ".into(),
                    builtin: None,
                    placement: placement.id,
                }],
                "the declaration is the action's and the answer is the placement's",
            );
            automation::cfg_set(tx, placement.id, "作業フォルダ", Some("\"~/work\"")).expect("answer");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"), vec![]);
        });
    }

    /// **An answer its setting no longer takes is refused at the launch** — here a choice taken off the
    /// list after it was answered, which the answer's own check could not see coming.
    #[test]
    fn an_answer_the_setting_does_not_take_is_refused_at_launch() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let cfg = automation::cfg_add(
                tx,
                action.id,
                "どれ",
                crate::model::AutomationCfgKind::Choice,
                false,
                Some(r#"["a","b"]"#),
            )
            .expect("cfg");
            automation::cfg_set(tx, placement.id, "どれ", Some(r#""b""#)).expect("answer");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"), vec![]);
            automation::cfg_update(tx, cfg.id, None, None, None, Some(Some(r#"["a"]"#))).expect("take b off");
            let unmet = check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check");
            assert!(
                matches!(
                    unmet.as_slice(),
                    [Unmet::MisansweredCfg { step, cfg, placement: p, .. }]
                        if step == "直す" && cfg == "どれ" && *p == placement.id
                ),
                "{unmet:?}",
            );
        });
    }

    #[test]
    fn an_agent_this_machine_cannot_start_is_refused_and_an_unasked_machine_is_not() {
        with_tx(|tx| {
            let (automation, _, placement) = launchable(tx);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&[]), nothing_asked(), None).expect("check"),
                vec![Unmet::AgentMissing { step: "直す".into(), agent: "claude".into(), placement: placement.id }],
            );
            assert_eq!(
                check(tx.conn(), automation.id, None, nothing_asked(), None).expect("check"),
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

    /// **A step none of the three chooses for is refused** — not at its placement, not by a role, and
    /// with no default of Amenbo's — even on a machine nobody has asked what it can start: there is no
    /// agent to ask about at all.
    #[test]
    fn a_step_nobody_is_chosen_for_where_it_is_placed_is_refused() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            automation::placement_step_clear(tx, placement.id, step.id).expect("take the choice back");
            assert_eq!(
                check(tx.conn(), automation.id, None, nothing_asked(), None).expect("check"),
                vec![Unmet::AgentUnchosen { step: "直す".into(), placement: placement.id }],
            );
            let err = launch(tx, automation.id, &here(&claude())).expect_err("refused");
            let Error::NotReady(msg) = err else { panic!("a launch that cannot go ahead is not_ready") };
            assert_eq!(
                msg.parts().iter().map(|p| p.code()).collect::<Vec<_>>(),
                vec![Some(ErrorCode::NotReadyAutomationAgentUnchosen)],
            );
        });
    }

    /// Who carries `step` at `placement` in the draft, with `default_agent` as Amenbo's own.
    fn carried(
        tx: &WriteTx<'_>,
        automation: &Automation,
        placement: &AutomationPlacement,
        step: &AutomationStep,
        default_agent: Option<&str>,
    ) -> Option<Carrier> {
        carriers(tx.conn(), automation.id, ReadsFrom::Draft, default_agent)
            .expect("carriers")
            .remove(&(placement.id, step.id))
    }

    /// A role named `name` whose round starts at `entry`, falling back to `agent` and `model`.
    fn role_at(
        tx: &WriteTx<'_>,
        automation: &Automation,
        name: &str,
        entry: i64,
        agent: Option<&str>,
        model: Option<&str>,
    ) -> crate::model::AutomationRole {
        let new = NewRole {
            name: name.into(),
            entry_placement_id: Some(entry),
            agent: agent.map(str::to_string),
            model: model.map(str::to_string),
            ..NewRole::default()
        };
        automation::role_add(tx, automation.id, new).expect("a role")
    }

    /// **A step's agent is chosen from three, in order** — the step at its placement, then the role whose
    /// round reaches the placement, then Amenbo's own default — and the model comes from wherever the
    /// agent did: Amenbo's names none, and a step's own choice never takes its role's model.
    #[test]
    fn a_step_falls_back_to_its_role_and_then_to_amenbo() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            automation::placement_step_clear(tx, placement.id, step.id).expect("take the choice back");

            assert_eq!(carried(tx, &automation, &placement, &step, None), None, "none of the three");
            assert_eq!(
                carried(tx, &automation, &placement, &step, Some("codex")),
                Some(Carrier { agent: "codex".into(), model: None, at: ChosenAt::Amenbo }),
            );
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), Some("claude")).expect("check"),
                vec![],
                "Amenbo's default carries a step nobody chose for",
            );

            let role = role_at(tx, &automation, "実装", placement.id, Some("claude"), Some("opus"));
            assert_eq!(
                carried(tx, &automation, &placement, &step, Some("codex")),
                Some(Carrier { agent: "claude".into(), model: Some("opus".into()), at: ChosenAt::Role { role_id: role.id } }),
                "the role comes before Amenbo, with its model",
            );
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"), vec![]);

            automation::placement_step_set(tx, placement.id, step.id, "gemini", None).expect("choose");
            assert_eq!(
                carried(tx, &automation, &placement, &step, Some("codex")),
                Some(Carrier { agent: "gemini".into(), model: None, at: ChosenAt::Step }),
                "the step's own choice comes first, and the role's model does not come with it",
            );
        });
    }

    /// **A placement more than one round reaches takes the first role defined** that names an agent, and
    /// a round stops where the automation's entry or another role's round begins.
    #[test]
    fn a_placement_falls_back_to_the_first_role_whose_round_reaches_it() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            automation::placement_step_clear(tx, placement.id, step.id).expect("take the choice back");
            let entry = automation.entry_placement_id.expect("an entry");

            // Defined first, and starting at the automation's entry — but the work placement is where
            // another role's round begins, so this round stops short of it.
            role_at(tx, &automation, "受付", entry, Some("codex"), None);
            role_at(tx, &automation, "名無し", placement.id, None, None);
            let first = role_at(tx, &automation, "実装", placement.id, Some("claude"), None);
            role_at(tx, &automation, "予備", placement.id, Some("gemini"), None);

            assert_eq!(
                carried(tx, &automation, &placement, &step, None),
                Some(Carrier { agent: "claude".into(), model: None, at: ChosenAt::Role { role_id: first.id } }),
            );
        });
    }

    /// **The round reaching a placement is the first role defined whose round does**, a role naming no
    /// agent included — unlike the role a step falls back to, which passes such a role by.
    #[test]
    fn a_placement_is_in_the_round_of_the_first_role_reaching_it_with_or_without_an_agent() {
        with_tx(|tx| {
            let (automation, _, placement) = launchable(tx);
            let entry = automation.entry_placement_id.expect("an entry");
            let rounds = || rounds_by_placement(tx.conn(), automation.id, ReadsFrom::Draft).expect("rounds");
            assert_eq!(rounds(), BTreeMap::new(), "no role, no round");

            let reception = role_at(tx, &automation, "受付", entry, Some("codex"), None);
            let unnamed = role_at(tx, &automation, "名無し", placement.id, None, None);
            role_at(tx, &automation, "実装", placement.id, Some("claude"), None);

            let rounds = rounds();
            assert_eq!(rounds.get(&entry), Some(&reception.id));
            assert_eq!(rounds.get(&placement.id), Some(&unnamed.id), "the role defined first, though it names no agent");
        });
    }

    /// **A launch copies the agent a step falls back to** into the run, so the pane opens on it.
    #[test]
    fn a_launch_copies_the_agent_a_step_falls_back_to() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            automation::placement_step_clear(tx, placement.id, step.id).expect("take the choice back");
            let startable = claude();
            let by = Launcher { default_agent: Some("claude"), ..here(&startable) };
            let run = launch(tx, automation.id, &by).expect("launch");
            let copy = read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|def| def.step_id == Some(step.id))
                .expect("the step is copied");
            assert_eq!((copy.agent.as_str(), copy.model.as_deref()), ("claude", None));
        });
    }

    /// Turn the one step `action` holds into a script that runs `program` (`AMB-D-1016`).
    fn runs_a_script(tx: &WriteTx<'_>, action: &AutomationAction, program: &std::path::Path) {
        let step = only_step(tx, action);
        let script =
            NewScript { program: program.to_string_lossy().into_owned(), args: Vec::new(), timeout_minutes: None };
        let script = Some(Some(script));
        automation::step_update(tx, step.id, None, None, None, None, None, None, None, None, None, script, None)
            .expect("make it a script");
    }

    /// A folder of its own under the system's temporary one, for the program a test points a script at.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("amenbo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("make the folder");
        dir
    }

    /// **A script step is asked whether its program is there, and not who carries it out**
    /// (`AMB-D-1016`): nobody is chosen for it, and only a path with no file is refused — a folder
    /// standing there included.
    #[test]
    fn a_script_whose_program_is_not_there_is_refused_and_no_agent_is_asked_for() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let dir = scratch("script-missing");
            let program = dir.join("check.sh");
            runs_a_script(tx, &action, &program);
            automation::placement_step_clear(tx, placement.id, only_step(tx, &action).id).expect("nobody chosen");
            let missing = vec![Unmet::ScriptMissing {
                step: "直す".into(),
                program: program.to_string_lossy().into_owned(),
                placement: placement.id,
            }];
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"), missing);
            std::fs::create_dir(&program).expect("a folder where the program should be");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"), missing);
            let err = launch(tx, automation.id, &here(&claude())).expect_err("refused");
            let Error::NotReady(msg) = err else { panic!("a launch that cannot go ahead is not_ready") };
            assert_eq!(
                msg.parts().iter().map(|p| p.code()).collect::<Vec<_>>(),
                vec![Some(ErrorCode::NotReadyAutomationScriptMissing)],
            );
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// **A program this machine will not run is refused, and one it will launches** — on macOS and
    /// Linux, a file with no permission to execute.
    #[cfg(unix)]
    #[test]
    fn a_script_whose_program_is_not_executable_is_refused_until_it_is() {
        use std::os::unix::fs::PermissionsExt;
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let dir = scratch("script-not-executable");
            let program = dir.join("check.sh");
            std::fs::write(&program, "#!/bin/sh\n").expect("write the program");
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644)).expect("not executable");
            runs_a_script(tx, &action, &program);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::ScriptNotExecutable {
                    step: "直す".into(),
                    program: program.to_string_lossy().into_owned(),
                    placement: placement.id,
                }],
            );
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
            assert_eq!(check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"), vec![]);
            automation::placement_step_clear(tx, placement.id, only_step(tx, &action).id).expect("nobody chosen");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![],
                "a script needs nobody chosen to carry it out",
            );
            launch(tx, automation.id, &here(&claude())).expect("launches");
            let _ = std::fs::remove_dir_all(&dir);
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
            let copy = read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.placement_id == Some(placement.id))
                .expect("the placement's copy");
            assert_eq!(copy.agent, "claude");
            assert_eq!(copy.model.as_deref(), Some("opus"));
        });
    }

    #[test]
    fn a_step_further_inside_an_action_is_asked_for_its_agent_too() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            goes_on_to(tx, &action, &placement, "書く", "codex");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
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
                    None,
                )
                .expect("check"),
                vec![Unmet::ModelMissing {
                    step: "直す".into(),
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
                    None,
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![],
                "an agent nobody has asked says nothing about its models (AMB-D-865)",
            );
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), &offering("codex-cli", &["gpt"]), None)
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
                    None,
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
            automation::update(tx, automation.id, None, None, Some(true), None, None).expect("archive");
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
            assert_eq!(named, vec![("placement", id.as_str()), ("step", "直す"), ("agent", "claude")]);
        });
    }

    /// **A save asks the launch check**, and one that would not launch is refused with its reasons and
    /// leaves no version behind.
    #[test]
    fn a_draft_that_would_not_launch_is_not_saved() {
        with_tx(|tx| {
            let (automation, action, _) = launchable(tx);
            automation::exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある"))
                .expect("exit");
            let err = automation::save(tx, automation.id, None).expect_err("refused");
            let Error::NotReady(msg) = err else { panic!("a save that cannot go ahead is not_ready") };
            assert_eq!(msg.code(), Some(ErrorCode::NotReadyAutomation));
            assert_eq!(
                msg.parts().iter().map(|p| p.code()).collect::<Vec<_>>(),
                vec![Some(ErrorCode::NotReadyAutomationOpenExit)],
                "the agents this machine has are the launch's to ask, not the save's",
            );
            assert!(read::automation_version_ids(tx.conn(), automation.id).expect("read").is_empty());
            assert!(automation::unsaved(tx.conn(), automation.id).expect("unsaved"));
        });
    }

    /// **What is written after a save is unsaved until the next one**, and a save with nothing written
    /// since adds no version.
    #[test]
    fn a_save_takes_the_draft_and_what_is_written_after_it_waits_for_the_next() {
        with_tx(|tx| {
            let empty = mk_automation(tx, "まだ何も無い");
            assert!(!automation::unsaved(tx.conn(), empty.id).expect("unsaved"), "nothing to save yet");

            let (automation, _, placement) = launchable(tx);
            assert!(automation::unsaved(tx.conn(), automation.id).expect("unsaved"), "never saved");
            let first = automation::save(tx, automation.id, None).expect("save");
            assert_eq!(first.version, 1);
            assert!(!automation::unsaved(tx.conn(), automation.id).expect("unsaved"));
            let again = automation::save(tx, automation.id, None).expect("save with nothing written");
            assert_eq!(again.id, first.id, "nothing written since, so no new version");

            automation::update(tx, automation.id, Some("2件やりきる"), None, None, None, None).expect("rename");
            assert!(
                !automation::unsaved(tx.conn(), automation.id).expect("unsaved"),
                "the name is not part of a version",
            );
            automation::placement_move(tx, placement.id, crate::ops::Position::Top).expect("move");
            assert!(automation::unsaved(tx.conn(), automation.id).expect("unsaved"));
            launch(tx, automation.id, &here(&claude())).expect("launch");
            let second = automation::save(tx, automation.id, None).expect("saved while a run goes on");
            assert_eq!(second.version, 2);
            assert!(!automation::unsaved(tx.conn(), automation.id).expect("unsaved"));
        });
    }

    #[test]
    fn the_two_refusals_that_stand_alone_name_themselves_too() {
        with_tx(|tx| {
            let startable = claude();
            let (automation, _, _) = launchable(tx);
            automation::update(tx, automation.id, None, None, Some(true), None, None).expect("archive");
            let err = launch(tx, automation.id, &here(&startable)).expect_err("archived");
            let Error::Invalid(msg) = err else { panic!("an archived automation is invalid") };
            assert_eq!(msg.code(), Some(ErrorCode::InvalidAutomationArchived));
            assert_eq!(
                msg.fields().iter().map(|(key, _)| key).collect::<Vec<_>>(),
                vec!["automation"],
            );

            automation::update(tx, automation.id, None, None, Some(false), None, None).expect("bring back");
            let closed = Launcher { workspace_open: Some(false), ..here(&startable) };
            let err = launch(tx, automation.id, &closed).expect_err("closed");
            let Error::Invalid(msg) = err else { panic!("a closed workspace is invalid") };
            assert_eq!(msg.code(), Some(ErrorCode::InvalidAutomationWorkspaceClosed));
        });
    }

    /// **An entry refuses what it does not read** (`AMB-D-981`): only the built-in that files a task
    /// reads anything, so an agent's step and a built-in that takes a task are handed nothing — a file,
    /// or the title or classification of a task to file. Refused before any run is made.
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
                HandedAtLaunch { files: vec![file], ..Default::default() },
                HandedAtLaunch { title: Some("an issue".into()), ..Default::default() },
                HandedAtLaunch { classification: vec![("職能".into(), "実装".into())], ..Default::default() },
            ] {
                let err = launch_handing(tx, automation.id, &here(&startable), &handed, ReadsFrom::Saved)
                    .expect_err("not read");
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
            let titled = HandedAtLaunch { title: Some("an issue".into()), ..Default::default() };
            let pic = Picture::read(tx.conn(), &take).expect("the picture");
            let err = entry_reads(tx.conn(), &take, &pic, &titled).expect_err("reads nothing");
            assert!(err.to_string().contains("reads nothing handed over at launch"), "{err}");
            assert_eq!(entry_reads(tx.conn(), &take, &pic, &HandedAtLaunch::default()).expect("nothing"), None);
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
            assert_eq!(
                read::automation_run_defs_of(tx.conn(), run.id).expect("defs").len(),
                3,
                "and the take's and the close's",
            );
            let copy = copy_of_it(tx);
            assert_eq!(copy.name, "直す");
            assert_eq!(copy.prompt.as_deref(), Some("fix it"));
            assert_eq!(copy.step_id, Some(step.id));
            let exits: Vec<RunDefExit> = serde_json::from_str(&copy.exits).expect("exits");
            assert_eq!(exits.len(), 2, "the done way out and the error one");

            // Once the run is ended, the copy is what its record reads from (`AMB-D-961`), whatever the
            // definition becomes.
            crate::ops::automation_stop::stop(tx, run.id, crate::ops::automation_stop::Ending::Canceled)
                .expect("stop");
            automation::action_update(tx, action.id, Some("取り直す"), None)
                .expect("edit the definition once the run is over");
            automation::step_update(tx, step.id, None, Some("take another"), None, None, None, None, None, None, None, None, None)
            .expect("rewrite the prompt once the run is over");
            let copy = copy_of_it(tx);
            assert_eq!(copy.name, "直す", "the copy is what the run reads from here on");
            assert_eq!(copy.prompt.as_deref(), Some("fix it"));
        });
    }

    /// **A run copies how long each step is expected to take**, with the default put in where the step
    /// says nothing, so the copy keeps saying it once the step is written on.
    #[test]
    fn a_run_copies_how_long_a_step_is_expected_to_take() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            let expected_of = |tx: &WriteTx<'_>, run: i64| {
                read::automation_run_defs_of(tx.conn(), run)
                    .expect("defs")
                    .into_iter()
                    .find(|d| d.placement_id == Some(placement.id))
                    .expect("the placement's copy")
                    .expected_minutes
            };
            let expect = |tx: &WriteTx<'_>, minutes| {
                automation::step_update(tx, step.id, None, None, None, None, None, None, None, None, None, None, minutes)
                    .expect("write how long it takes")
            };
            let stop = |tx: &WriteTx<'_>, run: i64| {
                crate::ops::automation_stop::stop(tx, run, crate::ops::automation_stop::Ending::Canceled)
                    .expect("stop")
            };

            let first = launch(tx, automation.id, &here(&claude())).expect("launch");
            assert_eq!(expected_of(tx, first.id), Some(DEFAULT_EXPECTED_MINUTES), "a step that says nothing");
            stop(tx, first.id);

            expect(tx, Some(Some(25)));
            let second = launch(tx, automation.id, &here(&claude())).expect("launch");
            assert_eq!(expected_of(tx, second.id), Some(25));
            stop(tx, second.id);

            expect(tx, Some(Some(40)));
            assert_eq!(expected_of(tx, second.id), Some(25), "the copy is what the run reads from");
        });
    }

    /// **A placement standing on a saved version is checked and copied as that version holds the
    /// action**, not as the action has been written on since: a way out and a required input written
    /// after the save ask nothing of it, and the run copies the prompt as it was saved.
    #[test]
    fn a_placement_is_checked_and_copied_as_the_version_it_stands_on() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            let step = only_step(tx, &action);
            automation::action_version_add(tx, action.id).expect("save the action");
            let stand_on = |version: Option<i64>| {
                tx.conn()
                    .execute(
                        "UPDATE automation_placement SET version = ?1 WHERE id = ?2",
                        rusqlite::params![version, placement.id],
                    )
                    .expect("stand the placement on a version");
            };
            stand_on(Some(1));
            let again = Some("fix it again");
            automation::step_update(tx, step.id, None, again, None, None, None, None, None, None, None, None, None)
                .expect("write on");
            automation::exit_add(tx, AutomationOwner::Action, action.id, Some("直すところがある"))
                .expect("a way out written since");
            mk_in(tx, &action, "仕様", AutomationPortKind::Value, true);
            let unmet = checked(tx, &automation);
            assert!(unmet.is_empty(), "version 1 has neither: {unmet:?}");

            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            let copy = read::automation_run_defs_of(tx.conn(), run.id)
                .expect("defs")
                .into_iter()
                .find(|d| d.placement_id == Some(placement.id))
                .expect("the placement's copy");
            assert_eq!(copy.prompt.as_deref(), Some("fix it"), "the prompt as it was saved");

            stand_on(None);
            assert!(!checked(tx, &automation).is_empty(), "the action as it is written now has both");
        });
    }

    /// **A copy names the action its placement stood on, and whether one run at a time may be inside
    /// it, as the version stood on says** — and keeps both once the placement is taken off.
    #[test]
    fn a_copy_keeps_the_action_and_its_one_run_at_a_time_after_the_placement_goes() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            automation::action_set_one_at_a_time(tx, action.id, true).expect("turn it on");
            automation::action_save(tx, action.id).expect("save the action");
            tx.conn()
                .execute("UPDATE automation_placement SET version = 1 WHERE id = ?1", [placement.id])
                .expect("stand the placement on version 1");
            automation::action_set_one_at_a_time(tx, action.id, false).expect("turned off since");

            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            let copy_of_it = |tx: &WriteTx<'_>| {
                read::automation_run_defs_of(tx.conn(), run.id)
                    .expect("defs")
                    .into_iter()
                    .find(|d| d.placement_id == Some(placement.id))
                    .expect("the placement's copy")
            };
            let copy = copy_of_it(tx);
            assert_eq!(copy.action_id, Some(action.id));
            assert!(copy.one_at_a_time, "as version 1 holds it, not as the action is written now");
            let others = read::automation_run_defs_of(tx.conn(), run.id).expect("defs");
            assert!(
                others.iter().filter(|d| d.placement_id != Some(placement.id)).all(|d| !d.one_at_a_time),
                "the take's and the close's are off",
            );

            automation::placement_delete(tx, placement.id).expect("take the placement off");
            let copy = copy_of_it(tx);
            assert_eq!(copy.action_id, Some(action.id), "kept after the placement goes");
            assert!(copy.one_at_a_time, "kept after the placement goes");
        });
    }

    /// **A launch reads the saved definition and a test run the draft** — checked and copied alike. Before
    /// anybody saves, a launch has only the draft to read, and its copy records no version.
    #[test]
    fn a_launch_reads_the_saved_definition_and_a_test_run_the_draft() {
        with_tx(|tx| {
            let (automation, action, placement) = launchable(tx);
            crate::ops::test_support::room_for_runs(tx, automation.id);
            let step = only_step(tx, &action);
            let startable = vec!["claude".to_string(), "codex".to_string()];
            let copied = |tx: &WriteTx<'_>, run: &AutomationRun| {
                let copy = read::automation_run_defs_of(tx.conn(), run.id)
                    .expect("defs")
                    .into_iter()
                    .find(|d| d.placement_id == Some(placement.id))
                    .expect("the placement's copy");
                (copy.agent, copy.automation_version)
            };
            let unsaved = launch(tx, automation.id, &here(&startable)).expect("launch");
            assert_eq!(copied(tx, &unsaved), ("claude".to_string(), None), "never saved: the draft");

            automation::save(tx, automation.id, None).expect("save");
            automation::placement_step_set(tx, placement.id, step.id, "codex", None).expect("choose another");
            let saved = launch(tx, automation.id, &here(&startable)).expect("launch");
            assert_eq!(copied(tx, &saved), ("claude".to_string(), Some(1)), "the saved version");
            let tried = launch_handing(tx, automation.id, &here(&startable), &HandedAtLaunch::default(), ReadsFrom::Draft)
                .expect("test run");
            assert_eq!(copied(tx, &tried), ("codex".to_string(), None), "the draft");

            automation::set_entry(tx, automation.id, None).expect("no entry in the draft");
            launch(tx, automation.id, &here(&startable)).expect("the saved version still has its entry");
            let err = launch_handing(tx, automation.id, &here(&startable), &HandedAtLaunch::default(), ReadsFrom::Draft)
                .expect_err("the draft has none");
            assert!(matches!(err, Error::NotReady(_)), "{err:?}");
        });
    }

    /// **An automation a run was launched from is not deleted, and the refusal says so by its code** — the
    /// code is what the CLI answers with archiving, which is how such an automation goes out of the way.
    #[test]
    fn an_automation_its_runs_are_filed_under_is_refused_as_having_runs() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");
            crate::ops::automation_stop::stop(tx, run.id, crate::ops::automation_stop::Ending::Canceled)
                .expect("stop");
            let err = automation::delete(tx, automation.id).expect_err("a run is filed under it");
            let Error::Invalid(msg) = err else { panic!("an automation with runs is invalid to delete") };
            assert_eq!(msg.code(), Some(ErrorCode::InvalidAutomationHasRuns));
            assert_eq!(msg.fields().iter().collect::<Vec<_>>(), vec![("count", "1")]);
            automation::update(tx, automation.id, None, None, Some(true), None, None).expect("archiving is the way instead");
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
            let task = crate::ops::test_support::mk_task_in(tx, "一件", Some(automation.project_id));
            crate::ops::task::set_assignee(tx, task, Some(ActorKind::Ai), ActorKind::Ai).expect("give it to the AI");
            let run = launch(tx, automation.id, &here(&claude())).expect("launch");

            // Nothing has run yet, so what is waiting is where the run starts: the built-in that takes
            // a task.
            let Waiting::Step(first) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the entry is what a fresh run waits for")
            };
            assert_eq!(first.builtin.as_deref(), Some("take_task"));

            // Carried out on the spot, it takes the task, and the answer is read off the way out it
            // took — the one leading on to the step that works on it.
            match crate::ops::test_support::open(tx, run.id, first.id, None).expect("take") {
                crate::ops::automation_step::Opened::Carried { .. } => {}
                other => panic!("a built-in is carried out, not {other:?}"),
            }
            let Waiting::Step(work) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the step that works on the task is what it waits for next")
            };
            assert_eq!(work.placement_id, Some(placement.id));

            // A step under way is nobody's to open a second time.
            let opening = match crate::ops::test_support::open(tx, run.id, work.id, None)
                .expect("open")
            {
                crate::ops::automation_step::Opened::Ready(ready) => *ready,
                crate::ops::automation_step::Opened::Stopped { missing, .. } => {
                    panic!("stopped for {missing:?}")
                }
                crate::ops::automation_step::Opened::NoAgent { agent, .. } => {
                    panic!("cannot start {agent}")
                }
                crate::ops::automation_step::Opened::Carried { .. } | crate::ops::automation_step::Opened::Waiting { .. } | crate::ops::automation_step::Opened::LeftToWait { .. } | crate::ops::automation_step::Opened::Holding { .. } => panic!("not a built-in"),
                crate::ops::automation_step::Opened::LeftTaskOpen { .. } => panic!("left a task open"),
                crate::ops::automation_step::Opened::Script(_) => panic!("not a script"),
                crate::ops::automation_step::Opened::WaitsForRoom { .. } => panic!("not kept out of an action"),
            };
            assert!(matches!(next_def(tx.conn(), run.id).expect("next"), Waiting::Nothing));

            // Once it has reported, the answer is read off the way out it took — here the done one,
            // which goes on to the built-in that closes the task.
            crate::ops::automation_report::done(tx, opening.run_step.id, None, "did it")
                .expect("report");
            let Waiting::Step(close) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the close is what it waits for next")
            };
            assert_eq!(close.builtin.as_deref(), Some("close_task"));

            // Carried out on the spot, it closes the task and the round with it: the run goes round,
            // and what it waits for is its entry again.
            crate::ops::test_support::open(tx, run.id, close.id, None).expect("close");
            let Waiting::Step(again) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the next round starts at the entry")
            };
            assert_eq!(again.id, first.id, "the entry of the copy it has");

            // With nothing left to take, the run comes to its end: nothing is waiting, and the run is no
            // longer running.
            crate::ops::test_support::open(tx, run.id, again.id, None).expect("take again");
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
        let run = crate::ops::automation_run::launch_past_the_task_checks(tx, automation.id, &here(&claude()))
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
            crate::ops::automation_step::Opened::Carried { .. } | crate::ops::automation_step::Opened::Waiting { .. } | crate::ops::automation_step::Opened::LeftToWait { .. } | crate::ops::automation_step::Opened::Holding { .. } => panic!("not a built-in"),
            crate::ops::automation_step::Opened::LeftTaskOpen { .. } => panic!("left a task open"),
            crate::ops::automation_step::Opened::Script(_) => panic!("not a script"),
            crate::ops::automation_step::Opened::WaitsForRoom { .. } => panic!("not kept out of an action"),
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
    /// it goes next (`AMB-D-1015`).
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

            automation::edge_update(tx, onward.id, Some(EdgeTarget::Halt), None)
                .expect("point it at an ending");
            assert_eq!(waits_for(tx), "読む", "the line it copied still leads on");

            let (_, late) = mk_placed(tx, &automation, "直す", "fix it", "claude");
            automation::edge_update(tx, onward.id, Some(EdgeTarget::Go(late.id)), None)
                .expect("point it at a new placement");
            assert_eq!(waits_for(tx), "読む", "a placement added since is not in its copies");

            automation::edge_delete(tx, onward.id).expect("delete the edge");
            // The entry comes off last (`AMB-D-977`), so the spot taken off is the one added since.
            automation::placement_delete(tx, late.id)
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

    /// Open the step `run` waits for, and hand back the execution it opened.
    fn open_next(tx: &WriteTx<'_>, run_id: i64) -> crate::model::AutomationRunStep {
        let Waiting::Step(def) = next_def(tx.conn(), run_id).expect("next") else {
            panic!("the run waits for a step")
        };
        match crate::ops::test_support::open(tx, run_id, def.id, None).expect("open") {
            crate::ops::automation_step::Opened::Ready(ready) => ready.run_step,
            _ => panic!("a person's step opens a terminal"),
        }
    }

    /// **A run's trail is the spots it passed on its task, the way out it left each by and the line
    /// it walked, and where it stands** — read off its copies, one pass per spot.
    #[test]
    fn a_run_s_trail_is_the_spots_it_passed_and_the_one_it_stands_at() {
        with_tx(|tx| {
            let (automation, first, onward) = two_spots(tx);
            let second = onward.to_id.expect("the line goes on to the second spot");
            let run = standing_between(tx, &automation);
            let left_by = exit_id(tx, AutomationOwner::Action, first.action_id, None);

            let between = trail(tx.conn(), run.id).expect("trail");
            assert_eq!(
                between.passed,
                vec![Passed {
                    placement_id: Some(first.id),
                    status: AutomationRunStepStatus::Done,
                    exit_id: Some(left_by),
                    edge_id: Some(onward.id),
                }],
            );
            assert_eq!(between.at, Some(second), "the spot it is about to open is where it stands");
            let placement = read::automation_placement(tx.conn(), second).expect("read").expect("the spot");
            let action =
                read::automation_action(tx.conn(), placement.action_id).expect("read").expect("its action");
            let step = only_step(tx, &action).id;
            assert!(between.steps.is_empty(), "it has opened nothing in the spot yet");
            assert_eq!(between.step, Some(step), "the step it is about to open is the one it stands at");

            open_next(tx, run.id);
            let on = trail(tx.conn(), run.id).expect("trail");
            assert_eq!(on.passed.len(), 2);
            assert_eq!(
                on.passed[1],
                Passed {
                    placement_id: Some(second),
                    status: AutomationRunStepStatus::Running,
                    exit_id: None,
                    edge_id: None,
                },
            );
            assert_eq!(on.at, Some(second));
            assert_eq!(
                on.steps,
                vec![PassedStep { step_id: Some(step), status: AutomationRunStepStatus::Running, edge_id: None }],
            );
            assert_eq!(on.step, Some(step));
        });
    }

    /// **A run just launched stands at the step it opens first** — the built-in that takes a task,
    /// though nothing is open there yet.
    #[test]
    fn a_run_just_launched_stands_at_the_step_it_opens_first() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let run = crate::ops::automation_run::launch_past_the_task_checks(tx, automation.id, &here(&claude()))
                .expect("launch");
            let Waiting::Step(entry) = next_def(tx.conn(), run.id).expect("next") else {
                panic!("the entry is what a fresh run waits for")
            };

            let waiting = trail(tx.conn(), run.id).expect("trail");
            assert!(waiting.passed.is_empty());
            assert_eq!(waiting.at, automation.entry_placement_id);
            assert!(waiting.steps.is_empty());
            assert!(entry.step_id.is_some());
            assert_eq!(waiting.step, entry.step_id);
        });
    }

    /// **The steps inside one spot fold into one pass** — the picture draws spots, so going from one
    /// step of an action to the next lights no new box and walks no line of the automation's.
    #[test]
    fn the_steps_inside_one_spot_fold_into_one_pass() {
        with_tx(|tx| {
            let (automation, first, onward) = two_spots(tx);
            let second = onward.to_id.expect("the line goes on to the second spot");
            let placement = read::automation_placement(tx.conn(), second).expect("read").expect("the spot");
            let action =
                read::automation_action(tx.conn(), placement.action_id).expect("read").expect("its action");
            let outer = only_step(tx, &action);
            let inner = a_second_step(tx, &action, &placement);
            automation::edge_add(tx, AutomationPictureOwner::Action, inner.id, None, EdgeTarget::Exit(None), None)
                .expect("the second step leaves the action by its done way out");
            let between_steps = read::automation_edge_for_exit(
                tx.conn(),
                AutomationPictureOwner::Action,
                outer.id,
                exit_id(tx, AutomationOwner::Step, outer.id, None),
            )
            .expect("read")
            .expect("the line from the first step to the second");
            let run = standing_between(tx, &automation);

            let opened = open_next(tx, run.id);
            crate::ops::automation_report::done(tx, opened.id, None, "read it").expect("report");
            let inside = trail(tx.conn(), run.id).expect("trail");
            assert_eq!(inside.passed.len(), 2, "going on inside the spot is not a new pass");
            assert_eq!(inside.passed[1].placement_id, Some(second));
            assert_eq!(inside.passed[1].exit_id, None, "it has not left the spot yet");
            assert_eq!(inside.passed[1].edge_id, None);
            assert_eq!(inside.at, Some(second));
            let first_step = PassedStep {
                step_id: Some(outer.id),
                status: AutomationRunStepStatus::Done,
                edge_id: Some(between_steps.id),
            };
            assert_eq!(inside.steps, vec![first_step.clone()], "the step it passed, and the line it walked on");
            assert_eq!(inside.step, Some(inner.id), "the step it is about to open");

            let last = open_next(tx, run.id);
            let on = trail(tx.conn(), run.id).expect("trail");
            assert_eq!(on.passed.len(), 2);
            assert_eq!(
                on.steps,
                vec![
                    first_step,
                    PassedStep { step_id: Some(inner.id), status: AutomationRunStepStatus::Running, edge_id: None },
                ],
            );
            assert_eq!(on.step, Some(inner.id));
            crate::ops::automation_report::done(tx, last.id, None, "read it again").expect("report");
            let left = trail(tx.conn(), run.id).expect("trail");
            assert_eq!(left.passed.len(), 2);
            assert_eq!(left.passed[0].placement_id, Some(first.id));
            assert_eq!(
                left.passed[1].exit_id,
                Some(exit_id(tx, AutomationOwner::Action, action.id, None)),
                "the pass ends on the action's way out the last step returned to",
            );
            assert!(left.passed[1].edge_id.is_some(), "the line the automation draws after it");
            assert!(left.steps.is_empty(), "once it has left the spot, no step inside it is lit");
        });
    }

    /// **A second launch does not wait for the first** (`AMB-D-947`). With room for two, both are
    /// `running` from the moment they are made and both carry a `started_at`.
    #[test]
    fn a_second_launch_starts_beside_the_first_rather_than_behind_it() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            automation::update(tx, automation.id, None, None, None, Some(2), None).expect("room for two");
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

    /// **A launch past the most runs an automation may have going is refused** — and a paused run holds
    /// its place as much as a running one (`AMB-D-961`), so pausing one does not make room.
    #[test]
    fn a_launch_past_the_most_runs_under_way_is_refused_and_a_paused_one_counts() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let automation =
                automation::update(tx, automation.id, None, None, None, Some(2), None).expect("set a most");
            let startable = claude();
            let first = launch(tx, automation.id, &here(&startable)).expect("the first is under the most");
            let second = launch(tx, automation.id, &here(&startable)).expect("the second reaches it");
            let refused = |tx: &WriteTx<'_>| {
                let err = launch(tx, automation.id, &here(&startable)).expect_err("a third is past the most");
                let Error::Invalid(msg) = err else { panic!("a launch past the most is invalid") };
                assert_eq!(msg.code(), Some(ErrorCode::InvalidAutomationRunLimit));
                assert_eq!(
                    msg.fields().iter().collect::<Vec<_>>(),
                    vec![
                        ("automation", automation.name.as_str()),
                        ("max", "2"),
                        ("runs", format!("{}, {}", first.id, second.id).as_str()),
                    ],
                );
            };
            refused(tx);

            crate::ops::automation_stop::pause(tx, first.id).expect("pause");
            let pausing = read::automation_run(tx.conn(), first.id).expect("read").expect("the run");
            let at_end = crate::model::AutomationPauseKind::EndOfAction;
            crate::ops::automation_stop::settle(tx, pausing, at_end).expect("settle");
            let paused = read::automation_run(tx.conn(), first.id).expect("read").expect("the run");
            assert_eq!(paused.status, AutomationRunStatus::Paused);
            refused(tx);
        });
    }

    /// **What was handed is refused before the most runs under way is**: a launch handed the wrong
    /// thing would not go through with room to spare either, so that is the refusal it gets.
    #[test]
    fn what_was_handed_is_refused_before_the_most_runs_under_way() {
        with_tx(|tx| {
            let (automation, _, _) = launchable(tx);
            let automation =
                automation::update(tx, automation.id, None, None, None, Some(1), None).expect("set a most");
            let startable = claude();
            launch(tx, automation.id, &here(&startable)).expect("the first reaches the most");
            let titled = HandedAtLaunch { title: Some("an issue".into()), ..Default::default() };
            let err = launch_handing(tx, automation.id, &here(&startable), &titled, ReadsFrom::Saved)
                .expect_err("handed what the entry does not read");
            assert!(err.to_string().contains("reads nothing handed over at launch"), "{err}");
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
            let second = a_second_step(tx, &action, &placement);
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::OpenExit {
                    step: "見直す".into(),
                    exit: crate::model::DONE_EXIT.into(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: Some(second.id),
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
            // The run holds the task here, so the line out of it closes the task on the way.
            crate::ops::test_support::mk_closed_after(tx, &automation, placement.id, Some("差し戻し"));
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![],
            );

            automation::exit_rename(tx, mine.id, Some("戻す")).expect("rename");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![],
                "both lines stay on the way out they key",
            );

            automation::exit_delete(tx, mine.id).expect("delete");
            let unmet =
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check");
            assert!(
                unmet.contains(&Unmet::OpenExit {
                    step: "見直す".into(),
                    exit: crate::model::DONE_EXIT.into(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: Some(second.id),
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "見直す".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: Some(second.id),
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![],
                "the first step inside hands it on",
            );
        });
    }

    /// **A wire from a step's own way out inside the action does not feed its input** (`AMB-T-5641`), as
    /// on the automation's picture: the first time the step opens nothing has left by it yet.
    #[test]
    fn a_wire_from_a_steps_own_way_out_inside_does_not_feed_its_input() {
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
            let exit = read::automation_exit_by_name(tx.conn(), AutomationOwner::Step, second.id, None)
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
            automation::wire_add(tx, on, second.id, None, "下書き", second.id, "下書き")
                .expect("wire back into itself");
            assert_eq!(
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "見直す".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: Some(second.id),
                }],
                "the only wire into it comes back from its own way out",
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
                check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check"),
                vec![Unmet::UnwiredInput {
                    step: "直す".into(),
                    port: "下書き".into(),
                    builtin: None,
                    placement: Some(placement.id),
                    inside: Some(step.id),
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
            let (action, first) = mk_placed(tx, &automation, "直す", "fix it", "claude");
            let second = a_second_step(tx, &action, &first);
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
            let unmet = check(tx.conn(), automation.id, Some(&claude()), nothing_asked(), None).expect("check");
            let about = |at: i64| Unmet::OpenExit {
                step: "見直す".into(),
                exit: crate::model::DONE_EXIT.into(),
                builtin: None,
                placement: Some(at),
                inside: Some(second.id),
            };
            assert!(unmet.contains(&about(first.id)), "{unmet:?}");
            assert!(unmet.contains(&about(again.id)), "{unmet:?}");
            let fields = about(again.id).msg().fields().iter().map(|(k, v)| (k, v.to_string())).collect::<Vec<_>>();
            assert!(fields.contains(&("placement", again.id.to_string())), "{fields:?}");
            assert!(fields.contains(&("inside_step", second.id.to_string())), "{fields:?}");
            assert_eq!(Unmet::NoEntry.placement(), None);
        });
    }

    /// **What a trigger's script said is read from its `output.json` alone** — `"file"` true or false
    /// under version 1. Every other way it ended, an exit code of 0 with no answer included, is not an
    /// answer.
    #[test]
    fn a_trigger_script_answers_in_its_output_not_its_exit_code() {
        use automation_script::Ended;
        let script = StepScript { program: "/usr/local/bin/judge".into(), args: Vec::new(), timeout_minutes: 5 };
        let wrote = |output: serde_json::Value| Ended::Wrote { output, files: Vec::new() };
        assert_eq!(script_said(&script, &wrote(serde_json::json!({ "version": 1, "file": true }))), Ok(true));
        assert_eq!(script_said(&script, &wrote(serde_json::json!({ "version": 1, "file": false }))), Ok(false));
        for unshaped in [
            serde_json::json!({ "file": true }),
            serde_json::json!({ "version": 2, "file": true }),
            serde_json::json!({ "version": 1 }),
            serde_json::json!({ "version": 1, "file": "yes" }),
        ] {
            let said = script_said(&script, &wrote(unshaped.clone()));
            assert!(said.as_ref().is_err_and(|why| why.contains("not in the shape")), "{unshaped}: {said:?}");
        }
        for ended in [Ended::NotStarted("no such file".into()), Ended::TimedOut, Ended::Stopped, Ended::NoOutput] {
            assert!(script_said(&script, &ended).is_err(), "{ended:?} is no answer");
        }
        assert_eq!(
            script_said(&script, &Ended::TimedOut),
            Err("the program was still running after 5 minutes, and was stopped".to_string())
        );
    }

    /// **A trigger's script is run and heard**: what it writes to `AMENBO_OUTPUT` is its answer, and one
    /// that ends other than with 0 has none, whatever it wrote.
    #[cfg(unix)]
    #[test]
    fn a_trigger_script_is_run_and_its_answer_read() {
        let sh = |body: &str| StepScript {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), body.into()],
            timeout_minutes: crate::model::DEFAULT_SCRIPT_TIMEOUT_MINUTES,
        };
        let yes = sh(r#"echo '{"version":1,"file":true}' > "$AMENBO_OUTPUT""#);
        assert_eq!(ask_script(&yes, || false), Ok(true));
        let no = sh(r#"echo '{"version":1,"file":false}' > "$AMENBO_OUTPUT""#);
        assert_eq!(ask_script(&no, || false), Ok(false));
        let fell = sh(r#"echo '{"version":1,"file":true}' > "$AMENBO_OUTPUT"; exit 3"#);
        assert_eq!(ask_script(&fell, || false), Err("the program ended with exit code 3".to_string()));
    }

    /// **What the program of a trigger's task wrote is read from its `output.json` alone** — a title
    /// that is not blank, and notes that may be left out, under version 1. Any other shape, and any
    /// other way it ended, is nothing to file.
    #[test]
    fn a_trigger_task_script_writes_its_title_and_notes_in_its_output() {
        use automation_script::Ended;
        let script = StepScript { program: "/usr/local/bin/write-task".into(), args: Vec::new(), timeout_minutes: 5 };
        let wrote = |output: serde_json::Value| Ended::Wrote { output, files: Vec::new() };
        assert_eq!(
            script_wrote(&script, &wrote(serde_json::json!({ "version": 1, "title": " 受信箱を見る ", "notes": "三通" }))),
            Ok(("受信箱を見る".to_string(), "三通".to_string()))
        );
        assert_eq!(
            script_wrote(&script, &wrote(serde_json::json!({ "version": 1, "title": "見る" }))),
            Ok(("見る".to_string(), String::new())),
            "no notes"
        );
        for unshaped in [
            serde_json::json!({ "title": "見る" }),
            serde_json::json!({ "version": 2, "title": "見る" }),
            serde_json::json!({ "version": 1 }),
            serde_json::json!({ "version": 1, "title": "  " }),
            serde_json::json!({ "version": 1, "title": 3 }),
            serde_json::json!({ "version": 1, "title": "見る", "notes": ["a"] }),
        ] {
            let said = script_wrote(&script, &wrote(unshaped.clone()));
            assert!(said.as_ref().is_err_and(|why| why.contains("not in the shape")), "{unshaped}: {said:?}");
        }
        for ended in [Ended::NotStarted("no such file".into()), Ended::TimedOut, Ended::Stopped, Ended::NoOutput] {
            assert!(script_wrote(&script, &ended).is_err(), "{ended:?} is nothing to file");
        }
    }

    /// **A trigger's programs are run in turn**: the one that asks first, and the one that writes the
    /// task only once it said yes. One saying no leaves the other unrun, and either falling over is no
    /// answer at all.
    #[cfg(unix)]
    #[test]
    fn a_trigger_asks_first_and_then_has_its_task_written() {
        let sh = |body: &str| StepScript {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), body.into()],
            timeout_minutes: crate::model::DEFAULT_SCRIPT_TIMEOUT_MINUTES,
        };
        let yes = sh(r#"echo '{"version":1,"file":true}' > "$AMENBO_OUTPUT""#);
        let no = sh(r#"echo '{"version":1,"file":false}' > "$AMENBO_OUTPUT""#);
        let write = sh(r#"echo '{"version":1,"title":"見る","notes":"三通"}' > "$AMENBO_OUTPUT""#);
        let fell = sh("exit 3");

        let both = TriggerScripts { ask: Some(yes.clone()), write: Some(write.clone()) };
        assert_eq!(
            ask_trigger(&both, || false),
            Ok(Some(ScriptsSaid { scripts: both.clone(), wrote: Some(("見る".into(), "三通".into())) }))
        );
        let asked = TriggerScripts { ask: Some(yes), write: None };
        assert_eq!(ask_trigger(&asked, || false), Ok(Some(ScriptsSaid { scripts: asked.clone(), wrote: None })));
        let written = TriggerScripts { ask: None, write: Some(write.clone()) };
        assert!(ask_trigger(&written, || false).is_ok_and(|said| said.is_some_and(|said| said.wrote.is_some())));

        // Saying no, the writer is never run: one that would fall over is not heard from.
        assert_eq!(ask_trigger(&TriggerScripts { ask: Some(no), write: Some(fell.clone()) }, || false), Ok(None));
        assert_eq!(
            ask_trigger(&TriggerScripts { ask: Some(fell.clone()), write: Some(write) }, || false),
            Err("the program ended with exit code 3".to_string())
        );
        assert!(ask_trigger(&TriggerScripts { ask: None, write: Some(fell) }, || false).is_err());
    }

    /// **A trigger's task is read from its file as it stands** — the first line that is not blank is
    /// the title, the rest the notes. A file not there or blank says nothing to file.
    #[test]
    fn a_trigger_task_is_read_from_its_file() {
        let dir = std::env::temp_dir().join(format!("amenbo-trigger-task-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let at = |name: &str, text: &str| {
            let path = dir.join(name);
            std::fs::write(&path, text).expect("write");
            path.to_string_lossy().into_owned()
        };
        assert_eq!(
            task_from_file(&at("full.md", "\n  受信箱を見る  \r\n\n- 三通\n- 急ぎ\n\n")),
            Ok(("受信箱を見る".to_string(), "- 三通\n- 急ぎ".to_string()))
        );
        assert_eq!(
            task_from_file(&at("indented.md", "見る\n  \n\n    code\n")),
            Ok(("見る".to_string(), "    code".to_string())),
            "a line of spaces is a blank line, and the indent of the notes is kept"
        );
        assert_eq!(task_from_file(&at("title.md", "見る")), Ok(("見る".to_string(), String::new())));
        assert!(task_from_file(&at("blank.md", " \n\n")).is_err_and(|why| why.contains("blank")));
        let gone = dir.join("gone.md").to_string_lossy().into_owned();
        assert!(task_from_file(&gone).is_err_and(|why| why.contains("could not be read")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
