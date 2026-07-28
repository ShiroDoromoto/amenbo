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
    move |e| Error::Storage(format!("{}: {}", path.display(), StoreEngineError::from(e)))
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

impl From<crate::store_engine::StoreEngineError> for Error {
    fn from(e: crate::store_engine::StoreEngineError) -> Self {
        match e {
            // A reach refusal crosses unchanged, even when it surfaces from the engine. Rounding it into
            // `Storage` would make a containment refusal claim to be a store failure, and would erase the
            // `out_of_reach` code from the contract.
            crate::store_engine::StoreEngineError::OutOfReach(inner) => inner,
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

    // `invalid_value`, one refusal at a time.
    InvalidCommitSha,
    InvalidAttachmentTooLarge,
    InvalidDimensionPeriodOrder,
    InvalidDimensionValuesUnordered,
    InvalidDecisionEditRejected,
    InvalidDecisionAcceptRejected,
    InvalidDecisionRejectAccepted,
    InvalidDecisionReopenRejected,
    InvalidDecisionSelfSupersede,
    InvalidDecisionSelfAmend,
    InvalidDecisionSelfBuildsOn,
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
            ErrorCode::InvalidCommitSha => "invalid_commit_sha",
            ErrorCode::InvalidAttachmentTooLarge => "invalid_attachment_too_large",
            ErrorCode::InvalidDimensionPeriodOrder => "invalid_dimension_period_order",
            ErrorCode::InvalidDimensionValuesUnordered => "invalid_dimension_values_unordered",
            ErrorCode::InvalidDecisionEditRejected => "invalid_decision_edit_rejected",
            ErrorCode::InvalidDecisionAcceptRejected => "invalid_decision_accept_rejected",
            ErrorCode::InvalidDecisionRejectAccepted => "invalid_decision_reject_accepted",
            ErrorCode::InvalidDecisionReopenRejected => "invalid_decision_reopen_rejected",
            ErrorCode::InvalidDecisionSelfSupersede => "invalid_decision_self_supersede",
            ErrorCode::InvalidDecisionSelfAmend => "invalid_decision_self_amend",
            ErrorCode::InvalidDecisionSelfBuildsOn => "invalid_decision_self_builds_on",
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
        ErrorCode::InvalidCommitSha,
        ErrorCode::InvalidAttachmentTooLarge,
        ErrorCode::InvalidDimensionPeriodOrder,
        ErrorCode::InvalidDimensionValuesUnordered,
        ErrorCode::InvalidDecisionEditRejected,
        ErrorCode::InvalidDecisionAcceptRejected,
        ErrorCode::InvalidDecisionRejectAccepted,
        ErrorCode::InvalidDecisionReopenRejected,
        ErrorCode::InvalidDecisionSelfSupersede,
        ErrorCode::InvalidDecisionSelfAmend,
        ErrorCode::InvalidDecisionSelfBuildsOn,
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
            "invalid_commit_sha",
            "invalid_attachment_too_large",
            "invalid_dimension_period_order",
            "invalid_dimension_values_unordered",
            "invalid_decision_edit_rejected",
            "invalid_decision_accept_rejected",
            "invalid_decision_reject_accepted",
            "invalid_decision_reopen_rejected",
            "invalid_decision_self_supersede",
            "invalid_decision_self_amend",
            "invalid_decision_self_builds_on",
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
