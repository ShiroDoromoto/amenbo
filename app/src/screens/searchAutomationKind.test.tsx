// @vitest-environment jsdom
// The search screen's third side: the documents an automation's steps share (`AMB-D-944`). Only the
// read is stubbed; the chips, the narrowing box and the hit row all run for real.
//
// What these guard: **the side is reachable** — a fourth chip, so a reader can ask for the automations
// alone; **the narrowing box is off on it, and says why** — that side has no listing to lend the box a
// grammar, and the line a reader is shown there is not the one shown before any side is picked; and
// **the hit's ref opens the build screen** — which is where an automation's documents are read, and the
// row carries the number to open it by and nothing else (`AMB-T-5278`).
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SearchHitDto } from "../bindings/bindings";

const hoisted = vi.hoisted(() => ({
  hits: [] as SearchHitDto[],
  asked: [] as { kind: string | null; filter: string }[],
  /** The automations the row asked to open, in order. */
  opened: [] as number[],
}));

vi.mock("../core/reads", async (real) => {
  const mod = await real<typeof import("../core/reads")>();
  return {
    ...mod,
    useSearch: (q: { kind: string | null; filter: string }) => {
      hoisted.asked.push({ kind: q.kind, filter: q.filter });
      return {
        answer: { hits: hoisted.hits, totalMatched: hoisted.hits.length },
        loading: false,
        error: null,
      };
    },
  };
});
// One frozen value, not a fresh one per call: `useSyncExternalStore` compares what it is handed with
// what it last had, and a new object every time is an update every time.
vi.mock("../core/snapshot", async (real) => {
  const mod = await real<typeof import("../core/snapshot")>();
  const empty = { ...mod.getSnapshot(), projects: [] };
  return { ...mod, subscribe: () => () => {}, getSnapshot: () => empty };
});

import { t } from "../core/i18n";
import { SearchScreen } from "./SearchScreen";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function hit(over: Partial<SearchHitDto> = {}): SearchHitDto {
  return {
    face: "body",
    kind: "automation",
    ref: "AMB-AUT-1",
    title: "Review and fix",
    at: "2026-09-22T02:34:00Z",
    snippet: "Every patch keeps the house style.",
    matches: [],
    ...over,
  };
}

async function render() {
  await act(async () => {
    root.render(createElement(SearchScreen, {
      onOpenTask: () => {},
      onOpenDecision: () => {},
      onOpenAutomation: (id: number) => hoisted.opened.push(id),
    }));
  });
}

/** Type into a controlled field: React listens for the native setter, so assigning `.value` alone is not seen. */
async function type(el: HTMLInputElement, text: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => {
    set.call(el, text);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** Type the words and press Search — the screen asks nothing until it is submitted. */
async function ask(words: string) {
  await type(container.querySelector<HTMLInputElement>(".srch__input")!, words);
  await act(async () => {
    chip(t("search.run")).click();
  });
}

/** The question the screen last put. `Array.at` is past this build's lib target, so it is indexed. */
const lastAsked = () => hoisted.asked[hoisted.asked.length - 1];

const chips = () => [...container.querySelectorAll("button")];
function chip(label: string): HTMLButtonElement {
  const found = chips().find((b) => b.textContent?.trim() === label);
  if (!found) throw new Error(`no button labelled ${label}`);
  return found;
}

beforeEach(() => {
  hoisted.hits = [];
  hoisted.asked = [];
  hoisted.opened = [];
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

describe("the automations are a side of their own", () => {
  it("is a chip a reader can pick, beside the two records", async () => {
    await render();
    const labels = chips().map((b) => b.textContent?.trim());
    expect(labels).toContain(t("search.kind.automation"));
    await act(async () => chip(t("search.kind.automation")).click());
    expect(lastAsked().kind).toBe("automation");
  });

  it("turns the narrowing box off, and says which of the two reasons it is", async () => {
    await render();
    const box = () => container.querySelector<HTMLInputElement>(".srch__filter")!;
    // No side picked: the move back is to pick one.
    expect(box().disabled).toBe(true);
    expect(box().placeholder).toBe(t("search.filterPhOff"));

    await act(async () => chip(t("search.kind.task")).click());
    expect(box().disabled).toBe(false);

    // A side picked that has no listing: the move back is to pick another one, so the line differs.
    await act(async () => chip(t("search.kind.automation")).click());
    expect(box().disabled).toBe(true);
    expect(box().placeholder).toBe(t("search.filterPhNone"));
  });

  it("does not carry what was typed in the box into the question", async () => {
    await render();
    await act(async () => chip(t("search.kind.task")).click());
    await type(container.querySelector<HTMLInputElement>(".srch__filter")!, "status:todo");
    await ask("house style");
    expect(lastAsked().filter).toBe("status:todo");

    await act(async () => chip(t("search.kind.automation")).click());
    expect(lastAsked().filter).toBe("");
  });

  it("opens the automation the ref names, which is where its documents are read", async () => {
    hoisted.hits = [hit()];
    await render();
    await ask("house style");
    const ref = container.querySelector<HTMLElement>(".srch__ref")!;
    expect(ref.tagName).toBe("BUTTON");
    expect(ref.textContent).toBe("AMB-AUT-1");

    await act(async () => ref.click());

    // The number and nothing else: which project it is in is answered on the way there.
    expect(hoisted.opened).toEqual([1]);
  });

  it("still opens a task's hit, which is the side that has somewhere to lead", async () => {
    hoisted.hits = [hit({ kind: "task", ref: "AMB-T-12", title: "Write it down", face: "title" })];
    await render();
    await ask("house style");
    const ref = container.querySelector(".srch__ref")!;
    expect(ref.tagName).toBe("BUTTON");
  });
});
