// Which of this window's sessions have a turn standing in them, whether or not a pane is drawing them.
//
// A pane is a drawing of a session and comes down whenever the person turns to another page or another
// project (`../shell/TerminalPane`). **The turn does not come down with it** — that is exactly the turn
// the dots on the pages and the badges on the project tabs exist to carry (`AMB-T-3610`) — so it is
// kept where the session is, which is the host (`crate::pty::Pane`, `AMB-D-860`).
//
// **Nothing is assembled here.** `session://said` says only that an agent spoke; what it said is read
// back off `pty_sessions`, which is the one record of it. A second one built out of the events would
// be a copy the pane's own row could come to disagree with (`./plate`), and the two would then say
// different things about the same pane.
//
// **`waiting` alone crosses.** What the face makes of this is a turn an agent declared, and never a
// silence — the same rule the badge on the face switch already reads by (`../shell/terminalBadge`,
// `AMB-D-748`). The other half of a turn is the sentence left unsent, which belongs to the pane: it is
// the only thing that can see its own input box.

import type { PtySessionDto, SessionSaidDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { CLOSED_EVENT, SAID_EVENT } from "./terminal";

/** No session has a turn standing in it. */
export const NO_TURNS: ReadonlySet<string> = new Set<string>();

/** Whether two answers name the same sessions, so a reading that has not moved draws nothing again. */
function same(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a.size !== b.size) return false;
  for (const one of a) if (!b.has(one)) return false;
  return true;
}

/**
 * Watch the turns standing in this window's sessions, and answer with the set each time it moves.
 *
 * What comes back stops the watching. The listeners are taken up asynchronously, so one stopped before
 * they landed has to be remembered as stopped rather than left to arrive into nothing.
 *
 * The host is asked once as this starts and again on every statement and every terminal ending: a
 * statement is a person-scale event, and the answer is a list this process already holds in memory.
 * Reads are numbered because they are separate round trips — an older one landing last would put back
 * a reading the newer one had already moved past.
 *
 * Outside Tauri (`npm run dev` in a browser) there is no host to listen to and nothing that says
 * anything, so the answer stays as it started.
 */
export function watchStanding(onChange: (standing: ReadonlySet<string>) => void): () => void {
  let live = true;
  let stop: (() => void) | null = null;
  let latest = 0;
  let told: ReadonlySet<string> = NO_TURNS;

  const read = () => {
    const mine = ++latest;
    void invoke<PtySessionDto[]>("pty_sessions")
      .then((open) => {
        if (!live || mine !== latest) return;
        const now = new Set(
          open.filter((one) => one.waiting !== null).map((one) => one.session),
        );
        if (same(now, told)) return;
        told = now;
        onChange(now);
      })
      .catch(() => {});
  };

  void import("@tauri-apps/api/event")
    .then(async ({ listen }) => {
      const offSaid = await listen<SessionSaidDto>(SAID_EVENT, read);
      // A session that ended has no turn standing in it, and the host stops answering with it at all.
      const offClosed = await listen<string>(CLOSED_EVENT, read);
      const off = () => { offSaid(); offClosed(); };
      if (live) stop = off;
      else off();
    })
    .catch(() => {});

  // What is already standing, for a face that has just come up: the window it was split out of, or
  // the interface being rebuilt around sessions that never stopped running (`AMB-D-753`).
  read();

  return () => {
    live = false;
    stop?.();
    stop = null;
  };
}
