// The two gates on a translation: is it there, and does it still carry the values the sentence is
// about.
//
// Both failures are silent at runtime by design. A missing key falls back to English, so the screen
// stays whole and nothing says how much of a language is actually translated. A mangled `{name}`
// throws nothing either — the sentence simply comes out with a hole where the id, the path or the
// count should have been, or with `{name}` printed at the reader. Neither shows up in a log, so both
// are counted at build time instead.
//
// The reference is what `en` actually holds, not what a type would allow — `err` carries templates
// for only the codes whose sentence can be rebuilt from structured fields, and a translator has no
// business filling in the rest.
//
// A language with no dictionary file is not a gap: it resolves to English everywhere, deliberately,
// and there is nothing to count. These gates are about a file that exists and is half-written, or
// written by a machine that translated the placeholder along with the prose.
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

/**
 * Every string a dictionary renders, by where it sits. A doctor entry is two sentences under one
 * key, so each is listed on its own — they are interpolated separately and can break separately.
 */
function strings(dict: Translation): Map<string, string> {
  const found = new Map<string, string>();
  for (const section of SECTIONS) {
    for (const [key, value] of Object.entries(dict[section] as Record<string, unknown>)) {
      if (typeof value === "string") found.set(`${section}: ${key}`, value);
      else if (value && typeof value === "object") {
        for (const [part, sentence] of Object.entries(value as Record<string, string>)) {
          found.set(`${section}: ${key}.${part}`, sentence);
        }
      }
    }
  }
  return found;
}

/** The `{name}` placeholders in a template, deduplicated and ordered so two can be compared. */
function placeholders(template: string): string[] {
  return [...new Set([...template.matchAll(/\{(\w+)\}/g)].map((m) => m[1]))].sort();
}

/**
 * Where a translated string stopped carrying what English carries. Only strings the dictionary
 * actually holds are judged — a key it has not translated is the other gate's business.
 */
function driftedTemplates(dict: Translation): string[] {
  const theirs = strings(dict);
  const drift: string[] = [];
  for (const [at, english] of strings(en)) {
    const translated = theirs.get(at);
    if (translated === undefined) continue;
    const want = placeholders(english);
    const got = placeholders(translated);
    const lost = want.filter((p) => !got.includes(p));
    const invented = got.filter((p) => !want.includes(p));
    if (lost.length) drift.push(`${at}: drops {${lost.join("}, {")}}`);
    if (invented.length) drift.push(`${at}: adds {${invented.join("}, {")}}`);
  }
  return drift;
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

describe("every translated sentence still carries its values", () => {
  for (const [lang, dict] of Object.entries(DICTIONARIES)) {
    // A placeholder is not a word: dropping one loses the id / path / count the sentence exists to
    // report, and inventing one puts `{name}` on the screen, since nothing fills a name the caller
    // was never asked for.
    it(`${lang} interpolates the same values English does`, () => {
      const drift = driftedTemplates(dict);
      expect(drift, `${lang}: ${drift.length} template(s) out of step`).toEqual([]);
    });
  }
});
