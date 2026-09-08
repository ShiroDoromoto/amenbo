// @vitest-environment jsdom
// The files a project was left reading, when the reader goes to another project and comes back.
//
// Moving to another project is not closing what was open on this one. Each project's tabs are held
// under that project and the face draws the one it is on, so the next project comes up with nothing
// open and the one left behind is found as it was left — the same tabs, and the same one on top
// (`AMB-D-835`, `./TerminalFace`).
//
// It holds for the run and no further: nothing here is written down, and what a restart comes back
// with is not answered by this.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";
import type { OpenFile } from "../files/FilesPanel";

const hoisted = vi.hoisted(() => ({
  /** The face's way of opening a file, taken off the tree it hands it to. */
  read: undefined as undefined | ((at: OpenFile) => void),
  /** And its way of hearing that rows have gone to the bin. */
  gone: undefined as undefined | ((root: string, went: string[]) => void),
  /** What the reading column was last handed: the files it holds, and the one on top. */
  drawn: { open: [] as readonly OpenFile[], reading: null as OpenFile | null },
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, dir: string) => void },
    start: PaneStart = {},
  ) => {
    on.opened(start.session ?? "s1", "/repo");
    return Promise.resolve(() => {});
  },
}));

// Neither column draws anything here. What is under test is what the face holds and hands them: the
// tree is where a file is opened from, and the panel is what it is drawn in.
vi.mock("../files/FolderTree", () => ({
  FolderTree: (props: {
    onRead?: (at: OpenFile) => void;
    onGone?: (root: string, went: string[]) => void;
  }) => {
    hoisted.read = props.onRead;
    hoisted.gone = props.onGone;
    return null;
  },
}));
vi.mock("../files/FilesPanel", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../files/FilesPanel")>()),
  FilesPanel: (props: { open: readonly OpenFile[]; reading: OpenFile | null }) => {
    hoisted.drawn = { open: props.open, reading: props.reading };
    return null;
  },
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: {
    listProjects: () => [
      { id: 1, name: "amenbo", color: "#101820", icon: null },
      { id: 2, name: "the site", color: "#ffe066", icon: null },
    ],
  },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/repo", exists: true }],
    live: [{ path: "/repo", exists: true }],
    answered: true,
  }),
}));

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const README: OpenFile = { root: "/repo", path: ["README.md"] };
const MAKEFILE: OpenFile = { root: "/repo", path: ["Makefile"] };

const mount = async () => {
  await act(async () => {
    root.render(createElement(TerminalFace, { onWindow: () => {}, note: null }));
    await new Promise((r) => setTimeout(r, 0));
  });
};

/** Open a file the way the tree in the rail does. */
const read = async (at: OpenFile) => {
  await act(async () => { hoisted.read?.(at); });
};

/** Go to a project by pressing its tab, which is the way to another project (`./ProjectTabs`). */
const goTo = async (name: string) => {
  const tab = [...container.querySelectorAll<HTMLElement>(".ptabs__tab")]
    .find((one) => one.querySelector(".ptabs__name")?.textContent === name);
  await act(async () => { tab?.click(); });
};

/** The tabs the reading column is drawing, by the file each one is. */
const holding = () => hoisted.drawn.open.map((one) => one.path.join("/"));
const onTop = () => hoisted.drawn.reading?.path.join("/") ?? null;

beforeEach(() => {
  // The widths and the wish are kept on the device, so a run that inherited the last one's would be
  // drawing the test before it (`../talk/columns`).
  localStorage.clear();
  // Wide enough for the panes to have columns beside them rather than drawers.
  window.innerWidth = 1600;
  hoisted.read = undefined;
  hoisted.gone = undefined;
  hoisted.drawn = { open: [], reading: null };
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the files a project is left reading", () => {
  it("comes back to the project it was opened on, in the order and with the one on top", async () => {
    await mount();
    await read(README);
    await read(MAKEFILE);
    expect(holding()).toEqual(["README.md", "Makefile"]);
    expect(onTop()).toBe("Makefile");

    await goTo("the site");
    expect(holding()).toEqual([]);
    expect(onTop()).toBeNull();

    await goTo("amenbo");
    expect(holding()).toEqual(["README.md", "Makefile"]);
    expect(onTop()).toBe("Makefile");
  });

  // Each project answers for its own: opening a file on the second one says nothing about what the
  // first was left holding.
  it("holds one answer per project", async () => {
    await mount();
    await read(README);
    await goTo("the site");
    await read(MAKEFILE);
    expect(holding()).toEqual(["Makefile"]);

    await goTo("amenbo");
    expect(holding()).toEqual(["README.md"]);
  });

  // A file binned is a file that is not there, and the project it was binned on is the one that lets
  // go of it — coming back must not draw a tab for a row the tree no longer has (`onGone`).
  it("does not bring back a file that went to the bin", async () => {
    await mount();
    await read(README);
    await read(MAKEFILE);
    await act(async () => { hoisted.gone?.("/repo", ["Makefile"]); });
    expect(holding()).toEqual(["README.md"]);

    await goTo("the site");
    await goTo("amenbo");
    expect(holding()).toEqual(["README.md"]);
    expect(onTop()).toBe("README.md");
  });
});
