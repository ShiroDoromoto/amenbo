// What `Intl` is being asked for, pinned in a few languages that disagree with English about it.
//
// The point of these is not to re-test the platform's tables — it is to show that the right formatter
// is being asked the right question: the largest unit that fits, the word a language has for the day
// after tomorrow, the separator that groups thousands. Get the question wrong and every language is
// wrong at once, silently, because nothing falls back and nothing is missing.
import { beforeEach, describe, expect, it, vi } from "vitest";

const snap = { language: null as string | null, dateLocale: null as string | null };
vi.mock("../snapshot", () => ({ getSnapshot: () => snap }));

import {
  agoLabel, dueLabel, formatDay, formatDayTime, formatNumber, formatStamp, monthLabel, weekdayLabels,
} from "./format";
import { tf, tn } from "./index";
import { de } from "./locales/de";

const NOW = new Date("2026-06-21T12:00:00Z").getTime();
const ago = (secs: number) => new Date(NOW - secs * 1000).toISOString();

beforeEach(() => {
  snap.language = null;
  snap.dateLocale = null;
});

describe("how long ago", () => {
  it("words the gap in the largest unit that fits", () => {
    expect(agoLabel(ago(5), "en-US", NOW)).toBe("now");
    expect(agoLabel(ago(60), "en-US", NOW)).toBe("1 minute ago");
    expect(agoLabel(ago(120), "en-US", NOW)).toBe("2 minutes ago");
    expect(agoLabel(ago(3600), "en-US", NOW)).toBe("1 hour ago");
    expect(agoLabel(ago(86_400 * 3), "en-US", NOW)).toBe("3 days ago");
    expect(agoLabel(ago(120), "ja-JP", NOW)).toMatch(/^2\s?分前$/);
  });

  // A row written a moment ago can carry a timestamp a hair ahead of this clock. "in -0 minutes"
  // would be the tell; the gap is floored at zero instead.
  it("does not run backwards on a timestamp from just ahead", () => {
    expect(agoLabel(new Date(NOW + 2000).toISOString(), "en-US", NOW)).toBe("now");
  });

  // `Intl` throws a RangeError on a NaN, and one unreadable row must not take the screen with it.
  it("renders nothing for a timestamp it cannot read", () => {
    expect(agoLabel("not a timestamp", "en-US", NOW)).toBe("");
  });
});

describe("the due chip", () => {
  // Local noon, so the day is the same one whatever timezone the test runs in.
  const today = new Date(2026, 5, 21, 12, 0, 0);

  it("counts whole calendar days, so tomorrow is the next date", () => {
    expect(dueLabel("2026-06-21", "en-US", today)).toBe("today");
    expect(dueLabel("2026-06-22", "en-US", today)).toBe("tomorrow");
    expect(dueLabel("2026-06-20", "en-US", today)).toBe("yesterday");
    expect(dueLabel("2026-06-23", "en-US", today)).toBe("in 2 days");
    expect(dueLabel("2026-06-18", "en-US", today)).toBe("3 days ago");
  });

  // The reason the wording is not counted out by hand: several of the nineteen have a word here that
  // English does not, and writing "in 2 days" in every language would throw it away.
  it("takes the word a language has for the day after tomorrow", () => {
    expect(dueLabel("2026-06-23", "ja-JP", today)).toBe("明後日");
    expect(dueLabel("2026-06-23", "de-DE", today)).toBe("übermorgen");
  });

  // The chip colours by the day alone (`dueKind`); the wording has to cut the same way, or a date
  // carrying a time would read "tomorrow" under a chip coloured for today.
  it("judges by the day even when a time is attached", () => {
    expect(dueLabel("2026-06-21T23:00:00Z", "en-US", today)).toBe("today");
  });

  it("renders nothing for a date it cannot read", () => {
    expect(dueLabel("", "en-US", today)).toBe("");
  });
});

describe("a quantity", () => {
  it("is grouped the way the locale groups one", () => {
    expect(formatNumber(1_234_567, undefined, "en-US")).toBe("1,234,567");
    expect(formatNumber(1_234_567, undefined, "de-DE")).toBe("1.234.567");
    // Indian grouping does not break every three digits, which is the case a hand-written
    // "insert a comma every three" gets wrong.
    expect(formatNumber(1_234_567, undefined, "hi-IN")).toBe("12,34,567");
  });

  it("takes the locale's decimal separator", () => {
    const opts = { minimumFractionDigits: 1, maximumFractionDigits: 1 };
    expect(formatNumber(1.5, opts, "en-US")).toBe("1.5");
    expect(formatNumber(1.5, opts, "de-DE")).toBe("1,5");
  });

  // Every number a sentence interpolates is a quantity, so the one place they all pass through does
  // this — no caller has to remember to.
  it("is formatted on its way into a sentence", () => {
    // The words come from the language and the digits from the locale, and they are separate
    // answers — so the German number survives a sentence German has no translation for. The two
    // keys are taken out of the German dictionary to make that state, since the language is
    // translated in full.
    snap.language = "de";
    const held = { more: de.ui["cal.more"], tasks: de.ui["act.nTasks.other"] };
    delete de.ui["cal.more"];
    delete de.ui["act.nTasks.other"];
    try {
      expect(tf("cal.more", { n: 1234 })).toBe("+1.234 more");
      expect(tn("act.nTasks", 1234)).toBe("1.234 tasks");
    } finally {
      de.ui["cal.more"] = held.more;
      de.ui["act.nTasks.other"] = held.tasks;
    }
  });
});

describe("a date", () => {
  // Built from local parts, so the day under test is the day the formatter sees whatever timezone
  // this runs in.
  const at = new Date(2026, 5, 21, 15, 30, 0);

  it("is written in the locale's order", () => {
    expect(formatDay(at, "en-US")).toBe("6/21/2026");
    expect(formatDay(at, "ja-JP")).toBe("2026/6/21");
  });

  it("renders nothing when it cannot be read", () => {
    expect(formatDay(new Date("nope"), "en-US")).toBe("");
    expect(formatDayTime(new Date("nope"), "en-US")).toBe("");
  });

  it("carries the day and the time, and leaves out the year", () => {
    const written = formatDayTime(at, "en-US");
    expect(written).toMatch(/^6\/21, \d{2}:\d{2}/);
    expect(written).not.toContain("2026");
  });

  // What a decision record is dated by: read years later, it has to say which year.
  it("carries the year as well when it is a stamp", () => {
    expect(formatStamp(at, "en-US")).toMatch(/^6\/21\/2026, \d{2}:\d{2}/);
    expect(formatStamp(at, "ja-JP")).toMatch(/^2026\/6\/21 \d{2}:\d{2}/);
    expect(formatStamp(new Date("nope"), "en-US")).toBe("");
  });
});

describe("the calendar's headings", () => {
  it("name the month the way the language names it", () => {
    expect(monthLabel(2026, 5, "ja-JP")).toBe("2026年6月");
    expect(monthLabel(2026, 5, "en-US")).toBe("June 2026");
    expect(monthLabel(2026, 5, "de-DE")).toBe("Juni 2026");
  });

  it("walk the week from Sunday, and rotate by weekStart", () => {
    expect(weekdayLabels(0, "ja-JP")).toEqual(["日", "月", "火", "水", "木", "金", "土"]);
    expect(weekdayLabels(1, "ja-JP")).toEqual(["月", "火", "水", "木", "金", "土", "日"]);
    expect(weekdayLabels(0, "en-US")[0]).toBe("Sun");
  });
});

// The whole reason `dateLocale()` is the default parameter and not `currentLang()`: the setting is
// there for the reader whose dates are shaped unlike their language's, and it has to reach the
// formatters or it reaches nothing.
describe("the locale the formatters default to", () => {
  it("is config.date_locale when it is set", () => {
    snap.language = "ja";
    snap.dateLocale = "en-US";
    expect(formatDay(new Date(2026, 5, 21, 12, 0, 0))).toBe("6/21/2026");
  });

  it("is the language's own when it is not", () => {
    snap.language = "ja";
    expect(monthLabel(2026, 5)).toBe("2026年6月");
  });
});
