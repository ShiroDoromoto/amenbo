// **A terminal made taller brings its scrollback back down onto the screen** — a stand-in for what
// xterm.js 6.0.0 does only when the cursor is on the bottom row.
//
// Given more rows, xterm.js's `Buffer.resize` pulls lines back out of the scrollback while the cursor
// sits on the last row, and otherwise adds blank rows under what is drawn. A program like Claude Code
// keeps its cursor on its input line with a status line or two below it, so it gets the blank rows.
// It then answers the new size by redrawing the bottom of its conversation from the top of the
// screen, and the lines that were already in the scrollback are drawn a second time right under it.
//
// So before a resize that adds rows, the cursor goes to the bottom row, and after it the cursor goes
// back to the row it was on — moved down by however many lines came back out of the scrollback, which
// is where that row's content now is.
//
// **A terminal made shorter has to send just as many lines up, or the next one made taller loses
// lines.** Given fewer rows, `Buffer.resize` drops the rows under the cursor first and sends only the
// rest up into the scrollback. Made taller again, the terminal brings back a whole row's worth for
// every row it gains, more than went up, and the program's redraw writes over the difference. So a
// resize that takes rows away moves the cursor to the bottom row too, as long as anything is drawn
// under it, and the rows go up into the scrollback whole. The rows under the cursor are only dropped
// where keeping them would send the cursor's own row up off the screen.

import type { Terminal } from "@xterm/xterm";

/** As much of a terminal as this reads and writes. */
export type Growable = Pick<Terminal, "rows" | "buffer" | "modes" | "write">;

/** What fits the terminal to its box: the rows that will be asked for, and the fitting itself. */
export interface Fit {
  /** How many rows fitting would give the terminal, or `undefined` where it cannot tell yet. */
  rows(): number | undefined;
  fit(): void;
}

/**
 * A terminal's fitting, with the output written to it going through the same place.
 *
 * **The output has to wait while the cursor is away.** Moving the cursor and moving it back are two
 * writes with the fitting between them, and the fitting can only happen once the first has been
 * read. Output written in that gap would be read with the cursor on the bottom row instead of where
 * the program left it, so it is held and written after the cursor is back.
 */
export interface Fitter {
  /** Fit the terminal to its box. Settles once the fitting, and the cursor's return, are done. */
  fit(): Promise<void>;
  /** Write a program's output, behind any fitting that is in progress. */
  write(data: string | Uint8Array): void;
}

export function fitter(term: Growable, sized: Fit): Fitter {
  const held: (string | Uint8Array)[] = [];
  let busy: Promise<void> | null = null;
  let again = false;

  const written = (data: string) => new Promise<void>((done) => term.write(data, done));

  async function once(): Promise<void> {
    const rows = sized.rows();
    if (rows === undefined || rows === term.rows) {
      sized.fit();
      return;
    }
    // What the cursor is on is read once everything written before has been, not as it stood when
    // the box changed.
    await written("");
    const buffer = term.buffer.active;
    // The alternate screen has no scrollback at all. Under origin mode a cursor position counts from
    // the scroll region's top, so the rows written below would mean other rows.
    if (buffer.type !== "normal" || term.modes.originMode) {
      sized.fit();
      return;
    }
    const { cursorX, cursorY } = buffer;
    const bottom = term.rows - 1;
    // The row the cursor is put on for the resize. Taller: the bottom row, where there is anything to
    // bring back. Shorter: the bottom row, where anything is drawn under the cursor, but never so low
    // that the cursor's own row would go up off the screen.
    const row =
      rows > term.rows
        ? buffer.baseY > 0
          ? bottom
          : cursorY
        : drawnUnder(cursorY)
          ? Math.min(bottom, cursorY + rows - 1)
          : cursorY;
    if (row === cursorY) {
      sized.fit();
      return;
    }
    await written(`\x1b[${row + 1};1H`);
    sized.fit();
    // The row put on the bottom kept its content, and the cursor's row is as far above it as before.
    const back = term.buffer.active.cursorY - (row - cursorY);
    await written(`\x1b[${back + 1};${cursorX + 1}H`);
  }

  /** Whether anything is drawn on the screen's rows under row `y`. */
  function drawnUnder(y: number): boolean {
    const buffer = term.buffer.active;
    for (let below = y + 1; below < term.rows; below++) {
      if (buffer.getLine(buffer.baseY + below)?.translateToString(true)) return true;
    }
    return false;
  }

  return {
    fit() {
      // A change that comes in while one is being fitted is fitted after it, to the size the box has
      // then.
      if (busy) {
        again = true;
        return busy;
      }
      busy = (async () => {
        try {
          do {
            again = false;
            await once();
          } while (again);
        } finally {
          busy = null;
          for (const data of held.splice(0)) term.write(data);
        }
      })();
      return busy;
    },
    write(data) {
      if (busy) held.push(data);
      else term.write(data);
    },
  };
}
