// The two halves of "what can this machine start" that are not a question the frame puts once
// (`crate::wake`).
//
// Asking is `wake_choices`, and the frame does that itself. What is here is what stands around it:
// the two words that say the answer has changed since it was drawn — what this machine can start,
// and what this person last opened with — and the press that goes and asks again.
//
// **A word carries no rows.** What changed is the device's settings, and the frame already has a
// question that reads them, so being told is the whole of it and the thing to do with it is ask
// again — the shape `folder-changed` takes (`AMB-D-785`).
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";

/** The host's word that a fresh probe found something other than what was remembered. */
const REFRESHED_EVENT = "agents-installed";

/** The host's word that this person's answer changed — what they last opened a pane with. */
const CHOSEN_EVENT = "agent-chosen";

/**
 * Be told when what this machine can start has changed under the answer on the screen, until the
 * returned function is called.
 *
 * The host draws on a remembered answer so a window does not wait on a login shell, and asks the
 * machine again behind it (`crate::wake`). This is how the fresh answer reaches a frame that is
 * already drawn.
 */
export async function onAgentsInstalled(take: () => void): Promise<() => void> {
  return await told(REFRESHED_EVENT, take);
}

/**
 * Be told when what this person last opened a pane with has changed, until the returned function is
 * called.
 *
 * A frame reads the ranks once, when it is drawn (`crate::wake`), and the frame standing beside a
 * pane was drawn while the answer was still nobody's. Without this it goes on asking what the person
 * answered by opening that pane (`AMB-T-4357`).
 */
export async function onAgentChosen(take: () => void): Promise<() => void> {
  return await told(CHOSEN_EVENT, take);
}

/** Listen for one of the host's words, where there is a host to listen to. */
async function told(event: string, take: () => void): Promise<() => void> {
  if (!inTauri()) return () => {};
  try {
    const { listen } = await import("@tauri-apps/api/event");
    return await listen(event, () => take());
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
