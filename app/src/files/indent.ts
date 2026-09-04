// Where a line's indentation belongs, decided from rules rather than from a syntax tree.
//
// The rules are VS Code's, and so is the procedure: find the line above that has something on it,
// take its indentation, and move one unit in or out if a pattern says to. It reads as too little to
// work and it is not. Run over this repository's own files and held to where their formatters
// actually put each line, it puts 96.0% of Rust and 95.8% of TypeScript exactly right — above what
// a parse tree manages (87.2% and 87.7%), because a hand-written grammar falls behind the language
// it parses while a rule about a brace does not (`AMB-T-3745`).
//
// **Where it loses is the languages that close by going back out.** Python and YAML land in the 70s
// against a parse tree's 90s, and no amount of rewriting the rules fixes it: the vocabulary has
// "one deeper" and "one shallower", and "one shallower from here on" cannot be said in it. Those
// two are answered with a parse tree instead, which is its own work (`AMB-D-769`).
//
// The rules are run against the line with its strings and comments blanked (`./tmdoc`), so a brace
// inside `"}}}"` is not read as one that opened a block.

import { indentService, indentUnit, type IndentContext } from "@codemirror/language";
import { EditorSelection, Prec, type Extension, type Text } from "@codemirror/state";
import { keymap, type EditorView } from "@codemirror/view";
import type { Rules } from "./langconfig";
import type { TmDoc, TmField } from "./tmdoc";

/** How far the line's text is pushed in, in columns. A line with nothing on it is pushed in none. */
function indentOf(text: string, tabSize: number): number {
  let columns = 0;
  for (const ch of text) {
    if (ch === "\t") columns += tabSize - (columns % tabSize);
    else if (ch === " ") columns += 1;
    else return columns;
  }
  return 0;
}

/**
 * The text of the line above `from`, as far as the caret has got, or null for none.
 *
 * "Above" is measured against a line break that may not exist yet: pressing Enter asks where the
 * new line goes **before** inserting it, so the line the answer depends on is the one the caret is
 * still sitting in, cut at the caret. Reading the document alone would look one line too far up.
 *
 * `bare` is what comes back — the line with its strings and comments blanked — because what this is
 * read for is the patterns. Where the line sits is read off the line as written.
 */
function above(
  doc: Text,
  tokens: TmDoc,
  from: number,
  tabSize: number,
): { text: string; column: number; from: number } | null {
  for (let at = from; at > 0;) {
    const line = doc.lineAt(at - 1);
    const text = tokens.bare(doc, line.number).slice(0, Math.min(at - line.from, line.length));
    if (text.trim() !== "") return { text, column: indentOf(line.text, tabSize), from: line.from };
    at = line.from;
  }
  return null;
}

/** Build the indentation for one language: the rules, and what pressing Enter does. */
export function indenting(field: TmField, rules: Rules): Extension {
  return [onEnter(field, rules), indentService.of((context: IndentContext, pos: number): number => {
    const doc = context.state.doc;
    const tokens = context.state.field(field);
    const unit = context.state.facet(indentUnit).length || context.state.tabSize;

    // The line the text will start on, which is not always a line the document has yet.
    const tabSize = context.state.tabSize;
    const here = context.lineAt(pos, 1);
    const before = above(doc, tokens, here.from, tabSize);
    if (before === null) return 0;

    let indent = before.column;

    // A line the language says stands outside the block structure — the middle of a doc comment, a
    // label — tells nothing about where the next one goes, so its own column is not inherited.
    if (rules.unIndented?.test(before.text) === true) {
      indent = above(doc, tokens, before.from, tabSize)?.column ?? indent;
    }

    if (rules.increase?.test(before.text) === true) indent += unit;
    else if (rules.indentNext?.test(before.text) === true) indent += unit;

    // The only rule that reads the line being written rather than the one above it: `}` and `else`
    // pull themselves back out as they are typed. A line that does not exist yet has nothing on it,
    // so this cannot fire on the way out of pressing Enter — only on the way through a re-indent.
    if (rules.decrease?.test(here.text) === true) indent -= unit;

    return Math.max(0, indent);
  })];
}

/**
 * What a language says about pressing Enter, where the rules above have nothing to say.
 *
 * This is the other half of the configuration and it is not an afterthought: Python's whole
 * indentation rule lives here rather than in `indentationRules`, because "a line ending in a colon
 * opens a block" is a fact about the Enter key and not about the line. The rules can also carry text
 * into the new line, which is how a `//` comment or a `/** ... *\/` block keeps going.
 *
 * Only a plain caret is handled. With a selection, or with several, this steps aside and lets the
 * ordinary new line happen: what the rules describe is what follows one line, and there is no one
 * line when a selection is being replaced.
 */
function onEnter(field: TmField, rules: Rules): Extension {
  if (rules.onEnter.length === 0) return [];
  // Above the default keymap, whose Enter would otherwise answer first.
  return Prec.high(keymap.of([{
    key: "Enter",
    run(view: EditorView) {
      const state = view.state;
      if (state.readOnly) return false;
      const range = state.selection.main;
      if (!range.empty || state.selection.ranges.length !== 1) return false;

      const doc = state.doc;
      const line = doc.lineAt(range.head);
      const tokens = state.field(field);
      const at = range.head - line.from;
      const before = tokens.bare(doc, line.number).slice(0, at);
      const after = line.text.slice(at);
      const previous = line.number > 1 ? tokens.bare(doc, line.number - 1) : "";

      const rule = rules.onEnter.find((one) =>
        one.before.test(before)
        && (one.after === null || one.after.test(after))
        && (one.previous === null || one.previous.test(previous)));
      if (rule === undefined) return false;

      const unit = state.facet(indentUnit).length || state.tabSize;
      const own = indentOf(line.text, state.tabSize);
      const deeper = rule.indent === "indent" || rule.indent === "indentOutdent";
      const column = Math.max(0, rule.indent === "outdent" ? own - unit : deeper ? own + unit : own);
      const carried = rule.appendText.slice(0, rule.appendText.length - rule.removeText);
      const opened = `\n${" ".repeat(column)}${carried}`;

      // `indentOutdent` writes two lines: the caret lands on the deeper one, and what was after it
      // — the `*/` closing a doc comment — goes onto a third at the original depth.
      const insert = rule.indent === "indentOutdent"
        ? `${opened}\n${" ".repeat(own)}`
        : opened;

      view.dispatch({
        changes: { from: range.head, insert },
        selection: EditorSelection.cursor(range.head + opened.length),
        userEvent: "input",
      });
      return true;
    },
  }]));
}
