// @vitest-environment jsdom
// The way from a task a failed round was filed as to the terminal the round fell in: the run's pane is
// gone to, turned to its terminal (`../shell/RunPicture`) — whatever the run stands at, an acknowledged
// failure that has left the "running" tab included.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { AutomationRunCardDto } from "../bindings/bindings";

/** The runs whose failed round each task was filed as, by the task — what the host answers
 *  `automation_run_of_incident` with. */
const hoisted = vi.hoisted(() => ({ filed: new Map<number, AutomationRunCardDto>() }));

vi.mock("../core/automations", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/automations")>()),
  useRunOfIncident: (taskId: number) => hoisted.filed.get(taskId) ?? null,
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
const failedRun = (run: number, task: number, acknowledged = false): AutomationRunCardDto => ({
  run, project: 1, projectName: "P", automation: 3, automationName: "Nightly", status: "failed", launch: 6,
  pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false, waiting: false, waitsForRoom: false,
  stepsDone: 2, passes: [], reportWithheld: [], acknowledged,
  incident: { id: task, ref: `AMB-T-${task}`, title: `Task ${task}`, person: true },
} as AutomationRunCardDto);

async function settle() {
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}

async function open(taskId: number, onGoToRun?: (project: number, run: number, launch?: number | null) => void) {
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
  hoisted.filed = new Map();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the task a failed round was filed as", () => {
  it("goes to its launch's pane with the run in front, turned to its terminal", async () => {
    const id = (await addTask(1, "障害"))!;
    hoisted.filed.set(id, failedRun(81, id));
    const went: Array<[number, number, number | null | undefined]> = [];
    await open(id, (project, run, launch) => went.push([project, run, launch]));
    expect(container.textContent).toContain(t("auto.run.failedRound"));
    await act(async () => press()!.click());
    expect(went).toEqual([[1, 81, 6]]);
    expect(faceOf(81)).toBe("terminal");
  });

  it("goes there just the same once the failure has been acknowledged", async () => {
    const id = (await addTask(1, "確認済みの障害"))!;
    hoisted.filed.set(id, failedRun(84, id, true));
    const went: Array<[number, number]> = [];
    await open(id, (project, run) => went.push([project, run]));
    expect(container.textContent).toContain(t("auto.run.failedRound"));
    await act(async () => press()!.click());
    expect(went).toEqual([[1, 84]]);
    expect(faceOf(84)).toBe("terminal");
  });

  it("offers nothing on a task no failed round was filed as", async () => {
    const id = (await addTask(1, "ふつうのタスク"))!;
    hoisted.filed.set(id + 1000, failedRun(82, id + 1000));
    await open(id, () => {});
    expect(press()).toBeUndefined();
  });
});
