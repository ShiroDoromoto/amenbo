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
 * empty list under a 💬 count that is perfectly correct. Outside Tauri (the mock) it returns an empty array.
 */
export async function loadTaskActivity(taskId: number, limit?: number): Promise<ActivityItem[]> {
  if (!inTauri()) return [];
  return invoke<ActivityItem[]>("task_activity", { taskId, limit });
}
