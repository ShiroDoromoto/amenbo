// Which rows of the rail's git half a reader has picked out, and the two rules that move it: a
// press with a key held down, and a walk with the arrows (`./GitPanel`).
//
// **It is the tree's rule, written once for the other half** (`./FolderTree`). A reader who has
// learnt that ⌘ takes a row in and Shift reaches from where they were is owed the same answer here,
// and the two halves are one rail — a set of rows gathered one way and another would be two things
// to learn about one panel.
//
// **The rows live in one list at a time.** git's three answers are three lists, and what is done
// with a set of rows is done to one of them: staging is what a changed row takes and unstaging what
// a staged one takes, and a set spanning both would be a press with two meanings. So picking in one
// list puts down what was picked in another, the way picking in one folder's tree does.

/** Which of the half's three lists a set of picked rows is in. */
export type Which = "conflict" | "staged" | "changed";

/**
 * The rows a reader picked out, as their whole paths, and the list they are in.
 *
 * `anchor` is the end a range is measured from — the last row picked without Shift. It is held
 * rather than read back off `keys`, because a range runs both ways: the rows in it say which they
 * are and not which end the reader started at, so a range pulled back past its own start would grow
 * the other way instead.
 */
export type Picked = {
  which: Which | null;
  keys: string[];
  anchor: string | null;
};

/** Nothing picked out, which is how the half is drawn until a reader presses a row. */
export const PICKED_NONE: Picked = { which: null, keys: [], anchor: null };

/** What the reader was holding down when they pressed, or which way a walk is being made. */
export type How =
  /** This row alone, putting down whatever else was picked. */
  | "one"
  /** The machine's own key: take this row into the set, or back out of it. */
  | "add"
  /** Shift: everything from the end the range is measured from to here. */
  | "spread";

/** Every row from one to the other, both ends included, in the order the list draws them. */
function between(rows: string[], from: string, to: string): string[] {
  const end = rows.indexOf(to);
  if (end < 0) return [];
  const at = rows.indexOf(from);
  const start = at < 0 ? end : at;
  return rows.slice(Math.min(start, end), Math.max(start, end) + 1);
}

/**
 * The set a press on one row leaves behind.
 *
 * `from` is the end a range is measured from where nothing has been picked yet — the row the
 * pointer is on for a press, and the row the keyboard was standing on for a walk. A reader pressing
 * Shift on a list nobody has touched means the run between where they were and where they pressed,
 * and the two callers know that row by different names.
 *
 * **A key held down over another list picks this row alone.** ⌘ over a row of the changes list
 * while the staged list is the one with rows in it is not a reader adding to that set — it is them
 * beginning another, and the set they had is one press of theirs away from being acted on.
 */
export function pick(
  which: Which,
  rows: string[],
  now: Picked,
  key: string,
  how: How,
  from: string = key,
): Picked {
  const here = now.which === which;
  if (how === "spread") {
    const end = (here ? now.anchor : null) ?? from;
    return { which, keys: between(rows, end, key), anchor: end };
  }
  if (how === "add" && here) {
    return {
      which,
      keys: now.keys.includes(key) ? now.keys.filter((one) => one !== key) : [...now.keys, key],
      anchor: key,
    };
  }
  return { which, keys: [key], anchor: key };
}

/** The rows picked out of this list, or none where the set is in another. */
export function keysIn(which: Which, now: Picked): string[] {
  return now.which === which ? now.keys : [];
}

/**
 * What is left of a set once the list has been read again.
 *
 * A path git no longer names is a row that has stopped being drawn — staging one moves it from one
 * list to the other — and a set holding rows nobody can see is a set the next press acts on
 * silently. What is still on the list stays picked, so staging one of five leaves the other four.
 */
export function kept(now: Picked, rows: string[]): Picked {
  if (now.which === null) return now;
  const live = rows.filter((key) => now.keys.includes(key));
  if (live.length === now.keys.length) return now;
  if (live.length === 0) return PICKED_NONE;
  return {
    which: now.which,
    keys: live,
    anchor: now.anchor !== null && live.includes(now.anchor) ? now.anchor : live[live.length - 1],
  };
}
