// @vitest-environment jsdom
// What the columns beside the panes have to do, which the arithmetic behind them cannot say
// (`../talk/columns`): that each of them can be put away, that the way back is on the screen the
// moment it has been, and that a window with no room for columns opens neither over the panes until
// it is asked to.
//
// The pairing is what is really pinned here. A cross with no button to press afterwards is a panel a
// reader loses; a button that only appears on a narrow window is a column that cannot be closed on a
// wide one. Both spellings look right in the code and only one of them is usable.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({ folders: [{ path: "/repo", exists: true }] }));

// The frame a slot puts up, stood in for: what is under test is what is drawn *beside* the panes.
vi.mock("../talk/agent", () => ({
  mountAgentFrame: () => Promise.resolve(() => {}),
}));
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: hoisted.folders, live: hoisted.folders, answered: true }),
}));

import { TerminalFace } from "./TerminalFace";
import { PANE_MIN, RAIL_DEFAULT, SIDE_DEFAULT } from "../talk/columns";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const q = (sel: string) => container.querySelector<HTMLElement>(sel);
const click = async (el: HTMLElement | null) => {
  await act(async () => { el?.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
};
/** A button on the top row, by what it is called — which for the rail's is not what it says: the
 *  control is drawn as bars and named after what it opens. */
const bar = (name: string) =>
  [...container.querySelectorAll<HTMLElement>(".termface__bar button")]
    .find((one) => (one.getAttribute("aria-label") ?? one.textContent) === name) ?? null;

const mount = async () => {
  await act(async () => {
    root.render(createElement(TerminalFace, { onWindow: () => {}, note: null, onWaiting: () => {} }));
  });
};

beforeEach(() => {
  // The wish and the widths are this device's, and they are kept: a test that inherited the last
  // one's answers would be testing the run before it.
  localStorage.clear();
  // Wide enough for two panes with both columns beside them (`../talk/columns`).
  window.innerWidth = 1600;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the columns beside the panes", () => {
  it("are both there on a window with room for them, each at the width it starts at", async () => {
    await mount();
    expect(q(".termface__column--rail")).not.toBeNull();
    expect(q(".termface__column--side")).not.toBeNull();
    const style = (q(".termface") as HTMLElement).style;
    expect(style.getPropertyValue("--rail-w")).toBe(`${RAIL_DEFAULT}px`);
    expect(style.getPropertyValue("--side-w")).toBe(`${SIDE_DEFAULT}px`);
  });

  it("each carry the edge their width is dragged by", async () => {
    await mount();
    expect(q(".termface__grip--rail")).not.toBeNull();
    expect(q(".termface__grip--side")).not.toBeNull();
  });
});

describe("closing a column, and opening it again", () => {
  it("takes the rail away on the press that brings it back", async () => {
    await mount();
    await click(bar(t("face.rail")));
    expect(q(".termface__column--rail")).toBeNull();
    // The press is still there, and it says the rail is not.
    expect(bar(t("face.rail"))?.getAttribute("aria-expanded")).toBe("false");
    await click(bar(t("face.rail")));
    expect(q(".termface__column--rail")).not.toBeNull();
  });

  it("closes the file face from its own cross and opens it from the top row", async () => {
    await mount();
    await click(q(".files__close"));
    expect(q(".termface__column--side")).toBeNull();
    await click(bar(t("files.tab")));
    expect(q(".termface__column--side")).not.toBeNull();
  });

  it("puts the file face away when the half already up is pressed again", async () => {
    await mount();
    // It opens on the files, so pressing the files is asking for what is already there.
    await click(bar(t("files.tab")));
    expect(q(".termface__column--side")).toBeNull();
    // The other half is not "close it again" — it is the half to show.
    await click(bar(t("files.memo")));
    expect(q(".termface__column--side")).not.toBeNull();
  });

  it("keeps the answer, so a column closed is still closed on the next run", async () => {
    await mount();
    await click(bar(t("face.rail")));
    await act(async () => root.unmount());
    root = createRoot(container);
    await mount();
    expect(q(".termface__column--rail")).toBeNull();
  });
});

describe("a window with no room for columns", () => {
  // Narrow enough that both columns together leave the middle under one pane's worth of floor
  // (`../talk/columns`) — which is the whole of it: the count that was asked for is not in it.
  const NO_ROOM = PANE_MIN + RAIL_DEFAULT + SIDE_DEFAULT - 1;

  it("draws neither of them over the panes until one is asked for", async () => {
    window.innerWidth = NO_ROOM;
    await mount();
    expect(q(".termface__column--rail")).toBeNull();
    expect(q(".termface__drawer")).toBeNull();
    await click(bar(t("face.rail")));
    expect(q(".termface__drawer")).not.toBeNull();
  });

  it("puts a drawer away again on the press that opened it", async () => {
    window.innerWidth = NO_ROOM;
    await mount();
    await click(bar(t("face.rail")));
    await click(bar(t("face.rail")));
    expect(q(".termface__drawer")).toBeNull();
  });
});
