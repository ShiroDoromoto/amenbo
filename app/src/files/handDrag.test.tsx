// @vitest-environment jsdom
// Carrying a row of the file panel to a pane and letting it go there (`AMB-D-820`).
//
// A row is a thing to open as well as a thing to carry, so the whole gesture turns on telling a press
// from a drag — and on the click a finished drag ends in never reaching the row, or the file just
// handed to a pane opens in the panel lying over it.
//
// The pane it lands on is asked of the document rather than remembered, and asked again at the
// landing: a pane whose program ended while the row was being carried is one a press-time answer
// would get wrong.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { DRAG_SLOP } from "../core/pointerDrag";
import { HAND_ATTR, paneUnder, useHandDrag } from "./handDrag";

/** jsdom lays nothing out, so what is under the pointer is stated rather than measured. */
function under(el: Element | null): void {
  (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () => el;
}

let container: HTMLDivElement;
let root: Root;
/** What the face was told to do with landed rows. */
let landed: [string, string[]][];
/** Which panes have something running in them, which is what decides whether one takes a row. */
let running: string[];
/** The pane the pointer is over, as the face would draw the surface on it. */
let overFrame: string | null;
let press: ((wholes: string[], event: unknown) => void) | null;

function Face() {
  const drag = useHandDrag(
    (frame, wholes) => { landed.push([frame, wholes]); },
    (frame) => running.includes(frame),
  );
  press = drag.press as unknown as typeof press;
  overFrame = drag.overFrame;
  // One row and two panes. The panes answer for what is under the pointer, which in a laid-out
  // browser is what `elementFromPoint` would have found.
  return createElement("div", null,
    createElement("li", {
      className: "files__item", id: "row",
      onPointerDown: (e: never) => press?.(["/work/a/notes.md"], e),
    }, "notes.md"),
    createElement("div", { [HAND_ATTR]: "1", id: "one" }),
    createElement("div", { [HAND_ATTR]: "2", id: "two" }));
}

/** A press, a move and a release, as a browser delivers them through a captured pointer. */
function pointer(kind: string, x: number, y: number, button = 0): PointerEvent {
  const e = new MouseEvent(kind, { bubbles: true, clientX: x, clientY: y, button });
  Object.defineProperty(e, "pointerId", { value: 7 });
  return e as PointerEvent;
}

async function down(x: number, y: number, button = 0) {
  await act(async () => {
    document.getElementById("row")?.dispatchEvent(pointer("pointerdown", x, y, button));
  });
}

async function to(kind: "pointermove" | "pointerup" | "pointercancel", x: number, y: number) {
  await act(async () => {
    document.getElementById("row")?.dispatchEvent(pointer(kind, x, y));
    // The hit test is deferred to a frame, which jsdom runs as a timer.
    await new Promise((r) => setTimeout(r, 20));
  });
}

/** The row travels far enough to become a drag, over whatever `under` is answering with. */
async function carryTo(x: number) {
  await to("pointermove", x, 100);
}

beforeEach(() => {
  landed = [];
  running = ["1", "2"];
  press = null;
  overFrame = null;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  // jsdom has no pointer capture at all; the gesture only ever asks for it and gives it back.
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
  Element.prototype.hasPointerCapture = () => false;
  (globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  act(() => root.render(createElement(Face)));
  under(null);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  document.body.className = "";
});

describe("the pane a row is over", () => {
  it("is the pane drawn under it, and none where there is no pane", () => {
    under(document.getElementById("two"));
    expect(paneUnder(10, 10)).toBe("2");
    under(null);
    expect(paneUnder(10, 10)).toBeNull();
  });
});

describe("telling a press from a drag", () => {
  it("does not begin one until the pointer has travelled", async () => {
    await down(100, 100);
    under(document.getElementById("one"));
    await to("pointermove", 100 + DRAG_SLOP - 1, 100);
    expect(document.querySelector(".files__ghost"), "a press that barely moved took the row")
      .toBeNull();
    expect(overFrame).toBeNull();

    await to("pointermove", 100 + DRAG_SLOP + 4, 100);
    // What follows the pointer is the row's own node, copied.
    expect(document.querySelector(".files__ghost")?.textContent).toBe("notes.md");
    expect(overFrame).toBe("1");
  });

  /** A press that never became a drag is a click, and the row opens the file as it always did. */
  it("lands nothing, and leaves nothing behind, when the press did not travel", async () => {
    await down(100, 100);
    await to("pointerup", 101, 100);
    expect(landed).toEqual([]);
    expect(document.querySelector(".files__ghost")).toBeNull();
    expect(document.body.classList.contains("is-dragging")).toBe(false);
    let reached = false;
    container.addEventListener("click", () => { reached = true; });
    document.getElementById("row")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(reached, "a row that was pressed and not carried did not open").toBe(true);
  });

  // 🚨 Stopping it takes stopping the event, not preventing its default: the row's handler is
  // React's, hung on the tree's root rather than on the row (`../screens/boardDrag`).
  it("keeps the click a finished drag ends in from opening the file", async () => {
    let reached = 0;
    container.addEventListener("click", () => { reached += 1; });

    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);
    await to("pointerup", 300, 100);
    document.getElementById("row")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(reached, "the file opened under a drag that had just finished").toBe(0);

    // Taken once. The next real click is somebody pressing the row, and it opens.
    document.getElementById("row")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(reached).toBe(1);
  });

  /** A right press on a row is its menu; taking it would put the row in hand with no way down. */
  it("ignores a press that is not the main button", async () => {
    await down(100, 100, 2);
    under(document.getElementById("one"));
    await carryTo(300);
    expect(document.querySelector(".files__ghost")).toBeNull();
  });
});

describe("letting a row go", () => {
  it("hands the face the pane it came down on, with the path the row stands for", async () => {
    await down(100, 100);
    under(document.getElementById("two"));
    await carryTo(300);
    expect(overFrame).toBe("2");

    await to("pointerup", 300, 100);
    expect(landed).toEqual([["2", ["/work/a/notes.md"]]]);
    expect(overFrame).toBeNull();
    expect(document.querySelector(".files__ghost")).toBeNull();
    expect(document.body.classList.contains("is-dragging")).toBe(false);
  });

  it("lands nothing where it came down on no pane at all", async () => {
    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);
    under(null);
    await to("pointerup", 900, 900);
    expect(landed, "a row let go beside the panes was handed to one anyway").toEqual([]);
  });

  /** A pane with nothing running in it has nowhere to put a path, so it neither lights up nor
   *  receives — and the answer is taken at the moment it is needed, not at the press. */
  it("neither offers nor receives where the pane has nothing running in it", async () => {
    running = [];
    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);
    expect(overFrame, "a pane with nothing running in it offered to take a row").toBeNull();

    await to("pointerup", 300, 100);
    expect(landed).toEqual([]);
  });

  /** The press outliving the gesture would leave the page marked as dragging with nothing held. */
  it("puts the fences down when the gesture is cancelled", async () => {
    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);
    expect(document.body.classList.contains("is-dragging")).toBe(true);

    await to("pointercancel", 300, 100);
    expect(landed).toEqual([]);
    expect(document.querySelector(".files__ghost")).toBeNull();
    expect(document.body.classList.contains("is-dragging")).toBe(false);
  });
});
