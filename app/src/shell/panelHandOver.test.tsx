// @vitest-environment jsdom
// Handing a file from the panel to the pane the reader is working in (`AMB-D-820`).
//
// Nothing is carried — what goes into the terminal is the path the file is at — so what the face has
// to get right is **which** pane, and that is the one thing no unit below it can answer. The
// panel is one thing beside as many panes as the page holds, and a face that handed the file to the
// first of them draws exactly like one that hands it to the focused one.
//
// The other half is the item not being there at all: a face whose focused pane has nothing running
// hands nothing down, and the row's menu then has no item to draw (`../files/FilesPanel`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  saved: null as unknown,
  running: [] as { session: string; startedAt: string; folder: string | null }[],
  /** What the face handed the panel, or nothing where it handed it none. */
  handOver: undefined as ((wholes: string[]) => void) | undefined,
  /** Every paste the face asked for: the session it named, and the text. */
  pasted: [] as { session: string; text: string }[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string) => void },
    start: PaneStart = {},
  ) => {
    on.opened(start.session ?? "unasked", "2026-08-25T00:00:00Z");
    return Promise.resolve(() => {});
  },
}));

vi.mock("../talk/terminal", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/terminal")>()),
  pasteIntoTerminal: async (session: string, text: string) => {
    hoisted.pasted.push({ session, text });
  },
}));

vi.mock("../talk/frames", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/frames")>()),
  frameNames: async () => new Map(),
  nameFrame: async () => new Map(),
  savedLayout: async () => hoisted.saved,
  keepLayout: async () => {},
}));

vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));

// The panel itself draws nothing here: what is under test is what the face hands it.
vi.mock("../files/FilesPanel", () => ({
  FilesPanel: (props: { onHandOver?: (wholes: string[]) => void }) => {
    hoisted.handOver = props.onHandOver;
    return null;
  },
}));

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
    invoke: async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "pty_sessions") return hoisted.running;
      if (cmd === "panes_drawn") return undefined;
      return real.invoke(cmd, args);
    },
  };
});

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

async function mount() {
  await act(async () => {
    root.render(createElement(TerminalFace, {
      onWindow: () => {}, note: null, onWaiting: () => {}, goPane: null,
    }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

/** Press a pane, which is how a reader says which one they are working in (`AMB-D-838`). */
async function focusPane(frame: string) {
  await act(async () => {
    container.querySelector<HTMLElement>(`[data-hand="${frame}"]`)
      ?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  window.innerWidth = 1600;
  hoisted.handOver = undefined;
  hoisted.pasted = [];
  hoisted.saved = {
    count: 2,
    nextId: 3,
    project: 1,
    frames: [
      { id: "1", project: 1, folder: "/work/a" },
      { id: "2", project: 1, folder: "/work/b" },
    ],
  };
  hoisted.running = ["a", "b"].map((one, at) => ({
    session: `s-${one}`,
    startedAt: `2026-08-25T00:00:0${at}Z`,
    folder: `/work/${one}`,
  }));
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("handing a file from the panel to a pane", () => {
  it("puts the path in front of what is running in the pane being worked in", async () => {
    await mount();
    await focusPane("2");
    await act(async () => {
      hoisted.handOver?.(["/work/a/notes.md"]);
      await new Promise((r) => setTimeout(r, 0));
    });
    // The pane the reader is standing on, not the first one on the page.
    expect(hoisted.pasted).toEqual([{ session: "s-b", text: "'/work/a/notes.md'" }]);
  });

  /** The same quoting a pane's own drop does (`AMB-D-801`): a screenshot's name has spaces in it on
   *  all three machines, and an unquoted path splits into two words before the reader sees it. */
  it("quotes the path it hands over", async () => {
    await mount();
    await focusPane("2");
    await act(async () => {
      hoisted.handOver?.(["/work/a/it's a shot.png"]);
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(hoisted.pasted).toEqual([{ session: "s-b", text: "'/work/a/it'\\''s a shot.png'" }]);
  });

  /** Several rows picked out go over together, each quoted on its own — the same line a drop of
   *  several files puts in a pane (`AMB-T-4242`). */
  it("puts every path it is handed in front of what is running, one line", async () => {
    await mount();
    await focusPane("2");
    await act(async () => {
      hoisted.handOver?.(["/work/a/notes.md", "/work/a/a shot.png"]);
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(hoisted.pasted)
      .toEqual([{ session: "s-b", text: "'/work/a/notes.md' '/work/a/a shot.png'" }]);
  });

  it("hands the panel nothing where the pane being worked in has nothing running", async () => {
    hoisted.running = [];
    await mount();
    expect(hoisted.handOver).toBeUndefined();
  });
});
