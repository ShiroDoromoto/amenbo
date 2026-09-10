//! The pseudo-terminal a pane of the talk window is filled with.
//!
//! What runs in a pane draws full-screen: an agent's interface is a TUI that owns the rectangle and
//! repaints it, so there is no stream of lines to split into a transcript and no place to put a
//! chat box beneath one. A pane is therefore a real terminal rather than a rendering of one
//! (`AMB-D-747`), and this module is the host half of it — opening a PTY, carrying its bytes both
//! ways, telling it how large the pane on screen is, and closing it.
//!
//! **Bytes travel whole.** What comes off a PTY is a byte stream with escape sequences in it, and a
//! read ends wherever the kernel happened to fill the buffer — through the middle of a sequence, or
//! of a multi-byte character. Only the emulator drawing the pane can put those back together, so a
//! chunk is carried to the webview base64-encoded and handed over exactly as it arrived. Decoding
//! here would corrupt the split ones and buy nothing.
//!
//! Two things are read out of the stream on the way past, and neither changes what is carried. One
//! is the four bytes a terminal is asked its cursor position with, and only on Windows, where they
//! were not written by the program at all — see `CursorQuery` below. The other is which private
//! modes the program has put the terminal into, which outlive the bytes that set them — see
//! `Modes`.
//!
//! **The session's name goes in as an environment variable**, and the whole of its value is that it
//! is inherited: an agent that runs `amenbo` from inside the terminal is several processes deep by
//! then, and what names its session has to survive that distance rather than be guessed at from the
//! outside.
//!
//! **A pane is also handed somewhere to be spoken to.** Beside the session's name goes a throwaway
//! directory, and what the agent says about its session — whose turn it is, a name for the pane — is
//! left there as one file per statement (`AMB-D-749`). This module watches that directory and carries
//! each statement on to the pane, then takes the directory away with the terminal: nothing said about a
//! session outlives the session. A run that ends without closing its terminals — a quit, a crash — takes
//! nothing away, so [`crate::pty::sweep`] clears what an earlier run left as this one comes up.
//!
//! **What is started, and with what around it, is [`crate::launch`]'s.** Which shell each operating
//! system has to be asked for, and what a terminal owes the program in it, live there — this module
//! is only the terminal itself.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use portable_pty::{native_pty_system, ChildKiller, MasterPty, PtySize};
use tauri::{Emitter, Manager};

use amenbo_core::harness::Handle;

use crate::dto::{PtyChunkDto, PtyReplayDto, PtySessionDto, SessionSaidDto};
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
/// It is what tells a process several levels deep in a pane which pane it is in — the one thing no
/// amount of watching from outside can establish, folder and clock having been measured and separated
/// nothing (`AMB-T-3549`).
///
/// The name is core's ([`amenbo_core::session::SESSION_VAR`]) rather than one of ours: the surface
/// layer's verbs read it back out of the environment to decide whether they are inside a pane at all
/// (`AMB-D-749`), so a name spelled twice is a name that can drift into a vocabulary that refuses
/// everywhere.
const SESSION_ENV: &str = amenbo_core::session::SESSION_VAR;

/// The variable naming the throwaway directory this pane's statements are left in, set beside
/// [`SESSION_ENV`] on every terminal opened. Core's name, for the reason [`SESSION_ENV`] gives.
///
/// **A pane with no directory is a pane the surface layer refuses in**, loudly, which is the right
/// failure: an agent told "ok" for a statement dropped where nothing is watching would believe it had
/// spoken while the person's screen never changed.
const DIR_ENV: &str = amenbo_core::session::DIR_VAR;

/// The event each statement an agent makes about its session arrives on. The payload is a
/// `SessionSaidDto`, and like the output it goes to the talk window alone.
const SAID_EVENT: &str = "session://said";

/// The event saying the opening instruction was left in the pane's input box unsent. The payload is
/// the session's id as a string, and it is emitted once, at the end of the hand-over.
///
/// **Only [`crate::handover::Handover::LeftForTheReader`] is reported.** `Sent` has nothing to say —
/// the sentence went in and the pane is as it should be — and `Gone` has nobody to say it to: the
/// terminal ended, so there is no input box holding anything, no AI running in the pane to be
/// missing its premise, and no keypress a person could make that would change either. What is left
/// is the one ending a reader can act on, and what they act with is one Enter.
const UNSENT_EVENT: &str = "pty://unsent";

/// How often the drop box is looked in. A statement is a person-scale event — an agent says a handful
/// in a session — so this is slow enough to cost nothing and quick enough that a pane's label does not
/// visibly lag what the agent just said.
const LISTEN_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

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

/// The tail of what a terminal has written, cut into the runs it was written at.
///
/// **The size is kept with the bytes because it cannot be worked out from them.** Where a line
/// ended was decided when it was written, and an emulator told a different width folds it somewhere
/// else — which for a program that draws by moving the cursor about leaves parts of old frames
/// standing (`AMB-T-4514`). Reading the whole tail at the size the terminal happens to be at now
/// fixes the newest of it and leaves everything written before the last change folded wrong
/// (`AMB-T-4516`); read run by run, each at its own size, all of it is folded where it was written.
///
/// The cap is on the tail rather than on a run, so a terminal resized a hundred times keeps the
/// same quarter of a megabyte a terminal never resized does.
struct Recent {
    /// The runs, oldest first. A run ends where the size changed.
    runs: VecDeque<Run>,
    /// Bytes across every run, so the cap is a count rather than a walk.
    len: usize,
    /// The size in force, which whatever is written next belongs to.
    at: Size,
    /// The private modes the program has put the terminal into ([`Modes`]). They are kept beside the
    /// runs rather than in them because they have to outlive the cap: the bytes that set a mode are
    /// written once, at the start, and are the first thing to fall away.
    modes: Modes,
}

/// As much of the tail as was written at one size.
struct Run {
    at: Size,
    bytes: VecDeque<u8>,
}

/// A terminal's size in characters — width, then height.
type Size = (u16, u16);

/// The byte every escape sequence begins with.
const ESC: u8 = 0x1b;

/// How long a mode sequence is let run before it is given up on. `ESC [ ? 1000;1002;1003;1006 h` is
/// longer than a program sends and far shorter than a chunk, so an `ESC` in the middle of a file
/// being printed costs a handful of bytes rather than a buffer that grows with the file.
const MODE_MAX: usize = 32;

/// Which private modes the terminal has been put into, and what the latest value of each is.
///
/// A private mode is turned on with `ESC [ ? <n> h` and off with `ESC [ ? <n> l` (DECSET / DECRST),
/// and a program asks for the ones it wants as it starts and then never again. Claude Code asks for
/// bracketed paste (`?2004`) and focus reporting (`?1004`) in its first hundred bytes. Those bytes
/// fall out of the tail as soon as the session has written [`RECENT`], and a pane built after that —
/// the project being switched, the terminal opened in its own window — reads a tail with no mention
/// of them and comes up with bracketed paste off. A multi-line paste then arrives as `L1\rL2\r…`,
/// which sends every line but the last (`AMB-T-4566`).
///
/// So the modes are read out of the stream as it goes past and held whole, and [`Pane::adopt`] puts
/// them in front of the tail. What the tail itself carries is read again after them, which is why
/// holding only the latest value is enough: a mode set inside the tail lands last either way.
struct Modes {
    /// The latest value of each mode, by number. Ordered, so the same session hands over the same
    /// bytes every time it is adopted.
    latest: BTreeMap<u16, bool>,
    /// A sequence that ran off the end of a chunk, waiting for the rest of it. A read ends wherever
    /// the kernel filled the buffer, so the `ESC` and the `h` can arrive in different chunks.
    partial: Vec<u8>,
}

impl Modes {
    fn new() -> Self {
        Self {
            latest: BTreeMap::new(),
            partial: Vec::new(),
        }
    }

    /// Take in a chunk, keeping the latest value of every mode it sets or resets.
    fn take_in(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if self.partial.is_empty() {
                if b == ESC {
                    self.partial.push(b);
                }
                continue;
            }
            match self.partial.len() {
                // `ESC [` opens every CSI sequence, and only the private ones carry a `?`:
                // `ESC [ > …` and `ESC [ = …` are asking something else entirely.
                1 if b == b'[' => self.partial.push(b),
                2 if b == b'?' => self.partial.push(b),
                n if (3..MODE_MAX).contains(&n) && (b.is_ascii_digit() || b == b';') => {
                    self.partial.push(b)
                }
                n if n >= 3 && (b == b'h' || b == b'l') => {
                    self.settle(b == b'h');
                    self.partial.clear();
                }
                // Anything else was not a mode sequence — `ESC [ ? 2004 $ p` asks what a mode is
                // rather than setting it, and a stray `ESC` is just a byte in a file.
                _ => {
                    self.partial.clear();
                    // The byte that ended this one may be the start of the next.
                    if b == ESC {
                        self.partial.push(b);
                    }
                }
            }
        }
    }

    /// Keep the modes named in the sequence just read. One sequence may name several, `;` apart.
    fn settle(&mut self, on: bool) {
        for param in self.partial[3..].split(|&b| b == b';') {
            if let Some(mode) = std::str::from_utf8(param).ok().and_then(|p| p.parse().ok()) {
                self.latest.insert(mode, on);
            }
        }
    }

    /// The sequences that put a terminal back into these modes, for a pane to read before the tail.
    fn bytes(&self) -> Vec<u8> {
        self.latest
            .iter()
            .flat_map(|(mode, on)| {
                format!("\x1b[?{mode}{}", if *on { 'h' } else { 'l' }).into_bytes()
            })
            .collect()
    }
}

impl Recent {
    fn new(at: Size) -> Self {
        Self {
            runs: VecDeque::new(),
            len: 0,
            at,
            modes: Modes::new(),
        }
    }

    /// Take in a chunk at the size in force, dropping the oldest bytes once the tail is over the cap.
    fn push(&mut self, bytes: &[u8]) {
        self.modes.take_in(bytes);
        match self.runs.back_mut() {
            Some(run) if run.at == self.at => run.bytes.extend(bytes),
            _ => self.runs.push_back(Run {
                at: self.at,
                bytes: bytes.iter().copied().collect(),
            }),
        }
        self.len += bytes.len();
        let mut over = self.len.saturating_sub(RECENT);
        // The oldest run goes first and whole runs fall away with it, which is what keeps a run's
        // size attached to bytes that are still there to be read at it.
        while over > 0 {
            let Some(front) = self.runs.front_mut() else { break };
            let dropped = over.min(front.bytes.len());
            front.bytes.drain(..dropped);
            self.len -= dropped;
            over -= dropped;
            if front.bytes.is_empty() {
                self.runs.pop_front();
            }
        }
    }

    /// Every byte kept, in the order it was written — for a reader with no size to honour.
    fn bytes(&self) -> Vec<u8> {
        self.runs
            .iter()
            .flat_map(|run| run.bytes.iter().copied())
            .collect()
    }
}

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

impl Terminals {
    /// How many sessions are open right now.
    ///
    /// Asked by the way out of the app (`crate::quit`), where the only thing worth knowing is
    /// whether there is anything to lose — a count is that, and it can be had without copying the
    /// registry the way [`pty_sessions`] does.
    pub fn open(&self) -> usize {
        self.0.lock().expect("terminals lock").len()
    }
}

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
    /// The tail of what the terminal has written, for whatever pane draws it next ([`Recent`],
    /// capped at [`RECENT`]).
    recent: Mutex<Recent>,
    /// Whether the agent in this pane has read Amenbo's canon — whether it ran `amenbo agent` here
    /// (`AMB-D-805`).
    ///
    /// **This is the state, and the drop box is not.** The fact arrives as one statement passing
    /// through, and the file it arrived in is swept on age by whichever Amenbo comes up next
    /// ([`sweep`]) — so a pane that had gone a day without a word would read as never briefed, and be
    /// handed the sentence a second time. What the box carries is only ever read forwards; once it is
    /// read it is held here, where nothing else can take it away.
    ///
    /// **Nothing this process does raises it.** A newline sent because the screen looked right is a
    /// guess, and a guess is what this exists to stop standing in for the fact.
    briefed: AtomicBool,
    /// The opening sentence sitting in this pane's input box, unsent — kept for the one press that
    /// can send it (`AMB-D-805`).
    ///
    /// It is put here when the hand-over runs out of patience and taken when it goes, so it goes
    /// once however many times a person presses Enter. `None` is a pane with nothing owed: one whose
    /// sentence rode in on the command line, one the hand-over got through to, or one already sent.
    unsent: Mutex<Option<String>>,
    /// Whether the opening sentence is still being handed over — true from the moment that thread
    /// starts until it is done with the pane.
    ///
    /// **Two things must never be writing into one input box.** The rename waits this out rather
    /// than racing it: both of them paste into a screen that has stood still, so both could pick the
    /// same still screen, and what a person would then press Enter on is one line made of two
    /// (`AMB-D-872`).
    opening: AtomicBool,
    /// The name this pane's provider is still to be told, and whether a thread is carrying one
    /// ([`Renaming`]).
    renaming: Mutex<Renaming>,
}

/// A pane's rename, as the thread carrying it and the name it is to carry next.
///
/// **The newest name wins and there is one thread.** A rename waits for the pane to stand still,
/// which can be the length of an agent's answer — long enough for a person to rename the pane again,
/// and two threads pasting into one box would put both names in it. So a second naming replaces what
/// the thread is to carry rather than starting one of its own, and the thread reads this again every
/// time it finishes one (`AMB-D-872`).
#[derive(Default)]
struct Renaming {
    /// The name not yet carried into the pane, or `None` when the provider has been told the latest
    /// one.
    owed: Option<String>,
    /// Whether a thread is on it. It stays true across the wait, which is what a second naming
    /// checks to know it has nothing to start.
    running: bool,
}

impl Pane {
    fn new(target: &str, at: Size) -> Self {
        Self {
            target: Mutex::new(target.to_owned()),
            recent: Mutex::new(Recent::new(at)),
            briefed: AtomicBool::new(false),
            unsent: Mutex::new(None),
            opening: AtomicBool::new(false),
            renaming: Mutex::new(Renaming::default()),
        }
    }

    /// Take in one statement on its way to the window.
    ///
    /// One of them is the pane's own business as well as the person's: the fact that `amenbo agent`
    /// ran here. Every other verb passes straight through — a name is the frame being named, and the
    /// frame is the window's.
    fn take_in(&self, said: &amenbo_core::session::Said) {
        use amenbo_core::session::Statement;
        match &said.statement {
            Statement::Briefed => self.briefed.store(true, Ordering::Relaxed),
            Statement::Name(_) => {}
        }
    }

    /// Whether the agent in this pane has the canon.
    ///
    /// Ordering is relaxed because there is nothing beside it to be ordered against: one bit, written
    /// once by the thread watching the drop box and read by the hand-over — which is looking every
    /// half-second and owes nothing to being told on the exact pass it happened.
    fn briefed(&self) -> bool {
        self.briefed.load(Ordering::Relaxed)
    }

    /// The sentence has been left in this pane's input box for a person to send.
    fn leave(&self, instruction: String) {
        *self.unsent.lock().expect("pane unsent lock") = Some(instruction);
    }

    /// Take the sentence that was left, if one still is. **Taken rather than read**, so the pane has
    /// nothing owed the moment it goes out and a second press finds nothing to send.
    fn take_unsent(&self) -> Option<String> {
        self.unsent.lock().expect("pane unsent lock").take()
    }

    /// Whether the opening sentence is still on its way into this pane — either a thread is handing
    /// it over, or it is sitting in the input box waiting for a person's Enter.
    fn opening(&self) -> bool {
        self.opening.load(Ordering::Relaxed)
            || self.unsent.lock().expect("pane unsent lock").is_some()
    }

    /// Say whether the opening sentence is in flight. Set before the thread starts and cleared when
    /// it is done, so the rename never sees a gap that is not one.
    fn handing_over(&self, yes: bool) {
        self.opening.store(yes, Ordering::Relaxed);
    }

    /// This pane's provider is to be told it is called `line`. Answers whether a thread has to be
    /// started — false where one is already carrying names into this pane and will pick this up.
    fn rename_to(&self, line: String) -> bool {
        let mut renaming = self.renaming.lock().expect("pane renaming lock");
        renaming.owed = Some(line);
        if renaming.running {
            return false;
        }
        renaming.running = true;
        true
    }

    /// The next name to carry, or `None` — which also puts the thread down, under the one lock, so a
    /// naming arriving in that moment either replaces the name or starts a thread and never neither.
    fn next_rename(&self) -> Option<String> {
        let mut renaming = self.renaming.lock().expect("pane renaming lock");
        let next = renaming.owed.take();
        if next.is_none() {
            renaming.running = false;
        }
        next
    }

    /// No more names will be carried into this pane — the terminal went, or the pane never came free
    /// of its opening sentence.
    fn rename_over(&self) {
        let mut renaming = self.renaming.lock().expect("pane renaming lock");
        renaming.owed = None;
        renaming.running = false;
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
        self.recent.lock().expect("pane recent lock").push(bytes);
        self.target()
    }

    /// Say what size the terminal is now, so what it writes from here is kept apart from what it
    /// wrote before.
    ///
    /// Said only where the terminal took the size ([`pty_resize`]): a resize that failed left the
    /// program writing to the width it already had, and a run opened for a size nothing is being
    /// written at would hand the next pane a fold that never happened.
    fn resized(&self, at: Size) {
        self.recent.lock().expect("pane recent lock").at = at;
    }

    /// The tail as it stands — what the pane has drawn lately, for a reader that is not a pane.
    ///
    /// The handover ([`crate::handover`]) is that reader: it is looking for the words it pasted, and
    /// — where a program answers a paste without drawing it — for the tail moving at all. It takes
    /// the same copy a pane adopting the session is given, rather than a window onto the buffer, so
    /// nothing holds this lock while it searches.
    fn screen(&self) -> Vec<u8> {
        self.recent.lock().expect("pane recent lock").bytes()
    }

    /// Send what follows to this window, and answer with the tail as it stood at that moment — in
    /// the runs it was written in, each carrying the size it belongs to ([`Recent`]) — with the
    /// modes the terminal is in ([`Modes`]) in front of all of it.
    ///
    /// **The modes come first because the tail is not enough on its own.** A program asks for
    /// bracketed paste once, as it starts, and a session that has written a quarter of a megabyte
    /// since has nothing left saying so; a pane that read only the tail would come up with it off
    /// (`AMB-T-4566`). They are handed over at the size the tail begins at: they belong to no size
    /// of their own, and a run carrying none would be read at whatever the pane happens to measure.
    ///
    /// After an overflow the tail begins wherever the cap fell, which can be part-way through an
    /// escape sequence — so a pane adopting a long-running session can open with a few characters
    /// of noise at the very top. The alternative is holding every byte a terminal ever wrote.
    fn adopt(&self, label: &str) -> Vec<PtyReplayDto> {
        let recent = self.recent.lock().expect("pane recent lock");
        self.point_at(label);
        let encode = |bytes: Vec<u8>| base64::engine::general_purpose::STANDARD.encode(bytes);
        let modes = recent.modes.bytes();
        let mut replay = Vec::with_capacity(recent.runs.len() + 1);
        // A terminal that has written nothing has no modes either, so there is nothing to put in
        // front of and nothing to put there.
        if let Some(first) = recent.runs.front().filter(|_| !modes.is_empty()) {
            replay.push(PtyReplayDto {
                cols: first.at.0,
                rows: first.at.1,
                base64: encode(modes),
            });
        }
        replay.extend(recent.runs.iter().map(|run| PtyReplayDto {
            cols: run.at.0,
            rows: run.at.1,
            base64: encode(run.bytes.iter().copied().collect::<Vec<u8>>()),
        }));
        replay
    }
}

/// One open pseudo-terminal — the three handles onto it that outlive the call that opened it.
///
/// The reader is not among them. It belongs to the thread that drains it and is never reached for
/// again, which is what lets that thread run without contending for the registry's lock on every
/// chunk it reads.
pub struct Terminal {
    /// The folder this terminal was opened in, as the filesystem spells it. It is kept because it is
    /// also the fence [`crate::fileproto`] reads a file inside — so what the person can see in the
    /// pane and what they can be shown the contents of are one answer, settled when the pane opened
    /// and not re-derived per request. A terminal opened without one can be read nothing at all.
    folder: Option<PathBuf>,
    /// The id the agent in it was started as — a catalog row or one of this device's registrations —
    /// or `None` for a plain prompt. Kept for the reason `folder` is: it was settled when the
    /// terminal started, and a pane that adopts this session has no other way to learn what is
    /// running in it ([`crate::dto::PtySessionDto`]).
    agent: Option<String>,
    /// The master side, kept for one purpose: telling the terminal how large the pane is.
    master: Box<dyn MasterPty + Send>,
    /// The keystrokes side. Writing to the master is what a key press is.
    writer: Box<dyn Write + Send>,
    /// The way to end the program, kept apart from the child itself: the child belongs to the thread
    /// draining the terminal, which waits on it and must not be reached for from anywhere else.
    ///
    /// It exists because nothing else can end a terminal. A pane going away never does — that is a
    /// pane moving, and the session outlives it (`AMB-D-753`) — so without this the only way out is
    /// the program deciding to stop, which is exactly what a runaway does not do.
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// Which window is drawing this session, and what it has written lately. Shared with the thread
    /// draining it, and the one part of a terminal a pane may move.
    pane: Arc<Pane>,
    /// When the terminal was started (RFC3339 UTC). It stays in the registry and is never handed to a
    /// pane: the one thing it settles is the order [`pty_sessions`] answers in, and a pane is drawn
    /// and thrown away as the session moves windows, so it could not keep it anyway.
    started_at: String,
}

/// A session id: sixteen bytes of the operating system's randomness, in hex.
///
/// Random rather than counted because it leaves this process. A counter starts again at one every
/// launch, so a write carrying `session=1` could have come from this run or from the one before it,
/// and the pairing it exists to make would be wrong exactly when the app was restarted.
fn new_session() -> String {
    random_hex(16)
}

/// That many bytes of OS randomness, written as lower-case hex — the shape both a session id and the
/// name of a pasted file want, and the only one that is safe to put in a path without asking what is
/// in it.
fn random_hex(bytes: usize) -> String {
    let mut drawn = vec![0u8; bytes];
    getrandom::fill(&mut drawn).expect("failed to draw OS randomness");
    drawn.iter().fold(String::with_capacity(bytes * 2), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Now, in milliseconds since the epoch — the floor a pane's own session is picked above
/// (`crate::agent_sessions`). A clock this cannot read is zero, which takes the newest session in
/// the folder rather than none.
fn since_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(0))
}

/// How long a program started on a handle has to last before the handle is believed.
///
/// A provider handed a session id it cannot use says so and exits, which takes well under a second;
/// one that came up stays up for as long as somebody is talking to it. Five seconds sits past the
/// first and nowhere near the second.
const BELIEVED_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// Watch for the session this pane opened to appear in its provider's own list, and write the
/// handle down on the frame's row (`AMB-D-869`).
///
/// **On a thread, because the pane is not waiting for it.** What is being watched for is a provider
/// writing its first record, which is quick but is not instant, and a pane that waited would be a
/// window holding still while a terminal opened. Nothing on the screen depends on the answer: what
/// it buys is the way back into this pane the next time the app comes up.
fn read_back(
    app: tauri::AppHandle,
    command: &'static str,
    ask: amenbo_core::agent_sessions::Ask,
    frame: String,
    folder: String,
    since: i64,
) {
    std::thread::spawn(move || {
        let face = app.state::<crate::frames::TalkFace>();
        let taken = face.resume_hints();
        let found =
            crate::agent_sessions::appeared(command, &ask, std::path::Path::new(&folder), since, &taken);
        match found {
            Some(handle) => face.resumed_from(&frame, handle),
            // Nothing appeared before the wait ran out. The pane is running either way; what is lost
            // is the way back into it, and next run opens a fresh session there.
            None => log::warn!("no session appeared for frame {frame} in {folder}"),
        }
    });
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

/// How a webview's agent id is started ([`Started`]), or the refusal for an id nothing answers to.
///
/// The lookup is what keeps the pane's command line out of the webview's hands: an id that names
/// neither a catalog row nor one of this device's registrations is turned away here rather than
/// handed to a shell. It is refused under `crate::wake`'s code, because what it names is that rule
/// and not this door — the same id is turned away the same way where a folder's answer is written
/// down.
///
/// **The two kinds take the instruction by different routes, and deliberately so** (`AMB-D-794`):
///
/// | | what the shell is handed | what is owed afterwards |
/// |---|---|---|
/// | a catalog row | the program, with the instruction as its opening prompt | nothing |
/// | a registered row | the line as the reader wrote it | the instruction, handed over in two stages |
///
/// What the instruction is, either way, is [`amenbo_core::agents::pane_instruction`]: the sentence a
/// session in a folder gets, and after it what is said only to a pane.
///
/// A registered line is not taken apart and not rebuilt. Amenbo does not know where in
/// `claude --model opus` an opening instruction would go — before the flags, after them, behind a
/// flag of its own — so it does not guess: the line is started as it stands and the sentence follows
/// it into the pane (`AMB-D-793`).
fn started_as(agent: &str, handle: Option<Handle<'_>>) -> Result<Started, CmdError> {
    let cmd = amenbo_core::config::Paths::command_name();
    let config = amenbo_core::config::Paths::resolve()
        .map(|paths| amenbo_core::config::Config::load(&paths.config_file))
        .unwrap_or_default();
    if let Some(launch) = amenbo_core::wake::started_as(agent) {
        return Ok(opening_line(launch, config.model_for(agent), handle));
    }
    if let Some(own) = config.custom_agent(agent) {
        return Ok(Started {
            line: own.line.clone(),
            hand_over: Some(amenbo_core::agents::pane_instruction(cmd)),
        });
    }
    Err(CmdError::coded(
        "wake_unknown_agent",
        "That is not an agent Amenbo knows how to start.",
        serde_json::json!({ "agent": agent }),
    ))
}

/// How one catalogued agent is started: the program, with the launch instruction handed to it as its
/// opening prompt (`AMB-T-3596`) — so a row out of the catalog is owed nothing afterwards.
///
/// **Every terminal this window opens gets it, and it is never put to the person first.** It is
/// plumbing — the sentence that points an agent at `agent --json` — and a pane that asked before
/// sending it would be asking whether the person wants their AI to know where it is working.
///
/// The instruction names the binary this build is ([`amenbo_core::config::Paths::command_name`]), so
/// a dev-channel window starts agents on the dev channel's own command rather than on the production
/// one the reader may not have installed.
///
/// **There is no card offering the reader a sentence to send instead.** Handing one over was how this
/// worked while the terminal was somebody else's — Amenbo wrote a request, the person carried it to a
/// terminal outside — and once the terminal is here there is nothing to carry: the sentence goes in as
/// the pane opens. What is left of the old shape would be a card asking a person who has just arrived
/// to decide what to ask for, which is the one thing they do not yet know.
///
/// **The model is whichever one was chosen for this agent, and no model at all where none was**
/// (`amenbo_core::config::Config::agent_model`). A line naming none starts the provider on however
/// its own settings have it, which is what a person who has never been asked expects to happen — so
/// the flag is absent rather than passed empty (`amenbo_core::harness::opening`).
///
/// **It is read here rather than carried in from the face.** What model an agent comes up on is a
/// fact about the agent and is kept against it (`AMB-D-865`), so every road that starts one — the
/// press on the empty frame, the offer a folder with several puts up, the row on a frame whose
/// program has ended — reaches the same answer without any of them having to hand it along.
///
/// **A row the reader registered gets none.** Its line is theirs as they wrote it and is never taken
/// apart (`AMB-D-794`), so there is nowhere in it a model flag could be put that would not be Amenbo
/// guessing at somebody else's command line.
fn opening_line(
    launch: &amenbo_core::harness::Launch,
    model: Option<&amenbo_core::config::AgentModel>,
    handle: Option<Handle<'_>>,
) -> Started {
    let cmd = amenbo_core::config::Paths::command_name();
    let named = model.map(|one| one.id.as_str());
    Started {
        line: launch::command_line(
            launch.command,
            &amenbo_core::harness::opening(launch, cmd, named, handle),
        ),
        hand_over: None,
    }
}

/// How a pane is started, and whether anything is still owed to what is running in it.
///
/// The two are separate because the instruction has two routes and only one of them is finished by
/// the time the program starts. A catalogued row takes it as an argument, which is the whole of the
/// hand-over; a launch line Amenbo did not compose has nowhere to put one, so the line is started as
/// it stands and the sentence follows it into the pane (`AMB-D-793`, `AMB-D-794`).
struct Started {
    /// What the pane's shell is asked to run.
    line: String,
    /// The instruction still to be handed over once the pane draws, or `None` where the line already
    /// carries it.
    hand_over: Option<String>,
}

/// How long between one look at the pane and the next, while the instruction is being handed over.
///
/// Slow enough that a program repainting its interface is not raced on every frame, quick enough
/// that the sentence goes in about when the input box appears — a look this long is also what makes
/// the pane's stillness worth something, since the handover pastes only into a screen that has held
/// the same bytes across several of them. What is being waited for is person-scale: a program coming
/// up, and sometimes a person answering a question it asked first.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

/// How many looks the hand-over gets before the sentence is left for the reader. With [`SETTLE`]
/// between them this is a minute — long enough to cover a trust prompt somebody has to walk back to
/// the screen for, short enough that the thread behind it is not a thread for the session's life.
const TRIES: usize = 120;

/// Hand the instruction to whatever is running in this pane, on a thread of its own.
///
/// It is a thread because it is a conversation: the loop writes, then reads what the pane drew, and
/// both of those outlive the call that opened the terminal. What it may do is bounded by [`TRIES`] —
/// and by the terminal, which it asks about on every pass, so a pane closed in the middle of this
/// takes the thread with it.
fn hand_over(app: tauri::AppHandle, session: String, pane: Arc<Pane>, instruction: String) {
    // Said before the thread is spawned, so a rename arriving in the same breath finds the sentence
    // in flight rather than the gap before it started (`Pane::opening`).
    pane.handing_over(true);
    std::thread::spawn(move || {
        let open = |app: &tauri::AppHandle| {
            app.state::<Terminals>().0.lock().expect("terminals lock").contains_key(&session)
        };
        let ended = crate::handover::hand_over(
            &instruction,
            TRIES,
            // A pane being started: ten seconds of a screen that will not stand still buys the paste
            // anyway, because what the movement means here is a program still drawing itself.
            Some(crate::handover::RESTLESS),
            || pane.briefed(),
            || open(&app).then(|| pane.screen()),
            |bytes| {
                let terminals = app.state::<Terminals>();
                let mut open = terminals.0.lock().expect("terminals lock");
                let Some(terminal) = open.get_mut(&session) else { return false };
                terminal.writer.write_all(bytes).and_then(|()| terminal.writer.flush()).is_ok()
            },
            || std::thread::sleep(SETTLE),
        );
        // Said once, at the end. Which of the three happened is the one thing a person reading a pane
        // that behaved oddly cannot work out from the screen, and the log is where all three are kept.
        log::debug!("opening instruction for session {session}: {ended:?}");
        // The one of the three the reader is told about, because it is the one they can finish. A
        // sentence left in the input box is not a failure — it is a finished state needing a keypress
        // — and a screen holding one is indistinguishable from a screen that was handed its sentence,
        // so the row above the pane says which it is (`app/src/talk/nameplate.ts`).
        if ended == crate::handover::Handover::LeftForTheReader {
            // Kept before it is said, so that the pane can answer for the sentence from the moment
            // anybody is told there is one to answer for (`pty_brief`).
            pane.leave(instruction);
            let _ = app.emit_to(pane.target().as_str(), UNSENT_EVENT, &session);
        }
        // Last, and after the sentence has been left: what this releases is the input box, and it is
        // not free while the sentence is still going into it.
        pane.handing_over(false);
    });
}

/// How many looks a rename gets before the name is given up on.
///
/// With [`SETTLE`] between them this is ten minutes, and it is longer than the opening sentence's
/// minute because what it is waiting out is longer: the sentence waits for a program to finish
/// starting, and this waits for an agent to finish answering. A rename nobody is watching for is
/// worth waiting on — and the pane says the name itself the whole time, which is the copy nothing
/// depends on (`AMB-D-872`).
const RENAME_TRIES: usize = 1200;

/// Tell the provider running in this pane what the pane is called, on a thread of its own.
///
/// **It waits, and what it waits for is an input box nobody else is using.** The opening sentence
/// has the box first — a rename pasted on top of one still going in would make a single line out of
/// two — and after that the wait is for the pane to stand still, which is an agent's answer ending.
/// Neither is hurried: what is being carried is a copy of a name the pane already shows.
fn rename_pane(app: tauri::AppHandle, session: String, pane: Arc<Pane>) {
    std::thread::spawn(move || {
        let open = |app: &tauri::AppHandle| {
            app.state::<Terminals>().0.lock().expect("terminals lock").contains_key(&session)
        };
        while let Some(line) = pane.next_rename() {
            // The opening sentence first. A pane that never comes free of it is one this has nothing
            // safe to do to, so the name is given up rather than pasted onto somebody else's line.
            let mut waited = 0;
            while pane.opening() {
                if !open(&app) || waited >= RENAME_TRIES {
                    log::debug!("rename for session {session}: the opening sentence still has the box");
                    pane.rename_over();
                    return;
                }
                waited += 1;
                std::thread::sleep(SETTLE);
            }
            let ended = crate::handover::hand_over(
                &line,
                RENAME_TRIES,
                // Nothing but stillness buys the paste. A pane that is moving here is one an agent is
                // answering in, and that ends by itself (`crate::handover::RESTLESS`).
                None,
                // There is no fact to get off on: what would answer "the provider has this name" is
                // the provider's own list of sessions, which is the thing being written to.
                || false,
                || open(&app).then(|| pane.screen()),
                |bytes| {
                    let terminals = app.state::<Terminals>();
                    let mut open = terminals.0.lock().expect("terminals lock");
                    let Some(terminal) = open.get_mut(&session) else { return false };
                    terminal.writer.write_all(bytes).and_then(|()| terminal.writer.flush()).is_ok()
                },
                || std::thread::sleep(SETTLE),
            );
            log::debug!("rename for session {session}: {ended:?}");
            if ended == crate::handover::Handover::Gone {
                pane.rename_over();
                return;
            }
        }
    });
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

/// Open a terminal, start the user's login shell in it, and answer with the session it is.
///
/// `cwd` is the folder the shell starts in; with none given it starts in the user's home. It is
/// resolved on the filesystem before anything is started, both because a folder that is not there is
/// worth saying so about rather than failing inside `spawn`, and because the resolved form is what
/// [`crate::fileproto`] measures a file against. `cols` and `rows` are the pane's size in characters,
/// which the emulator on the far side measures from the space it has.
///
/// What is started is the user's own shell, reached for the way [`crate::launch`] reaches for it on
/// this operating system. `agent` is the catalogued id of the AI to start inside that shell
/// ([`crate::wake`]); with none given the pane is a bare prompt. **The id is turned into a command
/// here**, out of the catalog — what the webview names is a row, never a command line — and the
/// launch instruction rides in on that command line as the agent's opening prompt
/// ([`opening_line`]).
///
/// `frame` is the place of the arrangement this terminal is being drawn in
/// (`app/src/talk/layout.ts`), and it is here because the way back into what is started is written
/// down against the place rather than against the process: a pane comes back in the next run, and
/// the session in it does not (`AMB-D-869`).
///
/// **A frame that already carries a handle for this provider is opened on it**, and one that does
/// not is opened on a handle issued here, where the provider takes one
/// ([`amenbo_core::harness::issue`]). Neither ever crosses to the window: both are read and written
/// on this side, so a pane's way back is not something a webview could put there.
// Seven of these are what a window holds about a pane, one answer each. Gathered into a shape they
// would be taken apart again on arrival.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn pty_open(
    app: tauri::AppHandle,
    window: tauri::Window,
    terminals: tauri::State<'_, Terminals>,
    frame: Option<String>,
    cwd: Option<String>,
    agent: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<PtySessionDto, CmdError> {
    let session = new_session();
    let started_at = amenbo_core::time::Timestamp::now().to_rfc3339_z();
    let opened_at = since_epoch_ms();
    // Read against a clock that only goes forwards, because what it is asked is how long the program
    // lasted — a wall clock moved between the two readings would answer with the move.
    let opened = std::time::Instant::now();
    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = native_pty_system().openpty(size).map_err(failed)?;

    let folder = cwd
        .map(|dir| std::fs::canonicalize(dir).map_err(failed))
        .transpose()?;

    // The way back into what this frame was running. A frame that came back with a handle for this
    // provider is opened on it; one that has none is opened on a handle issued here and written
    // down, so the pane has a way back the next time the app comes up (`AMB-D-869`).
    let face = app.state::<crate::frames::TalkFace>();
    let back = frame
        .as_deref()
        .zip(agent.as_deref())
        .and_then(|(frame, agent)| face.comes_back_on(frame, agent));
    let launch = agent.as_deref().and_then(amenbo_core::wake::started_as);
    let issued = match back {
        Some(_) => None,
        None => launch.and_then(amenbo_core::harness::issue),
    };
    let handle = back
        .as_deref()
        .map(Handle::Back)
        .or_else(|| issued.as_deref().map(Handle::New));
    let started = agent.as_deref().map(|id| started_as(id, handle)).transpose()?;
    // Written down before the program is started, so a quit that comes between the two still leaves
    // the pane a way back — the session is made under this handle whether or not anybody is watching.
    if let (Some(frame), Some(issued)) = (frame.as_deref(), issued.as_deref()) {
        face.resumed_from(frame, issued.to_string());
    }
    // The frame to take the way back off again, should the program end in moments. Only where the
    // handle rode in on the launch line: `codex` is pointed at a directory instead, which is derived
    // from the frame afresh every time and is not a row a refusal could be traced to (`AMB-D-869`).
    let on_the_line = frame
        .clone()
        .filter(|_| launch.is_some_and(|launch| launch.resume.is_some()));
    // Kept against the session, so a pane that adopts this terminal later can say what is running in
    // it. It is the id as it was asked for — a catalog row, or one of this device's registrations —
    // and which of the two it is stays the catalog's answer rather than being decided here.
    let agent_id = agent;
    let run = started.as_ref().map(|s| s.line.as_str());
    let mut cmd = launch::command(folder.clone(), run);
    cmd.env(SESSION_ENV, &session);
    // Codex is resumed by a directory rather than by a name, so the pane is pointed at one of its own
    // and the path goes down on that frame's row (`AMB-D-869`, `crate::codex_home`). Every other
    // provider is given nothing here: their way back is a session id on the launch line.
    if let Some(frame) = frame.as_deref() {
        if let Some(home) = crate::codex_home::for_pane(frame, agent_id.as_deref()) {
            cmd.env(crate::codex_home::ENV, &home);
            face.resumed_from(frame, home.to_string_lossy().into_owned());
        }
    }
    // The drop box is made here rather than left for the first statement to make, so that a pane which
    // cannot be spoken to is one the surface layer refuses in from the start: with no directory named,
    // every verb fails loudly inside the terminal instead of writing where nothing is watching.
    let drop_box = std::env::temp_dir().join(format!("{DROP_BOX_PREFIX}{session}"));
    match std::fs::create_dir_all(&drop_box) {
        Ok(()) => {
            cmd.env(DIR_ENV, &drop_box);
        }
        Err(e) => log::warn!("no drop box for session {session}: {e}"),
    }

    let mut child = pair.slave.spawn_command(cmd).map_err(failed)?;
    let killer = child.clone_killer();
    // Let go of the slave now the child holds its own. While this process keeps it open the master
    // never reaches end-of-file, so the drain below would sit there for good after the program
    // exited and the pane would never be told it had closed.
    drop(pair.slave);

    let reader = pair.master.try_clone_reader().map_err(failed)?;
    let writer = pair.master.take_writer().map_err(failed)?;
    // The chunks go to whichever window asked for the terminal. Nothing here decides which that is:
    // the pane that called is the pane that draws, and if the user later moves it to the other
    // window, `pty_attach` moves this along with it.
    let pane = Arc::new(Pane::new(window.label(), (cols, rows)));

    let opened_in = folder.as_ref().map(|f| f.to_string_lossy().into_owned());

    terminals.0.lock().expect("terminals lock").insert(
        session.clone(),
        Terminal {
            folder,
            agent: agent_id.clone(),
            master: pair.master,
            writer,
            killer,
            pane: Arc::clone(&pane),
            started_at,
        },
    );

    // The one provider that names its own handle: the pane is started, and which session it took is
    // read back out of the provider's own list once it has one (`crate::agent_sessions`).
    //
    // Only where a session is being made. A pane coming back into one already has its handle, and no
    // new row appears in that folder for the reading to find.
    if let (Some(frame), Some(folder), Some((command, ask))) = (
        frame.filter(|_| back.is_none()),
        opened_in.clone(),
        launch.and_then(|launch| Some((launch.command, launch.resume.as_ref()?.ask?))),
    ) {
        read_back(app.clone(), command, ask, frame, folder, opened_at);
    }

    listen(app.clone(), session.clone(), Arc::clone(&pane), drop_box);
    // Once the terminal is in the registry, which is where the hand-over reaches for the writer. It
    // may well start before the drain thread below has put anything in the tail it reads; a pane
    // holding nothing is one it waits on rather than writes into (see the handover module).
    if let Some(instruction) = started.and_then(|s| s.hand_over) {
        hand_over(app.clone(), session.clone(), Arc::clone(&pane), instruction);
    }

    let id = session.clone();
    std::thread::spawn(move || {
        drain(&app, &id, &pane, reader);
        // Reap the program before the pane is told, so nothing is left behind for the length of a
        // round trip to the webview. Its exit status says nothing a person needs: what a terminal
        // ends with is what is on the screen, which the pane already has.
        let _ = child.wait();
        // Whether the registry still held it says who ended it: `pty_close` takes the entry out
        // before it kills, so an entry still here is a program that ended on its own.
        let itself = app
            .state::<Terminals>()
            .0
            .lock()
            .expect("terminals lock")
            .remove(&id)
            .is_some();
        // A program that ended by itself within moments of starting never got as far as a session,
        // so the handle written down for it is taken back before it can refuse the next run too
        // (`crate::frames::TalkFace::gave_up`).
        if let Some(frame) = on_the_line.filter(|_| itself && opened.elapsed() < BELIEVED_AFTER) {
            app.state::<crate::frames::TalkFace>().gave_up(&frame);
        }
        let _ = app.emit_to(pane.target().as_str(), CLOSED_EVENT, &id);
    });

    Ok(PtySessionDto {
        session,
        folder: opened_in,
        agent: agent_id,
    })
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

/// How long a drop box nobody has written to is left alone before it is swept. Long enough that it can
/// only be a directory from a run that is over: a pane speaks the moment a person starts using it, and
/// one that has been silent for a day has nothing in it worth keeping either way.
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// What a drop box's name begins with, which is how it is told from everything else sharing the
/// temporary directory ([`OUR_PREFIXES`]).
const DROP_BOX_PREFIX: &str = "amenbo-session-";

/// What pasted images are kept under — a pane's ([`paste_box`]) and the run's own ([`page_box`])
/// alike. **A directory apart from the drop box**, because the drop box is watched and everything
/// put in it is read as a statement the agent made ([`listen`]) — an image left there would be read
/// as one and thrown away for not parsing.
const PASTE_BOX_PREFIX: &str = "amenbo-pasted-";

/// The names the sweep answers for: every directory this process leaves in the temporary directory
/// the whole machine shares, and nothing else.
const OUR_PREFIXES: [&str; 2] = [DROP_BOX_PREFIX, PASTE_BOX_PREFIX];

/// Where one pane's pasted images go. **Made on the first paste rather than with the terminal**: most
/// panes never take an image, and a directory made for every one of them is a directory the sweep
/// then has to come back for.
fn paste_box(session: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{PASTE_BOX_PREFIX}{session}"))
}

/// What the run's own paste box is named between the prefix and its randomness. **`page-` is a name
/// no session can have**: a session id is sixteen bytes in hex ([`new_session`]), and hex has no `p`
/// and no `-` in it, so the run's box can never be mistaken for a pane's.
const PAGE_BOX_INFIX: &str = "page-";

/// Where an image pasted somewhere that is not a pane goes — the draft page's field, the file
/// panel's editor.
///
/// **One directory for the run rather than one per place**, because neither of those places is a
/// session: they are drawn once for the whole window and nothing closes them the way closing a pane
/// closes a terminal. So there is no moment to take a box of theirs away at, and a box per place
/// would only mean more of them left standing.
///
/// **It is left for the next launch's sweep**, which is what the app does with everything volatile
/// it cannot take away on the way out (`sweep`). A path pasted into a draft therefore reaches the
/// image for as long as it is worth reaching — the run it was pasted in, and a while after — and a
/// draft kept for longer than that keeps a path and not a picture. That is the whole of what this
/// door promises.
///
/// Drawn once and held, so every paste in one run lands in the same place.
fn page_box() -> &'static std::path::Path {
    static BOX: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    BOX.get_or_init(|| {
        std::env::temp_dir().join(format!("{PASTE_BOX_PREFIX}{PAGE_BOX_INFIX}{}", random_hex(8)))
    })
}

/// Write an image the webview was handed into the directory kept for whoever took the paste, and
/// answer with the path it landed at.
///
/// **The bytes are read in the webview and nothing here decodes them** (`AMB-D-854`). The engine has
/// already turned whatever the machine's clipboard was holding into a `File` whose type it names —
/// macOS's TIFF and Windows' DIB both arrive as PNG — so this door writes what it is given under the
/// name that type asks for.
///
/// **What it is for is that a place a person writes can only be pasted into as text.** A terminal
/// takes a line, and so does a draft and a file being edited, so an image has to become a path
/// before the paste can happen at all. What is written around the path is the pasting side's: a pane
/// quotes it because a name with a space in it is two words to a shell, and an editor does not
/// (`AMB-D-832`).
///
/// **A pane names its session and everywhere else names none.** With a session, the image goes to
/// that pane's box and a session naming no open terminal is refused rather than written for — one
/// made for a pane that is gone is one nothing will ever take away. Without, it goes to the run's
/// own box ([`page_box`]), which is what the draft page and the panel's editor paste into
/// (`AMB-T-4446`).
#[tauri::command]
pub fn pty_paste_image(
    terminals: tauri::State<'_, Terminals>,
    session: Option<String>,
    mime: String,
    bytes: Vec<u8>,
) -> Result<String, CmdError> {
    let dir = match &session {
        Some(session) => {
            if !terminals.0.lock().expect("terminals lock").contains_key(session) {
                return Err(gone(session));
            }
            paste_box(session)
        }
        None => page_box().to_path_buf(),
    };
    let extension = extension_for(&mime)
        .ok_or_else(|| paste_refused(format!("{mime} is not an image type a file can be named for")))?;
    std::fs::create_dir_all(&dir).map_err(paste_refused)?;
    let path = dir.join(format!("pasted-{}.{extension}", random_hex(4)));
    std::fs::write(&path, bytes).map_err(paste_refused)?;
    Ok(path.to_string_lossy().into_owned())
}

/// The extension a file of this type is named with, or none where there is no naming it.
///
/// **A closed table rather than the subtype as it stands**, because the answer is composed into a
/// path and the type is a string the webview handed over. Two rows are not their subtype: `jpeg` is
/// written `.jpg` the way everything that makes one writes it, and `svg+xml` carries a `+` no file
/// name wants.
///
/// Anything the engine hangs off the type — `image/png;charset=…` — is cut before the reading. What
/// is being asked is which format it is, and a parameter never answers that.
fn extension_for(mime: &str) -> Option<&'static str> {
    let name = mime.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    Some(match name.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        _ => return None,
    })
}

/// The refusal for an image that could not be written down — a type no file can be named for, or the
/// filesystem saying no. The pane meets both the same way: the paste does not happen, and the reader
/// is told what stopped it.
fn paste_refused(reason: impl std::fmt::Display) -> CmdError {
    let reason = reason.to_string();
    CmdError::coded(
        "pty_paste_failed",
        format!("The pasted image could not be saved: {reason}"),
        serde_json::json!({ "reason": reason }),
    )
}

/// Clear the drop boxes an earlier run left behind. Call it once, off the launch path.
///
/// A terminal that closes takes its own away ([`listen`]), but a run that ends without closing them —
/// the app quit, the machine restarted — cannot: the thread that would do it goes with the process. So
/// the tidying is done from the other end, by whoever comes up next.
///
/// **A box is judged by when it was last written to, not by whose it is.** Several Amenbos share one
/// temporary directory — the shipped one, a development build, another checkout's — and none of them
/// can tell whether another's session is still running. Age settles it without asking: nothing but a
/// dead run's leavings is a day untouched. And being wrong is cheap, because a box is only ever read
/// forwards — a statement written after one is swept lands in a directory the writer makes again, and
/// the pane reading it is none the wiser.
pub fn sweep() {
    sweep_in(&std::env::temp_dir(), STALE_AFTER);
}

/// The sweep itself, over a named directory and against a named age — so it can be asked what it does
/// somewhere other than the one temporary directory the whole machine shares.
fn sweep_in(dir: &std::path::Path, older_than: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let ours = path
            .file_name()
            .is_some_and(|n| OUR_PREFIXES.iter().any(|p| n.to_string_lossy().starts_with(p)));
        if !ours {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .and_then(|at| at.elapsed().map_err(std::io::Error::other))
            .is_ok_and(|since| since > older_than);
        if stale {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// Watch one pane's drop box, carrying each statement on to the pane as it appears, and take the
/// directory away when the terminal it belonged to is gone.
///
/// **Looked in rather than watched for.** A statement is a person-scale event — an agent says a handful
/// in a whole session — so a poll of a directory holding a handful of small files costs less than the
/// machinery that would tell us it changed, and it behaves the same on all three operating systems.
///
/// The registry is asked *before* each read and the loop ends *after* one, so the statements an agent
/// makes in its last breath are carried before the box is taken away.
fn listen(app: tauri::AppHandle, session: String, pane: Arc<Pane>, dir: std::path::PathBuf) {
    std::thread::spawn(move || {
        let mut last: Option<String> = None;
        loop {
            let open = app
                .state::<Terminals>()
                .0
                .lock()
                .expect("terminals lock")
                .contains_key(&session);
            // A drop box that cannot be read is silence, not an error: it is watched while it is being
            // written to, and the next look is 200ms away.
            for said in amenbo_core::session::said_after(&dir, last.as_deref()).unwrap_or_default() {
                last = Some(said.name.clone());
                // The pane keeps what is its own before the window is told: this thread is the only
                // reader of the box, so a statement passed on without being taken in here is one
                // nothing keeps (`AMB-D-805`).
                pane.take_in(&said);
                let dto = SessionSaidDto::of(&session, said);
                // To the window drawing the pane, which is where the output goes and for the same
                // reason: the terminal is drawn in whichever window is its home right now, and a
                // statement sent to a fixed one would reach nobody as soon as it moved (`AMB-D-753`).
                if app.emit_to(pane.target().as_str(), SAID_EVENT, dto).is_err() {
                    return;
                }
            }
            if !open {
                break;
            }
            std::thread::sleep(LISTEN_EVERY);
        }
        // Nothing said about a session outlives the session. The window keeps what it needs in memory
        // (`AMB-D-749`), and what is left here is a directory of files nobody will ever read again.
        let _ = std::fs::remove_dir_all(&dir);
        // And the images pasted into the pane, whose paths were pasted into a terminal that is now
        // gone. There may never have been one — a pane that took no image has no directory — and
        // taking away what was never there is the same nothing as taking away what was.
        let _ = std::fs::remove_dir_all(paste_box(&session));
    });
}

impl SessionSaidDto {
    /// One statement in the shape the webview reads it.
    ///
    /// The session is the pane's own rather than the one written in the file: a drop box belongs to one
    /// terminal, so which pane spoke is already known, and taking the file's word for it would let
    /// something that wandered into the directory name a pane it is not in.
    fn of(session: &str, said: amenbo_core::session::Said) -> Self {
        use amenbo_core::session::Statement;
        let verb = said.statement.verb();
        let text = match said.statement {
            Statement::Name(text) => Some(text),
            // The fact is the whole of it, so there is no line to draw (`AMB-D-805`).
            Statement::Briefed => None,
        };
        SessionSaidDto { session: session.to_string(), verb, at: said.at, cwd: said.cwd, text }
    }
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

/// The sessions this process has open, oldest first — each with when it began.
///
/// A pane asks this on the way up, to find out whether the terminal it is there to draw is already
/// running — which it is every time the pane has moved rather than been made: split out into its own
/// window, folded back into the board, or rebuilt in place because the interface around it was
/// (`app/src/shell/TerminalFace.tsx`). The registry is the only thing that knows, because it is the
/// only part of a terminal that outlives the window: a webview that went away took its emulator with
/// it and could tell nothing to whatever draws next.
///
/// **The order is part of the answer.** The face puts each session back in the place whose folder it
/// is running in, and two panes working in one folder are told apart by nothing else — so the oldest
/// session goes in the oldest place, which is the pairing they were opened in. Left as the registry
/// holds them the order is a `HashMap`'s, which is to say a different one each run: the two panes
/// would trade contents at some splits and not others, and each would then be drawn under the other
/// one's name, since a name belongs to the place (`amenbo_core::frames`). `started_at` alone can tie
/// — two panes opened in the same second — so the session's own id settles it, arbitrarily but the
/// same way every time.
#[tauri::command]
pub fn pty_sessions(terminals: tauri::State<'_, Terminals>) -> Vec<PtySessionDto> {
    in_open_order(
        terminals
            .0
            .lock()
            .expect("terminals lock")
            .iter()
            .map(|(session, terminal)| {
                (
                    terminal.started_at.clone(),
                    PtySessionDto {
                        session: session.clone(),
                        folder: terminal.folder.as_ref().map(|f| f.to_string_lossy().into_owned()),
                        agent: terminal.agent.clone(),
                    },
                )
            })
            .collect(),
    )
}

/// The sessions as [`pty_sessions`] answers with them: oldest first, ties settled by the session's
/// own id so the same set always comes back in the same order.
///
/// Each session comes in paired with when it started, which is the registry's to keep and not part
/// of the answer: the order is settled here, and a pane is handed it already in that order. The
/// timestamp is RFC 3339 with a fixed offset, so the text sorts the way the instants do.
fn in_open_order(mut open: Vec<(String, PtySessionDto)>) -> Vec<PtySessionDto> {
    open.sort_by(|(a_at, a), (b_at, b)| (a_at, &a.session).cmp(&(b_at, &b.session)));
    open.into_iter().map(|(_, one)| one).collect()
}

/// Draw an already-open terminal in the pane that is asking, and hand back what it has said lately.
///
/// This is what a pane calls in place of [`pty_open`] when it is taking over a session that is
/// already running: the same terminal shown in the other window after the user split the app in two
/// or folded it back, and the same terminal after a language change rebuilt the interface around it.
/// Nothing about the terminal moves — the program inside it is never told that the window it is
/// drawn in changed, and never stops running for it.
///
/// What comes back is the tail in the runs it was written in ([`Recent`]), each carrying the size
/// it belongs to and its bytes base64-encoded — the way a chunk is, and for the same reason: these
/// are bytes rather than text. The pane reads them back run by run at the size on each, which is
/// what keeps a tail written across a resize folded where it was written. See [`Pane::adopt`] for
/// what the oldest of them can look like.
#[tauri::command]
pub fn pty_attach(
    window: tauri::Window,
    terminals: tauri::State<'_, Terminals>,
    session: String,
) -> Result<Vec<PtyReplayDto>, CmdError> {
    let open = terminals.0.lock().expect("terminals lock");
    let terminal = open.get(&session).ok_or_else(|| gone(&session))?;
    Ok(terminal.pane.adopt(window.label()))
}

/// End the program in a terminal, and forget the session.
///
/// **It is the only way out.** A pane going away leaves the terminal running — that is a pane moving
/// between windows or pages, and the session is not the pane (`AMB-D-753`) — so short of this, a
/// terminal ends when the program in it decides to, which is the one thing a runaway will not do.
///
/// The registry entry is dropped here rather than left for the drain to clear, so a second close says
/// the terminal is gone instead of trying to kill it twice. The drain ends on its own once the program
/// does, and emits the close the pane listens for: what is on the screen stays as it is, which is what
/// a terminal ends with.
#[tauri::command]
pub fn pty_close(terminals: tauri::State<'_, Terminals>, session: String) -> Result<(), CmdError> {
    let mut terminal = terminals
        .0
        .lock()
        .expect("terminals lock")
        .remove(&session)
        .ok_or_else(|| gone(&session))?;
    terminal.killer.kill().map_err(failed)
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

/// Send the opening sentence this pane is still owed, now that a person has pressed Enter in it.
///
/// **The press is what makes this safe.** Everything the hand-over withholds a newline for is one
/// question — is that an input box, or a program's own first question — and a person sending
/// something of their own into a box they can see settles it (`AMB-D-805`). So this is the one place
/// the sentence goes in with the newline that submits it ([`crate::handover::paste_and_send`]).
///
/// **It is asked on every eligible press and answers once.** Which press that is belongs to the pane
/// drawing the terminal, which is where a key is; whether anything is owed belongs here, where the
/// two things that settle it are:
///
/// - the pane has been briefed, and the sentence is not owed however it got there. The fact outranks
///   the screen and outranks this — an agent that ran `amenbo agent` has the canon, and a second copy
///   arriving in its input box would be Amenbo talking over the person's own first message;
/// - nothing is left to send, because the hand-over got through, the sentence rode in on the command
///   line, or an earlier press already sent it ([`Pane::take_unsent`]).
///
/// **A pane that has merely spoken is not one that has been briefed.** An agent saying anything at
/// all is not the answer — what is owed is a narrower question, and only the one verb settles it.
///
/// **Nothing comes back but whether the terminal was there.** Whether the sentence went used to be
/// answered here, for a row above the pane that said a sentence was sitting in an input box and had
/// to stop saying it once the sending happened. That row now carries the pane's name and nothing else
/// (`AMB-D-862`), so there is no notice left to take back and no reader for the answer. A press that
/// turned out to need nothing is not a failure either. Only a terminal that is not there at all is
/// refused, the same way a write to one is.
#[tauri::command]
pub fn pty_brief(terminals: tauri::State<'_, Terminals>, session: String) -> Result<(), CmdError> {
    let mut open = terminals.0.lock().expect("terminals lock");
    let terminal = open.get_mut(&session).ok_or_else(|| gone(&session))?;
    if terminal.pane.briefed() {
        return Ok(());
    }
    let Some(instruction) = terminal.pane.take_unsent() else { return Ok(()) };
    let bytes = crate::handover::paste_and_send(&instruction);
    terminal
        .writer
        .write_all(&bytes)
        .and_then(|()| terminal.writer.flush())
        .map_err(failed)
}

/// The line to type into a pane running `agent` to have it call itself `name`, or `None` where there
/// is nothing to type — no agent in the pane, one Amenbo has no launch row for, or one whose product
/// has no rename command of its own ([`amenbo_core::harness::Rename`]).
///
/// The name is cut to what the provider takes rather than sent to be refused: Copilot answers a name
/// over its bound with an error and keeps the name it had. Amenbo's own names are already shorter
/// than every bound in the table (`amenbo_core::frames::NAME_LIMIT`), so this is what keeps that true
/// when a row is added rather than what it costs today.
fn rename_line(agent: Option<&str>, name: &str) -> Option<String> {
    let rename = agent.and_then(amenbo_core::harness::find_launch)?.rename.as_ref()?;
    let name = match rename.limit {
        Some(limit) => name.chars().take(limit).collect::<String>(),
        None => name.to_owned(),
    };
    Some(format!("{} {name}", rename.command))
}

/// Tell the provider running in this pane that the pane is now called `name`.
///
/// **The card is the name and this is the copy.** Amenbo's own name for the frame is settled before
/// this is called and is not waiting on it (`crate::frames`); what this does is put the provider's
/// own rename command in the pane the way a person would type it, so the provider's list of sessions
/// says the same thing the row above the pane does (`AMB-D-872`).
///
/// **Nothing happens for a provider with no such command.** OpenCode and Gemini CLI have none, and a
/// line typed at them goes to the model as a sentence and is answered as one
/// ([`amenbo_core::harness::Rename`]) — so those, and a pane running no agent at all, are answered
/// with nothing done rather than with a refusal. Only a session id naming no terminal is refused.
///
/// **It answers before the name is anywhere.** What follows is a wait — for the opening sentence to
/// be out of the input box, and for the pane to stand still — and the caller has nothing to do with
/// it: the name it asked for is already on the frame, and this is the copy catching up.
#[tauri::command]
pub fn pty_rename(
    app: tauri::AppHandle,
    terminals: tauri::State<'_, Terminals>,
    session: String,
    name: String,
) -> Result<(), CmdError> {
    let (line, pane) = {
        let open = terminals.0.lock().expect("terminals lock");
        let terminal = open.get(&session).ok_or_else(|| gone(&session))?;
        let Some(line) = rename_line(terminal.agent.as_deref(), &name) else { return Ok(()) };
        (line, Arc::clone(&terminal.pane))
    };
    if pane.rename_to(line) {
        rename_pane(app, session, pane);
    }
    Ok(())
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
        .map_err(failed)?;
    // Said only once the terminal took it. What follows is kept as a run of its own, and a run opened
    // for a size the program is not writing to would hand the next pane a fold that never happened.
    terminal.pane.resized((cols, rows));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use portable_pty::CommandBuilder;

    /// The size a pane opens a terminal at, for a test that is not about the size.
    const OPENED_AT: Size = (80, 24);

    /// The line typed into a pane says the provider's own rename command, and only for a provider
    /// that has one.
    ///
    /// **The two with none are the test the money is on.** OpenCode and Gemini CLI read `/rename` as
    /// a sentence and answer it, which is a turn and tokens spent on Amenbo talking to itself
    /// (`AMB-T-4652`).
    #[test]
    fn only_a_provider_with_a_rename_command_is_typed_at() {
        assert_eq!(
            rename_line(Some("claude-code"), "the migration").as_deref(),
            Some("/rename the migration")
        );
        for id in ["opencode", "gemini-cli"] {
            assert_eq!(rename_line(Some(id), "the migration"), None, "{id}");
        }
        // A pane running no agent at all, and one running something Amenbo has no row for.
        assert_eq!(rename_line(None, "the migration"), None);
        assert_eq!(rename_line(Some("a-shell-somebody-registered"), "the migration"), None);
    }

    /// A name longer than the provider takes is cut to what it takes, in characters.
    ///
    /// Copilot is the only row with a bound, and it answers a name over it with an error and keeps
    /// the name it had — so a name that reached it whole would leave the provider showing the *old*
    /// name while Amenbo showed the new one.
    #[test]
    fn a_name_over_the_providers_bound_is_cut_to_it_on_a_character() {
        let long = "の".repeat(120);
        let line = rename_line(Some("github-copilot"), &long).expect("copilot renames");
        let name = line.strip_prefix("/rename ").expect("the command's own line");
        assert_eq!(name.chars().count(), 100, "cut to the bound the measurement found");
        assert_eq!(name, "の".repeat(100), "and cut on a character, not on a byte");
        // The rows with no bound are handed the name whole.
        let claude = rename_line(Some("claude-code"), &long).expect("claude code renames");
        assert_eq!(claude.strip_prefix("/rename ").map(str::chars).map(Iterator::count), Some(120));
    }

    /// One thread carries the names, and the newest name is the one it carries next.
    ///
    /// A rename waits for the pane to stand still, which can be as long as an agent's answer — long
    /// enough for a person to rename the pane again. Two threads pasting into one input box would
    /// put both names in it (`AMB-D-872`).
    #[test]
    fn a_second_naming_replaces_what_the_thread_carries_rather_than_starting_one() {
        let pane = Pane::new("main", OPENED_AT);

        assert!(pane.rename_to("/rename first".to_owned()), "nothing was carrying names yet");
        assert!(!pane.rename_to("/rename second".to_owned()), "a thread is already on it");
        assert!(!pane.rename_to("/rename third".to_owned()));

        assert_eq!(pane.next_rename().as_deref(), Some("/rename third"), "the newest name");
        assert_eq!(pane.next_rename(), None, "and nothing behind it");
        // The thread is down, so the naming after that starts one again.
        assert!(pane.rename_to("/rename fourth".to_owned()));
    }

    /// A pane still being handed its opening sentence is not one a rename may type into.
    ///
    /// Both of them paste into a screen that has stood still, so both could pick the same one — and
    /// what a person would then press Enter on is a single line made of two.
    #[test]
    fn the_opening_sentence_has_the_input_box_until_it_is_out_of_it() {
        let pane = Pane::new("main", OPENED_AT);
        assert!(!pane.opening(), "a pane nothing was handed");

        pane.handing_over(true);
        assert!(pane.opening(), "while the thread is on it");
        pane.handing_over(false);
        assert!(!pane.opening());

        // And the sentence left in the box holds it just as the thread did: it is sitting there
        // waiting for a person's Enter, and a rename pasted after it would go out behind it.
        pane.leave("Before you act on any request".to_owned());
        assert!(pane.opening(), "while the sentence is in the box");
        assert!(pane.take_unsent().is_some());
        assert!(!pane.opening(), "and it is free once the sentence has gone");
    }

    /// The bytes of every run a pane was handed, back in one piece — for the tests that are asking
    /// what was kept rather than how it was cut.
    fn run_bytes(replay: &[PtyReplayDto]) -> Vec<u8> {
        replay
            .iter()
            .flat_map(|run| {
                base64::engine::general_purpose::STANDARD
                    .decode(&run.base64)
                    .expect("a run this process encoded")
            })
            .collect()
    }

    /// The tail a pane adopting a session is given never outgrows its cap, however much the
    /// program in the terminal writes — a `yes` left running is the case, and a buffer that grew
    /// with it would be this process's memory going with it.
    #[test]
    fn what_is_kept_for_the_next_pane_stops_at_the_cap() {
        let pane = Pane::new("main", OPENED_AT);
        for _ in 0..8 {
            pane.keep(&vec![b'x'; RECENT / 2]);
        }
        assert_eq!(run_bytes(&pane.adopt("main")).len(), RECENT);
    }

    /// And what it keeps is the *end* of the output, not the start: what a pane has to draw is
    /// where the terminal is now, and the prompt it is sitting on is the last thing written.
    #[test]
    fn what_is_kept_is_the_end_of_the_output() {
        let pane = Pane::new("main", OPENED_AT);
        pane.keep(&vec![b'o'; RECENT]);
        pane.keep(b"$ ");
        let kept = run_bytes(&pane.adopt("main"));
        assert_eq!(kept.len(), RECENT);
        assert_eq!(&kept[kept.len() - 2..], b"$ ");
    }

    /// The tail is handed over in the runs it was written in, each carrying the size it was written
    /// at. A pane reading the whole of it at one size would fold everything written before the last
    /// resize in the wrong places (`AMB-T-4516`).
    #[test]
    fn the_tail_is_cut_where_the_size_changed() {
        let pane = Pane::new("main", (110, 30));
        pane.keep(b"wide");
        pane.resized((26, 30));
        pane.keep(b"narrow");

        let replay = pane.adopt("main");
        assert_eq!(
            replay
                .iter()
                .map(|run| (run.cols, run.rows, run_bytes(std::slice::from_ref(run))))
                .collect::<Vec<_>>(),
            vec![
                (110, 30, b"wide".to_vec()),
                (26, 30, b"narrow".to_vec()),
            ]
        );
    }

    /// A size the terminal is already at opens no run. Chunks arrive by the hundred and a resize is
    /// rare, so the common tail is one run and stays one.
    #[test]
    fn writing_on_at_the_same_size_stays_one_run() {
        let pane = Pane::new("main", OPENED_AT);
        pane.keep(b"one");
        pane.resized(OPENED_AT);
        pane.keep(b"two");
        let replay = pane.adopt("main");
        assert_eq!(replay.len(), 1);
        assert_eq!(run_bytes(&replay), b"onetwo".to_vec());
    }

    /// The cap falls on the tail rather than on a run: the oldest run goes first and whole runs fall
    /// away with it, so a terminal resized often keeps the same quarter of a megabyte as one never
    /// resized. What is left is still each at the size it was written at.
    #[test]
    fn the_cap_drops_the_oldest_runs_first() {
        let pane = Pane::new("main", (110, 30));
        pane.keep(&vec![b'o'; RECENT]);
        pane.resized((26, 30));
        pane.keep(&vec![b'n'; RECENT]);

        let replay = pane.adopt("main");
        assert_eq!(replay.len(), 1, "the wide run is wholly out of the tail");
        assert_eq!((replay[0].cols, replay[0].rows), (26, 30));
        assert_eq!(run_bytes(&replay).len(), RECENT);
    }

    /// A terminal nobody has written in yet hands over nothing at all — there is no run to read and
    /// no size to read it at, and a pane opens on the size it measures for itself.
    #[test]
    fn a_terminal_that_has_written_nothing_hands_over_no_runs() {
        let pane = Pane::new("main", OPENED_AT);
        assert!(pane.adopt("main").is_empty());
    }

    /// The modes the program asked for come back however long ago it asked. Bracketed paste is
    /// requested once, in the first hundred bytes; a pane built after the tail has turned over reads
    /// nothing about it and pastes line by line instead (`AMB-T-4566`).
    #[test]
    fn the_modes_outlive_the_bytes_that_set_them() {
        let pane = Pane::new("main", OPENED_AT);
        pane.keep(b"\x1b[?2004h\x1b[?1004h");
        pane.keep(&vec![b'x'; RECENT]);

        let replay = pane.adopt("main");
        let tail = run_bytes(&replay[1..]);
        assert!(!tail.windows(8).any(|w| w == b"\x1b[?2004"), "the tail has turned over");
        assert_eq!(
            run_bytes(std::slice::from_ref(&replay[0])),
            b"\x1b[?1004h\x1b[?2004h".to_vec(),
            "and the modes are read back before it, lowest number first"
        );
    }

    /// The modes are handed over at the size the tail begins at. A run carrying no size would be
    /// read at whatever the pane measured for itself, which is not what the rest of the tail says.
    #[test]
    fn the_modes_are_read_at_the_size_the_tail_begins_at() {
        let pane = Pane::new("main", (110, 30));
        pane.keep(b"\x1b[?2004h");
        pane.resized((26, 30));
        pane.keep(b"narrow");

        let replay = pane.adopt("main");
        assert_eq!((replay[0].cols, replay[0].rows), (110, 30));
    }

    /// The latest value of a mode is the one that comes back: a program that turned bracketed paste
    /// off meant it, and a pane put back into the mode it wanted three screens ago would paste
    /// differently from the terminal it is replacing.
    #[test]
    fn the_latest_value_of_a_mode_is_the_one_kept() {
        let pane = Pane::new("main", OPENED_AT);
        pane.keep(b"\x1b[?2004h");
        pane.keep(b"\x1b[?2004l");
        pane.keep(&vec![b'x'; RECENT]);

        let replay = pane.adopt("main");
        assert_eq!(
            run_bytes(std::slice::from_ref(&replay[0])),
            b"\x1b[?2004l".to_vec()
        );
    }

    /// A sequence split across two chunks is still read. A read ends wherever the kernel filled the
    /// buffer, so the `ESC` and the `h` arriving together is luck rather than a rule.
    #[test]
    fn a_mode_sequence_split_across_chunks_is_still_read() {
        let pane = Pane::new("main", OPENED_AT);
        pane.keep(b"\x1b[?20");
        pane.keep(b"04h");
        pane.keep(&vec![b'x'; RECENT]);

        assert_eq!(
            run_bytes(std::slice::from_ref(&pane.adopt("main")[0])),
            b"\x1b[?2004h".to_vec()
        );
    }

    /// One sequence may name several modes, `;` apart — mouse reporting is asked for that way — and
    /// what is not a mode sequence at all is left alone. `ESC [ ? 2004 $ p` asks what a mode is set
    /// to, and `ESC [ 4 h` is not a private mode.
    #[test]
    fn several_modes_in_one_sequence_are_read_and_the_rest_is_not() {
        let pane = Pane::new("main", OPENED_AT);
        pane.keep(b"\x1b[?1000;1006h\x1b[?2004$p\x1b[4h");
        pane.keep(&vec![b'x'; RECENT]);

        assert_eq!(
            run_bytes(std::slice::from_ref(&pane.adopt("main")[0])),
            b"\x1b[?1000h\x1b[?1006h".to_vec()
        );
    }

    /// A terminal that has written bytes but no mode sequence hands over its runs and nothing else.
    #[test]
    fn a_terminal_in_no_modes_gets_nothing_in_front_of_its_tail() {
        let pane = Pane::new("main", OPENED_AT);
        pane.keep(b"plain");
        let replay = pane.adopt("main");
        assert_eq!(replay.len(), 1);
        assert_eq!(run_bytes(&replay), b"plain".to_vec());
    }

    /// A pane put back in its place is put there by folder and by nothing else, so two terminals
    /// running in one folder are told apart only by the order they come back in. Oldest first is
    /// what pairs them with the places they were opened in; a `HashMap`'s order would trade their
    /// contents at some splits and not others, and each would then be drawn under the other's name.
    #[test]
    fn the_sessions_come_back_in_the_order_they_were_started() {
        let at = |session: &str, started_at: &str| {
            (
                started_at.to_owned(),
                PtySessionDto {
                    session: session.into(),
                    folder: Some("/work/repo".into()),
                    agent: None,
                },
            )
        };
        let order = |open: Vec<(String, PtySessionDto)>| {
            in_open_order(open).into_iter().map(|one| one.session).collect::<Vec<_>>()
        };

        let newest_first = vec![
            at("c", "2026-08-24T00:00:02Z"),
            at("b", "2026-08-24T00:00:01Z"),
            at("a", "2026-08-24T00:00:00Z"),
        ];
        assert_eq!(order(newest_first), ["a", "b", "c"]);

        // Two panes opened in the same second still come back the same way round every time.
        let tied = vec![
            at("y", "2026-08-24T00:00:00Z"),
            at("x", "2026-08-24T00:00:00Z"),
        ];
        assert_eq!(order(tied), ["x", "y"]);
    }

    /// The window a session's chunks go to is the pane's to move, which is the whole of how a
    /// terminal changes windows without being restarted.
    #[test]
    fn adopting_a_session_sends_what_follows_to_the_new_window() {
        let pane = Pane::new(crate::windows::BOARD, OPENED_AT);
        assert_eq!(pane.keep(b"before"), crate::windows::BOARD);
        assert_eq!(run_bytes(&pane.adopt(crate::windows::TALK)), b"before".to_vec());
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
    #[cfg(unix)]
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

    /// The sweep takes what a dead run left and nothing else — both the directories a pane makes, and
    /// neither of them while it is in use. The temporary directory is shared with every other program
    /// on the machine, so what it passes over matters as much as what it removes.
    #[test]
    fn the_sweep_takes_what_a_pane_leaves_and_nothing_else() {
        let dir = amenbo_scratch::scratch("pty-sweep");
        let box_one = dir.join(format!("{DROP_BOX_PREFIX}aaaa"));
        let pasted = dir.join(format!("{PASTE_BOX_PREFIX}aaaa"));
        let not_ours = dir.join("some-other-programs-work");
        for made in [&box_one, &pasted, &not_ours] {
            std::fs::create_dir_all(made).expect("made");
        }
        std::fs::write(box_one.join("a-statement.json"), "{}").expect("written");
        std::fs::write(pasted.join("pasted-0a0b0c0d.png"), [0u8]).expect("written");

        // Nothing is old enough yet: a box in use is a box that stays.
        sweep_in(&dir, std::time::Duration::from_secs(24 * 60 * 60));
        assert!(box_one.is_dir(), "a box written to a moment ago is still in use");
        assert!(pasted.is_dir(), "and so is the directory its pasted images are in");

        sweep_in(&dir, std::time::Duration::ZERO);
        assert!(!box_one.exists(), "the drop box and the statements in it are gone");
        assert!(!pasted.exists(), "and the pasted images with it");
        assert!(not_ours.is_dir(), "and what was never ours was not touched");
    }

    /// A pasted image is named for the type the webview gave it. The name is composed into a path, so
    /// what is read is a closed table: a type with no row in it is refused rather than guessed at.
    #[test]
    fn a_pasted_image_is_named_for_its_type_and_nothing_else_is_named_at_all() {
        assert_eq!(extension_for("image/png"), Some("png"));
        // Not the subtype as it stands — a file of this type is written .jpg everywhere else too.
        assert_eq!(extension_for("image/jpeg"), Some("jpg"));
        // What the engine hangs off the type says nothing about which format it is.
        assert_eq!(extension_for("image/png;charset=binary"), Some("png"));
        assert_eq!(extension_for("IMAGE/PNG"), Some("png"));

        assert_eq!(extension_for("text/plain"), None);
        assert_eq!(extension_for("image/heic"), None);
        assert_eq!(extension_for(""), None);
    }

    /// The two directories a pane makes are told apart by name. Nothing else keeps them apart: the
    /// drop box is watched and reads everything in it as a statement, so an image put there would be
    /// read as one and thrown away for not parsing.
    #[test]
    fn the_pasted_images_are_not_in_the_drop_box() {
        let session = "aaaa";
        let pasted = paste_box(session);
        let drop_box = std::env::temp_dir().join(format!("{DROP_BOX_PREFIX}{session}"));
        assert_ne!(pasted, drop_box);
        assert!(!pasted.starts_with(&drop_box), "and neither is inside the other");
        assert!(!drop_box.starts_with(&pasted));
    }

    /// The box the draft page and the editor paste into is the run's, and it is one box: every paste
    /// made outside a pane lands in the same place for as long as the app is up.
    #[test]
    fn everywhere_that_is_not_a_pane_pastes_into_one_box() {
        assert_eq!(page_box(), page_box(), "asked twice in a run, it is the same directory");
    }

    /// It is a box the sweep answers for, and one no pane can be handed. A session is hex, and this
    /// is not — so a run's box and a pane's never collide however either is named.
    #[test]
    fn the_run_s_box_is_swept_and_is_no_pane_s() {
        let name = page_box().file_name().expect("a name").to_string_lossy().into_owned();
        assert!(
            OUR_PREFIXES.iter().any(|p| name.starts_with(p)),
            "the sweep would leave `{name}` behind"
        );
        assert!(name.starts_with(&format!("{PASTE_BOX_PREFIX}{PAGE_BOX_INFIX}")));
        // No session can be named into this box, because a session id is hex and this is not.
        assert!(!new_session().starts_with(PAGE_BOX_INFIX), "a session cannot be named `page-…`");
        assert_ne!(page_box(), paste_box(&new_session()), "so it is not any pane's");
    }

    /// The one statement a pane keeps for itself, taken off the same drop box the window reads.
    ///
    /// What a person is told and what the pane knows come out of one pass over the box, so this walks
    /// the real statements rather than the verb alone: everything an agent says about its work goes
    /// past without leaving a mark, and the fact that it ran `amenbo agent` leaves one.
    #[test]
    fn only_the_fact_that_the_agent_ran_leaves_the_pane_briefed() {
        use amenbo_core::session::{say, Statement, Surface};

        let dir = amenbo_scratch::scratch("pty-briefed");
        let surface = Surface { session: "a-session".into(), dir: dir.clone() };
        let pane = Pane::new("main", OPENED_AT);

        say(&surface, &Statement::Name("the top fix".into())).expect("said");
        for said in amenbo_core::session::said_after(&dir, None).expect("read") {
            pane.take_in(&said);
        }
        assert!(!pane.briefed(), "nothing an agent says about its work is the fact");

        say(&surface, &Statement::Briefed).expect("said");
        for said in amenbo_core::session::said_after(&dir, None).expect("read") {
            pane.take_in(&said);
        }
        assert!(pane.briefed(), "and the fact itself is");
    }

    /// What a pane is owed goes out once, whatever a person presses after.
    ///
    /// The guard is the taking rather than a second flag: a press that finds nothing to send is a
    /// press that does nothing, and there is no window in which two of them could each find the
    /// sentence still there.
    #[test]
    fn the_sentence_a_pane_was_left_holding_goes_out_once() {
        let pane = Pane::new("main", OPENED_AT);
        assert!(pane.take_unsent().is_none(), "a pane the hand-over got through to is owed nothing");

        pane.leave("Before you act on any request".into());
        assert_eq!(pane.take_unsent().as_deref(), Some("Before you act on any request"));
        assert!(pane.take_unsent().is_none(), "and the next press finds nothing left to send");
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
