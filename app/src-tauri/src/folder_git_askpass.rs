//! The window's end of the wire git's askpass helper calls back on.
//!
//! git and ssh, finding no terminal, run the program named in `GIT_ASKPASS` / `SSH_ASKPASS` and
//! read the answer off its standard output. Amenbo ships that program beside the app
//! (`crates/amenbo-askpass`), and what it does is hand the question to this module. Everything
//! about the shape of the wire — the two variables, the framing, why it is loopback TCP — is
//! written once, in [`amenbo_askpass`], because both ends have to agree on it.
//!
//! **Why the two variables `AMB-D-906` put there are gone from
//! [`crate::folder_git_write`]**: they were placed so that a call needing a password would say so
//! instead of hanging on a terminal nobody is watching. `ssh -o BatchMode=yes` had to go — under it
//! ssh asks nothing, this helper included, so the question would never leave the machine. What
//! `GIT_TERMINAL_PROMPT=0` costs is only a sentence, and the measurement of both is the table in
//! [`crate::folder_git_write`]'s module doc-comment (`AMB-D-913`). The closed stdin next door stays:
//! it is there for `ssh-keygen` and `ssh-add`, which read stdin for reasons that have nothing to do
//! with authentication.
//!
//! **The door is opened once and stands for the life of the app.** A listener raised per call would
//! be a port opened and closed on every `git fetch`, and the helper git starts has no way of being
//! told a number that changed after git was started.
//!
//! **Where the helper is not beside the app, nothing is set at all.** That is a build tree, where
//! the app is run out of `target/` and the bundle's neighbours do not exist. git then behaves as it
//! did before any of this: it looks for a terminal, finds a closed stdin, and says so.

use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::OnceLock;
use std::time::Duration;

/// How long a question is allowed to sit unread before the connection is dropped.
///
/// The helper is on the other side of it with its own longer limit, so what this really bounds is a
/// caller that connected and then said nothing — a port scan, or something that found the port and
/// does not speak this wire. Nothing legitimate takes a second: the question is already written
/// when the connection is made.
const PATIENCE: Duration = Duration::from_secs(10);

/// The listener, and the word that says a caller is one of this app's own children.
struct Door {
    at: SocketAddr,
    token: String,
}

/// What a git child is told, so that what it starts can reach back here — empty where this build
/// has no helper to point at.
///
/// `SSH_ASKPASS_REQUIRE=force` is the one of the three that is not obvious. Without it ssh uses an
/// askpass only when it has no terminal *and* a display is set, which on a mac is neither reliable
/// nor true; `force` says to use it whatever else is around. It is what VS Code and Zed both set,
/// measured in `AMB-T-4968`.
pub fn env() -> Vec<(&'static str, String)> {
    // The helper first, so that a build tree — where there is none — never opens a port for a
    // caller that could not reach it anyway.
    let Some(helper) = beside() else { return Vec::new() };
    let Some(door) = door() else { return Vec::new() };
    let helper = helper.to_string_lossy().into_owned();
    vec![
        ("GIT_ASKPASS", helper.clone()),
        ("SSH_ASKPASS", helper),
        ("SSH_ASKPASS_REQUIRE", "force".to_owned()),
        (amenbo_askpass::ADDR_VAR, door.at.to_string()),
        (amenbo_askpass::TOKEN_VAR, door.token.clone()),
    ]
}

/// The helper this build ships, where the running binary is standing beside it. `None` out of a
/// build tree — the same answer, for the same reason, as the bundled CLI next to it
/// ([`amenbo_core::config::Paths::askpass_file_name`]).
fn beside() -> Option<std::path::PathBuf> {
    let at = std::env::current_exe().ok()?.parent()?.join(amenbo_core::config::Paths::askpass_file_name());
    at.is_file().then_some(at)
}

/// The door, opened the first time it is asked for. `None` where the port could not be taken, which
/// is a machine with no loopback to bind — the caller then sets nothing and git goes on as it did
/// before.
fn door() -> Option<&'static Door> {
    static DOOR: OnceLock<Option<Door>> = OnceLock::new();
    DOOR.get_or_init(open).as_ref()
}

/// Bind the port, draw the token, and leave a thread standing on it.
fn open() -> Option<Door> {
    let listening = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).ok()?;
    let at = listening.local_addr().ok()?;
    // Sixteen bytes of the operating system's own randomness, drawn once per run of the app — the
    // same shape and the same source as a pane's session id (`crate::pty`).
    let token = crate::pty::random_hex(16);
    let mine = token.clone();
    std::thread::spawn(move || {
        for talk in listening.incoming().flatten() {
            let mine = mine.clone();
            // One thread per question. They overlap in the ordinary case — a `git push` that needs
            // both a username and a password asks twice, and ssh asks once per key — and a caller
            // that says nothing must not hold up the one behind it.
            std::thread::spawn(move || serve(talk, &mine, answered));
        }
    });
    Some(Door { at, token })
}

/// Read one question, answer it, and be done with the connection.
///
/// `answer` is taken rather than called by name so that a test can stand where the dialog will:
/// everything between the socket and the person is here, and the only part that is not is what a
/// person says.
fn serve(mut talk: TcpStream, token: &str, answer: impl Fn(&str) -> Option<String>) {
    let _ = talk.set_read_timeout(Some(PATIENCE));
    let Some(bytes) = amenbo_askpass::whole(&mut talk) else { return };
    let Some((said, asked)) = amenbo_askpass::read_question(&bytes) else { return };
    // Not this app's child: something else on the machine found the port. It is answered with the
    // refusal rather than with silence, so that our own helper — which can only be here by mistake
    // — ends now instead of waiting out its own limit.
    let said = (said == token).then(|| answer(&asked)).flatten();
    let _ = talk.write_all(&amenbo_askpass::answer(said.as_deref()));
}

/// What the person says — once there is a dialog to say it in (`AMB-T-4970`).
///
/// Until then there is nobody to ask, and the honest answer is that there is no answer: the helper
/// ends non-zero, git falls back to the terminal it has not got, and the failure the reader is
/// shown is git's own — word for word the one they get today (`AMB-D-906`, 3-4). What this task
/// leaves behind is the road the question travels, which is the half that cannot be built from
/// inside a dialog.
fn answered(_asked: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// Drive one question through a real socket, standing where the dialog will stand. This is the
    /// whole of the window's side: what arrives, what is checked, and what goes back.
    fn ask(token: &str, sent: &[u8], answer: impl Fn(&str) -> Option<String> + Send) -> Vec<u8> {
        let listening = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let at = listening.local_addr().unwrap();
        std::thread::scope(|threads| {
            threads.spawn(|| {
                let (talk, _) = listening.accept().unwrap();
                serve(talk, token, answer);
            });
            let mut talk = TcpStream::connect(at).unwrap();
            talk.write_all(sent).unwrap();
            talk.shutdown(std::net::Shutdown::Write).unwrap();
            let mut back = Vec::new();
            talk.read_to_end(&mut back).unwrap();
            back
        })
    }

    #[test]
    fn a_question_from_this_app_s_own_helper_is_answered() {
        let sent = amenbo_askpass::question("cafef00d", "Password for 'https://github.com':");
        let back = ask("cafef00d", &sent, |asked| Some(format!("answer to {asked}")));
        assert_eq!(
            amenbo_askpass::read_answer(&back).unwrap(),
            "answer to Password for 'https://github.com':"
        );
    }

    /// The port is loopback, and anything running as this person can reach it. The token is what
    /// stands between that and a password dialog wearing Amenbo's face, so a caller without it is
    /// never put in front of the person at all.
    #[test]
    fn a_question_carrying_the_wrong_token_never_reaches_the_person() {
        let asked = std::sync::atomic::AtomicBool::new(false);
        let sent = amenbo_askpass::question("not-the-token", "Password:");
        let back = ask("cafef00d", &sent, |_| {
            asked.store(true, std::sync::atomic::Ordering::SeqCst);
            Some("hunter2".into())
        });
        assert_eq!(amenbo_askpass::read_answer(&back), None, "and it is told so");
        assert!(!asked.load(std::sync::atomic::Ordering::SeqCst), "nothing was put to the person");
    }

    /// Bytes that are not a question at all. Nothing is written back — there is no reader on the
    /// other side that would know what to do with an answer — and the connection simply ends.
    #[test]
    fn bytes_that_are_not_a_question_are_dropped() {
        assert!(ask("cafef00d", b"GET / HTTP/1.1", |_| Some("hunter2".into())).is_empty());
    }

    /// The dialog was closed without an answer. That is not an empty password, and what goes back
    /// has to keep the two apart.
    #[test]
    fn a_question_the_person_walked_away_from_is_not_an_empty_password() {
        let sent = amenbo_askpass::question("cafef00d", "Password:");
        let back = ask("cafef00d", &sent, |_| None);
        assert_eq!(amenbo_askpass::read_answer(&back), None);
    }

    /// What this build points git at, and what it does when there is nothing to point at. Out of a
    /// build tree there is no helper beside the binary, so nothing is set and git is left exactly
    /// as it was.
    #[test]
    fn nothing_is_set_where_there_is_no_helper_to_point_at() {
        let set = env();
        match beside() {
            None => assert!(set.is_empty(), "a build tree ships no helper"),
            Some(helper) => {
                let named: Vec<_> = set.iter().map(|(name, _)| *name).collect();
                assert!(named.contains(&"GIT_ASKPASS"));
                assert!(named.contains(&"SSH_ASKPASS"));
                assert!(named.contains(&amenbo_askpass::ADDR_VAR));
                assert!(named.contains(&amenbo_askpass::TOKEN_VAR));
                let forced = set.iter().find(|(name, _)| *name == "SSH_ASKPASS_REQUIRE");
                assert_eq!(forced.map(|(_, how)| how.as_str()), Some("force"));
                assert!(helper.is_file());
            }
        }
    }
}
