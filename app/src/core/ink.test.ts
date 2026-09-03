// The ink over a colour the user picked. What is being asserted is the floor, not a palette: any colour the
// picker can produce has to come back with an ink that clears AA over it.
import { describe, expect, it } from "vitest";
import { contrastRatio, inkOn } from "./ink";

describe("contrastRatio", () => {
  it("runs from 1 (a colour with itself) to 21 (black on white)", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(contrastRatio("#3366aa", "#3366aa")).toBeCloseTo(1, 5);
  });

  it("reads the short form, and the order of the pair does not matter", () => {
    expect(contrastRatio("#fff", "#000")).toBeCloseTo(21, 5);
    expect(contrastRatio("#0d7777", "#ffffff")).toBeCloseTo(contrastRatio("#ffffff", "#0d7777"), 5);
  });

  it("is 1 for a colour it cannot read, rather than a number that would read as a verdict", () => {
    expect(contrastRatio("rebeccapurple", "#ffffff")).toBe(1);
  });
});

describe("inkOn", () => {
  it("keeps white over a dark colour", () => {
    expect(inkOn("#0d7777")).toBe("#ffffff");
    expect(inkOn("#222222")).toBe("#ffffff");
  });

  it("takes the default colour off white — the case the letter was unreadable in", () => {
    const ink = inkOn("#9aa7b2");
    expect(ink).not.toBe("#ffffff");
    expect(contrastRatio("#9aa7b2", "#ffffff")).toBeLessThan(4.5);
    expect(contrastRatio("#9aa7b2", ink)).toBeGreaterThanOrEqual(4.5);
  });

  it("clears AA over every colour the picker can produce", () => {
    for (let r = 0; r < 256; r += 17) {
      for (let g = 0; g < 256; g += 17) {
        for (let b = 0; b < 256; b += 17) {
          const bg = `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
          expect(contrastRatio(bg, inkOn(bg))).toBeGreaterThanOrEqual(4.5);
        }
      }
    }
  });

  it("stays in the ground's own hue rather than dropping to black", () => {
    // A light blue: the ink is a dark blue, not #000000.
    const ink = inkOn("#8ab4f8");
    expect(ink).not.toBe("#000000");
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(ink.slice(i, i + 2), 16));
    expect(b).toBeGreaterThan(r);
    expect(b).toBeGreaterThan(g);
  });

  it("reads the short form", () => {
    expect(inkOn("#000")).toBe("#ffffff");
    expect(inkOn("#eee")).not.toBe("#ffffff");
  });

  it("leaves white where the colour is in a form it cannot read", () => {
    expect(inkOn("rebeccapurple")).toBe("#ffffff");
    expect(inkOn("")).toBe("#ffffff");
  });
});
