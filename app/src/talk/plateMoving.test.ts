// @vitest-environment jsdom
// The lamp on the row, as the plate drives it. What is pinned here is the shape of the answer rather
// than the mark: a stream that keeps arriving is one state and not a hundred, a pane that quietens
// settles on the clock because nothing else will ever say so, and a pane whose program has exited is
// out — the stream did not go quiet, it ended.
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { STILL_AFTER_MS } from "./moving";

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

/** Which of the lamp's two faces the row is drawn on now. */
const dot = () => host.querySelector<HTMLElement>(".plate__dot")!.dataset.face;

beforeEach(() => {
  vi.useFakeTimers();
  host = document.createElement("div");
  plate = mountPlate(host);
  plate.opened("pane-1", AT, null);
});

afterEach(() => {
  plate.stop();
  vi.useRealTimers();
});

describe("the lamp follows the stream and reads nothing else into it", () => {
  it("is out on a pane that has printed nothing", () => {
    expect(dot()).toBe("out");
  });

  it("lights on the first chunk", () => {
    plate.output();
    expect(dot()).toBe("lit");
  });

  it("bridges the gaps inside one piece of work rather than flickering through them", () => {
    plate.output();
    // A compiler between files, an agent between tool calls: quiet, but not stopped.
    vi.advanceTimersByTime(STILL_AFTER_MS - 1);
    plate.output();
    vi.advanceTimersByTime(STILL_AFTER_MS - 1);
    expect(dot()).toBe("lit");
  });

  it("settles on the clock, because stopping is the absence of an event", () => {
    plate.output();
    vi.advanceTimersByTime(STILL_AFTER_MS);
    expect(dot()).toBe("out");
  });

  it("goes out when the program exits, whatever the last chunk's clock says", () => {
    plate.output();
    plate.closed("pane-1");
    expect(dot()).toBe("out");
  });

});
