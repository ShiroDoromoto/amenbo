//! The **surface layer** of Amenbo's vocabulary: what an AI says about the session it is running in,
//! inside the talk window's terminal (`AMB-D-749`).
//!
//! Everything else Amenbo does lands in the store, means the same wherever it is typed, and is still
//! true tomorrow. Nothing here is. A session is the terminal it runs in — it has no existence outside
//! that rectangle — so what is said about one is written to the running window and to nowhere else,
//! and is gone when the window is.
//!
//! **The line is drawn by place, not by capability.** The question a new verb is held to is whether it
//! would mean anything typed outside the talk window. `waiting` would not, so it lives here; a task's
//! status would, so it does not. Capability moves — what Amenbo can derive today it could derive
//! differently tomorrow — and a line drawn on it leaves the verbs behind when it moves.
//!
//! **Outside the window every verb here fails, loudly.** Answering "ok" where nothing was shown is the
//! worst thing this layer could do: the AI would believe it had declared something and stop trying,
//! while the person's screen never changed.
//!
//! **A statement reaches the window through the environment.** The window hands each terminal it opens
//! two things ([`SESSION_VAR`], [`DIR_VAR`]): which session this is, and a throwaway directory to leave
//! statements in. An agent runs `amenbo` several processes deep inside that terminal, so the
//! environment is what carries them that distance — nothing outside can work out which pane a process
//! belongs to.
//!
//! A statement is **one file, written whole**: it is composed under a temporary name and renamed into
//! place, so a reader watching the directory either sees a complete statement or no file at all. Names
//! sort in the order they were said. The window reads them, keeps what it needs in memory, and the
//! directory dies with the run — this is a drop box, never a log.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

use crate::error::{Error, Result};

/// The variable a session's id is carried in, into the terminal and everything started inside it. It is
/// set by the window that opened the terminal (`app/src-tauri/src/pty.rs`) and by nothing else.
pub const SESSION_VAR: &str = "AMENBO_SESSION";

/// The variable naming the throwaway directory this run's statements are left in. Set beside
/// [`SESSION_VAR`] by the window, on every terminal it opens.
pub const DIR_VAR: &str = "AMENBO_SESSION_DIR";

/// The shape of a statement file. Bumped when a reader would have to be changed to keep understanding
/// one — the window and this module ship together, but a window left running across an update does not.
pub const SCHEMA: u32 = 1;

/// The talk window's terminal, as seen from inside it: which session this is, and where statements go.
///
/// Holding one is the proof that this process is inside the window. It cannot be constructed from
/// anywhere else — [`surface`] reads it from the environment or answers `None`.
#[derive(Debug, Clone)]
pub struct Surface {
    /// The session's id, as the window knows it.
    pub session: String,
    /// The directory statements are dropped into.
    pub dir: PathBuf,
}

/// The talk window this process is running in, or `None` when it is running anywhere else.
///
/// Both halves have to be there. One without the other is not a window that half-opened: it is an
/// environment somebody copied part of, and a statement written on that footing would be dropped where
/// nothing is watching — a silent success, which is the one answer this layer must never give.
pub fn surface() -> Option<Surface> {
    from_parts(crate::env::session(), crate::env::session_dir())
}

/// The longest session id a write will carry into the ledger. The window mints 32 hex characters
/// ([`SESSION_VAR`]); the cap is here because the variable is inherited from an environment anything
/// can set, and a ledger line above its own size limit is dropped rather than written — a stray value
/// would take the event down with it. A longer id is refused whole rather than cut: a shortened one
/// names a session that does not exist, and naming the wrong session is worse than naming none.
pub const MAX_ID_BYTES: usize = 128;

/// The session a write records itself as coming from, or `None` when the process is running outside the
/// talk window's terminal.
///
/// This is not the surface layer ([`surface`]) and asks for less. A statement needs somewhere to be
/// left, so half an environment is no window at all; a name tag needs nothing but the name — it rides a
/// write that was going to the ledger regardless, and stamping it costs nothing when nobody reads it.
///
/// Blank, whitespace, or past [`MAX_ID_BYTES`]: `None`. An empty author is allowed to mean "unknown";
/// a guess is not.
pub fn id() -> Option<String> {
    id_from(crate::env::session())
}

/// The rule [`id`] applies, apart from the environment it reads — separate for the reason
/// [`from_parts`] is: the environment is process-wide and a test suite is not.
fn id_from(raw: Option<String>) -> Option<String> {
    raw.filter(|s| !s.trim().is_empty() && s.len() <= MAX_ID_BYTES)
}

/// The rule [`surface`] applies, apart from the environment it reads: both halves present, and neither
/// of them blank. It is separate because the environment is process-wide while a test suite is not —
/// the rule can be asked directly, where setting the variables to ask it could not be undone.
fn from_parts(session: Option<String>, dir: Option<std::ffi::OsString>) -> Option<Surface> {
    let session = session.filter(|s| !s.trim().is_empty())?;
    let dir = dir.filter(|d| !d.is_empty())?;
    Some(Surface { session, dir: PathBuf::from(dir) })
}

/// What an AI says about the session it is in. Each variant is one verb of the surface vocabulary.
///
/// Only two of them are owed: [`Statement::Waiting`] and [`Statement::Finished`], which say the things
/// nothing else can find out (`AMB-D-748`). The rest are offered — a name that is never set leaves a
/// pane labelled by its folder, and nobody is misled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// Name this pane. The name sticks to the frame, not to the process in it.
    Name(String),
    /// A line about what is being done now, for the pane's label.
    Note(String),
    /// A person's turn has come, and why. Owed — the window cannot derive it (`AMB-D-748`).
    Waiting(String),
    /// The work is done, and what came of it. Owed, for the same reason.
    Finished(String),
}

/// How much of the pane's label the reason for a person's turn may take, in the columns a terminal
/// would count.
///
/// **The label holds one line, and the reason is the last of three things on it** (`app/src/talk/
/// nameplate.ts`): what the pane is called, what its session is on, and this. A reason written past
/// that does not make the row longer — it pushes the other two into ellipses, and three things all cut
/// short is a label nobody can read any of.
///
/// **So the overflow is stopped at the door rather than mended at the display.** What comes in is a
/// sentence an agent wrote, which is as long as the agent felt like being; the row cannot argue with
/// it afterwards. Refusing costs the agent one rewrite and says exactly what is wanted: the one thing
/// a person has to decide. What led up to it belongs where it will still be true tomorrow — a comment
/// on the task — and saying it here would be writing a paragraph onto a pane's label.
///
/// **Measured in room rather than in characters**, because room is what runs out. A count of
/// characters would give a reason in Japanese twice the row a reason in English gets, which is the
/// same rule reading as two different rules to the two people it is applied to. Sixty columns is
/// thirty Japanese characters or sixty English ones, and either is a sentence.
pub const WAITING_LIMIT: usize = 60;

/// The room a string takes on a row, counted the way a terminal counts it: two columns for the
/// characters that are drawn twice as wide, one for the rest.
///
/// **An approximation, and deliberately a generous one.** The full answer is a Unicode table that
/// moves with every release, and what it is being asked for here is whether a sentence will fit on a
/// label — so the wide blocks that actually turn up in one are named, and anything else counts as
/// narrow. Erring that way lets a rare character through rather than refusing a reason a person could
/// have read.
fn columns(text: &str) -> usize {
    text.chars().map(|c| if wide(c) { 2 } else { 1 }).sum()
}

/// Whether a character is drawn two columns wide. The blocks are the East Asian wide and fullwidth
/// ones — CJK, kana, hangul, the fullwidth forms — plus the emoji that are drawn as wide everywhere
/// they are drawn at all.
fn wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F        // Hangul Jamo, the initial consonants
        | 0x2E80..=0x303E      // CJK radicals through the kana punctuation
        | 0x3041..=0x33FF      // kana, bopomofo, hangul compatibility, the squared abbreviations
        | 0x3400..=0x4DBF      // CJK ideographs, extension A
        | 0x4E00..=0x9FFF      // CJK ideographs
        | 0xA000..=0xA4CF      // Yi
        | 0xAC00..=0xD7A3      // Hangul syllables
        | 0xF900..=0xFAFF      // CJK compatibility ideographs
        | 0xFE30..=0xFE6F      // CJK compatibility forms
        | 0xFF00..=0xFF60      // fullwidth forms
        | 0xFFE0..=0xFFE6      // fullwidth signs
        | 0x1F300..=0x1F64F    // emoji: symbols, pictographs, emoticons
        | 0x1F900..=0x1F9FF    // emoji: supplemental symbols
        | 0x20000..=0x3FFFD    // CJK ideographs, the supplementary planes
    )
}

/// A statement whose text takes more room than it may: how much it takes, and how much it is allowed.
/// It carries no sentence — the surface that refused it assembles that (`amenbo-cli`'s `output`), the
/// way validate's issues do. Both figures are columns ([`columns`]), which is what the message has to
/// say for the two numbers in it to be comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overlong {
    /// The room the text takes, in columns.
    pub got: usize,
    /// The room it is allowed, in columns.
    pub limit: usize,
}

impl Statement {
    /// How far this statement runs past its bound, or `None` when it is within one — which everything
    /// but [`Statement::Waiting`] is, having nothing that has to fit beside two other things.
    pub fn overlong(&self) -> Option<Overlong> {
        let Statement::Waiting(text) = self else { return None };
        let got = columns(text.trim());
        (got > WAITING_LIMIT).then_some(Overlong { got, limit: WAITING_LIMIT })
    }

    /// The word this statement is filed under, and the one the window branches on. It is the verb the
    /// person typed, so the two never drift.
    pub fn verb(&self) -> &'static str {
        match self {
            Statement::Name(_) => "name",
            Statement::Note(_) => "note",
            Statement::Waiting(_) => "waiting",
            Statement::Finished(_) => "finished",
        }
    }

    /// The statement's own fields, on top of the ones every statement carries.
    fn body(&self) -> Value {
        match self {
            Statement::Name(text)
            | Statement::Note(text)
            | Statement::Waiting(text)
            | Statement::Finished(text) => json!({ "text": text }),
        }
    }
}

/// Distinguishes two statements made in the same nanosecond by the same process — which a loop can do,
/// and a clock with millisecond resolution can do easily.
static SAID: AtomicU64 = AtomicU64::new(0);

/// Leave one statement for the window, and answer with the file it was left in.
///
/// The write is whole or absent: a temporary name first, then a rename, which is atomic on every
/// filesystem we run on. A reader woken by the directory changing therefore never parses half a
/// statement.
pub fn say(surface: &Surface, statement: &Statement) -> Result<PathBuf> {
    let at = crate::time::Timestamp::now();
    let mut record = json!({
        "schema": SCHEMA,
        "session": surface.session,
        "at": at.to_rfc3339_z(),
        "verb": statement.verb(),
        // The folder the statement was made in, which is the agent's own: it starts as the one the
        // terminal was opened in and moves with every `cd`, so it is read here rather than assumed from
        // the launch.
        "cwd": std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()),
    });
    merge(&mut record, statement.body());

    fs::create_dir_all(&surface.dir)?;
    let name = file_name();
    let final_path = surface.dir.join(&name);
    let partial = surface.dir.join(format!(".{name}.partial"));
    let mut f = fs::File::create(&partial)?;
    f.write_all(serde_json::to_string(&record)?.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    drop(f);
    fs::rename(&partial, &final_path)?;
    Ok(final_path)
}

/// The file one statement is left in. It sorts in the order statements were made: a fixed-width instant
/// first, then the process and a counter, which separate two made in the same instant without disturbing
/// that order.
///
/// The instant is read at full resolution here rather than taken from the record's `at`, which is a
/// second — the stamp Amenbo displays and promises. A second is far coarser than an agent speaks: four
/// statements in a row land inside one, and the name is the only thing that says which came first.
///
/// The session's id is not in the name. It is chosen by the window, and a name has to be a legal
/// filename on three operating systems.
fn file_name() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = SAID.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:020}-{}-{n:04}.json", std::process::id())
}

/// Fold a statement's own fields into the record every statement shares.
fn merge(into: &mut Value, from: Value) {
    let (Some(target), Some(fields)) = (into.as_object_mut(), from.as_object()) else { return };
    for (k, v) in fields {
        target.insert(k.clone(), v.clone());
    }
}

/// A statement as the window reads it back: what was said, by which session, when, and where.
///
/// The name it was left under rides along, because that is what orders the drop box and what a reader
/// remembers to know how far it has got. A statement is read once — re-reading the directory must not
/// hand the window what it has already been told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// The file it was left in, which sorts in the order statements were made.
    pub name: String,
    /// The pane it was said in, as the window named it.
    pub session: String,
    /// When it was said (RFC3339 UTC).
    pub at: String,
    /// The folder the agent was in when it said it, where that could be read.
    pub cwd: Option<String>,
    /// What was said.
    pub statement: Statement,
}

impl Said {
    /// Read one statement back out of the record [`say`] wrote, or `None` when this reader cannot make
    /// sense of it — a shape from a later version, or a file that is not a statement at all.
    ///
    /// A window left running across an update is what this is for. It goes on reading the statements it
    /// knows and passes over the ones it does not, rather than drawing a verb it can mean nothing by.
    fn read(name: &str, v: &Value) -> Option<Said> {
        if v["schema"].as_u64()? > u64::from(SCHEMA) {
            return None;
        }
        let text = || v["text"].as_str().map(str::to_string);
        let statement = match v["verb"].as_str()? {
            "name" => Statement::Name(text()?),
            "note" => Statement::Note(text()?),
            "waiting" => Statement::Waiting(text()?),
            "finished" => Statement::Finished(text()?),
            _ => return None,
        };
        Some(Said {
            name: name.to_string(),
            session: v["session"].as_str()?.to_string(),
            at: v["at"].as_str()?.to_string(),
            cwd: v["cwd"].as_str().map(str::to_string),
            statement,
        })
    }
}

/// Every statement left in `dir` under a name later than `after`, oldest first — the window's half of
/// the drop box, and the only reader of [`say`]'s format.
///
/// `after` is the [`Said::name`] of the last statement the caller was handed; with `None` the whole box
/// is read. Names sort in the order the statements were made, so "later than" is a string comparison
/// and there is nothing to remember but the last name.
///
/// A file that is not a statement is skipped rather than raised: this directory is watched while it is
/// being written to, and one unreadable file is no reason to lose the rest.
pub fn said_after(dir: &Path, after: Option<&str>) -> Result<Vec<Said>> {
    let mut names: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect(),
        // A directory nobody has said anything in yet is not a failure; it is silence.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::from(e)),
    };
    names.sort();
    Ok(names
        .iter()
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            if after.is_some_and(|last| name.as_str() <= last) {
                return None;
            }
            Said::read(&name, &serde_json::from_str::<Value>(&fs::read_to_string(p).ok()?).ok()?)
        })
        .collect())
}

/// The surface layer's own canon (`amenbo session --json`), for the AI that is inside the window and can
/// therefore use it. It is deliberately absent from `amenbo agent --json`, which is read everywhere:
/// teaching a vocabulary in a place most readers cannot run it would invite exactly the silent failure
/// this layer exists to prevent (`AMB-D-749`).
pub fn spec() -> Value {
    json!({
        "schemaVersion": SCHEMA,
        "layer": "surface",
        "what": "The vocabulary of the terminal you are running in. It moves the pane on the person's \
                 screen and touches no store: nothing said here outlives this window, and none of it \
                 can be said from outside it.",
        "owed": [
            "Say `waiting` the moment a person's turn has come, and why. Nobody can find this out by \
             watching — silence looks the same whether you are building, thinking, or waiting. The \
             reason goes on one line of the pane's label beside two other things, so it is bounded and \
             a longer one is refused rather than cut: say the one thing they have to decide, and leave \
             what led up to it where it will still be true tomorrow.",
            "Say `finished` when the work is done, and what came of it."
        ],
        "offered": [
            "`name` and `note` label the pane. Say nothing and it is labelled by its folder, which \
             misleads no one."
        ],
        "promises": "A statement is information, never a promise. Say what has happened, not what you \
                     will do — a person who believes a promise stops checking, and this layer cannot \
                     make one hold.",
        "commands": [
            { "command": "session name", "args": "<text>", "summary": "Name this pane. The name sticks to the frame, so it survives what runs in it." },
            { "command": "session note", "args": "<text>", "summary": "A line about what you are doing now, shown on the pane's label." },
            { "command": "session waiting", "args": "<text>", "summary": format!("A person's turn has come. Say why in the same breath — the reason is what they read. It goes on one line of the label: up to {WAITING_LIMIT} columns — {WAITING_LIMIT} characters of English or half that of Japanese — and a longer one is refused rather than cut.") },
            { "command": "session finished", "args": "<text>", "summary": "The work is done. Say what came of it." }
        ],
        // Columns rather than characters, so the bound means the same room in every language it is
        // written in. A reader that wants to check before speaking can, and a reader that does not
        // is told by the refusal.
        "limits": { "waiting": { "unit": "columns", "max": WAITING_LIMIT } },
        "outside": "Every one of these fails outside the talk window's terminal, with a non-zero exit. \
                    That is deliberate: a quiet success would leave you believing you had spoken while \
                    the person's screen never changed."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface_at(dir: &Path) -> Surface {
        Surface { session: "pane-1".to_string(), dir: dir.to_path_buf() }
    }

    #[test]
    fn a_statement_is_left_whole_and_says_who_said_it() {
        let dir = amenbo_scratch::scratch("session-say");
        let path = say(&surface_at(&dir), &Statement::Waiting("the migration needs a decision".into()))
            .expect("the statement is written");

        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).expect("valid JSON");
        assert_eq!(v["verb"], "waiting");
        assert_eq!(v["text"], "the migration needs a decision");
        assert_eq!(v["session"], "pane-1", "the pane it was said in rides with it");
        assert_eq!(v["schema"], SCHEMA, "and the shape a reader is holding it to");
        assert!(v["at"].as_str().is_some_and(|s| s.ends_with('Z')), "stamped in UTC: {v}");
    }

    /// The reason for a person's turn is the one thing here with a bound, and what is counted is the
    /// room it takes rather than the characters it is made of: a reason in Japanese and one in English
    /// get the same row, which is the point of measuring at all. Nothing else is bounded — the rest of
    /// the row's places are not shared three ways.
    #[test]
    fn a_reason_past_the_bound_is_named_as_such_and_measured_in_the_room_it_takes() {
        let half = WAITING_LIMIT / 2;
        assert_eq!(
            Statement::Waiting("あ".repeat(half)).overlong(),
            None,
            "a Japanese reason of exactly the bound is within it",
        );
        assert_eq!(
            Statement::Waiting("a".repeat(WAITING_LIMIT)).overlong(),
            None,
            "and so is an English one of the same room, at twice the characters",
        );

        assert_eq!(
            Statement::Waiting("あ".repeat(half + 1)).overlong(),
            Some(Overlong { got: WAITING_LIMIT + 2, limit: WAITING_LIMIT }),
            "one character past it is past it, and it is said in the same unit as the bound",
        );
        assert_eq!(
            Statement::Waiting("a".repeat(WAITING_LIMIT + 1)).overlong(),
            Some(Overlong { got: WAITING_LIMIT + 1, limit: WAITING_LIMIT }),
        );

        assert_eq!(
            Statement::Waiting(format!("  {}  ", "a".repeat(WAITING_LIMIT))).overlong(),
            None,
            "the room the reason takes is the reason, not the spaces around it",
        );

        for other in [
            Statement::Name("x".repeat(WAITING_LIMIT * 3)),
            Statement::Note("x".repeat(WAITING_LIMIT * 3)),
            Statement::Finished("x".repeat(WAITING_LIMIT * 3)),
        ] {
            assert_eq!(other.overlong(), None, "only the reason is bounded: {other:?}");
        }
    }

    /// The text of every statement read back, in the order it came.
    fn texts(said: &[Said]) -> Vec<String> {
        said.iter()
            .map(|s| match &s.statement {
                Statement::Name(t)
                | Statement::Note(t)
                | Statement::Waiting(t)
                | Statement::Finished(t) => t.clone(),
            })
            .collect()
    }

    #[test]
    fn statements_come_back_in_the_order_they_were_said() {
        let dir = amenbo_scratch::scratch("session-order");
        let s = surface_at(&dir);
        for verb in ["one", "two", "three"] {
            say(&s, &Statement::Note(verb.to_string())).expect("written");
        }
        let said = said_after(&dir, None).expect("read back");
        assert_eq!(
            texts(&said),
            vec!["one", "two", "three"],
            "oldest first, even within one millisecond",
        );
        assert_eq!(said[0].session, "pane-1", "the pane it was said in comes back with it");
        assert!(said[0].cwd.is_some(), "and the folder it was said in");
    }

    /// A reader is handed each statement once. Everything the window does with one — a name, a pane's
    /// label, a person's turn — happens on the way past, so being told twice is being told wrongly.
    #[test]
    fn a_reader_is_told_only_what_it_has_not_been_told() {
        let dir = amenbo_scratch::scratch("session-after");
        let s = surface_at(&dir);
        for verb in ["one", "two"] {
            say(&s, &Statement::Note(verb.to_string())).expect("written");
        }
        let first = said_after(&dir, None).expect("read back");
        assert_eq!(texts(&first), vec!["one", "two"]);

        let last = first.last().expect("two were read").name.clone();
        assert!(
            said_after(&dir, Some(&last)).expect("read back").is_empty(),
            "nothing was said since, so nothing comes back",
        );

        say(&s, &Statement::Note("three".into())).expect("written");
        assert_eq!(
            texts(&said_after(&dir, Some(&last)).expect("read back")),
            vec!["three"],
            "and what was said since comes back on its own",
        );
    }

    #[test]
    fn a_half_written_file_is_not_read_and_neither_is_a_directory_nobody_used() {
        let dir = amenbo_scratch::scratch("session-partial");
        let s = surface_at(&dir);
        say(&s, &Statement::Note("real".into())).expect("written");
        // What a composing writer leaves behind: a dotted, extension-less name a reader must skip.
        fs::write(dir.join(".00000000000000000001-1-0000.json.partial"), "{\"verb\":").unwrap();
        let said = said_after(&dir, None).expect("read back");
        assert_eq!(said.len(), 1, "the partial file is not among them: {said:?}");

        assert!(
            said_after(&dir.join("never-made"), None)
                .expect("silence is not a failure")
                .is_empty(),
            "a directory nobody has spoken in reads as empty",
        );
    }

    /// A window left running across an update meets shapes it was not written for. It passes over them
    /// and goes on with the rest — drawing a verb it cannot mean anything by is the failure here.
    #[test]
    fn a_statement_this_reader_cannot_understand_is_passed_over() {
        let dir = amenbo_scratch::scratch("session-unknown");
        let s = surface_at(&dir);
        say(&s, &Statement::Note("real".into())).expect("written");
        for (name, body) in [
            ("00000000000000000001-1-0000.json", json!({ "schema": SCHEMA + 1, "session": "pane-1", "at": "2026-08-24T00:00:00Z", "verb": "note", "text": "from a later version" })),
            ("00000000000000000002-1-0000.json", json!({ "schema": SCHEMA, "session": "pane-1", "at": "2026-08-24T00:00:00Z", "verb": "shrug", "text": "a verb that is not one" })),
            ("00000000000000000003-1-0000.json", json!({ "schema": SCHEMA, "session": "pane-1", "at": "2026-08-24T00:00:00Z", "verb": "note" })),
        ] {
            fs::write(dir.join(name), body.to_string()).unwrap();
        }
        let said = said_after(&dir, None).expect("read back");
        assert_eq!(texts(&said), vec!["real"], "only the one it understands: {said:?}");
    }

    #[test]
    fn the_surface_needs_both_halves_named() {
        let named = |s: &str, d: &str| {
            from_parts(Some(s.to_string()), Some(std::ffi::OsString::from(d))).is_some()
        };
        assert!(named("pane-1", "/tmp/drop"), "both halves named: this is the window");
        assert!(!named("", "/tmp/drop"), "a blank session names no pane");
        assert!(!named("  ", "/tmp/drop"), "and neither does whitespace");
        assert!(!named("pane-1", ""), "a blank directory is nowhere to leave a statement");
        assert!(
            from_parts(Some("pane-1".into()), None).is_none(),
            "a session with nowhere to speak is not the window — a statement would go unheard",
        );
        assert!(
            from_parts(None, Some("/tmp/drop".into())).is_none(),
            "and a directory with no session names nothing the window could file it under",
        );
    }

    #[test]
    fn a_write_carries_the_session_it_was_made_in_and_never_a_guess_at_one() {
        let stamp = |raw: &str| id_from(Some(raw.to_string()));
        assert_eq!(stamp("pane-1").as_deref(), Some("pane-1"), "the window named it: the write carries it");
        assert_eq!(stamp(""), None, "a blank variable names no session");
        assert_eq!(stamp("  "), None, "and neither does whitespace");
        assert_eq!(
            stamp(&"x".repeat(MAX_ID_BYTES + 1)),
            None,
            "past the cap the id is dropped whole — a cut one would name a session nobody has",
        );
        assert_eq!(
            stamp(&"x".repeat(MAX_ID_BYTES)).map(|s| s.len()),
            Some(MAX_ID_BYTES),
            "the cap itself is inside it",
        );
        assert_eq!(id_from(None), None, "outside the window there is nothing to carry");
    }

    #[test]
    fn a_name_tag_asks_for_less_than_a_statement_does() {
        // `surface` needs somewhere to leave a statement; a stamp on a write that was happening anyway
        // needs nothing but the name, so half an environment still names the session it came from.
        assert!(from_parts(Some("pane-1".into()), None).is_none(), "no drop box: not the surface layer");
        assert_eq!(
            id_from(Some("pane-1".into())).as_deref(),
            Some("pane-1"),
            "but the write still knows which pane it was made in",
        );
    }

    #[test]
    fn the_spec_names_every_verb_the_layer_answers_to() {
        let spec = spec();
        let named: Vec<&str> = spec["commands"]
            .as_array()
            .expect("commands is an array")
            .iter()
            .map(|c| c["command"].as_str().unwrap_or_default())
            .collect();
        for verb in ["name", "note", "waiting", "finished"] {
            assert!(
                named.contains(&format!("session {verb}").as_str()),
                "the canon is missing `session {verb}`: {named:?}",
            );
        }
        assert!(
            spec["outside"].as_str().is_some_and(|s| s.contains("non-zero")),
            "the canon says outright that the layer fails outside the window",
        );
    }
}
