//! The hourly tick: the one plain timer amenbo asks this machine's scheduler to hold (`AMB-D-707`),
//! whether it may ask at all, and what amenbo does once that timer wakes it (`AMB-D-706`).
//!
//! Two halves, joined by nothing but the hour:
//!
//! | half | what it is |
//! |---|---|
//! | whether we are woken | the answer on record ([`TickConsent`]), what the scheduler holds ([`probe`]), and the rule that settles the two ([`fix_for`]) |
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
use crate::plugin_manifest::Face;
use crate::plugin_runner::{Waiting, Worked};
use crate::plugin_subscribe::EnabledSubscribers;
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
///
/// It is a fact about the **build**, not about the machine: a Linux without systemd on it still has a
/// door compiled in, and what it does not have is something behind the door — which is [`probe`]'s
/// answer and the writes' refusal, said where the machine is actually asked.
pub fn available() -> bool {
    platform::AVAILABLE
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
/// systemd user units — so each door lands with the OS that needs it (`AMB-T-3253` / `AMB-T-3254`)
/// rather than being guessed at from here. A target with no door yet answers through `nodoor`, which is
/// the honest state and not a half-written one.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(not(target_os = "linux"))]
mod nodoor;
#[cfg(not(target_os = "linux"))]
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
    #[cfg(not(target_os = "linux"))]
    fn a_target_with_no_door_answers_rather_than_pretending() {
        assert!(!available());
        assert!(!probe().expect("a target with no door still has a state to report"));
        assert!(register().is_err());
        assert!(unregister().is_err());
    }

    /// And on a target that has one, the reading still answers — with nothing held, on a machine that
    /// was never asked to hold anything. What the writes do is the machine's to decide, so they are not
    /// called here: a build box is not a place to register a timer on.
    #[test]
    #[cfg(target_os = "linux")]
    fn a_target_with_a_door_reads_the_scheduler_without_writing_to_it() {
        assert!(available());
        assert!(!probe().expect("the scheduler has a state to report"));
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
