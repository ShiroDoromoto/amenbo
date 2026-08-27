// Brackets: closing one as it is opened, wrapping a selection in a pair, and drawing the partner of
// the one under the caret.
//
// All three need to know what a character is part of, and that is the whole reason they are written
// here rather than taken off the shelf. CodeMirror's own bracket matching walks the syntax tree and,
// with no tree to walk, falls back to reading the text — which pairs the `{` opening a function
// with a `}` inside `"}}}"` (measured). It is not a rare accident: the share of bracket characters
// sitting inside a string or a comment is 17.8% in Rust, 11.0% in TypeScript — and **98.0% in
// YAML**, where a bare scalar is a string (`AMB-T-3745`).
//
// The same knowledge answers `notIn`, the field beside a pair in the language configuration saying
// where it must not be closed. Over half the quote characters typed into a file are typed inside a
// string or a comment, so it is not decoration either.

import { EditorSelection, MapMode, Prec, StateEffect, StateField, type EditorState, type Extension, type Range } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, keymap, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import type { Pair, Rules } from "./langconfig";
import type { TmDoc, TmField } from "./tmdoc";

// Announcing a closing half this editor has just put in, so the field below can hold on to it.
const rememberClose = StateEffect.define<number>();

// Where the closing halves this editor inserted itself are sitting. Typing the closing character
// should step over one of those and only one of those: an editor that steps over every closing
// character it meets eats the one somebody meant to type.
const inserted = StateField.define<readonly number[]>({
  create: () => [],
  update(value, tr) {
    const kept: number[] = [];
    for (const at of value) {
      const to = tr.docChanged ? tr.changes.mapPos(at, -1, MapMode.TrackDel) : at;
      if (to !== null) kept.push(to);
    }
    // The effect carries a position in the document the transaction produced, so it is appended
    // rather than mapped.
    for (const effect of tr.effects) if (effect.is(rememberClose)) kept.push(effect.value);
    return kept;
  },
});

/** What the character at `pos` is part of. */
export function kindAt(tokens: TmDoc, state: EditorState, pos: number): string {
  const line = state.doc.lineAt(pos);
  return tokens.typeAt(state.doc, line.number, pos - line.from);
}

/** The pair whose opening text ends at `pos`, longest first so `f"` wins over `"`. */
function opening(pairs: Pair[], before: string): Pair | null {
  let best: Pair | null = null;
  for (const pair of pairs) {
    if (before.endsWith(pair.open) && (best === null || pair.open.length > best.open.length)) best = pair;
  }
  return best;
}

/**
 * Typing, where typing means something other than putting the character in.
 *
 * Three cases, and everything else falls through to CodeMirror: a pair is opened over a selection
 * and wraps it, a pair is opened with nothing selected and closes itself, and a closing character
 * is typed where this editor already put one.
 */
function typing(field: TmField, rules: Rules): Extension {
  return Prec.high(EditorView.inputHandler.of((view, from, to, text) => {
    if (text.length !== 1 || view.state.readOnly) return false;
    const tokens = view.state.field(field);

    // Wrapping comes first: with something selected, the character is a pair of quotes around it
    // rather than a replacement of it.
    if (view.state.selection.ranges.some((range) => !range.empty)) {
      const pair = opening(rules.surrounding, text);
      if (pair === null) return false;
      view.dispatch(view.state.changeByRange((range) => ({
        changes: [
          { from: range.from, insert: pair.open },
          { from: range.to, insert: pair.close },
        ],
        range: EditorSelection.range(range.from + pair.open.length, range.to + pair.open.length),
      })), { userEvent: "input.type" });
      return true;
    }

    const line = view.state.doc.lineAt(from);
    const before = line.text.slice(0, from - line.from) + text;

    // Stepping over a closer this editor put in. Only one it put in, and only when the caret is
    // sitting right on it.
    if (view.state.field(inserted).includes(from) && view.state.doc.sliceString(from, from + 1) === text) {
      view.dispatch({ selection: EditorSelection.cursor(from + 1), userEvent: "input.type" });
      return true;
    }

    const pair = opening(rules.closing, before);
    if (pair === null) return false;
    // `notIn` is the language saying where this pair is not a pair — a quote inside a string is the
    // end of one, not the start of another.
    if (pair.notIn?.includes(kindAt(tokens, view.state, from)) === true) return false;

    const at = from + text.length;
    view.dispatch({
      changes: { from, to, insert: text + pair.close },
      selection: EditorSelection.cursor(at),
      effects: rememberClose.of(at),
      userEvent: "input.type",
    });
    return true;
  }));
}

/** Backspacing between the two halves of a pair takes both. */
function unwrapping(rules: Rules): Extension {
  // Above the default keymap, which binds Backspace to taking one character. Below it this would
  // never be reached, because the first binding that answers wins.
  return Prec.high(keymap.of([{
    key: "Backspace",
    run(view) {
      const state = view.state;
      if (state.readOnly) return false;
      const ranges = state.selection.ranges;
      if (!ranges.every((r) => r.empty)) return false;
      const changes = ranges.map((range) => {
        for (const pair of rules.closing) {
          const open = state.doc.sliceString(range.from - pair.open.length, range.from);
          const close = state.doc.sliceString(range.from, range.from + pair.close.length);
          if (open === pair.open && close === pair.close) {
            return { from: range.from - pair.open.length, to: range.from + pair.close.length };
          }
        }
        return null;
      });
      if (changes.some((c) => c === null)) return false;
      view.dispatch({ changes: changes as { from: number; to: number }[], userEvent: "delete.backward" });
      return true;
    },
  }]));
}

const MATCHED = Decoration.mark({ class: "tm-bracket" });
const UNMATCHED = Decoration.mark({ class: "tm-bracket-lost" });

// How many lines a search for the partner will read before giving up. A bracket whose partner is
// two thousand lines away is one nobody is looking at, and the search is run on every cursor move.
const SCAN_LINES = 2_000;

/**
 * Scan for the partner of the bracket at `pos`, stepping over every one inside a literal.
 *
 * A line at a time rather than a character at a time: the tokens are held per line, and asking the
 * document for one character two hundred thousand times is the slow way to read it.
 */
export function partner(
  tokens: TmDoc,
  state: EditorState,
  pos: number,
  open: string,
  close: string,
  forward: boolean,
): number | null {
  const doc = state.doc;
  const from = doc.lineAt(pos);
  const last = forward
    ? Math.min(doc.lines, from.number + SCAN_LINES)
    : Math.max(1, from.number - SCAN_LINES);
  let depth = 0;

  for (let n = from.number; forward ? n <= last : n >= last; n += forward ? 1 : -1) {
    const line = doc.line(n);
    const first = n === from.number && forward ? pos - line.from : 0;
    const stop = n === from.number && !forward ? pos - line.from : line.text.length - 1;
    for (let i = forward ? first : stop; forward ? i <= line.text.length - 1 : i >= 0; i += forward ? 1 : -1) {
      const here = line.text[i];
      if (here !== open && here !== close) continue;
      if (tokens.typeAt(doc, n, i) !== "other") continue;
      depth += (here === open) === forward ? 1 : -1;
      if (depth === 0) return line.from + i;
    }
  }
  return null;
}

/** Draw the pair the caret is sitting on, if it is sitting on one. */
function matching(field: TmField, rules: Rules): Extension {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = this.find(view);
      }
      update(update: ViewUpdate) {
        if (update.docChanged || update.selectionSet || update.viewportChanged) {
          this.decorations = this.find(update.view);
        }
      }
      private find(view: EditorView): DecorationSet {
        const head = view.state.selection.main.head;
        const tokens = view.state.field(field);
        const marks: Range<Decoration>[] = [];
        // The caret is "on" a bracket when one is on either side of it, which is what makes the
        // partner light up as it is typed as well as when it is walked onto.
        for (const at of [head - 1, head]) {
          if (at < 0 || at >= view.state.doc.length) continue;
          const ch = view.state.doc.sliceString(at, at + 1);
          const pair = rules.brackets.find(([o, c]) => ch === o || ch === c);
          if (pair === undefined) continue;
          if (kindAt(tokens, view.state, at) !== "other") continue;
          const found = partner(tokens, view.state, at, pair[0], pair[1], ch === pair[0]);
          if (found === null) {
            marks.push(UNMATCHED.range(at, at + 1));
          } else {
            marks.push(MATCHED.range(at, at + 1), MATCHED.range(found, found + 1));
          }
          break;
        }
        marks.sort((a, b) => a.from - b.from);
        return Decoration.set(marks);
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );
}

// A box rather than a colour: the bracket already carries whichever colour its scope gave it, and
// repainting it would say something about what it is instead of where its partner went.
const PAINT = EditorView.theme({
  ".tm-bracket": { outline: "1px solid var(--c-border-strong)", borderRadius: "2px" },
  ".tm-bracket-lost": { color: "var(--c-code-invalid)", fontWeight: "var(--fw-bold)" },
});

/** Everything about brackets, for one language. */
export function brackets(field: TmField, rules: Rules): Extension {
  return [
    inserted,
    typing(field, rules),
    unwrapping(rules),
    matching(field, rules),
    PAINT,
  ];
}
