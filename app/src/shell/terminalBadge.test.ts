// The badge is a knock, not a status light. What is pinned here is the difference: it goes up for a
// turn that came while the person was elsewhere, and stays down for one they have already been shown
// — including the one that came up while they were sitting on the terminal face.
import { describe, expect, it } from "vitest";
import { NO_ATTENTION, badgeUp, knock, looked, turnCame } from "./terminalBadge";

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

// The OS is knocked on at exactly the moment the badge goes up. They answer the same question — a turn
// came up while nobody was looking at the terminal — and reading both off one state is what keeps them
// from ever disagreeing, so what is pinned here is that the two never come apart.
describe("knocking on the OS", () => {
  it("knocks when the badge goes up, and only then", () => {
    const away = turnCame(NO_ATTENTION, true, false);
    expect(knock(NO_ATTENTION, away), "the badge went up in silence").toBe(true);
    // The same turn said again is the same turn: the badge did not move, so nothing knocks.
    expect(knock(away, turnCame(away, true, false))).toBe(false);
  });

  it("does not knock for a turn that came up while the person was already there", () => {
    const here = turnCame(NO_ATTENTION, true, true);
    expect(badgeUp(here), "being on the face is being told").toBe(false);
    expect(knock(NO_ATTENTION, here)).toBe(false);
  });

  it("does not knock when a turn is answered, or when the badge is merely spent", () => {
    const away = turnCame(NO_ATTENTION, true, false);
    expect(knock(away, looked(away)), "crossing over knocked").toBe(false);
    expect(knock(away, turnCame(away, false, false)), "going back to work knocked").toBe(false);
  });

  it("knocks again for a turn that is genuinely new", () => {
    let a = looked(turnCame(NO_ATTENTION, true, false));
    a = turnCame(a, false, false); // back at work
    const again = turnCame(a, true, false);
    expect(knock(a, again)).toBe(true);
  });
});
