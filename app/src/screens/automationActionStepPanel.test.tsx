// @vitest-environment jsdom
// What the panel draws for the pressed step inside an action, and what each control writes
// (`AMB-T-5315`). The reads and the write doors are stubbed; every field, its control and what it
// sends run for real.
//
// What these guard: **nothing pressed says so** rather than drawing an empty form; **every field
// writes on the step** — the prompt and the flags are the terminal's, who carries it out is left to
// where the action is placed (`AMB-D-960`), and what it declares is the step's rather than the
// action's; **the way out says where the run goes
// next**, on the action's own picture, and choosing "nothing said" takes that line away;
// **the error way out is drawn with that pulldown and nothing else**, being carried from birth and
// neither renamed nor removed; **the entry is named from the step it names**; and **deleting asks
// first**, taking the panel's selection with it.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionDetailDto, AutomationStepDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  editStep: vi.fn(),
  setWire: vi.fn(),
  clearWire: vi.fn(),
  addEdge: vi.fn(),
  editEdge: vi.fn(),
  removeEdge: vi.fn(),
  declareExit: vi.fn(),
  renameExit: vi.fn(),
  removeExit: vi.fn(),
  declareInput: vi.fn(),
  editInput: vi.fn(),
  removeInput: vi.fn(),
  removeStep: vi.fn(),
  setEntry: vi.fn(),
  confirm: vi.fn(async () => true),
}));

vi.mock("../core/automations", () => ({
  editAutomationStep: hoisted.editStep,
  setAutomationWire: hoisted.setWire,
  clearAutomationWire: hoisted.clearWire,
  addAutomationEdge: hoisted.addEdge,
  editAutomationEdge: hoisted.editEdge,
  removeAutomationEdge: hoisted.removeEdge,
  declareAutomationExit: hoisted.declareExit,
  renameAutomationExit: hoisted.renameExit,
  removeAutomationExit: hoisted.removeExit,
  declareAutomationInput: hoisted.declareInput,
  editAutomationInput: hoisted.editInput,
  removeAutomationInput: hoisted.removeInput,
  removeAutomationStep: hoisted.removeStep,
  setAutomationActionEntry: hoisted.setEntry,
}));
vi.mock("../core/dialog", () => ({ confirmDialog: hoisted.confirm }));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));
// The machine's own answers. `inTauri` is false here, so neither probe is made and what the panel
// draws is the step's own value.
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t } from "../core/i18n";
import { AutomationActionStepPanel } from "./AutomationActionStepPanel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function step(over: Partial<AutomationStepDto> = {}): AutomationStepDto {
  return {
    id: 11,
    name: "Take the next task",
    prompt: "take one",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    // What core writes at birth: the unnamed way out, and the error one nobody can delete.
    exits: [{ id: 10, outputs: [] }, { id: 19, name: "*", outputs: [] }],
    inputs: [],
    ...over,
  };
}

function action(over: Partial<AutomationActionDetailDto> = {}): AutomationActionDetailDto {
  return {
    id: 4,
    name: "Take one",
    note: "",
    global: false,
    usedBy: 1,
    entryStepId: 11,
    steps: [step()],
    edges: [],
    wires: [],
    exits: [{ id: 20, outputs: [] }],
    inputs: [],
    settings: [],
    heldBy: [],
    ...over,
  };
}

async function render(props: Parameters<typeof AutomationActionStepPanel>[0]) {
  await act(async () => {
    root.render(createElement(AutomationActionStepPanel, props));
  });
}

const selects = () => [...container.querySelectorAll<HTMLSelectElement>("select")];
const boxes = () => [...container.querySelectorAll<HTMLInputElement>("input")];
const buttons = () => [...container.querySelectorAll<HTMLButtonElement>("button")];
const button = (label: string) => buttons().find((one) => one.textContent === label)!;

/** The pulldown that says what happens after one way out, in the order the ways out are drawn. */
const nextPicks = () =>
  selects().filter((one) => {
    const label = one.getAttribute("aria-label");
    return label === t("auto.step.exitUnnamed") || label === t("auto.pic.errorExit");
  });

/** The line that declares one more of a family, found by what its empty box asks for. */
const declareLine = (what: string) =>
  [...container.querySelectorAll<HTMLDivElement>(".autostep__declare")].find(
    (one) => one.querySelector("input")!.placeholder === what,
  )!;

async function typeInto(box: HTMLInputElement | HTMLTextAreaElement, value: string) {
  const proto = box instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(proto.prototype, "value")!.set!.call(box, value);
    box.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function pick(select: HTMLSelectElement, value: string) {
  await act(async () => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  for (const one of Object.values(hoisted)) one.mockReset();
  hoisted.confirm.mockResolvedValue(true);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the panel of one step", () => {
  it("says so while nothing is pressed", async () => {
    await render({ action: action(), stepId: null, onRemoved: () => undefined });
    expect(container.textContent).toContain(t("auto.act.stepNone"));
    expect(selects()).toHaveLength(0);
  });

  it("writes the prompt onto the step when the caret leaves the box", async () => {
    await render({ action: action(), stepId: 11, onRemoved: () => undefined });
    const box = container.querySelector("textarea")!;
    await typeInto(box, "take the next ready one");
    expect(hoisted.editStep).not.toHaveBeenCalled();
    await act(async () => box.dispatchEvent(new FocusEvent("focusout", { bubbles: true })));
    expect(hoisted.editStep).toHaveBeenCalledWith(11, { prompt: "take the next ready one" });
  });

  it("declares an input on the step, not on the action", async () => {
    await render({ action: action(), stepId: 11, onRemoved: () => undefined });
    const line = declareLine(t("auto.step.inputName"));
    await typeInto(line.querySelector("input")!, "draft");
    await act(async () => line.querySelector<HTMLButtonElement>("button")!.click());
    expect(hoisted.declareInput).toHaveBeenCalledWith("step", 11, {
      name: "draft",
      kind: "value",
    });
  });
});

describe("what happens after a way out", () => {
  const two = action({
    steps: [step(), step({ id: 12, name: "Review", exits: [{ id: 30, outputs: [] }] })],
  });

  it("offers the other steps, numbered as the picture draws them, and sends the line on this picture", async () => {
    await render({ action: two, stepId: 11, onRemoved: () => undefined });
    const next = nextPicks()[0]!;
    const offered = [...next.options].map((one) => one.textContent);
    expect(offered).toContain(t("auto.step.nextGo").replace("{name}", "2. Review"));
    expect(offered.some((one) => one?.includes("1. "))).toBe(false);
    expect([...next.querySelectorAll("optgroup")].map((one) => one.label)).toEqual([
      t("auto.step.nextGroupStep"),
      t("auto.step.nextGroupExit"),
      t("auto.step.nextGroupEnd"),
    ]);
    await pick(next, "go:12");
    expect(hoisted.addEdge).toHaveBeenCalledWith(
      "action",
      { boxId: 11, exitName: undefined },
      { ends: "go", to: 12 },
    );
  });

  it("offers leaving the action by each way out it declares, and says which", async () => {
    const declared = action({
      exits: [{ id: 20, outputs: [] }, { id: 21, name: "gave up", outputs: [] }],
    });
    await render({ action: declared, stepId: 11, onRemoved: () => undefined });
    const next = nextPicks()[0]!;
    expect([...next.options].map((one) => one.textContent)).toContain(
      t("auto.step.nextExit").replace("{name}", "gave up"),
    );
    await pick(next, "exit:gave up");
    expect(hoisted.addEdge).toHaveBeenCalledWith(
      "action",
      { boxId: 11, exitName: undefined },
      { ends: "exit", exitTo: "gave up" },
    );
    await pick(next, "exit:");
    expect(hoisted.addEdge).toHaveBeenLastCalledWith(
      "action",
      { boxId: 11, exitName: undefined },
      { ends: "exit", exitTo: undefined },
    );
  });

  it("reads a line that leaves the action back as the way out it returns to", async () => {
    const leaving = action({
      exits: [{ id: 20, outputs: [] }, { id: 21, name: "gave up", outputs: [] }],
      edges: [{ id: 5, fromId: 11, ends: "exit", exitTo: "gave up" }],
    });
    await render({ action: leaving, stepId: 11, onRemoved: () => undefined });
    expect(nextPicks()[0]!.value).toBe("exit:gave up");
  });

  it("takes the line away where the reader says nothing is decided yet", async () => {
    const wired = action({
      steps: two.steps,
      edges: [{ id: 5, fromId: 11, toId: 12, ends: "go" }],
    });
    await render({ action: wired, stepId: 11, onRemoved: () => undefined });
    expect(nextPicks()[0]!.value).toBe("go:12");
    await pick(nextPicks()[0]!, "");
    expect(hoisted.removeEdge).toHaveBeenCalledWith(5);
  });

  it("gives the error way out that pulldown and nothing else", async () => {
    await render({ action: action(), stepId: 11, onRemoved: () => undefined });
    const row = container.querySelector(".autostep__exiterr")!;
    expect(row.textContent).toContain(t("auto.pic.errorExit"));
    expect(row.querySelector("input")).toBeNull();
    expect(row.querySelectorAll("button")).toHaveLength(0);
    expect(row.querySelectorAll("select")).toHaveLength(1);
  });
});

describe("the step a placement opens first", () => {
  it("says so on the step that is it", async () => {
    await render({ action: action(), stepId: 11, onRemoved: () => undefined });
    expect(container.textContent).toContain(t("auto.act.entryIs"));
    expect(buttons().some((one) => one.textContent === t("auto.act.entrySet"))).toBe(false);
  });

  it("names this step where another one is it", async () => {
    await render({
      action: action({ entryStepId: 12, steps: [step(), step({ id: 12, name: "Review" })] }),
      stepId: 11,
      onRemoved: () => undefined,
    });
    await act(async () => button(t("auto.act.entrySet")).click());
    expect(hoisted.setEntry).toHaveBeenCalledWith(4, 11);
  });
});

describe("taking a step out", () => {
  it("asks first, and hands the screen back its selection", async () => {
    const onRemoved = vi.fn();
    await render({ action: action(), stepId: 11, onRemoved });
    await act(async () => button(t("auto.act.stepRemove")).click());
    expect(hoisted.confirm).toHaveBeenCalledWith(t("auto.act.stepRemoveConfirm"));
    expect(hoisted.removeStep).toHaveBeenCalledWith(11);
    expect(onRemoved).toHaveBeenCalled();
  });

  it("writes nothing where the reader says no", async () => {
    hoisted.confirm.mockResolvedValue(false);
    const onRemoved = vi.fn();
    await render({ action: action(), stepId: 11, onRemoved });
    await act(async () => button(t("auto.act.stepRemove")).click());
    expect(hoisted.removeStep).not.toHaveBeenCalled();
    expect(onRemoved).not.toHaveBeenCalled();
  });

  it("names the box the reader is looking at, not the one the line came from", async () => {
    await render({ action: action(), stepId: 11, onRemoved: () => undefined });
    expect(boxes()[0]!.value).toBe("Take the next task");
  });
});
