//! Local persistence. **The engine (SQLite) is the truth source.**
//!
//! Every write goes through one of `Store`'s write wrappers (`add_task` / `set_task_status` …), which
//! opens `BEGIN IMMEDIATE`, lets [`crate::ops`] do its reads and writes inside it, and commits — i.e.
//! **one logical operation = one transaction**. Reads likewise hit the engine's indexed SQL directly.
//!
//! **`Store` keeps no in-memory `Database`.** Mutations land on the truth source itself, so there is no
//! lost-update window. **Exclusion is left to SQLite**: writers serialize per transaction (milliseconds)
//! and `busy_timeout` absorbs the wait. Nothing serializes a whole process, so the CLI can still write
//! while the GUI is open.
//!
//! JSON is not the truth source; it is a projection `export` can emit at any time.
//!
//! The module is split by concern:
//! - [`open`]    — open / open_read / init (issuing the identity, assembling genesis).
//! - [`persist`] — the write path (the one-operation-one-transaction write wrappers, config writeout).
//! - [`read`]    — list_tasks / read_model (serving reads from indexed SQL).
//!
//! backup / restore live outside `Store`, in [`crate::archive`]. That is the only road that replaces the
//! truth source — it goes through the manifest, the version gate, the version chain and stage-and-swap —
//! and there is deliberately no short path that drops a raw snapshot back in place.

use std::fs;
use std::path::Path;

use crate::config::{Config, Paths};
use crate::error::Result;
use crate::identity::Identity;
use crate::store_engine::schema::col;
use crate::store_engine::sql::{Exists, Select, Sql};
use crate::store_engine::StoreEngine;

mod hard_erase;
pub(crate) mod open;
mod owner;
mod persist;
mod read;
mod write_reach;
#[cfg(test)]
mod tests;

pub use hard_erase::{HardEraseReport, HardEraseTarget};
pub use read::SyncChanges;

/// Result of the read-only integrity check run at startup. Computed once in `open` when
/// `config.startup_integrity_check` is on; the CLI/GUI surfaces any problem as a warning. Inspection
/// only — no side effects.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StartupHealth {
    /// The read-only integrity findings (orphaned or dangling references, tampered signatures, …).
    pub doctor: crate::validate::DoctorResult,
}

impl StartupHealth {
    /// Whether there is anything worth showing (any doctor issue).
    pub fn has_warnings(&self) -> bool {
        !self.doctor.issues.is_empty()
    }
}

/// A snapshot of this store's version / format state. It is what `doctor`, `--version` and
/// `agent --json` show to answer "how far can this build open the store?" and "is there an update?".
/// Update detection rests on a single upstream `latest.json`; detecting a store whose format has moved
/// ahead of this build is `format_version`'s job.
#[derive(Clone, Debug, serde::Serialize)]
pub struct VersionStatus {
    /// Version of the running binary ([`crate::agent::VERSION`]).
    pub app_version: &'static str,
    /// The monotonic `format_version` the store records (0 = the pre-guard baseline).
    pub format_version: i64,
    /// The highest format version this binary can open ([`crate::model::FORMAT_VERSION`]).
    pub max_supported_format: i64,
    /// **An update is available** — the version in the published `latest.json` is newer than
    /// `app_version`. `version_status()` on its own always reports `false` (it is local and does no
    /// network I/O); this only goes true via [`VersionStatus::with_upstream`].
    pub update_available: bool,
    /// The version to update to (what `update_available` rests on), for display. `None` = no update.
    pub newer_version: Option<String>,
    /// The latest version upstream (the published `latest.json`) announces — display and call-to-action
    /// material. `None` = the update check is disabled, was not fetched, or failed, or upstream was never
    /// folded in (plain `version_status()`). If it is newer than `app_version`,
    /// [`VersionStatus::with_upstream`] raises `update_available`.
    pub latest_version: Option<String>,
}

impl VersionStatus {
    /// Fold in upstream `latest.json` ([`crate::update_check::LatestRelease`]) and raise
    /// `update_available` / `newer_version`. With `upstream=None` (check disabled, not fetched, or
    /// failed) the answer stays "no update" — this stays pure and offline. Doing the lookup
    /// ([`crate::update_check::check`]) is the caller's job: `version_status()` is also called on the hot
    /// pre-command path, so network I/O is opt-in rather than baked in here.
    #[must_use]
    pub fn with_upstream(mut self, upstream: Option<&crate::update_check::LatestRelease>) -> Self {
        self.latest_version = upstream.map(|r| r.version.clone());
        // The upstream version only counts as an update when it is newer than `app_version` (we keep
        // `latest_version` either way, as information).
        self.newer_version = self
            .latest_version
            .as_deref()
            .filter(|v| version_is_newer(v, self.app_version))
            .map(str::to_string);
        self.update_available = self.newer_version.is_some();
        self
    }
}

/// Loosely parse a version string into `(major, minor, patch)`. Pre-release / build metadata (anything
/// after `-` or `+`) is ignored. Unparsable input yields `None` — incomparable, so callers can fall back
/// to the safe answer ("not newer").
///
/// Shared with [`crate::plugin_compat`], which must tell *"the floor is below us"* from *"the floor is not
/// a version at all"* — a distinction [`version_is_newer`] deliberately collapses.
pub(crate) fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next().unwrap_or(v);
    let mut it = core.split('.');
    let major = it.next()?.trim().parse().ok()?;
    let minor = it.next().unwrap_or("0").trim().parse().ok()?;
    let patch = it.next().unwrap_or("0").trim().parse().ok()?;
    Some((major, minor, patch))
}

/// True when `candidate` is **newer** than `base`. If either side fails to parse, false (the safe side).
pub(crate) fn version_is_newer(candidate: &str, base: &str) -> bool {
    match (parse_version(candidate), parse_version(base)) {
        (Some(c), Some(b)) => c > b,
        _ => false,
    }
}

/// An open store: config, paths and the local identity, plus **the engine that is the truth source**.
pub struct Store {
    pub config: Config,
    pub paths: Paths,
    /// This store's local identity (display name, bound_hw). Never synced.
    pub identity: Identity,
    /// Whether this open re-bound `bound_hw` because it detected a clone (the CLI warns about it).
    pub forked: bool,
    /// Result of the startup integrity check (`Some` only when `config.startup_integrity_check` is on).
    /// The CLI/GUI surfaces any problem as a warning. Inspection only — nothing is repaired.
    pub startup_check: Option<StartupHealth>,
    /// The store engine DB (`data_dir/store.sqlite`) that **is the truth source**. Reads and writes go
    /// straight here — ops' writes land through a transaction (`write_one` / `WriteTx`). It is opened
    /// even for a read-only open ([`Store::open_read_at`]); failing to open it fails the open itself (a
    /// store without its truth source does not exist). Swapping the file out happens outside an open
    /// `Store` ([`crate::archive`]), so a `Store` without an engine is not constructible.
    pub(crate) engine: StoreEngine,
    /// How far this open reaches ([`crate::reach::Reach`]). The default is the whole machine — the
    /// human's CLI, the GUI and library use all sit here. **Only the surface that carries the AI facet
    /// (the CLI)** narrows to the bound project via [`Store::with_reach`], after which every row leaving
    /// this store belongs to that project.
    pub(crate) reach: crate::reach::Reach,
}

impl Store {
    /// Narrow this store's reads to a single project. The surface (the CLI) derives the reach from the
    /// facet and the binding and declares it once, right after open — every piece of code that later
    /// carries the store around can then stay unaware of the reach.
    pub fn with_reach(mut self, reach: crate::reach::Reach) -> Self {
        self.reach = reach;
        self
    }

    /// How far this store reaches.
    pub fn reach(&self) -> crate::reach::Reach {
        self.reach
    }

    /// Read the folder-binding registry ([`crate::binding::Registry`]) from the store's binding table
    /// (straight out of the engine that is already open).
    pub fn bindings(&self) -> crate::binding::Registry {
        crate::overview::load_bindings(self.engine.conn())
    }

    /// Write the folder-binding registry back in a single transaction, so a save can never tear: the
    /// pairs the registry has dropped go, the pairs it has gained arrive, and a folder that is still
    /// bound keeps the row — and the id — it already had ([`crate::overview::write_bindings`]).
    pub fn save_bindings(&self, reg: &crate::binding::Registry) -> Result<()> {
        let tx = self.engine.transaction()?;
        crate::overview::write_bindings(&tx, reg)?;
        tx.commit().map_err(crate::store_engine::StoreEngineError::from)?;
        Ok(())
    }

    /// The bindings as rows, **id included** ([`crate::binding::BoundFolder`]) — what
    /// [`Self::bindings`] drops. A surface that has to name one binding rather than list a project's
    /// folders reads this one.
    pub fn bound_folders(&self) -> Result<Vec<crate::binding::BoundFolder>> {
        crate::overview::bound_folders(self.engine.conn())
    }

    /// [`Self::bound_folders`], narrowed to the folders **one project** has. This is the set a task's
    /// place is named from (`AMB-D-648`): a task's folder is one of its own project's, which is what
    /// keeps a place from naming a folder outside the project the task lives in.
    pub fn bound_folders_of(&self, project_id: i64) -> Result<Vec<crate::binding::BoundFolder>> {
        Ok(self.bound_folders()?.into_iter().filter(|f| f.project_id == project_id).collect())
    }

    /// Re-point one binding at another folder, keeping its id
    /// ([`crate::overview::repoint_binding`] carries the whole of what that means). `None` when no
    /// binding has that id.
    pub fn repoint_binding(
        &self,
        id: i64,
        project_id: i64,
        dir: &str,
    ) -> Result<Option<crate::binding::Repoint>> {
        let tx = self.engine.transaction()?;
        let done = crate::overview::repoint_binding(&tx, id, project_id, dir)?;
        tx.commit().map_err(crate::store_engine::StoreEngineError::from)?;
        Ok(done)
    }

    // ── Machine-local overview state (read receipts, inbox archive) ────────────────
    //
    // Device state that is never synced, keyed by task_id (a task's primary key). Read and written
    // straight through the engine that is already open — we never open a second connection to the same
    // file.

    /// This machine's read state (per-task last_seen plus a last_seen for the mailbox as a whole).
    pub fn read_receipts(&self) -> Result<crate::read_receipts::ReadReceipts> {
        crate::overview::read_receipts(&self.engine)
    }

    /// Mark a task as seen (last viewed at `at`).
    pub fn mark_task_seen(&self, task_id: i64, at: &str) -> Result<()> {
        crate::overview::mark_task_seen(&self.engine, task_id, at)
    }

    /// Mark the whole mailbox as seen (advance the badge's freshness baseline to `at`).
    pub fn mark_mailbox_seen(&self, at: &str) -> Result<()> {
        crate::overview::mark_mailbox_seen(&self.engine, at)
    }

    // ── Plugin observation dispatch (mounting the dispatcher at the write seam) ─────
    //
    // The ops write points appended semantic events to the outbox inside their transactions (`AMB-D-367`);
    // these are the caller `AMB-D-367` hands the cursor to — the single dispatcher's mount. The cursor is
    // owned here, not by the dispatcher's own halves, and both faces share the one persisted cursor
    // (`AMB-D-380`; see `crate::plugin_drive`).

    /// Drive the plugin observation dispatcher once from the **persisted** cursor — the mount both faces
    /// use (`AMB-D-380`). Reads the stored cursor, fans everything committed since onto the queues of the
    /// plugins that observe it, persists where it got to so the next drive — in either face — continues
    /// past it, and then launches the runners (`AMB-D-399`). `face` selects which subscriptions resolve and
    /// is recorded beside the cursor for diagnosis. `runner_argv` is how **this face** re-runs itself as a
    /// runner process (`AMB-T-2175`) — the face owns the spelling of its own entry point, and this hands it
    /// the store to work. The returned [`Delivered`](crate::plugin_dispatch::Delivered) names the runners it
    /// launched, carries the replies to surface, and says whether a retention gap was hit; there is nothing
    /// in it to wait for. The cursor is already stored on return. Every run, and a gap, land in this
    /// machine's execution log (`AMB-D-361`) — the store knows where that file is, so no face has to name it.
    pub fn drive_plugins_persisted(
        &self,
        face: crate::plugin_drive::Face,
        subs: &dyn crate::plugin_dispatch::Subscribers,
        runner_argv: &[&str],
    ) -> Result<crate::plugin_dispatch::Delivered> {
        // A runner opens the store at this base directory for itself: it is a process of its own, and this
        // `Store` is the caller's, closed when the command that opened it returns (`AMB-D-399`).
        let launcher =
            crate::plugin_runner::SelfRunner::new(runner_argv, self.paths.base_dir.clone());
        crate::plugin_drive::drive_persisted(
            &self.engine,
            face,
            subs,
            Some(&launcher),
            Some(&self.paths.plugin_log_file()),
        )
    }

    /// Drive the dispatcher **only if a previous run left delivery unfinished** — the startup kick both
    /// faces make (`AMB-D-399`, [`plugin_drive::resume_persisted`](crate::plugin_drive::resume_persisted)).
    ///
    /// Same drive, same cursor, same runner entry point as the write seam above; what differs is when it is
    /// worth making. A face reaches this on every start, reads included, so it asks first — two reads — and
    /// returns `None` when there was nothing standing, taking no write lock at all. `Some` carries what the
    /// drive moved, exactly as the write seam's does.
    pub fn resume_plugin_delivery(
        &self,
        face: crate::plugin_drive::Face,
        subs: &dyn crate::plugin_dispatch::Subscribers,
        runner_argv: &[&str],
    ) -> Result<Option<crate::plugin_dispatch::Delivered>> {
        let launcher =
            crate::plugin_runner::SelfRunner::new(runner_argv, self.paths.base_dir.clone());
        crate::plugin_drive::resume_persisted(
            &self.engine,
            face,
            subs,
            Some(&launcher),
            Some(&self.paths.plugin_log_file()),
        )
    }

    /// Drive delivery and work every queue **to its end, in this process** — the flush a caller asks for on
    /// purpose (`AMB-T-2470`, [`plugin_drive::flush_persisted`](crate::plugin_drive::flush_persisted)).
    ///
    /// Same cursor and same fan-out as the two mounts above; what differs is that no runner process is
    /// started — the queues are worked here, so this returns only once they are empty (or a runner stopped
    /// short) and can say how much left each one. There is no `runner_argv` for that reason: nothing re-runs
    /// this executable, so no face has to name its own entry point. A queue a live runner already holds is
    /// left to it and reported by nobody here.
    pub fn flush_plugin_delivery(
        &self,
        face: crate::plugin_drive::Face,
        subs: &dyn crate::plugin_dispatch::Subscribers,
    ) -> Result<crate::plugin_drive::Flushed> {
        crate::plugin_drive::flush_persisted(
            &self.engine,
            face,
            subs,
            Some(&self.paths.plugin_log_file()),
        )
    }

    /// Stop delivering to a plugin: throw away what is waiting for it and end the runner working it, on one
    /// transaction (`AMB-D-399`). Returns how many queued rows went.
    ///
    /// `project` narrows the drop to one project's share, which is what a switch closing means
    /// (`AMB-D-434`); `None` is the whole plugin — an uninstall. The lease goes
    /// only once the queue is empty, read inside the same transaction: a plugin still on in another project
    /// has work left, and the runner already on it is the one that should carry it out.
    ///
    /// The pair is why this is one transaction rather than two calls. A queue emptied with the lease left
    /// standing would be a claim no runner can release (the holder releases only what it can see is empty,
    /// and the rows are gone), and a lease dropped with rows left would end a runner that still has work.
    pub fn drop_plugin_delivery(&self, plugin: &str, project: Option<i64>) -> Result<usize> {
        let tx = self.engine.write()?;
        let dropped = tx.drop_queued(plugin, project)?;
        if crate::store_engine::queued_for(tx.conn(), plugin, 1)?.is_empty() {
            tx.drop_runner(plugin)?;
        }
        tx.commit()?;
        Ok(dropped)
    }

    /// The inbox items archived (dismissed) on this machine, as task_ids.
    pub fn inbox_archive_ids(&self) -> Result<Vec<i64>> {
        crate::overview::inbox_archive_ids(&self.engine)
    }

    /// Archive an inbox item (idempotent).
    pub fn inbox_archive_add(&self, task_id: i64) -> Result<()> {
        crate::overview::inbox_archive_add(&self.engine, task_id)
    }

    /// Un-archive an inbox item (idempotent).
    pub fn inbox_archive_remove(&self, task_id: i64) -> Result<()> {
        crate::overview::inbox_archive_remove(&self.engine, task_id)
    }

    /// GC read receipts: drop the rows whose task_id `keep` rejects (i.e. deleted tasks). `true` if
    /// anything was dropped.
    pub fn retain_live_read_receipts(&self, keep: impl Fn(i64) -> bool) -> Result<bool> {
        crate::overview::retain_live_read_receipts(&self.engine, keep)
    }

    /// GC the inbox archive (the counterpart of [`Store::retain_live_read_receipts`]).
    pub fn retain_live_inbox_archive(&self, keep: impl Fn(i64) -> bool) -> Result<bool> {
        crate::overview::retain_live_inbox_archive(&self.engine, keep)
    }

    /// The inbox items this device has already raised an OS notification for, as task_ids.
    pub fn mailbox_notified_ids(&self) -> Result<Vec<i64>> {
        crate::overview::mailbox_notified_ids(&self.engine)
    }

    /// Record that these inbox items have now been notified (idempotent, batched).
    pub fn mailbox_notified_add(&self, task_ids: &[i64]) -> Result<()> {
        crate::overview::mailbox_notified_add(&self.engine, task_ids)
    }

    /// GC the mailbox notified set (the counterpart of [`Store::retain_live_inbox_archive`]).
    pub fn retain_live_mailbox_notified(&self, keep: impl Fn(i64) -> bool) -> Result<bool> {
        crate::overview::retain_live_mailbox_notified(&self.engine, keep)
    }

    /// The nudges to put to the person now ([`crate::nudge`]). `stage_open` answers whether a nudge's
    /// declared stage is one this caller is in — the caller holds the settings a stage is about.
    pub fn pending_nudges(
        &self,
        stage_open: impl Fn(&str) -> bool,
    ) -> Result<Vec<&'static crate::nudge::Nudge>> {
        crate::nudge::pending(&self.engine, stage_open)
    }

    /// Record that a nudge has been put — called once it has actually been shown, not when it was
    /// judged due.
    pub fn mark_nudge_put(&self, nudge_id: &str) -> Result<()> {
        crate::nudge::mark_put(&self.engine, nudge_id)
    }

    /// What is written on this project's draft page ([`crate::memo`]).
    pub fn memo(&self, project_id: i64) -> Result<String> {
        crate::memo::memo(&self.engine, project_id)
    }

    /// Write this project's draft page. Blank erases it.
    pub fn set_memo(&self, project_id: i64, text: &str) -> Result<()> {
        crate::memo::set_memo(&self.engine, project_id, text)
    }

    /// What this device kept of the talk window's arrangement ([`crate::frames`]).
    pub fn saved_layout(&self) -> Result<Option<crate::frames::SavedLayout>> {
        crate::frames::saved_layout(&self.engine)
    }

    /// Keep the part of the talk window's arrangement that outlives the run.
    pub fn save_layout(&self, layout: &crate::frames::SavedLayout) -> Result<()> {
        crate::frames::save_layout(&self.engine, layout)
    }

    /// Count one launch of the app on this device (the tally two of the metrics are read from).
    pub fn record_launch(&self) -> Result<()> {
        crate::nudge::record_launch(&self.engine)
    }

    /// Whether the tick's banner has a question to put on this device today
    /// ([`crate::tick::banner_shows`]) — the whole of what the app asks before drawing it.
    pub fn tick_banner_shows(&self, today: chrono::NaiveDate) -> Result<bool> {
        crate::tick::banner_shows(self, today)
    }

    /// Put the tick's banner off until tomorrow — what the **later** button records, and the only thing
    /// it records (the question stays unanswered).
    pub fn defer_tick_banner(&self, day: &str) -> Result<()> {
        crate::overview::defer_tick_banner(&self.engine, day)
    }

    /// Has this project been opted out of the lint hooks — did `hooks uninstall` run in it
    /// Has this project been opted out of the lint hooks — did `hooks uninstall` run in it
    /// ([`crate::hooks`])? It says what was explicitly asked for here, never what `.git/hooks` currently
    /// holds. The *answer* to the hook question is not per project and does not live here: it is
    /// [`crate::config::Config::hook_consent`].
    pub fn hook_opted_out(&self, project_id: i64) -> Result<bool> {
        crate::overview::hook_opted_out(&self.engine, project_id)
    }

    /// Opt a project out of the lint hooks, or take that back — what `hooks uninstall` and `hooks
    /// install` record about the repository they ran in.
    pub fn set_hook_optout(&self, project_id: i64, opted_out: bool) -> Result<()> {
        crate::overview::set_hook_optout(&self.engine, project_id, opted_out)
    }

    /// What this project answered about having its folder start an AI on `amenbo agent`
    /// ([`crate::harness`]), or `None` when it has never been asked. It says what was answered, never
    /// what the provider settings hold — those are read every time, and the two meet in
    /// [`crate::harness::reconcile`].
    pub fn harness_consent(&self, project_id: i64) -> Result<Option<crate::harness::Consent>> {
        crate::overview::harness_consent(&self.engine, project_id)
    }

    /// Record this project's answer to that question.
    pub fn set_harness_consent(
        &self,
        project_id: i64,
        consent: crate::harness::Consent,
    ) -> Result<()> {
        crate::overview::set_harness_consent(&self.engine, project_id, consent)
    }

    /// Forget this project's answer, back to never having been asked — the way a refusal is taken back,
    /// since a `no` is otherwise silent for good. The next surface that reads it puts the question again.
    pub fn clear_harness_consent(&self, project_id: i64) -> Result<()> {
        crate::overview::clear_harness_consent(&self.engine, project_id)
    }
}

/// The read-only check that makes cleaning up phantom empty stores (`doctor --fix`) safe. It reads the
/// given store engine DB (`store.sqlite`) **without `Store::open`** — so no locks and no self-healing
/// (identity backfill and friends) — and returns `true` only if it holds none of the user's content (no
/// project, no task). Any content means `false`: this may be real data rather than a phantom, and must
/// never be deleted. A missing file has no content either, so `true` (this is a read; it creates
/// nothing). The truth source is plaintext SQLite, so a legacy encrypted store makes the plaintext query
/// fail and we return `Err` — which lands the caller (`doctor --fix`) on the safe side: it cannot verify
/// the contents, so it leaves the store alone rather than deleting real data it merely cannot read. A
/// schema too old to query `project` / `task` behaves the same way. The engine is opened with
/// [`StoreEngine::open_read`], which **runs no DDL** — this is a peek at *another* store (a deletion
/// candidate), and it takes no exclusion. Content is probed with existence queries on `project` / `task`
/// directly; nothing is hydrated.
pub fn store_file_is_content_empty(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }
    // Pass the version gate before opening the engine. A store past our supported ceiling errors out
    // here, which again lands `doctor --fix` on the safe side: it cannot verify the contents, so it does
    // not delete.
    open::ensure_format_supported(&crate::store_engine::probe_format_stamp(path))?;
    let engine = StoreEngine::open_read(path)?;
    // "Has content" = at least one project or task row. Tables are named from the registry and the
    // predicate reads the column straight off it. This read happens after the engine is open, so unlike
    // the probes that read the physical schema raw (`store_engine::probe_live_projects`) it may be
    // written against the registry's current shape.
    const P: col::project::Cols = col::project::ALL;
    const T: col::task::Cols = col::task::ALL;
    let mut sel = Select::new();
    let has_content = sel.pred(Exists::over(P.table).pred().or(Exists::over(T.table).pred()));
    let sql = Sql::select(&sel);
    let content = engine
        .conn()
        .query_row(sql.text(), rusqlite::params_from_iter(sql.params()), |r| has_content.get(r))
        .map_err(crate::store_engine::StoreEngineError::from)?;
    Ok(!content)
}

/// Write to a temp file and rename it into place, so an interrupted write cannot leave a corrupt file.
/// This is the one atomic-write path shared by store persistence (`persist`) and the standalone
/// [`crate::config::Config::save`] that runs without an open store (first-run GUI setup, the facet
/// setters) — it is what keeps config on the filesystem without exposing it to torn writes.
pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn ensure_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}
