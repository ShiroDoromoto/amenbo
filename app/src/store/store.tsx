import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { getSnapshot, notifyDataChanged, notifyInboxChanged, subscribe } from "../core/snapshot";
import { invalidateQueries } from "../core/query";
import * as mut from "../core/mutations";
import { markTaskSeen } from "../core/readReceipts";
import { archiveInboxItem, unarchiveInboxItem } from "../core/inboxArchive";
import { errText } from "../core/i18n";
import { subscribeNotice } from "../core/notice";
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
  addTask(projectId: number | null, title: string, notes?: string): Promise<number | null>;
  toggleDone(id: number): void;
  setStatus(id: number, status: Status): void;
  setPriority(id: number, priority: Priority | null): void;
  setAssignee(id: number, kind: Facet | null): void;
  addComment(taskId: number, text: string): void;
  /** Take back a comment posted by mistake (it is deleted outright). */
  removeComment(commentId: number, taskId: number): void;
  setNotes(id: number, notes: string): void;
  setTitle(id: number, title: string): void;
  deleteTask(id: number): void;
  // CRUD over dimensions (classification axes) and their values, plus assigning a value to a task (attached and
  // detached from the task detail).
  addDimension(projectId: number, name: string): void;
  renameDimension(id: number, name: string): void;
  updateDimension(id: number, notes: string): void;
  setDimensionOrdered(id: number, ordered: boolean): void;
  setDimensionTimeAxis(id: number, timeAxis: boolean): void;
  removeDimension(id: number): void;
  addDimensionValue(dimensionId: number, name: string): void;
  renameDimensionValue(valueId: number, name: string): void;
  setDimensionValuePeriod(valueId: number, startOn: string | undefined, endOn: string | undefined): void;
  removeDimensionValue(valueId: number): void;
  moveDimensionValue(valueId: number, pos: { before?: number; after?: number }): void;
  setTaskDimensionValue(taskId: number, valueId: number): void;
  unsetTaskDimensionValue(taskId: number, valueId: number): void;
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

  const runResult = useCallback(<T,>(p: Promise<T>): Promise<T | null> => {
    return p.catch((e): null => {
      console.error("[amenbo] mutation failed:", e);
      setNotice(errText(e));
      return null;
    });
  }, []);

  const store: Store = useMemo(() => ({
    listActivity() { return activity; },

    addTask(projectId, title, notes) { return runResult(mut.addTask(projectId, title, notes)); },
    toggleDone(id) { run(mut.toggleDone(id)); },
    setStatus(id, status) { run(mut.setStatus(id, status)); },
    setPriority(id, priority) { run(mut.setPriority(id, priority)); },
    setAssignee(id, kind) { run(mut.setAssignee(id, kind)); },
    addComment(taskId, text) { run(mut.addComment(taskId, text)); },
    removeComment(commentId, taskId) { run(mut.removeComment(commentId, taskId)); },
    setNotes(id, notes) { run(mut.setNotes(id, notes)); },
    setTitle(id, title) { run(mut.setTitle(id, title)); },
    deleteTask(id) { run(mut.deleteTask(id)); },
    addDimension(projectId, name) { run(mut.addDimension(projectId, name)); },
    renameDimension(id, name) { run(mut.renameDimension(id, name)); },
    updateDimension(id, notes) { run(mut.updateDimension(id, notes)); },
    setDimensionOrdered(id, ordered) { run(mut.setDimensionOrdered(id, ordered)); },
    setDimensionTimeAxis(id, timeAxis) { run(mut.setDimensionTimeAxis(id, timeAxis)); },
    removeDimension(id) { run(mut.removeDimension(id)); },
    addDimensionValue(dimensionId, name) { run(mut.addDimensionValue(dimensionId, name)); },
    renameDimensionValue(valueId, name) { run(mut.renameDimensionValue(valueId, name)); },
    setDimensionValuePeriod(valueId, startOn, endOn) { run(mut.setDimensionValuePeriod(valueId, startOn, endOn)); },
    removeDimensionValue(valueId) { run(mut.removeDimensionValue(valueId)); },
    moveDimensionValue(valueId, pos) { run(mut.moveDimensionValue(valueId, pos)); },
    setTaskDimensionValue(taskId, valueId) { run(mut.setTaskDimensionValue(taskId, valueId)); },
    unsetTaskDimensionValue(taskId, valueId) { run(mut.unsetTaskDimensionValue(taskId, valueId)); },
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
  }), [activity, run, runResult]);

  return (
    <Ctx.Provider value={store}>
      {children}
      {notice && (
        <div className="toast toast--warn" role="alert" onClick={() => setNotice(null)}>
          ⚠ {notice}
        </div>
      )}
    </Ctx.Provider>
  );
}
