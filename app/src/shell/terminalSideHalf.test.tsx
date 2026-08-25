// @vitest-environment jsdom
// Which half of the file face is up.
//
// The panel opens on the memo, and the reason is not which of the two is used more: the memo is
// opened by a person who wants it, and nothing is lost by its being closed (`../talk/columns`).
// After the first run the default has nothing to say — what is up is whichever half the person left
// up, which is this device's own answer and is kept between runs.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneEvents, PaneStart } from "../talk/terminal";

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/repo", exists: true }],
    live: [{ path: "/repo", exists: true }],
    answered: true,
  }),
}));

// The frame a slot puts up, stood in for: what is under test is the row above the panes and the
// panel beside them, neither of which needs a terminal to be running.
vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: PaneEvents,
    start: PaneStart = {},
  ) => {
    on.opened(start.session ?? "s1", "2026-08-24T00:00:00Z", "/repo");
    return Promise.resolve(() => {});
  },
}));

import { TerminalFace } from "./TerminalFace";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const q = (sel: string) => container.querySelector<HTMLElement>(sel);
const click = async (el: HTMLElement | null) => {
  await act(async () => { el?.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
};
/** A button on the top row, by what it says. */
const bar = (name: string) =>
  [...container.querySelectorAll<HTMLElement>(".termface__bar button")]
    .find((one) => (one.getAttribute("aria-label") ?? one.textContent) === name) ?? null;
/** The order the file face's two halves are offered in. */
const halves = () =>
  [...container.querySelectorAll<HTMLElement>(".termface__sides button")]
    .map((one) => one.textContent);

const mount = async () => {
  await act(async () => {
    root.render(createElement(TerminalFace, { onWindow: () => {}, note: null, onWaiting: () => {} }));
  });
};

beforeEach(() => {
  // What half is up is kept on the device, so a test that inherited the last one's answer would be
  // testing the run before it (`../talk/columns`).
  localStorage.clear();
  // Wide enough for the panes to have columns beside them rather than drawers.
  window.innerWidth = 1600;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the half the file face opens on", () => {
  it("is the memo, and the memo is offered first", async () => {
    await mount();
    expect(q(".memo__field")).not.toBeNull();
    expect(halves()).toEqual([t("files.memo"), t("files.tab")]);
  });

  it("is whichever half was up last, so the default is only the first run's", async () => {
    await mount();
    await click(bar(t("files.tab")));
    await act(async () => root.unmount());
    root = createRoot(container);
    await mount();
    expect(q(".memo__field")).toBeNull();
  });
});
