//! **Reading what the reader's own Worker said when it would not do the thing** (`AMB-D-884`, ported from
//! the `viewer` plugin the retreat retired).
//!
//! **A refusal is the only thing this side can tell the person about their server.** Nothing here can
//! look inside their Cloudflare account, and what is written here is the one place their question — "why
//! is my phone not updating?" — is answered. So a refusal has to leave with two things on it: what
//! happened, and what to do about it.
//!
//! The Worker writes its own sentence for the first, and for some answers that sentence is the whole of
//! it: a database with no room left already says that raising the account is what makes room. For the
//! rest — a token that no longer opens the door, a route the Worker does not have — the Worker cannot know
//! what the person should do, because what is wrong is on this side. That half is [`what_to_do_about`].
//!
//! And some answers are not the Worker's at all: an exception it did not catch is answered by Cloudflare
//! in front of it, as a plain page rather than the Worker's JSON. Reading that as "the Worker said
//! nothing" would lose the one number on it worth having.

use std::time::Duration;

use chrono::{DateTime, Utc};

use super::pace::{the_wait_asked, the_wait_in_words};

/// As much of a non-JSON answer as is worth repeating: one line, bounded, and nothing of the request that
/// produced it. Cloudflare's pages are the reason it exists — they carry a fault number and a lot of
/// markup around it.
const BODY_OF_ONE_LINE: usize = 200;

/// The sentence D1 throws when the database has no room left, and the only thing that says it: the binding
/// throws a plain error with no code and no status, and the `7500` its REST API answers with never reaches
/// a Worker.
///
/// **Everything else has to fall through.** A database that was briefly unreachable, read as "buy more
/// storage", sends someone to pay for nothing.
const WHAT_D1_SAYS_WHEN_IT_IS_FULL: &str = "Exceeded maximum DB size";

/// A door answering that it would not.
///
/// It is a value rather than a sentence so that a caller with a reading of its own — a send honouring the
/// moment it was told to come back at — can take the answers it knows and leave the rest to the words
/// here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// The door, named the way the Worker's own routes are.
    pub path: String,
    /// What it answered.
    pub status: u16,
    /// The Worker's own sentence, or empty when what came back was not its JSON.
    pub said: String,
    /// What came back when it was not the Worker's JSON, trimmed to a line. Cloudflare's own error pages
    /// carry a number that says which fault it was, and that is worth keeping.
    pub page: String,
    /// The `Retry-After` the answer carried, if it carried one.
    pub wait_for: String,
}

impl Refused {
    /// What this refusal says, as a line for whoever is trying to fix it.
    ///
    /// **Which of the two leads depends on whose side the fault is.** When the Worker's own sentence
    /// carries the move, it is the sentence and nothing is put in front of it. When the fault is here,
    /// the move leads and the Worker's words follow in quotes — they describe a shape mismatch in its
    /// terms, which is worth having and is not what the person should act on.
    pub fn in_words(&self, now: DateTime<Utc>) -> String {
        let said = match (self.said.as_str(), self.page.as_str()) {
            ("", "") => String::new(),
            ("", page) => format!(
                "the Worker itself did not answer — this came from Cloudflare in front of it: {page}"
            ),
            (said, _) => said.to_string(),
        };
        let next = what_to_do_about(self.status, &self.wait_for, &said, now);
        match (next, said.is_empty()) {
            (Some(next), false) => {
                format!("{} answered {} — {next} (it said: {:?})", self.path, self.status, said)
            }
            (Some(next), true) => format!("{} answered {} — {next}", self.path, self.status),
            (None, false) => format!("{} answered {}: {said}", self.path, self.status),
            (None, true) => format!("{} answered {}", self.path, self.status),
        }
    }

    /// How long this refusal asks to be left for, when it asks at all — what a caller honours instead of
    /// reading the words.
    pub fn asks_to_be_left_for(&self, now: DateTime<Utc>) -> Option<Duration> {
        the_wait_asked(&self.wait_for, now)
    }
}

/// The move a refusal calls for, or `None` when the Worker's own sentence is already the whole of it.
///
/// **The refusals worth adding to are the ones whose cause is on this side.** A database with no room left
/// is the Worker's to explain and it does; a token that no longer opens its door is not something the
/// Worker can know the fix for, because the fix is here.
pub fn what_to_do_about(
    status: u16,
    wait_for: &str,
    said: &str,
    now: DateTime<Utc>,
) -> Option<String> {
    // **A full database is read before the wait, because it wears one.** Every exception the Worker meets
    // comes back as a 503 with a `Retry-After` on it — it stopped reading D1's sentence on purpose, that
    // being the one part of this that cannot be corrected when a reading turns out wrong. So the reading
    // lives here, where a new build can fix it, and it has to come first: a database that has used its
    // room is not going to be different in a minute, and telling someone to wait for one is telling them
    // to wait forever.
    if the_database_is_full(said) {
        return Some(
            "the database has no room left — waiting will not change that, and nothing more will fit \
             until the Cloudflare account it is in is raised to a larger plan"
                .to_string(),
        );
    }
    // An answer that says when to come back is a wait, whatever else it says. That is HTTP's own reading
    // of it, and it is the one case where doing nothing is the right move. The number is read rather than
    // repeated: a build that pasted the header into a sentence about seconds said things like "left for
    // Wed, 30 Aug 2026 06:00:00 GMT seconds".
    if let Some(wait) = the_wait_asked(wait_for, now) {
        return Some(format!(
            "this one clears itself — the server asked to be left for {}",
            the_wait_in_words(wait)
        ));
    }
    match status {
        401 | 403 => Some(
            "the token this device writes with is not the one that Worker takes — pressing setup \
             stands the server up again and writes a new one"
                .to_string(),
        ),
        503 => Some(
            "the Worker is standing but its write token was never set on it — pressing setup \
             finishes what a deploy left half done"
                .to_string(),
        ),
        400 | 404 | 405 | 413 => Some(
            "this build and that Worker do not agree on the route — the one in the account came from \
             another version, and pressing setup deploys the one this build carries"
                .to_string(),
        ),
        _ => None,
    }
}

/// Whether the database has no room left, read out of whatever the server passed on.
pub fn the_database_is_full(said: &str) -> bool {
    said.contains(WHAT_D1_SAYS_WHEN_IT_IS_FULL)
}

/// Flatten an answer that was not the Worker's JSON into something one line can hold: the whitespace
/// collapsed, and cut where it stops being worth reading.
pub fn one_line_of(raw: &str) -> String {
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(BODY_OF_ONE_LINE) {
        Some((at, _)) => format!("{}…", &flat[..at]),
        None => flat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(when: &str) -> DateTime<Utc> {
        crate::time::Timestamp::parse_rfc3339(when).expect("an instant").0
    }

    fn refused(status: u16, said: &str, wait_for: &str) -> Refused {
        Refused {
            path: "/records".into(),
            status,
            said: said.into(),
            page: String::new(),
            wait_for: wait_for.into(),
        }
    }

    /// A full database is read before the wait it wears. Every exception the Worker meets comes back as a
    /// 503 with a `Retry-After`, so reading the wait first would tell someone to wait a minute for
    /// something that will never be different.
    #[test]
    fn a_full_database_is_read_before_the_wait_it_wears() {
        let now = at("2026-09-14T10:00:00Z");
        let full = refused(503, "D1_ERROR: Exceeded maximum DB size", "60");
        let said = full.in_words(now);
        assert!(said.contains("no room left"), "{said}");
        assert!(said.contains("larger plan"), "{said}");
        assert!(!said.contains("left for"), "a wait was read where waiting cannot help: {said}");
    }

    /// An answer that says when to come back is a wait, whatever else it says — and the number is read
    /// rather than repeated.
    #[test]
    fn a_wait_is_read_rather_than_repeated() {
        let now = at("2026-08-30T05:00:00Z");
        let said = refused(503, "the database is busy", "Wed, 30 Aug 2026 06:00:00 GMT").in_words(now);
        assert!(said.contains("clears itself"), "{said}");
        assert!(said.contains("left for 60m"), "{said}");
        assert!(!said.contains("GMT seconds"), "the header went into the sentence whole: {said}");
    }

    /// A token the Worker no longer takes is this side's to fix, so the move leads and the Worker's own
    /// words follow it in quotes.
    #[test]
    fn a_refusal_this_side_caused_leads_with_the_move() {
        let now = at("2026-09-14T10:00:00Z");
        let said = refused(401, "a bearer token is required", "").in_words(now);
        assert!(said.starts_with("/records answered 401 — "), "{said}");
        assert!(said.contains("pressing setup"), "{said}");
        assert!(said.contains("\"a bearer token is required\""), "{said}");
    }

    /// A refusal the Worker explains for itself is left to say it, with nothing put in front.
    #[test]
    fn a_refusal_the_worker_explains_is_left_to_say_it() {
        let now = at("2026-09-14T10:00:00Z");
        let said = refused(409, "the order here has reached 12, and you asked to read on from 40", "")
            .in_words(now);
        assert_eq!(
            said,
            "/records answered 409: the order here has reached 12, and you asked to read on from 40"
        );
    }

    /// An answer that was not the Worker's JSON came from Cloudflare in front of it, and the number on
    /// that page is the one thing worth keeping.
    #[test]
    fn a_page_from_in_front_of_the_worker_is_not_silence() {
        let now = at("2026-09-14T10:00:00Z");
        let page = Refused {
            path: "/records".into(),
            status: 500,
            said: String::new(),
            page: one_line_of("<html>\n  <h1>Error 1101</h1>\n  Worker threw exception\n</html>"),
            wait_for: String::new(),
        };
        let said = page.in_words(now);
        assert!(said.contains("Cloudflare in front of it"), "{said}");
        assert!(said.contains("Error 1101"), "{said}");
    }

    /// A page is flattened to one line and cut where it stops being worth reading.
    #[test]
    fn a_page_is_cut_where_it_stops_being_worth_reading() {
        assert_eq!(one_line_of("  a\n b\t c  "), "a b c");
        let long = one_line_of(&"x".repeat(500));
        assert_eq!(long.chars().count(), BODY_OF_ONE_LINE + 1);
        assert!(long.ends_with('…'));
    }

    /// A refusal with nothing on it at all still says which door answered what.
    #[test]
    fn a_refusal_with_nothing_on_it_says_which_door_answered_what() {
        let now = at("2026-09-14T10:00:00Z");
        let bare = Refused {
            path: "/meta".into(),
            status: 418,
            said: String::new(),
            page: String::new(),
            wait_for: String::new(),
        };
        assert_eq!(bare.in_words(now), "/meta answered 418");
    }

    /// The wait is handed over as a number as well as in words: a caller honouring it must not have to
    /// read the sentence back.
    #[test]
    fn the_wait_is_handed_over_as_a_number_too() {
        let now = at("2026-09-14T10:00:00Z");
        assert_eq!(refused(503, "", "60").asks_to_be_left_for(now), Some(Duration::from_secs(60)));
        assert_eq!(refused(503, "", "").asks_to_be_left_for(now), None);
    }
}
