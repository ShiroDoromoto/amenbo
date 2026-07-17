// @vitest-environment jsdom
// The wiring that pushes a language switch (config.language) through the whole UI at once. AppShell
// subscribes at the shell root with `useSyncExternalStore(subscribe, currentLang)` and remounts
// everything below it via `key={lang}`. Children that cache language-derived values in their own
// state (useQuery screens, memoized chrome) keep the old language across a plain re-render and pick
// up the new one only on the keyed remount — pinned here against a control that omits the key.
import { act, createElement, useState, useSyncExternalStore } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { applySnapshot, getSnapshot, subscribe } from "../core/snapshot";
import { currentLang, t } from "../core/i18n";

// React 18's act() requires this environment flag to be set.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// A child that bakes its translated label into useState at mount and never re-reads t() on a
// re-render — the model of a useQuery screen or memoized chrome that caches language-derived values.
// It re-localizes only when it is remounted.
function CachedChild() {
  const [label] = useState(() => t("topbar.back"));
  return createElement("span", { "data-testid": "label" }, label);
}

// The production wiring: subscribe to currentLang at the shell root and key the subtree by lang.
function KeyedShell() {
  const lang = useSyncExternalStore(subscribe, currentLang);
  return createElement("div", { key: lang }, createElement(CachedChild));
}

// The control: it subscribes and re-renders, but sets no key. The child is never remounted, so it keeps the language it cached.
function UnkeyedShell() {
  useSyncExternalStore(subscribe, currentLang);
  return createElement("div", null, createElement(CachedChild));
}

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  // Every test starts in Japanese (EMPTY carries language:null, and currentLang() then reads "ja").
  applySnapshot({ ...getSnapshot(), language: "ja" });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const label = () => container.querySelector('[data-testid="label"]')?.textContent;

describe("immediate reflection of a language switch (key remount)", () => {
  it("with key={lang}, even a child that caches language-derived values remounts to the new language on switch", () => {
    act(() => root.render(createElement(KeyedShell)));
    expect(label()).toBe(t("topbar.back", "ja")); // the Japanese label

    act(() => applySnapshot({ ...getSnapshot(), language: "en" }));
    expect(label()).toBe(t("topbar.back", "en")); // "Back" — the remount picks it up
    expect(label()).not.toBe(t("topbar.back", "ja"));
  });

  it("contrast: without a key, the child stays on its cached old language even after subscribing and re-rendering", () => {
    act(() => root.render(createElement(UnkeyedShell)));
    expect(label()).toBe(t("topbar.back", "ja"));

    act(() => applySnapshot({ ...getSnapshot(), language: "en" }));
    expect(label()).toBe(t("topbar.back", "ja"));
  });
});
