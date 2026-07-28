// Which of the nineteen a code is read as. The store keeps `config.language` opaque and the
// platform hands over whatever it likes (`navigator.language`), so this is the one place that
// decides — and the one place that decides what happens when the answer is none of them.
import { describe, expect, it, vi } from "vitest";

vi.mock("../snapshot", () => ({ getSnapshot: () => ({ language: null, dateLocale: null }) }));

import { DEFAULT_LANG, guessLang, langEndonym, LANGS, normalizeLang } from "./index";

describe("normalizeLang", () => {
  it("carries nineteen languages, and English is where they fall back to", () => {
    expect(LANGS).toHaveLength(19);
    expect(DEFAULT_LANG).toBe("en");
    expect(new Set(LANGS).size).toBe(LANGS.length);
  });

  it("reads every supported code as itself, whatever case it arrives in", () => {
    for (const lang of LANGS) {
      expect(normalizeLang(lang), lang).toBe(lang);
      expect(normalizeLang(lang.toLowerCase()), lang).toBe(lang);
      expect(normalizeLang(lang.replace("-", "_")), lang).toBe(lang);
    }
  });

  // What the platform actually hands over. `navigator.language` names Chinese by region, not by
  // script, so a reader in Taiwan has to reach Traditional rather than falling to English.
  it("narrows a region onto the script it is written in", () => {
    expect(normalizeLang("zh-TW")).toBe("zh-Hant");
    expect(normalizeLang("zh-HK")).toBe("zh-Hant");
    expect(normalizeLang("zh-CN")).toBe("zh-Hans");
    // A bare `zh` is Simplified in practice, which is the better guess than no Chinese at all.
    expect(normalizeLang("zh")).toBe("zh-Hans");
  });

  it("reads a Portuguese of any region as the one Portuguese carried", () => {
    expect(normalizeLang("pt")).toBe("pt-BR");
    expect(normalizeLang("pt-PT")).toBe("pt-BR");
  });

  it("drops a region it does not name and keeps the language", () => {
    expect(normalizeLang("de-AT")).toBe("de");
    expect(normalizeLang("en-GB")).toBe("en");
    expect(normalizeLang("fr-CA")).toBe("fr");
  });

  // The inversion this is here for: showing Japanese to someone who did not ask for it is the one
  // answer that cannot be right, so nothing falls that way any more.
  it("falls to English for anything unset or unrecognized, never to Japanese", () => {
    for (const code of [undefined, null, "", "   ", "kl", "not a language", "-", "xx-YY"]) {
      expect(normalizeLang(code), String(code)).toBe("en");
    }
  });
});

// What someone is started in before they have picked. It only fills the language step of first-run
// setup — a language already chosen is never guessed over — so being wrong costs one click, while
// asking "which language do you read?" in a language they cannot read costs the whole screen.
describe("guessLang", () => {
  it("takes the reader's first preference that we carry", () => {
    expect(guessLang(["ja-JP", "en-US"])).toBe("ja");
    expect(guessLang(["fr-CA"])).toBe("fr");
    expect(guessLang(["zh-TW", "zh-CN"])).toBe("zh-Hant");
  });

  // The head of the list is often a language nobody has translated, while the next one down is one
  // the reader also reads. Stopping at the head would send them to English past a better answer.
  it("reads past a preference we do not carry to one we do", () => {
    expect(guessLang(["ca", "es-ES", "en"])).toBe("es");
    expect(guessLang(["gd", "cy", "de"])).toBe("de");
  });

  it("is English when nothing in the list is carried, and when there is no list", () => {
    expect(guessLang(["ca", "eu", "gl"])).toBe("en");
    expect(guessLang([])).toBe("en");
  });
});

// The picker's labels. Nothing else in the app is left untranslated on purpose: the reader looking
// at this list has not chosen a language yet, so each line has to be readable to the one person it
// is for.
describe("langEndonym", () => {
  it("names every language, in that language's own script", () => {
    for (const lang of LANGS) {
      expect(langEndonym(lang).trim(), lang).not.toBe("");
    }
    expect(langEndonym("ja")).toBe("日本語");
    expect(langEndonym("en")).toBe("English");
    expect(langEndonym("uk")).toBe("Українська");
  });

  // Chinese and Portuguese are carried as two entries and one; a label that named only the language
  // would leave the reader choosing between two identical lines.
  it("says which Chinese and which Portuguese each line is", () => {
    expect(langEndonym("zh-Hans")).not.toBe(langEndonym("zh-Hant"));
    expect(langEndonym("pt-BR")).toContain("Brasil");
  });
});
