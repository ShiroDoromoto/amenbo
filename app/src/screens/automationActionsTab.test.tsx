// @vitest-environment jsdom
// The "actions" tab — the library, and the one box a prompt is written in (`AMB-T-5258`). Only the
// read and the write door are stubbed; the rows, the reach column, the counted sentence and what
// Save sends all run for real.
//
// What these guard: **both reaches are one list**, each row saying which library holds it, so a
// reader is never left to work out why an action they can see is not in the list they are looking
// at; **an action nobody runs says so in words** rather than counting to zero; **the box is opened in
// place of the row** and says what a rewrite reaches before Save is within reach; and **Save sends
// the name and the prompt together through the one door**, which is the whole of what makes this the
// only place a prompt is written.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  actions: [] as AutomationActionCardDto[],
  edit: vi.fn(async (_id: number, _patch: { name?: string; prompt?: string }) => {}),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => hoisted.actions,
  editAutomationAction: hoisted.edit,
}));

import { t, tn } from "../core/i18n";
import { AutomationActionsTab } from "./AutomationActionsTab";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function action(over: Partial<AutomationActionCardDto> = {}): AutomationActionCardDto {
  return {
    id: 3,
    name: "Take one",
    prompt: "Take the next task.",
    entryStepId: 11,
    steps: 1,
    global: false,
    usedBy: 2,
    ...over,
  };
}

async function render() {
  await act(async () => {
    root.render(createElement(AutomationActionsTab, { projectId: 1 }));
  });
}

const rows = () => [...container.querySelectorAll(".auto__row")].map((one) => one.textContent ?? "");

function button(label: string): HTMLButtonElement {
  const found = [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(label));
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}

/** Typing into a controlled field: React listens for `input`, not for the value being assigned. */
function type(field: HTMLInputElement | HTMLTextAreaElement, text: string) {
  const proto = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
  Object.getOwnPropertyDescriptor(proto.prototype, "value")!.set!.call(field, text);
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  hoisted.actions = [];
  hoisted.edit.mockClear();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the library", () => {
  it("says an empty library is empty", async () => {
    await render();
    expect(container.textContent).toContain(t("auto.actions.empty"));
  });

  it("draws both reaches as one list, each row saying which one holds it", async () => {
    hoisted.actions = [
      action({ id: 1, name: "Report", global: true }),
      action({ id: 2, name: "Take one", global: false }),
    ];
    await render();
    expect(rows()[0]).toContain(t("auto.actions.reachDevice"));
    expect(rows()[1]).toContain(t("auto.actions.reachProject"));
  });

  it("says how many automations run each action", async () => {
    hoisted.actions = [action({ usedBy: 4 })];
    await render();
    expect(rows()[0]).toContain(tn("auto.actions.usedBy", 4));
  });

  it("says in words that nobody runs one, rather than counting to zero", async () => {
    hoisted.actions = [action({ usedBy: 0 })];
    await render();
    expect(rows()[0]).toContain(t("auto.actions.unused"));
    expect(rows()[0]).not.toContain(tn("auto.actions.usedBy", 0));
  });
});

describe("writing a prompt", () => {
  async function open() {
    hoisted.actions = [action()];
    await render();
    await act(async () => { button("Take one").click(); });
  }

  it("opens the box in place of the row, on what the action holds now", async () => {
    await open();
    expect(container.querySelector(".auto__row")).toBeNull();
    expect(container.querySelector("input")!.value).toBe("Take one");
    expect(container.querySelector("textarea")!.value).toBe("Take the next task.");
  });

  it("says what a rewrite reaches, where the rewrite is happening", async () => {
    await open();
    expect(container.textContent).toContain(t("auto.actions.reaches"));
  });

  it("sends the name and the prompt together, and closes the box", async () => {
    await open();
    type(container.querySelector("input")!, "Take the next one");
    type(container.querySelector("textarea")!, "Take the next ready task.");
    await act(async () => { button(t("auto.actions.save")).click(); });
    expect(hoisted.edit).toHaveBeenCalledWith(3, {
      name: "Take the next one",
      prompt: "Take the next ready task.",
    });
    expect(container.querySelector(".auto__row")).not.toBeNull();
  });

  it("writes nothing when the box is closed by Cancel", async () => {
    await open();
    type(container.querySelector("textarea")!, "Something else entirely.");
    await act(async () => { button(t("auto.actions.cancel")).click(); });
    expect(hoisted.edit).not.toHaveBeenCalled();
    expect(container.querySelector(".auto__row")).not.toBeNull();
  });
});
