// The label on the reveal button is the only thing this module decides, and it is decided from a
// string the OS writes for a different purpose. What is held here is that the two names are only
// spoken when the user agent actually places the reader on that OS — everything else, including the
// user agents of the machines nobody tested on, falls to the wording that is true anywhere.
import { describe, expect, it } from "vitest";
import { hostOs, revealLabelKey } from "./platform";
import { en } from "./i18n/locales/en";

const MACOS = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const WINDOWS = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";
const LINUX = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

describe("reading the OS off the user agent", () => {
  it("places the two webviews whose file manager has a name", () => {
    expect(hostOs(MACOS)).toBe("macos");
    expect(hostOs(WINDOWS)).toBe("windows");
  });

  it("leaves everything else unplaced rather than guessing", () => {
    expect(hostOs(LINUX)).toBe("other");
    expect(hostOs("")).toBe("other");
  });
});

describe("the label the reveal button carries", () => {
  it("names the file manager the press actually opens", () => {
    expect(revealLabelKey("macos")).toBe("newproj.openFinder");
    expect(revealLabelKey("windows")).toBe("newproj.openExplorer");
    expect(revealLabelKey("other")).toBe("newproj.openFileManager");
  });

  it("asks for keys English can answer — a key with no entry would render as itself", () => {
    for (const os of ["macos", "windows", "other"] as const) {
      expect(en.ui[revealLabelKey(os)]).toBeTruthy();
    }
  });
});
