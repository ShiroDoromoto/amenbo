// Which chip a decision wears. What is being asserted is the reading order — the unfinished writing
// is read before the status, and the status is read as itself and not as what an edge says about it.
import { describe, expect, it } from "vitest";
import { decisionStatusChip } from "./decisionStatus";

describe("decisionStatusChip", () => {
  it("reads the unfinished writing first — a decision is `decided` from the moment it is saved", () => {
    expect(decisionStatusChip("decided", true)).toBe("chip--status-draft");
    expect(decisionStatusChip("rejected", true)).toBe("chip--status-draft");
  });

  it("gives the two finished steps a chip each", () => {
    expect(decisionStatusChip("decided", false)).toBe("chip--status-decided");
    expect(decisionStatusChip("rejected", false)).toBe("chip--status-rejected");
  });
});
