// Reading a session's tail back into a pane that has just taken it over.
//
// The host hands the tail over in runs, each carrying the size it was written at (`crate::pty::Recent`).
// What is pinned here is the order that makes those sizes worth carrying: the size goes in front of the
// bytes it belongs to, and the bytes are on the screen before the next size moves it. Get either wrong
// and the tail comes back folded where it was never written — which is a thing only a screen shows, so
// nothing else would catch it (`AMB-T-4516`, `AMB-T-4531`).
import { describe, expect, it } from "vitest";
import type { PtyReplayDto } from "../bindings/bindings";
import { replayTail } from "./terminal";

/** One run of a tail: a size, and the text to be read at it. */
function run(cols: number, rows: number, text: string): PtyReplayDto {
  return { cols, rows, base64: btoa(text) };
}

/** A terminal that keeps what it was told and in what order, and nothing else. */
function watching(): {
  term: Parameters<typeof replayTail>[0];
  seen: string[];
  /** Let the write the terminal is holding land. */
  land: () => void;
} {
  const seen: string[] = [];
  let holding: (() => void) | null = null;
  return {
    seen,
    land: () => {
      const go = holding;
      holding = null;
      go?.();
    },
    term: {
      resize: (cols: number, rows: number) => seen.push(`resize ${cols}x${rows}`),
      // The real one queues and calls back once the bytes are on the screen. Here the callback is
      // held so a test can say when that happened.
      write: (data: Uint8Array | string, done?: () => void) => {
        seen.push(`write ${new TextDecoder().decode(data as Uint8Array)}`);
        holding = () => done?.();
      },
    } as Parameters<typeof replayTail>[0],
  };
}

/** Run every write through as soon as it is made — the shape of a terminal that never keeps one. */
function prompt(): { term: Parameters<typeof replayTail>[0]; seen: string[] } {
  const seen: string[] = [];
  return {
    seen,
    term: {
      resize: (cols: number, rows: number) => seen.push(`resize ${cols}x${rows}`),
      write: (data: Uint8Array | string, done?: () => void) => {
        seen.push(`write ${new TextDecoder().decode(data as Uint8Array)}`);
        done?.();
      },
    } as Parameters<typeof replayTail>[0],
  };
}

describe("replayTail", () => {
  it("puts each run's size in front of that run's bytes", async () => {
    const { term, seen } = prompt();

    await replayTail(term, [run(110, 30, "wide"), run(26, 30, "narrow")]);

    expect(seen).toEqual(["resize 110x30", "write wide", "resize 26x30", "write narrow"]);
  });

  it("does not move the size until the bytes before it are on the screen", async () => {
    const { term, seen, land } = watching();

    const reading = replayTail(term, [run(110, 30, "wide"), run(26, 30, "narrow")]);
    await Promise.resolve();

    // The second run's size is the one that would fold the first run's bytes if it went first.
    expect(seen).toEqual(["resize 110x30", "write wide"]);

    land();
    await Promise.resolve();
    expect(seen).toEqual(["resize 110x30", "write wide", "resize 26x30", "write narrow"]);

    land();
    await reading;
  });

  it("leaves the terminal at the size it found it at where a run carries none", async () => {
    const { term, seen } = prompt();

    await replayTail(term, [run(0, 0, "sizeless")]);

    expect(seen).toEqual(["write sizeless"]);
  });

  it("writes nothing for a session that has written nothing", async () => {
    const { term, seen } = prompt();

    await replayTail(term, []);

    expect(seen).toEqual([]);
  });
});
