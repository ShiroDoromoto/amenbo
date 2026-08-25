// @vitest-environment jsdom
// The first loop's one press, from the far side: the ledger hands the face a folder and whose project
// it is, and a pane opens in it with nothing asked (`../components/FirstLoop`, `./AppShell`).
//
// It has a frame of its own rather than riding in `terminalLayout.test.tsx` because the question is
// about the press arriving from the *other* face: what it has to do is put a pane where there was
// none, go to the one that is already there, land in the project the ledger named rather than the one
// the rail happened to be on, and refuse a folder that project is not bound to.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoundFolderDto } from "../bindings/bindings";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  mounts: [] as { cwd?: string }[],
  // Which folders each project is bound to, as the face reads them back when a press arrives.
  bound: new Map<number, string[]>(),
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string, where?: string) => void },
    start: PaneStart = {},
  ) => {
    hoisted.mounts.push({ cwd: start.cwd ?? undefined });
    on.opened(`s${hoisted.mounts.length}`, "2026-08-24T00:00:00Z", start.cwd ?? undefined);
    return Promise.resolve(() => {});
  },
}));

// Two projects for the rail, and one folder each — the press being about a folder the ledger has
// already settled, none of it is asked here.
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

// The bindings the face checks the handed pair against — the one thing standing between a folder and
// a pane under a project it does not belong to.
vi.mock("../core/mutations", async (original) => ({
  ...(await original<Record<string, unknown>>()),
  fetchBoundFolders: (projectId: number) =>
    Promise.resolve((hoisted.bound.get(projectId) ?? []).map(folder)),
}));

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const draw = (openIn: { project: number; dir: string; nth: number } | null) =>
  act(async () => {
    root.render(createElement(TerminalFace, {
      onWindow: () => {}, note: null, onWaiting: () => {}, openIn,
    }));
  });

const shownProject = () =>
  container.querySelector(".rail__project--on")!.textContent;

beforeEach(() => {
  // The face measures the window to work out whether the columns beside the panes are columns at all
  // (`../talk/columns`). jsdom's window is 1024, which is genuinely too narrow for two panes and two
  // columns — so a test about what is drawn beside the panes says it is on a wide screen.
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

describe("a folder handed in from the ledger", () => {
  it("opens a pane in it, where the face had only the way in", async () => {
    await draw(null);
    expect(hoisted.mounts, "a terminal was started before anybody asked for one").toEqual([]);
    expect(container.querySelector(".slot--empty")).not.toBeNull();

    await draw({ project: 1, dir: "/work/handed", nth: 1 });

    expect(hoisted.mounts.map((one) => one.cwd)).toEqual(["/work/handed"]);
  });

  it("goes to it rather than starting a second terminal in a folder that has one", async () => {
    await draw({ project: 1, dir: "/work/handed", nth: 1 });
    const after = hoisted.mounts.length;

    await draw({ project: 1, dir: "/work/handed", nth: 2 });

    expect(hoisted.mounts).toHaveLength(after);
  });

  it("lands in the project the ledger named, taking the screen there", async () => {
    await draw({ project: 1, dir: "/work/one", nth: 1 });
    expect(shownProject()).toBe("one");

    await draw({ project: 2, dir: "/work/two", nth: 2 });

    expect(hoisted.mounts.map((one) => one.cwd)).toEqual(["/work/one", "/work/two"]);
    // The other project's pane is not on this screen: what is shown is one project's panes.
    expect(shownProject()).toBe("two");
    expect(container.querySelectorAll(".rail__row")).toHaveLength(1);
  });

  it("opens nothing where the folder is not one the named project is bound to", async () => {
    await draw({ project: 2, dir: "/work/one", nth: 1 });

    expect(hoisted.mounts, "a pane was made under a project the folder does not belong to").toEqual([]);
    // And not under the folder's own project either: what was asked for is a pair, and there is no
    // half of it to fall back on.
    expect(container.querySelector(".slot--empty")).not.toBeNull();
  });
});
