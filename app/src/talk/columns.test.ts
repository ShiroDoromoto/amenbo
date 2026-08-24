// @vitest-environment jsdom
// What the columns beside the panes have to keep true: a width that survives a wider screen and an
// older build, a wish that is remembered, and the one rule about when a column stops being one.
import { beforeEach, describe, expect, it } from "vitest";
import {
  clampRailWidth, clampSideWidth, getRailShown, getRailWidth, getSideShown, getSideWidth, PANE_MIN,
  RAIL_DEFAULT, RAIL_MIN, setRailShown, setRailWidth, setSideShown, setSideWidth, sidesAreDrawers,
  SIDE_DEFAULT, SIDE_MIN,
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

describe("the sides", () => {
  it("stay columns when one pane was asked for on a wide window", () => {
    // Asking for one pane is not asking for the rail to go away.
    expect(sidesAreDrawers(1, 2560, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(false);
  });

  it("are drawers where keeping them would take the panes under the least a terminal is worth", () => {
    expect(sidesAreDrawers(2, 2 * PANE_MIN + 100, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
    expect(sidesAreDrawers(4, 900, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
  });

  it("counts four panes as two across, which is what has to fit", () => {
    const room = 2 * PANE_MIN + RAIL_DEFAULT + SIDE_DEFAULT;
    expect(sidesAreDrawers(4, room, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(false);
    expect(sidesAreDrawers(2, room, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(false);
  });

  it("asks for as much width as the count puts across, all the way up", () => {
    // The two counts that go past a square: six is three across and eight is four, so each wants
    // that much and no more (`./layout`). A window with room for exactly the wider one keeps its
    // columns at both.
    const room = (across: number) => across * PANE_MIN + RAIL_DEFAULT + SIDE_DEFAULT;
    expect(sidesAreDrawers(6, room(3), RAIL_DEFAULT, SIDE_DEFAULT)).toBe(false);
    expect(sidesAreDrawers(6, room(3) - 1, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
    expect(sidesAreDrawers(8, room(4), RAIL_DEFAULT, SIDE_DEFAULT)).toBe(false);
    expect(sidesAreDrawers(8, room(4) - 1, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
    // Eight wants more than six does: a window that suits three across is a drawer at four.
    expect(sidesAreDrawers(8, room(3), RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
  });

  it("are columns again once a closed one stops taking width", () => {
    const tight = 2 * PANE_MIN + SIDE_DEFAULT;
    expect(sidesAreDrawers(2, tight, RAIL_DEFAULT, SIDE_DEFAULT)).toBe(true);
    // The rail closed: what it would have taken is room the panes now have.
    expect(sidesAreDrawers(2, tight, 0, SIDE_DEFAULT)).toBe(false);
  });
});
