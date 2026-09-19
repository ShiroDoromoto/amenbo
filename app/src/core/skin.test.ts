// @vitest-environment jsdom
// Wearing a skin. What is asserted is the mechanism the values win by — which side each rule
// matches, that the sheet stays last in `<head>`, and that a value cannot end the declaration it is
// written into.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./ipc", () => ({ invoke: () => Promise.reject(new Error("no host")) }));

import type { SkinTablesDto } from "../bindings/bindings";
import { applySkin } from "./skin";

/** A skin as the window receives it, with only the parts a test is about spelled out. */
const wearing = (over: Partial<SkinTablesDto>): SkinTablesDto => ({
  name: "n",
  title: "n",
  light: {},
  dark: {},
  font: null,
  ...over,
});

const sheetText = () => {
  const el = document.getElementById("amenbo-skin") as HTMLStyleElement | null;
  return [...(el?.sheet?.cssRules ?? [])].map((r) => r.cssText);
};

describe("applySkin", () => {
  beforeEach(() => {
    document.head.innerHTML = "";
  });

  it("writes one rule per side, each matching only its own", () => {
    applySkin(wearing({
      name: "washi",
      title: "washi",
      light: { "c-bg": "#faf7f0" },
      dark: { "c-bg": "#1a1713" },
    }));
    const rules = sheetText();
    expect(rules).toHaveLength(2);
    expect(rules[0]).toContain('[data-theme="light"]');
    expect(rules[0]).toContain("--c-bg: #faf7f0");
    expect(rules[1]).toContain('[data-theme="dark"]');
    expect(rules[1]).toContain("--c-bg: #1a1713");
  });

  it("keeps the sheet last in the head, which is what its values win by", () => {
    document.head.appendChild(document.createElement("style"));
    applySkin(wearing({ name: "n", title: "n", light: { "c-bg": "#fff" }, dark: {} }));
    expect(document.head.lastElementChild?.id).toBe("amenbo-skin");

    // A chunk loaded later appends its own; the next change takes the place back.
    document.head.appendChild(document.createElement("style"));
    applySkin(wearing({ name: "n", title: "n", light: { "c-bg": "#eee" }, dark: {} }));
    expect(document.head.lastElementChild?.id).toBe("amenbo-skin");
  });

  it("replaces what was on rather than piling up", () => {
    applySkin(wearing({ name: "a", title: "a", light: { "c-bg": "#111" }, dark: {} }));
    applySkin(wearing({ name: "b", title: "b", light: { "c-bg": "#222" }, dark: {} }));
    const rules = sheetText();
    expect(rules).toHaveLength(2);
    expect(rules[0]).toContain("--c-bg: #222");
  });

  it("takes the skin off, leaving the sheet with nothing in it", () => {
    applySkin(wearing({ name: "a", title: "a", light: { "c-bg": "#111" }, dark: {} }));
    applySkin(null);
    expect(sheetText()).toHaveLength(0);
  });

  it("cannot be made to end the declaration it is written into", () => {
    applySkin(wearing({
      name: "x",
      title: "x",
      // A value that would close the rule if it were spliced in as text.
      light: { "c-bg": "red; } body { display: none } .x {" },
      dark: {},
    }));
    const rules = sheetText();
    expect(rules).toHaveLength(2);
    expect(rules.join("")).not.toContain("display: none");
  });

  it("leaves out a value that has no business being one", () => {
    applySkin(wearing({
      name: "x",
      title: "x",
      light: {
        "c-bg": "#fff",
        "c-text": "red /* and a comment */",
        "c-surface": "url(http://example.invalid/x.png)",
        "c-edge": "a".repeat(600),
      },
      dark: {},
    }));
    const light = sheetText()[0];
    expect(light).toContain("--c-bg: #fff");
    expect(light).not.toContain("--c-text");
    expect(light).not.toContain("--c-surface");
    expect(light).not.toContain("--c-edge");
  });

  it("writes only names that are names", () => {
    applySkin(wearing({
      name: "x",
      title: "x",
      light: { "c-bg": "#fff", "c bg": "#000", "--c-text": "#000", "c-bg; }": "#000" },
      dark: {},
    }));
    const light = sheetText()[0];
    expect(light).toContain("--c-bg: #fff");
    expect(light).not.toContain("#000");
  });
});
