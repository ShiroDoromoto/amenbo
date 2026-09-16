//! The wire between the helper git starts and the window that answers it.
//!
//! git and ssh both know one way to ask a person for a password without a terminal: they run the
//! program named in `GIT_ASKPASS` / `SSH_ASKPASS`, hand it the question as its one argument, and
//! read the answer off its standard output. Neither of them can be handed a callback, so the thing
//! that answers has to be a file on disk — this crate's own binary (`src/main.rs`), shipped beside
//! the app as a Tauri sidecar — and that file then has to reach back to the window the person is
//! looking at.
//!
//! **This module is the seam both sides are written against**, so that neither can drift from the
//! other: the helper links it to write a question, the app links it to read one. The names of the
//! two variables are here for the same reason — the app sets them on the child, the helper reads
//! them out of its own environment, and a rename that reached only one of them would leave git
//! starting a helper that quietly finds nothing to talk to.
//!
//! **Loopback TCP, and not a Unix socket.** Of the three ways measured (`AMB-T-4968`) — a Node
//! process, TCP, a Unix socket — TCP is the only one that is the same shape on all three operating
//! systems this ships to. The socket file has no standard-library form on Windows at all, and a
//! second mechanism there would mean a second wire to keep true.
//!
//! **What stands in for the access a loopback port does not have**: any process on the machine can
//! connect to `127.0.0.1`, so the port alone would let anything running as this person pull up a
//! password dialog wearing Amenbo's face. The token closes that — it is drawn afresh for each run
//! of the app, it reaches the helper only by inheritance from the app itself, and a request that
//! does not carry it is answered with nothing (`AMB-D-913`).

use std::io::Read;

/// Where the helper calls back to: `127.0.0.1:<port>`, as the app's own listener reports itself.
///
/// The port is asked for rather than fixed — a fixed one is a port somebody else already has — so
/// the number is not knowable until the app is running, which is why it travels in the environment
/// rather than being compiled into either side.
pub const ADDR_VAR: &str = "AMENBO_ASKPASS_ADDR";

/// The word that says a request came from this app's own children and not from something else on
/// the machine that found the port.
pub const TOKEN_VAR: &str = "AMENBO_ASKPASS_TOKEN";

/// The first byte of an answer that has a value behind it; the rest of the bytes are the value.
const GRANTED: u8 = b'+';

/// The whole of an answer with nothing behind it — nobody was there, the person closed the dialog,
/// or the request was not this app's to answer.
const REFUSED: u8 = b'-';

/// A question, as it goes down the wire: the token on the first line, then the question itself to
/// the end of the stream.
///
/// **The question is not escaped and does not need to be.** git's prompts are one line, ssh's run
/// to several (`Enter passphrase for key …`), and either way the stream ends where the question
/// does — the sender half-closes, which is a frame no byte inside the question can imitate. The
/// token is drawn as hex, so the one newline that separates the two is unambiguous.
pub fn question(token: &str, asked: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(token.len() + asked.len() + 1);
    out.extend_from_slice(token.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(asked.as_bytes());
    out
}

/// Read back what [`question`] wrote: the token, and what is being asked.
///
/// `None` where the bytes are not a question at all — no newline to end the token, or text that is
/// not UTF-8. Both are the answer to something that is not this app's helper talking, which is the
/// case this returns rather than guesses at.
pub fn read_question(bytes: &[u8]) -> Option<(String, String)> {
    let at = bytes.iter().position(|b| *b == b'\n')?;
    let token = std::str::from_utf8(&bytes[..at]).ok()?;
    let asked = std::str::from_utf8(&bytes[at + 1..]).ok()?;
    Some((token.to_owned(), asked.to_owned()))
}

/// An answer, as it goes back: one byte saying whether there is a value, then the value.
///
/// The byte is what keeps "the person typed nothing and pressed return" apart from "there was
/// nobody to ask". git reads an empty password as a password, so the two cannot share a shape.
pub fn answer(said: Option<&str>) -> Vec<u8> {
    match said {
        Some(said) => {
            let mut out = Vec::with_capacity(said.len() + 1);
            out.push(GRANTED);
            out.extend_from_slice(said.as_bytes());
            out
        }
        None => vec![REFUSED],
    }
}

/// Read back what [`answer`] wrote. `None` is every way of not having a value — the refusal byte,
/// a stream that ended before it said anything, or bytes that are not text.
pub fn read_answer(bytes: &[u8]) -> Option<String> {
    match bytes.split_first() {
        Some((&GRANTED, said)) => std::str::from_utf8(said).ok().map(str::to_owned),
        _ => None,
    }
}

/// Everything `read` has to say, or `None` where it stopped part way.
///
/// Both sides read a whole frame before answering, and both are reading a stream whose other end
/// has half-closed, so "to the end" is the only length either of them knows.
pub fn whole(read: &mut impl Read) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    read.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_comes_back_as_it_was_asked() {
        let bytes = question("abc123", "Password for 'https://git@github.com':");
        let (token, asked) = read_question(&bytes).unwrap();
        assert_eq!(token, "abc123");
        assert_eq!(asked, "Password for 'https://git@github.com':");
    }

    /// ssh asks in several lines. The frame is the end of the stream, so the newlines inside the
    /// question are carried rather than read as the end of it.
    #[test]
    fn a_question_of_several_lines_keeps_all_of_them() {
        let asked = "Enter passphrase for key '/home/x/.ssh/id_ed25519':\n(hit return to skip)";
        let (_, back) = read_question(&question("abc123", asked)).unwrap();
        assert_eq!(back, asked);
    }

    /// What arrives when something other than the helper opens the port: there is no token line to
    /// read, and the request is not answered with a guess.
    #[test]
    fn bytes_that_are_not_a_question_are_not_read_as_one() {
        assert!(read_question(b"no newline anywhere").is_none());
    }

    /// The empty password is a password. Nothing else about the wire keeps it apart from the
    /// refusal, so the first byte has to.
    #[test]
    fn nothing_typed_and_nobody_asked_are_two_different_answers() {
        assert_eq!(read_answer(&answer(Some(""))), Some(String::new()));
        assert_eq!(read_answer(&answer(None)), None);
    }

    #[test]
    fn an_answer_comes_back_as_it_was_given() {
        assert_eq!(read_answer(&answer(Some("hunter2"))).unwrap(), "hunter2");
    }

    /// A connection the app closed without writing — the window went away mid-question. Nothing is
    /// the right answer, and it is the one this gives rather than an empty password.
    #[test]
    fn a_stream_that_said_nothing_is_not_an_empty_password() {
        assert_eq!(read_answer(b""), None);
    }
}
