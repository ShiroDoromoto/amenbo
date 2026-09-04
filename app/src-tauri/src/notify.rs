//! What an OS notification is about — the one thing a toast has to carry besides its words.
//!
//! Amenbo speaks up for two reasons, and they are answered in different places: something arrived in
//! the inbox, which is a record on the board, and a pane handed the turn over, which is a terminal that
//! may be in a window of its own (`AMB-D-753`). A click has to land where the thing is, so the kind
//! travels with the toast — through the identifier on macOS, which is the only field of ours the OS
//! hands back, and beside the click on Windows.
//!
//! It is two words and not a boolean because a third reason would be a third word rather than an
//! argument nobody can read at the call site.
//!
//! **Where the kind reaches depends on the platform**, so the two halves that carry it are declared
//! where they are used. A toast on Linux goes out through the plugin, which carries no click of ours:
//! the kind is parsed there like anywhere else — the command takes the same argument on every
//! machine — and then there is nowhere for it to go, which is a thing that platform does not offer
//! rather than something left undone.

/// Which of Amenbo's two voices a toast is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Something is in the inbox. It is a count and names no single record, so a click opens the inbox.
    Arrival,
    /// A pane says its turn has come. It names no single pane either — which one is drawn on the rail
    /// and the pages — so a click opens the terminal.
    Turn,
}

impl Kind {
    /// The word that crosses to the front end, and the one written into a macOS identifier. It is only
    /// ever asked for where a click can be answered, which is the two platforms that carry one.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Arrival => "arrival",
            Kind::Turn => "turn",
        }
    }

    /// The kind a word names. Anything else is an arrival: the inbox is where this started and where
    /// a toast from an older build would have meant to go.
    pub fn parse(word: &str) -> Self {
        match word {
            "turn" => Kind::Turn,
            _ => Kind::Arrival,
        }
    }

    /// The kind a macOS notification identifier says it is (`amenbo-<kind>-<n>`). The identifier is
    /// the only field of ours the OS hands back on a click, and only that OS hands one back at all.
    #[cfg(target_os = "macos")]
    pub fn of(identifier: &str) -> Self {
        Self::parse(identifier.split('-').nth(1).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn an_identifier_says_which_voice_it_was() {
        assert_eq!(Kind::of("amenbo-turn-7"), Kind::Turn);
        assert_eq!(Kind::of("amenbo-arrival-7"), Kind::Arrival);
        // A toast an older build scheduled, or one whose identifier came back mangled: the inbox is
        // where this started, so a click that cannot be placed goes there rather than nowhere.
        assert_eq!(Kind::of("amenbo"), Kind::Arrival);
        assert_eq!(Kind::of(""), Kind::Arrival);
    }

    #[test]
    fn anything_unreadable_is_an_arrival_rather_than_nothing() {
        assert_eq!(Kind::parse("turn"), Kind::Turn);
        assert_eq!(Kind::parse("something else"), Kind::Arrival);
        assert_eq!(Kind::parse(""), Kind::Arrival);
    }
}
