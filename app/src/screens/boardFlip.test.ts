// The pure decision core of the board move flourish: which cards slide, and by how much. The DOM side
// (snapshotting placements, applying transforms) is left to the browser; here we pin the guards that keep the
// effect best-effort — changed column (not a mere reflow), mounted in both layouts, in the viewport, not being
// dragged, and clipped.
import { describe, expect, it } from "vitest";
import { planFlip, type FlipCard } from "./boardFlip";

const VP = { width: 1000, height: 800 };
const card = (left: number, top: number, col: number): FlipCard => ({
  rect: { left, top, width: 100, height: 60 },
  col,
});

describe("planFlip", () => {
  it("slides a card that changed column, with the delta back to its old position", () => {
    const first = new Map([[1, card(0, 0, 0)]]);
    const last = new Map([[1, card(300, 40, 1)]]);
    const moves = planFlip(first, last, { viewport: VP, maxCards: 8 });
    expect(moves).toEqual([{ id: 1, dx: -300, dy: -40 }]);
  });

  it("ignores a card that shifted within its column (a reflow, not a move)", () => {
    // A sibling was inserted above it: it moved down the page but stayed in the same column.
    const first = new Map([[1, card(0, 0, 0)]]);
    const last = new Map([[1, card(0, 140, 0)]]);
    expect(planFlip(first, last, { viewport: VP, maxCards: 8 })).toEqual([]);
  });

  it("ignores a card mounted in only one layout (entering the view, not moving within it)", () => {
    const first = new Map<number, FlipCard>();
    const last = new Map([[1, card(300, 40, 1)]]);
    expect(planFlip(first, last, { viewport: VP, maxCards: 8 })).toEqual([]);
  });

  it("does not animate the card being dragged (a local move, not an outside one)", () => {
    const first = new Map([[1, card(0, 0, 0)]]);
    const last = new Map([[1, card(300, 40, 1)]]);
    expect(planFlip(first, last, { draggingId: 1, viewport: VP, maxCards: 8 })).toEqual([]);
  });

  it("skips a card that sits outside the viewport in either layout", () => {
    const offscreenFirst = new Map([[1, card(0, -500, 0)]]); // above the top
    const last = new Map([[1, card(0, 40, 1)]]);
    expect(planFlip(offscreenFirst, last, { viewport: VP, maxCards: 8 })).toEqual([]);

    const first = new Map([[1, card(0, 40, 0)]]);
    const offscreenLast = new Map([[1, card(2000, 40, 1)]]); // right of the edge
    expect(planFlip(first, offscreenLast, { viewport: VP, maxCards: 8 })).toEqual([]);
  });

  it("clips a burst to maxCards, in DOM order", () => {
    const first = new Map<number, FlipCard>();
    const last = new Map<number, FlipCard>();
    for (let i = 1; i <= 5; i++) {
      first.set(i, card(0, i * 70, 0));
      last.set(i, card(300, i * 70, 1)); // each crossed from column 0 to column 1
    }
    const moves = planFlip(first, last, { viewport: VP, maxCards: 2 });
    expect(moves.map((m) => m.id)).toEqual([1, 2]);
  });
});
