// @vitest-environment jsdom
// The roles of an automation on its build screen: the list down the left, the head of the picked
// one, and its default agent and model. The write doors are stubbed; every row, field and press runs
// for real.
//
// What these guard: **each row counts the steps its role's round reaches**, over every placement it
// is on; **the picked row is marked and a press picks another**; **"+ Add a role" writes one named
// after where it goes** and says so once it is written; **the head renames, moves and removes the
// role**, a blank name going back to what it was; and **the default agent and model are chosen the way
// a step's are** (`AMB-D-960`) — the empty choice clears each, and with no agent there is no model to
// pick.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationDetailDto, AutomationPlacementDto, AutomationRoleDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  add: vi.fn(),
  edit: vi.fn(),
  move: vi.fn(),
  remove: vi.fn(),
}));

vi.mock("../core/automations", () => ({
  addAutomationRole: hoisted.add,
  editAutomationRole: hoisted.edit,
  moveAutomationRole: hoisted.move,
  removeAutomationRole: hoisted.remove,
}));
vi.mock("../core/dialog", () => ({ confirmDialog: () => Promise.resolve(true) }));
vi.mock("../core/boundFolders", () => ({
  useBoundFolders: () => ({ all: [], live: [], answered: true }),
}));
// `inTauri` is false here, so the machine is not asked which agents and models it has: the pulldowns
// hold the role's own values and nothing else.
vi.mock("../core/ipc", () => ({ invoke: () => Promise.resolve(null) }));

import { t, tf } from "../core/i18n";
import { AutomationRoleHead, AutomationRoleList, AutomationRoleWho, roleStepCount } from "./AutomationRoles";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function placement(id: number, roleId: number | undefined, steps: number): AutomationPlacementDto {
  return {
    id,
    name: `Action ${id}`,
    actionId: 100 + id,
    global: false,
    roleId,
    prompt: "",
    interactive: false,
    reportToTask: false,
    showHistory: true,
    showNotes: true,
    showDecisions: true,
    showComments: true,
    exits: [],
    inputs: [],
    settings: [],
    steps: Array.from({ length: steps }, (_, n) => ({ stepId: id * 10 + n, name: `step ${n}`, interactive: false })),
  };
}

const alice: AutomationRoleDto = { id: 1, name: "Alice", headcount: 1, agent: "claude-code", model: "opus", passes: [] };
const bob: AutomationRoleDto = { id: 2, name: "Bob", headcount: 1, passes: [] };

function detail(over: Partial<AutomationDetailDto> = {}): AutomationDetailDto {
  return {
    id: 7,
    projectId: 1,
    name: "Morning round",
    notes: "",
    archived: false,
    triggers: [],
    roles: [alice, bob],
    placements: [placement(1, 1, 2), placement(2, 1, 3), placement(3, 2, 1), placement(4, undefined, 4)],
    edges: [],
    wires: [],
    maxHeadcount: 10,
    heldBy: [],
    unsaved: false,
    saveBlocks: [],
    ...over,
  };
}

async function draw(node: ReturnType<typeof createElement>) {
  await act(async () => root.render(node));
}

const buttonNamed = (words: string) =>
  [...container.querySelectorAll<HTMLButtonElement>("button")].find((one) => one.textContent === words)!;

async function press(button: HTMLButtonElement) {
  await act(async () => button.click());
}

async function pick(control: HTMLSelectElement, value: string) {
  await act(async () => {
    control.value = value;
    control.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  for (const one of [hoisted.add, hoisted.edit, hoisted.move, hoisted.remove]) {
    one.mockReset();
    one.mockResolvedValue(undefined);
  }
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the role list", () => {
  it("counts the steps each role's round reaches", () => {
    expect(roleStepCount(detail(), 1)).toBe(5);
    expect(roleStepCount(detail(), 2)).toBe(1);
    expect(roleStepCount(detail(), 3)).toBe(0);
  });

  it("draws a row per role with its count, marks the picked one, and picks another", async () => {
    const picked = vi.fn();
    await draw(createElement(AutomationRoleList, { automation: detail(), pickedId: 2, onPick: picked, onAdded: () => undefined }));
    const rows = [...container.querySelectorAll<HTMLButtonElement>(".autoroles__row")];
    expect(rows.map((one) => one.querySelector(".autoroles__name")!.textContent)).toEqual(["Alice", "Bob"]);
    expect(rows.map((one) => one.querySelector(".autoroles__count")!.textContent)).toEqual(["5", "1"]);
    expect(rows[1].getAttribute("aria-current")).toBe("true");
    expect(rows[0].hasAttribute("aria-current")).toBe(false);
    await press(rows[0]);
    expect(picked).toHaveBeenCalledWith(1);
  });

  it("adds a role after the ones there and says how many there were", async () => {
    const added = vi.fn();
    await draw(createElement(AutomationRoleList, { automation: detail(), pickedId: 1, onPick: () => undefined, onAdded: added }));
    await press(buttonNamed(t("auto.build.roleAdd")));
    expect(hoisted.add).toHaveBeenCalledWith(7, tf("auto.build.roleNew", { n: 3 }));
    expect(added).toHaveBeenCalledWith(2);
  });

  it("puts a refused add on the column", async () => {
    hoisted.add.mockRejectedValue("no room");
    const added = vi.fn();
    await draw(createElement(AutomationRoleList, { automation: detail(), pickedId: 1, onPick: () => undefined, onAdded: added }));
    await press(buttonNamed(t("auto.build.roleAdd")));
    expect(container.querySelector("[role=alert]")!.textContent).toContain("no room");
    expect(added).not.toHaveBeenCalled();
  });
});

describe("the picked role's head", () => {
  const head = (role: AutomationRoleDto, onRemoved = () => undefined) =>
    draw(createElement(AutomationRoleHead, { automation: detail(), role, onRemoved }));
  const nameBox = () => container.querySelector<HTMLInputElement>(".autorole__name")!;

  async function typeName(value: string) {
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      setter.call(nameBox(), value);
      nameBox().dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => {
      nameBox().dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
  }

  it("renames the role, and a blank name goes back to the one it had", async () => {
    await head(alice);
    expect(nameBox().value).toBe("Alice");
    await typeName("Carol");
    expect(hoisted.edit).toHaveBeenCalledWith(1, { name: "Carol" });
    hoisted.edit.mockClear();
    await typeName("  ");
    expect(hoisted.edit).not.toHaveBeenCalled();
    expect(nameBox().value).toBe("Alice");
  });

  it("moves the role past its neighbours, and not past either end", async () => {
    await head(alice);
    expect(buttonNamed(t("auto.build.roleUp")).disabled).toBe(true);
    await press(buttonNamed(t("auto.build.roleDown")));
    expect(hoisted.move).toHaveBeenCalledWith(1, "after", 2);
    await head(bob);
    expect(buttonNamed(t("auto.build.roleDown")).disabled).toBe(true);
    await press(buttonNamed(t("auto.build.roleUp")));
    expect(hoisted.move).toHaveBeenCalledWith(2, "before", 1);
  });

  it("redraws the icon by writing a new seed, a different one each press", async () => {
    await head({ ...alice, icon: "old-seed" });
    await press(buttonNamed(t("auto.build.roleIconRedo")));
    await press(buttonNamed(t("auto.build.roleIconRedo")));
    expect(hoisted.edit).toHaveBeenCalledTimes(2);
    const seeds = hoisted.edit.mock.calls.map(([id, patch]) => {
      expect(id).toBe(1);
      expect(Object.keys(patch as object)).toEqual(["icon"]);
      return (patch as { icon: string }).icon;
    });
    for (const seed of seeds) {
      expect(seed).not.toBe("");
      expect(seed).not.toBe("old-seed");
    }
    expect(seeds[0]).not.toBe(seeds[1]);
  });

  it("puts a refused redraw on the head", async () => {
    hoisted.edit.mockRejectedValue("no room");
    await head(alice);
    await press(buttonNamed(t("auto.build.roleIconRedo")));
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("no room");
  });

  it("removes the role and hands the screen back", async () => {
    const removed = vi.fn();
    await head(bob, removed);
    await press(buttonNamed(t("auto.build.roleRemove")));
    expect(hoisted.remove).toHaveBeenCalledWith(2);
    expect(removed).toHaveBeenCalled();
  });
});

describe("the picked role's agent and model", () => {
  const who = (role: AutomationRoleDto) => draw(createElement(AutomationRoleWho, { automation: detail(), role }));
  const agentOf = (id: number) => container.querySelector<HTMLSelectElement>(`#role-agent-${id}`)!;
  const modelOf = (id: number) => container.querySelector<HTMLSelectElement>(`#role-model-${id}`)!;

  it("chooses the default agent and model the way a step's are", async () => {
    await who(alice);
    expect(container.textContent).toContain(t("auto.build.roleWho"));
    expect(container.textContent).toContain(t("auto.build.roleAgent"));
    expect(agentOf(1).value).toBe("claude-code");
    expect(modelOf(1).value).toBe("opus");
    await pick(modelOf(1), "");
    expect(hoisted.edit).toHaveBeenCalledWith(1, { model: null });
    await pick(agentOf(1), "");
    expect(hoisted.edit).toHaveBeenCalledWith(1, { agent: null });
  });

  it("offers no model while no agent is chosen", async () => {
    await who(bob);
    expect(agentOf(2).value).toBe("");
    expect(modelOf(2).disabled).toBe(true);
  });
});
