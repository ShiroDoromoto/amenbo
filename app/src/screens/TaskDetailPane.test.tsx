// @vitest-environment jsdom
// The trap is in the asynchrony: the task arrives later, via useTask, so at the moment a reply is
// requested the pane still says "not found" and there is no comment box in the DOM. Aim the focus at
// it right then, without holding the request, and the focus is spent on a null ref and lost — the
// pane opens, but the keystrokes fall through to the feed. So every reply-focus test must go through
// **the path where the task arrives late** (the first paint says "not found"), which means using a
// task id that is not warm in the cache.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { TaskDetailPane } from "./TaskDetailPane";
import { StoreProvider } from "../store/store";
import { loadSnapshot } from "../core/snapshot";
import { addTask } from "../core/mutations";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const commentBox = () => container.querySelector<HTMLTextAreaElement>("textarea.compose__input");
const tabButton = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label);

/** Render with these props (re-rendering into the same root is a props update, which is how a reply request arrives late). */
function render(props: {
  taskId: number;
  focusCommentAt?: number;
  editCommentAt?: { commentId: number; nonce: number };
}) {
  act(() =>
    root.render(createElement(StoreProvider, null, createElement(TaskDetailPane, props))),
  );
}

/** Wait for useTask (useQuery) to resolve; until it does, the pane renders "not found". */
async function settle() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeAll(async () => {
  await loadSnapshot();
});

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("TaskDetailPane reply focus", () => {
  it("even when the task arrives later, focus lands the moment the comment box is rendered", async () => {
    render({ taskId: 1, focusCommentAt: 1 });
    expect(commentBox()).toBeNull();
    expect(container.textContent).toContain(t("detail.notFound"));

    await settle();

    const box = commentBox();
    expect(box).not.toBeNull();
    expect(document.activeElement).toBe(box);
  });

  it("even while on the activity tab, it returns to the detail tab and focuses the comment box", async () => {
    render({ taskId: 3 });
    await settle();

    act(() => tabButton(t("detail.tab.activity"))!.click());
    expect(commentBox()).toBeNull(); // The comment box lives on the detail tab and nowhere else

    render({ taskId: 3, focusCommentAt: 1 });
    await settle();

    const box = commentBox();
    expect(box).not.toBeNull();
    expect(document.activeElement).toBe(box);
  });
});

describe("TaskDetailPane finishing a creation", () => {
  // The pane the compose form hands over to is where a creation is ended (`AMB-D-554`): the task it just
  // made is still being created, so the row saying so has to be there, and pressing it has to clear the
  // premise rather than merely hide the row.
  it("a task just created says it is still being created, and the button ends that", async () => {
    const id = await addTask(null, "作りかけ");
    expect(id).not.toBeNull();
    render({ taskId: id! });
    await settle();

    expect(container.textContent).toContain(t("chip.draft"));
    const finish = Array.from(container.querySelectorAll("button"))
      .find((b) => b.textContent === t("detail.finishCreating"));
    expect(finish).toBeDefined();

    await act(async () => { finish!.click(); });
    await settle();

    expect(container.textContent).not.toContain(t("chip.draft"));
    expect(Array.from(container.querySelectorAll("button")).some((b) => b.textContent === t("detail.finishCreating")))
      .toBe(false);
  });
});

describe("TaskDetailPane dating the task", () => {
  // The question the page could not answer before: how long has this been sitting here. The stamp is
  // written in the reader's locale, so the assertion is on the year rather than on a formatted string.
  it("says when the task was written, and when it was last written to", async () => {
    render({ taskId: 1 }); // The fixture that has moved since it was filed (mock/data.ts)
    await settle();

    const meta = container.querySelector(".meta");
    expect(meta).not.toBeNull();
    expect(meta!.textContent).toContain(t("detail.updated"));
    // Both stamps are there — filed in June, last written to a fortnight later.
    expect(meta!.textContent).toMatch(/2026/);
    expect(meta!.querySelector(`[title="${t("detail.updatedHint")}"]`)).not.toBeNull();
  });

  it("says nothing about an update on a task nobody has written to since", async () => {
    render({ taskId: 2 }); // Filed and untouched: created and updated are the same instant
    await settle();

    const meta = container.querySelector(".meta");
    expect(meta).not.toBeNull();
    expect(meta!.textContent).toContain(t("detail.created"));
    expect(meta!.textContent).not.toContain(t("detail.updated"));
  });
});

describe("TaskDetailPane targeted edit", () => {
  it("the comment named by the pencil becomes an edit box carrying its body text", async () => {
    render({ taskId: 1, editCommentAt: { commentId: 2, nonce: 1 } });
    await settle();

    const drafts = Array.from(container.querySelectorAll<HTMLTextAreaElement>("textarea.compose__input"))
      .filter((el) => el.value !== "");
    expect(drafts).toHaveLength(1); // The new-comment box is empty; only the edit box carries text
    expect(drafts[0].value).toContain("先方確認待ち");
  });

  it("with nothing named, no row becomes an edit box", async () => {
    render({ taskId: 1 });
    await settle();

    const drafts = Array.from(container.querySelectorAll<HTMLTextAreaElement>("textarea.compose__input"))
      .filter((el) => el.value !== "");
    expect(drafts).toHaveLength(0);
  });
});
