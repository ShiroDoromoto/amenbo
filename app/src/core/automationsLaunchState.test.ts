import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
const tauri = vi.hoisted(() => ({ on: true }));
vi.mock("./ipc", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("./snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./snapshot")>()),
  inTauri: () => tauri.on,
}));

import { fetchLaunchState, launchHeadcount, launchWorking, roleHeld } from "./automations";
import type { AutomationLaunchStateDto, AutomationRunCardDto } from "../bindings/bindings";

beforeEach(() => {
  invoke.mockReset();
  tauri.on = true;
});

// One launch is read at once, by its id, for the pane drawn per launch.
describe("reading one launch", () => {
  it("asks for the launch by its id and hands back what it answers", async () => {
    const state = { launch: 3, project: 1, automation: 2, automationName: "Alice", status: "running", maxHeadcount: 10, roles: [] };
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

// The pane header's "N / M working" and the hold a role's seats read.
describe("counting one launch", () => {
  const run = (over: Partial<AutomationRunCardDto>) => ({ status: "running", waiting: false, ...over }) as unknown as AutomationRunCardDto;
  const role = (headcount: number, runs: AutomationRunCardDto[], held = false) => ({
    role: headcount, name: "Alice", defined: headcount, headcount, underWay: runs.length, toLetGo: 0, held, runs,
    waiting: 0, firstWaiting: [],
  });
  const state = (over: Partial<AutomationLaunchStateDto> = {}): AutomationLaunchStateDto => ({
    launch: 3, project: 1, automation: 2, automationName: "Alice", status: "running", maxHeadcount: 10, failed: [],
    roles: [
      role(3, [run({ run: 1 }), run({ run: 2, waiting: true }), run({ run: 3, pauseBeforeNextTask: true })]),
      role(2, [run({ run: 4, status: "paused" })], true),
    ],
    ...over,
  });

  it("counts the seats working a task: running, and not waiting for one to turn up", () => {
    expect(launchWorking(state())).toBe(2);
  });

  it("counts nothing working once the launch has ended", () => {
    expect(launchWorking(state({ status: "ended" }))).toBe(0);
  });

  it("adds every role's headcount up", () => {
    expect(launchHeadcount(state())).toBe(5);
  });

  it("holds a role stopped on its own, and every role of a paused launch", () => {
    const running = state();
    expect(roleHeld(running, running.roles[0]!)).toBe(false);
    expect(roleHeld(running, running.roles[1]!)).toBe(true);
    const paused = state({ status: "paused" });
    expect(roleHeld(paused, paused.roles[0]!)).toBe(true);
  });
});
