// @vitest-environment jsdom
import { Terminal } from "@xterm/xterm";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { fitter } from "./growing";

// These open a real xterm.js terminal, because what they stand guard over is how xterm.js's own
// resize treats the scrollback: a release that brings the lines back by itself fails the first one,
// which is the moment to look whether `./growing` can go.

let term: Terminal | undefined;

beforeAll(() => {
  // jsdom has no `matchMedia`, and xterm.js watches the screen's pixel ratio with one.
  if (typeof window.matchMedia !== "function") {
    window.matchMedia = (query: string) =>
      ({
        matches: false,
        media: query,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        addListener: () => {},
        removeListener: () => {},
        dispatchEvent: () => false,
      }) as MediaQueryList;
  }
});

afterEach(() => {
  term?.dispose();
  term = undefined;
  document.body.replaceChildren();
});

const written = (t: Terminal, data: string) => new Promise<void>((done) => t.write(data, done));

/** A terminal 10 rows tall with `lines` lines written to it, numbered from 0. */
async function opened(lines: number) {
  const host = document.createElement("div");
  document.body.append(host);
  term = new Terminal({ cols: 20, rows: 10 });
  term.open(host);
  const text = Array.from({ length: lines }, (_, i) => `line ${i}`).join("\r\n");
  await written(term, text);
  return term;
}

/** A box that fits the terminal to `rows` rows, standing in for the fit addon jsdom cannot measure. */
function box(t: Terminal) {
  const size = { rows: t.rows };
  const fitting = fitter(t, { rows: () => size.rows, fit: () => t.resize(t.cols, size.rows) });
  return { size, fitting };
}

/** The text on a row of the screen, counted from its top. */
function rowText(t: Terminal, y: number): string {
  const buffer = t.buffer.active;
  return buffer.getLine(buffer.baseY + y)?.translateToString(true) ?? "";
}

describe("fitter", () => {
  it("is needed: xterm.js adds blank rows when the cursor has rows under it", async () => {
    const t = await opened(30);
    // The cursor on the third row from the bottom, as a program with a status line under its input
    // leaves it.
    await written(t, "\x1b[8;3H");
    t.resize(20, 15);
    expect(t.buffer.active.baseY).toBe(20);
    expect(rowText(t, 0)).toBe("line 20");
  });

  it("brings the scrollback back when the cursor has rows under it, and puts the cursor back", async () => {
    const t = await opened(30);
    await written(t, "\x1b[8;3H");
    const { size, fitting } = box(t);
    size.rows = 15;
    await fitting.fit();
    expect(t.rows).toBe(15);
    expect(t.buffer.active.baseY).toBe(15);
    expect(rowText(t, 0)).toBe("line 15");
    // The cursor is on the line it was on, five rows lower now that five came back above it.
    expect(t.buffer.active.cursorY).toBe(12);
    expect(t.buffer.active.cursorX).toBe(2);
    expect(rowText(t, 12)).toBe("line 27");
  });

  it("holds output written while it fits, and writes it where the cursor came back to", async () => {
    const t = await opened(30);
    await written(t, "\x1b[8;3H");
    const { size, fitting } = box(t);
    size.rows = 15;
    const fitted = fitting.fit();
    fitting.write("X");
    await fitted;
    await written(t, "");
    expect(rowText(t, 12)).toBe("liXe 27");
  });

  it("brings back only what the scrollback has", async () => {
    const t = await opened(13);
    await written(t, "\x1b[8;3H");
    const { size, fitting } = box(t);
    size.rows = 15;
    await fitting.fit();
    expect(t.buffer.active.baseY).toBe(0);
    expect(rowText(t, 0)).toBe("line 0");
    expect(rowText(t, t.buffer.active.cursorY)).toBe("line 10");
  });

  it("leaves a terminal with no scrollback as the resize leaves it", async () => {
    const t = await opened(5);
    await written(t, "\x1b[2;3H");
    const { size, fitting } = box(t);
    size.rows = 15;
    await fitting.fit();
    expect(t.rows).toBe(15);
    expect(t.buffer.active.cursorY).toBe(1);
    expect(t.buffer.active.cursorX).toBe(2);
  });

  it("is needed: xterm.js drops the rows under the cursor when made shorter", async () => {
    const t = await opened(30);
    await written(t, "\x1b[8;3H");
    t.resize(20, 6);
    // Two of the four rows taken away were the ones under the cursor, so only two went up.
    expect(t.buffer.active.baseY).toBe(22);
    expect(t.buffer.active.length).toBe(28);
  });

  it("sends the rows taken away up into the scrollback whole, and puts the cursor back", async () => {
    const t = await opened(30);
    await written(t, "\x1b[8;3H");
    const { size, fitting } = box(t);
    size.rows = 6;
    await fitting.fit();
    expect(t.rows).toBe(6);
    expect(t.buffer.active.baseY).toBe(24);
    expect(rowText(t, 0)).toBe("line 24");
    expect(rowText(t, 5)).toBe("line 29");
    expect(t.buffer.active.cursorY).toBe(3);
    expect(t.buffer.active.cursorX).toBe(2);
    expect(rowText(t, 3)).toBe("line 27");
  });

  it("drops rows under the cursor rather than send the cursor's own row up off the screen", async () => {
    const t = await opened(30);
    await written(t, "\x1b[2;3H");
    const { size, fitting } = box(t);
    size.rows = 6;
    await fitting.fit();
    expect(t.buffer.active.baseY).toBe(21);
    expect(t.buffer.active.cursorY).toBe(0);
    expect(rowText(t, 0)).toBe("line 21");
  });

  it("leaves a terminal with nothing under the cursor, made shorter, as the resize leaves it", async () => {
    const t = await opened(5);
    const { size, fitting } = box(t);
    size.rows = 6;
    await fitting.fit();
    expect(t.rows).toBe(6);
    expect(t.buffer.active.baseY).toBe(0);
    expect(rowText(t, 0)).toBe("line 0");
    expect(t.buffer.active.cursorY).toBe(4);
  });

  it("neither loses nor repeats a line when made shorter and then taller again", async () => {
    const t = await opened(30);
    await written(t, "\x1b[8;3H");
    const { size, fitting } = box(t);
    for (let i = 0; i < 3; i++) {
      size.rows = 6;
      await fitting.fit();
      size.rows = 10;
      await fitting.fit();
    }
    const buffer = t.buffer.active;
    expect(buffer.length).toBe(30);
    for (let i = 0; i < 30; i++) {
      expect(buffer.getLine(i)?.translateToString(true)).toBe(`line ${i}`);
    }
    expect(buffer.baseY).toBe(20);
    expect(buffer.cursorY).toBe(7);
    expect(rowText(t, 7)).toBe("line 27");
  });

  it("leaves the alternate screen as the resize leaves it", async () => {
    const t = await opened(30);
    await written(t, "\x1b[?1049h\x1b[8;3H");
    const { size, fitting } = box(t);
    size.rows = 15;
    await fitting.fit();
    expect(t.buffer.active.type).toBe("alternate");
    expect(t.buffer.active.cursorY).toBe(7);
    expect(t.buffer.active.cursorX).toBe(2);
  });
});
