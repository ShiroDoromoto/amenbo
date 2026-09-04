// @vitest-environment jsdom
// A press on the empty frame that arrives before the read of what the project is bound to comes back
// (`AMB-T-3700`).
//
// The read is out for a moment every time the face lands on a project, and `live` is empty for exactly
// that long. Answering a press from it says "this project has no folder" about a project that has one,
// and the folder picker comes up on a binding that was already made. What is pinned here is that the
// press waits for the answer instead of being answered from the gap — and that a project genuinely
// bound to none still reaches the picker, which is the half a fix could quietly take away.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoundFolderDto } from "../bindings/bindings";
import type { PaneStart } from "../talk/terminal";

const hoisted = vi.hoisted(() => ({
  /** Where a pane was opened, in the order the panes went up. */
  mounts: [] as (string | undefined)[],
  /** What the read of the project's folders says right now, and whether it has said it. */
  bound: { paths: [] as string[], answered: false },
  /** Which projects the folder picker was raised for — the thing that must not happen on a binding
   *  that is already there. */
  picked: [] as number[],
}));

vi.mock("../talk/agent", () => ({
  mountAgentFrame: (
    _host: HTMLElement,
    _lang: string,
    on: { opened: (s: string, at: string, where?: string) => void },
    start: PaneStart = {},
  ) => {
    hoisted.mounts.push(start.cwd ?? undefined);
    on.opened(`s${hoisted.mounts.length}`, "2026-08-24T00:00:00Z", start.cwd ?? undefined);
    return Promise.resolve(() => {});
  },
}));

vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "one" }] },
}));

const folder = (path: string): BoundFolderDto =>
  ({ path, exists: true, mismatch: null, legacy: false, pointerMissing: false, foreign: null });

// The read, held where the test can move it: what it says and whether it has said it are the two
// halves this is about (`../core/boundFolders`).
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => {
    const all = hoisted.bound.paths.map(folder);
    return { all, live: all, answered: hoisted.bound.answered };
  },
}));

vi.mock("../core/mutations", async (original) => ({
  ...(await original<Record<string, unknown>>()),
  chooseFolderFor: (projectId: number) => {
    hoisted.picked.push(projectId);
    return Promise.resolve("/work/picked");
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

/** The read comes back, with the folders named. The face is drawn again because the hook it reads
 *  through is stubbed and has nothing of its own to announce with. */
const readLands = async (...paths: string[]) => {
  hoisted.bound.paths = paths;
  hoisted.bound.answered = true;
  await draw();
};

const pressOpen = () =>
  act(async () => {
    container.querySelector<HTMLButtonElement>(".slot__open")!.click();
  });

beforeEach(() => {
  window.innerWidth = 1600;
  hoisted.mounts = [];
  hoisted.bound = { paths: [], answered: false };
  hoisted.picked = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("opening a pane while the read of the project's folders is still out", () => {
  it("waits for the answer rather than treating the gap as no folder", async () => {
    await draw();
    await pressOpen();

    expect(hoisted.picked, "nothing is asked from an unanswered read").toEqual([]);
    expect(hoisted.mounts, "and nothing is opened from it either").toEqual([]);

    await readLands("/work/one");

    expect(hoisted.picked).toEqual([]);
    expect(hoisted.mounts, "the held press opens where the project is bound").toEqual(["/work/one"]);
  });

  // The other half: the picker is the right answer for a project bound to nothing, and it must still
  // come up once the read has actually said so.
  it("reaches the picker once the read says there is no folder", async () => {
    await draw();
    await pressOpen();
    await readLands();

    expect(hoisted.picked).toEqual([1]);
  });

  // Several folders is the third answer, and it is a question rather than an opening.
  it("puts the question up once the read says there are several", async () => {
    await draw();
    await pressOpen();
    await readLands("/work/one", "/work/two");

    expect(hoisted.mounts).toEqual([]);
    expect(hoisted.picked).toEqual([]);
    expect(container.querySelector(".slot--asking"), "the folder question is on screen").not.toBe(null);
  });

  // A press that arrives after the read is answered on the spot — the held press must not put a delay
  // into the common way in.
  it("opens straight away when the read has already landed", async () => {
    hoisted.bound = { paths: ["/work/one"], answered: true };
    await draw();
    await pressOpen();

    expect(hoisted.mounts).toEqual(["/work/one"]);
  });
});
