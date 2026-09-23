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
  scope: vi.fn(async (_id: number, _project: number | null) => {}),
}));

vi.mock("../core/automations", () => ({
  useAutomationActions: () => hoisted.actions,
  addAutomationAction: hoisted.add,
  setAutomationActionScope: hoisted.scope,
}));
vi.mock("../mock/adapter", () => ({
  dataAdapter: { listProjects: () => [{ id: 1, name: "amenbo" }, { id: 2, name: "site" }] },
}));

import { t, tf } from "../core/i18n";
import { AutomationActionsTab } from "./AutomationActionsTab";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function action(over: Partial<AutomationActionCardDto> = {}): AutomationActionCardDto {
  return {
    id: 3,
    name: "Take one",
    note: "",
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

/** The name box of the make form — the list's search box stands beside it. */
const nameBox = () => container.querySelector<HTMLInputElement>(".actlib__make input")!;

/** Pick a reach in the make form, which starts with none picked. */
async function pickReach(value: "project" | "global") {
  const reach = container.querySelector<HTMLSelectElement>(".actlib__make select")!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(reach, value);
    reach.dispatchEvent(new Event("change", { bubbles: true }));
  });
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
    expect(rows()[0]).toContain(t("auto.actions.reachGlobal"));
    expect(rows()[1]).toContain(t("auto.actions.reachProject"));
  });

  it("says how many automations run each action", async () => {
    hoisted.actions = [action({ usedBy: 4 })];
    await render();
    expect(rows()[0]).toContain(tf("auto.actions.usedN", { n: 4 }));
  });

  it("carries the first line of what an action is for, and nothing where none is written", async () => {
    hoisted.actions = [
      action({ id: 1, name: "Report", note: "\nSays what the run did\nand where it stopped" }),
      action({ id: 2, name: "Take one", note: "" }),
    ];
    await render();
    const notes = [...container.querySelectorAll(".auto__row")].map(
      (one) => one.querySelector(".auto__note")?.textContent ?? null,
    );
    expect(notes).toEqual(["Says what the run did", null]);
  });

  it("says in words that nobody runs one, rather than counting to zero", async () => {
    hoisted.actions = [action({ usedBy: 0 })];
    await render();
    expect(rows()[0]).toContain(t("auto.actions.usedNone"));
    expect(rows()[0]).not.toContain(tf("auto.actions.usedN", { n: 0 }));
  });
});

describe("narrowing it", () => {
  it("finds an action by what its name and its note say", async () => {
    hoisted.actions = [
      action({ id: 1, name: "Report", note: "Says what the run did" }),
      action({ id: 2, name: "Take one", note: "" }),
    ];
    await render();
    await act(async () => { type(container.querySelector<HTMLInputElement>(".actlib__search")!, "run did"); });
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toContain("Report");
  });

  it("narrows to one reach, and says so when nothing is left", async () => {
    hoisted.actions = [action({ id: 1, name: "Report", global: true })];
    await render();
    await act(async () => { button(t("auto.actions.reachProject")).click(); });
    expect(rows()).toHaveLength(0);
    expect(container.textContent).toContain(t("auto.actions.noMatch"));
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
    expect(container.querySelector(".actlib__make input")).not.toBeNull();
    expect(container.querySelector("select")).not.toBeNull();
    expect(container.querySelector("textarea")).toBeNull();
  });

  it("makes nothing until a reach is picked", async () => {
    await openForm();
    type(nameBox(), "Review");
    expect(button(t("auto.actions.add")).disabled).toBe(true);
  });

  it("makes it in this project's library when that reach is picked", async () => {
    await openForm();
    type(nameBox(), "Review");
    await pickReach("project");
    await act(async () => { button(t("auto.actions.add")).click(); });
    expect(hoisted.add).toHaveBeenCalledWith("Review", 1);
  });

  it("makes it in the device's library when that reach is picked", async () => {
    await openForm();
    type(nameBox(), "Review");
    await pickReach("global");
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
    type(nameBox(), "Review");
    await pickReach("project");
    await act(async () => { button(t("auto.actions.add")).click(); });
    expect(opened).toEqual([9]);
  });
});

// Opened from the sidebar there is no project (`AMB-D-954`): the list is the device's library alone,
// so there is no reach to narrow to, and what is made is global without asking.
describe("the library opened from the sidebar", () => {
  async function renderDevice() {
    await act(async () => {
      root.render(
        createElement(AutomationActionsTab, { projectId: null, onOpen: (id: number) => opened.push(id) }),
      );
    });
  }

  it("offers no reach to narrow to", async () => {
    hoisted.actions = [action({ global: true })];
    await renderDevice();
    expect(container.querySelector(".actchip")).toBeNull();
  });

  it("makes it in the device's library, the one reach there is", async () => {
    await renderDevice();
    await act(async () => { button(t("auto.actions.add")).click(); });
    const options = [...container.querySelectorAll(".actlib__make option")].map((one) => one.getAttribute("value"));
    expect(options).toEqual(["global"]);
    type(nameBox(), "Review");
    await act(async () => { button(t("auto.actions.add")).click(); });
    expect(hoisted.add).toHaveBeenCalledWith("Review", null);
  });
});

// Moving a reach from the row, on the entrance that owns it now (`AMB-D-954`): a project moves its own
// to the device's library in one press; the sidebar asks which project for a global one; a refusal
// stays under the row in core's words.
describe("moving an action's reach", () => {
  beforeEach(() => {
    hoisted.scope.mockReset();
    hoisted.scope.mockResolvedValue(undefined);
  });

  async function renderAt(projectId: number | null) {
    await act(async () => {
      root.render(createElement(AutomationActionsTab, { projectId, onOpen: (id: number) => opened.push(id) }));
    });
  }

  it("moves a project's own action to the device's library from that project", async () => {
    hoisted.actions = [action({ id: 3, global: false }), action({ id: 5, name: "Shared", global: true })];
    await renderAt(1);
    const moves = [...container.querySelectorAll(".actlib__moveslot button")];
    expect(moves).toHaveLength(1);
    await act(async () => { button(t("auto.actions.toGlobal")).click(); });
    expect(hoisted.scope).toHaveBeenCalledWith(3, null);
    expect(opened).toEqual([]);
  });

  it("asks which project before moving a global action from the sidebar", async () => {
    hoisted.actions = [action({ id: 5, name: "Shared", global: true })];
    await renderAt(null);
    await act(async () => { button(t("auto.actions.toProject")).click(); });
    const move = () => [...container.querySelectorAll<HTMLButtonElement>(".actlib__moveplace button")]
      .find((b) => b.textContent === t("auto.actions.move"))!;
    expect(move().disabled).toBe(true);
    const pick = container.querySelector<HTMLSelectElement>(".actlib__moveplace select")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(pick, "2");
      pick.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await act(async () => { move().click(); });
    expect(hoisted.scope).toHaveBeenCalledWith(5, 2);
  });

  it("keeps core's refusal under the row", async () => {
    hoisted.actions = [action({ id: 3, global: false })];
    hoisted.scope.mockRejectedValue("placed by Nightly (7) in site");
    await renderAt(1);
    await act(async () => { button(t("auto.actions.toGlobal")).click(); });
    expect(container.querySelector(".actlib__moveplace")?.textContent).toContain("Nightly");
  });
});
