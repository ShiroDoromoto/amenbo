//! The envelope around the store's version chain: **capture the store whole before the first
//! step, and put it back whole if any step fails.**
//!
//! The chain itself is [`crate::store_engine::migrate`] — numbered steps, each one transaction, each
//! stamping the version it carries the store to. That gets the DB half right, but a step may also move
//! files (attachment blobs, the layout of the store directory), and no transaction covers those. So the
//! chain does not run alone: it runs inside this envelope.
//!
//! The discipline is:
//!
//! 1. **Capture before you move.** One verified `.amenbo-backup` of the whole store — truth source and
//!    every attachment blob — taken before a single step runs.
//! 2. **Where it lands is not the user's decision.** `<app-data>/pre-migrate-<stamp>.amenbo-backup`,
//!    inside amenbo's own data area, never a directory the user owns. The user-placed, off-machine copy
//!    is the deliberate `amenbo backup <path>` gesture and stays that way; this one is a
//!    machine-local rewind point taken automatically, whose only job is to exist until the migration is
//!    trusted. The caller shows the human where it went.
//! 3. **Refuse before you write.** The disk budget is estimated from file sizes first ([`plan_space`]),
//!    so a store too big for the free space is a refusal with a number in it — not a half-written
//!    archive discovered at the end.
//! 4. **There is no "continue without a backup" branch.** A capture that fails aborts the migration
//!    with the store untouched.
//!
//! **A failed run is rolled back.**
//! A step that fails takes its own transaction down with it, but the steps that already committed —
//! and any file a step moved — stand. The archive is what undoes them: the live store is restored from
//! it, and the store comes back at the version it started at. If even that fails, the error names the
//! archive so a human can restore it by hand; nothing is silently left half-migrated.
//!
//! **Nothing pending means nothing happens.** A store already at the latest version is not backed up
//! and not opened for writing — the envelope exists for a migration, and there is none.
//!
//! Where a migration runs: [`at_startup`] is **the** execution site, and both surfaces call it before
//! they open anything: the
//! CLI at the top of a command, the GUI in its setup. The installer does not migrate — it puts binaries
//! down and stops there, because data touched from inside an installer makes every interruption, every
//! permission problem and every failed cleanup the installer's problem.
//!
//! Which surface starts first is unknowable (the human may run `amenbo task list` before ever opening
//! the app, or the reverse), so **both enter the same path and one of them waits**: the migration takes
//! an exclusive lock on its own sidecar ([`migration_lock_path`]) and the other side blocks on it, then
//! re-reads the version and finds nothing left to do. A store already current never takes the lock at
//! all — the check costs one bare read of `store_meta`.
//!
//! A *third* process that is already past its startup — the long-running GUI whose store gets migrated
//! by a CLI — is not covered by that lock and is not meant to be: its opens fail with `store_busy`
//! while the chain runs, and with `format_ahead` afterwards, which is the restart prompt.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::archive::{self, ARCHIVE_EXT, BackupReport, StoreSource};
use crate::config::Paths;
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::store_engine::migrate::{self as chain, Run, Step, STEPS};
use crate::store_engine::{probe_format_version, StoreEngine};

/// Filename prefix of the automatic pre-migration archive (`pre-migrate-<stamp>.amenbo-backup`), mirroring
/// the `store.pre-restore-<stamp>` aside a restore leaves behind: the same "we moved your data, here is the
/// before" vocabulary. It is also what [`archive::sweep_superseded`] recognises a superseded one by.
const PRE_MIGRATE_PREFIX: &str = "pre-migrate-";

/// Fixed name of the migration's exclusion sidecar, beside the truth source.
pub const MIGRATION_LOCK_NAME: &str = "store.migrate.lock";

/// The lock that makes the two surfaces one path: whichever of CLI/GUI starts first holds this while it
/// migrates, and the other blocks here until it can look again.
///
/// A **separate file** from the swap lock ([`crate::swap_lock::SWAP_LOCK_NAME`]) on purpose. The run this
/// lock covers takes the swap lock itself — for the chain, and again if it has to roll back — and an
/// advisory lock is held per open file description, so one process nesting the two on a single file would
/// block waiting for itself.
pub fn migration_lock_path(db_path: &Path) -> PathBuf {
    db_path.with_file_name(MIGRATION_LOCK_NAME)
}

/// Rough per-entry cost of the uncompressed tar container: a 512-byte header plus up to 512 bytes of
/// content padding. Deliberately an over-estimate — [`plan_space`] must not under-promise.
const TAR_ENTRY_OVERHEAD: u64 = 1024;

/// The tar end-of-archive marker (two zero blocks) plus the manifest's own entry.
const TAR_FIXED_OVERHEAD: u64 = 2 * TAR_ENTRY_OVERHEAD;

/// Where a pre-migration backup lands: `<app-data>/pre-migrate-<stamp>.amenbo-backup`. `stamp` is the
/// caller's timestamp, so repeated migrations never collide and [`archive::backup_from`]'s
/// refuse-to-overwrite stays a real guard rather than a nuisance. Under `AMENBO_HOME` the app-data root
/// *is* that directory, so an isolated store's backup stays isolated too.
pub fn pre_migration_backup_path(stamp: &str) -> PathBuf {
    Paths::data_root().join(format!("{PRE_MIGRATE_PREFIX}{stamp}.{ARCHIVE_EXT}"))
}

/// The disk budget of a pre-migration backup, computed from file sizes alone — no snapshot is taken, so
/// a caller can show these numbers before committing to anything.
///
/// Every field is an **upper bound**: the archive bundles a `VACUUM INTO` snapshot, which compacts away
/// free pages, and its blob bytes are copied verbatim. Reporting the live (uncompacted) sizes therefore
/// over-states the archive rather than risking a mid-write ENOSPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpacePlan {
    /// Upper bound on the finished archive: the truth source (plus its `-wal`/`-shm` sidecars, whose
    /// committed frames the snapshot folds in), every attachment blob, tar overhead.
    pub archive_bytes: u64,
    /// Transient peak on top of the archive: the staged snapshot, deleted once appended.
    pub staging_bytes: u64,
    /// What must be free for the backup to complete: `archive_bytes + staging_bytes`.
    pub required_bytes: u64,
    /// What is actually free on the filesystem holding the destination.
    pub available_bytes: u64,
}

impl SpacePlan {
    /// Whether the destination filesystem can hold the backup.
    pub fn fits(&self) -> bool {
        self.available_bytes >= self.required_bytes
    }
}

/// Bytes the store occupies on disk right now: the truth source plus its WAL sidecars. The `-wal` frames
/// are committed data the snapshot must materialise, so they belong in the estimate; a missing sidecar
/// (checkpointed store, or a plain file in a unit test) simply contributes nothing.
fn store_bytes(db_path: &Path) -> u64 {
    let mut total = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    for ext in ["-wal", "-shm"] {
        let mut side = db_path.to_path_buf().into_os_string();
        side.push(ext);
        total += std::fs::metadata(PathBuf::from(side)).map(|m| m.len()).unwrap_or(0);
    }
    total
}

/// Estimate the backup's disk budget for `source` and read the free space at `dest`'s parent directory
/// (which must already exist — under the OS layout it is the app-data root).
///
/// Pure measurement: reads file sizes and the filesystem's free space, writes nothing.
pub fn plan_space(source: &StoreSource, dest: &Path) -> Result<SpacePlan> {
    let db = store_bytes(&source.db_path);
    let mut archive_bytes = TAR_FIXED_OVERHEAD + db + TAR_ENTRY_OVERHEAD;
    for blob in archive::list_blobs(&source.db_path)? {
        archive_bytes += blob.size_bytes + TAR_ENTRY_OVERHEAD;
    }
    let dir = dest.parent().unwrap_or_else(|| Path::new("."));
    let available_bytes = fs2::available_space(dir).map_err(|e| {
        Error::invalid(
            format!("cannot read free space at {}: {e}", dir.display()),
            format!("{} の空き容量を読めません: {e}", dir.display()),
        )
    })?;
    Ok(SpacePlan {
        archive_bytes,
        staging_bytes: db,
        required_bytes: archive_bytes + db,
        available_bytes,
    })
}

/// Render bytes as whole MiB for an error message — a raw byte count nobody can eyeball is worse than a
/// rounded one. Rounds up, so a sub-MiB requirement never reads as "needs 0 MiB".
fn mib(bytes: u64) -> u64 {
    bytes.div_ceil(1024 * 1024)
}

/// Refuse the migration when the destination filesystem cannot hold the backup, naming what is needed and
/// what is free. Separated from [`plan_space`] so a caller can *show* the plan first and only then
/// enforce it.
pub fn ensure_space(plan: &SpacePlan, dest: &Path) -> Result<()> {
    if plan.fits() {
        return Ok(());
    }
    let (need, archive, staging, free) = (
        mib(plan.required_bytes),
        mib(plan.archive_bytes),
        mib(plan.staging_bytes),
        mib(plan.available_bytes),
    );
    let dir = dest.parent().unwrap_or(dest).display().to_string();
    Err(Error::invalid(
        format!(
            "not enough free space for the pre-migration backup: needs ~{need} MiB (archive ~{archive} MiB + staging ~{staging} MiB), but only ~{free} MiB is free at {dir}. free up space and run the migration again — nothing has been changed"
        ),
        format!(
            "移行前バックアップの空き容量が足りません: 約 {need} MiB 必要です（アーカイブ 約 {archive} MiB ＋ 一時領域 約 {staging} MiB）が、{dir} の空きは 約 {free} MiB です。空き容量を確保してから移行をやり直してください（まだ何も変更していません）"
        ),
    ))
}

/// What a migration is **about to** do, handed to the caller before a byte is written.
///
/// The space numbers are the reason this exists. [`ensure_space`] already refuses a disk that cannot hold
/// the backup, with the figures in the message — but a refusal is a bad first sight of them. The surface
/// shows this instead (the CLI as a line, the GUI in its migration screen), so "needs ~X MiB, ~Y MiB
/// free" is something the human read *before* the migration either ran or refused to.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Pending {
    /// The version the store carries now.
    pub from: i64,
    /// The version the chain will carry it to.
    pub to: i64,
    /// How many steps stand between the two.
    pub steps: usize,
    /// The disk budget of the pre-migration backup that wraps them.
    pub plan: SpacePlan,
}

/// A no-op sink for callers (and tests) that do not show the human what is pending.
pub fn announce_ignore(_p: &Pending) {}

/// What a migration did.
#[derive(Debug, Clone, Serialize)]
pub struct MigrationReport {
    /// The pre-migration archive this run was wrapped in — **kept** on success, so the human still has
    /// the "before" until they trust the new state. `None` when nothing was pending: no migration, no
    /// backup.
    pub backup: Option<BackupReport>,
    /// The chain's own account of what ran (empty when the store was already current).
    pub run: Run,
    /// The pre-migration archives from earlier migrations that this run's own backup superseded, and which
    /// it therefore deleted ([`archive::sweep_superseded`]). Empty when there were none.
    pub superseded: Vec<String>,
}

impl MigrationReport {
    /// Did this change the store?
    pub fn migrated(&self) -> bool {
        self.run.migrated()
    }
}

/// Migrate `source` to the latest version, wrapped in a pre-migration backup — the whole envelope (see
/// the module docs). `base_dir` is the store's directory (what a step's file half may touch); `dest` is
/// where the archive is written and `stamp` names the aside a rollback leaves behind.
///
/// Returns without touching anything when the store has no pending step. Takes the archive destination
/// and the chain as arguments — the OS layout answers both for production ([`migrate_store`]), and a
/// test drives its own store and its own chain without an app-data root to isolate.
///
/// `announce` is called once the run is known to be real and its cost is known, and before anything is
/// written (see [`Pending`]).
pub fn migrate_into(
    source: &StoreSource,
    base_dir: &Path,
    dest: &Path,
    stamp: &str,
    steps: &'static [Step],
    announce: &mut impl FnMut(&Pending),
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<MigrationReport> {
    let from = probe_format_version(&source.db_path);
    let pending = chain::pending(from, steps);
    if pending.is_empty() {
        return Ok(MigrationReport {
            backup: None,
            run: Run { from, to: from, applied: Vec::new() },
            superseded: Vec::new(),
        });
    }

    // 1. Capture the store whole — or refuse to migrate. Both the space check and the capture itself
    //    fail *before* a step has run, so the store is untouched either way. The plan is shown first:
    //    the numbers a refusal would carry are worth more before the decision than inside the error.
    let plan = plan_space(source, dest)?;
    announce(&Pending {
        from,
        to: chain::latest_version(steps),
        steps: pending.len(),
        plan,
    });
    ensure_space(&plan, dest)?;
    let backup = archive::backup_from(source, dest, progress)?;

    // 1b. The store's rewind point is now this archive — verified, and a faithful copy of the store as it
    //     stands. Whatever earlier migrations left behind is not a rewind point any more: nothing
    //     can go back to it, and every one of them is a whole copy of the store. Sweep them here, not after
    //     the chain, so a migration that keeps failing and being retried cannot pile them up either.
    let superseded =
        archive::sweep_superseded(dest.parent().unwrap_or(Path::new(".")), PRE_MIGRATE_PREFIX, &format!(".{ARCHIVE_EXT}"), dest);

    // 2. Run the chain, holding the store's swap lock: an open that lands mid-chain is turned away with
    //    `store_busy` (retryable) rather than reading a store that is halfway between two versions. Both
    //    the lock and the engine are dropped at the end of this block — the rollback below replaces the
    //    file underneath and takes the same lock, and neither may happen under the other.
    let outcome = {
        let _swap = crate::swap_lock::hold_for_swap(&source.db_path)?;
        let engine = StoreEngine::open(&source.db_path)?;
        chain::run(&engine, base_dir, steps, progress)
    };

    match outcome {
        Ok(run) => Ok(MigrationReport { backup: Some(backup), run, superseded }),
        Err(failure) => Err(roll_back(dest, &source.db_path, stamp, &failure.to_string(), progress)),
    }
}

/// Production entry point for a store the caller has already located: the real chain, into the archive
/// path amenbo chooses ([`pre_migration_backup_path`]).
pub fn migrate_store(
    source: &StoreSource,
    base_dir: &Path,
    stamp: &str,
    announce: &mut impl FnMut(&Pending),
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<MigrationReport> {
    let dest = pre_migration_backup_path(stamp);
    migrate_into(source, base_dir, &dest, stamp, STEPS, announce, progress)
}

/// Does this device's store have a step waiting for it? The bare check [`at_startup`] makes before it
/// touches anything — one read of the store's version, no lock, no write.
///
/// Exposed because a surface has to decide **what to show** before the run begins: the GUI puts up its
/// migration screen only when there is a migration, and putting it up means not opening the store from
/// the window meanwhile. Answering that with `at_startup` itself would mean running the migration to find
/// out whether to announce it.
///
/// A `false` here is not a promise: the CLI may be mid-chain right now (this does not wait on the lock),
/// and the store may be gone or unreadable. It is the same honest, cheap look the run itself starts with,
/// and the run re-checks under the lock.
pub fn is_pending() -> bool {
    archive::enumerate_store()
        .is_some_and(|source| !chain::pending(probe_format_version(&source.db_path), STEPS).is_empty())
}

/// **The execution site**: migrate this device's store if it is behind, with the other surface
/// held at the door meanwhile. Both the CLI and the GUI call exactly this, before they open anything.
///
/// - Nothing to do → `Ok(None)`, having taken no lock and written nothing. This is the common case and it
///   costs one bare read of the store's version.
/// - Something to do → take the migration lock (**blocking**: the surface that got here second waits for
///   the first), then read the version *again* under it, because waiting is usually how you find out
///   somebody else did the work. Still pending → run the envelope.
/// - No store on this device → `Ok(None)`. A fresh install is born at the latest version; there is
///   nothing to carry forward.
pub fn at_startup(
    announce: &mut impl FnMut(&Pending),
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<Option<MigrationReport>> {
    let Some(source) = archive::enumerate_store() else {
        return Ok(None);
    };
    if chain::pending(probe_format_version(&source.db_path), STEPS).is_empty() {
        return Ok(None);
    }

    let _lock = crate::swap_lock::hold_exclusive(&migration_lock_path(&source.db_path))?;
    if chain::pending(probe_format_version(&source.db_path), STEPS).is_empty() {
        return Ok(None); // the surface we waited for migrated it.
    }

    let stamp = crate::time::Timestamp::now().0.format("%Y%m%dT%H%M%SZ").to_string();
    migrate_store(&source, &Paths::user_base(), &stamp, announce, progress).map(Some)
}

/// Put the store back the way the archive holds it, and report the migration as failed either way.
///
/// The steps that committed before the failure are real, and so is any file a step moved — the archive is
/// the only thing that undoes them. When the restore itself fails there is nothing left to try
/// automatically, so the error says so and names the archive: a half-migrated store the human does not
/// know about is the one outcome this whole envelope exists to prevent.
///
/// It rewinds ([`archive::rewind_into`]) rather than restores: the archive holds the store at the version
/// the run started from, and that *older* shape is exactly what a rollback is for. The user-facing restore
/// would carry it up the chain — re-applying the steps that just failed.
fn roll_back(
    archive_path: &Path,
    db_path: &Path,
    stamp: &str,
    failure: &str,
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Error {
    let at = archive_path.display();
    match archive::rewind_into(archive_path, &format!("{stamp}-migration-failed"), db_path, progress) {
        Ok(_) => Error::invalid(
            format!(
                "the migration failed and your store was rolled back to how it was before it started: {failure}. the pre-migration backup is kept at {at}"
            ),
            format!(
                "移行に失敗したため、開始前の状態へ丸ごと戻しました: {failure}（移行前バックアップは {at} に残しています）"
            ),
        ),
        Err(rollback) => Error::invalid(
            format!(
                "the migration failed ({failure}) and rolling back failed too ({rollback}). your store may be half-migrated — restore it from the pre-migration backup at {at} (`amenbo restore {at}`)"
            ),
            format!(
                "移行に失敗し（{failure}）、開始前の状態へ戻すことにも失敗しました（{rollback}）。ストアが移行途中のままの可能性があります——移行前バックアップ {at} から復元してください（`amenbo restore {at}`）"
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::migrate::Apply;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amenbo-migrate-{tag}-{}", crate::tmpdir::suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A store at the baseline with a canary in it, so a rollback has something to prove — the shape an
    /// older build left behind, which is what a migration is run on.
    fn store_with_a_row(dir: &Path) -> StoreSource {
        store_with_a_row_at(dir, Some(chain::BASELINE_VERSION))
    }

    /// The same store as this build creates one: born at the latest version, with no step to run.
    fn current_store_with_a_row(dir: &Path) -> StoreSource {
        store_with_a_row_at(dir, None)
    }

    /// `version` = the format version to stamp; `None` stamps this build's own (genesis).
    fn store_with_a_row_at(dir: &Path, version: Option<i64>) -> StoreSource {
        let db_path = dir.join(crate::config::STORE_FILE_NAME);
        let engine = StoreEngine::open(&db_path).unwrap();
        match version {
            Some(v) => engine.set_meta(crate::store_engine::META_FORMAT_VERSION, Some(&v.to_string())).unwrap(),
            None => engine.stamp_format_version().unwrap(),
        }
        engine
            .conn()
            .execute("INSERT INTO store_meta (key, value) VALUES ('canary', 'before')", [])
            .unwrap();
        StoreSource { db_path, bindings: vec![] }
    }

    fn canary(db_path: &Path) -> Option<String> {
        let engine = StoreEngine::open_read(db_path).unwrap();
        engine.get_meta("canary").unwrap()
    }

    const RENAME_CANARY: &[Step] = &[Step {
        to: 3,
        name: "rename the canary",
        apply: Apply::Sql("UPDATE store_meta SET value = 'after' WHERE key = 'canary';"),
    }];

    const FAILS: &[Step] = &[
        Step {
            to: 3,
            name: "rename the canary",
            apply: Apply::Sql("UPDATE store_meta SET value = 'after' WHERE key = 'canary';"),
        },
        Step { to: 4, name: "explode", apply: Apply::Sql("INSERT INTO no_such_table (x) VALUES (1);") },
    ];

    /// Where a test's pre-migration archive lands (the OS layout's answer is
    /// [`pre_migration_backup_path`]; a test names its own so no app-data root is involved).
    fn archive_at(home: &Path) -> PathBuf {
        home.join(format!("{PRE_MIGRATE_PREFIX}S.{ARCHIVE_EXT}"))
    }

    /// The archive this run takes is the store's rewind point, so the ones earlier migrations left
    /// are not — they go, and the report says so. A store with nothing pending sweeps nothing (it takes no
    /// archive, so nothing has been superseded).
    #[test]
    fn a_new_pre_migration_archive_supersedes_the_ones_before_it() {
        let home = scratch("sweep");
        let source = store_with_a_row(&home);
        let dest = archive_at(&home);

        // Two archives from earlier migrations, plus a file that is none of amenbo's business.
        let old_a = home.join(format!("{PRE_MIGRATE_PREFIX}20260101T000000Z.{ARCHIVE_EXT}"));
        let old_b = home.join(format!("{PRE_MIGRATE_PREFIX}20260202T000000Z.{ARCHIVE_EXT}"));
        let unrelated = home.join(format!("holiday-photos.{ARCHIVE_EXT}"));
        for f in [&old_a, &old_b, &unrelated] {
            std::fs::write(f, b"old").unwrap();
        }

        let report =
            migrate_into(&source, &home, &dest, "S", RENAME_CANARY, &mut announce_ignore, &mut crate::progress::ignore)
                .unwrap();

        assert_eq!(report.superseded.len(), 2, "both earlier archives went: {:?}", report.superseded);
        assert!(!old_a.exists() && !old_b.exists());
        assert!(dest.is_file(), "the rewind point this run took is kept");
        assert!(unrelated.is_file(), "a file that is not a pre-migration archive is not amenbo's to delete");
        std::fs::remove_dir_all(&home).ok();
    }

    /// The caller's sink hears the chain too, not only the backup that wraps it — otherwise a
    /// surface goes quiet the moment the backup is done, which is exactly when the steps start.
    #[test]
    fn the_chain_reports_itself_to_the_same_sink_the_backup_does() {
        let home = scratch("ticks");
        let source = store_with_a_row(&home);
        let dest = archive_at(&home);

        let mut phases: Vec<crate::progress::Phase> = Vec::new();
        let mut seen = |p: &Progress| {
            phases.push(p.phase);
            ControlFlow::Continue(())
        };

        migrate_into(&source, &home, &dest, "S", RENAME_CANARY, &mut announce_ignore, &mut seen).unwrap();

        assert!(phases.contains(&crate::progress::Phase::Snapshotting), "the backup was taken: {phases:?}");
        assert_eq!(
            phases.last(),
            Some(&crate::progress::Phase::Migrating),
            "and the chain ran after it, saying so: {phases:?}"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_store_with_nothing_pending_is_neither_backed_up_nor_opened_for_writing() {
        let home = scratch("current");
        let source = current_store_with_a_row(&home);
        let dest = archive_at(&home);

        let report =
            migrate_into(&source, &home, &dest, "S", STEPS, &mut announce_ignore, &mut crate::progress::ignore).unwrap();

        assert!(!report.migrated());
        assert!(report.backup.is_none(), "no migration, no backup");
        assert!(report.superseded.is_empty(), "and no rewind point to supersede anything");
        assert!(!dest.exists());
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_migration_is_wrapped_in_a_backup_that_is_kept_on_success() {
        let home = scratch("ok");
        let source = store_with_a_row(&home);
        let dest = archive_at(&home);

        // The caller is told what is coming — versions, step count, and the disk it will need — before a
        // byte is written, so the surface can show it rather than let a refusal be the first sight of it.
        let mut announced: Vec<Pending> = Vec::new();
        let mut seen = |p: &Pending| announced.push(*p);

        let report =
            migrate_into(&source, &home, &dest, "S", RENAME_CANARY, &mut seen, &mut crate::progress::ignore).unwrap();

        assert!(report.migrated());
        assert_eq!(report.run.to, 3);
        assert_eq!(canary(&source.db_path).as_deref(), Some("after"), "the step ran");
        assert!(dest.is_file(), "the pre-migration archive is kept: {}", dest.display());
        assert_eq!(report.backup.unwrap().path, dest.display().to_string());

        assert_eq!(announced.len(), 1, "announced exactly once");
        let pending = announced[0];
        assert_eq!((pending.from, pending.to, pending.steps), (2, 3, 1));
        assert!(pending.plan.required_bytes > 0, "the space it needs is in the announcement");
        std::fs::remove_dir_all(&home).ok();
    }

    /// A store with nothing pending is announced to nobody: there is no cost to show, because there is
    /// no migration.
    #[test]
    fn nothing_pending_is_not_announced() {
        let home = scratch("quiet");
        let source = current_store_with_a_row(&home);
        let dest = archive_at(&home);

        let mut announced = 0;
        let mut seen = |_: &Pending| announced += 1;

        migrate_into(&source, &home, &dest, "S", STEPS, &mut seen, &mut crate::progress::ignore).unwrap();

        assert_eq!(announced, 0);
        std::fs::remove_dir_all(&home).ok();
    }

    /// The migration's exclusion is a sidecar of its own, beside the truth source — **not** the swap lock,
    /// which the run itself takes (and which one process cannot hold twice).
    #[test]
    fn the_migration_lock_is_a_sidecar_of_its_own() {
        let db = Path::new("/data/store.sqlite");
        assert_eq!(migration_lock_path(db), Path::new("/data/store.migrate.lock"));
        assert_ne!(migration_lock_path(db), crate::swap_lock::lock_path(db));
    }

    /// What makes the two surfaces one path: the second one to arrive blocks until the first is done —
    /// it does not migrate a store somebody else is already migrating.
    #[test]
    fn the_second_surface_waits_for_the_first() {
        use std::sync::mpsc;
        use std::time::Duration;

        let home = scratch("wait");
        let db = home.join(crate::config::STORE_FILE_NAME);
        let lock = migration_lock_path(&db);

        let held = crate::swap_lock::hold_exclusive(&lock).unwrap();

        let (tx, rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let _second = crate::swap_lock::hold_exclusive(&lock).unwrap();
            tx.send(()).unwrap();
        });

        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "the second surface is still waiting at the door"
        );
        drop(held);
        rx.recv_timeout(Duration::from_secs(5)).expect("and gets in once the first is done");
        waiter.join().unwrap();
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_failed_migration_is_rolled_back_whole_and_the_archive_is_named() {
        let home = scratch("fail");
        let source = store_with_a_row(&home);
        let dest = archive_at(&home);

        let err =
            migrate_into(&source, &home, &dest, "S", FAILS, &mut announce_ignore, &mut crate::progress::ignore).unwrap_err();

        // The message is bilingual (whichever locale is active) — what it must always carry is the
        // archive it kept, so a human can act on it.
        let msg = err.to_string();
        assert!(msg.contains(PRE_MIGRATE_PREFIX), "the error names the archive it kept: {msg}");
        assert_eq!(
            canary(&source.db_path).as_deref(),
            Some("before"),
            "the committed step is undone — the store is as it started"
        );
        assert_eq!(
            probe_format_version(&source.db_path),
            chain::BASELINE_VERSION,
            "and so is its version — the one it carried before the run"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn the_space_plan_refuses_a_disk_that_cannot_hold_the_backup() {
        let home = scratch("space");
        let source = store_with_a_row(&home);
        let dest = home.join("x.amenbo-backup");

        let plan = plan_space(&source, &dest).unwrap();
        assert!(plan.required_bytes >= plan.archive_bytes, "staging is on top of the archive");
        assert!(ensure_space(&plan, &dest).is_ok(), "a scratch dir fits a tiny store");

        let full = SpacePlan { available_bytes: 0, ..plan };
        let err = ensure_space(&full, &dest).unwrap_err().to_string();
        assert!(err.contains("MiB"), "the refusal has the numbers in it: {err}");

        std::fs::remove_dir_all(&home).ok();
    }
}
