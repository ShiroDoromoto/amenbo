import { useCallback, useState } from "react";
import type { Nav } from "./AppShell";

/** What the right pane has selected (none, a task, or a decision). It is part of one unit of history. */
export type Selection =
  | { type: "none" }
  | { type: "task"; id: number }
  | { type: "decision"; id: number };

/**
 * One unit of history: where you are, which is the Nav (project and view) together with the right pane's
 * selection. ＜/＞ walk over Locations, so task → decision → ＜ lands back on the original task.
 */
export type Location = { nav: Nav; sel: Selection };

export const NO_SELECTION: Selection = { type: "none" };

export type NavHistory = {
  loc: Location;
  /** Move to a new Location and push it (a Location identical to the current one is not pushed). */
  go: (loc: Location) => void;
  back: () => void;
  forward: () => void;
  canBack: boolean;
  canForward: boolean;
};

export type NavState = { stack: Location[]; index: number };
export type NavAction = { type: "push"; loc: Location } | { type: "back" } | { type: "forward" };

// The project a screen arrives holding is part of where you are, so two ways into the same screen that
// name different projects are two places — otherwise walking in from a creation and then from the
// sidebar would leave the first one's project still on the screen.
const sameNav = (a: Nav, b: Nav) => a.type === b.type && a.id === b.id && a.pick === b.pick;
const sameSel = (a: Selection, b: Selection) =>
  a.type === b.type && (a.type === "none" || a.id === (b as { id: string | number }).id);
const sameLocation = (a: Location | undefined, b: Location) =>
  !!a && sameNav(a.nav, b.nav) && sameSel(a.sel, b.sel);

/**
 * The pure transition over the Location history stack, testable without a DOM. It behaves like browser history:
 * a push truncates everything ahead of the current position before adding, and back/forward move the index.
 * Pushing the Location we are already on (same nav, same right-pane selection) is not pushed, so ＜/＞ do not
 * fill up with nothing. At either end it is a no-op, returning the same state.
 */
export function navReduce(s: NavState, a: NavAction): NavState {
  switch (a.type) {
    case "push": {
      if (sameLocation(s.stack[s.index], a.loc)) return s;
      const stack = s.stack.slice(0, s.index + 1);
      stack.push(a.loc);
      return { stack, index: stack.length - 1 };
    }
    case "back":
      return s.index > 0 ? { ...s, index: s.index - 1 } : s;
    case "forward":
      return s.index < s.stack.length - 1 ? { ...s, index: s.index + 1 } : s;
  }
}

/**
 * The history stack of Locations (a Nav plus the right pane's selection), which the header's ＜/＞ move over.
 * Selecting in the right pane (a task or decision detail) belongs to the same trail as navigating. The transition
 * logic is factored out into `navReduce`, where it is tested.
 */
export function useNavHistory(initialNav: Nav): NavHistory {
  const [{ stack, index }, setState] = useState<NavState>({
    stack: [{ nav: initialNav, sel: NO_SELECTION }],
    index: 0,
  });
  const go = useCallback((loc: Location) => setState((s) => navReduce(s, { type: "push", loc })), []);
  const back = useCallback(() => setState((s) => navReduce(s, { type: "back" })), []);
  const forward = useCallback(() => setState((s) => navReduce(s, { type: "forward" })), []);
  return {
    loc: stack[index],
    go,
    back,
    forward,
    canBack: index > 0,
    canForward: index < stack.length - 1,
  };
}
