// @vitest-environment jsdom
// What the columns beside the panes have to keep true: a width that survives a wider screen and an
// older build, a wish that is remembered, and a ceiling that leaves the middle its floor.
import { beforeEach, describe, expect, it } from "vitest";
import {
  clampRailWidth, clampSideWidth, getRailShown, getRailWidth, getSideShown, getSideTab, getSideWidth,
  PANE_MIN, RAIL_DEFAULT, RAIL_MIN, setRailShown, setRailWidth, setSideShown, setSideTab,
  setSideWidth, railMax, sideMax, SIDE_DEFAULT, SIDE_MIN,
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

describe("a column's ceiling", () => {
  // The room, not a share of the window. 0.3 and 0.4 of the narrowest window the application opens
  // leave 288px in the middle — under the floor a pane is drawn at.
  it("is what the window has left once the other column and a pane's floor are out of it", () => {
    window.innerWidth = 960;
    expect(railMax(SIDE_DEFAULT)).toBe(960 - SIDE_DEFAULT - PANE_MIN);
    expect(sideMax(RAIL_DEFAULT)).toBe(960 - RAIL_DEFAULT - PANE_MIN);
  });

  // Closing a column is what makes room, so what it is taking is nought and the other may have it.
  it("grows by what a column the person closed is no longer taking", () => {
    window.innerWidth = 960;
    expect(sideMax(0)).toBe(960 - PANE_MIN);
  });

  // The middle is what the ceiling is for: dragged to it, a pane still has its floor.
  it("leaves the middle its floor when both columns are dragged out to it", () => {
    window.innerWidth = 960;
    const side = clampSideWidth(9999, RAIL_DEFAULT);
    const rail = clampRailWidth(9999, side);
    expect(960 - rail - side).toBeGreaterThanOrEqual(PANE_MIN);
  });

  // A floor is a floor. On a window too narrow for one there is nothing to be done by making the
  // column narrower than it is drawable at.
  it("never falls below the column's own floor", () => {
    window.innerWidth = 400;
    expect(railMax(SIDE_DEFAULT)).toBe(RAIL_MIN);
    expect(sideMax(RAIL_DEFAULT)).toBe(SIDE_MIN);
  });
});
