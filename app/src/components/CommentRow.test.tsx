// @vitest-environment jsdom
// CommentRow is shared by the task and decision panes, so its edit/remove path is tested once, here. What matters
// is that a *refused* write is never mistaken for a success: the edit box must not close on a rejected edit (the
// draft would be lost), and a rejected delete must be shown rather than swallowed. The actions live behind
// `inTauri()`, so we claim to be inside the shell; only the boundaries (native confirm dialog, attachments,
// markdown) are replaced.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CommentRow } from "./CommentRow";
import { exactLabel } from "../core/i18n";

const hoisted = vi.hoisted(() => ({
  /** The confirm dialog's answers, consumed from the front; once exhausted, everything is an OK. */
  answers: [] as boolean[],
}));

vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return { ...orig, inTauri: () => true };
});
vi.mock("../core/dialog", () => ({
  confirmDialog: () => Promise.resolve(hoisted.answers.shift() ?? true),
}));
vi.mock("./Attachments", () => ({ Attachments: () => null }));
vi.mock("./Markdown", () => ({ Markdown: ({ children }: { children: string }) => children }));

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  hoisted.answers = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  act(() => { root = createRoot(container); });
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const AUTHOR = { kind: "human", name: "Alice" } as const;

function render(props: Partial<Parameters<typeof CommentRow>[0]>) {
  act(() => {
    root.render(createElement(CommentRow, {
      id: 1, author: AUTHOR, at: new Date().toISOString(), text: "hello", target: "task_comment",
      onEdit: () => {}, onRemove: () => {}, ...props,
    }));
  });
}

// Selected by class, not by `title`: the labels are localized (default `ja`), the classes are not.
const editBtn = () => container.querySelector<HTMLButtonElement>(".comment__act");
const removeBtn = () => container.querySelector<HTMLButtonElement>(".comment__rm");
// A few microtask hops: remove() awaits the confirm dialog and then the write, edit awaits the write.
const flush = () => act(async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); });

// React tracks the controlled value through its own setter, so a bare `el.value = …` does not fire onChange.
// Go through the prototype setter, the way the other pane tests do.
function type(el: HTMLTextAreaElement, text: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  act(() => {
    set.call(el, text);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("CommentRow edit", () => {
  it("closes the box and reports the trimmed body when the edit lands", async () => {
    const onEdit = vi.fn(() => Promise.resolve());
    render({ onEdit });
    act(() => editBtn()!.click());
    type(container.querySelector<HTMLTextAreaElement>(".compose__input")!, "  edited  ");
    act(() => container.querySelector<HTMLButtonElement>(".btn--primary")!.click());
    await flush();
    expect(onEdit).toHaveBeenCalledWith("edited");
    expect(container.querySelector(".compose__input"), "box closed on success").toBeNull();
    expect(container.querySelector(".errortext")).toBeNull();
  });

  it("keeps the box open with the draft and shows the error when the edit is refused", async () => {
    const onEdit = vi.fn(() => Promise.reject(new Error("editComment refused")));
    render({ onEdit });
    act(() => editBtn()!.click());
    type(container.querySelector<HTMLTextAreaElement>(".compose__input")!, "edited");
    act(() => container.querySelector<HTMLButtonElement>(".btn--primary")!.click());
    await flush();
    expect(container.querySelector<HTMLTextAreaElement>(".compose__input")?.value, "box still open, draft kept").toBe("edited");
    expect(container.querySelector(".errortext")?.textContent).toContain("refused");
  });
});

describe("CommentRow remove", () => {
  it("shows the error when a confirmed delete is refused", async () => {
    hoisted.answers = [true]; // confirm the delete
    const onRemove = vi.fn(() => Promise.reject(new Error("removeComment refused")));
    render({ onRemove });
    act(() => removeBtn()!.click());
    await flush();
    expect(onRemove).toHaveBeenCalledOnce();
    expect(container.querySelector(".errortext")?.textContent).toContain("refused");
  });

  it("does not call onRemove when the confirm is declined", async () => {
    hoisted.answers = [false]; // decline the delete
    const onRemove = vi.fn(() => Promise.resolve());
    render({ onRemove });
    act(() => removeBtn()!.click());
    await flush();
    expect(onRemove).not.toHaveBeenCalled();
    expect(container.querySelector(".errortext")).toBeNull();
  });

  // The meta line is the only place a comment's time is written, so the exact instant has to be
  // reachable from it — the CLI prints it outright, and an old comment's wording no longer carries a
  // date on its own.
  it("puts the exact instant on the when, for both the posting and the edit", () => {
    const at = "2026-01-02T03:04:05Z";
    const editedAt = "2026-03-04T05:06:07Z";
    render({ at, editedAt });
    // The avatar carries a title of its own, so the two times are read off the end of the line.
    const titles = Array.from(container.querySelectorAll<HTMLElement>(".comment__meta span[title]"))
      .map((el) => el.title);
    expect(titles.slice(-2)).toEqual([exactLabel(at), exactLabel(editedAt)]);
  });
});
