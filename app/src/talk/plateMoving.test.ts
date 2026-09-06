// @vitest-environment jsdom
// The lamp on the row, as the plate drives it. What is pinned here is the shape of the answer rather
// than the mark: a stream that keeps arriving is one state and not a hundred, a pane that quietens
// settles on the clock because nothing else will ever say so, and a pane whose program has exited is
// out — the stream did not go quiet, it ended. And over all of it, the face that calls somebody: it
// wins wherever a turn is standing, whatever the stream is doing.
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import type { SessionSaidDto } from "../bindings/bindings";
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

/** Which of the lamp's three faces the row is drawn on now. */
const dot = () => host.querySelector<HTMLElement>(".plate__dot")!.dataset.face;
const row = () => host.querySelector<HTMLElement>(".plate")!;

const said = (over: Partial<SessionSaidDto> & Pick<SessionSaidDto, "verb">): SessionSaidDto =>
  ({ session: "pane-1", at: AT, ...over });

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

  it("does not move for a pane that is merely printing — the lit face is a glow held still", () => {
    plate.output();
    // Nothing is animating, so nothing has a phase to be put in step with.
    expect(row().style.getPropertyValue("--phase")).toBe("");
  });
});

describe("the lamp calls when a turn is standing, over whatever the stream is doing", () => {
  it("blinks on the turn and goes back to the stream when the agent does", () => {
    plate.output();
    expect(dot()).toBe("lit");

    plate.said(said({ verb: "waiting", text: "which of the two" }));
    expect(dot(), "a turn was standing and the lamp still reported the stream").toBe("calling");

    plate.said(said({ verb: "note", text: "on it" }));
    expect(dot()).toBe("lit");
  });

  it("calls on a pane that has printed nothing at all", () => {
    // The turn does not come off the stream, so it does not need one.
    plate.said(said({ verb: "waiting", text: "which of the two" }));
    expect(dot()).toBe("calling");
  });

  it("joins the blink where every other pane already is", () => {
    plate.said(said({ verb: "waiting", text: "which of the two" }));
    const phase = row().style.getPropertyValue("--phase");
    expect(phase, "a blinking row was left without a phase to fall in with").not.toBe("");

    // Still the same turn: setting it again would start the blink over, which is the one thing the
    // shared phase exists to prevent.
    vi.advanceTimersByTime(100);
    plate.said(said({ verb: "waiting", text: "still which of the two" }));
    expect(row().style.getPropertyValue("--phase")).toBe(phase);
  });
});
