// @vitest-environment jsdom
// The way from a task a failed round was filed as to the terminal the round fell in: the run's pane is
// gone to, turned to its terminal (`../shell/RunPicture`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationRunCardDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({ live: [] as AutomationRunCardDto[] }));

vi.mock("../core/automations", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/automations")>()),
  useLiveRuns: () => hoisted.live,
}));

import { TaskDetailPane } from "./TaskDetailPane";
import { StoreProvider } from "../store/store";
import { loadSnapshot } from "../core/snapshot";
import { addTask } from "../core/mutations";
import { t, tf } from "../core/i18n";
import { faceOf } from "../shell/runFaces";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** A failed run whose round was filed as the task `task`. */
const failedRun = (run: number, task: number): AutomationRunCardDto => ({
  run, project: 1, projectName: "P", automation: 3, automationName: "Nightly", status: "failed",
  pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false, waiting: false, waitsForRoom: false,
  stepsDone: 2, passes: [], reportWithheld: [], acknowledged: false,
  incident: { id: task, ref: `AMB-T-${task}`, person: true },
} as AutomationRunCardDto);

async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

async function open(taskId: number, onGoToRun?: (project: number, run: number) => void) {
  act(() => root.render(createElement(StoreProvider, null, createElement(TaskDetailPane, { taskId, onGoToRun }))));
  await settle();
}

const press = () =>
  [...container.querySelectorAll<HTMLButtonElement>("button")].find((one) =>
    one.textContent === tf("auto.run.openFailedRound", { automation: "Nightly" }));

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  hoisted.live = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the task a failed round was filed as", () => {
  it("goes to the run's pane, turned to its terminal", async () => {
    const id = (await addTask(1, "障害"))!;
    hoisted.live = [failedRun(81, id)];
    const went: Array<[number, number]> = [];
    await open(id, (project, run) => went.push([project, run]));
    expect(container.textContent).toContain(t("auto.run.failedRound"));
    await act(async () => press()!.click());
    expect(went).toEqual([[1, 81]]);
    expect(faceOf(81)).toBe("terminal");
  });

  it("offers nothing on a task no failed round was filed as", async () => {
    const id = (await addTask(1, "ふつうのタスク"))!;
    hoisted.live = [failedRun(82, id + 1000), { ...failedRun(83, id), status: "running" }];
    await open(id, () => {});
    expect(press()).toBeUndefined();
  });
});
