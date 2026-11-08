// When each run's terminal last printed, read off Rust's terminals (`./runOutputs`).
//
// **One reader, however many panes**: two listeners share one `pty_sessions` per beat. **It reads only
// while someone listens**, and stops when the last one goes.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PtySessionDto } from "../bindings/bindings";
import { OUTPUTS_EVERY_MS, hearOutputs, outputsOf } from "./runOutputs";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn<(cmd: string) => Promise<PtySessionDto[]>>() }));
vi.mock("../core/ipc", () => ({ invoke }));

const session = (more: Partial<PtySessionDto>): PtySessionDto => ({
  session: "s", folder: null, agent: null, ...more,
});

describe("outputsOf", () => {
  it("keeps the latest output of each run, and leaves out terminals with no run or no output", () => {
    const got = outputsOf([
      session({ run: 1, lastOutput: 100 }),
      session({ run: 1, lastOutput: 300 }),
      session({ run: 1, lastOutput: 200 }),
      session({ run: 2 }),
      session({ lastOutput: 500 }),
    ]);
    expect([...got]).toEqual([[1, 300]]);
  });
});

describe("hearOutputs", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invoke.mockReset();
    invoke.mockResolvedValue([session({ run: 7, lastOutput: 42 })]);
  });
  afterEach(() => vi.useRealTimers());

  it("reads once per beat for every listener, and not at all once the last one has gone", async () => {
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = hearOutputs(first);
    const stopSecond = hearOutputs(second);
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("pty_sessions");
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(OUTPUTS_EVERY_MS);
    expect(invoke, "two listeners read twice a beat").toHaveBeenCalledTimes(2);

    stopFirst();
    await vi.advanceTimersByTimeAsync(OUTPUTS_EVERY_MS);
    expect(invoke, "the reading stopped while one still listens").toHaveBeenCalledTimes(3);

    stopSecond();
    await vi.advanceTimersByTimeAsync(OUTPUTS_EVERY_MS * 3);
    expect(invoke, "the reading goes on with nobody listening").toHaveBeenCalledTimes(3);
  });
});
