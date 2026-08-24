// @vitest-environment jsdom
// What a window comes up as, and what it must not come up as (`AMB-T-3607`, `AMB-T-3687`).
//
// **After a run there are no frames.** What the app kept is the split the person chose and the
// project they were looking at, so the face comes up laid out their way with one way in on it — and
// not with the places they opened last time, which would be empty boxes drawn exactly like that way
// in, saying nothing except that something used to be there.
//
// **Inside a run there are.** The arrangement is how the two windows share one face, so a window that
// reads one with panes in it draws them — as places, with nothing started in any of them: a session
// is a process, and the pane is the offer to start one (`../talk/layout`).
//
// Both halves are invisible in code that looks right either way. A face that started what it drew
// would look like a face that remembered well, and the reader would find agents running that nobody
// asked for. A face that wrote its own opening arrangement before the answer came back would look
// like a face that had nothing to read — and in the window a terminal was split out into, the panes
// it was split out of would be gone.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { tn } from "../core/i18n";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  mounts: [] as { start: PaneStart }[],
  kept: [] as unknown[],
  /** The arrangement the host answers with, and the hand that lets it answer late. */
  saved: null as unknown,
  answer: null as null | (() => void),
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string) => void },
    start: PaneStart = {},
  ) => {
    hoisted.mounts.push({ start });
    on.opened(start.session ?? `s${hoisted.mounts.length}`, "2026-08-24T00:00:00Z");
    return Promise.resolve(() => {});
  },
}));

vi.mock("../talk/frames", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/frames")>()),
  frameNames: async () => new Map(),
  nameFrame: async () => new Map(),
  savedLayout: () =>
    new Promise((resolve) => {
      hoisted.answer = () => resolve(hoisted.saved);
    }),
  keepLayout: async (layout: unknown) => { hoisted.kept.push(layout); },
}));

// The host is not here, but the gate this is about only closes inside Tauri: outside it there is
// nothing to read and nothing to wait for.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));

// The file face beside the panes, stood out: it watches folders on the host, and inside Tauri — which
// is what this file pretends to be — that is a read nothing here can answer.
vi.mock("../files/FilesPanel", () => ({ FilesPanel: () => null }));

// The ledger's projects and the one folder this one is bound to. A pane carries its own project, so
// what these answer for is the face's opening one and the way in on an empty face.
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/work/repo", exists: true }],
    live: [{ path: "/work/repo", exists: true }],
    answered: true,
  }),
}));

// Nothing is running: what a session with no pane does to the face is `AMB-D-753`'s and not this
// file's. Only that one read is answered — everything else goes on to the host that is not here, and
// is caught by whoever asked, which is what the rest of the window does in a browser anyway.
vi.mock("../core/ipc", async (importOriginal) => {
  const real = await importOriginal<typeof import("../core/ipc")>();
  return {
    ...real,
    invoke: async (cmd: string, args?: Record<string, unknown>) =>
      (cmd === "pty_sessions" ? [] : real.invoke(cmd, args)),
  };
});

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];
/** The split the control says is on, as a number of panes to a page. */
const splitOn = () => container.querySelector(".termface__count--on")?.textContent ?? null;

/** Let the host answer, and let React settle around it. */
async function answered() {
  await act(async () => {
    hoisted.answer?.();
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** Put the face up and let its first reads go out. */
async function mount() {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root.render(createElement(TerminalFace, { onWindow: () => {}, note: null, onWaiting: () => {} }));
  });
}

beforeEach(async () => {
  // The face measures the window to work out whether the columns beside the panes are columns at all
  // (`../talk/columns`). jsdom's window is 1024, which is genuinely too narrow for two panes and two
  // columns — so a test about what is drawn beside the panes says it is on a wide screen.
  window.innerWidth = 1600;
  hoisted.mounts = [];
  hoisted.kept = [];
  hoisted.answer = null;
  // What the app kept of the last run: the split, and the project the face was on. There are no
  // frames in it, and there is no id to carry over, because neither is kept (`../talk/layout`).
  hoisted.saved = { count: 4, nextId: 1, project: 1, frames: [] };
  await mount();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the first window of a run", () => {
  it("draws nothing into a slot until the host has answered", () => {
    // A pane put up first and replaced afterwards would start a terminal in a frame the answer was
    // about to take away.
    expect(hoisted.mounts).toHaveLength(0);
    expect(q(".slot")).toHaveLength(0);
  });

  it("comes up on the split that was kept, with one way in and nothing running", async () => {
    await answered();
    // The panes are gone and the split is not: what a person set is theirs to come back to, and what
    // they opened died with the run.
    expect(splitOn()).toBe(tn("face.panes", 4));
    expect(q(".slot--empty")).toHaveLength(1);
    expect(q(".slot")).toHaveLength(1);
    expect(hoisted.mounts).toHaveLength(0);
  });

  it("writes the arrangement back with nothing in it, and nobody working anywhere", async () => {
    // A write before the answer would be an arrangement overwritten by a face that had not read it
    // yet — in the other window, the panes it was split out of.
    expect(hoisted.kept).toHaveLength(0);
    await answered();
    expect(hoisted.kept[hoisted.kept.length - 1]).toEqual({ count: 4, nextId: 1, project: 1, frames: [] });
  });

  it("comes up with the way in and nothing running where nothing was kept at all", async () => {
    hoisted.saved = null;
    await answered();
    // A pane is made by opening one: a face with nothing to read starts nothing at all.
    expect(hoisted.mounts).toHaveLength(0);
    expect(q(".slot--empty")).toHaveLength(1);
  });
});

describe("a window that reads an arrangement with panes in it", () => {
  beforeEach(async () => {
    act(() => root.unmount());
    container.remove();
    hoisted.saved = {
      count: 2,
      nextId: 3,
      project: 1,
      frames: [
        { id: "1", project: 1, folder: "/work/repo" },
        { id: "2", project: 1, folder: "/work/repo" },
      ],
    };
    await mount();
  });

  it("draws them as places to open a terminal in, and starts none of them", async () => {
    await answered();
    // Two frames, each drawn as the offer to start one — which is what a frame with nothing running
    // in it is (`./TerminalPane`).
    expect(q(".slot")).toHaveLength(2);
    expect(q(".slot__open")).toHaveLength(2);
    expect(hoisted.mounts).toHaveLength(0);
  });

  it("starts the one that is pressed, and only that one", async () => {
    await answered();
    await act(async () => {
      q(".slot__open")[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(hoisted.mounts).toHaveLength(1);
    // In the folder that frame is working in, which is the other half of what came over.
    expect(hoisted.mounts[0]!.start.cwd).toBe("/work/repo");
  });

  it("keeps the shape as it changes, and keeps no session in it", async () => {
    await answered();
    await act(async () => {
      q(".slot__open")[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 0));
    });
    // A terminal opening is not a change of shape: what is written is where the panes are.
    expect(JSON.stringify(hoisted.kept)).not.toContain("session");
    // And the pane being worked in goes with it, for the window a terminal is split out into: a face
    // that read this one lands on the first place, which is where the person is (`../talk/layout`).
    expect(hoisted.kept[hoisted.kept.length - 1]).toEqual({ ...(hoisted.saved as object), splitOut: "1" });
  });
});
