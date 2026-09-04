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
// about the work rather than about the stream. What it turns into is a glow held still in the dot the
// pane is already marked with, which says the one thing that is true.
//
// **Nothing here moves.** The dot has one face that does — the one saying a person is needed — and
// giving movement to the commonest state instead would leave every pane on the screen in motion all
// day, which is the whole of what the mark was meant not to do (`../talk/nameplate.ts`). So this
// contributes a hue and a brightness, and the rhythm below belongs to the turn.

/** How long after the last output a pane still reads as moving.
 *
 *  Long enough to bridge the gaps inside one piece of work — a compiler between files, an agent
 *  between tool calls — and short enough that a pane which has actually stopped settles while the
 *  reader is still looking at it. */
export const STILL_AFTER_MS = 1500;

/** One turn of the blink — the lamp's calling face, and the mark on the other end of the row that
 *  says the same thing (`../talk/nameplate.ts`).
 *
 *  It is the two together that fix the number: they are one signal drawn twice, so a dot falling to
 *  one beat while the mark beside it falls to another would read as two things being asked. Fast
 *  enough to read as a call rather than as breathing — what it reports is a person being needed now,
 *  which is an event and not a state that lasts. */
export const BLINK_MS = 900;

/** How many hues there are to tell panes apart with — a screenful, which is the most that are ever
 *  side by side (`./layout`). */
const HUES = [199, 152, 32, 280];

/**
 * The hue a frame's dot is drawn in.
 *
 * **Hue says which pane, never what is happening in it** — that is the glow's, and the two must not
 * be read for each other. Frames on one page are consecutive, so taking the id in turn gives every
 * pane on a screen a different colour without anything having to know what else is on it.
 *
 * The one face that leaves the hue behind is the calling one, which is drawn in the warning colour
 * because it is no longer saying which pane this is — it is saying come here (`../talk/nameplate.ts`).
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
 * Where in the turn a mark starting now should begin, as a CSS `animation-delay`.
 *
 * **Panes blink together or they do not blink at all.** Started when each turn was handed over, they
 * would beat at unrelated phases, and a screen of marks blinking past each other is a screen nobody
 * can look away from. Reading the offset off the clock rather than off the moment the blink started
 * is what puts them in step: every mark on every page is the same distance into the same turn,
 * whenever it joined — the dot and the mark at the other end of its own row included.
 */
export function phaseDelay(at: number, period: number = BLINK_MS): string {
  return `-${at % period}ms`;
}
