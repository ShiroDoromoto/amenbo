// @vitest-environment jsdom
// The folder a task is worked in, asked on the compose pane only when the project has several to choose
// from (`AMB-D-1012`). With one, the pane looks as it always has and core fills the folder in.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { listLabel, t, tf } from "../core/i18n";
import type { BoundFolderDto, TaskRoleDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  folders: [] as string[],
  roles: [] as TaskRoleDto[],
  filed: [] as { title: string; at: string | null | undefined }[],
  handed: [] as { assignee: string | null | undefined; roleId: number | null | undefined }[],
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    addTask: async (
      _projectId: number, title: string, _notes?: string, _due?: string | null, _start?: string | null, at?: string | null,
      assignee?: string | null, roleId?: number | null,
    ) => {
      hoisted.filed.push({ title, at });
      hoisted.handed.push({ assignee, roleId });
      return 1;
    },
    fetchProjectRoles: async (): Promise<TaskRoleDto[]> => hoisted.roles,
    fetchBoundFolders: async (): Promise<BoundFolderDto[]> =>
      hoisted.folders.map((path) => ({
        path, exists: true, mismatch: null, legacy: false, pointerMissing: false, foreign: null,
      })),
  };
});

import { TaskComposePane } from "./TaskComposePane";
import { StoreProvider } from "../store/store";
import { loadSnapshot } from "../core/snapshot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.filed = [];
  hoisted.handed = [];
  hoisted.folders = [];
  hoisted.roles = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

function type(input: HTMLInputElement, text: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  set.call(input, text);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function choose(select: HTMLSelectElement, value: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!;
  set.call(select, value);
  select.dispatchEvent(new Event("change", { bubbles: true }));
}

const create = () =>
  [...container.querySelectorAll<HTMLButtonElement>("button")].find((x) => x.textContent === t("compose.create"))!;
/** The role field's grid of choices, or null while it is not drawn. */
const rolePicker = () => container.querySelector<HTMLElement>(`[role="radiogroup"][aria-label="${t("detail.role")}"]`);
/** Each choice of the role field, as the label it is read by and whether it is checked. */
const roleChoices = () =>
  [...rolePicker()!.querySelectorAll<HTMLLabelElement>("label")].map((l) => ({
    name: l.textContent, checked: l.querySelector("input")!.checked, icon: l.querySelector(".identicon") !== null,
  }));
const pickRole = (value: string) => rolePicker()!.querySelector<HTMLInputElement>(`input[value="${value}"]`)!.click();
/** The assignee's button for a person or the AI. */
const owner = (facet: "human" | "ai") =>
  [...container.querySelectorAll<HTMLButtonElement>(`[role="group"][aria-label="${t("detail.assignee")}"] button`)]
    .find((x) => x.textContent === t(facet === "ai" ? "facet.ai" : "facet.human"))!;
const folderPicker = () =>
  container.querySelector<HTMLSelectElement>(`select[aria-label="${t("compose.folder")}"]`);

async function open() {
  await act(async () => {
    root.render(createElement(StoreProvider, null,
      createElement(TaskComposePane, { projectId: 1, label: "p", onCreated: () => {}, onCancel: () => {} })));
  });
  act(() => type(container.querySelector<HTMLInputElement>("input")!, "one task"));
}

describe("the folder a new task is worked in", () => {
  it("is asked for when the project has several, and the create waits for it", async () => {
    hoisted.folders = ["/work/app", "/work/mobile"];
    await open();
    const picker = folderPicker();
    expect(picker).not.toBeNull();
    expect(create().disabled).toBe(true);

    act(() => choose(picker!, "/work/mobile"));
    expect(create().disabled).toBe(false);
    await act(async () => { create().click(); });
    expect(hoisted.filed).toEqual([{ title: "one task", at: "/work/mobile" }]);
  });

  it("is not asked for when the project has one, and core is left to fill it in", async () => {
    hoisted.folders = ["/work/app"];
    await open();
    expect(folderPicker()).toBeNull();
    await act(async () => { create().click(); });
    expect(hoisted.filed).toEqual([{ title: "one task", at: null }]);
  });
});

describe("who a new task is handed to", () => {
  const pressed = () => (["human", "ai"] as const).filter((f) => owner(f).getAttribute("aria-pressed") === "true");

  it("is a person or the AI, with neither pressed until one is, and nobody when neither is", async () => {
    await open();
    expect(pressed()).toEqual([]);
    expect(container.textContent).not.toContain(t("detail.unassigned"));
    await act(async () => { create().click(); });
    expect(hoisted.handed).toEqual([{ assignee: null, roleId: null }]);
  });

  it("lights only the one pressed last", async () => {
    await open();
    await act(async () => { owner("ai").click(); });
    expect(pressed()).toEqual(["ai"]);
    await act(async () => { owner("human").click(); });
    expect(pressed()).toEqual(["human"]);
    await act(async () => { create().click(); });
    expect(hoisted.handed).toEqual([{ assignee: "human", roleId: null }]);
  });
});

describe("the role a new task is worked under", () => {
  const role = (id: number, name: string): TaskRoleDto =>
    ({ id, name, icon: null, automationId: 1, automationName: "開発" });

  it("is asked for once the task is handed to the AI, and a create without it says why under the field", async () => {
    hoisted.roles = [role(11, "実装係"), role(12, "レビュー係")];
    await open();
    expect(rolePicker()).toBeNull();

    await act(async () => { owner("ai").click(); });
    expect(rolePicker()).not.toBeNull();
    expect(roleChoices()).toEqual([
      { name: "実装係", checked: false, icon: true },
      { name: "レビュー係", checked: false, icon: true },
      { name: t("detail.roleNone"), checked: false, icon: false },
    ]);
    const heading = container.querySelector(".rolepart__h")!.textContent;
    expect(heading).toContain(t("detail.roleRequiredMark"));
    expect(heading).toContain(tf("detail.roleFrom", { name: "開発" }));

    await act(async () => { create().click(); });
    expect(hoisted.filed).toEqual([]);
    expect(container.querySelector('[role="alert"]')?.textContent).toContain(t("detail.roleRequired"));

    act(() => pickRole("12"));
    expect(container.querySelector('[role="alert"]')).toBeNull();
    expect(roleChoices().map((c) => c.checked)).toEqual([false, true, false]);
    await act(async () => { create().click(); });
    expect(hoisted.handed).toEqual([{ assignee: "ai", roleId: 12 }]);
  });

  it("names every automation the roles come from, and each role with its automation", async () => {
    hoisted.roles = [role(11, "実装係"), { ...role(21, "実装係"), automationId: 2, automationName: "保守" }];
    await open();
    await act(async () => { owner("ai").click(); });
    expect(container.querySelector(".rolepart__h")!.textContent)
      .toContain(tf("detail.roleFrom", { name: listLabel(["開発", "保守"]) }));
    expect(roleChoices().map((c) => c.name)).toEqual(["開発 · 実装係", "保守 · 実装係", t("detail.roleNone")]);
  });

  it("takes no role as a choice of its own", async () => {
    hoisted.roles = [role(11, "実装係")];
    await open();
    await act(async () => { owner("ai").click(); });
    act(() => pickRole("none"));
    await act(async () => { create().click(); });
    expect(hoisted.handed).toEqual([{ assignee: "ai", roleId: null }]);
  });

  it("is not asked for a task a person holds, nor in a project with no role", async () => {
    hoisted.roles = [role(11, "実装係")];
    await open();
    await act(async () => { owner("human").click(); });
    expect(rolePicker()).toBeNull();

    act(() => root.unmount());
    root = createRoot(container);
    hoisted.roles = [];
    await open();
    await act(async () => { owner("ai").click(); });
    expect(rolePicker()).toBeNull();
    await act(async () => { create().click(); });
    expect(hoisted.handed).toEqual([{ assignee: "ai", roleId: null }]);
  });
});
