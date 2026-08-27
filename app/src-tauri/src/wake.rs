//! **Which agent a folder's pane is opened with** — the command layer over [`amenbo_core::wake`].
//!
//! The judgment itself is core's, and the two halves it needs are gathered here: the folder's trace
//! comes off the disk ([`amenbo_core::harness::probe`]) and what this machine can start comes off
//! the pane's own shell ([`crate::launch::installed`]). Neither belongs in core — one is a probe of
//! a shell this crate owns, and the other is a folder the webview named.
//!
//! **The folder is resolved once, here, and handed back.** A pane and its answer have to be about
//! the same folder, and `~/work` and `/Users/me/work` are the same folder written twice — so a face
//! asks with whatever it has, and opens with the canonical form this answers in.
//!
//! **What a press chooses is kept against the person, and what a project pins is kept against the
//! project.** `wake_chose` writes the first ([`amenbo_core::config::Config::last_agent`]) and
//! `wake_remember` the second — the two ranks [`amenbo_core::wake::settle`] reads in that order.
//!
//! **The question comes in two shapes, and the ranks are read the same way for both.** A pane about
//! to open in a known folder asks `wake_probe`; an empty frame, where no folder has been settled
//! yet, asks `wake_choices` about the project and hands over the folders it is bound to. What
//! differs is only where the trace is read from — one folder, or all of them — because a preference
//! shown in any of a project's folders is the project's ([`amenbo_core::wake`]).
//!
//! **Not on the perf budget, deliberately.** A probe's time is a login shell reading the reader's
//! own profile ([`crate::launch::installed`]), so it busts a 50 ms budget on every machine and
//! nothing in Amenbo can make it not. A WARN that fires every time a window opens and names nothing
//! anyone can act on is noise in the one log that is meant to be read.
//!
//! **The webview never names a program.** What crosses is an id; the command it becomes is read on
//! this side, out of [`amenbo_core::harness::LAUNCHES`] or out of this device's own registrations
//! ([`amenbo_core::config::Config::custom_agents`]). A pane is a shell with a command line, so an id
//! that turned into whatever string arrived would be a shell injection with a webview at the other
//! end of it.
//!
//! **A registered command is written here and read like any other row** (`AMB-D-794`). The three
//! doors that keep it — `wake_register`, `wake_amend`, `wake_unregister` — sit beside the ones
//! that keep an answer, because they are the same kind of thing: a whole-file rewrite of the
//! device's settings, serialized through the store. What the registration is *allowed* to hold is
//! wider than a catalog row by design — a whole command line, arguments and all — and the line it
//! draws is elsewhere: the line is never taken apart here, and only its first word is ever looked
//! for on the `PATH`.

use std::path::{Path, PathBuf};

use amenbo_core::wake::{self, Choice};

use crate::dto::{WakeCandidateDto, WakeDto};
use crate::error::CmdError;

/// What a folder says, what this machine says, and the answer the two come to.
///
/// `folder` is where the pane would open, and it is asked for rather than defaulted: a pane is opened
/// in the folder the person chose (`app/src/talk/agent.ts`), so there is no such thing here as a probe
/// about nowhere in particular.
///
/// `project` is whose answer is remembered. A pane that belongs to no project — the window split out
/// before the board told it which one it was on — passes none, and gets the rank without a
/// remembered answer on top of it.
#[tauri::command]
pub fn wake_probe(folder: String, project: Option<i64>) -> Result<WakeDto, CmdError> {
    let folder = resolve(folder)?;
    let found = amenbo_core::harness::probe(&folder, amenbo_core::config::Paths::command_name());
    let config = config()?;
    let candidates = weighed(&found, config.custom_agents());
    answer(
        Some(folder.to_string_lossy().into_owned()),
        candidates,
        project,
        &config,
    )
}

/// What a **project** opens its panes with, asked before there is a pane or a folder to ask about.
///
/// This is the empty frame's question (`app/src/shell/EmptySlot.tsx`), and it is the same judgment
/// as [`wake_probe`] over a wider trace: `folders` are the project's own bound folders, and a
/// provider traced in any of them is traced for the project. A folder that has gone is skipped
/// rather than refused — the reader is choosing what to open with, and a stale binding is not a
/// reason to put a refusal in place of the choice.
#[tauri::command]
pub fn wake_choices(project: Option<i64>, folders: Vec<String>) -> Result<WakeDto, CmdError> {
    let command = amenbo_core::config::Paths::command_name();
    let found: Vec<amenbo_core::harness::Wiring> = folders
        .iter()
        .filter_map(|one| std::fs::canonicalize(one).ok())
        .flat_map(|one| amenbo_core::harness::probe(&one, command))
        .collect();
    let config = config()?;
    let candidates = weighed(&found, config.custom_agents());
    answer(None, candidates, project, &config)
}

/// Every provider Amenbo can start — the catalog's and this device's own — told apart by what this
/// machine can start.
///
/// One probe answers for all of them, registered rows included: what is asked about a registration
/// is the first word of its line ([`amenbo_core::config::CustomAgent::command`]), which is the only
/// part of it that names something to look for.
fn weighed(
    found: &[amenbo_core::harness::Wiring],
    registered: &[amenbo_core::config::CustomAgent],
) -> Vec<wake::Candidate> {
    let mut commands: Vec<&str> = amenbo_core::harness::LAUNCHES
        .iter()
        .map(|one| one.command)
        .collect();
    commands.extend(registered.iter().map(|one| one.command()));
    let installed = crate::launch::installed(&commands);
    wake::candidates(found, registered, |cmd| {
        installed.iter().any(|one| one == cmd)
    })
}

/// The candidates and the project's answer, in the shape a face reads.
fn answer(
    folder: Option<String>,
    candidates: Vec<wake::Candidate>,
    project: Option<i64>,
    config: &amenbo_core::config::Config,
) -> Result<WakeDto, CmdError> {
    let kept = project.and_then(|id| config.agent_for(id));
    let settled = match wake::settle(kept, config.last_agent(), &candidates) {
        Choice::Settled(id) => Some(id),
        Choice::Ask(_) | Choice::Nothing => None,
    };
    Ok(WakeDto {
        folder,
        offered: wake::offered(&candidates)
            .iter()
            .map(|one| one.id.clone())
            .collect(),
        candidates: candidates.iter().map(row).collect(),
        settled,
        kept: kept.map(str::to_string),
    })
}

/// Keep this project's answer, so the next pane opened in it starts with the same thing.
///
/// The id is checked against the catalog **and this device's registrations** rather than trusted,
/// because what is written here is read back as the thing to start.
///
/// **Written through the store, though it is read without one.** A write is a whole-file rewrite of
/// the device's settings, so one built on a copy read minutes ago would carry back whatever the
/// settings screen changed in between; the store is what serializes that. Reading is the other way
/// round — it loses nothing and happens every time a window opens — so [`wake_probe`] takes the
/// cheap road, the same one `ui_language` takes.
#[tauri::command]
pub fn wake_remember(project: i64, agent: String) -> Result<(), CmdError> {
    crate::migrate::gate()?;
    let mut store = amenbo_core::Store::open_at(paths()?).map_err(not_kept)?;
    known(&store.config, &agent)?;
    store.config.remember_agent(project, &agent);
    store.save_config().map_err(not_kept)
}

/// Keep what a pane was just opened with as **this person's** answer, so the next one they open
/// anywhere comes up on the same thing ([`amenbo_core::config::Config::last_agent`]).
///
/// Every press that chooses one goes through here — the row on the empty frame, the offer a folder
/// with several puts up, and the row on a frame whose program has ended (`app/src/talk/agent.ts`).
/// The rank's own answer does not: a pane that opened on what was already settled is nobody
/// deciding anything.
///
/// [`amenbo_core::wake::SHELL`] is allowed where [`wake_remember`] would refuse it. What is being
/// recorded is what this person opened with, and a plain prompt is one of the things they open with;
/// what a *project* works with is a different question, and a shell is not an answer to it.
///
/// Written through the store for the same reason [`wake_remember`] is: a write is a whole-file
/// rewrite of the device's settings.
#[tauri::command]
pub fn wake_chose(agent: String) -> Result<(), CmdError> {
    crate::migrate::gate()?;
    let mut store = amenbo_core::Store::open_at(paths()?).map_err(not_kept)?;
    if agent != wake::SHELL {
        known(&store.config, &agent)?;
    }
    store.config.remember_last_agent(&agent);
    store.save_config().map_err(not_kept)
}

/// Drop this project's answer, so the next pane opened in it settles one from the rank again — what
/// the project's settings write when the reader takes the choice back off.
#[tauri::command]
pub fn wake_forget(project: i64) -> Result<(), CmdError> {
    crate::migrate::gate()?;
    let mut store = amenbo_core::Store::open_at(paths()?).map_err(not_kept)?;
    store.config.forget_agent(project);
    store.save_config().map_err(not_kept)
}

/// Register a command of the reader's own, answering with the id it was given — which is what a
/// face hands back to open a pane with (`AMB-D-794`).
///
/// **The line is not judged**, past both halves being non-empty. What is written here runs in the
/// pane's shell, and the pane is a real terminal (`AMB-D-747`): anything Amenbo refused to register
/// would still be something the reader could type into it, so a guard here would buy nothing and
/// cost them the tool they actually use.
///
/// Written through the store for the same reason a kept answer is ([`wake_remember`]).
#[tauri::command]
pub fn wake_register(label: String, line: String) -> Result<String, CmdError> {
    crate::migrate::gate()?;
    let mut store = amenbo_core::Store::open_at(paths()?).map_err(not_kept)?;
    let id = store
        .config
        .register_agent(&label, &line)
        .map_err(badly_written)?
        .id
        .clone();
    store.save_config().map_err(not_kept)?;
    Ok(id)
}

/// Correct a registered command, keeping its id — so a row fixed after a typo is still the one this
/// project pinned and this person last opened with.
#[tauri::command]
pub fn wake_amend(id: String, label: String, line: String) -> Result<(), CmdError> {
    crate::migrate::gate()?;
    let mut store = amenbo_core::Store::open_at(paths()?).map_err(not_kept)?;
    registered(&store.config, &id)?;
    store.config.amend_agent(&id, &label, &line).map_err(badly_written)?;
    store.save_config().map_err(not_kept)
}

/// Drop a registered command.
///
/// Where its id was written down it is left alone, the same as a catalogued tool that has been
/// uninstalled: [`amenbo_core::wake::settle`] passes over an answer that is no longer offered, so a
/// stale id costs a line in the settings and nothing else.
#[tauri::command]
pub fn wake_unregister(id: String) -> Result<(), CmdError> {
    crate::migrate::gate()?;
    let mut store = amenbo_core::Store::open_at(paths()?).map_err(not_kept)?;
    registered(&store.config, &id)?;
    store.config.forget_custom_agent(&id);
    store.save_config().map_err(not_kept)
}

/// Whether an id names something this machine can be asked to start — a catalog row, or a command
/// registered on this device. The guard on every id that is written down as an answer.
fn known(config: &amenbo_core::config::Config, agent: &str) -> Result<(), CmdError> {
    if wake::started_as(agent).is_some() || config.custom_agent(agent).is_some() {
        return Ok(());
    }
    Err(CmdError::coded(
        "wake_unknown_agent",
        "That is not an agent Amenbo knows how to start.",
        serde_json::json!({ "agent": agent }),
    ))
}

/// The registered row an id names, or the refusal for an id nothing is registered under — what the
/// two doors that change a registration ask before they change one.
fn registered<'a>(
    config: &'a amenbo_core::config::Config,
    id: &str,
) -> Result<&'a amenbo_core::config::CustomAgent, CmdError> {
    config.custom_agent(id).ok_or_else(|| {
        CmdError::coded(
            "wake_unknown_agent",
            "That is not an agent Amenbo knows how to start.",
            serde_json::json!({ "agent": id }),
        )
    })
}

/// The refusal for a registration with a half missing — no name to draw it by, or no command line
/// to start.
fn badly_written(e: amenbo_core::error::Error) -> CmdError {
    CmdError::coded(
        "wake_not_registered",
        format!("That command could not be registered: {e}"),
        serde_json::json!({ "reason": e.to_string() }),
    )
}

/// The refusal for a choice that reached the settings and did not land.
fn not_kept(e: amenbo_core::error::Error) -> CmdError {
    CmdError::coded(
        "wake_not_kept",
        format!("The choice could not be saved: {e}"),
        serde_json::json!({ "reason": e.to_string() }),
    )
}

/// One candidate as the face draws it.
fn row(one: &wake::Candidate) -> WakeCandidateDto {
    WakeCandidateDto {
        id: one.id.clone(),
        label: one.label.clone(),
        command: one.command.clone(),
        line: one.line.clone(),
        traced: one.traced,
        installed: one.installed,
    }
}

/// The folder a probe is about, canonical.
///
/// There is no fallback. A pane opens where the person said it should, and the folder is theirs from
/// the moment they chose it (`app/src/talk/agent.ts`) — so anything this could put in place of a
/// missing one would be a probe of a folder the terminal is not in.
///
/// A folder that is not there is refused rather than answered about — the answer would be "nothing
/// is traced here", which reads as a folder with no agent rather than as a folder that is gone.
fn resolve(folder: String) -> Result<PathBuf, CmdError> {
    let named = PathBuf::from(folder);
    std::fs::canonicalize(&named).map_err(|e| unreachable_folder(&named, &e))
}


/// The refusal for a folder that could not be resolved on this machine.
fn unreachable_folder(folder: &Path, e: &std::io::Error) -> CmdError {
    CmdError::coded(
        "wake_no_folder",
        format!("That folder could not be read: {e}"),
        serde_json::json!({ "folder": folder.to_string_lossy(), "reason": e.to_string() }),
    )
}

/// This device's configuration — where a folder's remembered answer is kept.
fn config() -> Result<amenbo_core::config::Config, CmdError> {
    Ok(amenbo_core::config::Config::load(&paths()?.config_file))
}

/// Where this install keeps its files, or the refusal for an install that cannot say.
fn paths() -> Result<amenbo_core::config::Paths, CmdError> {
    amenbo_core::config::Paths::resolve().map_err(|e| {
        CmdError::coded(
            "wake_no_config",
            format!("Amenbo could not find its own files: {e}"),
            serde_json::json!({ "reason": e.to_string() }),
        )
    })
}
