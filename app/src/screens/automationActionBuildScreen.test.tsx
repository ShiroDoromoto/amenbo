// @vitest-environment jsdom
// The screen one library action is built in (`AMB-T-5315`). The read and the write doors are
// stubbed; the picture, the panels and what each press opens run for real.
//
// What these guard: **the steps inside the action are the boxes of the picture** (`AMB-D-949`), so a
// reader presses a step rather than reading a list; **the press that writes the first one says so
// while the picture is empty**, that being where there is no line to press and nowhere else to
// start; **an action nothing opens says so**, that being what the launch check would refuse a
// placement of it for; and **what the action is for is written when the caret leaves it**, the way the
// automation's notes are (`AMB-D-952`); and **the head says what is saved, and saves or discards the
// draft from its presses** (`AMB-D-1005`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AutomationActionDetailDto,
  AutomationLaunchBlockDto,
  AutomationRunCardDto,
  AutomationStepDto,
} from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  action: null as AutomationActionDetailDto | null,
  editAction: vi.fn(),
  editStep: vi.fn(),
  save: vi.fn(),
  discard: vi.fn(),
  confirm: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  // The rows number their step by the automation's picture; none is read here.
  useAutomation: () => null,
  useAutomationAction: () => hoisted.action,
  useAutomationActions: () => [],
  editAutomationAction: hoisted.editAction,
  editAutomationStep: hoisted.editStep,
  saveAutomationAction: hoisted.save,
  discardAutomationAction: hoisted.discard,
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
vi.mock("../core/dialog", () => ({ confirmDialog: hoisted.confirm }));

import { errSentence, errText, t, tf } from "../core/i18n";
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
    exits: [{ id: 10, name: "完了", outputs: [] }, { id: 19, name: "*", outputs: [] }],
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
    exits: [{ id: 20, name: "完了", outputs: [] }],
    inputs: [],
    settings: [],
    heldBy: [],
    placedOn: [],
    unsaved: false,
    saveBlocks: [],
    ...over,
  };
}

async function render() {
  await act(async () => {
    root.render(
      createElement(AutomationActionBuildScreen, {
        id: 4,
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
  hoisted.save.mockReset();
  hoisted.discard.mockReset();
  hoisted.confirm.mockReset().mockResolvedValue(true);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the action build screen", () => {
  it("draws the steps inside the action as the boxes of the picture", async () => {
    hoisted.action = action({
      steps: [step(), step({ id: 12, name: "Review" })],
      edges: [{ id: 5, fromId: 11, exitName: "完了", toId: 12, ends: "go" }],
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

  it("arrives with the step it was opened on pressed", async () => {
    hoisted.action = action({
      steps: [step(), step({ id: 12, name: "Review" })],
      edges: [{ id: 5, fromId: 11, exitName: "完了", toId: 12, ends: "go" }],
    });
    await act(async () => {
      root.render(createElement(AutomationActionBuildScreen, { id: 4, openingStep: 12, onBack: () => undefined }));
    });
    expect(titleBox()?.value).toBe("Review");
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
          onBack: () => undefined,
          onGoToAutomation: goTo,
        }),
      );
    });
    await openDeclaration();
    const names = [...container.querySelectorAll(".actplaced__one")];
    expect(names.map((one) => one.textContent?.replace(" ↗", ""))).toEqual(["Dev loop", "Elsewhere"]);
    // Every project's is gone to, since each opens on the one automations screen (`AMB-D-992`).
    await act(async () => (names[0] as HTMLButtonElement).click());
    expect(goTo).toHaveBeenCalledWith(1, 7);
    await act(async () => (names[1] as HTMLButtonElement).click());
    expect(goTo).toHaveBeenCalledWith(2, 8);
  });

  it("closes the panel from its own close button", async () => {
    await render();
    await act(async () => nodes()[0]!.click());
    await act(async () => container.querySelector<HTMLButtonElement>(".actpanel__close")!.click());
    expect(container.querySelector(".actpanel")).toBeNull();
  });

  // The head carries the name, the reach and the count, and what it is for is read in the panel "Edit"
  // opens rather than over the picture (`AMB-T-5526`).
  it("heads the screen with the action, and opens its input and output from the frames of the picture", async () => {
    hoisted.action = action({
      note: "Takes the next task off the queue\nand says which",
      exits: [
        { id: 1, name: "完了", outputs: [] },
        { id: 2, name: "*", outputs: [] },
      ],
    });
    await render();
    const head = container.querySelector(".actbuild__head")?.textContent ?? "";
    expect(head).not.toContain("Takes the next task off the queue");
    expect(container.querySelector(".actbuild__head .actdecl__used")).not.toBeNull();
    expect(has(t("auto.act.edit"))).toBe(true);
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

});

// A global action is written wherever it is opened (`AMB-D-992`): whether it can be changed is its
// own, not the screen's it was reached from.
describe("a global action", () => {
  async function render() {
    hoisted.action = action({ global: true });
    await act(async () => {
      root.render(createElement(AutomationActionBuildScreen, { id: 4, onBack: () => undefined }));
    });
  }

  it("is written, with Edit on its head and no lock", async () => {
    await render();
    expect(has(t("auto.act.edit"))).toBe(true);
    expect(container.querySelector('[data-icon="lock"]')).toBeNull();
  });

  it("opens a step with the fields open", async () => {
    await render();
    await act(async () => { nodes()[0].click(); });
    expect(container.querySelector(".actpanel__body")).not.toBeNull();
    expect(container.querySelector("fieldset:disabled")).toBeNull();
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
    pauseBeforeNextTask: false,
    pausableBeforeNextTask: false,
    waiting: false,
    stepsDone: 1,
    reportWithheld: [],
    acknowledged: false,
    ...over,
  };
}

describe("an action a run is going on (AMB-D-1015)", () => {
  const goToRun = vi.fn();
  async function renderHeld(runs: AutomationRunCardDto[] = [heldRun()]) {
    hoisted.action = action({ heldBy: runs });
    await act(async () => {
      root.render(
        createElement(AutomationActionBuildScreen, {
          id: 4,
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

  it("opens what it is for and its steps with the fields open", async () => {
    await renderHeld();
    await act(async () => {
      buttons().find((one) => one.textContent === t("auto.act.edit"))!.click();
    });
    expect(noteBox().closest("fieldset")).toBeNull();
    expect(container.querySelector<HTMLInputElement>(".actpanel__titlein")?.readOnly).toBe(false);
    await act(async () => { nodes()[0].click(); });
    expect(container.querySelector("fieldset:disabled")).toBeNull();
    expect(container.querySelector<HTMLInputElement>(".actpanel__titlein")?.readOnly).toBe(false);
  });

  it("offers adding a step, with no lock on the band or the head", async () => {
    await renderHeld();
    expect(has(t("auto.act.stepAdd"))).toBe(true);
    expect(container.querySelector('[data-icon="lock"]')).toBeNull();
  });

  it("says nothing of runs while none is going", async () => {
    await renderHeld([]);
    expect(container.querySelector(".autoheld")).toBeNull();
    expect(has(t("auto.act.edit"))).toBe(true);
  });
});

describe("saving the action", () => {
  const head = () => container.querySelector(".actsaved")?.textContent;
  const button = (label: string) => buttons().find((one) => one.textContent === label)!;
  const blocks = () => [...container.querySelectorAll(".autolaunch__blocks > li")].map((one) => one.textContent);
  const saved = { version: 3, savedAt: "2026-10-01T09:00:00Z" };
  const empty: AutomationLaunchBlockDto = {
    code: "not_ready_automation_action_empty",
    message_en: "English for not_ready_automation_action_empty",
    fields: { action: "Take one" },
  };
  const openExit: AutomationLaunchBlockDto = {
    code: "not_ready_automation_exit_open",
    message_en: "English for not_ready_automation_exit_open",
    fields: { inside_step: "11", step: "Take the next task", exit: "完了" },
  };

  it("says nothing is saved yet, and offers no discard with no version to go back to", async () => {
    hoisted.action = action({ unsaved: true });
    await render();
    expect(head()).toBe(t("auto.saved.never"));
    expect(button(t("auto.saved.save")).disabled).toBe(false);
    expect(button(t("auto.saved.discard")).disabled).toBe(true);
  });

  it("names the version saved last, with both presses shut while nothing is unsaved", async () => {
    hoisted.action = action({ saved, unsaved: false });
    await render();
    expect(head()).toContain("3");
    expect(head()).not.toBe(t("auto.saved.unsaved"));
    expect(button(t("auto.saved.save")).disabled).toBe(true);
    expect(button(t("auto.saved.discard")).disabled).toBe(true);
  });

  it("saves from its press", async () => {
    hoisted.action = action({ saved, unsaved: true });
    await render();
    expect(head()).toBe(t("auto.saved.unsaved"));
    await act(async () => { button(t("auto.saved.save")).click(); });
    expect(hoisted.save).toHaveBeenCalledWith(4);
  });

  it("asks before discarding, and discards nothing when the answer is no", async () => {
    hoisted.action = action({ saved, unsaved: true });
    await render();
    hoisted.confirm.mockResolvedValueOnce(false);
    await act(async () => { button(t("auto.saved.discard")).click(); });
    expect(hoisted.confirm).toHaveBeenCalledWith(tf("auto.saved.discardConfirm", { version: 3 }));
    expect(hoisted.discard).not.toHaveBeenCalled();
    await act(async () => { button(t("auto.saved.discard")).click(); });
    expect(hoisted.discard).toHaveBeenCalledWith(4);
  });

  it("lists why it cannot be saved and shuts the press, and a reason about a step opens that step", async () => {
    hoisted.action = action({ unsaved: true, saveBlocks: [empty, openExit] });
    await render();
    expect(container.querySelector(".autolaunch__head")?.textContent).toBe(t("auto.saved.blocked"));
    expect(blocks()).toEqual([errSentence(empty), errSentence(openExit)]);
    expect(button(t("auto.saved.save")).disabled).toBe(true);
    expect(container.querySelector(".actpanel")).toBeNull();
    await act(async () => { container.querySelector<HTMLButtonElement>(".autolaunch__go")!.click(); });
    expect(titleBox()?.value).toBe("Take the next task");
  });

  it("lists no reasons to save while nothing is unsaved", async () => {
    hoisted.action = action({ saved, unsaved: false, saveBlocks: [empty] });
    await render();
    expect(container.querySelector(".autolaunch")).toBeNull();
  });

  it("puts a refused save where the reasons are", async () => {
    hoisted.save.mockRejectedValueOnce(new Error("held"));
    hoisted.action = action({ unsaved: true });
    await render();
    await act(async () => { button(t("auto.saved.save")).click(); });
    expect(container.querySelector(".autolaunch__refused")?.textContent).toBe(errText(new Error("held")));
  });
});
