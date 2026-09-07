// @vitest-environment jsdom
// The window's own record of what its sessions have said, and which of those turns a person has been
// to (`./spoken`).
//
// What is pinned here is the part neither reader can check for itself: that there is **one** record.
// The row above a pane and the dots on the pages are the same turn drawn twice, and a turn taken down
// in one place while it still stands in the other is exactly the state a reader cannot make sense of
// — a pane they have answered, with a dot on its page saying it is still calling.
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Sessions } from "./sessions";

// No host here: the map is moved by `sawPane` alone, which is the half of it that is not an event.
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

const { sawPane, watchSpoken } = await import("./spoken");

const stops: Array<() => void> = [];

/** Watch, and hand back what the watcher was last told. */
function watching(): { last: () => Sessions } {
  let last: Sessions = new Map();
  stops.push(watchSpoken((sessions) => { last = sessions; }));
  return { last: () => last };
}

afterEach(() => {
  while (stops.length) stops.pop()!();
});

describe("the window's record of its sessions", () => {
  it("answers a new watcher with what it already has", async () => {
    // A pane opened after the turn came still has to see it — the record outlives every reader, which
    // is the whole reason it is not kept in one.
    const first = watching();
    await Promise.resolve();
    sawPane("pane-1");
    expect(first.last().size).toBe(0);
  });

  it("tells every watcher the same thing", async () => {
    const a = watching();
    const b = watching();
    await Promise.resolve();
    expect(a.last()).toBe(b.last());
  });

  it("says nothing about a session it has never heard of", async () => {
    const one = watching();
    await Promise.resolve();
    const before = one.last();
    sawPane("pane-nobody-opened");
    // The same map: an arrival at a pane the window is not holding records nothing, and a watcher
    // woken for a change that did not happen would redraw every row for nothing.
    expect(one.last()).toBe(before);
  });
});
