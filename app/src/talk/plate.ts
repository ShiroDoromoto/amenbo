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
  type Changeover,
  type Held,
} from "./nameplate";
import { closed, NO_SESSIONS, opened, said, type Sessions } from "./sessions";

/** A pane's label, and the pane's way of telling it what happened. */
export type Plate = {
  /** A terminal has started in the pane, under this session id. */
  opened(session: string, startedAt: string): void;
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
 */
export function mountPlate(
  host: HTMLElement,
  lang: () => Lang = currentLang,
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
  let expiry: ReturnType<typeof setTimeout> | undefined;
  let live = true;

  function redraw(): void {
    if (!live) return;
    const { now, changeover: next } = nowOf(held, finished, changeover, Date.now());
    changeover = next;
    draw(
      {
        name: names.get(frame) ?? null,
        now,
        say: sayOf(held, running === null ? undefined : sessions.get(running)),
      },
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
      readWork();
    },
    said: (statement) => {
      sessions = said(sessions, statement);
      redraw();
    },
    closed: (session) => {
      sessions = closed(sessions, session);
      if (running === session) running = null;
      held = [];
      redraw();
    },
    named: (known) => {
      names = known;
      redraw();
    },
    stop: () => {
      live = false;
      clearTimeout(expiry);
      unlisten?.();
      host.replaceChildren();
    },
  };
}
