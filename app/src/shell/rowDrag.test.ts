// @vitest-environment jsdom
// The half of a reorder that belongs to a list: which side of a row the pointer is on, and therefore where a drop
// would put the row. The other half — when a press has become a drag, and which row it is over at all — is asked
// by the board's cards too and is pinned in `../core/pointerDrag.test`.
//
// Where a drop lands reads the document, and a headless DOM has no layout — every point in one is over nothing at
// all. So the hit test takes the document as a parameter and these tests answer for it, which is also what lets a
// row be placed exactly where a case needs it.
import { describe, expect, it } from "vitest";
import { landing, sideOfRow } from "./rowDrag";

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

describe("which side of a row the pointer is on", () => {
  it("is decided by the midline, and the midline belongs to the half below it", () => {
    const rect = { top: 100, height: 40 };
    expect(sideOfRow(100, rect)).toBe("before");
    expect(sideOfRow(119, rect)).toBe("before");
    expect(sideOfRow(120, rect)).toBe("after");
    expect(sideOfRow(139, rect)).toBe("after");
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
