// Whether this machine is using Amenbo as one window or two, and which face the one window is
// showing (`AMB-D-753`).
//
// The shape is a device-local, persisted UI setting, kept in localStorage the way the sidebar's and
// the theme's are (core/sidebarCollapsed, core/theme): it is neither domain data — nobody else's
// Amenbo cares how many windows this one is in — nor ephemeral, because coming back to the shape you
// left is the whole of "remember it". The default is one window, so nothing is split out until
// somebody asks for it.
//
// The face is **not** persisted. A launch shows the board, in one window or two: what someone opens
// Amenbo to look at is the ledger, and a terminal that came up because it was up last time would be
// a window's worth of shell the user did not ask for.
const SHAPE_KEY = "amenbo.windowShape";

/** One window with two faces in it, or the terminal split out into a window of its own. */
export type WindowShape = "one" | "two";

/** The face the board's window is showing: the ledger, or the terminal. */
export type Face = "tasks" | "terminal";

export function getWindowShape(): WindowShape {
  return (typeof localStorage !== "undefined" ? localStorage.getItem(SHAPE_KEY) : null) === "two"
    ? "two"
    : "one";
}

/** Persist and return the shape actually adopted. Returns the value even where localStorage is unavailable. */
export function setWindowShape(shape: WindowShape): WindowShape {
  try {
    localStorage.setItem(SHAPE_KEY, shape);
  } catch {
    /* take the shape even where localStorage is unavailable, just do not remember it */
  }
  return shape;
}
