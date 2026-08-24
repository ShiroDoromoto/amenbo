import { describe, expect, it } from "vitest";
import { hueOf, movingAt, phaseDelay, BLINK_MS, QUIET_AFTER_MS, quietFor, STILL_AFTER_MS } from "./moving";

describe("what counts as moving", () => {
  it("is nothing at all before anything has arrived", () => {
    expect(movingAt(null, 1_000_000)).toBe(false);
  });

  it("bridges the gaps inside one piece of work", () => {
    const at = 1_000_000;
    expect(movingAt(at - STILL_AFTER_MS + 1, at)).toBe(true);
  });

  it("settles once the window has passed, rather than waiting for something to say so", () => {
    const at = 1_000_000;
    expect(movingAt(at - STILL_AFTER_MS, at)).toBe(false);
  });
});

describe("the hue is which pane, not what is happening in it", () => {
  it("gives every pane of a full screen a colour of its own", () => {
    const screenful = ["1", "2", "3", "4"].map(hueOf);
    expect(new Set(screenful).size).toBe(4);
  });

  it("keeps a frame's colour whatever is running in it — it belongs to the place", () => {
    expect(hueOf("3")).toBe(hueOf("3"));
  });

  it("answers for a frame id that is not a number rather than drawing nothing", () => {
    expect(Number.isFinite(hueOf("not a number"))).toBe(true);
  });
});

describe("the phase", () => {
  it("puts two marks that started at different moments at the same point in the turn", () => {
    const early = 5_000_000;
    const late = early + BLINK_MS * 3;
    expect(phaseDelay(late)).toBe(phaseDelay(early));
  });

  it("reads off the clock, so a mark joining mid-turn joins where the others are", () => {
    expect(phaseDelay(BLINK_MS + 250)).toBe("-250ms");
  });
});

// How long a pane has been quiet is a measurement, and the row says it only where it is worth saying.
// What is pinned here is the silence of the ordinary case: the gaps inside a piece of work must not
// turn into a number on the screen, because a number that is always there is one nobody reads.
describe("how long a pane has been quiet", () => {
  const at = 100_000_000;

  it("is nothing before anything has come out", () => {
    expect(quietFor(null, at)).toBeNull();
  });

  it("is nothing while the silence is still an ordinary one", () => {
    expect(quietFor(at - 1, at), "a pane that just printed").toBeNull();
    expect(quietFor(at - (QUIET_AFTER_MS - 1), at), "just under the line").toBeNull();
  });

  it("is whole minutes once it is worth saying", () => {
    expect(quietFor(at - QUIET_AFTER_MS, at)).toBe(8);
    // Whole minutes: a reader is being told roughly how long, and a number that moved every second
    // would be a clock rather than an answer.
    expect(quietFor(at - (12 * 60_000 + 59_000), at)).toBe(12);
  });
});
