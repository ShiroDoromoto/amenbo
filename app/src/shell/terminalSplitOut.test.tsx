// @vitest-environment jsdom
// What the window the terminal is split out into is.
//
// **It is the face, whole.** The point of the second window is to put the terminal on another
// display, so a window that arrived there with one pane and a way back would be a person carrying a
// terminal out rather than moving where they work (`AMB-D-753`). The rail, the split, the pages and
// the file face all go with it, and the only difference is that the ledger is not in this window.
//
// **And it comes up where the person left.** Which pane they were working in is kept with the shape
// (`../talk/layout`), because the press that splits hands nothing over — there is one arrangement,
// and both windows read it. A window that ignored it would open on the first place of the first
// project, which is somewhere the reader was not.
//
// Neither is visible in code that looks right either way: a window with a pane in it looks like a
// window that was built correctly, whichever pane it is and whatever is missing beside it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  saved: null as unknown,
  /** The terminals the host says are running, as it answers: oldest first
   *  (`crate::pty::pty_sessions`). */
  running: [] as { session: string; startedAt: string; folder: string | null }[],
  /** Which session each pane was put up to draw, in the order the panes were built. */
  drawn: [] as (string | null | undefined)[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string) => void },
    start: PaneStart = {},
  ) => {
    hoisted.drawn.push(start.session);
    on.opened(start.session ?? "s1", "2026-08-24T00:00:00Z");
    return Promise.resolve(() => {});
  },
}));

vi.mock("../talk/frames", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/frames")>()),
  frameNames: async () => new Map(),
  nameFrame: async () => new Map(),
  savedLayout: async () => hoisted.saved,
  keepLayout: async () => {},
}));

// Inside Tauri, because that is where an arrangement is kept at all.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));

// The file face beside the page watches folders on a host that is not here.
vi.mock("../files/FilesPanel", () => ({ FilesPanel: () => null }));

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/work/a", exists: true }],
    live: [{ path: "/work/a", exists: true }],
    answered: true,
  }),
}));

vi.mock("../core/ipc", async (importOriginal) => {
  const real = await importOriginal<typeof import("../core/ipc")>();
  return {
    ...real,
    invoke: async (cmd: string, args?: Record<string, unknown>) =>
      (cmd === "pty_sessions" ? hoisted.running : real.invoke(cmd, args)),
  };
});

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const pressed = vi.fn();

const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];
/** The places drawn on the page that is up, as the frames they are (`../talk/layout`). */
const drawnPanes = () => q(".slot").map((el) => el.getAttribute("data-hand"));
/** Which place the face says is being worked in — read off the pane itself, which is the only thing
 *  that says so now that the list of them is gone (`AMB-D-838`). */
const worked = () => container.querySelector(".slot--focused")?.getAttribute("data-hand") ?? null;

/** Put the face up, in one window or the other, and let the arrangement come back. */
async function mount(ownWindow: boolean) {
  await act(async () => {
    root.render(createElement(TerminalFace, {
      onWindow: pressed, ownWindow, note: null, onWaiting: () => {},
    }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  // The face measures the window to work out whether the columns beside the panes are columns at all
  // (`../talk/columns`). jsdom's window is 1024, which is genuinely too narrow for two panes and two
  // columns — so a test that reads what is beside the panes says it is on a wide screen.
  window.innerWidth = 1600;
  pressed.mockReset();
  hoisted.running = [];
  hoisted.drawn = [];
  // Four panes over two pages, and the person was working in the third — which is on the second
  // page, and is not where a restore lands by itself.
  hoisted.saved = {
    count: 2,
    nextId: 5,
    project: 1,
    frames: [
      { id: "1", project: 1, folder: "/work/a" },
      { id: "2", project: 1, folder: "/work/b" },
      { id: "3", project: 1, folder: "/work/c" },
      { id: "4", project: 1, folder: "/work/d" },
    ],
    splitOut: "3",
  };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the window the terminal is split out into", () => {
  it("draws the whole face — the rail, the split and the pages — and not one pane", async () => {
    await mount(true);
    // The folders of the project are beside the panes, and the panes are the page's own two.
    expect(container.querySelector(".rail")).not.toBeNull();
    expect(drawnPanes()).toHaveLength(2);
    // The split is choosable, and the pages of this project are reachable: without either, the panes
    // beyond the one on screen are panes the reader cannot get to. Two of them, because the split is
    // two panes here and two is the one count that is also asked which way it sits
    // (`../talk/layout`).
    expect(q(".termface__counts")).toHaveLength(2);
    expect(q(".termface__count--glyph")).toHaveLength(2);
    expect(q(".termface__page")).toHaveLength(2);
  });

  it("comes up on the pane that was being worked in, and on its page", async () => {
    await mount(true);
    // The third place, which is the one the arrangement was left split out on.
    expect(worked()).toBe("3");
    // The second page, where that pane is — two panes to a page, and it is the third.
    expect(container.querySelector(".termface__page--on")?.textContent).toContain("2");
  });

  it("leaves the board where a restore lands, on the first place", async () => {
    // The kept pane is the split-out window's to read. Which pane is being worked in *now* is the
    // board's own state, and an old write must not move a reader's place.
    await mount(false);
    expect(worked()).toBe("1");
  });
});

describe("the terminals that were running in the face it left", () => {
  it("puts each back in the place it was opened in, where two panes share a folder", async () => {
    // The folder is all there is to tell two places apart, so what pairs them is the order: the
    // oldest terminal in the oldest place. Paired any other way the two panes trade contents, and
    // each is drawn under the other's name — a name belongs to the place (`../talk/frames`).
    hoisted.saved = {
      count: 2,
      nextId: 3,
      project: 1,
      frames: [
        { id: "1", project: 1, folder: "/work/a" },
        { id: "2", project: 1, folder: "/work/a" },
      ],
      splitOut: "1",
    };
    hoisted.running = [
      { session: "older", startedAt: "2026-08-24T00:00:00Z", folder: "/work/a" },
      { session: "newer", startedAt: "2026-08-24T00:00:09Z", folder: "/work/a" },
    ];
    await mount(true);
    expect(hoisted.drawn).toEqual(["older", "newer"]);
  });
});

describe("the button that changes how many windows the app is", () => {
  it("says the move it makes, from whichever window is being read", async () => {
    // The words, not the row: the button carries a mark before them, and what is pinned here is
    // which way the press goes (`../components/Icon`).
    const says = () => q(".termface__action")[0]!.textContent!.trim();
    await mount(false);
    expect(says()).toBe(t("face.splitOut"));
    expect(q(".termface__action")[0]!.querySelector('[data-icon="newWindow"]')).not.toBeNull();
    await act(() => root.unmount());
    root = createRoot(container);
    await mount(true);
    expect(says()).toBe(t("face.merge"));
    // The same mark either way: what it draws is the arrangement the control is about, and the
    // words are what say which direction this press goes.
    expect(q(".termface__action")[0]!.querySelector('[data-icon="newWindow"]')).not.toBeNull();
  });

  it("hands nothing over — there is one arrangement, and both windows read it", async () => {
    await mount(false);
    await act(async () => {
      q(".termface__action")[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(pressed).toHaveBeenCalledTimes(1);
    expect(pressed.mock.calls[0]).toHaveLength(0);
  });
});
