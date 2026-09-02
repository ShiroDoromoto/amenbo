//! The error type the whole core speaks in. The CLI puts this `code()` straight into
//! `{ "error": { "code", "message" } }`: the `--json` error contract *is* this code — the stable code
//! [`Error::code`] returns below.
//!
//! i18n layering:
//! The machine/agent surface (`--json`'s `code`) is a fixed-English contract, and `code()` here is its
//! canonical source. **The prose is English too, everywhere** — there is one sentence per failure, the
//! `Display` one, and [`Error::message_en`] is the name the CLI's all-English surface calls it by. No
//! second language is written here: nineteen of them cannot be, and the side that holds the dictionary is
//! the side that can write them (`AMB-D-396` / `AMB-D-413`).
//!
//! The GUI reads that dictionary instead of this prose. Tauri commands return failures as a structured
//! `CmdError` (`app/src-tauri/src/error.rs`) carrying `code()` (the stable code), the sentence, and
//! `fields` for interpolation; the front end (`errLabel` in `app/src/core/i18n/index.ts`) maps `code` onto
//! per-language templates. A code with no template falls back to the English sentence, which is the same
//! answer an untranslated key gets.
//!
//! Which is why a code has two grains ([`ErrorCode`]). A template can only be written for a code that means
//! one sentence, so the refusals the GUI shows a person carry a **sentence** code ([`Msg::coded`]) and send
//! the values that sentence is built from ([`Msg::with`]) instead of only the prose. Everything else keeps
//! its variant's **family** code and reads in English, which is the settled answer for a surface no one is
//! translating (`AMB-D-413`).

use std::path::Path;

use rusqlite::Connection;
use thiserror::Error;

use crate::store_engine::StoreEngineError;

/// Map a raw SQLite error into the core [`enum@Error`], naming the file the failure came from. A bare
/// `file is not a database` — SQLite says nothing about *which* file — leaves nobody able to tell which
/// store it was. Anything that reads a store by path maps its SQLite errors through this.
pub(crate) fn sqlite_at(path: &Path) -> impl Fn(rusqlite::Error) -> Error + '_ {
    move |e| {
        let e = StoreEngineError::from(e);
        // Contention is not this store failing, so it is not named as one — see the crossing in
        // `From<StoreEngineError>`, which is where the sentence lives.
        if e.is_contention() {
            return Error::from(e);
        }
        Error::Storage(format!("{}: {}", path.display(), e))
    }
}

/// [`sqlite_at`], for code that holds only a connection. The connection knows which file it is
/// open on, so the name is read back from it rather than threaded down beside it — a path passed as an
/// extra argument can drift from the database actually being read, this one cannot. It names the
/// connection's **main** database and nothing else.
pub(crate) fn sqlite_on(conn: &Connection) -> impl Fn(rusqlite::Error) -> Error + '_ {
    move |e| engine_on(conn)(StoreEngineError::from(e))
}

/// [`sqlite_on`], for a failure that has already been raised to a [`StoreEngineError`] — the read-model
/// (`store_engine::read`) maps SQLite itself, so its callers never see the raw error and cannot use
/// `sqlite_on`. The naming rule is the same: the connection says which file it is open on.
pub(crate) fn engine_on(conn: &Connection) -> impl Fn(StoreEngineError) -> Error + '_ {
    move |e| match (&e, conn.path()) {
        // A reach refusal names no file: it is not the store that failed, and naming the SQLite
        // path would leak the store's location into a containment message. It crosses unchanged.
        (StoreEngineError::OutOfReach(_), _) => Error::from(e),
        // Nor did the store fail when it is merely held: the crossing above turns that into
        // `store_busy`, and prefixing a path onto it would put a file name in front of a sentence
        // about waiting.
        (held, _) if held.is_contention() => Error::from(e),
        // An in-memory database reports no name (or an empty one) — there is nothing to name, so it falls
        // back to the bare mapping rather than printing an empty prefix.
        (_, Some(p)) if !p.is_empty() => Error::Storage(format!("{p}: {e}")),
        _ => Error::from(e),
    }
}

/// The values a sentence is built from, kept apart from the sentence itself so the side holding the
/// dictionary can write it in its own language (`AMB-D-413`). Everything is stringified on the way in:
/// a field is a value to drop into a template, never a number to compute with.
#[derive(Debug, Clone, Default)]
pub struct Fields(Vec<(&'static str, String)>);

impl Fields {
    /// The pairs, in the order they were added.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.0.iter().map(|(k, v)| (*k, v.as_str()))
    }

    /// Are there none? (A sentence that needs no value still names itself with a code.)
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// An error message: one sentence, in English. It is what the CLI contracts to print, and what a reader
/// gets wherever no dictionary was consulted — a second language is not written here, because nineteen of
/// them cannot be and the side holding the dictionary is the side that can (`AMB-D-396` / `AMB-D-413`).
///
/// Dynamic detail (ids and the like) is interpolated into the sentence by the caller.
///
/// A message may also **name itself**: [`Msg::coded`] pins a fine-grained [`ErrorCode`] to this one
/// sentence, and [`Msg::with`] sends the values that sentence is built from alongside it. That pair is
/// what lets the GUI compose the sentence in the reader's language instead of showing the English one
/// written here (`AMB-D-413`). A message that names nothing keeps its variant's coarse code and falls
/// back to the sentence, which is the answer for every surface the GUI does not show.
///
/// Some refusals are not one sentence but one sentence **plus a list of reasons**, and how many
/// reasons there are is only known at the moment of refusing — a reservation turned away because two
/// blockers stand and a premise is unsettled says three things at once. No single template can be
/// written for that, so those reasons ride as [`Msg::part`]s: each names its own sentence and carries
/// its own values, and the side holding the dictionary writes each one and joins them with the
/// punctuation its language joins with.
#[derive(Debug, Clone)]
pub struct Msg {
    en: String,
    code: Option<ErrorCode>,
    fields: Fields,
    parts: Vec<Msg>,
}

impl Msg {
    pub fn new(en: impl Into<String>) -> Self {
        Msg {
            en: en.into(),
            code: None,
            fields: Fields::default(),
            parts: Vec::new(),
        }
    }

    /// Pin the code that names **this sentence** rather than its variant's family, so a dictionary can
    /// hold a template for it.
    pub fn coded(mut self, code: ErrorCode) -> Self {
        self.code = Some(code);
        self
    }

    /// Send one of the values the sentence is built from, under the name the template interpolates it by.
    pub fn with(mut self, key: &'static str, value: impl std::fmt::Display) -> Self {
        self.fields.0.push((key, value.to_string()));
        self
    }

    /// Add one of the sentences this one is composed of, in the order it is to be read. A part names
    /// its own code and carries its own values, so the dictionary writes it like any other sentence —
    /// what it cannot be is a string composed here, which would be English inside the reader's line.
    pub fn part(mut self, part: Msg) -> Self {
        self.parts.push(part);
        self
    }

    /// The sentence.
    pub fn en(&self) -> &str {
        &self.en
    }

    /// The code naming this one sentence, if it names itself.
    pub fn code(&self) -> Option<ErrorCode> {
        self.code
    }

    /// The values the sentence is built from.
    pub fn fields(&self) -> &Fields {
        &self.fields
    }

    /// The sentences this one is composed of, in reading order. Empty for the great majority, which
    /// are one sentence and nothing else.
    pub fn parts(&self) -> &[Msg] {
        &self.parts
    }
}

impl std::fmt::Display for Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.en)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    NotFound(Msg),

    /// An abbreviated id matched more than one entity; the candidates are listed back.
    #[error("id '{prefix}' is ambiguous. candidates: {candidates:?}")]
    AmbiguousId {
        prefix: String,
        candidates: Vec<String>,
    },

    #[error("{0}")]
    Invalid(Msg),

    #[error("{0}")]
    Conflict(Msg),

    /// The compare-and-swap behind a reservation (`todo → in_progress`) lost: the target is not `todo`, so
    /// it cannot be reserved — another session already picked it up (`in_progress`), or it never passed
    /// through `todo` at all. This is the write-time guard against double-booking, and it carries its own
    /// stable code precisely so the CLI and the GUI can branch on it.
    #[error("{0}")]
    AlreadyReserved(Msg),

    /// The task being reserved has unmet preconditions: an unfinished blocker still stands, or a decision
    /// linked as its grounds is not settled (`proposed` / `rejected` / superseded). There is no `--force` —
    /// the way through is `task undepend` / `decision link --unlink` / `decision accept`.
    #[error("{0}")]
    NotReady(Msg),

    /// Something reached past the project the binding (`.amenbo`) names. An AI facet's reach is closed to
    /// the bound project and entities outside it are **invisible** to it — but silently answering "zero
    /// results" or "not found" would be a lie, and we do not deny that a thing exists. This is the code
    /// that says only "you cannot reach that from here" (see [`crate::reach::Reach`]).
    #[error("{0}")]
    OutOfReach(Msg),

    /// The path a project→dir binding points at is gone. We do not quietly go and work somewhere else.
    #[error("the linked project directory was not found: {0}")]
    BindingStale(String),

    /// The store has been migrated forward past this build's [`crate::model::FORMAT_VERSION`] — an **old
    /// binary against a newer store** (gated by [`crate::store::open::ensure_format_supported`]).
    ///
    /// It gets a stable code of its own rather than folding into `Invalid`: a long-lived GUI can be
    /// overtaken by a newer build in another process while it is still running, and it has to catch that
    /// one case to fall back to a full-screen "please restart" — which it cannot do if the case is mixed in
    /// with every other `invalid_value`.
    #[error("{0}")]
    FormatAhead(Msg),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("store (engine) operation failed: {0}")]
    Storage(String),

    #[error("the store is in use by another process: {0}")]
    StoreBusy(Msg),
}

/// The sentence core writes when the store is held by somebody else. The reader's own language comes
/// from the `store_busy` template, which says the one thing true of every way the family arises — the
/// store is in use, ask again — so this English is the log's line and the CLI's, not the screen's.
///
/// Named here rather than written at each of the three crossings below, so the three cannot drift into
/// saying different things about the same condition.
fn contended() -> Error {
    Error::store_busy("the store is in use by another program; try again in a moment")
}

impl From<crate::store_engine::StoreEngineError> for Error {
    fn from(e: crate::store_engine::StoreEngineError) -> Self {
        match e {
            // A reach refusal crosses unchanged, even when it surfaces from the engine. Rounding it into
            // `Storage` would make a containment refusal claim to be a store failure, and would erase the
            // `out_of_reach` code from the contract.
            crate::store_engine::StoreEngineError::OutOfReach(inner) => inner,
            // The store held by another writer is `store_busy` and not `storage_error`. The two ask
            // opposite things of the reader: `store_busy` says wait and do it again, `storage_error` says
            // the store gave way — restart, and send the log. A contended write is transient and the
            // batch never opened, so telling a reader their store failed would be false twice over.
            // `AMB-D-154` puts the store's whole concurrency answer on the write lock and this timeout,
            // which makes a refusal past the wait an ordinary outcome of that design, not a fault in it.
            other if other.is_contention() => contended(),
            other => Error::Storage(other.to_string()),
        }
    }
}

/// The **typed registry** of the stable machine-readable codes that ride the `--json` error contract
/// (fixed English).
///
/// [`ErrorCode::as_str`] is the **one and only place** a code string is written down — raw `&'static str`
/// literals are never scattered through the code, so drift is contained structurally rather than by
/// vigilance. Every producer goes through this type, and [`ErrorCode::ALL`] is the single source of truth
/// the consumers' (CLI / GUI) parity tests check themselves against.
///
/// The registry holds codes at two grains. The **family** codes are one per [`enum@Error`] variant, and are
/// what a failure carries when nothing finer is said about it. The **sentence** codes below them name one
/// particular refusal — `not_found_task` rather than `not_found` — and exist so a dictionary can hold a
/// template for it and write it in the reader's language (`AMB-D-413`). A sentence code is pinned on the
/// message with [`Msg::coded`], and which failures deserve one is settled by measurement: the sentences the
/// GUI actually shows a person get one, and everything else keeps its family code and falls back to the
/// English prose the message carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    NotFound,
    AmbiguousId,
    InvalidValue,
    Conflict,
    AlreadyReserved,
    NotReady,
    OutOfReach,
    BindingStale,
    FormatAhead,
    IoError,
    ParseError,
    StorageError,
    StoreBusy,

    // `not_found`, one entity at a time. The entity is the whole sentence — "task X was not found" and
    // "dimension X was not found" share nothing a template could reuse — so the noun rides the code
    // rather than the fields (ops::Noun is where each one is pinned).
    NotFoundTask,
    NotFoundDecision,
    NotFoundProject,
    NotFoundUser,
    NotFoundComment,
    NotFoundDimension,
    NotFoundDimensionValue,
    NotFoundBlob,

    // The reasons a reservation is turned away. `not_ready` is one refusal, but the reasons under it
    // are a list whose length is only known at the moment of refusing, so each reason names itself
    // and rides as a part of the message (see `Msg::part`).
    NotReadyOpenBlocker,
    NotReadyPremiseSuperseded,
    NotReadyPremiseRejected,
    NotReadyPremiseUnsettled,
    NotReadyNotStarted,
    NotReadyDraft,

    // `invalid_value`, one refusal at a time.
    InvalidCommitSha,
    InvalidAttachmentTooLarge,
    InvalidDimensionPeriodOrder,
    InvalidDimensionValuesUnordered,
    InvalidDimensionRequiredWithoutValues,
    InvalidDimensionRequiredUnset,
    InvalidDimensionSlugShape,
    InvalidDimensionSlugTaken,
    InvalidDimensionNameWhitespace,
    InvalidDimensionMultiTimeAxis,
    InvalidDimensionDemoteHolders,
    InvalidDimensionCloseNotClosable,
    InvalidDimensionCloseLastOpen,
    InvalidDimensionSetClosedValue,
    InvalidTaskRequiredDimension,
    InvalidDecisionRequiredDimension,
    InvalidDecisionEditRejected,
    InvalidDecisionAcceptRejected,
    InvalidDecisionRejectAccepted,
    InvalidDecisionReopenRejected,
    InvalidDecisionSelfSupersede,
    InvalidDecisionSelfAmend,
    InvalidDecisionSelfBuildsOn,

    // Backup, restore and export — the refusals a person meets by choosing the wrong path or the wrong
    // file, which is every refusal these three surfaces raise that is not the store itself being broken.
    // The path rides as a field: it is the one part of the sentence no dictionary can hold.
    InvalidBackupDestIsDir,
    InvalidBackupDestExists,
    InvalidRestoreSourceIsDir,
    InvalidRestoreNotAnArchive,
    InvalidRestoreMissingSnapshot,
    // An archive this build cannot read, in either direction. Each names the version it found against
    // the one it can take, so the reader can tell "restore this with the Amenbo that wrote it" from
    // "update Amenbo and try again" — and the newer-store gate names the very build to run.
    InvalidRestoreLayoutTooOld,
    InvalidRestoreLayoutTooNew,
    InvalidRestoreArchiveNewer,
    InvalidExportDestExists,

    // The startup migration, whose screen is the whole window — there is no app behind it and nothing
    // else for the reader to go on. The failures carry their numbers and paths as fields; the inner
    // reason (`failure` / `rollback`) stays English prose, being whatever went wrong underneath.
    InvalidMigrationNoSpace,
    InvalidMigrationRolledBack,
    InvalidMigrationRollbackFailed,
    // The plugin screens: browsing a catalog, installing from it, opening a gate, writing a setting,
    // updating, rolling back, removing. Everything a person meets there arrives from a machine they do
    // not control — a catalog document, an asset off the network, a manifest an update replaced — so the
    // refusals are many and each says something different about what to do next.

    // `not_found`: the plugin, or a build of it, is not where it was looked for.
    NotFoundPluginInCatalog,
    NotFoundPluginInstalled,
    NotFoundPluginBuildOfficial,
    NotFoundPluginBuildOriginUnknown,
    NotFoundPluginBuildSourceGone,
    NotFoundPluginBuildSourceSilent,
    NotFoundPluginBuildDelisted,
    NotFoundPluginRollbackBuild,

    // `conflict`: this machine already holds the name being installed.
    ConflictPluginInstalled,
    ConflictPluginInstallBroken,

    // `invalid_value`: the catalog document itself, and the address it was fetched from.
    InvalidCatalogUnreadable,
    InvalidCatalogVersionAhead,
    InvalidCatalogDetailSwapped,
    InvalidCatalogDetailUnreadable,
    InvalidCatalogDetailNamesOther,
    InvalidCatalogUrlScheme,
    InvalidCatalogUrlOfficial,
    InvalidCatalogKeyRotated,
    InvalidCatalogKeyDocument,
    InvalidCatalogKeyAbsent,

    // `invalid_value`: one entry of the catalog, as the intake door judged it.
    InvalidPluginEntry,
    InvalidPluginEntryDropped,
    InvalidPluginEntryDuplicate,

    // `invalid_value`: the platform, and the asset published for it.
    InvalidPluginOsUnsupported,
    InvalidPluginAssetAbsent,
    InvalidPluginAssetEmpty,
    InvalidPluginAssetZipOffWindows,
    InvalidPluginAssetTarUnreadable,
    InvalidPluginAssetZipUnreadable,
    InvalidPluginAssetWithoutProgram,
    InvalidPluginManifestUnwritable,

    // `invalid_value`: provenance — the checksum over the bytes, and the signature over them.
    InvalidPluginChecksumFormat,
    InvalidPluginChecksumMismatch,
    InvalidPluginChecksumLength,
    InvalidPluginChecksumNotHex,
    InvalidPluginKeyMalformed,
    InvalidPluginSignatureMalformed,
    InvalidPluginSignatureMismatch,
    InvalidPluginUnsigned,

    // `invalid_value`: what an install left on disk, read back.
    InvalidPluginManifestMalformed,
    InvalidPluginManifestNamesOther,
    InvalidPluginProgramAbsent,

    // `invalid_value`: updating, and the one build back a rollback consumes.
    InvalidPluginUpdatePlatform,
    InvalidPluginRollbackManifestAbsent,
    InvalidPluginRollbackManifestUnparsable,

    // `invalid_value`: the gate, the compatibility declarations behind it, and a setting's floor. The
    // incompatibility itself is a reason under the refusal rather than a code of its own — the same
    // three verdicts read under two different sentences (enabling, and updating), so they ride as
    // parts (see `Msg::part`).
    InvalidPluginProjectRequired,
    InvalidPluginSettingsRequired,
    // The author's own check, read fail-closed at the gate (`AMB-D-664`). Two codes rather than one
    // because the two are different facts and only one of them is about the values: the check looked and
    // said no, or it was raised and said nothing this build can act on (`AMB-D-354`).
    InvalidPluginCheckRefused,
    InvalidPluginCheckSilent,
    InvalidPluginIncompatible,
    InvalidPluginUpdateIncompatible,
    PluginIncompatiblePayload,
    PluginIncompatibleAmenboOld,
    PluginIncompatibleFloorUnreadable,
    InvalidPluginConfigValueTooLarge,
    InvalidPluginConfigValueControlChars,
}

impl ErrorCode {
    /// The stable string that rides the contract — this match is the only place a code string is written.
    /// It is exhaustive, so adding a variant breaks the build rather than shipping an undefined code.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::NotFound => "not_found",
            ErrorCode::AmbiguousId => "ambiguous_id",
            ErrorCode::InvalidValue => "invalid_value",
            ErrorCode::Conflict => "conflict",
            ErrorCode::AlreadyReserved => "already_reserved",
            ErrorCode::NotReady => "not_ready",
            ErrorCode::OutOfReach => "out_of_reach",
            ErrorCode::BindingStale => "binding_stale",
            ErrorCode::FormatAhead => "format_ahead",
            ErrorCode::IoError => "io_error",
            ErrorCode::ParseError => "parse_error",
            ErrorCode::StorageError => "storage_error",
            ErrorCode::StoreBusy => "store_busy",
            ErrorCode::NotFoundTask => "not_found_task",
            ErrorCode::NotFoundDecision => "not_found_decision",
            ErrorCode::NotFoundProject => "not_found_project",
            ErrorCode::NotFoundUser => "not_found_user",
            ErrorCode::NotFoundComment => "not_found_comment",
            ErrorCode::NotFoundDimension => "not_found_dimension",
            ErrorCode::NotFoundDimensionValue => "not_found_dimension_value",
            ErrorCode::NotFoundBlob => "not_found_blob",
            ErrorCode::NotReadyOpenBlocker => "not_ready_open_blocker",
            ErrorCode::NotReadyPremiseSuperseded => "not_ready_premise_superseded",
            ErrorCode::NotReadyPremiseRejected => "not_ready_premise_rejected",
            ErrorCode::NotReadyPremiseUnsettled => "not_ready_premise_unsettled",
            ErrorCode::NotReadyNotStarted => "not_ready_not_started",
            ErrorCode::NotReadyDraft => "not_ready_draft",
            ErrorCode::InvalidCommitSha => "invalid_commit_sha",
            ErrorCode::InvalidAttachmentTooLarge => "invalid_attachment_too_large",
            ErrorCode::InvalidDimensionPeriodOrder => "invalid_dimension_period_order",
            ErrorCode::InvalidDimensionValuesUnordered => "invalid_dimension_values_unordered",
            ErrorCode::InvalidDimensionRequiredWithoutValues => {
                "invalid_dimension_required_without_values"
            }
            ErrorCode::InvalidDimensionRequiredUnset => "invalid_dimension_required_unset",
            ErrorCode::InvalidDimensionSlugShape => "invalid_dimension_slug_shape",
            ErrorCode::InvalidDimensionSlugTaken => "invalid_dimension_slug_taken",
            ErrorCode::InvalidDimensionNameWhitespace => "invalid_dimension_name_whitespace",
            ErrorCode::InvalidDimensionMultiTimeAxis => "invalid_dimension_multi_time_axis",
            ErrorCode::InvalidDimensionDemoteHolders => "invalid_dimension_demote_holders",
            ErrorCode::InvalidDimensionCloseNotClosable => "invalid_dimension_close_not_closable",
            ErrorCode::InvalidDimensionCloseLastOpen => "invalid_dimension_close_last_open",
            ErrorCode::InvalidDimensionSetClosedValue => "invalid_dimension_set_closed_value",
            ErrorCode::InvalidTaskRequiredDimension => "invalid_task_required_dimension",
            ErrorCode::InvalidDecisionRequiredDimension => "invalid_decision_required_dimension",
            ErrorCode::InvalidDecisionEditRejected => "invalid_decision_edit_rejected",
            ErrorCode::InvalidDecisionAcceptRejected => "invalid_decision_accept_rejected",
            ErrorCode::InvalidDecisionRejectAccepted => "invalid_decision_reject_accepted",
            ErrorCode::InvalidDecisionReopenRejected => "invalid_decision_reopen_rejected",
            ErrorCode::InvalidDecisionSelfSupersede => "invalid_decision_self_supersede",
            ErrorCode::InvalidDecisionSelfAmend => "invalid_decision_self_amend",
            ErrorCode::InvalidDecisionSelfBuildsOn => "invalid_decision_self_builds_on",
            ErrorCode::InvalidBackupDestIsDir => "invalid_backup_dest_is_dir",
            ErrorCode::InvalidBackupDestExists => "invalid_backup_dest_exists",
            ErrorCode::InvalidRestoreSourceIsDir => "invalid_restore_source_is_dir",
            ErrorCode::InvalidRestoreNotAnArchive => "invalid_restore_not_an_archive",
            ErrorCode::InvalidRestoreMissingSnapshot => "invalid_restore_missing_snapshot",
            ErrorCode::InvalidRestoreLayoutTooOld => "invalid_restore_layout_too_old",
            ErrorCode::InvalidRestoreLayoutTooNew => "invalid_restore_layout_too_new",
            ErrorCode::InvalidRestoreArchiveNewer => "invalid_restore_archive_newer",
            ErrorCode::InvalidExportDestExists => "invalid_export_dest_exists",
            ErrorCode::InvalidMigrationNoSpace => "invalid_migration_no_space",
            ErrorCode::InvalidMigrationRolledBack => "invalid_migration_rolled_back",
            ErrorCode::InvalidMigrationRollbackFailed => "invalid_migration_rollback_failed",
            ErrorCode::NotFoundPluginInCatalog => "not_found_plugin_in_catalog",
            ErrorCode::NotFoundPluginInstalled => "not_found_plugin_installed",
            ErrorCode::NotFoundPluginBuildOfficial => "not_found_plugin_build_official",
            ErrorCode::NotFoundPluginBuildOriginUnknown => "not_found_plugin_build_origin_unknown",
            ErrorCode::NotFoundPluginBuildSourceGone => "not_found_plugin_build_source_gone",
            ErrorCode::NotFoundPluginBuildSourceSilent => "not_found_plugin_build_source_silent",
            ErrorCode::NotFoundPluginBuildDelisted => "not_found_plugin_build_delisted",
            ErrorCode::NotFoundPluginRollbackBuild => "not_found_plugin_rollback_build",
            ErrorCode::ConflictPluginInstalled => "conflict_plugin_installed",
            ErrorCode::ConflictPluginInstallBroken => "conflict_plugin_install_broken",
            ErrorCode::InvalidCatalogUnreadable => "invalid_catalog_unreadable",
            ErrorCode::InvalidCatalogVersionAhead => "invalid_catalog_version_ahead",
            ErrorCode::InvalidCatalogDetailSwapped => "invalid_catalog_detail_swapped",
            ErrorCode::InvalidCatalogDetailUnreadable => "invalid_catalog_detail_unreadable",
            ErrorCode::InvalidCatalogDetailNamesOther => "invalid_catalog_detail_names_other",
            ErrorCode::InvalidCatalogUrlScheme => "invalid_catalog_url_scheme",
            ErrorCode::InvalidCatalogUrlOfficial => "invalid_catalog_url_official",
            ErrorCode::InvalidCatalogKeyRotated => "invalid_catalog_key_rotated",
            ErrorCode::InvalidCatalogKeyDocument => "invalid_catalog_key_document",
            ErrorCode::InvalidCatalogKeyAbsent => "invalid_catalog_key_absent",
            ErrorCode::InvalidPluginEntry => "invalid_plugin_entry",
            ErrorCode::InvalidPluginEntryDropped => "invalid_plugin_entry_dropped",
            ErrorCode::InvalidPluginEntryDuplicate => "invalid_plugin_entry_duplicate",
            ErrorCode::InvalidPluginOsUnsupported => "invalid_plugin_os_unsupported",
            ErrorCode::InvalidPluginAssetAbsent => "invalid_plugin_asset_absent",
            ErrorCode::InvalidPluginAssetEmpty => "invalid_plugin_asset_empty",
            ErrorCode::InvalidPluginAssetZipOffWindows => "invalid_plugin_asset_zip_off_windows",
            ErrorCode::InvalidPluginAssetTarUnreadable => "invalid_plugin_asset_tar_unreadable",
            ErrorCode::InvalidPluginAssetZipUnreadable => "invalid_plugin_asset_zip_unreadable",
            ErrorCode::InvalidPluginAssetWithoutProgram => "invalid_plugin_asset_without_program",
            ErrorCode::InvalidPluginManifestUnwritable => "invalid_plugin_manifest_unwritable",
            ErrorCode::InvalidPluginChecksumFormat => "invalid_plugin_checksum_format",
            ErrorCode::InvalidPluginChecksumMismatch => "invalid_plugin_checksum_mismatch",
            ErrorCode::InvalidPluginChecksumLength => "invalid_plugin_checksum_length",
            ErrorCode::InvalidPluginChecksumNotHex => "invalid_plugin_checksum_not_hex",
            ErrorCode::InvalidPluginKeyMalformed => "invalid_plugin_key_malformed",
            ErrorCode::InvalidPluginSignatureMalformed => "invalid_plugin_signature_malformed",
            ErrorCode::InvalidPluginSignatureMismatch => "invalid_plugin_signature_mismatch",
            ErrorCode::InvalidPluginUnsigned => "invalid_plugin_unsigned",
            ErrorCode::InvalidPluginManifestMalformed => "invalid_plugin_manifest_malformed",
            ErrorCode::InvalidPluginManifestNamesOther => "invalid_plugin_manifest_names_other",
            ErrorCode::InvalidPluginProgramAbsent => "invalid_plugin_program_absent",
            ErrorCode::InvalidPluginUpdatePlatform => "invalid_plugin_update_platform",
            ErrorCode::InvalidPluginRollbackManifestAbsent => "invalid_plugin_rollback_manifest_absent",
            ErrorCode::InvalidPluginRollbackManifestUnparsable => "invalid_plugin_rollback_manifest_unparsable",
            ErrorCode::InvalidPluginProjectRequired => "invalid_plugin_project_required",
            ErrorCode::InvalidPluginSettingsRequired => "invalid_plugin_settings_required",
            ErrorCode::InvalidPluginCheckRefused => "invalid_plugin_check_refused",
            ErrorCode::InvalidPluginCheckSilent => "invalid_plugin_check_silent",
            ErrorCode::InvalidPluginIncompatible => "invalid_plugin_incompatible",
            ErrorCode::InvalidPluginUpdateIncompatible => "invalid_plugin_update_incompatible",
            ErrorCode::PluginIncompatiblePayload => "plugin_incompatible_payload",
            ErrorCode::PluginIncompatibleAmenboOld => "plugin_incompatible_amenbo_old",
            ErrorCode::PluginIncompatibleFloorUnreadable => "plugin_incompatible_floor_unreadable",
            ErrorCode::InvalidPluginConfigValueTooLarge => "invalid_plugin_config_value_too_large",
            ErrorCode::InvalidPluginConfigValueControlChars => "invalid_plugin_config_value_control_chars",
        }
    }

    /// Every error code the core can emit. The single source of truth for the consumers' parity tests and
    /// for the contract snapshot.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::NotFound,
        ErrorCode::AmbiguousId,
        ErrorCode::InvalidValue,
        ErrorCode::Conflict,
        ErrorCode::AlreadyReserved,
        ErrorCode::NotReady,
        ErrorCode::OutOfReach,
        ErrorCode::BindingStale,
        ErrorCode::FormatAhead,
        ErrorCode::IoError,
        ErrorCode::ParseError,
        ErrorCode::StorageError,
        ErrorCode::StoreBusy,
        ErrorCode::NotFoundTask,
        ErrorCode::NotFoundDecision,
        ErrorCode::NotFoundProject,
        ErrorCode::NotFoundUser,
        ErrorCode::NotFoundComment,
        ErrorCode::NotFoundDimension,
        ErrorCode::NotFoundDimensionValue,
        ErrorCode::NotFoundBlob,
        ErrorCode::NotReadyOpenBlocker,
        ErrorCode::NotReadyPremiseSuperseded,
        ErrorCode::NotReadyPremiseRejected,
        ErrorCode::NotReadyPremiseUnsettled,
        ErrorCode::NotReadyNotStarted,
        ErrorCode::NotReadyDraft,
        ErrorCode::InvalidCommitSha,
        ErrorCode::InvalidAttachmentTooLarge,
        ErrorCode::InvalidDimensionPeriodOrder,
        ErrorCode::InvalidDimensionValuesUnordered,
        ErrorCode::InvalidDimensionRequiredWithoutValues,
        ErrorCode::InvalidDimensionRequiredUnset,
        ErrorCode::InvalidDimensionSlugShape,
        ErrorCode::InvalidDimensionSlugTaken,
        ErrorCode::InvalidDimensionNameWhitespace,
        ErrorCode::InvalidDimensionMultiTimeAxis,
        ErrorCode::InvalidDimensionDemoteHolders,
        ErrorCode::InvalidDimensionCloseNotClosable,
        ErrorCode::InvalidDimensionCloseLastOpen,
        ErrorCode::InvalidDimensionSetClosedValue,
        ErrorCode::InvalidTaskRequiredDimension,
        ErrorCode::InvalidDecisionRequiredDimension,
        ErrorCode::InvalidDecisionEditRejected,
        ErrorCode::InvalidDecisionAcceptRejected,
        ErrorCode::InvalidDecisionRejectAccepted,
        ErrorCode::InvalidDecisionReopenRejected,
        ErrorCode::InvalidDecisionSelfSupersede,
        ErrorCode::InvalidDecisionSelfAmend,
        ErrorCode::InvalidDecisionSelfBuildsOn,
        ErrorCode::InvalidBackupDestIsDir,
        ErrorCode::InvalidBackupDestExists,
        ErrorCode::InvalidRestoreSourceIsDir,
        ErrorCode::InvalidRestoreNotAnArchive,
        ErrorCode::InvalidRestoreMissingSnapshot,
        ErrorCode::InvalidRestoreLayoutTooOld,
        ErrorCode::InvalidRestoreLayoutTooNew,
        ErrorCode::InvalidRestoreArchiveNewer,
        ErrorCode::InvalidExportDestExists,
        ErrorCode::InvalidMigrationNoSpace,
        ErrorCode::InvalidMigrationRolledBack,
        ErrorCode::InvalidMigrationRollbackFailed,
        ErrorCode::NotFoundPluginInCatalog,
        ErrorCode::NotFoundPluginInstalled,
        ErrorCode::NotFoundPluginBuildOfficial,
        ErrorCode::NotFoundPluginBuildOriginUnknown,
        ErrorCode::NotFoundPluginBuildSourceGone,
        ErrorCode::NotFoundPluginBuildSourceSilent,
        ErrorCode::NotFoundPluginBuildDelisted,
        ErrorCode::NotFoundPluginRollbackBuild,
        ErrorCode::ConflictPluginInstalled,
        ErrorCode::ConflictPluginInstallBroken,
        ErrorCode::InvalidCatalogUnreadable,
        ErrorCode::InvalidCatalogVersionAhead,
        ErrorCode::InvalidCatalogDetailSwapped,
        ErrorCode::InvalidCatalogDetailUnreadable,
        ErrorCode::InvalidCatalogDetailNamesOther,
        ErrorCode::InvalidCatalogUrlScheme,
        ErrorCode::InvalidCatalogUrlOfficial,
        ErrorCode::InvalidCatalogKeyRotated,
        ErrorCode::InvalidCatalogKeyDocument,
        ErrorCode::InvalidCatalogKeyAbsent,
        ErrorCode::InvalidPluginEntry,
        ErrorCode::InvalidPluginEntryDropped,
        ErrorCode::InvalidPluginEntryDuplicate,
        ErrorCode::InvalidPluginOsUnsupported,
        ErrorCode::InvalidPluginAssetAbsent,
        ErrorCode::InvalidPluginAssetEmpty,
        ErrorCode::InvalidPluginAssetZipOffWindows,
        ErrorCode::InvalidPluginAssetTarUnreadable,
        ErrorCode::InvalidPluginAssetZipUnreadable,
        ErrorCode::InvalidPluginAssetWithoutProgram,
        ErrorCode::InvalidPluginManifestUnwritable,
        ErrorCode::InvalidPluginChecksumFormat,
        ErrorCode::InvalidPluginChecksumMismatch,
        ErrorCode::InvalidPluginChecksumLength,
        ErrorCode::InvalidPluginChecksumNotHex,
        ErrorCode::InvalidPluginKeyMalformed,
        ErrorCode::InvalidPluginSignatureMalformed,
        ErrorCode::InvalidPluginSignatureMismatch,
        ErrorCode::InvalidPluginUnsigned,
        ErrorCode::InvalidPluginManifestMalformed,
        ErrorCode::InvalidPluginManifestNamesOther,
        ErrorCode::InvalidPluginProgramAbsent,
        ErrorCode::InvalidPluginUpdatePlatform,
        ErrorCode::InvalidPluginRollbackManifestAbsent,
        ErrorCode::InvalidPluginRollbackManifestUnparsable,
        ErrorCode::InvalidPluginProjectRequired,
        ErrorCode::InvalidPluginSettingsRequired,
        ErrorCode::InvalidPluginCheckRefused,
        ErrorCode::InvalidPluginCheckSilent,
        ErrorCode::InvalidPluginIncompatible,
        ErrorCode::InvalidPluginUpdateIncompatible,
        ErrorCode::PluginIncompatiblePayload,
        ErrorCode::PluginIncompatibleAmenboOld,
        ErrorCode::PluginIncompatibleFloorUnreadable,
        ErrorCode::InvalidPluginConfigValueTooLarge,
        ErrorCode::InvalidPluginConfigValueControlChars,
    ];
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Error {
    /// The stable machine-readable code for `--json` error output (fixed English; contractual).
    /// It defines no string of its own — it defers to the [`ErrorCode`] registry.
    pub fn code(&self) -> &'static str {
        self.error_code().as_str()
    }

    /// Maps this error onto its contractual [`ErrorCode`], no string literals. A message that names its own
    /// sentence wins over its variant's family code — that finer code is the whole point of pinning one, and
    /// a variant covering thirty different refusals cannot say which one this is. CLI/GUI parity takes this
    /// type as its single source of truth.
    pub fn error_code(&self) -> ErrorCode {
        self.msg().and_then(Msg::code).unwrap_or_else(|| self.variant_code())
    }

    /// The values this error's sentence is built from, for the surface that composes it (`AMB-D-413`). Empty
    /// for a message that names no sentence, and for the variants that carry no [`Msg`] at all — the latter
    /// are already structured, and the Tauri layer reads their parts off the variant itself.
    pub fn fields(&self) -> Option<&Fields> {
        self.msg().map(Msg::fields).filter(|f| !f.is_empty())
    }

    /// The sentences this error's message is composed of, for the surface that writes each one in the
    /// reader's language ([`Msg::part`]). Empty for everything that says one thing and is done, which is
    /// nearly all of it.
    pub fn parts(&self) -> &[Msg] {
        self.msg().map(Msg::parts).unwrap_or_default()
    }

    /// The bilingual payload, for the variants that carry one.
    fn msg(&self) -> Option<&Msg> {
        match self {
            Error::NotFound(m)
            | Error::Invalid(m)
            | Error::Conflict(m)
            | Error::AlreadyReserved(m)
            | Error::NotReady(m)
            | Error::OutOfReach(m)
            | Error::FormatAhead(m)
            | Error::StoreBusy(m) => Some(m),
            Error::AmbiguousId { .. }
            | Error::BindingStale(_)
            | Error::Io(_)
            | Error::Json(_)
            | Error::Storage(_) => None,
        }
    }

    /// The family code of this variant, before any finer one the message names.
    fn variant_code(&self) -> ErrorCode {
        match self {
            Error::NotFound(_) => ErrorCode::NotFound,
            Error::AmbiguousId { .. } => ErrorCode::AmbiguousId,
            Error::Invalid(_) => ErrorCode::InvalidValue,
            Error::Conflict(_) => ErrorCode::Conflict,
            Error::AlreadyReserved(_) => ErrorCode::AlreadyReserved,
            Error::NotReady(_) => ErrorCode::NotReady,
            Error::OutOfReach(_) => ErrorCode::OutOfReach,
            Error::BindingStale(_) => ErrorCode::BindingStale,
            Error::FormatAhead(_) => ErrorCode::FormatAhead,
            Error::Io(_) => ErrorCode::IoError,
            Error::Json(_) => ErrorCode::ParseError,
            Error::Storage(_) => ErrorCode::StorageError,
            Error::StoreBusy(_) => ErrorCode::StoreBusy,
        }
    }

    /// The message for the CLI's all-English surface (`--json`'s `message`, and human prose). It is the
    /// `Display` sentence under the name that surface knows it by — there is only ever one sentence, so
    /// writing a second English rendering here would be a second place for it to drift.
    pub fn message_en(&self) -> String {
        self.to_string()
    }

    pub fn invalid(en: impl Into<String>) -> Self {
        Error::Invalid(Msg::new(en))
    }

    pub fn not_found(en: impl Into<String>) -> Self {
        Error::NotFound(Msg::new(en))
    }

    pub fn conflict(en: impl Into<String>) -> Self {
        Error::Conflict(Msg::new(en))
    }

    pub fn already_reserved(en: impl Into<String>) -> Self {
        Error::AlreadyReserved(Msg::new(en))
    }

    pub fn out_of_reach(en: impl Into<String>) -> Self {
        Error::OutOfReach(Msg::new(en))
    }

    pub fn format_ahead(en: impl Into<String>) -> Self {
        Error::FormatAhead(Msg::new(en))
    }

    pub fn store_busy(en: impl Into<String>) -> Self {
        Error::StoreBusy(Msg::new(en))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_one_sentence_and_it_is_english() {
        let e = Error::not_found("task 'X' not found");
        // The machine contract: the code stays fixed English.
        assert_eq!(e.code(), "not_found");
        // The prose face and the CLI's name for it are the same sentence — no second rendering to drift.
        assert_eq!(e.to_string(), "task 'X' not found");
        assert_eq!(e.message_en(), e.to_string());
    }

    #[test]
    fn error_code_registry_is_the_full_fixed_set() {
        use std::collections::BTreeSet;
        // The contract snapshot: it pins the full set of core error codes. Adding or renaming a code means
        // updating this list too — that is the point, it is a deliberate checkpoint. This set is the single
        // source of truth the CLI's and GUI's parity tests are held against.
        let expected: BTreeSet<&str> = [
            "not_found",
            "ambiguous_id",
            "invalid_value",
            "conflict",
            "already_reserved",
            "not_ready",
            "out_of_reach",
            "binding_stale",
            "format_ahead",
            "io_error",
            "parse_error",
            "storage_error",
            "store_busy",
            "not_found_task",
            "not_found_decision",
            "not_found_project",
            "not_found_user",
            "not_found_comment",
            "not_found_dimension",
            "not_found_dimension_value",
            "not_found_blob",
            "not_ready_open_blocker",
            "not_ready_premise_superseded",
            "not_ready_premise_rejected",
            "not_ready_premise_unsettled",
            "not_ready_not_started",
            "not_ready_draft",
            "invalid_commit_sha",
            "invalid_attachment_too_large",
            "invalid_dimension_period_order",
            "invalid_dimension_values_unordered",
            "invalid_dimension_required_without_values",
            "invalid_dimension_required_unset",
            "invalid_dimension_slug_shape",
            "invalid_dimension_slug_taken",
            "invalid_dimension_name_whitespace",
            "invalid_dimension_multi_time_axis",
            "invalid_dimension_demote_holders",
            "invalid_dimension_close_not_closable",
            "invalid_dimension_close_last_open",
            "invalid_dimension_set_closed_value",
            "invalid_task_required_dimension",
            "invalid_decision_required_dimension",
            "invalid_decision_edit_rejected",
            "invalid_decision_accept_rejected",
            "invalid_decision_reject_accepted",
            "invalid_decision_reopen_rejected",
            "invalid_decision_self_supersede",
            "invalid_decision_self_amend",
            "invalid_decision_self_builds_on",
            "invalid_backup_dest_is_dir",
            "invalid_backup_dest_exists",
            "invalid_restore_source_is_dir",
            "invalid_restore_not_an_archive",
            "invalid_restore_missing_snapshot",
            "invalid_restore_layout_too_old",
            "invalid_restore_layout_too_new",
            "invalid_restore_archive_newer",
            "invalid_export_dest_exists",
            "invalid_migration_no_space",
            "invalid_migration_rolled_back",
            "invalid_migration_rollback_failed",
            "not_found_plugin_in_catalog",
            "not_found_plugin_installed",
            "not_found_plugin_build_official",
            "not_found_plugin_build_origin_unknown",
            "not_found_plugin_build_source_gone",
            "not_found_plugin_build_source_silent",
            "not_found_plugin_build_delisted",
            "not_found_plugin_rollback_build",
            "conflict_plugin_installed",
            "conflict_plugin_install_broken",
            "invalid_catalog_unreadable",
            "invalid_catalog_version_ahead",
            "invalid_catalog_detail_swapped",
            "invalid_catalog_detail_unreadable",
            "invalid_catalog_detail_names_other",
            "invalid_catalog_url_scheme",
            "invalid_catalog_url_official",
            "invalid_catalog_key_rotated",
            "invalid_catalog_key_document",
            "invalid_catalog_key_absent",
            "invalid_plugin_entry",
            "invalid_plugin_entry_dropped",
            "invalid_plugin_entry_duplicate",
            "invalid_plugin_os_unsupported",
            "invalid_plugin_asset_absent",
            "invalid_plugin_asset_empty",
            "invalid_plugin_asset_zip_off_windows",
            "invalid_plugin_asset_tar_unreadable",
            "invalid_plugin_asset_zip_unreadable",
            "invalid_plugin_asset_without_program",
            "invalid_plugin_manifest_unwritable",
            "invalid_plugin_checksum_format",
            "invalid_plugin_checksum_mismatch",
            "invalid_plugin_checksum_length",
            "invalid_plugin_checksum_not_hex",
            "invalid_plugin_key_malformed",
            "invalid_plugin_signature_malformed",
            "invalid_plugin_signature_mismatch",
            "invalid_plugin_unsigned",
            "invalid_plugin_manifest_malformed",
            "invalid_plugin_manifest_names_other",
            "invalid_plugin_program_absent",
            "invalid_plugin_update_platform",
            "invalid_plugin_rollback_manifest_absent",
            "invalid_plugin_rollback_manifest_unparsable",
            "invalid_plugin_project_required",
            "invalid_plugin_settings_required",
            "invalid_plugin_check_refused",
            "invalid_plugin_check_silent",
            "invalid_plugin_incompatible",
            "invalid_plugin_update_incompatible",
            "plugin_incompatible_payload",
            "plugin_incompatible_amenbo_old",
            "plugin_incompatible_floor_unreadable",
            "invalid_plugin_config_value_too_large",
            "invalid_plugin_config_value_control_chars",
        ]
        .into_iter()
        .collect();
        let actual: BTreeSet<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(actual, expected, "the full set of core error codes does not match the contract");
        // The strings are unique — no two variants may define the same code.
        assert_eq!(
            ErrorCode::ALL.len(),
            actual.len(),
            "duplicate error code strings"
        );
        // A code is non-empty lowercase snake_case — the shape the contract promises.
        for c in ErrorCode::ALL {
            let s = c.as_str();
            assert!(!s.is_empty(), "an empty code");
            assert!(
                s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "a code is lowercase snake_case: {s}"
            );
        }
    }

    #[test]
    fn a_named_sentence_reports_its_own_code_and_its_values() {
        // A refusal the GUI shows names itself, and sends what the sentence is about apart from the
        // sentence — that pair is what the dictionary side needs to write it in a third language.
        let e = Error::NotFound(
            Msg::new("task 'AMB-T-12' not found")
                .coded(ErrorCode::NotFoundTask)
                .with("ref", "AMB-T-12"),
        );
        assert_eq!(e.code(), "not_found_task", "the sentence's code wins over the variant's family");
        let fields: Vec<_> = e.fields().expect("values ride along").iter().collect();
        assert_eq!(fields, vec![("ref", "AMB-T-12")]);

        // A message that names no sentence keeps the family code, and carries no values: its whole
        // answer is the prose, which is what a reader gets where no template exists.
        let plain = Error::not_found("task 'X' not found");
        assert_eq!(plain.code(), "not_found");
        assert!(plain.fields().is_none());
    }

    #[test]
    fn structured_variants_read_in_english_too() {
        // The variants that carry no `Msg` write their prose in the `#[error]` template, which is the one
        // place their sentence could still have been left in another language.
        let e = Error::AmbiguousId { prefix: "01K".into(), candidates: vec!["a".into(), "b".into()] };
        assert_eq!(e.code(), "ambiguous_id");
        assert!(e.to_string().contains("is ambiguous"));

        // Detail that arrives from a library is already English, and the prefix no longer changes that.
        let storage = Error::Storage("engine write failed".into());
        assert_eq!(storage.code(), "storage_error");
        assert!(storage.to_string().starts_with("store (engine) operation failed"));

        for e in [
            e,
            storage,
            Error::BindingStale("/gone".into()),
            Error::Json(serde_json::from_str::<i32>("{").unwrap_err()),
        ] {
            assert!(
                !e.to_string().chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)),
                "no kana in a core sentence: {e}"
            );
        }
    }
}
