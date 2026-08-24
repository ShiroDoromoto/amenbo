// @vitest-environment jsdom
// Who gets knocked about, and what a knock is allowed to mean (`AMB-T-3610`, `AMB-D-748`).
//
// A dot on a page says "somebody is needed there". It goes up for the two things that say so — the
// agent handing a turn over, and the ledger saying a task the pane is holding is no longer ready —
// and for nothing else. **Silence must never raise it**: a pane that has said nothing for an hour is
// a pane that has said nothing, and a dot over it would be Amenbo claiming a turn nobody declared.
//
// ⌘J is the other half: the reader goes to the pane rather than the pane coming to the reader, since
// pulling a screen out from under somebody who is reading it is the thing the badge exists to avoid.
// It moves and sends nothing.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  /** Each pane's way of telling the face a turn is standing in it. */
  tell: [] as ((waiting: boolean) => void)[],
  typed: [] as string[],
}));

// The ledger's projects and the one folder this one is bound to, so opening a pane asks nothing.
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({
    all: [{ path: "/repo", exists: true }],
    live: [{ path: "/repo", exists: true }],
    answered: true,
  }),
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string) => void },
    _paneClass: string,
    start: PaneStart = {},
  ) => {
    on.opened(start.session ?? `s${hoisted.tell.length + 1}`, "2026-08-24T00:00:00Z");
    return Promise.resolve(() => {});
  },
}));

// The row above the pane, stood in for: what it draws is its own business, and the one thing the
// face takes from it is whether a turn is standing there.
vi.mock("../talk/plate", () => ({
  mountPlate: (
    _host: HTMLElement,
    _lang: unknown,
    onWaiting: (waiting: boolean) => void = () => {},
  ) => {
    hoisted.tell.push(onWaiting);
    return {
      opened: () => {}, output: () => {}, said: () => {}, closed: () => {},
      named: () => {}, focused: () => {}, stop: () => {},
    };
  },
}));

// Nothing may be sent to a terminal by any of this.
vi.mock("../talk/terminal", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../talk/terminal")>()),
  writeTo: (_session: string, data: string) => { hoisted.typed.push(data); },
}));

import { TerminalFace } from "./TerminalFace";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let told: boolean[];

const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];
const click = async (el: HTMLElement) => {
  await act(async () => { el.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
};
const press = async (key: string) => {
  await act(async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key, metaKey: true, bubbles: true }));
  });
};
/** Open another pane in the project being shown, which is two presses: the way in beside the project's
 *  name on the rail goes to a page with room for one, and the empty frame there opens it. */
const openPane = async () => {
  const room = q(".rail__open")[0];
  if (room) await click(room);
  await click(q(".slot--empty .slot__open")[0]!);
};
/** A pane says a turn is standing in it — or that it is not any more. */
const turn = async (pane: number, standing: boolean) => {
  await act(async () => { hoisted.tell[pane]!(standing); });
};

beforeEach(async () => {
  // The face measures the window to work out whether the columns beside the panes are columns at all
  // (`../talk/columns`). jsdom's window is 1024, which is genuinely too narrow for two panes and two
  // columns — so a test about what is drawn beside the panes says it is on a wide screen.
  window.innerWidth = 1600;
  hoisted.tell = [];
  hoisted.typed = [];
  told = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  await act(async () => {
    root.render(createElement(TerminalFace, {
      onSplitOut: () => {},
      note: null,
      onWaiting: (waiting: boolean) => told.push(waiting),
    }));
  });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a turn standing on a page", () => {
  it("wears no dot while nobody has said a turn is standing", async () => {
    await openPane();
    expect(q(".termface__needs")).toHaveLength(0);
  });

  it("says nothing about the page in front of you, and wears a dot once you leave it", async () => {
    await openPane();
    await turn(0, true);
    // The pane is on the screen and its own row says whose turn it is. A dot here would be the face
    // telling somebody about what they are looking at.
    expect(q(".termface__needs")).toHaveLength(0);
    // The shell is told either way: behind the other face, nothing of this can be seen at all.
    expect(told).toEqual([true]);

    // Fill this page and open one on the next, then turn to it.
    await openPane();
    await openPane();
    await press("2");
    expect(q(".termface__needs")).toHaveLength(1);
  });

  it("keeps the turn when the page turns, and drops it when the pane says it is over", async () => {
    await openPane();
    await openPane();
    await openPane();
    await turn(0, true);
    await press("2");
    // Turning a page takes the panes down. The turn is not taken down with them — a page turn is
    // exactly when nobody is looking at that pane.
    expect(q(".termface__needs")).toHaveLength(1);

    await press("1");
    await turn(0, false);
    await press("2");
    expect(q(".termface__needs")).toHaveLength(0);
    expect(told).toEqual([true, false]);
  });

  it("knocks the shell once however many panes are standing", async () => {
    await openPane();
    await openPane();
    await turn(0, true);
    await turn(1, true);
    // Two panes, one knock.
    expect(told).toEqual([true]);
    // And only says it is over when the last of them is.
    await turn(0, false);
    expect(told).toEqual([true]);
    await turn(1, false);
    expect(told).toEqual([true, false]);
  });
});

describe("going to the pane that needs you", () => {
  it("goes to the page it is on and does not type into it", async () => {
    // Fill this page, then make a pane on the next one and let its turn come.
    await openPane();
    await openPane();
    await openPane();
    await turn(2, true);

    await press("1");
    expect(q(".termface__page--on")[0]!.textContent).toContain("1");

    await press("j");
    expect(q(".termface__page--on")[0]!.textContent).toContain("2");
    // ⌘J moves. A terminal is somebody's, and typing into it on their behalf is not being told.
    expect(hoisted.typed).toEqual([]);
  });

  it("says so, in the ledger's own terms, when there is nowhere to go", async () => {
    await openPane();
    await press("j");
    expect(container.textContent).toContain(t("face.nothingNeedsYou"));
  });
});
