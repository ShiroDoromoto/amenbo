import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
const tauri = vi.hoisted(() => ({ on: true }));
vi.mock("./ipc", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("./snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./snapshot")>()),
  inTauri: () => tauri.on,
}));

import { fetchLaunchState, fetchLiveLaunches, launchPause, roleHeld } from "./automations";
import type { AutomationLaunchStateDto, AutomationRunCardDto } from "../bindings/bindings";

beforeEach(() => {
  invoke.mockReset();
  tauri.on = true;
});

// One launch is read at once, by its id, for the pane drawn per launch.
describe("reading one launch", () => {
  it("asks for the launch by its id and hands back what it answers", async () => {
    const state = { launch: 3, project: 1, automation: 2, automationName: "Alice", status: "running", maxHeadcount: 10, maxRunLimit: 100, underWay: 0, roles: [] };
    invoke.mockResolvedValue(state);
    expect(await fetchLaunchState(3)).toBe(state);
    expect(invoke).toHaveBeenCalledWith("automation_launch_state", { launchId: 3 });
  });

  it("reads nothing outside Tauri", async () => {
    tauri.on = false;
    expect(await fetchLaunchState(3)).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });
});

// The launches that have not ended are read at once, for the panes the app stands again as it comes up.
describe("reading the launches still going", () => {
  it("asks with nothing and hands back what it answers", async () => {
    const live = [{ launch: 3, project: 1 }, { launch: 5, project: 2 }];
    invoke.mockResolvedValue(live);
    expect(await fetchLiveLaunches()).toBe(live);
    expect(invoke).toHaveBeenCalledWith("automation_launches_live", {});
  });

  it("reads nothing outside Tauri", async () => {
    tauri.on = false;
    expect(await fetchLiveLaunches()).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
  });
});

// The pane header's "Paused: n / m seats stopped" and the hold a role's seats read.
describe("counting one launch", () => {
  const run = (over: Partial<AutomationRunCardDto>) => ({ status: "running", waiting: false, ...over }) as unknown as AutomationRunCardDto;
  const role = (headcount: number, runs: AutomationRunCardDto[], held = false) => ({
    role: headcount, name: "Alice", defined: headcount, headcount, underWay: runs.length, toLetGo: 0, held, runs,
    waiting: 0, firstWaiting: [], atRunLimit: false,
  });
  const state = (over: Partial<AutomationLaunchStateDto> = {}): AutomationLaunchStateDto => ({
    launch: 3, project: 1, automation: 2, automationName: "Alice", status: "running", maxHeadcount: 10, maxRunLimit: 100, underWay: 0, attention: [],
    roles: [
      role(3, [run({ run: 1 }), run({ run: 2, waiting: true }), run({ run: 3, pauseBeforeNextTask: true })]),
      role(2, [run({ run: 4, status: "paused" })], true),
    ],
    ...over,
  });

  it("says nothing of seats stopped while the launch is not paused", () => {
    expect(launchPause(state())).toBeNull();
    expect(launchPause(state({ status: "ended" }))).toBeNull();
  });

  it("counts a paused launch's seats stopped: paused, or waiting for something to turn up", () => {
    // Four seats: one still working its task, one waiting, one asked to pause and still working, one paused.
    expect(launchPause(state({ status: "paused" }))).toEqual({ stopped: 2, seats: 4 });
  });

  it("counts every seat stopped once none is working a task", () => {
    const stopped = state({
      status: "paused",
      roles: [role(2, [run({ run: 1, status: "paused" }), run({ run: 2, waiting: true })])],
    });
    expect(launchPause(stopped)).toEqual({ stopped: 2, seats: 2 });
  });

  it("holds a role stopped on its own, and every role of a paused launch", () => {
    const running = state();
    expect(roleHeld(running, running.roles[0]!)).toBe(false);
    expect(roleHeld(running, running.roles[1]!)).toBe(true);
    const paused = state({ status: "paused" });
    expect(roleHeld(paused, paused.roles[0]!)).toBe(true);
  });
});
