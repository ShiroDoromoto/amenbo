// @vitest-environment jsdom
// What keeps a waiting run waiting, on its pane (`AMB-D-999`). What these guard: the head says how many
// tasks the wait cannot take; every thing holding them back is a line with its count; a value, a task
// and a decision are presses to where each is cleared, and a start day and a creation are only said.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { HeldBack } from "./BuiltinCard";
import { RefNavProvider, type RefNav } from "../core/refNav";
import { t, tf } from "../core/i18n";
import type { AutomationHeldBackDto } from "../bindings/bindings";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const held: AutomationHeldBackDto = {
  project: 3,
  tasks: 30,
  values: [{ dimensionId: 7, axis: "リリース", value: "入力と一覧", count: 28 }],
  blockers: [{ id: 321, title: "先行", count: 2 }],
  decisions: [{ id: 990, title: "根拠", count: 1 }],
  notStarted: 1,
  firstStart: "2026-10-01",
  drafts: 1,
};

function render(nav: RefNav) {
  act(() => root.render(createElement(RefNavProvider, { value: nav, children: createElement(HeldBack, { held }) })));
}

const press = (text: string) =>
  [...container.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.includes(text));

describe("HeldBack", () => {
  it("says how many tasks it cannot take, and one line per thing stopping them with its count", () => {
    render({});
    expect(container.textContent).toContain(tf("auto.run.heldBack", { count: 30 }));
    const lines = [...container.querySelectorAll("li")].map((li) => li.textContent);
    expect(lines).toHaveLength(5);
    expect(lines[0]).toContain("入力と一覧 (リリース)");
    expect(lines[0]).toContain("28");
    expect(lines[1]).toContain("AMB-T-321 先行");
    expect(lines[2]).toContain("根拠");
    expect(lines[3]).toContain("2026-10-01");
    expect(lines[4]).toContain(t("chip.draft"));
  });

  it("takes a value to its axis on the run's project, and a task or a decision to itself", () => {
    const went: string[] = [];
    render({
      openDimension: (project, dimension) => went.push(`axis ${project}:${dimension}`),
      selectTask: (id) => went.push(`task ${id}`),
      selectDecision: (id) => went.push(`decision ${id}`),
    });
    act(() => press("入力と一覧")!.click());
    act(() => press("先行")!.click());
    act(() => press("根拠")!.click());
    expect(went).toEqual(["axis 3:7", "task 321", "decision 990"]);
    // Nothing but time or the task's own pane clears these two, so they are not presses.
    expect(container.querySelectorAll("button")).toHaveLength(3);
  });

  it("holds the presses down where there is nowhere to go", () => {
    render({});
    expect([...container.querySelectorAll("button")].every((b) => b.disabled)).toBe(true);
  });
});
