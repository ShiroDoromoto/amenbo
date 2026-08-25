// The two columns beside the panes of the terminal face — the rail on one side, the file face on the
// other — and the three things a person may do to either: close it, open it again, and drag the edge
// between it and the panes.
//
// **A column that cannot be closed goes on taking width from the middle.** The panes are what the
// face is for, and on a laptop the two columns together are most of a pane's worth of room. So each
// one has a way to close it and a way to open it again, and neither is ever left without the other:
// a column folded away with no way back is worse than one that never folded.
//
// **The width is the person's, and it is kept on this device.** It is a UI setting rather than
// anything about the work, so it lives in `localStorage` beside the board's own columns and follows
// the same shape they do — a floor, a ceiling read off the window, and a clamp applied on the way in
// and on the way out, so a value written by an older build or a wider screen cannot come back as a
// column with no room left beside it (`AMB-D-312`, `../core/sidebarWidth`).
//
// **What is kept is the wish, not what is on the screen.** A column can be a column or a drawer, and
// which of those it is depends on how much room there is; whether the person wants it at all does
// not. So the flag says whether they asked for it, and `sidesAreDrawers` says how it is drawn.
//
// **Which half of the file face is up is kept the same way.** It is a thing about the person rather
// than about the work — the same reading that put the agent a pane starts with on this device
// (`AMB-T-3686`) — so the answer they left is the one they come back to, and what the face opens on
// is only ever the first run's answer.

/**
 * The least width a terminal is worth drawing in.
 *
 * It is what the middle is measured against: the columns beside the panes are drawers exactly when
 * keeping them as columns would take the page below this. Rounded rather than derived — what a
 * terminal needs is however many columns of text the program in it expects, and no number this side
 * can compute answers that.
 */
export const PANE_MIN = 320;

const RAIL_WIDTH = "amenbo.termface.railWidth";
const SIDE_WIDTH = "amenbo.termface.sideWidth";
const RAIL_SHOWN = "amenbo.termface.railShown";
const SIDE_SHOWN = "amenbo.termface.sideShown";
const SIDE_TAB = "amenbo.termface.sideTab";

/** The rail's floor and where it starts — the fixed width it shipped with, so nothing moves until
 *  somebody drags it. */
export const RAIL_MIN = 120;
export const RAIL_DEFAULT = 160;

/** The file face's floor and where it starts, likewise the width it shipped with (16rem). */
export const SIDE_MIN = 200;
export const SIDE_DEFAULT = 256;

/** The most a column may take: a fraction of the window, and never less than its own floor — a
 *  window too narrow for the floor is a window where the sides are drawers anyway. */
function ceiling(share: number, floor: number): number {
  const cap = typeof window === "undefined" ? 520 : Math.round(window.innerWidth * share);
  return Math.max(cap, floor);
}

export function railMax(): number {
  return ceiling(0.3, RAIL_MIN);
}

export function sideMax(): number {
  return ceiling(0.4, SIDE_MIN);
}

export function clampRailWidth(px: number): number {
  return Math.min(Math.max(px, RAIL_MIN), railMax());
}

export function clampSideWidth(px: number): number {
  return Math.min(Math.max(px, SIDE_MIN), sideMax());
}

/** A width this device has kept, clamped, or where it starts when nothing has been kept. */
function keptWidth(key: string, fallback: number, clamp: (px: number) => number): number {
  const raw = typeof localStorage === "undefined" ? null : localStorage.getItem(key);
  const px = raw === null ? NaN : Number(raw);
  return Number.isFinite(px) ? clamp(px) : fallback;
}

/** Clamp, keep, and answer with the width actually taken — which is a width even where nothing can
 *  be kept, so a drag still moves the column in a browser that refuses storage. */
function keepWidth(key: string, px: number, clamp: (px: number) => number): number {
  const taken = clamp(px);
  try {
    localStorage.setItem(key, String(taken));
  } catch { /* take the width even where localStorage is unavailable */ }
  return taken;
}

export function getRailWidth(): number {
  return keptWidth(RAIL_WIDTH, RAIL_DEFAULT, clampRailWidth);
}

export function setRailWidth(px: number): number {
  return keepWidth(RAIL_WIDTH, px, clampRailWidth);
}

export function getSideWidth(): number {
  return keptWidth(SIDE_WIDTH, SIDE_DEFAULT, clampSideWidth);
}

export function setSideWidth(px: number): number {
  return keepWidth(SIDE_WIDTH, px, clampSideWidth);
}

/** Whether a column has been asked for. Both start shown: the face has always drawn them, and a
 *  first run that came up with two closed columns would be hiding what it is offering. */
function keptShown(key: string): boolean {
  return (typeof localStorage === "undefined" ? null : localStorage.getItem(key)) !== "0";
}

function keepShown(key: string, want: boolean): boolean {
  try {
    localStorage.setItem(key, want ? "1" : "0");
  } catch { /* take the answer even where localStorage is unavailable */ }
  return want;
}

export function getRailShown(): boolean {
  return keptShown(RAIL_SHOWN);
}

export function setRailShown(want: boolean): boolean {
  return keepShown(RAIL_SHOWN, want);
}

export function getSideShown(): boolean {
  return keptShown(SIDE_SHOWN);
}

export function setSideShown(want: boolean): boolean {
  return keepShown(SIDE_SHOWN, want);
}

/** Which half of the file face is up: the memo a person writes on, or the folder's own files. */
export type SideTab = "files" | "memo";

/**
 * The half this device had up, or the one the face opens on where nothing has been kept.
 *
 * **It opens on the memo**, and the reason is not which of the two is used more. It is who starts:
 * the memo is opened by a person who wants it, and nothing is lost by its being closed — it is there
 * the moment they ask. The files half is a reading of what the folder has been doing
 * (`../files/FilesPanel`), which is there to be gone to whenever a person wants it and asks for
 * nothing while they are away. The memo also has something in it the moment it is opened — their own
 * words — where the files half opens on whatever happened to move last.
 *
 * Anything else kept reads as the memo: the value is one of two words, and a word from an older
 * build or a hand-edited store is not an answer.
 */
export function getSideTab(): SideTab {
  const kept = typeof localStorage === "undefined" ? null : localStorage.getItem(SIDE_TAB);
  return kept === "files" ? "files" : "memo";
}

/** Keep the half that was asked for, and answer with it — a half even where nothing can be kept. */
export function setSideTab(which: SideTab): SideTab {
  try {
    localStorage.setItem(SIDE_TAB, which);
  } catch { /* take the half even where localStorage is unavailable */ }
  return which;
}

/**
 * Whether the columns beside the panes are drawers rather than columns.
 *
 * **It is decided by room and by nothing else.** What is measured is whether a pane's worth of
 * floor is left in the middle — not whether the count that was asked for would fit across it. The
 * count is deliberately not in it: it would make the width a window needs jump as the count goes
 * up, so the same window would fold its columns at one count and keep them at another, and on a
 * window sitting near that boundary opening the rail, opening the memo or dragging an edge would
 * each flip the answer. Nor is somebody rescued by the fold: closing both columns hands the panes
 * only what those columns were taking, split across the count, which does not turn a pane too
 * narrow to read into one that reads — it takes the rail away for nothing. What is protected is
 * that the middle does not collapse. Making the chosen count comfortable is not this side's to
 * promise: a count that is cramped on a narrow window is the choice of whoever pressed for it.
 *
 * Asking for one pane is not asking for the rail to go away either — somebody who splits a wide
 * screen down to one terminal wants that terminal large, and a face that closed the rail on them
 * would be answering a question nobody asked.
 *
 * `rail` and `side` are the widths those columns would take if they were columns, which is zero for
 * one the person has closed: closing a column is what makes room, so it has to count. They are the
 * wish rather than what is drawn, so this cannot answer itself — a drawer that took no width would
 * make the window wide enough for columns, which would make it a column again.
 */
export function sidesAreDrawers(width: number, rail: number, side: number): boolean {
  return width - rail - side < PANE_MIN;
}
