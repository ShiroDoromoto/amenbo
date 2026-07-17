// Division of labour: the watcher lives on the Tauri host (the mtime signature in
// `app/src-tauri/src/commands.rs`) and emits nothing but a payload-free wake-up. What changed is the
// change feed's word, and the frontend folds that into scopes to invalidate in a targeted way
// (`core/changes`; a fold that fails becomes a gap). This file exercises the receiving end — the
// full re-read on gap→reconcile, and the coalescing of overlapping wake-ups. The dataset→scope table
// is covered in `changes.test.ts`.
import { afterEach, describe, it, expect } from "vitest";

import {
  applySnapshot,
  getInboxDataGeneration,
  getLastReconcile,
  getSnapshot,
  notifyDataChanged,
  notifyInboxChanged,
  reconcile,
  subscribe,
} from "./snapshot";

describe("reconcile — coalescing multiple wake-ups", () => {
  it("four wake-ups in a row collapse to two source-of-truth re-reads: the first plus one trailing (no thrash)", async () => {
    // Every loadSnapshot fires exactly one notification, so counting them counts the re-reads.
    let notifies = 0;
    const unsub = subscribe(() => {
      notifies++;
    });
    try {
      // Four wake-ups land at once: the first goes in flight, the other three join as pending.
      await Promise.all([
        reconcile("focus"),
        reconcile("manual"),
        reconcile("gap"),
        reconcile("focus"),
      ]);
      // The first pass plus one trailing pass for everything that arrived during it: exactly two, not four.
      expect(notifies).toBe(2);
      // The last reason is the last wake-up's, which is what observability reads.
      expect(getLastReconcile()?.reason).toBe("focus");
    } finally {
      unsub();
    }
  });

  it("a single wake-up runs exactly once (no trailing pass)", async () => {
    let notifies = 0;
    const unsub = subscribe(() => {
      notifies++;
    });
    try {
      await reconcile("gap");
      expect(notifies).toBe(1);
      expect(getLastReconcile()?.reason).toBe("gap");
    } finally {
      unsub();
    }
  });
});

describe("getInboxDataGeneration — suppressing redundant mailbox recomputation", () => {
  const restore = getSnapshot();
  afterEach(() => applySnapshot(restore));

  it("swapping in real data (applySnapshot) advances the generation", () => {
    const before = getInboxDataGeneration();
    applySnapshot({ ...restore, meUserId: "someone-else" });
    expect(getInboxDataGeneration()).toBe(before + 1);
  });

  it("an archive change (notifyInboxChanged) advances the generation (membership shrinks, so recount)", () => {
    const before = getInboxDataGeneration();
    notifyInboxChanged();
    expect(getInboxDataGeneration()).toBe(before + 1);
  });

  it("a read-state change (notifyDataChanged) does not advance the generation (membership is independent of read state, so no recompute)", () => {
    const before = getInboxDataGeneration();
    notifyDataChanged();
    notifyDataChanged();
    expect(getInboxDataGeneration()).toBe(before);
  });
});
