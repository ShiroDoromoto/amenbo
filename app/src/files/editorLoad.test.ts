// Which files the editor still wraps. Everything else in this module needs a layout to do anything
// at all, which jsdom does not run — but the line this draws is the whole of the fix for
// `AMB-T-3808`, and it is decided from the text alone.
import { describe, expect, it } from "vitest";
import { wrappable } from "./editorLoad";

describe("the text an editor wraps", () => {
  it("is ordinary source, however much of it there is", () => {
    expect(wrappable("")).toBe(true);
    expect(wrappable("let x = 1;\nlet y = 2;\n")).toBe(true);
    // Five million characters, none of the lines long. Line count does not enter into it.
    expect(wrappable("x".repeat(80).concat("\n").repeat(60_000))).toBe(true);
  });

  it("is not a file squashed onto one line", () => {
    expect(wrappable("x".repeat(1024 * 1024))).toBe(false);
    // One long line among short ones is enough: the cost is the line's, not the file's.
    expect(wrappable(`short\n${"x".repeat(30_000)}\nshort\n`)).toBe(false);
  });

  it("is decided by the longest line, at 20,000 characters", () => {
    expect(wrappable("x".repeat(20_000))).toBe(true);
    expect(wrappable("x".repeat(20_001))).toBe(false);
    // The last line counts too, whether or not the file ends in a newline.
    expect(wrappable(`a\n${"x".repeat(20_001)}`)).toBe(false);
    expect(wrappable(`a\n${"x".repeat(20_001)}\n`)).toBe(false);
  });
});
