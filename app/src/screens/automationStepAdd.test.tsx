// @vitest-environment jsdom
// The dialog that puts a step in on a line, and the one that declares what a way out hands on
// (`AMB-T-5257`).
//
// What these guard: **a box on an automation runs a library action or carries a prompt written
// here**, and one that runs an action is not asked to declare what the action declares; **inside an
// action there is no library to pick from** (`AMB-D-949`) and the press goes through that picture's
// own door — the line it was opened from, or the action itself where there is no line yet
// (`AMB-T-5315`); **what the dialog took is what is sent**, ways out and inputs together; **nothing
// is sent until the dialog has what a step cannot be made without**; and, for the output artefact, **the name starts on
// the way out's own and stops following once somebody writes their own** — but only where that way
// out hands on nothing yet.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  insert: vi.fn(),
  insertInside: vi.fn(),
  add: vi.fn(),
  output: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => [{ id: 4, name: "Review", steps: 1, global: false, usedBy: 1 }],
  insertAutomationStep: hoisted.insert,
  insertAutomationActionStep: hoisted.insertInside,
  addAutomationStep: hoisted.add,
  addAutomationOutput: hoisted.output,
}));

import { t } from "../core/i18n";
import { AutomationOutputAdd } from "./AutomationOutputAdd";
import { AutomationStepAdd } from "./AutomationStepAdd";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let host: HTMLDivElement;

/** The dialogs draw into `document.body`, so what is looked at is the page and not the host. */
const boxes = () => [...document.body.querySelectorAll<HTMLInputElement>("input")];
const selects = () => [...document.body.querySelectorAll<HTMLSelectElement>("select")];
const buttons = () => [...document.body.querySelectorAll<HTMLButtonElement>("button")];
const button = (label: string) => buttons().find((b) => b.textContent === label)!;

/** Type into a controlled box the way React hears it — its own value tracker has to be moved. */
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
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  hoisted.insert.mockReset();
  hoisted.insertInside.mockReset();
  hoisted.add.mockReset();
  hoisted.output.mockReset();
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("putting a step in on a line", () => {
  async function open() {
    await act(async () => {
      root.render(
        createElement(AutomationStepAdd, {
          into: { picture: "automation", edgeId: 9 },
          projectId: 1,
          agent: "claude-code",
          onClose: () => undefined,
        }),
      );
    });
  }

  it("will not send a step with no name, nor one with no prompt to run on", async () => {
    await open();
    expect(button(t("auto.add.put")).disabled).toBe(true);
    await typeInto(boxes()[0]!, "実装する");
    expect(button(t("auto.add.put")).disabled).toBe(true);
    await typeInto(document.body.querySelector("textarea")!, "やる");
    expect(button(t("auto.add.put")).disabled).toBe(false);
  });

  it("sends the ways out and the inputs it took, with the line it was opened from", async () => {
    await open();
    await typeInto(boxes()[0]!, "実装する");
    await typeInto(document.body.querySelector("textarea")!, "やる");
    await act(async () => button(t("auto.add.exitAdd")).click());
    await typeInto(boxes()[1]!, "直すところがある");
    await act(async () => button(t("auto.add.inputAdd")).click());
    await typeInto(boxes()[2]!, "要件");
    await act(async () => button(t("auto.add.put")).click());

    expect(hoisted.insert).toHaveBeenCalledWith(9, {
      name: "実装する",
      source: { prompt: "やる" },
      agent: "claude-code",
      interactive: false,
      exits: ["直すところがある"],
      inputs: [{ name: "要件", kind: "value", required: true }],
    });
  });

  it("asks a step that runs a library action for nothing the action declares", async () => {
    await open();
    await typeInto(boxes()[0]!, "見直す");
    await pick(selects()[0]!, "4");
    expect(document.body.querySelector("textarea")).toBeNull();
    expect(buttons().some((b) => b.textContent === t("auto.add.exitAdd"))).toBe(false);
    expect(buttons().some((b) => b.textContent === t("auto.add.inputAdd"))).toBe(false);
    await act(async () => button(t("auto.add.put")).click());
    expect(hoisted.insert.mock.calls[0]![1]).toMatchObject({
      source: { action: 4 },
      exits: [],
      inputs: [],
    });
  });
});

describe("putting a step in inside an action", () => {
  async function open(into: { picture: "action"; edgeId: number } | { picture: "action"; actionId: number }) {
    await act(async () => {
      root.render(
        createElement(AutomationStepAdd, {
          into,
          projectId: 1,
          agent: "claude-code",
          onClose: () => undefined,
        }),
      );
    });
  }

  it("offers no library to pick from — an action places no actions", async () => {
    await open({ picture: "action", edgeId: 9 });
    expect(selects().some((one) => one.value === "" && one.options.length > 1)).toBe(false);
    expect(document.body.querySelector("textarea")).not.toBeNull();
  });

  it("sends a step to the line it was opened from", async () => {
    await open({ picture: "action", edgeId: 9 });
    await typeInto(boxes()[0]!, "直す");
    await typeInto(document.body.querySelector("textarea")!, "やる");
    await act(async () => button(t("auto.add.put")).click());
    expect(hoisted.insertInside).toHaveBeenCalledWith(9, {
      name: "直す",
      prompt: "やる",
      agent: "claude-code",
      interactive: false,
      exits: [],
      inputs: [],
    });
    expect(hoisted.insert).not.toHaveBeenCalled();
  });

  it("adds the first step where the picture has no line to press", async () => {
    await open({ picture: "action", actionId: 4 });
    await typeInto(boxes()[0]!, "取る");
    await typeInto(document.body.querySelector("textarea")!, "やる");
    await act(async () => button(t("auto.add.put")).click());
    expect(hoisted.add).toHaveBeenCalledWith(4, {
      name: "取る",
      prompt: "やる",
      agent: "claude-code",
      interactive: false,
      exits: [],
      inputs: [],
    });
  });
});

describe("declaring what a way out hands on", () => {
  async function open(outputs: { name: string; kind: "value"; required: boolean }[]) {
    await act(async () => {
      root.render(
        createElement(AutomationOutputAdd, {
          exit: { id: 3, name: "drafted", outputs },
          onClose: () => undefined,
        }),
      );
    });
  }

  it("starts the name on the way out's own where it hands on nothing yet", async () => {
    await open([]);
    expect(boxes()[0]!.value).toBe("drafted");
  });

  it("does not, where the way out already hands something on", async () => {
    await open([{ name: "draft", kind: "value", required: true }]);
    expect(boxes()[0]!.value).toBe("");
  });

  it("does not for the unnamed way out, which has no name to lend", async () => {
    await act(async () => {
      root.render(
        createElement(AutomationOutputAdd, {
          exit: { id: 3, outputs: [] },
          onClose: () => undefined,
        }),
      );
    });
    expect(boxes()[0]!.value).toBe("");
  });

  it("stops following the way out once somebody writes their own", async () => {
    await open([]);
    await typeInto(boxes()[0]!, "下書き");
    await pick(selects()[0]!, "file");
    await pick(selects()[1]!, "no");
    await act(async () => button(t("auto.out.add")).click());
    expect(hoisted.output).toHaveBeenCalledWith(3, {
      name: "下書き",
      kind: "file",
      required: false,
    });
  });
});
