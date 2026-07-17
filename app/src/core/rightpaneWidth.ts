// The width of the right pane (task detail / compose / decision detail). A device-local UI setting (persisted), kept
// in localStorage like the theme (core/theme) — it is neither domain data nor ephemeral state.
// The default is generous (480px): it is easy to miss that the pane can be dragged wider, so it starts out with room
// to work in. Dragging the handle takes it up to ~50% of the viewport width, and down to a fixed floor that keeps it
// readable. Validation of the value (NaN, out of range) is guaranteed by clamping on both read and write.
const KEY = "amenbo.rightpaneWidth";

export const RIGHTPANE_MIN = 320;
export const RIGHTPANE_DEFAULT = 480;

/** The maximum width = 50% of the viewport, never below the floor. Even on a narrow screen the minimum width is honoured. */
export function rightpaneMax(): number {
  const half = typeof window !== "undefined" ? Math.round(window.innerWidth * 0.5) : 720;
  return Math.max(half, RIGHTPANE_MIN);
}

/** Clamp a px value into [min, max]. */
export function clampRightpaneWidth(px: number): number {
  return Math.min(Math.max(px, RIGHTPANE_MIN), rightpaneMax());
}

export function getRightpaneWidth(): number {
  const raw = typeof localStorage !== "undefined" ? localStorage.getItem(KEY) : null;
  const n = raw !== null ? Number(raw) : NaN;
  return Number.isFinite(n) ? clampRightpaneWidth(n) : RIGHTPANE_DEFAULT;
}

/** Clamp, persist, and return the width actually adopted. Returns a value even where localStorage is unavailable. */
export function setRightpaneWidth(px: number): number {
  const w = clampRightpaneWidth(px);
  try { localStorage.setItem(KEY, String(w)); } catch { /* apply the width even where localStorage is unavailable */ }
  return w;
}
