// Theme (appearance) switching. The preference is os (the default, following the OS's prefers-color-scheme),
// dark, or light. The CSS is fully tokenised, so setting [data-theme="dark"] is what makes it dark
// (styles/tokens.css). This module keeps <html data-theme> holding the resolved theme (dark|light) at all times.
// The preference persists GUI-locally (localStorage). While following the OS, matchMedia changes are picked up too.
// An app draws more than one window, and every one of them wears the answer: the window it was answered in applies
// it and tells the others, which apply it where they stand (`CHANGED`).
export type ThemePref = "os" | "dark" | "light";

const KEY = "amenbo.theme";
const PREFS: ThemePref[] = ["os", "dark", "light"];

/**
 * What one window says when the appearance is changed in it, so the others change with it.
 *
 * **The preference is written once and applied in every window, and only the writer knows when.**
 * `localStorage` is shared between the windows of one app, but nothing tells a page it has been
 * written: a window that has already drawn keeps the appearance it came up with until something
 * makes it look again. Following the OS is the one case that repairs itself, because each window
 * hears `prefers-color-scheme` for itself — so what breaks is a reader who named dark or light while
 * the terminal was split out into a window of its own.
 *
 * The host's own events are the road rather than `storage`: a window's `storage` event is the same
 * origin's, and whether two webviews of one app are that is an answer macOS, Windows and Linux do
 * not have to agree on. An event emitted from a webview reaches every window of the app, the
 * emitting one included, and applying an appearance already applied costs nothing.
 */
const CHANGED = "theme-changed";

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
  // And the windows that are not this one. The preference travels rather than being read back, so a
  // window changes with this one even where there was no `localStorage` to remember it in.
  void import("@tauri-apps/api/event")
    .then(({ emit }) => emit(CHANGED, pref))
    // Outside Tauri (`npm run dev` in a browser) there is one window and nobody to tell.
    .catch(() => {});
}

/**
 * Call once at startup: apply the current preference, and keep applying it as it changes.
 *
 * Two things change it. The OS, while the preference is to follow it — heard per window, because
 * `prefers-color-scheme` is the window's own. And a reader, in whichever window they answered the
 * question in ({@link CHANGED}) — which is never certainly this one.
 */
export function initTheme(): void {
  apply(getThemePref());
  if (typeof window !== "undefined" && window.matchMedia) {
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (getThemePref() === "os") apply("os");
    });
  }
  // A window lives as long as the page in it, so there is nothing here to stop listening for: the
  // listener goes when the window it is applying an appearance to does.
  void import("@tauri-apps/api/event")
    .then(({ listen }) => listen<ThemePref>(CHANGED, (said) => apply(said.payload)))
    .catch(() => {});
}
