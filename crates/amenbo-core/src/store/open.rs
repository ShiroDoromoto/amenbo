//! The paths that open a store. `open_at` orchestrates the writing open — minting an identity,
//! genesis, stamping `format_version` — in one place; `open_read_at` is the lightweight read-only
//! open that skips all of it (zero DDL: a `query_only` engine queried with read-model SQL and
//! nothing else). `init` is genesis. Neither open hydrates a `Database` — the engine is the source
//! of truth, and reads go straight to it as SQL. **Open does not migrate**: carrying an old store
//! forward is the job of the chain of version-keyed, numbered steps
//! ([`crate::store_engine::migrate::run`]), and the only site that runs it is
//! [`crate::migrate::at_startup`] (the one path both CLI and GUI go through). So open never touches
//! an existing store's `format_version` either — it stamps only at genesis, and that stamp records
//! one fact: the store this build just created was born in the newest shape and needs no step of the
//! chain. What is left in open is **gates and nothing else**: read-only checks that refuse, by name
//! and without writing a byte, a store this build cannot read — [`ensure_truth_source_in_place`]
//! (the truth source is not where this build reads it: a pre-consolidation layout, or the old file
//! name), [`ensure_format_supported`] (the store is **too new**), [`ensure_integer_keyed`] (the
//! pre-consolidation key space). Every store the gates refuse sits **below** the migration chain's
//! baseline ([`crate::store_engine::migrate::BASELINE_VERSION`]), so no step can be written for it
//! (the chain can only carry versions above the baseline). Nor can the gates be phrased in terms of
//! versions: below the baseline, the version a store claims cannot be trusted (a missing one — v0 —
//! means "not stamped yet", not "old"), so what justifies the refusal is not a version but **the
//! shape on disk itself**.

use std::fs;

use crate::config::Config;
use crate::config::Paths;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::store_engine::StoreEngine;

use super::{ensure_dir, StartupHealth, Store, VersionStatus};

/// The **whereabouts** gate. If the truth source is not where this build reads it — at
/// [`crate::config::STORE_FILE_NAME`] directly under `base` — but **is on disk in some other
/// shape**, refuse and name the last build that can fold it (0.1.9). Two diagnoses (a
/// **pre-consolidation layout**: `stores/<id>/` plus `root/`, with no single DB under `base`; or the
/// **old file name**: only `oplog.sqlite` under `base`), one reason to refuse: this build can carry
/// neither shape forward. Without this gate, both turn into "an empty store": with no truth source
/// under `base`, `StoreEngine::open` concludes genesis, creates an empty `store.sqlite`, and the app
/// cheerfully shows zero projects and zero tasks — the data sits untouched right next to it, but to
/// the user it is as if everything was erased. **Neither shape is repaired here**: renaming away
/// from the old file name would be a repair that writes during open, and it would land straight in
/// the key-space gate ([`ensure_integer_keyed`]) anyway (a store still under the old name never went
/// through the consolidation) — a repair with nowhere to land is not a repair. The layout is checked
/// first because the `oplog.sqlite` lying under `base` in the old layout is a **fossil** from the
/// root-store era, not the truth source ([`crate::archive::resolve_store_file`] is authoritative) —
/// mistaking the fossil for "a store under the old name" would tell the user to fix the wrong shape.
fn ensure_truth_source_in_place(base: &std::path::Path) -> Result<()> {
    if base.join(crate::config::STORE_FILE_NAME).exists() {
        return Ok(()); // The truth source is where this build reads it (the normal case).
    }
    if crate::archive::has_legacy_layout(base) {
        return Err(Error::invalid(
            "this device's store predates the consolidation — it is still N stores under `stores/` plus `root/`, \
             and this build reads one database and can no longer fold that layout. install amenbo 0.1.9 (the last \
             build that folds), run `amenbo migrate` there, then update again (nothing is lost: the data is \
             untouched where it is, and that migration backs the store up before it moves anything)",
            "この端末のストアは統合前のレイアウト（`stores/` の N ストア ＋ `root/`）のままです。このビルドは単一の \
             データベースを読み、この形をもう畳めません。amenbo 0.1.9（畳み込みを行う最後の版）を入れて `amenbo \
             migrate` を実行してから、改めて更新してください（失われるものはありません: データはそのまま残っており、 \
             その移行は動かす前にストアをバックアップします）",
        ));
    }
    if !base.join(crate::config::LEGACY_STORE_FILE_NAME).exists() {
        return Ok(()); // No truth source anywhere — this is genesis (we are about to create one).
    }
    Err(Error::invalid(
        "this device's store still carries the old truth-source name (`oplog.sqlite`) — it has not been opened \
         for writing since before the consolidation, and this build neither renames it nor reads the \
         pre-consolidation shape behind it. install amenbo 0.1.9 (the last build that migrates it), run \
         `amenbo migrate` there, then update again (nothing is lost: that migration backs the store up before \
         it moves anything)",
        "この端末のストアは旧称（`oplog.sqlite`）のままです。統合より前から書き込みで開かれていない世代で、この \
         ビルドはリネームもしなければ、その先の統合前の形も読めません。amenbo 0.1.9（移行を行う最後の版）を入れて \
         `amenbo migrate` を実行してから、改めて更新してください（失われるものはありません: その移行は動かす前に \
         ストアをバックアップします）",
    ))
}

/// The forward-migration version gate. If the `format_version` the store recorded is **above** what
/// the running binary supports ([`crate::model::FORMAT_VERSION`] — the version this build stamps,
/// i.e. the highest it supports), we have an **old binary against a new store**: the store ran ahead
/// and the data is physically outside this build's assumptions, so fail immediately with a
/// bilingual hard error. A warning-and-continue is not an option — the very next query would crash
/// on a raw SQLite error. The test is exactly "`store.format_version` > this binary's ceiling", so a
/// missing version (v0, the compatibility baseline) is never above the ceiling and existing stores
/// still open. The error carries its own stable code, `format_ahead` ([`Error::format_ahead`]): the
/// GUI stays resident after launch and can be overtaken by a newer process, so it has to catch this
/// one condition — and nothing else — to show its full-screen "please restart" state, which means it
/// cannot be folded in with the other `invalid_value` errors. The wording **leads with restart**:
/// GUI and CLI ship together, so a user who hits this gate **already has the new build on disk** —
/// only the process in memory is stale, and what they need is a restart, not an update (the update
/// advice comes after it; an old binary cannot know whether a newer one is installed, so both have
/// to be offered, in that order). **Run this before `StoreEngine::open`**: the engine's `init`
/// issues the registry DDL before handing back a connection, so reading the version through the
/// engine would mean the old binary had already written to the new store before the hard error
/// fired — which is no gate at all. Read the version with a bare, DDL-free connection instead. Every
/// path that opens the engine goes through this gate: the writing open [`Store::open_at`], the
/// read-only open [`Store::open_read_at`], the already-initialized check in [`Store::init`] (via
/// [`crate::store_engine::probe_is_populated`], which does not open the engine), the staging step of
/// a full restore (`archive::stage_snapshot`), and the phantom-store check in `doctor --fix`
/// ([`crate::store::store_file_is_content_empty`]).
pub(crate) fn ensure_format_supported(stamp: &crate::store_engine::FormatStamp) -> Result<()> {
    let max = crate::model::FORMAT_VERSION;
    let store_format = stamp.version;
    if store_format <= max {
        return Ok(());
    }
    // **Name** the version to use. The store carries it itself: the app version that stamped the
    // format version (`format_version_set_by`) is, by definition, an app version that can open this
    // store — no network needed. Older stores that carry no name fall back to the generic "reinstall
    // from the latest installer".
    let (en_fix, ja_fix) = match stamp.set_by.as_deref() {
        Some(app) => (
            format!("use amenbo {app} or newer — that is the version that wrote this store"),
            format!("amenbo {app} 以降を使ってください（このストアを書いたのがその版です）"),
        ),
        None => (
            "reinstall from the latest installer (GUI + CLI ship together) or run `amenbo update`".to_string(),
            "最新インストーラで入れ直すか `amenbo update` を実行してください（GUI と CLI は一体配布）".to_string(),
        ),
    };
    Err(Error::format_ahead(
        format!(
            "this store was updated by a newer amenbo (format v{store_format}); this build supports up to v{max}. restart amenbo — if it is already running, it is still the old process. if restarting does not help, {en_fix}. there is no downgrade: to go back, restore the pre-migration backup the update left behind"
        ),
        format!(
            "このストアは新しい amenbo（format v{store_format}）で更新されています。このビルドは v{max} まで対応です。amenbo を再起動してください（起動中なら、それはまだ旧いプロセスです）。再起動しても直らなければ、{ja_fix}。版を下げる道はありません——戻すなら、更新時に残した移行前バックアップから復元してください"
        ),
    ))
}

/// The key-space gate. A store whose record tables are still ULID-keyed (`TEXT`) holds
/// pre-consolidation (pre-fold) content. **This build cannot re-key it**, so it fails with the same
/// advice as [`ensure_truth_source_in_place`]: migrate with the last build that consolidates (0.1.9),
/// then update. It looks at something different from the whereabouts gate: that one looks at the
/// shape on disk (`stores/` plus `root/`, or the old file name), this one at the **generation of the
/// content** inside the single file this build reads. A machine that set `AMENBO_HOME` never had the
/// old layout, so it sails past the first gate and lands here. Let it through silently and the
/// engine's `CREATE TABLE IF NOT EXISTS` will put integer-keyed tables next to ULID-keyed ones,
/// leaving a store that is neither.
pub(crate) fn ensure_integer_keyed(engine: &StoreEngine) -> Result<()> {
    if !engine.is_legacy_keyed()? {
        return Ok(());
    }
    Err(Error::invalid(
        "this store predates the consolidation — its rows are still ULID-keyed, and this build reads the \
         integer-keyed shape and can no longer re-key one. install amenbo 0.1.9 (the last build that \
         migrates it), run `amenbo migrate` there, then update again (nothing is lost: that migration \
         backs the store up before it moves anything)",
        "このストアは統合前の世代です（行がまだ ULID キー）。このビルドは INTEGER キーの形を読み、再キーはもう \
         できません。amenbo 0.1.9（移行を行う最後の版）を入れて `amenbo migrate` を実行してから、改めて更新して \
         ください（失われるものはありません: その移行は動かす前にストアをバックアップします）",
    ))
}

/// The key-space gate for the writing open: peek at the live file over a `query_only` connection
/// **before** opening the engine.
///
/// The only thing it has to tell apart is genesis: a file with no tables at all (missing, or just
/// created by `Connection::open`) has nothing to lose, and the engine's `CREATE TABLE` is the right
/// answer for it. If the file cannot be read (corrupt, or a never-migrated at-rest-encrypted store),
/// pass silently — with nothing to go on, inventing an error here is less accurate than letting the
/// `StoreEngine::open` that follows return the real one.
fn ensure_integer_keyed_at(db_path: &std::path::Path) -> Result<()> {
    if !db_path.is_file() {
        return Ok(()); // genesis
    }
    let Ok(engine) = StoreEngine::open_read(db_path) else { return Ok(()) };
    if !engine.has_any_table().unwrap_or(false) {
        return Ok(()); // An empty file is genesis.
    }
    ensure_integer_keyed(&engine)
}

impl Store {
    /// Open the store at the default location, creating and initializing a default one if none exists.
    pub fn open() -> Result<Store> {
        let paths = Paths::resolve()?;
        Store::open_at(paths)
    }

    pub fn open_at(paths: Paths) -> Result<Store> {
        let _perf = crate::perf::Timer::start("store.open_at");
        // Write exclusion is left to SQLite's own writer lock (per transaction) plus `busy_timeout`.
        ensure_dir(&paths.base_dir)?;

        // Restore and migration (the swap) take this store's `store.swap.lock` exclusively. Take it
        // shared for the duration of this open so we never read a store mid-swap (if it is held, we
        // fail with `store_busy`). It is held only until open returns, not for the life of the store
        // — holding it longer would self-deadlock against `restore`, which takes `&mut self`.
        let _swap = crate::swap_lock::guard_write_open(&paths.store_file)?;

        // The identity (display name, bound_hw) is local. If it still lives in the old vault layout
        // (`accounts/P0/`), lift it to sit directly under `base` before reading it (one-way,
        // idempotent) — there is exactly one place we read it from, and we mint one if it is absent.
        crate::config::lift_legacy_identity(&paths.base_dir)?;
        let (mut identity, mut identity_dirty) = if paths.identity_file.exists() {
            (Identity::load(&paths.identity_file)?, false)
        } else {
            (Identity::generate("ローカルユーザー"), true)
        };
        // Settle the config before the store proper (the config file is independent of it).
        let config = if paths.config_file.exists() {
            let raw = fs::read_to_string(&paths.config_file)?;
            serde_json::from_str::<Config>(&raw)?
        } else {
            Config::default()
        };

        // If the store looks copied to a different machine (bound_hw mismatch), rebind bound_hw to
        // the current one.
        let mut forked = false;
        if identity.hw_mismatch() {
            identity.rebind_hw();
            identity_dirty = true;
            forked = true;
        }
        let new_identity = identity_dirty;

        // If the truth source is not where this build reads it (an unfolded layout, or a file under
        // the old name), refuse here and say so — this is the gate that keeps an empty genesis from
        // painting over it.
        ensure_truth_source_in_place(&paths.base_dir)?;
        // Open the truth-source engine next to the store proper (`base_dir/store.sqlite`). If it
        // cannot be opened there is no truth, so the open itself fails. Past the gate, the truth
        // source has exactly one name (a store under the old name never gets this far).
        let db_path = paths.base_dir.join(crate::config::STORE_FILE_NAME);
        // The truth source is plaintext SQLite with no application-level at-rest encryption
        // (confidentiality on the machine is left to OS full-disk encryption — FileVault, BitLocker).
        // There is no at-rest key derivation either, so a legacy encrypted store can no longer be
        // decrypted and migrated here: it has to be opened once by a build that still holds the key
        // and moved to plaintext (we fail explicitly instead). New and already-plaintext stores pass
        // straight through.
        let at_rest = crate::store_engine::at_rest_status(&db_path);
        if at_rest.exists && !at_rest.plaintext {
            return Err(crate::error::Error::invalid(
                "this store is still encrypted at rest; open it once with an older build that still carries the at-rest key to migrate it to plaintext before using this build",
                "このストアはまだ at-rest 暗号化されています。at-rest 鍵を持つ旧いビルドで一度開いて平文へ移行してから、このビルドで使ってください",
            ));
        }
        // The version gate. If the store has been migrated forward past what the running binary
        // supports, fail here with the bilingual "update" hard error, before this build's read SQL
        // reaches for a column or table that is gone and crashes on a raw SQLite error. Existing
        // stores with no stamp (v0) are never above the ceiling and pass through. Read the version
        // from a bare connection **before opening the engine**: `StoreEngine::open` issues the
        // registry DDL before handing back a connection, so reading it through the engine would mean
        // the old binary had already written to the new store before the gate fired.
        ensure_format_supported(&crate::store_engine::probe_format_stamp(&db_path))?;
        // Refuse a store in the pre-consolidation key space (ULID keys) before the engine can create
        // integer-keyed tables alongside it.
        ensure_integer_keyed_at(&db_path)?;
        let engine = StoreEngine::open(&db_path)?;
        let engine_populated = engine.is_populated()?;

        // **This open does not migrate the truth source**: if the engine is already populated (the
        // normal case for an existing store) it writes nothing and simply reads. Only when the engine
        // is empty — nothing to lose, i.e. a brand-new store — do we stamp genesis and flush the
        // config to disk (a new store has no config.json yet).
        let config_dirty = !engine_populated;
        if !engine_populated {
            // Genesis. Stamp the store-level scalars (**only at genesis**).
            engine.set_meta(
                crate::store_engine::META_SCHEMA_VERSION,
                Some(crate::model::SCHEMA_VERSION),
            )?;
            // **This is the only place the format version is stamped.** For a store born at genesis
            // the stamp records a fact: it came into existence in this build's newest shape, so no
            // step of the chain needs to run against it. The only thing allowed to advance an
            // existing store's version is the migration chain (`crate::store_engine::migrate::run`)
            // — an unconditional stamp in open would claim a migration that never ran, and stores
            // that were never migrated would start claiming the new version.
            engine.stamp_format_version()?;
        }

        if new_identity {
            ensure_dir(&paths.base_dir)?;
            identity.save(&paths.identity_file)?;
        }

        let mut store = Store {
            config,
            paths,
            identity,
            forked,
            startup_check: None,
            engine,
            reach: crate::reach::Reach::default(),
        };
        // The domain data already lives in the engine (genesis wrote it there above). All that is
        // left is the config — flush its genesis defaults to the file.
        if config_dirty {
            store.persist()?;
        }
        // The read-only integrity check at startup (on by default). Findings come back as warnings
        // for the caller to display. Inspection only — it never repairs anything.
        if store.config.startup_integrity_check {
            store.startup_check = Some(store.compute_startup_health()?);
        }
        Ok(store)
    }

    /// **The lightweight, read-only open.** It shrinks the fixed cost the GUI's read commands pay per
    /// IPC round-trip by dropping every side effect of the writing path (minting an identity,
    /// genesis, stamping `format_version`). All it does is read the identity and config (cheap) and
    /// open the `store.sqlite` beside them (the truth source, which is also the read model); the
    /// reads themselves are served by the indexed SQL in `store_engine::read::*`. **It writes nothing
    /// to disk** — the engine is opened with [`StoreEngine::open_read`], which runs no DDL and sets
    /// `PRAGMA query_only = ON`, so that is not a promise made by this doc comment but an invariant
    /// **SQLite enforces**. It does not wait on writers either (reads see a WAL snapshot and proceed
    /// concurrently with one). If the engine is missing or empty — a new store that `open_at` has
    /// never populated — it defers to the writing `open_at`, which creates genesis. That the card
    /// DTOs from `open_read_at` and `open_at` agree is guarded by `read_open_matches_full_open`.
    pub fn open_read_at(paths: Paths) -> Result<Store> {
        let _perf = crate::perf::Timer::start("store.open_read");
        // No identity means no read model either (the store has never been opened) — defer to the
        // full open.
        if !paths.identity_file.exists() {
            return Store::open_at(paths);
        }
        // Read-only means we write nothing to disk, so we look for **this build's truth-source name
        // and nothing else** — the read path has neither a way to repair a store under the old name
        // or an unfolded layout, nor the wording to refuse it. If the truth source is not here, defer
        // to the writing `open_at` and let it decide whether to create genesis or refuse by name.
        let db_path = paths.base_dir.join(crate::config::STORE_FILE_NAME);
        if !db_path.exists() {
            return Store::open_at(paths);
        }
        // Mid-swap, fail with `store_busy` instead of opening. Because the read path writes nothing
        // to disk it **never creates** the lock sidecar (absent means no swap in progress, so we pass).
        // The guard is held only for the duration of this open.
        let _swap = crate::swap_lock::guard_read_open(&db_path)?;
        let identity = Identity::load(&paths.identity_file)?;
        // Settle the config before opening the engine (it goes on the `Store`).
        let config = if paths.config_file.exists() {
            let raw = fs::read_to_string(&paths.config_file)?;
            serde_json::from_str::<Config>(&raw).unwrap_or_default()
        } else {
            Config::default()
        };
        // The read path goes through the version gate too. It writes nothing to disk (it never
        // migrates forward), but its read-model SQL assumes the new binary's schema, so a store
        // migrated past the supported ceiling would break reads with a raw SQLite error just the
        // same. As on the writing path, read the version from a bare connection before opening the
        // engine.
        ensure_format_supported(&crate::store_engine::probe_format_stamp(&db_path))?;
        // Open the engine with `open_read`, which **runs no DDL** (`query_only = ON`) — a lock-free
        // read must never rewrite the physical schema of a store another process is writing to.
        //
        // The truth source is plaintext SQLite. On a legacy encrypted store the first query in
        // `is_populated` fails and we drop into the `_ => open_at` fallback below (`open_at` does not
        // decrypt and migrate; it fails explicitly). An engine that is merely empty (never populated)
        // takes the same fallback and lets `open_at` do genesis.
        let engine = match StoreEngine::open_read(&db_path) {
            Ok(e) if matches!(e.is_populated(), Ok(true)) => e,
            _ => return Store::open_at(paths),
        };
        // A store in the pre-consolidation key space (ULID keys) is refused by name on this path too.
        ensure_integer_keyed(&engine)?;
        // The startup integrity check (doctor) folds over everything — even against the read model it
        // is a full-table aggregate, O(total) — so the read open does not compute it: paying that on
        // every IPC read would blow the budget. Instead `build_snapshot`, which needs the health
        // display and is itself a whole-store aggregate (and therefore outside the budget), calls
        // `compute_startup_health` when it needs it. The writing `open_at` still computes it at open
        // time and puts it on `startup_check`.
        Ok(Store {
            config,
            paths,
            identity,
            forked: false,
            startup_check: None,
            engine,
            reach: crate::reach::Reach::default(),
        })
    }

    /// The version / format state of this store ([`VersionStatus`]), as surfaced by `doctor`,
    /// `--version` and `agent --json`. `format_version` is read from the truth-source engine; if it
    /// cannot be read we report v0, which is safe because this is display-only.
    pub fn version_status(&self) -> VersionStatus {
        let format_version = self.engine.format_version().unwrap_or(0);
        VersionStatus {
            app_version: crate::agent::VERSION,
            format_version,
            max_supported_format: crate::model::FORMAT_VERSION,
            // Only upstream (latest.json) can say whether an update exists, and asking costs a
            // network call, so `version_status()` alone never claims one — this is set only when the
            // caller runs the result through `VersionStatus::with_upstream`.
            update_available: false,
            newer_version: None,
            latest_version: None,
        }
    }

    /// Compute the read-only startup integrity check: the internal consistency of the truth source
    /// (orphaned and dangling references, and so on). Pure inspection — no side effects, no automatic
    /// repair — with every kind queried through the read model's indexed SQL (`doctor`). The writing
    /// `open_at` calls this at open time and puts the result on `startup_check`; the read path cannot
    /// afford a full-table aggregate on every IPC read, so `open_read_at` does not call it and
    /// `build_snapshot` — which needs the health display — calls it on demand instead (the lazy entry
    /// point). What it looks at is **the inside of the store only**: environment issues (the `.amenbo`
    /// pointer and the managed block in bound folders, which [`crate::doctor::report`] gathers) are
    /// deliberately not included, because this runs on every snapshot — in the GUI, on every
    /// store-changed tick — and a filesystem walk per bound folder has no business there (the
    /// environment changes independently of the store, so there is nothing to gain from re-checking
    /// it every tick; the GUI surfaces it from a dedicated check run once at app startup).
    pub fn compute_startup_health(&self) -> Result<StartupHealth> {
        let doctor = self.doctor()?;
        Ok(StartupHealth { doctor })
    }

    /// Genesis initialization. `name` becomes the first local user's name. Errors if the store is
    /// already initialized (the engine has content).
    pub fn init(paths: Paths, name: Option<&str>) -> Result<Store> {
        // Refuse a store under the old name (`oplog.sqlite`) or an unfolded layout at the gate, ahead
        // of the already-initialized check. That check only looks for the current file name, so
        // without the gate it would read "no truth source" as "not initialized" and genesis would
        // create an empty store right next to the existing data.
        ensure_truth_source_in_place(&paths.base_dir)?;
        let db_path = paths.base_dir.join(crate::config::STORE_FILE_NAME);
        // The truth source is plaintext, so probe (side-effect free) whether it is populated as
        // plaintext. On a legacy encrypted store that has not been migrated the plaintext probe
        // fails and cannot conclude "already initialized" — but that store is caught by the explicit
        // error in the `open_at` below, so genesis never tramples existing data. This probe **does
        // not open the engine** (a bare connection reading one row of `store_meta`): peering through
        // the engine would run migration DDL before the version gate inside `open_at` — an old
        // binary's `amenbo init` would rewrite a newer store's schema before it even got to answer
        // "already initialized".
        if crate::store_engine::probe_is_populated(&db_path) {
            return Err(Error::conflict(
                "this store is already initialized",
                "この store は既に初期化済みです",
            ));
        }
        // Lift an identity from the old vault layout to sit under `base` before deciding whether to
        // mint one — do it the other way round and a store whose display name lives in the old layout
        // gets a freshly minted identity written over it.
        crate::config::lift_legacy_identity(&paths.base_dir)?;
        if !paths.identity_file.exists() {
            ensure_dir(&paths.base_dir)?;
            Identity::generate(name.unwrap_or("ローカルユーザー")).save(&paths.identity_file)?;
        }
        let mut store = Store::open_at(paths)?;
        // The human facet's display name of record lives in `config.human_name`. Seed it from the init
        // name so name-based assignee resolution (`--to <name>`) and the roster resolve to this subject;
        // the caller persists it via `store.save_config()`.
        if let Some(n) = name.map(str::trim).filter(|s| !s.is_empty()) {
            store.config.human_name = Some(n.to_string());
        }
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("amenbo-open-{tag}-{}", crate::tmpdir::suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Opening a machine still on the old vault layout (`accounts/P0/identity.json`) lifts the
    /// identity to sit under `base`, and **the display name survives** — fail to lift it and we mint
    /// a fresh identity and lose the name.
    #[test]
    fn a_legacy_vault_identity_is_lifted_to_the_base_and_keeps_its_name() {
        let base = scratch("legacy-vault-identity");
        let legacy_dir = base.join("accounts").join("P0");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        crate::identity::Identity::generate("Alice")
            .save(&legacy_dir.join(crate::config::IDENTITY_FILE_NAME))
            .unwrap();

        let store = Store::open_at(Paths::at(base.clone())).expect("a machine on the old layout opens too");

        assert_eq!(store.identity.user_name, "Alice", "the old vault's display name carries over");
        assert!(
            base.join(crate::config::IDENTITY_FILE_NAME).is_file(),
            "the identity has been lifted to sit directly under base"
        );
        assert!(
            !legacy_dir.join(crate::config::IDENTITY_FILE_NAME).exists(),
            "nothing is left on the old layout (there is one place to read it)"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// A pre-consolidation machine (the store lives under `stores/<id>/`, with no single DB under
    /// `base`) is refused by name. Silently doing genesis would bring up an empty app in place of the
    /// store still sitting untouched under `stores/`.
    #[test]
    fn an_unfolded_layout_is_refused_with_the_migrate_command() {
        let base = scratch("unfolded");
        let store_dir = base.join("stores").join("01KVK43WVK4QVHQGWXF7DRNSYX");
        std::fs::create_dir_all(&store_dir).unwrap();
        std::fs::write(store_dir.join(crate::config::STORE_FILE_NAME), b"SQLite format 3\0").unwrap();

        let err = ensure_truth_source_in_place(&base).expect_err("a legacy layout must not open");
        assert!(
            err.message_en().contains("amenbo migrate"),
            "names the way forward: {}",
            err.message_en()
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// A store still under the old name (`oplog.sqlite`) is refused by the same gate. "Fixing" it
    /// with an automatic rename would only land it in the key-space gate (its content is
    /// pre-consolidation) — refusing by name is both more accurate and lets open write nothing.
    #[test]
    fn a_store_under_the_legacy_name_is_refused_and_left_alone() {
        let base = scratch("legacy-name");
        let legacy = base.join(crate::config::LEGACY_STORE_FILE_NAME);
        std::fs::write(&legacy, b"SQLite format 3\0").unwrap();

        let err = ensure_truth_source_in_place(&base).expect_err("a store under the old name must not open");

        assert!(err.message_en().contains("0.1.9"), "names the build that migrates it: {}", err.message_en());
        assert!(legacy.is_file(), "and the store is left where it is — the gate does not write");
        assert!(
            !base.join(crate::config::STORE_FILE_NAME).exists(),
            "no empty store is created beside it"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// A machine whose truth source is under the current name passes, even with leftovers from
    /// `stores/` next to it — that is what a folded store normally looks like.
    #[test]
    fn a_store_where_this_build_reads_it_passes_the_gate() {
        let base = scratch("in-place");
        std::fs::create_dir_all(base.join("stores").join("01KVK43WVK4QVHQGWXF7DRNSYX")).unwrap();
        std::fs::write(base.join(crate::config::STORE_FILE_NAME), b"SQLite format 3\0").unwrap();

        assert!(ensure_truth_source_in_place(&base).is_ok(), "a folded store passes straight through, leftovers and all");
        // A machine with no truth source anywhere is genesis, and passes as well.
        assert!(ensure_truth_source_in_place(&scratch("genesis")).is_ok());
        std::fs::remove_dir_all(&base).ok();
    }

    /// Even once it is a single file, a store whose content is pre-consolidation (ULID-keyed) is
    /// refused by name. There is no way to re-key it, and letting it through would have the migration
    /// chain create integer-keyed tables beside the ULID-keyed ones, leaving a store that is neither.
    #[test]
    fn a_ulid_keyed_store_is_refused_with_the_build_that_can_migrate_it() {
        let base = scratch("ulid-keyed");
        let db = base.join(crate::config::STORE_FILE_NAME);
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE task (id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL DEFAULT '');",
            )
            .unwrap();
        }

        let engine = StoreEngine::open_read(&db).unwrap();
        let err = ensure_integer_keyed(&engine).expect_err("a pre-consolidation store must not open");
        assert!(
            err.message_en().contains("0.1.9"),
            "names the last build that can migrate it: {}",
            err.message_en()
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// `oplog.sqlite` under the old name is the truth source only when the directory holding it *is*
    /// the store. A file of the same name lying under `base` in the old layout is a fossil from the
    /// root-store era, not the truth source. The gate looks at the layout first, so it does not
    /// misdiagnose the fossil as "a store under the old name" and instead names the shape that
    /// actually has to move (the unfolded layout).
    #[test]
    fn a_fossil_beside_the_legacy_layout_is_not_the_truth_source() {
        let base = scratch("fossil");
        let store_dir = base.join("stores").join("01KVK43WVK4QVHQGWXF7DRNSYX");
        std::fs::create_dir_all(&store_dir).unwrap();
        std::fs::write(store_dir.join(crate::config::STORE_FILE_NAME), b"SQLite format 3\0").unwrap();
        // Put an unopenable (encrypted) file under the old name directly under `base`.
        std::fs::write(base.join(crate::config::LEGACY_STORE_FILE_NAME), b"\x66\xfa\x56\xa4not sqlite").unwrap();

        let err = ensure_truth_source_in_place(&base).expect_err("the layout is what has to move");

        assert!(
            err.message_en().contains("stores/"),
            "the unfolded layout is the diagnosis, not the fossil's name: {}",
            err.message_en()
        );
        assert!(
            base.join(crate::config::LEGACY_STORE_FILE_NAME).exists(),
            "and the fossil is left alone — the gate writes nothing"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    /// The no-downgrade gate: a store that is too new is not opened, and the refusal **names the
    /// version required**.
    #[test]
    fn the_gate_names_the_app_version_that_wrote_a_too_new_store() {
        let stamp = crate::store_engine::FormatStamp {
            version: crate::model::FORMAT_VERSION + 1,
            set_by: Some("9.9.9".into()),
        };

        let err = ensure_format_supported(&stamp).unwrap_err();

        assert!(matches!(err, Error::FormatAhead(_)), "it falls to the dedicated code: {err}");
        let msg = err.to_string();
        assert!(msg.contains("9.9.9"), "it names the version: {msg}");
    }

    /// An older store that names no version (written before this key was stamped) falls back to
    /// "reinstall" — being unable to name a version is no reason to let it through.
    #[test]
    fn a_too_new_store_with_no_name_still_refuses() {
        let stamp = crate::store_engine::FormatStamp {
            version: crate::model::FORMAT_VERSION + 1,
            set_by: None,
        };

        let err = ensure_format_supported(&stamp).unwrap_err();

        assert!(matches!(err, Error::FormatAhead(_)));
        assert!(err.to_string().contains("amenbo"));
    }

    /// A store at or below the supported ceiling passes, including v0 (a missing stamp — the
    /// compatibility baseline).
    #[test]
    fn a_store_this_build_supports_passes() {
        for version in [0, crate::model::FORMAT_VERSION] {
            let stamp = crate::store_engine::FormatStamp { version, set_by: None };
            assert!(ensure_format_supported(&stamp).is_ok(), "v{version} opens");
        }
    }

    /// The stamping side: the app version that stamped the format version is recorded alongside it —
    /// that is the value a later (older) build's gate names.
    #[test]
    fn stamping_the_format_version_records_the_app_version_that_did_it() {
        let engine = crate::store_engine::StoreEngine::open_in_memory().unwrap();
        engine.stamp_format_version().unwrap();

        let stamp = crate::store_engine::read_format_stamp(engine.conn()).unwrap();
        assert_eq!(stamp.version, crate::model::FORMAT_VERSION);
        assert_eq!(stamp.set_by.as_deref(), Some(crate::agent::VERSION));
    }

    /// The swap gate: while the file is being swapped ([`crate::swap_lock`] held exclusively),
    /// **both** opens fail immediately with `store_busy`, so nobody reads a store mid-swap. Once it
    /// is released they open as before. Restore and migration ([`crate::archive`]) are what swap, and
    /// both take this lock.
    #[test]
    fn a_held_swap_lock_makes_opens_store_busy() {
        let base = scratch("busy-open");
        let paths = Paths::at(base.clone());
        {
            // Materialize the store and the identity, then close.
            Store::open_at(paths.clone()).unwrap();
        }
        let db = base.join(crate::config::STORE_FILE_NAME);

        let held = crate::swap_lock::hold_for_swap(&db).unwrap();
        match Store::open_at(paths.clone()) {
            Err(e) => assert_eq!(e.code(), "store_busy", "the write open refuses mid-swap"),
            Ok(_) => panic!("the write open should refuse mid-swap"),
        }
        match Store::open_read_at(paths.clone()) {
            Err(e) => assert_eq!(e.code(), "store_busy", "the read open refuses mid-swap"),
            Ok(_) => panic!("the read open should refuse mid-swap"),
        }

        drop(held);
        Store::open_at(paths.clone()).expect("write open after the swap released");
        Store::open_read_at(paths).expect("read open after the swap released");
        std::fs::remove_dir_all(&base).ok();
    }
}
