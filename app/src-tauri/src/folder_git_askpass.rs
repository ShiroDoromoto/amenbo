//! The window's end of the wire git's askpass helper calls back on, and the question it puts to the
//! person.
//!
//! git and ssh, finding no terminal, run the program named in `GIT_ASKPASS` / `SSH_ASKPASS` and
//! read the answer off its standard output. Amenbo ships that program beside the app
//! (`crates/amenbo-askpass`), and what it does is hand the question to this module. Everything
//! about the shape of the wire — the two variables, the framing, why it is loopback TCP — is
//! written once, in [`amenbo_askpass`], because both ends have to agree on it.
//!
//! **What the person is shown is git's own sentence, word for word** (`AMB-D-913`). Amenbo does not
//! say it again in its own words: `Username for 'https://github.com':` and `Enter passphrase for key
//! '…':` are git's and ssh's, in whatever language they wrote them. What this side adds is the frame
//! around it — which call is waiting, and the offer to keep the answer — and that frame is the only
//! part with a translation ([`crate::dto::GitAskDto`], `app/src/files/GitAsk.tsx`).
//!
//! **What the question is read for is never its words.** Which field git is asking for, whether to
//! hide what is typed, whether there is a credential complete enough to keep — all three are read
//! off the shape of the URL git quoted (`About`), because the words around it go through git's own
//! gettext and ssh's are not git's at all.
//!
//! **Amenbo keeps nothing of what is typed** (`AMB-D-913`). Where the person asks for it to be kept
//! it goes to `git credential approve`, and from there into whatever credential helper this machine
//! has (`approve`); where they do not, it is used for that one call and dropped. The reason is that
//! the agent in the pane runs git too, off the same helper — a value Amenbo held as well would be a
//! second answer to the same question, and revoking one of them would leave the other standing.
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
//! **The word a caller carries is drawn per call, not per run of the app**
//! ([`during`](crate::folder_git_askpass::during)). It has two jobs and they want the same thing: it
//! says the caller is one of this app's own children, and it says *which* call — so the question can
//! name what is waiting on it. A word good for the life of the app would still be good long after
//! the last git that held it ended.
//!
//! **Where the helper is not beside the app, nothing is set at all.** That is a build tree, where
//! the app is run out of `target/` and the bundle's neighbours do not exist. git then behaves as it
//! did before any of this: it looks for a terminal, finds a closed stdin, and says so.

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

use tauri::Emitter;

use crate::dto::GitAskDto;

/// How long a question is allowed to sit unread before the connection is dropped.
///
/// The helper is on the other side of it with its own longer limit, so what this really bounds is a
/// caller that connected and then said nothing — a port scan, or something that found the port and
/// does not speak this wire. Nothing legitimate takes a second: the question is already written
/// when the connection is made.
const PATIENCE: Duration = Duration::from_secs(10);

/// How long the person is given to answer before the question is taken to be unanswered.
///
/// It is the helper's own limit (`amenbo_askpass`'s `WAIT`), because the two are waiting on the same
/// person: a window answering after the helper had given up would be a password typed into a call
/// that is already over. What the length is measured in is somebody finding a password, not a
/// machine answering.
const ANSWER_GRACE: Duration = Duration::from_secs(10 * 60);

/// Told to the windows when git is waiting on an answer: the question, and what is waiting on it.
///
/// It goes to every window rather than to one, because the face that runs git is in whichever window
/// currently holds it — the board before the terminal is split out, and the talk window after
/// (`AMB-D-753`). Only one window has that face at a time (`app/src/shell/AppShell.tsx`), so only
/// one question is ever drawn.
pub const ASKED_EVENT: &str = "git-askpass://asked";

/// The listener. The word that says a caller is one of this app's own children is [`during`]'s.
struct Door {
    at: SocketAddr,
}

/// One git that is running, as the door knows it for as long as it runs.
#[derive(Clone)]
struct Asking {
    /// What a person would have typed to start it — `git push`. It is the one line of the question
    /// that is this side's rather than git's: the question itself never says what is waiting on it.
    doing: String,
    /// The folder git was run in, which is where [`approve`] has to be run for the repository's own
    /// configuration — and so its own credential helper — to be the one that answers.
    dir: PathBuf,
}

/// What the person said, on its way back to the git that is waiting.
struct Said {
    /// What was typed, or `None` where the dialog was closed without an answer. The two are not the
    /// same: git reads an empty password as a password.
    said: Option<String>,
    /// Whether they asked for it to be kept. Honoured only where the question named a credential
    /// complete enough to keep ([`About::savable`]).
    save: bool,
}

/// Every git running right now, by the word its children carry.
fn live() -> &'static Mutex<HashMap<String, Asking>> {
    static LIVE: OnceLock<Mutex<HashMap<String, Asking>>> = OnceLock::new();
    LIVE.get_or_init(Mutex::default)
}

/// Every question standing on a window right now, by the number it was sent out under.
fn standing() -> &'static Mutex<HashMap<u64, mpsc::Sender<Said>>> {
    static STANDING: OnceLock<Mutex<HashMap<u64, mpsc::Sender<Said>>>> = OnceLock::new();
    STANDING.get_or_init(Mutex::default)
}

/// The app, so that a door thread holding a question can reach the windows.
///
/// `None` until setup has run, and in a test — where there are no windows, and so no question to
/// put. A door that finds none answers with nothing, which is the same answer it gave before there
/// was a dialog at all.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Hand this module the app, once there is one (`crate::run`).
pub fn init(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

/// A git that is running, and the word its children carry to say so. Dropping it ends the word.
///
/// The word is drawn whether or not this build has a helper to point at: what it stands for is a git
/// that is running, which is true either way, and [`During::env`] is the one place that has to know
/// there is nobody to tell.
pub struct During(String);

/// Say that a git is about to run in `dir`, doing `doing` — `push`, `commit`, the word a person
/// would have typed after `git`.
pub fn during(dir: &Path, doing: &str) -> During {
    // Sixteen bytes of the operating system's own randomness — the same shape and the same source as
    // a pane's session id (`crate::pty`).
    let word = crate::pty::random_hex(16);
    let asking = Asking {
        doing: format!("git {doing}").trim_end().to_owned(),
        dir: dir.to_path_buf(),
    };
    if let Ok(mut live) = live().lock() {
        live.insert(word.clone(), asking);
    }
    During(word)
}

impl During {
    /// What this git's children are told, so that what they start can reach back here — empty where
    /// this build has no helper to point at.
    ///
    /// `SSH_ASKPASS_REQUIRE=force` is the one of the three that is not obvious. Without it ssh uses
    /// an askpass only when it has no terminal *and* a display is set, which on a mac is neither
    /// reliable nor true; `force` says to use it whatever else is around. It is what VS Code and Zed
    /// both set, measured in `AMB-T-4968`.
    pub fn env(&self) -> Vec<(&'static str, String)> {
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
            (amenbo_askpass::TOKEN_VAR, self.0.clone()),
        ]
    }
}

impl Drop for During {
    fn drop(&mut self) {
        if let Ok(mut live) = live().lock() {
            live.remove(&self.0);
        }
    }
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

/// Bind the port and leave a thread standing on it.
fn open() -> Option<Door> {
    let listening = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).ok()?;
    let at = listening.local_addr().ok()?;
    std::thread::spawn(move || {
        for talk in listening.incoming().flatten() {
            // One thread per question. They overlap in the ordinary case — a `git push` that needs
            // both a username and a password asks twice, and ssh asks once per key — and a caller
            // that says nothing must not hold up the one behind it.
            std::thread::spawn(move || serve(talk, running, put));
        }
    });
    Some(Door { at })
}

/// Read one question, answer it, and be done with the connection.
///
/// `who` and `ask` are taken rather than called by name so that a test can stand where the window
/// will: everything between the socket and the person is here, and the only part that is not is what
/// a person says.
fn serve(
    mut talk: TcpStream,
    who: impl Fn(&str) -> Option<Asking>,
    ask: impl Fn(&Asking, &str) -> Option<String>,
) {
    let _ = talk.set_read_timeout(Some(PATIENCE));
    let Some(bytes) = amenbo_askpass::whole(&mut talk) else { return };
    let Some((word, asked)) = amenbo_askpass::read_question(&bytes) else { return };
    // A word naming no running git: something else on the machine found the port, or a helper
    // outlived the call it was started for. It is answered with the refusal rather than with
    // silence, so that our own helper ends now instead of waiting out its own limit.
    let said = who(&word).and_then(|asking| ask(&asking, &asked));
    let _ = talk.write_all(&amenbo_askpass::answer(said.as_deref()));
}

/// The git this word was drawn for, where it is still running.
fn running(word: &str) -> Option<Asking> {
    live().lock().ok()?.get(word).cloned()
}

/// The number the next question goes out under. It names one question on one window, and nothing
/// outside this run of the app reads it.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// Put the question to the window and hand back what the person said.
///
/// `None` is every way of having no answer: no window to ask in, a window that never came back, or a
/// dialog closed without one. git is told the same thing by the helper's non-zero exit whichever it
/// was, and then fails in its own words — which is what a reader gets today (`AMB-D-906`, 3-4).
fn put(asking: &Asking, asked: &str) -> Option<String> {
    let app = APP.get()?;
    let about = About::of(asked);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (say, heard) = mpsc::channel();
    standing().lock().ok()?.insert(id, say);
    let sent = app.emit(
        ASKED_EVENT,
        GitAskDto {
            id,
            doing: asking.doing.clone(),
            asked: asked.to_owned(),
            secret: about.secret,
            savable: about.savable(),
        },
    );
    // The window the question is drawn in comes forward by itself once it has it
    // (`app/src/files/GitAsk.tsx`) — a password box behind what the reader is looking at is a call
    // that has stopped for no reason they can see.
    let heard = match sent {
        Ok(()) => heard.recv_timeout(ANSWER_GRACE).ok(),
        Err(_) => None,
    };
    if let Ok(mut standing) = standing().lock() {
        standing.remove(&id);
    }
    let Said { said, save } = heard?;
    if let (true, Some(at), Some(said)) = (save, about.at.as_ref(), said.as_deref()) {
        approve(&asking.dir, at, said);
    }
    said
}

/// The window has answered — or closed the dialog, which is a `said` of `null`.
///
/// A number naming no standing question is ignored rather than refused: this is a door the webview
/// can press at any time, and a question whose git gave up while the person was typing is gone from
/// the list by the time the answer arrives.
#[tauri::command]
pub fn folder_git_askpass_said(id: u64, said: Option<String>, save: bool) {
    let sender = standing().lock().ok().and_then(|mut standing| standing.remove(&id));
    if let Some(sender) = sender {
        let _ = sender.send(Said { said, save });
    }
}

// ── what the question turns out to be about ──────────────────────────────────────────────────

/// The credential git named in a question, taken apart into the fields `git credential` writes.
///
/// It is read out of the question because the question is the only thing that crosses the wire, and
/// it is read positionally rather than by the words around it: `Username` and `Password` go through
/// git's own gettext, and ssh's questions are not git's at all.
struct At {
    protocol: String,
    /// The host as git wrote it, port and all — which is also how `git credential` spells it.
    host: String,
    /// The path inside the host, where git named one (`credential.useHttpPath`). Empty otherwise.
    path: String,
    /// The user the credential is for. Empty while git is still asking who it is for.
    username: String,
}

/// What one question turns out to be about.
struct About {
    /// The credential it names, where it names one. `None` for ssh's questions, which are about a
    /// key file rather than about a URL.
    at: Option<At>,
    /// Whether what is typed is hidden while it is typed.
    secret: bool,
}

impl At {
    /// Read a URL as git wrote it into a question: `https://user@host:port/path`.
    ///
    /// `None` where it is not a URL at all — the quoted part of `Enter passphrase for key
    /// '/home/x/.ssh/id_ed25519':` is a path, and a path is not a credential.
    fn of(spelled: &str) -> Option<At> {
        let (protocol, rest) = spelled.split_once("://")?;
        let named = |b: &u8| b.is_ascii_alphanumeric() || b"+-.".contains(b);
        if protocol.is_empty() || !protocol.bytes().all(|b| named(&b)) {
            return None;
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        // The last `@` and not the first: a username can hold one, and a host cannot.
        let (username, host) = authority.rsplit_once('@').unwrap_or(("", authority));
        (!host.is_empty()).then(|| At {
            protocol: protocol.to_owned(),
            host: host.to_owned(),
            path: path.to_owned(),
            username: username.to_owned(),
        })
    }
}

impl About {
    fn of(asked: &str) -> About {
        let at = quoted(asked).and_then(At::of);
        // git is still asking who the credential is for, so what is typed is a name and goes in
        // plain. Everything else is a secret — a password, or a key's passphrase.
        let naming = at.as_ref().is_some_and(|at| at.username.is_empty());
        // Except a question ending in a question mark, which is ssh asking something answered in
        // words: whether to go on connecting to a host it has not seen before. OpenSSH is not
        // translated, and neither it nor git ends a prompt for a secret that way.
        let in_words = asked.trim_end().ends_with('?');
        About { at, secret: !naming && !in_words }
    }

    /// Whether "keep it" can be honoured. It can where the question is the second of git's two — the
    /// one already naming the user — because only then is there a whole credential to keep. What was
    /// typed into the first is carried into the second by git itself.
    fn savable(&self) -> bool {
        self.at.as_ref().is_some_and(|at| !at.username.is_empty())
    }
}

/// What a question has between its first quote and its last. git and ssh both write the thing they
/// are asking about that way, and nothing else in either sentence is quoted.
fn quoted(asked: &str) -> Option<&str> {
    let opened = asked.find('\'')?;
    let closed = asked.rfind('\'')?;
    (closed > opened).then(|| &asked[opened + 1..closed])
}

/// Hand a credential to whatever helper this machine keeps them in — `git credential approve`.
///
/// **Amenbo is not one of the places it can land.** What this does is put it where the git the
/// person runs themselves, and the agent in the pane beside them, already look for it (`AMB-D-913`)
/// — so there is one place holding it and one place to revoke it.
///
/// Anything that goes wrong is dropped. The call it was typed for is going ahead either way, and a
/// helper that would not take it is not a reason to fail a push that worked.
fn approve(dir: &Path, at: &At, password: &str) {
    let mut asked = String::new();
    for (name, value) in [
        ("protocol", at.protocol.as_str()),
        ("host", at.host.as_str()),
        ("path", at.path.as_str()),
        ("username", at.username.as_str()),
        ("password", password),
    ] {
        if value.is_empty() {
            continue;
        }
        // git's own protocol is one field to a line, so a value holding a line break is one it
        // cannot carry. Nothing is sent rather than half of it.
        if value.contains(['\n', '\r', '\0']) {
            return;
        }
        asked.push_str(name);
        asked.push('=');
        asked.push_str(value);
        asked.push('\n');
    }
    let Some(mut git) = amenbo_core::sys::git() else { return };
    let started = git
        .arg("-C")
        .arg(dir)
        .args(["credential", "approve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = started else { return };
    if let Some(mut into) = child.stdin.take() {
        // The blank line ends the request, and letting the pipe go after it is what shows the helper
        // the end of the stream.
        let _ = into.write_all(asked.as_bytes()).and_then(|()| into.write_all(b"\n"));
    }
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A git that is running, as the door would have found it.
    fn asking() -> Asking {
        Asking { doing: "git push".into(), dir: PathBuf::from("/") }
    }

    /// Drive one question through a real socket, standing where the window will stand. This is the
    /// whole of the window's side: what arrives, what is checked, and what goes back.
    fn ask(word: &str, sent: &[u8], answer: impl Fn(&Asking, &str) -> Option<String> + Send) -> Vec<u8> {
        let listening = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let at = listening.local_addr().unwrap();
        std::thread::scope(|threads| {
            threads.spawn(|| {
                let (talk, _) = listening.accept().unwrap();
                serve(talk, |said| (said == word).then(asking), answer);
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
        let back = ask("cafef00d", &sent, |_, asked| Some(format!("answer to {asked}")));
        assert_eq!(
            amenbo_askpass::read_answer(&back).unwrap(),
            "answer to Password for 'https://github.com':"
        );
    }

    /// The port is loopback, and anything running as this person can reach it. The word is what
    /// stands between that and a password dialog wearing Amenbo's face, so a caller without one is
    /// never put in front of the person at all.
    #[test]
    fn a_question_carrying_the_wrong_word_never_reaches_the_person() {
        let asked = std::sync::atomic::AtomicBool::new(false);
        let sent = amenbo_askpass::question("not-the-word", "Password:");
        let back = ask("cafef00d", &sent, |_, _| {
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
        assert!(ask("cafef00d", b"GET / HTTP/1.1", |_, _| Some("hunter2".into())).is_empty());
    }

    /// The dialog was closed without an answer. That is not an empty password, and what goes back
    /// has to keep the two apart.
    #[test]
    fn a_question_the_person_walked_away_from_is_not_an_empty_password() {
        let sent = amenbo_askpass::question("cafef00d", "Password:");
        let back = ask("cafef00d", &sent, |_, _| None);
        assert_eq!(amenbo_askpass::read_answer(&back), None);
    }

    /// The word names the one git it was drawn for, and stops naming anything the moment that git is
    /// over. A helper that outlives its call — one still holding the port when git has already given
    /// up — is answered like any other stranger.
    #[test]
    fn a_word_names_the_git_it_was_drawn_for_and_then_nothing() {
        let during = during(Path::new("/tmp"), "push");
        let word = during.0.clone();
        assert_eq!(running(&word).map(|a| a.doing), Some("git push".to_owned()));
        drop(during);
        assert!(running(&word).is_none(), "the git it named is over");
    }

    /// What this build points git at, and what it does when there is nothing to point at. Out of a
    /// build tree there is no helper beside the binary, so nothing is set and git is left exactly
    /// as it was.
    #[test]
    fn nothing_is_set_where_there_is_no_helper_to_point_at() {
        let during = during(Path::new("/tmp"), "fetch");
        let set = during.env();
        match beside() {
            None => assert!(set.is_empty(), "a build tree ships no helper"),
            Some(helper) => {
                let named: Vec<_> = set.iter().map(|(name, _)| *name).collect();
                assert!(named.contains(&"GIT_ASKPASS"));
                assert!(named.contains(&"SSH_ASKPASS"));
                assert!(named.contains(&amenbo_askpass::ADDR_VAR));
                let word = set.iter().find(|(name, _)| *name == amenbo_askpass::TOKEN_VAR);
                assert_eq!(word.map(|(_, said)| said.as_str()), Some(during.0.as_str()));
                let forced = set.iter().find(|(name, _)| *name == "SSH_ASKPASS_REQUIRE");
                assert_eq!(forced.map(|(_, how)| how.as_str()), Some("force"));
                assert!(helper.is_file());
            }
        }
    }

    /// git's first question: it has a URL and no user in it yet. What is typed is a name, so it is
    /// not hidden, and there is nothing whole enough to keep.
    #[test]
    fn the_question_asking_who_the_credential_is_for_is_typed_in_the_open() {
        let about = About::of("Username for 'https://github.com': ");
        assert!(!about.secret);
        assert!(!about.savable());
        assert_eq!(about.at.as_ref().map(|at| at.host.as_str()), Some("github.com"));
    }

    /// git's second question. The user is in the URL by now — git put it there — so this one is
    /// hidden as it is typed and is the one that can be kept.
    #[test]
    fn the_question_asking_for_the_password_is_the_one_that_can_be_kept() {
        let about = About::of("Password for 'https://alice@github.com': ");
        assert!(about.secret);
        assert!(about.savable());
        let at = about.at.unwrap();
        assert_eq!((at.protocol.as_str(), at.host.as_str(), at.username.as_str()), ("https", "github.com", "alice"));
        assert!(at.path.is_empty(), "git named no path");
    }

    /// The same question in another language. Nothing is read off the words, so the reading does not
    /// move with them.
    #[test]
    fn a_question_git_wrote_in_another_language_is_read_the_same_way() {
        let about = About::of("'https://alice@github.com' のパスワード: ");
        assert!(about.secret);
        assert!(about.savable());
    }

    /// A port travels with the host, because that is how `git credential` spells one too. A path
    /// travels on its own line, and only where git named one.
    #[test]
    fn a_port_stays_with_the_host_and_a_path_is_its_own_field() {
        let at = At::of("https://alice@example.com:8443/team/repo.git").unwrap();
        assert_eq!(at.host, "example.com:8443");
        assert_eq!(at.path, "team/repo.git");
    }

    /// ssh's question is about a key file, not about a URL. There is no credential to keep, and what
    /// is typed is still a secret.
    #[test]
    fn a_key_s_passphrase_is_hidden_and_is_not_a_credential_to_keep() {
        let about = About::of("Enter passphrase for key '/home/x/.ssh/id_ed25519': ");
        assert!(about.secret);
        assert!(!about.savable());
        assert!(about.at.is_none(), "a path is not a URL");
    }

    /// ssh asking whether to go on connecting to a host it has not seen. The answer is a word the
    /// person has to read back to themselves, so hiding it would be hiding what they typed from the
    /// only reader there is.
    #[test]
    fn a_question_answered_in_words_is_not_hidden() {
        let asked = "The authenticity of host 'github.com (203.0.113.4)' can't be established.\n\
                     ED25519 key fingerprint is SHA256:+DiY3wvvV6TuJJhbpZisF.\n\
                     Are you sure you want to continue connecting (yes/no/[fingerprint])? ";
        let about = About::of(asked);
        assert!(!about.secret);
        assert!(!about.savable());
    }
}
