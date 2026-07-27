// Which language the UI is in, and which locale it writes dates in. The language is looked up from
// snapshot.language on every call (getSnapshot is a synchronous cache read, so it is safe to call
// during render).
import { getSnapshot } from "../snapshot";

export type Lang = "ja" | "en";

/** Normalizes a BCP-47-ish code to a supported language; unset or unsupported means `ja`. */
export function normalizeLang(code?: string | null): Lang {
  return code?.toLowerCase().startsWith("en") ? "en" : "ja";
}

/** The current UI language (snapshot.language, normalized). */
export function currentLang(): Lang {
  return normalizeLang(getSnapshot().language);
}

/** The locale each language writes its dates in, when nothing else is asked for. */
const LANG_LOCALE: Record<Lang, string> = { ja: "ja-JP", en: "en-US" };

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
