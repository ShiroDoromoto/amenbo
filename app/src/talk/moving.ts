// The one thing a pane can say about itself without being told: that it is moving.
//
// **Silence has no single reading, but output is a fact** (`AMB-D-748`). A pane that has printed
// something in the last moment is running something; a pane that has not may be building, thinking or
// waiting on a person, and nothing here tries to tell those apart. So what is drawn is the fact and
// only the fact, and it is drawn small.
//
// **It is not a meter.** How much a program prints is a property of the program — a build log runs
// hot and an agent working carefully prints almost nothing — so a mark that followed the volume would
// make the noisiest pane look like the busiest. What is measured is whether anything arrived inside a
// fixed window, and what that turns into is a fixed rhythm: the same beat whether one line came or a
// thousand.
//
// **It is not a spinner either.** Something that turns says progress is being made, which is a claim
// about the work rather than about the stream. A pulse in the dot the pane is already marked with
// says the one thing that is true.

/** How long after the last output a pane still reads as moving.
 *
 *  Long enough to bridge the gaps inside one piece of work — a compiler between files, an agent
 *  between tool calls — and short enough that a pane which has actually stopped settles while the
 *  reader is still looking at it. */
export const STILL_AFTER_MS = 1500;

/** One turn of the pulse. Slow enough to read as breathing rather than blinking: what it reports is
 *  a state that lasts, and a fast mark reads as an event that just happened. */
export const PULSE_MS = 2000;

/** How many hues there are to tell panes apart with — a screenful, which is the most that are ever
 *  side by side (`./layout`). */
const HUES = [199, 152, 32, 280];

/**
 * The hue a frame's dot is drawn in.
 *
 * **Hue says which pane, never what is happening in it** — that is the opacity's, and the two must not
 * be read for each other. Frames on one page are consecutive, so taking the id in turn gives every
 * pane on a screen a different colour without anything having to know what else is on it.
 */
export function hueOf(frame: string): number {
  const n = Number(frame);
  return HUES[(Number.isFinite(n) ? Math.abs(Math.trunc(n)) : 0) % HUES.length]!;
}

/** How long a pane has to have been quiet before how long is worth saying.
 *
 *  Everything shorter is ordinary: a compiler between files, an agent between tool calls, a person
 *  reading what came back. What this is for is the pane that has been quiet long enough that a reader
 *  cannot tell any more whether they left it a minute ago or half an hour ago — and half an hour is
 *  what the dot alone cannot tell them. */
export const QUIET_AFTER_MS = 8 * 60 * 1000;

/**
 * How many whole minutes a pane has been quiet, where that is worth saying at all.
 *
 * **It is a measurement and not a reading of one** (`AMB-D-748`). Nothing here says the pane has
 * stopped, is stuck, or is waiting: the three reasons for silence are indistinguishable from outside
 * and this does not try. How long a thing has been true is a fact about the stream, the same kind of
 * fact as whether anything arrived at all — and it is the one a dot cannot carry, a dot being off in
 * exactly the same way after one minute and after forty.
 *
 * `null` where there is nothing to say: nothing has ever come out, or it came out recently enough that
 * how long is not yet a question.
 */
export function quietFor(lastOutput: number | null, at: number): number | null {
  if (lastOutput === null) return null;
  const since = at - lastOutput;
  return since < QUIET_AFTER_MS ? null : Math.floor(since / 60_000);
}

/** Whether a pane counts as moving: something arrived, and not long enough ago to have settled. */
export function movingAt(lastOutput: number | null, at: number): boolean {
  return lastOutput !== null && at - lastOutput < STILL_AFTER_MS;
}

/**
 * Where in the turn a dot starting now should begin, as a CSS `animation-delay`.
 *
 * **Four panes pulse together or they do not pulse at all.** Started when each began moving, they
 * would beat at four unrelated phases, and a screen of marks blinking past each other is a screen
 * nobody can look away from. Reading the offset off the clock rather than off the moment the dot
 * started is what puts them in step: every dot on every page is the same distance into the same turn,
 * whenever it joined.
 */
export function phaseDelay(at: number, period: number = PULSE_MS): string {
  return `-${at % period}ms`;
}
