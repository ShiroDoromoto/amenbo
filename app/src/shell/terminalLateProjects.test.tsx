// @vitest-environment jsdom
// The face on a machine whose ledger answers after the arrangement does (`AMB-T-4398`).
//
// The two reads the face comes up on are not in step. Which projects there are is read off the
// snapshot, which the window fills in as it lands; the arrangement is read from the host and comes
// back whenever it comes back. On a machine where the snapshot is the slower of the two, the face
// takes its project after the restore has already gone out — and a restore that laid the older
// answer back over it would leave the face with no project at all.
//
// That is invisible in code that looks right, because the page draws nothing while it has no
// project: the reader is left with an empty page, no dashed frame and no way to open a pane, on a
// build where the same page had one a moment ago. It only bites where the arrangement has no frames
// in it to name a project — a project nobody has bound a folder to, which is every machine on its
// first road.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../mock/types";

const hoisted = vi.hoisted(() => ({
  /** The projects the ledger holds. Empty until the snapshot lands, which is the state under test. */
  projects: [] as { id: number; name: string }[],
  /** The arrangement the host answers with, and the hand that lets it answer late. */
  saved: null as unknown,
  answer: null as null | (() => void),
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: () => Promise.resolve(() => {}),
}));

vi.mock("../talk/frames", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/frames")>()),
  frameNames: async () => new Map(),
  nameFrame: async () => new Map(),
  savedLayout: () =>
    new Promise((resolve) => {
      hoisted.answer = () => resolve(hoisted.saved);
    }),
  keepLayout: async () => {},
}));

// The gate the restore waits behind only closes inside Tauri, so the face has to believe it is there.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));

vi.mock("../files/FilesPanel", () => ({ FilesPanel: () => null }));
vi.mock("../files/FolderTree", () => ({ FolderTree: () => null }));
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => hoisted.projects as Project[] },
}));
// Bound to nothing, which is the project this is about: nothing has been opened in it, so the
// arrangement that comes back has no frame to name a project with.
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
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

const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];

const face = () =>
  createElement(TerminalFace, { onWindow: () => {}, note: null, onWaiting: () => {} });

beforeEach(async () => {
  window.innerWidth = 1600;
  hoisted.answer = null;
  hoisted.projects = [];
  // What was kept last run: the split, the project the face was on, and no frames — a project with
  // no folder has never had a pane opened in it (`../talk/layout`).
  hoisted.saved = { count: 1, nextId: 1, project: 1, frames: [] };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  // Up while the ledger still says there are no projects: this is the render the restore goes out on.
  await act(async () => { root.render(face()); });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a ledger that answers after the arrangement went out", () => {
  it("keeps the project the face was told about, and draws the way in", async () => {
    // The snapshot lands, and the face is told which project it is on.
    hoisted.projects = [{ id: 1, name: "amenbo" }];
    await act(async () => { root.render(face()); });
    // Only now does the host answer with the arrangement the face asked for before any of that.
    await act(async () => {
      hoisted.answer?.();
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(q(".slot--empty")).toHaveLength(1);
  });
});
