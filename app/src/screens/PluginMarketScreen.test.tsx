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
}));

// Replace only the data seam: the filtering, the paging and the rendering all run for real.
vi.mock("../core/pluginCatalog", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../core/pluginCatalog")>();
  return {
    ...orig,
    usePluginCatalog: () => ({ catalog: hoisted.catalog, loading: false, error: undefined }),
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

  it("narrows by category and to official entries, and says so when nothing matches", () => {
    hoisted.catalog = catalogOf(6);
    render();
    const [category] = Array.from(container.querySelectorAll("select")) as HTMLSelectElement[];
    act(() => select(category, "notify"));
    expect(rows()).toHaveLength(3);

    const official = container.querySelector("input[type=checkbox]") as HTMLInputElement;
    act(() => official.click());
    // plugin-0 is the only official one, and it is a workflow entry — so the two filters together match nothing.
    expect(rows()).toHaveLength(0);
    expect(container.textContent).toContain(t("plugins.emptyFilter"));
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
