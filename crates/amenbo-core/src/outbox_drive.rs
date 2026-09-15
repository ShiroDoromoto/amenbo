//! **Driving the outbox at the write seam** — the one walk everything that observes a write is fed from
//! (`AMB-D-367`, `AMB-D-380`, `AMB-D-399`).
//!
//! An ops write point appends a semantic event to the [outbox](crate::store_engine::outbox) inside its own
//! transaction, so generation is leak-free. Getting those rows to whoever cares is this module's, and it is
//! a **single walk with a single cursor**: [`walk`] drains everything past the cursor, hands each row to the
//! caller's observer, reclaims what it read, and says how far it got.
//!
//! **One walk, because a second reader cannot be added for free.** The reclaim rides the same transaction
//! as the walk, so a reader that came back afterwards for the same rows would find them gone; giving it a
//! cursor of its own would mean the reclaim could only advance to the older of the two, and the outbox would
//! then be held open by whichever reader ran least often. So the walk is made once and what it saw is
//! carried out on [`Walked::seen`], for the caller to hand on. Today that is a notification
//! ([`crate::notify_dispatch`]) and, while the mechanism lasts, a plugin's queue
//! ([`crate::plugin_dispatch`]); neither is named here.
//!
//! **One cursor, because an event must reach a reader exactly once across both faces** (`AMB-D-380`). A
//! session cursor held in the GUI's memory left the events it delivered still standing in the outbox, so the
//! next CLI run carried them a second time — and what a notification does leaves the machine, which makes a
//! double send a bug the user sees. There is no per-face start position either: the persisted cursor is the
//! single answer to "how far has this store been walked", and whatever sits past it is uncarried, whichever
//! face got there first.
//!
//! **Two mounts, one walk.** [`walk_persisted`] is what the write seam makes — what just committed goes out
//! behind it. The other is the one every face makes as it starts, for what a *previous* run left half
//! carried (`AMB-D-399`): the write seam cannot answer for that, since the write it would ride may never
//! come. Both go through the same cursor, and the advance is **contention-tolerant** — the cursor is re-read
//! under the write lock and only ever moves forward, so a face that loses the race leaves the winner's
//! position standing and picks up from it next time.
//!
//! **The walk rides one transaction of its own**, separate from the mutation that triggered it: the drive
//! never enlarges that operation, and the cursor is not rewritten when it did not move, so a read command
//! that drives finds nothing and writes nothing. What each observer *does* with what it was handed — start a
//! process, post to a relay — belongs outside that transaction, because holding the write lock across a
//! subprocess is exactly what a queue exists to avoid.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::notify_dispatch::Happened;
use crate::store_engine::outbox::{events_since, outbox_head, trim_fanned_out, OutboxRow, OutboxSlice};
use crate::store_engine::{StoreEngine, WriteTx};

/// How many events one [`walk`] call drains per page. A drive made after each write sees one event at a
/// time; this only bounds a catch-up drain after downtime, so it is generous — the page cost is one query.
const PAGE: i64 = 256;

/// The `store_meta` key the walk's cursor is persisted under — the id of the last outbox event a drive
/// carried, shared by both faces (`AMB-D-380`). Distinct from the outbox's retention watermark
/// ([`META_OUTBOX_TRUNCATED_THROUGH`](crate::store_engine::outbox) — that is the producer's low-water mark;
/// this is the consumer's high-water mark) and from the change feed's own cursor (a different consumer,
/// `AMB-D-367`). A store that has never persisted one carries no row, which reads back as `0`.
///
/// The spelling is the one already in every store, from when the only reader was the plugin dispatcher.
/// Renaming it would be a migration for a name and nothing else, so it stays as written.
pub const CURSOR_META: &str = "plugin_dispatch_cursor";

/// The `store_meta` key holding which [`Face`] last advanced [`CURSOR_META`] — written beside the cursor,
/// on the same transaction, so the two never disagree. It is **diagnostic only**: the cursor's meaning does
/// not depend on it, and nothing branches on it. When a double send or a miss is being chased, it is what
/// says which face carried a span, to line the store up against this machine's logs (`AMB-D-361`). Its
/// spelling is historical for the reason [`CURSOR_META`]'s is.
pub const CURSOR_FACE_META: &str = "plugin_dispatch_cursor_face";

/// **Which face is driving** (`AMB-D-383`) — the short-lived CLI a person or their AI runs, or the
/// long-lived GUI.
///
/// It is stamped beside the cursor it advanced ([`CURSOR_FACE_META`]), and a plugin's subscription declares
/// the faces it fires on ([`EventSubscription::faces`](crate::plugin_manifest::EventSubscription::faces))
/// out of the same vocabulary — one type, so the two cannot drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    /// The command face — the CLI a person or their AI runs, and the one face a `reply` hook may fire on,
    /// since it is the only one with a caller waiting to read the reply (`AMB-D-383`).
    Cli,
    /// The long-lived GUI.
    Gui,
}

impl Face {
    /// The wire token, matching how the face is spelled in a manifest and beside the cursor.
    pub fn as_str(self) -> &'static str {
        match self {
            Face::Cli => "cli",
            Face::Gui => "gui",
        }
    }

    /// The inverse of [`as_str`](Face::as_str) — for reading a face back out of a plain string that never
    /// went through serde, which is how it is stored beside the cursor (`AMB-D-380`). A token outside the
    /// vocabulary is `None`: a stamp this build cannot read is no answer, not a wrong one.
    pub fn parse(s: &str) -> Option<Face> {
        match s {
            "cli" => Some(Face::Cli),
            "gui" => Some(Face::Gui),
            _ => None,
        }
    }
}

/// What one walk moved: how far it read, whether it hit a retention gap, and every event it passed.
#[must_use = "carry `seen` out to whatever observes a write"]
pub struct Walked {
    /// The cursor to store for the next pass — the id of the last outbox event walked, or the outbox head
    /// when a gap forced a resync. Equal to the cursor passed in when nothing was there to read.
    pub cursor: i64,
    /// Retention had trimmed past the cursor: the span between it and the head is lost and reached nobody.
    /// The cursor is resynced to the head. Carrying is best-effort, so this is not an error (`AMB-D-352`).
    pub gapped: bool,
    /// **Every event this walk passed**, recognised ones only, in the order they fired. The outbox is
    /// reclaimed on the same transaction, so this is the only copy there will be — whoever the caller hands
    /// it to is the caller's business, and nothing here knows who that is. Empty on a retention gap.
    pub seen: Vec<Happened>,
}

/// Read the persisted cursor — the id of the last event a previous run carried, or `0` when none was ever
/// stored (a fresh store, starting from the bottom of the outbox). A value that does not parse is treated
/// the same as absent: the honest floor is `0`, and a resync on the first drive costs at most a gap the
/// outbox already reports.
pub fn persisted_cursor(engine: &StoreEngine) -> Result<i64> {
    Ok(engine.get_meta(CURSOR_META)?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Read the face that last advanced the cursor ([`CURSOR_FACE_META`]), or `None` when nothing has advanced
/// it — a store that has never carried anything, or one last driven by a build that did not stamp the face.
/// This is the diagnostic half of the pair: it says who moved the cursor to where it now stands, never
/// whose turn is next (`AMB-D-380` — both faces drive, and nothing branches on this). An unreadable token
/// reads as `None`, the same honest floor an unparsable cursor gets.
pub fn persisted_cursor_face(engine: &StoreEngine) -> Result<Option<Face>> {
    Ok(engine.get_meta(CURSOR_FACE_META)?.as_deref().and_then(Face::parse))
}

/// Whether a previous run left the outbox standing — **read-only**, and the question a startup asks before
/// taking the write lock (`AMB-D-399`).
///
/// An outbox row *is* leftover, without consulting the cursor: the walk reclaims what it read on the same
/// transaction, so anything still standing there was never carried out.
pub fn outbox_unfinished(engine: &StoreEngine) -> Result<bool> {
    Ok(outbox_head(engine.conn())? > 0)
}

/// **Walk the outbox** past `cursor`, on the caller's transaction.
///
/// Every recognised row is handed to `observe` and carried out on [`Walked::seen`]; then everything through
/// the cursor is reclaimed, on this same transaction, so a copy and the reclaim of what it copied commit
/// together. A row this Amenbo does not recognise — an event outside [`V1_EVENTS`](crate::lifecycle) — is
/// warned about and skipped, and the cursor still walks past it: the alternative is an outbox that never
/// drains because of one row nobody can read.
///
/// On a retention gap the cursor is resynced to the head and nothing is handed on for the lost span (see
/// [`Walked::gapped`]). `log` is the delivery log (`AMB-D-361`), used here for the one thing this step alone
/// knows: a gap leaves no other trace, the events being gone. `None` records nothing, which is what a test
/// of the walk itself wants.
///
/// An `observe` that fails fails the whole walk, and with it the transaction: an observer is writing on the
/// caller's transaction, so a half-written row must not be committed beside a cursor that says it was
/// carried.
pub fn walk(
    tx: &WriteTx<'_>,
    cursor: i64,
    log: Option<&Path>,
    mut observe: impl FnMut(&WriteTx<'_>, &OutboxRow, &'static str) -> Result<()>,
) -> Result<Walked> {
    let conn = tx.conn();
    let mut cursor = cursor;
    let mut seen: Vec<Happened> = Vec::new();
    loop {
        match events_since(conn, cursor, PAGE)? {
            OutboxSlice::Gap => {
                // Retention passed the cursor; the lost events cannot be replayed. Resync to the head and
                // hand nothing on for the gap — carrying is best-effort (`AMB-D-352`). A gap can only
                // surface on the first page (the cursor only ever moves forward), so nothing has been
                // observed yet. It is recorded here rather than left to the caller: this is where the fact
                // is known, and a silently dropped span is precisely what the log exists to make visible
                // (`AMB-D-361`).
                if let Some(path) = log {
                    crate::delivery_log::record_gap(path);
                }
                return Ok(Walked { cursor: outbox_head(conn)?, gapped: true, seen: Vec::new() });
            }
            OutboxSlice::Events { rows, more } => {
                for row in &rows {
                    cursor = row.id;
                    let Some(event) = crate::lifecycle::recognised(&row.event) else {
                        tracing::warn!(
                            event = %row.event,
                            id = row.record_id,
                            "unrecognised outbox event; skipped"
                        );
                        continue;
                    };
                    // Which project the event happened in, read off the row (`AMB-D-405`) — the emit door
                    // stamped it, so nothing is looked up here. That is what makes a deletion routable at
                    // all: the record it names is gone by now, and a task that has moved since would
                    // otherwise route its older events to its new home. `None` is a real answer (a record
                    // in no project, or a row from before the column).
                    // Carried before `observe` is called, because it is not the observer's answer: an event
                    // nobody subscribes to is still an event a project may report.
                    seen.push(Happened {
                        event: row.event.clone(),
                        record_id: row.record_id,
                        project: row.project,
                        actor: row.actor.clone(),
                        new_state: row.new_state.clone(),
                        parent: row.parent,
                    });
                    observe(tx, row, event)?;
                }
                if !more {
                    break;
                }
            }
        }
    }
    // Everything through `cursor` has been handed to everyone who observes it, so the outbox is free of it
    // — on this same transaction, so the walk and the reclaim are one atom. This is the whole of what
    // `AMB-D-399` moves off an observer's critical path: the outbox is reclaimed at the walk's speed, not at
    // the slowest reader's.
    trim_fanned_out(conn, cursor)?;
    Ok(Walked { cursor, gapped: false, seen })
}

/// **Walk once from the persisted cursor**, on a transaction of its own — the mount both faces make
/// (`AMB-D-380`, `AMB-D-399`).
///
/// The walk, the outbox reclaim it authorises and the cursor that records it commit together: either all
/// three land or none does, so the outbox can never be reclaimed past what was observed, nor observed twice.
///
/// The cursor is re-read **inside** the transaction, whose `BEGIN IMMEDIATE` holds the write lock from the
/// start: the other face may have walked the same span while this one was assembling, so a cursor that is
/// not ahead of what is already stored is not written (the walk itself found nothing in that case — it read
/// past the same trimmed rows). The cursor never goes backwards, which is what keeps an event from being
/// carried twice (`AMB-D-380`).
///
/// `face` is stamped beside the cursor for diagnosis ([`CURSOR_FACE_META`]) and handed to `observe`, which
/// is where it decides anything. Whatever the caller does with [`Walked::seen`] belongs **after** this
/// returns: the transaction is closed by then, and starting a process under the write lock is what the
/// whole shape exists to avoid.
pub fn walk_persisted(
    engine: &StoreEngine,
    log: Option<&Path>,
    face: Face,
    observe: impl FnMut(&WriteTx<'_>, &OutboxRow, &'static str) -> Result<()>,
) -> Result<Walked> {
    let cursor = persisted_cursor(engine)?;
    let tx = engine.write()?;
    let walked = walk(&tx, cursor, log, observe)?;
    if walked.cursor > persisted_cursor(engine)? {
        tx.set_meta(CURSOR_META, Some(&walked.cursor.to_string()))?;
        tx.set_meta(CURSOR_FACE_META, Some(face.as_str()))?;
    }
    tx.commit()?;
    Ok(walked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::outbox::EventRow;

    fn emit(e: &StoreEngine, event: &str, id: i64) {
        let tx = e.write().unwrap();
        tx.emit_event(&EventRow {
            event,
            record_id: id,
            actor: "ai",
            at: "2026-07-23T09:00:00Z",
            new_state: None,
            project: None,
            record: None,
            parent: None,
        })
        .unwrap();
        tx.commit().unwrap();
    }

    /// Walk from the persisted cursor with nobody observing — what every face does on a store where nothing
    /// is subscribed and no project reports.
    fn drive(e: &StoreEngine, face: Face) -> Walked {
        walk_persisted(e, None, face, |_, _, _| Ok(())).unwrap()
    }

    /// A store that never drove carries no cursor, read back as `0`.
    #[test]
    fn an_unpersisted_cursor_reads_as_zero() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert_eq!(persisted_cursor(&e).unwrap(), 0);
    }

    /// The walk hands every recognised row out on `seen`, in the order they fired, and reclaims what it
    /// read — the copy and the reclaim being one atom is the whole reason there is only one walk.
    #[test]
    fn a_walk_carries_what_it_saw_out_and_reclaims_it() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.done", 2);

        let walked = drive(&e, Face::Cli);
        assert_eq!(walked.cursor, 2, "the cursor walked to the head");
        assert!(!walked.gapped);
        let names: Vec<&str> = walked.seen.iter().map(|h| h.event.as_str()).collect();
        assert_eq!(names, ["task.created", "task.done"], "both events, oldest first");
        assert_eq!(
            events_since(e.conn(), 0, 10).unwrap(),
            OutboxSlice::Gap,
            "everything walked is gone from the outbox"
        );
    }

    /// Each row is offered to the observer beside being carried out on `seen`, with the catalog name the
    /// walk recognised it by — so an observer never parses the event string a second time.
    #[test]
    fn the_observer_is_offered_every_row_the_walk_carries() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "comment.added", 2);

        let mut offered: Vec<(&'static str, i64)> = Vec::new();
        let walked =
            walk_persisted(&e, None, Face::Cli, |_, row, event| {
                offered.push((event, row.record_id));
                Ok(())
            })
            .unwrap();

        assert_eq!(offered, [("task.created", 1), ("comment.added", 2)]);
        assert_eq!(walked.seen.len(), 2, "and the same rows went out on `seen`");
    }

    /// A row this build does not recognise is warned about and skipped — reaching neither the observer nor
    /// `seen` — and the cursor still walks past it, so one unreadable row cannot stall the outbox.
    #[test]
    fn an_unrecognised_row_is_skipped_and_the_cursor_still_walks() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.levitated", 2);

        let mut offered = 0usize;
        let walked = walk_persisted(&e, None, Face::Cli, |_, _, _| {
            offered += 1;
            Ok(())
        })
        .unwrap();

        assert_eq!(offered, 1, "only the row the catalog knows");
        assert_eq!(walked.seen.len(), 1);
        assert_eq!(walked.cursor, 2, "and the cursor is past the one it could not read");
    }

    /// The persisted cursor advances across simulated short-lived runs, so a second run does not carry the
    /// first run's events again — the whole reason the cursor is stored (`AMB-D-367`).
    #[test]
    fn the_cursor_persists_across_runs_so_events_are_carried_once() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);

        assert_eq!(drive(&e, Face::Cli).seen.len(), 2, "both committed events on the first run");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor is stored at the head");

        emit(&e, "task.created", 3);
        assert_eq!(drive(&e, Face::Cli).seen.len(), 1, "only what committed since the stored cursor");
        assert_eq!(persisted_cursor(&e).unwrap(), 3);
    }

    /// The two faces share the one cursor, so what the GUI carried the CLI does not carry again — the
    /// double send `AMB-D-380` closes. The face beside the cursor names whoever moved it last, and a pass
    /// that moved nothing does not restamp it.
    #[test]
    fn one_cursor_spans_both_faces_so_an_event_is_carried_once() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);

        assert_eq!(drive(&e, Face::Gui).seen.len(), 1, "the long-lived face carries it");
        assert_eq!(persisted_cursor_face(&e).unwrap(), Some(Face::Gui));

        assert!(drive(&e, Face::Cli).seen.is_empty(), "the other face finds it already past");
        assert_eq!(
            persisted_cursor_face(&e).unwrap(),
            Some(Face::Gui),
            "a pass that moved nothing does not restamp the face"
        );

        emit(&e, "task.created", 2);
        assert_eq!(drive(&e, Face::Cli).seen.len(), 1);
        assert_eq!(persisted_cursor_face(&e).unwrap(), Some(Face::Cli), "the face follows the mover");
    }

    /// A face from a build that knows a face this one does not: unreadable, so unanswered.
    #[test]
    fn a_face_this_build_cannot_read_is_no_answer() {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.write().unwrap();
        tx.set_meta(CURSOR_FACE_META, Some("daemon")).unwrap();
        tx.commit().unwrap();
        assert_eq!(persisted_cursor_face(&e).unwrap(), None);
    }

    /// The stored cursor only ever moves forward: a pass that comes back with an older one — the other face
    /// having walked the same span while this was running — stores nothing and leaves the winner's position
    /// standing (`AMB-D-380`). Carrying an event twice is exactly what this refuses.
    #[test]
    fn the_stored_cursor_never_moves_backwards() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);

        let _ = drive(&e, Face::Gui);
        assert_eq!(persisted_cursor(&e).unwrap(), 2);

        // The loser comes back from the cursor it read before the race: the outbox is empty behind the
        // winner's reclaim, so it finds nothing and does not write its own position back.
        let tx = e.write().unwrap();
        let walked = walk(&tx, 0, None, |_, _, _| Ok(())).unwrap();
        tx.commit().unwrap();
        assert_eq!(walked.cursor, 0, "it read nothing, so it is still where it started");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the winner's position stands");
    }

    /// A drive with nothing new leaves the stored cursor untouched — no needless `store_meta` write on a
    /// read command that happens to drive.
    #[test]
    fn a_no_op_drive_does_not_rewrite_the_cursor() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        let _ = drive(&e, Face::Cli);
        assert_eq!(persisted_cursor(&e).unwrap(), 1);

        assert!(drive(&e, Face::Cli).seen.is_empty());
        assert_eq!(persisted_cursor(&e).unwrap(), 1, "an empty drive does not move the cursor");
    }

    /// A retention gap resyncs the cursor to the head, reports `gapped`, and writes the one line that says
    /// a span went uncarried — the events themselves being gone (`AMB-D-352` / `AMB-D-361`).
    #[test]
    fn a_gap_resyncs_persists_the_head_and_is_logged() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);
        // Pretend retention trimmed through id 1, and the stored cursor is behind it.
        let tx = e.write().unwrap();
        tx.set_meta(crate::store_engine::outbox::META_OUTBOX_TRUNCATED_THROUGH, Some("1")).unwrap();
        tx.commit().unwrap();

        let log = amenbo_scratch::scratch("outbox-drive-gap").join(crate::delivery_log::FILE_NAME);
        let _ = std::fs::remove_file(&log);
        let walked = walk_persisted(&e, Some(&log), Face::Cli, |_, _, _| Ok(())).unwrap();

        assert!(walked.gapped, "a cursor behind the watermark is a gap");
        assert!(walked.seen.is_empty(), "nothing is carried for a span that is gone");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor resyncs to the head and is persisted");
        let lines = crate::delivery_log::read(&log);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].outcome, crate::delivery_log::Outcome::Gap);
    }

    /// A store with nothing standing is not driven at all: the question a startup asks answers from one
    /// read and takes no write lock, so a read command pays for that and nothing else.
    #[test]
    fn an_empty_outbox_is_nothing_left_over() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(!outbox_unfinished(&e).unwrap(), "a fresh store left nothing behind");
        emit(&e, "task.created", 1);
        assert!(outbox_unfinished(&e).unwrap(), "the outbox is holding what was never carried out");
        let _ = drive(&e, Face::Cli);
        assert!(!outbox_unfinished(&e).unwrap(), "and nothing once the walk has reclaimed it");
    }
}
