// Which colour each run of characters is drawn in — the half of the bridge between a TextMate
// grammar and CodeMirror that is about looking at a file rather than editing one (`AMB-D-769`).
//
// **Only the viewport is painted.** The tokens themselves come from `./tmdoc`, which holds them for
// the whole document because the editing manners need lines the screen does not show; what is done
// here is turning the tokens of the lines actually on screen into decorations, which is cheap and
// is redone on every scroll and every keystroke.

import { RangeSetBuilder, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import type { TmField } from "./tmdoc";

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

/** Build the extension that paints one editor, reading its tokens out of `field`. */
export function painting(field: TmField): Extension {
  const painter = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;

      constructor(view: EditorView) {
        this.decorations = this.paint(view);
      }

      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) this.decorations = this.paint(update.view);
      }

      private paint(view: EditorView): DecorationSet {
        const doc = view.state.doc;
        const tokens = view.state.field(field);
        const from = doc.lineAt(view.viewport.from).number;
        const to = doc.lineAt(view.viewport.to).number;

        const builder = new RangeSetBuilder<Decoration>();
        for (let n = from; n <= to; n++) {
          const line = doc.line(n);
          for (const token of tokens.tokensAt(doc, n)) {
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
