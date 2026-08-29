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
))) {
  const row = document.createElement("button");
  row.textContent = "row";
  document.body.appendChild(row);
  row.focus();
  await act(async () => {
    root.render(createElement(Menu, {
      at: AT,
      onClose: () => { closed += 1; },
      children: items,
    }));
  });
  return row;
}

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
