import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { getSnapshot, notifyDataChanged, notifyInboxChanged, subscribe } from "../core/snapshot";
import { invalidateQueries } from "../core/query";
import * as mut from "../core/mutations";
import { markTaskSeen } from "../core/readReceipts";
import { archiveInboxItem, unarchiveInboxItem } from "../core/inboxArchive";
import { errText } from "../core/i18n";
import { subscribeNotice } from "../core/notice";
import { Icon } from "../components/Icon";
import type { ActivityItem, Facet, Priority, Status } from "../mock/types";

/**
 * Store: it holds the writes (mutators) and the one cheap read that rides the snapshot (activity), nothing more. It
 * does not read task lists — each view pulls its own window through the task_page hooks in core/reads, which is what
 * keeps memory bounded. Mutators delegate to core/mutations: under Tauri they invoke a per-action command and apply
 * the snapshot it returns; in a plain browser they fall back to mutating the mock cache. Failures (a write conflict,
 * say) surface as a transient toast.
 */
interface Store {
  listActivity(): ActivityItem[];
  // Returns the id of the task created, so the caller can open its detail right away. null on failure (a toast says so).
  addTask(
    projectId: number | null,
    title: string,
    notes?: string,
    due?: string | null,
    start?: string | null,
  ): Promise<number | null>;
  /**
   * Move a task's status. `rejected` is the one value that carries a reason, and it is required — the
   * pull-down collects it and this routes it to the write that keeps it (`AMB-D-397`).
   */
  setStatus(id: number, status: Status, reason?: string): void;
  /**
   * End the second stage of a creation (`AMB-D-554`): the task stops being held out of the mailbox and
   * out of a reservation. It runs one way — nothing puts a task back to being created.
   */
  finishCreating(id: number): void;
  setPriority(id: number, priority: Priority | null): void;
  /** Set the due date (`YYYY-MM-DD`), or null to take it away. */
  setDue(id: number, due: string | null): void;
  /** Set the start day (`YYYY-MM-DD`), or null to take it away. A day still ahead holds the task unready. */
  setStart(id: number, start: string | null): void;
  setAssignee(id: number, kind: Facet | null): void;
  addComment(taskId: number, text: string): void;
  /** Take back a comment posted by mistake (it is deleted outright). */
  removeComment(commentId: number, taskId: number): void;
  /** The same, for a comment on a decision record — the activity feed retracts either kind from the row. */
  removeDecisionComment(commentId: number, decisionId: number): void;
  setNotes(id: number, notes: string): void;
  setTitle(id: number, title: string): void;
  deleteTask(id: number): void;
  // CRUD over dimensions (classification axes) and their values, plus assigning a value to a task (attached and
  // detached from the task detail).
  addDimension(projectId: number, name: string): void;
  renameDimension(id: number, name: string): void;
  /**
   * Rename the axis's readable key (`AMB-D-735`) — what it answers to outside Amenbo. Replaced, never
   * cleared. It answers whether the write landed, because a key is refused for reasons the panel
   * cannot see coming — a shape that cannot be carried outside, a key another axis already answers to
   * — and the field has to be put back rather than left showing a key nothing was saved under. The
   * refusal itself has already gone to a toast.
   */
  setDimensionSlug(id: number, slug: string): Promise<boolean>;
  updateDimension(id: number, notes: string): void;
  /**
   * Let one record answer this axis with several of its values (`AMB-D-826`). Raising it takes
   * nothing away; lowering it is refused while a record still holds several, and the sentence says
   * how many.
   */
  setDimensionMulti(id: number, multi: boolean): void;
  setDimensionOrdered(id: number, ordered: boolean): void;
  setDimensionTimeAxis(id: number, timeAxis: boolean): void;
  /**
   * Let this axis's values be closed rather than deleted (`AMB-D-829`). An axis holds one role, so
   * naming it closable takes the time axis off it, and giving the role up leaves whatever was closed
   * closed — reopening is free on any axis.
   */
  setDimensionClosable(id: number, closable: boolean): void;
  setDimensionShowOnCard(id: number, showOnCard: boolean): void;
  /**
   * Make this axis refuse to be left empty (`AMB-D-734`). It is read where a creation is finished and
   * nowhere else, so raising it never moves a task that is already through that door.
   */
  setDimensionRequired(id: number, required: boolean): void;
  /**
   * Narrow or widen which of the two entities this axis classifies (`AMB-D-789`). It decides where the
   * axis is offered, not what has been answered on it, so narrowing takes no assignment away.
   */
  setDimensionAppliesTo(id: number, appliesTo: "task" | "decision" | "both"): void;
  removeDimension(id: number): void;
  addDimensionValue(dimensionId: number, name: string): void;
  renameDimensionValue(valueId: number, name: string): void;
  /** The value's counterpart of `setDimensionSlug`, unique within its axis, and refusable the same way. */
  setDimensionValueSlug(valueId: number, slug: string): Promise<boolean>;
  setDimensionValuePeriod(valueId: number, startOn: string | undefined, endOn: string | undefined): void;
  /**
   * Close a value, or open it again (`AMB-D-829`). Closing keeps every record already on it and every
   * filter naming it, and stops the value taking new records; deleting is the other act and takes the
   * classification with it.
   */
  setDimensionValueClosed(valueId: number, closed: boolean): void;
  /**
   * Delete a value of an axis. `reassignTo` names another value of the same axis for the tasks
   * answering with this one to move to — which a required axis demands rather than emptying them out.
   * Left out, their classification goes with the value, as it does on an ordinary axis.
   */
  removeDimensionValue(valueId: number, reassignTo?: number): void;
  moveDimensionValue(valueId: number, pos: { before?: number; after?: number }): void;
  /**
   * Put a task on an axis, or take it off. Both answer whether the write landed, because both can be
   * refused — a required axis will not be emptied (`AMB-D-734`) — and every caller moves the screen
   * before the answer comes. A `false` is that caller's cue to put the screen back; the refusal itself
   * has already gone to a toast.
   */
  setTaskDimensionValue(taskId: number, valueId: number): Promise<boolean>;
  unsetTaskDimensionValue(taskId: number, valueId: number): Promise<boolean>;
  // Reordering projects (sidebar drag & drop). Failures — the reorder command rejecting, say — go through run() like
  // every other mutator so they reach a toast; called directly they would fail in silence.
  moveProject(projectId: number, position: "top" | "bottom" | "before" | "after", anchorId?: number): void;
  markSeen(taskId: number): void;
  archiveInbox(taskId: number): void;
  unarchiveInbox(taskId: number): void;
}

const Ctx = createContext<Store | null>(null);
export function useStore(): Store {
  const s = useContext(Ctx);
  if (!s) throw new Error("StoreProvider missing");
  return s;
}

/**
 * The provider that hands out the write mutators and the cheap activity list. The context value is kept a stable
 * reference with `useMemo`, depending only on `activity` (plus run/runResult, themselves stable): rebuilt every render
 * it would re-render every consumer — Board/List/Detail and every row under them — and defeat the rows' memoisation.
 * Marking a task seen (`markSeen`) does not change who is in the inbox, so it stops at `notifyDataChanged`; archiving
 * and restoring (`archiveInbox`/`unarchiveInbox`) do change that set — and therefore the badge count — so they bump the
 * inbox generation with `notifyInboxChanged`. Neither change rides the snapshot (both are machine-local), so the
 * `smartView` queries are invalidated explicitly (a no-op under the mock).
 */
export function StoreProvider({ children }: { children: ReactNode }) {
  const [activity, setActivity] = useState<ActivityItem[]>(() => getSnapshot().activity.map((a) => ({ ...a })));
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    return subscribe(() => {
      setActivity(getSnapshot().activity.map((a) => ({ ...a })));
    });
  }, []);

  useEffect(() => {
    if (!notice) return;
    const id = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(id);
  }, [notice]);

  useEffect(() => subscribeNotice(setNotice), []);

  // Surface a mutator failure (a rejected write, say) as a toast. setNotice is the stable reference useState returns,
  // so run is stable too — it never depends on activity.
  const run = useCallback((p: Promise<void>): void => {
    p.catch((e) => {
      console.error("[amenbo] mutation failed:", e);
      setNotice(errText(e));
    });
  }, []);

  // The same toast, plus the yes/no an optimistic caller needs: a screen that moved before the write
  // landed has to know whether to keep the move or take it back. `run` cannot say — it returns nothing —
  // and `runResult`'s `null` is only distinguishable from success where the write has a value to return.
  const runOk = useCallback((p: Promise<void>): Promise<boolean> => {
    return p.then(() => true).catch((e) => {
      console.error("[amenbo] mutation failed:", e);
      setNotice(errText(e));
      return false;
    });
  }, []);

  const runResult = useCallback(<T,>(p: Promise<T>): Promise<T | null> => {
    return p.catch((e): null => {
      console.error("[amenbo] mutation failed:", e);
      setNotice(errText(e));
      return null;
    });
  }, []);

  const store: Store = useMemo(() => ({
    listActivity() { return activity; },

    addTask(projectId, title, notes, due, start) { return runResult(mut.addTask(projectId, title, notes, due, start)); },
    setStatus(id, status, reason) {
      // The one fork on the way down: a rejection goes through the write that also keeps the reason, so
      // no surface can reach `rejected` and leave the reasoning behind.
      run(status === "rejected" ? mut.rejectTask(id, reason ?? "") : mut.setStatus(id, status));
    },
    finishCreating(id) { run(mut.finishTaskCreation(id)); },
    setPriority(id, priority) { run(mut.setPriority(id, priority)); },
    setDue(id, due) { run(mut.setDue(id, due)); },
    setStart(id, start) { run(mut.setStart(id, start)); },
    setAssignee(id, kind) { run(mut.setAssignee(id, kind)); },
    addComment(taskId, text) { run(mut.addComment(taskId, text)); },
    removeComment(commentId, taskId) { run(mut.removeComment(commentId, taskId)); },
    removeDecisionComment(commentId, decisionId) { run(mut.removeDecisionComment(commentId, decisionId)); },
    setNotes(id, notes) { run(mut.setNotes(id, notes)); },
    setTitle(id, title) { run(mut.setTitle(id, title)); },
    deleteTask(id) { run(mut.deleteTask(id)); },
    addDimension(projectId, name) { run(mut.addDimension(projectId, name)); },
    renameDimension(id, name) { run(mut.renameDimension(id, name)); },
    setDimensionSlug(id, slug) { return runOk(mut.setDimensionSlug(id, slug)); },
    updateDimension(id, notes) { run(mut.updateDimension(id, notes)); },
    setDimensionMulti(id, multi) { run(mut.setDimensionMulti(id, multi)); },
    setDimensionOrdered(id, ordered) { run(mut.setDimensionOrdered(id, ordered)); },
    setDimensionTimeAxis(id, timeAxis) { run(mut.setDimensionTimeAxis(id, timeAxis)); },
    setDimensionClosable(id, closable) { run(mut.setDimensionClosable(id, closable)); },
    setDimensionShowOnCard(id, showOnCard) { run(mut.setDimensionShowOnCard(id, showOnCard)); },
    setDimensionRequired(id, required) { run(mut.setDimensionRequired(id, required)); },
    setDimensionAppliesTo(id, appliesTo) { run(mut.setDimensionAppliesTo(id, appliesTo)); },
    removeDimension(id) { run(mut.removeDimension(id)); },
    addDimensionValue(dimensionId, name) { run(mut.addDimensionValue(dimensionId, name)); },
    renameDimensionValue(valueId, name) { run(mut.renameDimensionValue(valueId, name)); },
    setDimensionValueSlug(valueId, slug) { return runOk(mut.setDimensionValueSlug(valueId, slug)); },
    setDimensionValuePeriod(valueId, startOn, endOn) { run(mut.setDimensionValuePeriod(valueId, startOn, endOn)); },
    setDimensionValueClosed(valueId, closed) { run(mut.setDimensionValueClosed(valueId, closed)); },
    removeDimensionValue(valueId, reassignTo) { run(mut.removeDimensionValue(valueId, reassignTo ?? null)); },
    moveDimensionValue(valueId, pos) { run(mut.moveDimensionValue(valueId, pos)); },
    setTaskDimensionValue(taskId, valueId) { return runOk(mut.setTaskDimensionValue(taskId, valueId)); },
    unsetTaskDimensionValue(taskId, valueId) { return runOk(mut.unsetTaskDimensionValue(taskId, valueId)); },
    moveProject(projectId, position, anchorId) { run(mut.moveProject(projectId, position, anchorId)); },
    markSeen(taskId) {
      markTaskSeen(taskId).then(() => { notifyDataChanged(); invalidateQueries((k) => k[0] === "smartView"); }).catch(() => {});
    },
    archiveInbox(taskId) {
      archiveInboxItem(taskId).then(() => { notifyInboxChanged(); invalidateQueries((k) => k[0] === "smartView"); }).catch(() => {});
    },
    unarchiveInbox(taskId) {
      unarchiveInboxItem(taskId).then(() => { notifyInboxChanged(); invalidateQueries((k) => k[0] === "smartView"); }).catch(() => {});
    },
  }), [activity, run, runOk, runResult]);

  return (
    <Ctx.Provider value={store}>
      {children}
      {notice && (
        <div className="toast toast--warn" role="alert" onClick={() => setNotice(null)}>
          <Icon name="warning" /> {notice}
        </div>
      )}
    </Ctx.Provider>
  );
}
