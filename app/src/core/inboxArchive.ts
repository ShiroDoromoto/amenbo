// The seam for inbox items archived (dismissed) on this device. Per-device, never-synced state,
// read and written through core (Tauri commands) into the unified store's inbox_archive table —
// the same shape as read receipts (readReceipts.ts). Ids are task primary keys (number). This is
// not Project.archived (archiving a project in the store): it is only a display filter for the inbox.
// Outside Tauri (plain browser = mock) these return empty / are no-ops: the mock has no archive.
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";

/** The inbox items archived on this device (task ids, ascending) — read when rendering the inbox. */
export async function loadInboxArchived(): Promise<number[]> {
  if (!inTauri()) return [];
  return invoke<number[]>("inbox_archived");
}

/** Archive an inbox item (drop it from the list). Returns the full id list after the change. */
export async function archiveInboxItem(taskId: number): Promise<number[]> {
  if (!inTauri()) return [];
  return invoke<number[]>("inbox_archive", { taskId });
}

/** Unarchive an inbox item (put it back in the inbox). Returns the full id list after the change. */
export async function unarchiveInboxItem(taskId: number): Promise<number[]> {
  if (!inTauri()) return [];
  return invoke<number[]>("inbox_unarchive", { taskId });
}
