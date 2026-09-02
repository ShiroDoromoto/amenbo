// @vitest-environment jsdom
// The classification selects in the detail pane move before the write answers, and one of these writes
// can be refused: a required axis will not be emptied (`AMB-D-734`). What that made possible was a pane
// showing a value the store never took — the toast said no, the select stayed where the reader had put
// it, and the two disagreed until the task was opened again.
//
// What these guard: a refused assignment **puts the select back**, and one that lands **leaves it**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const AXIS = { id: 900, name: "プロダクト", notes: "", role: "none", cardinality: "single", ordered: false, showOnCard: false, required: true,
  appliesTo: "both", values: [{ id: 901, name: "Amenbo本体" }, { id: 902, name: "Viewer" }] };
/** The same axis, admitting several of its values at once (`AMB-D-826`). */
const MULTI = { ...AXIS, cardinality: "multi" };

const hoisted = vi.hoisted(() => ({
  /** Set it to make the assignment write refuse, the way core refuses to empty a required axis. */
  setFails: false,
  /** Every assignment the pane asked for, as `taskId:valueId`. */
  asked: [] as string[],
  /** The axes hung on the task's project. A test narrows one to see the pane stop offering it. */
  axes: [] as Array<Record<string, unknown>>,
}));

// Only the two writes the selects drive are stood in for; everything else — the snapshot, the store, the
// pane's own state — runs for real, which is the point: the rollback lives in the wiring between them.
vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    setTaskDimensionValue: async (taskId: number, valueId: number) => {
      hoisted.asked.push(`${taskId}:${valueId}`);
      if (hoisted.setFails) throw { code: "invalid_dimension_required_unset", message_en: "refused" };
    },
    unsetTaskDimensionValue: async (taskId: number, valueId: number) => {
      hoisted.asked.push(`unset:${taskId}:${valueId}`);
    },
  };
});

// The mock store has no axes, so one is hung on the task's project — the pane reads them off the snapshot.
vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return {
    ...orig,
    getSnapshot: () => {
      const snap = orig.getSnapshot();
      return { ...snap, projects: snap.projects.map((p) => (p.id === 1 ? { ...p, dimensions: hoisted.axes } : p)) };
    },
  };
});

import { TaskDetailPane } from "./TaskDetailPane";
import { StoreProvider } from "../store/store";
import { loadSnapshot } from "../core/snapshot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const axisSelect = () => container.querySelector<HTMLSelectElement>("select.inlineselect")!;

/** The values a multi-select axis draws as chips, in the order the row reads. */
const chips = () =>
  Array.from(container.querySelectorAll(".chip--dim")).map((c) => c.textContent?.replace("×", "").trim());
/** The cross on the chip standing for `name`. */
const cross = (name: string) =>
  Array.from(container.querySelectorAll<HTMLElement>(".chip--dim"))
    .find((c) => c.textContent?.includes(name))!
    .querySelector<HTMLButtonElement>(".chip__x")!;

/** Pick a value in the axis select, the way a reader does. */
async function pick(value: string) {
  const select = axisSelect();
  await act(async () => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await settle();
}

/** Wait for useTask (useQuery) and for the write's answer to come back. */
async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.setFails = false;
  hoisted.asked = [];
  hoisted.axes = [AXIS];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("TaskDetailPane classification selects", () => {
  async function open() {
    act(() => root.render(createElement(StoreProvider, null, createElement(TaskDetailPane, { taskId: 1 }))));
    await settle();
  }

  it("puts the select back when the write is refused", async () => {
    hoisted.setFails = true;
    await open();
    expect(axisSelect().value).toBe(""); // The task carries no value on this axis yet

    await pick("901");

    expect(hoisted.asked).toEqual(["1:901"]); // It did try
    expect(axisSelect().value).toBe(""); // …and the refusal took the select back
  });

  it("leaves the select where the reader put it when the write lands", async () => {
    await open();

    await pick("901");

    expect(hoisted.asked).toEqual(["1:901"]);
    expect(axisSelect().value).toBe("901");
  });

  // A multi-select axis keeps what it had (`AMB-D-826`), so the pane draws a value put on beside the
  // ones already there rather than over them — and the cross is the way off, on this side too.
  it("gains a value on a multi-select axis and keeps the one it had", async () => {
    hoisted.axes = [MULTI];
    await open();

    await pick("901");
    await pick("902");

    expect(hoisted.asked).toEqual(["1:901", "1:902"]);
    expect(chips()).toEqual(["Amenbo本体", "Viewer"]);
  });

  it("takes one value off a multi-select axis through the cross", async () => {
    hoisted.axes = [MULTI];
    await open();
    await pick("901");
    await pick("902");
    hoisted.asked = [];

    await act(async () => { cross("Amenbo本体").click(); });
    await settle();

    expect(hoisted.asked).toEqual(["unset:1:901"]);
    expect(chips()).toEqual(["Viewer"]);
  });

  // The mirror of the leak `AMB-D-789` was written about: an axis narrowed to the decision side runs on
  // no task, so the pane neither offers it nor — this axis being required — holds anything back over it.
  // Both read the same narrowed list, so the select being gone is the whole of it.
  it("draws nothing for an axis narrowed to the decision side", async () => {
    hoisted.axes = [{ ...AXIS, appliesTo: "decision" }];
    await open();
    expect(container.querySelector('option[value="901"]')).toBeNull();
    expect(container.textContent).not.toContain(AXIS.name);
  });
});
