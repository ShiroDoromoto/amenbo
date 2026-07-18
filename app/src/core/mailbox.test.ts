import { describe, it, expect } from "vitest";
import { arrivalsToAnnounce } from "./mailbox";
import type { InboxItemBrief } from "./reads";

// The rule that decides which arrivals ring: (unread ∨ unseen) ∧ not-yet-notified — source D announces on its
// `unread` gate, source C on its `unseen` gate. The persistence around it (the notified set surviving restarts)
// is covered by core's overview round-trip; here we pin the predicate.
const d = (id: number, unread: boolean): InboxItemBrief => ({ id, unread, unseen: false });
const c = (id: number, unseen: boolean): InboxItemBrief => ({ id, unread: false, unseen });

describe("arrivalsToAnnounce — (unread ∨ unseen) ∧ un-notified", () => {
  it("announces the unread D items that are not yet in the notified set", () => {
    const items = [d(1, true), d(2, true), d(3, true)];
    expect(arrivalsToAnnounce(items, new Set([2]))).toEqual([1, 3]);
  });

  it("never announces a read D item, even when it is not in the notified set", () => {
    const items = [d(1, false), d(2, true)];
    expect(arrivalsToAnnounce(items, new Set())).toEqual([2]);
  });

  it("announces an unseen source-C item (no unread flag), and never a seen one", () => {
    const items = [c(1, true), c(2, false)];
    expect(arrivalsToAnnounce(items, new Set())).toEqual([1]);
  });

  it("gates each source on its own flag when C and D are mixed", () => {
    const items = [d(1, true), d(2, false), c(3, true), c(4, false)];
    expect(arrivalsToAnnounce(items, new Set())).toEqual([1, 3]);
  });

  it("respects the notified set for unseen C items too (no re-fire on relaunch)", () => {
    const items = [c(1, true), c(2, true)];
    expect(arrivalsToAnnounce(items, new Set([1]))).toEqual([2]);
  });

  it("stays silent when everything eligible has already been notified (the steady state on relaunch)", () => {
    const items = [d(1, true), d(2, false), c(3, true)];
    expect(arrivalsToAnnounce(items, new Set([1, 3]))).toEqual([]);
  });

  it("on a fresh store (empty notified set) announces every eligible item once — the startup catch-up", () => {
    const items = [d(10, true), c(11, true), d(12, false), c(13, false)];
    expect(arrivalsToAnnounce(items, new Set())).toEqual([10, 11]);
  });

  it("is empty when the inbox is empty", () => {
    expect(arrivalsToAnnounce([], new Set([1, 2]))).toEqual([]);
  });
});
