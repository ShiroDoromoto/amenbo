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
import type { AutomationActionDetailDto, AutomationRunCardDto, AutomationStepDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  action: null as AutomationActionDetailDto | null,
  editAction: vi.fn(),
  editStep: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  // The rows number their step by the automation's picture; none is read here.
  useAutomation: () => null,
  useAutomationAction: () => hoisted.action,
  useAutomationActions: () => [],
  editAutomationAction: hoisted.editAction,
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
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t, tf } from "../core/i18n";
import { AutomationActionBuildScreen } from "./AutomationActionBuildScreen";

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
    showNotes: true,
    showDecisions: true,
    showComments: true,
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
    placedOn: [],
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

/** The note's box — the one labelled with it, on the part of the screen that is the action's own. */
function noteBox(): HTMLTextAreaElement {
  return container.querySelector<HTMLTextAreaElement>(
    `textarea[aria-label="${t("auto.actions.note")}"]`,
  )!;
}

/** The panel's head, as the box its name is typed in. */
const titleBox = () => container.querySelector<HTMLInputElement>(".actpanel__titlein");

/** Typing into a controlled one-line box. */
function typeLine(field: HTMLInputElement, text: string) {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, text);
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

/** Open the action itself in the panel, the way a reader does: the edit button on its row. */
async function openDeclaration() {
  const edit = buttons().find((one) => one.textContent === t("auto.act.edit"))!;
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
  hoisted.editStep.mockReset();
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
      // Numbered as a list naming them does, and the step a placement opens first wears the mark
      // the automation picture puts on its entry.
      `1${t("auto.pic.entry")}Take the next task`,
      "2Review",
    ]);
  });

  it("opens on the picture alone, and pins what the pressed step holds beside it", async () => {
    await render();
    expect(container.querySelector(".actpanel")).toBeNull();
    await act(async () => nodes()[0]!.click());
    expect(titleBox()?.value).toBe("Take the next task");
    expect(textareas().map((one) => one.value)).toContain("take one");
  });

  it("renames the step from the panel's head, when the caret leaves it", async () => {
    await render();
    await act(async () => nodes()[0]!.click());
    await act(async () => {
      typeLine(titleBox()!, "Take one");
      leave(titleBox()!);
    });
    expect(hoisted.editStep).toHaveBeenCalledWith(11, { name: "Take one" });
  });

  it("renames the action from the head of its own panel", async () => {
    await render();
    await openDeclaration();
    await act(async () => {
      typeLine(titleBox()!, "Implement");
      leave(titleBox()!);
    });
    expect(hoisted.editAction).toHaveBeenCalledWith(4, { name: "Implement" });
  });

  it("names the automations it is placed on, and goes to one on its press", async () => {
    const goTo = vi.fn();
    hoisted.action = action({
      placedOn: [
        { id: 7, name: "Dev loop", project: 1 },
        { id: 8, name: "Elsewhere", project: 2 },
      ],
    });
    await act(async () => {
      root.render(
        createElement(AutomationActionBuildScreen, {
          id: 4,
          projectId: 1,
          onBack: () => undefined,
          onGoToAutomation: goTo,
        }),
      );
    });
    await openDeclaration();
    const names = [...container.querySelectorAll(".actplaced__one")];
    expect(names.map((one) => one.textContent?.replace(" ↗", ""))).toEqual(["Dev loop", "Elsewhere"]);
    // Another project's automation is not gone to from this project's screen.
    expect(names[1]!.tagName).toBe("SPAN");
    await act(async () => (names[0] as HTMLButtonElement).click());
    expect(goTo).toHaveBeenCalledWith(1, 7);
  });

  it("closes the panel from its own close button", async () => {
    await render();
    await act(async () => nodes()[0]!.click());
    await act(async () => container.querySelector<HTMLButtonElement>(".actpanel__close")!.click());
    expect(container.querySelector(".actpanel")).toBeNull();
  });

  it("reads the action in its row, and opens its input and output from the frames of the picture", async () => {
    hoisted.action = action({
      note: "Takes the next task off the queue\nand says which",
      exits: [
        { id: 1, outputs: [] },
        { id: 2, name: "*", outputs: [] },
      ],
    });
    await render();
    expect(container.querySelector(".actdecl__note")?.textContent).toBe("Takes the next task off the queue");
    expect(container.querySelector(".actpanel")).toBeNull();

    const frames = () => [...container.querySelectorAll<HTMLButtonElement>(".autopic__frame")];
    expect(frames()).toHaveLength(2);
    await act(async () => frames().find((one) => one.classList.contains("autopic__frame--in"))!.click());
    expect(container.querySelector(".actpanel .actbuild__sec")?.textContent).toBe(t("auto.pic.actionIn"));
    await act(async () => frames().find((one) => !one.classList.contains("autopic__frame--in"))!.click());
    expect(container.querySelector(".actpanel .actbuild__sec")?.textContent).toBe(t("auto.pic.actionOut"));
    expect(container.querySelector(".actpanel")?.textContent).toContain(t("auto.pic.errorExit"));
  });

  it("draws a setting as a chip with an empty slot for its answer, and opens a row to declare only on add", async () => {
    hoisted.action = action({
      settings: [{ name: "branch", kind: "choice", required: true }],
    });
    await render();
    const frames = () => [...container.querySelectorAll<HTMLButtonElement>(".autopic__frame")];
    await act(async () => frames().find((one) => one.classList.contains("autopic__frame--in"))!.click());
    const panel = container.querySelector(".actpanel")!;
    expect(panel.querySelector(".actport--cfg")?.textContent).toContain("branch");
    expect(panel.querySelector(".autodecl__slot")?.textContent).toBe(t("auto.decl.answeredOnPlacement"));
    expect(panel.querySelector(".autostep__declare")).toBeNull();
    // What renames or removes it is behind its "⋯".
    expect(panel.querySelector(".autostep__decl")).toBeNull();
    await act(async () => panel.querySelector<HTMLButtonElement>(".autodecl__more")!.click());
    expect(panel.querySelector(".autostep__decl")).not.toBeNull();
    const adds = [...panel.querySelectorAll<HTMLButtonElement>(".autosec__add")];
    await act(async () => adds[1]!.click());
    expect(panel.querySelectorAll(".autostep__declare")).toHaveLength(1);
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
    expect(noteBox().placeholder).toBe(t("auto.actions.notePlaceholder"));

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

// A global action opened from a project (`AMB-D-954`): read there and changed from the sidebar. The
// panels still open, with every control in them shut; nothing adds a step; and the row carries the
// press that goes to where it is changed. Opened from the sidebar, or a project's own, it is written.
describe("a global action opened from a project", () => {
  const goTo = vi.fn();
  async function renderAt(projectId: number | null, global = true) {
    hoisted.action = action({ global });
    await act(async () => {
      root.render(
        createElement(AutomationActionBuildScreen, {
          id: 4,
          projectId,
          onBack: () => undefined,
          onGoToGlobal: goTo,
        }),
      );
    });
  }
  beforeEach(() => goTo.mockClear());

  it("says it is changed from the sidebar, and goes there on the row's press", async () => {
    await renderAt(1);
    expect(container.querySelector('[data-icon="lock"]')).not.toBeNull();
    await act(async () => {
      buttons().find((one) => one.textContent === t("auto.act.openInSidebar"))!.click();
    });
    expect(goTo).toHaveBeenCalledWith(4);
  });

  it("opens what it is for to be read, with the fields shut", async () => {
    await renderAt(1);
    expect(has(t("auto.act.edit"))).toBe(false);
    await act(async () => {
      buttons().find((one) => one.textContent === t("auto.act.read"))!.click();
    });
    expect(noteBox().closest("fieldset")?.disabled).toBe(true);
    expect(titleBox()?.readOnly).toBe(true);
  });

  it("opens a step to be read, with the fields shut", async () => {
    await renderAt(1);
    await act(async () => { nodes()[0].click(); });
    const body = container.querySelector<HTMLFieldSetElement>(".actpanel__body");
    expect(body?.disabled).toBe(true);
    expect(container.querySelector<HTMLButtonElement>(".actpanel__close")?.disabled).toBe(false);
  });

  it("offers no way to add a step", async () => {
    await renderAt(1);
    expect(has(t("auto.act.stepAdd"))).toBe(false);
  });

  it("is written from the sidebar, where it is changed", async () => {
    await renderAt(null);
    expect(has(t("auto.act.edit"))).toBe(true);
    expect(container.querySelector('[data-icon="lock"]')).toBeNull();
  });

  it("leaves a project's own action written from that project", async () => {
    await renderAt(1, false);
    expect(has(t("auto.act.edit"))).toBe(true);
    expect(has(t("auto.act.openInSidebar"))).toBe(false);
  });
});

/** A run going on an automation that places the action. */
function heldRun(over: Partial<AutomationRunCardDto> = {}): AutomationRunCardDto {
  return {
    run: 31,
    project: 2,
    projectName: "Other",
    automation: 9,
    automationName: "Night round",
    status: "running",
    pauseRequested: false,
    waiting: false,
    stepsDone: 1,
    ...over,
  };
}

describe("an action a run is going on (AMB-D-961)", () => {
  const goToRun = vi.fn();
  async function renderHeld(runs: AutomationRunCardDto[] = [heldRun()]) {
    hoisted.action = action({ heldBy: runs });
    await act(async () => {
      root.render(
        createElement(AutomationActionBuildScreen, {
          id: 4,
          projectId: 1,
          onBack: () => undefined,
          onGoToRun: goToRun,
        }),
      );
    });
  }
  beforeEach(() => goToRun.mockClear());

  it("names the runs using it, and goes to a run's pane on its line", async () => {
    await renderHeld();
    expect(container.querySelector(".autoheld")?.textContent).toContain(tf("auto.held.by", { run: 31 }));
    expect(container.querySelector(".autoheld")?.textContent).toContain("Night round");
    await act(async () => { [...container.querySelectorAll<HTMLButtonElement>(".autoheld .btn")].find((b) => b.textContent === t("auto.held.openPane"))!.click(); });
    expect(goToRun).toHaveBeenCalledWith(2, 31);
  });

  it("opens what it is for and its steps to be read, with the fields shut", async () => {
    await renderHeld();
    expect(has(t("auto.act.edit"))).toBe(false);
    await act(async () => {
      buttons().find((one) => one.textContent === t("auto.act.read"))!.click();
    });
    expect(noteBox().closest("fieldset")?.disabled).toBe(true);
    await act(async () => { nodes()[0].click(); });
    expect(container.querySelector<HTMLFieldSetElement>(".actpanel__body")?.disabled).toBe(true);
  });

  it("offers no way to add a step, and does not say it is changed from the sidebar", async () => {
    await renderHeld();
    expect(has(t("auto.act.stepAdd"))).toBe(false);
    // The held band carries its own lock; the one that says "changed from the sidebar" is the head's.
    expect(container.querySelector('.actdecl [data-icon="lock"]')).toBeNull();
  });

  it("says nothing of runs, and holds nothing shut, while none is going", async () => {
    await renderHeld([]);
    expect(container.querySelector(".autoheld")).toBeNull();
    expect(has(t("auto.act.edit"))).toBe(true);
  });
});
