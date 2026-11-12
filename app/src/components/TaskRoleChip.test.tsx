// @vitest-environment jsdom
// The role on a list row and a board card: the role's picture and name, cut with an ellipsis and whole
// in the tooltip, shown even when its automation is archived, and nothing at all for a task with no
// role or a role the project's answer does not hold.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskRoleDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  roles: new Map<number, TaskRoleDto[]>(),
  asked: [] as number[],
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    fetchProjectRoles: async (projectId: number): Promise<TaskRoleDto[]> => {
      hoisted.asked.push(projectId);
      return hoisted.roles.get(projectId) ?? [];
    },
  };
});

import { TaskRoleChip } from "./TaskRoleChip";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const role = (id: number, name: string, automationArchived = false): TaskRoleDto => ({
  id, name, icon: null, automationId: id, automationName: "開発", automationArchived,
});

async function render(...chips: { projectId: number | null; roleId: number | null }[]) {
  await act(async () => { root.render(chips.map((c, i) => createElement(TaskRoleChip, { ...c, key: i }))); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}
const shown = () => [...container.querySelectorAll<HTMLElement>(".taskrole")].map((e) => ({
  name: e.querySelector(".taskrole__name")?.textContent, tip: e.title, icon: e.querySelector("svg, canvas, img") !== null,
}));

beforeEach(() => {
  hoisted.roles.clear();
  hoisted.asked.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("TaskRoleChip", () => {
  it("shows the role's picture and name, with the whole name in the tooltip and no automation name", async () => {
    hoisted.roles.set(101, [role(1, "レビュー担当"), role(2, "実装担当")]);
    await render({ projectId: 101, roleId: 2 });
    expect(shown()).toEqual([{ name: "実装担当", tip: "実装担当", icon: true }]);
    expect(container.textContent).not.toContain("開発");
  });

  it("shows a role whose automation is archived", async () => {
    hoisted.roles.set(102, [role(3, "古い役割", true)]);
    await render({ projectId: 102, roleId: 3 });
    expect(shown().map((s) => s.name)).toEqual(["古い役割"]);
  });

  it("reads each project's roles for a list that mixes projects", async () => {
    hoisted.roles.set(103, [role(4, "設計")]);
    hoisted.roles.set(104, [role(5, "検証")]);
    await render({ projectId: 103, roleId: 4 }, { projectId: 104, roleId: 5 }, { projectId: 103, roleId: 4 });
    expect(shown().map((s) => s.name)).toEqual(["設計", "検証", "設計"]);
    expect([...new Set(hoisted.asked)].sort()).toEqual([103, 104]);
  });

  it("draws nothing for a task with no role, and reads nothing for it", async () => {
    hoisted.roles.set(105, [role(6, "設計")]);
    await render({ projectId: 105, roleId: null });
    expect(container.innerHTML).toBe("");
    expect(hoisted.asked).toEqual([]);
  });

  it("draws nothing for a role the project's answer does not hold", async () => {
    hoisted.roles.set(106, [role(7, "設計")]);
    await render({ projectId: 106, roleId: 99 });
    expect(container.innerHTML).toBe("");
  });
});
