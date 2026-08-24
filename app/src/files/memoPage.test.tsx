// @vitest-environment jsdom
// What the draft page has to do with what is typed on it (`AMB-T-3608`).
//
// **Nobody presses save.** A draft one has to remember to keep is a draft that gets lost, so the
// page keeps what is typed on its own — after a moment's quiet, and again on the way out, because a
// person who closes the window mid-sentence meant to keep the sentence.
//
// **One page per project, and never another project's.** The page is the project's own, so switching
// project reads the other page rather than carrying this one over — which would put a draft in front
// of somebody who is working on something else, in a field they are about to type into.
//
// **Nothing on the page explains it, so the keeping has to be visible.** A person cannot tell from a
// field that what they type is being written, and the page refuses to say so in a sentence — so what
// it does say is the state itself, and that is what is asserted here.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  kept: {} as Record<number, string>,
  writes: [] as { project: number; text: string }[],
}));

vi.mock("./memo", () => ({
  projectMemo: async (projectId: number) => hoisted.kept[projectId] ?? "",
  setProjectMemo: async (projectId: number, text: string) => {
    hoisted.writes.push({ project: projectId, text });
    hoisted.kept[projectId] = text;
  },
}));

import { MemoPage } from "./MemoPage";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const field = () => container.querySelector<HTMLTextAreaElement>("textarea")
  ?? document.body.querySelector<HTMLTextAreaElement>(".memo__page textarea")!;
const button = (text: string) =>
  [...document.body.querySelectorAll("button")].find((b) => b.textContent?.includes(text));
/** How the page says the writing stands, read off the face that is in front of the reader. */
const word = () => (document.body.querySelector<HTMLElement>(".memo__page .memo__word")
  ?? container.querySelector<HTMLElement>(".memo__word")!).textContent;

async function draw(projectId: number) {
  await act(async () => {
    root.render(createElement(MemoPage, { projectId }));
    await new Promise((r) => setTimeout(r, 0));
  });
}

async function type(value: string) {
  await act(async () => {
    const one = field();
    // React reads the value off the element, so it is set before the event it is announced with.
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!
      .set!.call(one, value);
    one.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  hoisted.kept = {};
  hoisted.writes = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.useRealTimers();
});

describe("the project's draft page", () => {
  it("shows what was written on it", async () => {
    hoisted.kept[1] = "組み立てかけの依頼";
    await draw(1);
    expect(field().value).toBe("組み立てかけの依頼");
  });

  it("keeps what is typed once the typing settles, and not once per key", async () => {
    await draw(1);
    await type("長");
    await type("長い");
    await type("長い依頼");
    expect(hoisted.writes).toEqual([]);

    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    // One write for the burst, and it is the last of what was typed.
    expect(hoisted.writes).toEqual([{ project: 1, text: "長い依頼" }]);
  });

  it("keeps the last of it on the way out, without waiting for the quiet", async () => {
    await draw(1);
    await type("書きかけ");
    await act(async () => { root.unmount(); });
    expect(hoisted.writes).toEqual([{ project: 1, text: "書きかけ" }]);

    container.remove();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  it("never carries one project's draft to another", async () => {
    hoisted.kept[1] = "こちらの下書き";
    hoisted.kept[2] = "あちらの下書き";
    await draw(1);
    expect(field().value).toBe("こちらの下書き");
    await draw(2);
    expect(field().value).toBe("あちらの下書き");
  });

  it("says nothing until something is typed", async () => {
    hoisted.kept[1] = "組み立てかけの依頼";
    await draw(1);
    // Reading what is already there is not a write, so there is nothing to report about one.
    expect(word()).toBe("");
  });

  it("says the typing is in hand, and then that it was kept", async () => {
    await draw(1);
    await type("長い依頼");
    expect(word()).toBe(t("files.memoTyping"));

    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(word()).toBe(t("files.memoKept"));
  });

  it("leaves 'kept' standing through the quiet that follows", async () => {
    await draw(1);
    await type("長い依頼");
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    // The quiet is the anxious part. Showing the word for a moment and taking it away again would
    // leave the reader who is looking at exactly this moment with nothing.
    expect(word()).toBe(t("files.memoKept"));
  });

  it("starts the wide page on a blank mark", async () => {
    await draw(1);
    await type("長い依頼");
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    await act(async () => { button(t("files.memoWide"))!.click(); });
    expect(word()).toBe("");

    // And the wide page reports its own writing from there.
    await type("長い依頼をもっと");
    expect(word()).toBe(t("files.memoTyping"));
  });

  it("opens wide, and comes back", async () => {
    hoisted.kept[1] = "広く書きたい";
    await draw(1);
    await act(async () => { button(t("files.memoWide"))!.click(); });
    // The same text, in the middle of the window and at a readable width — not a second copy of it.
    expect(document.body.querySelectorAll("textarea")).toHaveLength(1);
    expect(document.body.querySelector<HTMLTextAreaElement>(".memo__field--wide")!.value)
      .toBe("広く書きたい");

    await act(async () => { button(t("files.memoNarrow"))!.click(); });
    expect(document.body.querySelector(".memo__page")).toBeNull();
  });
});
