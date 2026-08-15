// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { ActivityFeed } from "./ActivityFeed";
import { StoreProvider } from "../store/store";
import { loadSnapshot } from "../core/snapshot";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const replied: number[] = [];
const repliedDecisions: number[] = [];
const openedTasks: number[] = [];
const openedDecisions: number[] = [];
const edited: Array<[number, number]> = [];

// The reply button carries a mark rather than a character, so it is found by which mark it drew.
const replyButtons = () =>
  Array.from(container.querySelectorAll("button")).filter((b) => b.querySelector('svg[data-icon="reply"]'));
const targetButtons = () =>
  Array.from(container.querySelectorAll("button")).filter((b) => b.textContent?.startsWith("→"));

// Outside Tauri, so the snapshot comes from the mock fixtures: one comment plus a system row.
beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  replied.length = 0;
  repliedDecisions.length = 0;
  openedTasks.length = 0;
  openedDecisions.length = 0;
  edited.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  act(() =>
    root.render(
      createElement(
        StoreProvider,
        null,
        createElement(ActivityFeed, {
          onOpenTask: (id: number) => openedTasks.push(id),
          onOpenDecision: (id: number) => openedDecisions.push(id),
          onReplyToTask: (id: number) => replied.push(id),
          onReplyToDecision: (id: number) => repliedDecisions.push(id),
          onEditComment: (taskId: number, commentId: number) => edited.push([taskId, commentId]),
          onEditDecisionComment: () => {},
        }),
      ),
    ),
  );
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("ActivityFeed target buttons", () => {
  it("a row aimed at a live task opens that task", () => {
    const b = targetButtons().find((x) => x.textContent?.includes("ワイヤーフレーム作成"));
    act(() => b!.click());
    expect(openedTasks).toEqual([1]);
  });

  it("a row aimed at a deleted task (task.deleted) is not a clickable button", () => {
    expect(targetButtons().some((x) => x.textContent?.includes("重複していた下書き"))).toBe(false);
    expect(container.textContent).toContain("→ 重複していた下書き");
  });

  it("a row aimed at a project (project.deleted) is not a clickable button", () => {
    expect(targetButtons().some((x) => x.textContent?.includes("旧サイト"))).toBe(false);
    expect(container.textContent).toContain("→ 旧サイト（統合前）");
  });

  // A row aimed at a *deleted* decision (decision.deleted) has nothing to open, exactly like a deleted task.
  it("a row aimed at a deleted decision is not a clickable button", () => {
    expect(targetButtons().some((x) => x.textContent?.includes("旧方針の決定"))).toBe(false);
    expect(container.textContent).toContain("→ 旧方針の決定");
  });

  // A live decision does have somewhere to go. It must open as a *decision*: the two numbering spaces overlap, so
  // routing the id to onOpenTask would open whatever task happens to carry the same number.
  it("a row aimed at a live decision opens that decision, not the task of the same number", () => {
    const b = targetButtons().find((x) => x.textContent?.includes("RDB を真実源にする"));
    act(() => b!.click());
    expect(openedDecisions).toEqual([3]);
    expect(openedTasks).toEqual([]);
  });
});

describe("ActivityFeed reply buttons", () => {
  it("appears on every comment row, whatever it hangs off", () => {
    // The mock's activity holds two comments — one on task #1, one on decision #3 — plus the system rows.
    expect(replyButtons()).toHaveLength(2);
  });

  it("clicking returns the task to reply to", () => {
    act(() => replyButtons()[0].click());
    expect(replied).toEqual([1]);
  });

  it("a comment on a decision replies on the decision's timeline", () => {
    act(() => replyButtons()[1].click());
    expect(repliedDecisions).toEqual([3]);
    expect(replied).toEqual([]);
  });
});
