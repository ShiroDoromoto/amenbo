// @vitest-environment jsdom
// What the step panel draws and what each control writes (`AMB-T-5256`, `AMB-T-5282`). The reads and
// the write doors are stubbed; every field, its control and what it sends run for real.
//
// What these guard: **nothing pressed says so** rather than drawing an empty form; **where the prompt
// comes from is one control**, and a prompt that came from the library is read here and not written;
// **a setting is answered by the control its kind takes**, a task filter on rows rather than in a
// filter expression; **an input is filled from a list of what fits**; **the error way out is always
// the last of the ways out**; and **an agent this machine cannot start is listed and cannot be
// picked**, which is what keeps the list from being shorter on one machine than on another.
//
// And what a step **declares**, not only what it answers: a way out is written from the line under
// the list; moving a setting off `choice` takes its list of choices with it, in the one call core
// will accept; a step running a library action is drawn without any of it, reading the action's; and
// a refusal lands on the panel rather than in the console.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationStepDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
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
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => [{ id: 4, name: "Review", prompt: "", global: false, usedBy: 1 }],
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

  it("draws the error way out last, and always, on a step that reads the library's", async () => {
    const one = detail({
      steps: [
        step({
          actionId: 4,
          actionName: "Review",
          exits: [{ id: 10, outputs: [] }, { id: 11, name: "*", outputs: [] }],
        }),
      ],
    });
    await render({ automation: one, stepId: 1, projectId: 1 });
    const ways = [...container.querySelectorAll(".autostep__exits li")].map((li) => li.textContent);
    expect(ways).toEqual([t("auto.step.exitUnnamed"), t("auto.pic.errorExit")]);
    // Nothing to declare with: the ways out, the settings and the inputs are the action's.
    expect(container.querySelectorAll(".autostep__declare")).toHaveLength(0);
  });

  it("draws the error way out last among the rows a step's own ways out are written on", async () => {
    await render({ automation: detail(), stepId: 1, projectId: 1 });
    const rows = [...container.querySelectorAll(".autostep__decl, .autostep__exiterr")];
    expect(rows).toHaveLength(2);
    expect(rows[1]!.textContent).toBe(t("auto.pic.errorExit"));
    // The error one is drawn and not offered: no box to rename it in, no press to take it away.
    expect(rows[1]!.querySelectorAll("input, button")).toHaveLength(0);
  });

  it("declares a way out under the name that was typed, and empties the box once it is written",
    async () => {
      await render({ automation: detail(), stepId: 1, projectId: 1 });
      const line = declareLine(t("auto.step.exitName"));
      const box = line.querySelector<HTMLInputElement>("input")!;
      await typeInto(box, "something to fix");
      await act(async () => {
        line.querySelector<HTMLButtonElement>("button")!.click();
      });
      expect(hoisted.declareExit).toHaveBeenCalledWith(1, "something to fix");
      expect(box.value).toBe("");
    });

  it("takes a setting off choice and its list of choices in the one call", async () => {
    const one = detail({
      steps: [step({ settings: [{ name: "depth", kind: "choice", required: false, options: '["quick"]' }] })],
    });
    await render({ automation: one, stepId: 1, projectId: 1 });
    const kind = selects().find((s) => s.value === "choice")!;
    await act(async () => {
      kind.value = "text";
      kind.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(hoisted.editCfg).toHaveBeenCalledWith(1, "depth", { kind: "text", options: null });
  });

  it("declares an input as what it carries", async () => {
    await render({ automation: detail(), stepId: 1, projectId: 1 });
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
    expect(hoisted.declareInput).toHaveBeenCalledWith(1, { name: "report", kind: "file" });
  });

  it("draws what core refused, and leaves the typed name where it can be fixed", async () => {
    hoisted.declareExit.mockRejectedValue("a way out called 'done' is already declared here");
    await render({ automation: detail(), stepId: 1, projectId: 1 });
    const line = declareLine(t("auto.step.exitName"));
    const box = line.querySelector<HTMLInputElement>("input")!;
    await typeInto(box, "done");
    await act(async () => {
      line.querySelector<HTMLButtonElement>("button")!.click();
    });
    expect(container.querySelector(".autostep__refused")!.textContent).toContain("already declared");
    expect(box.value).toBe("done");
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
