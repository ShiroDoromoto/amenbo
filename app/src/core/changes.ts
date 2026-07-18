// Incremental reads of the change feed. Core emits "which row changed, and how" on the same seam as the write
// transaction; this pulls only what lies past the cursor. Neither values nor bodies ride along (by design), so
// the caller re-reads the changed rows from the source of truth.
//
// The cursor lives in this module, and memory is enough for it: after a restart the snapshot reads the store
// whole, so take the cursor **before** that read and resume from there (a change that slips into the gap between
// the two stays ahead of the cursor — it is seen twice, never lost; the other order loses it forever).
//
// When the feed cannot say what changed, it says so. Every condition under which `drainChanges` returns `gap` is
// exactly that:
//   - the cursor expired (truncation dropped rows we had not read)
//   - no cursor, or the feed cannot be read (just after startup, or a failed IPC). This round returns gap, but
//     the position is re-established — losing it for good would retire the feed and turn every wake into a full
//     re-read.
//   - the store changed yet the feed holds no rows (a whole-file swap such as `fold`, `stage_and_swap` or a
//     `backup` restore — writes that do not pass through `WriteTx`)
//   - an unknown dataset (a new table with nowhere to fold into)
//   - too much has piled up (a full re-read is cheaper than draining the pages)
// The caller (`watchStore`) reads this and falls to `reconcile("gap")`.
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";

/** One changed row: the bare fact that `rowId` in `dataset` was `op`-ed, and nothing more. */
export interface ChangeRow {
  dataset: string;
  rowId: number;
  op: "insert" | "update" | "delete";
}

/** The changes past a cursor — what the `changes_since` command returns. */
interface Changes {
  rows: ChangeRow[];
  /** The cursor to hand back on the next call. */
  cursor: number;
  /** The page was cut short: pull the rest with the returned cursor. */
  more: boolean;
  /** The cursor expired (truncation dropped unread rows). An empty answer must not be read as "no change". */
  expired: boolean;
}

/**
 * dataset (the table the feed names) → the scope whose queries it invalidates. It follows the `WriteAck` scope
 * vocabulary (`tasks` / `decisions`), adding only the surfaces that are neither (attachments, projects). The
 * scopes land in `query.invalidateScopes`.
 *
 * A dataset that is not listed here **is not folded — it falls to gap**: dropping it silently would freeze the
 * screen on stale data for that one table alone. Add a table, add it here.
 */
const DATASET_SCOPES: Readonly<Record<string, readonly string[]>> = {
  task: ["tasks"],
  task_comment: ["tasks"], // the comment count shows on the task card
  task_commit: ["tasks"], // the recorded commit SHAs show on the task detail pane
  dependency: ["tasks"], // adding or dropping one changes the ready/blocked display
  dimension: ["tasks"],
  dimension_value: ["tasks"],
  task_dimension_value: ["tasks"], // a dimension value is a board column and a list filter
  decision: ["decisions"],
  decision_comment: ["decisions"],
  decision_edge: ["decisions"],
  decision_task_link: ["tasks", "decisions"], // shows on both (a task's decision badge, a decision's linked tasks)
  attachment: ["attachments"],
  project: ["projects"],
};

/**
 * Fold changed rows into the set of scopes to invalidate. `unknown` means a dataset arrived that `DATASET_SCOPES`
 * has no home for: it cannot be folded, so the caller falls to gap.
 */
export function foldScopes(rows: readonly ChangeRow[]): { scopes: Set<string>; unknown: boolean } {
  const scopes = new Set<string>();
  for (const row of rows) {
    const mapped = DATASET_SCOPES[row.dataset];
    if (!mapped) return { scopes: new Set(), unknown: true };
    for (const s of mapped) scopes.add(s);
  }
  return { scopes, unknown: false };
}

/** How many pages one incremental read will drain. Past that, a full re-read is cheaper than finishing the drain. */
const MAX_PAGES = 20;

/** Our position in the feed. `null` = never taken (startup, or a failed read) = we cannot say what changed. */
let cursor: number | null = null;

/**
 * Note where the feed currently ends. Call this **before** reading the store whole; from then on `drainChanges`
 * returns only what came after. Outside Tauri (the mock) there is no feed, so it sits at 0 and nothing arrives.
 */
export async function takeChangeCursor(): Promise<void> {
  if (!inTauri()) {
    cursor = 0;
    return;
  }
  try {
    const head = await invoke<number>("change_cursor");
    cursor = typeof head === "number" ? head : null; // a non-number is no position at all — same as never taken
  } catch {
    cursor = null; // no position means the next wake is a gap, which is the safe side
  }
}

/** The folded result of a drain. On `gap` the scopes are empty and the caller re-reads the source of truth. */
export interface DrainedChanges {
  scopes: Set<string>;
  gap: boolean;
}

const GAP: () => DrainedChanges = () => ({ scopes: new Set(), gap: true });

/**
 * Drain the changes past the cursor, page by page, fold them into the set of scopes they touched, and advance the
 * cursor. O(number of changes) — `gap` (and with it a full re-read by the caller) only when we cannot say what
 * changed. A round with no position (just after startup, or after an earlier IPC failure) re-establishes one
 * before returning gap; otherwise the feed would never be used again and every wake from then on would be a full
 * re-read.
 */
export async function drainChanges(): Promise<DrainedChanges> {
  if (!inTauri()) return GAP();
  if (cursor === null) {
    await takeChangeCursor();
    return GAP();
  }
  const scopes = new Set<string>();
  let at = cursor;
  for (let page = 0; page < MAX_PAGES; page++) {
    let res: Changes;
    try {
      res = await invoke<Changes>("changes_since", { cursor: at, limit: null });
    } catch {
      cursor = null; // the feed is unreadable, so our position is lost; the next round starts from gap too
      return GAP();
    }
    if (res.expired) {
      cursor = res.cursor; // after the re-read we can resume from where the feed now begins
      return GAP();
    }
    at = res.cursor;
    const folded = foldScopes(res.rows);
    if (folded.unknown) {
      cursor = at; // a table we cannot fold: leave it to the full re-read, but keep the cursor moving
      return GAP();
    }
    for (const s of folded.scopes) scopes.add(s);
    if (!res.more) {
      cursor = at;
      // Not a single row, yet the store changed: a write that never reaches the feed (a whole-file swap).
      return scopes.size === 0 ? GAP() : { scopes, gap: false };
    }
  }
  cursor = at;
  return GAP(); // too much piled up
}
