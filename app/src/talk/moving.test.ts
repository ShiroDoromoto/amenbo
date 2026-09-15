import { describe, expect, it } from "vitest";
import { hueOf, movingAt, STILL_AFTER_MS } from "./moving";

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
