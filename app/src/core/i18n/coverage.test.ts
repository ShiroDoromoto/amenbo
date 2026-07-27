// The untranslated-key gate.
//
// A missing key goes quiet at runtime by design: the reader sees the English string and the screen
// stays whole. That is what makes it dangerous here — nothing on screen, and nothing in a log, says
// how much of a language is actually translated. So it is counted at build time instead: a language
// that ships a dictionary covers every key English has, or this fails and names what is missing.
//
// The reference is what `en` actually holds, not what a type would allow — `err` carries templates
// for only the codes whose sentence can be rebuilt from structured fields, and a translator has no
// business filling in the rest.
//
// A language with no dictionary file is not a gap: it resolves to English everywhere, deliberately,
// and there is nothing to count. This gate is about a file that exists and is half-written.
import { describe, expect, it } from "vitest";
import { DICTIONARIES } from "./index";
import type { Translation } from "./keys";
import { en } from "./locales/en";

const SECTIONS = ["status", "priority", "view", "ui", "err", "doctor"] as const;

/** The keys English has and this dictionary does not, as "section: key". */
function untranslated(dict: Translation): string[] {
  const gaps: string[] = [];
  for (const section of SECTIONS) {
    const theirs = dict[section] as Record<string, unknown>;
    for (const key of Object.keys(en[section])) {
      if (theirs[key] === undefined) gaps.push(`${section}: ${key}`);
    }
  }
  return gaps;
}

describe("every dictionary this build carries is fully translated", () => {
  for (const [lang, dict] of Object.entries(DICTIONARIES)) {
    it(`${lang} has a string for every key English has`, () => {
      const gaps = untranslated(dict);
      // The count leads: a language that lands half-machine-translated shows as "312 keys", and the
      // list underneath says which ones.
      expect(gaps, `${lang}: ${gaps.length} untranslated key(s)`).toEqual([]);
    });
  }
});
