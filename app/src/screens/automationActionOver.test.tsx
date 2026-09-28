// @vitest-environment jsdom
// An action's build screen opened over the automation it was reached from (`AMB-D-1004`). The read
// and the write doors are stubbed; the screen, its panel and the column that panel lands in run for
// real.
//
// What these guard: **only the back closes it** — a press on the backdrop and Escape leave it where
// it is; **the back is named after the automation**, which is where it lands, and the head names the
// box by its number; **"open full screen" is the other way out**; **its panel lands in its own
// column**, the shell's being behind the backdrop; and **an action still being made is closed only by
// the pair at its foot**, "stop making it" only once the reader has said so to the question it asks.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionDetailDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  action: null as AutomationActionDetailDto | null,
  editAction: vi.fn(),
  finish: vi.fn(),
  abandon: vi.fn(),
  confirm: vi.fn(),
  editStep: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  // The rows number their step by the automation's picture; none is read here.
  useAutomation: () => null,
  useAutomationAction: () => hoisted.action,
  useAutomationActions: () => [],
  editAutomationAction: hoisted.editAction,
  finishCreatingAutomationAction: hoisted.finish,
  abandonAutomationAction: hoisted.abandon,
  editAutomationStep: hoisted.editStep,
  setAutomationWire: vi.fn(),
  clearAutomationWire: vi.fn(),
  addAutomationEdge: vi.fn(),
  editAutomationEdge: vi.fn(),
  removeAutomationEdge: vi.fn(),
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
vi.mock("../core/dialog", () => ({ confirmDialog: hoisted.confirm }));
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t, tf } from "../core/i18n";
import { AutomationActionOver } from "./AutomationActionOver";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const action: AutomationActionDetailDto = {
  id: 4,
  name: "Take one",
  note: "",
  global: false,
  usedBy: 1,
  entryStepId: undefined,
  steps: [],
  edges: [],
  wires: [],
  exits: [],
  inputs: [],
  settings: [],
  heldBy: [],
  placedOn: [],
};

const onBack = vi.fn();
const onFull = vi.fn();
const onAbandoned = vi.fn();

async function render() {
  await act(async () => {
    root.render(
      createElement(AutomationActionOver, { actionId: 4, automationName: "Nightly", onBack, onFull, onAbandoned }),
    );
  });
}

const buttons = () => [...document.body.querySelectorAll<HTMLButtonElement>(".actover button")];
const press = (label: string) => buttons().find((one) => one.textContent?.includes(label))!;

beforeEach(() => {
  hoisted.action = action;
  onBack.mockReset();
  onFull.mockReset();
  onAbandoned.mockReset();
  hoisted.finish.mockReset().mockResolvedValue(undefined);
  hoisted.abandon.mockReset().mockResolvedValue(undefined);
  hoisted.confirm.mockReset();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("an action opened over its automation", () => {
  it("goes back by the press named after the automation", async () => {
    await render();
    await act(async () => press(tf("auto.over.back", { name: "Nightly" })).click());
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("stays open on a press outside it and on Escape", async () => {
    await render();
    const backdrop = document.body.querySelector<HTMLElement>(".modal__overlay")!;
    await act(async () => {
      backdrop.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
      backdrop.click();
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(onBack).not.toHaveBeenCalled();
    expect(document.body.querySelector(".actover")).not.toBeNull();
  });

  it("names the box it is placed in by its number on the picture", async () => {
    await act(async () => {
      root.render(
        createElement(AutomationActionOver, { actionId: 4, automationName: "Nightly", boxNo: 3, onBack, onFull, onAbandoned }),
      );
    });
    const where = document.body.querySelector(".actover__where")!;
    expect(where.querySelector(".autopic__no")?.textContent).toBe("3");
    expect(where.textContent).toContain(t("auto.over.placed"));
  });

  it("opens the action full screen by its own press", async () => {
    await render();
    await act(async () => press(t("auto.over.full")).click());
    expect(onFull).toHaveBeenCalledTimes(1);
  });

  it("draws its panel in its own column, shown only while a panel stands", async () => {
    await render();
    const pane = document.body.querySelector<HTMLElement>(".actover__pane")!;
    expect(pane.hidden).toBe(true);
    await act(async () => press(t("auto.act.edit")).click());
    expect(pane.hidden).toBe(false);
    expect(pane.querySelector(".actpanel")).not.toBeNull();
  });
});

describe("an action still being made, opened over its automation", () => {
  beforeEach(() => {
    hoisted.action = { ...action, draft: true };
  });

  it("says it is being made and offers neither the back nor the full screen", async () => {
    await render();
    expect(document.body.querySelector(".actover__draft")?.textContent).toBe(t("chip.draft"));
    expect(buttons().some((one) => one.textContent?.includes(tf("auto.over.back", { name: "Nightly" })))).toBe(false);
    expect(buttons().some((one) => one.textContent?.includes(t("auto.over.full")))).toBe(false);
  });

  it("finishes creating and then goes back", async () => {
    await render();
    await act(async () => press(t("auto.over.finish")).click());
    expect(hoisted.finish).toHaveBeenCalledWith(4);
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("gives the action up only once the reader says so", async () => {
    await render();
    hoisted.confirm.mockResolvedValue(false);
    await act(async () => press(t("auto.over.abandon")).click());
    expect(hoisted.abandon).not.toHaveBeenCalled();
    expect(onAbandoned).not.toHaveBeenCalled();

    hoisted.confirm.mockResolvedValue(true);
    await act(async () => press(t("auto.over.abandon")).click());
    expect(hoisted.confirm).toHaveBeenLastCalledWith(tf("auto.over.abandonConfirm", { name: "Take one" }));
    expect(hoisted.abandon).toHaveBeenCalledWith(4);
    expect(onAbandoned).toHaveBeenCalledTimes(1);
  });
});
