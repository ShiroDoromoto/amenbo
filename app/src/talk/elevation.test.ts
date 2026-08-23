// @vitest-environment jsdom
// What the band has to carry, in the two ways it could quietly stop carrying it: the way out going
// missing, and the sentences arriving in a language the reader does not read.
//
// Neither would look broken. A band with the fact and no remedy still reads as a warning, and a band
// in English on a Japanese screen still reads as a band — it is only that the person it was written
// for cannot act on either one.
import { describe, expect, it } from "vitest";
import { elevationBand } from "./elevation";
import { en } from "../core/i18n/locales/en";
import { ja } from "../core/i18n/locales/ja";

describe("elevationBand", () => {
  it("says what is true, why it costs anything, and how to get out of it", () => {
    const band = elevationBand("en");

    expect(band.textContent).toContain(en.ui["talk.elevated.title"]);
    expect(band.textContent).toContain(en.ui["talk.elevated.body"]);
    // The way out is the half a person can act on. A band that states the fact and stops is a
    // warning about something the reader has no move against.
    expect(band.textContent).toContain(en.ui["talk.elevated.fix"]);
  });

  it("is written in the language the window is in", () => {
    const band = elevationBand("ja");

    expect(band.textContent).toContain(ja.ui["talk.elevated.title"]);
    expect(band.textContent).not.toContain(en.ui["talk.elevated.title"]);
  });

  // It reports a state the window has been in since it opened, not an event that just happened —
  // `alert` would interrupt a reader mid-sentence to say something that will still be true later.
  it("is announced as a state rather than as an interruption", () => {
    expect(elevationBand("en").getAttribute("role")).toBe("status");
  });
});
