import { useEffect, useState } from "react";
import { Markdown } from "./Markdown";
import { Attachments } from "./Attachments";
import { inTauri } from "../core/snapshot";
import { isEnterSubmit } from "../core/keys";
import { confirmDialog } from "../core/dialog";
import { t } from "../core/i18n";

/**
 * One comment row in a timeline. Tasks and decision records draw the body from different places (activity vs.
 * decision_comment) and attach to different targets, but the row looks the same and offers the same actions
 * (edit ✎, delete ✕, attachments), so it is defined once, here. Editing rewrites the body in place — it is not a
 * repost, so the id, the position in the thread and the attachments all survive. Deletion is physical and cannot
 * be undone, so it always goes through a confirm dialog (editing merely overwrites, and does not). An edited
 * comment gets an "edited, <when>" mark on its meta line. No revision history is kept by design, which makes
 * that mark the reader's only clue that the body is not the one they read a moment ago.
 * `startEditAt` is the outside world's signal to begin editing; every increment re-opens it, so the same row can
 * be opened again and again.
 */
export function CommentRow({ id, author, ago, editedAgo, text, target, onEdit, onRemove, startEditAt }: {
  id: number;
  author: { kind: string; name: string };
  ago: string;
  editedAgo?: string;
  text: string;
  target: "task_comment" | "decision_comment";
  onEdit: (text: string) => void;
  onRemove: () => void;
  startEditAt?: number;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  const startEdit = () => { setDraft(text); setEditing(true); };
  useEffect(() => {
    if (startEditAt === undefined) return;
    setDraft(text);
    setEditing(true);
    // The draft copies the body as it stood when editing opened; making `text` a dependency would let an outside change overwrite it mid-edit.
  }, [startEditAt]);
  const save = () => {
    if (!draft.trim()) return; // core rejects an empty body; here the button is simply unpressable.
    onEdit(draft.trim());
    setEditing(false);
  };
  const remove = async () => {
    if (await confirmDialog(t("comment.removeConfirm"))) onRemove();
  };

  return (
    <div className="comment">
      <div className="comment__meta">
        <span>
          {author.kind === "ai" ? "🤖" : "👤"} {author.name} · {ago}
          {editedAgo && <span className="faint"> · {t("comment.edited")} {editedAgo}</span>}
        </span>
        {inTauri() && !editing && (
          <>
            <button className="feed__action comment__act" title={t("comment.edit")} onClick={startEdit}>✎</button>
            <button className="feed__action comment__rm" title={t("comment.remove")} onClick={() => void remove()}>✕</button>
          </>
        )}
      </div>
      {editing ? (
        <div className="compose">
          <textarea
            className="compose__input"
            rows={4}
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); save(); }
              if (e.key === "Escape") setEditing(false);
            }}
          />
          <div className="compose__actions">
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("detail.notesHint")}</span>
            <span>
              <button className="btn" onClick={() => setEditing(false)}>{t("detail.cancel")}</button>
              <button className="btn btn--primary" style={{ marginLeft: 6 }} disabled={!draft.trim()} onClick={save}>
                {t("detail.save")}
              </button>
            </span>
          </div>
        </div>
      ) : (
        <div className="comment__body markdown">
          <Markdown>{text}</Markdown>
        </div>
      )}
      {inTauri() && <Attachments target={target} targetId={id} compact />}
    </div>
  );
}
