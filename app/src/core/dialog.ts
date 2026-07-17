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
