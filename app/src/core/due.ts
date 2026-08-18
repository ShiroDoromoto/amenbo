// The sidebar's due row: which tasks it warns about, and on which step.
//
// One definition serves both the badge and the list the row opens, because the two must not be able to
// disagree — a warning whose colour points where the tasks are not is the mistake `AMB-D-327` took out
// of the inbox, and it would come straight back if the badge counted one set and the row opened another.
//
// Two steps, in the vocabulary the palette already speaks (`styles/tokens.css`):
//   - **stop** — its day has gone, or its day is today. Nothing moves until a hand is put to it.
//   - **heed** — its day is tomorrow. It moves on its own; know it while it comes.
//
// The same two days take the same two steps on a task's own due chip (`--c-due-*`), so a row read in a
// list and the warning read in the sidebar never disagree about how late a day is.
//
// `done:false` is closed-or-not (`AMB-D-397`), so work that was finished and work that was decided
// against both drop out. "Tomorrow" needs no filter arm of its own: the day keys parse the relative
// forms, so `due:tomorrow` is the day after the store's own today (`amenbo-core/src/time.rs`).

/** Its day has gone. */
export const DUE_OVERDUE = "done:false due:overdue";
/** Its day is today. */
export const DUE_TODAY = "done:false due:today";
/** Its day is tomorrow. */
export const DUE_TOMORROW = "done:false due:tomorrow";

/**
 * The three windows the row is built from, in the order the list shows them — the soonest deadline
 * first. They are disjoint (one `due_on`, three different days), so nothing is counted twice.
 */
export const DUE_WINDOWS = [DUE_OVERDUE, DUE_TODAY, DUE_TOMORROW];

/** How much stands on each step: what wants a hand now, and what is coming tomorrow. */
export interface DueCounts {
  stop: number;
  heed: number;
}

/** The step a badge is drawn on. */
export type DueStep = "stop" | "heed";

/**
 * The badges the due row draws, most urgent first. Two steps get two badges rather than one merged
 * badge: merging would have to pick a single colour and drop the other count, and the two say
 * different things to the reader. A step with nothing on it draws nothing.
 */
export function dueBadges(counts: DueCounts): { step: DueStep; count: number }[] {
  const steps: { step: DueStep; count: number }[] = [
    { step: "stop", count: counts.stop },
    { step: "heed", count: counts.heed },
  ];
  return steps.filter((b) => b.count > 0);
}
