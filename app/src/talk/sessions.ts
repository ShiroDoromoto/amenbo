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
//
// A pane taking up a session that was already running is handed what the host kept of it — the turn
// standing in it (`AMB-D-860`). That is the same second source read back rather than a third one: the
// host holds it because it heard the agent say it, and a pane comes and goes while the session does
// not.

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
  /** Why a person's turn has come, said by the agent — the one thing nothing can find out by watching
   *  (`AMB-D-748`). Null where none is standing. It is written by `declared`, off what the agent said
   *  in this pane (`./plate`). */
  readonly waiting: string | null;
  /** Whether the sentence Amenbo opens an agent with is sitting in this pane's input box, unsent.
   *  It is the window's own doing and not a guess: the host handed the sentence over itself and says
   *  how that ended (`crate::pty`). */
  readonly unsent: boolean;
};

export type Sessions = ReadonlyMap<string, Session>;

/** What the window knows when it starts a session itself. */
export type Opened = {
  session: string;
  startedAt: string;
  folder?: string | null;
  project?: number | null;
  agent?: string | null;
  /** A turn already standing in this session, as the host holds it (`crate::pty::Pane`). It is here
   *  for the pane that comes **back** up: a session outlives the pane drawing it, so one that handed
   *  its turn over while the reader was on another page still has it when they turn back
   *  (`AMB-D-860`). A terminal this pane just started has none. */
  waiting?: string | null;
};

export const NO_SESSIONS: Sessions = new Map<string, Session>();

/** The map with one entry replaced — every change here is a new map, so a render is a comparison. */
function withEntry(sessions: Sessions, entry: Session): Sessions {
  const next = new Map(sessions);
  next.set(entry.session, entry);
  return next;
}

/** Record a session this pane has just put a terminal in — one it started, or one it took up that was
 *  already running. What it was started with is known exactly, and a turn already standing in it comes
 *  from the host along with that (`crate::pty::pty_sessions`); the rest waits to be said. Re-opening an
 *  id that is already there replaces it: an id is drawn fresh per terminal, so the same one twice is
 *  the same session being described again. */
export function opened(sessions: Sessions, open: Opened): Sessions {
  return withEntry(sessions, {
    session: open.session,
    folder: open.folder ?? null,
    project: open.project ?? null,
    agent: open.agent ?? null,
    startedAt: open.startedAt,
    waiting: open.waiting ?? null,
    unsent: false,
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
 * **Whose turn it is is not read off this.** A turn goes up by a word and comes down by a person
 * arriving, and the host writes both (`crate::pty::Pane`, `AMB-D-859`); what a statement moves here
 * is only where the agent is working. The turn arrives by `declared`, which is that answer written
 * down.
 *
 * **A verb this does not know is passed over**, and so is one it does. The vocabulary has shrunk
 * before and will again, and a CLI from before a word went away still posts it into a newer window's
 * drop box. */
export function said(sessions: Sessions, statement: SessionSaidDto): Sessions {
  const known = sessions.get(statement.session);
  const entry: Session = known ?? {
    session: statement.session,
    folder: null,
    project: null,
    agent: null,
    startedAt: statement.at,
    waiting: null,
    unsent: false,
  };
  // The folder moves with the agent, so the newest statement's is the current one.
  const folder = statement.cwd ?? entry.folder;
  // **An agent that has spoken at all has the sentence.** Every verb of this layer is a word of
  // Amenbo's own, said by running Amenbo's command in this pane — so a statement of any kind, a name
  // as much as a turn, is an agent that knows where it is working. That is the one thing a sentence
  // left in the input box says is missing, and it is taken back by the agent working rather than by a
  // word for taking it back.
  return withEntry(sessions, { ...entry, folder, unsent: false });
}

/**
 * Write down what is standing in this session: why a person's turn has come, or `null` where none is.
 *
 * **It is written and never worked out.** What the caller hands over is the agent's own word about
 * itself, and nothing here decides whether a turn is still standing from anything else.
 */
export function declared(sessions: Sessions, session: string, waiting: string | null): Sessions {
  const known = sessions.get(session);
  if (!known || known.waiting === waiting) return sessions;
  return withEntry(sessions, { ...known, waiting });
}

/**
 * Record that this pane's opening sentence was left in its input box, unsent (`crate::pty`).
 *
 * It is written only for a session the window is holding. Unlike a statement, this cannot be the
 * first thing heard about one: the host gives up on the hand-over only after a minute of looking at
 * the pane, and the pane it is about was registered before the first of those looks.
 */
export function unsent(sessions: Sessions, session: string): Sessions {
  const known = sessions.get(session);
  return known ? withEntry(sessions, { ...known, unsent: true }) : sessions;
}

/**
 * Record that the sentence has gone out of this pane's input box (`crate::pty`).
 *
 * **It is the box the notice was about, and the box is empty now.** What was owed the reader while it
 * sat there was that it was theirs to send; a row still saying so after the sending would point them
 * at a keypress that does nothing.
 *
 * It is not the same news as the agent having read it — that is settled by the agent saying so
 * (`AMB-D-805`), and a pane can send the sentence to a program that never runs the command. Which is
 * why this is written on the sending rather than waited for: the word that would take the notice back
 * may never be said, and the notice would stand for the life of the pane.
 */
export function sent(sessions: Sessions, session: string): Sessions {
  const known = sessions.get(session);
  return known ? withEntry(sessions, { ...known, unsent: false }) : sessions;
}

/** Forget a session whose terminal has closed. **Nothing running is kept**: the process is gone, and a
 *  record of it left behind would have the window describing something that is not there. */
export function closed(sessions: Sessions, session: string): Sessions {
  if (!sessions.has(session)) return sessions;
  const next = new Map(sessions);
  next.delete(session);
  return next;
}
