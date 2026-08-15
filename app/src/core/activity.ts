// Paging back through the activity history. The snapshot carries only the newest 100 rows, so anything older is
// reached by calling core's `activity_page` for one window at a time.
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";
import type { ActivityItem } from "../mock/types";

/** Newest first, skip `offset` rows and return `limit` of them. Core's DTO has the same shape as ActivityItem. */
export async function loadActivityPage(offset: number, limit: number): Promise<ActivityItem[]> {
  if (!inTauri()) return [];
  return invoke<ActivityItem[]>("activity_page", { offset, limit });
}

/**
 * One task's activity (comments included), newest first. The detail pane reads this rather than the snapshot's window
 * of the newest 100 rows: as tasks pile up that window drops the comments on older tasks, and the pane would show an
 * empty list under a comment count that is perfectly correct. Outside Tauri (the mock) it returns an empty array.
 */
export async function loadTaskActivity(taskId: number, limit?: number): Promise<ActivityItem[]> {
  if (!inTauri()) return [];
  return invoke<ActivityItem[]>("task_activity", { taskId, limit });
}

/**
 * What names one row: its id **and** the sequence that id was drawn from (`AMB-D-388`).
 *
 * The timeline merges sources that number independently — the ledger and task comments share one
 * counter, a decision comment is numbered against its own table — so an id on its own names two
 * different rows. Anything that treats a row as identified (de-duplicating a page boundary, keying a
 * list) has to pair the two, or one of the collided rows quietly stands in for the other.
 */
export function activityRowKey(it: ActivityItem): string {
  return `${it.seq}:${it.id}`;
}

/** Drop repeats, so a row arriving on both sides of the seed/older-page boundary appears once. Order is preserved. */
export function dedupActivityRows(list: ActivityItem[]): ActivityItem[] {
  const seen = new Set<string>();
  const out: ActivityItem[] = [];
  for (const it of list) {
    const key = activityRowKey(it);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(it);
  }
  return out;
}
