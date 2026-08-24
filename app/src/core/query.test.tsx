// @vitest-environment jsdom
// The query layer's condition of life: **an invalidation always reaches a query that has subscribers**. It must
// still hold after a key change, after moving back and forth between views, with several subscribers on one key,
// through hours of churn that overflows the LRU, and through StrictMode's double mount. Break it and an entry can
// be on screen yet counted as not live, at which point no invalidation path — an ack, a scope, a full re-read —
// ever refetches it again and the board freezes on stale data. `watchStore.test` and `changes.test` only look at
// a single wake, so they let this failure straight through.
import { act, createElement, StrictMode, useState, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  __queryCache, invalidateAllQueries, invalidateQueries, invalidateScopes, useQuery, type QueryKey,
} from "./query";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let fetched: Map<string, number>; // key → how many times it was fetched; a rise is how we see an invalidation land

/** Carry "this is the nth fetch of that key" in the value, so a refetch is visible in the DOM. */
async function fetchFor(k: string): Promise<string> {
  const n = (fetched.get(k) ?? 0) + 1;
  fetched.set(k, n);
  return `${k}#${n}`;
}

function Probe({ k }: { k: string }) {
  const { data } = useQuery<string>(["probe", k], () => fetchFor(k));
  return createElement("span", { "data-k": k }, data ?? "…");
}

function render(node: ReactNode): void {
  act(() => root.render(node));
}

/** Flush the fetches' resolution and the re-renders they set off. */
async function settle(): Promise<void> {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

const shown = (k: string) => container.querySelector(`[data-k="${k}"]`)?.textContent;
const count = (k: string) => fetched.get(k) ?? 0;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  fetched = new Map();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("useQuery — invalidations reach queries that have subscribers", () => {
  it("a mounted query is refetched by a full invalidation and the new value appears on screen", async () => {
    render(createElement(Probe, { k: "board" }));
    await settle();
    expect(shown("board")).toBe("board#1");

    invalidateAllQueries(); // stands in for an external write reaching reconcile("gap")
    await settle();
    expect(shown("board")).toBe("board#2");
  });

  it("an unmounted query is not refetched (invalidation touches only what is live)", async () => {
    render(createElement(Probe, { k: "gone" }));
    await settle();
    render(createElement(Probe, { k: "here" }));
    await settle();

    invalidateAllQueries();
    await settle();
    expect(count("gone")).toBe(1); // not on screen, so there is no reason to refetch it
    expect(shown("here")).toBe("here#2");
  });

  it("invalidation reaches a returned-to key even after key changes and view round-trips (list ⇄ board)", async () => {
    for (const k of ["list", "board", "list", "board"]) {
      render(createElement(Probe, { k }));
      await settle();
    }
    const before = count("board");

    invalidateAllQueries();
    await settle();
    expect(count("board")).toBe(before + 1);
    expect(shown("board")).toBe(`board#${before + 1}`);
  });

  it("when several subscribe to the same key, invalidation still reaches the rest after one leaves", async () => {
    const two = createElement("div", null, createElement(Probe, { k: "shared" }), createElement(Probe, { k: "shared" }));
    render(two);
    await settle();

    render(createElement("div", null, createElement(Probe, { k: "shared" }))); // unmount one of the two
    await settle();
    const before = count("shared");

    invalidateAllQueries();
    await settle();
    expect(count("shared")).toBe(before + 1);
    expect(shown("shared")).toBe(`shared#${before + 1}`);
  });

  it("a board left on screen stays live even through long churn that overflows the LRU (how it breaks in the wild)", async () => {
    function Session({ churn }: { churn: number }) {
      return createElement(
        "div",
        null,
        createElement(Probe, { k: "board" }), // the board, which stays on screen throughout
        createElement(Probe, { k: `churn-${churn}`, key: churn }), // the surfaces that come and go (panes, typing in a filter…)
      );
    }
    // Push more keys through than the LRU holds (128), so the older entries are evicted. The board stays mounted.
    for (let i = 0; i < 160; i++) {
      render(createElement(Session, { churn: i }));
      await settle();
    }
    const before = count("board");

    invalidateAllQueries();
    await settle();
    expect(count("board")).toBe(before + 1);
    expect(shown("board")).toBe(`board#${before + 1}`);

    invalidateQueries((key: QueryKey) => key[0] === "probe"); // a targeted invalidation (the scope path) lands too
    await settle();
    expect(count("board")).toBe(before + 2);
  });

  it("stays live through StrictMode double-mounting (effects firing twice)", async () => {
    render(createElement(StrictMode, null, createElement(Probe, { k: "board" })));
    await settle();
    const before = count("board");

    invalidateAllQueries();
    await settle();
    expect(count("board")).toBeGreaterThan(before);
    expect(shown("board")).toBe(`board#${count("board")}`);
  });

  it("stays live even when the same screen remounts repeatedly (like the shell remount on a language switch)", async () => {
    function Shell({ gen }: { gen: number }) {
      return createElement("div", { key: gen }, createElement(Probe, { k: "board" })); // a new key remounts the whole subtree
    }
    for (let gen = 0; gen < 5; gen++) {
      render(createElement(Shell, { gen }));
      await settle();
    }
    const before = count("board");

    invalidateAllQueries();
    await settle();
    expect(count("board")).toBe(before + 1);
  });

  it("even after the subscription is lost, the next render re-attaches and refetches on its own (self-healing from a real-world freeze)", async () => {
    let bump = () => {};
    function Host() {
      const [n, setN] = useState(0);
      bump = () => setN(n + 1);
      return createElement(Probe, { k: "board" });
    }
    render(createElement(Host));
    await settle();
    const before = count("board");

    __queryCache.get(JSON.stringify(["probe", "board"]))!.listeners.clear();
    invalidateAllQueries();
    await settle();
    expect(count("board")).toBe(before); // while it is detached nothing lands — the frozen board, reproduced

    act(() => bump()); // let anything re-render this tree…
    await settle();
    expect(count("board")).toBe(before + 1); // …and it re-attaches itself and refetches from the source of truth

    invalidateAllQueries(); // invalidations land again from here on: it is live once more
    await settle();
    expect(count("board")).toBe(before + 2);
  });

  it("while there are subscribers, the fetcher sees the latest props (cache is shared, fetcher is swapped)", async () => {
    function Toggle() {
      const [n, setN] = useState(0);
      const { data } = useQuery<string>(["probe", "board"], () => fetchFor(`board-${n}`));
      return createElement(
        "span",
        { "data-k": "board", onClick: () => setN(1) },
        `${data ?? "…"}`,
      );
    }
    render(createElement(Toggle));
    await settle();
    expect(count("board-0")).toBe(1);

    act(() => container.querySelector<HTMLElement>('[data-k="board"]')!.click()); // the prop (n) changes
    invalidateAllQueries();
    await settle();
    expect(count("board-1")).toBe(1); // the refetch runs the latest fetcher, not the stale closure
  });
});

// The other half of the fold in `core/changes`: a scope is only worth naming if some query listens for it.
// The two are written apart — a dataset map and a switch — so a scope added to one and not the other is a
// silent no-op: the feed folds it, nothing refetches, and the screen keeps the state it had.
describe("invalidateScopes — a scope reaches the queries drawn from it", () => {
  function KeyProbe({ qkey, k }: { qkey: QueryKey; k: string }) {
    const { data } = useQuery<string>(qkey, () => fetchFor(k));
    return createElement("span", { "data-k": k }, data ?? "…");
  }

  it("refetches the installed plugins for the plugin scope, and for no other", async () => {
    render(createElement(KeyProbe, { qkey: ["plugin-installs", 1], k: "installs" }));
    await settle();
    expect(count("installs")).toBe(1);

    invalidateScopes(new Set(["tasks"]));
    await settle();
    expect(count("installs")).toBe(1); // a task write is not a plugin's business

    invalidateScopes(new Set(["plugins"]));
    await settle();
    expect(count("installs")).toBe(2); // a gate moved outside this window: re-read the rows that draw it
  });

  // What nothing is working on is answered from two faces at once, so it goes stale from either. A reservation
  // handed back is a task write and a proposal settled is a decision write, and a mapping that named only one
  // of them would leave the mark standing on a row that had just been picked up or ruled on.
  it("refetches what nothing is working on for a task write and for a decision write", async () => {
    render(createElement(KeyProbe, { qkey: ["adrift", 1], k: "adrift" }));
    await settle();
    expect(count("adrift")).toBe(1);

    invalidateScopes(new Set(["plugins"]));
    await settle();
    expect(count("adrift")).toBe(1);

    invalidateScopes(new Set(["tasks"]));
    await settle();
    expect(count("adrift")).toBe(2);

    invalidateScopes(new Set(["decisions"]));
    await settle();
    expect(count("adrift")).toBe(3);
  });
});
