import { describe, expect, it } from "vitest";
import type { AutomationRunCardDto } from "../bindings/bindings";
import { runSayOf } from "./runWords";

/** A run as the "running" tab reads it, failed and nobody told yet. */
function card(over: Partial<AutomationRunCardDto> = {}): AutomationRunCardDto {
  return {
    run: 12,
    project: 1,
    projectName: "lantern",
    automation: 3,
    automationName: "nightly",
    status: "failed",
    pauseRequested: false,
    waiting: false,
    stoppedReason: "crashed",
    stepName: "build",
    placement: 7,
    stepsDone: 2,
    reportWithheld: [],
    acknowledged: false,
    ...over,
  };
}

describe("the row over a run's pane no step has arrived in", () => {
  it("says nothing until the run has been read", () => {
    expect(runSayOf(undefined)).toBeNull();
  });

  it("names the step the run stopped on, and why, with the box it stopped at", () => {
    const say = runSayOf(card())!;
    expect(say.run).toBe(12);
    expect(say.automation).toBe("nightly");
    expect(say.step).toBe("build");
    expect(say.automationId).toBe(3);
    expect(say.placement).toBe(7);
    expect(say.builtin).toBe(false);
    expect(say.state?.status).toBe("failed");
    expect(say.state?.why).not.toBeNull();
    expect(say.state?.acknowledged).toBe(false);
  });

  it("says a held run as held", () => {
    const say = runSayOf(card({ status: "paused", stoppedReason: null }))!;
    expect(say.state?.status).toBe("paused");
    expect(say.state?.why).toBeNull();
  });

  it("names no task while a built-in waits for the next one", () => {
    const task = { id: 5, key: "AMB-T-5", title: "t" } as unknown as AutomationRunCardDto["task"];
    expect(runSayOf(card({ status: "running", waiting: true, task }))!.task).toBeNull();
    expect(runSayOf(card({ task }))!.task).toBe(task);
  });
});
