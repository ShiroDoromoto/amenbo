// @vitest-environment jsdom
// The window's reading of which of its sessions have a turn standing in them (`./standing`).
//
// What is pinned here is the part neither reader can check for itself: that there is **one** answer,
// and that it is the host's. The row above a pane and the dots on the pages are the same turn drawn
// twice, and a turn taken down in one place while it still stands in the other is exactly the state a
// reader cannot make sense of — a pane they have answered, with a dot on its page saying it is still
// calling. Kept here rather than asked for, the arrival would go away with the webview and every
// answered turn would stand again in the next window to draw the session.
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PtySessionDto } from "../bindings/bindings";
import type { Turns } from "./standing";

const hoisted = vi.hoisted(() => ({
  /** What the host says is running, and which of those have a turn standing in them. */
  running: [] as PtySessionDto[],
  /** The sessions `pty_saw` was called for, in order. */
  saw: [] as string[],
}));

// No statements here: what moves the answer in this test is a person arriving at a pane, which is not
// an event but a call.
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("../core/ipc", () => ({
  invoke: async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "pty_sessions") return hoisted.running;
    if (cmd !== "pty_saw") return undefined;
    // The host is where the arrival is written, so the answer moves with it (`crate::pty::Pane`).
    const session = args!.session as string;
    hoisted.saw.push(session);
    hoisted.running = hoisted.running.map((one) =>
      one.session === session ? { ...one, waiting: null } : one);
    return undefined;
  },
}));

const { sawPane, watchStanding } = await import("./standing");

const stops: Array<() => void> = [];

/** Watch, and hand back what the watcher was last told. */
function watching(): { last: () => Turns } {
  let last: Turns = new Map();
  stops.push(watchStanding((turns) => { last = turns; }));
  return { last: () => last };
}

/** One session the host is holding, with a turn standing in it or without. */
const open = (session: string, waiting: string | null): PtySessionDto =>
  ({ session, startedAt: "2026-09-06T00:00:00Z", folder: null, waiting, cols: 80, rows: 24 });

/** Let the reads of the host land. */
const settled = async () => { for (let i = 0; i < 4; i++) await Promise.resolve(); };

afterEach(() => {
  while (stops.length) stops.pop()!();
  hoisted.running = [];
  hoisted.saw = [];
});

describe("the window's reading of the turns standing in its sessions", () => {
  it("answers a new watcher with what it already has, and every watcher with the same thing", async () => {
    // A pane opened after the turn came still has to see it — the reading outlives every reader,
    // which is the whole reason it is not kept in one.
    hoisted.running = [open("pane-1", "which of the two")];
    const first = watching();
    await settled();
    const second = watching();
    expect([...second.last()]).toEqual([["pane-1", "which of the two"]]);
    expect(second.last()).toBe(first.last());
  });

  it("takes the turn down where the person came, and leaves the others standing", async () => {
    hoisted.running = [open("pane-1", "which of the two"), open("pane-2", "and this")];
    const one = watching();
    await settled();

    sawPane("pane-1");
    await settled();
    expect(hoisted.saw, "the arrival was kept in the window instead of being said").toEqual(["pane-1"]);
    expect([...one.last().keys()]).toEqual(["pane-2"]);
  });

  it("says nothing about a session with no turn standing in it", async () => {
    hoisted.running = [open("pane-1", null)];
    const one = watching();
    await settled();
    const before = one.last();

    sawPane("pane-1");
    await settled();
    // The same answer: an arrival at a pane nobody is being called to moves nothing, and a watcher
    // woken for a change that did not happen would redraw every row for nothing.
    expect(one.last()).toBe(before);
  });
});
