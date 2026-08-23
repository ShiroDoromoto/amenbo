// Keeping a pane's label up to date, wherever the pane is drawn.
//
// A pane has two homes — the board's terminal face and the window it is split out into (`AMB-D-753`)
// — and the same line belongs above it in both. What that line says is worked out in `./nameplate`;
// this is everything around it: holding what the pane's session has said, asking the ledger what it is
// holding, and putting the row up again whenever either changes.
//
// **It listens for the store rather than being told.** A reservation is made by the agent in the pane
// running the CLI, which is a write from outside the webview drawing it — so what says one happened is
// the host noticing the store change on disk, exactly as it does for the board.

import type { SessionSaidDto, SessionWorkDto, TaskCardDto } from "../bindings/bindings";
import { currentLang, type Lang } from "../core/i18n";
import { invoke } from "../core/ipc";
import { frameNames, ONLY_FRAME, type FrameNames } from "./frames";
import {
  FINISHED_HOLD_MS,
  mountNameplate,
  NO_CHANGEOVER,
  nowOf,
  sayOf,
  standsAsTurn,
  type Changeover,
  type Held,
} from "./nameplate";
import { movingAt, STILL_AFTER_MS } from "./moving";
import { closed, NO_SESSIONS, opened, said, type Sessions } from "./sessions";

/** A pane's label, and the pane's way of telling it what happened. */
export type Plate = {
  /** A terminal has started in the pane, under this session id. */
  opened(session: string, startedAt: string): void;
  /** Something came out of the terminal. Said per chunk and read as a time, never as a quantity: what
   *  it turns into is a fixed rhythm rather than a meter (`./moving`). */
  output(): void;
  /** The agent said something about its session. */
  said(statement: SessionSaidDto): void;
  /** The program in the terminal has exited. */
  closed(session: string): void;
  /** The frames have been named afresh — what a naming answered with. */
  named(names: FrameNames): void;
  /** Take the label away. */
  stop(): void;
};

/**
 * Put a label above a pane and keep it there.
 *
 * `lang` is asked each time rather than taken once: the board knows the reader's language from the
 * snapshot, and the split-out window learns it from a question it asks as it comes up, so neither can
 * hand over a settled answer at the moment the pane is built.
 *
 * `frame` is which of the arrangement's places this pane is in (`./layout`), because the name on the
 * row belongs to the place rather than to the session (`./frames`). The split-out window is one pane
 * and has only ever had one frame, so it takes the default.
 *
 * `onWaiting` is told whenever the answer to "is a turn standing in this pane" changes. **A turn
 * stands for two reasons and neither of them is silence** (`AMB-D-748`, `AMB-T-3610`): the agent said
 * so (`waiting`), or the ledger says something this session is holding is no longer ready — a
 * blocker opened, a premise came unsettled. Both are also what the row itself leads with
 * (`./nameplate`), so the badge and the label say the same thing about the same pane. What an agent
 * has *not* said is not one of them: a pane that has gone quiet is a pane that has gone quiet.
 *
 * It is said from here because the session map and what the ledger answered are both here, and the
 * callers who want it are the shell's: with the terminal behind the other face neither this label nor
 * the dot on its page can be seen at all, so the badge on the face switch and the dots on the pages
 * are drawn from this instead (`../shell/terminalBadge`, `../shell/TerminalFace`). The split-out
 * window passes nothing — its label is on screen already. The change is what is reported, not the
 * statement: an agent at work says a great deal and almost none of it moves the answer.
 */
export function mountPlate(
  host: HTMLElement,
  lang: () => Lang = currentLang,
  onWaiting: (waiting: boolean) => void = () => {},
  frame: string = ONLY_FRAME,
): Plate {
  const draw = mountNameplate(host);

  // What the pane's session has said, and what the ledger says it is holding. Both are gone when the
  // pane is: a session has no existence outside the terminal it runs in (`AMB-D-749`).
  let sessions: Sessions = NO_SESSIONS;
  let names: FrameNames = new Map();
  let held: Held[] = [];
  let finished = 0;
  let changeover: Changeover = NO_CHANGEOVER;
  let running: string | null = null;
  // Whether a terminal has ever run in this pane. It is not `running !== null` — a pane whose program
  // has exited still has a row, because what it just finished is the one thing worth saying at that
  // moment. What has no row is a pane that has never had a session: the face there is the invitation
  // to choose a folder (`./agent`), and a label about the session would be about nothing.
  let ran = false;
  let expiry: ReturnType<typeof setTimeout> | undefined;
  let live = true;
  // What `onWaiting` was last told, so it hears the changes and not every statement.
  let waiting = false;
  // When something last came out of the terminal, and whether that still counts as moving. The time is
  // written on every chunk and the row is only redrawn when the answer turns over: a busy build prints
  // hundreds of times a second, and a row redrawn with each of them would spend the pane's frames on a
  // mark that had not changed.
  let lastOutput: number | null = null;
  let moving = false;
  let settling: ReturnType<typeof setTimeout> | undefined;

  /** Take in a chunk having crossed, and draw the change where there is one. */
  function tookOutput(): void {
    if (!live) return;
    lastOutput = Date.now();
    clearTimeout(settling);
    // Coming back for the answer once the window has passed. Nothing else says a pane has stopped —
    // stopping is the absence of an event, so the clock is the only thing that can notice it.
    settling = setTimeout(() => {
      if (moving && !movingAt(lastOutput, Date.now())) {
        moving = false;
        redraw();
      }
    }, STILL_AFTER_MS);
    if (moving) return;
    moving = true;
    redraw();
  }

  /** Say whether a turn is standing here, where that is not what was said last. */
  function tellWaiting(): void {
    // The same question the row leads with, asked once: declared, or derived (`./nameplate`).
    const now = live && standsAsTurn(sayOf(held, running === null ? undefined : sessions.get(running)));
    if (now === waiting) return;
    waiting = now;
    onWaiting(now);
  }

  function redraw(): void {
    if (!live) return;
    const { now, changeover: next } = nowOf(held, finished, changeover, Date.now());
    changeover = next;
    const name = names.get(frame) ?? null;
    // A frame that was named keeps its row whether or not anything has run in it: the name is the
    // person's, and it outlives every session the frame holds (`./frames`).
    draw(
      ran || name !== null
        ? {
            name,
            now,
            say: sayOf(held, running === null ? undefined : sessions.get(running)),
            dot: { frame, moving },
          }
        : null,
      lang(),
    );
    // "Just finished" comes down on its own, so something has to come back for it. Nothing else on the
    // row expires, and a redraw that finds it already down does nothing.
    clearTimeout(expiry);
    if (now.kind === "finished" && next.shownAt !== null) {
      expiry = setTimeout(redraw, next.shownAt + FINISHED_HOLD_MS - Date.now());
    }
  }

  /** Ask the ledger what this pane's session is on, then draw it. */
  function readWork(): void {
    const session = running;
    if (session === null) return;
    void invoke<SessionWorkDto>("session_work", { session })
      .then(async (work) => {
        const tasks = work.holding.length
          ? await invoke<TaskCardDto[]>("tasks_by_ids", { ids: work.holding })
          : [];
        // A pane that went away, or a session that ended, while the read was out: what came back is
        // about a session that is no longer the one being drawn.
        if (!live || running !== session) return;
        held = tasks;
        finished = work.finished;
        // What came back can be the whole of the change: a blocker opened on a task this pane is
        // holding while its agent said nothing at all.
        tellWaiting();
        redraw();
      })
      .catch(() => {});
  }

  void frameNames()
    .then((known) => {
      names = known;
      redraw();
    })
    .catch(() => {});

  let unlisten: (() => void) | null = null;
  void import("@tauri-apps/api/event")
    .then(({ listen }) => listen("store-changed", () => readWork()))
    .then((off) => {
      if (live) unlisten = off;
      else off();
    })
    // Outside Tauri (`npm run dev` in a browser) nothing writes to a store and nothing announces one.
    .catch(() => {});

  redraw();

  return {
    opened: (session, startedAt) => {
      sessions = opened(sessions, { session, startedAt });
      running = session;
      ran = true;
      tellWaiting();
      readWork();
    },
    output: tookOutput,
    said: (statement) => {
      sessions = said(sessions, statement);
      tellWaiting();
      redraw();
    },
    closed: (session) => {
      // A pane whose program has exited is not moving, whatever the last chunk's clock still says: the
      // stream did not go quiet, it ended.
      clearTimeout(settling);
      moving = false;
      lastOutput = null;
      sessions = closed(sessions, session);
      if (running === session) running = null;
      held = [];
      tellWaiting();
      redraw();
    },
    named: (known) => {
      names = known;
      redraw();
    },
    stop: () => {
      live = false;
      // The pane is going, and a turn standing in it is nobody's to be knocked about any more: the
      // badge is the shell's and outlives this, so it has to be told on the way out.
      tellWaiting();
      clearTimeout(expiry);
      clearTimeout(settling);
      unlisten?.();
      host.replaceChildren();
    },
  };
}
