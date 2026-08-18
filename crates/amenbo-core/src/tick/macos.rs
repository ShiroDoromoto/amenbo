//! The door into this machine's scheduler: `SMAppService`, over an agent plist carried in the app
//! bundle (`AMB-D-707`).
//!
//! **Why the bundle and not `~/Library/LaunchAgents`.** A plist written straight into that directory
//! belongs to no bundle, so macOS files it as a legacy agent and lists it under the developer's name
//! (`AMB-D-549`). A user looking for amenbo in their settings does not find it, and `AMB-D-707`'s
//! "one row, and switching it off stops everything" goes with it. An agent registered through
//! `SMAppService` is listed under the app.
//!
//! **The cost is that only the bundle can open the door.** `SMAppService` reads the plist out of the
//! *calling process's* main bundle, and a process launched through the symlink that puts the CLI on
//! `PATH` has no main bundle to read from — macOS resolves it to the directory the symlink sits in,
//! not to the app the symlink points into. Hence [`reachable_from_here`]: from inside the bundle the
//! three faces work, and from outside it a caller re-launches the bundle's own executable and asks
//! again ([`crate::tick::reachable_from_here`]).

use std::path::PathBuf;

use objc2_foundation::{NSBundle, NSString};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::error::{Error, Result};

/// The agent plist's name inside the bundle, under `Contents/Library/LaunchAgents/`. It is what
/// `SMAppService` is asked for, and the file itself carries the schedule — bundled, so signed,
/// so fixed at build time. `AMB-D-707` is what makes that harmless: the tick is plain and hourly,
/// and there is no reason for a build to want a different cadence than the one before it.
const PLIST: &str = "work.amenbo.tick.plist";

/// Where the plist sits, relative to the bundle root — the one path this module needs to look at
/// itself, to answer whether the process is running from inside a bundle that carries it.
const PLIST_IN_BUNDLE: &str = "Contents/Library/LaunchAgents/work.amenbo.tick.plist";

/// macOS has a door.
pub(super) const AVAILABLE: bool = true;

/// `SMAppService` arrived in macOS 13, and the framework holding it is old enough that only the class
/// itself can answer whether the system in front of us has it.
fn class_is_there() -> bool {
    objc2::runtime::AnyClass::get(c"SMAppService").is_some()
}

/// The bundle this process was launched from, when it carries our agent plist.
fn bundle_with_plist() -> Option<PathBuf> {
    let path = PathBuf::from(NSBundle::mainBundle().bundlePath().to_string());
    path.join(PLIST_IN_BUNDLE).is_file().then_some(path)
}

pub(super) fn reachable_from_here() -> bool {
    class_is_there() && bundle_with_plist().is_some()
}

/// This executable, at the path it really sits at, when that path is inside a bundle carrying the
/// plist. The symlink that puts the CLI on `PATH` is what has to come off, and that is the whole of
/// what this does: launched by the resolved path, the process has the bundle as its main bundle and
/// the door opens. `current_exe` on this platform hands back the path the process was launched by,
/// symlink and all, so the resolving is asked for here rather than assumed.
pub(super) fn relaunch_target() -> Option<PathBuf> {
    if !class_is_there() || bundle_with_plist().is_some() {
        return None;
    }
    let exe = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
    let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"))?;
    bundle.join(PLIST_IN_BUNDLE).is_file().then_some(exe)
}

/// The agent, as `SMAppService` names it. Only ever called where [`reachable_from_here`] holds.
fn service() -> objc2::rc::Retained<SMAppService> {
    unsafe { SMAppService::agentServiceWithPlistName(&NSString::from_str(PLIST)) }
}

/// What a caller outside the bundle is told. It names the process that has to ask rather than a
/// command to type, because which one that is depends on who is asking.
fn out_of_bundle() -> Error {
    Error::invalid(
        "the hourly tick is registered by amenbo.app itself, and this process is not running from inside it",
    )
}

pub(super) fn probe() -> Result<bool> {
    if !reachable_from_here() {
        return Err(out_of_bundle());
    }
    // Enabled is the only status that means the tick actually fires. A user who switches the row
    // off in System Settings leaves one that is neither enabled nor gone, and what they can see is
    // off — the same reading the login registration takes (`AMB-D-546`).
    Ok(unsafe { service().status() } == SMAppServiceStatus::Enabled)
}

pub(super) fn register() -> Result<()> {
    if !reachable_from_here() {
        return Err(out_of_bundle());
    }
    let service = service();
    match unsafe { service.registerAndReturnError() } {
        Ok(()) => Ok(()),
        // Registering what is already registered is answered as a failure by macOS, and is not
        // one: what the caller wanted is what the OS holds. Only a status that disagrees is an
        // error — which is what makes this idempotent.
        Err(_) if unsafe { service.status() } == SMAppServiceStatus::Enabled => Ok(()),
        Err(e) => Err(Error::invalid(format!(
            "the hourly tick could not be registered: {}",
            e.localizedDescription()
        ))),
    }
}

pub(super) fn unregister() -> Result<()> {
    if !reachable_from_here() {
        return Err(out_of_bundle());
    }
    let service = service();
    match unsafe { service.unregisterAndReturnError() } {
        Ok(()) => Ok(()),
        // The mirror of register's: nothing registered is the state the caller asked for.
        Err(_) if unsafe { service.status() } != SMAppServiceStatus::Enabled => Ok(()),
        Err(e) => Err(Error::invalid(format!(
            "the hourly tick could not be taken away: {}",
            e.localizedDescription()
        ))),
    }
}
