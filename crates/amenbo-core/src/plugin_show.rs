//! **What the author's code puts on the settings form** (`AMB-D-727`) — the parts a run may answer with,
//! and the whole of what Amenbo draws from somebody else's program.
//!
//! Before this, a run could say two things about itself: whether it went well, and one line of the
//! author's beside the button (`AMB-D-664`). That is not enough to get anyone through a setup. A QR has
//! to be looked at with a phone, an address has to be copied without being retyped, a token has to be
//! fetched from a page somebody has to be sent to — and the settings form had no way to carry any of it,
//! so `viewer` was writing a QR to a file and asking the operating system to open it, which is exactly
//! the sort of thing that fails silently on somebody else's machine.
//!
//! **The author declares, Amenbo draws.** A part carries strings and nothing else — no markup, no
//! markdown, no image bytes, no layout. A `qr` is the text to encode, not a picture; a `link` is a URL and
//! the words on the button. That line is what keeps a plugin a child process (`AMB-D-346`) instead of
//! something that ships a webview per platform, and it is what keeps the person reading the form able to
//! tell Amenbo's words from a stranger's.
//!
//! **One vocabulary, two runs.** A check's verdict ([`plugin_check::Verdict`](crate::plugin_check::Verdict))
//! and an operation's answer both carry `show`, read the same way here — one implementation, and one thing
//! for an author to learn.
//!
//! ```json
//! { "v": 1, "ok": true, "show": [
//!   { "text": "Read this with your phone's camera" },
//!   { "qr": "https://apps.apple.com/…" }
//! ]}
//! ```
//!
//! **`qr` and `link` are an official plugin's alone** (`AMB-D-727`), for the reason `AMB-D-575` draws its
//! line in the same place: both *carry a destination*, and a QR is the worse of the two — it is read by a
//! phone and opened outside Amenbo, where nothing here can stop it. A third party has `copy`, which puts
//! the same string in front of a person who can read it before going there. The badge is the catalog's and
//! an author cannot set it (`AMB-D-347`), so this is a line a machine cannot be wrong about. What a
//! stranger asked for is **dropped** rather than refused: the rest of what they wrote still draws.
//!
//! **The caps refuse rather than trim** (`AMB-D-727`: ten parts, four kilobytes). A form is not a
//! document, and what does not fit belongs on the execution log (`AMB-D-361`). Trimming would put Amenbo's
//! edit of the author's answer in front of a person as if it were theirs — the same reading
//! [`plugin_check`](crate::plugin_check) already takes of a sentence past its floor.

use serde_json::Value;

/// The most parts one answer may put on the form (`AMB-D-727`). A form is a place to fill things in, not
/// a page to read.
pub const MAX_PARTS: usize = 10;

/// The most one answer's parts may weigh, in bytes of the JSON they arrived as (`AMB-D-727`). The
/// companion to [`MAX_PARTS`]: ten parts are still ten parts when each one is a novel.
pub const MAX_SHOW_BYTES: usize = 4096;

/// One thing the author's run asked to have drawn (`AMB-D-727`).
///
/// Every variant holds strings the author supplied and nothing Amenbo would have to interpret: what a
/// part *looks* like is the screen's, and what it *says* is theirs. `Qr` and `Link` reach this type only
/// for an official plugin — [`read`] is where that is settled, so nothing downstream has to remember it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// A line of explanation, drawn plain.
    Text(String),
    /// A heading, to break a long answer into parts.
    Heading(String),
    /// A line that should stand out — a caution, a thing not to miss.
    Note(String),
    /// An ordered set of lines, drawn as a list.
    List(Vec<String>),
    /// A string with a copy button beside it, for what nobody should have to retype.
    Copy(String),
    /// A string to draw as a QR code. **Official plugins only.**
    Qr(String),
    /// A button that opens a URL in the reader's browser. **Official plugins only.**
    Link {
        /// Where it goes. Held to `http`/`https` by [`read`] — a button on a settings form is not a way
        /// to reach a scheme that starts something on this machine.
        url: String,
        /// The words on the button.
        label: String,
    },
}

impl Part {
    /// Whether this part carries a destination, and so rides for an official plugin alone
    /// (`AMB-D-727`).
    fn official_only(&self) -> bool {
        matches!(self, Part::Qr(_) | Part::Link { .. })
    }
}

/// Read a run's `show` into the parts Amenbo will draw (`AMB-D-727`).
///
/// `said` is the document's `show` value, or `None` for a run that wrote none — which is every plugin
/// written before this vocabulary existed, and is no parts rather than a fault. `official` is the badge
/// off the installed manifest (`AMB-D-347`).
///
/// `None` means **this answer cannot be drawn**, and what a caller does with that is its own: a check
/// reads it fail-closed and the whole verdict goes unread (`AMB-D-354`), while an operation's stdout has
/// never been consumed at all and simply draws nothing. It is `None` for a shape this build does not
/// speak — a part that is not an object, a kind with no name here, a value of the wrong type — for a
/// string carrying control characters, for a `link` pointing somewhere a browser should not be sent, and
/// for an answer past either cap.
///
/// The one thing dropped rather than refused is a third party's `qr` or `link`: they asked for something
/// that is not theirs to have, and the rest of what they wrote is still worth drawing.
pub fn read(said: Option<&Value>, official: bool) -> Option<Vec<Part>> {
    let Some(said) = said else { return Some(Vec::new()) };
    if said.is_null() {
        return Some(Vec::new());
    }
    let listed = said.as_array()?;
    if listed.len() > MAX_PARTS || said.to_string().len() > MAX_SHOW_BYTES {
        return None;
    }
    let mut parts = Vec::with_capacity(listed.len());
    for one in listed {
        let part = part(one)?;
        if part.official_only() && !official {
            continue;
        }
        parts.push(part);
    }
    Some(parts)
}

/// One part out of the array, or `None` for a shape this build does not speak.
///
/// A part is an object naming exactly one kind: `{"text": "…"}`. Two keys would be two parts written as
/// one, and a name this build does not know is an author's typo far more often than it is a plugin from
/// the future — the version marker is what says which vocabulary is being spoken, so a stray key here is
/// worth breaking on rather than swallowing.
fn part(said: &Value) -> Option<Part> {
    let fields = said.as_object()?;
    let (kind, value) = match fields.len() {
        1 => fields.iter().next()?,
        _ => return None,
    };
    Some(match kind.as_str() {
        "text" => Part::Text(line(value)?),
        "heading" => Part::Heading(line(value)?),
        "note" => Part::Note(line(value)?),
        "copy" => Part::Copy(line(value)?),
        "qr" => Part::Qr(line(value)?),
        "list" => Part::List(
            value.as_array()?.iter().map(line).collect::<Option<Vec<String>>>()?,
        ),
        "link" => {
            let link = value.as_object()?;
            let url = line(link.get("url")?)?;
            // A button that opens a browser opens a browser. `http`/`https` is what a destination on a
            // settings form is; anything else is a way to hand a scheme handler on this machine a string,
            // which is not what the author was offered here.
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return None;
            }
            Part::Link { url, label: line(link.get("label")?)? }
        }
        _ => return None,
    })
}

/// One string out of a part, held to the floor Amenbo puts under every author string it shows: a string,
/// with no control characters in it. The length is the answer's as a whole ([`MAX_SHOW_BYTES`]) rather
/// than each string's own — a `qr` carries a URL and a `text` carries a sentence, and one cap for both
/// would be wrong for one of them.
fn line(said: &Value) -> Option<String> {
    let said = said.as_str()?;
    if said.chars().any(char::is_control) {
        return None;
    }
    Some(said.to_string())
}

/// Read the parts one **operation** answered with (`AMB-D-727`) — the settings face's other run.
///
/// An operation's stdout has never been a return value the form consumes (`AMB-D-664`: what it drew was
/// the author's line on stderr), so a plugin that writes something else there is not doing anything
/// wrong and must not be turned into an error by this. Whatever cannot be read as an answer is **no
/// parts**, and the run's own verdict is untouched: the exit code is what says whether an operation
/// succeeded (`AMB-D-353`), and an `ok` written into this document is not consulted for one — that
/// reading belongs to the check, whose whole purpose is to answer it.
pub fn of_stdout(stdout: &str, official: bool) -> Vec<Part> {
    let Ok(document) = serde_json::from_str::<Value>(stdout.trim()) else {
        return Vec::new();
    };
    let Some(document) = document.as_object() else { return Vec::new() };
    if document.get("v").and_then(Value::as_u64) != Some(u64::from(crate::plugin_payload::VERSION)) {
        return Vec::new();
    }
    read(document.get("show"), official).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parts(said: Value, official: bool) -> Option<Vec<Part>> {
        read(Some(&said), official)
    }

    // ───────────────────────── the vocabulary (`AMB-D-727`) ─────────────────────────

    #[test]
    fn every_part_an_author_may_ask_for_is_read() {
        let read = parts(
            json!([
                { "text": "Read this with your phone" },
                { "heading": "Pair the device" },
                { "note": "This code expires in ten minutes" },
                { "list": ["Open the app", "Point the camera"] },
                { "copy": "https://greenhouse.example.test/board" },
                { "qr": "https://apps.apple.com/x" },
                { "link": { "url": "https://api.slack.com/apps", "label": "Create a webhook" } },
            ]),
            true,
        )
        .expect("the whole vocabulary is readable");
        assert_eq!(
            read,
            vec![
                Part::Text("Read this with your phone".into()),
                Part::Heading("Pair the device".into()),
                Part::Note("This code expires in ten minutes".into()),
                Part::List(vec!["Open the app".into(), "Point the camera".into()]),
                Part::Copy("https://greenhouse.example.test/board".into()),
                Part::Qr("https://apps.apple.com/x".into()),
                Part::Link {
                    url: "https://api.slack.com/apps".into(),
                    label: "Create a webhook".into(),
                },
            ]
        );
    }

    /// A run that says nothing to draw is every plugin written before this existed.
    #[test]
    fn a_run_that_asks_for_nothing_draws_nothing() {
        assert_eq!(read(None, true), Some(Vec::new()));
        assert_eq!(read(Some(&Value::Null), true), Some(Vec::new()));
        assert_eq!(parts(json!([]), true), Some(Vec::new()));
    }

    // ─────────── what carries a destination is official's alone (`AMB-D-727`) ───────────

    /// A stranger asked for something that is not theirs to have. The rest of what they wrote still
    /// draws — the whole answer is not thrown away over it.
    #[test]
    fn a_third_partys_qr_and_link_are_dropped_and_the_rest_stands() {
        let read = parts(
            json!([
                { "text": "Open the console" },
                { "qr": "https://apps.apple.com/x" },
                { "link": { "url": "https://example.test/x", "label": "Go" } },
                { "copy": "https://example.test/x" },
            ]),
            false,
        )
        .expect("what a third party may draw still draws");
        assert_eq!(
            read,
            vec![
                Part::Text("Open the console".into()),
                Part::Copy("https://example.test/x".into()),
            ],
            "copy is the third party's way of naming a destination"
        );
    }

    // ──────────────── what this build does not speak (`AMB-D-354`'s reading) ────────────────

    #[test]
    fn a_shape_this_build_does_not_speak_cannot_be_drawn() {
        for said in [
            json!({}),                                  // the parts are a list
            json!(["Read this"]),                       // a part is an object
            json!([{}]),                                // naming no kind
            json!([{ "text": "a", "note": "b" }]),      // two parts written as one
            json!([{ "banner": "a" }]),                 // a kind this build has no name for
            json!([{ "text": 7 }]),                     // a line is a string
            json!([{ "list": "one" }]),                 // a list is a list
            json!([{ "list": [7] }]),
            json!([{ "link": "https://example.test" }]), // a link is a url and its words
            json!([{ "link": { "url": "https://example.test" } }]),
            json!([{ "link": { "label": "Go" } }]),
            json!([{ "text": "one\ntwo" }]),            // the floor under every author string
        ] {
            assert!(parts(said.clone(), true).is_none(), "read as parts: {said}");
        }
    }

    /// A button on a settings form sends somebody to a page. A scheme that starts something on this
    /// machine is not that, and the author was never offered it.
    #[test]
    fn a_link_that_would_not_open_a_page_is_refused() {
        for url in ["file:///etc/passwd", "amenbo://open", "javascript:alert(1)", "/relative"] {
            assert!(
                parts(json!([{ "link": { "url": url, "label": "Go" } }]), true).is_none(),
                "read as a link: {url}"
            );
        }
    }

    // ─────────────────────────── the caps refuse (`AMB-D-727`) ───────────────────────────

    #[test]
    fn an_answer_past_the_caps_is_refused_rather_than_trimmed() {
        let one = json!({ "text": "x" });
        let at_the_cap = Value::Array(vec![one.clone(); MAX_PARTS]);
        assert_eq!(
            parts(at_the_cap, true).map(|p| p.len()),
            Some(MAX_PARTS),
            "the cap itself is allowed"
        );
        assert!(
            parts(Value::Array(vec![one; MAX_PARTS + 1]), true).is_none(),
            "an eleventh part is not trimmed away — the answer is refused whole"
        );
        let heavy = json!([{ "text": "x".repeat(MAX_SHOW_BYTES) }]);
        assert!(parts(heavy, true).is_none(), "one part can still be too much to draw");
    }

    // ──────────────── an operation's stdout was never a return value ────────────────

    #[test]
    fn an_operation_answers_with_the_parts_it_wrote() {
        let said = of_stdout(
            r#"{"v":1,"ok":true,"show":[{"qr":"https://apps.apple.com/x"}]}"#,
            true,
        );
        assert_eq!(said, vec![Part::Qr("https://apps.apple.com/x".into())]);
    }

    /// Anything else on an operation's stdout is not a fault of the author's: nothing consumed it before
    /// this vocabulary existed, so it draws nothing and the run stands as it is.
    #[test]
    fn an_operation_that_wrote_something_else_draws_nothing() {
        for stdout in [
            "",
            "done\n",
            "[]",
            r#"{"ok":true,"show":[{"text":"x"}]}"#,  // no version marker
            r#"{"v":2,"show":[{"text":"x"}]}"#,      // a version this build does not speak
            r#"{"v":1,"show":[{"banner":"x"}]}"#,    // an answer this build cannot draw
        ] {
            assert!(of_stdout(stdout, true).is_empty(), "drew something: {stdout}");
        }
    }

    /// The exit code is what says whether an operation succeeded (`AMB-D-353`), so an `ok` written into
    /// one of these documents changes nothing about the run — only the check reads that word.
    #[test]
    fn an_operations_own_ok_is_not_read_here() {
        assert_eq!(
            of_stdout(r#"{"v":1,"ok":false,"show":[{"text":"x"}]}"#, true),
            vec![Part::Text("x".into())]
        );
    }
}
