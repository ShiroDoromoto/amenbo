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
 * Remember the answer to "do not ask again", or forget it. Only the checkbox in the question writes
 * here; a reader who wants the question back turns the same checkbox off.
 */
export function setAsksBeforeTrash(asks: boolean): void {
  try {
    if (asks) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, "yes");
  } catch {
    /* take the answer for this session even where localStorage is unavailable */
  }
}
