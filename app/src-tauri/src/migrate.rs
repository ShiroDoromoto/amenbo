//! The GUI's half of the one execution site: carry this device's store forward at startup, or refuse to
//! open it at all — **with a human watching it happen**. Everything about *how* is core's
//! ([`amenbo_core::migrate::at_startup`] — the lock the CLI waits on and that this waits on, the
//! pre-migration backup, the rollback); what is the GUI's is that [`is_pending()`](crate::migrate::is_pending)
//! is asked before anything opens the store, that a pending migration runs on **its own thread** rather than
//! inside `setup` (a long migration there is a window that never appears) while the window comes up on the
//! migration screen — what it is about to do, what it costs, its progress, and what it left behind (where
//! the pre-migration backup is, and which older rewind points it swept: a whole copy of the store is never
//! deleted in silence) — and that [`gate()`](crate::migrate::gate) stands in front of every store open in
//! `commands`: a store between versions, or one left at the *old* version by a failed (rolled-back)
//! migration, is not one this build may read. There is no consent prompt: the pre-migration backup is the
//! answer to "what if this goes wrong", and the only other button a prompt could offer is one that leaves
//! the store unusable by the build that is already running. The screen tells; it does not ask.

use std::ops::ControlFlow;
use std::sync::{Mutex, OnceLock};

use amenbo_core::migrate::{MigrationReport, Pending};
use amenbo_core::progress::Progress;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

use crate::commands::DataProgressDto;
use crate::error::CmdError;

/// Stage transitions, carrying the whole [`MigrationStatusDto`]. The window may well mount *after* one of
/// them, so this is a nudge and not the source of truth — the screen pulls [`status()`] on mount and
/// listens from there.
const CHANGED_EVENT: &str = "migration-changed";
/// One tick of the run ([`DataProgressDto`] — the same shape the data ops emit, so the front reads both
/// with one type): the pre-migration backup's phases, then one per step of the chain
/// ([`amenbo_core::progress::Phase::Migrating`]), so a long chain does not read as a frozen window.
const PROGRESS_EVENT: &str = "migration-progress";

/// What the startup migration is doing, for the migration screen.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatusDto {
    /// `idle` (nothing to carry forward — no screen) / `running` / `done` / `failed`. A stable string the
    /// front switches on; never shown as it is.
    pub stage: String,
    /// What the run is about to do, once core has announced it (`running`, before the first byte is
    /// written).
    pub pending: Option<MigrationPendingDto>,
    /// The latest tick of the run — the pre-migration backup, then the chain's steps — so a window that
    /// mounts mid-run has a bar to draw.
    pub progress: Option<DataProgressDto>,
    /// What the run did (`done`).
    pub report: Option<MigrationDoneDto>,
    /// Why the run failed (`failed`). The same structured error every store open now refuses with, so the
    /// screen localises it exactly like any other (`errLabel`).
    #[ts(type = "{ code: string; message_en: string; fields: Record<string, unknown> | null } | null")]
    pub error: Option<CmdError>,
}

impl MigrationStatusDto {
    fn idle() -> Self {
        MigrationStatusDto { stage: "idle".into(), pending: None, progress: None, report: None, error: None }
    }
}

/// [`amenbo_core::migrate::Pending`] as the screen reads it: the two versions, the steps between them, and
/// the disk the pre-migration backup needs. Bytes, not MiB — the rounding belongs on the side that has the
/// reader.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct MigrationPendingDto {
    /// The format version the store carries now.
    pub from: i32,
    /// The version the chain will carry it to.
    pub to: i32,
    /// How many steps stand between the two.
    pub steps: usize,
    /// Upper bound on the finished archive.
    pub archive_bytes: usize,
    /// Transient peak on top of it (the staged snapshot, deleted once appended).
    pub staging_bytes: usize,
    /// What must be free for the backup to complete.
    pub required_bytes: usize,
    /// What is actually free where the backup lands.
    pub available_bytes: usize,
}

/// [`amenbo_core::migrate::MigrationReport`] as the completion panel reads it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct MigrationDoneDto {
    /// The version the store came from.
    pub from: i32,
    /// The version it is at now.
    pub to: i32,
    /// Where the pre-migration backup is kept — the only way back (there is no downgrade).
    pub backup_path: Option<String>,
    /// The pre-migration backups from earlier migrations that this run's own backup superseded, and which
    /// it therefore deleted. Shown, never silent.
    pub superseded: Vec<String>,
}

/// The one state the migration thread writes, and the commands and the screen read.
fn state() -> &'static Mutex<MigrationStatusDto> {
    static STATE: OnceLock<Mutex<MigrationStatusDto>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(MigrationStatusDto::idle()))
}

/// Change the state and tell the window it changed (one place, so no transition is published half-written).
fn set(app: &AppHandle, f: impl FnOnce(&mut MigrationStatusDto)) {
    let status = {
        let mut guard = state().lock().expect("migration status poisoned");
        f(&mut guard);
        guard.clone()
    };
    let _ = app.emit(CHANGED_EVENT, status);
}

/// Does this device's store have a step waiting for it (core's cheap check)? `setup` asks this to decide
/// whether there is anything to show — and, if there is not, opens the store immediately as before.
pub fn is_pending() -> bool {
    amenbo_core::migrate::is_pending()
}

/// Mark the migration as under way **before** the thread that runs it starts, so a window that mounts
/// first still finds `running` — and goes to the screen instead of to the store.
pub fn begin() {
    state().lock().expect("migration status poisoned").stage = "running".into();
}

/// Run the startup migration to its end, publishing everything the screen shows. Returns whether the store
/// may be opened afterwards (a failed migration leaves it whole but at the old version — nothing in this
/// process may read it).
///
/// Never panics and never fails the app's setup: a failure becomes the state every store open then refuses
/// with, which is where the GUI can actually show a human what happened.
pub fn run(app: &AppHandle) -> bool {
    let mut announce = |p: &Pending| {
        log::info!(
            "migrating the store: format v{} → v{} ({} step(s)); pre-migration backup needs ~{} MiB, ~{} MiB free",
            p.from,
            p.to,
            p.steps,
            p.plan.required_bytes.div_ceil(1024 * 1024),
            p.plan.available_bytes.div_ceil(1024 * 1024),
        );
        set(app, |s| s.pending = Some(pending_dto(p)));
    };
    let mut progress = |p: &Progress| {
        let tick = DataProgressDto::of(p);
        let _ = app.emit(PROGRESS_EVENT, tick.clone());
        state().lock().expect("migration status poisoned").progress = Some(tick);
        // A migration has no "cancel", which is where it parts company with backup and restore: stepping out
        // leaves the store at the old version, which this build cannot open. Do not pretend it can be stopped.
        ControlFlow::Continue(())
    };

    match amenbo_core::migrate::at_startup(&mut announce, &mut progress) {
        Ok(report) => {
            let done = report.filter(|r| r.migrated()).map(|r| done_dto(&r));
            if let Some(done) = &done {
                log::info!(
                    "store migrated to format v{} (pre-migration backup at {})",
                    done.to,
                    done.backup_path.as_deref().unwrap_or("-"),
                );
            }
            // The run found nothing to do — the CLI finished the chain while we were waiting on the lock — so
            // there is nothing to show. Fold the screen away without raising it and go straight into the app.
            set(app, |s| {
                s.stage = if done.is_some() { "done".into() } else { "idle".into() };
                s.report = done;
                s.progress = None;
            });
            true
        }
        Err(e) => {
            log::error!("store migration failed: {e}");
            set(app, |s| {
                s.stage = "failed".into();
                s.progress = None;
                s.error = Some(CmdError::from(e));
            });
            false
        }
    }
}

/// What the migration screen shows: its first read (the window mounts after the thread has started), and
/// its state again after every event.
pub fn status() -> MigrationStatusDto {
    state().lock().expect("migration status poisoned").clone()
}

/// The door every store open passes (`commands::ensure_migrated`): a migration that is **running** means
/// the store is between versions, and one that **failed** means it is at the old one. Neither is a store
/// this build may read, so both are refused here rather than half-understood in a window.
pub fn gate() -> Result<(), CmdError> {
    let status = state().lock().expect("migration status poisoned");
    match status.stage.as_str() {
        "failed" => Err(status.error.clone().unwrap_or_else(|| {
            CmdError::coded(
                "migration_failed",
                "The store's migration failed.",
                serde_json::Value::Null,
            )
        })),
        "running" => Err(CmdError::coded(
            "migration_running",
            "The store is being updated. Wait for it to finish.",
            serde_json::Value::Null,
        )),
        _ => Ok(()),
    }
}

fn pending_dto(p: &Pending) -> MigrationPendingDto {
    MigrationPendingDto {
        from: p.from as i32,
        to: p.to as i32,
        steps: p.steps,
        archive_bytes: p.plan.archive_bytes as usize,
        staging_bytes: p.plan.staging_bytes as usize,
        required_bytes: p.plan.required_bytes as usize,
        available_bytes: p.plan.available_bytes as usize,
    }
}

fn done_dto(r: &MigrationReport) -> MigrationDoneDto {
    MigrationDoneDto {
        from: r.run.from as i32,
        to: r.run.to as i32,
        backup_path: r.backup.as_ref().map(|b| b.path.clone()),
        superseded: r.superseded.clone(),
    }
}
