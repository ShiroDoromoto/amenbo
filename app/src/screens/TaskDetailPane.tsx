import { useEffect, useRef, useState } from "react";
import { Markdown } from "../components/Markdown";
import { Attachments } from "../components/Attachments";
import { Commits } from "../components/Commits";
import { CommentRow } from "../components/CommentRow";
import { useStore } from "../store/store";
import { dataAdapter } from "../mock/adapter";
import { getSnapshot, inTauri } from "../core/snapshot";
import { useTask } from "../core/reads";
import { addComment as mutAddComment, editComment as mutEditComment, removeComment as mutRemoveComment, fetchTaskDimensions } from "../core/mutations";
import { loadTaskActivity } from "../core/activity";
import { confirmDialog } from "../core/dialog";
import {
  DueChip, FacetAvatar, PriorityDot, TaskIdChip,
} from "../components/atoms";
import { errText, priorityLabel, statusLabel, t, tf } from "../core/i18n";
import { isEnterSubmit } from "../core/keys";
import { useRefNav } from "../core/refNav";
import type { Actor, ActivityItem, Facet, Membership, Priority, Status, TaskCard } from "../mock/types";

// The memberships to display. The Tauri DTO already carries task.memberships (names included); the mock
// fallback builds a single entry by resolving the primary membership (projectId) against snapshot.projects.
function membershipsOf(task: TaskCard): Membership[] {
  if (task.memberships.length) return task.memberships;
  if (!task.projectId) return [];
  const p = getSnapshot().projects.find((pp) => pp.id === task.projectId);
  if (!p) return [];
  return [{ project: { id: p.id, name: p.name } }];
}

const TAB_KEYS = ["detail", "activity"] as const;
type TabKey = (typeof TAB_KEYS)[number];

const COMMENT_PAGE = 20; // Bounded memory: how many comments render initially (older ones load on demand).

/**
 * The detail pane for one task. Opening it marks the task seen (a comment arriving while you look at it may as
 * well count as read). Comments are fetched per-task and only the newest `commentLimit` of them render — but a
 * row named by "✎" that falls outside the window (i.e. is old) would be a dead button, so the window is widened
 * far enough to include it. Editing a comment does not change the comment count, so the refetch effect (keyed on
 * commentCount) never fires; the writer therefore re-reads this task's activity itself. Resolving the numbers in
 * body links (`AMB-T-<n>`) and assigning dimension values both take the primary project as their context, because
 * that is where the axes live. A dimension assignment updates this pane's map optimistically (cards do not carry
 * dimensions). Focusing the comment box for "↩ reply" cannot happen at request time — the textarea may not be
 * rendered yet — so it sets a flag and lands the focus on the render that produces it.
 */
export function TaskDetailPane({
  taskId, onDeleted, onDirtyChange, onSelectDecision, focusCommentAt, editCommentAt,
}: {
  taskId: number;
  onDeleted?: () => void;
  /** Report unsaved input to the parent (AppShell), which guards against discarding it on outside-click / ✕. */
  onDirtyChange?: (dirty: boolean) => void;
  onSelectDecision?: (id: number) => void;
  /** Nonce that focuses the comment box when opened via "↩ reply" in the activity feed. Every increment re-focuses, so you can reply to the same task again and again. undefined = an ordinary selection. */
  focusCommentAt?: number;
  /** The comment to open in edit mode when opened via "✎" in the activity feed. The nonce lets the same comment be re-opened; if it is an old comment, the render window (commentLimit) widens to reach its row. */
  editCommentAt?: { commentId: number; nonce: number };
}) {
  const store = useStore();
  const refNav = useRefNav();
  const [tab, setTab] = useState<TabKey>("detail");
  const [comment, setComment] = useState("");
  const [commentError, setCommentError] = useState<string | null>(null);
  const [editingNotes, setEditingNotes] = useState(false);
  const [notesDraft, setNotesDraft] = useState("");
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  // Cancelling with Escape also fires blur, which runs saveTitle — this flag makes that save a no-op.
  const titleCancelRef = useRef(false);
  const [commentLimit, setCommentLimit] = useState(COMMENT_PAGE);
  const commentRef = useRef<HTMLTextAreaElement>(null);
  const pendingCommentFocus = useRef(false);
  // The pane fetches comments per-task: the snapshot's window of the latest 100 would push an old task's
  // comments out of view and leave the list empty. Outside Tauri (mock) this stays null and we fall back to the store.
  const [taskActivity, setTaskActivity] = useState<ActivityItem[] | null>(null);
  const [dimValues, setDimValues] = useState<Record<number, number>>({});
  const task = useTask(taskId);
  const commentCount = task?.comments ?? 0; // Grows on a post, which is what triggers the refetch
  useEffect(() => {
    if (!inTauri()) { setTaskActivity(null); return; }
    let alive = true;
    loadTaskActivity(taskId).then((items) => { if (alive) setTaskActivity(items); });
    store.markSeen(taskId);
    return () => { alive = false; };
  }, [taskId, commentCount]);
  // Pull this task's dimension assignments from the read-model (Tauri only; empty in the mock).
  useEffect(() => {
    if (!inTauri()) { setDimValues({}); return; }
    let alive = true;
    fetchTaskDimensions(taskId).then((rows) => {
      if (!alive) return;
      const m: Record<number, number> = {};
      for (const r of rows) m[r.dimensionId] = r.valueId;
      setDimValues(m);
    }).catch(() => {});
    return () => { alive = false; };
  }, [taskId]);
  // Unsaved input means: notes are being edited and the draft differs from what is stored, or a comment draft is
  // still sitting there. On unmount it always resets to false, so nothing carries over to the next task opened.
  const notesDirty = editingNotes && notesDraft !== (task?.notes ?? "");
  const titleDirty = editingTitle && titleDraft.trim() !== (task?.title ?? "");
  const commentDirty = comment.trim() !== "";
  useEffect(() => {
    onDirtyChange?.(notesDirty || titleDirty || commentDirty);
    return () => onDirtyChange?.(false);
  }, [notesDirty, titleDirty, commentDirty, onDirtyChange]);
  useEffect(() => {
    if (focusCommentAt === undefined) return;
    pendingCommentFocus.current = true;
    setTab("detail");
  }, [focusCommentAt]);
  useEffect(() => {
    if (!pendingCommentFocus.current || tab !== "detail") return;
    const el = commentRef.current;
    if (!el) return; // Not rendered yet (the task is still loading) — land it on the next render.
    pendingCommentFocus.current = false;
    el.focus();
    el.scrollIntoView?.({ block: "nearest" });
  });
  useEffect(() => {
    if (editCommentAt === undefined) return;
    setTab("detail");
  }, [editCommentAt?.nonce]);
  const roster = dataAdapter.listRoster();
  if (!task) return <div className="rightpane__empty">{t("detail.notFound")}</div>;

  const notesProjectId = membershipsOf(task)[0]?.project.id ?? task.projectId ?? null;
  const axisProject = notesProjectId ? getSnapshot().projects.find((p) => p.id === notesProjectId) : undefined;

  const startEditNotes = () => { setNotesDraft(task.notes ?? ""); setEditingNotes(true); };

  // Title editing. Saving rejects an empty title (core rejects it too) and writes only when something changed.
  // Escape uses titleCancelRef to skip the save that blur would otherwise trigger. Every path closes edit mode.
  const startEditTitle = () => { setTitleDraft(task.title); setEditingTitle(true); };
  const saveTitle = () => {
    if (titleCancelRef.current) { titleCancelRef.current = false; setEditingTitle(false); return; }
    const next = titleDraft.trim();
    if (next && next !== task.title) store.setTitle(taskId, next);
    setEditingTitle(false);
  };

  const saveNotes = () => { store.setNotes(taskId, notesDraft); setEditingNotes(false); };

  // This task's comments, oldest first (activity comes newest-first, so reverse it). Under Tauri that is the
  // per-task fetch (taskActivity); in the mock it falls back to filtering the store's window.
  const commentSource = taskActivity !== null
    ? taskActivity.filter((a) => a.kind === "comment")
    : store.listActivity().filter((a) => a.kind === "comment" && a.target.id === taskId);
  const allComments = commentSource.slice().reverse();
  const editIndex = editCommentAt ? allComments.findIndex((c) => c.id === editCommentAt.commentId) : -1;
  const limit = editIndex >= 0 ? Math.max(commentLimit, allComments.length - editIndex) : commentLimit;
  const olderCount = Math.max(0, allComments.length - limit);
  const comments = allComments.slice(olderCount);

  // Await the post and only clear the box once it lands: a refused comment used to blank the input, losing the
  // body the user just wrote. On failure the text stays put and the error is shown, so retrying costs nothing.
  const submitComment = async () => {
    const body = comment.trim();
    if (!body) return;
    setCommentError(null);
    try {
      await mutAddComment(taskId, body);
      setComment("");
    } catch (e) {
      setCommentError(errText(e));
    }
  };
  const editComment = async (commentId: number, text: string) => {
    await mutEditComment(commentId, taskId, text);
    if (inTauri()) setTaskActivity(await loadTaskActivity(taskId));
  };
  // Mirror editComment: await the write (so a refusal reaches CommentRow's error surface instead of a swallowed
  // toast) and reload the local activity the same way. Not routed through `store.removeComment`, which drops the
  // promise on the floor.
  const removeComment = async (commentId: number) => {
    await mutRemoveComment(commentId, taskId);
    if (inTauri()) setTaskActivity(await loadTaskActivity(taskId));
  };
  const removeTask = async () => {
    if (await confirmDialog(tf("detail.deleteConfirm", { title: task.title }))) {
      store.deleteTask(taskId);
      onDeleted?.();
    }
  };

  return (
    <div className="detail">
      <div className="detail__tabs">
        {TAB_KEYS.map((tk) => (
          <button key={tk} className={`detail__tab ${tk === tab ? "detail__tab--active" : ""}`} onClick={() => setTab(tk)}>
            {t(`detail.tab.${tk}`)}
          </button>
        ))}
      </div>

      {tab === "detail" && (
        <div className="detail__body">
          <div className="detail__title">
            <span className="row__check" style={{ cursor: "pointer", marginRight: 6 }} onClick={() => store.toggleDone(taskId)}>
              {task.status === "done" ? "☑" : "◻"}
            </span>
            {editingTitle ? (
              <input
                className="compose__input"
                style={{ minHeight: "unset", fontSize: "inherit", fontWeight: "inherit", flex: 1 }}
                autoFocus
                value={titleDraft}
                placeholder={t("compose.titlePh")}
                onChange={(e) => setTitleDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (isEnterSubmit(e) && !e.shiftKey) { e.preventDefault(); e.currentTarget.blur(); }
                  if (e.key === "Escape") { e.preventDefault(); titleCancelRef.current = true; e.currentTarget.blur(); }
                }}
                onBlur={saveTitle}
              />
            ) : (
              <>
                <span style={{ cursor: "text" }} title={t("detail.edit")} onDoubleClick={startEditTitle}>{task.title}</span>
                <button className="feed__action" style={{ marginLeft: 6 }} onClick={startEditTitle}>{t("detail.edit")}</button>
              </>
            )}
            <TaskIdChip id={taskId} />
          </div>

          <div className="detail__actions">
            <select className="btn" value={task.status} onChange={(e) => store.setStatus(taskId, e.target.value as Status)}>
              {(["todo", "in_progress", "done", "blocked"] as const).map((s) => (
                <option key={s} value={s}>{statusLabel(s)}</option>
              ))}
            </select>
            <PriorityDot priority={task.priority} />
            <select
              className="btn"
              value={task.priority ?? ""}
              title={t("detail.priority")}
              onChange={(e) => store.setPriority(taskId, e.target.value === "" ? null : (e.target.value as Priority))}
            >
              <option value="">{t("detail.priorityNone")}</option>
              {(["high", "medium", "low"] as const).map((p) => (
                <option key={p} value={p}>{priorityLabel(p)}</option>
              ))}
            </select>
            <DueChip due={task.due} label={task.dueLabel} />
          </div>

          <div className="detail__field">
            <span className="detail__flabel">{t("detail.assignee")}</span>
            <span>
              <AssigneePicker
                current={task.assignee ?? null}
                roster={roster}
                onPick={(kind) => {
                  if (kind === "") store.setAssignee(taskId, null, "human");
                  else store.setAssignee(taskId, kind, kind);
                }}
              />
            </span>
          </div>
          <div className="detail__field">
            <span className="detail__flabel">{t("detail.membership")}</span>
            <span>
              {(() => {
                const memberships = membershipsOf(task);
                if (memberships.length === 0) return <span className="faint">{t("detail.none")}</span>;
                // Classification is attached and detached in the dimension selects below, so this row shows only the project name.
                return memberships[0].project.name;
              })()}
            </span>
          </div>
          {axisProject && axisProject.dimensions.map((dim) => (
            <div className="detail__field" key={dim.id}>
              <span className="detail__flabel">{dim.name}</span>
              <span>
                <select
                  className="card__status"
                  value={dimValues[dim.id] ?? ""}
                  onChange={(e) => {
                    const valueId = Number(e.target.value);
                    const prev = dimValues[dim.id];
                    if (valueId) {
                      store.setTaskDimensionValue(task.id, valueId);
                      setDimValues((m) => ({ ...m, [dim.id]: valueId }));
                    } else if (prev) {
                      store.unsetTaskDimensionValue(task.id, prev);
                      setDimValues((m) => { const n = { ...m }; delete n[dim.id]; return n; });
                    }
                  }}
                >
                  <option value="">{t("detail.none")}</option>
                  {dim.values.map((v) => (
                    <option key={v.id} value={v.id}>{v.name}</option>
                  ))}
                </select>
              </span>
            </div>
          ))}
          {task.blockedBy && task.blockedBy.length > 0 && (
            <div className="detail__field">
              <span className="detail__flabel">{t("detail.blockedBy")}</span>
              <span title={t("detail.blockedByHint")}>
                {task.blockedBy.map((b) => (
                  <button
                    type="button"
                    className="chip chip--link"
                    key={b.id}
                    style={{ marginRight: 4 }}
                    onClick={() => refNav.selectTask?.(b.id)}
                  >
                    ⛔ {b.name}
                  </button>
                ))}
              </span>
            </div>
          )}
          {task.linkedDecisions && task.linkedDecisions.length > 0 && (
            <div className="detail__field">
              <span className="detail__flabel">{t("detail.linkedDecisions")}</span>
              <span>
                {task.linkedDecisions.map((d, i) => {
                  const unsettled = task.blockedByDecisions?.some((b) => b.id === d.id) ?? false;
                  return (
                    <span key={d.id}>
                      {i > 0 && ", "}
                      <button
                        className="feed__action"
                        style={{ padding: "0 4px" }}
                        title={unsettled ? t("detail.premiseUnsettled") : undefined}
                        onClick={() => onSelectDecision?.(d.id)}
                      >
                        {unsettled && "⚠ "}{d.ref ?? ""} {d.name ?? t("dec.unknownName")}
                      </button>
                    </span>
                  );
                })}
              </span>
            </div>
          )}

          <div className="detail__sep" />

          <div>
            <div className="detail__section-h">
              {t("detail.notes")}
              {!editingNotes && (
                <button className="feed__action" style={{ marginLeft: 8 }} onClick={startEditNotes}>
                  {task.notes ? t("detail.edit") : `＋ ${t("detail.add")}`}
                </button>
              )}
            </div>
            {editingNotes ? (
              <div className="compose">
                <textarea
                  className="compose__input"
                  rows={6}
                  autoFocus
                  value={notesDraft}
                  placeholder={t("detail.notesPh")}
                  onChange={(e) => setNotesDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); saveNotes(); }
                    if (e.key === "Escape") setEditingNotes(false);
                  }}
                />
                <div className="compose__actions">
                  <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("detail.notesHint")}</span>
                  <span>
                    <button className="btn" onClick={() => setEditingNotes(false)}>{t("detail.cancel")}</button>
                    <button className="btn btn--primary" style={{ marginLeft: 6 }} onClick={saveNotes}>{t("detail.save")}</button>
                  </span>
                </div>
              </div>
            ) : task.notes ? (
              <div className="notes notes--md markdown" onDoubleClick={startEditNotes}>
                <Markdown>{task.notes}</Markdown>
              </div>
            ) : (
              <div className="faint" style={{ fontSize: "var(--fs-sm)" }}>{t("detail.noNotes")}</div>
            )}
          </div>

          <div className="detail__sep" />

          {inTauri() && <Attachments target="task" targetId={taskId} />}

          <div className="detail__sep" />

          {inTauri() && <Commits taskId={taskId} />}

          <div className="detail__sep" />

          <div>
            <div className="detail__section-h">{t("detail.activityCategory")} · 💬 {task.comments}</div>
            {comments.length === 0 ? (
              <div className="faint" style={{ marginTop: 6, fontSize: "var(--fs-sm)" }}>{t("detail.noComments")}</div>
            ) : (
              <div className="comments">
                {olderCount > 0 && (
                  <button className="feed__action" onClick={() => setCommentLimit((n) => n + COMMENT_PAGE)}>
                    {tf("common.loadMore", { n: olderCount })}
                  </button>
                )}
                {comments.map((c) => (
                  <CommentRow
                    key={c.id}
                    id={c.id}
                    author={c.author}
                    ago={c.ago}
                    editedAgo={c.editedAgo}
                    text={c.text ?? ""}
                    target="task_comment"
                    onEdit={(text) => editComment(c.id, text)}
                    onRemove={() => removeComment(c.id)}
                    startEditAt={editCommentAt?.commentId === c.id ? editCommentAt.nonce : undefined}
                  />
                ))}
              </div>
            )}
            <div className="compose">
              <textarea
                ref={commentRef}
                className="compose__input"
                rows={3}
                placeholder={t("detail.commentPh")}
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                onKeyDown={(e) => {
                  if (isEnterSubmit(e) && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void submitComment(); }
                }}
              />
              {commentError && <div className="newproj__error" role="alert">⚠ {commentError}</div>}
              <div className="compose__actions">
                <span className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("detail.commentHint")}</span>
                <button className="btn btn--primary" disabled={!comment.trim()} onClick={() => void submitComment()}>{t("detail.send")}</button>
              </div>
            </div>
          </div>

          <div className="meta">
            {t("detail.created")}: {task.createdBy ? `${task.createdBy.name}（${task.createdBy.kind === "ai" ? t("facet.ai") : t("facet.human")}）` : "—"} · id {task.id} · {t("detail.restoreHint")}
          </div>
          <div className="detail__danger">
            <button className="btn btn--danger" onClick={removeTask} title={t("detail.deleteTip")}>🗑 {t("detail.delete")}</button>
          </div>
        </div>
      )}

      {tab === "activity" && (
        <div className="detail__body">
          {taskActivity === null || taskActivity.length === 0 ? (
            <span className="faint">{t("detail.noActivity")}</span>
          ) : (
            <div className="feed">
              {taskActivity.map((it) => (
                <div className="feed__item" key={it.id}>
                  <div className="feed__body">
                    <div className="feed__line">
                      <strong>{it.author.name}</strong>{" "}
                      {it.kind === "comment" ? `「${it.text}」` : it.event?.text}
                    </div>
                    <div className="feed__meta">
                      <span>{it.ago}{it.editedAgo && <span className="faint"> · {t("comment.edited")} {it.editedAgo}</span>}</span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// The assignee picker. A native <select> can hold nothing but emoji — no <img>, no components — so this is a
// small custom dropdown that draws each facet with the same FacetAvatar the cards and lists use (a registered
// avatar, or a per-facet identicon plus ring colour). The options are [unassigned, one per facet]. An outside
// click or Escape closes it.
function AssigneePicker({
  current, roster, onPick,
}: {
  current: Actor | null;
  roster: Actor[];
  onPick: (kind: "" | Facet) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  const pick = (kind: "" | Facet) => { onPick(kind); setOpen(false); };
  return (
    <span className="apick" ref={ref}>
      <button
        type="button"
        className="btn apick__trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        {current ? <FacetAvatar actor={current} showName /> : <span className="faint">{t("detail.unassigned")}</span>}
        <span className="apick__caret" aria-hidden="true">▾</span>
      </button>
      {open && (
        <ul className="apick__menu" role="listbox">
          <li role="option" aria-selected={!current}>
            <button type="button" className="apick__opt" onClick={() => pick("")}>
              <span className="faint">{t("detail.unassigned")}</span>
            </button>
          </li>
          {roster.map((a) => (
            <li key={a.kind} role="option" aria-selected={current?.kind === a.kind}>
              <button type="button" className="apick__opt" onClick={() => pick(a.kind)}>
                <FacetAvatar actor={a} showName />
              </button>
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}

