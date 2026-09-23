// @vitest-environment jsdom
// The screen one library action is built in (`AMB-T-5315`). The read and the write doors are
// stubbed; the picture, the panels and what each press opens run for real.
//
// What these guard: **the steps inside the action are the boxes of the picture** (`AMB-D-949`), so a
// reader presses a step rather than reading a list; **the press that writes the first one says so
// while the picture is empty**, that being where there is no line to press and nowhere else to
// start; **an action nothing opens says so**, that being what the launch check would refuse a
// placement of it for; and **what the action is for is written when the caret leaves it**, the way the
// automation's notes are (`AMB-D-952`).
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
const textareas = () => [...container.querySelectorAll("textarea")];

/** The note's box — the field labelled with it, on the part of the screen that is the action's own. */
function noteBox(): HTMLTextAreaElement {
  const field = [...container.querySelectorAll("label")].find(
    (one) => one.querySelector(".autostep__label")?.textContent === t("auto.actions.note"),
  );
  return field!.querySelector("textarea")!;
}

/** Open the declaration in the panel, the way a reader does: its one button over the picture. */
async function openDeclaration() {
  const edit = buttons().find((one) => one.textContent === t("auto.act.declEdit"))!;
  await act(async () => edit.click());
}

/** Typing into a controlled field: React listens for `input`, not for the value being assigned. */
function type(field: HTMLTextAreaElement, text: string) {
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(field, text);
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

/** The caret leaving a field, which is what React's `onBlur` listens for. */
function leave(field: HTMLElement) {
  field.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
}

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
      // The step a placement opens first wears the mark the automation picture puts on its entry.
      `${t("auto.pic.entry")}Take the next task`,
      "Review",
    ]);
  });

  it("opens on the picture alone, and pins what the pressed step holds beside it", async () => {
    await render();
    expect(container.querySelector(".actpanel")).toBeNull();
    await act(async () => nodes()[0]!.click());
    expect(container.querySelector(".actpanel__title")?.textContent).toBe("Take the next task");
    expect(textareas().map((one) => one.value)).toContain("take one");
  });

  it("closes the panel from its own close button", async () => {
    await render();
    await act(async () => nodes()[0]!.click());
    await act(async () => container.querySelector<HTMLButtonElement>(".actpanel__close")!.click());
    expect(container.querySelector(".actpanel")).toBeNull();
  });

  it("reads the declaration in the band without opening anything", async () => {
    hoisted.action = action({
      note: "Takes the next task off the queue\nand says which",
      exits: [
        { id: 1, outputs: [] },
        { id: 2, name: "*", outputs: [] },
      ],
    });
    await render();
    expect(container.querySelector(".actdecl__note")?.textContent).toBe("Takes the next task off the queue");
    expect(container.querySelector(".actdecl")?.textContent).toContain(t("auto.pic.errorExit"));
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

  it("shows what the action is for, and writes it when the caret leaves the box", async () => {
    hoisted.action = action({ note: "Takes the next task off the queue" });
    await render();
    await openDeclaration();
    expect(noteBox().value).toBe("Takes the next task off the queue");
    expect(container.textContent).toContain(t("auto.actions.noteWhat"));

    await act(async () => {
      type(noteBox(), "Takes one task");
      leave(noteBox());
    });
    expect(hoisted.editAction).toHaveBeenCalledWith(4, { note: "Takes one task" });
  });

  it("writes nothing when the caret leaves a note it did not change", async () => {
    hoisted.action = action({ note: "Takes one task" });
    await render();
    await openDeclaration();
    await act(async () => leave(noteBox()));
    expect(hoisted.editAction).not.toHaveBeenCalled();
  });

  it("says when no step is opened first", async () => {
    hoisted.action = action({ entryStepId: undefined });
    await render();
    expect(container.textContent).toContain(t("auto.act.noEntry"));
  });
});
