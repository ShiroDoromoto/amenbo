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
const openedTasks: number[] = [];
const edited: Array<[number, number]> = [];

const replyButtons = () =>
  Array.from(container.querySelectorAll("button")).filter((b) => b.textContent?.startsWith("↩"));
const targetButtons = () =>
  Array.from(container.querySelectorAll("button")).filter((b) => b.textContent?.startsWith("→"));

// Outside Tauri, so the snapshot comes from the mock fixtures: one comment plus a system row.
beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  replied.length = 0;
  openedTasks.length = 0;
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
          onReplyToTask: (id: number) => replied.push(id),
          onEditComment: (taskId: number, commentId: number) => edited.push([taskId, commentId]),
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

  // A row aimed at a decision (decision.deleted) is the same: decisions are deleted outright, so there is
  // nothing to open. Miss this and the decision's id gets opened as if it were a task id.
  it("a row aimed at a decision is not a clickable button", () => {
    expect(targetButtons().some((x) => x.textContent?.includes("旧方針の決定"))).toBe(false);
    expect(container.textContent).toContain("→ 旧方針の決定");
  });
});

describe("ActivityFeed reply buttons", () => {
  it("appears only on comment rows aimed at a task", () => {
    // The mock's activity is one comment (aimed at task #1) plus a system row.
    expect(replyButtons()).toHaveLength(1);
  });

  it("clicking returns the task to reply to", () => {
    act(() => replyButtons()[0].click());
    expect(replied).toEqual([1]);
  });
});
