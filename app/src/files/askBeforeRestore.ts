// Whether the face asks before it throws away what a file has that git has not recorded. A
// device-local setting, kept the way the bin's question is (`./askBeforeTrash`) — it is a habit of
// the person at this machine, not something about the project.
//
// **It is a switch of its own and not the bin's.** The two questions read alike and stand in the
// same place in the settings, but what they cost is not the same thing said twice: a row in the bin
// comes back, and a change that was never written down is nowhere once it is gone (`AMB-D-906`,
// `AMB-D-777`). Somebody who silenced the reversible question has said nothing about the other one,
// and a single switch would put words in their mouth.
//
// Stored as the *off* switch rather than the on one, so a machine with no localStorage — and a
// reader who has never touched the checkbox — both land on asking.
const KEY = "amenbo.restoreWithoutAsking";

/** Whether a change about to be thrown away still gets a question. True where nothing was stored. */
export function asksBeforeRestore(): boolean {
  try {
    return localStorage.getItem(KEY) !== "yes";
  } catch {
    return true; // nothing could have been remembered, so the question stands
  }
}

/**
 * Remember the answer to "do not ask again", or forget it.
 *
 * **Both directions are somebody's press**, for the reason the bin's are: the question's own
 * checkbox only ever turns it off and is drawn inside the thing it silences, so the way back is in
 * the settings screen (`../screens/SettingsScreen`).
 */
export function setAsksBeforeRestore(asks: boolean): void {
  try {
    if (asks) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, "yes");
  } catch {
    /* take the answer for this session even where localStorage is unavailable */
  }
}
