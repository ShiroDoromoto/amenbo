// Whether the panel asks before it puts a row in the bin. A device-local setting (persisted), kept
// in localStorage like the dismissed update banner (`../core/updateDismissed`) — it is a habit of
// the person at this machine, not something about the project.
//
// **It is on until somebody turns it off, and only they can.** A file manager does not ask, but a
// list of files inside an editor does: deleting is rare there, the row sits a pixel away from the
// ones that open a file, and the bin is nowhere on the screen to reassure anybody afterwards
// (`AMB-D-777`). The undo does not make the question redundant — it is what makes the answer to a
// slip cheap, and a slip nobody noticed is still one nobody undid.
//
// Stored as the *off* switch rather than the on one, so a machine with no localStorage — and a
// reader who has never touched the checkbox — both land on asking.
//
// **What writes it is two presses, not one.** The checkbox in the question turns it off; the settings
// screen turns it back on. The second is not a convenience — the first is drawn inside the thing it
// silences, so without a second place the switch only ever goes one way (`AMB-D-777`).
const KEY = "amenbo.trashWithoutAsking";

/** Whether a row about to be binned still gets a question. True where nothing was ever stored. */
export function asksBeforeTrash(): boolean {
  try {
    return localStorage.getItem(KEY) !== "yes";
  } catch {
    return true; // nothing could have been remembered, so the question stands
  }
}

/**
 * Remember the answer to "do not ask again", or forget it.
 *
 * **Both directions are somebody's press.** The question's own checkbox only ever turns it off, and
 * it is drawn inside the question — so a reader who turned it off would never see it again, and the
 * one-way switch was a setting nobody could take back. The way back is in the settings screen
 * (`../screens/SettingsScreen`), which is where a reader goes looking for a switch they remember
 * flipping.
 */
export function setAsksBeforeTrash(asks: boolean): void {
  try {
    if (asks) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, "yes");
  } catch {
    /* take the answer for this session even where localStorage is unavailable */
  }
}
