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

import { SearchScreen } from "./SearchScreen";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let openedTasks: number[];
let openedDecisions: number[];

const hit = (over: Partial<SearchAnswer["hits"][number]> & { ref: string }): SearchAnswer["hits"][number] => ({
  face: "body",
  kind: over.ref.includes("-T-") ? "task" : "decision",
  title: "a record",
  at: "2026-07-30T00:00:00Z",
  snippet: "…the words…",
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
/** The last query the screen asked for — what it is showing right now. */
const lastAsked = () => hoisted.asked[hoisted.asked.length - 1];

beforeEach(() => {
  hoisted.asked = [];
  hoisted.answer = null;
  hoisted.error = undefined;
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
    type(inputs()[0], "plugin catalog");
    type(inputs()[1], "status:todo");
    press(button(t("search.run")));
    expect(lastAsked()).toMatchObject({ text: "plugin catalog", filter: "status:todo", offset: 0 });
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

  it("narrows to a face without losing the words, and back to the first page", () => {
    hoisted.answer = { hits: [hit({ ref: "AMB-T-1" })], totalMatched: 60 };
    render();
    type(inputs()[0], "search");
    press(button(t("search.run")));
    press(button("›")); // walk forward a page…
    expect(lastAsked().offset).toBeGreaterThan(0);
    press(button(t("search.kind.comment"))); // …then narrow
    // The chip that reads "comment" narrows the face, not the kind: which record the words are on and
    // which face of it are two axes, so this one leaves both sides standing (`AMB-D-562`).
    expect(lastAsked()).toMatchObject({ text: "search", kind: null, face: "comment", offset: 0 });
    press(button(t("search.kind.task")));
    expect(lastAsked()).toMatchObject({ text: "search", kind: "task", face: null, offset: 0 });
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
