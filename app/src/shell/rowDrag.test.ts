// @vitest-environment jsdom
// The arithmetic a reorder is now built on, which is the arithmetic the webview's own drag used to do for free
// (`AMB-D-775`): when a press has become a drag, which side of a row the pointer is on, and which row that even is.
//
// The last of those reads the document, and a headless DOM has no layout — every point in one is over nothing at
// all. So the hit test takes the document as a parameter and these tests answer for it, which is also what lets a
// row be placed exactly where a case needs it.
import { describe, expect, it } from "vitest";
import { DRAG_SLOP, draggedFar, landing, rowUnder, sideOfRow } from "./rowDrag";

/** A row of the given height at the given top, answering for its own rectangle. */
function row(id: number, top: number, height = 40): HTMLElement {
  const el = document.createElement("button");
  el.setAttribute("data-project-row", "");
  el.dataset.projectId = String(id);
  el.getBoundingClientRect = () => ({ top, height, bottom: top + height, left: 0, right: 200, width: 200, x: 0, y: top, toJSON: () => ({}) });
  return el;
}

/** A document whose one row stands wherever a case put it, and nothing anywhere else. */
const over = (el: Element | null) => ({ elementFromPoint: () => el });

const idOf = (el: HTMLElement) => {
  const id = Number(el.dataset.projectId);
  return Number.isFinite(id) && id !== 0 ? id : null;
};

describe("when a press has become a drag", () => {
  it("holds still for a hand that is holding still, and gives way once it travels", () => {
    const from = { x: 100, y: 100 };
    // A click is a press that went nowhere — and a hand on a trackpad never goes exactly nowhere.
    expect(draggedFar(from, { x: 100, y: 100 })).toBe(false);
    expect(draggedFar(from, { x: 102, y: 102 })).toBe(false);
    expect(draggedFar(from, { x: 100, y: 100 + DRAG_SLOP })).toBe(true);
    // Diagonal travel counts as travel: it is the distance and not either axis on its own.
    expect(draggedFar(from, { x: 96, y: 96 })).toBe(true);
  });
});

describe("which side of a row the pointer is on", () => {
  it("is decided by the midline, and the midline belongs to the half below it", () => {
    const rect = { top: 100, height: 40 };
    expect(sideOfRow(100, rect)).toBe("before");
    expect(sideOfRow(119, rect)).toBe("before");
    expect(sideOfRow(120, rect)).toBe("after");
    expect(sideOfRow(139, rect)).toBe("after");
  });
});

describe("which row the pointer is over", () => {
  it("is asked of the document rather than remembered — a list can scroll under a held pointer", () => {
    const one = row(7, 100);
    expect(rowUnder({ x: 10, y: 110 }, "data-project-row", over(one))).toBe(one);
    // The hit lands on whatever is drawn inside the row, and the row is what answers.
    const inside = document.createElement("span");
    one.appendChild(inside);
    expect(rowUnder({ x: 10, y: 110 }, "data-project-row", over(inside))).toBe(one);
  });

  it("is nothing where the pointer is off the list", () => {
    expect(rowUnder({ x: 10, y: 10 }, "data-project-row", over(null))).toBeNull();
    expect(rowUnder({ x: 10, y: 10 }, "data-project-row", over(document.createElement("div")))).toBeNull();
  });
});

describe("where a drag that ended here would put the row", () => {
  it("names the row it landed on and which side of it", () => {
    expect(landing(3, { x: 10, y: 110 }, "data-project-row", idOf, over(row(7, 100))))
      .toEqual({ id: 7, side: "before" });
    expect(landing(3, { x: 10, y: 130 }, "data-project-row", idOf, over(row(7, 100))))
      .toEqual({ id: 7, side: "after" });
  });

  it("is nothing where the row would go back where it was, so no write is made at all", () => {
    // Dropped on itself. Answering "before itself" would be a write that reorders nothing.
    expect(landing(7, { x: 10, y: 110 }, "data-project-row", idOf, over(row(7, 100)))).toBeNull();
    // Released off the list — over the smart views, over the window's own chrome, past the edge entirely.
    expect(landing(7, { x: 10, y: 10 }, "data-project-row", idOf, over(null))).toBeNull();
  });
});
