// What a file's name says about the language it is written in, and that every language named here
// can actually be stood up. The colouring itself is CodeMirror's and is not exercised under jsdom
// (`./editorLoad`) — what is held here is everything before the editor.
import { describe, expect, it } from "vitest";

import { GRAMMARS, SCOPES, langFor, loadGrammar } from "./grammars";

describe("langFor", () => {
  it("reads the language off the suffix", () => {
    expect(langFor("main.rs")).toBe("rust");
    expect(langFor("Cargo.toml")).toBe("toml");
    expect(langFor("_ci.yml")).toBe("yaml");
  });

  // The JavaScript family is one grammar: VS Code's TypeScriptReact reads all four spellings.
  it("sends every JavaScript spelling to the one grammar that reads them all", () => {
    for (const name of ["a.js", "a.jsx", "a.ts", "a.tsx", "a.mjs", "a.cts"]) {
      expect(langFor(name)).toBe("tsx");
    }
  });

  it("reads a name that has no suffix to read", () => {
    expect(langFor(".zshrc")).toBe("shellscript");
    // The leading dot of a dotfile is not a suffix, so an unknown one is unknown rather than
    // matched on the whole name.
    expect(langFor(".gitignore")).toBeNull();
  });

  it("does not care how the name is cased", () => {
    expect(langFor("README.MD")).toBe("markdown");
    expect(langFor("Main.RS")).toBe("rust");
  });

  it("has no answer for a language nothing here bundles", () => {
    expect(langFor("notes.txt")).toBeNull();
    expect(langFor("Main.java")).toBeNull();
    expect(langFor("noext")).toBeNull();
  });
});

// Every language the map can name must be one the loader can fetch and the registry can enter, or
// opening that file throws where it should have drawn plain text.
it("names a module and a scope for every language it bundles", () => {
  for (const lang of Object.keys(GRAMMARS)) {
    expect(typeof GRAMMARS[lang as keyof typeof GRAMMARS]).toBe("function");
    expect(SCOPES[lang as keyof typeof GRAMMARS]).toMatch(/^(source|text)\./);
  }
});

// A scope name is a string nothing checks until a file is opened in that language, so getting one
// wrong shows up as a file that draws blank rather than as anything red.
describe("loadGrammar", () => {
  it("stands up every language it bundles", async () => {
    for (const lang of Object.keys(GRAMMARS) as (keyof typeof GRAMMARS)[]) {
      const { grammar, initial } = await loadGrammar(lang);
      expect(grammar.tokenizeLine("x", initial).tokens.length).toBeGreaterThan(0);
    }
  }, 60_000);

  it("reads a line with the scopes the colours are picked from", async () => {
    const { grammar, initial } = await loadGrammar("rust");
    const scopes = grammar.tokenizeLine("fn main() {} // hi", initial)
      .tokens.map((t) => t.scopes[t.scopes.length - 1]);
    expect(scopes).toContain("keyword.other.fn.rust");
    expect(scopes).toContain("entity.name.function.rust");
    expect(scopes).toContain("comment.line.double-slash.rust");
  });

  // The stack is what carries a block comment across a line break, and the whole painter is built
  // on handing it back in.
  it("carries what a line left open into the next one", async () => {
    const { grammar, initial } = await loadGrammar("rust");
    const opened = grammar.tokenizeLine("/* still", initial);
    const inside = grammar.tokenizeLine("open */", opened.ruleStack);
    expect(inside.tokens.every((t) => t.scopes.includes("comment.block.rust"))).toBe(true);
    // The same line read from the top of a file is not a comment at all — the stack is the whole
    // difference, and it is the thing the painter has to hold on to.
    const cold = grammar.tokenizeLine("open */", initial);
    expect(cold.tokens.some((t) => t.scopes.includes("comment.block.rust"))).toBe(false);
  });
});
