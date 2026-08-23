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
//!
//! The same walk answers the question from the other end ([`adrift`]): which reservations are held by a
//! session that is gone. A pane can only be asked about the work it is doing; a window can be asked
//! about work nothing is doing any more.

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

/// Which of `reserved` are held by a session that is gone.
///
/// A task is adrift when the newest thing that moved it was moved by a session this app started, and
/// that session is no longer among `live`. Nothing else counts as adrift, and the two exclusions are
/// the whole of what keeps this honest:
///
/// - **A line with no session belongs to nobody.** A reservation made at somebody's own terminal has
///   no session id on it ([`crate::activity_log::Line::session`]), and Amenbo cannot see whether that
///   terminal is still open. Calling it adrift would be telling a person their own work had stopped.
/// - **The newest move is the one that counts.** A task somebody else has since taken is theirs,
///   however it started here — the same rule [`work`] walks by.
///
/// `reserved` is what the store says is reserved *now*; this only answers who is holding it. The two
/// are read apart on purpose: the status is a column and the holder is a line, and a walk that decided
/// both would be re-deriving what the store already knows.
///
/// The answer keeps `reserved`'s order — the caller's, which is the store's — because the order tasks
/// are shown in is not this walk's to choose.
pub fn adrift(path: &Path, reserved: &[i64], live: &HashSet<String>) -> Vec<i64> {
    let want: HashSet<i64> = reserved.iter().copied().collect();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut gone: HashSet<i64> = HashSet::new();
    let mut lines = activity_log::rev_lines(path);
    while let Some(line) = lines.next() {
        // Bounded by the file for the same reason `work` is, and stopping early for a better one: once
        // every reserved task has had its newest move read, there is nothing older that can change an
        // answer.
        if lines.read_bytes() > SCAN_BUDGET || seen.len() == want.len() {
            break;
        }
        let (Some(task), Some(_)) = (line.task, moved_to(&line)) else { continue };
        if !want.contains(&task) || !seen.insert(task) {
            continue;
        }
        if let Some(session) = line.session.as_deref() {
            if !live.contains(session) {
                gone.insert(task);
            }
        }
    }
    reserved.iter().copied().filter(|task| gone.contains(task)).collect()
}

/// Which of `proposed` were put up by a session that is gone.
///
/// The decision half of [`adrift`], and it reads the same way: the ledger says who put a decision up
/// and which pane they were in ([`crate::activity_log::event::decision_proposed`]), and this process
/// says which of its sessions are still running. A proposal is adrift when the pane that made it has
/// gone and nobody has settled it since.
///
/// `proposed` is what the store says is still proposed *now*; the walk only answers who put it there.
/// The status is a column and the author is a line, and neither can be read off the other.
///
/// **A proposal made outside a pane is never among them**, for the same reason a reservation made at
/// somebody's own terminal is not: the line carries no session, and Amenbo cannot see that terminal.
///
/// A decision that was accepted and later reopened keeps its one line, because a reopen is not a
/// decision being put up — it is one coming back. So a reopened proposal is asked about under its
/// author's pane, which is what it is: a proposal that is open, put up by a pane that has gone.
pub fn adrift_decisions(path: &Path, proposed: &[i64], live: &HashSet<String>) -> Vec<i64> {
    let want: HashSet<i64> = proposed.iter().copied().collect();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut gone: HashSet<i64> = HashSet::new();
    let mut lines = activity_log::rev_lines(path);
    while let Some(line) = lines.next() {
        if lines.read_bytes() > SCAN_BUDGET || seen.len() == want.len() {
            break;
        }
        if line.event["kind"] != "decision.proposed" {
            continue;
        }
        let Some(decision) = line.decision else { continue };
        if !want.contains(&decision) || !seen.insert(decision) {
            continue;
        }
        if let Some(session) = line.session.as_deref() {
            if !live.contains(session) {
                gone.insert(decision);
            }
        }
    }
    proposed.iter().copied().filter(|id| gone.contains(id)).collect()
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

    /// One decision put up, as the ledger holds it.
    fn proposed(id: i64, decision: i64, session: Option<&str>) -> Entry {
        Entry {
            id,
            at: Timestamp::now(),
            actor: Some(ActorKind::Ai),
            session: session.map(str::to_string),
            project: None,
            task: None,
            decision: Some(decision),
            event: event::decision_proposed("a decision"),
        }
    }

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

    #[test]
    fn a_reservation_whose_session_is_gone_is_adrift_and_one_nobody_named_is_not() {
        let dir = amenbo_scratch::scratch("session-work-adrift");
        let ledger = dir.join("activity.jsonl");
        for entry in [
            moved(1, 11, Some("pane-gone"), "todo", "in_progress"),
            moved(2, 12, Some("pane-live"), "todo", "in_progress"),
            // Reserved at somebody's own terminal: Amenbo cannot see whether that terminal is open.
            moved(3, 13, None, "todo", "in_progress"),
        ] {
            append(&ledger, &entry);
        }

        let live: HashSet<String> = ["pane-live".to_string()].into_iter().collect();
        assert_eq!(adrift(&ledger, &[11, 12, 13], &live), vec![11]);
    }

    #[test]
    fn a_reservation_somebody_still_here_has_taken_over_is_not_adrift() {
        let dir = amenbo_scratch::scratch("session-work-adrift-taken");
        let ledger = dir.join("activity.jsonl");
        for entry in [
            moved(1, 11, Some("pane-gone"), "todo", "in_progress"),
            // Handed back and taken again by a pane that is still here. The newest move is what counts.
            moved(2, 11, Some("pane-gone"), "in_progress", "todo"),
            moved(3, 11, Some("pane-live"), "todo", "in_progress"),
        ] {
            append(&ledger, &entry);
        }

        let live: HashSet<String> = ["pane-live".to_string()].into_iter().collect();
        assert!(adrift(&ledger, &[11], &live).is_empty());
    }

    #[test]
    fn a_proposal_whose_session_is_gone_is_adrift_and_one_nobody_named_is_not() {
        let dir = amenbo_scratch::scratch("session-work-adrift-decisions");
        let ledger = dir.join("activity.jsonl");
        for entry in [
            proposed(1, 21, Some("pane-gone")),
            proposed(2, 22, Some("pane-live")),
            // Put up at somebody's own terminal: Amenbo cannot see whether that terminal is open.
            proposed(3, 23, None),
        ] {
            append(&ledger, &entry);
        }

        let live: HashSet<String> = ["pane-live".to_string()].into_iter().collect();
        assert_eq!(adrift_decisions(&ledger, &[21, 22, 23], &live), vec![21]);
    }

    #[test]
    fn a_reservation_and_a_proposal_do_not_answer_for_each_other() {
        let dir = amenbo_scratch::scratch("session-work-adrift-kinds");
        let ledger = dir.join("activity.jsonl");
        // The same number on both sides: task 11 and decision 11 are different records, and a walk
        // that read the id without the kind would answer for the wrong one.
        append(&ledger, &moved(1, 11, Some("pane-gone"), "todo", "in_progress"));
        append(&ledger, &proposed(2, 11, Some("pane-live")));

        let live: HashSet<String> = ["pane-live".to_string()].into_iter().collect();
        assert_eq!(adrift(&ledger, &[11], &live), vec![11], "the reservation is the gone pane's");
        assert!(adrift_decisions(&ledger, &[11], &live).is_empty(), "the proposal is not");
    }

    #[test]
    fn a_task_the_ledger_says_nothing_about_is_not_adrift() {
        let dir = amenbo_scratch::scratch("session-work-adrift-silent");
        let ledger = dir.join("activity.jsonl");
        append(&ledger, &moved(1, 11, Some("pane-gone"), "todo", "in_progress"));

        // 12 was never moved through a pane — reserved before the ledger carried sessions, say. Silence
        // is not evidence that nothing is holding it.
        assert_eq!(adrift(&ledger, &[11, 12], &HashSet::new()), vec![11]);
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
