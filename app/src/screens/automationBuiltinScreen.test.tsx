// @vitest-environment jsdom
// One built-in, opened (`AMB-D-964`).
//
// What these guard: **it draws the definition** — what it does and in what order, its settings, its
// inputs, its ways out with what each hands on and the error way out, and when it leaves by that one —
// **and says it is read, not changed**, with no press on it that writes;
// and **an action build screen opened on a built-in's action lands here** rather than on steps nobody
// can edit.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationActionDetailDto, AutomationBuiltinDto } from "../bindings/bindings";

const take: AutomationBuiltinDto = {
  key: "task_take",
  name: "Take a task",
  does: "reserves the first task the filter finds",
  steps: ["looks for tasks", "reserves the first"],
  halts: ["the store refuses"],
  settings: [{ name: "filter", kind: "taskfilter", required: true }],
  inputs: [{ name: "hint", kind: "value", required: false }],
  exits: [{ name: "taken", outputs: [{ name: "task", kind: "task_take", required: true }] }, { name: "完了", outputs: [] }],
  usedBy: 2,
};

const bare: AutomationBuiltinDto = { ...take, steps: [], halts: [], settings: [], inputs: [], exits: [] };
/** Whether the built-in is drawn with nothing on any of its rows. */
let empty = false;

const hoisted = vi.hoisted(() => ({ action: null as AutomationActionDetailDto | null }));

vi.mock("../core/automations", () => ({
  useAutomationBuiltins: () => [empty ? bare : take],
  useAutomationAction: () => hoisted.action,
  editAutomationAction: vi.fn(),
}));

import { t, tn } from "../core/i18n";
import { AutomationActionBuildScreen } from "./AutomationActionBuildScreen";
import { AutomationBuiltinScreen } from "./AutomationBuiltinScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const back = vi.fn();

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  back.mockClear();
  empty = false;
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("a built-in, opened", () => {
  it("draws what it does, reads, takes and leaves by", async () => {
    await act(async () => {
      root.render(createElement(AutomationBuiltinScreen, { builtinKey: "task_take", onBack: back }));
    });
    const text = container.textContent ?? "";
    expect(text).toContain("Take a task");
    expect(text).toContain("reserves the first task the filter finds");
    expect(text).toContain(t("auto.actions.reachBuiltin"));
    expect(text).toContain(tn("auto.actions.usedBy", 2));
    for (const word of ["filter", "hint", "taken", "task", "完了"]) {
      expect(text).toContain(word);
    }
  });

  it("draws the steps it takes in order, and when it leaves by the error way out", async () => {
    await act(async () => {
      root.render(createElement(AutomationBuiltinScreen, { builtinKey: "task_take", onBack: back }));
    });
    const lists = [...container.querySelectorAll(".actdecl__list")];
    expect(lists.map((one) => [...one.querySelectorAll("li")].map((li) => li.textContent))).toEqual([
      ["looks for tasks", "reserves the first"],
      ["the store refuses"],
    ]);
    expect(lists[0]!.tagName).toBe("OL");
    const text = container.textContent ?? "";
    expect(text).toContain(t("auto.builtin.steps"));
    expect(text).toContain(t("auto.builtin.halts"));
    expect(text).toContain(t("auto.builtin.haltsThen"));
    expect(container.querySelector(".actport--error")?.textContent).toBe(t("auto.pic.errorExit"));
  });

  it("says it is read, and offers nothing but the way back", async () => {
    await act(async () => {
      root.render(createElement(AutomationBuiltinScreen, { builtinKey: "task_take", onBack: back }));
    });
    expect(container.querySelector('[data-icon="lock"]')).not.toBeNull();
    const buttons = [...container.querySelectorAll("button")];
    expect(buttons).toHaveLength(1);
    expect(container.querySelector("input, textarea, select")).toBeNull();
    await act(async () => { buttons[0]!.click(); });
    expect(back).toHaveBeenCalledTimes(1);
  });

  it("goes back by a label that names no screen, since it is opened from more than one", async () => {
    await act(async () => {
      root.render(createElement(AutomationBuiltinScreen, { builtinKey: "task_take", onBack: back }));
    });
    expect(container.querySelector("button")?.textContent?.trim()).toBe(t("auto.builtin.back"));
  });

  it("draws a dash on every row with nothing in it, the error way out with no other, and no line after no halts", async () => {
    empty = true;
    await act(async () => {
      root.render(createElement(AutomationBuiltinScreen, { builtinKey: "task_take", onBack: back }));
    });
    const none = [...container.querySelectorAll(".actdecl__none")].map((one) => one.textContent);
    expect(none).toEqual(["—", "—", "—", "—"]);
    expect(container.textContent).not.toContain(t("auto.builtin.haltsThen"));
    expect(container.querySelectorAll(".actport--error")).toHaveLength(1);
  });

  it("is where the build screen of a built-in's action lands", async () => {
    hoisted.action = {
      id: 40,
      name: "Take a task",
      note: "reserves the first task the filter finds",
      global: true,
      builtin: "task_take",
      usedBy: 2,
      steps: [],
      edges: [],
      wires: [],
      exits: [],
      inputs: [],
      settings: [],
      heldBy: [],
      placedOn: [],
    };
    await act(async () => {
      root.render(createElement(AutomationActionBuildScreen, { id: 40, onBack: back }));
    });
    expect(container.querySelector('[data-icon="lock"]')).not.toBeNull();
    expect(container.querySelector(".actbuild__canvas")).toBeNull();
  });
});
