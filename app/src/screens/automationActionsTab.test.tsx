// @vitest-environment jsdom
// The "actions" tab — the library, and the way one is made in it (`AMB-T-5329`). Only the read and
// the write doors are stubbed; the rows, the reach column, the counted sentence and what each press
// sends all run for real.
//
// What these guard: **both reaches are one list**, each row saying which library holds it, so a
// reader is never left to work out why an action they can see is not in the list they are looking
// at; **an action nobody runs says so in words** rather than counting to zero; and **a row opens the
// build screen on that action** (`AMB-T-5315`), which is where its steps and their prompts are.
//
// And on making one: **the way to make an action is there while the library is empty**, which is
// where it is most needed; **it sends a name and a reach and no prompt**, because a prompt belongs
// to a step; and **the screen opens on the row that was just made**, which is how the two halves are
// one act to the reader.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  actions: [] as AutomationActionCardDto[],
  add: vi.fn(async (_name: string, _project: number | null) => {}),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => hoisted.actions,
  addAutomationAction: hoisted.add,
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
    steps: 1,
    global: false,
    usedBy: 2,
    ...over,
  };
}

/** What a press on a row asked to open, in the order it was asked. */
let opened: number[] = [];

async function render() {
  await act(async () => {
    root.render(
      createElement(AutomationActionsTab, {
        projectId: 1,
        onOpen: (id: number) => opened.push(id),
      }),
    );
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
  opened = [];
  hoisted.add.mockReset();
  hoisted.add.mockImplementation(async () => {});
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

describe("opening one", () => {
  it("opens the build screen on the action the row names", async () => {
    hoisted.actions = [action({ id: 3, name: "Take one" })];
    await render();
    await act(async () => { button("Take one").click(); });
    expect(opened).toEqual([3]);
  });
});

describe("making one", () => {
  /** Open the form the way a reader does: press the one control the tab draws over the list. */
  async function openForm() {
    await render();
    await act(async () => { button(t("auto.actions.add")).click(); });
  }

  it("offers the way to make one while the library is empty", async () => {
    await render();
    expect(container.textContent).toContain(t("auto.actions.empty"));
    expect(button(t("auto.actions.add"))).not.toBeNull();
  });

  it("asks for a name and a reach, and for no prompt", async () => {
    await openForm();
    expect(container.querySelector("input")).not.toBeNull();
    expect(container.querySelector("select")).not.toBeNull();
    expect(container.querySelector("textarea")).toBeNull();
  });

  it("makes it in this project's library by default", async () => {
    await openForm();
    type(container.querySelector("input")!, "Review");
    await act(async () => { button(t("auto.actions.add")).click(); });
    expect(hoisted.add).toHaveBeenCalledWith("Review", 1);
  });

  it("makes it in the device's library when that reach is picked", async () => {
    await openForm();
    type(container.querySelector("input")!, "Review");
    const reach = container.querySelector("select")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(reach, "device");
      reach.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => { button(t("auto.actions.add")).click(); });
    expect(hoisted.add).toHaveBeenCalledWith("Review", null);
  });

  it("writes nothing while the name is blank", async () => {
    await openForm();
    expect(button(t("auto.actions.add")).disabled).toBe(true);
  });

  it("opens the build screen on the row that was just made", async () => {
    hoisted.actions = [action({ id: 3, name: "Take one" })];
    hoisted.add.mockImplementation(async (name: string) => {
      hoisted.actions = [...hoisted.actions, action({ id: 9, name })];
    });
    await render();
    await act(async () => { button(t("auto.actions.add")).click(); });
    type(container.querySelector("input")!, "Review");
    await act(async () => { button(t("auto.actions.add")).click(); });
    expect(opened).toEqual([9]);
  });
});
