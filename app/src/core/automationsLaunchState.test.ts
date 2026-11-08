import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
const tauri = vi.hoisted(() => ({ on: true }));
vi.mock("./ipc", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("./snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./snapshot")>()),
  inTauri: () => tauri.on,
}));

import { fetchLaunchState } from "./automations";

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
