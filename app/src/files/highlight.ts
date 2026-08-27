// The bridge between a TextMate grammar and CodeMirror: about a hundred lines, because no published
// one exists (`AMB-D-769`). What it owes CodeMirror is a `DecorationSet` over the visible lines;
// what it owes the grammar is that lines reach it in order, each with the rule stack the line above
// left behind.
//
// **Only the viewport is painted, and only from the last stack we already hold.** A TextMate
// grammar has no way to start in the middle of a document — whether a line is inside a block
// comment is the previous line's answer — so a naive painter would re-read the whole file on every
// keystroke. Holding one stack per line turns that into: a typed character throws away the stacks
// below it, and the next paint reads forward only as far as the screen. Measured, a keystroke is
// 1-2 ms and opening the largest file the panel will read is about 120 ms (`AMB-T-3738`).
//
// The grammar arrives asynchronously and the editor does not wait for it: an uncoloured file is
// what this panel drew before, so a grammar that never loads costs nothing but colour.

import { RangeSetBuilder, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { loadGrammar, type LangId } from "./grammars";

// The scopes, longest first, so `keyword.operator` is read as an operator rather than as a keyword.
// A prefix matches a scope that equals it or continues it after a dot — never `constant` matching
// `constantly`, which is why the dot is spelled out rather than left to `startsWith`.
const KINDS: [prefix: string, kind: string][] = [
  ["entity.other.attribute-name", "attribute"],
  ["entity.name.function", "function"],
  ["entity.name.section", "heading"],
  ["entity.name.tag", "tag"],
  ["entity.name.type", "type"],
  ["entity.name.class", "type"],
  ["constant.numeric", "number"],
  ["keyword.operator", "operator"],
  ["markup.inline.raw", "string"],
  ["support.function", "function"],
  ["support.class", "type"],
  ["support.type", "type"],
  ["markup.heading", "heading"],
  ["punctuation.definition.comment", "comment"],
  // The name a key is written under. Every data format has one and each grammar calls it something
  // else — a tag in YAML, a section in TOML, a property name in JSON — so the three are brought to
  // the same place rather than left to read as three different kinds of thing.
  ["variable.other.key", "tag"],
  ["constant", "constant"],
  ["comment", "comment"],
  ["invalid", "invalid"],
  ["keyword", "keyword"],
  ["storage", "keyword"],
  ["string", "string"],
  ["variable", "variable"],
];

/**
 * Which class a token is drawn in, or null for a token nothing here has an opinion about.
 *
 * `scopes` is the stack TextMate hands back, outermost first, so the search runs from the end: the
 * innermost scope that anything here recognises is the one that describes the token.
 */
export function kindOf(scopes: readonly string[]): string | null {
  for (let i = scopes.length - 1; i >= 0; i--) {
    const scope = scopes[i];
    for (const [prefix, kind] of KINDS) {
      if (scope === prefix || scope.startsWith(`${prefix}.`)) return kind;
    }
  }
  return null;
}

// Colour is the panel's, not a ported editor theme's: one token per kind, resolved against the
// theme that is up (`styles/tokens.css`). A kind with no colour of its own is drawn in the body
// text's, which is what leaving it out of this list means.
const KIND_NAMES = [...new Set(KINDS.map(([, kind]) => kind))];
const PAINT = Object.fromEntries(
  KIND_NAMES.map((kind) => [`.tm-${kind}`, { color: `var(--c-code-${kind})` }]),
);

// One mark per kind rather than one per token: a paint of a full screen draws thousands of tokens,
// and CodeMirror compares decorations by identity when it works out what actually moved.
const MARKS: Record<string, Decoration> = Object.fromEntries(
  KIND_NAMES.map((kind) => [kind, Decoration.mark({ class: `tm-${kind}` })]),
);

/** Build the extension that colours `lang`, once its grammar has been fetched. */
export async function textmate(lang: LangId): Promise<Extension> {
  const { grammar, initial } = await loadGrammar(lang);

  const painter = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      // `stacks[n]` is the rule stack a document's line n + 1 starts from, so index 0 is where the
      // grammar itself starts. Truncating this array is how an edit is forgotten.
      private stacks = [initial];

      constructor(view: EditorView) {
        this.decorations = this.paint(view);
      }

      update(update: ViewUpdate) {
        if (!update.docChanged && !update.viewportChanged) return;
        if (update.docChanged) {
          let first = Number.MAX_SAFE_INTEGER;
          update.changes.iterChangedRanges((_fromA, _toA, fromB) => {
            first = Math.min(first, update.state.doc.lineAt(fromB).number);
          });
          // A line that changed may end differently, so the stack it hands on is unknown — and so
          // is every stack after it. What is kept is everything strictly above.
          if (first < this.stacks.length) this.stacks.length = first;
        }
        this.decorations = this.paint(update.view);
      }

      private paint(view: EditorView): DecorationSet {
        const doc = view.state.doc;
        const from = doc.lineAt(view.viewport.from).number;
        const to = doc.lineAt(view.viewport.to).number;

        // Reading forward to the top of the viewport is the cost of a grammar with no way in from
        // the middle. It is paid once per scroll into new ground, and never again while the lines
        // above stay untouched — which is what makes a keystroke cost the screen rather than the
        // file.
        for (let n = this.stacks.length; n < from; n++) {
          this.stacks[n] = grammar.tokenizeLine(doc.line(n).text, this.stacks[n - 1]).ruleStack;
        }

        const builder = new RangeSetBuilder<Decoration>();
        for (let n = from; n <= to; n++) {
          const line = doc.line(n);
          const result = grammar.tokenizeLine(line.text, this.stacks[n - 1]);
          this.stacks[n] = result.ruleStack;
          for (const token of result.tokens) {
            const kind = kindOf(token.scopes);
            if (kind === null || token.startIndex === token.endIndex) continue;
            builder.add(line.from + token.startIndex, line.from + token.endIndex, MARKS[kind]);
          }
        }
        return builder.finish();
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );

  return [painter, EditorView.theme(PAINT)];
}
