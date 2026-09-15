// @vitest-environment jsdom
// The row that says which pane a task or a decision was made in, and what pressing it asks for
// (`AMB-D-897`).
//
// What this guards: the two states a press cannot be told apart by looks. A pane that is still on
// the screen is gone to and nothing is written down for it — putting a way back on a frame that is
// already running would leave it there for whatever opens in that frame next. A pane that is gone is
// opened again, and the way back goes down first, because the opening is what reads it.
//
// The name is the other half: what a pane is called now outranks what it was called then, and a pane
// nobody ever named still has a row, because the press is what the row is for.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** The frames this run has names for, as `frame_names` answers. */
  names: [] as Array<{ frame: string; name: string; by: "session" | "person" }>,
  /** The panes the arrangement holds, as `talk_layout` answers. */
  frames: [] as Array<{ id: string }>,
}));

vi.mock("../core/ipc", () => ({
  invoke: (cmd: string) => {
    if (cmd === "frame_names") return Promise.resolve(hoisted.names);
    if (cmd === "talk_layout") return Promise.resolve({ count: 1, frames: hoisted.frames });
    return Promise.resolve(null);
  },
}));
// Only the one answer is replaced: the dictionary reads the snapshot for the reader's language, and a
// module swapped whole would take that with it.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));

import { MadeIn } from "./MadeIn";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.names = [];
  hoisted.frames = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Draw the row for one recorded pane, and answer with what each half of the press was told. */
async function drawn(made: { pane: string; paneName: string | null }) {
  const asked: { openAgain: number; wentTo: Array<[number, string]> } = { openAgain: 0, wentTo: [] };
  await act(async () => {
    root.render(createElement(MadeIn, {
      made,
      project: 7,
      openAgain: () => { asked.openAgain += 1; return Promise.resolve(); },
      onGoToPane: (project: number, pane: string) => { asked.wentTo.push([project, pane]); },
    }));
  });
  return asked;
}

describe("the row that says which pane made this", () => {
  it("draws the name the pane has now, over the one the record kept", async () => {
    hoisted.names = [{ frame: "pane-1", name: "reviewing the migration", by: "person" }];
    await drawn({ pane: "pane-1", paneName: "what it was called then" });

    expect(container.querySelector("button")?.textContent).toContain("reviewing the migration");
  });

  it("draws the name the record kept where the pane is gone", async () => {
    await drawn({ pane: "pane-1", paneName: "what it was called then" });

    expect(container.querySelector("button")?.textContent).toContain("what it was called then");
  });

  it("still draws a row for a pane nobody named", async () => {
    await drawn({ pane: "pane-1", paneName: null });

    // The words are the dictionary's; what is guarded is that there is a button to press at all.
    expect(container.querySelector("button")).not.toBeNull();
  });

  it("goes straight to a pane that is still open, writing nothing down for it", async () => {
    hoisted.frames = [{ id: "pane-1" }];
    const asked = await drawn({ pane: "pane-1", paneName: null });

    await act(async () => { container.querySelector("button")?.click(); });
    expect(asked.openAgain).toBe(0);
    expect(asked.wentTo).toEqual([[7, "pane-1"]]);
  });

  it("puts the way back down before it asks for a pane that is gone", async () => {
    hoisted.frames = [{ id: "some-other-pane" }];
    const asked = await drawn({ pane: "pane-1", paneName: null });

    await act(async () => { container.querySelector("button")?.click(); });
    expect(asked.openAgain).toBe(1);
    expect(asked.wentTo).toEqual([[7, "pane-1"]]);
  });
});
