// @vitest-environment jsdom
// Wearing a skin. What is asserted is the mechanism the values win by — which side each rule
// matches, that the sheet stays last in `<head>`, and that a value cannot end the declaration it is
// written into.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./ipc", () => ({ invoke: () => Promise.reject(new Error("no host")) }));

import type { SkinTablesDto } from "../bindings/bindings";
import { applySkin, pictureSlots, skinTitle } from "./skin";
import { setThemePref } from "./theme";

/** A skin as the window receives it, with only the parts a test is about spelled out. */
const wearing = (over: Partial<SkinTablesDto>): SkinTablesDto => ({
  name: "n",
  title: "n",
  titles: {},
  light: {},
  dark: {},
  font: null,
  backgrounds: {},
  stamp: "s1",
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
import iconTsx from "../components/Icon.tsx?raw";

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

// The pictures a skin lays, on the way from a filename to a `background` layer.
//
// What the author wrote is a filename and two words. Everything else about the address — the
// scheme, the skin's name in front of it, the stamp behind it — is built here, and that is what
// keeps `url()` out of every value they can write (`AMB-D-936`).
describe("the pictures a skin lays", () => {
  beforeEach(() => {
    document.head.innerHTML = "";
  });

  const laid = () => {
    const rules = sheetText();
    return rules[rules.length - 1];
  };

  it("goes on one rule of its own, matching both sides", () => {
    applySkin(wearing({
      name: "washi",
      light: { "c-bg": "#fff" },
      dark: { "c-bg": "#000" },
      backgrounds: { "c-bg": { file: "paper.png", fit: "cover", at: "center" } },
    }));
    const rules = sheetText();
    expect(rules).toHaveLength(3);
    expect(rules[2]).toContain("[data-theme]");
    expect(rules[2]).not.toContain('[data-theme="');
    expect(rules[2]).toContain('--c-bg-pic: url("amenboskin://localhost/washi/paper.png?v=s1")');
  });

  it("says how big the picture is, except where it repeats", () => {
    applySkin(wearing({
      name: "washi",
      backgrounds: {
        "c-bg": { file: "a.png", fit: "contain", at: "top-left" },
        "c-surface": { file: "b.png", fit: "tile", at: "top" },
      },
    }));
    expect(laid()).toContain("--c-bg-pic: url(\"amenboskin://localhost/washi/a.png?v=s1\") top left / contain no-repeat");
    expect(laid()).toContain("--c-surface-pic: url(\"amenboskin://localhost/washi/b.png?v=s1\") top repeat");
  });

  it("carries the stamp of the file, so the same names are fetched again", () => {
    const slots = pictureSlots(wearing({
      name: "washi",
      stamp: "1700000000000-2048",
      backgrounds: { "c-bg": { file: "paper.png", fit: "cover", at: "center" } },
    }));
    expect(slots["c-bg-pic"]).toContain("?v=1700000000000-2048");
  });

  it("writes a name and a filename as one segment each", () => {
    const slots = pictureSlots(wearing({
      name: "my skin",
      backgrounds: { "c-bg": { file: "a b/c.png", fit: "cover", at: "center" } },
    }));
    expect(slots["c-bg-pic"]).toContain("/my%20skin/a%20b%2Fc.png?");
  });

  it("lays nothing in a place this build has no slot for", () => {
    applySkin(wearing({
      name: "washi",
      light: { "c-bg": "#fff" },
      backgrounds: { "c-text": { file: "a.png", fit: "cover", at: "center" } },
    }));
    expect(sheetText()).toHaveLength(2);
  });

  it("lays the way this build lays one where the word is not one of ours", () => {
    const slots = pictureSlots(wearing({
      name: "washi",
      backgrounds: { "c-bg": { file: "a.png", fit: "stretch", at: "middle" } },
    }));
    expect(slots["c-bg-pic"]).toBe('url("amenboskin://localhost/washi/a.png?v=s1") center / cover no-repeat');
  });

  it("takes the pictures off with the skin", () => {
    applySkin(wearing({
      name: "washi",
      backgrounds: { "c-bg": { file: "a.png", fit: "cover", at: "center" } },
    }));
    applySkin(null);
    expect(sheetText()).toHaveLength(0);
  });
});

// The three vocabularies a background is written in, held to core's.
//
// The check has already ruled on the place, the fit and the spot by the time the tables reach the
// window — but what each word turns into is written here, and a word this file has no answer for is
// laid the way this build lays one. A place added in core and not here is a picture that silently
// does not draw.
describe("the words a background is written in", () => {
  const list = (src: string, re: RegExp, what: string) =>
    [...only(src, re, what).matchAll(/"([a-z0-9-]+)"/g)].map((m) => m[1]).sort();

  it("name the same four places", () => {
    expect(list(skinTs, /const PLACES = \[([\s\S]*?)\];/, "`PLACES` in skin.ts")).toEqual(
      list(skinRs, /pub const BACKGROUNDS: &\[&str\] = &\[([\s\S]*?)\];/, "`BACKGROUNDS` in skin.rs"),
    );
  });

  it("name the same ways of laying one, and the same default", () => {
    expect(list(skinTs, /const FITS = \[([\s\S]*?)\];/, "`FITS` in skin.ts")).toEqual(
      list(skinRs, /pub const FITS: &\[&str\] = &\[([\s\S]*?)\];/, "`FITS` in skin.rs"),
    );
    expect(only(skinTs, /const FIT_DEFAULT = "([a-z]+)";/, "`FIT_DEFAULT` in skin.ts")).toBe(
      only(skinRs, /pub const FIT_DEFAULT: &str = "([a-z]+)";/, "`FIT_DEFAULT` in skin.rs"),
    );
  });

  it("name the same nine spots, and the same default", () => {
    expect(list(skinTs, /const SPOTS = \[([\s\S]*?)\];/, "`SPOTS` in skin.ts")).toEqual(
      list(skinRs, /pub const SPOTS: &\[&str\] = &\[([\s\S]*?)\];/, "`SPOTS` in skin.rs"),
    );
    expect(only(skinTs, /const SPOT_DEFAULT = "([a-z]+)";/, "`SPOT_DEFAULT` in skin.ts")).toBe(
      only(skinRs, /pub const SPOT_DEFAULT: &str = "([a-z]+)";/, "`SPOT_DEFAULT` in skin.rs"),
    );
  });
});

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

// The icons a skin may replace, held to the icons there are.
//
// `ICONS` in `crates/amenbo-core/src/skin.rs` is what the check searches to say whether a name the
// author wrote is one this build draws, and it is compiled into a binary that cannot read
// `Icon.tsx` at run time. An icon added here and not there is one a skin is told it may not
// replace — which is not what happened, and nothing else would show it.
describe("the icons a skin may replace", () => {
  it("are the ones this build draws, and no others", () => {
    const names = only(iconTsx, /export type IconName =\n([\s\S]*?);\n/, "`IconName` in Icon.tsx");
    const inTsx = [...names.matchAll(/"([A-Za-z]+)"/g)].map((m) => m[1]);
    expect(inTsx.length).toBeGreaterThan(0);

    const list = only(skinRs, /pub const ICONS: &\[&str\] = &\[([\s\S]*?)\];/, "`ICONS` in skin.rs");
    const inRs = [...list.matchAll(/"([A-Za-z]+)"/g)].map((m) => m[1]);

    expect([...inRs].sort()).toEqual([...inTsx].sort());
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
