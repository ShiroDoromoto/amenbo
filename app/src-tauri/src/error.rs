//! The structured errors handed back to the GUI (webview). A Tauri command reports failure as the [`CmdError`]
//! defined here. i18n is settled at the reader, so instead of a bare string we carry a stable machine `code`
//! (canonically [`amenbo_core::Error::code`]) and per-code `fields` for interpolation. The front end (`errLabel`
//! in `app/src/core/i18n/index.ts`) maps `code` to a per-language template and fills it from the fields.
//!
//! The sentence is what a reader gets where no template exists, and it is English wherever it comes from:
//! core writes one (`AMB-D-413`), and a refusal raised by this layer ([`CmdError::coded`]) writes one too.
//! The reader's own language comes from the template, never from here.

use serde::Serialize;

/// One of the sentences a refusal is composed of: its own code, its own values, and the English it
/// falls back to. A refusal carries these when what it has to say is a sentence **plus a list** whose
/// length only the refusal knows — the reasons a reservation was turned away, say. Joining them is the
/// front end's to do, in the punctuation the reader's language joins with.
#[derive(Debug, Clone, Serialize)]
pub struct CmdErrorPart {
    /// The code naming this one sentence, so the front end can look up its template.
    pub code: String,
    /// The English sentence, for a part whose code has no template.
    pub message_en: String,
    /// The values this sentence is built from (`null` when it needs none).
    pub fields: serde_json::Value,
}

/// The structured error returned to the GUI (the `Err` type of a Tauri command).
///
/// `serde` serialises it to `{ code, message_en, fields, parts }`, and the front end's invoke receives
/// that object as the rejection reason.
#[derive(Debug, Clone, Serialize)]
pub struct CmdError {
    /// Stable machine-readable code (an i18n key; the contract is that it stays English). Core-originated codes come from [`amenbo_core::Error::code`].
    pub code: String,
    /// The sentence, in English — what a reader gets where no template covers the code. The `_en` is not
    /// a leftover from a pair: it is the reminder that this is *not* the reader's language, so a use
    /// site that reaches for it is visibly choosing the fallback over a template.
    pub message_en: String,
    /// Per-code structured values for interpolation (`null` for variants that have none).
    pub fields: serde_json::Value,
    /// The sentences this refusal is composed of, in reading order. Empty for the great majority,
    /// which say one thing and are done — and a boxed slice rather than a `Vec` because this type is
    /// the `Err` of every command's `Result`: its width is paid on the calls that succeed too, and the
    /// list is fixed the moment the refusal is built.
    pub parts: Box<[CmdErrorPart]>,
}

impl CmdError {
    fn new(code: impl Into<String>, message_en: impl Into<String>, fields: serde_json::Value) -> Self {
        CmdError {
            code: code.into(),
            message_en: message_en.into(),
            fields,
            parts: Box::default(),
        }
    }

    /// Build one from an explicit stable code, for errors the front end branches on by code
    /// (`init_pointer_exists` and the like). `fields` are the specifics the code's template drops
    /// into its sentence — the path, the candidates, the URL, the fingerprint.
    ///
    /// The reader's sentence is the template the front end holds for this code, in whatever language
    /// they read; the English one goes along as what a reader gets when there is no template for it —
    /// the same answer an untranslated key gets.
    pub fn coded(
        code: impl Into<String>,
        message_en: impl Into<String>,
        fields: serde_json::Value,
    ) -> Self {
        CmdError::new(code, message_en, fields)
    }
}

impl From<amenbo_core::Error> for CmdError {
    fn from(e: amenbo_core::Error) -> Self {
        use amenbo_core::Error as E;
        // Structured variants surrender their interpolation fields (the front end drops them into the code's template).
        // The free-form variants (NotFound/Invalid/Conflict/...) carry theirs on the message, but only where the
        // refusal names its own sentence (`Msg::coded`); the rest carry the whole sentence and nothing to
        // interpolate, hence null.
        let fields = match &e {
            E::AmbiguousId { prefix, candidates } => {
                serde_json::json!({ "prefix": prefix, "candidates": candidates })
            }
            E::BindingStale(path) => serde_json::json!({ "path": path }),
            _ => match e.fields() {
                Some(f) => {
                    serde_json::Value::Object(f.iter().map(|(k, v)| (k.to_string(), v.into())).collect())
                }
                None => serde_json::Value::Null,
            },
        };
        let mut out = CmdError::new(e.code(), e.message_en(), fields);
        out.parts = e.parts().iter().map(part_of).collect();
        out
    }
}

/// One of a message's parts, on the wire. A part always names its own code — a part that named nothing
/// would arrive as a sentence the front end could only print in English, which is the whole thing this
/// avoids — so the fallback here is unreachable in practice and kept only so the mapping is total.
fn part_of(m: &amenbo_core::Msg) -> CmdErrorPart {
    CmdErrorPart {
        code: m.code().map(|c| c.as_str().to_string()).unwrap_or_else(|| "error".to_string()),
        message_en: m.en().to_string(),
        fields: match m.fields().is_empty() {
            true => serde_json::Value::Null,
            false => serde_json::Value::Object(
                m.fields().iter().map(|(k, v)| (k.to_string(), v.into())).collect(),
            ),
        },
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
    /// and the sentence it came with is all it carries.
    fn from(s: String) -> Self {
        CmdError::new("error", s, serde_json::Value::Null)
    }
}

impl From<&str> for CmdError {
    fn from(s: &str) -> Self {
        CmdError::from(s.to_string())
    }
}

impl std::fmt::Display for CmdError {
    /// For `{e}` in logs and the like; prints the sentence.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message_en)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A core refusal that names its own sentence hands the webview that name and the values behind it,
    /// not just the prose. Without this the front end would hold a template it could never fill: the code
    /// would arrive and the `{ref}` would stay on screen as `{ref}`.
    #[test]
    fn a_core_error_that_names_its_sentence_surrenders_its_values() {
        let e = CmdError::from(amenbo_core::Error::NotFound(
            amenbo_core::Msg::new("task 'AMB-T-12' not found")
                .coded(amenbo_core::ErrorCode::NotFoundTask)
                .with("ref", "AMB-T-12"),
        ));

        assert_eq!(e.code, "not_found_task");
        assert_eq!(e.fields["ref"], "AMB-T-12");
    }

    /// A refusal composed of several sentences surrenders each one whole — its code and its values — rather
    /// than the English it happens to read as. Folding them into one string here is what would put English
    /// inside a reader's line, which is the thing the parts exist to avoid.
    #[test]
    fn a_refusal_with_several_reasons_surrenders_each_reason_on_its_own() {
        let e = CmdError::from(amenbo_core::Error::NotReady(
            amenbo_core::Msg::new("cannot reserve task AMB-T-12: blocker AMB-T-9 is not done")
                .coded(amenbo_core::ErrorCode::NotReady)
                .with("ref", "AMB-T-12")
                .part(
                    amenbo_core::Msg::new("blocker AMB-T-9 is not done")
                        .coded(amenbo_core::ErrorCode::NotReadyOpenBlocker)
                        .with("ref", "AMB-T-9"),
                ),
        ));

        assert_eq!(e.code, "not_ready");
        assert_eq!(e.fields["ref"], "AMB-T-12");
        assert_eq!(e.parts.len(), 1);
        assert_eq!(e.parts[0].code, "not_ready_open_blocker");
        assert_eq!(e.parts[0].fields["ref"], "AMB-T-9");
    }

    /// The other side of it: a refusal that names nothing keeps its family code and sends no fields, so
    /// the front end falls back to the sentence core wrote. That is the majority, and it has to keep
    /// reading.
    #[test]
    fn a_core_error_that_names_nothing_carries_only_its_sentence() {
        let e = CmdError::from(amenbo_core::Error::not_found("task 'X' not found"));

        assert_eq!(e.code, "not_found");
        assert!(e.fields.is_null(), "nothing to interpolate: {}", e.fields);
        assert_eq!(e.message_en, "task 'X' not found");
    }

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
        assert!(
            !e.message_en.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)),
            "no kana reaches the wire from here: {}",
            e.message_en
        );
    }
}
