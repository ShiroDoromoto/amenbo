// @vitest-environment jsdom
// What the columns beside the panes have to keep true: a width that survives a wider screen and an
// older build, a wish that is remembered, a width that belongs to the project it was dragged in
// rather than to the device, and a ceiling that leaves the middle its floor.
import { beforeEach, describe, expect, it } from "vitest";
import {
  clampRailWidth, clampSideNarrow, clampSideWide, clampTabsWidth, getRailShown, getRailWidth,
  getSideNarrow, getSideShown, getSideTab, getSideWide, getTabsCompact, getTabsWidth, PANE_MIN,
  RAIL_DEFAULT, RAIL_MIN, setRailShown, setRailWidth, setSideNarrow, setSideShown, setSideTab,
  setSideWide, setTabsCompact, setTabsWidth, tabsMax, tabsWidth, TABS_COMPACT_WIDTH, TABS_DEFAULT,
  TABS_MIN, railMax, sideNarrowMax, sideWideMax, SIDE_MIN, SIDE_NARROW_DEFAULT, SIDE_WIDE_DEFAULT,
} from "./columns";

/** What the tab column is taking while nothing has been folded or dragged — it comes off the window
 *  before any of these are measured, so every ceiling below is written against it. */
const TABS = TABS_DEFAULT;

/** Two projects, because most of what is kept here is kept for one of them and not the other. */
const ONE = 1;
const TWO = 2;

beforeEach(() => {
  localStorage.clear();
});

describe("a column's width", () => {
  it("starts at the width the face shipped with, so nothing moves until somebody drags", () => {
    expect(getRailWidth(ONE)).toBe(RAIL_DEFAULT);
    expect(getSideNarrow(ONE)).toBe(SIDE_NARROW_DEFAULT);
  });

  it("comes back as it was left", () => {
    setRailWidth(ONE, 240);
    setSideNarrow(ONE, 400);
    expect(getRailWidth(ONE)).toBe(240);
    expect(getSideNarrow(ONE)).toBe(400);
  });

  it("never goes under the floor, however far the drag went", () => {
    expect(setRailWidth(ONE, 0)).toBe(RAIL_MIN);
    expect(setSideNarrow(ONE, -100)).toBe(SIDE_MIN);
  });

  it("is clamped on the way out too, so a width kept on a wider screen cannot come back whole", () => {
    // Written straight into storage, the way a run on a 4K display would have left it.
    localStorage.setItem("amenbo.termface.sideNarrow.1", "3000");
    expect(getSideNarrow(ONE)).toBeLessThanOrEqual(clampSideNarrow(3000));
    expect(getSideNarrow(ONE)).toBeLessThan(3000);
  });

  it("falls back where what was kept is not a width at all", () => {
    localStorage.setItem("amenbo.termface.railWidth.1", "wide");
    expect(getRailWidth(ONE)).toBe(RAIL_DEFAULT);
  });

  it("holds the floor even on a window with no room for it", () => {
    expect(clampRailWidth(RAIL_MIN)).toBe(RAIL_MIN);
  });
});

describe("a width kept for the project it was dragged in", () => {
  // The number of panes and the amount there is to read are not the same from one project to the
  // next, so one answer for the whole device fits one of them and fights the others (`AMB-D-835`).
  it("leaves the other projects where they were", () => {
    setRailWidth(ONE, 240);
    setSideNarrow(ONE, 400);
    expect(getRailWidth(TWO)).toBe(RAIL_DEFAULT);
    expect(getSideNarrow(TWO)).toBe(SIDE_NARROW_DEFAULT);
  });

  it("comes back to the project it belongs to, whichever was dragged last", () => {
    setRailWidth(ONE, 240);
    setRailWidth(TWO, 130);
    expect(getRailWidth(ONE)).toBe(240);
    expect(getRailWidth(TWO)).toBe(130);
  });

  // The face has no project for a moment as it comes up. It is drawn at the defaults, and a drag
  // there is taken without being written — a project's answer is not overwritten by a run that had
  // not been told which project it was on.
  it("is the default, and is kept nowhere, for a face on no project", () => {
    expect(getRailWidth(null)).toBe(RAIL_DEFAULT);
    expect(setRailWidth(null, 240)).toBe(240);
    expect(localStorage.length).toBe(0);
    expect(getRailWidth(null)).toBe(RAIL_DEFAULT);
  });
});

describe("the file face's two widths", () => {
  it("start apart, the wide one at what reading wants", () => {
    expect(getSideNarrow(ONE)).toBe(SIDE_NARROW_DEFAULT);
    expect(getSideWide(ONE)).toBe(SIDE_WIDE_DEFAULT);
  });

  // They answer different questions, so dragging one says nothing about the other (`AMB-D-835`).
  it("are dragged and kept separately", () => {
    setSideNarrow(ONE, 300);
    setSideWide(ONE, 700);
    expect(getSideNarrow(ONE)).toBe(300);
    expect(getSideWide(ONE)).toBe(700);
  });

  it("keeps the wide one per project like the rest", () => {
    setSideWide(ONE, 700);
    expect(getSideWide(TWO)).toBe(SIDE_WIDE_DEFAULT);
  });

  // The wide width lies over the panes, so a pane's floor is not in its sum. What it leaves is the
  // rail, which is never covered.
  it("lets the wide one take the window, less the rail and the tabs", () => {
    window.innerWidth = 960;
    expect(sideWideMax(RAIL_DEFAULT)).toBe(960 - TABS - RAIL_DEFAULT);
    expect(clampSideWide(9999, RAIL_DEFAULT)).toBe(960 - TABS - RAIL_DEFAULT);
    // Where the narrow one stops is well short of that: it is drawn beside the panes.
    expect(sideNarrowMax(RAIL_DEFAULT)).toBeLessThan(sideWideMax(RAIL_DEFAULT));
  });

  it("holds the wide one to the same floor, on a window with no room for it", () => {
    window.innerWidth = 200;
    expect(sideWideMax(RAIL_DEFAULT)).toBe(SIDE_MIN);
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

describe("whether the project tabs are compact", () => {
  // A first run that came up with a column of coloured letters would be asking a person to learn
  // which is which before being told any of it once.
  it("is the names on a device that has never said otherwise", () => {
    expect(getTabsCompact()).toBe(false);
  });

  it("is remembered once they are folded, and again once they are brought back", () => {
    expect(setTabsCompact(true)).toBe(true);
    expect(getTabsCompact()).toBe(true);
    setTabsCompact(false);
    expect(getTabsCompact()).toBe(false);
  });

  it("takes less of the window folded than named", () => {
    expect(tabsWidth(true)).toBeLessThan(tabsWidth(false));
  });
});

// The named width is dragged and kept for the device, the compact one is the mark's own and is not
// (`AMB-D-848`).
describe("the tab column's own width", () => {
  // A window with room for the column and both of the ones beside it: what is under test here is
  // what a person dragged, and a window too narrow for any of it answers with the floor whatever
  // they did.
  beforeEach(() => {
    window.innerWidth = 1280;
  });

  it("starts at the width the column shipped with", () => {
    expect(getTabsWidth()).toBe(TABS_DEFAULT);
    expect(tabsWidth(false)).toBe(TABS_DEFAULT);
  });

  it("comes back as it was left, on whichever project is on the screen", () => {
    setTabsWidth(200);
    expect(getTabsWidth()).toBe(200);
    expect(tabsWidth(false)).toBe(200);
  });

  it("leaves the folded width where it is, however far the named one was dragged", () => {
    setTabsWidth(240);
    expect(tabsWidth(true)).toBe(TABS_COMPACT_WIDTH);
  });

  it("never goes under the floor, however far the drag went", () => {
    expect(setTabsWidth(0)).toBe(TABS_MIN);
  });

  it("is clamped on the way out too, so a width kept on a wider screen cannot come back whole", () => {
    window.innerWidth = 960;
    // Written straight into storage, the way a run on a 4K display would have left it.
    localStorage.setItem("amenbo.termface.tabsWidth", "3000");
    expect(getTabsWidth()).toBe(tabsMax());
    expect(getTabsWidth()).toBeLessThan(3000);
  });

  it("falls back where what was kept is not a width at all", () => {
    localStorage.setItem("amenbo.termface.tabsWidth", "wide");
    expect(getTabsWidth()).toBe(TABS_DEFAULT);
  });

  // The tabs are measured against the window itself, because what they take is what `roomBeside`
  // subtracts: dragged to the ceiling, the two columns beside the panes still have their floors and
  // the middle still has its own.
  it("leaves the middle and both columns beside it their floors", () => {
    window.innerWidth = 960;
    const tabs = clampTabsWidth(9999);
    expect(960 - tabs - RAIL_MIN - SIDE_MIN).toBeGreaterThanOrEqual(PANE_MIN);
  });

  it("grows by what a column the person closed is no longer taking", () => {
    window.innerWidth = 960;
    expect(tabsMax(0, 0) - tabsMax()).toBe(RAIL_MIN + SIDE_MIN);
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
    expect(railMax(SIDE_NARROW_DEFAULT)).toBe(960 - TABS - SIDE_NARROW_DEFAULT - PANE_MIN);
    expect(sideNarrowMax(RAIL_DEFAULT)).toBe(960 - TABS - RAIL_DEFAULT - PANE_MIN);
  });

  // Closing a column is what makes room, so what it is taking is nought and the other may have it.
  it("grows by what a column the person closed is no longer taking", () => {
    window.innerWidth = 960;
    expect(sideNarrowMax(0)).toBe(960 - TABS - PANE_MIN);
  });

  // The tabs are never closed, so what they take is off the window before anything else is measured:
  // folding their names away is the one thing that gives any of it back (`AMB-D-838`).
  it("grows by what the tabs give back when their names are folded away", () => {
    window.innerWidth = 960;
    const named = sideNarrowMax(RAIL_DEFAULT);
    setTabsCompact(true);
    expect(sideNarrowMax(RAIL_DEFAULT) - named).toBe(TABS - tabsWidth(true));
  });

  // The middle is what the ceiling is for: dragged to it, a pane still has its floor.
  it("leaves the middle its floor when both columns are dragged out to it", () => {
    window.innerWidth = 960;
    const side = clampSideNarrow(9999, RAIL_DEFAULT);
    const rail = clampRailWidth(9999, side);
    expect(960 - TABS - rail - side).toBeGreaterThanOrEqual(PANE_MIN);
  });

  // A floor is a floor. On a window too narrow for one there is nothing to be done by making the
  // column narrower than it is drawable at.
  it("never falls below the column's own floor", () => {
    window.innerWidth = 400;
    expect(railMax(SIDE_NARROW_DEFAULT)).toBe(RAIL_MIN);
    expect(sideNarrowMax(RAIL_DEFAULT)).toBe(SIDE_MIN);
  });
});
