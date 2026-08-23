// Reading amenbo's refs back out of whatever a program drew in a pane.
//
// A pane is a real terminal (`AMB-D-747`), so what runs in it writes characters and nothing else:
// `AMB-T-42` in an agent's output is a task to amenbo and a string to the terminal. This is the half
// that turns the one into the other, and the whole point of owning a terminal — a standard one has
// no way to know that some of the characters it drew are records.
//
// It reads the **drawn buffer**, not the byte stream. By the time a character sits in a cell, every
// escape that coloured it or moved the cursor there has already been spent, so this works for any
// program at all rather than for the ones whose output happens to be plain.
//
// **Wrapping is the difficulty.** A pane is narrow, a TUI wraps at its width, and a ref broken
// across two rows is a ref no pattern finds in either of them. So a row is never read on its own:
// the rows one logical line was drawn across are joined back together first, the joining is
// searched, and every hit is mapped back to the cells it came from — which is how a link can span
// the fold.
//
// **Elision is not recoverable, and is not pretended to be.** A TUI that ran out of width writes
// `…`, and the characters it stands for were never drawn. There is nothing in the buffer to find.
//
// The cost of being wrong is small in both directions, which is what makes reading text this way
// worth doing at all: a false hit opens a task that exists, and a miss leaves characters that were
// never clickable anyway.

import { REF_RE, parseRef, type RefSpace } from "../core/idref";

/** One cell of a drawn row: what was drawn in it, and how much of the row it takes up. */
export interface Cell {
  /** The characters drawn there — `""` where nothing was. */
  chars: string;
  /**
   * Columns occupied: 1 ordinarily, 2 for a wide character, and 0 for the column such a character
   * spills into. The zero is why a position cannot be arithmetic on an offset: one Japanese
   * character earlier in the row and every column after it has moved.
   */
  width: number;
}

/** As much of a terminal's buffer as a scan needs to know about. Rows are 0-based, scrollback included. */
export interface Rows {
  /** How many rows the buffer holds. */
  readonly length: number;
  /** Whether row `y` is the continuation of the row above it, rather than a line of its own. */
  wrapped(y: number): boolean;
  /** The cells of row `y`, one entry per column. */
  cells(y: number): readonly Cell[];
}

/** A position in the buffer, in the 1-based coordinates a terminal's own ranges are written in. */
export interface At {
  x: number;
  y: number;
}

/** A ref found in the buffer: what it says, what it names, and where it sits. */
export interface TerminalRef {
  /** The ref as it was drawn, e.g. `AMB-T-42`. */
  text: string;
  /** Which space it names. */
  space: RefSpace;
  /** The number it names. */
  num: number;
  /** Where it sits — both ends inclusive, spanning two rows where it was drawn across the fold. */
  range: { start: At; end: At };
}

/** The first row of the logical line `y` belongs to. */
function firstRow(rows: Rows, y: number): number {
  let top = y;
  while (top > 0 && rows.wrapped(top)) top--;
  return top;
}

/** The last row of the logical line `y` belongs to. */
function lastRow(rows: Rows, y: number): number {
  let bottom = y;
  while (bottom + 1 < rows.length && rows.wrapped(bottom + 1)) bottom++;
  return bottom;
}

/**
 * The whole logical line `y` was drawn across, as one string, with the place each of its characters
 * came from recorded beside it.
 *
 * The positions are collected rather than computed, one entry per UTF-16 code unit so they line up
 * with what a regular expression reports as an index. Computing them would mean assuming a cell is a
 * column and a character is a cell, and neither holds: a wide character covers two columns, a
 * combining mark rides along in one cell, and a blank cell has no character in it at all.
 */
function joined(rows: Rows, y: number): { text: string; at: At[] } {
  const top = firstRow(rows, y);
  const bottom = lastRow(rows, y);
  let text = "";
  const at: At[] = [];
  for (let row = top; row <= bottom; row++) {
    const cells = rows.cells(row);
    for (let x = 0; x < cells.length; x++) {
      const cell = cells[x];
      if (cell.width === 0) continue; // the column a wide character spilled into holds nothing of its own
      // A cell nothing was drawn in still occupies its column, and the space stands for it: a ref is
      // bounded by what is beside it, and closing a gap would join two words that were never one.
      const chars = cell.chars === "" ? " " : cell.chars;
      text += chars;
      for (let i = 0; i < chars.length; i++) at.push({ x: x + 1, y: row + 1 });
    }
  }
  return { text, at };
}

/**
 * The refs reachable on row `y` (0-based) — every one drawn on it, including the ones that begin on
 * a row above or end on a row below, because a ref across the fold is on both.
 *
 * Refs belonging to the same logical line but touching neither end of this row are left out: they are
 * found again when their own row is asked for, and returning them here would offer a link at a
 * position that does not carry it.
 */
export function refsOnRow(rows: Rows, y: number): TerminalRef[] {
  if (y < 0 || y >= rows.length) return [];
  const { text, at } = joined(rows, y);
  // A fresh matcher each time: the shared pattern is global, and stepping its `lastIndex` here would
  // leave it wherever this scan stopped for whoever reads next.
  const re = new RegExp(REF_RE.source, REF_RE.flags);
  const found: TerminalRef[] = [];
  for (let m = re.exec(text); m !== null; m = re.exec(text)) {
    const parsed = parseRef(m[0]);
    const start = at[m.index];
    const end = at[m.index + m[0].length - 1];
    if (!parsed || !start || !end) continue;
    if (start.y > y + 1 || end.y < y + 1) continue;
    found.push({ text: m[0], space: parsed.space, num: parsed.num, range: { start, end } });
  }
  return found;
}

/**
 * The record an `amenbo://` address names, or `null` for anything else.
 *
 * This is the other way a ref becomes clickable: amenbo's own output wraps one in OSC 8, the escape
 * that says where text points, so the pane is told the destination instead of having to find it
 * (`AMB-T-3595`). Wrapping and elision cannot touch it — the address travels beside the characters
 * rather than in them — which is why both ways exist rather than one replacing the other.
 *
 * Everything else is `null`, deliberately. A pane draws output this app did not write, and an OSC 8
 * from some other program names a destination nobody here vouched for: nothing is opened, navigated
 * to, or handed on. The addresses this answers for reach one function that selects a record by
 * number, and there is no path from here to a browser.
 */
export function refFromUrl(url: string): { space: RefSpace; num: number } | null {
  const m = /^amenbo:\/\/(task|decision)\/(\d+)$/.exec(url);
  if (!m) return null;
  const num = Number(m[2]);
  return Number.isSafeInteger(num) ? { space: m[1] as RefSpace, num } : null;
}
