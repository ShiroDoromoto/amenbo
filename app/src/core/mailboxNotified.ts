// The seam for the mailbox's notified set: the inbox items this device has already raised an OS
// notification for. Per-device, never-synced state, read and written through core (Tauri commands)
// into the unified store's `mailbox_notified` table — the same shape as the inbox archive
// (inboxArchive.ts). Ids are task primary keys (number).
//
// It is what makes an arrival announce exactly once: the mailbox loads it as its baseline at startup
// (so what arrived while the app was closed is caught up once, then never re-announced) and appends to
// it each time it announces. Outside Tauri (plain browser = mock) these return empty / are no-ops.
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";

/** The task ids this device has already notified (ascending) — the mailbox's baseline at startup. */
export async function loadNotified(): Promise<number[]> {
  if (!inTauri()) return [];
  return invoke<number[]>("mailbox_notified_ids");
}

/** Record that these ids have now been notified (idempotent, batched). Returns the full list after. */
export async function addNotified(taskIds: number[]): Promise<number[]> {
  if (!inTauri() || taskIds.length === 0) return [];
  return invoke<number[]>("mailbox_notified_add", { taskIds });
}
