// The half of the coverage gate that is not in a dictionary.
//
// A skin says its own name per language, in a `titles:` block inside the skin file, and not in the
// dictionaries beside this test. That is what lets somebody who ships a skin of their own translate
// its name — and it is also what puts the four shipped skins outside coverage.test.ts, which reads
// the dictionaries and nothing else. Add a twentieth language there and the four YAML files stay
// green while the new reader is handed three English names and one Japanese one.
//
// So hold the shipped skins to the dictionaries this build loads: whatever language the UI can be
// read in, the shipped names come in that language too. A skin somebody else wrote is not held to
// anything — a name it has no translation for falls back to `title`, which is the author's own word
// for their work.
//
// The skin files are read out of the crate with Vite's `?raw`, the way the Rust↔TS parity tests read
// Rust. `titles:` is read with a line sweep rather than a YAML parser: the block is one level of
// plain `lang: "name"` pairs, and a gate on four files is no reason to ship a parser.
import { describe, expect, it } from "vitest";
import { DICTIONARIES } from "./index";

const skins = import.meta.glob("../../../../crates/amenbo-core/skins/*.yaml", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/**
 * The languages the UI can be read in — the dictionaries this build actually loads, the same
 * reference coverage.test.ts holds a translation to. A list of languages kept beside this one would
 * go stale the first time a dictionary is added, and the gate would then pass by not looking.
 */
const LANGUAGES = Object.keys(DICTIONARIES);

/** `<name>.yaml` → the skin's name, so a failure says which file to open. */
function skinName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1, -".yaml".length);
}

/**
 * The languages a skin writes a name in. The block runs from the `titles:` line to the next line
 * that starts in the first column; a key with a blank value is not a name, since an empty string
 * would reach the screen as an empty label instead of falling back to `title`.
 */
function named(yaml: string): Set<string> {
  const lines = yaml.split("\n");
  const start = lines.findIndex((line) => line.trimEnd() === "titles:");
  if (start < 0) return new Set();
  const found = new Set<string>();
  for (const line of lines.slice(start + 1)) {
    if (!/^\s/.test(line)) break;
    const pair = line.match(/^\s+([\w-]+)\s*:\s*(\S.*?)\s*$/);
    if (pair && pair[2] !== '""' && pair[2] !== "''") found.add(pair[1]);
  }
  return found;
}

describe("every shipped skin says its name in every language this build reads", () => {
  it("finds the shipped skins at all", () => {
    // A glob that stops matching would empty the gate while staying green.
    expect(Object.keys(skins).length).toBeGreaterThan(0);
  });

  for (const [path, yaml] of Object.entries(skins)) {
    const skin = skinName(path);

    it(`${skin} has a name in each of the ${LANGUAGES.length} languages`, () => {
      const titles = named(yaml);
      const gaps = LANGUAGES.filter((lang) => !titles.has(lang));
      // The count leads, the way it does for a half-written dictionary; the list underneath says
      // which languages the skin file still owes a name in.
      expect(gaps, `${skin}.yaml: ${gaps.length} language(s) with no name — ${gaps.join(", ")}`)
        .toEqual([]);
    });

    it(`${skin} writes no name in a language nothing is read in`, () => {
      // A code no dictionary answers to — a typo, or a language dropped since — is a name that
      // reaches no reader, and nothing at runtime says so.
      const stray = [...named(yaml)].filter((lang) => !LANGUAGES.includes(lang));
      expect(stray, `${skin}.yaml: ${stray.join(", ")} — no dictionary is read in that language`)
        .toEqual([]);
    });
  }
});
