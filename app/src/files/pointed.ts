// What an agent pointed at, kept for as long as the window is up (`AMB-D-749`).
//
// `point` is the one verb of the surface layer that leaves something to come back to: a file, a
// task, a decision, a URL, and the reason it is worth opening. Nothing here is written down — a
// session has no existence outside the rectangle it runs in, and neither has what was said in one —
// so this is a map held in the window, keyed by the session that said it.
//
// **It is keyed by session because the row that draws it follows the focused pane** (`AMB-T-3603`).
// The rows below it belong to the project and do not move when the pane does; this one is the pane's
// own, and mixing the two would leave a reader unable to say what just happened where.
//
// **Read means opened.** Nothing here guesses at attention: a row counts as read when somebody
// clicked it, which is the only reading of "read" this side can honestly make. What that count is
// for is the one line said when the session ends — during the run the count is all that is shown,
// because an agent at work is not the moment to interrupt.
import type { SessionSaidDto } from "../bindings/bindings";

/** One thing an agent pointed at. */
export type Pointed = {
  /** When it was said (RFC3339 UTC) — also what identifies it, per session. */
  at: string;
  /** What was pointed at, as it was typed: a path, a ref, a URL. */
  target: string;
  /** Why it is worth opening. */
  why: string;
  /** The folder the agent was in when it said so, which is what a relative path is read against. */
  cwd: string | null;
  /** Whether somebody has opened it. */
  read: boolean;
};

/** What every session has pointed at, newest first. */
export type PointedBySession = ReadonlyMap<string, Pointed[]>;

/**
 * Take one statement. Anything that is not a `point` is not this module's business, and a `point`
 * with nothing to point at is dropped rather than drawn as a row that opens nothing.
 */
export function tookPoint(was: PointedBySession, said: SessionSaidDto): PointedBySession {
  if (said.verb !== "point" || !said.target) return was;
  const next = new Map(was);
  const one: Pointed = {
    at: said.at,
    target: said.target,
    why: said.why ?? "",
    cwd: said.cwd ?? null,
    read: false,
  };
  next.set(said.session, [one, ...(was.get(said.session) ?? [])]);
  return next;
}

/** Mark one row opened. */
export function markRead(was: PointedBySession, session: string, at: string): PointedBySession {
  const list = was.get(session);
  if (!list) return was;
  const next = new Map(was);
  next.set(session, list.map((one) => (one.at === at ? { ...one, read: true } : one)));
  return next;
}

/** How many of them nobody has opened. */
export function unread(list: readonly Pointed[]): number {
  return list.filter((one) => !one.read).length;
}

/**
 * The newest thing each session pointed at that the person has been shown — what the badge on the
 * files switch is read off (`../shell/TerminalFace`).
 *
 * **It is not `read`.** `read` is per row and means somebody clicked it, which is what the count
 * inside the panel is about. This is per session and means the person has been on the half at all,
 * which is what a knock is about: the badge answers "something came up while you were somewhere
 * else", not "something is still standing over there" — the same rule, for the same reason, as the
 * badge the terminal segment wears (`../shell/terminalBadge`). A badge that came back every time the
 * panel was closed would be up from the first `point` to the end of the run, and one that is always
 * up says nothing.
 *
 * What is held is the newest `at` rather than a flag, because a flag cannot tell a point that has
 * arrived since from the one already shown.
 */
export type ShownBySession = ReadonlyMap<string, string>;

/** The newest thing a session pointed at — the first of them — or nothing where it pointed at none. */
export function newestPoint(list: readonly Pointed[]): string | null {
  return list[0]?.at ?? null;
}

/** Whether something this session pointed at is waiting to be shown. */
export function pointWaits(shown: ShownBySession, session: string, newest: string | null): boolean {
  return newest !== null && shown.get(session) !== newest;
}

/** Take everything this session has pointed at so far as shown, `newest` being the last of it. */
export function tookShown(
  was: ShownBySession, session: string, newest: string,
): ShownBySession {
  if (was.get(session) === newest) return was;
  return new Map(was).set(session, newest);
}

/**
 * The file a target names inside the project's folder, as segments from it — or nothing, where it
 * names something else.
 *
 * A target is written by an agent typing at a shell, so it arrives in every shape a path comes in:
 * absolute, relative to wherever that agent had got to, with `./` in front of it. What it must not
 * do is reach out of the folder the face is rooted at, and that is settled here as well as at the
 * host's own fence — not because this side is trusted, but because a row that cannot be opened
 * should not be drawn as one that can (`AMB-D-747`).
 */
export function fileUnder(root: string, cwd: string | null, target: string): string[] | null {
  if (target === "" || isRef(target) || isUrl(target)) return null;
  const sep = root.includes("\\") && !root.includes("/") ? "\\" : "/";
  const parts = (path: string) => path.split(/[\\/]+/).filter((p) => p !== "" && p !== ".");
  const absolute = /^([a-zA-Z]:[\\/]|[\\/])/.test(target);
  const from = absolute ? "" : (cwd ?? root);
  const whole = absolute ? target : `${from}${sep}${target}`;

  const wanted: string[] = [];
  for (const part of parts(whole)) {
    if (part === "..") {
      if (wanted.pop() === undefined) return null;
    } else {
      wanted.push(part);
    }
  }
  const under = parts(root);
  // Inside the folder, and not merely starting with the same letters: `/work/repo-2` is not in
  // `/work/repo`, which comparing the strings would say it was.
  if (wanted.length <= under.length) return null;
  if (!under.every((part, i) => part === wanted[i])) return null;
  return wanted.slice(under.length);
}

/** Whether the target is one of Amenbo's own records rather than a place on the disk. */
export function isRef(target: string): boolean {
  return /^AMB-[TD]-\d+$/i.test(target.trim());
}

/** Whether the target is somewhere on the web. */
export function isUrl(target: string): boolean {
  return /^https?:\/\//i.test(target.trim());
}
