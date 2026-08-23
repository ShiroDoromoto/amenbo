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
//! **Not on the perf budget, deliberately.** A probe's time is a login shell reading the reader's
//! own profile ([`crate::launch::installed`]), so it busts a 50 ms budget on every machine and
//! nothing in Amenbo can make it not. A WARN that fires every time a window opens and names nothing
//! anyone can act on is noise in the one log that is meant to be read.
//!
//! **The webview never names a program.** What crosses is a catalogued id; the command it becomes is
//! read out of [`amenbo_core::harness::HARNESSES`] on this side. A pane is a shell with a command
//! line, so an id that turned into whatever string arrived would be a shell injection with a webview
//! at the other end of it.

use std::path::{Path, PathBuf};

use amenbo_core::wake::{self, Choice};

use crate::dto::{WakeCandidateDto, WakeDto};
use crate::error::CmdError;

/// What a folder says, what this machine says, and the answer the two come to.
///
/// `folder` is where the pane would open, and it is asked for rather than defaulted: a pane is opened
/// in the folder the person chose (`app/src/talk/agent.ts`), so there is no such thing here as a probe
/// about nowhere in particular.
#[tauri::command]
pub fn wake_probe(folder: String) -> Result<WakeDto, CmdError> {
    let folder = resolve(folder)?;
    let found = amenbo_core::harness::probe(&folder, amenbo_core::config::Paths::command_name());
    let commands: Vec<&str> = amenbo_core::harness::HARNESSES
        .iter()
        .map(|h| h.command)
        .collect();
    let installed = crate::launch::installed(&commands);
    let candidates = wake::candidates(&found, |cmd| installed.iter().any(|one| one == cmd));

    let config = config()?;
    let settled = match wake::settle(config.agent_for(&folder), &candidates) {
        Choice::Settled(id) => Some(id.to_string()),
        Choice::Ask(_) | Choice::Nothing => None,
    };
    Ok(WakeDto {
        folder: folder.to_string_lossy().into_owned(),
        offered: wake::offered(&candidates)
            .iter()
            .map(|one| one.id.to_string())
            .collect(),
        candidates: candidates.iter().map(row).collect(),
        settled,
    })
}

/// Keep this folder's answer, so the offer is not put again the next time a pane opens here.
///
/// The id is checked against the catalog rather than trusted, because what is written here is read
/// back as the thing to start.
///
/// **Written through the store, though it is read without one.** A write is a whole-file rewrite of
/// the device's settings, so one built on a copy read minutes ago would carry back whatever the
/// settings screen changed in between; the store is what serializes that. Reading is the other way
/// round — it loses nothing and happens every time a window opens — so [`wake_probe`] takes the
/// cheap road, the same one `ui_language` takes.
#[tauri::command]
pub fn wake_remember(folder: String, agent: String) -> Result<(), CmdError> {
    if wake::started_as(&agent).is_none() {
        return Err(CmdError::coded(
            "wake_unknown_agent",
            "That is not an agent Amenbo knows how to start.",
            serde_json::json!({ "agent": agent }),
        ));
    }
    let folder = resolve(folder)?;
    crate::migrate::gate()?;
    let mut store = amenbo_core::Store::open_at(paths()?).map_err(not_kept)?;
    store.config.remember_agent(&folder, &agent);
    store.save_config().map_err(not_kept)
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
        id: one.id.to_string(),
        label: one.label.to_string(),
        command: one.command.to_string(),
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
