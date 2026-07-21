// The pure decision core of the board move flourish: which cards slide, and by how much. The DOM side
// (snapshotting rects, applying transforms) is left to the browser; here we pin the guards that keep the effect
// best-effort — moved, mounted in both layouts, in the viewport, not being dragged, and clipped.
import { describe, expect, it } from "vitest";
import { planFlip, type FlipRect } from "./boardFlip";

const VP = { width: 1000, height: 800 };
const rect = (left: number, top: number): FlipRect => ({ left, top, width: 100, height: 60 });

describe("planFlip", () => {
  it("slides a card that moved, with the delta back to its old position", () => {
    const first = new Map([[1, rect(0, 0)]]);
    const last = new Map([[1, rect(300, 40)]]);
    const moves = planFlip(first, last, { viewport: VP, maxCards: 20 });
    expect(moves).toEqual([{ id: 1, dx: -300, dy: -40 }]);
  });

  it("slides a neighbour the move reflowed too, so the whole shift reads as one motion", () => {
    // Card 1 crossed columns; card 2 stayed in its column but the vacated slot pulled it up — it slides as well.
    const first = new Map([[1, rect(0, 0)], [2, rect(0, 140)]]);
    const last = new Map([[1, rect(300, 40)], [2, rect(0, 70)]]);
    const moves = planFlip(first, last, { viewport: VP, maxCards: 20 });
    expect(moves).toEqual([
      { id: 1, dx: -300, dy: -40 },
      { id: 2, dx: 0, dy: 70 },
    ]);
  });

  it("ignores a card that did not move", () => {
    const first = new Map([[1, rect(300, 40)]]);
    const last = new Map([[1, rect(300, 40)]]);
    expect(planFlip(first, last, { viewport: VP, maxCards: 20 })).toEqual([]);
  });

  it("ignores a card mounted in only one layout (entering the view, not moving within it)", () => {
    const first = new Map<number, FlipRect>();
    const last = new Map([[1, rect(300, 40)]]);
    expect(planFlip(first, last, { viewport: VP, maxCards: 20 })).toEqual([]);
  });

  it("does not animate the card being dragged (a local move, not an outside one)", () => {
    const first = new Map([[1, rect(0, 0)]]);
    const last = new Map([[1, rect(300, 40)]]);
    expect(planFlip(first, last, { draggingId: 1, viewport: VP, maxCards: 20 })).toEqual([]);
  });

  it("skips a card that sits outside the viewport in either layout", () => {
    const offscreenFirst = new Map([[1, rect(0, -500)]]); // above the top
    const last = new Map([[1, rect(0, 40)]]);
    expect(planFlip(offscreenFirst, last, { viewport: VP, maxCards: 20 })).toEqual([]);

    const first = new Map([[1, rect(0, 40)]]);
    const offscreenLast = new Map([[1, rect(2000, 40)]]); // right of the edge
    expect(planFlip(first, offscreenLast, { viewport: VP, maxCards: 20 })).toEqual([]);
  });

  it("clips a burst to maxCards, in DOM order", () => {
    const first = new Map<number, FlipRect>();
    const last = new Map<number, FlipRect>();
    for (let i = 1; i <= 5; i++) {
      first.set(i, rect(0, i * 70));
      last.set(i, rect(300, i * 70));
    }
    const moves = planFlip(first, last, { viewport: VP, maxCards: 2 });
    expect(moves.map((m) => m.id)).toEqual([1, 2]);
  });
});
