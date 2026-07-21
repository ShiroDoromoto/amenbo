// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { dismissUpdate, getDismissedUpdate, isUpdateDismissed, sessionDismissCovers, versionIsNewer } from "./updateDismissed";

// The version comparison is a port of core's `version_is_newer`, so it is held to core's own test cases rather than a
// second hand-written list: the Rust test is pulled in with Vite's `?raw` and its assertions are replayed here. Change
// the semantics on one side only and this breaks.
import storeTestsRs from "../../../crates/amenbo-core/src/store/tests.rs?raw";

function coreVersionCases(): { candidate: string; base: string; expected: boolean }[] {
  const cases: { candidate: string; base: string; expected: boolean }[] = [];
  for (const m of storeTestsRs.matchAll(/assert!\((!?)version_is_newer\("([^"]*)",\s*"([^"]*)"\)\)/g)) {
    cases.push({ candidate: m[2], base: m[3], expected: m[1] !== "!" });
  }
  return cases;
}

describe("versionIsNewer", () => {
  it("matches core's version_is_newer on every case core tests", () => {
    const cases = coreVersionCases();
    expect(cases.length).toBeGreaterThan(0); // the extraction itself must not silently find nothing
    for (const c of cases) {
      expect(versionIsNewer(c.candidate, c.base), `${c.candidate} vs ${c.base}`).toBe(c.expected);
    }
  });
});

describe("update dismissal", () => {
  beforeEach(() => localStorage.clear());

  it("has nothing dismissed until something is", () => {
    expect(getDismissedUpdate()).toBeNull();
    expect(isUpdateDismissed("1.3.0")).toBe(false);
  });

  it("keeps the dismissed version quiet across reloads", () => {
    dismissUpdate("1.3.0");
    expect(getDismissedUpdate()).toBe("1.3.0");
    expect(isUpdateDismissed("1.3.0")).toBe(true);
  });

  it("stays quiet for an older offer than the one dismissed", () => {
    dismissUpdate("1.3.0");
    expect(isUpdateDismissed("1.2.0")).toBe(true);
  });

  it("speaks up again once a newer version is offered", () => {
    dismissUpdate("1.3.0");
    expect(isUpdateDismissed("1.4.0")).toBe(false);
    expect(isUpdateDismissed("1.3.1")).toBe(false);
  });

  it("does not record a version-less dismissal", () => {
    dismissUpdate(null);
    expect(getDismissedUpdate()).toBeNull();
  });

  it("shows a version-less offer even after an earlier dismissal", () => {
    dismissUpdate("1.3.0");
    expect(isUpdateDismissed(null)).toBe(false);
  });
});

describe("sessionDismissCovers", () => {
  it("covers nothing when nothing was dismissed this session", () => {
    expect(sessionDismissCovers(undefined, "1.3.0")).toBe(false);
    expect(sessionDismissCovers(undefined, null)).toBe(false);
  });

  it("keeps the dismissed version and older ones quiet, but lets a newer offer through", () => {
    expect(sessionDismissCovers("1.3.0", "1.3.0")).toBe(true);
    expect(sessionDismissCovers("1.3.0", "1.2.0")).toBe(true);
    expect(sessionDismissCovers("1.3.0", "1.4.0")).toBe(false);
    expect(sessionDismissCovers("1.3.0", "1.3.1")).toBe(false);
  });

  it("treats a version-less dismissal as covering only the version-less offer", () => {
    expect(sessionDismissCovers(null, null)).toBe(true);
    expect(sessionDismissCovers(null, "1.3.0")).toBe(false);
  });

  it("does not let a versioned dismissal cover a later version-less offer", () => {
    expect(sessionDismissCovers("1.3.0", null)).toBe(false);
  });
});
