// The board's one standing notice (`AMB-D-535`): which of the candidates is drawn, and — the whole point
// of having one place decide — that the answer is never two of them.
import { describe, expect, it } from "vitest";
import { BOARD_NOTICES, pickBoardNotice, type BoardNoticeStanding } from "./boardNotice";

const standing = (over: Partial<BoardNoticeStanding> = {}): BoardNoticeStanding => ({
  linkFolder: false,
  firstLoop: false,
  agentHookWiring: false,
  ...over,
});

describe("pickBoardNotice", () => {
  it("draws nothing where nothing is standing", () => {
    expect(pickBoardNotice(standing())).toBeNull();
  });

  it("draws the one that is standing, wherever it sits in the order", () => {
    expect(pickBoardNotice(standing({ linkFolder: true }))).toBe("linkFolder");
    expect(pickBoardNotice(standing({ firstLoop: true }))).toBe("firstLoop");
    expect(pickBoardNotice(standing({ agentHookWiring: true }))).toBe("agentHookWiring");
  });

  // The reason this function exists: two notices at once is the screen that does not say which comes first.
  it("picks one where several are standing, and it is the earliest premise", () => {
    expect(pickBoardNotice(standing({ firstLoop: true, agentHookWiring: true }))).toBe("firstLoop");
    expect(pickBoardNotice(standing({ linkFolder: true, agentHookWiring: true }))).toBe("linkFolder");
    expect(
      pickBoardNotice(standing({ linkFolder: true, firstLoop: true, agentHookWiring: true })),
    ).toBe("linkFolder");
  });

  // The order is the decision, so it is pinned rather than inferred: with no folder there is nowhere to
  // open a terminal, nowhere to paste and nothing to wire (`AMB-D-533`), and the wiring means nothing to a
  // reader who has not yet seen Amenbo hold a task (`AMB-D-516`).
  it("orders the folder ahead of the loop, and the loop ahead of the wiring", () => {
    expect([...BOARD_NOTICES]).toEqual(["linkFolder", "firstLoop", "agentHookWiring"]);
  });
});
