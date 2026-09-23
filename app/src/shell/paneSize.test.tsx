// @vitest-environment jsdom
// The size a pane takes, picked from its row (`AMB-D-959`).
//
// What these guard: **the mark is the size the pane takes now**, so the row says it before anything
// is opened; **the list opens as the pointer comes onto the mark, and on a press**; **it holds the
// six in order with a name under each, the one standing now framed**; and **a pick is handed on,
// while picking the size it already is hands on nothing**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";
import { PaneSize } from "./PaneSize";
import { BOXES } from "../talk/layout";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const picked = vi.fn();

async function render(size: "whole" | "quarter" = "quarter") {
  await act(async () => { root.render(createElement(PaneSize, { size, onSize: picked })); });
}
const mark = () => container.querySelector<HTMLButtonElement>(".panesize")!;
const list = () => document.body.querySelector(".panesize__list");
const items = () => [...document.body.querySelectorAll<HTMLButtonElement>(".panesize__one")];

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  picked.mockClear();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the size on a pane's row", () => {
  it("draws the size the pane takes now, filled in on the page's grid", async () => {
    await render("quarter");
    const took = mark().querySelector(".panesize__took")!;
    expect(Number(took.getAttribute("width"))).toBe(BOXES.quarter.across * 2 - 1);
    expect(Number(took.getAttribute("height"))).toBe(BOXES.quarter.down * 4 - 1);
    expect(list()).toBeNull();
  });

  it("opens the list as the pointer comes onto the mark", async () => {
    await render();
    await act(async () => { mark().dispatchEvent(new MouseEvent("mouseover", { bubbles: true })); });
    expect(list()).not.toBeNull();
  });

  it("opens the list on a press, for a hand that does not hover", async () => {
    await render();
    await act(async () => { mark().click(); });
    expect(list()).not.toBeNull();
  });

  it("holds the six sizes in order, each named, the one standing now framed", async () => {
    await render("quarter");
    await act(async () => { mark().click(); });
    expect(items().map((one) => one.textContent)).toEqual([
      t("pane.size.whole"),
      t("pane.size.half"),
      t("pane.size.halfDown"),
      t("pane.size.quarter"),
      t("pane.size.sixth"),
      t("pane.size.eighth"),
    ]);
    const now = items().filter((one) => one.getAttribute("aria-checked") === "true");
    expect(now.map((one) => one.textContent)).toEqual([t("pane.size.quarter")]);
  });

  it("hands on the size picked, and closes", async () => {
    await render("quarter");
    await act(async () => { mark().click(); });
    await act(async () => { items()[0].click(); });
    expect(picked).toHaveBeenCalledWith("whole");
    expect(list()).toBeNull();
  });

  it("stays open when the pointer that opened it presses the mark", async () => {
    await render();
    await act(async () => { mark().dispatchEvent(new MouseEvent("mouseover", { bubbles: true })); });
    await act(async () => { mark().click(); });
    expect(list()).not.toBeNull();
  });

  it("hands on nothing for the size the pane already is", async () => {
    await render("quarter");
    await act(async () => { mark().click(); });
    await act(async () => { items()[3].click(); });
    expect(picked).not.toHaveBeenCalled();
  });
});
