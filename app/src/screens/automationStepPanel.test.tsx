// @vitest-environment jsdom
// What the panel draws for the pressed spot and what each control writes (`AMB-T-5256`,
// `AMB-T-5282`). The reads and the write doors are stubbed; every field, its control and what it
// sends run for real.
//
// What these guard: **nothing pressed says so** rather than drawing an empty form; **each field
// writes on the layer it belongs to** (`AMB-D-949`) — the name and the declarations on the library
// action, the answer on this placement, the prompt and the flags on the action's step; **a setting is
// answered by the control its kind takes**, a task filter on rows rather than in a filter expression;
// **an input is filled from a list of what fits**; **the error way out is always the last line of the
// ways out**; and **a refusal lands on the panel** rather than in the console.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationPlacementDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  editAction: vi.fn(),
  editStep: vi.fn(),
  answerCfg: vi.fn(),
  setWire: vi.fn(),
  clearWire: vi.fn(),
  declareExit: vi.fn(),
  renameExit: vi.fn(),
  removeExit: vi.fn(),
  declareCfg: vi.fn(),
  editCfg: vi.fn(),
  removeCfg: vi.fn(),
  declareInput: vi.fn(),
  editInput: vi.fn(),
  removeInput: vi.fn(),
  setEntry: vi.fn(),
  addEdge: vi.fn(),
  editEdge: vi.fn(),
  removeEdge: vi.fn(),
  removePlacement: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  editAutomationAction: hoisted.editAction,
  editAutomationStep: hoisted.editStep,
  answerAutomationCfg: hoisted.answerCfg,
  setAutomationWire: hoisted.setWire,
  clearAutomationWire: hoisted.clearWire,
  declareAutomationExit: hoisted.declareExit,
  renameAutomationExit: hoisted.renameExit,
  removeAutomationExit: hoisted.removeExit,
  declareAutomationCfg: hoisted.declareCfg,
  editAutomationCfg: hoisted.editCfg,
  removeAutomationCfg: hoisted.removeCfg,
  declareAutomationInput: hoisted.declareInput,
  editAutomationInput: hoisted.editInput,
  removeAutomationInput: hoisted.removeInput,
  setAutomationEntry: hoisted.setEntry,
  addAutomationEdge: hoisted.addEdge,
  editAutomationEdge: hoisted.editEdge,
  removeAutomationEdge: hoisted.removeEdge,
  removeAutomationPlacement: hoisted.removePlacement,
}));
// Taking a spot off asks first, and what the machine would put up is not this test's business.
vi.mock("../core/dialog", () => ({ confirmDialog: () => Promise.resolve(true) }));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));
// The machine's own answers. `inTauri` is false in this environment, so neither probe is made and
// what the panel draws is the spot's own value plus whatever these would have added.
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t } from "../core/i18n";
import { AutomationStepPanel } from "./AutomationStepPanel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** The action standing at a spot, flattened onto it the way the door hands it over. */
function spot(over: Partial<AutomationPlacementDto> = {}): AutomationPlacementDto {
  return {
    id: 1,
    name: "Take the next task",
    actionId: 4,
    stepId: 11,
    prompt: "take one",
    agent: "claude-code",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    exits: [{ id: 10, outputs: [] }, { id: 11, name: "*", outputs: [] }],
    inputs: [],
    settings: [],
    ...over,
  };
}

function detail(over: Partial<AutomationDetailDto> = {}): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    entryPlacementId: 1,
    archived: false,
    placements: [spot()],
    edges: [],
    wires: [],
    ...over,
  };
}

type Props = Parameters<typeof AutomationStepPanel>[0];

/** `onRemoved` is the screen's business, so a test that is not about it does not have to pass one. */
async function render(props: Omit<Props, "onRemoved"> & { onRemoved?: () => void }) {
  await act(async () => {
    root.render(createElement(AutomationStepPanel, { onRemoved: () => undefined, ...props }));
  });
}

const selects = () => [...container.querySelectorAll<HTMLSelectElement>("select")];

/** Type into a controlled box the way React hears it — its own value tracker has to be moved. */
async function typeInto(box: HTMLInputElement, value: string) {
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setter.call(box, value);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
const boxes = () => [...container.querySelectorAll<HTMLInputElement>("input")];

/** The tick box of the row that reads like this, found by the words beside it. */
const checkFor = (label: string) =>
  [...container.querySelectorAll<HTMLLabelElement>(".autostep__check")]
    .find((one) => one.textContent?.includes(label))!
    .querySelector<HTMLInputElement>("input")!;

/** The pulldown that says what happens after one way out, found by the way out it hangs on. */
const nextFor = (exit: string) =>
  [...container.querySelectorAll<HTMLSelectElement>("select")].find(
    (one) => one.getAttribute("aria-label") === exit,
  )!;

/** Pick a value on a pulldown the way a reader does. */
async function pick(control: HTMLSelectElement, value: string) {
  await act(async () => {
    control.value = value;
    control.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

/** The line that declares one more of a family, found by what its empty box asks for. */
const declareLine = (what: string) =>
  [...container.querySelectorAll<HTMLDivElement>(".autostep__declare")].find(
    (one) => one.querySelector("input")!.placeholder === what,
  )!;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  for (const one of Object.values(hoisted)) one.mockReset();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the panel of one spot", () => {
  it("says so while nothing is pressed", async () => {
    await render({ automation: detail(), placementId: null, projectId: 1 });
    expect(container.textContent).toContain(t("auto.step.none"));
    expect(selects()).toHaveLength(0);
  });

  /// The name is the library action's, not the spot's — one action placed twice is one name, which
  /// is what the library is for.
  it("writes the name onto the action when the caret leaves the box", async () => {
    await render({ automation: detail(), placementId: 1, projectId: 1 });
    const box = boxes()[0]!;
    await typeInto(box, "Take one");
    expect(hoisted.editAction).not.toHaveBeenCalled();
    await act(async () => {
      box.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    expect(hoisted.editAction).toHaveBeenCalledWith(4, { name: "Take one" });
  });

  /// The prompt is the action's step's: what is stood up is a terminal, and an action holds the
  /// steps (`AMB-D-950`).
  it("writes the prompt onto the step the action opens", async () => {
    await render({ automation: detail(), placementId: 1, projectId: 1 });
    const prompt = container.querySelector<HTMLTextAreaElement>("textarea")!;
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
      setter.call(prompt, "take another");
      prompt.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => {
      prompt.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    expect(hoisted.editStep).toHaveBeenCalledWith(11, { prompt: "take another" });
  });

  /// An action that holds no step has no prompt to draw and no agent to pick — the launch check is
  /// what names it, and the panel leaves those fields off rather than drawing them over nothing.
  it("leaves the step's own fields off where the action opens nothing", async () => {
    const one = detail({ placements: [spot({ stepId: undefined, prompt: "" })] });
    await render({ automation: one, placementId: 1, projectId: 1 });
    expect(container.querySelector("textarea")).toBeNull();
    expect(container.textContent).not.toContain(t("auto.step.agent"));
    // What the action declares is still there to write on.
    expect(container.textContent).toContain(t("auto.step.exits"));
  });

  it("answers a task filter on rows, and writes the object naming each part", async () => {
    const one = detail({
      placements: [spot({ settings: [{ name: "which", kind: "taskfilter", required: true }] })],
    });
    await render({ automation: one, placementId: 1, projectId: 1 });
    const chip = [...container.querySelectorAll<HTMLButtonElement>(".autostep__chip")].find(
      (b) => b.textContent === t("filter.opt.assignee.meAi"),
    )!;
    await act(async () => {
      chip.click();
    });
    // The answer is the placement's, so one action placed twice is not answered for both.
    expect(hoisted.answerCfg).toHaveBeenCalledWith(1, "which", '{"assignee":["me-ai"]}');
  });

  it("fills an input from what fits, and empties it back to nothing reaching it", async () => {
    const one = detail({
      placements: [
        spot({ id: 1, exits: [{ id: 10, outputs: [{ name: "note", kind: "value", required: true }] }] }),
        spot({
          id: 2,
          name: "work",
          actionId: 5,
          stepId: 12,
          inputs: [{ name: "note", kind: "value", required: true }],
        }),
      ],
      wires: [
        { id: 3, fromId: 1, fromPortName: "note", toId: 2, toPortName: "note" },
      ],
    });
    await render({ automation: one, placementId: 2, projectId: 1 });
    const wire = selects().find((s) => [...s.options].some((o) => o.textContent?.includes("note")))!;
    await act(async () => {
      wire.value = "";
      wire.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(hoisted.clearWire).toHaveBeenCalledWith(3);
  });

  it("draws the error way out last, and offers neither a rename nor a delete on it", async () => {
    await render({ automation: detail(), placementId: 1, projectId: 1 });
    const ways = [...container.querySelectorAll(".autostep__exits li")];
    expect(ways).toHaveLength(2);
    expect(ways[0]!.querySelector("input")!.placeholder).toBe(t("auto.step.exitUnnamed"));
    expect(ways[1]!.className).toContain("autostep__exiterr");
    expect(ways[1]!.textContent).toContain(t("auto.pic.errorExit"));
    // It takes an edge like any other way out — what it does not take is a rename or a delete.
    expect(ways[1]!.querySelectorAll("input, button")).toHaveLength(0);
    expect(nextFor(t("auto.pic.errorExit"))).toBeDefined();
  });

  it("declares a way out on the action under the name that was typed, and empties the box", async () => {
    await render({ automation: detail(), placementId: 1, projectId: 1 });
    const line = declareLine(t("auto.step.exitName"));
    const box = line.querySelector<HTMLInputElement>("input")!;
    await typeInto(box, "something to fix");
    await act(async () => {
      line.querySelector<HTMLButtonElement>("button")!.click();
    });
    expect(hoisted.declareExit).toHaveBeenCalledWith("action", 4, "something to fix");
    expect(box.value).toBe("");
  });

  it("takes a setting off choice and its list of choices in the one call", async () => {
    const one = detail({
      placements: [
        spot({ settings: [{ name: "depth", kind: "choice", required: false, options: '["quick"]' }] }),
      ],
    });
    await render({ automation: one, placementId: 1, projectId: 1 });
    const kind = selects().find((s) => s.value === "choice")!;
    await act(async () => {
      kind.value = "text";
      kind.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(hoisted.editCfg).toHaveBeenCalledWith(4, "depth", { kind: "text", options: null });
  });

  it("declares an input on the action as what it carries", async () => {
    await render({ automation: detail(), placementId: 1, projectId: 1 });
    const line = declareLine(t("auto.step.inputName"));
    await typeInto(line.querySelector<HTMLInputElement>("input")!, "report");
    await act(async () => {
      const kind = line.querySelector<HTMLSelectElement>("select")!;
      kind.value = "file";
      kind.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => {
      line.querySelector<HTMLButtonElement>("button")!.click();
    });
    expect(hoisted.declareInput).toHaveBeenCalledWith("action", 4, {
      name: "report",
      kind: "file",
    });
  });

  it("draws what core refused, and leaves the typed name where it can be fixed", async () => {
    hoisted.declareExit.mockRejectedValue("a way out called 'done' is already declared here");
    await render({ automation: detail(), placementId: 1, projectId: 1 });
    const line = declareLine(t("auto.step.exitName"));
    const box = line.querySelector<HTMLInputElement>("input")!;
    await typeInto(box, "done");
    await act(async () => {
      line.querySelector<HTMLButtonElement>("button")!.click();
    });
    expect(container.querySelector('[role="alert"]')!.textContent).toContain("already declared");
    expect(box.value).toBe("done");
  });

  it("takes a flag on the spot, onto the step it is a flag of", async () => {
    await render({ automation: detail(), placementId: 1, projectId: 1 });
    await act(async () => {
      checkFor(t("auto.step.interactive")).click();
    });
    expect(hoisted.editStep).toHaveBeenCalledWith(11, { interactive: true });
  });

  /// Where a run opens is the automation's, not the spot's — so the tick box writes on the
  /// definition, and unticking it leaves the automation with no entry at all rather than refusing.
  it("names this spot as where a run opens, and gives the entry back", async () => {
    await render({ automation: detail({ entryPlacementId: undefined }), placementId: 1, projectId: 1 });
    const entry = checkFor(t("auto.step.entry"));
    expect(entry.checked).toBe(false);
    await act(async () => entry.click());
    expect(hoisted.setEntry).toHaveBeenCalledWith(7, 1);

    await render({ automation: detail(), placementId: 1, projectId: 1 });
    await act(async () => checkFor(t("auto.step.entry")).click());
    expect(hoisted.setEntry).toHaveBeenCalledWith(7, null);
  });

  /// One way out decides one thing, so the row writes the one edge on it: adding where nothing was
  /// said, changing the one that is there, and taking it away for "nothing said yet".
  it("says what happens after a way out, changes it, and takes it back", async () => {
    await render({ automation: detail(), placementId: 1, projectId: 1 });
    await pick(nextFor(t("auto.step.exitUnnamed")), "done");
    expect(hoisted.addEdge).toHaveBeenCalledWith(
      "automation",
      { boxId: 1, exitName: undefined },
      { ends: "done" },
    );

    const said = detail({
      edges: [{ id: 8, fromId: 1, ends: "done" }],
    });
    await render({ automation: said, placementId: 1, projectId: 1 });
    await pick(nextFor(t("auto.step.exitUnnamed")), "go:1");
    expect(hoisted.editEdge).toHaveBeenCalledWith(8, { ends: "go", to: 1 });

    await render({ automation: said, placementId: 1, projectId: 1 });
    await pick(nextFor(t("auto.step.exitUnnamed")), "");
    expect(hoisted.removeEdge).toHaveBeenCalledWith(8);
  });

  /// The limit is a `go` edge's alone: an edge that closes the task or stops the run is taken once,
  /// and core refuses one there.
  it("writes the limit of a go edge, and draws none on one that ends the task", async () => {
    const looping = detail({
      edges: [{ id: 8, fromId: 1, ends: "go", toId: 1, maxTimes: 10 }],
    });
    await render({ automation: looping, placementId: 1, projectId: 1 });
    const limit = boxes().find((b) => b.type === "number")!;
    expect(limit.value).toBe("10");
    await typeInto(limit, "");
    await act(async () => limit.dispatchEvent(new FocusEvent("focusout", { bubbles: true })));
    expect(hoisted.editEdge).toHaveBeenCalledWith(8, { maxTimes: null });

    await render({
      automation: detail({ edges: [{ id: 8, fromId: 1, ends: "done" }] }),
      placementId: 1,
      projectId: 1,
    });
    expect(boxes().some((b) => b.type === "number")).toBe(false);
  });

  /// The panel is drawn from the spot, so the screen has to be told to stop showing it — and what
  /// stays behind is the library action, which outlives any one picture.
  it("takes the spot off and tells the screen there is nothing left to draw", async () => {
    const onRemoved = vi.fn();
    await render({ automation: detail(), placementId: 1, projectId: 1, onRemoved });
    const press = [...container.querySelectorAll<HTMLButtonElement>("button")].find(
      (one) => one.textContent === t("auto.step.placementRemove"),
    )!;
    await act(async () => press.click());
    expect(hoisted.removePlacement).toHaveBeenCalledWith(1);
    expect(onRemoved).toHaveBeenCalled();
  });
});
