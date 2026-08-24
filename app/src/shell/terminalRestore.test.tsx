// @vitest-environment jsdom
// What a kept arrangement must and must not bring back (`AMB-T-3607`).
//
// The shape comes back — the panes a person laid out, where they were, and what each was working on
// — because throwing that away is the failure `AMB-D-434` is written against. What must not come
// back is anything that was running: a session died with the last run, so a restored frame is a
// place to open a terminal in, and it stays a place until somebody presses.
//
// Both halves are invisible in code that looks right either way. A face that started the frames it
// restored would look like a face that remembered well, and the reader would find agents running
// that nobody asked for. A face that wrote its own opening arrangement before the kept one came back
// would look like a face that had nothing kept — and the person's panes would be gone for good.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  mounts: [] as { start: PaneStart }[],
  kept: [] as unknown[],
  /** The arrangement the store answers with, and the hand that lets it answer late. */
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

// The store is not here, but the gate this is about only closes inside Tauri: outside it there is
// nothing kept and nothing to wait for.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));

// The file face beside the panes, stood out: it watches folders on the host, and inside Tauri — which
// is what this file pretends to be — that is a read nothing here can answer.
vi.mock("../files/FilesPanel", () => ({ FilesPanel: () => null }));

// The ledger's projects and the one folder this one is bound to. A restored pane carries its own
// project, so what these answer for is the face's opening one and the way in on an empty face.
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

/** Let the store answer, and let React settle around it. */
async function answered() {
  await act(async () => {
    hoisted.answer?.();
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(async () => {
  hoisted.mounts = [];
  hoisted.kept = [];
  hoisted.answer = null;
  hoisted.saved = {
    count: 2,
    nextId: 3,
    // The project the face was on. It comes back with the shape and goes out with it again — the
    // window with no ledger to have been on is what reads it (`../talk/layout`).
    project: 1,
    frames: [
      { id: "1", project: 1, folder: "/work/repo" },
      { id: "2", project: 1, folder: "/work/repo" },
    ],
  };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root.render(createElement(TerminalFace, { onWindow: () => {}, note: null, onWaiting: () => {} }));
  });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("an arrangement that was kept", () => {
  it("draws nothing into a slot until the store has answered", () => {
    // A pane put up first and replaced afterwards would start a terminal in a frame the restore was
    // about to take away.
    expect(hoisted.mounts).toHaveLength(0);
    expect(q(".slot")).toHaveLength(0);
  });

  it("comes back as places to open a terminal in, and starts none of them", async () => {
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
    // In the folder that frame was working in, which is the other half of what was kept.
    expect(hoisted.mounts[0]!.start.cwd).toBe("/work/repo");
  });

  it("never writes anything over the kept one before it has come back", async () => {
    // A write before the answer would be the person's panes gone, and nothing would say it had
    // happened.
    expect(hoisted.kept).toHaveLength(0);
    await answered();
    // What goes back is what came back, with the pane being worked in on it: a restore lands on the
    // first place, and that is where the person is — which is where the window comes up when the
    // terminal is split out (`../talk/layout`).
    expect(hoisted.kept[hoisted.kept.length - 1])
      .toEqual({ ...(hoisted.saved as object), splitOut: "1" });
  });

  it("keeps the shape as it changes, and keeps no session in it", async () => {
    await answered();
    await act(async () => {
      q(".slot__open")[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 0));
    });
    // A terminal opening is not a change of shape: what is kept is where the panes are.
    expect(JSON.stringify(hoisted.kept)).not.toContain("session");
  });

  it("comes up with the way in and nothing running where nothing was kept", async () => {
    hoisted.saved = null;
    await answered();
    // A pane is made by opening one: a face with nothing to restore starts nothing at all.
    expect(hoisted.mounts).toHaveLength(0);
    expect(q(".slot--empty")).toHaveLength(1);
  });
});
