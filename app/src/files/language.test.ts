// What the editor does with a file beyond drawing it: where it puts a new line, what it will fold
// away, and which bracket it says is the partner of the one under the caret.
//
// All three are held against real snippets rather than synthetic ones, because the thing that makes
// them hard is real text — a brace inside a string, a comment on the line that decides the indent.
// None of it needs a screen: indentation and folding are asked for from the state.
import { foldable, getIndentation, indentUnit } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { partner } from "./brackets";
import { compile, configFor } from "./langconfig";
import { language } from "./language";
import { loadGrammar, type LangId } from "./grammars";
import { tmDocField, TmDoc } from "./tmdoc";

async function stateOf(lang: LangId | null, doc: string) {
  return EditorState.create({
    doc,
    extensions: [indentUnit.of("    "), await language(lang)],
  });
}

/** Where the editor would put the caret if Enter were pressed at the end of `doc`. */
async function indentAfter(lang: LangId, doc: string) {
  const state = await stateOf(lang, `${doc}\n`);
  return getIndentation(state, state.doc.length);
}

describe("indentation", () => {
  it("goes one level in after a line that opens a block", async () => {
    expect(await indentAfter("rust", "fn main() {")).toBe(4);
    expect(await indentAfter("tsx", "function f() {")).toBe(4);
  });

  it("stays where it is on a line that opens nothing", async () => {
    expect(await indentAfter("rust", "fn main() {\n    let x = 1;")).toBe(4);
  });

  // The rule that reads the line being typed rather than the one above it.
  it("comes back out on the line that closes the block", async () => {
    const state = await stateOf("rust", "fn main() {\n    let x = 1;\n}");
    expect(getIndentation(state, state.doc.line(3).from)).toBe(0);
  });

  // The reason the rules are run against the line with its literals blanked. Read raw, this line
  // ends in `{` inside a string and would push the next one in.
  it("is not fooled by a brace inside a string", async () => {
    expect(await indentAfter("rust", 'fn main() {\n    let s = "{";')).toBe(4);
  });

  it("has an answer for a language nothing was baked for", async () => {
    // TOML has no VS Code extension, so this is the generic bracket rule and nothing else.
    expect(await indentAfter("toml", "a = [")).toBe(4);
  });
});

// Pressing Enter is its own path, and for some languages it is the only one that says anything:
// Python's rule about a colon is written here rather than in the indentation patterns. Driven
// through the state rather than a view, which is what these tests can stand up.
describe("what pressing Enter does", () => {
  async function enterAt(lang: LangId, doc: string, at: number) {
    const state = await stateOf(lang, doc);
    const rules = compile(await configFor(lang));
    const line = state.doc.lineAt(at);
    const before = line.text.slice(0, at - line.from);
    const after = line.text.slice(at - line.from);
    const previous = line.number > 1 ? state.doc.line(line.number - 1).text : "";
    return rules.onEnter.find((one) =>
      one.before.test(before)
      && (one.after === null || one.after.test(after))
      && (one.previous === null || one.previous.test(previous)));
  }

  // Python has no indentation patterns at all: a colon opening a block is stated as an Enter rule
  // and nowhere else, so a reader who skipped this half would find Python did not indent.
  it("opens a block after a line that ends in a colon", async () => {
    const doc = "def f():";
    expect((await enterAt("python", doc, doc.length))?.indent).toBe("indent");
  });

  it("says nothing after a line that opens nothing", async () => {
    const doc = "x = 1";
    expect(await enterAt("python", doc, doc.length)).toBeUndefined();
  });

  // The other thing an Enter rule can do: carry text onto the new line, which is how a comment
  // keeps going rather than stopping at the line break. Rust asks for something after the caret,
  // so this fires where a comment is split and not where one simply ends.
  it("carries a line comment onto the line a split makes", async () => {
    const doc = "// a comment";
    const rule = await enterAt("rust", doc, doc.indexOf("comment"));
    expect(rule?.appendText).toBe("// ");
    expect(rule?.indent).toBe("none");
    expect(await enterAt("rust", doc, doc.length)).toBeUndefined();
  });
});

describe("folding", () => {
  it("folds what is pushed in under a line, and stops where it comes back out", async () => {
    const state = await stateOf("rust", "fn main() {\n    let x = 1;\n    let y = 2;\n}\n");
    const range = foldable(state, state.doc.line(1).from, state.doc.line(1).to);
    // The opening line stays on screen and the closing brace, being at its depth, stays with it.
    expect(range).toEqual({ from: state.doc.line(1).to, to: state.doc.line(3).to });
  });

  it("folds a language that closes by coming back out, with no bracket to look for", async () => {
    const state = await stateOf("python", "def f():\n    a = 1\n    b = 2\nc = 3\n");
    const range = foldable(state, state.doc.line(1).from, state.doc.line(1).to);
    expect(range).toEqual({ from: state.doc.line(1).to, to: state.doc.line(3).to });
  });

  it("leaves a blank line at the tail outside the fold", async () => {
    const state = await stateOf("rust", "fn a() {\n    let x = 1;\n\n}\n");
    expect(foldable(state, 0, state.doc.line(1).to)?.to).toBe(state.doc.line(2).to);
  });

  // Folding is a count of the spaces at the head of a line, so a file written in nothing this panel
  // reads still folds. It is the one manner that never needed a grammar.
  it("folds a file whose language nothing here knows", async () => {
    const state = await stateOf(null, "one:\n    two\n    three\nfour\n");
    expect(foldable(state, 0, state.doc.line(1).to)?.to).toBe(state.doc.line(3).to);
  });

  it("has nothing to fold on a line nothing is under", async () => {
    const state = await stateOf("rust", "let x = 1;\nlet y = 2;\n");
    expect(foldable(state, 0, state.doc.line(1).to)).toBeNull();
  });

  it("folds a marked region whole, past the lines inside it", async () => {
    const state = await stateOf("rust", "// #region one\nfn a() {}\nfn b() {}\n// #endregion\n");
    expect(foldable(state, 0, state.doc.line(1).to)?.to).toBe(state.doc.line(4).to);
  });
});

describe("the partner of a bracket", () => {
  async function scan(lang: LangId, doc: string, at: number) {
    const { grammar, initial } = await loadGrammar(lang);
    const field = tmDocField(grammar, initial);
    const state = EditorState.create({ doc, extensions: [field] });
    const tokens: TmDoc = state.field(field);
    const rules = compile(await configFor(lang));
    const ch = doc[at];
    const pair = rules.brackets.find(([o, c]) => ch === o || ch === c)!;
    return partner(tokens, state, at, pair[0], pair[1], ch === pair[0]);
  }

  // Reading the text alone, the first `}` is the one inside the string. This is the failure the
  // whole token-aware scan exists to avoid.
  it("steps over a bracket that is inside a string", async () => {
    const doc = 'fn main() {\n    let s = "}}}";\n    let t = 1;\n}\n';
    expect(await scan("rust", doc, doc.indexOf("{"))).toBe(doc.lastIndexOf("}"));
  });

  it("steps over one inside a comment", async () => {
    const doc = "fn main() {\n    // }\n}\n";
    expect(await scan("rust", doc, doc.indexOf("{"))).toBe(doc.lastIndexOf("}"));
  });

  it("counts the pairs in between", async () => {
    const doc = "fn a() { if b() { c(); } }\n";
    expect(await scan("rust", doc, doc.indexOf("{"))).toBe(doc.lastIndexOf("}"));
  });

  it("searches backward from a closing bracket too", async () => {
    const doc = "fn a() {\n    b();\n}\n";
    expect(await scan("rust", doc, doc.lastIndexOf("}"))).toBe(doc.indexOf("{"));
  });

  // Nearly every bracket in a YAML file is inside a string, because a bare scalar is one: 98.0% of
  // them, measured. A scan that read the text alone would be wrong almost every time here.
  it("steps over the brackets a data format writes into its own values", async () => {
    const doc = "a: [1, 2]\nb: not ] a bracket\nc: 3\n";
    expect(await scan("yaml", doc, doc.indexOf("["))).toBe(doc.indexOf("]"));
  });

  it("has no answer for a bracket nothing closes", async () => {
    expect(await scan("rust", "fn a() {\n    b();\n", 7)).toBeNull();
  });
});
