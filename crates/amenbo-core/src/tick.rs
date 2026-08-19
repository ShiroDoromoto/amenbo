//! The hourly tick: the one plain timer amenbo asks this machine's scheduler to hold (`AMB-D-707`),
//! whether it may ask at all, and what amenbo does once that timer wakes it (`AMB-D-706`).
//!
//! Two halves, joined by nothing but the hour:
//!
//! | half | what it is |
//! |---|---|
//! | whether we are woken | the answer on record ([`TickConsent`]), what the scheduler holds ([`probe`]), the rule that settles the two ([`fix_for`]), and whether there is a question to put at all ([`banner_shows`]) |
//! | what is done once awake | the declaration table ([`PURPOSES`]), the day mark that holds a purpose to one turn a day ([`once_a_day`]), and the entry that walks them ([`run`]) |
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
//! **Where the question is put is the app, and it is put in a banner** (`AMB-D-718`). Whether that banner
//! has anything to ask today is [`banner_shows`] — the same shape as the answer itself, a judgement made
//! here and drawn there.
//!
//! **The answer and the registration are two independent facts.** The answer says what was consented
//! to and never what the scheduler holds, which is [`probe`]'s answer and is read from the OS every
//! time. The row is the user's to switch off from their own settings, and an answer read as a mirror
//! of the OS would leave amenbo claiming a timer that is not there. The two meet in exactly one place,
//! [`fix_for`].
//!
//! **What goes inside a registration is not here.** A plist, a scheduler task and a systemd unit are
//! not one shape with three spellings, so each OS writes its own, and the build picks one. Three points from
//! `AMB-D-707` are not writing style but whether the premise holds at all, and every door has to meet
//! them: macOS registers through `SMAppService` so the row carries amenbo's name rather than the
//! developer's; all three need the missed-run setting turned on explicitly, none having it by default;
//! and Windows has to have both battery gates turned off, or a laptop off its charger runs no tick at
//! all.
//!
//! **What it is registered *as* is one rule for every door** (`registration_name`, which only the
//! targets that have a door compile): production keeps the plain name, and every other build carries
//! its channel in it. A scheduler namespace is one per user and not one per build, so two amenbos on
//! a machine — production beside the shared dev build, or two per-task instances — would otherwise
//! be handed the same row to fight over.
//!
//! **A door can also have a say in *who* may open it**, which macOS does — see [`reachable_from_here`]
//! and [`relaunch_target`]. Everywhere else, whoever can reach the machine can work the door.
//!
//! **Being woken is not being due.** The timer carries no meaning, so an hour that is owed nothing is the
//! ordinary case and [`PURPOSES`] is where an hour's work is decided. What is owed is counted in calendar
//! days rather than in wake-ups (`AMB-D-708`): a machine that was asleep is woken for the hours it missed,
//! and anything counted per wake-up rings several times over on the day it comes back.
//!
//! **The queues are worked on every tick, including the ones with nothing to say** (`AMB-D-706`). A runner
//! killed mid-queue leaves its rows standing until the next write drives delivery again, and the writes a
//! daily purpose makes are a day apart — so the tick with nothing to emit is exactly the one that should
//! carry what a previous run left behind ([`crate::plugin_drive`]). It works them **in this process**: the
//! tick is a process the scheduler started for this and nothing else, so there is no command being made to
//! wait, and handing the queues to runners nobody watches would leave it with nothing to report.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::plugin_manifest::{Face, Scope};
use crate::plugin_runner::{Waiting, Worked};
use crate::plugin_subscribe::{EnabledSubscribers, InstalledPlugin};
use crate::store::Store;
use crate::store_engine::StoreEngine;
use crate::time::date_to_string;

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
    /// answer follows rather than putting the timer back behind them. It follows all the way to
    /// **unanswered**, not to a `no` (`AMB-D-718`): a `no` silences the offer as well as the timer, and
    /// would leave the way back open only to someone who knows `tick install` is there.
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
/// ([`TickFix::TakeTheAnswerBack`]), back to unanswered, which is what leaves the offer able to be put
/// again where they can see it (`AMB-D-718`). Asking for the timer outright is still `tick install`.
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
///
/// It is a fact about the **build**, not about the machine: a Linux without systemd on it still has a
/// door compiled in, and what it does not have is something behind the door — which is [`probe`]'s
/// answer and the writes' refusal, said where the machine is actually asked.
pub fn available() -> bool {
    platform::AVAILABLE
}

/// Whether taking the registration away still leaves the user a row to look at.
///
/// macOS is the one that does. It keeps its own record of a background item, and `unregister` does not
/// reach it: the row stays in Login Items with the toggle reading as allowed, while nothing runs behind
/// it. There is no further move for amenbo to make — so this is here to be *said*, at the moment the
/// registration is taken away, rather than to be acted on. Left unsaid, the row reads as "it did not
/// work"; said first, it is just how the OS keeps its list.
///
/// A fact about the **build**, like [`available`], and for the same reason: which door this binary was
/// compiled against is fixed long before it is asked.
pub fn removal_leaves_a_row() -> bool {
    platform::REMOVAL_LEAVES_A_ROW
}

/// The plain name production registers under on Linux and Windows — the one word the units and the
/// task are named from.
#[cfg(any(target_os = "linux", windows, test))]
const TICK_NAME: &str = "amenbo-tick";

/// The plain label production files the agent under on macOS — reverse-DNS, because that is what a
/// launchd label is, and the plist in the bundle is named for it.
#[cfg(any(target_os = "macos", test))]
const TICK_LABEL: &str = "work.amenbo.tick";

/// What *this build* registers under: `base` on production, and `base` carrying this build's
/// channel behind `separator` everywhere else. Each door passes its own two — a systemd unit, a
/// scheduler task and a launchd label are spelled differently and named by the same rule.
///
/// A machine holds one scheduler namespace per user — `~/.config/systemd/user`, the Task Scheduler,
/// and macOS's record of background items — so two amenbos naming their registration the same word
/// do not get one each: the second write lands on the first's row, and the build that lost it is
/// left with a registration that reads as held while pointing at somebody else's executable.
/// Nothing in the probe can tell that apart, [`TickFix::Rewrite`] being deliberately blind to what a
/// registration points at. Production beside the shared dev build is this repository's ordinary day,
/// and two per-task instances (`AMB-T-ID=<id>`) are the same collision again.
///
/// **The production names are untouched**, so an upgrade writes over the row an older build
/// registered rather than leaving it behind next to a new one.
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
fn registration_name(base: &str, separator: char) -> String {
    registration_name_for(base, separator, crate::config::Paths::APP_NAME)
}

/// The rule [`registration_name`] applies, taking the channel as an argument for the reason
/// [`crate::config::Paths::is_dev_app_name`] does — a running binary's channel is fixed at compile
/// time, so only a table can pin what each name maps to.
#[cfg(any(target_os = "macos", target_os = "linux", windows, test))]
fn registration_name_for(base: &str, separator: char, app_name: &str) -> String {
    if app_name == crate::config::Paths::PRODUCTION_APP_NAME {
        base.to_string()
    } else {
        format!("{base}{separator}{app_name}")
    }
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

/// Settle the answer on record against what the scheduler holds, once — [`fix_for`] with its hands
/// attached. What comes out is what the record has to become: `None` means it still says what it said,
/// which is the overwhelmingly common case, and `Some(answer)` is what to write in its place — with
/// `Some(None)` the device put back to never having answered (`AMB-D-718`). That inner shape is
/// [`crate::config::Config::tick_consent`]'s own, so a caller writes across what it is handed rather
/// than reading the move out of it.
///
/// **Persisting is the caller's**, because where the answer is kept is: the CLI carries it on the store
/// it already has open, and the app on the config it loaded as it came up. What is here is the part they
/// must not answer differently.
///
/// Three things end it before the scheduler is asked at all, and each is a process not spent:
///
/// 1. **No door on this target** — there is nothing to settle against.
/// 2. **Nobody has answered** — [`fix_for`] says `Nothing` for an unanswered device whatever the
///    scheduler holds, and that is most machines, on every startup.
/// 3. **This process cannot work the door** ([`reachable_from_here`]) — on macOS the CLI on `PATH` is
///    exactly that process, and re-launching the bundle's own copy once per command to settle a state
///    that rarely drifts is a cost with no occasion. The app is launched from inside the bundle, so it
///    settles there, which is why it is the app that calls this on that platform.
///
/// Everything it does is best-effort: a scheduler that will not say what it holds leaves both halves as
/// the last run left them, which is the state that was working.
pub fn settle(consent: Option<TickConsent>) -> Option<Option<TickConsent>> {
    if !available() || consent.is_none() || !reachable_from_here() {
        return None;
    }
    let registered = probe().ok()?;
    match fix_for(consent, registered) {
        TickFix::Nothing => None,
        TickFix::Rewrite => {
            let _ = register();
            None
        }
        TickFix::Deregister => {
            let _ = unregister();
            None
        }
        TickFix::TakeTheAnswerBack => Some(None),
    }
}

/// **Is there a question to put here today?** — the one answer the banner that asks for the tick is drawn
/// from (`AMB-D-718`).
///
/// The banner spans the whole app rather than a board, because the timer it asks about is the device's,
/// and it is judged on every launch. So the conditions are read cheapest first, and the first one that
/// says no ends it:
///
/// | condition | why it is asked |
/// |---|---|
/// | this build has a door ([`available`]) | on a target amenbo cannot register on, "start checking" is a button with nothing behind it |
/// | nobody here has answered | an answer given is not a question to put again ([`TickConsent`]) |
/// | **later** was not pressed today | that button's whole meaning is one day of quiet ([`crate::overview::tick_banner_later`]) |
/// | an open task carries a due day | the warning only ever speaks about `done:false` work with a day on it |
/// | a plugin subscribed to `task.due` is enabled somewhere | carrying the warning outward is a plugin's; with none listening, a yes changes nothing |
///
/// The last two read the store and the plugins directory, which is why they are last: on the machine that
/// has already answered — every machine, after the first time — this returns without touching either.
///
/// It does not ask [`reachable_from_here`]. That one is about the process holding the door, and the
/// process that puts this question is the app, which on every target is the one that can work it; the CLI
/// puts the question in its own words and never through here.
pub fn banner_shows(store: &Store, today: NaiveDate) -> Result<bool> {
    if !available() || store.config.tick_consent.is_some() {
        return Ok(false);
    }
    if crate::overview::tick_banner_later(&store.engine)?.as_deref()
        == Some(date_to_string(today).as_str())
    {
        return Ok(false);
    }
    if !crate::store_engine::read::any_open_task_is_dated(store.engine.conn())? {
        return Ok(false);
    }
    warning_has_a_carrier(store, &crate::plugin_installed::installed(&store.paths)?)
}

/// Whether any of `installed` subscribes to `task.due` and has its gate open at the layer its author
/// declared (`AMB-D-601`) — anywhere on this device, in any project.
///
/// Anywhere, because the question behind it is about the timer, which is one per machine: a warning that
/// reaches one project is a warning the tick is worth having. Which projects it reaches, and which it does
/// not, is a thing the plugin's own settings say, and not something to weigh here.
///
/// A plugin that is enabled but incompatible with this build, or one whose subscription names a face this
/// device's drives never use, still counts. Both are states the person can be shown and can fix, and
/// neither is a reason to go quiet about the timer that would carry the warning once they had.
///
/// The installed set is passed in rather than found here, the way [`run_over`] takes its table: what is on
/// disk is [`crate::plugin_installed`]'s answer, and taking it as an argument is what lets this be driven
/// by a set a test wrote.
fn warning_has_a_carrier(store: &Store, installed: &[InstalledPlugin]) -> Result<bool> {
    for plugin in installed {
        if !plugin.manifest.events.iter().any(|e| e.event == crate::plugin_payload::name::TASK_DUE) {
            continue;
        }
        let declared = plugin.manifest.scope;
        if store.layers_with_plugin_enabled(&plugin.name)?.iter().any(|layer| match declared {
            Scope::Project => !layer.is_device(),
            Scope::Machine => layer.is_device(),
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// One thing amenbo may have to do when it is woken: the id its day mark is kept under, and the work.
pub struct Purpose {
    /// What the day mark is keyed by. It names a line of the table compiled into this binary, so no row of
    /// the store can hold it — and a mark left under an id no build declares any more is inert, which is
    /// what lets a purpose be retired without a sweep.
    pub id: &'static str,
    /// The work itself, run at most once per calendar day. Whatever it wants carried outward goes on the
    /// outbox; the tick works the queues once every purpose has had its turn.
    pub run: fn(&Store) -> Result<()>,
}

/// Every purpose a tick carries out, in the order it carries them out.
///
/// A tick that finds nothing owed is not a wasted one — it still works the queues, which is the half that
/// has to happen whether or not there was anything to say (`AMB-D-706`).
pub const PURPOSES: &[Purpose] =
    &[Purpose { id: crate::due::PURPOSE, run: crate::due::emit }];

/// What became of one purpose's turn for a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// The work ran, and the day is now marked.
    Taken,
    /// This device had already carried the purpose out on that day, so nothing ran.
    AlreadyTaken,
}

/// Take `purpose`'s turn for `day`, unless this device has already taken it (`AMB-D-708`).
///
/// The mark is compared for equality with the day asked about, not for being older than it: what is asked
/// is whether *this* day's turn has been taken, and a mark from the future — a clock moved back, a machine
/// carried west — answers that with no.
///
/// **A failing `take` leaves the mark where it was**, so a later tick of the same day tries again. The day
/// is marked by work that was carried out, never by work that was attempted.
pub fn once_a_day(
    engine: &StoreEngine,
    purpose: &str,
    day: NaiveDate,
    take: impl FnOnce() -> Result<()>,
) -> Result<Turn> {
    let day = date_to_string(day);
    if crate::overview::tick_day(engine, purpose)?.as_deref() == Some(day.as_str()) {
        return Ok(Turn::AlreadyTaken);
    }
    take()?;
    crate::overview::mark_tick_day(engine, purpose, &day)?;
    Ok(Turn::Taken)
}

/// What one tick did.
#[derive(Debug, Default)]
pub struct Report {
    /// The purposes carried out here, in the order they ran.
    pub ran: Vec<&'static str>,
    /// The purposes this device had already carried out on this day, so the tick left them alone.
    pub already_done: Vec<&'static str>,
    /// The purposes that failed, each with what it said. One failure stops that purpose and nothing else:
    /// the tick goes on to the next, and on to the queues.
    pub failed: Vec<(&'static str, String)>,
    /// One report per queue this tick worked.
    pub worked: Vec<Worked>,
    /// The queues still standing when the tick was done. A queue a live runner already held is here: one
    /// queue is worked by one runner, and nothing was taken off it here (`AMB-D-399`).
    pub left: Vec<Waiting>,
}

impl Report {
    /// How many events left the queues on this tick.
    pub fn delivered(&self) -> i64 {
        self.worked.iter().map(|w| w.delivered).sum()
    }
}

/// **Woken, judge, drop** — the whole of what the scheduler starts (`AMB-D-706`).
///
/// Every purpose that has not had its turn on `day` takes it, and then the plugin queues are worked to
/// their end, whether or not any purpose had something to say.
pub fn run(store: &Store, day: NaiveDate) -> Result<Report> {
    run_over(store, day, PURPOSES)
}

/// [`run`] with the table said out loud — the testable half, the way the nudges split. What this build
/// ships is [`PURPOSES`], and what the walk does with a purpose that failed, or one whose turn is already
/// taken, is answered by driving a table the test wrote rather than by whatever happens to be declared.
fn run_over(store: &Store, day: NaiveDate, purposes: &[Purpose]) -> Result<Report> {
    let mut report = Report::default();
    for purpose in purposes {
        match once_a_day(&store.engine, purpose.id, day, || (purpose.run)(store)) {
            Ok(Turn::Taken) => report.ran.push(purpose.id),
            Ok(Turn::AlreadyTaken) => report.already_done.push(purpose.id),
            Err(e) => report.failed.push((purpose.id, e.to_string())),
        }
    }

    // Unlike the drive that rides along with a write, an unreadable plugins directory is a failure here:
    // working the queues is half of what this process was started for, so it cannot quietly do none of it.
    let installed = crate::plugin_installed::installed(&store.paths)?;
    let subscribers = EnabledSubscribers::new(&installed, store);
    let flushed = store.flush_plugin_delivery(Face::Cli, &subscribers)?;
    report.worked = flushed.worked;
    // What is still standing, minus the queues this tick worked: those are reported by their own counts,
    // and a queue named twice would read as two backlogs.
    report.left = crate::plugin_runner::waiting(store.read_model())?
        .into_iter()
        .filter(|w| !report.worked.iter().any(|f| f.plugin == w.depth.plugin))
        .collect();
    Ok(report)
}

/// The door into this machine's scheduler, chosen for the target being built.
///
/// What goes into a registration is per-OS in a way the rest of this module is deliberately not — the
/// plist macOS wants is written by the app bundle through `SMAppService`, the Windows task has to be
/// built from XML because `schtasks` has no flag for the battery gates, and Linux writes a pair of
/// systemd user units. A target with no door answers through `nodoor`, which is the honest state and
/// not a half-written one.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod nodoor;
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
use nodoor as platform;

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn day(s: &str) -> NaiveDate {
        s.parse().expect("a test day is written as YYYY-MM-DD")
    }

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
    #[test]
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    fn a_target_with_no_door_answers_rather_than_pretending() {
        assert!(!available());
        assert!(!reachable_from_here());
        assert!(!probe().expect("a target with no door still has a state to report"));
        assert!(register().is_err());
        assert!(unregister().is_err());
    }

    /// And on a target that has one, the reading still answers — with nothing held, on a machine that
    /// was never asked to hold anything. What the writes do is the machine's to decide, so they are not
    /// called here: a build box is not a place to register a timer on.
    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn a_target_with_a_door_reads_the_scheduler_without_writing_to_it() {
        assert!(available());
        assert!(reachable_from_here());
        assert!(!probe().expect("the scheduler has a state to report"));
    }

    /// macOS has the door, and a test binary is exactly the caller that cannot open it: it runs from the
    /// target directory rather than from inside the app bundle the plist is carried in. All three faces
    /// refuse rather than answering about a bundle they are not in — including the read, whose
    /// alternative would be reporting "nothing registered" about a machine it never asked.
    #[test]
    #[cfg(target_os = "macos")]
    fn the_bundles_door_does_not_open_for_a_process_outside_it() {
        assert!(available());
        assert!(!reachable_from_here());
        assert!(probe().is_err());
        assert!(register().is_err());
        assert!(unregister().is_err());
    }

    /// Only macOS leaves the user something to look at after the registration is taken away, and the
    /// point of pinning it is that it is a property of the door rather than of the run: nothing is
    /// registered here, and the answer is the same either way — which is what lets the removal say it
    /// without first asking the machine anything.
    #[test]
    fn only_the_door_that_keeps_its_own_record_leaves_a_row_behind() {
        assert_eq!(removal_leaves_a_row(), cfg!(target_os = "macos"));
    }

    /// One machine, several amenbos: production keeps the name it has always registered under, and
    /// every other build asks for a row of its own rather than for that one. Both spellings are
    /// here, because the rule is one and the doors that apply it are three.
    #[test]
    fn a_build_that_is_not_production_registers_under_a_name_of_its_own() {
        use crate::config::Paths;
        let unix = |app: &str| registration_name_for(TICK_NAME, '-', app);
        let mac = |app: &str| registration_name_for(TICK_LABEL, '.', app);

        // Untouched, so an upgrade writes over the row the build before it registered.
        assert_eq!(unix(Paths::PRODUCTION_APP_NAME), "amenbo-tick");
        assert_eq!(mac(Paths::PRODUCTION_APP_NAME), "work.amenbo.tick");
        // The shared dev build, and one task's throwaway instance: three amenbos, three rows.
        assert_eq!(unix(Paths::DEV_APP_NAME), "amenbo-tick-amenbo-dev");
        assert_eq!(mac(Paths::DEV_APP_NAME), "work.amenbo.tick.amenbo-dev");
        assert_eq!(unix("amenbo-dev-3321"), "amenbo-tick-amenbo-dev-3321");
        assert_eq!(mac("amenbo-dev-3321"), "work.amenbo.tick.amenbo-dev-3321");
        assert_ne!(mac(Paths::DEV_APP_NAME), mac("amenbo-dev-3321"));
        // And whatever this build is, its name is that one word plus what tells it apart.
        assert!(registration_name(TICK_NAME, '-').starts_with(TICK_NAME));
        assert!(registration_name(TICK_LABEL, '.').starts_with(TICK_LABEL));
    }

    /// A device nobody has answered for is left alone without the scheduler being asked at all — the
    /// state most machines are in, on every startup.
    #[test]
    fn settling_an_unasked_device_asks_the_scheduler_nothing() {
        assert_eq!(settle(None), None);
    }

    /// A yes the scheduler no longer holds takes the answer back to **unanswered** rather than to a no
    /// (`AMB-D-718`) — a no would silence the offer along with the timer. A build box is exactly that
    /// device, holding no registration, and this row of the table writes nothing to the scheduler.
    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn a_yes_the_scheduler_no_longer_holds_leaves_the_device_unanswered() {
        assert_eq!(settle(Some(TickConsent::Yes)), Some(None));
        // And a no with nothing registered is the two already agreeing.
        assert_eq!(settle(Some(TickConsent::No)), None);
    }

    /// And a process that cannot work the door settles nothing either, whatever the answer says: on
    /// macOS a test binary is exactly that process, so an answer moved here would be one moved on a
    /// reading nobody could take.
    #[test]
    #[cfg(target_os = "macos")]
    fn settling_from_outside_the_bundle_moves_no_answer() {
        assert_eq!(settle(Some(TickConsent::Yes)), None);
        assert_eq!(settle(Some(TickConsent::No)), None);
    }

    /// A store with one project, which is what the banner's conditions are read off.
    fn store_with_project(tag: &str) -> (Store, i64) {
        let mut store = Store::open_at(crate::config::Paths::at(amenbo_scratch::scratch(tag))).unwrap();
        let project = store
            .project_add(crate::ops::project::NewProject {
                name: "期日".into(),
                view: crate::model::View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap();
        (store, project.id)
    }

    /// File a task, with or without a day on it.
    fn file(store: &mut Store, project: i64, title: &str, due: Option<NaiveDate>) -> i64 {
        store
            .add_task(crate::ops::task::NewTask {
                title: title.into(),
                project_id: Some(project),
                due_on: due,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(crate::model::ActorKind::Human),
                at_binding_id: None,
            })
            .unwrap()
            .id
    }

    /// One installed plugin, as the resolver reads it — the manifest's `scope` and `events` are the two
    /// fields the carrier check asks about, and the rest is filler.
    fn installed(name: &str, scope: Scope, events: &[&str]) -> InstalledPlugin {
        use crate::plugin_manifest::{EventSubscription, Manifest, Os};
        InstalledPlugin {
            name: name.into(),
            program: std::path::PathBuf::from(format!("/plugins/{name}")),
            manifest: Manifest {
                name: name.into(),
                desc: String::new(),
                about: None,
                author: String::new(),
                repo: String::new(),
                os: vec![Os::Linux],
                category: String::new(),
                url: String::new(),
                checksum: String::new(),
                signature: None,
                assets: Default::default(),
                official: false,
                detail_sum: None,
                scope,
                payload_v: crate::plugin_payload::VERSION,
                min_amenbo: None,
                config: Vec::new(),
                events: events.iter().map(|e| EventSubscription::new(*e)).collect(),
                agent: None,
                settings: None,
            },
            origin: None,
        }
    }

    /// Lay a well-formed install down under the store's own base, so [`banner_shows`] finds it the way it
    /// finds a real one: the home, the executable, and the manifest that marks the install finished.
    fn lay_down(store: &Store, plugin: &InstalledPlugin) {
        let home = store.paths.plugin_dir(&plugin.name);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(crate::plugin_installed::program_file_name(&plugin.name)),
            b"#!/bin/sh\n",
        )
        .unwrap();
        std::fs::write(
            home.join(crate::plugin_installed::MANIFEST_FILE_NAME),
            serde_json::to_string(&plugin.manifest).unwrap(),
        )
        .unwrap();
    }

    /// Open a plugin's gate at one layer — what an enable does.
    fn enable_at(store: &mut Store, plugin: &str, layer: crate::plugin_layer::Layer) {
        crate::plugin_trust::enable(
            store,
            plugin,
            layer,
            &[],
            |_| true,
            &crate::plugin_check::Checked::NotDeclared,
        )
        .unwrap();
    }

    /// The three conditions the decision names, each on its own: with all of them met the banner has a
    /// question to put, and taking any one away silences it (`AMB-D-718`).
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn the_banner_asks_only_where_all_three_conditions_hold() {
        let today = day("2026-08-19");
        let (mut store, project) = store_with_project("tick-banner-conditions");
        let dated = file(&mut store, project, "期日つき", Some(today));
        lay_down(&store, &installed("carrier", Scope::Project, &["task.due"]));
        enable_at(&mut store, "carrier", crate::plugin_layer::Layer::Project(project));

        // Unanswered, dated work on the board, and something listening: the whole question is live.
        assert!(banner_shows(&store, today).unwrap());

        // Answered — either way. The question is the device's and it has been put once already.
        for answer in [TickConsent::Yes, TickConsent::No] {
            store.config.tick_consent = Some(answer);
            assert!(!banner_shows(&store, today).unwrap(), "{answer:?} is an answer, not a question");
        }
        store.config.tick_consent = None;

        // Nothing dated left: the warning only ever speaks about open work with a day on it, so there is
        // nothing here for the timer to say.
        store
            .set_task_status(dated, crate::model::TaskStatus::Done, crate::model::ActorKind::Human)
            .unwrap();
        assert!(!banner_shows(&store, today).unwrap());
        file(&mut store, project, "期日なし", None);
        assert!(!banner_shows(&store, today).unwrap(), "a task with no day is not dated work");
        file(&mut store, project, "また期日つき", Some(today));
        assert!(banner_shows(&store, today).unwrap());

        // Nobody listening: carrying the warning outward is a plugin's, so a yes would change nothing.
        crate::plugin_trust::disable(&mut store, "carrier", crate::plugin_layer::Layer::Project(project))
            .unwrap();
        assert!(!banner_shows(&store, today).unwrap());
    }

    /// **Later** is one day of quiet and not an answer: the banner stays down for the day it was pressed
    /// on, and is back the next.
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn later_holds_the_banner_back_for_that_day_alone() {
        let today = day("2026-08-19");
        let (mut store, project) = store_with_project("tick-banner-later");
        file(&mut store, project, "期日つき", Some(today));
        lay_down(&store, &installed("carrier", Scope::Project, &["task.due"]));
        enable_at(&mut store, "carrier", crate::plugin_layer::Layer::Project(project));

        crate::overview::defer_tick_banner(&store.engine, "2026-08-19").unwrap();
        assert!(!banner_shows(&store, today).unwrap());
        assert!(banner_shows(&store, day("2026-08-20")).unwrap(), "the next day it is asked again");
        assert!(
            store.config.tick_consent.is_none(),
            "later answers nothing — the question is still open",
        );
    }

    /// The gate that counts is the one the plugin's author declared (`AMB-D-601`): a project plugin
    /// switched on in some project, a machine plugin switched on for the device, and neither reads the
    /// other's row.
    #[test]
    fn a_carrier_is_counted_at_the_layer_its_author_declared() {
        use crate::plugin_layer::Layer;
        let (mut store, project) = store_with_project("tick-banner-carrier");
        let of_the_project = [installed("slack", Scope::Project, &["task.due"])];
        let of_the_device = [installed("slack", Scope::Machine, &["task.due"])];

        // Installed, subscribed, and switched on nowhere.
        assert!(!warning_has_a_carrier(&store, &of_the_project).unwrap());

        // The project's own switch answers for a project plugin, and not for a machine one.
        enable_at(&mut store, "slack", Layer::Project(project));
        assert!(warning_has_a_carrier(&store, &of_the_project).unwrap());
        assert!(!warning_has_a_carrier(&store, &of_the_device).unwrap());

        // And the device's switch the other way round.
        let (mut store, _) = store_with_project("tick-banner-carrier-device");
        enable_at(&mut store, "slack", Layer::Device);
        assert!(warning_has_a_carrier(&store, &of_the_device).unwrap());
        assert!(!warning_has_a_carrier(&store, &of_the_project).unwrap());
    }

    /// A plugin that is on but does not subscribe to `task.due` carries nothing — being installed and
    /// enabled is not the same as listening for the warning.
    #[test]
    fn a_plugin_that_is_on_but_not_listening_carries_nothing() {
        let (mut store, project) = store_with_project("tick-banner-not-listening");
        enable_at(&mut store, "elsewhere", crate::plugin_layer::Layer::Project(project));
        let elsewhere = [installed("elsewhere", Scope::Project, &["task.done", "comment.added"])];
        assert!(!warning_has_a_carrier(&store, &elsewhere).unwrap());

        // The day-before warning is its own event, and subscribing to it alone is not subscribing to this.
        let tomorrow_only = [installed("elsewhere", Scope::Project, &["task.due_tomorrow"])];
        assert!(!warning_has_a_carrier(&store, &tomorrow_only).unwrap());
    }

    /// The rule the hourly wake-up rests on: within one calendar day the work is carried out once, however
    /// many times amenbo is woken, and the next day it is owed again (`AMB-D-708`).
    #[test]
    fn a_purpose_takes_one_turn_a_day_however_often_the_tick_is_woken() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let runs = Cell::new(0);
        let take = || {
            runs.set(runs.get() + 1);
            Ok(())
        };

        assert_eq!(once_a_day(&engine, "due", day("2026-08-18"), take).unwrap(), Turn::Taken);
        assert_eq!(runs.get(), 1);
        // The same day, woken again — hourly, all day long.
        for _ in 0..5 {
            assert_eq!(
                once_a_day(&engine, "due", day("2026-08-18"), take).unwrap(),
                Turn::AlreadyTaken
            );
        }
        assert_eq!(runs.get(), 1, "the day is the unit, not the wake-up");

        // The first tick after the local day turned over.
        assert_eq!(once_a_day(&engine, "due", day("2026-08-19"), take).unwrap(), Turn::Taken);
        assert_eq!(runs.get(), 2);
    }

    /// Each purpose is counted on its own: one taking its turn says nothing about another's.
    #[test]
    fn one_purpose_taking_its_turn_leaves_the_others_owed() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert_eq!(once_a_day(&engine, "due", day("2026-08-18"), || Ok(())).unwrap(), Turn::Taken);
        assert_eq!(once_a_day(&engine, "tidy", day("2026-08-18"), || Ok(())).unwrap(), Turn::Taken);
        assert_eq!(
            once_a_day(&engine, "due", day("2026-08-18"), || Ok(())).unwrap(),
            Turn::AlreadyTaken
        );
    }

    /// Work that failed is work that was not carried out, so the day stays unmarked and the next tick of
    /// the same day tries again. Marking on the attempt would lose the day to one bad run.
    #[test]
    fn a_purpose_that_failed_is_owed_again_on_the_same_day() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let err = || Err(crate::error::Error::Invalid(crate::error::Msg::new("the work did not run")));

        assert!(once_a_day(&engine, "due", day("2026-08-18"), err).is_err());
        assert!(crate::overview::tick_day(&engine, "due").unwrap().is_none(), "nothing was marked");
        assert_eq!(once_a_day(&engine, "due", day("2026-08-18"), || Ok(())).unwrap(), Turn::Taken);
    }

    /// A mark from a day that is not this one is a turn still owed — a clock moved back, or a machine
    /// carried west over a date line, leaves one, and the day it names has no claim on today.
    #[test]
    fn a_mark_from_another_day_does_not_take_this_days_turn() {
        let engine = StoreEngine::open_in_memory().unwrap();
        crate::overview::mark_tick_day(&engine, "due", "2026-08-19").unwrap();
        assert_eq!(once_a_day(&engine, "due", day("2026-08-18"), || Ok(())).unwrap(), Turn::Taken);
        assert_eq!(
            crate::overview::tick_day(&engine, "due").unwrap().as_deref(),
            Some("2026-08-18")
        );
    }

    /// The walk: every purpose is asked in turn, one that failed is reported rather than thrown, and the
    /// tick still reaches the queues afterwards. With nothing installed there is nothing to deliver, which
    /// is the ordinary shape of a tick and not an error.
    #[test]
    fn a_tick_asks_every_purpose_and_reaches_the_queues_whatever_they_answered() {
        let dir = amenbo_scratch::scratch("tick-walk");
        let store = Store::open_at(crate::config::Paths::at(dir)).unwrap();
        let table = [
            Purpose { id: "carried-out", run: |_| Ok(()) },
            Purpose {
                id: "would-not-run",
                run: |_| Err(crate::error::Error::Invalid(crate::error::Msg::new("no"))),
            },
            Purpose { id: "after-the-failure", run: |_| Ok(()) },
        ];

        let report = run_over(&store, day("2026-08-18"), &table).unwrap();
        assert_eq!(report.ran, ["carried-out", "after-the-failure"]);
        assert_eq!(report.failed.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["would-not-run"]);
        assert_eq!(report.delivered(), 0, "nothing is installed, so nothing is delivered");
        assert!(report.left.is_empty(), "and nothing is left owed");

        // Woken again the same day: what was carried out is not carried out twice, and what failed is
        // still owed.
        let again = run_over(&store, day("2026-08-18"), &table).unwrap();
        assert_eq!(again.already_done, ["carried-out", "after-the-failure"]);
        assert_eq!(again.failed.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["would-not-run"]);
    }
}
