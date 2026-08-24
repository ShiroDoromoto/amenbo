// @vitest-environment jsdom
// What the ledger reads to mark the rows nothing is working on.
//
// The judgement itself is the host's — the store says what is reserved and still proposed, this process says
// which panes it started are still running — and what is tested here is the seam: the host answers in ids
// because the face asking already holds the rows, and those ids have to reach the face as something it can
// look a row up in.
//
// **The browser iteration loop has no panes**, so it has none that have gone, and the answer there is nothing
// rather than a fixture: a mark drawn from made-up data would be a mark nobody could act on.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AdriftDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  tauri: true,
  answer: { tasks: [] as number[], decisions: [] as number[] } as AdriftDto,
  /** What the host was asked, so the argument name the command takes is held to as well. */
  asked: [] as Record<string, unknown>[],
}));

vi.mock("./snapshot", () => ({ inTauri: () => hoisted.tauri }));
vi.mock("./ipc", () => ({
  invoke: (_cmd: string, args: Record<string, unknown>) => {
    hoisted.asked.push(args);
    return Promise.resolve(hoisted.answer);
  },
}));

const { fetchAdrift } = await import("./adrift");

beforeEach(() => {
  hoisted.tauri = true;
  hoisted.answer = { tasks: [], decisions: [] };
  hoisted.asked = [];
});

afterEach(() => vi.restoreAllMocks());

describe("what nothing is working on", () => {
  it("comes back as sets a row can be looked up in, with the two kinds apart", async () => {
    hoisted.answer = { tasks: [11, 13], decisions: [21] };
    const adrift = await fetchAdrift(7);

    expect(adrift.tasks.has(11)).toBe(true);
    expect(adrift.tasks.has(12)).toBe(false);
    // A task and a decision are two numbering spaces, so an id in one says nothing about the other.
    expect(adrift.tasks.has(21)).toBe(false);
    expect(adrift.decisions.has(21)).toBe(true);
  });

  it("asks about one project", async () => {
    await fetchAdrift(7);
    expect(hoisted.asked).toEqual([{ project: 7 }]);
  });

  it("answers with nothing outside the app, without asking", async () => {
    hoisted.tauri = false;
    const adrift = await fetchAdrift(7);

    expect(adrift.tasks.size).toBe(0);
    expect(adrift.decisions.size).toBe(0);
    expect(hoisted.asked).toEqual([]);
  });
});
