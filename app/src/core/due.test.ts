import { describe, expect, it } from "vitest";
import { DUE_OVERDUE, DUE_TODAY, DUE_TOMORROW, DUE_WINDOWS, dueBadges } from "./due";

describe("the due row's two steps", () => {
  it("draws the step that wants a hand ahead of the one that is only coming", () => {
    expect(dueBadges({ stop: 3, heed: 2 })).toEqual([
      { step: "stop", count: 3 },
      { step: "heed", count: 2 },
    ]);
  });

  it("leaves out a step with nothing on it, so a quiet day shows no colour at all", () => {
    expect(dueBadges({ stop: 0, heed: 2 })).toEqual([{ step: "heed", count: 2 }]);
    expect(dueBadges({ stop: 1, heed: 0 })).toEqual([{ step: "stop", count: 1 }]);
    expect(dueBadges({ stop: 0, heed: 0 })).toEqual([]);
  });
});

describe("the windows the badge counts and the row opens", () => {
  // The badge counting one set while the row opens another is the failure this view exists to avoid,
  // so the list's windows are held to being exactly the ones the two steps are summed from.
  it("are the three the steps are built from, soonest deadline first", () => {
    expect(DUE_WINDOWS).toEqual([DUE_OVERDUE, DUE_TODAY, DUE_TOMORROW]);
  });

  it("leave closed work out, whether it was carried out or decided against", () => {
    for (const w of DUE_WINDOWS) expect(w).toContain("done:false");
  });
});
