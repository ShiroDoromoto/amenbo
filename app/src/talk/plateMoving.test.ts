// @vitest-environment jsdom
// The dot on the row, as the plate drives it. What is pinned here is the shape of the answer rather
// than the mark: a stream that keeps arriving is one state and not a hundred, a pane that quietens
// settles on the clock because nothing else will ever say so, and a pane whose program has exited is
// still — the stream did not go quiet, it ended.
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { STILL_AFTER_MS } from "./moving";

// The two boundaries the plate reaches across. Neither has anything to say about a stream.
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

/** How the row is drawn now: "yes" while the pane is moving. */
const dot = () => host.querySelector<HTMLElement>(".plate__dot")!.dataset.moving;

beforeEach(() => {
  vi.useFakeTimers();
  host = document.createElement("div");
  plate = mountPlate(host, () => "en");
  plate.opened("pane-1", AT);
});

afterEach(() => {
  plate.stop();
  vi.useRealTimers();
});

describe("the dot follows the stream and reads nothing else into it", () => {
  it("is still on a pane that has printed nothing", () => {
    expect(dot()).toBe("no");
  });

  it("moves on the first chunk", () => {
    plate.output();
    expect(dot()).toBe("yes");
  });

  it("bridges the gaps inside one piece of work rather than flickering through them", () => {
    plate.output();
    // A compiler between files, an agent between tool calls: quiet, but not stopped.
    vi.advanceTimersByTime(STILL_AFTER_MS - 1);
    plate.output();
    vi.advanceTimersByTime(STILL_AFTER_MS - 1);
    expect(dot()).toBe("yes");
  });

  it("settles on the clock, because stopping is the absence of an event", () => {
    plate.output();
    vi.advanceTimersByTime(STILL_AFTER_MS);
    expect(dot()).toBe("no");
  });

  it("goes still when the program exits, whatever the last chunk's clock says", () => {
    plate.output();
    plate.closed("pane-1");
    expect(dot()).toBe("no");
  });

  it("does not restart the turn on a pane that is already moving — four panes beat together", () => {
    plate.output();
    const delay = host.querySelector<HTMLElement>(".plate__dot")!.style.animationDelay;
    vi.advanceTimersByTime(100);
    plate.output();
    plate.output();
    expect(host.querySelector<HTMLElement>(".plate__dot")!.style.animationDelay).toBe(delay);
  });
});
