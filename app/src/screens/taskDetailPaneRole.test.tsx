// @vitest-environment jsdom
// The role field of the detail pane: drawn only for a task the AI holds in a project that defines a
// role, offering the project's roles and "no role", and writing the choice at once.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskRoleDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  roles: [] as TaskRoleDto[],
  /** Every role write the pane asked for, as `taskId:roleId`. */
  asked: [] as string[],
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    fetchProjectRoles: async (): Promise<TaskRoleDto[]> => hoisted.roles,
    setTaskRole: async (id: number, roleId: number | null) => { hoisted.asked.push(`${id}:${roleId}`); },
  };
});

import { TaskDetailPane } from "./TaskDetailPane";
import { StoreProvider } from "../store/store";
import { loadSnapshot } from "../core/snapshot";
import { addTask, setAssignee } from "../core/mutations";
import { t } from "../core/i18n";
import type { Facet } from "../mock/types";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const role = (id: number, name: string): TaskRoleDto => ({ id, name, icon: null, automationId: 1, automationName: "開発" });
const roleField = () => container.querySelector<HTMLElement>(`[role="radiogroup"][aria-label="${t("detail.role")}"]`);
/** The field's choices, each as its value and whether it is checked. */
const choices = () =>
  [...roleField()!.querySelectorAll<HTMLInputElement>('input[type="radio"]')].map((i) => ({
    name: i.closest("label")!.textContent, value: i.value, checked: i.checked,
  }));

async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

/** A task of project 1, handed to `kind`, open in the pane. */
async function open(kind: Facet) {
  const id = (await addTask(1, "役割の欄"))!;
  await setAssignee(id, kind);
  act(() => root.render(createElement(StoreProvider, null, createElement(TaskDetailPane, { taskId: id }))));
  await settle();
  return id;
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.roles = [role(11, "実装係"), role(12, "レビュー係")];
  hoisted.asked = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("TaskDetailPane role field", () => {
  it("offers the project's roles and no role for a task the AI holds, and writes the choice", async () => {
    const id = await open("ai");
    expect(roleField()).not.toBeNull();
    expect(choices()).toEqual([
      { name: "実装係", value: "11", checked: false },
      { name: "レビュー係", value: "12", checked: false },
      { name: t("detail.roleNone"), value: "none", checked: true }, // A task with no role is one worked in a session
    ]);

    await act(async () => {
      roleField()!.querySelector<HTMLInputElement>('input[value="12"]')!.click();
    });
    expect(hoisted.asked).toEqual([`${id}:12`]);
  });

  it("is not drawn for a task a person holds", async () => {
    await open("human");
    expect(roleField()).toBeNull();
  });

  it("is not drawn in a project that defines no role", async () => {
    hoisted.roles = [];
    await open("ai");
    expect(roleField()).toBeNull();
  });
});
