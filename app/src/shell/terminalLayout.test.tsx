// @vitest-environment jsdom
// What the arrangement has to do to the panes, which the layout's own tests cannot see: they know
// where a pane is, not whether turning a page killed the terminal in one.
//
// Three things are pinned here, each invisible in code that looks right either way. A pane is made by
// opening one — a face with nothing open draws one way in and no boxes, and a question walked away
// from leaves nothing behind. A pane opens in a folder of the project it belongs to, and in nothing
// else, which is what keeps one screen to one project (`../talk/layout`). And turning a page takes
// panes down and *picks the terminals up* when it comes back rather than starting them again — a
// second shell where the reader left one is a lost session with nothing to say it happened.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";

type Mounted = { start: PaneStart; said: (statement: unknown) => void; session: string };

const hoisted = vi.hoisted(() => ({
  mounts: [] as unknown[],
  detached: 0,
  /** The folders the project being shown is bound to. */
  folders: [{ path: "/repo", exists: true }] as { path: string; exists: boolean }[],
}));

// The frame a slot puts up, stood in for. It answers the way a real one does — a session id comes
// back, and what the agent in it says arrives through the same callback — so the face has something
// to arrange. What agent runs in it is the frame's own question and not this one's (`../talk/agent`).
vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string) => void; said: (statement: unknown) => void },
    _paneClass: string,
    start: PaneStart = {},
  ) => {
    const session = start.session ?? `s${hoisted.mounts.length + 1}`;
    hoisted.mounts.push({ start, said: on.said, session });
    on.opened(session, "2026-08-24T00:00:00Z");
    return Promise.resolve(() => { hoisted.detached++; });
  },
}));

// The ledger's projects and the folders each is bound to. Both are reads the face makes of the store,
// and what is under test is what it does with the answers.
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: hoisted.folders, live: hoisted.folders, answered: true }),
}));

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const mounts = () => hoisted.mounts as Mounted[];
const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];
const click = async (el: HTMLElement) => {
  await act(async () => { el.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
};
const press = async (key: string) => {
  await act(async () => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key, metaKey: true, bubbles: true }));
  });
};
/** Ask for another pane in the project being shown — the way in beside its name on the rail. */
const openPane = async () => click(q(".rail__open")[0] ?? q(".slot--empty")[0]!);
/** Put the face up. It is not in `beforeEach` because what the project is bound to is set per test,
 *  and the face reads it as it comes up. */
const mount = async () => {
  await act(async () => {
    root.render(createElement(TerminalFace, { onSplitOut: () => {}, note: null, onWaiting: () => {} }));
  });
};

beforeEach(() => {
  hoisted.mounts = [];
  hoisted.detached = 0;
  hoisted.folders = [{ path: "/repo", exists: true }];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the face comes up with nothing open", () => {
  it("draws one way in and no boxes beside it", async () => {
    await mount();
    expect(mounts()).toHaveLength(0);
    expect(q(".slot")).toHaveLength(1);
    expect(q(".slot--empty")).toHaveLength(1);
  });

  it("counts one page, because that is where the way in is", async () => {
    await mount();
    expect(q(".termface__page")).toHaveLength(1);
  });
});

describe("a pane works in a folder of its project", () => {
  it("asks nothing where the project is bound to one folder", async () => {
    await mount();
    await openPane();
    expect(mounts()).toHaveLength(1);
    expect(mounts()[0]!.start.cwd).toBe("/repo");
  });

  it("opens the next pane there too, and does not ask again", async () => {
    await mount();
    await openPane();
    await openPane();
    expect(mounts().map((one) => one.start.cwd)).toEqual(["/repo", "/repo"]);
  });

  it("asks which folder where the project is bound to several, and makes no pane until it is answered", async () => {
    hoisted.folders = [{ path: "/repo", exists: true }, { path: "/site", exists: true }];
    await mount();
    await openPane();
    expect(mounts(), "a pane was opened before the question was answered").toHaveLength(0);
    const choices = q(".slot--asking .agent__choice");
    expect(choices.map((one) => one.textContent)).toEqual(["/repo", "/site"]);

    await click(choices[1]!);
    expect(mounts()).toHaveLength(1);
    expect(mounts()[0]!.start.cwd).toBe("/site");
    expect(q(".slot--asking"), "the question stayed up behind the pane").toHaveLength(0);
  });

  it("leaves nothing behind when the question is walked away from", async () => {
    hoisted.folders = [{ path: "/repo", exists: true }, { path: "/site", exists: true }];
    await mount();
    await openPane();
    // Going to a pane, or to a project, is a person doing something else: the question goes with it.
    await click(q(".termface__page")[0]!);
    expect(q(".slot--asking")).toHaveLength(0);
    expect(q(".slot--empty"), "a place was left where nothing was opened").toHaveLength(1);
  });
});

describe("turning a page", () => {
  it("takes the panes down and picks the same terminals up again — never starts a second", async () => {
    await mount();
    await openPane();
    await openPane();
    await openPane();               // a third pane, which is page 2 at two a page
    const started = mounts().slice(0, 2).map((one) => one.session);
    expect(q(".termface__page")).toHaveLength(2);

    await press("2");
    expect(hoisted.detached, "the panes were left drawn on a page nobody is on").toBe(2);
    expect(mounts(), "turning a page started a terminal").toHaveLength(3);

    await press("1");
    expect(mounts()).toHaveLength(5);
    expect(mounts().slice(3).map((one) => one.start.session), "the panes were given different terminals")
      .toEqual(started);
  });

  it("does not answer a digit while the other face is the one showing", async () => {
    await mount();
    // The face is kept mounted behind `hidden` so the emulator survives the switch — which means a
    // page could be turned under a reader who is looking at the ledger.
    const hidden = document.createElement("div");
    hidden.hidden = true;
    document.body.appendChild(hidden);
    const other = createRoot(hidden);
    await act(async () => {
      other.render(createElement(TerminalFace, { onSplitOut: () => {}, note: null, onWaiting: () => {} }));
    });
    const before = hidden.querySelector(".termface__page--on")!.textContent;
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "2", metaKey: true, bubbles: true }));
    });
    expect(hidden.querySelector(".termface__page--on")!.textContent).toBe(before);
    act(() => other.unmount());
    hidden.remove();
  });
});

describe("how many panes", () => {
  it("carries the pane being worked in over to the page it is on at the new count", async () => {
    await mount();
    await openPane();
    await openPane();                              // two panes, both on page 1 at two a page
    await click(q(".termface__count")[0]!);        // one pane a page

    // At one a page the second pane is page 2, and that is where the screen is: a person who asks for
    // one pane means the one they were looking at.
    expect(q(".termface__page--on")[0]!.textContent).toBe("2");
    expect(q(".slot")).toHaveLength(1);
    expect(mounts(), "the pane carried across was restarted rather than kept").toHaveLength(2);
  });
});
