import { describe, expect, it } from "vitest";
import {
  filterPlugins, pluginCategories, pluginLayer, sortPlugins, unreachableSources, type PluginEntry,
} from "./pluginCatalog";

const entry = (over: Partial<PluginEntry>): PluginEntry => ({
  name: "worktree",
  desc: "cut a git worktree per task",
  author: "amenbo",
  repo: "ShiroDoromoto/amenbo",
  os: ["macos", "linux", "windows"],
  category: "workflow",
  official: true,
  listed: true,
  featured: false,
  ...over,
});

describe("filterPlugins", () => {
  const entries = [
    entry({}),
    entry({ name: "slack", desc: "post to a channel", author: "someone", category: "notify", official: false, os: ["macos"] }),
    entry({ name: "winonly", desc: "windows helper", author: "someone", category: "workflow", official: false, os: ["windows"], listed: false }),
  ];

  it("keeps everything when nothing is asked of it", () => {
    expect(filterPlugins(entries, {}).map((e) => e.name)).toEqual(["worktree", "slack", "winonly"]);
  });

  // The search covers the three fields a user knows a plugin by. Description and author matter as much as
  // the name: a catalog is browsed by what a plugin *does*, not only by what it is called.
  it("searches the name, the description and the author, ignoring case", () => {
    expect(filterPlugins(entries, { q: "WORKtree" }).map((e) => e.name)).toEqual(["worktree"]);
    expect(filterPlugins(entries, { q: "channel" }).map((e) => e.name)).toEqual(["slack"]);
    expect(filterPlugins(entries, { q: "someone" }).map((e) => e.name)).toEqual(["slack", "winonly"]);
  });

  it("treats blank search as no search", () => {
    expect(filterPlugins(entries, { q: "   " })).toHaveLength(3);
  });

  it("narrows by category and by OS support", () => {
    expect(filterPlugins(entries, { category: "notify" }).map((e) => e.name)).toEqual(["slack"]);
    expect(filterPlugins(entries, { os: "windows" }).map((e) => e.name)).toEqual(["worktree", "winonly"]);
  });

  // The layers nest, so "listed" has to keep the official entries: they passed the same review and more.
  it("narrows by trust layer, keeping the official entries inside listed", () => {
    expect(filterPlugins(entries, { layer: "official" }).map((e) => e.name)).toEqual(["worktree"]);
    expect(filterPlugins(entries, { layer: "listed" }).map((e) => e.name)).toEqual(["worktree", "slack"]);
    expect(filterPlugins(entries, { layer: "third-party" }).map((e) => e.name)).toEqual(["winonly"]);
  });

  // Every control narrows the same list, so two of them together narrow further rather than widening.
  it("combines the controls", () => {
    expect(filterPlugins(entries, { category: "workflow", os: "macos", layer: "official" }).map((e) => e.name))
      .toEqual(["worktree"]);
    expect(filterPlugins(entries, { category: "notify", os: "windows" })).toEqual([]);
  });
});

describe("pluginCategories", () => {
  // The vocabulary is the catalog's, not ours: the choices are whatever the entries carry, de-duplicated.
  it("lists each category once, sorted", () => {
    const entries = [entry({}), entry({ name: "b", category: "notify" }), entry({ name: "c", category: "workflow" })];
    expect(pluginCategories(entries)).toEqual(["notify", "workflow"]);
  });
});

describe("unreachableSources", () => {
  // A source that answered from neither the network nor a cache contributed nothing — the list is short by
  // however much it holds, and saying so is the difference between "no plugins" and "we could not look".
  it("names only the catalogs that did not answer", () => {
    const catalog = {
      entries: [],
      sources: [
        { url: "https://official", name: "amenbo", fingerprint: "6272CBB782CB57A0", official: true, reachable: true, offered: 3 },
        { url: "https://third", name: "third", fingerprint: null, official: false, reachable: false, offered: 0 },
      ],
      dropped: 0,
    };
    expect(unreachableSources(catalog)).toEqual(["https://third"]);
  });
});

describe("pluginLayer", () => {
  // Two independent facts, one ladder: who wrote it, and who reviewed it onto the official index.
  it("reads the layer off the two flags", () => {
    expect(pluginLayer(entry({}))).toBe("official");
    expect(pluginLayer(entry({ official: false }))).toBe("listed");
    expect(pluginLayer(entry({ official: false, listed: false }))).toBe("third-party");
  });
});

describe("sortPlugins", () => {
  const dated = [
    entry({ name: "old", addedAt: "2026-01-01T00:00:00Z" }),
    entry({ name: "undated" }),
    entry({ name: "new", addedAt: "2026-07-01T00:00:00Z" }),
  ];

  it("puts the newest first", () => {
    expect(sortPlugins(dated, "new").map((e) => e.name)).toEqual(["new", "old", "undated"]);
  });

  // A catalog that never wrote the field says the date is unknown, not that the plugin is ancient.
  it("sinks an entry with no date rather than dating it to the epoch", () => {
    const ordered = sortPlugins(dated, "new");
    expect(ordered[ordered.length - 1].name).toBe("undated");
  });

  it("sorts by name on the other ordering, and never mutates its input", () => {
    expect(sortPlugins(dated, "name").map((e) => e.name)).toEqual(["new", "old", "undated"]);
    expect(dated.map((e) => e.name)).toEqual(["old", "undated", "new"]);
  });

  // The curation is a flag, not a ranking: it says which plugins are recommended and nothing about
  // their order, so the recommended ones rise as a block and the "new" rule orders them inside it.
  it("lifts the recommended entries as a block, newest first inside it", () => {
    const mixed = [
      entry({ name: "plain-new", addedAt: "2026-07-01T00:00:00Z" }),
      entry({ name: "picked-old", addedAt: "2026-01-01T00:00:00Z", featured: true }),
      entry({ name: "picked-new", addedAt: "2026-06-01T00:00:00Z", featured: true }),
    ];
    expect(sortPlugins(mixed, "featured").map((e) => e.name)).toEqual([
      "picked-new", "picked-old", "plain-new",
    ]);
  });

  // Nothing is hidden by the ordering: a plugin no one got round to recommending is still in the list.
  it("keeps every entry when nothing is recommended, which is then just the newest ordering", () => {
    expect(sortPlugins(dated, "featured").map((e) => e.name)).toEqual(["new", "old", "undated"]);
  });
});
