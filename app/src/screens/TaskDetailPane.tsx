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
import { activityRowKey, loadTaskActivity } from "../core/activity";
import { confirmDialog } from "../core/dialog";
import {
  DateField, DueChip, FacetAvatar, PremiseChangedField, PriorityDot, StatusSelect, TaskIdChip, When,
} from "../components/atoms";
import { errText, eventText, exactLabel, priorityLabel, t, tf } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import { useRefNav } from "../core/refNav";
import { axesFor } from "../core/appliesTo";
import { DimensionField } from "../components/DimensionField";
import type { DimensionDto } from "../bindings/bindings";
import type { Actor, ActivityItem, Facet, Placement, Priority, TaskCard } from "../mock/types";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";

// The placement to display. The Tauri DTO already carries task.placement (name included); the mock
// fallback builds one by resolving projectId against snapshot.projects.
function placementOf(task: TaskCard): Placement | null {
  if (task.placement) return task.placement;
  if (!task.projectId) return null;
  const p = getSnapshot().projects.find((pp) => pp.id === task.projectId);
  if (!p) return null;
  return { project: { id: p.id, name: p.name } };
}

const TAB_KEYS = ["detail", "activity"] as const;
type TabKey = (typeof TAB_KEYS)[number];

const COMMENT_PAGE = 20; // Bounded memory: how many comments render initially (older ones load on demand).

/**
 * The detail pane for one task. Opening it marks the task seen (a comment arriving while you look at it may as
 * well count as read). Comments are fetched per-task and only the newest `commentLimit` of them render — but a
 * row named by the pencil that falls outside the window (i.e. is old) would be a dead button, so the window is widened
 * far enough to include it. Editing a comment does not change the comment count, so the refetch effect (keyed on
 * commentCount) never fires; the writer therefore re-reads this task's activity itself. Resolving the numbers in
 * body links (`AMB-T-<n>`) and assigning dimension values both take the primary project as their context, because
 * that is where the axes live. A dimension assignment updates this pane's map optimistically, so the selects
 * answer without waiting for the round trip; the board redraws the chips on its cards off the write's own ack,
 * which is why nothing here reaches for them. Focusing the comment box for the reply arrow cannot happen at request
 * time — the textarea may not be rendered yet — so it sets a flag and lands the focus on the render that
 * produces it.
 */
export function TaskDetailPane({
  taskId, onDeleted, onDirtyChange, onSelectDecision, focusCommentAt, editCommentAt,
}: {
  taskId: number;
  onDeleted?: () => void;
  /** Report unsaved input to the parent (AppShell), which guards against discarding it on outside-click / cross. */
  onDirtyChange?: (dirty: boolean) => void;
  onSelectDecision?: (id: number) => void;
  /** Nonce that focuses the comment box when opened via the reply arrow in the activity feed. Every increment re-focuses, so you can reply to the same task again and again. undefined = an ordinary selection. */
  focusCommentAt?: number;
  /** The comment to open in edit mode when opened via the pencil in the activity feed. The nonce lets the same comment be re-opened; if it is an old comment, the render window (commentLimit) widens to reach its row. */
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
  const [dimValues, setDimValues] = useState<Record<number, number[]>>({});
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
      // One row per assignment, and a multi-select axis answers with several (`AMB-D-826`) — so the
      // map is keyed by axis and holds every value the task carries on it, not the last row read.
      const m: Record<number, number[]> = {};
      for (const r of rows) (m[r.dimensionId] ??= []).push(r.valueId);
      setDimValues(m);
    }).catch(() => {});
    return () => { alive = false; };
  }, [taskId]);

  // What the field shows for one axis — every value the task carries on it. It is both how an
  // assignment is drawn optimistically and how a refused one is taken back, so the two can never
  // disagree about what "cleared" looks like.
  const showDimValues = (dimensionId: number, values: number[]) =>
    setDimValues((m) => {
      const n = { ...m };
      if (values.length === 0) delete n[dimensionId];
      else n[dimensionId] = values;
      return n;
    });
  // Drawing one value put on: a single-select axis replaces what it had, a multi-select one gains it
  // (`AMB-D-826`) — the same split core makes when it writes.
  const withValue = (dim: DimensionDto, valueId: number) =>
    dim.cardinality === "multi" ? [...(dimValues[dim.id] ?? []), valueId] : [valueId];
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

  const notesProjectId = placementOf(task)?.project.id ?? task.projectId ?? null;
  const axisProject = notesProjectId ? getSnapshot().projects.find((p) => p.id === notesProjectId) : undefined;
  // The axes this project offers a task — an axis narrowed to decisions classifies nothing here
  // (`AMB-D-789`), so it neither draws a select nor holds anything back.
  const taskAxes = axesFor("task", axisProject?.dimensions ?? []);
  // The axes this project refuses to be left empty on, that this task still carries no value on
  // (`AMB-D-734`). Core reads the same premise when a creation is finished, over the same side; reading
  // it here too is what lets the pane hold the button and name what is missing, instead of the click
  // coming back refused.
  const unmetRequired = taskAxes.filter((d) => d.required && !dimValues[d.id]?.length);

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
    // The id alone does not name a task: decision comments ride the same timeline and number against their own
    // table, so without the type a decision's comment lands in the thread of the task that shares its number.
    : store.listActivity().filter((a) => a.kind === "comment" && a.target.type === "task" && a.target.id === taskId);
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
            {editingTitle ? (
              <input
                {...asTyped}
                className="compose__input"
                style={{ minHeight: "unset", flex: 1 }}
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
                <button className="btn" onClick={startEditTitle}>{t("detail.edit")}</button>
              </>
            )}
            <TaskIdChip id={taskId} />
          </div>

          <div className="detail__actions">
            <StatusSelect id={taskId} status={task.status} onStatus={store.setStatus} premiseChange={task.premiseChange} draft={task.draft} className="btn" />
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
            <DueChip due={task.due} />
          </div>

          <div className="detail__field">
            <span className="detail__flabel">{t("detail.assignee")}</span>
            <span>
              <AssigneePicker
                current={task.assignee ?? null}
                roster={roster}
                onPick={(kind) => {
                  if (kind === "") store.setAssignee(taskId, null);
                  else store.setAssignee(taskId, kind);
                }}
              />
            </span>
          </div>
          <div className="detail__field">
            <span className="detail__flabel">{t("detail.project")}</span>
            <span>
              {(() => {
                const placement = placementOf(task);
                if (!placement) return <span className="faint">{t("detail.none")}</span>;
                // Classification is attached and detached in the dimension selects below, so this row shows only the project name.
                return placement.project.name;
              })()}
            </span>
          </div>
          {/* The two days, as the fields that write them — the chip above says how the due date stands, and
              the premise row below says whether the start day is still holding the task back. Neither of
              those is a way of changing one, which is what these are. */}
          <div className="detail__field">
            <span className="detail__flabel">{t("date.due")}</span>
            <DateField label={t("date.due")} value={task.due} onChange={(day) => store.setDue(taskId, day)} />
          </div>
          <div className="detail__field">
            <span className="detail__flabel">{t("date.start")}</span>
            <DateField label={t("date.start")} value={task.startOn} onChange={(day) => store.setStart(taskId, day)} />
          </div>
          {taskAxes.map((dim) => (
            <div className="detail__field" key={dim.id}>
              <span className="detail__flabel">{dim.name}</span>
              <span>
                {/* The field moves first and the write answers after, so a refusal has to put it back:
                    a required axis will not be emptied (`AMB-D-734`), and left alone the pane would go
                    on showing "none" over a value the store still holds. */}
                <DimensionField
                  dim={dim}
                  selected={dimValues[dim.id] ?? []}
                  onSet={(valueId) => {
                    const prev = dimValues[dim.id] ?? [];
                    showDimValues(dim.id, withValue(dim, valueId));
                    void store.setTaskDimensionValue(task.id, valueId)
                      .then((ok) => { if (!ok) showDimValues(dim.id, prev); });
                  }}
                  onUnset={(valueId) => {
                    const prev = dimValues[dim.id] ?? [];
                    showDimValues(dim.id, prev.filter((v) => v !== valueId));
                    void store.unsetTaskDimensionValue(task.id, valueId)
                      .then((ok) => { if (!ok) showDimValues(dim.id, prev); });
                  }}
                />
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
                    className="feed__target"
                    key={b.id}
                    style={{ marginRight: 4 }}
                    onClick={() => refNav.selectTask?.(b.id)}
                  >
                    <Icon name="blocked" /> {b.name}
                  </button>
                ))}
              </span>
            </div>
          )}
          {/* The third reason a reservation is refused. It sits beside the other two rather than in the
              date fields, because what it reports is not when the work starts but why it cannot start yet. */}
          {task.notStartedUntil && (
            <div className="detail__field">
              <span className="detail__flabel">{t("detail.notStarted")}</span>
              <span title={tf("block.notStarted", { date: task.notStartedUntil })}>
                <Icon name="hourglass" /> {task.notStartedUntil}
              </span>
            </div>
          )}
          {/* The fourth premise (`AMB-D-553`), and the only one whose resolution is the creator's own next move
              rather than something to wait on — so it carries the button that ends the creation. It sits with the
              other premises because what it reports is why the task cannot start yet, and it says "finish creating"
              rather than anything borrowed from publishing: nobody is being asked to approve it (`AMB-D-558`). */}
          {task.draft && (
            <div className="detail__field">
              <span className="detail__flabel">{t("detail.draft")}</span>
              <span title={t("block.draft")}>
                <Icon name="pencil" /> {t("chip.draft")}
                <button
                  className="btn"
                  style={{ marginLeft: 6 }}
                  disabled={unmetRequired.length > 0}
                  onClick={() => store.finishCreating(taskId)}
                >
                  {t("detail.finishCreating")}
                </button>
                {/* Why the button is held, written out rather than left to a tooltip: the axes to fill in
                    are right above in this same pane, so naming them is the whole instruction. */}
                {unmetRequired.length > 0 && (
                  <span className="faint" style={{ marginLeft: 6 }}>
                    {tf("detail.finishCreatingBlocked", { names: unmetRequired.map((d) => d.name).join(", ") })}
                  </span>
                )}
              </span>
            </div>
          )}
          {/* Holder-side surface of `AMB-D-366` / `AMB-D-373`: what moved under the holder after they reserved
              this. Core fills it only for a reservation that actually acquired one, so the field appears
              exactly when there is something to say. */}
          {task.premiseChange && (
            <PremiseChangedField
              pc={task.premiseChange}
              onSelectTask={(id) => refNav.selectTask?.(id)}
              onSelectDecision={(id) => onSelectDecision?.(id)}
            />
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
                        className="feed__target"
                        style={{ padding: "0 4px" }}
                        title={unsettled ? t("detail.premiseUnsettled") : undefined}
                        onClick={() => onSelectDecision?.(d.id)}
                      >
                        {unsettled && <><Icon name="warning" />{" "}</>}{d.ref ?? ""} {d.name ?? t("dec.unknownName")}
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
                <button className="btn" style={{ marginLeft: 8 }} onClick={startEditNotes}>
                  {task.notes ? t("detail.edit") : <><Icon name="plus" /> {t("detail.add")}</>}
                </button>
              )}
            </div>
            {editingNotes ? (
              <div className="compose">
                <textarea
                  {...asTyped}
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
                  <span className="meta">{t("detail.notesHint")}</span>
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
              <div className="faint">{t("detail.noNotes")}</div>
            )}
          </div>

          <div className="detail__sep" />

          {inTauri() && <Attachments target="task" targetId={taskId} />}

          <div className="detail__sep" />

          {inTauri() && <Commits taskId={taskId} />}

          <div className="detail__sep" />

          <div>
            <div className="detail__section-h">{t("detail.activityCategory")} · <Icon name="comment" size="md" /> {task.comments}</div>
            {comments.length === 0 ? (
              <div className="faint" style={{ marginTop: 6 }}>{t("detail.noComments")}</div>
            ) : (
              <div className="comments">
                {olderCount > 0 && (
                  <button className="btn" onClick={() => setCommentLimit((n) => n + COMMENT_PAGE)}>
                    {tf("common.loadMore", { n: olderCount })}
                  </button>
                )}
                {comments.map((c) => (
                  <CommentRow
                    key={c.id}
                    id={c.id}
                    author={c.author}
                    at={c.at}
                    editedAt={c.editedAt}
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
                {...asTyped}
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
              {commentError && <ErrorNote>{commentError}</ErrorNote>}
              <div className="compose__actions">
                <span className="meta">{t("detail.commentHint")}</span>
                <button className="btn btn--primary" disabled={!comment.trim()} onClick={() => void submitComment()}>{t("detail.send")}</button>
              </div>
            </div>
          </div>

          {/* Who filed it, and when — the same sentence, finished. Nothing else on this page dates the task
              itself, so "how long has this been sitting here" had no answer at all. The stamp carries the
              year, because a task can be any distance back. `updated` is dropped when it has not moved off
              `created`, so an untouched task says one thing rather than the same stamp twice, and it is
              hovered to read what moves it: **any** write does (`AMB-D-372`), which is not the same
              question as when the status last moved. */}
          <div className="meta">
            {t("detail.created")}: {task.createdBy ? tf("facet.named", { name: task.createdBy.name, facet: t(task.createdBy.kind === "ai" ? "facet.ai" : "facet.human") }) : "—"}
            {" · "}<span title={task.createdAt}>{exactLabel(task.createdAt)}</span>
            {task.updatedAt !== task.createdAt && (
              <>
                {" · "}{t("detail.updated")}:{" "}
                <span title={t("detail.updatedHint")}>{exactLabel(task.updatedAt)}</span>
              </>
            )}
            {" · "}id {task.id} · {t("detail.deleteNoUndo")}
          </div>
          <div className="detail__danger">
            <button className="btn btn--danger" onClick={removeTask} title={t("detail.deleteTip")}><Icon name="trash" /> {t("detail.delete")}</button>
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
                // One task's rows all come from the shared counter today, but the identity of an
                // activity row is (sequence, id) wherever it is drawn (`AMB-D-388`).
                <div className="feed__item" key={activityRowKey(it)}>
                  <div className="feed__body">
                    <div className="feed__line">
                      <strong>{it.author.name}</strong>{" "}
                      {it.kind === "comment" ? tf("comment.quoted", { text: it.text ?? "" }) : it.event && eventText(it.event, it.target.title)}
                    </div>
                    <div className="feed__meta">
                      <span><When at={it.at} editedAt={it.editedAt} /></span>
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
        <span className="apick__caret"><Icon name="chevronDown" /></span>
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

