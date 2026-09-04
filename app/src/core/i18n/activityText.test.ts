// The wording of a timeline line. The backend sends the kind and the values and no prose at all, so
// everything a reader sees on this surface is written here — which makes this the only place the
// wording can be pinned, in both languages. (A relative time and a due chip come off the same bare
// values, but no dictionary words them: they are `Intl`'s, and format.test.ts holds them.)
import { describe, expect, it, vi } from "vitest";

vi.mock("../snapshot", () => ({ getSnapshot: () => ({ language: "ja", dateLocale: null }) }));

import { eventText, targetTitle } from "./index";

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

  // A proposal is the one thing about a decision the ledger holds and the columns cannot: what
  // `status` says is that a decision *is* proposed, not that anybody put it up (`AMB-T-3639`).
  it("says a decision was put up, in its own words rather than the generic line", () => {
    expect(eventText({ kind: "decision.proposed" }, "どちらの道を採るか", "ja")).toBe("「どちらの道を採るか」を提案");
    expect(eventText({ kind: "decision.proposed" }, "Which road", "en")).toBe("Proposed “Which road”");
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
