import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
const says = vi.fn();
vi.mock("./ipc", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("./snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./snapshot")>()),
  inTauri: () => true,
}));
vi.mock("./dialog", () => ({ confirmDialog: (message: string, labels?: unknown) => says(message, labels) }));

import {
  holdLaunchRole,
  pauseLaunchBeforeNextTask,
  releaseLaunchRole,
  resetLaunchHeadcount,
  resetLaunchLimit,
  resumeLaunch,
  setLaunchHeadcount,
  setLaunchLimit,
  stopLaunch,
} from "./automations";

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
  says.mockReset();
});

// A role's menu in a launch's pane acts on that one launch and that one role; the definition is not
// touched (`AMB-D-1123`). The bounds and the refusals are core's, so each press only names its command.
describe("acting on one role of a launch", () => {
  it("changes the role's headcount for this launch", async () => {
    await setLaunchHeadcount(4, 9, 3);
    expect(invoke).toHaveBeenCalledWith("automation_launch_headcount_set", { launchId: 4, roleId: 9, headcount: 3 });
  });

  it("puts the role back at its definition's headcount", async () => {
    await resetLaunchHeadcount(4, 9);
    expect(invoke).toHaveBeenCalledWith("automation_launch_headcount_reset", { launchId: 4, roleId: 9 });
  });

  it("stops the role once its tasks under way are done", async () => {
    await holdLaunchRole(4, 9);
    expect(invoke).toHaveBeenCalledWith("automation_launch_role_hold", { launchId: 4, roleId: 9 });
  });

  it("takes the role up again", async () => {
    await releaseLaunchRole(4, 9);
    expect(invoke).toHaveBeenCalledWith("automation_launch_role_release", { launchId: 4, roleId: 9 });
  });

  it("hands a refusal back to the caller", async () => {
    invoke.mockRejectedValue(new Error("refused"));
    await expect(setLaunchHeadcount(4, 9, 11)).rejects.toThrow("refused");
  });
});

// The pane's header acts on the whole launch, every role alike.
describe("acting on a whole launch", () => {
  it("changes the run limit for this launch, or lifts it", async () => {
    await setLaunchLimit(4, 3);
    expect(invoke).toHaveBeenCalledWith("automation_launch_limit_set", { launchId: 4, limit: 3 });
    await setLaunchLimit(4, null);
    expect(invoke).toHaveBeenCalledWith("automation_launch_limit_set", { launchId: 4, limit: null });
  });

  it("puts the run limit back at its definition's", async () => {
    await resetLaunchLimit(4);
    expect(invoke).toHaveBeenCalledWith("automation_launch_limit_reset", { launchId: 4 });
  });

  it("pauses it before each run takes its next task", async () => {
    await pauseLaunchBeforeNextTask(4);
    expect(invoke).toHaveBeenCalledWith("automation_launch_pause_before_next_task", { launchId: 4 });
  });

  it("picks it up again", async () => {
    await resumeLaunch(4);
    expect(invoke).toHaveBeenCalledWith("automation_launch_resume", { launchId: 4 });
  });

  it("stops it without asking here", async () => {
    await stopLaunch(4);
    expect(says).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("automation_launch_stop", { launchId: 4 });
  });
});
