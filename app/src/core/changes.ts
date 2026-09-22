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
  task_dependency: ["tasks"], // adding or dropping one changes the ready/blocked display
  dimension: ["tasks"],
  dimension_value: ["tasks"],
  task_dimension_value: ["tasks"], // a dimension value is a board column and a list filter
  decision: ["decisions"],
  decision_comment: ["decisions"],
  decision_edge: ["decisions"],
  decision_dimension_value: ["decisions"], // what a decision is classified as, on its own pane
  decision_task_link: ["tasks", "decisions"], // shows on both (a task's decision badge, a decision's linked tasks)
  // The session a task or a decision was made in (`AMB-D-897`). It is drawn on the owner's detail
  // pane, so it folds to the owner's scope — and it is written once, in the same transaction as the
  // owner, so the fold costs nothing that creation was not already paying.
  task_made_in: ["tasks"],
  decision_made_in: ["decisions"],
  attachment: ["attachments"],
  project: ["projects"],
  // Amenbo's own credentials (`AMB-D-884`). The value is never drawn; what is drawn is whether one is
  // set, and for the Viewer that answer *is* the screen — a device with no server and one with a server
  // are two different panes, and the three fields setup writes are what tells them apart. So it folds to
  // the Viewer's scope: the CLI's `viewer setup` is the ordinary way this arrives while the pane is open.
  // The notification shelf reads a credential the same way and names no scope yet; it is drawn from its
  // own writes, and a target saved from the CLI is what it does not yet hear.
  secret: ["viewer"],
  // The notification tables (`AMB-D-885`): the device's shelf of targets, and the three a project's own
  // row is written on. Folded to nothing for `secret`'s reason rather than a different one — nothing on
  // screen draws them yet, so no query goes stale when one moves, and falling to gap would buy a full
  // re-read for a change nobody can see. The two screens that will draw them — the shelf under the
  // device's settings, the notification pane under a project's — name their scope here when they arrive.
  notify_target: [],
  project_notify: [],
  project_notify_target: [],
  project_notify_event: [],
  // This device's own tables (`AMB-D-856`). None of them travels anywhere, and all three are written by
  // the CLI and drawn here: the folders a project is bound to, and the two answers given for it. They
  // fold to the project they are about, which is the surface each of them is on.
  binding_project_dir: ["projects"],
  hook_optout: ["projects"],
  harness_consent: ["projects"],
  // The ten tables an automation's definition is built in. Two of them are drawn — the library and
  // the steps pointing into it, which the "actions" tab reads as one list — and name their scope
  // below. The rest are folded to nothing for `notify_target`'s reason: no pane draws them yet, so no
  // query goes stale when one is built, and falling to gap would buy a full re-read for a change
  // nobody can see. The tabs that will draw them name their scope here when they arrive. Of the five
  // tables a run is written in, all five are listed for the same reason: the run screens that will
  // draw them — the automation pane and the "running" tab — name their scope here when they arrive.
  automation: [],
  // The library and the pointers into it: the "actions" tab draws every action with how many
  // automations run it, so a step taking up an action or letting one go moves that list as surely as
  // the action's own row does.
  automation_action: ["automationActions"],
  automation_note: [],
  automation_step: ["automationActions"],
  automation_step_note: [],
  automation_cfg: [],
  automation_exit: [],
  automation_port: [],
  automation_edge: [],
  automation_wire: [],
  // A run taking a lane, handing one back or ending moves the number the workspace band draws, so
  // this one names a scope of its own. The snapshot carries the other half — how many lanes there
  // are — and that is a setting, written from one screen rather than arriving on the feed.
  automation_run: ["automationLanes"],
  // The steps a run copied at launch. Written once, in the same transaction as the run, and read only
  // by what reads that run — so it is folded to nothing rather than made to re-read the band.
  automation_run_def: [],
  automation_run_task: [],
  automation_run_step: [],
  automation_run_value: [],
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
      // An empty set of scopes is an answer, not a failure: the feed spoke, and it named nothing — a
      // commit of the store's own bookkeeping, which no dataset covers (the drive reclaiming the outbox
      // it walked, a `store_meta` scalar). The two writes that reach no row at all — a whole-file swap,
      // and `config.json` — are told apart by the caller off the signature's own legs (`AMB-D-856`), so
      // they are no longer guessed at from here.
      return { scopes, gap: false };
    }
  }
  cursor = at;
  return GAP(); // too much piled up
}
