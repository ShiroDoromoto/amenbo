// The ladder a registered image is taken down, from the 96px it is stored at to the pixels a 24px or
// an 18px box draws it at. What is asserted is the shape of the reduction — every step at most a
// half, the last one exactly on the target, and nothing at all where the browser's own step is as
// good — rather than what any one pair of numbers comes to.
import { describe, expect, it } from "vitest";
import { shrinkSteps } from "./shrinkImage";

describe("shrinkSteps", () => {
  // 24 CSS px on a 1x screen is 24 device pixels, which is the quarter the browser cannot stand in
  // one step (`AMB-T-5172`).
  it("halves 96 down to a 24px mark on a 1x screen", () => {
    expect(shrinkSteps(96, 24)).toEqual([48, 24]);
  });

  // 18 is not a power of two below 96, so the last step is shorter than a half — which is the
  // direction that is safe. What must not happen is a step longer than one.
  it("lands exactly on a target no halving reaches", () => {
    expect(shrinkSteps(96, 18)).toEqual([48, 24, 18]);
  });

  it("never takes more than half in one step", () => {
    for (let px = 1; px <= 96; px++) {
      const steps = shrinkSteps(96, px);
      let from = 96;
      for (const step of steps) {
        expect(step).toBeGreaterThanOrEqual(from / 2);
        expect(step).toBeLessThan(from);
        from = step;
      }
      if (steps.length > 0) expect(steps[steps.length - 1]).toBe(px);
    }
  });

  // A half is the one ratio every browser's filter covers well, so a bake there would cost a canvas
  // and a second copy of the bytes to draw the same thing.
  it("hands the stored image over where one browser step covers the reduction", () => {
    expect(shrinkSteps(96, 48)).toEqual([]);
    expect(shrinkSteps(96, 96)).toEqual([]);
  });

  // The box is larger than the image it was given — an older bake, or an image that arrived some
  // other way. Enlarging is the browser's, and doing it here would only fix the blur in place.
  it("does not enlarge", () => {
    expect(shrinkSteps(48, 96)).toEqual([]);
  });

  it("asks for nothing where either side is not a size", () => {
    expect(shrinkSteps(0, 24)).toEqual([]);
    expect(shrinkSteps(96, 0)).toEqual([]);
    expect(shrinkSteps(Number.NaN, 24)).toEqual([]);
  });
});
