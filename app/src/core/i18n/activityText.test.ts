// The wording of a timeline line, a relative time and a due chip. The backend sends the kind and
// the values and no prose at all, so everything a reader sees on these three surfaces is written
// here — which makes this the only place the wording can be pinned, in both languages.
import { describe, expect, it, vi } from "vitest";

vi.mock("../snapshot", () => ({ getSnapshot: () => ({ language: "ja", dateLocale: null }) }));

import { agoLabel, dueLabel, eventText, targetTitle } from "./index";

const NOW = new Date("2026-06-21T12:00:00Z").getTime();
const ago = (secs: number) => new Date(NOW - secs * 1000).toISOString();

describe("a system event as a line", () => {
  it("names the status a task moved to, not merely that it moved", () => {
    const ev = { kind: "task.status_changed", status: "done" };
    expect(eventText(ev, "配色チェック", "ja")).toBe("「配色チェック」を完了に変更");
    expect(eventText(ev, "配色チェック", "en")).toBe("Changed “配色チェック” to Done");
  });

  it("tells an assignment from a delegation from a withdrawal", () => {
    expect(eventText({ kind: "task.assigned", toKind: "ai" }, "X", "en")).toBe("Delegated “X” to AI");
    expect(eventText({ kind: "task.assigned", toKind: "human" }, "X", "en")).toBe("Assigned “X”");
    // No facet is not a missing value here — it is the sentence saying the assignee was taken away.
    expect(eventText({ kind: "task.assigned" }, "X", "en")).toBe("Unassigned “X”");
  });

  it("counts what went with a deleted project, and says nothing of a count of none", () => {
    const ev = { kind: "project.deleted", tasks: 4, decisions: 1 };
    expect(eventText(ev, "旧サイト", "ja")).toBe("「旧サイト」を削除（タスク4件・決定1件）");
    // The singular is not cosmetic: "1 decisions" is the tell of a label built by rote.
    expect(eventText(ev, "旧サイト", "en")).toBe("Deleted “旧サイト” (4 tasks, 1 decision)");

    const empty = { kind: "project.deleted", tasks: 0, decisions: 0 };
    expect(eventText(empty, "空の PJ", "ja")).toBe("「空の PJ」を削除");
    expect(eventText(empty, "空の PJ", "en")).toBe("Deleted “空の PJ”");
  });

  it("reports a deletion as a deletion, whichever kind of row was deleted", () => {
    expect(eventText({ kind: "task.deleted" }, "下書き", "ja")).toBe("「下書き」を削除");
    expect(eventText({ kind: "decision.deleted" }, "旧方針", "ja")).toBe("「旧方針」を削除");
  });

  // A newer core can emit a kind this build has never heard of. Falling to the generic line keeps
  // the row on screen; showing nothing would lose the fact that anything happened at all.
  it("falls to the generic line for a kind it does not know", () => {
    expect(eventText({ kind: "task.hatched" }, "X", "en")).toBe("Updated “X”");
  });

  it("puts a stand-in where a target's name is past recovering", () => {
    expect(targetTitle("", "ja")).toBe("（削除済み）");
    expect(targetTitle("", "en")).toBe("(deleted)");
    expect(targetTitle("生きているタスク", "ja")).toBe("生きているタスク");
    expect(eventText({ kind: "task.deleted" }, "", "en")).toBe("Deleted “(deleted)”");
  });
});

describe("how long ago", () => {
  it("words the gap in the largest unit that fits", () => {
    expect(agoLabel(ago(5), "en", NOW)).toBe("just now");
    expect(agoLabel(ago(60), "en", NOW)).toBe("1 minute ago");
    expect(agoLabel(ago(120), "en", NOW)).toBe("2 minutes ago");
    expect(agoLabel(ago(3600), "en", NOW)).toBe("1 hour ago");
    expect(agoLabel(ago(86_400 * 3), "en", NOW)).toBe("3 days ago");
    expect(agoLabel(ago(5), "ja", NOW)).toBe("たった今");
    expect(agoLabel(ago(120), "ja", NOW)).toBe("2分前");
    expect(agoLabel(ago(86_400 * 3), "ja", NOW)).toBe("3日前");
  });

  // A row written a moment ago can carry a timestamp a hair ahead of this clock. "in -0 minutes"
  // would be the tell; the gap is floored at zero instead.
  it("does not run backwards on a timestamp from just ahead", () => {
    expect(agoLabel(new Date(NOW + 2000).toISOString(), "en", NOW)).toBe("just now");
  });
});

describe("the due chip", () => {
  // Local noon, so the day is the same one whatever timezone the test runs in.
  const today = new Date(2026, 5, 21, 12, 0, 0);

  it("counts whole calendar days, so tomorrow is the next date", () => {
    expect(dueLabel("2026-06-21", "en", today)).toBe("Today");
    expect(dueLabel("2026-06-22", "en", today)).toBe("Tomorrow");
    expect(dueLabel("2026-06-20", "en", today)).toBe("Yesterday");
    expect(dueLabel("2026-06-23", "en", today)).toBe("In 2 days");
    expect(dueLabel("2026-06-18", "en", today)).toBe("3 days ago");
    expect(dueLabel("2026-06-21", "ja", today)).toBe("今日");
    expect(dueLabel("2026-06-23", "ja", today)).toBe("2日後");
    expect(dueLabel("2026-06-18", "ja", today)).toBe("3日前");
  });

  // The chip colours by the day alone (`dueKind`); the wording has to cut the same way, or a date
  // carrying a time would read "tomorrow" under a chip coloured for today.
  it("judges by the day even when a time is attached", () => {
    expect(dueLabel("2026-06-21T23:00:00Z", "en", today)).toBe("Today");
  });
});
