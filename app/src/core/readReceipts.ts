// The seam for last-seen state. It is per-device and never synced, and core (through Tauri commands) reads and
// writes it in the store's `read_receipt` table — not in localStorage. Unread comments in the inbox and the
// freshness of its badge both rest on it. Outside Tauri (a plain browser, the mock) it returns empty and no-ops:
// the mock has no notion of "seen".
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";
import type { ReadReceipts } from "../mock/types";

const EMPTY: ReadReceipts = { tasks: {}, mailboxLastSeen: null };

/** Read this device's last-seen state (on GUI startup, and when the inbox is drawn). */
export async function loadReadReceipts(): Promise<ReadReceipts> {
  if (!inTauri()) return EMPTY;
  return invoke<ReadReceipts>("read_receipts");
}

/** Mark a task as seen (when its detail pane is opened). Returns the whole updated state. */
export async function markTaskSeen(taskId: number): Promise<ReadReceipts> {
  if (!inTauri()) return EMPTY;
  return invoke<ReadReceipts>("mark_task_seen", { taskId });
}

/** Mark the whole inbox as seen (when the inbox view is opened). Returns the whole updated state. */
export async function markMailboxSeen(): Promise<ReadReceipts> {
  if (!inTauri()) return EMPTY;
  return invoke<ReadReceipts>("mark_mailbox_seen");
}

/**
 * Inbox source D, which does not depend on what has been seen: of the unfinished tasks assigned to me, the ones
 * carrying at least one comment addressed to me, as `{ id, unread }`. Membership is decided by the existence of
 * such a comment — a task stays in even once it is read — and `unread` is purely for display (the unread dot).
 * Core walks the comments per task, rather than through the snapshot's 100-item recency window. The inbox view
 * unions this with sources A, B and C and subtracts what has been archived. Empty outside Tauri (the mock).
 */
export async function loadCommentTasks(): Promise<{ id: number; unread: boolean }[]> {
  if (!inTauri()) return [];
  const pairs = await invoke<[number, boolean][]>("mailbox_comment_tasks");
  return pairs.map(([id, unread]) => ({ id, unread }));
}

/**
 * For the inbox: for each of the given task ids, core works out per task when the activity that put the task in
 * the inbox last happened (a new assignment, source C, or a comment addressed to me, source D) and returns that
 * instant as `triggeredAt` (RFC3339 UTC). Ids with no such activity are omitted; only ids already showing in the
 * inbox are meant to be passed. Empty outside Tauri (the mock), which has no notion of activity times either.
 */
export async function loadTriggeredAt(taskIds: number[]): Promise<Record<string, string>> {
  if (!inTauri() || taskIds.length === 0) return {};
  const pairs = await invoke<[number, string][]>("mailbox_triggered_at", { taskIds });
  return Object.fromEntries(pairs.map(([id, at]) => [String(id), at]));
}
