//! **The tick** — what amenbo does when the OS scheduler wakes it (`AMB-D-706`).
//!
//! Three parts, and declaring a new thing for a tick to do is meant to be one line in the middle one:
//!
//! | part | what it is |
//! |---|---|
//! | the entry | [`run`] — woken, judge what is owed, work the queues, drop |
//! | the declaration table | [`PURPOSES`] — an id, and the work it stands for |
//! | the log of what has been done | the device-local day mark ([`crate::overview::tick_day`]) |
//!
//! **What the OS holds is one bare "wake amenbo every hour", and nothing else** (`AMB-D-707`). The meaning
//! is judged here, so a purpose declared later moves nothing on the OS side and the registration never
//! has to be rewritten. It also means the tick is woken far more often than it has anything to do: being
//! woken is not being due, and the table below is where that is decided.
//!
//! **A purpose is counted in calendar days, not in wake-ups** (`AMB-D-708`). The hourly wake-up is the
//! only clock there is, and a machine that was asleep is woken for the hours it missed, so anything
//! counted per wake-up rings several times over on the day it comes back. [`once_a_day`] is the whole of
//! that rule.
//!
//! **The queues are worked on every tick, including the ones with nothing to say** (`AMB-D-706`). A runner
//! killed mid-queue leaves its rows standing until the next write drives delivery again, and writes this
//! feature makes are a day apart — so the tick with nothing to emit is exactly the one that should carry
//! what a previous run left behind ([`crate::plugin_drive`]). It works them **in this process**: the tick
//! is a process the scheduler started for this and nothing else, so there is no command being made to
//! wait, and handing the queues to runners nobody watches would leave it with nothing to report.

use chrono::NaiveDate;

use crate::error::Result;
use crate::plugin_manifest::Face;
use crate::plugin_runner::{Waiting, Worked};
use crate::plugin_subscribe::EnabledSubscribers;
use crate::store::Store;
use crate::store_engine::StoreEngine;
use crate::time::date_to_string;

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
/// Empty: the entry is here and nothing has been declared into it yet. A tick over an empty table is not a
/// wasted one — it still works the queues, which is the half that has to happen whether or not there was
/// anything to say (`AMB-D-706`).
pub const PURPOSES: &[Purpose] = &[];

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

/// **Woken, judge, drop** — the whole of what the OS scheduler starts (`AMB-D-706`).
///
/// Every purpose that has not had its turn on `day` takes it, and then the plugin queues are worked to
/// their end, whether or not any purpose had something to say.
pub fn run(store: &Store, day: NaiveDate) -> Result<Report> {
    run_over(store, day, PURPOSES)
}

/// [`run`] with the table said out loud — the testable half, the way the nudges split. What this build
/// ships is [`PURPOSES`]; the walk over a table is what a test has to be able to drive, and an empty
/// declaration table would otherwise leave the walk unread.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn day(s: &str) -> NaiveDate {
        s.parse().expect("a test day is written as YYYY-MM-DD")
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
