// Which language the UI is in, and which locale it writes dates in. The language is looked up from
// snapshot.language on every call (getSnapshot is a synchronous cache read, so it is safe to call
// during render).
import { getSnapshot } from "../snapshot";

/**
 * The languages the UI can be read in. English leads because it is where everything falls back to:
 * an unset language, a code from outside this list, and a key a dictionary has not translated all
 * end there.
 *
 * Chinese and Portuguese are carried by script and region rather than by language alone. Simplified
 * and Traditional are separate writing systems, and Brazilian Portuguese is a separate vocabulary —
 * a machine pass between either pair does not produce what a reader would have written, so each is
 * its own dictionary.
 */
export const LANGS = [
  "en", "ja", "zh-Hans", "zh-Hant", "ko", "es", "pt-BR", "fr", "de", "it", "ru",
  "hi", "id", "vi", "th", "tr", "pl", "nl", "uk",
] as const;

export type Lang = (typeof LANGS)[number];

/** Where everything falls back to: an unset setting, an unknown code, an untranslated key. */
export const DEFAULT_LANG: Lang = "en";

/** Lowercased code → the language it reads as, for the two that carry a script or a region. */
const BY_SUBTAG: Record<string, Lang> = {
  // Simplified is what a bare `zh` means in practice, and what the mainland regions write.
  zh: "zh-Hans", "zh-hans": "zh-Hans", "zh-cn": "zh-Hans", "zh-sg": "zh-Hans",
  "zh-hant": "zh-Hant", "zh-tw": "zh-Hant", "zh-hk": "zh-Hant", "zh-mo": "zh-Hant",
  // Brazilian Portuguese is the only Portuguese carried, so a European code reads as it rather than
  // falling all the way to English.
  pt: "pt-BR", "pt-br": "pt-BR", "pt-pt": "pt-BR",
};

/** Lowercased code → the language, for the sixteen that need no narrowing. */
const BY_PRIMARY: Record<string, Lang> = Object.fromEntries(
  LANGS.filter((l) => !l.includes("-")).map((l) => [l, l]),
);

/**
 * Normalizes a BCP-47-ish code to a supported language. An unset or unrecognized code is English —
 * a language nobody asked for is a worse answer than the one everything else falls back to.
 *
 * A code is read from the most specific match down: the whole thing (`zh-Hant`), then its first two
 * subtags (`zh-TW`), then its language alone (`de-AT` → `de`). That is what lets a code the platform
 * hands over — `navigator.language` gives `zh-TW`, not `zh-Hant` — land on the dictionary that
 * matches it.
 */
export function normalizeLang(code?: string | null): Lang {
  const subtags = code?.trim().toLowerCase().split(/[-_]/).filter(Boolean) ?? [];
  if (subtags.length === 0) return DEFAULT_LANG;
  const [primary, secondary] = subtags;
  const narrowed = secondary ? `${primary}-${secondary}` : primary;
  return BY_SUBTAG[narrowed] ?? BY_SUBTAG[primary] ?? BY_PRIMARY[primary] ?? DEFAULT_LANG;
}

/** The current UI language (snapshot.language, normalized). */
export function currentLang(): Lang {
  return normalizeLang(getSnapshot().language);
}

/**
 * The locale each language writes its dates in, when nothing else is asked for. A language names
 * words and a locale names formats, so each is paired with the region whose conventions its readers
 * are likeliest to expect.
 */
const LANG_LOCALE: Record<Lang, string> = {
  en: "en-US", ja: "ja-JP", "zh-Hans": "zh-Hans-CN", "zh-Hant": "zh-Hant-TW", ko: "ko-KR",
  es: "es-ES", "pt-BR": "pt-BR", fr: "fr-FR", de: "de-DE", it: "it-IT", ru: "ru-RU",
  hi: "hi-IN", id: "id-ID", vi: "vi-VN", th: "th-TH", tr: "tr-TR", pl: "pl-PL", nl: "nl-NL",
  uk: "uk-UA",
};

/**
 * The locale dates are written in — `config.date_locale` when it is set, otherwise the one that
 * goes with the language.
 *
 * Two settings, because they answer different questions: the language decides the words, this
 * decides the date's shape. They agree for most people, which is why the second one is normally
 * unset; it exists for the reader whose answers differ — a Japanese UI with ISO dates (`sv-SE`),
 * say.
 *
 * A tag the platform cannot use falls back to the language's rather than throwing: `Intl` rejects a
 * malformed tag with a `RangeError`, and nothing is stopping a typo from reaching here — the store
 * keeps the value opaque, since what is a usable locale is the formatter's judgement and not
 * something amenbo can settle when the value is written.
 */
export function dateLocale(): string {
  const fallback = LANG_LOCALE[currentLang()];
  const declared = getSnapshot().dateLocale?.trim();
  if (!declared) return fallback;
  try {
    return Intl.DateTimeFormat.supportedLocalesOf(declared).length > 0 ? declared : fallback;
  } catch {
    return fallback;
  }
}
