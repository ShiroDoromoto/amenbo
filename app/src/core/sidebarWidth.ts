// The width of the left sidebar (smart views / projects). A device-local UI setting (persisted), kept in localStorage
// like the right-pane width (core/rightpaneWidth) — it is neither domain data nor ephemeral state. The default (232px)
// matches the fixed width the sidebar shipped with, so nothing shifts until the user drags the handle. Dragging takes it
// up to ~40% of the viewport width, and down to a fixed floor that keeps a project name legible. Validation of the value
// (NaN, out of range) is guaranteed by clamping on both read and write.
const KEY = "amenbo.sidebarWidth";

export const SIDEBAR_MIN = 180;
export const SIDEBAR_DEFAULT = 232;

/**
 * The width the column stands at compact — the marks and the icons, and the padding around them
 * (`core/sidebarCompact`, `styles/global.css`).
 *
 * It is the terminal face's compact tab column to the pixel (`../talk/columns`), because the two
 * columns are the same column on two faces (`AMB-D-848`): a project that folded to one width on one
 * face and another on the other would be saying the fold is about the face rather than about the
 * names. Not dragged and not clamped — compact is one width, and what a person drags is the named
 * one.
 */
export const SIDEBAR_COMPACT = 46;

/** The maximum width = 40% of the viewport, never below the floor. Even on a narrow screen the minimum width is honoured. */
export function sidebarMax(): number {
  const cap = typeof window !== "undefined" ? Math.round(window.innerWidth * 0.4) : 520;
  return Math.max(cap, SIDEBAR_MIN);
}

/** Clamp a px value into [min, max]. */
export function clampSidebarWidth(px: number): number {
  return Math.min(Math.max(px, SIDEBAR_MIN), sidebarMax());
}

export function getSidebarWidth(): number {
  const raw = typeof localStorage !== "undefined" ? localStorage.getItem(KEY) : null;
  const n = raw !== null ? Number(raw) : NaN;
  return Number.isFinite(n) ? clampSidebarWidth(n) : SIDEBAR_DEFAULT;
}

/** Clamp, persist, and return the width actually adopted. Returns a value even where localStorage is unavailable. */
export function setSidebarWidth(px: number): number {
  const w = clampSidebarWidth(px);
  try { localStorage.setItem(KEY, String(w)); } catch { /* apply the width even where localStorage is unavailable */ }
  return w;
}
