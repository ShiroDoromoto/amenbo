// @vitest-environment jsdom
// What the row above a pane is headed with, and where that comes from.
//
// **The pane's own name, and the folder until it has one.** A frame that has never been named is not
// a frame with nothing to call it: it is working somewhere, and where is a fact rather than a message
// from whatever is running in it (`AMB-D-748`). What is pinned here is that the folder gives way the
// moment a name arrives and never the other way round, and that a pane that has never had a terminal
// has no row at all — the face there is the invitation to choose a folder.
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../core/ipc", () => ({ invoke: async () => ({ holding: [], finished: 0 }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("./frames", async (orig) => ({
  ...(await orig<typeof import("./frames")>()),
  frameNames: async () => new Map<string, string>(),
}));

const { mountPlate } = await import("./plate");

const AT = "2026-08-24T09:00:00Z";

let host: HTMLElement;
let plate: ReturnType<typeof mountPlate>;

/** The row is put up again once the ledger has answered what the pane is holding, which is a
 *  question out over the boundary — so what it says is read after that has come back. */
const settled = () => new Promise((done) => setTimeout(done, 0));

/** What the row is headed with now, or null where there is no row. */
const heading = () => host.querySelector<HTMLElement>(".plate:not([hidden]) .plate__name")?.textContent ?? null;

beforeEach(() => {
  host = document.createElement("div");
  plate = mountPlate(host, () => "en");
});

afterEach(() => plate.stop());

describe("what the row above a pane is headed with", () => {
  it("says nothing about a frame no terminal has ever run in", () => {
    expect(heading()).toBeNull();
  });

  it("is the folder the terminal was started in", async () => {
    plate.opened("pane-1", AT, "/work/amenbo");
    await settled();
    expect(heading()).toBe("amenbo");
  });

  it("gives way to a name the moment there is one, and does not come back over it", async () => {
    plate.opened("pane-1", AT, "/work/amenbo");
    await settled();
    plate.named(new Map([["1", "the migration"]]));
    expect(heading()).toBe("the migration");
    // A naming that was refused answers with the name that stood (`./frames`), so the row never has
    // to work out which of two names is the frame's.
    plate.named(new Map([["1", "the migration"]]));
    expect(heading()).toBe("the migration");
  });

  it("is empty for a pane whose terminal was started nowhere in particular", async () => {
    plate.opened("pane-1", AT, null);
    await settled();
    expect(heading()).toBe("");
  });
});
