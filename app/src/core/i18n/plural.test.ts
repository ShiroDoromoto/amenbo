// Which arm a counted sentence comes out in. The rules belong to the platform (`Intl.PluralRules`),
// so what is worth pinning is that we ask it rather than counting to one ourselves — and what
// happens for a language whose arms are not all translated yet.
import { describe, expect, it, vi } from "vitest";

vi.mock("../snapshot", () => ({ getSnapshot: () => ({ language: null, dateLocale: null }) }));

import { pluralCategory, tn } from "./index";
import { ru } from "./locales/ru";

describe("pluralCategory", () => {
  it("splits English at one and leaves Japanese undivided", () => {
    expect(pluralCategory(1, "en")).toBe("one");
    expect(pluralCategory(0, "en")).toBe("other");
    expect(pluralCategory(2, "en")).toBe("other");
    for (const n of [0, 1, 2, 5, 21]) expect(pluralCategory(n, "ja"), String(n)).toBe("other");
  });

  // The reason this is not a comparison with 1: Russian takes a third form at two-through-four and a
  // fourth from five, and neither is anything an English-shaped rule would reach.
  it("gives Russian the forms Russian has", () => {
    expect(pluralCategory(1, "ru")).toBe("one");
    expect(pluralCategory(3, "ru")).toBe("few");
    expect(pluralCategory(7, "ru")).toBe("many");
    expect(pluralCategory(11, "ru")).toBe("many");
  });
});

describe("tn", () => {
  it("takes the arm the language asks for, with the count filled in", () => {
    expect(tn("act.nTasks", 1, "en")).toBe("1 task");
    expect(tn("act.nTasks", 4, "en")).toBe("4 tasks");
    expect(tn("act.nTasks", 1, "ja")).toBe("タスク1件");
  });

  // Three asks Russian for `few`. Take that arm away, as a dictionary that stopped after `one` and
  // `other` would have it, and the count lands on the language's own `other` — a sentence that
  // reads a little wrong, rather than a key at the reader.
  it("falls to the other arm when the one asked for is not written", () => {
    const held = ru.ui["act.nTasks.few"];
    delete ru.ui["act.nTasks.few"];
    try {
      expect(tn("act.nTasks", 3, "ru")).toBe(ru.ui["act.nTasks.other"]!.replace("{n}", "3"));
    } finally {
      ru.ui["act.nTasks.few"] = held;
    }
  });

  // The one thing a fallback must never do is print the key at the reader.
  it("never leaves a bare key on screen", () => {
    expect(tn("act.nTasks", 3, "pl")).not.toContain("act.nTasks");
    expect(tn("act.nTasks", 3, "uk")).not.toContain("act.nTasks");
  });
});
