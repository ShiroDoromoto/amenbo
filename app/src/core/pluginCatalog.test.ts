import { describe, expect, it } from "vitest";
import { filterPlugins, pluginCategories, unreachableSources, type PluginEntry } from "./pluginCatalog";

const entry = (over: Partial<PluginEntry>): PluginEntry => ({
  name: "worktree",
  desc: "cut a git worktree per task",
  author: "amenbo",
  repo: "ShiroDoromoto/amenbo",
  os: ["macos", "linux", "windows"],
  category: "workflow",
  official: true,
  ...over,
});

describe("filterPlugins", () => {
  const entries = [
    entry({}),
    entry({ name: "slack", desc: "post to a channel", author: "someone", category: "notify", official: false, os: ["macos"] }),
    entry({ name: "winonly", desc: "windows helper", author: "someone", category: "workflow", official: false, os: ["windows"] }),
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

  it("narrows by category, by OS support and to official entries", () => {
    expect(filterPlugins(entries, { category: "notify" }).map((e) => e.name)).toEqual(["slack"]);
    expect(filterPlugins(entries, { os: "windows" }).map((e) => e.name)).toEqual(["worktree", "winonly"]);
    expect(filterPlugins(entries, { officialOnly: true }).map((e) => e.name)).toEqual(["worktree"]);
  });

  // Every control narrows the same list, so two of them together narrow further rather than widening.
  it("combines the controls", () => {
    expect(filterPlugins(entries, { category: "workflow", os: "windows", officialOnly: true }).map((e) => e.name))
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
        { url: "https://official", official: true, reachable: true, offered: 3 },
        { url: "https://third", official: false, reachable: false, offered: 0 },
      ],
      dropped: 0,
    };
    expect(unreachableSources(catalog)).toEqual(["https://third"]);
  });
});
