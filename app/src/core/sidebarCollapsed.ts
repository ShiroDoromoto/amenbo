// Whether the left sidebar (smart views / projects) is collapsed. A device-local UI setting (persisted), kept in
// localStorage like the sidebar width (core/sidebarWidth) — it is neither domain data nor ephemeral state. Default is
// expanded (false), so nothing folds until the user toggles it. Collapsing hides the sidebar entirely (the grid column
// drops to 0), the primary use being to reclaim width on a narrow screen.
const KEY = "amenbo.sidebarCollapsed";

export function getSidebarCollapsed(): boolean {
  return (typeof localStorage !== "undefined" ? localStorage.getItem(KEY) : null) === "1";
}

/** Persist and return the collapsed flag actually adopted. Returns the value even where localStorage is unavailable. */
export function setSidebarCollapsed(collapsed: boolean): boolean {
  try { localStorage.setItem(KEY, collapsed ? "1" : "0"); } catch { /* apply the state even where localStorage is unavailable */ }
  return collapsed;
}
