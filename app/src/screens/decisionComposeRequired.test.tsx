// @vitest-environment jsdom
// The demand a decision meets while its writer is still in front of it. A required axis is read where a
// decision is settled (`AMB-D-790`), and that press is somebody else's — so a form that let the record
// go out blank would put the refusal in front of the wrong person, hours later.
//
// What these guard: only the **required** axes on the **decision** side draw a select, the button is
// **held** until each is answered and says which, and what is answered **rides with the create**.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t, tf } from "../core/i18n";

const REQUIRED = { id: 900, name: "テーマ", notes: "", role: "none", ordered: false, showOnCard: false,
  required: true, appliesTo: "both",
  values: [{ id: 901, name: "メイン" }, { id: 902, name: "talk-window" }] };
const OPTIONAL = { ...REQUIRED, id: 910, name: "影響半径", required: false,
  values: [{ id: 911, name: "広い" }] };
const WORK_ONLY = { ...REQUIRED, id: 920, name: "占有", appliesTo: "task",
  values: [{ id: 921, name: "iOS" }] };
const TIME_AXIS = { ...REQUIRED, id: 930, name: "フェーズ", role: "time_axis",
  values: [{ id: 931, name: "運用第2期", startOn: "2026-07-08" }] };

const hoisted = vi.hoisted(() => ({
  /** The axes the project carries. */
  axes: [] as unknown[],
  /** Every create the form asked for, as `<title>:<value ids>`. */
  asked: [] as string[],
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
  addDecision: (_p: number, title: string, _b: string, ids: number[]) => {
    hoisted.asked.push(`${title}:${ids.join(",")}`);
    return Promise.resolve();
  },
  fetchProjectDecisionDimensionAssignments: () => Promise.resolve([]),
}));

import { DecisionCompose } from "./DecisionsScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const selects = () => [...container.querySelectorAll<HTMLSelectElement>("select.inlineselect")];
const addButton = () =>
  [...container.querySelectorAll("button")].find((b) => b.textContent === t("dec.add"))!;

async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

async function type(value: string) {
  const input = container.querySelector("input")!;
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function pick(select: HTMLSelectElement, value: string) {
  await act(async () => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

async function open() {
  act(() => root.render(createElement(DecisionCompose, { projectId: 1, onDone: () => {} })));
  await settle();
}

beforeEach(() => {
  hoisted.axes = [REQUIRED];
  hoisted.asked = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("recording a decision under the axes its project demands", () => {
  // The rest of the axes are the detail pane's. Drawing every one here would turn recording a decision
  // into a form to work through, which is not what the demand is about.
  it("draws a select for the required axes alone", async () => {
    hoisted.axes = [REQUIRED, OPTIONAL];
    await open();
    expect(selects()).toHaveLength(1);
  });

  // `AMB-D-789`: an axis narrowed to the work side demands nothing of a decision, and holding the
  // button on one would ask for a value no decision can carry.
  it("ignores a required axis narrowed to the task side", async () => {
    hoisted.axes = [WORK_ONLY];
    await open();
    expect(selects()).toHaveLength(0);
  });

  // `AMB-D-147`: the era containing today goes on the decision as it is recorded, so asking for it here
  // would cost a choice every time and land on the same value. Nothing is sent for it either — the
  // create writes it, not the form.
  it("does not ask for the time axis, which the create fills", async () => {
    hoisted.axes = [TIME_AXIS];
    await open();
    await type("時代は訊かれない");
    expect(selects()).toHaveLength(0);
    expect(addButton().disabled).toBe(false);
    await act(async () => { addButton().click(); });
    await settle();
    expect(hoisted.asked).toEqual(["時代は訊かれない:"]);
  });

  it("holds the button until the axis is answered, and names it", async () => {
    await open();
    await type("窓をどう建てるか");
    expect(addButton().disabled).toBe(true);
    expect(container.textContent).toContain(tf("detail.finishCreatingBlocked", { names: "テーマ" }));

    await pick(selects()[0], "901");

    expect(addButton().disabled).toBe(false);
    expect(container.textContent).not.toContain(tf("detail.finishCreatingBlocked", { names: "テーマ" }));
  });

  it("sends what was answered along with the create", async () => {
    await open();
    await type("窓をどう建てるか");
    await pick(selects()[0], "902");
    await act(async () => { addButton().click(); });
    await settle();

    expect(hoisted.asked).toEqual(["窓をどう建てるか:902"]);
  });

  // A project demanding nothing is the common case, and it is left exactly as it was.
  it("asks for nothing where the project demands nothing", async () => {
    hoisted.axes = [OPTIONAL];
    await open();
    await type("ふつうの決定");
    expect(selects()).toHaveLength(0);
    expect(addButton().disabled).toBe(false);
    await act(async () => { addButton().click(); });
    await settle();
    expect(hoisted.asked).toEqual(["ふつうの決定:"]);
  });
});
