//! `comment`: a task's timeline, and the preview a `show` prints of it.

use serde_json::json;

use amenbo_core::Store;
use amenbo_core::model::AttachmentTarget;

use crate::cli::*;
use crate::cmd::arg::body_arg;
use crate::cmd::attach::attach_add;
use crate::cmd::labels::{task_comment_label, task_label};
use crate::cmd::task::resolve_task;
use crate::output::{confirm, count_header, human, print_json, warn_body, write_envelope, CliError, Flags};

pub(crate) fn comment(store: &mut Store, flags: &Flags, sub: CommentCmd) -> Result<i32, CliError> {
    match sub {
        CommentCmd::Add { task, text } => {
            let text = body_arg(text)?;
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            // The author is our own facet; add_comment's author argument is the trace string for the audit log.
            let s = store.add_task_comment(tid, flags.facet()?, &text).map_err(CliError::from)?;
            warn_body(&text); // non-blocking readability hint on write (stderr)
            write_envelope(flags, "comment.add", "comment", serde_json::to_value(&s).unwrap(), None, false, format!("✓ Added comment: {}", task_label(tid)));
        }
        CommentCmd::List { task, limit, offset } => {
            let tid = resolve_task(store, &task).map_err(CliError::from)?;
            let result = store.comment_list(tid, offset, limit).map_err(CliError::from)?;
            if flags.json {
                print_json(&result);
            } else {
                human(flags, format!("{} — {}", count_header(result.count, result.total_matched, "comment"), result.task.name));
                for c in &result.comments {
                    human(flags, comment_line(amenbo_core::idref::RefKind::TaskComment, c));
                }
            }
        }
        CommentCmd::Rm { comment } => {
            let cid = resolve_live_task_comment(store, &comment)?;
            if !confirm(flags, "delete comment")? {
                return Ok(0);
            }
            let changed = store.remove_task_comment(cid, flags.facet()?).map_err(CliError::from)?;
            write_envelope(flags, "comment.rm", "comment", json!({ "id": cid, "deleted": true }), None, !changed, format!("✓ Deleted comment: {}", task_comment_label(cid)));
        }
        CommentCmd::Edit { comment, text } => {
            let text = body_arg(text)?;
            let cid = resolve_live_task_comment(store, &comment)?;
            let c = store.edit_task_comment(cid, &text).map_err(CliError::from)?;
            warn_body(&text); // non-blocking readability hint on write (stderr)
            write_envelope(flags, "comment.edit", "comment", serde_json::to_value(&c).unwrap(), Some(vec!["text".to_string()]), false, format!("✓ Edited comment: {}", task_comment_label(cid)));
        }
        CommentCmd::Attach { comment, source, url, name } => {
            // Look only in the task-comment table (as `comment rm` / `comment edit` do) — which table an id
            // belongs to is said by the command, not by the id.
            let cid = resolve_live_task_comment(store, &comment)?;
            return attach_add(store, flags, AttachmentTarget::TaskComment, cid, &source, url, name);
        }
    }
    Ok(0)
}

/// One comment as a human-readable line, shared by `comment list` and `decision comment list` — which is
/// why the ref's kind is passed in: the two listings read different tables, and the same id names a row in
/// each (`AMB-D-377`). It leads with the comment's ref: a comment carries no conversational number, so this
/// id is the only handle that can be passed to `comment rm` / `comment attach`, and a comment left out of
/// the listing is not addressable at all. It is namespaced like every other exposed ref and pastes straight
/// back — resolution reads `AMB-TC-<n>` / `AMB-DC-<n>` and the bare `<n>` alike. A comment that was edited
/// shows the edit time next to the post time — no revision history is kept, so this is the reader's only
/// clue that the text is not what they read a moment ago. An unedited comment adds nothing.
/// How many comments a `show` previews, newest first (`AMB-D-448`) — the rest are one command away.
const COMMENT_PREVIEW_COUNT: usize = 3;

/// The longest a previewed comment runs before it is cut (`AMB-D-448`), in characters. Long enough for
/// a first sentence to land, short enough that three of them cannot push the rest of a `show` off the
/// screen — which is the whole reason a preview is not the text.
const COMMENT_PREVIEW_CHARS: usize = 60;

/// One comment's text as a `show` previews it: a single line, cut to a readable length.
///
/// A preview is one line, so every run of whitespace — a newline included — folds to one space: a
/// comment written as a bullet list would otherwise take over the shape of the section it is being
/// previewed inside. The cut counts characters rather than bytes, so a Japanese comment is cut where a
/// reader would say it is, and never in the middle of one.
fn comment_preview(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= COMMENT_PREVIEW_CHARS {
        return line;
    }
    line.chars().take(COMMENT_PREVIEW_CHARS).collect::<String>() + "…"
}

/// The comments section both `show`s print (`AMB-D-448`), as the lines to write.
///
/// Same shape on either side, because the accident is the same one: a reader who does not notice that
/// anything was said. So the count is always marked, `(none)` included (`AMB-D-75`) — the newest three
/// are previewed under it, and the way to the full text is named on the last line, since a preview is
/// cut and a reader who needs the rest should not have to know which command holds it.
pub(crate) fn comment_section(
    comments: &[amenbo_core::query::CommentItem],
    full_text_command: &str,
) -> Vec<String> {
    if comments.is_empty() {
        return vec!["comments: (none)".to_string()];
    }
    let mut lines = vec![format!("comments ({}, newest first):", comments.len())];
    // The store lists oldest first. Reversed here so the newest is on top and cannot be missed.
    for c in comments.iter().rev().take(COMMENT_PREVIEW_COUNT) {
        lines.push(format!(
            "  [{}] {}: {}",
            c.created_at.to_rfc3339_z(),
            c.author.name,
            comment_preview(&c.text)
        ));
    }
    lines.push(format!("  full text: {full_text_command}"));
    lines
}

pub(crate) fn comment_line(kind: amenbo_core::idref::RefKind, c: &amenbo_core::query::CommentItem) -> String {
    let at = c.created_at.to_rfc3339_z();
    let edited = c.edited_at.map(|t| format!(" · edited {}", t.to_rfc3339_z())).unwrap_or_default();
    format!("  {}  [{at}{edited}] {}: {}", amenbo_core::idref::render(kind, c.id), c.author.name, c.text)
}

/// Resolve a live task-comment id (for `comment rm`).
pub(crate) fn resolve_live_task_comment(store: &Store, reference: &str) -> Result<i64, CliError> {
    let hits = store.resolve_task_comment(reference).map_err(CliError::from)?;
    pick_comment(hits, reference)
}

/// Resolve a live decision-comment id — the decision-side counterpart of [`resolve_live_task_comment`].
pub(crate) fn resolve_live_decision_comment(store: &Store, reference: &str) -> Result<i64, CliError> {
    let hits = store.resolve_decision_comment(reference).map_err(CliError::from)?;
    pick_comment(hits, reference)
}

fn pick_comment(hits: Vec<i64>, reference: &str) -> Result<i64, CliError> {
    hits.into_iter().next().ok_or_else(|| comment_not_found(reference))
}

/// A comment reference that names no row — in either comment table. The hint points at both listings: a
/// comment carries no conversational number, so the listing is the only place its id comes from.
pub(crate) fn comment_not_found(reference: &str) -> CliError {
    CliError {
        code: "not_found",
        message: format!("comment '{reference}' not found"),
        hint: Some("list the comments to find the id (`comment list <task>` / `decision comment list <decision>`)".to_string()),
        exit: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a `show` puts in place of a comment's text (`AMB-D-448`): one line, cut where a reader would
    /// cut it. The two halves are separate — a short comment written over several lines is folded and
    /// not cut, and a long one on a single line is cut and not folded.
    #[test]
    fn a_comment_preview_is_one_line_cut_at_a_readable_length() {
        assert_eq!(comment_preview("as it was written"), "as it was written");
        assert_eq!(
            comment_preview("a list:\n- one\n- two"),
            "a list: - one - two",
            "a preview is a line, so every run of whitespace folds to one space",
        );

        // The cut counts characters, so a Japanese comment is cut where a reader would say it is —
        // counting bytes would land inside one and hand back something that is not text.
        let long = "あ".repeat(COMMENT_PREVIEW_CHARS + 5);
        let cut = comment_preview(&long);
        assert_eq!(cut.chars().count(), COMMENT_PREVIEW_CHARS + 1, "the mark is what says it was cut");
        assert!(cut.ends_with('…'));
        assert!(cut.starts_with(&"あ".repeat(COMMENT_PREVIEW_CHARS)));

        // Exactly at the length is not cut: the mark means there is more, and there is not.
        let exact = "あ".repeat(COMMENT_PREVIEW_CHARS);
        assert_eq!(comment_preview(&exact), exact);
    }

    /// The comments section both `show`s print (`AMB-D-448`). The same shape either side, and the count
    /// is marked even at zero (`AMB-D-75`) — a reader who cannot tell "nothing was said" from "the
    /// timeline was not looked at" is the accident the section exists for.
    #[test]
    fn the_comments_section_marks_the_count_and_names_the_way_to_the_full_text() {
        use amenbo_core::query::CommentItem;
        use amenbo_core::view::Ref;

        assert_eq!(comment_section(&[], "amenbo comment list AMB-T-1"), vec!["comments: (none)"]);

        let said = |id: i64, text: &str| CommentItem {
            id,
            text: text.to_string(),
            author: Ref { id: "human".to_string(), name: "Human".to_string() },
            author_kind: Some(amenbo_core::model::ActorKind::Human),
            created_at: amenbo_core::time::Timestamp::now(),
            edited_at: None,
        };
        let all: Vec<CommentItem> = (1..=4).map(|i| said(i, &format!("said {i}"))).collect();
        let lines = comment_section(&all, "amenbo comment list AMB-T-1");

        assert_eq!(lines[0], "comments (4, newest first):", "the count is every comment, not the preview's");
        assert_eq!(lines.len(), 2 + COMMENT_PREVIEW_COUNT, "the header, three previews and the way on");
        assert!(lines[1].ends_with("said 4"), "newest first, whatever order the store lists them in");
        assert!(lines[3].ends_with("said 2"));
        assert_eq!(lines[4], "  full text: amenbo comment list AMB-T-1");
    }
}
