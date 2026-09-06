// @vitest-environment jsdom
// Reordering a project row with a press and a move, which is the whole of what the sidebar lost when the app itself
// took over what is dropped on it: with that switch thrown the webview's in-window drag stops firing at all on macOS
// and Windows (`AMB-D-775`).
//
// What is under test here is the one thing the webview used to decide and now nobody else does — **whether a press
// was a navigation or a reorder**. Both arrive as the same three events, and getting it wrong is not a subtle
// failure: every attempt to reorder would open a project instead. The arithmetic underneath is `rowDrag.test`.
//
// A headless DOM has no layout, so the hit test the release resolves against is answered here, exactly as the rows
// would stand on a screen.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** Every reorder the sidebar asked for, in order. Empty is the assertion several of these make. */
  moved: [] as { id: number; position: string; anchor?: number }[],
}));

vi.mock("../store/store", () => ({
  useStore: () => ({
    moveProject: (id: number, position: string, anchor?: number) => hoisted.moved.push({ id, position, anchor }),
  }),
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: {
    smartViews: () => [],
    listProjects: () => [
      { id: 1, name: "Greenhouse", color: "#0a0", icon: null, openCount: 0, proposedDecisionCount: 0 },
      { id: 2, name: "Orchard", color: "#00a", icon: null, openCount: 0, proposedDecisionCount: 0 },
      { id: 3, name: "Vineyard", color: "#a00", icon: null, openCount: 0, proposedDecisionCount: 0 },
    ],
  },
}));

vi.mock("../core/mailbox", () => ({ useInboxCount: () => 0 }));
vi.mock("../core/reads", () => ({
  useArchivedProjects: () => [],
  useDueCounts: () => ({ overdue: 0, today: 0, tomorrow: 0 }),
}));

import { Sidebar } from "./Sidebar";
import { DRAG_SLOP } from "../core/pointerDrag";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
/** Where a click on a row would have taken us. Empty is a press that reordered instead. */
let went: string[];

const ROW_HEIGHT = 40;
const rows = () => [...container.querySelectorAll<HTMLElement>("[data-project-row]")];

/** The row for a project id, standing where it would stand on a screen. */
const rowFor = (id: number) => rows().find((r) => r.dataset.projectId === String(id))!;

function press(el: HTMLElement, at: { x: number; y: number }) {
  el.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, clientX: at.x, clientY: at.y }));
}
function move(el: HTMLElement, at: { x: number; y: number }) {
  el.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: at.x, clientY: at.y }));
}
function release(el: HTMLElement, at: { x: number; y: number }) {
  el.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: at.x, clientY: at.y }));
  el.dispatchEvent(new MouseEvent("click", { bubbles: true }));
}

beforeEach(() => {
  hoisted.moved = [];
  went = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  act(() => {
    root.render(createElement(Sidebar, {
      nav: { type: "view", id: "inbox" },
      onNav: (n: { type: string; id?: string }) => went.push(`${n.type}:${n.id}`),
    }));
  });
  // The three rows, stacked from y=100 — the layout a headless DOM will not do.
  rows().forEach((row, i) => {
    const top = 100 + i * ROW_HEIGHT;
    row.getBoundingClientRect = () => ({
      top, height: ROW_HEIGHT, bottom: top + ROW_HEIGHT, left: 0, right: 200, width: 200, x: 0, y: top,
      toJSON: () => ({}),
    });
  });
  // And the hit test, answering with whichever of them the point falls in.
  document.elementFromPoint = (_x: number, y: number) =>
    rows().find((row) => {
      const r = row.getBoundingClientRect();
      return y >= r.top && y < r.top + r.height;
    }) ?? null;
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  document.body.classList.remove("dragging-row");
});

describe("a press on a project row", () => {
  it("still opens the project when the hand did not travel", () => {
    const row = rowFor(2);
    act(() => {
      press(row, { x: 40, y: 150 });
      // A hand on a trackpad never holds perfectly still, and this much wander is not a reorder.
      move(row, { x: 41, y: 152 });
      release(row, { x: 41, y: 152 });
    });
    expect(went).toEqual(["project:2"]);
    expect(hoisted.moved).toEqual([]);
  });

  it("reorders instead of opening once it travels, and the click behind it is swallowed", () => {
    const row = rowFor(3);
    act(() => {
      press(row, { x: 40, y: 190 });
      // Up and over the first row, past its midline, so the row lands above it.
      move(row, { x: 40, y: 190 - DRAG_SLOP });
      move(row, { x: 40, y: 110 });
      release(row, { x: 40, y: 110 });
    });
    expect(hoisted.moved).toEqual([{ id: 3, position: "before", anchor: 1 }]);
    // The click arrives whatever the press turned out to be. Left alone it would open the project as well.
    expect(went).toEqual([]);
  });

  it("takes the side from where the pointer was released and not from the last row it crossed", () => {
    const row = rowFor(1);
    act(() => {
      press(row, { x: 40, y: 110 });
      move(row, { x: 40, y: 110 + DRAG_SLOP });
      move(row, { x: 40, y: 150 });
      // Released below the midline of the third row (180..220), having last passed over the second.
      release(row, { x: 40, y: 210 });
    });
    expect(hoisted.moved).toEqual([{ id: 1, position: "after", anchor: 3 }]);
  });

  it("writes nothing when it is released back on itself, or off the list", () => {
    const row = rowFor(2);
    act(() => {
      press(row, { x: 40, y: 150 });
      move(row, { x: 40, y: 150 + DRAG_SLOP });
      release(row, { x: 40, y: 155 });
    });
    act(() => {
      press(row, { x: 40, y: 150 });
      move(row, { x: 40, y: 150 + DRAG_SLOP });
      release(row, { x: 40, y: 9000 });
    });
    expect(hoisted.moved).toEqual([]);
    expect(went).toEqual([]);
  });

  it("writes nothing when the system takes the drag away", () => {
    const row = rowFor(3);
    act(() => {
      press(row, { x: 40, y: 190 });
      move(row, { x: 40, y: 110 });
      row.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true }));
      row.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    // An interrupted drag is not a choice anybody made — and it is not a navigation either.
    expect(hoisted.moved).toEqual([]);
    expect(went).toEqual([]);
  });
});

describe("the two fences the webview used to hold", () => {
  it("stops the selection and the context menu while a row is held, and lets both back afterwards", () => {
    const row = rowFor(3);
    act(() => {
      press(row, { x: 40, y: 190 });
      move(row, { x: 40, y: 110 });
    });
    expect(document.body.classList.contains("dragging-row")).toBe(true);
    // Left alone, this opens the webview's own menu — on macOS no pointer event arrives again until it is
    // dismissed, measured at 12.2 seconds (`AMB-T-3755`).
    const menu = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.dispatchEvent(menu);
    expect(menu.defaultPrevented).toBe(true);

    act(() => release(row, { x: 40, y: 110 }));
    expect(document.body.classList.contains("dragging-row")).toBe(false);
    const after = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.dispatchEvent(after);
    expect(after.defaultPrevented).toBe(false);
  });
});
