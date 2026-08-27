import { describe, expect, it } from "vitest";

import { axesFor, classifiesDecisions, classifiesTasks } from "./appliesTo";

const axis = (appliesTo: "task" | "decision" | "both") => ({ appliesTo });

describe("appliesTo", () => {
  it("reads the wide side as both, which is where an axis nobody narrowed sits", () => {
    expect(classifiesTasks(axis("both"))).toBe(true);
    expect(classifiesDecisions(axis("both"))).toBe(true);
  });

  it("keeps a narrowed axis off the side it no longer classifies", () => {
    expect(classifiesTasks(axis("task"))).toBe(true);
    expect(classifiesDecisions(axis("task"))).toBe(false);
    expect(classifiesTasks(axis("decision"))).toBe(false);
    expect(classifiesDecisions(axis("decision"))).toBe(true);
  });

  it("splits a list per side and leaves the order alone", () => {
    const dims = [axis("task"), axis("both"), axis("decision")];
    expect(axesFor("task", dims)).toEqual([dims[0], dims[1]]);
    expect(axesFor("decision", dims)).toEqual([dims[1], dims[2]]);
  });
});
