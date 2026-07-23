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

    /// Write the folder-binding registry back atomically (a full rewrite of both indexes in a single
    /// transaction), so a whole-file rewrite can never tear.
    pub fn save_bindings(&self, reg: &crate::binding::Registry) -> Result<()> {
        let tx = self.engine.transaction()?;
        crate::overview::write_bindings(&tx, reg)?;
        tx.commit().map_err(crate::store_engine::StoreEngineError::from)?;
        Ok(())
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
    // owned here, not by `plugin_dispatch::deliver`, and the two faces differ only in where it lives (see
    // `crate::plugin_drive`).

    /// Drive the plugin observation dispatcher once from the **persisted** cursor — the short-lived (CLI)
    /// face (`AMB-T-2033`). Reads the stored cursor, fires the subscribers of everything committed since,
    /// and persists where it advanced to so the next process continues past it. The returned
    /// [`Delivered`](crate::plugin_dispatch::Delivered) carries the hooks to **join** before the process
    /// exits and whether a retention gap was hit. The cursor is already stored on return.
    pub fn drive_plugins_persisted(
        &self,
        subs: &dyn crate::plugin_dispatch::Subscribers,
    ) -> Result<crate::plugin_dispatch::Delivered> {
        crate::plugin_drive::drive_persisted(&self.engine, subs)
    }

    /// Drive the dispatcher from an **in-memory** cursor — the long-lived (GUI) face (`AMB-T-2033`).
    /// Delivers what committed since `cursor` without persisting; the caller keeps
    /// [`Delivered::cursor`](crate::plugin_dispatch::Delivered::cursor) in memory for the next drive and
    /// drops the hooks (fire-and-forget, its process outliving them).
    pub fn deliver_plugins(
        &self,
        cursor: i64,
        subs: &dyn crate::plugin_dispatch::Subscribers,
    ) -> Result<crate::plugin_dispatch::Delivered> {
        crate::plugin_dispatch::deliver(self.engine.conn(), cursor, subs)
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
