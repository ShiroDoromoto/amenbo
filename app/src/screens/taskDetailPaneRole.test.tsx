// @vitest-environment jsdom
// The role field of the detail pane: drawn only for a task the AI holds in a project that defines a
// role, offering the project's roles and "no role", and writing the choice at once. A task handed to the
// AI from a person waits on the choice: nothing is checked and nothing written, and reaching for
// anything else meanwhile says why under the field.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskRoleDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  roles: [] as TaskRoleDto[],
  /** Every role write the pane asked for, as `taskId:roleId`. */
  asked: [] as string[],
  /** Every assignee write asked for, as `taskId:kind`. */
  assigned: [] as string[],
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    fetchProjectRoles: async (): Promise<TaskRoleDto[]> => hoisted.roles,
    setTaskRole: async (id: number, roleId: number | null) => { hoisted.asked.push(`${id}:${roleId}`); },
    setAssignee: async (id: number, kind: Facet | null) => {
      hoisted.assigned.push(`${id}:${kind}`);
      await orig.setAssignee(id, kind);
    },
  };
});

import { TaskDetailPane } from "./TaskDetailPane";
import { StoreProvider } from "../store/store";
import { getSnapshot, loadSnapshot } from "../core/snapshot";
import { addTask, setAssignee } from "../core/mutations";
import { t, tf } from "../core/i18n";
import type { Facet } from "../mock/types";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const role = (id: number, name: string): TaskRoleDto => ({ id, name, icon: null, automationId: 1, automationName: "開発", automationArchived: false });
const roleField = () => container.querySelector<HTMLElement>(`[role="radiogroup"][aria-label="${t("detail.role")}"]`);
/** The field's choices, each as its value and whether it is checked. */
const choices = () =>
  [...roleField()!.querySelectorAll<HTMLInputElement>('input[type="radio"]')].map((i) => ({
    name: i.closest("label")!.textContent, value: i.value, checked: i.checked,
  }));

const reason = () => [...container.querySelectorAll('[role="alert"]')].find((e) => e.textContent === t("detail.roleRequired")) ?? null;

/** Hand the open task to `kind` through the assignee picker. */
async function pickAssignee(kind: Facet) {
  await act(async () => { container.querySelector<HTMLButtonElement>(".apick__trigger")!.click(); });
  const at = getSnapshot().roster.findIndex((a) => a.kind === kind);
  await act(async () => { container.querySelectorAll<HTMLButtonElement>(".apick__opt")[at + 1]!.click(); });
  await settle();
}

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
  hoisted.assigned = [];
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

  // The one a task already has stays offered (`taskRoleChoices`); this task has none.
  it("does not offer an archived automation's roles to a task without one", async () => {
    hoisted.roles = [role(11, "実装係"), { ...role(31, "旧係"), automationId: 2, automationName: "旧開発", automationArchived: true }];
    await open("ai");
    expect(choices().map((c) => c.value)).toEqual(["11", "none"]);
    expect(container.querySelector(".rolepart__hint")!.textContent).toBe(tf("detail.roleFrom", { name: "開発" }));
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

  it("heads the field with the required mark and the automation the roles come from", async () => {
    await open("ai");
    expect(container.querySelector(".rolepart__title")!.textContent).toBe(t("detail.role"));
    expect(container.querySelector(".rolepart__req")!.textContent).toBe(t("detail.roleRequiredMark"));
    expect(container.querySelector(".rolepart__hint")!.textContent).toBe(tf("detail.roleFrom", { name: "開発" }));
  });

  it("handing a person's task to the AI leaves every choice unchecked and writes nothing yet", async () => {
    const id = await open("human");
    hoisted.assigned = [];
    await pickAssignee("ai");
    expect(choices().map((c) => c.checked)).toEqual([false, false, false]);
    expect(reason()).toBeNull();
    expect(hoisted.assigned).toEqual([]);
    expect(getSnapshot().tasks.find((x) => x.id === id)!.assignee?.kind).toBe("human");
  });

  it("says why under the field when anything else is reached for before a role is chosen", async () => {
    await open("human");
    await pickAssignee("ai");
    await act(async () => { container.querySelector<HTMLSelectElement>(`select[title="${t("detail.priority")}"]`)!.focus(); });
    expect(reason()).not.toBeNull();
    expect(reason()!.parentElement).toBe(container.querySelector(".rolepart"));
  });

  it("holds the reason's line unseen while the hand-over waits, so saying why moves nothing below it", async () => {
    await open("human");
    await pickAssignee("ai");
    const part = container.querySelector(".rolepart")!;
    const held = part.querySelector(".rolepart__held");
    expect(held).not.toBeNull();
    expect(held!.getAttribute("aria-hidden")).toBe("true");
    expect(held!.textContent).toBe(t("detail.roleRequired"));
    const at = [...part.children].indexOf(held!);
    await act(async () => { container.querySelector<HTMLSelectElement>(`select[title="${t("detail.priority")}"]`)!.focus(); });
    expect(part.querySelector(".rolepart__held")).toBeNull();
    expect([...part.children].indexOf(reason()!)).toBe(at);
  });

  it("choosing a role clears the reason and writes the hand-over, then the role", async () => {
    const id = await open("human");
    hoisted.assigned = [];
    await pickAssignee("ai");
    await act(async () => { container.querySelector<HTMLSelectElement>(`select[title="${t("detail.priority")}"]`)!.focus(); });
    await act(async () => { roleField()!.querySelector<HTMLInputElement>('input[value="12"]')!.click(); });
    await settle();
    expect(reason()).toBeNull();
    expect(hoisted.assigned).toEqual([`${id}:ai`]);
    expect(hoisted.asked).toEqual([`${id}:12`]);
  });

  it("handing it back to a person drops the waiting hand-over unwritten", async () => {
    const id = await open("human");
    hoisted.assigned = [];
    await pickAssignee("ai");
    await pickAssignee("human");
    expect(roleField()).toBeNull();
    expect(hoisted.assigned).toEqual([`${id}:human`]);
    expect(hoisted.asked).toEqual([]);
  });
});
