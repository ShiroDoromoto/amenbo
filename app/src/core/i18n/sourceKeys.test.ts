// The other direction of the coverage gate. coverage.test.ts reads outward — every key `en` holds
// must reach every language — and nothing reads back: a call site can ask for a key `en` has never
// had, and neither the type nor the build says so. `t(key: string)` takes any string, and a lookup
// that finds nothing returns the key, so the miss ships as a screen with the bare key printed where
// a button label belongs.
//
// So sweep the sources for the keys they ask for by name and hold them to `en`. Only literals are
// read: a key assembled at runtime (`t(`view.${kind}`)`) is not knowable here, and the dictionary
// keeps its own tests for those families. That leaves the gate exact — every key it names is one a
// reader can actually be shown.
//
// The sources are pulled in with Vite's `?raw`, the way the Rust↔TS parity tests read Rust: no
// codegen, and no node dependency in the browser-targeted tsconfig.
import { describe, expect, it } from "vitest";
import { en } from "./locales/en";

const sources = import.meta.glob("../../**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/**
 * Keys a call site names **because** nothing holds them. The English fallback has one hole it cannot
 * fill — a key no language has — and the only way to test what the screen does with it is to ask for
 * one. Listing it here is what keeps the gate from reading that test as the bug it is testing for.
 */
const DELIBERATELY_ABSENT = new Set(["nowhere.atall"]);

/**
 * Every `t("…")` / `tf("…")` in a source file, as `key`. The word boundary is what keeps `format(`
 * and the like out; a first argument that is not a quoted literal simply does not match, which is
 * the runtime-assembled case dropping out on its own. A backtick literal counts too where it holds
 * no `${…}` — nothing here rejects one, and a key spelled that way is as fixed as any other.
 */
function literalKeys(src: string): string[] {
  const calls = src.matchAll(/\btf?\(\s*(?:"([^"\\]+)"|`([^`\\$]+)`)/g);
  return [...calls].map((m) => m[1] ?? m[2]);
}

/** Where each key is asked for, so a failure names the file rather than only the key. */
function askedFor(): Map<string, string[]> {
  const asked = new Map<string, string[]>();
  for (const [path, src] of Object.entries(sources)) {
    for (const key of literalKeys(src)) {
      if (DELIBERATELY_ABSENT.has(key)) continue;
      const at = asked.get(key) ?? [];
      if (!at.includes(path)) at.push(path);
      asked.set(key, at);
    }
  }
  return asked;
}

describe("every key the sources name is one English holds", () => {
  it("finds the calls at all", () => {
    // A regex that stops matching would empty the gate while staying green, so the sweep is asked
    // to have found the screens' worth of keys it is there to judge.
    expect(askedFor().size).toBeGreaterThan(100);
  });

  it("has an English string for each of them", () => {
    const missing = [...askedFor()]
      .filter(([key]) => en.ui[key as keyof typeof en.ui] === undefined)
      .map(([key, at]) => `${key} — ${at.join(", ")}`);
    expect(missing, `${missing.length} key(s) with no English string`).toEqual([]);
  });
});
