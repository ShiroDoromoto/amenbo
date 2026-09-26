import { t } from "./i18n";
import { inTauri } from "./snapshot";

/** Whether a confirmation is on screen now, waiting for its answer. */
let asking = false;

// In the Tauri v2 webview, window.confirm()/alert()/prompt() are unimplemented no-ops (they behave
// as if the user always cancelled), so a confirmation guard built on them never runs its action
// even when the user clicks OK. Delegate to the native confirmation dialog (plugin-dialog's
// confirm), falling back to window.confirm only when iterating in a plain browser. True on OK,
// false on cancel.
//
// **One at a time.** A second call while one is still open answers false at once, without opening
// anything. On macOS, rfd (0.16.0) remembers the key window when each dialog is made and brings it
// back to the front once that dialog is gone; a second sheet made while the first is up remembers
// the first sheet itself, and once both are answered that sheet comes back as a dialog of its own
// that neither button closes. Clicks that pile up while the app is busy are what stack them.
//
// The buttons are named in the app's language: left to itself the dialog says OK and Cancel.
export async function confirmDialog(message: string): Promise<boolean> {
  if (!inTauri()) return window.confirm(message);
  if (asking) return false;
  asking = true;
  try {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    return await confirm(message, { okLabel: t("dialog.ok"), cancelLabel: t("dialog.cancel") });
  } finally {
    asking = false;
  }
}

/**
 * Open the machine's own file picker and return the paths chosen, in the order the reader chose
 * them. Empty where they cancelled, and outside Tauri, where there is no picker to open.
 *
 * **Paths, the way a drop hands them over** (`./hostDrop`) — so what is picked and what is dropped
 * go down the same road, and the two answers being one is what keeps either of them explainable.
 */
export async function pickFiles(only?: PickOnly): Promise<string[]> {
  return await picked(false, only);
}

/**
 * What a picker offers, where the caller takes one shape of file and no other. A hint rather than
 * a gate: every machine's panel lets the reader widen it, and what actually decides is the read
 * that follows, so this saves a reader from choosing a file that was never going to be taken.
 */
export type PickOnly = { name: string; extensions: string[] };

/**
 * The same, for folders. It is a second door rather than a flag on the first because the machine's
 * picker takes `directory` as a yes or a no: one window cannot offer files and folders together, so
 * whoever opens it has already decided which they are after (`@tauri-apps/plugin-dialog`).
 */
export async function pickFolders(): Promise<string[]> {
  return await picked(true);
}

/**
 * Open the machine's own save panel and return the path chosen, under a suggested name. `null`
 * where the reader cancelled, and outside Tauri, where there is no panel to open.
 *
 * A path, the way the two pickers above answer — what is written there is the host's to write, and
 * this side never holds the bytes.
 */
export async function pickSaveAs(suggested: string): Promise<string | null> {
  if (!inTauri()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const chosen = await save({ defaultPath: suggested });
  return typeof chosen === "string" ? chosen : null;
}

/** The one call both doors are: what the picker answered, as a list either way. */
async function picked(directory: boolean, only?: PickOnly): Promise<string[]> {
  if (!inTauri()) return [];
  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({ multiple: true, directory, filters: only ? [only] : undefined });
  if (typeof chosen === "string") return [chosen];
  return Array.isArray(chosen) ? chosen.filter((one) => typeof one === "string") : [];
}
