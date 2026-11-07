import { describe, expect, it } from "vitest";
import { hueOf, movingAt, QUIET_AFTER_MS, quietAt, STILL_AFTER_MS } from "./moving";

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
    const screenful = [0, 1, 2, 3].map(hueOf);
    expect(new Set(screenful).size).toBe(4);
  });

  it("keeps a slot's colour whatever is running in it — it belongs to the place", () => {
    expect(hueOf(2)).toBe(hueOf(2));
  });

  it("goes round for a page deeper than there are hues, rather than drawing nothing", () => {
    expect(hueOf(4)).toBe(hueOf(0));
    expect(Number.isFinite(hueOf(-1))).toBe(true);
  });
});

describe("how far a silence has gone", () => {
  const at = 100_000_000;
  const min = 60_000;

  it("is nothing before anything has arrived", () => {
    expect(quietAt(null, at, 10)).toBe(0);
  });

  it("is nothing until the terminal has been quiet for the first stage's time", () => {
    expect(quietAt(at - QUIET_AFTER_MS + 1, at, 10)).toBe(0);
    expect(quietAt(at - QUIET_AFTER_MS, at, 10)).toBe(1);
  });

  it("reaches the second stage at twice the time the step is expected to take", () => {
    expect(quietAt(at - 20 * min + 1, at, 10)).toBe(1);
    expect(quietAt(at - 20 * min, at, 10)).toBe(2);
  });

  it("never reaches the second stage before the first, however short the step is expected to be", () => {
    expect(quietAt(at - 2 * min, at, 1)).toBe(0);
    expect(quietAt(at - QUIET_AFTER_MS, at, 1)).toBe(2);
  });

  it("stays at the first stage where the step has no expected time", () => {
    expect(quietAt(at - 600 * min, at, null)).toBe(1);
  });
});
