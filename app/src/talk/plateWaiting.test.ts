// @vitest-environment jsdom
// The one thing the plate says to somebody other than the reader looking at it: a turn is standing in
// this pane. The board wears it as a badge on the face switch, where the label itself cannot be seen
// (`AMB-D-753`), so what is pinned here is that it follows `waiting` and not the pane's chatter — an
// agent at work says a great deal — and that the pane going away takes the turn with it.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";

// The two boundaries the plate reaches across. Neither has anything to say about a turn standing.
vi.mock("../core/ipc", () => ({ invoke: async () => ({ holding: [], finished: 0 }) }));
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
  it("says it once, and says it is over when the agent goes back to work", () => {
    plate.opened("pane-1", AT, null);
    plate.said(say({ verb: "note", text: "running the tests" }));
    expect(told, "a pane merely working was reported as a turn").toEqual([]);

    plate.said(say({ verb: "waiting", text: "which of the two" }));
    expect(told).toEqual([true]);

    // The turn stands while nobody has answered it, and the pane keeps talking.
    plate.said(say({ verb: "waiting", text: "still which of the two" }));
    expect(told, "the same turn was reported twice").toEqual([true]);

    plate.said(say({ verb: "note", text: "on it" }));
    expect(told).toEqual([true, false]);
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

    plate.said(say({ verb: "note", text: "reading the store" }));
    expect(told, "the agent spoke Amenbo's own words and was still called unsent").toEqual([true, false]);
  });

  it("says nothing at all to a pane nobody is waiting on", () => {
    plate.opened("pane-1", AT, null);
    plate.said(say({ verb: "finished", text: "it landed" }));
    plate.closed("pane-1");
    plate.stop();
    expect(told).toEqual([]);
  });
});
