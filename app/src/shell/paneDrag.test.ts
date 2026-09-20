// The two answers a drag on a pane needs, away from the face that draws them. jsdom has no layout,
// so every rectangle here is written out — which is what lets a case put the pointer exactly where
// the question is interesting.
import { describe, expect, it } from "vitest";
import { ACROSS, DOWN, type Size } from "../talk/layout";
import { axisOnPane, sideOnPane, sizeStretchedTo } from "./paneDrag";

/** A page a cell wide of 10px and a row 100px tall, at the origin — so a point reads as cells. */
const GRID = { left: 0, top: 0, width: ACROSS * 10, height: DOWN * 100 };

/** Where a corner pulled to this many cells across and down lands. */
function pulledTo(spot: { across: number; down: number }, across: number, down: number): Size {
  return sizeStretchedTo(GRID, spot, { x: (spot.across + across) * 10, y: (spot.down + down) * 100 });
}

describe("which way a pane has its neighbours", () => {
  it("is down the page for the sizes that take the whole width", () => {
    expect(axisOnPane("whole")).toBe("down");
    expect(axisOnPane("half-down")).toBe("down");
  });

  it("is across it for every size narrower than the page", () => {
    for (const size of ["half", "quarter", "sixth", "eighth"] as const) {
      expect(axisOnPane(size)).toBe("across");
    }
  });
});

describe("which side of a pane a drop lands on", () => {
  const rect = { left: 100, top: 40, width: 200, height: 80 };

  it("reads the midline across for a pane with neighbours beside it", () => {
    expect(sideOnPane({ x: 150, y: 80 }, rect, "quarter")).toBe("before");
    expect(sideOnPane({ x: 250, y: 80 }, rect, "quarter")).toBe("after");
  });

  it("reads it down the page for a pane that takes the whole width", () => {
    expect(sideOnPane({ x: 250, y: 60 }, rect, "half-down")).toBe("before");
    expect(sideOnPane({ x: 150, y: 100 }, rect, "half-down")).toBe("after");
  });
});

describe("the size a pulled corner lands on", () => {
  const home = { across: 0, down: 0 };

  it("takes the rows from the pull down and the columns from the pull across", () => {
    expect(pulledTo(home, 12, 2)).toBe("whole");
    expect(pulledTo(home, 6, 2)).toBe("half");
    expect(pulledTo(home, 12, 1)).toBe("half-down");
    expect(pulledTo(home, 6, 1)).toBe("quarter");
    expect(pulledTo(home, 4, 1)).toBe("sixth");
    expect(pulledTo(home, 3, 1)).toBe("eighth");
  });

  it("stops at the widest size the pull has covered, so dragging out grows a step at a time", () => {
    expect(pulledTo(home, 5, 1)).toBe("sixth");
    expect(pulledTo(home, 11, 1)).toBe("quarter");
    expect(pulledTo(home, 11, 2)).toBe("half");
  });

  it("gives the narrowest of its row count to a pull that has covered none of them", () => {
    expect(pulledTo(home, 1, 1)).toBe("eighth");
    expect(pulledTo(home, 2, 2)).toBe("half");
  });

  it("is measured from the cell the pane stands in, not from the edge of the page", () => {
    // A pane starting half way across, pulled three cells further: an eighth, the same as one
    // pulled three cells from the left edge.
    expect(pulledTo({ across: 6, down: 0 }, 3, 1)).toBe("eighth");
    // And pulled to the right edge of the page from there — six cells, which is a quarter.
    expect(pulledTo({ across: 6, down: 0 }, 6, 1)).toBe("quarter");
  });

  it("is not held to the room left on the page, because the order lays the pane down again", () => {
    // Six cells in from the left, pulled past the right edge: the size it asks for is the whole
    // page, and where that lands is the order's answer rather than this one.
    expect(sizeStretchedTo(GRID, { across: 6, down: 0 }, { x: 400, y: 200 })).toBe("whole");
  });
});
