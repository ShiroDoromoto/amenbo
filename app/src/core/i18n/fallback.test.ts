// A language file holds what has been translated, and nothing forces it to be complete — so the
// question that decides whether the screen survives is what happens to a key it does not carry.
// The answer is English, and it is tested through the front door: t() on a key the language is
// missing. The tests take a key out of the Japanese dictionary to make that state, since Japanese
// is currently translated in full.
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../snapshot", () => ({ getSnapshot: () => ({ language: "ja", dateLocale: null }) }));

import { statusLabel, t, tf } from "./index";
import { en } from "./locales/en";
import { ja } from "./locales/ja";

const missing: Array<() => void> = [];

/** Drops a key from the Japanese dictionary for one test, as an untranslated language would have it. */
function untranslate<S extends "ui" | "status">(section: S, key: keyof (typeof ja)[S]) {
  const held = ja[section][key];
  delete ja[section][key];
  missing.push(() => {
    ja[section][key] = held;
  });
}

afterEach(() => {
  while (missing.length) missing.pop()!();
});

describe("the English fallback", () => {
  it("renders the language's own string when it has one", () => {
    expect(t("topbar.refresh", "ja")).toBe(ja.ui["topbar.refresh"]);
    expect(t("topbar.refresh", "en")).toBe(en.ui["topbar.refresh"]);
  });

  it("renders an untranslated key in English", () => {
    untranslate("ui", "topbar.refresh");
    expect(t("topbar.refresh", "ja")).toBe(en.ui["topbar.refresh"]);
  });

  it("interpolates into the English template too", () => {
    untranslate("ui", "cal.more");
    expect(tf("cal.more", { n: 3 }, "ja")).toBe("+3 more");
  });

  it("covers the labels that are not UI chrome", () => {
    untranslate("status", "todo");
    expect(statusLabel("todo", "ja")).toBe(en.status.todo);
  });

  // The one hole nothing can fill: a key no language has. It shows as itself rather than as blank,
  // so a typo in a call site is visible on screen instead of silently erasing the label.
  it("renders a key no language has as the key itself", () => {
    expect(t("nowhere.atall", "ja")).toBe("nowhere.atall");
    expect(t("nowhere.atall", "en")).toBe("nowhere.atall");
  });

  it("takes the current language from the snapshot when none is passed", () => {
    expect(t("topbar.refresh")).toBe(ja.ui["topbar.refresh"]);
  });
});
