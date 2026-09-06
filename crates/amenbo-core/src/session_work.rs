//! Which talk-window session is holding which task — the **volatile area** beside the store
//! (`AMB-D-758`).
//!
//! This is the other half of [`crate::session`]. That one is what an AI *says* about the session it is
//! in; this is what the session *did*, read back off the record it left while doing it. Both end up on
//! the same pane's label, and the difference between them is the whole of why both exist — a statement
//! is a claim, and a reservation is a fact.
//!
//! **It is read, never inferred.** Every status move made from inside a pane is written here under the
//! session's id ([`crate::session::id`]), so which pane reserved a task is a record rather than a
//! guess — asked of a pane ([`work`], for the label above it) or of a task ([`holder`], for the way
//! back from the ledger to the pane the work is happening in). The one attempt to guess it — from the folder a session was started in and the time it wrote
//! — was right in none of fifteen cases (`AMB-T-3549`).
//!
//! **Nothing here outlives the run that wrote it, and that is the point.** A session id is a throwaway
//! token minted per process ([`crate::session::SESSION_VAR`]) with nothing to resolve it against
//! afterwards, so a permanent record of one is a value that stops meaning anything the moment its
//! window closes. This area is emptied as the window comes up ([`clear`]) and taken away with the pane
//! that made it ([`forget`]), which is why nothing kept for good has to carry one.
//!
//! **Three parties, and none of them does two of the jobs** (`AMB-D-758`):
//!
//! | | |
//! |---|---|
//! | writes | **core**, at the status move, when a session id is there to write ([`record`]) |
//! | reads | **the talk window**, and nothing else ([`work`], [`holder`]) |
//! | empties | **the talk window** — all of it as it starts, one session's as its pane closes |
//!
//! **No judgement of core's reads this.** Reserving, `ready`, `task list` — none of them answers
//! differently for a window being open, and the test of that is a machine that has never started the
//! talk window at all: not one of core's answers changes.
//!
//! **A move made outside a pane is not written at all**, which is not an error case. Somebody's own
//! terminal, an editor that is not the talk window, a person typing at a shell — all of them go through
//! the same path and leave one thing off it.
//!
//! **What that costs is not one missing row — it is a stale answer** (`AMB-D-855`). [`newest`] takes
//! the highest `seq` the area has about a task, so a task whose newest move was made elsewhere is
//! answered for by the older row that is still here: the pane that reserved it yesterday and let it go
//! outside is named as holding it, and `done` is not seen at all. So this is read for a **label** and
//! nothing else. Nothing that writes the ledger may stand on it — a hand-back driven from here would
//! move a task on the strength of a row the world has already passed.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::activity_log::Entry;

/// The volatile area's directory name, beside the store in app-data
/// ([`crate::config::Paths::sessions_dir`]).
pub const DIR_NAME: &str = "sessions";

/// What a session's file is named beyond its id. Only so that a person who opens the directory can see
/// what they are looking at — nothing reads the extension.
const FILE_EXT: &str = "jsonl";

/// How much of one session's file a read may pull in, counted back from its end.
///
/// The area is emptied on every start and every close, so what accumulates is one pane's status moves
/// during one run of the window — some tens of rows in a working day, at the ~50 B a row runs to. The
/// bound is here for the run that is not that, and it is taken **from the end** because only the newest
/// row about a task decides anything. What can be lost off the front is a row something newer has
/// already answered for, or one so old the pane has long since moved on.
const SCAN_BUDGET: u64 = 256 * 1024;

/// What one session has on its hands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
    /// The tasks it reserved and has not ended, newest reservation first. A task it moved to `blocked`
    /// is still among them: the reservation stands, and the pane's label says it has stopped rather
    /// than dropping it (`AMB-D-748` — `blocked` is shown as a fact, never read as a signal).
    pub holding: Vec<i64>,
    /// The tasks it ended, newest first — carried out or decided against. Both are the same event to a
    /// pane's label, which counts what is off this session's hands rather than how it went.
    pub finished: Vec<i64>,
}

/// One row: a status move made inside a pane.
struct Row {
    /// The activity sequence the move was minted under ([`Entry::id`]). It is one counter for the whole
    /// store, so it orders rows across sessions as well as within one — which is what decides who is
    /// holding a task two panes have both had.
    seq: i64,
    task: i64,
    /// The status it moved to, as the ledger's event words it.
    to: String,
}

/// Record one status move, if it was made inside a pane and the entry is one.
///
/// Called by core straight after the ledger line ([`crate::store::Store::add_system_event`]), and
/// infallible for the same reason that append is: this is a label's raw material, not a system of
/// record, so a full disk costs a row on a nameplate and nothing else.
///
/// The session id is read **here, at the write point**, never carried on the entry: an entry is also
/// re-encoded elsewhere, and an id taken at that moment would stamp this session onto somebody else's
/// work.
pub fn record(dir: &Path, entry: &Entry) {
    let Some(session) = crate::session::id() else { return };
    let (Some(task), Some(to)) = (entry.task, moved_to(&entry.event)) else { return };
    let Some(path) = file_of(dir, &session) else { return };
    let mut line = match serde_json::to_vec(&serde_json::json!({ "seq": entry.id, "task": task, "to": to })) {
        Ok(line) => line,
        Err(_) => return,
    };
    line.push(b'\n');
    if let Err(e) = append(&path, &line) {
        tracing::warn!(task, error = %e, "session work: the move was not recorded; a pane's label may be short a row");
    }
}

/// Read what `session` has on its hands.
///
/// **Only the newest move of a task counts, whoever made it** ([`newest`]). A task another pane has
/// since reserved is theirs, however it started here.
pub fn work(dir: &Path, session: &str) -> Work {
    let mut holding: Vec<(i64, i64)> = Vec::new();
    let mut finished: Vec<(i64, i64)> = Vec::new();
    for (task, Move { seq, owner, to }) in newest(dir) {
        if owner != session {
            continue;
        }
        match to.as_str() {
            "in_progress" | "blocked" => holding.push((seq, task)),
            "done" | "rejected" => finished.push((seq, task)),
            // Handing a task back (`→ todo`) is the newest thing this session did to it, and what it
            // says is that the task is nobody's.
            _ => {}
        }
    }
    // Newest first, on the one order the whole area shares.
    holding.sort_unstable_by(|a, b| b.cmp(a));
    finished.sort_unstable_by(|a, b| b.cmp(a));
    Work {
        holding: holding.into_iter().map(|(_, task)| task).collect(),
        finished: finished.into_iter().map(|(_, task)| task).collect(),
    }
}

/// The session holding `task`, or `None` where nothing in the area is holding it.
///
/// It is [`work`] read the other way round, and it answers the one question the ledger cannot: a task
/// says it is `in_progress`, and this says **whose pane** made it so. The same rule decides it — the
/// newest row about the task, whoever wrote it — so a task passed from one pane to another names the
/// pane that has it now.
///
/// `None` is not "nobody is working on it". A move made outside a pane leaves no row at all, and a
/// window that has just started has emptied the area ([`clear`]): both are the area saying nothing,
/// and nothing is what may be said back (`AMB-D-758`).
pub fn holder(dir: &Path, task: i64) -> Option<String> {
    let Move { owner, to, .. } = newest(dir).remove(&task)?;
    matches!(to.as_str(), "in_progress" | "blocked").then_some(owner)
}

/// The newest row about each task the area knows of, whoever wrote it.
///
/// **One counter orders the whole area** ([`Entry::id`]), so rows from different sessions compare
/// directly — which is what decides who is holding a task two panes have both had. A directory that is
/// not there is a window in which nothing has been moved yet.
fn newest(dir: &Path) -> HashMap<i64, Move> {
    let mut newest: HashMap<i64, Move> = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return newest };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(owner) = session_of(&path) else { continue };
        for row in rows(&path) {
            match newest.get(&row.task) {
                Some(held) if held.seq >= row.seq => {}
                _ => {
                    newest.insert(row.task, Move { seq: row.seq, owner: owner.clone(), to: row.to });
                }
            }
        }
    }
    newest
}

/// Empty the whole area. The talk window calls this **as it comes up**, and the emptying is total
/// because at that moment it is true: no session this process opened is running yet, so every row in
/// there was left by a run that has ended.
pub fn clear(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// Take away what one session left, when its terminal closes (`pty://closed`).
///
/// **Only the window can do this.** Whether a session is still running is known to the process holding
/// its pseudo-terminal and to nothing else; the CLI that wrote the rows is a short-lived process that
/// was gone before the question could be asked.
pub fn forget(dir: &Path, session: &str) {
    let Some(path) = file_of(dir, session) else { return };
    let _ = std::fs::remove_file(path);
}

/// The last thing anything did to one task: when it was done, in which pane, and what it was.
struct Move {
    seq: i64,
    /// The session whose file the row was in — the pane the move was made in.
    owner: String,
    to: String,
}

/// The status a ledger event moved a task to, or `None` where the event did not move one.
fn moved_to(event: &Value) -> Option<&str> {
    if event["kind"] != "task.status_changed" {
        return None;
    }
    event["new"].as_str()
}

/// Where `session`'s rows go, or `None` when its id cannot be a file name.
///
/// The id is inherited from an environment anything can set ([`crate::session::SESSION_VAR`]), so it is
/// held to a name a directory entry can be rather than trusted: letters, digits, `-` and `_`. The
/// window mints thirty-two hex characters, so nothing real is turned away — and a value that is not
/// that is refused whole rather than mangled into shape, because a mangled id names a session that does
/// not exist.
fn file_of(dir: &Path, session: &str) -> Option<PathBuf> {
    if session.is_empty() || !session.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return None;
    }
    Some(dir.join(format!("{session}.{FILE_EXT}")))
}

/// Whose file this is, or `None` when it is not one of ours.
fn session_of(path: &Path) -> Option<String> {
    if path.extension()? != FILE_EXT {
        return None;
    }
    Some(path.file_stem()?.to_str()?.to_string())
}

/// Append one row with a single atomic `write`, making the directory on the way if this is the first
/// row of the run.
///
/// The atomicity is the ledger's ([`crate::activity_log`]) and for the same reason: one pane can have
/// several `amenbo` processes in it at once, and `O_APPEND` is what keeps two rows from landing inside
/// each other. Deliberately `write` and never `write_all` — the latter retries a short write as a
/// second syscall, and another process's row can land between the halves.
fn append(path: &Path, line: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new().append(true).create(true).open(path)?;
    let written = file.write(line)?;
    if written < line.len() {
        tracing::warn!(written, len = line.len(), "session work: short write; the row is left truncated");
    }
    Ok(())
}

/// The rows in one session's file, oldest first — the tail of it, and only the whole rows in that tail.
fn rows(path: &Path) -> Vec<Row> {
    tail(path)
        .lines()
        .filter_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            Some(Row {
                seq: v.get("seq")?.as_i64()?,
                task: v.get("task")?.as_i64()?,
                to: v.get("to")?.as_str()?.to_string(),
            })
        })
        .collect()
}

/// The last [`SCAN_BUDGET`] bytes of `path`, cut back to a row boundary. Anything that cannot be read
/// is silence: a file removed by a close that raced this read is a session that is gone, which is what
/// no rows says.
fn tail(path: &Path) -> String {
    let Ok(mut file) = File::open(path) else { return String::new() };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(SCAN_BUDGET);
    if from > 0 && file.seek(SeekFrom::Start(from)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    if from > 0 {
        // The first row in the window is half a row. Drop it rather than parse it.
        match bytes.iter().position(|b| *b == b'\n') {
            Some(i) => bytes.drain(..=i),
            None => bytes.drain(..),
        };
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::activity_log::event;
    use crate::model::ActorKind;
    use crate::time::Timestamp;

    /// One status move, as core hands it over.
    fn moved(id: i64, task: i64, old: &str, new: &str) -> Entry {
        Entry {
            id,
            at: Timestamp::now(),
            actor: Some(ActorKind::Ai),
            project: None,
            task: Some(task),
            decision: None,
            event: event::task_status_changed(old, new),
        }
    }

    /// Write `entry` as `session` — the environment variable core reads is process-wide, so the rows a
    /// test needs from several sessions are written through the same door [`record`] uses, one below
    /// the read of it.
    fn record_as(dir: &Path, session: &str, entry: &Entry) {
        let (task, to) = (entry.task.unwrap(), moved_to(&entry.event).unwrap());
        let path = file_of(dir, session).expect("a test names its sessions plainly");
        let mut line = serde_json::to_vec(&serde_json::json!({ "seq": entry.id, "task": task, "to": to })).unwrap();
        line.push(b'\n');
        append(&path, &line).expect("the scratch directory is writable");
    }

    /// The way back: a task naming the pane that has it. Read off the same rows the label is, so the
    /// two can never disagree about whose a task is.
    #[test]
    fn a_task_names_the_pane_holding_it() {
        let dir = amenbo_scratch::scratch("session-work-holder");
        record_as(&dir, "pane-1", &moved(1, 11, "todo", "in_progress"));
        record_as(&dir, "pane-2", &moved(2, 12, "todo", "in_progress"));
        record_as(&dir, "pane-2", &moved(3, 12, "in_progress", "blocked"));

        assert_eq!(holder(&dir, 11).as_deref(), Some("pane-1"));
        assert_eq!(holder(&dir, 12).as_deref(), Some("pane-2"), "a pane that has stopped still holds it");
        assert_eq!(holder(&dir, 13), None, "a task the area has never heard of");
    }

    /// Ended and handed back are both "no pane has this", and neither may name the pane it was last
    /// touched in: the row is a record of a move, not a claim on the task.
    #[test]
    fn a_task_nobody_is_holding_names_no_pane() {
        let dir = amenbo_scratch::scratch("session-work-holder-ended");
        record_as(&dir, "pane-1", &moved(1, 11, "todo", "in_progress"));
        record_as(&dir, "pane-1", &moved(2, 11, "in_progress", "done"));
        record_as(&dir, "pane-1", &moved(3, 12, "todo", "in_progress"));
        record_as(&dir, "pane-1", &moved(4, 12, "in_progress", "todo"));

        assert_eq!(holder(&dir, 11), None);
        assert_eq!(holder(&dir, 12), None);
    }

    /// And a task two panes have both had names the one that has it now — the same rule the label
    /// reads by, from the other end.
    #[test]
    fn a_task_taken_over_names_the_pane_that_took_it() {
        let dir = amenbo_scratch::scratch("session-work-holder-taken");
        record_as(&dir, "pane-1", &moved(1, 11, "todo", "in_progress"));
        record_as(&dir, "pane-1", &moved(2, 11, "in_progress", "todo"));
        record_as(&dir, "pane-2", &moved(3, 11, "todo", "in_progress"));

        assert_eq!(holder(&dir, 11).as_deref(), Some("pane-2"));
    }

    #[test]
    fn a_session_is_holding_what_it_reserved_and_has_not_ended() {
        let dir = amenbo_scratch::scratch("session-work");
        for entry in [
            moved(1, 11, "todo", "in_progress"),
            moved(2, 12, "todo", "in_progress"),
            moved(3, 12, "in_progress", "done"),
            moved(4, 13, "todo", "in_progress"),
            moved(5, 13, "in_progress", "blocked"),
        ] {
            record_as(&dir, "pane-1", &entry);
        }

        let work = work(&dir, "pane-1");
        assert_eq!(work.holding, vec![13, 11], "newest reservation first, and a stopped one stays");
        assert_eq!(work.finished, vec![12]);
    }

    #[test]
    fn another_sessions_work_is_not_this_ones() {
        let dir = amenbo_scratch::scratch("session-work-others");
        record_as(&dir, "pane-2", &moved(1, 11, "todo", "in_progress"));
        record_as(&dir, "pane-1", &moved(2, 13, "todo", "in_progress"));

        let work = work(&dir, "pane-1");
        assert_eq!(work.holding, vec![13], "pane-2's reservation is pane-2's: {work:?}");
    }

    /// A task taken over by another pane is theirs, however it started here — the newest move is what
    /// says whose it is, and reading only our own rows would have this pane still claiming it.
    #[test]
    fn a_task_another_pane_has_since_reserved_is_no_longer_held_here() {
        let dir = amenbo_scratch::scratch("session-work-taken");
        record_as(&dir, "pane-1", &moved(1, 11, "todo", "in_progress"));
        record_as(&dir, "pane-1", &moved(2, 11, "in_progress", "todo"));
        record_as(&dir, "pane-2", &moved(3, 11, "todo", "in_progress"));

        assert_eq!(work(&dir, "pane-1"), Work::default(), "it is pane-2's now");
        assert_eq!(work(&dir, "pane-2").holding, vec![11]);
    }

    #[test]
    fn a_task_handed_back_is_nobodys() {
        let dir = amenbo_scratch::scratch("session-work-handed-back");
        record_as(&dir, "pane-1", &moved(1, 11, "todo", "in_progress"));
        record_as(&dir, "pane-1", &moved(2, 11, "in_progress", "todo"));

        assert_eq!(work(&dir, "pane-1"), Work::default());
    }

    /// An id reaches core through an environment anything can set, so it is held to a name a directory
    /// entry can be. One that is not is refused whole — a mangled id names a session that does not
    /// exist, and no session at all is the honest answer. Outside a pane there is no id to begin with,
    /// which is the same refusal one step earlier.
    #[test]
    fn a_session_id_that_could_not_be_a_file_name_is_refused() {
        let dir = amenbo_scratch::scratch("session-work-names");
        assert!(file_of(&dir, "").is_none(), "an empty id names no file");
        assert!(file_of(&dir, "../../etc/passwd").is_none(), "and one that would climb out of the area");
        assert!(file_of(&dir, "pane 1").is_none());
        assert!(file_of(&dir, "0f1e2d3c4b5a69788796a5b4c3d2e1f0").is_some(), "what the window mints");
    }

    #[test]
    fn closing_a_pane_takes_away_what_it_held_and_leaves_the_others() {
        let dir = amenbo_scratch::scratch("session-work-forget");
        record_as(&dir, "pane-1", &moved(1, 11, "todo", "in_progress"));
        record_as(&dir, "pane-2", &moved(2, 12, "todo", "in_progress"));

        forget(&dir, "pane-1");

        assert_eq!(work(&dir, "pane-1"), Work::default());
        assert_eq!(work(&dir, "pane-2").holding, vec![12]);
    }

    #[test]
    fn starting_up_empties_the_whole_area() {
        let dir = amenbo_scratch::scratch("session-work-clear");
        record_as(&dir, "pane-1", &moved(1, 11, "todo", "in_progress"));
        record_as(&dir, "pane-2", &moved(2, 12, "todo", "in_progress"));

        clear(&dir);

        assert_eq!(work(&dir, "pane-1"), Work::default());
        assert_eq!(work(&dir, "pane-2"), Work::default());
    }

    #[test]
    fn an_area_nothing_has_been_moved_in_is_silence() {
        let dir = amenbo_scratch::scratch("session-work-empty");
        assert_eq!(work(&dir.join("never-written"), "pane-1"), Work::default());
    }

    /// A row torn by a full disk, or one from a build that wrote something else, costs its own row and
    /// no more — the same contract the ledger's reader holds.
    #[test]
    fn a_row_that_cannot_be_read_costs_only_itself() {
        let dir = amenbo_scratch::scratch("session-work-torn");
        record_as(&dir, "pane-1", &moved(1, 11, "todo", "in_progress"));
        let path = file_of(&dir, "pane-1").unwrap();
        append(&path, b"{\"seq\":2,\"task\":\n").unwrap();
        record_as(&dir, "pane-1", &moved(3, 13, "todo", "in_progress"));

        assert_eq!(work(&dir, "pane-1").holding, vec![13, 11]);
    }
}
