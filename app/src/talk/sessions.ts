// What the talk window knows about the sessions running in it, while they are running.
//
// A session is a process in a pane, and this is the whole of what is held about one. It lives in the
// window's memory and nowhere else: a session has no existence outside the terminal it runs in
// (`AMB-D-749`), so a record of one that outlived the window would describe something that is not
// there. What *is* kept between runs is the name of the frame, which belongs to the frame and not to
// this (`./frames`).
//
// **Nothing here is guessed.** Two things grow the map and both are certain: the window started the
// session itself, so it knows what it started and where; or a write came past carrying the session's
// name on it — a statement the agent left in its drop box — which says what it says about the session
// it names. There is no confidence, no likelihood and no inference, because the one attempt to infer a
// session from its folder and its time was right in none of fifteen cases (`AMB-T-3549`). Where neither
// source has spoken, the field is null and the window says nothing.

import type { SessionSaidDto } from "../bindings/bindings";

/** One running session, as the window knows it. */
export type Session = {
  /** The pane's id, as the host named it when it opened the terminal. */
  readonly session: string;
  /** The folder the agent is in. It starts as the one the terminal was opened in and moves with the
   *  agent's own `cd`, which is what a statement carries. */
  readonly folder: string | null;
  /** The project that folder is bound to, where the session was started in one. */
  readonly project: number | null;
  /** Which agent was started, where the window started it. */
  readonly agent: string | null;
  /** When the session began (RFC3339 UTC). */
  readonly startedAt: string;
  /** The last line the agent said about what it is doing. */
  readonly note: string | null;
  /** Why a person's turn has come, said by the agent — the one thing nothing can find out by watching
   *  (`AMB-D-748`). Null once the agent goes back to work. */
  readonly waiting: string | null;
  /** When a person last looked at this pane (RFC3339 UTC), or null if they have not since it spoke. */
  readonly seen: string | null;
};

export type Sessions = ReadonlyMap<string, Session>;

/** What the window knows when it starts a session itself. */
export type Opened = {
  session: string;
  startedAt: string;
  folder?: string | null;
  project?: number | null;
  agent?: string | null;
};

export const NO_SESSIONS: Sessions = new Map<string, Session>();

/** The map with one entry replaced — every change here is a new map, so a render is a comparison. */
function withEntry(sessions: Sessions, entry: Session): Sessions {
  const next = new Map(sessions);
  next.set(entry.session, entry);
  return next;
}

/** Record a session the window has just started. What it was started with is known exactly; the rest
 *  waits to be said. Re-opening an id that is already there replaces it: an id is drawn fresh per
 *  terminal, so the same one twice is the same session being described again. */
export function opened(sessions: Sessions, open: Opened): Sessions {
  return withEntry(sessions, {
    session: open.session,
    folder: open.folder ?? null,
    project: open.project ?? null,
    agent: open.agent ?? null,
    startedAt: open.startedAt,
    note: null,
    waiting: null,
    seen: null,
  });
}

/** Take in one statement an agent made about its session.
 *
 * A statement names its session, so it can be the first thing the window hears about one — it arrives
 * from the host the moment it is written, which can be before the pane that opened the terminal has
 * finished registering it. That is why an unknown session is recorded rather than dropped.
 *
 * `name` is not here: a name belongs to the frame, not to the session running in it (`./frames`).
 *
 * **A turn is taken back by working, not by a word for taking it back.** There is no way to say "never
 * mind" — the layer has no such verb — because an agent that has stopped waiting is an agent that has
 * gone back to doing something, and saying what it is doing is a word it already has. Two things end a
 * turn: a note, and the work being finished. A third word would be one an agent could forget while
 * still remembering to say the other two, and a turn nobody took back is the one thing this must never
 * leave standing. */
export function said(sessions: Sessions, statement: SessionSaidDto): Sessions {
  const known = sessions.get(statement.session);
  const entry: Session = known ?? {
    session: statement.session,
    folder: null,
    project: null,
    agent: null,
    startedAt: statement.at,
    note: null,
    waiting: null,
    seen: null,
  };
  // The folder moves with the agent, so the newest statement's is the current one.
  const folder = statement.cwd ?? entry.folder;
  switch (statement.verb) {
    case "note":
      // Saying what it is doing now is the agent back at work: whatever it was waiting for, it is not
      // waiting any more. Nothing else is read into a note.
      return withEntry(sessions, { ...entry, folder, note: statement.text ?? null, waiting: null });
    case "waiting":
      // A turn nobody has looked at yet: what `seen` answers is whether the person has been back since
      // the pane last spoke, so a new turn puts that question back.
      return withEntry(sessions, { ...entry, folder, waiting: statement.text ?? null, seen: null });
    case "finished":
      // What came of the work is the last thing the session has to say about what it is doing, and it
      // is nobody's turn any more.
      return withEntry(sessions, { ...entry, folder, note: statement.text ?? null, waiting: null });
    default:
      // `name` moves the frame's name and `point` fills the window's own list — neither is anything
      // about the session, but both say where the agent is.
      return withEntry(sessions, { ...entry, folder });
  }
}

/** Record that a person has looked at this pane. */
export function seen(sessions: Sessions, session: string, at: string): Sessions {
  const known = sessions.get(session);
  return known ? withEntry(sessions, { ...known, seen: at }) : sessions;
}

/** Forget a session whose terminal has closed. **Nothing running is kept**: the process is gone, and a
 *  record of it left behind would have the window describing something that is not there. */
export function closed(sessions: Sessions, session: string): Sessions {
  if (!sessions.has(session)) return sessions;
  const next = new Map(sessions);
  next.delete(session);
  return next;
}
