// The line the tree is laid out to. jsdom measures nothing, so the width a name is drawn at is
// stood in for by the test and what is checked is which line wins.
import { describe, expect, it } from "vitest";
import { widest } from "./widest";

/** A name as wide as its letters, at 7px a letter. */
const wide = (name: string) => name.length * 7;

describe("the widest line of the tree", () => {
  it("is the one with the widest name where every line stands at the same depth", () => {
    const lines = [
      { depth: 0, name: "a.md" },
      { depth: 0, name: "a-very-long-name.md" },
      { depth: 0, name: "b.md" },
    ];
    expect(widest(lines, wide, 12)?.name).toBe("a-very-long-name.md");
  });

  it("counts how far in a line stands as well as its name", () => {
    // 3 × 12 + 4 × 7 = 64 against 0 + 8 × 7 = 56: the shorter name is the wider line.
    const lines = [
      { depth: 0, name: "abcdefgh" },
      { depth: 3, name: "abcd" },
    ];
    expect(widest(lines, wide, 12)?.name).toBe("abcd");
  });

  it("goes by how wide a name is drawn, not by how many letters it has", () => {
    const drawn = (name: string) => (name === "MMMM" ? 48 : name.length * 3);
    const lines = [
      { depth: 0, name: "iiiiiiii" },
      { depth: 0, name: "MMMM" },
    ];
    expect(widest(lines, drawn, 12)?.name).toBe("MMMM");
  });

  it("is nothing for a tree with no lines", () => {
    expect(widest([], wide, 12)).toBeNull();
  });
});
