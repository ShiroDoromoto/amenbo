// @vitest-environment jsdom
// The window's record of which of its sessions have a turn standing in them (`./standing`).
//
// What is pinned here is the part neither reader can check for itself: that there is **one** record.
// The row above a pane and the dots on the pages are the same turn drawn twice, and a turn taken down
// in one place while it still stands in the other is exactly the state a reader cannot make sense of
// — a pane they have answered, with a dot on its page saying it is still calling.
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PtySessionDto } from "../bindings/bindings";
import type { Turns } from "./standing";

const hoisted = vi.hoisted(() => ({
  /** What the host says is running, and which of those have a turn declared in them. */
  running: [] as PtySessionDto[],
}));

// No statements here: what moves the answer in this test is `sawPane`, which is the half of it that
// is not an event.
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("../core/ipc", () => ({
  invoke: async (cmd: string) => (cmd === "pty_sessions" ? hoisted.running : undefined),
}));

const { sawPane, watchStanding } = await import("./standing");

const stops: Array<() => void> = [];

/** Watch, and hand back what the watcher was last told. */
function watching(): { last: () => Turns } {
  let last: Turns = { standing: new Set(), seen: new Map() };
  stops.push(watchStanding((turns) => { last = turns; }));
  return { last: () => last };
}

/** One session the host is holding, with a turn declared in it or without. */
const open = (session: string, waiting: string | null): PtySessionDto =>
  ({ session, startedAt: "2026-09-06T00:00:00Z", folder: null, waiting });

afterEach(() => {
  while (stops.length) stops.pop()!();
  hoisted.running = [];
});

describe("the window's record of the turns standing in its sessions", () => {
  it("answers a new watcher with what it already has", async () => {
    // A pane opened after the turn came still has to see it — the record outlives every reader, which
    // is the whole reason it is not kept in one.
    hoisted.running = [open("pane-1", "which of the two")];
    const first = watching();
    await Promise.resolve();
    const second = watching();
    expect([...second.last().standing]).toEqual(["pane-1"]);
    expect(second.last()).toBe(first.last());
  });

  it("takes the turn down when the person comes to that pane, and leaves the others standing", async () => {
    hoisted.running = [open("pane-1", "which of the two"), open("pane-2", "and this")];
    const one = watching();
    await Promise.resolve();

    sawPane("pane-1");
    expect([...one.last().standing]).toEqual(["pane-2"]);
    expect(one.last().seen.has("pane-1")).toBe(true);
  });

  it("says nothing about a session with no turn declared in it", async () => {
    hoisted.running = [open("pane-1", null)];
    const one = watching();
    await Promise.resolve();
    const before = one.last();
    sawPane("pane-1");
    sawPane("pane-nobody-opened");
    // The same answer: an arrival at a pane nobody is being called to records nothing, and a watcher
    // woken for a change that did not happen would redraw every row for nothing.
    expect(one.last()).toBe(before);
  });
});
