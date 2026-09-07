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
import { frameLabel, frameNames, ONLY_FRAME, type FrameNames } from "./frames";
import { faceOf, mountNameplate, type Plate as Row } from "./nameplate";
import { movingAt, STILL_AFTER_MS } from "./moving";
import { closed, NO_SESSIONS, opened, said, type Sessions } from "./sessions";

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
  /** The program in the terminal has exited. */
  closed(session: string): void;
  /** The frames have been named afresh — what a naming answered with. */
  named(names: FrameNames): void;
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
 * `frame` is which of the arrangement's places this pane is in (`./layout`), because the name on the
 * row belongs to the place rather than to the session (`./frames`). A lone pane that has never been
 * told which place it is takes the first of them.
 */
export function mountPlate(host: HTMLElement, frame: string = ONLY_FRAME): Plate {
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
  // When something last came out of the terminal, and whether that still counts as moving — which is
  // the lamp's lit face (`./nameplate`). The time is written on every chunk and the row is only redrawn
  // when the answer turns over: a busy build prints hundreds of times a second, and a row redrawn with
  // each of them would spend the pane's frames on a mark that had not changed.
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
    return { name: frameLabel(names, frame, folder), dot: { frame, face: faceOf(moving) } };
  }

  function redraw(): void {
    if (!live) return;
    draw(row());
  }

  void frameNames()
    .then((known) => {
      names = known;
      redraw();
    })
    .catch(() => {});

  redraw();

  return {
    opened: (session, startedAt, where) => {
      sessions = opened(sessions, { session, startedAt });
      running = session;
      folder = where;
      ran = true;
      redraw();
    },
    output: tookOutput,
    said: (statement) => {
      sessions = said(sessions, statement);
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
      redraw();
    },
    named: (known) => {
      names = known;
      redraw();
    },
    // A pane that has been taken down has no row to read: what it said was about a session that is
    // gone, and handing the last of it back would be this saying something it can no longer see.
    read: () => (live ? row() : null),
    stop: () => {
      live = false;
      clearTimeout(settling);
      host.replaceChildren();
    },
  };
}
