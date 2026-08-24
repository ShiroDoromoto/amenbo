// Which OS the webview is running on, for the places a label has to name something the OS owns.
//
// Tauri gives no platform to the frontend without a plugin, so the answer comes off the user agent
// the webview already sends — the same source `blobUrl.ts` reads for the host form of its URLs.
// That is a weaker signal than asking the OS, so this admits a third answer: anything the string
// does not positively place on macOS or Windows is `other`, and callers say something true of any
// file manager rather than guess a name.
import type { UiKey } from "./i18n/keys";

/** macOS and Windows are named because their file manager has a name every reader knows. */
export type HostOs = "macos" | "windows" | "other";

/** The user agent this webview reports, or an empty string outside a browser (tests, node). */
function userAgent(): string {
  return typeof navigator !== "undefined" ? navigator.userAgent : "";
}

/** The OS the webview runs on, as far as its user agent admits. */
export function hostOs(ua: string = userAgent()): HostOs {
  if (/Windows/i.test(ua)) return "windows";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macos";
  return "other";
}

/**
 * The label for "show this folder where the OS shows folders", named for the file manager the press
 * actually opens. `reveal_item_in_dir` opens Finder on macOS, Explorer on Windows and whatever the
 * machine has on Linux, so one label naming Finder is a lie on two of the three.
 */
export function revealLabelKey(os: HostOs = hostOs()): UiKey {
  if (os === "windows") return "newproj.openExplorer";
  if (os === "macos") return "newproj.openFinder";
  return "newproj.openFileManager";
}
