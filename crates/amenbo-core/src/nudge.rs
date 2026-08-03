//! **Nudges** — the ground the "you have been using this a while; would you like …" prompts stand on
//! (`AMB-D-542`).
//!
//! Three parts, and adding a nudge is meant to be one line in the middle one:
//!
//! | part | what it is |
//! |---|---|
//! | the metric catalog | [`Metric`] — a name, and the number it stands for |
//! | the declaration table | [`NUDGES`] — id, the thresholds that have to be met, the stage it has a place in, once or not |
//! | the log of what has been put | the device-local `nudge_fired` table ([`crate::overview`]) |
//!
//! **The judgement lives here, not in the surface that shows it** (`AMB-D-544`): both halves of the material
//! are in core — what the store holds, and what this device has tallied — so a caller asks
//! [`pending`] and is handed the nudges it should put now. The wording and the way it is shown belong
//! to the caller.
//!
//! **The order the judgement is made in** is the point of it. What has already been put, and what stage
//! the caller is in, narrow first — both answerable without touching a record — and only what survives
//! that has its metrics counted. Someone who has answered every nudge counts nothing at all, however
//! often the caller asks.
//!
//! **Nothing is declared yet, and that is a working state.** [`NUDGES`] is empty, so [`pending`] hands
//! back nothing and no metric is ever counted; the first nudge is a line added to it.
//!
//! **What is counted stays here.** The numbers are read to answer one question on this machine and are
//! never written anywhere else, never sent, and never synced (`AMB-D-542`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::error::Result;
use crate::store_engine::schema::col;
use crate::store_engine::sql::{Expr, Pred, Select, Sql, Table};
use crate::store_engine::{StoreEngine, StoreEngineError};
use crate::time::{local_day_start_utc, Timestamp};

/// A number that says something about how much amenbo has been used here. Two kinds behind one face
/// (`AMB-D-543`): most are **counted** off the store when asked, and only what the store cannot answer —
/// what leaves no record behind — is **tallied** as it happens.
///
/// **Every counted metric owes the same thing: an answer whose cost does not grow with the store.**
/// Either the count is served by an index, or it is cut to a window ([`ACTIVE_DAYS_WINDOW`]). A metric
/// that can promise neither does not belong on the counted side — it belongs on the tallied one, or
/// nowhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Metric {
    /// How many tasks the store holds. Counted: the row count of one table, served by the key's own
    /// B-tree.
    TaskCount,
    /// On how many separate days something was written here, over the last [`ACTIVE_DAYS_WINDOW`] days.
    /// Counted, and windowed — the whole-history version of this reads every record ever written, which
    /// is the shape that punishes the person who has used amenbo longest.
    ActiveDays,
    /// How many times the app has been launched on this device. Tallied: a launch leaves no record.
    LaunchCount,
    /// How many days ago this device was first launched. Tallied (the day is), because the store cannot
    /// say when someone started using it — the oldest record only says when they first wrote something.
    DaysSinceFirstLaunch,
}

/// How far back [`Metric::ActiveDays`] looks. Long enough that a month of steady use reads as steady
/// use, short enough that the count is over a bounded slice of the store rather than all of it.
pub const ACTIVE_DAYS_WINDOW: i64 = 90;

impl Metric {
    /// The catalog: every metric a nudge may be declared against.
    pub const ALL: &'static [Metric] =
        &[Metric::TaskCount, Metric::ActiveDays, Metric::LaunchCount, Metric::DaysSinceFirstLaunch];

    /// The name this metric is written by — in a declaration below, and at any boundary that has to
    /// carry a metric as text.
    pub fn name(self) -> &'static str {
        match self {
            Metric::TaskCount => "task_count",
            Metric::ActiveDays => "active_days",
            Metric::LaunchCount => "launch_count",
            Metric::DaysSinceFirstLaunch => "days_since_first_launch",
        }
    }

    /// The metric that goes by `name`, or `None` for a name the catalog does not hold.
    pub fn from_name(name: &str) -> Option<Metric> {
        Metric::ALL.iter().copied().find(|m| m.name() == name)
    }

    /// This metric's value. `today` is the reader's day, passed in rather than read from the clock so
    /// that the two metrics that count from it can be pinned by a test.
    pub fn count(self, engine: &StoreEngine, today: NaiveDate) -> Result<i64> {
        match self {
            Metric::TaskCount => count_rows(engine, col::task::ALL.table),
            Metric::ActiveDays => active_days(engine, today),
            Metric::LaunchCount => Ok(crate::overview::usage_tallies(engine)?.0),
            Metric::DaysSinceFirstLaunch => {
                let (_, first_day) = crate::overview::usage_tallies(engine)?;
                // No first launch recorded is `0` — the same answer as "first launched today", and the
                // same for every threshold: not yet. There is nothing else it could honestly be.
                let Some(first) =
                    first_day.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
                else {
                    return Ok(0);
                };
                Ok((today - first).num_days().max(0))
            }
        }
    }
}

/// One line of the declaration table: a nudge, and everything that decides whether it is put.
pub struct Nudge {
    /// What this nudge is called. It is the key of the log row that records it as put, so it outlives
    /// the code — renaming one puts it a second time to everyone who has already answered it.
    pub id: &'static str,
    /// Every threshold has to be met (they are ANDed). An empty list is a nudge with no condition on
    /// use at all — one held by its stage alone.
    pub when: &'static [Threshold],
    /// The stage this nudge has a place in — a name the caller answers ("the setting this is about is
    /// still unanswered"). `None` is a nudge with no stage: it applies whenever its thresholds are met.
    pub stage: Option<&'static str>,
    /// Put once ever, or every time it applies? `true` makes the log row a veto — put once, never
    /// again. `false` leaves the stage as the only thing that stops it, which is what a nudge tied to
    /// an unanswered setting wants: it goes on applying until the setting is answered, and the log row
    /// records when it last went out rather than closing it.
    pub once: bool,
}

/// A condition on one metric: it has to have reached `at_least`.
pub struct Threshold {
    pub metric: Metric,
    pub at_least: i64,
}

/// **The declaration table.** Every nudge amenbo can put, and the whole of what decides it — adding one
/// is adding a line here, and nothing else.
///
/// Empty, and a shipping build with it empty is a working build: [`pending`] hands back nothing and
/// nothing is ever counted (`AMB-D-542`).
pub const NUDGES: &[Nudge] = &[];

/// The nudges to put now: those the caller's stage admits, whose thresholds are met, and which the log
/// does not already close. `stage_open` answers a nudge's [`Nudge::stage`] — the caller holds the
/// settings a stage is about, so it is the one that can.
///
/// Judged against the reader's day ([`crate::time::today`]).
pub fn pending(engine: &StoreEngine, stage_open: impl Fn(&str) -> bool) -> Result<Vec<&'static Nudge>> {
    pending_from(NUDGES, engine, crate::time::today(), stage_open)
}

/// [`pending`] with the table and the day said out loud — the testable half, so the thresholds can be
/// pinned at their boundaries without the store having to be brought to them.
fn pending_from(
    nudges: &'static [Nudge],
    engine: &StoreEngine,
    today: NaiveDate,
    stage_open: impl Fn(&str) -> bool,
) -> Result<Vec<&'static Nudge>> {
    if nudges.is_empty() {
        return Ok(Vec::new());
    }
    // The cheap half first: what has been put, and what the caller's stage admits. Both are answered
    // without reading a record, and what they drop is never counted for.
    let closed: BTreeSet<String> =
        crate::overview::nudges_fired(engine)?.into_iter().map(|(id, _)| id).collect();
    let candidates: Vec<&'static Nudge> = nudges
        .iter()
        .filter(|n| !(n.once && closed.contains(n.id)))
        .filter(|n| n.stage.is_none_or(&stage_open))
        .collect();

    // Only now the counting, and each metric at most once however many nudges name it.
    let mut counted: BTreeMap<Metric, i64> = BTreeMap::new();
    let mut due = Vec::new();
    for nudge in candidates {
        let mut met = true;
        for threshold in nudge.when {
            let value = match counted.get(&threshold.metric) {
                Some(v) => *v,
                None => *counted
                    .entry(threshold.metric)
                    .or_insert(threshold.metric.count(engine, today)?),
            };
            if value < threshold.at_least {
                met = false;
                break;
            }
        }
        if met {
            due.push(nudge);
        }
    }
    Ok(due)
}

/// Record that `nudge_id` has been put, now. The caller does this once it has actually shown the nudge —
/// a nudge judged due and never shown is one the person has not seen, and closing it here would lose it.
pub fn mark_put(engine: &StoreEngine, nudge_id: &str) -> Result<()> {
    crate::overview::mark_nudge_fired(engine, nudge_id, &Timestamp::now().to_rfc3339_z())
}

/// Count one launch of the app on this device, on the reader's day.
pub fn record_launch(engine: &StoreEngine) -> Result<()> {
    crate::overview::record_launch(engine, &crate::time::date_to_string(crate::time::today()))
}

/// The row count of `table` — `SELECT COUNT(*)`, which SQLite serves off the key's own B-tree rather
/// than by reading the rows.
fn count_rows(engine: &StoreEngine, table: Table) -> Result<i64> {
    let mut sel = Select::new();
    let n = sel.count_all();
    let sql = Sql::from(&sel, table);
    engine
        .conn()
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| n.get(r))
        .map_err(StoreEngineError::from)
        .map_err(Into::into)
}

/// On how many days of the last [`ACTIVE_DAYS_WINDOW`] was something written here.
///
/// The window is what keeps this bounded: the cut is a fixed-width instant and the stored instants are
/// too, so a text comparison *is* the time comparison, and each table is asked only about the slice
/// after it. The days come back distinct from SQLite, so what crosses into Rust is at most one string
/// per day in the window — never one per record.
///
/// The days are UTC days, not the reader's: an instant carries its day in its first ten characters,
/// and re-grouping them onto local days would mean reading the instants themselves. What the metric is
/// for is an order of magnitude of use, and the two can differ only where a day's writing sits on one
/// side of midnight — at most one day, at the window's edge.
fn active_days(engine: &StoreEngine, today: NaiveDate) -> Result<i64> {
    let since = local_day_start_utc(today - chrono::Duration::days(ACTIVE_DAYS_WINDOW))
        .to_rfc3339_z();
    let mut days: BTreeSet<String> = BTreeSet::new();
    // What "used it" means: wrote something, anywhere the store keeps writing — the two records and the
    // two timelines. `updated_at` rather than `created_at`, because coming back to an old task is using
    // amenbo just as much as making a new one.
    for updated_at in [
        col::task::ALL.updated_at,
        col::decision::ALL.updated_at,
        col::task_comment::ALL.updated_at,
        col::decision_comment::ALL.updated_at,
    ] {
        let mut sel = Select::new();
        sel.distinct();
        // The day of an instant is an expression over a column, not a column — the select-list seam for
        // exactly that, and it carries no bind of its own (the window's cut is in the `WHERE`).
        let day = sel.expr::<String>(updated_at.day().to_sql());
        let mut sql = Sql::from(&sel, updated_at.table());
        sql.push_where(Some(&Pred::cmp(updated_at, ">=", since.as_str())));
        let conn = engine.conn();
        let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sql.params()), |r| day.get(r))
            .map_err(StoreEngineError::from)?;
        for row in rows {
            days.insert(row.map_err(StoreEngineError::from)?);
        }
    }
    Ok(days.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names are the boundary's half of the catalog, so every metric has one and no two share it.
    #[test]
    fn every_metric_goes_by_a_name_of_its_own() {
        let names: BTreeSet<&str> = Metric::ALL.iter().map(|m| m.name()).collect();
        assert_eq!(names.len(), Metric::ALL.len(), "two metrics answer to the same name");
        for m in Metric::ALL {
            assert_eq!(Metric::from_name(m.name()), Some(*m));
        }
        assert_eq!(Metric::from_name("no_such_metric"), None);
    }

    /// A store nobody has launched counts zero for both tallies, and counting a launch moves the tally
    /// while leaving the first day where it was.
    #[test]
    fn the_tallies_start_at_nothing_and_the_first_day_is_written_once() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        assert_eq!(Metric::LaunchCount.count(&engine, day).unwrap(), 0);
        assert_eq!(Metric::DaysSinceFirstLaunch.count(&engine, day).unwrap(), 0);

        crate::overview::record_launch(&engine, "2026-08-04").unwrap();
        crate::overview::record_launch(&engine, "2026-08-09").unwrap();
        assert_eq!(Metric::LaunchCount.count(&engine, day).unwrap(), 2);
        assert_eq!(
            Metric::DaysSinceFirstLaunch
                .count(&engine, NaiveDate::from_ymd_opt(2026, 8, 14).unwrap())
                .unwrap(),
            10,
            "counted from the first launch, which the second one did not move"
        );
    }

    /// The empty table is a working state: nothing is due, and nothing is counted to find that out.
    #[test]
    fn a_store_with_no_nudge_declared_has_none_to_put() {
        let engine = StoreEngine::open_in_memory().unwrap();
        assert!(pending(&engine, |_| true).unwrap().is_empty());
    }

    const STAGED: &[Nudge] = &[Nudge {
        id: "test.staged",
        when: &[Threshold { metric: Metric::LaunchCount, at_least: 3 }],
        stage: Some("setting_unanswered"),
        once: true,
    }];

    /// The threshold is `at_least`: the nudge is due on the launch that reaches it, not the one after.
    #[test]
    fn a_threshold_is_met_at_its_boundary() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        for expected_due in [false, false, true] {
            crate::overview::record_launch(&engine, "2026-08-04").unwrap();
            let due = pending_from(STAGED, &engine, day, |_| true).unwrap();
            assert_eq!(!due.is_empty(), expected_due, "at {:?} launches", Metric::LaunchCount.count(&engine, day));
        }
    }

    /// The two cheap filters: a closed stage keeps a nudge back however far past its threshold the use
    /// is, and a once-only nudge that has been put is never put again.
    #[test]
    fn the_stage_and_the_log_each_hold_a_nudge_back() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        for _ in 0..5 {
            crate::overview::record_launch(&engine, "2026-08-04").unwrap();
        }
        assert!(
            pending_from(STAGED, &engine, day, |stage| {
                assert_eq!(stage, "setting_unanswered", "the declared stage is what is asked about");
                false
            })
            .unwrap()
            .is_empty(),
            "the stage it has a place in is closed"
        );

        let due = pending_from(STAGED, &engine, day, |_| true).unwrap();
        assert_eq!(due.iter().map(|n| n.id).collect::<Vec<_>>(), ["test.staged"]);

        mark_put(&engine, "test.staged").unwrap();
        assert!(
            pending_from(STAGED, &engine, day, |_| true).unwrap().is_empty(),
            "put once, and the log closes it"
        );
    }

    const REPEATING: &[Nudge] =
        &[Nudge { id: "test.repeating", when: &[], stage: Some("still_unanswered"), once: false }];

    /// A nudge that is not once-only is held by its stage alone — the log row records when it last went
    /// out, and does not close it.
    #[test]
    fn a_repeating_nudge_is_held_by_its_stage_and_not_by_the_log() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        assert_eq!(pending_from(REPEATING, &engine, day, |_| true).unwrap().len(), 1);
        mark_put(&engine, "test.repeating").unwrap();
        assert_eq!(
            pending_from(REPEATING, &engine, day, |_| true).unwrap().len(),
            1,
            "still due — nothing but the stage closes it"
        );
        assert!(pending_from(REPEATING, &engine, day, |_| false).unwrap().is_empty());
    }

    /// A metric a nudge does not reach is never counted: the store here holds no task table content, and
    /// the first unmet threshold stops the evaluation before the second metric is asked for.
    #[test]
    fn counting_stops_at_the_first_threshold_a_nudge_misses() {
        const TWO: &[Nudge] = &[Nudge {
            id: "test.two",
            when: &[
                Threshold { metric: Metric::LaunchCount, at_least: 99 },
                Threshold { metric: Metric::TaskCount, at_least: 1 },
            ],
            stage: None,
            once: true,
        }];
        let engine = StoreEngine::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        assert!(pending_from(TWO, &engine, day, |_| true).unwrap().is_empty());
    }

    /// The counted side reads the store: an empty store is empty on every count, and the windowed one
    /// answers `0` rather than reading the whole of it.
    #[test]
    fn the_counted_metrics_answer_over_an_empty_store() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        assert_eq!(Metric::TaskCount.count(&engine, day).unwrap(), 0);
        assert_eq!(Metric::ActiveDays.count(&engine, day).unwrap(), 0);
    }

    /// Active days: a day counts once however much was written on it, the tables are read together, and
    /// what is older than the window is not read at all.
    ///
    /// The rows go in as raw SQL — what is under test is the counting, and the projection that normally
    /// writes them would only put a second thing in the way of it. The instants are days away from the
    /// window's cut on either side, so no zone the test runs in can move a row across it.
    #[test]
    fn active_days_counts_a_day_once_across_the_tables_it_reads() {
        let engine = StoreEngine::open_in_memory().unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, updated_at) VALUES (1, '2026-08-04T01:00:00Z');
                 INSERT INTO task (id, updated_at) VALUES (1, '2026-08-04T01:00:00Z');
                 INSERT INTO task (id, updated_at) VALUES (2, '2026-08-04T23:00:00Z');
                 INSERT INTO task (id, updated_at) VALUES (3, '2026-01-01T00:00:00Z');
                 INSERT INTO decision (id, project_id, updated_at) \
                     VALUES (1, 1, '2026-06-01T00:00:00Z');",
            )
            .unwrap();
        assert_eq!(
            Metric::ActiveDays.count(&engine, day).unwrap(),
            2,
            "two tasks on one day are one day, the decision's day is another, and January is past the window"
        );
        assert_eq!(Metric::TaskCount.count(&engine, day).unwrap(), 3, "every task, window or not");
    }
}
