// @vitest-environment jsdom
// The way back from the ledger to the pane the work is happening in (`AMB-D-758`).
//
// A ref drawn in a pane reaches the task; this is the same road the other way, and it needs two
// things the face is the only holder of. **Which place each running session is drawn in** — the host
// pairs it with what the volatile area says a task is held by, and neither end can answer alone. And
// **what that place is called** — worked out from the panes around it, so a pane reads the same on
// the rail as it does on the task it is holding.
//
// Neither is visible in code that looks right either way: a face that tells the host nothing draws
// exactly like one that does, and the row on the task simply never appears.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneDrawnDto } from "../bindings/bindings";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  saved: null as unknown,
  running: [] as { session: string; startedAt: string; folder: string | null }[],
  /** What the face has told the host about where its terminals are drawn, newest last. */
  told: [] as PaneDrawnDto[][],
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
    invoke: async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "pty_sessions") return hoisted.running;
      if (cmd === "panes_drawn") {
        hoisted.told.push((args as { panes: PaneDrawnDto[] }).panes);
        return undefined;
      }
      return real.invoke(cmd, args);
    },
  };
});

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const focusedName = () =>
  container.querySelector(".rail__row--focused .rail__name")?.textContent ?? null;
const shownPage = () => container.querySelector(".termface__page--on")?.textContent ?? null;
const lastTold = () => hoisted.told[hoisted.told.length - 1] ?? [];

/** Put the face up, with a pane asked for or none, and let the arrangement come back. */
async function mount(goPane?: { session: string; nth: number }) {
  await act(async () => {
    root.render(createElement(TerminalFace, {
      onWindow: () => {}, note: null, onWaiting: () => {}, goPane: goPane ?? null,
    }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  window.innerWidth = 1600;
  hoisted.told = [];
  // Four panes over two pages, each in a folder of its own and each with a terminal running.
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
  };
  hoisted.running = ["a", "b", "c", "d"].map((one, at) => ({
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

describe("what the face tells the host about its panes", () => {
  it("names the place each running session is drawn in, and what that place is called", async () => {
    await mount();
    // Every pane, not only the ones on the page being shown: a task is held wherever it is held, and
    // a reader on the board cannot see which page anything is on.
    expect(lastTold()).toEqual([
      { session: "s-a", frame: "1", label: "a" },
      { session: "s-b", frame: "2", label: "b" },
      { session: "s-c", frame: "3", label: "c" },
      { session: "s-d", frame: "4", label: "d" },
    ]);
  });

  it("says nothing about a place whose terminal has ended", async () => {
    // A frame with no session is a place, and a place is not somewhere the ledger can send anybody:
    // there is nothing running there to be holding a task.
    hoisted.running = [hoisted.running[0]!];
    await mount();
    expect(lastTold()).toEqual([{ session: "s-a", frame: "1", label: "a" }]);
  });
});

describe("the face asked for the pane a task is being worked in", () => {
  it("goes to it, bringing its page up with it", async () => {
    // The third pane is on the second page, which is not where a restore lands by itself — so a face
    // that only moved the focus would leave the reader looking at the wrong two panes.
    await mount({ session: "s-c", nth: 1 });
    expect(focusedName()).toBe("c");
    expect(shownPage()).toContain("2");
  });

  it("does nothing for a session no place here is drawing", async () => {
    // It ended, or it is in the window this face is not. Either way there is nowhere to go, and
    // moving the reader somewhere else would be worse than not moving them.
    await mount({ session: "s-gone", nth: 1 });
    expect(focusedName()).toBe("a");
    expect(shownPage()).toContain("1");
  });
});
