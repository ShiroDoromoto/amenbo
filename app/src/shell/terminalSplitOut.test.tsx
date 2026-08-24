// @vitest-environment jsdom
// What the press that takes the terminal into a window of its own hands over.
//
// **One pane goes, and the window has to be told which.** The window it opens is one pane with no
// rail (`AMB-D-753`), so it cannot work out for itself which of the board's places it is drawing —
// and it has to know two things about it. The frame, because the name of a pane belongs to the place
// and not to the process in it (`../talk/frames`): a window naming some other frame would rename a
// pane still sitting on the board. The session, because a window that adopted "the one terminal that
// is open" would adopt none of several — the state a board with more than one pane running is in
// every day.
//
// Neither is visible in code that looks right either way: a window handed the wrong frame draws a
// terminal and looks like a window that was handed the right one.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  mounts: 0,
  folders: [{ path: "/repo", exists: true }] as { path: string; exists: boolean }[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string) => void },
    _paneClass: string,
    start: PaneStart = {},
  ) => {
    hoisted.mounts += 1;
    on.opened(start.session ?? `s${hoisted.mounts}`, "2026-08-24T00:00:00Z");
    return Promise.resolve(() => {});
  },
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }] },
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: hoisted.folders, live: hoisted.folders, answered: true }),
}));
vi.mock("../files/FilesPanel", () => ({ FilesPanel: () => null }));

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const split = vi.fn();

const q = (sel: string) => [...container.querySelectorAll<HTMLElement>(sel)];
const click = async (el: HTMLElement) => {
  await act(async () => { el.dispatchEvent(new MouseEvent("click", { bubbles: true })); });
};
/** Open another pane in the project being shown: the way in beside its name on the rail goes to a
 *  page with room, and the empty frame waiting there is what opens the pane. */
const openPane = async () => {
  const room = q(".rail__open")[0];
  if (room) await click(room);
  await click(q(".slot--empty .slot__open")[0]!);
};
/** Work in a pane, the way a person does: a press anywhere on it. */
const workIn = async (nth: number) => {
  await act(async () => {
    q(".slot")[nth]!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  });
};
const splitOut = () => click(q(".termface__action")[0]!);

const mount = async () => {
  await act(async () => {
    root.render(createElement(TerminalFace, { onSplitOut: split, note: null, onWaiting: () => {} }));
  });
};

beforeEach(() => {
  hoisted.mounts = 0;
  split.mockReset();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("taking the terminal into a window of its own", () => {
  it("hands over the pane being worked in, and the terminal running in it", async () => {
    await mount();
    await openPane();
    await openPane();
    // The pane just opened is the one being worked in, which is what a person means by "this one".
    await splitOut();
    expect(split).toHaveBeenCalledWith({ frame: "2", session: "s2" });
  });

  it("follows the pane the person moved to, not the one opened last", async () => {
    await mount();
    await openPane();
    await openPane();
    await workIn(0);
    await splitOut();
    expect(split).toHaveBeenCalledWith({ frame: "1", session: "s1" });
  });

  it("hands over nothing where there is no pane to be working in", async () => {
    // A face with nothing open has one way in and no place to split out — the window that opens has
    // to start a terminal for itself, and there is no frame of the board's for it to draw.
    await mount();
    await splitOut();
    expect(split).toHaveBeenCalledWith(null);
  });
});
