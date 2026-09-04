// @vitest-environment jsdom
// The terminal face on a machine that has no project at all (`AMB-T-4358`).
//
// Every road that starts in a pane starts here: the world is empty, and choosing a folder is what
// raises the project the pane belongs to. What is pinned is that the way in is drawn at all — the
// face used to hold the page back until it had been told which project it was on, which on a machine
// with none is a wait for the thing the press was about to make — and that the press carries through
// to a pane opened in the folder chosen, under the project that folder raised.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../mock/types";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  /** Where a pane was opened, in the order the panes went up. */
  mounts: [] as (string | undefined)[],
  /** The projects the ledger holds. Empty to begin with — that is the whole state under test — and
   *  the folder picker is what puts one in it. */
  projects: [] as { id: number; name: string }[],
  /** How many times the way in reached the picker, and what it answered with. */
  presses: 0,
  answer: null as string | null,
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string, where?: string) => void },
    start: PaneStart = {},
  ) => {
    hoisted.mounts.push(start.cwd ?? undefined);
    on.opened(`s${hoisted.mounts.length}`, "2026-09-05T00:00:00Z", start.cwd ?? undefined);
    return Promise.resolve(() => {});
  },
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => hoisted.projects as Project[] },
}));

vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));

// The picker, and the write behind it: what a folder chosen on a machine with no project does is
// raise one, which is why the ledger is moved here rather than in the test body (`../core/mutations`).
vi.mock("../core/mutations", async (original) => ({
  ...(await original<Record<string, unknown>>()),
  chooseWorkFolder: () => {
    hoisted.presses++;
    if (hoisted.answer !== null) hoisted.projects = [{ id: 7, name: "workshop" }];
    return Promise.resolve(hoisted.answer);
  },
}));

import { TerminalFace } from "./TerminalFace";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const draw = () =>
  act(async () => {
    root.render(createElement(TerminalFace, { onWindow: () => {}, note: null, onWaiting: () => {} }));
  });

const pressOpen = () =>
  act(async () => {
    container.querySelector<HTMLButtonElement>(".slot__open")!.click();
  });

beforeEach(() => {
  window.innerWidth = 1600;
  hoisted.mounts = [];
  hoisted.projects = [];
  hoisted.presses = 0;
  hoisted.answer = "/work/workshop";
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the terminal face with no project on the machine", () => {
  it("draws the way in rather than an empty page", async () => {
    await draw();

    expect(container.querySelector(".slot--empty"), "the empty frame is on the page").not.toBe(null);
    expect(container.querySelector(".slot__open"), "and it carries the press that opens").not.toBe(null);
  });

  // The column beside it says nothing about a project, because there is no project for it to be
  // about. The line it draws for a project bound to no folder belongs to a project that exists.
  it("says nothing about a project's folders", async () => {
    await draw();

    // Scoped to the column: the reading column beside the panes has a line of its own in the same
    // place, and it is about the files nobody has opened rather than about a project.
    expect(container.querySelector(".rail .files__none")).toBe(null);
  });

  it("raises a project from the folder chosen, and opens the pane in it", async () => {
    await draw();
    await pressOpen();

    expect(hoisted.presses, "the press goes straight to the picker").toBe(1);
    expect(hoisted.mounts, "and the pane opens in what it chose").toEqual(["/work/workshop"]);
    expect(
      container.querySelector(".ptabs__tab")?.getAttribute("aria-label"),
      "the project the folder raised is on the tabs",
    ).toBe("workshop");
  });

  // Cancelling raises nothing, and leaves the way in where it was: a reader who changed their mind is
  // still on a face they can start from.
  it("leaves the way in standing when the picker is cancelled", async () => {
    hoisted.answer = null;
    await draw();
    await pressOpen();

    expect(hoisted.mounts).toEqual([]);
    expect(container.querySelector(".slot__open")).not.toBe(null);
  });
});
