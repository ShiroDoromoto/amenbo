// The seam a screen draws its own panel into the shell's right-pane column through (`AMB-T-5418`).
//
// A build screen's panel belongs where the board's detail stands: in the grid's third column beside
// `.main`, not nested inside it. Nested, it scrolls inside a column that scrolls too — two scrollbars,
// and a height that has to be guessed at from the window. Here the column is the shell's, so its height
// is the row's and the only scroll is the pane's own, the way the board's is.
//
// The screen does not know the shell, so the shell lends it two things: the element to portal into and
// a way to say that it wants the column. A claim is held for as long as the panel is mounted, and is
// counted rather than flagged — one panel giving way to another unmounts the first in the same commit
// the second mounts, and a flag would let that order decide whether the column stays.
//
// Outside the provider (tests, previews) there is no column, and the panel is drawn where it stands.
import { createContext, useContext, useEffect, type ReactNode } from "react";

export interface PaneSlot {
  /** The element in the right-pane column the panel is portalled into, once it is there. */
  slot: HTMLElement | null;
  /** Ask for the column; the returned function gives the claim back. */
  claim: () => () => void;
}

const PaneSlotContext = createContext<PaneSlot | null>(null);

export function PaneSlotProvider({ value, children }: { value: PaneSlot; children: ReactNode }) {
  return <PaneSlotContext.Provider value={value}>{children}</PaneSlotContext.Provider>;
}

/** The right-pane column, claimed for as long as the caller is mounted — `null` outside the shell. */
export function usePaneSlot(): { slot: HTMLElement | null } | null {
  const pane = useContext(PaneSlotContext);
  const claim = pane?.claim;
  useEffect(() => claim?.(), [claim]);
  return pane && { slot: pane.slot };
}
