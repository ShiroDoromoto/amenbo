// Whether the task face's left sidebar is drawn compact — the marks and the icons alone, without the
// names. A device-local UI setting (persisted), kept in localStorage beside the sidebar's own width
// (core/sidebarWidth): it is neither domain data nor ephemeral state.
//
// **There is no state that takes the column away.** It had one, and what it cost was the way back:
// a column that is gone has to be brought back from somewhere else, so the control for it sat in the
// bar over the whole window and said nothing about which of the two faces it moved (`AMB-D-848`).
// Compact is 46px — near enough to nothing that the width is still reclaimed — and the way back is
// inside the column it is about, the way the terminal face's tabs have always done it
// (`../talk/columns`).
//
// The key is the one the old state was kept under, and a `1` an older build wrote is read as compact.
// What that build kept was "this column is in my way", and compact is the nearest thing this one has
// to what was asked for.
const KEY = "amenbo.sidebarCollapsed";

export function getSidebarCompact(): boolean {
  return (typeof localStorage !== "undefined" ? localStorage.getItem(KEY) : null) === "1";
}

/** Persist and return the state actually adopted. Returns the value even where localStorage is unavailable. */
export function setSidebarCompact(compact: boolean): boolean {
  try { localStorage.setItem(KEY, compact ? "1" : "0"); } catch { /* apply the state even where localStorage is unavailable */ }
  return compact;
}
