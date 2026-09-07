// Which of this window's sessions have a turn standing in them, whether or not a pane is drawing them.
//
// A pane is a drawing of a session and comes down whenever the person turns to another page or another
// project (`../shell/TerminalPane`). **The turn does not come down with it** — that is exactly the turn
// the dots on the pages and the badges on the project tabs exist to carry (`AMB-T-3610`).
//
// **Nothing about a turn is kept here** (`AMB-D-860`). The host holds it on the session
// (`crate::pty::Pane`), which outlives every pane and every window: a turn goes up when an agent says
// so and comes down when a person comes to the pane (`AMB-D-859`), and both are written there. Kept in
// the webview, the arrival would go away with the webview while the declaration stayed — and a second
// window drawing the same session would open with every answered turn standing again. So this asks and
// answers, and holds no record of its own that could disagree with the pane's row (`./plate`).
//
// **`session://said` carries the signal and not the value.** It says an agent spoke; what it said is
// read back off `pty_sessions`.
//
// **`waiting` alone crosses.** What the face makes of this is a turn an agent declared, and never a
// silence — the same rule the badge on the face switch already reads by (`../shell/terminalBadge`,
// `AMB-D-748`). The other half of a turn is the sentence left unsent, which belongs to the pane: it is
// the only thing that can see its own input box.
//
// **One reading for the window, not one per reader.** Everything that reads this reads the same
// answer, so the listeners are taken up once, on the first watcher, and the answer outlives every one
// of them — a reader that comes and goes with a pane cannot take it away.

import type { PtySessionDto, SessionSaidDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { CLOSED_EVENT, SAID_EVENT } from "./terminal";

/** The turns standing in this window's sessions: the session, and why it was handed over. Being in
 *  here is what standing means — a turn the person has come to is not in it (`AMB-D-859`). */
export type Turns = ReadonlyMap<string, string>;

/** Nobody's turn. */
export const NO_TURNS: Turns = new Map<string, string>();

/** What the watchers were last told. */
let turns: Turns = NO_TURNS;

const watchers = new Set<(turns: Turns) => void>();
/** How to stop listening, once the host's listeners have landed. `null` before that and after it. */
let stopListening: (() => void) | null = null;
/** Whether anything is still watching, read where the listeners land: they are taken up
 *  asynchronously, so the last watcher can leave before they arrive. */
let wanted = false;
/** Which read of the host is the current one. They are separate round trips, and an older one landing
 *  last would put back an answer the newer one had already moved past. */
let latest = 0;

/** Whether two answers say the same thing about the same sessions. */
function same(a: Turns, b: Turns): boolean {
  if (a.size !== b.size) return false;
  for (const [session, why] of a) if (b.get(session) !== why) return false;
  return true;
}

/** Ask the host which sessions have a turn standing in them, and hand the answer round where it moved. */
function read(): void {
  const mine = ++latest;
  void invoke<PtySessionDto[]>("pty_sessions")
    .then((open) => {
      if (!wanted || mine !== latest) return;
      const now = new Map<string, string>();
      for (const one of open) if (one.waiting !== null) now.set(one.session, one.waiting);
      if (same(now, turns)) return;
      turns = now;
      // Over a copy: a watcher taken off while the round is being handed out is one that would
      // otherwise still be called, and a pane going away in answer to what it just heard is an
      // ordinary thing here.
      for (const watcher of [...watchers]) watcher(turns);
    })
    .catch(() => {});
}

function listen(): void {
  void import("@tauri-apps/api/event")
    .then(async ({ listen }) => {
      const offSaid = await listen<SessionSaidDto>(SAID_EVENT, () => read());
      // A session that has ended is one the host stops answering with at all.
      const offClosed = await listen<string>(CLOSED_EVENT, () => read());
      const off = () => { offSaid(); offClosed(); };
      if (wanted) stopListening = off;
      else off();
    })
    // Outside Tauri (`npm run dev` in a browser) there is no host to listen to and nothing that says
    // anything, so the answer stays as it started.
    .catch(() => {});
}

/**
 * Watch the turns standing in this window's sessions, and answer with them each time they move.
 *
 * What comes back stops this watcher. The last one leaving stops the listening as well — what is kept
 * is a reading, and a window with nothing drawing from it has nothing to keep it up to date for.
 */
export function watchStanding(onChange: (turns: Turns) => void): () => void {
  watchers.add(onChange);
  if (!wanted) {
    wanted = true;
    listen();
    // What is already standing, for a face that has just come up: the window it was split out of, or
    // the interface being rebuilt around sessions that never stopped running (`AMB-D-753`).
    read();
  }
  onChange(turns);
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
 * It is said to the host rather than kept here, for the reason the declaration is kept there: the
 * session outlives the window, and half an answer held on either side of that comes apart.
 */
export function sawPane(session: string): void {
  void invoke<void>("pty_saw", { session })
    .then(() => read())
    .catch(() => {});
}
