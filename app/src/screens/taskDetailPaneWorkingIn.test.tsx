// @vitest-environment jsdom
// The row on a task that names the pane it is being worked in — the way back from the ledger to the
// terminal (`AMB-D-758`, `AMB-T-3717`).
//
// **What it must never do is guess.** A status move made outside a pane — somebody's own terminal, an
// editor, another machine — goes through the same path and leaves no row in the volatile area, and a
// pane that has closed leaves nothing to send a reader to. Both come back as no row at all, which
// says "no pane here is working on this" and never "nobody is working on this".
//
// The failure this guards is silent in both directions: a row that never appears looks like a task
// nobody is on, and a row that appears for a session that has gone looks like a pane to press.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskPaneDto } from "../bindings/bindings";
import { t } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** What the host answers `task_pane` with — the join of the volatile area and the face's panes. */
  pane: null as TaskPaneDto | null,
  /** Every pane the ledger asked to be taken to. */
  went: [] as string[],
}));

// Inside Tauri, because the volatile area and the panes drawing it only exist there.
vi.mock("../core/snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/snapshot")>()),
  inTauri: () => true,
}));

vi.mock("../core/activity", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/activity")>()),
  loadTaskActivity: async () => [],
}));
vi.mock("../core/mutations", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/mutations")>()),
  fetchTaskDimensions: async () => [],
}));

vi.mock("../core/ipc", async (importOriginal) => {
  const real = await importOriginal<typeof import("../core/ipc")>();
  return {
    ...real,
    invoke: async (cmd: string, args?: Record<string, unknown>) => {
      // The task itself comes from the mock fixtures: what is under test is the row beside it, not
      // how a task is read.
      if (cmd === "tasks_by_ids") {
        const ids = (args as { ids: number[] }).ids;
        return getSnapshot().tasks.filter((one) => ids.includes(one.id));
      }
      if (cmd === "task_pane") return hoisted.pane;
      if (cmd === "show_pane") {
        hoisted.went.push((args as { session: string }).session);
        return undefined;
      }
      return undefined;
    },
  };
});

import { TaskDetailPane } from "./TaskDetailPane";
import { StoreProvider } from "../store/store";
import { getSnapshot, loadSnapshot } from "../core/snapshot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const row = () => container.querySelector<HTMLButtonElement>(".detail__pane");

async function open() {
  act(() => root.render(createElement(StoreProvider, null, createElement(TaskDetailPane, { taskId: 1 }))));
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.pane = null;
  hoisted.went = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the pane a task is being worked in", () => {
  it("is named on the task, under the label the pane carries everywhere else", async () => {
    hoisted.pane = { session: "s-c", label: "移行の調査" };
    await open();
    expect(row()?.textContent).toContain("移行の調査");
    expect(container.textContent).toContain(t("detail.workingIn"));
  });

  it("is not there at all when no pane here is holding the task", async () => {
    // Reserved from somebody's own terminal, or in a pane that has since closed. There is nowhere to
    // send the reader, and a row that led nowhere would be worse than no row.
    await open();
    expect(row()).toBeNull();
    expect(container.textContent).not.toContain(t("detail.workingIn"));
  });

  it("sends the session and nothing else when it is pressed", async () => {
    // Which window holds the face, and where in it that session is drawn, are both answered past this
    // press (`crate::windows::show_pane`) — a place named here would be a second answer free to go
    // stale against the face's own.
    hoisted.pane = { session: "s-c", label: "移行の調査" };
    await open();
    await act(async () => {
      row()!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(hoisted.went).toEqual(["s-c"]);
  });
});
