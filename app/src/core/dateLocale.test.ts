// `config.date_locale` decides the shape of a date, `config.language` decides the words. They are
// normally the same answer, which is why the first is normally unset — these are the cases where
// they come apart, and the case where the value cannot be used at all.
import { beforeEach, describe, it, expect, vi } from "vitest";

const snap = { language: null as string | null, dateLocale: null as string | null };
vi.mock("./snapshot", () => ({ getSnapshot: () => snap }));

import { dateLocale } from "./i18n";

beforeEach(() => {
  snap.language = null;
  snap.dateLocale = null;
});

describe("dateLocale", () => {
  it("follows the language when nothing else is asked for", () => {
    snap.language = "ja";
    expect(dateLocale()).toBe("ja-JP");
    snap.language = "en";
    expect(dateLocale()).toBe("en-US");
  });

  // The whole point of the setting: a Japanese UI that writes ISO dates.
  it("takes the declared locale over the language's", () => {
    snap.language = "ja";
    snap.dateLocale = "sv-SE";
    expect(dateLocale()).toBe("sv-SE");
  });

  it("treats an empty or blank value as unset", () => {
    snap.language = "en";
    snap.dateLocale = "   ";
    expect(dateLocale()).toBe("en-US");
  });

  // Nothing validates the tag on the way in — what counts as a usable locale is the formatter's
  // judgement — so a typo must cost the setting, never the date.
  it("falls back to the language's when the platform cannot use the tag", () => {
    snap.language = "ja";
    snap.dateLocale = "not a locale";
    expect(dateLocale()).toBe("ja-JP");
  });

  // And the date actually comes out in it, which is the only reason any of this is here.
  it("is a locale the formatter accepts", () => {
    snap.language = "ja";
    snap.dateLocale = "sv-SE";
    const written = new Intl.DateTimeFormat(dateLocale(), {
      year: "numeric", month: "2-digit", day: "2-digit",
    }).format(new Date("2026-07-27T00:00:00Z"));
    expect(written).toContain("2026");
  });
});
