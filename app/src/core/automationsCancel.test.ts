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

import { acknowledgeRun, cancelRun, forceCancelRun, pauseBeforeNextTask, pauseRun, resumeRun } from "./automations";
import { t } from "./i18n";
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
  says.mockReset();
  invalidated.mockReset();
});

// A running run is force-cancelled where it stands, which can leave an agent's changes half made — so
// the press asks first. A paused one has nothing under way and asks nothing (`AMB-D-1002`).
describe("cancelling a run", () => {
  it("asks before a force-cancel, saying what may be left behind, and stops the run on a yes", async () => {
    says.mockResolvedValue(true);
    invoke.mockResolvedValue({ stopped: true, ...ack(["automationRuns", "tasks"]) });
    expect(await forceCancelRun(7)).toBe(true);
    // The buttons say what each does: a "cancel" that meant "don't cancel" would read backwards.
    expect(says).toHaveBeenCalledWith(t("auto.run.forceCancelConfirm"), {
      ok: t("auto.run.forceCancel"),
      cancel: t("auto.run.keepRunning"),
    });
    expect(invoke).toHaveBeenCalledWith("automation_run_stop", { runId: 7 });
    // The task the run had reserved goes back to `todo`, so the board is redrawn along with the runs.
    expect(staleOf(drawn)).toEqual(["automationRuns", "launchState", "heldBack", "taskPage"]);
  });

  // The run finished on its own before the press landed: nothing was stopped, but the picture the
  // press was made on is the one that is behind, so it is redrawn all the same.
  it("redraws the runs even when the press stopped nothing", async () => {
    says.mockResolvedValue(true);
    invoke.mockResolvedValue({ stopped: false, ...ack(["automationRuns", "tasks"]) });
    expect(await forceCancelRun(7)).toBe(false);
    expect(staleOf(drawn)).toContain("automationRuns");
  });

  it("stops nothing when the question is answered no", async () => {
    says.mockResolvedValue(false);
    expect(await forceCancelRun(7)).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("cancels a paused run without asking", async () => {
    invoke.mockResolvedValue(ack(["automationRuns", "tasks"]));
    await cancelRun(7);
    expect(says).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("automation_run_cancel", { runId: 7 });
    expect(staleOf(drawn)).toEqual(["automationRuns", "launchState", "heldBack", "taskPage"]);
  });

  // The press on a row is redrawn on its ack, not on the change feed, which can come tens of seconds
  // after it (`AMB-T-16706`).
  it("redraws the runs the moment a pause, a resume or a seen lands", async () => {
    for (const press of [pauseRun, pauseBeforeNextTask, resumeRun, acknowledgeRun]) {
      invalidated.mockReset();
      invoke.mockResolvedValue(ack(["automationRuns"]));
      await press(7);
      expect(staleOf(drawn)).toEqual(["automationRuns", "launchState", "heldBack"]);
    }
  });
});
