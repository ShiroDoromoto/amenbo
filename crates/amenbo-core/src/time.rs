//! Timestamp and date utilities.
//!
//! - Timestamps are ISO 8601 UTC, second precision, with a trailing `Z` — one instant, shown in
//!   whatever zone the reader is in.
//! - Dates are `YYYY-MM-DD`, and relative forms are accepted too: `today` / `tomorrow` / `yesterday` /
//!   `+3d` / `-2d`. A date carries no zone at all: it is the reader's own calendar day, the same
//!   `2026-07-28` in Tokyo and in London (`AMB-D-429`). What that makes zone-dependent is only where
//!   the relative forms count from — see [`today`].

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};

/// A UTC timestamp. Through serde it is read and written as `2026-06-19T08:00:00Z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub DateTime<Utc>);

impl Timestamp {
    pub fn now() -> Self {
        // Sub-second precision is part of neither what we display nor what we promise, so drop it.
        let now = Utc::now();
        Timestamp(DateTime::from_timestamp(now.timestamp(), 0).unwrap_or(now))
    }

    pub fn to_rfc3339_z(&self) -> String {
        self.0.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    }

    /// Parse an RFC3339 UTC timestamp (the `to_rfc3339_z` form, or any RFC3339) back into a
    /// `Timestamp`. Returns `None` on malformed input. Used to recover a `Timestamp` from a value
    /// stored as text in the engine read-model (e.g. a comment's `created_at`) or a ledger line's `at`,
    /// for relative-time display.
    pub fn parse_rfc3339(s: &str) -> Option<Self> {
        DateTime::parse_from_rfc3339(s).ok().map(|d| Timestamp(d.with_timezone(&Utc)))
    }

    /// The calendar day this instant fell on **for the reader**, on this machine's clock.
    ///
    /// This is the only shape in which an instant may be compared with a day someone named
    /// (`--since today`, `decided_after:today`): a day comes from [`today`] and is local, so reading
    /// the instant's UTC date instead asks two different calendars the same question. Nine hours
    /// separate them in Tokyo, and between midnight and nine in the morning the two disagree — which
    /// is exactly when something accepted a minute ago drops out of "accepted today".
    pub fn local_date(&self) -> NaiveDate {
        self.date_in(&Local)
    }

    /// [`Timestamp::local_date`] with the zone said out loud, which is the only way a test can stand
    /// east of UTC: the reader's zone is whatever the machine is set to, and a run in UTC cannot tell
    /// the right answer from the wrong one.
    fn date_in<Tz: TimeZone>(&self, tz: &Tz) -> NaiveDate {
        self.0.with_timezone(tz).date_naive()
    }
}

impl Default for Timestamp {
    /// The Unix epoch (`1970-01-01T00:00:00Z`): a neutral zero for building a record with
    /// `..Default::default()`. Real records overwrite `created_at` / `updated_at` with
    /// `Timestamp::now()` as they are created.
    fn default() -> Self {
        Timestamp(DateTime::from_timestamp(0, 0).expect("unix epoch is a valid timestamp"))
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_rfc3339_z())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        let parsed = DateTime::parse_from_rfc3339(&raw)
            .map_err(serde::de::Error::custom)?
            .with_timezone(&Utc);
        Ok(Timestamp(parsed))
    }
}

/// Today's date on **this machine's clock**, which is the day the person typing is living in.
///
/// It is the base every relative date counts from (`today` / `tomorrow` / `+3d`) and the day
/// `overdue` / `today` / a start day's arrival are judged against. A due date is a calendar day and
/// carries no zone (`AMB-D-429`), so the only question is whose day it is — and asking UTC gives an
/// answer nine hours behind a reader in Tokyo, where `--due tomorrow` typed before nine in the
/// morning would land on today. Timestamps are a different thing and stay UTC ([`Timestamp`]).
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// The instant the reader's day `d` begins, in UTC.
///
/// The counterpart of [`Timestamp::local_date`] for a comparison that cannot be made in Rust: stored
/// instants are UTC text of one fixed width, so a query cuts them lexicographically, and the cut has
/// to be the instant the named day starts *here* rather than the bare `YYYY-MM-DD` — which is UTC's
/// midnight, and so the wrong hour everywhere else.
pub fn local_day_start_utc(d: NaiveDate) -> Timestamp {
    day_start_in(d, &Local)
}

/// [`local_day_start_utc`] with the zone said out loud — the testable half, as on
/// [`Timestamp::local_date`].
fn day_start_in<Tz: TimeZone>(d: NaiveDate, tz: &Tz) -> Timestamp {
    let midnight = d.and_hms_opt(0, 0, 0).expect("midnight is a civil time every day has");
    // Two days a year a zone has no single answer. Where the clock repeated the hour, the earlier of
    // the two is where the day began; where it jumped over midnight altogether, the day begins when
    // the clock lands, an hour on.
    let start = tz
        .from_local_datetime(&midnight)
        .earliest()
        .or_else(|| tz.from_local_datetime(&(midnight + Duration::hours(1))).earliest());
    match start {
        Some(t) => Timestamp(t.with_timezone(&Utc)),
        None => Timestamp(DateTime::from_naive_utc_and_offset(midnight, Utc)),
    }
}

/// Interpret a date expression. `base` is the day a relative form counts from — normally today.
pub fn parse_date(input: &str, base: NaiveDate) -> Result<NaiveDate> {
    let s = input.trim();
    match s {
        "today" => return Ok(base),
        "tomorrow" => return Ok(base + Duration::days(1)),
        "yesterday" => return Ok(base - Duration::days(1)),
        _ => {}
    }

    // Relative forms: +Nd / -Nd
    if let Some(rest) = s.strip_suffix('d').or_else(|| s.strip_suffix('D')) {
        if rest.starts_with('+') || rest.starts_with('-') {
            if let Ok(n) = rest.parse::<i64>() {
                return Ok(base + Duration::days(n));
            }
        }
    }

    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| Error::invalid(format!("date '{input}' must be in YYYY-MM-DD form (a real calendar date) or today/tomorrow/+3d, etc.")))
}

/// Render a date as a `YYYY-MM-DD` string.
pub fn date_to_string(d: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The base a relative date counts from is the machine's own calendar day. Asking UTC instead
    /// answers with a different day for most of the morning east of it, which is where `tomorrow`
    /// lands on today.
    #[test]
    fn today_is_the_machines_own_day() {
        assert_eq!(today(), Local::now().date_naive());
    }

    /// What just happened happened today, at every hour of the day. The two sides of every
    /// instant-against-a-day comparison have to come from one calendar, and this is the seam where
    /// they meet: `now()` is UTC, `today()` is local, and reading the instant's UTC date puts the
    /// small hours of a day east of UTC on the day before.
    #[test]
    fn an_instant_falls_on_the_day_the_reader_is_living_in() {
        assert_eq!(Timestamp::now().local_date(), today());
    }

    /// The same seam, read from a zone east of UTC, where the answer differs: half past eleven at
    /// night in London is already the next morning in Tokyo, and the reader there is living in the
    /// later day. A run on a machine set to UTC — every CI box — cannot see this from
    /// [`Timestamp::local_date`] alone, since there the two calendars agree.
    #[test]
    fn a_day_east_of_utc_has_already_turned_over() {
        let tokyo = chrono::FixedOffset::east_opt(9 * 3600).expect("+09:00 is a zone offset");
        let late_in_london =
            Timestamp::parse_rfc3339("2026-07-28T23:30:00Z").expect("a well-formed instant");
        assert_eq!(
            late_in_london.date_in(&tokyo),
            NaiveDate::from_ymd_opt(2026, 7, 29).expect("a real day")
        );

        // And the cut a query makes for that Tokyo day starts nine hours before UTC's own midnight.
        let day = NaiveDate::from_ymd_opt(2026, 7, 29).expect("a real day");
        assert_eq!(day_start_in(day, &tokyo).to_rfc3339_z(), "2026-07-28T15:00:00Z");
    }
}
