// The mark a project is drawn as where its name will not fit. Both halves of it are worked out from
// what the person gave — a colour picked in a colour well, and a name typed in a field — so both have
// to hold for anything either of those can produce.
import { describe, expect, it } from "vitest";
import { inkOn, initialOf } from "./projectMark";

describe("the ink on a project's colour", () => {
  it("is light on a dark ground and dark on a light one", () => {
    expect(inkOn("#101820")).toBe("#fff");
    expect(inkOn("#ffe066")).toBe("#111");
  });

  // The trap the whole function is for: white on yellow is the letter a person cannot read, and it is
  // what a fixed ink would have drawn.
  it("does not leave a letter white on a colour a person can pick", () => {
    for (const pale of ["#ffff00", "#ffffff", "#c8f7c5", "#f0e68c"]) {
      expect(inkOn(pale)).toBe("#111");
    }
  });

  it("reads the short form the same as the long one", () => {
    expect(inkOn("#fff")).toBe(inkOn("#ffffff"));
    expect(inkOn("#012")).toBe(inkOn("#001122"));
  });

  // Nothing said is the theme's own text colour, which holds in both themes — where naming one of the
  // two would be a guess about a ground that is not being drawn either.
  it("names no ink at all for a colour it cannot read", () => {
    expect(inkOn("rebeccapurple")).toBeNull();
    expect(inkOn("")).toBeNull();
    expect(inkOn("#12345")).toBeNull();
  });
});

describe("the character a project is known by", () => {
  it("is the first one of its name", () => {
    expect(initialOf("amenbo")).toBe("a");
    expect(initialOf("  the site")).toBe("t");
  });

  // A name that opens outside the basic plane is two units long in one character, and half of one is
  // not a letter.
  it("is a whole character, not half of a pair", () => {
    expect(initialOf("🐋 whale")).toBe("🐋");
    expect(initialOf("𠮷野家")).toBe("𠮷");
  });

  it("is nothing at all where there is no name to take one from", () => {
    expect(initialOf("   ")).toBe("");
  });
});
