// @vitest-environment jsdom
// What the columns beside the panes have to keep true: a width that survives a wider screen and an
// older build, a wish that is remembered, and the one rule about when a column stops being one.
import { beforeEach, describe, expect, it } from "vitest";
import {
  clampRailWidth, clampSideWidth, getRailShown, getRailWidth, getSideShown, getSideTab, getSideWidth,
  PANE_MIN, RAIL_DEFAULT, RAIL_MIN, setRailShown, setRailWidth, setSideShown, setSideTab,
  setSideWidth, sidesAreDrawers, SIDE_DEFAULT, SIDE_MIN,
} from "./columns";

beforeEach(() => {
  localStorage.clear();
});

describe("a column's width", () => {
  it("starts at the width the face shipped with, so nothing moves until somebody drags", () => {
    expect(getRailWidth()).toBe(RAIL_DEFAULT);
    expect(getSideWidth()).toBe(SIDE_DEFAULT);
  });

  it("comes back as it was left", () => {
    setRailWidth(240);
    setSideWidth(400);
    expect(getRailWidth()).toBe(240);
    expect(getSideWidth()).toBe(400);
  });

  it("never goes under the floor, however far the drag went", () => {
    expect(setRailWidth(0)).toBe(RAIL_MIN);
    expect(setSideWidth(-100)).toBe(SIDE_MIN);
  });

  it("is clamped on the way out too, so a width kept on a wider screen cannot come back whole", () => {
    // Written straight into storage, the way a run on a 4K display would have left it.
    localStorage.setItem("amenbo.termface.sideWidth", "3000");
    expect(getSideWidth()).toBeLessThanOrEqual(clampSideWidth(3000));
    expect(getSideWidth()).toBeLessThan(3000);
  });

  it("falls back where what was kept is not a width at all", () => {
    localStorage.setItem("amenbo.termface.railWidth", "wide");
    expect(getRailWidth()).toBe(RAIL_DEFAULT);
  });

  it("holds the floor even on a window with no room for it", () => {
    expect(clampRailWidth(RAIL_MIN)).toBe(RAIL_MIN);
  });
});

describe("whether a column was asked for", () => {
  it("is both of them on a device that has never said otherwise", () => {
    expect(getRailShown()).toBe(true);
    expect(getSideShown()).toBe(true);
  });

  it("is remembered once one is closed, and again once it is opened", () => {
    setRailShown(false);
    setSideShown(false);
    expect(getRailShown()).toBe(false);
    expect(getSideShown()).toBe(false);
    setRailShown(true);
    expect(getRailShown()).toBe(true);
  });
});

describe("which half of the file face is up", () => {
  it("is the memo on a device that has never said otherwise", () => {
    expect(getSideTab()).toBe("memo");
  });

  it("comes back as it was left, so the default is only ever the first run's", () => {
    expect(setSideTab("files")).toBe("files");
    expect(getSideTab()).toBe("files");
    setSideTab("memo");
    expect(getSideTab()).toBe("memo");
  });

  it("reads as the memo where what was kept is not one of the two", () => {
    // An older build, or a store edited by hand: a word that is not an answer is not one.
    localStorage.setItem("amenbo.termface.sideTab", "tree");
    expect(getSideTab()).toBe("memo");
  });
});

describe("the sides", () => {
  it("stay columns while a pane's worth of floor is left in the middle", () => {
    const room = PANE_MIN + RAIL_DEFAULT + SIDE_DEFAULT;
    expect(sidesAreDrawers(room, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(false);
    expect(sidesAreDrawers(room - 1, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
  });

  it("answer the same on one window whatever count was asked for", () => {
    // The count is not in it, so nothing about it can flip the answer. A window that suits two
    // across keeps its columns at eight, where the panes are cramped — that is the choice of
    // whoever pressed for eight, and taking the rail away would not undo it.
    const room = 2 * PANE_MIN + RAIL_DEFAULT + SIDE_DEFAULT;
    expect(sidesAreDrawers(room, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(false);
    // And a window with no floor left folds them however few panes were asked for.
    const tight = PANE_MIN + RAIL_DEFAULT + SIDE_DEFAULT - 1;
    expect(sidesAreDrawers(tight, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
  });

  it("are columns again once a closed one stops taking width", () => {
    const tight = PANE_MIN + SIDE_DEFAULT;
    expect(sidesAreDrawers(tight, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
    // The rail closed: what it would have taken is room the panes now have.
    expect(sidesAreDrawers(tight, 0, SIDE_DEFAULT)).toBe(false);
  });
});
