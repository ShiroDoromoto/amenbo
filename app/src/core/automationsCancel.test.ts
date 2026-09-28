import { beforeEach, describe, it, expect, vi } from "vitest";

const invoke = vi.fn();
const says = vi.fn();
vi.mock("./ipc", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("./snapshot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./snapshot")>()),
  inTauri: () => true,
}));
vi.mock("./dialog", () => ({ confirmDialog: (message: string, labels?: unknown) => says(message, labels) }));

import { cancelRun, forceCancelRun } from "./automations";
import { t } from "./i18n";

beforeEach(() => {
  invoke.mockReset();
  says.mockReset();
});

// A running run is force-cancelled where it stands, which can leave an agent's changes half made — so
// the press asks first. A paused one has nothing under way and asks nothing (`AMB-D-1002`).
describe("cancelling a run", () => {
  it("asks before a force-cancel, saying what may be left behind, and stops the run on a yes", async () => {
    says.mockResolvedValue(true);
    invoke.mockResolvedValue(true);
    expect(await forceCancelRun(7)).toBe(true);
    // The buttons say what each does: a "cancel" that meant "don't cancel" would read backwards.
    expect(says).toHaveBeenCalledWith(t("auto.run.forceCancelConfirm"), {
      ok: t("auto.run.forceCancel"),
      cancel: t("auto.run.keepRunning"),
    });
    expect(invoke).toHaveBeenCalledWith("automation_run_stop", { runId: 7 });
  });

  it("stops nothing when the question is answered no", async () => {
    says.mockResolvedValue(false);
    expect(await forceCancelRun(7)).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("cancels a paused run without asking", async () => {
    invoke.mockResolvedValue(undefined);
    await cancelRun(7);
    expect(says).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("automation_run_cancel", { runId: 7 });
  });
});
