// @vitest-environment jsdom
// Wearing a skin. What is asserted is the mechanism the values win by — which side each rule
// matches, that the sheet stays last in `<head>`, and that a value cannot end the declaration it is
// written into.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./ipc", () => ({ invoke: () => Promise.reject(new Error("no host")) }));

import type { SkinTablesDto } from "../bindings/bindings";
import { applySkin, skinTitle } from "./skin";
import { setThemePref } from "./theme";

/** A skin as the window receives it, with only the parts a test is about spelled out. */
const wearing = (over: Partial<SkinTablesDto>): SkinTablesDto => ({
  name: "n",
  title: "n",
  titles: {},
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

  // A skin written for one side only takes the choice of appearance away while it is on. Held
  // here rather than on the settings screen: a skin is worn at startup and in every window, and
  // only one of those has that screen open.
  it("holds the appearance to the one side a skin wrote", () => {
    applySkin(wearing({ name: "retro", title: "Retro", light: {}, dark: { "c-bg": "#000" } }));
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("leaves the appearance alone for a skin that wrote both sides", () => {
    setThemePref("light");
    applySkin(wearing({ name: "retro", title: "Retro", light: {}, dark: { "c-bg": "#000" } }));
    applySkin(wearing({
      name: "washi",
      title: "Washi",
      light: { "c-bg": "#f2ebdc" },
      dark: { "c-bg": "#1a1611" },
    }));
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("gives the appearance back when the skin comes off", () => {
    setThemePref("light");
    applySkin(wearing({ name: "retro", title: "Retro", light: {}, dark: { "c-bg": "#000" } }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    applySkin(null);
    expect(document.documentElement.dataset.theme).toBe("light");
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

// Two spellings of one rule, held to each other.
//
// `usable` above asks what shape a value may have at the moment the declaration is written, and
// `usable_value` in `crates/amenbo-core/src/skin.rs` asks it while the file is being read. Neither
// can be dropped. The window is handed a table over IPC and reads it where it arrives; core is the
// only side that can tell the author why a value of theirs went nowhere. So the rule is written
// twice — and a value core keeps that the window then drops is exactly the silence the warning was
// added to end.
//
// Both spellings are read out of the tree with Vite's `?raw`, the way the Rust↔TS parity tests do:
// move one side alone and this breaks.
import skinTs from "./skin.ts?raw";
import skinRs from "../../../crates/amenbo-core/src/skin.rs?raw";

/** What a `\x` in a character literal stands for. The three this rule spells are all that is here. */
function unescape(after: string): string {
  return after === "\\" ? "\\" : after === "n" ? "\n" : after === "r" ? "\r" : after;
}

/** The one capture of a pattern, or a failure naming what stopped matching. */
function only(src: string, re: RegExp, what: string): string {
  const m = src.match(re);
  if (!m) throw new Error(`${what} is no longer spelled the way this test reads it`);
  return m[1];
}

describe("the shape a value may have", () => {
  it("is the same set of characters on both sides", () => {
    const fromTs = only(skinTs, /const VALUE = \/\^\[\^([^\]]*)\]\{1,\d+\}\$\/;/, "`VALUE` in skin.ts");
    const inTs = new Set<string>();
    for (let i = 0; i < fromTs.length; i++) {
      inTs.add(fromTs[i] === "\\" ? unescape(fromTs[++i]) : fromTs[i]);
    }

    const fromRs = only(
      skinRs,
      /pub const NOT_IN_A_VALUE: &\[char\] = &\[([^\]]*)\];/,
      "`NOT_IN_A_VALUE` in skin.rs",
    );
    const inRs = new Set(
      [...fromRs.matchAll(/'(\\.|[^'])'/g)].map((m) =>
        m[1].startsWith("\\") ? unescape(m[1].slice(1)) : m[1],
      ),
    );

    expect([...inRs].sort()).toEqual([...inTs].sort());
  });

  it("is capped at the same length on both sides", () => {
    const inTs = only(skinTs, /const VALUE = \/\^\[\^[^\]]*\]\{1,(\d+)\}\$\/;/, "`VALUE` in skin.ts");
    const inRs = only(skinRs, /pub const VALUE_MAX: usize = (\d+);/, "`VALUE_MAX` in skin.rs");
    expect(inRs).toBe(inTs);
  });

  it("keeps out the same three things a character class cannot say", () => {
    // A comment delimiter either way round, and `url(` with the space the applying side allows.
    expect(only(skinTs, /const NOT_IN_A_VALUE = \/(.*)\/i;/, "`NOT_IN_A_VALUE` in skin.ts")).toBe(
      "\\/\\*|\\*\\/|url\\s*\\(",
    );
    const asked = only(skinRs, /pub fn usable_value[\s\S]*?\n}\n([\s\S]*?)\n}\n/, "`usable_value` in skin.rs");
    expect(skinRs).toContain('value.contains("/*") || value.contains("*/")');
    expect(asked).toContain('"url"');
    expect(asked).toContain("starts_with('(')");
  });
});

describe("skinTitle", () => {
  const retro = { title: "Retro", titles: { ja: "レトロゲーム", fr: "Rétro" } };

  it("gives the name the author wrote for this reader's language", () => {
    expect(skinTitle(retro, "ja")).toBe("レトロゲーム");
    expect(skinTitle(retro, "fr")).toBe("Rétro");
  });

  it("gives the author's one name where they wrote none for this language", () => {
    expect(skinTitle(retro, "de")).toBe("Retro");
    expect(skinTitle({ title: "Washi", titles: {} }, "ja")).toBe("Washi");
  });

  it("reads a name written empty as a name not written", () => {
    expect(skinTitle({ title: "Washi", titles: { ja: "   " } }, "ja")).toBe("Washi");
  });
});
