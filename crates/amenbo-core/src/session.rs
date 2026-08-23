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
    /// Point at something worth opening — a file, a task, a decision, a URL — and say why.
    Point { target: String, why: String },
}

impl Statement {
    /// The word this statement is filed under, and the one the window branches on. It is the verb the
    /// person typed, so the two never drift.
    pub fn verb(&self) -> &'static str {
        match self {
            Statement::Name(_) => "name",
            Statement::Note(_) => "note",
            Statement::Waiting(_) => "waiting",
            Statement::Finished(_) => "finished",
            Statement::Point { .. } => "point",
        }
    }

    /// The statement's own fields, on top of the ones every statement carries.
    fn body(&self) -> Value {
        match self {
            Statement::Name(text)
            | Statement::Note(text)
            | Statement::Waiting(text)
            | Statement::Finished(text) => json!({ "text": text }),
            Statement::Point { target, why } => json!({ "target": target, "why": why }),
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

/// Every statement left in a directory, oldest first — the window's half of the drop box, and the only
/// reader of [`say`]'s format. A file that is not a statement is skipped rather than raised: this
/// directory is watched while it is being written to, and one unreadable file is no reason to lose the
/// rest.
pub fn statements(dir: &Path) -> Result<Vec<Value>> {
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
        .filter_map(|p| fs::read_to_string(p).ok())
        .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
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
             watching — silence looks the same whether you are building, thinking, or waiting.",
            "Say `finished` when the work is done, and what came of it."
        ],
        "offered": [
            "`name` and `note` label the pane. Say nothing and it is labelled by its folder, which \
             misleads no one.",
            "`point` puts something in the window's own list, for the person to open when they look."
        ],
        "promises": "A statement is information, never a promise. Say what has happened, not what you \
                     will do — a person who believes a promise stops checking, and this layer cannot \
                     make one hold.",
        "commands": [
            { "command": "session name", "args": "<text>", "summary": "Name this pane. The name sticks to the frame, so it survives what runs in it." },
            { "command": "session note", "args": "<text>", "summary": "A line about what you are doing now, shown on the pane's label." },
            { "command": "session waiting", "args": "<text>", "summary": "A person's turn has come. Say why in the same breath — the reason is what they read." },
            { "command": "session finished", "args": "<text>", "summary": "The work is done. Say what came of it." },
            { "command": "session point", "args": "<target> --why <text>", "summary": "Point at a file, a task, a decision or a URL worth opening, and say why it is worth it." }
        ],
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

    #[test]
    fn point_carries_its_target_and_its_reason() {
        let dir = amenbo_scratch::scratch("session-point");
        let path = say(
            &surface_at(&dir),
            &Statement::Point { target: "AMB-T-3592".into(), why: "the vocabulary lands here".into() },
        )
        .expect("the statement is written");

        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).expect("valid JSON");
        assert_eq!(v["verb"], "point");
        assert_eq!(v["target"], "AMB-T-3592");
        assert_eq!(v["why"], "the vocabulary lands here");
    }

    #[test]
    fn statements_come_back_in_the_order_they_were_said() {
        let dir = amenbo_scratch::scratch("session-order");
        let s = surface_at(&dir);
        for verb in ["one", "two", "three"] {
            say(&s, &Statement::Note(verb.to_string())).expect("written");
        }
        let said: Vec<String> = statements(&dir)
            .expect("read back")
            .iter()
            .map(|v| v["text"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(said, vec!["one", "two", "three"], "oldest first, even within one millisecond");
    }

    #[test]
    fn a_half_written_file_is_not_read_and_neither_is_a_directory_nobody_used() {
        let dir = amenbo_scratch::scratch("session-partial");
        let s = surface_at(&dir);
        say(&s, &Statement::Note("real".into())).expect("written");
        // What a composing writer leaves behind: a dotted, extension-less name a reader must skip.
        fs::write(dir.join(".00000000000000000001-1-0000.json.partial"), "{\"verb\":").unwrap();
        let said = statements(&dir).expect("read back");
        assert_eq!(said.len(), 1, "the partial file is not among them: {said:?}");

        assert!(
            statements(&dir.join("never-made")).expect("silence is not a failure").is_empty(),
            "a directory nobody has spoken in reads as empty",
        );
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
    fn the_spec_names_every_verb_the_layer_answers_to() {
        let spec = spec();
        let named: Vec<&str> = spec["commands"]
            .as_array()
            .expect("commands is an array")
            .iter()
            .map(|c| c["command"].as_str().unwrap_or_default())
            .collect();
        for verb in ["name", "note", "waiting", "finished", "point"] {
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
