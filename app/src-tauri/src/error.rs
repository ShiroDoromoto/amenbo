//! The structured errors handed back to the GUI (webview). A Tauri command reports failure as the [`CmdError`]
//! defined here. i18n is settled at the reader, so instead of a bare string we carry a stable machine `code`
//! (canonically [`amenbo_core::Error::code`]) and per-code `fields` for interpolation. The front end (`errLabel`
//! in `app/src/core/i18n/index.ts`) maps `code` to a per-language template and fills it from the fields.
//!
//! The two sentence faces are what a reader gets where no template exists. An error from core carries both of
//! its own (`message` is the Japanese `Display`, `message_en` the English one); a refusal raised by this layer
//! ([`CmdError::coded`]) writes only English, and its reader's sentence is the template.

use serde::Serialize;

/// The structured error returned to the GUI (the `Err` type of a Tauri command).
///
/// `serde` serialises it to `{ code, message, message_en, fields }`, and the front end's invoke
/// receives that object as the rejection reason.
#[derive(Debug, Clone, Serialize)]
pub struct CmdError {
    /// Stable machine-readable code (an i18n key; the contract is that it stays English). Core-originated codes come from [`amenbo_core::Error::code`].
    pub code: String,
    /// The human-facing Japanese sentence (core's `Display`). A `coded` error has no Japanese of its
    /// own and repeats the English one here.
    pub message: String,
    /// The human-facing English sentence (core's [`amenbo_core::Error::message_en`]).
    pub message_en: String,
    /// Per-code structured values for interpolation (`null` for variants that have none).
    pub fields: serde_json::Value,
}

impl CmdError {
    fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        message_en: impl Into<String>,
        fields: serde_json::Value,
    ) -> Self {
        CmdError {
            code: code.into(),
            message: message.into(),
            message_en: message_en.into(),
            fields,
        }
    }

    /// Build one from an explicit stable code, for errors the front end branches on by code
    /// (`init_pointer_exists` and the like). `fields` are the specifics the code's template drops
    /// into its sentence — the path, the candidates, the URL, the fingerprint.
    ///
    /// Only English is written here. The reader's sentence is the template the front end holds for
    /// this code, in whatever language they read; the English one goes along as what a reader gets
    /// when there is no template for it — the same answer an untranslated key gets.
    pub fn coded(
        code: impl Into<String>,
        message_en: impl Into<String>,
        fields: serde_json::Value,
    ) -> Self {
        let message_en = message_en.into();
        CmdError::new(code, message_en.clone(), message_en, fields)
    }
}

impl From<amenbo_core::Error> for CmdError {
    fn from(e: amenbo_core::Error) -> Self {
        use amenbo_core::Error as E;
        // Structured variants surrender their interpolation fields (the front end drops them into the code's template).
        // Free-form variants (NotFound/Invalid/Conflict/...) carry the whole sentence in message/message_en, hence null.
        let fields = match &e {
            E::AmbiguousId { prefix, candidates } => {
                serde_json::json!({ "prefix": prefix, "candidates": candidates })
            }
            E::BindingStale(path) => serde_json::json!({ "path": path }),
            _ => serde_json::Value::Null,
        };
        CmdError::new(e.code(), e.to_string(), e.message_en(), fields)
    }
}

impl From<amenbo_core::store_engine::StoreEngineError> for CmdError {
    /// The type `store_engine::read::*` returns directly. Routed through core's `Error::Storage` (code `storage_error`) so everything lines up.
    fn from(e: amenbo_core::store_engine::StoreEngineError) -> Self {
        CmdError::from(amenbo_core::Error::from(e))
    }
}

impl From<String> for CmdError {
    /// An ad-hoc error from outside core (GUI-local handling). It has no stable code, so it gets the generic `"error"`,
    /// and both language faces carry the same sentence.
    fn from(s: String) -> Self {
        CmdError::new("error", s.clone(), s, serde_json::Value::Null)
    }
}

impl From<&str> for CmdError {
    fn from(s: &str) -> Self {
        CmdError::from(s.to_string())
    }
}

impl std::fmt::Display for CmdError {
    /// For `{e}` in logs and the like; prints the `message` face.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A refusal this layer raises carries the code, the values, and one English sentence — and no
    /// second language. Writing one here would be a dictionary in Rust, which is the thing that
    /// cannot be carried to nineteen languages; the sentence the reader sees is composed from the
    /// code and these fields, on the side that holds the dictionary.
    #[test]
    fn a_coded_refusal_carries_values_and_one_language() {
        let e = CmdError::coded(
            "binding_nested_tree",
            "this folder is already inside an amenbo-managed tree (bound at /work/repo)",
            serde_json::json!({ "path": "/work/repo" }),
        );

        assert_eq!(e.code, "binding_nested_tree");
        assert_eq!(e.fields["path"], "/work/repo", "the value the sentence is built from is sent apart from it");
        assert_eq!(e.message, e.message_en, "there is one sentence, and it is the English one");
        assert!(
            !e.message_en.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)),
            "no kana reaches the wire from here: {}",
            e.message_en
        );
    }
}
