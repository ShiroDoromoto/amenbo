// @vitest-environment jsdom
// The two questions the webview's own drag used to answer for free, and which both gestures now ask
// here (`AMB-D-775`): when a press has become a drag, and what the pointer is over.
//
// The second reads the document, and a headless DOM has no layout — every point in one is over
// nothing at all. So the hit test takes the document as a parameter and these tests answer for it,
// which is also what lets a thing be placed exactly where a case needs it.
import { describe, expect, it } from "vitest";
import { DRAG_SLOP, draggedFar, elementUnder } from "./pointerDrag";

/** A document whose one element stands wherever a case put it, and nothing anywhere else. */
const over = (el: Element | null) => ({ elementFromPoint: () => el });

/** A row of the given height at the given top, answering for its own rectangle. */
function row(top: number, height = 40): HTMLElement {
  const el = document.createElement("button");
  el.setAttribute("data-project-row", "");
  el.getBoundingClientRect = () => ({ top, height, bottom: top + height, left: 0, right: 200, width: 200, x: 0, y: top, toJSON: () => ({}) });
  return el;
}

describe("when a press has become a drag", () => {
  it("holds still for a hand that is holding still, and gives way once it travels", () => {
    const from = { x: 100, y: 100 };
    // A click is a press that went nowhere — and a hand on a trackpad never goes exactly nowhere.
    expect(draggedFar(from, { x: 100, y: 100 })).toBe(false);
    expect(draggedFar(from, { x: 102, y: 102 })).toBe(false);
    expect(draggedFar(from, { x: 100, y: 100 + DRAG_SLOP })).toBe(true);
    // Diagonal travel counts as travel: it is the distance and not either axis on its own, so four
    // pixels up and four across is past a line neither of them reaches.
    expect(draggedFar(from, { x: 96, y: 96 })).toBe(true);
  });
});

describe("what the pointer is over", () => {
  it("is asked of the document rather than remembered — a list can scroll under a held pointer", () => {
    const one = row(100);
    expect(elementUnder({ x: 10, y: 110 }, "data-project-row", over(one))).toBe(one);
    // The hit lands on whatever is drawn inside it, and the thing carrying the attribute answers.
    const inside = document.createElement("span");
    one.appendChild(inside);
    expect(elementUnder({ x: 10, y: 110 }, "data-project-row", over(inside))).toBe(one);
  });

  it("is nothing where the pointer is off the thing entirely", () => {
    expect(elementUnder({ x: 10, y: 10 }, "data-project-row", over(null))).toBeNull();
    expect(elementUnder({ x: 10, y: 10 }, "data-project-row", over(document.createElement("div")))).toBeNull();
  });
});
