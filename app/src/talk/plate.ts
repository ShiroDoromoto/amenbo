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
import { frameLabel, frameNames, ONLY_FRAME, type FrameNames } from "./frames";
import {
  faceOf,
  FINISHED_HOLD_MS,
  mountNameplate,
  NO_CHANGEOVER,
  nowOf,
  sayOf,
  standsAsTurn,
  type Changeover,
  type Held,
  type Say,
} from "./nameplate";
import { movingAt, quietFor, STILL_AFTER_MS } from "./moving";
import {
  closed,
  NO_SESSIONS,
  opened,
  said,
  sent as wentOut,
  unsent as leftUnsent,
  type Sessions,
} from "./sessions";

/** A pane's label, and the pane's way of telling it what happened. */
export type Plate = {
  /** A terminal has started in the pane, under this session id, in `folder`. The folder is what the
   *  row calls the pane until something names it (`./frames`). */
  opened(session: string, startedAt: string, folder: string | null): void;
  /** Something came out of the terminal. Said per chunk and read as a time, never as a quantity: what
   *  it turns into is a fixed rhythm rather than a meter (`./moving`). */
  output(): void;
  /** The agent said something about its session. */
  said(statement: SessionSaidDto): void;
  /** The sentence Amenbo opens an agent with was left in this pane's input box, unsent. */
  unsent(session: string): void;
  /** That sentence has since gone out of the input box, on the reader's own Enter. */
  sent(session: string): void;
  /** The program in the terminal has exited. */
  closed(session: string): void;
  /** The frames have been named afresh — what a naming answered with. */
  named(names: FrameNames): void;
  /** Whether this is the pane being worked in. It decides one thing: whether a long silence says how
   *  long. A screen of panes each carrying a clock is a screen of clocks. */
  focused(is: boolean): void;
  /** Take the label away. */
  stop(): void;
};

/**
 * Put a label above a pane and keep it there.
 *
 * `lang` is asked each time rather than taken once: what the reader's language is comes out of the
 * snapshot, which is read as the window comes up — so the answer is not settled at the moment a pane
 * is built.
 *
 * `frame` is which of the arrangement's places this pane is in (`./layout`), because the name on the
 * row belongs to the place rather than to the session (`./frames`). A lone pane that has never been
 * told which place it is takes the first of them.
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
 * are drawn from this instead (`../shell/terminalBadge`, `../shell/TerminalFace`). The change is what
 * is reported, not the statement: an agent at work says a great deal and almost none of it moves the
 * answer.
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
  // The folder this pane's terminal was started in. It is what the row is headed with until the frame
  // is named, and it is kept here rather than read back out of the arrangement: what the row says is
  // about the session in front of the reader, and a place is not one.
  let folder: string | null = null;
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
  // When something last came out of the terminal, and whether that still counts as moving — which is
  // the lamp's lit face (`./nameplate`). The time is written on every chunk and the row is only redrawn
  // when the answer turns over: a busy build prints hundreds of times a second, and a row redrawn with
  // each of them would spend the pane's frames on a mark that had not changed.
  let lastOutput: number | null = null;
  let moving = false;
  let settling: ReturnType<typeof setTimeout> | undefined;
  // Whether this is the pane being worked in, and the clock that keeps a long silence's reading true.
  // Silence raises no events, so the only thing that can notice a minute passing is a minute passing.
  let focused = false;
  let ticking: ReturnType<typeof setInterval> | undefined;

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

  /**
   * What the end of the row says.
   *
   * The session's own words come first — a turn, a premise, a note — and how long it has been quiet
   * fills the slot only when they leave it empty. A measurement of silence is the least of what can be
   * said about a pane, and it must never stand where something that was actually said would.
   *
   * It is said in the pane being worked in and nowhere else. Every pane on a screen has been quiet for
   * some length of time, and a row of clocks is what a reader stops reading.
   */
  function saying(): Say {
    const said = sayOf(held, running === null ? undefined : sessions.get(running));
    if (said.kind !== "silent" || !focused) return said;
    const minutes = quietFor(lastOutput, Date.now());
    return minutes === null ? said : { kind: "quiet", minutes };
  }

  function redraw(): void {
    if (!live) return;
    const { now, changeover: next } = nowOf(held, finished, changeover, Date.now());
    changeover = next;
    const name = frameLabel(names, frame, folder);
    // The lamp is read off the same answer the row's right is, so the two cannot come to say different
    // things about the same pane (`./nameplate`).
    const say = saying();
    // A frame that was named keeps its row whether or not anything has run in it: the name is the
    // person's, and it outlives every session the frame holds (`./frames`). A folder standing in for
    // one is not that — it is what this pane's terminal is working in, so it goes when the pane has
    // never had one.
    draw(
      ran || names.has(frame)
        ? {
            name,
            now,
            say,
            dot: { frame, face: faceOf(say, moving) },
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

  /** Ask what this pane's session is on, then draw it (`commands.rs::session_work`). */
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
    opened: (session, startedAt, where) => {
      sessions = opened(sessions, { session, startedAt });
      running = session;
      folder = where;
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
    unsent: (session) => {
      sessions = leftUnsent(sessions, session);
      // A person is needed here, so the badge on the face switch and the dot on the page hear about
      // it the same way they hear about a handed-over turn (`./nameplate`).
      tellWaiting();
      redraw();
    },
    sent: (session) => {
      sessions = wentOut(sessions, session);
      // And they are not needed any more, which the same two have to hear for the same reason.
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
    focused: (is) => {
      if (is === focused) return;
      focused = is;
      clearInterval(ticking);
      ticking = undefined;
      // A minute passing raises nothing, so the row is asked again every minute while this is the pane
      // being worked in. Only while: a pane nobody is looking at has nothing to keep true.
      if (focused) ticking = setInterval(redraw, 60_000);
      redraw();
    },
    stop: () => {
      live = false;
      // The pane is going, and a turn standing in it is nobody's to be knocked about any more: the
      // badge is the shell's and outlives this, so it has to be told on the way out.
      tellWaiting();
      clearTimeout(expiry);
      clearTimeout(settling);
      clearInterval(ticking);
      unlisten?.();
      host.replaceChildren();
    },
  };
}
