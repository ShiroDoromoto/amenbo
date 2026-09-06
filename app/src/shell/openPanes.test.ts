// @vitest-environment jsdom
// What every way out of the app has to know before it asks about itself.
//
// Ending the app ends every session in the process at once, and so does the restart that applies an
// update (`./AppShell`, `../components/UpdateBanner`, `crate::quit`). What each of them asks first is
// whether there is a terminal to lose — a count, and nothing about what any of them was doing
// (`AMB-D-858`).
//
// The way it goes wrong is silent: a host that cannot answer looks exactly like a process with no
// pane open, and a way out that took a failed read for "nothing to lose" would go without asking.
// So the failure is pinned to zero deliberately, and it is the one reading here that is a choice
// rather than a count.
import { afterEach, describe, expect, it, vi } from "vitest";
import { openPanes } from "./openPanes";

const hoisted = vi.hoisted(() => ({
  /** The sessions the host says are open (`crate::pty::pty_sessions`). */
  running: [] as { session: string; startedAt: string; folder: string | null }[],
  /** Whether that read is refused. */
  refuse: false,
}));

vi.mock("../core/ipc", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "pty_sessions") {
      if (hoisted.refuse) throw new Error("the host could not answer");
      return hoisted.running;
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
  hoisted.refuse = false;
});

describe("what a way out of the app is about to lose", () => {
  it("is every pane in the process, not one window's", async () => {
    open("a", "b", "c");
    expect(await openPanes()).toBe(3);
  });

  it("is nothing at all when no terminal is open", async () => {
    open();
    expect(await openPanes()).toBe(0);
  });

  it("is nothing when the host cannot answer", async () => {
    open("a");
    hoisted.refuse = true;
    expect(await openPanes()).toBe(0);
  });
});
