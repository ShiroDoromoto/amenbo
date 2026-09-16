// Where a name is cut, which two faces read the same way: the tree selects the stem when a rename
// opens, and the rail's git half keeps what is past the cut on the screen when the row is narrow.
//
// The three that matter are the ones a single `indexOf` would get wrong: a name with two dots in it,
// a name that begins with one, and a name with none at all.
import { describe, expect, it } from "vitest";
import { stemEnd, tailFrom } from "./stem";

describe("where a name's stem ends", () => {
  it("cuts at the last dot, not the first", () => {
    expect("archive.tar.gz".slice(0, stemEnd("archive.tar.gz"))).toBe("archive.tar");
    expect("archive.tar.gz".slice(stemEnd("archive.tar.gz"))).toBe(".gz");
  });

  it("reads a name that begins with a dot as all stem", () => {
    // `.gitignore` is a name, not an extension on an empty stem — cut there, a rename would open
    // with nothing selected and the git half would shorten the whole of it away.
    expect(stemEnd(".gitignore")).toBe(".gitignore".length);
  });

  it("reads a name with no dot in it as all stem", () => {
    expect(stemEnd("Makefile")).toBe("Makefile".length);
  });
});

describe("where a name is cut to fit a narrow row", () => {
  /** What the row draws after the cut. */
  const tail = (name: string) => name.slice(tailFrom(name));

  it("keeps the word before the extension as well as the extension", () => {
    // The run this was found on: three drafts of one document, alike until the last few characters.
    expect(tail("【雨宮処凛様】「やまなしの春をめぐる覚え書き」校正済み.md")).toBe("」校正済み.md");
    expect(tail("【雨宮処凛様】「やまなしの春をめぐる覚え書き」草稿.md")).toBe("書き」草稿.md");
  });

  it("never cuts inside the extension", () => {
    // Half an extension says nothing about what the file is, so a long one is kept whole and the
    // tail is shorter than it would otherwise be.
    expect(tail("report.markdown")).toBe(".markdown");
  });

  it("leaves a short name whole, with nothing before the tail", () => {
    expect(tailFrom("a.rs")).toBe(0);
    expect(tail("a.rs")).toBe("a.rs");
  });
});
