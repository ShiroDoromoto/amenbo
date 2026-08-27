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
//! **What this machine can start is remembered, and only the first window pays for it.** The probe
//! is a login shell reading the reader's own profile, so it costs whatever that profile costs and is
//! abandoned at a deadline — pay it on every window and a machine whose profile waits on a network
//! reports itself bare. So the answer is written to the device's settings
//! ([`amenbo_core::config::Config::installed_agents`]) and the windows after the first come up on it,
//! while a fresh probe runs behind them and says `agents-installed` when what it found differs. A
//! probe that could not be run writes nothing: "nobody asked" must not be recorded as "nothing is
//! here" (`AMB-D-792`).
//!
//! **Not on the perf budget, deliberately.** The first probe's time is that same login shell, so it
//! busts a 50 ms budget on every machine and nothing in Amenbo can make it not. A WARN that fires
//! when a window opens and names nothing anyone can act on is noise in the one log that is meant to
//! be read.
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
use tauri::Emitter as _;

use crate::dto::{WakeCandidateDto, WakeDto, WakeReachDto};
use crate::error::CmdError;
use crate::launch::Probe;

/// Said when a fresh probe found something other than what was remembered, so a face drawing the
/// remembered answer knows to ask again.
///
/// **Being told is the whole of it.** The payload carries no rows: what changed is the device's
/// settings, and every window reads those through the question it already asks, so the one thing to
/// do with this is put that question again (the shape `folder-changed` takes, `AMB-D-785`).
const REFRESHED_EVENT: &str = "agents-installed";

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
pub fn wake_probe(
    app: tauri::AppHandle,
    folder: String,
    project: Option<i64>,
) -> Result<WakeDto, CmdError> {
    let folder = resolve(folder)?;
    let found = amenbo_core::harness::probe(&folder, amenbo_core::config::Paths::command_name());
    let config = config()?;
    let (candidates, reach) = weighed(&app, &found, config.custom_agents());
    answer(
        Some(folder.to_string_lossy().into_owned()),
        candidates,
        reach,
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
pub fn wake_choices(
    app: tauri::AppHandle,
    project: Option<i64>,
    folders: Vec<String>,
) -> Result<WakeDto, CmdError> {
    let command = amenbo_core::config::Paths::command_name();
    let found: Vec<amenbo_core::harness::Wiring> = folders
        .iter()
        .filter_map(|one| std::fs::canonicalize(one).ok())
        .flat_map(|one| amenbo_core::harness::probe(&one, command))
        .collect();
    let config = config()?;
    let (candidates, reach) = weighed(&app, &found, config.custom_agents());
    answer(None, candidates, reach, project, &config)
}

/// Ask this machine again, now, and keep what it says — the **search again** the face puts up where
/// the answer could not be got (`AMB-D-792`).
///
/// It answers whether the machine could be reached rather than what was found: what was found has
/// gone to the settings and out as [`REFRESHED_EVENT`], which is what every open window is already
/// listening for, so a press that succeeded needs nothing back but the news that it did. `false` is
/// the state to keep drawing — the shell would not start, or was still reading the profile when the
/// deadline ran out.
#[tauri::command]
pub fn wake_rescan(app: tauri::AppHandle) -> Result<bool, CmdError> {
    let registered = config().map(|one| one.custom_agents().to_vec()).unwrap_or_default();
    Ok(matches!(refresh(&app, &registered), Probe::Found(_)))
}

/// Every provider Amenbo can start — the catalog's and this device's own — told apart by what this
/// machine can start.
///
/// One probe answers for all of them, registered rows included: what is asked about a registration
/// is the first word of its line ([`amenbo_core::config::CustomAgent::command`]), which is the only
/// part of it that names something to look for.
///
/// **The remembered answer is what this draws on**, and the machine is asked again behind it. Only
/// a machine that has never been asked is waited for — there is nothing to draw until it answers,
/// and drawing nothing would say the machine is bare. What that first ask finds is kept, so it is
/// the only window that ever waits.
fn weighed(
    app: &tauri::AppHandle,
    found: &[amenbo_core::harness::Wiring],
    registered: &[amenbo_core::config::CustomAgent],
) -> (Vec<wake::Candidate>, WakeReachDto) {
    let (installed, reach) = match remembered() {
        // A remembered answer is an answer. That the probe running behind this one may fail is not
        // the reader's problem — they are looking at what this machine said last time, which is
        // still the truest thing anybody has.
        Some(kept) => {
            // Behind the answer, not in front of it: this window is already drawn by the time the
            // shell has finished reading the profile.
            let app = app.clone();
            let registered = registered.to_vec();
            std::thread::spawn(move || refresh(&app, &registered));
            (kept, WakeReachDto::Answered)
        }
        // Nothing to come up on, so this one window pays for the probe — and if it comes back with
        // nothing learned, that is what the face is told. Every row says `installed: false` either
        // way, which is why the reading cannot be left to the rows.
        None => match refresh(app, registered) {
            Probe::Found(fresh) => (fresh, WakeReachDto::Answered),
            Probe::Unreachable => (Vec::new(), WakeReachDto::Unreachable),
        },
    };
    let candidates = wake::candidates(found, registered, |cmd| {
        installed.iter().any(|one| one == cmd)
    });
    (candidates, reach)
}

/// What the last probe found, where this machine has been asked at all.
fn remembered() -> Option<Vec<String>> {
    config().ok()?.installed_agents().map(<[String]>::to_vec)
}

/// Ask this machine, keep the answer, and say so where it differs from what was kept.
///
/// **An unreachable machine leaves the settings alone.** Writing an empty list for it would record
/// "nothing is installed", which is the lie the remembering exists to end
/// ([`amenbo_core::config::Config::installed_agents`]).
///
/// A write is a whole-file rewrite of the device's settings, so it goes through the store for the
/// reason [`wake_remember`]'s does. Nothing is written where nothing changed: the common case is the
/// same answer as last time, and rewriting the file for it would put a write behind every window.
fn refresh(
    app: &tauri::AppHandle,
    registered: &[amenbo_core::config::CustomAgent],
) -> Probe {
    let mut commands: Vec<&str> = amenbo_core::harness::LAUNCHES
        .iter()
        .map(|one| one.command)
        .collect();
    commands.extend(registered.iter().map(|one| one.command()));
    let probe = crate::launch::installed(&commands);
    let Probe::Found(fresh) = &probe else {
        return probe;
    };
    if remembered().as_deref() == Some(fresh.as_slice()) {
        return probe;
    }
    if keep(fresh).is_ok() {
        let _ = app.emit(REFRESHED_EVENT, ());
    }
    probe
}

/// Write what a probe found to the device's settings.
fn keep(found: &[String]) -> Result<(), CmdError> {
    crate::migrate::gate()?;
    let mut store = amenbo_core::Store::open_at(paths()?).map_err(not_kept)?;
    store.config.remember_installed(found);
    store.save_config().map_err(not_kept)
}

/// The candidates and the project's answer, in the shape a face reads.
///
/// `offered` is the row and `settled` is what a press opens with, and the two are drawn from
/// different lists on purpose ([`amenbo_core::wake::offered`] against
/// [`amenbo_core::wake::startable`]): the row names every agent so an uninstalled one can be seen
/// and installed, while the answer is only ever one this machine can start.
fn answer(
    folder: Option<String>,
    candidates: Vec<wake::Candidate>,
    reach: WakeReachDto,
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
        reach,
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
