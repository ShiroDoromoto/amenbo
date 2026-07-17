//! Last-seen (read-receipt) state: **per-device, never synced**. It drives the mailbox's unread-comment
//! test and the freshness of the badge.
//!
//! **It is persisted in the store's `read_receipt` table plus a `store_meta` scalar.** Reading and writing
//! it is [`crate::store::Store`]'s job — `read_receipts` / `mark_task_seen` / `mark_mailbox_seen` /
//! `retain_live_read_receipts` (implemented in [`crate::overview`]). This type is the **in-memory value and
//! the unread logic**, cut loose from any of that.
//!
//! A task is keyed by its `i64` primary key. Being unsynced device state is no reason to spell the same
//! identifier as a different type.
//!
//! Times are RFC3339 UTC strings: in `z` form, lexicographic order *is* chronological order, so the unread
//! test is a string comparison.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// This device's read-receipt state. The fields serialise as camelCase (the same on the GUI seam and in
/// JSON files).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadReceipts {
    /// Per task (by primary key), when it was last looked at (RFC3339 UTC). Updated when the detail pane
    /// is opened; a comment whose `created_at` is newer than this counts as unread. It crosses to the GUI
    /// as JSON, so the keys arrive as decimal strings (a JSON object key is always a string).
    #[serde(default)]
    pub tasks: BTreeMap<i64, String>,
    /// When the mailbox as a whole was last looked at. Updated when the mailbox view is opened — this is
    /// what keeps the unread badge fresh.
    #[serde(default)]
    pub mailbox_last_seen: Option<String>,
}

impl ReadReceipts {
    /// Mark a task seen: its last-seen time becomes `at` (RFC3339 UTC).
    pub fn mark_task(&mut self, task_id: i64, at: impl Into<String>) {
        self.tasks.insert(task_id, at.into());
    }

    /// Mark the whole mailbox seen: the badge's reference time moves forward to `at`.
    pub fn mark_mailbox(&mut self, at: impl Into<String>) {
        self.mailbox_last_seen = Some(at.into());
    }

    /// When a task was last looked at. `None` if never — which makes every comment on it unread.
    pub fn task_last_seen(&self, task_id: i64) -> Option<&str> {
        self.tasks.get(&task_id).map(String::as_str)
    }

    /// The mailbox test: does task `task_id` carry an unread comment addressed to me?
    ///
    /// `comments` is that task's comments as `(author_uid, author_is_human, at [RFC3339 UTC])`. My own
    /// words — the human facet's — never surface in my own mailbox: nothing notifies me of what I just
    /// said. Anything else (my AI facet, or someone else) counts as unread once it is newer than
    /// `last_seen`. On a task with no `last_seen` at all — never opened — a single comment that is not mine
    /// makes it unread. In RFC3339 `z` form lexicographic order is chronological order, so the comparison
    /// is a string comparison.
    pub fn has_unread_comment<'a>(
        &self,
        task_id: i64,
        me_uid: &str,
        comments: impl IntoIterator<Item = (&'a str, bool, &'a str)>,
    ) -> bool {
        let last_seen = self.task_last_seen(task_id);
        comments.into_iter().any(|(author_uid, author_is_human, at)| {
            // My own (human) words are out of scope: only an AI facet's or someone else's count as
            // something addressed to me.
            if author_uid == me_uid && author_is_human {
                return false;
            }
            match last_seen {
                Some(seen) => at > seen,
                None => true,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_and_query_last_seen() {
        // Building the value (`mark_*`) and reading it back (`task_last_seen`). The persistence round
        // trip is covered over in `crate::overview`.
        let mut rr = ReadReceipts::default();
        rr.mark_task(11, "2026-06-22T10:00:00Z");
        rr.mark_mailbox("2026-06-22T11:00:00Z");
        assert_eq!(rr.task_last_seen(11), Some("2026-06-22T10:00:00Z"));
        assert_eq!(rr.task_last_seen(12), None);
        assert_eq!(rr.mailbox_last_seen.as_deref(), Some("2026-06-22T11:00:00Z"));
    }

    // The mailbox test (an unread comment addressed to me). An author is `(uid, is_human, at)`.
    const ME: &str = "uMe";
    const OTHER: &str = "uBob";

    #[test]
    fn unseen_task_with_foreign_comment_is_unread() {
        // Someone else's comment on a task never opened (no `last_seen`) is unread.
        let rr = ReadReceipts::default();
        assert!(rr.has_unread_comment(11, ME, [(OTHER, true, "2026-06-22T10:00:00Z")]));
    }

    #[test]
    fn my_own_human_comment_is_never_unread() {
        // My own (human) words never surface in my own mailbox.
        let rr = ReadReceipts::default();
        assert!(!rr.has_unread_comment(11, ME, [(ME, true, "2026-06-22T10:00:00Z")]));
    }

    #[test]
    fn my_ai_facet_comment_is_unread() {
        // My own AI facet (`is_human = false`, same uid) is speaking *to* the human, so it is unread.
        // Solo, the human and the AI share a uid — without this distinction the mailbox would always be
        // empty.
        let rr = ReadReceipts::default();
        assert!(rr.has_unread_comment(11, ME, [(ME, false, "2026-06-22T10:00:00Z")]));
    }

    #[test]
    fn comment_at_or_before_last_seen_is_read() {
        let mut rr = ReadReceipts::default();
        rr.mark_task(11, "2026-06-22T10:00:00Z");
        // At `last_seen` or before it: read.
        assert!(!rr.has_unread_comment(11, ME, [(OTHER, true, "2026-06-22T10:00:00Z")]));
        assert!(!rr.has_unread_comment(11, ME, [(OTHER, true, "2026-06-22T09:59:59Z")]));
        // After it: unread.
        assert!(rr.has_unread_comment(11, ME, [(OTHER, true, "2026-06-22T10:00:01Z")]));
    }

    #[test]
    fn newest_foreign_comment_after_last_seen_wins_over_own() {
        // Several comments after the last-seen time. Mine are ignored; someone else's newer one is what
        // makes the task unread.
        let mut rr = ReadReceipts::default();
        rr.mark_task(11, "2026-06-22T10:00:00Z");
        let comments = [
            (ME, true, "2026-06-22T11:00:00Z"),    // mine (human) — ignored
            (OTHER, true, "2026-06-22T10:30:00Z"), // someone else, after last-seen — unread
        ];
        assert!(rr.has_unread_comment(11, ME, comments));
    }

    #[test]
    fn no_comments_is_read() {
        let rr = ReadReceipts::default();
        let none: [(&str, bool, &str); 0] = [];
        assert!(!rr.has_unread_comment(11, ME, none));
    }
}
