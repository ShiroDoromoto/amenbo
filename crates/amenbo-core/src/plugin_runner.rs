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
//! - **Every run is told how much is behind it** ([`QUEUE_REMAINING_VAR`], `AMB-D-417`). A plugin sees one
//!   event per launch and has no way to know that forty-nine more are already queued for it, so the one
//!   thing standing between a project deletion and fifty messages is a number only the runner can give.
//!   Delivery itself is unchanged — nothing is held back, and whether to batch on the number is entirely
//!   the plugin's call.
//! - **Nothing is killed for being slow.** A runner has nobody behind it but the rest of its own plugin's
//!   queue, so a hook is waited on to its end ([`plugin_hooks::run_queued`](crate::plugin_hooks::run_queued))
//!   rather than cut off at five seconds — being cut off mid-work is exactly the half-done outside effect
//!   this layer exists to stop. The lease is pushed out **during** that wait as well as between rows
//!   ([`BEAT`], `AMB-T-2174`), so being slow is not mistaken for being gone: what a plugin that never
//!   returns holds is its own queue, and it holds it for as long as its runner is alive to say so.
//!
//! **A runner is a process of its own, and the drive does not wait for it** (`AMB-T-2175`). A runner started
//! as a thread of the driving process was only ever as long-lived as that process: a CLI command that
//! returned took the runner's pipes with it, so the plugin holding them died of `SIGPIPE` mid-work, with the
//! row already answered for and nothing in the log to say what happened — the half-done outside effect this
//! layer exists to stop, arriving by the back door. So a runner is launched as a **separate process**
//! ([`RunnerLauncher`]), and the drive returns the moment it is launched. Nothing has to be waited for,
//! because nothing the parent does can cut the runner short any more.
//!
//! It is not a daemon (`AMB-D-399` keeps that): it is started only when there is a queue to work and a lease
//! to take, and it ends when its own queue is empty. What it is, is **this same executable, re-run** — every
//! face already ships as one binary, so a runner needs no second one, and the entry point it re-runs itself
//! through is the face's own ([`SelfRunner`]).
//!
//! Because it is a process, it opens what it needs itself — its store, and the resolver over it
//! ([`run_process`]) — rather than borrowing the connection it was started from. That is also what makes the
//! resolver a runner reads *its own*: who is enabled is read again in the runner, not carried over from the
//! drive.
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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::config::Paths;
use crate::error::Result;
use crate::plugin_dispatch::{hook_for, Subscribers};
use crate::store_engine::{
    backlog, lease_of, queued_count, queued_for, queued_plugins, Lease, QueueDepth, StoreEngine,
};
use crate::time::Timestamp;

/// How many rows one pass reads from a plugin's queue at a time. A page is one query, and the runner keeps
/// reading until its queue is empty, so this bounds memory and nothing else.
const RUN_PAGE: i64 = 256;

/// The environment variable a plugin is told **how many events are still behind this one** under
/// (`AMB-D-417`).
///
/// A plugin is run once per event and cannot see its own queue, so batching — one message for the fifty
/// events a project deletion emitted, rather than fifty — is something only the runner can make possible.
/// It does not do the batching: amenbo delivers as fast as it can, in order (`AMB-D-399`), and what a
/// plugin does with the number is the plugin's business. It only says what it knows.
///
/// **`0` means the queue is empty as of this launch, not that nothing more is coming.** An event queued a
/// moment later is delivered like any other, so a plugin that flushes on `0` may end up sending twice —
/// two messages instead of one, never a message lost.
pub const QUEUE_REMAINING_VAR: &str = "AMENBO_PLUGIN_QUEUE_REMAINING";

/// How far ahead a runner holds its lease, pushed out again before every row it takes and while one runs.
///
/// It is the answer to one question only: **how long after a runner dies is its queue stuck?** Too short and
/// a queue is taken over on account of a runner that is merely quiet; too long and a machine that lost power
/// leaves that plugin unrun for that long after it comes back. A minute is generous against the first and
/// barely noticeable against the second, since a store is nearly always driven again within it.
///
/// It is *only* that question because a live runner keeps saying so ([`BEAT`]). Being slow and being gone
/// are different things, and the horizon is asked to tell apart only the second.
pub const LEASE_TTL: Duration = Duration::from_secs(60);

/// How often a runner pushes its lease out **while a plugin is still running** (`AMB-T-2174`).
///
/// Without it the horizon moves only between rows, so a plugin whose *one* run outlasts [`LEASE_TTL`] has
/// its queue taken over mid-run — not the rare double delivery the contract admits (`AMB-D-399`) but one on
/// every single run, for exactly the plugins whose work is worth not doing twice. A runner is doing nothing
/// but waiting on its child, so it beats from that wait
/// ([`wait_watched`](crate::plugin_exec::RunningPlugin::wait_watched)) on the connection it already holds —
/// no second connection, no thread.
///
/// A third of the horizon: two beats may be lost to a machine that stalls, or to a `beat` that finds the
/// store's write lock held, before anyone concludes this runner is gone.
const BEAT: Duration = Duration::from_secs(LEASE_TTL.as_secs() / 3);

/// How a face launches a runner — a **process**, and one it never waits for (`AMB-T-2175`).
///
/// The drive's whole part is to launch it: the lease is already taken by then, and what the runner does with
/// the queue behind it is its own business, outliving whatever started it. So this seam answers one question
/// and returns — *start a runner for `plugin`, holding `owner`'s lease*. [`SelfRunner`] is what a real face
/// hands over; a test hands its own, because a test wants the launch counted, not made.
///
/// An `Err` means no runner exists: the caller gives the lease straight back, so the queue is not held for
/// its horizon by a process that never started ([`start`]).
pub trait RunnerLauncher {
    /// Launch a runner for `plugin` under `owner`'s lease, and return without waiting for it.
    fn launch(&self, plugin: &str, owner: &str) -> std::io::Result<()>;
}

/// The launcher a real face hands a drive: **this same executable, re-run** as a runner process
/// (`AMB-T-2175`).
///
/// Every face ships as a single binary — the CLI is one, and so is the app — so a runner needs no second
/// one. What differs between faces is only how each names its own runner entry point, which is what `argv`
/// carries: whatever the face puts there is followed by the three things a runner needs, in this order —
/// **the plugin, the lease's owner, and the store's base directory**. The store is named rather than
/// resolved because a runner must work the store its parent drove, not whichever one its own environment
/// would resolve to.
///
/// The child is launched with **no stdio at all**. Nothing reads a runner's own output: a plugin's output is
/// captured per run into the execution log (`AMB-D-361`), which is where a diagnosis looks, and a pipe held
/// open to a parent that is about to exit is exactly what made a runner a thread's problem in the first
/// place.
pub struct SelfRunner {
    argv: Vec<String>,
    base_dir: PathBuf,
}

impl SelfRunner {
    /// A launcher that re-runs this executable through `argv` — the face's own runner entry point — over the
    /// store at `base_dir`.
    pub fn new(argv: &[&str], base_dir: PathBuf) -> Self {
        Self { argv: argv.iter().map(|a| (*a).to_string()).collect(), base_dir }
    }
}

impl RunnerLauncher for SelfRunner {
    fn launch(&self, plugin: &str, owner: &str) -> std::io::Result<()> {
        use std::process::Stdio;
        let child = std::process::Command::new(std::env::current_exe()?)
            .args(&self.argv)
            .arg(plugin)
            .arg(owner)
            .arg(&self.base_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        reap(child);
        Ok(())
    }
}

/// Collect `child` on a thread of its own — a launcher's only reason to hold a thread at all.
///
/// A parent that never waits leaves a zombie behind on Unix for as long as *it* lives, and a long-lived face
/// drives on every write, so they would pile up. This waits instead of the caller: it blocks in `waitpid` and
/// nothing else, holds no store and no lock, and if the parent exits first (the short-lived face's ordinary
/// case) it goes with it and the runner is reparented, still running. What it is emphatically not is a wait
/// the *drive* makes — that is the whole of what `AMB-T-2175` removes.
fn reap(mut child: std::process::Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

/// Work one plugin's queue **in this process**, having been launched as its runner (`AMB-T-2175`) — the body
/// behind each face's runner entry point.
///
/// It opens the store at `base_dir` and resolves the enabled subscribers over the plugins installed beside
/// it, both here rather than in the drive that launched this process: what a runner reads is the state at the
/// time it runs, so a plugin disabled since the fan-out is not fired. `owner` is the lease the launching
/// drive took on this runner's behalf — it is held, extended and given up under that name
/// ([`run_queue`]), which is what makes the launch and the work one runner rather than two.
///
/// A store or a plugins directory that will not read ends the runner without touching the queue: the rows
/// stay, the lease expires on its own horizon, and the next drive starts a runner again. Nothing here is
/// reported to a caller — there is none — so the one trace is the execution log every run lands in
/// (`AMB-D-361`).
pub fn run_process(base_dir: PathBuf, plugin: &str, owner: &str) {
    let paths = Paths::at(base_dir);
    let log = paths.plugin_log_file();
    let store = match crate::Store::open_at(paths) {
        Ok(store) => store,
        Err(e) => {
            tracing::warn!(error = %e, "a plugin runner could not open the store; its queue waits");
            return;
        }
    };
    // A directory that will not read is not "nothing is installed": run nothing rather than drop a plugin's
    // queue on the floor. The rows stay, and the next drive starts a runner again.
    let installed = match crate::plugin_installed::installed(&store.paths) {
        Ok(installed) => installed,
        Err(e) => {
            tracing::warn!(error = %e, "a plugin runner could not read the installed plugins; its queue waits");
            return;
        }
    };
    let subs = crate::plugin_subscribe::EnabledSubscribers::new(&installed, &store);
    run_queue(store.read_model(), &subs, plugin, owner, Some(&log));
}

/// Start a runner for every plugin with work waiting that nobody is already running (`AMB-D-399`), and
/// return the plugins whose runner this drive launched.
///
/// Called after the fan-out committed, on the drive's own connection: for each plugin named on the queue
/// table, the lease is claimed on a transaction of its own, and a runner is launched only for the plugins
/// whose lease this drive took. A plugin already being run is left alone — the runner holding it will see
/// the rows this drive queued before it leaves, because the transaction it leaves on reads the same queue.
///
/// The launch is a process ([`RunnerLauncher`]) and there is nothing to wait for, so what comes back is the
/// names and not a handle: a caller can say which queues it set going, and can do nothing else to them. A
/// launch that fails gives the lease straight back — waiting out its horizon would hold that queue for a
/// minute on account of a runner that never existed.
pub fn start(engine: &StoreEngine, launcher: &dyn RunnerLauncher) -> Result<Vec<String>> {
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
        if let Err(e) = launcher.launch(&plugin, &owner) {
            tracing::warn!(plugin = %plugin, error = %e, "a plugin runner would not start; its queue waits");
            if let Err(e) = give_back(engine, &plugin, &owner) {
                tracing::warn!(plugin = %plugin, error = %e, "and its lease could not be given back");
            }
            continue;
        }
        started.push(plugin);
    }
    Ok(started)
}

/// One plugin's queue as a diagnosis reads it: how much it owes, since when, and the lease standing for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// How much is queued, and since when.
    pub depth: QueueDepth,
    /// The runner lease as it stands, expired or not — `None` when no runner has claimed this queue.
    pub lease: Option<Lease>,
}

impl Waiting {
    /// Whether a runner is still saying it is on this queue, as of `now`. A lease past its horizon is a
    /// runner that died without releasing: the queue is not being worked, and the next drive takes it over.
    pub fn is_running(&self, now: &str) -> bool {
        self.lease.as_ref().is_some_and(|l| l.expires_at.as_str() > now)
    }
}

/// Every queue that still owes something, the one that has waited longest first — the delivery layer's
/// backlog, for a reader asking *why has nothing happened* (`AMB-D-399`).
///
/// The lease travels with the count because the count alone does not diagnose. Ten events with a live lease
/// is one plugin taking its time; ten with none is a plugin nothing is running; ten with a lease past its
/// horizon is a runner that died mid-queue. The three want different responses, and the execution log shows
/// none of them — what never ran wrote no line.
///
/// A read, and only a read: the lease is returned as it stands, with no judgement about its horizon
/// ([`Waiting::is_running`] is where a caller asks for one) and nothing claimed, released or started.
pub fn waiting(engine: &StoreEngine) -> Result<Vec<Waiting>> {
    backlog(engine.conn())?
        .into_iter()
        .map(|depth| {
            let lease = lease_of(engine.conn(), &depth.plugin)?;
            Ok(Waiting { depth, lease })
        })
        .collect()
}

/// Release the lease a launch was claimed for but never used, so the next drive can try again at once.
fn give_back(engine: &StoreEngine, plugin: &str, owner: &str) -> Result<()> {
    let tx = engine.write()?;
    tx.release_runner(plugin, owner)?;
    tx.commit()?;
    Ok(())
}

/// Work one plugin's queue to its end, holding `owner`'s lease throughout — the runner's whole body.
///
/// Rows are worked one at a time, oldest first, each bracketed by two transactions of its own: [`hold`]
/// before the plugin runs, and [`settle`] once it has returned. Both push the lease's horizon out and both
/// stop the runner when the lease is no longer its — the first so a runner already taken over does not fire
/// an event that is its successor's, the second because taking a row off a queue that is no longer this
/// runner's would be removing its successor's work. Between the two, [`beat`] pushes the horizon out from
/// inside the wait, so a run longer than the horizon is not mistaken for a runner that has died on it. The
/// loop ends where the queue does: [`leave`] re-reads it under the write lock and releases the lease only if
/// it is still empty, so an event queued a moment earlier is either seen by that read (and this runner stays
/// for it) or lands after the release (and the drive that queued it starts the next runner).
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
        // What this pass has to get through, counted once over the whole queue rather than per row
        // (`AMB-D-417`): a number counted again for every launch would sit at whatever is waiting right
        // now, so a plugin batching on it would never see the queue end while writes kept arriving.
        let counted = match queued_count(engine.conn(), plugin) {
            Ok(counted) => counted,
            Err(e) => return warn_stop(plugin, &e.to_string()),
        };
        let rows = match queued_for(engine.conn(), plugin, RUN_PAGE) {
            Ok(rows) => rows,
            Err(e) => return warn_stop(plugin, &e.to_string()),
        };
        // Rows queued between the count and the read belong to the next pass — the number handed out must
        // only fall — but the page in hand is work this pass is certainly doing, so it is the floor.
        let mut left = counted.max(rows.len() as i64);
        for row in &rows {
            // What stays behind once this row has been run, whether or not it reaches a plugin: a row that
            // resolves to nobody still leaves the queue.
            left -= 1;
            match hold(engine, plugin, owner) {
                Ok(true) => {}
                Ok(false) => return taken_over(plugin),
                Err(e) => return warn_stop(plugin, &e.to_string()),
            }
            match hook_for(subs, row) {
                Ok(Some(mut hook)) => {
                    hook.invocation = hook.invocation.env(QUEUE_REMAINING_VAR, left.to_string());
                    crate::plugin_hooks::run_queued(
                        &hook,
                        log,
                        Some(crate::plugin_hooks::Heartbeat {
                            every: BEAT,
                            beat: &|| beat(engine, plugin, owner),
                        }),
                    )
                }
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

/// Push the lease out before a row is run — and, through [`beat`], while it runs — saying whether it is
/// still this runner's to push. `false` means the queue has been taken over past this runner's horizon: the
/// row is its successor's to run, and firing it here would be a delivery nobody asked for on top of the one
/// the successor is making.
fn hold(engine: &StoreEngine, plugin: &str, owner: &str) -> Result<bool> {
    let tx = engine.write()?;
    if !tx.extend_runner(plugin, owner, &horizon(Timestamp::now()))? {
        return Ok(false);
    }
    tx.commit()?;
    Ok(true)
}

/// Push the lease out **while the row's plugin is still running** (`AMB-T-2174`), from the wait itself
/// ([`BEAT`]). Unlike [`hold`] it decides nothing: a run under way is not stopped part-done on account of a
/// lease, whatever the answer would be.
///
/// - **Taken over** — a successor is on this queue and will run this row again. Nothing to do here but
///   finish what is running; [`settle`] is where this runner reads the same fact and leaves the row to it.
/// - **A store error** — likely the write lock held by whoever took over, or a store that has gone away.
///   Either way the next beat tries again, and [`settle`] is the one that has to decide.
fn beat(engine: &StoreEngine, plugin: &str, owner: &str) {
    match hold(engine, plugin, owner) {
        Ok(true) => {}
        Ok(false) => tracing::debug!(
            plugin = %plugin,
            "this runner's lease was taken over while its plugin was still running"
        ),
        Err(e) => tracing::debug!(
            plugin = %plugin,
            error = %e,
            "a running plugin's lease could not be pushed out; trying again at the next beat"
        ),
    }
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
            project: None,
            record: None,
            parent: None,
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

    /// A resolver whose plugin is a shell line writing what it was told is left behind, one line per run.
    /// The number is added by the runner after this hands the invocation over, so reading it back out of a
    /// real child is the only place it can be seen at all.
    #[cfg(unix)]
    struct Reporting {
        plugin: &'static str,
        out: std::path::PathBuf,
    }
    #[cfg(unix)]
    impl Subscribers for Reporting {
        fn resolve(&self, _event: &str, _project: Option<i64>, _face: Face) -> Vec<Subscriber> {
            let invocation = PluginInvocation::new("/bin/sh")
                .arg("-c")
                .arg(format!("echo \"${QUEUE_REMAINING_VAR}\" >> {}", self.out.display()));
            vec![Subscriber::new(self.plugin, invocation)]
        }
        fn resolve_one(
            &self,
            plugin: &str,
            event: &str,
            project: Option<i64>,
            face: Face,
        ) -> Option<Subscriber> {
            self.resolve(event, project, face).into_iter().find(|s| s.plugin == plugin)
        }
    }

    #[cfg(unix)]
    fn reported(out: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(out)
            .unwrap()
            .lines()
            .map(|l| l.trim().to_string())
            .collect()
    }

    /// What every plugin is told is how much is behind it, counted once for the pass and handed down one
    /// per run (`AMB-D-417`): five queued events are `4,3,2,1,0`, and the last one is the only `0`. A real
    /// child is the only place the number is visible, so this is unix-only like the other end-to-end runs.
    #[cfg(unix)]
    #[test]
    fn each_run_is_told_how_many_are_still_behind_it() {
        let dir = amenbo_scratch::scratch("plugin-runner-remaining");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("remaining.txt");
        let _ = std::fs::remove_file(&out);

        let e = StoreEngine::open_in_memory().unwrap();
        for id in 1..=5 {
            queue(&e, "slack", id);
        }
        claim(&e, "slack", "mine");

        run_queue(&e, &Reporting { plugin: "slack", out: out.clone() }, "slack", "mine", None);

        assert_eq!(reported(&out), ["4", "3", "2", "1", "0"], "counted down to the end of the queue");
    }

    /// The number never grows while a pass is being counted down (`AMB-D-417`). An event queued *during* the
    /// pass is not in the count the pass started with, so the rows already in hand keep falling to `0` — and
    /// the newcomer is delivered right after, as its own pass, which is why `0` says the queue is empty now
    /// rather than promising nothing more is coming.
    #[cfg(unix)]
    #[test]
    fn an_event_queued_mid_pass_does_not_raise_what_the_runs_are_told() {
        /// Queues one more row the first time it is asked, then answers like [`Reporting`].
        struct QueuesOneMore<'a> {
            engine: &'a StoreEngine,
            inner: Reporting,
            queued: Mutex<bool>,
        }
        impl Subscribers for QueuesOneMore<'_> {
            fn resolve(&self, event: &str, project: Option<i64>, face: Face) -> Vec<Subscriber> {
                self.inner.resolve(event, project, face)
            }
            fn resolve_one(
                &self,
                plugin: &str,
                event: &str,
                project: Option<i64>,
                face: Face,
            ) -> Option<Subscriber> {
                let mut queued = self.queued.lock().unwrap();
                if !*queued {
                    *queued = true;
                    queue(self.engine, plugin, 99);
                }
                self.inner.resolve_one(plugin, event, project, face)
            }
        }

        let dir = amenbo_scratch::scratch("plugin-runner-remaining-mid-pass");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("remaining.txt");
        let _ = std::fs::remove_file(&out);

        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", 1);
        queue(&e, "slack", 2);
        claim(&e, "slack", "mine");

        let subs = QueuesOneMore {
            engine: &e,
            inner: Reporting { plugin: "slack", out: out.clone() },
            queued: Mutex::new(false),
        };
        run_queue(&e, &subs, "slack", "mine", None);

        assert_eq!(
            reported(&out),
            ["1", "0", "0"],
            "the pass counts down the two it started with; the one queued during it is its own pass"
        );
        assert!(queued_for(e.conn(), "slack", 10).unwrap().is_empty(), "and all three were run");
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

    /// A beat from inside a run pushes the horizon out, which is what keeps a long single run from looking
    /// like a runner that died on it (`AMB-T-2174`). The lease is planted with a horizon already in the past
    /// — the state a run longer than [`LEASE_TTL`] would otherwise reach — and one beat carries it forward.
    #[test]
    fn a_beat_carries_a_running_plugins_lease_past_its_horizon() {
        let e = StoreEngine::open_in_memory().unwrap();
        let tx = e.write().unwrap();
        assert!(tx
            .claim_runner("slack", "mine", "2000-01-01T00:00:00Z", &Timestamp::now().to_rfc3339_z())
            .unwrap());
        tx.commit().unwrap();

        beat(&e, "slack", "mine");

        let lease = crate::store_engine::lease_of(e.conn(), "slack").unwrap().unwrap();
        assert_eq!(lease.owner, "mine", "the lease is the same runner's");
        assert!(
            lease.expires_at.as_str() > Timestamp::now().to_rfc3339_z().as_str(),
            "and its horizon is ahead again: {}",
            lease.expires_at
        );
    }

    /// A beat by a runner whose lease was taken over moves nothing — the horizon it would push out is its
    /// successor's. The run under way is not stopped for it either: [`settle`] is where that is decided, once
    /// the plugin has actually returned.
    #[test]
    fn a_beat_by_a_taken_over_runner_moves_nothing() {
        let e = StoreEngine::open_in_memory().unwrap();
        claim(&e, "slack", "successor");
        let before = crate::store_engine::lease_of(e.conn(), "slack").unwrap().unwrap();

        beat(&e, "slack", "the-one-taken-over");

        assert_eq!(
            crate::store_engine::lease_of(e.conn(), "slack").unwrap().unwrap(),
            before,
            "the successor's lease is untouched"
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

    /// A launcher that starts no process and records what it was asked to start — what these tests are
    /// about is the gate in front of the launch, not the process behind it (that is [`run_process`]'s, and
    /// what it then does is [`run_queue`]'s, tested above).
    struct Launched {
        asked: Mutex<Vec<(String, String)>>,
        fails: bool,
    }
    impl Launched {
        fn new() -> Self {
            Self { asked: Mutex::new(Vec::new()), fails: false }
        }
        fn failing() -> Self {
            Self { asked: Mutex::new(Vec::new()), fails: true }
        }
    }
    impl RunnerLauncher for Launched {
        fn launch(&self, plugin: &str, owner: &str) -> std::io::Result<()> {
            self.asked.lock().unwrap().push((plugin.to_string(), owner.to_string()));
            if self.fails {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such runner"));
            }
            Ok(())
        }
    }

    /// Starting is gated by the lease, which is the whole of "one runner per plugin": the first drive takes
    /// it and launches a runner, and a drive while it stands launches nobody. The launch is a process and
    /// nobody waits for it, so what comes back is the plugin's name.
    #[test]
    fn a_drive_starts_a_runner_only_for_a_queue_nobody_is_running() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", 1);
        let launcher = Launched::new();

        let first = start(&e, &launcher).unwrap();
        assert_eq!(first, vec!["slack".to_string()], "the queue was unheld, so a runner is launched");

        // The lease taken above is still standing (nothing ran, so nothing reached `leave`), so a second
        // drive over the same queue leaves the work to whoever holds it.
        queue(&e, "slack", 2);
        let second = start(&e, &launcher).unwrap();
        assert!(second.is_empty(), "a plugin already being run is left alone");
        assert_eq!(launcher.asked.lock().unwrap().len(), 1, "and no second runner was launched");
    }

    /// The launch carries the lease this drive took, so the runner extends and gives up the very lease that
    /// gated its start — the two are one runner, not two.
    #[test]
    fn the_launch_carries_the_lease_the_drive_took() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", 1);
        let launcher = Launched::new();

        start(&e, &launcher).unwrap();
        let asked = launcher.asked.lock().unwrap();
        assert_eq!(asked[0].0, "slack");
        assert_eq!(
            asked[0].1,
            crate::store_engine::lease_of(e.conn(), "slack").unwrap().unwrap().owner,
            "the owner handed to the runner is the one standing on the lease"
        );
    }

    /// A runner that would not start gives its lease straight back, so the next drive tries again at once
    /// rather than leaving the queue held for the lease's horizon by a process that never existed.
    #[test]
    fn a_runner_that_would_not_start_gives_its_lease_back() {
        let e = StoreEngine::open_in_memory().unwrap();
        queue(&e, "slack", 1);

        let started = start(&e, &Launched::failing()).unwrap();
        assert!(started.is_empty(), "nothing was started, so nothing is reported as started");
        assert_eq!(
            crate::store_engine::lease_of(e.conn(), "slack").unwrap(),
            None,
            "and the queue is not held by the runner that never was"
        );

        // Which is to say the next drive is free to try again.
        let launcher = Launched::new();
        assert_eq!(start(&e, &launcher).unwrap().len(), 1);
    }

    /// The backlog carries the lease beside the count, which is the whole of its diagnostic value: a queue
    /// nobody claimed and a queue being worked read the same as numbers, and want opposite responses.
    #[test]
    fn the_backlog_says_who_is_on_each_queue() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(waiting(&e).unwrap().is_empty(), "nothing queued, nothing owed");

        queue(&e, "slack", 1);
        queue(&e, "email", 2);
        queue(&e, "slack", 3);
        claim(&e, "slack", "r1");

        let owed = waiting(&e).unwrap();
        assert_eq!(owed.len(), 2);
        let slack = owed.iter().find(|w| w.depth.plugin == "slack").unwrap();
        assert_eq!(slack.depth.waiting, 2);
        assert_eq!(slack.lease.as_ref().unwrap().owner, "r1");
        assert!(slack.is_running(&Timestamp::now().to_rfc3339_z()), "the lease was just claimed");

        let email = owed.iter().find(|w| w.depth.plugin == "email").unwrap();
        assert_eq!((email.depth.waiting, &email.lease), (1, &None), "nobody claimed this queue");

        // A horizon that has passed is a runner that died without releasing — still a lease on the row,
        // and not a queue anyone is working.
        assert!(!slack.is_running("2999-01-01T00:00:00Z"), "a lease past its horizon is nobody on it");
    }
}
