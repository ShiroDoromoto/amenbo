//! Timestamp and date utilities.
//!
//! - Timestamps are ISO 8601 UTC, second precision, with a trailing `Z` — one instant, shown in
//!   whatever zone the reader is in.
//! - Dates are `YYYY-MM-DD`, and relative forms are accepted too: `today` / `tomorrow` / `yesterday` /
//!   `+3d` / `-2d`. A date carries no zone at all: it is the reader's own calendar day, the same
//!   `2026-07-28` in Tokyo and in London (`AMB-D-429`). What that makes zone-dependent is only where
//!   the relative forms count from — see [`today`].

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
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
}
