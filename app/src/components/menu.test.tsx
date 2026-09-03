// @vitest-environment jsdom
// The menu shell, on its own rather than through one of the menus that wear it.
//
// What is pinned here is the machinery the callers do not write: which keys close it and which are
// its own, where the focus is when it opens and where it goes when it leaves. Nothing about items —
// those belong to whoever is drawing them.
import { act, createElement, useState, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { Menu, MenuItem } from "./Menu";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
/** How many times the menu asked to be closed — the shell's one answer to the outside. */
let closed = 0;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  closed = 0;
});

afterEach(() => {
  act(() => { root.unmount(); });
  container.remove();
});

const AT = { x: 10, y: 20 };

/** The menu with three plain items, opened from a row that already holds the focus. */
async function open(items: ReactNode = ["one", "two", "three"].map((one) => (
  createElement(MenuItem, { key: one, onClick: () => {}, children: one })
)), at = AT) {
  const row = document.createElement("button");
  row.textContent = "row";
  document.body.appendChild(row);
  row.focus();
  await act(async () => {
    root.render(createElement(Menu, {
      at,
      onClose: () => { closed += 1; },
      children: items,
    }));
  });
  return row;
}

/** The box as the browser drew it. jsdom lays nothing out, so a size is what the test says it is. */
function boxIs(width: number, height: number) {
  const drawn = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function measured(this: Element) {
    if (!this.classList.contains("menu")) return drawn.call(this);
    return { width, height, x: 0, y: 0, top: 0, left: 0, right: width, bottom: height,
      toJSON: () => ({}) } as DOMRect;
  };
  return () => { Element.prototype.getBoundingClientRect = drawn; };
}

/** Where the box ended up, as the two numbers it is placed by. */
const placed = () => {
  const el = document.querySelector<HTMLElement>(".menu")!;
  return { left: el.style.left, top: el.style.top };
};

const items = () => [...document.querySelectorAll<HTMLElement>(".menu__item")];

const press = (el: EventTarget, key: string) => act(async () => {
  el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

const pointerDownOn = (el: EventTarget) => act(async () => {
  el.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 0));
});

describe("the menu shell", () => {
  it("stands on its first item, so no key falls out of it", async () => {
    await open();
    expect(items().length).toBe(3);
    expect(document.activeElement).toBe(items()[0]);
  });

  it("walks its own items with the arrows, round at either end", async () => {
    await open();
    await press(document.activeElement!, "ArrowDown");
    expect(document.activeElement).toBe(items()[1]);
    await press(document.activeElement!, "ArrowUp");
    expect(document.activeElement).toBe(items()[0]);
    // Round, because a menu is short and a reader who walks off the end of one means to go on.
    await press(document.activeElement!, "ArrowUp");
    expect(document.activeElement).toBe(items()[2]);
  });

  it("is closed by Escape and by nothing else on the keyboard", async () => {
    await open();
    // It listened for every key once, which shut it on every press meant for the rows behind it.
    await press(document.body, "ArrowDown");
    await press(document.body, "a");
    await press(document.body, "Enter");
    expect(closed).toBe(0);
    await press(document.body, "Escape");
    expect(closed).toBe(1);
  });

  it("is closed by a press outside it and left alone by one inside", async () => {
    await open();
    // Inside is the first half of choosing an item: closing there would unmount the button before
    // the click could land on it.
    await pointerDownOn(items()[0]!);
    expect(closed).toBe(0);
    await pointerDownOn(document.body);
    expect(closed).toBe(1);
  });

  it("hands the focus back to the row it was opened on", async () => {
    const row = await open();
    await act(async () => { root.render(null); });
    expect(document.activeElement).toBe(row);
    row.remove();
  });

  it("leaves the focus where a reader put it themselves", async () => {
    const row = await open();
    const elsewhere = document.createElement("button");
    document.body.appendChild(elsewhere);
    elsewhere.focus();
    await act(async () => { root.render(null); });
    // Taking them back to the row would be undoing what they just said.
    expect(document.activeElement).toBe(elsewhere);
    row.remove();
    elsewhere.remove();
  });

  it("opens at the point it was asked for, where the box fits after it", async () => {
    const back = boxIs(200, 120);
    try {
      await open(undefined, { x: 300, y: 200 });
      // The menu is about the row it was opened on, so it is drawn where the reader pressed and
      // nowhere else while there is room for it there.
      expect(placed()).toEqual({ left: "300px", top: "200px" });
    } finally { back(); }
  });

  it("opens on the other side of the point where the box does not fit after it", async () => {
    const back = boxIs(200, 120);
    try {
      // The press that put this here is the one at the end of a row of tabs: it is always at the
      // right edge, so a box drawn after it is always outside the window.
      await open(undefined, { x: window.innerWidth - 10, y: window.innerHeight - 10 });
      expect(placed()).toEqual({
        left: `${window.innerWidth - 10 - 200}px`,
        top: `${window.innerHeight - 10 - 120}px`,
      });
    } finally { back(); }
  });

  it("stands against the window's edge where the box fits on neither side of the point", async () => {
    // Taller and wider than the window: nowhere it is put shows the whole of it, so it starts at
    // the edge and what is past the cap is scrolled to.
    const back = boxIs(window.innerWidth + 400, window.innerHeight + 400);
    try {
      await open(undefined, { x: 300, y: 200 });
      expect(placed()).toEqual({ left: "4px", top: "4px" });
    } finally { back(); }
  });

  it("caps the box at the window, so a list longer than it is scrolled rather than lost", async () => {
    await open();
    const el = document.querySelector<HTMLElement>(".menu")!;
    expect(el.style.maxHeight).toBe(`${window.innerHeight - 8}px`);
    expect(el.style.maxWidth).toBe(`${window.innerWidth - 8}px`);
  });

  it("puts the reader back on the first item when the face changes under them", async () => {
    function TwoFaces() {
      const [face, setFace] = useState("doors");
      return createElement(Menu, {
        at: AT,
        face,
        onClose: () => {},
        children: face === "doors"
          ? createElement(MenuItem, { onClick: () => setFace("apps"), children: "choose" })
          : ["app one", "app two"].map((one) => (
            createElement(MenuItem, { key: one, onClick: () => {}, children: one })
          )),
      });
    }
    await act(async () => { root.render(createElement(TwoFaces)); });
    await act(async () => {
      items()[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 0));
    });
    // The item they were standing on is gone, and a list nothing stands on is one every key falls
    // out of.
    expect(items().map((one) => one.textContent)).toEqual(["app one", "app two"]);
    expect(document.activeElement).toBe(items()[0]);
  });
});
