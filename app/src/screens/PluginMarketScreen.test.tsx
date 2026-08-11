// @vitest-environment jsdom
// The market screen's contract is `AMB-D-347`'s: one fetch feeds the whole screen, and the list never draws
// every entry. These tests hold the screen to both — the catalog hook is called once and the DOM stays one
// page wide however large the catalog is — plus the narrowing controls over the copy in hand.
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginCatalog, PluginDetail } from "../core/pluginCatalog";

const hoisted = vi.hoisted(() => ({
  catalog: { entries: [], sources: [], dropped: 0 } as PluginCatalog,
  /** Every registration that reached the door, with the name and the pin it was agreed on. */
  added: [] as { url: string; name?: string; agreedFingerprint?: string }[],
  probed: [] as string[],
  /** What a probe answers with — the fingerprint the consent panel then has to show. */
  probeFingerprint: "6272CBB782CB57A0" as string | null,
  removed: [] as string[],
  probeFails: false,
  // Which repositories the screen asked GitHub about, in order — the proof that a listed entry costs
  // nothing and only an opened one does (`AMB-D-347`).
  asked: [] as string[],
  // Whether each of those asks included the README (`AMB-D-638`) — `"unknown"` while the catalog's
  // detail has not answered yet, which is the screen waiting rather than asking.
  askedReadme: [] as (boolean | "unknown")[],
  facts: undefined as { stars?: number; downloads?: number; readme?: string; rateLimited: boolean } | undefined,
  factsError: undefined as unknown,
  /** What the catalog's detail document says about the opened plugin, or `null` for one it does not carry. */
  detail: null as PluginDetail | null | undefined,
}));

// Replace only the data seam: the filtering, the paging and the rendering all run for real.
vi.mock("../core/pluginCatalog", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginCatalog")>();
  return {
    ...orig,
    usePluginCatalog: () => ({ catalog: hoisted.catalog, loading: false, error: undefined }),
    probeCatalogSource: (url: string) => {
      if (hoisted.probeFails) return Promise.reject("plugin_catalog_url_invalid");
      hoisted.probed.push(url);
      return Promise.resolve({
        url,
        suggestedName: new URL(url).host,
        fingerprint: hoisted.probeFingerprint,
        registered: false,
        pinsANewKey: hoisted.probeFingerprint != null,
      });
    },
    addCatalogSource: (url: string, opts: { name?: string; agreedFingerprint?: string } = {}) => {
      hoisted.added.push({ url, ...opts });
      return Promise.resolve(true);
    },
    removeCatalogSource: (url: string) => {
      hoisted.removed.push(url);
      return Promise.resolve(true);
    },
    usePluginRepoFacts: (repo: string, readme: boolean | "unknown") => {
      hoisted.asked.push(repo);
      hoisted.askedReadme.push(readme);
      return { facts: hoisted.facts, loading: false, error: hoisted.factsError };
    },
    usePluginDetail: () => ({ detail: hoisted.detail, loading: false, error: undefined }),
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
    // What review put on the official index came from it; the rest is the in-house catalog's.
    source: i < 4 ? "https://official" : "https://inhouse",
    sourceName: i < 4 ? "amenbo" : "社内カタログ",
    featured: false,
  })),
  sources: [
    { url: "https://official", name: "amenbo", fingerprint: "6272CBB782CB57A0", official: true, reachable: true, offered: n },
    ...(n > 4
      ? [{ url: "https://inhouse", name: "社内カタログ", fingerprint: "AA11BB22CC33DD44", official: false, reachable: true, offered: n - 4 }]
      : []),
  ],
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
  hoisted.probed = [];
  hoisted.probeFingerprint = "6272CBB782CB57A0";
  hoisted.removed = [];
  hoisted.probeFails = false;
  hoisted.asked = [];
  hoisted.askedReadme = [];
  hoisted.facts = { stars: 512, downloads: 1234, readme: "# a plugin\n\nwhat it does", rateLimited: false };
  hoisted.factsError = undefined;
  hoisted.detail = null;
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

  // The badge is the trust picture: who wrote it and who reviewed it, one label per row — and on the
  // free layer, which shelf it came off (`AMB-D-389`) rather than an anonymous "other".
  it("badges each row with its layer, and a free-layer row with its catalog", () => {
    hoisted.catalog = catalogOf(6);
    render();
    const badges = rows().map((r) => r.querySelector(".chip")!.textContent);
    expect(badges[0]).toBe(t("plugins.layer.official"));
    expect(badges[1]).toBe(t("plugins.layer.listed"));
    expect(badges[5]).toBe("社内カタログ");
  });

  // The filter asks "where is this from?" at two grains: the three layers, then each registered
  // catalog one by one, so "only the in-house one" is one click (`AMB-D-389`). The list itself stays
  // mixed — splitting it is the reader's move, not the screen's default.
  it("offers each registered catalog in the source filter, and narrows to it", () => {
    hoisted.catalog = catalogOf(6);
    render();
    const origin = (Array.from(container.querySelectorAll("select")) as HTMLSelectElement[])[2];
    expect(Array.from(origin.options).map((o) => o.textContent)).toEqual([
      t("plugins.anyLayer"),
      t("plugins.layer.listed"), t("plugins.layer.official"), t("plugins.layer.third-party"),
      "社内カタログ",
    ]);

    act(() => select(origin, "https://inhouse"));
    expect(rows().map((r) => r.querySelector("strong")!.textContent)).toEqual(["plugin-4", "plugin-5"]);
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
      sources: [{ url: "https://third", name: "third", fingerprint: null, official: false, reachable: false, offered: 0 }],
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
  /** What the last ask of GitHub said about the README — the flag, not the repository. */
  const lastAsk = () => hoisted.askedReadme[hoisted.askedReadme.length - 1];
  /** A detail document carrying only what these tests are about: the words its author wrote. */
  const describedAs = (words: Partial<PluginDetail>): PluginDetail => ({
    events: [],
    config: [],
    scope: "project",
    compatible: true,
    ...words,
  });

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
    // Nothing else describes this plugin, so the README is still what the body is drawn from.
    expect(lastAsk()).toBe(true);
  });

  // `AMB-D-638`: where the author wrote a description, that is the body — and the README is not fetched
  // at all, so the two never say the same thing twice in two languages.
  it("draws the author's own words instead of the README, and stops asking GitHub for one", () => {
    hoisted.catalog = catalogOf(3);
    hoisted.detail = describedAs({ about: "## what it is for\n\nin the author's own words" });
    render();
    open(0);
    const text = detail()!.textContent!;
    expect(text).toContain("in the author's own words");
    expect(text).not.toContain("what it does");
    expect(text).not.toContain(t("plugins.noReadme"));
    expect(lastAsk()).toBe(false);
    // And the note under it says what was actually fetched, rather than promising a README.
    expect(text).toContain(t("plugins.figuresNote"));
    expect(text).not.toContain(t("plugins.factsNote"));
    // The figures are read either way: they are not what the description stood in for.
    expect(hoisted.asked).toEqual(["owner/plugin-0"]);
    expect(text).toContain("512");
  });

  // The same rule the one-line description follows (`AMB-D-623`): the reader's language, else the
  // author's own, and a fallback is never announced.
  it("draws the description in the reader's language where the author wrote one", () => {
    hoisted.catalog = catalogOf(3);
    hoisted.detail = describedAs({ about: "in English", aboutI18n: "作者の言葉で" });
    render();
    open(0);
    expect(detail()!.textContent).toContain("作者の言葉で");
    expect(detail()!.textContent).not.toContain("in English");
  });

  // Nothing is asked of GitHub until the catalog has answered, and the wait reads as loading: a screen
  // that said "no README" in between would be reporting a fetch that had not happened.
  it("waits for the catalog's answer before asking GitHub anything", () => {
    hoisted.catalog = catalogOf(3);
    hoisted.detail = undefined;
    render();
    open(0);
    expect(lastAsk()).toBe("unknown");
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
      { url: "https://official", name: "amenbo", fingerprint: "6272CBB782CB57A0", official: true, reachable: true, offered: 2 },
      { url: "https://third", name: "third", fingerprint: null, official: false, reachable: false, offered: 0 },
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

  // Every row says which key its plugins are trusted on, and a catalog with none says that instead —
  // it is the row nothing can be installed from (`AMB-D-389`).
  it("says what key each catalog is trusted on", () => {
    hoisted.catalog = twoSources;
    render();
    openPanel();
    expect(container.textContent).toContain("6272CBB782CB57A0");
    expect(container.textContent).toContain(t("plugins.sourceNoKey"));
  });

  const button = (label: string) =>
    Array.from(container.querySelectorAll("button")).find((b) => b.textContent === label)!;

  const typeUrl = async (url: string) => {
    const input = container.querySelector("input[type=url]") as HTMLInputElement;
    act(() => type(input, url));
    await act(async () => { button(t("plugins.addSource")).click(); });
  };

  // The load-bearing half of `AMB-D-389` on this screen: the button asks what registering would mean,
  // and nothing is written until the fingerprint has been shown and agreed to.
  it("shows the fingerprint before it registers anything, and pins the one it showed", async () => {
    hoisted.catalog = twoSources;
    render();
    openPanel();
    await typeUrl("  https://new.example/catalog.json  ");

    expect(hoisted.probed).toEqual(["https://new.example/catalog.json"]);
    expect(hoisted.added).toEqual([]);
    expect(container.textContent).toContain("6272CBB782CB57A0");
    expect(container.textContent).toContain(t("plugins.trustNote"));

    await act(async () => { button(t("plugins.trustAndAdd")).click(); });
    expect(hoisted.added).toEqual([{
      url: "https://new.example/catalog.json",
      name: "new.example",
      agreedFingerprint: "6272CBB782CB57A0",
    }]);
    expect((container.querySelector("input[type=url]") as HTMLInputElement).value).toBe("");
  });

  // The name is given on the same screen as the fingerprint, and defaults to the host that answered.
  it("registers under the name given while agreeing", async () => {
    hoisted.catalog = twoSources;
    render();
    openPanel();
    await typeUrl("https://new.example/catalog.json");
    const nameInput = container.querySelector(".catsrc__consent input[type=text]") as HTMLInputElement;
    act(() => type(nameInput, "社内カタログ"));
    await act(async () => { button(t("plugins.trustAndAdd")).click(); });
    expect(hoisted.added[0].name).toBe("社内カタログ");
  });

  // Backing out is a real answer, and it must leave the registration untouched.
  it("registers nothing when the consent is declined", async () => {
    hoisted.catalog = twoSources;
    render();
    openPanel();
    await typeUrl("https://new.example/catalog.json");
    await act(async () => { button(t("plugins.sourceCancel")).click(); });
    expect(hoisted.added).toEqual([]);
    expect(container.querySelector(".catsrc__consent")).toBeNull();
  });

  // A catalog that publishes no key asks for no trust — so the panel says what it costs (nothing can be
  // installed) rather than showing a fingerprint that does not exist.
  it("registers a catalog with no key without asking for trust", async () => {
    hoisted.catalog = twoSources;
    hoisted.probeFingerprint = null;
    render();
    openPanel();
    await typeUrl("https://new.example/catalog.json");
    expect(container.textContent).toContain(t("plugins.noKeyNote"));
    expect(container.querySelector(".catsrc__fp")).toBeNull();

    const consent = container.querySelector(".catsrc__consent")!;
    const add = Array.from(consent.querySelectorAll("button"))
      .find((b) => b.textContent === t("plugins.addSource"))!;
    await act(async () => { add.click(); });
    expect(hoisted.added).toEqual([{
      url: "https://new.example/catalog.json",
      name: "new.example",
      agreedFingerprint: undefined,
    }]);
  });

  // A URL core refuses (not http(s), the official catalog's own, or one whose key changed since it was
  // pinned) must land on screen, not in the console.
  it("shows why a rejected URL was rejected", async () => {
    hoisted.catalog = twoSources;
    hoisted.probeFails = true;
    render();
    openPanel();
    await typeUrl("ftp://nope");
    expect(container.textContent).toContain("plugin_catalog_url_invalid");
    expect(hoisted.added).toEqual([]);
    expect(container.querySelector(".catsrc__consent")).toBeNull();
  });
});
