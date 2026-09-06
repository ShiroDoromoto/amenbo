// @vitest-environment jsdom
// How long a pane has been quiet, as the plate decides whether to say it at all.
//
// The measurement itself is `./moving`'s and is pinned there. What is pinned here is the two things
// that keep it from becoming noise: it is said in the pane being worked in and nowhere else, and it
// never stands where something the session actually said would stand.
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { QUIET_AFTER_MS } from "./moving";

// The one boundary the plate reaches across. It has nothing to say about a stream.
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("./frames", async (orig) => ({
  ...(await orig<typeof import("./frames")>()),
  frameNames: async () => new Map<string, string>(),
}));

const { mountPlate } = await import("./plate");

const AT = "2026-08-24T09:00:00Z";

let host: HTMLElement;
let plate: ReturnType<typeof mountPlate>;

/** What the end of the row is saying. */
const say = () => host.querySelector<HTMLElement>(".plate__say")!.textContent;

/** A pane that printed something and then went quiet for longer than the line. */
function wentQuiet(): void {
  plate.output();
  vi.advanceTimersByTime(QUIET_AFTER_MS);
}

beforeEach(() => {
  vi.useFakeTimers();
  host = document.createElement("div");
  plate = mountPlate(host, () => "en");
  plate.opened("pane-1", AT, null);
});

afterEach(() => {
  plate.stop();
  vi.useRealTimers();
});

describe("saying how long a pane has been quiet", () => {
  it("is said in the pane being worked in", () => {
    plate.focused(true);
    wentQuiet();
    expect(say()).toContain("quiet for 8 min");
  });

  it("is not said in a pane nobody is working in — a screen of clocks is a screen nobody reads", () => {
    wentQuiet();
    expect(say()).toBe("");
  });

  it("stops being said the moment the pane stops being the one worked in", () => {
    plate.focused(true);
    wentQuiet();
    plate.focused(false);
    expect(say()).toBe("");
  });

  it("is not said while the silence is still an ordinary one", () => {
    plate.focused(true);
    plate.output();
    vi.advanceTimersByTime(QUIET_AFTER_MS - 60_000);
    expect(say()).toBe("");
  });

  it("never stands where something the session said would stand", () => {
    plate.focused(true);
    wentQuiet();
    expect(say()).toContain("quiet for");
    // A note is the agent talking about its own work, and it outranks a measurement of its silence.
    plate.said({ session: "pane-1", verb: "note", at: AT, text: "reading the migration" } as never);
    expect(say()).toBe("reading the migration");
  });

  it("keeps its reading true as the minutes pass, which nothing else would notice", () => {
    plate.focused(true);
    wentQuiet();
    expect(say()).toContain("8 min");
    vi.advanceTimersByTime(2 * 60_000);
    expect(say(), "the row went stale where nothing raises an event").toContain("10 min");
  });
});
