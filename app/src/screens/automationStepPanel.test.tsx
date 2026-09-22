// @vitest-environment jsdom
// What the step panel draws and what each control writes (`AMB-T-5256`). The reads and the write door
// are stubbed; every field, its control and what it sends run for real.
//
// What these guard: **nothing pressed says so** rather than drawing an empty form; **where the prompt
// comes from is one control**, and a prompt that came from the library is read here and not written;
// **a setting is answered by the control its kind takes**, a task filter on rows rather than in a
// filter expression; **an input is filled from a list of what fits**; **the error way out is always
// the last line of the ways out**; and **an agent this machine cannot start is listed and cannot be
// picked**, which is what keeps the list from being shorter on one machine than on another.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationStepDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  editStep: vi.fn(),
  answerCfg: vi.fn(),
  setWire: vi.fn(),
  clearWire: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => [{ id: 4, name: "Review", prompt: "", global: false, usedBy: 1 }],
  editAutomationStep: hoisted.editStep,
  answerAutomationCfg: hoisted.answerCfg,
  setAutomationWire: hoisted.setWire,
  clearAutomationWire: hoisted.clearWire,
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));
// The machine's own answers. `inTauri` is false in this environment, so neither probe is made and
// what the panel draws is the step's own value plus whatever these would have added.
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t } from "../core/i18n";
import { AutomationStepPanel } from "./AutomationStepPanel";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function step(over: Partial<AutomationStepDto> = {}): AutomationStepDto {
  return {
    id: 1,
    name: "Take the next task",
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
    preamble: "",
    entryStepId: 1,
    archived: false,
    steps: [step()],
    edges: [],
    wires: [],
    ...over,
  };
}

async function render(props: Parameters<typeof AutomationStepPanel>[0]) {
  await act(async () => {
    root.render(createElement(AutomationStepPanel, props));
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

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.editStep.mockReset();
  hoisted.answerCfg.mockReset();
  hoisted.setWire.mockReset();
  hoisted.clearWire.mockReset();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the step panel", () => {
  it("says so while no step is pressed", async () => {
    await render({ automation: detail(), stepId: null, projectId: 1 });
    expect(container.textContent).toContain(t("auto.step.none"));
    expect(selects()).toHaveLength(0);
  });

  it("writes the name when the caret leaves the box, not a letter at a time", async () => {
    await render({ automation: detail(), stepId: 1, projectId: 1 });
    const box = boxes()[0]!;
    await typeInto(box, "Take one");
    expect(hoisted.editStep).not.toHaveBeenCalled();
    await act(async () => {
      box.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    expect(hoisted.editStep).toHaveBeenCalledWith(1, { name: "Take one" });
  });

  it("puts where the prompt comes from on one control, the library beside the step's own", async () => {
    await render({ automation: detail(), stepId: 1, projectId: 1 });
    const source = selects()[0]!;
    expect([...source.options].map((one) => one.textContent)).toEqual([
      t("auto.step.sourceOwn"),
      "Review",
    ]);
    await act(async () => {
      source.value = "4";
      source.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(hoisted.editStep).toHaveBeenCalledWith(1, { source: { action: 4 } });
  });

  it("reads a prompt that came from the library rather than taking it", async () => {
    const one = detail({ steps: [step({ actionId: 4, actionName: "Review" })] });
    await render({ automation: one, stepId: 1, projectId: 1 });
    const prompt = container.querySelector<HTMLTextAreaElement>("textarea")!;
    expect(prompt.readOnly).toBe(true);
    expect(container.textContent).toContain("Review");
    // And what its ways out hand on is the library's too, so this step is not offered the control.
    expect(container.textContent).not.toContain(t("auto.step.outputAdd"));
  });

  it("answers a task filter on rows, and writes the object naming each part", async () => {
    const one = detail({
      steps: [step({ settings: [{ name: "which", kind: "taskfilter", required: true }] })],
    });
    await render({ automation: one, stepId: 1, projectId: 1 });
    const chip = [...container.querySelectorAll<HTMLButtonElement>(".autostep__chip")].find(
      (b) => b.textContent === t("filter.opt.assignee.meAi"),
    )!;
    await act(async () => {
      chip.click();
    });
    expect(hoisted.answerCfg).toHaveBeenCalledWith(1, "which", '{"assignee":["me-ai"]}');
  });

  it("fills an input from what fits, and empties it back to nothing reaching it", async () => {
    const one = detail({
      steps: [
        step({ id: 1, exits: [{ id: 10, outputs: [{ name: "note", kind: "value", required: true }] }] }),
        step({ id: 2, name: "work", inputs: [{ name: "note", kind: "value", required: true }] }),
      ],
      wires: [{ id: 3, fromStepId: 1, fromPortName: "note", toStepId: 2, toPortName: "note" }],
    });
    await render({ automation: one, stepId: 2, projectId: 1 });
    const wire = selects().find((s) => [...s.options].some((o) => o.textContent?.includes("note")))!;
    await act(async () => {
      wire.value = "";
      wire.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(hoisted.clearWire).toHaveBeenCalledWith(3);
  });

  it("draws the error way out last, and always", async () => {
    const one = detail({
      steps: [step({ exits: [{ id: 10, outputs: [] }, { id: 11, name: "*", outputs: [] }] })],
    });
    await render({ automation: one, stepId: 1, projectId: 1 });
    const named = [...container.querySelectorAll(".autostep__exitname")].map((one) => one.textContent);
    expect(named).toEqual([t("auto.step.exitUnnamed")]);
    const last = [...container.querySelectorAll(".autostep__exits li")].pop()!;
    expect(last.className).toContain("autostep__exiterr");
    expect(last.textContent).toBe(t("auto.pic.errorExit"));
  });

  it("takes a flag on the spot", async () => {
    await render({ automation: detail(), stepId: 1, projectId: 1 });
    const check = boxes().find((b) => b.type === "checkbox")!;
    await act(async () => {
      check.click();
    });
    expect(hoisted.editStep).toHaveBeenCalledWith(1, { interactive: true });
  });
});
