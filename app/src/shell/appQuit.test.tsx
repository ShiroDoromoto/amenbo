// @vitest-environment jsdom
// What the end of the app has to know before it asks about itself.
//
// Ending the app ends every session in the process at once, so the question it raises is about all
// of them together (`./HoldingAsk`, `crate::quit`). Gathering that is the one step with anything in
// it: the reservations are written per session in the volatile area (`AMB-D-758`), and the box is
// handed one list.
//
// The two ways it goes wrong are both silent. A session that cannot be read looks exactly like a
// session holding nothing, and answering a failed read with a guess would raise a question about
// tasks nobody is losing. The other way is a name drawn twice — the same task in the list twice
// reads as two things being lost, from a reader who has no way to tell it is one.
import { afterEach, describe, expect, it, vi } from "vitest";
import { heldByAll } from "./HoldingAsk";

const hoisted = vi.hoisted(() => ({
  /** The sessions the host says are open (`crate::pty::pty_sessions`). */
  running: [] as { session: string; startedAt: string; folder: string | null }[],
  /** What the volatile area answers for each of them (`session_work`). */
  holding: {} as Record<string, number[]>,
  /** The sessions whose read is refused, which is what a store that cannot be opened does. */
  refuse: new Set<string>(),
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "pty_sessions") return hoisted.running;
    if (cmd === "session_work") {
      const session = String(args?.session);
      if (hoisted.refuse.has(session)) throw new Error("the store could not be opened");
      return { holding: hoisted.holding[session] ?? [], finished: 0 };
    }
    throw new Error(`unexpected command ${cmd}`);
  }),
}));

function open(...sessions: string[]) {
  hoisted.running = sessions.map((session) => ({
    session,
    startedAt: "2026-09-06T00:00:00Z",
    folder: null,
  }));
}

afterEach(() => {
  hoisted.running = [];
  hoisted.holding = {};
  hoisted.refuse = new Set();
});

describe("what the end of the app is about to lose", () => {
  it("is every session's reservations, not one pane's", async () => {
    open("a", "b", "c");
    hoisted.holding = { a: [12], b: [], c: [3, 9] };
    expect(await heldByAll()).toEqual([3, 9, 12]);
  });

  it("names a task once, however many sessions answer with it", async () => {
    open("a", "b");
    hoisted.holding = { a: [7], b: [7] };
    expect(await heldByAll()).toEqual([7]);
  });

  it("is nothing at all when no terminal is open", async () => {
    open();
    expect(await heldByAll()).toEqual([]);
  });

  // A read that failed cannot say something is being left behind, so it does not get to say it.
  // What it must not do is take the sessions that did answer down with it.
  it("keeps what the other sessions answered when one read is refused", async () => {
    open("a", "b");
    hoisted.holding = { b: [4] };
    hoisted.refuse = new Set(["a"]);
    expect(await heldByAll()).toEqual([4]);
  });
});
