// Whether a pane comes up with the box under it open (`AMB-D-889`). A device-local setting
// (persisted), kept in localStorage like the theme (`./theme`) and the question before a row goes in
// the bin (`../files/askBeforeTrash`) — it is a habit of the person at this machine, not something
// about the project, and syncing it would hand one person's way of working to everybody else's
// machine.
//
// **What it answers is only where the next pane starts.** A pane already on the screen is left
// where its reader put it: the box takes room from the terminal above it, and a pane that folded
// itself because somebody pressed a different pane's control would wake the program inside to
// repaint at a moment nobody asked it to (`AMB-D-864`).
//
// Stored as the *open* switch rather than the shut one, so a machine with no localStorage — and a
// reader who has never pressed the control — both land on folded, which is what `AMB-D-889` settled
// as the way a pane starts.
//
// **There is no row for it in the settings screen, and it does not need one.** The switch is the
// press on the pane's own band (`../shell/TerminalPane`): it is in front of the reader whenever the
// setting is in effect, and the way back is the same press.
const KEY = "amenbo.composeStartsOpen";

/** Whether the next pane opens with its box open. False where nothing was ever stored. */
export function composeStartsOpen(): boolean {
  try {
    return localStorage.getItem(KEY) === "yes";
  } catch {
    return false; // nothing could have been remembered, so a pane starts the way it starts
  }
}

/** Remember which of the two the reader last chose, for the panes opened after this one. */
export function setComposeStartsOpen(open: boolean): void {
  try {
    if (open) localStorage.setItem(KEY, "yes");
    else localStorage.removeItem(KEY);
  } catch {
    /* the pane the press was made in still folds — only the panes after it forget */
  }
}
