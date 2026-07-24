// @vitest-environment jsdom
// The activity feed's ✎ / ✕ are wrapped in `inTauri()`, so bare jsdom (i.e. the browser-mock path) never renders
// them — ActivityFeed.test.tsx's net cannot catch either one. These tests pretend to be inside the Tauri shell.
// The receiving end (the detail pane opening the named comment for editing) lives in TaskDetailPane.test.tsx.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

// Stub the Tauri check and the surroundings that only work under Tauri (paging back through history, the native
// confirm dialog, writes to the store). Only the boundary that asks "are we inside Tauri" is replaced; the feed's
// own rendering runs for real.
const hoisted = vi.hoisted(() => ({
  confirmAnswer: true,
  removed: [] as Array<[number, number]>,
  removedFromDecision: [] as Array<[number, number]>,
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true };
});
// Paging back through history invokes core. Since we are only impersonating Tauri, hold it to never being called.
// Only the paging seam is replaced: the row identity these rows are keyed and de-duplicated by is the
// real one (`AMB-D-388`), and mocking it away would hide the collision it exists to survive.
vi.mock("../core/activity", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../core/activity")>()),
  loadActivityPage: () => Promise.resolve([]),
  loadTaskActivity: () => Promise.resolve([]),
}));
vi.mock("../core/dialog", () => ({ confirmDialog: () => Promise.resolve(hoisted.confirmAnswer) }));
vi.mock("../store/store", async () => {
  const { getSnapshot } = await import("../core/snapshot");
  return {
    useStore: () => ({
      listActivity: () => getSnapshot().activity,
      removeComment: (commentId: number, taskId: number) => hoisted.removed.push([commentId, taskId]),
      removeDecisionComment: (commentId: number, decisionId: number) =>
        hoisted.removedFromDecision.push([commentId, decisionId]),
    }),
  };
});

import { ActivityFeed } from "./ActivityFeed";
import { loadSnapshot } from "../core/snapshot";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
const edited: Array<[number, number]> = [];
const editedOnDecision: Array<[number, number]> = [];

const buttonsTitled = (title: string) =>
  Array.from(container.querySelectorAll("button")).filter((b) => b.getAttribute("title") === title);
// The buttons are a single emoji each, so they are told apart by their (localised) title — the test follows the wording wherever it goes.
const editButtons = () => buttonsTitled(t("comment.edit"));
const removeButtons = () => buttonsTitled(t("comment.remove"));

beforeAll(async () => {
  await loadSnapshot(); // The mock fixtures: one comment (#2, addressed to task #1) plus a system row.
});

beforeEach(() => {
  edited.length = 0;
  editedOnDecision.length = 0;
  hoisted.removed.length = 0;
  hoisted.removedFromDecision.length = 0;
  hoisted.confirmAnswer = true;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  act(() =>
    root.render(
      createElement(ActivityFeed, {
        onOpenTask: () => {},
        onOpenDecision: () => {},
        onReplyToTask: () => {},
        onReplyToDecision: () => {},
        onEditComment: (taskId: number, commentId: number) => edited.push([taskId, commentId]),
        onEditDecisionComment: (decisionId: number, commentId: number) =>
          editedOnDecision.push([decisionId, commentId]),
      }),
    ),
  );
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("ActivityFeed ✎ / ✕ (Tauri only)", () => {
  it("appears on every comment row, and on no system row", () => {
    // The fixtures hold two comments — one on a task, one on a live decision. A system row, and a row addressed to a
    // project or to something deleted, has no thread left to edit or delete in.
    expect(editButtons()).toHaveLength(2);
    expect(removeButtons()).toHaveLength(2);
  });

  it("✎ hands \"this comment of this task\" to the detail pane (does not edit inline)", () => {
    act(() => editButtons()[0].click());
    expect(edited).toEqual([[1, 2]]);
  });

  it("✕ deletes after confirmation (hard delete)", async () => {
    await act(async () => {
      removeButtons()[0].click();
    });
    expect(hoisted.removed).toEqual([[2, 1]]);
  });

  // The decision comment in the fixture carries comment id 2 — the same number a task comment already
  // has, since the two tables number independently (`AMB-D-388`). So this also pins that a row is
  // routed by what it hangs on, never by its id.
  it("a comment on a decision is edited and deleted through the decision's own writes", async () => {
    act(() => editButtons()[1].click());
    expect(editedOnDecision).toEqual([[3, 2]]);
    expect(edited).toEqual([]);
    await act(async () => {
      removeButtons()[1].click();
    });
    expect(hoisted.removedFromDecision).toEqual([[2, 3]]);
    expect(hoisted.removed).toEqual([]); // never through the task-side write
  });

  it("does not delete when the ✕ confirmation is canceled", async () => {
    hoisted.confirmAnswer = false;
    await act(async () => {
      removeButtons()[0].click();
    });
    expect(hoisted.removed).toEqual([]);
  });
});
