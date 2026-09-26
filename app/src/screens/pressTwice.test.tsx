// @vitest-environment jsdom
// A second press before the first write has come back sends nothing. Each press here lands in one go with
// the one before it, ahead of any render — the way presses piled up behind a stuck app arrive.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  addTask: [] as string[],
  addDimension: [] as string[],
  /** Lets go of the task writes the test is holding. */
  release: () => {},
}));

vi.mock("../core/mutations", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/mutations")>();
  return {
    ...orig,
    addTask: (_projectId: number, title: string) => {
      hoisted.addTask.push(title);
      return new Promise<number>((resolve) => { hoisted.release = () => resolve(1); });
    },
    addDimension: async (_projectId: number, name: string) => { hoisted.addDimension.push(name); },
  };
});

import { TaskComposePane } from "./TaskComposePane";
import { DimensionManager } from "./DimensionManager";
import { StoreProvider } from "../store/store";
import { loadSnapshot } from "../core/snapshot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.addTask = [];
  hoisted.addDimension = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

/** Type into a controlled box the way the browser does, so React's onChange sees it. */
function type(input: HTMLInputElement, text: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  set.call(input, text);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

const enter = (el: Element) =>
  el.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));

const button = (label: string) => {
  const b = [...container.querySelectorAll<HTMLButtonElement>("button")].find((x) => x.textContent === label);
  if (!b) throw new Error(`no button reads ${label}`);
  return b;
};

describe("creating a task", () => {
  function open(onCreated = () => {}) {
    act(() => root.render(createElement(StoreProvider, null,
      createElement(TaskComposePane, { projectId: 1, label: "p", onCreated, onCancel: () => {} }))));
    const title = container.querySelector<HTMLInputElement>("input")!;
    act(() => type(title, "one task"));
    return title;
  }

  it("files one task for two presses of the button", async () => {
    const created = vi.fn();
    open(created);
    const create = button(t("compose.create"));
    act(() => { create.click(); create.click(); });
    expect(hoisted.addTask).toEqual(["one task"]);
    expect(create.disabled).toBe(true);

    await act(async () => { hoisted.release(); });
    expect(created).toHaveBeenCalledTimes(1);
  });

  it("files one task for Enter pressed twice", async () => {
    const title = open();
    act(() => { enter(title); enter(title); });
    expect(hoisted.addTask).toEqual(["one task"]);
    await act(async () => { hoisted.release(); });
  });
});

describe("adding an axis from the inline box", () => {
  function openBox() {
    act(() => root.render(createElement(StoreProvider, null,
      createElement(DimensionManager, { projectId: 1, onClose: () => {} }))));
    act(() => button(t("dimmgr.addDimension")).click());
    const box = container.querySelector<HTMLInputElement>(".dimmgr__adddim")!;
    act(() => type(box, "Phase"));
    return box;
  }

  it("adds it once when Enter is followed by the box losing the caret", async () => {
    const box = openBox();
    await act(async () => {
      enter(box);
      box.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    expect(hoisted.addDimension).toEqual(["Phase"]);
  });

  it("adds nothing when Escape is followed by the box losing the caret", async () => {
    const box = openBox();
    await act(async () => {
      box.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      box.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    expect(hoisted.addDimension).toEqual([]);
  });
});
