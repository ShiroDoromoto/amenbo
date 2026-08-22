// Dates, times and numbers — written by `Intl`, never by hand.
//
// A date's shape, a relative time's wording and a number's separators are per-locale rules, and the
// platform already carries all of them. Writing them here would mean carrying nineteen sets: nineteen
// month orders, nineteen ways to say "3 days ago", nineteen grouping separators. So nothing in this
// file spells any of that out — it picks the formatter and hands over the value.
//
// The locale is `dateLocale()`, the one tag the app already resolves: `config.date_locale` when it is
// set, else the one that goes with the language. It is passed as a parameter with that default so a
// test can pin it, and so a caller that already knows the locale does not resolve it twice.
//
// Formatters are cached because constructing one is the expensive half — these run inside render, one
// call per row — while selecting from a built one is cheap.
import { dateLocale } from "./lang";

/** Built once per (locale, shape). The key is the locale plus the options it was asked for. */
const dateTimeFormats = new Map<string, Intl.DateTimeFormat>();
const numberFormats = new Map<string, Intl.NumberFormat>();
const relativeFormats = new Map<string, Intl.RelativeTimeFormat>();

function dateTimeFormat(locale: string, options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = `${locale}|${JSON.stringify(options)}`;
  let f = dateTimeFormats.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(locale, options);
    dateTimeFormats.set(key, f);
  }
  return f;
}

/**
 * How this locale writes a quantity: which separator groups the thousands, and which one opens the
 * decimals. `1,234.5` in English is `1.234,5` in German and `1 234,5` in French, and Indian grouping
 * breaks at a different place again (`12,34,567`).
 *
 * Only for numbers a reader reads *as* a quantity. An id, a year and a step number are not: grouping
 * would put a separator through the middle of one.
 */
export function formatNumber(
  n: number,
  options?: Intl.NumberFormatOptions,
  locale: string = dateLocale(),
): string {
  const key = `${locale}|${options ? JSON.stringify(options) : ""}`;
  let f = numberFormats.get(key);
  if (!f) {
    f = new Intl.NumberFormat(locale, options);
    numberFormats.set(key, f);
  }
  return f.format(n);
}

/**
 * A relative time in words: `-3` days is "3 days ago", `1` day is "tomorrow".
 *
 * `numeric: "auto"` is what asks for the word where the language has one — English stops at
 * yesterday/today/tomorrow, while Japanese, German, Russian and Polish all carry a word for the day
 * after tomorrow as well. Counting the days out instead would throw those away.
 */
function relative(value: number, unit: Intl.RelativeTimeFormatUnit, locale: string): string {
  let f = relativeFormats.get(locale);
  if (!f) {
    f = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
    relativeFormats.set(locale, f);
  }
  return f.format(value, unit);
}

/**
 * How long ago, from a timestamp — the wording under every comment and activity line. The unit is the
 * largest one the gap fills, so a reader gets "2 hours ago" rather than a count of minutes.
 *
 * An unparseable timestamp renders as nothing: `Intl` throws on a `NaN`, and a broken row must not
 * take the screen down with it.
 */
export function agoLabel(at: string, locale: string = dateLocale(), now: number = Date.now()): string {
  const ms = new Date(at).getTime();
  if (!Number.isFinite(ms)) return "";
  return agoSecondsLabel((now - ms) / 1000, locale);
}

/**
 * The same wording from an age rather than an instant — what a backend reports when the fact it holds is
 * "this copy is that many seconds old", with no timestamp to hand.
 *
 * The unit is the largest one the gap fills, and anything shorter than a minute is "just now": the question
 * these answer is roughly how stale, not how many seconds.
 */
export function agoSecondsLabel(seconds: number, locale: string = dateLocale()): string {
  if (!Number.isFinite(seconds)) return "";
  const secs = Math.max(0, Math.floor(seconds));
  if (secs < 60) return relative(0, "second", locale);
  if (secs < 3600) return relative(-Math.floor(secs / 60), "minute", locale);
  if (secs < 86_400) return relative(-Math.floor(secs / 3600), "hour", locale);
  return relative(-Math.floor(secs / 86_400), "day", locale);
}

/**
 * The due chip's wording, from the bare date core holds. Days are counted in whole calendar days from
 * today, so "tomorrow" means the next date rather than 24 hours from now — which is what a due date
 * means to the person who set it.
 */
export function dueLabel(due: string, locale: string = dateLocale(), today: Date = new Date()): string {
  // A due date is a day. Anything a caller has attached to it is cut off first, the same way
  // `dueKind` colours the chip, so the two never disagree about which day this is.
  const at = new Date(`${due.slice(0, 10)}T00:00:00`);
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const diff = Math.round((at.getTime() - start.getTime()) / 86_400_000);
  if (!Number.isFinite(diff)) return "";
  return relative(diff, "day", locale);
}

/** A calendar date — the day a decision was settled on, and anything else dated to the day. */
export function formatDay(at: Date, locale: string = dateLocale()): string {
  if (!Number.isFinite(at.getTime())) return "";
  return dateTimeFormat(locale, { year: "numeric", month: "numeric", day: "numeric" }).format(at);
}

/** A day and a time, without the year — for a timestamp inside the span a reader is looking at. */
export function formatDayTime(at: Date, locale: string = dateLocale()): string {
  if (!Number.isFinite(at.getTime())) return "";
  return dateTimeFormat(locale, {
    month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit",
  }).format(at);
}

/**
 * A dated instant — the day *and* the year, with the time. `formatDayTime` leaves the year out for a
 * stamp inside the span the reader is already looking at; this one is for a stamp that can be any
 * distance back, where "8/22 12:02" would not say which year it was.
 */
export function formatStamp(at: Date, locale: string = dateLocale()): string {
  if (!Number.isFinite(at.getTime())) return "";
  return dateTimeFormat(locale, {
    year: "numeric", month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit",
  }).format(at);
}

/** The calendar's month heading (month is 0..11, as in `getMonth`). */
export function monthLabel(year: number, month: number, locale: string = dateLocale()): string {
  return dateTimeFormat(locale, { year: "numeric", month: "long", timeZone: "UTC" })
    .format(new Date(Date.UTC(year, month, 1)));
}

/** A Sunday, so that formatting seven days from it walks a week in order. */
const WEEK_ANCHOR_SUNDAY = Date.UTC(2024, 0, 7);

/**
 * The calendar's weekday headings, in the locale's own abbreviations (`weekStart=0` starts the week
 * on Sunday). Formatted off a week that starts on a Sunday rather than named — the abbreviation is
 * two letters in some languages and three in others, and one character in Japanese and Chinese.
 */
export function weekdayLabels(weekStart = 0, locale: string = dateLocale()): string[] {
  const f = dateTimeFormat(locale, { weekday: "short", timeZone: "UTC" });
  const week = Array.from({ length: 7 }, (_, i) => f.format(new Date(WEEK_ANCHOR_SUNDAY + i * 86_400_000)));
  return [...week.slice(weekStart), ...week.slice(0, weekStart)];
}
