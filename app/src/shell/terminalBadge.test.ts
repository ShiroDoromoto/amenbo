// The badge is a knock, not a status light. What is pinned here is the difference: it goes up for a
// turn that came while the person was elsewhere, and stays down for one they have already been shown
// — including the one that came up while they were sitting on the terminal face.
import { describe, expect, it } from "vitest";
import { badgeUp, looked, NO_ATTENTION, turnCame } from "./terminalBadge";

describe("the terminal segment's badge", () => {
  it("goes up for a turn that comes while the ledger is up", () => {
    expect(badgeUp(turnCame(NO_ATTENTION, true, false))).toBe(true);
  });

  it("stays down for a turn that comes while the person is on the terminal face", () => {
    const a = turnCame(NO_ATTENTION, true, true);
    expect(badgeUp(a)).toBe(false);
    // And it does not appear on the way back to the ledger — they have been shown it.
    expect(badgeUp(turnCame(a, true, false))).toBe(false);
  });

  it("comes down when the person crosses over, and does not come back for the same turn", () => {
    let a = turnCame(NO_ATTENTION, true, false);
    a = looked(a);
    expect(badgeUp(a)).toBe(false);
    // The pane keeps saying the turn is standing while nobody has answered it.
    a = turnCame(a, true, false);
    expect(badgeUp(a), "the same turn knocked twice").toBe(false);
  });

  it("knocks again once the agent has gone back to work and stopped anew", () => {
    let a = looked(turnCame(NO_ATTENTION, true, false));
    a = turnCame(a, false, false); // back at work
    expect(badgeUp(a)).toBe(false);
    a = turnCame(a, true, false); // and waiting on a person again
    expect(badgeUp(a)).toBe(true);
  });

  it("is down with nothing waiting, whatever the person is looking at", () => {
    expect(badgeUp(turnCame(NO_ATTENTION, false, false))).toBe(false);
    expect(badgeUp(looked(NO_ATTENTION))).toBe(false);
  });
});
