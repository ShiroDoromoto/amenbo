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
import type { Count } from "./layout";

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

/**
 * Whether the columns beside the panes are drawers rather than columns.
 *
 * **It is decided by room and by nothing else.** The count is in it only as how many panes have to
 * fit across: four are drawn two across, so two counts of panes need the same width. Asking for one
 * pane is not asking for the rail to go away — somebody who splits a wide screen down to one terminal
 * wants that terminal large, and a face that closed the rail on them would be answering a question
 * nobody asked.
 *
 * `rail` and `side` are the widths those columns would take if they were columns, which is zero for
 * one the person has closed: closing a column is what makes room, so it has to count. They are the
 * wish rather than what is drawn, so this cannot answer itself — a drawer that took no width would
 * make the window wide enough for columns, which would make it a column again.
 */
export function sidesAreDrawers(count: Count, width: number, rail: number, side: number): boolean {
  const across = count === 4 ? 2 : count;
  return width - rail - side < across * PANE_MIN;
}
