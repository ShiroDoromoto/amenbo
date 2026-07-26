// @vitest-environment jsdom
// The market screen's contract is `AMB-D-347`'s: one fetch feeds the whole screen, and the list never draws
// every entry. These tests hold the screen to both — the catalog hook is called once and the DOM stays one
// page wide however large the catalog is — plus the narrowing controls over the copy in hand.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginCatalog } from "../core/pluginCatalog";

const hoisted = vi.hoisted(() => ({
  catalog: { entries: [], sources: [], dropped: 0 } as PluginCatalog,
  added: [] as string[],
  removed: [] as string[],
  addFails: false,
  // Which repositories the screen asked GitHub about, in order — the proof that a listed entry costs
  // nothing and only an opened one does (`AMB-D-347`).
  asked: [] as string[],
  facts: undefined as { stars?: number; downloads?: number; readme?: string; rateLimited: boolean } | undefined,
  factsError: undefined as unknown,
}));

// Replace only the data seam: the filtering, the paging and the rendering all run for real.
vi.mock("../core/pluginCatalog", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginCatalog")>();
  return {
    ...orig,
    usePluginCatalog: () => ({ catalog: hoisted.catalog, loading: false, error: undefined }),
    addCatalogSource: (url: string) => {
      if (hoisted.addFails) return Promise.reject("plugin_catalog_url_invalid");
      hoisted.added.push(url);
      return Promise.resolve(true);
    },
    removeCatalogSource: (url: string) => {
      hoisted.removed.push(url);
      return Promise.resolve(true);
    },
    usePluginRepoFacts: (repo: string) => {
      hoisted.asked.push(repo);
      return { facts: hoisted.facts, loading: false, error: hoisted.factsError };
    },
  };
});

import { PluginMarketScreen } from "./PluginMarketScreen";
import { PAGE_SIZE } from "../components/Pager";
import { t } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const catalogOf = (n: number): PluginCatalog => ({
  entries: Array.from({ length: n }, (_, i) => ({
    name: `plugin-${i}`,
    desc: i % 2 === 0 ? "even helper" : "odd helper",
    author: i === 0 ? "amenbo" : "someone",
    repo: `owner/plugin-${i}`,
    os: i % 2 === 0 ? ["macos", "linux"] : ["windows"],
    category: i % 2 === 0 ? "workflow" : "notify",
    official: i === 0,
    listed: i < 4,
    featured: false,
  })),
  sources: [{ url: "https://official", official: true, reachable: true, offered: n }],
  dropped: 0,
});

const rows = () => Array.from(container.querySelectorAll(".feed__item"));
const type = (el: HTMLInputElement, value: string) => {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  setter.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
};
const select = (el: HTMLSelectElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event("change", { bubbles: true }));
};

beforeEach(() => {
  hoisted.catalog = catalogOf(3);
  hoisted.added = [];
  hoisted.removed = [];
  hoisted.addFails = false;
  hoisted.asked = [];
  hoisted.facts = { stars: 512, downloads: 1234, readme: "# a plugin\n\nwhat it does", rateLimited: false };
  hoisted.factsError = undefined;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = () => act(() => root.render(createElement(PluginMarketScreen)));

describe("PluginMarketScreen", () => {
  it("draws a row per catalog entry", () => {
    render();
    expect(rows()).toHaveLength(3);
    expect(container.textContent).toContain("plugin-0");
    expect(container.textContent).toContain("owner/plugin-2");
  });

  // The whole point of `AMB-D-347`'s "one catalog file": a catalog of a thousand costs one page of DOM.
  it("draws one page, however large the catalog is", () => {
    hoisted.catalog = catalogOf(PAGE_SIZE * 3 + 7);
    render();
    expect(rows()).toHaveLength(PAGE_SIZE);
    expect(container.querySelector(".pager")).not.toBeNull();
  });

  // The search runs over the copy in hand: the entries never change, only which of them are drawn.
  it("searches the catalog it already holds", () => {
    hoisted.catalog = catalogOf(10);
    render();
    const search = container.querySelector("input[type=search]") as HTMLInputElement;
    act(() => type(search, "plugin-3"));
    expect(rows()).toHaveLength(1);
    expect(container.textContent).toContain("plugin-3");
    act(() => type(search, ""));
    expect(rows()).toHaveLength(10);
  });

  it("narrows by category and by trust layer, and says so when nothing matches", () => {
    hoisted.catalog = catalogOf(6);
    render();
    const [category, , layer] = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
    act(() => select(category, "notify"));
    expect(rows()).toHaveLength(3);

    act(() => select(layer, "official"));
    // plugin-0 is the only official one, and it is a workflow entry — so the two filters together match nothing.
    expect(rows()).toHaveLength(0);
    expect(container.textContent).toContain(t("plugins.emptyFilter"));
  });

  // The badge is the trust picture: who wrote it and who reviewed it, one label per row.
  it("badges each row with its layer", () => {
    hoisted.catalog = catalogOf(6);
    render();
    const badges = rows().map((r) => r.querySelector(".chip")!.textContent);
    expect(badges[0]).toBe(t("plugins.layer.official"));
    expect(badges[1]).toBe(t("plugins.layer.listed"));
    expect(badges[5]).toBe(t("plugins.layer.third-party"));
  });

  // The default ordering is the recommended one, which on a catalog that has curated nothing is
  // exactly the newest ordering — so this covers both until there is something to recommend.
  it("orders by newest first, and by name when asked", () => {
    hoisted.catalog = {
      ...catalogOf(3),
      entries: [
        { ...catalogOf(3).entries[0], name: "zeta", addedAt: "2026-07-01T00:00:00Z" },
        { ...catalogOf(3).entries[1], name: "alpha", addedAt: "2026-01-01T00:00:00Z" },
      ],
    };
    render();
    const names = () => rows().map((r) => r.querySelector("strong")!.textContent);
    expect(names()).toEqual(["zeta", "alpha"]);

    const sort = (Array.from(container.querySelectorAll("select")) as HTMLSelectElement[])[3];
    act(() => select(sort, "name"));
    expect(names()).toEqual(["alpha", "zeta"]);
  });

  // The recommendation is the index's, and it wears its own chip beside the trust badge rather than
  // on it: what a plugin is for is a different question from who wrote it.
  it("lifts a recommended entry to the top and badges it", () => {
    hoisted.catalog = {
      ...catalogOf(3),
      entries: [
        { ...catalogOf(3).entries[0], name: "zeta", addedAt: "2026-07-01T00:00:00Z" },
        { ...catalogOf(3).entries[1], name: "alpha", addedAt: "2026-01-01T00:00:00Z", featured: true },
      ],
    };
    render();
    const names = rows().map((r) => r.querySelector("strong")!.textContent);
    expect(names).toEqual(["alpha", "zeta"]);
    expect(rows()[0].textContent).toContain(t("plugins.featured"));
    expect(rows()[1].textContent).not.toContain(t("plugins.featured"));
  });

  // An empty catalog and a catalog we could not read are different answers, and the screen must not show
  // the first when it means the second.
  it("reports a catalog it could not reach instead of calling it empty", () => {
    hoisted.catalog = {
      entries: [],
      sources: [{ url: "https://third", official: false, reachable: false, offered: 0 }],
      dropped: 0,
    };
    render();
    expect(container.textContent).toContain("https://third");
    expect(container.textContent).toContain(t("plugins.emptyCatalog"));
  });
});

describe("PluginMarketScreen — the one entry it opens", () => {
  const open = (i: number) => act(() => (rows()[i] as HTMLElement).click());
  const detail = () => container.querySelector(".plugdet");

  // The load-bearing half of `AMB-D-347`: a list of any size asks GitHub nothing, and opening one entry
  // asks about that entry only.
  it("asks GitHub about an entry only once it is opened", () => {
    hoisted.catalog = catalogOf(6);
    render();
    expect(hoisted.asked).toEqual([]);
    expect(detail()).toBeNull();

    open(2);
    expect(hoisted.asked).toEqual(["owner/plugin-2"]);
    expect(detail()).not.toBeNull();
  });

  it("draws the figures and the README GitHub answered with", () => {
    hoisted.catalog = catalogOf(3);
    render();
    open(0);
    const text = detail()!.textContent!;
    expect(text).toContain("512");
    expect(text).toContain("1,234");
    expect(text).toContain("what it does");
  });

  // The catalog fields must stand on their own: a repository that answered nothing still has a name, a
  // description and a badge, and the detail says why the numbers are missing rather than showing none.
  it("still shows the catalog's own fields when GitHub could not be read", () => {
    hoisted.catalog = catalogOf(3);
    hoisted.facts = undefined;
    hoisted.factsError = "offline";
    render();
    open(1);
    const text = detail()!.textContent!;
    expect(text).toContain("plugin-1");
    expect(text).toContain(t("plugins.factsError"));
    expect(text).toContain(t("plugins.noReadme"));
  });

  // Too many requests is different news from a failure — waiting fixes it — so it gets its own line.
  it("says when GitHub is rate-limiting, over whatever it did have", () => {
    hoisted.catalog = catalogOf(3);
    hoisted.facts = { stars: 7, rateLimited: true };
    render();
    open(0);
    expect(detail()!.textContent).toContain(t("plugins.rateLimited"));
    expect(detail()!.textContent).toContain("7");
  });

  it("closes on the button and on Escape", () => {
    hoisted.catalog = catalogOf(3);
    render();

    open(0);
    const close = Array.from(detail()!.querySelectorAll("button"))
      .find((b) => b.textContent === t("plugins.close"))!;
    act(() => close.click());
    expect(detail()).toBeNull();

    open(0);
    act(() => { window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })); });
    expect(detail()).toBeNull();
  });

  // An entry the catalog stopped offering (a source was unregistered while its detail was open) has
  // nothing left to draw, so the detail goes with it rather than showing a plugin that is no longer listed.
  it("closes when the entry leaves the catalog underneath it", () => {
    hoisted.catalog = catalogOf(3);
    render();
    open(2);
    expect(detail()).not.toBeNull();

    hoisted.catalog = catalogOf(2);
    render();
    expect(detail()).toBeNull();
  });
});

describe("PluginMarketScreen — the catalogs it merges", () => {
  const twoSources: PluginCatalog = {
    ...catalogOf(2),
    sources: [
      { url: "https://official", official: true, reachable: true, offered: 2 },
      { url: "https://third", official: false, reachable: false, offered: 0 },
    ],
  };

  const openPanel = () => {
    const toggle = Array.from(container.querySelectorAll("button"))
      .find((b) => b.textContent!.includes(t("plugins.sources").split(" ")[0]))!;
    act(() => toggle.click());
  };

  it("lists every catalog, and offers to remove only the third-party ones", () => {
    hoisted.catalog = twoSources;
    render();
    openPanel();
    const rows = Array.from(container.querySelectorAll(".catsrc__row"));
    // Two catalogs plus the row that adds one.
    expect(rows).toHaveLength(3);
    expect(container.textContent).toContain("https://third");
    expect(container.textContent).toContain(t("plugins.sourceDown"));
    const removes = Array.from(container.querySelectorAll("button"))
      .filter((b) => b.textContent === t("plugins.removeSource"));
    expect(removes).toHaveLength(1);

    act(() => removes[0].click());
    expect(hoisted.removed).toEqual(["https://third"]);
  });

  it("registers a URL that was typed, trimmed", async () => {
    hoisted.catalog = twoSources;
    render();
    openPanel();
    const input = container.querySelector("input[type=url]") as HTMLInputElement;
    act(() => type(input, "  https://new/catalog.json  "));
    const add = Array.from(container.querySelectorAll("button"))
      .find((b) => b.textContent === t("plugins.addSource"))!;
    await act(async () => { add.click(); });
    expect(hoisted.added).toEqual(["https://new/catalog.json"]);
    expect(input.value).toBe("");
  });

  // A URL core refuses (not http(s), or the official catalog's own) must land on screen, not in the console.
  it("shows why a rejected URL was rejected", async () => {
    hoisted.catalog = twoSources;
    hoisted.addFails = true;
    render();
    openPanel();
    const input = container.querySelector("input[type=url]") as HTMLInputElement;
    act(() => type(input, "ftp://nope"));
    const add = Array.from(container.querySelectorAll("button"))
      .find((b) => b.textContent === t("plugins.addSource"))!;
    await act(async () => { add.click(); });
    expect(container.textContent).toContain("plugin_catalog_url_invalid");
    expect(hoisted.added).toEqual([]);
  });
});
