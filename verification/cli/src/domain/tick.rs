//! The `tick` domain: the machine's own scheduler waking amenbo, and what amenbo works out once it
//! is awake.
//!
//! **The wake is the assert.** A tick leaves a day mark behind and nothing else a reader can ask
//! for, so what it says on the way past is the whole of what can be judged — and a command written
//! to read that mark would be a face built for this harness rather than for anybody. Carrying the
//! turn out and judging what came back is what a scheduler itself would have got.
//!
//! **Nothing here registers a timer.** The registration is written outside the throwaway store a run
//! makes — into the launchd, systemd or Task Scheduler of whichever machine the gate is running on —
//! so a road that walked it would leave an hourly timer on a release box. Only the reading is here,
//! plus one premise's reach (`deferred`) — a day written into the run's own store, the way
//! `store worn-in` writes its tallies, and still nothing that touches the machine.

use std::path::Path;

use amenbo_scenario::{Args, Domain};

use crate::{req_bool, req_str, unmapped, Driver, Outcome};

/// The `store_meta` key the build keeps the band's "later" day under. The name is the build's, and it
/// is written down here for the reason `store.rs` writes its two down: a store is a plain SQLite
/// file, and the premise reaches into it directly.
const TICK_BANNER_LATER_KEY: &str = "tick.banner.later_day";

/// Stand up the band already put off: write the day "later" was pressed straight into the store, as
/// `today` or `yesterday`. Direct, like `wear_in`, and for the same reason: the state is a day
/// having passed — or not — since a press, which no run reaches by pressing, the band being judged
/// once at launch.
fn defer_banner(home: &Path, when: &str) -> Result<String, String> {
    let modifier = match when {
        "today" => "+0 days",
        "yesterday" => "-1 days",
        other => {
            return Err(format!(
                "`deferred` takes `today` or `yesterday`, not `{other}` — the band returns after one day, so anything further back is the same world as `yesterday`"
            ))
        }
    };
    let db = home.join(crate::domain::store::STORE_FILE);
    if !db.is_file() {
        return Err(format!(
            "there is no store at {} yet — `deferred` writes into one the run has already made",
            db.display()
        ));
    }
    let conn = rusqlite::Connection::open(&db)
        .map_err(|e| format!("could not open the store at {}: {e}", db.display()))?;
    let sql = |e: rusqlite::Error| format!("could not put the band off: {e}");
    // The reader's day and not UTC, for the reason `wear_in` gives: that is the day the build writes
    // there, and what reads it back compares the two as text.
    let day: String = conn
        .query_row("SELECT date('now', 'localtime', ?1)", [modifier], |r| r.get(0))
        .map_err(sql)?;
    conn.execute(
        "INSERT OR REPLACE INTO store_meta (key, value) VALUES (?1, ?2)",
        rusqlite::params![TICK_BANNER_LATER_KEY, day],
    )
    .map_err(sql)?;
    Ok(format!("the band was put off on {day} ({when})"))
}

impl Driver<'_> {
    /// The wake-up's one action, and it is a premise's: everything else in this domain is a reading.
    pub(crate) fn tick_action(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            "deferred" => {
                Ok(Outcome::action(defer_banner(&self.session.home, req_str(with, "when")?)?))
            }
            _ => Err(unmapped(Domain::Tick, op)),
        }
    }

    /// Whether the machine's scheduler is holding the hourly tick right now. It is read here and at
    /// the start of a run, and the assert is the difference between the two — see `holds` below.
    pub(crate) fn tick_registered(&self) -> Result<bool, String> {
        let v = self.run_json(&["tick", "status", "--json"])?;
        Ok(v["registered"].as_bool().unwrap_or(false))
    }

    pub(crate) fn tick_assert(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // One hour's turn, and whether the named purpose was carried out on it. The two answers
            // a tick gives about a purpose are opposite sides of the same rule: it ran, or the day it
            // is owed for was already marked. Both are read, so a purpose the build knows nothing
            // about is neither and comes out red rather than passing as "not carried out".
            "woken" => {
                let purpose = req_str(with, "purpose")?;
                let want = req_bool(with, "carried_out")?;
                // A tick exits non-zero when a purpose failed, and that is a verdict rather than a
                // failure to run — so the report is read either way and the failure is named below.
                let v = self.run_check(&["tick", "run", "--json"])?;
                let named = |key: &str| {
                    v[key]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                        .iter()
                        .any(|p| p.as_str() == Some(purpose))
                };
                let ran = named("ran");
                let held = named("already_done");
                let why = v["failed"]
                    .as_array()
                    .and_then(|all| all.iter().find(|f| f["purpose"].as_str() == Some(purpose)))
                    .and_then(|f| f["error"].as_str());
                let pass = ran == want && (ran || held) && why.is_none();
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the tick {} `{purpose}`{} (expected it to have {}, {})",
                        match (ran, held) {
                            (true, _) => "carried out",
                            (false, true) => "left the day already marked for",
                            (false, false) => "knows nothing of",
                        },
                        why.map(|e| format!(", failing with {e}")).unwrap_or_default(),
                        if want { "run" } else { "stood down" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            // What the run did to the scheduler's registration, and never what the machine holds.
            // The registration lives outside the throwaway store, in the launchd, systemd or Task
            // Scheduler this run was started on, so the absolute reading is the machine's own answer
            // and not this road's: on a machine where somebody uses the hourly tick it is `true`
            // before anything walks. What a road can honestly say is the difference across it —
            // `changed: false` being "left as it was found", which is what every road here is owed
            // since nothing in this harness registers a timer.
            "holds" => {
                let want = req_bool(with, "changed")?;
                let now = self.tick_registered()?;
                let changed = now != self.tick_at_start;
                let pass = changed == want;
                Ok(Outcome::assert(
                    pass,
                    format!(
                        "the run left the scheduler {} ({}; expected {}, {})",
                        if changed { "holding something else" } else { "as it was found" },
                        match (self.tick_at_start, now) {
                            (true, true) => "it was holding the hourly tick before and still is",
                            (false, false) => "it was holding nothing of ours before and still is",
                            (false, true) => "it was holding nothing of ours before and holds the hourly tick now",
                            (true, false) => "it was holding the hourly tick before and holds nothing of ours now",
                        },
                        if want { "a change" } else { "no change" },
                        if pass { "as expected" } else { "MISMATCH" }
                    ),
                ))
            }
            _ => Err(unmapped(Domain::Tick, op)),
        }
    }
}

#[cfg(test)]
mod deferred_tests {
    use super::*;

    /// A store holding only the table this premise reaches into, for the reason `worn_in_tests`
    /// stands up its two: the rest of the schema is the shipped build's business.
    fn store_with_meta() -> crate::scratch::Session {
        let session = crate::scratch::session("deferred-test", false).expect("a throwaway home");
        let conn = rusqlite::Connection::open(session.home.join(crate::domain::store::STORE_FILE))
            .expect("a store to open");
        conn.execute_batch("CREATE TABLE store_meta (key TEXT PRIMARY KEY, value TEXT);")
            .expect("the one table this reaches into");
        session
    }

    fn later_day(home: &Path) -> Option<String> {
        let conn = rusqlite::Connection::open(home.join(crate::domain::store::STORE_FILE))
            .expect("a store to open");
        conn.query_row("SELECT value FROM store_meta WHERE key = ?1", [TICK_BANNER_LATER_KEY], |r| {
            r.get(0)
        })
        .ok()
    }

    /// The whole of what the premise claims: the named day is on the key the build reads, and naming
    /// the other day replaces it — the only question ever put to this mark is about the day in hand.
    #[test]
    fn putting_the_band_off_writes_the_named_day() {
        let session = store_with_meta();
        let conn = rusqlite::Connection::open(session.home.join(crate::domain::store::STORE_FILE))
            .expect("a store to open");

        defer_banner(&session.home, "today").expect("today is a day it knows");
        let today: String =
            conn.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0)).expect("the day");
        assert_eq!(later_day(&session.home), Some(today));

        defer_banner(&session.home, "yesterday").expect("yesterday too, replacing the day");
        let yesterday: String = conn
            .query_row("SELECT date('now', 'localtime', '-1 days')", [], |r| r.get(0))
            .expect("the day before");
        assert_eq!(later_day(&session.home), Some(yesterday));
    }

    /// Anything further back is the same world as yesterday, so it is refused rather than mapped:
    /// a premise quietly accepting days it does not distinguish would read as distinguishing them.
    #[test]
    fn a_day_further_back_is_refused() {
        let session = store_with_meta();
        let err = defer_banner(&session.home, "last-week").expect_err("only the two named days");
        assert!(err.contains("last-week"), "{err}");
    }

    /// And a home with no store in it yet, which is what a premise that put this first would meet.
    #[test]
    fn a_home_with_no_store_is_refused() {
        let session = crate::scratch::session("deferred-empty-test", false).expect("a throwaway home");
        let err = defer_banner(&session.home, "today").expect_err("nothing to write into");
        assert!(err.contains("no store"), "{err}");
    }
}
