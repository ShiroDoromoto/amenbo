// @vitest-environment jsdom
// How long a run's step has run, as the plate keeps it on the row: counted on the clock while the run
// is under way, and held where it was once the run has stopped — a pane outlives its step.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { tf } from "../core/i18n";
import type { Say } from "./nameplate";

vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("./frames", async (orig) => ({
  ...(await orig<typeof import("./frames")>()),
  frameNames: async () => new Map<string, string>(),
}));

const { mountPlate } = await import("./plate");

const AT = Date.parse("2026-10-05T04:00:00Z");
const STATE = {
  word: "", why: null, exit: null, errorExit: false, acknowledged: false,
  pauseRequested: false, pauseBeforeNextTask: false, pausableBeforeNextTask: false,
};
const RUN: Say = {
  automation: "Nightly", run: 15, step: "check", automationId: 3, placement: 2, box: 2, builtin: false,
  interactive: false, by: { kind: "script" }, command: "/bin/true", startedAt: AT, action: null, task: null,
  state: { ...STATE, status: "running" },
};

let host: HTMLElement;
let plate: ReturnType<typeof mountPlate>;
const elapsed = () => host.querySelector(".plate__elapsed")?.textContent;

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(AT + 5_000);
  host = document.createElement("div");
});

afterEach(() => {
  plate.stop();
  vi.useRealTimers();
});

describe("the time a run's step has run", () => {
  it("counts on while the run is under way, and holds once it has stopped", () => {
    plate = mountPlate(host, "frame", 199, RUN);
    expect(elapsed()).toBe(tf("face.elapsed", { time: "0:05" }));

    vi.advanceTimersByTime(60_000);
    expect(elapsed()).toBe(tf("face.elapsed", { time: "1:05" }));

    plate.stated({ ...STATE, status: "completed" });
    vi.advanceTimersByTime(60_000);
    expect(elapsed()).toBe(tf("face.elapsed", { time: "1:05" }));
  });

  it("starts counting once a built-in's card says when it started", () => {
    plate = mountPlate(host, "frame", 199, { ...RUN, startedAt: null });
    expect(elapsed()).toBe("");

    plate.timed(AT);
    vi.advanceTimersByTime(1_000);
    expect(elapsed()).toBe(tf("face.elapsed", { time: "0:06" }));
  });
});
