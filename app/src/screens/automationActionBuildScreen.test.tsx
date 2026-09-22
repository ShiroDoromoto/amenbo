// @vitest-environment jsdom
// The screen one library action is built in (`AMB-T-5315`). The read and the write doors are
// stubbed; the picture, the panels and what each press opens run for real.
//
// What these guard: **the steps inside the action are the boxes of the picture** (`AMB-D-949`), so a
// reader presses a step rather than reading a list; **the way to write the first one is there while
// the picture is empty**, which is where there is no line to press and nowhere else to start; **it
// is gone once there is a step**, the road in from then on being the `+` on a line; and **an action
// nothing opens says so**, that being what the launch check would refuse a placement of it for.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionDetailDto, AutomationStepDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  action: null as AutomationActionDetailDto | null,
  editAction: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  useAutomationAction: () => hoisted.action,
  useAutomationActions: () => [],
  editAutomationAction: hoisted.editAction,
  editAutomationStep: vi.fn(),
  setAutomationWire: vi.fn(),
  clearAutomationWire: vi.fn(),
  setAutomationEdge: vi.fn(),
  clearAutomationEdge: vi.fn(),
  declareAutomationExit: vi.fn(),
  renameAutomationExit: vi.fn(),
  removeAutomationExit: vi.fn(),
  declareAutomationCfg: vi.fn(),
  editAutomationCfg: vi.fn(),
  removeAutomationCfg: vi.fn(),
  declareAutomationInput: vi.fn(),
  editAutomationInput: vi.fn(),
  removeAutomationInput: vi.fn(),
  removeAutomationStep: vi.fn(),
  setAutomationActionEntry: vi.fn(),
  addAutomationStep: vi.fn(),
  insertAutomationActionStep: vi.fn(),
  insertAutomationStep: vi.fn(),
  addAutomationOutput: vi.fn(),
}));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t } from "../core/i18n";
import { AutomationActionBuildScreen } from "./AutomationActionBuildScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function step(over: Partial<AutomationStepDto> = {}): AutomationStepDto {
  return {
    id: 11,
    name: "Take the next task",
    prompt: "take one",
    agent: "claude-code",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    exits: [{ id: 10, outputs: [] }, { id: 19, name: "*", outputs: [] }],
    inputs: [],
    ...over,
  };
}

function action(over: Partial<AutomationActionDetailDto> = {}): AutomationActionDetailDto {
  return {
    id: 4,
    name: "Take one",
    global: false,
    usedBy: 1,
    entryStepId: 11,
    steps: [step()],
    edges: [],
    wires: [],
    exits: [{ id: 20, outputs: [] }],
    inputs: [],
    settings: [],
    ...over,
  };
}

async function render() {
  await act(async () => {
    root.render(
      createElement(AutomationActionBuildScreen, {
        id: 4,
        projectId: 1,
        onBack: () => undefined,
      }),
    );
  });
}

const nodes = () => [...container.querySelectorAll<HTMLButtonElement>(".autopic__node")];
const buttons = () => [...container.querySelectorAll<HTMLButtonElement>("button")];
const has = (label: string) => buttons().some((one) => one.textContent === label);

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.action = action();
  hoisted.editAction.mockReset();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the action build screen", () => {
  it("draws the steps inside the action as the boxes of the picture", async () => {
    hoisted.action = action({
      steps: [step(), step({ id: 12, name: "Review" })],
      edges: [{ id: 5, fromId: 11, toId: 12, ends: "go" }],
    });
    await render();
    expect(nodes().map((one) => one.textContent)).toEqual([
      "Take the next task",
      "Review",
    ]);
  });

  it("shows what the pressed step holds", async () => {
    await render();
    expect(container.textContent).toContain(t("auto.act.stepNone"));
    await act(async () => nodes()[0]!.click());
    expect(container.querySelector("textarea")!.value).toBe("take one");
  });

  it("offers the first step while the picture is empty, and not once there is one", async () => {
    hoisted.action = action({ steps: [], entryStepId: undefined });
    await render();
    expect(container.textContent).toContain(t("auto.act.empty"));
    expect(has(t("auto.act.firstStep"))).toBe(true);

    hoisted.action = action();
    await render();
    expect(has(t("auto.act.firstStep"))).toBe(false);
  });

  it("says when no step is opened first", async () => {
    hoisted.action = action({ entryStepId: undefined });
    await render();
    expect(container.textContent).toContain(t("auto.act.noEntry"));
  });
});
