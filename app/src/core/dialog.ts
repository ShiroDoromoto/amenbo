import { inTauri } from "./snapshot";

// In the Tauri v2 webview, window.confirm()/alert()/prompt() are unimplemented no-ops (they behave
// as if the user always cancelled), so a confirmation guard built on them never runs its action
// even when the user clicks OK. Delegate to the native confirmation dialog (plugin-dialog's
// confirm), falling back to window.confirm only when iterating in a plain browser. True on OK,
// false on cancel.
export async function confirmDialog(message: string): Promise<boolean> {
  if (!inTauri()) return window.confirm(message);
  const { confirm } = await import("@tauri-apps/plugin-dialog");
  return confirm(message);
}

/**
 * Open the machine's own file picker and return the paths chosen, in the order the reader chose
 * them. Empty where they cancelled, and outside Tauri, where there is no picker to open.
 *
 * **Paths, the way a drop hands them over** (`./hostDrop`) — so what is picked and what is dropped
 * go down the same road, and the two answers being one is what keeps either of them explainable.
 */
export async function pickFiles(): Promise<string[]> {
  return await picked(false);
}

/**
 * The same, for folders. It is a second door rather than a flag on the first because the machine's
 * picker takes `directory` as a yes or a no: one window cannot offer files and folders together, so
 * whoever opens it has already decided which they are after (`@tauri-apps/plugin-dialog`).
 */
export async function pickFolders(): Promise<string[]> {
  return await picked(true);
}

/** The one call both doors are: what the picker answered, as a list either way. */
async function picked(directory: boolean): Promise<string[]> {
  if (!inTauri()) return [];
  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({ multiple: true, directory });
  if (typeof chosen === "string") return [chosen];
  return Array.isArray(chosen) ? chosen.filter((one) => typeof one === "string") : [];
}
