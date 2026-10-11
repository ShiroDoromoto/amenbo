// What a run's picture lights of the way it has come (`AMB-T-5825`).
//
// **Only the lap under way is lit.** A run that goes back to a spot it passed starts a lap there:
// the boxes and lines it walked before are left dark, and the line it came back by stays lit.
import { describe, expect, it } from "vitest";
import { picTrail, stepTrail } from "./RunPicture";
import type { AutomationRunPassDto, AutomationRunPassedStepDto } from "../bindings/bindings";

const pass = (placement: number, edge?: number): AutomationRunPassDto =>
  ({ placement, status: edge === undefined ? "running" : "done", edge });

describe("picTrail", () => {
  it("lights all it passed while it has not gone back", () => {
    const lit = picTrail({ passed: [pass(1, 10), pass(2, 20), pass(3)], at: 3, steps: [] });
    expect([...lit.boxes]).toEqual([1, 2, 3]);
    expect([...lit.edges]).toEqual([10, 20]);
    expect(lit.at).toBe(3);
  });

  it("leaves the lap before dark once it goes back", () => {
    // 1 → 2 → 3, back to 1 by 30, then 1 → 2 and under way at 2.
    const lit = picTrail({ passed: [pass(1, 10), pass(2, 20), pass(3, 30), pass(1, 10), pass(2)], at: 2, steps: [] });
    expect([...lit.boxes].sort((a, b) => a - b)).toEqual([1, 2]);
    expect([...lit.edges].sort((a, b) => a - b)).toEqual([10, 30]);
  });

  it("does not light a line of the lap before that the lap under way has not walked", () => {
    // 1 → 2 → 4 → 5, back to 2 by 50; under way at 3 after 2 → 3.
    const lit = picTrail({
      passed: [pass(1, 10), pass(2, 24), pass(4, 45), pass(5, 50), pass(2, 23), pass(3)], at: 3, steps: [],
    });
    expect([...lit.boxes].sort((a, b) => a - b)).toEqual([2, 3]);
    expect([...lit.edges].sort((a, b) => a - b)).toEqual([23, 50]);
  });

  it("starts from the last lap of several", () => {
    const lit = picTrail({
      passed: [pass(1, 12), pass(2, 21), pass(1, 12), pass(2, 21), pass(1)], at: 1, steps: [],
    });
    expect([...lit.boxes]).toEqual([1]);
    expect([...lit.edges]).toEqual([21]);
  });

  it("lights nothing for no trail", () => {
    const lit = picTrail(null);
    expect(lit.boxes.size).toBe(0);
    expect(lit.edges.size).toBe(0);
    expect(lit.at).toBeUndefined();
  });
});

/// Inside the box under way, the steps are lit by the same rule (`AMB-T-6505`).
describe("stepTrail", () => {
  const passed = (step: number, edge?: number): AutomationRunPassedStepDto =>
    ({ step, status: edge === undefined ? "running" : "done", edge });

  it("lights the steps it passed in the spot it is in, and stands at the step under way", () => {
    const lit = stepTrail({ passed: [pass(1, 10), pass(2)], at: 2, steps: [passed(5, 50), passed(6)], step: 6 });
    expect([...lit.boxes]).toEqual([5, 6]);
    expect([...lit.edges]).toEqual([50]);
    expect(lit.at).toBe(6);
  });

  it("leaves the lap before dark once it goes back to a step", () => {
    // 5 → 6, back to 5 by 65, under way at 5.
    const lit = stepTrail({ passed: [pass(2)], at: 2, steps: [passed(5, 56), passed(6, 65), passed(5)], step: 5 });
    expect([...lit.boxes]).toEqual([5]);
    expect([...lit.edges]).toEqual([65]);
  });

  it("lights nothing inside before it opens a step there, and for no trail", () => {
    const opening = stepTrail({ passed: [pass(2)], at: 2, steps: [], step: 5 });
    expect(opening.boxes.size).toBe(0);
    expect(opening.at).toBe(5);
    expect(stepTrail(null).at).toBeUndefined();
  });
});
