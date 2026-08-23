import { describe, expect, it } from "vitest";
import { hueOf, movingAt, phaseDelay, PULSE_MS, STILL_AFTER_MS } from "./moving";

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
  it("puts two dots that started at different moments at the same point in the turn", () => {
    const early = 5_000_000;
    const late = early + PULSE_MS * 3;
    expect(phaseDelay(late)).toBe(phaseDelay(early));
  });

  it("reads off the clock, so a dot joining mid-turn joins where the others are", () => {
    expect(phaseDelay(PULSE_MS + 250)).toBe("-250ms");
  });
});
