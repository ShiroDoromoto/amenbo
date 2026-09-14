//! **How fast the Viewer's carrier may go, and when it may not go at all** (`AMB-D-884`, ported from the
//! `viewer` plugin the retreat retired).
//!
//! Both answers come from the far end, and both are remembered ([`super::carried`]). The server says
//! "come back in N seconds" when it cannot answer, and says how many rows each write actually cost; a
//! carrier that forgot either between runs would learn the same lesson on every send.
//!
//! **Neither one stops the copying.** What is read out of the backlog has to keep up with it whatever the
//! network is doing — the feed's window turns, and what is not copied out in time is not copied out at
//! all. Waiting and being out of budget are the sending's business, and the sending is free to fall
//! behind.

use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};

use super::carried::Carried;

/// What a Cloudflare account on the free plan may write to D1 in a day. It is the number this is measured
/// against, and the one the pause is worked out from.
const ROWS_A_DAY: i64 = 100_000;

/// Where this stops. **The room left over is not slack — it is everything else in the account**: a second
/// database, the same one's own reads-with-writes, whatever the person runs beside this. A carrier that
/// spent the whole allowance would be the reason the rest of their account stopped working, which is not a
/// trade it is entitled to make on their behalf.
///
/// **A paid account is bounded by the same number here**, and this cannot see which plan it is. What that
/// costs is a pause, not a loss: the queue keeps what it could not send, and midnight UTC lifts it. What
/// it buys is that the free plan — where the wall is real — never reaches it.
pub const ROWS_WE_MAY_SPEND_A_DAY: i64 = ROWS_A_DAY * 9 / 10;

/// How long one refusal may hold the carrier. The wait is the server's to name and it is honoured, but a
/// number nothing here produced must not be able to wedge the sending for a week — and an hour is already
/// far past any refusal that clears itself.
pub const LONGEST_QUIET: Duration = Duration::from_secs(60 * 60);

/// The day a spend is counted against.
///
/// **It is UTC because the limit is**: D1's allowance turns over at midnight UTC, so counting against a
/// local day would reset this in the middle of the server's, or hold it past the server's own turnover.
pub fn the_day(now: DateTime<Utc>) -> String {
    now.format("%Y-%m-%d").to_string()
}

/// How many rows the carrier has cost today. A count carrying another day's date is not this day's, and
/// reads as nothing spent — which is the whole of the rollover. There is nothing to reset and nothing to
/// run at midnight.
pub fn spent_today(left: &Carried, now: DateTime<Utc>) -> i64 {
    match left.spent_on.as_deref() {
        Some(day) if day == the_day(now) => left.spent,
        _ => 0,
    }
}

/// Add what one write cost to today's count.
pub fn spend(left: &Carried, rows: i64, now: DateTime<Utc>) -> Carried {
    Carried {
        spent: spent_today(left, now) + rows,
        spent_on: Some(the_day(now)),
        ..left.clone()
    }
}

/// Whether today's allowance is gone.
///
/// **It is asked before a write and never during one.** What a write will cost is not knowable from this
/// side — an upsert onto a key already there costs a different number from one onto a key that is not —
/// so the count can only be read after the fact, and this can overshoot by at most one request's worth.
/// Guessing the cost instead would put a model of that database in here, to go stale the day it changes.
pub fn out_of_budget(left: &Carried, now: DateTime<Utc>) -> bool {
    spent_today(left, now) >= ROWS_WE_MAY_SPEND_A_DAY
}

/// How much of the quiet the server asked for is left, or `None` when it asked for none or the moment it
/// named has come.
///
/// A mark this build cannot read is no mark: the sending goes ahead, and the server says so again if it
/// meant it.
pub fn quiet(left: &Carried, now: DateTime<Utc>) -> Option<Duration> {
    let until = crate::time::Timestamp::parse_rfc3339(left.quiet_until.as_deref()?)?;
    (until.0 - now).to_std().ok().filter(|left| !left.is_zero())
}

/// **Whether a queue left standing is one nobody is coming back for** — the question a startup asks before
/// it starts a carrier nothing asked for ([`crate::Store::carry_what_was_left_behind`]).
///
/// A carrier is a process, and a process can die between reading the backlog out and placing it. What it
/// leaves is a queue, and the only thing that sets a carrier off is a write — so on a device nobody writes
/// to again, those rows sit there and the phone goes on showing what it had.
///
/// **But a queue standing still is not the same as a queue abandoned**, and the difference is what this
/// answers. Three of the four states here are the queue waiting on purpose:
///
/// - nothing is waiting, which is every device nobody has set the Viewer up on — nothing is ever read out
///   where there is no server, and nothing where the switch is off;
/// - the switch is off, and a queue kept across it is a queue meant to sit there until it comes back on;
/// - the far end asked to be left for a while, or today's allowance is spent. Both lift by themselves, and
///   a carrier started into either would open the store, read the same mark and exit.
///
/// **The turn is deliberately not among them.** Taking the hold to see whether anybody has it *is* taking
/// it, and a live carrier meeting that probe would stand down on a turn nobody was going to take. So a
/// start made while somebody else is carrying costs one process that finds the turn taken and stops —
/// which is what every write in a burst already costs, by the same design ([`super::lock`]).
pub fn stuck(waiting: i64, carrying: bool, left: &Carried, now: DateTime<Utc>) -> bool {
    waiting > 0 && carrying && quiet(left, now).is_none() && !out_of_budget(left, now)
}

/// Write down that the carrier is not to send for a while.
pub fn be_quiet_for(left: &Carried, wait: Duration, now: DateTime<Utc>) -> Carried {
    if wait.is_zero() {
        return left.clone();
    }
    let wait = wait.min(LONGEST_QUIET);
    let until = now + chrono::Duration::from_std(wait).unwrap_or(chrono::Duration::zero());
    Carried {
        quiet_until: Some(crate::time::Timestamp(until).to_rfc3339_z()),
        ..left.clone()
    }
}

/// Read a `Retry-After` into how long it asks for, or `None` when it asks for nothing this build can read.
///
/// **HTTP writes it two ways and both arrive here.** A number is seconds from now; a date is the moment to
/// come back at — and reading only the first puts the date itself into a sentence, which is how a refusal
/// comes out saying it has been asked to wait for `Wed, 30 Aug 2026 06:00:00 GMT seconds`.
///
/// A date already past is a wait of nothing, which is still a wait that was asked for: the caller learns
/// that the answer named one, and waits for no time at all.
pub fn the_wait_asked(header: &str, now: DateTime<Utc>) -> Option<Duration> {
    let header = header.trim();
    if header.is_empty() {
        return None;
    }
    if let Ok(seconds) = header.parse::<i64>() {
        return Some(Duration::from_secs(seconds.max(0) as u64));
    }
    let when = http_date(header)?;
    Some((when - now).to_std().unwrap_or(Duration::ZERO))
}

/// A wait as a log line says it, rounded to the second — nobody reading one acts on the milliseconds, and
/// the raw figure puts nine digits in the middle of a sentence.
pub fn the_wait_in_words(wait: Duration) -> String {
    let seconds = wait.as_secs();
    let (minutes, seconds) = (seconds / 60, seconds % 60);
    match (minutes, seconds) {
        (0, s) => format!("{s}s"),
        (m, 0) => format!("{m}m"),
        (m, s) => format!("{m}m{s}s"),
    }
}

/// The moment an HTTP date names. The preferred form is what a server writes today; the two obsolete ones
/// are read as well, because a header is whatever arrives rather than whatever is preferred.
///
/// **The day name in front is dropped rather than checked.** It says nothing the date does not, so a
/// server that writes the wrong one has still named the moment — and refusing to read it would turn a
/// wait that was asked for into a send that goes out anyway.
fn http_date(header: &str) -> Option<DateTime<Utc>> {
    const FORMS: &[&str] = &[
        // IMF-fixdate, the one a server is told to write.
        "%d %b %Y %H:%M:%S GMT",
        // RFC 850, obsolete and still emitted.
        "%d-%b-%y %H:%M:%S GMT",
        // asctime, obsolete and still emitted.
        "%b %e %H:%M:%S %Y",
    ];
    let after_the_day_name = header
        .split_once(", ")
        .map(|(_, rest)| rest)
        .or_else(|| header.split_once(' ').map(|(_, rest)| rest))?;
    FORMS
        .iter()
        .find_map(|form| NaiveDateTime::parse_from_str(after_the_day_name.trim(), form).ok())
        .map(|naive| naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(when: &str) -> DateTime<Utc> {
        crate::time::Timestamp::parse_rfc3339(when).expect("an instant").0
    }

    fn spent(rows: i64, day: &str) -> Carried {
        Carried { spent: rows, spent_on: Some(day.into()), ..Carried::default() }
    }

    /// The day a spend belongs to is UTC, because the allowance it is measured against turns over at
    /// midnight UTC. A local day would reset this in the middle of the server's own.
    #[test]
    fn a_spend_is_counted_against_the_utc_day() {
        assert_eq!(the_day(at("2026-09-14T23:59:59Z")), "2026-09-14");
        assert_eq!(the_day(at("2026-09-15T00:00:00Z")), "2026-09-15");
    }

    /// A count carrying another day's date reads as nothing spent. That is the whole of the rollover —
    /// nothing resets it, and nothing runs at midnight.
    #[test]
    fn yesterdays_count_is_not_todays() {
        let yesterday = spent(90_000, "2026-09-13");
        assert_eq!(spent_today(&yesterday, at("2026-09-14T00:00:01Z")), 0);
        assert!(!out_of_budget(&yesterday, at("2026-09-14T00:00:01Z")));
        assert!(out_of_budget(&yesterday, at("2026-09-13T23:59:59Z")));
    }

    /// What a write cost is added to today's count, and a write on a new day starts it again.
    #[test]
    fn what_a_write_cost_is_added_to_the_day_it_was_written_on() {
        let after = spend(&spent(1_000, "2026-09-14"), 500, at("2026-09-14T10:00:00Z"));
        assert_eq!((after.spent, after.spent_on.as_deref()), (1_500, Some("2026-09-14")));

        let tomorrow = spend(&after, 500, at("2026-09-15T10:00:00Z"));
        assert_eq!((tomorrow.spent, tomorrow.spent_on.as_deref()), (500, Some("2026-09-15")));
    }

    /// A queue with rows in it, a switch that is on and nothing asking for a wait is a queue whose carrier
    /// is not coming back — which is the one state a startup starts a new one for.
    #[test]
    fn a_queue_left_by_a_carrier_that_died_is_one_to_start_another_for() {
        let now = at("2026-09-14T10:00:00Z");
        assert!(stuck(3, true, &Carried::default(), now));
    }

    /// And the three states that are the queue waiting on purpose. Each is read without starting anything,
    /// which is the whole reason they are asked: a process started into any of them would open the store,
    /// read the same mark and exit.
    #[test]
    fn a_queue_waiting_on_purpose_is_left_where_it_is() {
        let now = at("2026-09-14T10:00:00Z");

        assert!(!stuck(0, true, &Carried::default(), now), "nothing is waiting");
        assert!(!stuck(3, false, &Carried::default(), now), "the switch is off, and the queue keeps");

        let asked_to_wait = be_quiet_for(&Carried::default(), Duration::from_secs(60), now);
        assert!(!stuck(3, true, &asked_to_wait, now), "the server asked to be left for a while");
        // And the moment it named having come, the same queue is one to carry.
        assert!(stuck(3, true, &asked_to_wait, at("2026-09-14T10:02:00Z")));

        let spent_up = spent(ROWS_WE_MAY_SPEND_A_DAY, "2026-09-14");
        assert!(!stuck(3, true, &spent_up, now), "today's allowance is gone");
        assert!(stuck(3, true, &spent_up, at("2026-09-15T00:00:01Z")), "and midnight UTC lifts it");
    }

    /// The line is under the whole allowance. What is left over is the rest of the person's own account,
    /// which this is not entitled to spend on their behalf.
    #[test]
    fn the_line_leaves_the_rest_of_the_account_room() {
        const { assert!(ROWS_WE_MAY_SPEND_A_DAY < ROWS_A_DAY) };
        assert!(!out_of_budget(&spent(ROWS_WE_MAY_SPEND_A_DAY - 1, "2026-09-14"), at("2026-09-14T00:00:00Z")));
        assert!(out_of_budget(&spent(ROWS_WE_MAY_SPEND_A_DAY, "2026-09-14"), at("2026-09-14T00:00:00Z")));
    }

    /// A quiet that was asked for holds until the moment it named, and no longer.
    #[test]
    fn a_quiet_holds_until_the_moment_it_named() {
        let left = be_quiet_for(&Carried::default(), Duration::from_secs(60), at("2026-09-14T10:00:00Z"));
        assert_eq!(quiet(&left, at("2026-09-14T10:00:30Z")), Some(Duration::from_secs(30)));
        assert_eq!(quiet(&left, at("2026-09-14T10:01:00Z")), None);
        assert_eq!(quiet(&left, at("2026-09-14T11:00:00Z")), None);
    }

    /// A wait longer than an hour is held to an hour. The server names the wait and it is honoured, but a
    /// number nothing here produced must not be able to wedge the sending for a week.
    #[test]
    fn a_wait_nothing_here_produced_cannot_wedge_the_sending() {
        let now = at("2026-09-14T10:00:00Z");
        let left = be_quiet_for(&Carried::default(), Duration::from_secs(60 * 60 * 24 * 7), now);
        assert_eq!(quiet(&left, now), Some(LONGEST_QUIET));
    }

    /// A wait of nothing writes no mark: there is nothing to wait for, and a mark would only be read back
    /// as already past.
    #[test]
    fn a_wait_of_nothing_writes_no_mark() {
        let left = be_quiet_for(&Carried::default(), Duration::ZERO, at("2026-09-14T10:00:00Z"));
        assert_eq!(left.quiet_until, None);
    }

    /// A mark this build cannot read is no mark: the sending goes ahead, and the server says so again if
    /// it meant it.
    #[test]
    fn a_mark_that_cannot_be_read_is_no_mark() {
        let left = Carried { quiet_until: Some("soon".into()), ..Carried::default() };
        assert_eq!(quiet(&left, at("2026-09-14T10:00:00Z")), None);
    }

    /// `Retry-After` arrives as seconds and as a date, and both are read. A build that read only the
    /// first put the date itself into a sentence about seconds.
    #[test]
    fn a_retry_after_is_read_whichever_way_it_is_written() {
        let now = at("2026-08-30T05:00:00Z");
        assert_eq!(the_wait_asked("60", now), Some(Duration::from_secs(60)));
        assert_eq!(the_wait_asked("  60  ", now), Some(Duration::from_secs(60)));
        assert_eq!(
            the_wait_asked("Wed, 30 Aug 2026 06:00:00 GMT", now),
            Some(Duration::from_secs(60 * 60))
        );
        assert_eq!(the_wait_asked("", now), None);
        assert_eq!(the_wait_asked("soon", now), None);
    }

    /// The day name in front of a date is dropped rather than judged. It says nothing the date does not,
    /// and a server that writes the wrong one has still named the moment — 30 August 2026 was a Sunday.
    #[test]
    fn the_day_name_in_front_of_a_date_is_not_judged() {
        let now = at("2026-08-30T05:00:00Z");
        assert_eq!(
            the_wait_asked("Sun, 30 Aug 2026 06:00:00 GMT", now),
            the_wait_asked("Wed, 30 Aug 2026 06:00:00 GMT", now)
        );
    }

    /// A date already past, and a number that is not positive, are both a wait of nothing — which is
    /// still a wait that was asked for. The caller learns the answer named one and waits no time at all.
    #[test]
    fn a_wait_already_over_is_still_a_wait_that_was_asked_for() {
        let now = at("2026-08-30T07:00:00Z");
        assert_eq!(the_wait_asked("Wed, 30 Aug 2026 06:00:00 GMT", now), Some(Duration::ZERO));
        assert_eq!(the_wait_asked("0", now), Some(Duration::ZERO));
        assert_eq!(the_wait_asked("-5", now), Some(Duration::ZERO));
    }

    /// A wait reads as a person says one, not as nine digits in the middle of a sentence.
    #[test]
    fn a_wait_reads_the_way_a_person_says_one() {
        assert_eq!(the_wait_in_words(Duration::from_millis(45_400)), "45s");
        assert_eq!(the_wait_in_words(Duration::from_secs(60)), "1m");
        assert_eq!(the_wait_in_words(Duration::from_secs(90)), "1m30s");
    }
}
