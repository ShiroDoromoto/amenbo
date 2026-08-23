//! What the ledger says one talk-window session has been doing: the tasks it reserved and has not
//! ended, and the ones it ended.
//!
//! This is the other half of [`crate::session`]. That one is what an AI *says* about the session it is
//! in; this is what the session *did*, read back off the record it left while doing it. Both end up on
//! the same pane's label, and the difference between them is the whole of why both exist — a statement
//! is a claim, and a reservation is a fact the ledger already holds.
//!
//! **It is read, never inferred.** Every write from inside a pane carries the session's id
//! ([`crate::activity_log::Line::session`]), so which pane reserved a task is a column rather than a
//! guess. The one attempt to guess it — from the folder a session was started in and the time it wrote
//! — was right in none of fifteen cases (`AMB-T-3549`), and a line with no session reads as unknown and
//! belongs to nobody.
//!
//! **Only the newest move of a task counts, whoever made it.** A task somebody else has since reserved
//! is theirs, however it started here, so the walk records the first move it meets for each task and
//! ignores everything older about it.

use std::collections::HashSet;
use std::path::Path;

use crate::activity_log::{self, Line};

/// How much of the ledger one read may pull in before it stops asking.
///
/// The walk cannot bound itself by what it finds — a session that reserved nothing has nothing to stop
/// on — so it is bounded by the file instead. At the ~200 B a line runs to this is some thousands of
/// lines, which is far more than a session's own work sits within: the answer is about what *this*
/// session is doing now, and a session whose reservations are that far back in a shared ledger has not
/// been the one writing.
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

/// Read what `session` has been doing off the ledger at `path`.
///
/// A ledger that is not there is a store nothing has happened in yet, which is silence and not a
/// failure — the same as a session that has written nothing.
pub fn work(path: &Path, session: &str) -> Work {
    let mut seen: HashSet<i64> = HashSet::new();
    let mut work = Work::default();
    let mut lines = activity_log::rev_lines(path);
    while let Some(line) = lines.next() {
        if lines.read_bytes() > SCAN_BUDGET {
            break;
        }
        let (Some(task), Some(moved_to)) = (line.task, moved_to(&line)) else { continue };
        if !seen.insert(task) {
            continue;
        }
        if line.session.as_deref() != Some(session) {
            continue;
        }
        match moved_to {
            "in_progress" | "blocked" => work.holding.push(task),
            "done" | "rejected" => work.finished.push(task),
            // Handing a task back (`→ todo`) is the newest thing this session did to it, and what it
            // says is that the task is nobody's.
            _ => {}
        }
    }
    work
}

/// The status a line moved a task to, or `None` where the line did not move one.
fn moved_to(line: &Line) -> Option<&str> {
    if line.event["kind"] != "task.status_changed" {
        return None;
    }
    line.event["new"].as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::activity_log::{append, event, Entry};
    use crate::model::ActorKind;
    use crate::time::Timestamp;

    /// One status move, as the ledger holds it.
    fn moved(id: i64, task: i64, session: Option<&str>, old: &str, new: &str) -> Entry {
        Entry {
            id,
            at: Timestamp::now(),
            actor: Some(ActorKind::Ai),
            session: session.map(str::to_string),
            project: None,
            task: Some(task),
            decision: None,
            event: event::task_status_changed(old, new),
        }
    }

    #[test]
    fn a_session_is_holding_what_it_reserved_and_has_not_ended() {
        let dir = amenbo_scratch::scratch("session-work");
        let ledger = dir.join("activity.jsonl");
        for entry in [
            moved(1, 11, Some("pane-1"), "todo", "in_progress"),
            moved(2, 12, Some("pane-1"), "todo", "in_progress"),
            moved(3, 12, Some("pane-1"), "in_progress", "done"),
            moved(4, 13, Some("pane-1"), "todo", "in_progress"),
            moved(5, 13, Some("pane-1"), "in_progress", "blocked"),
        ] {
            append(&ledger, &entry);
        }

        let work = work(&ledger, "pane-1");
        assert_eq!(work.holding, vec![13, 11], "newest reservation first, and a stopped one stays");
        assert_eq!(work.finished, vec![12]);
    }

    #[test]
    fn another_sessions_work_is_not_this_ones_and_neither_is_an_unnamed_write() {
        let dir = amenbo_scratch::scratch("session-work-others");
        let ledger = dir.join("activity.jsonl");
        for entry in [
            moved(1, 11, Some("pane-2"), "todo", "in_progress"),
            moved(2, 12, None, "todo", "in_progress"),
            moved(3, 13, Some("pane-1"), "todo", "in_progress"),
        ] {
            append(&ledger, &entry);
        }

        let work = work(&ledger, "pane-1");
        assert_eq!(work.holding, vec![13], "a line with no session belongs to nobody: {work:?}");
    }

    /// A task taken over by somebody else is theirs, however it started here — the newest move is what
    /// says whose it is, and reading only our own lines would have this pane still claiming it.
    #[test]
    fn a_task_somebody_else_has_since_reserved_is_no_longer_held_here() {
        let dir = amenbo_scratch::scratch("session-work-taken");
        let ledger = dir.join("activity.jsonl");
        for entry in [
            moved(1, 11, Some("pane-1"), "todo", "in_progress"),
            moved(2, 11, Some("pane-1"), "in_progress", "todo"),
            moved(3, 11, Some("pane-2"), "todo", "in_progress"),
        ] {
            append(&ledger, &entry);
        }

        assert_eq!(work(&ledger, "pane-1"), Work::default(), "it is pane-2's now");
        assert_eq!(work(&ledger, "pane-2").holding, vec![11]);
    }

    #[test]
    fn a_ledger_nothing_has_happened_in_is_silence() {
        let dir = amenbo_scratch::scratch("session-work-empty");
        assert_eq!(work(&dir.join("never-written.jsonl"), "pane-1"), Work::default());
    }
}
