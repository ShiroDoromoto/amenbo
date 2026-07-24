// @vitest-environment jsdom
// The decision search's client half. The match itself is core's (`decision_list`'s `text:`, comment bodies
// included) and is tested there; what is at stake here is the shape of the answer the screen filters by —
// three states that a plain "list of ids" flattens into one, and each of them shows a different set of
// decisions on screen:
//
//   nothing was asked        → null      → show everything
//   asked, nothing matched   → empty set → show nothing
//   asked, answer in flight  → the last answer, so the list does not flash back to everything mid-word
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  /** Queries the hook actually sent to core, in order. */
  asked: [] as string[],
  /** What core answers, by query text. Anything unlisted matches nothing. */
  answers: new Map<string, number[]>(),
}));

vi.mock("./snapshot", async (importOriginal) => {
  const orig = await importOriginal<typeof import("./snapshot")>();
  return { ...orig, inTauri: () => true };
});
vi.mock("./ipc", () => ({
  invoke: (cmd: string, args: { text: string }) => {
    if (cmd !== "decision_search") throw new Error(`unexpected command ${cmd}`);
    hoisted.asked.push(args.text);
    return Promise.resolve(hoisted.answers.get(args.text) ?? []);
  },
}));

import { useDecisionSearchIds } from "./reads";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** Render the hook's answer as text: `all` for null (nothing asked), otherwise the ids it narrowed to. */
function Probe({ text }: { text: string }) {
  const hits = useDecisionSearchIds(1, text);
  return createElement("span", { id: "out" }, hits === null ? "all" : `[${[...hits].join(",")}]`);
}

const render = (text: string) => act(() => root.render(createElement(Probe, { text })));
const shown = () => container.querySelector("#out")?.textContent;
async function settle() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

beforeEach(() => {
  hoisted.asked.length = 0;
  hoisted.answers.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("useDecisionSearchIds", () => {
  it("an empty box asks core nothing and narrows nothing", async () => {
    render("   ");
    await settle();
    expect(shown()).toBe("all");
    expect(hoisted.asked).toEqual([]);
  });

  it("a query narrows to what core matched, and matching nothing is an answer", async () => {
    hoisted.answers.set("計測", [7, 9]);
    render("計測");
    await settle();
    expect(shown()).toBe("[7,9]");

    render("見つからない語");
    await settle();
    expect(shown()).toBe("[]"); // not "all" — nothing matched is not the same as nothing asked
  });

  it("the previous answer stands while the next one is in flight", async () => {
    // Needles unique to this test: the query cache outlives a test (that is its job), so a term another
    // test already asked would be painted from the cache rather than being in flight at all.
    hoisted.answers.set("配", [7]);
    hoisted.answers.set("配送", [9]);
    render("配");
    await settle();
    expect(shown()).toBe("[7]");

    // The next keystroke is a new query key, so its data is undefined for a render. If that read as
    // "nothing asked", the list would flash back to every decision between characters.
    render("配送");
    expect(shown()).toBe("[7]");
    await settle();
    expect(shown()).toBe("[9]");
  });

  it("clearing the box drops the held answer at once, without waiting on a query", async () => {
    hoisted.answers.set("採択", [9]);
    render("採択");
    await settle();
    expect(shown()).toBe("[9]");

    render("");
    expect(shown()).toBe("all");
  });

  it("the query is sent trimmed, so trailing space does not make a second one of the same search", async () => {
    hoisted.answers.set("前提", [9]);
    render("前提");
    await settle();
    render("  前提  ");
    await settle();
    expect(shown()).toBe("[9]");
    expect(hoisted.asked).toEqual(["前提"]);
  });
});
