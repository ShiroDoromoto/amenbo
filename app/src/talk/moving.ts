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
// **Nothing here moves.** Movement given to a mark on every pane would leave the whole screen in
// motion all day, which is what a mark this small is meant not to do (`../talk/nameplate.ts`). So
// what this contributes is a hue and a brightness.

/** How long after the last output a pane still reads as moving.
 *
 *  Long enough to bridge the gaps inside one piece of work — a compiler between files, an agent
 *  between tool calls — and short enough that a pane which has actually stopped settles while the
 *  reader is still looking at it. */
export const STILL_AFTER_MS = 1500;

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
 */
export function hueOf(frame: string): number {
  const n = Number(frame);
  return HUES[(Number.isFinite(n) ? Math.abs(Math.trunc(n)) : 0) % HUES.length]!;
}

/** Whether a pane counts as moving: something arrived, and not long enough ago to have settled. */
export function movingAt(lastOutput: number | null, at: number): boolean {
  return lastOutput !== null && at - lastOutput < STILL_AFTER_MS;
}
