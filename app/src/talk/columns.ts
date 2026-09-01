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
// **A column is always a column.** It never lies over the panes, because the room for it is always
// there: the window's own floor is 960px and the three floors together are 640px, and the ceilings
// below are what keep the middle from being dragged away (`AMB-D-816`).
//
// **Which half of the file face is up is kept the same way.** It is a thing about the person rather
// than about the work — the same reading that put the agent a pane starts with on this device
// (`AMB-T-3686`) — so the answer they left is the one they come back to, and what the face opens on
// is only ever the first run's answer.

/**
 * The least width a terminal is worth drawing in.
 *
 * It is what the middle is kept at: a column's ceiling is whatever the window has left once this
 * and the column on the other side are out of it. Rounded rather than derived — what a terminal
 * needs is however many columns of text the program in it expects, and no number this side can
 * compute answers that.
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

/**
 * The most a column may take: whatever the window has left once the column on the other side and a
 * pane's floor are out of it, and never less than its own floor (`AMB-D-816`).
 *
 * **Room, not a share of the window.** A share cannot answer this: 0.3 and 0.4 of a 960px window are
 * 288px and 384px, and a person who drags both out is left with 288px in the middle — under the
 * floor a pane is drawn at, on the narrowest window the application opens. Measured against the
 * room, dragging a column simply stops where the middle would start giving way.
 *
 * `other` is what the column on the other side is taking, which is zero for one that is closed:
 * closing a column is what makes room, so it counts. Where it is not known — a width read back
 * before the face has drawn — the other side's floor stands in for it, which is the least it can
 * ever be taking while it is open.
 */
function ceiling(other: number, floor: number): number {
  const room = typeof window === "undefined" ? 1280 : window.innerWidth;
  return Math.max(room - other - PANE_MIN, floor);
}

export function railMax(side: number = SIDE_MIN): number {
  return ceiling(side, RAIL_MIN);
}

export function sideMax(rail: number = RAIL_MIN): number {
  return ceiling(rail, SIDE_MIN);
}

export function clampRailWidth(px: number, side: number = SIDE_MIN): number {
  return Math.min(Math.max(px, RAIL_MIN), railMax(side));
}

export function clampSideWidth(px: number, rail: number = RAIL_MIN): number {
  return Math.min(Math.max(px, SIDE_MIN), sideMax(rail));
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

export function getRailWidth(side: number = SIDE_MIN): number {
  return keptWidth(RAIL_WIDTH, RAIL_DEFAULT, (px) => clampRailWidth(px, side));
}

export function setRailWidth(px: number, side: number = SIDE_MIN): number {
  return keepWidth(RAIL_WIDTH, px, (one) => clampRailWidth(one, side));
}

export function getSideWidth(rail: number = RAIL_MIN): number {
  return keptWidth(SIDE_WIDTH, SIDE_DEFAULT, (px) => clampSideWidth(px, rail));
}

export function setSideWidth(px: number, rail: number = RAIL_MIN): number {
  return keepWidth(SIDE_WIDTH, px, (one) => clampSideWidth(one, rail));
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

