// Which standing notice the board carries above its tabs — and there is at most one of them
// (`AMB-D-535`). Two notices side by side are a screen that does not say which to do first, and the
// count only goes up from here.
//
// **The choice is one judgement, in one place.** Each notice used to carry its own draw condition, and
// the conditions had to agree with one another: the wiring row drew where the first loop did not, spelled
// as the negation of the first loop's own test. That holds for two and stops holding for three, where
// every notice added has to be written into every sibling's condition. Here a notice is a place in one
// ordered list instead.
//
// **The order is premise dependency, not importance.** An earlier entry is what a later one needs before
// it can mean anything, so the one whose premise is missing stands first and the rest wait their turn.
// Importance would have to be re-argued across the whole list every time one is added; a premise is
// derived from what the notice is for.

/**
 * The standing notices, in premise order.
 *
 * `firstLoop` comes before `agentHookWiring`: the wiring takes effect from the next session on, and a
 * reader who has never seen amenbo hold a task cannot tell what the setup is for (`AMB-D-516`). The loop
 * is spent by the first task to land — and that same task is what brings the wiring notice up.
 */
export const BOARD_NOTICES = ["firstLoop", "agentHookWiring"] as const;

export type BoardNotice = (typeof BOARD_NOTICES)[number];

/** Whether each notice has something to say right now — its own question, asked of its own state. */
export type BoardNoticeStanding = Record<BoardNotice, boolean>;

/**
 * The one notice the board draws, or `null` where none is standing.
 *
 * What is left out is not lost: project settings lists the wiring still waiting, whether or not the board
 * is the one showing it. The board is where the reader acts, the settings screen where they look over
 * everything (`AMB-D-535`).
 */
export function pickBoardNotice(standing: BoardNoticeStanding): BoardNotice | null {
  return BOARD_NOTICES.find((notice) => standing[notice]) ?? null;
}
