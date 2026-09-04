// The whole document's TextMate state, kept across edits.
//
// Colour needs the lines on screen. **The editing manners need more than that**: where a line's
// indentation belongs is decided by the line above it, which is above the viewport as often as not,
// and which bracket the caret answers to can be anywhere at all. So the rule stacks a painter would
// keep for a screenful are kept here for the document, and everything that reads tokens reads them
// from one place rather than tokenizing the file again per feature.
//
// **An edit forgets forward, never backward.** A line that changed may end in a different state, so
// its stack and every stack after it are dropped; everything above is untouched, which is what keeps
// a keystroke from costing the file (`AMB-T-3745` measured 1-2 ms per keystroke at the panel's read
// cap).
//
// What a token IS, for the manners' purposes, is one of four things — VS Code decides it by running
// a single regular expression over the scope name, and so does this.

import { StateField, type Text } from "@codemirror/state";
import type { IGrammar, IToken, StateStack } from "@shikijs/vscode-textmate";

/** What a run of characters is, as far as the editing manners care. */
export type TokenType = "comment" | "string" | "regex" | "other";

// VS Code's own test, verbatim in effect (`tokenization.ts`): a scope is one of the three named
// kinds if any dotted segment of it says so. `notIn: ["string"]` is answered with this and nothing
// more, and it is not decoration — over half the quote characters typed in a file are typed inside
// a string or a comment (`AMB-T-3745`).
const NAMED = /\b(comment|string|regex|regexp)\b/;

function typeOf(scopes: readonly string[]): TokenType {
  for (let i = scopes.length - 1; i >= 0; i--) {
    const named = NAMED.exec(scopes[i]);
    if (named !== null) return named[1] === "regexp" ? "regex" : (named[1] as TokenType);
  }
  return "other";
}

/** One document's tokens, tokenized forward on demand and forgotten from the first line an edit touched. */
export class TmDoc {
  // `stacks[n]` is the rule stack line n + 1 starts from, so index 0 is where the grammar begins.
  private stacks: StateStack[];
  // The last line asked for, kept because the callers ask about the same line several times in a
  // row — the painter, then the indent rule, then the bracket under the caret.
  private cachedLine = -1;
  private cached: IToken[] = [];

  constructor(private readonly grammar: IGrammar, initial: StateStack) {
    this.stacks = [initial];
  }

  /** Forget the state of line `n` and everything after it. */
  forgetFrom(n: number): void {
    if (n < this.stacks.length) this.stacks.length = n;
    if (this.cachedLine >= n) this.cachedLine = -1;
  }

  /** The tokens of line `n` (1-based), tokenizing down from the last line already held. */
  tokensAt(doc: Text, n: number): IToken[] {
    if (this.cachedLine === n) return this.cached;
    for (let i = this.stacks.length; i < n; i++) {
      this.stacks[i] = this.grammar.tokenizeLine(doc.line(i).text, this.stacks[i - 1]).ruleStack;
    }
    const result = this.grammar.tokenizeLine(doc.line(n).text, this.stacks[n - 1]);
    this.stacks[n] = result.ruleStack;
    this.cachedLine = n;
    this.cached = result.tokens;
    return this.cached;
  }

  /** What the character at `column` of line `n` is part of. */
  typeAt(doc: Text, n: number, column: number): TokenType {
    for (const token of this.tokensAt(doc, n)) {
      if (column >= token.startIndex && column < token.endIndex) return typeOf(token.scopes);
    }
    return "other";
  }

  /**
   * Line `n` with every string, comment and regular expression blanked to spaces.
   *
   * This is the line an indentation rule is run against, because the rules are written to catch a
   * brace that opens a block and would otherwise catch one inside `"}}}"`. Blanking rather than
   * deleting keeps every other character where it was, so a pattern anchored to the start of the
   * line still means what it says.
   */
  bare(doc: Text, n: number): string {
    const text = doc.line(n).text;
    let out = "";
    let at = 0;
    for (const token of this.tokensAt(doc, n)) {
      if (token.startIndex > at) out += text.slice(at, token.startIndex);
      const run = text.slice(token.startIndex, token.endIndex);
      out += typeOf(token.scopes) === "other" ? run : " ".repeat(run.length);
      at = token.endIndex;
    }
    return out + text.slice(at);
  }
}

/**
 * Build the field that holds one editor's `TmDoc`.
 *
 * A field rather than a plugin because the manners are asked for from the state — an indent service
 * is handed an `IndentContext`, not a view — and one field per editor because the document it
 * follows is that editor's.
 */
export function tmDocField(grammar: IGrammar, initial: StateStack) {
  return StateField.define<TmDoc>({
    create: () => new TmDoc(grammar, initial),
    update(value, tr) {
      if (!tr.docChanged) return value;
      let first = Number.MAX_SAFE_INTEGER;
      tr.changes.iterChangedRanges((_fromA, _toA, fromB) => {
        first = Math.min(first, tr.state.doc.lineAt(fromB).number);
      });
      if (first !== Number.MAX_SAFE_INTEGER) value.forgetFrom(first);
      return value;
    },
  });
}

/** The field, once built — the handle every reader of tokens is given. */
export type TmField = ReturnType<typeof tmDocField>;
