import { describe, it, expect } from "vitest";
import { arrivalsToAnnounce } from "./mailbox";
import type { InboxItemBrief } from "./reads";

// The rule that decides which arrivals ring: unread ∧ not-yet-notified. The persistence around it
// (the notified set surviving restarts) is covered by core's overview round-trip; here we pin the predicate.
const item = (id: number, unread: boolean): InboxItemBrief => ({ id, unread });

describe("arrivalsToAnnounce — unread ∧ un-notified", () => {
  it("announces the unread items that are not yet in the notified set", () => {
    const items = [item(1, true), item(2, true), item(3, true)];
    expect(arrivalsToAnnounce(items, new Set([2]))).toEqual([1, 3]);
  });

  it("never announces a read item, even when it is not in the notified set", () => {
    const items = [item(1, false), item(2, true)];
    expect(arrivalsToAnnounce(items, new Set())).toEqual([2]);
  });

  it("stays silent when everything unread has already been notified (the steady state on relaunch)", () => {
    const items = [item(1, true), item(2, false)];
    expect(arrivalsToAnnounce(items, new Set([1]))).toEqual([]);
  });

  it("on a fresh store (empty notified set) announces every unread item once — the startup catch-up", () => {
    const items = [item(10, true), item(11, true), item(12, false)];
    expect(arrivalsToAnnounce(items, new Set())).toEqual([10, 11]);
  });

  it("is empty when the inbox is empty", () => {
    expect(arrivalsToAnnounce([], new Set([1, 2]))).toEqual([]);
  });
});
