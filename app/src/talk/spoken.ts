// What the sessions of this window have said, kept where a pane going away cannot take it.
//
// A pane is a drawing of a session and comes down whenever the person turns to another page or
// another project (`../shell/TerminalPane`). **The statements do not come down with it**, and the one
// place that hears them is inside the pane: `session://said` is listened for in `./terminal`, by the
// pane drawing that session and nobody else. So an agent that hands its turn over while the reader is
// somewhere else is speaking to a listener that is not there — which is exactly the turn the dots on
// the pages and the badges on the project tabs exist to carry (`AMB-T-3610`). This is the same event,
// heard once for the whole window, so what those two are read off outlives the pane.
//
// **`waiting` alone crosses.** What is kept here is the map (`./sessions`); what the face makes of it
// is a turn an agent declared, and never a silence — the same rule the badge on the face switch
// already reads by, and for the same reason (`../shell/terminalBadge`, `AMB-D-748`). The other half
// of a turn is the sentence left unsent, which belongs to the pane: it is the only thing here that
// can see its own input box.
//
// Nothing here is remembered past the window: a session has no existence outside the terminal it runs
// in (`AMB-D-749`), and a session that ends is dropped on the news of it ending.

import { closed, NO_SESSIONS, said, type Sessions } from "./sessions";
import { CLOSED_EVENT, SAID_EVENT } from "./terminal";
import type { SessionSaidDto } from "../bindings/bindings";

/**
 * Hear every statement made in this window, and answer with the map each time it moves.
 *
 * What comes back stops the listening. The listeners are taken up asynchronously, so one stopped
 * before they landed has to be remembered as stopped rather than left to arrive into nothing.
 *
 * Outside Tauri (`npm run dev` in a browser) there is no host to listen to and nothing that says
 * anything, so the map stays as it started.
 */
export function watchSpoken(onChange: (sessions: Sessions) => void): () => void {
  let sessions: Sessions = NO_SESSIONS;
  let live = true;
  let stop: (() => void) | null = null;
  void import("@tauri-apps/api/event")
    .then(async ({ listen }) => {
      const offSaid = await listen<SessionSaidDto>(SAID_EVENT, ({ payload }) => {
        sessions = said(sessions, payload);
        onChange(sessions);
      });
      const offClosed = await listen<string>(CLOSED_EVENT, ({ payload }) => {
        const next = closed(sessions, payload);
        if (next === sessions) return;
        sessions = next;
        onChange(next);
      });
      const off = () => { offSaid(); offClosed(); };
      if (live) stop = off;
      else off();
    })
    .catch(() => {});
  return () => {
    live = false;
    stop?.();
    stop = null;
  };
}
