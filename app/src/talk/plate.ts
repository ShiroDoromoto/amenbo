// Keeping a pane's label up to date, wherever the pane is drawn.
//
// A pane has two homes — the board's terminal face and the window it is split out into (`AMB-D-753`)
// — and the same line belongs above it in both. What that line says is worked out in `./nameplate`;
// this is everything around it: holding what the pane's session has said, measuring what the pane
// itself is doing, and putting the row up again whenever either changes.
//
// **Nothing here is read off the ledger** (`AMB-D-858`). What a row is drawn from is what the agent
// in this pane declared and what this pane measured — both of which the webview already has, and
// neither of which the world can rewrite behind it.

import type { SessionSaidDto } from "../bindings/bindings";
import { currentLang, type Lang } from "../core/i18n";
import { frameLabel, frameNames, ONLY_FRAME, type FrameNames } from "./frames";
import {
  faceOf, mountNameplate, sayOf, standsAsTurn, type Plate as Row, type Say,
} from "./nameplate";
import { movingAt, quietFor, STILL_AFTER_MS } from "./moving";
import {
  closed,
  declared,
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
   *  row calls the pane until something names it (`./frames`).
   *
   *  `waiting` is a turn already standing in that session, which the host hands over with the rest
   *  of it (`crate::pty::pty_sessions`). It is not nothing for a pane that has just gone up: the
   *  reader turning back to a page is a pane coming up on a session that handed its turn over while
   *  they were away, and a row that started empty would be the one place saying so (`AMB-D-860`). */
  opened(session: string, startedAt: string, folder: string | null, waiting?: string | null): void;
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
  /**
   * The row as it stands, for a face that draws this pane somewhere other than above it.
   *
   * **It is the row and not a second answer** — the same value the label is drawn from, handed over
   * rather than worked out again, so a pane cannot be described two ways at once
   * (`./nameplate`, `../shell/PaneOrder`). Null where the pane has no row: what has never held a
   * session has nothing to say about one.
   *
   * What comes back is a reading and not a subscription. A caller that wants it later asks again.
   */
  read(): Row | null;
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
 * stands for two reasons and neither of them is silence** (`AMB-D-858`): the agent said so
 * (`waiting`), or the sentence Amenbo opened it with is still sitting in the input box. What an agent
 * has *not* said is not one of them: a pane that has gone quiet is a pane that has gone quiet.
 *
 * Nothing outside this row reads it any more — the badges and the dots that did have been taken away
 * (`AMB-D-862`) — and what is reported is the change rather than the statement: an agent at work says
 * a great deal and almost none of it moves the answer.
 */
export function mountPlate(
  host: HTMLElement,
  lang: () => Lang = currentLang,
  onWaiting: (waiting: boolean) => void = () => {},
  frame: string = ONLY_FRAME,
): Plate {
  const draw = mountNameplate(host);

  // What the pane's session has said. It is gone when the pane is: a session has no existence outside
  // the terminal it runs in (`AMB-D-749`).
  let sessions: Sessions = NO_SESSIONS;
  let names: FrameNames = new Map();
  // The folder this pane's terminal was started in. It is what the row is headed with until the frame
  // is named, and it is kept here rather than read back out of the arrangement: what the row says is
  // about the session in front of the reader, and a place is not one.
  let folder: string | null = null;
  let running: string | null = null;
  // Whether a terminal has ever run in this pane. It is not `running !== null` — a pane whose program
  // has exited still has a row, because what it just finished is the one thing worth saying at that
  // moment. What has no row is a pane that has never had a session: the face there is the invitation
  // to choose a folder (`./agent`), and a label about the session would be about nothing.
  let ran = false;
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
    const now = live && standsAsTurn(sayOf(running === null ? undefined : sessions.get(running)));
    if (now === waiting) return;
    waiting = now;
    onWaiting(now);
  }

  /**
   * What the end of the row says.
   *
   * The session's own word comes first — a turn standing — and how long it has been quiet fills the
   * slot only when that leaves it empty. A measurement of silence is the least of what can be said
   * about a pane, and it must never stand where something that was actually said would.
   *
   * It is said in the pane being worked in and nowhere else. Every pane on a screen has been quiet for
   * some length of time, and a row of clocks is what a reader stops reading.
   */
  function saying(): Say {
    const said = sayOf(running === null ? undefined : sessions.get(running));
    if (said.kind !== "silent" || !focused) return said;
    const minutes = quietFor(lastOutput, Date.now());
    return minutes === null ? said : { kind: "quiet", minutes };
  }

  /**
   * The row as it stands, or nothing where this pane has none.
   *
   * A frame that was named keeps its row whether or not anything has run in it: the name is the
   * person's, and it outlives every session the frame holds (`./frames`). A folder standing in for one
   * is not that — it is what this pane's terminal is working in, so it goes when the pane has never
   * had one.
   */
  function row(): Row | null {
    if (!(ran || names.has(frame))) return null;
    // The lamp is read off the same answer the row's right is, so the two cannot come to say different
    // things about the same pane (`./nameplate`).
    const say = saying();
    return { name: frameLabel(names, frame, folder), say, dot: { frame, face: faceOf(say, moving) } };
  }

  function redraw(): void {
    if (!live) return;
    draw(row(), lang());
  }

  void frameNames()
    .then((known) => {
      names = known;
      redraw();
    })
    .catch(() => {});

  redraw();

  return {
    opened: (session, startedAt, where, standing = null) => {
      sessions = opened(sessions, { session, startedAt, waiting: standing });
      running = session;
      folder = where;
      ran = true;
      tellWaiting();
      redraw();
    },
    output: tookOutput,
    said: (statement) => {
      sessions = said(sessions, statement);
      // The turn comes off the pane's own word about itself. The window used to keep one record of
      // every session's, so that the dots on the pages and this row were drawn from one answer and
      // could not disagree — there are no dots any more, and this row is the only reader left
      // (`AMB-D-862`).
      if (statement.verb === "waiting") {
        sessions = declared(sessions, statement.session, statement.text ?? null);
      } else if (statement.verb === "note" || statement.verb === "finished") {
        sessions = declared(sessions, statement.session, null);
      }
      tellWaiting();
      redraw();
    },
    unsent: (session) => {
      sessions = leftUnsent(sessions, session);
      tellWaiting();
      redraw();
    },
    sent: (session) => {
      sessions = wentOut(sessions, session);
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
    // A pane that has been taken down has no row to read: what it said was about a session that is
    // gone, and handing the last of it back would be this saying something it can no longer see.
    read: () => (live ? row() : null),
    stop: () => {
      live = false;
      tellWaiting();
      clearTimeout(settling);
      clearInterval(ticking);
      host.replaceChildren();
    },
  };
}
