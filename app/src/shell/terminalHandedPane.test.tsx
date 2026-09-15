// @vitest-environment jsdom
// The other ask the ledger hands this face: a pane rather than a folder — the place a task or a
// decision was made in (`AMB-D-897`, `../components/MadeIn`, `./AppShell`).
//
// It has a frame of its own beside `terminalHandedFolder.test.tsx` because what it is about is the
// opposite half of the same road. A folder is checked against the project's bindings and opens a
// terminal; a pane is checked against nothing, because the record it came off holds no folder and
// names no provider — so what it leaves is a place, asked where it works and what to run the way
// every new pane is (`./EmptySlot`).
//
// **The id is the whole of it.** A pane opened again under a new id would be a different place: the
// id is what a provider's own home is named after, and what the way back was written down against
// (`crate::pane_home`, `crate::frames::TalkFace`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoundFolderDto } from "../bindings/bindings";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  mounts: [] as { cwd?: string }[],
  bound: new Map<number, string[]>(),
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, where?: string) => void },
    start: PaneStart = {},
  ) => {
    hoisted.mounts.push({ cwd: start.cwd ?? undefined });
    on.opened(`s${hoisted.mounts.length}`, start.cwd ?? undefined);
    return Promise.resolve(() => {});
  },
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "one" }, { id: 2, name: "two" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/work/one", exists: true }],
    live: [{ path: "/work/one", exists: true }],
    answered: true,
  }),
}));

const folder = (path: string): BoundFolderDto =>
  ({ path, exists: true, mismatch: null, legacy: false, pointerMissing: false, foreign: null });

vi.mock("../core/mutations", async (original) => ({
  ...(await original<Record<string, unknown>>()),
  fetchBoundFolders: (projectId: number) =>
    Promise.resolve((hoisted.bound.get(projectId) ?? []).map(folder)),
}));

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const draw = (openIn: { project: number; dir?: string; pane?: string; nth: number } | null) =>
  act(async () => {
    root.render(createElement(TerminalFace, {
      onWindow: () => {}, note: null, openIn,
    }));
  });

/** The ids of the places drawn on this page, in the order they sit (`./TerminalPane`). */
const places = () =>
  [...container.querySelectorAll("[data-hand]")].map((one) => one.getAttribute("data-hand"));

beforeEach(() => {
  window.innerWidth = 1600;
  hoisted.mounts = [];
  hoisted.bound = new Map([[1, ["/work/one", "/work/handed"]], [2, ["/work/two"]]]);
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a pane handed in from the ledger", () => {
  it("makes the place it names, and starts nothing in it", async () => {
    await draw({ project: 1, pane: "a-pane-that-was", nth: 1 });

    // The record names no folder and no provider, so the pane is a place with the question still on
    // it — starting one would be answering both on the reader's behalf.
    expect(places()).toEqual(["a-pane-that-was"]);
    expect(hoisted.mounts, "a terminal was started for a pane nobody had answered for").toEqual([]);
  });

  it("makes it under the id it was given, so asking twice is one place", async () => {
    await draw({ project: 1, pane: "a-pane-that-was", nth: 1 });
    await draw({ project: 1, pane: "a-pane-that-was", nth: 2 });

    // Two places would mean the first was made under an id of its own — and with it, a way back
    // written down against an id nothing opens.
    expect(places()).toEqual(["a-pane-that-was"]);
  });

  it("goes to a pane that is still open rather than making a second one", async () => {
    await draw({ project: 1, dir: "/work/handed", nth: 1 });
    const open = places();
    expect(open).toHaveLength(1);

    await draw({ project: 1, pane: open[0]!, nth: 2 });

    // The same one place, and nothing started in it a second time: what the ask found was the pane
    // itself, not an id to make one under.
    expect(places()).toEqual(open);
    expect(hoisted.mounts).toHaveLength(1);
  });
});
