import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
const says = vi.fn();
vi.mock("./ipc", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
const invalidated = vi.fn();
vi.mock("./snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./snapshot")>()),
  inTauri: () => true,
  loadSnapshot: () => Promise.resolve(),
}));
vi.mock("./query", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./query")>()),
  invalidateQueries: (stale: (key: QueryKey) => boolean) => invalidated(stale),
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
import type { QueryKey } from "./query";

/** What core answers a press on a run or a launch with: no ids, just the scopes it leaves stale. */
const ack = (scopes: string[]) => ({ tasks: [], decisions: [], automations: [], actions: [], placements: [], scopes });

/** Which of these keys the last ack sent stale — the screen redraws them the moment the press lands. */
function staleOf(keys: QueryKey[]): string[] {
  const calls = invalidated.mock.calls;
  const stale = calls[calls.length - 1]?.[0] as ((key: QueryKey) => boolean) | undefined;
  return stale === undefined ? [] : keys.filter((key) => stale(key)).map((key) => String(key[0]));
}

const drawn: QueryKey[] = [["automationRuns"], ["launchState", 4], ["heldBack", 7], ["taskPage", "x"], ["automations", 1]];

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(ack(["automationRuns"]));
  says.mockReset();
  invalidated.mockReset();
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

  // The pane's header is redrawn on the press's ack, not on the change feed, which can come tens of
  // seconds after it (`AMB-T-16706`).
  it("redraws the launch's pane the moment a press lands", async () => {
    await pauseLaunchBeforeNextTask(4);
    expect(staleOf(drawn)).toEqual(["automationRuns", "launchState", "heldBack"]);
  });

  it("stops it without asking here", async () => {
    invoke.mockResolvedValue(ack(["automationRuns", "tasks"]));
    await stopLaunch(4);
    expect(says).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("automation_launch_stop", { launchId: 4 });
    expect(staleOf(drawn)).toEqual(["automationRuns", "launchState", "heldBack", "taskPage"]);
  });
});
