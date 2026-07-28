//! Attachment operations. An attachment on a task, a decision, or a comment on either comes in one of two
//! modes: `blob` (content-addressed ingest — the default) or `url` (an external link, not managed by us). A
//! blob's bytes live out of band in the content-addressed store ([`crate::blob::BlobStore`]); the truth
//! source — here — holds only the metadata: `blob_hash` / `filename` / `mime` / `size_bytes`. Deletion is
//! physical. The bytes are **not** erased at this layer — erase them inside the transaction and a rollback
//! puts the row back while the bytes stay gone. The delete op only reports the blob hash it let go;
//! confirming that nothing references it any more and reclaiming it is the caller's job, **after** the
//! commit (`Store::reclaim_blobs`). The full sweep in `store.gc_blobs()` remains as the catch-all for
//! whatever slips past.
//!
//! **Writes are engine SQL, directly.** Each mutator takes a [`WriteTx`] opened by the caller (the write
//! wrappers on [`crate::Store`]). There is one read-then-write — picking the trailing `order_key` — and
//! that read happens inside the same transaction: read it outside, and two attachments appended
//! concurrently to the same target take the same key.

use crate::error::{Error, Result};
use crate::model::{ActorKind, Attachment, AttachmentKind, AttachmentTarget};
use crate::ops::emit_create;
use crate::order;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// The `order_key` that appends to the end of this target's attachments (after the largest key among the
/// live ones). That maximum is asked of SQL **inside this operation's transaction**.
fn append_order_key(
    tx: &WriteTx<'_>,
    target_type: AttachmentTarget,
    target_id: i64,
) -> Result<String> {
    let last = read::max_attachment_order_key(tx.conn(), target_type.as_str(), target_id)?;
    Ok(order::key_between(last.as_deref(), None))
}

/// Create a `blob`-mode attachment. `blob_hash` / `size_bytes` describe the bytes the caller has already
/// ingested into [`crate::blob::BlobStore`]; `filename` is the original file name and `mime` the MIME type
/// already sniffed for it (optional).
#[allow(clippy::too_many_arguments)]
pub fn add_blob(
    tx: &WriteTx<'_>,
    target_type: AttachmentTarget,
    target_id: i64,
    blob_hash: &str,
    filename: &str,
    mime: Option<&str>,
    size_bytes: i64,
    created_by_kind: ActorKind,
) -> Result<Attachment> {
    let now = Timestamp::now();
    let a = Attachment {
        id: read::next_id(tx.conn(), "attachment")?,
        target_type,
        target_id,
        kind: AttachmentKind::Blob,
        blob_hash: Some(blob_hash.to_string()),
        filename: Some(filename.to_string()),
        mime: mime.map(str::to_string),
        size_bytes: Some(size_bytes),
        url: None,
        created_by_kind: Some(created_by_kind),
        order_key: append_order_key(tx, target_type, target_id)?,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::attachment(&a))?;
    Ok(a)
}

/// Whether a URL is acceptable as a url attachment: the scheme has to be a web one. **This is the shape of
/// the front door, not a defence at the point of use.** An attachment's url eventually reaches the OS
/// opener (`open` / `xdg-open` / `cmd start`), and an opener interprets whatever it is handed: `file:`
/// opens a local file, and a string beginning with `-` is eaten as an option to the command. The opening
/// side (GUI and CLI) rejects those too — but **as long as the rejected thing can be stored at all, the
/// defence has to be repeated on every consuming path, forever, without a gap.** So we refuse it at the
/// door.
pub fn is_web_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:"].iter().any(|p| lower.starts_with(p))
}

/// Create a `url`-mode attachment (an external link, not managed by us). `label` is the display name
/// (optional — callers fall back to the URL itself). The URL must carry a web scheme ([`is_web_url`]).
#[allow(clippy::too_many_arguments)]
pub fn add_url(
    tx: &WriteTx<'_>,
    target_type: AttachmentTarget,
    target_id: i64,
    url: &str,
    label: Option<&str>,
    created_by_kind: ActorKind,
) -> Result<Attachment> {
    let url = url.trim();
    if url.is_empty() {
        return Err(Error::invalid("a url attachment needs a url"));
    }
    if !is_web_url(url) {
        return Err(Error::invalid("a url attachment must be http, https or mailto"));
    }
    let now = Timestamp::now();
    let a = Attachment {
        id: read::next_id(tx.conn(), "attachment")?,
        target_type,
        target_id,
        kind: AttachmentKind::Url,
        blob_hash: None,
        filename: label.map(str::to_string),
        mime: None,
        size_bytes: None,
        url: Some(url.to_string()),
        created_by_kind: Some(created_by_kind),
        order_key: append_order_key(tx, target_type, target_id)?,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::attachment(&a))?;
    Ok(a)
}

/// Physically delete an attachment; a noop (`None`) if it was not there. The blob's bytes (the
/// content-addressed byte string) are not touched here — reclaiming a blob that has fallen to zero
/// references is GC's job, and it can only happen **after the commit** (a rollback brings the row back; it
/// does not bring the bytes back). Returns the blob hash the deleted attachment was pointing at (`None`
/// for a `url` attachment): the candidate for the caller (the write wrappers on [`crate::Store`]) to
/// reclaim once it has committed. After the row is gone there is nobody left to ask which blob was
/// orphaned, so it has to be picked up here.
pub fn remove(tx: &WriteTx<'_>, attachment_id: i64) -> Result<Option<Removed>> {
    let Some(existing) = read::attachment(tx.conn(), attachment_id)? else {
        return Ok(None);
    };
    tx.delete_record("attachment", attachment_id)?;
    Ok(Some(Removed { blob_hash: existing.blob_hash }))
}

/// The part of what [`remove`] deleted that GC has to hear about: the blob hash it let go (`None` for a
/// `url` attachment).
#[derive(Debug, Clone)]
pub struct Removed {
    pub blob_hash: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::with_tx;

    /// Add one `url` attachment (folds away the noise these tests share).
    fn url_on(tx: &WriteTx<'_>, tt: AttachmentTarget, target: i64, url: &str) -> Attachment {
        add_url(tx, tt, target, url, None, ActorKind::Ai).unwrap()
    }

    /// How many live attachments hang on that target.
    fn live_on(tx: &WriteTx<'_>, tt: AttachmentTarget, target: i64) -> usize {
        read::attachments_for_target(tx.conn(), tt.as_str(), target).unwrap().len()
    }

    #[test]
    fn blob_and_url_modes_carry_their_fields() {
        with_tx(|tx| {
            let blob = add_blob(
                tx,
                AttachmentTarget::Task,
                1001,
                "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                "report.pdf",
                Some("application/pdf"),
                1234,
                ActorKind::Ai,
            )
            .unwrap();
            assert_eq!(blob.kind, AttachmentKind::Blob);
            assert_eq!(blob.blob_hash.as_deref(), Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"));
            assert_eq!(blob.size_bytes, Some(1234));
            assert!(blob.url.is_none());

            let url = add_url(
                tx,
                AttachmentTarget::Decision,
                2001,
                "https://example.com/spec",
                Some("spec"),
                ActorKind::Human,
            )
            .unwrap();
            assert_eq!(url.kind, AttachmentKind::Url);
            assert_eq!(url.url.as_deref(), Some("https://example.com/spec"));
            assert!(url.blob_hash.is_none());
        });
    }

    #[test]
    fn empty_url_is_rejected() {
        with_tx(|tx| {
            assert!(add_url(tx, AttachmentTarget::Task, 1, "  ", None, ActorKind::Ai).is_err());
        });
    }

    /// Refused at the door: nothing that gets past here can make the OS opener open a `file:` later on.
    #[test]
    fn non_web_schemes_are_rejected_at_ingest() {
        with_tx(|tx| {
            for hostile in [
                "file:///etc/passwd",
                "javascript:alert(1)",
                "data:text/html;base64,PHNjcmlwdD4=",
                "-oProxyCommand=id", // shaped to be eaten as an option to the opener.
                "example.com",       // no scheme: an opener may read it as a local path.
            ] {
                let r = add_url(tx, AttachmentTarget::Task, 1, hostile, None, ActorKind::Ai);
                assert!(r.is_err(), "{hostile} must not be accepted");
            }
        });
    }

    #[test]
    fn web_schemes_are_accepted_case_insensitively_and_trimmed() {
        with_tx(|tx| {
            for ok in ["  https://example.com/x  ", "HTTP://example.com", "mailto:alice@example.com"] {
                let a = add_url(tx, AttachmentTarget::Task, 1, ok, None, ActorKind::Ai).unwrap();
                assert_eq!(a.url.as_deref(), Some(ok.trim())); // surrounding whitespace is not stored.
            }
        });
    }

    #[test]
    fn order_keys_append_in_sequence_per_target() {
        with_tx(|tx| {
            let a = url_on(tx, AttachmentTarget::Task, 1, "https://a");
            let b = url_on(tx, AttachmentTarget::Task, 1, "https://b");
            assert!(a.order_key < b.order_key, "later attachment sorts after earlier");
            // A different target starts its own sequence from the front.
            let c = url_on(tx, AttachmentTarget::Task, 2, "https://c");
            assert!(c.order_key <= a.order_key);
        });
    }

    #[test]
    fn target_type_round_trips_including_comments() {
        for t in [
            AttachmentTarget::Task,
            AttachmentTarget::Decision,
            AttachmentTarget::TaskComment,
            AttachmentTarget::DecisionComment,
        ] {
            assert_eq!(AttachmentTarget::parse(t.as_str()), Some(t));
        }
        assert_eq!(AttachmentTarget::parse("task_comment"), Some(AttachmentTarget::TaskComment));
        assert_eq!(AttachmentTarget::parse("decision_comment"), Some(AttachmentTarget::DecisionComment));
        assert_eq!(AttachmentTarget::parse("bogus"), None);
    }

    #[test]
    fn comment_attachments_are_separate_from_body_attachments() {
        // A comment's attachments live on their own target, distinct from the parent task/decision
        // body's attachments (so the per-comment timeline is preserved).
        with_tx(|tx| {
            url_on(tx, AttachmentTarget::Task, 1001, "https://body");
            url_on(tx, AttachmentTarget::TaskComment, 3001, "https://tc");
            url_on(tx, AttachmentTarget::DecisionComment, 4001, "https://dc");

            assert_eq!(live_on(tx, AttachmentTarget::Task, 1001), 1);
            assert_eq!(live_on(tx, AttachmentTarget::TaskComment, 3001), 1);
            assert_eq!(live_on(tx, AttachmentTarget::DecisionComment, 4001), 1);
            // The body attachment does not bleed into the comment target and vice-versa.
            assert_eq!(live_on(tx, AttachmentTarget::TaskComment, 1001), 0);
            assert_eq!(live_on(tx, AttachmentTarget::Task, 3001), 0);
        });
    }

    #[test]
    fn remove_takes_the_row_out_and_is_idempotent() {
        with_tx(|tx| {
            let a = url_on(tx, AttachmentTarget::Task, 1, "https://a");
            let removed = remove(tx, a.id).unwrap().expect("the attachment was there");
            // A url attachment owns no bytes, so it yields no GC candidate.
            assert_eq!(removed.blob_hash, None);
            assert_eq!(live_on(tx, AttachmentTarget::Task, 1), 0);
            // The second removal is a noop.
            assert!(remove(tx, a.id).unwrap().is_none());
            // So is an id that was never there.
            assert!(remove(tx, 999_999).unwrap().is_none());
        });
    }

    /// The shape the schema imposes on `blob_hash`: 64 hex digits, a BLAKE3 digest.
    const HASH: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    /// Removing a blob attachment hands the caller the hash of the bytes it let go — once the row is gone
    /// there is nobody to ask the engine which blob was orphaned. Whether the bytes actually die is the
    /// caller's call (another attachment may point at the same ones).
    #[test]
    fn removing_a_blob_attachment_reports_the_hash_it_let_go() {
        with_tx(|tx| {
            let a = add_blob(
                tx,
                AttachmentTarget::Task,
                1,
                HASH,
                "note.txt",
                Some("text/plain"),
                3,
                ActorKind::Ai,
            )
            .unwrap();
            let removed = remove(tx, a.id).unwrap().expect("the attachment was there");
            assert_eq!(removed.blob_hash.as_deref(), Some(HASH));
        });
    }
}
