// The status axis, as the GUI reads it. Core owns the values themselves (`TaskStatus` in model.rs);
// what lives here is the display order the surfaces share and the one reading of them a surface may
// need — whether a task has *ended*. Written once, so a value added to the axis cannot reach the
// board's columns and miss the pull-down, the row styling or the filters.
import type { Status } from "../mock/types";

/**
 * Every status, in the order the surfaces show them: `blocked` sits with the open ones and the two
 * terminals come last, `rejected` after `done`. This is the pull-down's option list — every value is
 * reachable from every surface that mounts it.
 */
export const STATUS_ALL: Status[] = ["todo", "in_progress", "blocked", "done", "rejected"];

/**
 * The board's columns. **`rejected` is deliberately not one of them** (`AMB-D-397`): a terminal that
 * is reached by exception does not deserve a standing column of its own, and a fifth one would take
 * width from the four the work actually moves through. Rejected cards fold into the done column,
 * which is the *closed* column ({@link isClosed}).
 */
export const STATUS_COLUMNS: Status[] = ["todo", "in_progress", "blocked", "done"];

/**
 * Is the task over, whichever way it ended? The mirror of core's `TaskStatus::is_closed` — the two
 * terminals are `done` (the work was carried out) and `rejected` (it was decided against), and the
 * difference is what they mean, not whether they are finished. Surfaces that fade or strike out an
 * ended task ask this; the pull-down beside it is what says *which* terminal it reached.
 */
export function isClosed(s: Status): boolean {
  return s === "done" || s === "rejected";
}
