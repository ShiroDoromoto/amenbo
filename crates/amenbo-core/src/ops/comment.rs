//! Comment operations — the human-authored, permanent half of a task's timeline.
//!
//! Permanent comments go into their own table, `task_comment` ([`TaskComment`]). The other half of the
//! timeline — system events — has no table at all: it lives only in the ledger file
//! ([`crate::activity_log`]), so it is emitted through [`crate::store::Store::add_system_event`] with
//! payloads from [`crate::activity_log::event`].

use crate::error::{Error, Result};
use crate::model::{ActorKind, TaskComment};
use crate::ops::{emit_create, emit_update, Noun};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// The word for a comment (the English/Japanese pair for `not_found` messages). Whether the parent is a task
/// or a decision is carried by the id, so one word suffices.
pub(crate) const COMMENT_NOUN: Noun = Noun { en: "comment", ja: "コメント" };

/// Shared preamble for adding a comment: the empty-body check sits in one place, whatever the parent's kind
/// (task / decision) — separate schemas, shared code. Returns the new comment's `now`; the caller takes the
/// id and assembles the record itself, because id allocation differs per table (`task_comment` draws on the
/// activity sequence it shares with the ledger, [`crate::store_engine::read::next_activity_id`], while
/// `decision_comment` uses its own `next_id`) and this function does not know the parent table.
pub(crate) fn prepare_comment(text: &str) -> Result<Timestamp> {
    if text.trim().is_empty() {
        return Err(Error::invalid("a comment body cannot be empty", "コメント本文は空にできません"));
    }
    Ok(Timestamp::now())
}

/// Add a comment to a task (written to its own table, `task_comment`). The author is already resolved by the
/// caller. The write is issued straight into the caller's [`WriteTx`].
pub fn add_comment(
    tx: &WriteTx<'_>,
    task_id: i64,
    author_kind: ActorKind,
    text: &str,
) -> Result<TaskComment> {
    let now = prepare_comment(text)?;
    let comment = TaskComment {
        id: read::next_activity_id(tx.conn())?,
        task_id,
        author_kind: Some(author_kind),
        text: text.to_string(),
        created_at: now,
        updated_at: now,
        edited_at: None,
    };
    emit_create(tx, record::task_comment(&comment))?;
    Ok(comment)
}

/// Hard-delete a task comment. A comment that is already gone is a no-op (`false`). Comments are not
/// append-only: with a single store and a single user, a mistaken post is better gone than kept behind a
/// "retracted" history, and no ownership check applies either. A comment's attachments are polymorphic (no FK
/// to hang a cascade on), so the delete op sweeps them itself ([`crate::ops::sweep_polymorphic`]) — and, as
/// with `attach rm`, reclaiming the bytes of a blob that lost its last reference is the GC's job.
pub fn remove_comment(tx: &WriteTx<'_>, id: i64) -> Result<bool> {
    if read::task_comment(tx.conn(), id)?.is_none() {
        return Ok(false);
    }
    crate::ops::sweep_polymorphic(tx, "task_comment", id)?;
    tx.delete_record("task_comment", id)?;
    Ok(true)
}

/// Rewrite a task comment's body; not_found if it is gone. A mistaken post can be deleted
/// ([`remove_comment`]), but there is no reason to make someone delete and re-post a comment they only want
/// to fix — comments are edited in place, and no struck-through correction trail is kept (if the history is
/// worth keeping, write it into the body). That it was edited is carried by `edited_at`, which is what a
/// reader sees as "edited" — `updated_at` cannot stand in for it (it has second precision, so a fix within
/// the same second still equals `created_at`).
pub fn edit_comment(tx: &WriteTx<'_>, id: i64, text: &str) -> Result<TaskComment> {
    let before = read::task_comment(tx.conn(), id)?
        
        .ok_or_else(|| COMMENT_NOUN.not_found(id.to_string()))?;
    let now = prepare_comment(text)?;
    let after =
        TaskComment { text: text.to_string(), updated_at: now, edited_at: Some(now), ..before.clone() };
    emit_update(tx, record::task_comment(&before), record::task_comment(&after))?;
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActorKind, AttachmentTarget};
    use crate::ops::test_support::{mk_task, new_engine};

    /// Retracting a mistaken comment is a hard delete, and its attachments go with it.
    #[test]
    fn remove_comment_takes_its_attachments_with_it() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let tid = mk_task(tx, "タスク");
        let c = add_comment(tx, tid, ActorKind::Ai, "誤投稿").unwrap();
        crate::ops::attachment::add_url(
            tx,
            AttachmentTarget::TaskComment,
            c.id,
            "https://example.com/",
            None,
            ActorKind::Ai,
        )
        .unwrap();

        assert!(remove_comment(tx, c.id).unwrap());
        assert!(read::task_comment(tx.conn(), c.id).unwrap().is_none(), "the row itself goes (not a tombstone)");
        assert!(
            read::attachments_for_target(tx.conn(), "task_comment", c.id).unwrap().is_empty(),
            "polymorphic attachments are swept by the delete op (no FK can hold them)"
        );
    }

    #[test]
    fn remove_comment_is_a_noop_when_it_is_gone() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        assert!(!remove_comment(tx, 9999).unwrap());
    }

    /// A post you want to fix is rewritten in place, not deleted and re-posted. The id does not change (so
    /// its attachments stay put), and neither does its place in the timeline.
    #[test]
    fn edit_comment_rewrites_the_body_in_place() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let tid = mk_task(tx, "タスク");
        let c = add_comment(tx, tid, ActorKind::Ai, "誤字のある投稿").unwrap();

        let edited = edit_comment(tx, c.id, "直した投稿").unwrap();
        assert_eq!(edited.id, c.id, "the id does not change (this is not a new post)");
        assert_eq!(edited.text, "直した投稿");
        let stored = read::task_comment(tx.conn(), c.id).unwrap().unwrap();
        assert_eq!(stored.text, "直した投稿", "the truth source is rewritten");
        assert_eq!(stored.created_at, c.created_at, "the post time does not move");
    }

    #[test]
    fn edit_comment_rejects_an_empty_body_and_a_gone_comment() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let tid = mk_task(tx, "タスク");
        let c = add_comment(tx, tid, ActorKind::Ai, "本文").unwrap();

        assert!(edit_comment(tx, c.id, "   ").is_err(), "an empty body is rejected, just as add rejects it");
        assert_eq!(read::task_comment(tx.conn(), c.id).unwrap().unwrap().text, "本文", "on rejection it stays as it was");
        assert!(edit_comment(tx, 9999, "x").is_err(), "editing a comment that is gone is not_found (unlike delete, this is no noop)");
    }
}
