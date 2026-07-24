import { describe, expect, it } from "vitest";
import { activityRowKey, dedupActivityRows } from "./activity";
import type { ActivityItem } from "../mock/types";

// The timeline merges sources that number independently (`AMB-D-388`), so two rows can carry the same
// id. These tests hold the identity to the pair that survives that.
const row = (seq: number, id: number, text: string): ActivityItem =>
  ({ seq, id, kind: "comment", text } as unknown as ActivityItem);

describe("activityRowKey", () => {
  it("tells apart two rows that share an id across sequences", () => {
    expect(activityRowKey(row(0, 42, "on a task"))).not.toBe(activityRowKey(row(1, 42, "on a decision")));
    expect(activityRowKey(row(1, 42, "a"))).toBe(activityRowKey(row(1, 42, "the same row")));
  });
});

describe("dedupActivityRows", () => {
  // The bug this closes: the collided row was dropped as a repeat, so it vanished from the feed the
  // moment an older page was loaded.
  it("keeps both rows of an id collision, and drops a true repeat", () => {
    const onTask = row(0, 42, "on a task");
    const onDecision = row(1, 42, "on a decision");
    expect(dedupActivityRows([onTask, onDecision])).toEqual([onTask, onDecision]);

    expect(dedupActivityRows([onTask, onDecision, onTask])).toEqual([onTask, onDecision]);
  });

  it("keeps the first of a repeat, in order", () => {
    const rows = [row(0, 3, "c"), row(0, 2, "b"), row(0, 3, "again"), row(0, 1, "a")];
    expect(dedupActivityRows(rows).map((r) => r.text)).toEqual(["c", "b", "a"]);
  });
});
