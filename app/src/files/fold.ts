// Where a block a reader can fold away begins and ends.
//
// **This needs no grammar at all.** A block is a run of lines pushed in further than the line that
// opens it, and counting the spaces at the head of a line is the whole of the measurement. Held
// against what a parse tree folds, the rules agree almost everywhere and fold rather more places —
// 504 against 467 in Rust — because a multi-line comment or a match arm has no node of its own but
// is indented all the same (`AMB-T-3745`).
//
// Amenbo's own repository contains no `#region` marker at all, which is the other half of the same
// finding: folding in practice is decided by indentation, and the markers are a second answer for
// the few files that ask for one.

import { foldService } from "@codemirror/language";
import type { EditorState, Extension } from "@codemirror/state";
import type { Rules } from "./langconfig";

/** How far in the line is pushed, or null for a line that is only whitespace. */
function indentOf(text: string, tabSize: number): number | null {
  let columns = 0;
  for (const ch of text) {
    if (ch === "\t") columns += tabSize - (columns % tabSize);
    else if (ch === " ") columns += 1;
    else return columns;
  }
  return null;
}

/**
 * The block that starts on the line covering `lineStart`, if one does.
 *
 * The range returned begins at the end of the opening line, so the line itself stays visible and
 * what is hidden is what it holds. A blank line at the tail of a block is left out of it: it reads
 * as the space before whatever comes next rather than as part of what came before.
 */
function byIndent(state: EditorState, lineStart: number): { from: number; to: number } | null {
  const doc = state.doc;
  const opening = doc.lineAt(lineStart);
  const own = indentOf(opening.text, state.tabSize);
  if (own === null) return null;

  let end = 0;
  for (let n = opening.number + 1; n <= doc.lines; n++) {
    const line = doc.line(n);
    const indent = indentOf(line.text, state.tabSize);
    if (indent === null) continue; // blank: it belongs to the block only if something deeper follows
    if (indent <= own) break;
    end = n;
  }
  if (end === 0) return null;

  // The end is the last line pushed in further than the opening one, which leaves a closing bracket
  // — sitting at the opening line's own depth — outside the fold and still on screen. That is where
  // a reader looks to see what was folded away.
  return { from: opening.to, to: doc.line(end).to };
}

/** The block a pair of `#region` markers names, if `lineStart` is on the opening one. */
function byMarker(state: EditorState, lineStart: number, rules: Rules): { from: number; to: number } | null {
  if (rules.foldStart === null || rules.foldEnd === null) return null;
  const doc = state.doc;
  const opening = doc.lineAt(lineStart);
  if (!rules.foldStart.test(opening.text)) return null;

  let depth = 1;
  for (let n = opening.number + 1; n <= doc.lines; n++) {
    const text = doc.line(n).text;
    if (rules.foldStart.test(text)) depth += 1;
    else if (rules.foldEnd.test(text)) {
      depth -= 1;
      if (depth === 0) return { from: opening.to, to: doc.line(n).to };
    }
  }
  return null;
}

/** Build the folding service for one language. Markers win where a line carries one. */
export function folding(rules: Rules): Extension {
  return foldService.of((state, lineStart) =>
    byMarker(state, lineStart, rules) ?? byIndent(state, lineStart));
}
