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
import { HAND_ATTR, type Held, INTO_ATTR, paneUnder, useHandDrag, watchCarry } from "./handDrag";

/** jsdom lays nothing out, so what is under the pointer is stated rather than measured. */
function under(el: Element | null): void {
  (document as unknown as { elementFromPoint: () => Element | null }).elementFromPoint = () => el;
}

let container: HTMLDivElement;
let root: Root;
/** The one row this page draws, said both ways round (`./handDrag`). */
const ROW: Held = { wholes: ["/work/a/notes.md"], root: "/work/a", paths: [["notes.md"]] };

/** What the face was told to do with landed rows. */
let landed: [string, string[]][];
/** Which panes have something running in them, which is what decides whether one takes a row. */
let running: string[];
/** The pane the pointer is over, as the face would draw the surface on it. */
let overFrame: string | null;
let press: ((taken: Held, event: unknown) => void) | null;
/** Whether the tree is still drawing the row, which is what a scroll under a held pointer decides. */
let rowThere: boolean;

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
    rowThere ? createElement("li", {
      className: "files__item", id: "row",
      onPointerDown: (e: never) => press?.(ROW, e),
    }, "notes.md") : null,
    createElement("div", { [HAND_ATTR]: "1", id: "one" }),
    createElement("div", { [HAND_ATTR]: "2", id: "two" }),
    // And one folder of the panel the row came from, which is the gesture's other landing.
    createElement("div", { [INTO_ATTR]: "src", "data-root": "/work/a", id: "into" }));
}

/** A press, a move and a release, as a browser delivers them through a captured pointer. */
function pointer(
  kind: string,
  x: number,
  y: number,
  button = 0,
  held: MouseEventInit = {},
): PointerEvent {
  const e = new MouseEvent(kind, { bubbles: true, clientX: x, clientY: y, button, ...held });
  Object.defineProperty(e, "pointerId", { value: 7 });
  return e as PointerEvent;
}

async function down(x: number, y: number, button = 0) {
  await act(async () => {
    document.getElementById("row")?.dispatchEvent(pointer("pointerdown", x, y, button));
  });
}

async function to(
  kind: "pointermove" | "pointerup" | "pointercancel",
  x: number,
  y: number,
  held: MouseEventInit = {},
) {
  await act(async () => {
    document.getElementById("row")?.dispatchEvent(pointer(kind, x, y, 0, held));
    // The hit test is deferred to a frame, which jsdom runs as a timer.
    await new Promise((r) => setTimeout(r, 20));
  });
}

/** Stop drawing the row, the way the tree's window does when the list scrolls under a held row. */
async function unmountRow() {
  rowThere = false;
  await act(async () => { root.render(createElement(Face)); });
}

/** A release the window hears, which is all there is left once the row has gone. */
async function releaseOnWindow(x: number, y: number) {
  await act(async () => {
    window.dispatchEvent(pointer("pointerup", x, y));
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
  rowThere = true;
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

  /**
   * The tree draws a window of its lines at a time, so a list that scrolls under a held row takes
   * that row out of the document. The gesture is the person's and not the row's, so it goes on
   * (`AMB-T-4619`).
   */
  it("carries on after the row it was taken from stops being drawn", async () => {
    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);
    await unmountRow();

    under(document.getElementById("two"));
    await releaseOnWindow(300, 100);
    expect(landed, "the row that stopped being drawn was never handed to the pane")
      .toEqual([["2", ["/work/a/notes.md"]]]);
    expect(document.querySelector(".files__ghost"), "the ghost stayed on the page").toBeNull();
    expect(document.body.classList.contains("is-dragging")).toBe(false);
  });

  /** What the stuck gesture cost was every press after it: `held` stayed full and turned them away. */
  it("takes the next row up after one stopped being drawn mid-carry", async () => {
    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);
    await unmountRow();
    await releaseOnWindow(300, 100);

    rowThere = true;
    await act(async () => { root.render(createElement(Face)); });
    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);
    await to("pointerup", 300, 100);
    expect(landed.length, "no row could be taken up again").toBe(2);
  });

  /**
   * The release that never comes. A pointer let go over the menu bar a window at the top of the
   * screen reveals, or over another application, is one this page is never told about — and what is
   * held then is held until something else puts it down (`AMB-T-4624`).
   */
  it("puts down the row still in hand when a new press lands", async () => {
    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);
    expect(document.querySelectorAll(".files__ghost")).toHaveLength(1);

    // No release: the gesture is simply still held when the next row is pressed.
    await down(100, 100);
    expect(document.querySelectorAll(".files__ghost"), "the row let go of was still in hand")
      .toHaveLength(0);

    under(document.getElementById("two"));
    await carryTo(300);
    await to("pointerup", 300, 100);
    expect(landed, "the press that landed on a held gesture was turned away")
      .toEqual([["2", ["/work/a/notes.md"]]]);
    expect(document.querySelector(".files__ghost")).toBeNull();
    expect(document.body.classList.contains("is-dragging")).toBe(false);
  });

  /** Nobody carries a row across a window they have left, and the release there is never heard. */
  it("puts the row down when the page loses the pointer altogether", async () => {
    await down(100, 100);
    under(document.getElementById("one"));
    await carryTo(300);

    await act(async () => { window.dispatchEvent(new Event("blur")); });
    expect(landed, "a row was handed to a pane by the window going away").toEqual([]);
    expect(document.querySelector(".files__ghost")).toBeNull();
    expect(document.body.classList.contains("is-dragging")).toBe(false);
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

describe("a row let go over a folder of the panel", () => {
  /** What the panel was told, in the order it was told it. */
  let over: (string | null)[];
  let dropped: { into: string | null; taken: Held; copy: boolean }[];
  let stop: (() => void) | null = null;

  beforeEach(() => {
    over = [];
    dropped = [];
    stop = watchCarry({
      over: (into) => over.push(into?.getAttribute(INTO_ATTR) ?? null),
      drop: (into, taken, copy) =>
        dropped.push({ into: into.getAttribute(INTO_ATTR), taken, copy }),
    });
  });

  afterEach(() => {
    stop?.();
    stop = null;
  });

  it("hands the panel the rows as the project knows them, and moves them by default", async () => {
    await down(100, 100);
    under(document.getElementById("into"));
    await carryTo(300);

    expect(over, "the folder under the pointer was not named").toEqual(["src"]);

    await to("pointerup", 300, 100);

    expect(dropped).toEqual([{ into: "src", taken: ROW, copy: false }]);
    // And the highlight is put down with the gesture, whatever it landed on.
    expect(over[over.length - 1]).toBeNull();
  });

  it("says a copy was asked for where the key for one was held", async () => {
    await down(100, 100);
    under(document.getElementById("into"));
    await carryTo(300);

    // jsdom reports no user agent this reads as macOS, so the key is the other two machines'.
    await to("pointerup", 300, 100, { ctrlKey: true });

    expect(dropped.map((one) => one.copy)).toEqual([true]);
  });

  it("names the folder only as the answer changes, not at every frame", async () => {
    await down(100, 100);
    under(document.getElementById("into"));
    await carryTo(300);
    await carryTo(320);
    await carryTo(340);

    expect(over, "the same folder was named again on every frame").toEqual(["src"]);
  });

  it("is a pane's, where the row came down on one", async () => {
    await down(100, 100);
    under(document.getElementById("two"));
    await carryTo(300);
    await to("pointerup", 300, 100);

    expect(landed).toEqual([["2", ROW.wholes]]);
    expect(dropped, "a row let go on a pane was carried into a folder as well").toEqual([]);
  });

  it("is nothing at all where the row came down on neither", async () => {
    await down(100, 100);
    under(document.getElementById("into"));
    await carryTo(300);
    under(null);
    await to("pointerup", 400, 100);

    expect(dropped).toEqual([]);
    expect(landed).toEqual([]);
  });
});
