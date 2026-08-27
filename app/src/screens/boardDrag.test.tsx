// @vitest-environment jsdom
// What moving a card by pointer has to get right, now that the browser does none of it for us.
//
// A card is a button as well as a thing to move, so the whole gesture turns on telling a press from a
// drag; and the column a card lands in is found by asking the document, not by remembering a
// rectangle that a scroll has since invalidated (`AMB-T-3755`). The two fences — no context menu, no
// text selection while a card is held — are measured here as well: without either, the gesture is
// unusable on at least two of the three operating systems, and neither shows up in a screenshot.
import { act, createElement, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { columnUnder, DRAG_SLOP, DROP_ATTR, splitColumn, travelled, useCardDrag } from "./boardDrag";

/** jsdom lays nothing out, so what is under the pointer is stated rather than measured. */
function under(el: Element | null): void {
  (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () => el;
}

let container: HTMLDivElement;
let root: Root;
/** What the board was told to do with a landed card. */
let landed: [string, number][];
/** The one handler a card puts on its press, as the board hands it down. */
let press: ((id: number, from: string, event: unknown) => void) | null;
let dragging: number | null;
let overColumn: string | null;
/** Where the card says it already is. Usually the column it sits in — but see the done column. */
let home: string;

function Board() {
  const drag = useCardDrag((column, id) => { landed.push([column, id]); });
  press = drag.press as unknown as typeof press;
  dragging = drag.draggingId;
  overColumn = drag.overColumn;
  // Two columns and one card. The card is the thing pressed; the columns answer for what is under the
  // pointer, which in a laid-out browser is what `elementFromPoint` would have found.
  const [taken] = useState(1);
  return createElement("div", null,
    createElement("div", { [DROP_ATTR]: "status:todo", id: "todo" },
      createElement("div", {
        className: "card", id: "card",
        onPointerDown: (e: never) => press?.(taken, home, e),
      }, "a card")),
    createElement("div", { [DROP_ATTR]: "status:done", id: "done" }));
}

/** A press, a move and a release, as a browser delivers them through a captured pointer. */
function pointer(kind: string, x: number, y: number, button = 0): PointerEvent {
  const e = new MouseEvent(kind, { bubbles: true, clientX: x, clientY: y, button });
  Object.defineProperty(e, "pointerId", { value: 7 });
  return e as PointerEvent;
}

async function down(x: number, y: number, button = 0) {
  await act(async () => {
    document.getElementById("card")?.dispatchEvent(pointer("pointerdown", x, y, button));
  });
}

async function to(kind: "pointermove" | "pointerup" | "pointercancel", x: number, y: number) {
  await act(async () => {
    document.getElementById("card")?.dispatchEvent(pointer(kind, x, y));
    // The hit test is deferred to a frame, which jsdom runs as a timer.
    await new Promise((r) => setTimeout(r, 20));
  });
}

beforeEach(() => {
  landed = [];
  home = "status:todo";
  press = null;
  dragging = null;
  overColumn = null;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  // jsdom has no pointer capture at all; the gesture only ever asks for it and gives it back.
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
  Element.prototype.hasPointerCapture = () => false;
  (globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  act(() => root.render(createElement(Board)));
  under(null);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  document.body.className = "";
});

describe("the point a card is over", () => {
  it("is the column drawn under it, and none where there is no column", () => {
    const todo = document.getElementById("todo");
    under(todo);
    expect(columnUnder(10, 10)).toBe("status:todo");
    // Found by walking up: what is literally under the pointer is the card's own text.
    under(document.getElementById("card"));
    expect(columnUnder(10, 10)).toBe("status:todo");
    under(null);
    expect(columnUnder(10, 10)).toBeNull();
  });

  /** One key spells a status, a dimension's value and the column for the cards carrying none. */
  it("comes apart into the board that drew it and which of its columns it is", () => {
    expect(splitColumn("status:todo")).toEqual(["status", "todo"]);
    expect(splitColumn("dim:12")).toEqual(["dim", "12"]);
    expect(splitColumn("dim:none")).toEqual(["dim", "none"]);
  });
});

describe("telling a press from a drag", () => {
  /** Without a threshold, a press meant to move a card navigates instead. */
  it("does not begin a drag until the pointer has travelled", async () => {
    expect(travelled({ x: 0, y: 0 }, { x: 3, y: 4 })).toBe(5);

    await down(100, 100);
    await to("pointermove", 100 + DRAG_SLOP - 1, 100);
    expect(dragging, "a press that barely moved took the card").toBeNull();
    expect(document.querySelector(".card--ghost")).toBeNull();

    await to("pointermove", 100 + DRAG_SLOP + 4, 100);
    expect(dragging).toBe(1);
    // What follows the pointer is the card's own node, copied.
    expect(document.querySelector(".card--ghost")?.textContent).toBe("a card");
  });

  /** A press that never became a drag is a click, and the card opens as it always did. */
  it("lands nothing, and leaves nothing behind, when the press did not travel", async () => {
    await down(100, 100);
    await to("pointerup", 101, 100);
    expect(landed).toEqual([]);
    expect(document.querySelector(".card--ghost")).toBeNull();
    expect(document.body.classList.contains("is-dragging")).toBe(false);
    // Nothing is left standing to swallow the click a plain press ends in — the card opens.
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    let reached = false;
    container.addEventListener("click", () => { reached = true; });
    document.getElementById("card")?.dispatchEvent(click);
    expect(reached).toBe(true);
  });

  // A card carried across the board is not a card somebody meant to open, and the press ends in a
  // click all the same. 🚨 Stopping it takes stopping the event, not preventing its default: the
  // card's handler is React's, hung on the root rather than on the card (`AMB-T-3794`).
  it("keeps the click a finished drag ends in from reaching the card", async () => {
    let reached = 0;
    container.addEventListener("click", () => { reached += 1; });

    await down(100, 100);
    under(document.getElementById("done"));
    await to("pointermove", 200, 100);
    await to("pointerup", 200, 100);
    document.getElementById("card")?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    expect(reached, "the card opened under a drag that had just finished").toBe(0);

    // Taken once. The next real click is somebody pressing the card, and it opens.
    document.getElementById("card")?.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    expect(reached).toBe(1);
  });

  /** A card is grabbed with the main button; the others belong to whatever else answers them. */
  it("ignores a press that is not the main button", async () => {
    await down(100, 100, 2);
    await to("pointermove", 200, 100);
    expect(dragging).toBeNull();
  });
});

describe("letting a card go", () => {
  it("hands the board the column it landed on", async () => {
    await down(100, 100);
    under(document.getElementById("done"));
    await to("pointermove", 200, 100);
    expect(overColumn).toBe("status:done");
    await to("pointerup", 200, 100);
    expect(landed).toEqual([["status:done", 1]]);
    expect(dragging).toBeNull();
    expect(document.querySelector(".card--ghost")).toBeNull();
  });

  // The board used to ask this two different ways — one for the status board, one for a dimension's.
  // Comparing the column it came from with the one it landed on says it once, for both.
  it("says nothing about a card let go over the column it came from", async () => {
    await down(100, 100);
    under(document.getElementById("todo"));
    await to("pointermove", 200, 100);
    await to("pointerup", 200, 100);
    expect(landed).toEqual([]);
  });

  // The done column draws the rejected cards too (`AMB-D-397`), so a rejected card sits under a "done"
  // heading while its own place is `rejected`. Letting it go there is a reader saying it was done
  // after all — which comparing against the column, rather than against the card, would have swallowed.
  it("lands a card let go over the column it sits in, where that is not the place it is in", async () => {
    home = "status:rejected";
    const done = document.getElementById("todo");
    done?.setAttribute(DROP_ATTR, "status:done");
    await down(100, 100);
    under(done);
    await to("pointermove", 200, 100);
    await to("pointerup", 200, 100);
    expect(landed).toEqual([["status:done", 1]]);
  });

  it("says nothing about a card let go over no column at all", async () => {
    await down(100, 100);
    under(document.getElementById("done"));
    await to("pointermove", 200, 100);
    under(null);
    await to("pointerup", 900, 900);
    expect(landed).toEqual([]);
  });
});

describe("the two fences", () => {
  // 🚨 Without this, a right click during a drag opens the browser's own menu and macOS delivers no
  // pointer event at all until it is dismissed — 12.2 seconds, measured. It reads as "it sometimes
  // sticks" (`AMB-T-3755`).
  it("swallows the context menu while a card is held, and only while it is held", async () => {
    const menu = () => {
      const e = new Event("contextmenu", { bubbles: true, cancelable: true });
      window.dispatchEvent(e);
      return e.defaultPrevented;
    };
    expect(menu()).toBe(false);

    await down(100, 100);
    under(document.getElementById("done"));
    await to("pointermove", 200, 100);
    expect(menu()).toBe(true);

    await to("pointerup", 200, 100);
    expect(menu(), "the menu was still being swallowed after the card was let go").toBe(false);
  });

  // 🚨 Without this, dragging down a column selects the text it passes over on macOS and Linux.
  it("stops text being selected from the press onwards, and only until the card is let go", async () => {
    await down(100, 100);
    // 🚨 Already up, before the pointer has moved at all: the browser begins selecting on the first
    // move, so a fence raised at the threshold arrives after the first few characters are already
    // blue (`AMB-T-3794`).
    expect(document.body.classList.contains("is-dragging")).toBe(true);
    await to("pointermove", 200, 100);
    expect(document.body.classList.contains("is-dragging")).toBe(true);
    await to("pointerup", 200, 100);
    expect(document.body.classList.contains("is-dragging")).toBe(false);
  });

  /** A gesture the OS takes away leaves nothing standing either. */
  it("puts everything back when the pointer is taken away mid-drag", async () => {
    await down(100, 100);
    under(document.getElementById("done"));
    await to("pointermove", 200, 100);
    await to("pointercancel", 200, 100);
    expect(landed).toEqual([]);
    expect(dragging).toBeNull();
    expect(document.body.classList.contains("is-dragging")).toBe(false);
    expect(document.querySelector(".card--ghost")).toBeNull();
  });
});
