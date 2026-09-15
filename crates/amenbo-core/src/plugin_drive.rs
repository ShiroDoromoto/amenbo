//! Mounting the plugin dispatcher on the outbox walk, and running what it queued (`AMB-D-367`,
//! `AMB-D-380`, `AMB-D-399`).
//!
//! The walk itself is not here (`AMB-D-884`): [`crate::outbox_drive`] reads the outbox once, past one
//! persisted cursor, for every reader a write is carried out to. This module is the plugins' share of that
//! walk — it hands each row to [`fan_out_row`], then does the two things that must happen *after* the
//! transaction commits: run the replying hooks, and start a runner for every queue that gained work.
//!
//! **Neither face waits for what it set going, and neither has to** (`AMB-T-2175`). A runner is a process of
//! its own ([`crate::plugin_runner`]), so a short-lived CLI command exiting no longer cuts one short, and a
//! long-lived GUI is not holding one open. What each face supplies is only how to launch itself as one
//! ([`RunnerLauncher`]) — the drive is the same on both.
//!
//! **Two mounts, one drive.** [`drive_persisted`] is the write seam's — what just committed goes out behind
//! it. [`resume_persisted`] is the one every face makes as it starts, for what a *previous* run left half
//! delivered (`AMB-D-399`): the write seam cannot answer for that, since the write it would ride may never
//! come. There are two places a run can be cut short, so [`unfinished`] looks at both — a walk that died
//! leaves rows in the outbox, a runner that died leaves rows on a queue.
//!
//! Running the queues sits outside the walk's transaction — a subprocess must not be launched under the
//! write lock, and by then each queue holds everything the run needs.

use crate::error::Result;
use crate::outbox_drive::{walk_persisted, Face, Walked};
use crate::plugin_dispatch::{fan_out_row, Delivered, FannedOut, Subscribers};
use crate::plugin_runner::RunnerLauncher;
use crate::store_engine::StoreEngine;

/// Drive the dispatcher once from the persisted cursor: walk the outbox onto the plugins' queues, and run
/// the queues — the mount both faces use (`AMB-D-380`, `AMB-D-399`).
///
/// The first half rides **one transaction** ([`walk_persisted`]): the fan-out copies each event onto the
/// queue of every plugin that observes it, the outbox rows it copied are reclaimed, and the cursor that says
/// how far it got is stored beside them. Either all three land or none does, so the outbox can never be
/// reclaimed past what was queued, nor queued twice. The second half runs outside it — a subprocess must not
/// be launched under the write lock — and needs nothing from it: each plugin's queue says for itself what it
/// still owes, and the runner reading it holds that plugin's lease, so there is only ever one (`AMB-D-399`).
///
/// `face` selects which subscriptions resolve (`AMB-D-383`) and is stamped beside the cursor for diagnosis.
/// `launcher` is how this face starts a runner process ([`RunnerLauncher`]); `None` launches none at all —
/// the fan-out still happens and its rows wait on the queues, which is what a caller with no executable to
/// re-run behind it (a test of the walk itself) wants. The returned [`Delivered`] names the runners it
/// launched, carries the replies to surface, says whether a retention gap was hit, and carries out every
/// event the walk saw for whoever else observes a write; there is nothing in it to wait for (`AMB-T-2175`).
/// `runs_log` is the execution log every run is recorded in (`AMB-D-361`), `delivery_log` the one a gap
/// lands in.
pub fn drive_persisted(
    engine: &StoreEngine,
    face: Face,
    subs: &dyn Subscribers,
    launcher: Option<&dyn RunnerLauncher>,
    runs_log: Option<&std::path::Path>,
    delivery_log: Option<&std::path::Path>,
) -> Result<Delivered> {
    let (walked, fanned) = fan_out_persisted(engine, face, subs, delivery_log)?;
    // The transaction is closed by now, so the replying hooks can run: they are synchronous, and holding
    // the write lock across a subprocess is exactly what the queue exists to avoid.
    let replies = crate::plugin_dispatch::run_replies(fanned.replies, runs_log);
    let runners = match launcher {
        Some(launcher) => crate::plugin_runner::start(engine, launcher)?,
        None => Vec::new(),
    };
    Ok(Delivered {
        cursor: walked.cursor,
        runners,
        replies,
        gapped: walked.gapped,
        seen: walked.seen,
    })
}

/// What a flush moved: the drive it made, and what each queue it worked got through (`AMB-T-2470`).
///
/// Carries a [`Delivered`], so it inherits that type's one obligation — the `replies` inside it already ran
/// and are somebody's answer to surface.
#[must_use = "surface the replies inside `delivered`"]
pub struct Flushed {
    /// The drive itself — where the walk reached, the replies it gathered, whether a gap was hit. Its
    /// `runners` are the queues this flush worked, since the launcher was the one that works them here.
    pub delivered: Delivered,
    /// One report per queue worked, in the order they were worked. A plugin whose lease was already held by a
    /// live runner is not among them: nothing was taken off its queue here, and saying otherwise would credit
    /// this flush with another runner's work.
    pub worked: Vec<crate::plugin_runner::Worked>,
}

/// Drive the dispatcher and work every queue **to its end, in this process** — the explicit flush
/// (`AMB-T-2470`).
///
/// Delivery otherwise rides along with whatever the user was actually doing: a drive walks the outbox and
/// starts a runner *process* per queue, because the command it rode in on must not be made to wait
/// ([`drive_persisted`]). That leaves nothing anybody can ask for on purpose — a queue with a dead runner
/// behind it waits for the next write, and the only way to push it was to run some unrelated command and hope
/// the startup kick caught it.
///
/// This is that ask, and the difference is the launcher: [`HereRunner`](crate::plugin_runner::HereRunner)
/// works each queue before it returns, so the caller can be told what moved. Everything else is the drive
/// both faces make — same cursor, same walk, same lease, same execution log — and the walk runs
/// unconditionally rather than only when something was left standing ([`resume_persisted`]): a caller asking
/// for a flush is not asking whether one is due.
pub fn flush_persisted(
    engine: &StoreEngine,
    face: Face,
    subs: &dyn Subscribers,
    runs_log: Option<&std::path::Path>,
    delivery_log: Option<&std::path::Path>,
) -> Result<Flushed> {
    let here = crate::plugin_runner::HereRunner::new(engine, subs, runs_log);
    let delivered = drive_persisted(engine, face, subs, Some(&here), runs_log, delivery_log)?;
    Ok(Flushed { delivered, worked: here.worked() })
}

/// Whether a previous run left delivery half-finished — **read-only, and asked of both layers**
/// (`AMB-D-399`).
///
/// There are two places a run can be cut short, so there are two places to look. A walk that died leaves
/// rows in the **outbox** with every queue empty; a runner that died leaves rows on a **queue** with the
/// outbox already reclaimed. Asking only about the queues misses the first kind entirely — nothing is
/// waiting on any queue, so nothing starts, and the events sit there until somebody happens to write.
pub fn unfinished(engine: &StoreEngine) -> Result<bool> {
    Ok(crate::outbox_drive::outbox_unfinished(engine)?
        || !crate::store_engine::queued_plugins(engine.conn())?.is_empty())
}

/// Pick up what a previous run left behind, and only then — the **startup kick** both faces make
/// (`AMB-D-399`).
///
/// Startup is the one moment Amenbo can catch a delivery nobody is going to trigger again: a run cut short
/// leaves its rows standing, and what would move them next is the next *write*, which may be days away or
/// never — a day spent reading only would leave them there. Driving from here covers both layers at once,
/// since a drive walks the outbox and then starts a runner for every queue that has work
/// ([`drive_persisted`]).
///
/// It is guarded rather than unconditional because every face reaches this on every start, including the
/// commands that only read: a drive opens a write transaction, and taking the store's write lock to find
/// nothing to do would put every read behind whatever holds it. [`unfinished`] answers from two reads, and
/// `None` is that answer — nothing was pending, so nothing was driven.
pub fn resume_persisted(
    engine: &StoreEngine,
    face: Face,
    subs: &dyn Subscribers,
    launcher: Option<&dyn RunnerLauncher>,
    runs_log: Option<&std::path::Path>,
    delivery_log: Option<&std::path::Path>,
) -> Result<Option<Delivered>> {
    if !unfinished(engine)? {
        return Ok(None);
    }
    drive_persisted(engine, face, subs, launcher, runs_log, delivery_log).map(Some)
}

/// The transactional half of a drive: walk the outbox onto the plugins' queues, on the walk's own
/// transaction. Returns what the walk moved and what the fan-out put somewhere — the replying hooks
/// included, those being the caller's to run once this has committed.
fn fan_out_persisted(
    engine: &StoreEngine,
    face: Face,
    subs: &dyn Subscribers,
    delivery_log: Option<&std::path::Path>,
) -> Result<(Walked, FannedOut)> {
    let mut fanned = FannedOut::default();
    let walked = walk_persisted(engine, delivery_log, face, |tx, row, event| {
        fan_out_row(tx, row, event, subs, face, &mut fanned)
    })?;
    Ok((walked, fanned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox_drive::{persisted_cursor, CURSOR_FACE_META};
    use crate::plugin_dispatch::{NoSubscribers, Subscriber};
    use crate::plugin_exec::PluginInvocation;
    use crate::store_engine::{outbox::EventRow, StoreEngine};

    /// A resolver that fires one fixed invocation for each of the named events.
    struct Fixed {
        events: Vec<&'static str>,
        invocation: PluginInvocation,
    }
    impl Subscribers for Fixed {
        fn resolve(&self, event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
            if self.events.contains(&event) {
                vec![Subscriber::new("fixed", self.invocation.clone())]
            } else {
                Vec::new()
            }
        }
    }
    fn bogus() -> PluginInvocation {
        PluginInvocation::new("/nonexistent/amenbo-drive-test-plugin")
    }

    /// What is standing on a plugin's queue, taken off as it is read — these tests start no runner, so the
    /// draining a runner would do is done here, and each assertion sees only what its own drive queued.
    fn drained(e: &StoreEngine, plugin: &str) -> Vec<crate::store_engine::QueueRow> {
        let rows = crate::store_engine::queued_for(e.conn(), plugin, 100).unwrap();
        for row in &rows {
            crate::store_engine::dequeue(e.conn(), row.id).unwrap();
        }
        rows
    }

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

    /// A launcher that makes no process and remembers who it was asked for — a start is what these tests
    /// count, and a runner is not theirs to actually run.
    #[derive(Default)]
    struct Launched(std::sync::Mutex<Vec<String>>);
    impl RunnerLauncher for Launched {
        fn launch(&self, plugin: &str, _owner: &str) -> std::io::Result<()> {
            self.0.lock().unwrap().push(plugin.to_string());
            Ok(())
        }
    }

    /// Put one row on a plugin's queue directly — a fan-out's leavings, from before the run that was cut
    /// short.
    fn queue(e: &StoreEngine, plugin: &str, record_id: i64) {
        let tx = e.write().unwrap();
        tx.queue_event(&crate::store_engine::QueuedEvent {
            plugin,
            face: "cli",
            event: "task.created",
            record_id,
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

    /// A store with nothing standing on either layer is not driven at all: the startup kick every face makes
    /// answers from reads and takes no write lock, so a read command pays for it and nothing else.
    #[test]
    fn a_start_with_nothing_pending_drives_nothing() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(!unfinished(&e).unwrap(), "a fresh store left nothing behind");

        let launcher = Launched::default();
        let resumed = resume_persisted(&e, Face::Cli, &NoSubscribers, Some(&launcher), None, None).unwrap();
        assert!(resumed.is_none(), "nothing was pending, so nothing was driven");
        assert_eq!(persisted_cursor(&e).unwrap(), 0, "and no cursor was written");
    }

    /// A fan-out cut short leaves the outbox standing with every queue empty. Looking only at the queues
    /// would find nothing to do and leave those events until somebody happens to write — so the start looks
    /// at the outbox too, and delivers them (`AMB-D-399`).
    #[test]
    fn a_start_resumes_an_outbox_a_previous_fan_out_left_standing() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.deleted", 2);
        assert!(unfinished(&e).unwrap(), "the outbox is holding what was never handed on");

        let subs = Fixed { events: vec!["task.created", "task.deleted"], invocation: bogus() };
        let launcher = Launched::default();
        let resumed = resume_persisted(&e, Face::Cli, &subs, Some(&launcher), None, None).unwrap();

        assert!(resumed.is_some(), "the start drove");
        assert_eq!(drained(&e, "fixed").len(), 2, "both leftover events reached the subscriber's queue");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "and the cursor is stored at the head");
    }

    /// A runner cut short leaves its queue standing with the outbox already reclaimed — the other half of
    /// `AMB-D-399`'s two layers. The start finds it by the queue alone and launches a runner for it.
    #[test]
    fn a_start_resumes_a_queue_a_previous_runner_left_standing() {
        let e = StoreEngine::open_in_memory().unwrap();
        // Deliver everything first, so the outbox is empty and the queue is the only thing left standing.
        emit(&e, "task.created", 1);
        let _ = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None, None).unwrap();
        queue(&e, "stalled", 1);
        assert!(unfinished(&e).unwrap(), "a queue with rows is unfinished delivery");

        let launcher = Launched::default();
        let resumed = resume_persisted(&e, Face::Cli, &NoSubscribers, Some(&launcher), None, None).unwrap();

        assert!(resumed.is_some(), "the start drove");
        assert_eq!(
            launcher.0.lock().unwrap().as_slice(),
            ["stalled"],
            "a runner was launched for the queue nobody was working"
        );
    }

    /// A flush works the queues **before it returns** and says what left each one (`AMB-T-2470`): what an
    /// ordinary drive hands to a process nobody watches is done here instead, which is the whole of what
    /// makes it reportable. A row leaves the queue whether or not its plugin ran — a delivery that failed is
    /// dropped, not retried (`AMB-D-399`) — so the count is what came off the queue, not what succeeded.
    #[test]
    fn a_flush_works_the_queues_here_and_says_what_left_them() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);

        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };
        let flushed = flush_persisted(&e, Face::Cli, &subs, None, None).unwrap();

        assert_eq!(flushed.delivered.runners, ["fixed"], "the queue it worked is the one it took the lease on");
        assert_eq!(flushed.worked.len(), 1, "one report per queue worked");
        assert_eq!(flushed.worked[0].plugin, "fixed");
        assert_eq!(flushed.worked[0].delivered, 2, "both rows came off the queue");
        assert_eq!(flushed.worked[0].left, 0, "and it is empty by the time the flush returns");
        assert!(
            crate::store_engine::queued_plugins(e.conn()).unwrap().is_empty(),
            "nothing is left waiting anywhere"
        );
    }

    /// A queue a live runner already holds is left to it, and reported by nobody: taking it over would put
    /// two runners on one queue, which is what the lease exists to prevent (`AMB-D-399`). The flush is not an
    /// error for it — the runner holding it is the one that will carry the rows out.
    #[test]
    fn a_flush_leaves_a_queue_a_live_runner_is_on() {
        let e = StoreEngine::open_in_memory().unwrap();
        // Deliver first, so the queue below is the only thing standing.
        emit(&e, "task.created", 1);
        let _ = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None, None).unwrap();
        queue(&e, "busy", 1);
        let tx = e.write().unwrap();
        let now = crate::time::Timestamp::now();
        assert!(tx
            .claim_runner("busy", "someone-else", "9999-01-01T00:00:00Z", &now.to_rfc3339_z())
            .unwrap());
        tx.commit().unwrap();

        let flushed = flush_persisted(&e, Face::Cli, &NoSubscribers, None, None).unwrap();

        assert!(flushed.worked.is_empty(), "another runner's work is not this flush's to report");
        assert!(flushed.delivered.runners.is_empty(), "and no lease was taken");
        assert_eq!(drained(&e, "busy").len(), 1, "the row is still there, for the runner that holds it");
    }

    /// The persisted cursor advances across simulated short-lived runs, so a second run does not re-fire the
    /// first run's events — the whole reason the CLI persists it (`AMB-D-367`).
    #[test]
    fn the_cursor_persists_across_runs_so_events_fire_once() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);
        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };

        // First run: both events are queued for the subscriber, and the cursor is persisted at the head.
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None, None).unwrap();
        assert_eq!(drained(&e, "fixed").len(), 2, "both committed events are queued on the first run");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor is stored at the head");

        // A fresh event, then a second short-lived run: only the new event is queued — the first two are
        // behind the persisted cursor.
        emit(&e, "task.created", 3);
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None, None).unwrap();
        assert_eq!(drained(&e, "fixed").len(), 1, "only what committed since the stored cursor is queued");
        assert_eq!(persisted_cursor(&e).unwrap(), 3);
    }

    /// With nothing installed ([`NoSubscribers`]) the drive fires nothing but still walks and persists the
    /// cursor to the head, so a plugin enabled later starts from what fires next, not the whole backlog.
    #[test]
    fn no_subscriber_advances_and_persists_the_cursor() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.status_changed", 2);

        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None, None).unwrap();
        assert!(d.runners.is_empty(), "nobody is installed, so nothing is queued and nobody runs");
        assert!(!d.gapped);
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor still walks to the head and is stored");
    }

    /// A retention gap resyncs the persisted cursor to the head and reports `gapped`, so the lost span is
    /// not replayed and the next run starts clean (`AMB-D-352` / `AMB-D-361`).
    #[test]
    fn a_gap_resyncs_and_persists_the_head() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);
        // Pretend retention trimmed through id 1, and the stored cursor is behind it.
        let tx = e.write().unwrap();
        tx.set_meta(crate::store_engine::outbox::META_OUTBOX_TRUNCATED_THROUGH, Some("1")).unwrap();
        tx.commit().unwrap();

        let d = drive_persisted(&e, Face::Cli, &NoSubscribers, None, None, None).unwrap();
        assert!(d.gapped, "a cursor behind the watermark is a gap");
        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor resyncs to the head and is persisted");
    }

    /// The two faces share the one cursor, so what the GUI delivered the CLI does not deliver again — the
    /// double fire `AMB-D-380` closes. The face beside the cursor names whoever moved it last.
    #[test]
    fn one_cursor_spans_both_faces_so_an_event_fires_once() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };

        // The long-lived face delivers it, as it does in the GUI.
        let _ = drive_persisted(&e, Face::Gui, &subs, None, None, None).unwrap();
        assert_eq!(drained(&e, "fixed").len(), 1, "the GUI's drive queues the event");
        assert_eq!(e.get_meta(CURSOR_FACE_META).unwrap().as_deref(), Some("gui"));

        // A short-lived run after it starts from the same stored cursor, so the event is already past.
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None, None).unwrap();
        assert!(drained(&e, "fixed").is_empty(), "what the other face delivered is not queued a second time");
        assert_eq!(
            e.get_meta(CURSOR_FACE_META).unwrap().as_deref(),
            Some("gui"),
            "a drive that moved nothing does not restamp the face"
        );

        // The next event is queued once, by whichever face gets there first.
        emit(&e, "task.created", 2);
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None, None).unwrap();
        assert_eq!(drained(&e, "fixed").len(), 1);
        assert_eq!(e.get_meta(CURSOR_FACE_META).unwrap().as_deref(), Some("cli"), "the face follows the mover");
    }

    /// The two halves are one atom: what the fan-out queued, what it reclaimed from the outbox, and the
    /// cursor that says how far it got all commit together (`AMB-D-399`). A plugin that has not run yet is
    /// what makes the three visible at once — its queue holds the events, and the outbox is already free of
    /// them.
    #[test]
    fn one_transaction_carries_the_queues_the_reclaim_and_the_cursor() {
        use crate::store_engine::{queued_for, OutboxSlice};
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1);
        emit(&e, "task.created", 2);

        // A resolver that subscribes, with nobody started to run what it queues: the reclaim does not wait
        // on a run, and that is the point.
        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };
        let _ = drive_persisted(&e, Face::Cli, &subs, None, None, None).unwrap();

        assert_eq!(persisted_cursor(&e).unwrap(), 2, "the cursor is stored at the fan-out's position");
        assert_eq!(
            crate::store_engine::events_since(e.conn(), 0, 10).unwrap(),
            OutboxSlice::Gap,
            "the outbox is reclaimed through the fan-out, whatever became of the runs"
        );
        assert_eq!(
            queued_for(e.conn(), "fixed", 10).unwrap().len(),
            2,
            "and what it was reclaimed against is standing on the plugin's queue"
        );
    }
}
