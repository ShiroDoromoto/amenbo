// Which of this window's sessions have a turn standing in them, whether or not a pane is drawing them.
//
// A pane is a drawing of a session and comes down whenever the person turns to another page or another
// project (`../shell/TerminalPane`). **The turn does not come down with it** — that is exactly the turn
// the dots on the pages and the badges on the project tabs exist to carry (`AMB-T-3610`).
//
// **The hand goes up on the host and comes down here** (`AMB-D-860`, `AMB-D-859`). An agent declares a
// turn, and the host keeps it on the session (`crate::pty::Pane`), which outlives every pane: nothing
// is assembled in the webview out of the statements, because a second record is one the pane's own row
// could come to disagree with (`./plate`). What ends a turn is the person arriving at that pane — a
// thing the window can see and the host cannot — so `sawPane` is written here, beside the answer it
// takes from.
//
// **`session://said` carries the signal and not the value.** It says an agent spoke; what it said is
// read back off `pty_sessions`.
//
// **`waiting` alone crosses.** What the face makes of this is a turn an agent declared, and never a
// silence — the same rule the badge on the face switch already reads by (`../shell/terminalBadge`,
// `AMB-D-748`). The other half of a turn is the sentence left unsent, which belongs to the pane: it is
// the only thing that can see its own input box.
//
// **One record for the window, not one per reader.** Everything that reads this reads the same answer,
// so the listeners are taken up once, on the first watcher, and what is held outlives every one of
// them — a reader that comes and goes with a pane cannot take the window's memory with it.

import type { PtySessionDto, SessionSaidDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { CLOSED_EVENT, SAID_EVENT } from "./terminal";

/** What the window knows about the turns in its sessions. */
export type Turns = {
  /** The sessions a turn is standing in: declared to the host, and not gone to since. */
  readonly standing: ReadonlySet<string>;
  /** When the person came to each pane whose turn was taken down that way (RFC3339 UTC). It is handed
   *  out because the row above a pane draws the same turn and must not answer differently
   *  (`./plate`, `AMB-D-859`). */
  readonly seen: ReadonlyMap<string, string>;
};

/** Nobody's turn, and nowhere anybody has been. */
export const NO_TURNS: Turns = { standing: new Set<string>(), seen: new Map<string, string>() };

/** The host's answer: the sessions whose agent has declared a turn. */
let declared: ReadonlySet<string> = new Set<string>();
/** The panes the person has come to since the turn in them was declared. */
let seen = new Map<string, string>();
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

/** Whether two answers name the same sessions, so a reading that has not moved draws nothing again. */
function same(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a.size !== b.size) return false;
  for (const one of a) if (!b.has(one)) return false;
  return true;
}

/** Work out what is standing and hand it round, where either half has moved. */
function settle(seenMoved: boolean): void {
  const standing = new Set([...declared].filter((session) => !seen.has(session)));
  if (!seenMoved && same(standing, turns.standing)) return;
  turns = { standing, seen: new Map(seen) };
  // Over a copy: a watcher taken off while the round is being handed out is one that would otherwise
  // still be called, and a pane going away in answer to what it just heard is an ordinary thing here.
  for (const watcher of [...watchers]) watcher(turns);
}

/** Ask the host which sessions have a turn declared in them. */
function read(): void {
  const mine = ++latest;
  void invoke<PtySessionDto[]>("pty_sessions")
    .then((open) => {
      if (!wanted || mine !== latest) return;
      declared = new Set(open.filter((one) => one.waiting !== null).map((one) => one.session));
      // A session the host no longer holds is one whose terminal has ended. Nothing about it is kept:
      // a session has no existence outside the terminal it runs in (`AMB-D-749`).
      for (const session of [...seen.keys()]) if (!declared.has(session)) seen.delete(session);
      settle(false);
    })
    .catch(() => {});
}

function listen(): void {
  void import("@tauri-apps/api/event")
    .then(async ({ listen }) => {
      const offSaid = await listen<SessionSaidDto>(SAID_EVENT, ({ payload }) => {
        // **A turn said after the person has left stands again.** The arrival that took the last one
        // down answered that one, and this is a new call.
        if (payload.verb === "waiting" && seen.delete(payload.session)) settle(true);
        read();
      });
      const offClosed = await listen<string>(CLOSED_EVENT, ({ payload }) => {
        // Nothing about a session outlives the session (`AMB-D-749`), the arrival that answered its
        // turn included.
        if (seen.delete(payload)) settle(true);
        read();
      });
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
 * is an answer, and a window with nothing drawing from it has nothing to keep it up to date for.
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
 */
export function sawPane(session: string, at: string = new Date().toISOString()): void {
  // A pane with no turn declared in it records nothing, and a second arrival at one records nothing
  // twice: a watcher woken for a change that did not happen would redraw every row for nothing.
  if (!declared.has(session) || seen.has(session)) return;
  seen.set(session, at);
  settle(true);
}
