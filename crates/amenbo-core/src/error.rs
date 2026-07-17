//! The error type the whole core speaks in. The CLI puts this `code()` straight into
//! `{ "error": { "code", "message" } }`: the `--json` error contract *is* this code — the stable code
//! [`Error::code`] returns below.
//!
//! i18n layering:
//! The machine/agent surface (`--json`'s `code`) is a fixed-English contract, and `code()` here is its
//! canonical source. Human-facing wording has two surfaces:
//! - **CLI (an all-English contract surface)** — use [`Error::message_en`].
//! - **GUI (a localized human surface)** — Tauri commands return failures as a structured `CmdError`
//!   (`app/src-tauri/src/error.rs`) carrying `code()` (the stable code), `to_string()` (the Japanese
//!   `Display`), [`Error::message_en`] (English) and `fields` for interpolation; the front end
//!   (`errLabel` in `app/src/core/i18n.ts`) maps `code` onto per-language templates. A code with no
//!   template falls back to the Japanese `Display` / the English `message_en`.
//!
//! The free-prose variants (`NotFound` / `Invalid` / `Conflict` / `FormatAhead` / `StoreBusy`) carry a
//! [`Msg`] payload so they can hold both languages at once. Detail that is already English because it
//! came from a library (`Storage` / `Io` / `Json`), and detail that is already structured
//! (`AmbiguousId` and friends), keeps its payload as-is: the language difference is absorbed entirely
//! by the prefix template.

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

/// An error message that holds both languages. `Display` (the GUI's human surface) is Japanese; the CLI
/// uses [`Msg::en`].
///
/// Dynamic detail (ids and the like) is interpolated into both sentences by the caller — the same value
/// in each.
#[derive(Debug, Clone)]
pub struct Msg {
    en: String,
    ja: String,
}

impl Msg {
    pub fn new(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Msg {
            en: en.into(),
            ja: ja.into(),
        }
    }

    /// The English sentence, for the CLI's all-English surface.
    pub fn en(&self) -> &str {
        &self.en
    }

    /// The Japanese sentence, for the GUI's localized human surface.
    pub fn ja(&self) -> &str {
        &self.ja
    }
}

impl std::fmt::Display for Msg {
    /// Renders Japanese, for the GUI's human surface (`to_string()`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.ja)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    NotFound(Msg),

    /// An abbreviated id matched more than one entity; the candidates are listed back.
    #[error("ID '{prefix}' は曖昧です。候補: {candidates:?}")]
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
    #[error("プロジェクトの紐付け先ディレクトリが見つかりません: {0}")]
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

    #[error("入出力エラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON の解析に失敗しました: {0}")]
    Json(#[from] serde_json::Error),

    #[error("ストア（engine）の操作に失敗しました: {0}")]
    Storage(String),

    #[error("ストアは別のプロセスが使用中です: {0}")]
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

    /// Maps this error onto its contractual [`ErrorCode`] — variant→code and nothing else, no string
    /// literals. CLI/GUI parity takes this type as its single source of truth.
    pub fn error_code(&self) -> ErrorCode {
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

    /// The English message for the CLI's all-English surface (`--json`'s `message`, and human prose).
    /// The GUI uses the Japanese `Display` (`to_string()`) instead.
    pub fn message_en(&self) -> String {
        match self {
            Error::NotFound(m) => m.en().to_string(),
            Error::AmbiguousId { prefix, candidates } => {
                format!("id '{prefix}' is ambiguous. candidates: {candidates:?}")
            }
            Error::Invalid(m) => m.en().to_string(),
            Error::Conflict(m) => m.en().to_string(),
            Error::AlreadyReserved(m) => m.en().to_string(),
            Error::NotReady(m) => m.en().to_string(),
            Error::OutOfReach(m) => m.en().to_string(),
            Error::BindingStale(p) => {
                format!("the linked project directory was not found: {p}")
            }
            Error::FormatAhead(m) => m.en().to_string(),
            Error::Io(e) => format!("I/O error: {e}"),
            Error::Json(e) => format!("failed to parse JSON: {e}"),
            Error::Storage(s) => format!("store (engine) operation failed: {s}"),
            Error::StoreBusy(m) => format!("the store is in use by another process: {m}", m = m.en()),
        }
    }

    pub fn invalid(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Error::Invalid(Msg::new(en, ja))
    }

    pub fn not_found(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Error::NotFound(Msg::new(en, ja))
    }

    pub fn conflict(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Error::Conflict(Msg::new(en, ja))
    }

    pub fn already_reserved(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Error::AlreadyReserved(Msg::new(en, ja))
    }

    pub fn not_ready(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Error::NotReady(Msg::new(en, ja))
    }

    pub fn out_of_reach(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Error::OutOfReach(Msg::new(en, ja))
    }

    pub fn format_ahead(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Error::FormatAhead(Msg::new(en, ja))
    }

    pub fn store_busy(en: impl Into<String>, ja: impl Into<String>) -> Self {
        Error::StoreBusy(Msg::new(en, ja))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_message_is_english_gui_display_is_japanese() {
        let e = Error::not_found("task 'X' not found", "タスク 'X' が見つかりません");
        // The machine contract: the code stays fixed English.
        assert_eq!(e.code(), "not_found");
        // The CLI's all-English surface.
        assert_eq!(e.message_en(), "task 'X' not found");
        // The GUI's localized human surface (the to_string() a Tauri command returns) is Japanese.
        assert_eq!(e.to_string(), "タスク 'X' が見つかりません");
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
    fn structured_variants_render_both_languages() {
        // Already-structured detail absorbs the language difference in the prefix template alone.
        let e = Error::AmbiguousId { prefix: "01K".into(), candidates: vec!["a".into(), "b".into()] };
        assert_eq!(e.code(), "ambiguous_id");
        assert!(e.message_en().contains("is ambiguous"));
        assert!(e.to_string().contains("は曖昧です"));

        // Detail that arrives from a library already in English rides both surfaces as-is.
        let storage = Error::Storage("engine write failed".into());
        assert_eq!(storage.code(), "storage_error");
        assert!(storage.message_en().starts_with("store (engine) operation failed"));
        assert!(storage.to_string().starts_with("ストア（engine）の操作に失敗"));
    }
}
