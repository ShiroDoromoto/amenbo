//! The **queue runners** — one per plugin, at most one at a time, each working its own queue from the head
//! (`AMB-D-399`).
//!
//! The fan-out ([`plugin_dispatch::fan_out`](crate::plugin_dispatch::fan_out)) left one row per event per
//! subscribed plugin. This is what turns those rows into runs, and the shape it takes is what the decision
//! bought:
//!
//! - **One runner per plugin, not one process per event.** Deleting a project can emit thousands of events;
//!   under the old shape that was thousands of subprocesses at once, all of them competing and most of them
//!   killed. Here each plugin has one runner working its own rows one at a time, so a plugin sees its events
//!   in the order they happened and the machine sees one child per plugin.
//! - **The lease is the whole of "at most one".** A runner starts only by claiming its plugin's lease
//!   ([`store_engine::runner`](crate::store_engine::runner)) on the transaction that looks for it, and it
//!   leaves only by releasing it on the transaction that finds its queue empty. Both sides pass through one
//!   write lock, so there is no gap between "nothing left" and "nobody running" for an event to fall into:
//!   whichever lands first, the other sees the state it left.
//! - **Nothing is killed for being slow.** A runner has nobody behind it but the rest of its own plugin's
//!   queue, so a hook is waited on to its end ([`plugin_hooks::run_queued`](crate::plugin_hooks::run_queued))
//!   rather than cut off at five seconds — being cut off mid-work is exactly the half-done outside effect
//!   this layer exists to stop. What a plugin that never returns holds is its own queue, and only until its
//!   lease's horizon hands it to the next runner.
//!
//! **A runner outlives the drive that started it, so it works on its own store.** The drive is a moment on
//! the caller's connection — a CLI command about to exit, a GUI action already answered — and a runner may
//! still be running long after. It therefore opens what it needs itself, through the [`RunnerEnv`] the face
//! hands it, rather than borrowing the connection it was started from. That is also what makes the resolver
//! a runner reads *its own*: who is enabled is read again in the runner, not carried over from the drive.
//!
//! **A row leaves the queue once its plugin has replied, and the reply is the child returning**
//! (`AMB-D-399`). A plugin that ran to its own end has answered for that event, and the row goes whichever
//! end it reached — a clean exit and a failing one both mean the plugin had it, and a failed event is
//! dropped rather than retried (`AMB-D-352`). A hook that would not launch, and a row that resolves to
//! nobody, go the same way: nothing is coming to answer for those, and a row held back would block the ones
//! behind it for good.
//!
//! What that leaves standing is the one case worth keeping — **nothing answered at all**. A runner killed
//! with its process, or a machine that lost power, never reaches the transaction that removes the row, so
//! the row is still there for the next runner rather than lost with the process that was carrying it.
//!
//! The price is the window between the child returning and that transaction committing: a crash inside it
//! re-delivers an event that already ran. There is no acknowledgement a plugin could write to close it,
//! because amenbo cannot see what the other side did with the event either way (`AMB-D-399`) — which is why
//! the contract asks a plugin to be safe to run twice, rather than asking amenbo to be sure.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::config::Paths;
use crate::error::Result;
use crate::plugin_dispatch::{hook_for, Subscribers};
use crate::store_engine::{queued_for, queued_plugins, StoreEngine};
use crate::time::Timestamp;

/// How many rows one pass reads from a plugin's queue at a time. A page is one query, and the runner keeps
/// reading until its queue is empty, so this bounds memory and nothing else.
const RUN_PAGE: i64 = 256;

/// How far ahead a runner holds its lease, pushed out again before every row it takes.
///
/// It is the answer to one question only: **how long after a runner dies is its queue stuck?** Too short and
/// a plugin that legitimately takes a while gets its queue taken over while it works (harmless — the worst
/// of it is one event delivered twice, which the contract already admits — but pointless); too long and a
/// machine that lost power leaves that plugin unrun for that long after it comes back. A minute is generous
/// against the first and barely noticeable against the second, since a store is nearly always driven again
/// within it.
///
/// The horizon moves **between rows**, not while one is being run: the wait for a plugin's child is a single
/// blocking call, and pushing the lease out during it would take a second connection and a thread of its
/// own. So a plugin whose *one* run outlasts this may have its queue taken over mid-run — the double
/// delivery the contract already admits, but systematically rather than rarely, for a plugin that slow.
pub const LEASE_TTL: Duration = Duration::from_secs(60);

/// What a runner thread works with: **its own** store, and a resolver over it.
///
/// A runner outlives the drive that started it, so it cannot borrow that drive's connection or its resolver
/// (see the note at the top). This is the seam it opens them through: the face's implementation opens the
/// device's store and builds the enabled-subscriber resolver over it, hands both to `f`, and closes them
/// when `f` returns. [`device_env`] is that implementation; a test supplies its own.
pub trait RunnerEnv: Send + Sync + 'static {
    /// Open a store and a resolver over it, and hand both to `f`. A failure to open is the implementation's
    /// to report — `f` is simply not called, and the runner's lease then expires on its own.
    fn with_store(&self, f: &mut dyn FnMut(&StoreEngine, &dyn Subscribers));
}

/// The [`RunnerEnv`] a real face hands its runners: this device's store at `paths`, with the enabled
/// subscribers resolved over the plugins installed on it. Both are opened inside the runner thread, so what
/// it reads is the state at the time it runs — a plugin disabled since the drive is not fired.
pub fn device_env(paths: Paths) -> impl RunnerEnv {
    DeviceEnv { paths }
}

struct DeviceEnv {
    paths: Paths,
}

impl RunnerEnv for DeviceEnv {
    fn with_store(&self, f: &mut dyn FnMut(&StoreEngine, &dyn Subscribers)) {
        let store = match crate::Store::open_at(self.paths.clone()) {
            Ok(store) => store,
            Err(e) => {
                tracing::warn!(error = %e, "a plugin runner could not open the store; its queue waits");
                return;
            }
        };
        // A directory that will not read is not "nothing is installed": run nothing rather than drop a
        // plugin's queue on the floor. The rows stay, and the next drive starts a runner again.
        let installed = match crate::plugin_installed::installed(&store.paths) {
            Ok(installed) => installed,
            Err(e) => {
                tracing::warn!(error = %e, "a plugin runner could not read the installed plugins; its queue waits");
                return;
            }
        };
        let subs = crate::plugin_subscribe::EnabledSubscribers::new(&installed, &store);
        f(store.read_model(), &subs);
    }
}

/// Start a runner for every plugin with work waiting that nobody is already running (`AMB-D-399`).
///
/// Called after the fan-out committed, on the drive's own connection: for each plugin named on the queue
/// table, the lease is claimed on a transaction of its own, and a thread is started only for the plugins
/// whose lease this drive took. A plugin already being run is left alone — the runner holding it will see
/// the rows this drive queued before it leaves, because the transaction it leaves on reads the same queue.
///
/// `finished` is signalled once per runner as it ends, which is how a short-lived caller waits for the work
/// it started **without** waiting on it for ever ([`Delivered::wait_for_runners`](crate::plugin_dispatch::Delivered::wait_for_runners)).
/// The returned handles are the runner threads: a long-lived face drops them, and nothing is cut short.
pub fn start(
    engine: &StoreEngine,
    env: std::sync::Arc<dyn RunnerEnv>,
    log: Option<&Path>,
    finished: &Sender<()>,
) -> Result<Vec<JoinHandle<()>>> {
    let mut started = Vec::new();
    for plugin in queued_plugins(engine.conn())? {
        let owner = new_owner();
        let now = Timestamp::now();
        let tx = engine.write()?;
        let claimed = tx.claim_runner(&plugin, &owner, &horizon(now), &now.to_rfc3339_z())?;
        tx.commit()?;
        if !claimed {
            continue;
        }
        let (env, log, finished) = (env.clone(), log.map(Path::to_path_buf), finished.clone());
        started.push(std::thread::spawn(move || {
            env.with_store(&mut |engine, subs| run_queue(engine, subs, &plugin, &owner, log.as_deref()));
            let _ = finished.send(());
        }));
    }
    Ok(started)
}

/// Work one plugin's queue to its end, holding `owner`'s lease throughout — the runner's whole body.
///
/// Rows are worked one at a time, oldest first, each bracketed by two transactions of its own: [`hold`]
/// before the plugin runs, and [`settle`] once it has returned. Both push the lease's horizon out and both
/// stop the runner when the lease is no longer its — the first so a runner already taken over does not fire
/// an event that is its successor's, the second because taking a row off a queue that is no longer this
/// runner's would be removing its successor's work. The loop ends where the queue does: [`leave`] re-reads
/// it under the write lock and releases the lease only if it is still empty, so an event queued a moment
/// earlier is either seen by that read (and this runner stays for it) or lands after the release (and the
/// drive that queued it starts the next runner).
///
/// A store error stops the runner where it stands, without releasing: warning is all there is to do, and the
/// lease's horizon is what brings the queue back rather than a lease held by a runner that is no longer
/// there. Losing the lease — taken over past its horizon — stops it the same way, and deliberately without a
/// release: the row it would give up belongs to its successor.
pub fn run_queue(
    engine: &StoreEngine,
    subs: &dyn Subscribers,
    plugin: &str,
    owner: &str,
    log: Option<&Path>,
) {
    loop {
        let rows = match queued_for(engine.conn(), plugin, RUN_PAGE) {
            Ok(rows) => rows,
            Err(e) => return warn_stop(plugin, &e.to_string()),
        };
        for row in &rows {
            match hold(engine, plugin, owner) {
                Ok(true) => {}
                Ok(false) => return taken_over(plugin),
                Err(e) => return warn_stop(plugin, &e.to_string()),
            }
            match hook_for(engine.conn(), subs, row) {
                Ok(Some(hook)) => crate::plugin_hooks::run_queued(&hook, log),
                Ok(None) => {}
                Err(e) => tracing::warn!(
                    plugin = %plugin,
                    event = %row.event,
                    error = %e,
                    "a queued event could not be turned into a run; dropped"
                ),
            }
            match settle(engine, plugin, owner, row.id) {
                Ok(true) => {}
                Ok(false) => return taken_over(plugin),
                Err(e) => return warn_stop(plugin, &e.to_string()),
            }
        }
        match leave(engine, plugin, owner) {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => return warn_stop(plugin, &e.to_string()),
        }
    }
}

/// Push the lease out before a row is run, and say whether it is still this runner's to push. `false` means
/// the queue has been taken over past this runner's horizon: the row is its successor's to run, and firing
/// it here would be a delivery nobody asked for on top of the one the successor is making.
fn hold(engine: &StoreEngine, plugin: &str, owner: &str) -> Result<bool> {
    let tx = engine.write()?;
    if !tx.extend_runner(plugin, owner, &horizon(Timestamp::now()))? {
        return Ok(false);
    }
    tx.commit()?;
    Ok(true)
}

/// Take one row off the queue now that its plugin has replied, and push the lease out, on one transaction —
/// the reply being the child having returned, whichever end it reached (`AMB-D-399`). `false` when the lease
/// is no longer this runner's: the row then stays, because removing it would be answering for a run its
/// successor has not made yet.
fn settle(engine: &StoreEngine, plugin: &str, owner: &str, row: i64) -> Result<bool> {
    let tx = engine.write()?;
    if !tx.extend_runner(plugin, owner, &horizon(Timestamp::now()))? {
        return Ok(false);
    }
    tx.dequeue_event(row)?;
    tx.commit()?;
    Ok(true)
}

/// Leave if there is nothing left to run: re-read the queue **under the write lock** and release the lease
/// only when it is still empty. `true` means this runner is done — either it released, or the lease it would
/// have released is already someone else's.
fn leave(engine: &StoreEngine, plugin: &str, owner: &str) -> Result<bool> {
    let tx = engine.write()?;
    if !queued_for(tx.conn(), plugin, 1)?.is_empty() {
        return Ok(false); // dropped unread: the transaction rolls back and the next pass reads the row
    }
    let released = tx.release_runner(plugin, owner)?;
    tx.commit()?;
    if !released {
        tracing::debug!(plugin = %plugin, "the lease this runner would have given up is already someone else's");
    }
    Ok(true)
}

/// A runner stopping because its queue is somebody else's now. Nothing is released: the lease it would give
/// up is the successor's, and so is the row it was on.
fn taken_over(plugin: &str) {
    tracing::debug!(plugin = %plugin, "this runner's lease was taken over; stopping");
}

/// A runner stopping on a store it cannot read or write. Nothing is released — the lease's horizon is what
/// brings this queue back, and it does so without anyone having to be alive to say so.
fn warn_stop(plugin: &str, error: &str) {
    tracing::warn!(plugin = %plugin, error = %error, "a plugin runner stopped on a store error; its queue waits");
}

/// The instant a lease taken now runs out at.
fn horizon(now: Timestamp) -> String {
    Timestamp(now.0 + chrono::Duration::from_std(LEASE_TTL).unwrap_or_default()).to_rfc3339_z()
}

/// A token no other runner on this device carries: the process, a count within it, and when it started. It
/// is what makes a takeover safe — a runner whose lease was taken over finds the row is no longer its own,
/// so it neither extends nor releases what it lost. Legible on purpose: it is also what a diagnosis reads
/// off the lease to say *who* is holding a queue.
fn new_owner() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        Timestamp::now().to_rfc3339_z()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_dispatch::Subscriber;
    use crate::plugin_exec::PluginInvocation;
    use crate::plugin_manifest::Face;
    use crate::store_engine::{QueuedEvent, StoreEngine};
    use std::sync::Mutex;

    /// A resolver that answers for one plugin and counts how many rows it was asked about — which, for a
    /// runner, is how many rows were run.
    struct Recording {
        plugin: &'static str,
        asked: Mutex<usize>,
    }
    impl Recording {
        fn new(plugin: &'static str) -> Self {
            Self { plugin, asked: Mutex::new(0) }
        }
    }
    impl Subscribers for Recording {
        fn resolve(&self, _event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
            // A program that does not exist: the run fails, which is a warning and nothing more, and leaves
            // the loop's behaviour — the part under test — exactly as a successful one would.
            vec![Subscriber::new(self.plugin, PluginInvocation::new("/nonexistent/amenbo-runner-test"))]
        }
        fn resolve_one(
            &self,
            plugin: &str,
            event: &str,
            project: Option<i64>,
            face: Face,
        ) -> Option<Subscriber> {
            *self.asked.lock().unwrap() += 1;
            self.resolve(event, project, face).into_iter().find(|s| s.plugin == plugin)
        }
    }

    fn queue(e: &StoreEngine, plugin: &str, record_id: i64) {
        let tx = e.write().unwrap();
        tx.queue_event(&QueuedEvent {
            plugin,
            face: "cli",
            event: "task.created",
            record_id,
            actor: "ai",
            at: "2026-07-25T09:00:00Z",
            new_state: None,
        })
        .unwrap();
        tx.commit().unwrap();
    }

    fn claim(e: &StoreEngine, plugin: &str, owner: &str) {
        let tx = e.write().unwrap();
        let now = Timestamp::now();
        assert!(tx.claim_runner(plugin, owner, &horizon(now), &now.to_rfc3339_z()).unwrap());
        tx.commit().unwrap();
    }

    /// A resolver that looks at the queue at the moment it is asked *how do I run this row* — after the
    /// runner has taken the row up, and before the plugin runs. What it records is how many rows were
    /// standing then, which is where the reply-shaped dequeue shows itself: the row being run is still one
    /// of them, because nothing has replied for it yet (`AMB-D-399`).
    struct Peeking<'a> {
        engine: &'a StoreEngine,
        plugin: &'static str,
        queued_while_running: Mutex<Vec<usize>>,
    }
    impl Subscribers for Peeking<'_> {
        fn resolve(&self, _event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
            vec![Subscriber::new(self.plugin, PluginInvocation::new("/nonexistent/amenbo-runner-test"))]
        }
        fn resolve_one(
            &self,
            plugin: &str,
            event: &str,
            project: Option<i64>,
            face: Face,
        ) -> Option<Subscriber> {
            let standing = queued_for(self.engine.conn(), plugin, 10).unwrap().len();
            self.queued_while_running.lock().unwrap().push(standing);
            self.resolve(event, project, face).into_iter().find(|s| s.plugin == plugin)
        }
    }

    /// A resolver that hands the queue to a successor while the runner is being asked how to run its row —
    /// a takeover past the horizon landing in the middle of a row rather than between two.
    struct Stealing<'a> {
        engine: &'a StoreEngine,
        from: &'static str,
    }
    impl Subscribers for Stealing<'_> {
        fn resolve(&self, _event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
            vec![Subscriber::new("slack", PluginInvocation::new("/nonexistent/amenbo-runner-test"))]
        }
        fn resolve_one(
            &self,
            plugin: &str,
            event: &str,
            project: Option<i64>,
            face: Face,
        ) -> Option<Subscriber> {
            let tx = self.engine.write().unwrap();
            tx.release_runner(plugin, self.from).unwrap();
            let now = Timestamp::now();
            assert!(tx.claim_runner(plugin, "successor", &horizon(now), &now.to_rfc3339_z()).unwrap());
            tx.commit().unwrap();
            self.resolve(event, project, face).into_iter().find(|s| s.plugin == plugin)
        }
    }

    /// The runner's whole shape in one: it takes its plugin's rows oldest first, and leaves by giving the
    /// lease up on the transaction that finds the queue empty. The program these rows resolve to does not
    /// exist, so every one of them failed — and every one of them still left the queue, which is the
    /// contract (`AMB-D-399`): a failed event is dropped, never retried.
    #[test]
    fn a_runner_works_its_queue_from_the_head_and_gives_the_lease_up() {
        let e = StoreEngine::open_in_memory().unwrap();
        for id in 1..=3 {
            queue(&e, "slack", id);
        }
        queue(&e, "mail", 9); // another plugin's queue is not this runner's to touch
        claim(&e, "slack", "mine");

        let subs = Recording::new("slack");
        run_queue(&e, &subs, "slack", "mine", None);

        assert_eq!(*subs.asked.lock().unwrap(), 3, "every one of its own rows was run");
        assert!(queued_for(e.conn(), "slack", 10).unwrap().is_empty(), "its own queue is worked to the end");
        assert_eq!(queued_for(e.conn(), "mail", 10).unwrap().len(), 1, "and nobody else's is touched");
        assert_eq!(
            crate::store_engine::lease_of(e.conn(), "slack").unwrap(),
            None,
            "and the lease is given up, so the next drive can start a runner"
        );
    }

    /// A runner whose lease was taken over past its horizon stops where it stands — it takes no further row
    /// (the rows belong to its successor now) and releases nothing.
    #[test]
    fn a_runner_that_lost_its_lease_takes_nothing_more() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", 1);
        claim(&e, "slack", "successor");

        let subs = Recording::new("slack");
        run_queue(&e, &subs, "slack", "the-one-taken-over", None);

        assert_eq!(queued_for(e.conn(), "slack", 10).unwrap().len(), 1, "the row is left for its holder");
        assert_eq!(*subs.asked.lock().unwrap(), 0, "and nothing was run");
        assert_eq!(
            crate::store_engine::lease_of(e.conn(), "slack").unwrap().unwrap().owner,
            "successor",
            "the successor's lease stands"
        );
    }

    /// A row is still on its queue while its plugin runs, and leaves only once the plugin has returned
    /// (`AMB-D-399`). Two rows make both halves visible at once: the first is asked about with both
    /// standing, and the second with one — its own — because the first has been answered for by then.
    #[test]
    fn a_row_stays_on_the_queue_until_its_plugin_has_replied() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", 1);
        queue(&e, "slack", 2);
        claim(&e, "slack", "mine");

        let subs =
            Peeking { engine: &e, plugin: "slack", queued_while_running: Mutex::new(Vec::new()) };
        run_queue(&e, &subs, "slack", "mine", None);

        assert_eq!(
            *subs.queued_while_running.lock().unwrap(),
            vec![2, 1],
            "each row is still queued while its own plugin runs"
        );
        assert!(
            queued_for(e.conn(), "slack", 10).unwrap().is_empty(),
            "and each is gone once that run returned"
        );
    }

    /// A runner taken over *while a row is running* leaves that row where it is: answering for it would be
    /// answering for a run its successor has not made. The event is delivered twice as a result — once here
    /// and once by the successor — which is the double delivery the contract admits (`AMB-D-399`).
    #[test]
    fn a_runner_taken_over_mid_row_leaves_that_row_for_its_successor() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", 1);
        queue(&e, "slack", 2);
        claim(&e, "slack", "mine");

        run_queue(&e, &Stealing { engine: &e, from: "mine" }, "slack", "mine", None);

        assert_eq!(
            queued_for(e.conn(), "slack", 10).unwrap().len(),
            2,
            "the row it was on is not its to answer for, and neither is the one behind it"
        );
        assert_eq!(
            crate::store_engine::lease_of(e.conn(), "slack").unwrap().unwrap().owner,
            "successor",
            "and the successor's lease is left standing"
        );
    }

    /// A row nobody can run leaves the queue all the same, so it cannot block the rows behind it — the
    /// contract is best-effort delivery, and a row held back would be a retry nobody asked for.
    #[test]
    fn a_row_that_resolves_to_nothing_still_leaves_the_queue() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "gone", 1);
        claim(&e, "gone", "mine");

        run_queue(&e, &crate::plugin_dispatch::NoSubscribers, "gone", "mine", None);
        assert!(queued_for(e.conn(), "gone", 10).unwrap().is_empty());
        assert_eq!(crate::store_engine::lease_of(e.conn(), "gone").unwrap(), None);
    }

    /// An env that opens nothing: the runner thread it is handed to ends at once, which is all these tests
    /// need — what a runner does once it *has* a store is [`run_queue`]'s, tested above.
    struct NoEnv;
    impl RunnerEnv for NoEnv {
        fn with_store(&self, _f: &mut dyn FnMut(&StoreEngine, &dyn Subscribers)) {}
    }

    /// Starting is gated by the lease, which is the whole of "one runner per plugin": the first drive takes
    /// it and starts a runner, and a drive while it stands starts nobody.
    #[test]
    fn a_drive_starts_a_runner_only_for_a_queue_nobody_is_running() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", 1);
        let (tx, _rx) = std::sync::mpsc::channel();
        let env = std::sync::Arc::new(NoEnv);

        let first = start(&e, env.clone(), None, &tx).unwrap();
        assert_eq!(first.len(), 1, "the queue was unheld, so a runner starts");

        // The lease taken above is still standing (this env's runner never reached `leave`), so a second
        // drive over the same queue leaves the work to whoever holds it.
        queue(&e, "slack", 2);
        let second = start(&e, env, None, &tx).unwrap();
        assert!(second.is_empty(), "a plugin already being run is left alone");
        for h in first {
            h.join().unwrap();
        }
    }
}
