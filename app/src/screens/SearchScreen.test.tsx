// @vitest-environment jsdom
// The search screen answers "where is this word written" (`AMB-D-449`), and these hold it to the three
// things that separate it from a filter box: nothing is asked until the words are submitted, a hit opens
// the record its ref names, and a search that could not run says so instead of reading as a word nothing
// is written with.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SearchAnswer, SearchQuery } from "../core/reads";

const hoisted = vi.hoisted(() => ({
  /** Every query the screen asked, in order — what proves typing costs no reads. */
  asked: [] as SearchQuery[],
  answer: null as SearchAnswer | null,
  error: undefined as unknown,
  /** What the scope pull-down has to choose among — the projects this device holds. */
  projects: [] as { id: number; name: string }[],
}));

// Only the read seam is replaced. Everything else — the ref spelling, the dictionary — is the real thing,
// because the ref is exactly what this screen turns into a destination.
vi.mock("../core/reads", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/reads")>();
  return {
    ...orig,
    useSearch: (q: SearchQuery) => {
      hoisted.asked.push(q);
      return { answer: hoisted.answer, loading: false, error: hoisted.error };
    },
  };
});

// The projects the pull-down offers are the device's, read off the snapshot the way every other screen
// reads it. Only that list is stood up here; the rest of the snapshot is the real one.
vi.mock("../core/snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/snapshot")>();
  return {
    ...orig,
    subscribe: () => () => {},
    getSnapshot: () => ({ ...orig.getSnapshot(), projects: hoisted.projects }),
  };
});

import { SearchScreen } from "./SearchScreen";
import { agoLabel, t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let openedTasks: number[];
let openedDecisions: number[];

/** The instant every fixture hit reports, so a row's own "… ago" can be asserted against it. */
const AT = "2026-07-30T00:00:00Z";

const hit = (over: Partial<SearchAnswer["hits"][number]> & { ref: string }): SearchAnswer["hits"][number] => ({
  face: "body",
  kind: over.ref.includes("-T-") ? "task" : "decision",
  title: "a record",
  at: AT,
  snippet: "…the words…",
  matches: [],
  ...over,
});

const render = () =>
  act(() => {
    root.render(
      createElement(SearchScreen, {
        onOpenTask: (id: number) => openedTasks.push(id),
        onOpenDecision: (id: number) => openedDecisions.push(id),
      }),
    );
  });

const inputs = () => Array.from(container.querySelectorAll<HTMLInputElement>("input"));
const scope = () => container.querySelector<HTMLSelectElement>("select")!;
const rows = () => Array.from(container.querySelectorAll(".feed__item"));
const button = (label: string) =>
  Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label)!;
const type = (el: HTMLInputElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  act(() => {
    setter.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
};
const press = (el: HTMLElement) => act(() => el.dispatchEvent(new MouseEvent("click", { bubbles: true })));
const choose = (el: HTMLSelectElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!;
  act(() => {
    setter.call(el, value);
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
};
/** The last query the screen asked for — what it is showing right now. */
const lastAsked = () => hoisted.asked[hoisted.asked.length - 1];

beforeEach(() => {
  hoisted.asked = [];
  hoisted.answer = null;
  hoisted.error = undefined;
  hoisted.projects = [];
  openedTasks = [];
  openedDecisions = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the search screen", () => {
  it("asks nothing while the words are being typed", () => {
    render();
    type(inputs()[0], "plug");
    type(inputs()[0], "plugin");
    // Every render asks the hook, but with the same empty question: nothing has been submitted.
    expect(hoisted.asked.every((q) => q.text === "")).toBe(true);
    expect(container.textContent).toContain(t("search.idle"));
  });

  it("asks the words once they are submitted, with the narrowing beside them", () => {
    render();
    // The side comes first, because the box is read in that side's grammar and nowhere else.
    press(button(t("search.kind.task")));
    type(inputs()[0], "plugin catalog");
    type(inputs()[1], "status:todo");
    press(button(t("search.run")));
    expect(lastAsked()).toMatchObject({
      text: "plugin catalog",
      kind: "task",
      filter: "status:todo",
      offset: 0,
    });
  });

  it("leaves the narrowing box off until a side is named, and asks nothing of what is in it", () => {
    render();
    press(button(t("search.kind.task")));
    type(inputs()[0], "plugin");
    type(inputs()[1], "status:todo");
    press(button(t("search.run")));
    expect(inputs()[1].disabled).toBe(false);
    // Back to both sides: there is no grammar to read `status:todo` in, and the two sides mean
    // different things by it (`AMB-D-563`), so the box goes off and its text stops counting.
    press(button(t("search.kindAll")));
    expect(inputs()[1].disabled).toBe(true);
    expect(lastAsked()).toMatchObject({ kind: null, filter: "" });
    // Kept rather than cleared — crossing over to look and back is not a reason to retype it.
    expect(inputs()[1].value).toBe("status:todo");
    press(button(t("search.kind.decision")));
    expect(lastAsked()).toMatchObject({ kind: "decision", filter: "status:todo" });
  });

  it("scopes to one project from the pull-down, and widens back to all of them", () => {
    hoisted.projects = [{ id: 1, name: "alpha" }, { id: 2, name: "beta" }];
    render();
    type(inputs()[0], "retention");
    press(button(t("search.run")));
    expect(lastAsked().projectId).toBe(null);
    // The scope is its own argument, never a key of the narrowing expression (`AMB-D-564`) — so it is
    // asked for beside the box rather than written into it, and both sides stay in the answer.
    choose(scope(), "2");
    expect(lastAsked()).toMatchObject({ text: "retention", projectId: 2, offset: 0 });
    choose(scope(), "");
    expect(lastAsked().projectId).toBe(null);
  });

  it("opens the record a hit's ref names, on the side the ref says", () => {
    hoisted.answer = { hits: [hit({ ref: "AMB-T-2505" }), hit({ ref: "AMB-D-449" })], totalMatched: 2 };
    render();
    type(inputs()[0], "search");
    press(button(t("search.run")));
    expect(rows()).toHaveLength(2);
    press(button("AMB-T-2505"));
    press(button("AMB-D-449"));
    expect(openedTasks).toEqual([2505]);
    expect(openedDecisions).toEqual([449]);
  });

  it("narrows on either axis without losing the words, and back to the first page", () => {
    hoisted.answer = { hits: [hit({ ref: "AMB-T-1" })], totalMatched: 60 };
    render();
    type(inputs()[0], "search");
    press(button(t("search.run")));
    press(button("›")); // walk forward a page…
    expect(lastAsked().offset).toBeGreaterThan(0);
    press(button(t("search.face.comment"))); // …then narrow the face
    expect(lastAsked()).toMatchObject({ text: "search", kind: null, face: "comment", offset: 0 });
  });

  it("holds the two axes apart, so a comment on a task is a question it can put", () => {
    hoisted.answer = { hits: [hit({ ref: "AMB-T-1" })], totalMatched: 1 };
    render();
    type(inputs()[0], "search");
    press(button(t("search.run")));
    // Setting one axis leaves the other where it was, which is the whole of `AMB-D-562`: as one row of
    // chips these were exclusive, and picking the face gave up the side.
    press(button(t("search.kind.task")));
    press(button(t("search.face.comment")));
    expect(lastAsked()).toMatchObject({ text: "search", kind: "task", face: "comment" });
    // And each widens on its own, or a reader who narrowed once could never get back.
    press(button(t("search.faceAll")));
    expect(lastAsked()).toMatchObject({ kind: "task", face: null });
    press(button(t("search.kindAll")));
    expect(lastAsked()).toMatchObject({ kind: null, face: null });
  });

  it("marks the runs the core says the words landed on, and leaves the excerpt otherwise whole", () => {
    hoisted.answer = {
      hits: [
        hit({
          ref: "AMB-T-1",
          snippet: "全文検索の索引を張る",
          matches: [{ start: 2, end: 4 }, { start: 5, end: 7 }],
        }),
      ],
      totalMatched: 1,
    };
    render();
    type(inputs()[0], "検索 索引");
    press(button(t("search.run")));
    const marks = Array.from(container.querySelectorAll(".srch__snippet mark")).map((m) => m.textContent);
    // The ranges are character positions, so a marked run is the characters they name — not the bytes
    // or the code units at those offsets.
    expect(marks).toEqual(["検索", "索引"]);
    expect(container.querySelector(".srch__snippet")!.textContent).toBe("全文検索の索引を張る");
  });

  it("shows an excerpt with no ranges exactly as it came, and never loses one to a range that does not fit", () => {
    const excerpt = "…the words…";
    hoisted.answer = {
      hits: [
        hit({ ref: "AMB-T-1", snippet: excerpt, matches: [] }),
        hit({ ref: "AMB-T-2", snippet: excerpt, matches: [{ start: 5, end: 99 }] }),
        hit({ ref: "AMB-T-3", snippet: excerpt, matches: [{ start: 90, end: 99 }] }),
      ],
      totalMatched: 3,
    };
    render();
    type(inputs()[0], "words");
    press(button(t("search.run")));
    const snippets = Array.from(container.querySelectorAll(".srch__snippet"));
    expect(snippets.map((s) => s.textContent)).toEqual([excerpt, excerpt, excerpt]);
    // A face carrying none of the words is the routine case, and one past the end costs the emphasis
    // rather than a character: only the range that fits draws a mark at all.
    expect(snippets.map((s) => s.querySelectorAll("mark").length)).toEqual([0, 1, 0]);
  });

  it("says which of the four things a hit landed on, without folding the two axes into one", () => {
    hoisted.answer = {
      hits: [
        hit({ ref: "AMB-T-1", face: "title" }),
        hit({ ref: "AMB-T-2", face: "comment", comment: "AMB-TC-9" }),
        hit({ ref: "AMB-D-3", face: "body" }),
        hit({ ref: "AMB-D-4", face: "attachment", comment: "AMB-DC-8" }),
      ],
      totalMatched: 4,
    };
    render();
    type(inputs()[0], "search");
    press(button(t("search.run")));
    const meta = rows().map((r) => r.querySelector(".feed__meta")!.textContent!);
    // The side reads on the row itself now, not only out of the ref's `AMB-T-` / `AMB-D-` spelling.
    expect(meta[0]).toContain(t("search.on.task"));
    expect(meta[1]).toContain(t("search.on.taskComment"));
    expect(meta[2]).toContain(t("search.on.decision"));
    expect(meta[3]).toContain(t("search.on.decisionComment"));
    // The face is the other axis, said beside it — and an attachment says which of the two it hangs off
    // by the target it is beside, which is the pair a single glyph could not tell apart.
    expect(meta[0]).toContain(t("search.face.title"));
    expect(meta[3]).toContain(t("search.face.attachment"));
    // A remark's own text needs no second word: the target already said "remark", so that row carries
    // one word fewer than the ones whose face adds something. Counted rather than matched, since the
    // target's own wording contains the face's in more than one language.
    const words = (i: number) =>
      Array.from(rows()[i].querySelectorAll(".feed__meta > span")).map((s) => s.textContent);
    expect(words(1)).toEqual([t("search.on.taskComment"), "AMB-TC-9", agoLabel(AT)]);
    expect(words(3)).toEqual([
      t("search.on.decisionComment"),
      t("search.face.attachment"),
      "AMB-DC-8",
      agoLabel(AT),
    ]);
  });

  it("says a search could not run rather than showing it as nothing matched", () => {
    hoisted.error = new Error("unknown filter key 'stats'");
    render();
    type(inputs()[0], "plugin");
    press(button(t("search.run")));
    expect(container.textContent).toContain(t("search.failed"));
    expect(container.textContent).toContain("unknown filter key 'stats'");
    expect(container.textContent).not.toContain(t("search.empty"));
  });
});
