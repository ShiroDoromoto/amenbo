import { useEffect, useState } from "react";
import type { Actor } from "../mock/types";
import { Markdown } from "./Markdown";
import { FacetAvatar, When } from "./atoms";
import { Attachments } from "./Attachments";
import { inTauri } from "../core/snapshot";
import { asTyped, isEnterSubmit } from "../core/keys";
import { confirmDialog } from "../core/dialog";
import { t, errText } from "../core/i18n";
import { ErrorNote } from "./ErrorNote";
import { Icon } from "./Icon";

/**
 * One comment row in a timeline. Tasks and decision records draw the body from different places (activity vs.
 * decision_comment) and attach to different targets, but the row looks the same and offers the same actions
 * (edit, delete, attachments), so it is defined once, here. Editing rewrites the body in place — it is not a
 * repost, so the id, the position in the thread and the attachments all survive. Deletion is physical and cannot
 * be undone, so it always goes through a confirm dialog (editing merely overwrites, and does not). An edited
 * comment gets an "edited, <when>" mark on its meta line. No revision history is kept by design, which makes
 * that mark the reader's only clue that the body is not the one they read a moment ago.
 * `startEditAt` is the outside world's signal to begin editing; every increment re-opens it, so the same row can
 * be opened again and again.
 */
export function CommentRow({ id, author, at, editedAt, text, target, onEdit, onRemove, startEditAt }: {
  id: number;
  author: Actor;
  at: string;
  editedAt?: string;
  text: string;
  target: "task_comment" | "decision_comment";
  // A failing write must not read as a success. Both may reject, so they may return a promise to await; a
  // synchronous `() => void` still satisfies the type (its result awaits to `undefined`).
  onEdit: (text: string) => void | Promise<void>;
  onRemove: () => void | Promise<void>;
  startEditAt?: number;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const startEdit = () => { setError(null); setDraft(text); setEditing(true); };
  const cancelEdit = () => { setError(null); setEditing(false); };
  useEffect(() => {
    if (startEditAt === undefined) return;
    setError(null);
    setDraft(text);
    setEditing(true);
    // The draft copies the body as it stood when editing opened; making `text` a dependency would let an outside change overwrite it mid-edit.
  }, [startEditAt]);
  // Await the edit before closing the box: on success it closes; on a refusal it stays open with the text intact
  // and the error shown, so the draft is never lost and retrying costs nothing.
  const save = async () => {
    const body = draft.trim();
    if (!body) return; // core rejects an empty body; here the button is simply unpressable.
    setError(null);
    try {
      await onEdit(body);
      setEditing(false);
    } catch (e) {
      setError(errText(e));
    }
  };
  // Deletion is physical; a refusal must be surfaced, not swallowed as though the row were gone.
  const remove = async () => {
    if (!(await confirmDialog(t("comment.removeConfirm")))) return;
    setError(null);
    try {
      await onRemove();
    } catch (e) {
      setError(errText(e));
    }
  };

  return (
    <div className="comment">
      <div className="comment__meta">
        <span>
          <FacetAvatar actor={author} /> {author.name} · <When at={at} editedAt={editedAt} />
        </span>
        {inTauri() && !editing && (
          <>
            <button className="feed__action comment__act" title={t("comment.edit")} onClick={startEdit}><Icon name="pencil" /></button>
            <button className="feed__action comment__rm" title={t("comment.remove")} onClick={() => void remove()}><Icon name="close" /></button>
          </>
        )}
      </div>
      {editing ? (
        <div className="compose">
          <textarea
            {...asTyped}
            className="compose__input"
            rows={4}
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void save(); }
              if (e.key === "Escape") cancelEdit();
            }}
          />
          <div className="compose__actions">
            <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("detail.notesHint")}</span>
            <span>
              <button className="btn" onClick={cancelEdit}>{t("detail.cancel")}</button>
              <button className="btn btn--primary" style={{ marginLeft: 6 }} disabled={!draft.trim()} onClick={() => void save()}>
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
      {error && <ErrorNote>{error}</ErrorNote>}
      {inTauri() && <Attachments target={target} targetId={id} compact />}
    </div>
  );
}
