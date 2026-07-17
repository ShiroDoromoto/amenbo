// Theme (appearance) switching. The preference is os (the default, following the OS's prefers-color-scheme),
// dark, or light. The CSS is fully tokenised, so setting [data-theme="dark"] is what makes it dark
// (styles/tokens.css). This module keeps <html data-theme> holding the resolved theme (dark|light) at all times.
// The preference persists GUI-locally (localStorage). While following the OS, matchMedia changes are picked up too.
export type ThemePref = "os" | "dark" | "light";

const KEY = "amenbo.theme";
const PREFS: ThemePref[] = ["os", "dark", "light"];

export function getThemePref(): ThemePref {
  const v = (typeof localStorage !== "undefined" && localStorage.getItem(KEY)) as ThemePref | null;
  return v && PREFS.includes(v) ? v : "os";
}

function prefersDark(): boolean {
  return typeof window !== "undefined" && window.matchMedia
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : false;
}

/** Resolve the preference to a concrete theme (dark|light) and put it on <html data-theme>. */
function apply(pref: ThemePref): void {
  const resolved = pref === "os" ? (prefersDark() ? "dark" : "light") : pref;
  document.documentElement.dataset.theme = resolved;
}

export function setThemePref(pref: ThemePref): void {
  try { localStorage.setItem(KEY, pref); } catch { /* no localStorage: apply it anyway, just do not remember it */ }
  apply(pref);
}

/** Call once at startup: apply the current preference and, while following the OS, keep following it. */
export function initTheme(): void {
  apply(getThemePref());
  if (typeof window !== "undefined" && window.matchMedia) {
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (getThemePref() === "os") apply("os");
    });
  }
}
