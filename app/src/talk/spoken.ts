// What the sessions of this window have said, and which of them a person has been back to.
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
// **The hand goes up by declaration and comes down by measurement** (`AMB-D-859`). An agent says a
// turn has come; what ends it is the person arriving at that pane, which is a thing the window can
// see and the agent cannot be relied on to say. So `sawPane` is written here rather than in either
// reader: the row above the pane and the dots on the pages have to agree about which turns are still
// standing, and a fact kept twice is a fact that comes apart.
//
// **One record for the window, not one per reader.** Everything that reads this reads the same map,
// so the listeners are taken up once, on the first watcher, and the map outlives every one of them —
// a reader that comes and goes with a pane cannot take the window's memory with it.
//
// Nothing here is remembered past the window: a session has no existence outside the terminal it runs
// in (`AMB-D-749`), and a session that ends is dropped on the news of it ending.

import { closed, NO_SESSIONS, said, seen, type Sessions } from "./sessions";
import { CLOSED_EVENT, SAID_EVENT } from "./terminal";
import type { SessionSaidDto } from "../bindings/bindings";

/** The window's own record. Module state because there is one window and one of these. */
let sessions: Sessions = NO_SESSIONS;
const watchers = new Set<(sessions: Sessions) => void>();
/** How to stop listening, once the host's listeners have landed. `null` before that and after it. */
let stopListening: (() => void) | null = null;
/** Whether anything is still watching, read where the listeners land: they are taken up
 *  asynchronously, so the last watcher can leave before they arrive. */
let wanted = false;

function moved(next: Sessions): void {
  if (next === sessions) return;
  sessions = next;
  // Over a copy: a watcher taken off while the round is being handed out is one that would otherwise
  // still be called, and a pane going away in answer to what it just heard is an ordinary thing here.
  for (const watcher of [...watchers]) watcher(sessions);
}

function listen(): void {
  void import("@tauri-apps/api/event")
    .then(async ({ listen }) => {
      const offSaid = await listen<SessionSaidDto>(SAID_EVENT, ({ payload }) => moved(said(sessions, payload)));
      const offClosed = await listen<string>(CLOSED_EVENT, ({ payload }) => moved(closed(sessions, payload)));
      const off = () => { offSaid(); offClosed(); };
      if (wanted) stopListening = off;
      else off();
    })
    // Outside Tauri (`npm run dev` in a browser) there is no host to listen to and nothing that says
    // anything, so the map stays as it started.
    .catch(() => {});
}

/**
 * Hear every statement made in this window, and answer with the map each time it moves.
 *
 * What comes back stops this watcher. The last one leaving stops the listening as well — what is kept
 * is the map, and a window with nothing drawing from it has nothing to keep it up to date for.
 */
export function watchSpoken(onChange: (sessions: Sessions) => void): () => void {
  watchers.add(onChange);
  if (!wanted) {
    wanted = true;
    listen();
  }
  onChange(sessions);
  return () => {
    watchers.delete(onChange);
    if (watchers.size > 0) return;
    wanted = false;
    stopListening?.();
    stopListening = null;
  };
}

/**
 * A person is at this pane — the turn standing in it, if one is, is over (`AMB-D-859`).
 *
 * **Arriving is the measurement, and it is the only one taken.** Printing is not: an agent that hands
 * a turn over and then prints the question would take its own hand down. Nor is the pane merely being
 * on the screen — a page of panes is several turns, and the one a person went to is the one they
 * attended to. Who says it is the pane, which is the only thing that knows both halves
 * (`../shell/TerminalPane`).
 *
 * A turn said after they have left stands again: `said` puts the question back on every `waiting`.
 */
export function sawPane(session: string, at: string = new Date().toISOString()): void {
  moved(seen(sessions, session, at));
}
