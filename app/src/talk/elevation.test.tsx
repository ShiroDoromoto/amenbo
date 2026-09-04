// @vitest-environment jsdom
// What the band has to carry, in the two ways it could quietly stop carrying it: the way out going
// missing, and the sentences arriving in a language the reader does not read.
//
// Neither would look broken. A band with the fact and no remedy still reads as a warning, and a band
// in English on a Japanese screen still reads as a band — it is only that the person it was written
// for cannot act on either one.
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ElevationBand } from "./elevation";
import { en } from "../core/i18n/locales/en";
import { ja } from "../core/i18n/locales/ja";
import type { Lang } from "../core/i18n";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

/** The band as it is drawn, in the language named. */
function band(lang: Lang): HTMLElement {
  act(() => root.render(createElement(ElevationBand, { lang })));
  return container.firstElementChild as HTMLElement;
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("ElevationBand", () => {
  it("says what is true, why it costs anything, and how to get out of it", () => {
    const drawn = band("en");

    expect(drawn.textContent).toContain(en.ui["talk.elevated.title"]);
    expect(drawn.textContent).toContain(en.ui["talk.elevated.body"]);
    // The way out is the half a person can act on. A band that states the fact and stops is a
    // warning about something the reader has no move against.
    expect(drawn.textContent).toContain(en.ui["talk.elevated.fix"]);
  });

  it("is written in the language the window is in", () => {
    const drawn = band("ja");

    expect(drawn.textContent).toContain(ja.ui["talk.elevated.title"]);
    expect(drawn.textContent).not.toContain(en.ui["talk.elevated.title"]);
  });

  // It reports a state the window has been in since it opened, not an event that just happened —
  // `alert` would interrupt a reader mid-sentence to say something that will still be true later.
  it("is announced as a state rather than as an interruption", () => {
    expect(band("en").getAttribute("role")).toBe("status");
  });
});
