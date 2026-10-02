// @vitest-environment jsdom
// The folder a task is worked in, asked on the compose pane only when the project has several to choose
// from (`AMB-D-1012`). With one, the pane looks as it always has and core fills the folder in.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";
import type { BoundFolderDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  folders: [] as string[],
  filed: [] as { title: string; at: string | null | undefined }[],
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    addTask: async (
      _projectId: number, title: string, _notes?: string, _due?: string | null, _start?: string | null, at?: string | null,
    ) => {
      hoisted.filed.push({ title, at });
      return 1;
    },
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
