// @vitest-environment jsdom
// Scrolling a box while something is held against its edge — the one thing HTML5 drag never did for
// us either, so there is no old behaviour to match, only the measurement (`AMB-T-3755`).
//
// A headless DOM lays nothing out: every box in one is zero by zero and every point in one is over
// nothing at all. So each box here states its own rectangle and its own room to scroll, and the
// document is a parameter that says what a point is over.
import { afterEach, describe, expect, it, vi } from "vitest";

import { EDGE_BAND, EDGE_STEP, edgePush, flowEdges, nudge, scrollNearEdge } from "./edgeScroll";

/** Every axis open, which is what the arithmetic on its own is being asked about. */
const anyAxis = () => true;

/** A box that answers for its own rectangle and its own room, because jsdom will not. */
function box(
  rect: { left: number; right: number; top: number; bottom: number },
  room: { x?: number; y?: number } = {},
  overflow = "auto",
): HTMLElement {
  const el = document.createElement("div");
  // Both longhands by name: what a computed style answers for is `overflow-x` and `overflow-y`, and a
  // headless DOM does not take the shorthand apart.
  el.style.overflowX = overflow;
  el.style.overflowY = overflow;
  Object.defineProperties(el, {
    scrollLeft: { value: 0, writable: true },
    scrollTop: { value: 0, writable: true },
    clientWidth: { value: 100 },
    clientHeight: { value: 100 },
    scrollWidth: { value: 100 + (room.x ?? 0) },
    scrollHeight: { value: 100 + (room.y ?? 0) },
  });
  el.getBoundingClientRect = () => ({ ...rect, width: rect.right - rect.left, height: rect.bottom - rect.top, x: rect.left, y: rect.top, toJSON: () => ({}) });
  return el;
}

/** A document whose one hit is whatever a case put under the pointer. */
const over = (el: Element | null) => ({ elementFromPoint: () => el });

/**
 * The same, for the real document — which is what the frame loop reads, having no seam of its own.
 * One is enough for what the loop is being asked here: whether it runs at all, and whether it stops.
 */
function underPointer(el: Element | null): void {
  (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () => el;
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("how far a box wants to move along one axis", () => {
  it("is nothing in the middle, and a step towards whichever edge the pointer came near", () => {
    expect(edgePush(500, 0, 1000)).toBe(0);
    expect(edgePush(EDGE_BAND, 0, 1000)).toBe(0);
    expect(edgePush(EDGE_BAND - 1, 0, 1000)).toBe(-EDGE_STEP);
    expect(edgePush(1000 - EDGE_BAND, 0, 1000)).toBe(0);
    expect(edgePush(1000 - EDGE_BAND + 1, 0, 1000)).toBe(EDGE_STEP);
  });

  it("is nothing at all where the pointer is outside the box — that point belongs to something else", () => {
    expect(edgePush(-5, 0, 1000)).toBe(0);
    expect(edgePush(1005, 0, 1000)).toBe(0);
  });

  it("splits a box too short to hold two bands, so the two edges never claim the same point", () => {
    // 60 tall against a 50 band: without the split, every point in it would be within reach of both
    // edges and the near one would win the whole box.
    expect(edgePush(100, 100, 160)).toBe(-EDGE_STEP);
    expect(edgePush(129, 100, 160)).toBe(-EDGE_STEP);
    expect(edgePush(130, 100, 160)).toBe(0);
    expect(edgePush(131, 100, 160)).toBe(EDGE_STEP);
  });
});

describe("moving one box one frame's worth", () => {
  it("moves it towards the edge the pointer is near, and says that it did", () => {
    const el = box({ left: 0, right: 200, top: 0, bottom: 400 }, { y: 356 });
    expect(nudge(el, { x: 100, y: 380 }, anyAxis)).toBe(true);
    expect(el.scrollTop).toBe(EDGE_STEP);
    expect(el.scrollLeft).toBe(0);
  });

  it("leaves a box with nothing to scroll alone, however near the edge the pointer is", () => {
    const el = box({ left: 0, right: 200, top: 0, bottom: 400 });
    expect(nudge(el, { x: 100, y: 380 }, anyAxis)).toBe(false);
    expect(el.scrollTop).toBe(0);
  });

  it("says no once the box is against the stop the pointer is asking for", () => {
    const el = box({ left: 0, right: 200, top: 0, bottom: 400 }, { y: 356 });
    el.scrollTop = 356;
    expect(nudge(el, { x: 100, y: 380 }, anyAxis)).toBe(false);
    expect(el.scrollTop).toBe(356);
    // The other way is still open, and the same box says so.
    expect(nudge(el, { x: 100, y: 10 }, anyAxis)).toBe(true);
    expect(el.scrollTop).toBe(356 - EDGE_STEP);
  });

  it("never travels past the stop, whatever the last step would have added", () => {
    const el = box({ left: 0, right: 200, top: 0, bottom: 400 }, { y: 356 });
    el.scrollTop = 353;
    expect(nudge(el, { x: 100, y: 380 }, anyAxis)).toBe(true);
    expect(el.scrollTop).toBe(356);
  });

  it("takes each axis on its own, so a board carries a card sideways and downwards at once", () => {
    const el = box({ left: 0, right: 400, top: 0, bottom: 400 }, { x: 200, y: 200 });
    expect(nudge(el, { x: 390, y: 390 }, anyAxis)).toBe(true);
    expect(el.scrollLeft).toBe(EDGE_STEP);
    expect(el.scrollTop).toBe(EDGE_STEP);
  });

  it("leaves an axis the reader cannot scroll where it is, however much room a script would find", () => {
    const el = box({ left: 0, right: 400, top: 0, bottom: 400 }, { x: 200, y: 200 });
    expect(nudge(el, { x: 390, y: 390 }, (axis) => axis === "x")).toBe(true);
    expect(el.scrollLeft).toBe(EDGE_STEP);
    expect(el.scrollTop).toBe(0);
  });
});

describe("scrolling whatever the pointer is over", () => {
  it("hands the travel to the box behind, where the one under the pointer cannot take it", () => {
    // A column that does not scroll, inside a board that scrolls sideways — which is the board.
    const board = box({ left: 0, right: 400, top: 0, bottom: 400 }, { x: 200 });
    const column = box({ left: 300, right: 400, top: 0, bottom: 400 });
    board.append(column);
    expect(scrollNearEdge({ x: 395, y: 200 }, over(column))).toBe(true);
    expect(board.scrollLeft).toBe(EDGE_STEP);
    expect(column.scrollLeft).toBe(0);
  });

  it("is nothing where the pointer is over nothing, and nothing where no box up the line can move", () => {
    expect(scrollNearEdge({ x: 10, y: 10 }, over(null))).toBe(false);
    const still = box({ left: 0, right: 200, top: 0, bottom: 400 });
    expect(scrollNearEdge({ x: 100, y: 395 }, over(still))).toBe(false);
  });

  it("walks past a clipped box rather than scrolling words a reader can never scroll back", () => {
    // A truncated title inside a list: it has more content than width, and writing to it works. What
    // it does not have is any way for the reader to undo that.
    const list = box({ left: 0, right: 200, top: 0, bottom: 400 }, { y: 356 });
    const title = box({ left: 0, right: 200, top: 360, bottom: 400 }, { x: 300 }, "hidden");
    list.append(title);
    expect(scrollNearEdge({ x: 195, y: 395 }, over(title))).toBe(true);
    expect(title.scrollLeft).toBe(0);
    expect(list.scrollTop).toBe(EDGE_STEP);
  });
});

describe("the loop that keeps a held box flowing", () => {
  it("scrolls with the hand holding still, and stops when it is called off", async () => {
    const el = box({ left: 0, right: 200, top: 0, bottom: 400 }, { y: 356 });
    document.body.append(el);
    underPointer(el);
    const moved = vi.fn();
    // The point never changes: a pointer resting against an edge fires no move at all, and this is
    // the whole reason the loop is its own.
    const at = { x: 100, y: 390 };
    const stop = flowEdges(() => at, moved);
    await new Promise((r) => setTimeout(r, 60));
    stop();
    const scrolled = el.scrollTop;
    expect(scrolled).toBeGreaterThanOrEqual(EDGE_STEP);
    expect(moved).toHaveBeenCalled();
    await new Promise((r) => setTimeout(r, 60));
    expect(el.scrollTop).toBe(scrolled);
  });

  it("scrolls nothing while the gesture says there is nothing in hand", async () => {
    const el = box({ left: 0, right: 200, top: 0, bottom: 400 }, { y: 356 });
    document.body.append(el);
    underPointer(el);
    const moved = vi.fn();
    const stop = flowEdges(() => null, moved);
    await new Promise((r) => setTimeout(r, 60));
    stop();
    expect(el.scrollTop).toBe(0);
    expect(moved).not.toHaveBeenCalled();
  });
});
