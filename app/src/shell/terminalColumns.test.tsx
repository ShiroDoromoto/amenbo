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
import {
  RAIL_DEFAULT, SIDE_NARROW_DEFAULT, SIDE_WIDE_DEFAULT, TABS_COMPACT_WIDTH, TABS_DEFAULT,
} from "../talk/columns";
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
    expect(style.getPropertyValue("--side-w")).toBe(`${SIDE_NARROW_DEFAULT}px`);
  });

  it("each carry the edge their width is dragged by", async () => {
    await mount();
    expect(q(".termface__grip--rail")).not.toBeNull();
    expect(q(".termface__grip--side")).not.toBeNull();
  });
});

// The tab column is the one at the edge that is never closed (`AMB-D-838`). What it buys is being
// told about a turn standing in a project nobody is looking at, and a way to close it would be a way
// to stop being told. Its named width is dragged like the others' (`AMB-D-848`).
describe("the project tabs", () => {
  it("are drawn at the width the face keeps for them, with the edge that drags it", async () => {
    await mount();
    expect(q(".ptabs")).not.toBeNull();
    const style = (q(".termface") as HTMLElement).style;
    expect(style.getPropertyValue("--tabs-w")).toBe(`${TABS_DEFAULT}px`);
    expect(q(".termface__grip--tabs")).not.toBeNull();
  });

  it("stay when both columns beside the panes are closed", async () => {
    await mount();
    await click(bar(t("face.railFolders")));
    await click(bar(t("files.side")));
    expect(q(".termface__column--rail")).toBeNull();
    expect(q(".termface__column--side")).toBeNull();
    expect(q(".ptabs")).not.toBeNull();
  });

  it("give the middle back what their names were taking, and keep the answer", async () => {
    await mount();
    await click(q(".ptabs__fold"));
    const style = () => (q(".termface") as HTMLElement).style.getPropertyValue("--tabs-w");
    expect(style()).toBe(`${TABS_COMPACT_WIDTH}px`);
    await act(async () => root.unmount());
    root = createRoot(container);
    await mount();
    expect(style()).toBe(`${TABS_COMPACT_WIDTH}px`);
  });

  // Folded, what is left is the mark, and a mark is one size: an edge on it would be offering to drag
  // the room around a 24px square.
  it("lose the edge while they are folded, and have it back when the names are", async () => {
    await mount();
    await click(q(".ptabs__fold"));
    expect(q(".termface__grip--tabs")).toBeNull();
    await click(q(".ptabs__fold"));
    expect(q(".termface__grip--tabs")).not.toBeNull();
  });

  // The width the drag left is the device's, not the run's.
  it("come back at the width they were dragged to", async () => {
    localStorage.setItem("amenbo.termface.tabsWidth", "200");
    await mount();
    expect((q(".termface") as HTMLElement).style.getPropertyValue("--tabs-w")).toBe("200px");
  });
});

describe("closing a column, and opening it again", () => {
  it("takes the rail away on the press that brings it back", async () => {
    await mount();
    await click(bar(t("face.railFolders")));
    expect(q(".termface__column--rail")).toBeNull();
    // The press is still there, and it says the rail is not.
    expect(bar(t("face.railFolders"))?.getAttribute("aria-expanded")).toBe("false");
    await click(bar(t("face.railFolders")));
    expect(q(".termface__column--rail")).not.toBeNull();
  });

  it("closes the file face from its own cross and opens it from the top row", async () => {
    await mount();
    await click(q(".files__close"));
    expect(q(".termface__column--side")).toBeNull();
    await click(bar(t("files.side")));
    expect(q(".termface__column--side")).not.toBeNull();
  });

  it("puts the file face away on the press that brings it back", async () => {
    await mount();
    // One control for the column, the way the rail has one: it says the column is up, and pressing
    // it says that is no longer wanted. Which half comes up is the column's own row of tabs.
    await click(bar(t("files.side")));
    expect(q(".termface__column--side")).toBeNull();
    expect(bar(t("files.side"))?.getAttribute("aria-expanded")).toBe("false");
    await click(bar(t("files.side")));
    expect(q(".termface__column--side")).not.toBeNull();
  });

  it("keeps the answer, so a column closed is still closed on the next run", async () => {
    await mount();
    await click(bar(t("face.railFolders")));
    await act(async () => root.unmount());
    root = createRoot(container);
    await mount();
    expect(q(".termface__column--rail")).toBeNull();
  });
});

// Reading in a 256px column is reading through a slot, and a wide one that pushed the panes aside
// would move the pane a reader is about to paste into. So the column has two widths and goes between
// them, and the wide one lies over the panes (`AMB-D-835`).
describe("the two widths the reading column stands on", () => {
  const widthOf = () => (q(".termface") as HTMLElement).style.getPropertyValue("--side-w");

  it("goes wide from the control beside the way out, over the panes rather than beside them", async () => {
    await mount();
    expect(widthOf()).toBe(`${SIDE_NARROW_DEFAULT}px`);

    await click(q(".files__width"));
    expect(widthOf()).toBe(`${SIDE_WIDE_DEFAULT}px`);
    // Over them: the panes keep the room they had, which is what makes the pane a reader is going
    // back to still be where they left it.
    expect(q(".termface__column--wide")).not.toBeNull();
  });

  it("goes back narrow on the next press outside it, and does not close", async () => {
    await mount();
    await click(q(".files__width"));
    expect(q(".termface__column--wide")).not.toBeNull();

    // A press on the panes is a reader going back to the work — and going back to the work is not
    // being finished with the file.
    await act(async () => {
      q(".termface__page-grid")?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(q(".termface__column--wide")).toBeNull();
    expect(q(".termface__column--side")).not.toBeNull();
    expect(widthOf()).toBe(`${SIDE_NARROW_DEFAULT}px`);
  });

  it("leaves the width alone for a press inside the column itself", async () => {
    await mount();
    await click(q(".files__width"));
    await act(async () => {
      q(".termface__column--side")?.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    });
    expect(q(".termface__column--wide")).not.toBeNull();
  });
});

describe("the narrowest window the application opens", () => {
  // 960px is the floor (`app/src-tauri/tauri.conf.json`), and the three floors together are 640px —
  // so a column never has to stop being one (`AMB-D-816`). The tabs come off the window before any
  // of that is measured (`AMB-D-838`), and at their widest they still leave the three their floors.
  it("draws both columns beside the panes, and no drawer over them", async () => {
    window.innerWidth = 960;
    await mount();
    expect(q(".ptabs")).not.toBeNull();
    expect(q(".termface__column--rail")).not.toBeNull();
    expect(q(".termface__column--side")).not.toBeNull();
    expect(q(".termface__drawer")).toBeNull();
  });
});
