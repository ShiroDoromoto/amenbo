// Which of the nineteen a code is read as. The store keeps `config.language` opaque and the
// platform hands over whatever it likes (`navigator.language`), so this is the one place that
// decides — and the one place that decides what happens when the answer is none of them.
import { describe, expect, it, vi } from "vitest";

vi.mock("../snapshot", () => ({ getSnapshot: () => ({ language: null, dateLocale: null }) }));

import { DEFAULT_LANG, LANGS, normalizeLang } from "./index";

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
