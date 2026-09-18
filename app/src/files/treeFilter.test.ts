// What a narrowed tree draws: the matches, every folder on the way to one, and the two things each
// row has to know to be drawn — how far in it stands, and how many names stand beside it.
import { describe, expect, it } from "vitest";
import { linesOfNames } from "./treeFilter";

const found = (path: string, isDir = false, ignored = false) =>
  ({ path: path.split("/"), isDir, ignored });

/** Each row as `depth:key`, which is the whole of what the tree draws down the left of it. */
const drawn = (rows: ReturnType<typeof linesOfNames>) => rows.map((r) => `${r.depth}:${r.key}`);

describe("the rows a narrowed tree draws", () => {
  it("draws every folder on the way down to a match, and draws it open", () => {
    const rows = linesOfNames([found("src/deep/needle.txt"), found("needle.md")]);
    expect(drawn(rows)).toEqual([
      "0:src",
      "1:src/deep",
      "2:src/deep/needle.txt",
      "0:needle.md",
    ]);
    // The folders on the way are drawn open: there is nothing under them here but the way to a
    // match, so a folded one would be a row with nothing to say where the match is.
    expect(rows.filter((r) => r.unfolded).map((r) => r.key)).toEqual(["src", "src/deep"]);
  });

  it("puts a folder in once however many matches stand under it", () => {
    const rows = linesOfNames([found("src/a-needle.rs"), found("src/b-needle.rs")]);
    expect(drawn(rows)).toEqual(["0:src", "1:src/a-needle.rs", "1:src/b-needle.rs"]);
  });

  it("reads a level the way a level comes back — folders first, then names", () => {
    const rows = linesOfNames([
      found("zeta-needle.txt"),
      found("alpha-needle.txt"),
      found("needles", true),
    ]);
    expect(drawn(rows)).toEqual(["0:needles", "0:alpha-needle.txt", "0:zeta-needle.txt"]);
  });

  it("says how many names stand beside each row and which of them it is", () => {
    const rows = linesOfNames([found("src/a-needle.rs"), found("src/b-needle.rs"), found("needle.md")]);
    const src = rows.find((r) => r.key === "src/a-needle.rs");
    expect([src?.posinset, src?.setsize]).toEqual([1, 2]);
    const top = rows.find((r) => r.key === "needle.md");
    expect([top?.posinset, top?.setsize]).toEqual([2, 2]);
  });

  it("marks a folder on the way where everything drawn under it is ignored", () => {
    const rows = linesOfNames([found("build/needle.js", false, true), found("src/needle.rs")]);
    expect(rows.find((r) => r.key === "build")?.ignored).toBe(true);
    expect(rows.find((r) => r.key === "src")?.ignored).toBe(false);
  });

  it("leaves a folder unmarked where any of what is drawn under it is not ignored", () => {
    const rows = linesOfNames([
      found("mixed/needle.js", false, true),
      found("mixed/needle.rs", false, false),
    ]);
    expect(rows.find((r) => r.key === "mixed")?.ignored).toBe(false);
  });

  it("draws nothing for nothing found", () => {
    expect(linesOfNames([])).toEqual([]);
  });
});
