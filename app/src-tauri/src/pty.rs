//! The pseudo-terminal a pane of the talk window is filled with.
//!
//! What runs in a pane draws full-screen: an agent's interface is a TUI that owns the rectangle and
//! repaints it, so there is no stream of lines to split into a transcript and no place to put a
//! chat box beneath one. A pane is therefore a real terminal rather than a rendering of one
//! (`AMB-D-747`), and this module is the host half of it — opening a PTY, carrying its bytes both
//! ways, telling it how large the pane on screen is, and closing it.
//!
//! **Bytes travel whole, and nothing here reads them.** What comes off a PTY is a byte stream with
//! escape sequences in it, and a read ends wherever the kernel happened to fill the buffer — through
//! the middle of a sequence, or of a multi-byte character. Only the emulator drawing the pane can
//! put those back together, so a chunk is carried to the webview base64-encoded and handed over
//! exactly as it arrived. Decoding here would corrupt the split ones and buy nothing.
//!
//! The one exception is the four bytes a terminal is asked its cursor position with, and only on
//! Windows, where they were not written by the program at all — see `CursorQuery` below.
//!
//! **The session's name goes in as an environment variable**, and the whole of its value is that it
//! is inherited: an agent that runs `amenbo` from inside the terminal is several processes deep by
//! then, and what names its session has to survive that distance rather than be guessed at from the
//! outside.
//!
//! **What is started, and with what around it, is [`crate::launch`]'s.** Which shell each operating
//! system has to be asked for, and what a terminal owes the program in it, live there — this module
//! is only the terminal itself.

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use portable_pty::{native_pty_system, MasterPty, PtySize};
use tauri::{Emitter, Manager};

use crate::dto::PtyChunkDto;
use crate::error::CmdError;
use crate::launch;

/// The event each chunk of a terminal's output arrives on. The payload is a `PtyChunkDto`, and it
/// goes to the one window drawing that session rather than to every window open: the other one has
/// no pane for it, and would be woken thousands of times for something it cannot draw. Which window
/// that is is the session's own ([`Pane::target`]), because the pane moves — the board draws the
/// terminal while the app is one window, the talk window once it has been split out.
const OUTPUT_EVENT: &str = "pty://output";

/// The event a terminal's end arrives on, once, when the program in it exits. The payload is the
/// session's id as a string. Nothing follows it — the session is gone from the registry by the time
/// it is emitted, so a write or a resize aimed at it is refused rather than silently dropped.
const CLOSED_EVENT: &str = "pty://closed";

/// The variable a session's id is carried in, into the terminal and everything started inside it.
/// A process that writes to the store while this is set can say which session it wrote from, which
/// is the one thing no amount of watching from outside can establish.
///
/// The name is core's ([`amenbo_core::session::SESSION_VAR`]) rather than one of ours: the surface
/// layer's verbs read it back out of the environment to decide whether they are inside a pane at all
/// (`AMB-D-749`), so a name spelled twice is a name that can drift into a vocabulary that refuses
/// everywhere.
const SESSION_ENV: &str = amenbo_core::session::SESSION_VAR;

/// How much of a terminal's output is carried in one chunk. Large enough that a full repaint of a
/// TUI crosses in a handful of events rather than hundreds, small enough that the first line of a
/// slow command does not wait for a buffer to fill.
const CHUNK: usize = 8 * 1024;

/// How much of a terminal's output is kept for a pane that adopts the session later.
///
/// A terminal's scrollback lives in the emulator, and the emulator goes with the webview it was
/// drawn in. Without a tail kept here, a session that changed windows would come up blank and stay
/// blank until the program inside it next wrote something — at a shell sitting on its prompt, that
/// is never. Several screens of a wide terminal fit in this; past it the oldest bytes are dropped
/// rather than the buffer growing without bound under a program that never stops writing.
const RECENT: usize = 256 * 1024;

/// The terminal being asked where its cursor is, and the answer.
///
/// ConPTY asks this of the terminal as it starts, and **holds the program it was given until an
/// answer comes back** — with none, a `cmd /c echo` never reaches its own first line and the pane
/// stays empty forever (`AMB-T-3565`). Nothing on Unix asks it, so nothing there answers.
///
/// The answer's content does not matter and one is not being reported: conhost is synchronising with
/// a terminal that has not drawn anything yet, and the top-left corner is where that terminal's
/// cursor is.
const CURSOR_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_ANSWER: &[u8] = b"\x1b[1;1R";

/// The terminals this process has open, by session id. Managed state, one for the whole app: a
/// terminal outlives any single command that touches it, and outlives the window it is drawn in.
#[derive(Default)]
pub struct Terminals(Mutex<HashMap<String, Terminal>>);

/// Where a session's output is going, and what it has said lately.
///
/// Both are held apart from [`Terminal`] because the thread draining the terminal reaches for them
/// on every chunk, and the registry's lock is the one thing that thread must not take that often:
/// `pty_write` is a key press, and a key press waiting behind a `cat` of a large file is the pane
/// going numb under the user's hands.
struct Pane {
    /// The label of the window drawing this session. It moves when the pane does, and the chunks
    /// follow it — which is what lets a terminal change windows without being restarted.
    target: Mutex<String>,
    /// The tail of what the terminal has written, for whatever pane draws it next ([`RECENT`]).
    recent: Mutex<VecDeque<u8>>,
}

impl Pane {
    fn new(target: &str) -> Self {
        Self {
            target: Mutex::new(target.to_owned()),
            recent: Mutex::new(VecDeque::new()),
        }
    }

    /// The window the chunks are going to right now.
    fn target(&self) -> String {
        self.target.lock().expect("pane target lock").clone()
    }

    /// Send what follows to this window instead.
    fn point_at(&self, label: &str) {
        *self.target.lock().expect("pane target lock") = label.to_owned();
    }

    /// Add a chunk to the tail, dropping the oldest bytes once it is over the cap, and answer where
    /// it is to be drawn.
    ///
    /// Keeping and routing are one step because [`Pane::adopt`] is the other half of it. A pane
    /// arriving between the two would be handed the chunk in what it is given to draw *and* sent it
    /// as an event, and would draw it twice; taking the same two locks in the same order in both
    /// places is what leaves the chunk on exactly one side of the handover.
    fn keep(&self, bytes: &[u8]) -> String {
        let mut recent = self.recent.lock().expect("pane recent lock");
        recent.extend(bytes);
        let over = recent.len().saturating_sub(RECENT);
        recent.drain(..over);
        self.target()
    }

    /// Send what follows to this window, and answer with the tail as it stood at that moment.
    ///
    /// After an overflow the tail begins wherever the cap fell, which can be part-way through an
    /// escape sequence — so a pane adopting a long-running session can open with a few characters
    /// of noise at the very top. The alternative is holding every byte a terminal ever wrote.
    fn adopt(&self, label: &str) -> Vec<u8> {
        let recent = self.recent.lock().expect("pane recent lock");
        self.point_at(label);
        recent.iter().copied().collect()
    }
}

/// One open pseudo-terminal — the three handles onto it that outlive the call that opened it.
///
/// The reader is not among them. It belongs to the thread that drains it and is never reached for
/// again, which is what lets that thread run without contending for the registry's lock on every
/// chunk it reads.
pub struct Terminal {
    /// The master side, kept for one purpose: telling the terminal how large the pane is.
    master: Box<dyn MasterPty + Send>,
    /// The keystrokes side. Writing to the master is what a key press is.
    writer: Box<dyn Write + Send>,
    /// Which window is drawing this session, and what it has written lately. Shared with the thread
    /// draining it, and the one part of a terminal a pane may move.
    pane: Arc<Pane>,
}

/// A session id: sixteen bytes of the operating system's randomness, in hex.
///
/// Random rather than counted because it leaves this process. A counter starts again at one every
/// launch, so a write carrying `session=1` could have come from this run or from the one before it,
/// and the pairing it exists to make would be wrong exactly when the app was restarted.
fn new_session() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("failed to draw OS randomness");
    bytes.iter().fold(String::with_capacity(32), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Turn a failure of the terminal itself into the refusal the webview is given. There is nothing
/// for a reader to do about most of them, so what they say is what the operating system said —
/// whichever of the two shapes it arrives in, the pty layer's or the file descriptor's.
fn failed(e: impl std::fmt::Display) -> CmdError {
    let reason = e.to_string();
    CmdError::coded(
        "pty_failed",
        format!("The terminal could not be started: {reason}"),
        serde_json::json!({ "reason": reason }),
    )
}

/// The refusal for a session id that names no open terminal — closed while the pane still had it,
/// or never opened at all.
fn gone(session: &str) -> CmdError {
    CmdError::coded(
        "pty_gone",
        "That terminal is no longer open.",
        serde_json::json!({ "session": session }),
    )
}

/// Open a terminal, start the user's login shell in it, and return the id of the session it is.
///
/// `cwd` is the folder the shell starts in; with none given it starts in the user's home. `cols`
/// and `rows` are the pane's size in characters, which the emulator on the far side measures from
/// the space it has.
///
/// What is started is the user's own shell, reached for the way [`crate::launch`] reaches for it on
/// this operating system. Which agent to start inside it is settled on top of this rather than here.
#[tauri::command]
pub fn pty_open(
    app: tauri::AppHandle,
    window: tauri::Window,
    terminals: tauri::State<'_, Terminals>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<String, CmdError> {
    let session = new_session();
    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = native_pty_system().openpty(size).map_err(failed)?;

    let mut cmd = launch::command(cwd, None);
    cmd.env(SESSION_ENV, &session);

    let mut child = pair.slave.spawn_command(cmd).map_err(failed)?;
    // Let go of the slave now the child holds its own. While this process keeps it open the master
    // never reaches end-of-file, so the drain below would sit there for good after the program
    // exited and the pane would never be told it had closed.
    drop(pair.slave);

    let reader = pair.master.try_clone_reader().map_err(failed)?;
    let writer = pair.master.take_writer().map_err(failed)?;
    // The chunks go to whichever window asked for the terminal. Nothing here decides which that is:
    // the pane that called is the pane that draws, and if the user later moves it to the other
    // window, `pty_attach` moves this along with it.
    let pane = Arc::new(Pane::new(window.label()));

    terminals.0.lock().expect("terminals lock").insert(
        session.clone(),
        Terminal {
            master: pair.master,
            writer,
            pane: Arc::clone(&pane),
        },
    );

    let id = session.clone();
    std::thread::spawn(move || {
        drain(&app, &id, &pane, reader);
        // Reap the program before the pane is told, so nothing is left behind for the length of a
        // round trip to the webview. Its exit status says nothing a person needs: what a terminal
        // ends with is what is on the screen, which the pane already has.
        let _ = child.wait();
        app.state::<Terminals>()
            .0
            .lock()
            .expect("terminals lock")
            .remove(&id);
        let _ = app.emit_to(pane.target().as_str(), CLOSED_EVENT, &id);
    });

    Ok(session)
}

/// Read one terminal to its end, sending each chunk on to the pane drawing it.
///
/// A read that fails is an end like any other here. The master reports the far side closing as
/// end-of-file already, so what is left is the fd itself failing — and there is no reading on from
/// that, only the same tidying up.
fn drain(app: &tauri::AppHandle, session: &str, pane: &Pane, mut reader: Box<dyn Read + Send>) {
    let mut buf = vec![0u8; CHUNK];
    let mut cursor = CursorQuery::new();
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let (bytes, asked) = cursor.take(&buf[..n]);
        answer_cursor(app, session, asked);
        if bytes.is_empty() {
            continue;
        }
        let target = pane.keep(&bytes);
        let chunk = PtyChunkDto {
            session: session.to_string(),
            base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        };
        if app.emit_to(target.as_str(), OUTPUT_EVENT, chunk).is_err() {
            return;
        }
    }
}

/// Tell the terminal where its cursor is, as many times as it asked.
///
/// The writer lives in the registry rather than with the thread reading, so this reaches for it
/// there. Contending for that lock costs nothing: on the operating system that asks, it asks once,
/// as the terminal starts.
fn answer_cursor(app: &tauri::AppHandle, session: &str, times: usize) {
    if times == 0 {
        return;
    }
    let terminals = app.state::<Terminals>();
    let mut open = terminals.0.lock().expect("terminals lock");
    let Some(terminal) = open.get_mut(session) else {
        return;
    };
    for _ in 0..times {
        let _ = terminal.writer.write_all(CURSOR_ANSWER);
    }
    let _ = terminal.writer.flush();
}

/// Takes the terminal's own cursor queries out of a terminal's output, so they can be answered here.
///
/// Answering is Windows' alone, and so is the taking out: on Unix a `ESC[6n` in the output came from
/// the program itself, which wants the real position and gets it from the emulator. On Windows it
/// came from conhost, which is talking to the terminal rather than through it — leaving it in the
/// stream would have the emulator answer as well, and the second answer would arrive at the shell as
/// something the user appears to have typed.
///
/// A read ends wherever the kernel filled the buffer, which can be part-way through the four bytes.
/// What could still become a query is held back until the next read rather than passed on, because
/// passing on half of one and answering the other half is how a query gets both answered and drawn.
struct CursorQuery {
    /// Whether this operating system's terminals are asked at all.
    asked: bool,
    /// The tail of the last chunk, when it was a prefix of a query and nothing more.
    held: Vec<u8>,
}

impl CursorQuery {
    fn new() -> Self {
        Self {
            asked: cfg!(windows),
            held: Vec::new(),
        }
    }

    /// Split a chunk into what the pane should draw and how many queries were taken out of it.
    fn take<'a>(&mut self, chunk: &'a [u8]) -> (Cow<'a, [u8]>, usize) {
        if !self.asked {
            return (Cow::Borrowed(chunk), 0);
        }
        let mut buf = std::mem::take(&mut self.held);
        buf.extend_from_slice(chunk);

        let mut out = Vec::with_capacity(buf.len());
        let mut asked = 0;
        let mut i = 0;
        while i < buf.len() {
            let rest = &buf[i..];
            if rest.starts_with(CURSOR_QUERY) {
                asked += 1;
                i += CURSOR_QUERY.len();
                continue;
            }
            if CURSOR_QUERY.starts_with(rest) {
                self.held.extend_from_slice(rest);
                break;
            }
            out.push(buf[i]);
            i += 1;
        }
        (Cow::Owned(out), asked)
    }
}

/// The sessions this process has open, in no particular order.
///
/// A pane asks this on the way up, to find out whether the terminal it is there to draw is already
/// running — which it is every time the pane has moved rather than been made: split out into its own
/// window, folded back into the board, or rebuilt in place because the interface around it was
/// (`app/src/shell/TerminalFace.tsx`). The registry is the only thing that knows, because it is the
/// only part of a terminal that outlives the window: a webview that went away took its emulator with
/// it and could tell nothing to whatever draws next.
#[tauri::command]
pub fn pty_sessions(terminals: tauri::State<'_, Terminals>) -> Vec<String> {
    terminals.0.lock().expect("terminals lock").keys().cloned().collect()
}

/// Draw an already-open terminal in the pane that is asking, and hand back what it has said lately.
///
/// This is what a pane calls in place of [`pty_open`] when it is taking over a session that is
/// already running: the same terminal shown in the other window after the user split the app in two
/// or folded it back, and the same terminal after a language change rebuilt the interface around it.
/// Nothing about the terminal moves — the program inside it is never told that the window it is
/// drawn in changed, and never stops running for it.
///
/// What comes back is base64, the way a chunk is, and for the same reason: these are bytes rather
/// than text. See [`Pane::adopt`] for what the oldest of them can look like.
#[tauri::command]
pub fn pty_attach(
    window: tauri::Window,
    terminals: tauri::State<'_, Terminals>,
    session: String,
) -> Result<String, CmdError> {
    let open = terminals.0.lock().expect("terminals lock");
    let terminal = open.get(&session).ok_or_else(|| gone(&session))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(terminal.pane.adopt(window.label())))
}

/// Send what was typed into the pane to the terminal. `data` is the text the emulator produced for
/// the key press, escape sequences and all, and its bytes go through untouched.
#[tauri::command]
pub fn pty_write(
    terminals: tauri::State<'_, Terminals>,
    session: String,
    data: String,
) -> Result<(), CmdError> {
    let mut open = terminals.0.lock().expect("terminals lock");
    let terminal = open.get_mut(&session).ok_or_else(|| gone(&session))?;
    terminal
        .writer
        .write_all(data.as_bytes())
        .and_then(|()| terminal.writer.flush())
        .map_err(failed)
}

/// Tell the terminal how large the pane is now, in characters.
///
/// This is what a program inside it reads when it asks the terminal its size, and what it is woken
/// by when that changes — so a TUI reflows to the new width because of this call, not because the
/// pane redrew.
#[tauri::command]
pub fn pty_resize(
    terminals: tauri::State<'_, Terminals>,
    session: String,
    cols: u16,
    rows: u16,
) -> Result<(), CmdError> {
    let open = terminals.0.lock().expect("terminals lock");
    let terminal = open.get(&session).ok_or_else(|| gone(&session))?;
    terminal
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    use portable_pty::CommandBuilder;

    /// The tail a pane adopting a session is given never outgrows its cap, however much the
    /// program in the terminal writes — a `yes` left running is the case, and a buffer that grew
    /// with it would be this process's memory going with it.
    #[test]
    fn what_is_kept_for_the_next_pane_stops_at_the_cap() {
        let pane = Pane::new("main");
        for _ in 0..8 {
            pane.keep(&vec![b'x'; RECENT / 2]);
        }
        assert_eq!(pane.adopt("main").len(), RECENT);
    }

    /// And what it keeps is the *end* of the output, not the start: what a pane has to draw is
    /// where the terminal is now, and the prompt it is sitting on is the last thing written.
    #[test]
    fn what_is_kept_is_the_end_of_the_output() {
        let pane = Pane::new("main");
        pane.keep(&vec![b'o'; RECENT]);
        pane.keep(b"$ ");
        let replay = pane.adopt("main");
        assert_eq!(replay.len(), RECENT);
        assert_eq!(&replay[replay.len() - 2..], b"$ ");
    }

    /// The window a session's chunks go to is the pane's to move, which is the whole of how a
    /// terminal changes windows without being restarted.
    #[test]
    fn adopting_a_session_sends_what_follows_to_the_new_window() {
        let pane = Pane::new(crate::windows::BOARD);
        assert_eq!(pane.keep(b"before"), crate::windows::BOARD);
        assert_eq!(pane.adopt(crate::windows::TALK), b"before");
        assert_eq!(pane.keep(b"after"), crate::windows::TALK);
    }

    /// A filter that answers, whichever operating system the test is running on. What Windows does
    /// is what is being asserted, and it has to be assertable from the machine the code is written
    /// on — the alternative is a rule that is only ever exercised where nobody can watch it.
    fn asking() -> CursorQuery {
        CursorQuery {
            asked: true,
            held: Vec::new(),
        }
    }

    /// The query is answered and does not reach the pane. Both halves matter: unanswered, the
    /// program in the terminal never starts; passed on, the emulator answers it too and the second
    /// answer lands in the shell as typing.
    #[test]
    fn the_terminals_own_question_is_answered_and_not_drawn() {
        let (bytes, asked) = asking().take(b"hi\x1b[6nthere");
        assert_eq!(asked, 1);
        assert_eq!(&*bytes, b"hithere");
    }

    /// A read ends wherever the kernel filled the buffer, and four bytes are as splittable as any
    /// others. Half a query held back and rejoined is one query; half passed on would be both
    /// unanswered and drawn on the screen as garbage.
    #[test]
    fn a_question_split_across_two_reads_is_still_one_question() {
        let mut cursor = asking();

        let (first, asked) = cursor.take(b"hi\x1b[");
        assert_eq!(asked, 0);
        assert_eq!(&*first, b"hi", "the start of a question was drawn");

        let (second, asked) = cursor.take(b"6nthere");
        assert_eq!(asked, 1);
        assert_eq!(&*second, b"there");
    }

    /// What was held back only looked like a question. It has to come out whole on the next read,
    /// in front of what followed it — an escape sequence that lost its escape is drawn as text.
    #[test]
    fn what_was_held_back_comes_out_whole_when_it_turns_out_to_be_something_else() {
        let mut cursor = asking();

        let (first, _) = cursor.take(b"\x1b[");
        assert!(first.is_empty());

        let (second, asked) = cursor.take(b"1;1H");
        assert_eq!(asked, 0);
        assert_eq!(&*second, b"\x1b[1;1H");
    }

    /// Where nothing asks, nothing is taken out. A program on Unix that asks where the cursor is
    /// wants the real position, which only the emulator drawing the pane knows — this must reach it.
    #[cfg(unix)]
    #[test]
    fn a_programs_own_question_reaches_the_pane_untouched() {
        let (bytes, asked) = CursorQuery::new().take(b"hi\x1b[6nthere");
        assert_eq!(asked, 0);
        assert_eq!(&*bytes, b"hi\x1b[6nthere");
    }

    /// Read a terminal to its end and hand back everything that came out of it, as bytes.
    fn read_to_end(mut reader: Box<dyn Read + Send>) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0u8; 1024];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        out
    }

    /// The whole point of naming the session through the environment: the process started in the
    /// terminal is not the one that will write to the store. An agent runs `amenbo`, which is a
    /// grandchild at best, so a name that reached only the shell would name nothing that writes.
    ///
    /// Two levels deep is what is asserted, because that is where inheritance would break if the
    /// terminal were started with a cleared environment — the shell would still have what was set
    /// on it directly, and only its own children would come up empty.
    #[cfg(unix)]
    #[test]
    fn the_session_name_reaches_a_grandchild() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open a pty");

        let mut cmd = CommandBuilder::new("/bin/sh");
        // No newline in the output: the line discipline turns one into a carriage return and a line
        // feed, and the brackets are what makes the assertion exact rather than a substring of some
        // longer word the shell might print.
        cmd.args(["-c", r#"/bin/sh -c 'printf "[%s]" "$AMENBO_SESSION"'"#]);
        cmd.env(SESSION_ENV, "a-session");

        let mut child = pair.slave.spawn_command(cmd).expect("start the shell");
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().expect("read the pty");
        let out = String::from_utf8_lossy(&read_to_end(reader)).into_owned();
        let _ = child.wait();

        assert!(
            out.contains("[a-session]"),
            "the grandchild did not inherit the session name: {out:?}"
        );
    }

    /// A session id has to be unique across restarts of the app, which is the case a counter gets
    /// wrong. Two draws being different does not prove randomness, but a fixed or a counted value
    /// would fail here, and those are what this is guarding against.
    #[test]
    fn every_session_gets_its_own_name() {
        let a = new_session();
        assert_eq!(a.len(), 32, "a session id is sixteen bytes in hex: {a:?}");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()), "not hex: {a:?}");
        assert_ne!(a, new_session());
    }
}
