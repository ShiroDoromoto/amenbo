// The two halves of "what can this machine start" that are not a question the frame puts once
// (`crate::wake`).
//
// Asking is `wake_choices`, and the frame does that itself. What is here is the pair around it: the
// word that the answer has changed since it was drawn, and the press that goes and asks again.
//
// **The word carries no rows.** What changed is the device's settings, and the frame already has a
// question that reads them, so being told is the whole of it and the thing to do with it is ask
// again — the shape `folder-changed` takes (`AMB-D-785`).
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";

/** The host's word that a fresh probe found something other than what was remembered. */
const REFRESHED_EVENT = "agents-installed";

/**
 * Be told when what this machine can start has changed under the answer on the screen, until the
 * returned function is called.
 *
 * The host draws on a remembered answer so a window does not wait on a login shell, and asks the
 * machine again behind it (`crate::wake`). This is how the fresh answer reaches a frame that is
 * already drawn.
 */
export async function onAgentsInstalled(take: () => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  try {
    const { listen } = await import("@tauri-apps/api/event");
    return await listen(REFRESHED_EVENT, () => take());
  } catch {
    // Not being told is what the frame already does without this: it draws the answer it was given
    // and asks again the next time it is put on screen. Nothing here is worth taking the frame down
    // for, so a listen that will not attach is swallowed the way the read's failure is.
    return () => {};
  }
}

/**
 * Ask this machine again, now — the **search again** put up where the answer could not be got
 * (`AMB-D-792`).
 *
 * What comes back is whether the machine could be reached, not what it has: what it has went to the
 * settings and out as the word above, so a press that worked is followed by asking again like any
 * other. Outside Tauri there is no machine to ask, and saying so is truer than claiming a reach.
 */
export async function wakeRescan(): Promise<boolean> {
  if (!inTauri()) return false;
  return await invoke<boolean>("wake_rescan");
}
