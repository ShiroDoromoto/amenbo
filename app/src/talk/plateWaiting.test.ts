// @vitest-environment jsdom
// The one thing the plate says to somebody other than the reader looking at it: a turn is standing in
// this pane. The board wears it as a badge on the face switch, where the label itself cannot be seen
// (`AMB-D-753`), so what is pinned here is that it follows `waiting` and not the pane's chatter — an
// agent at work says a great deal — and that the pane going away takes the turn with it.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";

// The one boundary the plate reaches across. It has nothing to say about a turn standing.
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("./frames", async (orig) => ({
  ...(await orig<typeof import("./frames")>()),
  frameNames: async () => new Map<string, string>(),
}));

const { mountPlate } = await import("./plate");

const AT = "2026-08-24T09:00:00Z";
const say = (over: Partial<SessionSaidDto> & Pick<SessionSaidDto, "verb">): SessionSaidDto =>
  ({ session: "pane-1", at: AT, ...over });

let told: boolean[];
let plate: ReturnType<typeof mountPlate>;

beforeEach(() => {
  told = [];
  plate = mountPlate(document.createElement("div"), () => "en", (w) => told.push(w));
});

describe("what the plate says about a turn standing in its pane", () => {
  it("says it once, and goes on saying it whatever the agent says next", () => {
    plate.opened("pane-1", AT, null);
    plate.said(say({ verb: "name", text: "the migration" }));
    expect(told, "a pane merely naming itself was reported as a turn").toEqual([]);

    plate.said(say({ verb: "waiting", text: "which of the two" }));
    expect(told).toEqual([true]);

    // The turn stands while nobody has come to the pane, and the pane keeps talking.
    plate.said(say({ verb: "waiting", text: "still which of the two" }));
    expect(told, "the same turn was reported twice").toEqual([true]);

    // **No word takes it back** (`AMB-D-859`). What ends one is the person arriving, which the window
    // sees and says (`./standing`).
    plate.said(say({ verb: "name", text: "still the migration" }));
    expect(told).toEqual([true]);
  });

  it("takes the turn away when the program in the terminal exits", () => {
    plate.opened("pane-1", AT, null);
    plate.said(say({ verb: "waiting", text: "which of the two" }));
    plate.closed("pane-1");
    expect(told, "the pane ended and the badge was left standing").toEqual([true, false]);
  });

  it("takes the turn away when the label itself comes down", () => {
    plate.opened("pane-1", AT, null);
    plate.said(say({ verb: "waiting", text: "which of the two" }));
    plate.stop();
    expect(told, "the face went and the badge was left standing").toEqual([true, false]);
  });

  it("calls a person for a sentence left in the input box, and stops once the pane speaks", () => {
    // Nothing at all happens in this pane until somebody presses Enter, so the badge is owed it the
    // same way it is owed a turn the agent handed over (`AMB-D-805`).
    plate.opened("pane-1", AT, null);
    plate.unsent("pane-1");
    expect(told).toEqual([true]);

    plate.said(say({ verb: "name", text: "the migration" }));
    expect(told, "the agent spoke Amenbo's own words and was still called unsent").toEqual([true, false]);
  });

  it("stops calling once the sentence has gone, whether or not the pane ever speaks", () => {
    // The press that sends it is the reader's own, and it is the last thing they are needed for here.
    // Waiting for the agent's word instead would leave the badge standing against a program that
    // never says one.
    plate.opened("pane-1", AT, null);
    plate.unsent("pane-1");
    expect(told).toEqual([true]);

    plate.sent("pane-1");
    expect(told, "there is nothing left in the box to press Enter for").toEqual([true, false]);
  });

  it("comes back up saying the turn that was handed over while it was away", () => {
    // A page turn takes the pane down, and an agent that hands its turn over then says it to a pane
    // that is not there. The host kept it, because the session outlives the pane drawing it
    // (`AMB-D-860`) — so the row that comes back is the one that left, and not an empty one.
    plate.opened("pane-1", AT, null, "which of the two");
    expect(told).toEqual([true]);

    plate.closed("pane-1");
    expect(told).toEqual([true, false]);
  });

  it("says nothing at all to a pane nobody is waiting on", () => {
    plate.opened("pane-1", AT, null);
    plate.said(say({ verb: "name", text: "the migration" }));
    plate.closed("pane-1");
    plate.stop();
    expect(told).toEqual([]);
  });
});
