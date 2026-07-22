// The single source of truth for the non-done board column cap.
//
// The columns must not grow the DOM without bound. `.column` is not a scroll container (it simply gets taller)
// and cards are variable-height, so pixel windowing has nothing to bite on: a cap-plus-affordance keeps every
// task from being mounted. Stack the first N, list the rest. boardFlip derives its animation backstop from this
// (a status change reflows at most two columns), so the value lives here — where neither consumer can import the
// other without a cycle — rather than in BoardScreen.
export const BOARD_COLUMN_CAP = 50;
