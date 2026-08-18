//! The hourly tick: the one plain timer amenbo asks this machine's scheduler to hold (`AMB-D-707`),
//! and whether it may ask at all.
//!
//! **What is registered carries no meaning.** It wakes amenbo once an hour, and amenbo decides once
//! awake what is due. That is the whole reason there is one: a second use never becomes a second
//! timer, so the path that rewrites the OS's own settings every time amenbo grows does not exist. The
//! user sees one row, and switching it off stops everything behind it.
//!
//! Registering writes into the machine's scheduler, which amenbo does not do unasked, so it asks —
//! **once for the tick as a feature, on this device** ([`TickConsent`], kept in
//! [`crate::config::Config::tick_consent`]). The same shape as the lint's [`crate::hooks::HookConsent`],
//! and for the same reason: nobody wants the tick on Tuesdays but not Wednesdays, so asking more than
//! once is repeating a question whose answer is already known. There is no scale below the device here
//! — one machine holds one timer — so the lint's per-repository opt-out has no counterpart, and
//! [`crate::config::Config::tick_consent`] is the only record there is.
//!
//! **The answer and the registration are two independent facts.** The answer says what was consented
//! to and never what the scheduler holds, which is [`probe`]'s answer and is read from the OS every
//! time. The row is the user's to switch off from their own settings, and an answer read as a mirror
//! of the OS would leave amenbo claiming a timer that is not there. The two meet in exactly one place,
//! [`fix_for`].
//!
//! **What goes inside a registration is per-OS.** A plist, a scheduler task and a systemd unit are not
//! one shape with three spellings, so each OS gets its own door ([`platform`]). Three points from
//! `AMB-D-707` are not writing style but whether the premise holds at all, and every door has to meet
//! them: macOS registers through `SMAppService` so the row carries amenbo's name rather than the
//! developer's; all three need the missed-run setting turned on explicitly, none having it by default;
//! and Windows has to have both battery gates turned off, or a laptop off its charger runs no tick at
//! all.
//!
//! **A door can also have a say in *who* may open it**, which macOS does — see [`reachable_from_here`]
//! and [`relaunch_target`]. Everywhere else, whoever can reach the machine can work the door.

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// The answer on record — **one for the device, given once and never asked for again**. There is no
/// `Unanswered` variant: never having answered is the *absence* of an answer (`Option::None`), which is
/// what makes "asked and refused" different from "never asked".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TickConsent {
    /// Hold the timer. It is one registration for the machine, so there is nowhere else for this yes to
    /// reach and nothing further to ask.
    Yes,
    /// Do not hold it, and do not ask again. It answers the question for good and forbids nothing: an
    /// explicit `tick install` asked for later is still honoured.
    No,
}

/// What a pass has to do to bring the answer and the registration back into agreement — the drift
/// table as a value, walked row by row in [`fix_for`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickFix {
    /// They already agree.
    Nothing,
    /// Wanted, and something is registered: write it again, so it names the executable running now. A
    /// registration pointing anywhere else wakes nothing, and says nothing about having failed.
    Rewrite,
    /// Wanted, and nothing is registered: the user switched the row off where they can see it, and the
    /// answer follows rather than putting the timer back behind them.
    TakeTheAnswerBack,
    /// Not wanted, and something is registered: take it away.
    Deregister,
}

/// Read the answer and the scheduler against each other and say what to do.
///
/// Registered-and-current is not told apart from registered-and-stale, and need not be: a scheduler
/// answers whether it holds the registration, not what it points at, so telling the two apart would
/// mean parsing a plist, an XML task and a unit file back. [`TickFix::Rewrite`] covers both — it is
/// what a stale registration needs and what a current one already says — which is how a build that has
/// moved gets the timer pointed at it again. The cost is one write per startup, once a door exists.
///
/// A registration the user removed is **not** written back. The lint makes the opposite call, and the
/// difference is what the two features are: a git hook is amenbo's own file in a repository's plumbing,
/// while the tick is a row in the user's system settings with a switch on it. Undoing that switch from
/// under them would make it not a switch — so the answer follows what they can see
/// ([`TickFix::TakeTheAnswerBack`]), and turning the tick back on is `tick install`.
///
/// Never having been asked leaves everything alone, registration included. Adopting one as a yes would
/// record an answer nobody gave, and taking it away would remove a timer the user may well have asked
/// an older build for; the question is still live either way, and it is the explicit faces that put it.
pub fn fix_for(consent: Option<TickConsent>, registered: bool) -> TickFix {
    match (consent, registered) {
        (Some(TickConsent::Yes), true) => TickFix::Rewrite,
        (Some(TickConsent::Yes), false) => TickFix::TakeTheAnswerBack,
        (Some(TickConsent::No), true) => TickFix::Deregister,
        (Some(TickConsent::No), false) | (None, _) => TickFix::Nothing,
    }
}

/// Whether this build has a door into this machine's scheduler at all.
///
/// False is not a failure and not a refusal to answer: it is the honest state of a target amenbo has
/// not learned to register on, and every face here says so rather than half-doing the work. While it is
/// false [`probe`] is `false`, and [`register`] and [`unregister`] refuse.
pub fn available() -> bool {
    platform::available()
}

/// Can *this process* work the door, or only a differently-launched one?
///
/// On macOS the door is the app bundle's (see [`platform`]), so a process launched from outside one
/// cannot open it — which the CLI on `PATH` is, being a symlink into the bundle rather than the
/// bundle's own executable. It is not a failure and not a refusal: the same machine, asked by the
/// bundle's executable, answers normally. A caller that can re-launch itself from inside the bundle
/// does that and asks again; one that cannot says which process has to ask.
///
/// Everywhere else the door has no such requirement, and this is true whenever [`available`] is.
pub fn reachable_from_here() -> bool {
    platform::reachable_from_here()
}

/// The executable to launch so that the door *is* reachable, for a caller that can launch one.
///
/// On macOS this is the bundle's own copy of whatever is running now: the same binary, named by the path
/// it really sits at rather than by the symlink that reached it, which is the whole of what
/// [`reachable_from_here`] turns on. `None` where re-launching would not help — every other target, and a
/// macOS process whose executable is not inside a bundle carrying the plist. That `None` is what makes a
/// re-launch loop impossible: the caller only ever launches something that will answer.
pub fn relaunch_target() -> Option<std::path::PathBuf> {
    platform::relaunch_target()
}

/// Is the scheduler holding amenbo's tick right now? Read from the OS, every time — never from the
/// answer on record (see the module docs).
pub fn probe() -> Result<bool> {
    platform::probe()
}

/// Write the registration, or write it again over one that is already there.
///
/// **Idempotent by contract**: running it twice leaves one registration and no error. It is what makes
/// [`TickFix::Rewrite`] a single move rather than a remove-then-add with a window in between, and what
/// lets an upgrade point the timer at the build running now without first asking what the old one said.
pub fn register() -> Result<()> {
    platform::register()
}

/// Take the registration away. Idempotent in the same way: with nothing registered it succeeds, having
/// left the machine in the state the caller asked for.
pub fn unregister() -> Result<()> {
    platform::unregister()
}

/// The door into this machine's scheduler: `SMAppService`, over an agent plist carried in the app
/// bundle (`AMB-D-707`).
///
/// **Why the bundle and not `~/Library/LaunchAgents`.** A plist written straight into that directory
/// belongs to no bundle, so macOS files it as a legacy agent and lists it under the developer's name
/// (`AMB-D-549`). A user looking for amenbo in their settings does not find it, and `AMB-D-707`'s
/// "one row, and switching it off stops everything" goes with it. An agent registered through
/// `SMAppService` is listed under the app.
///
/// **The cost is that only the bundle can open the door.** `SMAppService` reads the plist out of the
/// *calling process's* main bundle, and a process launched through the symlink that puts the CLI on
/// `PATH` has no main bundle to read from — macOS resolves it to the directory the symlink sits in,
/// not to the app the symlink points into. Hence [`reachable_from_here`]: from inside the bundle the
/// three faces work, and from outside it a caller re-launches the bundle's own executable and asks
/// again ([`crate::tick::reachable_from_here`]).
#[cfg(target_os = "macos")]
mod platform {
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

    /// `SMAppService` arrived in macOS 13, and the framework holding it is old enough that only the
    /// class itself can answer whether this system has it.
    pub(super) fn available() -> bool {
        objc2::runtime::AnyClass::get(c"SMAppService").is_some()
    }

    /// The bundle this process was launched from, when it carries our agent plist.
    fn bundle_with_plist() -> Option<PathBuf> {
        let path = PathBuf::from(NSBundle::mainBundle().bundlePath().to_string());
        path.join(PLIST_IN_BUNDLE).is_file().then_some(path)
    }

    pub(super) fn reachable_from_here() -> bool {
        available() && bundle_with_plist().is_some()
    }

    /// This executable, at the path it really sits at, when that path is inside a bundle carrying the
    /// plist. The symlink that puts the CLI on `PATH` is what has to come off, and that is the whole of
    /// what this does: launched by the resolved path, the process has the bundle as its main bundle and
    /// the door opens. `current_exe` on this platform hands back the path the process was launched by,
    /// symlink and all, so the resolving is asked for here rather than assumed.
    pub(super) fn relaunch_target() -> Option<PathBuf> {
        if !available() || bundle_with_plist().is_some() {
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
}

/// The door into this machine's scheduler, on the targets that have none written yet.
///
/// What goes into a registration is per-OS in a way the rest of this module is deliberately not — the
/// Windows task has to be built from XML because `schtasks` carries no flag for the battery gates, and
/// Linux writes a pair of user units — so each door lands with the OS that needs it (`AMB-T-3254` /
/// `AMB-T-3255`) rather than being guessed at from here.
#[cfg(not(target_os = "macos"))]
mod platform {
    use crate::error::{Error, Result};

    /// What every face here says while there is no door: the plain fact, and no hint, because there is
    /// nothing the reader could type to fix it.
    const NO_DOOR: &str = "amenbo cannot register the hourly tick on this system yet";

    pub(super) fn available() -> bool {
        false
    }

    /// Nothing is out of reach on a target where nothing is in reach: the honest answer about *where*
    /// the door can be opened from is that there is no door, which [`available`] already says.
    pub(super) fn reachable_from_here() -> bool {
        false
    }

    /// There is nothing to launch that would answer differently.
    pub(super) fn relaunch_target() -> Option<std::path::PathBuf> {
        None
    }

    pub(super) fn probe() -> Result<bool> {
        Ok(false)
    }

    pub(super) fn register() -> Result<()> {
        Err(Error::invalid(NO_DOOR))
    }

    pub(super) fn unregister() -> Result<()> {
        Err(Error::invalid(NO_DOOR))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The drift table in full, so a later reading of it has to move a row on purpose.
    #[test]
    fn the_answer_and_the_registration_settle_the_way_the_user_can_see() {
        // Wanted and held: written again, because a registration naming a place this build has moved
        // out of wakes nothing and says nothing about having failed.
        assert_eq!(fix_for(Some(TickConsent::Yes), true), TickFix::Rewrite);
        // Wanted but gone: the user switched the row off, and the answer follows what they can see.
        assert_eq!(fix_for(Some(TickConsent::Yes), false), TickFix::TakeTheAnswerBack);
        // Refused but held: left over from an answer that has since changed, and it goes.
        assert_eq!(fix_for(Some(TickConsent::No), true), TickFix::Deregister);
        assert_eq!(fix_for(Some(TickConsent::No), false), TickFix::Nothing);
    }

    /// Never asked is left alone, whatever the scheduler holds: neither reading a registration as a
    /// yes nor taking one away is something an unanswered question licenses.
    #[test]
    fn an_unasked_device_is_left_exactly_as_it_is() {
        assert_eq!(fix_for(None, false), TickFix::Nothing);
        assert_eq!(fix_for(None, true), TickFix::Nothing);
    }

    /// With no door on this target, the state is readable and the two writes refuse — the reason a
    /// caller can say so, rather than reporting a registration that was never written.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn a_target_with_no_door_answers_rather_than_pretending() {
        assert!(!available());
        assert!(!reachable_from_here());
        assert!(!probe().expect("a target with no door still has a state to report"));
        assert!(register().is_err());
        assert!(unregister().is_err());
    }

    /// macOS has the door, and a test binary is exactly the caller that cannot open it: it runs from
    /// the target directory rather than from inside the app bundle the plist is carried in. All three
    /// faces refuse rather than answering about a bundle they are not in — including the read, whose
    /// alternative would be reporting "nothing registered" about a machine it never asked.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_bundles_door_does_not_open_for_a_process_outside_it() {
        assert!(available(), "every macOS this builds for has SMAppService");
        assert!(!reachable_from_here());
        assert!(probe().is_err());
        assert!(register().is_err());
        assert!(unregister().is_err());
    }
}
