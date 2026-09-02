// @vitest-environment jsdom
// The classification selects on the decision pane (`AMB-D-781`) move before the write answers, the way
// the task pane's do. Nothing here is required — a decision has no creation to finish — so a refusal is
// the store saying no for its own reasons, and what it must not leave behind is a select showing a value
// the store never took.
//
// What these guard: an axis the project does not carry **draws nothing**, an assignment that lands
// **leaves the select**, and one that is refused **puts it back and says why**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Decision } from "../core/snapshot";

const AXIS = { id: 900, name: "テーマ", notes: "", role: "none", cardinality: "single", ordered: false, showOnCard: false, required: false,
  appliesTo: "both",
  values: [{ id: 901, name: "メイン" }, { id: 902, name: "talk-window" }] };
/** The same axis, admitting several of its values at once (`AMB-D-826`). */
const MULTI = { ...AXIS, cardinality: "multi" };

const hoisted = vi.hoisted(() => ({
  /** The axes the decision's project carries — emptied to check the project with none. */
  axes: [] as unknown[],
  /** Set it to make the assignment write refuse. */
  setFails: false,
  /** Every assignment the pane asked for, as `set:<decisionId>:<valueId>` / `unset:…`. */
  asked: [] as string[],
  /** What the decision already carries, as the read answers it. */
  assigned: [] as Array<{ dimensionId: number; valueId: number }>,
}));

vi.mock("../core/reads", () => ({
  useDecision: () => DECISION,
  useDecisionComments: () => [],
  useDecisionPage: () => [],
}));
vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return {
    ...orig,
    inTauri: () => true,
    getSnapshot: () => ({ ...orig.getSnapshot(), projects: [{ id: 1, name: "検証PJ", dimensions: hoisted.axes }] }),
  };
});
vi.mock("../core/mutations", () => ({
  fetchDecisionDimensions: () => Promise.resolve(hoisted.assigned),
  setDecisionDimensionValue: (decisionId: number, valueId: number) => {
    hoisted.asked.push(`set:${decisionId}:${valueId}`);
    return hoisted.setFails
      ? Promise.reject({ code: "out_of_reach", message_en: "refused" })
      : Promise.resolve();
  },
  unsetDecisionDimensionValue: (decisionId: number, valueId: number) => {
    hoisted.asked.push(`unset:${decisionId}:${valueId}`);
    return Promise.resolve();
  },
}));
// The attachment list invokes the store, which is none of this pane's business.
vi.mock("../components/Attachments", () => ({ Attachments: () => null }));

import { DecisionDetailPane } from "./DecisionDetailPane";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const DECISION = {
  id: 781, ref: "D-781", title: "決定781", body: "", status: "accepted",
  project: { id: 1, name: "検証PJ" },
  supersedes: [], supersededBy: [], amends: [], amendedBy: [], buildsOn: [], builtOnBy: [],
  decidedAt: null, decidedBy: null, linkedTasks: [],
  createdAt: "2026-08-27T00:00:00Z", updatedAt: "2026-08-27T00:00:00Z",
} as unknown as Decision;

let container: HTMLDivElement;
let root: Root;

const axisSelect = () => container.querySelector<HTMLSelectElement>("select.inlineselect");

/** The values drawn as chips, in the order the row reads. */
const chips = () =>
  Array.from(container.querySelectorAll(".chip--dim")).map((c) => c.textContent?.replace("×", "").trim());
/** The cross on the chip standing for `name`. */
const cross = (name: string) =>
  Array.from(container.querySelectorAll<HTMLElement>(".chip--dim"))
    .find((c) => c.textContent?.includes(name))!
    .querySelector<HTMLButtonElement>(".chip__x")!;

/** Pick a value in the axis select, the way a reader does. */
async function pick(value: string) {
  const select = axisSelect()!;
  await act(async () => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await settle();
}

/** Wait for the read to land and for the write's answer to come back. */
async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

async function open() {
  act(() => root.render(createElement(DecisionDetailPane, { decisionId: DECISION.id })));
  await settle();
}

beforeEach(() => {
  hoisted.axes = [AXIS];
  hoisted.setFails = false;
  hoisted.asked = [];
  hoisted.assigned = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("DecisionDetailPane classification selects", () => {
  it("draws nothing where the project carries no axes", async () => {
    hoisted.axes = [];
    await open();
    expect(axisSelect()).toBeNull();
  });

  // The leak `AMB-D-789` was written about: an axis narrowed to the work side — an exclusive lane on
  // a real device — had a select on every decision, and a decision occupies no device.
  it("draws nothing for an axis narrowed to the task side", async () => {
    hoisted.axes = [{ ...AXIS, appliesTo: "task" }];
    await open();
    expect(axisSelect()).toBeNull();
  });

  it("shows the value the decision already carries", async () => {
    hoisted.assigned = [{ dimensionId: AXIS.id, valueId: 902 }];
    await open();
    expect(axisSelect()!.value).toBe("902");
  });

  it("leaves the select where the reader put it when the write lands", async () => {
    await open();
    expect(axisSelect()!.value).toBe("");

    await pick("901");

    expect(hoisted.asked).toEqual(["set:781:901"]);
    expect(axisSelect()!.value).toBe("901");
  });

  it("clears the axis through the unset write", async () => {
    hoisted.assigned = [{ dimensionId: AXIS.id, valueId: 901 }];
    await open();

    await pick("");

    expect(hoisted.asked).toEqual(["unset:781:901"]);
    expect(axisSelect()!.value).toBe("");
  });

  // A multi-select axis is the one that keeps what it had (`AMB-D-826`): the chips are what it carries,
  // the select offers only what it does not, and the cross is the way off — the same `unset` write the
  // single-select axis clears itself through.
  it("draws every value a multi-select axis carries, and offers only the rest", async () => {
    hoisted.axes = [MULTI];
    hoisted.assigned = [{ dimensionId: MULTI.id, valueId: 901 }, { dimensionId: MULTI.id, valueId: 902 }];
    await open();

    expect(chips()).toEqual(["メイン", "talk-window"]);
    expect(axisSelect()).toBeNull(); // Nothing left to offer
  });

  it("gains a value on a multi-select axis and keeps the one it had", async () => {
    hoisted.axes = [MULTI];
    hoisted.assigned = [{ dimensionId: MULTI.id, valueId: 901 }];
    await open();
    expect(chips()).toEqual(["メイン"]);

    await pick("902");

    expect(hoisted.asked).toEqual(["set:781:902"]);
    expect(chips()).toEqual(["メイン", "talk-window"]);
  });

  it("takes one value off a multi-select axis through the cross", async () => {
    hoisted.axes = [MULTI];
    hoisted.assigned = [{ dimensionId: MULTI.id, valueId: 901 }, { dimensionId: MULTI.id, valueId: 902 }];
    await open();

    await act(async () => { cross("メイン").click(); });
    await settle();

    expect(hoisted.asked).toEqual(["unset:781:901"]);
    expect(chips()).toEqual(["talk-window"]);
  });

  it("puts a refused value back on a multi-select axis without dropping the others", async () => {
    hoisted.axes = [MULTI];
    hoisted.assigned = [{ dimensionId: MULTI.id, valueId: 901 }];
    hoisted.setFails = true;
    await open();

    await pick("902");

    expect(hoisted.asked).toEqual(["set:781:902"]);
    expect(chips()).toEqual(["メイン"]);
    expect(container.textContent).toContain("refused");
  });

  it("puts the select back and says why when the write is refused", async () => {
    hoisted.setFails = true;
    await open();

    await pick("901");

    expect(hoisted.asked).toEqual(["set:781:901"]); // It did try
    expect(axisSelect()!.value).toBe(""); // …and the refusal took the select back
    expect(container.textContent).toContain("refused");
  });
});
