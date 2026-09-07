// @vitest-environment jsdom
// Which project the face comes up on after a run of the app (`AMB-T-4517`).
//
// **What is kept is what a person set** (`../talk/layout`): the split of each project, and the
// project they were looking at. A run that ends leaves no panes behind, so the project is all there
// is to say where the reader was — and a face that opened on the ledger's own project instead would
// draw a working screen every time, on the wrong project.
//
// The ledger opens on the first project by itself at a launch, which is nobody answering about the
// terminal. So the project it names is passed on only where the reader went to it, and a face told
// none reads the arrangement. Both screens look right, which is why this is pinned here rather than
// left to the eye.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../mock/types";

const hoisted = vi.hoisted(() => ({
  saved: null as unknown,
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: () => Promise.resolve(() => {}),
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

vi.mock("../files/FilesPanel", () => ({ FilesPanel: () => null }));
vi.mock("../files/FolderTree", () => ({ FolderTree: () => null }));

// Two projects, because a project that is the only one comes back by having nowhere else to be. The
// first is the one a launch puts the ledger on, and the second is the one the last run was left on.
vi.mock("../mock/adapter", () => ({
  dataAdapter: {
    listProjects: () => [
      { id: 1, name: "seedbed" },
      { id: 2, name: "waterside" },
    ] as Project[],
  },
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
      (cmd === "pty_sessions" ? [] : real.invoke(cmd, args)),
  };
});

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** The project the face is on, read off the tab that is marked as the one being shown. */
const shownProject = () =>
  container.querySelector(".ptabs__tab--on")?.getAttribute("aria-label") ?? null;

/** Put the face up, told a project or told none, and let the arrangement come back. */
async function mount(projectId: number | null) {
  await act(async () => {
    root.render(createElement(TerminalFace, {
      onWindow: () => {}, note: null, onWaiting: () => {}, projectId,
    }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  window.innerWidth = 1600;
  // A run that ended: no panes left behind, and the second project with the split it was left at.
  hoisted.saved = {
    count: 2,
    nextId: 1,
    project: 2,
    splits: { 2: { count: 2, orient: "across" } },
    frames: [],
  };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a face that was told no project", () => {
  it("comes up on the project the last run was left on", async () => {
    await mount(null);
    expect(shownProject()).toBe("waterside");
  });

  // Nothing kept is nothing to come back to, and the first project is where a face with no answer
  // starts.
  it("falls back to the first project where nothing was kept", async () => {
    hoisted.saved = null;
    await mount(null);
    expect(shownProject()).toBe("seedbed");
  });
});

describe("a face told the project the reader came from", () => {
  it("opens on that project rather than on the kept one", async () => {
    await mount(1);
    expect(shownProject()).toBe("seedbed");
  });
});
